//! `src/nvim/grid.c` - the screen grid and its glyph (`schar_T`) encoding.
//!
//! Only the `schar_T` encoding half of this file is translated so far:
//! the interning cache plus the small conversion helpers around it,
//! which `optionstr.c`'s `set_chars_option` needs to turn a
//! `'listchars'`/`'fillchars'` character into a `schar_T`.
//!
//! A `schar_T` is a `u32` holding either the glyph's own UTF-8 bytes
//! inline (up to four of them, NUL-padded) or, for anything longer, a
//! marker byte plus an index into the process-wide glyph cache. The
//! discriminator is the FIRST byte in memory order: `0xFF` means
//! "interned". The original spells that out as two `#ifdef
//! ORDER_BIG_ENDIAN` branches (`0xFF + (idx << 8)` on little-endian,
//! `idx + (0xFF << 24)` on big-endian) precisely because both encode
//! the same memory-order layout; this translation keeps the same two
//! branches rather than collapsing them, so the bit patterns stay
//! identical on both endiannesses.
//!
//! Deferred (each genuinely blocked, not simply "not gotten to yet"):
//! everything that actually draws - `grid_line_start`/`grid_line_puts`/
//! `grid_put_linebuf`/`grid_scroll`/`grid_clear` and the `ScreenGrid`
//! allocation helpers - needs the UI event pipeline (`ui_line`,
//! `ui_call_grid_scroll`), the highlight attribute registry
//! (`hl_combine_attr`), and `decor`'s own providers, none translated.
//! `schar_cache_clear` likewise needs `decor_check_invalid_glyphs`.
//!
//! `line_do_arabic_shape` and its two private helpers landed once
//! `arabic.rs`'s own `arabic_shape` did. Note the argument order at
//! its `arabic_shape` call: the NEXT character is passed as that
//! function's `prev_c`, and the PREVIOUS one as its `next_c`. That is
//! not a mistake - the glyph buffer is in VISUAL order while
//! `arabic_shape` reasons in LOGICAL order, and Arabic runs
//! right-to-left, so the two are mirrored.
//!
//! `schar_get_adv` is deliberately NOT translated: the original's
//! `schar_get`/`schar_get_adv` split exists purely so a caller can
//! append into a larger scratch buffer without a second copy, and
//! [`schar_get`] here already returns the bytes owned.

use crate::map::Set;
use crate::types_defs::{MAX_SCHAR_SIZE, ScharT};

/// The process-wide glyph interning cache (`glyph_cache`).
///
/// The original is a `Set(glyph)` of NUL-terminated strings; here the
/// keys are plain byte vectors, since a `schar_T`'s own glyph may not
/// contain embedded NULs anyway.
pub static GLYPH_CACHE: std::sync::LazyLock<crate::globals::GlobalCell<Set<Vec<u8>>>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(Set::default()));

/// `schar_from_ascii(x)` - a single ASCII byte as a `schar_T`.
///
/// The original is a macro with two endianness branches, deliberately
/// written so a compile-time-constant argument stays compile-time
/// constant; `const fn` gives the same guarantee here.
#[must_use]
pub const fn schar_from_ascii(x: u8) -> ScharT {
    if cfg!(target_endian = "big") {
        (x as ScharT) << 24
    } else {
        x as ScharT
    }
}

/// Whether `sc` refers to the glyph cache rather than holding its own
/// bytes inline (`schar_high`).
#[must_use]
pub const fn schar_high(sc: ScharT) -> bool {
    if cfg!(target_endian = "big") {
        (sc & 0xFF00_0000) == 0xFF00_0000
    } else {
        (sc & 0xFF) == 0xFF
    }
}

/// The glyph-cache index carried by a `schar_high` value
/// (`schar_idx`).
#[must_use]
pub const fn schar_idx(sc: ScharT) -> u32 {
    if cfg!(target_endian = "big") {
        sc & 0x00FF_FFFF
    } else {
        sc >> 8
    }
}

