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
//! Deferred: everything else in the file.

use crate::buffer_defs::{BufT, WinT};
use crate::pos_defs::LinenrT;
use crate::window::global_stl_height;

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
        buf.b_ml.ml_line_count = 5;
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        assert_eq!(unsafe { number_width(&mut win) }, 1);

        // Corrupt the cached width directly - if a second call with the
        // SAME line count truly hits the cache (rather than recomputing),
        // it returns this corrupted value instead of the real one (1).
        win.w_nrwidth_width = 99;
        assert_eq!(unsafe { number_width(&mut win) }, 99);

        // Changing the line count invalidates the cache and forces a
        // genuine recompute.
        buf.b_ml.ml_line_count = 50000;
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

