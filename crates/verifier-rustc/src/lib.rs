#![feature(rustc_private)]
#![feature(deref_patterns)]
#![allow(internal_features)]

// Linking rustc_driver makes rustc's private dependency graph available in
// dylib form when this crate is built as a test target.
extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_index;
extern crate rustc_middle;
extern crate rustc_span;

use std::fmt;

use rustc_hir::def_id::LocalDefId;
use rustc_middle::{mir::Body, ty::TyCtxt};
use verifier_core::{Environment, Name};

mod engine;
mod spec;
mod types;

pub use engine::{ExecutionError, Obligation, ObligationKind};
pub use spec::{SpecError, SpecErrorKind};

pub struct Verification {
    pub environment: Environment<Name>,
    pub obligations: Vec<Obligation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Spec(SpecError),
    Execution(ExecutionError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spec(error) => error.fmt(f),
            Self::Execution(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

/// Extracts source-level contracts and generates verification obligations for
/// one rustc MIR body.
pub fn verify<'tcx>(
    tcx: TyCtxt<'tcx>,
    owner: LocalDefId,
    body: &Body<'tcx>,
) -> Result<Verification, Error> {
    let spec = spec::collect(tcx, owner, body).map_err(Error::Spec)?;
    let mut environment = Environment::new();
    let obligations =
        engine::execute(&mut environment, tcx, body, &spec).map_err(Error::Execution)?;

    Ok(Verification { environment, obligations })
}
