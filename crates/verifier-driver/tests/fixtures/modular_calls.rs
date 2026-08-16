#![feature(register_tool)]
#![register_tool(verifier)]

#[verifier::requires(value < 2147483647)]
#[verifier::ensures(result == value + 1)]
fn increment(value: i32) -> i32 {
    value + 1
}

#[verifier::requires(value >= 0 && value < 2147483647)]
#[verifier::ensures(result > value)]
pub fn increment_nonnegative(value: i32) -> i32 {
    increment(value)
}

#[verifier::ensures(result >= 0)]
pub fn guarded_increment(value: i32) -> i32 {
    if value >= 0 && value < 2147483647 { increment(value) } else { 0 }
}

#[verifier::ensures(result == input)]
fn echo(input: (i32, bool)) -> (i32, bool) {
    input
}

#[verifier::ensures(result == input)]
pub fn call_echo(input: (i32, bool)) -> (i32, bool) {
    echo(input)
}

#[verifier::requires(value >= 0)]
#[verifier::ensures(result >= 0)]
pub fn recursive_countdown(value: i32) -> i32 {
    if value == 0 { 0 } else { recursive_countdown(value - 1) }
}

fn main() {}
