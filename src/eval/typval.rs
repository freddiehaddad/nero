//! Translated from `src/nvim/eval/typval.c` (tractable core only:
//! `dict_T`/`dictitem_T` allocation, lookup, and insertion).
//!
//! `typval.c` (~4000 lines) is the core of the Vimscript value system:
//! `typval_T`/`list_T`/`dict_T`/`blob_T` construction, (de)serialization
//! via a shared encode-traversal abstraction, deep copying, and every
//! built-in operation on those types. Only the foundational `dict_T`
//! allocation/lookup/insertion primitives are translated here; see
//! this module's own per-function deferral notes below for the rest.
//!
//! # `dict_T`/`dictitem_T` representation (the design decision this
//! unblocks)
//!
//! See `eval/typval_defs.rs`'s `DictitemT`/`DictT` doc comments for
//! the full reasoning. In short: the original's `dictitem_T` uses a C
//! "flexible array member" (`di_key[]`) so that `hashtab_defs.rs`'s
//! `HashitemT.hi_key` can point directly at the key bytes living
//! *inside* the same allocation as the rest of the item, letting
//! `TV_DICT_HI2DI` recover the owning `dictitem_T*` via
//! `hi_key - offsetof(dictitem_T, di_key)` pointer arithmetic. Rust
//! has no safe equivalent of this (a faithful replication would need
//! a hand-rolled dynamically-sized type: manual `Layout` computation,
//! raw `alloc`/`dealloc`, fat-pointer reconstruction on every access -
//! disproportionate unsafe complexity for what is, in the original,
//! purely a one-pointer memory optimization with no observable
//! behavioral difference). So `DictitemT.di_key` is an owned `Vec<u8>`
//! instead (a separate heap allocation, matching the already-existing
//! `ChangedtickDictItem`/`ScopeDictDictItem` precedent), and `DictT`
//! carries a new `dv_index: HashMap<usize, *mut DictitemT>` side table
//! (keyed by each item's `hi_key` address) in place of
//! `TV_DICT_HI2DI` - every function below that would use that macro
//! consults `dv_index` instead.
//!
//! `DictitemT`/`DictT` are heap-allocated via `Box::into_raw`/
//! `Box::from_raw`, matching `ListitemT`'s established raw-pointer-
//! linked convention (not `Rc`/`RefCell`).
//!
//! # Translated
//! `tv_dict_item_alloc`(`_len`) (collapsed into one function taking
//! `&[u8]`, matching this crate's established "no separate NUL-scanning
//! variant needed" precedent - e.g. `hashtab.rs`'s `hash_find`/
//! `hash_find_len`); `tv_dict_item_free` (the List/Dict/Blob/Partial-
//! valued-item branches are `unimplemented!()` - see its own doc
//! comment); `tv_dict_item_remove`; `tv_dict_alloc`/
//! `tv_dict_free_contents`/`tv_dict_free_dict`/`tv_dict_free`;
//! `tv_dict_find`/`tv_dict_has_key`; `tv_dict_add` (omits the
//! original's `tv_dict_wrong_func_name` g:/l: validation - needs
//! `get_globvar_dict`/`get_funccal_local_ht`/`var_wrong_func_name`,
//! none translated, and nothing in this crate can even construct a
//! real global/local-funccall scope dict yet for that check to apply
//! to).
//!
//! `gc_first_dict` (the original's file-static "list of all live
//! dicts, for `:garbagecollect`" linked-list head) is translated as
//! its own `GlobalCell`-backed static, matching `buffer.rs`'s
//! `TOP_FILE_NUM`/`BUF_FREE_COUNT` precedent - the linked-list
//! bookkeeping itself (`dv_used_next`/`dv_used_prev`) is maintained
//! faithfully even though the actual garbage collector that would walk
//! it is a much later phase, so that phase won't need to retrofit this
//! bookkeeping later.
//!
//! `watchers`/`lua_table_ref` are left inert: `DictT` has no `watchers`
//! field at all yet (needs a `QUEUE` intrusive-linked-list translation
//! first - see `typval_defs.rs`), and `lua_table_ref` is always
//! `LUA_NOREF` (the Lua host, phase 13, isn't started).
//!
//! # Deferred
//! - `tv_dict_item_copy`: needs `tv_copy` (deep-copies a `TypvalT`,
//!   including refcount increments for List/Dict/Blob/Partial), not
//!   yet translated.
//! - `tv_clear`/`tv_free`/`tv_copy` themselves: `tv_clear`'s *real*
//!   behavior is implemented via a shared encode-traversal abstraction
//!   (`encode_vim_to_nothing`, `viml_encode.c` - reused for JSON/
//!   msgpack encoding too, not just clearing) - a separate, substantial
//!   subsystem of its own, not attempted here.
//! - Every other `tv_dict_*` function (`tv_dict_get_string`,
//!   `tv_dict_add_list`/`_dict`/`_str`/`_nr`/`_float`/`_allocated_str`,
//!   `tv_dict_extend`, iteration helpers, etc.): straightforward to add
//!   once needed, layered on top of the primitives here.

