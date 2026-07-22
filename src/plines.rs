//! Translated from `src/nvim/plines.c` (tractable core only).
//!
//! `plines.c` (~1030 lines) calculates the vertical and horizontal
//! size of text as displayed in a window - screen-column/character-
//! width computation, a substantial subsystem of its own comparable in
//! scope to `mbyte.c` but for on-screen width rather than byte-level
//! decoding. Almost every real function needs `'linebreak'`/
//! `'breakindent'`/`'showbreak'`-aware wrapping arithmetic
//! (`charsize_regular`/`linesize_regular`), inline virtual text via
//! `marktree.c`'s filtered iteration (`init_charsize_arg`), or
//! `move.c`'s window-column-offset accounting (`win_col_off`/
//! `win_col_off2`). That last one needs `drawscreen.c`'s
//! `number_width` (which itself needs `marktree.c`'s `buf_meta_total`)
//! and `window.c`'s `win_fdccol_count` (which needs the fold
//! subsystem) - each its own real, not-yet-translated dependency chain.
//!
//! Translated: `win_chartabsize`, `charsize_nowrap` - the two
//! genuinely self-contained functions with no dependency on the wrap/
//! virtual-text/window-column-offset machinery above (needed
//! `indent.c`'s `tabstop_padding`, harvested alongside since it had no
//! other tractable caller yet either, and `charset.c`'s `ptr2cells`,
//! already existing).
//!
//! Deferred: everything else, including `charsize_fast_impl`/
//! `charsize_fast`/`linesize_fast` (need `in_win_border`, which needs
//! the `win_col_off` chain above), `charsize_regular`/
//! `linesize_regular` (need inline virtual text + `'linebreak'`/
//! `'breakindent'`/`'showbreak'` wrap arithmetic), `init_charsize_arg`
//! (needs `marktree.c`'s filtered iteration for virtual text),
//! `getvcol`/`getvvcol`/`linetabsize*` (need the above), and
//! everything past the file's own "horizontal size" section
//! (vertical size / fold-aware line-height calculations, needing
//! `fold.c`).

use crate::ascii_defs::TAB;
use crate::buffer_defs::WinT;
use crate::pos_defs::ColnrT;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

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
}
