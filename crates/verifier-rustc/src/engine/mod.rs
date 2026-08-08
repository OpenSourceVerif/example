mod executor;
mod loop_analysis;
mod obligation;

pub(crate) use executor::execute_with_spec;
pub use obligation::{ExecutionError, Limits, Obligation, ObligationKind};
