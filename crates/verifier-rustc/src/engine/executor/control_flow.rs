use rustc_middle::mir::{
    BasicBlock, Location, NonDivergingIntrinsic, RETURN_PLACE, StatementKind, TerminatorKind,
};
use smallvec::SmallVec;
use verifier_core::Term;

use crate::{
    contracts::{Clause, Source, instantiate},
    engine::{
        loop_analysis::LoopInfo,
        obligation::{ExecutionError, LocationExt, Obligation, ObligationKind},
    },
};

use super::{Evaluate, Execute, Executor, State, conjoin, entry_loc};

impl<'a, 'tcx> Executor<'a, 'tcx> {
    fn instantiate_in_state(
        &mut self,
        clause: &Clause,
        state: &State,
    ) -> Result<Term, ExecutionError> {
        instantiate(self.context, clause, |source| match source {
            Source::Local(local) => state.store[local],
            Source::Result => None,
        })
        .map_err(|message| state.location.error(format!("invalid loop invariant: {message}")))
    }

    fn instantiate_postcondition(
        &mut self,
        clause: &Clause,
        value: Term,
        location: Location,
    ) -> Result<Term, ExecutionError> {
        instantiate(self.context, clause, |source| match source {
            Source::Local(local) => self.entry[local],
            Source::Result => Some(value),
        })
        .map_err(|message| location.error(format!("invalid postcondition: {message}")))
    }

    fn transition(
        &mut self,
        mut state: State,
        target: BasicBlock,
        pending: &mut Vec<State>,
    ) -> Result<(), ExecutionError> {
        let source = state.location.block;

        if let Some(info) = self.loops.backedge(source, target).cloned() {
            return self.preserve_loop(state, &info);
        }

        if let Some(info) = self.loops.header(target).cloned()
            && self.loops.is_external_entry(source, &info)
        {
            return self.enter_loop(state, &info, pending);
        }

        state.location = entry_loc(target);
        pending.push(state);
        Ok(())
    }

    fn enter_loop(
        &mut self,
        mut state: State,
        info: &LoopInfo,
        pending: &mut Vec<State>,
    ) -> Result<(), ExecutionError> {
        if info.invariants.is_empty() {
            return Err(state.location.error(format!(
                "loop with header {:?} requires at least one `#[verifier::invariant(...)]`",
                info.header
            )));
        }

        let premise = conjoin(self.context, &state.facts);
        for clause in &info.invariants {
            let invariant = self.instantiate_in_state(clause, &state)?;
            let condition = self.context.implies(premise, invariant);
            self.obligations.push(Obligation {
                kind: ObligationKind::LoopInvariantInitialization,
                location: state.location,
                span: Some(clause.span),
                condition,
            });
        }

        for local in &info.modified_locals {
            if state.store[*local].is_none() {
                continue;
            }
            let ty = self.body.local_decls[*local].ty;
            let Some(sort) = self.sort_for_ty(ty) else { continue };
            let name = format!(
                "loop_{}_local_{}_{}",
                info.header.index(),
                local.index(),
                self.fresh_counter
            );
            self.fresh_counter += 1;
            let symbol = self.context.symbol(&name, sort);
            let value = self.context.sym(symbol);
            state.store[*local] = Some(value);
            self.add_integer_range_facts(ty, value, &mut state.facts);
        }

        for clause in &info.invariants {
            let invariant = self.instantiate_in_state(clause, &state)?;
            state.facts.push(invariant);
        }
        state.location = entry_loc(info.header);
        pending.push(state);
        Ok(())
    }

    fn preserve_loop(&mut self, state: State, info: &LoopInfo) -> Result<(), ExecutionError> {
        if info.invariants.is_empty() {
            return Err(state.location.error(format!(
                "loop with header {:?} requires at least one `#[verifier::invariant(...)]`",
                info.header
            )));
        }
        let premise = conjoin(self.context, &state.facts);
        for clause in &info.invariants {
            let invariant = self.instantiate_in_state(clause, &state)?;
            let condition = self.context.implies(premise, invariant);
            self.obligations.push(Obligation {
                kind: ObligationKind::LoopInvariantPreservation,
                location: state.location,
                span: Some(clause.span),
                condition,
            });
        }
        Ok(())
    }
}

