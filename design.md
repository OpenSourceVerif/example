# Generative scoped interner design

## Recommendation

Put only the syntax, sort, list, and name interners behind a compilation-scoped native TLS key. Use the generative bare-reference mechanism prototyped in [`generative-scoped-tls`](https://github.com/sssxks/generative-scoped-tls), rather than ordinary closure-only `scoped_tls` access.

Keep `Environment` explicit. Remove `Context` from function signatures and remove `Builder`. Make typed constructors methods on `Environment` taking `&self`, with only its non-semantic sort cache behind interior mutability.

The resulting boundary is:

- Implicit: the arena in which raw syntax, sorts, lists, and names are canonicalized.
- Explicit: the environment under which raw syntax is scoped and sorted.
- Unsafe once: installation of the borrowed arena for a synchronous dynamic call tree.
- Safe everywhere else: obtaining and using a generatively branded bare reference.

The prototype changes an important part of the previous design: borrowed definitions such as `TermDef<'_>`, `SortDef<'_>`, `Fields<'_, T>`, and `&str` do **not** have to be replaced with owned snapshots merely to make TLS ergonomic. A deep caller can bind an ordinary `&Context`/`&RefCell<Context>` with statement syntax, and the fresh generative lifetime prevents ordinary lexical escape.

## Why this is the right implicit boundary

The extrinsic refactor separated two fundamentally different things:

- The interner determines the identity of raw syntax.
- `Environment` determines whether a term is scoped and well sorted.

The current [`Context`](/home/xks/repos/example/crates/verifier-core/src/context/mod.rs:19) is implementation machinery. Passing it communicates no semantic information. By contrast, `Environment` is part of the judgment:

```text
Gamma |- term : sort
```

The same interned `Var(0)` is deliberately `Int` in one environment and `Bool` in another, as tested in [`context/mod.rs`](/home/xks/repos/example/crates/verifier-core/src/context/mod.rs:109). Hiding the environment would therefore hide a real semantic input. Hiding the arena does not.

This is a narrow and principled use of ambient state: canonical storage is ambient; meaning remains explicit.

## Updated TLS mechanism review

| Design | Lookup | Lifetime | Main issue | Recommendation |
|---|---|---|---|---|
| Explicit `&mut Context` | Direct | Lexical | Parameter drilling and serialized construction | Baseline |
| `std::thread_local!` owning `RefCell<Interners>` | Native/platform TLS | Thread | No compilation boundary; independent per-thread arenas | Fallback only |
| `thread_local::ThreadLocal` | Per-object thread map | Container lifetime | Map lookup, retained values, thread-ID reuse | Reject |
| Ordinary `scoped_tls` | Native TLS pointer | Dynamic scope | Safe access is closure-bounded | Sound but less ergonomic |
| Manual leaked `&'static RefCell<_>` | Native TLS pointer | Process/thread | Leak and false session lifetime | Reject |
| Generative scoped TLS | Native TLS pointer | Dynamic installation plus branded lexical borrow | `set` must remain unsafe because suspension can violate the dynamic lifetime | Recommended |

The standard library's `LocalKey` uses the fastest TLS implementation available on the target. A `const`-initialized non-dropping `Cell<*const ()>` uses the efficient native representation where available. [Rust `LocalKey` documentation](https://doc.rust-lang.org/std/thread/struct.LocalKey.html), [`thread_local!` documentation](https://doc.rust-lang.org/std/macro.thread_local.html).

Both `scoped_tls` and the prototype store a nullable raw pointer in such a TLS cell and restore the previous pointer with an RAII guard. The new ingredient is a fresh lexical brand at each lookup:

```rust
scoped!(let interners = INTERNERS);
// `interners` is an ordinary branded `&T`.
```

The brand prevents safe code from extending that reference to `'static` or returning it through an incompatible lifetime. The outer `set` call remains unsafe because the compiler cannot prove that the lexical scope is contained in the pointer's dynamic installation scope when suspension is possible.

Rustc's `SessionGlobals` remains a useful precedent for the broader architecture: compiler-session interners are installed in scoped TLS and accessed without drilling a handle through every call. [Rustc `SessionGlobals`](https://github.com/rust-lang/rust/blob/main/compiler/rustc_span/src/lib.rs#L2364-L2515). The prototype improves the local access syntax; it does not change the need for a carefully defined session boundary.

The `thread_local` crate still solves the wrong problem here. Its values live in a per-object mapping and are retained until the `ThreadLocal` container is dropped; its internal thread IDs may be reused after a thread exits. [The `thread_local` crate documentation](https://docs.rs/thread_local/latest/thread_local/). None of that is needed for one compiler-session arena.

## What the generative prototype proves

The prototype establishes the following useful shape:

```rust
fn deep() {
    scoped!(let context = CONTEXT);
    // `context: &Context`
    use_context(context);
}

let context = Context::default();
let body = || deep();

// SAFETY: the complete call tree is synchronous, and no branded reference
// remains usable after `set` returns.
unsafe { CONTEXT.set(&context, body) };
```

The reference is bare in the sense that subsequent field access and ordinary method calls use a normal `&Context`; access is not forced into `with(|context| ...)` CPS.

The proof has three parts:

```text
lifetime(reference from scoped!)
    <= lexical scope at the lookup site       (generativity)
    <= dynamic extent of ScopedKey::set       (unsafe synchronous contract)
    <= lifetime of the installed value        (ordinary borrow checking)
```

Normal return and panic unwinding preserve stack order. The reset guard restores the previous pointer after inner frames have finished unwinding.

### The unavoidable unsafe boundary

`ScopedKey::set` must remain unsafe. Do not hide it behind a generally safe function like this:

```rust
// Unsound as a general safe API.
pub fn scope<R>(f: impl FnOnce() -> R) -> R {
    let interners = Interners::default();
    unsafe { INTERNERS.set(&interners, f) }
}
```

An arbitrary safe `FnOnce` can poll a future that calls `scoped!`, suspend while retaining the resulting reference, return from `set`, and resume later. The prototype includes an expected-failure Miri fixture demonstrating the resulting dangling reference when the unsafe contract is violated.

The verifier should therefore keep one explicit unsafe installation point, either by calling `set` directly or through an `unsafe fn` whose contract is identical:

```rust
/// # Safety
///
/// `body` and every function it calls must be synchronous. No future,
/// coroutine, or generator may retain a reference obtained from `INTERNERS`
/// across a suspension that lets this invocation return.
pub unsafe fn scope<R>(body: impl FnOnce() -> R) -> R {
    assert!(!INTERNERS.is_set(), "interner scope is already active");
    let interners = RefCell::new(Interners::default());
    unsafe { INTERNERS.set(&interners, body) }
}
```

Construct the callback outside the `unsafe` block so that its body does not inherit an unsafe context:

```rust
fn after_analysis(...) -> Compilation {
    let body = || {
        for owner in tcx.hir_body_owners() {
            // verify and report while the interner binding is active
        }
    };

    // SAFETY: this rustc callback executes a synchronous verification call tree.
    unsafe { intern::scope(body) };
    Compilation::Stop
}
```

This unsafe boundary is acceptable here because it is singular, auditable, and matches the verifier's current synchronous architecture. If verification becomes async, this design must be revisited; task-local storage or explicit context passing is then the appropriate mechanism.

## Proposed storage setup

Start with the least invasive layout:

```rust
use std::cell::RefCell;

use generative_scoped_tls::{scoped, scoped_thread_local};

#[derive(Default)]
pub struct Interners {
    terms: StructInterner<Term, TermDefStored>,
    term_lists: ListInterner<Term>,
    names: StringInterner<Name>,
    sorts: StructInterner<Sort, SortDefStored>,
    sort_lists: ListInterner<Sort>,
}

scoped_thread_local! {
    pub static INTERNERS: RefCell<Interners>;
}
```

`Interners` replaces the name `Context`. It may need to be public or doc-hidden so the TLS key can be used across `verifier-core` and `verifier-rustc`, but its fields and raw mutation methods should remain private. The important removal is from ordinary function signatures and owned result types, not necessarily making the storage type unnameable.

A deep mutating operation obtains a bare reference to the cell and borrows it only for the actual interner operation:

```rust
fn intern(definition: TermDef<'_>) -> Term {
    scoped!(let interners = INTERNERS);
    interners.borrow_mut().intern_term(definition)
}
```

A read-only phase can bind and borrow once:

```rust
fn smt(environment: &Environment<Name>, vc: Term) -> Result<String, SmtError> {
    scoped!(let interners = INTERNERS);
    let interners = interners.borrow();

    // Recursive formatting now uses an ordinary `&Interners` and retains the
    // existing borrowed TermDef/SortDef/Fields representation.
    format_checked(&interners, environment, vc)
}
```

This gives two performance modes:

- Fine-grained methods such as `.intern()` perform one TLS lookup and one short dynamic borrow.
- Read-heavy phases such as SMT formatting perform one TLS lookup, one shared borrow, and then ordinary reference access for the whole traversal.

If the single `RefCell<Interners>` produces borrow conflicts or excessive false coupling, split it later along actual mutation boundaries:

```rust
pub struct Interners {
    terms: RefCell<TermInterners>,
    sorts: RefCell<SortInterners>,
    names: RefCell<StringInterner<Name>>,
}
```

That would, for example, permit holding a term-definition borrow while interning a sort. It adds borrow flags and structural complexity, so it should be driven by concrete call patterns or profiling rather than done preemptively.

Do not attempt to return a generative `&mut Interners` at every lookup. Independent lookups could then create aliased mutable references. Interior mutability remains necessary for mutation; the prototype removes CPS syntax, not Rust's aliasing rules.

## Arena scope and handle identity

Install one arena around the full owner loop in [`after_analysis`](/home/xks/repos/example/crates/verifier-driver/src/callbacks.rs:12).

That means:

- All term, sort, list, and name handles in one compilation share one identity domain.
- Common definitions can be reused across bodies.
- [`Verification`](/home/xks/repos/example/crates/verifier-rustc/src/lib.rs:27) no longer owns `cx`.
- SMT rendering occurs while the arena is still installed.
- Arena memory is reclaimed at the end of the compilation rather than leaked.

This retains more memory than the current per-body `Context`, so peak memory across many bodies belongs in the benchmark. If that is significant, use one scope per fully verified-and-reported body, but then explicitly treat handles from different bodies as incomparable.

The underlying prototype supports nested `set` calls and correctly restores the previous binding. The verifier wrapper should nevertheless reject nesting. With plain numeric handles, an outer `Term(0)` used while a different inner arena is installed could resolve as an unrelated inner definition.

Similarly, handles must not cross threads. Make `Term`, `Sort`, and `Name` non-`Send` and non-`Sync` with a zero-sized marker such as `PhantomData<Rc<()>>` if the index type implementation permits it without changing layout. This statically prevents the most dangerous arena mismatch.

The scoped interning law is:

```text
intern(a) = intern(b)  <=>  a = b
resolve(intern(a)) = a
```

Both laws are within one installed arena. They concern raw syntax only:

```text
Term equality       = raw syntax equality
(Environment, Term) = a scoped expression with a sorting judgment
```

The same raw variable term can have different sorts under different environments.

Preventing a handle from escaping one compilation scope and being used in a later scope on the same thread would require branded handles, an arena ID in every handle, or a one-scope-per-process policy. Branding every term would reintroduce pervasive lifetime parameters; arena IDs increase handle size. For the current compiler plugin, one non-nested scope plus thread-confined handles is the appropriate tradeoff. Reconsider arena IDs if `verifier-core` becomes a general multi-session library.

## Keep the borrowed definition representation

The earlier design proposed replacing `Fields<'a, T>` with an owned interned-list handle because ordinary TLS access could not return a reference beyond its closure. The prototype removes that prerequisite.

The existing API can remain during the TLS migration:

```rust
fn get(&self, term: Term) -> TermDef<'_>;
fn get(&self, sort: Sort) -> SortDef<'_>;
fn get(&self, name: Name) -> &str;
```

At a deep read site:

```rust
scoped!(let interners = INTERNERS);
let interners = interners.borrow();
let definition = interners.get(term);
```

`definition` is bounded first by the `Ref<Interners>` guard and then by the generative lexical reference. No `'static` lifetime is fabricated.

When an algorithm must inspect a definition and then mutate the interner recursively, end the shared borrow first. Variable-length fields can be copied into the existing `SmallVec` pattern:

```rust
let fields: SmallVec<[_; 4]> = {
    scoped!(let interners = INTERNERS);
    let interners = interners.borrow();
    match interners.get(term).kind {
        TermKind::Tuple(fields) => fields.into(),
        _ => return Err(TypeError::ExpectedTuple(term)),
    }
};

// The RefCell shared borrow is gone; recursive checking may intern new sorts.
for field in fields {
    check(field)?;
}
```

Read-only recursive operations such as SMT formatting need no copy: borrow once around the full traversal.

Changing `Fields` into an owned list handle may still be worthwhile independently if it simplifies the IR or improves a measured hot path, but it is no longer a TLS prerequisite and should not be bundled into the first implementation.

The one-field wrapper remains unnecessary:

```rust
pub struct TermDef<'a> {
    pub kind: TermKind<'a>,
}
```

Unless future metadata is planned, collapse it into one enum named `TermDef<'a>`:

```rust
pub enum TermDef<'a> {
    Var(Var),
    Const(i128),
    Bool(bool),
    Unit,
    Binary { op: Op, lhs: Term, rhs: Term },
    Unary { op: Uop, expr: Term },
    Call { function: Var, arguments: Fields<'a, Term> },
    Tuple(Fields<'a, Term>),
    Proj { tuple: Term, field: Field },
}
```

This simplification is independent of TLS and matches the project's preference for fewer nominal wrappers.

## Restore a polymorphic raw `Intern`

The old two-parameter trait carried information the input type already supplies. After extrinsic sorting, a raw interning trait can honestly mean only “canonicalize this definition”:

```rust
pub trait Intern {
    type Id;

    fn intern(self) -> Self::Id;
}
```

Implement it for definition types:

```rust
impl Intern for TermDef<'_> {
    type Id = Term;

    fn intern(self) -> Term {
        scoped!(let interners = INTERNERS);
        interners.borrow_mut().intern_term(self)
    }
}

impl Intern for SortDef<'_> {
    type Id = Sort;

    fn intern(self) -> Sort {
        scoped!(let interners = INTERNERS);
        interners.borrow_mut().intern_sort(self)
    }
}

impl Intern for &str {
    type Id = Name;

    fn intern(self) -> Name {
        scoped!(let interners = INTERNERS);
        interners.borrow_mut().intern_name(self)
    }
}
```

Call sites become:

```rust
let int = SortDef::Int.intern();
let name = "value".intern();
let term = TermDef::Const(42).intern();
```

The output type is determined by the definition type; there is no `intern_term`/`intern_sort` vocabulary at ordinary call sites.

Making raw `Intern` public is compatible with extrinsic sorting because it does not claim that a term is sorted. It only preserves structural identity. Typed APIs and checked boundaries are responsible for the judgment `Gamma |- term : sort`.

## Remove `Builder` without hiding the environment

The current [`Builder`](/home/xks/repos/example/crates/verifier-core/src/environment.rs:108) exists because construction requires mutable access to both `Context` and the environment sort cache:

```rust
pub struct Builder<'a, B> {
    context: &'a mut Context,
    environment: &'a mut Environment<B>,
}
```

Once interning is implicit, the remaining mutation during typed construction is the non-semantic sort cache. Change:

```rust
sorts: HashMap<Term, Sort>
```

to:

```rust
sorts: RefCell<HashMap<Term, Sort>>
```

Then typed operations can take `&self`:

```rust
impl<B> Environment<B> {
    pub fn sort(&self, term: Term) -> Result<Sort, TypeError>;

    pub fn var(&self, var: Var) -> Term;
    pub fn int(&self, value: i128) -> Term;
    pub fn bool(&self, value: bool) -> Term;
    pub fn unit(&self) -> Term;
    pub fn tuple(&self, fields: &[Term]) -> Term;
    pub fn proj(&self, tuple: Term, field: impl Into<Field>) -> Term;
    pub fn call(&self, function: Var, arguments: &[Term]) -> Term;
    pub fn binary(&self, op: Op, lhs: Term, rhs: Term) -> Term;
    pub fn unary(&self, op: Uop, term: Term) -> Term;
}
```

Declaration mutation remains explicit and append-only:

```rust
pub fn bind_value(&mut self, ...);
pub fn bind_function(&mut self, ...);
```

Now nested constructor syntax compiles:

```rust
let condition = environment.and(
    environment.ge(environment.var(x), environment.int(0)),
    environment.le(environment.var(x), environment.int(10)),
);
```

TLS removes the interner borrow from the signature. Interior mutability of the cache removes the remaining sequential `&mut Environment` borrow. These are separate changes; both are needed for the full ergonomic improvement.

Restore a constructor-generating macro for repetitive thin wrappers such as `add`, `sub`, `and`, and `implies`. Keep the typing rules centralized in `binary`, `unary`, `call`, `tuple`, and `proj`; the macro should generate vocabulary, not semantic logic.

Interner-only tests can then use raw `.intern()` without constructing an `Environment`, while environment tests exercise checked constructors.

## Raw construction requires checked boundaries

Making raw interning public changes a current assumption. [`smt`](/home/xks/repos/example/crates/verifier-core/src/smt.rs:172) presently treats presence in the cache as evidence that the VC was checked:

```rust
let sort = environment
    .cached_sort(vc)
    .expect("unchecked verification condition");
```

Once callers may intern raw syntax, the cache is only memoization, not a proof token.

Use an explicit checker:

```rust
pub fn smt(
    environment: &Environment<Name>,
    vc: Term,
) -> Result<String, SmtError> {
    let sort = environment.sort(vc)?;
    if sort != SortDef::Bool.intern() {
        return Err(SmtError::ExpectedBool(sort));
    }

    // Formatting may now assume a checked judgment.
}
```

Similarly:

- `Environment::sort` should return a structured `TypeError` for an out-of-range variable, wrong declaration kind, wrong arity, operand mismatch, or invalid projection.
- Convenience constructors may panic on misuse if they are verifier-programming APIs, but should share the same rule implementation.
- A public `Clause` should use a checked constructor or have private fields; it currently permits arbitrary pairs in [`contract/mod.rs`](/home/xks/repos/example/crates/verifier-core/src/contract/mod.rs:11).
- Instantiation should validate its source clause once or accept an already checked clause.
- SMT should validate the Boolean root instead of requiring a warm cache.

Malformed raw syntax may exist; boundaries requiring a sorting judgment return an error.

## Performance assessment

With the generative design, one `scoped!` binding costs approximately:

1. one native/platform TLS access;
2. one pointer load;
3. one null check;
4. creation of a zero-sized generative guard.

After binding, subsequent use is ordinary reference access. This is materially better than re-entering a TLS closure for every field access.

Mutation still requires a `RefCell` borrow check. With the initial whole-context layout:

- `.intern()` performs a TLS binding plus one `borrow_mut` and the existing hash-table/list work;
- a read-only traversal can bind and `borrow()` once for the entire traversal;
- environment sorting additionally accesses its own `RefCell<HashMap<...>>` cache.

The prototype demonstrates the lifetime shape and its tests pass, but it does not by itself establish end-to-end performance. Benchmark against `362f4c8` in the actual rustc-loaded configuration:

1. Repeatedly intern one existing literal or binary node.
2. Intern a large set of unique nodes.
3. Build and sort a shared term DAG.
4. Traverse and format an existing DAG.
5. Verify the existing driver fixtures end to end.
6. Measure peak memory across many bodies.

Record wall time, cycles, instructions, branches, branch misses, and allocations. Inspect optimized assembly to determine whether the actual linkage uses an inline TLS load or a helper such as `__tls_get_addr`.

Initial low-risk optimizations:

- Bind the TLS reference once in read-heavy functions.
- Keep dynamic borrows short in functions that may recursively intern.
- Pre-intern and store `Int`, `Bool`, and unit sort handles instead of hashing them on every operation.
- Copy variable-length fields only when ending a shared borrow is necessary for recursive mutation.
- Split the context `RefCell` only if call patterns justify it.
- Keep `Environment` out of TLS.

### Eliminating `RefCell`

The prototype eliminates closure syntax, not the need to mediate mutation. Returning a bare `&mut Interners` on every lookup would permit aliased mutable references from repeated lookups and is unsound.

The current [`StructInterner`](/home/xks/repos/example/crates/interner/src/struct_interner.rs:13) stores definitions in a movable `IndexVec`, and [`ListInterner`](/home/xks/repos/example/crates/interner/src/list_interner.rs:80) stores elements in a movable `Vec`. Stable references cannot survive arbitrary later insertion into those stores.

If profiling identifies `RefCell` as significant, optimization options are, in order:

1. Amortize TLS lookup and shared borrows over whole phases.
2. Reduce the number of interner calls in typed constructors.
3. Split term, sort, and name storage cells.
4. Introduce stable arena allocation plus a carefully audited append-only interior-mutation API.

The last option is a separate unsafe storage project. Single-threaded execution alone does not make mutable aliasing safe.

## Parallelism policy

The first implementation is explicitly synchronous and thread-confined.

Current verification and SMT rendering run synchronously in the rustc callback, so the unsafe `set` contract is natural. If body verification later becomes parallel:

- Each worker may install its own arena, but all handles must remain on that worker through checking and formatting; or
- A shared compilation arena must use synchronization/sharding and globally coherent handles.

Do not allow two live threads to use unrelated arenas while `Term(u32)` remains `Send`: numerically equal handles would not imply equal definitions.

If async execution is introduced, OS TLS is also semantically wrong because a future may move between threads. Use task-local storage or return to explicit context passing. The generative mechanism deliberately treats suspension as an unsafe-contract violation rather than pretending OS TLS is async-safe.

## Implementation plan

### 1. Pin and validate the TLS primitive

- Add `generative-scoped-tls` as a pinned dependency or vendor it as a small workspace crate; do not reproduce its unsafe core ad hoc in `verifier-core`.
- Retain its normal tests, compile-fail `'static` escape test, nesting/unwind tests, thread-local test, and expected-failure async Miri fixture.
- Document the verifier-specific synchronous `set` safety argument next to the single unsafe call.
- Ensure no safe general-purpose wrapper hides that contract.

### 2. Establish interner laws and performance baseline

- Add focused benchmarks for hit, miss, checked construction, traversal, SMT formatting, and end-to-end verification.
- State the arena-scoped `iff` and resolve laws in documentation.
- Add verifier tests for environment-dependent sorting.
- Decide and test that `Term`, `Sort`, and `Name` are thread-confined.

### 3. Introduce generative scoped interning without changing the IR representation

- Rename `Context` to `Interners`.
- Install `RefCell<Interners>` around the full synchronous owner loop.
- Reject nested verifier interner scopes even though the primitive supports nesting.
- Add the associated-output `Intern` trait.
- Replace `cx` parameters with `scoped!(let interners = INTERNERS)` at the few functions that need direct storage access.
- Keep `TermDef<'_>`, `SortDef<'_>`, `Fields<'_, T>`, and the current stored/borrowed conversion during this stage.
- Remove `cx` from `Verification`, parser, instantiation, executor, SMT, and helper signatures.

At the end of this stage, storage is implicit but the calculus representation has not been conflated with the TLS migration.

### 4. Remove `Builder`

- Put the environment sort cache in `RefCell<HashMap<Term, Sort>>`.
- Change sorting and typed constructors to take `&self`.
- Move the typed constructor family from `Builder` onto `Environment`.
- Restore the thin constructor macro.
- Delete `Builder`.
- Add a compile/runtime test for nested constructor expressions.

### 5. Open raw interning and strengthen checked boundaries

- Export `Intern`.
- Collapse the one-field `TermDef` wrapper if no metadata is planned.
- Introduce `TypeError` and make `Environment::sort` return `Result`.
- Make SMT check its input rather than relying on a cached sort.
- Check or encapsulate `Clause` construction.
- Ensure instantiation reports malformed source/target terms without panicking.

### 6. Measure before changing storage

- Compare the generative TLS version to the baseline in the actual driver.
- Inspect TLS code generation and dynamic-borrow counts.
- Pre-intern primitive sorts and amortize read borrows.
- Split storage cells or redesign `Fields` only if profiles show a concrete benefit.
- Consider stable arena storage only if the safe design's overhead is material.

## Required tests

At minimum:

1. Equal term definitions intern to one handle.
2. Unequal term definitions do not share a handle.
3. The same laws hold for sorts, lists, and names.
4. `resolve(intern(def)) == def` within one arena.
5. The same raw `Var(0)` term has different sorts under two environments.
6. Those sort caches do not interfere.
7. Raw ill-sorted syntax can be interned.
8. Checking ill-sorted syntax returns `TypeError`.
9. SMT rejects an unchecked or non-Boolean VC with an error.
10. A nested constructor expression compiles and produces the expected term.
11. `scoped!` yields an ordinary `&RefCell<Interners>` or `&Interners` as designed.
12. A branded reference cannot be coerced to `'static` or returned from its lexical scope.
13. TLS access outside an installed binding fails clearly.
14. The verifier rejects nested arena installation.
15. Panic unwinding restores/unsets the binding.
16. The async misuse fixture fails under Miri when the unsafe `set` contract is deliberately violated.
17. `Term`, `Sort`, `Name`, `Environment`, and `Verification` do not cross threads.
18. One compilation processes and formats several bodies inside one arena.
19. `Verification` no longer contains storage machinery.
20. Interner-only tests do not create an `Environment`.
21. Environment-constructor tests do not mention `Context` or `Builder`.

## Bottom line

The prototype improves the design substantially:

1. Use generative scoped TLS to bind a bare reference with one native TLS lookup, rather than forcing every access through a closure.
2. Keep the unsafe `set` boundary explicit and justify the verifier's synchronous call tree; do not expose a generally safe scope wrapper.
3. Keep `Environment` explicit and use interior mutability only for its cache.
4. Remove `Context` from signatures and remove `Builder`, while retaining a private/doc-hidden `Interners` storage type.
5. Keep the existing borrowed definition and `Fields<'_, T>` representation during the first migration; an owned list-handle redesign is optional, not required by TLS.
6. Restore a polymorphic raw `Intern`, and make sorting/SMT boundaries perform real checks.

The intended use becomes:

```rust
let body = || {
    let int = SortDef::Int.intern();

    let mut environment = Environment::new();
    let x = environment.bind_value(int, "x".intern());

    let condition = environment.and(
        environment.ge(environment.var(x), environment.int(0)),
        environment.le(environment.var(x), environment.int(10)),
    );

    let script = smt(&environment, condition)?;
    Ok::<_, Error>(script)
};

// SAFETY: the verification call tree is synchronous; no TLS-derived reference
// survives or suspends beyond this call.
let script = unsafe { intern::scope(body) }?;
```

`Context` and `Builder` disappear from ordinary APIs; the environment remains visible; raw syntax interning is polymorphic; borrowed definitions remain available; and the only unsafe operation is the auditable installation of a compilation-scoped arena.
