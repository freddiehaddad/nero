//! Translated from `src/nvim/garray.c` ("Functions for handling growing
//! arrays") and the macros in `src/nvim/garray.h`.
//!
//! Deferred (real forward dependencies, and a design mismatch - not faked):
//! - `ga_concat_strings`, `ga_clear_strings`, `ga_remove_duplicate_strings`,
//!   `GA_DEEP_CLEAR`/`GA_DEEP_CLEAR_PTR`: these all treat `ga_data` as an
//!   array of *string pointers* (`char **`/`const char **`), a completely
//!   different usage of `garray_T` than "one flat byte buffer" (which is
//!   what the rest of this file's functions treat it as, and what this
//!   translation's `GarrayT.ga_data: Vec<u8>` models directly). When a
//!   caller that uses `garray_T` as a string list is translated, it should
//!   become a plain `Vec<Vec<u8>>` directly rather than routing through
//!   `GarrayT` - so these functions aren't given a home here. Also,
//!   `ga_remove_duplicate_strings` needs `sort_strings` (`strings.c`) and
//!   `path_fnamecmp` (`path.c`, phase 8), neither translated yet.
//! - `WLOG(...)` calls (`log.c`, this phase but not yet translated): the
//!   surrounding logic is translated in full; only the warning-log side
//!   effect itself is a deferred no-op, clearly marked below.

use crate::garray_defs::GarrayT;

impl GarrayT {
    /// Clear an allocated growing array (`ga_clear`).
    #[inline]
    pub fn ga_clear(&mut self) {
        // Initialize growing array without resetting itemsize or growsize.
        self.ga_data = Vec::new();
        self.ga_maxlen = 0;
        self.ga_len = 0;
    }

    /// Initialize a growing array (`ga_init`).
    #[inline]
    pub fn ga_init(&mut self, itemsize: i32, growsize: i32) {
        self.ga_data = Vec::new();
        self.ga_maxlen = 0;
        self.ga_len = 0;
        self.ga_itemsize = itemsize;
        self.ga_set_growsize(growsize);
    }

    /// A setter for the growsize that guarantees it will be at least 1
    /// (`ga_set_growsize`).
    #[inline]
    pub fn ga_set_growsize(&mut self, growsize: i32) {
        if growsize < 1 {
            // WLOG("trying to set an invalid ga_growsize: %d", growsize) - deferred, see module docs.
            self.ga_growsize = 1;
        } else {
            self.ga_growsize = growsize;
        }
    }

    /// Make room in the growing array for at least `n` items (`ga_grow`).
    pub fn ga_grow(&mut self, n: i32) {
        if self.ga_maxlen - self.ga_len >= n {
            // the garray still has enough space, do nothing
            return;
        }

        if self.ga_growsize < 1 {
            // WLOG("ga_growsize(%d) is less than 1", gap->ga_growsize) - deferred, see module docs.
        }

        // the garray grows by at least growsize
        let mut n = n.max(self.ga_growsize);

        // A linear growth is very inefficient when the array grows big. This
        // is a compromise between allocating memory that won't be used and
        // too many copy operations. A factor of 1.5 seems reasonable.
        n = n.max(self.ga_len / 2);

        let new_maxlen = self.ga_len + n;
        let new_size = self.ga_itemsize as usize * new_maxlen as usize;

        // reallocate and clear the new memory (Vec::resize does both the
        // realloc and the memset(pp + old_size, 0, new_size - old_size) in
        // one safe call).
        self.ga_data.resize(new_size, 0);

        self.ga_maxlen = new_maxlen;
    }

    /// Append one byte to a growarray which contains bytes (`ga_append`).
    #[inline]
    pub fn ga_append(&mut self, c: u8) {
        self.ga_grow(1);
        let idx = self.ga_len as usize * self.ga_itemsize as usize;
        self.ga_data[idx] = c;
        self.ga_len += 1;
    }

    /// `GA_APPEND(item_type, gap, item)` (from `src/nvim/garray.h`): append
    /// a single item of any `Copy` type to the array.
    ///
    /// # Safety
    /// The caller must ensure `self.ga_itemsize == size_of::<T>()` (set via
    /// [`GarrayT::new`]) - like the original macro, this is not checked.
    #[inline]
    pub unsafe fn ga_append_item<T: Copy>(&mut self, item: T) {
        self.ga_grow(1);
        let idx = self.ga_len as usize * std::mem::size_of::<T>();
        let ptr = self.ga_data.as_mut_ptr().add(idx) as *mut T;
        ptr.write(item);
        self.ga_len += 1;
    }

