//! Translated from `src/nvim/plines.c` (tractable core only).
//!
//! `plines.c` (~1030 lines) calculates the vertical and horizontal
//! size of text as displayed in a window - screen-column/character-
//! width computation, a substantial subsystem of its own comparable in
//! scope to `mbyte.c` but for on-screen width rather than byte-level
//! decoding. `charsize_regular`/`linesize_regular` additionally need
//! `'linebreak'`/`'breakindent'`/`'showbreak'`-aware wrapping
//! arithmetic on top of what `init_charsize_arg` itself now provides.
//!
//! Translated: `win_chartabsize`, `charsize_nowrap` (needed `indent.c`'s
//! `tabstop_padding` and `charset.c`'s `ptr2cells`); `in_win_border`
//! (needs `move.c`'s `win_col_off`/`win_col_off2`); `charsize_fast_impl`/
//! `charsize_fast`/`linesize_fast` (the "doesn't handle inline virtual
//! text/wrap-option arithmetic" fast path - needed `mbyte.c`'s
//! `StrCharInfo`/`utf_ptr2StrCharInfo`/`utfc_next`); `init_charsize_arg`
//! (decides which of the two modes applies for a given line, and -
//! when there's a preceding line - populates `CharsizeArg.iter`/
//! `virt_row` via `marktree.c`'s `marktree_itr_get_filter`, translated
//! alongside since this was its only real caller in this crate so
//! far). `CharsizeArg`/`CsType` are translated field-for-field/
//! variant-for-variant in full.
//!
//! Deferred: `charsize_regular`/`linesize_regular` - **re-investigated
//! precisely this session, now that `get_breakindent_win`/`ns_in_win`/
//! `marktree_itr_next_filter`/`mt_decor`/`mt_right`/`mt_invalid`/
//! `DecorVirtText` all exist**: `charsize_regular`'s OWN body still has
//! three distinct, non-trivial sub-algorithms beyond what those
//! prerequisites unlock, each deserving unhurried, dedicated attention
//! rather than being rushed alongside everything else already done
//! this session:
//! 1. Inline-virtual-text width accumulation (walks `csarg.iter` via
//!    `marktree_itr_current`/`marktree_itr_next_filter`, needs
//!    `mt_decor_virt`'s `DecorVirtText` linked list - tractable now,
//!    but not yet attempted).
//! 2. `'breakindent'`/`'showbreak'` wrap-position arithmetic - THREE
//!    separate rounding-arithmetic branches for where a wrapped
//!    screen line's head-indent applies (`max_head_vcol` positive/
//!    zero/negative), needing careful hand-tracing before trusting
//!    any test.
//! 3. `'linebreak'` word-wrap boundary detection (break at a blank
//!    before a non-blank, scanning back to the last non-blank-after-
//!    blank) - needs `charset.c`'s `vim_isbreak` (now translated) and
//!    `virt_text_cursor_off` (not yet checked).
//!
//! `getvcol`/`getvvcol`/`linetabsize*` need the above too - `getvcol`
//! itself already calls `init_charsize_arg` unconditionally, even on
//! the `kCharsizeFast` path, so it still needs `charsize_regular` to
//! exist before it can be translated even though the fast path alone
//! wouldn't otherwise need it. Everything past the file's own
//! "horizontal size" section (vertical size / fold-aware line-height
//! calculations, needing `fold.c`) remains deferred too.

use crate::ascii_defs::TAB;
use crate::buffer_defs::WinT;
use crate::pos_defs::{ColnrT, LinenrT, MAXCOL};

/// Which character-size computation mode applies to a given line
/// (`CSType`, a plain `bool` in the original - `kCharsizeRegular`/
/// `kCharsizeFast` - modeled here as a small enum for clarity, since
/// the original itself names the two states rather than treating the
/// value as an opaque boolean at call sites).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsType {
    Regular,
    Fast,
}

/// `inline_filter`: a [`crate::marktree_defs::MetaFilter`] selecting
/// only [`crate::marktree_defs::MetaIndex::Inline`] (inline virtual
/// text) - the only kind [`init_charsize_arg`] itself cares about.
const INLINE_FILTER: [u32; crate::marktree_defs::K_MT_META_COUNT] = {
    let mut filter = [0u32; crate::marktree_defs::K_MT_META_COUNT];
    filter[crate::marktree_defs::MetaIndex::Inline as usize] =
        crate::marktree_defs::MT_FILTER_SELECT;
    filter
};

