//! Translated from `src/nvim/hashtab.c` ("Handling of a hashtable with
//! Vim-specific properties").
//!
//! Each item in a hashtable has a NUL terminated string key. A key can
//! appear only once in the table.
//!
//! A hash number is computed from the key for quick lookup. When the hashes
//! of two different keys point to the same entry an algorithm is used to
//! iterate over other entries in the table until the right one is found. To
//! make the iteration work removed keys are different from entries where a
//! key was never present.
//!
//! The mechanism has been partly based on how Python Dictionaries are
//! implemented. The algorithm is from Knuth Vol. 3, Sec. 6.4.
//!
//! The hashtable grows to accommodate more entries when needed. At least 1/3
//! of the entries is empty to keep the lookup efficient (at the cost of
//! extra memory).
//!
//! Deferred:
//! - `hash_clear_all` (frees every value via `hi_key - off`, i.e. "the key
//!   is embedded inside its value at a fixed offset" pointer arithmetic):
//!   needs a concrete "value that embeds a key" type from a real caller
//!   before the right Rust shape (probably a trait or closure parameter
//!   instead of raw offset arithmetic) can be chosen; not yet started.
//! - `hash_debug_results`/`HT_DEBUG` counters: a `#ifdef HT_DEBUG`-only
//!   diagnostic tool, not compiled in normal (non-debug-instrumented)
//!   builds.
//! - The duplicate-key error message in `hash_add` (`siemsg(...)`): needs
//!   `message.c` (phase 15); `hash_add` still returns `FAIL` correctly, it
//!   just doesn't print anything yet.
//!
//! `hash_find`/`hash_find_len` collapse into a single function here: both
//! exist in the original only because C string keys aren't
//! length-prefixed, so a NUL-terminated-vs-explicit-length distinction is
//! needed; a Rust `&[u8]` slice always carries its own length, so there is
//! nothing left to distinguish.

use crate::ascii_defs::NUL;
use crate::hashtab_defs::{HashArray, HashT, HashitemT, HashtabT, HT_INIT_SIZE};
use crate::vim_defs::{FAIL, OK};

// Magic value for algorithm that walks through the array.
const PERTURB_SHIFT: u32 = 5;

/// `hash_removed`: the pointee of the `HI_KEY_REMOVED` sentinel. Never
/// actually read or written - only its *address* matters, as a pointer
/// value guaranteed distinct from any real key pointer and from null,
/// exactly like the original's `char hash_removed;` global +
/// `#define HI_KEY_REMOVED (&hash_removed)`.
static HASH_REMOVED: u8 = 0;

/// `HI_KEY_REMOVED`
#[inline]
pub fn hi_key_removed() -> *mut std::os::raw::c_char {
    std::ptr::addr_of!(HASH_REMOVED) as *mut std::os::raw::c_char
}

/// `HASHITEM_EMPTY(hi)`
#[inline]
pub fn hashitem_empty(hi: &HashitemT) -> bool {
    hi.hi_key.is_null() || hi.hi_key == hi_key_removed()
}

/// Function to get the `HI_KEY_REMOVED` value (`_hash_key_removed`).
///
/// Used for testing because luajit ffi does not allow getting addresses of
/// globals.
#[inline]
pub fn _hash_key_removed() -> *mut std::os::raw::c_char {
    hi_key_removed()
}

/// Reads the NUL-terminated bytes at a raw `hi_key` pointer, for comparing
/// against a candidate key. `hi_key` must be non-null and not
/// [`hi_key_removed`] (checked by every caller below via [`hashitem_empty`]
/// first).
///
/// # Safety
/// `hi_key` must point to a valid NUL-terminated byte string, live for the
/// duration of the call - exactly the same contract the original places on
/// every `hi_key` a caller stores via [`hash_add_item`].
#[inline]
unsafe fn hi_key_bytes<'a>(hi_key: *const std::os::raw::c_char) -> &'a [u8] {
    std::ffi::CStr::from_ptr(hi_key).to_bytes()
}

impl HashtabT {
    /// Initialize an empty hash table (`hash_init`).
    pub fn hash_init() -> HashtabT {
        HashtabT {
            ht_mask: HT_INIT_SIZE as HashT - 1,
            ht_used: 0,
            ht_filled: 0,
            ht_changed: 0,
            ht_locked: 0,
            ht_array: HashArray::Small([HashitemT::default(); HT_INIT_SIZE]),
        }
    }

