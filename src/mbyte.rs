//! Translated from `src/nvim/mbyte.c` (partial).
//!
//! Translated: the pure byte<->codepoint algorithms that need no
//! external library - `utf8len_tab`/`utf8len_tab_zero`,
//! `utf_byte2len`, `utf_ptr2len`, `utf_ptr2len_len`, `utf_ptr2char`,
//! `utf_char2len`, `utf_char2bytes`, `utf_safe_read_char_adv`
//! (`static`/private), `utf_strnicmp`, `mb_strnicmp`, `mb_stricmp`
//! (trivial `mb_strnicmp(s1, s2, MAXCOL)` wrapper) - plus, now that
//! the `utf8proc-sys` FFI dependency has actually been added (see
//! `Cargo.toml`'s own comment recording that decision):
//! `utf_iscomposing_first`, `utf_composinglike`, `utf_iscomposing`,
//! `utfc_ptr2len`, `utfc_ptr2len_len`, `utf_fold`, `mb_toupper`/
//! `mb_tolower`/`mb_islower`/`mb_isupper`; character *display width*:
//! `intable`/`utf_printable` (the portable, non-`__SSE2__` reference
//! algorithm; the SSE2 intrinsics fast path is a pure optimization
//! producing identical results, not translated), `cw_value` (always
//! returns 0, since the real `cw_table` is populated only by the eval
//! engine's `setcellwidths()`, not yet translated, matching every real
//! session's default, unconfigured state exactly), `prop_is_emojilike`,
//! `utf_char2cells`, `utf_ptr2cells` (needs `charset.c`'s
//! `vim_isprintc`/`char2cells`, themselves needing a documented
//! default-table approximation of `g_chartab`; see `charset.rs`'s own
//! module doc for exactly what that means); and now the substantial
//! standalone backward-scanning algorithm this file is most known for:
//! `utf_ptr2CharInfo_impl` (as `utf_ptr2char_info_impl`, `static`/
//! private in the original too), `utf_ptr2CharInfo` (as
//! `utf_ptr2char_info`, an inline function in the original's own
//! header - now has a real caller in `strings.c`'s `mb_strup_buf`/
//! `strcase_save`), `always_break`/`always_break_two`
//! (`static`/private), and **`utf_head_off`** itself - the
//! bidirectional (backward-then-forward) grapheme-cluster-boundary
//! scan used to find where a composing-character sequence really
//! starts. Verified beyond ordinary unit tests: before writing any
//! test, the exact `boundclass`/`grapheme_break`/`arabic_combine`
//! values the algorithm depends on (for a lone CJK character, a
//! combining-mark pair, and two independent adjacent CJK characters)
//! were probed directly via a throwaway scratch test calling the real
//! `utf8proc_sys`/`arabic_combine` functions (not committed), then the
//! hand-traced expected offsets were cross-checked against those real
//! values before being written into the permanent test suite - all
//! passed on the first real run, confirming both the translation and
//! the by-hand trace of the algorithm were correct.
//!
//! `utf_ptr2char_info_impl` deliberately deviates from [`utf_ptr2char`]'s
//! `wrapping_*`-arithmetic pattern only in *how little* it needs to
//! read: since [`UTF8LEN_TAB`]-derived `len < 2` is always negative in
//! the original regardless of what a further, unconditional byte read
//! would show, this translation returns early instead of performing
//! that (potentially out-of-bounds, on a Rust slice) extra read - see
//! its own doc comment for the full reasoning. It reuses the same
//! `wrapping_*` discipline as [`utf_ptr2char`] for the reassembly
//! arithmetic itself, for the same overflow reason (see that
//! function's own doc comment): **translating this function's sibling
//! surfaced a genuine, pre-existing overflow-panic bug in
//! [`utf_ptr2char`] itself** (a maximal 6-byte lead byte with maximal
//! continuation bytes overflows the `u32` accumulation, `panic!`ing in
//! a debug build instead of the original C's well-defined unsigned
//! wraparound) - fixed in the same pass, with a dedicated regression
//! test using the exact adversarial byte sequence that reproduces it.
//!
//! `mbyte.c` as a whole (~3060 lines) is far larger than even this:
//! `utf_ptr2cells_len` (bounded-length sibling of `utf_ptr2cells`,
//! likely trivial once needed - not added speculatively without a real
//! caller); encoding-name canonicalization and `iconv`-based conversion
//! need the still-undecided `iconv` FFI (`iconv_defs.rs`). Each is its
//! own follow-up, not bundled in here.
//!
//! `mb_toupper`/`mb_tolower` have one narrow, documented gap: the
//! original also supports `'casemap'` with `"internal"` explicitly
//! removed (a rare, non-default configuration - `"internal"` is part
//! of the option's own default value, and nothing in this crate yet
//! parses `'casemap'` to produce any other value), which calls the
//! locale-sensitive `towupper()`/`towlower()`. Those aren't reliably
//! available across every platform this crate targets via the `libc`
//! crate (verified: no `wint_t` on this Windows target) - falls back
//! to the same "internal" behavior instead, documented as a narrow,
//! temporary gap on each function rather than a silent behavior
//! change.
//!
//! Deferred (need another not-yet-decided subsystem):
//! `utf_ptr2cells_len`, and everything else in the file (encoding-name
//! tables, `iconv` conversion, `show_utf8`, etc.).

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
    //
    // Uses `wrapping_*` throughout: the original's C `uint32_t` math
    // wraps silently on overflow (well-defined, matches this exactly),
    // but a plain `<<`/`+`/`-` on Rust's `u32` panics on overflow in
    // debug builds. A genuine 6-byte lead byte (0xFC/0xFD) with
    // maximal continuation bytes does overflow this accumulation
    // (verified via a standalone scratch reproduction before fixing) -
    // `wrapping_*` reproduces the original's real, intended wraparound
    // behavior instead of panicking.
    let is_continuation = |b: u32| (b & 0xC0) == 0x80;

    let v1 = u32::from(*p.get(1).unwrap_or(&0));
    if !is_continuation(v1) {
        return v0 as i32;
    }
    if len == 2 {
        return (v0.wrapping_shl(6).wrapping_add(v1).wrapping_sub((0xC0 << 6) + 0x80)) as i32;
    }

    let v2 = u32::from(*p.get(2).unwrap_or(&0));
    if !is_continuation(v2) {
        return v0 as i32;
    }
    if len == 3 {
        return (v0
            .wrapping_shl(12)
            .wrapping_add(v1.wrapping_shl(6))
            .wrapping_add(v2)
            .wrapping_sub((0xE0 << 12) + (0x80 << 6) + 0x80)) as i32;
    }

    let v3 = u32::from(*p.get(3).unwrap_or(&0));
    if !is_continuation(v3) {
        return v0 as i32;
    }
    if len == 4 {
        return (v0
            .wrapping_shl(18)
            .wrapping_add(v1.wrapping_shl(12))
            .wrapping_add(v2.wrapping_shl(6))
            .wrapping_add(v3)
            .wrapping_sub((0xF0 << 18) + (0x80 << 12) + (0x80 << 6) + 0x80)) as i32;
    }

    let v4 = u32::from(*p.get(4).unwrap_or(&0));
    if !is_continuation(v4) {
        return v0 as i32;
    }
    if len == 5 {
        return (v0
            .wrapping_shl(24)
            .wrapping_add(v1.wrapping_shl(18))
            .wrapping_add(v2.wrapping_shl(12))
            .wrapping_add(v3.wrapping_shl(6))
            .wrapping_add(v4)
            .wrapping_sub((0xF8 << 24) + (0x80 << 18) + (0x80 << 12) + (0x80 << 6) + 0x80))
            as i32;
    }

    let v5 = u32::from(*p.get(5).unwrap_or(&0));
    if !is_continuation(v5) {
        return v0 as i32;
    }
    // len == 6
    (v0.wrapping_shl(30)
        .wrapping_add(v1.wrapping_shl(24))
        .wrapping_add(v2.wrapping_shl(18))
        .wrapping_add(v3.wrapping_shl(12))
        .wrapping_add(v4.wrapping_shl(6))
        .wrapping_add(v5)
        .wrapping_sub((0xFC << 30) + (0x80 << 24) + (0x80 << 18) + (0x80 << 12) + (0x80 << 6) + 0x80))
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

/// When `c` is the first char of a string, determine if it needs to be
/// prefixed by a space byte to be drawn correctly, and not merge with
/// the space left of the string (`utf_iscomposing_first`).
#[must_use]
pub fn utf_iscomposing_first(c: i32) -> bool {
    // SAFETY: utf8proc_grapheme_break is a pure function with no
    // preconditions on its inputs.
    c >= 128 && !unsafe { utf8proc_sys::utf8proc_grapheme_break(b' ' as i32, c) }
}

