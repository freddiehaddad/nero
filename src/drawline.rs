//! Translated from `src/nvim/drawline.c` (tractable core only).
//!
//! `drawline.c` (~3400 lines) is the real screen-line-rendering
//! engine (`win_line`, the single most complex drawing function in
//! the original, plus fold-column/virtual-text/decoration-provider
//! setup around it). Almost every function needs the real screen
//! grid (`ScreenGrid`/`schar_T` cell buffers) and/or the decoration-
//! provider Lua-callback machinery, neither translated.
//!
//! Translated: `use_cursor_line_highlight` (whether `CursorLineSign`
//! highlighting applies to a given line) and `get_rightmost_vcol`
//! (the rightmost virtual column that `'cursorcolumn'`/
//! `'colorcolumn'` would draw at). Both are small, self-contained,
//! `static`-in-the-original helpers with no design freedom of their
//! own - translated ahead of their real callers (`draw_foldcolumn`/
//! `get_line_number_attr`/`win_line` itself, none translated),
//! matching this crate's established "translate a small, simple,
//! mechanically-correct piece ahead of the surrounding engine"
//! precedent (e.g. `ops.rs`'s `reset_lbr`/`restore_lbr`).
//!
//! `get_rightmost_vcol`'s `color_cols` parameter deviates from the
//! original's raw `const int *` (`-1`-sentinel-terminated array): it's
//! a plain `Option<&[i32]>` slice with no sentinel needed, matching
//! this crate's usual "idiomatic Rust equivalent, not the exact C
//! representation" convention - there is no real caller yet to
//! populate a genuine sentinel-terminated array either way (`'colorcolumn'`
//! parsing isn't translated).
//!
//! Also `margin_columns_win` (the margins between which
//! `'cursorlineopt'`'s `"screenline"` highlighting applies). It
//! returns a `(left_col, right_col)` tuple rather than using two out
//! parameters, and the original's six-file-static result cache is
//! omitted: that cache is a pure speed optimization keyed on the
//! window pointer and its virtual column, with no observable effect,
//! so reproducing it would add mutable statics for nothing.
//!
//! Deferred: everything else in the file.

use crate::buffer_defs::WinT;
use crate::pos_defs::LinenrT;

/// Whether `CursorLineSign` highlighting is to be used for line
/// `lnum` in window `wp` (`use_cursor_line_highlight`).
#[must_use]
pub fn use_cursor_line_highlight(wp: &WinT, lnum: LinenrT) -> bool {
    wp.w_onebuf_opt.wo_cul != 0
        && lnum == wp.w_cursorline
        && (wp.w_p_culopt_flags & crate::option_vars::opt_culopt_flag::NUMBER as u8) != 0
}

