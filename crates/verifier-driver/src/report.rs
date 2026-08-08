use rustc_middle::ty::TyCtxt;
use verifier_core::format_expr;
use verifier_rustc::{FunctionVerification, VerificationError};

pub(crate) fn success(name: &str, verification: &FunctionVerification) {
    println!("{name}:");
    for (index, obligation) in verification.obligations.iter().enumerate() {
        let mut formatted = String::new();
        format_expr(&mut formatted, &verification.context, obligation.condition);
        println!("  {:?} {index}: {formatted}", obligation.kind);
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
