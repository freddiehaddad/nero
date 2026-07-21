//! Translated from `src/nvim/ascii_defs.h`.
//!
//! The original's `#include "ascii_defs.h.inline.generated.h"` is part of
//! neovim's C-only build-time declaration generator (`src/gen/gen_declarations.lua`),
//! which re-declares `static inline` functions so other translation units
//! can see them. Rust has no such mechanism because it's unnecessary: any
//! `pub fn`/`pub const fn` here is already visible to every other module
//! that imports it. No Rust counterpart needed.

use crate::macros_defs::toupper_asc;

/// `CHAR_ORD(x)`
#[inline]
pub fn char_ord(x: u8) -> u8 {
    if x < b'a' {
        x - b'A'
    } else {
        x - b'a'
    }
}

/// `CHAR_ORD_LOW(x)`
#[inline]
pub fn char_ord_low(x: u8) -> u8 {
    x.wrapping_sub(b'a')
}

/// `CHAR_ORD_UP(x)`
#[inline]
pub fn char_ord_up(x: u8) -> u8 {
    x.wrapping_sub(b'A')
}

/// `ROT13(c, a)`
#[inline]
pub fn rot13(c: i32, a: i32) -> i32 {
    (((c - a) + 13) % 26) + a
}

// Definitions of various common control characters.
pub const NUL: u8 = 0o000;
pub const BELL: u8 = 0o007;
pub const BS: u8 = 0o010;
pub const TAB: u8 = 0o011;
pub const NL: u8 = 0o012;
pub const NL_STR: &str = "\x0A";
pub const FF: u8 = 0o014;
/// CR is used by Mac OS X
pub const CAR: u8 = 0o015;
pub const ESC: u8 = 0o033;
pub const ESC_STR: &str = "\x1B";
pub const DEL: u8 = 0x7f;
pub const DEL_STR: &str = "\x7F";
/// Control Sequence Introducer
pub const CSI: u8 = 0x9b;
pub const CSI_STR: &str = "\u{9b}";
/// Device Control String
pub const DCS: u8 = 0x90;
/// String Terminator
pub const STERM: u8 = 0x9c;

pub const POUND: u32 = 0xA3;

/// `CTRL_CHR(x)`: `'?' -> DEL`, `'@' -> ^@`, etc.
#[inline]
pub fn ctrl_chr(x: i32) -> i32 {
    toupper_asc(x) ^ 0x40
}

/// `META(x)`
#[inline]
pub fn meta(x: i32) -> i32 {
    x | 0x80
}

pub const CTRL_F_STR: &str = "\x06";
pub const CTRL_H_STR: &str = "\x08";
pub const CTRL_V_STR: &str = "\x16";

/// @
pub const CTRL_AT: u8 = 0;
pub const CTRL_A: u8 = 1;
pub const CTRL_B: u8 = 2;
pub const CTRL_C: u8 = 3;
pub const CTRL_D: u8 = 4;
pub const CTRL_E: u8 = 5;
pub const CTRL_F: u8 = 6;
pub const CTRL_G: u8 = 7;
pub const CTRL_H: u8 = 8;
pub const CTRL_I: u8 = 9;
pub const CTRL_J: u8 = 10;
pub const CTRL_K: u8 = 11;
pub const CTRL_L: u8 = 12;
pub const CTRL_M: u8 = 13;
pub const CTRL_N: u8 = 14;
pub const CTRL_O: u8 = 15;
pub const CTRL_P: u8 = 16;
pub const CTRL_Q: u8 = 17;
pub const CTRL_R: u8 = 18;
pub const CTRL_S: u8 = 19;
pub const CTRL_T: u8 = 20;
pub const CTRL_U: u8 = 21;
pub const CTRL_V: u8 = 22;
pub const CTRL_W: u8 = 23;
pub const CTRL_X: u8 = 24;
pub const CTRL_Y: u8 = 25;
pub const CTRL_Z: u8 = 26;
// CTRL- [ Left Square Bracket == ESC
/// `\` BackSLash
pub const CTRL_BSL: u8 = 28;
/// `]` Right Square Bracket
pub const CTRL_RSB: u8 = 29;
/// `^`
pub const CTRL_HAT: u8 = 30;
pub const CTRL__: u8 = 31;

/// Character that separates dir names in a path (`PATHSEP`/`PATHSEPSTR`).
/// Note: this is `'/'` even on Windows in the original - neovim treats `/`
/// as its primary path separator on every platform.
pub const PATHSEP: u8 = b'/';
pub const PATHSEPSTR: &str = "/";

/// Checks if `c` is a space or tab character.
///
/// See also [`ascii_isdigit`].
#[inline]
pub fn ascii_iswhite(c: i32) -> bool {
    c == b' ' as i32 || c == b'\t' as i32
}