/// `schar_from_buf(const char *buf, size_t len)`
///
/// `buf` need not be NUL terminated, but may not contain embedded
/// NULs. The original asserts `len < MAX_SCHAR_SIZE` (strictly less,
/// as a NUL still needs a byte); that assert is kept as a
/// `debug_assert!`, matching how the original's own `assert` compiles
/// out of release builds.
///
/// # Panics
/// In debug builds, if `buf` is `MAX_SCHAR_SIZE` bytes or longer, or
/// if the glyph cache has grown past its own `0xFFFFFF` index limit.
#[must_use]
pub fn schar_from_buf(buf: &[u8]) -> ScharT {
    debug_assert!(buf.len() < MAX_SCHAR_SIZE);
    if buf.len() <= 4 {
        // The original `memcpy`s straight into the `schar_T`, so the
        // bytes land in memory order and the remainder stays zero.
        let mut bytes = [0u8; 4];
        bytes[..buf.len()].copy_from_slice(buf);
        return ScharT::from_ne_bytes(bytes);
    }

    // SAFETY: a plain global-cell borrow, no aliasing hazard.
    let cache = unsafe { GLYPH_CACHE.get_mut() };
    let (idx, _status) = cache.put(buf.to_vec());
    let idx = u32::try_from(idx).expect("glyph cache index fits in u32");
    debug_assert!(idx < 0xFF_FFFF);
    if cfg!(target_endian = "big") {
        idx + (0xFF << 24)
    } else {
        0xFF + (idx << 8)
    }
}

/// `schar_from_char(int c)` - a single codepoint as a `schar_T`.
///
/// The original clamps anything at or above `0x200000` to `U+FFFD`
/// with a "this must NEVER happen, even if the file contained overlong
/// sequences" note, and encodes straight into the `schar_T`'s own four
/// bytes - a codepoint below that limit is at most four UTF-8 bytes,
/// so it always fits inline and never touches the glyph cache.
#[must_use]
pub fn schar_from_char(c: i32) -> ScharT {
    let c = if c >= 0x0020_0000 { 0xFFFD } else { c };
    let mut bytes = [0u8; 4];
    let len = crate::mbyte::utf_char2bytes(c, &mut bytes);
    let len = usize::try_from(len).unwrap_or(0).min(4);
    ScharT::from_ne_bytes({
        let mut b = [0u8; 4];
        b[..len].copy_from_slice(&bytes[..len]);
        b
    })
}

/// `schar_from_str(const char *str)`
///
/// `None` models the original's own `NULL` argument, which yields `0`
/// (`set_chars_option`'s own `chars_tab` entries rely on this for
/// their optional `def`/`fallback` strings).
#[must_use]
pub fn schar_from_str(str: Option<&[u8]>) -> ScharT {
    match str {
        None => 0,
        Some(s) => schar_from_buf(s),
    }
}

/// The number of bytes in `sc`'s own glyph (`schar_len`).
#[must_use]
pub fn schar_len(sc: ScharT) -> usize {
    if schar_high(sc) {
        // SAFETY: a plain global-cell borrow, no aliasing hazard.
        let cache = unsafe { GLYPH_CACHE.get_mut() };
        let idx = schar_idx(sc) as usize;
        debug_assert!(idx < cache.len());
        cache.keys().get(idx).map_or(0, Vec::len)
    } else {
        let bytes = sc.to_ne_bytes();
        bytes.iter().position(|&b| b == 0).unwrap_or(4)
    }
}

/// `sc`'s glyph bytes with a trailing NUL, as the original's own
/// `char sc_buf[MAX_SCHAR_SIZE]` scratch buffer always has.
///
/// `utf_ptr2char`/`utf_ptr2cells` both read a NUL-terminated buffer
/// and rely on that terminator to stop; [`schar_get`] returns just the
/// glyph, so callers that hand the bytes to those two must re-add it.
/// Without the terminator an empty glyph would index an empty slice.
fn schar_get_nul_terminated(sc: ScharT) -> Vec<u8> {
    let mut buf = schar_get(sc);
    buf.push(0);
    buf
}

/// How many screen cells `sc` occupies (`schar_cells`).
#[must_use]
pub fn schar_cells(sc: ScharT) -> i32 {
    // hot path
    let ascii = if cfg!(target_endian = "big") {
        (sc & 0x80FF_FFFF) == 0
    } else {
        sc < 0x80
    };
    if ascii {
        return 1;
    }

    // SAFETY: the buffer is NUL-terminated, which is exactly what
    // `utf_ptr2cells` needs to stop.
    unsafe { crate::mbyte::utf_ptr2cells(&schar_get_nul_terminated(sc)) }
}

/// `sc`'s first codepoint (`schar_get_first_codepoint`).
#[must_use]
pub fn schar_get_first_codepoint(sc: ScharT) -> i32 {
    crate::mbyte::utf_ptr2char(&schar_get_nul_terminated(sc))
}

