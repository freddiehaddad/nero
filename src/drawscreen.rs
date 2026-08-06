//! Translated from `src/nvim/drawscreen.c` (tractable core only).
//!
//! `drawscreen.c` is neovim's actual screen-redraw driver (thousands
//! of lines) - almost entirely dependent on the real TUI/grid
//! rendering pipeline, not attempted here. Translated: `number_width`
//! (needed by `plines.c`'s/`move.c`'s window-column-offset
//! calculations), `redraw_buf_status_later` (needed by `change.c`'s
//! `changed_internal`/`unchanged`), and [`comp_col`] - computes
//! `sc_col`/`ru_col` (screen columns reserved for the "showcmd"/ruler
//! areas of the last status/command line) and `v:echospace`, whenever
//! `'ruler'`/`'showcmd'`/`'showcmdloc'`/`Columns` change. Needed only
//! already-real pieces: `window.rs`'s `last_stl_height`,
//! `option_vars.rs`'s `p_ru`/`p_sc`/`p_sloc`, `globals.rs`'s
//! `Columns`/`ru_wid`/`sc_col`/`ru_col`, and `eval/vars.rs`'s
//! `set_vim_var_nr`. Harvested ahead of its real caller, `option.c`'s
//! `did_set_option` (not yet translated), matching this crate's
//! established "small, simple, mechanically correct piece ahead of
//! its real caller" precedent.
//!
//! Also translated: the whole redraw-scheduling family -
//! [`redraw_later`], [`set_must_redraw`], [`redraw_all_later`],
//! [`redraw_buf_later`], [`redraw_curbuf_later`],
//! [`redraw_win_range_later`], [`redraw_win_line`] (`redrawWinline`),
//! [`redraw_buf_line_later`] and [`redraw_buf_range_later`], plus the
//! `UPD_*` level constants and [`REDRAW_NOT_ALLOWED`]. These are pure
//! bookkeeping over already-real per-window fields (`w_redr_type`,
//! `w_lines_valid`, `w_redraw_top`/`w_redraw_bot`) and the
//! `must_redraw` global, so they are fully faithful without needing
//! any of the grid/TUI pipeline - what they schedule is only acted on
//! later by `update_screen`, which remains deferred. Needed by
//! `change.c`'s `changed_common`/`changed_lines_redraw_buf` chain.
//!
//! Deferred: everything else in the file.

use crate::buffer_defs::{BufT, WinT};
use crate::globals::GlobalCell;
use crate::pos_defs::LinenrT;
use crate::window::global_stl_height;

/// Buffer not changed, or changes marked with `b_mod_*` (`UPD_VALID`).
pub const UPD_VALID: i32 = 10;
/// Redisplay inverted part that changed (`UPD_INVERTED`).
pub const UPD_INVERTED: i32 = 20;
/// Redisplay whole inverted part (`UPD_INVERTED_ALL`).
pub const UPD_INVERTED_ALL: i32 = 25;
/// Display first `w_upd_rows` screen lines (`UPD_REDRAW_TOP`).
pub const UPD_REDRAW_TOP: i32 = 30;
/// Like [`UPD_NOT_VALID`] but may scroll (`UPD_SOME_VALID`).
pub const UPD_SOME_VALID: i32 = 35;
/// Buffer needs a complete redraw (`UPD_NOT_VALID`).
pub const UPD_NOT_VALID: i32 = 40;
/// Screen messed up, clear it (`UPD_CLEAR`).
pub const UPD_CLEAR: i32 = 50;

/// While computing a statusline and the like we do not want any
/// `w_redr_type` or `must_redraw` to be set (`redraw_not_allowed`).
pub static REDRAW_NOT_ALLOWED: GlobalCell<bool> = GlobalCell::new(false);

/// The line the `'hlsearch'` highlight currently reports the cursor
/// on (`search_hl_has_cursor_lnum`).
pub static SEARCH_HL_HAS_CURSOR_LNUM: GlobalCell<LinenrT> = GlobalCell::new(0);

