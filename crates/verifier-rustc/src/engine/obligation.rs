use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use rustc_middle::mir::Location;
use rustc_span::Span;
use verifier_core::Term;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObligationKind {
    RuntimeAssertion,
    CallPrecondition,
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

impl Display for ExecutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "MIR symbolic execution failed at {:?}[{:?}]: {}",
            self.location.block, self.location.statement_index, self.message
        )
    }
}

impl Error for ExecutionError {}

pub(super) trait LocationExt {
    fn error(self, message: impl Into<String>) -> ExecutionError;
}

impl LocationExt for Location {
    fn error(self, message: impl Into<String>) -> ExecutionError {
        ExecutionError { location: self, message: message.into() }
    }
}
