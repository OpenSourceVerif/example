use rustc_middle::ty::{IntTy, Ty, TyKind, UintTy};
use smallvec::SmallVec;
use verifier_core::{Sort, Term};

use super::Executor;

impl<'a, 'tcx> Executor<'a, 'tcx> {
    pub(super) fn sort_for_ty(&mut self, ty: Ty<'tcx>) -> Option<Sort> {
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

    pub(super) fn integer_layout(&self, ty: Ty<'tcx>) -> Option<(u64, bool)> {
        let pointer_bits = self.tcx.data_layout.pointer_size().bits();
        match ty.kind() {
            TyKind::Int(kind) => Some((int_width(*kind, pointer_bits), true)),
            TyKind::Uint(kind) => Some((uint_width(*kind, pointer_bits), false)),
            _ => None,
        }
    }

    pub(super) fn add_integer_range_facts(
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

pub(super) fn integer_bounds(bits: u64, signed: bool) -> Option<(i128, i128)> {
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

pub(super) fn integer_from_bits(raw: u128, bits: u64, signed: bool) -> Option<i128> {
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
    use super::{integer_bounds, integer_from_bits};

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
}
