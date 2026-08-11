mod executor;
mod loop_analysis;
mod obligation;

pub(crate) use executor::execute;
pub use obligation::{ExecutionError, Obligation, ObligationKind};
