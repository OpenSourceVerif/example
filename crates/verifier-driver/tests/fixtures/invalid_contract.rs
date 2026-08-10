#![feature(register_tool)]
#![register_tool(verifier)]

#[verifier::ensures(result > 0)]
pub fn zero() -> i32 {
    0
}

fn main() {}
