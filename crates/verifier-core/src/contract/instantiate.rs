use std::{
    error::Error,
    fmt::{self, Display},
};

use hashbrown::HashMap;
use smallvec::SmallVec;

use crate::{
    Declaration, DefStore, Environment, INTERNERS, Sort, Term, TermDef, TypeError, Var, scoped,
};

use super::Clause;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The target of one source-environment declaration during instantiation.
pub enum Actual {
    Value(Term),
    Function(Var),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstantiateError<B> {
    Missing(Var),
    Target(Var),
    Unbound(B),
    Kind(Var),
    Sort { var: Var, expected: Sort, actual: Sort },
    Signature(Var),
    Invalid(TypeError),
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
            Self::Invalid(error) => write!(f, "invalid target term: {error}"),
        }
    }
}

impl<B: fmt::Debug + Display> Error for InstantiateError<B> {}

pub fn instantiate<B: Copy, T>(
    clause: &Clause<B>,
    target: &Environment<T>,
    mut get: impl FnMut(B) -> Option<Actual>,
) -> Result<Term, InstantiateError<B>> {
    let mut terms = HashMap::new();
    let mut actuals = HashMap::new();
    visit(target, clause, clause.term, &mut get, &mut terms, &mut actuals)
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
    target: &Environment<T>,
    clause: &Clause<B>,
    term: Term,
    get: &mut impl FnMut(B) -> Option<Actual>,
    terms: &mut HashMap<Term, Term>,
    actuals: &mut HashMap<Var, Actual>,
) -> Result<Term, InstantiateError<B>> {
    if let Some(term) = terms.get(&term) {
        return Ok(*term);
    }
    let result = match owned_term(term) {
        OwnedTerm::Var(var) => {
            let (declaration, actual) = actual(clause, var, get, actuals)?;
            let (Declaration::Value(expected), Actual::Value(term)) = (declaration, actual) else {
                return Err(InstantiateError::Kind(var));
            };
            let found = target.sort(term).map_err(InstantiateError::Invalid)?;
            if found != expected {
                return Err(InstantiateError::Sort { var, expected, actual: found });
            }
            term
        }
        OwnedTerm::Const | OwnedTerm::Bool | OwnedTerm::Unit => {
            target.sort(term).map_err(InstantiateError::Invalid)?;
            term
        }
        OwnedTerm::Unary { op, expr } => {
            let expr = visit(target, clause, expr, get, terms, actuals)?;
            target.unary(op, expr)
        }
        OwnedTerm::Binary { op, lhs, rhs } => {
            let lhs = visit(target, clause, lhs, get, terms, actuals)?;
            let rhs = visit(target, clause, rhs, get, terms, actuals)?;
            target.binary(op, lhs, rhs)
        }
        OwnedTerm::Call { function, arguments } => {
            let (declaration, actual) = actual(clause, function, get, actuals)?;
            let (Declaration::Function { domain, range }, Actual::Function(function)) =
                (declaration, actual)
            else {
                return Err(InstantiateError::Kind(function));
            };
            let Some((target_declaration, _)) = target.get(function) else {
                return Err(InstantiateError::Target(function));
            };
            if target_declaration != &(Declaration::Function { domain, range }) {
                return Err(InstantiateError::Signature(function));
            }
            let arguments = arguments
                .iter()
                .map(|argument| visit(target, clause, *argument, get, terms, actuals))
                .collect::<Result<SmallVec<[_; 4]>, _>>()?;
            target.call(function, &arguments)
        }
        OwnedTerm::Tuple(fields) => {
            let fields = fields
                .iter()
                .map(|field| visit(target, clause, *field, get, terms, actuals))
                .collect::<Result<SmallVec<[_; 4]>, _>>()?;
            target.tuple(&fields)
        }
        OwnedTerm::Proj { tuple, field } => {
            let tuple = visit(target, clause, tuple, get, terms, actuals)?;
            target.proj(tuple, field)
        }
    };
    terms.insert(term, result);
    Ok(result)
}

enum OwnedTerm {
    Var(Var),
    Const,
    Bool,
    Unit,
    Unary { op: crate::Uop, expr: Term },
    Binary { op: crate::Op, lhs: Term, rhs: Term },
    Call { function: Var, arguments: SmallVec<[Term; 4]> },
    Tuple(SmallVec<[Term; 4]>),
    Proj { tuple: Term, field: crate::Field },
}

fn owned_term(term: Term) -> OwnedTerm {
    scoped!(let interners = INTERNERS);
    let interners = interners.borrow();
    match interners.get(term) {
        TermDef::Var(var) => OwnedTerm::Var(var),
        TermDef::Const(_) => OwnedTerm::Const,
        TermDef::Bool(_) => OwnedTerm::Bool,
        TermDef::Unit => OwnedTerm::Unit,
        TermDef::Unary { op, expr } => OwnedTerm::Unary { op, expr },
        TermDef::Binary { op, lhs, rhs } => OwnedTerm::Binary { op, lhs, rhs },
        TermDef::Call { function, arguments } => {
            OwnedTerm::Call { function, arguments: arguments.into() }
        }
        TermDef::Tuple(fields) => OwnedTerm::Tuple(fields.into()),
        TermDef::Proj { tuple, field } => OwnedTerm::Proj { tuple, field },
    }
}

#[cfg(test)]
mod tests {
    use super::{Actual, InstantiateError, instantiate};
    use crate::{
        DefStore, Environment, INTERNERS, Intern, SortDef, TermDef, contract::Clause, scope, scoped,
    };

    #[test]
    fn substitutes_variables_and_renames_functions() {
        // SAFETY: this test is synchronous.
        unsafe {
            scope(|| {
                let int = SortDef::Int.intern();
                let mut source = Environment::new();
                let function = source.bind_function(&[int], int, 1);
                let parameter = source.bind_value(int, 2);
                let parameter = source.var(parameter);
                let term = source.call(function, &[parameter]);
                let clause = Clause { term, environment: source };

                let mut target = Environment::new();
                let target_function = target.bind_function(&[int], int, "f");
                let value = target.int(42);
                let term = instantiate(&clause, &target, |binding| match binding {
                    1 => Some(Actual::Function(target_function)),
                    2 => Some(Actual::Value(value)),
                    _ => None,
                })
                .unwrap();

                scoped!(let interners = INTERNERS);
                assert!(matches!(
                    interners.borrow().get(term),
                    TermDef::Call { function, .. } if function == target_function
                ));
            })
        }
    }

    #[test]
    fn reports_unbound_variables() {
        // SAFETY: this test is synchronous.
        unsafe {
            scope(|| {
                let int = SortDef::Int.intern();
                let mut source = Environment::new();
                let var = source.bind_value(int, 7);
                let term = source.var(var);
                let clause = Clause { term, environment: source };
                let target = Environment::<()>::new();
                assert_eq!(
                    instantiate(&clause, &target, |_| None),
                    Err(InstantiateError::Unbound(7))
                );
            })
        }
    }
}
