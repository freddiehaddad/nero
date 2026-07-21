//! Translated from `src/nvim/charset.c` (partial).
//!
//! `charset.c` is large (42KB) and most of it depends on `buf_T`/`g_chartab`
//! (character-class tables built from the `'iskeyword'`/`'isident'`/
//! `'isfname'`/`'isprint'` options - `option.c`, phase 4, and `buffer_defs.h`,
//! phase 3) or multi-byte width calculation (`mbyte.c`, phase 7). Only the
//! functions with no such dependency are translated in this pass: the
//! `skip*` family, the `getdigits*` family, `vim_isblankline`, `hex2nr`,
//! `hexhex2nr`.
//!
//! Deferred (real forward dependencies):
//! - `init_chartab`/`buf_init_chartab`/`check_isopt`: need `buf_T`
//!   (`buffer_defs.h`) and option parsing (`option.c`).
//! - `vim_isIDc`/`vim_iswordc`/`vim_iswordp`/`vim_isfilec`/`vim_isprintc`
//!   families: need `g_chartab` (built by the above).
//! - `rem_backslash`/`backslash_halve`/`backslash_halve_save`: need
//!   `vim_isfilec` (hence `g_chartab`).
//! - `trans_characters`/`transstr`/`str_foldcase`/`transchar`* family,
//!   `byte2cells`/`char2cells`/`ptr2cells`/`vim_strsize`/`vim_strnsize`:
//!   need multi-byte width/encoding functions (`mbyte.c`).
//! - `vim_str2nr`: produces `varnumber_T`/`uvarnumber_T` (eval, phase 5) and
//!   is substantial in its own right; deferred as a unit to translate
//!   alongside the eval engine rather than piecemeal.
//! - `skipbin`/`skiptobin`: trivial once `ascii_isbdigit` exists (it
//!   already does), but omitted from *this* pass purely to keep the batch
//!   focused - trivial to add alongside `vim_str2nr` later since they
//!   share the same "recognize bin/oct/hex numbers" theme.
//!
//! The `skip*`/`getdigits*` functions below return `usize` byte offsets
//! into the input slice (how far the "cursor" advanced) rather than a new
//! raw pointer, since Rust slices are addressed by index, not pointer
//! arithmetic - this is the direct structural translation of "pointer
//! advanced past X", not a behavior change.

use crate::ascii_defs::{ascii_isdigit, ascii_iswhite, ascii_isxdigit};

/// Skip over whitespace (`skipwhite`). Returns the offset of the first
/// non-whitespace byte (or `p.len()` if none).
pub fn skipwhite(p: &[u8]) -> usize {
    let mut i = 0;
    while i < p.len() && ascii_iswhite(p[i] as i32) {
        i += 1;
    }
    i
}

/// Like [`skipwhite`], but skip up to `len` bytes (`skipwhite_len`).
pub fn skipwhite_len(p: &[u8], len: usize) -> usize {
    let bound = p.len().min(len);
    let mut i = 0;
    while i < bound && ascii_iswhite(p[i] as i32) {
        i += 1;
    }
    i
}

/// Returns the number of whitespace columns (bytes) at the start of `p`
/// (`getwhitecols`). (`getwhitecols_curline`, which calls this on the
/// current line, is deferred - needs the cursor/buffer subsystem.)
#[inline]
pub fn getwhitecols(p: &[u8]) -> usize {
    skipwhite(p)
}

/// Skip over digits (`skipdigits`). Returns the offset of the first
/// non-digit byte.
pub fn skipdigits(q: &[u8]) -> usize {
    let mut i = 0;
    while i < q.len() && ascii_isdigit(q[i] as i32) {
        i += 1;
    }
    i
}

/// Skip over digits and hex characters (`skiphex`).
pub fn skiphex(q: &[u8]) -> usize {
    let mut i = 0;
    while i < q.len() && ascii_isxdigit(q[i] as i32) {
        i += 1;
    }
    i
}

/// Skip to the next digit, or the end of the slice (`skiptodigit`).
pub fn skiptodigit(q: &[u8]) -> usize {
    let mut i = 0;
    while i < q.len() && !ascii_isdigit(q[i] as i32) {
        i += 1;
    }
    i
}