    /// Find item for given `key` in the hashtable (`hash_find`/`hash_find_len`
    /// - see module docs for why they're unified).
    ///
    /// Returns the hash item corresponding to the given key. If not found,
    /// returns the empty item that would be used for that key.
    ///
    /// WARNING: the returned reference becomes invalid as soon as the hash
    /// table is changed in any way (matches the original's warning).
    pub fn hash_find(&mut self, key: &[u8]) -> &mut HashitemT {
        let hash = hash_hash_len(key);
        self.hash_lookup(key, hash)
    }

    /// Like [`HashtabT::hash_find`], but the caller computes `hash`
    /// (`hash_lookup`).
    pub fn hash_lookup(&mut self, key: &[u8], hash: HashT) -> &mut HashitemT {
        let idx = self.hash_lookup_idx(key, hash);
        &mut self.ht_array.as_mut_slice()[idx]
    }

    /// Core of `hash_lookup`, returning just the index so callers that also
    /// need to mutate other `self` fields (`hash_add_item`, `hash_remove`)
    /// aren't stuck holding a `&mut HashitemT` borrow of `self` the whole
    /// time (a purely mechanical Rust borrow-checker accommodation - the
    /// original C, with no borrow checker, doesn't need this split).
    fn hash_lookup_idx(&self, key: &[u8], hash: HashT) -> usize {
        // Quickly handle the most common situations:
        // - return if there is no item at all
        // - skip over a removed item
        // - return if the item matches
        let mask = self.ht_mask;
        // `raw_idx` is the *unmasked* recurrence variable, matching the
        // original's `idx = 5 * idx + perturb + 1` (which keeps growing
        // and wrapping at the integer width - masking only happens at the
        // point of indexing, `ht_array[idx & ht_mask]`). Masking `raw_idx`
        // itself on every iteration (instead of only when indexing) would
        // shrink the effective probe sequence and could cycle forever
        // without ever finding a null slot - caught by a hanging test.
        let mut raw_idx = hash & mask;
        let array = self.ht_array.as_slice();
        let hi = &array[raw_idx];
        if hi.hi_key.is_null() {
            return raw_idx;
        }
        if hi.hi_key != hi_key_removed() && hi.hi_hash == hash && unsafe { hi_key_bytes(hi.hi_key) } == key {
            return raw_idx;
        }

        let mut freeitem: Option<usize> = if hi.hi_key == hi_key_removed() {
            Some(raw_idx)
        } else {
            None
        };

        // Need to search through the table to find the key. The algorithm
        // to step through the table starts with large steps, gradually
        // becoming smaller down to (1/4 table size + 1). This means it goes
        // through all table entries in the end. When we run into a null
        // key it's clear that the key isn't there. Return the first
        // available slot found (can be a slot of a removed item).
        //
        // Safety net not present in the original: mathematically, this
        // probe sequence visits every slot within `mask + 1` iterations, so
        // never finding a null slot within that many steps can only mean
        // the table is completely full (e.g. items were added to a locked
        // table past its capacity, which `hash_lock`'s own doc comment
        // warns against) or the recurrence has a bug. Either way, panicking
        // loudly here is preferable to an infinite loop.
        let mut perturb = hash;
        for _ in 0..=mask {
            raw_idx = raw_idx.wrapping_mul(5).wrapping_add(perturb).wrapping_add(1);
            perturb >>= PERTURB_SHIFT;
            let idx = raw_idx & mask;

            let hi = &array[idx];
            if hi.hi_key.is_null() {
                return freeitem.unwrap_or(idx);
            }

            if hi.hi_hash == hash && hi.hi_key != hi_key_removed() && unsafe { hi_key_bytes(hi.hi_key) } == key {
                return idx;
            }

            if hi.hi_key == hi_key_removed() && freeitem.is_none() {
                freeitem = Some(idx);
            }
        }
        panic!(
            "hashtab probe exceeded table size ({} slots) without finding a free slot - \
             table is completely full, likely from adding items to a locked table past its \
             capacity (see hash_lock's doc comment)",
            mask + 1
        );
    }

