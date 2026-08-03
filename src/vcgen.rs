use super::{Alloc, Context, Expr, ExprDef, Op, Stmt, StmtDef, Uop, Var};

pub fn subst(ctxt: &mut Context, expr: Expr, var: Var, replacement: Expr) -> Expr {
    match ctxt[expr] {
        ExprDef::Var(find) if find == var => replacement,
        ExprDef::Var(_) => expr,
        ExprDef::Const(_) => expr,
        ExprDef::Binary { lhs, rhs, op } => {
            let lhs = subst(ctxt, lhs, var, replacement);
            let rhs = subst(ctxt, rhs, var, replacement);

            ctxt.alloc(ExprDef::Binary { lhs, rhs, op })
        }
        ExprDef::Unary { op, expr } => {
            let expr = subst(ctxt, expr, var, replacement);
            ctxt.alloc(ExprDef::Unary { expr, op })
        }
    }
}

// {ret} s {k}
pub fn wp(ctxt: &mut Context, s: Stmt, k: Expr) -> Expr {
    use ExprDef::*;
    use Op::*;
    use Uop::*;

    match ctxt[s] {
        StmtDef::Seq(first, second) => {
            let intermediate = wp(ctxt, second, k);
            wp(ctxt, first, intermediate)
        }
        StmtDef::If { cond, then_branch, else_branch } => {
            let then_requires = wp(ctxt, then_branch, k);
            let else_requires = wp(ctxt, else_branch, k);

            let then = ctxt.alloc(Binary { lhs: cond, rhs: then_requires, op: And });
            let else_ = ctxt.alloc(Unary { op: Not, expr: cond });
            let else_ = ctxt.alloc(Binary { lhs: else_, rhs: else_requires, op: And });
            ctxt.alloc(ExprDef::Binary { lhs: then, rhs: else_, op: And })
        }
        StmtDef::Assign { var, def } => subst(ctxt, k, var, def),
    }
}