    /// Reserves room for one more item and returns a pointer to it, without
    /// initializing it (`ga_append_via_ptr`).
    ///
    /// # Safety
    /// The returned pointer is valid for exactly `self.ga_itemsize` bytes;
    /// the caller must fully initialize it (matches the original's
    /// contract, which hands back a raw, uninitialized slot).
    pub unsafe fn ga_append_via_ptr(&mut self, item_size: usize) -> *mut u8 {
        if item_size as i32 != self.ga_itemsize {
            // WLOG("wrong item size (%zu), should be %d", ...) - deferred, see module docs.
        }
        self.ga_grow(1);
        let idx = self.ga_len as usize * self.ga_itemsize as usize;
        self.ga_len += 1;
        self.ga_data.as_mut_ptr().add(idx)
    }

    /// Concatenate a string (as a byte slice) to a growarray which contains
    /// bytes (`ga_concat_len`).
    ///
    /// WARNING (kept from the original): the parameter may not overlap with
    /// the growing array.
    pub fn ga_concat_len(&mut self, s: &[u8]) {
        if s.is_empty() {
            return;
        }
        self.ga_grow(s.len() as i32);
        let start = self.ga_len as usize;
        self.ga_data[start..start + s.len()].copy_from_slice(s);
        self.ga_len += s.len() as i32;
    }

    /// Concatenate a string to a growarray which contains characters
    /// (`ga_concat`). When `s` is `None` does not do anything.
    #[inline]
    pub fn ga_concat(&mut self, s: Option<&[u8]>) {
        if let Some(s) = s {
            self.ga_concat_len(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ga_grow_follows_growth_formula() {
        let mut ga = GarrayT::new(1, 4);
        ga.ga_grow(1);
        // n = max(1, growsize=4) = 4; n = max(4, len(0)/2=0) = 4; new_maxlen = 0+4 = 4
        assert_eq!(ga.ga_maxlen, 4);
        assert_eq!(ga.ga_data.len(), 4);

        ga.ga_len = 4; // pretend the array is full
        ga.ga_grow(1);
        // n = max(1, 4) = 4; n = max(4, len(4)/2=2) = 4; new_maxlen = 4+4=8
        assert_eq!(ga.ga_maxlen, 8);
    }

    #[test]
    fn ga_grow_noop_when_room_available() {
        let mut ga = GarrayT::new(1, 4);
        ga.ga_grow(4);
        let maxlen_before = ga.ga_maxlen;
        ga.ga_len = 1;
        ga.ga_grow(2); // maxlen(4) - len(1) = 3 >= 2, no growth needed
        assert_eq!(ga.ga_maxlen, maxlen_before);
    }

    #[test]
    fn ga_append_appends_bytes_in_order() {
        let mut ga = GarrayT::new(1, 4);
        ga.ga_append(b'a');
        ga.ga_append(b'b');
        ga.ga_append(b'c');
        assert_eq!(ga.ga_len, 3);
        assert_eq!(&ga.ga_data[..3], b"abc");
    }

    #[test]
    fn ga_append_item_generic_over_type() {
        let mut ga = GarrayT::new(std::mem::size_of::<i32>() as i32, 2);
        unsafe {
            ga.ga_append_item::<i32>(10);
            ga.ga_append_item::<i32>(20);
        }
        assert_eq!(ga.ga_len, 2);
        let ptr = ga.ga_data.as_ptr() as *const i32;
        unsafe {
            assert_eq!(*ptr, 10);
            assert_eq!(*ptr.add(1), 20);
        }
    }

    #[test]
    fn ga_append_via_ptr_reserves_writable_slot() {
        let mut ga = GarrayT::new(4, 2);
        unsafe {
            let p = ga.ga_append_via_ptr(4) as *mut i32;
            p.write(42);
        }
        assert_eq!(ga.ga_len, 1);
        let ptr = ga.ga_data.as_ptr() as *const i32;
        unsafe {
            assert_eq!(*ptr, 42);
        }
    }

    #[test]
    fn ga_concat_len_appends_bytes() {
        let mut ga = GarrayT::new(1, 8);
        ga.ga_concat_len(b"hello");
        ga.ga_concat_len(b" world");
        assert_eq!(ga.ga_len, 11);
        assert_eq!(&ga.ga_data[..11], b"hello world");
    }

    #[test]
    fn ga_concat_none_is_noop() {
        let mut ga = GarrayT::new(1, 8);
        ga.ga_concat(None);
        assert_eq!(ga.ga_len, 0);
    }

    #[test]
    fn ga_clear_resets_len_and_maxlen_but_not_itemsize() {
        let mut ga = GarrayT::new(4, 8);
        ga.ga_grow(2);
        ga.ga_clear();
        assert_eq!(ga.ga_len, 0);
        assert_eq!(ga.ga_maxlen, 0);
        assert_eq!(ga.ga_itemsize, 4); // preserved
        assert_eq!(ga.ga_growsize, 8); // preserved
    }
}
