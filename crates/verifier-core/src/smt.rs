//! SMT-LIB rendering for symbolic terms and verification conditions.

use std::fmt::Display;

use fixedbitset::FixedBitSet;

use crate::{
    Context, Declaration, DefStore, Environment, Field, Name, Op, Sort, SortDef, Term, TermKind,
    Uop, Var, swrite,
};

mod string_write {
    #[macro_export]
    macro_rules! swrite {
        ($destination:expr, $($format_args:tt)*) => {{
            let destination: &mut ::std::string::String = $destination;
            ::std::fmt::Write::write_fmt(destination, ::std::format_args!($($format_args)*))
                .expect("infallible");
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

pub fn format_expr(sink: &mut String, ctxt: &Context, environment: &Environment<Name>, expr: Term) {
    match ctxt.get(expr).kind {
        TermKind::Var(var) => format_var(sink, ctxt, environment, var),
        TermKind::Const(value) => swrite!(sink, "{}", value),
        TermKind::Bool(value) => swrite!(sink, "{}", value),
        TermKind::Call { function, arguments } => {
            swrite!(sink, "(");
            format_var(sink, ctxt, environment, function);
            for argument in arguments {
                swrite!(sink, " ");
                format_expr(sink, ctxt, environment, *argument);
            }
            swrite!(sink, ")");
        }
        TermKind::Unary { op, expr } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr(sink, ctxt, environment, expr);
            swrite!(sink, ")");
        }
        TermKind::Binary { op, lhs, rhs } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr(sink, ctxt, environment, lhs);
            swrite!(sink, " ");
            format_expr(sink, ctxt, environment, rhs);
            swrite!(sink, ")");
        }
        TermKind::Unit => swrite!(sink, "tuple0"),
        TermKind::Tuple(fields) => {
            swrite!(sink, "(tuple{}", fields.len());
            for field in fields {
                swrite!(sink, " ");
                format_expr(sink, ctxt, environment, *field);
            }
            swrite!(sink, ")");
        }
        TermKind::Proj { tuple, field } => {
            let sort =
                environment.cached_sort(tuple).expect("unchecked tuple reached SMT emission");
            let SortDef::Tuple(fields) = ctxt.get(sort) else {
                panic!("projection from non-tuple term reached SMT emission")
            };
            swrite!(sink, "(tuple{}!{} ", fields.len(), field);
            format_expr(sink, ctxt, environment, tuple);
            swrite!(sink, ")");
        }
    }
}

fn format_var(sink: &mut String, ctxt: &Context, environment: &Environment<Name>, var: Var) {
    swrite!(sink, "{}!{}", ctxt.get(*environment.binding(var)), var.index());
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
    }
}

fn collect_tuple_arities(arities: &mut FixedBitSet, ctxt: &Context, sort: Sort) {
    let SortDef::Tuple(fields) = ctxt.get(sort) else { return };
    arities.grow(fields.len() + 1);
    arities.insert(fields.len());
    for field in fields {
        collect_tuple_arities(arities, ctxt, *field);
    }
}

fn declare_tuples(sink: &mut String, ctxt: &Context, environment: &Environment<Name>) {
    let mut arities = FixedBitSet::with_capacity(64);
    for (_, declaration, _) in environment.iter() {
        match declaration {
            Declaration::Value(sort) => collect_tuple_arities(&mut arities, ctxt, *sort),
            Declaration::Function { domain, range } => {
                for sort in domain {
                    collect_tuple_arities(&mut arities, ctxt, *sort);
                }
                collect_tuple_arities(&mut arities, ctxt, *range);
            }
        }
    }
    for sort in environment.cached_sorts() {
        collect_tuple_arities(&mut arities, ctxt, sort);
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

pub fn smt(ctxt: &Context, environment: &Environment<Name>, vc: Term) -> String {
    let sort = environment.cached_sort(vc).expect("unchecked verification condition");
    assert_eq!(ctxt.get(sort), SortDef::Bool, "verification condition must be Bool");

    let mut result = String::new();
    let sink = &mut result;
    swrite!(sink, "(set-logic ALL)\n\n");
    declare_tuples(sink, ctxt, environment);

    for (var, declaration, _) in environment.iter() {
        match declaration {
            Declaration::Value(sort) => {
                swrite!(sink, "(declare-const ");
                format_var(sink, ctxt, environment, var);
                swrite!(sink, " ");
                format_sort(sink, ctxt, *sort);
                swrite!(sink, ")\n");
            }
            Declaration::Function { domain, range } => {
                swrite!(sink, "(declare-fun ");
                format_var(sink, ctxt, environment, var);
                swrite!(sink, " (");
                for (index, sort) in domain.iter().enumerate() {
                    if index > 0 {
                        swrite!(sink, " ");
                    }
                    format_sort(sink, ctxt, *sort);
                }
                swrite!(sink, ") ");
                format_sort(sink, ctxt, *range);
                swrite!(sink, ")\n");
            }
        }
    }

    swrite!(sink, "\n(assert (not ");
    format_expr(sink, ctxt, environment, vc);
    swrite!(sink, "))\n\n(check-sat)\n(get-model)\n");
    result
}

#[cfg(test)]
mod tests {
    use crate::{Context, Environment, format_expr, smt};

    #[test]
    fn formats_full_width_integer_constants() {
        let mut context = Context::default();
        let mut environment = Environment::new();
        let value = context.builder(&mut environment).int_lit(i128::MIN);
        let mut formatted = String::new();
        format_expr(&mut formatted, &context, &environment, value);
        assert_eq!(formatted, i128::MIN.to_string());
    }

    #[test]
    fn distinguishes_variables_with_the_same_name() {
        let mut context = Context::default();
        let int = context.int_sort();
        let name = context.name("x");
        let mut environment = Environment::new();
        let first = environment.bind_value(int, name);
        let second = environment.bind_value(int, name);
        let mut terms = context.builder(&mut environment);
        let first = terms.var(first);
        let second = terms.var(second);
        let equality = terms.eq(first, second);
        let output = smt(&context, &environment, equality);
        assert!(output.contains("(declare-const x!0 Int)"));
        assert!(output.contains("(declare-const x!1 Int)"));
        assert!(output.contains("(= x!0 x!1)"));
    }

    #[test]
    fn declares_functions_and_projects_tuples() {
        let mut context = Context::default();
        let int = context.int_sort();
        let bool = context.bool_sort();
        let unit = context.unit_sort();
        let pair = context.tuple_sort(&[int, bool]);
        let pair_name = context.name("pair");
        let unit_name = context.name("unit");
        let function_name = context.name("f");
        let mut environment = Environment::new();
        let pair = environment.bind_value(pair, pair_name);
        let unit = environment.bind_value(unit, unit_name);
        let function = environment.bind_function(&[int], int, function_name);
        let mut terms = context.builder(&mut environment);
        let pair = terms.var(pair);
        let unit = terms.var(unit);
        let first = terms.proj(pair, 0);
        let one = terms.int_lit(1);
        let called = terms.call(function, &[one]);
        let call_holds = terms.eq(called, one);
        let pair_holds = terms.eq(first, one);
        let unit_value = terms.unit();
        let unit_holds = terms.eq(unit, unit_value);
        let tail = terms.and(pair_holds, unit_holds);
        let vc = terms.and(call_holds, tail);
        let output = smt(&context, &environment, vc);
        assert!(output.contains("(declare-datatype Tuple0 ((tuple0)))"));
        assert!(output.contains(
            "(declare-datatype Tuple2 (par (T0 T1) ((tuple2 (tuple2!0 T0) (tuple2!1 T1)))))"
        ));
        assert!(output.contains("(declare-const pair!0 (Tuple2 Int Bool))"));
        assert!(output.contains("(declare-fun f!2 (Int) Int)"));
        assert!(output.contains("(tuple2!0 pair!0)"));
        assert!(output.contains("(= unit!1 tuple0)"));
    }
}
