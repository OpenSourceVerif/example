use index_vec::IndexVec as FieldVec;
use rustc_index::{Idx, IndexVec};
use rustc_middle::{
    mir::{
        self, AggregateKind, BinOp as MirOp, Location, Operand, Place, ProjectionElem, Rvalue, UnOp,
    },
    ty::Ty,
};
use smallvec::SmallVec;
use verifier_core::{Context, DefStore, Field, Op, SortDef, Term};

use crate::{
    engine::obligation::{ExecutionError, LocationExt},
    types::{integer_bounds, integer_from_bits, integer_layout},
};

use super::{Evaluate, Executor, State};

trait FieldIndexExt {
    fn to_field(self) -> Field;
}

impl<I: Idx> FieldIndexExt for I {
    fn to_field(self) -> Field {
        Field::from_usize(self.index())
    }
}

impl<'a, 'tcx> Executor<'a, 'tcx> {
    fn read_place(
        &mut self,
        state: &State,
        place: Place<'tcx>,
        location: Location,
    ) -> Result<Term, ExecutionError> {
        let mut value = *state.store[place.local].as_ref().ok_or_else(|| {
            location.error(format!("read of uninitialized local `{:?}`", place.local))
        })?;

        for projection in place.projection {
            let ProjectionElem::Field(field, _) = projection else {
                return Err(location.error(format!("place projection `{projection:?}`")));
            };
            let SortDef::Tuple(fields) = self.cx.get(self.cx.term_sort(value)) else {
                return Err(location.error("field projection from non-tuple term"));
            };
            if field.as_usize() >= fields.len() {
                return Err(location.error(format!("field {field:?} is outside symbolic tuple")));
            }
            value = self.cx.proj(value, field.to_field());
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
        let updated = write_projection(self.cx, root, place.projection, term, state.location)?;
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
                let ty = operand.ty(self.body, self.tcx);
                if !ty.is_bool() {
                    return Err(state
                        .location
                        .error(format!("bitwise not on unsupported type `{ty}`")));
                }
                let term = self.evaluate(state, operand)?;
                Ok(self.cx.not(term))
            }
            Rvalue::UnaryOp(UnOp::Neg, operand) => {
                let value = self.evaluate(state, operand)?;
                Ok(self.cx.neg(value))
            }
            Rvalue::Aggregate(kind, operands) if matches!(&**kind, AggregateKind::Tuple) => {
                let values = self.evaluate(state, operands)?;
                Ok(self.cx.tuple(&values))
            }
            Rvalue::UnaryOp(..)
            | Rvalue::Cast(..)
            | Rvalue::Repeat(..)
            | Rvalue::Ref(..)
            | Rvalue::ThreadLocalRef(..)
            | Rvalue::RawPtr(..)
            | Rvalue::Discriminant(..)
            | Rvalue::Aggregate(..)
            | Rvalue::CopyForDeref(..)
            | Rvalue::WrapUnsafeBinder(..)
            | Rvalue::Reborrow(..) => {
                Err(state.location.error(format!("unsupported rvalue `{rvalue:?}`")))
            }
        }
    }
}

