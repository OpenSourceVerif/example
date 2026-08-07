use crate::{Name, Sort, SortDef, Sym, SymDef, SymDefInterned, Term, TermDef};

mod shorthand;

use interner::{ArrayInterner, StringInterner};

pub trait Intern<I, D> {
    type Ref<'a>
    where
        Self: 'a;

    fn intern(&mut self, def: D) -> I;

    fn get<'a>(&'a self, idx: I) -> Self::Ref<'a>;
}

#[derive(Default)]
pub struct Context {
    exprs: ArrayInterner<Term, TermDef>,
    sym_names: StringInterner<Name>,
    syms: ArrayInterner<Sym, SymDefInterned>,
    sorts: ArrayInterner<Sort, SortDef>,
}

impl Context {
    pub fn syms<'a>(&'a self) -> impl Iterator<Item = SymDef<'a>> {
        self.syms.iter().map(|SymDefInterned { name, sort }| {
            let name = &self.sym_names[*name];

            SymDef { name, sort: *sort }
        })
    }
}

impl Intern<Term, TermDef> for Context {
    type Ref<'a>
        = TermDef
    where
        Self: 'a;

    fn intern(&mut self, expr: TermDef) -> Term {
        self.exprs.intern(expr)
    }

    fn get(&self, idx: Term) -> TermDef {
        self.exprs[idx].clone()
    }
}

impl<'input> Intern<Sym, SymDef<'input>> for Context {
    type Ref<'a>
        = SymDef<'a>
    where
        Self: 'a;

    fn intern(&mut self, sym: SymDef<'input>) -> Sym {
        let name = self.sym_names.intern(sym.name);
        self.syms.intern(SymDefInterned { name, sort: sym.sort })
    }

    fn get<'a>(&'a self, idx: Sym) -> SymDef<'a> {
        let SymDefInterned { name, sort } = self.syms[idx];
        let name = &self.sym_names[name];

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
    fn interner_works() {
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

        assert_eq!(context.syms().count(), 1);
    }
}