use crate::eval::typval_defs::{dict_item_flags, DictT, DictitemT, ScopeType, TypvalT, TypvalValue, VarLockStatus};
use crate::globals::GlobalCell;
use crate::vim_defs::OK;

/// `LUA_NOREF`: represents a missing Lua reference - `DictT`'s own
/// `lua_table_ref` is always this value until the Lua host (phase 13)
/// exists.
const LUA_NOREF: crate::types_defs::LuaRef = -1;

/// `gc_first_dict`: head of the linked list of all live dictionaries
/// (via `dv_used_next`/`dv_used_prev`), maintained for a future
/// `:garbagecollect` implementation - see this module's own doc
/// comment.
static GC_FIRST_DICT: GlobalCell<*mut DictT> = GlobalCell::new(std::ptr::null_mut());

/// Allocate a dictionary item. The type and value of the item
/// (`.di_tv`) still need to be initialized by the caller
/// (`tv_dict_item_alloc`/`tv_dict_item_alloc_len` - collapsed into one
/// function here, see this module's own doc comment for why).
#[must_use]
pub fn tv_dict_item_alloc(key: &[u8]) -> *mut DictitemT {
    let mut di_key = Vec::with_capacity(key.len() + 1);
    di_key.extend_from_slice(key);
    di_key.push(0); // NUL terminator, matching hi_key's C-string contract
    Box::into_raw(Box::new(DictitemT {
        di_tv: TypvalT::default(),
        di_flags: dict_item_flags::ALLOC,
        di_key,
    }))
}

/// Free a dictionary item, also clearing the value (`tv_dict_item_free`).
///
/// The original's `tv_clear(&item->di_tv)` recursively decrements
/// refcounts for List/Dict/Blob/Partial-valued items via the not-yet-
/// translated encode-traversal machinery; those 4 branches are
/// `unimplemented!()` here - narrow, reached only when actually
/// freeing an item whose value is one of those 4 types (nothing in
/// this crate's own test suite constructs one yet).
///
/// # Safety
/// `item` must be a valid pointer previously returned by
/// [`tv_dict_item_alloc`] (or, for the "not separately allocated"
/// case - `di_flags` without [`dict_item_flags::ALLOC`] - a pointer
/// into a live, embedded `dictitem_T`-shaped struct like
/// `ChangedtickDictItem`), not yet freed, and no longer reachable from
/// any hashtable/other structure (the caller's job - see
/// [`tv_dict_item_remove`] for the usual "remove from hashtab, then
/// free" pairing this crate expects).
pub unsafe fn tv_dict_item_free(item: *mut DictitemT) {
    // SAFETY: forwarded from this function's own safety doc.
    let value_is_refcounted = unsafe {
        matches!(
            (*item).di_tv.value,
            TypvalValue::List(_) | TypvalValue::Dict(_) | TypvalValue::Blob(_) | TypvalValue::Partial(_)
        )
    };
    if value_is_refcounted {
        unimplemented!(
            "tv_dict_item_free: freeing a List/Dict/Blob/Partial-valued item needs \
             tv_list_unref/tv_dict_unref/tv_blob_unref/partial_unref, none translated yet"
        );
    }

    // SAFETY: forwarded from this function's own safety doc.
    let flags = unsafe { (*item).di_flags };
    if flags & dict_item_flags::ALLOC != 0 {
        // SAFETY: `DI_FLAGS_ALLOC` guarantees this came from
        // `tv_dict_item_alloc`'s own `Box::into_raw` - forwarded from
        // this function's own safety doc.
        drop(unsafe { Box::from_raw(item) });
    } else {
        // Not separately allocated (e.g. embedded in another struct
        // like `ChangedtickDictItem`) - clear the value in place but
        // don't free the item itself, matching the original exactly.
        // Assigning through the raw pointer runs the old value's Drop
        // (releasing any owned String/Vec) automatically.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*item).di_tv = TypvalT::default() };
    }
}

