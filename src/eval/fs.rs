//! Translated from `src/nvim/eval/fs.c` (tractable core only).
//!
//! `eval/fs.c` (~1900 lines) implements filesystem-related Vimscript
//! builtins: `delete()`, `getcwd()`, `isdirectory()`, `mkdir()`,
//! `rename()`, `glob()`/`globpath()`, `readfile()`/`writefile()`, and
//! more. Most need substantial not-yet-translated machinery
//! (`delete_recursive`'s directory-tree walk,
//! `find_file_in_path_option`'s `'path'`-option search) - this file
//! starts with the smallest, most self-contained subset.
//!
//! Translated: `isdirectory()` (via the already-existing
//! [`crate::os::fs::os_isdir`]), `isabsolutepath()` (via the already-
//! existing [`crate::path::path_is_absolute`]), `delete()` for its
//! two tractable flag values (`""` - delete a file, via
//! [`crate::os::fs::os_remove`]; `"d"` - delete an empty directory,
//! via [`crate::os::fs::os_rmdir`]), `filereadable()`/
//! `filewritable()` (via the already-existing
//! [`crate::os::fs::os_file_is_readable`]/
//! [`crate::os::fs::os_file_is_writable`]), `getfsize()`/
//! `getftime()`/`getftype()` (via the already-existing
//! [`crate::os::fs::os_fileinfo`]/[`crate::os::fs::os_fileinfo_link`]
//! narrow-subset `FileInfo` - see that module's own doc comment for
//! what's NOT modeled; `getfperm()`, needing the still-deferred
//! `os_getperm` permission bits, is not translated here), `mkdir()`
//! for both its plain, single-directory case (via the already-existing
//! [`crate::os::fs::os_mkdir`]) and the `"p"` recursive-create flag
//! (via [`crate::os::fs::os_mkdir_recurse`]) - `"D"`/`"R"`
//! (deferred deletion) need the `:defer` subsystem (not yet
//! translated) and panic via `unimplemented!()` if actually
//! reached. The `"rf"` (recursive delete) flag needs
//! `delete_recursive` (a directory-tree walk, not yet translated) and
//! panics via `unimplemented!()` if actually reached. Every function
//! taking a path/name needs the byte string to be valid UTF-8 to
//! build a `Path` from (this crate's established
//! `path_full_dir_name`-style convention - see that function's own
//! body in `path.rs`), gracefully treating invalid UTF-8
//! the same as
//! a nonexistent path/name rather than panicking.
//!
//! Also translated: `getcwd()`/`haslocaldir()` (via the shared
//! `resolve_cd_scope` helper) - re-examined after this module doc's
//! own earlier note flagged "per-window/tab local-cwd tracking" as a
//! blocker, and found `WinT.w_localdir`/`TabpageT.tp_localdir`/
//! `GLOBALS.globaldir` already existed (from an earlier session's
//! `buffer_defs.rs`/`globals.rs` translation), along with
//! `crate::vim_defs::CdScope` (phase-1 foundational headers) and
//! `crate::window::find_tabpage`/`find_win_by_nr` - the blocker had
//! simply never been re-verified since. `getcwd()`'s own Windows-only
//! `slash_adjust()` post-processing step is translated alongside, in
//! `path.rs`.
//!
//! Also translated: `rename()` (via a new `vim_rename`, from
//! `fileio.c`, hosted here alongside its only caller) - the common
//! case (renaming within the same filesystem) via the already-
//! existing [`crate::os::fs::os_fileinfo`]/[`crate::os::fs::os_remove`]/
//! [`crate::os::fs::os_rename`]. Two deliberate, narrower-than-the-
//! original simplifications, documented on `vim_rename` itself: the
//! "same name" fast path uses plain byte equality instead of the
//! original's own case/separator-style-normalizing `path_fnamecmp`
//! (not yet translated), and a failed `os_rename` (typically a
//! cross-filesystem move) `unimplemented!()`s instead of falling back
//! to `vim_copyfile` (needs symlink-aware copying, not yet
//! translated) - both narrow, honestly-documented gaps rather than a
//! silently-wrong result.
//!
//! Also translated: `glob2regpat()` (via a new `file_pat_to_reg_pat`,
//! from `fileio.c`, hosted here alongside its only currently-
//! translated caller) - converts a glob-shaped file pattern (e.g.
//! `"*.c"`, `"file{one,two}.txt"`) into an equivalent Vim regex search
//! pattern. Needed [`crate::charset::vim_isfilec`], itself now
//! translated using the same fixed-default-rule shortcut as
//! `vim_isprintc`/`vim_isbreak`/`vim_isidc` (`'isfname'`'s own default
//! is a fixed, `BACKSLASH_IN_FILENAME`-conditional split, verified
//! directly against `options.lua` rather than assumed still needing
//! the real `g_chartab`). Every output was independently cross-checked
//! against a real `nvim` binary's own `glob2regpat()` before trusting
//! it, including the platform-specific Windows-only bracket-escape
//! behavior for a backslash before a valid `'isfname'` character.

use crate::eval::typval_defs::{TypvalT, TypvalValue};

/// Convert a Vimscript string's raw bytes into a `Path`, matching
/// `path.rs`'s own established `path_full_dir_name`-style convention
/// (fails gracefully, `None`, on invalid UTF-8 rather than panicking -
/// `std::path::Path` needs valid Unicode on this crate's targets).
fn bytes_to_path(name: &[u8]) -> Option<&std::path::Path> {
    std::str::from_utf8(name).ok().map(std::path::Path::new)
}

/// `isdirectory({path})` - whether `{path}` is a directory
/// (`f_isdirectory`, `eval/fs.c`), via the already-existing
/// [`crate::os::fs::os_isdir`].
pub(crate) fn f_isdirectory(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let name = crate::eval::typval::tv_get_string(&argvars[0]);
    let is_dir = bytes_to_path(&name).is_some_and(crate::os::fs::os_isdir);
    rettv.value = TypvalValue::Number(i64::from(is_dir));
}

/// `isabsolutepath({path})` - whether `{path}` is an absolute path
/// (`f_isabsolutepath`, `eval/fs.c`), via the already-existing
/// [`crate::path::path_is_absolute`].
pub(crate) fn f_isabsolutepath(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let name = crate::eval::typval::tv_get_string(&argvars[0]);
    rettv.value = TypvalValue::Number(i64::from(crate::path::path_is_absolute(&name)));
}

/// `filereadable({file})` - whether `{file}` exists, can be read, and
/// is not a directory (`f_filereadable`, `eval/fs.c`), via the
/// already-existing [`crate::os::fs::os_isdir`]/
/// [`crate::os::fs::os_file_is_readable`].
pub(crate) fn f_filereadable(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let name = crate::eval::typval::tv_get_string(&argvars[0]);
    let readable = !name.is_empty()
        && bytes_to_path(&name).is_some_and(|p| {
            !crate::os::fs::os_isdir(p) && crate::os::fs::os_file_is_readable(p)
        });
    rettv.value = TypvalValue::Number(i64::from(readable));
}

/// `filewritable({file})` - `0` if `{file}` doesn't exist or isn't
/// writable, `1` for a writable file, `2` for a writable directory
/// (`f_filewritable`, `eval/fs.c`), via the already-existing
/// [`crate::os::fs::os_file_is_writable`].
pub(crate) fn f_filewritable(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let name = crate::eval::typval::tv_get_string(&argvars[0]);
    let writable = bytes_to_path(&name).map_or(0, crate::os::fs::os_file_is_writable);
    rettv.value = TypvalValue::Number(i64::from(writable));
}

/// `getfsize({fname})` - the size in bytes of `{fname}` (`f_getfsize`,
/// `eval/fs.c`), via the already-existing [`crate::os::fs::os_fileinfo`]/
/// [`crate::os::fs::os_fileinfo_size`]. `0` for a directory, `-1` if
/// `{fname}` can't be found, `-2` on an (essentially unreachable on a
/// 64-bit target) size overflow.
pub(crate) fn f_getfsize(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let name = crate::eval::typval::tv_get_string(&argvars[0]);
    let n = match bytes_to_path(&name) {
        None => -1,
        Some(path) => match crate::os::fs::os_fileinfo(path) {
            None => -1,
            Some(_) if crate::os::fs::os_isdir(path) => 0,
            Some(info) => {
                let size = crate::os::fs::os_fileinfo_size(&info);
                let signed = size as i64;
                if signed as u64 == size { signed } else { -2 }
            }
        },
    };
    rettv.value = TypvalValue::Number(n);
}

