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
//! Also translated: [`do_one_arg`] - isolates one whitespace-separated
//! (outside backtick-quoted spans) argument from a command-line-style
//! byte string, respecting backslash-escaped characters via the
//! already-real `charset::rem_backslash`. Needed only already-real
//! `charset::rem_backslash`/`skipwhite` and `ascii_defs::ascii_isspace`,
//! making it genuinely standalone even though its own real caller
//! (`get_arglist`, same file) needs `GA_APPEND`/real argument-list
//! building not yet translated. See its own doc comment for a real,
//! hand-traced observation about its apparent "compaction" never
//! actually shifting anything.
//!
//! Also translated: [`alist_new`] - allocates a fresh, empty argument
//! list and installs it as the current window's own `w_alist`, needing
//! only already-real `crate::globals::GLOBALS.curwin`/`max_alist_id`
//! and this file's own `alist_init`. Translated ahead of its own real
//! caller (`ex_args`'s `":arglocal"` handling, `exarg_T`-based Ex-
//! command dispatch, not translated) - faithfully does NOT release
//! any previous `w_alist` value itself, matching the original exactly
//! (verified via `ex_args`'s own real body: it always calls
//! `alist_unlink` on the old value itself, first, before ever calling
//! this function).
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
//! Deferred: everything else - `get_arglist`/`alist_expand`/`alist_add`/
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

/// Creates a new argument list and uses it for the current window
/// (`alist_new`). The original's own `xmalloc`-then-assign, with no
/// prior release of `curwin->w_alist` - callers are responsible for
/// that themselves first (matching `ex_args`'s own real call site,
/// which always calls `alist_unlink` on the previous value before
/// this function), so this function faithfully does the same: it
/// never touches the previous `w_alist` pointer at all.
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live [`crate::buffer_defs::WinT`].
pub unsafe fn alist_new() {
    let mut al = Box::new(AlistT { al_refcount: 1, ..AlistT::default() });
    // SAFETY: a plain field increment through one exclusive borrow.
    let new_id = {
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.max_alist_id += 1;
        g.max_alist_id
    };
    al.id = new_id;
    alist_init(&mut al);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*crate::globals::GLOBALS.get_mut().curwin).w_alist = Box::into_raw(al);
    }
}

