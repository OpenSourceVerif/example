use crate::{Name, Sort, SortDef, Sym, SymDef, SymDefInterned, Term, TermDef};

mod builders;

use interner::{List, ListInterner, StringInterner, StructInterner};

pub trait Intern<I, D> {
    type Ref<'a>
    where
        Self: 'a;

    fn intern(&mut self, def: D) -> I;

    fn get<'a>(&'a self, idx: I) -> Self::Ref<'a>;
}

#[derive(Default)]
pub struct Context {
    terms: StructInterner<Term, TermDef<List<Term>>>,
    term_lists: ListInterner<Term>,
    names: StringInterner<Name>,
    syms: StructInterner<Sym, SymDefInterned>,
    sorts: StructInterner<Sort, SortDef>,
}

impl Context {
    pub fn syms<'a>(&'a self) -> impl Iterator<Item = SymDef<'a>> {
        self.syms.iter().map(|SymDefInterned { name, sort }| {
            let name = &self.names[*name];

            SymDef { name, sort: *sort }
        })
    }
}

impl<'input> Intern<Term, TermDef<&'input [Term]>> for Context {
    type Ref<'a>
        = TermDef<&'a [Term]>
    where
        Self: 'a;

    fn intern(&mut self, term: TermDef<&'input [Term]>) -> Term {
        let term = term.map_fields(|f| self.term_lists.intern(f));
        self.terms.intern(term)
    }

    fn get(&self, term: Term) -> TermDef<&[Term]> {
        self.terms[term].map_fields(|f| &self.term_lists[f])
    }
}

impl<'input> Intern<Sym, SymDef<'input>> for Context {
    type Ref<'a>
        = SymDef<'a>
    where
        Self: 'a;

    fn intern(&mut self, sym: SymDef<'input>) -> Sym {
        let name = self.names.intern(sym.name);
        self.syms.intern(SymDefInterned { name, sort: sym.sort })
    }

    fn get<'a>(&'a self, idx: Sym) -> SymDef<'a> {
        let SymDefInterned { name, sort } = self.syms[idx];
        let name = &self.names[name];

        SymDef { name, sort }
    }
}

impl Intern<Sort, SortDef> for Context {
    type Ref<'a>
        = SortDef
    where
        Self: 'a;

    fn intern(&mut self, sort: SortDef) -> Sort {
        self.sorts.intern(sort)
    }

    fn get(&self, idx: Sort) -> SortDef {
        self.sorts[idx]
    }
}

#[cfg(test)]
mod tests {
    use super::{Context, Intern};
    use crate::{Sort, SortDef, Sym, SymDef, Term, TermDef};

    #[test]
    fn handle_preserves_definition_equality() {
        let mut context = Context::default();

        let int: Sort = context.intern(SortDef::Int);
        let same_int: Sort = context.intern(SortDef::Int);
        assert_eq!(same_int, int);

        let x: Sym = context.intern(SymDef { name: "x", sort: int });
        let same_x: Sym = context.intern(SymDef { name: "x", sort: int });
        assert_eq!(same_x, x);

        let x_expr: Term = context.intern(TermDef::Sym(x));
        let same_x_expr: Term = context.intern(TermDef::Sym(x));
        assert_eq!(same_x_expr, x_expr);

        let tuple = context.tuple(&[x_expr, same_x_expr]);
        let same_tuple = context.tuple(&[same_x_expr, x_expr]);
        assert_eq!(same_tuple, tuple);
        assert_eq!(context.get(tuple), TermDef::Tuple(&[x_expr, x_expr][..]));

        assert_eq!(context.syms().count(), 1);
    }
}