/// Remove item from dictionary and free it (`tv_dict_item_remove`).
///
/// # Safety
/// `item` must be a valid pointer currently present in `dict`
/// (previously added via [`tv_dict_add`]), matching
/// [`tv_dict_item_free`]'s own contract for the rest.
pub unsafe fn tv_dict_item_remove(dict: &mut DictT, item: *mut DictitemT) {
    // SAFETY: forwarded from this function's own safety doc.
    let key_ptr = unsafe { (*item).di_key.as_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let key: &[u8] = unsafe { &(*item).di_key };
    // Strip the trailing NUL `di_key` carries - `hash_remove` (like
    // `hash_find`) takes the bare key bytes.
    let key = &key[..key.len().saturating_sub(1)];
    dict.dv_hashtab.hash_remove(key);
    // `dv_index` is keyed by each item's `hi_key` address (the key
    // bytes' own pointer), not the item's own address - matching how
    // `tv_dict_add` inserted it.
    dict.dv_index.remove(&(key_ptr as usize));
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict_item_free(item) };
}

/// Allocate an empty dictionary. Caller should take care of the
/// reference count (`tv_dict_alloc`).
#[must_use]
pub fn tv_dict_alloc() -> *mut DictT {
    let d = Box::into_raw(Box::new(DictT {
        dv_lock: VarLockStatus::Unlocked,
        dv_scope: ScopeType::NoScope,
        dv_refcount: 0,
        dv_copy_id: 0,
        dv_hashtab: crate::hashtab_defs::HashtabT::hash_init(),
        dv_index: std::collections::HashMap::new(),
        dv_copydict: std::ptr::null_mut(),
        dv_used_next: std::ptr::null_mut(),
        dv_used_prev: std::ptr::null_mut(),
        lua_table_ref: LUA_NOREF,
    }));

    // Add the dict to the list of dicts for garbage collection.
    // SAFETY: GC_FIRST_DICT is only ever read/written through this
    // module's own functions, which never hold a live reference across
    // another call into this same cell.
    let gc_first = unsafe { *GC_FIRST_DICT.get_mut() };
    if !gc_first.is_null() {
        // SAFETY: gc_first is either null (checked above) or a live
        // pointer previously produced by this same function.
        unsafe { (*gc_first).dv_used_prev = d };
    }
    // SAFETY: forwarded from this function's own reasoning above.
    unsafe { (*d).dv_used_next = gc_first };
    // SAFETY: forwarded from this function's own reasoning above.
    unsafe { *GC_FIRST_DICT.get_mut() = d };

    d
}

/// Free items contained in a dictionary (`tv_dict_free_contents`).
///
/// # Safety
/// `d` must be a valid, non-null pointer to a live `DictT` whose every
/// item satisfies [`tv_dict_item_free`]'s own safety contract.
pub unsafe fn tv_dict_free_contents(d: *mut DictT) {
    // SAFETY: forwarded from this function's own safety doc.
    let dict = unsafe { &mut *d };
    // Unlike the original (which locks dv_hashtab, walks it via
    // HASHTAB_ITER + TV_DICT_HI2DI, and removes each item one at a
    // time), dv_index already gives a direct list of every live item -
    // no hashtab traversal/locking needed at all.
    let items: Vec<*mut DictitemT> = dict.dv_index.values().copied().collect();
    for item in items {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_dict_item_free(item) };
    }
    dict.dv_index.clear();
    dict.dv_hashtab = crate::hashtab_defs::HashtabT::hash_init();
}

