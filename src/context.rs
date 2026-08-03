use crate::{Expr, ExprDef, Stmt, StmtDef, Var, Vec};
use std::ops::{Index, IndexMut};

pub trait Alloc<D> {
    type Idx;

    fn alloc(&mut self, def: D) -> Self::Idx;
}

#[derive(Default)]
pub struct Context {
    stmts: Vec<Stmt, StmtDef>,
    exprs: Vec<Expr, ExprDef>,
    vars: Vec<Var, Box<str>>,
}

impl Alloc<ExprDef> for Context {
    type Idx = Expr;

    fn alloc(&mut self, expr: ExprDef) -> Expr {
        self.exprs.push(expr)
    }
}

impl Alloc<StmtDef> for Context {
    type Idx = Stmt;

    fn alloc(&mut self, stmt: StmtDef) -> Stmt {
        self.stmts.push(stmt)
    }
}

impl Alloc<Box<str>> for Context {
    type Idx = Var;

    fn alloc(&mut self, name: Box<str>) -> Var {
        self.vars.push(name)
    }
}

impl Index<Expr> for Context {
    type Output = ExprDef;

    fn index(&self, index: Expr) -> &Self::Output {
        &self.exprs[index]
    }
}

impl IndexMut<Expr> for Context {
    fn index_mut(&mut self, index: Expr) -> &mut Self::Output {
        &mut self.exprs[index]
    }
}

impl Index<Stmt> for Context {
    type Output = StmtDef;

    fn index(&self, index: Stmt) -> &Self::Output {
        &self.stmts[index]
    }
}

impl IndexMut<Stmt> for Context {
    fn index_mut(&mut self, index: Stmt) -> &mut Self::Output {
        &mut self.stmts[index]
    }
}
