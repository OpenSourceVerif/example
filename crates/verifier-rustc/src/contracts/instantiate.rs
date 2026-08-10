use smallvec::SmallVec;
use verifier_core::{Context, DefStore, Term, TermDef};

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
    match context.get(template) {
        TermDef::Var(index) => {
            let source = sources
                .get(index)
                .copied()
                .ok_or_else(|| format!("term refers to missing variable {index}"))?;
            value(source).ok_or_else(|| match source {
                Source::Local(local) => format!("no value for local {local:?}"),
                Source::Result => "no value for `result`".to_owned(),
            })
        }
        TermDef::Sym(_) | TermDef::Const(_) | TermDef::Bool(_) | TermDef::Unit => Ok(template),
        TermDef::Unary { op, expr } => {
            let expr = instantiate_term(context, expr, sources, value)?;
            Ok(context.unary(op, expr))
        }
        TermDef::Binary { op, lhs, rhs } => {
            let lhs = instantiate_term(context, lhs, sources, value)?;
            let rhs = instantiate_term(context, rhs, sources, value)?;
            Ok(context.binary(op, lhs, rhs))
        }
        TermDef::Call { func, arg } => {
            let arg = instantiate_term(context, arg, sources, value)?;
            Ok(context.call(func, arg))
        }
        TermDef::Tuple(fields) => {
            // copy to 1. workaround borrow checker; 2. scratch to interning a new tuple in context.
            let mut fields = SmallVec::<[_; 4]>::from_slice(fields);

            for field in &mut fields {
                *field = instantiate_term(context, *field, sources, value)?;
            }

            Ok(context.tuple(&fields))
        }
    }
}

#[cfg(test)]
mod tests {
    use rustc_middle::mir::Local;
    use rustc_span::DUMMY_SP;
    use smallvec::smallvec;
    use verifier_core::{Context, DefStore, TermDef};

    use super::{Clause, Source, instantiate};

    #[test]
    fn substitutes_variables_and_preserves_callee() {
        let mut context = Context::default();
        let int = context.int_sort();
        let function_sort = context.arrow(int, int);
        let function = context.symbol("f", function_sort);
        let variable = context.var(0);
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

        assert_eq!(context.get(instantiated), TermDef::Call { func: function, arg: value });
    }
}
