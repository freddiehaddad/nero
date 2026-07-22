//! Translated from `src/nvim/cursor.c` (tractable core only).
//!
//! `cursor.c` (~520 lines) is the cursor-position validity/movement
//! utility file. Most of it needs the screen-column/character-width
//! subsystem (`plines.c`'s `getvcol`/`getvvcol`/`init_charsize_arg`/
//! `win_charsize`, none translated - a substantial subsystem of its
//! own, comparable in scope to `mbyte.c` but for on-screen character
//! width rather than byte-level decoding), the fold subsystem
//! (`fold.c`'s `hasFolding`/`hasFoldingWin`), or the redraw pipeline
//! (`redraw_later`/`changed_cline_bef_curs`).
//!
//! Translated: `check_pos`, `check_visual_pos`, `get_cursor_line_ptr`/
//! `get_cursor_pos_ptr`/`get_cursor_line_len`/`get_cursor_pos_len`
//! (thin `curwin`/`curbuf` + `memline.c`'s `ml_get_buf`/`ml_get_buf_len`
//! wrappers), `gchar_cursor`/`char_before_cursor` (+ `mbyte.c`'s
//! `utf_ptr2char`/`utf_head_off`), `pchar_cursor`, `adjust_cursor_col`,
//! `inc_cursor`/`dec_cursor` (thin wrappers over `memline.c`'s newly
//! translated `inc`/`dec`, operating on `curwin.w_cursor`). None of
//! these have a real caller translated yet either (their actual
//! callers are all in `normal.c`/`insert.c`/`ops.c`/`search.c`/etc.,
//! future phases) - translated anyway as this file's own genuinely
//! tractable core, matching the established per-file "harvest what's
//! mechanically composable from already-translated primitives, defer
//! the rest" treatment used for `mark.c`/`undo.c`/`buffer.c`/
//! `change.c`, since these are simple, faithful 1:1 wrappers with no
//! design freedom of their own to get wrong.
//!
//! `pchar_cursor` needs one deliberate representation note:
//! `memline.rs`'s `ml_get_buf_mut` (unlike the original's own
//! `ml_get_buf_mut`, which returns a raw pointer directly into the
//! live `ml_line_ptr` heap buffer, so writing through it mutates that
//! buffer automatically) returns a *disconnected copy* - see its own
//! doc comment ("this is very limited"). To actually persist a byte
//! write (matching `ml_flush_line`'s own later read of
//! `buf.b_ml.ml_line_ptr` as the source of truth), `pchar_cursor` here
//! calls `ml_get_buf_mut` first (for its real side effects: loading
//! the line into the cache, marking it dirty, and the undo
//! bookkeeping via `ml_add_deleted_len_buf`), then writes the new byte
//! directly into `buf.b_ml.ml_line_ptr`'s own slot - the actual
//! persistent storage this crate uses for "the currently locked/cached
//! line", equivalent to the original's pointer-based mutation.
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `getviscol`/`getviscol2`/`coladvance_force`/`coladvance`/
//!   `coladvance2`/`getvpos`/`set_leftcol`: need `plines.c`'s
//!   `getvcol`/`getvvcol`/character-width machinery, and (`set_leftcol`
//!   specifically) `redraw_later`/`changed_cline_bef_curs`.
//! - `check_cursor_lnum`/`get_cursor_rel_lnum`: need `fold.c`'s
//!   `hasFolding`/`hasFoldingWin`/`hasAnyFolding`.
//! - `check_cursor_col`/`check_cursor`: need `plines.c`'s `getvcol`
//!   (only for the narrow `'virtualedit'=all` coladd-clamping branch -
//!   deliberately not translated with that branch silently dropped,
//!   since that would be a real, if narrow, behavioral deviation, not
//!   a faithful translation) and `check_cursor_lnum` (fold.c, above).
//! - `virtual_active` (from `state.c`, not `cursor.c` itself, but
//!   checked while investigating `check_cursor_col`/`coladvance2`
//!   above, its only callers here): confirmed fully tractable
//!   whenever needed (`crate::globals::GLOBALS.State`/`virtual_op`/
//!   `Visual`, `crate::option_vars`'s `get_ve_flags`/`opt_ve_flag`,
//!   and `crate::ascii_defs::CTRL_V` all already exist) - not
//!   translated *yet* only because its own real callers
//!   (`coladvance2`/`check_cursor_col`) are themselves still deferred.