/// Return the required width for the `'number'`/`'relativenumber'`
/// column in `wp`, caching the result until the relevant line count
/// changes (`number_width`).
///
/// # Safety
/// `wp.w_buffer` must be a valid, non-null pointer to a live `BufT`.
pub unsafe fn number_width(wp: &mut WinT) -> i32 {
    let mut lnum: LinenrT = if wp.w_onebuf_opt.wo_rnu != 0 && wp.w_onebuf_opt.wo_nu == 0 {
        // cursor line shows "0"
        wp.w_view_height
    } else {
        // cursor line shows absolute line number
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*wp.w_buffer }.b_ml.ml_line_count
    };

    if lnum == wp.w_nrwidth_line_count {
        return wp.w_nrwidth_width;
    }
    wp.w_nrwidth_line_count = lnum;

    // reset for 'statuscolumn'
    if wp.w_onebuf_opt.wo_stc.as_deref().is_some_and(|s| !s.is_empty()) {
        wp.w_statuscol_line_count = 0; // make sure width is re-estimated
        let nu_or_rnu = wp.w_onebuf_opt.wo_nu != 0 || wp.w_onebuf_opt.wo_rnu != 0;
        wp.w_nrwidth_width = i32::from(nu_or_rnu) * (wp.w_onebuf_opt.wo_nuw as i32);
        return wp.w_nrwidth_width;
    }

    let mut n = 0;
    loop {
        lnum /= 10;
        n += 1;
        if lnum <= 0 {
            break;
        }
    }

    // 'numberwidth' gives the minimal width plus one
    n = n.max(wp.w_onebuf_opt.wo_nuw as i32 - 1);

    // If 'signcolumn' is set to 'number' and there is a sign to display, then
    // the minimal width for the number column is 2.
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*wp.w_buffer };
    if n < 2
        && crate::buffer::buf_meta_total(buf, crate::marktree_defs::MetaIndex::SignText) != 0
        && wp.w_minscwidth == crate::option_vars::SCL_NUM
    {
        n = 2;
    }

    wp.w_nrwidth_width = n;
    n
}

/// Mark every window in the current tab page that's editing `buf` and
/// currently shows a status line/winbar to redraw its status line
/// later (`redraw_buf_status_later`).
///
/// The original's own `set_must_redraw(UPD_VALID)` call is omitted - a
/// pure "please redraw the screen" scheduling signal with no
/// observable effect without a real screen/grid rendering pipeline,
/// matching the established precedent (`move.rs`'s/`cursor.rs`'s own
/// omitted `redraw_later` calls). `wp.w_redr_status` itself, a real,
/// observable per-window flag, IS set faithfully.
///
/// The original's own `FOR_ALL_WINDOWS_IN_TAB(wp, curtab)` always
/// resolves to its `firstwin` branch at this specific call site (the
/// macro compares `curtab` to itself), so this walks
/// `GLOBALS.firstwin`/`w_next` directly instead, matching
/// `fmarks_check_names`'s own established precedent for this exact
/// simplification.
///
/// # Safety
/// `buf` must be a valid, non-null pointer to a live `BufT`.
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid,
/// live `WinT` pointers.
pub unsafe fn redraw_buf_status_later(buf: *mut BufT) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let curwin = g.curwin;
    let mut wp = g.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &mut *wp };
        if w.w_buffer == buf
            && (w.w_status_height != 0
                // SAFETY: forwarded from this function's own safety
                // doc (touches OPTION_VARS, no BufT/WinT access).
                || (wp == curwin && unsafe { global_stl_height() } != 0)
                || w.w_winbar_height != 0)
        {
            w.w_redr_status = true;
        }
        wp = w.w_next;
    }
}

/// Schedule window `wp` to be redrawn later, at level `type_`
/// (`redraw_later`).
///
/// Nothing happens while exiting, while redrawing is disallowed, or
/// when the window is already scheduled for an equal-or-heavier
/// redraw. `must_redraw` is kept at the maximum over all windows.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`, unless
/// `GLOBALS.exiting` is set (matching the original's own
/// `assert(wp != NULL || exiting)`).
pub unsafe fn redraw_later(wp: *mut WinT, type_: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    debug_assert!(!wp.is_null() || g.exiting);
    // SAFETY: reading a plain `bool` global, matching this crate's
    // established `GlobalCell::get_mut` convention.
    let not_allowed = *unsafe { REDRAW_NOT_ALLOWED.get_mut() };
    if g.exiting || not_allowed {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc - `wp` is
    // non-null here, since `exiting` was just ruled out.
    let w = unsafe { &mut *wp };
    if w.w_redr_type < type_ {
        w.w_redr_type = type_;
        if type_ >= UPD_NOT_VALID {
            w.w_lines_valid = 0;
        }
        // must_redraw is the maximum over all windows.
        g.must_redraw = g.must_redraw.max(type_);
    }
}

/// Set `must_redraw` to `type_` unless it already has a higher value,
/// or redrawing is currently not allowed (`set_must_redraw`).
pub fn set_must_redraw(type_: i32) {
    // SAFETY: reading a plain `bool` global, matching this crate's
    // established `GlobalCell::get_mut` convention.
    if *unsafe { REDRAW_NOT_ALLOWED.get_mut() } {
        return;
    }
    // SAFETY: as above - a plain `i32` field of `GLOBALS`.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    g.must_redraw = g.must_redraw.max(type_);
}

/// Mark all windows to be redrawn later (`redraw_all_later`).
///
/// # Safety
/// Same as [`redraw_later`], for every window in
/// `GLOBALS.firstwin`'s own `w_next` chain.
pub unsafe fn redraw_all_later(type_: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { redraw_later(wp, type_) };
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    // This may be needed when switching tabs.
    set_must_redraw(type_);
}

/// Mark every window displaying `buf` to be redrawn later
/// (`redraw_buf_later`).
///
/// # Safety
/// Same as [`redraw_all_later`].
pub unsafe fn redraw_buf_later(buf: *mut BufT, type_: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &*wp };
        let next = w.w_next;
        if w.w_buffer == buf {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { redraw_later(wp, type_) };
        }
        wp = next;
    }
}

