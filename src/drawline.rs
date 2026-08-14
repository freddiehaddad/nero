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

/// Reusable line-rendering scratch buffer (`extra_buf`).
static EXTRA_BUF: crate::globals::GlobalCell<Vec<u8>> =
    crate::globals::GlobalCell::new(Vec::new());

/// Variables shared by `win_line()` and its helpers (`winlinevars_T`).
#[allow(dead_code)]
#[derive(Debug)]
struct WinlinevarsT {
    lnum: crate::pos_defs::LinenrT,
    foldinfo: crate::fold_defs::FoldinfoT,
    startrow: i32,
    row: i32,
    vcol: crate::pos_defs::ColnrT,
    col: i32,
    boguscols: i32,
    old_boguscols: i32,
    vcol_off_co: i32,
    off: i32,
    cul_attr: i32,
    line_attr: i32,
    line_attr_lowprio: i32,
    sign_num_attr: i32,
    prev_num_attr: i32,
    sign_cul_attr: i32,
    fromcol: i32,
    tocol: i32,
    vcol_sbr: crate::pos_defs::ColnrT,
    need_showbreak: bool,
    char_attr: i32,
    n_extra: i32,
    n_attr: i32,
    p_extra: *mut u8,
    extra_attr: i32,
    sc_extra: crate::types_defs::ScharT,
    sc_final: crate::types_defs::ScharT,
    extra_for_extmark: bool,
    extra: [u8; 11],
    diff_hlf: crate::highlight_defs::HlfT,
    n_virt_lines: i32,
    n_virt_below: i32,
    filler_lines: i32,
    filler_todo: i32,
    virt_below_skip: i32,
    filler_lines_skip: i32,
    sattrs: [crate::sign_defs::SignTextAttrs;
        crate::sign_defs::SIGN_SHOW_MAX as usize],
    need_lbr: bool,
    virt_inline: crate::decoration_defs::VirtText,
    virt_inline_i: usize,
    virt_inline_hl_mode: crate::decoration_defs::HlMode,
    reset_extra_attr: bool,
    skip_cells: i32,
    skipped_cells: i32,
    color_cols: *const i32,
}

impl Default for WinlinevarsT {
    fn default() -> Self {
        Self {
            lnum: 0,
            foldinfo: crate::fold_defs::FoldinfoT::default(),
            startrow: 0,
            row: 0,
            vcol: 0,
            col: 0,
            boguscols: 0,
            old_boguscols: 0,
            vcol_off_co: 0,
            off: 0,
            cul_attr: 0,
            line_attr: 0,
            line_attr_lowprio: 0,
            sign_num_attr: 0,
            prev_num_attr: 0,
            sign_cul_attr: 0,
            fromcol: 0,
            tocol: 0,
            vcol_sbr: 0,
            need_showbreak: false,
            char_attr: 0,
            n_extra: 0,
            n_attr: 0,
            p_extra: std::ptr::null_mut(),
            extra_attr: 0,
            sc_extra: 0,
            sc_final: 0,
            extra_for_extmark: false,
            extra: [0; 11],
            diff_hlf: crate::highlight_defs::HlfT::default(),
            n_virt_lines: 0,
            n_virt_below: 0,
            filler_lines: 0,
            filler_todo: 0,
            virt_below_skip: 0,
            filler_lines_skip: 0,
            sattrs: [crate::sign_defs::SignTextAttrs {
                text: [0; crate::types_defs::SIGN_WIDTH as usize],
                hl_id: 0,
            }; crate::sign_defs::SIGN_SHOW_MAX as usize],
            need_lbr: false,
            virt_inline: Vec::new(),
            virt_inline_i: 0,
            virt_inline_hl_mode: crate::decoration_defs::HlMode::Unknown,
            reset_extra_attr: false,
            skip_cells: 0,
            skipped_cells: 0,
            color_cols: std::ptr::null(),
        }
    }
}

