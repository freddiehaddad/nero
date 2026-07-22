//! Translated from `src/nvim/charset.c` (partial).
//!
//! `charset.c` is large (42KB) and most of it depends on `buf_T`/`g_chartab`
//! (character-class tables built from the `'iskeyword'`/`'isident'`/
//! `'isfname'`/`'isprint'` options - `option.c`, phase 4, and `buffer_defs.h`,
//! phase 3) or multi-byte width calculation (`mbyte.c`, phase 7). Translated
//! in this pass (no such dependency, or only a documented default-table
//! approximation of `g_chartab` - see below): the `skip*` family, the
//! `getdigits*` family, `vim_isblankline`, `hex2nr`, `hexhex2nr`,
//! `vim_isprintc`, `char2cells`, `ptr2cells`; and now that `mbyte.c`'s
//! `utfc_ptr2len` exists too: `vim_strsize`/`vim_strnsize` (screen-cell
//! width of a whole string, counting TABs as two cells).
//!
//! `vim_isprintc`/`char2cells` need `g_chartab`, which isn't translated
//! (needs `buf_T`/option parsing), but their *default* (pre-`'isprint'`-
//! customization) values follow a simple, fixed rule directly verified
//! against `buf_init_chartab`'s own global-reset branch: control
//! characters unprintable/2-cells, printable ASCII and Latin-1
//! printable/1-cell. This crate implements exactly that fixed rule
//! rather than the general `g_chartab` machinery - correct for every
//! real session that hasn't customized `'isprint'` (the common case),
//! documented as a simplification on each function rather than
//! pretending the general mechanism exists. `char2cells`'s special-key
//! (`IS_SPECIAL`/negative `c`) branch is deferred separately (needs
//! `keycodes.h`, no current caller passes such a value).
//!
//! Deferred (real forward dependencies):
//! - `init_chartab`/`buf_init_chartab`/`check_isopt`: need `buf_T`
//!   (`buffer_defs.h`) and option parsing (`option.c`).
//! - `vim_isIDc`/`vim_iswordc`/`vim_iswordp`/`vim_isfilec` families: need
//!   the real `g_chartab` (built by the above) - unlike `vim_isprintc`
//!   above, these don't have a simple fixed-default-rule shortcut
//!   (`'iskeyword'`'s default already varies by `'encoding'`).
//! - `rem_backslash`/`backslash_halve`/`backslash_halve_save`: need
//!   `vim_isfilec` (hence the real `g_chartab`).
//! - `byte2cells`: needs the real `g_chartab` directly (`g_chartab[b] &
//!   CT_CELL_MASK`) - unlike `char2cells`/`ptr2cells`, its `>= 0x80`
//!   case returns 0 unconditionally rather than delegating to
//!   `utf_char2cells`, so there's no analogous fixed-default-rule
//!   shortcut available for the `< 0x80` case either (it's meant to be
//!   read consistently with the real table, not approximated).
//! - `trans_characters`/`transstr`/`transstr_len`/`transstr_buf`/
//!   `str_foldcase`/`transchar`* family: need `byte2cells` (above) and
//!   `transchar_byte`/`transchar_hex` (not yet checked) - re-examine
//!   once `byte2cells` itself is unblocked.
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

/// Check that `c` is a printable character (`vim_isprintc`).
///
/// This uses `g_chartab`'s own DEFAULT initialization rule
/// (`buf_init_chartab`'s unconditional, global-reset branch) rather
/// than the real, possibly-`'isprint'`-customized `g_chartab` itself
/// (not yet translated - needs `buf_T`/option parsing): control
/// characters (0x00-0x1F, 0x7F-0x9F) are unprintable, printable ASCII
/// (0x20-0x7E) and Latin-1 (0xA0-0xFF) are printable. This is exactly
/// the behavior of any real session that hasn't customized
/// `'isprint'` (a rare, non-default configuration), not a made-up
/// approximation. For `c >= 0x100`, delegates to
/// [`crate::mbyte::utf_printable`] (fully general, no option
/// dependency at all).
#[must_use]
pub fn vim_isprintc(c: i32) -> bool {
    if c <= 0 {
        return false;
    }
    if c >= 0x100 {
        return crate::mbyte::utf_printable(c);
    }
    (0x20..=0x7E).contains(&c) || c >= 0xA0
}

/// Return number of display cells occupied by character `c`
/// (`char2cells`).
///
/// `c` can be a special key (negative number) in the original, in
/// which case 3 or 4 is returned (via `IS_SPECIAL`/`K_SECOND`,
/// `keycodes.h`, not yet translated) - deferred, documented gap: no
/// caller in this crate yet passes an encoded special-key value here.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (for `c >= 0x80`, via
/// [`crate::mbyte::utf_char2cells`]; and for `'display'`'s `"uhex"`
/// flag on the control-character path).
#[must_use]
pub unsafe fn char2cells(c: i32) -> i32 {
    if c >= 0x80 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::mbyte::utf_char2cells(c) };
    }
    if (0x20..=0x7E).contains(&c) {
        return 1;
    }
    // g_chartab's own DEFAULT initialization rule for the remaining
    // (control/DEL) range: 2 cells normally (displayed as e.g. "^I"),
    // 4 if 'display' contains "uhex".
    // SAFETY: forwarded from this function's own safety doc.
    let dy_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.dy_flags;
    if dy_flags & crate::option_vars::opt_dy_flag::UHEX != 0 {
        4
    } else {
        2
    }
}

/// Return number of display cells occupied by character at `p`
/// (`ptr2cells`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// [`crate::mbyte::utf_ptr2cells`]/[`char2cells`]).
#[must_use]
pub unsafe fn ptr2cells(p: &[u8]) -> i32 {
    let Some(&b0) = p.first() else {
        return 1;
    };
    if b0 >= 0x80 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::mbyte::utf_ptr2cells(p) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { char2cells(i32::from(b0)) }
}