impl<'a, 'tcx, 'mir> Execute<&'mir StatementKind<'tcx>> for Executor<'a, 'tcx> {
    fn execute(
        &mut self,
        mut state: State,
        statement: &'mir StatementKind<'tcx>,
        pending: &mut Vec<State>,
    ) -> Result<(), ExecutionError> {
        let location = state.location;

        use NonDivergingIntrinsic as Intrinsic;
        use StatementKind as Kind;

        match statement {
            Kind::Assign(deref!((place, rvalue))) => {
                let term = self.evaluate(&state, rvalue)?;
                self.write_place(&mut state, *place, term)?;
            }
            Kind::SetDiscriminant { place: deref!(_place), variant_index: _ } => {
                todo!();
            }
            Kind::StorageLive(local) | Kind::StorageDead(local) => {
                state.store[*local] = None;
            }
            Kind::Intrinsic(deref!(Intrinsic::Assume(operand))) => {
                let term = self.evaluate(&state, operand)?;
                state.facts.push(term);
            }
            Kind::Intrinsic(deref!(other)) => {
                return Err(location.error(format!("intrinsic `{other:?}`")));
            }
            Kind::Nop
            | Kind::FakeRead(_)
            | Kind::PlaceMention(_)
            | Kind::AscribeUserType(_, _)
            | Kind::Coverage(_)
            | Kind::ConstEvalCounter
            | Kind::BackwardIncompatibleDropHint { .. } => {}
        };

        state.location.statement_index += 1;
        pending.push(state);
        Ok(())
    }
}

impl<'a, 'tcx, 'mir> Execute<&'mir TerminatorKind<'tcx>> for Executor<'a, 'tcx> {
    fn execute(
        &mut self,
        mut state: State,
        terminator: &'mir TerminatorKind<'tcx>,
        pending: &mut Vec<State>,
    ) -> Result<(), ExecutionError> {
        match terminator {
            TerminatorKind::Goto { target } => {
                self.transition(state, *target, pending)?;
            }
            TerminatorKind::SwitchInt { discr, targets } => {
                let discr_term = self.evaluate(&state, discr)?;
                let discr_ty = discr.ty(self.body, self.tcx);
                let mut excluded = SmallVec::<[Term; 4]>::new();

                for (bits, target) in targets.iter() {
                    let condition = self.evaluate(&state, (discr_term, discr_ty, bits))?;
                    excluded.push(condition);

                    let mut branch = state.clone();
                    branch.facts.push(condition);
                    self.transition(branch, target, pending)?;
                }

                for equality in excluded {
                    let inequality = self.context.not(equality);
                    state.facts.push(inequality);
                }
                self.transition(state, targets.otherwise(), pending)?;
            }
            TerminatorKind::Assert { cond, expected, target, .. } => {
                let assertion = self.evaluate(&state, (cond, *expected))?;
                let current_fact = conjoin(self.context, &state.facts);
                let implication = self.context.implies(current_fact, assertion);
                self.obligations.push(Obligation {
                    kind: ObligationKind::RuntimeAssertion,
                    location: state.location,
                    span: Some(
                        self.body.basic_blocks[state.location.block].terminator().source_info.span,
                    ),
                    condition: implication,
                });

                state.facts.push(assertion);
                self.transition(state, *target, pending)?;
            }
            TerminatorKind::Return => {
                let fact = conjoin(self.context, &state.facts);
                let value = state.store[RETURN_PLACE]
                    .ok_or_else(|| state.location.error("return place is uninitialized"))?;
                for clause in &self.spec.ensures {
                    let postcondition =
                        self.instantiate_postcondition(clause, value, state.location)?;
                    let condition = self.context.implies(fact, postcondition);
                    self.obligations.push(Obligation {
                        kind: ObligationKind::Postcondition,
                        location: state.location,
                        span: Some(clause.span),
                        condition,
                    });
                }
            }
            TerminatorKind::Unreachable => {}
            TerminatorKind::FalseEdge { real_target, .. }
            | TerminatorKind::FalseUnwind { real_target, .. } => {
                self.transition(state, *real_target, pending)?;
            }
            TerminatorKind::UnwindResume => todo!(),
            TerminatorKind::UnwindTerminate(..) => todo!(),
            TerminatorKind::Drop { .. } => todo!(),
            TerminatorKind::Call { .. } => todo!(),
            TerminatorKind::TailCall { .. } => todo!(),
            TerminatorKind::Yield { .. } => todo!(),
            TerminatorKind::CoroutineDrop => todo!(),
            TerminatorKind::InlineAsm { .. } => todo!(),
        }

        Ok(())
    }
}
