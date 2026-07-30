//! Translated from `src/nvim/insert.c` (tractable core only).
//!
//! `insert.c` (~4400 lines) is Insert mode's own state machine: entry/
//! exit, key handling, backspace, digraphs, the replace-mode "pop"
//! stack, and much more - almost none of that is attempted here, since
//! it needs real buffer modification, the redraw pipeline, and
//! Insert-mode-specific global state (`stop_insert`/`Insstart`/etc.)
//! none of which are translated yet.
//!
//! Translated: [`get_nolist_virtcol`] - the value `w_virtcol` would
//! have if `'list'` were off, unless `'cpo'` contains the `'L'`
//! flag. Every real dependency (`getvcol_nolist`/`validate_virtcol`/
//! `vim_strchr`, `option_vars::CPO_LISTWM`) already existed;
//! translated ahead of its own real callers (`ins_tab`/several
//! others in this same file, none translated), matching this crate's
//! established "small, self-contained, no design freedom to get
//! wrong" precedent for translating ahead of a real caller.

use crate::pos_defs::ColnrT;

/// Get the value `w_virtcol` would have if `'list'` were off, unless
/// `'cpo'` contains the `'L'` flag (`get_nolist_virtcol`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` - same requirement as
/// [`crate::plines::getvcol_nolist`]/[`crate::move::validate_virtcol`].
#[must_use]
pub unsafe fn get_nolist_virtcol() -> ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &mut *curwin };

    // check validity of cursor in current buffer
    if win.w_buffer.is_null()
        // SAFETY: forwarded from this function's own safety doc.
        || unsafe { (*win.w_buffer).b_ml.ml_mfp }.is_null()
        // SAFETY: forwarded from this function's own safety doc.
        || win.w_cursor.lnum > unsafe { (*win.w_buffer).b_ml.ml_line_count }
    {
        return 0;
    }

    if win.w_onebuf_opt.wo_list != 0
        // SAFETY: forwarded from this function's own safety doc.
        && crate::strings::vim_strchr(
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo.as_deref().unwrap_or(&[]),
            i32::from(crate::option_vars::CPO_LISTWM),
        )
        .is_none()
    {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::plines::getvcol_nolist(&mut win.w_cursor) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::r#move::validate_virtcol(curwin) };
    win.w_virtcol
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{BufT, WinT};
    use crate::globals::global_state_test_lock;
    use crate::memline_defs::MemlineT;

    struct CurwinGuard {
        previous: *mut WinT,
    }

    impl CurwinGuard {
        fn set(win: &mut WinT) -> Self {
            // SAFETY: single-threaded test, lock held by the caller.
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let previous = g.curwin;
            g.curwin = win;
            CurwinGuard { previous }
        }
    }

    impl Drop for CurwinGuard {
        fn drop(&mut self) {
            // SAFETY: restoring the previous value on drop.
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.previous;
        }
    }

    fn buf_with_one_real_line() -> BufT {
        BufT {
            b_ml: MemlineT { ml_mfp: std::ptr::NonNull::dangling().as_ptr(), ml_line_count: 1, ..Default::default() },
            ..Default::default()
        }
    }

    #[test]
    fn get_nolist_virtcol_is_zero_when_w_buffer_is_null() {
        let _lock = global_state_test_lock();
        let mut win = WinT { w_buffer: std::ptr::null_mut(), ..Default::default() };
        let _guard = CurwinGuard::set(&mut win);
        assert_eq!(unsafe { get_nolist_virtcol() }, 0);
    }

    #[test]
    fn get_nolist_virtcol_is_zero_when_ml_mfp_is_null() {
        let _lock = global_state_test_lock();
        let mut buf = BufT { b_ml: MemlineT { ml_mfp: std::ptr::null_mut(), ..Default::default() }, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf, ..Default::default() };
        let _guard = CurwinGuard::set(&mut win);
        assert_eq!(unsafe { get_nolist_virtcol() }, 0);
    }

    #[test]
    fn get_nolist_virtcol_is_zero_when_cursor_past_the_last_line() {
        let _lock = global_state_test_lock();
        let mut buf = buf_with_one_real_line();
        let mut win = WinT {
            w_buffer: &mut buf,
            w_cursor: crate::pos_defs::PosT { lnum: 5, ..Default::default() },
            ..Default::default()
        };
        let _guard = CurwinGuard::set(&mut win);
        assert_eq!(unsafe { get_nolist_virtcol() }, 0);
    }

    #[test]
    fn get_nolist_virtcol_uses_w_virtcol_when_list_is_off() {
        let _lock = global_state_test_lock();
        let mut buf = buf_with_one_real_line();
        let cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let mut win = WinT {
            w_buffer: &mut buf,
            w_cursor: cursor,
            // Must match w_cursor exactly, or validate_virtcol's own
            // internal check_cursor_moved call clears VALID_VIRTCOL
            // (among other bits) right back out before ever reaching
            // its own "already valid" fast-path check below.
            w_valid_cursor: cursor,
            w_virtcol: 42,
            w_valid: i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL),
            ..Default::default()
        };
        win.w_onebuf_opt.wo_list = 0;
        let _guard = CurwinGuard::set(&mut win);
        assert_eq!(unsafe { get_nolist_virtcol() }, 42);
    }

    #[test]
    fn get_nolist_virtcol_uses_w_virtcol_when_cpo_contains_l_flag() {
        let _lock = global_state_test_lock();
        let mut buf = buf_with_one_real_line();
        let cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let mut win = WinT {
            w_buffer: &mut buf,
            w_cursor: cursor,
            // Same reasoning as the sibling test above.
            w_valid_cursor: cursor,
            w_virtcol: 7,
            w_valid: i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL),
            ..Default::default()
        };
        win.w_onebuf_opt.wo_list = 1;
        let _guard = CurwinGuard::set(&mut win);
        let previous_cpo = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo = Some(b"aBL".to_vec());
        assert_eq!(unsafe { get_nolist_virtcol() }, 7);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo = previous_cpo;
    }
}
