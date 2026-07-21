//! Translated from `src/nvim/garray_defs.h`.

/// Structure used for growing arrays (`garray_T`).
/// This is used to store information that only grows, is deleted all at
/// once, and needs to be accessed by index. See `ga_clear()` and `ga_grow()`
/// (`src/nvim/garray.rs`, translated from `garray.c`).
///
/// `ga_data` is a raw byte buffer (`Vec<u8>`), matching the original's
/// `void *` in spirit: the "item type" is genuinely erased at this level
/// (callers reinterpret it as `[u8]`, `[SomeStruct]`, an array of string
/// pointers, etc. depending on context, exactly like the C code's pointer
/// casts) - but *ownership and growth* are handled by `Vec`'s safe,
/// automatic allocation instead of manual `malloc`/`realloc`/`free`, since
/// nothing here needs the manual block/header tricks that (for example)
/// the arena allocator in `memory.c` does. `ga_maxlen` is kept as an
/// explicit field (capacity *in items*, mirroring the original's field
/// exactly) even though it is derivable from `ga_data`, for direct
/// traceability to the C struct's layout.
#[derive(Debug, Clone, Default)]
pub struct GarrayT {
    /// current number of items used
    pub ga_len: i32,
    /// maximum number of items possible
    pub ga_maxlen: i32,
    /// sizeof(item)
    pub ga_itemsize: i32,
    /// number of items to grow each time
    pub ga_growsize: i32,
    /// the backing byte buffer (in place of `void *ga_data`)
    pub ga_data: Vec<u8>,
}

impl GarrayT {
    /// `GA_EMPTY_INIT_VALUE`
    #[inline]
    pub fn empty_init_value() -> GarrayT {
        GarrayT {
            ga_len: 0,
            ga_maxlen: 0,
            ga_itemsize: 0,
            ga_growsize: 1,
            ga_data: Vec::new(),
        }
    }

    /// `GA_INIT(itemsize, growsize)`
    #[inline]
    pub fn new(itemsize: i32, growsize: i32) -> GarrayT {
        GarrayT {
            ga_len: 0,
            ga_maxlen: 0,
            ga_itemsize: itemsize,
            ga_growsize: growsize,
            ga_data: Vec::new(),
        }
    }

    /// `GA_EMPTY(ga_ptr)` (from `src/nvim/garray.h`)
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ga_len <= 0
    }
}
