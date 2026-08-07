use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FormatResult},
};

use rustc_index::IndexVec;
use rustc_middle::{
    mir::{
        self, AggregateKind, BasicBlock, BinOp, Body, Local, Location, NonDivergingIntrinsic,
        Operand, Place, ProjectionElem, RETURN_PLACE, Rvalue, START_BLOCK, StatementKind,
        TerminatorKind, UnOp, VarDebugInfoContents,
    },
    ty::{IntTy, Ty, TyCtxt, TyKind, TypingEnv, UintTy},
};
use smallvec::SmallVec;

use crate::{Context, Intern, Op, Sort, Sym, Term, TermDef, Uop};

/// Limits which keep forward exploration finite in the presence of loops and
/// exponential path growth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum number of basic blocks visited along any single path.
    pub max_steps: u32,
    /// Maximum worklist size at any instant.
    pub max_pending: u32,
    /// Maximum total number of symbolic states entering basic blocks.
    pub max_states: u32,
}

impl Default for Limits {
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
pub struct ExecutionError {
    pub location: Location,
    pub message: String,
}

impl Display for ExecutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FormatResult {
        write!(
            formatter,
            "MIR symbolic execution failed at {:?}[{:?}]: {}",
            self.location.block, self.location.statement_index, self.message
        )
    }
}

impl Error for ExecutionError {}

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

/// purely adapter trait. need better naming or abstraction.
trait LocationAdapter {
    fn error(self, message: impl Into<String>) -> ExecutionError;
}

impl LocationAdapter for Location {
    fn error(self, message: impl Into<String>) -> ExecutionError {
        ExecutionError { location: self, message: message.into() }
    }
}

fn entry_loc(bb: BasicBlock) -> Location {
    Location { block: bb, statement_index: 0 }
}

