#![feature(register_tool)]
#![register_tool(verifier)]

#[verifier::ensures(result > input)]
pub fn zero(input: i32) -> i32 {
    let _ = input;
    0
}

fn main() {}
