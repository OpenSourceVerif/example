//! Open contract clauses.

mod instantiate;
mod parser;

use crate::{Environment, Term, TypeError};

pub use instantiate::{Actual, InstantiateError, instantiate};
pub use parser::{Expected, ParseError, ParseErrorKind, ResolveError, parse};

/// A well-sorted term and the environment that scopes its frontend bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause<B> {
    term: Term,
    environment: Environment<B>,
}

impl<B> Clause<B> {
    /// Checks and creates an open contract clause.
    pub fn new(term: Term, environment: Environment<B>) -> Result<Self, TypeError> {
        environment.sort(term)?;
        Ok(Self { term, environment })
    }

    /// Returns the clause's term.
    pub const fn term(&self) -> Term {
        self.term
    }

    /// Returns the environment which scopes the term's frontend bindings.
    pub const fn environment(&self) -> &Environment<B> {
        &self.environment
    }

    const fn from_checked(term: Term, environment: Environment<B>) -> Self {
        Self { term, environment }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Environment, INTERNERS, Intern, Interners, Op, SortDef, TermDef, TypeError};

    use super::Clause;

    #[test]
    fn requires_a_well_sorted_term() {
        let interners = Interners::default();
        let body = || {
            let environment = Environment::<()>::new();
            let term = environment.int(1);
            assert_eq!(Clause::new(term, environment).unwrap().term(), term);

            let environment = Environment::<()>::new();
            let yes = TermDef::Bool(true).intern();
            let invalid = TermDef::Binary { op: Op::Add, lhs: yes, rhs: yes }.intern();
            let int = SortDef::Int.intern();
            let bool = SortDef::Bool.intern();
            assert_eq!(
                Clause::new(invalid, environment),
                Err(TypeError::Sort { expected: int, actual: bool })
            );
        };
        // SAFETY: `body` is synchronous and discards all arena values.
        unsafe { INTERNERS.set(&interners, body) }
    }
}
