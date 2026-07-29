//! Translated from `src/nvim/file_search.c` (tractable core only).
//!
//! `file_search.c` implements the `findfile()`/`:find` file-searching
//! engine - almost entirely dependent on the full `vim_findfile`
//! search-context/path-traversal machinery, none of which is
//! translated.
//!
//! Translated: [`vim_findfile_stopdir`], [`ff_wc_equal`],
//! [`ff_path_in_stoplist`] - small, pure string/path helpers needing
//! only already-translated pieces (`mbyte.rs`, `path.rs`,
//! `option_vars.rs`).
//!
//! Deferred: everything else - `vim_findfile_init`/`vim_findfile`/
//! `find_file_in_path`/`find_directory_in_path`/`grab_file_name`/
//! `file_name_in_line`/`vim_chdirfile`/`vim_chdir`/
//! `find_file_in_path_option`/`find_file_name_in_path`/
//! `file_name_at_cursor`, all needing the full search-context/path-
//! traversal/file-expansion machinery.

use crate::mbyte::utf_ptr2char;
use crate::path::{path_fnamencmp, vim_ispathsep};

/// Splits `buf` at the first unescaped `;` (where `\;` is an escape
/// sequence for a literal `;`), returning the de-escaped stopdir list
/// before it and the remainder after it (`vim_findfile_stopdir`).
///
/// Returns `(stopdir, rest)`: `stopdir` is `buf` up to (but not
/// including) the terminating `;`, with any `\;` sequences un-escaped
/// to a plain `;`; `rest` is `None` if there is no unescaped `;` (this
/// is the last/only stopdir segment), or `Some(...)` with everything
/// after it otherwise.
///
/// The original mutates `buf` in place (de-escaping into the same
/// allocation - always safe since de-escaping only ever shrinks or
/// preserves length) and returns a pointer into that same buffer for
/// the remainder. This instead returns a freshly-owned `Vec<u8>` for
/// the de-escaped segment and a borrowed sub-slice of `buf` for the
/// remainder - nothing here needs the two results to share the
/// original's single backing allocation.
#[must_use]
pub fn vim_findfile_stopdir(buf: &[u8]) -> (Vec<u8>, Option<&[u8]>) {
    let mut out = Vec::with_capacity(buf.len());
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == b';' {
            return (out, Some(&buf[i + 1..]));
        }
        if buf[i] == b'\\' && buf.get(i + 1) == Some(&b';') {
            out.push(b';');
            i += 2;
        } else {
            out.push(buf[i]);
            i += 1;
        }
    }
    (out, None)
}

/// Checks if two wildcard paths are equal (`ff_wc_equal`). They are
/// equal if they have the same length, compare equal character by
/// character (respecting `'fileignorecase'`), and the only difference
/// (if any) is the single byte right after a `**` - the internal
/// recursion-depth count, stored as one raw byte whose *value* is the
/// count (`"**3"` becomes the 3 bytes `**` + `0x03`, not ASCII digit
/// characters - see `file_search.c`'s own comment on this encoding).
///
/// `None` represents the original's `NULL` (both `None` is equal,
/// exactly one `None` is never equal to a real path).
///
/// The original's `s1 == s2` pointer-identity fast path is omitted -
/// it's a pure performance micro-optimization for "the exact same
/// pointer was passed twice", not a behavior difference: the
/// character-by-character comparison below already returns `true` for
/// identical content regardless.
#[must_use]
pub fn ff_wc_equal(s1: Option<&[u8]>, s2: Option<&[u8]>) -> bool {
    let (Some(s1), Some(s2)) = (s1, s2) else {
        return s1.is_none() && s2.is_none();
    };

    let fic = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_fic != 0;
    let mut prev1 = 0i32;
    let mut prev2 = 0i32;
    let mut i = 0;
    let mut j = 0;
    while i < s1.len() && j < s2.len() {
        let c1 = utf_ptr2char(&s1[i..]);
        let c2 = utf_ptr2char(&s2[j..]);

        let differs = if fic {
            // SAFETY: mb_tolower has no preconditions beyond a valid
            // codepoint-or-negative value, which utf_ptr2char always
            // returns.
            unsafe { crate::mbyte::mb_tolower(c1) != crate::mbyte::mb_tolower(c2) }
        } else {
            c1 != c2
        };
        if differs && (prev1 != i32::from(b'*') || prev2 != i32::from(b'*')) {
            return false;
        }
        prev2 = prev1;
        prev1 = c1;

        // SAFETY: `s1[i..]`/`s2[j..]` are valid, non-empty (loop
        // condition) byte slices.
        i += unsafe { crate::mbyte::utfc_ptr2len(&s1[i..]) } as usize;
        j += unsafe { crate::mbyte::utfc_ptr2len(&s2[j..]) } as usize;
    }
    i == s1.len() && j == s2.len()
}

