# Architecture

The project is organized around dependency boundaries rather than execution
order. Higher layers may depend on lower layers; lower layers must not import
types from higher layers.

## `interner`

`interner` owns the generic storage used to assign stable indices to hashable
values and strings. It has no verifier-specific concepts.

## `verifier-core`

`verifier-core` owns symbolic terms, sorts, symbols, their interned context, and
SMT-LIB rendering. It intentionally has no rustc-private dependencies. This
keeps the symbolic model independently testable and leaves room for another
frontend without coupling it to MIR.

## `verifier-rustc`

`verifier-rustc` is the Rust frontend and verification engine. Its `contracts`
modules extract and parse source-level attributes. Its `engine` modules analyze
MIR loops, symbolically execute supported statements and terminators, and emit
verification obligations.

The crate exposes `generate_obligations` as its main operation. The symbolic
`Context` does not know how to execute MIR; the frontend creates and owns that
association for the duration of one function analysis.

## `verifier-driver`

`verifier-driver` owns process arguments, rustc callbacks, Z3 execution, and
diagnostics. It renders each obligation as a complete SMT-LIB script, sends it
to Z3, and reports failed obligations with their counterexample models. It does
not implement symbolic semantics. Solver and command-line policy belong here
rather than in the verification engine.

## Testing boundaries

Pure algorithms have unit tests beside their modules. End-to-end tests belong
to `verifier-driver` and invoke the built `verifier` binary on dedicated fixture
files. Files under the repository-level `examples` directory are documentation
examples, not test inputs.
