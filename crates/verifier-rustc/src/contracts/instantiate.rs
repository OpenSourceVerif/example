use smallvec::SmallVec;
use verifier_core::{Context, DefStore, Term, TermKind};

use super::{Clause, Source};

pub(crate) fn instantiate(
    context: &mut Context,
    clause: &Clause,
    mut value: impl FnMut(Source) -> Option<Term>,
) -> Result<Term, String> {
    instantiate_term(context, clause.term, &clause.sources, &mut value)
}

fn instantiate_term(
    context: &mut Context,
    template: Term,
    sources: &[Source],
    value: &mut impl FnMut(Source) -> Option<Term>,
) -> Result<Term, String> {
    let def = context.get(template);
    match def.kind {
        TermKind::Param(index) => {
            let source = sources
                .get(index as usize)
                .copied()
                .ok_or_else(|| format!("term refers to missing variable {index}"))?;
            let value = value(source).ok_or_else(|| match source {
                Source::Local(local) => format!("no value for local {local:?}"),
                Source::Result => "no value for `result`".to_owned(),
            })?;
            if context.term_sort(value) != def.sort {
                return Err(format!("value for term parameter {index} has the wrong sort"));
            }
            Ok(value)
        }
        TermKind::Sym(_) | TermKind::Const(_) | TermKind::Bool(_) | TermKind::Unit => Ok(template),
        TermKind::Unary { op, expr } => {
            let expr = instantiate_term(context, expr, sources, value)?;
            Ok(context.unary(op, expr))
        }
        TermKind::Binary { op, lhs, rhs } => {
            let lhs = instantiate_term(context, lhs, sources, value)?;
            let rhs = instantiate_term(context, rhs, sources, value)?;
            Ok(context.binary(op, lhs, rhs))
        }
        TermKind::Call { func, arg } => {
            let arg = instantiate_term(context, arg, sources, value)?;
            Ok(context.call(func, arg))
        }
        TermKind::Tuple(fields) => {
            // copy to 1. workaround borrow checker; 2. scratch to interning a new tuple in context.
            let mut fields: SmallVec<[_; 4]> = fields.into();

            for field in &mut fields {
                *field = instantiate_term(context, *field, sources, value)?;
            }

            Ok(context.tuple(&fields))
        }
        TermKind::Proj { tuple, field } => {
            let tuple = instantiate_term(context, tuple, sources, value)?;
            Ok(context.proj(tuple, field))
        }
    }
}

#[cfg(test)]
mod tests {
    use rustc_middle::mir::Local;
    use rustc_span::DUMMY_SP;
    use smallvec::smallvec;
    use verifier_core::{Context, DefStore, TermKind};

    use super::{Clause, Source, instantiate};

    #[test]
    fn substitutes_variables_and_preserves_callee() {
        let mut context = Context::default();
        let int = context.int_sort();
        let function_sort = context.arrow(int, int);
        let function = context.symbol("f", function_sort);
        let variable = context.param(0, int);
        let call = context.call(function, variable);
        let clause = Clause {
            term: call,
            span: DUMMY_SP,
            sources: smallvec![Source::Local(Local::from_usize(1))],
        };
        let value = context.int_lit(42);

        let instantiated = instantiate(&mut context, &clause, |source| match source {
            Source::Local(_) => Some(value),
            Source::Result => None,
        })
        .unwrap();

        assert_eq!(context.get(instantiated).kind, TermKind::Call { func: function, arg: value });
    }
}
