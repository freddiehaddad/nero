//! Translated from `src/nvim/map_defs.h`, `src/nvim/map_key_impl.c.h`,
//! `src/nvim/map_value_impl.c.h`, and the generic parts of `src/nvim/map.c`
//! ("Hash maps and sets" - parts derived from `khash.h`, part of klib, MIT
//! license).
//!
//! The original implements one hand-written open-addressing hash table
//! algorithm (`MapHash` + `mh_find_bucket`/`mh_get`/`mh_rehash`/`mh_put`/
//! `mh_delete`), then uses C preprocessor macros (`KEY_DECLS`/`MAP_DECLS`,
//! via `map_key_impl.c.h`/`map_value_impl.c.h` textually `#include`d once
//! per key/value type) to stamp out a separate copy of that same algorithm
//! for every `(K)`/`(K, V)` pair actually used (`int`, `cstr_t`, `path_t`,
//! `ptr_t`, `uint64_t`, `int64_t`, `uint32_t`, `String`, `HlEntry`,
//! `ColorKey`, ...). This is C's standard workaround for the lack of
//! generics. Rust has real generics, so this translation writes the
//! algorithm exactly once, as a generic `Set<K>`/`Map<K, V>` - the direct,
//! literal replacement for "the same code, copy-pasted per type via the
//! preprocessor", not a redesign (same reasoning as `garray.h`'s
//! `GA_APPEND` macro -> `GarrayT::ga_append_item<T>`).
//!
//! Per-type `hash_X`/`equal_X` functions become one `K: Hash + Eq` bound
//! (Rust's `std::hash::Hash` is the native equivalent of "a hash function
//! for this type" - the exact hash *values* differ from the original's
//! custom hash functions, which is fine: nothing outside this table ever
//! observes a hash value, only internally-consistent bucketing matters).
//! `n_keys`/`keys_capacity` bookkeeping is dropped: `Vec<K>` already
//! tracks its own length/capacity, so there is nothing left to
//! independently track.
//!
//! Deferred: `hash_path_t`/`equal_path_t` (Windows drive-letter/backslash
//! folding + case-insensitive comparison, needs `path_fnamencmp` -
//! `path.c`, not fully translated - and `mbyte.c`'s `str_foldcase`);
//! `pmap_del2` (needs the `cstr_t`/`ptr_t` ownership-freeing convention of
//! callers not yet translated).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// `MH_TOMBSTONE`
const MH_TOMBSTONE: u32 = u32::MAX;
/// `UPPER_FILL`
const UPPER_FILL: f64 = 0.77;

/// `roundup32(x)`: round up to the next power of two.
fn roundup32(mut x: u32) -> u32 {
    x -= 1;
    x |= x >> 1;
    x |= x >> 2;
    x |= x >> 4;
    x |= x >> 8;
    x |= x >> 16;
    x + 1
}

fn hash_of<K: Hash>(key: &K) -> u32 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish() as u32
}

/// A hash set with insertion-order-preserving compact storage
/// (`Set(T)`/`MH_DECLS`/`KEY_DECLS`).
///
/// NOTE (kept from the original): callers must manage memory for keys and
/// values themselves in the original C; here that's just normal Rust
/// ownership, so there's nothing extra to do.
pub struct Set<K> {
    /// sparse index array: `0` = empty, [`MH_TOMBSTONE`] = deleted slot,
    /// otherwise `keys` index + 1 (`MapHash.hash`)
    hash: Vec<u32>,
    /// live entries (`MapHash.size`)
    size: u32,
    /// live + tombstoned entries (`MapHash.n_occupied`)
    n_occupied: u32,
    /// resize trigger point (`MapHash.upper_bound`)
    upper_bound: u32,
    /// compact array of live keys, in insertion order except for
    /// swap-on-delete (`Set(T).keys`)
    keys: Vec<K>,
}

impl<K> Default for Set<K> {
    fn default() -> Self {
        Set {
            hash: Vec::new(),
            size: 0,
            n_occupied: 0,
            upper_bound: 0,
            keys: Vec::new(),
        }
    }
}