/// `getftime({fname})` - the last modification time of `{fname}`, in
/// seconds since the Unix epoch (`f_getftime`, `eval/fs.c`), via the
/// already-existing [`crate::os::fs::os_fileinfo`]/
/// [`crate::os::fs::os_fileinfo_mtime`]. `-1` if `{fname}` can't be
/// found.
pub(crate) fn f_getftime(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let name = crate::eval::typval::tv_get_string(&argvars[0]);
    let mtime = bytes_to_path(&name).and_then(crate::os::fs::os_fileinfo).map(|info| crate::os::fs::os_fileinfo_mtime(&info));
    rettv.value = TypvalValue::Number(mtime.unwrap_or(-1));
}

/// `getftype({fname})` - a description of `{fname}`'s file type
/// (`f_getftype`, `eval/fs.c`), via the already-existing
/// [`crate::os::fs::os_fileinfo_link`]/[`crate::os::fs::os_fileinfo_type_str`]
/// (`lstat`-like - does NOT follow a trailing symlink, matching the
/// original's own use of `os_fileinfo_link` here specifically). A
/// NULL `String` (`v:null`-adjacent, matching the original's own
/// `rettv->vval.v_string = NULL`, which stringifies to `""`) if
/// `{fname}` doesn't exist.
pub(crate) fn f_getftype(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let name = crate::eval::typval::tv_get_string(&argvars[0]);
    let type_str = bytes_to_path(&name)
        .and_then(crate::os::fs::os_fileinfo_link)
        .map(|info| crate::os::fs::os_fileinfo_type_str(&info));
    rettv.value = TypvalValue::String(type_str.map(|s| s.as_bytes().to_vec()));
}

/// `delete({name} [, {flags}])` - delete a file (`{flags}` omitted or
/// empty), an empty directory (`{flags} == "d"`), or a directory tree
/// (`{flags} == "rf"`, not yet translated - needs `delete_recursive`)
/// (`f_delete`, `eval/fs.c`). Returns `0` on success, `-1` on failure
/// (a missing/non-UTF8 `{name}`, or an unrecognized `{flags}` value -
/// the original's own `check_secure()`/invalid-argument `emsg` display
/// omitted, matching this module's established "skip the message,
/// keep the state" policy).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` (via
/// [`crate::ex_cmds::check_secure`]).
pub(crate) unsafe fn f_delete(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(-1);
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::ex_cmds::check_secure() } {
        return;
    }

    let name = crate::eval::typval::tv_get_string(&argvars[0]);
    if name.is_empty() {
        return;
    }
    let Some(path) = bytes_to_path(&name) else { return };

    let flags = if argvars.len() > 1 { crate::eval::typval::tv_get_string(&argvars[1]) } else { Vec::new() };

    if flags.is_empty() {
        rettv.value = TypvalValue::Number(if crate::os::fs::os_remove(path) == 0 { 0 } else { -1 });
    } else if flags == b"d" {
        rettv.value = TypvalValue::Number(if crate::os::fs::os_rmdir(path) == 0 { 0 } else { -1 });
    } else if flags == b"rf" {
        unimplemented!("delete(): recursive directory delete needs delete_recursive, not yet translated");
    }
    // Any other flags value: rettv stays -1 (matching the original's
    // own semsg-then-fallthrough - v_number was already set to -1
    // at the very start and no branch above re-touches it).
}

/// `mkdir({name} [, {flags} [, {prot}]])` - create directory `{name}`
/// (`f_mkdir`, `eval/fs.c`), via the already-existing
/// [`crate::os::fs::os_mkdir`]. Returns `1` on success, `0` on
/// failure (matching the original's own `OK`/`FAIL`, which are `1`/`0`,
/// its own `semsg` display on failure omitted, matching this module's
/// established "skip the message, keep the state" policy).
///
/// Both the plain, single-directory case and `"p"` (create
/// intermediate directories, via
/// [`crate::os::fs::os_mkdir_recurse`]) are modeled. `"D"`/`"R"`
/// (schedule deferred deletion) need the `:defer` subsystem
/// (`can_add_defer`/`defer_add`, not yet translated) and panic via
/// `unimplemented!()` if actually reached.
///
/// # Safety
/// Touches `crate::globals::GLOBALS` (via
/// [`crate::ex_cmds::check_secure`]).
pub(crate) unsafe fn f_mkdir(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(i64::from(crate::vim_defs::FAIL));
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::ex_cmds::check_secure() } {
        return;
    }

    let mut dir = crate::eval::typval::tv_get_string(&argvars[0]);
    if dir.is_empty() {
        return;
    }

    // Remove trailing slashes (`*path_tail(dir) == NUL` in the
    // original - i.e. the whole string is consumed by trailing path
    // separators).
    if crate::path::path_tail(&dir) == dir.len() {
        dir.truncate(crate::path::path_tail_with_sep(&dir));
    }

    let mut prot = 0o755;
    if argvars.len() > 1 {
        if argvars.len() > 2 {
            let p = crate::eval::typval::tv_get_number(&argvars[2]);
            if p == -1 {
                return;
            }
            prot = p as i32;
        }
        let flags = crate::eval::typval::tv_get_string(&argvars[1]);
        if flags.contains(&b'D') || flags.contains(&b'R') {
            unimplemented!("mkdir(): \"D\"/\"R\" flags need the :defer subsystem, not yet translated");
        }
        if flags.contains(&b'p') {
            // The original's own semsg(e_mkdir, failed_dir, ...) on
            // failure is skipped, matching this crate's established
            // "omit the message display, keep the state and return
            // value exact" policy - the FAIL already set above stands.
            if crate::os::fs::os_mkdir_recurse(&dir, prot).is_err() {
                return;
            }
            rettv.value = TypvalValue::Number(i64::from(crate::vim_defs::OK));
            return;
        }
    }

    let Some(path) = bytes_to_path(&dir) else { return };
    let ok = crate::os::fs::os_mkdir(path, prot) == 0;
    rettv.value = TypvalValue::Number(i64::from(if ok { crate::vim_defs::OK } else { crate::vim_defs::FAIL }));
}

/// `pathshorten({path} [, {len}])` - shorten each non-tail path
/// component of `{path}` to at most `{len}` (default `1`) characters
/// (`f_pathshorten`, `eval/fs.c`), via the already-translated
/// [`crate::path::shorten_dir_len`].
///
/// # Safety
/// Forwarded from [`crate::path::shorten_dir_len`]'s own safety doc.
pub(crate) unsafe fn f_pathshorten(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let mut trim_len = 1;
    if argvars.len() > 1 {
        trim_len = crate::eval::typval::tv_get_number(&argvars[1]) as i32;
        if trim_len < 1 {
            trim_len = 1;
        }
    }

    match crate::eval::typval::tv_get_string_chk(&argvars[0]) {
        None => rettv.value = TypvalValue::String(None),
        // SAFETY: forwarded from this function's own safety doc.
        Some(p) => rettv.value = TypvalValue::String(Some(unsafe { crate::path::shorten_dir_len(&p, trim_len) })),
    }
}

/// `rename({from}, {to})` - rename (or move) a file (`f_rename`,
/// `eval/fs.c`, via [`vim_rename`]). Returns `0` on success, `-1` on
/// failure (the original's own `check_secure()` guard is kept
/// faithfully; a missing/non-UTF8 `{from}`/`{to}` is treated the same
/// as a failed rename, matching this module's established
/// `bytes_to_path` convention).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` (via
/// [`crate::ex_cmds::check_secure`]).
pub(crate) unsafe fn f_rename(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(-1);
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::ex_cmds::check_secure() } {
        return;
    }

    let from = crate::eval::typval::tv_get_string(&argvars[0]);
    let to = crate::eval::typval::tv_get_string(&argvars[1]);
    let (Some(from_path), Some(to_path)) = (bytes_to_path(&from), bytes_to_path(&to)) else {
        return;
    };

    rettv.value = TypvalValue::Number(i64::from(vim_rename(from_path, to_path)));
}

