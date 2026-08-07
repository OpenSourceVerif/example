#![feature(rustc_private)]
#![allow(internal_features)]

extern crate rustc_driver;
extern crate rustc_interface;
extern crate rustc_middle;

use std::env::args;

use example::{Context, format_expr};
use rustc_driver::{Callbacks, Compilation};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;

#[derive(Default)]
struct SymbolicExecutionCallbacks;

impl Callbacks for SymbolicExecutionCallbacks {
    fn after_analysis(&mut self, _compiler: &Compiler, tcx: TyCtxt<'_>) -> Compilation {
        for local_def_id in tcx.hir_body_owners() {
            let body = tcx.mir_drops_elaborated_and_const_checked(local_def_id).borrow();
            let name = tcx.def_path_str(local_def_id.to_def_id());
            let mut context = Context::default();

            match context.execute(tcx, &body) {
                Ok(result) => {
                    println!("{name}:");
                    for (index, path) in result.return_paths.iter().enumerate() {
                        let mut condition = String::new();
                        format_expr(&mut condition, &context, path.fact);
                        let mut value = String::new();
                        format_expr(&mut value, &context, path.value);
                        println!("  return path {index}: {condition} => {value}");
                    }
                    for (index, assertion) in result.assertions.iter().enumerate() {
                        let mut formatted = String::new();
                        format_expr(&mut formatted, &context, *assertion);
                        println!("  assertion {index}: {formatted}");
                    }
                }
                Err(error) => eprintln!("{name}: skipped: {error}"),
            }
        }

        Compilation::Stop
    }
}

fn main() {
    let arguments: Vec<String> = args().collect();
    rustc_driver::run_compiler(&arguments, &mut SymbolicExecutionCallbacks);
}
