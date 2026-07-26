//! Translated from `src/nvim/eval/fs.c` (tractable core only).
//!
//! `eval/fs.c` (~1900 lines) implements filesystem-related Vimscript
//! builtins: `delete()`, `getcwd()`, `isdirectory()`, `mkdir()`,
//! `rename()`, `glob()`/`globpath()`, `readfile()`/`writefile()`, and
//! more. Most need substantial not-yet-translated machinery
//! (`vim_rename`'s cross-device-safe rename, `delete_recursive`'s
//! directory-tree walk, `find_file_in_path_option`'s `'path'`-option
//! search, per-window/tab local-cwd tracking for `getcwd()`) - this
//! file starts with the smallest, most self-contained subset.
//!
//! Translated: `isdirectory()` (via the already-existing
//! [`crate::os::fs::os_isdir`]), `isabsolutepath()` (via the already-
//! existing [`crate::path::path_is_absolute`]), `delete()` for its
//! two tractable flag values (`""` - delete a file, via
//! [`crate::os::fs::os_remove`]; `"d"` - delete an empty directory,
//! via [`crate::os::fs::os_rmdir`]), `filereadable()`/
//! `filewritable()` (via the already-existing
//! [`crate::os::fs::os_file_is_readable`]/
//! [`crate::os::fs::os_file_is_writable`]), and `getfsize()`/
//! `getftime()`/`getftype()` (via the already-existing
//! [`crate::os::fs::os_fileinfo`]/[`crate::os::fs::os_fileinfo_link`]
//! narrow-subset `FileInfo` - see that module's own doc comment for
//! what's NOT modeled; `getfperm()`, needing the still-deferred
//! `os_getperm` permission bits, is not translated here). The `"rf"`
//! (recursive delete) flag needs `delete_recursive` (a directory-tree
//! walk, not yet translated) and panics via `unimplemented!()` if
//! actually reached. Every function taking a path/name needs the byte
//! string to be valid UTF-8 to build a `Path` from (this crate's
//! established `path_full_dir_name`-style convention - see that
//! function's own body in `path.rs`), gracefully treating invalid UTF-8
//! the same as
//! a nonexistent path/name rather than panicking.

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
}