/// Rename (or move) a file, matching the original's own `vim_rename`
/// (`fileio.c`) common-case behavior: renaming within the same
/// filesystem.
///
/// Deliberately narrower than the original in two documented ways:
/// - The "same name" fast path (`path_fnamecmp(from, to) == 0`) is
///   approximated via plain byte equality rather than
///   `path_fnamecmp`'s own case/separator-style-normalizing
///   comparison (not yet translated) - identical on any case-sensitive
///   filesystem, the common case on this crate's supported platforms.
///   The original's own further `p_fic`-gated "different case, same
///   name, needs a temp-file round-trip" sub-case
///   (`rename_with_tmp`) is not modeled at all.
/// - When `os_rename` itself fails (typically a cross-filesystem
///   move), the original falls back to `vim_copyfile` (copy, then
///   delete the source) - not yet translated (needs symlink-aware
///   copying) - `unimplemented!()`s if actually reached, rather than
///   silently reporting failure for what could be a legitimate move.
///
/// @return `0` on success, `-1` on failure (`from` doesn't exist).
fn vim_rename(from: &std::path::Path, to: &std::path::Path) -> i32 {
    if from.as_os_str() == to.as_os_str() {
        return 0;
    }

    if crate::os::fs::os_fileinfo(from).is_none() {
        return -1;
    }

    crate::os::fs::os_remove(to);

    if crate::os::fs::os_rename(from, to) == crate::vim_defs::OK {
        return 0;
    }

    unimplemented!("vim_rename(): os_rename failed, vim_copyfile fallback not yet translated");
}

/// `glob2regpat({string})` - convert a file pattern into a search
/// pattern (`f_glob2regpat`, `eval/fs.c`, via [`file_pat_to_reg_pat`]).
pub(crate) unsafe fn f_glob2regpat(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let pat = crate::eval::typval::tv_get_string_chk(&argvars[0]);
    rettv.value = TypvalValue::String(pat.and_then(|p| file_pat_to_reg_pat(&p, None, false)));
}

/// Convert a file pattern (e.g. `"*.c"`, `"file{one,two}.txt"`) into an
/// equivalent Vim regex search pattern (`file_pat_to_reg_pat`,
/// `fileio.c`, hosted here alongside its only currently-translated
/// caller, `f_glob2regpat`).
///
/// `allow_dirs`, if `Some`, is set to `true` when the pattern includes
/// a path separator (meaning matching should be allowed to span
/// directories) - `false` otherwise, matching the original's own
/// unconditional `*allow_dirs = false;` up front. `no_bslash` disables
/// the Windows-only `\`-escaping special cases (matches the original's
/// own `no_bslash` parameter exactly).
///
/// Returns `None` on an unbalanced `{`/`}` (the original's own
/// `E219`/`E220` error paths - message display is omitted, matching
/// this module's established policy, but the `None`/null-return
/// failure signal itself is kept faithfully).
///
/// Collapses the original's own separate size-computation-then-fill
/// two-pass structure into a single pass building a growing `Vec<u8>`,
/// since Rust's own `Vec` needs no pre-sizing dance, matching this
/// crate's established simplification for this exact C idiom (e.g.
/// `winrestcmd`/`vim_strsave_shellescape`).
#[must_use]
pub fn file_pat_to_reg_pat(pat: &[u8], mut allow_dirs: Option<&mut bool>, no_bslash: bool) -> Option<Vec<u8>> {
    if let Some(ad) = allow_dirs.as_deref_mut() {
        *ad = false;
    }

    if pat.is_empty() {
        return Some(b"^$".to_vec());
    }

    let mut reg_pat = Vec::with_capacity(pat.len() + 2);

    // Skip leading '*'s (keeping at least one byte if the ENTIRE
    // pattern is nothing but stars) - a leading '*' needs no explicit
    // ".*"/anchor: an un-anchored regex (no leading '^') already
    // matches anywhere in the string, achieving the same effect.
    let mut start = 0;
    if pat[0] == b'*' {
        while start < pat.len() - 1 && pat[start] == b'*' {
            start += 1;
        }
    } else {
        reg_pat.push(b'^');
    }

    // Trim trailing '*'s the same way, from the other end, and skip
    // the closing "$" anchor when the pattern ends with one - a
    // trailing '*' already means "match anything after", achieved by
    // simply not anchoring the end either.
    let mut end = pat.len() - 1; // inclusive index of the last byte considered
    let mut add_dollar = true;
    if pat[end] == b'*' {
        while end > start && pat[end] == b'*' {
            end -= 1;
        }
        add_dollar = false;
    }

    let mut nested: i32 = 0;
    let mut p = start;
    while p < pat.len() && nested >= 0 && p <= end {
        match pat[p] {
            b'*' => {
                reg_pat.push(b'.');
                reg_pat.push(b'*');
                // "**" matches like "*".
                while p + 1 < pat.len() && pat[p + 1] == b'*' {
                    p += 1;
                }
            }
            b @ (b'.' | b'~') => {
                reg_pat.push(b'\\');
                reg_pat.push(b);
            }
            b'?' => {
                reg_pat.push(b'.');
            }
            b'\\' => match pat.get(p + 1).copied() {
                None => {
                    // Trailing lone backslash: contributes nothing,
                    // matching the original's own early, empty `break`.
                }
                Some(next) => {
                    if cfg!(windows)
                        && !no_bslash
                        && (crate::charset::vim_isfilec(i32::from(next)) || next == b'*' || next == b'?')
                        && next != b'+'
                    {
                        // "\x" -> "[\/]" e.g. "dir\file"; "\*"/"\?"
                        // likewise. No extra `p` advancement here - the
                        // character right after the backslash (`next`)
                        // is reprocessed as its own, separate character
                        // on the NEXT loop iteration, matching the
                        // original's own single `p++` (from the
                        // backslash's own position) for this branch.
                        reg_pat.extend_from_slice(b"[\\/]");
                        if let Some(ad) = allow_dirs.as_deref_mut() {
                            *ad = true;
                        }
                    } else {
                        // `*++p`: advance past the backslash to examine
                        // the character immediately following it -
                        // undoing escaping from `ExpandEscape()`.
                        p += 1;
                        let c = pat[p];
                        if c == b'?' && (!cfg!(windows) || no_bslash) {
                            reg_pat.push(b'?');
                        } else if matches!(c, b',' | b'%' | b'#') || crate::ascii_defs::ascii_isspace(i32::from(c)) || matches!(c, b'{' | b'}') {
                            reg_pat.push(c);
                        } else if c == b'\\' && pat.get(p + 1) == Some(&b'\\') && pat.get(p + 2) == Some(&b'{') {
                            // "\\\{n,m\}" -> "\{n,m}".
                            reg_pat.push(b'\\');
                            reg_pat.push(b'{');
                            p += 2;
                        } else {
                            if let Some(ad) = allow_dirs.as_deref_mut()
                                && crate::path::vim_ispathsep(i32::from(c))
                                && (!cfg!(windows) || !no_bslash || c != b'\\')
                            {
                                *ad = true;
                            }
                            reg_pat.push(b'\\');
                            reg_pat.push(c);
                        }
                    }
                }
            },
            b'/' if cfg!(windows) => {
                reg_pat.extend_from_slice(b"[\\/]");
                if let Some(ad) = allow_dirs.as_deref_mut() {
                    *ad = true;
                }
            }
            b'{' => {
                reg_pat.push(b'\\');
                reg_pat.push(b'(');
                nested += 1;
            }
            b'}' => {
                reg_pat.push(b'\\');
                reg_pat.push(b')');
                nested -= 1;
            }
            b',' => {
                if nested != 0 {
                    reg_pat.push(b'\\');
                    reg_pat.push(b'|');
                } else {
                    reg_pat.push(b',');
                }
            }
            c => {
                if let Some(ad) = allow_dirs.as_deref_mut()
                    && crate::path::vim_ispathsep(i32::from(c))
                {
                    *ad = true;
                }
                reg_pat.push(c);
            }
        }
        p += 1;
    }

    if add_dollar {
        reg_pat.push(b'$');
    }

    if nested != 0 {
        return None;
    }
    Some(reg_pat)
}

