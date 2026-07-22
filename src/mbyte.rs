//! Translated from `src/nvim/mbyte.c` (partial).
//!
//! Translated (this pass): the pure byte<->codepoint algorithms that
//! need no external library - `utf8len_tab`/`utf8len_tab_zero`,
//! `utf_byte2len`, `utf_ptr2len`, `utf_ptr2len_len`, `utf_ptr2char`,
//! `utf_char2len`, `utf_char2bytes`.
//!
//! `mbyte.c` as a whole (~3060 lines) is far larger than this pass:
//! most of the rest (composing-character/grapheme-cluster detection,
//! case folding, character width, encoding-name canonicalization,
//! `iconv`-based conversion) needs either the vendored `utf8proc` C
//! library (a genuine FFI-vs-crate decision, deliberately not made in
//! the same pass as these dependency-free algorithms) or `iconv`
//! (ditto, already flagged in `iconv_defs.rs`). This pass exists to
//! unblock the many callers (`path.c`'s `path_fnamencmp`, `mark.c`'s
//! `mark_mb_adjustpos`, etc.) that only need basic UTF-8 decode/encode
//! - not to translate the whole file at once.
//!
//! Deferred (need the `utf8proc` FFI dependency, not yet added):
//! `utf_composinglike`/`utfc_ptr2len`/`utfc_ptr2len_len` (grapheme
//! cluster boundaries), `utf_char2cells`/`ptr2cells` (character
//! display width), `utf_fold` (case folding), `mb_toupper`/
//! `mb_tolower` (case conversion), `mb_strnicmp`, `utf_head_off`, and
//! everything else in the file (encoding-name tables, `iconv`
//! conversion, composing-character legacy tables, etc.).

/// To speed up `BYTELEN()`; a lookup table to quickly get the length
/// in bytes of a UTF-8 character from the first byte of a UTF-8
/// string. Bytes which are illegal when used as the first byte have a
/// 1. The NUL byte has length 1 (`utf8len_tab`).
///
/// Mechanically extracted from the real `mbyte.c` source (not
/// hand-transcribed) and cross-checked against a from-scratch formula
/// derived from the standard UTF-8 lead-byte ranges - both agree on
/// all 256 entries (verified via a throwaway Python script during
/// translation, not committed).
#[rustfmt::skip]
pub const UTF8LEN_TAB: [u8; 256] = [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 1, 1,
];

/// Like [`UTF8LEN_TAB`] above, but using a zero for illegal lead bytes
/// (`utf8len_tab_zero`). Same mechanical-extraction-plus-formula-
/// cross-check verification as `UTF8LEN_TAB`.
#[rustfmt::skip]
pub const UTF8LEN_TAB_ZERO: [u8; 256] = [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 0, 0,
];

/// Return length of UTF-8 character, obtained from the first byte.
/// `b` must be between 0 and 255! Returns 1 for an invalid first byte
/// value (`utf_byte2len`).
#[must_use]
pub fn utf_byte2len(b: u8) -> u8 {
    UTF8LEN_TAB[b as usize]
}

/// Get the length of a UTF-8 byte sequence representing a single
/// codepoint.
///
/// @return Sequence length, 0 for empty string and 1 for non-UTF-8
///         byte sequence (`utf_ptr2len`).
///
/// The original operates on a NUL-terminated C string: if the claimed
/// multi-byte sequence runs past the end of the real content, the
/// scan naturally reaches the string's NUL terminator, which always
/// fails the continuation-byte check (`0x00 & 0xc0 == 0x00 != 0x80`),
/// so it correctly falls back to `1`. A Rust `&[u8]` has no implicit
/// terminator, so - to reproduce that same real-world stopping
/// behavior rather than optimistically assuming unseen bytes beyond
/// the slice are valid continuations (which could make a caller
/// slice out of bounds using the returned length) - running out of
/// slice partway through the expected sequence is treated exactly
/// like hitting a byte that fails the continuation check.
#[must_use]
pub fn utf_ptr2len(p: &[u8]) -> i32 {
    let Some(&b0) = p.first() else {
        return 0;
    };
    if b0 == 0 {
        return 0;
    }
    let len = UTF8LEN_TAB[b0 as usize];
    for i in 1..usize::from(len) {
        match p.get(i) {
            Some(&b) if (b & 0xc0) == 0x80 => {}
            _ => return 1, // continuation-byte check failed, or ran out of slice.
        }
    }
    i32::from(len)
}