/// Mark every window displaying the current buffer to be redrawn
/// later (`redraw_curbuf_later`).
///
/// # Safety
/// Same as [`redraw_buf_later`], plus `GLOBALS.curbuf` must be valid.
pub unsafe fn redraw_curbuf_later(type_: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { redraw_buf_later(curbuf, type_) };
}

/// Mark the line range `first..=last` of window `wp` for redrawing
/// (`redraw_win_range_later`).
///
/// Only has an effect when the range actually overlaps the window's
/// currently displayed lines.
///
/// # Safety
/// Same as [`redraw_later`].
pub unsafe fn redraw_win_range_later(wp: *mut WinT, first: LinenrT, last: LinenrT) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    if last >= w.w_topline && first < w.w_botline {
        if w.w_redraw_top == 0 || w.w_redraw_top > first {
            w.w_redraw_top = first;
        }
        if w.w_redraw_bot == 0 || w.w_redraw_bot < last {
            w.w_redraw_bot = last;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { redraw_later(wp, UPD_VALID) };
    }
}

/// Something changed in window `wp` at buffer line `lnum` that
/// requires that line, and possibly others, to be redrawn
/// (`redrawWinline`).
///
/// Used when entering/leaving Insert mode with the cursor on a folded
/// line, and to remove the `"$"` left by a change command. Note that
/// when lines are also inserted or deleted, `w_redraw_top`/
/// `w_redraw_bot` may become invalid and the whole window has to be
/// redrawn.
///
/// # Safety
/// Same as [`redraw_win_range_later`].
pub unsafe fn redraw_win_line(wp: *mut WinT, lnum: LinenrT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { redraw_win_range_later(wp, lnum, lnum) };
}

/// Mark line `line` of `buf` for redrawing in every window showing
/// that buffer (`redraw_buf_line_later`).
///
/// With `force`, a line past the end of the buffer still extends
/// `w_redraw_bot`, so the area below the last line is redrawn too.
///
/// # Safety
/// Same as [`redraw_buf_later`].
pub unsafe fn redraw_buf_line_later(buf: *mut BufT, line: LinenrT, force: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*buf }.b_ml.ml_line_count;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let next = unsafe { &*wp }.w_next;
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*wp }.w_buffer == buf {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { redraw_win_line(wp, line.min(line_count)) };
            if force && line > line_count {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &mut *wp }.w_redraw_bot = line;
            }
        }
        wp = next;
    }
}

/// Mark the line range `first..=last` of `buf` for redrawing in every
/// window showing that buffer (`redraw_buf_range_later`).
///
/// # Safety
/// Same as [`redraw_buf_later`].
pub unsafe fn redraw_buf_range_later(buf: *mut BufT, first: LinenrT, last: LinenrT) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let next = unsafe { &*wp }.w_next;
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*wp }.w_buffer == buf {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { redraw_win_range_later(wp, first, last) };
        }
        wp = next;
    }
}

