use std::collections::BTreeMap;

// A simple index structure that maintains a sorted map with duplicates.
// Allows average case of approximately O(log n) (assuming few duplicates in matched IndexKey),
// and O(1) get for minimum value (for iteration).
#[derive(Debug)]
pub struct MemoryIndex<
    IndexKey: PartialOrd + Ord + std::fmt::Debug,
    ArenaKey: PartialOrd + Ord + std::fmt::Debug,
>(BTreeMap<IndexKey, Vec<ArenaKey>>);
impl<
        IndexKey: PartialOrd + Ord + std::fmt::Debug,
        ArenaKey: PartialOrd + Ord + std::fmt::Debug,
    > Default for MemoryIndex<IndexKey, ArenaKey>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<
        IndexKey: PartialOrd + Ord + std::fmt::Debug,
        ArenaKey: PartialOrd + Ord + std::fmt::Debug,
    > MemoryIndex<IndexKey, ArenaKey>
{
    pub fn new() -> MemoryIndex<IndexKey, ArenaKey> {
        Self(BTreeMap::new())
    }

    pub fn remove(&mut self, arena_key: &ArenaKey, val: IndexKey) {
        if let Some(entry) = self.0.get_mut(&val) {
            entry.retain(|x| x != arena_key);
        }
    }

    pub fn add(&mut self, arena_key: ArenaKey, val: IndexKey) {
        let entry = self.0.entry(val).or_default();
        entry.push(arena_key);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&IndexKey, &ArenaKey)> {
        #[allow(clippy::borrow_deref_ref)]
        self.0.iter().flat_map(|x| x.1.iter().map(|y| (&*x.0, y)))
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}
