use std::cell::RefCell;

use generative_scoped_tls::{scoped, scoped_thread_local};

use crate::{Name, Sort, SortDef, Term, TermDef};

mod storage;

use storage::{SortDefStored, TermDefStored};

use interner::{ListInterner, StringInterner, StructInterner};

pub trait DefStore<I> {
    type Ref<'a>
    where
        Self: 'a;

    fn get<'a>(&'a self, idx: I) -> Self::Ref<'a>;
}

#[derive(Default)]
/// Interned syntax shared by every environment in one verification scope.
///
/// This is public only because the scoped TLS key's type is part of its public type. Ordinary
/// clients should use [`Intern`] and [`INTERNERS`] rather than passing this storage around.
pub struct Interners {
    terms: StructInterner<Term, TermDefStored>,
    term_lists: ListInterner<Term>,
    names: StringInterner<Name>,
    sorts: StructInterner<Sort, SortDefStored>,
    sort_lists: ListInterner<Sort>,
}

impl Interners {
    fn name(&mut self, name: &str) -> Name {
        self.names.intern(name)
    }

    pub(crate) fn intern_term(&mut self, term: TermDef<'_>) -> Term {
        self.terms.intern(TermDefStored::store(term, &mut self.term_lists))
    }

    pub(crate) fn intern_sort(&mut self, sort: SortDef<'_>) -> Sort {
        self.sorts.intern(SortDefStored::store(sort, &mut self.sort_lists))
    }
}

impl DefStore<Term> for Interners {
    type Ref<'a>
        = TermDef<'a>
    where
        Self: 'a;

    fn get(&self, term: Term) -> TermDef<'_> {
        self.terms[term].borrow(&self.term_lists)
    }
}

impl DefStore<Sort> for Interners {
    type Ref<'a>
        = SortDef<'a>
    where
        Self: 'a;

    fn get(&self, sort: Sort) -> SortDef<'_> {
        self.sorts[sort].borrow(&self.sort_lists)
    }
}

impl DefStore<Name> for Interners {
    type Ref<'a>
        = &'a str
    where
        Self: 'a;

    fn get(&self, name: Name) -> &str {
        &self.names[name]
    }
}

scoped_thread_local!(pub static INTERNERS: RefCell<Interners>);

/// A value that can be interned in the current verification scope.
pub trait Intern {
    type Id;

    fn intern(self) -> Self::Id;
}

impl Intern for TermDef<'_> {
    type Id = Term;

    fn intern(self) -> Term {
        scoped!(let interners = INTERNERS);
        interners.borrow_mut().intern_term(self)
    }
}

impl Intern for SortDef<'_> {
    type Id = Sort;

    fn intern(self) -> Sort {
        scoped!(let interners = INTERNERS);
        interners.borrow_mut().intern_sort(self)
    }
}

impl Intern for &str {
    type Id = Name;

    fn intern(self) -> Name {
        scoped!(let interners = INTERNERS);
        interners.borrow_mut().name(self)
    }
}

/// Runs synchronous verification with a fresh interning arena installed on this thread.
///
/// # Safety
///
/// `body` and anything it calls must not suspend or otherwise retain a reference obtained from
/// [`INTERNERS`] after `body` returns. In particular, no future or coroutine may yield while such
/// a reference is live. Interned handles must not be returned or stored for use after `body`, and
/// verifier scopes must not be nested: handles identify entries only within this arena and carry
/// no arena identity.
pub unsafe fn scope<R>(body: impl FnOnce() -> R) -> R {
    assert!(!INTERNERS.is_set(), "nested verifier interner scope");
    let interners = RefCell::new(Interners::default());
    // SAFETY: delegated to this function's caller; the callback is formed before installation.
    unsafe { INTERNERS.set(&interners, body) }
}

#[cfg(test)]
mod tests {
    use super::{DefStore, INTERNERS, Intern, scope};
    use crate::{Environment, Field, Fields, SortDef, TermDef, scoped};

    #[test]
    fn interns_terms_and_allocates_variables() {
        // SAFETY: this test is entirely synchronous and does not retain scoped references.
        unsafe {
            scope(|| {
                let int = SortDef::Int.intern();
                let same_int = SortDef::Int.intern();
                let pair = SortDef::Tuple(Fields::new(&[int, int])).intern();
                assert_eq!(same_int, int);

                let mut environment = Environment::new();
                let x = environment.bind_value(int, "x");
                let other_x = environment.bind_value(int, "x");
                assert_ne!(other_x, x);

                let x_expr = environment.var(x);
                let same_x_expr = environment.var(x);
                assert_eq!(same_x_expr, x_expr);
                assert_ne!(environment.var(other_x), x_expr);

                let tuple = environment.tuple(&[x_expr, same_x_expr]);
                let same_tuple = environment.tuple(&[same_x_expr, x_expr]);
                assert_eq!(same_tuple, tuple);
                scoped!(let interners = INTERNERS);
                assert_eq!(
                    interners.borrow().get(tuple),
                    TermDef::Tuple(Fields::new(&[x_expr, x_expr]))
                );
                assert_eq!(environment.sort(tuple), Ok(pair));
            })
        }
    }

    #[test]
    fn variables_are_sorted_by_their_environment() {
        // SAFETY: this test is entirely synchronous and does not retain scoped references.
        unsafe {
            scope(|| {
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
            })
        }
    }

    #[test]
    fn projects_literal_and_symbolic_tuples() {
        // SAFETY: this test is entirely synchronous and does not retain scoped references.
        unsafe {
            scope(|| {
                let int = SortDef::Int.intern();
                let bool = SortDef::Bool.intern();
                let tuple_sort = SortDef::Tuple(Fields::new(&[int, bool])).intern();
                let unit_sort = SortDef::Tuple(Fields::new(&[])).intern();
                let mut environment = Environment::new();
                let tuple_var = environment.bind_value(tuple_sort, "pair");
                let tuple_sym = environment.var(tuple_var);

                let second = Field::from(1);
                let symbolic_field = environment.proj(tuple_sym, second);
                assert_eq!(environment.sort(symbolic_field), Ok(bool));

                let one = environment.int(1);
                let yes = environment.bool(true);
                let tuple = environment.tuple(&[one, yes]);
                assert_eq!(environment.proj(tuple, Field::from(0)), one);
                assert_eq!(environment.proj(tuple, second), yes);
                let unit = environment.unit();
                assert_eq!(environment.sort(unit), Ok(unit_sort));
                assert_eq!(environment.tuple(&[]), unit);
            })
        }
    }

    #[test]
    fn nested_constructor_calls_need_no_temporaries() {
        // SAFETY: this test is entirely synchronous and does not retain scoped references.
        unsafe {
            scope(|| {
                let environment = Environment::<()>::new();
                let term = environment.and(
                    environment.bool(true),
                    environment.eq(
                        environment.int(1),
                        environment.add(environment.int(0), environment.int(1)),
                    ),
                );
                assert_eq!(environment.sort(term), Ok(SortDef::Bool.intern()));
            })
        }
    }

    #[test]
    #[should_panic(expected = "nested verifier interner scope")]
    fn rejects_nested_scopes() {
        // SAFETY: this deliberately tests the runtime guard; neither scope suspends.
        unsafe { scope(|| scope(|| ())) }
    }
}