/// Initialize a [`CharsizeArg`] for computing the display size of
/// `line` (line number `lnum` in window `wp`), and report which
/// computation mode applies (`init_charsize_arg`).
///
/// Unlike the original (which populates a caller-allocated
/// `CharsizeArg *csarg` out-parameter), this returns a freshly-built
/// `CharsizeArg` by value alongside the `CsType` - matching this
/// crate's established preference for return values over out-params
/// (e.g. `ml_get`/`ml_get_buf`) wherever the original's choice was
/// really just a C calling-convention detail, not meaningful state
/// shared across multiple call sites.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid.
#[must_use]
pub unsafe fn init_charsize_arg<'a>(
    wp: *mut WinT,
    lnum: LinenrT,
    line: &'a [u8],
) -> (CharsizeArg<'a>, CsType) {
    // SAFETY: forwarded from this function's own safety doc.
    let wpref = unsafe { &*wp };
    let mut csarg = CharsizeArg {
        win: wp,
        line,
        use_tabstop: wpref.w_onebuf_opt.wo_list == 0 || wpref.w_p_lcs_chars.tab1 != 0,
        indent_width: i32::MIN,
        virt_row: -1,
        cur_text_width_left: 0,
        cur_text_width_right: 0,
        max_head_vcol: 0,
        iter: crate::marktree_defs::MarkTreeIter::default(),
    };

    if lnum > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let buf = unsafe { &*wpref.w_buffer };
        if crate::marktree::marktree_itr_get_filter(
            &buf.b_marktree,
            lnum - 1,
            0,
            lnum,
            0,
            &INLINE_FILTER,
            &mut csarg.iter,
        ) {
            csarg.virt_row = lnum - 1;
        }
    }

    let cstype = if csarg.virt_row >= 0
        || (wpref.w_onebuf_opt.wo_wrap != 0
            && (wpref.w_onebuf_opt.wo_lbr != 0
                || wpref.w_onebuf_opt.wo_bri != 0
                || !crate::option::get_showbreak_value(wpref).is_empty()))
    {
        CsType::Regular
    } else {
        CsType::Fast
    };

    (csarg, cstype)
}

/// Return the number of cells the first char in `p` will take on the
/// screen, taking into account the size of a tab. Also see
/// [`crate::cursor`]'s cursor-position functions (`win_chartabsize`).
///
/// # Safety
/// `wp.w_buffer` must be a valid, non-null pointer to a live `BufT`.
/// Also touches `crate::option_vars::OPTION_VARS` (via
/// `crate::charset::ptr2cells`).
#[must_use]
pub unsafe fn win_chartabsize(wp: &WinT, p: &[u8], col: ColnrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*wp.w_buffer };
    if p[0] == TAB && (wp.w_onebuf_opt.wo_list == 0 || wp.w_p_lcs_chars.tab1 != 0) {
        return crate::indent::tabstop_padding(col, buf.b_p_ts, buf.b_p_vts_array.as_deref());
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::charset::ptr2cells(p) }
}

/// Get the number of cells taken up on the screen at the given virtual
/// column. Takes an already-decoded `cur_char` rather than decoding
/// `cur` itself (`charsize_nowrap`).
///
/// @see [`win_chartabsize`]
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// `crate::charset::ptr2cells`).
#[must_use]
pub unsafe fn charsize_nowrap(
    b_p_ts: crate::types_defs::OptInt,
    b_p_vts_array: Option<&[ColnrT]>,
    cur: &[u8],
    use_tabstop: bool,
    vcol: ColnrT,
    cur_char: i32,
) -> i32 {
    if cur_char == i32::from(TAB) && use_tabstop {
        crate::indent::tabstop_padding(vcol, b_p_ts, b_p_vts_array)
    } else if cur_char < 0 {
        crate::mbyte_defs::K_INVALID_BYTE_CELLS
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::charset::ptr2cells(cur) }
    }
}

/// The result of a character-size computation: total display width,
/// plus how much of that width is attributable to a `'breakindent'`/
/// `'showbreak'` head or a `'linebreak'` tail (`CharSize`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharSize {
    pub width: i32,
    pub head: i32,
    pub tail: i32,
}

