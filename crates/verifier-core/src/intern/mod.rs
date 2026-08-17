use std::hash::{Hash, Hasher};

use interner::{Arena, Definition, Interner, Stored};
use scoped_tls::{scoped, scoped_thread_local};

use crate::{Fields, Name, Sort, SortDef, Term, TermDef};

/// Resolves one identity handle against its interner.
pub trait Resolve<I> {
    type Def<'a>: ?Sized
    where
        Self: 'a;

    fn resolve<'a>(&'a self, identity: I) -> &'a Self::Def<'a>;
}

/// A definition which can be canonicalized in the current interner session.
pub trait Intern {
    type Id;

    fn intern(self) -> Self::Id;
}

/// Append-only syntax storage for one synchronous verification session.
///
/// Definitions and their borrowed slices never move after publication. `Term`,
/// `Sort`, and `Name` are direct pointers into these interners, so they are
/// valid only while this instance is alive.
pub struct Interners {
    terms: Interner<TermDef<'static>>,
    sorts: Interner<SortDef<'static>>,
    names: Interner<&'static str>,
}

impl Interners {
    pub fn resolve_term<'a>(&'a self, term: Term) -> &'a TermDef<'a> {
        // SAFETY: the installed-session contract forbids foreign or stale
        // handles, so `term` belongs to this interner and remains live.
        unsafe { self.terms.resolve_unchecked(term.0) }
    }

    pub fn resolve_sort<'a>(&'a self, sort: Sort) -> &'a SortDef<'a> {
        // SAFETY: identical to `resolve_term` for sort definitions.
        unsafe { self.sorts.resolve_unchecked(sort.0) }
    }

    pub fn resolve_name(&self, name: Name) -> &str {
        // SAFETY: identical to `resolve_term` for names.
        unsafe { self.names.resolve_unchecked(name.0) }
    }
}

impl Default for Interners {
    fn default() -> Self {
        Self { terms: Interner::new(), sorts: Interner::new(), names: Interner::new() }
    }
}

impl Definition for TermDef<'static> {
    type Input<'a> = TermDef<'a>;

    fn hash<H: Hasher>(input: &TermDef<'_>, state: &mut H) {
        Hash::hash(input, state);
    }

    fn equivalent(value: &TermDef<'_>, input: &TermDef<'_>) -> bool {
        value == input
    }

    fn alloc<'arena, 'input>(
        arena: Arena<'arena>,
        definition: TermDef<'input>,
    ) -> Stored<'arena, TermDef<'arena>> {
        let definition = match definition {
            TermDef::Var(var) => TermDef::Var(var),
            TermDef::Const(value) => TermDef::Const(value),
            TermDef::Bool(value) => TermDef::Bool(value),
            TermDef::Unit => TermDef::Unit,
            TermDef::Binary { op, lhs, rhs } => TermDef::Binary { op, lhs, rhs },
            TermDef::Unary { op, expr } => TermDef::Unary { op, expr },
            TermDef::Call { function, arguments } => TermDef::Call {
                function,
                arguments: Fields::new(arena.copy_slice(arguments.as_ref())),
            },
            TermDef::Tuple(fields) => {
                TermDef::Tuple(Fields::new(arena.copy_slice(fields.as_ref())))
            }
            TermDef::Proj { tuple, field } => TermDef::Proj { tuple, field },
        };
        arena.alloc(definition)
    }
}

impl Definition for SortDef<'static> {
    type Input<'a> = SortDef<'a>;

    fn hash<H: Hasher>(input: &SortDef<'_>, state: &mut H) {
        Hash::hash(input, state);
    }

    fn equivalent(value: &SortDef<'_>, input: &SortDef<'_>) -> bool {
        value == input
    }

    fn alloc<'arena, 'input>(
        arena: Arena<'arena>,
        definition: SortDef<'input>,
    ) -> Stored<'arena, SortDef<'arena>> {
        let definition = match definition {
            SortDef::Int => SortDef::Int,
            SortDef::Bool => SortDef::Bool,
            SortDef::Tuple(fields) => {
                SortDef::Tuple(Fields::new(arena.copy_slice(fields.as_ref())))
            }
        };
        arena.alloc(definition)
    }
}

impl Resolve<Term> for Interners {
    type Def<'a>
        = TermDef<'a>
    where
        Self: 'a;

    fn resolve<'a>(&'a self, term: Term) -> &'a TermDef<'a> {
        self.resolve_term(term)
    }
}

impl Resolve<Sort> for Interners {
    type Def<'a>
        = SortDef<'a>
    where
        Self: 'a;

    fn resolve<'a>(&'a self, sort: Sort) -> &'a SortDef<'a> {
        self.resolve_sort(sort)
    }
}

impl Resolve<Name> for Interners {
    type Def<'a>
        = str
    where
        Self: 'a;

    fn resolve(&self, name: Name) -> &str {
        self.resolve_name(name)
    }
}

