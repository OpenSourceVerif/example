use std::collections::HashMap;

use rustc_index::IndexVec;
use rustc_middle::mir::{Body, Local, Place, VarDebugInfoContents};
use smallvec::SmallVec;
use verifier_core::{Context, Intern, Term, TermDef};

pub(crate) fn instantiate(
    context: &mut Context,
    template: Term,
    values: &HashMap<String, Term>,
) -> Result<Term, String> {
    match context.get(template) {
        TermDef::Sym(symbol) => {
            let name = context.get(symbol).name;
            values.get(name).copied().ok_or_else(|| format!("no value for `{name}`"))
        }
        TermDef::Const(_) | TermDef::Bool(_) | TermDef::Unit => Ok(template),
        TermDef::Unary { op, expr } => {
            let expr = instantiate(context, expr, values)?;
            Ok(context.unary(op, expr))
        }
        TermDef::Binary { op, lhs, rhs } => {
            let lhs = instantiate(context, lhs, values)?;
            let rhs = instantiate(context, rhs, values)?;
            Ok(context.binary(op, lhs, rhs))
        }
        TermDef::Call { func, arg } => {
            let arg = instantiate(context, arg, values)?;
            Ok(context.call(func, arg))
        }
        TermDef::Tuple(fields) => {
            // copy to 1. workaround borrow checker; 2. scratch to interning a new tuple in context.
            let mut fields = SmallVec::<[_; 4]>::from_slice(fields);

            for field in &mut fields {
                *field = instantiate(context, *field, values)?;
            }

            Ok(context.tuple(&fields))
        }
    }
}

pub(crate) fn local_bindings(
    body: &Body<'_>,
    store: &IndexVec<Local, Option<Term>>,
) -> HashMap<String, Term> {
    let mut values = HashMap::new();
    for info in &body.var_debug_info {
        let VarDebugInfoContents::Place(Place { local, projection }) = info.value else { continue };
        if projection.is_empty()
            && let Some(value) = store[local]
        {
            values.insert(info.name.as_str().to_owned(), value);
        }
    }
    values
}
