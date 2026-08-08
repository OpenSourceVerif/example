# Contracts

The verifier reads preconditions, postconditions, and loop invariants from
attributes in the `verifier` tool namespace.

```rust
#![feature(register_tool)]
#![feature(stmt_expr_attributes)]
#![register_tool(verifier)]

#[verifier::requires(n >= 0)]
#[verifier::ensures(result == 0)]
fn countdown(n: i32) -> i32 {
    let mut i = n;

    #[verifier::invariant(i >= 0)]
    #[verifier::invariant(i <= n)]
    while i > 0 {
        i -= 1;
    }

    i
}
```

The verifier generates separate conditions for the function postcondition,
loop-invariant initialization, and loop-invariant preservation. A precondition
is an assumption while verifying the function body; checking it at call sites
will be added together with modular call support.

Contract expressions currently support boolean and integer variables,
parentheses, integer and boolean literals, unary `!` and `-`, arithmetic,
comparisons, `&&`, `||`, and right-associative implication with `==>`.
