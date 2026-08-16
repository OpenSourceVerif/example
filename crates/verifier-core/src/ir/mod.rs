//! Interned symbolic term and sort definitions.

use std::{fmt, marker::PhantomData, rc::Rc};

use index_vec::{Idx, IndexSlice, define_index_type};

mod fields;

macro_rules! scoped_index {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(transparent)]
        pub struct $name {
            raw: u32,
            // Handles are meaningful only while this thread's arena is installed.
            thread: PhantomData<Rc<()>>,
        }

        impl $name {
            pub fn from_usize(value: usize) -> Self {
                assert!(u32::try_from(value).is_ok(), "index exceeds u32::MAX");
                Self { raw: value as u32, thread: PhantomData }
            }

            pub const fn index(self) -> usize {
                self.raw as usize
            }
        }

        impl Idx for $name {
            fn from_usize(value: usize) -> Self {
                Self::from_usize(value)
            }

            fn index(self) -> usize {
                self.index()
            }
        }

        impl From<usize> for $name {
            fn from(value: usize) -> Self {
                Self::from_usize(value)
            }
        }

        impl From<$name> for usize {
            fn from(value: $name) -> Self {
                value.index()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.index().fmt(f)
            }
        }
    };
}

scoped_index!(Term);
scoped_index!(Sort);
scoped_index!(Name);
define_index_type! {
    /// A stable index into an append-only [`crate::Environment`].
    pub struct Var = u32;
}
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
pub enum TermDef<'c> {
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
pub enum SortDef<'c> {
    Int,
    Bool,
    Tuple(Fields<'c, Sort>),
}
