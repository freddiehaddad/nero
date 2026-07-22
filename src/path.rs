//! Translated from `src/nvim/path.c` (partial).
//!
//! `path.c` is large (75.7KB) and most of it needs filesystem I/O
//! (`os/fs.c`), option state (`'fileignorecase'` etc., `option.c`), or
//! multi-byte-aware case folding (`mbyte.c`). Now that `os/fs.rs`
//! translates a synchronous-file-op core (`os_dirname`/`os_realpath`/
//! etc.), the pure-string functions plus a few real-filesystem-touching
//! ones built directly on top of them are translated here:
//! `vim_ispathsep`(+`_nocolon`), `vim_ispathlistsep`, `path_head_length`,
//! `is_path_head`, `path_skip_sep`, `get_past_head`, `path_tail`,
//! `path_next_component`, `path_has_drive_letter`, `path_is_absolute`,
//! `after_pathsep`, `add_pathsep`, `path_is_url`, `path_with_url`,
//! `path_to_slash`, `path_to_slash_save`, `append_path`,
//! `path_full_dir_name`, `path_to_absolute`, `vim_full_name`
//! (`vim_FullName`), `full_name_save` (`FullName_save`), `save_abs_path`.
//!
//! Several originals use `MB_PTR_ADV`/check `utf_head_off` to advance
//! multi-byte-safely. This translation intentionally scans byte-by-byte
//! instead (no `mbyte.c` dependency), which is not a behavior change for
//! any of these specific functions: they only ever test for ASCII
//! separator bytes (`/`, `\`, `:`), and no byte belonging to a multi-byte
//! UTF-8 sequence's continuation or lead bytes (`0x80..=0xFF`) can ever
//! equal one of those ASCII separator bytes - so a naive byte scan and a
//! UTF-8-character-aware scan agree on every separator position.
//!
//! `path_full_dir_name`/`path_to_absolute` convert their `&[u8]` inputs
//! to `&str`/`Path` to call into `os/fs.rs`, failing (returning `None`)
//! on invalid UTF-8 - a known, documented simplification (same
//! tradeoff already made by `os/env.rs`'s `os_getenv`), since real-world
//! paths are overwhelmingly valid UTF-8 and losslessly handling
//! arbitrary non-UTF-8 byte sequences as native paths isn't expressible
//! via safe, dependency-free `std` APIs.
//!
//! Deferred: everything else requiring options or multibyte case-
//! folding, including `path_fnamecmp`/`path_fnamencmp` (needed by
//! `garray.c`'s `ga_remove_duplicate_strings` - on Windows these need
//! `'fileignorecase'`, `_getdrive()`, and `utf_fold`, a deeper
//! dependency than initially assumed; still not translated).

use crate::os::os_defs::MAXPATHL;

/// True if `c` is a path separator (`vim_ispathsep`). Note that on Windows
/// this includes the colon.
#[cfg(unix)]
pub fn vim_ispathsep(c: i32) -> bool {
    c == b'/' as i32 // Unix has ':' inside file names
}
#[cfg(windows)]
pub fn vim_ispathsep(c: i32) -> bool {
    // BACKSLASH_IN_FILENAME is always defined on Windows (win_defs.h).
    c == b':' as i32 || c == b'/' as i32 || c == b'\\' as i32
}

/// Like [`vim_ispathsep`], but excludes the colon for MS-Windows
/// (`vim_ispathsep_nocolon`).
pub fn vim_ispathsep_nocolon(c: i32) -> bool {
    #[cfg(windows)]
    {
        vim_ispathsep(c) && c != b':' as i32
    }
    #[cfg(not(windows))]
    {
        vim_ispathsep(c)
    }
}

/// True if `c` is a path list separator (`vim_ispathlistsep`).
pub fn vim_ispathlistsep(c: i32) -> bool {
    #[cfg(unix)]
    {
        c == b':' as i32
    }
    #[cfg(not(unix))]
    {
        c == b';' as i32 // might not be right for every system...
    }
}

