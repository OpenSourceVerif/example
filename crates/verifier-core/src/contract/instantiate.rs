use smallvec::SmallVec;

use crate::{Context, DefStore, Sort, Term, TermKind};

use super::Clause;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstantiateError<B> {
    Missing(u32),
    Unbound(B),
    Sort { param: u32, expected: Sort, actual: Sort },
}

pub fn instantiate<B: Copy>(
    cx: &mut Context,
    clause: &Clause<B>,
    mut get: impl FnMut(B) -> Option<Term>,
) -> Result<Term, InstantiateError<B>> {
    visit(cx, clause.term, &clause.bindings, &mut get)
}

fn visit<B: Copy>(
    cx: &mut Context,
    term: Term,
    bindings: &[B],
    get: &mut impl FnMut(B) -> Option<Term>,
) -> Result<Term, InstantiateError<B>> {
    let def = cx.get(term);
    match def.kind {
        TermKind::Param(param) => {
            let binding =
                bindings.get(param as usize).copied().ok_or(InstantiateError::Missing(param))?;
            let term = get(binding).ok_or(InstantiateError::Unbound(binding))?;
            let actual = cx.term_sort(term);
            if actual != def.sort {
                return Err(InstantiateError::Sort { param, expected: def.sort, actual });
            }
            Ok(term)
        }
        TermKind::Sym(_) | TermKind::Const(_) | TermKind::Bool(_) | TermKind::Unit => Ok(term),
        TermKind::Unary { op, expr } => {
            let expr = visit(cx, expr, bindings, get)?;
            Ok(cx.unary(op, expr))
        }
        TermKind::Binary { op, lhs, rhs } => {
            let lhs = visit(cx, lhs, bindings, get)?;
            let rhs = visit(cx, rhs, bindings, get)?;
            Ok(cx.binary(op, lhs, rhs))
        }
        TermKind::Call { func, arg } => {
            let arg = visit(cx, arg, bindings, get)?;
            Ok(cx.call(func, arg))
        }
        TermKind::Tuple(fields) => {
            let mut fields: SmallVec<[_; 4]> = fields.into();
            for field in &mut fields {
                *field = visit(cx, *field, bindings, get)?;
            }
            Ok(cx.tuple(&fields))
        }
        TermKind::Proj { tuple, field } => {
            let tuple = visit(cx, tuple, bindings, get)?;
            Ok(cx.proj(tuple, field))
        }
    }
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;

    use super::{InstantiateError, instantiate};
    use crate::{Context, DefStore, TermKind, contract::Clause};

    #[test]
    fn substitutes_parameters_and_preserves_callees() {
        let mut cx = Context::default();
        let int = cx.int_sort();
        let function = cx.arrow(int, int);
        let function = cx.symbol("f", function);
        let param = cx.param(0, int);
        let clause = Clause { term: cx.call(function, param), bindings: smallvec![7] };
        let value = cx.int_lit(42);

        let term =
            instantiate(&mut cx, &clause, |binding| (binding == 7).then_some(value)).unwrap();

        assert_eq!(cx.get(term).kind, TermKind::Call { func: function, arg: value });
    }

    #[test]
    fn reports_unbound_parameters() {
        let mut cx = Context::default();
        let int = cx.int_sort();
        let clause = Clause { term: cx.param(0, int), bindings: smallvec![7] };

        assert_eq!(instantiate(&mut cx, &clause, |_| None), Err(InstantiateError::Unbound(7)));
    }
}
