use crate::{Expr, ExprDef, Name, Sort, SortDef, Stmt, StmtDef, Sym, SymDef, SymDefInterned};

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
    stmts: ArrayInterner<Stmt, StmtDef>,
    exprs: ArrayInterner<Expr, ExprDef>,
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

impl Intern<Expr, ExprDef> for Context {
    type Ref<'a>
        = ExprDef
    where
        Self: 'a;

    fn intern(&mut self, expr: ExprDef) -> Expr {
        self.exprs.intern(expr)
    }

    fn get(&self, idx: Expr) -> ExprDef {
        self.exprs[idx]
    }
}

impl Intern<Stmt, StmtDef> for Context {
    type Ref<'a>
        = StmtDef
    where
        Self: 'a;

    fn intern(&mut self, stmt: StmtDef) -> Stmt {
        self.stmts.intern(stmt)
    }

    fn get(&self, idx: Stmt) -> StmtDef {
        self.stmts[idx]
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
    use crate::{Expr, ExprDef, Sort, SortDef, Stmt, StmtDef, Sym, SymDef};

    #[test]
    fn interner_works() {
        let mut context = Context::default();

        let int: Sort = context.intern(SortDef::Int);
        let same_int: Sort = context.intern(SortDef::Int);
        assert_eq!(same_int, int);

        let x: Sym = context.intern(SymDef { name: "x", sort: int });
        let same_x: Sym = context.intern(SymDef { name: "x", sort: int });
        assert_eq!(same_x, x);

        let x_expr: Expr = context.intern(ExprDef::Sym(x));
        let same_x_expr: Expr = context.intern(ExprDef::Sym(x));
        assert_eq!(same_x_expr, x_expr);

        let assignment: Stmt = context.intern(StmtDef::Assign { var: x, def: x_expr });
        let same_assignment: Stmt = context.intern(StmtDef::Assign { var: x, def: x_expr });
        assert_eq!(same_assignment, assignment);

        assert_eq!(context.syms().count(), 1);
    }
}
