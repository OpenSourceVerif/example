use crate::Vec;
use bumpalo::Bump;
use index_vec::Idx;
use std::{collections::HashMap, ops::Index};

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
        if let Some(&name) = self.by_text.get(text) {
            return name;
        }

        let index =
            u32::try_from(self.by_name.len()).expect("string interner exhausted all u32 names");

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

        let name = Idx::from_usize(index as usize);

        self.by_name.push(stored);
        let previous = self.by_text.insert(stored, name);
        debug_assert!(previous.is_none());

        name
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
