use rustc_middle::ty::TyCtxt;
use verifier_core::smt;
use verifier_rustc::{FunctionVerification, VerificationError};

use crate::solver;

pub(crate) fn verify(tcx: TyCtxt<'_>, name: &str, verification: &FunctionVerification) {
    for (index, obligation) in verification.obligations.iter().enumerate() {
        let script = smt(&verification.context, obligation.condition);
        match solver::check(&script) {
            Ok(None) => {}
            Ok(Some(model)) => {
                let model = if model.is_empty() { String::new() } else { format!("\n{model}") };
                tcx.dcx()
                    .err(format!("{name}: {:?} {index} failed\n{script}{model}", obligation.kind));
            }
            Err(error) => {
                tcx.dcx().err(format!("{name}: {:?} {index}: {error}", obligation.kind));
            }
        }
    }
}

pub(crate) fn failure(tcx: TyCtxt<'_>, name: &str, error: &VerificationError) {
    match error {
        VerificationError::Specification(error) => {
            let location = tcx.sess.source_map().span_to_diagnostic_string(error.span);
            eprintln!("{name}: invalid specification at {location}: {error}");
        }
        VerificationError::Execution(error) => eprintln!("{name}: skipped: {error}"),
    }
}
