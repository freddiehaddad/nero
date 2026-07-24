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
//! width of a whole string, counting TABs as two cells); `byte2cells`
//! (the single-byte sibling of `char2cells`); `nr2hex`/`transchar_hex`
//! (hex-escape formatting for non-printable/illegal characters);
//! `charset.h`'s `vim_isbreak` (translated proactively for its real
//! caller, `plines.c`'s `charsize_regular` - since translated, and
//! `plines.c` itself is now fully complete; `charset.h` has no
//! dedicated module of its own in this crate, same treatment as
//! `buffer.h`'s `buf_meta_total` in `buffer.rs`).
//!
//! `vim_isprintc`/`char2cells`/`byte2cells` need `g_chartab`, which isn't
//! translated (needs `buf_T`/option parsing), but their *default* (pre-
//! `'isprint'`-customization) values follow a simple, fixed rule directly
//! verified against `buf_init_chartab`'s own global-reset branch: control
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
//! - `transchar`/`transchar_buf`/`transchar_byte`/`transchar_byte_buf`/
//!   `transchar_nonprint` are now translated too (this pass), returning
//!   an owned `Vec<u8>` (including trailing NUL) instead of a pointer
//!   into the original's shared static `transchar_charbuf` - this
//!   crate's usual preference for owned return values over the
//!   original's shared-mutable-scratch-buffer memory model when
//!   nothing yet depends on pointer stability across calls. The
//!   `IS_SPECIAL`/`K_SECOND` special-key prefix (`keycodes.h`, not yet
//!   translated) is NOT handled - no current caller passes an encoded
//!   special-key value. The original's `!chartab_initialized && (c >=
//!   ' ' && c <= '~')` disjunct is also omitted from `transchar_buf`:
//!   it's a pure subset of what `vim_isprintc` itself already covers
//!   for `c <= 0xFF`, and `chartab_initialized` can never become `true`
//!   in this crate anyway (nothing sets it).
//! - `trans_characters`/`transstr_len`/`transstr_buf`: `transstr`
//!   itself is now translated - see its own doc comment for why
//!   `transstr_len` isn't (no real caller, superseded by `Vec`'s
//!   dynamic growth) and why `transstr_buf`'s own length-truncating
//!   variant is deferred separately (needed by `drawline.c`/
//!   `statusline.c`, neither yet translated). `str_foldcase` is now
//!   translated too (its unlimited/`buf == NULL` case; the
//!   fixed-buffer-truncating variant, used by `syntax.c`, is deferred
//!   the same way as `transstr_buf`'s). `trans_characters` (in-place,
//!   fixed-buffer-with-room-budget mutation of a caller's own buffer)
//!   remains deferred - re-examine once a real caller surfaces.
//! - `vim_str2nr` is now translated too, now that the eval engine's
//!   `VarnumberT`/`UvarnumberT` exist - the goto-based state machine in
//!   the original (converging hex/octal/binary/decimal prefix
//!   detection onto one of 4 shared digit-accumulation blocks) becomes
//!   a `Radix` enum (`base`/`is_digit`/`digit_value` per radix) plus
//!   structured `if`/`else` prefix detection that computes "which
//!   radix, how many prefix bytes to skip" before falling into one
//!   shared parsing loop - the same observable control flow, restated
//!   without `goto`. `skipbin`/`skiptobin` (the `skip*` family's own
//!   binary-digit members, trivial once `ascii_isbdigit` existed) are
//!   translated alongside it too, completing that theme.
//!
//! The `skip*`/`getdigits*` functions below return `usize` byte offsets
//! into the input slice (how far the "cursor" advanced) rather than a new
//! raw pointer, since Rust slices are addressed by index, not pointer
//! arithmetic - this is the direct structural translation of "pointer
//! advanced past X", not a behavior change.

use crate::ascii_defs::{ascii_isbdigit, ascii_isdigit, ascii_isodigit, ascii_iswhite, ascii_isxdigit};
use crate::eval::typval_defs::{UvarnumberT, VarnumberT, UVARNUMBER_MAX, VARNUMBER_MAX, VARNUMBER_MIN};

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