/// `MHPutStatus`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MhPutStatus {
    Existing,
    NewKeyDidFit,
    NewKeyRealloc,
}

impl<K: Hash + Eq + Clone> Set<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// `set_size(set)`
    #[inline]
    pub fn len(&self) -> usize {
        self.size as usize
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// `mh_realloc`: (re)allocate the sparse hash-index array with at
    /// least `n_min_buckets` buckets and rebuild it from `self.keys`.
    fn mh_realloc(&mut self, n_min_buckets: u32) {
        let n_buckets = roundup32(n_min_buckets.max(16));
        self.hash = vec![0u32; n_buckets as usize];
        self.n_occupied = 0;
        self.size = 0;
        self.upper_bound = (n_buckets as f64 * UPPER_FILL + 0.5) as u32;
    }

    /// `mh_clear`
    pub fn clear(&mut self) {
        if !self.hash.is_empty() {
            self.hash.fill(0);
            self.size = 0;
            self.n_occupied = 0;
        }
        self.keys.clear();
    }

    /// find bucket to get or put `key` (`mh_find_bucket`).
    ///
    /// Returns the bucket index, or [`MH_TOMBSTONE`] if not found and `put`
    /// was false. If found-or-empty: `self.hash[result]` is 0 (empty,
    /// `put` site) or the tombstone/live value as appropriate.
    fn mh_find_bucket(&self, key: &K, put: bool) -> u32 {
        let mask = self.hash.len() as u32 - 1;
        let k = hash_of(key);
        let mut i = k & mask;
        let last = i;
        let mut site = if put { last } else { MH_TOMBSTONE };
        let mut step: u32 = 0;
        while self.hash[i as usize] != 0 {
            if self.hash[i as usize] == MH_TOMBSTONE {
                if site == last {
                    site = i;
                }
            } else if &self.keys[(self.hash[i as usize] - 1) as usize] == key {
                return i;
            }
            step += 1;
            i = (i.wrapping_add(step)) & mask;
            assert!(i != last, "mh_find_bucket: hash table full during probe");
        }
        if site == last {
            site = i;
        }
        site
    }

    /// `mh_get`: returns the index into `self.keys()` if found.
    pub fn get_index(&self, key: &K) -> Option<usize> {
        if self.hash.is_empty() {
            return None;
        }
        let idx = self.mh_find_bucket(key, false);
        if idx != MH_TOMBSTONE {
            Some((self.hash[idx as usize] - 1) as usize)
        } else {
            None
        }
    }

    #[inline]
    pub fn contains(&self, key: &K) -> bool {
        self.get_index(key).is_some()
    }

    /// `mh_rehash`: rebuild `self.hash` from `self.keys` (which must
    /// already be allocated and empty before calling, per the original).
    fn mh_rehash(&mut self) {
        for k in 0..self.keys.len() {
            let idx = self.mh_find_bucket(&self.keys[k], true);
            assert!(
                self.hash[idx as usize] == 0,
                "mh_rehash: expected an empty slot (tombstones should exist during rehash)"
            );
            self.hash[idx as usize] = k as u32 + 1;
        }
        self.n_occupied = self.keys.len() as u32;
        self.size = self.keys.len() as u32;
    }

    /// Puts `key`. Returns `(index, status)`; `status` reveals whether an
    /// existing key was found (`mh_put`).
    pub fn put(&mut self, key: K) -> (usize, MhPutStatus) {
        if self.hash.is_empty() {
            self.mh_realloc(0);
        }
        if self.n_occupied >= self.upper_bound {
            // If we likely were to resize soon, do it now to avoid extra rehash.
            if self.size as f64 >= self.upper_bound as f64 * 0.9 {
                // TODO(bfredl in the original): we never shrink, but maybe that's fine.
                let n_buckets = self.hash.len() as u32;
                self.mh_realloc(n_buckets + 1);
            } else {
                // Just a lot of tombstones from deleted items, start all over again.
                self.hash.fill(0);
                self.size = 0;
                self.n_occupied = 0;
            }
            self.mh_rehash();
        }

        let idx = self.mh_find_bucket(&key, true);

        if self.hash[idx as usize] == 0 || self.hash[idx as usize] == MH_TOMBSTONE {
            self.size += 1;
            if self.hash[idx as usize] == 0 {
                self.n_occupied += 1;
            }
            let pos = self.keys.len();
            let had_capacity = self.keys.len() < self.keys.capacity();
            self.keys.push(key);
            self.hash[idx as usize] = pos as u32 + 1;
            let status = if had_capacity {
                MhPutStatus::NewKeyDidFit
            } else {
                MhPutStatus::NewKeyRealloc
            };
            (pos, status)
        } else {
            let pos = (self.hash[idx as usize] - 1) as usize;
            assert!(self.keys[pos] == key, "mh_put: bucket/key mismatch");
            (pos, MhPutStatus::Existing)
        }
    }

    /// Deletes `key` if found (`mh_delete`).
    ///
    /// Returns the removed key (which may differ in identity, though not
    /// in equality, from the searched-for `key` - matching the original's
    /// `*key = set->keys[k]` semantics) and its old index, or `None` if not
    /// found.
    pub fn delete(&mut self, key: &K) -> Option<(K, usize)> {
        if self.size == 0 {
            return None;
        }
        let idx = self.mh_find_bucket(key, false);
        if idx == MH_TOMBSTONE {
            return None;
        }
        let k = (self.hash[idx as usize] - 1) as usize;
        self.hash[idx as usize] = MH_TOMBSTONE;

        let last = self.keys.len() - 1;
        self.size -= 1;
        let removed_key = if last != k {
            let idx2 = self.mh_find_bucket(&self.keys[last], false);
            assert!(
                self.hash[idx2 as usize] == last as u32 + 1,
                "mh_delete: inconsistent hash index for the swapped-in key"
            );
            self.hash[idx2 as usize] = k as u32 + 1;
            self.keys.swap(k, last);
            self.keys.pop().unwrap()
        } else {
            self.keys.pop().unwrap()
        };
        Some((removed_key, k))
    }

    /// Direct read access to the compact key array (e.g. for iteration,
    /// matching `set_foreach`/`map_foreach_key`).
    #[inline]
    pub fn keys(&self) -> &[K] {
        &self.keys
    }
}

