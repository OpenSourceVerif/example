use crate::{
    Context, Expr, ExprDef, ExprDef::*, Intern, Op, Op::*, Sort, SortDef, SortDef::*, Stmt,
    StmtDef::*, Sym, SymDef, Uop, Uop::*,
};

// Entries are munched one at a time because `||` lexes as a single token,
// so a `| ... |` pattern with an empty argument list can never match it.
macro_rules! define_constructors {
($($output:ident { $($body:tt)* })*) => {
    $(define_constructors! { @entries $output $($body)* })*
};
(@entries $output:ident) => {};
(@entries $output:ident $method:ident, || $definition:expr; $($rest:tt)*) => {
    impl Context {
        pub fn $method(&mut self) -> $output {
            self.intern($definition)
        }
    }
    define_constructors! { @entries $output $($rest)* }
};
(
    @entries $output:ident
    $method:ident,
    |$($arg:ident : $arg_ty:ty),* $(,)?|
    $definition:expr;
    $($rest:tt)*
) => {
    impl Context {
        pub fn $method(
            &mut self,
            $($arg: $arg_ty),*
        ) -> $output {
            self.intern($definition)
        }
    }
    define_constructors! { @entries $output $($rest)* }
};
}

define_constructors! {
Expr {
    sym, |sym: Sym| Sym(sym);
    int_lit, |value: i32| Const(value);
    bool_lit, |value: bool| ExprDef::Bool(value);

    call, |func: Sym, arg: Expr| Call { func, arg };

    binary, |op: Op, lhs: Expr, rhs: Expr| Binary { op, lhs, rhs };
    add, |lhs: Expr, rhs: Expr| Binary { op: Add, lhs, rhs };
    sub, |lhs: Expr, rhs: Expr| Binary { op: Sub, lhs, rhs };
    and, |lhs: Expr, rhs: Expr| Binary { op: And, lhs, rhs };
    gt, |lhs: Expr, rhs: Expr| Binary { op: Gt, lhs, rhs };
    ge, |lhs: Expr, rhs: Expr| Binary { op: Ge, lhs, rhs };
    le, |lhs: Expr, rhs: Expr| Binary { op: Le, lhs, rhs };
    implies, |lhs: Expr, rhs: Expr| Binary { op: Implies, lhs, rhs };

    unary, |op: Uop, expr: Expr| Unary { op, expr };
    not, |expr: Expr| Unary { op: Not, expr };
    neg, |expr: Expr| Unary { op: Neg, expr };
}

Stmt {
    skip, || Skip;
    assign, |var: Sym, def: Expr| Assign { var, def };
    seq, |first: Stmt, second: Stmt| Seq { first, second };
    if_, |cond: Expr, then_branch: Stmt, else_branch: Stmt| If { cond, then_branch, else_branch };
    assert, |expr: Expr| Assert(expr);
}

Sym {
    symbol, |name: &str, sort: Sort| SymDef { name, sort };
}

Sort {
    int_sort, || Int;
    bool_sort, || SortDef::Bool;
    arrow, |domain: Sort, range: Sort| Arrow( domain, range );
}
}
