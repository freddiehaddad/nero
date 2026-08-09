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
//! Also translated: [`get_lcs_ext`] (the `'listchars'` `"extends"`
//! character) and [`foldcolumn_sep_char`] (the `'fillchars'`
//! fold-level separator), both small `static` helpers of `win_line`'s
//! own drawing loop with no design freedom, translated ahead of their
//! real callers like the two above.
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

/// The `'listchars'` `"extends"` character to show that a line
/// continues beyond the right of the screen (`get_lcs_ext`).
///
/// Returns `NUL` when nothing should be shown.
#[must_use]
pub fn get_lcs_ext(wp: &WinT) -> crate::types_defs::ScharT {
    if wp.w_onebuf_opt.wo_wrap != 0 {
        // Line never continues beyond the right of the screen with
        // 'wrap'.
        return 0;
    }
    if wp.w_onebuf_opt.wo_wrap_flags & crate::option_defs::opt_flags::INSECURE != 0 {
        // If 'nowrap' was set from a modeline, forcibly use '>'.
        return crate::grid::schar_from_ascii(b'>');
    }
    if wp.w_onebuf_opt.wo_list != 0 {
        wp.w_p_lcs_chars.ext
    } else {
        0
    }
}

/// The `'fillchars'` character separating fold levels in the fold
/// column (`foldcolumn_sep_char`).
///
/// `first_level` is the fold level of the first (outermost) fold on
/// the line and `i` the offset of the column being drawn within it.
#[must_use]
pub fn foldcolumn_sep_char(first_level: i32, i: i32, wp: &WinT) -> crate::types_defs::ScharT {
    if first_level == 1 {
        wp.w_p_fcs_chars.foldsep
    } else if wp.w_p_fcs_chars.foldinner != 0 {
        wp.w_p_fcs_chars.foldinner
    } else if first_level + i <= 9 {
        // Only a single-digit level fits in one cell; the guard above
        // is what keeps this in '0'..='9'.
        crate::grid::schar_from_ascii((b'0' as i32 + first_level + i) as u8)
    } else {
        crate::grid::schar_from_ascii(b'>')
    }
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

    // ---- get_lcs_ext ----

    /// With 'wrap' on, a line never runs off the right edge, so the
    /// "extends" char must be suppressed even when 'list' is on and a
    /// char is configured.
    #[test]
    fn lcs_ext_is_suppressed_while_wrapping() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_wrap = 1;
        wp.w_onebuf_opt.wo_list = 1;
        wp.w_p_lcs_chars.ext = u32::from(b'#');
        assert_eq!(get_lcs_ext(&wp), 0);
    }

    /// A 'nowrap' coming from a modeline forces '>' - overriding both
    /// 'list' being off and any configured char.
    #[test]
    fn lcs_ext_is_forced_to_gt_when_nowrap_came_from_a_modeline() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_wrap = 0;
        wp.w_onebuf_opt.wo_wrap_flags = crate::option_defs::opt_flags::INSECURE;
        wp.w_onebuf_opt.wo_list = 0;
        wp.w_p_lcs_chars.ext = u32::from(b'#');
        assert_eq!(get_lcs_ext(&wp), crate::grid::schar_from_ascii(b'>'));
    }

    /// 'wrap' is checked BEFORE the insecure flag, so a wrapping
    /// window ignores the modeline override entirely.
    #[test]
    fn lcs_ext_prefers_wrap_over_the_modeline_override() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_wrap = 1;
        wp.w_onebuf_opt.wo_wrap_flags = crate::option_defs::opt_flags::INSECURE;
        assert_eq!(get_lcs_ext(&wp), 0);
    }

    #[test]
    fn lcs_ext_uses_the_configured_char_only_with_list_on() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_wrap = 0;
        wp.w_p_lcs_chars.ext = u32::from(b'#');

        wp.w_onebuf_opt.wo_list = 0;
        assert_eq!(get_lcs_ext(&wp), 0);

        wp.w_onebuf_opt.wo_list = 1;
        assert_eq!(get_lcs_ext(&wp), u32::from(b'#'));
    }

    // ---- foldcolumn_sep_char ----

    /// The outermost level uses 'foldsep', regardless of what
    /// 'foldinner' is set to.
    #[test]
    fn foldcolumn_sep_uses_foldsep_at_the_first_level() {
        let mut wp = WinT::default();
        wp.w_p_fcs_chars.foldsep = u32::from(b'|');
        wp.w_p_fcs_chars.foldinner = u32::from(b'!');
        assert_eq!(foldcolumn_sep_char(1, 0, &wp), u32::from(b'|'));
    }

    /// Deeper levels use 'foldinner' when it is set - and NOT the
    /// digit fallback, even though the level would fit in one cell.
    #[test]
    fn foldcolumn_sep_uses_foldinner_below_the_first_level() {
        let mut wp = WinT::default();
        wp.w_p_fcs_chars.foldsep = u32::from(b'|');
        wp.w_p_fcs_chars.foldinner = u32::from(b'!');
        assert_eq!(foldcolumn_sep_char(2, 0, &wp), u32::from(b'!'));
    }

    /// With 'foldinner' unset, the level itself is drawn as a digit.
    #[test]
    fn foldcolumn_sep_falls_back_to_the_level_digit() {
        let mut wp = WinT::default();
        wp.w_p_fcs_chars.foldsep = u32::from(b'|');
        wp.w_p_fcs_chars.foldinner = 0;
        assert_eq!(
            foldcolumn_sep_char(2, 0, &wp),
            crate::grid::schar_from_ascii(b'2')
        );
        // The offset within the fold advances the digit.
        assert_eq!(
            foldcolumn_sep_char(2, 3, &wp),
            crate::grid::schar_from_ascii(b'5')
        );
        assert_eq!(
            foldcolumn_sep_char(9, 0, &wp),
            crate::grid::schar_from_ascii(b'9')
        );
    }

    /// Past a single digit there is no room, so '>' is drawn. The
    /// boundary is on first_level + i, not first_level alone.
    #[test]
    fn foldcolumn_sep_uses_gt_past_a_single_digit() {
        let mut wp = WinT::default();
        wp.w_p_fcs_chars.foldinner = 0;
        assert_eq!(
            foldcolumn_sep_char(10, 0, &wp),
            crate::grid::schar_from_ascii(b'>')
        );
        assert_eq!(
            foldcolumn_sep_char(9, 1, &wp),
            crate::grid::schar_from_ascii(b'>')
        );
    }

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
