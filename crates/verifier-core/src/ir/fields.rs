use core::slice;
use std::ops::Deref;

use index_vec::IndexSlice;
use smallvec::SmallVec;

use crate::{Field, Fields};

impl<'c, T> Fields<'c, T> {
    pub fn new(fields: &'c [T]) -> Self {
        Self(IndexSlice::new(fields))
    }
}

impl<T> AsRef<[T]> for Fields<'_, T> {
    fn as_ref(&self) -> &[T] {
        self.0.as_ref()
    }
}

impl<T> Deref for Fields<'_, T> {
    type Target = IndexSlice<Field, [T]>;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a, T> IntoIterator for Fields<'a, T> {
    type Item = &'a T;
    type IntoIter = slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> slice::Iter<'a, T> {
        self.0.iter()
    }
}

// tiny interop, love it.
impl<T: Clone, const N: usize> Into<SmallVec<[T; N]>> for Fields<'_, T> {
    fn into(self) -> SmallVec<[T; N]> {
        self.as_ref().into()
    }
}
