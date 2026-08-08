#![feature(register_tool)]
#![register_tool(verifier)]

#[verifier::requires(a >= 0 && b >= 0)]
#[verifier::ensures(result >= 0)]
pub fn abs_difference(a: i32, b: i32) -> i32 {
    if a > b { a - b } else { b - a }
}

fn main() {}
