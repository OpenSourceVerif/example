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
/// Interned term syntax, sorts, lists, and names shared by multiple environments.
pub struct Context {
    terms: StructInterner<Term, TermDefStored>,
    term_lists: ListInterner<Term>,
    names: StringInterner<Name>,
    sorts: StructInterner<Sort, SortDefStored>,
    sort_lists: ListInterner<Sort>,
}

impl Context {
    pub fn name(&mut self, name: &str) -> Name {
        self.names.intern(name)
    }

    pub(crate) fn intern_term(&mut self, term: TermDef<'_>) -> Term {
        self.terms.intern(TermDefStored::store(term, &mut self.term_lists))
    }

    pub(crate) fn intern_sort(&mut self, sort: SortDef<'_>) -> Sort {
        self.sorts.intern(SortDefStored::store(sort, &mut self.sort_lists))
    }
}

impl DefStore<Term> for Context {
    type Ref<'a>
        = TermDef<'a>
    where
        Self: 'a;

    fn get(&self, term: Term) -> TermDef<'_> {
        self.terms[term].borrow(&self.term_lists)
    }
}

impl DefStore<Sort> for Context {
    type Ref<'a>
        = SortDef<'a>
    where
        Self: 'a;

    fn get(&self, sort: Sort) -> SortDef<'_> {
        self.sorts[sort].borrow(&self.sort_lists)
    }
}

impl DefStore<Name> for Context {
    type Ref<'a>
        = &'a str
    where
        Self: 'a;

    fn get(&self, name: Name) -> &str {
        &self.names[name]
    }
}

#[cfg(test)]
mod tests {
    use super::{Context, DefStore};
    use crate::{Environment, Field, Fields, SortDef, TermKind};

    #[test]
    fn interns_terms_and_allocates_variables() {
        let mut context = Context::default();

        let int = context.int_sort();
        let same_int = context.int_sort();
        let pair = context.tuple_sort(&[int, int]);
        assert_eq!(same_int, int);

        let mut environment = Environment::new();
        let x = environment.bind_value(int, "x");
        let other_x = environment.bind_value(int, "x");
        assert_ne!(other_x, x);

        let mut terms = context.builder(&mut environment);
        let x_expr = terms.var(x);
        let same_x_expr = terms.var(x);
        assert_eq!(same_x_expr, x_expr);
        assert_ne!(terms.var(other_x), x_expr);

        let tuple = terms.tuple(&[x_expr, same_x_expr]);
        let same_tuple = terms.tuple(&[same_x_expr, x_expr]);
        assert_eq!(same_tuple, tuple);
        assert_eq!(
            terms.context().get(tuple).kind,
            TermKind::Tuple(Fields::new(&[x_expr, x_expr]))
        );
        assert_eq!(terms.term_sort(tuple), pair);
    }

    #[test]
    fn variables_are_sorted_by_their_environment() {
        let mut context = Context::default();
        let int = context.int_sort();
        let bool = context.bool_sort();

        let mut ints = Environment::new();
        let int_var = ints.bind_value(int, ());
        let int_term = context.builder(&mut ints).var(int_var);

        let mut bools = Environment::new();
        let bool_var = bools.bind_value(bool, ());
        let bool_term = context.builder(&mut bools).var(bool_var);

        assert_eq!(int_term, bool_term);
        assert_eq!(context.builder(&mut ints).term_sort(int_term), int);
        assert_eq!(context.builder(&mut bools).term_sort(bool_term), bool);
    }

    #[test]
    fn projects_literal_and_symbolic_tuples() {
        let mut context = Context::default();
        let int = context.int_sort();
        let bool = context.bool_sort();
        let tuple_sort = context.tuple_sort(&[int, bool]);
        let unit_sort = context.unit_sort();
        let mut environment = Environment::new();
        let tuple_var = environment.bind_value(tuple_sort, "pair");
        let mut terms = context.builder(&mut environment);
        let tuple_sym = terms.var(tuple_var);

        let second = Field::from(1);
        let symbolic_field = terms.proj(tuple_sym, second);
        assert_eq!(terms.term_sort(symbolic_field), bool);
        assert_eq!(
            terms.context().get(symbolic_field).kind,
            TermKind::Proj { tuple: tuple_sym, field: second }
        );

        let one = terms.int_lit(1);
        let yes = terms.bool_lit(true);
        let tuple = terms.tuple(&[one, yes]);
        assert_eq!(terms.proj(tuple, Field::from(0)), one);
        assert_eq!(terms.proj(tuple, second), yes);
        let unit = terms.unit();
        assert_eq!(terms.term_sort(unit), unit_sort);
        assert_eq!(terms.tuple(&[]), unit);
        assert_eq!(terms.context().get(tuple_sort), SortDef::Tuple(Fields::new(&[int, bool])));
    }
}
