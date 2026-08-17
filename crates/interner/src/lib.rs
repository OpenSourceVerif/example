//! Arena-backed canonical storage for lifetime-varying definitions.
//!
//! The crate owns the raw pointer and lifetime-rebranding boundary. Handles are
//! deliberately unbranded: callers must establish the dynamic rule that a
//! handle is resolved only by the interner which created it and while that
//! interner is alive.

extern crate self as interner;

use std::{
    cell::RefCell,
    fmt,
    hash::{BuildHasher, Hash, Hasher},
    marker::PhantomData,
    mem,
    ptr::NonNull,
    rc::Rc,
};

use bumpalo::Bump;
use hashbrown::{DefaultHashBuilder, HashTable};
pub use interner_derive::Covariant;

/// A definition family whose arena lifetime can be shortened.
///
/// Derive this trait so the generated identity reborrow makes covariance a
/// compiler-checked structural property.
///
/// ```compile_fail
/// #[derive(Clone, Copy, interner::Covariant)]
/// struct Contravariant<'a>(fn(&'a ()));
/// ```
///
/// # Safety
///
/// Every `Value<'a>` must have the same layout, and changing only its lifetimes
/// must be valid. [`Covariant::shorten`] must reborrow the same definition with
/// only its internal lifetimes shortened.
pub unsafe trait Covariant: 'static {
    type Value<'a>: Copy;

    fn shorten<'long: 'short, 'short>(
        value: &'short Self::Value<'long>,
    ) -> &'short Self::Value<'short>;
}

/// Describes one family of definitions stored by an [`Interner`].
///
/// [`Definition::hash`] and [`Definition::equivalent`] define canonical value
/// equality across input and stored representations. Inputs equivalent to the
/// same stored value must hash equally. Violating that rule can create duplicate
/// identities, but is not a memory-safety violation.
///
/// [`Definition::alloc`] must allocate the returned value through the supplied
/// [`Arena`]. Its type ensures that contained references remain valid for the
/// arena lifetime. Published values must remain immutable.
pub trait Definition: Covariant {
    type Input<'a>;

    fn hash<H: Hasher>(input: &Self::Input<'_>, state: &mut H);

    fn equivalent<'a, 'b>(value: &Self::Value<'a>, input: &Self::Input<'b>) -> bool;

    fn alloc<'arena, 'input>(
        arena: Arena<'arena>,
        input: Self::Input<'input>,
    ) -> Stored<'arena, Self::Value<'arena>>;
}

/// A one-pointer identity returned by an [`Interner`].
///
/// Identities are thread-confined and carry no arena lifetime or ownership
/// token. They may be copied and compared after their interner is dropped, but
/// may no longer be resolved.
#[repr(transparent)]
pub struct Interned<D> {
    pointer: NonNull<()>,
    definition: PhantomData<fn() -> D>,
    thread: PhantomData<Rc<()>>,
}

impl<D> Interned<D> {
    fn new(pointer: NonNull<()>) -> Self {
        Self { pointer, definition: PhantomData, thread: PhantomData }
    }
}

impl<D> Clone for Interned<D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D> Copy for Interned<D> {}

impl<D> PartialEq for Interned<D> {
    fn eq(&self, other: &Self) -> bool {
        self.pointer == other.pointer
    }
}

impl<D> Eq for Interned<D> {}

impl<D> PartialOrd for Interned<D> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<D> Ord for Interned<D> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.pointer.cmp(&other.pointer)
    }
}

impl<D> Hash for Interned<D> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pointer.hash(state);
    }
}

impl<D> fmt::Debug for Interned<D> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Interned").field(&self.pointer).finish()
    }
}

/// Restricted access to an interner's bump arena while allocating a value.
#[derive(Clone, Copy)]
pub struct Arena<'a> {
    bump: &'a Bump,
}

impl<'a> Arena<'a> {
    pub fn alloc<T: Copy>(self, value: T) -> Stored<'a, T> {
        Stored { pointer: NonNull::from(self.bump.alloc(value)), lifetime: PhantomData }
    }

    pub fn copy_slice<T: Copy>(self, values: &[T]) -> &'a [T] {
        self.bump.alloc_slice_copy(values)
    }

    pub fn copy_str(self, text: &str) -> &'a str {
        self.bump.alloc_str(text)
    }
}

