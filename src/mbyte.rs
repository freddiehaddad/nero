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
//! the by-hand trace of the algorithm were correct. Also
//! [`mb_off_next`] - the offset from an arbitrary byte position to the
//! start of the NEXT character, a thin wrapper reusing
//! [`utf_head_off`]/[`utfc_ptr2len`] directly, needed by `drawline.c`/
//! `ex_getln.c` (neither translated yet).
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
//! caller); `iconv`-based conversion needs the still-undecided
//! `iconv` FFI (`iconv_defs.rs`). Each is its own follow-up, not
//! bundled in here. **Encoding-name canonicalization itself does NOT
//! need `iconv`** (re-verified directly against the real source
//! before this update's own addition, correcting an earlier,
//! over-broad note here that had bundled the two together) - see
//! `ENC_CANON_TABLE`/`enc_canon_search`/`enc_canon_props` below.
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
//!
//! Also translated: `mb_isalpha` (trivial `mb_islower(a) ||
//! mb_isupper(a)`); the line-break punctuation predicates
//! `utf_eat_space`/`utf_allow_break_before`/`utf_allow_break_after`/
//! `utf_allow_break` (pure, fixed-table lookups needing no `g_chartab`/
//! options at all - their own real callers, `ops.c`'s "J" join command
//! and `textformat.c`'s `gq` auto-formatting, aren't translated yet,
//! but these are small, self-contained, and have no design freedom to
//! get wrong, matching this crate's established "translate ahead of a
//! real caller" precedent; the original's own hand-rolled binary
//! search over 2 fixed, verified-sorted arrays is replaced with
//! `[T]::binary_search`, provably equivalent given the data's own real
//! sortedness); `utf_valid_string` (always checks the WHOLE given
//! slice - the original's own `end == NULL` "stop at the first NUL"
//! mode isn't modeled separately, since a NUL byte is itself a valid,
//! length-1 ASCII "character" per `UTF8LEN_TAB_ZERO`, so it never
//! terminates either scan early; a caller wanting NUL-terminated
//! behavior can simply slice `s` up to its own first NUL first);
//! `bomb_size`/`remove_bom` (BOM byte-count/stripping, needing only
//! already-real `BufT.b_p_bomb`/`b_p_bin`/`b_p_fenc` fields).
//!
//! Also translated: `ENC_CANON_TABLE`/`enc_canon_search`/
//! `enc_canon_props` (the canonical-encoding-name lookup table and its
//! 2 pure query functions - `enc_canonize`'s own further name-aliasing/
//! normalization logic, `enc_alias_table`, is NOT translated, since
//! nothing needs it yet; `codepage`, this table's own 3rd field in the
//! original, is likewise omitted - see `EncCanonEntry`'s own doc
//! comment for why). This directly unblocks `bufwrite.c`'s
//! `get_fio_flags`/`make_bom` as a follow-up, once `FIO_*`/
//! `ucs2bytes` also exist.
//!
//! Also translated: [`utf_class_tab`]/[`utf_class`] (Unicode character
//! classification via `UTF_CLASS_TABLE`, a 71-entry sorted-interval
//! table mechanically extracted from the real source, plus the
//! already-real `prop_is_emojilike`/`crate::charset::vim_iswordc_tab`
//! for its own Latin1/emoji fast paths - mutually recursive with
//! `vim_iswordc_tab`, see that function's own doc comment) and
//! [`mb_get_class_tab`]/[`mb_get_class`] (their own real callers,
//! placed here next to `utf_class_tab` rather than matching the
//! original's own much-earlier declaration order, for readability).
//! Also [`utf_ambiguous_width`] - needs only the already-real
//! [`utf_ptr2char_info`]/`prop_is_emojilike`.

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

