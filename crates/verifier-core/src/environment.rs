use std::{cell::RefCell, fmt};

use hashbrown::HashMap;
use index_vec::IndexVec;
use smallvec::SmallVec;

use crate::{
    Declaration::{Function, Value},
    Field, Fields, Intern, Op, Sort, SortDef, Term, TermDef, Uop, Var, def,
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// A first-order declaration. Functions are not value sorts and cannot occur as terms.
pub enum Declaration {
    Value(Sort),
    Function { domain: SmallVec<[Sort; 2]>, range: Sort },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    UnknownVariable(Var),
    FunctionAsValue(Var),
    ValueAsFunction(Var),
    Arity { function: Var, expected: usize, actual: usize },
    Sort { expected: Sort, actual: Sort },
    Equality { lhs: Sort, rhs: Sort },
    ExpectedTuple(Sort),
    MissingField { sort: Sort, field: Field },
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownVariable(var) => write!(f, "unknown variable {var:?}"),
            Self::FunctionAsValue(var) => write!(f, "function {var:?} used as a value"),
            Self::ValueAsFunction(var) => write!(f, "value {var:?} called as a function"),
            Self::Arity { function, expected, actual } => write!(
                f,
                "function {function:?} expects {expected} arguments but received {actual}"
            ),
            Self::Sort { expected, actual } => {
                write!(f, "expected sort {expected:?}, found {actual:?}")
            }
            Self::Equality { lhs, rhs } => {
                write!(f, "equality operands have different sorts {lhs:?} and {rhs:?}")
            }
            Self::ExpectedTuple(sort) => write!(f, "expected a tuple, found sort {sort:?}"),
            Self::MissingField { sort, field } => {
                write!(f, "tuple sort {sort:?} has no field {field:?}")
            }
        }
    }
}

impl std::error::Error for TypeError {}

/// The declarations and frontend bindings under which terms are scoped and typed.
///
/// Entries are append-only, so adding a fresh variable does not reinterpret existing terms.
/// Sorts are cached per environment because one interned term may have different sorts under
/// different environments. The cache is populated only by checking interned terms.
pub struct Environment<B> {
    entries: IndexVec<Var, (Declaration, B)>,
    sorts: RefCell<HashMap<Term, Sort>>,
}

impl<B> Environment<B> {
    pub fn new() -> Self {
        Self { entries: IndexVec::new(), sorts: RefCell::new(HashMap::new()) }
    }

    pub fn bind_value(&mut self, sort: Sort, binding: B) -> Var {
        self.entries.push((Value(sort), binding))
    }

    pub fn bind_function(&mut self, domain: &[Sort], range: Sort, binding: B) -> Var {
        self.entries.push((Function { domain: domain.into(), range }, binding))
    }

    pub fn declaration(&self, var: Var) -> &Declaration {
        &self.entries[var].0
    }

    pub fn binding(&self, var: Var) -> &B {
        &self.entries[var].1
    }

    pub fn get(&self, var: Var) -> Option<(&Declaration, &B)> {
        self.entries.get(var).map(|(declaration, binding)| (declaration, binding))
    }

