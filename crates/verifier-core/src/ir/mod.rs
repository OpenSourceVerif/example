//! Interned symbolic term and sort definitions.

use std::{fmt, marker::PhantomData, ptr::NonNull, rc::Rc};

use index_vec::{IndexSlice, define_index_type};

mod fields;

#[derive(Debug)]
pub(crate) struct NameDef {
    pub(crate) text: &'static str,
}

macro_rules! pointer_handle {
    ($name:ident, $definition:ty) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(transparent)]
        pub struct $name {
            pointer: NonNull<$definition>,
            // Handles belong to the current thread's installed arena.
            thread: PhantomData<Rc<()>>,
        }

        impl $name {
            pub(crate) fn new(pointer: NonNull<$definition>) -> Self {
                Self { pointer, thread: PhantomData }
            }

            pub(crate) const fn pointer(self) -> NonNull<$definition> {
                self.pointer
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.debug_tuple(stringify!($name)).field(&self.pointer).finish()
            }
        }
    };
}

pointer_handle!(Term, TermDef<'static>);
pointer_handle!(Sort, SortDef<'static>);
pointer_handle!(Name, NameDef);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
