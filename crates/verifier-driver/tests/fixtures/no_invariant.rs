#![feature(register_tool)]
#![register_tool(verifier)]

fn missing_invariant(n: i32) -> i32 {
    let mut i = 0;
    while i < n {
        i += 1;
    }
    i
}

fn main() {}
