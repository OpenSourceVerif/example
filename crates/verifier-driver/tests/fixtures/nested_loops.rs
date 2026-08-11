#![feature(register_tool)]
#![feature(stmt_expr_attributes)]
#![register_tool(verifier)]

#[verifier::requires(outer >= 0 && inner >= 0)]
#[verifier::ensures(result == 0)]
pub fn nested(outer: i32, inner: i32) -> i32 {
    let mut i = outer;

    #[verifier::invariant(i >= 0)]
    while i > 0 {
        let mut j = inner;

        #[verifier::invariant(j >= 0)]
        while j > 0 {
            j -= 1;
        }
        i -= 1;
    }

    i
}

fn main() {}
