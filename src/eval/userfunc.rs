//! Translated from `src/nvim/eval/userfunc.c` (tractable core only).
//!
//! `userfunc.c` (~4300 lines) is the user-defined-function subsystem:
//! defining/parsing/calling/profiling Vimscript functions, `funccall_T`
//! construction/teardown, and so on - almost none of that is attempted
//! here.
//!
//! Translated: `func_ptr_ref`/`func_ptr_unref` - the two `ufunc_T`
//! reference-counting primitives that operate directly on a
//! `ufunc_T*`.
//!
//! Also translated: `func_hashtab` itself (the file-static registry of
//! all named functions, keyed by `uf_name`), `func_init`,
//! `func_tbl_get`, `find_func`, `func_remove`, plus a NEW (not in the
//! original) [`func_hashtab_add`] helper - see `FuncHashtab`'s own
//! doc comment for the design (mirrors `DictT`'s already-established
//! `dv_hashtab`/`dv_index` pair, for exactly the same reason: `uf_name`
//! is an owned `Vec<u8>` here, not a true C flexible array member, so
//! `HIKEY2UF`'s pointer-arithmetic recovery has no safe Rust
//! equivalent).
//!
//! Also translated: `func_name_refcount`, `func_ref`/`func_unref` (the
//! string-based siblings of `func_ptr_ref`/`func_ptr_unref` - only
//! numbered functions (`"123"`) and lambdas (`"<lambda>42"`) are
//! genuinely refcounted BY NAME; ordinary named functions aren't, so
//! these are usually no-ops). Both `name` parameters are bare bytes
//! with NO trailing NUL (matching `find_func`'s own convention, NOT
//! `UfuncT.uf_name`'s NUL-terminated storage form - callers reading a
//! name out of `uf_name` must strip the trailing NUL first, exactly
//! like `eval/typval.rs`'s `tv_dict_item_remove` already does for the
//! analogous `DictitemT.di_key` case). The original's
//! `internal_error("func_ref()")`/`internal_error("func_unref()")` -
//! reached only when a NUMBERED function reference has no backing
//! `UfuncT`, a genuine "should never happen" condition - become
//! `debug_assert!`s, matching this crate's established policy for
//! internal invariant violations (e.g. `mf_put`). The original's
//! further `EXITFREE`-build/`entered_free_all_mem`-shutdown carve-out
//! (which downgrades `func_unref`'s case from an abort to a silent
//! no-op specifically during process teardown) has no equivalent here
//! - this crate has no such shutdown-tracking state - so it's moot.
//!
//! `func_ptr_unref`'s "reference count hit zero, and the function
//! isn't mid-call" branch (`func_clear_free`) is `unimplemented!()`:
//! it still needs `funccal_unref` (needs `previous_funccal`'s
//! file-static list + `fc_referenced`'s own reachability algorithm,
//! neither translated - `funccall_T`'s real fields alone are not
//! enough). Nothing translated so far can actually construct a real,
//! positively-refcounted `UfuncT` yet (no function-definition
//! machinery exists), so this branch is currently unreachable in
//! practice, not just narrow - matching the "harvest the reachable
//! core, defer the exact unreachable branch" pattern already used
//! elsewhere in this crate (e.g. `mark.c`'s `get_global_marks`).

use crate::ascii_defs::ascii_isdigit;
use crate::eval::typval_defs::UfuncT;
use crate::globals::GlobalCell;
use crate::hashtab::hashitem_empty;
use crate::hashtab_defs::HashtabT;
use crate::vim_defs::OK;
use std::collections::HashMap;
use std::sync::LazyLock;

/// The function hash table (`func_hashtab`) plus this crate's own side
/// index (`index`) - mirroring `DictT`'s `dv_hashtab`/`dv_index` pair
/// (`eval/typval_defs.rs`), for exactly the same reason: `UfuncT.uf_name`
/// is an owned `Vec<u8>` here (not a true C flexible array member), so
/// `HIKEY2UF`'s `hi_key - offsetof(ufunc_T, uf_name)` pointer-arithmetic
/// recovery has no safe Rust equivalent. `index` maps each live item's
/// `hi_key` address (the `uf_name` buffer's own pointer, as a `usize`)
/// to its owning `*mut UfuncT`, populated/depopulated in lockstep with
/// `ht` by [`func_hashtab_add`]/[`func_remove`] - never read/written
/// independently of `ht`, exactly like `dv_index`.
///
/// `uf_name` must be NUL-terminated for any `UfuncT` added here,
/// matching `DictitemT.di_key`'s own established convention (`hi_key`
/// is ultimately read back as a C string by `hashtab.rs`'s internals).
struct FuncHashtab {
    ht: HashtabT,
    index: HashMap<usize, *mut UfuncT>,
}

