//! Translated from `src/nvim/garray_defs.h`.

/// Structure used for growing arrays (`garray_T`).
/// This is used to store information that only grows, is deleted all at
/// once, and needs to be accessed by index. See `ga_clear()` and `ga_grow()`
/// (`src/nvim/garray.c`, phase 2).
///
/// Kept as a raw/untyped buffer (`ga_data: *mut c_void`, `ga_itemsize` in
/// bytes) exactly like the original C struct, rather than a generic `Vec<T>`
///   - `garray.c`'s logic (not yet translated) operates on it via manual
///     pointer arithmetic over an item size known only at runtime, so that
///     is what is faithfully mirrored here for now.
#[repr(C)]
pub struct GarrayT {
    /// current number of items used
    pub ga_len: i32,
    /// maximum number of items possible
    pub ga_maxlen: i32,
    /// sizeof(item)
    pub ga_itemsize: i32,
    /// number of items to grow each time
    pub ga_growsize: i32,
    /// pointer to the first item
    pub ga_data: *mut std::ffi::c_void,
}

impl GarrayT {
    /// `GA_EMPTY_INIT_VALUE`
    pub const EMPTY_INIT_VALUE: GarrayT = GarrayT {
        ga_len: 0,
        ga_maxlen: 0,
        ga_itemsize: 0,
        ga_growsize: 1,
        ga_data: std::ptr::null_mut(),
    };

    /// `GA_INIT(itemsize, growsize)`
    #[inline]
    pub const fn new(itemsize: i32, growsize: i32) -> GarrayT {
        GarrayT {
            ga_len: 0,
            ga_maxlen: 0,
            ga_itemsize: itemsize,
            ga_growsize: growsize,
            ga_data: std::ptr::null_mut(),
        }
    }
}
