# Verifier

This workspace contains an experimental symbolic verifier built on rustc's MIR.
It extracts source-level contracts, symbolically explores supported MIR, and
prints the resulting verification obligations in an SMT-compatible term model.

## Workspace

- `crates/interner` provides generic array and string interners.
- `crates/verifier-core` owns the rustc-independent symbolic IR and SMT rendering.
- `crates/verifier-rustc` extracts contracts and generates obligations from MIR.
- `crates/verifier-driver` provides the `verifier` compiler-driver executable.

The dependency direction is strictly:

```text
interner <- verifier-core <- verifier-rustc <- verifier-driver
                         ^                    |
                         +--------------------+
```

## Development

The pinned nightly toolchain installs the rustc development components used by
the compiler integration.

```sh
just test
```

To run the verifier on a bundled example:

```sh
cargo run -p verifier-driver --bin verifier -- examples/countdown.rs
```

See [the contract guide](docs/contracts.md), [the architecture](docs/architecture.md),
and [the development commands](docs/development.md) for more detail.
