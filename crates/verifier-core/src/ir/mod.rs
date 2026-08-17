//! Interned symbolic term and sort definitions.

use index_vec::{IndexSlice, define_index_type};
use interner::{Covariant, Interned};

mod fields;

/// The identity of an interned term definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Term(pub(crate) Interned<TermDef<'static>>);

/// The identity of an interned term definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sort(pub(crate) Interned<SortDef<'static>>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// The identity of an interned string.
pub struct Name(pub(crate) Interned<&'static str>);

define_index_type! {
    /// A stable index into an append-only [`crate::Environment`].
    pub struct Var = u32;
}
define_index_type! { pub struct Field = u32; }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fields<'a, T>(&'a IndexSlice<Field, [T]>);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Covariant)]
pub enum TermDef<'a> {
    Var(Var),
    Const(i128),
    Bool(bool),
    Unit,
    Binary { op: Op, lhs: Term, rhs: Term },
    Unary { op: Uop, expr: Term },
    Call { function: Var, arguments: Fields<'a, Term> },
    Tuple(Fields<'a, Term>),
    Proj { tuple: Term, field: Field },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Covariant)]
pub enum SortDef<'a> {
    Int,
    Bool,
    Tuple(Fields<'a, Sort>),
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::{Name, Sort, Term};

    #[test]
    fn handles_are_one_pointer() {
        assert_eq!(size_of::<Term>(), size_of::<usize>());
        assert_eq!(size_of::<Sort>(), size_of::<usize>());
        assert_eq!(size_of::<Name>(), size_of::<usize>());
    }
}