use crate::buffer_defs::BufT;
use crate::pos_defs::{ColnrT, PosT};

/// Make sure `pos.lnum` and `pos.col` are valid in `buf`. This allows
/// for the col to be on the NUL byte (`check_pos`).
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT` (same requirement as `crate::memline::ml_get_buf_len`).
pub unsafe fn check_pos(buf: &mut BufT, pos: &mut PosT) {
    pos.lnum = pos.lnum.min(buf.b_ml.ml_line_count);
    if pos.col > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        pos.col = pos.col.min(unsafe { crate::memline::ml_get_buf_len(buf, pos.lnum) });
    }
}

/// Check if `Visual.start` position is valid, correct it if not. Can
/// be called when in Visual mode and a change has been made
/// (`check_visual_pos`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` whose `b_ml.ml_mfp`, if non-null, points to a live
/// `MemfileT`.
pub unsafe fn check_visual_pos() {
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }.b_ml.ml_line_count;
    let start_lnum = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start.lnum;

    if start_lnum > line_count {
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.Visual.start.lnum = line_count;
        g.Visual.start.col = 0;
        g.Visual.start.coladd = 0;
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let len = unsafe { crate::memline::ml_get_len(start_lnum) };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        if g.Visual.start.col > len {
            g.Visual.start.col = len;
            g.Visual.start.coladd = 0;
        }
    }
}

/// @return pointer to cursor line (`get_cursor_line_ptr`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin`/`curbuf` must be valid, non-null
/// pointers to live `WinT`/`BufT`, and `curbuf.b_ml.ml_mfp`, if
/// non-null, must point to a live `MemfileT`.
#[must_use]
pub unsafe fn get_cursor_line_ptr() -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.lnum;
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::ml_get_buf(curbuf, lnum) }
}

/// @return pointer to cursor position (`get_cursor_pos_ptr`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
#[must_use]
pub unsafe fn get_cursor_pos_ptr() -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { get_cursor_line_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col as usize;
    line[col..].to_vec()
}

/// @return length (excluding the NUL) of the cursor line
/// (`get_cursor_line_len`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
#[must_use]
pub unsafe fn get_cursor_line_len() -> ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.lnum;
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::ml_get_buf_len(curbuf, lnum) }
}

/// @return length (excluding the NUL) of the cursor position
/// (`get_cursor_pos_len`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
#[must_use]
pub unsafe fn get_cursor_pos_len() -> ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { get_cursor_line_len() };
    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col;
    len - col
}

/// @return the character under the cursor (`gchar_cursor`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
#[must_use]
pub unsafe fn gchar_cursor() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let p = unsafe { get_cursor_pos_ptr() };
    crate::mbyte::utf_ptr2char(&p)
}

