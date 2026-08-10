use crate::{Name, Sort, SortDef, Sym, SymDef, Term, TermDef};
use index_vec::IndexVec;

mod builders;
mod storage;

use storage::{SortDefStored, SymDefStored, TermDefStored};

use interner::{ListInterner, StringInterner, StructInterner};

pub trait Intern<I, D> {
    fn intern(&mut self, def: D) -> I;
}

pub trait DefStore<I> {
    type Ref<'a>
    where
        Self: 'a;

    fn get<'a>(&'a self, idx: I) -> Self::Ref<'a>;
}

#[derive(Default)]
pub struct Context {
    terms: StructInterner<Term, TermDefStored>,
    term_lists: ListInterner<Term>,
    names: StringInterner<Name>,
    syms: IndexVec<Sym, SymDefStored>,
    sorts: StructInterner<Sort, SortDefStored>,
    sort_lists: ListInterner<Sort>,
}

impl Context {
    /// declare a fresh symbol.
    pub fn symbol(&mut self, name: &str, sort: Sort) -> Sym {
        let name = self.names.intern(name);
        self.syms.push(SymDefStored { name, sort })
    }

    pub fn syms(&self) -> impl Iterator<Item = (Sym, SymDef<'_>)> {
        self.syms.iter_enumerated().map(|(sym, SymDefStored { name, sort })| {
            let name = &self.names[*name];

            (sym, SymDef { name, sort: *sort })
        })
    }

    pub(crate) fn sorts(&self) -> impl Iterator<Item = (Sort, SortDef<'_>)> {
        self.sorts.iter_enumerated().map(|(sort, def)| (sort, def.borrow(&self.sort_lists)))
    }
}

impl Intern<Term, TermDef<'_>> for Context {
    fn intern(&mut self, term: TermDef<'_>) -> Term {
        self.terms.intern(TermDefStored::store(term, &mut self.term_lists))
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

impl Intern<Sort, SortDef<'_>> for Context {
    fn intern(&mut self, sort: SortDef<'_>) -> Sort {
        self.sorts.intern(SortDefStored::store(sort, &mut self.sort_lists))
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

impl DefStore<Sym> for Context {
    type Ref<'a>
        = SymDef<'a>
    where
        Self: 'a;

    fn get(&self, sym: Sym) -> SymDef<'_> {
        let SymDefStored { name, sort } = self.syms[sym];
        SymDef { name: &self.names[name], sort }
    }
}

#[cfg(test)]
mod tests {
    use super::{Context, DefStore};
    use crate::{Field, Fields, SortDef, Sym, TermKind};

    #[test]
    fn interns_terms_and_allocates_symbols() {
        let mut context = Context::default();

        let int = context.int_sort();
        let same_int = context.int_sort();
        assert_eq!(same_int, int);

        let x: Sym = context.symbol("x", int);
        let other_x: Sym = context.symbol("x", int);
        assert_ne!(other_x, x);

        let x_expr = context.sym(x);
        let same_x_expr = context.sym(x);
        assert_eq!(same_x_expr, x_expr);
        assert_ne!(context.sym(other_x), x_expr);

        let tuple = context.tuple(&[x_expr, same_x_expr]);
        let same_tuple = context.tuple(&[same_x_expr, x_expr]);
        assert_eq!(same_tuple, tuple);
        assert_eq!(context.get(tuple).kind, TermKind::Tuple(Fields::new(&[x_expr, x_expr])));
        assert_eq!(context.get(tuple).sort, context.tuple_sort(&[int, int]));

        assert_eq!(context.syms().count(), 2);
    }

    #[test]
    fn parameters_are_sorted() {
        let mut context = Context::default();
        let int = context.int_sort();
        let bool = context.bool_sort();

        let int_param = context.param(0, int);
        let bool_param = context.param(0, bool);

        assert_ne!(int_param, bool_param);
        assert_eq!(context.get(int_param).sort, int);
        assert_eq!(context.get(bool_param).sort, bool);
    }

    #[test]
    fn projects_literal_and_symbolic_tuples() {
        let mut context = Context::default();
        let int = context.int_sort();
        let bool = context.bool_sort();
        let tuple_sort = context.tuple_sort(&[int, bool]);
        let tuple_sym = context.symbol("pair", tuple_sort);
        let tuple_sym = context.sym(tuple_sym);

        let second = Field::from(1);
        let symbolic_field = context.proj(tuple_sym, second);
        assert_eq!(context.get(symbolic_field).sort, bool);
        assert_eq!(
            context.get(symbolic_field).kind,
            TermKind::Proj { tuple: tuple_sym, field: second }
        );

        let one = context.int_lit(1);
        let yes = context.bool_lit(true);
        let tuple = context.tuple(&[one, yes]);
        assert_eq!(context.proj(tuple, Field::from(0)), one);
        assert_eq!(context.proj(tuple, second), yes);
        let unit = context.unit();
        let unit_sort = context.unit_sort();
        assert_eq!(context.get(unit).sort, unit_sort);
        assert_eq!(context.tuple(&[]), unit);
        assert_eq!(context.get(tuple_sort), SortDef::Tuple(Fields::new(&[int, bool])));
    }
}
