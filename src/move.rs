//! Translated from `src/nvim/move.c` (tractable core only).
//!
//! `move.c` is neovim's cursor/window-scrolling-position file
//! (`curs_columns`, `scroll_cursor_top`, etc., thousands of lines) -
//! most of it deeply tied to the display/redraw pipeline (`w_topline`/
//! `w_botline`/screen-row bookkeeping, `redraw_later`, folding), a
//! separate rendering-subsystem undertaking (phase 9).
//!
//! Translated: `win_col_off`/`win_col_off2` (needed by `plines.c`'s
//! `in_win_border`), `set_valid_virtcol` (needed by `cursor.c`'s
//! `coladvance`/`coladvance_force`), and now, with `plines.c`'s
//! `getvvcol` available: `check_cursor_moved`, `validate_virtcol`,
//! `validate_cursor_col`, `update_curswant`/`update_curswant_force`,
//! `cursor_valid`, plus the trivial `w_valid`-bit-clearing family
//! `changed_cline_bef_curs`/`changed_line_abv_curs`/
//! `changed_line_abv_curs_win`/`invalidate_botline_win`/
//! `approximate_botline_win`. Each of the non-trivial functions omits
//! the same kind of pure redraw-scheduling side effect already
//! established for `set_valid_virtcol` (`redraw_for_cursorcolumn`), and
//! `check_cursor_moved`'s own "concealed line visibility toggled"
//! inner branch (reached only when `wp == curwin`,
//! `w_valid_cursor.lnum > 0`, AND `'conceallevel' >= 2` - a narrow,
//! opt-in-only combination) is `unimplemented!()`: it needs
//! `decoration.c`'s `conceal_cursor_line`/`decor_conceal_line`, neither
//! translated yet.
//!
//! `validate_botline_win` (which would otherwise be a trivial one-line
//! wrapper) was investigated and NOT translated: its real work,
//! `comp_botline`, needs `plines_correct_topline` (fold-aware,
//! `fold.c`'s real tree search), `redraw_for_cursorline`/
//! `set_empty_rows`/`win_check_anchored_floats` (redraw + floating-
//! window machinery) - genuinely substantial, not a quick win.
//!
//! Deferred: everything else (window-scrolling/`w_topline`/`w_botline`
//! maintenance, `curs_columns`'s full screen-row/column computation,
//! `validate_cursor`/`curs_rows`/`validate_cheight`/`validate_botline_win`,
//! all needing `fold.c`'s real fold-tree search and/or the redraw
//! pipeline).

use crate::buffer_defs::{w_valid, WinT};
use crate::types_defs::SIGN_WIDTH;

/// The `number_width(wp) + (*wp->w_p_stc == NUL)` expression shared by
/// both `win_col_off` and `win_col_off2` below (the original doesn't
/// share this as a named helper, but the two real functions are
/// otherwise identical here - a private helper avoids duplicating the
/// same logic twice for no behavioral reason).
///
/// # Safety
/// Same as [`crate::drawscreen::number_width`].
unsafe fn num_col_width(wp: &mut WinT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let nw = unsafe { crate::drawscreen::number_width(wp) };
    let stc_is_empty = wp.w_onebuf_opt.wo_stc.as_deref().is_none_or(<[u8]>::is_empty);
    nw + i32::from(stc_is_empty)
}

fn has_num_col(wp: &WinT) -> bool {
    wp.w_onebuf_opt.wo_nu != 0
        || wp.w_onebuf_opt.wo_rnu != 0
        || wp.w_onebuf_opt.wo_stc.as_deref().is_some_and(|s| !s.is_empty())
}

/// Return the number of columns used on the left of `wp` by the
/// `'number'`/`'relativenumber'`/`'statuscolumn'` column, the
/// `'foldcolumn'`, and the sign column (`win_col_off`).
///
/// # Safety
/// Same as [`crate::drawscreen::number_width`].
#[must_use]
pub unsafe fn win_col_off(wp: &mut WinT) -> i32 {
    let num_part = if has_num_col(wp) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { num_col_width(wp) }
    } else {
        0
    };

    num_part + crate::window::win_fdccol_count(wp) + wp.w_scwidth * SIGN_WIDTH
}

/// Return the difference in column offset for the second screen line
/// of a wrapped line: positive if `'number'`/`'relativenumber'` is on
/// and `'n'` is in `'cpoptions'` (`win_col_off2`).
///
/// # Safety
/// Same as [`crate::drawscreen::number_width`]. Also touches
/// `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn win_col_off2(wp: &mut WinT) -> i32 {
    let p_cpo = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo.clone();
    let has_n_cpo = p_cpo.as_deref().is_some_and(|s| {
        crate::strings::vim_strchr(s, i32::from(crate::option_vars::CPO_NUMCOL)).is_some()
    });

    if has_num_col(wp) && has_n_cpo {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { num_col_width(wp) };
    }
    0
}