/// Get the length of UTF-8 byte sequence `p[..size]`. Does not include
/// any following composing characters.
///
/// @return 1 for `""`, an illegal byte sequence (also in an incomplete
///         byte sequence), or `size == 0`; a number greater than
///         `size` for an incomplete byte sequence; never zero
///         otherwise (`utf_ptr2len_len`).
///
/// Callers are responsible for `size <= p.len()` (matching the
/// original's own contract of `size` being how many bytes are valid
/// starting at `p`): unlike [`utf_ptr2len`], this never needs to treat
/// "ran out of slice" specially, since the scan is already bounded by
/// the caller-supplied `size`, not by an implicit NUL terminator.
#[must_use]
pub fn utf_ptr2len_len(p: &[u8], size: usize) -> i32 {
    if size == 0 {
        return 1;
    }
    let len = UTF8LEN_TAB[p[0] as usize];
    if len == 1 {
        return 1; // NUL, ascii or illegal lead byte
    }
    let m = if usize::from(len) > size { size } else { usize::from(len) };
    for &b in p.iter().take(m).skip(1) {
        if (b & 0xc0) != 0x80 {
            return 1;
        }
    }
    i32::from(len)
}

/// Convert a UTF-8 byte sequence to a character number.
///
/// If the sequence is illegal or truncated by a NUL then the first
/// byte is returned. For an overlong sequence this may return zero.
/// Does not include composing characters for obvious reasons
/// (`utf_ptr2char`).
///
/// @return Unicode codepoint or byte value.
///
/// # Panics
/// If `p` is empty (the original requires a non-null, NUL-terminated
/// string; an empty slice has no analogous "first byte" to fall back
/// on).
#[must_use]
pub fn utf_ptr2char(p: &[u8]) -> i32 {
    let v0 = u32::from(p[0]);
    if v0 < 0x80 {
        // Be quick for ASCII.
        return v0 as i32;
    }

    let len = UTF8LEN_TAB[v0 as usize];
    if len < 2 {
        return v0 as i32;
    }

    // Matches the original's CHECK/LEN_RETURN/S macros exactly, just
    // spelled out instead of using preprocessor macros: each
    // continuation byte must be 0b10xxxxxx, and the final codepoint is
    // reassembled by shifting each byte's low 6 (or 7, for the lead
    // byte) bits into place and subtracting the fixed lead-byte-marker
    // contribution.
    let is_continuation = |b: u32| (b & 0xC0) == 0x80;

    let v1 = u32::from(*p.get(1).unwrap_or(&0));
    if !is_continuation(v1) {
        return v0 as i32;
    }
    if len == 2 {
        return ((v0 << 6) + v1 - ((0xC0 << 6) + 0x80)) as i32;
    }

    let v2 = u32::from(*p.get(2).unwrap_or(&0));
    if !is_continuation(v2) {
        return v0 as i32;
    }
    if len == 3 {
        return ((v0 << 12) + (v1 << 6) + v2 - ((0xE0 << 12) + (0x80 << 6) + 0x80)) as i32;
    }

    let v3 = u32::from(*p.get(3).unwrap_or(&0));
    if !is_continuation(v3) {
        return v0 as i32;
    }
    if len == 4 {
        return ((v0 << 18) + (v1 << 12) + (v2 << 6) + v3
            - ((0xF0 << 18) + (0x80 << 12) + (0x80 << 6) + 0x80)) as i32;
    }

    let v4 = u32::from(*p.get(4).unwrap_or(&0));
    if !is_continuation(v4) {
        return v0 as i32;
    }
    if len == 5 {
        return ((v0 << 24) + (v1 << 18) + (v2 << 12) + (v3 << 6) + v4
            - ((0xF8 << 24) + (0x80 << 18) + (0x80 << 12) + (0x80 << 6) + 0x80))
            as i32;
    }

    let v5 = u32::from(*p.get(5).unwrap_or(&0));
    if !is_continuation(v5) {
        return v0 as i32;
    }
    // len == 6
    ((v0 << 30) + (v1 << 24) + (v2 << 18) + (v3 << 12) + (v4 << 6) + v5
        - ((0xFC << 30) + (0x80 << 24) + (0x80 << 18) + (0x80 << 12) + (0x80 << 6) + 0x80))
        as i32
}

