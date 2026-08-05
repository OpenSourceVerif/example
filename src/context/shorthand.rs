use crate::{
    Context, Expr,
    ExprDef::{self, *},
    Intern, Op,
    Op::*,
    Sort, SortDef,
    SortDef::*,
    Stmt,
    StmtDef::*,
    Sym, SymDef,
    Uop::{self, *},
};

macro_rules! define_constructors {
    ($($output:ident {
        $($method:ident ($($arg:ident : $arg_ty:ty),* $(,)?) $definition:expr;)*
    })*) => {$(
        impl Context {
            $(pub fn $method(&mut self, $($arg: $arg_ty),*) -> $output {
                self.intern($definition)
            })*
        }
    )*};
}

define_constructors! {

Expr {
    sym(sym: Sym) Sym(sym);
    int_lit(value: i32) Const(value);
    bool_lit(value: bool) ExprDef::Bool(value);

    call(func: Sym, arg: Expr) Call { func, arg };

    binary(op: Op, lhs: Expr, rhs: Expr) Binary { op, lhs, rhs };
    add(lhs: Expr, rhs: Expr) Binary { op: Add, lhs, rhs };
    sub(lhs: Expr, rhs: Expr) Binary { op: Sub, lhs, rhs };
    mul(lhs: Expr, rhs: Expr) Binary { op: Mul, lhs, rhs };
    eq(lhs: Expr, rhs: Expr) Binary { op: Eq, lhs, rhs };
    ne(lhs: Expr, rhs: Expr) Binary { op: Ne, lhs, rhs };
    lt(lhs: Expr, rhs: Expr) Binary { op: Lt, lhs, rhs };
    le(lhs: Expr, rhs: Expr) Binary { op: Le, lhs, rhs };
    gt(lhs: Expr, rhs: Expr) Binary { op: Gt, lhs, rhs };
    ge(lhs: Expr, rhs: Expr) Binary { op: Ge, lhs, rhs };
    and(lhs: Expr, rhs: Expr) Binary { op: And, lhs, rhs };
    or(lhs: Expr, rhs: Expr) Binary { op: Or, lhs, rhs };
    implies(lhs: Expr, rhs: Expr) Binary { op: Implies, lhs, rhs };

    unary(op: Uop, expr: Expr) Unary { op, expr };
    not(expr: Expr) Unary { op: Not, expr };
    neg(expr: Expr) Unary { op: Neg, expr };
}

Stmt {
    skip() Skip;
    assign(var: Sym, def: Expr) Assign { var, def };
    seq(first: Stmt, second: Stmt) Seq { first, second };
    if_(cond: Expr, then_branch: Stmt, else_branch: Stmt) If {
        cond,
        then_branch,
        else_branch,
    };
    assert(expr: Expr) Assert(expr);
}

Sym {
    symbol(name: &str, sort: Sort) SymDef { name, sort };
}

Sort {
    int_sort() Int;
    bool_sort() SortDef::Bool;
    arrow(domain: Sort, range: Sort) Arrow(domain, range);
}

}
