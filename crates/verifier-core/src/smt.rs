//! SMT-LIB rendering for symbolic terms and verification conditions.

use std::fmt::Display;

use fixedbitset::FixedBitSet;

use crate::{
    Declaration, Environment, Field, INTERNERS, Intern, Interners, Name, Op, Sort, SortDef, Term,
    TermDef, TypeError, Uop, Var, scoped, swrite,
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

/// Checks and formats one symbolic expression as SMT-LIB.
pub fn format_expr(
    sink: &mut String,
    env: &Environment<Name>,
    expr: Term,
) -> Result<(), TypeError> {
    env.sort(expr)?;
    let interners = scoped!(INTERNERS);
    format_expr_with(sink, interners, env, expr);
    Ok(())
}

fn format_expr_with(sink: &mut String, interners: &Interners, env: &Environment<Name>, expr: Term) {
    match *interners.resolve_term(expr) {
        TermDef::Var(var) => format_var(sink, interners, env, var),
        TermDef::Const(value) => swrite!(sink, "{}", value),
        TermDef::Bool(value) => swrite!(sink, "{}", value),
        TermDef::Call { function, arguments } => {
            swrite!(sink, "(");
            format_var(sink, interners, env, function);
            for argument in arguments {
                swrite!(sink, " ");
                format_expr_with(sink, interners, env, *argument);
            }
            swrite!(sink, ")");
        }
        TermDef::Unary { op, expr } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr_with(sink, interners, env, expr);
            swrite!(sink, ")");
        }
        TermDef::Binary { op, lhs, rhs } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr_with(sink, interners, env, lhs);
            swrite!(sink, " ");
            format_expr_with(sink, interners, env, rhs);
            swrite!(sink, ")");
        }
        TermDef::Unit => swrite!(sink, "tuple0"),
        TermDef::Tuple(fields) => {
            swrite!(sink, "(tuple{}", fields.len());
            for field in fields {
                swrite!(sink, " ");
                format_expr_with(sink, interners, env, *field);
            }
            swrite!(sink, ")");
        }
        TermDef::Proj { tuple, field } => {
            let sort = env.cached_sort(tuple).expect("unchecked tuple reached SMT emission");
            let SortDef::Tuple(fields) = *interners.resolve_sort(sort) else {
                panic!("projection from non-tuple term reached SMT emission")
            };
            swrite!(sink, "(tuple{}!{} ", fields.len(), field);
            format_expr_with(sink, interners, env, tuple);
            swrite!(sink, ")");
        }
    }
}

fn format_var(sink: &mut String, interners: &Interners, env: &Environment<Name>, var: Var) {
    swrite!(sink, "{}!{}", interners.resolve_name(*env.binding(var)), var.index());
}

fn format_sort(sink: &mut String, interners: &Interners, sort: Sort) {
    match *interners.resolve_sort(sort) {
        SortDef::Int => swrite!(sink, "Int"),
        SortDef::Bool => swrite!(sink, "Bool"),
        SortDef::Tuple(fields) if fields.is_empty() => swrite!(sink, "Tuple0"),
        SortDef::Tuple(fields) => {
            swrite!(sink, "(Tuple{}", fields.len());
            for field in fields {
                swrite!(sink, " ");
                format_sort(sink, interners, *field);
            }
            swrite!(sink, ")");
        }
    }
}

fn collect_tuple_arities(arities: &mut FixedBitSet, interners: &Interners, sort: Sort) {
    let SortDef::Tuple(fields) = *interners.resolve_sort(sort) else { return };
    arities.grow(fields.len() + 1);
    arities.insert(fields.len());
    for field in fields {
        collect_tuple_arities(arities, interners, *field);
    }
}

