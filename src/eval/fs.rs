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
//! existing [`crate::path::path_is_absolute`]), and `delete()` for its
//! two tractable flag values (`""` - delete a file, via
//! [`crate::os::fs::os_remove`]; `"d"` - delete an empty directory,
//! via [`crate::os::fs::os_rmdir`]). The `"rf"` (recursive delete)
//! flag needs `delete_recursive` (a directory-tree walk, not yet
//! translated) and panics via `unimplemented!()` if actually reached.
//! `delete()`/`isdirectory()`/`isabsolutepath()` all need a byte
//! string to be valid UTF-8 to build a `Path` from (this crate's
//! established `path_full_dir_name`-style convention - see that
//! function's own body in `path.rs`), gracefully treating invalid
//! UTF-8 the same as a nonexistent path/name rather than panicking.

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
}