    /// Add (empty) item for key `key` to the hashtable (`hash_add`).
    ///
    /// Returns `OK` on success, `FAIL` if the key is already present.
    ///
    /// # Safety
    /// `key` must point to a valid NUL-terminated byte string, and must be
    /// contained in the (externally owned) value the caller will store: it
    /// must outlive the hashtable entry, matching the original's
    /// raw-pointer contract.
    pub unsafe fn hash_add(&mut self, key: *mut std::os::raw::c_char) -> i32 {
        let key_bytes = hi_key_bytes(key);
        let hash = hash_hash_len(key_bytes);
        let idx = self.hash_lookup_idx(key_bytes, hash);
        if !hashitem_empty(&self.ht_array.as_slice()[idx]) {
            // siemsg(_("E685: Internal error: hash_add(): duplicate key \"%s\""), key) - deferred, see module docs.
            return FAIL;
        }
        self.hash_add_item(key, hash);
        OK
    }

    /// Add item for key `key` (already looked up via [`HashtabT::hash_lookup`]
    /// on an empty slot) to the hashtable (`hash_add_item`).
    ///
    /// # Safety
    /// Same contract as [`HashtabT::hash_add`].
    pub unsafe fn hash_add_item(&mut self, key: *mut std::os::raw::c_char, hash: HashT) {
        let key_bytes = hi_key_bytes(key);
        let idx = self.hash_lookup_idx(key_bytes, hash);
        self.ht_used += 1;
        self.ht_changed += 1;
        if self.ht_array.as_slice()[idx].hi_key.is_null() {
            self.ht_filled += 1;
        }
        let hi = &mut self.ht_array.as_mut_slice()[idx];
        hi.hi_key = key;
        hi.hi_hash = hash;

        // When the space gets low may resize the array.
        self.hash_may_resize(0);
    }

    /// Remove the item found at `key` from the hashtable (`hash_remove`).
    ///
    /// Caller must take care of freeing the item itself.
    pub fn hash_remove(&mut self, key: &[u8]) {
        let hash = hash_hash_len(key);
        let idx = self.hash_lookup_idx(key, hash);
        self.ht_used -= 1;
        self.ht_changed += 1;
        self.ht_array.as_mut_slice()[idx].hi_key = hi_key_removed();
        self.hash_may_resize(0);
    }

    /// Lock hashtable (prevent changes in the array) (`hash_lock`).
    ///
    /// Don't use this when items are to be added! Must call
    /// [`HashtabT::hash_unlock`] later.
    #[inline]
    pub fn hash_lock(&mut self) {
        self.ht_locked += 1;
    }

    /// Unlock hashtable (allow changes in the array again) (`hash_unlock`).
    ///
    /// Table will be resized (shrunk) when necessary. This must balance a
    /// call to [`HashtabT::hash_lock`].
    pub fn hash_unlock(&mut self) {
        self.ht_locked -= 1;
        self.hash_may_resize(0);
    }

