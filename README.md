# Verifier

This workspace contains an experimental symbolic verifier built on rustc's MIR.
It extracts source-level contracts, symbolically explores supported MIR, and
checks the resulting verification obligations with Z3 using generated SMT-LIB
scripts.

## Workspace

- `crates/verifier-core` owns the rustc-independent symbolic IR and SMT rendering.
- `crates/verifier-rustc` extracts contracts and generates obligations from MIR.
- `crates/verifier-driver` provides the `verifier` compiler-driver executable.

The dependency direction is strictly:

```text
verifier-core <- verifier-rustc <- verifier-driver
           ^                          |
           +--------------------------+
```

## Development

The pinned nightly toolchain installs the rustc development components used by
the compiler integration. `z3` must be available on `PATH`.

```sh
just test
```

To run the verifier on a bundled example:

```sh
cargo run -p verifier-driver --bin verifier -- examples/countdown.rs
```

Valid obligations produce no output. A failed obligation is written to stderr
with Z3's counterexample model.

See [the contract guide](docs/contracts.md), [the architecture](docs/architecture.md),
and [the development commands](docs/development.md) for more detail.
