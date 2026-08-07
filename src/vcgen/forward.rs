use std::{error::Error, fmt};

use rustc_middle::{
    mir::{
        self, AggregateKind, BasicBlock, BasicBlockData, BinOp, Body, Local, Location, NonDivergingIntrinsic, Operand, Place, ProjectionElem, RETURN_PLACE, Rvalue, START_BLOCK, Statement, StatementKind, Terminator, TerminatorKind, UnOp, VarDebugInfoContents,
    }, ty::{IntTy, Ty, TyCtxt, TyKind, TypingEnv, UintTy},
};
use smallvec::SmallVec;

use crate::{Context, Op, Sort, Sym, Term, Uop};

/// Limits which keep forward exploration finite in the presence of loops and
/// exponential path growth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionLimits {
    /// Maximum number of basic blocks visited along any single path.
    pub max_steps: u32,
    /// Maximum worklist size at any instant.
    pub max_pending: u32,
    /// Maximum total number of symbolic states removed from the worklist.
    pub max_states: u32,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self { max_steps: 10_000, max_pending: 1_024, max_states: 100_000 }
    }
}

/// A successfully returned MIR path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReturnPath {
    /// path condition
    pub fact: Term,
    pub value: Term,
}

/// The entry-state symbol assigned to one MIR argument local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolicArgument {
    pub local: Local,
    pub symbol: Sym,
    pub value: Term,
}

/// The finite symbolic execution tree produced for one MIR body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionResult {
    pub arguments: Vec<SymbolicArgument>,
    pub return_paths: Vec<ReturnPath>,
    pub assertions: Vec<Term>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirExecutionError {
    pub location: Location,
    pub message: String,
}

impl fmt::Display for MirExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "MIR symbolic execution failed at {:?}[{:?}]: {}",
            self.location.block, self.location.statement_index, self.message
        )
    }
}

impl Error for MirExecutionError {}


#[derive(Debug, Clone)]
struct State {
    bb: BasicBlock,
    store: Vec<Option<Term>>,
    facts: SmallVec<[Term; 8]>,
    steps: u32,
}

index_vec::define_index_type! {
    pub struct Stmt = u32;
}

/// purely adapter trait. need better naming or abstraction.
trait LocationAdapter {
    fn error(self, message: impl Into<String>) -> MirExecutionError;
}

impl LocationAdapter for Location {
    fn error(self, message: impl Into<String>) -> MirExecutionError {
        MirExecutionError {
            location: self,
            message: message.into(),
        }
    }
}

trait BasicBlockDataAdapter<'tcx> {
    fn entry_loc(&self, bb: BasicBlock) -> Location {
        Location { block: bb, statement_index: 0 }
    }

    fn statements_loc<'a>(&'a self, bb: BasicBlock) -> impl Iterator<Item = (Location, &'a Statement<'tcx>)>
    where
        'tcx: 'a;

    fn terminator_loc<'a>(&'a self, bb: BasicBlock) -> (Location, &'a Terminator<'tcx>)
    where
        'tcx: 'a;
}

impl<'tcx> BasicBlockDataAdapter<'tcx> for BasicBlockData<'tcx> {
    fn statements_loc<'a>(
        self: &'a BasicBlockData<'tcx>,
        bb: BasicBlock,
    ) -> impl Iterator<Item = (Location, &'a Statement<'tcx>)>
    where
        'tcx: 'a,
    {
        self.statements
            .iter()
            .enumerate()
            .map(move |(idx, statement)| (Location { block: bb, statement_index: idx }, statement))
    }

    fn terminator_loc<'a>(&'a self, bb: BasicBlock) -> (Location, &'a Terminator<'tcx>)
    where
        'tcx: 'a {
        (Location {block:bb, statement_index = self.statements.len()}, self.terminator())
    }
}

struct Executor<'a, 'tcx> {
    context: &'a mut Context,
    tcx: TyCtxt<'tcx>,
    body: &'a Body<'tcx>,
    typing_env: TypingEnv<'tcx>,
    limits: ExecutionLimits,
    result: ExecutionResult,
}