/// Return the number of character cells string `s` will take on the
/// screen, counting TABs as two characters: "^I" (`vim_strsize`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via [`vim_strnsize`]).
#[must_use]
pub unsafe fn vim_strsize(s: &[u8]) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { vim_strnsize(s, crate::pos_defs::MAXCOL) }
}

/// Return the number of character cells the first `len` bytes of `s`
/// will take on the screen, counting TABs as two characters: "^I"
/// (`vim_strnsize`). Stops early at a NUL byte, same as the original's
/// own NUL-terminated-string handling.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// [`crate::mbyte::utfc_ptr2len`]/[`ptr2cells`]).
#[must_use]
pub unsafe fn vim_strnsize(s: &[u8], len: i32) -> i32 {
    let mut size = 0i32;
    let mut len = len;
    let mut pos = 0usize;
    loop {
        // Matches the original's `while (*s != NUL && --len >= 0)`
        // exactly, including its short-circuit evaluation order: `len`
        // is only ever decremented once we know there's a real byte
        // left to process.
        if pos >= s.len() || s[pos] == 0 {
            break;
        }
        len -= 1;
        if len < 0 {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let l = unsafe { crate::mbyte::utfc_ptr2len(&s[pos..]) };
        // SAFETY: forwarded from this function's own safety doc.
        size += unsafe { ptr2cells(&s[pos..]) };
        pos += l as usize;
        len -= l - 1;
    }
    size
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

    #[test]
    fn vim_isprintc_matches_g_chartab_default_rule_below_0x100() {
        assert!(!vim_isprintc(0)); // NUL
        assert!(!vim_isprintc(-1));
        assert!(!vim_isprintc(0x1f)); // control char
        assert!(vim_isprintc(i32::from(b' '))); // start of printable ASCII
        assert!(vim_isprintc(i32::from(b'~'))); // end of printable ASCII
        assert!(!vim_isprintc(0x7f)); // DEL
        assert!(!vim_isprintc(0x9f)); // still in the unprintable gap
        assert!(vim_isprintc(0xa0)); // start of printable Latin-1
        assert!(vim_isprintc(0xff)); // end of printable Latin-1
    }

    #[test]
    fn vim_isprintc_delegates_to_utf_printable_at_and_above_0x100() {
        assert!(vim_isprintc(0x0100)); // ordinary Latin Extended-A
        assert!(!vim_isprintc(0x200b)); // in utf_printable's nonprint table
    }

    #[test]
    fn char2cells_printable_ascii_is_one_and_control_is_two() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { char2cells(i32::from(b'a')) }, 1);
        assert_eq!(unsafe { char2cells(0x01) }, 2); // control char, no uhex
    }

    #[test]
    fn char2cells_control_char_is_four_with_uhex() {
        let _guard = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.dy_flags;
        opts.dy_flags = crate::option_vars::opt_dy_flag::UHEX;

        assert_eq!(unsafe { char2cells(0x01) }, 4);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.dy_flags = prev;
    }

    #[test]
    fn char2cells_delegates_to_utf_char2cells_above_0x80() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { char2cells(0x4e00) }, unsafe {
            crate::mbyte::utf_char2cells(0x4e00)
        });
    }

    #[test]
    fn ptr2cells_ascii_matches_char2cells() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { ptr2cells(b"a") }, unsafe { char2cells(i32::from(b'a')) });
    }

    #[test]
    fn ptr2cells_empty_slice_is_one() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { ptr2cells(b"") }, 1);
    }

    #[test]
    fn ptr2cells_multibyte_matches_utf_ptr2cells() {
        let _guard = crate::globals::global_state_test_lock();
        let cjk = "一".as_bytes();
        assert_eq!(unsafe { ptr2cells(cjk) }, unsafe { crate::mbyte::utf_ptr2cells(cjk) });
    }

    #[test]
    fn vim_strsize_counts_ascii_as_one_cell_each() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsize(b"hello") }, 5);
    }

    #[test]
    fn vim_strsize_counts_tab_as_two_cells() {
        let _guard = crate::globals::global_state_test_lock();
        // TAB (control char, no 'isprint' customization) is 2 cells
        // per this crate's own documented default-g_chartab rule.
        assert_eq!(unsafe { vim_strsize(b"a\tb") }, 1 + 2 + 1);
    }

    #[test]
    fn vim_strsize_counts_double_wide_cjk_as_two_cells() {
        let _guard = crate::globals::global_state_test_lock();
        // "一本" - two East Asian Wide characters, 2 cells each.
        assert_eq!(unsafe { vim_strsize("一本".as_bytes()) }, 4);
    }

    #[test]
    fn vim_strsize_stops_at_the_trailing_nul() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsize(b"ab\0cd") }, 2);
    }

    #[test]
    fn vim_strnsize_stops_early_at_the_byte_bound() {
        let _guard = crate::globals::global_state_test_lock();
        // len=3 means "process at most 3 bytes" - stops after "abc",
        // never reaching "de".
        assert_eq!(unsafe { vim_strnsize(b"abcde", 3) }, 3);
    }

    #[test]
    fn vim_strnsize_len_bound_can_split_a_multibyte_character() {
        let _guard = crate::globals::global_state_test_lock();
        // "一" is 3 bytes/2 cells; a len bound of 2 still counts it in
        // full once entered (matches the original's own `len -= l - 1`
        // bookkeeping, which only checks the byte budget *before*
        // consuming each whole character, never mid-character).
        let bytes = "一x".as_bytes();
        assert_eq!(unsafe { vim_strnsize(bytes, 2) }, 2);
    }
}
