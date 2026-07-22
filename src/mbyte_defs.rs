//! Translated from `src/nvim/mbyte_defs.h` (partial).
//!
//! Translated: `MB_MAXBYTES`/`MB_MAXCHAR`, the `ENC_*` encoding-
//! property flags, `UNICODE_INVALID`, `GraphemeState`/
//! `GRAPHEME_STATE_INIT` (now that the `utf8proc` FFI dependency this
//! type is meaningless without has actually been added).
//!
//! Deferred (each needs a subsystem not yet reached in this pass):
//! - `ConvFlags`/`vimconv_T`: needs a real `iconv_t` FFI/crate decision
//!   (`iconv_defs.rs` already notes this - the real iconv binding is a
//!   vendored third-party dependency, not yet reached via FFI).
//! - `CharInfo`/`StrCharInfo`/`CharBoundsOff`: not needed by any
//!   translated caller yet.

/// Maximum number of bytes in a multi-byte character. It can be one
/// 32-bit character of up to 6 bytes, or one 16-bit character of up to
/// three bytes plus six following composing characters of three bytes
/// each (`MB_MAXBYTES`).
pub const MB_MAXBYTES: usize = 21;
/// Maximum length of a Unicode character, excluding composing
/// characters (`MB_MAXCHAR`).
pub const MB_MAXCHAR: usize = 6;

/// Properties used in `enc_canon_table[]` (first three mutually
/// exclusive) (`ENC_*`).
pub mod enc {
    pub const ENC_8BIT: i32 = 0x01;
    pub const ENC_DBCS: i32 = 0x02;
    pub const ENC_UNICODE: i32 = 0x04;

    /// Unicode: Big endian.
    pub const ENC_ENDIAN_B: i32 = 0x10;
    /// Unicode: Little endian.
    pub const ENC_ENDIAN_L: i32 = 0x20;

    /// Unicode: UCS-2.
    pub const ENC_2BYTE: i32 = 0x40;
    /// Unicode: UCS-4.
    pub const ENC_4BYTE: i32 = 0x80;
    /// Unicode: UTF-16.
    pub const ENC_2WORD: i32 = 0x100;

    /// Latin1.
    pub const ENC_LATIN1: i32 = 0x200;
    /// Latin9.
    pub const ENC_LATIN9: i32 = 0x400;
    /// Mac Roman (not Macro Man! :-).
    pub const ENC_MACROMAN: i32 = 0x800;
}

/// `UNICODE_INVALID`.
pub const UNICODE_INVALID: i32 = 0xFFFD;

/// State threaded through repeated calls to `utf8proc_grapheme_break_stateful`
/// (`GraphemeState`, `utf8proc_int32_t` in the original - matches
/// `utf8proc-sys`'s own `utf8proc_int32_t = i32`).
pub type GraphemeState = i32;

/// Initial value for a fresh [`GraphemeState`], before the first call
/// in a sequence (`GRAPHEME_STATE_INIT`, `mbyte.h`).
pub const GRAPHEME_STATE_INIT: GraphemeState = 0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enc_flags_are_distinct_bits() {
        let all = [
            enc::ENC_8BIT,
            enc::ENC_DBCS,
            enc::ENC_UNICODE,
            enc::ENC_ENDIAN_B,
            enc::ENC_ENDIAN_L,
            enc::ENC_2BYTE,
            enc::ENC_4BYTE,
            enc::ENC_2WORD,
            enc::ENC_LATIN1,
            enc::ENC_LATIN9,
            enc::ENC_MACROMAN,
        ];
        let mut seen = 0;
        for f in all {
            assert_eq!(seen & f, 0, "flag {f:#x} overlaps a previous one");
            seen |= f;
        }
    }
}
