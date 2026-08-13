//! Interned symbolic term and sort definitions.

use index_vec::{IndexSlice, define_index_type};

mod fields;

define_index_type! { pub struct Term = u32; }
define_index_type! {
    /// A stable index into an append-only [`crate::Environment`].
    pub struct Var = u32;
}
define_index_type! { pub struct Sort = u32; }
define_index_type! { pub struct Name = u32; }
define_index_type! { pub struct Field = u32; }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fields<'c, T>(&'c IndexSlice<Field, [T]>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Implies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Uop {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TermKind<'c> {
    Var(Var),
    Const(i128),
    Bool(bool),
    Unit,
    Binary { op: Op, lhs: Term, rhs: Term },
    Unary { op: Uop, expr: Term },
    Call { function: Var, arguments: Fields<'c, Term> },
    Tuple(Fields<'c, Term>),
    Proj { tuple: Term, field: Field },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TermDef<'c> {
    pub kind: TermKind<'c>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortDef<'c> {
    Int,
    Bool,
    Tuple(Fields<'c, Sort>),
}