/// Isolates one whitespace-separated (outside backtick-quoted spans)
/// argument from `str`, respecting backslash-escaped characters via
/// [`crate::charset::rem_backslash`], NUL-terminating it in place at
/// the position where it ends, and returning the byte offset in
/// `str` where the NEXT argument (if any) begins (`do_one_arg`).
///
/// Mirrors the original's own two-cursor (`p` write / read cursor)
/// structure exactly, even though - verified by hand-tracing every
/// branch before writing this - `p` and the read cursor always stay
/// numerically equal to each other throughout: every branch reads and
/// writes the SAME number of bytes (the "keep this backslash" branch
/// copies 2 bytes in, 2 bytes out; every other branch copies 1 byte
/// in, 1 byte out), so despite its "compaction" APPEARANCE this
/// function never actually shifts anything leftward for any input.
/// Kept structurally faithful to the original anyway (both cursors
/// present, not collapsed into one), matching this crate's own
/// literal-translation mandate rather than simplifying based on this
/// derived (if verified) observation.
///
/// Unlike the original's own `char *` return value (a pointer INTO
/// the same buffer), returns a plain byte offset - `str` already
/// carries its own bounds as a Rust slice, with no NUL terminator to
/// scan for (`str.len()` is the true end of the remaining input). The
/// final `*p = NUL` write is skipped when the write cursor lands
/// exactly at `str.len()` (the whole remaining input was consumed
/// with no break) - the original always has room for this because a
/// real C string keeps its own trailing NUL byte one past the last
/// real character; a Rust slice representing "exactly the meaningful
/// bytes, no more" has no such spare byte to (harmlessly) overwrite.
pub fn do_one_arg(str: &mut [u8]) -> usize {
    let mut inbacktick = false;
    let mut p = 0usize;
    let mut i = 0usize;

    while i < str.len() {
        // When the backslash is used for escaping the special
        // meaning of a character we need to keep it until wildcard
        // expansion.
        if crate::charset::rem_backslash(&str[i..]) {
            str[p] = str[i];
            p += 1;
            i += 1;
            str[p] = str[i];
            p += 1;
            i += 1;
        } else {
            // An item ends at a space not in backticks.
            if !inbacktick && crate::ascii_defs::ascii_isspace(i32::from(str[i])) {
                break;
            }
            if str[i] == b'`' {
                inbacktick = !inbacktick;
            }
            str[p] = str[i];
            p += 1;
            i += 1;
        }
    }

    let next = i + crate::charset::skipwhite(&str[i..]);
    if p < str.len() {
        str[p] = 0;
    }
    next
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

    #[test]
    fn alist_new_creates_a_fresh_empty_refcounted_arglist_for_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_curwin = g.curwin;
        g.curwin = &mut win as *mut crate::buffer_defs::WinT;

        unsafe { alist_new() };

        // Read the result back through `GLOBALS.curwin`'s OWN stored
        // pointer, never by touching `win` directly again - a direct
        // `win.w_alist` read here would freeze the pointer stored in
        // `GLOBALS.curwin` (a real Tree Borrows violation, confirmed
        // via `cargo miri test`: a later write through that same
        // stored pointer - e.g. from a second `alist_new()` call -
        // would then fail). Matches this crate's established "always
        // derive via the already-stored global pointer, never
        // independently from the local a second time" discipline.
        let al_ptr = unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_alist };
        assert!(!al_ptr.is_null());
        // SAFETY: `alist_new` just installed a real, `Box`-allocated
        // `AlistT` here.
        let al = unsafe { &*al_ptr };
        assert_eq!(al.al_refcount, 1);
        assert!(al.al_ga.is_empty());
        assert_eq!(al.al_ga.ga_itemsize, std::mem::size_of::<AentryT>() as i32);
        assert!(al.id > 0);

        // Clean up: free the allocated `AlistT` (this test's own
        // responsibility, matching `alist_unlink`'s "must have been
        // `Box::into_raw`-allocated" contract) and restore `curwin`.
        drop(unsafe { Box::from_raw(al_ptr) });
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn alist_new_assigns_a_fresh_monotonically_increasing_id_each_call() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_curwin = g.curwin;
        g.curwin = &mut win as *mut crate::buffer_defs::WinT;

        unsafe { alist_new() };
        // Always read through `GLOBALS.curwin`'s own stored pointer
        // (never `win.w_alist` directly) - see the sibling test above
        // for why a direct field read here would freeze the stored
        // pointer and break the SECOND `alist_new()` call's own
        // internal write below (confirmed via a real `cargo miri
        // test` failure before this fix).
        let first_al_ptr = unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_alist };
        let first_id = unsafe { (*first_al_ptr).id };

        unsafe { alist_new() };
        let second_al_ptr = unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_alist };
        let second_id = unsafe { (*second_al_ptr).id };

        assert!(second_id > first_id);

        drop(unsafe { Box::from_raw(first_al_ptr) });
        drop(unsafe { Box::from_raw(second_al_ptr) });
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn do_one_arg_single_word_consumes_the_whole_input() {
        let mut buf = b"foo".to_vec();
        let next = do_one_arg(&mut buf);
        assert_eq!(buf, b"foo");
        assert_eq!(next, 3);
    }

    #[test]
    fn do_one_arg_stops_at_an_unquoted_space_and_skips_it() {
        let mut buf = b"foo bar".to_vec();
        let next = do_one_arg(&mut buf);
        // The space at index 3 is overwritten with NUL, terminating
        // "foo" in place; "bar" (untouched) starts at index 4.
        assert_eq!(buf, b"foo\0bar");
        assert_eq!(next, 4);
    }

    #[test]
    fn do_one_arg_skips_multiple_spaces_between_arguments() {
        let mut buf = b"foo   bar".to_vec();
        let next = do_one_arg(&mut buf);
        assert_eq!(&buf[..3], b"foo");
        assert_eq!(buf[3], 0);
        assert_eq!(next, 6); // skipwhite skips all 3 spaces
        assert_eq!(&buf[next..], b"bar");
    }

    #[test]
    fn do_one_arg_backtick_quoted_span_protects_an_embedded_space() {
        let mut buf = b"`foo bar` baz".to_vec();
        let next = do_one_arg(&mut buf);
        // The whole backtick-quoted span (9 bytes: ` f o o ' ' b a r `)
        // is one argument; the space right after it breaks, and
        // skipwhite lands on "baz".
        assert_eq!(&buf[..9], b"`foo bar`");
        assert_eq!(buf[9], 0);
        assert_eq!(next, 10);
        assert_eq!(&buf[next..], b"baz");
    }

    #[test]
    fn do_one_arg_backslash_escaped_space_is_kept_and_does_not_break() {
        // "\<space>" is always kept (rem_backslash treats an escaped
        // space specially on every platform, not just non-Windows) -
        // so the whole thing is ONE argument, the embedded space is
        // NOT treated as a separator.
        let mut buf = b"foo\\ bar".to_vec();
        let next = do_one_arg(&mut buf);
        assert_eq!(buf, b"foo\\ bar");
        assert_eq!(next, 8);
    }

    #[test]
    fn do_one_arg_no_input_returns_zero() {
        let mut buf: Vec<u8> = Vec::new();
        let next = do_one_arg(&mut buf);
        assert_eq!(next, 0);
    }

    #[test]
    fn do_one_arg_leading_space_is_itself_the_immediate_break() {
        // A leading space breaks on the very first iteration (nothing
        // consumed into the argument at all) - next skips past it.
        let mut buf = b" foo".to_vec();
        let next = do_one_arg(&mut buf);
        assert_eq!(buf[0], 0);
        assert_eq!(next, 1);
        assert_eq!(&buf[next..], b"foo");
    }
}
