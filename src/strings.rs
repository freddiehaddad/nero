//! Translated from `src/nvim/strings.c` (partial).
//!
//! `strings.c` is large (93KB) and mixes several unrelated concerns: a
//! handful of low-level byte-string utilities (translated here), a large
//! custom `vim_snprintf`/`vim_vsnprintf` implementation with positional
//! `$`-style format-argument support, and dozens of `f_*` Vimscript builtin
//! function implementations (`f_strlen`, `f_tr`, `f_trim`, etc.) that
//! belong with the eval engine (phase 5), not here. Only the low-level
//! utilities with no eval dependency are translated in this pass:
//! - `xstrnsave`, `vim_stricmp`, `vim_strnicmp`, `striequal`,
//!   `vim_strnicmp_asc`, `sort_strings`, `has_non_ascii`,
//!   `has_non_ascii_len`, `concat_str`; and now that `mbyte.c`'s
//!   `mb_toupper`/`mb_tolower`/`utf_ptr2char_info`/`utf_char2bytes` all
//!   exist: `vim_strup`, `vim_strsave_up` (also serving
//!   `vim_strnsave_up`/`vim_strcpy_up`/`vim_strncpy_up`/`vim_memcpy_up`'s
//!   role - a Rust `&[u8]` slice already knows its own exact length, so
//!   there's no separate "whole string" vs. "first `n` bytes" variant to
//!   keep apart), `mb_strup_buf`, `strcase_save`; `vim_strchr` (re-examined:
//!   an earlier note claimed this needed `charset.c`'s real `g_chartab`/
//!   `option.c`, but re-reading the actual body shows it only needs
//!   `strchr`/`strstr`-equivalent byte/substring search plus the
//!   already-translated `utf_char2bytes` - no chartab dependency at all;
//!   used extremely widely - 380+ call sites - across the rest of the
//!   original source, so this was worth double-checking rather than
//!   trusting the stale note).
//!
//! `vim_strup`/`vim_strsave_up`/`mb_strup_buf`/`strcase_save` are all
//! self-bounding via NUL-scanning (matching the original's own
//! `strlen`-based sizing/`while (*p != NUL)` loops) rather than
//! operating on their input slice's full length verbatim - unlike
//! [`xstrnsave`] (a lower-level "copy exactly N bytes, embedded NULs
//! included" primitive taking an explicit length, which deliberately
//! does *not* stop at an embedded NUL, per its own doc comment). Each
//! returns/leaves its own trailing NUL byte, matching this crate's
//! established `Vec<u8>`-includes-its-own-NUL convention. `vim_strchr`
//! is the same way (stops at the first embedded NUL, matching real
//! `strchr`/`strstr` on a NUL-terminated C string), but returns a byte
//! offset (`Option<usize>`) rather than a NUL-terminated `Vec<u8>`,
//! matching this crate's established "index instead of a raw pointer
//! into the same buffer" convention (e.g. `path.rs`'s `path_tail`/
//! `get_past_head`).
//!
//! Deferred:
//! - `vim_strsave_escaped`/`_ext`, `vim_strnsave_unquoted`,
//!   `vim_strsave_shellescape`, `del_trailing_spaces`:
//!   need `charset.c`'s real `g_chartab`/`option.c`.
//! - `vim_snprintf`/`vim_vsnprintf`/`kv_do_printf` and the whole custom
//!   positional-argument printf machinery: Rust's native `format!`/
//!   `write!` macros are the direct replacement for this (matching
//!   `printf`-style format strings is a C-specific problem this
//!   translation doesn't have), used directly at whichever call sites
//!   actually need formatted output when those are translated.
//! - Every `f_*` function (Vimscript builtins operating on `typval_T`):
//!   belongs with the eval engine, phase 5.
//! - `reverse_text`, `strrep`, `cmp_keyvalue_value*`: not yet reached: no
//!   caller translated yet that needs them.

use crate::ascii_defs::NUL;
use crate::macros_defs::tolower_loc;

