//! SMT-LIB rendering for symbolic terms and verification conditions.

use std::fmt::Display;

use fixedbitset::FixedBitSet;

use crate::{Context, DefStore, Field, Op, Sort, SortDef, Term, TermKind, Uop, swrite};

mod string_write {
    #[macro_export]
    macro_rules! swrite {
        ($destination:expr, $($format_args:tt)*) => {{
            let destination: &mut ::std::string::String = $destination;

            ::std::fmt::Write::write_fmt(
                destination,
                ::std::format_args!($($format_args)*),
            ).expect("infallible");
        }};
    }
}

impl Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.index())
    }
}

impl Op {
    pub const fn smt_style(&self) -> &'static str {
        match self {
            Op::Implies => "=>",
            Op::Or => "or",
            Op::And => "and",
            Op::Eq => "=",
            Op::Ne => "distinct",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
        }
    }
}

impl Uop {
    pub const fn smt_style(&self) -> &'static str {
        match self {
            Uop::Not => "not",
            Uop::Neg => "-",
        }
    }
}

pub fn format_expr(sink: &mut String, ctxt: &Context, expr: Term) {
    match ctxt.get(expr).kind {
        TermKind::Param(index) => panic!("open term parameter {index} reached SMT emission"),
        TermKind::Sym(sym) => format_sym(sink, ctxt, sym),
        TermKind::Const(value) => swrite!(sink, "{}", value),
        TermKind::Bool(value) => swrite!(sink, "{}", value),
        TermKind::Call { func, arg } => {
            swrite!(sink, "(");
            format_sym(sink, ctxt, func);
            swrite!(sink, " ");
            format_expr(sink, ctxt, arg);
            swrite!(sink, ")");
        }
        TermKind::Unary { op, expr } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr(sink, ctxt, expr);
            swrite!(sink, ")");
        }
        TermKind::Binary { op, lhs, rhs } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr(sink, ctxt, lhs);
            swrite!(sink, " ");
            format_expr(sink, ctxt, rhs);
            swrite!(sink, ")");
        }
        TermKind::Unit => swrite!(sink, "tuple0"),
        TermKind::Tuple(fields) => {
            swrite!(sink, "(tuple{}", fields.len());
            for field in fields {
                swrite!(sink, " ");
                format_expr(sink, ctxt, *field);
            }
            swrite!(sink, ")");
        }
        TermKind::Proj { tuple, field } => {
            let SortDef::Tuple(fields) = ctxt.get(ctxt.term_sort(tuple)) else {
                panic!("projection from non-tuple term reached SMT emission")
            };
            swrite!(sink, "(tuple{}!{} ", fields.len(), field);
            format_expr(sink, ctxt, tuple);
            swrite!(sink, ")");
        }
    }
}

fn format_sym(sink: &mut String, ctxt: &Context, sym: crate::Sym) {
    swrite!(sink, "{}!{}", ctxt.get(sym).name, sym.index());
}

pub fn format_sort(sink: &mut String, ctxt: &Context, sort: Sort) {
    match ctxt.get(sort) {
        SortDef::Int => swrite!(sink, "Int"),
        SortDef::Bool => swrite!(sink, "Bool"),
        SortDef::Tuple(fields) if fields.is_empty() => swrite!(sink, "Tuple0"),
        SortDef::Tuple(fields) => {
            swrite!(sink, "(Tuple{}", fields.len());
            for field in fields {
                swrite!(sink, " ");
                format_sort(sink, ctxt, *field);
            }
            swrite!(sink, ")");
        }
        SortDef::Arrow(domain, range) => {
            swrite!(sink, "(");
            format_sort(sink, ctxt, domain);
            swrite!(sink, ") ");
            format_sort(sink, ctxt, range);
        }
    }
}

fn sort_type(ctxt: &Context, sort: Sort) -> &'static str {
    match ctxt.get(sort) {
        SortDef::Int | SortDef::Bool | SortDef::Tuple(_) => "const",
        SortDef::Arrow(_, _) => "fun",
    }
}

