use std::{
    cell::RefCell,
    hash::{BuildHasher, Hash},
    ptr::NonNull,
};

use bumpalo::Bump;
use hashbrown::{DefaultHashBuilder, HashTable};
use scoped_tls::{scoped, scoped_thread_local};

use crate::{Fields, Name, Sort, SortDef, Term, TermDef, ir::NameDef};

/// Resolves one identity handle against an interning arena.
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
/// The tables are dropped before the arena. They contain only identity
/// pointers; definitions and their borrowed slices never move after
/// publication. `Term`, `Sort`, and `Name` are direct pointers into this arena,
/// so they are valid only while this instance is alive.
pub struct Interners {
    terms: RefCell<HashTable<Term>>,
    sorts: RefCell<HashTable<Sort>>,
    names: RefCell<HashTable<Name>>,
    hash_builder: DefaultHashBuilder,
    arena: Bump,
}

impl Default for Interners {
    fn default() -> Self {
        Self {
            terms: RefCell::new(HashTable::new()),
            sorts: RefCell::new(HashTable::new()),
            names: RefCell::new(HashTable::new()),
            hash_builder: DefaultHashBuilder::default(),
            arena: Bump::new(),
        }
    }
}

impl Interners {
    fn intern_term(&self, definition: TermDef<'_>) -> Term {
        let hash = self.hash(&definition);
        let mut terms = self.terms.borrow_mut();
        if let Some(term) = terms.find(hash, |term| self.resolve_term(*term) == &definition) {
            return *term;
        }

        let definition = self.store_term(definition);
        let term = Term::new(NonNull::from(self.arena.alloc(definition)));
        terms.insert_unique(hash, term, |term| self.hash(self.resolve_term(*term)));
        term
    }

    fn intern_sort(&self, definition: SortDef<'_>) -> Sort {
        let hash = self.hash(&definition);
        let mut sorts = self.sorts.borrow_mut();
        if let Some(sort) = sorts.find(hash, |sort| self.resolve_sort(*sort) == &definition) {
            return *sort;
        }

        let definition = self.store_sort(definition);
        let sort = Sort::new(NonNull::from(self.arena.alloc(definition)));
        sorts.insert_unique(hash, sort, |sort| self.hash(self.resolve_sort(*sort)));
        sort
    }

    fn intern_name(&self, text: &str) -> Name {
        let hash = self.hash(text);
        let mut names = self.names.borrow_mut();
        if let Some(name) = names.find(hash, |name| self.resolve_name(*name) == text) {
            return *name;
        }

        let text = self.store_str(text);
        let name = Name::new(NonNull::from(self.arena.alloc(NameDef { text })));
        names.insert_unique(hash, name, |name| self.hash(self.resolve_name(*name)));
        name
    }

    /// Resolves a term while rebranding all nested arena borrows to `self`.
    pub fn resolve_term<'a>(&'a self, term: Term) -> &'a TermDef<'a> {
        // SAFETY: `term` points to a fully initialized, immutable definition in
        // this arena. The session contract forbids foreign or stale handles.
        // Every nested slice was allocated in the same arena, so shrinking its
        // internal `'static` implementation lifetime to `'a` is valid.
        unsafe { &*(term.pointer().as_ptr() as *const TermDef<'a>) }
    }

    pub fn resolve_sort<'a>(&'a self, sort: Sort) -> &'a SortDef<'a> {
        // SAFETY: identical to `resolve_term` for sort definitions.
        unsafe { &*(sort.pointer().as_ptr() as *const SortDef<'a>) }
    }

    pub fn resolve_name(&self, name: Name) -> &str {
        // SAFETY: `name` is a current-arena pointer to an immutable `NameDef`.
        unsafe { &name.pointer().as_ref().text }
    }

    fn store_term(&self, definition: TermDef<'_>) -> TermDef<'static> {
        match definition {
            TermDef::Var(var) => TermDef::Var(var),
            TermDef::Const(value) => TermDef::Const(value),
            TermDef::Bool(value) => TermDef::Bool(value),
            TermDef::Unit => TermDef::Unit,
            TermDef::Binary { op, lhs, rhs } => TermDef::Binary { op, lhs, rhs },
            TermDef::Unary { op, expr } => TermDef::Unary { op, expr },
            TermDef::Call { function, arguments } => TermDef::Call {
                function,
                arguments: Fields::new(self.store_slice(arguments.as_ref())),
            },
            TermDef::Tuple(fields) => {
                TermDef::Tuple(Fields::new(self.store_slice(fields.as_ref())))
            }
            TermDef::Proj { tuple, field } => TermDef::Proj { tuple, field },
        }
    }

    fn store_sort(&self, definition: SortDef<'_>) -> SortDef<'static> {
        match definition {
            SortDef::Int => SortDef::Int,
            SortDef::Bool => SortDef::Bool,
            SortDef::Tuple(fields) => {
                SortDef::Tuple(Fields::new(self.store_slice(fields.as_ref())))
            }
        }
    }

    fn store_slice<T: Copy>(&self, values: &[T]) -> &'static [T] {
        let values = self.arena.alloc_slice_copy(values);
        let pointer = values.as_ptr();
        let length = values.len();
        // SAFETY: the slice is immutable after this point and its bump
        // allocation remains live for the entire interner session. The
        // implementation-only `'static` is never exposed without rebranding.
        unsafe { std::slice::from_raw_parts(pointer, length) }
    }

    fn store_str(&self, text: &str) -> &'static str {
        let text = self.arena.alloc_str(text);
        let pointer = text.as_ptr();
        let length = text.len();
        // SAFETY: identical to `store_slice`; UTF-8 validity is preserved by
        // copying from an existing `str`.
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(pointer, length)) }
    }

    fn hash<T: Hash + ?Sized>(&self, value: &T) -> u64 {
        self.hash_builder.hash_one(value)
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
    /// The interning arena installed for the current verification session.
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
        interners.intern_term(self)
    }
}

