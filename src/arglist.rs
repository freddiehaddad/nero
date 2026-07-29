//! Translated from `src/nvim/arglist.c` (tractable core only).
//!
//! `arglist.c` (~1100 lines) is neovim's argument-list (`:args`,
//! `:argadd`, `:next`, etc.) management file - almost entirely dependent
//! on buffer-list/window/file-expansion infrastructure (`buflist_new`,
//! real path expansion, window switching for `:all`), none translated.
//!
//! Translated: [`alist_clear`]/[`alist_init`]/[`alist_unlink`] - the
//! small, self-contained struct-lifecycle trio operating on an
//! [`AlistT`]'s own [`crate::garray_defs::GarrayT`]-backed storage
//! (already-translated `GarrayT::ga_clear`/`ga_init` methods do the real
//! work), plus the file-static `arglist_locked` guard
//! (`check_arglist_locked`) they and future arglist-mutating functions
//! share.
//!
//! `al_ga` is stored (see `arglist_defs.rs`) as a byte-erased `GarrayT`,
//! matching the original's own generic growarray machinery, with no
//! typed accessor yet for reading/writing real `AentryT` bytes through
//! it (a gap already documented at `eval/funcs.rs`'s own `f_argv`/
//! `get_arglist_as_rettv`, which `unimplemented!()` on that exact case) -
//! so nothing in this crate can currently place a real `AentryT`'s bytes
//! into `al_ga` in the first place. This means the original's
//! `GA_DEEP_CLEAR` (which individually frees each entry's own `ae_fname`
//! before resetting the growarray) has no observable per-item work to
//! do for any `al_ga` this crate can actually construct today - calling
//! `ga_clear` alone is behaviorally equivalent.
//!
//! Deferred: everything else - `alist_new`/`alist_expand`/`alist_add`/
//! `alist_set`/`get_arglist_exp`/`set_arglist`/`do_arglist`/`ex_args`/
//! `ex_next`/`ex_previous`/`ex_argument`/`ex_all`, all needing real
//! buffer/window/path-expansion machinery.

use crate::arglist_defs::{AentryT, AlistT};
use crate::globals::GlobalCell;

/// This flag is set whenever the argument list is being changed and
/// calling a function that might trigger an autocommand
/// (`arglist_locked`).
static ARGLIST_LOCKED: GlobalCell<bool> = GlobalCell::new(false);

/// `Ok(())` if the argument list may be modified right now, `Err(())` if
/// it is currently locked (`check_arglist_locked`, `FAIL`/`OK` in the
/// original). Omits the original's own
/// `emsg(_(e_cannot_change_arglist_recursively))` display, matching this
/// crate's established "skip the deferred message-display side effect,
/// keep the exact same pass/fail outcome" policy (e.g.
/// `window::check_split_disallowed`).
fn check_arglist_locked() -> Result<(), ()> {
    // SAFETY: a plain read through one exclusive borrow.
    if unsafe { *ARGLIST_LOCKED.get_mut() } {
        return Err(());
    }
    Ok(())
}

/// Clears an argument list: frees all file names and resets it to zero
/// entries (`alist_clear`). A no-op while the argument list is currently
/// locked (see this module's own doc comment for why no per-item
/// `AentryT` freeing is needed here today).
pub fn alist_clear(al: &mut AlistT) {
    if check_arglist_locked().is_err() {
        return;
    }
    al.al_ga.ga_clear();
}

/// Initializes an argument list's growarray for [`AentryT`] items,
/// 5 at a time (`alist_init`).
pub fn alist_init(al: &mut AlistT) {
    al.al_ga.ga_init(std::mem::size_of::<AentryT>() as i32, 5);
}

