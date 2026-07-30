//! Translated from `src/nvim/os/fs.c` (tractable core only).
//!
//! `os/fs.c` was previously assessed (earlier this session) as entirely
//! blocked on the deferred libuv FFI-vs-Rust-runtime decision
//! (phase 11) - but that decision is really about the *async* event
//! loop/reactor (sockets, pipes, timers, non-blocking I/O), not these
//! plain *synchronous* file operations. libuv's own `uv_fs_*` functions
//! used throughout this file are themselves just portable wrappers
//! around the platform's blocking file-system calls; Rust's
//! `std::fs`/`std::env` already provide the exact same portable,
//! synchronous primitives natively (same reasoning as
//! `os/time.rs`/`os/env.rs`), so this tractable subset is translated
//! now instead of waiting on that decision.
//!
//! Translated: `os_chdir`, `os_dirname`, `os_path_exists`, `os_isdir`,
//! `os_isrealdir`, `os_mkdir`, `os_mkdir_recurse`, `os_file_mkdir`,
//! `os_rmdir`,
//! `os_remove`, `os_rename`, `os_file_settime` (regular files only -
//! see its own doc comment for why directories aren't supported on
//! Windows), `os_realpath`, `os_fsync`, `os_open`,
//! `os_file_is_readable`,
//! `os_file_is_writable` (the latter two via `libc::access`, the
//! same underlying syscall the original's own `uv_fs_access` wraps -
//! needs a `Path` -> `CString` conversion, requiring valid UTF-8, a
//! narrow, documented gap matching `path.rs`'s own established
//! `path_full_dir_name` precedent for the same reason).
//! Functions that in the original return a raw libuv error code
//! (`os_chdir`/`os_mkdir`/`os_rmdir`/`os_remove`/`os_fsync`) are
//! translated to return `0` on success and `-1` on any failure: this
//! collapses libuv's specific per-error-cause codes (`UV_ENOENT`,
//! `UV_EACCES`, etc.) into one generic failure value, since nothing
//! consuming those specific codes is translated yet - revisit if/when
//! a caller needs to distinguish failure causes. `os_open` instead
//! returns `Option<std::fs::File>` directly (the opened resource, not
//! a raw fd/error code) - see its own doc comment.
//!
//! Also translated: `os_fileinfo`/`os_fileinfo_link`/
//! `os_fileinfo_size`/`os_fileinfo_mtime`/`os_fileinfo_type_str` (see
//! [`FileInfoT`]) - but only a narrow subset of the original's own
//! `FileInfo`/`uv_stat_t` (size, modification time, file type), all
//! backed directly by `std::fs::Metadata` rather than a full `stat`
//! translation. In particular, NO raw Unix-style mode/permission bits
//! are modeled (the same still-deferred decision as `os_getperm`
//! below) - `getfperm()`/`eval/fs.c`'s own caller of those bits
//! remains deferred. `os_fileinfo_hardlinks`/`os_fileinfo_blksize`/
//! `os_fileinfo_id`/`os_fileinfo_inode`/`os_fileid*` remain deferred
//! too (no real caller yet needing the fields `std::fs::Metadata`
//! doesn't portably expose).
//!
//! `os_set_cloexec` is intentionally NOT translated (not merely
//! deferred): `std::fs::File`/`OpenOptions` already open every file
//! with `O_CLOEXEC` set atomically on Unix, and with a non-inheritable
//! handle (`bInheritHandle = FALSE`) on Windows, by default - verified
//! against Rust's own standard library behavior. `os_set_cloexec`'s
//! entire job is therefore already done for every file this crate
//! opens; adding an explicit `fcntl(F_SETFD, FD_CLOEXEC)` call on top
//! would be redundant, not a missing translation.
//!
//! Deferred (each needs either the `FileInfo`-vs-`std::fs::Metadata`
//! representation decision, or real byte-level I/O, neither settled
//! yet):
//! - `os_getperm`/`os_setperm`/`os_nodetype`/`os_stat` (raw Unix-style
//!   mode bits - libuv synthesizes these even on Windows for
//!   compatibility; needs a real decision on how to model that
//!   cross-platform rather than rushing it).
//! - `os_fopen`/`os_close`/`os_read`/`os_readv`/`os_write`/`os_dup*`/
//!   `os_copy`: real byte-level file I/O with the raw-fd calling
//!   convention (`memfile.c`'s own `mf_read`/`mf_write`/`mf_close`,
//!   which need this exact shape of I/O, instead go directly through
//!   `std::io::{Read, Write, Seek}` on
//!   `MemfileT.mf_fd: Option<std::fs::File>`, sidestepping the need
//!   for these raw-fd wrappers entirely for that specific caller).
//! - `os_exepath`/`os_can_exe`/`is_executable*`: executable-search
//!   logic tied to `'path'`-searching semantics (`path.c`) and exec-bit
//!   permission checks (`os_getperm`).
//! - `os_copy_xattr`/`os_get_acl`/`os_set_acl`/`os_free_acl`/
//!   `os_file_owned`/`os_chown`/`os_fchown`: platform ACL/xattr/
//!   ownership APIs, out of scope until a real FFI decision is made.
//! - `os_mkdtemp`: unique temp-directory creation via libuv's own
//!   `uv_fs_mkdtemp` random-name generation, not yet ported.
//! - `os_scandir`/`os_scandir_next`/`os_closedir`: need the `Directory`
//!   struct (deferred alongside `FileInfo`/`uv_dirent_t`).
//! - `os_resolve_shortcut`/`os_is_reparse_point_include`: Windows
//!   shortcut (`*.lnk`)/reparse-point resolution via COM
//!   (`IPersistFile`), a genuinely different, more complex API surface
//!   than plain symlink resolution - out of scope until a COM-FFI
//!   decision is made.

use crate::vim_defs::{FAIL, OK};
use std::path::Path;

/// `O_NOFOLLOW`, unified across platforms to match the original's own
/// fallback: real Unix systems define this in `<fcntl.h>` (refuse to
/// open, and fail, if the target path is itself a symlink); Windows'
/// `os/win_defs.h` `#define`s it to `0` (a no-op bit - Windows' CRT
/// `open()` emulation has no equivalent flag). Re-exported here
/// because `os_open` (this module) and `memfile.c`'s `mf_do_open`
/// (`crate::memfile`) both need the exact same value.
#[cfg(unix)]
pub const O_NOFOLLOW: i32 = libc::O_NOFOLLOW;
#[cfg(windows)]
pub use crate::os::win_defs::O_NOFOLLOW;