    /// Resize hashtable (new size can be given or automatically computed)
    /// (`hash_may_resize`).
    ///
    /// * `minitems` - Minimum number of items the new table should hold. If
    ///   zero, new size will depend on currently used items: shrink when too
    ///   much empty space, grow when not enough empty space. If non-zero,
    ///   the passed `minitems` will be used.
    fn hash_may_resize(&mut self, mut minitems: usize) {
        // Don't resize a locked table.
        if self.ht_locked > 0 {
            return;
        }

        let minsize: usize;
        let oldsize = self.ht_mask + 1;
        if minitems == 0 {
            // Return quickly for small tables with at least two null items.
            // Items are required for the lookup to decide a key isn't there.
            if self.ht_filled < HT_INIT_SIZE - 1 && self.ht_array.is_small() {
                return;
            }

            // Grow or refill the array when it's more than 2/3 full
            // (including removed items, so that they get cleaned up).
            // Shrink the array when it's less than 1/5 full. When growing it
            // is at least 1/4 full (avoids repeated grow-shrink operations).
            if self.ht_filled * 3 < oldsize * 2 && self.ht_used > oldsize / 5 {
                return;
            }

            minsize = if self.ht_used > 1000 {
                // it's big, don't make too much room
                self.ht_used * 2
            } else {
                // make plenty of room
                self.ht_used * 4
            };
        } else {
            // Use specified size.
            minitems = minitems.max(self.ht_used);
            // array is up to 2/3 full
            minsize = (minitems * 3).div_ceil(2);
        }

        let mut newsize = HT_INIT_SIZE;
        while newsize < minsize {
            // make sure it's always a power of 2
            newsize <<= 1;
            assert!(newsize != 0, "hashtab new size overflowed");
        }

        let newarray_is_small = newsize == HT_INIT_SIZE;

        if !newarray_is_small && newsize == oldsize && self.ht_filled * 3 < oldsize * 2 {
            // The hashtab is already at the desired size, and there are not
            // too many removed items, bail out.
            return;
        }

        // Unlike the original (which must copy ht_smallarray into a
        // temporary buffer first to avoid the old/new arrays aliasing when
        // both are the small array), Rust's ownership model already gives
        // us a private, exclusively-owned `old_array` by moving it out - no
        // aliasing is possible.
        let old_array = std::mem::replace(
            &mut self.ht_array,
            HashArray::Small([HashitemT::default(); HT_INIT_SIZE]),
        );

        let mut new_array = if newarray_is_small {
            HashArray::Small([HashitemT::default(); HT_INIT_SIZE])
        } else {
            HashArray::Large(vec![HashitemT::default(); newsize])
        };

        // Move all the items from the old array to the new one, placing
        // them in the right spot. The new array won't have any removed
        // items, so this is also a cleanup action.
        let newmask = newsize as HashT - 1;
        let mut todo = self.ht_used;

        for olditem in old_array.as_slice() {
            if todo == 0 {
                break;
            }
            if hashitem_empty(olditem) {
                continue;
            }
            // The algorithm to find the spot to add the item is identical
            // to the algorithm in hash_lookup(). But we only need to search
            // for a null key, so it's simpler.
            let mut new_raw_idx = olditem.hi_hash & newmask;
            let mut newi = new_raw_idx;
            if !new_array.as_slice()[newi].hi_key.is_null() {
                let mut perturb = olditem.hi_hash;
                let mut found = false;
                for _ in 0..=newmask {
                    new_raw_idx = new_raw_idx.wrapping_mul(5).wrapping_add(perturb).wrapping_add(1);
                    perturb >>= PERTURB_SHIFT;
                    newi = new_raw_idx & newmask;
                    if new_array.as_slice()[newi].hi_key.is_null() {
                        found = true;
                        break;
                    }
                }
                assert!(
                    found,
                    "hashtab resize probe exceeded new table size ({} slots) without finding \
                     a free slot - the new size was computed too small for ht_used items",
                    newmask + 1
                );
            }
            new_array.as_mut_slice()[newi] = *olditem;
            todo -= 1;
        }

        self.ht_array = new_array;
        self.ht_mask = newmask;
        self.ht_filled = self.ht_used;
        self.ht_changed += 1;
    }
}

