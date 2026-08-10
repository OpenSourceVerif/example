#![feature(rustc_private)]
#![allow(internal_features)]

extern crate rustc_driver;
extern crate rustc_interface;
extern crate rustc_middle;

use std::env::args;

mod callbacks;
mod report;
mod solver;

use callbacks::VerifierCallbacks;

fn main() {
    let arguments: Vec<String> = args().collect();
    rustc_driver::run_compiler(&arguments, &mut VerifierCallbacks);
}