/// Argument bag for the character-size functions (`CharsizeArg`).
///
/// `win` mirrors the original's raw `win_T *win` (this crate's usual
/// convention for a live, aliasable window pointer); `line` is a
/// borrowed slice rather than a raw `char *`, matching this crate's
/// preference for safe borrows over pointers for read-only line
/// content elsewhere (e.g. `win_chartabsize`'s own `p: &[u8]`
/// parameter).
///
/// `iter` (the marktree filtered-iteration state used by
/// [`init_charsize_arg`] for inline virtual text) is now a real,
/// populated field - `init_charsize_arg` itself is translated (needed
/// `marktree.c`'s `marktree_itr_get_filter`, translated alongside
/// since this was its only real caller). `charsize_regular`/
/// `linesize_regular` (which would actually READ `iter`'s virtual-text
/// state) remain deferred - see this module's own doc comment.
#[derive(Debug, Default)]
pub struct CharsizeArg<'a> {
    pub win: *mut WinT,
    pub line: &'a [u8],
    pub use_tabstop: bool,
    pub indent_width: i32,
    pub virt_row: i32,
    pub cur_text_width_left: i32,
    pub cur_text_width_right: i32,
    pub max_head_vcol: i32,
    pub iter: crate::marktree_defs::MarkTreeIter,
}