impl Context {
    /// Executes an actual rustc MIR control-flow graph using symbolic values.
    ///
    /// The supported value domain is booleans, Rust integers, tuples, and
    /// zero-sized values. Direct locals and tuple-field projections are
    /// modeled. Calls, references, raw pointers, heap memory, and unsupported
    /// arithmetic return a location-aware error rather than silently producing
    /// an unsound result.
    pub fn execute<'tcx>(
        &mut self,
        tcx: TyCtxt<'tcx>,
        body: &Body<'tcx>,
    ) -> Result<ExecutionResult, MirExecutionError> {
        self.execute_limiting_to(tcx, body, ExecutionLimits::default())
    }

    pub fn execute_limiting_to<'tcx>(
        &mut self,
        tcx: TyCtxt<'tcx>,
        body: &Body<'tcx>,
        limits: ExecutionLimits,
    ) -> Result<ExecutionResult, MirExecutionError> {
        Executor {
            context: self,
            tcx,
            body,
            typing_env: TypingEnv::post_analysis(tcx, body.source.def_id()),
            limits,
            result: ExecutionResult::default(),
        }
        .run()
    }

    fn conjoin<'a>(&mut self, terms: impl IntoIterator<Item = &'a Term>) -> Term {
        let mut terms = terms.into_iter().copied();

        if let Some(first) = terms.next() {
            terms.fold(first, |lhs, rhs| self.and(lhs, rhs))
        } else {
            self.bool_lit(true)
        }
    }
}

impl<'a, 'tcx> Executor<'a, 'tcx> {
    fn run(mut self) -> Result<ExecutionResult, MirExecutionError> {
        let initial = self.initial_state()?;

        // pending states to be explored and explored states.
        let mut pending = vec![initial];
        let mut explored = 0_usize;

        while let Some(mut state) = pending.pop() {
            let bb = state.bb;
            let data = &self.body.basic_blocks[bb];

            explored += 1;
            if explored > self.limits.max_states {
                return Err(data.entry_loc(bb).error(format!(
                    "execution exceeded the {}-state exploration limit",
                    self.limits.max_states
                )));
            }
            if state.steps >= self.limits.max_steps {
                return Err(data.entry_loc(bb).error(format!(
                    "path exceeded the {}-step exploration limit (the body may contain a loop)",
                    self.limits.max_steps
                )));
            }
            state.steps += 1;

            for (loc, stmt) in data.statements_loc(bb) {
                self.execute_statement(&mut state, &stmt.kind, loc)?;
            }

            let (loc, terminator) = data.terminator_loc(bb);
            self.execute_terminator(state, &terminator.kind, loc, &mut pending)?;

            if pending.len() > self.limits.max_pending {
                return Err(loc.error(format!(
                    "more than {} symbolic paths are pending",
                    self.limits.max_pending
                )));
            }
        }

        Ok(self.result)
    }

    fn initial_state(&mut self) -> Result<State, MirExecutionError> {
        let location = self.body.basic_blocks[START_BLOCK].entry_loc(START_BLOCK);
        let mut store: Vec<Option<Term>> = self.body.local_decls.iter().map(|_| None).collect();
        let mut facts = SmallVec::new();

        for idx in 1..=self.body.arg_count {
            let local = Local::from_usize(idx);
            let ty = self.body.local_decls[local].ty;
            let Some(sort) = self.sort_for_ty(ty) else {
                return Err(location
                    .error(format!("argument {idx} has unsupported type `{ty}`")));
            };
            let name = self.argument_name(local, idx);
            let symbol = self.context.symbol(&name, sort);
            let term = self.context.sym(symbol);
            self.result.arguments.push(SymbolicArgument { local, symbol, value: term });
            store[local.as_usize()] = Some(term));
            self.add_integer_range_facts(ty, term, &mut facts);
        }