impl Default for FuncHashtab {
    fn default() -> Self {
        FuncHashtab { ht: HashtabT::hash_init(), index: HashMap::new() }
    }
}

/// `func_hashtab` itself - a file-static in the original, kept private
/// here too (only reachable through this module's own `pub fn`s,
/// matching the encapsulation boundary the original itself draws via
/// `func_tbl_get()`).
static FUNC_HASHTAB: LazyLock<GlobalCell<FuncHashtab>> =
    LazyLock::new(|| GlobalCell::new(FuncHashtab::default()));

/// (Re-)initialize the function hash table (`func_init`).
pub fn func_init() {
    *unsafe { FUNC_HASHTAB.get_mut() } = FuncHashtab::default();
}

/// Return the function hash table (`func_tbl_get`).
pub fn func_tbl_get() -> *mut HashtabT {
    unsafe { &mut FUNC_HASHTAB.get_mut().ht as *mut HashtabT }
}

/// Add `fp` to the function hash table, keyed by its own `uf_name`.
///
/// NEW: not a separate function in the original, which inlines
/// `hash_add(&func_hashtab, UF2HIKEY(fp))` directly at its 3 real call
/// sites (inside `get_lambda_tv`/user-function-definition/lua-function
/// registration code, none of which are translated yet). Factored out
/// here so `ht`/`FuncHashtab::index` are always updated together,
/// atomically - exactly the same reasoning that already justifies
/// `dv_index` existing at all, and why `tv_dict_add` wraps `hash_add`
/// rather than leaving every caller to call `hash_add` directly.
///
/// Returns `FAIL` if a function with the same name already exists
/// (matching [`HashtabT::hash_add`]'s own contract), `OK` otherwise.
///
/// # Safety
/// `fp` must be a valid, non-null pointer to a live [`UfuncT`] whose
/// `uf_name` is NUL-terminated, outliving this hashtable entry, and
/// not already present in this table.
pub unsafe fn func_hashtab_add(fp: *mut UfuncT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let key_ptr = unsafe { (*fp).uf_name.as_mut_ptr() as *mut std::os::raw::c_char };
    let table = unsafe { FUNC_HASHTAB.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let rc = unsafe { table.ht.hash_add(key_ptr) };
    if rc == OK {
        table.index.insert(key_ptr as usize, fp);
    }
    rc
}

/// Find a function by name, return pointer to it in ufuncs
/// (`find_func`).
///
/// Returns a null pointer for an unknown function.
#[must_use]
pub fn find_func(name: &[u8]) -> *mut UfuncT {
    let table = unsafe { FUNC_HASHTAB.get_mut() };
    let hi = table.ht.hash_find(name);
    if hashitem_empty(hi) {
        return std::ptr::null_mut();
    }
    table
        .index
        .get(&(hi.hi_key as usize))
        .copied()
        .unwrap_or(std::ptr::null_mut())
}

/// Remove the function from the function hash table. If the function
/// was deleted while it still has references this was already done
/// (`func_remove`).
///
/// Returns `true` if the entry was deleted, `false` if it wasn't
/// found.
///
/// # Safety
/// `fp` must be a valid, non-null pointer to a live [`UfuncT`]
/// previously added via [`func_hashtab_add`] (or not present at all,
/// in which case this is a no-op returning `false`).
pub unsafe fn func_remove(fp: *mut UfuncT) -> bool {
    let table = unsafe { FUNC_HASHTAB.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let name: &[u8] = unsafe { &(*fp).uf_name };
    // hash_find/hash_remove take the bare key bytes (no trailing NUL) -
    // strip it, mirroring tv_dict_item_remove's own established
    // convention for the same reason.
    let name = &name[..name.len().saturating_sub(1)];
    let key_ptr = {
        let hi = table.ht.hash_find(name);
        if hashitem_empty(hi) {
            return false;
        }
        hi.hi_key as usize
    };
    table.ht.hash_remove(name);
    table.index.remove(&key_ptr);
    true
}

/// Whether a function name is genuinely refcounted BY NAME
/// (`func_name_refcount`): numbered functions (`"123"`) and lambdas
/// (`"<lambda>42"`) are; an ordinary named function's `ufunc_T` lives
/// for the whole script's lifetime once defined, so it isn't.
fn func_name_refcount(name: &[u8]) -> bool {
    match name.first() {
        Some(&b) => ascii_isdigit(b as i32) || (b == b'<' && name.get(1) == Some(&b'l')),
        None => false,
    }
}

/// Count a reference to a Function (`func_ref`) - the string-based
/// sibling of [`func_ptr_ref`]. See this module's own doc comment for
/// `name`'s expected bare-bytes form and the `debug_assert!`
/// translation policy for the original's internal-error report.
pub fn func_ref(name: Option<&[u8]>) {
    let Some(name) = name else { return };
    if !func_name_refcount(name) {
        return;
    }
    let fp = find_func(name);
    if !fp.is_null() {
        // SAFETY: `find_func` only ever returns a pointer it looked up
        // from `FUNC_HASHTAB`'s own table, or null.
        unsafe { func_ptr_ref(fp) };
    } else {
        debug_assert!(
            !ascii_isdigit(name[0] as i32),
            "func_ref: numbered function not found - internal error (original: func_ref())"
        );
    }
}

/// Unreference a Function (`func_unref`) - the string-based sibling of
/// [`func_ptr_unref`]. See this module's own doc comment for `name`'s
/// expected bare-bytes form and the `debug_assert!` translation policy
/// for the original's internal-error report.
pub fn func_unref(name: Option<&[u8]>) {
    let Some(name) = name else { return };
    if !func_name_refcount(name) {
        return;
    }
    let fp = find_func(name);
    debug_assert!(
        !(fp.is_null() && ascii_isdigit(name[0] as i32)),
        "func_unref: numbered function not found - internal error (original: func_unref())"
    );
    // SAFETY: `fp` is either null (func_ptr_unref's own null check
    // handles this, matching the original's own unconditional call)
    // or a valid pointer just looked up from `FUNC_HASHTAB`'s table.
    unsafe { func_ptr_unref(fp) };
}

/// Count a reference to a function (`func_ptr_ref`).
///
/// # Safety
/// `fp`, if non-null, must be a valid pointer to a live `UfuncT`.
pub unsafe fn func_ptr_ref(fp: *mut UfuncT) {
    if !fp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*fp).uf_refcount += 1 };
    }
}

