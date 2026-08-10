use verifier_core::Sort;

mod collect;
mod instantiate;
mod model;
mod parser;

pub(crate) use collect::collect_function_spec;
pub(crate) use instantiate::instantiate;
pub(crate) use model::Source;
pub use model::{Clause, FunctionSpec, LoopSpec, SpecError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Binding {
    sort: Sort,
    source: Source,
    ambiguous: bool,
}
