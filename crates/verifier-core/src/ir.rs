//! Interned symbolic term, symbol, and sort definitions.

use index_vec::define_index_type;

define_index_type! { pub struct Term = u32; }
define_index_type! { pub struct Sym = u32; }
define_index_type! { pub struct Sort = u32; }
define_index_type! { pub struct Name = u32; }

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TermDef {
    Sym(Sym),
    Const(i128),
    Bool(bool),
    Unit,
    Binary { op: Op, lhs: Term, rhs: Term },
    Unary { op: Uop, expr: Term },
    Call { func: Sym, arg: Term },
    Tuple(Box<[Term]>),
}

#[derive(Debug, Clone, Copy)]
pub struct SymDef<'c> {
    pub name: &'c str,
    pub sort: Sort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymDefInterned {
    pub name: Name,
    pub sort: Sort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortDef {
    Int,
    Bool,
    Arrow(Sort, Sort),
}