/// Determine how many bytes certain unicode codepoint will occupy
/// (`utf_char2len`).
#[must_use]
pub fn utf_char2len(c: i32) -> i32 {
    if c < 0x80 {
        1
    } else if c < 0x800 {
        2
    } else if c < 0x10000 {
        3
    } else if c < 0x200000 {
        4
    } else if c < 0x4000000 {
        5
    } else {
        6
    }
}

/// Convert Unicode character to UTF-8 string (`utf_char2bytes`).
///
/// `buf` must have room for at least 6 bytes (`MB_MAXBYTES`'s
/// underlying single-character length, [`crate::mbyte_defs::MB_MAXCHAR`]).
///
/// @return Number of bytes (1-6) written to the front of `buf`.
///
/// # Panics
/// If `buf` has fewer than [`utf_char2len`]`(c)` bytes of room.
#[must_use]
pub fn utf_char2bytes(c: i32, buf: &mut [u8]) -> i32 {
    let c = c as u32;
    if c < 0x80 {
        // 7 bits
        buf[0] = c as u8;
        1
    } else if c < 0x800 {
        // 11 bits
        buf[0] = (0xc0 + (c >> 6)) as u8;
        buf[1] = (0x80 + (c & 0x3f)) as u8;
        2
    } else if c < 0x10000 {
        // 16 bits
        buf[0] = (0xe0 + (c >> 12)) as u8;
        buf[1] = (0x80 + ((c >> 6) & 0x3f)) as u8;
        buf[2] = (0x80 + (c & 0x3f)) as u8;
        3
    } else if c < 0x200000 {
        // 21 bits
        buf[0] = (0xf0 + (c >> 18)) as u8;
        buf[1] = (0x80 + ((c >> 12) & 0x3f)) as u8;
        buf[2] = (0x80 + ((c >> 6) & 0x3f)) as u8;
        buf[3] = (0x80 + (c & 0x3f)) as u8;
        4
    } else if c < 0x4000000 {
        // 26 bits
        buf[0] = (0xf8 + (c >> 24)) as u8;
        buf[1] = (0x80 + ((c >> 18) & 0x3f)) as u8;
        buf[2] = (0x80 + ((c >> 12) & 0x3f)) as u8;
        buf[3] = (0x80 + ((c >> 6) & 0x3f)) as u8;
        buf[4] = (0x80 + (c & 0x3f)) as u8;
        5
    } else {
        // 31 bits
        buf[0] = (0xfc + (c >> 30)) as u8;
        buf[1] = (0x80 + ((c >> 24) & 0x3f)) as u8;
        buf[2] = (0x80 + ((c >> 18) & 0x3f)) as u8;
        buf[3] = (0x80 + ((c >> 12) & 0x3f)) as u8;
        buf[4] = (0x80 + ((c >> 6) & 0x3f)) as u8;
        buf[5] = (0x80 + (c & 0x3f)) as u8;
        6
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8len_tab_matches_hand_derived_formula_for_all_256_bytes() {
        // Cross-checked mechanically (not by eye) against the real
        // mbyte.c source during translation; this test re-derives the
        // same formula independently as a standing regression check.
        for b in 0u32..=255 {
            let expected = match b {
                0x00..=0x7F => 1,
                0x80..=0xBF => 1, // illegal as a lead byte
                0xC0..=0xDF => 2,
                0xE0..=0xEF => 3,
                0xF0..=0xF7 => 4,
                0xF8..=0xFB => 5,
                0xFC..=0xFD => 6,
                _ => 1, // 0xFE, 0xFF: illegal
            };
            assert_eq!(UTF8LEN_TAB[b as usize], expected, "byte {b:#04x}");

            let expected_zero = match b {
                0x80..=0xBF | 0xFE..=0xFF => 0,
                _ => expected,
            };
            assert_eq!(UTF8LEN_TAB_ZERO[b as usize], expected_zero, "byte {b:#04x}");
        }
    }

    #[test]
    fn utf_byte2len_matches_table() {
        assert_eq!(utf_byte2len(b'A'), 1);
        assert_eq!(utf_byte2len(0xC2), 2);
        assert_eq!(utf_byte2len(0x80), 1); // illegal lead byte
    }

    #[test]
    fn utf_ptr2len_handles_empty_ascii_and_multibyte() {
        assert_eq!(utf_ptr2len(b""), 0);
        assert_eq!(utf_ptr2len(b"\0"), 0);
        assert_eq!(utf_ptr2len(b"A"), 1);
        assert_eq!(utf_ptr2len("é".as_bytes()), 2); // U+00E9, 2-byte UTF-8
        assert_eq!(utf_ptr2len("日".as_bytes()), 3); // U+65E5, 3-byte UTF-8
        assert_eq!(utf_ptr2len("😀".as_bytes()), 4); // U+1F600, 4-byte UTF-8
    }

    #[test]
    fn utf_ptr2len_returns_1_for_truncated_multibyte_sequence() {
        // A 3-byte lead byte with only 1 continuation byte following -
        // truncated, so the trailing continuation-byte check fails.
        let bytes = "日".as_bytes();
        assert_eq!(utf_ptr2len(&bytes[..2]), 1);
    }

    #[test]
    fn utf_ptr2len_len_reports_incomplete_sequences_past_size() {
        let bytes = "日".as_bytes(); // 3-byte sequence
        assert_eq!(utf_ptr2len_len(bytes, 3), 3);
        // Only 1 byte available but the lead byte claims 3 - and that
        // 1 available byte isn't even a valid continuation byte
        // check target (m = min(len, size) = 1, loop doesn't run) -
        // so this returns the full claimed length (3), matching the
        // original's ">  size" incomplete-sequence contract.
        assert_eq!(utf_ptr2len_len(bytes, 1), 3);
    }

    #[test]
    fn utf_ptr2char_decodes_ascii_and_multibyte_correctly() {
        assert_eq!(utf_ptr2char(b"A"), i32::from(b'A'));
        assert_eq!(utf_ptr2char("é".as_bytes()), 0xE9);
        assert_eq!(utf_ptr2char("日".as_bytes()), 0x65E5);
        assert_eq!(utf_ptr2char("😀".as_bytes()), 0x1F600);
    }

    #[test]
    fn utf_ptr2char_falls_back_to_first_byte_for_illegal_sequence() {
        // 0xC2 is a valid 2-byte lead, but followed by an ASCII byte
        // (not a continuation byte) - illegal, falls back to the lead
        // byte's own value.
        assert_eq!(utf_ptr2char(&[0xC2, b'A']), 0xC2);
    }

    #[test]
    fn utf_char2len_matches_utf8_boundary_table() {
        assert_eq!(utf_char2len(0x00), 1);
        assert_eq!(utf_char2len(0x7F), 1);
        assert_eq!(utf_char2len(0x80), 2);
        assert_eq!(utf_char2len(0x7FF), 2);
        assert_eq!(utf_char2len(0x800), 3);
        assert_eq!(utf_char2len(0xFFFF), 3);
        assert_eq!(utf_char2len(0x10000), 4);
        assert_eq!(utf_char2len(0x1FFFFF), 4);
        assert_eq!(utf_char2len(0x200000), 5);
        assert_eq!(utf_char2len(0x3FFFFFF), 5);
        assert_eq!(utf_char2len(0x4000000), 6);
    }

    #[test]
    fn utf_char2bytes_and_utf_ptr2char_roundtrip_for_various_codepoints() {
        for &c in &[0x41, 0xE9, 0x65E5, 0x1F600] {
            let mut buf = [0u8; 6];
            let len = utf_char2bytes(c, &mut buf);
            assert_eq!(utf_char2len(c), len);
            assert_eq!(utf_ptr2char(&buf[..len as usize]), c);
            assert_eq!(utf_ptr2len(&buf[..len as usize]), len);
        }
    }

    #[test]
    fn utf_char2bytes_matches_known_encodings() {
        let mut buf = [0u8; 6];
        assert_eq!(utf_char2bytes(b'A' as i32, &mut buf), 1);
        assert_eq!(&buf[..1], b"A");

        let mut buf = [0u8; 6];
        assert_eq!(utf_char2bytes(0xE9, &mut buf), 2);
        assert_eq!(&buf[..2], "é".as_bytes());

        let mut buf = [0u8; 6];
        assert_eq!(utf_char2bytes(0x1F600, &mut buf), 4);
        assert_eq!(&buf[..4], "😀".as_bytes());
    }
}