/// Copy up to `len` bytes of `string` into newly allocated memory and
/// NUL-terminate. The result always has size `len + 1`, even when `string`
/// is shorter than `len` (`xstrnsave`).
///
/// Note: like the rest of this crate's memory-module conventions, `string`
/// is modeled as exact-length content, not a NUL-scanned C buffer - so
/// unlike the original's `strncpy` (which stops copying at an embedded NUL
/// and zero-pads the rest), this simply copies all of `string` (up to
/// `len` bytes) verbatim.
pub fn xstrnsave(string: &[u8], len: usize) -> Vec<u8> {
    let mut ret = vec![0u8; len + 1];
    let n = string.len().min(len);
    ret[..n].copy_from_slice(&string[..n]);
    ret
}

/// Compare two strings ignoring case, using the current locale
/// (`vim_stricmp`). Doesn't work for multi-byte characters.
///
/// Returns `0` for a match, `<0` if `s1 < s2`, `>0` if `s1 > s2`.
pub fn vim_stricmp(s1: &[u8], s2: &[u8]) -> i32 {
    let mut i1 = s1.iter();
    let mut i2 = s2.iter();
    loop {
        let c1 = i1.next().copied().unwrap_or(NUL);
        let c2 = i2.next().copied().unwrap_or(NUL);
        let diff = tolower_loc(c1 as i32) - tolower_loc(c2 as i32);
        if diff != 0 {
            return diff; // this character different
        }
        if c1 == NUL {
            break; // strings match until NUL
        }
    }
    0 // strings match
}

/// Compare two strings for length `len`, ignoring case, using the current
/// locale (`vim_strnicmp`). Doesn't work for multi-byte characters.
///
/// Returns `0` for a match, `<0` if `s1 < s2`, `>0` if `s1 > s2`.
pub fn vim_strnicmp(s1: &[u8], s2: &[u8], len: usize) -> i32 {
    let mut i1 = s1.iter();
    let mut i2 = s2.iter();
    for _ in 0..len {
        let c1 = i1.next().copied().unwrap_or(NUL);
        let c2 = i2.next().copied().unwrap_or(NUL);
        let diff = tolower_loc(c1 as i32) - tolower_loc(c2 as i32);
        if diff != 0 {
            return diff; // this character different
        }
        if c1 == NUL {
            break; // strings match until NUL
        }
    }
    0 // strings match
}

/// Case-insensitive [`crate::memory::strequal`] (`striequal`).
pub fn striequal(a: Option<&[u8]>, b: Option<&[u8]>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => vim_stricmp(a, b) == 0,
        _ => false,
    }
}

/// Compare two ASCII strings for length `len`, ignoring case, ignoring
/// locale (`vim_strnicmp_asc`).
///
/// Returns `0` for a match, `<0` if `s1 < s2`, `>0` if `s1 > s2`.
pub fn vim_strnicmp_asc(s1: &[u8], s2: &[u8], len: usize) -> i32 {
    use crate::macros_defs::tolower_asc;
    let mut i1 = s1.iter();
    let mut i2 = s2.iter();
    let mut i = 0;
    for _ in 0..len {
        let c1 = i1.next().copied().unwrap_or(NUL);
        let c2 = i2.next().copied().unwrap_or(NUL);
        i = tolower_asc(c1 as i32) - tolower_asc(c2 as i32);
        if i != 0 {
            break; // this character is different
        }
        if c1 == NUL {
            break; // strings match until NUL
        }
    }
    i
}

/// Sort an array of strings (`sort_strings`). The original sorts in place
/// via `qsort`+`strcmp`; `sort_unstable` is Rust's native equivalent
/// (`strcmp`-style byte-lexicographic ordering is exactly `Ord` for
/// `[u8]`/`Vec<u8>`, and `qsort` never claimed stability either).
pub fn sort_strings(files: &mut [Vec<u8>]) {
    files.sort_unstable();
}

/// Returns true if `s` contains a non-ASCII byte (128 or higher)
/// (`has_non_ascii`/`has_non_ascii_len` - unified, since `&[u8]` always
/// carries its own length; `None`/absent input returns false like the
/// original's `NULL` case).
pub fn has_non_ascii(s: Option<&[u8]>) -> bool {
    match s {
        Some(s) => s.iter().any(|&b| b >= 128),
        None => false,
    }
}