/// Removes a reference from an argument list. Ignored when `al` is the
/// global argument list. If the argument list is no longer used by any
/// window, clears and frees it (`alist_unlink`).
///
/// # Safety
/// `al` must be a valid, non-null pointer to a live [`AlistT`]. Unless
/// it is pointer-equal to the crate's global argument list
/// (`&raw mut GLOBALS.get_mut().global_alist`, via
/// [`crate::globals::GlobalCell::as_ptr`]), it must have been allocated
/// via `Box::into_raw` (matching the original's own `xmalloc`), since
/// this function may free it via `Box::from_raw` - matching this
/// crate's established `xmalloc`-as-`Box::into_raw`/`xfree`-as-
/// `Box::from_raw` convention (e.g. `undo::UHeader`, `marktree`'s node
/// allocator).
pub unsafe fn alist_unlink(al: *mut AlistT) {
    let global_alist_ptr = {
        // SAFETY: never dereferenced as a reference, only compared and
        // used to derive a field pointer - matching `GlobalCell::as_ptr`'s
        // own intended "stable pointer without an intermediate &mut"
        // usage.
        let globals_ptr = crate::globals::GLOBALS.as_ptr();
        unsafe { std::ptr::addr_of_mut!((*globals_ptr).global_alist) }
    };

    if al == global_alist_ptr {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc - `al` is a
    // valid, non-null, live `AlistT`.
    let al_ref = unsafe { &mut *al };
    al_ref.al_refcount -= 1;
    if al_ref.al_refcount <= 0 {
        alist_clear(al_ref);
        // SAFETY: `al` is not the global arglist (checked above), so per
        // this function's own safety doc it must have been allocated via
        // `Box::into_raw`.
        drop(unsafe { Box::from_raw(al) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RAII guard resetting `ARGLIST_LOCKED` to its default (`false`)
    /// when dropped, so a test that locks it can't leak that state into
    /// later tests even on an assertion panic.
    struct ArglistLockedGuard;

    impl Drop for ArglistLockedGuard {
        fn drop(&mut self) {
            unsafe { *ARGLIST_LOCKED.get_mut() = false };
        }
    }

    fn lock_arglist() -> ArglistLockedGuard {
        unsafe { *ARGLIST_LOCKED.get_mut() = true };
        ArglistLockedGuard
    }

    #[test]
    fn alist_init_sets_up_the_growarray() {
        let _lock = crate::globals::global_state_test_lock();
        let mut al = AlistT::default();
        alist_init(&mut al);
        assert_eq!(al.al_ga.ga_itemsize, std::mem::size_of::<AentryT>() as i32);
        assert_eq!(al.al_ga.ga_growsize, 5);
        assert!(al.al_ga.is_empty());
    }

    #[test]
    fn alist_clear_resets_a_nonempty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut al = AlistT::default();
        alist_init(&mut al);
        al.al_ga.ga_len = 3;
        alist_clear(&mut al);
        assert!(al.al_ga.is_empty());
        // ga_clear preserves itemsize/growsize (only resets len/data).
        assert_eq!(al.al_ga.ga_itemsize, std::mem::size_of::<AentryT>() as i32);
    }

    #[test]
    fn alist_clear_is_a_no_op_when_locked() {
        let _lock = crate::globals::global_state_test_lock();
        let mut al = AlistT::default();
        alist_init(&mut al);
        al.al_ga.ga_len = 3;
        let _guard = lock_arglist();
        alist_clear(&mut al);
        // Unchanged: the lock check returned early before ga_clear.
        assert_eq!(al.al_ga.ga_len, 3);
    }

    #[test]
    fn check_arglist_locked_reflects_the_flag() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(check_arglist_locked().is_ok());
        let _guard = lock_arglist();
        assert!(check_arglist_locked().is_err());
    }

    #[test]
    fn alist_unlink_ignores_the_global_arglist() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.global_alist.al_refcount = 1;
        let global_ptr = unsafe {
            std::ptr::addr_of_mut!((*crate::globals::GLOBALS.as_ptr()).global_alist)
        };
        // Must not decrement, clear, or free the global arglist.
        unsafe { alist_unlink(global_ptr) };
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.global_alist.al_refcount,
            1
        );
    }

    #[test]
    fn alist_unlink_frees_when_refcount_drops_to_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let al = Box::into_raw(Box::new(AlistT {
            al_refcount: 1,
            ..Default::default()
        }));
        // refcount 1 -> 0: freed. If this leaked or double-freed, a
        // Miri/ASan run would catch it - the pointer is not touched
        // again after this call.
        unsafe { alist_unlink(al) };
    }

    #[test]
    fn alist_unlink_only_decrements_when_still_referenced() {
        let _lock = crate::globals::global_state_test_lock();
        let mut al = AlistT {
            al_refcount: 2,
            ..Default::default()
        };
        let al_ptr = &mut al as *mut AlistT;
        unsafe { alist_unlink(al_ptr) };
        assert_eq!(al.al_refcount, 1);
    }
}