/// Free a dictionary itself, ignoring items it contains. Ignores the
/// reference count (`tv_dict_free_dict`).
///
/// # Safety
/// `d` must be a valid pointer previously returned by
/// [`tv_dict_alloc`], not yet freed.
pub unsafe fn tv_dict_free_dict(d: *mut DictT) {
    // Remove the dict from the list of dicts for garbage collection.
    // SAFETY: forwarded from this function's own safety doc.
    let (used_prev, used_next) = unsafe { ((*d).dv_used_prev, (*d).dv_used_next) };
    if used_prev.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *GC_FIRST_DICT.get_mut() = used_next };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*used_prev).dv_used_next = used_next };
    }
    if !used_next.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*used_next).dv_used_prev = used_prev };
    }

    // NLUA_CLEAR_REF(d->lua_table_ref): omitted - the Lua host (phase
    // 13) isn't started, and lua_table_ref is always LUA_NOREF here.

    // SAFETY: forwarded from this function's own safety doc.
    drop(unsafe { Box::from_raw(d) });
}

/// Free a dictionary, including all items it contains. Ignores the
/// reference count (`tv_dict_free`).
///
/// # Safety
/// Same as [`tv_dict_free_contents`]/[`tv_dict_free_dict`] combined.
pub unsafe fn tv_dict_free(d: *mut DictT) {
    // The original's `tv_in_free_unref_items` re-entrancy guard is
    // always false here - nothing in this crate can trigger the
    // garbage-collector's "unreferencing everything" pass that sets it
    // (that pass doesn't exist yet).
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict_free_contents(d) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict_free_dict(d) };
}

/// Find item in dictionary (`tv_dict_find`).
///
/// Unlike the original (`ptrdiff_t len`, negative meaning
/// "NUL-terminated"), takes `key: &[u8]` directly - a Rust slice
/// always carries its own length, so there is nothing left to
/// distinguish (same reasoning as `hashtab.rs`'s own `hash_find`/
/// `hash_find_len` collapse).
#[must_use]
pub fn tv_dict_find(d: Option<&mut DictT>, key: &[u8]) -> Option<*mut DictitemT> {
    let d = d?;
    let hi = d.dv_hashtab.hash_find(key);
    if crate::hashtab::hashitem_empty(hi) {
        return None;
    }
    d.dv_index.get(&(hi.hi_key as usize)).copied()
}

/// Check if a key is present in a dictionary (`tv_dict_has_key`).
#[must_use]
pub fn tv_dict_has_key(d: Option<&mut DictT>, key: &[u8]) -> bool {
    tv_dict_find(d, key).is_some()
}