/// Set `wp.w_virtcol`/`w_valid`'s `VALID_VIRTCOL` bit for virtual
/// column `vcol` (`set_valid_virtcol`).
///
/// Deviates from the original by omitting the
/// `redraw_for_cursorcolumn(wp)` call: a pure redraw-scheduling side
/// effect (marks dirty regions for `'cursorcolumn'`/`'cursorlineopt'=
/// "screenline"` redraws) that doesn't feed back into any value this
/// crate currently computes - matches the established
/// "skip the deferred-subsystem side effect, keep the state correct"
/// policy (e.g. `mf_write`/`ml_open`/`u_sync`'s omitted `iemsg`/`emsg`
/// calls). `redraw_for_cursorcolumn` itself needs `conceal_cursor_line`/
/// `redrawWinline`/`redraw_later` (decoration.c/drawscreen.c's redraw-
/// tracking side, not yet translated).
pub fn set_valid_virtcol(wp: &mut WinT, vcol: crate::pos_defs::ColnrT) {
    wp.w_virtcol = vcol;
    wp.w_valid |= i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL);
}

/// Check if the cursor has moved. Set the `w_valid` flag accordingly
/// (`check_cursor_moved`).
///
/// The "concealed line visibility toggled" inner branch (reached only
/// when `wp == curwin`, `w_valid_cursor.lnum > 0`, AND
/// `'conceallevel' >= 2` - a narrow, opt-in-only combination) is
/// `unimplemented!()`: it needs `decoration.c`'s `conceal_cursor_line`/
/// `decor_conceal_line`, neither translated yet. Every other case
/// (in particular every call with `'conceallevel' < 2`, the default)
/// is fully translated.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn check_cursor_moved(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    if w.w_cursor.lnum != w.w_valid_cursor.lnum {
        w.w_valid &= !(i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_WCOL)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_CHEIGHT)
            | i32::from(w_valid::VALID_CROW)
            | i32::from(w_valid::VALID_TOPLINE));

        // Concealed line visibility toggled.
        // SAFETY: forwarded from this function's own safety doc.
        let is_curwin = std::ptr::eq(wp, unsafe { crate::globals::GLOBALS.get_mut() }.curwin);
        if is_curwin && w.w_valid_cursor.lnum > 0 && w.w_onebuf_opt.wo_cole >= 2 {
            unimplemented!(
                "check_cursor_moved: the concealed-line-visibility-toggled branch needs \
                 decoration.c's conceal_cursor_line/decor_conceal_line"
            );
        }
        w.w_valid_cursor = w.w_cursor;
        w.w_valid_leftcol = w.w_leftcol;
        w.w_valid_skipcol = w.w_skipcol;
        w.w_viewport_invalid = true;
    } else if w.w_skipcol != w.w_valid_skipcol {
        w.w_valid &= !(i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_WCOL)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_CHEIGHT)
            | i32::from(w_valid::VALID_CROW)
            | i32::from(w_valid::VALID_BOTLINE)
            | i32::from(w_valid::VALID_BOTLINE_AP));
        w.w_valid_cursor = w.w_cursor;
        w.w_valid_leftcol = w.w_leftcol;
        w.w_valid_skipcol = w.w_skipcol;
    } else if w.w_cursor.col != w.w_valid_cursor.col
        || w.w_leftcol != w.w_valid_leftcol
        || w.w_cursor.coladd != w.w_valid_cursor.coladd
    {
        w.w_valid &=
            !(i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_WCOL) | i32::from(
                w_valid::VALID_VIRTCOL,
            ));
        w.w_valid_cursor.col = w.w_cursor.col;
        w.w_valid_leftcol = w.w_leftcol;
        w.w_valid_cursor.coladd = w.w_cursor.coladd;
        w.w_viewport_invalid = true;
    }
}

/// Validate `wp.w_virtcol` only (`validate_virtcol`).
///
/// Omits the original's `redraw_for_cursorcolumn(wp)` call - a pure
/// redraw-scheduling side effect, matching [`set_valid_virtcol`]'s own
/// precedent.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid.
pub unsafe fn validate_virtcol(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_cursor_moved(wp) };

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*wp }.w_valid & i32::from(w_valid::VALID_VIRTCOL) != 0 {
        return;
    }

    let mut virtcol = 0;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::plines::getvvcol(wp, &mut (*wp).w_cursor, None, Some(&mut virtcol), None, 0);
    }
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    w.w_virtcol = virtcol;
    w.w_valid |= i32::from(w_valid::VALID_VIRTCOL);
}