        Ok(State { bb: START_BLOCK, store, facts, steps: 0 })
    }

    fn argument_name(&self, local: Local, argument_number: usize) -> String {
        self.body
            .var_debug_info
            .iter()
            .find_map(|info| {
                if info.argument_index != Some(argument_number as u16) {
                    return None;
                }
                match info.value {
                    VarDebugInfoContents::Place(place)
                        if place.local == local && place.projection.is_empty() =>
                    {
                        Some(info.name.as_str().to_owned())
                    }
                    _ => None,
                }
            })
            .unwrap_or_else(|| format!("arg{argument_number}"))
    }

    fn execute_statement(
        &mut self,
        state: &mut State,
        statement: &StatementKind<'tcx>,
        location: Location,
    ) -> Result<(), MirExecutionError> {
        match statement {
            StatementKind::Assign(assignment) => {
                let (place, rvalue) = &**assignment;
                let value = self.eval_rvalue(state, rvalue, location)?;
                self.write_place(state, *place, value, location)
            }
            StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
                state.store[local.as_usize()] = None;
                Ok(())
            }
            StatementKind::Intrinsic(intrinsic) => match &**intrinsic {
                NonDivergingIntrinsic::Assume(operand) => {
                    let term = self
                        .eval_operand(state, operand, location)?;
                    state.facts.push(term);
                    Ok(())
                }
                other => Err(location.error(format!("intrinsic `{other:?}`"))),
            },
            StatementKind::Nop
            | StatementKind::FakeRead(_)
            | StatementKind::PlaceMention(_)
            | StatementKind::AscribeUserType(_, _)
            | StatementKind::Coverage(_)
            | StatementKind::ConstEvalCounter
            | StatementKind::BackwardIncompatibleDropHint { .. } => Ok(()),
            other => Err(location.error(format!("statement `{other:?}`"))),
        }
    }

    fn execute_terminator(
        &mut self,
        mut state: State,
        terminator: &TerminatorKind<'tcx>,
        location: Location,
        pending: &mut Vec<State>,
    ) -> Result<(), MirExecutionError> {
        match terminator {
            TerminatorKind::Goto { target } => {
                state.bb = *target;
                pending.push(state);
            }
            TerminatorKind::SwitchInt { discr, targets } => {
                let discr_value = self
                    .eval_operand(&state, discr, location)?;
                let discr_ty = discr.ty(self.body, self.tcx);
                let mut excluded = SmallVec::<[Term; 4]>::new();

                for (bits, target) in targets.iter() {
                    let condition = self.switch_equality(discr_value, discr_ty, bits, location)?;
                    excluded.push(condition);
                    let mut branch = state.clone();
                    branch.bb = target;
                    branch.facts.push(condition);
                    pending.push(branch);
                }

                for equality in excluded {
                    let inequality = self.context.not(equality);
                    state.facts.push(inequality);
                }
                state.bb = targets.otherwise();
                pending.push(state);
            }
            TerminatorKind::Assert { cond, expected, target, .. } => {
                let mut condition = self
                    .eval_operand(&state, cond, location)?;
                if !expected {
                    condition = self.context.not(condition);
                }
                let path_condition = self.context.conjoin(&state.facts);
                let assertion = self.context.implies(path_condition, condition);
                self.result.assertions.push(assertion);
                state.facts.push(condition);
                state.bb = *target;
                pending.push(state);
            }
            TerminatorKind::Return => {
                let fact = self.context.conjoin(&state.facts);
                let value = state.store[RETURN_PLACE.as_usize()].unwrap();
                self.result.return_paths.push(ReturnPath { fact, value });
            }
            TerminatorKind::Unreachable => {}
            TerminatorKind::FalseEdge { real_target, .. }
            | TerminatorKind::FalseUnwind { real_target, .. } => {
                state.bb = *real_target;
                pending.push(state);
            }
            other => return Err(location.error(format!("terminator `{other:?}`"))),
        }
        Ok(())
    }

    fn eval_rvalue(
        &mut self,
        state: &State,
        rvalue: &Rvalue<'tcx>,
        location: Location,
    ) -> Result<Term, MirExecutionError> {
        match rvalue {
            Rvalue::Use(operand, _) => self.eval_operand(state, operand, location),
            Rvalue::BinaryOp(op, operands) => {
                let (lhs, rhs) = &**operands;
                self.eval_binary(state, *op, lhs, rhs, location)
            }
            Rvalue::UnaryOp(op, operand) => {
                let value = self
                    .eval_operand(state, operand, location)?;
                let operand_ty = operand.ty(self.body, self.tcx);
                let operation = match op {
                    UnOp::Not if operand_ty.is_bool() => Uop::Not,
                    UnOp::Neg if self.integer_layout(operand_ty).is_some() => Uop::Neg,
                    other => return Err(location.error(format!("unary operation `{other:?}`"))),
                };
                Ok(self.context.unary(operation, value))
            }
            Rvalue::Aggregate(kind, operands) if matches!(&**kind, AggregateKind::Tuple) => {
                let values:Box<[Term]> = operands
                    .iter()
                    .map(|operand| self.eval_operand(state, operand, location))
                    .collect()?;
                todo!();
                // Ok(Term)
            }
            Rvalue::Cast(_, operand, target_ty) => {
                let source_ty = operand.ty(self.body, self.tcx);
                if self.is_lossless_integer_cast(source_ty, *target_ty) {
                    self.eval_operand(state, operand, location)
                } else {
                    Err(location.error(format!(
                        "potentially lossy cast from `{source_ty}` to `{target_ty}`"
                    )))
                }
            }
            other => Err(location.error(format!("rvalue `{other:?}`"))),
        }
    }

    fn eval_binary(
        &mut self,
        state: &State,
        operation: BinOp,
        lhs: &Operand<'tcx>,
        rhs: &Operand<'tcx>,
        location: Location,
    ) -> Result<Term, MirExecutionError> {
        let lhs_term = self.eval_operand(state, lhs, location)?;
        let rhs_term = self.eval_operand(state, rhs, location)?;

        let checked_arithmetic = matches!(
            operation,
            BinOp::AddWithOverflow | BinOp::SubWithOverflow | BinOp::MulWithOverflow
        );
        let translated_operation = match operation {
            BinOp::AddWithOverflow => Op::Add,
            BinOp::SubWithOverflow => Op::Sub,
            BinOp::MulWithOverflow => Op::Mul,
            BinOp::Eq => Op::Eq,
            BinOp::Ne => Op::Ne,
            BinOp::Lt => Op::Lt,
            BinOp::Le => Op::Le,
            BinOp::Gt => Op::Gt,
            BinOp::Ge => Op::Ge,
            BinOp::BitAnd if lhs.ty(self.body, self.tcx).is_bool() => Op::And,
            BinOp::BitOr if lhs.ty(self.body, self.tcx).is_bool() => Op::Or,
            other => return Err(location.error(format!("binary operation `{other:?}`"))),
        };
        let value = self.context.binary(translated_operation, lhs_term, rhs_term);

        if checked_arithmetic {
            let ty = lhs.ty(self.body, self.tcx);
            let Some((bits, signed)) = self.integer_layout(ty) else {
                return Err(
                    location.error(format!("checked arithmetic on unsupported type `{ty}`"))
                );
            };
            let Some((minimum, maximum)) = integer_bounds(bits, signed) else {
                return Err(location.error(""));
            };
            let minimum = self.context.int_lit(minimum);
            let maximum = self.context.int_lit(maximum);
            let below_minimum = self.context.lt(value, minimum);
            let above_maximum = self.context.gt(value, maximum);
            let overflowed = self.context.or(below_minimum, above_maximum);
            let tuple = TermDef::Tuple(vec![value, overflowed]);
            return Ok();
        }

        Ok(value)
    }

    fn eval_operand(
        &mut self,
        state: &State,
        operand: &Operand<'tcx>,
        location: Location,
    ) -> Result<Term, MirExecutionError> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.read_place(state, *place, location),
            Operand::Constant(constant) => {
                let ty = constant.const_.ty();
                if ty.is_unit() {
                    return Ok();
                }
                if ty.is_bool() {
                    let value =
                        constant.const_.try_eval_bool(self.tcx, self.typing_env).ok_or_else(
                            || location.error(format!("constant `{constant}` is not evaluatable")),
                        )?;
                    return Ok(self.context.bool_lit(value));
                }
                let Some((bits, signed)) = self.integer_layout(ty) else {
                    return Err(location
                        .error(format!("constant `{constant}` has unsupported type `{ty}`")));
                };
                let raw =
                    constant.const_.try_eval_bits(self.tcx, self.typing_env).ok_or_else(|| {
                        location.error(format!("constant `{constant}` is not evaluatable"))
                    })?;
                let value = integer_from_bits(raw, bits, signed).ok_or_else(|| {
                    location.error(format!(
                        "constant `{constant}` does not fit the symbolic integer domain"
                    ))
                })?;
                Ok(self.context.int_lit(value))
            }
            Operand::RuntimeChecks(_) => Err(location.error("runtime-check configuration operand")),
        }
    }

    fn read_place(
        &self,
        state: &State,
        place: Place<'tcx>,
        location: Location,
    ) -> Result<Term, MirExecutionError> {
        let mut value = state.store[place.local.as_usize()].as_ref().ok_or_else(|| {
            location.error(format!("read of uninitialized local `{:?}`", place.local))
        })?;

        for projection in place.projection {
            match (projection, value) {
                (ProjectionElem::Field(field, _), Term::Aggregate(fields)) => {
                    value = fields.get(field.as_usize()).ok_or_else(|| {
                        location.error(format!("field {field:?} is outside symbolic aggregate"))
                    })?;
                }
                (other, _) => return Err(location.error(format!("place projection `{other:?}`"))),
            }
        }
        Ok(value.clone())
    }

    fn write_place(
        &self,
        state: &mut State,
        place: Place<'tcx>,
        value: Term,
        location: Location,
    ) -> Result<(), MirExecutionError> {
        if place.projection.is_empty() {
            state.store[place.local.as_usize()] = Some(value);
            return Ok(());
        }

        let root = state.store[place.local.as_usize()].as_mut().ok_or_else(|| {
            location.error(format!("write through uninitialized local `{:?}`", place.local))
        })?;
        write_projection(root, place.projection, value, location)
    }

    fn switch_equality(
        &mut self,
        discr: Term,
        ty: Ty<'tcx>,
        raw: u128,
        location: Location,
    ) -> Result<Term, MirExecutionError> {
        if ty.is_bool() {
            return match raw {
                0 => Ok(self.context.not(discr)),
                1 => Ok(discr),
                _ => Err(location.error(format!("invalid boolean switch value {raw}"))),
            };
        }
        let Some((bits, signed)) = self.integer_layout(ty) else {
            return Err(location.error(format!("switch on unsupported type `{ty}`")));
        };
        let value = integer_from_bits(raw, bits, signed).ok_or_else(|| {
            location.error("switch value does not fit the symbolic integer domain")
        })?;
        let value = self.context.int_lit(value);
        Ok(self.context.eq(discr, value))
    }

    fn sort_for_ty(&mut self, ty: Ty<'tcx>) -> Option<Sort> {
        if ty.is_bool() {
            Some(self.context.bool_sort())
        } else if self
            .integer_layout(ty)
            .and_then(|(bits, signed)| integer_bounds(bits, signed))
            .is_some()
        {
            Some(self.context.int_sort())
        } else {
            None
        }
    }

    fn integer_layout(&self, ty: Ty<'tcx>) -> Option<(u64, bool)> {
        let pointer_bits = self.tcx.data_layout.pointer_size().bits();
        match ty.kind() {
            TyKind::Int(kind) => Some((int_width(*kind, pointer_bits), true)),
            TyKind::Uint(kind) => Some((uint_width(*kind, pointer_bits), false)),
            _ => None,
        }
    }

    fn add_integer_range_facts(
        &mut self,
        ty: Ty<'tcx>,
        term: Term,
        facts: &mut SmallVec<[Term; 8]>,
    ) {
        let Some((bits, signed)) = self.integer_layout(ty) else { return };
        if let Some((minimum, maximum)) = integer_bounds(bits, signed) {
            let minimum = self.context.int_lit(minimum);
            let maximum = self.context.int_lit(maximum);
            facts.push(self.context.ge(term, minimum));
            facts.push(self.context.le(term, maximum));
        }
    }

    fn is_lossless_integer_cast(&self, source: Ty<'tcx>, target: Ty<'tcx>) -> bool {
        let (Some(source), Some(target)) =
            (self.integer_layout(source), self.integer_layout(target))
        else {
            return false;
        };
        let (Some((source_min, source_max)), Some((target_min, target_max))) =
            (integer_bounds(source.0, source.1), integer_bounds(target.0, target.1))
        else {
            return false;
        };
        target_min <= source_min && source_max <= target_max
    }
}

