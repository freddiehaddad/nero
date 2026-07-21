//! Translated from `src/nvim/hashtab_defs.h`.

/// Type for hash number (hash calculation result) (`hash_T`).
pub type HashT = usize;

/// Initial size for a hashtable (`HT_INIT_SIZE`).
/// Our items are relatively small and growing is expensive, thus start with
/// 16. Must be a power of 2. This allows for storing 10 items (2/3 of 16)
/// before a resize is needed.
pub const HT_INIT_SIZE: usize = 16;

/// Hashtable item (`hashitem_T`).
///
/// Each item has a NUL terminated string key. A key can appear only once in
/// the table.
///
/// A hash number is computed from the key for quick lookup. When the hashes
/// of two different keys point to the same entry an algorithm is used to
/// iterate over other entries in the table until the right one is found. To
/// make the iteration work, removed keys are different from entries where a
/// key was never present.
///
/// Note that this does not contain a pointer to the key and another pointer
/// to the value. Instead, it is assumed that the key is contained within the
/// value, so that you can get a pointer to the value by subtracting an
/// offset from the pointer to the key. This reduces the size of this item by
/// 1/3. (Preserved as-is: `hi_key` is a raw pointer into the surrounding
/// value, not an owned `CString`.)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HashitemT {
    /// Cached hash number for `hi_key`.
    pub hi_hash: HashT,
    /// Item key.
    ///
    /// Possible values mean the following:
    /// - null: Item was never used.
    /// - `HI_KEY_REMOVED`: Item was removed.
    /// - (Any other pointer value): Item is currently being used.
    pub hi_key: *mut std::os::raw::c_char,
}

impl Default for HashitemT {
    fn default() -> Self {
        HashitemT {
            hi_hash: 0,
            hi_key: std::ptr::null_mut(),
        }
    }
}

/// The original's `ht_array` is a raw pointer that, for a small table,
/// points *into this same struct's* `ht_smallarray` field - safe in C
/// (structs aren't implicitly relocated), but a dangling-pointer hazard in
/// Rust, where structs move freely (returned by value, put in a `Vec`,
/// etc. - any such move would leave a raw self-pointer pointing at the
/// old, now-invalid location). This enum gets the identical observable
/// behavior (small inline array up to `HT_INIT_SIZE` items, falling back
/// to a heap array beyond that) without a self-referential pointer.
///
/// `#[allow(clippy::large_enum_variant)]`: boxing `Small` (as clippy
/// suggests by default) would defeat its entire purpose, which - per the
/// original's own comment on `HT_INIT_SIZE` - is avoiding a heap
/// allocation for small tables, the common case.
#[allow(clippy::large_enum_variant)]
pub enum HashArray {
    Small([HashitemT; HT_INIT_SIZE]),
    Large(Vec<HashitemT>),
}

impl HashArray {
    #[inline]
    pub fn as_slice(&self) -> &[HashitemT] {
        match self {
            HashArray::Small(a) => a,
            HashArray::Large(v) => v,
        }
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [HashitemT] {
        match self {
            HashArray::Small(a) => a,
            HashArray::Large(v) => v,
        }
    }

    #[inline]
    pub fn is_small(&self) -> bool {
        matches!(self, HashArray::Small(_))
    }
}

/// An array-based hashtable (`hashtab_T`).
///
/// Keys are NUL terminated strings. They cannot be repeated within a table.
/// Values are of any type.
///
/// The hashtable grows to accommodate more entries when needed.
pub struct HashtabT {
    /// mask used for hash value (nr of items in array is `ht_mask + 1`)
    pub ht_mask: HashT,
    /// number of items used
    pub ht_used: usize,
    /// number of items used or removed
    pub ht_filled: usize,
    /// incremented when adding or removing an item
    pub ht_changed: i32,
    /// counter for `hash_lock()`
    pub ht_locked: i32,
    /// the array (in place of the original's raw `ht_array` pointer +
    /// inline `ht_smallarray` field - see [`HashArray`])
    pub ht_array: HashArray,
}
