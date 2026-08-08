use rustc_index::IndexVec;
use rustc_middle::{
    mir::{self, AggregateKind, BinOp, Location, Operand, Place, ProjectionElem, Rvalue, UnOp},
    ty::Ty,
};
use verifier_core::{Context, Intern, Op, Term, TermDef, Uop};

use crate::engine::obligation::{ExecutionError, LocationExt};

use super::{
    Evaluate, Executor, State,
    numeric::{integer_bounds, integer_from_bits},
};

impl<'a, 'tcx> Executor<'a, 'tcx> {
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

    pub(super) fn write_place(
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
                Ok(self.context.unary(Uop::Neg, value))
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
        if what.1 { Ok(term) } else { Ok(self.context.not(term)) }
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
