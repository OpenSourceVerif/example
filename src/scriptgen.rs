use crate::{
    Context, Expr, ExprDef, Intern, Op, Sort, SortDef, Uop, swrite,
};

mod string_write {
    #[macro_export]
    macro_rules! swrite {
        ($destination:expr, $($format_args:tt)*) => {{
            // Compile-time check: `$destination` must be `&mut String`.
            let destination: &mut ::std::string::String = $destination;

            ::std::fmt::Write::write_fmt(
                destination,
                ::std::format_args!($($format_args)*),
            ).expect("infallible");

        }};
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

pub fn format_expr(sink: &mut String, ctxt: &Context, expr: Expr) {
    match ctxt.get(expr) {
        ExprDef::Sym(var) => swrite!(sink, "{}", ctxt.get(var).name),
        ExprDef::Const(value) => swrite!(sink, "{}", value),
        ExprDef::Bool(value) => swrite!(sink, "{}", value),
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

pub fn smt(ctxt: &Context, vc: Expr) -> String {
    let mut result = String::new();
    let sink = &mut result;

    swrite!(sink, "(set-logic QF_UFLIA)\n\n");

    for sym in ctxt.syms() {
        swrite!(sink, "(declare-{} {} ", sort_type(ctxt, sym.sort), sym.name);
        format_sort(sink, ctxt, sym.sort);
        swrite!(sink, ")\n");
    }

    swrite!(sink, "\n");

    swrite!(sink, "(assert (not ");
    format_expr(sink, ctxt, vc);
    swrite!(sink, "))\n");

    swrite!(sink, "\n(check-sat)\n(get-model)\n");

    result
}