scoped_thread_local!(
    /// The interners installed for the current verification session.
    ///
    /// # Safety
    ///
    /// In addition to [`scoped_tls::ScopedKey::set`]'s reference-lifetime
    /// contract, the caller must ensure that every `Term`, `Sort`, and `Name`
    /// created under this binding becomes unusable before `set` returns. These
    /// identities contain pointers into the installed [`Interners`]. Interner
    /// bindings must not be nested, so one dynamic call tree has one identity
    /// domain.
    pub static INTERNERS: Interners
);

impl Intern for TermDef<'_> {
    type Id = Term;

    fn intern(self) -> Term {
        let interners = scoped!(INTERNERS);
        Term(interners.terms.intern(self))
    }
}

impl Intern for SortDef<'_> {
    type Id = Sort;

    fn intern(self) -> Sort {
        let interners = scoped!(INTERNERS);
        Sort(interners.sorts.intern(self))
    }
}

impl Intern for &str {
    type Id = Name;

    fn intern(self) -> Name {
        let interners = scoped!(INTERNERS);
        Name(interners.names.intern(self))
    }
}

#[macro_export]
/// Resolves an identity to a definition borrowed from the current interner.
///
/// ```rust,ignore
/// let term = TermDef::Const(1).intern();
/// def!(let def = term);
/// ```
///
/// Nested borrows cannot escape the lexical lookup scope:
///
/// ```compile_fail
/// use verifier_core::{Fields, Term, TermDef, def};
///
/// fn escape(term: Term) -> &'static [Term] {
///     def!(let definition = term);
///     let TermDef::Tuple(fields) = *definition else { return &[] };
///     fields.as_ref()
/// }
/// ```
///
/// This is a statement macro because `tls` must live for the scope.
macro_rules! def {
    (let $definition:ident = $identity:expr $(;)?) => {
        let tls = $crate::scoped!($crate::INTERNERS);
        let $definition = $crate::Resolve::resolve(tls, $identity);
    };
}

#[cfg(test)]
#[macro_export]
macro_rules! test {
    ($name:ident $body:tt) => {
        #[test]
        fn $name() {
            let interners = Interners::default();
            let body = || $body;
            // SAFETY: all tests declared through `test!` are synchronous
            // and discard all arena values before returning.
            unsafe {
                INTERNERS.set(&interners, body);
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::{INTERNERS, Intern, Interners};
    use crate::{
        Environment, Field, Fields, SortDef, TermDef,
        term::{add, and, bool as boolean, eq, int as integer, proj, tuple, var as variable},
    };

    test! {
        interns_and_resolves_borrowed_definitions {
            let one = integer(1);
            let pair = tuple(&[one, one]);

            def!(let def = pair);

            assert_eq!(
                def,
                &TermDef::Tuple(Fields::new(&[one, one]))
            );
            assert_eq!(def.intern(), pair);
        }
    }

    test! {
        references_survive_arena_and_table_growth {
            let first = integer(0);
            def!(let definition = first);

            let int = SortDef::Int.intern();
            def!(let sort_definition = int);

            let name = "first".intern();
            def!(let text = name);

            for value in 1..2_000 {
                integer(value);
            }

            assert_eq!(definition, &TermDef::Const(0));
            assert_eq!(sort_definition, &SortDef::Int);
            assert_eq!(text, "first");
            assert_eq!(definition.intern(), first);
        }
    }

    test! {
        variables_are_sorted_by_their_environment {
            let int = SortDef::Int.intern();
            let bool = SortDef::Bool.intern();

            let mut ints = Environment::new();
            let int_var = ints.bind_value(int, ());
            let int_term = variable(int_var);

            let mut bools = Environment::new();
            let bool_var = bools.bind_value(bool, ());
            let bool_term = variable(bool_var);

            assert_eq!(int_term, bool_term);
            assert_eq!(ints.sort(int_term), Ok(int));
            assert_eq!(bools.sort(bool_term), Ok(bool));
        }
    }

    test! {
        projects_literal_and_symbolic_tuples {
            let int = SortDef::Int.intern();
            let bool = SortDef::Bool.intern();

            let tuple_sort =
                SortDef::Tuple(Fields::new(&[int, bool])).intern();

            let mut env = Environment::new();
            let tuple_var = env.bind_value(tuple_sort, "pair");
            let tuple_sym = variable(tuple_var);

            let second = Field::from(1);

            assert_eq!(
                env.sort(proj(tuple_sym, second)),
                Ok(bool)
            );

            let one = integer(1);
            let yes = boolean(true);
            let pair = tuple(&[one, yes]);

            assert_eq!(proj(pair, Field::from(0)), one);
            assert_eq!(proj(pair, second), yes);
        }
    }

    test! {
        nested_term_construction_is_checked_at_once {
            let env = Environment::<()>::new();

            assert_eq!(
                env.sort(
                    and(
                        boolean(true),
                        eq(
                            integer(1),
                            add(integer(0), integer(1)),
                        ),
                    ),
                ),
                Ok(SortDef::Bool.intern())
            );
        }
    }
}