/// Proof that a definition was allocated in the supplied [`Arena`].
pub struct Stored<'a, T> {
    pointer: NonNull<T>,
    lifetime: PhantomData<&'a T>,
}

struct Entry<D> {
    identity: Interned<D>,
    hash: u64,
}

/// Canonical append-only storage for one definition family.
///
/// The table is dropped before the arena and contains only copied identities.
/// Published definitions never move. `RefCell` makes accidental reentrant
/// interning a panic instead of an aliasing violation. Definitions must not be
/// zero-sized because their addresses are their identities.
pub struct Interner<D: Definition> {
    table: RefCell<HashTable<Entry<D>>>,
    hash_builder: DefaultHashBuilder,
    arena: Bump,
}

impl<D: Definition> Interner<D> {
    pub fn new() -> Self {
        assert_ne!(mem::size_of::<D::Value<'static>>(), 0, "interned definitions must have size");
        Self {
            table: RefCell::new(HashTable::new()),
            hash_builder: DefaultHashBuilder::default(),
            arena: Bump::new(),
        }
    }

    pub fn intern<'a>(&self, input: D::Input<'a>) -> Interned<D> {
        let mut state = self.hash_builder.build_hasher();
        D::hash(&input, &mut state);
        let hash = state.finish();
        let mut table = self.table.borrow_mut();
        if let Some(entry) = table.find(hash, |entry| {
            // SAFETY: every table entry was created by this interner, and the
            // arena outlives the table and this lookup.
            let value = unsafe { self.resolve_unchecked(entry.identity) };
            D::equivalent(value, &input)
        }) {
            return entry.identity;
        }

        let stored = D::alloc(Arena { bump: &self.arena }, input);
        let identity = Interned::new(stored.pointer.cast());
        table.insert_unique(hash, Entry { identity, hash }, |entry| entry.hash);
        identity
    }

    /// Resolves an identity without checking its arena domain.
    ///
    /// # Safety
    ///
    /// `identity` must have been created by this interner, and this interner
    /// must not have been dropped since its creation.
    pub unsafe fn resolve_unchecked<'a>(&'a self, identity: Interned<D>) -> &'a D::Value<'a> {
        // SAFETY: delegated to the caller. The erased pointer addresses a value
        // allocated by `D::alloc`; its private `'static` is only an
        // implementation lifetime and is immediately shortened by `D`.
        let value = unsafe { &*identity.pointer.as_ptr().cast::<D::Value<'static>>() };
        D::shorten(value)
    }
}

impl<D: Definition> Default for Interner<D> {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: shared references are covariant over their referent lifetime, and the
// implementation reborrows the same stored reference.
unsafe impl Covariant for &'static str {
    type Value<'a> = &'a str;

    fn shorten<'long: 'short, 'short>(value: &'short &'long str) -> &'short &'short str {
        value
    }
}

