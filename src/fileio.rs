//! Translated from `src/nvim/fileio.c` (tractable core only).
//!
//! `fileio.c` (~3600 lines) is the real file-reading (`readfile()`)
//! and encoding-detection (`check_for_bom`) engine. Almost everything
//! needs real buffered file I/O and buffer-line construction, neither
//! translated.
//!
//! Translated: `get_fio_flags` (resolve the `FIO_*` conversion flags
//! for a given encoding name, via `mbyte.c`'s already-real
//! `enc_canon_props`; the `ENC_DBCS` branch needs `iconv()`, not
//! translated, but simply returns `0` in the original too - no
//! shortcut taken, this is the real behavior) and `ucs2bytes`
//! (`static`/private in the original - encode one Unicode codepoint
//! as bytes in a given `FIO_*` encoding; needed by `bufwrite.c`'s
//! `make_bom`).
//!
//! Deferred: everything else in the file.

use crate::mbyte::enc_canon_props;

/// `FIO_*` conversion flags (`fileio.h`).
pub mod fio {
    /// Convert Latin1.
    pub const FIO_LATIN1: i32 = 0x01;
    /// Convert UTF-8.
    pub const FIO_UTF8: i32 = 0x02;
    /// Convert UCS-2.
    pub const FIO_UCS2: i32 = 0x04;
    /// Convert UCS-4.
    pub const FIO_UCS4: i32 = 0x08;
    /// Convert UTF-16.
    pub const FIO_UTF16: i32 = 0x10;
    /// Little endian.
    pub const FIO_ENDIAN_L: i32 = 0x80;
    /// Skip encoding conversion.
    pub const FIO_NOCONVERT: i32 = 0x2000;
    /// Check for BOM at start of file.
    pub const FIO_UCSBOM: i32 = 0x4000;
    /// Allow all formats.
    pub const FIO_ALL: i32 = -1;
}

/// Return the `FIO_*` flags needed for the internal conversion if
/// `name` was unicode or latin1, otherwise `0`. If `name` is empty,
/// uses `'encoding'` (`get_fio_flags`).
///
/// # Safety
/// `crate::option_vars::OPTION_VARS` must be a valid, initialized
/// singleton (same requirement as every other `OPTION_VARS`-reading
/// function in this crate).
#[must_use]
pub unsafe fn get_fio_flags(name: &[u8]) -> i32 {
    let owned_enc;
    let name = if name.is_empty() {
        // SAFETY: forwarded from this function's own safety doc.
        owned_enc = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_enc
            .clone()
            .unwrap_or_default();
        &owned_enc[..]
    } else {
        name
    };
    let prop = enc_canon_props(name);
    if prop & crate::mbyte_defs::enc::ENC_UNICODE != 0 {
        if prop & crate::mbyte_defs::enc::ENC_2BYTE != 0 {
            if prop & crate::mbyte_defs::enc::ENC_ENDIAN_L != 0 {
                return fio::FIO_UCS2 | fio::FIO_ENDIAN_L;
            }
            return fio::FIO_UCS2;
        }
        if prop & crate::mbyte_defs::enc::ENC_4BYTE != 0 {
            if prop & crate::mbyte_defs::enc::ENC_ENDIAN_L != 0 {
                return fio::FIO_UCS4 | fio::FIO_ENDIAN_L;
            }
            return fio::FIO_UCS4;
        }
        if prop & crate::mbyte_defs::enc::ENC_2WORD != 0 {
            if prop & crate::mbyte_defs::enc::ENC_ENDIAN_L != 0 {
                return fio::FIO_UTF16 | fio::FIO_ENDIAN_L;
            }
            return fio::FIO_UTF16;
        }
        return fio::FIO_UTF8;
    }
    if prop & crate::mbyte_defs::enc::ENC_LATIN1 != 0 {
        return fio::FIO_LATIN1;
    }
    // must be ENC_DBCS, requires iconv() - not translated, matching
    // the original's own real (not a placeholder) `return 0;` here.
    0
}

