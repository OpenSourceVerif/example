//! SMT-LIB rendering for symbolic terms and verification conditions.

use std::fmt::Display;

use fixedbitset::FixedBitSet;

use crate::{
    Declaration, DefStore, Environment, Field, INTERNERS, Intern, Interners, Name, Op, Sort,
    SortDef, Term, TermDef, TypeError, Uop, Var, scoped, swrite,
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

pub fn format_expr(sink: &mut String, environment: &Environment<Name>, expr: Term) {
    scoped!(let interners = INTERNERS);
    format_expr_with(sink, &interners.borrow(), environment, expr);
}

fn format_expr_with(
    sink: &mut String,
    interners: &Interners,
    environment: &Environment<Name>,
    expr: Term,
) {
    match interners.get(expr) {
        TermDef::Var(var) => format_var(sink, interners, environment, var),
        TermDef::Const(value) => swrite!(sink, "{}", value),
        TermDef::Bool(value) => swrite!(sink, "{}", value),
        TermDef::Call { function, arguments } => {
            swrite!(sink, "(");
            format_var(sink, interners, environment, function);
            for argument in arguments {
                swrite!(sink, " ");
                format_expr_with(sink, interners, environment, *argument);
            }
            swrite!(sink, ")");
        }
        TermDef::Unary { op, expr } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr_with(sink, interners, environment, expr);
            swrite!(sink, ")");
        }
        TermDef::Binary { op, lhs, rhs } => {
            swrite!(sink, "({} ", op.smt_style());
            format_expr_with(sink, interners, environment, lhs);
            swrite!(sink, " ");
            format_expr_with(sink, interners, environment, rhs);
            swrite!(sink, ")");
        }
        TermDef::Unit => swrite!(sink, "tuple0"),
        TermDef::Tuple(fields) => {
            swrite!(sink, "(tuple{}", fields.len());
            for field in fields {
                swrite!(sink, " ");
                format_expr_with(sink, interners, environment, *field);
            }
            swrite!(sink, ")");
        }
        TermDef::Proj { tuple, field } => {
            let sort =
                environment.cached_sort(tuple).expect("unchecked tuple reached SMT emission");
            let SortDef::Tuple(fields) = interners.get(sort) else {
                panic!("projection from non-tuple term reached SMT emission")
            };
            swrite!(sink, "(tuple{}!{} ", fields.len(), field);
            format_expr_with(sink, interners, environment, tuple);
            swrite!(sink, ")");
        }
    }
}

fn format_var(sink: &mut String, interners: &Interners, environment: &Environment<Name>, var: Var) {
    swrite!(sink, "{}!{}", interners.get(*environment.binding(var)), var.index());
}