/// The length of the path head on the current platform (`path_head_length`):
/// 3 on Windows (`C:\`), 1 otherwise (`/`).
pub const fn path_head_length() -> i32 {
    if cfg!(windows) {
        3
    } else {
        1
    }
}

/// True if `path` begins with characters denoting the head of a path (e.g.
/// `'/'` on Linux and `'D:'` on Windows) (`is_path_head`).
pub fn is_path_head(path: &[u8]) -> bool {
    #[cfg(windows)]
    {
        !path.is_empty() && (path[0] as char).is_ascii_alphabetic() && path.get(1) == Some(&b':')
    }
    #[cfg(not(windows))]
    {
        !path.is_empty() && vim_ispathsep(path[0] as i32)
    }
}

/// Advances past consecutive path separators (`path_skip_sep`).
///
/// * `colon` - Whether `':'` counts as a separator on MS-Windows (see
///   [`vim_ispathsep`]).
///
/// Returns the offset of the first non-separator byte (or `path.len()`).
pub fn path_skip_sep(path: &[u8], colon: bool) -> usize {
    let mut i = 0;
    while i < path.len()
        && if colon {
            vim_ispathsep(path[i] as i32)
        } else {
            vim_ispathsep_nocolon(path[i] as i32)
        }
    {
        i += 1;
    }
    i
}

/// Gets the offset of one byte past the head of a path name
/// (`get_past_head`). Unix: after `"/"`; Windows: after `"c:\"`. If there is
/// no head, returns `0`.
pub fn get_past_head(path: &[u8]) -> usize {
    // `i` is only ever mutated inside the `#[cfg(windows)]` block
    // below; on other platforms that block is compiled out entirely,
    // which would otherwise make plain rustc/clippy flag `mut` as
    // unused there - suppressed since it's a real, needed `mut` on
    // Windows, not a genuine bug (verified via cross-compiling this
    // crate for `x86_64-unknown-linux-gnu`, since this Windows dev
    // machine can't otherwise catch non-Windows-only lints).
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut i = 0;
    #[cfg(windows)]
    {
        // May skip "c:"
        if is_path_head(path) {
            i = 2;
        }
    }
    i + path_skip_sep(&path[i.min(path.len())..], true)
}

/// Gets the tail (last path component) of `fname` (`path_tail`).
///
/// Examples: `"dir/file.txt"` -> `"file.txt"`; `"file.txt"` -> `"file.txt"`;
/// `"dir/"` -> `""`.
///
/// Returns the offset of one byte past the last path separator (`fname.len()`
/// if `fname` ends in a separator).
pub fn path_tail(fname: &[u8]) -> usize {
    let mut tail = get_past_head(fname);
    let mut i = tail;
    // Find last part of path.
    while i < fname.len() {
        if vim_ispathsep_nocolon(fname[i] as i32) {
            tail = i + 1;
        }
        i += 1;
    }
    tail
}

/// Gets the next separator-delimited component of a path name
/// (`path_next_component`).
///
/// Returns the offset of the first found path separator + 1, or
/// `fname.len()` if `fname` doesn't contain a path separator.
pub fn path_next_component(fname: &[u8]) -> usize {
    let mut i = 0;
    while i < fname.len() && !vim_ispathsep(fname[i] as i32) {
        i += 1;
    }
    if i < fname.len() {
        i += 1;
    }
    i
}

/// True if `p[..path_len]` starts with a drive letter (`C:`, `C|`)
/// (`path_has_drive_letter`).
pub fn path_has_drive_letter(p: &[u8], path_len: usize) -> bool {
    path_len >= 2
        && p.len() >= 2
        && (p[0] as char).is_ascii_alphabetic()
        && (p[1] == b':' || p[1] == b'|')
        && (path_len == 2
            || (p.len() > 2 && matches!(p[2], b'/' | b'\\' | b'?' | b'#')))
}

