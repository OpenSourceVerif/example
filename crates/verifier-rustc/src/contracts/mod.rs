use rustc_middle::mir::Local;
use verifier_core::Sort;

mod collect;
mod instantiate;
mod model;
mod parser;

pub(crate) use collect::collect_function_spec;
pub(crate) use instantiate::{instantiate, local_bindings};
pub use model::{Clause, FunctionSpec, LoopSpec, SpecError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Binding {
    sort: Sort,
    local: Option<Local>,
    ambiguous: bool,
}