/// Opens or creates a file, returning the open handle directly
/// (`os_open`).
///
/// `flags` mirrors the small subset of POSIX `open()` flag bits this
/// crate's actual callers need (`memfile.c`'s `mf_open`/
/// `mf_open_file`/`mf_do_open`, the only real call sites so far) via
/// the `libc` crate's cross-platform `O_*` constants (`libc::O_RDWR`,
/// `libc::O_CREAT`, `libc::O_EXCL`, `libc::O_TRUNC` - all defined
/// consistently on both Unix and Windows/MSVC, verified empirically)
/// plus this module's own [`O_NOFOLLOW`]. `O_RDONLY` is `0` (no bits
/// set) on every platform, so "not `O_RDWR`" is treated as read-only -
/// this crate's real call sites never pass `O_WRONLY` alone, so that
/// combination isn't handled specially.
///
/// The original returns a raw file descriptor (or a negative libuv
/// error code) via `uv_fs_open`. This translation instead returns the
/// opened file directly, matching `MemfileT.mf_fd`'s own
/// `Option<std::fs::File>` representation (see that field's doc
/// comment for the general "idiomatic Rust resource, not the C
/// primitive" rationale) - nothing in this crate consumes `os_open`'s
/// result as a raw numeric fd.
///
/// `mode` (Unix permission bits for a newly-created file, e.g.
/// `libc::S_IREAD | libc::S_IWRITE`) is applied via
/// `OpenOptionsExt::mode` on Unix; Windows has no equivalent
/// permission-bits concept for `CreateFile` (matching the original's
/// own libuv backend, which likewise ignores `mode` on Windows), so
/// it's ignored there too.
///
/// When `O_EXCL` is set, this uses `OpenOptions::create_new`, which
/// per Rust's own documentation atomically fails if *anything*
/// already exists at the target path - including a dangling symlink,
/// without following it - on every platform. This gives the real
/// `O_CREAT | O_EXCL` call site (`mf_open_file`) the same
/// symlink-attack protection Unix's `O_NOFOLLOW` would provide, even
/// on Windows, where `O_NOFOLLOW` itself is a documented no-op.
///
/// @return `Some(file)` on success, `None` on failure.
#[must_use]
pub fn os_open(
    path: &Path,
    flags: i32,
    #[cfg_attr(not(unix), allow(unused_variables))] mode: i32,
) -> Option<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    if flags & libc::O_RDWR != 0 {
        opts.read(true).write(true);
    } else {
        opts.read(true);
    }
    if flags & libc::O_EXCL != 0 {
        opts.create_new(true);
    } else if flags & libc::O_CREAT != 0 {
        opts.create(true);
    }
    if flags & libc::O_TRUNC != 0 {
        opts.truncate(true);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode as u32);
        if flags & O_NOFOLLOW != 0 {
            opts.custom_flags(libc::O_NOFOLLOW);
        }
    }
    // Windows: O_NOFOLLOW has no enforceable equivalent (see this
    // module's own O_NOFOLLOW doc comment) - a narrow, documented,
    // accepted gap for the one real caller where O_EXCL is NOT also
    // set (mf_do_open's plain O_RDONLY recovery-open path); the
    // O_CREAT|O_EXCL new-swapfile path is still fully protected on
    // every platform via create_new() above.

    opts.open(path).ok()
}

/// Force any buffered modifications to `file` to be written to disk
/// (`os_fsync`).
///
/// @return `0` for success, `-1` for failure (see the module doc
///         comment for why the original's specific negative libuv
///         error code isn't preserved).
pub fn os_fsync(file: &std::fs::File) -> i32 {
    if file.sync_all().is_ok() {
        0
    } else {
        -1
    }
}