/// Checks if `c` is a space or tab character or NUL.
///
/// See also [`ascii_isdigit`].
#[inline]
pub fn ascii_iswhite_or_nul(c: i32) -> bool {
    ascii_iswhite(c) || c == NUL as i32
}

/// Checks if `c` is a space or tab or newline character or NUL.
///
/// See also [`ascii_isdigit`].
#[inline]
pub fn ascii_iswhite_nl_or_nul(c: i32) -> bool {
    ascii_iswhite(c) || c == b'\n' as i32 || c == NUL as i32
}

/// Check whether character is a decimal digit.
///
/// Library `isdigit()` function is officially locale-dependent and, for
/// example, returns true for superscript 1 (¹) in locales where encoding
/// contains it in lower 8 bits. Also avoids crashes in case `c` is below 0
/// or above 255: library functions are officially defined as accepting only
/// `EOF` and unsigned char values (otherwise it is undefined behaviour),
/// which may be used for some optimizations (e.g. a simple
/// `return isdigit_table[c];`).
#[inline]
pub fn ascii_isdigit(c: i32) -> bool {
    (b'0' as i32..=b'9' as i32).contains(&c)
}

/// Checks if `c` is a hexadecimal digit, that is, one of 0-9, a-f, A-F.
///
/// See also [`ascii_isdigit`].
#[inline]
pub fn ascii_isxdigit(c: i32) -> bool {
    (b'0' as i32..=b'9' as i32).contains(&c)
        || (b'a' as i32..=b'f' as i32).contains(&c)
        || (b'A' as i32..=b'F' as i32).contains(&c)
}

/// Checks if `c` is an "identifier" character.
///
/// That is, whether it is an alphanumeric character or underscore.
#[inline]
pub fn ascii_isident(c: i32) -> bool {
    crate::macros_defs::ascii_isalnum(c) || c == b'_' as i32
}

/// Checks if `c` is a binary digit, that is, 0-1.
///
/// See also [`ascii_isdigit`].
#[inline]
pub fn ascii_isbdigit(c: i32) -> bool {
    c == b'0' as i32 || c == b'1' as i32
}

/// Checks if `c` is an octal digit, that is, 0-7.
///
/// See also [`ascii_isdigit`].
#[inline]
pub fn ascii_isodigit(c: i32) -> bool {
    (b'0' as i32..=b'7' as i32).contains(&c)
}

/// Checks if `c` is a white-space character, that is, one of `\f`, `\n`,
/// `\r`, `\t`, `\v`.
///
/// See also [`ascii_isdigit`].
#[inline]
pub fn ascii_isspace(c: i32) -> bool {
    (9..=13).contains(&c) || c == b' ' as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_ord_matches_c_macro() {
        // CHAR_ORD(x) = x < 'a' ? x - 'A' : x - 'a'
        assert_eq!(char_ord(b'A'), 0);
        assert_eq!(char_ord(b'Z'), 25);
        assert_eq!(char_ord(b'a'), 0);
        assert_eq!(char_ord(b'z'), 25);
    }

    #[test]
    fn rot13_round_trips() {
        // rot13('n', 'a') applied twice returns the original ('a'-based
        // alphabet, matching how ROT13 is actually invoked in normal.c).
        let n = rot13(b'n' as i32, b'a' as i32);
        assert_eq!(rot13(n, b'a' as i32), b'n' as i32);
        assert_eq!(rot13(b'a' as i32, b'a' as i32), b'n' as i32);
    }

    #[test]
    fn ctrl_chr_matches_known_mappings() {
        // '?' -> DEL, '@' -> ^@ per the doc comment on CTRL_CHR.
        assert_eq!(ctrl_chr(b'?' as i32), DEL as i32);
        assert_eq!(ctrl_chr(b'@' as i32), CTRL_AT as i32);
        assert_eq!(ctrl_chr(b'A' as i32), CTRL_A as i32);
    }

    #[test]
    fn ascii_predicates() {
        assert!(ascii_iswhite(b' ' as i32));
        assert!(ascii_iswhite(b'\t' as i32));
        assert!(!ascii_iswhite(b'x' as i32));
        assert!(ascii_isdigit(b'5' as i32));
        assert!(!ascii_isdigit(b'a' as i32));
        assert!(ascii_isxdigit(b'f' as i32));
        assert!(ascii_isxdigit(b'F' as i32));
        assert!(!ascii_isxdigit(b'g' as i32));
        assert!(ascii_isident(b'_' as i32));
        assert!(ascii_isident(b'9' as i32));
        assert!(!ascii_isident(b'-' as i32));
        assert!(ascii_isbdigit(b'0' as i32));
        assert!(ascii_isbdigit(b'1' as i32));
        assert!(!ascii_isbdigit(b'2' as i32));
        assert!(ascii_isodigit(b'7' as i32));
        assert!(!ascii_isodigit(b'8' as i32));
        assert!(ascii_isspace(b'\r' as i32));
        assert!(ascii_isspace(b' ' as i32));
    }
}
