use crate::{Alloc, Context, Expr, ExprDef, Sort, SortDef, swrite, vcgen::VerificationCondition};

mod string_write {
    #[macro_export]
    macro_rules! swrite {
        ($destination:expr, $($format_args:tt)*) => {{
            // Compile-time check: `$destination` must be `&mut String`.
            let destination: &mut ::std::string::String = $destination;
            unsafe{
                ::std::fmt::Write::write_fmt(
                    destination,
                    ::std::format_args!($($format_args)*),
                ).unwrap_unchecked();
            }
        }};
    }
}

pub fn format_expr(sink: &mut String, ctxt: &Context, expr: Expr) {
    match ctxt.get(expr) {
        ExprDef::Sym(var) => swrite!(sink, "{}", ctxt.get(var).name),
        ExprDef::Const(value) => swrite!(sink, "{}", value),
        ExprDef::Call { func, arg } => {
            swrite!(sink, "({} ", &ctxt.get(func).name,);
            format_expr(sink, ctxt, arg);
            swrite!(sink, ")");
        }
        ExprDef::Unary { op, expr } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr(sink, ctxt, expr);
            swrite!(sink, ")");
        }
        ExprDef::Binary { op, lhs, rhs } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr(sink, ctxt, lhs);
            swrite!(sink, " ");
            format_expr(sink, ctxt, rhs);
            swrite!(sink, ")");
        }
    }
}

pub fn format_sort(sink: &mut String, ctxt: &Context, sort: Sort) {
    match ctxt.get(sort) {
        SortDef::Int => swrite!(sink, "Int"),
        SortDef::Bool => swrite!(sink, "Bool"),
        SortDef::Arrow(l, r) => {
            swrite!(sink, "(");
            format_sort(sink, ctxt, l);
            swrite!(sink, ") ");
            format_sort(sink, ctxt, r);
        }
    }
}

fn sort_type(ctxt: &Context, sort: Sort) -> &'static str {
    match ctxt.get(sort) {
        SortDef::Int => "const",
        SortDef::Bool => "const",
        SortDef::Arrow(_, _) => "fun",
    }
}

pub fn smt(ctxt: &Context, vc: VerificationCondition) -> String {
    let mut result = String::new();
    let sink = &mut result;

    swrite!(sink, "(set-logic QF_UFLIA)\n");

    for sym in ctxt.syms() {
        swrite!(sink, "(declare-{} {} ", sort_type(ctxt, sym.sort), sym.name);
        format_sort(sink, ctxt, sym.sort);
        swrite!(sink, ")");
    }

    swrite!(sink, "\n");

    for expr in vc.assume {
        swrite!(sink, "(assert \n");
        format_expr(sink, ctxt, expr);
        swrite!(sink, ")\n");
    }

    swrite!(sink, "(assert (not \n");
    format_expr(sink, ctxt, vc.goal);
    swrite!(sink, "))\n");

    swrite!(sink, "\n(check-sat)\n(get-model)\n");

    result
}