impl Definition for &'static str {
    type Input<'a> = &'a str;

    fn hash<H: Hasher>(input: &&str, state: &mut H) {
        Hash::hash(input, state);
    }

    fn equivalent(value: &&str, input: &&str) -> bool {
        value == input
    }

    fn alloc<'arena>(arena: Arena<'arena>, input: &str) -> Stored<'arena, &'arena str> {
        let text = arena.copy_str(input);
        arena.alloc(text)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        hash, mem,
        panic::{AssertUnwindSafe, catch_unwind},
        ptr,
    };

    use super::{Arena, Covariant, Definition, Interner, Stored};

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    struct Number(u32);

    // SAFETY: `Number` contains no lifetimes to rebrand.
    unsafe impl Covariant for Number {
        type Value<'a> = Number;

        fn shorten<'long: 'short, 'short>(value: &'short Number) -> &'short Number {
            value
        }
    }

    impl Definition for Number {
        type Input<'a> = Number;

        fn hash<H: hash::Hasher>(input: &Number, state: &mut H) {
            hash::Hash::hash(input, state);
        }

        fn equivalent(value: &Number, input: &Number) -> bool {
            value == input
        }

        fn alloc<'arena, 'input>(arena: Arena<'arena>, input: Number) -> Stored<'arena, Number> {
            arena.alloc(input)
        }
    }

    #[test]
    fn equal_values_share_an_identity() {
        let interner = Interner::<Number>::new();
        let first = interner.intern(Number(3));
        let second = interner.intern(Number(3));

        assert_eq!(first, second);
        // SAFETY: `first` belongs to the live `interner`.
        assert_eq!(unsafe { interner.resolve_unchecked(first) }, &Number(3));
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Colliding(u32);

    impl hash::Hash for Colliding {
        fn hash<H: hash::Hasher>(&self, _state: &mut H) {}
    }

    // SAFETY: `Colliding` contains no lifetimes to rebrand.
    unsafe impl Covariant for Colliding {
        type Value<'a> = Colliding;

        fn shorten<'long: 'short, 'short>(value: &'short Colliding) -> &'short Colliding {
            value
        }
    }

    impl Definition for Colliding {
        type Input<'a> = Colliding;

        fn hash<H: hash::Hasher>(input: &Colliding, state: &mut H) {
            hash::Hash::hash(input, state);
        }

        fn equivalent(value: &Colliding, input: &Colliding) -> bool {
            value == input
        }

        fn alloc<'arena, 'input>(
            arena: Arena<'arena>,
            input: Colliding,
        ) -> Stored<'arena, Colliding> {
            arena.alloc(input)
        }
    }

    #[test]
    fn compares_values_when_hashes_collide() {
        let interner = Interner::<Colliding>::new();
        let first = interner.intern(Colliding(1));
        let second = interner.intern(Colliding(2));

        assert_ne!(first, second);
        assert_eq!(interner.intern(Colliding(1)), first);
    }

    #[test]
    fn references_survive_table_and_arena_growth() {
        let interner = Interner::<Number>::new();
        let first = interner.intern(Number(0));
        // SAFETY: `first` belongs to the live `interner`.
        let value = unsafe { interner.resolve_unchecked(first) };

        for number in 1..10_000 {
            interner.intern(Number(number));
        }

        assert_eq!(value, &Number(0));
        assert_eq!(mem::size_of_val(&first), mem::size_of::<usize>());
    }

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    struct Reentrant(u32);

    thread_local! {
        static REENTER: Cell<*const Interner<Reentrant>> = const { Cell::new(ptr::null()) };
    }

    // SAFETY: `Reentrant` contains no lifetimes to rebrand.
    unsafe impl Covariant for Reentrant {
        type Value<'a> = Reentrant;

        fn shorten<'long: 'short, 'short>(value: &'short Reentrant) -> &'short Reentrant {
            value
        }
    }

    impl Definition for Reentrant {
        type Input<'a> = Reentrant;

        fn hash<H: hash::Hasher>(input: &Reentrant, state: &mut H) {
            hash::Hash::hash(input, state);
        }

        fn equivalent(value: &Reentrant, input: &Reentrant) -> bool {
            REENTER.with(|slot| {
                let pointer = slot.replace(ptr::null());
                if !pointer.is_null() {
                    // SAFETY: the test installs a pointer to its live local
                    // interner and clears it before making the nested call.
                    unsafe { (&*pointer).intern(*input) };
                }
            });
            value == input
        }

        fn alloc<'arena, 'input>(
            arena: Arena<'arena>,
            input: Reentrant,
        ) -> Stored<'arena, Reentrant> {
            arena.alloc(input)
        }
    }

    #[test]
    fn reentrant_interning_panics_without_corrupting_the_table() {
        let interner = Interner::<Reentrant>::new();
        let first = interner.intern(Reentrant(1));
        REENTER.with(|slot| slot.set(&interner));

        let result = catch_unwind(AssertUnwindSafe(|| interner.intern(Reentrant(1))));

        assert!(result.is_err());
        assert_eq!(interner.intern(Reentrant(1)), first);
    }

    #[test]
    fn copies_borrowed_input() {
        let interner = Interner::<&'static str>::new();
        let mut input = String::from("name");
        let name = interner.intern(&input);
        input.push_str(" changed");

        // SAFETY: `name` belongs to the live `interner`.
        assert_eq!(*unsafe { interner.resolve_unchecked(name) }, "name");
    }
}
