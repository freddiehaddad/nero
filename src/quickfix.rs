//! Translated from `src/nvim/quickfix.c` (tractable core only).
//!
//! `quickfix.c` (~7000 lines) implements the `:make`/`:grep`/`:cfile`/
//! `:copen` quickfix-and-location-list subsystem: parsing compiler/grep
//! output into a `qf_list_T` of `qfline_T` entries, the quickfix window,
//! error navigation (`:cnext`/`:cprev`/etc.), and much more. Almost every
//! function needs `qf_info_T`'s real fields (still just an opaque
//! placeholder, [`crate::types_defs::QfInfoT`]), `errorformat` parsing,
//! or the quickfix window/buffer UI - none of which exist yet.
//!
//! Translated: [`qf_mark_adjust`] - `mark_adjust_buf`'s (`mark.c`) own
//! real dependency, needed to translate that function faithfully.
//! `qf_mark_adjust`'s OWN first, real check
//! (`buf->b_has_qf_entry & buf_has_flag`) is always false in this crate
//! today: nothing can currently populate `BufT.b_has_qf_entry` with
//! either flag (that only happens when a real quickfix/location list
//! actually gains an entry pointing at the buffer, via
//! `qf_fill_buffer`/`qf_alloc_entry`-style internals, none translated) -
//! so this function's entire remaining body (the real per-list,
//! per-entry line-number-adjustment walk) is genuinely, provably
//! unreachable today, and is not translated at all (not even as an
//! `unimplemented!()` stub) - matching this crate's established
//! "translate only the real, reachable fast path" precedent (e.g.
//! `win_lines_concealed`/`decor_conceal_line`/the whole `AUTOCMDS`-
//! empty-registry family).
//!
//! Deferred: everything else in the file.

use crate::buffer_defs::{BufT, WinT, BUF_HAS_LL_ENTRY, BUF_HAS_QF_ENTRY};

/// Adjust quickfix/location-list error entries for changed line numbers.
///
/// `wp` is `None` to check the quickfix list, or `Some` for a potential
/// location list into `buf`.
///
/// Always returns `false` in this crate today: `buf.b_has_qf_entry`
/// can never hold either [`BUF_HAS_QF_ENTRY`]/[`BUF_HAS_LL_ENTRY`] flag
/// yet (see this module's own doc comment), so the original's own
/// first, real check (`if (!(buf->b_has_qf_entry & buf_has_flag))
/// return false;`) is unconditionally taken - the rest of the
/// original's body (walking `qi->qf_lists[]`'s real entries) is
/// genuinely unreachable and not translated.
#[must_use]
pub fn qf_mark_adjust(
    buf: &BufT,
    wp: Option<&WinT>,
    _line1: crate::pos_defs::LinenrT,
    _line2: crate::pos_defs::LinenrT,
    _amount: crate::pos_defs::LinenrT,
    _amount_after: crate::pos_defs::LinenrT,
) -> bool {
    let buf_has_flag = if wp.is_none() { BUF_HAS_QF_ENTRY } else { BUF_HAS_LL_ENTRY };
    if buf.b_has_qf_entry & buf_has_flag == 0 {
        return false;
    }
    unreachable!(
        "qf_mark_adjust's real entry-adjustment body is unreachable today: nothing in this \
         crate can set BufT.b_has_qf_entry to a nonzero value yet, see this module's own doc \
         comment"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_false_for_the_quickfix_list_when_b_has_qf_entry_is_unset() {
        let buf = BufT::default();
        assert!(!qf_mark_adjust(&buf, None, 1, 5, 2, 0));
    }

    #[test]
    fn always_false_for_a_location_list_when_b_has_qf_entry_is_unset() {
        let buf = BufT::default();
        let wp = WinT::default();
        assert!(!qf_mark_adjust(&buf, Some(&wp), 1, 5, 2, 0));
    }

    #[test]
    fn checks_the_ll_entry_flag_specifically_when_wp_is_some() {
        // BUF_HAS_QF_ENTRY alone set, but wp is Some (location list) -
        // must check BUF_HAS_LL_ENTRY, not BUF_HAS_QF_ENTRY, so this
        // still hits the real "no ll entry" false-return fast path.
        let buf = BufT { b_has_qf_entry: BUF_HAS_QF_ENTRY, ..BufT::default() };
        let wp = WinT::default();
        assert!(!qf_mark_adjust(&buf, Some(&wp), 1, 5, 2, 0));
    }

    #[test]
    fn checks_the_qf_entry_flag_specifically_when_wp_is_none() {
        // BUF_HAS_LL_ENTRY alone set, but wp is None (quickfix list) -
        // must check BUF_HAS_QF_ENTRY, not BUF_HAS_LL_ENTRY.
        let buf = BufT { b_has_qf_entry: BUF_HAS_LL_ENTRY, ..BufT::default() };
        assert!(!qf_mark_adjust(&buf, None, 1, 5, 2, 0));
    }

    #[test]
    #[should_panic(expected = "qf_mark_adjust's real entry-adjustment body is unreachable today")]
    fn panics_if_the_real_flag_is_ever_actually_set() {
        // Demonstrates precisely where the real, not-yet-translated
        // body would begin, in case a future change ever lets
        // something set this flag for real.
        let buf = BufT { b_has_qf_entry: BUF_HAS_QF_ENTRY, ..BufT::default() };
        let _ = qf_mark_adjust(&buf, None, 1, 5, 2, 0);
    }
}
