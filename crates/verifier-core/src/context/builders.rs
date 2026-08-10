//! Shorthand for intern method.

use crate::{
    Context, Intern, Op,
    Op::*,
    Sort, SortDef,
    SortDef::*,
    Sym, Term,
    TermDef::{self, *},
    Uop::{self, *},
};

macro_rules! define_builders {
    ($($return:ident {
        $($method:ident ($($argument:ident : $type:ty),* $(,)?) $definition:expr;)*
    })*) => {$(
        impl Context {
            $(pub fn $method(&mut self, $($argument: $type),*) -> $return {
                self.intern($definition)
            })*
        }
    )*};
}

define_builders! {

Term {
    var(index: usize) Var(index);
    sym(sym: Sym) Sym(sym);
    int_lit(value: i128) Const(value);
    bool_lit(value: bool) TermDef::Bool(value);
    unit() Unit;
    tuple(fields: &[Term]) Tuple(fields);
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

Sort {
    int_sort() Int;
    bool_sort() SortDef::Bool;
    arrow(domain: Sort, range: Sort) Arrow(domain, range);
}

}
