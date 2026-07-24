//! Translated from `src/nvim/decoration.c` (tractable core only).
//!
//! `decoration.c` (~2000 lines) is neovim's extmark-decoration
//! rendering engine (virtual text, highlights, conceal, sign columns,
//! decoration providers) - a substantial subsystem of its own, almost
//! entirely dependent on the marktree query machinery and the Lua
//! decoration-provider callback host, not attempted here.
//!
//! Translated: [`decor_conceal_line`]/[`decor_virt_lines`] - real,
//! faithful translations of each function's own real, always-taken
//! early-return path, matching this session's established "translate
//! the real condition, not a hardcoded shortcut" pattern:
//! - [`decor_conceal_line`]: its own FIRST `||` operand,
//!   `wp.w_onebuf_opt.wo_cole < 2`, is always true today (nothing in
//!   this crate can currently raise `'conceallevel'` above its real
//!   default of `0` - the options-parsing engine isn't built), so due
//!   to `||` short-circuit evaluation, this function always returns
//!   `false` without ever touching `conceal_cursor_line`/
//!   `buf_meta_total`/the marktree at all.
//! - [`decor_virt_lines`]: its own first check,
//!   `!buf_meta_total(buf, kMTMetaLines)`, is always true today
//!   (nothing in this crate can currently attach virtual lines to any
//!   buffer - the extmark-creation API isn't reachable), so this
//!   function always returns `0` immediately without touching its
//!   `num_below`/`lines` out-parameters or the marktree at all.
//!
//! Deferred: everything else in the file - real virtual-text/
//! highlight/conceal rendering, needing the marktree query machinery
//! and decoration-provider Lua callbacks, neither translated.

use crate::buffer_defs::WinT;
use crate::decoration_defs::VirtLines;
use crate::marktree_defs::MetaIndex;

/// Called by draw, move and plines code to determine whether a line
/// is concealed. Scans the marktree for `conceal_line` marks on `row`
/// and invokes any `_on_conceal_line` decoration provider callbacks,
/// if necessary (`decor_conceal_line`).
///
/// `check_cursor`: if `true`, avoid an early return for an
/// unconcealed cursorline. Accepted for signature fidelity but
/// genuinely unused by the real, always-taken early-return path
/// translated here (see this module's own doc comment) - the clause
/// that reads it is short-circuited away before ever being evaluated.
///
/// # Safety
/// `wp.w_buffer` must be a valid, non-null pointer to a live `BufT`
/// (forwarded to the real marktree-scanning path, unreachable today).
#[must_use]
pub unsafe fn decor_conceal_line(wp: &WinT, row: i32, _check_cursor: bool) -> bool {
    if row < 0 || wp.w_onebuf_opt.wo_cole < 2 {
        return false;
    }
    unimplemented!(
        "decoration::decor_conceal_line: the real marktree-scanning/decoration-provider path is \
         not yet translated - unreachable in practice today since 'conceallevel' can never be \
         raised above its default of 0, see this module's own doc comment"
    );
}

/// Return the number of rows occupied by the virtual lines attached
/// between `start_row` and `end_row` (`decor_virt_lines`).
///
/// `apply_folds`: only count virtual lines that are not in folds.
/// Accepted for signature fidelity but genuinely unused by the real,
/// always-taken early-return path translated here (see this module's
/// own doc comment).
///
/// # Safety
/// `wp.w_buffer` must be a valid, non-null pointer to a live `BufT`.
#[must_use]
pub unsafe fn decor_virt_lines(
    wp: &WinT,
    _start_row: i32,
    _end_row: i32,
    _num_below: Option<&mut i32>,
    _lines: Option<&mut VirtLines>,
    _apply_folds: bool,
) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*wp.w_buffer };
    if crate::buffer::buf_meta_total(buf, MetaIndex::Lines) == 0 {
        // Only pay for what you use: in case virt_lines feature is
        // not active in a buffer, plines do not need to access the
        // marktree at all.
        return 0;
    }
    unimplemented!(
        "decoration::decor_virt_lines: the real marktree-scanning path is not yet translated - \
         unreachable in practice today since nothing can attach virtual lines to any buffer, \
         see this module's own doc comment"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{BufT, WinoptT};

    fn win_with_cole(cole: crate::types_defs::OptInt, buf: *mut BufT) -> WinT {
        WinT { w_onebuf_opt: WinoptT { wo_cole: cole, ..Default::default() }, w_buffer: buf, ..Default::default() }
    }

    #[test]
    fn decor_conceal_line_false_by_default_conceallevel() {
        let mut buf = BufT::default();
        let wp = win_with_cole(0, &mut buf as *mut BufT);
        assert!(!unsafe { decor_conceal_line(&wp, 0, false) });
    }

    #[test]
    fn decor_conceal_line_false_for_negative_row_regardless_of_conceallevel() {
        let mut buf = BufT::default();
        let wp = win_with_cole(0, &mut buf as *mut BufT);
        assert!(!unsafe { decor_conceal_line(&wp, -1, false) });
    }

    #[test]
    #[should_panic(expected = "not yet translated")]
    fn decor_conceal_line_panics_when_conceallevel_is_2_or_higher() {
        // Not achievable via any real translated function yet (nothing
        // can raise 'conceallevel') - pokes it directly to prove the
        // real, faithfully-translated short-circuit condition is in
        // place, independent of how wo_cole eventually gets set.
        let mut buf = BufT::default();
        let wp = win_with_cole(2, &mut buf as *mut BufT);
        let _ = unsafe { decor_conceal_line(&wp, 0, false) };
    }

    #[test]
    fn decor_virt_lines_zero_when_no_virt_lines_meta() {
        let mut buf = BufT::default();
        let wp = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        assert_eq!(unsafe { decor_virt_lines(&wp, 0, 1, None, None, true) }, 0);
    }

    #[test]
    #[should_panic(expected = "not yet translated")]
    fn decor_virt_lines_panics_when_meta_total_is_nonzero() {
        // Not achievable via any real translated function yet (nothing
        // can attach virtual lines) - pokes the marktree meta_root
        // directly to prove the real, faithfully-translated check is
        // in place, independent of how it eventually gets populated.
        let mut buf = BufT::default();
        buf.b_marktree.meta_root[MetaIndex::Lines as usize] = 1;
        let wp = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let _ = unsafe { decor_virt_lines(&wp, 0, 1, None, None, true) };
    }
}
