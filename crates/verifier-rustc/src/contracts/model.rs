use std::fmt::{Display, Formatter, Result as FormatResult};

use rustc_middle::mir::Local;
use rustc_span::Span;
use smallvec::SmallVec;
use verifier_core::Term;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Local(Local),
    Result,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause {
    pub term: Term,
    pub span: Span,
    pub(crate) sources: SmallVec<[Source; 4]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopSpec {
    pub span: Span,
    pub invariants: Vec<Clause>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionSpec {
    pub requires: Vec<Clause>,
    pub ensures: Vec<Clause>,
    pub loops: Vec<LoopSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecError {
    pub span: Span,
    pub message: String,
}

impl Display for SpecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FormatResult {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SpecError {}
