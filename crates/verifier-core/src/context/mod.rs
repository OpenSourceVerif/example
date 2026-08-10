use crate::{Name, Sort, SortDef, Sym, SymDef, SymDefStored, Term, TermDef};
use index_vec::IndexVec;

mod builders;

use interner::{List, ListInterner, StringInterner, StructInterner};

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
    terms: StructInterner<Term, TermDef<List<Term>>>,
    term_lists: ListInterner<Term>,
    names: StringInterner<Name>,
    syms: IndexVec<Sym, SymDefStored>,
    sorts: StructInterner<Sort, SortDef>,
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
}

impl<'a> Intern<Term, TermDef<&'a [Term]>> for Context {
    fn intern(&mut self, term: TermDef<&'a [Term]>) -> Term {
        let term = term.map_fields(|f| self.term_lists.intern(f));
        self.terms.intern(term)
    }
}

impl DefStore<Term> for Context {
    type Ref<'a>
        = TermDef<&'a [Term]>
    where
        Self: 'a;

    fn get(&self, term: Term) -> TermDef<&[Term]> {
        self.terms[term].map_fields(|f| &self.term_lists[f])
    }
}

impl Intern<Sort, SortDef> for Context {
    fn intern(&mut self, sort: SortDef) -> Sort {
        self.sorts.intern(sort)
    }
}

impl DefStore<Sort> for Context {
    type Ref<'a>
        = SortDef
    where
        Self: 'a;

    fn get(&self, idx: Sort) -> SortDef {
        self.sorts[idx]
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
    use super::{Context, DefStore, Intern};
    use crate::{Sort, SortDef, Sym, Term, TermDef};

    #[test]
    fn interns_terms_and_allocates_symbols() {
        let mut context = Context::default();

        let int: Sort = context.intern(SortDef::Int);
        let same_int: Sort = context.intern(SortDef::Int);
        assert_eq!(same_int, int);

        let x: Sym = context.symbol("x", int);
        let other_x: Sym = context.symbol("x", int);
        assert_ne!(other_x, x);

        let x_expr: Term = context.intern(TermDef::Sym(x));
        let same_x_expr: Term = context.intern(TermDef::Sym(x));
        assert_eq!(same_x_expr, x_expr);
        assert_ne!(context.sym(other_x), x_expr);

        let tuple = context.tuple(&[x_expr, same_x_expr]);
        let same_tuple = context.tuple(&[same_x_expr, x_expr]);
        assert_eq!(same_tuple, tuple);
        assert_eq!(context.get(tuple), TermDef::Tuple(&[x_expr, x_expr][..]));

        assert_eq!(context.syms().count(), 2);
    }
}
