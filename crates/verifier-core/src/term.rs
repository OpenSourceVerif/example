//! Syntax-only constructors for interned terms.
//!
//! These functions canonicalize syntax but do not check it under an
//! [`crate::Environment`]. Boundaries which require a sorting judgment call
//! [`crate::Environment::sort`].

use crate::{Field, Fields, Intern, Op, Term, TermDef, Uop, Var, def};

macro_rules! binary {
    ($($name:ident: $op:ident),* $(,)?) => {$(
        pub fn $name(lhs: Term, rhs: Term) -> Term {
            binary(Op::$op, lhs, rhs)
        }
    )*};
}

pub fn var(var: Var) -> Term {
    TermDef::Var(var).intern()
}

pub fn int(value: i128) -> Term {
    TermDef::Const(value).intern()
}

pub fn bool(value: bool) -> Term {
    TermDef::Bool(value).intern()
}

pub fn unit() -> Term {
    TermDef::Unit.intern()
}

pub fn tuple(fields: &[Term]) -> Term {
    if fields.is_empty() {
        return unit();
    }
    TermDef::Tuple(Fields::new(fields)).intern()
}

pub fn proj(tuple: Term, field: impl Into<Field>) -> Term {
    let field = field.into();
    def!(let definition = tuple);
    if let TermDef::Tuple(fields) = *definition {
        return fields[field];
    }
    TermDef::Proj { tuple, field }.intern()
}

pub fn call(function: Var, arguments: &[Term]) -> Term {
    TermDef::Call { function, arguments: Fields::new(arguments) }.intern()
}

pub fn binary(op: Op, lhs: Term, rhs: Term) -> Term {
    TermDef::Binary { op, lhs, rhs }.intern()
}

pub fn unary(op: Uop, expr: Term) -> Term {
    TermDef::Unary { op, expr }.intern()
}

binary! {
    add: Add,
    sub: Sub,
    mul: Mul,
    eq: Eq,
    ne: Ne,
    lt: Lt,
    le: Le,
    gt: Gt,
    ge: Ge,
    and: And,
    or: Or,
    implies: Implies,
}

pub fn not(expr: Term) -> Term {
    unary(Uop::Not, expr)
}

pub fn neg(expr: Term) -> Term {
    unary(Uop::Neg, expr)
}

#[cfg(test)]
mod tests {
    use crate::{Fields, INTERNERS, Intern, Interners, Op, TermDef};

    use super::{add, bool, int, proj, tuple, unit};

    #[test]
    fn constructors_only_intern_syntax() {
        let interners = Interners::default();
        let body = || {
            let one = int(1);
            assert_eq!(add(one, one), TermDef::Binary { op: Op::Add, lhs: one, rhs: one }.intern());
        };
        // SAFETY: `body` is synchronous and discards all arena values.
        unsafe { INTERNERS.set(&interners, body) }
    }

    #[test]
    fn normalizes_tuple_syntax() {
        let interners = Interners::default();
        let body = || {
            let one = int(1);
            let yes = bool(true);
            let pair = tuple(&[one, yes]);

            assert_eq!(tuple(&[]), unit());
            assert_eq!(proj(pair, 0), one);
            assert_eq!(proj(pair, 1), yes);
            assert_eq!(pair, TermDef::Tuple(Fields::new(&[one, yes])).intern());
        };
        // SAFETY: `body` is synchronous and discards all arena values.
        unsafe { INTERNERS.set(&interners, body) }
    }
}
