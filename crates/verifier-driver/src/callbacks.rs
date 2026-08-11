use rustc_driver::{Callbacks, Compilation};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;
use verifier_rustc::verify;

use crate::report;

#[derive(Default)]
pub(crate) struct VerifierCallbacks;

impl Callbacks for VerifierCallbacks {
    fn after_analysis(&mut self, _compiler: &Compiler, tcx: TyCtxt<'_>) -> Compilation {
        for owner in tcx.hir_body_owners() {
            let body = tcx.mir_drops_elaborated_and_const_checked(owner).borrow();
            let name = tcx.def_path_str(owner.to_def_id());

            match verify(tcx, owner, &body) {
                Ok(verification) => report::obligations(tcx, &name, &verification),
                Err(error) => report::failure(tcx, &name, &body, &error),
            }
        }

        Compilation::Stop
    }
}
