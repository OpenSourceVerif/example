use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FormatResult},
};

use rustc_middle::mir::Location;
use rustc_span::Span;
use verifier_core::Term;

/// Limits which keep forward exploration finite in the presence of loops and
/// exponential path growth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum number of basic blocks visited along any single path.
    pub max_steps: u32,
    /// Maximum worklist size at any instant.
    pub max_pending: u32,
    /// Maximum total number of symbolic states entering basic blocks.
    pub max_states: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_steps: 10_000, max_pending: 1_024, max_states: 100_000 }
    }
}

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
    pub span: Option<Span>,
    pub condition: Term,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionError {
    pub location: Location,
    pub message: String,
}

impl Display for ExecutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FormatResult {
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