/// Decode one multibyte character from an escaped key sequence
/// (`mb_unescape`).
///
/// `K_SPECIAL KS_SPECIAL KE_FILLER` is the escape used to represent a
/// literal `K_SPECIAL` byte inside a key string, so it is folded back
/// to a single `K_SPECIAL`. A bare `K_SPECIAL` starts a real special
/// key, which can never be part of a multibyte character, so the scan
/// stops there.
///
/// @return `Some((character_bytes, consumed))` when a multibyte
///         character was decoded, where `consumed` is how many input
///         bytes it occupied, or `None` otherwise. The original
///         returns a pointer into a `static char buf[6]` and advances
///         the caller's own pointer through a `const char **`;
///         returning the bytes plus a length removes both the shared
///         buffer and the in/out pointer.
///
/// ASCII bails out immediately: a byte below 128 is a complete
/// character on its own, so it is never reported here.
#[must_use]
pub fn mb_unescape(p: &[u8]) -> Option<(Vec<u8>, usize)> {
    // The maximum length of a UTF-8 character is 4 bytes.
    let mut buf: Vec<u8> = Vec::with_capacity(4);

    let mut str_idx = 0;
    while str_idx < p.len() && p[str_idx] != crate::ascii_defs::NUL && buf.len() < 4 {
        if p[str_idx] == crate::keycodes_defs::K_SPECIAL
            && p.get(str_idx + 1) == Some(&crate::keycodes_defs::KS_SPECIAL)
            && p.get(str_idx + 2) == Some(&crate::keycodes_defs::KE_FILLER)
        {
            buf.push(crate::keycodes_defs::K_SPECIAL);
            str_idx += 2;
        } else if p[str_idx] == crate::keycodes_defs::K_SPECIAL {
            // A special key can't be a multibyte char.
            break;
        } else {
            buf.push(p[str_idx]);
        }

        // Report a multibyte character once one is complete. An
        // illegal sequence yields a length of 1 here, so it does not
        // match.
        if utf_ptr2len(&buf) > 1 {
            return Some((buf, str_idx + 1));
        }

        // Bail out quickly for ASCII.
        if buf[0] < 128 {
            break;
        }

        str_idx += 1;
    }

    None
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

/// Whether `byte` is a UTF-8 continuation (trail) byte
/// (`utf_is_trail_byte`).
#[must_use]
pub const fn utf_is_trail_byte(byte: u8) -> bool {
    (byte & 0xC0) == 0x80
}

/// The byte offsets from `p` (at index `pos` within `base`) to the
/// first and one-past-end bytes of the codepoint it points into,
/// looking at no more than `p_len` bytes forward
/// (`utf_cp_bounds_len`).
///
/// `pos` may point anywhere in the byte stream, including into the
/// middle of a multi-byte sequence - that is the whole purpose of this
/// function. An illegal or incomplete sequence yields `{ 0, 1 }`.
///
/// The original takes two raw pointers (`base` and `p_in`) purely so
/// it can bound how far backward the scan may walk; a slice plus an
/// index carries the same information without raw pointers.
///
/// # Panics
/// In debug builds, if `pos` is out of bounds or `p_len` is zero.
#[must_use]
pub fn utf_cp_bounds_len(
    base: &[u8],
    pos: usize,
    p_len: i32,
) -> crate::mbyte_defs::CharBoundsOff {
    use crate::mbyte_defs::CharBoundsOff;
    const ILLEGAL: CharBoundsOff = CharBoundsOff {
        begin_off: 0,
        end_off: 1,
    };
    debug_assert!(pos < base.len() && p_len > 0);
    if base[pos] < 0x80 {
        // be quick for ASCII
        return ILLEGAL;
    }

    // How far back the scan may walk: not past `base`'s start, and
    // never more than one maximal codepoint.
    let max_first_off = -(pos.min(crate::mbyte_defs::MB_MAXCHAR - 1) as i32);
    let mut first_off = 0i32;
    while utf_is_trail_byte(base[(pos as i32 + first_off) as usize]) {
        if first_off == max_first_off {
            // failed to find first byte
            return ILLEGAL;
        }
        first_off -= 1;
    }

    let lead = base[(pos as i32 + first_off) as usize];
    let max_end_off = i32::from(UTF8LEN_TAB[lead as usize]) + first_off;
    if max_end_off <= 0 || max_end_off > p_len {
        // illegal or incomplete sequence
        return ILLEGAL;
    }

    for end_off in 1..max_end_off {
        let idx = pos as i32 + end_off;
        if idx as usize >= base.len() || !utf_is_trail_byte(base[idx as usize]) {
            // not enough trail bytes
            return ILLEGAL;
        }
    }

    CharBoundsOff {
        begin_off: i8::try_from(-first_off).unwrap_or(0),
        end_off: i8::try_from(max_end_off).unwrap_or(1),
    }
}

/// The byte offsets from `pos` to the first and one-past-end bytes of
/// the codepoint it points into (`utf_cp_bounds`).
///
/// Counts individual codepoints of composed characters separately.
#[must_use]
pub fn utf_cp_bounds(base: &[u8], pos: usize) -> crate::mbyte_defs::CharBoundsOff {
    utf_cp_bounds_len(base, pos, i32::MAX)
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

/// Build a `schar_T` from `buf`, prefixing a space when the sequence
/// begins with a composing character (`schar_from_buf_first`).
///
/// A leading composing character has nothing to combine with, so the
/// original gives it a space to sit on rather than letting it merge
/// into whatever precedes it on screen.
fn schar_from_buf_first(buf: &[u8], first_compose: bool) -> crate::types_defs::ScharT {
    if first_compose {
        let mut cbuf = [0u8; crate::types_defs::MAX_SCHAR_SIZE];
        cbuf[0] = b' ';
        cbuf[1..=buf.len()].copy_from_slice(buf);
        crate::grid::schar_from_buf(&cbuf[..buf.len() + 1])
    } else {
        crate::grid::schar_from_buf(buf)
    }
}

/// Get the screen character at `p`, along with its first codepoint
/// (`utfc_ptr2schar`).
///
/// Returns `(schar, firstc)`. The original takes `firstc` as an
/// out-parameter with a "NOT optional, you are gonna need it" note;
/// a tuple says the same thing without letting a caller skip it.
///
/// Returns a `schar` of `0` for an invalid byte sequence.
///
/// # Safety
/// Forwarded from [`utfc_ptr2len_len`]'s own safety doc.
#[must_use]
pub unsafe fn utfc_ptr2schar(p: &[u8]) -> (crate::types_defs::ScharT, i32) {
    let c = utf_ptr2char(p);
    let firstc = c;
    let first_compose = utf_iscomposing_first(c);
    let maxlen = crate::types_defs::MAX_SCHAR_SIZE - 1 - usize::from(first_compose);
    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { utfc_ptr2len_len(p, maxlen.min(p.len())) };
    let len = usize::try_from(len).unwrap_or(0);

    if len == 1 && p[0] >= 0x80 {
        return (0, firstc); // invalid sequence
    }

    (schar_from_buf_first(&p[..len], first_compose), firstc)
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

/// `mb_isalpha`.
///
/// # Safety
/// Same as [`mb_tolower`] (forwarded via [`mb_islower`]/[`mb_isupper`]).
#[must_use]
pub unsafe fn mb_isalpha(a: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { mb_islower(a) || mb_isupper(a) }
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

/// Compare strings, optionally case-insensitively (`mb_strcmp_ic`).
///
/// @return 0 if strings are equal, <0 if `s1` < `s2`, >0 if `s1` >
/// `s2` (matching [`mb_stricmp`]'s own convention for the
/// case-insensitive path; the case-sensitive path uses plain
/// lexicographic byte comparison, `strcmp`'s Rust equivalent).
#[must_use]
pub fn mb_strcmp_ic(ic: bool, s1: &[u8], s2: &[u8]) -> i32 {
    if ic {
        mb_stricmp(s1, s2)
    } else {
        match s1.cmp(s2) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }
    }
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

    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { crate::charset::vim_isprintc(c) } {
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

/// Sorted list of non-overlapping `(first, last, class)` intervals
/// used by [`utf_class_tab`] to classify characters `>= 0x100`
/// (`classes[]`, a `static struct clinterval[]` in the original).
///
/// Mechanically extracted from the real `mbyte.c` source via a
/// throwaway PowerShell regex script (not hand-transcribed) - all 71
/// entries, verified to match the original 1:1 in count and value.
#[rustfmt::skip]
const UTF_CLASS_TABLE: &[(u32, u32, u32)] = &[
    (0x037e, 0x037e, 1),
    (0x0387, 0x0387, 1),
    (0x055a, 0x055f, 1),
    (0x0589, 0x0589, 1),
    (0x05be, 0x05be, 1),
    (0x05c0, 0x05c0, 1),
    (0x05c3, 0x05c3, 1),
    (0x05f3, 0x05f4, 1),
    (0x060c, 0x060c, 1),
    (0x061b, 0x061b, 1),
    (0x061f, 0x061f, 1),
    (0x066a, 0x066d, 1),
    (0x06d4, 0x06d4, 1),
    (0x0700, 0x070d, 1),
    (0x0964, 0x0965, 1),
    (0x0970, 0x0970, 1),
    (0x0df4, 0x0df4, 1),
    (0x0e4f, 0x0e4f, 1),
    (0x0e5a, 0x0e5b, 1),
    (0x0f04, 0x0f12, 1),
    (0x0f3a, 0x0f3d, 1),
    (0x0f85, 0x0f85, 1),
    (0x104a, 0x104f, 1),
    (0x10fb, 0x10fb, 1),
    (0x1361, 0x1368, 1),
    (0x166d, 0x166e, 1),
    (0x1680, 0x1680, 0),
    (0x169b, 0x169c, 1),
    (0x16eb, 0x16ed, 1),
    (0x1735, 0x1736, 1),
    (0x17d4, 0x17dc, 1),
    (0x1800, 0x180a, 1),
    (0x2000, 0x200b, 0),
    (0x200c, 0x2027, 1),
    (0x2028, 0x2029, 0),
    (0x202a, 0x202e, 1),
    (0x202f, 0x202f, 0),
    (0x2030, 0x205e, 1),
    (0x205f, 0x205f, 0),
    (0x2060, 0x206f, 1),
    (0x2070, 0x207f, 0x2070),
    (0x2080, 0x2094, 0x2080),
    (0x20a0, 0x27ff, 1),
    (0x2800, 0x28ff, 0x2800),
    (0x2900, 0x2998, 1),
    (0x29d8, 0x29db, 1),
    (0x29fc, 0x29fd, 1),
    (0x2e00, 0x2e7f, 1),
    (0x3000, 0x3000, 0),
    (0x3001, 0x3020, 1),
    (0x3030, 0x3030, 1),
    (0x303d, 0x303d, 1),
    (0x3040, 0x309f, 0x3040),
    (0x30a0, 0x30ff, 0x30a0),
    (0x3300, 0x9fff, 0x4e00),
    (0xac00, 0xd7a3, 0xac00),
    (0xf900, 0xfaff, 0x4e00),
    (0xfd3e, 0xfd3f, 1),
    (0xfe30, 0xfe6b, 1),
    (0xff00, 0xff0f, 1),
    (0xff1a, 0xff20, 1),
    (0xff3b, 0xff40, 1),
    (0xff5b, 0xff65, 1),
    (0x1d000, 0x1d24f, 1),
    (0x1d400, 0x1d7ff, 1),
    (0x1f000, 0x1f2ff, 1),
    (0x1f300, 0x1f9ff, 1),
    (0x20000, 0x2a6df, 0x4e00),
    (0x2a700, 0x2b73f, 0x4e00),
    (0x2b740, 0x2b81f, 0x4e00),
    (0x2f800, 0x2fa1f, 0x4e00),
];

/// Get class of a Unicode character `c` given an explicit `chartab`
/// (`utf_class_tab`): `0` for white space, `1` for punctuation, `2` or
/// bigger for some class of word character (bigger classes, e.g.
/// Hiragana/Katakana/CJK/Hangul/braille/super-and-subscript, are
/// distinguished from plain "class 2" so that adjacent runs of
/// different exotic scripts don't get merged into a single "word" by
/// the `w`/`b`/`e` word-motion commands).
///
/// For `c < 0x100`, uses `'iskeyword'`'s own default value the same
/// way [`crate::charset::vim_iswordc_tab`] does (see that function's
/// own doc comment for why this is a genuinely faithful, not merely
/// approximate, translation in this crate today) - dispatched through
/// `vim_iswordc_tab` itself since the two functions are mutually
/// recursive in the original (this one delegates to it for `c <
/// 0x100`; it delegates back here for `c >= 0x100`).
#[must_use]
pub fn utf_class_tab(c: i32, chartab: &[u64; 4]) -> i32 {
    if c < 0x100 {
        if c == i32::from(b' ') || c == i32::from(crate::ascii_defs::TAB) || c == 0 || c == 0xa0 {
            return 0; // blank
        }
        if crate::charset::vim_iswordc_tab(c, chartab) {
            return 2; // word character
        }
        return 1; // punctuation
    }

    // SAFETY: utf8proc_get_property never returns null (documented
    // utf8proc contract - it always returns a valid "default entry"
    // even for out-of-range/invalid codepoints).
    let prop = unsafe { &*utf8proc_sys::utf8proc_get_property(c) };
    // emoji
    if prop_is_emojilike(prop) {
        return 3;
    }

    // binary search in table
    let c = c as u32;
    let mut bot = 0i64;
    let mut top = UTF_CLASS_TABLE.len() as i64 - 1;
    while top >= bot {
        let mid = ((bot + top) / 2) as usize;
        let (first, last, cls) = UTF_CLASS_TABLE[mid];
        if last < c {
            bot = mid as i64 + 1;
        } else if first > c {
            top = mid as i64 - 1;
        } else {
            return cls as i32;
        }
    }

    // most other characters are "word" characters
    2
}

/// Get class of a Unicode character `c` in the current buffer
/// (`utf_class`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn utf_class(c: i32) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let chartab = unsafe { &(*crate::globals::GLOBALS.get_mut().curbuf).b_chartab };
    utf_class_tab(c, chartab)
}

/// Get class of the character at pointer `p`, given an explicit
/// `chartab` (`mb_get_class_tab`): `0` for blank or NUL, `1` for
/// punctuation, `2` for an alphanumeric word character, `>2` for other
/// word characters (including CJK and emoji) - placed here next to
/// [`utf_class_tab`] (its own real dependency) rather than matching
/// the original's own much-earlier declaration order in `mbyte.c`,
/// for readability.
#[must_use]
pub fn mb_get_class_tab(p: &[u8], chartab: &[u64; 4]) -> i32 {
    let Some(&b0) = p.first() else {
        return 0;
    };
    if UTF8LEN_TAB[b0 as usize] == 1 {
        if b0 == 0 || crate::ascii_defs::ascii_iswhite(i32::from(b0)) {
            return 0;
        }
        if crate::charset::vim_iswordc_tab(i32::from(b0), chartab) {
            return 2;
        }
        return 1;
    }
    utf_class_tab(utf_ptr2char(p), chartab)
}

/// Get class of the character at pointer `p` in the current buffer
/// (`mb_get_class`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn mb_get_class(p: &[u8]) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let chartab = unsafe { &(*crate::globals::GLOBALS.get_mut().curbuf).b_chartab };
    mb_get_class_tab(p, chartab)
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

/// Whether the character at `p` has ambiguous East Asian Width (so
/// `'ambiwidth'` decides its real display width), or turns into an
/// emoji when immediately followed by `U+FE0F` VARIATION SELECTOR-16
/// (`utf_ambiguous_width`).
///
/// No real caller is translated yet (`api/ui.c`'s `nvim_ui_attach`/
/// `tui/tui.c`'s terminal-capability probing) - harvested ahead of
/// them, matching this crate's established precedent for a small,
/// self-contained function with no design freedom of its own.
#[must_use]
pub fn utf_ambiguous_width(p: &[u8]) -> bool {
    // be quick if there is nothing to print or ASCII-only
    if p.first().copied().unwrap_or(0) == 0 || p.get(1).copied().unwrap_or(0) == 0 {
        return false;
    }

    let info = utf_ptr2char_info(p);
    if info.value >= 0x80 {
        // SAFETY: utf8proc_get_property never returns null (documented
        // utf8proc contract - it always returns a valid "default entry"
        // even for out-of-range/invalid codepoints).
        let prop = unsafe { &*utf8proc_sys::utf8proc_get_property(info.value) };
        if prop.ambiguous_width() != 0 || prop_is_emojilike(prop) {
            return true;
        }
    }

    // check if second sequence is 0xFE0F VS-16 which can turn things
    // into emoji, safe with no second sequence (fewer than 3 bytes
    // remain after `info.len`, matching the original's own NUL-safe
    // `memcmp` - a genuine mismatch, not a bounds error, when the
    // buffer simply doesn't extend that far).
    p.get(info.len..info.len + 3) == Some(&[0xef, 0xb8, 0x8f][..])
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

/// Return the offset from `p_idx` to the first byte of a character.
/// When `p_idx` is at the start of a character `0` is returned,
/// otherwise the offset to the next character (`mb_off_next`). Can
/// start anywhere in a stream of bytes.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via [`utf_head_off`]/
/// [`utfc_ptr2len`]).
#[must_use]
pub unsafe fn mb_off_next(base: &[u8], p_idx: usize) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let head_off = unsafe { utf_head_off(base, p_idx) };
    if head_off == 0 {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { utfc_ptr2len(&base[p_idx - head_off as usize..]) };
    len - head_off
}

/// Get the character at the start of `s` plus the byte length to
/// advance past it - composing characters are SKIPPED (folded into
/// the base character's own advance), matching the original's
/// `mb_ptr2char_adv` (a `const char **pp` in/out pointer there;
/// `(codepoint, bytes_consumed)` here, since a Rust slice already
/// carries its own length).
///
/// # Safety
/// Touches `OPTION_VARS` (via [`utfc_ptr2len`]).
#[must_use]
pub unsafe fn mb_ptr2char_adv(s: &[u8]) -> (i32, usize) {
    let c = utf_ptr2char(s);
    // SAFETY: forwarded from this function's own safety doc.
    let len = usize::try_from(unsafe { utfc_ptr2len(s) }).unwrap_or(0);
    (c, len)
}

/// Get the character at the start of `s` plus the byte length to
/// advance past it - composing characters are returned as SEPARATE
/// characters on the next call (`mb_cptr2char_adv`).
#[must_use]
pub fn mb_cptr2char_adv(s: &[u8]) -> (i32, usize) {
    let c = utf_ptr2char(s);
    let len = usize::try_from(utf_ptr2len(s)).unwrap_or(0);
    (c, len)
}

/// The number of display cells `str` (a whole, NUL-free Vimscript
/// string) occupies, one composing-aware character at a time
/// (`mb_string2cells`).
///
/// # Safety
/// Touches `OPTION_VARS` (via [`utfc_ptr2len`]/[`utf_ptr2cells`]).
#[must_use]
pub unsafe fn mb_string2cells(str: &[u8]) -> usize {
    let mut clen = 0usize;
    let mut p = 0usize;
    while p < str.len() {
        // SAFETY: forwarded from this function's own safety doc.
        clen += usize::try_from(unsafe { utf_ptr2cells(&str[p..]) }).unwrap_or(0);
        // SAFETY: forwarded from this function's own safety doc.
        let adv = usize::try_from(unsafe { utfc_ptr2len(&str[p..]) }).unwrap_or(0);
        p += adv.max(1);
    }
    clen
}

/// Count the number of characters in a NUL-terminated Vimscript
/// string byte slice - composing marks are grouped with their base
/// character into one unit, matching [`utfc_ptr2len`]'s own grouping
/// (`mb_charlen`).
///
/// # Safety
/// Touches `OPTION_VARS` (via [`utfc_ptr2len`]).
#[must_use]
pub unsafe fn mb_charlen(s: &[u8]) -> i32 {
    let mut count = 0i32;
    let mut p = 0usize;
    while p < s.len() {
        // SAFETY: forwarded from this function's own safety doc.
        let adv = usize::try_from(unsafe { utfc_ptr2len(&s[p..]) }).unwrap_or(0).max(1);
        p += adv;
        count += 1;
    }
    count
}

/// Count the characters in `s[..len]`, reporting both the codepoint
/// count and the UTF-16 code-unit count (`mb_utflen`).
///
/// Both counters are ADDED to, not assigned, matching the original's
/// own `*codepoints += count` accumulate-into-out-parameter shape, so
/// callers can total several chunks.
///
/// Characters above the BMP take two UTF-16 code units, so
/// `codeunits` grows by one extra per such character. Note the
/// original deliberately reads the raw byte value for an invalid
/// sequence (only whether it fits in the BMP matters), which this
/// mirrors.
pub fn mb_utflen(s: &[u8], len: usize, codepoints: &mut usize, codeunits: &mut usize) {
    let mut count = 0usize;
    let mut extra = 0usize;
    let mut i = 0usize;
    while i < len {
        let clen = usize::try_from(utf_ptr2len_len(&s[i..], len - i)).unwrap_or(1).max(1);
        // NB: gets the byte value of invalid sequence bytes. We only
        // care whether the char fits in the BMP or not.
        let c = if clen > 1 { utf_ptr2char(&s[i..]) } else { i32::from(s[i]) };
        count += 1;
        if c > 0xFFFF {
            extra += 1;
        }
        i += clen;
    }
    *codepoints += count;
    *codeunits += count + extra;
}

/// Byte offset just past the character at character index `index` in
/// `s[..len]`, or `-1` if the string has fewer than `index`
/// characters (`mb_utf_index_to_bytes`).
///
/// With `use_utf16_units` the index counts UTF-16 code units, so a
/// character above the BMP advances the count by two.
#[must_use]
pub fn mb_utf_index_to_bytes(s: &[u8], len: usize, index: usize, use_utf16_units: bool) -> isize {
    if index == 0 {
        return 0;
    }
    let mut count = 0usize;
    let mut i = 0usize;
    while i < len {
        let clen = usize::try_from(utf_ptr2len_len(&s[i..], len - i)).unwrap_or(1).max(1);
        // NB: as in mb_utflen, the raw byte value is used for an
        // invalid sequence.
        let c = if clen > 1 { utf_ptr2char(&s[i..]) } else { i32::from(s[i]) };
        count += 1;
        if use_utf16_units && c > 0xFFFF {
            count += 1;
        }
        if count >= index {
            return (i + clen) as isize;
        }
        i += clen;
    }
    -1
}

/// Byte index of the character BEFORE `p_idx` in `line`
/// (`mb_prevptr`).
///
/// Returns `p_idx` unchanged when it is already at the start.
/// Composing characters are NOT grouped here - the original's
/// `MB_PTR_BACK` steps back one whole base character only.
///
/// # Safety
/// Forwarded from [`utf_head_off`]'s own safety doc. `p_idx` must be
/// within `line`.
#[must_use]
pub unsafe fn mb_prevptr(line: &[u8], p_idx: usize) -> usize {
    if p_idx > 0 {
        // SAFETY: forwarded from this function's own safety doc -
        // `p_idx > 0`, so `p_idx - 1` is a valid index.
        let off = usize::try_from(unsafe { utf_head_off(line, p_idx - 1) }).unwrap_or(0);
        return p_idx - off - 1;
    }
    p_idx
}

/// Character length of `str[..len]`, each multi-byte character (with
/// any following composing characters) counting as one
/// (`mb_charlen_len`).
///
/// Unlike [`mb_charlen`] this is bounded by `len` as well as by a NUL
/// byte, stopping at whichever comes first.
///
/// # Safety
/// Forwarded from [`utfc_ptr2len`]'s own safety doc.
#[must_use]
pub unsafe fn mb_charlen_len(str: &[u8], len: usize) -> i32 {
    let end = len.min(str.len());
    let mut count = 0i32;
    let mut p = 0usize;
    while p < end && str[p] != crate::ascii_defs::NUL {
        // SAFETY: forwarded from this function's own safety doc.
        let adv = usize::try_from(unsafe { utfc_ptr2len(&str[p..]) }).unwrap_or(0).max(1);
        p += adv;
        count += 1;
    }
    count
}

/// Skip a `"2byte-"`/`"8bit-"` prefix on an encoding name, returning
/// the byte index just past it (`enc_skip`).
#[must_use]
pub fn enc_skip(p: &[u8]) -> usize {
    if p.starts_with(b"2byte-") {
        return 6;
    }
    if p.starts_with(b"8bit-") {
        return 5;
    }
    0
}

/// Adjust the cursor to a multi-byte character's head byte, and reset
/// `coladd` when it sits on the right half of a double-wide character
/// (`mb_adjust_cursor`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS`, with the usual "no overlapping
/// live access" requirement. Forwarded from
/// [`crate::mark::mark_mb_adjustpos`]'s own safety doc.
pub unsafe fn mb_adjust_cursor() {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *globals.curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &mut *globals.curwin };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::mark::mark_mb_adjustpos(buf, &mut curwin.w_cursor) };
}

/// Whether space is NOT allowed before/after `cc` (`utf_eat_space`) -
/// a fixed set of Unicode punctuation ranges (general/supplemental
/// punctuation, CJK symbols, full-width ASCII punctuation).
#[must_use]
pub const fn utf_eat_space(cc: i32) -> bool {
    (cc >= 0x2000 && cc <= 0x206F) // General punctuations
        || (cc >= 0x2e00 && cc <= 0x2e7f) // Supplemental punctuations
        || (cc >= 0x3000 && cc <= 0x303f) // CJK symbols and punctuations
        || (cc >= 0xff01 && cc <= 0xff0f) // Full width ASCII punctuations
        || (cc >= 0xff1a && cc <= 0xff20) // ..
        || (cc >= 0xff3b && cc <= 0xff40) // ..
        || (cc >= 0xff5b && cc <= 0xff65) // ..
}

/// Punctuation characters where a line break is never allowed
/// immediately BEFORE (`utf_allow_break_before`'s own fixed,
/// sorted `BOL_prohibition_punct` table - closing brackets/
/// punctuation that shouldn't start a new line).
const BOL_PROHIBITION_PUNCT: [i32; 43] = [
    b'!' as i32,
    b'%' as i32,
    b')' as i32,
    b',' as i32,
    b':' as i32,
    b';' as i32,
    b'>' as i32,
    b'?' as i32,
    b']' as i32,
    b'}' as i32,
    0x2019, // ' right single quotation mark
    0x201d, // " right double quotation mark
    0x2020, // dagger
    0x2021, // double dagger
    0x2026, // horizontal ellipsis
    0x2030, // per mille sign
    0x2031, // per ten thousand sign
    0x203c, // double exclamation mark
    0x2047, // double question mark
    0x2048, // question exclamation mark
    0x2049, // exclamation question mark
    0x2103, // degree celsius
    0x2109, // degree fahrenheit
    0x3001, // ideographic comma
    0x3002, // ideographic full stop
    0x3009, // right angle bracket
    0x300b, // right double angle bracket
    0x300d, // right corner bracket
    0x300f, // right white corner bracket
    0x3011, // right black lenticular bracket
    0x3015, // right tortoise shell bracket
    0x3017, // right white lenticular bracket
    0x3019, // right white tortoise shell bracket
    0x301b, // right white square bracket
    0xff01, // fullwidth exclamation mark
    0xff09, // fullwidth right parenthesis
    0xff0c, // fullwidth comma
    0xff0e, // fullwidth full stop
    0xff1a, // fullwidth colon
    0xff1b, // fullwidth semicolon
    0xff1f, // fullwidth question mark
    0xff3d, // fullwidth right square bracket
    0xff5d, // fullwidth right curly bracket
];

/// Punctuation characters where a line break is never allowed
/// immediately AFTER (`utf_allow_break_after`'s own fixed, sorted
/// `EOL_prohibition_punct` table - opening brackets/punctuation that
/// shouldn't end a line).
const EOL_PROHIBITION_PUNCT: [i32; 19] = [
    b'(' as i32,
    b'<' as i32,
    b'[' as i32,
    b'`' as i32,
    b'{' as i32,
    0x2018, // left single quotation mark
    0x201c, // left double quotation mark
    0x3008, // left angle bracket
    0x300a, // left double angle bracket
    0x300c, // left corner bracket
    0x300e, // left white corner bracket
    0x3010, // left black lenticular bracket
    0x3014, // left tortoise shell bracket
    0x3016, // left white lenticular bracket
    0x3018, // left white tortoise shell bracket
    0x301a, // left white square bracket
    0xff08, // fullwidth left parenthesis
    0xff3b, // fullwidth left square bracket
    0xff5b, // fullwidth left curly bracket
];

/// Whether a line break is allowed before `cc` (`utf_allow_break_before`).
///
/// The original's own hand-rolled binary search over
/// `BOL_PROHIBITION_PUNCT` (a real, verified-sorted array) is
/// replaced with `[T]::binary_search`, provably equivalent given the
/// array's own real sortedness - Rust's standard binary search over
/// the identical data.
#[must_use]
pub fn utf_allow_break_before(cc: i32) -> bool {
    BOL_PROHIBITION_PUNCT.binary_search(&cc).is_err()
}

/// Whether a line break is allowed after `cc` (`utf_allow_break_after`).
/// See [`utf_allow_break_before`]'s own doc for the binary-search
/// substitution reasoning.
#[must_use]
pub fn utf_allow_break_after(cc: i32) -> bool {
    EOL_PROHIBITION_PUNCT.binary_search(&cc).is_err()
}

/// Whether a line break is allowed between `cc` and `ncc`
/// (`utf_allow_break`) - never between two identical em-dashes or
/// horizontal-ellipsis characters, otherwise delegates to
/// [`utf_allow_break_after`]/[`utf_allow_break_before`].
#[must_use]
pub fn utf_allow_break(cc: i32, ncc: i32) -> bool {
    if cc == ncc && (cc == 0x2014 || cc == 0x2026) {
        return false;
    }
    utf_allow_break_after(cc) && utf_allow_break_before(ncc)
}

/// Returns `true` if `s` is a valid UTF-8 byte sequence
/// (`utf_valid_string`).
///
/// Always checks the WHOLE given slice (the original's own `end ==
/// NULL` "stop at the first NUL" mode is not modeled separately - a
/// NUL byte is itself a valid, length-1 ASCII "character" per
/// [`UTF8LEN_TAB_ZERO`], so it never terminates the scan early on
/// either path; a caller wanting the NUL-terminated behavior can
/// simply slice `s` up to its own first NUL byte before calling this,
/// an equally faithful and arguably safer approach than relying on an
/// implicit terminator a Rust `&[u8]` doesn't have).
#[must_use]
pub fn utf_valid_string(s: &[u8]) -> bool {
    let mut p = 0usize;
    while p < s.len() {
        let l = usize::from(UTF8LEN_TAB_ZERO[s[p] as usize]);
        if l == 0 {
            return false; // invalid lead byte
        }
        if p + l > s.len() {
            return false; // incomplete byte sequence
        }
        p += 1;
        for _ in 1..l {
            if (s[p] & 0xc0) != 0x80 {
                return false; // invalid trail byte
            }
            p += 1;
        }
    }
    true
}

/// Number of bytes a byte-order mark for the buffer's own `'fenc'`
/// would occupy, or `0` if `'bomb'`/binary mode mean no BOM should be
/// written (`bomb_size`).
#[must_use]
pub fn bomb_size(buf: &crate::buffer_defs::BufT) -> i32 {
    if buf.b_p_bomb == 0 || buf.b_p_bin != 0 {
        return 0;
    }
    match buf.b_p_fenc.as_deref() {
        None | Some(b"utf-8") => 3,
        Some(fenc) if fenc.starts_with(b"ucs-2") || fenc.starts_with(b"utf-16") => 2,
        Some(fenc) if fenc.starts_with(b"ucs-4") => 4,
        _ => 0,
    }
}

/// Remove every UTF-8 byte-order-mark (`EF BB BF`) occurrence from
/// `s`, shifting the remaining bytes down in place (`remove_bom`).
pub fn remove_bom(s: &mut Vec<u8>) {
    let mut p = 0;
    while p < s.len() {
        if s[p] == 0xef && s.get(p + 1) == Some(&0xbb) && s.get(p + 2) == Some(&0xbf) {
            s.drain(p..p + 3);
            continue;
        }
        p += 1;
    }
}

/// One entry of [`ENC_CANON_TABLE`] (the original's own anonymous
/// `struct { const char *name; int prop; int codepage; }`).
///
/// `codepage` is deliberately NOT modeled: nothing in this crate
/// consumes it yet (it's only used by `enc_canonize`'s own DOS/
/// Windows-codepage detection path and a handful of `os_win_console.c`
/// call sites, none translated) - a documented, narrow omission,
/// re-addable field-for-field from the real table (already viewed in
/// full during translation) the moment a real consumer needs it,
/// rather than risking a transcription mistake on ~20 `DBCS_*`
/// sentinel values nothing currently reads.
pub struct EncCanonEntry {
    pub name: &'static str,
    pub prop: i32,
}

/// The screen character for `p`, using no more than `len` bytes
/// (`utfc_ptrlen2schar`).
///
/// Like `utfc_ptr2schar`, but bounded by a known length rather than
/// scanning to a NUL.
///
/// @return `(schar, firstc)` - the packed screen character, plus the
///         first codepoint (the original's own `int *firstc`
///         out-parameter). A `schar` of 0 means the sequence was
///         invalid or truncated, in which case `firstc` is the raw
///         leading byte rather than a decoded codepoint.
///
/// # Safety
/// Forwarded from [`utfc_ptr2len_len`]'s own safety doc.
#[must_use]
pub unsafe fn utfc_ptrlen2schar(p: &[u8], len: i32) -> (crate::types_defs::ScharT, i32) {
    let first_byte = p.first().copied().unwrap_or(0);
    if (len == 1 && first_byte >= 0x80) || len == 0 {
        // Invalid or truncated sequence.
        return (0, i32::from(first_byte));
    }

    let c = utf_ptr2char(p);
    let first_compose = utf_iscomposing_first(c);
    let maxlen = crate::types_defs::MAX_SCHAR_SIZE as i32 - 1 - i32::from(first_compose);
    let mut len = len;
    if len > maxlen {
        // SAFETY: forwarded from this function's own safety doc.
        len = unsafe { utfc_ptr2len_len(p, maxlen as usize) };
    }

    let take = (len.max(0) as usize).min(p.len());
    (schar_from_buf_first(&p[..take], first_compose), c)
}

/// The canonical encoding name at `idx`, for `ExpandGeneric`
/// (`get_encoding_name`).
///
/// The original takes an unused `expand_T *xp` that has no equivalent
/// here, and returns `NULL` past the end of the table; that becomes
/// `None`.
#[must_use]
pub fn get_encoding_name(idx: i32) -> Option<&'static str> {
    if idx < 0 || idx as usize >= ENC_CANON_TABLE.len() {
        return None;
    }
    Some(ENC_CANON_TABLE[idx as usize].name)
}

/// Canonical encoding names and their properties (`enc_canon_table`).
/// `"iso-8859-n"` is handled by `enc_canonize()` directly (not
/// translated) - this table's own entries for those exist only to be
/// found BY NAME when `enc_canon_search`/`enc_canon_props` are called
/// with an already-canonical `"iso-8859-N"` string, same as the
/// original.
///
/// Mechanically transcribed directly from the real `mbyte.c` source
/// (not hand-typed from memory) - every `name`/`prop` pair was
/// re-read from the original array literal immediately before being
/// written here, index by index, to avoid a transcription slip.
pub const ENC_CANON_TABLE: [EncCanonEntry; 60] = [
    EncCanonEntry { name: "latin1", prop: crate::mbyte_defs::enc::ENC_8BIT + crate::mbyte_defs::enc::ENC_LATIN1 }, // 0: IDX_LATIN_1
    EncCanonEntry { name: "iso-8859-2", prop: crate::mbyte_defs::enc::ENC_8BIT },               // 1: IDX_ISO_2
    EncCanonEntry { name: "iso-8859-3", prop: crate::mbyte_defs::enc::ENC_8BIT },               // 2: IDX_ISO_3
    EncCanonEntry { name: "iso-8859-4", prop: crate::mbyte_defs::enc::ENC_8BIT },               // 3: IDX_ISO_4
    EncCanonEntry { name: "iso-8859-5", prop: crate::mbyte_defs::enc::ENC_8BIT },               // 4: IDX_ISO_5
    EncCanonEntry { name: "iso-8859-6", prop: crate::mbyte_defs::enc::ENC_8BIT },               // 5: IDX_ISO_6
    EncCanonEntry { name: "iso-8859-7", prop: crate::mbyte_defs::enc::ENC_8BIT },               // 6: IDX_ISO_7
    EncCanonEntry { name: "iso-8859-8", prop: crate::mbyte_defs::enc::ENC_8BIT },               // 7: IDX_ISO_8
    EncCanonEntry { name: "iso-8859-9", prop: crate::mbyte_defs::enc::ENC_8BIT },               // 8: IDX_ISO_9
    EncCanonEntry { name: "iso-8859-10", prop: crate::mbyte_defs::enc::ENC_8BIT },              // 9: IDX_ISO_10
    EncCanonEntry { name: "iso-8859-11", prop: crate::mbyte_defs::enc::ENC_8BIT },              // 10: IDX_ISO_11
    EncCanonEntry { name: "iso-8859-13", prop: crate::mbyte_defs::enc::ENC_8BIT },              // 11: IDX_ISO_13
    EncCanonEntry { name: "iso-8859-14", prop: crate::mbyte_defs::enc::ENC_8BIT },              // 12: IDX_ISO_14
    EncCanonEntry { name: "iso-8859-15", prop: crate::mbyte_defs::enc::ENC_8BIT + crate::mbyte_defs::enc::ENC_LATIN9 }, // 13: IDX_ISO_15
    EncCanonEntry { name: "koi8-r", prop: crate::mbyte_defs::enc::ENC_8BIT },                   // 14: IDX_KOI8_R
    EncCanonEntry { name: "koi8-u", prop: crate::mbyte_defs::enc::ENC_8BIT },                   // 15: IDX_KOI8_U
    EncCanonEntry { name: "utf-8", prop: crate::mbyte_defs::enc::ENC_UNICODE },                 // 16: IDX_UTF8
    EncCanonEntry {
        name: "ucs-2",
        prop: crate::mbyte_defs::enc::ENC_UNICODE + crate::mbyte_defs::enc::ENC_ENDIAN_B + crate::mbyte_defs::enc::ENC_2BYTE,
    }, // 17: IDX_UCS2
    EncCanonEntry {
        name: "ucs-2le",
        prop: crate::mbyte_defs::enc::ENC_UNICODE + crate::mbyte_defs::enc::ENC_ENDIAN_L + crate::mbyte_defs::enc::ENC_2BYTE,
    }, // 18: IDX_UCS2LE
    EncCanonEntry {
        name: "utf-16",
        prop: crate::mbyte_defs::enc::ENC_UNICODE + crate::mbyte_defs::enc::ENC_ENDIAN_B + crate::mbyte_defs::enc::ENC_2WORD,
    }, // 19: IDX_UTF16
    EncCanonEntry {
        name: "utf-16be",
        prop: crate::mbyte_defs::enc::ENC_UNICODE + crate::mbyte_defs::enc::ENC_ENDIAN_B + crate::mbyte_defs::enc::ENC_2WORD,
    }, // 20: IDX_UTF16BE
    EncCanonEntry {
        name: "utf-16le",
        prop: crate::mbyte_defs::enc::ENC_UNICODE + crate::mbyte_defs::enc::ENC_ENDIAN_L + crate::mbyte_defs::enc::ENC_2WORD,
    }, // 21: IDX_UTF16LE
    EncCanonEntry {
        name: "ucs-4",
        prop: crate::mbyte_defs::enc::ENC_UNICODE + crate::mbyte_defs::enc::ENC_ENDIAN_B + crate::mbyte_defs::enc::ENC_4BYTE,
    }, // 22: IDX_UCS4
    EncCanonEntry {
        name: "ucs-4le",
        prop: crate::mbyte_defs::enc::ENC_UNICODE + crate::mbyte_defs::enc::ENC_ENDIAN_L + crate::mbyte_defs::enc::ENC_4BYTE,
    }, // 23: IDX_UCS4LE
    EncCanonEntry { name: "debug", prop: crate::mbyte_defs::enc::ENC_DBCS },     // 24: IDX_DEBUG
    EncCanonEntry { name: "euc-jp", prop: crate::mbyte_defs::enc::ENC_DBCS },    // 25: IDX_EUC_JP
    EncCanonEntry { name: "sjis", prop: crate::mbyte_defs::enc::ENC_DBCS },      // 26: IDX_SJIS
    EncCanonEntry { name: "euc-kr", prop: crate::mbyte_defs::enc::ENC_DBCS },    // 27: IDX_EUC_KR
    EncCanonEntry { name: "euc-cn", prop: crate::mbyte_defs::enc::ENC_DBCS },    // 28: IDX_EUC_CN
    EncCanonEntry { name: "euc-tw", prop: crate::mbyte_defs::enc::ENC_DBCS },    // 29: IDX_EUC_TW
    EncCanonEntry { name: "big5", prop: crate::mbyte_defs::enc::ENC_DBCS },      // 30: IDX_BIG5
    EncCanonEntry { name: "cp437", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 31: IDX_CP437
    EncCanonEntry { name: "cp737", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 32: IDX_CP737
    EncCanonEntry { name: "cp775", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 33: IDX_CP775
    EncCanonEntry { name: "cp850", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 34: IDX_CP850
    EncCanonEntry { name: "cp852", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 35: IDX_CP852
    EncCanonEntry { name: "cp855", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 36: IDX_CP855
    EncCanonEntry { name: "cp857", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 37: IDX_CP857
    EncCanonEntry { name: "cp860", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 38: IDX_CP860
    EncCanonEntry { name: "cp861", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 39: IDX_CP861
    EncCanonEntry { name: "cp862", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 40: IDX_CP862
    EncCanonEntry { name: "cp863", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 41: IDX_CP863
    EncCanonEntry { name: "cp865", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 42: IDX_CP865
    EncCanonEntry { name: "cp866", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 43: IDX_CP866
    EncCanonEntry { name: "cp869", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 44: IDX_CP869
    EncCanonEntry { name: "cp874", prop: crate::mbyte_defs::enc::ENC_8BIT },     // 45: IDX_CP874
    EncCanonEntry { name: "cp932", prop: crate::mbyte_defs::enc::ENC_DBCS },     // 46: IDX_CP932
    EncCanonEntry { name: "cp936", prop: crate::mbyte_defs::enc::ENC_DBCS },     // 47: IDX_CP936
    EncCanonEntry { name: "cp949", prop: crate::mbyte_defs::enc::ENC_DBCS },     // 48: IDX_CP949
    EncCanonEntry { name: "cp950", prop: crate::mbyte_defs::enc::ENC_DBCS },     // 49: IDX_CP950
    EncCanonEntry { name: "cp1250", prop: crate::mbyte_defs::enc::ENC_8BIT },    // 50: IDX_CP1250
    EncCanonEntry { name: "cp1251", prop: crate::mbyte_defs::enc::ENC_8BIT },    // 51: IDX_CP1251
    EncCanonEntry { name: "cp1253", prop: crate::mbyte_defs::enc::ENC_8BIT },    // 52: IDX_CP1253
    EncCanonEntry { name: "cp1254", prop: crate::mbyte_defs::enc::ENC_8BIT },    // 53: IDX_CP1254
    EncCanonEntry { name: "cp1255", prop: crate::mbyte_defs::enc::ENC_8BIT },    // 54: IDX_CP1255
    EncCanonEntry { name: "cp1256", prop: crate::mbyte_defs::enc::ENC_8BIT },    // 55: IDX_CP1256
    EncCanonEntry { name: "cp1257", prop: crate::mbyte_defs::enc::ENC_8BIT },    // 56: IDX_CP1257
    EncCanonEntry { name: "cp1258", prop: crate::mbyte_defs::enc::ENC_8BIT },    // 57: IDX_CP1258
    EncCanonEntry { name: "macroman", prop: crate::mbyte_defs::enc::ENC_8BIT + crate::mbyte_defs::enc::ENC_MACROMAN }, // 58: IDX_MACROMAN
    EncCanonEntry { name: "hp-roman8", prop: crate::mbyte_defs::enc::ENC_8BIT }, // 59: IDX_HPROMAN8
];

/// Find encoding `name` in the list of canonical encoding names.
/// Returns `None` if not found (`enc_canon_search`).
#[must_use]
pub fn enc_canon_search(name: &[u8]) -> Option<usize> {
    ENC_CANON_TABLE.iter().position(|entry| entry.name.as_bytes() == name)
}

/// Find canonical encoding `name` in the list and return its
/// properties. Returns `0` if not found (`enc_canon_props`).
#[must_use]
pub fn enc_canon_props(name: &[u8]) -> i32 {
    if let Some(i) = enc_canon_search(name) {
        return ENC_CANON_TABLE[i].prop;
    }
    if name.starts_with(b"2byte-") {
        return crate::mbyte_defs::enc::ENC_DBCS;
    }
    if name.starts_with(b"8bit-") || name.starts_with(b"iso-8859-") {
        return crate::mbyte_defs::enc::ENC_8BIT;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mb_unescape_returns_none_for_ascii() {
        // A byte below 128 is a complete character on its own, so the
        // scan bails out immediately without reporting anything.
        assert_eq!(mb_unescape(b"abc"), None);
        assert_eq!(mb_unescape(b"x"), None);
    }

    #[test]
    fn mb_unescape_returns_none_for_an_empty_or_nul_input() {
        assert_eq!(mb_unescape(b""), None);
        assert_eq!(mb_unescape(b"\0rest"), None);
    }

    #[test]
    fn mb_unescape_decodes_a_plain_multibyte_character() {
        // U+00E9 is 0xC3 0xA9 in UTF-8: two bytes in, two consumed.
        let (bytes, used) = mb_unescape(b"\xc3\xa9tail").expect("a multibyte char");
        assert_eq!(bytes, vec![0xc3, 0xa9]);
        assert_eq!(used, 2);
    }

    #[test]
    fn mb_unescape_folds_the_k_special_escape() {
        // K_SPECIAL KS_SPECIAL KE_FILLER represents one literal
        // K_SPECIAL byte (0x80), which then forms the LEAD byte of a
        // multibyte sequence rather than a special key.
        let mut input = vec![
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::KS_SPECIAL,
            crate::keycodes_defs::KE_FILLER,
        ];
        // 0x80 is a continuation byte, so on its own it is illegal and
        // yields length 1; append a real second byte to complete a
        // two-byte sequence's worth of input.
        input.push(0xa9);

        // The escape collapses to a single 0x80, so the buffer holds
        // 0x80 0xA9 - still not a valid lead byte, hence None.
        assert_eq!(mb_unescape(&input), None);
    }

    #[test]
    fn mb_unescape_stops_at_a_bare_k_special() {
        // A bare K_SPECIAL starts a real special key, which can never
        // be part of a multibyte character, so the scan stops.
        let input = [0xc3, crate::keycodes_defs::K_SPECIAL, 0xa9];
        assert_eq!(mb_unescape(&input), None);
    }

    #[test]
    fn enc_canon_table_has_exactly_60_entries() {
        assert_eq!(ENC_CANON_TABLE.len(), 60);
    }

    #[test]
    fn enc_canon_table_first_and_last_entries_match_the_real_source() {
        assert_eq!(ENC_CANON_TABLE[0].name, "latin1");
        assert_eq!(ENC_CANON_TABLE[0].prop, crate::mbyte_defs::enc::ENC_8BIT + crate::mbyte_defs::enc::ENC_LATIN1);
        assert_eq!(ENC_CANON_TABLE[59].name, "hp-roman8");
        assert_eq!(ENC_CANON_TABLE[59].prop, crate::mbyte_defs::enc::ENC_8BIT);
    }

    #[test]
    fn enc_canon_search_finds_a_real_entry() {
        assert_eq!(enc_canon_search(b"utf-8"), Some(16));
        assert_eq!(enc_canon_search(b"macroman"), Some(58));
    }

    #[test]
    fn enc_canon_search_returns_none_for_an_unknown_name() {
        assert_eq!(enc_canon_search(b"not-a-real-encoding"), None);
    }

    #[test]
    fn enc_canon_props_returns_the_table_entrys_prop() {
        assert_eq!(enc_canon_props(b"utf-8"), crate::mbyte_defs::enc::ENC_UNICODE);
        assert_eq!(
            enc_canon_props(b"ucs-2"),
            crate::mbyte_defs::enc::ENC_UNICODE + crate::mbyte_defs::enc::ENC_ENDIAN_B + crate::mbyte_defs::enc::ENC_2BYTE
        );
        assert_eq!(enc_canon_props(b"latin1"), crate::mbyte_defs::enc::ENC_8BIT + crate::mbyte_defs::enc::ENC_LATIN1);
    }

    #[test]
    fn enc_canon_props_falls_back_to_2byte_prefix() {
        assert_eq!(enc_canon_props(b"2byte-anything"), crate::mbyte_defs::enc::ENC_DBCS);
    }

    #[test]
    fn enc_canon_props_falls_back_to_8bit_prefix() {
        assert_eq!(enc_canon_props(b"8bit-anything"), crate::mbyte_defs::enc::ENC_8BIT);
    }

    #[test]
    fn enc_canon_props_falls_back_to_iso_8859_prefix() {
        // "iso-8859-1" isn't literally in ENC_CANON_TABLE (that's
        // "latin1" instead, aliased via the NOT-translated
        // enc_alias_table/enc_canonize) - enc_canon_props itself,
        // called directly with an "iso-8859-N" string, falls through
        // to this generic prefix match, matching the real source.
        assert_eq!(enc_canon_props(b"iso-8859-1"), crate::mbyte_defs::enc::ENC_8BIT);
        assert_eq!(enc_canon_props(b"iso-8859-16"), crate::mbyte_defs::enc::ENC_8BIT);
    }

    #[test]
    fn enc_canon_props_returns_zero_for_a_completely_unknown_name() {
        assert_eq!(enc_canon_props(b"not-a-real-encoding"), 0);
    }

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

    // --- utf_ambiguous_width ---

    #[test]
    fn utf_ambiguous_width_empty_or_single_byte_is_false() {
        assert!(!utf_ambiguous_width(b""));
        assert!(!utf_ambiguous_width(b"a"));
        assert!(!utf_ambiguous_width(b"a\0"));
    }

    #[test]
    fn utf_ambiguous_width_plain_ascii_is_false() {
        assert!(!utf_ambiguous_width(b"ab"));
    }

    #[test]
    fn utf_ambiguous_width_true_for_an_ambiguous_width_character() {
        // U+00A1 (INVERTED EXCLAMATION MARK) has East Asian Width
        // "Ambiguous" - already relied upon by the existing
        // utf_char2cells_ambiguous_width_follows_ambiwidth_option test.
        let mut s = "¡".as_bytes().to_vec();
        s.push(b'x');
        assert!(utf_ambiguous_width(&s));
    }

    #[test]
    fn utf_ambiguous_width_true_when_followed_by_variation_selector_16() {
        // A plain ASCII base character (itself neither ambiguous-width
        // nor emoji-like) immediately followed by U+FE0F (VS-16, UTF-8
        // bytes EF B8 8F) - turns into an emoji presentation, matching
        // the original's own dedicated memcmp check.
        let mut s = vec![b'a'];
        s.extend_from_slice(&[0xef, 0xb8, 0x8f]);
        assert!(utf_ambiguous_width(&s));
    }

    #[test]
    fn utf_ambiguous_width_false_for_an_unambiguous_character_without_vs16() {
        // U+4E2D ('中', CJK) is unconditionally double-width (not
        // "Ambiguous"), not emoji-like, and not followed by VS-16.
        let mut s = "中".as_bytes().to_vec();
        s.push(b'x');
        assert!(!utf_ambiguous_width(&s));
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
    fn utf_is_trail_byte_matches_the_continuation_bit_pattern() {
        assert!(utf_is_trail_byte(0x80));
        assert!(utf_is_trail_byte(0xBF));
        assert!(!utf_is_trail_byte(0x7F)); // ASCII
        assert!(!utf_is_trail_byte(0xC0)); // 2-byte lead
        assert!(!utf_is_trail_byte(0xE0)); // 3-byte lead
    }

    #[test]
    fn utf_cp_bounds_locates_the_codepoint_from_any_byte_inside_it() {
        // "─" is E2 94 80: from each of its three bytes the reported
        // window must cover the whole codepoint.
        let s = "─".as_bytes();
        assert_eq!(s.len(), 3);
        for (pos, begin, end) in [(0usize, 0i8, 3i8), (1, 1, 2), (2, 2, 1)] {
            let b = utf_cp_bounds(s, pos);
            assert_eq!((b.begin_off, b.end_off), (begin, end), "at {pos}");
            // The codepoint spans `pos - begin_off .. pos + end_off`,
            // so the two offsets always SUM to its byte length.
            assert_eq!(i32::from(b.begin_off) + i32::from(b.end_off), 3, "at {pos}");
        }
    }

    #[test]
    fn utf_cp_bounds_reports_a_single_byte_for_ascii_and_illegal_input() {
        // ASCII takes the fast path.
        let b = utf_cp_bounds(b"abc", 1);
        assert_eq!((b.begin_off, b.end_off), (0, 1));

        // A lone continuation byte has no lead byte to find.
        let b = utf_cp_bounds(&[0x80, 0x80], 0);
        assert_eq!((b.begin_off, b.end_off), (0, 1));

        // A lead byte whose trail bytes are missing is incomplete.
        let b = utf_cp_bounds(&[0xE2, 0x41], 0);
        assert_eq!((b.begin_off, b.end_off), (0, 1));
    }

    #[test]
    fn utf_cp_bounds_len_rejects_a_sequence_that_runs_past_the_limit() {
        // The full three-byte sequence is fine unbounded...
        let s = "─".as_bytes();
        assert_eq!(utf_cp_bounds_len(s, 0, 3).end_off, 3);
        // ...but reports a single byte when only two may be examined.
        assert_eq!(utf_cp_bounds_len(s, 0, 2).end_off, 1);
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
    fn utfc_ptr2schar_returns_the_glyph_and_its_first_codepoint() {
        let _l = crate::globals::global_state_test_lock();
        for (s, first) in [("a", 0x61i32), ("é", 0xE9), ("─", 0x2500)] {
            let (sc, firstc) = unsafe { utfc_ptr2schar(s.as_bytes()) };
            assert_eq!(firstc, first, "{s}");
            assert_eq!(crate::grid::schar_get(sc), s.as_bytes(), "{s}");
        }
    }

    #[test]
    fn utfc_ptr2schar_keeps_a_combining_char_with_its_base() {
        let _l = crate::globals::global_state_test_lock();
        let bytes = "e\u{0301}".as_bytes();
        let (sc, firstc) = unsafe { utfc_ptr2schar(bytes) };
        assert_eq!(firstc, i32::from(b'e'));
        assert_eq!(crate::grid::schar_get(sc), bytes);
    }

    #[test]
    fn utfc_ptr2schar_rejects_an_invalid_utf8_byte() {
        let _l = crate::globals::global_state_test_lock();
        // A lone continuation byte is not a valid sequence.
        let (sc, _) = unsafe { utfc_ptr2schar(&[0x80]) };
        assert_eq!(sc, 0);
    }

    #[test]
    fn utfc_ptr2schar_gives_a_leading_combining_char_a_space_to_sit_on() {
        let _l = crate::globals::global_state_test_lock();
        // U+0301 has nothing to combine with, so the original
        // prefixes a space rather than letting it merge into whatever
        // precedes it on screen.
        let bytes = "\u{0301}".as_bytes();
        let (sc, _) = unsafe { utfc_ptr2schar(bytes) };
        assert_eq!(crate::grid::schar_get(sc), " \u{0301}".as_bytes());
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
    fn mb_strcmp_ic_true_uses_case_insensitive_compare() {
        assert_eq!(mb_strcmp_ic(true, b"FOO", b"foo"), 0);
        assert_eq!(mb_strcmp_ic(true, b"FOO", b"bar"), mb_stricmp(b"FOO", b"bar"));
    }

    #[test]
    fn mb_strcmp_ic_false_is_case_sensitive() {
        assert_ne!(mb_strcmp_ic(false, b"FOO", b"foo"), 0);
        assert_eq!(mb_strcmp_ic(false, b"foo", b"foo"), 0);
        assert!(mb_strcmp_ic(false, b"abc", b"abd") < 0);
        assert!(mb_strcmp_ic(false, b"abd", b"abc") > 0);
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

    // --- utf_class_tab / utf_class / mb_get_class_tab / mb_get_class ---

    #[test]
    fn utf_class_tab_ascii_blank_punctuation_and_word() {
        let chartab = [0u64; 4];
        assert_eq!(utf_class_tab(i32::from(b' '), &chartab), 0);
        assert_eq!(utf_class_tab(i32::from(crate::ascii_defs::TAB), &chartab), 0);
        assert_eq!(utf_class_tab(0, &chartab), 0);
        assert_eq!(utf_class_tab(0xa0, &chartab), 0); // nbsp
        assert_eq!(utf_class_tab(i32::from(b'!'), &chartab), 1);
        assert_eq!(utf_class_tab(i32::from(b'a'), &chartab), 2);
    }

    #[test]
    fn utf_class_tab_table_lookups() {
        let chartab = [0u64; 4];
        // Exact single-value interval, class 1 (punctuation).
        assert_eq!(utf_class_tab(0x0387, &chartab), 1);
        // Blank interval.
        assert_eq!(utf_class_tab(0x1680, &chartab), 0);
        // Superscript interval, its own distinct class (0x2070).
        assert_eq!(utf_class_tab(0x2070, &chartab), 0x2070);
        // CJK interval, its own distinct class (0x4e00) - '中' itself.
        assert_eq!(utf_class_tab(0x4e2d, &chartab), 0x4e00);
        // Hangul interval, its own distinct class (0xac00).
        assert_eq!(utf_class_tab(0xac00, &chartab), 0xac00);
        // The table's own first entry (binary-search edge) and its own
        // last entry, (0x2f800, 0x2fa1f, 0x4e00) - also CJK, not
        // punctuation.
        assert_eq!(utf_class_tab(0x037e, &chartab), 1);
        assert_eq!(utf_class_tab(0x2fa1f, &chartab), 0x4e00);
    }

    #[test]
    fn utf_class_tab_absent_from_table_defaults_to_word_character() {
        let chartab = [0u64; 4];
        // Cyrillic 'А' (U+0410) isn't in UTF_CLASS_TABLE at all, and
        // isn't emoji-like - falls through to the default "most other
        // characters are word characters" rule.
        assert_eq!(utf_class_tab(0x0410, &chartab), 2);
        // A codepoint immediately after a single-value interval
        // (0x0387 + 1) that itself isn't in any interval.
        assert_eq!(utf_class_tab(0x0388, &chartab), 2);
    }

    #[test]
    fn utf_class_tab_emoji_beats_the_table_entry() {
        let chartab = [0u64; 4];
        // U+1F600 (😀) is within the table's own (0x1f300, 0x1f9ff, 1)
        // interval, but prop_is_emojilike is checked FIRST - class 3
        // wins over the table's own class 1.
        assert_eq!(utf_class_tab(0x1f600, &chartab), 3);
    }

    /// Points `GLOBALS.curbuf` at `buf` for the guard's lifetime,
    /// restoring the previous pointer on drop. Holds
    /// `global_state_test_lock` for its entire lifetime, matching
    /// `charset.rs`'s own identical `CurbufGuard` precedent.
    struct CurbufGuard {
        previous: *mut crate::buffer_defs::BufT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut crate::buffer_defs::BufT) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous, _lock }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    #[test]
    fn utf_class_uses_the_current_buffer() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        assert_eq!(unsafe { utf_class(i32::from(b'a')) }, 2);
        assert_eq!(unsafe { utf_class(i32::from(b' ')) }, 0);
    }

    #[test]
    fn mb_get_class_tab_blank_punctuation_and_word() {
        let chartab = [0u64; 4];
        assert_eq!(mb_get_class_tab(b"", &chartab), 0);
        assert_eq!(mb_get_class_tab(b"\0", &chartab), 0);
        assert_eq!(mb_get_class_tab(b" ", &chartab), 0);
        assert_eq!(mb_get_class_tab(b"!", &chartab), 1);
        assert_eq!(mb_get_class_tab(b"a", &chartab), 2);
    }

    #[test]
    fn mb_get_class_tab_multibyte_delegates_to_utf_class_tab() {
        let chartab = [0u64; 4];
        assert_eq!(mb_get_class_tab("中".as_bytes(), &chartab), 0x4e00);
    }

    #[test]
    fn mb_get_class_uses_the_current_buffer() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        assert_eq!(unsafe { mb_get_class(b"a") }, 2);
        assert_eq!(unsafe { mb_get_class(b" ") }, 0);
        assert_eq!(unsafe { mb_get_class("中".as_bytes()) }, 0x4e00);
    }

    // --- utfc_ptrlen2schar / get_encoding_name ---

    #[test]
    fn utfc_ptrlen2schar_decodes_a_plain_ascii_byte() {
        let (sc, firstc) = unsafe { utfc_ptrlen2schar(b"a", 1) };
        assert_eq!(firstc, i32::from(b'a'));
        assert_ne!(sc, 0, "a valid single byte is a real screen char");
    }

    #[test]
    fn utfc_ptrlen2schar_reports_a_truncated_sequence() {
        // A lone continuation/lead byte with len 1, and a zero
        // length, are both "invalid or truncated": schar is 0 and
        // firstc is the RAW byte rather than a decoded codepoint.
        let (sc, firstc) = unsafe { utfc_ptrlen2schar(b"\xe4", 1) };
        assert_eq!(sc, 0);
        assert_eq!(firstc, 0xe4);

        let (sc, firstc) = unsafe { utfc_ptrlen2schar(b"a", 0) };
        assert_eq!(sc, 0);
        assert_eq!(firstc, i32::from(b'a'));
    }

    #[test]
    fn utfc_ptrlen2schar_decodes_a_multibyte_character() {
        let bytes = "é".as_bytes();
        let (sc, firstc) = unsafe { utfc_ptrlen2schar(bytes, bytes.len() as i32) };
        assert_eq!(firstc, 0xe9);
        assert_ne!(sc, 0);
    }

    #[test]
    fn utfc_ptrlen2schar_respects_the_given_length() {
        // The same buffer decodes differently depending on how much of
        // it the caller says is available - which is the whole point
        // of this variant over utfc_ptr2schar.
        let bytes = "é".as_bytes();
        let (_sc_full, firstc_full) = unsafe { utfc_ptrlen2schar(bytes, bytes.len() as i32) };
        let (sc_trunc, firstc_trunc) = unsafe { utfc_ptrlen2schar(bytes, 1) };
        assert_eq!(firstc_full, 0xe9);
        assert_eq!(sc_trunc, 0, "one byte of a two-byte sequence is truncated");
        assert_eq!(firstc_trunc, i32::from(bytes[0]));
    }

    #[test]
    fn get_encoding_name_reports_table_entries_and_nothing_past_the_end() {
        assert_eq!(get_encoding_name(0), Some("latin1"));
        let last = ENC_CANON_TABLE.len() as i32 - 1;
        assert_eq!(get_encoding_name(last), Some(ENC_CANON_TABLE[last as usize].name));

        assert_eq!(get_encoding_name(ENC_CANON_TABLE.len() as i32), None);
        assert_eq!(get_encoding_name(-1), None);
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

    // --- mb_off_next ---

    #[test]
    fn mb_off_next_zero_for_ascii() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { mb_off_next(b"hello", 2) }, 0);
    }

    #[test]
    fn mb_off_next_zero_already_at_a_lead_byte() {
        let _guard = option_vars_test_lock();
        // "日" (U+65E5) = [0xE6, 0x97, 0xA5], a lone 3-byte character -
        // same fixture as utf_head_off's own already-verified tests.
        let base = "日\0".as_bytes();
        assert_eq!(unsafe { mb_off_next(base, 0) }, 0);
    }

    #[test]
    fn mb_off_next_from_continuation_bytes_of_a_lone_char() {
        let _guard = option_vars_test_lock();
        // utf_head_off(base, 1) == 1, utf_head_off(base, 2) == 2
        // (already verified above); utfc_ptr2len(&base[0..]) == 3 (a
        // lone character, nothing composes with it).
        let base = "日\0".as_bytes();
        assert_eq!(unsafe { mb_off_next(base, 1) }, 2); // 3 - 1
        assert_eq!(unsafe { mb_off_next(base, 2) }, 1); // 3 - 2
    }

    #[test]
    fn mb_off_next_walks_through_a_combining_mark() {
        let _guard = option_vars_test_lock();
        // 'e' + COMBINING ACUTE ACCENT (2 bytes) + NUL - same fixture
        // as utf_head_off's own already-verified combining-mark test;
        // utfc_ptr2len(&base[0..]) == 3 (the whole composed cluster).
        let base = [0x65u8, 0xCC, 0x81, 0x00];
        assert_eq!(unsafe { mb_off_next(&base, 1) }, 2); // 3 - 1
        assert_eq!(unsafe { mb_off_next(&base, 2) }, 1); // 3 - 2
    }

    #[test]
    fn mb_off_next_illegal_lone_continuation_byte_is_zero() {
        let _guard = option_vars_test_lock();
        // Same fixture as utf_head_off's own "illegal" test - head_off
        // is 0, so mb_off_next short-circuits before ever calling
        // utfc_ptr2len.
        let base = [0x80u8, 0x00];
        assert_eq!(unsafe { mb_off_next(&base, 0) }, 0);
    }

    #[test]
    fn mb_ptr2char_adv_skips_a_following_combining_mark() {
        let _guard = option_vars_test_lock();
        // "e" + COMBINING ACUTE ACCENT (U+0301): composing-aware advance
        // treats the whole cluster as one unit.
        let s = "e\u{0301}x".as_bytes();
        let (c, len) = unsafe { mb_ptr2char_adv(s) };
        assert_eq!(c, 'e' as i32);
        assert_eq!(len, "e\u{0301}".len());
        assert_eq!(s[len], b'x');
    }

    #[test]
    fn mb_ptr2char_adv_plain_ascii() {
        let _guard = option_vars_test_lock();
        let (c, len) = unsafe { mb_ptr2char_adv(b"hi") };
        assert_eq!(c, 'h' as i32);
        assert_eq!(len, 1);
    }

    #[test]
    fn mb_cptr2char_adv_returns_a_combining_mark_as_its_own_character() {
        // Composing-unaware: advances by utf_ptr2len only, so the
        // combining mark is its OWN separate character on the next
        // call, unlike mb_ptr2char_adv's own composing-aware skip.
        let s = "e\u{0301}".as_bytes();
        let (c1, len1) = mb_cptr2char_adv(s);
        assert_eq!(c1, 'e' as i32);
        assert_eq!(len1, 1);
        let (c2, _len2) = mb_cptr2char_adv(&s[len1..]);
        assert_eq!(c2, 0x0301);
    }

    #[test]
    fn mb_string2cells_sums_ascii_widths() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { mb_string2cells(b"hello") }, 5);
    }

    #[test]
    fn mb_string2cells_counts_a_double_width_char_as_two() {
        let _guard = option_vars_test_lock();
        // CJK ideograph U+4E00 is double-width.
        assert_eq!(unsafe { mb_string2cells("一".as_bytes()) }, 2);
    }

    #[test]
    fn mb_string2cells_empty_string_is_zero() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { mb_string2cells(b"") }, 0);
    }

    #[test]
    fn mb_charlen_counts_ascii_bytes() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { mb_charlen(b"hello") }, 5);
    }

    #[test]
    fn mb_charlen_empty_is_zero() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { mb_charlen(b"") }, 0);
    }

    #[test]
    fn mb_charlen_groups_a_composing_mark_with_its_base() {
        let _guard = option_vars_test_lock();
        // "e" + COMBINING ACUTE ACCENT (U+0301) - one grouped character.
        let mut s = b"e".to_vec();
        s.extend_from_slice("\u{0301}".as_bytes());
        assert_eq!(unsafe { mb_charlen(&s) }, 1);
    }

    #[test]
    fn mb_charlen_counts_multibyte_characters() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { mb_charlen("一二三".as_bytes()) }, 3);
    }

    // --- mb_utflen / mb_utf_index_to_bytes ---

    /// "a" + U+1F600 (4 bytes, outside the BMP) + "b": 6 bytes,
    /// 3 codepoints, 4 UTF-16 code units.
    fn astral_sample() -> Vec<u8> {
        "a\u{1F600}b".as_bytes().to_vec()
    }

    #[test]
    fn mb_utflen_counts_codepoints_and_utf16_units() {
        // Cross-verified against real nvim: for this string strchars()
        // is 3 and strutf16len() is 4.
        let s = astral_sample();
        let (mut cp, mut cu) = (0usize, 0usize);
        mb_utflen(&s, s.len(), &mut cp, &mut cu);
        assert_eq!(cp, 3);
        assert_eq!(cu, 4);
    }

    #[test]
    fn mb_utflen_accumulates_rather_than_assigning() {
        // The original's out-parameters are `+=`, so a second chunk
        // adds to the running totals.
        let s = astral_sample();
        let (mut cp, mut cu) = (10usize, 20usize);
        mb_utflen(&s, s.len(), &mut cp, &mut cu);
        assert_eq!(cp, 13);
        assert_eq!(cu, 24);
    }

    #[test]
    fn mb_utflen_of_an_empty_range_adds_nothing() {
        let (mut cp, mut cu) = (0usize, 0usize);
        mb_utflen(b"", 0, &mut cp, &mut cu);
        assert_eq!((cp, cu), (0, 0));
    }

    #[test]
    fn mb_utf_index_to_bytes_by_codepoint() {
        // Cross-verified against real nvim's vim.str_byteindex, which
        // is this function's own caller: indices 0/1/2/3 give
        // 0/1/5/6 for this string.
        let s = astral_sample();
        let len = s.len();
        assert_eq!(mb_utf_index_to_bytes(&s, len, 0, false), 0);
        assert_eq!(mb_utf_index_to_bytes(&s, len, 1, false), 1);
        assert_eq!(mb_utf_index_to_bytes(&s, len, 2, false), 5);
        assert_eq!(mb_utf_index_to_bytes(&s, len, 3, false), 6);
    }

    #[test]
    fn mb_utf_index_to_bytes_past_the_end_is_minus_one() {
        // Cross-verified: vim.str_byteindex errors for this index,
        // which is how the -1 surfaces.
        let s = astral_sample();
        assert_eq!(mb_utf_index_to_bytes(&s, s.len(), 4, false), -1);
    }

    #[test]
    fn mb_utf_index_to_bytes_by_utf16_unit() {
        // Cross-verified against real nvim's
        // vim.str_byteindex(s, 'utf-16', i): 0/1/2/3/4 give
        // 0/1/5/5/6. Note 2 and 3 both land on 5 - an index pointing
        // into the middle of a surrogate pair resolves to the end of
        // the whole character.
        let s = astral_sample();
        let len = s.len();
        assert_eq!(mb_utf_index_to_bytes(&s, len, 0, true), 0);
        assert_eq!(mb_utf_index_to_bytes(&s, len, 1, true), 1);
        assert_eq!(mb_utf_index_to_bytes(&s, len, 2, true), 5);
        assert_eq!(mb_utf_index_to_bytes(&s, len, 3, true), 5);
        assert_eq!(mb_utf_index_to_bytes(&s, len, 4, true), 6);
        assert_eq!(mb_utf_index_to_bytes(&s, len, 5, true), -1);
    }

    #[test]
    fn mb_utf_index_to_bytes_on_pure_ascii_is_the_index_itself() {
        assert_eq!(mb_utf_index_to_bytes(b"hello", 5, 3, false), 3);
        assert_eq!(mb_utf_index_to_bytes(b"hello", 5, 5, false), 5);
        assert_eq!(mb_utf_index_to_bytes(b"hello", 5, 6, false), -1);
    }

    // --- mb_prevptr / mb_charlen_len / enc_skip ---

    #[test]
    fn mb_prevptr_steps_back_one_ascii_byte() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { mb_prevptr(b"hello", 3) }, 2);
        assert_eq!(unsafe { mb_prevptr(b"hello", 1) }, 0);
    }

    #[test]
    fn mb_prevptr_at_the_start_stays_put() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { mb_prevptr(b"hello", 0) }, 0);
    }

    #[test]
    fn mb_prevptr_steps_back_over_a_whole_multibyte_character() {
        let _guard = option_vars_test_lock();
        // "一二三" - three 3-byte characters, so from index 6 (the
        // start of the third) the previous one starts at 3.
        let s = "一二三".as_bytes();
        assert_eq!(unsafe { mb_prevptr(s, 6) }, 3);
        assert_eq!(unsafe { mb_prevptr(s, 3) }, 0);
    }

    #[test]
    fn mb_charlen_len_is_bounded_by_the_given_length() {
        let _guard = option_vars_test_lock();
        let s = "一二三".as_bytes();
        assert_eq!(unsafe { mb_charlen_len(s, 9) }, 3);
        // Only the first two characters are within 6 bytes.
        assert_eq!(unsafe { mb_charlen_len(s, 6) }, 2);
        assert_eq!(unsafe { mb_charlen_len(s, 0) }, 0);
    }

    #[test]
    fn mb_charlen_len_stops_at_a_nul_before_the_length() {
        let _guard = option_vars_test_lock();
        assert_eq!(unsafe { mb_charlen_len(b"ab\0cd", 5) }, 2);
    }

    #[test]
    fn enc_skip_strips_the_documented_prefixes_only() {
        assert_eq!(enc_skip(b"2byte-euc-jp"), 6);
        assert_eq!(enc_skip(b"8bit-latin1"), 5);
        assert_eq!(enc_skip(b"utf-8"), 0);
        assert_eq!(enc_skip(b""), 0);
        // A near-miss must not be stripped.
        assert_eq!(enc_skip(b"2byte"), 0);
    }

    #[test]
    fn mb_isalpha_true_for_ascii_letters() {
        let _guard = option_vars_test_lock();
        assert!(unsafe { mb_isalpha(i32::from(b'a')) });
        assert!(unsafe { mb_isalpha(i32::from(b'A')) });
    }

    #[test]
    fn mb_isalpha_false_for_digits_and_space() {
        let _guard = option_vars_test_lock();
        assert!(!unsafe { mb_isalpha(i32::from(b'5')) });
        assert!(!unsafe { mb_isalpha(i32::from(b' ')) });
    }

    #[test]
    fn utf_eat_space_true_at_range_boundaries() {
        assert!(utf_eat_space(0x2000));
        assert!(utf_eat_space(0x206f));
        assert!(utf_eat_space(0x3000)); // CJK symbols/punctuation start
    }

    #[test]
    fn utf_eat_space_false_outside_any_range() {
        assert!(!utf_eat_space(0x1fff));
        assert!(!utf_eat_space(0x2070));
        assert!(!utf_eat_space(i32::from(b'a')));
    }

    #[test]
    fn bol_prohibition_punct_table_is_sorted() {
        // binary_search requires this - verified programmatically,
        // not just by eye, matching the array's own real, mechanically
        // transcribed C source order.
        assert!(BOL_PROHIBITION_PUNCT.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(BOL_PROHIBITION_PUNCT.len(), 43);
    }

    #[test]
    fn eol_prohibition_punct_table_is_sorted() {
        assert!(EOL_PROHIBITION_PUNCT.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(EOL_PROHIBITION_PUNCT.len(), 19);
    }

    #[test]
    fn utf_allow_break_before_false_for_listed_punctuation() {
        assert!(!utf_allow_break_before(i32::from(b'!')));
        assert!(!utf_allow_break_before(0x2019));
        assert!(!utf_allow_break_before(0xff5d)); // last table entry
    }

    #[test]
    fn utf_allow_break_before_true_for_ordinary_characters() {
        assert!(utf_allow_break_before(i32::from(b'a')));
    }

    #[test]
    fn utf_allow_break_after_false_for_listed_punctuation() {
        assert!(!utf_allow_break_after(i32::from(b'(')));
        assert!(!utf_allow_break_after(0xff5b)); // last table entry
    }

    #[test]
    fn utf_allow_break_after_true_for_ordinary_characters() {
        assert!(utf_allow_break_after(i32::from(b'a')));
    }

    #[test]
    fn utf_allow_break_never_between_identical_em_dashes_or_ellipses() {
        assert!(!utf_allow_break(0x2014, 0x2014));
        assert!(!utf_allow_break(0x2026, 0x2026));
    }

    #[test]
    fn utf_allow_break_delegates_to_before_and_after_otherwise() {
        assert!(utf_allow_break(i32::from(b'a'), i32::from(b'b')));
        assert!(!utf_allow_break(i32::from(b'('), i32::from(b'a'))); // '(' forbids break after
        assert!(!utf_allow_break(i32::from(b'a'), i32::from(b'!'))); // '!' forbids break before
    }

    #[test]
    fn utf_valid_string_accepts_ascii_and_multibyte() {
        assert!(utf_valid_string(b"hello"));
        assert!(utf_valid_string("一二三".as_bytes()));
        assert!(utf_valid_string(b""));
    }

    #[test]
    fn utf_valid_string_rejects_an_invalid_lead_byte() {
        assert!(!utf_valid_string(&[0xff]));
    }

    #[test]
    fn utf_valid_string_rejects_an_incomplete_sequence() {
        // 0xE4 is a 3-byte lead byte, but nothing follows.
        assert!(!utf_valid_string(&[0xe4]));
    }

    #[test]
    fn utf_valid_string_rejects_an_invalid_trail_byte() {
        // 0xE4 (3-byte lead) followed by 2 bytes that don't look like
        // continuation bytes (0x00 & 0xc0 != 0x80).
        assert!(!utf_valid_string(&[0xe4, 0x00, 0x00]));
    }

    #[test]
    fn bomb_size_default_fenc_is_3_bytes() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_bomb: 1,
            b_p_bin: 0,
            ..Default::default()
        };
        assert_eq!(bomb_size(&buf), 3);
        buf.b_p_fenc = Some(b"utf-8".to_vec());
        assert_eq!(bomb_size(&buf), 3);
    }

    #[test]
    fn bomb_size_ucs2_and_utf16_are_2_bytes() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_bomb: 1,
            b_p_bin: 0,
            b_p_fenc: Some(b"ucs-2".to_vec()),
            ..Default::default()
        };
        assert_eq!(bomb_size(&buf), 2);
        buf.b_p_fenc = Some(b"utf-16".to_vec());
        assert_eq!(bomb_size(&buf), 2);
    }

    #[test]
    fn bomb_size_ucs4_is_4_bytes() {
        let buf = crate::buffer_defs::BufT {
            b_p_bomb: 1,
            b_p_bin: 0,
            b_p_fenc: Some(b"ucs-4".to_vec()),
            ..Default::default()
        };
        assert_eq!(bomb_size(&buf), 4);
    }

    #[test]
    fn bomb_size_zero_when_bomb_option_off() {
        let buf = crate::buffer_defs::BufT {
            b_p_bomb: 0,
            b_p_bin: 0,
            ..Default::default()
        };
        assert_eq!(bomb_size(&buf), 0);
    }

    #[test]
    fn bomb_size_zero_when_binary_mode() {
        let buf = crate::buffer_defs::BufT {
            b_p_bomb: 1,
            b_p_bin: 1,
            ..Default::default()
        };
        assert_eq!(bomb_size(&buf), 0);
    }

    #[test]
    fn bomb_size_zero_for_an_unrecognized_encoding() {
        let buf = crate::buffer_defs::BufT {
            b_p_bomb: 1,
            b_p_bin: 0,
            b_p_fenc: Some(b"latin1".to_vec()),
            ..Default::default()
        };
        assert_eq!(bomb_size(&buf), 0);
    }

    #[test]
    fn remove_bom_strips_a_leading_bom() {
        let mut s = vec![0xef, 0xbb, 0xbf, b'h', b'i'];
        remove_bom(&mut s);
        assert_eq!(s, b"hi");
    }

    #[test]
    fn remove_bom_no_bom_is_unchanged() {
        let mut s = b"hi".to_vec();
        remove_bom(&mut s);
        assert_eq!(s, b"hi");
    }

    #[test]
    fn remove_bom_leaves_a_lone_0xef_not_forming_a_real_bom() {
        let mut s = vec![0xef, b'x', b'y'];
        remove_bom(&mut s);
        assert_eq!(s, vec![0xef, b'x', b'y']);
    }

    #[test]
    fn remove_bom_removes_every_occurrence() {
        let mut s = vec![0xef, 0xbb, 0xbf, b'a', 0xef, 0xbb, 0xbf, b'b'];
        remove_bom(&mut s);
        assert_eq!(s, b"ab");
    }
}
