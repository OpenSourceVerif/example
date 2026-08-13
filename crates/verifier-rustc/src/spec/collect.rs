use std::{collections::HashMap, ops::Range};

use hir::AttrArgs::Delimited;
use hir::Attribute::Unparsed;
use hir::def_id::LocalDefId;
use hir::{Attribute, Expr};
use rustc_ast::token::Delimiter;
use rustc_hir::{
    self as hir,
    intravisit::{self, Visitor},
};
use rustc_middle::{
    mir::{Body, RETURN_PLACE, VarDebugInfo, VarDebugInfoContents},
    ty::TyCtxt,
};
use rustc_span::{BytePos, Span, Spanned, Symbol};
use verifier_core::{
    Context, Sort,
    contract::{ResolveError, parse},
};

use crate::types::RustcTy;

use super::{Clause, LoopSpec, Slot, Spec, SpecError, SpecErrorKind};

type Bindings = HashMap<Symbol, Option<(Sort, Slot)>>;

pub(crate) fn collect<'tcx>(
    cx: &mut Context,
    tcx: TyCtxt<'tcx>,
    owner: LocalDefId,
    body: &Body<'tcx>,
) -> Result<Spec, SpecError> {
    let args = bindings(cx, tcx, body, |info| info.argument_index.is_some());
    let result = Symbol::intern("result");
    #[allow(deprecated)]
    let attrs = tcx.get_all_attrs(owner.to_def_id());
    if args.contains_key(&result)
        && let Some(attr) = attrs.iter().find(|attr| is(attr, "ensures"))
    {
        return Err(SpecError { span: attr.span(), kind: SpecErrorKind::ReservedResult });
    }

    let mut results = args.clone();
    if let Some(sort) = cx.sort(tcx, body.local_decls[RETURN_PLACE].ty) {
        results.insert(result, Some((sort, Slot::Result)));
    }
    let locals = bindings(cx, tcx, body, |_| true);
    let mut spec = Spec::default();

    for attr in attrs {
        if let Some(clause) = clause(cx, tcx, attr, "requires", &args)? {
            spec.requires.push(clause);
        } else if let Some(clause) = clause(cx, tcx, attr, "ensures", &results)? {
            spec.ensures.push(clause);
        }
    }

    let hir_body = tcx.hir_body_owned_by(owner);
    let mut collector =
        LoopCollector { cx, tcx, bindings: &locals, loops: Vec::new(), error: None };
    collector.visit_expr(hir_body.value);
    if let Some(error) = collector.error {
        return Err(error);
    }
    spec.loops = collector.loops;

    Ok(spec)
}

fn bindings<'tcx>(
    cx: &mut Context,
    tcx: TyCtxt<'tcx>,
    body: &Body<'tcx>,
    include: impl Fn(&VarDebugInfo<'tcx>) -> bool,
) -> Bindings {
    let mut bindings = Bindings::new();
    for info in body.var_debug_info.iter().filter(|info| include(info)) {
        let VarDebugInfoContents::Place(place) = info.value else { continue };
        if !place.projection.is_empty() {
            continue;
        }
        let Some(sort) = cx.sort(tcx, body.local_decls[place.local].ty) else { continue };
        let binding = (sort, Slot::Local(place.local));
        bindings
            .entry(info.name)
            .and_modify(|previous| {
                if *previous != Some(binding) {
                    *previous = None;
                }
            })
            .or_insert(Some(binding));
    }
    bindings
}

fn clause(
    cx: &mut Context,
    tcx: TyCtxt<'_>,
    attr: &Attribute,
    name: &str,
    bindings: &Bindings,
) -> Result<Option<Clause>, SpecError> {
    if !is(attr, name) {
        return Ok(None);
    }
    let Unparsed(deref!(item)) = attr else {
        return Err(SpecError { span: attr.span(), kind: SpecErrorKind::Args });
    };
    let Delimited(args) = &item.args else {
        return Err(SpecError { span: attr.span(), kind: SpecErrorKind::Args });
    };
    if args.delim != Delimiter::Parenthesis {
        return Err(SpecError { span: attr.span(), kind: SpecErrorKind::Args });
    }
    let span = args.dspan.open.shrink_to_hi().to(args.dspan.close.shrink_to_lo());
    let text = tcx
        .sess
        .source_map()
        .span_to_snippet(span)
        .map_err(|_| SpecError { span, kind: SpecErrorKind::Snippet })?;
    let node = parse(cx, &text, |name| match bindings.get(&Symbol::intern(name)) {
        Some(Some(binding)) => Ok(*binding),
        Some(None) => Err(ResolveError::Ambiguous),
        None => Err(ResolveError::Unknown),
    })
    .map_err(|error| SpecError {
        span: subspan(span, error.range),
        kind: SpecErrorKind::Parse(error.kind),
    })?;
    Ok(Some(Spanned { node, span: attr.span() }))
}

fn is(attr: &Attribute, name: &str) -> bool {
    attr.path_matches(&[Symbol::intern("verifier"), Symbol::intern(name)])
}

fn subspan(span: Span, range: Range<usize>) -> Span {
    let lo = span.lo();
    span.with_lo(lo + BytePos(range.start as u32)).with_hi(lo + BytePos(range.end as u32))
}

struct LoopCollector<'a, 'tcx> {
    cx: &'a mut Context,
    tcx: TyCtxt<'tcx>,
    bindings: &'a Bindings,
    loops: Vec<LoopSpec>,
    error: Option<SpecError>,
}

impl<'tcx> Visitor<'tcx> for LoopCollector<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if matches!(expr.kind, hir::ExprKind::Loop(..)) {
            let invariants = self
                .tcx
                .hir_attrs(expr.hir_id)
                .iter()
                .filter_map(|attr| {
                    clause(self.cx, self.tcx, attr, "invariant", self.bindings).transpose()
                })
                .collect();
            match invariants {
                Ok(invariants) => self.loops.push(LoopSpec { invariants }),
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            }
        }
        if self.error.is_none() {
            intravisit::walk_expr(self, expr);
        }
    }
}