/// Skip over binary digits (`skipbin`).
pub fn skipbin(q: &[u8]) -> usize {
    let mut i = 0;
    while i < q.len() && ascii_isbdigit(q[i] as i32) {
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

/// Skip to the next binary character, or the end of the slice
/// (`skiptobin`).
pub fn skiptobin(q: &[u8]) -> usize {
    let mut i = 0;
    while i < q.len() && !ascii_isbdigit(q[i] as i32) {
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

/// Allow binary numbers (`STR2NR_BIN`).
pub const STR2NR_BIN: i32 = 1 << 0;
/// Allow octal numbers (`STR2NR_OCT`).
pub const STR2NR_OCT: i32 = 1 << 1;
/// Allow hexadecimal numbers (`STR2NR_HEX`).
pub const STR2NR_HEX: i32 = 1 << 2;
/// Octal with prefix `"0o"`: `0o777` (`STR2NR_OOCT`).
pub const STR2NR_OOCT: i32 = 1 << 3;
/// Ignore embedded single quotes (`STR2NR_QUOTE`).
pub const STR2NR_QUOTE: i32 = 1 << 4;
/// Always assume bin/oct/hex (`STR2NR_FORCE`).
pub const STR2NR_FORCE: i32 = 1 << 7;
/// Recognize all radixes (`STR2NR_ALL`).
pub const STR2NR_ALL: i32 = STR2NR_BIN | STR2NR_OCT | STR2NR_HEX | STR2NR_OOCT;
/// All radixes except plain (un-prefixed) octal (`STR2NR_NO_OCT`).
pub const STR2NR_NO_OCT: i32 = STR2NR_BIN | STR2NR_HEX | STR2NR_OOCT;

/// Which radix [`vim_str2nr`]'s shared digit-accumulation loop is
/// currently parsing in - replaces the original's `goto
/// vim_str2nr_bin`/`_oct`/`_dec`/`_hex` convergence onto one of 4
/// near-identical `PARSE_NUMBER` macro expansions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Radix {
    Bin,
    Oct,
    Dec,
    Hex,
}

impl Radix {
    fn base(self) -> UvarnumberT {
        match self {
            Radix::Bin => 2,
            Radix::Oct => 8,
            Radix::Dec => 10,
            Radix::Hex => 16,
        }
    }

    fn is_digit(self, c: u8) -> bool {
        match self {
            Radix::Bin => c == b'0' || c == b'1',
            Radix::Oct => ascii_isodigit(c as i32),
            Radix::Dec => ascii_isdigit(c as i32),
            Radix::Hex => ascii_isxdigit(c as i32),
        }
    }

    fn digit_value(self, c: u8) -> UvarnumberT {
        match self {
            Radix::Hex => hex2nr(c as i32) as UvarnumberT,
            _ => UvarnumberT::from(c - b'0'),
        }
    }
}

/// Convert a string into a signed and/or unsigned number, taking care
/// of hexadecimal, octal, and binary numbers. Accepts a `-` sign
/// (`vim_str2nr`).
///
/// If `prep` is given, returns a flag indicating the type of number
/// parsed: `0` decimal, `b'0'`/`b'O'`/`b'o'` octal, `b'B'`/`b'b'`
/// binary, `b'X'`/`b'x'` hex. If `len` is given, the length of the
/// number in bytes is returned. If `nptr` is given, the signed result
/// is returned in it. If `unptr` is given, the unsigned result is
/// returned in it. If `what` contains [`STR2NR_BIN`]/[`STR2NR_OCT`]/
/// [`STR2NR_HEX`], recognize binary/octal/hex numbers respectively. If
/// `what` contains [`STR2NR_FORCE`], always assume bin/oct/hex. If
/// `what` contains [`STR2NR_QUOTE`], ignore embedded single quotes. If
/// `maxlen > 0`, check at most `maxlen` bytes. If `strict` is `true`,
/// check the number strictly: `len` (if given) is set to `0` and
/// nothing else is written if it fails.
///
/// Unlike the original's raw, NUL-terminated `char *`, `start` is a
/// bounded `&[u8]` slice - parsing also stops at `start.len()`
/// regardless of `maxlen`, the direct structural equivalent of the
/// original relying on its string's own NUL terminator to stop when
/// `maxlen == 0`.
#[allow(clippy::too_many_arguments)]
pub fn vim_str2nr(
    start: &[u8],
    prep: Option<&mut i32>,
    mut len: Option<&mut i32>,
    what: i32,
    nptr: Option<&mut VarnumberT>,
    unptr: Option<&mut UvarnumberT>,
    maxlen: i32,
    strict: bool,
    mut overflow: Option<&mut bool>,
) {
    let ended = |idx: usize| -> bool { (maxlen != 0 && idx as i32 >= maxlen) || idx >= start.len() };

    if let Some(l) = len.as_deref_mut() {
        *l = 0;
    }

    let negative = start.first() == Some(&b'-');
    let mut idx = usize::from(negative);
    let mut pre: i32 = 0;

    let radix = if what & STR2NR_FORCE != 0 {
        let masked = what & !(STR2NR_FORCE | STR2NR_QUOTE);
        if masked == STR2NR_HEX {
            if !ended(idx + 2)
                && start[idx] == b'0'
                && matches!(start[idx + 1], b'x' | b'X')
                && ascii_isxdigit(start[idx + 2] as i32)
            {
                idx += 2;
            }
            Radix::Hex
        } else if masked == STR2NR_BIN {
            if !ended(idx + 2)
                && start[idx] == b'0'
                && matches!(start[idx + 1], b'b' | b'B')
                && ascii_isbdigit(start[idx + 2] as i32)
            {
                idx += 2;
            }
            Radix::Bin
        } else if masked == STR2NR_OCT || masked == STR2NR_OOCT || masked == (STR2NR_OCT | STR2NR_OOCT) {
            if !ended(idx + 2)
                && start[idx] == b'0'
                && matches!(start[idx + 1], b'o' | b'O')
                && ascii_isodigit(start[idx + 2] as i32)
            {
                idx += 2;
            }
            Radix::Oct
        } else if masked == 0 {
            Radix::Dec
        } else {
            unreachable!("vim_str2nr: invalid `what` bitmask for STR2NR_FORCE");
        }
    } else if what & (STR2NR_HEX | STR2NR_OCT | STR2NR_OOCT | STR2NR_BIN) != 0
        && !ended(idx + 1)
        && start[idx] == b'0'
        && start[idx + 1] != b'8'
        && start[idx + 1] != b'9'
    {
        pre = i32::from(start[idx + 1]);
        if what & STR2NR_HEX != 0
            && !ended(idx + 2)
            && matches!(pre as u8, b'X' | b'x')
            && ascii_isxdigit(start[idx + 2] as i32)
        {
            idx += 2;
            Radix::Hex
        } else if what & STR2NR_BIN != 0
            && !ended(idx + 2)
            && matches!(pre as u8, b'B' | b'b')
            && ascii_isbdigit(start[idx + 2] as i32)
        {
            idx += 2;
            Radix::Bin
        } else if what & STR2NR_OOCT != 0
            && !ended(idx + 2)
            && matches!(pre as u8, b'O' | b'o')
            && ascii_isodigit(start[idx + 2] as i32)
        {
            idx += 2;
            Radix::Oct
        } else {
            // Detect old octal format: '0' followed by octal digits.
            pre = 0;
            if what & STR2NR_OCT == 0 || !ascii_isodigit(start[idx + 1] as i32) {
                Radix::Dec
            } else {
                let mut i = 2;
                let mut is_old_octal = true;
                while !ended(idx + i) && ascii_isdigit(start[idx + i] as i32) {
                    if start[idx + i] > b'7' {
                        is_old_octal = false;
                        break;
                    }
                    i += 1;
                }
                if is_old_octal {
                    pre = i32::from(b'0');
                    Radix::Oct
                } else {
                    Radix::Dec
                }
            }
        }
    } else {
        Radix::Dec
    };

    // Shared digit-accumulation loop (the original's `PARSE_NUMBER`
    // macro, expanded once per radix via `goto`).
    let after_prefix = idx;
    let base = radix.base();
    let mut un: UvarnumberT = 0;
    while !ended(idx) {
        if what & STR2NR_QUOTE != 0 && idx > after_prefix && start[idx] == b'\'' {
            idx += 1;
            if !ended(idx) && radix.is_digit(start[idx]) {
                continue;
            }
            idx -= 1;
        }
        if !radix.is_digit(start[idx]) {
            break;
        }
        let digit = radix.digit_value(start[idx]);
        if un < UVARNUMBER_MAX / base || (un == UVARNUMBER_MAX / base && (base != 10 || digit <= UVARNUMBER_MAX % 10))
        {
            un = base * un + digit;
        } else {
            un = UVARNUMBER_MAX;
            if let Some(o) = overflow.as_deref_mut() {
                *o = true;
            }
        }
        idx += 1;
    }

    // Check for an alphanumeric character immediately following, that
    // is most likely a typo.
    if strict
        && idx as i32 != maxlen
        && !ended(idx)
        && crate::macros_defs::ascii_isalnum(start[idx] as i32)
    {
        return;
    }

    if let Some(p) = prep {
        *p = pre;
    }
    if let Some(l) = len {
        *l = idx as i32;
    }
    if let Some(n) = nptr {
        if negative {
            // avoid overflow
            if un > VARNUMBER_MAX as UvarnumberT {
                *n = VARNUMBER_MIN;
                if let Some(o) = overflow {
                    *o = true;
                }
            } else {
                *n = -(un as VarnumberT);
            }
        } else {
            if un > VARNUMBER_MAX as UvarnumberT {
                un = VARNUMBER_MAX as UvarnumberT;
                if let Some(o) = overflow {
                    *o = true;
                }
            }
            *n = un as VarnumberT;
        }
    }
    if let Some(u) = unptr {
        *u = un;
    }
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

/// Characters in the DEFAULT `'breakat'` value (`" \t!@*-+;:,./?"`) -
/// see [`vim_isbreak`]'s own doc comment for why this is a fixed
/// default-value table rather than the real, `'breakat'`-customizable
/// `breakat_flags[256]` (needs `optionstr.c`'s `did_set_breakat`
/// option-string parsing, not yet translated).
const DEFAULT_BREAKAT: &[u8] = b" \t!@*-+;:,./?";

/// Check if `c` is one of the characters in `'breakat'` (`vim_isbreak`).
/// Used very often if `'linebreak'` is set. Only works for ASCII
/// characters, matching the original's own documented limitation.
///
/// Uses the DEFAULT `'breakat'` value (`" \t!@*-+;:,./?"`) rather than
/// the real, possibly-customized `breakat_flags[256]` table (needs
/// `optionstr.c`'s `did_set_breakat`, not yet translated) - correct
/// for every real session that hasn't customized `'breakat'` (the
/// common case), documented as a simplification rather than pretending
/// the general mechanism exists (matching [`vim_isprintc`]'s own
/// precedent exactly).
#[must_use]
pub fn vim_isbreak(c: i32) -> bool {
    u8::try_from(c).is_ok_and(|b| DEFAULT_BREAKAT.contains(&b))
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

/// Return number of display cells occupied by byte `b`, treated as an
/// isolated single byte rather than a full (possibly multi-byte)
/// character (`byte2cells`). Returns `0` for any byte `>= 0x80` (a
/// lone byte like that has no standalone cell width of its own in a
/// UTF-8 stream - a real difference from [`char2cells`], which
/// decodes a full character there instead). For `b < 0x80`, uses the
/// same `g_chartab`-default-rule width as [`char2cells`] (see that
/// function's own doc comment for the "fixed default rule, not the
/// real `'isprint'`-customizable `g_chartab`" caveat, which applies
/// identically here).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (for `'display'`'s
/// `"uhex"` flag on the control-character path, same as [`char2cells`]).
#[must_use]
pub unsafe fn byte2cells(b: i32) -> i32 {
    if b >= 0x80 {
        return 0;
    }
    if (0x20..=0x7E).contains(&b) {
        return 1;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let dy_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.dy_flags;
    if dy_flags & crate::option_vars::opt_dy_flag::UHEX != 0 {
        4
    } else {
        2
    }
}

/// Convert `n`'s low nibble to its lowercase hex digit character
/// (`nr2hex`, `static inline` in the original - kept private here too).
fn nr2hex(n: u32) -> u8 {
    if (n & 0xf) <= 9 {
        (n & 0xf) as u8 + b'0'
    } else {
        (n & 0xf) as u8 - 10 + b'a'
    }
}

/// Convert a non-printable/illegal character to a hex string like
/// `"<FFFF>"` (`transchar_hex`). Returns the formatted bytes including
/// their own trailing NUL, matching this crate's usual
/// `Vec<u8>`-owns-its-NUL convention for freshly-produced string
/// outputs (e.g. `strings.rs`'s `vim_strup`).
#[must_use]
pub fn transchar_hex(c: i32) -> Vec<u8> {
    let mut buf = vec![b'<'];
    if c > 0xFF {
        if c > 0xFFFF {
            buf.push(nr2hex((c as u32) >> 20));
            buf.push(nr2hex((c as u32) >> 16));
        }
        buf.push(nr2hex((c as u32) >> 12));
        buf.push(nr2hex((c as u32) >> 8));
    }
    buf.push(nr2hex((c as u32) >> 4));
    buf.push(nr2hex(c as u32));
    buf.push(b'>');
    buf.push(0);
    buf
}

/// Convert a non-printable character to 2-4 printable ones
/// (`transchar_nonprint`). Doesn't work for multi-byte characters -
/// `c` must be `<= 0xFF`.
///
/// `buf` is `Option<&BufT>` (the original's nullable `const buf_T *`)
/// - only consulted for `'fileformat'` when translating a lone CR.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (for `'display'`'s
/// `"uhex"` flag).
#[must_use]
pub unsafe fn transchar_nonprint(buf: Option<&crate::buffer_defs::BufT>, c: i32) -> Vec<u8> {
    let mut c = c;
    if c == i32::from(crate::ascii_defs::NL) {
        // we use newline in place of a NUL
        c = i32::from(crate::ascii_defs::NUL);
    } else if buf.is_some_and(|b| {
        c == i32::from(crate::ascii_defs::CAR)
            && crate::option::get_fileformat(b) == crate::option_vars::EOL_MAC
    }) {
        // we use CR in place of NL in this case
        c = i32::from(crate::ascii_defs::NL);
    }
    debug_assert!(c <= 0xff);

    // SAFETY: forwarded from this function's own safety doc.
    let dy_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.dy_flags;
    if dy_flags & crate::option_vars::opt_dy_flag::UHEX != 0 || c > 0x7f {
        // 'display' has "uhex"
        transchar_hex(c)
    } else {
        // 0x00 - 0x1f and 0x7f: DEL displayed as ^?
        vec![b'^', (c as u8) ^ 0x40, 0]
    }
}

/// Convert character `c` for displaying (`transchar_buf`).
///
/// # Deferred
/// The original's `IS_SPECIAL(c)`/`K_SECOND(c)` special-key prefix
/// (`"~@"` followed by the second byte) is NOT handled here - needs
/// `keycodes.h` (not yet translated), and no caller in this crate
/// currently passes an encoded special-key value. The original's
/// `!chartab_initialized && (c >= ' ' && c <= '~')` disjunct is also
/// omitted: it's a pure subset of what [`vim_isprintc`] itself already
/// covers for `c <= 0xFF`, and `chartab_initialized` can never become
/// `true` in this crate anyway (nothing sets it - `init_chartab`/
/// `buf_init_chartab` aren't translated).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via [`vim_isprintc`]/
/// [`transchar_nonprint`]).
#[must_use]
pub unsafe fn transchar_buf(buf: Option<&crate::buffer_defs::BufT>, c: i32) -> Vec<u8> {
    if c <= 0xFF && vim_isprintc(c) {
        // printable character
        vec![c as u8, 0]
    } else if c <= 0xFF {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { transchar_nonprint(buf, c) }
    } else {
        transchar_hex(c)
    }
}

/// Like [`transchar_buf`] but for the current buffer (`transchar`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS`'s `curbuf` plus everything
/// [`transchar_buf`] touches.
#[must_use]
pub unsafe fn transchar(c: i32) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { transchar_buf(Some(curbuf), c) }
}

/// Like [`transchar_buf`], but called with a byte instead of a
/// character. Checks for an illegal UTF-8 byte
/// (`transchar_byte_buf`).
///
/// # Safety
/// Same as [`transchar_buf`].
#[must_use]
pub unsafe fn transchar_byte_buf(buf: Option<&crate::buffer_defs::BufT>, c: i32) -> Vec<u8> {
    if c >= 0x80 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { transchar_nonprint(buf, c) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { transchar_buf(buf, c) }
}

/// Like [`transchar_byte_buf`] but for the current buffer
/// (`transchar_byte`).
///
/// # Safety
/// Same as [`transchar`].
#[must_use]
pub unsafe fn transchar_byte(c: i32) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { transchar_byte_buf(Some(curbuf), c) }
}

/// Copy `s` and replace special characters with printable ones
/// (`transstr`). Works like `strtrans()`.
///
/// Unlike the original (which pre-computes the exact required length
/// via `transstr_len` then writes into a freshly `xmalloc`-ed buffer
/// of that size), this builds the result directly into a growing
/// `Vec<u8>` - Rust has no need for the original's separate
/// length-computing pre-pass, since `Vec` grows dynamically.
/// `transstr_len` itself is therefore not translated as its own
/// function (it had no external caller anyway - only `transstr`/
/// `kv_transstr` ever called it in the original).
///
/// `transstr_buf`'s own distinct "truncate to fit a caller-provided
/// max length" contract (used by `drawline.c`/`statusline.c`, neither
/// yet translated) is deferred separately - this function only covers
/// the unlimited-length case `transstr` itself always uses
/// (`transstr_buf(s, -1, ...)` in the original).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via [`vim_isprintc`],
/// [`crate::mbyte::utfc_ptr2len`], and [`transchar_byte`]'s own
/// `curbuf` dependency).
#[must_use]
pub unsafe fn transstr(s: &[u8], untab: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut pos = 0usize;

    while pos < s.len() && s[pos] != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let l = unsafe { crate::mbyte::utfc_ptr2len(&s[pos..]) } as usize;
        if l > 1 {
            let c = crate::mbyte::utf_ptr2char(&s[pos..]);
            if vim_isprintc(c) {
                out.extend_from_slice(&s[pos..pos + l]);
            } else {
                let mut off = 0usize;
                while off < l {
                    let c2 = crate::mbyte::utf_ptr2char(&s[pos + off..]);
                    let hex = transchar_hex(c2);
                    // drop transchar_hex's own trailing NUL - transstr
                    // appends exactly one, at the very end, below.
                    out.extend_from_slice(&hex[..hex.len() - 1]);
                    off += crate::mbyte::utf_ptr2len(&s[pos + off..]) as usize;
                }
            }
            pos += l;
        } else if s[pos] == crate::ascii_defs::TAB && !untab {
            out.push(s[pos]);
            pos += 1;
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            let tb = unsafe { transchar_byte(i32::from(s[pos])) };
            // drop transchar_byte's own trailing NUL, same reason.
            out.extend_from_slice(&tb[..tb.len() - 1]);
            pos += 1;
        }
    }
    out.push(0);
    out
}

/// Convert `str_` to lowercase, treating multi-byte characters as
/// well as possible (`str_foldcase`, the unlimited/`buf == NULL` case
/// only - see this function's own "Deferred" note).
///
/// Similar in spirit to `strings.rs`'s own `strcase_save(orig, false)`,
/// but NOT identical: this preserves the original's own extra gating
/// condition, `(c < 0x80 || olen > 1) && c != lc` - a single INVALID
/// byte `>= 0x80` (`olen == 1` for an otherwise-illegal UTF-8 lead
/// byte) is left completely untouched here, whereas `strcase_save`
/// would still attempt `mb_tolower` on it. Composing/combining marks
/// following a base character are always copied through byte-for-byte
/// unchanged (only the base character itself is ever decoded via
/// `utf_ptr2char`/replaced) - matches the original's own
/// `i += utfc_ptr2len(...)` advance (the *composed* length) versus
/// `olen = utf_ptr2len(...)` (the base character's own length used
/// for the replacement decision).
///
/// `str_` is treated as NUL-terminated (this crate's usual
/// line-storage convention), not scanned for exactly `orglen` bytes as
/// the original's own explicit length parameter allows (no current
/// caller needs an embedded-NUL substring).
///
/// # Deferred
/// The original's `buf != NULL` fixed-buffer, `buflen`-truncating
/// variant (used by `syntax.c`, not yet translated) is not
/// implemented here - only the unlimited/allocating case.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// [`crate::mbyte::mb_tolower`]/[`crate::mbyte::utfc_ptr2len`]).
#[must_use]
pub unsafe fn str_foldcase(str_: &[u8]) -> Vec<u8> {
    let mut res = Vec::with_capacity(str_.len() + 1);
    let mut pos = 0usize;

    while pos < str_.len() && str_[pos] != 0 {
        let c = crate::mbyte::utf_ptr2char(&str_[pos..]);
        let olen = crate::mbyte::utf_ptr2len(&str_[pos..]) as usize;
        // SAFETY: forwarded from this function's own safety doc.
        let lc = unsafe { crate::mbyte::mb_tolower(c) };

        // Only replace when it's not an invalid sequence (ASCII
        // character or more than one byte) and mb_tolower() actually
        // changes it.
        if (c < 0x80 || olen > 1) && c != lc {
            let mut buf = [0u8; crate::mbyte_defs::MB_MAXBYTES];
            let nlen = crate::mbyte::utf_char2bytes(lc, &mut buf) as usize;
            res.extend_from_slice(&buf[..nlen]);
        } else {
            res.extend_from_slice(&str_[pos..pos + olen]);
        }

        // Composing/combining marks (if any) are never decoded or
        // replaced above - copy them through unchanged.
        // SAFETY: forwarded from this function's own safety doc.
        let composed_len = unsafe { crate::mbyte::utfc_ptr2len(&str_[pos..]) } as usize;
        if composed_len > olen {
            res.extend_from_slice(&str_[pos + olen..pos + composed_len]);
        }
        pos += composed_len;
    }
    res.push(0);
    res
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

    /// Installs `new_curbuf` as `GLOBALS.curbuf` for the test's
    /// duration, restoring the previous pointer on drop (including on
    /// test panic via unwinding). Holds `global_state_test_lock` for
    /// its entire lifetime, matching `mark.rs`'s own `CurbufGuard`
    /// precedent (a plain `BufT` is enough here - unlike
    /// `cursor.rs`'s `CursorTestGuard`, nothing in this file's tests
    /// needs a real, `ml_open`-ed memline).
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
    fn skipbin_and_skiptobin() {
        assert_eq!(skipbin(b"101102"), 5); // stops at the '2' (index 5)
        assert_eq!(skipbin(b"abc"), 0);
        assert_eq!(skiptobin(b"xyz101"), 3);
        assert_eq!(skiptobin(b"xyz"), 3); // end of slice, no binary digit found
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

    /// Convenience wrapper around [`vim_str2nr`] for tests: returns
    /// `(signed value, unsigned value, prep byte, len consumed)`.
    fn str2nr(s: &[u8], what: i32) -> (VarnumberT, UvarnumberT, i32, i32) {
        let mut n: VarnumberT = 0;
        let mut u: UvarnumberT = 0;
        let mut prep: i32 = 0;
        let mut len: i32 = 0;
        vim_str2nr(s, Some(&mut prep), Some(&mut len), what, Some(&mut n), Some(&mut u), 0, false, None);
        (n, u, prep, len)
    }

    #[test]
    fn vim_str2nr_plain_decimal() {
        assert_eq!(str2nr(b"123", STR2NR_ALL), (123, 123, 0, 3));
    }

    #[test]
    fn vim_str2nr_negative_decimal() {
        assert_eq!(str2nr(b"-123", STR2NR_ALL), (-123, 123, 0, 4));
    }

    #[test]
    fn vim_str2nr_hex_lowercase_prefix() {
        assert_eq!(str2nr(b"0x1A", STR2NR_ALL), (26, 26, i32::from(b'x'), 4));
    }

    #[test]
    fn vim_str2nr_hex_uppercase_prefix() {
        assert_eq!(str2nr(b"0X1a", STR2NR_ALL), (26, 26, i32::from(b'X'), 4));
    }

    #[test]
    fn vim_str2nr_binary_prefix() {
        assert_eq!(str2nr(b"0b101", STR2NR_ALL), (5, 5, i32::from(b'b'), 5));
    }

    #[test]
    fn vim_str2nr_explicit_octal_prefix() {
        assert_eq!(str2nr(b"0o17", STR2NR_ALL), (15, 15, i32::from(b'o'), 4));
    }

    #[test]
    fn vim_str2nr_old_style_octal() {
        // Leading '0' is itself included in the accumulated digits (it
        // contributes 0 numerically) - len covers the whole "017".
        assert_eq!(str2nr(b"017", STR2NR_ALL), (15, 15, i32::from(b'0'), 3));
    }

    #[test]
    fn vim_str2nr_old_style_octal_with_invalid_digit_falls_back_to_decimal() {
        // '8'/'9' are not valid octal digits - the original falls back
        // to parsing the whole thing as decimal instead.
        assert_eq!(str2nr(b"018", STR2NR_ALL), (18, 18, 0, 3));
        assert_eq!(str2nr(b"019", STR2NR_ALL), (19, 19, 0, 3));
    }

    #[test]
    fn vim_str2nr_only_recognizes_radixes_allowed_by_what() {
        // Without STR2NR_HEX in `what`, "0x1A" is NOT recognized as hex
        // - it stops at the first non-decimal-digit ('x'), consuming
        // only the leading "0".
        assert_eq!(str2nr(b"0x1A", STR2NR_OCT | STR2NR_BIN), (0, 0, 0, 1));
    }

    #[test]
    fn vim_str2nr_force_hex_without_prefix() {
        let mut n: VarnumberT = 0;
        let mut prep: i32 = 0;
        vim_str2nr(b"1A", Some(&mut prep), None, STR2NR_HEX | STR2NR_FORCE, Some(&mut n), None, 0, false, None);
        assert_eq!(n, 0x1A);
        // FORCE mode never touches `pre` at all in the original - it
        // stays at its initial 0 regardless of the actual radix forced.
        assert_eq!(prep, 0);
    }

    #[test]
    fn vim_str2nr_force_bin_without_prefix() {
        assert_eq!(str2nr(b"101", STR2NR_BIN | STR2NR_FORCE), (5, 5, 0, 3));
    }

    #[test]
    fn vim_str2nr_force_dec() {
        assert_eq!(str2nr(b"123", STR2NR_FORCE), (123, 123, 0, 3));
    }

    #[test]
    fn vim_str2nr_force_still_skips_a_present_prefix() {
        // FORCE mode still skips a matching "0x"/"0b"/"0o" prefix if
        // one happens to be present, rather than parsing it literally.
        assert_eq!(str2nr(b"0x1A", STR2NR_HEX | STR2NR_FORCE), (26, 26, 0, 4));
    }

    #[test]
    fn vim_str2nr_quote_separated_digits() {
        assert_eq!(str2nr(b"1'000'000", STR2NR_ALL | STR2NR_QUOTE), (1_000_000, 1_000_000, 0, 9));
    }

    #[test]
    fn vim_str2nr_quote_not_recognized_without_the_flag() {
        // Without STR2NR_QUOTE, the embedded quote ends the number.
        assert_eq!(str2nr(b"1'000", STR2NR_ALL), (1, 1, 0, 1));
    }

    #[test]
    fn vim_str2nr_maxlen_limits_how_much_is_parsed() {
        let mut n: VarnumberT = 0;
        let mut len: i32 = 0;
        vim_str2nr(b"12345", None, Some(&mut len), STR2NR_ALL, Some(&mut n), None, 3, false, None);
        assert_eq!(n, 123);
        assert_eq!(len, 3);
    }

    #[test]
    fn vim_str2nr_stops_at_trailing_garbage_when_not_strict() {
        assert_eq!(str2nr(b"123abc", STR2NR_ALL), (123, 123, 0, 3));
    }

    #[test]
    fn vim_str2nr_strict_fails_on_trailing_alnum() {
        let mut n: VarnumberT = 123; // pre-set, must stay untouched on failure
        let mut len: i32 = 99; // pre-set, must be reset to 0 on failure
        vim_str2nr(b"123abc", None, Some(&mut len), STR2NR_ALL, Some(&mut n), None, 0, true, None);
        assert_eq!(len, 0);
        assert_eq!(n, 123); // untouched - function returned early
    }

    #[test]
    fn vim_str2nr_strict_succeeds_when_maxlen_exactly_consumed() {
        // Even in strict mode, trailing garbage BEYOND maxlen doesn't
        // fail the parse, since idx == maxlen short-circuits the check.
        let mut n: VarnumberT = 0;
        let mut len: i32 = 0;
        vim_str2nr(b"123abc", None, Some(&mut len), STR2NR_ALL, Some(&mut n), None, 3, true, None);
        assert_eq!(n, 123);
        assert_eq!(len, 3);
    }

    #[test]
    fn vim_str2nr_strict_succeeds_with_no_trailing_chars_at_all() {
        let mut n: VarnumberT = 0;
        let mut len: i32 = 0;
        vim_str2nr(b"123", None, Some(&mut len), STR2NR_ALL, Some(&mut n), None, 0, true, None);
        assert_eq!(n, 123);
        assert_eq!(len, 3);
    }

    #[test]
    fn vim_str2nr_overflow_sets_flag_and_clamps() {
        let mut n: VarnumberT = 0;
        let mut u: UvarnumberT = 0;
        let mut overflow = false;
        // Larger than i64::MAX.
        vim_str2nr(
            b"99999999999999999999",
            None,
            None,
            STR2NR_ALL,
            Some(&mut n),
            Some(&mut u),
            0,
            false,
            Some(&mut overflow),
        );
        assert!(overflow);
        assert_eq!(n, VARNUMBER_MAX);
        // Genuine quirk of the original, faithfully preserved: when
        // BOTH nptr and unptr are requested, the nptr branch's own
        // `un = VARNUMBER_MAX` clamp reassigns the *shared* local `un`
        // before `*unptr = un;` runs afterwards - so unptr "inherits"
        // nptr's i64::MAX clamp here too, rather than reporting the
        // true (larger) UVARNUMBER_MAX accumulated by the parsing loop
        // itself. See vim_str2nr_overflow_unptr_only_sees_true_max
        // below for the case without that interaction.
        assert_eq!(u, VARNUMBER_MAX as UvarnumberT);
    }

    #[test]
    fn vim_str2nr_overflow_unptr_only_sees_true_max() {
        // Without also requesting nptr, unptr reports the parsing
        // loop's own true accumulated-and-clamped UVARNUMBER_MAX,
        // unaffected by the nptr-only clamping quirk above.
        let mut u: UvarnumberT = 0;
        let mut overflow = false;
        vim_str2nr(
            b"99999999999999999999",
            None,
            None,
            STR2NR_ALL,
            None,
            Some(&mut u),
            0,
            false,
            Some(&mut overflow),
        );
        assert!(overflow);
        assert_eq!(u, UVARNUMBER_MAX);
    }

    #[test]
    fn vim_str2nr_no_digits_at_all_leaves_zero() {
        assert_eq!(str2nr(b"", STR2NR_ALL), (0, 0, 0, 0));
        assert_eq!(str2nr(b"abc", STR2NR_ALL), (0, 0, 0, 0));
    }

    #[test]
    fn vim_str2nr_lone_minus_sign_consumes_the_sign_but_parses_as_zero() {
        // The '-' itself is consumed (idx advances past it) even
        // though no digits follow - len reflects that 1-byte advance,
        // matching the original's own unconditional `ptr++` for a
        // leading '-' before any digit-parsing begins.
        assert_eq!(str2nr(b"-", STR2NR_ALL), (0, 0, 0, 1));
    }

    #[test]
    fn vim_str2nr_none_out_params_are_all_optional() {
        // Every out-parameter can be omitted independently - this
        // should not panic.
        vim_str2nr(b"123", None, None, STR2NR_ALL, None, None, 0, false, None);
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
    fn vim_isbreak_recognizes_every_default_breakat_character() {
        for &b in DEFAULT_BREAKAT {
            assert!(vim_isbreak(i32::from(b)), "expected {b:#x} to be a break character");
        }
    }

    #[test]
    fn vim_isbreak_rejects_ordinary_letters_and_digits() {
        assert!(!vim_isbreak(i32::from(b'a')));
        assert!(!vim_isbreak(i32::from(b'Z')));
        assert!(!vim_isbreak(i32::from(b'5')));
        assert!(!vim_isbreak(i32::from(b'_')));
    }

    #[test]
    fn vim_isbreak_rejects_out_of_byte_range_values() {
        assert!(!vim_isbreak(-1));
        assert!(!vim_isbreak(256));
        assert!(!vim_isbreak(i32::MAX));
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
    fn byte2cells_printable_ascii_is_one_and_control_is_two() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { byte2cells(i32::from(b' ')) }, 1);
        assert_eq!(unsafe { byte2cells(i32::from(b'a')) }, 1);
        assert_eq!(unsafe { byte2cells(i32::from(crate::ascii_defs::TAB)) }, 2);
    }

    #[test]
    fn byte2cells_control_char_is_four_with_uhex() {
        let _guard = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.dy_flags;
        opts.dy_flags = crate::option_vars::opt_dy_flag::UHEX;

        assert_eq!(unsafe { byte2cells(0x01) }, 4);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.dy_flags = prev;
    }

    #[test]
    fn byte2cells_any_byte_at_or_above_0x80_is_zero() {
        // Unlike char2cells, byte2cells never decodes a full
        // multibyte character - a lone byte >= 0x80 has no standalone
        // cell width of its own.
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { byte2cells(0x80) }, 0);
        assert_eq!(unsafe { byte2cells(0xff) }, 0);
    }

    #[test]
    fn transchar_hex_two_digit_form_for_byte_values() {
        assert_eq!(transchar_hex(0x41), b"<41>\0");
    }

    #[test]
    fn transchar_hex_four_digit_form_above_0xff() {
        assert_eq!(transchar_hex(0x1234), b"<1234>\0");
    }

    #[test]
    fn transchar_hex_six_digit_form_above_0xffff() {
        assert_eq!(transchar_hex(0x123456), b"<123456>\0");
    }

    #[test]
    fn transchar_hex_zero_is_two_digits() {
        assert_eq!(transchar_hex(0), b"<00>\0");
    }

    #[test]
    fn transchar_nonprint_nul_displays_as_caret_at() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { transchar_nonprint(None, i32::from(crate::ascii_defs::NL)) }, b"^@\0");
    }

    #[test]
    fn transchar_nonprint_del_displays_as_caret_question() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { transchar_nonprint(None, 0x7f) }, b"^?\0");
    }

    #[test]
    fn transchar_nonprint_cr_in_mac_fileformat_displays_as_caret_j() {
        let _guard = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { ..Default::default() };
        buf.b_p_ff = Some(b"mac".to_vec());
        assert_eq!(
            unsafe { transchar_nonprint(Some(&buf), i32::from(crate::ascii_defs::CAR)) },
            b"^J\0"
        );
    }

    #[test]
    fn transchar_nonprint_cr_outside_mac_fileformat_displays_as_caret_m() {
        let _guard = crate::globals::global_state_test_lock();
        let buf = crate::buffer_defs::BufT { ..Default::default() }; // default fileformat isn't mac
        assert_eq!(
            unsafe { transchar_nonprint(Some(&buf), i32::from(crate::ascii_defs::CAR)) },
            b"^M\0"
        );
    }

    #[test]
    fn transchar_nonprint_above_0x7f_uses_hex_form() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { transchar_nonprint(None, 0x80) }, transchar_hex(0x80));
    }

    #[test]
    fn transchar_nonprint_uhex_flag_forces_hex_form_for_control_chars() {
        let _guard = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.dy_flags;
        opts.dy_flags = crate::option_vars::opt_dy_flag::UHEX;

        assert_eq!(unsafe { transchar_nonprint(None, 1) }, transchar_hex(1));

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.dy_flags = prev;
    }

    #[test]
    fn transchar_buf_printable_ascii_is_the_char_itself() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { transchar_buf(None, i32::from(b'A')) }, b"A\0");
    }

    #[test]
    fn transchar_buf_control_char_delegates_to_transchar_nonprint() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { transchar_buf(None, 1) }, b"^A\0");
    }

    #[test]
    fn transchar_buf_above_0xff_uses_hex_form_directly() {
        let _guard = crate::globals::global_state_test_lock();
        // U+4E00 (CJK): > 0xFF, so goes straight to transchar_hex,
        // never through vim_isprintc/transchar_nonprint's c<=0xFF path.
        assert_eq!(unsafe { transchar_buf(None, 0x4e00) }, transchar_hex(0x4e00));
    }

    #[test]
    fn transchar_byte_buf_below_0x80_delegates_to_transchar_buf() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { transchar_byte_buf(None, i32::from(b'A')) }, unsafe {
            transchar_buf(None, i32::from(b'A'))
        });
    }

    #[test]
    fn transchar_byte_buf_at_or_above_0x80_goes_straight_to_nonprint() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { transchar_byte_buf(None, 0x80) }, unsafe {
            transchar_nonprint(None, 0x80)
        });
    }

    #[test]
    fn transstr_plain_printable_ascii_is_unchanged() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        assert_eq!(unsafe { transstr(b"hello\0", false) }, b"hello\0");
    }

    #[test]
    fn transstr_tab_kept_as_is_when_not_untab() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        assert_eq!(unsafe { transstr(b"a\tb\0", false) }, b"a\tb\0");
    }

    #[test]
    fn transstr_tab_translated_to_caret_i_when_untab() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        // TAB (0x09) as a control char: '^' + (0x09 ^ 0x40) = '^' + 'I'.
        assert_eq!(unsafe { transstr(b"a\tb\0", true) }, b"a^Ib\0");
    }

    #[test]
    fn transstr_control_char_becomes_caret_notation() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        // 0x01 -> '^' + (0x01 ^ 0x40) = '^' + 'A'.
        assert_eq!(unsafe { transstr(b"a\x01b\0", false) }, b"a^Ab\0");
    }

    #[test]
    fn transstr_printable_multibyte_char_is_unchanged() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        // "日" (U+65E5) is an ordinary printable CJK character -
        // verified via vim_isprintc directly before writing this test.
        let input = "a日b\0".as_bytes();
        assert_eq!(unsafe { transstr(input, false) }, input);
    }

    #[test]
    fn transstr_nonprintable_multibyte_char_becomes_hex_escape() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        // U+200B (ZERO WIDTH SPACE) is NOT printable (verified via
        // vim_isprintc directly - matches its own existing test
        // `vim_isprintc_delegates_to_utf_printable_at_and_above_0x100`).
        let input = "a\u{200b}b\0".as_bytes();
        assert_eq!(unsafe { transstr(input, false) }, b"a<200b>b\0");
    }

    #[test]
    fn transstr_empty_string_is_just_the_nul() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        assert_eq!(unsafe { transstr(b"\0", false) }, b"\0");
    }

    #[test]
    fn str_foldcase_plain_ascii_is_lowercased() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { str_foldcase(b"ABC\0") }, b"abc\0");
    }

    #[test]
    fn str_foldcase_already_lowercase_is_unchanged() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { str_foldcase(b"abc\0") }, b"abc\0");
    }

    #[test]
    fn str_foldcase_lone_invalid_byte_is_left_untouched() {
        // 0xC0 alone (an illegal UTF-8 lead byte with no continuation)
        // would become 0xE0 under a blind mb_tolower call - verified
        // directly via a throwaway scratch probe - but str_foldcase's
        // own gate (c < 0x80 || olen > 1) excludes single invalid
        // bytes >= 0x80 from ever being replaced, matching the
        // original's own explicit intent.
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { str_foldcase(&[0xC0, 0]) }, [0xC0, 0]);
    }

    #[test]
    fn str_foldcase_preserves_a_following_combining_mark() {
        // "E" + COMBINING ACUTE ACCENT (U+0301) - only the base letter
        // is decoded/replaced; the combining mark is copied through
        // byte-for-byte unchanged (verified against the composing
        // behavior already confirmed for mbyte.rs's utfc_next).
        let _guard = crate::globals::global_state_test_lock();
        let input = "E\u{0301}\0".as_bytes();
        let expected = "e\u{0301}\0".as_bytes();
        assert_eq!(unsafe { str_foldcase(input) }, expected);
    }

    #[test]
    fn str_foldcase_cjk_character_has_no_case_and_is_unchanged() {
        let _guard = crate::globals::global_state_test_lock();
        let input = "一\0".as_bytes();
        assert_eq!(unsafe { str_foldcase(input) }, input);
    }

    #[test]
    fn str_foldcase_empty_string_is_just_the_nul() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { str_foldcase(b"\0") }, b"\0");
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
