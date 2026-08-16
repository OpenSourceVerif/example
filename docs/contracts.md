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
loop-invariant initialization, loop-invariant preservation, runtime assertions,
and call-site preconditions. A precondition is an assumption while verifying
the function body and an obligation at every supported call site. A callee's
postconditions become facts on the caller's normal-return path.

## Modular calls

Modular execution currently supports direct, monomorphic calls, including
recursive calls, to functions in the crate when every argument and the return
value has a supported sort. The callee body is verified separately. At a call,
the verifier creates a fresh symbolic return value and constrains it with the
callee postconditions; it does not inline the body.

The fresh result deliberately models a relation rather than an SMT function.
Ordinary Rust functions may depend on hidden state, so two calls with equal
arguments are not assumed to return equal results. Pure logical functions can
use the core function-term representation separately.

Indirect, generic, external, diverging, and tail calls remain unsupported. The
executor follows only a call's normal-return edge; unwind behavior is outside
the current partial-correctness model. An unsupported function without a
contract is reported as not verified. An unsupported function with a contract
is an error, because its postconditions must not be trusted without a proof.

Contract expressions currently support boolean and integer variables,
parentheses, integer and boolean literals, unary `!` and `-`, arithmetic,
comparisons, `&&`, `||`, and right-associative implication with `==>`.
