//! Translated from `src/nvim/linematch.c`: the "linematch" algorithm used
//! by diff mode to align individual lines within a diff hunk across 2 or
//! more buffers (an `int**`-indexed dynamic-programming search over an
//! N-dimensional tensor, one dimension per compared buffer).
//!
//! `mmfile_t` (the original's xdiff-derived `{ ptr, size }` byte-buffer
//! type) is represented here simply as `&[u8]` - the same "idiomatic
//! native equivalent instead of an FFI-flavored struct" translation this
//! crate already applies to `char_u *`-plus-length elsewhere (e.g.
//! `charset.rs`, `mbyte.rs`). A buffer with zero content (the original's
//! `{ .ptr = NULL, .size = 0 }`, used by `diff.c`'s caller when a diff
//! block has zero lines in some buffer) is simply the empty slice `&[]`,
//! which already behaves correctly everywhere the original relies on
//! `memchr`/pointer-arithmetic no-ops for a NULL/zero-size buffer.
//!
//! This file is a single, self-contained, pure computational algorithm
//! with no dependency on any not-yet-translated subsystem - its only
//! caller (`diff.c`'s `diff_read_line`/linematch integration) is part of
//! the not-yet-translated real diff-computation engine (see `diff.rs`'s
//! own module doc comment).
//!
//! Translated so far: `line_len`/`matching_chars`/`matching_chars_iwhite`
//! - the core "how well do these two lines match" primitives (a
//! longest-common-subsequence-style character count). The rest of the
//! file (the N-dimensional tensor search itself) is translated
//! incrementally in later commits.
//!
//! Also note: `LN_MAX_BUFS`/`LN_DECISION_MAX` are declared here even
//! though only used by later additions to this file, since they are
//! genuine top-of-file constants in the original (`#define`s just above
//! `diffcmppath_S`) rather than something specific to any one function.

/// Maximum number of buffers `linematch_nbuffers` can compare at once
/// (`LN_MAX_BUFS`).
pub const LN_MAX_BUFS: usize = 8;

/// `pow(2, LN_MAX_BUFS) - 1` - the maximum number of distinct non-empty
/// "which buffers advance" bitmask choices (`LN_DECISION_MAX`).
pub const LN_DECISION_MAX: usize = 255;

/// Cap on how many bytes of a single line are considered when scoring how
/// well two lines match (`MATCH_CHAR_MAX_LEN`).
const MATCH_CHAR_MAX_LEN: usize = 800;

/// Length of the first line in `m` (the byte offset of the first `b'\n'`),
/// or `m.len()` itself if `m` contains no newline at all (`line_len`).
fn line_len(m: &[u8]) -> usize {
    m.iter().position(|&b| b == b'\n').unwrap_or(m.len())
}

/// Returns the number of matching characters between the first lines of
/// `m1` and `m2`, respecting sequence order - i.e. the length of their
/// longest common subsequence, computed over at most
/// `MATCH_CHAR_MAX_LEN - 1` bytes of each (`matching_chars`).
///
/// Examples (matching the original's own doc comment):
/// `matching_chars(b"aabc", b"acba")` is 2 (`'a'` and `'b'` in common);
/// `matching_chars(b"123hello567", b"he123ll567o")` is 8 (`"123"`, `"ll"`
/// and `"567"` in common); `matching_chars(b"abcdefg", b"gfedcba")` is 1
/// (every character is in common, but only 1 at a time in sequence).
fn matching_chars(m1: &[u8], m2: &[u8]) -> i32 {
    let s1len = line_len(m1).min(MATCH_CHAR_MAX_LEN - 1);
    let s2len = line_len(m2).min(MATCH_CHAR_MAX_LEN - 1);
    let s1 = &m1[..s1len];
    let s2 = &m2[..s2len];

    // Rolling 2-row longest-common-subsequence table: `matrix[icur]` is
    // the row currently being computed (indexed by `j + 1` for column
    // `j`), `matrix[1 - icur]` is the previous row - saves memory over a
    // full `s1len x s2len` table, matching the original exactly (which
    // stores only 2 rows of the `i` axis, indexed by a toggling `icur`).
    let mut matrix = [[0i32; MATCH_CHAR_MAX_LEN]; 2];
    let mut icur = 1usize;
    for &c1 in s1 {
        icur = 1 - icur;
        let prev = 1 - icur;
        for (j, &c2) in s2.iter().enumerate() {
            // skip char in s1
            if matrix[prev][j + 1] > matrix[icur][j + 1] {
                matrix[icur][j + 1] = matrix[prev][j + 1];
            }
            // skip char in s2
            if matrix[icur][j] > matrix[icur][j + 1] {
                matrix[icur][j + 1] = matrix[icur][j];
            }
            // compare char in s1 and s2
            if c1 == c2 && matrix[prev][j] + 1 > matrix[icur][j + 1] {
                matrix[icur][j + 1] = matrix[prev][j] + 1;
            }
        }
    }
    matrix[icur][s2len]
}

