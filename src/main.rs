#![feature(rustc_private)]
#![allow(internal_features)]

extern crate rustc_driver;
extern crate rustc_interface;
extern crate rustc_middle;

use std::env::args;

use rustc_driver::{
    Callbacks,
    Compilation::{self, *},
};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;

struct OurCallbacks;

impl Callbacks for OurCallbacks {
    fn after_analysis(&mut self, _compiler: &Compiler, tcx: TyCtxt<'_>) -> Compilation {
        for def_id in tcx.hir_body_owners() {
            let body = tcx.mir_drops_elaborated_and_const_checked(def_id).borrow();

            verify_body(tcx, def_id, &body);
        }

        Stop
    }
}

fn main() {
    let args: Box<[String]> = args().collect();

    rustc_driver::run_compiler(&args, &mut OurCallbacks);
}