/// Changes the current directory to `path` (`os_chdir`).
///
/// The original also does verbose-logging (`smsg`, gated on
/// `'verbose' >= 5`) and notifies attached UIs (`ui_call_chdir`) on
/// success - both deferred (`message.c`/`ui.c` not yet translated);
/// this covers only the actual directory change.
///
/// @return `0` on success, `-1` on failure (see the module doc comment
///         for why the original's specific negative libuv error code
///         isn't preserved).
pub fn os_chdir(path: &Path) -> i32 {
    match std::env::set_current_dir(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Get the name of the current directory, with backslashes normalized
/// to forward slashes (`os_dirname`).
///
/// Simplified from the original's caller-supplied-buffer-plus-length
/// (`OK`/`FAIL` return) to an owned `Option<Vec<u8>>` - same convention
/// already used by `crate::os::stdpaths::get_appname`.
#[must_use]
pub fn os_dirname() -> Option<Vec<u8>> {
    let dir = std::env::current_dir().ok()?;
    let mut bytes = dir.to_string_lossy().into_owned().into_bytes();
    crate::path::path_to_slash(&mut bytes);
    Some(bytes)
}

/// Check if a path exists (`os_path_exists`).
#[must_use]
pub fn os_path_exists(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
}

/// Convert a `Path` to a `CString` for a `libc` call, matching this
/// crate's established (documented, narrow) "requires valid UTF-8"
/// convention already used by `path.rs`'s own `path_full_dir_name`
/// for the same reason (`std::path::Path` on Windows is natively
/// UTF-16; `libc::access` needs a narrow, NUL-terminated C string).
fn path_to_cstring(path: &Path) -> Option<std::ffi::CString> {
    std::ffi::CString::new(path.to_str()?).ok()
}

// The `libc` crate doesn't export R_OK/W_OK for the Windows target
// (its own `_access` from the MSVC CRT uses the identical, POSIX-
// inherited values - confirmed against Microsoft's own `_access`
// documentation), so they're defined here directly rather than via
// `libc::R_OK`/`libc::W_OK` (available on Unix only).
const R_OK: i32 = 4;
const W_OK: i32 = 2;

/// Check if a file is readable (`os_file_is_readable`), via
/// [`libc::access`] with `R_OK` - the same underlying syscall the
/// original's own `uv_fs_access` wraps.
#[must_use]
pub fn os_file_is_readable(path: &Path) -> bool {
    let Some(cpath) = path_to_cstring(path) else { return false };
    // SAFETY: cpath is a valid, NUL-terminated C string for its own
    // lifetime, which outlives this call.
    unsafe { libc::access(cpath.as_ptr(), R_OK) == 0 }
}

/// Check if a file is writable (`os_file_is_writable`), via
/// [`libc::access`] with `W_OK`.
///
/// @return `0` if `path` is not writable, `1` if it's a writable
///         file, `2` if it's a directory with write access.
#[must_use]
pub fn os_file_is_writable(path: &Path) -> i32 {
    let Some(cpath) = path_to_cstring(path) else { return 0 };
    // SAFETY: forwarded from os_file_is_readable's own safety doc.
    let writable = unsafe { libc::access(cpath.as_ptr(), W_OK) == 0 };
    if !writable {
        return 0;
    }
    if os_isdir(path) { 2 } else { 1 }
}

/// Check if the given path exists and is a directory (`os_isdir`).
///
/// Simplified from the original's `os_getperm()`-based `S_ISDIR` check
/// to `std::fs::metadata` directly - same observable "true iff `name`
/// exists and is a directory" contract, without needing to replicate
/// libuv's cross-platform `st_mode` bit synthesis (see the module doc
/// comment's note on the deferred `os_getperm`).
#[must_use]
pub fn os_isdir(name: &Path) -> bool {
    std::fs::metadata(name).is_ok_and(|m| m.is_dir())
}

/// Check if the given path is a directory and not a symlink to a
/// directory (`os_isrealdir`).
///
/// @return `true` if `name` is a directory and NOT a symlink to a
///         directory, `false` if `name` is not a directory or an error
///         occurred.
#[must_use]
pub fn os_isrealdir(name: &Path) -> bool {
    match std::fs::symlink_metadata(name) {
        Ok(meta) => !meta.is_symlink() && meta.is_dir(),
        Err(_) => false,
    }
}

/// A narrow subset of the original's own `FileInfo` (`fs_defs.h`,
/// itself just a thin wrapper over libuv's own `uv_stat_t`) - only
/// what [`os_fileinfo_size`]/[`os_fileinfo_mtime`]/
/// [`os_fileinfo_type_str`] need (size, modification time, file
/// type), backed directly by `std::fs::Metadata` rather than a full
/// `stat`/`uv_stat_t` translation. Deliberately has NO raw Unix-style
/// mode/permission bits - see this module's own top doc comment for
/// why (the same still-deferred decision as `os_getperm`).
#[derive(Debug)]
pub struct FileInfoT {
    metadata: std::fs::Metadata,
}

/// Get information about a file, following symlinks (`os_fileinfo`).
///
/// @return `None` on failure (`path` doesn't exist, or some other
///         `stat`-style error), matching the original's own `bool`
///         success/failure return (this crate's own idiom folds the
///         out-parameter and the status into one `Option`).
#[must_use]
pub fn os_fileinfo(path: &Path) -> Option<FileInfoT> {
    std::fs::metadata(path).ok().map(|metadata| FileInfoT { metadata })
}

/// Get information about a file, WITHOUT following a trailing symlink
/// (`os_fileinfo_link`).
#[must_use]
pub fn os_fileinfo_link(path: &Path) -> Option<FileInfoT> {
    std::fs::symlink_metadata(path).ok().map(|metadata| FileInfoT { metadata })
}

/// Get the file size from a `FileInfoT` (`os_fileinfo_size`).
#[must_use]
pub fn os_fileinfo_size(info: &FileInfoT) -> u64 {
    info.metadata.len()
}

/// Get the last modification time from a `FileInfoT`, as seconds
/// since the Unix epoch (`file_info->stat.st_mtim.tv_sec` in the
/// original). `0` if the platform can't report a modification time,
/// or it's before the epoch (matching this crate's established
/// "narrow, documented gap rather than a panic" convention for
/// awkward corners of an otherwise-tractable function - a modern
/// file's mtime is essentially never actually before 1970).
#[must_use]
pub fn os_fileinfo_mtime(info: &FileInfoT) -> i64 {
    info.metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

/// Get a `getftype()`-style file-type description from a `FileInfoT`
/// (mirrors the original's own `f_getftype`'s `S_ISREG`/`S_ISDIR`/
/// `S_ISLNK`/`S_ISBLK`/`S_ISCHR`/`S_ISFIFO`/`S_ISSOCK` dispatch,
/// inlined here rather than as a separate `os_nodetype`-style function
/// since this crate has no other caller for it yet). The block/char-
/// device, FIFO, and socket distinctions are Unix-only (via
/// `std::os::unix::fs::FileTypeExt`) - Windows has no equivalent
/// concept, so those 4 variants are unreachable there, matching
/// `std::fs::FileType`'s own platform-native capabilities.
#[must_use]
pub fn os_fileinfo_type_str(info: &FileInfoT) -> &'static str {
    let ft = info.metadata.file_type();
    if ft.is_symlink() {
        return "link";
    }
    if ft.is_dir() {
        return "dir";
    }
    if ft.is_file() {
        return "file";
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if ft.is_block_device() {
            return "bdev";
        }
        if ft.is_char_device() {
            return "cdev";
        }
        if ft.is_fifo() {
            return "fifo";
        }
        if ft.is_socket() {
            return "socket";
        }
    }
    "other"
}

/// Resolve `name` to its canonical (symlink-free, absolute) path
/// (`os_realpath`).
///
/// Simplified from the original's caller-supplied-buffer-plus-length
/// contract to an owned `Option<Vec<u8>>` - same convention already
/// used by [`os_dirname`].
///
/// @return `Some(real_path)` on success, `None` on failure.
#[must_use]
pub fn os_realpath(name: &Path) -> Option<Vec<u8>> {
    let real = std::fs::canonicalize(name).ok()?;
    let mut bytes = real.to_string_lossy().into_owned().into_bytes();
    // `std::fs::canonicalize` returns Windows's `\\?\`-prefixed
    // "verbatim" extended-length paths (e.g. `\\?\C:\foo`, or
    // `\\?\UNC\server\share` for UNC paths); libuv's `uv_fs_realpath`
    // (what the original wraps) strips this prefix so plain drive-
    // letter/UNC paths come back out, matching what the rest of this
    // codebase's path functions (e.g. `path_has_drive_letter`) expect.
    // This normalization is a no-op on non-Windows targets.
    strip_windows_verbatim_prefix(&mut bytes);
    crate::path::path_to_slash(&mut bytes);
    Some(bytes)
}

/// Strips a Windows extended-length-path `\\?\` prefix in place,
/// converting `\\?\UNC\server\share` back to `\\server\share` and
/// `\\?\C:\foo` back to `C:\foo`. No-op if the prefix isn't present.
fn strip_windows_verbatim_prefix(path: &mut Vec<u8>) {
    const VERBATIM_UNC_PREFIX: &[u8] = br"\\?\UNC\";
    const VERBATIM_PREFIX: &[u8] = br"\\?\";
    if path.starts_with(VERBATIM_UNC_PREFIX) {
        let rest = path[VERBATIM_UNC_PREFIX.len()..].to_vec();
        path.clear();
        path.extend_from_slice(br"\\");
        path.extend_from_slice(&rest);
    } else if path.starts_with(VERBATIM_PREFIX) {
        path.drain(..VERBATIM_PREFIX.len());
    }
}

/// Make a directory (`os_mkdir`).
///
/// `mode` (Unix permission bits) is applied on Unix via
/// `std::os::unix::fs::DirBuilderExt::mode`; Windows directories have
/// no equivalent concept, so `mode` is ignored there, matching the
/// original's own libuv backend (`uv_fs_mkdir` likewise ignores `mode`
/// on Windows).
///
/// @return `0` for success, `-1` for failure.
pub fn os_mkdir(path: &Path, #[cfg_attr(not(unix), allow(unused_variables))] mode: i32) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(mode as u32);
        match builder.create(path) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }
    #[cfg(not(unix))]
    {
        match std::fs::create_dir(path) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }
}

/// Returns `true` if `&dir[..end]` is valid UTF-8 and names an
/// existing directory - the "no directory found yet" test both
/// phases of [`os_mkdir_recurse`] need. Invalid UTF-8 is treated as
/// "not a directory" (this crate's established "invalid UTF-8 is
/// treated the same as a nonexistent path" convention - see
/// `path.rs`'s own `dir_of_file_exists`).
fn is_dir_prefix(dir: &[u8], end: usize) -> bool {
    match std::str::from_utf8(&dir[..end]) {
        Ok(s) => os_isdir(Path::new(s)),
        Err(_) => false,
    }
}

/// Make a directory, with higher levels when needed
/// (`os_mkdir_recurse`).
///
/// Returns `Ok(created)` on success - `created` is the full path of
/// the first directory this call actually created (`None` if `dir`
/// already existed and nothing needed to be created), matching the
/// original's own `*created` out-parameter (left `NULL` in that same
/// case). Returns `Err(failed_dir)` on failure - `failed_dir` is the
/// specific directory `os_mkdir` itself failed on (may be an
/// intermediate level, not necessarily `dir` itself), matching the
/// original's own `*failed_dir` out-parameter. The original's own
/// separate libuv error code is folded into this simple `Ok`/`Err`
/// split, matching [`os_mkdir`]'s own already-established "0 success /
/// -1 failure" simplification (no specific error CAUSE is surfaced
/// anywhere in this crate yet).
///
/// # Algorithm
/// Mirrors the original's own two-phase pointer-truncation walk, but
/// using a `Vec<usize>` of byte offsets ("boundaries") into `dir`
/// instead of embedding NUL bytes into a mutable buffer (a Rust slice
/// already supports cheap, safe truncation by index, with no sentinel
/// byte needed - the original's own repeated `*e = NUL`/`*e =
/// PATHSEP` byte-patching is really just recomputing "how much of
/// `dir`, from the start, should currently be considered live"):
///
/// 1. Shrinking phase: starting from `boundaries = [dir.len()]`,
///    while `dir[..*last boundary]` does NOT already name an existing
///    directory, shrink by one more trailing path component (via
///    [`crate::path::path_tail_with_sep`]) and push the new boundary
///      - until an existing directory IS found (the loop stops,
///        keeping that boundary as the last element), or the boundary
///        has shrunk back to [`crate::path::get_past_head`] (the
///        root), in which case `past_head` itself becomes the final
///        boundary and the loop stops unconditionally (matching the
///        original's own early `break`, with no further `os_isdir`
///        re-check of the root).
/// 2. Creation phase: walk the recorded boundaries, EXCLUDING the
///    last one (already confirmed to exist, or the assumed-to-exist
///    root), from shallowest to deepest - i.e. in the REVERSE of the
///    order they were pushed - calling [`os_mkdir`] at each
///    successively longer prefix. This creates every missing
///    intermediate level, matching the original's own "restore one
///    truncation point, `os_mkdir`, repeat" loop exactly. The
///    original's own "the path ends in trailing separators only"
///    special case (silently skip creating a final, all-separator
///    segment, e.g. a trailing `"///"`) is checked only for the LAST
///    creation step (the one reaching `dir.len()` exactly), matching
///    the original's own `e == real_end` guard.
pub fn os_mkdir_recurse(dir: &[u8], mode: i32) -> Result<Option<Vec<u8>>, Vec<u8>> {
    let past_head = crate::path::get_past_head(dir);
    let real_end = dir.len();

    let mut boundaries = vec![real_end];
    loop {
        let cur_end = *boundaries.last().unwrap();
        if is_dir_prefix(dir, cur_end) {
            break;
        }
        let e = crate::path::path_tail_with_sep(&dir[..cur_end]);
        if e <= past_head {
            boundaries.push(past_head);
            break;
        }
        boundaries.push(e);
    }

    let mut created = None;
    for i in (0..boundaries.len().saturating_sub(1)).rev() {
        let end = boundaries[i];
        if end == real_end {
            // Path ends with something like "////" - ignore this.
            let segment = &dir[boundaries[i + 1]..end];
            if !segment.is_empty() && crate::memory::memcnt(segment, crate::ascii_defs::PATHSEP) == segment.len() {
                break;
            }
        }
        let Ok(prefix_str) = std::str::from_utf8(&dir[..end]) else {
            return Err(dir[..end].to_vec());
        };
        if os_mkdir(Path::new(prefix_str), mode) != 0 {
            return Err(dir[..end].to_vec());
        }
        if created.is_none() {
            created = crate::path::full_name_save(Some(&dir[..end]), false);
        }
    }

    Ok(created)
}

/// Create the parent directory of a file if it does not exist
/// (`os_file_mkdir`).
///
/// @param fname Full path of the file name whose parent directories
///              we want to create
/// @param mode  Permissions for the newly-created directory.
///
/// @return `0` for success, `-1` for failure - matches
/// [`os_mkdir_recurse`]'s own already-established simplification of
/// the original's separate libuv error code (never surfaced anywhere
/// in this crate). NOTE: this is the plain `0`/`-1` libuv-style
/// convention (same as [`os_mkdir`]/[`os_mkdir_recurse`]), NOT the
/// Vimscript `OK`(`1`)/`FAIL`(`0`) boolean convention - the original's
/// own doc comment says exactly "0 for success, libuv error code for
/// failure", not "OK"/"FAIL". The original's own `emsg`/`semsg`
/// message-display calls on both failure branches are skipped,
/// keeping the exact same `-1` return value (this crate's established
/// "skip the deferred message-display side effect" policy).
#[must_use]
pub fn os_file_mkdir(fname: &[u8], mode: i32) -> i32 {
    if crate::path::dir_of_file_exists(fname) {
        return 0;
    }

    let tail = crate::path::path_tail_with_sep(fname);

    // The original's own `*last_char = tail + strlen(tail) - 1`
    // reads `fname`'s OWN LAST byte (since `tail` always points at or
    // before the very end) - checking whether `fname` ends in a path
    // separator itself, i.e. has no real file-name component at all
    // (e.g. "/foo/bar/"). An empty `fname` can never reach here in
    // practice (the `dir_of_file_exists` check above already returns
    // `true` for it via `path_tail_with_sep`'s own `tail == 0` fast
    // path, matching the original's own control flow exactly), but
    // `fname.last()` is still guarded defensively rather than
    // unwrapped blindly.
    if fname.last().is_none_or(|&b| crate::path::vim_ispathsep(i32::from(b))) {
        // E32: No file name.
        return -1;
    }

    match os_mkdir_recurse(&fname[..tail], mode) {
        Ok(_) => 0,
        Err(_failed_dir) => -1,
    }
}

/// Remove a directory (`os_rmdir`).
///
/// @return `0` for success, `-1` for failure.
pub fn os_rmdir(path: &Path) -> i32 {
    match std::fs::remove_dir(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Remove a file (`os_remove`).
///
/// @return `0` for success, non-zero for failure.
pub fn os_remove(path: &Path) -> i32 {
    match std::fs::remove_file(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Rename a file or directory (`os_rename`).
///
/// @return `OK` for success, `FAIL` for failure.
pub fn os_rename(path: &Path, new_path: &Path) -> i32 {
    if std::fs::rename(path, new_path).is_ok() {
        OK
    } else {
        FAIL
    }
}

/// Converts a Unix timestamp (seconds since the epoch, possibly with
/// a fractional part) into a `SystemTime`. Returns `None` for a
/// negative value (a pre-1970 timestamp) instead of panicking -
/// `Duration::from_secs_f64` itself panics on negative input, a case
/// the original's own bare `double` parameter has no equivalent
/// restriction against. Neither of [`os_file_settime`]'s own two real
/// callers (both in `bufwrite.c`, not yet translated) can ever
/// actually produce one in practice, since they always come from an
/// existing file's own real `stat` timestamps, but this guards the
/// theoretical case explicitly rather than relying on that.
fn unix_timestamp_to_system_time(ts: f64) -> Option<std::time::SystemTime> {
    if ts < 0.0 {
        return None;
    }
    Some(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs_f64(ts))
}

/// Set the access and modification times of a file (`os_file_settime`).
///
/// `atime`/`mtime` are Unix timestamps in seconds (with a fractional
/// part for sub-second precision), matching the original's own bare
/// `double` parameters.
///
/// Only regular files are supported here, not directories: setting
/// this requires opening the target first (`std::fs::File::
/// set_times`, stable since Rust 1.75), and `std::fs::OpenOptions`
/// cannot portably open a directory at all on Windows (confirmed via
/// a standalone scratch program - `CreateFile` on a directory needs
/// `FILE_FLAG_BACKUP_SEMANTICS`, which Rust's std doesn't set) - a
/// real, underlying platform-API limitation, not a shortcut taken
/// here. Opening requires WRITE access specifically (also confirmed
/// via a standalone scratch program - a read-only handle's
/// `set_times` call fails with "Access is denied" on Windows), so
/// this always opens with `.write(true)`.
///
/// @return `OK` for success, `FAIL` for failure.
pub fn os_file_settime(path: &Path, atime: f64, mtime: f64) -> i32 {
    let (Some(atime), Some(mtime)) = (unix_timestamp_to_system_time(atime), unix_timestamp_to_system_time(mtime))
    else {
        return FAIL;
    };
    let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) else {
        return FAIL;
    };
    let times = std::fs::FileTimes::new().set_accessed(atime).set_modified(mtime);
    if file.set_times(times).is_ok() {
        OK
    } else {
        FAIL
    }
}

/// Serializes tests that read or mutate the real, process-wide current
/// working directory (`std::env::current_dir`/`set_current_dir`) -
/// genuine OS-level global state shared by every thread in this test
/// binary (Rust's test harness runs tests concurrently across
/// threads), unlike a per-test temp directory. Without this, a chdir
/// test running concurrently with a cwd-*reading* test (e.g. in
/// `path.rs`, which may read cwd more than once within a single test)
/// could observe the directory change mid-test, causing a rare,
/// non-deterministic failure - confirmed to happen in practice (one
/// `path.rs` test failed once across dozens of repeated full-suite
/// runs before this lock was added, traced to exactly this race with
/// `os_chdir`'s own test).
///
/// Acquire this for the entire body of any test that reads OR writes
/// the real current directory - even read-only tests need it, since a
/// concurrent writer could still invalidate an in-progress multi-read
/// sequence. Uses `PoisonError::into_inner` so one panicking test
/// under the lock doesn't permanently poison it for every subsequent
/// test that needs it.
#[cfg(test)]
pub(crate) fn cwd_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A unique-per-test scratch directory under the system temp dir,
    /// removed on drop even if the test panics (RAII), so concurrently
    /// running tests never collide and never leak files.
    struct TempScratch {
        path: std::path::PathBuf,
    }

    impl TempScratch {
        fn new(name: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut path = std::env::temp_dir();
            path.push(format!(
                "nero_fs_test_{name}_{}_{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            TempScratch { path }
        }
    }

    impl Drop for TempScratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn strip_windows_verbatim_prefix_removes_plain_prefix() {
        let mut p = br"\\?\C:\Users\test".to_vec();
        strip_windows_verbatim_prefix(&mut p);
        assert_eq!(p, br"C:\Users\test");
    }

    #[test]
    fn strip_windows_verbatim_prefix_converts_unc_prefix() {
        let mut p = br"\\?\UNC\server\share\dir".to_vec();
        strip_windows_verbatim_prefix(&mut p);
        assert_eq!(p, br"\\server\share\dir");
    }

    #[test]
    fn strip_windows_verbatim_prefix_is_noop_without_prefix() {
        let mut p = br"C:\Users\test".to_vec();
        strip_windows_verbatim_prefix(&mut p);
        assert_eq!(p, br"C:\Users\test");
    }

    #[test]
    fn os_realpath_resolves_and_has_no_verbatim_prefix() {
        let scratch = TempScratch::new("realpath");
        let resolved = os_realpath(&scratch.path).expect("scratch dir exists");
        assert!(!resolved.starts_with(br"\\?\"));
        // The resolved path must still point at the same real
        // directory (compare canonicalized to sidestep any 8.3-name or
        // case differences).
        let resolved_path = std::path::Path::new(std::str::from_utf8(&resolved).unwrap());
        assert_eq!(
            resolved_path.canonicalize().unwrap(),
            scratch.path.canonicalize().unwrap()
        );
    }

    #[test]
    fn os_realpath_returns_none_for_missing_path() {
        let scratch = TempScratch::new("realpath_missing");
        assert_eq!(os_realpath(&scratch.path.join("does_not_exist")), None);
    }

    #[test]
    fn os_fsync_succeeds_on_a_writable_file() {
        let scratch = TempScratch::new("fsync");
        let path = scratch.path.join("f.txt");
        let file = std::fs::File::create(&path).unwrap();
        assert_eq!(os_fsync(&file), 0);
    }

    #[test]
    fn os_open_rdonly_reads_an_existing_file() {
        let scratch = TempScratch::new("open_rdonly");
        let path = scratch.path.join("f.txt");
        std::fs::write(&path, b"hello").unwrap();

        let mut file = os_open(&path, libc::O_RDONLY, 0).expect("file exists");
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut buf).unwrap();
        assert_eq!(buf, b"hello");
    }

    #[test]
    fn os_open_rdonly_fails_for_a_missing_file() {
        let scratch = TempScratch::new("open_rdonly_missing");
        let path = scratch.path.join("does_not_exist.txt");
        assert!(os_open(&path, libc::O_RDONLY, 0).is_none());
    }

    #[test]
    fn os_open_rdwr_creat_excl_creates_and_writes_a_new_file() {
        let scratch = TempScratch::new("open_creat_excl");
        let path = scratch.path.join("new.txt");

        // S_IREAD/S_IWRITE's type varies by platform in the libc crate
        // (i32 on Windows, u32 on Linux); `as i32` unifies them for
        // this always-small, always-positive value. clippy flags this
        // as redundant on whichever single target it happens to check
        // (both Windows and Linux already use i32) - allowed
        // explicitly since it's still required for portability to any
        // Unix libc where these are u32.
        #[allow(clippy::unnecessary_cast)]
        let mode = (libc::S_IREAD | libc::S_IWRITE) as i32;
        let mut file = os_open(&path, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL, mode)
            .expect("new file should be creatable");
        std::io::Write::write_all(&mut file, b"data").unwrap();
        drop(file);

        assert_eq!(std::fs::read(&path).unwrap(), b"data");
    }

    #[test]
    fn os_open_rdwr_creat_excl_fails_if_file_already_exists() {
        let scratch = TempScratch::new("open_creat_excl_exists");
        let path = scratch.path.join("existing.txt");
        std::fs::write(&path, b"pre-existing").unwrap();

        #[allow(clippy::unnecessary_cast)]
        let mode = (libc::S_IREAD | libc::S_IWRITE) as i32;
        // O_EXCL must refuse to open/create when something is already
        // there - the exact "symlink attack" protection mf_open_file
        // relies on (see os_open's own doc comment).
        assert!(os_open(&path, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL, mode).is_none());
        // The pre-existing content must be untouched.
        assert_eq!(std::fs::read(&path).unwrap(), b"pre-existing");
    }

    #[test]
    fn os_open_truncates_when_o_trunc_is_set() {
        let scratch = TempScratch::new("open_trunc");
        let path = scratch.path.join("f.txt");
        std::fs::write(&path, b"old contents").unwrap();

        let file = os_open(&path, libc::O_RDWR | libc::O_TRUNC, 0).expect("file exists");
        drop(file);
        assert_eq!(std::fs::read(&path).unwrap(), b"");
    }

    #[cfg(unix)]
    #[test]
    fn os_open_with_o_nofollow_refuses_a_symlink() {
        let scratch = TempScratch::new("open_nofollow_unix");
        let target = scratch.path.join("target.txt");
        std::fs::write(&target, b"real file").unwrap();
        let link = scratch.path.join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(os_open(&link, libc::O_RDONLY | O_NOFOLLOW, 0).is_none());
        // Without O_NOFOLLOW, the symlink is followed normally.
        assert!(os_open(&link, libc::O_RDONLY, 0).is_some());
    }

    #[test]
    fn os_path_exists_and_os_isdir_distinguish_files_and_dirs() {
        let scratch = TempScratch::new("exists");
        let file_path = scratch.path.join("a_file.txt");
        std::fs::File::create(&file_path)
            .unwrap()
            .write_all(b"x")
            .unwrap();

        assert!(os_path_exists(&scratch.path));
        assert!(os_path_exists(&file_path));
        assert!(!os_path_exists(&scratch.path.join("does_not_exist")));

        assert!(os_isdir(&scratch.path));
        assert!(!os_isdir(&file_path));
        assert!(!os_isdir(&scratch.path.join("does_not_exist")));
    }

    #[test]
    fn os_isrealdir_rejects_files() {
        let scratch = TempScratch::new("isrealdir");
        let file_path = scratch.path.join("a_file.txt");
        std::fs::File::create(&file_path).unwrap();

        assert!(os_isrealdir(&scratch.path));
        assert!(!os_isrealdir(&file_path));
    }

    #[test]
    fn os_mkdir_rmdir_roundtrip() {
        let scratch = TempScratch::new("mkdir_rmdir");
        let new_dir = scratch.path.join("child");

        assert_eq!(os_mkdir(&new_dir, 0o755), 0);
        assert!(os_isdir(&new_dir));

        assert_eq!(os_rmdir(&new_dir), 0);
        assert!(!os_path_exists(&new_dir));
    }

    #[test]
    fn os_mkdir_fails_when_parent_missing() {
        let scratch = TempScratch::new("mkdir_fail");
        let deep = scratch.path.join("missing_parent").join("child");
        assert_eq!(os_mkdir(&deep, 0o755), -1);
    }

    /// Converts a `Path` into the `&[u8]`-based representation
    /// `os_mkdir_recurse` (and most of this crate's other path-taking
    /// functions) expects. Every scratch-directory path in these tests
    /// is built from plain ASCII components, so this always succeeds.
    fn path_bytes(p: &std::path::Path) -> Vec<u8> {
        p.to_str().unwrap().as_bytes().to_vec()
    }

    /// Like [`path_bytes`], but additionally normalizes to
    /// forward-slash separators - matching `full_name_save`'s own
    /// unconditional `path_to_slash` normalization (via
    /// `vim_full_name`), so this can be compared byte-for-byte against
    /// `os_mkdir_recurse`'s own returned `created` path on EVERY
    /// platform, including Windows (where a plain `Path`/`PathBuf`
    /// would otherwise stringify with backslashes).
    fn full_name_bytes(p: &std::path::Path) -> Vec<u8> {
        p.to_str().unwrap().replace('\\', "/").into_bytes()
    }

    #[test]
    fn os_mkdir_recurse_creates_every_missing_level() {
        let scratch = TempScratch::new("mkdir_recurse_all_missing");
        let target = scratch.path.join("a").join("b").join("c");
        let target_bytes = path_bytes(&target);

        let created = os_mkdir_recurse(&target_bytes, 0o755).expect("should succeed");

        assert!(os_isdir(&target));
        assert!(os_isdir(&scratch.path.join("a")));
        assert!(os_isdir(&scratch.path.join("a").join("b")));
        // The FIRST directory actually created is the shallowest
        // missing one ("a"), matching the original's own *created
        // out-parameter semantics.
        assert_eq!(created, Some(full_name_bytes(&scratch.path.join("a"))));
    }

    #[test]
    fn os_mkdir_recurse_is_a_noop_when_the_full_path_already_exists() {
        let scratch = TempScratch::new("mkdir_recurse_already_exists");
        let target = scratch.path.join("already").join("here");
        std::fs::create_dir_all(&target).unwrap();
        let target_bytes = path_bytes(&target);

        let created = os_mkdir_recurse(&target_bytes, 0o755).expect("should succeed");

        assert!(created.is_none(), "nothing should have been created");
        assert!(os_isdir(&target));
    }

    #[test]
    fn os_mkdir_recurse_creates_only_the_missing_tail() {
        let scratch = TempScratch::new("mkdir_recurse_partial");
        // "a" already exists; "a/b/c" does not.
        let existing = scratch.path.join("a");
        std::fs::create_dir_all(&existing).unwrap();
        let target = existing.join("b").join("c");
        let target_bytes = path_bytes(&target);

        let created = os_mkdir_recurse(&target_bytes, 0o755).expect("should succeed");

        assert!(os_isdir(&target));
        // The first NEW directory created is "a/b" (the shallowest
        // missing level), not "a" (already existed) or "a/b/c".
        assert_eq!(created, Some(full_name_bytes(&existing.join("b"))));
    }

    #[test]
    fn os_mkdir_recurse_fails_when_a_file_blocks_an_intermediate_level() {
        let scratch = TempScratch::new("mkdir_recurse_blocked");
        // Create a plain FILE at "blocker", then try to create
        // "blocker/child" underneath it - "blocker" can never become a
        // directory, so os_mkdir must fail at exactly that level (the
        // shallowest missing one, processed BEFORE "blocker/child" is
        // ever attempted).
        let blocker = scratch.path.join("blocker");
        std::fs::write(&blocker, b"not a directory").unwrap();
        let target = blocker.join("child");
        let target_bytes = path_bytes(&target);

        let err = os_mkdir_recurse(&target_bytes, 0o755).expect_err("should fail");
        assert_eq!(err, path_bytes(&blocker));
        assert!(!os_isdir(&target));
    }

    #[test]
    fn os_mkdir_recurse_ignores_trailing_separators() {
        let scratch = TempScratch::new("mkdir_recurse_trailing_sep");
        let target = scratch.path.join("a").join("b");
        // Deliberately append a trailing separator run - matches the
        // original's own "path ends with something like '////'" case,
        // which must be silently ignored rather than attempted as its
        // own (nonsensical, already-covered) creation step.
        let mut target_bytes = path_bytes(&target);
        target_bytes.extend_from_slice(b"///");

        let created = os_mkdir_recurse(&target_bytes, 0o755).expect("should succeed");

        assert!(os_isdir(&target));
        assert!(created.is_some(), "the real levels should still have been created");
    }

    #[test]
    fn os_file_mkdir_is_a_noop_when_parent_already_exists() {
        let scratch = TempScratch::new("file_mkdir_parent_exists");
        let fname = scratch.path.join("file.txt");
        assert_eq!(os_file_mkdir(&path_bytes(&fname), 0o755), 0);
        // Nothing besides the already-existing scratch dir should
        // exist - os_file_mkdir never creates the FILE itself, only
        // parent directories.
        assert!(!os_path_exists(&fname));
    }

    #[test]
    fn os_file_mkdir_creates_missing_parent_directories() {
        let scratch = TempScratch::new("file_mkdir_creates_parents");
        let fname = scratch.path.join("a").join("b").join("file.txt");
        assert_eq!(os_file_mkdir(&path_bytes(&fname), 0o755), 0);
        assert!(os_isdir(&scratch.path.join("a")));
        assert!(os_isdir(&scratch.path.join("a").join("b")));
        assert!(!os_path_exists(&fname), "the FILE itself is never created");
    }

    #[test]
    fn os_file_mkdir_fails_when_fname_has_no_real_file_name() {
        let scratch = TempScratch::new("file_mkdir_no_filename");
        // Ends in a trailing separator: "a/b/" - the "file name"
        // portion is empty, matching the original's own "E32: No file
        // name" branch.
        let mut fname_bytes = path_bytes(&scratch.path.join("a").join("b"));
        fname_bytes.push(b'/');
        assert_eq!(os_file_mkdir(&fname_bytes, 0o755), -1);
    }

    #[test]
    fn os_file_mkdir_fails_when_a_file_blocks_an_intermediate_level() {
        let scratch = TempScratch::new("file_mkdir_blocked");
        let blocker = scratch.path.join("blocker");
        std::fs::write(&blocker, b"not a directory").unwrap();
        let fname = blocker.join("child").join("file.txt");
        assert_eq!(os_file_mkdir(&path_bytes(&fname), 0o755), -1);
    }

    #[test]
    fn os_remove_deletes_a_file() {
        let scratch = TempScratch::new("remove");
        let file_path = scratch.path.join("to_delete.txt");
        std::fs::File::create(&file_path).unwrap();
        assert!(os_path_exists(&file_path));

        assert_eq!(os_remove(&file_path), 0);
        assert!(!os_path_exists(&file_path));
    }

    #[test]
    fn os_remove_fails_for_missing_file() {
        let scratch = TempScratch::new("remove_missing");
        assert_eq!(os_remove(&scratch.path.join("nope.txt")), -1);
    }

    #[test]
    fn os_rename_moves_a_file() {
        let scratch = TempScratch::new("rename");
        let src = scratch.path.join("src.txt");
        let dst = scratch.path.join("dst.txt");
        std::fs::File::create(&src).unwrap();

        assert_eq!(os_rename(&src, &dst), OK);
        assert!(!os_path_exists(&src));
        assert!(os_path_exists(&dst));
    }

    #[test]
    fn os_rename_fails_for_missing_source() {
        let scratch = TempScratch::new("rename_missing");
        let src = scratch.path.join("nope.txt");
        let dst = scratch.path.join("dst.txt");
        assert_eq!(os_rename(&src, &dst), FAIL);
    }

    #[test]
    fn os_file_settime_sets_atime_and_mtime() {
        let scratch = TempScratch::new("file_settime");
        let path = scratch.path.join("target.txt");
        std::fs::write(&path, b"hello").unwrap();

        // An arbitrary, well-in-the-past timestamp - 2000-01-01
        // 00:00:00 UTC - distinct from "now" so a real change is
        // actually observable.
        let target_secs: f64 = 946_684_800.0;

        assert_eq!(os_file_settime(&path, target_secs, target_secs), OK);

        let metadata = std::fs::metadata(&path).unwrap();
        let mtime = metadata.modified().unwrap();
        let expected = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs_f64(target_secs);
        assert_eq!(mtime, expected);
    }

    #[test]
    fn os_file_settime_fails_for_a_missing_file() {
        let scratch = TempScratch::new("file_settime_missing");
        let path = scratch.path.join("does_not_exist.txt");
        assert_eq!(os_file_settime(&path, 1_000_000.0, 1_000_000.0), FAIL);
    }

    #[test]
    fn os_file_settime_fails_for_a_negative_timestamp() {
        let scratch = TempScratch::new("file_settime_negative");
        let path = scratch.path.join("target.txt");
        std::fs::write(&path, b"hello").unwrap();
        assert_eq!(os_file_settime(&path, -1.0, 1_000_000.0), FAIL);
        assert_eq!(os_file_settime(&path, 1_000_000.0, -1.0), FAIL);
    }

    #[test]
    fn unix_timestamp_to_system_time_rejects_negative_values() {
        assert!(unix_timestamp_to_system_time(-0.001).is_none());
    }

    #[test]
    fn unix_timestamp_to_system_time_accepts_zero_and_epoch() {
        assert_eq!(unix_timestamp_to_system_time(0.0), Some(std::time::SystemTime::UNIX_EPOCH));
    }

    #[test]
    fn os_dirname_returns_current_dir_with_forward_slashes() {
        let _guard = cwd_test_lock();
        let dir = os_dirname().expect("current dir should be readable");
        assert!(!dir.is_empty());
        assert!(!dir.contains(&b'\\'));
    }

    #[test]
    fn os_chdir_changes_and_reports_failure_for_missing_dir() {
        let _guard = cwd_test_lock();
        let original = std::env::current_dir().unwrap();
        let scratch = TempScratch::new("chdir");

        assert_eq!(os_chdir(&scratch.path), 0);
        let now = std::env::current_dir().unwrap();
        // Compare canonicalized paths since chdir may resolve symlinks
        // differently than the raw scratch path string.
        assert_eq!(
            now.canonicalize().unwrap(),
            scratch.path.canonicalize().unwrap()
        );

        // Restore, since current_dir is genuine global process state
        // shared by every test thread.
        assert_eq!(os_chdir(&original), 0);

        assert_eq!(os_chdir(&scratch.path.join("does_not_exist")), -1);
    }

    #[test]
    fn os_file_is_readable_true_for_an_existing_file() {
        let scratch = TempScratch::new("readable_existing");
        let path = scratch.path.join("f.txt");
        std::fs::write(&path, b"hello").unwrap();
        assert!(os_file_is_readable(&path));
    }

    #[test]
    fn os_file_is_readable_false_for_a_nonexistent_path() {
        let scratch = TempScratch::new("readable_missing");
        let path = scratch.path.join("does_not_exist.txt");
        assert!(!os_file_is_readable(&path));
    }

    #[test]
    fn os_file_is_readable_true_for_a_directory() {
        let scratch = TempScratch::new("readable_dir");
        assert!(os_file_is_readable(&scratch.path));
    }

    #[test]
    fn os_file_is_writable_returns_1_for_a_writable_file() {
        let scratch = TempScratch::new("writable_file");
        let path = scratch.path.join("f.txt");
        std::fs::write(&path, b"hello").unwrap();
        assert_eq!(os_file_is_writable(&path), 1);
    }

    #[test]
    fn os_file_is_writable_returns_2_for_a_writable_directory() {
        let scratch = TempScratch::new("writable_dir");
        assert_eq!(os_file_is_writable(&scratch.path), 2);
    }

    #[test]
    fn os_file_is_writable_returns_0_for_a_nonexistent_path() {
        let scratch = TempScratch::new("writable_missing");
        let path = scratch.path.join("does_not_exist.txt");
        assert_eq!(os_file_is_writable(&path), 0);
    }

    #[cfg(windows)]
    #[test]
    // set_readonly(false) is flagged by clippy because on Unix it'd
    // make the file world-writable - but this test is cfg(windows)
    // only, where set_readonly toggles just the DOS read-only
    // attribute (the exact thing this test is exercising).
    #[allow(clippy::permissions_set_readonly_false)]
    fn os_file_is_writable_returns_0_for_a_readonly_file() {
        let scratch = TempScratch::new("writable_readonly");
        let path = scratch.path.join("ro.txt");
        std::fs::write(&path, b"hello").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms).unwrap();

        assert_eq!(os_file_is_writable(&path), 0);
        assert!(os_file_is_readable(&path));

        // Restore write access so TempScratch's own Drop-based cleanup
        // (remove_dir_all) can actually delete this file afterward.
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(&path, perms).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn os_file_is_writable_returns_0_for_a_readonly_file() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = TempScratch::new("writable_readonly_unix");
        let path = scratch.path.join("ro.txt");
        std::fs::write(&path, b"hello").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();

        assert_eq!(os_file_is_writable(&path), 0);
        assert!(os_file_is_readable(&path));

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }

    #[test]
    fn os_fileinfo_returns_none_for_a_missing_path() {
        let scratch = TempScratch::new("fileinfo_missing");
        assert!(os_fileinfo(&scratch.path.join("nope.txt")).is_none());
    }

    #[test]
    fn os_fileinfo_size_matches_the_written_content_length() {
        let scratch = TempScratch::new("fileinfo_size");
        let path = scratch.path.join("f.txt");
        std::fs::write(&path, b"hello world").unwrap();

        let info = os_fileinfo(&path).expect("file exists");
        assert_eq!(os_fileinfo_size(&info), 11);
    }

    #[test]
    fn os_fileinfo_mtime_is_a_recent_real_timestamp() {
        let scratch = TempScratch::new("fileinfo_mtime");
        let path = scratch.path.join("f.txt");
        std::fs::write(&path, b"hello").unwrap();

        let info = os_fileinfo(&path).expect("file exists");
        let mtime = os_fileinfo_mtime(&info);
        // Comfortably past 2020-01-01 (1577836800) - a loose sanity
        // bound, not a flaky exact-time check.
        assert!(mtime > 1_577_836_800);
    }

    #[test]
    fn os_fileinfo_type_str_identifies_a_regular_file() {
        let scratch = TempScratch::new("fileinfo_type_file");
        let path = scratch.path.join("f.txt");
        std::fs::write(&path, b"hello").unwrap();

        let info = os_fileinfo(&path).expect("file exists");
        assert_eq!(os_fileinfo_type_str(&info), "file");
    }

    #[test]
    fn os_fileinfo_type_str_identifies_a_directory() {
        let scratch = TempScratch::new("fileinfo_type_dir");
        let info = os_fileinfo(&scratch.path).expect("dir exists");
        assert_eq!(os_fileinfo_type_str(&info), "dir");
    }

    #[test]
    fn os_fileinfo_follows_symlinks_but_link_does_not() {
        let scratch = TempScratch::new("fileinfo_symlink");
        let target = scratch.path.join("target.txt");
        std::fs::write(&target, b"hello").unwrap();
        let link = scratch.path.join("link.txt");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        #[cfg(windows)]
        let symlink_created = std::os::windows::fs::symlink_file(&target, &link).is_ok();
        #[cfg(not(windows))]
        let symlink_created = true;

        // Creating a symlink on Windows needs a developer-mode/
        // elevation privilege this local test run might not have -
        // skip gracefully rather than fail on an unrelated
        // permissions gap unrelated to the actual code under test.
        if !symlink_created {
            return;
        }

        let link_info = os_fileinfo_link(&link).expect("link exists");
        assert_eq!(os_fileinfo_type_str(&link_info), "link");

        let followed_info = os_fileinfo(&link).expect("target exists via the symlink");
        assert_eq!(os_fileinfo_type_str(&followed_info), "file");
        assert_eq!(os_fileinfo_size(&followed_info), 5);
    }
}
