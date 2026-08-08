#![feature(rustc_private)]
#![feature(deref_patterns)]
#![allow(internal_features)]

// Linking rustc_driver makes rustc's private dependency graph available in
// dylib form when this crate is built as a test target.
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_middle;
extern crate rustc_span;

use std::fmt::{Display, Formatter, Result as FormatResult};

use rustc_hir::def_id::LocalDefId;
use rustc_middle::{mir::Body, ty::TyCtxt};
use verifier_core::Context;

mod contracts;
mod engine;

pub use contracts::SpecError;
pub use engine::{ExecutionError, Limits, Obligation, ObligationKind};

pub struct FunctionVerification {
    pub context: Context,
    pub obligations: Vec<Obligation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationError {
    Specification(SpecError),
    Execution(ExecutionError),
}

impl Display for VerificationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FormatResult {
        match self {
            Self::Specification(error) => Display::fmt(error, formatter),
            Self::Execution(error) => Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for VerificationError {}

/// Extracts source-level contracts and generates verification obligations for
/// one rustc MIR body.
pub fn generate_obligations<'tcx>(
    tcx: TyCtxt<'tcx>,
    owner: LocalDefId,
    body: &Body<'tcx>,
) -> Result<FunctionVerification, VerificationError> {
    let mut context = Context::default();
    let specification = contracts::collect_function_spec(&mut context, tcx, owner, body)
        .map_err(VerificationError::Specification)?;
    let obligations = engine::execute_with_spec(&mut context, tcx, body, &specification)
        .map_err(VerificationError::Execution)?;

    Ok(FunctionVerification { context, obligations })
}
