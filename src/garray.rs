//! Translated from `src/nvim/garray.c` ("Functions for handling growing
//! arrays") and the macros in `src/nvim/garray.h`.
//!
//! [`remove_duplicate_strings`]/[`concat_strings`] (`ga_remove_duplicate_
//! strings`/`ga_concat_strings`) ARE translated, but as plain
//! `Vec<Vec<u8>>`-operating functions rather than `GarrayT` methods:
//! the original treats `ga_data` as an array of *string pointers*
//! (`char **`/`const char **`) for these two, a completely different
//! usage of `garray_T` than "one flat byte buffer" (which is what the
//! rest of this file's functions treat it as, and what this
//! translation's `GarrayT.ga_data: Vec<u8>` models directly) -
//! translated ahead of a real caller (`path.c`/`runtime.c`, neither
//! translated yet), matching this crate's established "small,
//! self-contained piece ahead of the surrounding engine" precedent,
//! now that both real dependencies (`path.c`'s `path_fnamecmp`,
//! `strings.c`'s `sort_strings`) exist.
//!
//! `ga_clear_strings`/`GA_DEEP_CLEAR`/`GA_DEEP_CLEAR_PTR` need NO Rust
//! equivalent at all for the same reason `optval_free`/`tv_dict_clear`
//! don't (see `option.rs`'s/`eval/typval.rs`'s own module docs): a
//! `Vec<Vec<u8>>`'s own `Drop` (or a plain `.clear()`) already
//! individually releases every owned string, exactly matching what the
//! original's manual per-string `xfree` loop achieves by hand.
//!
//! - `WLOG(...)` calls now use `crate::log::logmsg` directly (`log.c` is
//!   translated as of this revision).

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
            crate::log::logmsg(
                crate::log::LOGLVL_WRN,
                None,
                Some("ga_set_growsize"),
                Some(line!() as i32),
                true,
                &format!("trying to set an invalid ga_growsize: {growsize}"),
            );
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
            crate::log::logmsg(
                crate::log::LOGLVL_WRN,
                None,
                Some("ga_grow"),
                Some(line!() as i32),
                true,
                &format!("ga_growsize({}) is less than 1", self.ga_growsize),
            );
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
    /// # Alignment
    /// The original stores items in an `xmalloc`ed `void *ga_data`, which is
    /// aligned for any fundamental type, so it dereferences the slot as a
    /// plain `item_type *`. This translation backs the array with a
    /// `Vec<u8>`, which guarantees only byte alignment, so the typed store
    /// must be an unaligned one - an aligned write here is undefined
    /// behaviour for any `T` with an alignment above 1 (confirmed under
    /// Miri). The observable result is identical, and on the platforms this
    /// targets an unaligned store of an aligned-size scalar costs nothing.
    ///
    /// # Safety
    /// The caller must ensure `self.ga_itemsize == size_of::<T>()` (set via
    /// [`GarrayT::new`]) - like the original macro, this is not checked.
    #[inline]
    pub unsafe fn ga_append_item<T: Copy>(&mut self, item: T) { unsafe {
        self.ga_grow(1);
        let idx = self.ga_len as usize * std::mem::size_of::<T>();
        let ptr = self.ga_data.as_mut_ptr().add(idx) as *mut T;
        ptr.write_unaligned(item);
        self.ga_len += 1;
    }}

    /// Reserves room for one more item and returns a pointer to it, without
    /// initializing it (`ga_append_via_ptr`).
    ///
    /// # Safety
    /// The returned pointer is valid for exactly `self.ga_itemsize` bytes;
    /// the caller must fully initialize it (matches the original's
    /// contract, which hands back a raw, uninitialized slot).
    ///
    /// It carries only byte alignment, because the backing store is a
    /// `Vec<u8>` rather than the original's `xmalloc`ed `void *` (see
    /// [`GarrayT::ga_append_item`]). A caller that casts it to some
    /// `*mut T` must therefore use [`std::ptr::write_unaligned`] and
    /// [`std::ptr::read_unaligned`]; an aligned access is undefined
    /// behaviour for any `T` whose alignment exceeds 1.
    pub unsafe fn ga_append_via_ptr(&mut self, item_size: usize) -> *mut u8 { unsafe {
        if item_size as i32 != self.ga_itemsize {
            crate::log::logmsg(
                crate::log::LOGLVL_WRN,
                None,
                Some("ga_append_via_ptr"),
                Some(line!() as i32),
                true,
                &format!("wrong item size ({}), should be {}", item_size, self.ga_itemsize),
            );
        }
        self.ga_grow(1);
        let idx = self.ga_len as usize * self.ga_itemsize as usize;
        self.ga_len += 1;
        self.ga_data.as_mut_ptr().add(idx)
    }}

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