/// Check if the character pointed to by `p2` is a composing character
/// when it comes after `p1`.
///
/// We use the definition in UAX#29 as implemented by utf8proc with the
/// following exceptions:
///
/// - ASCII chars always begin a new cluster. This is a long assumed
///   invariant in the code base and very useful for performance (we
///   can exit early for ASCII all over the place). As of Unicode 15.1
///   this will only break BOUNDCLASS_UREPEND followed by ASCII, which
///   should be exceedingly rare.
/// - When `'arabicshape'` is active, some pairs of arabic letters "ab"
///   are replaced with "c" taking one single cell, which behaves like
///   a cluster.
///
/// `state` should be set to [`crate::mbyte_defs::GRAPHEME_STATE_INIT`]
/// before the first call (`utf_composinglike`).
///
/// # Panics
/// If `p1` or `p2` is empty.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via `arabic_combine`) -
/// same requirement as every other function that does so.
#[must_use]
pub unsafe fn utf_composinglike(
    p1: &[u8],
    p2: &[u8],
    state: &mut crate::mbyte_defs::GraphemeState,
) -> bool {
    if p2[0] < 128 {
        return false;
    }

    let first = utf_ptr2char(p1);
    let second = utf_ptr2char(p2);

    // SAFETY: state is a valid, exclusively-borrowed i32 for the
    // duration of this call.
    if !unsafe { utf8proc_sys::utf8proc_grapheme_break_stateful(first, second, state) } {
        return true;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::arabic::arabic_combine(first, second) }
}

/// Same as [`utf_composinglike`] but operating on UCS-4 values
/// (`utf_iscomposing`).
///
/// # Safety
/// Same as [`utf_composinglike`].
#[must_use]
pub unsafe fn utf_iscomposing(
    c1: i32,
    c2: i32,
    state: &mut crate::mbyte_defs::GraphemeState,
) -> bool {
    // SAFETY: state is a valid, exclusively-borrowed i32.
    !unsafe { utf8proc_sys::utf8proc_grapheme_break_stateful(c1, c2, state) }
        // SAFETY: forwarded from this function's own safety doc.
        || unsafe { crate::arabic::arabic_combine(c1, c2) }
}

/// Return the number of bytes occupied by a UTF-8 character in a
/// string. This includes following composing characters. Returns zero
/// for an empty slice (`utfc_ptr2len`).
///
/// Like [`utf_ptr2len`], "running out of slice" partway through a
/// composing-character scan is treated the same as hitting a byte
/// that ends the sequence (see that function's own doc comment for
/// why - the original relies on a NUL terminator Rust slices don't
/// have).
///
/// # Safety
/// Touches `OPTION_VARS` (via [`utf_composinglike`]) - same
/// requirement as every other function that does so.
#[must_use]
pub unsafe fn utfc_ptr2len(p: &[u8]) -> i32 {
    let Some(&b0) = p.first() else {
        return 0;
    };
    if b0 == 0 {
        return 0;
    }
    if b0 < 0x80 && *p.get(1).unwrap_or(&0) < 0x80 {
        // be quick for ASCII
        return 1;
    }

    // Skip over first UTF-8 char, stopping at a NUL byte.
    let mut len = utf_ptr2len(p);

    // Check for illegal byte.
    if len == 1 && b0 >= 0x80 {
        return 1;
    }

    // Check for composing characters.
    let mut prevlen = 0usize;
    let mut state = crate::mbyte_defs::GRAPHEME_STATE_INIT;
    loop {
        let len_u = len as usize;
        if p.get(len_u).is_none_or(|&b| b < 0x80) {
            return len;
        }
        // SAFETY: forwarded from this function's own safety doc; both
        // slices are non-empty (len_u < p.len(), just checked above,
        // and prevlen < len_u by construction).
        if !unsafe { utf_composinglike(&p[prevlen..], &p[len_u..], &mut state) } {
            return len;
        }

        // Skip over composing char.
        prevlen = len_u;
        len += utf_ptr2len(&p[len_u..]);
    }
}

/// Return the number of bytes the UTF-8 encoding of the character at
/// `p[size]` takes. This includes following composing characters.
/// Returns 0 for an empty slice. Returns 1 for an illegal char or an
/// incomplete byte sequence (`utfc_ptr2len_len`).
///
/// Callers are responsible for `size <= p.len()`, same contract as
/// [`utf_ptr2len_len`].
///
/// # Safety
/// Same as [`utfc_ptr2len`].
#[must_use]
pub unsafe fn utfc_ptr2len_len(p: &[u8], size: usize) -> i32 {
    if size < 1 || p[0] == 0 {
        return 0;
    }
    if p[0] < 0x80 && (size == 1 || p[1] < 0x80) {
        // be quick for ASCII
        return 1;
    }

    // Skip over first UTF-8 char, stopping at a NUL byte.
    let mut len = utf_ptr2len_len(p, size);

    // Check for illegal byte and incomplete byte sequence.
    if (len == 1 && p[0] >= 0x80) || len as usize > size {
        return 1;
    }

    // Check for composing characters. We can only display a limited
    // amount, but skip all of them (otherwise the cursor would get
    // stuck).
    let mut prevlen = 0usize;
    let mut state = crate::mbyte_defs::GRAPHEME_STATE_INIT;
    while (len as usize) < size {
        let len_u = len as usize;
        if p[len_u] < 0x80 {
            break;
        }

        // Next character length should not go beyond size to ensure
        // that utf_composinglike(...) does not read beyond size.
        let len_next_char = utf_ptr2len_len(&p[len_u..], size - len_u);
        if len_next_char as usize > size - len_u {
            break;
        }

        // SAFETY: forwarded from this function's own safety doc; both
        // slices are non-empty (len_u < size <= p.len(), and
        // prevlen < len_u by construction).
        if !unsafe { utf_composinglike(&p[prevlen..], &p[len_u..], &mut state) } {
            break;
        }

        // Skip over composing char.
        prevlen = len_u;
        len += len_next_char;
    }
    len
}

/// Return the folded-case equivalent of `a`, which is a UCS-4
/// character. Uses full case folding (`utf_fold`).
#[must_use]
pub fn utf_fold(a: i32) -> i32 {
    if a < 0x80 {
        // be fast for ASCII
        return if (0x41..=0x5a).contains(&a) { a + 32 } else { a };
    }

    // utf8proc only does full case folding, which breaks some tests -
    // matches the original's own documented workaround exactly:
    // (0xdf) ß == ss in full casefolding, which breaks vim spell tests
    // relying on the vim spell files (E763); (0x130) İ == i̇ in full
    // casefolding.
    if a == 0xdf || a == 0x130 {
        return a;
    }

    let mut result = [0i32; 1];
    // SAFETY: result is a valid, correctly-sized (1-element) output
    // buffer; the last_boundclass out-param is null, which is valid
    // per utf8proc's own contract when UTF8PROC_CHARBOUND (not used
    // here) isn't set.
    let res = unsafe {
        utf8proc_sys::utf8proc_decompose_char(
            a,
            result.as_mut_ptr(),
            1,
            utf8proc_sys::utf8proc_option_t::UTF8PROC_CASEFOLD,
            std::ptr::null_mut(),
        )
    };
    if res == 1 {
        result[0]
    } else {
        a
    }
}

/// Return the upper-case equivalent of `a`, which is a UCS-4
/// character. Use simple case folding (`mb_toupper`).
///
/// See this module's own doc comment for the narrow, documented
/// `'casemap'`-without-`"internal"` gap.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` - same requirement as
/// every other function that does so.
#[must_use]
pub unsafe fn mb_toupper(a: i32) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let cmp_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cmp_flags;

    // If 'casemap' contains "keepascii" use ASCII style toupper().
    if a < 128 && cmp_flags & crate::option_vars::opt_cmp_flag::KEEPASCII != 0 {
        return crate::macros_defs::toupper_asc(a);
    }

    // (`cmp_flags & opt_cmp_flag::INTERNAL == 0` - the towupper()
    // branch - fall through to the same handling as "internal" below;
    // see this module's own doc comment for why.)

    // For characters below 128 use locale sensitive toupper().
    if a < 128 {
        return crate::macros_defs::toupper_loc(a);
    }

    // SAFETY: utf8proc_toupper is a pure function with no
    // preconditions (returns `c` unchanged for invalid/no-uppercase
    // codepoints, per its own doc).
    unsafe { utf8proc_sys::utf8proc_toupper(a) }
}

