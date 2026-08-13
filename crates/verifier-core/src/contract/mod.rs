//! Open contract clauses.

mod instantiate;
mod parser;

use crate::{Environment, Term};

pub use instantiate::{Actual, InstantiateError, instantiate};
pub use parser::{Expected, ParseError, ParseErrorKind, ResolveError, parse};

/// A term and the environment that scopes its frontend bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause<B> {
    pub term: Term,
    pub environment: Environment<B>,
}
