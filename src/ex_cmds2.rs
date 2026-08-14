//! Translated from `src/nvim/ex_cmds2.c` (tractable core only).
//!
//! `ex_cmds2.c` (~900 lines) implements a mix of the `:ruby`/
//! `:python3`/`:perl` script-host command stubs, the
//! buffer-abandon-checking family (`autowrite`/`can_abandon`/
//! `check_changed`/`check_changed_any`), `:compiler`/`:checktime`/
//! `:drop`/`:listdo`-family commands, and `buf_write_all` - almost
//! none of that is attempted here.
//!
//! Translated: `check_fname` (the original's own
//! `emsg(_(e_noname))` on failure is omitted - message display, not
//! tractable - the identical `FAIL` return is kept, matching this
//! crate's established policy), `autowrite`/`autowrite_all` (the
//! real, always-taken-today early-return fast path: `'autowrite'`/
//! `'autowriteall'` both default off, and nothing in this crate can
//! currently turn either on - there is no `:set` command parser yet -
//! so every real invocation today hits this exact fast path,
//! `unimplemented!()`s only if genuinely reached, needing
//! `buf_write_all` -> `buf_write`, real file-writing machinery from
//! `fileio.c`/`bufwrite.c`, not yet translated), and `can_abandon`
//! (fully real - composes `buf_hide`/`buf_is_changed`/`autowrite`,
//! all of which already exist).
//!
//! Deferred: `check_changed`/`check_changed_any` (need the real
//! confirmation-dialog subsystem for their own `'confirm'` branch),
//! `buf_write_all`/`buf_write` (real file writing),
//! `dialog_changed`/`dialog_close_terminal`, `ex_listdo`
//! (`:argdo`/`:windo`/`:bufdo`/etc.), `ex_compiler`/`ex_checktime`/
//! `ex_drop`, and the `:ruby`/`:python3`/`:perl` script-host stubs
//! (need the Lua/script-host integration, phase 13).

use crate::buffer_defs::BufT;
use crate::vim_defs::{FAIL, OK};

/// Append `nr` to `bufnrs` unless it is already present
/// (`add_bufnum`).
#[allow(dead_code)]
fn add_bufnum(bufnrs: &mut Vec<i32>, nr: i32) {
    if !bufnrs.contains(&nr) {
        bufnrs.push(nr);
    }
}

/// Check that `curbuf` has a file name (`check_fname`).
///
/// The original's own `emsg(_(e_noname))` on failure is omitted
/// (message display, not tractable) - the identical `FAIL` return is
/// kept, matching this crate's established policy.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn check_fname() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    if curbuf.b_ffname.is_none() { FAIL } else { OK }
}

/// If `'autowrite'`/`'autowriteall'` is set, try to write `buf`
/// (`autowrite`).
///
/// Only the real, always-taken-today early-return fast path is
/// translated: `'autowrite'`/`'autowriteall'` both default to off,
/// and nothing in this crate can currently turn either on (no `:set`
/// command parser yet) - every real invocation today hits this exact
/// early `FAIL` return, matching the original's own behavior for an
/// unconfigured session precisely, not an approximation.
/// `unimplemented!()`s only if genuinely reached (needs
/// `buf_write_all` -> `buf_write`, real file-writing machinery from
/// `fileio.c`/`bufwrite.c`, not yet translated).
///
/// # Safety
/// `buf` must be a valid, mutable reference to a live `BufT` - same
/// requirement as [`crate::undo::buf_is_changed`].
pub unsafe fn autowrite(buf: &mut BufT, forceit: bool) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    if !(ov.p_aw != 0 || ov.p_awa != 0)
        || ov.p_write == 0
        || crate::buffer::bt_dontwrite(Some(buf))
        || (!forceit && buf.b_p_ro != 0)
        || buf.b_ffname.is_none()
    {
        return FAIL;
    }
    unimplemented!(
        "autowrite: buf_write_all/buf_write not yet translated - unreachable \
         while 'autowrite'/'autowriteall' both default off and nothing can \
         currently set them"
    )
}

