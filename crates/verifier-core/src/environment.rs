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
/// different environments. The cache is derived data, so typed construction only needs `&self`.
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
    /// Terms constructed or previously checked under the environment are O(1) lookups.
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

    fn term(&self, sort: Sort, definition: TermDef<'_>) -> Term {
        let term = definition.intern();
        self.remember(term, sort);
        term
    }

    pub fn var(&self, var: Var) -> Term {
        let sort = match self.declaration(var) {
            Value(sort) => *sort,
            Function { .. } => panic!("function used as a value"),
        };
        self.term(sort, TermDef::Var(var))
    }

    pub fn int(&self, value: i128) -> Term {
        self.term(int_sort(), TermDef::Const(value))
    }

    pub fn bool(&self, value: bool) -> Term {
        self.term(bool_sort(), TermDef::Bool(value))
    }

    pub fn unit(&self) -> Term {
        self.term(unit_sort(), TermDef::Unit)
    }

    pub fn tuple(&self, fields: &[Term]) -> Term {
        if fields.is_empty() {
            return self.unit();
        }
        let sorts = fields
            .iter()
            .map(|field| self.sort(*field).expect("checked tuple field"))
            .collect::<SmallVec<[_; 4]>>();
        self.term(tuple_sort(&sorts), TermDef::Tuple(Fields::new(fields)))
    }

    pub fn proj(&self, tuple: Term, field: impl Into<Field>) -> Term {
        let field = field.into();
        def!(let definition = tuple);
        if let TermDef::Tuple(fields) = *definition {
            return fields[field.index()];
        }
        let sort = self.projection_sort(tuple, field).expect("checked tuple projection");
        self.term(sort, TermDef::Proj { tuple, field })
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

    pub fn call(&self, function: Var, arguments: &[Term]) -> Term {
        let sort = self.call_sort(function, arguments).expect("checked function call");
        self.term(sort, TermDef::Call { function, arguments: Fields::new(arguments) })
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

    pub fn binary(&self, op: Op, lhs: Term, rhs: Term) -> Term {
        let sort = self.binary_sort(op, lhs, rhs).expect("checked binary expression");
        self.term(sort, TermDef::Binary { op, lhs, rhs })
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

    pub fn unary(&self, op: Uop, expr: Term) -> Term {
        let sort = self.unary_sort(op, expr).expect("checked unary expression");
        self.term(sort, TermDef::Unary { op, expr })
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

    pub fn add(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::Add, lhs, rhs)
    }
    pub fn sub(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::Sub, lhs, rhs)
    }
    pub fn mul(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::Mul, lhs, rhs)
    }
    pub fn eq(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::Eq, lhs, rhs)
    }
    pub fn ne(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::Ne, lhs, rhs)
    }
    pub fn lt(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::Lt, lhs, rhs)
    }
    pub fn le(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::Le, lhs, rhs)
    }
    pub fn gt(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::Gt, lhs, rhs)
    }
    pub fn ge(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::Ge, lhs, rhs)
    }
    pub fn and(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::And, lhs, rhs)
    }
    pub fn or(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::Or, lhs, rhs)
    }
    pub fn implies(&self, lhs: Term, rhs: Term) -> Term {
        self.binary(Op::Implies, lhs, rhs)
    }
    pub fn not(&self, expr: Term) -> Term {
        self.unary(Uop::Not, expr)
    }
    pub fn neg(&self, expr: Term) -> Term {
        self.unary(Uop::Neg, expr)
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