/// A hash map layered on [`Set`] (`Map(T, U)`/`MAP_DECLS`): an array of
/// values parallel to the set's compact key array.
pub struct Map<K, V> {
    set: Set<K>,
    values: Vec<V>,
}

impl<K, V> Default for Map<K, V> {
    fn default() -> Self {
        Map {
            set: Set::default(),
            values: Vec::new(),
        }
    }
}

impl<K: Hash + Eq + Clone, V> Map<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.set.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// `map_get(T, U)`. Note: the original returns `value_init_##U` (a
    /// per-type default) when not found; here that's spelled with
    /// `V: Default` at the call site via [`Map::get_or_default`], while
    /// this method follows normal Rust convention and returns `Option<&V>`.
    #[inline]
    pub fn get(&self, key: &K) -> Option<&V> {
        self.set.get_index(key).map(|i| &self.values[i])
    }

    #[inline]
    pub fn contains_key(&self, key: &K) -> bool {
        self.set.contains(key)
    }

    /// `map_put(T, U)`: insert or overwrite the value for `key`.
    pub fn insert(&mut self, key: K, value: V) {
        let (pos, status) = self.set.put(key);
        match status {
            MhPutStatus::Existing => {
                self.values[pos] = value;
            }
            MhPutStatus::NewKeyDidFit | MhPutStatus::NewKeyRealloc => {
                debug_assert_eq!(pos, self.values.len());
                self.values.push(value);
            }
        }
    }

    /// `map_del(T, U)`: remove `key`, returning its value if present.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let (_removed_key, idx) = self.set.delete(key)?;
        let last = self.values.len() - 1;
        if idx != last {
            self.values.swap(idx, last);
        }
        self.values.pop()
    }

    /// Iterates `(key, value)` pairs (`map_foreach`).
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.set.keys.iter().zip(self.values.iter())
    }
}