/// Flush all buffers, except the ones that are readonly or are never
/// written (`autowrite_all`).
///
/// Same fast-path reasoning as [`autowrite`]: `'autowrite'`/
/// `'autowriteall'` both default off, so this is a real, faithful
/// no-op for every session this crate can currently construct - the
/// per-buffer loop condition is still modeled precisely (not skipped)
/// so a future test/session that DOES set one of these options is
/// still handled faithfully (a genuinely-qualifying buffer
/// `unimplemented!()`s exactly like [`autowrite`] does, rather than
/// silently doing nothing).
///
/// # Safety
/// Walks the real `GLOBALS.firstbuf`/`b_next` linked list - same
/// requirement as [`crate::undo::any_buf_is_changed`].
pub unsafe fn autowrite_all() {
    // SAFETY: forwarded from this function's own safety doc.
    let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    if !(ov.p_aw != 0 || ov.p_awa != 0) || ov.p_write == 0 {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let mut bp = unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf;
    while !bp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let buf = unsafe { &mut *bp };
        let next = buf.b_next;
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::undo::buf_is_changed(buf) }
            && buf.b_p_ro == 0
            && !crate::buffer::bt_dontwrite(Some(buf))
        {
            unimplemented!(
                "autowrite_all: buf_write_all/buf_write not yet translated - \
                 unreachable while 'autowrite'/'autowriteall' both default \
                 off and nothing can currently set them"
            );
        }
        bp = next;
    }
}

