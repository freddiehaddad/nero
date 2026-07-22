//! Translated from `src/nvim/path.c` (partial).
//!
//! `path.c` is large (75.7KB) and most of it needs filesystem I/O
//! (`os/fs.c`), option state (`'fileignorecase'` etc., `option.c`), or
//! multi-byte-aware case folding (`mbyte.c`). Now that `os/fs.rs`
//! translates a synchronous-file-op core (`os_dirname`/`os_realpath`/
//! etc.) and `mbyte.c`'s FFI-dependent functions exist, the pure-string
//! functions plus the option-/multibyte-dependent ones built directly
//! on top of them are translated here: `vim_ispathsep`(+`_nocolon`),
//! `vim_ispathlistsep`, `path_head_length`, `is_path_head`,
//! `path_skip_sep`, `get_past_head`, `path_tail`, `path_tail_with_sep`,
//! `path_next_component`, `path_has_drive_letter`, `path_is_absolute`,
//! `after_pathsep`, `add_pathsep`, `path_is_url`, `path_with_url`,
//! `path_to_slash`, `path_to_slash_save`, `append_path`,
//! `path_full_dir_name`, `path_to_absolute`, `vim_full_name`
//! (`vim_FullName`), `full_name_save` (`FullName_save`),
//! `save_abs_path`, `path_fnamencmp`, `path_fnamecmp`, `pathcmp`,
//! `path_shorten_fname`, `path_try_shorten_fname`, `same_directory`
//! (now tractable now that `path_tail_with_sep` exists too - its
//! nullable `char *f1, char *f2` parameters are modeled as
//! `Option<&[u8]>` since, unlike most other functions here, the
//! original has no `FUNC_ATTR_NONNULL_*` attribute and explicitly
//! null-checks both).
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
//! `path_fnamencmp`/`path_fnamecmp`/`pathcmp` needed `mbyte.c`'s
//! `utf_ptr2char`/`utfc_ptr2len`/`utf_fold`/`mb_toupper`/`utf_strnicmp`/
//! `mb_strnicmp` (all now translated) plus a hand-written FFI to the
//! MSVC CRT's `_getdrive()` (Windows-only, see `win32_getdrive`). Their
//! Windows (`BACKSLASH_IN_FILENAME`) and non-Windows implementations
//! are genuinely different algorithms in the original (Windows: a
//! per-character, drive-letter- and separator-aware walk; elsewhere:
//! plain `strncmp`/`mb_strnicmp`, since `\\` isn't a separator there) -
//! translated as such, not unified into one shared code path.
//! `path_fnamencmp`'s Windows-only drive-letter pre-processing loop
//! (comparing an explicit-drive path like `C:\xxx` against an
//! implicit-drive one like `\xxx`) restores the pre-advance pointer by
//! saving and returning to it directly rather than replicating the
//! original's `p2 -= utfc_ptr2len(p2)` retreat-by-remeasuring - behaves
//! identically for every realistic input (a drive letter is always
//! followed by a single ASCII byte) and is strictly safer (a true
//! restore, not pointer arithmetic that could otherwise land off a
//! character boundary for a pathological input).
//!
//! Deferred: everything else requiring options, multibyte case
//! folding, or subsystems not yet ported (wildcard expansion,
//! `'suffixes'`/`'wildignore'`, etc.).

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