fn format_sort(sink: &mut String, interners: &Interners, sort: Sort) {
    match interners.get(sort) {
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
    let SortDef::Tuple(fields) = interners.get(sort) else { return };
    arities.grow(fields.len() + 1);
    arities.insert(fields.len());
    for field in fields {
        collect_tuple_arities(arities, interners, *field);
    }
}

fn declare_tuples(sink: &mut String, interners: &Interners, environment: &Environment<Name>) {
    let mut arities = FixedBitSet::with_capacity(64);
    for (_, declaration, _) in environment.iter() {
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
    for sort in environment.cached_sorts() {
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

pub fn smt(environment: &Environment<Name>, vc: Term) -> Result<String, TypeError> {
    let sort = environment.sort(vc)?;
    let bool = SortDef::Bool.intern();
    if sort != bool {
        return Err(TypeError::Sort { expected: bool, actual: sort });
    }

    scoped!(let interners = INTERNERS);
    let interners = interners.borrow();

    let mut result = String::new();
    let sink = &mut result;
    swrite!(sink, "(set-logic ALL)\n\n");
    declare_tuples(sink, &interners, environment);

    for (var, declaration, _) in environment.iter() {
        match declaration {
            Declaration::Value(sort) => {
                swrite!(sink, "(declare-const ");
                format_var(sink, &interners, environment, var);
                swrite!(sink, " ");
                format_sort(sink, &interners, *sort);
                swrite!(sink, ")\n");
            }
            Declaration::Function { domain, range } => {
                swrite!(sink, "(declare-fun ");
                format_var(sink, &interners, environment, var);
                swrite!(sink, " (");
                for (index, sort) in domain.iter().enumerate() {
                    if index > 0 {
                        swrite!(sink, " ");
                    }
                    format_sort(sink, &interners, *sort);
                }
                swrite!(sink, ") ");
                format_sort(sink, &interners, *range);
                swrite!(sink, ")\n");
            }
        }
    }

    swrite!(sink, "\n(assert (not ");
    format_expr_with(sink, &interners, environment, vc);
    swrite!(sink, "))\n\n(check-sat)\n(get-model)\n");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::{
        Environment, Fields, Intern, Op, SortDef, TermDef, TypeError, format_expr, scope, smt,
    };

    #[test]
    fn formats_full_width_integer_constants() {
        // SAFETY: this test is synchronous.
        unsafe {
            scope(|| {
                let environment = Environment::<crate::Name>::new();
                let value = environment.int(i128::MIN);
                let mut formatted = String::new();
                format_expr(&mut formatted, &environment, value);
                assert_eq!(formatted, i128::MIN.to_string());
            })
        }
    }

    #[test]
    fn distinguishes_variables_with_the_same_name() {
        // SAFETY: this test is synchronous.
        unsafe {
            scope(|| {
                let int = SortDef::Int.intern();
                let name = "x".intern();
                let mut environment = Environment::new();
                let first = environment.bind_value(int, name);
                let second = environment.bind_value(int, name);
                let first = environment.var(first);
                let second = environment.var(second);
                let equality = environment.eq(first, second);
                let output = smt(&environment, equality).unwrap();
                assert!(output.contains("(declare-const x!0 Int)"));
                assert!(output.contains("(declare-const x!1 Int)"));
                assert!(output.contains("(= x!0 x!1)"));
            })
        }
    }

    #[test]
    fn declares_functions_and_projects_tuples() {
        // SAFETY: this test is synchronous.
        unsafe {
            scope(|| {
                let int = SortDef::Int.intern();
                let bool = SortDef::Bool.intern();
                let unit = SortDef::Tuple(Fields::new(&[])).intern();
                let pair_sort = SortDef::Tuple(Fields::new(&[int, bool])).intern();
                let mut environment = Environment::new();
                let pair = environment.bind_value(pair_sort, "pair".intern());
                let unit = environment.bind_value(unit, "unit".intern());
                let function = environment.bind_function(&[int], int, "f".intern());
                let pair = environment.var(pair);
                let unit = environment.var(unit);
                let first = environment.proj(pair, 0);
                let one = environment.int(1);
                let called = environment.call(function, &[one]);
                let call_holds = environment.eq(called, one);
                let pair_holds = environment.eq(first, one);
                let unit_value = environment.unit();
                let unit_holds = environment.eq(unit, unit_value);
                let tail = environment.and(pair_holds, unit_holds);
                let vc = environment.and(call_holds, tail);
                let output = smt(&environment, vc).unwrap();
                assert!(output.contains("(declare-datatype Tuple0 ((tuple0)))"));
                assert!(output.contains(
                    "(declare-datatype Tuple2 (par (T0 T1) ((tuple2 (tuple2!0 T0) (tuple2!1 T1)))))"
                ));
                assert!(output.contains("(declare-const pair!0 (Tuple2 Int Bool))"));
                assert!(output.contains("(declare-fun f!2 (Int) Int)"));
                assert!(output.contains("(tuple2!0 pair!0)"));
                assert!(output.contains("(= unit!1 tuple0)"));
            })
        }
    }

    #[test]
    fn checks_raw_terms_at_the_smt_boundary() {
        // SAFETY: this test is synchronous.
        unsafe {
            scope(|| {
                let environment = Environment::<crate::Name>::new();
                let yes = TermDef::Bool(true).intern();
                let invalid = TermDef::Binary { op: Op::Add, lhs: yes, rhs: yes }.intern();
                let int = SortDef::Int.intern();
                let bool = SortDef::Bool.intern();

                assert_eq!(
                    smt(&environment, invalid),
                    Err(TypeError::Sort { expected: int, actual: bool })
                );
            })
        }
    }
}
