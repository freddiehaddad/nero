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
//! Also translated: [`win_lines_concealed`] - fully real and complete
//! (not a "real early-return path" translation like the two above),
//! since its only two dependencies, `crate::fold::has_any_folding`
//! and `wp.w_onebuf_opt.wo_cole`, are both already real. Used by
//! `move.c`'s `check_top_offset`.
//!
//! Deferred: everything else in the file - real virtual-text/
//! highlight/conceal rendering, needing the marktree query machinery
//! and decoration-provider Lua callbacks, neither translated.

use crate::buffer::buf_meta_total;
use crate::buffer_defs::BufT;
use crate::buffer_defs::WinT;
use crate::decoration_defs::VirtLines;
use crate::types_defs::TriState;
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

/// Return `true` when `wp` may have concealed lines: either real
/// folds exist, or `'conceallevel'` hides whole lines (`>= 2`)
/// (`win_lines_concealed`). Fully real and complete - needs only
/// already-translated [`crate::fold::has_any_folding`] and
/// `wp.w_onebuf_opt.wo_cole`.
///
/// # Safety
/// Same as [`crate::fold::has_any_folding`].
#[must_use]
pub unsafe fn win_lines_concealed(wp: &WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { crate::fold::has_any_folding(wp) }) || wp.w_onebuf_opt.wo_cole >= 2
}

/// Count the number of signs in a range after adding or removing a
/// sign, or to (re-)initialize a range in `buf.b_signcols.count`
/// (`buf_signcols_count_range`).
///
/// `add` is `1`, `-1` or `0` for an added, deleted or initialized
/// range. `clear` is `False`, `True` or `None` for an added/deleted,
/// cleared, or initialized range.
///
/// # Scope
///
/// The guard is translated in full and is always taken today, so this
/// function is complete as written for every reachable call.
///
/// Its `!buf_meta_total(buf, MetaIndex::SignText)` operand is always
/// true, because nothing translated can attach a sign-text extmark to
/// any buffer yet - the same real "nothing has been registered"
/// condition [`decor_virt_lines`] relies on just above. The
/// `!buf.b_signcols.autom` operand is likewise true for every buffer
/// this crate can build, and `||` short-circuits before either of the
/// later operands is even evaluated.
///
/// The counting body behind that guard is `unimplemented!()`: it needs
/// `marktree_itr_get_overlap`, `marktree_itr_step_overlap` and
/// `marktree_itr_step_out_filter`, none of which are translated (the
/// overlap-iterator variants deferred in `marktree.rs`). It is
/// unreachable while no sign-text extmark can exist.
pub fn buf_signcols_count_range(buf: &mut BufT, row1: i32, row2: i32, add: i32, clear: TriState) {
    if !buf.b_signcols.autom || row2 < row1 || buf_meta_total(buf, MetaIndex::SignText) == 0 {
        return;
    }

    let _ = (add, clear);
    unimplemented!(
        "sign counting needs marktree_itr_get_overlap/_step_overlap/_step_out_filter, \
         not yet translated; unreachable while no sign-text extmark can exist"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{BufT, WinoptT};

    /// The default buffer has `autom == false`, so the guard's very
    /// first operand already ends the call - this is the shape every
    /// currently-reachable call takes.
    #[test]
    fn signcols_count_range_returns_early_without_auto_signcolumn() {
        let mut buf = BufT::default();
        assert!(!buf.b_signcols.autom);
        buf_signcols_count_range(&mut buf, 0, 5, 1, TriState::False);
        assert_eq!(buf.b_signcols.max, 0);
        assert_eq!(buf.b_signcols.count[0], 0);
    }

    /// With `autom` forced on, an inverted range still returns before
    /// the marktree is consulted.
    #[test]
    fn signcols_count_range_returns_early_for_an_inverted_range() {
        let mut buf = BufT::default();
        buf.b_signcols.autom = true;
        buf_signcols_count_range(&mut buf, 7, 3, 1, TriState::None);
        assert_eq!(buf.b_signcols.max, 0);
    }

    /// The operand that stays true for real sessions until an extmark
    /// API exists: a buffer with no sign-text mark at all.
    #[test]
    fn signcols_count_range_returns_early_without_any_sign_text_mark() {
        let mut buf = BufT::default();
        buf.b_signcols.autom = true;
        assert_eq!(buf_meta_total(&buf, MetaIndex::SignText), 0);
        buf_signcols_count_range(&mut buf, 0, 0, -1, TriState::True);
        assert_eq!(buf.b_signcols.max, 0);
    }

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

    #[test]
    fn win_lines_concealed_false_by_default() {
        let mut buf = BufT::default();
        let wp = win_with_cole(0, &mut buf as *mut BufT);
        assert!(!unsafe { win_lines_concealed(&wp) });
    }

    #[test]
    fn win_lines_concealed_true_when_conceallevel_is_2_or_higher() {
        let mut buf = BufT::default();
        let wp = win_with_cole(2, &mut buf as *mut BufT);
        assert!(unsafe { win_lines_concealed(&wp) });
    }

    #[test]
    fn win_lines_concealed_true_when_folding_may_exist_even_with_conceallevel_0() {
        let mut buf = BufT::default();
        let wp = WinT {
            // 'foldenable' on, 'foldmethod' unset (NOT "manual" by
            // default) - has_any_folding's own "no folds" fast path
            // only applies when foldmethod IS manual with no real
            // folds, so this genuinely reports true.
            w_onebuf_opt: WinoptT { wo_fen: 1, ..Default::default() },
            w_buffer: &mut buf as *mut BufT,
            ..Default::default()
        };
        assert!(unsafe { win_lines_concealed(&wp) });
    }
}