/// Shared "resolve `{winnr}[, {tabnr}]` into `(scope, tabpage,
/// window)`" logic used by both `getcwd()`/`haslocaldir()` (their own
/// duplicated "Pre-conditions and scope extraction together" loop,
/// plus the tabpage/window lookup that follows it, `eval/fs.c`).
/// `Err(())` on any of the original's own genuine argument-error paths
/// (an invalid or out-of-range scope number, or an unresolvable
/// `{tabnr}`/`{winnr}`) - message display (`emsg`) is omitted, matching
/// this module's established "skip the message, keep the state/
/// control-flow" policy, but the early-return/failure itself IS kept
/// faithfully.
///
/// `scope_number[0]`/`[1]` mirror the original's own `int
/// scope_number[]` array, indexed by `CdScope::Window` (0)/
/// `CdScope::Tabpage` (1) - `{winnr}` is `argvars[0]`, `{tabnr}` is
/// `argvars[1]`.
///
/// # Safety
/// Touches `crate::globals::GLOBALS`; forwards
/// [`crate::window::find_tabpage`]/[`crate::window::find_win_by_nr`]'s
/// own safety docs.
unsafe fn resolve_cd_scope(
    argvars: &[TypvalT],
) -> Result<(crate::vim_defs::CdScope, *mut crate::buffer_defs::TabpageT, *mut crate::buffer_defs::WinT), ()> {
    use crate::vim_defs::CdScope;

    let mut scope = CdScope::Invalid;
    let mut scope_number: [i32; 2] = [0, 0];

    for i in 0..2usize {
        if i >= argvars.len() || matches!(argvars[i].value, TypvalValue::Unknown) {
            break;
        }
        if !matches!(argvars[i].value, TypvalValue::Number(_)) {
            return Err(()); // E475: Invalid argument.
        }
        let n = crate::eval::typval::tv_get_number(&argvars[i]) as i32;
        scope_number[i] = n;
        if n < -1 {
            return Err(()); // E475: Invalid argument.
        }
        // Use the narrowest scope the user requested.
        if n >= 0 && scope == CdScope::Invalid {
            scope = if i == 0 { CdScope::Window } else { CdScope::Tabpage };
        } else if n < 0 {
            scope = if i == 0 { CdScope::Tabpage } else { CdScope::Global };
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    // SAFETY: forwarded from this function's own safety doc.
    let mut win = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;

    // Find the tabpage by number.
    if scope_number[1] > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { crate::window::find_tabpage(scope_number[1]) };
        if tp.is_null() {
            return Err(()); // E5000: Cannot find tab number.
        }
    }

    // Find the window in `tp` by number, null if none.
    if scope_number[0] >= 0 {
        if scope_number[1] < 0 {
            return Err(()); // E5001: Higher scope cannot be -1 if lower scope is >= 0.
        }
        if scope_number[0] > 0 {
            // SAFETY: forwarded from this function's own safety doc.
            win = unsafe { crate::window::find_win_by_nr(&argvars[0], tp) };
            if win.is_null() {
                return Err(()); // E5002: Cannot find window number.
            }
        }
    }

    Ok((scope, tp, win))
}

/// `getcwd([{winnr} [, {tabnr}]])` - the effective |current-directory|
/// for the given scope, falling back progressively to the broader
/// enclosing scope (window -> tabpage -> global -> the real OS
/// current directory) whenever a narrower scope has no local directory
/// set of its own (`f_getcwd`, `eval/fs.c`), via [`resolve_cd_scope`].
/// An empty string on any argument error, or if even
/// [`crate::os::fs::os_dirname`] itself fails.
///
/// # Safety
/// Forwarded from [`resolve_cd_scope`]'s own safety doc.
pub(crate) unsafe fn f_getcwd(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::String(None);

    // SAFETY: forwarded from this function's own safety doc.
    let Ok((scope, tp, win)) = (unsafe { resolve_cd_scope(argvars) }) else {
        return;
    };

    // Mirrors the original's own switch/fallthrough exactly: each
    // narrower scope tries its own local directory first, falling
    // through to the next broader scope whenever unset, all the way
    // down to the real OS current directory.
    use crate::vim_defs::CdScope;
    let from: Option<Vec<u8>> = 'scope_fallthrough: {
        if scope == CdScope::Window {
            // SAFETY: forwarded from this function's own safety doc.
            let w_localdir = unsafe { &*win }.w_localdir.clone();
            if w_localdir.is_some() {
                break 'scope_fallthrough w_localdir;
            }
        }
        if matches!(scope, CdScope::Window | CdScope::Tabpage) {
            // SAFETY: forwarded from this function's own safety doc.
            let tp_localdir = unsafe { &*tp }.tp_localdir.clone();
            if tp_localdir.is_some() {
                break 'scope_fallthrough tp_localdir;
            }
        }
        if matches!(scope, CdScope::Window | CdScope::Tabpage | CdScope::Global) {
            // SAFETY: forwarded from this function's own safety doc.
            let globaldir = unsafe { crate::globals::GLOBALS.get_mut() }.globaldir.clone();
            if globaldir.is_some() {
                break 'scope_fallthrough globaldir;
            }
        }
        crate::os::fs::os_dirname()
    };
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut cwd = from.unwrap_or_default();

    // SAFETY: forwarded from this function's own safety doc.
    #[cfg(windows)]
    unsafe {
        crate::path::slash_adjust(&mut cwd);
    }

    rettv.value = TypvalValue::String(Some(cwd));
}

/// `haslocaldir([{winnr} [, {tabnr}]])` - whether the given scope
/// (defaults to window scope, `MIN_CD_SCOPE`, when no scope was
/// requested at all) has its own local (`:lcd`/`:tcd`) working
/// directory set (`f_haslocaldir`, `eval/fs.c`), via
/// [`resolve_cd_scope`]. The global scope never has a local directory
/// of its own, so it's always `0` there.
///
/// # Safety
/// Forwarded from [`resolve_cd_scope`]'s own safety doc.
pub(crate) unsafe fn f_haslocaldir(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(0);

    // SAFETY: forwarded from this function's own safety doc.
    let Ok((mut scope, tp, win)) = (unsafe { resolve_cd_scope(argvars) }) else {
        return;
    };
    if scope == crate::vim_defs::CdScope::Invalid {
        scope = crate::vim_defs::CdScope::MIN; // == Window
    }

    let has = match scope {
        crate::vim_defs::CdScope::Window => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &*win }.w_localdir.is_some()
        }
        crate::vim_defs::CdScope::Tabpage => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &*tp }.tp_localdir.is_some()
        }
        crate::vim_defs::CdScope::Global | crate::vim_defs::CdScope::Invalid => false,
    };
    rettv.value = TypvalValue::Number(i64::from(has));
}

/// `browse({save}, {title}, {initdir}, {default})` - put up a file
/// requester (`f_browse`, `eval/fs.c`). A GUI-only feature - this
/// crate never runs a GUI, so `has('browse')` is always false and
/// this always returns an empty string, matching the original's own
/// real, unconditional body exactly (it never even inspects its own
/// arguments).
pub(crate) fn f_browse(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::String(None);
}