/// Skip to the next hex character, or the end of the slice (`skiptohex`).
pub fn skiptohex(q: &[u8]) -> usize {
    let mut i = 0;
    while i < q.len() && !ascii_isxdigit(q[i] as i32) {
        i += 1;
    }
    i
}

/// Skip over text until `' '` or `'\t'` or the end of the slice
/// (`skiptowhite`).
pub fn skiptowhite(p: &[u8]) -> usize {
    let mut i = 0;
    while i < p.len() && p[i] != b' ' && p[i] != b'\t' {
        i += 1;
    }
    i
}

/// Like [`skiptowhite`], but also skips escaped characters
/// (`skiptowhite_esc`).
pub fn skiptowhite_esc(p: &[u8]) -> usize {
    let mut i = 0;
    while i < p.len() && p[i] != b' ' && p[i] != b'\t' {
        if (p[i] == b'\\' || p[i] == crate::ascii_defs::CTRL_V) && i + 1 < p.len() {
            i += 1;
        }
        i += 1;
    }
    i
}

/// Skip over text until `'\n'` or the end of the slice (`skip_to_newline`).
pub fn skip_to_newline(p: &[u8]) -> usize {
    p.iter().position(|&b| b == b'\n').unwrap_or(p.len())
}

/// Gets a number from the start of `s` (`try_getdigits`).
///
/// Returns `Some((value, bytes_consumed))` on success (matching the
/// original's `*pp` advance), or `None` on overflow (matching the
/// original's `false` return on `ERANGE` overflow to `INTMAX_MIN`/`MAX`).
/// A string with no leading digits at all parses as `(0, 0)`, matching
/// `strtoimax`'s behavior of returning 0 and not advancing the pointer.
pub fn try_getdigits(s: &[u8]) -> Option<(i64, usize)> {
    let neg = s.first() == Some(&b'-');
    let start = if neg { 1 } else { 0 };
    let digits_end = start + skipdigits(&s[start.min(s.len())..]);
    if digits_end == start {
        return Some((0, 0)); // no digits at all: strtoimax-style "0, no advance"
    }
    let text = std::str::from_utf8(&s[start..digits_end]).ok()?;
    let magnitude: i128 = text.parse().ok()?;
    let value = if neg { -magnitude } else { magnitude };
    if value < i64::MIN as i128 || value > i64::MAX as i128 {
        return None; // overflow
    }
    Some((value as i64, digits_end))
}

/// Gets a number from `s` and skips over it (`getdigits`).
///
/// Returns `(value, bytes_consumed)`; `def` on parse failure/overflow, and
/// panics if `strict` is true and parsing failed (matching the original's
/// `abort()`).
pub fn getdigits(s: &[u8], strict: bool, def: i64) -> (i64, usize) {
    match try_getdigits(s) {
        Some(result) => result,
        None => {
            assert!(!strict, "getdigits: overflow with strict=true");
            (def, 0)
        }
    }
}

/// Gets an `i32` number from `s` (`getdigits_int`).
pub fn getdigits_int(s: &[u8], strict: bool, def: i32) -> (i32, usize) {
    let (number, consumed) = getdigits(s, strict, def as i64);
    if !(i32::MIN as i64..=i32::MAX as i64).contains(&number) {
        if strict {
            panic!("getdigits_int: value out of i32 range");
        }
        return (def, consumed);
    }
    (number as i32, consumed)
}

/// Gets a `c_long`-sized number from `s` (`getdigits_long`).
pub fn getdigits_long(s: &[u8], strict: bool, def: std::os::raw::c_long) -> (std::os::raw::c_long, usize) {
    let (number, consumed) = getdigits(s, strict, def as i64);
    if !(std::os::raw::c_long::MIN as i64..=std::os::raw::c_long::MAX as i64).contains(&number) {
        if strict {
            panic!("getdigits_long: value out of c_long range");
        }
        return (def, consumed);
    }
    (number as std::os::raw::c_long, consumed)
}

/// Gets an `i32` number from `s` (`getdigits_int32`).
pub fn getdigits_int32(s: &[u8], strict: bool, def: i32) -> (i32, usize) {
    getdigits_int(s, strict, def)
}

/// Check that `lbuf` is empty or only contains blanks (`vim_isblankline`).
pub fn vim_isblankline(lbuf: &[u8]) -> bool {
    let i = skipwhite(lbuf);
    i == lbuf.len() || lbuf[i] == b'\r' || lbuf[i] == b'\n'
}