/// `sc` as an ASCII byte, or NUL when it is not ASCII
/// (`schar_get_ascii`).
#[must_use]
pub const fn schar_get_ascii(sc: ScharT) -> u8 {
    if cfg!(target_endian = "big") {
        if (sc & 0x80FF_FFFF) == 0 {
            (sc >> 24) as u8
        } else {
            0
        }
    } else if sc < 0x80 {
        sc as u8
    } else {
        0
    }
}

/// `schar_get(char *buf_out, schar_T sc)` - the glyph's own bytes.
///
/// Returns them owned rather than filling a caller-provided buffer:
/// the original's `buf_out`/`schar_get_adv` split exists purely to let
/// a caller append into a larger scratch buffer without a second copy,
/// which is not a distinction Rust needs at this call depth. The
/// original also writes a trailing NUL, which is part of its C string
/// representation rather than part of the glyph.
#[must_use]
pub fn schar_get(sc: ScharT) -> Vec<u8> {
    if schar_high(sc) {
        // SAFETY: a plain global-cell borrow, no aliasing hazard.
        let cache = unsafe { GLYPH_CACHE.get_mut() };
        let idx = schar_idx(sc) as usize;
        debug_assert!(idx < cache.len());
        cache.keys().get(idx).cloned().unwrap_or_default()
    } else {
        let bytes = sc.to_ne_bytes();
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(4);
        bytes[..len].to_vec()
    }
}

/// The first raw UTF-8 byte of `sc` (`schar_get_first_byte`).
fn schar_get_first_byte(sc: ScharT) -> u8 {
    if schar_high(sc) {
        // SAFETY: a plain global-cell borrow, no aliasing hazard.
        let cache = unsafe { GLYPH_CACHE.get_mut() };
        let idx = schar_idx(sc) as usize;
        debug_assert!(idx < cache.len());
        cache.keys().get(idx).and_then(|k| k.first()).copied().unwrap_or(0)
    } else {
        sc.to_ne_bytes()[0]
    }
}

/// Whether `sc` starts in the Arabic Unicode block
/// (`schar_in_arabic_block`).
///
/// Tests only the FIRST UTF-8 byte: `0xD8`/`0xD9` are the two lead
/// bytes covering U+0600..U+07FF, so masking off the low bit of the
/// lead byte identifies both at once. This is a cheap pre-filter, not
/// an exact test - `line_do_arabic_shape` re-checks with
/// `ARABIC_CHAR` before shaping anything.
fn schar_in_arabic_block(sc: ScharT) -> bool {
    (schar_get_first_byte(sc) & 0xFE) == 0xD8
}

/// `ARABIC_CHAR(ch)` (`arabic.h`) - whether `ch` is in U+0600..U+06FF.
const fn arabic_char(ch: i32) -> bool {
    (ch & 0xFF00) == 0x0600
}

/// The first two codepoints of `sc`, or `0` when not available
/// (`schar_get_first_two_codepoints`).
fn schar_get_first_two_codepoints(sc: ScharT) -> (i32, i32) {
    let buf = schar_get_nul_terminated(sc);
    let c0 = crate::mbyte::utf_ptr2char(&buf);
    if c0 == 0 {
        return (0, 0);
    }
    let len = usize::try_from(crate::mbyte::utf_ptr2len(&buf)).unwrap_or(0);
    let c1 = if len < buf.len() {
        crate::mbyte::utf_ptr2char(&buf[len..])
    } else {
        0
    };
    (c0, c1)
}