/// Return the character immediately before the cursor
/// (`char_before_cursor`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`]. Also touches
/// `crate::option_vars::OPTION_VARS` (via `crate::mbyte::utf_head_off`).
#[must_use]
pub unsafe fn char_before_cursor() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col;
    if col == 0 {
        return -1;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { get_cursor_line_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let prev_len = unsafe { crate::mbyte::utf_head_off(&line, (col - 1) as usize) } + 1;
    crate::mbyte::utf_ptr2char(&line[(col - prev_len) as usize..])
}

/// Write a character at the current cursor position. It is directly
/// written into the block (`pchar_cursor`).
///
/// See this module's own doc comment for why this writes into
/// `buf.b_ml.ml_line_ptr` directly rather than through
/// `ml_get_buf_mut`'s own (disconnected-copy) return value.
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
pub unsafe fn pchar_cursor(c: u8) {
    // SAFETY: forwarded from this function's own safety doc.
    let (lnum, col) = {
        let curwin = unsafe { &*crate::globals::GLOBALS.get_mut().curwin };
        (curwin.w_cursor.lnum, curwin.w_cursor.col as usize)
    };
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    // Called only for its side effects (loading+dirtying the cached
    // line, undo bookkeeping) - the returned copy itself is discarded,
    // see this module's own doc comment for why the real write below
    // goes through `ml_line_ptr` directly instead.
    // SAFETY: forwarded from this function's own safety doc.
    let _ = unsafe { crate::memline::ml_get_buf_mut(curbuf, lnum) };
    if let Some(line) = curbuf.b_ml.ml_line_ptr.as_mut() {
        line[col] = c;
    }
}

/// Make sure `curwin.w_cursor` is not on the NUL at the end of the
/// line. Allow it when in Visual mode and `'selection'` is not "old"
/// (`adjust_cursor_col`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
pub unsafe fn adjust_cursor_col() {
    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col;
    let visual_active = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active;
    let sel_first_byte = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_sel
        .as_deref()
        .and_then(|s| s.first())
        .copied();

    if col > 0
        && (!visual_active || sel_first_byte == Some(b'o'))
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { gchar_cursor() } == 0
    {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col -= 1;
    }
}

/// Increment the cursor position. See [`crate::memline::inc`] for
/// return values (`inc_cursor`).
///
/// # Safety
/// Same as [`crate::memline::inc`]. `crate::globals::GLOBALS.curwin`
/// must additionally be a valid, non-null pointer to a live `WinT`.
pub unsafe fn inc_cursor() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::inc(&mut curwin.w_cursor) }
}

