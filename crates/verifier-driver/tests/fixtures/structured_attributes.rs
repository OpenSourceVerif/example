#![feature(register_tool)]
#![register_tool(verifier)]

#[verifier::requires (
    value >= 0
)]
#[verifier::ensures (
    result >= 0
)]
pub fn identity(value: i32) -> i32 {
    value
}

pub fn no_contract(result: i32) -> i32 {
    result
}

fn main() {}