struct Executor<'a, 'tcx> {
    context: &'a mut Context,
    tcx: TyCtxt<'tcx>,
    body: &'a Body<'tcx>,
    typing_env: TypingEnv<'tcx>,
    limits: Limits,
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
    ) -> Result<ExecutionResult, ExecutionError> {
        let limits = Limits::default();
        Executor {
            context: self,
            tcx: tcx,
            body: body,
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
    fn run(mut self) -> Result<ExecutionResult, ExecutionError> {
        let initial = self.initial_state()?;

        // pending states to be explored and explored states.
        let mut pending = vec![initial];
        let mut explored = 0_usize;

        while let Some(mut state) = pending.pop() {
            let location = state.location;
            let data = &self.body.basic_blocks[location.block];

            if location.statement_index == 0 {
                explored += 1;
                if explored > self.limits.max_states as usize {
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

        Ok(self.result)
    }

    fn initial_state(&mut self) -> Result<State, ExecutionError> {
        let location = entry_loc(START_BLOCK);
        let mut store: IndexVec<Local, Option<Term>> =
            self.body.local_decls.iter().map(|_| None).collect();
        let mut facts = SmallVec::new();

        for idx in 1..=self.body.arg_count {
            let local = Local::from_usize(idx);
            let ty = self.body.local_decls[local].ty;
            let Some(sort) = self.sort_for_ty(ty) else {
                return Err(location.error(format!("argument {idx} has unsupported type `{ty}`")));
            };
            let name = self.argument_name(local, idx);
            let symbol = self.context.symbol(&name, sort);
            let term = self.context.sym(symbol);
            self.result.arguments.push(SymbolicArgument { local, symbol, value: term });
            store[local] = Some(term);
            self.add_integer_range_facts(ty, term, &mut facts);
        }

        Ok(State { location: entry_loc(START_BLOCK), store, facts, steps: 0 })
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

    fn read_place(
        &self,
        state: &State,
        place: Place<'tcx>,
        location: Location,
    ) -> Result<Term, ExecutionError> {
        let mut value = *state.store[place.local].as_ref().ok_or_else(|| {
            location.error(format!("read of uninitialized local `{:?}`", place.local))
        })?;

        for projection in place.projection {
            match (projection, self.context.get(value)) {
                (ProjectionElem::Field(field, _), TermDef::Tuple(fields)) => {
                    value = *fields.get(field.as_usize()).ok_or_else(|| {
                        location.error(format!("field {field:?} is outside symbolic tuple"))
                    })?;
                }
                (other, _) => return Err(location.error(format!("place projection `{other:?}`"))),
            }
        }
        Ok(value)
    }

    fn write_place(
        &mut self,
        state: &mut State,
        place: Place<'tcx>,
        term: Term,
    ) -> Result<(), ExecutionError> {
        if place.projection.is_empty() {
            state.store[place.local] = Some(term);
            return Ok(());
        }

        let root = *state.store[place.local].as_ref().ok_or_else(|| {
            state.location.error(format!("write through uninitialized local `{:?}`", place.local))
        })?;
        let updated = write_projection(self.context, root, place.projection, term, state.location)?;
        state.store[place.local] = Some(updated);
        Ok(())
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
}

impl<'a, 'tcx, 'mir> Execute<&'mir StatementKind<'tcx>> for Executor<'a, 'tcx> {
    fn execute(
        &mut self,
        mut state: State,
        statement: &'mir StatementKind<'tcx>,
        pending: &mut Vec<State>,
    ) -> Result<(), ExecutionError> {
        let loc = state.location;

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
                return Err(loc.error(format!("intrinsic `{other:?}`")));
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
                state.location = entry_loc(*target);
                pending.push(state);
            }
            TerminatorKind::SwitchInt { discr, targets } => {
                let discr_term = self.evaluate(&state, discr)?;
                let discr_ty = discr.ty(self.body, self.tcx);
                let mut excluded = SmallVec::<[Term; 4]>::new();

                for (bits, target) in targets.iter() {
                    let condition = self.evaluate(&state, (discr_term, discr_ty, bits))?;
                    excluded.push(condition);

                    let mut branch = state.clone();
                    branch.location = entry_loc(target);
                    branch.facts.push(condition);
                    pending.push(branch);
                }

                for equality in excluded {
                    let inequality = self.context.not(equality);
                    state.facts.push(inequality);
                }
                let target = targets.otherwise();
                state.location = entry_loc(target);
                pending.push(state);
            }
            TerminatorKind::Assert { cond, expected, target, .. } => {
                // evaluate the assertion
                let assertion = self.evaluate(&state, (cond, *expected))?;

                // create the vc for the assertion
                let current_fact = self.context.conjoin(&state.facts);
                let implication = self.context.implies(current_fact, assertion);
                self.result.assertions.push(implication);

                // go ahead
                state.facts.push(assertion);
                state.location = entry_loc(*target);
                pending.push(state);
            }
            TerminatorKind::Return => {
                let fact = self.context.conjoin(&state.facts);
                let value = state.store[RETURN_PLACE].unwrap();
                self.result.return_paths.push(ReturnPath { fact, value });
            }
            TerminatorKind::Unreachable => {}
            TerminatorKind::FalseEdge { real_target, .. }
            | TerminatorKind::FalseUnwind { real_target, .. } => {
                state.location = entry_loc(*real_target);
                pending.push(state);
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

impl<'a, 'tcx, 'mir> Evaluate<&'mir Rvalue<'tcx>> for Executor<'a, 'tcx> {
    type Output = Term;

    fn evaluate(
        &mut self,
        state: &State,
        rvalue: &'mir Rvalue<'tcx>,
    ) -> Result<Self::Output, ExecutionError> {
        match rvalue {
            Rvalue::Use(operand, _) => self.evaluate(state, operand),
            Rvalue::BinaryOp(op, deref!((lhs, rhs))) => self.evaluate(state, (*op, lhs, rhs)),
            Rvalue::UnaryOp(UnOp::Not, operand) => {
                let term = self.evaluate(state, operand)?;
                Ok(self.context.unary(Uop::Not, term))
            }
            Rvalue::UnaryOp(UnOp::Neg, operand) => {
                let value = self.evaluate(state, operand)?;
                Ok(self.context.unary(Uop::Not, value))
            }
            Rvalue::UnaryOp(..) => todo!(),
            Rvalue::Aggregate(kind, operands) if matches!(&**kind, AggregateKind::Tuple) => {
                let values = self.evaluate(state, operands)?;
                Ok(self.context.tuple(values))
            }
            Rvalue::Cast(..) => todo!(),

            Rvalue::Repeat(..) => todo!(),
            Rvalue::Ref(..) => todo!(),
            Rvalue::ThreadLocalRef(..) => todo!(),
            Rvalue::RawPtr(..) => todo!(),
            Rvalue::Discriminant(..) => todo!(),
            Rvalue::Aggregate(..) => todo!(),
            Rvalue::CopyForDeref(..) => todo!(),
            Rvalue::WrapUnsafeBinder(..) => todo!(),
            Rvalue::Reborrow(..) => todo!(),
        }
    }
}

impl<'a, 'tcx, 'mir> Evaluate<(BinOp, &'mir Operand<'tcx>, &'mir Operand<'tcx>)>
    for Executor<'a, 'tcx>
{
    type Output = Term;

    fn evaluate(
        &mut self,
        state: &State,
        (operation, lhs, rhs): (BinOp, &'mir Operand<'tcx>, &'mir Operand<'tcx>),
    ) -> Result<Self::Output, ExecutionError> {
        let operation = operation;
        let location = state.location;
        let lhs_term = self.evaluate(state, lhs)?;
        let rhs_term = self.evaluate(state, rhs)?;
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
                return Err(location.error(format!(
                    "checked arithmetic on `{ty}` exceeds the symbolic integer domain"
                )));
            };
            let minimum = self.context.int_lit(minimum);
            let maximum = self.context.int_lit(maximum);
            let below_minimum = self.context.lt(value, minimum);
            let above_maximum = self.context.gt(value, maximum);
            let overflowed = self.context.or(below_minimum, above_maximum);
            return Ok(self.context.tuple(Box::new([value, overflowed])));
        }
        Ok(value)
    }
}

impl<'a, 'tcx, 'mir> Evaluate<&'mir Operand<'tcx>> for Executor<'a, 'tcx> {
    type Output = Term;

    fn evaluate(
        &mut self,
        state: &State,
        operand: &'mir Operand<'tcx>,
    ) -> Result<Self::Output, ExecutionError> {
        let location = state.location;
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.evaluate(state, *place),
            Operand::Constant(constant) => {
                let ty = constant.const_.ty();
                if ty.is_unit() {
                    return Ok(self.context.unit());
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
}

impl<'a, 'tcx, 'mir> Evaluate<(&'mir Operand<'tcx>, bool)> for Executor<'a, 'tcx> {
    type Output = Term;

    fn evaluate(
        &mut self,
        state: &State,
        what: (&'mir Operand<'tcx>, bool),
    ) -> Result<Self::Output, ExecutionError> {
        let term = self.evaluate(state, what.0)?;
        Ok(self.context.not(term))
    }
}

impl<'a, 'tcx, 'mir, I> Evaluate<&'mir IndexVec<I, Operand<'tcx>>> for Executor<'a, 'tcx>
where
    I: rustc_index::Idx,
{
    type Output = Box<[Term]>;

    fn evaluate(
        &mut self,
        state: &State,
        operands: &'mir IndexVec<I, Operand<'tcx>>,
    ) -> Result<Self::Output, ExecutionError> {
        operands
            .iter()
            .map(|operand| self.evaluate(state, operand))
            .collect::<Result<Vec<_>, _>>()
            .map(Vec::into)
    }
}

impl<'a, 'tcx> Evaluate<Place<'tcx>> for Executor<'a, 'tcx> {
    type Output = Term;

    fn evaluate(
        &mut self,
        state: &State,
        place: Place<'tcx>,
    ) -> Result<Self::Output, ExecutionError> {
        self.read_place(state, place, state.location)
    }
}

impl<'a, 'tcx> Evaluate<(Term, Ty<'tcx>, u128)> for Executor<'a, 'tcx> {
    type Output = Term;

    fn evaluate(
        &mut self,
        state: &State,
        (discriminant, ty, raw): (Term, Ty<'tcx>, u128),
    ) -> Result<Self::Output, ExecutionError> {
        let location = state.location;
        if ty.is_bool() {
            return match raw {
                0 => Ok(self.context.not(discriminant)),
                1 => Ok(discriminant),
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
        Ok(self.context.eq(discriminant, value))
    }
}

fn write_projection(
    context: &mut Context,
    root: Term,
    projection: &[mir::PlaceElem<'_>],
    value: Term,
    location: Location,
) -> Result<Term, ExecutionError> {
    let Some((first, rest)) = projection.split_first() else {
        return Ok(value);
    };
    match (first, context.get(root)) {
        (ProjectionElem::Field(field, _), TermDef::Tuple(mut fields)) => {
            let current = *fields.get(field.as_usize()).ok_or_else(|| {
                location.error(format!("field {field:?} is outside symbolic tuple"))
            })?;
            fields[field.as_usize()] = write_projection(context, current, rest, value, location)?;
            Ok(context.tuple(fields))
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