/// True if `fname` is an absolute path (`path_is_absolute`).
pub fn path_is_absolute(fname: &[u8]) -> bool {
    #[cfg(windows)]
    {
        if fname.is_empty() {
            return false;
        }
        // A name like "d:/foo" and "//server/share" is absolute
        // /foo and \foo are absolute too because Windows keeps a current drive.
        let c2 = fname.get(2).copied().unwrap_or(0) as i32;
        ((fname[0] as char).is_ascii_alphabetic()
            && fname.get(1) == Some(&b':')
            && vim_ispathsep_nocolon(c2))
            || vim_ispathsep_nocolon(fname[0] as i32)
    }
    #[cfg(not(windows))]
    {
        // UNIX: This just checks if the file name starts with '/' or '~'.
        fname.first() == Some(&b'/') || fname.first() == Some(&b'~')
    }
}

/// True if `p` (an index into `b`) points to just after a path separator
/// (`after_pathsep`). `b` must be the start of the file name.
///
/// The original also checks `utf_head_off(b, p - 1) == 0` (not landing
/// mid-multi-byte-character); omitted here since path separators are
/// always ASCII - see module docs.
pub fn after_pathsep(b: &[u8], p: usize) -> bool {
    p > 0 && p <= b.len() && vim_ispathsep(b[p - 1] as i32)
}

/// Appends a path separator to `p` if it doesn't already end with one, in
/// place (`add_pathsep`).
///
/// Returns `false` if `p` is already at `MAXPATHL` capacity and a separator
/// cannot be added (matching the original's overflow guard), `true`
/// otherwise (including when `p` is empty, matching the original).
pub fn add_pathsep(p: &mut Vec<u8>) -> bool {
    if p.is_empty() {
        return true;
    }
    if !after_pathsep(p, p.len()) {
        if p.len() >= MAXPATHL as usize {
            return false;
        }
        p.push(crate::ascii_defs::PATHSEP);
    }
    true
}

/// `path_is_url()` has found `":/"` (`URL_SLASH`).
pub const URL_SLASH: i32 = 1;
/// `path_is_url()` has found `":\"` (`URL_BACKSLASH`).
pub const URL_BACKSLASH: i32 = 2;

/// Check if the `":/"` or `":\"` of a URL is at `p` (`path_is_url`).
///
/// @return `URL_SLASH` for `"name:/"`, `URL_BACKSLASH` for `"name:\"`,
///         zero otherwise.
#[must_use]
pub fn path_is_url(p: &[u8]) -> i32 {
    // In the spec ':' is enough to recognize a scheme:
    // <https://url.spec.whatwg.org/#scheme-state>
    if p.starts_with(b":/") {
        URL_SLASH
    } else if p.starts_with(b":\\\\") {
        URL_BACKSLASH
    } else {
        0
    }
}

/// Check if `fname` starts with `"name:/"` or `"name:\"` (`path_with_url`).
///
/// @return URL_SLASH for `"name:/"`, URL_BACKSLASH for `"name:\"`, zero
///         otherwise.
#[must_use]
pub fn path_with_url(fname: &[u8]) -> i32 {
    // first character must be alpha
    let Some(&first) = fname.first() else {
        return 0;
    };
    if !crate::macros_defs::ascii_isalpha(first as i32) {
        return 0;
    }

    if path_has_drive_letter(fname, fname.len()) {
        return 0;
    }

    // check body: (alpha, digit, '+', '-', '.') following RFC3986
    let mut i = 1;
    while i < fname.len() {
        let c = fname[i];
        if crate::macros_defs::ascii_isalnum(c as i32) || c == b'+' || c == b'-' || c == b'.' {
            i += 1;
        } else {
            break;
        }
    }

    // check last char is not '+', '-', or '.'
    let last = fname[i - 1];
    if last == b'+' || last == b'-' || last == b'.' {
        return 0;
    }

    // ":/" or ":\\" must follow
    path_is_url(&fname[i..])
}

/// Convert `p` to use forward slashes in place, unless it looks like a
/// URL (`path_to_slash`).
pub fn path_to_slash(p: &mut [u8]) {
    if path_with_url(p) == 0 {
        crate::memory::memchrsub(p, b'\\', crate::ascii_defs::PATHSEP);
    }
}