impl<'a, 'tcx, 'mir> Evaluate<(MirOp, &'mir Operand<'tcx>, &'mir Operand<'tcx>)>
    for Executor<'a, 'tcx>
{
    type Output = Term;

    fn evaluate(
        &mut self,
        state: &State,
        (mir_op, lhs, rhs): (MirOp, &'mir Operand<'tcx>, &'mir Operand<'tcx>),
    ) -> Result<Self::Output, ExecutionError> {
        let location = state.location;
        let lhs_term = self.evaluate(state, lhs)?;
        let rhs_term = self.evaluate(state, rhs)?;
        let checked_arithmetic = matches!(
            mir_op,
            MirOp::AddWithOverflow | MirOp::SubWithOverflow | MirOp::MulWithOverflow
        );
        let op = match mir_op {
            MirOp::AddWithOverflow => Op::Add,
            MirOp::SubWithOverflow => Op::Sub,
            MirOp::MulWithOverflow => Op::Mul,
            MirOp::Eq => Op::Eq,
            MirOp::Ne => Op::Ne,
            MirOp::Lt => Op::Lt,
            MirOp::Le => Op::Le,
            MirOp::Gt => Op::Gt,
            MirOp::Ge => Op::Ge,
            MirOp::BitAnd if lhs.ty(self.body, self.tcx).is_bool() => Op::And,
            MirOp::BitOr if lhs.ty(self.body, self.tcx).is_bool() => Op::Or,
            MirOp::Add
            | MirOp::AddUnchecked
            | MirOp::Sub
            | MirOp::SubUnchecked
            | MirOp::Mul
            | MirOp::MulUnchecked
            | MirOp::Div
            | MirOp::Rem
            | MirOp::BitXor
            | MirOp::BitAnd
            | MirOp::BitOr
            | MirOp::Shl
            | MirOp::ShlUnchecked
            | MirOp::Shr
            | MirOp::ShrUnchecked
            | MirOp::Cmp
            | MirOp::Offset => {
                return Err(location.error(format!("unsupported binary operation `{mir_op:?}`")));
            }
        };
        let value = self.cx.binary(op, lhs_term, rhs_term);
        if checked_arithmetic {
            let ty = lhs.ty(self.body, self.tcx);
            let Some(layout) = integer_layout(self.tcx, ty) else {
                return Err(
                    location.error(format!("checked arithmetic on unsupported type `{ty}`"))
                );
            };
            let Some((minimum, maximum)) = integer_bounds(layout) else {
                return Err(location.error(format!(
                    "checked arithmetic on `{ty}` exceeds the symbolic integer domain"
                )));
            };
            let minimum = self.cx.int_lit(minimum);
            let maximum = self.cx.int_lit(maximum);
            let below_minimum = self.cx.lt(value, minimum);
            let above_maximum = self.cx.gt(value, maximum);
            let overflowed = self.cx.or(below_minimum, above_maximum);
            return Ok(self.cx.tuple(&[value, overflowed]));
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
                    return Ok(self.cx.unit());
                }
                if ty.is_bool() {
                    let value =
                        constant.const_.try_eval_bool(self.tcx, self.typing_env).ok_or_else(
                            || location.error(format!("constant `{constant}` is not evaluatable")),
                        )?;
                    return Ok(self.cx.bool_lit(value));
                }
                let Some(layout) = integer_layout(self.tcx, ty) else {
                    return Err(location
                        .error(format!("constant `{constant}` has unsupported type `{ty}`")));
                };
                let raw =
                    constant.const_.try_eval_bits(self.tcx, self.typing_env).ok_or_else(|| {
                        location.error(format!("constant `{constant}` is not evaluatable"))
                    })?;
                let value = integer_from_bits(raw, layout).ok_or_else(|| {
                    location.error(format!(
                        "constant `{constant}` does not fit the symbolic integer domain"
                    ))
                })?;
                Ok(self.cx.int_lit(value))
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
        if what.1 { Ok(term) } else { Ok(self.cx.not(term)) }
    }
}

impl<'a, 'tcx, 'mir, I> Evaluate<&'mir IndexVec<I, Operand<'tcx>>> for Executor<'a, 'tcx>
where
    I: rustc_index::Idx,
{
    type Output = SmallVec<[Term; 4]>;

    fn evaluate(
        &mut self,
        state: &State,
        operands: &'mir IndexVec<I, Operand<'tcx>>,
    ) -> Result<Self::Output, ExecutionError> {
        operands.iter().map(|operand| self.evaluate(state, operand)).collect()
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
                0 => Ok(self.cx.not(discriminant)),
                1 => Ok(discriminant),
                _ => Err(location.error(format!("invalid boolean switch value {raw}"))),
            };
        }
        let Some(layout) = integer_layout(self.tcx, ty) else {
            return Err(location.error(format!("switch on unsupported type `{ty}`")));
        };
        let value = integer_from_bits(raw, layout).ok_or_else(|| {
            location.error("switch value does not fit the symbolic integer domain")
        })?;
        let value = self.cx.int_lit(value);
        Ok(self.cx.eq(discriminant, value))
    }
}

fn write_projection(
    cx: &mut Context,
    root: Term,
    projection: &[mir::PlaceElem<'_>],
    value: Term,
    location: Location,
) -> Result<Term, ExecutionError> {
    let Some((first, rest)) = projection.split_first() else {
        return Ok(value);
    };
    let ProjectionElem::Field(field, _) = first else {
        return Err(location.error(format!("place projection `{first:?}`")));
    };
    let SortDef::Tuple(field_sorts) = cx.get(cx.term_sort(root)) else {
        return Err(location.error("field projection from non-tuple term"));
    };
    let field = field.to_field();
    if field_sorts.get(field).is_none() {
        return Err(location.error(format!("field {field:?} is outside symbolic tuple")));
    }

    let mut fields: FieldVec<Field, Term> =
        field_sorts.indices().map(|field| cx.proj(root, field)).collect();
    let current = fields[field];
    fields[field] = write_projection(cx, current, rest, value, location)?;
    Ok(cx.tuple(fields.as_raw_slice()))
}