    pub fn iter(&self) -> impl Iterator<Item = (Var, &Declaration, &B)> {
        self.entries
            .iter_enumerated()
            .map(|(var, (declaration, binding))| (var, declaration, binding))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn cached_sort(&self, term: Term) -> Option<Sort> {
        self.sorts.borrow().get(&term).copied()
    }

    pub(crate) fn cached_sorts(&self) -> Vec<Sort> {
        self.sorts.borrow().values().copied().collect()
    }

    fn remember(&self, term: Term, sort: Sort) {
        if let Some(previous) = self.sorts.borrow_mut().insert(term, sort) {
            assert_eq!(previous, sort, "term has inconsistent sorts in one environment");
        }
    }

    /// Checks and returns a term's sort under this environment.
    ///
    /// Terms previously checked under the environment are O(1) lookups.
    pub fn sort(&self, term: Term) -> Result<Sort, TypeError> {
        if let Some(sort) = self.cached_sort(term) {
            return Ok(sort);
        }

        def!(let definition = term);
        let sort = match *definition {
            TermDef::Var(var) => match self.get(var).map(|entry| entry.0) {
                Some(Value(sort)) => *sort,
                Some(Function { .. }) => return Err(TypeError::FunctionAsValue(var)),
                None => return Err(TypeError::UnknownVariable(var)),
            },
            TermDef::Const(_) => int_sort(),
            TermDef::Bool(_) => bool_sort(),
            TermDef::Unit => unit_sort(),
            TermDef::Binary { op, lhs, rhs } => self.binary_sort(op, lhs, rhs)?,
            TermDef::Unary { op, expr } => self.unary_sort(op, expr)?,
            TermDef::Call { function, arguments } => {
                self.call_sort(function, arguments.as_ref())?
            }
            TermDef::Tuple(fields) => {
                let sorts = fields
                    .iter()
                    .map(|field| self.sort(*field))
                    .collect::<Result<SmallVec<[_; 4]>, _>>()?;
                tuple_sort(&sorts)
            }
            TermDef::Proj { tuple, field } => self.projection_sort(tuple, field)?,
        };
        self.remember(term, sort);
        Ok(sort)
    }

    fn projection_sort(&self, tuple: Term, field: Field) -> Result<Sort, TypeError> {
        let tuple = self.sort(tuple)?;
        def!(let definition = tuple);
        match *definition {
            SortDef::Tuple(fields) => fields
                .get(field.index())
                .copied()
                .ok_or(TypeError::MissingField { sort: tuple, field }),
            _ => Err(TypeError::ExpectedTuple(tuple)),
        }
    }

    fn call_sort(&self, function: Var, arguments: &[Term]) -> Result<Sort, TypeError> {
        let Some((declaration, _)) = self.get(function) else {
            return Err(TypeError::UnknownVariable(function));
        };
        let Function { domain, range } = declaration else {
            return Err(TypeError::ValueAsFunction(function));
        };
        if arguments.len() != domain.len() {
            return Err(TypeError::Arity {
                function,
                expected: domain.len(),
                actual: arguments.len(),
            });
        }
        for (argument, expected) in arguments.iter().zip(domain) {
            self.expect_sort(*argument, *expected)?;
        }
        Ok(*range)
    }

    fn binary_sort(&self, op: Op, lhs: Term, rhs: Term) -> Result<Sort, TypeError> {
        let lhs = self.sort(lhs)?;
        let rhs = self.sort(rhs)?;
        let int = int_sort();
        let bool = bool_sort();
        match op {
            Op::Add | Op::Sub | Op::Mul | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                expect(lhs, int)?;
                expect(rhs, int)?;
                Ok(if matches!(op, Op::Add | Op::Sub | Op::Mul) { int } else { bool })
            }
            Op::Eq | Op::Ne => {
                if lhs != rhs {
                    return Err(TypeError::Equality { lhs, rhs });
                }
                Ok(bool)
            }
            Op::And | Op::Or | Op::Implies => {
                expect(lhs, bool)?;
                expect(rhs, bool)?;
                Ok(bool)
            }
        }
    }

    fn unary_sort(&self, op: Uop, expr: Term) -> Result<Sort, TypeError> {
        let operand = self.sort(expr)?;
        let expected = match op {
            Uop::Not => bool_sort(),
            Uop::Neg => int_sort(),
        };
        expect(operand, expected)?;
        Ok(expected)
    }

    fn expect_sort(&self, term: Term, expected: Sort) -> Result<(), TypeError> {
        expect(self.sort(term)?, expected)
    }
}

impl<B> Default for Environment<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Clone> Clone for Environment<B> {
    fn clone(&self) -> Self {
        Self { entries: self.entries.clone(), sorts: RefCell::new(self.sorts.borrow().clone()) }
    }
}

impl<B: fmt::Debug> fmt::Debug for Environment<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.entries).finish()
    }
}

impl<B: PartialEq> PartialEq for Environment<B> {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl<B: Eq> Eq for Environment<B> {}

fn int_sort() -> Sort {
    SortDef::Int.intern()
}

fn bool_sort() -> Sort {
    SortDef::Bool.intern()
}

fn unit_sort() -> Sort {
    tuple_sort(&[])
}

fn tuple_sort(fields: &[Sort]) -> Sort {
    SortDef::Tuple(Fields::new(fields)).intern()
}

fn expect(actual: Sort, expected: Sort) -> Result<(), TypeError> {
    if actual == expected { Ok(()) } else { Err(TypeError::Sort { expected, actual }) }
}

#[cfg(test)]
mod tests {
    use crate::{
        Environment, INTERNERS, Intern, Interners, Op, SortDef, TypeError,
        term::{binary, bool},
    };

    #[test]
    fn only_checked_terms_are_cached() {
        let interners = Interners::default();
        let body = || {
            let env = Environment::<()>::new();
            let yes = bool(true);
            let invalid = binary(Op::Add, yes, yes);
            let int_sort = SortDef::Int.intern();
            let bool_sort = SortDef::Bool.intern();

            assert_eq!(env.cached_sort(invalid), None);
            assert_eq!(
                env.sort(invalid),
                Err(TypeError::Sort { expected: int_sort, actual: bool_sort })
            );
            assert_eq!(env.cached_sort(invalid), None);
            assert_eq!(env.cached_sort(yes), Some(bool_sort));
        };
        // SAFETY: `body` is synchronous and discards all arena values.
        unsafe { INTERNERS.set(&interners, body) }
    }
}
