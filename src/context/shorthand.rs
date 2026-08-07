use crate::{
    Context,
    TermDef::{self, *},
    Intern, Op,
    Op::*,
    Sort, SortDef,
    SortDef::*,
    Sym, SymDef, Term,
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

Term {
    sym(sym: Sym) Sym(sym);
    int_lit(value: i128) Const(value);
    bool_lit(value: bool) TermDef::Bool(value);

    call(func: Sym, arg: Term) Call { func, arg };

    binary(op: Op, lhs: Term, rhs: Term) Binary { op, lhs, rhs };
    add(lhs: Term, rhs: Term) Binary { op: Add, lhs, rhs };
    sub(lhs: Term, rhs: Term) Binary { op: Sub, lhs, rhs };
    mul(lhs: Term, rhs: Term) Binary { op: Mul, lhs, rhs };
    eq(lhs: Term, rhs: Term) Binary { op: Eq, lhs, rhs };
    ne(lhs: Term, rhs: Term) Binary { op: Ne, lhs, rhs };
    lt(lhs: Term, rhs: Term) Binary { op: Lt, lhs, rhs };
    le(lhs: Term, rhs: Term) Binary { op: Le, lhs, rhs };
    gt(lhs: Term, rhs: Term) Binary { op: Gt, lhs, rhs };
    ge(lhs: Term, rhs: Term) Binary { op: Ge, lhs, rhs };
    and(lhs: Term, rhs: Term) Binary { op: And, lhs, rhs };
    or(lhs: Term, rhs: Term) Binary { op: Or, lhs, rhs };
    implies(lhs: Term, rhs: Term) Binary { op: Implies, lhs, rhs };

    unary(op: Uop, expr: Term) Unary { op, expr };
    not(expr: Term) Unary { op: Not, expr };
    neg(expr: Term) Unary { op: Neg, expr };
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
