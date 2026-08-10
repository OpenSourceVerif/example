use interner::{List, ListInterner};

use crate::{Field, Fields, Name, Op, Sort, SortDef, Sym, Term, TermDef, TermKind, Uop};

use TermKindStored as Stored;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum TermKindStored {
    Param(u32),
    Sym(Sym),
    Const(i128),
    Bool(bool),
    Unit,
    Binary { op: Op, lhs: Term, rhs: Term },
    Unary { op: Uop, expr: Term },
    Call { func: Sym, arg: Term },
    Tuple(List<Term>),
    Proj { tuple: Term, field: Field },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct TermDefStored {
    pub sort: Sort,
    pub kind: TermKindStored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum SortDefStored {
    Int,
    Bool,
    Tuple(List<Sort>),
    Arrow(Sort, Sort),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct SymDefStored {
    pub name: Name,
    pub sort: Sort,
}

impl TermDefStored {
    pub(super) fn store(term: TermDef<'_>, lists: &mut ListInterner<Term>) -> Self {
        let kind = match term.kind {
            TermKind::Param(index) => Stored::Param(index),
            TermKind::Sym(sym) => Stored::Sym(sym),
            TermKind::Const(value) => Stored::Const(value),
            TermKind::Bool(value) => Stored::Bool(value),
            TermKind::Unit => Stored::Unit,
            TermKind::Binary { op, lhs, rhs } => Stored::Binary { op, lhs, rhs },
            TermKind::Unary { op, expr } => Stored::Unary { op, expr },
            TermKind::Call { func, arg } => Stored::Call { func, arg },
            TermKind::Tuple(fields) => Stored::Tuple(lists.intern(fields.as_ref())),
            TermKind::Proj { tuple, field } => Stored::Proj { tuple, field },
        };
        Self { sort: term.sort, kind }
    }

    pub(super) fn borrow(self, lists: &ListInterner<Term>) -> TermDef<'_> {
        let kind = match self.kind {
            Stored::Param(index) => TermKind::Param(index),
            Stored::Sym(sym) => TermKind::Sym(sym),
            Stored::Const(value) => TermKind::Const(value),
            Stored::Bool(value) => TermKind::Bool(value),
            Stored::Unit => TermKind::Unit,
            Stored::Binary { op, lhs, rhs } => TermKind::Binary { op, lhs, rhs },
            Stored::Unary { op, expr } => TermKind::Unary { op, expr },
            Stored::Call { func, arg } => TermKind::Call { func, arg },
            Stored::Tuple(fields) => TermKind::Tuple(Fields::new(&lists[fields])),
            Stored::Proj { tuple, field } => TermKind::Proj { tuple, field },
        };
        TermDef { sort: self.sort, kind }
    }
}

impl SortDefStored {
    pub(super) fn store(sort: SortDef<'_>, lists: &mut ListInterner<Sort>) -> Self {
        match sort {
            SortDef::Int => Self::Int,
            SortDef::Bool => Self::Bool,
            SortDef::Tuple(fields) => Self::Tuple(lists.intern(fields.as_ref())),
            SortDef::Arrow(domain, range) => Self::Arrow(domain, range),
        }
    }

    pub(super) fn borrow(self, lists: &ListInterner<Sort>) -> SortDef<'_> {
        match self {
            Self::Int => SortDef::Int,
            Self::Bool => SortDef::Bool,
            Self::Tuple(fields) => SortDef::Tuple(Fields::new(&lists[fields])),
            Self::Arrow(domain, range) => SortDef::Arrow(domain, range),
        }
    }
}