/// Concatenate two strings and return the result in newly allocated memory
/// (`concat_str`).
pub fn concat_str(str1: &[u8], str2: &[u8]) -> Vec<u8> {
    let mut dest = Vec::with_capacity(str1.len() + str2.len());
    dest.extend_from_slice(str1);
    dest.extend_from_slice(str2);
    dest
}

/// ASCII lower-to-upper case translation, language independent, in
/// place (`vim_strup`).
///
/// Stops at the first embedded NUL byte, matching the original's own
/// `while ((c = *p) != NUL)` loop exactly (a genuine NUL-terminated C
/// string never has meaningful content past its first NUL) - anything
/// in `p` from that point on is left untouched, not uppercased.
pub fn vim_strup(p: &mut [u8]) {
    for c in p.iter_mut() {
        if *c == NUL {
            break;
        }
        if c.is_ascii_lowercase() {
            *c -= 0x20;
        }
    }
}

/// Like [`xstrnsave`], but make all characters uppercase using ASCII
/// lower-to-upper case translation, language independent
/// (`vim_strsave_up`; also serves the role of the original's
/// `vim_strnsave_up`/`vim_strcpy_up`/`vim_strncpy_up`/`vim_memcpy_up`,
/// since a Rust `&[u8]` slice already knows its own exact length -
/// there's no separate "whole NUL-terminated string" vs. "first `n`
/// bytes" variant to keep apart from the length-taking one).
///
/// Unlike [`xstrnsave`] (a lower-level "copy exactly N bytes, embedded
/// NULs included" primitive taking an explicit length), this - like
/// [`vim_strup`] - is self-bounding via NUL-scanning, matching the
/// original's own `strlen(string)`-sized allocation: the result is
/// truncated at `string`'s first embedded NUL (if any), then
/// NUL-terminated there, not a verbatim same-length copy.
#[must_use]
pub fn vim_strsave_up(string: &[u8]) -> Vec<u8> {
    let end = string.iter().position(|&b| b == NUL).unwrap_or(string.len());
    let mut result = string[..end].to_vec();
    vim_strup(&mut result);
    result.push(NUL);
    result
}

/// Multi-byte uppercase `src`, returning a newly allocated result
/// (`mb_strup_buf`).
///
/// Deviates from the original's `char *dst` out-parameter (which the
/// caller must pre-size to `strlen(src) * MB_MAXBYTES + 1` in the
/// worst case): returns a freshly, exactly sized `Vec<u8>` instead,
/// sidestepping that sizing concern entirely. Matches the original's
/// own explicit NUL-termination (`dst[i] = NUL;`): the returned
/// `Vec<u8>` includes a trailing NUL byte, same as this crate's other
/// `strings.c` functions (e.g. [`xstrnsave`]) and the original's own
/// NUL-terminated-C-string representation.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// [`crate::mbyte::mb_toupper`]) - same requirement as every other
/// function that does so.
#[must_use]
pub unsafe fn mb_strup_buf(src: &[u8]) -> Vec<u8> {
    let mut dst = Vec::with_capacity(src.len() + 1);
    let mut p = 0usize;
    while p < src.len() && src[p] != NUL {
        let ci = crate::mbyte::utf_ptr2char_info(&src[p..]);
        let c = if ci.value < 0 { i32::from(src[p]) } else { ci.value };
        // SAFETY: forwarded from this function's own safety doc.
        let upper = unsafe { crate::mbyte::mb_toupper(c) };
        let mut buf = [0u8; crate::mbyte_defs::MB_MAXBYTES];
        let n = crate::mbyte::utf_char2bytes(upper, &mut buf) as usize;
        dst.extend_from_slice(&buf[..n]);
        p += ci.len;
    }
    dst.push(NUL);
    dst
}

