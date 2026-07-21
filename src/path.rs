//! Translated from `src/nvim/path.c` (partial).
//!
//! `path.c` is large (75.7KB) and most of it needs filesystem I/O
//! (`os/fs.c`), option state (`'fileignorecase'` etc., `option.c`), or
//! multi-byte-aware case folding (`mbyte.c`) - none translated yet. Only
//! the pure, in-memory path-string functions with no such dependency are
//! translated here: `vim_ispathsep`(+`_nocolon`), `vim_ispathlistsep`,
//! `path_head_length`, `is_path_head`, `path_skip_sep`, `get_past_head`,
//! `path_tail`, `path_next_component`, `path_has_drive_letter`,
//! `path_is_absolute`, `after_pathsep`, `add_pathsep`.
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
//! Deferred: everything requiring filesystem access, options, or
//! multibyte case-folding, including `path_fnamecmp`/`path_fnamencmp`
//! (needed by `garray.c`'s `ga_remove_duplicate_strings` - on Windows
//! these need `'fileignorecase'`, `_getdrive()`, and `utf_fold`, a deeper
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
}