fn declare_tuples(sink: &mut String, interners: &Interners, env: &Environment<Name>) {
    let mut arities = FixedBitSet::with_capacity(64);
    for (_, declaration, _) in env.iter() {
        match declaration {
            Declaration::Value(sort) => collect_tuple_arities(&mut arities, interners, *sort),
            Declaration::Function { domain, range } => {
                for sort in domain {
                    collect_tuple_arities(&mut arities, interners, *sort);
                }
                collect_tuple_arities(&mut arities, interners, *range);
            }
        }
    }
    for sort in env.cached_sorts() {
        collect_tuple_arities(&mut arities, interners, sort);
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

pub fn smt(env: &Environment<Name>, vc: Term) -> Result<String, TypeError> {
    let sort = env.sort(vc)?;
    let bool = SortDef::Bool.intern();
    if sort != bool {
        return Err(TypeError::Sort { expected: bool, actual: sort });
    }

    let interners = scoped!(INTERNERS);

    let mut result = String::new();
    let sink = &mut result;
    swrite!(sink, "(set-logic ALL)\n\n");
    declare_tuples(sink, interners, env);

    for (var, declaration, _) in env.iter() {
        match declaration {
            Declaration::Value(sort) => {
                swrite!(sink, "(declare-const ");
                format_var(sink, interners, env, var);
                swrite!(sink, " ");
                format_sort(sink, interners, *sort);
                swrite!(sink, ")\n");
            }
            Declaration::Function { domain, range } => {
                swrite!(sink, "(declare-fun ");
                format_var(sink, interners, env, var);
                swrite!(sink, " (");
                for (index, sort) in domain.iter().enumerate() {
                    if index > 0 {
                        swrite!(sink, " ");
                    }
                    format_sort(sink, interners, *sort);
                }
                swrite!(sink, ") ");
                format_sort(sink, interners, *range);
                swrite!(sink, ")\n");
            }
        }
    }

    swrite!(sink, "\n(assert (not ");
    format_expr_with(sink, interners, env, vc);
    swrite!(sink, "))\n\n(check-sat)\n(get-model)\n");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::{
        Environment, Fields, INTERNERS, Intern, Interners, SortDef, TypeError, format_expr, smt,
        term::{
            add, and, bool, call, eq, int as integer, proj, unit as unit_term, var as variable,
        },
        test,
    };

    test! {
        formats_full_width_integer_constants {
            let env = Environment::new();
            let mut formatted = String::new();
            format_expr(&mut formatted, &env, integer(i128::MIN)).unwrap();
            assert_eq!(formatted, i128::MIN.to_string());
        }
    }

    test! {
        distinguishes_variables_with_the_same_name {
            let int = SortDef::Int.intern();
            let name = "x".intern();
            let mut env = Environment::new();
            let first = variable(env.bind_value(int, name));
            let second = variable(env.bind_value(int, name));
            let output = smt(&env, eq(first, second)).unwrap();
            assert!(output.contains("(declare-const x!0 Int)"));
            assert!(output.contains("(declare-const x!1 Int)"));
            assert!(output.contains("(= x!0 x!1)"));
        }
    }

    test! {
        declares_functions_and_projects_tuples {
            let int = SortDef::Int.intern();
            let bool = SortDef::Bool.intern();
            let unit = SortDef::Tuple(Fields::new(&[])).intern();
            let pair_sort = SortDef::Tuple(Fields::new(&[int, bool])).intern();
            let mut env = Environment::new();
            let pair = variable(env.bind_value(pair_sort, "pair".intern()));
            let unit = variable(env.bind_value(unit, "unit".intern()));
            let function = env.bind_function(&[int], int, "f".intern());
            let one = integer(1);
            let output = smt(
                &env,
                and(
                    eq(call(function, &[one]), one),
                    and(eq(proj(pair, 0), one), eq(unit, unit_term())),
                ),
            )
            .unwrap();
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

    test! {
        checks_raw_terms_at_the_smt_boundary {
            let env = Environment::new();
            let int_sort = SortDef::Int.intern();
            let bool_sort = SortDef::Bool.intern();

            assert_eq!(
                smt(&env, add(bool(true), bool(true))),
                Err(TypeError::Sort { expected: int_sort, actual: bool_sort })
            );
        }
    }

    test! {
        checks_raw_terms_at_the_expression_boundary {
            let mut output = String::new();
            let env = Environment::new();
            let int_sort = SortDef::Int.intern();
            let bool_sort = SortDef::Bool.intern();

            assert_eq!(
                format_expr(&mut output, &env, add(bool(true), bool(true))),
                Err(TypeError::Sort { expected: int_sort, actual: bool_sort })
            );
            assert!(output.is_empty());
        }
    }
}
