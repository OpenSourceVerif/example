#![feature(register_tool)]
#![register_tool(verifier)]

#[verifier::requires(value >= 0)]
#[verifier::ensures(result == value)]
fn require_nonnegative(value: i32) -> i32 {
    value
}

#[verifier::ensures(result == value)]
pub fn unchecked(value: i32) -> i32 {
    require_nonnegative(value)
}

fn main() {}
