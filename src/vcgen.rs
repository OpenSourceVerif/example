use crate::Program;

use super::{Context, Expr, ExprDef, ExprDef::*, Intern, Stmt, StmtDef, Sym};

pub struct Obligation {
    pub assume: Box<[Expr]>,
    pub goal: Expr,
}

impl Context {
    pub fn subst(&mut self, expr: Expr, sym: Sym, replacement: Expr) -> Expr {
        match self.get(expr) {
            ExprDef::Sym(find) if find == sym => replacement,
            ExprDef::Sym(_) => expr,
            ExprDef::Const(_) => expr,
            ExprDef::Bool(_) => expr,
            ExprDef::Binary { lhs, rhs, op } => {
                let lhs = self.subst(lhs, sym, replacement);
                let rhs = self.subst(rhs, sym, replacement);

                self.intern(Binary { lhs, rhs, op })
            }
            ExprDef::Unary { op, expr } => {
                let expr = self.subst(expr, sym, replacement);
                self.intern(Unary { expr, op })
            }
            ExprDef::Call { func, arg } => {
                let arg = self.subst(arg, sym, replacement);
                self.call(func, arg)
            }
        }
    }
}

// {ret} stmt {k}
pub fn wp(ctxt: &mut Context, stmt: Stmt, k: Expr) -> Expr {
    match ctxt.get(stmt) {
        StmtDef::Skip => k,
        StmtDef::Seq { first, second } => {
            let intermediate = wp(ctxt, second, k);
            wp(ctxt, first, intermediate)
        }
        StmtDef::If { cond, then_branch, else_branch } => {
            let then_requires = wp(ctxt, then_branch, k);
            let else_requires = wp(ctxt, else_branch, k);

            let then = ctxt.implies(cond, then_requires);
            let not_cond = ctxt.not(cond);
            let else_ = ctxt.implies(not_cond, else_requires);
            ctxt.and(then, else_)
        }
        StmtDef::Assign { var, def } => ctxt.subst(k, var, def),
        StmtDef::Assert(expr) => ctxt.and(k, expr),
    }
}

pub fn vc(ctxt: &mut Context, Program { body, requires, ensures }: Program) -> Obligation {
    let encoded_body = wp(ctxt, body, ensures);

    Obligation { assume: requires, goal: encoded_body }
}