fn write_projection(
    root: &mut Term,
    projection: &[mir::PlaceElem<'_>],
    value: Term,
    location: Location,
) -> Result<(), MirExecutionError> {
    let Some((first, rest)) = projection.split_first() else {
        *root = value;
        return Ok(());
    };
    match (first, root) {
        (ProjectionElem::Field(field, _), Term::Aggregate(fields)) => {
            let field = fields.get_mut(field.as_usize()).ok_or_else(|| {
                location.error(format!("field {field:?} is outside symbolic aggregate"))
            })?;
            write_projection(field, rest, value, location)
        }
        (other, _) => Err(location.error(format!("place projection `{other:?}`"))),
    }
}

fn int_width(kind: IntTy, pointer_bits: u64) -> u64 {
    match kind {
        IntTy::I8 => 8,
        IntTy::I16 => 16,
        IntTy::I32 => 32,
        IntTy::I64 => 64,
        IntTy::I128 => 128,
        IntTy::Isize => pointer_bits,
    }
}

fn uint_width(kind: UintTy, pointer_bits: u64) -> u64 {
    match kind {
        UintTy::U8 => 8,
        UintTy::U16 => 16,
        UintTy::U32 => 32,
        UintTy::U64 => 64,
        UintTy::U128 => 128,
        UintTy::Usize => pointer_bits,
    }
}

fn integer_bounds(bits: u64, signed: bool) -> Option<(i128, i128)> {
    if signed {
        if bits == 128 {
            Some((i128::MIN, i128::MAX))
        } else {
            let magnitude = 1_i128 << (bits - 1);
            Some((-magnitude, magnitude - 1))
        }
    } else if bits < 128 {
        Some((0, (1_i128 << bits) - 1))
    } else {
        None
    }
}

fn integer_from_bits(raw: u128, bits: u64, signed: bool) -> Option<i128> {
    if signed {
        if bits == 128 {
            Some(raw as i128)
        } else {
            let mask = (1_u128 << bits) - 1;
            let raw = raw & mask;
            if raw & (1_u128 << (bits - 1)) == 0 {
                Some(raw as i128)
            } else {
                Some((raw | !mask) as i128)
            }
        }
    } else {
        i128::try_from(raw).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionResult, integer_bounds, integer_from_bits};
    use crate::{Context, Intern};

    #[test]
    fn decodes_signed_mir_constants_using_their_rust_width() {
        assert_eq!(integer_from_bits(0xff, 8, true), Some(-1));
        assert_eq!(integer_from_bits(0x80, 8, true), Some(-128));
        assert_eq!(integer_from_bits(0x7f, 8, true), Some(127));
        assert_eq!(integer_from_bits(u128::MAX, 128, true), Some(-1));
    }

    #[test]
    fn rejects_unsigned_values_outside_the_term_constant_domain() {
        assert_eq!(integer_bounds(128, false), None);
        assert_eq!(integer_from_bits(i128::MAX as u128 + 1, 128, false), None);
    }

    #[test]
    fn assertion_vc_is_guarded_by_its_path_condition() {
        let mut context = Context::default();
        let bool_sort = context.bool_sort();
        let path_symbol = context.symbol("path", bool_sort);
        let assertion_symbol = context.symbol("assertion", bool_sort);
        let path = context.sym(path_symbol);
        let assertion = context.sym(assertion_symbol);
        let execution = ExecutionResult {
            assertions: vec![context.implies(path, assertion)],
            ..ExecutionResult::default()
        };

        let vc = context.conjoin(execution.assertions.iter());

        assert_eq!(
            context.get(vc),
            crate::TermDef::Binary { op: crate::Op::Implies, lhs: path, rhs: assertion }
        );
    }
}
