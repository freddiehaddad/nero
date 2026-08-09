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

/// A growarray of a specific item type, for the original's `garray_T`
/// uses whose items are NOT plain old data.
///
/// [`GarrayT`] erases its item type into a `Vec<u8>`, exactly as the
/// original's `void *ga_data` does. That works for the many garrays
/// holding bytes, pointers, or `Copy` structs, but it cannot hold an
/// item that owns heap memory - `aentry_T`, for instance, owns its
/// `ae_fname` string. Storing such an item's bytes in a byte buffer
/// would leak the owned allocation (nothing would ever drop it) or
/// double-free it, and `GarrayT::ga_append_item` accordingly requires
/// `T: Copy`.
///
/// This type keeps the original's growarray SEMANTICS (an
/// append-only, index-addressed array cleared all at once, with an
/// explicit grow size) while letting `Vec<T>` own the items properly.
///
/// `ga_len` and `ga_maxlen` are DERIVED here rather than stored as
/// separate fields the way the original keeps them: a length that can
/// disagree with the actual contents is precisely the bug this type
/// exists to rule out.
#[derive(Debug, Clone)]
pub struct TypedGarrayT<T> {
    /// number of items to grow each time (`ga_growsize`)
    pub ga_growsize: i32,
    /// the items themselves (in place of `void *ga_data`)
    pub items: Vec<T>,
}

impl<T> Default for TypedGarrayT<T> {
    fn default() -> Self {
        TypedGarrayT { ga_growsize: 1, items: Vec::new() }
    }
}

impl<T> TypedGarrayT<T> {
    /// `GA_INIT(sizeof(T), growsize)` - the item size is carried by
    /// the type itself, so only the grow size is a parameter.
    #[inline]
    #[must_use]
    pub fn new(growsize: i32) -> Self {
        TypedGarrayT { ga_growsize: growsize, items: Vec::new() }
    }

    /// `ga_len` - the current number of items used.
    #[inline]
    #[must_use]
    pub fn ga_len(&self) -> i32 {
        i32::try_from(self.items.len()).unwrap_or(i32::MAX)
    }

    /// `ga_maxlen` - the number of items that fit without growing.
    #[inline]
    #[must_use]
    pub fn ga_maxlen(&self) -> i32 {
        i32::try_from(self.items.capacity()).unwrap_or(i32::MAX)
    }

    /// `GA_EMPTY(ga_ptr)`
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// `ga_clear()` - drop every item and reset to empty. Unlike
    /// [`GarrayT::ga_clear`] this also runs each item's own
    /// destructor, which is what the original's `GA_DEEP_CLEAR`
    /// hand-written per-item free loops do.
    #[inline]
    pub fn ga_clear(&mut self) {
        self.items.clear();
        self.items.shrink_to_fit();
    }

    /// `ga_init(itemsize, growsize)` - reset to empty with a new grow
    /// size.
    #[inline]
    pub fn ga_init(&mut self, growsize: i32) {
        self.ga_clear();
        self.ga_growsize = growsize;
    }

    /// `ga_grow(n)` - make room for at least `n` more items.
    #[inline]
    pub fn ga_grow(&mut self, n: i32) {
        if n > 0 {
            self.items.reserve(n as usize);
        }
    }

    /// One item by index, or `None` when out of range.
    #[inline]
    #[must_use]
    pub fn get(&self, idx: i32) -> Option<&T> {
        usize::try_from(idx).ok().and_then(|i| self.items.get(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_garray_default_is_empty_with_a_grow_size_of_one() {
        let ga: TypedGarrayT<i32> = TypedGarrayT::default();
        assert!(ga.is_empty());
        assert_eq!(ga.ga_len(), 0);
        assert_eq!(ga.ga_growsize, 1);
    }

    #[test]
    fn typed_garray_len_tracks_the_items_rather_than_a_separate_field() {
        // The whole point of the type: there is no length field that
        // can disagree with the contents.
        let mut ga: TypedGarrayT<i32> = TypedGarrayT::new(5);
        assert_eq!(ga.ga_len(), 0);
        ga.items.push(1);
        ga.items.push(2);
        assert_eq!(ga.ga_len(), 2);
        assert!(!ga.is_empty());
        ga.items.pop();
        assert_eq!(ga.ga_len(), 1);
    }

    #[test]
    fn typed_garray_grow_reserves_without_changing_the_length() {
        let mut ga: TypedGarrayT<i32> = TypedGarrayT::new(5);
        ga.ga_grow(10);
        assert_eq!(ga.ga_len(), 0, "growing must not invent items");
        assert!(ga.ga_maxlen() >= 10);
    }

    #[test]
    fn typed_garray_init_resets_the_items_and_sets_the_grow_size() {
        let mut ga: TypedGarrayT<i32> = TypedGarrayT::new(1);
        ga.items.extend([1, 2, 3]);
        ga.ga_init(5);
        assert!(ga.is_empty());
        assert_eq!(ga.ga_growsize, 5);
    }

    #[test]
    fn typed_garray_get_is_bounds_checked_including_negative_indices() {
        let mut ga: TypedGarrayT<i32> = TypedGarrayT::new(1);
        ga.items.extend([10, 20]);
        assert_eq!(ga.get(0), Some(&10));
        assert_eq!(ga.get(1), Some(&20));
        assert_eq!(ga.get(2), None);
        // The original indexes with a plain int, so a negative index
        // must not wrap into a huge usize.
        assert_eq!(ga.get(-1), None);
    }

    /// The reason this type exists: an item owning heap memory is
    /// dropped properly when the growarray is cleared. A byte-erased
    /// `GarrayT` would simply forget the allocation.
    #[test]
    fn typed_garray_clear_drops_items_that_own_heap_memory() {
        use std::rc::Rc;

        let witness = Rc::new(());
        let mut ga: TypedGarrayT<(Rc<()>, Vec<u8>)> = TypedGarrayT::new(2);
        ga.items.push((Rc::clone(&witness), b"owned name".to_vec()));
        assert_eq!(Rc::strong_count(&witness), 2);

        ga.ga_clear();
        assert!(ga.is_empty());
        assert_eq!(
            Rc::strong_count(&witness),
            1,
            "clearing must drop the item, releasing what it owned"
        );
    }
}