/// Force-update `wp.w_curswant` from `wp.w_virtcol`
/// (`update_curswant_force`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` whose own `w_buffer` is also valid.
pub unsafe fn update_curswant_force() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { validate_virtcol(curwin) };
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *curwin };
    w.w_curswant = w.w_virtcol;
    w.w_set_curswant = false;
}

/// Update `wp.w_curswant` if `wp.w_set_curswant` is set
/// (`update_curswant`).
///
/// # Safety
/// Same as [`update_curswant_force`].
pub unsafe fn update_curswant() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*curwin }.w_set_curswant {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { update_curswant_force() };
    }
}

/// @return true if `wp.w_wrow`/`wp.w_wcol` are both currently valid
/// (`cursor_valid`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
#[must_use]
pub unsafe fn cursor_valid(wp: *mut WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_cursor_moved(wp) };
    // SAFETY: forwarded from this function's own safety doc.
    let valid_flags = unsafe { &*wp }.w_valid;
    let want = i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_WCOL);
    (valid_flags & want) == want
}

/// Validate `wp.w_wcol` and `wp.w_virtcol` only (`validate_cursor_col`).
///
/// # Safety
/// Same as [`validate_virtcol`].
pub unsafe fn validate_cursor_col(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { validate_virtcol(wp) };

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*wp }.w_valid & i32::from(w_valid::VALID_WCOL) != 0 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    let mut col = w.w_virtcol;
    // SAFETY: forwarded from this function's own safety doc.
    let off = unsafe { win_col_off(w) };
    col += off;
    // SAFETY: forwarded from this function's own safety doc.
    let width = w.w_view_width - off + unsafe { win_col_off2(w) };

    // long line wrapping, adjust wp->w_wrow
    if w.w_onebuf_opt.wo_wrap != 0 && col >= w.w_view_width && width > 0 {
        // use same formula as what is used in curs_columns()
        col -= ((col - w.w_view_width) / width + 1) * width;
    }
    if col > w.w_leftcol {
        col -= w.w_leftcol;
    } else {
        col = 0;
    }
    w.w_wcol = col;

    w.w_valid |= i32::from(w_valid::VALID_WCOL);
}

/// Called when text before the cursor changed in a way that affects
/// its screen position - clears bits related to lines up to and
/// including the cursor's own line, but not `w_botline`
/// (`changed_cline_bef_curs`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn changed_cline_bef_curs(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_valid &= !(i32::from(w_valid::VALID_WROW)
        | i32::from(w_valid::VALID_WCOL)
        | i32::from(w_valid::VALID_VIRTCOL)
        | i32::from(w_valid::VALID_CROW)
        | i32::from(w_valid::VALID_CHEIGHT)
        | i32::from(w_valid::VALID_TOPLINE));
}

/// Call this when the length of a line (in screen characters) above
/// the cursor has changed. Need to take care of `w_botline`
/// separately! (`changed_line_abv_curs_win`)
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn changed_line_abv_curs_win(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_valid &= !(i32::from(w_valid::VALID_WROW)
        | i32::from(w_valid::VALID_WCOL)
        | i32::from(w_valid::VALID_VIRTCOL)
        | i32::from(w_valid::VALID_CROW)
        | i32::from(w_valid::VALID_CHEIGHT)
        | i32::from(w_valid::VALID_TOPLINE));
}

/// Like [`changed_line_abv_curs_win`], but for `curwin`
/// (`changed_line_abv_curs`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT`.
pub unsafe fn changed_line_abv_curs() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_line_abv_curs_win(curwin) };
}

/// Mark `wp.w_botline` as invalid, because of some change in the
/// buffer (`invalidate_botline_win`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn invalidate_botline_win(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_valid &=
        !(i32::from(w_valid::VALID_BOTLINE) | i32::from(w_valid::VALID_BOTLINE_AP));
}