/// `mb_islower`.
///
/// # Safety
/// Same as [`mb_toupper`].
#[must_use]
pub unsafe fn mb_islower(a: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { mb_toupper(a) != a }
}

/// Return the lower-case equivalent of `a`, which is a UCS-4
/// character. Use simple case folding (`mb_tolower`).
///
/// See this module's own doc comment for the narrow, documented
/// `'casemap'`-without-`"internal"` gap.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` - same requirement as
/// every other function that does so.
#[must_use]
pub unsafe fn mb_tolower(a: i32) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let cmp_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cmp_flags;

    // If 'casemap' contains "keepascii" use ASCII style tolower().
    if a < 128 && cmp_flags & crate::option_vars::opt_cmp_flag::KEEPASCII != 0 {
        return crate::macros_defs::tolower_asc(a);
    }

    // For characters below 128 use locale sensitive tolower().
    if a < 128 {
        return crate::macros_defs::tolower_loc(a);
    }

    // SAFETY: utf8proc_tolower is a pure function with no
    // preconditions (returns `c` unchanged for invalid/no-lowercase
    // codepoints, per its own doc).
    unsafe { utf8proc_sys::utf8proc_tolower(a) }
}

/// `mb_isupper`.
///
/// # Safety
/// Same as [`mb_tolower`].
#[must_use]
pub unsafe fn mb_isupper(a: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { mb_tolower(a) != a }
}

/// Read a single (possibly multi-byte) character from `s`, never
/// reading past `s`'s own bounds (`utf_safe_read_char_adv`, `static` in
/// the original - kept private here too).
///
/// Returns `(codepoint, consumed)`:
/// - `(0, 0)` if `s` is empty (end of buffer).
/// - `(-1, 0)` if the byte sequence is illegal or incomplete (does not
///   advance).
/// - `(c, k)` otherwise: the decoded codepoint and the number of bytes
///   it occupies.
///
/// The original also treats a real embedded NUL byte as "end of
/// string" (returns 0, advances by 1) because it scans a NUL-terminated
/// C string bounded additionally by a caller-supplied length. This
/// translation takes an explicit, already-bounded byte slice instead,
/// so an embedded NUL is just an ordinary ASCII byte (length 1, value
/// 0) like [`utf_ptr2char`] treats it elsewhere in this module; no
/// caller here relies on embedded-NUL-as-terminator semantics.
fn utf_safe_read_char_adv(s: &[u8]) -> (i32, usize) {
    let Some(&b0) = s.first() else {
        return (0, 0); // end of buffer
    };

    let k = usize::from(UTF8LEN_TAB_ZERO[b0 as usize]);

    if k == 1 {
        // ASCII character (or NUL, see doc comment above).
        return (i32::from(b0), 1);
    }

    if k <= s.len() {
        // We have a multibyte sequence and it isn't truncated by the
        // slice's own bounds, so utf_ptr2char() is safe to use. Or the
        // first byte is illegal (k == 0), and it's also safe to use
        // utf_ptr2char() (0 <= s.len() always holds, and s is
        // non-empty here).
        let c = utf_ptr2char(s);

        // On failure, utf_ptr2char() returns the first byte, so check
        // equality with the first byte. The only non-ASCII character
        // which equals the first byte of its own UTF-8 representation
        // is U+00C3 (UTF-8: 0xC3 0x83), so that special case is also
        // checked. Safe even if s.len() == 1: k > 1 here always means
        // s.len() >= k >= 2 (k == 0 never reaches the 0xC3 check since
        // c would then equal b0 exactly, failing the first half of the
        // condition).
        if c != i32::from(b0) || (c == 0xC3 && s.get(1) == Some(&0x83)) {
            // byte sequence was successfully decoded
            return (c, k);
        }
    }

    // byte sequence is incomplete or illegal
    (-1, 0)
}

/// Version of `strnicmp()` that handles multi-byte characters. Needed
/// for Big5, Shift-JIS and UTF-8 encoding (`utf_strnicmp`).
///
/// Compares at most `n1` bytes of `s1` and `n2` bytes of `s2`.
///
/// @return zero if `s1` and `s2` are equal (ignoring case), the
/// difference between two characters otherwise.
#[must_use]
pub fn utf_strnicmp(s1: &[u8], s2: &[u8], n1: usize, n2: usize) -> i32 {
    let mut p1 = &s1[..n1.min(s1.len())];
    let mut p2 = &s2[..n2.min(s2.len())];
    let mut c1;
    let mut c2;

    loop {
        let (v1, k1) = utf_safe_read_char_adv(p1);
        let (v2, k2) = utf_safe_read_char_adv(p2);
        c1 = v1;
        c2 = v2;
        p1 = &p1[k1..];
        p2 = &p2[k2..];

        if c1 <= 0 || c2 <= 0 {
            break;
        }

        if c1 == c2 {
            continue;
        }

        let cdiff = utf_fold(c1) - utf_fold(c2);
        if cdiff != 0 {
            return cdiff;
        }
    }

    // some string ended or has an incomplete/illegal character sequence

    if c1 == 0 || c2 == 0 {
        // some string ended. shorter string is smaller
        if c1 == 0 && c2 == 0 {
            return 0;
        }
        return if c1 == 0 { -1 } else { 1 };
    }

    // Continue with bytewise comparison to produce some result that
    // would make comparison operations involving this function
    // transitive.
    //
    // If only one string had an error, comparison should be made with
    // the folded version of the other string. In this case it is
    // enough to fold just one character to determine the result of
    // comparison.
    let mut buffer1 = [0u8; 6];
    let mut buffer2 = [0u8; 6];
    if c1 != -1 && c2 == -1 {
        let len = utf_char2bytes(utf_fold(c1), &mut buffer1) as usize;
        p1 = &buffer1[..len];
    } else if c2 != -1 && c1 == -1 {
        let len = utf_char2bytes(utf_fold(c2), &mut buffer2) as usize;
        p2 = &buffer2[..len];
    }

    while !p1.is_empty() && !p2.is_empty() && p1[0] != 0 && p2[0] != 0 {
        let cdiff = i32::from(p1[0]) - i32::from(p2[0]);
        if cdiff != 0 {
            return cdiff;
        }
        p1 = &p1[1..];
        p2 = &p2[1..];
    }

    // Treat "ran out of bytes" and "hit an embedded NUL" as the same
    // ending condition for the final determination.
    let n1_done = p1.is_empty() || p1[0] == 0;
    let n2_done = p2.is_empty() || p2[0] == 0;

    if n1_done && n2_done {
        return 0;
    }
    if n1_done { -1 } else { 1 }
}

/// Compare strings case-insensitively, handling multi-byte characters
/// (`mb_strnicmp`). Compares at most `nn` bytes of each string.
///
/// @return zero if `s1` and `s2` are equal (ignoring case), the
/// difference between two characters otherwise.
#[must_use]
pub fn mb_strnicmp(s1: &[u8], s2: &[u8], nn: usize) -> i32 {
    utf_strnicmp(s1, s2, nn, nn)
}

/// Compare strings case-insensitively, handling multi-byte characters
/// (`mb_stricmp`).
///
/// We need to call this even when we aren't dealing with a multi-byte
/// encoding because it takes care of all ASCII and non-ASCII encodings
/// (including characters with umlauts in latin1, etc.), while a plain
/// byte-wise case-insensitive compare only handles the system locale
/// version, which often does not handle non-ASCII properly.
///
/// @return 0 if strings are equal, <0 if `s1` < `s2`, >0 if `s1` >
/// `s2`.
#[must_use]
pub fn mb_stricmp(s1: &[u8], s2: &[u8]) -> i32 {
    mb_strnicmp(s1, s2, crate::pos_defs::MAXCOL as usize)
}

/// Return true if `c` (`>= 0x100`) is in `table`, a sorted list of
/// non-overlapping `(first, last)` inclusive intervals (`intable`,
/// `static` in the original - kept private here too).
fn intable(table: &[(i32, i32)], c: i32) -> bool {
    // first quick check for Latin1 etc. characters
    if c < table[0].0 {
        return false;
    }

    // binary search in table
    let mut bot = 0usize;
    let mut top = table.len();
    loop {
        let mid = (bot + top) / 2;
        if table[mid].1 < c {
            bot = mid + 1;
        } else if table[mid].0 > c {
            top = mid;
        } else {
            return true;
        }
        if top <= bot {
            return false;
        }
    }
}

