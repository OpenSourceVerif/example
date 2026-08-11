use rustc_middle::ty::{IntTy, Ty, TyCtxt, TyKind, UintTy};
use verifier_core::{Context, Sort};

pub(crate) trait RustcTy<'tcx> {
    fn sort(&mut self, tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Option<Sort>;
}

impl<'tcx> RustcTy<'tcx> for Context {
    fn sort(&mut self, tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Option<Sort> {
        if let TyKind::Tuple(fields) = ty.kind() {
            let sorts =
                fields.iter().map(|field| self.sort(tcx, field)).collect::<Option<Vec<_>>>()?;
            Some(self.tuple_sort(&sorts))
        } else if ty.is_bool() {
            Some(self.bool_sort())
        } else if integer_layout(tcx, ty).and_then(|layout| integer_bounds(layout)).is_some() {
            Some(self.int_sort())
        } else {
            None
        }
    }
}

pub(crate) fn integer_layout(tcx: TyCtxt<'_>, ty: Ty<'_>) -> Option<(u64, bool)> {
    let pointer_bits = tcx.data_layout.pointer_size().bits();
    match ty.kind() {
        TyKind::Int(kind) => Some((int_width(*kind, pointer_bits), true)),
        TyKind::Uint(kind) => Some((uint_width(*kind, pointer_bits), false)),
        _ => None,
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

pub(crate) fn integer_bounds((bits, signed): (u64, bool)) -> Option<(i128, i128)> {
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

pub(crate) fn integer_from_bits(raw: u128, (bits, signed): (u64, bool)) -> Option<i128> {
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
        assert_eq!(integer_from_bits(0xff, (8, true)), Some(-1));
        assert_eq!(integer_from_bits(0x80, (8, true)), Some(-128));
        assert_eq!(integer_from_bits(0x7f, (8, true)), Some(127));
        assert_eq!(integer_from_bits(u128::MAX, (128, true)), Some(-1));
    }

    #[test]
    fn rejects_unsigned_values_outside_the_term_constant_domain() {
        assert_eq!(integer_bounds((128, false)), None);
        assert_eq!(integer_from_bits(i128::MAX as u128 + 1, (128, false)), None);
    }
}
