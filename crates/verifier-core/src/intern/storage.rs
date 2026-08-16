use interner::{List, ListInterner};

use crate::{Field, Fields, Op, Sort, SortDef, Term, TermDef, Uop, Var};

use TermKindStored as Stored;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum TermKindStored {
    Var(Var),
    Const(i128),
    Bool(bool),
    Unit,
    Binary { op: Op, lhs: Term, rhs: Term },
    Unary { op: Uop, expr: Term },
    Call { function: Var, arguments: List<Term> },
    Tuple(List<Term>),
    Proj { tuple: Term, field: Field },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct TermDefStored {
    pub kind: TermKindStored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum SortDefStored {
    Int,
    Bool,
    Tuple(List<Sort>),
}

impl TermDefStored {
    pub(super) fn store(term: TermDef<'_>, lists: &mut ListInterner<Term>) -> Self {
        let kind = match term {
            TermDef::Var(var) => Stored::Var(var),
            TermDef::Const(value) => Stored::Const(value),
            TermDef::Bool(value) => Stored::Bool(value),
            TermDef::Unit => Stored::Unit,
            TermDef::Binary { op, lhs, rhs } => Stored::Binary { op, lhs, rhs },
            TermDef::Unary { op, expr } => Stored::Unary { op, expr },
            TermDef::Call { function, arguments } => {
                Stored::Call { function, arguments: lists.intern(arguments.as_ref()) }
            }
            TermDef::Tuple(fields) => Stored::Tuple(lists.intern(fields.as_ref())),
            TermDef::Proj { tuple, field } => Stored::Proj { tuple, field },
        };
        Self { kind }
    }

    pub(super) fn borrow(self, lists: &ListInterner<Term>) -> TermDef<'_> {
        let kind = match self.kind {
            Stored::Var(var) => TermDef::Var(var),
            Stored::Const(value) => TermDef::Const(value),
            Stored::Bool(value) => TermDef::Bool(value),
            Stored::Unit => TermDef::Unit,
            Stored::Binary { op, lhs, rhs } => TermDef::Binary { op, lhs, rhs },
            Stored::Unary { op, expr } => TermDef::Unary { op, expr },
            Stored::Call { function, arguments } => {
                TermDef::Call { function, arguments: Fields::new(&lists[arguments]) }
            }
            Stored::Tuple(fields) => TermDef::Tuple(Fields::new(&lists[fields])),
            Stored::Proj { tuple, field } => TermDef::Proj { tuple, field },
        };
        kind
    }
}

impl SortDefStored {
    pub(super) fn store(sort: SortDef<'_>, lists: &mut ListInterner<Sort>) -> Self {
        match sort {
            SortDef::Int => Self::Int,
            SortDef::Bool => Self::Bool,
            SortDef::Tuple(fields) => Self::Tuple(lists.intern(fields.as_ref())),
        }
    }

    pub(super) fn borrow(self, lists: &ListInterner<Sort>) -> SortDef<'_> {
        match self {
            Self::Int => SortDef::Int,
            Self::Bool => SortDef::Bool,
            Self::Tuple(fields) => SortDef::Tuple(Fields::new(&lists[fields])),
        }
    }
}