/// Return true for characters that can be displayed in a normal way.
/// Only for characters of 0x100 and above! (`utf_printable`).
///
/// Translated from the portable (non-`__SSE2__`) reference
/// implementation in the original - the `__SSE2__` intrinsics fast
/// path is a pure performance optimization producing bit-for-bit
/// identical results (same fixed interval table), not translated
/// (this crate doesn't use platform SIMD intrinsics anywhere else
/// either).
#[must_use]
pub fn utf_printable(c: i32) -> bool {
    // Sorted list of non-overlapping intervals.
    // 0xd800-0xdfff is reserved for UTF-16, actually illegal.
    const NONPRINT: &[(i32, i32)] = &[
        (0x070f, 0x070f),
        (0x180b, 0x180e),
        (0x200b, 0x200f),
        (0x202a, 0x202e),
        (0x2060, 0x206f),
        (0xd800, 0xdfff),
        (0xfeff, 0xfeff),
        (0xfff9, 0xfffb),
        (0xfffe, 0xffff),
    ];
    !intable(NONPRINT, c)
}

/// Check if `c` has a user-configured cell width via `'cellwidths'`
/// (`cw_value`, `static` in the original - kept private here too).
///
/// Always returns 0 (no override): the original's `cw_table` is
/// populated only by the eval engine's `setcellwidths()` VimL builtin
/// (`f_setcellwidths`, `eval/funcs.c`, not yet translated) - this
/// matches every real session's DEFAULT (nobody has called
/// `setcellwidths()`) state exactly, not an approximation.
fn cw_value(_c: i32) -> i32 {
    0
}

/// `prop_is_emojilike` (`static` in the original - kept private here
/// too).
fn prop_is_emojilike(prop: &utf8proc_sys::utf8proc_property_t) -> bool {
    prop.boundclass() == utf8proc_sys::utf8proc_boundclass_t::UTF8PROC_BOUNDCLASS_EXTENDED_PICTOGRAPHIC.0
        || prop.boundclass() == utf8proc_sys::utf8proc_boundclass_t::UTF8PROC_BOUNDCLASS_REGIONAL_INDICATOR.0
}

