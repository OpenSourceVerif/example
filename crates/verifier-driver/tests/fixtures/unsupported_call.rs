#![feature(register_tool)]
#![register_tool(verifier)]

fn id(value: i32) -> i32 {
    value
}

pub fn call(value: i32) -> i32 {
    id(value)
}

fn main() {}
