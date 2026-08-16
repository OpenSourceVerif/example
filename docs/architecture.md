# Architecture

The project is organized around dependency boundaries rather than execution
order. Higher layers may depend on lower layers; lower layers must not import
types from higher layers.

## `verifier-core`

`verifier-core` owns symbolic terms, sorts, names, environments, open contract
clauses, contract parsing and instantiation, and SMT-LIB rendering. It
intentionally has no rustc-private dependencies. This keeps the symbolic model
independently testable and leaves room for another frontend without coupling it
to MIR.

Symbolic identity storage and symbolic scope are separate:

- `Interners` owns canonical `Term`, `Sort`, and `Name` definitions for one
  synchronous compiler session. The driver installs it in scoped thread-local
  storage around the complete verification call tree, so ordinary operations
  can intern and resolve definitions without threading a storage parameter
  through every function.
- `Environment<B>` is explicit and belongs to one verification or open clause.
  It contains append-only declarations, frontend bindings, and a derived sort
  cache. A raw interned term has syntax identity but is checked and sorted only
  under an environment.

The interned handles point into the current session arena and are thread
confined. They must not escape the installed session, cross threads, or remain
live across suspension. Parallel or asynchronous verification therefore
requires a different session-storage design rather than a safe wrapper around
the current scoped TLS contract.

## `verifier-rustc`

`verifier-rustc` is the Rust frontend and verification engine. Its `spec`
modules extract source-level attributes. Its `engine` modules analyze
MIR loops, symbolically execute supported statements and terminators, and emit
verification obligations.

The crate exposes `verify` as its main operation. It creates an explicit
`Environment` for one function analysis and keeps MIR locations, symbolic local
values, path facts, loop analysis, and rustc identities in the frontend. These
frontend concerns do not belong in the core environment.

## `verifier-driver`

`verifier-driver` owns process arguments, rustc callbacks, Z3 execution, and
diagnostics. It renders each obligation as a complete SMT-LIB script, sends it
to Z3, and reports failed obligations with their counterexample models. It does
not implement symbolic semantics. Solver and command-line policy belong here
rather than in the verification engine. It also owns the single unsafe interner
installation point and documents why the dynamic verification call tree is
synchronous.

## Checked boundaries

Raw definitions may be interned without proving that they are well-sorted.
`Environment::sort` is the central checker. Contract construction requires a
well-sorted term under its owned environment, while the source contract parser
additionally requires a Boolean term. SMT expression formatting checks its
input before traversing it, and full SMT script generation requires a Boolean
verification condition.

## Testing boundaries

Pure algorithms have unit tests beside their modules. End-to-end tests belong
to `verifier-driver` and invoke the built `verifier` binary on dedicated fixture
files. Files under the repository-level `examples` directory are documentation
examples, not test inputs.
