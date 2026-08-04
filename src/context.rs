use crate::{
    Expr, ExprDef, Name, Sort, SortDef, Stmt, StmtDef, Sym, SymDef, SymDefInterned, Vec,
    string_interner::StringInterner,
};

pub trait Alloc<I, D> {
    type Ref<'a>
    where
        Self: 'a;

    fn alloc(&mut self, def: D) -> I;

    fn get<'a>(&'a self, idx: I) -> Self::Ref<'a>;
}

#[derive(Default)]
pub struct Context {
    stmts: Vec<Stmt, StmtDef>,
    exprs: Vec<Expr, ExprDef>,
    sym_names: StringInterner<Name>,
    syms: Vec<Sym, SymDefInterned>,
    sorts: Vec<Sort, SortDef>,
}

impl Context {
    pub fn syms<'a>(&'a self) -> impl Iterator<Item = SymDef<'a>> {
        self.syms.iter().map(|SymDefInterned { name, sort }| {
            let name = &self.sym_names[*name];

            SymDef { name, sort: *sort }
        })
    }
}

impl Alloc<Expr, ExprDef> for Context {
    type Ref<'a>
        = ExprDef
    where
        Self: 'a;

    fn alloc(&mut self, expr: ExprDef) -> Expr {
        self.exprs.push(expr)
    }

    fn get(&self, idx: Expr) -> ExprDef {
        self.exprs[idx]
    }
}

impl Alloc<Stmt, StmtDef> for Context {
    type Ref<'a>
        = StmtDef
    where
        Self: 'a;

    fn alloc(&mut self, stmt: StmtDef) -> Stmt {
        self.stmts.push(stmt)
    }

    fn get(&self, idx: Stmt) -> StmtDef {
        self.stmts[idx]
    }
}

impl<'input> Alloc<Sym, SymDef<'input>> for Context {
    type Ref<'a>
        = SymDef<'a>
    where
        Self: 'a;

    fn alloc(&mut self, sym: SymDef<'input>) -> Sym {
        let name = self.sym_names.intern(sym.name);
        self.syms.push(SymDefInterned { name, sort: sym.sort })
    }

    fn get<'a>(&'a self, idx: Sym) -> SymDef<'a> {
        let SymDefInterned { name, sort } = self.syms[idx];
        let name = &self.sym_names[name];

        SymDef { name, sort }
    }
}

impl Alloc<Sort, SortDef> for Context {
    type Ref<'a>
        = SortDef
    where
        Self: 'a;

    fn alloc(&mut self, sort: SortDef) -> Sort {
        self.sorts.push(sort)
    }

    fn get(&self, idx: Sort) -> SortDef {
        self.sorts[idx]
    }
}