/// Gets the offset of the tail of `fname`, including path separators
/// (`path_tail_with_sep`).
///
/// Takes care of `"c:/"` and `"//"`. That means
/// `path_tail_with_sep(b"dir///file.txt")` returns the offset of
/// `"///file.txt"`.
///
/// @return the offset of the last path separator of `fname`, if there
/// is any; `0` (i.e. `fname` itself) if it contains no path separator.
#[must_use]
pub fn path_tail_with_sep(fname: &[u8]) -> usize {
    // Don't remove the '/' from "c:/file".
    let past_head = get_past_head(fname);
    let mut tail = path_tail(fname);
    while tail > past_head && after_pathsep(fname, tail) {
        tail -= 1;
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

/// [`crate::mbyte::utf_ptr2char`], but treating an empty slice as the C
/// original's implicit NUL terminator (returning 0/`NUL`) instead of
/// panicking. Needed because the byte slices in this module are plain
/// filename buffers, not NUL-terminated C strings, yet several
/// functions here (mirroring the original's own pointer walks) need to
/// read "one past the end" and see a NUL exactly like the original
/// does.
fn utf_ptr2char_or_nul(p: &[u8]) -> i32 {
    if p.is_empty() { 0 } else { crate::mbyte::utf_ptr2char(p) }
}

/// Hand-written FFI to the MSVC C runtime's `_getdrive()` (declared in
/// `<direct.h>`), needed by [`path_fnamencmp`]'s Windows-only
/// drive-letter handling. No new crate dependency: like this crate's
/// other hand-written Win32 FFI (see `os/proc.rs`, `os/users.rs`), a
/// single narrowly-scoped `extern` declaration is used instead. Unlike
/// those, `_getdrive()` is a plain CRT function (cdecl), not a
/// `kernel32.dll` "system"-call API, and needs no explicit `#[link]`
/// attribute: every Rust binary built for `*-pc-windows-msvc` already
/// links the C runtime that provides it (verified with a standalone
/// scratch `cargo run` probe during translation).
///
/// Returns the current default drive number: 1 = A, 2 = B, 3 = C, etc.
#[cfg(windows)]
fn win32_getdrive() -> i32 {
    extern "C" {
        fn _getdrive() -> i32;
    }
    // SAFETY: _getdrive() takes no arguments, has no preconditions, and
    // cannot fail.
    unsafe { _getdrive() }
}

/// Compare two file names, handling `'fileignorecase'` and (per
/// `BACKSLASH_IN_FILENAME`, Windows-only) treating `/`/`\` as
/// equivalent (the per-character loop used by the Windows variant of
/// [`path_fnamencmp`], after its drive-letter pre-processing).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (for `'fileignorecase'`).
#[cfg(windows)]
unsafe fn path_fnamencmp_loop(mut p1: &[u8], mut p2: &[u8], mut len: usize) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let p_fic = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_fic;
    let mut c1 = 0i32;
    let mut c2 = 0i32;

    while len > 0 {
        c1 = utf_ptr2char_or_nul(p1);
        c2 = utf_ptr2char_or_nul(p2);
        if c1 == 0
            || c2 == 0
            || (c1 != c2
                && ((c1 != i32::from(b'/') && c1 != i32::from(b'\\'))
                    || (c2 != i32::from(b'/') && c2 != i32::from(b'\\')))
                && (p_fic == 0 || crate::mbyte::utf_fold(c1) != crate::mbyte::utf_fold(c2)))
        {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc
        // (utfc_ptr2len touches OPTION_VARS transitively).
        let step1 = unsafe { crate::mbyte::utfc_ptr2len(p1) } as usize;
        // SAFETY: same as above.
        let step2 = unsafe { crate::mbyte::utfc_ptr2len(p2) } as usize;
        // The original decrements `len` (a C `size_t`) unconditionally,
        // which would wrap around to a huge value if a single
        // character's byte length ever exceeded the remaining `len` -
        // unreachable for this function's two current callers (`len`
        // is always derived from one of the compared strings' own full
        // byte length, so it always lands on a character boundary
        // within that string), but `saturating_sub` is used here
        // rather than blindly replicating unsigned wraparound, which
        // would silently make the length bound meaningless for any
        // future caller that passes a `len` splitting a multi-byte
        // character.
        len = len.saturating_sub(step1);
        p1 = &p1[step1..];
        p2 = &p2[step2..];
    }

    if p_fic != 0 {
        crate::mbyte::utf_fold(c1) - crate::mbyte::utf_fold(c2)
    } else {
        c1 - c2
    }
}

/// A byte-oriented `strncmp(s1, s2, n)` equivalent: compares up to `n`
/// bytes, stopping early at the first difference or an embedded NUL in
/// either slice (matching a NUL-terminated C string's own natural
/// bound - `fname1`/`fname2` here are plain byte slices, not
/// guaranteed NUL-terminated, but no real path ever contains an
/// embedded NUL, so this only matters for the "ran off the end of a
/// too-short slice" case, treated identically to hitting a real NUL).
///
/// Returns 0 if equal, the signed byte-value difference otherwise.
#[cfg(not(windows))]
fn strncmp_bytes(s1: &[u8], s2: &[u8], n: usize) -> i32 {
    for k in 0..n {
        let b1 = s1.get(k).copied().unwrap_or(0);
        let b2 = s2.get(k).copied().unwrap_or(0);
        if b1 != b2 {
            return i32::from(b1) - i32::from(b2);
        }
        if b1 == 0 {
            return 0; // both ended (via NUL or slice end) at the same spot
        }
    }
    0
}

/// Compare two file names, handling `'/'` and `'\\'` correctly and
/// dealing with `'fileignorecase'` (`path_fnamencmp`). Compares at most
/// `len` bytes.
///
/// Note: does not account for maximum name lengths and things like
/// `"../dir"`, thus it is not 100% accurate. The OS may also use a
/// different algorithm for case-insensitive comparison.
///
/// @return 0 if they are equal, non-zero otherwise.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (for `'fileignorecase'`
/// and, on Windows, [`crate::mbyte::mb_toupper`]) - same requirement as
/// every other function that does so.
#[must_use]
pub unsafe fn path_fnamencmp(fname1: &[u8], fname2: &[u8], len: usize) -> i32 {
    // BACKSLASH_IN_FILENAME is defined only on Windows (win_defs.h);
    // on every other platform '\\' is just an ordinary filename byte,
    // and the original uses a completely different, much simpler
    // implementation (plain strncmp/mb_strnicmp - no per-character,
    // separator-aware walk at all).
    #[cfg(windows)]
    {
        let (mut p1, mut p2): (&[u8], &[u8]) = (fname1, fname2);

        // To allow proper comparison of absolute paths:
        //   - one with explicit drive letter C:\xxx
        //   - another with implicit drive letter \xxx
        // advance the pointer, of the explicit one, to skip the drive.
        for _swap in 0..2 {
            let c1 = utf_ptr2char_or_nul(p1);
            let mut c2 = utf_ptr2char_or_nul(p2);

            if (c1 == i32::from(b'/') || c1 == i32::from(b'\\'))
                && crate::macros_defs::ascii_isalpha(c2)
            {
                // SAFETY: forwarded from this function's own safety doc.
                let drive = unsafe { crate::mbyte::mb_toupper(c2) } - i32::from(b'A') + 1;

                // Check for the colon.
                let before_colon_check = p2;
                // SAFETY: forwarded from this function's own safety doc.
                let advance = unsafe { crate::mbyte::utfc_ptr2len(p2) } as usize;
                p2 = &p2[advance..];
                c2 = utf_ptr2char_or_nul(p2);
                let current_drive = win32_getdrive();
                if c2 == i32::from(b':') && drive == current_drive {
                    // skip the drive for comparison
                    // SAFETY: forwarded from this function's own safety doc.
                    let advance2 = unsafe { crate::mbyte::utfc_ptr2len(p2) } as usize;
                    p2 = &p2[advance2..];
                    break;
                }
                // ignore: undo the "check for colon" advance. The
                // original does this via `p2 -= utfc_ptr2len(p2)`
                // (re-measuring the length of whatever follows the
                // presumed drive letter); restoring the exact
                // previously-saved slice instead is equivalent for
                // every realistic input (the two lengths always match
                // when what follows the letter is itself a single
                // ASCII byte, as any real path separator/colon is),
                // and is strictly safer than replicating pointer
                // arithmetic that could otherwise land on a
                // non-character boundary.
                p2 = before_colon_check;
            }

            std::mem::swap(&mut p1, &mut p2);
        }

        // SAFETY: forwarded from this function's own safety doc.
        unsafe { path_fnamencmp_loop(p1, p2, len) }
    }

    #[cfg(not(windows))]
    {
        // SAFETY: forwarded from this function's own safety doc.
        let p_fic = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_fic;
        if p_fic != 0 {
            crate::mbyte::mb_strnicmp(fname1, fname2, len)
        } else {
            strncmp_bytes(fname1, fname2, len)
        }
    }
}

/// Compare two file names (`path_fnamecmp`).
///
/// On some systems case in a file name does not matter, on others it
/// does.
///
/// Handles `'/'` and `'\\'` correctly and deals with `'fileignorecase'`.
///
/// @return 0 if they are equal, non-zero otherwise.
///
/// # Safety
/// Same as [`path_fnamencmp`] (Windows), or [`pathcmp`] (elsewhere).
#[must_use]
pub unsafe fn path_fnamecmp(fname1: &[u8], fname2: &[u8]) -> i32 {
    #[cfg(windows)]
    {
        let len = std::cmp::max(fname1.len(), fname2.len());
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { path_fnamencmp(fname1, fname2, len) }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { pathcmp(fname1, fname2, None) }
    }
}

/// Compare path `p` to `q` (`pathcmp`).
///
/// @param maxlen If `Some`, compare at most `maxlen` bytes of each;
///        `None` compares the whole of both.
///
/// Return value like `strcmp(p, q)`, but consider path separators (a
/// trailing slash is ignored, and - when built with
/// `BACKSLASH_IN_FILENAME` - `/` and `\` compare equal).
///
/// See also [`crate::path::path_fnamencmp`].
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (for `'fileignorecase'`
/// and [`crate::mbyte::mb_toupper`]) - same requirement as every other
/// function that does so.
#[must_use]
pub unsafe fn pathcmp(p: &[u8], q: &[u8], maxlen: Option<usize>) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let p_fic = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_fic;

    let mut i = 0usize;
    let mut j = 0usize;
    // `s` remembers which of `p`/`q` "won" (the one that didn't end
    // first, or that ended with just a trailing slash) - `None` means
    // neither has been determined yet (still scanning), matching the
    // original's `s == NULL` sentinel.
    let mut s: Option<&[u8]> = None;

    loop {
        if let Some(maxlen) = maxlen {
            if i >= maxlen || j >= maxlen {
                break;
            }
        }

        let c1 = utf_ptr2char_or_nul(&p[i.min(p.len())..]);
        let c2 = utf_ptr2char_or_nul(&q[j.min(q.len())..]);

        // End of "p": check if "q" also ends or just has a slash.
        if c1 == 0 {
            if c2 == 0 {
                return 0; // full match
            }
            s = Some(q);
            i = j;
            break;
        }

        // End of "q": check if "p" just has a slash.
        if c2 == 0 {
            s = Some(p);
            break;
        }

        // SAFETY: forwarded from this function's own safety doc.
        let differs = if p_fic != 0 {
            unsafe { crate::mbyte::mb_toupper(c1) != crate::mbyte::mb_toupper(c2) }
        } else {
            c1 != c2
        };
        #[cfg(windows)]
        let differs = differs
            && !((c1 == i32::from(b'/') && c2 == i32::from(b'\\'))
                || (c1 == i32::from(b'\\') && c2 == i32::from(b'/')));

        if differs {
            if vim_ispathsep(c1) {
                return -1;
            }
            if vim_ispathsep(c2) {
                return 1;
            }
            // SAFETY: forwarded from this function's own safety doc.
            return if p_fic != 0 {
                unsafe { crate::mbyte::mb_toupper(c1) - crate::mbyte::mb_toupper(c2) }
            } else {
                c1 - c2
            };
        }

        // SAFETY: forwarded from this function's own safety doc.
        i += unsafe { crate::mbyte::utfc_ptr2len(&p[i.min(p.len())..]) } as usize;
        // SAFETY: same as above.
        j += unsafe { crate::mbyte::utfc_ptr2len(&q[j.min(q.len())..]) } as usize;
    }

    let Some(s) = s else {
        return 0; // "i" or "j" ran into maxlen
    };

    let c1 = utf_ptr2char_or_nul(&s[i.min(s.len())..]);
    // SAFETY: forwarded from this function's own safety doc.
    let step = unsafe { crate::mbyte::utfc_ptr2len(&s[i.min(s.len())..]) } as usize;
    let c2 = utf_ptr2char_or_nul(&s[(i + step).min(s.len())..]);

    // ignore a trailing slash, but not "//" or ":/"
    let is_slash = if cfg!(windows) {
        c1 == i32::from(b'/') || c1 == i32::from(b'\\')
    } else {
        c1 == i32::from(b'/')
    };
    if c2 == 0 && i > 0 && !after_pathsep(s, i) && is_slash {
        return 0; // match with trailing slash
    }
    if std::ptr::eq(s, q) {
        return -1; // no match
    }
    1
}

/// True if file names `f1` and `f2` are in the same directory
/// (`same_directory`). `f1` may be a short name, `f2` must be a full
/// path.
///
/// `f1`/`f2` are `Option` rather than plain slices to faithfully model
/// the original's own nullable `char *f1, char *f2` parameters (the
/// original explicitly checks for `NULL` and returns `false` - unlike
/// most other functions in this module, it has no `FUNC_ATTR_NONNULL_*`
/// attribute at all).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via [`pathcmp`]).
#[must_use]
pub unsafe fn same_directory(f1: Option<&[u8]>, f2: Option<&[u8]>) -> bool {
    let (Some(f1), Some(f2)) = (f1, f2) else {
        return false; // safety check
    };

    let (ffname, _) = vim_full_name(f1, false);
    let t1 = path_tail_with_sep(&ffname);
    let t2 = path_tail_with_sep(f2);

    // SAFETY: forwarded from this function's own safety doc.
    t1 == t2 && unsafe { pathcmp(&ffname, f2, Some(t1)) } == 0
}

/// Try to find a shortname by comparing the fullname with `dir_name`
/// (`path_shorten_fname`).
///
/// @param full_path The full path of the file.
/// @param dir_name The directory to shorten relative to.
///
/// @return
///   - `Some` (a sub-slice into `full_path`) if shortened.
///   - `None` if no shorter name is possible.
///
/// # Safety
/// Same as [`path_fnamencmp`].
#[must_use]
pub unsafe fn path_shorten_fname<'a>(full_path: &'a [u8], dir_name: &[u8]) -> Option<&'a [u8]> {
    let len = dir_name.len();

    // If full_path and dir_name do not match, it's impossible to make
    // one relative to the other.
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { path_fnamencmp(dir_name, full_path, len) } != 0 {
        return None;
    }

    // If dir_name is a path head, full_path can always be made relative.
    if len == path_head_length() as usize && is_path_head(dir_name) {
        return Some(&full_path[len.min(full_path.len())..]);
    }

    let p = &full_path[len.min(full_path.len())..];

    // If p is not pointing to a path separator, this means that
    // full_path's last directory name is longer than dir_name's last
    // directory, so they don't actually match.
    let first = *p.first()?;
    if !vim_ispathsep(i32::from(first)) {
        return None;
    }

    // Skip the matched separator, then any following separators (but
    // not a colon).
    let skip = path_skip_sep(&p[1..], false);
    Some(&p[(1 + skip).min(p.len())..])
}

