use rustc_index::IndexVec;
use rustc_middle::{
    mir::{BasicBlock, Body, Local, Location, START_BLOCK, VarDebugInfoContents},
    ty::{TyCtxt, TypingEnv},
};
use smallvec::SmallVec;
use verifier_core::{Context, Term};

use crate::contracts::{FunctionSpec, Source, instantiate};

use super::{
    loop_analysis::LoopAnalysis,
    obligation::{ExecutionError, Limits, LocationExt, Obligation},
};

mod control_flow;
mod eval;
mod numeric;

#[derive(Debug, Clone)]
struct State {
    location: Location,
    store: IndexVec<Local, Option<Term>>,
    facts: SmallVec<[Term; 8]>,
    steps: u32,
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
    context: &'a mut Context,
    tcx: TyCtxt<'tcx>,
    body: &'a Body<'tcx>,
    typing_env: TypingEnv<'tcx>,
    limits: Limits,
    spec: &'a FunctionSpec,
    loops: LoopAnalysis,
    entry: IndexVec<Local, Option<Term>>,
    fresh_counter: u32,
    obligations: Vec<Obligation>,
}

/// Executes a rustc MIR control-flow graph using symbolic values and generates
/// the obligations induced by its source-level contracts.
pub(crate) fn execute_with_spec<'tcx>(
    context: &mut Context,
    tcx: TyCtxt<'tcx>,
    body: &Body<'tcx>,
    spec: &FunctionSpec,
) -> Result<Vec<Obligation>, ExecutionError> {
    let limits = Limits::default();
    let location = entry_loc(START_BLOCK);
    let loops = LoopAnalysis::new(body, spec).map_err(|message| location.error(message))?;
    Executor {
        context,
        tcx,
        body,
        typing_env: TypingEnv::post_analysis(tcx, body.source.def_id()),
        limits,
        spec,
        loops,
        entry: body.local_decls.iter().map(|_| None).collect(),
        fresh_counter: 0,
        obligations: Vec::new(),
    }
    .run()
}

fn conjoin<'a>(context: &mut Context, terms: impl IntoIterator<Item = &'a Term>) -> Term {
    let mut terms = terms.into_iter().copied();

    if let Some(first) = terms.next() {
        terms.fold(first, |lhs, rhs| context.and(lhs, rhs))
    } else {
        context.bool_lit(true)
    }
}

impl<'a, 'tcx> Executor<'a, 'tcx> {
    fn run(mut self) -> Result<Vec<Obligation>, ExecutionError> {
        let initial = self.initial_state()?;
        let mut pending = vec![initial];
        let mut explored = 0;

        while let Some(mut state) = pending.pop() {
            let location = state.location;
            let data = &self.body.basic_blocks[location.block];

            if location.statement_index == 0 {
                explored += 1;
                if explored > self.limits.max_states {
                    return Err(location.error(format!(
                        "execution exceeded the {}-state exploration limit",
                        self.limits.max_states
                    )));
                }
                if state.steps >= self.limits.max_steps {
                    return Err(location.error(format!(
                        "path exceeded the {}-step exploration limit (the body may contain a loop)",
                        self.limits.max_steps
                    )));
                }
                state.steps += 1;
            }

            if let Some(statement) = data.statements.get(location.statement_index) {
                self.execute(state, &statement.kind, &mut pending)?;
            } else if location.statement_index == data.statements.len() {
                self.execute(state, &data.terminator().kind, &mut pending)?;
            } else {
                return Err(location.error("program counter is outside its basic block"));
            }

            if pending.len() > self.limits.max_pending as usize {
                return Err(location.error(format!(
                    "more than {} symbolic paths are pending",
                    self.limits.max_pending
                )));
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
            let Some(sort) = self.sort_for_ty(ty) else {
                return Err(location.error(format!("argument {index} has unsupported type `{ty}`")));
            };
            let name = self.argument_name(local, index);
            let symbol = self.context.symbol(&name, sort);
            let term = self.context.sym(symbol);
            self.entry[local] = Some(term);
            store[local] = Some(term);
            self.add_integer_range_facts(ty, term, &mut facts);
        }

        for clause in &self.spec.requires {
            let term = instantiate(self.context, clause, |source| match source {
                Source::Local(local) => store[local],
                Source::Result => None,
            })
            .map_err(|message| location.error(format!("invalid precondition: {message}")))?;
            facts.push(term);
        }

        Ok(State { location: entry_loc(START_BLOCK), store, facts, steps: 0 })
    }

    fn argument_name(&self, local: Local, index: usize) -> String {
        self.body
            .var_debug_info
            .iter()
            .find_map(|info| {
                if info.argument_index != Some(index as u16) {
                    return None;
                }
                match info.value {
                    VarDebugInfoContents::Place(place)
                        if place.local == local && place.projection.is_empty() =>
                    {
                        Some(info.name.as_str().to_owned())
                    }
                    VarDebugInfoContents::Place(..) | VarDebugInfoContents::Const(..) => None,
                }
            })
            .unwrap_or_else(|| format!("arg{index}"))
    }
}

#[cfg(test)]
mod tests {
    use verifier_core::{Context, DefStore};

    #[test]
    fn assertion_vc_is_guarded_by_its_path_condition() {
        let mut context = Context::default();
        let bool_sort = context.bool_sort();
        let path_symbol = context.symbol("path", bool_sort);
        let assertion_symbol = context.symbol("assertion", bool_sort);
        let path = context.sym(path_symbol);
        let assertion = context.sym(assertion_symbol);
        let vc = context.implies(path, assertion);

        assert_eq!(
            context.get(vc),
            verifier_core::TermDef::Binary {
                op: verifier_core::Op::Implies,
                lhs: path,
                rhs: assertion,
            }
        );
    }
}