/// For UTF-8 character `c` return 2 for a double-width character, 1
/// for others. Returns 4 or 6 for an unprintable character. Is only
/// correct for characters >= 0x80. When `'ambiwidth'` is `"double"`,
/// return 2 for a character with East Asian Width class
/// A(mbiguous) (`utf_char2cells`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (for `'ambiwidth'`/
/// `'emoji'`).
#[must_use]
pub unsafe fn utf_char2cells(c: i32) -> i32 {
    if c < 0x80 {
        return 1;
    }

    if !crate::charset::vim_isprintc(c) {
        // unprintable is displayed either as <xx> or <xxxx>
        return if c > 0xFF { 6 } else { 4 };
    }

    let n = cw_value(c);
    if n != 0 {
        return n;
    }

    // SAFETY: utf8proc_get_property never returns null (documented
    // utf8proc contract - it always returns a valid "default entry"
    // even for out-of-range/invalid codepoints).
    let prop = unsafe { &*utf8proc_sys::utf8proc_get_property(c) };

    if prop.charwidth() == 2 {
        return 2;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    if opts.p_ambw.as_deref().is_some_and(|s| s.first() == Some(&b'd')) && prop.ambiguous_width() != 0
    {
        return 2;
    }

    // Characters below 1F000 may be considered single width
    // traditionally, making them double width causes problems.
    if opts.p_emoji != 0 && c >= 0x1f000 && prop.ambiguous_width() == 0 && prop_is_emojilike(prop) {
        return 2;
    }

    1
}

/// Return the number of display cells the character at `p` occupies.
/// This doesn't take care of unprintable characters, use
/// [`crate::charset::ptr2cells`] for that (`utf_ptr2cells`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via [`utf_char2cells`]
/// and, on the ASCII-overlong-sequence path,
/// [`crate::charset::char2cells`]).
#[must_use]
pub unsafe fn utf_ptr2cells(p: &[u8]) -> i32 {
    let Some(&b0) = p.first() else {
        return 1;
    };
    if b0 < 0x80 {
        return 1;
    }

    let len = utf_ptr2len(p) as usize;
    let c = utf_ptr2char(p);

    // An illegal byte, or overlong-encoded NUL, is displayed as <xx>.
    // (Equivalent to the original's utf_ptr2CharInfo_impl(...) <= 0
    // check: that helper always yields a value <= 0 exactly when
    // utf_ptr2len collapses to 1 (illegal/truncated) or the decoded
    // codepoint is 0 - not translated separately since utf_ptr2len/
    // utf_ptr2char already exist and utf_ptr2cells_len itself uses
    // this same equivalent formulation in the original.)
    if len == 1 || c == 0 {
        return 4;
    }

    // If the char is ASCII it must be an overlong sequence.
    if c < 0x80 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::charset::char2cells(c) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let cells = unsafe { utf_char2cells(c) };
    // SAFETY: forwarded from this function's own safety doc.
    let p_emoji = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_emoji;
    if cells == 1 && p_emoji != 0 {
        // SAFETY: utf8proc_get_property never returns null.
        let prop = unsafe { &*utf8proc_sys::utf8proc_get_property(c) };
        if prop_is_emojilike(prop) {
            let c2 = if len < p.len() { utf_ptr2char(&p[len..]) } else { 0 };
            if c2 == 0xFE0F {
                return 2; // emoji presentation
            }
        }
    }
    cells
}

/// Convert a UTF-8 byte sequence of the given claimed `len` (as
/// returned by [`UTF8LEN_TAB`], 1-6) to a signed code point, returning
/// a negative value if the sequence is illegal (`utf_ptr2CharInfo_impl`,
/// `static` in the original - kept private here too).
///
/// Unlike [`utf_ptr2char`] (which degrades gracefully to "return the
/// raw byte value" for anything invalid, useful for display), this
/// distinguishes "definitely invalid" (negative) from every real,
/// valid decoded codepoint (including 0, for an overlong-encoded
/// NUL) - needed by callers (like [`utf_head_off`]) that look up
/// Unicode properties keyed on the actual codepoint value, not just
/// "is this displayable".
///
/// Does not handle ASCII: only ever called with `len >= 1` from
/// [`UTF8LEN_TAB`] on an already-confirmed non-ASCII lead byte.
///
/// # Slice-bounds deviation from the original
/// The original always reads byte `p[1]` unconditionally, even for
/// `len == 1` (an illegal lead byte) - safe there only because `p`
/// points into a NUL-terminated buffer with always at least one more
/// readable byte. Since `len == 1` yields a negative result either
/// way in the original (whether or not that unconditional read
/// "succeeds" its own continuation-byte check, the fixed correction
/// term `1 << 31` keeps the top bit set), this translation
/// short-circuits to `-1` for `len < 2` without performing that read
/// at all - identical observable result, and avoids a potential
/// out-of-bounds slice access. For `len >= 2`, "ran out of slice" is
/// treated the same as "byte failed its continuation-byte check",
/// matching [`utf_ptr2len`]'s own established precedent (see its own
/// doc comment for why).
///
/// Uses `wrapping_*` arithmetic throughout for the same reason
/// [`utf_ptr2char`] does (see its own doc comment) - the original's C
/// `uint32_t` math wraps silently on overflow by design, which a
/// plain `<<`/`+`/`-` on Rust's `u32` does not reproduce (panics in
/// debug builds instead).
fn utf_ptr2char_info_impl(p: &[u8], len: usize) -> i32 {
    if len < 2 {
        // See this function's own doc comment: always negative here,
        // via either of the original's own two code paths.
        return -1;
    }

    let is_continuation = |b: u8| (b & 0xC0) == 0x80;
    let v0 = u32::from(p[0]);

    let Some(&b1) = p.get(1) else { return -1 };
    if !is_continuation(b1) {
        return -1;
    }
    let mut code_point = v0.wrapping_shl(6).wrapping_add(u32::from(b1));
    if len == 2 {
        return code_point.wrapping_sub(0x80 + (0xC0 << 6)) as i32;
    }

    let Some(&b2) = p.get(2) else { return -1 };
    if !is_continuation(b2) {
        return -1;
    }
    code_point = code_point.wrapping_shl(6).wrapping_add(u32::from(b2));
    if len == 3 {
        return code_point.wrapping_sub(0x80 + (0x80 << 6) + (0xE0 << 12)) as i32;
    }

    let Some(&b3) = p.get(3) else { return -1 };
    if !is_continuation(b3) {
        return -1;
    }
    code_point = code_point.wrapping_shl(6).wrapping_add(u32::from(b3));
    if len == 4 {
        return code_point.wrapping_sub(0x80 + (0x80 << 6) + (0x80 << 12) + (0xF0 << 18)) as i32;
    }

    let Some(&b4) = p.get(4) else { return -1 };
    if !is_continuation(b4) {
        return -1;
    }
    code_point = code_point.wrapping_shl(6).wrapping_add(u32::from(b4));
    if len == 5 {
        return code_point
            .wrapping_sub(0x80 + (0x80 << 6) + (0x80 << 12) + (0x80 << 18) + (0xF8 << 24))
            as i32;
    }

    let Some(&b5) = p.get(5) else { return -1 };
    if !is_continuation(b5) {
        return -1;
    }
    code_point = code_point.wrapping_shl(6).wrapping_add(u32::from(b5));
    // len == 6 (no `0xFC << 30` term: it evaluates to 0 after 32-bit
    // truncation, matching the original's own commented-out term -
    // verified: 0xFC's lowest 2 bits are 0, and only those 2 bits
    // survive a `<< 30` truncated to 32 bits).
    code_point.wrapping_sub(0x80 + (0x80 << 6) + (0x80 << 12) + (0x80 << 18) + (0x80 << 24)) as i32
}

/// Return information (decoded codepoint + byte length) about the
/// character at `p` (`utf_ptr2CharInfo`, an inline function in the
/// original's own header).
///
/// @return information about the character. When the sequence is
/// illegal, [`crate::mbyte_defs::CharInfo`]'s `value` is negative and
/// `len` is 1.
///
/// # Panics
/// If `p` is empty (matching the original's `FUNC_ATTR_NONNULL_ALL` -
/// an empty slice has no analogous "first byte" to inspect).
#[must_use]
pub fn utf_ptr2char_info(p: &[u8]) -> crate::mbyte_defs::CharInfo {
    let first = p[0];
    if first < 0x80 {
        return crate::mbyte_defs::CharInfo { value: i32::from(first), len: 1 };
    }
    let mut len = usize::from(UTF8LEN_TAB[first as usize]);
    let code_point = utf_ptr2char_info_impl(p, len);
    if code_point < 0 {
        len = 1;
    }
    crate::mbyte_defs::CharInfo { value: code_point, len }
}

/// Return information about the first character of `line` as a
/// [`crate::mbyte_defs::StrCharInfo`] positioned at offset 0
/// (`utf_ptr2StrCharInfo`, an inline function in the original's own
/// header).
///
/// The original can be called with `ptr` pointing anywhere inside a
/// buffer; callers here that need to start partway through a larger
/// buffer pass a sub-slice (`&line[start..]`) instead, and account for
/// `start` themselves when interpreting the resulting `pos` - see
/// [`crate::mbyte_defs::StrCharInfo`]'s own doc comment for why.
///
/// # Panics
/// If `line` is empty (matching the original's `FUNC_ATTR_NONNULL_ALL`).
#[must_use]
pub fn utf_ptr2str_char_info(line: &[u8]) -> crate::mbyte_defs::StrCharInfo {
    crate::mbyte_defs::StrCharInfo { pos: 0, chr: utf_ptr2char_info(line) }
}

/// Return information about the next character after `cur`, given the
/// same `line` buffer `cur.pos` is an offset into. Composing and
/// combining characters are considered part of the current character
/// (`utfc_next`).
///
/// Like [`utfc_ptr2len`], running out of `line` partway through
/// (rather than hitting `line`'s own trailing NUL, per this crate's
/// line-storage convention) is treated the same as hitting a byte that
/// ends the sequence, rather than panicking - the original relies on
/// always eventually finding a real NUL terminator.
///
/// # Safety
/// Touches global grapheme-break state via [`utf_iscomposing`]
/// (forwarded from that function's own safety doc) - though the
/// common ASCII-fast-path below never actually reaches it.
#[must_use]
pub unsafe fn utfc_next(
    line: &[u8],
    cur: crate::mbyte_defs::StrCharInfo,
) -> crate::mbyte_defs::StrCharInfo {
    let next_pos = cur.pos + cur.chr.len;
    let next_byte = line.get(next_pos).copied().unwrap_or(0);
    if next_byte < 0x80 {
        return crate::mbyte_defs::StrCharInfo {
            pos: next_pos,
            chr: crate::mbyte_defs::CharInfo { value: i32::from(next_byte), len: 1 },
        };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { utfc_next_impl(line, cur) }
}

/// The non-ASCII-fast-path core of [`utfc_next`] (`utfc_next_impl`).
/// Assumes the caller already handled the ASCII case.
///
/// `next_pos` is always in-bounds and `>= 0x80` on every entry into
/// the loop below - guaranteed by construction, not just assumed: the
/// only caller ([`utfc_next`]) only reaches this function when its own
/// bounds-checked read of `line[next_pos]` was `Some(byte >= 0x80)`,
/// and the loop's own bottom re-establishes the same guarantee for
/// each subsequent iteration before looping back (mirroring the
/// original's own reliance on always eventually finding a real NUL
/// terminator in a well-formed buffer).
///
/// # Safety
/// Same as [`utfc_next`].
unsafe fn utfc_next_impl(
    line: &[u8],
    cur: crate::mbyte_defs::StrCharInfo,
) -> crate::mbyte_defs::StrCharInfo {
    let mut prev_code = cur.chr.value;
    let mut next_pos = cur.pos + cur.chr.len;
    debug_assert!(line[next_pos] >= 0x80);
    let mut state = crate::mbyte_defs::GRAPHEME_STATE_INIT;

    loop {
        let next_len = usize::from(UTF8LEN_TAB[usize::from(line[next_pos])]);
        let next_code = utf_ptr2char_info_impl(&line[next_pos..], next_len);
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { utf_iscomposing(prev_code, next_code, &mut state) } {
            return crate::mbyte_defs::StrCharInfo {
                pos: next_pos,
                chr: crate::mbyte_defs::CharInfo {
                    value: next_code,
                    len: if next_code < 0 { 1 } else { next_len },
                },
            };
        }

        prev_code = next_code;
        next_pos += next_len;
        let next_byte = line.get(next_pos).copied().unwrap_or(0);
        if next_byte < 0x80 {
            return crate::mbyte_defs::StrCharInfo {
                pos: next_pos,
                chr: crate::mbyte_defs::CharInfo { value: i32::from(next_byte), len: 1 },
            };
        }
    }
}

/// `true` if boundclass `bc` always starts a new cluster regardless of
/// what's before. False negatives are allowed (perf cost, not
/// correctness) (`always_break`, `static` in the original - kept
/// private here too).
fn always_break(bc: u32) -> bool {
    bc == utf8proc_sys::utf8proc_boundclass_t::UTF8PROC_BOUNDCLASS_CONTROL.0
}

/// `true` if `bc2` always starts a cluster after `bc1`. False
/// negatives are allowed (perf cost, not correctness)
/// (`always_break_two`, `static` in the original - kept private here
/// too).
fn always_break_two(bc1: u32, bc2: u32) -> bool {
    use utf8proc_sys::utf8proc_boundclass_t as B;
    // don't check for UTF8PROC_BOUNDCLASS_CONTROL for bc2 as it either
    // has been checked by "always_break" on first iteration or when it
    // was bc1 in the previous iteration
    (bc1 != B::UTF8PROC_BOUNDCLASS_PREPEND.0 && bc2 == B::UTF8PROC_BOUNDCLASS_OTHER.0)
        || (B::UTF8PROC_BOUNDCLASS_CR.0..=B::UTF8PROC_BOUNDCLASS_CONTROL.0).contains(&bc1)
        || (bc2 == B::UTF8PROC_BOUNDCLASS_EXTENDED_PICTOGRAPHIC.0
            && (bc1 == B::UTF8PROC_BOUNDCLASS_OTHER.0
                || bc1 == B::UTF8PROC_BOUNDCLASS_EXTENDED_PICTOGRAPHIC.0))
}

/// Return the offset from `base[p_idx]` back to the start of its
/// character, including any composing characters that form the same
/// grapheme cluster. `base` must be the start of the string (i.e.
/// `p_idx` indexes into it), which must include a trailing NUL byte
/// like every other line buffer in this crate (see [`utf_ptr2len`]'s
/// own doc comment for why a NUL terminator matters here, and this
/// function's own "slice-bounds" note below for the one place it
/// matters most). Returns 0 if `base[p_idx]` is the NUL at the end of
/// the string, and 0 when already at the first byte of a character
/// (`utf_head_off`).
///
/// This is a genuine bidirectional (backward-then-forward)
/// grapheme-cluster-boundary scan: unlike every other function in this
/// module, it reads *before* `p_idx` down to the start of the buffer
/// to find where the enclosing cluster actually begins, then scans
/// forward again to re-locate `p_idx` within it.
///
/// # Slice-bounds note
/// The original relies on `base` being NUL-terminated to safely probe
/// a handful of bytes ahead of `p_idx` (`safe_end = start + last_len`,
/// where `last_len` can be up to 6). This translation does not need to
/// clamp anything extra: `utf_ptr2char_info_impl` (private) only ever
/// reports `cur_code >= 0` (i.e. only lets this function compute
/// `safe_end` at all) once it has *itself* successfully read all
/// `last_len` bytes starting at `start` - so `base.len() >= start +
/// last_len` is already guaranteed transitively by that check, exactly
/// mirroring how the original's reliance on the NUL terminator "just
/// works" for any real, NUL-terminated line.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// [`utf_composinglike`]/[`utfc_ptr2len_len`] and `arabic_combine`) -
/// same requirement as every other function that does so.
#[must_use]
pub unsafe fn utf_head_off(base: &[u8], p_idx: usize) -> i32 {
    if base[p_idx] < 0x80 {
        // be quick for ASCII
        return 0;
    }

    let mut start = p_idx;

    // move start to the first byte of this codepoint - might stop on
    // a continuation byte if overlong, handled by
    // utf_ptr2char_info_impl.
    while start > 0 && (base[start] & 0xc0) == 0x80 && (p_idx - start) < 6 {
        start -= 1;
    }

    let last_len = usize::from(UTF8LEN_TAB[base[start] as usize]);
    let cur_code = utf_ptr2char_info_impl(&base[start..], last_len);
    if cur_code < 0 || p_idx - start >= last_len {
        return 0; // p must be part of an illegal sequence
    }
    let safe_end = start + last_len;

    // SAFETY: utf8proc_get_property never returns null (documented
    // utf8proc contract).
    let mut cur_bc = unsafe { &*utf8proc_sys::utf8proc_get_property(cur_code) }.boundclass();
    if always_break(cur_bc) || start == 0 {
        return (p_idx - start) as i32;
    }

    // backtrack to find the start of a cluster; we might go too far,
    // checked in the next loop.
    let mut cur_pos = start;
    let p_start = start;
    let mut cur_code = cur_code;

    loop {
        // Invariant: `start > 0` always holds on entry (established
        // before the loop by the `start == 0` return above, and
        // re-established each iteration by the `else if start == 0
        // { break; }` below), so `base[start - 1]` never underflows.
        if base[start - 1] == 0 {
            break;
        }

        start -= 1;
        if base[start] < 0x80 {
            // stop on ascii, we are done
            break;
        }

        while start > 0 && (base[start] & 0xc0) == 0x80 && (cur_pos - start) < 6 {
            start -= 1;
        }

        let prev_len = usize::from(UTF8LEN_TAB[base[start] as usize]);
        let prev_code = utf_ptr2char_info_impl(&base[start..], prev_len);
        if prev_code < 0 || prev_len < cur_pos - start {
            start = cur_pos; // start at valid sequence after invalid bytes
            break;
        }

        // SAFETY: utf8proc_get_property never returns null.
        let prev_bc = unsafe { &*utf8proc_sys::utf8proc_get_property(prev_code) }.boundclass();
        // SAFETY: forwarded from this function's own safety doc.
        if always_break_two(prev_bc, cur_bc)
            && !unsafe { crate::arabic::arabic_combine(prev_code, cur_code) }
        {
            start = cur_pos; // prev_code cannot be a part of this cluster
            break;
        } else if start == 0 {
            break;
        }
        cur_pos = start;
        cur_bc = prev_bc;
        cur_code = prev_code;
    }

    // hot path: we are already on the first codepoint of a sequence
    if start == p_start && last_len > p_idx - start {
        return (p_idx - start) as i32;
    }

    let mut q = start;
    while q < p_idx {
        // don't need to find end of cluster - once we reached the
        // codepoint of p, we are done.
        // SAFETY: forwarded from this function's own safety doc.
        let len = usize::try_from(unsafe { utfc_ptr2len_len(&base[q..], safe_end - q) })
            .expect("utfc_ptr2len_len returns a non-negative length");

        if q + len > p_idx {
            return (p_idx - q) as i32;
        }

        q += len;
    }

    0
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
    fn utf_ptr2char_decodes_maximal_6_byte_sequence_without_overflow_panic() {
        // Regression test: a genuine 6-byte lead byte (0xFC/0xFD) with
        // maximal continuation bytes (0xBF each) overflows the u32
        // accumulation used to reassemble the codepoint - caught via a
        // standalone scratch reproduction (`(v0<<30)+...` panicked
        // with "attempt to add with overflow" in a debug build before
        // this function was switched to `wrapping_*` arithmetic,
        // matching the original C's well-defined unsigned wraparound).
        // Expected value cross-checked independently: a 6-byte
        // sequence carries 31 payload bits (1 from the lead byte + 5*6
        // from continuation bytes), so the all-ones payload is
        // `i32::MAX` (0x7FFFFFFF).
        let bytes = [0xFDu8, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF];
        assert_eq!(utf_ptr2char(&bytes), i32::MAX);
    }

    #[test]
    fn utf_ptr2char_info_ascii_is_length_one() {
        let info = utf_ptr2char_info(b"A");
        assert_eq!(info.value, i32::from(b'A'));
        assert_eq!(info.len, 1);
    }

    #[test]
    fn utf_ptr2char_info_decodes_multibyte_with_correct_length() {
        let info = utf_ptr2char_info("日".as_bytes());
        assert_eq!(info.value, 0x65E5);
        assert_eq!(info.len, 3);
    }

    #[test]
    fn utf_ptr2char_info_illegal_sequence_reports_negative_value_and_length_one() {
        // A lone continuation byte: illegal lead byte per UTF8LEN_TAB,
        // so utf_ptr2char_info_impl reports a negative value here too
        // (unlike utf_ptr2char, which degrades to the raw byte value).
        let info = utf_ptr2char_info(&[0x80]);
        assert!(info.value < 0);
        assert_eq!(info.len, 1);
    }

    #[test]
    fn utf_ptr2char_info_overlong_or_truncated_sequence_falls_back_to_length_one() {
        // 0xC2 is a valid 2-byte lead, but followed by an ASCII byte
        // (not a continuation byte) - illegal, so utf_ptr2char_info
        // forces len back to 1 (matching the original's `if
        // (code_point < 0) { len = 1; }`), even though UTF8LEN_TAB
        // itself would have claimed 2 bytes.
        let info = utf_ptr2char_info(&[0xC2, b'A']);
        assert!(info.value < 0);
        assert_eq!(info.len, 1);
    }

    #[test]
    fn utf_ptr2str_char_info_starts_at_offset_zero() {
        let ci = utf_ptr2str_char_info(b"abc");
        assert_eq!(ci.pos, 0);
        assert_eq!(ci.chr.value, i32::from(b'a'));
        assert_eq!(ci.chr.len, 1);
    }

    #[test]
    fn utfc_next_walks_plain_ascii_one_byte_at_a_time() {
        let line = b"abc";
        let ci0 = utf_ptr2str_char_info(line);
        let ci1 = unsafe { utfc_next(line, ci0) };
        assert_eq!(ci1.pos, 1);
        assert_eq!(ci1.chr.value, i32::from(b'b'));
        let ci2 = unsafe { utfc_next(line, ci1) };
        assert_eq!(ci2.pos, 2);
        assert_eq!(ci2.chr.value, i32::from(b'c'));
    }

    #[test]
    fn utfc_next_advances_past_a_multibyte_character() {
        // "日" is 3 bytes (0x65E5), followed by ASCII 'A'.
        let line = "日A".as_bytes();
        let ci0 = utf_ptr2str_char_info(line);
        assert_eq!(ci0.chr.value, 0x65E5);
        assert_eq!(ci0.chr.len, 3);
        let ci1 = unsafe { utfc_next(line, ci0) };
        assert_eq!(ci1.pos, 3);
        assert_eq!(ci1.chr.value, i32::from(b'A'));
    }

    #[test]
    fn utfc_next_skips_a_composing_combining_mark() {
        // "e" + COMBINING ACUTE ACCENT (U+0301, 2 bytes: 0xCC 0x81) + "f".
        // utfc_next treats the combining mark as part of the CURRENT
        // character, so advancing past "e" lands directly on "f" (byte
        // offset 3 = 1 ('e') + 2 (combining mark)), never stopping on
        // the combining mark itself. Verified against the real
        // utf8proc-backed behavior via a throwaway scratch probe before
        // writing this assertion.
        let line = "e\u{0301}f".as_bytes();
        let ci0 = utf_ptr2str_char_info(line);
        assert_eq!(ci0.pos, 0);
        assert_eq!(ci0.chr.value, i32::from(b'e'));
        let ci1 = unsafe { utfc_next(line, ci0) };
        assert_eq!(ci1.pos, 3);
        assert_eq!(ci1.chr.value, i32::from(b'f'));
    }

    #[test]
    fn utfc_next_past_the_end_of_a_nul_terminated_line_reads_the_nul() {
        // Matches this crate's line-storage convention (a "line" byte
        // slice includes its own trailing NUL) - advancing off the end
        // reads that NUL, exactly like the original's own reliance on a
        // real NUL terminator (not a special "out of bounds" case).
        let line = b"a\0"; // 1-byte line, NUL-terminated per convention
        let ci0 = utf_ptr2str_char_info(line);
        let ci1 = unsafe { utfc_next(line, ci0) };
        assert_eq!(ci1.pos, 1);
        assert_eq!(ci1.chr.value, 0);
        assert_eq!(ci1.chr.len, 1);
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

    /// Serializes tests that mutate `OPTION_VARS.cmp_flags`/`p_arshape`/
    /// `p_tbidi` (shared global state). Delegates to the crate-wide
    /// `crate::globals::global_state_test_lock` - see that function's
    /// own doc comment for why a single shared lock is used.
    fn option_vars_test_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::globals::global_state_test_lock()
    }

    #[test]
    fn utf_iscomposing_first_is_false_for_ascii() {
        assert!(!utf_iscomposing_first(b'A' as i32));
    }

    #[test]
    fn utf_iscomposing_first_is_true_for_a_combining_mark() {
        // U+0301 COMBINING ACUTE ACCENT is a well-known combining mark.
        assert!(utf_iscomposing_first(0x0301));
    }

    #[test]
    fn utfc_ptr2len_includes_a_following_combining_mark() {
        // "e" + U+0301 (combining acute accent) forms one grapheme
        // cluster ("é" as two codepoints instead of the single
        // precomposed U+00E9).
        let mut bytes = b"e".to_vec();
        bytes.extend_from_slice("\u{0301}".as_bytes());
        bytes.push(b'x'); // trailing ASCII, must NOT be included

        let len = unsafe { utfc_ptr2len(&bytes) };
        assert_eq!(len as usize, 1 + "\u{0301}".len());
    }

    #[test]
    fn utfc_ptr2len_is_just_the_base_character_without_composing_marks() {
        // Plain ASCII: no composing characters follow.
        assert_eq!(unsafe { utfc_ptr2len(b"ax") }, 1);
        // A precomposed character (no separate combining mark
        // following) is also just its own length.
        assert_eq!(unsafe { utfc_ptr2len("é".as_bytes()) }, 2);
    }

    #[test]
    fn utfc_ptr2len_returns_zero_for_empty_or_nul() {
        assert_eq!(unsafe { utfc_ptr2len(b"") }, 0);
        assert_eq!(unsafe { utfc_ptr2len(b"\0") }, 0);
    }

    #[test]
    fn utfc_ptr2len_len_matches_utfc_ptr2len_when_size_covers_everything() {
        let mut bytes = b"e".to_vec();
        bytes.extend_from_slice("\u{0301}".as_bytes());

        let full_len = unsafe { utfc_ptr2len(&bytes) };
        let bounded_len = unsafe { utfc_ptr2len_len(&bytes, bytes.len()) };
        assert_eq!(full_len, bounded_len);
    }

    #[test]
    fn utf_fold_lowercases_ascii_and_leaves_other_bytes_unchanged() {
        assert_eq!(utf_fold(i32::from(b'A')), i32::from(b'a'));
        assert_eq!(utf_fold(i32::from(b'z')), i32::from(b'z'));
        assert_eq!(utf_fold(i32::from(b'0')), i32::from(b'0'));
    }

    #[test]
    fn utf_fold_case_folds_a_non_ascii_letter() {
        // U+00C9 (É) case-folds to U+00E9 (é).
        assert_eq!(utf_fold(0xC9), 0xE9);
    }

    #[test]
    fn utf_fold_preserves_the_documented_special_case_exceptions() {
        // 0xdf (ß) and 0x130 (İ) are deliberately excluded from full
        // casefolding by the original (see utf_fold's own doc comment
        // for why) - both must come back unchanged.
        assert_eq!(utf_fold(0xdf), 0xdf);
        assert_eq!(utf_fold(0x130), 0x130);
    }

    #[test]
    fn mb_toupper_tolower_use_ascii_style_when_keepascii_is_set() {
        let _lock = option_vars_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.cmp_flags;
        opts.cmp_flags = crate::option_vars::opt_cmp_flag::INTERNAL
            | crate::option_vars::opt_cmp_flag::KEEPASCII;

        assert_eq!(unsafe { mb_toupper(i32::from(b'a')) }, i32::from(b'A'));
        assert_eq!(unsafe { mb_tolower(i32::from(b'A')) }, i32::from(b'a'));
        assert!(unsafe { mb_islower(i32::from(b'a')) });
        assert!(!unsafe { mb_islower(i32::from(b'A')) });
        assert!(unsafe { mb_isupper(i32::from(b'A')) });
        assert!(!unsafe { mb_isupper(i32::from(b'a')) });

        // Non-ASCII still goes through utf8proc regardless of
        // keepascii (which only affects characters < 128).
        assert_eq!(unsafe { mb_toupper(0xE9) }, 0xC9); // é -> É
        assert_eq!(unsafe { mb_tolower(0xC9) }, 0xE9); // É -> é

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cmp_flags = prev;
    }

    #[test]
    fn mb_toupper_tolower_use_locale_toupper_when_keepascii_is_unset() {
        let _lock = option_vars_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.cmp_flags;
        // "internal" only, without "keepascii" - falls to TOUPPER_LOC/
        // TOLOWER_LOC for ASCII, which (in the "C"/default locale this
        // test runs under) behaves the same as plain ASCII case
        // conversion.
        opts.cmp_flags = crate::option_vars::opt_cmp_flag::INTERNAL;

        assert_eq!(unsafe { mb_toupper(i32::from(b'a')) }, i32::from(b'A'));
        assert_eq!(unsafe { mb_tolower(i32::from(b'A')) }, i32::from(b'a'));

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cmp_flags = prev;
    }

    #[test]
    fn utf_safe_read_char_adv_handles_ascii_multibyte_truncation_and_illegal() {
        assert_eq!(utf_safe_read_char_adv(b""), (0, 0));
        assert_eq!(utf_safe_read_char_adv(b"A"), (i32::from(b'A'), 1));
        assert_eq!(utf_safe_read_char_adv("é".as_bytes()), (0xE9, 2));
        // Truncated 3-byte sequence (only the lead byte present).
        assert_eq!(utf_safe_read_char_adv(&"日".as_bytes()[..1]), (-1, 0));
        // Illegal lead byte (a lone continuation byte).
        assert_eq!(utf_safe_read_char_adv(&[0x80]), (-1, 0));
        // Embedded NUL is just an ordinary ASCII byte here (see doc
        // comment).
        assert_eq!(utf_safe_read_char_adv(&[0]), (0, 1));
    }

    #[test]
    fn utf_strnicmp_ascii_case_insensitive() {
        assert_eq!(utf_strnicmp(b"Hello", b"hello", 5, 5), 0);
        assert_eq!(utf_strnicmp(b"abc", b"abd", 3, 3), -1);
        assert_eq!(utf_strnicmp(b"abd", b"abc", 3, 3), 1);
    }

    #[test]
    fn utf_strnicmp_shorter_string_is_smaller() {
        assert!(utf_strnicmp(b"ab", b"abc", 2, 3) < 0);
        assert!(utf_strnicmp(b"abc", b"ab", 3, 2) > 0);
        assert_eq!(utf_strnicmp(b"abc", b"abc", 3, 3), 0);
    }

    #[test]
    fn utf_strnicmp_respects_length_bounds() {
        // Only the first 2 bytes of each are compared: "he" == "HE".
        assert_eq!(utf_strnicmp(b"hello", b"HELLO", 2, 2), 0);
        // Comparing "he" (len 2) against "hel" (len 3): shorter is
        // smaller.
        assert!(utf_strnicmp(b"hello", b"hello", 2, 3) < 0);
    }

    #[test]
    fn utf_strnicmp_multibyte_case_folding() {
        // U+00C9 (É) vs U+00E9 (é): equal under case folding.
        assert_eq!(utf_strnicmp("É".as_bytes(), "é".as_bytes(), 2, 2), 0);
        // Different codepoints entirely.
        assert_ne!(utf_strnicmp("日".as_bytes(), "本".as_bytes(), 3, 3), 0);
    }

    #[test]
    fn mb_strnicmp_matches_utf_strnicmp_with_same_bound() {
        assert_eq!(mb_strnicmp(b"FOO", b"foo", 3), 0);
        assert_eq!(mb_strnicmp(b"FOO", b"bar", 3), utf_strnicmp(b"FOO", b"bar", 3, 3));
    }

    #[test]
    fn mb_stricmp_is_case_insensitive_and_handles_non_ascii() {
        assert_eq!(mb_stricmp(b"FOO", b"foo"), 0);
        assert_ne!(mb_stricmp(b"FOO", b"bar"), 0);
        // É (U+00C9) vs é (U+00E9): equal under case folding, same as
        // utf_strnicmp/mb_strnicmp's own multi-byte handling.
        assert_eq!(mb_stricmp("É".as_bytes(), "é".as_bytes()), 0);
    }

    #[test]
    fn utf_printable_recognizes_the_nonprint_table_boundaries() {
        // Inside the fixed nonprint intervals: unprintable.
        assert!(!utf_printable(0x070f)); // single-value interval
        assert!(!utf_printable(0x200b)); // ZERO WIDTH SPACE (start of range)
        assert!(!utf_printable(0x200f)); // end of that range
        assert!(!utf_printable(0xffff)); // end of the last range
        // Outside every interval: printable.
        assert!(utf_printable(0x0100)); // Ā, ordinary Latin Extended-A
        assert!(utf_printable(0x4e00)); // 一, CJK
        assert!(utf_printable(0x2059)); // just before the 0x2060 range starts
    }

    #[test]
    fn utf_char2cells_ascii_is_always_one() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { utf_char2cells(i32::from(b'A')) }, 1);
    }

    #[test]
    fn utf_char2cells_wide_cjk_character_is_two() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { utf_char2cells(0x4e00) }, 2); // 一, East Asian Wide
    }

    #[test]
    fn utf_char2cells_ordinary_latin_is_one() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { utf_char2cells(0xe9) }, 1); // é
    }

    #[test]
    fn utf_char2cells_unprintable_nonprint_char_is_six_above_0xff() {
        let _guard = option_vars_test_lock();
        // U+200B is in utf_printable's nonprint table and > 0xFF.
        assert_eq!(unsafe { utf_char2cells(0x200b) }, 6);
    }

    #[test]
    fn utf_char2cells_ambiguous_width_follows_ambiwidth_option() {
        let _guard = option_vars_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_ambw.clone();

        // U+00A1 (INVERTED EXCLAMATION MARK) has East Asian Width
        // "Ambiguous" - single width unless 'ambiwidth' is "double".
        opts.p_ambw = Some(b"single".to_vec());
        assert_eq!(unsafe { utf_char2cells(0xa1) }, 1);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ambw = Some(b"double".to_vec());
        assert_eq!(unsafe { utf_char2cells(0xa1) }, 2);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ambw = prev;
    }

    #[test]
    fn utf_ptr2cells_ascii_is_one() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { utf_ptr2cells(b"A") }, 1);
    }

    #[test]
    fn utf_ptr2cells_empty_slice_is_one() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { utf_ptr2cells(b"") }, 1);
    }

    #[test]
    fn utf_ptr2cells_illegal_lead_byte_is_four() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { utf_ptr2cells(&[0x80]) }, 4); // lone continuation byte
    }

    #[test]
    fn utf_ptr2cells_matches_utf_char2cells_for_valid_multibyte() {
        let _guard = option_vars_test_lock();
        let cjk = "一".as_bytes(); // U+4E00
        assert_eq!(unsafe { utf_ptr2cells(cjk) }, unsafe { utf_char2cells(0x4e00) });
    }

    #[test]
    fn utf_head_off_is_zero_for_ascii_byte() {
        let _guard = option_vars_test_lock();
        let base = b"Ax\0";
        assert_eq!(unsafe { utf_head_off(base, 0) }, 0);
        assert_eq!(unsafe { utf_head_off(base, 1) }, 0);
    }

    #[test]
    fn utf_head_off_is_zero_at_the_trailing_nul() {
        let _guard = option_vars_test_lock();
        // The NUL terminator itself is < 0x80, so the "quick for
        // ASCII" fast path returns 0 for it too, matching the
        // original's own documented "if p points to the NUL at the
        // end of the string return 0" contract.
        let base = "日\0".as_bytes(); // [0xE6, 0x97, 0xA5, 0x00]
        assert_eq!(unsafe { utf_head_off(base, 3) }, 0);
    }

    #[test]
    fn utf_head_off_is_zero_when_already_at_the_first_byte_of_a_lone_char() {
        let _guard = option_vars_test_lock();
        // A single, standalone multi-byte character at the very start
        // of the buffer (start == base): already at the first byte.
        let base = "日\0".as_bytes();
        assert_eq!(unsafe { utf_head_off(base, 0) }, 0);
    }

    #[test]
    fn utf_head_off_returns_full_offset_for_continuation_bytes_of_a_lone_char() {
        let _guard = option_vars_test_lock();
        // "日" (U+65E5) = [0xE6, 0x97, 0xA5], a lone 3-byte character
        // with nothing before it - every continuation byte should
        // report its own distance back to the lead byte at index 0.
        let base = "日\0".as_bytes();
        assert_eq!(unsafe { utf_head_off(base, 1) }, 1);
        assert_eq!(unsafe { utf_head_off(base, 2) }, 2);
    }

    #[test]
    fn utf_head_off_walks_back_through_a_combining_mark_to_the_base_char() {
        let _guard = option_vars_test_lock();
        // 'e' (ASCII, 1 byte) + U+0301 COMBINING ACUTE ACCENT (2
        // bytes: 0xCC 0x81) + NUL. Verified this composes into one
        // grapheme cluster via a direct utf8proc_grapheme_break probe
        // before writing this test (returns false = "no break",
        // meaning the mark belongs with 'e'). Pointing at either byte
        // of the combining mark should walk back to the base 'e' at
        // index 0.
        let base = [0x65u8, 0xCC, 0x81, 0x00];
        assert_eq!(unsafe { utf_head_off(&base, 1) }, 1); // lead byte of the mark
        assert_eq!(unsafe { utf_head_off(&base, 2) }, 2); // 2nd byte of the mark
    }

    #[test]
    fn utf_head_off_does_not_merge_two_independent_cjk_characters() {
        let _guard = option_vars_test_lock();
        // "日本" = two standalone CJK ideographs, each its own
        // grapheme cluster (verified via a direct utf8proc probe
        // before writing this test: both are BOUNDCLASS_OTHER, and
        // OTHER-followed-by-OTHER always breaks per always_break_two,
        // and they don't arabic-combine either). Pointing into the
        // second character's continuation bytes must walk back only
        // to *its own* lead byte (index 3), never all the way back to
        // the first character (index 0) - this is the key case that
        // exercises the backtrack loop's cluster-boundary detection
        // rather than just reaching the very start of the buffer.
        let base = "日本\0".as_bytes(); // [E6,97,A5, E6,9C,AC, 00]
        assert_eq!(base.len(), 7);
        assert_eq!(unsafe { utf_head_off(base, 3) }, 0); // lead byte of 本
        assert_eq!(unsafe { utf_head_off(base, 4) }, 1); // 2nd byte of 本
        assert_eq!(unsafe { utf_head_off(base, 5) }, 2); // 3rd byte of 本
        // and the first character's own continuation bytes still walk
        // back only to index 0, not affected by what follows it.
        assert_eq!(unsafe { utf_head_off(base, 1) }, 1);
        assert_eq!(unsafe { utf_head_off(base, 2) }, 2);
    }

    #[test]
    fn utf_head_off_illegal_lone_continuation_byte_returns_zero() {
        let _guard = option_vars_test_lock();
        // A lone 0x80 continuation byte with nothing valid before it:
        // utf_ptr2char_info_impl can't decode a valid character
        // starting there (len 1, since UTF8LEN_TAB[0x80] == 1 - an
        // illegal lead byte), so cur_code < 0 and this returns 0
        // ("p must be part of an illegal sequence").
        let base = [0x80u8, 0x00];
        assert_eq!(unsafe { utf_head_off(&base, 0) }, 0);
    }
}
