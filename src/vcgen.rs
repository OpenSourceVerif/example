use crate::Program;

use super::{Context, Expr, ExprDef, ExprDef::*, Intern, Stmt, StmtDef, Sym};

pub struct VerificationCondition {
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

    // {ret} stmt {k}
    pub fn wp(&mut self, stmt: Stmt, k: Expr) -> Expr {
        match self.get(stmt) {
            StmtDef::Skip => k,
            StmtDef::Seq { first, second } => {
                let intermediate = self.wp(second, k);
                self.wp(first, intermediate)
            }
            StmtDef::If { cond, then_branch, else_branch } => {
                let then_requires = self.wp(then_branch, k);
                let else_requires = self.wp(else_branch, k);

                let then = self.implies(cond, then_requires);
                let not_cond = self.not(cond);
                let else_ = self.implies(not_cond, else_requires);
                self.and(then, else_)
            }
            StmtDef::Assign { var, def } => self.subst(k, var, def),
            StmtDef::Assert(expr) => self.and(k, expr),
        }
    }

    pub fn vc(&mut self, Program { body, requires, ensures }: Program) -> VerificationCondition {
        let encoded_body = self.wp(body, ensures);

        VerificationCondition { assume: requires, goal: encoded_body }
    }
}