/// Make given string all upper-case or all lower-case, returning a
/// newly allocated result (`strcase_save`).
///
/// Handles multi-byte characters as good as possible. Matches the
/// original's own explicit NUL-termination (`res[res_index] = NUL;`):
/// the returned `Vec<u8>` includes a trailing NUL byte, same as
/// [`mb_strup_buf`] above.
///
/// @param upper If true make uppercase, otherwise lowercase.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// [`crate::mbyte::mb_toupper`]/[`crate::mbyte::mb_tolower`]).
#[must_use]
pub unsafe fn strcase_save(orig: &[u8], upper: bool) -> Vec<u8> {
    let mut res = Vec::with_capacity(orig.len() + 1);
    let mut p = 0usize;
    while p < orig.len() && orig[p] != NUL {
        let char_info = crate::mbyte::utf_ptr2char_info(&orig[p..]);
        let c = if char_info.value < 0 { i32::from(orig[p]) } else { char_info.value };
        // SAFETY: forwarded from this function's own safety doc.
        let newc = unsafe { if upper { crate::mbyte::mb_toupper(c) } else { crate::mbyte::mb_tolower(c) } };

        let mut buf = [0u8; crate::mbyte_defs::MB_MAXBYTES];
        let newl = crate::mbyte::utf_char2bytes(newc, &mut buf) as usize;
        res.extend_from_slice(&buf[..newl]);
        p += char_info.len;
    }
    res.push(NUL);
    res
}

