//! Synchronous scoped thread-local storage with lexical bare references.
//!
//! Native TLS stores a raw pointer to a value borrowed from an outer stack
//! frame. [`scoped!`] reconstructs an ordinary `&T` whose lifetime is bounded
//! by a temporary proxy at the lookup site.
//!
//! ```
//! use scoped_tls::{scoped, scoped_thread_local};
//!
//! struct Context { answer: u32 }
//! scoped_thread_local!(static CONTEXT: Context);
//!
//! fn deep() {
//!     let context = scoped!(CONTEXT);
//!     assert_eq!(context.answer, 42);
//! }
//!
//! let context = Context { answer: 42 };
//! let body = || deep();
//! // SAFETY: this call tree is synchronous. No reference obtained through
//! // `scoped!` remains usable after this `set` invocation returns.
//! unsafe { CONTEXT.set(&context, body) };
//! ```

use std::{cell::Cell, fmt, marker::PhantomData, ops::Deref, thread::LocalKey};

/// A native thread-local key which temporarily borrows its value.
pub struct ScopedKey<T> {
    slot: &'static LocalKey<Cell<*const ()>>,
    invariant: PhantomData<fn(T) -> T>,
}

// SAFETY: the key contains no shared `T`. Its mutable pointer slot is distinct
// for every OS thread.
unsafe impl<T> Sync for ScopedKey<T> {}

/// The temporary lifetime anchor created by [`scoped!`].
#[doc(hidden)]
pub struct ScopedRef<T> {
    pointer: *const T,
}

impl<T> Deref for ScopedRef<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        // SAFETY: construction requires the pointer to remain valid for every
        // reference derived through this proxy.
        unsafe { &*self.pointer }
    }
}

impl<T> ScopedKey<T> {
    #[doc(hidden)]
    pub const fn __new(slot: &'static LocalKey<Cell<*const ()>>) -> Self {
        Self { slot, invariant: PhantomData }
    }

    /// Installs `value` while `body` executes on the current thread.
    ///
    /// The previous binding is restored on return and panic unwinding.
    ///
    /// # Safety
    ///
    /// Every reference produced by [`scoped!`] from this binding must become
    /// unusable before this invocation returns. In particular, a future,
    /// coroutine, or generator must not suspend while retaining such a
    /// reference and then let this invocation return.
    pub unsafe fn set<R>(&'static self, value: &T, body: impl FnOnce() -> R) -> R {
        struct Reset {
            slot: &'static LocalKey<Cell<*const ()>>,
            previous: *const (),
        }

        impl Drop for Reset {
            fn drop(&mut self) {
                self.slot.with(|slot| slot.set(self.previous));
            }
        }

        let previous = self.slot.with(|slot| {
            let previous = slot.get();
            slot.set(value as *const T as *const ());
            previous
        });
        let _reset = Reset { slot: self.slot, previous };
        body()
    }

    #[inline]
    pub fn is_set(&'static self) -> bool {
        self.slot.with(|slot| !slot.get().is_null())
    }

    /// Provides closure-bounded access when a bare reference is unnecessary.
    #[inline]
    pub fn with<R>(&'static self, body: impl FnOnce(&T) -> R) -> R {
        let pointer = self.current_ptr();
        // SAFETY: `pointer` is non-null and the installing `set` contract keeps
        // it live for this synchronous call.
        unsafe { body(&*pointer) }
    }

    #[inline]
    fn current_ptr(&'static self) -> *const T {
        let pointer = self.slot.with(Cell::get);
        assert!(!pointer.is_null(), "scoped TLS key is not set");
        pointer.cast()
    }

    #[doc(hidden)]
    #[inline]
    pub unsafe fn __scoped_ref(&'static self) -> ScopedRef<T> {
        ScopedRef { pointer: self.current_ptr() }
    }
}

impl<T> fmt::Debug for ScopedKey<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("ScopedKey").finish_non_exhaustive()
    }
}

/// Declares one scoped native TLS key.
#[macro_export]
macro_rules! scoped_thread_local {
    ($(#[$attribute:meta])* $visibility:vis static $name:ident : $type:ty $(;)?) => {
        $(#[$attribute])*
        $visibility static $name: $crate::ScopedKey<$type> = $crate::ScopedKey::__new({
            ::std::thread_local! {
                static SLOT: ::std::cell::Cell<*const ()> = const {
                    ::std::cell::Cell::new(::std::ptr::null())
                };
            }
            &SLOT
        });
    };
}

/// Returns `&T` from the current binding.
///
/// Use this as a `let` initializer: `let value = scoped!(KEY);`
///
/// no expression-level composition beyond that form are promised.
///
/// ```compile_fail
/// use scoped_tls::{scoped, scoped_thread_local};
/// scoped_thread_local!(static NUMBER: u32);
///
/// fn escape() -> &'static u32 {
///     scoped!(NUMBER)
/// }
/// ```
#[macro_export]
macro_rules! scoped {
    ($key:expr $(,)?) => {{
        match &$key {
            key => unsafe {
                // SAFETY: this fresh proxy lexically bounds derived references.
                // The dynamic obligation belongs to the installing `set` call.
                &*key.__scoped_ref()
            },
        }
    }};
}

#[cfg(test)]
mod tests {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::Barrier,
        thread,
    };

    scoped_thread_local!(static NUMBER: u32);

    #[test]
    fn returns_a_lexical_reference() {
        let number = 42;
        let body = || {
            let number = scoped!(NUMBER);
            let _: &u32 = number;
            assert_eq!(*number, 42);
        };
        // SAFETY: `body` is synchronous and its reference cannot escape.
        unsafe { NUMBER.set(&number, body) };
        assert!(!NUMBER.is_set());
    }

    #[test]
    fn restores_after_unwind() {
        let number = 3;
        let body = || panic!("boom");
        let result = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: `body` is synchronous and does not retain a reference.
            unsafe { NUMBER.set(&number, body) };
        }));
        assert!(result.is_err());
        assert!(!NUMBER.is_set());
    }

    #[test]
    fn each_thread_has_an_independent_binding() {
        static READY: Barrier = Barrier::new(2);
        thread::scope(|scope| {
            scope.spawn(|| run_thread(11));
            run_thread(22);
        });

        fn run_thread(expected: u32) {
            let body = || {
                READY.wait();
                let actual = scoped!(NUMBER);
                assert_eq!(*actual, expected);
            };
            // SAFETY: `body` is synchronous and its reference cannot escape.
            unsafe { NUMBER.set(&expected, body) };
        }
    }
}
