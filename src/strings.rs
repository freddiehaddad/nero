//! Translated from `src/nvim/strings.c` (partial).
//!
//! `strings.c` is large (93KB) and mixes several unrelated concerns: a
//! handful of low-level byte-string utilities (translated here), a large
//! custom `vim_snprintf`/`vim_vsnprintf` implementation with positional
//! `$`-style format-argument support, and dozens of `f_*` Vimscript builtin
//! function implementations (`f_strlen`, `f_tr`, `f_trim`, etc.) that
//! belong with the eval engine (phase 5), not here. Only the low-level
//! utilities with no eval/multibyte dependency are translated in this
//! pass:
//! - `xstrnsave`, `vim_stricmp`, `vim_strnicmp`, `striequal`,
//!   `vim_strnicmp_asc`, `sort_strings`, `has_non_ascii`,
//!   `has_non_ascii_len`, `concat_str`.
//!
//! Deferred:
//! - `vim_strsave_escaped`/`_ext`, `vim_strnsave_unquoted`,
//!   `vim_strsave_shellescape`, `vim_strsave_up`/`vim_strup`/friends,
//!   `mb_strup_buf`, `strcase_save`, `del_trailing_spaces`, `vim_strchr`:
//!   need multi-byte character handling (`mbyte.c`, phase 7) and/or
//!   `charset.c`/`option.c`.
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
}
