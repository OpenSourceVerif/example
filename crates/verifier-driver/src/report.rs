use rustc_middle::{mir::Body, ty::TyCtxt};
use verifier_core::smt;
use verifier_rustc::{Error, Verification};

use crate::solver;

pub(crate) fn obligations(tcx: TyCtxt<'_>, name: &str, verification: &Verification) {
    for (index, obligation) in verification.obligations.iter().enumerate() {
        let script = match smt(&verification.environment, obligation.condition) {
            Ok(script) => script,
            Err(error) => {
                tcx.dcx().span_err(
                    obligation.span,
                    format!(
                        "{name}: {:?} {index}: ill-sorted obligation: {error}",
                        obligation.kind
                    ),
                );
                continue;
            }
        };
        match solver::check(&script) {
            Ok(None) => {}
            Ok(Some(model)) => {
                let model = if model.is_empty() { String::new() } else { format!("\n{model}") };
                tcx.dcx().span_err(
                    obligation.span,
                    format!("{name}: {:?} {index} failed\n{script}{model}", obligation.kind),
                );
            }
            Err(error) => {
                tcx.dcx().span_err(
                    obligation.span,
                    format!("{name}: {:?} {index}: {error}", obligation.kind),
                );
            }
        }
    }
}

pub(crate) fn failure(tcx: TyCtxt<'_>, name: &str, body: &Body<'_>, error: &Error) {
    match error {
        Error::Spec(error) => {
            tcx.dcx().span_err(error.span, format!("{name}: invalid contract: {error}"));
        }
        Error::Execution { error, contracted } => {
            let span = body.source_info(error.location).span;
            let message = format!("{name}: not verified: {error}");
            if *contracted {
                tcx.dcx().span_err(span, message);
            } else {
                tcx.dcx().span_warn(span, message);
            }
        }
    }
}
