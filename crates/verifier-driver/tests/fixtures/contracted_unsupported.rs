#![feature(register_tool)]
#![register_tool(verifier)]

#[verifier::ensures(result >= 0)]
fn complement(input: u8) -> u8 {
    let _ = input;
    !input
}

fn main() {}