/// Whether `path` is one of `stopdirs`, or an ANCESTOR of one, that a
/// path search should not recurse past (`ff_path_in_stoplist`).
///
/// Note the direction: since the underlying `path_fnamencmp` comparison
/// stops as soon as either string ends, a match requires `path` (the
/// shorter side) to be a byte-for-byte prefix of a `stopdirs` entry
/// (the longer side) at a path-separator boundary - so `"/home"`
/// matches a stopdir of `"/home/rks"`, not the other way around.
#[must_use]
pub fn ff_path_in_stoplist(path: &[u8], stopdirs: &[&[u8]]) -> bool {
    // eat up trailing path separators, except the first
    let mut path_len = path.len();
    while path_len > 1 && vim_ispathsep(i32::from(path[path_len - 1])) {
        path_len -= 1;
    }
    let path = &path[..path_len];

    // if no path consider it as match
    if path.is_empty() {
        return true;
    }

    for &stopdir in stopdirs {
        // match for parent directory. So '/home' also matches
        // '/home/rks'. Check for a path separator in stopdir, else
        // '/home/r' would also match '/home/rks'.
        // SAFETY: both are plain, valid byte slices.
        if unsafe { path_fnamencmp(stopdir, path, path.len()) } == 0
            && (stopdir.len() <= path.len() || vim_ispathsep(i32::from(stopdir[path.len()])))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vim_findfile_stopdir_splits_at_semicolon() {
        let (stopdir, rest) = vim_findfile_stopdir(b"/home;/usr");
        assert_eq!(stopdir, b"/home");
        assert_eq!(rest, Some(b"/usr".as_slice()));
    }

    #[test]
    fn vim_findfile_stopdir_no_semicolon_is_the_whole_thing() {
        let (stopdir, rest) = vim_findfile_stopdir(b"/home");
        assert_eq!(stopdir, b"/home");
        assert_eq!(rest, None);
    }

    #[test]
    fn vim_findfile_stopdir_unescapes_backslash_semicolon() {
        let (stopdir, rest) = vim_findfile_stopdir(b"/a\\;b;/rest");
        assert_eq!(stopdir, b"/a;b");
        assert_eq!(rest, Some(b"/rest".as_slice()));
    }

    #[test]
    fn vim_findfile_stopdir_trailing_backslash_is_safe() {
        // A lone trailing backslash: must not panic (the one-past-the-
        // end read this function's own doc discusses).
        let (stopdir, rest) = vim_findfile_stopdir(b"/home\\");
        assert_eq!(stopdir, b"/home\\");
        assert_eq!(rest, None);
    }

    #[test]
    fn vim_findfile_stopdir_empty_input() {
        let (stopdir, rest) = vim_findfile_stopdir(b"");
        assert_eq!(stopdir, b"");
        assert_eq!(rest, None);
    }

    #[test]
    fn ff_wc_equal_both_none_is_equal() {
        assert!(ff_wc_equal(None, None));
    }

    #[test]
    fn ff_wc_equal_one_none_is_not_equal() {
        assert!(!ff_wc_equal(None, Some(b"a")));
        assert!(!ff_wc_equal(Some(b"a"), None));
    }

    #[test]
    fn ff_wc_equal_identical_strings() {
        assert!(ff_wc_equal(Some(b"foo/bar"), Some(b"foo/bar")));
    }

    #[test]
    fn ff_wc_equal_different_strings() {
        assert!(!ff_wc_equal(Some(b"foo"), Some(b"bar")));
    }

    #[test]
    fn ff_wc_equal_different_lengths() {
        assert!(!ff_wc_equal(Some(b"foo"), Some(b"foobar")));
    }

    #[test]
    fn ff_wc_equal_star_counters_are_equal() {
        // The '**N' recursion-depth count is internally stored as a
        // single RAW BYTE whose VALUE is the count (see file_search.c's
        // own comment: "'**3' is transposed to '**^C'... '**76' is
        // transposed to '**N'"), not ASCII digit characters - so "**\20"
        // in ff_wc_equal's own doc comment means the 3 bytes [b'*',
        // b'*', 20u8], not 5 literal characters. Differing counter
        // bytes should be ignored since both preceding characters are
        // '*'.
        assert!(ff_wc_equal(Some(&[b'*', b'*', 20]), Some(&[b'*', b'*', 24])));
    }

    #[test]
    fn ff_wc_equal_only_exempts_the_position_right_after_double_star() {
        // A literal backslash-then-digit-characters sequence (NOT the
        // internal raw-byte encoding above) does NOT get the exemption
        // for its second digit, since prev1 is the first digit (not
        // '*') by the time the second digit is compared.
        assert!(!ff_wc_equal(Some(b"**\\20"), Some(b"**\\24")));
    }

    #[test]
    fn ff_path_in_stoplist_empty_path_matches() {
        assert!(ff_path_in_stoplist(b"", &[b"/home"]));
    }

    #[test]
    fn ff_path_in_stoplist_exact_match() {
        assert!(ff_path_in_stoplist(b"/home", &[b"/home"]));
    }

    #[test]
    fn ff_path_in_stoplist_parent_directory_matches() {
        // path_fnamencmp(stopdir, path, path.len()) stops as soon as
        // EITHER string ends, so a match requires `path` (the shorter
        // side) to be a byte-for-byte prefix of `stopdir` (the longer
        // side) - i.e. `path` is an ANCESTOR of one of the stopdirs,
        // not the other way around. Verified by direct derivation
        // against path_fnamencmp's own already-tested short-circuit-at-
        // NUL behavior before writing this assertion.
        assert!(ff_path_in_stoplist(b"/home", &[b"/home/rks"]));
    }

    #[test]
    fn ff_path_in_stoplist_prefix_without_path_sep_does_not_match() {
        // '/home/r' should NOT match a '/home/rks' stopdir - it's just
        // a string prefix, not a real ancestor directory (no path
        // separator right after it in the stopdir).
        assert!(!ff_path_in_stoplist(b"/home/r", &[b"/home/rks"]));
    }

    #[test]
    fn ff_path_in_stoplist_no_match() {
        assert!(!ff_path_in_stoplist(b"/etc", &[b"/home", b"/usr"]));
    }

    #[test]
    fn ff_path_in_stoplist_trailing_separators_are_trimmed() {
        assert!(ff_path_in_stoplist(b"/home/", &[b"/home"]));
    }
}
