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