/// Unreference a function: decrement the reference count and free it
/// when it becomes zero (and the function isn't mid-call)
/// (`func_ptr_unref`).
///
/// # Deferred
/// The "reference count hit zero, and `uf_calls == 0`" branch
/// (`func_clear_free`) is `unimplemented!()` - see this module's own
/// doc comment for why.
///
/// # Safety
/// `fp`, if non-null, must be a valid pointer to a live `UfuncT`.
pub unsafe fn func_ptr_unref(fp: *mut UfuncT) {
    if fp.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*fp).uf_refcount -= 1 };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*fp).uf_refcount } <= 0 {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { (*fp).uf_calls } == 0 {
            unimplemented!(
                "func_ptr_unref: func_clear_free needs funccal_unref (needs \
                 previous_funccal's file-static list + fc_referenced's reachability \
                 algorithm, neither translated) - see this module's own doc comment"
            );
        }
        // Otherwise: still being called (`uf_calls != 0`) - freed
        // later when `uf_calls` becomes zero, matching the original's
        // own comment exactly. Nothing more to do here.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vim_defs::FAIL;

    #[test]
    fn func_ptr_ref_null_is_noop() {
        unsafe { func_ptr_ref(std::ptr::null_mut()) };
    }

    #[test]
    fn func_ptr_ref_increments_refcount() {
        let mut fp = UfuncT { uf_refcount: 1, ..Default::default() };
        unsafe { func_ptr_ref(&mut fp as *mut UfuncT) };
        assert_eq!(fp.uf_refcount, 2);
    }

    #[test]
    fn func_ptr_unref_null_is_noop() {
        unsafe { func_ptr_unref(std::ptr::null_mut()) };
    }

    #[test]
    fn func_ptr_unref_decrements_without_freeing_when_still_referenced() {
        let mut fp = UfuncT { uf_refcount: 2, ..Default::default() };
        unsafe { func_ptr_unref(&mut fp as *mut UfuncT) };
        assert_eq!(fp.uf_refcount, 1);
    }

    #[test]
    fn func_ptr_unref_noop_when_hits_zero_but_still_being_called() {
        let mut fp = UfuncT { uf_refcount: 1, uf_calls: 3, ..Default::default() };
        // Refcount hits 0, but uf_calls != 0 - matches the original's
        // "freed later when uf_calls becomes zero" comment, so this
        // must NOT hit the unimplemented!() branch.
        unsafe { func_ptr_unref(&mut fp as *mut UfuncT) };
        assert_eq!(fp.uf_refcount, 0);
        assert_eq!(fp.uf_calls, 3);
    }

    #[test]
    #[should_panic(expected = "func_clear_free needs funccal_unref")]
    fn func_ptr_unref_panics_when_hits_zero_and_not_being_called() {
        let mut fp = UfuncT { uf_refcount: 1, uf_calls: 0, ..Default::default() };
        unsafe { func_ptr_unref(&mut fp as *mut UfuncT) };
    }

    // The following tests all touch the shared FUNC_HASHTAB GlobalCell -
    // each acquires global_state_test_lock() for its whole body and
    // calls func_init() first, so leftover entries from a previous test
    // (or a concurrently-running one, absent the lock) can't leak in.

    #[test]
    fn func_init_starts_with_an_empty_table() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        assert!(find_func(b"anything").is_null());
    }

    #[test]
    fn func_hashtab_add_then_find_func_round_trips() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"MyFunc\0".to_vec(), ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        let rc = unsafe { func_hashtab_add(fp_ptr) };
        assert_eq!(rc, OK);
        let found = find_func(b"MyFunc");
        assert_eq!(found, fp_ptr);
    }

    #[test]
    fn find_func_returns_null_for_unknown_name() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"Known\0".to_vec(), ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        unsafe { func_hashtab_add(fp_ptr) };
        assert!(find_func(b"Unknown").is_null());
    }

    #[test]
    fn func_hashtab_add_rejects_duplicate_name() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp1 = Box::new(UfuncT { uf_name: b"Dup\0".to_vec(), ..Default::default() });
        let mut fp2 = Box::new(UfuncT { uf_name: b"Dup\0".to_vec(), ..Default::default() });
        let rc1 = unsafe { func_hashtab_add(fp1.as_mut() as *mut UfuncT) };
        let rc2 = unsafe { func_hashtab_add(fp2.as_mut() as *mut UfuncT) };
        assert_eq!(rc1, OK);
        assert_eq!(rc2, FAIL);
    }

    #[test]
    fn func_remove_deletes_a_present_entry_and_reports_true() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"Removable\0".to_vec(), ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        unsafe { func_hashtab_add(fp_ptr) };
        assert!(!find_func(b"Removable").is_null());
        let removed = unsafe { func_remove(fp_ptr) };
        assert!(removed);
        assert!(find_func(b"Removable").is_null());
    }

    #[test]
    fn func_remove_reports_false_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"NeverAdded\0".to_vec(), ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        let removed = unsafe { func_remove(fp_ptr) };
        assert!(!removed);
    }

    #[test]
    fn func_tbl_get_returns_a_usable_pointer_reflecting_ht_used() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"Counted\0".to_vec(), ..Default::default() });
        unsafe { func_hashtab_add(fp.as_mut() as *mut UfuncT) };
        let ht_ptr = func_tbl_get();
        assert!(!ht_ptr.is_null());
        assert_eq!(unsafe { (*ht_ptr).ht_used }, 1);
    }

    #[test]
    fn func_name_refcount_true_for_numbered_and_lambda_names() {
        assert!(func_name_refcount(b"123"));
        assert!(func_name_refcount(b"<lambda>42"));
    }

    #[test]
    fn func_name_refcount_false_for_ordinary_names_and_empty() {
        assert!(!func_name_refcount(b"MyFunc"));
        assert!(!func_name_refcount(b""));
        // '<' alone, without a following 'l', doesn't count either.
        assert!(!func_name_refcount(b"<SNR>1_Foo"));
    }

    #[test]
    fn func_ref_none_is_noop() {
        func_ref(None);
    }

    #[test]
    fn func_ref_ordinary_name_is_noop_even_if_unregistered() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        // "MyFunc" isn't refcounted by name at all - must return
        // immediately without ever calling find_func/debug_assert!.
        func_ref(Some(b"MyFunc"));
    }

    #[test]
    fn func_ref_increments_refcount_for_a_registered_numbered_function() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"42\0".to_vec(), uf_refcount: 1, ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        unsafe { func_hashtab_add(fp_ptr) };
        func_ref(Some(b"42"));
        assert_eq!(fp.uf_refcount, 2);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "func_ref: numbered function not found")]
    fn func_ref_panics_for_unregistered_numbered_function() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        func_ref(Some(b"999"));
    }

    #[test]
    fn func_unref_none_is_noop() {
        func_unref(None);
    }

    #[test]
    fn func_unref_ordinary_name_is_noop_even_if_unregistered() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        func_unref(Some(b"MyFunc"));
    }

    #[test]
    fn func_unref_decrements_refcount_for_a_registered_lambda() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp =
            Box::new(UfuncT { uf_name: b"<lambda>1\0".to_vec(), uf_refcount: 2, ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        unsafe { func_hashtab_add(fp_ptr) };
        func_unref(Some(b"<lambda>1"));
        assert_eq!(fp.uf_refcount, 1);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "func_unref: numbered function not found")]
    fn func_unref_panics_for_unregistered_numbered_function() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        func_unref(Some(b"999"));
    }
}