/// Get an owned copy of `p` with backslashes converted to forward
/// slashes, unless it looks like a URL (`path_to_slash_save`).
#[must_use]
pub fn path_to_slash_save(p: &[u8]) -> Vec<u8> {
    let mut owned = p.to_vec();
    path_to_slash(&mut owned);
    owned
}

/// Append `to_append` to `path` with a slash in between, in place, up
/// to `max_len` bytes total (`append_path`).
///
/// @return `true` (`OK`) on success, `false` (`FAIL`) if there isn't
///         room for the appended path within `max_len`.
pub fn append_path(path: &mut Vec<u8>, to_append: &[u8], max_len: usize) -> bool {
    // Do not append empty string or a dot.
    if to_append.is_empty() || to_append == b"." {
        return true;
    }

    // Combine the path segments, separated by a slash.
    if let Some(&last) = path.last() {
        if !vim_ispathsep_nocolon(last as i32) {
            // +1 for the NUL at the end.
            if path.len() + crate::ascii_defs::PATHSEPSTR.len() + 1 > max_len {
                return false; // No space for trailing slash.
            }
            path.push(crate::ascii_defs::PATHSEP);
        }
    }

    // +1 for the NUL at the end.
    if path.len() + to_append.len() + 1 > max_len {
        return false;
    }

    path.extend_from_slice(to_append);
    true
}

/// Used by `path_to_absolute` to expand `directory` to its full path
/// (`path_full_dir_name`).
///
/// @return `Some(full_path)` on success, `None` on failure.
#[must_use]
pub fn path_full_dir_name(directory: &[u8]) -> Option<Vec<u8>> {
    if directory.is_empty() {
        return crate::os::fs::os_dirname();
    }

    if let Some(real) = crate::os::fs::os_realpath(std::path::Path::new(
        std::str::from_utf8(directory).ok()?,
    )) {
        return Some(real);
    }

    // Path does not exist (yet). For a full path fail, will use the
    // path as-is.
    if path_is_absolute(directory) {
        return None;
    }
    // For a relative path use the current directory and append the
    // directory name.
    let mut buffer = crate::os::fs::os_dirname()?;
    if !append_path(&mut buffer, directory, MAXPATHL as usize) {
        return None;
    }
    Some(buffer)
}

/// Used by [`vim_full_name`]/`fix_fname` (not yet translated) to expand
/// a filename to its full path (`path_to_absolute`).
///
/// @param force  Also expand when `fname` is already absolute.
///
/// @return `Some(full_path)` on success, `None` on failure.
fn path_to_absolute(fname: &[u8], force: bool) -> Option<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    let mut end_of_path: Vec<u8> = fname.to_vec();

    // expand it if forced or not an absolute path
    let needs_expand = force
        || !path_is_absolute(fname)
        || (cfg!(windows) && matches!(fname.first(), Some(b'/') | Some(b'\\')));

    if needs_expand {
        // Find the last path separator (or, on Windows, a drive-letter
        // colon, or the special ".." with no separator at all),
        // splitting `fname` into a "relative_directory" prefix (used to
        // resolve the base directory) and an "end_of_path" suffix
        // (appended back on at the very end).
        let mut p_idx: Option<usize> = fname.iter().rposition(|&b| b == b'/');

        if cfg!(windows) {
            if p_idx.is_none() {
                p_idx = fname.iter().rposition(|&b| b == b'\\');
            }
            if p_idx.is_none()
                && fname.len() >= 2
                && crate::macros_defs::ascii_isalpha(fname[0] as i32)
                && fname[1] == b':'
            {
                // drive letter
                p_idx = Some(1);
            }
        }

        if p_idx.is_none() && fname == b".." {
            // Handle ".." without path separators.
            p_idx = Some(2);
        }

        let relative_directory: Vec<u8>;
        match p_idx {
            Some(mut idx) => {
                if idx < fname.len()
                    && vim_ispathsep(fname[idx] as i32)
                    && fname[idx + 1..] == b".."[..]
                {
                    // For "/path/dir/.." include the "/..".
                    idx += 3;
                }
                let copy_end = (idx + 1).min(fname.len());
                relative_directory = fname[..copy_end].to_vec();
                let end_start = if idx < fname.len() && vim_ispathsep(fname[idx] as i32) {
                    idx + 1
                } else {
                    idx
                }
                .min(fname.len());
                end_of_path = fname[end_start..].to_vec();
            }
            None => {
                relative_directory = Vec::new();
                // end_of_path stays as the whole fname (set above).
            }
        }

        buf = path_full_dir_name(&relative_directory)?;
    }

    if !append_path(&mut buf, &end_of_path, MAXPATHL as usize) {
        return None;
    }
    Some(buf)
}