/// Sort `names` and remove duplicate entries
/// (`ga_remove_duplicate_strings`). `names` is expected to contain a
/// list of file names.
///
/// Modeled as a plain `&mut Vec<Vec<u8>>`-operating function rather
/// than a `GarrayT` method, per this module's own doc comment: a
/// `garray_T` used as a string list is a fundamentally different shape
/// than `GarrayT`'s own byte-buffer model. `Vec::remove`'s own
/// built-in "shift everything after this index down by one" behavior
/// already replicates the original's manual `for (j = i + 1; ...)`
/// close-the-gap loop, so it isn't translated separately.
///
/// # Safety
/// Same as [`crate::path::path_fnamecmp`] (every element of `names`
/// must be a valid file-name byte string per that function's own
/// contract - in practice, always true for a plain `Vec<u8>`).
pub unsafe fn remove_duplicate_strings(names: &mut Vec<Vec<u8>>) {
    // sort first, which puts duplicates next to each other
    crate::strings::sort_strings(names);

    // loop over the list in reverse
    let mut i = names.len();
    while i > 1 {
        i -= 1;
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::path::path_fnamecmp(&names[i - 1], &names[i]) } == 0 {
            names.remove(i);
        }
    }
}

/// For a list of strings: concatenate all of them with `sep` as
/// separator (`ga_concat_strings`). Modeled as a plain `&[Vec<u8>]`-
/// taking function rather than a `GarrayT` method, per this module's
/// own doc comment.
#[must_use]
pub fn concat_strings(strings: &[Vec<u8>], sep: &[u8]) -> Vec<u8> {
    let payload_len: usize = strings.iter().map(Vec::len).sum();
    let separator_len = sep.len().saturating_mul(strings.len().saturating_sub(1));
    let mut result = Vec::with_capacity(payload_len + separator_len);

    for (idx, string) in strings.iter().enumerate() {
        if idx != 0 {
            result.extend_from_slice(sep);
        }
        result.extend_from_slice(string);
    }

    result
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
        // The `Vec<u8>` backing store carries only byte alignment, so
        // reading items back out must be unaligned too.
        let ptr = ga.ga_data.as_ptr() as *const i32;
        unsafe {
            assert_eq!(ptr.read_unaligned(), 10);
            assert_eq!(ptr.add(1).read_unaligned(), 20);
        }
    }

    #[test]
    fn ga_append_via_ptr_reserves_writable_slot() {
        let mut ga = GarrayT::new(4, 2);
        unsafe {
            let p = ga.ga_append_via_ptr(4) as *mut i32;
            p.write_unaligned(42);
        }
        assert_eq!(ga.ga_len, 1);
        let ptr = ga.ga_data.as_ptr() as *const i32;
        unsafe {
            assert_eq!(ptr.read_unaligned(), 42);
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

    #[test]
    fn remove_duplicate_strings_sorts_and_dedups() {
        let mut names: Vec<Vec<u8>> = vec![b"b.txt".to_vec(), b"a.txt".to_vec(), b"b.txt".to_vec()];
        unsafe { remove_duplicate_strings(&mut names) };
        assert_eq!(names, vec![b"a.txt".to_vec(), b"b.txt".to_vec()]);
    }

    #[test]
    fn remove_duplicate_strings_no_duplicates_just_sorts() {
        let mut names: Vec<Vec<u8>> = vec![b"b.txt".to_vec(), b"a.txt".to_vec()];
        unsafe { remove_duplicate_strings(&mut names) };
        assert_eq!(names, vec![b"a.txt".to_vec(), b"b.txt".to_vec()]);
    }

    #[test]
    fn remove_duplicate_strings_collapses_3_consecutive_duplicates_to_1() {
        let mut names: Vec<Vec<u8>> =
            vec![b"c.txt".to_vec(), b"c.txt".to_vec(), b"c.txt".to_vec()];
        unsafe { remove_duplicate_strings(&mut names) };
        assert_eq!(names, vec![b"c.txt".to_vec()]);
    }

    #[test]
    fn remove_duplicate_strings_empty_is_a_noop() {
        let mut names: Vec<Vec<u8>> = vec![];
        unsafe { remove_duplicate_strings(&mut names) };
        assert!(names.is_empty());
    }

    #[test]
    fn remove_duplicate_strings_single_element_is_a_noop() {
        let mut names: Vec<Vec<u8>> = vec![b"a.txt".to_vec()];
        unsafe { remove_duplicate_strings(&mut names) };
        assert_eq!(names, vec![b"a.txt".to_vec()]);
    }

    #[test]
    fn concat_strings_joins_with_separator() {
        let strings = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()];
        assert_eq!(concat_strings(&strings, b","), b"a,b,c".to_vec());
    }

    #[test]
    fn concat_strings_empty_list_is_empty_string() {
        let strings: Vec<Vec<u8>> = vec![];
        assert_eq!(concat_strings(&strings, b","), Vec::<u8>::new());
    }

    #[test]
    fn concat_strings_single_element_has_no_separator() {
        let strings = vec![b"only".to_vec()];
        assert_eq!(concat_strings(&strings, b","), b"only".to_vec());
    }

    #[test]
    fn concat_strings_supports_empty_elements_and_separator() {
        let strings = vec![Vec::new(), b"b".to_vec(), Vec::new()];
        assert_eq!(concat_strings(&strings, b"::"), b"::b::".to_vec());
        assert_eq!(concat_strings(&strings, b""), b"b".to_vec());
    }
}