impl<K: Hash + Eq + Clone, V: Default + Clone> Map<K, V> {
    /// `map_get(T, U)`'s exact original semantics: a per-type default
    /// value when the key isn't present, instead of `Option`.
    pub fn get_or_default(&self, key: &K) -> V {
        self.get(key).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_put_get_delete_roundtrip() {
        let mut s: Set<i32> = Set::new();
        let (pos1, status1) = s.put(10);
        assert_eq!(pos1, 0);
        // The very first insertion always reallocates in the original too:
        // keys_capacity starts at 0, so `pos(0) >= keys_capacity(0)` is true
        // even for the first key - matched here since an empty Vec's
        // capacity is likewise 0 before its first push.
        assert_eq!(status1, MhPutStatus::NewKeyRealloc);
        assert!(s.contains(&10));
        assert!(!s.contains(&20));

        let (_pos2, status2) = s.put(10); // duplicate
        assert_eq!(status2, MhPutStatus::Existing);
        assert_eq!(s.len(), 1);

        let removed = s.delete(&10);
        assert!(removed.is_some());
        assert!(!s.contains(&10));
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn set_handles_many_insertions_and_resizes() {
        let mut s: Set<i32> = Set::new();
        for i in 0..500 {
            s.put(i);
        }
        assert_eq!(s.len(), 500);
        for i in 0..500 {
            assert!(s.contains(&i), "missing {i}");
        }
        for i in 0..250 {
            s.delete(&i);
        }
        assert_eq!(s.len(), 250);
        for i in 0..250 {
            assert!(!s.contains(&i));
        }
        for i in 250..500 {
            assert!(s.contains(&i));
        }
    }

    #[test]
    fn set_delete_and_reinsert_stays_consistent() {
        let mut s: Set<i32> = Set::new();
        for i in 0..50 {
            s.put(i);
        }
        for i in (0..50).step_by(2) {
            s.delete(&i);
        }
        for i in 100..150 {
            s.put(i);
        }
        for i in (1..50).step_by(2) {
            assert!(s.contains(&i), "should still have odd {i}");
        }
        for i in (0..50).step_by(2) {
            assert!(!s.contains(&i), "should have deleted even {i}");
        }
        for i in 100..150 {
            assert!(s.contains(&i), "should have new {i}");
        }
    }

    #[test]
    fn map_insert_get_remove() {
        let mut m: Map<String, i32> = Map::new();
        m.insert("a".to_string(), 1);
        m.insert("b".to_string(), 2);
        assert_eq!(m.get(&"a".to_string()), Some(&1));
        assert_eq!(m.get(&"b".to_string()), Some(&2));
        assert_eq!(m.get(&"c".to_string()), None);
        assert_eq!(m.len(), 2);

        m.insert("a".to_string(), 100); // overwrite
        assert_eq!(m.get(&"a".to_string()), Some(&100));
        assert_eq!(m.len(), 2);

        assert_eq!(m.remove(&"a".to_string()), Some(100));
        assert_eq!(m.get(&"a".to_string()), None);
        assert_eq!(m.get(&"b".to_string()), Some(&2)); // unaffected by removal of "a"
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn map_get_or_default_matches_value_init_semantics() {
        let m: Map<i32, i32> = Map::new();
        assert_eq!(m.get_or_default(&5), 0); // value_init_int == 0
    }

    #[test]
    fn map_iter_covers_all_pairs() {
        let mut m: Map<i32, i32> = Map::new();
        for i in 0..20 {
            m.insert(i, i * 10);
        }
        let mut seen: Vec<(i32, i32)> = m.iter().map(|(k, v)| (*k, *v)).collect();
        seen.sort();
        let expected: Vec<(i32, i32)> = (0..20).map(|i| (i, i * 10)).collect();
        assert_eq!(seen, expected);
    }
}
