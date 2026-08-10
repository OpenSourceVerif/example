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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TermDef<Fields> {
    Sym(Sym),
    Const(i128),
    Bool(bool),
    Unit,
    Binary { op: Op, lhs: Term, rhs: Term },
    Unary { op: Uop, expr: Term },
    Call { func: Sym, arg: Term },
    Tuple(Fields),
}

impl<Fields> TermDef<Fields> {
    pub(crate) fn map_fields<Mapped>(self, map: impl FnOnce(Fields) -> Mapped) -> TermDef<Mapped> {
        match self {
            TermDef::Sym(sym) => TermDef::Sym(sym),
            TermDef::Const(value) => TermDef::Const(value),
            TermDef::Bool(value) => TermDef::Bool(value),
            TermDef::Unit => TermDef::Unit,
            TermDef::Binary { op, lhs, rhs } => TermDef::Binary { op, lhs, rhs },
            TermDef::Unary { op, expr } => TermDef::Unary { op, expr },
            TermDef::Call { func, arg } => TermDef::Call { func, arg },
            TermDef::Tuple(fields) => TermDef::Tuple(map(fields)),
        }
    }
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