/// `browsedir({title}, {initdir})` - put up a directory requester
/// (`f_browsedir`, `eval/fs.c`) - a thin, real delegate to
/// [`f_browse`] in the original itself (not just "the same
/// behavior") - GUI-only, always an empty string here.
pub(crate) fn f_browsedir(argvars: &[TypvalT], rettv: &mut TypvalT) {
    f_browse(argvars, rettv);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(s: &[u8]) -> TypvalT {
        TypvalT { value: TypvalValue::String(Some(s.to_vec())), ..Default::default() }
    }

    fn globals_test_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::globals::global_state_test_lock()
    }

    // --- f_isdirectory ---

    #[test]
    fn isdirectory_true_for_a_real_directory() {
        let tmp = std::env::temp_dir();
        let mut rettv = TypvalT::default();
        f_isdirectory(&[string(tmp.to_str().unwrap().as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn isdirectory_false_for_a_nonexistent_path() {
        let mut rettv = TypvalT::default();
        f_isdirectory(&[string(b"/definitely/does/not/exist/nero-test")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_isabsolutepath ---

    #[test]
    fn isabsolutepath_true_for_an_absolute_path() {
        let mut rettv = TypvalT::default();
        let name: &[u8] = if cfg!(windows) { b"C:\\foo" } else { b"/foo" };
        f_isabsolutepath(&[string(name)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn isabsolutepath_false_for_a_relative_path() {
        let mut rettv = TypvalT::default();
        f_isabsolutepath(&[string(b"relative/path")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_delete ---

    #[test]
    fn delete_removes_a_real_file() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let path = std::env::temp_dir().join("nero_test_delete_file.txt");
        std::fs::write(&path, b"x").unwrap();
        assert!(path.exists());

        let mut rettv = TypvalT::default();
        unsafe { f_delete(&[string(path.to_str().unwrap().as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        assert!(!path.exists());
    }

    #[test]
    fn delete_empty_directory_with_d_flag() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let path = std::env::temp_dir().join("nero_test_delete_empty_dir");
        let _ = std::fs::remove_dir(&path);
        std::fs::create_dir(&path).unwrap();

        let mut rettv = TypvalT::default();
        unsafe { f_delete(&[string(path.to_str().unwrap().as_bytes()), string(b"d")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        assert!(!path.exists());
    }

    #[test]
    fn delete_nonexistent_file_fails() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let mut rettv = TypvalT::default();
        unsafe {
            f_delete(&[string(b"/definitely/does/not/exist/nero-test-delete")], &mut rettv);
        }
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn delete_empty_name_fails() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let mut rettv = TypvalT::default();
        unsafe { f_delete(&[string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn delete_unrecognized_flags_fails() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let path = std::env::temp_dir().join("nero_test_delete_bad_flags.txt");
        std::fs::write(&path, b"x").unwrap();

        let mut rettv = TypvalT::default();
        unsafe {
            f_delete(&[string(path.to_str().unwrap().as_bytes()), string(b"zz")], &mut rettv);
        }
        assert_eq!(rettv.value, TypvalValue::Number(-1));
        // Cleanup: the file is untouched by an unrecognized flag.
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn delete_fails_and_secure_is_bumped_when_secure_is_set() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 1;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let mut rettv = TypvalT::default();
        unsafe { f_delete(&[string(b"irrelevant")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.secure, 2);
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
    }

    // --- f_mkdir ---

    #[test]
    fn mkdir_creates_a_new_directory() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let path = std::env::temp_dir().join("nero_test_mkdir_new_dir");
        let _ = std::fs::remove_dir(&path);

        let mut rettv = TypvalT::default();
        unsafe { f_mkdir(&[string(path.to_str().unwrap().as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        assert!(path.is_dir());

        std::fs::remove_dir(&path).unwrap();
    }

    #[test]
    fn mkdir_strips_trailing_slashes() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let path = std::env::temp_dir().join("nero_test_mkdir_trailing_slash");
        let _ = std::fs::remove_dir(&path);
        let mut with_slash = path.to_str().unwrap().as_bytes().to_vec();
        with_slash.push(b'/');

        let mut rettv = TypvalT::default();
        unsafe { f_mkdir(&[string(&with_slash)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        assert!(path.is_dir());

        std::fs::remove_dir(&path).unwrap();
    }

    #[test]
    fn mkdir_p_creates_missing_intermediate_directories() {
        // Cross-verified against real nvim: mkdir(deep, 'p') returns 1
        // and the whole chain exists afterwards, whereas the same call
        // without 'p' fails with E739.
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let root = std::env::temp_dir().join("nero_test_mkdir_p_root");
        let _ = std::fs::remove_dir_all(&root);
        let deep = root.join("a").join("b").join("c");

        let mut rettv = TypvalT::default();
        unsafe {
            f_mkdir(
                &[string(deep.to_str().unwrap().as_bytes()), string(b"p")],
                &mut rettv,
            );
        }
        assert_eq!(rettv.value, TypvalValue::Number(1));
        assert!(deep.is_dir());
        assert!(root.join("a").is_dir(), "intermediate levels too");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn mkdir_p_on_an_existing_directory_still_succeeds() {
        // Cross-verified against real nvim: a second mkdir(dir, 'p')
        // returns 1 rather than failing.
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let root = std::env::temp_dir().join("nero_test_mkdir_p_twice");
        let _ = std::fs::remove_dir_all(&root);
        let deep = root.join("a");
        std::fs::create_dir_all(&deep).unwrap();

        let mut rettv = TypvalT::default();
        unsafe {
            f_mkdir(
                &[string(deep.to_str().unwrap().as_bytes()), string(b"p")],
                &mut rettv,
            );
        }
        assert_eq!(rettv.value, TypvalValue::Number(1));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn mkdir_p_honours_an_explicit_prot_argument() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let root = std::env::temp_dir().join("nero_test_mkdir_p_prot");
        let _ = std::fs::remove_dir_all(&root);
        let deep = root.join("a").join("b");

        let mut rettv = TypvalT::default();
        unsafe {
            f_mkdir(
                &[
                    string(deep.to_str().unwrap().as_bytes()),
                    string(b"p"),
                    TypvalT { value: TypvalValue::Number(0o700), ..Default::default() },
                ],
                &mut rettv,
            );
        }
        assert_eq!(rettv.value, TypvalValue::Number(1));
        assert!(deep.is_dir());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn mkdir_fails_when_parent_is_missing() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let path = std::env::temp_dir().join("nero_test_mkdir_missing_parent").join("child");

        let mut rettv = TypvalT::default();
        unsafe { f_mkdir(&[string(path.to_str().unwrap().as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        assert!(!path.exists());
    }

    #[test]
    fn mkdir_fails_for_an_empty_name() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let mut rettv = TypvalT::default();
        unsafe { f_mkdir(&[string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn mkdir_fails_and_secure_is_bumped_when_secure_is_set() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 1;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let mut rettv = TypvalT::default();
        unsafe { f_mkdir(&[string(b"irrelevant")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.secure, 2);
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
    }

    #[test]
    fn mkdir_p_relative_to_an_existing_root_creates_only_the_missing_part() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        // The temp dir already exists, so only the two new levels are
        // created - exercising the shrink-then-create walk's stop
        // condition rather than running all the way to the root.
        let root = std::env::temp_dir().join("nero_test_mkdir_p_partial");
        let _ = std::fs::remove_dir_all(&root);
        let deep = root.join("only").join("these");

        let mut rettv = TypvalT::default();
        unsafe {
            f_mkdir(
                &[string(deep.to_str().unwrap().as_bytes()), string(b"p")],
                &mut rettv,
            );
        }
        assert_eq!(rettv.value, TypvalValue::Number(1));
        assert!(deep.is_dir());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn mkdir_d_flag_is_unimplemented() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let mut rettv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            f_mkdir(&[string(b"irrelevant"), string(b"D")], &mut rettv);
        }));
        assert!(result.is_err(), "expected a panic (:defer subsystem not yet translated)");
    }

    // --- f_pathshorten ---

    fn num(n: crate::eval::typval_defs::VarnumberT) -> TypvalT {
        TypvalT { value: TypvalValue::Number(n), ..Default::default() }
    }

    #[test]
    fn pathshorten_default_trim_len_is_one() {
        let _guard = globals_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_pathshorten(&[string(b"foo/bar/baz.txt")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"f/b/baz.txt".to_vec())));
    }

    #[test]
    fn pathshorten_explicit_trim_len() {
        let _guard = globals_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_pathshorten(&[string(b"foo/bar/baz.txt"), num(2)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"fo/ba/baz.txt".to_vec())));
    }

    #[test]
    fn pathshorten_clamps_a_trim_len_below_one() {
        let _guard = globals_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_pathshorten(&[string(b"foo/bar/baz.txt"), num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"f/b/baz.txt".to_vec())));
    }

    // --- f_browse / f_browsedir ---

    #[test]
    fn browse_always_returns_an_empty_string() {
        let mut rettv = TypvalT::default();
        f_browse(
            &[num(0), string(b"title"), string(b"/tmp"), string(b"default.txt")],
            &mut rettv,
        );
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn browsedir_always_returns_an_empty_string() {
        let mut rettv = TypvalT::default();
        f_browsedir(&[string(b"title"), string(b"/tmp")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    // --- f_filereadable ---

    #[test]
    fn filereadable_true_for_an_existing_file() {
        let path = std::env::temp_dir().join("nero_test_filereadable.txt");
        std::fs::write(&path, b"x").unwrap();
        let mut rettv = TypvalT::default();
        f_filereadable(&[string(path.to_str().unwrap().as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn filereadable_false_for_a_nonexistent_path() {
        let mut rettv = TypvalT::default();
        f_filereadable(&[string(b"/definitely/does/not/exist/nero-test")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn filereadable_false_for_a_directory() {
        let tmp = std::env::temp_dir();
        let mut rettv = TypvalT::default();
        f_filereadable(&[string(tmp.to_str().unwrap().as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn filereadable_false_for_an_empty_name() {
        let mut rettv = TypvalT::default();
        f_filereadable(&[string(b"")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_filewritable ---

    #[test]
    fn filewritable_returns_1_for_a_writable_file() {
        let path = std::env::temp_dir().join("nero_test_filewritable.txt");
        std::fs::write(&path, b"x").unwrap();
        let mut rettv = TypvalT::default();
        f_filewritable(&[string(path.to_str().unwrap().as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn filewritable_returns_2_for_a_writable_directory() {
        let tmp = std::env::temp_dir();
        let mut rettv = TypvalT::default();
        f_filewritable(&[string(tmp.to_str().unwrap().as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(2));
    }

    #[test]
    fn filewritable_returns_0_for_a_nonexistent_path() {
        let mut rettv = TypvalT::default();
        f_filewritable(&[string(b"/definitely/does/not/exist/nero-test")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_getfsize ---

    #[test]
    fn getfsize_returns_the_byte_length_of_a_file() {
        let path = std::env::temp_dir().join("nero_test_getfsize.txt");
        std::fs::write(&path, b"hello world").unwrap();
        let mut rettv = TypvalT::default();
        f_getfsize(&[string(path.to_str().unwrap().as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(11));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn getfsize_returns_0_for_a_directory() {
        let tmp = std::env::temp_dir();
        let mut rettv = TypvalT::default();
        f_getfsize(&[string(tmp.to_str().unwrap().as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn getfsize_returns_minus_1_for_a_nonexistent_path() {
        let mut rettv = TypvalT::default();
        f_getfsize(&[string(b"/definitely/does/not/exist/nero-test")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    // --- f_getftime ---

    #[test]
    fn getftime_returns_a_recent_real_timestamp() {
        let path = std::env::temp_dir().join("nero_test_getftime.txt");
        std::fs::write(&path, b"hello").unwrap();
        let mut rettv = TypvalT::default();
        f_getftime(&[string(path.to_str().unwrap().as_bytes())], &mut rettv);
        let TypvalValue::Number(mtime) = rettv.value else { panic!("expected a Number") };
        assert!(mtime > 1_577_836_800);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn getftime_returns_minus_1_for_a_nonexistent_path() {
        let mut rettv = TypvalT::default();
        f_getftime(&[string(b"/definitely/does/not/exist/nero-test")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    // --- f_getftype ---

    #[test]
    fn getftype_identifies_a_regular_file() {
        let path = std::env::temp_dir().join("nero_test_getftype.txt");
        std::fs::write(&path, b"hello").unwrap();
        let mut rettv = TypvalT::default();
        f_getftype(&[string(path.to_str().unwrap().as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(Some(b"file".to_vec())));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn getftype_identifies_a_directory() {
        let tmp = std::env::temp_dir();
        let mut rettv = TypvalT::default();
        f_getftype(&[string(tmp.to_str().unwrap().as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(Some(b"dir".to_vec())));
    }

    #[test]
    fn getftype_returns_null_string_for_a_nonexistent_path() {
        let mut rettv = TypvalT::default();
        f_getftype(&[string(b"/definitely/does/not/exist/nero-test")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    // --- resolve_cd_scope / f_getcwd / f_haslocaldir ---

    /// RAII guard restoring `GLOBALS.curtab`/`curwin`/`first_tabpage`/
    /// `firstwin`/`globaldir` on drop - callers must hold
    /// `globals_test_lock()` for the guard's whole lifetime.
    struct CdGlobalsGuard {
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_curwin: *mut crate::buffer_defs::WinT,
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
        prev_firstwin: *mut crate::buffer_defs::WinT,
        prev_globaldir: Option<Vec<u8>>,
    }

    impl CdGlobalsGuard {
        fn set(tab: *mut crate::buffer_defs::TabpageT, win: *mut crate::buffer_defs::WinT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = CdGlobalsGuard {
                prev_curtab: globals.curtab,
                prev_curwin: globals.curwin,
                prev_first_tabpage: globals.first_tabpage,
                prev_firstwin: globals.firstwin,
                prev_globaldir: globals.globaldir.take(),
            };
            globals.curtab = tab;
            globals.curwin = win;
            globals.first_tabpage = tab;
            globals.firstwin = win;
            guard
        }
    }

    impl Drop for CdGlobalsGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.curtab = self.prev_curtab;
            globals.curwin = self.prev_curwin;
            globals.first_tabpage = self.prev_first_tabpage;
            globals.firstwin = self.prev_firstwin;
            globals.globaldir = self.prev_globaldir.take();
        }
    }

    /// Forces `'shellslash'` on for its whole lifetime, restoring the
    /// previous value on drop (even on panic). On Windows,
    /// `f_getcwd`'s own `slash_adjust()` post-processing step
    /// otherwise converts every `/` in these tests' own forward-
    /// slash-only expected paths into `\` (the real, correct default
    /// behavior without `'shellslash'` set) - forcing it on instead
    /// makes `slash_adjust` look for (nonexistent) backslashes in
    /// these specific test inputs, a platform-independent no-op. Inert
    /// on non-Windows (`slash_adjust` doesn't exist/isn't called
    /// there), but harmless to use unconditionally.
    struct ShellslashGuard(i32);

    impl ShellslashGuard {
        fn force_on() -> Self {
            let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let prev = ov.p_ssl;
            ov.p_ssl = 1;
            ShellslashGuard(prev)
        }
    }

    impl Drop for ShellslashGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ssl = self.0;
        }
    }

    #[test]
    fn resolve_cd_scope_no_args_is_invalid_scope_with_curtab_curwin() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        let tab_ptr = &mut tab as *mut crate::buffer_defs::TabpageT;
        let _guard = CdGlobalsGuard::set(tab_ptr, win_ptr);

        let (scope, tp, w) = unsafe { resolve_cd_scope(&[]) }.unwrap();
        assert_eq!(scope, crate::vim_defs::CdScope::Invalid);
        assert_eq!(tp, tab_ptr);
        assert_eq!(w, win_ptr);
    }

    #[test]
    fn resolve_cd_scope_window_number_zero_is_window_scope() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        let (scope, ..) = unsafe { resolve_cd_scope(&[num(0)]) }.unwrap();
        assert_eq!(scope, crate::vim_defs::CdScope::Window);
    }

    #[test]
    fn resolve_cd_scope_window_negative_one_widens_to_tabpage() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        let (scope, ..) = unsafe { resolve_cd_scope(&[num(-1)]) }.unwrap();
        assert_eq!(scope, crate::vim_defs::CdScope::Tabpage);
    }

    #[test]
    fn resolve_cd_scope_both_negative_one_is_global() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        let (scope, ..) = unsafe { resolve_cd_scope(&[num(-1), num(-1)]) }.unwrap();
        assert_eq!(scope, crate::vim_defs::CdScope::Global);
    }

    #[test]
    fn resolve_cd_scope_number_below_negative_one_errors() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        assert!(unsafe { resolve_cd_scope(&[num(-2)]) }.is_err());
    }

    #[test]
    fn resolve_cd_scope_non_number_arg_errors() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        assert!(unsafe { resolve_cd_scope(&[string(b"x")]) }.is_err());
    }

    #[test]
    fn resolve_cd_scope_unknown_tabnr_errors() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        // Only 1 real tabpage exists (the current one) - tabnr 99
        // cannot be found.
        assert!(unsafe { resolve_cd_scope(&[num(0), num(99)]) }.is_err());
    }

    #[test]
    fn resolve_cd_scope_tabnr_negative_with_winnr_nonnegative_errors() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        // E5001: a real winnr (0 or higher) cannot be combined with
        // tabnr == -1.
        assert!(unsafe { resolve_cd_scope(&[num(0), num(-1)]) }.is_err());
    }

    #[test]
    fn resolve_cd_scope_unknown_winnr_errors() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        // Only 1 real window exists (curwin) - winnr 99 cannot be
        // found.
        assert!(unsafe { resolve_cd_scope(&[num(99)]) }.is_err());
    }

    #[test]
    fn getcwd_explicit_window_scope_uses_localdir_first() {
        let _lock = globals_test_lock();
        let _ssl_guard = ShellslashGuard::force_on();
        let mut win = crate::buffer_defs::WinT { handle: 1, w_localdir: Some(b"/win/dir".to_vec()), ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT { tp_localdir: Some(b"/tab/dir".to_vec()), ..Default::default() };
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);
        unsafe { crate::globals::GLOBALS.get_mut() }.globaldir = Some(b"/global/dir".to_vec());

        // getcwd(0): an EXPLICIT window-scope request (0 = current
        // window) - unlike bare getcwd() (see
        // getcwd_bare_no_args_always_uses_the_real_os_current_directory
        // below), this genuinely resolves to CdScope::Window and
        // consults w_localdir.
        let mut rettv = TypvalT::default();
        unsafe { f_getcwd(&[num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"/win/dir".to_vec())));
    }

    #[test]
    fn getcwd_falls_back_to_tabpage_localdir() {
        let _lock = globals_test_lock();
        let _ssl_guard = ShellslashGuard::force_on();
        let mut win = crate::buffer_defs::WinT { handle: 1, w_localdir: None, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT { tp_localdir: Some(b"/tab/dir".to_vec()), ..Default::default() };
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);
        unsafe { crate::globals::GLOBALS.get_mut() }.globaldir = Some(b"/global/dir".to_vec());

        let mut rettv = TypvalT::default();
        unsafe { f_getcwd(&[num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"/tab/dir".to_vec())));
    }

    #[test]
    fn getcwd_falls_back_to_globaldir() {
        let _lock = globals_test_lock();
        let _ssl_guard = ShellslashGuard::force_on();
        let mut win = crate::buffer_defs::WinT { handle: 1, w_localdir: None, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT { tp_localdir: None, ..Default::default() };
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);
        unsafe { crate::globals::GLOBALS.get_mut() }.globaldir = Some(b"/global/dir".to_vec());

        let mut rettv = TypvalT::default();
        unsafe { f_getcwd(&[num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"/global/dir".to_vec())));
    }

    #[test]
    fn getcwd_falls_back_to_the_real_os_current_directory() {
        let _lock = globals_test_lock();
        let _ssl_guard = ShellslashGuard::force_on();
        let mut win = crate::buffer_defs::WinT { handle: 1, w_localdir: None, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT { tp_localdir: None, ..Default::default() };
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);
        unsafe { crate::globals::GLOBALS.get_mut() }.globaldir = None;

        let mut rettv = TypvalT::default();
        unsafe { f_getcwd(&[num(0)], &mut rettv) };
        let TypvalValue::String(Some(cwd)) = rettv.value else { panic!("expected a String") };
        assert_eq!(Some(cwd), crate::os::fs::os_dirname());
    }

    #[test]
    fn getcwd_bare_no_args_always_uses_the_real_os_current_directory() {
        let _lock = globals_test_lock();
        let _ssl_guard = ShellslashGuard::force_on();
        // A genuine, easy-to-miss original behavior, verified by
        // hand-tracing eval/fs.c's own f_getcwd: with ZERO arguments,
        // argvars[0].v_type == VAR_UNKNOWN makes the scope-extraction
        // loop `break` immediately, leaving `scope` at kCdScopeInvalid
        // - which jumps STRAIGHT to the switch's LAST case
        // (os_dirname()), WITHOUT ever consulting
        // w_localdir/tp_localdir/globaldir at all, even though all
        // three are set here. Only an EXPLICIT scope argument (e.g.
        // getcwd(0), see getcwd_explicit_window_scope_uses_localdir_first
        // above) exercises that fallback chain - matches real nvim's
        // own documented distinction (bare getcwd() reports the real,
        // already-synced OS current directory).
        let mut win = crate::buffer_defs::WinT { handle: 1, w_localdir: Some(b"/win/dir".to_vec()), ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT { tp_localdir: Some(b"/tab/dir".to_vec()), ..Default::default() };
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);
        unsafe { crate::globals::GLOBALS.get_mut() }.globaldir = Some(b"/global/dir".to_vec());

        let mut rettv = TypvalT::default();
        unsafe { f_getcwd(&[], &mut rettv) };
        let TypvalValue::String(Some(cwd)) = rettv.value else { panic!("expected a String") };
        assert_eq!(Some(cwd), crate::os::fs::os_dirname());
    }

    #[test]
    fn getcwd_argument_error_yields_a_null_string() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        let mut rettv = TypvalT::default();
        unsafe { f_getcwd(&[string(b"x")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn haslocaldir_no_args_defaults_to_window_scope() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, w_localdir: Some(b"/win/dir".to_vec()), ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        let _guard = CdGlobalsGuard::set(&mut tab, win_ptr);

        let mut rettv = TypvalT::default();
        unsafe { f_haslocaldir(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));

        // Reuse the SAME pointer already stored in GLOBALS.curwin
        // (not a freshly-derived one from `win` again) - deriving a
        // second independent pointer from the same local would
        // invalidate the one GLOBALS.curwin already holds under Tree
        // Borrows.
        unsafe { &mut *win_ptr }.w_localdir = None;
        let mut rettv = TypvalT::default();
        unsafe { f_haslocaldir(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn haslocaldir_tabpage_scope_via_negative_winnr() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT { tp_localdir: Some(b"/tab/dir".to_vec()), ..Default::default() };
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        let mut rettv = TypvalT::default();
        unsafe { f_haslocaldir(&[num(-1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn haslocaldir_global_scope_is_always_false() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, w_localdir: Some(b"/win/dir".to_vec()), ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT { tp_localdir: Some(b"/tab/dir".to_vec()), ..Default::default() };
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        let mut rettv = TypvalT::default();
        unsafe { f_haslocaldir(&[num(-1), num(-1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn haslocaldir_argument_error_yields_zero() {
        let _lock = globals_test_lock();
        let mut win = crate::buffer_defs::WinT { handle: 1, ..Default::default() };
        let mut tab = crate::buffer_defs::TabpageT::default();
        let _guard = CdGlobalsGuard::set(&mut tab, &mut win);

        let mut rettv = TypvalT::default();
        unsafe { f_haslocaldir(&[string(b"x")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_rename / vim_rename ---

    #[test]
    fn rename_moves_a_real_file() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let from = std::env::temp_dir().join("nero_test_rename_from.txt");
        let to = std::env::temp_dir().join("nero_test_rename_to.txt");
        let _ = std::fs::remove_file(&to);
        std::fs::write(&from, b"contents").unwrap();

        let mut rettv = TypvalT::default();
        unsafe {
            f_rename(&[string(from.to_str().unwrap().as_bytes()), string(to.to_str().unwrap().as_bytes())], &mut rettv);
        }
        assert_eq!(rettv.value, TypvalValue::Number(0));
        assert!(!from.exists());
        assert_eq!(std::fs::read(&to).unwrap(), b"contents");

        std::fs::remove_file(&to).unwrap();
    }

    #[test]
    fn rename_overwrites_an_existing_destination() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let from = std::env::temp_dir().join("nero_test_rename_overwrite_from.txt");
        let to = std::env::temp_dir().join("nero_test_rename_overwrite_to.txt");
        std::fs::write(&from, b"new").unwrap();
        std::fs::write(&to, b"old").unwrap();

        let mut rettv = TypvalT::default();
        unsafe {
            f_rename(&[string(from.to_str().unwrap().as_bytes()), string(to.to_str().unwrap().as_bytes())], &mut rettv);
        }
        assert_eq!(rettv.value, TypvalValue::Number(0));
        assert_eq!(std::fs::read(&to).unwrap(), b"new");

        std::fs::remove_file(&to).unwrap();
    }

    #[test]
    fn rename_fails_when_from_does_not_exist() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let mut rettv = TypvalT::default();
        unsafe {
            f_rename(
                &[string(b"/definitely/does/not/exist/nero-test-rename-from"), string(b"/tmp/nero-test-rename-to")],
                &mut rettv,
            );
        }
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn rename_same_path_is_a_no_op_success() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        // Deliberately does NOT exist - `vim_rename`'s "same name" fast
        // path returns success without ever checking `from`/`to` exist,
        // matching the original's own early `path_fnamecmp` check
        // (which runs before the "from exists" check).
        let name: &[u8] = b"/definitely/does/not/exist/nero-test-rename-same";
        let mut rettv = TypvalT::default();
        unsafe { f_rename(&[string(name), string(name)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn rename_fails_and_secure_is_bumped_when_secure_is_set() {
        let _guard = globals_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 1;
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;

        let mut rettv = TypvalT::default();
        unsafe { f_rename(&[string(b"irrelevant-from"), string(b"irrelevant-to")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.secure, 2);
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
    }

    // --- file_pat_to_reg_pat / f_glob2regpat ---

    #[test]
    fn file_pat_to_reg_pat_empty_pattern_is_anchor_only() {
        assert_eq!(file_pat_to_reg_pat(b"", None, false), Some(b"^$".to_vec()));
    }

    #[test]
    fn file_pat_to_reg_pat_plain_literal_escapes_dots() {
        assert_eq!(file_pat_to_reg_pat(b"file.txt", None, false), Some(b"^file\\.txt$".to_vec()));
    }

    #[test]
    fn file_pat_to_reg_pat_leading_star_needs_no_anchor() {
        // A leading '*' is stripped without emitting ".*"/"^" at all -
        // an un-anchored regex already matches anywhere in the string,
        // achieving the same "match anything before" effect.
        assert_eq!(file_pat_to_reg_pat(b"*.c", None, false), Some(b"\\.c$".to_vec()));
    }

    #[test]
    fn file_pat_to_reg_pat_trailing_star_needs_no_dollar() {
        // A trailing '*' is trimmed OUT of the scan range entirely (the
        // same way a leading '*' is skipped) - nothing at all is
        // emitted for it, not even ".*": an un-anchored regex (no
        // trailing '$') already matches any longer string starting
        // with "foo", achieving "match anything after" implicitly.
        assert_eq!(file_pat_to_reg_pat(b"foo*", None, false), Some(b"^foo".to_vec()));
    }

    #[test]
    fn file_pat_to_reg_pat_bare_star_has_no_anchors_at_all() {
        assert_eq!(file_pat_to_reg_pat(b"*", None, false), Some(b".*".to_vec()));
    }

    #[test]
    fn file_pat_to_reg_pat_double_star_collapses_like_a_single_star() {
        assert_eq!(file_pat_to_reg_pat(b"a**b", None, false), Some(b"^a.*b$".to_vec()));
    }

    #[test]
    fn file_pat_to_reg_pat_question_mark_becomes_any_char() {
        assert_eq!(file_pat_to_reg_pat(b"a?b", None, false), Some(b"^a.b$".to_vec()));
    }

    #[test]
    fn file_pat_to_reg_pat_tilde_is_escaped() {
        assert_eq!(file_pat_to_reg_pat(b"a~b", None, false), Some(b"^a\\~b$".to_vec()));
    }

    #[test]
    fn file_pat_to_reg_pat_braces_become_alternation() {
        assert_eq!(file_pat_to_reg_pat(b"{foo,bar}", None, false), Some(b"^\\(foo\\|bar\\)$".to_vec()));
    }

    #[test]
    fn file_pat_to_reg_pat_unbalanced_opening_brace_fails() {
        assert_eq!(file_pat_to_reg_pat(b"{foo", None, false), None);
    }

    #[test]
    fn file_pat_to_reg_pat_unbalanced_closing_brace_fails() {
        assert_eq!(file_pat_to_reg_pat(b"foo}", None, false), None);
    }

    #[test]
    fn file_pat_to_reg_pat_sets_allow_dirs_for_a_path_separator() {
        // `case '/':`'s own "[\/]" bracket-escape only exists inside
        // the original's `#ifdef BACKSLASH_IN_FILENAME` (Windows-only)
        // - on non-Windows, '/' falls through to the plain `default:`
        // case instead, emitted as a literal character (still setting
        // allow_dirs via vim_ispathsep either way). Verified directly
        // against a real nvim binary on Windows before trusting this.
        let mut allow_dirs = false;
        let result = file_pat_to_reg_pat(b"a/b", Some(&mut allow_dirs), false);
        assert!(allow_dirs);
        if cfg!(windows) {
            assert_eq!(result, Some(b"^a[\\/]b$".to_vec()));
        } else {
            assert_eq!(result, Some(b"^a/b$".to_vec()));
        }
    }

    #[test]
    fn file_pat_to_reg_pat_leaves_allow_dirs_false_without_a_separator() {
        let mut allow_dirs = true; // starts true - must be reset to false up front.
        let result = file_pat_to_reg_pat(b"abc", Some(&mut allow_dirs), false);
        assert!(!allow_dirs);
        assert_eq!(result, Some(b"^abc$".to_vec()));
    }

    #[test]
    fn file_pat_to_reg_pat_trailing_lone_backslash_contributes_nothing() {
        assert_eq!(file_pat_to_reg_pat(b"a\\", None, false), Some(b"^a$".to_vec()));
    }

    #[test]
    fn file_pat_to_reg_pat_escaped_comma_is_platform_specific() {
        // Hand-traced against the real C control flow: on Windows, the
        // backslash-before-an-isfname-char branch fires for ANY valid
        // 'isfname' character (comma included, per its own default),
        // producing "[\/]" for the backslash itself, then reprocessing
        // the comma on its own as a literal ','. On non-Windows, the
        // backslash instead falls into the "undo ExpandEscape()"
        // un-escaping branch, producing a bare literal ',' with no
        // leading "[\/]" at all.
        if cfg!(windows) {
            assert_eq!(file_pat_to_reg_pat(b"a\\,b", None, false), Some(b"^a[\\/],b$".to_vec()));
        } else {
            assert_eq!(file_pat_to_reg_pat(b"a\\,b", None, false), Some(b"^a,b$".to_vec()));
        }
    }

    #[test]
    fn file_pat_to_reg_pat_escaped_question_mark_is_platform_specific() {
        // Both '*' and '?' are EXPLICITLY listed in the Windows
        // bracket-escape condition regardless of vim_isfilec, so "\?"
        // triggers it there too (reprocessing '?' as its own wildcard
        // afterward); non-Windows un-escapes it straight to a literal
        // '?' character instead.
        if cfg!(windows) {
            assert_eq!(file_pat_to_reg_pat(b"a\\?b", None, false), Some(b"^a[\\/].b$".to_vec()));
        } else {
            assert_eq!(file_pat_to_reg_pat(b"a\\?b", None, false), Some(b"^a?b$".to_vec()));
        }
    }

    #[test]
    fn f_glob2regpat_wraps_file_pat_to_reg_pat() {
        let mut rettv = TypvalT::default();
        unsafe { f_glob2regpat(&[string(b"*.c")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"\\.c$".to_vec())));
    }

    #[test]
    fn f_glob2regpat_type_error_yields_a_null_string() {
        let mut rettv = TypvalT::default();
        // A List value is not "stringish" - tv_get_string_chk returns
        // None for it (matching the original's own str_errors-driven
        // failure for this exact type), which glob2regpat() propagates
        // straight through to a null result string.
        let list_tv = TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() };
        unsafe { f_glob2regpat(&[list_tv], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }
}
