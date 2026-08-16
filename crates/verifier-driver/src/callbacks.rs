use rustc_driver::{Callbacks, Compilation};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;
use verifier_core::scope;
use verifier_rustc::verify;

use crate::report;

#[derive(Default)]
pub(crate) struct VerifierCallbacks;

impl Callbacks for VerifierCallbacks {
    fn after_analysis(&mut self, _compiler: &Compiler, tcx: TyCtxt<'_>) -> Compilation {
        let verify_crate = || {
            for owner in tcx.hir_body_owners() {
                let body = tcx.mir_drops_elaborated_and_const_checked(owner).borrow();
                let name = tcx.def_path_str(owner.to_def_id());

                match verify(tcx, owner, &body) {
                    Ok(verification) => report::obligations(tcx, &name, &verification),
                    Err(error) => report::failure(tcx, &name, &body, &error),
                }
            }
        };

        // SAFETY: rustc's analysis callback and every verifier call above are synchronous. No
        // future or coroutine can suspend with an interner reference, and this is the sole scope
        // installed by the driver.
        unsafe { scope(verify_crate) };

        Compilation::Stop
    }
}
