//! Translated from `src/nvim/change.c` (partial).
//!
//! `change.c` (~2200 lines) is the buffer-modification/change-tracking
//! core (`changed`/`changed_bytes`/`changed_lines`, insert-mode byte
//! insertion, indent-preservation helpers, etc.). Re-examined after
//! `memline.c`'s write side (`ml_replace`/`ml_append`/`ml_delete`) and
//! `autocmd.c`'s `apply_autocmds` (real, faithful "no autocmds
//! registered" bypass path) were both completed - `change_warning` is
//! now tractable too, since `autocmd_busy` is a real, always-`false`
//! global (see `crate::autocmd::AUTOCMD_BUSY`'s own doc comment) and
//! `apply_autocmds` itself is real. `changed`/`changed_internal`/
//! `changed_common`/`changed_lines_invalidate_win` (etc.) still need a
//! wide spread of OTHER not-yet-translated subsystems though:
//! `ml_open_file` (swap-file creation), window/fold display
//! bookkeeping (`redraw_buf_status_later`, `find_wl_entry`,
//! `invalidate_botline_win`, `buf_meta_total`), `diff_internal`/
//! `diff_update_line` (`diff.c`), and `buf_inc_changedtick` (the real
//! `b:` dict watcher machinery, eval engine/phase 5).
//!
//! Translated here: `file_ff_differs` (needed by `undo.c`'s
//! `bufIsChanged`) and `change_warning` (needed a real `apply_autocmds`
//! plus `autocmd_busy`, both now available). `change_warning`'s own
//! real message display (`msg_start`/`msg_source`/`msg_puts_hl`/
//! `msg_clr_eos`/`msg_end`/`msg_delay`/`showmode`) is skipped -
//! `message.c`'s display pipeline is not yet tractable - but every
//! OTHER observable state change is kept faithfully, including
//! `set_vim_var_string(VV_WARNINGMSG, ...)`: unlike `evalvars_init`
//! (the full `v:` scope-dict-wiring bootstrap, still not translated),
//! `set_vim_var_string` itself only writes directly to the `VIMVARS`
//! storage slot (`crate::eval::vars::VIMVARS[idx].tv`), which is real
//! and requires no dict/hashtable wiring at all - confirmed by reading
//! its own body before wiring this call in for real.
//!
//! Deferred: everything else in the file - each is its own substantial
//! undertaking blocked on subsystems not yet translated (the display
//! pipeline, the fold/diff subsystems, the eval engine's `b:` dict
//! watchers, etc. - see above).

use crate::buffer_defs::{b_flags, BufT};

