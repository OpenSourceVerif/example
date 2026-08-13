use std::fmt;

use hashbrown::HashMap;
use index_vec::IndexVec;
use smallvec::SmallVec;

use crate::{
    Context, DefStore, Field, Fields, Op, Sort, SortDef, Term, TermDef, TermKind, Uop, Var,
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// A first-order declaration. Functions are not value sorts and cannot occur as terms.
pub enum Declaration {
    Value(Sort),
    Function { domain: SmallVec<[Sort; 2]>, range: Sort },
}

/// The declarations and frontend bindings under which terms are scoped and typed.
///
/// Entries are append-only, so adding a fresh variable does not reinterpret existing terms.
/// Sorts are cached per environment because one interned term may have different sorts under
/// different environments.
pub struct Environment<B> {
    entries: IndexVec<Var, (Declaration, B)>,
    sorts: HashMap<Term, Sort>,
}

impl<B> Environment<B> {
    pub fn new() -> Self {
        Self { entries: IndexVec::new(), sorts: HashMap::new() }
    }

    pub fn bind_value(&mut self, sort: Sort, binding: B) -> Var {
        self.entries.push((Declaration::Value(sort), binding))
    }

    pub fn bind_function(&mut self, domain: &[Sort], range: Sort, binding: B) -> Var {
        self.entries.push((Declaration::Function { domain: domain.into(), range }, binding))
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
        self.sorts.get(&term).copied()
    }

    pub(crate) fn cached_sorts(&self) -> impl Iterator<Item = Sort> + '_ {
        self.sorts.values().copied()
    }

    fn remember(&mut self, term: Term, sort: Sort) {
        if let Some(previous) = self.sorts.insert(term, sort) {
            assert_eq!(previous, sort, "term has inconsistent sorts in one environment");
        }
    }
}

impl<B> Default for Environment<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Clone> Clone for Environment<B> {
    fn clone(&self) -> Self {
        Self { entries: self.entries.clone(), sorts: self.sorts.clone() }
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

/// Typed term construction under one environment.
pub struct Builder<'a, B> {
    context: &'a mut Context,
    environment: &'a mut Environment<B>,
}

impl Context {
    pub fn builder<'a, B>(&'a mut self, environment: &'a mut Environment<B>) -> Builder<'a, B> {
        Builder { context: self, environment }
    }

    pub fn int_sort(&mut self) -> Sort {
        self.intern_sort(SortDef::Int)
    }

    pub fn bool_sort(&mut self) -> Sort {
        self.intern_sort(SortDef::Bool)
    }

    pub fn unit_sort(&mut self) -> Sort {
        self.tuple_sort(&[])
    }

    pub fn tuple_sort(&mut self, fields: &[Sort]) -> Sort {
        self.intern_sort(SortDef::Tuple(Fields::new(fields)))
    }
}

macro_rules! binary_builders {
    ($($name:ident: $op:ident;)*) => {$(
        pub fn $name(&mut self, lhs: Term, rhs: Term) -> Term {
            self.binary(Op::$op, lhs, rhs)
        }
    )*};
}

impl<B> Builder<'_, B> {
    pub fn context(&self) -> &Context {
        self.context
    }

    pub fn environment(&self) -> &Environment<B> {
        self.environment
    }

    /// Returns the term's sort under this environment.
    ///
    /// Terms constructed or previously checked under the environment are expected O(1) lookups.
    pub fn term_sort(&mut self, term: Term) -> Sort {
        if let Some(sort) = self.environment.cached_sort(term) {
            return sort;
        }

        let sort = match self.context.get(term).kind {
            TermKind::Var(var) => match self.environment.declaration(var) {
                Declaration::Value(sort) => *sort,
                Declaration::Function { .. } => panic!("function used as a value"),
            },
            TermKind::Const(_) => self.context.int_sort(),
            TermKind::Bool(_) => self.context.bool_sort(),
            TermKind::Unit => self.context.unit_sort(),
            TermKind::Binary { op, lhs, rhs } => self.binary_sort(op, lhs, rhs),
            TermKind::Unary { op, expr } => self.unary_sort(op, expr),
            TermKind::Call { function, arguments } => {
                let arguments: SmallVec<[_; 4]> = arguments.into();
                self.call_sort(function, &arguments)
            }
            TermKind::Tuple(fields) => {
                let fields: SmallVec<[_; 4]> = fields.into();
                let sorts: SmallVec<[_; 4]> =
                    fields.iter().map(|field| self.term_sort(*field)).collect();
                self.context.tuple_sort(&sorts)
            }
            TermKind::Proj { tuple, field } => {
                let tuple = self.term_sort(tuple);
                match self.context.get(tuple) {
                    SortDef::Tuple(fields) => fields[field],
                    _ => panic!("projection from non-tuple term"),
                }
            }
        };
        self.environment.remember(term, sort);
        sort
    }

