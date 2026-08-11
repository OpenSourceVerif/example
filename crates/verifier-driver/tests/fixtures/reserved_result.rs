#![feature(register_tool)]
#![register_tool(verifier)]

#[verifier::requires(result >= 0)]
#[verifier::ensures(result >= 0)]
pub fn collision(result: i32) -> i32 {
    result
}

fn main() {}