/// Check that virtual column `vcol` is in the rightmost column of
/// window `wp` (`in_win_border`).
///
/// # Safety
/// Same as [`crate::r#move::win_col_off`]/[`crate::r#move::win_col_off2`].
unsafe fn in_win_border(wp: &mut WinT, vcol: ColnrT) -> bool {
    if wp.w_view_width == 0 {
        // there is no border
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let width1 = wp.w_view_width - unsafe { crate::r#move::win_col_off(wp) };

    if vcol < width1 - 1 {
        return false;
    }
    if vcol == width1 - 1 {
        return true;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let width2 = width1 + unsafe { crate::r#move::win_col_off2(wp) };
    if width2 <= 0 {
        return false;
    }
    (vcol - width1) % width2 == width2 - 1
}

/// Like `charsize_regular` (not yet translated), except it doesn't
/// handle inline virtual text, `'linebreak'`, `'breakindent'` or
/// `'showbreak'`. Handles normal characters, tabs and wrapping. Always
/// inlined in the original; the always-inlined core of
/// [`charsize_fast`] here too (`charsize_fast_impl`).
///
/// # Safety
/// `wp.w_buffer` must be a valid, non-null pointer to a live `BufT`.
/// Also touches `crate::option_vars::OPTION_VARS` (via `ptr2cells`,
/// transitively through `in_win_border`'s `win_col_off2`).
#[must_use]
pub unsafe fn charsize_fast_impl(
    wp: &mut WinT,
    cur: &[u8],
    use_tabstop: bool,
    vcol: ColnrT,
    cur_char: i32,
) -> CharSize {
    // A tab gets expanded, depending on the current column.
    if cur_char == i32::from(TAB) && use_tabstop {
        // SAFETY: forwarded from this function's own safety doc.
        let buf = unsafe { &*wp.w_buffer };
        return CharSize {
            width: crate::indent::tabstop_padding(vcol, buf.b_p_ts, buf.b_p_vts_array.as_deref()),
            ..CharSize::default()
        };
    }

    let width = if cur_char < 0 {
        crate::mbyte_defs::K_INVALID_BYTE_CELLS
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::charset::ptr2cells(cur) }
    };

    // If a double-width char doesn't fit at the end of a line, it
    // wraps to the next line, and the last column displays a '>'.
    if width == 2
        && cur_char >= 0x80
        && wp.w_onebuf_opt.wo_wrap != 0
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { in_win_border(wp, vcol) }
    {
        CharSize { width: 3, head: 1, tail: 0 }
    } else {
        CharSize { width, ..CharSize::default() }
    }
}

/// Like `charsize_regular` (not yet translated), except it doesn't
/// handle inline virtual text, `'linebreak'`, `'breakindent'` or
/// `'showbreak'` (`charsize_fast`).
///
/// # Safety
/// `csarg.win` must be a valid, non-null pointer to a live `WinT`
/// whose own `w_buffer` is also valid. Also touches
/// `crate::option_vars::OPTION_VARS` (via `ptr2cells`).
#[must_use]
pub unsafe fn charsize_fast(
    csarg: &CharsizeArg,
    cur: &[u8],
    vcol: ColnrT,
    cur_char: i32,
) -> CharSize {
    // SAFETY: forwarded from this function's own safety doc.
    let wp = unsafe { &mut *csarg.win };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { charsize_fast_impl(wp, cur, csarg.use_tabstop, vcol, cur_char) }
}

/// Like `linesize_regular` (not yet translated), but can be used when
/// the fast path applies (`linesize_fast`).
///
/// # Safety
/// Same as [`charsize_fast`].
#[must_use]
pub unsafe fn linesize_fast(csarg: &CharsizeArg, vcol_arg: i32, len: ColnrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let wp = unsafe { &mut *csarg.win };
    let use_tabstop = csarg.use_tabstop;
    let line = csarg.line;
    let mut vcol: i64 = i64::from(vcol_arg);
    let mut vcol_arg = vcol_arg;

    let mut ci = crate::mbyte::utf_ptr2str_char_info(line);
    while (ci.pos as i32) < len && line.get(ci.pos).copied().unwrap_or(0) != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let width = unsafe {
            charsize_fast_impl(wp, &line[ci.pos..], use_tabstop, vcol_arg, ci.chr.value)
        }
        .width;
        vcol += i64::from(width);
        // SAFETY: forwarded from this function's own safety doc.
        ci = unsafe { crate::mbyte::utfc_next(line, ci) };
        if vcol > i64::from(MAXCOL) {
            vcol_arg = MAXCOL;
            break;
        }
        vcol_arg = vcol as i32;
    }

    vcol_arg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    #[test]
    fn init_charsize_arg_plain_line_zero_is_fast_with_no_virtual_text_check() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let (csarg, cstype) = unsafe { init_charsize_arg(&mut win as *mut WinT, 0, b"abc\0") };
        assert_eq!(cstype, CsType::Fast);
        assert_eq!(csarg.virt_row, -1);
        assert_eq!(csarg.indent_width, i32::MIN);
    }

    #[test]
    fn init_charsize_arg_finds_inline_virtual_text_on_the_preceding_line() {
        let mut buf = BufT::default();
        // Mark the line ABOVE lnum=5 (i.e. row 4, 0-indexed) as having
        // inline virtual text, via the public marktree_put API.
        let key = crate::marktree_defs::MtKey {
            pos: crate::marktree_defs::MtPos::new(4, 0),
            ns: 0,
            id: 1,
            flags: crate::marktree::mt_flags(false, false, false, false)
                | crate::marktree::MT_FLAG_DECOR_VIRT_TEXT_INLINE,
            decor_data: crate::decoration_defs::DecorInlineData {
                hl: crate::decoration_defs::DecorHighlightInline::default(),
            },
        };
        crate::marktree::marktree_put(&mut buf.b_marktree, key, -1, -1, false);
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };

        let (csarg, cstype) = unsafe { init_charsize_arg(&mut win as *mut WinT, 5, b"abc\0") };
        assert_eq!(cstype, CsType::Regular);
        assert_eq!(csarg.virt_row, 4);
    }

    #[test]
    fn init_charsize_arg_wrap_with_linebreak_is_regular() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_wrap = 1;
        win.w_onebuf_opt.wo_lbr = 1;

        let (csarg, cstype) = unsafe { init_charsize_arg(&mut win as *mut WinT, 0, b"abc\0") };
        assert_eq!(cstype, CsType::Regular);
        assert_eq!(csarg.virt_row, -1); // regular via 'linebreak', not virtual text
    }

    #[test]
    fn init_charsize_arg_wrap_with_breakindent_is_regular() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_wrap = 1;
        win.w_onebuf_opt.wo_bri = 1;

        let (_csarg, cstype) = unsafe { init_charsize_arg(&mut win as *mut WinT, 0, b"abc\0") };
        assert_eq!(cstype, CsType::Regular);
    }

    #[test]
    fn init_charsize_arg_wrap_with_showbreak_is_regular() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_wrap = 1;
        win.w_onebuf_opt.wo_sbr = Some(b">>".to_vec());

        let (_csarg, cstype) = unsafe { init_charsize_arg(&mut win as *mut WinT, 0, b"abc\0") };
        assert_eq!(cstype, CsType::Regular);
    }

    #[test]
    fn init_charsize_arg_linebreak_without_wrap_stays_fast() {
        // 'linebreak'/'breakindent'/'showbreak' only matter when 'wrap'
        // is also on (matching the original's `wp->w_p_wrap && (...)`).
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_wrap = 0;
        win.w_onebuf_opt.wo_lbr = 1;
        win.w_onebuf_opt.wo_bri = 1;
        win.w_onebuf_opt.wo_sbr = Some(b">>".to_vec());

        let (_csarg, cstype) = unsafe { init_charsize_arg(&mut win as *mut WinT, 0, b"abc\0") };
        assert_eq!(cstype, CsType::Fast);
    }

    #[test]
    fn init_charsize_arg_use_tabstop_depends_on_list_and_tab1_glyph() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };

        // 'list' off: always use tabstop-based padding.
        let (csarg, _) = unsafe { init_charsize_arg(&mut win as *mut WinT, 0, b"a\0") };
        assert!(csarg.use_tabstop);

        // 'list' on, no tab1 glyph configured: falls back to ptr2cells.
        win.w_onebuf_opt.wo_list = 1;
        let (csarg, _) = unsafe { init_charsize_arg(&mut win as *mut WinT, 0, b"a\0") };
        assert!(!csarg.use_tabstop);

        // 'list' on, WITH a tab1 glyph configured: back to tabstop padding.
        win.w_p_lcs_chars.tab1 = 1;
        let (csarg, _) = unsafe { init_charsize_arg(&mut win as *mut WinT, 0, b"a\0") };
        assert!(csarg.use_tabstop);
    }

    #[test]
    fn win_chartabsize_plain_ascii_is_one_cell() {
        let mut buf = BufT::default();
        let win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        assert_eq!(unsafe { win_chartabsize(&win, b"x", 0) }, 1);
    }

    #[test]
    fn win_chartabsize_tab_uses_tabstop_padding() {
        let mut buf = BufT { b_p_ts: 8, ..Default::default() };
        let win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        // col=2, ts=8 (no vts): padding = 8 - (2%8) = 6.
        assert_eq!(unsafe { win_chartabsize(&win, &[TAB], 2) }, 6);
    }

    #[test]
    fn win_chartabsize_tab_with_list_and_no_tab1_glyph_falls_through_to_ptr2cells() {
        // 'list' is on but no tab1 listchars glyph is set: falls
        // through to ptr2cells (matches the original's own
        // `!wp->w_p_list || wp->w_p_lcs_chars.tab1` condition - both
        // being false/zero here disables the tabstop_padding path).
        // ptr2cells treats a raw TAB byte as a control char ("^I"),
        // 2 cells - matches charset.rs's own char2cells precedent.
        let mut buf = BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_list = 1;
        assert_eq!(unsafe { win_chartabsize(&win, &[TAB], 2) }, 2);
    }

    #[test]
    fn charsize_nowrap_tab_uses_tabstop_padding() {
        assert_eq!(
            unsafe { charsize_nowrap(8, None, &[TAB], true, 2, i32::from(TAB)) },
            6
        );
    }

    #[test]
    fn charsize_nowrap_negative_char_is_invalid_byte_cells() {
        assert_eq!(unsafe { charsize_nowrap(8, None, b"\xff", false, 0, -1) }, 4);
    }

    #[test]
    fn charsize_nowrap_plain_ascii_is_one_cell() {
        assert_eq!(unsafe { charsize_nowrap(8, None, b"x", false, 0, i32::from(b'x')) }, 1);
    }

    /// Shared setup for the `in_win_border`/`charsize_fast_impl` tests
    /// below: `w_view_width=10`, `'number'` on with a 1-digit line
    /// count (`number_width==1`), no foldcolumn/signcolumn/cpo 'n'
    /// flag. Hand-traced: `win_col_off == 2` (number_width(1) +
    /// stc_empty(1)), so `width1 == 10 - 2 == 8`; `win_col_off2 == 0`
    /// (no cpo 'n'), so `width2 == 8 + 0 == 8`.
    fn border_test_win(buf: *mut BufT) -> WinT {
        let mut win = WinT { w_buffer: buf, ..Default::default() };
        win.w_view_width = 10;
        win.w_onebuf_opt.wo_nu = 1;
        win
    }

    #[test]
    fn in_win_border_zero_view_width_is_always_false() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_view_width = 0;
        assert!(!unsafe { in_win_border(&mut win, 100) });
    }

    #[test]
    fn in_win_border_true_at_first_and_second_wrap_boundary() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5; // number_width == 1
        let mut win = border_test_win(&mut buf as *mut BufT);

        assert!(!unsafe { in_win_border(&mut win, 6) }); // 6 < width1-1(7)
        assert!(unsafe { in_win_border(&mut win, 7) }); // == width1-1
        assert!(!unsafe { in_win_border(&mut win, 8) }); // (8-8)%8=0 != width2-1(7)
        assert!(unsafe { in_win_border(&mut win, 15) }); // (15-8)%8=7 == width2-1(7)
    }

    #[test]
    fn charsize_fast_impl_tab_uses_tabstop_padding() {
        let mut buf = BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let cs = unsafe { charsize_fast_impl(&mut win, &[TAB], true, 2, i32::from(TAB)) };
        assert_eq!(cs, CharSize { width: 6, head: 0, tail: 0 });
    }

    #[test]
    fn charsize_fast_impl_negative_char_is_invalid_byte_cells() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let cs = unsafe { charsize_fast_impl(&mut win, b"\xff", false, 0, -1) };
        assert_eq!(cs, CharSize { width: 4, head: 0, tail: 0 });
    }

    #[test]
    fn charsize_fast_impl_doublewidth_char_not_at_border_is_plain_width_two() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5; // number_width == 1
        let mut win = border_test_win(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_wrap = 1;

        // vcol=6 is not at the border (see in_win_border trace above).
        let cjk = "一".as_bytes(); // U+4E00, East Asian Wide: 2 cells
        let cs = unsafe { charsize_fast_impl(&mut win, cjk, false, 6, 0x4E00) };
        assert_eq!(cs, CharSize { width: 2, head: 0, tail: 0 });
    }

    #[test]
    fn charsize_fast_impl_doublewidth_char_at_border_with_wrap_gets_the_overflow_marker() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5; // number_width == 1
        let mut win = border_test_win(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_wrap = 1;

        // vcol=7 IS at the border (see in_win_border trace above).
        let cjk = "一".as_bytes();
        let cs = unsafe { charsize_fast_impl(&mut win, cjk, false, 7, 0x4E00) };
        assert_eq!(cs, CharSize { width: 3, head: 1, tail: 0 });
    }

    #[test]
    fn charsize_fast_impl_doublewidth_char_at_border_without_wrap_stays_plain() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5;
        let mut win = border_test_win(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_wrap = 0; // 'wrap' off: no overflow marker regardless of border

        let cjk = "一".as_bytes();
        let cs = unsafe { charsize_fast_impl(&mut win, cjk, false, 7, 0x4E00) };
        assert_eq!(cs, CharSize { width: 2, head: 0, tail: 0 });
    }

    #[test]
    fn charsize_fast_forwards_to_charsize_fast_impl_via_csarg() {
        let mut buf = BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let csarg =
            CharsizeArg { win: &mut win as *mut WinT, use_tabstop: true, ..Default::default() };
        let cs = unsafe { charsize_fast(&csarg, &[TAB], 2, i32::from(TAB)) };
        assert_eq!(cs, CharSize { width: 6, head: 0, tail: 0 });
    }

    #[test]
    fn linesize_fast_sums_plain_ascii_widths() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let line = b"abc\0"; // includes trailing NUL per this crate's line convention
        let csarg = CharsizeArg { win: &mut win as *mut WinT, line, ..Default::default() };
        assert_eq!(unsafe { linesize_fast(&csarg, 0, MAXCOL) }, 3);
    }

    #[test]
    fn linesize_fast_respects_the_len_limit() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let line = b"abcde\0";
        let csarg = CharsizeArg { win: &mut win as *mut WinT, line, ..Default::default() };
        // len=2 stops after 'a','b' only.
        assert_eq!(unsafe { linesize_fast(&csarg, 0, 2) }, 2);
    }

    #[test]
    fn linesize_fast_counts_a_tab_with_tabstop_padding() {
        let mut buf = BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let line: &[u8] = &[TAB, b'x', 0];
        let csarg = CharsizeArg {
            win: &mut win as *mut WinT,
            line,
            use_tabstop: true,
            ..Default::default()
        };
        // TAB at vcol 0, ts=8: padding = 8 - (0%8) = 8. Then 'x' at
        // vcol=8: plain ascii width 1. Total 9.
        assert_eq!(unsafe { linesize_fast(&csarg, 0, MAXCOL) }, 9);
    }

    #[test]
    fn linesize_fast_clamps_to_maxcol_on_overflow() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let line = b"ab\0";
        let csarg = CharsizeArg { win: &mut win as *mut WinT, line, ..Default::default() };
        // Starting one below MAXCOL: 'a' pushes vcol to exactly MAXCOL
        // (not yet over - `vcol > MAXCOL` is false when equal), 'b'
        // pushes it to MAXCOL+1 (over) - clamped back to MAXCOL.
        assert_eq!(unsafe { linesize_fast(&csarg, MAXCOL - 1, MAXCOL) }, MAXCOL);
    }
}