fn declare_tuples(sink: &mut String, ctxt: &Context) {
    let mut arities = FixedBitSet::with_capacity(64);

    for (_, def) in ctxt.sorts() {
        if let SortDef::Tuple(fields) = def {
            let arity = fields.len();
            arities.grow(arity + 1);
            arities.insert(arity);
        }
    }

    for arity in arities.ones() {
        swrite!(sink, "(declare-datatype Tuple{} ", arity);
        if arity == 0 {
            swrite!(sink, "((tuple0)))\n");
            continue;
        }

        swrite!(sink, "(par (");
        for field in 0..arity {
            if field > 0 {
                swrite!(sink, " ");
            }
            swrite!(sink, "T{}", field);
        }
        swrite!(sink, ") ((tuple{}", arity);
        for field in 0..arity {
            swrite!(sink, " (tuple{}!{} T{})", arity, field, field);
        }
        swrite!(sink, "))))\n");
    }
}

pub fn smt(ctxt: &Context, vc: Term) -> String {
    assert_eq!(ctxt.get(ctxt.term_sort(vc)), SortDef::Bool, "verification condition must be Bool");

    let mut result = String::new();
    let sink = &mut result;

    // Symbolic MIR can contain multiplication of two symbolic integers.
    swrite!(sink, "(set-logic ALL)\n\n");
    declare_tuples(sink, ctxt);

    for (sym, def) in ctxt.syms() {
        swrite!(sink, "(declare-{} ", sort_type(ctxt, def.sort));
        format_sym(sink, ctxt, sym);
        swrite!(sink, " ");
        format_sort(sink, ctxt, def.sort);
        swrite!(sink, ")\n");
    }

    swrite!(sink, "\n(assert (not ");
    format_expr(sink, ctxt, vc);
    swrite!(sink, "))\n\n(check-sat)\n(get-model)\n");

    result
}

#[cfg(test)]
mod tests {
    use crate::{Context, format_expr, smt};

    #[test]
    fn formats_full_width_integer_constants() {
        let mut context = Context::default();
        let value = context.int_lit(i128::MIN);
        let mut formatted = String::new();

        format_expr(&mut formatted, &context, value);

        assert_eq!(formatted, i128::MIN.to_string());
    }

    #[test]
    fn distinguishes_symbols_with_the_same_name() {
        let mut context = Context::default();
        let int = context.int_sort();
        let first = context.symbol("x", int);
        let second = context.symbol("x", int);
        let first = context.sym(first);
        let second = context.sym(second);
        let equality = context.eq(first, second);

        let output = smt(&context, equality);

        assert!(output.contains("(declare-const x!0 Int)"));
        assert!(output.contains("(declare-const x!1 Int)"));
        assert!(output.contains("(= x!0 x!1)"));
    }

    #[test]
    fn declares_and_projects_tuples() {
        let mut context = Context::default();
        let int = context.int_sort();
        let bool = context.bool_sort();
        let unit = context.unit_sort();
        let pair = context.tuple_sort(&[int, bool]);
        let pair = context.symbol("pair", pair);
        let unit = context.symbol("unit", unit);
        let pair = context.sym(pair);
        let unit = context.sym(unit);
        let first = context.proj(pair, 0);
        let one = context.int_lit(1);
        let pair_holds = context.eq(first, one);
        let unit_value = context.unit();
        let unit_holds = context.eq(unit, unit_value);
        let vc = context.and(pair_holds, unit_holds);

        let output = smt(&context, vc);

        assert!(output.contains("(declare-datatype Tuple0 ((tuple0)))"));
        assert!(output.contains(
            "(declare-datatype Tuple2 (par (T0 T1) ((tuple2 (tuple2!0 T0) (tuple2!1 T1)))))"
        ));
        assert!(output.contains("(declare-const pair!0 (Tuple2 Int Bool))"));
        assert!(output.contains("(tuple2!0 pair!0)"));
        assert!(output.contains("(= unit!1 tuple0)"));
    }
}
