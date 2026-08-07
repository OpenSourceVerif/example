#![feature(rustc_private)]
#![allow(internal_features)]

mod context;
mod def;
mod scriptgen;
mod vcgen;

pub use context::{Context, Intern};
pub use def::{TermDef, Name, Op, Sort, SortDef, Sym, SymDef, SymDefInterned, Term, Uop};
pub use scriptgen::{format_expr, smt};
// Linking rustc_driver makes rustc's private dependency graph available in
// dylib form when this crate is built as a test target.
extern crate rustc_driver;
extern crate rustc_middle;
pub use vcgen::{
    AssertionObligation, ExecutionLimits, MirExecutionError, ExecutionResult, ReturnPath,
    SymbolicArgument,
};