/// Turn `fname` into a full path (`vim_FullName`).
///
/// @param force  Also expand when `fname` is already absolute.
///
/// @return `(full_path, success)` - `full_path` is always populated,
///         falling back to `fname` (slash-converted) on failure exactly
///         like the original's own out-parameter contract; `success`
///         is `false` (`FAIL`) if expansion failed.
#[must_use]
pub fn vim_full_name(fname: &[u8], force: bool) -> (Vec<u8>, bool) {
    if path_with_url(fname) != 0 {
        return (fname.to_vec(), true);
    }

    let (mut result, ok) = match path_to_absolute(fname, force) {
        Some(full) => (full, true),
        None => (fname.to_vec(), false), // something failed; use the filename
    };
    path_to_slash(&mut result);
    (result, ok)
}

/// Get an allocated copy of the full path to `fname` (`FullName_save`).
///
/// @param force  Also expand when `fname` is already absolute.
#[must_use]
pub fn full_name_save(fname: Option<&[u8]>, force: bool) -> Option<Vec<u8>> {
    let fname = fname?;
    Some(vim_full_name(fname, force).0)
}

/// Saves the absolute path (`save_abs_path`).
///
/// @param name An absolute or relative path.
/// @return The absolute path of `name`.
#[must_use]
pub fn save_abs_path(name: &[u8]) -> Vec<u8> {
    if !path_is_absolute(name) {
        // `full_name_save` only returns `None` when its own `fname`
        // argument is `None`, which it never is here.
        full_name_save(Some(name), true).expect("Some(name) input never yields None")
    } else {
        path_to_slash_save(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ispathsep_recognizes_forward_slash_everywhere() {
        assert!(vim_ispathsep(b'/' as i32));
        assert!(!vim_ispathsep(b'a' as i32));
    }

    #[test]
    fn path_head_length_matches_platform() {
        #[cfg(windows)]
        assert_eq!(path_head_length(), 3);
        #[cfg(not(windows))]
        assert_eq!(path_head_length(), 1);
    }

    #[test]
    fn path_tail_examples_from_doc_comment() {
        assert_eq!(path_tail(b"dir/file.txt"), 4);
        assert_eq!(&b"dir/file.txt"[path_tail(b"dir/file.txt")..], b"file.txt");
        assert_eq!(&b"file.txt"[path_tail(b"file.txt")..], b"file.txt");
        assert_eq!(&b"dir/"[path_tail(b"dir/")..], b"");
    }

    #[test]
    fn path_next_component_finds_next_segment() {
        let p = b"dir/sub/file.txt";
        let next = path_next_component(p);
        assert_eq!(&p[next..], b"sub/file.txt");
        assert_eq!(path_next_component(b"noseparator"), b"noseparator".len());
    }

    #[test]
    fn path_skip_sep_skips_consecutive_separators() {
        assert_eq!(path_skip_sep(b"///foo", true), 3);
        assert_eq!(path_skip_sep(b"foo", true), 0);
    }

    #[test]
    fn path_has_drive_letter_recognizes_windows_drives() {
        assert!(path_has_drive_letter(b"C:/foo", 6));
        assert!(path_has_drive_letter(b"C:", 2));
        assert!(!path_has_drive_letter(b"/foo", 4));
        assert!(!path_has_drive_letter(b"1:/foo", 6));
    }

    #[test]
    fn after_pathsep_detects_trailing_separator() {
        assert!(after_pathsep(b"dir/", 4));
        assert!(!after_pathsep(b"dir", 3));
        assert!(!after_pathsep(b"dir/", 0));
    }

    #[test]
    fn add_pathsep_appends_when_missing() {
        let mut p = b"dir".to_vec();
        assert!(add_pathsep(&mut p));
        assert_eq!(p.last(), Some(&crate::ascii_defs::PATHSEP));

        let mut already = b"dir/".to_vec();
        let len_before = already.len();
        assert!(add_pathsep(&mut already));
        assert_eq!(already.len(), len_before); // unchanged, already has separator

        let mut empty: Vec<u8> = Vec::new();
        assert!(add_pathsep(&mut empty)); // true, but stays empty
        assert!(empty.is_empty());
    }

    #[test]
    fn path_is_url_recognizes_slash_and_backslash_schemes() {
        assert_eq!(path_is_url(b":/foo"), URL_SLASH);
        assert_eq!(path_is_url(b":\\\\foo"), URL_BACKSLASH);
        assert_eq!(path_is_url(b"foo"), 0);
        assert_eq!(path_is_url(b""), 0);
    }

    #[test]
    fn path_with_url_recognizes_http_scheme() {
        assert_eq!(path_with_url(b"http://example.com"), URL_SLASH);
        assert_eq!(path_with_url(b"file:\\\\C:\\foo"), URL_BACKSLASH);
    }

    #[test]
    fn path_with_url_rejects_plain_paths_and_drive_letters() {
        assert_eq!(path_with_url(b"/usr/local/bin"), 0);
        assert_eq!(path_with_url(b"C:/foo"), 0); // drive letter, not a URL
        assert_eq!(path_with_url(b""), 0);
        assert_eq!(path_with_url(b"1nvalid://foo"), 0); // must start with alpha
    }

    #[test]
    fn path_with_url_rejects_trailing_dot_before_colon() {
        // scheme body ending in '.', '+', or '-' right before ':' is invalid
        assert_eq!(path_with_url(b"foo.:/bar"), 0);
    }

    #[test]
    fn path_to_slash_converts_backslashes_for_non_url_paths() {
        let mut p = b"C:\\Users\\test".to_vec();
        path_to_slash(&mut p);
        assert_eq!(&p, b"C:/Users/test");
    }

    #[test]
    fn path_to_slash_leaves_urls_untouched() {
        let mut p = b"http:\\\\example.com\\path".to_vec();
        path_to_slash(&mut p);
        assert_eq!(&p, b"http:\\\\example.com\\path");
    }

    #[test]
    fn path_to_slash_save_returns_new_converted_copy() {
        let original = b"a\\b\\c".to_vec();
        let converted = path_to_slash_save(&original);
        assert_eq!(&converted, b"a/b/c");
        assert_eq!(&original, b"a\\b\\c"); // original untouched
    }

    #[test]
    fn append_path_adds_separator_when_missing() {
        let mut path = b"C:/Users".to_vec();
        assert!(append_path(&mut path, b"test", MAXPATHL as usize));
        assert_eq!(&path, b"C:/Users/test");
    }

    #[test]
    fn append_path_avoids_double_separator() {
        let mut path = b"C:/Users/".to_vec();
        assert!(append_path(&mut path, b"test", MAXPATHL as usize));
        assert_eq!(&path, b"C:/Users/test");
    }

    #[test]
    fn append_path_ignores_empty_and_dot() {
        let mut path = b"C:/Users".to_vec();
        assert!(append_path(&mut path, b"", MAXPATHL as usize));
        assert_eq!(&path, b"C:/Users");
        assert!(append_path(&mut path, b".", MAXPATHL as usize));
        assert_eq!(&path, b"C:/Users");
    }

    #[test]
    fn append_path_fails_when_exceeding_max_len() {
        let mut path = b"C:/Users".to_vec();
        assert!(!append_path(&mut path, b"test", 9)); // no room
    }

    #[test]
    fn path_full_dir_name_empty_directory_returns_cwd() {
        let _guard = crate::os::fs::cwd_test_lock();
        let cwd = path_full_dir_name(b"").unwrap();
        let real_cwd = crate::os::fs::os_dirname().unwrap();
        assert_eq!(cwd, real_cwd);
    }

    #[test]
    fn path_full_dir_name_resolves_existing_relative_dir() {
        let _guard = crate::os::fs::cwd_test_lock();
        // "." always exists and is relative; path_full_dir_name should
        // resolve it to the (absolute) current directory via
        // os_realpath.
        let resolved = path_full_dir_name(b".").unwrap();
        assert!(path_is_absolute(&resolved));
    }

    #[test]
    fn path_to_absolute_relative_filename_joins_with_cwd() {
        let _guard = crate::os::fs::cwd_test_lock();
        let cwd = crate::os::fs::os_dirname().unwrap();
        let result = path_to_absolute(b"foo.txt", false).unwrap();
        let mut expected = cwd;
        assert!(append_path(&mut expected, b"foo.txt", MAXPATHL as usize));
        assert_eq!(result, expected);
    }

    #[test]
    fn path_to_absolute_relative_subdir_joins_with_cwd() {
        let _guard = crate::os::fs::cwd_test_lock();
        let cwd = crate::os::fs::os_dirname().unwrap();
        let result = path_to_absolute(b"sub/foo.txt", false).unwrap();
        let mut expected = cwd;
        assert!(append_path(&mut expected, b"sub/", MAXPATHL as usize));
        assert!(append_path(&mut expected, b"foo.txt", MAXPATHL as usize));
        assert_eq!(result, expected);
    }

    #[test]
    fn path_to_absolute_dotdot_resolves_to_parent_of_cwd() {
        let _guard = crate::os::fs::cwd_test_lock();
        let cwd = std::env::current_dir().unwrap();
        let parent = cwd.parent();
        let Some(parent) = parent else {
            return; // cwd is filesystem root; ".." has no distinct parent to compare
        };
        let result = path_to_absolute(b"..", false).unwrap();
        let result_path = std::path::Path::new(std::str::from_utf8(&result).unwrap());
        assert_eq!(
            result_path.canonicalize().unwrap(),
            parent.canonicalize().unwrap()
        );
    }

    #[test]
    fn path_to_absolute_already_absolute_without_force_is_unchanged() {
        let _guard = crate::os::fs::cwd_test_lock();
        let abs = crate::os::fs::os_dirname().unwrap();
        let result = path_to_absolute(&abs, false).unwrap();
        assert_eq!(result, abs);
    }

    #[test]
    fn vim_full_name_url_is_passed_through_unchanged() {
        let (result, ok) = vim_full_name(b"http://example.com/foo", false);
        assert!(ok);
        assert_eq!(result, b"http://example.com/foo");
    }

    #[test]
    fn vim_full_name_relative_path_succeeds_and_slash_converts() {
        let _guard = crate::os::fs::cwd_test_lock();
        let (result, ok) = vim_full_name(b"foo.txt", false);
        assert!(ok);
        assert!(!result.contains(&b'\\'));
        assert!(path_is_absolute(&result));
    }

    #[test]
    fn full_name_save_none_input_gives_none() {
        assert_eq!(full_name_save(None, false), None);
    }

    #[test]
    fn full_name_save_relative_path_gives_absolute_result() {
        let _guard = crate::os::fs::cwd_test_lock();
        let result = full_name_save(Some(b"foo.txt"), false).unwrap();
        assert!(path_is_absolute(&result));
    }

    #[test]
    fn save_abs_path_of_already_absolute_path_is_slash_converted_copy() {
        let _guard = crate::os::fs::cwd_test_lock();
        let abs = crate::os::fs::os_dirname().unwrap();
        let result = save_abs_path(&abs);
        assert_eq!(result, abs); // already absolute and slash-normalized
    }

    #[test]
    fn save_abs_path_of_relative_path_resolves_against_cwd() {
        let _guard = crate::os::fs::cwd_test_lock();
        let result = save_abs_path(b"foo.txt");
        assert!(path_is_absolute(&result));
    }
}
