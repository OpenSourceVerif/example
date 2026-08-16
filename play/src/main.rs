use std::cell::RefCell;
use std::fmt::Debug;
use std::io::{self, Read};
use std::ops::Deref;
use std::str::{FromStr, SplitWhitespace};

/// Public API: deliberately has no lifetime parameter.
pub struct Scanner {
    // This `'static` is an implementation detail and a lie maintained by
    // `ScannerTemp`: the backing `Box<str>` actually owns the bytes.
    input: RefCell<SplitWhitespace<'static>>,
}

impl Scanner {
    pub fn next<T>(&self) -> T
    where
        T: FromStr,
        T::Err: Debug,
    {
        self.input
            .borrow_mut()
            .next()
            .expect("input exhausted")
            .parse()
            .expect("failed to parse token")
    }
}

/// Owns both a value and the storage that value may borrow from.
///
/// `value` must be declared before `owner`: struct fields are dropped in
/// declaration order, so the dependent value is destroyed first.
#[doc(hidden)]
pub struct ScannerTemp {
    value: Scanner,
    _owner: Box<str>,
}

impl Deref for ScannerTemp {
    type Target = Scanner;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[doc(hidden)]
pub fn __scanner_temp() -> ScannerTemp {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();

    ScannerTemp::from_owner(input.into_boxed_str())
}

impl ScannerTemp {
    fn from_owner(owner: Box<str>) -> Self {
        // Moving the Box does not move its heap allocation, and the owner is
        // never exposed for mutation. The pointer therefore remains valid
        // until ScannerTemp is dropped.
        let ptr: *const str = owner.as_ref();

        // SAFETY:
        // - `ptr` points into `owner`'s allocation;
        // - `owner` is stored in the same ScannerTemp as `value`;
        // - callers receive only `&Scanner`, whose lifetime is tied to
        //   ScannerTemp;
        // - Scanner's safe API cannot return a borrowed token;
        // - `value` is dropped before `owner`.
        let input: &'static str = unsafe { &*ptr };
        let value = Scanner { input: RefCell::new(input.split_whitespace()) };

        ScannerTemp { value, _owner: owner }
    }
}

#[macro_export]
macro_rules! scanner_new {
    () => {
        // The outer borrow is essential. In a `let` initializer it triggers
        // temporary lifetime extension for the ScannerTemp returned here.
        &*$crate::__scanner_temp()
    };
}

fn main() {
    let sc = scanner_new!();

    let n: usize = sc.next();
    let mut a = Vec::with_capacity(n);

    for _ in 0..n {
        a.push(sc.next::<i64>());
    }

    println!("{a:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_from_a_lifetime_extended_temporary() {
        let sc = &*ScannerTemp::from_owner("5 3 -1 4 1 5".into());

        let n: usize = sc.next();
        let a: Vec<i64> = (0..n).map(|_| sc.next()).collect();

        assert_eq!(a, [3, -1, 4, 1, 5]);
    }
}
