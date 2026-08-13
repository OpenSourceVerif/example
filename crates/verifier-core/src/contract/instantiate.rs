use std::{error::Error, fmt::{self, Display}};

use hashbrown::HashMap;
use smallvec::SmallVec;

use crate::{Builder, Context, Declaration, DefStore, Environment, Sort, Term, TermKind, Var};

use super::Clause;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The target of one source-environment declaration during instantiation.
pub enum Actual {
    Value(Term),
    Function(Var),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstantiateError<B> {
    Missing(Var),
    Target(Var),
    Unbound(B),
    Kind(Var),
    Sort { var: Var, expected: Sort, actual: Sort },
    Signature(Var),
}

impl<B: Display> Display for InstantiateError<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(var) => write!(f, "missing declaration for variable {var:?}"),
            Self::Target(var) => write!(f, "missing target declaration for variable {var:?}"),
            Self::Unbound(binding) => write!(f, "no value for {binding}"),
            Self::Kind(var) => write!(f, "variable {var:?} has the wrong declaration kind"),
            Self::Sort { var, expected, actual } => {
                write!(f, "variable {var:?} expects {expected:?}, found {actual:?}")
            }
            Self::Signature(var) => write!(f, "function {var:?} has a different signature"),
        }
    }
}

impl<B: fmt::Debug + Display> Error for InstantiateError<B> {}

pub fn instantiate<B: Copy, T>(
    cx: &mut Context,
    clause: &Clause<B>,
    target: &mut Environment<T>,
    mut get: impl FnMut(B) -> Option<Actual>,
) -> Result<Term, InstantiateError<B>> {
    let mut builder = cx.builder(target);
    let mut terms = HashMap::new();
    let mut actuals = HashMap::new();
    visit(&mut builder, clause, clause.term, &mut get, &mut terms, &mut actuals)
}

fn actual<B: Copy>(
    clause: &Clause<B>,
    var: Var,
    get: &mut impl FnMut(B) -> Option<Actual>,
    actuals: &mut HashMap<Var, Actual>,
) -> Result<(Declaration, Actual), InstantiateError<B>> {
    let (declaration, binding) =
        clause.environment.get(var).ok_or(InstantiateError::Missing(var))?;
    let value = if let Some(value) = actuals.get(&var) {
        *value
    } else {
        let value = get(*binding).ok_or(InstantiateError::Unbound(*binding))?;
        actuals.insert(var, value);
        value
    };
    Ok((declaration.clone(), value))
}

fn visit<B: Copy, T>(
    builder: &mut Builder<'_, T>,
    clause: &Clause<B>,
    term: Term,
    get: &mut impl FnMut(B) -> Option<Actual>,
    terms: &mut HashMap<Term, Term>,
    actuals: &mut HashMap<Var, Actual>,
) -> Result<Term, InstantiateError<B>> {
    if let Some(term) = terms.get(&term) {
        return Ok(*term);
    }
    let result = match builder.context().get(term).kind {
        TermKind::Var(var) => {
            let (declaration, actual) = actual(clause, var, get, actuals)?;
            let (Declaration::Value(expected), Actual::Value(term)) = (declaration, actual) else {
                return Err(InstantiateError::Kind(var));
            };
            let found = builder.term_sort(term);
            if found != expected {
                return Err(InstantiateError::Sort { var, expected, actual: found });
            }
            term
        }
        TermKind::Const(_) | TermKind::Bool(_) | TermKind::Unit => {
            builder.term_sort(term);
            term
        }
        TermKind::Unary { op, expr } => {
            let expr = visit(builder, clause, expr, get, terms, actuals)?;
            builder.unary(op, expr)
        }
        TermKind::Binary { op, lhs, rhs } => {
            let lhs = visit(builder, clause, lhs, get, terms, actuals)?;
            let rhs = visit(builder, clause, rhs, get, terms, actuals)?;
            builder.binary(op, lhs, rhs)
        }
        TermKind::Call { function, arguments } => {
            let arguments: SmallVec<[_; 4]> = arguments.into();
            let (declaration, actual) = actual(clause, function, get, actuals)?;
            let (Declaration::Function { domain, range }, Actual::Function(function)) =
                (declaration, actual)
            else {
                return Err(InstantiateError::Kind(function));
            };
            let Some((target, _)) = builder.environment().get(function) else {
                return Err(InstantiateError::Target(function));
            };
            if target != &(Declaration::Function { domain, range }) {
                return Err(InstantiateError::Signature(function));
            }
            let arguments = arguments
                .iter()
                .map(|argument| visit(builder, clause, *argument, get, terms, actuals))
                .collect::<Result<SmallVec<[_; 4]>, _>>()?;
            builder.call(function, &arguments)
        }
        TermKind::Tuple(fields) => {
            let fields: SmallVec<[_; 4]> = fields.into();
            let fields = fields
                .iter()
                .map(|field| visit(builder, clause, *field, get, terms, actuals))
                .collect::<Result<SmallVec<[_; 4]>, _>>()?;
            builder.tuple(&fields)
        }
        TermKind::Proj { tuple, field } => {
            let tuple = visit(builder, clause, tuple, get, terms, actuals)?;
            builder.proj(tuple, field)
        }
    };
    terms.insert(term, result);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{Actual, InstantiateError, instantiate};
    use crate::{Context, DefStore, Environment, TermKind, contract::Clause};

    #[test]
    fn substitutes_variables_and_renames_functions() {
        let mut cx = Context::default();
        let int = cx.int_sort();
        let mut source = Environment::new();
        let function = source.bind_function(&[int], int, 1);
        let parameter = source.bind_value(int, 2);
        let parameter = cx.builder(&mut source).var(parameter);
        let term = cx.builder(&mut source).call(function, &[parameter]);
        let clause = Clause { term, environment: source };

        let mut target = Environment::new();
        let target_function = target.bind_function(&[int], int, "f");
        let value = cx.builder(&mut target).int_lit(42);
        let term = instantiate(&mut cx, &clause, &mut target, |binding| match binding {
            1 => Some(Actual::Function(target_function)),
            2 => Some(Actual::Value(value)),
            _ => None,
        })
        .unwrap();

        assert!(matches!(
            cx.get(term).kind,
            TermKind::Call { function, .. } if function == target_function
        ));
    }

    #[test]
    fn reports_unbound_variables() {
        let mut cx = Context::default();
        let int = cx.int_sort();
        let mut source = Environment::new();
        let var = source.bind_value(int, 7);
        let term = cx.builder(&mut source).var(var);
        let clause = Clause { term, environment: source };
        let mut target = Environment::<()>::new();

        assert_eq!(
            instantiate(&mut cx, &clause, &mut target, |_| None),
            Err(InstantiateError::Unbound(7))
        );
    }
}
