//! Translated from `src/nvim/fileio.c` (tractable core only).
//!
//! `fileio.c` (~3600 lines) is the real file-reading (`readfile()`)
//! and encoding-detection (`check_for_bom`) engine. Almost everything
//! needs real buffered file I/O and buffer-line construction, neither
//! translated.
//!
//! Also translated: [`is_dev_fd_file`]/[`readfile_linenr`]/
//! [`write_lnum_adjust`] - three self-contained helpers needing no
//! file I/O. `is_dev_fd_file` rejects `/dev/fd/0`, `/1` and `/2`
//! (opening those can hang the editor) but only as a LONE digit, so
//! `/dev/fd/10` stays valid. `write_lnum_adjust` leaves the `0`
//! "nothing is missing an EOL" sentinel alone rather than shifting it
//! into a real line number.
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

/// Whether `fname` names a `/dev/fd/N` file that is safe to open
/// (`is_dev_fd_file`).
///
/// Some shells on some systems pass these in place of a real file.
/// `/dev/fd/0`, `/dev/fd/1` and `/dev/fd/2` are deliberately REJECTED
/// because opening those can hang the editor, but only when the digit
/// is the last character, so `/dev/fd/10` is still accepted.
#[must_use]
pub fn is_dev_fd_file(fname: &[u8]) -> bool {
    const PREFIX: &[u8] = b"/dev/fd/";
    if !fname.starts_with(PREFIX) {
        return false;
    }
    let Some(&first_digit) = fname.get(PREFIX.len()) else {
        return false;
    };
    if !crate::ascii_defs::ascii_isdigit(i32::from(first_digit)) {
        return false;
    }
    // Everything after the first digit must be digits, to the end.
    let rest = &fname[PREFIX.len() + 1..];
    let after = crate::charset::skipdigits(rest);
    if rest.get(after).is_some_and(|&c| c != crate::ascii_defs::NUL) {
        return false;
    }

    // A single digit 0/1/2 is the unsafe case; more digits are fine.
    let has_more_digits =
        rest.first().is_some_and(|&c| c != crate::ascii_defs::NUL);
    has_more_digits || !matches!(first_digit, b'0' | b'1' | b'2')
}

/// Estimate the line number reached after reading more bytes
/// (`readfile_linenr`), for error messages that include one.
///
/// `linecnt` is the buffer's line count before the extra bytes were
/// read, and `more` is those bytes.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer.
#[must_use]
pub unsafe fn readfile_linenr(
    linecnt: crate::pos_defs::LinenrT,
    more: &[u8],
) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_ml.ml_line_count };
    let newlines = crate::pos_defs::LinenrT::try_from(
        more.iter().filter(|&&c| c == b'\n').count(),
    )
    .unwrap_or(crate::pos_defs::LinenrT::MAX);
    line_count - linecnt + 1 + newlines
}

/// Adjust the line marked as missing its end-of-line for the next
/// write (`write_lnum_adjust`), used when `do_filter()` deletes the
/// filter's input lines.
///
/// Does nothing when no line is missing an EOL, so the sentinel `0`
/// is never shifted into a real line number.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer.
pub unsafe fn write_lnum_adjust(offset: crate::pos_defs::LinenrT) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    if curbuf.b_no_eol_lnum != 0 {
        curbuf.b_no_eol_lnum += offset;
    }
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
    fn is_dev_fd_file_accepts_multi_digit_descriptors() {
        assert!(is_dev_fd_file(b"/dev/fd/3"));
        assert!(is_dev_fd_file(b"/dev/fd/10"));
        assert!(is_dev_fd_file(b"/dev/fd/123"));
    }

    #[test]
    fn is_dev_fd_file_rejects_the_three_standard_descriptors() {
        // Opening these can hang the editor, so they are excluded...
        assert!(!is_dev_fd_file(b"/dev/fd/0"));
        assert!(!is_dev_fd_file(b"/dev/fd/1"));
        assert!(!is_dev_fd_file(b"/dev/fd/2"));
        // ...but only as a LONE digit: a longer number starting with
        // one of them is a different descriptor and stays valid.
        assert!(is_dev_fd_file(b"/dev/fd/01"));
        assert!(is_dev_fd_file(b"/dev/fd/20"));
    }

    #[test]
    fn is_dev_fd_file_rejects_anything_else() {
        assert!(!is_dev_fd_file(b"/dev/fd/"));
        assert!(!is_dev_fd_file(b"/dev/fd/x"));
        // Trailing non-digits disqualify it.
        assert!(!is_dev_fd_file(b"/dev/fd/3x"));
        assert!(!is_dev_fd_file(b"/dev/null"));
        assert!(!is_dev_fd_file(b""));
    }

    #[test]
    fn readfile_linenr_counts_newlines_in_the_extra_bytes() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        buf.b_ml.ml_line_count = 10;
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = globals.curbuf;
        globals.curbuf = &mut buf as *mut crate::buffer_defs::BufT;

        // 10 - 8 + 1 = 3, plus two more newlines.
        assert_eq!(unsafe { readfile_linenr(8, b"a\nb\nc") }, 5);
        // With no newlines the estimate is just the base.
        assert_eq!(unsafe { readfile_linenr(8, b"abc") }, 3);
        assert_eq!(unsafe { readfile_linenr(10, b"") }, 1);

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

    #[test]
    fn write_lnum_adjust_only_shifts_a_real_line() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let ptr: *mut crate::buffer_defs::BufT = &mut buf;
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = globals.curbuf;
        globals.curbuf = ptr;

        // Everything below goes through `ptr` rather than touching
        // `buf` directly: interleaving the two invalidates the raw
        // pointer's tag under Tree Borrows, which Miri rejects.
        //
        // 0 is the "no line is missing an EOL" sentinel, so it must
        // not be shifted into a real line number.
        unsafe { (*ptr).b_no_eol_lnum = 0 };
        unsafe { write_lnum_adjust(5) };
        assert_eq!(unsafe { (*ptr).b_no_eol_lnum }, 0);

        unsafe { (*ptr).b_no_eol_lnum = 7 };
        unsafe { write_lnum_adjust(5) };
        assert_eq!(unsafe { (*ptr).b_no_eol_lnum }, 12);
        // A negative offset shifts back the other way.
        unsafe { write_lnum_adjust(-2) };
        assert_eq!(unsafe { (*ptr).b_no_eol_lnum }, 10);

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

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
