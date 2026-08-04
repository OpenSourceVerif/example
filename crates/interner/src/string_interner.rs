use std::ops::Index;

use bumpalo::Bump;
use hashbrown::HashMap;
use hashbrown::hash_map::EntryRef;
use index_vec::Idx;
use index_vec::IndexVec as Vec;

pub struct StringInterner<K: Idx> {
    // These must be dropped before `arena`.
    by_text: HashMap<&'static str, K>,
    by_name: Vec<K, &'static str>,

    // Kept in a Box so moving StringInterner does not move this allocation.
    // Declared last so it is dropped last.
    arena: Box<Bump>,
}

impl<K: Idx> StringInterner<K> {
    pub fn new() -> Self {
        Self { by_text: HashMap::new(), by_name: Vec::new(), arena: Box::new(Bump::new()) }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            by_text: HashMap::with_capacity(capacity),
            by_name: Vec::with_capacity(capacity),
            arena: Box::new(Bump::new()),
        }
    }

    pub fn intern(&mut self, text: &str) -> K {
        match self.by_text.entry_ref(text) {
            EntryRef::Occupied(entry) => return *entry.get(),
            EntryRef::Vacant(entry) => {
                let allocated: &str = self.arena.alloc_str(text);

                let stored: &'static str = unsafe {
                    /*
                     * SAFETY:
                     *
                     * - `allocated` points into `self.arena`.
                     * - The arena is privately owned and is never reset or replaced.
                     * - `by_text` and `by_name` are dropped before `arena`.
                     * - The forged `'static` lifetime is never exposed publicly.
                     * - `resolve` returns references bounded by the borrow of `self`.
                     */
                    std::mem::transmute::<&str, &'static str>(allocated)
                };

                let name = self.by_name.push(stored);
                unsafe {
                    /*
                     * SAFETY:
                     * `stored` was produced by `alloc_str(text)`, so it contains exactly
                     * the same bytes as the lookup key `text`. Therefore it has identical
                     * equality and hash behavior.
                     */
                    entry.insert_with_key_unchecked(stored, name);
                }
                name
            }
        }
    }

    pub fn resolve(&self, name: K) -> Option<&str> {
        self.by_name.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

impl<K: Idx> Default for StringInterner<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Idx> Index<K> for StringInterner<K> {
    type Output = str;

    fn index(&self, name: K) -> &Self::Output {
        self.resolve(name).expect("name does not belong to this interner")
    }
}
