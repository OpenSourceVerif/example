use std::fmt;

use rustc_middle::mir::Location;
use rustc_span::Span;
use verifier_core::Term;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObligationKind {
    RuntimeAssertion,
    Postcondition,
    LoopInvariantInitialization,
    LoopInvariantPreservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Obligation {
    pub kind: ObligationKind,
    pub location: Location,
    pub span: Span,
    pub condition: Term,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionError {
    pub location: Location,
    pub message: String,
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "MIR symbolic execution failed at {:?}[{:?}]: {}",
            self.location.block, self.location.statement_index, self.message
        )
    }
}

impl std::error::Error for ExecutionError {}

pub(super) trait LocationExt {
    fn error(self, message: impl Into<String>) -> ExecutionError;
}

impl LocationExt for Location {
    fn error(self, message: impl Into<String>) -> ExecutionError {
        ExecutionError { location: self, message: message.into() }
    }
}
