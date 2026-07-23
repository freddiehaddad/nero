//! Translated from `src/nvim/eval/userfunc.c` (tractable core only).
//!
//! `userfunc.c` (~4300 lines) is the user-defined-function subsystem:
//! defining/parsing/calling/profiling Vimscript functions, `func_hashtab`
//! (the file-static registry of all named functions, keyed by
//! `uf_name` - not yet translated, will need its own `DictT.dv_index`-
//! style side table for `HIKEY2UF`'s pointer-arithmetic recovery when
//! it is), `funccall_T` construction/teardown, and so on - almost none
//! of that is attempted here.
//!
//! Translated: `func_ptr_ref`/`func_ptr_unref` - the two `ufunc_T`
//! reference-counting primitives that operate directly on a
//! `ufunc_T*`, needing neither `func_hashtab` nor a function *name*
//! (unlike their string-based siblings `func_ref`/`func_unref`, which
//! both need `find_func`/`func_name_refcount` - real registry lookups,
//! not translated).
//!
//! `func_ptr_unref`'s "reference count hit zero, and the function
//! isn't mid-call" branch (`func_clear_free`) is `unimplemented!()`:
//! it needs `funccal_unref` (`funccall_T`'s own real fields, itself
//! deferred - see `eval/typval_defs.rs`'s own module doc) and
//! `func_remove` (`func_hashtab`, also not translated). Nothing
//! translated so far can actually construct a real, positively-
//! refcounted `UfuncT` yet (no function-definition machinery exists),
//! so this branch is currently unreachable in practice, not just
//! narrow - matching the "harvest the reachable core, defer the exact
//! unreachable branch" pattern already used elsewhere in this crate
//! (e.g. `mark.c`'s `get_global_marks`).

use crate::eval::typval_defs::UfuncT;

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
                "func_ptr_unref: func_clear_free needs funccal_unref (funccall_T's real \
                 fields, not yet translated) and func_remove (func_hashtab, not yet \
                 translated) - see this module's own doc comment"
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
}
