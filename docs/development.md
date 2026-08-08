# Development

The repository uses the toolchain pinned in `rust-toolchain.toml`, including
`rustc-dev` and `llvm-tools`. Cargo commands should be run from the workspace
root.

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
