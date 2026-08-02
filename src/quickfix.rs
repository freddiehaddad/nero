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
//! empty-registry family). Also [`qf_fmt_text`] - a pure string
//! transform (collapse a newline plus any immediately-following
//! whitespace/newlines into a single space) that only touches a
//! caller-provided [`crate::garray_defs::GarrayT`] output buffer and a
//! plain `&[u8]` input, with no dependency on `qf_info_T`/`qf_list_T`
//! at all (unlike most of this file).
//!
//! Deferred: everything else in the file - in particular, every OTHER
//! function operating on `qf_info_T`/`qf_list_T`'s own real fields
//! (e.g. `qf_stack_empty`/`qf_list_empty`/`qf_cmdtitle`/
//! `qf_store_title`) remains genuinely blocked: those structs are
//! still just an opaque placeholder
//! ([`crate::types_defs::QfInfoT`]) with no real fields translated -
//! individually "small" helper functions built on top of them are NOT
//! tractable until the underlying quickfix-list storage itself is
//! translated (a separate, substantial undertaking).

use crate::buffer_defs::{BufT, WinT, BUF_HAS_LL_ENTRY, BUF_HAS_QF_ENTRY};
use crate::garray_defs::GarrayT;

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

/// Convert `text`, replacing a newline and any immediately-following
/// whitespace/newlines with a single space, appending the result byte
/// by byte to `gap` (`qf_fmt_text`).
pub fn qf_fmt_text(gap: &mut GarrayT, text: &[u8]) {
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'\n' {
            gap.ga_append(b' ');
            i += 1;
            while i < text.len() && (crate::ascii_defs::ascii_iswhite(i32::from(text[i])) || text[i] == b'\n') {
                i += 1;
            }
        } else {
            gap.ga_append(text[i]);
            i += 1;
        }
    }
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

    // --- qf_fmt_text ---

    fn fmt(text: &[u8]) -> Vec<u8> {
        let mut gap = GarrayT::new(1, 4);
        qf_fmt_text(&mut gap, text);
        gap.ga_data[..gap.ga_len as usize].to_vec()
    }

    #[test]
    fn qf_fmt_text_empty_is_unchanged() {
        assert_eq!(fmt(b""), b"");
    }

    #[test]
    fn qf_fmt_text_plain_text_with_no_newline_is_unchanged() {
        assert_eq!(fmt(b"hello world"), b"hello world");
    }

    #[test]
    fn qf_fmt_text_single_newline_becomes_a_space() {
        assert_eq!(fmt(b"foo\nbar"), b"foo bar");
    }

    #[test]
    fn qf_fmt_text_newline_followed_by_spaces_collapses_to_one_space() {
        assert_eq!(fmt(b"foo\n   bar"), b"foo bar");
    }

    #[test]
    fn qf_fmt_text_multiple_consecutive_newlines_collapse_to_one_space() {
        assert_eq!(fmt(b"foo\n\n\nbar"), b"foo bar");
    }

    #[test]
    fn qf_fmt_text_trailing_newline_with_nothing_after() {
        assert_eq!(fmt(b"foo\n"), b"foo ");
    }

    #[test]
    fn qf_fmt_text_leading_newline() {
        assert_eq!(fmt(b"\nfoo"), b" foo");
    }

    #[test]
    fn qf_fmt_text_mixed_whitespace_and_newline_run_all_absorbed() {
        // space, tab, newline, space - all whitespace-or-newline, so
        // ALL are absorbed after the first newline triggers the
        // single space append.
        assert_eq!(fmt(b"a\n \t\n b"), b"a b");
    }

    #[test]
    fn qf_fmt_text_tab_alone_is_preserved_as_is_when_not_after_a_newline() {
        // A tab NOT immediately following a newline is just a plain
        // byte, copied through unchanged (only whitespace immediately
        // AFTER a newline gets absorbed).
        assert_eq!(fmt(b"a\tb"), b"a\tb");
    }
}