/// Get the hash number for a key (`hash_hash`/`hash_hash_len` - unified for
/// the same reason as `hash_find`/`hash_find_len`, see module docs).
///
/// If you think you know a better hash function: run a script that uses
/// hashtables a lot with both algorithms and compare. This is a simplistic
/// algorithm that appears to do very well, suggested by George Reilly.
pub fn hash_hash_len(key: &[u8]) -> HashT {
    if key.is_empty() {
        return 0;
    }
    let mut hash: HashT = key[0] as HashT;
    if hash == 0 {
        return 0;
    }
    for &b in &key[1..] {
        if b == NUL {
            break;
        }
        hash = hash.wrapping_mul(101).wrapping_add(b as HashT);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_ptr(s: &std::ffi::CString) -> *mut std::os::raw::c_char {
        s.as_ptr() as *mut std::os::raw::c_char
    }

    #[test]
    fn hash_hash_len_matches_known_values() {
        assert_eq!(hash_hash_len(b""), 0);
        // Single byte: hash == first byte, no cycle needed.
        assert_eq!(hash_hash_len(b"a"), b'a' as HashT);
    }

    #[test]
    fn init_creates_empty_small_table() {
        let ht = HashtabT::hash_init();
        assert_eq!(ht.ht_used, 0);
        assert_eq!(ht.ht_filled, 0);
        assert_eq!(ht.ht_mask, HT_INIT_SIZE as HashT - 1);
        assert!(ht.ht_array.is_small());
    }

    #[test]
    fn add_find_remove_roundtrip() {
        let mut ht = HashtabT::hash_init();
        let k1 = std::ffi::CString::new("foo").unwrap();
        let k2 = std::ffi::CString::new("bar").unwrap();

        assert_eq!(unsafe { ht.hash_add(key_ptr(&k1)) }, OK);
        assert_eq!(unsafe { ht.hash_add(key_ptr(&k2)) }, OK);
        assert_eq!(ht.ht_used, 2);

        // Duplicate insert fails.
        assert_eq!(unsafe { ht.hash_add(key_ptr(&k1)) }, FAIL);

        let found = ht.hash_find(b"foo");
        assert!(!hashitem_empty(found));
        assert_eq!(unsafe { hi_key_bytes(found.hi_key) }, b"foo");

        let missing = ht.hash_find(b"nope");
        assert!(hashitem_empty(missing));

        ht.hash_remove(b"foo");
        assert_eq!(ht.ht_used, 1);
        assert!(hashitem_empty(ht.hash_find(b"foo")));
        assert!(!hashitem_empty(ht.hash_find(b"bar")));
    }

    #[test]
    fn resizes_when_growing_past_small_array() {
        let mut ht = HashtabT::hash_init();
        // Keep owned CStrings alive for the whole test.
        let keys: Vec<std::ffi::CString> = (0..50)
            .map(|i| std::ffi::CString::new(format!("key{i}")).unwrap())
            .collect();
        for k in &keys {
            assert_eq!(unsafe { ht.hash_add(key_ptr(k)) }, OK);
        }
        assert_eq!(ht.ht_used, 50);
        assert!(!ht.ht_array.is_small());
        for k in &keys {
            let bytes = k.as_bytes();
            assert!(!hashitem_empty(ht.hash_find(bytes)), "missing {bytes:?}");
        }
    }

    #[test]
    fn hash_lock_prevents_resize() {
        // NOTE: hash_lock()'s contract explicitly warns "Don't use this
        // when items are to be added!" - a locked table never resizes, so
        // inserting more items than fit in the current array has no null
        // slot left to terminate probing (an infinite loop, matching a real
        // hazard in the original C too, not a translation bug). This test
        // stays safely within the 16-slot small array's capacity while
        // locked, then unlocks and grows further to confirm resize resumes.
        let mut ht = HashtabT::hash_init();
        ht.hash_lock();
        let keys: Vec<std::ffi::CString> = (0..10)
            .map(|i| std::ffi::CString::new(format!("k{i}")).unwrap())
            .collect();
        for k in &keys {
            assert_eq!(unsafe { ht.hash_add(key_ptr(k)) }, OK);
        }
        // Still using the small array's mask: resize was skipped while locked.
        assert_eq!(ht.ht_mask, HT_INIT_SIZE as HashT - 1);
        assert!(ht.ht_array.is_small());

        ht.hash_unlock();
        let more_keys: Vec<std::ffi::CString> = (10..50)
            .map(|i| std::ffi::CString::new(format!("k{i}")).unwrap())
            .collect();
        for k in &more_keys {
            assert_eq!(unsafe { ht.hash_add(key_ptr(k)) }, OK);
        }
        assert!(!ht.ht_array.is_small());
    }

    #[test]
    #[should_panic(expected = "hashtab probe exceeded table size")]
    fn locking_past_capacity_panics_loudly_instead_of_hanging() {
        // Regression test: an earlier draft of hash_lookup_idx's probe
        // recurrence masked the running index on every iteration instead
        // of only when indexing into the array, which collapsed the probe
        // sequence and caused a genuine infinite loop here. The bounded
        // loop + panic added afterwards turns "filled a locked table past
        // capacity" (a real hazard per hash_lock's own doc comment) into a
        // fast, loud failure instead of a hang.
        let mut ht = HashtabT::hash_init();
        ht.hash_lock();
        // HT_INIT_SIZE items exactly fill all 16 slots (the last insertion
        // still finds the one remaining empty slot); one more than that has
        // nowhere left to go and must exhaust the bounded probe loop.
        let keys: Vec<std::ffi::CString> = (0..=HT_INIT_SIZE)
            .map(|i| std::ffi::CString::new(format!("k{i}")).unwrap())
            .collect();
        for k in &keys {
            unsafe { ht.hash_add(key_ptr(k)) };
        }
    }
}