/// Try to find a shortname by comparing the fullname with the current
/// directory (`path_try_shorten_fname`).
///
/// @param full_path The full path of the file.
///
/// @return
///   - A sub-slice into `full_path` if shortened.
///   - `full_path` unchanged if no shorter name is possible or the
///     current directory couldn't be determined.
///
/// # Safety
/// Same as [`path_shorten_fname`].
#[must_use]
pub unsafe fn path_try_shorten_fname(full_path: &[u8]) -> &[u8] {
    let Some(dirname) = crate::os::fs::os_dirname() else {
        return full_path;
    };
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { path_shorten_fname(full_path, &dirname) } {
        Some(p) if !p.is_empty() => p,
        _ => full_path,
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

    #[test]
    fn path_fnamencmp_equal_strings_compare_equal() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { path_fnamencmp(b"/foo/bar", b"/foo/bar", 8) }, 0);
    }

    #[test]
    fn path_fnamencmp_different_strings_compare_nonzero() {
        let _guard = crate::globals::global_state_test_lock();
        assert_ne!(unsafe { path_fnamencmp(b"/foo/bar", b"/foo/baz", 8) }, 0);
    }

    #[test]
    fn path_fnamencmp_respects_len_bound() {
        let _guard = crate::globals::global_state_test_lock();
        // Only the first 4 bytes ("/foo") are compared.
        assert_eq!(unsafe { path_fnamencmp(b"/foo/bar", b"/foo/baz", 4) }, 0);
    }

    #[test]
    fn path_fnamencmp_case_sensitive_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        // Default OptionVars::default() zero-initializes p_fic (off).
        assert_ne!(unsafe { path_fnamencmp(b"/FOO", b"/foo", 4) }, 0);
    }

    #[test]
    fn path_fnamencmp_case_insensitive_when_fileignorecase_set() {
        let _guard = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_fic;
        opts.p_fic = 1;

        assert_eq!(unsafe { path_fnamencmp(b"/FOO", b"/foo", 4) }, 0);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_fic = prev;
    }

    #[cfg(windows)]
    #[test]
    fn path_fnamencmp_windows_treats_forward_and_back_slash_as_equal() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { path_fnamencmp(b"a/b", b"a\\b", 3) }, 0);
    }

    #[cfg(windows)]
    #[test]
    fn path_fnamencmp_windows_matches_explicit_and_implicit_drive_when_current() {
        let _guard = crate::globals::global_state_test_lock();
        // win32_getdrive() returns the CWD's drive; build an explicit
        // path using that same drive letter so it should compare equal
        // to the implicit-drive form.
        let drive = win32_getdrive();
        let letter = (b'A' + (drive - 1) as u8) as char;
        let explicit = format!("{letter}:/foo/bar");
        assert_eq!(
            unsafe { path_fnamencmp(b"/foo/bar", explicit.as_bytes(), 8) },
            0
        );
    }

    #[test]
    fn path_fnamecmp_equal_and_different() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { path_fnamecmp(b"/a/b", b"/a/b") }, 0);
        assert_ne!(unsafe { path_fnamecmp(b"/a/b", b"/a/c") }, 0);
    }

    #[test]
    fn pathcmp_equal_paths_match() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { pathcmp(b"/a/b", b"/a/b", None) }, 0);
    }

    #[test]
    fn pathcmp_trailing_slash_still_matches() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { pathcmp(b"/a/b/", b"/a/b", None) }, 0);
        assert_eq!(unsafe { pathcmp(b"/a/b", b"/a/b/", None) }, 0);
    }

    #[test]
    fn pathcmp_double_slash_does_not_match_via_trailing_slash_rule() {
        let _guard = crate::globals::global_state_test_lock();
        // "//' is not just a single trailing separator, so this must
        // not spuriously match like a single trailing slash would.
        assert_ne!(unsafe { pathcmp(b"/a/b//", b"/a/b", None) }, 0);
    }

    #[test]
    fn pathcmp_path_separator_sorts_before_other_characters() {
        let _guard = crate::globals::global_state_test_lock();
        // "/a" vs "/ab": at the mismatch point, "/a"'s NUL-equivalent
        // position is a path separator boundary in effect; more
        // directly, comparing "/a/x" vs "/aXx" - '/' should sort
        // first.
        assert!(unsafe { pathcmp(b"/a/x", b"/aXx", None) } < 0);
    }

    #[test]
    fn pathcmp_respects_maxlen() {
        let _guard = crate::globals::global_state_test_lock();
        // Differ only after byte 3, which is excluded by maxlen.
        assert_eq!(unsafe { pathcmp(b"/ab/1", b"/ab/2", Some(3)) }, 0);
    }

    #[test]
    fn path_shorten_fname_strips_matching_directory_prefix() {
        let _guard = crate::globals::global_state_test_lock();
        let result = unsafe { path_shorten_fname(b"/home/user/project/file.txt", b"/home/user") };
        assert_eq!(result, Some(&b"project/file.txt"[..]));
    }

    #[test]
    fn path_shorten_fname_returns_none_when_not_a_prefix() {
        let _guard = crate::globals::global_state_test_lock();
        let result = unsafe { path_shorten_fname(b"/var/log/file.txt", b"/home/user") };
        assert_eq!(result, None);
    }

    #[test]
    fn path_shorten_fname_returns_none_when_last_dir_name_only_longer() {
        let _guard = crate::globals::global_state_test_lock();
        // dir_name matches as a byte prefix but doesn't end at a
        // separator boundary in full_path ("/home/user2" vs
        // "/home/user").
        let result = unsafe { path_shorten_fname(b"/home/user2/file.txt", b"/home/user") };
        assert_eq!(result, None);
    }

    #[cfg(unix)]
    #[test]
    fn path_shorten_fname_root_dir_name_always_shortens() {
        let _guard = crate::globals::global_state_test_lock();
        let result = unsafe { path_shorten_fname(b"/etc/hosts", b"/") };
        assert_eq!(result, Some(&b"etc/hosts"[..]));
    }

    #[test]
    fn path_shorten_fname_exact_match_returns_none() {
        let _guard = crate::globals::global_state_test_lock();
        // Verified against the real C source: when full_path exactly
        // equals dir_name, `p` (full_path + len) points at the NUL
        // terminator, and `vim_ispathsep(NUL)` is false - so the
        // original itself returns NULL here (there's no trailing
        // separator to skip past), not an empty-but-successful
        // shortening.
        let result = unsafe { path_shorten_fname(b"/home/user", b"/home/user") };
        assert_eq!(result, None);
    }

    #[test]
    fn path_try_shorten_fname_relative_to_cwd() {
        let _guard = crate::globals::global_state_test_lock();
        let _cwd_guard = crate::os::fs::cwd_test_lock();
        let cwd = crate::os::fs::os_dirname().unwrap();
        let mut full = cwd.clone();
        assert!(append_path(&mut full, b"file.txt", MAXPATHL as usize));

        let shortened = unsafe { path_try_shorten_fname(&full) };
        assert_eq!(shortened, b"file.txt");
    }

    #[test]
    fn path_try_shorten_fname_unrelated_path_returns_full_path_unchanged() {
        let _guard = crate::globals::global_state_test_lock();
        let _cwd_guard = crate::os::fs::cwd_test_lock();
        // A path that (almost certainly) doesn't share a prefix with
        // the cwd should come back unchanged.
        let unrelated: &[u8] = if cfg!(windows) {
            b"Z:\\definitely\\not\\the\\cwd\\file.txt"
        } else {
            b"/definitely/not/the/cwd/file.txt"
        };
        let shortened = unsafe { path_try_shorten_fname(unrelated) };
        assert_eq!(shortened, unrelated);
    }

    #[test]
    fn path_tail_with_sep_single_separator_includes_it() {
        // "dir/file.txt": path_tail alone would return the offset of
        // "file.txt" (4); path_tail_with_sep includes the separator
        // itself, so it returns the offset of "/file.txt" (3).
        assert_eq!(path_tail_with_sep(b"dir/file.txt"), 3);
    }

    #[test]
    fn path_tail_with_sep_collapses_multiple_separators() {
        // Matches this function's own doc example exactly:
        // path_tail_with_sep("dir///file.txt") returns a pointer to
        // "///file.txt" - the offset of the *first* of the three
        // consecutive separators, not just the last one.
        assert_eq!(path_tail_with_sep(b"dir///file.txt"), 3);
    }

    #[test]
    fn path_tail_with_sep_no_separator_returns_zero() {
        // "fname if it contains no path separator" - per this
        // function's own doc comment.
        assert_eq!(path_tail_with_sep(b"file.txt"), 0);
    }

    #[test]
    fn same_directory_is_false_when_either_path_is_none() {
        let _guard = crate::globals::global_state_test_lock();
        assert!(!unsafe { same_directory(None, Some(b"/a/b")) });
        assert!(!unsafe { same_directory(Some(b"/a/b"), None) });
        assert!(!unsafe { same_directory(None, None) });
    }

    #[test]
    fn same_directory_true_for_two_files_in_the_same_directory() {
        let _guard = crate::globals::global_state_test_lock();
        let _cwd_guard = crate::os::fs::cwd_test_lock();
        let cwd = crate::os::fs::os_dirname().unwrap();

        let mut f1 = cwd.clone();
        assert!(append_path(&mut f1, b"a.txt", MAXPATHL as usize));
        let mut f2 = cwd;
        assert!(append_path(&mut f2, b"b.txt", MAXPATHL as usize));

        assert!(unsafe { same_directory(Some(&f1), Some(&f2)) });
    }

    #[test]
    fn same_directory_false_for_files_in_different_directories() {
        let _guard = crate::globals::global_state_test_lock();
        let _cwd_guard = crate::os::fs::cwd_test_lock();
        let cwd = crate::os::fs::os_dirname().unwrap();

        let mut f1 = cwd.clone();
        assert!(append_path(&mut f1, b"a.txt", MAXPATHL as usize));
        let mut f2 = cwd;
        assert!(append_path(&mut f2, b"subdir", MAXPATHL as usize));
        assert!(append_path(&mut f2, b"b.txt", MAXPATHL as usize));

        assert!(!unsafe { same_directory(Some(&f1), Some(&f2)) });
    }
}