/// Advance the current `'colorcolumn'` pointer past `vcol`
/// (`advance_color_col`).
///
/// # Safety
/// `color_cols`, when non-null, must point into a live `-1`-terminated
/// integer array.
#[allow(dead_code)]
unsafe fn advance_color_col(wlv: &mut WinlinevarsT, vcol: i32) {
    if wlv.color_cols.is_null() {
        return;
    }
    while unsafe { *wlv.color_cols } >= 0
        && vcol > unsafe { *wlv.color_cols }
    {
        wlv.color_cols = unsafe { wlv.color_cols.add(1) };
    }
    if unsafe { *wlv.color_cols } < 0 {
        wlv.color_cols = std::ptr::null();
    }
}

/// Reconcile temporary columns inserted to force wrapping
/// (`fix_for_boguscols`).
#[allow(dead_code)]
fn fix_for_boguscols(wlv: &mut WinlinevarsT) {
    wlv.n_extra += wlv.vcol_off_co;
    wlv.vcol -= wlv.vcol_off_co;
    wlv.vcol_off_co = 0;
    wlv.col -= wlv.boguscols;
    wlv.old_boguscols = wlv.boguscols;
    wlv.boguscols = 0;
}

/// Whether `CursorLineNr` applies to this screen row
/// (`use_cursor_line_nr`).
#[allow(dead_code)]
fn use_cursor_line_nr(wp: &WinT, wlv: &WinlinevarsT) -> bool {
    let flags = u32::from(wp.w_p_culopt_flags);
    wp.w_onebuf_opt.wo_cul != 0
        && wlv.lnum == wp.w_cursorline
        && flags & crate::option_vars::opt_culopt_flag::NUMBER != 0
        && (wlv.row == wlv.startrow + wlv.filler_lines
            || (wlv.row > wlv.startrow + wlv.filler_lines
                && flags & crate::option_vars::opt_culopt_flag::LINE != 0))
}

/// Fill a run of cells in the current drawline buffers (`draw_col_fill`).
///
/// # Safety
/// `wlv.off..wlv.off + width` must be valid in both global line
/// buffers.
#[allow(dead_code)]
unsafe fn draw_col_fill(
    wlv: &mut WinlinevarsT,
    fillchar: crate::types_defs::ScharT,
    width: i32,
    attr: i32,
) {
    for _ in 0..width {
        let offset = wlv.off as usize;
        (unsafe { crate::grid::LINEBUF_CHAR.get_mut() })[offset] = fillchar;
        (unsafe { crate::grid::LINEBUF_ATTR.get_mut() })[offset] = attr;
        wlv.off += 1;
    }
}

/// Initialize one screen line at column zero (`win_line_start`).
///
/// # Safety
/// Global line buffers must contain at least `wp.w_view_width` cells.
#[allow(dead_code)]
unsafe fn win_line_start(wp: &WinT, wlv: &mut WinlinevarsT) {
    wlv.col = 0;
    wlv.off = 0;
    wlv.need_lbr = false;
    let space = crate::grid::schar_from_ascii(b' ');
    for index in 0..wp.w_view_width.max(0) as usize {
        (unsafe { crate::grid::LINEBUF_CHAR.get_mut() })[index] = space;
        (unsafe { crate::grid::LINEBUF_ATTR.get_mut() })[index] = 0;
        (unsafe { crate::grid::LINEBUF_VCOL.get_mut() })[index] = -1;
    }
}

/// Ensure the reusable scratch buffer has at least `size` bytes and
/// return its data pointer (`get_extra_buf`).
///
/// # Safety
/// The returned pointer is invalidated by a later call that grows or
/// frees the buffer; no concurrent access is allowed.
#[allow(dead_code)]
unsafe fn get_extra_buf(size: usize) -> *mut u8 {
    let size = size.max(64);
    // SAFETY: forwarded from this function's own safety doc.
    let buffer = unsafe { EXTRA_BUF.get_mut() };
    if buffer.len() < size {
        *buffer = vec![0; size];
    }
    buffer.as_mut_ptr()
}

