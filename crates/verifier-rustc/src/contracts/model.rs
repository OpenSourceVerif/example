use std::fmt;

use rustc_middle::mir::Local;
use rustc_span::Span;
use verifier_core::contract::{Clause as OpenClause, ParseErrorKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    Local(Local),
    Result,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

pub(crate) type Clause = Spanned<OpenClause<Slot>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoopSpec {
    pub invariants: Vec<Clause>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Spec {
    pub requires: Vec<Clause>,
    pub ensures: Vec<Clause>,
    pub loops: Vec<LoopSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecErrorKind {
    Parse(ParseErrorKind),
    Args,
    Source,
    ReservedResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecError {
    pub span: Span,
    pub kind: SpecErrorKind,
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            SpecErrorKind::Parse(kind) => kind.fmt(f),
            SpecErrorKind::Args => {
                f.write_str("contract attribute requires parenthesized arguments")
            }
            SpecErrorKind::Source => f.write_str("contract source is unavailable"),
            SpecErrorKind::ReservedResult => f.write_str("`result` is reserved in contracts"),
        }
    }
}

impl std::error::Error for SpecError {}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(local) => write!(f, "local {local:?}"),
            Self::Result => f.write_str("`result`"),
        }
    }
}