impl Intern for SortDef<'_> {
    type Id = Sort;

    fn intern(self) -> Sort {
        let interners = scoped!(INTERNERS);
        interners.intern_sort(self)
    }
}

impl Intern for &str {
    type Id = Name;

    fn intern(self) -> Name {
        let interners = scoped!(INTERNERS);
        interners.intern_name(self)
    }
}

#[macro_export]
/// Resolves an interned identity to a definition borrowed from the current arena.
///
/// This is a statement macro because the hidden scoped-TLS proxy must live for
/// at least as long as the resulting definition:
///
/// ```
/// use verifier_core::{Environment, INTERNERS, Intern, Interners, def};
///
/// let interners = Interners::default();
/// let body = || {
///     let term = Environment::<()>::new().int(1);
///     def!(let def = term);
///     let same = def.intern();
///     assert_eq!(same, term);
/// };
/// // SAFETY: `body` is synchronous and discards every arena value.
/// unsafe { INTERNERS.set(&interners, body) };
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
macro_rules! def {
    (let $definition:ident = $identity:expr $(;)?) => {
        let __interners = $crate::scoped!($crate::INTERNERS);
        let $definition = $crate::Resolve::resolve(__interners, $identity);
    };
}

#[cfg(test)]
mod tests {
    use super::{INTERNERS, Intern, Interners};
    use crate::{Environment, Field, Fields, SortDef, TermDef};

    #[test]
    fn interns_and_resolves_borrowed_definitions() {
        let interners = Interners::default();
        let body = || {
            let environment = Environment::<()>::new();
            let one = environment.int(1);
            let tuple = environment.tuple(&[one, one]);
            def!(let def = tuple);
            assert_eq!(def, &TermDef::Tuple(Fields::new(&[one, one])));
            assert_eq!(def.intern(), tuple);
        };
        // SAFETY: `body` is synchronous and discards all arena values.
        unsafe { INTERNERS.set(&interners, body) }
    }

    #[test]
    fn references_survive_arena_and_table_growth() {
        let interners = Interners::default();
        let body = || {
            let environment = Environment::<()>::new();
            let first = environment.int(0);
            def!(let definition = first);
            let int = SortDef::Int.intern();
            def!(let sort_definition = int);
            let name = "first".intern();
            def!(let text = name);
            for value in 1..2_000 {
                environment.int(value);
            }
            assert_eq!(definition, &TermDef::Const(0));
            assert_eq!(sort_definition, &SortDef::Int);
            assert_eq!(text, "first");
            assert_eq!(definition.intern(), first);
        };
        // SAFETY: `body` is synchronous and discards all arena values.
        unsafe { INTERNERS.set(&interners, body) }
    }

    #[test]
    fn variables_are_sorted_by_their_environment() {
        let interners = Interners::default();
        let body = || {
            let int = SortDef::Int.intern();
            let bool = SortDef::Bool.intern();
            let mut ints = Environment::new();
            let int_var = ints.bind_value(int, ());
            let int_term = ints.var(int_var);
            let mut bools = Environment::new();
            let bool_var = bools.bind_value(bool, ());
            let bool_term = bools.var(bool_var);

            assert_eq!(int_term, bool_term);
            assert_eq!(ints.sort(int_term), Ok(int));
            assert_eq!(bools.sort(bool_term), Ok(bool));
        };
        // SAFETY: `body` is synchronous and discards all arena values.
        unsafe { INTERNERS.set(&interners, body) }
    }

    #[test]
    fn projects_literal_and_symbolic_tuples() {
        let interners = Interners::default();
        let body = || {
            let int = SortDef::Int.intern();
            let bool = SortDef::Bool.intern();
            let tuple_sort = SortDef::Tuple(Fields::new(&[int, bool])).intern();
            let mut environment = Environment::new();
            let tuple_var = environment.bind_value(tuple_sort, "pair");
            let tuple_sym = environment.var(tuple_var);

            let second = Field::from(1);
            assert_eq!(environment.sort(environment.proj(tuple_sym, second)), Ok(bool));
            let one = environment.int(1);
            let yes = environment.bool(true);
            let tuple = environment.tuple(&[one, yes]);
            assert_eq!(environment.proj(tuple, Field::from(0)), one);
            assert_eq!(environment.proj(tuple, second), yes);
        };
        // SAFETY: `body` is synchronous and discards all arena values.
        unsafe { INTERNERS.set(&interners, body) }
    }

    #[test]
    fn nested_constructor_calls_need_no_temporaries() {
        let interners = Interners::default();
        let body = || {
            let environment = Environment::<()>::new();
            let term = environment.and(
                environment.bool(true),
                environment.eq(
                    environment.int(1),
                    environment.add(environment.int(0), environment.int(1)),
                ),
            );
            assert_eq!(environment.sort(term), Ok(SortDef::Bool.intern()));
        };
        // SAFETY: `body` is synchronous and discards all arena values.
        unsafe { INTERNERS.set(&interners, body) }
    }
}