/// Convert a Unicode character to bytes, appending them to `out`
/// (`ucs2bytes`). Returns `true` for an error, `false` when it's OK -
/// the original's own in-out `char **pp` pointer is replaced by
/// appending to a growing `Vec<u8>`, matching this crate's own
/// established "no separate length-then-fill pass needed" convention
/// (e.g. `grow_string_tv`).
pub fn ucs2bytes(c: u32, out: &mut Vec<u8>, flags: i32) -> bool {
    let mut error = false;

    if flags & fio::FIO_UCS4 != 0 {
        if flags & fio::FIO_ENDIAN_L != 0 {
            out.push(c as u8);
            out.push((c >> 8) as u8);
            out.push((c >> 16) as u8);
            out.push((c >> 24) as u8);
        } else {
            out.push((c >> 24) as u8);
            out.push((c >> 16) as u8);
            out.push((c >> 8) as u8);
            out.push(c as u8);
        }
    } else if flags & (fio::FIO_UCS2 | fio::FIO_UTF16) != 0 {
        let mut c = c;
        if c >= 0x10000 {
            if flags & fio::FIO_UTF16 != 0 {
                // Make two words, ten bits of the character in each.
                // First word is 0xd800-0xdbff, second 0xdc00-0xdfff.
                c -= 0x10000;
                if c >= 0x100000 {
                    error = true;
                }
                let cc = ((c >> 10) & 0x3ff) + 0xd800;
                if flags & fio::FIO_ENDIAN_L != 0 {
                    out.push(cc as u8);
                    out.push((cc >> 8) as u8);
                } else {
                    out.push((cc >> 8) as u8);
                    out.push(cc as u8);
                }
                c = (c & 0x3ff) + 0xdc00;
            } else {
                error = true;
            }
        }
        if flags & fio::FIO_ENDIAN_L != 0 {
            out.push(c as u8);
            out.push((c >> 8) as u8);
        } else {
            out.push((c >> 8) as u8);
            out.push(c as u8);
        }
    } else {
        // Latin1
        if c >= 0x100 {
            error = true;
            out.push(0xBF);
        } else {
            out.push(c as u8);
        }
    }

    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    #[test]
    fn get_fio_flags_utf8() {
        assert_eq!(unsafe { get_fio_flags(b"utf-8") }, fio::FIO_UTF8);
    }

    #[test]
    fn get_fio_flags_latin1() {
        assert_eq!(unsafe { get_fio_flags(b"latin1") }, fio::FIO_LATIN1);
    }

    #[test]
    fn get_fio_flags_ucs2_big_endian() {
        assert_eq!(unsafe { get_fio_flags(b"ucs-2") }, fio::FIO_UCS2);
    }

    #[test]
    fn get_fio_flags_ucs2_little_endian() {
        assert_eq!(
            unsafe { get_fio_flags(b"ucs-2le") },
            fio::FIO_UCS2 | fio::FIO_ENDIAN_L
        );
    }

    #[test]
    fn get_fio_flags_ucs4_big_endian() {
        assert_eq!(unsafe { get_fio_flags(b"ucs-4") }, fio::FIO_UCS4);
    }

    #[test]
    fn get_fio_flags_utf16_little_endian() {
        assert_eq!(
            unsafe { get_fio_flags(b"utf-16le") },
            fio::FIO_UTF16 | fio::FIO_ENDIAN_L
        );
    }

    #[test]
    fn get_fio_flags_dbcs_returns_zero() {
        assert_eq!(unsafe { get_fio_flags(b"sjis") }, 0);
    }

    #[test]
    fn get_fio_flags_unknown_name_returns_zero() {
        assert_eq!(unsafe { get_fio_flags(b"not-a-real-encoding") }, 0);
    }

    #[test]
    fn get_fio_flags_empty_name_uses_p_enc() {
        let _lock = global_state_test_lock();
        let saved = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = Some(b"utf-8".to_vec());
        let result = unsafe { get_fio_flags(b"") };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = saved;
        assert_eq!(result, fio::FIO_UTF8);
    }

    #[test]
    fn ucs2bytes_utf8_is_not_handled_here_latin1_fallback_for_ascii() {
        let mut out = Vec::new();
        let error = ucs2bytes(0x41, &mut out, fio::FIO_LATIN1);
        assert!(!error);
        assert_eq!(out, vec![0x41]);
    }

    #[test]
    fn ucs2bytes_latin1_out_of_range_errors_and_writes_0xbf() {
        let mut out = Vec::new();
        let error = ucs2bytes(0x1000, &mut out, fio::FIO_LATIN1);
        assert!(error);
        assert_eq!(out, vec![0xBF]);
    }

    #[test]
    fn ucs2bytes_ucs2_big_endian() {
        let mut out = Vec::new();
        let error = ucs2bytes(0xfeff, &mut out, fio::FIO_UCS2);
        assert!(!error);
        assert_eq!(out, vec![0xfe, 0xff]);
    }

    #[test]
    fn ucs2bytes_ucs2_little_endian() {
        let mut out = Vec::new();
        let error = ucs2bytes(0xfeff, &mut out, fio::FIO_UCS2 | fio::FIO_ENDIAN_L);
        assert!(!error);
        assert_eq!(out, vec![0xff, 0xfe]);
    }

    #[test]
    fn ucs2bytes_ucs4_little_endian() {
        let mut out = Vec::new();
        let error = ucs2bytes(0x0001_0000, &mut out, fio::FIO_UCS4 | fio::FIO_ENDIAN_L);
        assert!(!error);
        assert_eq!(out, vec![0x00, 0x00, 0x01, 0x00]);
    }

    #[test]
    fn ucs2bytes_ucs4_big_endian() {
        let mut out = Vec::new();
        let error = ucs2bytes(0x0001_0000, &mut out, fio::FIO_UCS4);
        assert!(!error);
        assert_eq!(out, vec![0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn ucs2bytes_utf16_surrogate_pair_big_endian() {
        // U+10000 -> surrogate pair 0xD800 0xDC00 (the smallest
        // codepoint requiring UTF-16 surrogate encoding).
        let mut out = Vec::new();
        let error = ucs2bytes(0x0001_0000, &mut out, fio::FIO_UTF16);
        assert!(!error);
        assert_eq!(out, vec![0xd8, 0x00, 0xdc, 0x00]);
    }

    #[test]
    fn ucs2bytes_utf16_surrogate_pair_little_endian() {
        let mut out = Vec::new();
        let error = ucs2bytes(0x0001_0000, &mut out, fio::FIO_UTF16 | fio::FIO_ENDIAN_L);
        assert!(!error);
        assert_eq!(out, vec![0x00, 0xd8, 0x00, 0xdc]);
    }

    #[test]
    fn ucs2bytes_ucs2_out_of_range_errors_via_utf16_fallback_path() {
        // FIO_UCS2 (without FIO_UTF16) can't represent codepoints
        // >= 0x10000 at all - the original's own `else { error =
        // true; }` branch, still writing the low 16 bits as a
        // (wrong, but faithfully-replicated) 2-byte value.
        let mut out = Vec::new();
        let error = ucs2bytes(0x0001_0000, &mut out, fio::FIO_UCS2);
        assert!(error);
        assert_eq!(out, vec![0x00, 0x00]);
    }
}
