# Development

The repository uses `nightly-2026-07-07`, pinned in `rust-toolchain.toml`, with
`rustc-dev`, `llvm-tools`, and `rustfmt`. Cargo commands should be run from the
workspace root. `z3` and `just` must be available on `PATH`.

The `justfile` provides fixed entry points for the supported environments:

```sh
just fmt       # apply Rust formatting locally
just check     # check every workspace target locally
just test      # run every workspace test locally
just ci        # run the non-mutating CI checks
```

The recipes intentionally have no environment switches or configurable command
parameters. A future packaging or deployment environment should receive its own
explicit recipe instead of adding conditional behavior to these commands.

GitHub Actions installs the pinned compiler, `just` 1.47.1, and Z3 before
running `just ci`. Local and hosted checks therefore share the same project
entry point.

## Updating the compiler

Rustc-private APIs follow the compiler rather than a stable compatibility
contract. Advance the date in `rust-toolchain.toml` and the matching CI install
command together, then run `just ci`. Keep the update separate from feature
work so compiler API changes and verifier behavior changes can be reviewed
independently.