/// Converts a single hex digit character to its value (`hex2nr`). Only
/// meaningful for characters that are actually hex digits; like the
/// original, this doesn't validate its input (use [`hexhex2nr`] or check
/// with [`crate::ascii_defs::ascii_isxdigit`] first).
pub fn hex2nr(c: i32) -> i32 {
    if (b'a' as i32..=b'f' as i32).contains(&c) {
        return c - b'a' as i32 + 10;
    }
    if (b'A' as i32..=b'F' as i32).contains(&c) {
        return c - b'A' as i32 + 10;
    }
    c - b'0' as i32
}

/// Convert two hex characters to a byte (`hexhex2nr`).
///
/// Returns `-1` if either character is not a hex digit.
pub fn hexhex2nr(p: &[u8]) -> i32 {
    if p.len() < 2 || !ascii_isxdigit(p[0] as i32) || !ascii_isxdigit(p[1] as i32) {
        return -1;
    }
    (hex2nr(p[0] as i32) << 4) + hex2nr(p[1] as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipwhite_skips_spaces_and_tabs() {
        assert_eq!(skipwhite(b"  \tfoo"), 3);
        assert_eq!(skipwhite(b"foo"), 0);
        assert_eq!(skipwhite(b"   "), 3);
    }

    #[test]
    fn skipwhite_len_bounds_by_len() {
        assert_eq!(skipwhite_len(b"     foo", 3), 3);
        assert_eq!(skipwhite_len(b"  foo", 10), 2);
    }

    #[test]
    fn skipdigits_and_skiphex() {
        assert_eq!(skipdigits(b"123abc"), 3);
        assert_eq!(skiphex(b"1a2B3xyz"), 5);
    }

    #[test]
    fn skiptodigit_and_skiptohex() {
        assert_eq!(skiptodigit(b"abc123"), 3);
        assert_eq!(skiptodigit(b"abc"), 3); // NUL-equivalent: end of slice
        assert_eq!(skiptohex(b"zzza1"), 3);
    }

    #[test]
    fn skiptowhite_and_esc_variant() {
        assert_eq!(skiptowhite(b"foo bar"), 3);
        assert_eq!(skiptowhite(b"foobar"), 6);
        // "foo\\ bar baz" - the escaped space should not stop the scan.
        assert_eq!(skiptowhite_esc(b"foo\\ bar baz"), 8);
    }

    #[test]
    fn skip_to_newline_finds_lf_or_end() {
        assert_eq!(skip_to_newline(b"abc\ndef"), 3);
        assert_eq!(skip_to_newline(b"abcdef"), 6);
    }

    #[test]
    fn try_getdigits_parses_and_advances() {
        assert_eq!(try_getdigits(b"123abc"), Some((123, 3)));
        assert_eq!(try_getdigits(b"-45xyz"), Some((-45, 3)));
        assert_eq!(try_getdigits(b"abc"), Some((0, 0)));
    }

    #[test]
    fn try_getdigits_detects_overflow() {
        assert_eq!(try_getdigits(b"99999999999999999999999"), None);
    }

    #[test]
    fn getdigits_uses_default_on_failure() {
        let (v, consumed) = getdigits(b"99999999999999999999999", false, -1);
        assert_eq!(v, -1);
        assert_eq!(consumed, 0);
    }

    #[test]
    #[should_panic]
    fn getdigits_aborts_when_strict_and_overflowing() {
        getdigits(b"99999999999999999999999", true, -1);
    }

    #[test]
    fn vim_isblankline_detects_blank_or_whitespace_only_lines() {
        assert!(vim_isblankline(b""));
        assert!(vim_isblankline(b"   "));
        assert!(vim_isblankline(b"  \r"));
        assert!(!vim_isblankline(b"  x"));
    }

    #[test]
    fn hex2nr_and_hexhex2nr() {
        assert_eq!(hex2nr(b'a' as i32), 10);
        assert_eq!(hex2nr(b'F' as i32), 15);
        assert_eq!(hex2nr(b'5' as i32), 5);
        assert_eq!(hexhex2nr(b"1F"), 0x1F);
        assert_eq!(hexhex2nr(b"zz"), -1);
        assert_eq!(hexhex2nr(b"1"), -1); // too short
    }
}