/// Release drawline scratch storage (`drawline_free_all_mem`).
///
/// # Safety
/// No pointer returned by [`get_extra_buf`] may be used afterward.
pub unsafe fn drawline_free_all_mem() {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { EXTRA_BUF.get_mut() } = Vec::new();
}

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

    struct ExtraBufGuard(Vec<u8>);

    struct DrawLinebufGuard {
        chars: Vec<crate::types_defs::ScharT>,
        attrs: Vec<crate::types_defs::SattrT>,
        vcols: Vec<crate::pos_defs::ColnrT>,
    }

    impl DrawLinebufGuard {
        fn install(size: usize) -> Self {
            Self {
                chars: std::mem::replace(
                    unsafe { crate::grid::LINEBUF_CHAR.get_mut() },
                    vec![0; size],
                ),
                attrs: std::mem::replace(
                    unsafe { crate::grid::LINEBUF_ATTR.get_mut() },
                    vec![0; size],
                ),
                vcols: std::mem::replace(
                    unsafe { crate::grid::LINEBUF_VCOL.get_mut() },
                    vec![0; size],
                ),
            }
        }
    }

    impl Drop for DrawLinebufGuard {
        fn drop(&mut self) {
            *unsafe { crate::grid::LINEBUF_CHAR.get_mut() } =
                std::mem::take(&mut self.chars);
            *unsafe { crate::grid::LINEBUF_ATTR.get_mut() } =
                std::mem::take(&mut self.attrs);
            *unsafe { crate::grid::LINEBUF_VCOL.get_mut() } =
                std::mem::take(&mut self.vcols);
        }
    }

    impl ExtraBufGuard {
        fn empty() -> Self {
            Self(std::mem::take(unsafe { EXTRA_BUF.get_mut() }))
        }
    }

    impl Drop for ExtraBufGuard {
        fn drop(&mut self) {
            *unsafe { EXTRA_BUF.get_mut() } = std::mem::take(&mut self.0);
        }
    }

    #[test]
    fn get_extra_buf_allocates_at_least_64_and_reuses_large_enough_storage() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ExtraBufGuard::empty();
        let first = unsafe { get_extra_buf(3) };
        unsafe { *first = 0x5a };
        assert_eq!(unsafe { EXTRA_BUF.get_mut() }.len(), 64);

        let second = unsafe { get_extra_buf(32) };
        assert_eq!(second, first);
        assert_eq!(unsafe { *second }, 0x5a);

        let _ = unsafe { get_extra_buf(100) };
        assert_eq!(unsafe { EXTRA_BUF.get_mut() }.len(), 100);
    }

    #[test]
    fn drawline_free_all_mem_releases_the_scratch_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ExtraBufGuard::empty();
        let _ = unsafe { get_extra_buf(128) };
        assert_eq!(unsafe { EXTRA_BUF.get_mut() }.len(), 128);
        unsafe { drawline_free_all_mem() };
        assert!(unsafe { EXTRA_BUF.get_mut() }.is_empty());
    }

    #[test]
    fn winlinevars_default_matches_zero_initialized_draw_state() {
        let state = WinlinevarsT::default();
        assert_eq!(state.lnum, 0);
        assert_eq!(state.foldinfo, crate::fold_defs::FoldinfoT::default());
        assert_eq!(state.off, 0);
        assert_eq!(state.extra.len(), 11);
        assert_eq!(
            state.sattrs.len(),
            crate::sign_defs::SIGN_SHOW_MAX as usize
        );
        assert!(state.p_extra.is_null());
        assert!(state.color_cols.is_null());
        assert!(state.virt_inline.is_empty());
        assert_eq!(
            state.virt_inline_hl_mode,
            crate::decoration_defs::HlMode::Unknown
        );
    }

    #[test]
    fn advance_color_col_skips_past_columns_and_clears_at_sentinel() {
        let columns = [2, 5, 9, -1];
        let mut state = WinlinevarsT {
            color_cols: columns.as_ptr(),
            ..Default::default()
        };

        unsafe { advance_color_col(&mut state, 5) };
        assert_eq!(unsafe { *state.color_cols }, 5);
        unsafe { advance_color_col(&mut state, 8) };
        assert_eq!(unsafe { *state.color_cols }, 9);
        unsafe { advance_color_col(&mut state, 10) };
        assert!(state.color_cols.is_null());

        unsafe { advance_color_col(&mut state, 20) };
        assert!(state.color_cols.is_null());
    }

    #[test]
    fn fix_for_boguscols_restores_real_columns_and_tracks_old_amount() {
        let mut state = WinlinevarsT {
            n_extra: 4,
            vcol: 20,
            vcol_off_co: 3,
            col: 12,
            boguscols: 5,
            old_boguscols: 1,
            ..Default::default()
        };

        fix_for_boguscols(&mut state);

        assert_eq!(state.n_extra, 7);
        assert_eq!(state.vcol, 17);
        assert_eq!(state.vcol_off_co, 0);
        assert_eq!(state.col, 7);
        assert_eq!(state.old_boguscols, 5);
        assert_eq!(state.boguscols, 0);
    }

    #[test]
    fn use_cursor_line_nr_handles_first_and_wrapped_screen_rows() {
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_cul = 1;
        win.w_cursorline = 12;
        win.w_p_culopt_flags =
            crate::option_vars::opt_culopt_flag::NUMBER as u8;
        let mut state = WinlinevarsT {
            lnum: 12,
            startrow: 3,
            filler_lines: 2,
            row: 5,
            ..Default::default()
        };
        assert!(use_cursor_line_nr(&win, &state));

        state.row = 6;
        assert!(!use_cursor_line_nr(&win, &state));
        win.w_p_culopt_flags |= crate::option_vars::opt_culopt_flag::LINE as u8;
        assert!(use_cursor_line_nr(&win, &state));

        state.lnum = 11;
        assert!(!use_cursor_line_nr(&win, &state));
    }

    #[test]
    fn draw_col_fill_writes_cells_and_advances_the_line_offset() {
        let _lock = crate::globals::global_state_test_lock();
        let _linebuf = DrawLinebufGuard::install(6);
        let mut state = WinlinevarsT {
            off: 2,
            ..Default::default()
        };
        let fill = crate::grid::schar_from_ascii(b'-');

        unsafe { draw_col_fill(&mut state, fill, 3, 17) };

        assert_eq!(state.off, 5);
        assert_eq!(
            &unsafe { crate::grid::LINEBUF_CHAR.get_mut() }[2..5],
            &[fill; 3]
        );
        assert_eq!(
            &unsafe { crate::grid::LINEBUF_ATTR.get_mut() }[2..5],
            &[17; 3]
        );
    }

    #[test]
    fn win_line_start_resets_offsets_and_clears_the_visible_width() {
        let _lock = crate::globals::global_state_test_lock();
        let _linebuf = DrawLinebufGuard::install(6);
        unsafe {
            crate::grid::LINEBUF_CHAR.get_mut().fill(99);
            crate::grid::LINEBUF_ATTR.get_mut().fill(7);
            crate::grid::LINEBUF_VCOL.get_mut().fill(8);
        }
        let win = WinT {
            w_view_width: 4,
            ..Default::default()
        };
        let mut state = WinlinevarsT {
            col: 9,
            off: 3,
            need_lbr: true,
            ..Default::default()
        };

        unsafe { win_line_start(&win, &mut state) };

        assert_eq!((state.col, state.off, state.need_lbr), (0, 0, false));
        let space = crate::grid::schar_from_ascii(b' ');
        assert_eq!(
            &unsafe { crate::grid::LINEBUF_CHAR.get_mut() }[..4],
            &[space; 4]
        );
        assert_eq!(
            &unsafe { crate::grid::LINEBUF_ATTR.get_mut() }[..4],
            &[0; 4]
        );
        assert_eq!(
            &unsafe { crate::grid::LINEBUF_VCOL.get_mut() }[..4],
            &[-1; 4]
        );
        assert_eq!(unsafe { crate::grid::LINEBUF_CHAR.get_mut() }[4], 99);
    }

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
