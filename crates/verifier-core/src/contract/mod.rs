//! Open contract clauses.

mod instantiate;
mod parser;

use smallvec::SmallVec;

use crate::Term;

pub use instantiate::{InstantiateError, instantiate};
pub use parser::{Expected, ParseError, ParseErrorKind, ResolveError, parse};

/// An open term and the frontend bindings for its parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause<B> {
    pub term: Term,
    pub bindings: SmallVec<[B; 4]>,
}