/// Return true if `'fileformat'` and/or `'fileencoding'` has a
/// different value from when editing started (`save_file_ff()`
/// called). Also true when `'endofline'` was changed and `'binary'`
/// is set, or when `'bomb'` was changed and `'binary'` is not set.
/// Also true when `'endofline'` was changed and `'fixeol'` is not set.
/// When `ignore_empty` is true, don't consider a new, empty buffer to
/// be changed (`file_ff_differs`).
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT` (touched transitively via `crate::memline::ml_get_buf`).
#[must_use]
pub unsafe fn file_ff_differs(buf: &mut BufT, ignore_empty: bool) -> bool {
    // In a buffer that was never loaded the options are not valid.
    if buf.b_flags & (b_flags::BF_NEVERLOADED as i32) != 0 {
        return false;
    }
    if ignore_empty
        && buf.b_flags & (b_flags::BF_NEW as i32) != 0
        && buf.b_ml.ml_line_count == 1
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { crate::memline::ml_get_buf(buf, 1) }.first() == Some(&0)
    {
        return false;
    }
    let ff_first_byte = buf.b_p_ff.as_deref().and_then(<[u8]>::first).copied().unwrap_or(0);
    if buf.b_start_ffc != i32::from(ff_first_byte) {
        return true;
    }
    if (buf.b_p_bin != 0 || buf.b_p_fixeol == 0)
        && (buf.b_start_eof != buf.b_p_eof || buf.b_start_eol != buf.b_p_eol)
    {
        return true;
    }
    if buf.b_p_bin == 0 && buf.b_start_bomb != buf.b_p_bomb {
        return true;
    }
    let Some(start_fenc) = &buf.b_start_fenc else {
        return buf.b_p_fenc.as_deref().is_some_and(|s| !s.is_empty());
    };
    buf.b_p_fenc.as_deref().unwrap_or(b"") != start_fenc.as_slice()
}

/// If the file is readonly, give a warning message with the first
/// change. Don't do this for autocommands. Doesn't use `emsg()`,
/// because it flushes the macro buffer. If we have undone all changes
/// `b_changed` will be false, but `b_did_warn` will be true. `col` is
/// the column for the message; non-zero when in insert mode and
/// `'showmode'` is on.
///
/// Careful: may trigger autocommands that reload the buffer
/// (`change_warning`).
///
/// The real message display (`msg_start`/`msg_source`/`msg_puts_hl`/
/// `msg_clr_eos`/`msg_end`/`msg_delay`/`showmode`) is skipped -
/// `message.c`'s display pipeline is not yet tractable - but every
/// OTHER observable state change is kept: `apply_autocmds` is called
/// for real (currently always a no-op today - see
/// `crate::autocmd`'s own module doc), `v:warningmsg` is set for real
/// via `set_vim_var_string` (it only touches the `VIMVARS` storage
/// slot directly, no `evalvars_init` dict-wiring needed), and
/// `buf.b_did_warn`/`GLOBALS.redraw_cmdline` are still set exactly as
/// the original does.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` (touched transitively via `curbuf_is_changed`).
pub unsafe fn change_warning(buf: &mut BufT, _col: i32) {
    // Note this checks the GLOBAL curbuf's changed status, NOT `buf`'s
    // own - matching the original's own `curbufIsChanged()` call
    // exactly (every real call site happens to pass `buf == curbuf`,
    // but this is not assumed/simplified away here).
    // SAFETY: forwarded from this function's own safety doc.
    if !buf.b_did_warn
        && !unsafe { crate::undo::curbuf_is_changed() }
        && !unsafe { *crate::autocmd::AUTOCMD_BUSY.get_mut() }
        && buf.b_p_ro != 0
    {
        buf.b_ro_locked += 1;
        let _ = crate::autocmd::apply_autocmds(
            crate::autocmd_defs::EventT::FileChangedRO,
            None,
            None,
            false,
            Some(&*buf),
        );
        buf.b_ro_locked -= 1;
        if buf.b_p_ro == 0 {
            return;
        }

        // Real message display is skipped - see this function's own
        // doc comment. v:warningmsg IS set for real, matching the
        // original's set_vim_var_string(VV_WARNINGMSG, _(w_readonly), -1).
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            crate::eval::vars::set_vim_var_string(
                crate::eval::vars::VimVarIndex::Warningmsg,
                Some(b"W10: Warning: Changing a readonly file"),
            )
        };
        buf.b_did_warn = true;
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_ff_differs_false_for_never_loaded_buffer() {
        let mut buf = BufT { b_flags: b_flags::BF_NEVERLOADED as i32, ..Default::default() };
        assert!(!unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_false_for_new_empty_buffer_when_ignoring_empty() {
        // ml_open touches shared GLOBALS.got_int internally via
        // mf_sync - must hold the lock like every other GlobalCell-
        // touching test (see memfile.rs's mf_sync tests for the same
        // reasoning).
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        buf.b_flags = b_flags::BF_NEW as i32;

        assert!(!unsafe { file_ff_differs(&mut buf, true) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn file_ff_differs_true_when_new_empty_buffer_not_ignored() {
        // See file_ff_differs_false_for_new_empty_buffer_when_ignoring_
        // empty's own comment: ml_open touches shared GLOBALS.got_int.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        buf.b_flags = b_flags::BF_NEW as i32;
        // b_start_ffc defaults to 0, which differs from b_p_ff's
        // (also-defaulted) empty/None first byte only if we force a
        // mismatch - set b_p_ff so the ffc check itself trips.
        buf.b_p_ff = Some(b"unix".to_vec());
        buf.b_start_ffc = i32::from(b'd'); // "dos", deliberately different

        assert!(unsafe { file_ff_differs(&mut buf, false) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn file_ff_differs_true_when_fileformat_first_char_changed() {
        let mut buf = BufT {
            b_p_ff: Some(b"dos".to_vec()),
            b_start_ffc: i32::from(b'u'), // was "unix" when editing started
            ..Default::default()
        };
        assert!(unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_false_when_nothing_changed() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_fenc: Some(b"utf-8".to_vec()),
            b_start_fenc: Some(b"utf-8".to_vec()),
            ..Default::default()
        };
        assert!(!unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_true_when_fileencoding_changed() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_fenc: Some(b"latin1".to_vec()),
            b_start_fenc: Some(b"utf-8".to_vec()),
            ..Default::default()
        };
        assert!(unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_true_when_bomb_changed_and_not_binary() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_bin: 0,
            b_p_bomb: 1,
            b_start_bomb: 0,
            ..Default::default()
        };
        assert!(unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_ignores_bomb_change_when_binary() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_bin: 1,
            b_p_bomb: 1,
            b_start_bomb: 0,
            ..Default::default()
        };
        assert!(!unsafe { file_ff_differs(&mut buf, false) });
    }

    /// Points `GLOBALS.curbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime (this
    /// guard does NOT acquire its own lock - matching this crate's
    /// established "compose with an externally-held lock" pattern for
    /// guards that need to combine with other shared-state setup).
    struct CurbufGuard {
        previous: *mut BufT,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut BufT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    #[test]
    fn change_warning_is_a_noop_when_buffer_not_readonly() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ro: 0, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        unsafe { change_warning(&mut buf, 0) };

        assert!(!buf.b_did_warn);
    }

    #[test]
    fn change_warning_is_a_noop_when_already_warned() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ro: 1, b_did_warn: true, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline = true;

        unsafe { change_warning(&mut buf, 0) };

        // Unchanged: change_warning's own guard condition requires
        // b_did_warn == false to do anything at all.
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline);
    }

    #[test]
    fn change_warning_is_a_noop_when_autocmd_busy() {
        // Not achievable via any real translated function yet (nothing
        // can set AUTOCMD_BUSY true) - pokes it directly to prove
        // change_warning's own `!autocmd_busy` guard condition is
        // faithfully translated, independent of how AUTOCMD_BUSY
        // eventually gets set.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ro: 1, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        unsafe { *crate::autocmd::AUTOCMD_BUSY.get_mut() = true };

        unsafe { change_warning(&mut buf, 0) };

        unsafe { *crate::autocmd::AUTOCMD_BUSY.get_mut() = false };
        assert!(!buf.b_did_warn);
    }

    #[test]
    fn change_warning_is_a_noop_when_buffer_already_changed() {
        let _lock = crate::globals::global_state_test_lock();
        // b_changed != 0 makes curbuf_is_changed() true (bt_dontwrite
        // is false for a plain, default buftype).
        let mut buf = BufT { b_p_ro: 1, b_changed: 1, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        unsafe { change_warning(&mut buf, 0) };

        assert!(!buf.b_did_warn);
    }

    #[test]
    fn change_warning_warns_once_for_an_unchanged_readonly_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ro: 1, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline = true;

        unsafe { change_warning(&mut buf, 0) };

        assert!(buf.b_did_warn);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline);
        // b_ro_locked is incremented then decremented around the
        // apply_autocmds call - net zero once change_warning returns.
        assert_eq!(buf.b_ro_locked, 0);
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_str(crate::eval::vars::VimVarIndex::Warningmsg) },
            b"W10: Warning: Changing a readonly file"
        );

        // A second call is now a no-op (b_did_warn short-circuits).
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline = true;
        unsafe { change_warning(&mut buf, 0) };
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.redraw_cmdline);

        // Reset: VIMVARS is shared, process-wide state.
        unsafe {
            crate::eval::vars::set_vim_var_string(crate::eval::vars::VimVarIndex::Warningmsg, None)
        };
    }

    #[test]
    fn change_warning_leaves_warningmsg_untouched_when_a_noop() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ro: 0, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        unsafe { change_warning(&mut buf, 0) };

        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_str(crate::eval::vars::VimVarIndex::Warningmsg) },
            Vec::<u8>::new()
        );
    }
}
