#![feature(register_tool)]
#![feature(stmt_expr_attributes)]
#![register_tool(verifier)]

#[verifier::requires(a >= 0 && b >= 0)]
#[verifier::ensures(result >= 0)]
pub fn abs_difference(a: i32, b: i32) -> i32 {
    if a > b { a - b } else { b - a }
}

#[verifier::requires(n >= 0)]
#[verifier::ensures(result == 0)]
pub fn countdown(n: i32) -> i32 {
    let mut i = n;

    #[verifier::invariant(i >= 0)]
    #[verifier::invariant(i <= n)]
    while i > 0 {
        i -= 1;
    }

    i
}

fn main() {}
