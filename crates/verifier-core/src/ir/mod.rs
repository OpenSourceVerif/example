//! Interned symbolic term, symbol, and sort definitions.

use index_vec::{IndexSlice, define_index_type};

mod fields;

define_index_type! { pub struct Term = u32; }
define_index_type! { pub struct Sym = u32; }
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
    Param(u32),
    Sym(Sym),
    Const(i128),
    Bool(bool),
    Unit,
    Binary { op: Op, lhs: Term, rhs: Term },
    Unary { op: Uop, expr: Term },
    Call { func: Sym, arg: Term },
    Tuple(Fields<'c, Term>),
    Proj { tuple: Term, field: Field },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TermDef<'c> {
    pub sort: Sort,
    pub kind: TermKind<'c>,
}

#[derive(Debug, Clone, Copy)]
pub struct SymDef<'c> {
    pub name: &'c str,
    pub sort: Sort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortDef<'c> {
    Int,
    Bool,
    Tuple(Fields<'c, Sort>),
    Arrow(Sort, Sort),
}