/// Columns needed by the standard ruler (`COL_RULER`).
const COL_RULER: i32 = 17;
/// Compute columns for the ruler and shown-command areas. `sc_col` is
/// also used to decide the maximum length of a message on the status
/// line. If there is a status line for the last window, `sc_col` is
/// independent of `ru_col` (`comp_col`).
///
/// The original's own `sc_col`/`ru_col` globals are mutated directly
/// throughout its body; this translation instead computes into local
/// variables and assigns `GLOBALS.sc_col`/`ru_col` once at the very
/// end - a faithful, purely-cosmetic reordering, since nothing else
/// reads either global mid-computation (this crate is single-
/// threaded throughout, matching every other function's own
/// established assumption).
///
/// # Safety
/// `crate::globals::GLOBALS.firstwin`'s own `w_next` chain must
/// consist of valid, live `WinT` pointers (forwarded to
/// `crate::window::last_stl_height`, which in turn forwards to
/// `crate::window::one_window`).
pub unsafe fn comp_col() {
    // SAFETY: forwarded from this function's own safety doc.
    let last_has_status = unsafe { crate::window::last_stl_height(false) } > 0;

    // SAFETY: momentary reads, no aliasing.
    let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    let p_ru = ov.p_ru;
    let p_sc = ov.p_sc;
    let sloc_is_l = ov.p_sloc.as_deref().and_then(<[u8]>::first).copied() == Some(b'l');

    let mut sc_col: i32 = 0;
    let mut ru_col: i32 = 0;

    // SAFETY: momentary read, no aliasing.
    let ru_wid = unsafe { crate::globals::GLOBALS.get_mut() }.ru_wid;
    if p_ru != 0 {
        ru_col = (if ru_wid != 0 { ru_wid } else { COL_RULER }) + 1;
        // no last status line, adjust sc_col
        if !last_has_status {
            sc_col = ru_col;
        }
    }
    if p_sc != 0 && sloc_is_l {
        sc_col += crate::normal_defs::SHOWCMD_COLS as i32;
        if p_ru == 0 || last_has_status {
            // no need for separating space
            sc_col += 1;
        }
    }

    // SAFETY: momentary read, no aliasing.
    let columns = unsafe { crate::globals::GLOBALS.get_mut() }.Columns;
    debug_assert!(sc_col >= 0);
    sc_col = columns - sc_col;
    debug_assert!(ru_col >= 0);
    ru_col = columns - ru_col;
    if sc_col <= 0 {
        // screen too narrow, will become a mess
        sc_col = 1;
    }
    if ru_col <= 0 {
        ru_col = 1;
    }

    // SAFETY: momentary writes, no aliasing.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    g.sc_col = sc_col;
    g.ru_col = ru_col;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::vars::set_vim_var_nr(crate::eval::vars::VimVarIndex::Echospace, i64::from(sc_col - 1)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RAII guard installing a window chain and restoring the previous
    /// globals afterwards (even on panic). Self-locking, matching this
    /// crate's established per-file test-guard convention.
    struct RedrawTestGuard {
        prev_firstwin: *mut WinT,
        prev_curwin: *mut WinT,
        prev_curbuf: *mut BufT,
        prev_exiting: bool,
        prev_must_redraw: i32,
        prev_not_allowed: bool,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl RedrawTestGuard {
        fn set(firstwin: *mut WinT, curbuf: *mut BufT) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = RedrawTestGuard {
                prev_firstwin: g.firstwin,
                prev_curwin: g.curwin,
                prev_curbuf: g.curbuf,
                prev_exiting: g.exiting,
                prev_must_redraw: g.must_redraw,
                prev_not_allowed: *unsafe { REDRAW_NOT_ALLOWED.get_mut() },
                _lock,
            };
            g.firstwin = firstwin;
            g.curwin = firstwin;
            g.curbuf = curbuf;
            g.exiting = false;
            g.must_redraw = 0;
            unsafe { *REDRAW_NOT_ALLOWED.get_mut() = false };
            guard
        }
    }

    impl Drop for RedrawTestGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.firstwin = self.prev_firstwin;
            g.curwin = self.prev_curwin;
            g.curbuf = self.prev_curbuf;
            g.exiting = self.prev_exiting;
            g.must_redraw = self.prev_must_redraw;
            unsafe { *REDRAW_NOT_ALLOWED.get_mut() = self.prev_not_allowed };
        }
    }

    /// A window that is displaying lines 1..=10, so range-based
    /// scheduling actually overlaps it.
    fn visible_win(buf: *mut BufT) -> WinT {
        WinT {
            w_buffer: buf,
            w_topline: 1,
            w_botline: 11,
            ..Default::default()
        }
    }

    #[test]
    fn redraw_later_raises_the_level_and_tracks_the_maximum() {
        let mut buf = BufT::default();
        let mut win = visible_win(&mut buf);
        let _guard = RedrawTestGuard::set(&mut win, &mut buf);

        unsafe { redraw_later(&mut win, UPD_VALID) };
        assert_eq!(win.w_redr_type, UPD_VALID);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.must_redraw, UPD_VALID);

        // A heavier redraw wins, and also invalidates the line cache.
        win.w_lines_valid = 7;
        unsafe { redraw_later(&mut win, UPD_NOT_VALID) };
        assert_eq!(win.w_redr_type, UPD_NOT_VALID);
        assert_eq!(win.w_lines_valid, 0);

        // A lighter one is ignored entirely.
        unsafe { redraw_later(&mut win, UPD_VALID) };
        assert_eq!(win.w_redr_type, UPD_NOT_VALID);
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.must_redraw,
            UPD_NOT_VALID
        );
    }

    #[test]
    fn redraw_later_does_nothing_while_exiting_or_disallowed() {
        let mut buf = BufT::default();
        let mut win = visible_win(&mut buf);
        let _guard = RedrawTestGuard::set(&mut win, &mut buf);

        unsafe { crate::globals::GLOBALS.get_mut() }.exiting = true;
        unsafe { redraw_later(&mut win, UPD_CLEAR) };
        assert_eq!(win.w_redr_type, 0);

        unsafe { crate::globals::GLOBALS.get_mut() }.exiting = false;
        unsafe { *REDRAW_NOT_ALLOWED.get_mut() = true };
        unsafe { redraw_later(&mut win, UPD_CLEAR) };
        assert_eq!(win.w_redr_type, 0);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.must_redraw, 0);
    }

    #[test]
    fn set_must_redraw_keeps_the_maximum_and_respects_the_ban() {
        let mut buf = BufT::default();
        let mut win = visible_win(&mut buf);
        let _guard = RedrawTestGuard::set(&mut win, &mut buf);

        set_must_redraw(UPD_SOME_VALID);
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.must_redraw,
            UPD_SOME_VALID
        );
        set_must_redraw(UPD_VALID); // lower, ignored
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.must_redraw,
            UPD_SOME_VALID
        );

        unsafe { *REDRAW_NOT_ALLOWED.get_mut() = true };
        set_must_redraw(UPD_CLEAR);
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.must_redraw,
            UPD_SOME_VALID
        );
    }

    #[test]
    fn redraw_buf_later_only_touches_windows_on_that_buffer() {
        let mut buf_a = BufT::default();
        let mut buf_b = BufT::default();
        let mut win_b = visible_win(&mut buf_b);
        let mut win_a = visible_win(&mut buf_a);
        win_a.w_next = &mut win_b;
        let _guard = RedrawTestGuard::set(&mut win_a, &mut buf_a);

        unsafe { redraw_buf_later(&mut buf_a, UPD_NOT_VALID) };
        assert_eq!(win_a.w_redr_type, UPD_NOT_VALID);
        assert_eq!(win_b.w_redr_type, 0, "other buffer's window untouched");
    }

    #[test]
    fn redraw_all_later_walks_the_whole_chain() {
        let mut buf_a = BufT::default();
        let mut buf_b = BufT::default();
        let mut win_b = visible_win(&mut buf_b);
        let mut win_a = visible_win(&mut buf_a);
        win_a.w_next = &mut win_b;
        let _guard = RedrawTestGuard::set(&mut win_a, &mut buf_a);

        unsafe { redraw_all_later(UPD_INVERTED) };
        assert_eq!(win_a.w_redr_type, UPD_INVERTED);
        assert_eq!(win_b.w_redr_type, UPD_INVERTED, "regardless of buffer");
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.must_redraw,
            UPD_INVERTED
        );
    }

    #[test]
    fn redraw_curbuf_later_uses_the_current_buffer() {
        let mut buf_a = BufT::default();
        let mut buf_b = BufT::default();
        let mut win_b = visible_win(&mut buf_b);
        let mut win_a = visible_win(&mut buf_a);
        win_a.w_next = &mut win_b;
        let _guard = RedrawTestGuard::set(&mut win_a, &mut buf_b);

        unsafe { redraw_curbuf_later(UPD_REDRAW_TOP) };
        assert_eq!(win_b.w_redr_type, UPD_REDRAW_TOP);
        assert_eq!(win_a.w_redr_type, 0);
    }

    #[test]
    fn redraw_win_range_later_records_the_widest_range_seen() {
        let mut buf = BufT::default();
        let mut win = visible_win(&mut buf);
        let _guard = RedrawTestGuard::set(&mut win, &mut buf);

        unsafe { redraw_win_range_later(&mut win, 4, 6) };
        assert_eq!(win.w_redraw_top, 4);
        assert_eq!(win.w_redraw_bot, 6);
        assert_eq!(win.w_redr_type, UPD_VALID);

        // Widen at both ends.
        unsafe { redraw_win_range_later(&mut win, 2, 9) };
        assert_eq!(win.w_redraw_top, 2);
        assert_eq!(win.w_redraw_bot, 9);

        // A narrower range does not shrink what is already pending.
        unsafe { redraw_win_range_later(&mut win, 5, 5) };
        assert_eq!(win.w_redraw_top, 2);
        assert_eq!(win.w_redraw_bot, 9);
    }

    #[test]
    fn redraw_win_range_later_ignores_ranges_outside_the_window() {
        let mut buf = BufT::default();
        let mut win = visible_win(&mut buf);
        let _guard = RedrawTestGuard::set(&mut win, &mut buf);

        // Entirely above w_topline.
        unsafe { redraw_win_range_later(&mut win, 0, 0) };
        assert_eq!(win.w_redraw_top, 0);
        assert_eq!(win.w_redr_type, 0);

        // Entirely at/below w_botline.
        unsafe { redraw_win_range_later(&mut win, 11, 20) };
        assert_eq!(win.w_redraw_bot, 0);
        assert_eq!(win.w_redr_type, 0);
    }

    #[test]
    fn redraw_win_line_schedules_a_single_line() {
        let mut buf = BufT::default();
        let mut win = visible_win(&mut buf);
        let _guard = RedrawTestGuard::set(&mut win, &mut buf);

        unsafe { redraw_win_line(&mut win, 5) };
        assert_eq!(win.w_redraw_top, 5);
        assert_eq!(win.w_redraw_bot, 5);
    }

    #[test]
    fn redraw_buf_line_later_clamps_to_the_last_line() {
        let mut buf = BufT {
            b_ml: crate::memline_defs::MemlineT {
                ml_line_count: 3,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut win = visible_win(&mut buf);
        let _guard = RedrawTestGuard::set(&mut win, &mut buf);

        // Line 8 is past the end, so it is clamped to line 3.
        unsafe { redraw_buf_line_later(&mut buf, 8, false) };
        assert_eq!(win.w_redraw_top, 3);
        assert_eq!(win.w_redraw_bot, 3);
    }

    #[test]
    fn redraw_buf_line_later_with_force_extends_past_the_last_line() {
        let mut buf = BufT {
            b_ml: crate::memline_defs::MemlineT {
                ml_line_count: 3,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut win = visible_win(&mut buf);
        let _guard = RedrawTestGuard::set(&mut win, &mut buf);

        unsafe { redraw_buf_line_later(&mut buf, 8, true) };
        assert_eq!(win.w_redraw_top, 3, "clamped for the range itself");
        assert_eq!(win.w_redraw_bot, 8, "but force pushes the bottom out");
    }

    #[test]
    fn redraw_buf_range_later_only_touches_windows_on_that_buffer() {
        let mut buf_a = BufT::default();
        let mut buf_b = BufT::default();
        let mut win_b = visible_win(&mut buf_b);
        let mut win_a = visible_win(&mut buf_a);
        win_a.w_next = &mut win_b;
        let _guard = RedrawTestGuard::set(&mut win_a, &mut buf_a);

        unsafe { redraw_buf_range_later(&mut buf_a, 2, 4) };
        assert_eq!(win_a.w_redraw_top, 2);
        assert_eq!(win_a.w_redraw_bot, 4);
        assert_eq!(win_b.w_redraw_top, 0);
        assert_eq!(win_b.w_redraw_bot, 0);
    }

    #[test]
    fn number_width_absolute_mode_counts_digits() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 123;
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        // 123 has 3 digits; 'numberwidth' defaults to 0 here so the
        // `n.max(nuw - 1)` clamp doesn't raise it.
        assert_eq!(unsafe { number_width(&mut win) }, 3);
    }

    #[test]
    fn number_width_caches_until_line_count_changes() {
        let mut buf = BufT { ..Default::default() };
        let buf_ptr: *mut BufT = &mut buf;
        // Mutate the buffer only through `buf_ptr`, the same pointer
        // `win.w_buffer` holds. Writing through the `buf` variable
        // directly after that pointer is live is a foreign write that
        // disables its lineage, making `number_width`'s own reborrow
        // undefined behaviour under Tree Borrows.
        unsafe { (*buf_ptr).b_ml.ml_line_count = 5 };
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        assert_eq!(unsafe { number_width(&mut win) }, 1);

        // Corrupt the cached width directly - if a second call with the
        // SAME line count truly hits the cache (rather than recomputing),
        // it returns this corrupted value instead of the real one (1).
        win.w_nrwidth_width = 99;
        assert_eq!(unsafe { number_width(&mut win) }, 99);

        // Changing the line count invalidates the cache and forces a
        // genuine recompute.
        unsafe { (*buf_ptr).b_ml.ml_line_count = 50000 };
        assert_eq!(unsafe { number_width(&mut win) }, 5);
    }

    #[test]
    fn number_width_relativenumber_without_number_uses_view_height() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 999_999; // irrelevant in this mode
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_rnu = 1;
        win.w_onebuf_opt.wo_nu = 0;
        win.w_view_height = 42; // 2 digits

        assert_eq!(unsafe { number_width(&mut win) }, 2);
    }

    #[test]
    fn number_width_numberwidth_option_raises_minimum() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5; // 1 digit
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_nuw = 6; // minimal width is nuw - 1 = 5

        assert_eq!(unsafe { number_width(&mut win) }, 5);
    }

    #[test]
    fn number_width_statuscolumn_set_uses_nu_rnu_times_nuw() {
        let mut buf = BufT { ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_stc = Some(b"%n".to_vec());
        win.w_onebuf_opt.wo_nu = 1;
        win.w_onebuf_opt.wo_nuw = 4;
        // Force a cache-miss on this first call - lnum computed from the
        // default ml_line_count (0) would otherwise trivially equal the
        // also-defaulted w_nrwidth_line_count (0), short-circuiting before
        // the statuscolumn branch below is ever reached.
        win.w_nrwidth_line_count = -1;

        assert_eq!(unsafe { number_width(&mut win) }, 4);
        assert_eq!(win.w_statuscol_line_count, 0);
    }

    #[test]
    fn number_width_statuscolumn_set_without_nu_or_rnu_is_zero() {
        let mut buf = BufT { ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_stc = Some(b"%n".to_vec());
        win.w_onebuf_opt.wo_nuw = 4;
        // Same cache-miss forcing as above.
        win.w_nrwidth_line_count = -1;

        assert_eq!(unsafe { number_width(&mut win) }, 0);
    }

    /// RAII guard saving/restoring `GLOBALS.firstwin`/`GLOBALS.curwin`
    /// around a `redraw_buf_status_later` test. Does NOT acquire its
    /// own lock (matching `mark.rs`'s own `FirstwinGuard`/
    /// `NamedfmGuard` precedent) - callers must hold
    /// `crate::globals::global_state_test_lock()` for this guard's
    /// entire lifetime.
    struct WinGlobalsGuard {
        prev_firstwin: *mut WinT,
        prev_curwin: *mut WinT,
    }

    impl WinGlobalsGuard {
        fn set(firstwin: *mut WinT, curwin: *mut WinT) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_firstwin = g.firstwin;
            let prev_curwin = g.curwin;
            g.firstwin = firstwin;
            g.curwin = curwin;
            WinGlobalsGuard { prev_firstwin, prev_curwin }
        }
    }

    impl Drop for WinGlobalsGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.firstwin = self.prev_firstwin;
            g.curwin = self.prev_curwin;
        }
    }

    #[test]
    fn redraw_buf_status_later_marks_a_matching_window_with_status_height() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, w_status_height: 1, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, std::ptr::null_mut());

        unsafe { redraw_buf_status_later(buf_ptr) };
        assert!(unsafe { &*win_ptr }.w_redr_status);
    }

    #[test]
    fn redraw_buf_status_later_ignores_a_window_editing_a_different_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut other_buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut win =
            WinT { w_buffer: &mut other_buf as *mut BufT, w_status_height: 1, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, std::ptr::null_mut());

        unsafe { redraw_buf_status_later(buf_ptr) };
        assert!(!unsafe { &*win_ptr }.w_redr_status);
    }

    #[test]
    fn redraw_buf_status_later_marks_curwin_with_global_statusline() {
        let _lock = crate::globals::global_state_test_lock();
        // p_ls == 3 makes global_stl_height() nonzero.
        let saved_p_ls = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls = 3;

        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        // wp == curwin, no w_status_height/w_winbar_height of its own.
        let _guard = WinGlobalsGuard::set(win_ptr, win_ptr);

        unsafe { redraw_buf_status_later(buf_ptr) };
        assert!(unsafe { &*win_ptr }.w_redr_status);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls = saved_p_ls;
    }

    #[test]
    fn redraw_buf_status_later_global_statusline_does_not_help_a_non_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let saved_p_ls = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls = 3;

        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        // curwin is a DIFFERENT (null) window - wp itself is not curwin,
        // so the global-statusline check must not apply to it.
        let _guard = WinGlobalsGuard::set(win_ptr, std::ptr::null_mut());

        unsafe { redraw_buf_status_later(buf_ptr) };
        assert!(!unsafe { &*win_ptr }.w_redr_status);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls = saved_p_ls;
    }

    #[test]
    fn redraw_buf_status_later_marks_a_window_with_winbar_height() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, w_winbar_height: 1, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, std::ptr::null_mut());

        unsafe { redraw_buf_status_later(buf_ptr) };
        assert!(unsafe { &*win_ptr }.w_redr_status);
    }

    #[test]
    fn redraw_buf_status_later_skips_a_matching_window_with_none_of_the_three_conditions() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, std::ptr::null_mut());

        unsafe { redraw_buf_status_later(buf_ptr) };
        assert!(!unsafe { &*win_ptr }.w_redr_status);
    }

    #[test]
    fn redraw_buf_status_later_walks_every_window_in_the_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut other_buf = BufT::default();

        // win2 (tail): matches buf, has a status height -> should be marked.
        let mut win2 = WinT { w_buffer: buf_ptr, w_status_height: 1, ..Default::default() };
        let win2_ptr = &mut win2 as *mut WinT;
        // win1 (head): edits a DIFFERENT buffer -> must not be marked,
        // and win2 (its own w_next) must still be reached and checked.
        let mut win1 = WinT {
            w_buffer: &mut other_buf as *mut BufT,
            w_status_height: 1,
            w_next: win2_ptr,
            ..Default::default()
        };
        let win1_ptr = &mut win1 as *mut WinT;
        let _guard = WinGlobalsGuard::set(win1_ptr, std::ptr::null_mut());

        unsafe { redraw_buf_status_later(buf_ptr) };
        assert!(!unsafe { &*win1_ptr }.w_redr_status);
        assert!(unsafe { &*win2_ptr }.w_redr_status);
    }

    // --- comp_col ---

    /// Resets every global [`comp_col`] reads/writes to a known,
    /// neutral state.
    fn reset_comp_col_globals() {
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.ru_wid = 0;
        g.Columns = 80;
        g.sc_col = 0;
        g.ru_col = 0;
        let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        ov.p_ru = 0;
        ov.p_sc = 0;
        ov.p_sloc = None;
    }

    #[test]
    fn comp_col_with_everything_off_yields_full_width_sc_col_and_ru_col() {
        let _lock = crate::globals::global_state_test_lock();
        reset_comp_col_globals();
        let mut win = WinT { handle: crate::window::LOWEST_WIN_ID, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, win_ptr);

        unsafe { comp_col() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        // Neither 'ruler' nor 'showcmd'/'l' are set, so sc_col starts
        // at 0 -> Columns - 0 == 80; ru_col starts at 0 -> Columns - 0
        // == 80 too (still computed unconditionally, matching the
        // original).
        assert_eq!(g.sc_col, 80);
        assert_eq!(g.ru_col, 80);
        reset_comp_col_globals();
    }

    #[test]
    fn comp_col_with_ruler_and_single_window_reserves_ru_col_width_in_sc_col() {
        let _lock = crate::globals::global_state_test_lock();
        reset_comp_col_globals();
        let mut win = WinT { handle: crate::window::LOWEST_WIN_ID, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, win_ptr);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ru = 1;

        unsafe { comp_col() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        // last_has_status is false (single window, default 'laststatus'
        // doesn't force one) so sc_col == ru_col == Columns -
        // (COL_RULER + 1) == 80 - 18 == 62.
        assert_eq!(g.ru_col, 62);
        assert_eq!(g.sc_col, 62);
        reset_comp_col_globals();
    }

    #[test]
    fn comp_col_with_ru_wid_override_uses_it_instead_of_col_ruler() {
        let _lock = crate::globals::global_state_test_lock();
        reset_comp_col_globals();
        let mut win = WinT { handle: crate::window::LOWEST_WIN_ID, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, win_ptr);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ru = 1;
        unsafe { crate::globals::GLOBALS.get_mut() }.ru_wid = 30;

        unsafe { comp_col() };

        // ru_col == Columns - (ru_wid + 1) == 80 - 31 == 49.
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.ru_col, 49);
        reset_comp_col_globals();
    }

    #[test]
    fn comp_col_with_showcmd_at_line_adds_showcmd_cols_and_a_separator_space() {
        let _lock = crate::globals::global_state_test_lock();
        reset_comp_col_globals();
        let mut win = WinT { handle: crate::window::LOWEST_WIN_ID, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, win_ptr);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sc = 1;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sloc = Some(b"last".to_vec());

        unsafe { comp_col() };

        // 'ruler' is off, so the "!p_ru || last_has_status" separator
        // condition is true (p_ru == 0) -> +1 extra:
        // sc_col == Columns - (SHOWCMD_COLS + 1) == 80 - 11 == 69.
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.sc_col, 69);
        reset_comp_col_globals();
    }

    #[test]
    fn comp_col_showcmd_at_column_is_not_recognized_as_line() {
        let _lock = crate::globals::global_state_test_lock();
        reset_comp_col_globals();
        let mut win = WinT { handle: crate::window::LOWEST_WIN_ID, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, win_ptr);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sc = 1;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sloc = Some(b"statusline".to_vec());

        unsafe { comp_col() };

        // p_sloc doesn't start with 'l' ("statusline" starts with
        // 's'), so the whole SHOWCMD_COLS branch is skipped entirely.
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.sc_col, 80);
        reset_comp_col_globals();
    }

    #[test]
    fn comp_col_clamps_to_1_when_the_screen_is_too_narrow() {
        let _lock = crate::globals::global_state_test_lock();
        reset_comp_col_globals();
        let mut win = WinT { handle: crate::window::LOWEST_WIN_ID, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, win_ptr);
        unsafe { crate::globals::GLOBALS.get_mut() }.Columns = 5;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ru = 1;

        unsafe { comp_col() };

        // ru_col would be 5 - 18 == -13, clamped to 1; sc_col follows
        // the same clamp (ru_col's own pre-clamp value, since
        // last_has_status is false here too).
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.ru_col, 1);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.sc_col, 1);
        reset_comp_col_globals();
    }

    #[test]
    fn comp_col_sets_v_echospace_to_sc_col_minus_1() {
        let _lock = crate::globals::global_state_test_lock();
        reset_comp_col_globals();
        let mut win = WinT { handle: crate::window::LOWEST_WIN_ID, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, win_ptr);

        unsafe { comp_col() };

        let sc_col = unsafe { crate::globals::GLOBALS.get_mut() }.sc_col;
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_nr(crate::eval::vars::VimVarIndex::Echospace) },
            i64::from(sc_col - 1)
        );
        reset_comp_col_globals();
    }
}

