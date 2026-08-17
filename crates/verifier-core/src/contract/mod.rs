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
    env: Environment<B>,
}

impl<B> Clause<B> {
    /// Checks and creates an open contract clause.
    pub fn new(term: Term, env: Environment<B>) -> Result<Self, TypeError> {
        env.sort(term)?;
        Ok(Self { term, env })
    }

    /// Returns the clause's term.
    pub const fn term(&self) -> Term {
        self.term
    }

    /// Returns the environment which scopes the term's frontend bindings.
    pub const fn env(&self) -> &Environment<B> {
        &self.env
    }

    const fn from_checked(term: Term, env: Environment<B>) -> Self {
        Self { term, env }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Environment, INTERNERS, Intern, Interners, Op, SortDef, TermDef, TypeError, term::int, test
    };

    use super::Clause;

    test! {
        requires_a_well_sorted_term {
            let env = Environment::<()>::new();
            let term = int(1);
            assert_eq!(Clause::new(term, env).unwrap().term(), term);

            let env = Environment::<()>::new();
            let yes = TermDef::Bool(true).intern();
            let invalid = TermDef::Binary { op: Op::Add, lhs: yes, rhs: yes }.intern();
            let int = SortDef::Int.intern();
            let bool = SortDef::Bool.intern();
            assert_eq!(
                Clause::new(invalid, env),
                Err(TypeError::Sort { expected: int, actual: bool })
            );
        }
    }
}