/// Apply Arabic shaping to a whole line of glyphs in place
/// (`line_do_arabic_shape`).
///
/// The original takes a `schar_T *buf` plus a separate `cols` count; a
/// slice carries both, so a caller wanting to shape only part of a
/// line passes `&mut buf[..cols]`.
///
/// Note the argument order at the `arabic_shape` call: the NEXT
/// character is passed as that function's `prev_c`/`prev_c1`, and the
/// PREVIOUS one as its `next_c`. That is not a mistake - the glyph
/// buffer is in VISUAL order while `arabic_shape` reasons in LOGICAL
/// order, and Arabic runs right-to-left, so the two are mirrored.
/// Preserved exactly as the original has it.
///
/// # Safety
/// Forwarded from [`crate::arabic::arabic_shape`]'s own safety doc.
pub unsafe fn line_do_arabic_shape(buf: &mut [ScharT]) {
    let cols = buf.len();

    // quickly skip over non-arabic text
    let Some(start) = buf.iter().position(|&sc| schar_in_arabic_block(sc)) else {
        return;
    };

    let mut c0prev = 0i32;
    let (mut c0, mut c1) = schar_get_first_two_codepoints(buf[start]);

    for i in start..cols {
        let (c0next, c1next) = schar_get_first_two_codepoints(if i + 1 < cols {
            buf[i + 1]
        } else {
            0
        });

        if arabic_char(c0) {
            // SAFETY: forwarded from this function's own safety doc.
            let (c0new, c1new) = unsafe {
                // Visual order vs logical order - see this function's
                // own doc comment.
                crate::arabic::arabic_shape(c0, c1, c0next, c1next, c0prev)
            };

            if c0new != c0 || c1new != c1 {
                let sc_bytes = schar_get(buf[i]);
                let mut scbuf_new = [0u8; MAX_SCHAR_SIZE];
                let mut len = usize::try_from(crate::mbyte::utf_char2bytes(c0new, &mut scbuf_new))
                    .unwrap_or(0);
                if c1new != 0 {
                    len += usize::try_from(crate::mbyte::utf_char2bytes(
                        c1new,
                        &mut scbuf_new[len..],
                    ))
                    .unwrap_or(0);
                }

                let off = usize::try_from(
                    crate::mbyte::utf_char2len(c0)
                        + if c1 != 0 {
                            crate::mbyte::utf_char2len(c1)
                        } else {
                            0
                        },
                )
                .unwrap_or(0);
                let mut rest = sc_bytes.len().saturating_sub(off);

                if rest > 0 && rest + len + 1 > MAX_SCHAR_SIZE {
                    // Too bigly, discard one code-point. This is
                    // enough because c0 cannot grow by more than two
                    // bytes (base arabic to extended arabic).
                    let tail = &sc_bytes[off..off + rest];
                    let b = crate::mbyte::utf_cp_bounds(tail, rest - 1);
                    rest -= usize::try_from(b.begin_off).unwrap_or(0) + 1;
                }

                scbuf_new[len..len + rest].copy_from_slice(&sc_bytes[off..off + rest]);
                buf[i] = schar_from_buf(&scbuf_new[..len + rest]);
            }
        }

        c0prev = c0;
        c0 = c0next;
        c1 = c1next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The glyph cache is process-wide, so any test touching it must
    /// hold the same lock every other global-state test holds.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        crate::globals::global_state_test_lock()
    }

    #[test]
    fn schar_from_ascii_round_trips_through_schar_get() {
        let _l = lock();
        for c in *b"a ~@" {
            let sc = schar_from_ascii(c);
            assert!(!schar_high(sc));
            assert_eq!(schar_get(sc), vec![c]);
        }
    }

    #[test]
    fn schar_from_buf_stores_short_glyphs_inline() {
        let _l = lock();
        // One, two and three byte UTF-8 all fit in the four inline
        // bytes, so none of them touch the cache.
        for s in ["x", "é", "─", "│", "abcd"] {
            let sc = schar_from_buf(s.as_bytes());
            assert!(!schar_high(sc), "{s} should be inline");
            assert_eq!(schar_get(sc), s.as_bytes());
        }
    }

    #[test]
    fn schar_from_buf_interns_long_glyphs_and_reuses_the_entry() {
        let _l = lock();
        // Five bytes is one past the inline limit.
        let long = "a\u{0301}\u{0302}".as_bytes();
        assert!(long.len() > 4);
        let sc = schar_from_buf(long);
        assert!(schar_high(sc));
        assert_eq!(schar_get(sc), long);
        // Interning the same glyph again yields the same value.
        assert_eq!(schar_from_buf(long), sc);
    }

    #[test]
    fn schar_from_buf_gives_distinct_values_to_distinct_long_glyphs() {
        let _l = lock();
        let a = "a\u{0301}\u{0302}".as_bytes();
        let b = "b\u{0301}\u{0302}".as_bytes();
        let sa = schar_from_buf(a);
        let sb = schar_from_buf(b);
        assert_ne!(sa, sb);
        assert_eq!(schar_get(sa), a);
        assert_eq!(schar_get(sb), b);
    }

    #[test]
    fn schar_from_char_encodes_a_codepoint_inline() {
        let _l = lock();
        for (c, s) in [(0x61, "a"), (0xE9, "é"), (0x2500, "─"), (0x1F600, "\u{1F600}")] {
            assert_eq!(schar_get(schar_from_char(c)), s.as_bytes(), "U+{c:04X}");
        }
    }

    #[test]
    fn schar_from_char_clamps_an_out_of_range_codepoint_to_replacement() {
        let _l = lock();
        // The original clamps anything >= 0x200000 to U+FFFD.
        assert_eq!(
            schar_from_char(0x0020_0000),
            schar_from_char(0xFFFD),
            "at the limit"
        );
        assert_eq!(schar_get(schar_from_char(0x0030_0000)), "\u{FFFD}".as_bytes());
        // One below the limit still encodes as itself.
        assert_ne!(schar_from_char(0x001F_FFFF), schar_from_char(0xFFFD));
    }

    #[test]
    fn schar_len_and_cells_agree_for_inline_and_interned_glyphs() {
        let _l = lock();
        // (glyph, expected byte length, expected screen cells)
        for (s, len, cells) in [
            ("a", 1usize, 1i32),
            ("é", 2, 1),
            ("─", 3, 1),
            ("一", 3, 2),
            ("a\u{0301}\u{0302}", 5, 1),
        ] {
            let sc = schar_from_buf(s.as_bytes());
            assert_eq!(schar_len(sc), len, "len of {s}");
            assert_eq!(schar_cells(sc), cells, "cells of {s}");
        }
    }

    #[test]
    fn schar_cells_takes_the_ascii_fast_path() {
        let _l = lock();
        for c in *b"a ~@" {
            assert_eq!(schar_cells(schar_from_ascii(c)), 1);
        }
    }

    #[test]
    fn schar_get_ascii_returns_the_byte_only_for_ascii() {
        let _l = lock();
        for c in *b"a ~@" {
            assert_eq!(schar_get_ascii(schar_from_ascii(c)), c);
        }
        // A non-ASCII glyph yields NUL instead.
        assert_eq!(schar_get_ascii(schar_from_buf("é".as_bytes())), 0);
        assert_eq!(schar_get_ascii(schar_from_buf("─".as_bytes())), 0);
    }

    #[test]
    fn schar_get_first_codepoint_reads_through_the_cache() {
        let _l = lock();
        assert_eq!(schar_get_first_codepoint(schar_from_ascii(b'a')), 0x61);
        assert_eq!(schar_get_first_codepoint(schar_from_buf("─".as_bytes())), 0x2500);
        // An interned (>4 byte) glyph reports its BASE codepoint.
        let long = schar_from_buf("a\u{0301}\u{0302}".as_bytes());
        assert!(schar_high(long));
        assert_eq!(schar_get_first_codepoint(long), 0x61);
    }

    #[test]
    fn schar_accessors_handle_the_empty_glyph() {
        let _l = lock();
        // Zero decodes to no bytes at all. The NUL terminator that
        // `schar_get_nul_terminated` re-adds is what keeps
        // `utf_ptr2char` from indexing an empty slice here.
        assert_eq!(schar_len(0), 0);
        assert_eq!(schar_get_first_codepoint(0), 0);
        assert_eq!(schar_get_ascii(0), 0);
        assert_eq!(schar_cells(0), 1);
    }

    #[test]
    fn line_do_arabic_shape_leaves_a_non_arabic_line_alone() {
        let _l = lock();
        let mut buf: Vec<ScharT> = "hello"
            .chars()
            .map(|c| schar_from_char(c as i32))
            .collect();
        let before = buf.clone();
        unsafe { line_do_arabic_shape(&mut buf) };
        assert_eq!(buf, before);
    }

    #[test]
    fn line_do_arabic_shape_leaves_an_empty_line_alone() {
        let _l = lock();
        let mut buf: Vec<ScharT> = Vec::new();
        unsafe { line_do_arabic_shape(&mut buf) };
        assert!(buf.is_empty());
    }

    #[test]
    fn schar_in_arabic_block_matches_only_the_arabic_lead_bytes() {
        let _l = lock();
        // U+0600..U+07FF encode with a 0xD8 or 0xD9 lead byte.
        assert!(schar_in_arabic_block(schar_from_char(0x0644))); // LAM
        assert!(schar_in_arabic_block(schar_from_char(0x0627))); // ALEF
        // Latin, and a box-drawing char (0xE2 lead), are not.
        assert!(!schar_in_arabic_block(schar_from_ascii(b'a')));
        assert!(!schar_in_arabic_block(schar_from_char(0x2500)));
    }

    #[test]
    fn schar_get_first_two_codepoints_splits_a_base_and_its_composing_char() {
        let _l = lock();
        // A bare character has no second codepoint.
        assert_eq!(
            schar_get_first_two_codepoints(schar_from_char(0x0644)),
            (0x0644, 0)
        );
        // A base plus one composing char reports both.
        let sc = schar_from_buf("\u{0644}\u{0627}".as_bytes());
        assert_eq!(schar_get_first_two_codepoints(sc), (0x0644, 0x0627));
        // The empty glyph reports nothing at all.
        assert_eq!(schar_get_first_two_codepoints(0), (0, 0));
    }

    #[test]
    fn line_do_arabic_shape_joins_a_run_of_lam() {
        let _l = lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let (prev_arshape, prev_tbidi) = (opts.p_arshape, opts.p_tbidi);
        opts.p_arshape = 1;
        opts.p_tbidi = 0;

        // Three LAMs in a row. Hand-traced against a_LAM's own ACHARS
        // row (isolated fedd, initial fedf, medial fee0, final fede)
        // and this function's visual-vs-logical argument mirroring:
        // the FIRST glyph has no visual predecessor but does have a
        // successor, which `arabic_shape` sees as its `prev_c`, so it
        // shapes as a FINAL form; the middle one joins both ways and
        // is MEDIAL; the last has only a visual predecessor, seen as
        // `next_c`, so it is INITIAL.
        let lam = schar_from_char(0x0644);
        let mut buf = vec![lam; 3];
        unsafe { line_do_arabic_shape(&mut buf) };

        assert_eq!(schar_get_first_two_codepoints(buf[0]).0, 0xfede);
        assert_eq!(schar_get_first_two_codepoints(buf[1]).0, 0xfee0);
        assert_eq!(schar_get_first_two_codepoints(buf[2]).0, 0xfedf);

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_arshape = prev_arshape;
        opts.p_tbidi = prev_tbidi;
    }

    #[test]
    fn line_do_arabic_shape_gates_only_the_ligature_on_arabicshape() {
        let _l = lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let (prev_arshape, prev_tbidi) = (opts.p_arshape, opts.p_tbidi);

        // A LAM carrying a composing ALEF. With 'arabicshape' ON the
        // pair collapses into the LAM-ALEF ligature and the composing
        // char is consumed.
        let lam_alef = schar_from_buf("\u{0644}\u{0627}".as_bytes());
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_arshape = 1;
        opts.p_tbidi = 0;
        let mut buf = vec![lam_alef];
        unsafe { line_do_arabic_shape(&mut buf) };
        assert_eq!(schar_get_first_two_codepoints(buf[0]), (0xfefb, 0));

        // With it OFF the ligature does not form - but the ordinary
        // joining forms still apply, so the LAM becomes its ISOLATED
        // presentation form and keeps its composing char. Only
        // `arabic_combine` consults 'arabicshape'.
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_arshape = 0;
        let mut buf = vec![lam_alef];
        unsafe { line_do_arabic_shape(&mut buf) };
        assert_eq!(schar_get_first_two_codepoints(buf[0]), (0xfedd, 0x0627));

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_arshape = prev_arshape;
        opts.p_tbidi = prev_tbidi;
    }

    #[test]
    fn schar_from_str_maps_none_to_zero() {
        let _l = lock();
        assert_eq!(schar_from_str(None), 0);
        assert_eq!(schar_from_str(Some(b"-")), schar_from_ascii(b'-'));
    }

    #[test]
    fn schar_from_buf_of_an_empty_slice_is_zero() {
        let _l = lock();
        // Zero is also what an all-NUL inline value decodes back to.
        assert_eq!(schar_from_buf(b""), 0);
        assert_eq!(schar_get(0), Vec::<u8>::new());
    }

    #[test]
    fn schar_high_discriminates_on_the_first_byte_in_memory_order() {
        // The two endianness branches must agree that the marker is
        // the first byte in memory order.
        let marked = if cfg!(target_endian = "big") {
            0xFF << 24
        } else {
            0xFFu32
        };
        assert!(schar_high(marked));
        assert!(!schar_high(schar_from_ascii(b'a')));
        assert_eq!(schar_idx(marked), 0);
    }
}
