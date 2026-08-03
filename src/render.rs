use crate::{Context, Expr, ExprDef, Op, Uop};

pub fn format(ctxt: &Context, expr: Expr) -> String {
    format_at(ctxt, expr, 0)
}

fn format_at(ctxt: &Context, expr: Expr, parent_precedence: u8) -> String {
    let (rendered, precedence) = match ctxt[expr] {
        ExprDef::Var(var) => (ctxt[var].to_owned(), 9),
        ExprDef::Const(value) => (value.to_string(), 9),
        ExprDef::Call { func: function, arg: argument } => {
            (format!("{}({})", &ctxt[function], format_at(ctxt, argument, 0)), 9)
        }
        ExprDef::Unary { op, expr } => {
            let operator = match op {
                Uop::Not => "!",
                Uop::Neg => "-",
            };
            (format!("{operator}{}", format_at(ctxt, expr, 8)), 8)
        }
        ExprDef::Binary { lhs, rhs, op } => {
            let (operator, precedence, right_associative) = match op {
                Op::Implies => ("=>", 1, true),
                Op::Or => ("||", 2, false),
                Op::And => ("&&", 3, false),
                Op::Eq => ("==", 4, false),
                Op::Ne => ("!=", 4, false),
                Op::Lt => ("<", 5, false),
                Op::Le => ("<=", 5, false),
                Op::Gt => (">", 5, false),
                Op::Ge => (">=", 5, false),
                Op::Add => ("+", 6, false),
                Op::Sub => ("-", 6, false),
                Op::Mul => ("*", 7, false),
            };
            let lhs_precedence = if right_associative { precedence + 1 } else { precedence };
            let rhs_precedence = if right_associative { precedence } else { precedence + 1 };
            (
                format!(
                    "{} {operator} {}",
                    format_at(ctxt, lhs, lhs_precedence),
                    format_at(ctxt, rhs, rhs_precedence)
                ),
                precedence,
            )
        }
    };

    if precedence < parent_precedence { format!("({rendered})") } else { rendered }
}
