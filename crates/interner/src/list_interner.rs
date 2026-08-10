use std::{
    fmt,
    hash::{BuildHasher, Hash, Hasher},
    marker::PhantomData,
    ops::{Index, Range},
};

use hashbrown::{DefaultHashBuilder, HashTable};

/// Identifies an interned list by its range in the interner's element storage.
#[repr(C)]
pub struct List<T> {
    start: u32,
    len: u32,
    marker: PhantomData<fn() -> T>,
}

impl<T> List<T> {
    fn new(start: usize, len: usize) -> Self {
        let start = u32::try_from(start).expect("list interner contains too many elements");
        let len = u32::try_from(len).expect("interned list contains too many elements");
        start.checked_add(len).expect("list interner contains too many elements");

        Self { start, len, marker: PhantomData }
    }

    fn range(self) -> Range<usize> {
        let start = self.start as usize;
        start..start + self.len as usize
    }

    pub fn len(self) -> usize {
        self.len as usize
    }

    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

impl<T> Clone for List<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for List<T> {}

impl<T> fmt::Debug for List<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("List").field("start", &self.start).field("len", &self.len).finish()
    }
}

impl<T> PartialEq for List<T> {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start && self.len == other.len
    }
}

impl<T> Eq for List<T> {}

impl<T> Hash for List<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.start.hash(state);
        self.len.hash(state);
    }
}

struct Entry<T> {
    list: List<T>,
    hash: u64,
}

/// Interns slices in flat element storage and assigns each distinct slice a
/// stable range handle.
///
/// Equal slices are stored once. Handles do not borrow the interner; use
/// [`resolve`](Self::resolve) or indexing to access their elements.
pub struct ListInterner<T, S = DefaultHashBuilder> {
    by_value: HashTable<Entry<T>>,
    values: Vec<T>,
    hash_builder: S,
}

impl<T> ListInterner<T, DefaultHashBuilder> {
    pub fn new() -> Self {
        Self::with_hasher(DefaultHashBuilder::default())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, DefaultHashBuilder::default())
    }
}

impl<T, S> ListInterner<T, S> {
    pub fn with_hasher(hash_builder: S) -> Self {
        Self { by_value: HashTable::new(), values: Vec::new(), hash_builder }
    }

    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        Self { by_value: HashTable::with_capacity(capacity), values: Vec::new(), hash_builder }
    }

    pub fn resolve(&self, list: List<T>) -> Option<&[T]> {
        self.values.get(list.range())
    }
}

impl<T, S> ListInterner<T, S>
where
    T: Clone + Eq + Hash,
    S: BuildHasher,
{
    pub fn intern(&mut self, values: &[T]) -> List<T> {
        let hash = self.hash_builder.hash_one(values);

        if let Some(entry) =
            self.by_value.find(hash, |entry| self.values[entry.list.range()] == *values)
        {
            return entry.list;
        }

        let list = List::new(self.values.len(), values.len());
        self.values.extend_from_slice(values);
        self.by_value.insert_unique(hash, Entry { list, hash }, |entry| entry.hash);
        list
    }
}

impl<T> Default for ListInterner<T, DefaultHashBuilder> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, S> Index<List<T>> for ListInterner<T, S> {
    type Output = [T];

    fn index(&self, list: List<T>) -> &Self::Output {
        self.resolve(list).expect("handle does not belong to this interner")
    }
}

#[cfg(test)]
mod tests {
    use std::{hash, mem};

    use super::{List, ListInterner};

    #[test]
    fn handle_is_two_u32s() {
        assert_eq!(mem::size_of::<List<u8>>(), 2 * mem::size_of::<u32>());
    }

    #[test]
    fn equal_lists_share_a_handle() {
        let mut interner = ListInterner::<i32>::new();

        let first = interner.intern(&[1, 2, 3]);
        let second = interner.intern(&[1, 2, 3]);

        assert_eq!(first, second);
        assert_eq!(&interner[first], &[1, 2, 3]);
    }

    #[test]
    fn distinct_lists_have_distinct_handles() {
        let mut interner = ListInterner::<i32>::with_capacity(2);

        let first = interner.intern(&[1, 2]);
        let second = interner.intern(&[1, 3]);

        assert_ne!(first, second);
        assert_eq!(interner.resolve(second), Some(&[1, 3][..]));
    }

    #[derive(Clone, Eq, PartialEq)]
    struct Colliding(u8);

    impl hash::Hash for Colliding {
        fn hash<H: hash::Hasher>(&self, _state: &mut H) {}
    }

    #[test]
    fn compares_contents_when_hashes_collide() {
        let mut interner = ListInterner::<Colliding>::new();

        let first = interner.intern(&[Colliding(1)]);
        let second = interner.intern(&[Colliding(2)]);
        let first_again = interner.intern(&[Colliding(1)]);

        assert_ne!(first, second);
        assert_eq!(first, first_again);
    }

    #[test]
    fn clones_the_input() {
        let mut interner = ListInterner::<String>::new();
        let mut input = [String::from("first"), String::from("second")];
        let list = interner.intern(&input);

        input[0].push_str(" changed");

        assert_eq!(input[0], "first changed");
        assert_eq!(interner[list], ["first", "second"]);
    }

    #[test]
    fn supports_empty_and_zero_sized_lists() {
        let mut interner = ListInterner::<()>::new();

        let empty = interner.intern(&[]);
        let three = interner.intern(&[(); 3]);

        assert!(empty.is_empty());
        assert_eq!(three.len(), 3);
        assert!(interner[empty].is_empty());
        assert_eq!(interner[three].len(), 3);
    }

    #[repr(align(64))]
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    struct Aligned(u8);

    #[test]
    fn aligns_elements() {
        let mut interner = ListInterner::<Aligned>::new();
        let list = interner.intern(&[Aligned(1), Aligned(2)]);
        let values = &interner[list];

        assert_eq!(values.as_ptr().addr() % mem::align_of::<Aligned>(), 0);
        assert_eq!(values[1], Aligned(2));
    }
}