/// Whether `buf` can be abandoned: hidden, unchanged, shown in more
/// than one window, successfully auto-written, or `forceit` is set
/// (`can_abandon`).
///
/// # Safety
/// `buf` must be a valid, mutable reference to a live `BufT` - same
/// requirement as [`autowrite`]/[`crate::undo::buf_is_changed`].
#[must_use]
pub unsafe fn can_abandon(buf: &mut BufT, forceit: bool) -> bool {
    // Short-circuit order matches the original's own `||` chain
    // exactly: `autowrite` must only be called (and only risk
    // `unimplemented!()`) when every earlier condition is false,
    // same as the original never calling it if the buffer is already
    // hidden/unchanged/shown-in-multiple-windows.
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::buffer::buf_hide(buf) } {
        return true;
    }
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { crate::undo::buf_is_changed(buf) } {
        return true;
    }
    if buf.b_nwindows > 1 {
        return true;
    }
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { autowrite(buf, forceit) } == OK {
        return true;
    }
    forceit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    fn reset_option_vars() {
        // SAFETY: single-threaded test, lock held by the caller.
        let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        ov.p_aw = 0;
        ov.p_awa = 0;
        ov.p_write = 1;
    }

    #[test]
    fn add_bufnum_preserves_first_seen_order_and_rejects_duplicates() {
        let mut numbers = vec![3, 1];
        add_bufnum(&mut numbers, 3);
        add_bufnum(&mut numbers, 2);
        add_bufnum(&mut numbers, 1);
        assert_eq!(numbers, vec![3, 1, 2]);
    }

    struct CurbufGuard {
        previous: *mut BufT,
    }

    impl CurbufGuard {
        fn set(buf: &mut BufT) -> Self {
            // SAFETY: single-threaded test, lock held by the caller.
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let previous = g.curbuf;
            g.curbuf = buf;
            CurbufGuard { previous }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            // SAFETY: restoring the previous value on drop.
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    #[test]
    fn check_fname_ok_when_curbuf_has_a_file_name() {
        let _lock = global_state_test_lock();
        let mut buf = BufT { b_ffname: Some(b"foo.txt".to_vec()), ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf);
        assert_eq!(unsafe { check_fname() }, OK);
    }

    #[test]
    fn check_fname_fail_when_curbuf_has_no_file_name() {
        let _lock = global_state_test_lock();
        let mut buf = BufT { b_ffname: None, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf);
        assert_eq!(unsafe { check_fname() }, FAIL);
    }

    #[test]
    fn autowrite_fails_fast_by_default_even_for_a_changed_writable_buffer() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        let mut buf = BufT {
            b_ffname: Some(b"foo.txt".to_vec()),
            b_changed: 1,
            ..Default::default()
        };
        assert_eq!(unsafe { autowrite(&mut buf, false) }, FAIL);
    }

    #[test]
    fn autowrite_fails_fast_when_buffer_has_no_file_name() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        // Force autowrite "on" - still fails fast, since there is no
        // file name at all (checked before ever reaching the
        // not-yet-translated real write path).
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_aw = 1;
        let mut buf = BufT { b_ffname: None, ..Default::default() };
        assert_eq!(unsafe { autowrite(&mut buf, false) }, FAIL);
    }

    #[test]
    fn autowrite_fails_fast_for_a_readonly_buffer_without_forceit() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_aw = 1;
        let mut buf =
            BufT { b_ffname: Some(b"foo.txt".to_vec()), b_p_ro: 1, ..Default::default() };
        assert_eq!(unsafe { autowrite(&mut buf, false) }, FAIL);
    }

    #[test]
    #[should_panic(expected = "autowrite: buf_write_all/buf_write not yet translated")]
    fn autowrite_panics_when_genuinely_configured_to_write() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_aw = 1;
        let mut buf = BufT { b_ffname: Some(b"foo.txt".to_vec()), ..Default::default() };
        unsafe { autowrite(&mut buf, false) };
    }

    #[test]
    fn autowrite_all_is_a_real_no_op_by_default() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        let mut buf = BufT {
            b_ffname: Some(b"foo.txt".to_vec()),
            b_changed: 1,
            b_next: std::ptr::null_mut(),
            ..Default::default()
        };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let previous = g.firstbuf;
        g.firstbuf = &mut buf;
        unsafe { autowrite_all() };
        unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf = previous;
    }

    #[test]
    fn autowrite_all_skips_readonly_and_dontwrite_buffers_without_panicking() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_aw = 1;
        let mut ro_buf = BufT {
            b_ffname: Some(b"ro.txt".to_vec()),
            b_changed: 1,
            b_p_ro: 1,
            b_next: std::ptr::null_mut(),
            ..Default::default()
        };
        let mut unchanged_buf = BufT {
            b_ffname: Some(b"unchanged.txt".to_vec()),
            b_changed: 0,
            b_next: &mut ro_buf,
            ..Default::default()
        };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let previous = g.firstbuf;
        g.firstbuf = &mut unchanged_buf;
        unsafe { autowrite_all() };
        unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf = previous;
    }

    #[test]
    fn autowrite_all_panics_for_a_genuinely_qualifying_buffer() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_aw = 1;
        let mut buf = BufT {
            b_ffname: Some(b"foo.txt".to_vec()),
            b_changed: 1,
            b_next: std::ptr::null_mut(),
            ..Default::default()
        };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let previous = g.firstbuf;
        g.firstbuf = &mut buf;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            autowrite_all();
        }));
        unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf = previous;
        // Manually inspect the panic payload rather than using
        // #[should_panic] - that attribute checks the FINAL panic
        // that unwinds the test thread, but this test's own
        // catch_unwind (needed to restore GLOBALS.firstbuf even on
        // panic) means any re-panic here would carry a DIFFERENT
        // message than the original one caught above.
        let err = result.expect_err("autowrite_all should have panicked");
        let msg = err
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| err.downcast_ref::<&str>().copied())
            .expect("panic payload should be a string");
        assert!(
            msg.contains("autowrite_all: buf_write_all/buf_write not yet translated"),
            "unexpected panic message: {msg}"
        );
    }

    #[test]
    fn can_abandon_true_when_buffer_can_be_hidden() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        // 'bufhidden' == "hide" makes buf_hide() true unconditionally.
        let mut buf = BufT {
            b_p_bh: Some(b"hide".to_vec()),
            b_changed: 1,
            ..Default::default()
        };
        assert!(unsafe { can_abandon(&mut buf, false) });
    }

    #[test]
    fn can_abandon_true_when_buffer_is_unchanged() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        let mut buf = BufT { b_changed: 0, ..Default::default() };
        assert!(unsafe { can_abandon(&mut buf, false) });
    }

    #[test]
    fn can_abandon_true_when_shown_in_more_than_one_window() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        let mut buf = BufT { b_changed: 1, b_nwindows: 2, ..Default::default() };
        assert!(unsafe { can_abandon(&mut buf, false) });
    }

    #[test]
    fn can_abandon_true_when_forceit_is_set() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        let mut buf = BufT { b_changed: 1, b_nwindows: 1, ..Default::default() };
        assert!(unsafe { can_abandon(&mut buf, true) });
    }

    #[test]
    fn can_abandon_false_for_a_genuinely_unabandonable_buffer() {
        let _lock = global_state_test_lock();
        reset_option_vars();
        let mut buf = BufT {
            b_changed: 1,
            b_nwindows: 1,
            b_ffname: Some(b"foo.txt".to_vec()),
            ..Default::default()
        };
        // autowrite() itself fails fast (p_aw/p_awa both off by
        // default), so this never reaches the not-yet-translated real
        // write path.
        assert!(!unsafe { can_abandon(&mut buf, false) });
    }
}