/// `strchr()` version which handles multibyte strings (`vim_strchr`).
///
/// @param string  String to search in.
/// @param c  Character to search for.
///
/// @return the byte offset of the first occurrence of character `c` in
/// `string`, or `None` if it was not found or the character is invalid.
/// The NUL character is never found (matching the original's own
/// documented caveat - use `.len()` instead), and the scan never looks
/// past the first embedded NUL (matching the original's own
/// NUL-terminated-C-string `strchr`/`strstr` semantics, since a Rust
/// `&[u8]` has no implicit terminator of its own).
#[must_use]
pub fn vim_strchr(string: &[u8], c: i32) -> Option<usize> {
    if c <= 0 {
        return None;
    }

    let end = string.iter().position(|&b| b == NUL).unwrap_or(string.len());
    let string = &string[..end];

    if c < 0x80 {
        return string.iter().position(|&b| b == c as u8);
    }

    let mut u8char = [0u8; crate::mbyte_defs::MB_MAXCHAR];
    let len = crate::mbyte::utf_char2bytes(c, &mut u8char) as usize;
    let needle = &u8char[..len];
    if needle.is_empty() || needle.len() > string.len() {
        return None;
    }
    string.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xstrnsave_pads_short_strings_and_truncates_long_ones() {
        let v = xstrnsave(b"ab", 5);
        assert_eq!(v.len(), 6);
        assert_eq!(&v[..2], b"ab");
        assert!(v[2..].iter().all(|&b| b == 0));

        let v2 = xstrnsave(b"abcdef", 3);
        assert_eq!(v2.len(), 4);
        assert_eq!(&v2[..3], b"abc");
    }

    #[test]
    fn vim_stricmp_ignores_case() {
        assert_eq!(vim_stricmp(b"Hello", b"hello"), 0);
        assert_ne!(vim_stricmp(b"Hello", b"World"), 0);
        assert_eq!(vim_stricmp(b"abc", b"abc"), 0);
    }

    #[test]
    fn vim_strnicmp_bounds_by_len() {
        assert_eq!(vim_strnicmp(b"ABCxyz", b"abcXYZ", 3), 0); // "ABC" vs "abc" ci-equal within len=3
        assert_eq!(vim_strnicmp(b"ABCxyz", b"abcXYZ", 6), 0); // full ci-equal too
        assert_ne!(vim_strnicmp(b"ABCabc", b"ABCxyz", 6), 0); // genuinely differs
    }

    #[test]
    fn striequal_handles_none_like_strequal() {
        assert!(striequal(None, None));
        assert!(!striequal(None, Some(b"a")));
        assert!(striequal(Some(b"ABC"), Some(b"abc")));
    }

    #[test]
    fn vim_strnicmp_asc_is_locale_independent() {
        assert_eq!(vim_strnicmp_asc(b"ABC", b"abc", 3), 0);
    }

    #[test]
    fn sort_strings_sorts_lexicographically() {
        let mut v = vec![b"banana".to_vec(), b"apple".to_vec(), b"cherry".to_vec()];
        sort_strings(&mut v);
        assert_eq!(v, vec![b"apple".to_vec(), b"banana".to_vec(), b"cherry".to_vec()]);
    }

    #[test]
    fn has_non_ascii_detects_high_bytes() {
        assert!(!has_non_ascii(Some(b"hello")));
        assert!(has_non_ascii(Some(&[b'h', 200, b'i'])));
        assert!(!has_non_ascii(None));
    }

    #[test]
    fn concat_str_joins_without_separator() {
        assert_eq!(concat_str(b"foo", b"bar"), b"foobar");
    }

    #[test]
    fn vim_strup_uppercases_ascii_letters_in_place() {
        let mut s = b"Hello, World! 123\0".to_vec();
        vim_strup(&mut s);
        assert_eq!(&s, b"HELLO, WORLD! 123\0");
    }

    #[test]
    fn vim_strup_stops_at_first_embedded_nul() {
        let mut s = b"ab\0cd".to_vec(); // 'c'/'d' come after an embedded NUL
        vim_strup(&mut s);
        assert_eq!(&s, b"AB\0cd"); // untouched past the NUL
    }

    #[test]
    fn vim_strsave_up_returns_nul_terminated_uppercase_copy() {
        assert_eq!(vim_strsave_up(b"hello\0"), b"HELLO\0");
    }

    #[test]
    fn vim_strsave_up_truncates_at_first_embedded_nul() {
        // Matches the original's own strlen()-based sizing: content
        // past the first NUL isn't part of the "real" string at all,
        // so the result is truncated there (not just left unmodified).
        assert_eq!(vim_strsave_up(b"ab\0cd"), b"AB\0");
    }

    #[test]
    fn mb_strup_buf_uppercases_ascii_and_multibyte() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: touches OPTION_VARS via mb_toupper, guarded above.
        let result = unsafe { mb_strup_buf("héllo\0".as_bytes()) };
        assert_eq!(result, "HÉLLO\0".as_bytes());
    }

    #[test]
    fn mb_strup_buf_stops_at_first_embedded_nul() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: forwarded, guarded above.
        let result = unsafe { mb_strup_buf(b"ab\0cd") };
        assert_eq!(result, b"AB\0");
    }

    #[test]
    fn strcase_save_uppercases_when_requested() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: touches OPTION_VARS via mb_toupper, guarded above.
        let result = unsafe { strcase_save("héllo\0".as_bytes(), true) };
        assert_eq!(result, "HÉLLO\0".as_bytes());
    }

    #[test]
    fn strcase_save_lowercases_when_requested() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: touches OPTION_VARS via mb_tolower, guarded above.
        let result = unsafe { strcase_save("HÉLLO\0".as_bytes(), false) };
        assert_eq!(result, "héllo\0".as_bytes());
    }

    #[test]
    fn vim_strchr_finds_ascii_byte() {
        assert_eq!(vim_strchr(b"hello\0", i32::from(b'l')), Some(2));
    }

    #[test]
    fn vim_strchr_not_found_returns_none() {
        assert_eq!(vim_strchr(b"hello\0", i32::from(b'z')), None);
    }

    #[test]
    fn vim_strchr_never_finds_nul() {
        assert_eq!(vim_strchr(b"hello\0", 0), None);
    }

    #[test]
    fn vim_strchr_rejects_negative_c() {
        assert_eq!(vim_strchr(b"hello\0", -1), None);
    }

    #[test]
    fn vim_strchr_stops_at_first_embedded_nul() {
        // "z" only appears after the embedded NUL - matching real
        // strchr()'s own NUL-terminated-string semantics, it must not
        // be found.
        assert_eq!(vim_strchr(b"ab\0z", i32::from(b'z')), None);
    }

    #[test]
    fn vim_strchr_finds_multibyte_character() {
        // "héllo\0": h=1 byte, é=2 bytes (U+00E9), so 'é' starts at
        // byte offset 1.
        assert_eq!(vim_strchr("héllo\0".as_bytes(), 0xe9), Some(1));
    }

    #[test]
    fn vim_strchr_multibyte_not_found() {
        assert_eq!(vim_strchr("hello\0".as_bytes(), 0xe9), None);
    }
}
