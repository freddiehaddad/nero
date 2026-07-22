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
//! `os_isrealdir`, `os_mkdir`, `os_rmdir`, `os_remove`, `os_rename`,
//! `os_realpath`.
//! Functions that in the original return a raw libuv error code
//! (`os_chdir`/`os_mkdir`/`os_rmdir`/`os_remove`) are translated to
//! return `0` on success and `-1` on any failure: this collapses
//! libuv's specific per-error-cause codes (`UV_ENOENT`, `UV_EACCES`,
//! etc.) into one generic failure value, since nothing consuming those
//! specific codes is translated yet - revisit if/when a caller needs to
//! distinguish failure causes.
//!
//! Deferred (each needs either the `FileInfo`-vs-`std::fs::Metadata`
//! representation decision, or real byte-level I/O, neither settled
//! yet):
//! - `os_getperm`/`os_setperm`/`os_nodetype`/`os_stat` (raw Unix-style
//!   mode bits - libuv synthesizes these even on Windows for
//!   compatibility; needs a real decision on how to model that
//!   cross-platform rather than rushing it).
//! - `os_open`/`os_fopen`/`os_close`/`os_read`/`os_readv`/`os_write`/
//!   `os_dup*`/`os_set_cloexec`/`os_copy`/`os_fsync`: real byte-level
//!   file I/O.
//! - `os_fileinfo*`/`os_fileid*`: need the `FileInfo` struct itself
//!   (deferred in `fs_defs.rs`, needs the same mode-bits decision).
//! - `os_exepath`/`os_can_exe`/`is_executable*`: executable-search
//!   logic tied to `'path'`-searching semantics (`path.c`) and exec-bit
//!   permission checks (`os_getperm`).
//! - `os_copy_xattr`/`os_get_acl`/`os_set_acl`/`os_free_acl`/
//!   `os_file_owned`/`os_chown`/`os_fchown`: platform ACL/xattr/
//!   ownership APIs, out of scope until a real FFI decision is made.
//! - `os_file_settime`/`os_file_is_readable`/`os_file_is_writable`:
//!   tractable in principle (`std::fs::metadata`/permissions), deferred
//!   only for lack of time this pass - revisit alongside `os_getperm`.
//! - `os_mkdir_recurse`/`os_file_mkdir`/`os_mkdtemp`: build on
//!   `os_mkdir` plus recursive-creation logic not ported this pass.
//! - `os_scandir`/`os_scandir_next`/`os_closedir`: need the `Directory`
//!   struct (deferred alongside `FileInfo`/`uv_dirent_t`).
//! - `os_resolve_shortcut`/`os_is_reparse_point_include`: Windows
//!   shortcut (`*.lnk`)/reparse-point resolution via COM
//!   (`IPersistFile`), a genuinely different, more complex API surface
//!   than plain symlink resolution - out of scope until a COM-FFI
//!   decision is made.

use crate::vim_defs::{FAIL, OK};
use std::path::Path;

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
    fn os_dirname_returns_current_dir_with_forward_slashes() {
        let dir = os_dirname().expect("current dir should be readable");
        assert!(!dir.is_empty());
        assert!(!dir.contains(&b'\\'));
    }

    #[test]
    fn os_chdir_changes_and_reports_failure_for_missing_dir() {
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
}