/// Add item to dictionary (`tv_dict_add`).
///
/// @return `FAIL` if key already exists.
///
/// Omits the original's `tv_dict_wrong_func_name` check (rejecting a
/// function-typed value added to the real `g:`/`l:` scope dict) - see
/// this module's own doc comment for why.
///
/// # Safety
/// `item` must be a valid, non-null pointer previously returned by
/// [`tv_dict_item_alloc`] (or equivalent), not already present in any
/// dictionary's hashtable.
pub unsafe fn tv_dict_add(d: &mut DictT, item: *mut DictitemT) -> i32 {
    // SAFETY: `di_key` is owned by `*item`, which the caller
    // guarantees outlives this hashtable entry (forwarded from this
    // function's own safety doc).
    let key_ptr = unsafe { (*item).di_key.as_mut_ptr() as *mut std::os::raw::c_char };
    // SAFETY: forwarded from this function's own safety doc.
    let rc = unsafe { d.dv_hashtab.hash_add(key_ptr) };
    if rc == OK {
        d.dv_index.insert(key_ptr as usize, item);
    }
    rc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vim_defs::FAIL;

    #[test]
    fn tv_dict_item_alloc_copies_key_and_nul_terminates() {
        let item = tv_dict_item_alloc(b"hello");
        unsafe {
            assert_eq!((*item).di_key, b"hello\0");
            assert_eq!((*item).di_flags, dict_item_flags::ALLOC);
            assert!(matches!((*item).di_tv.value, TypvalValue::Unknown));
            tv_dict_item_free(item);
        }
    }

    #[test]
    fn tv_dict_item_free_clears_in_place_when_not_separately_allocated() {
        let mut item = DictitemT {
            di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(42) },
            di_flags: 0, // NOT DI_FLAGS_ALLOC
            di_key: b"x\0".to_vec(),
        };
        unsafe { tv_dict_item_free(&mut item as *mut DictitemT) };
        assert!(matches!(item.di_tv.value, TypvalValue::Unknown));
        // The item itself (a plain stack value here) is untouched/
        // still valid to read - it was never `Box::from_raw`'d.
        assert_eq!(item.di_key, b"x\0");
    }

    #[test]
    fn tv_dict_alloc_and_free_round_trip() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!((*d).dv_refcount, 0);
            assert!((*d).dv_hashtab.hash_find(b"missing").hi_key.is_null());
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_then_find_roundtrip() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let item = tv_dict_item_alloc(b"greeting");
            (*item).di_tv.value = TypvalValue::Number(7);
            assert_eq!(tv_dict_add(&mut *d, item), OK);

            let found = tv_dict_find(Some(&mut *d), b"greeting");
            assert_eq!(found, Some(item));
            assert!(matches!((*found.unwrap()).di_tv.value, TypvalValue::Number(7)));

            assert!(tv_dict_has_key(Some(&mut *d), b"greeting"));
            assert!(!tv_dict_has_key(Some(&mut *d), b"nope"));

            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_duplicate_key_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let item1 = tv_dict_item_alloc(b"k");
            assert_eq!(tv_dict_add(&mut *d, item1), OK);

            let item2 = tv_dict_item_alloc(b"k");
            assert_eq!(tv_dict_add(&mut *d, item2), FAIL);
            // item2 was never added to the dict - free it directly to
            // avoid leaking it in this test.
            tv_dict_item_free(item2);

            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_find_returns_none_for_missing_key_and_none_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_find(Some(&mut *d), b"absent"), None);
            assert_eq!(tv_dict_find(None, b"absent"), None);
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_item_remove_removes_from_both_hashtab_and_index() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let item = tv_dict_item_alloc(b"temp");
            assert_eq!(tv_dict_add(&mut *d, item), OK);
            assert!(tv_dict_has_key(Some(&mut *d), b"temp"));

            tv_dict_item_remove(&mut *d, item);
            assert!(!tv_dict_has_key(Some(&mut *d), b"temp"));
            assert!((*d).dv_index.is_empty());

            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_free_contents_frees_every_item_and_resets_hashtab() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            for key in [b"a".as_slice(), b"b".as_slice(), b"c".as_slice()] {
                let item = tv_dict_item_alloc(key);
                assert_eq!(tv_dict_add(&mut *d, item), OK);
            }
            assert_eq!((*d).dv_index.len(), 3);

            tv_dict_free_contents(d);
            assert!((*d).dv_index.is_empty());
            assert!(!tv_dict_has_key(Some(&mut *d), b"a"));

            tv_dict_free_dict(d);
        }
    }

    #[test]
    #[should_panic(expected = "tv_list_unref/tv_dict_unref/tv_blob_unref/partial_unref")]
    fn tv_dict_item_free_panics_on_dict_valued_item() {
        let mut item = DictitemT {
            di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) },
            di_flags: 0,
            di_key: b"x\0".to_vec(),
        };
        unsafe { tv_dict_item_free(&mut item as *mut DictitemT) };
    }

    #[test]
    fn multiple_dicts_maintain_the_gc_linked_list_correctly() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = tv_dict_alloc();
        let d2 = tv_dict_alloc();
        let d3 = tv_dict_alloc();
        unsafe {
            // Most-recently-allocated dict is at the head.
            assert_eq!(*GC_FIRST_DICT.get_mut(), d3);
            assert_eq!((*d3).dv_used_next, d2);
            assert_eq!((*d2).dv_used_next, d1);
            assert!((*d1).dv_used_next.is_null());
            assert!((*d3).dv_used_prev.is_null());
            assert_eq!((*d2).dv_used_prev, d3);
            assert_eq!((*d1).dv_used_prev, d2);

            // Remove the middle one; the list should re-link around it.
            tv_dict_free(d2);
            assert_eq!((*d3).dv_used_next, d1);
            assert_eq!((*d1).dv_used_prev, d3);

            tv_dict_free(d3);
            tv_dict_free(d1);
            assert!((*GC_FIRST_DICT.get_mut()).is_null());
        }
    }
}
