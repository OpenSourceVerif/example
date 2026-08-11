use rustc_index::IndexVec;
use rustc_middle::{
    mir::{BasicBlock, Body, Local, Location, START_BLOCK, VarDebugInfoContents},
    ty::{Ty, TyCtxt, TypingEnv},
};
use rustc_span::Symbol;
use smallvec::SmallVec;
use verifier_core::{Context, Term, contract::instantiate};

use crate::{
    spec::{Slot, Spec},
    types::{RustcTy, integer_bounds, integer_layout},
};

use super::{
    loop_analysis::LoopAnalysis,
    obligation::{ExecutionError, LocationExt, Obligation},
};

mod control_flow;
mod eval;

#[derive(Debug, Clone)]
struct State {
    /// our program counter.
    location: Location,
    /// symbolic state.
    store: IndexVec<Local, Option<Term>>,
    /// conditions on current path.
    facts: SmallVec<[Term; 8]>,
}

/// Evaluates an argument without changing the symbolic state's control flow.
trait Evaluate<What> {
    type Output;

    fn evaluate(&mut self, state: &State, what: What) -> Result<Self::Output, ExecutionError>;
}

/// Executes an argument, consuming one state and producing zero or more successors.
trait Execute<What> {
    fn execute(
        &mut self,
        state: State,
        what: What,
        pending: &mut Vec<State>,
    ) -> Result<(), ExecutionError>;
}

fn entry_loc(block: BasicBlock) -> Location {
    Location { block, statement_index: 0 }
}

struct Executor<'a, 'tcx> {
    cx: &'a mut Context,
    tcx: TyCtxt<'tcx>,
    body: &'a Body<'tcx>,
    typing_env: TypingEnv<'tcx>,
    spec: &'a Spec,
    loops: LoopAnalysis,
    entry: IndexVec<Local, Option<Term>>,
    fresh_counter: u32,
    obligations: Vec<Obligation>,
}

/// Executes a rustc MIR control-flow graph using symbolic values and generates
/// the obligations induced by its source-level contracts.
pub(crate) fn execute<'tcx>(
    cx: &mut Context,
    tcx: TyCtxt<'tcx>,
    body: &Body<'tcx>,
    spec: &Spec,
) -> Result<Vec<Obligation>, ExecutionError> {
    let location = entry_loc(START_BLOCK);
    let loops = LoopAnalysis::new(body, spec).map_err(|error| location.error(error.to_string()))?;
    Executor {
        cx,
        tcx,
        body,
        typing_env: TypingEnv::post_analysis(tcx, body.source.def_id()),
        spec,
        loops,
        entry: body.local_decls.iter().map(|_| None).collect(),
        fresh_counter: 0,
        obligations: Vec::new(),
    }
    .run()
}

fn conjoin<'a>(cx: &mut Context, terms: impl IntoIterator<Item = &'a Term>) -> Term {
    let mut terms = terms.into_iter().copied();

    if let Some(first) = terms.next() {
        terms.fold(first, |lhs, rhs| cx.and(lhs, rhs))
    } else {
        cx.bool_lit(true)
    }
}

impl<'a, 'tcx> Executor<'a, 'tcx> {
    fn add_integer_range_facts(
        &mut self,
        ty: Ty<'tcx>,
        term: Term,
        facts: &mut SmallVec<[Term; 8]>,
    ) {
        let Some(bounds) = integer_layout(self.tcx, ty).and_then(integer_bounds) else { return };
        let minimum = self.cx.int_lit(bounds.0);
        let maximum = self.cx.int_lit(bounds.1);
        facts.push(self.cx.ge(term, minimum));
        facts.push(self.cx.le(term, maximum));
    }

    fn run(mut self) -> Result<Vec<Obligation>, ExecutionError> {
        let initial = self.initial_state()?;
        let mut pending = vec![initial];

        while let Some(state) = pending.pop() {
            let location = state.location;
            let data = &self.body.basic_blocks[location.block];

            if let Some(statement) = data.statements.get(location.statement_index) {
                self.execute(state, &statement.kind, &mut pending)?;
            } else if location.statement_index == data.statements.len() {
                self.execute(state, &data.terminator().kind, &mut pending)?;
            } else {
                return Err(location.error("program counter is outside its basic block"));
            }
        }

        Ok(self.obligations)
    }

    fn initial_state(&mut self) -> Result<State, ExecutionError> {
        let location = entry_loc(START_BLOCK);
        let mut store: IndexVec<Local, Option<Term>> =
            self.body.local_decls.iter().map(|_| None).collect();
        let mut facts = SmallVec::new();

        for index in 1..=self.body.arg_count {
            let local = Local::from_usize(index);
            let ty = self.body.local_decls[local].ty;
            let Some(sort) = self.cx.sort(self.tcx, ty) else {
                return Err(location.error(format!("argument {index} has unsupported type `{ty}`")));
            };
            let symbol = match self.argument_name(local, index) {
                Some(name) => self.cx.symbol(name.as_str(), sort),
                None => self.cx.symbol(&format!("arg{index}"), sort),
            };
            let term = self.cx.sym(symbol);
            self.entry[local] = Some(term);
            store[local] = Some(term);
            self.add_integer_range_facts(ty, term, &mut facts);
        }

        for clause in &self.spec.requires {
            let term = instantiate(self.cx, &clause.node, |slot| match slot {
                Slot::Local(local) => store[local],
                Slot::Result => None,
            })
            .map_err(|message| location.error(format!("invalid precondition: {message}")))?;
            facts.push(term);
        }

        Ok(State { location: entry_loc(START_BLOCK), store, facts })
    }

    fn argument_name(&self, local: Local, index: usize) -> Option<Symbol> {
        self.body.var_debug_info.iter().find_map(|info| {
            if info.argument_index != Some(index as u16) {
                return None;
            }
            match info.value {
                VarDebugInfoContents::Place(place)
                    if place.local == local && place.projection.is_empty() =>
                {
                    Some(info.name)
                }
                VarDebugInfoContents::Place(..) | VarDebugInfoContents::Const(..) => None,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use verifier_core::{Context, DefStore};

    #[test]
    fn assertion_vc_is_guarded_by_its_path_condition() {
        let mut cx = Context::default();
        let bool_sort = cx.bool_sort();
        let path_symbol = cx.symbol("path", bool_sort);
        let assertion_symbol = cx.symbol("assertion", bool_sort);
        let path = cx.sym(path_symbol);
        let assertion = cx.sym(assertion_symbol);
        let vc = cx.implies(path, assertion);

        assert_eq!(
            cx.get(vc).kind,
            verifier_core::TermKind::Binary {
                op: verifier_core::Op::Implies,
                lhs: path,
                rhs: assertion,
            }
        );
    }
}
