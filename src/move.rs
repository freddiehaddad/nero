//! Translated from `src/nvim/move.c` (tractable core only).
//!
//! `move.c` is neovim's cursor/window-scrolling-position file
//! (`curs_columns`, `scroll_cursor_top`, etc., thousands of lines) -
//! almost entirely dependent on the real screen-column/character-width
//! subsystem (`plines.c`'s `getvcol`, itself still substantially
//! deferred - see that module's own doc comment) and the display
//! pipeline. Only the two window-column-offset functions needed by
//! `plines.c`'s `in_win_border` are translated: `win_col_off`,
//! `win_col_off2`.
//!
//! Deferred: everything else in the file.

use crate::buffer_defs::WinT;
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
}
