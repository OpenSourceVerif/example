use crate::{swrite, Context, TermDef, Intern, Op, Sort, SortDef, Term, Uop};

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

pub fn format_expr(sink: &mut String, ctxt: &Context, expr: Term) {
    match ctxt.get(expr) {
        TermDef::Sym(var) => swrite!(sink, "{}", ctxt.get(var).name),
        TermDef::Const(value) => swrite!(sink, "{}", value),
        TermDef::Bool(value) => swrite!(sink, "{}", value),
        TermDef::Call { func, arg } => {
            swrite!(sink, "({} ", &ctxt.get(func).name,);
            format_expr(sink, ctxt, arg);
            swrite!(sink, ")");
        }
        TermDef::Unary { op, expr } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr(sink, ctxt, expr);
            swrite!(sink, ")");
        }
        TermDef::Binary { op, lhs, rhs } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr(sink, ctxt, lhs);
            swrite!(sink, " ");
            format_expr(sink, ctxt, rhs);
            swrite!(sink, ")");
        }
        TermDef::Unit => swrite!(sink, "()"),
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

pub fn smt(ctxt: &Context, vc: Term) -> String {
    let mut result = String::new();
    let sink = &mut result;

    // Symbolic MIR can contain multiplication of two symbolic integers.
    swrite!(sink, "(set-logic ALL)\n\n");

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

#[cfg(test)]
mod tests {
    use crate::{format_expr, Context};

    #[test]
    fn formats_full_width_integer_constants() {
        let mut context = Context::default();
        let value = context.int_lit(i128::MIN);
        let mut formatted = String::new();

        format_expr(&mut formatted, &context, value);

        assert_eq!(formatted, i128::MIN.to_string());
    }
}
