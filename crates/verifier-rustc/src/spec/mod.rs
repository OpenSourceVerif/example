mod collect;
mod model;

pub(crate) use collect::collect;
pub(crate) use model::{Clause, LoopSpec, Slot, Spec};
pub use model::{SpecError, SpecErrorKind};