/// Mark `wp.w_botline` as only approximately valid (`approximate_botline_win`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn approximate_botline_win(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_valid &= !i32::from(w_valid::VALID_BOTLINE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    fn win_with_buf(buf: *mut BufT) -> WinT {
        WinT { w_buffer: buf, ..Default::default() }
    }

    #[test]
    fn win_col_off_zero_when_nothing_enabled() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        assert_eq!(unsafe { win_col_off(&mut win) }, 0);
    }

    #[test]
    fn win_col_off_counts_number_column_and_foldcolumn_and_signcolumn() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5; // number_width == 1
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_nu = 1;
        win.w_onebuf_opt.wo_fdc = Some(b"2".to_vec());
        win.w_scwidth = 1;

        // number_width(1) + stc_empty(1, no statuscolumn) + fdccol(2)
        // + scwidth(1) * SIGN_WIDTH(2) = 1 + 1 + 2 + 2 = 6.
        assert_eq!(unsafe { win_col_off(&mut win) }, 6);
    }

    #[test]
    fn win_col_off_statuscolumn_set_excludes_the_plus_one() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_stc = Some(b"%n".to_vec());
        win.w_onebuf_opt.wo_nuw = 3;

        // has_num_col via non-empty w_p_stc; num_col_width = number_width
        // (3, from the 'statuscolumn' branch: (nu||rnu)=0 so 0*nuw=0... )
        // wait: nu/rnu both 0 here, so number_width's own stc-branch gives
        // 0 * nuw = 0; stc_is_empty is false (stc is set) so +0.
        assert_eq!(unsafe { win_col_off(&mut win) }, 0);
    }

    #[test]
    fn win_col_off2_zero_without_cpo_n_flag() {
        // win_col_off2 reads the shared OPTION_VARS.p_cpo internally, even
        // though this test never touches it explicitly - must still hold
        // the lock so a concurrently-running test that DOES mutate p_cpo
        // (e.g. win_col_off2_nonzero_with_cpo_n_flag_and_number_column)
        // can't be observed mid-mutation.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_nu = 1;
        // p_cpo left at its default (None) - no 'n' flag present.
        assert_eq!(unsafe { win_col_off2(&mut win) }, 0);
    }

    #[test]
    fn win_col_off2_nonzero_with_cpo_n_flag_and_number_column() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo = Some(b"n".to_vec());

        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5; // number_width == 1
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_nu = 1;

        assert_eq!(unsafe { win_col_off2(&mut win) }, 2); // number_width(1) + stc_empty(1)

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo = prev;
    }

    #[test]
    fn check_cursor_moved_lnum_change_clears_lnum_related_bits() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_WCOL)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_CHEIGHT)
            | i32::from(w_valid::VALID_CROW)
            | i32::from(w_valid::VALID_TOPLINE)
            | i32::from(w_valid::VALID_BOTLINE); // extra bit that should survive
        win.w_cursor.lnum = 5;
        win.w_valid_cursor.lnum = 1; // different -> triggers the lnum branch

        unsafe { check_cursor_moved(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_BOTLINE));
        assert_eq!(win.w_valid_cursor.lnum, 5);
        assert!(win.w_viewport_invalid);
    }

    #[test]
    fn check_cursor_moved_skipcol_change_clears_different_bits() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_BOTLINE)
            | i32::from(w_valid::VALID_TOPLINE); // should survive
        win.w_cursor.lnum = 1;
        win.w_valid_cursor.lnum = 1; // same -> skip the lnum branch
        win.w_skipcol = 3;
        win.w_valid_skipcol = 0; // different -> triggers the skipcol branch

        unsafe { check_cursor_moved(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_TOPLINE));
        assert_eq!(win.w_valid_skipcol, 3);
    }

    #[test]
    fn check_cursor_moved_col_change_clears_col_related_bits() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_WCOL)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_CHEIGHT); // should survive
        win.w_cursor.lnum = 1;
        win.w_valid_cursor.lnum = 1;
        win.w_skipcol = 0;
        win.w_valid_skipcol = 0;
        win.w_cursor.col = 4;
        win.w_valid_cursor.col = 1; // different -> triggers the col branch

        unsafe { check_cursor_moved(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_CHEIGHT));
        assert_eq!(win.w_valid_cursor.col, 4);
        assert!(win.w_viewport_invalid);
    }

    #[test]
    fn check_cursor_moved_noop_when_nothing_changed() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_WCOL);
        // w_valid_cursor/w_leftcol/w_skipcol all match w_cursor's
        // defaults already (all zero) - nothing should change.

        unsafe { check_cursor_moved(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_WCOL));
    }

    #[test]
    fn check_cursor_moved_panics_on_conceal_branch_when_curwin_and_conceallevel_2() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_cole = 2;
        win.w_cursor.lnum = 5;
        win.w_valid_cursor.lnum = 1; // > 0 and different from w_cursor.lnum

        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        // catch_unwind (rather than #[should_panic]) so curwin is
        // always restored before this test returns, even though the
        // call panics - otherwise GLOBALS.curwin would dangle,
        // pointing at this test's about-to-be-dropped local `win`.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            check_cursor_moved(&mut win as *mut WinT);
        }));

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;

        let err = result.expect_err("expected check_cursor_moved to panic");
        let msg = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_default();
        assert!(
            msg.contains("concealed-line-visibility-toggled"),
            "unexpected panic message: {msg}"
        );
    }

    #[test]
    fn validate_virtcol_computes_and_marks_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 3, coladd: 0 };

        unsafe { validate_virtcol(&mut win as *mut WinT) };

        assert_eq!(win.w_virtcol, 3); // plain ASCII 'l' at col 3 in "hello"
        assert_ne!(win.w_valid & i32::from(w_valid::VALID_VIRTCOL), 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn validate_cursor_col_basic_no_wrap_no_leftcol() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 };

        unsafe { validate_cursor_col(&mut win as *mut WinT) };

        assert_eq!(win.w_wcol, 2);
        assert_ne!(win.w_valid & i32::from(w_valid::VALID_WCOL), 0);
        assert_ne!(win.w_valid & i32::from(w_valid::VALID_VIRTCOL), 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn validate_cursor_col_short_circuits_when_already_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 };
        // w_valid_cursor must match w_cursor (and w_valid_leftcol/
        // w_valid_skipcol match their counterparts) so the internal
        // check_cursor_moved call (via validate_virtcol) is a true
        // no-op and doesn't clear the bits pre-marked below.
        win.w_valid_cursor = win.w_cursor;
        // Pre-mark both VALID_VIRTCOL and VALID_WCOL, with a
        // deliberately WRONG w_wcol - it must be left untouched since
        // the function should short-circuit without recomputing.
        win.w_valid = i32::from(w_valid::VALID_VIRTCOL) | i32::from(w_valid::VALID_WCOL);
        win.w_wcol = 999;

        unsafe { validate_cursor_col(&mut win as *mut WinT) };

        assert_eq!(win.w_wcol, 999);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn validate_cursor_col_clamps_to_zero_when_leftcol_exceeds_col() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 };
        win.w_leftcol = 5; // scrolled right past the cursor's own column

        unsafe { validate_cursor_col(&mut win as *mut WinT) };

        assert_eq!(win.w_wcol, 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn cursor_valid_true_when_wrow_and_wcol_both_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_WCOL);
        // Keep w_valid_cursor/w_leftcol/w_skipcol matching defaults so
        // check_cursor_moved (called internally) doesn't clear them.
        assert!(unsafe { cursor_valid(&mut win as *mut WinT) });
    }

    #[test]
    fn cursor_valid_false_when_only_wrow_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW);
        assert!(!unsafe { cursor_valid(&mut win as *mut WinT) });
    }

    #[test]
    fn update_curswant_force_copies_virtcol_and_clears_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 4, coladd: 0 };
        win.w_set_curswant = true;

        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        unsafe { update_curswant_force() };

        assert_eq!(win.w_curswant, 4);
        assert!(!win.w_set_curswant);

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn update_curswant_is_noop_when_flag_not_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_cursor = crate::pos_defs::PosT { lnum: 0, col: 4, coladd: 0 };
        win.w_set_curswant = false;
        win.w_curswant = 99;

        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        unsafe { update_curswant() };

        assert_eq!(win.w_curswant, 99); // untouched

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn changed_cline_bef_curs_clears_expected_bits_only() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_WCOL)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_CROW)
            | i32::from(w_valid::VALID_CHEIGHT)
            | i32::from(w_valid::VALID_TOPLINE)
            | i32::from(w_valid::VALID_BOTLINE); // must survive

        unsafe { changed_cline_bef_curs(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_BOTLINE));
    }

    #[test]
    fn changed_line_abv_curs_win_clears_expected_bits_only() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_TOPLINE)
            | i32::from(w_valid::VALID_BOTLINE); // must survive

        unsafe { changed_line_abv_curs_win(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_BOTLINE));
    }

    #[test]
    fn changed_line_abv_curs_operates_on_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_BOTLINE);

        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        unsafe { changed_line_abv_curs() };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_BOTLINE));

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn invalidate_botline_win_clears_both_botline_bits() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_BOTLINE)
            | i32::from(w_valid::VALID_BOTLINE_AP)
            | i32::from(w_valid::VALID_WROW); // must survive

        unsafe { invalidate_botline_win(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_WROW));
    }

    #[test]
    fn approximate_botline_win_clears_only_botline() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_BOTLINE) | i32::from(w_valid::VALID_BOTLINE_AP); // must survive

        unsafe { approximate_botline_win(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_BOTLINE_AP));
    }
}
