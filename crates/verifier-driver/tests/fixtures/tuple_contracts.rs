#![feature(register_tool)]
#![register_tool(verifier)]
#![allow(dead_code)]

#[verifier::ensures(result == input)]
fn rebuild(input: (i32, bool)) -> (i32, bool) {
    (input.0, input.1)
}

#[verifier::ensures(result == input)]
fn restore(mut input: (i32, bool)) -> (i32, bool) {
    let old = input.0;
    input.0 = 0;
    input.0 = old;
    input
}

#[verifier::ensures(result == input)]
fn unit_id(input: ()) {
    input
}

fn main() {}