/// Compute the margins between which `'cursorlineopt'`'s
/// `"screenline"` highlighting is used (`margin_columns_win`).
///
/// Returns `(left_col, right_col)`. The original takes both as out
/// parameters and additionally caches the last result in six file
/// statics keyed on the window pointer and its virtual column; that
/// cache is a pure speed optimization with no observable effect, so
/// it is omitted here rather than reproduced as mutable statics.
///
/// # Safety
/// Forwarded from `crate::r#move::win_col_off`'s own safety doc.
#[must_use]
pub unsafe fn margin_columns_win(wp: &mut WinT) -> (i32, i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let cur_col_off = unsafe { crate::r#move::win_col_off(wp) };
    let width1 = wp.w_view_width - cur_col_off;
    // SAFETY: forwarded from this function's own safety doc.
    let width2 = width1 + unsafe { crate::r#move::win_col_off2(wp) };

    let mut left_col = 0;
    let mut right_col = width1;

    if wp.w_virtcol >= width1 && width2 > 0 {
        right_col = width1 + ((wp.w_virtcol - width1) / width2 + 1) * width2;
        left_col = (wp.w_virtcol - width1) / width2 * width2 + width1;
    }

    (left_col, right_col)
}

/// The rightmost virtual column that `'cursorcolumn'`/
/// `'colorcolumn'` would draw at (`get_rightmost_vcol`). `color_cols`
/// is `None` when `'colorcolumn'` is unset/empty, matching the
/// original's own `NULL` case.
#[must_use]
pub fn get_rightmost_vcol(wp: &WinT, color_cols: Option<&[i32]>) -> i32 {
    let mut ret = 0;
    if wp.w_onebuf_opt.wo_cuc != 0 {
        ret = wp.w_virtcol;
    }
    if let Some(cols) = color_cols {
        for &c in cols {
            ret = ret.max(c);
        }
    }
    ret
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- margin_columns_win ----

    #[test]
    fn margin_columns_win_covers_the_first_screen_line_when_virtcol_fits() {
        // A window with no number/fold/sign columns: width1 is the
        // full view width, and a cursor inside it needs no wrapping,
        // so the margins are the whole line.
        let mut wp = WinT {
            w_view_width: 80,
            w_virtcol: 10,
            ..Default::default()
        };
        assert_eq!(unsafe { margin_columns_win(&mut wp) }, (0, 80));
    }

    #[test]
    fn margin_columns_win_advances_by_whole_screen_lines_when_wrapped() {
        // With w_virtcol past width1 the margins step forward in
        // width2-sized chunks. Here width1 == width2 == 80, so a
        // virtcol of 100 lands on the second screen line: 80..160.
        let mut wp = WinT {
            w_view_width: 80,
            w_virtcol: 100,
            ..Default::default()
        };
        assert_eq!(unsafe { margin_columns_win(&mut wp) }, (80, 160));

        // A virtcol on the third screen line gives 160..240.
        wp.w_virtcol = 200;
        assert_eq!(unsafe { margin_columns_win(&mut wp) }, (160, 240));
    }

    #[test]
    fn margin_columns_win_left_edge_is_exactly_width1_at_the_boundary() {
        // At virtcol == width1 the cursor is the FIRST cell of the
        // second screen line, so the left margin is width1 itself.
        let mut wp = WinT {
            w_view_width: 80,
            w_virtcol: 80,
            ..Default::default()
        };
        assert_eq!(unsafe { margin_columns_win(&mut wp) }, (80, 160));

        // One cell earlier it is still on the first screen line.
        wp.w_virtcol = 79;
        assert_eq!(unsafe { margin_columns_win(&mut wp) }, (0, 80));
    }

    #[test]
    fn use_cursor_line_highlight_true_when_all_conditions_hold() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_cul = 1;
        wp.w_cursorline = 5;
        wp.w_p_culopt_flags = crate::option_vars::opt_culopt_flag::NUMBER as u8;
        assert!(use_cursor_line_highlight(&wp, 5));
    }

    #[test]
    fn use_cursor_line_highlight_false_when_cul_is_off() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_cul = 0;
        wp.w_cursorline = 5;
        wp.w_p_culopt_flags = crate::option_vars::opt_culopt_flag::NUMBER as u8;
        assert!(!use_cursor_line_highlight(&wp, 5));
    }

    #[test]
    fn use_cursor_line_highlight_false_for_a_different_line() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_cul = 1;
        wp.w_cursorline = 5;
        wp.w_p_culopt_flags = crate::option_vars::opt_culopt_flag::NUMBER as u8;
        assert!(!use_cursor_line_highlight(&wp, 6));
    }

    #[test]
    fn use_cursor_line_highlight_false_without_the_number_flag() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_cul = 1;
        wp.w_cursorline = 5;
        // "line" only, no "number"/"both".
        wp.w_p_culopt_flags = crate::option_vars::opt_culopt_flag::LINE as u8;
        assert!(!use_cursor_line_highlight(&wp, 5));
    }

    #[test]
    fn use_cursor_line_highlight_true_with_both_flag() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_cul = 1;
        wp.w_cursorline = 5;
        wp.w_p_culopt_flags = (crate::option_vars::opt_culopt_flag::LINE
            | crate::option_vars::opt_culopt_flag::NUMBER) as u8;
        assert!(use_cursor_line_highlight(&wp, 5));
    }

    #[test]
    fn get_rightmost_vcol_zero_when_cuc_off_and_no_color_cols() {
        let wp = WinT::default();
        assert_eq!(get_rightmost_vcol(&wp, None), 0);
    }

    #[test]
    fn get_rightmost_vcol_uses_w_virtcol_when_cuc_is_set() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_cuc = 1;
        wp.w_virtcol = 42;
        assert_eq!(get_rightmost_vcol(&wp, None), 42);
    }

    #[test]
    fn get_rightmost_vcol_uses_the_max_color_column() {
        let wp = WinT::default();
        assert_eq!(get_rightmost_vcol(&wp, Some(&[10, 30, 20])), 30);
    }

    #[test]
    fn get_rightmost_vcol_takes_the_larger_of_cuc_and_color_cols() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_cuc = 1;
        wp.w_virtcol = 50;
        assert_eq!(get_rightmost_vcol(&wp, Some(&[10, 30, 20])), 50);

        wp.w_virtcol = 5;
        assert_eq!(get_rightmost_vcol(&wp, Some(&[10, 30, 20])), 30);
    }
}