/// Same as [`matching_chars`], but ignores space/tab whitespace in both
/// lines first (`matching_chars_iwhite`).
fn matching_chars_iwhite(m1: &[u8], m2: &[u8]) -> i32 {
    // The original reads one byte *past* `line_len(s)` here (`i <= slen`,
    // not `i < slen`). That boundary byte is well-defined and safe to
    // read whenever `slen < s.len()` (it is simply the next real byte in
    // the buffer - typically the line's own `b'\n'` terminator, or,  if
    // `MATCH_CHAR_MAX_LEN - 1` capped `slen` short of the true line
    // length, an ordinary content byte) - both cases are reproduced
    // exactly here. The single case where the original's `s[slen]` would
    // read *past the end of the allocation* is a final line with no
    // trailing newline that also fits entirely within the cap (so
    // `slen == s.len()` exactly); `slice::get` is used instead of direct
    // indexing so that one case safely stops the loop instead of
    // reading undefined memory - there is no well-defined original
    // behavior to match there in the first place. Note that even a
    // legitimately-read boundary byte often ends up discarded anyway:
    // [`matching_chars`] re-derives its own `line_len`/cap truncation
    // over whatever `strip` returns, which removes a trailing `b'\n'`
    // carried over from here again.
    let strip = |s: &[u8]| -> Vec<u8> {
        let slen = line_len(s).min(MATCH_CHAR_MAX_LEN - 1);
        let mut out = Vec::with_capacity(slen + 1);
        for i in 0..=slen {
            let Some(&e) = s.get(i) else { break };
            if e != b' ' && e != b'\t' {
                out.push(e);
            }
        }
        out
    };
    matching_chars(&strip(m1), &strip(m2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_len_stops_at_newline() {
        assert_eq!(line_len(b"hello\nworld"), 5);
    }

    #[test]
    fn line_len_is_whole_buffer_without_newline() {
        assert_eq!(line_len(b"hello"), 5);
        assert_eq!(line_len(b""), 0);
    }

    #[test]
    fn matching_chars_examples_from_original_doc_comment() {
        assert_eq!(matching_chars(b"aabc", b"acba"), 2);
        assert_eq!(matching_chars(b"123hello567", b"he123ll567o"), 8);
        assert_eq!(matching_chars(b"abcdefg", b"gfedcba"), 1);
    }

    #[test]
    fn matching_chars_identical_lines_match_fully() {
        assert_eq!(matching_chars(b"identical", b"identical"), 9);
    }

    #[test]
    fn matching_chars_only_considers_first_line() {
        // Only "abc" (before the '\n') should be compared, not "zzz".
        assert_eq!(matching_chars(b"abc\nzzz", b"abc"), 3);
    }

    #[test]
    fn matching_chars_no_overlap_is_zero() {
        assert_eq!(matching_chars(b"abc", b"xyz"), 0);
    }

    #[test]
    fn matching_chars_iwhite_ignores_space_and_tab() {
        // "a b\tc" stripped of whitespace is "abc"; the trailing '\n'
        // carried over from both inputs' own line boundary is discarded
        // again by `matching_chars`'s own re-truncation (see that
        // function's doc comment), so the result is exactly as if
        // comparing "abc" against "abc".
        assert_eq!(matching_chars_iwhite(b"a b\tc\n", b"abc\n"), 3);
    }

    #[test]
    fn matching_chars_iwhite_final_line_without_newline_is_safe() {
        // No trailing newline and the line fits entirely within the cap:
        // the original would read one byte past the end here. Must not
        // panic, and must still compare the real content correctly.
        assert_eq!(matching_chars_iwhite(b"abc", b"abc"), 3);
    }
}
