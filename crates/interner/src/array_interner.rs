use hashbrown::{
    hash_table::Entry,
    DefaultHashBuilder,
    HashTable,
};
use index_vec::{Idx, IndexVec};

use std::{
    hash::{BuildHasher, Hash},
    ops::Index,
};

/// Interns hashable values and assigns each distinct value a stable index.
///
/// Each value is owned exactly once, by `by_index`. The hash table stores only
/// indices and hashes/compares them by resolving them through `by_index`.
pub struct ArrayInterner<K: Idx, V, S = DefaultHashBuilder> {
    by_value: HashTable<K>,
    by_index: IndexVec<K, V>,
    hash_builder: S,
}

impl<K: Idx, V> ArrayInterner<K, V, DefaultHashBuilder> {
    pub fn new() -> Self {
        Self::with_hasher(DefaultHashBuilder::default())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(
            capacity,
            DefaultHashBuilder::default(),
        )
    }
}

impl<K: Idx, V, S> ArrayInterner<K, V, S> {
    pub fn with_hasher(hash_builder: S) -> Self {
        Self {
            by_value: HashTable::new(),
            by_index: IndexVec::new(),
            hash_builder,
        }
    }

    pub fn with_capacity_and_hasher(
        capacity: usize,
        hash_builder: S,
    ) -> Self {
        Self {
            by_value: HashTable::with_capacity(capacity),
            by_index: IndexVec::with_capacity(capacity),
            hash_builder,
        }
    }

    pub fn resolve(&self, index: K) -> Option<&V> {
        self.by_index.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &V> {
        self.by_index.iter()
    }

    pub fn len(&self) -> usize {
        self.by_index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_index.is_empty()
    }
}

impl<K, V, S> ArrayInterner<K, V, S>
where
    K: Idx,
    V: Eq + Hash,
    S: BuildHasher,
{
    pub fn intern(&mut self, value: V) -> K {
        let hash = self.hash_builder.hash_one(&value);

        // Split the fields so `by_value` can be mutably borrowed while the
        // lookup callbacks read `by_index` and `hash_builder`.
        let Self {
            by_value,
            by_index,
            hash_builder,
        } = self;

        match by_value.entry(
            hash,

            // Compare the candidate index's value with the incoming value.
            |index| by_index[*index] == value,

            // Recompute an existing entry's hash if the table resizes.
            |index| hash_builder.hash_one(&by_index[*index]),
        ) {
            Entry::Occupied(entry) => *entry.get(),

            Entry::Vacant(entry) => {
                let index = by_index.push(value);
                entry.insert(index);
                index
            }
        }
    }
}

impl<K: Idx, V> Default
    for ArrayInterner<K, V, DefaultHashBuilder>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Idx, V, S> Index<K> for ArrayInterner<K, V, S> {
    type Output = V;

    fn index(&self, index: K) -> &Self::Output {
        self.resolve(index)
            .expect("index does not belong to this interner")
    }
}