    fn term(&mut self, sort: Sort, kind: TermKind<'_>) -> Term {
        let term = self.context.intern_term(TermDef { kind });
        self.environment.remember(term, sort);
        term
    }

    pub fn var(&mut self, var: Var) -> Term {
        let sort = match self.environment.declaration(var) {
            Declaration::Value(sort) => *sort,
            Declaration::Function { .. } => panic!("function used as a value"),
        };
        self.term(sort, TermKind::Var(var))
    }

    pub fn int_lit(&mut self, value: i128) -> Term {
        let sort = self.context.int_sort();
        self.term(sort, TermKind::Const(value))
    }

    pub fn bool_lit(&mut self, value: bool) -> Term {
        let sort = self.context.bool_sort();
        self.term(sort, TermKind::Bool(value))
    }

    pub fn unit(&mut self) -> Term {
        let sort = self.context.unit_sort();
        self.term(sort, TermKind::Unit)
    }

    pub fn tuple(&mut self, fields: &[Term]) -> Term {
        if fields.is_empty() {
            return self.unit();
        }
        let sorts: SmallVec<[_; 4]> = fields.iter().map(|field| self.term_sort(*field)).collect();
        let sort = self.context.tuple_sort(&sorts);
        self.term(sort, TermKind::Tuple(Fields::new(fields)))
    }

    pub fn proj(&mut self, tuple: Term, field: impl Into<Field>) -> Term {
        let field = field.into();
        if let TermKind::Tuple(fields) = self.context.get(tuple).kind {
            return fields[field];
        }
        let tuple_sort = self.term_sort(tuple);
        let sort = match self.context.get(tuple_sort) {
            SortDef::Tuple(fields) => fields[field],
            _ => panic!("projection from non-tuple term"),
        };
        self.term(sort, TermKind::Proj { tuple, field })
    }

    pub fn call(&mut self, function: Var, arguments: &[Term]) -> Term {
        let sort = self.call_sort(function, arguments);
        self.term(sort, TermKind::Call { function, arguments: Fields::new(arguments) })
    }

    fn call_sort(&mut self, function: Var, arguments: &[Term]) -> Sort {
        let (domain, range) = match self.environment.declaration(function).clone() {
            Declaration::Function { domain, range } => (domain, range),
            Declaration::Value(_) => panic!("value called as a function"),
        };
        assert_eq!(arguments.len(), domain.len(), "function argument count mismatch");
        for (argument, expected) in arguments.iter().zip(domain) {
            assert_eq!(self.term_sort(*argument), expected, "function argument sort mismatch");
        }
        range
    }

    pub fn binary(&mut self, op: Op, lhs: Term, rhs: Term) -> Term {
        let sort = self.binary_sort(op, lhs, rhs);
        self.term(sort, TermKind::Binary { op, lhs, rhs })
    }

    fn binary_sort(&mut self, op: Op, lhs: Term, rhs: Term) -> Sort {
        let lhs = self.term_sort(lhs);
        let rhs = self.term_sort(rhs);
        let int = self.context.int_sort();
        let bool = self.context.bool_sort();
        match op {
            Op::Add | Op::Sub | Op::Mul => {
                assert_eq!((lhs, rhs), (int, int), "integer operation sort mismatch");
                int
            }
            Op::Eq | Op::Ne => {
                assert_eq!(lhs, rhs, "equality sort mismatch");
                bool
            }
            Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                assert_eq!((lhs, rhs), (int, int), "comparison sort mismatch");
                bool
            }
            Op::And | Op::Or | Op::Implies => {
                assert_eq!((lhs, rhs), (bool, bool), "boolean operation sort mismatch");
                bool
            }
        }
    }

    binary_builders! {
        add: Add;
        sub: Sub;
        mul: Mul;
        eq: Eq;
        ne: Ne;
        lt: Lt;
        le: Le;
        gt: Gt;
        ge: Ge;
        and: And;
        or: Or;
        implies: Implies;
    }

    pub fn unary(&mut self, op: Uop, expr: Term) -> Term {
        let sort = self.unary_sort(op, expr);
        self.term(sort, TermKind::Unary { op, expr })
    }

    fn unary_sort(&mut self, op: Uop, expr: Term) -> Sort {
        let operand = self.term_sort(expr);
        let int = self.context.int_sort();
        let bool = self.context.bool_sort();
        match op {
            Uop::Not => {
                assert_eq!(operand, bool, "boolean negation sort mismatch");
                bool
            }
            Uop::Neg => {
                assert_eq!(operand, int, "integer negation sort mismatch");
                int
            }
        }
    }

    pub fn not(&mut self, expr: Term) -> Term {
        self.unary(Uop::Not, expr)
    }

    pub fn neg(&mut self, expr: Term) -> Term {
        self.unary(Uop::Neg, expr)
    }
}
