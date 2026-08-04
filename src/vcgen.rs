use super::{Alloc, Context, Expr, ExprDef, Op, Stmt, StmtDef, Sym, Uop};

pub struct VerificationCondition {
    pub assume: Box<[Expr]>,
    pub goal: Expr,
}

pub fn subst(ctxt: &mut Context, expr: Expr, sym: Sym, replacement: Expr) -> Expr {
    match ctxt.get(expr) {
        ExprDef::Sym(find) if find == sym => replacement,
        ExprDef::Sym(_) => expr,
        ExprDef::Const(_) => expr,
        ExprDef::Binary { lhs, rhs, op } => {
            let lhs = subst(ctxt, lhs, sym, replacement);
            let rhs = subst(ctxt, rhs, sym, replacement);

            ctxt.alloc(ExprDef::Binary { lhs, rhs, op })
        }
        ExprDef::Unary { op, expr } => {
            let expr = subst(ctxt, expr, sym, replacement);
            ctxt.alloc(ExprDef::Unary { expr, op })
        }
        ExprDef::Call { func, arg } => {
            let arg = subst(ctxt, arg, sym, replacement);
            ctxt.alloc(ExprDef::Call { func, arg })
        }
    }
}

// {ret} stmt {k}
pub fn wp(ctxt: &mut Context, stmt: Stmt, k: Expr) -> Expr {
    use ExprDef::*;
    use Op::*;
    use Uop::*;

    match ctxt.get(stmt) {
        StmtDef::Skip => k,
        StmtDef::Seq { first, second } => {
            let intermediate = wp(ctxt, second, k);
            wp(ctxt, first, intermediate)
        }
        StmtDef::If { cond, then_branch, else_branch } => {
            let then_requires = wp(ctxt, then_branch, k);
            let else_requires = wp(ctxt, else_branch, k);

            let then = ctxt.alloc(Binary { lhs: cond, rhs: then_requires, op: Implies });
            let not_cond = ctxt.alloc(Unary { op: Not, expr: cond });
            let else_ = ctxt.alloc(Binary { lhs: not_cond, rhs: else_requires, op: Implies });
            ctxt.alloc(Binary { lhs: then, rhs: else_, op: And })
        }
        StmtDef::Assign { var, def } => subst(ctxt, k, var, def),
    }
}

pub fn vc(
    ctxt: &mut Context,
    requires: Box<[Expr]>,
    body: Stmt,
    ensures: Expr,
) -> VerificationCondition {
    let encoded_body = wp(ctxt, body, ensures);

    VerificationCondition { assume: requires, goal: encoded_body }
}
