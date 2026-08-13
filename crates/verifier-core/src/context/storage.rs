use interner::{List, ListInterner};

use crate::{Field, Fields, Op, Sort, SortDef, Term, TermDef, TermKind, Uop, Var};

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
        let kind = match term.kind {
            TermKind::Var(var) => Stored::Var(var),
            TermKind::Const(value) => Stored::Const(value),
            TermKind::Bool(value) => Stored::Bool(value),
            TermKind::Unit => Stored::Unit,
            TermKind::Binary { op, lhs, rhs } => Stored::Binary { op, lhs, rhs },
            TermKind::Unary { op, expr } => Stored::Unary { op, expr },
            TermKind::Call { function, arguments } => {
                Stored::Call { function, arguments: lists.intern(arguments.as_ref()) }
            }
            TermKind::Tuple(fields) => Stored::Tuple(lists.intern(fields.as_ref())),
            TermKind::Proj { tuple, field } => Stored::Proj { tuple, field },
        };
        Self { kind }
    }

    pub(super) fn borrow(self, lists: &ListInterner<Term>) -> TermDef<'_> {
        let kind = match self.kind {
            Stored::Var(var) => TermKind::Var(var),
            Stored::Const(value) => TermKind::Const(value),
            Stored::Bool(value) => TermKind::Bool(value),
            Stored::Unit => TermKind::Unit,
            Stored::Binary { op, lhs, rhs } => TermKind::Binary { op, lhs, rhs },
            Stored::Unary { op, expr } => TermKind::Unary { op, expr },
            Stored::Call { function, arguments } => {
                TermKind::Call { function, arguments: Fields::new(&lists[arguments]) }
            }
            Stored::Tuple(fields) => TermKind::Tuple(Fields::new(&lists[fields])),
            Stored::Proj { tuple, field } => TermKind::Proj { tuple, field },
        };
        TermDef { kind }
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