/// Decrement the line pointer, crossing line boundaries as necessary.
/// See [`crate::memline::dec`] for return values (`dec_cursor`).
///
/// # Safety
/// Same as [`crate::memline::dec`]. `crate::globals::GLOBALS.curwin`
/// must additionally be a valid, non-null pointer to a live `WinT`.
pub unsafe fn dec_cursor() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::dec(&mut curwin.w_cursor) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::WinT;
    use crate::globals::{global_state_test_lock, GLOBALS};

    /// RAII guard installing `win`/`buf` as curwin/curbuf, restoring
    /// the previous pointers on drop (including on test panic via
    /// unwinding). Holds `global_state_test_lock` for its entire
    /// lifetime, matching `mark.rs`'s own `MarkTestGuard`/`CurbufGuard`
    /// precedent - needed since `ml_open` (used to build the test
    /// memline below) touches shared `GLOBALS.got_int` internally.
    struct CursorTestGuard {
        prev_curwin: *mut WinT,
        prev_curbuf: *mut BufT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CursorTestGuard {
        fn set(win: *mut WinT, buf: *mut BufT) -> Self {
            let _lock = global_state_test_lock();
            let globals = unsafe { GLOBALS.get_mut() };
            let guard = CursorTestGuard {
                prev_curwin: globals.curwin,
                prev_curbuf: globals.curbuf,
                _lock,
            };
            globals.curwin = win;
            globals.curbuf = buf;
            guard
        }
    }

    impl Drop for CursorTestGuard {
        fn drop(&mut self) {
            let globals = unsafe { GLOBALS.get_mut() };
            globals.curwin = self.prev_curwin;
            globals.curbuf = self.prev_curbuf;
        }
    }

    /// Installs `win`/`buf` as curwin/curbuf (acquiring the lock
    /// first, per `mark.rs`'s `open_and_set_curbuf` precedent), then
    /// opens a fresh memline for `buf` and replaces line 1 with
    /// `line`. Callers must close `buf.b_ml.ml_mfp` themselves after
    /// the guard is dropped.
    fn open_and_set_test_buf(win: &mut WinT, buf: &mut BufT, line: &[u8]) -> CursorTestGuard {
        let guard = CursorTestGuard::set(win as *mut WinT, buf as *mut BufT);
        assert_eq!(unsafe { crate::memline::ml_open(buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(buf, 1, line) },
            crate::vim_defs::OK
        );
        guard
    }

    fn close_buf_with_memline(buf: BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn check_pos_clamps_lnum_and_col() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        let mut pos = PosT { lnum: 99, col: 0, coladd: 0 };
        unsafe { check_pos(&mut buf, &mut pos) };
        assert_eq!(pos.lnum, 1); // clamped to ml_line_count

        let mut pos2 = PosT { lnum: 1, col: 99, coladd: 0 };
        unsafe { check_pos(&mut buf, &mut pos2) };
        assert_eq!(pos2.col, 2); // clamped to ml_get_buf_len("hi") == 2

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_pos_leaves_col_zero_untouched() {
        // col == 0 is never clamped (matches the original's own `if
        // (pos->col > 0)` guard) - even on a fully out-of-range lnum.
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        let mut pos = PosT { lnum: 1, col: 0, coladd: 0 };
        unsafe { check_pos(&mut buf, &mut pos) };
        assert_eq!(pos.col, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_visual_pos_clamps_lnum_and_col() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        unsafe { GLOBALS.get_mut() }.Visual.start = PosT { lnum: 99, col: 5, coladd: 3 };
        unsafe { check_visual_pos() };
        let start = unsafe { GLOBALS.get_mut() }.Visual.start;
        assert_eq!(start, PosT { lnum: 1, col: 0, coladd: 0 });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_visual_pos_clamps_col_when_lnum_valid() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        unsafe { GLOBALS.get_mut() }.Visual.start = PosT { lnum: 1, col: 99, coladd: 3 };
        unsafe { check_visual_pos() };
        let start = unsafe { GLOBALS.get_mut() }.Visual.start;
        assert_eq!(start, PosT { lnum: 1, col: 2, coladd: 0 }); // ml_get_len("hi") == 2

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn cursor_line_and_pos_accessors_match_expectations() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 2, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        assert_eq!(unsafe { get_cursor_line_ptr() }, b"hello\0".to_vec());
        assert_eq!(unsafe { get_cursor_pos_ptr() }, b"llo\0".to_vec());
        assert_eq!(unsafe { get_cursor_line_len() }, 5);
        assert_eq!(unsafe { get_cursor_pos_len() }, 3);
        assert_eq!(unsafe { gchar_cursor() }, i32::from(b'l'));

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn char_before_cursor_returns_minus_one_at_col_zero() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 0, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        assert_eq!(unsafe { char_before_cursor() }, -1);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn char_before_cursor_returns_preceding_ascii_char() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 2, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        // "hello": col 2 is the second 'l', so the char immediately
        // before it (col 1) is 'e'.
        assert_eq!(unsafe { char_before_cursor() }, i32::from(b'e'));

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn pchar_cursor_writes_directly_into_the_line() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 1, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        unsafe { pchar_cursor(b'A') };
        assert_eq!(unsafe { get_cursor_line_ptr() }, b"hAllo\0".to_vec());

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn adjust_cursor_col_steps_back_off_trailing_nul() {
        let mut buf = BufT::default();
        // Cursor sitting on the line's own NUL (col == line length).
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 5, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        unsafe { adjust_cursor_col() };
        assert_eq!(unsafe { &*GLOBALS.get_mut().curwin }.w_cursor.col, 4);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn adjust_cursor_col_is_noop_when_not_on_nul() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 2, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        unsafe { adjust_cursor_col() };
        assert_eq!(unsafe { &*GLOBALS.get_mut().curwin }.w_cursor.col, 2);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn inc_cursor_and_dec_cursor_move_curwin_w_cursor() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 0, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        assert_eq!(unsafe { inc_cursor() }, 0);
        assert_eq!(unsafe { &*GLOBALS.get_mut().curwin }.w_cursor.col, 1);

        assert_eq!(unsafe { dec_cursor() }, 0);
        assert_eq!(unsafe { &*GLOBALS.get_mut().curwin }.w_cursor.col, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }
}
