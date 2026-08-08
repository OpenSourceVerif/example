use std::collections::HashMap;

use rustc_hir::intravisit::Visitor;
use rustc_hir::{self as hir, intravisit};
use rustc_middle::{
    mir::{Body, RETURN_PLACE, VarDebugInfoContents},
    ty::{Ty, TyCtxt, TyKind, UintTy},
};
use rustc_span::Span;
use verifier_core::{Context, Sort};

use super::{Binding, FunctionSpec, LoopSpec, SpecError, parser::parse_clause};

pub(crate) fn collect_function_spec<'tcx>(
    context: &mut Context,
    tcx: TyCtxt<'tcx>,
    owner: hir::def_id::LocalDefId,
    body: &Body<'tcx>,
) -> Result<FunctionSpec, SpecError> {
    let bindings = collect_bindings(context, tcx, body);
    let mut state_bindings = bindings.clone();
    state_bindings.remove("result");
    let mut spec = FunctionSpec::default();

    #[allow(deprecated)]
    for attribute in tcx.get_all_attrs(owner.to_def_id()) {
        let snippet = tcx.sess.source_map().span_to_snippet(attribute.span()).ok();
        let Some(snippet) = snippet else { continue };
        if let Some(expression) = attribute_expression(&snippet, "requires") {
            spec.requires.push(parse_clause(
                context,
                &state_bindings,
                expression,
                attribute.span(),
            )?);
        } else if let Some(expression) = attribute_expression(&snippet, "ensures") {
            spec.ensures.push(parse_clause(context, &bindings, expression, attribute.span())?);
        }
    }

    let hir_body = tcx.hir_body_owned_by(owner);
    let mut collector = LoopCollector { tcx, loops: Vec::new() };
    collector.visit_expr(hir_body.value);
    for (span, attributes) in collector.loops {
        let mut invariants = Vec::new();
        for (attribute_span, snippet) in attributes {
            if let Some(expression) = attribute_expression(&snippet, "invariant") {
                invariants.push(parse_clause(
                    context,
                    &state_bindings,
                    expression,
                    attribute_span,
                )?);
            }
        }
        if !invariants.is_empty() {
            spec.loops.push(LoopSpec { span, invariants });
        }
    }

    Ok(spec)
}

fn collect_bindings<'tcx>(
    context: &mut Context,
    tcx: TyCtxt<'tcx>,
    body: &Body<'tcx>,
) -> HashMap<String, Binding> {
    let mut bindings: HashMap<String, Binding> = HashMap::new();
    for info in &body.var_debug_info {
        let VarDebugInfoContents::Place(place) = info.value else { continue };
        if !place.projection.is_empty() {
            continue;
        }
        let Some(sort) = sort_for_ty(context, tcx, body.local_decls[place.local].ty) else {
            continue;
        };
        let name = info.name.as_str().to_owned();
        let binding = Binding { sort, local: Some(place.local), ambiguous: false };
        if let Some(previous) = bindings.get_mut(&name) {
            if previous.local != binding.local || previous.sort != binding.sort {
                previous.ambiguous = true;
            }
        } else {
            bindings.insert(name, binding);
        }
    }

    let return_ty = body.local_decls[RETURN_PLACE].ty;
    if let Some(sort) = sort_for_ty(context, tcx, return_ty) {
        bindings.insert("result".to_owned(), Binding { sort, local: None, ambiguous: false });
    }
    bindings
}

fn sort_for_ty<'tcx>(context: &mut Context, tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Option<Sort> {
    if ty.is_bool() {
        return Some(context.bool_sort());
    }
    let supported_integer = match ty.kind() {
        TyKind::Int(_) => true,
        TyKind::Uint(UintTy::U128) => false,
        TyKind::Uint(UintTy::Usize) => tcx.data_layout.pointer_size().bits() < 128,
        TyKind::Uint(_) => true,
        _ => false,
    };
    supported_integer.then(|| context.int_sort())
}

struct LoopCollector<'tcx> {
    tcx: TyCtxt<'tcx>,
    loops: Vec<(Span, Vec<(Span, String)>)>,
}

impl<'tcx> intravisit::Visitor<'tcx> for LoopCollector<'tcx> {
    fn visit_expr(&mut self, expression: &'tcx hir::Expr<'tcx>) {
        if matches!(expression.kind, hir::ExprKind::Loop(..)) {
            let attributes = self
                .tcx
                .hir_attrs(expression.hir_id)
                .iter()
                .filter_map(|attribute| {
                    self.tcx
                        .sess
                        .source_map()
                        .span_to_snippet(attribute.span())
                        .ok()
                        .map(|snippet| (attribute.span(), snippet))
                })
                .collect();
            self.loops.push((expression.span, attributes));
        }
        intravisit::walk_expr(self, expression);
    }
}

fn attribute_expression<'a>(snippet: &'a str, name: &str) -> Option<&'a str> {
    let snippet = snippet.trim();
    let prefix = format!("#[verifier::{name}(");
    snippet.strip_prefix(&prefix).and_then(|rest| rest.strip_suffix(")]")).map(str::trim)
}
