use crate::{Context, Expr, ExprDef, Intern, Program, Stmt, StmtDef, Sym, vcgen::VC};

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

                self.binary(op, lhs, rhs)
            }
            ExprDef::Unary { op, expr } => {
                let expr = self.subst(expr, sym, replacement);
                self.unary(op, expr)
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

    pub fn vc_by_wp(&mut self, Program { body, requires, ensures }: Program) -> VC {
        let transformed_ensures = self.wp(body, ensures);

        self.implies(requires, transformed_ensures)
    }
}
