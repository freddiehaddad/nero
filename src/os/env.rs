//! Translated from `src/nvim/os/env.c` (tractable core only).
//!
//! Translated: `env_init`, `os_getenv` (like `getenv()` but returns
//! `None` for an empty value), `os_env_exists`, `os_setenv`,
//! `os_unsetenv`, `os_get_pid`. In the original these wrap
//! `uv_os_getenv`/`uv_os_setenv`/`uv_os_unsetenv`/`getpid` (libuv or the
//! C standard library) purely for portability; Rust's own `std::env`/
//! `std::process` already provide the same portable primitives
//! natively (same reasoning as `os/time.rs`), so they're translated now
//! rather than waiting on the still-open libuv FFI-vs-Rust-runtime
//! decision (phase 11). Also `os_homedir`/`init_homedir` (+ its own
//! `os_uv_homedir` file-static, only partially implemented - see its
//! own doc comment).
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `os_getenv_buf`/`os_getenv_noalloc`: write into `NameBuff`
//!   (`crate::globals::GLOBALS`) - tractable in principle, deferred only
//!   because nothing calls them yet without a fixed-size-buffer-filling
//!   caller to validate against.
//! - `os_free_fullenv`/`os_getenvname_at_index`: need libuv's
//!   `uv_os_environ`/raw platform `environ`/`GetEnvironmentStringsW`
//!   enumeration API, not just a single-variable get/set.
//! - `os_hint_priority`/`os_get_hostname`: platform process/host APIs
//!   not yet decided (would need raw libc/Win32 FFI).
//! - `free_homedir`: `#ifdef EXITFREE`-only (debug/leak-detection
//!   build flag with no equivalent concept in this crate, same
//!   reasoning as other `EXITFREE`-gated functions elsewhere); also
//!   moot here since `HOMEDIR`'s `Option<Vec<u8>>` already drops its
//!   contents automatically, with no separate "free" step needed.
//! - `expand_env*`/`vim_getenv`/`home_replace*`: need `path.c`'s
//!   directory/file-name manipulation functions and writable access to
//!   `option_vars.h`'s options from here.
//! - `vim_runtime_dir`/`remove_tail`: only called by `vim_getenv`,
//!   deferred with it.
//! - `vim_env_iter`/`vim_env_iter_rev`: only consumed by
//!   `set_runtimepath_default`/similar (not yet translated).
//! - `get_env_name`: needs `expand_T` (cmdline completion, phase 7).
//! - `os_setenv_append_path`/`os_shell_is_cmdexe`: need `'shell'`
//!   parsing logic not yet translated, and `os/fs.c` real I/O.
//! - `vim_unsetenv_ext`/`vim_setenv_ext`/`restore_env_var`: thin
//!   wrappers intentionally deferred alongside their only caller
//!   (`os/lang.c`, not yet translated).

use super::os::NVIM_TESTING;
use crate::globals::GlobalCell;

/// Sets initial values for various environment-derived variables
/// (`env_init`).
pub fn env_init() {
    unsafe { *NVIM_TESTING.get_mut() = os_env_exists(b"NVIM_TEST", false) };
}

/// Like `getenv()`, but returns `None` if the variable is empty
/// (`os_getenv`).
///
/// Result must be freed by the caller (N/A in Rust - ownership is
/// simply returned).
///
/// @see os_env_exists
/// @see os_getenv_noalloc
#[must_use]
pub fn os_getenv(name: &[u8]) -> Option<Vec<u8>> {
    if name.is_empty() {
        return None;
    }
    let name = std::str::from_utf8(name).ok()?;
    match std::env::var_os(name) {
        Some(v) if !v.is_empty() => Some(v.to_string_lossy().into_owned().into_bytes()),
        _ => None,
    }
}

/// Returns true if environment variable `name` is defined (even if
/// empty). Returns false if not found or other failure (`os_env_exists`).
///
/// @param nonempty Require a non-empty value. Treat empty as "does not
///                 exist".
#[must_use]
pub fn os_env_exists(name: &[u8], nonempty: bool) -> bool {
    if name.is_empty() {
        return false;
    }
    let Ok(name) = std::str::from_utf8(name) else {
        return false;
    };
    match std::env::var_os(name) {
        Some(v) => !nonempty || !v.is_empty(),
        None => false,
    }
}

/// Sets an environment variable (`os_setenv`).
///
/// Windows (Vim-compat): Empty string (`:let $FOO=""`) undefines the
/// env var.
///
/// # Safety
/// Same requirement as `std::env::set_var`/`std::env::remove_var`: not
/// sound to call while other threads are concurrently reading/writing
/// the process environment (matches the original's own implicit
/// single-threaded-access assumption, which this crate preserves
/// throughout rather than adding new synchronization not present in the
/// original).
pub unsafe fn os_setenv(name: &[u8], value: &[u8], overwrite: i32) -> i32 {
    if name.is_empty() {
        return -1;
    }
    let Ok(name_str) = std::str::from_utf8(name) else {
        return -1;
    };

    if cfg!(windows) {
        if overwrite == 0 && !os_env_exists(name, true) {
            return 0;
        }
        if value.is_empty() {
            // Windows (Vim-compat): Empty string undefines the env var.
            return unsafe { os_unsetenv(name) };
        }
    } else if overwrite == 0 && os_env_exists(name, false) {
        return 0;
    }

    let Ok(value_str) = std::str::from_utf8(value) else {
        return -1;
    };
    // SAFETY: forwarded from this function's own safety contract.
    unsafe { std::env::set_var(name_str, value_str) };
    0
}

/// Unset environment variable (`os_unsetenv`).
///
/// # Safety
/// Same requirement as `std::env::remove_var` - see [`os_setenv`].
pub unsafe fn os_unsetenv(name: &[u8]) -> i32 {
    if name.is_empty() {
        return -1;
    }
    let Ok(name_str) = std::str::from_utf8(name) else {
        return -1;
    };
    // SAFETY: forwarded from this function's own safety contract.
    unsafe { std::env::remove_var(name_str) };
    0
}

/// Get the process ID of the Nvim process (`os_get_pid`).
#[must_use]
pub fn os_get_pid() -> i64 {
    std::process::id() as i64
}

/// The "real", resolved user home directory, set by [`init_homedir`]
/// (`homedir`, a file-static in the original - not an `EXTERN` global,
/// same treatment as `buffer.c`'s own file-statics like
/// `crate::buffer`'s `TOP_FILE_NUM`).
static HOMEDIR: std::sync::LazyLock<GlobalCell<Option<Vec<u8>>>> =
    std::sync::LazyLock::new(|| GlobalCell::new(None));

/// Gets the "real", resolved user home directory as determined by
/// [`init_homedir`] (`os_homedir`).
///
/// The original `emsg`s and returns `NULL` if `init_homedir` hasn't
/// been called yet or failed; `emsg()` itself isn't translated yet
/// (`message.c`), so this just returns `None` in that case.
///
/// # Safety
/// Touches a `GlobalCell` - same requirement as every other function
/// that does so: no overlapping live access.
#[must_use]
pub unsafe fn os_homedir() -> Option<Vec<u8>> {
    unsafe { HOMEDIR.get_mut() }.clone()
}

/// Queries the OS for the current user's home directory
/// (`os_uv_homedir`, `static` in the original).
///
/// **Not yet implemented** (always returns `None`): the original wraps
/// `uv_os_homedir()`, which on Windows calls `GetUserProfileDirectoryW`
/// (a Win32 API needing an FFI decision not yet made) and on Unix reads
/// `getpwuid()` (needs the same `libc`-FFI decision noted in
/// `os/users.c`'s investigation). [`init_homedir`] still produces a
/// correct result in the overwhelmingly common case where `$HOME` (or,
/// on Windows, `$HOMEDRIVE`+`$HOMEPATH`) is set, which covers virtually
/// every real login session - this is only a fallback for the rarer
/// case where none of those are set.
fn os_uv_homedir() -> Option<Vec<u8>> {
    None
}

/// Sets the resolved user home directory (`HOMEDIR`, read via
/// [`os_homedir`]), as follows (`init_homedir`):
///  1. get value of `$HOME`
///  2. if `$HOME` is not set, try the following:
///
/// For Windows:
///  1. assemble homedir using `$HOMEDRIVE` and `$HOMEPATH`
///  2. try `os_uv_homedir` (not yet implemented, see its own doc)
///  3. resolve a direct reference to another system variable
///  4. guess `C:/`
///
/// For Unix:
///  1. try `os_uv_homedir` (not yet implemented, see its own doc)
///  2. resolve it with `os_realpath` (this also works with mounts and
///     links)
///  3. fall back to the current working directory as a last resort
///
/// # Safety
/// Touches a `GlobalCell` - same requirement as every other function
/// that does so: no overlapping live access.
pub unsafe fn init_homedir() {
    // In case we are called a second time.
    unsafe { *HOMEDIR.get_mut() = None };

    let mut var: Option<Vec<u8>> = os_getenv(b"HOME");

    if cfg!(windows) {
        if var.is_none() {
            // Typically, $HOME is not defined on Windows, unless the
            // user has specifically defined it. However, $HOMEDRIVE
            // and $HOMEPATH are automatically defined for each user on
            // Windows NT platforms. Try constructing $HOME from these.
            let homedrive = os_getenv(b"HOMEDRIVE");
            let homepath = os_getenv(b"HOMEPATH")
                .unwrap_or_else(|| crate::ascii_defs::PATHSEPSTR.as_bytes().to_vec());
            if let Some(homedrive) = homedrive {
                let mut combined = homedrive;
                combined.extend_from_slice(&homepath);
                if !combined.is_empty() {
                    var = Some(combined);
                }
            }
        }
        if var.is_none() {
            var = os_uv_homedir();
        }

        // Weird but true: $HOME may contain an indirect reference to
        // another variable, esp. "%USERPROFILE%". Happens when
        // $USERPROFILE isn't set when $HOME is being set.
        //
        // Extract an owned (name, suffix) pair first so the borrow of
        // `var` ends before `var` is reassigned below.
        let indirect_ref: Option<(Vec<u8>, Vec<u8>)> = var.as_ref().and_then(|v| {
            if v.first() != Some(&b'%') {
                return None;
            }
            let rel_pos = v[1..].iter().position(|&b| b == b'%')?;
            let name = v[1..1 + rel_pos].to_vec();
            let suffix = v[1 + rel_pos..].to_vec(); // starts with the closing '%'
            Some((name, suffix))
        });
        if let Some((name, suffix)) = indirect_ref {
            var = None;
            if let Some(exp) = os_getenv(&name) {
                if !exp.is_empty() {
                    let mut combined = exp;
                    combined.extend_from_slice(&suffix[1..]);
                    var = Some(combined);
                }
            }
        }

        // Default home dir is C:/. Best assumption we can make in such
        // a situation.
        if var.as_ref().is_none_or(|v| v.is_empty()) {
            var = Some(b"C:/".to_vec());
        }
    } else {
        if var.is_none() {
            var = os_uv_homedir();
        }

        // Get the actual path. This resolves links.
        if let Some(v) = &var {
            if let Ok(path_str) = std::str::from_utf8(v) {
                if let Some(real) = crate::os::fs::os_realpath(std::path::Path::new(path_str)) {
                    var = Some(real);
                }
            }
        }

        // Fall back to current working directory if home is not found.
        if var.as_ref().is_none_or(|v| v.is_empty()) {
            var = crate::os::fs::os_dirname();
        }
    }

    if let Some(mut result) = var {
        crate::path::path_to_slash(&mut result);
        unsafe { *HOMEDIR.get_mut() = Some(result) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Environment variables are process-global state shared by all
    // threads; Rust's default test runner uses multiple threads, so
    // every test here uses a unique variable name to avoid colliding
    // with any other concurrently-running test in this crate.

    /// Serializes tests that mutate `$HOME`/`$HOMEDRIVE`/`$HOMEPATH`
    /// (real, well-known environment variable names that can't be
    /// namespaced per-test the way arbitrary `NERO_TEST_ENV_*` names
    /// can), since Rust's multi-threaded test runner would otherwise
    /// let these tests race against each other.
    static HOMEDIR_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RAII guard restoring a set of environment variables to their
    /// original values on drop (including on test panic via
    /// unwinding).
    struct EnvVarGuard {
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl EnvVarGuard {
        fn set(vars: &[(&'static str, Option<&str>)]) -> Self {
            let saved = vars
                .iter()
                .map(|(name, _)| (*name, std::env::var_os(name)))
                .collect();
            for (name, value) in vars {
                // SAFETY: serialized via HOMEDIR_TEST_LOCK, held by every
                // caller of this helper.
                unsafe {
                    match value {
                        Some(v) => std::env::set_var(name, v),
                        None => std::env::remove_var(name),
                    }
                }
            }
            EnvVarGuard { saved }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            for (name, value) in &self.saved {
                // SAFETY: serialized via HOMEDIR_TEST_LOCK, held by every
                // caller of `EnvVarGuard::set`.
                unsafe {
                    match value {
                        Some(v) => std::env::set_var(name, v),
                        None => std::env::remove_var(name),
                    }
                }
            }
        }
    }

    #[test]
    fn init_homedir_uses_home_when_set() {
        let _lock = HOMEDIR_TEST_LOCK.lock().unwrap();
        let _guard = EnvVarGuard::set(&[("HOME", Some("C:/some/home"))]);
        unsafe {
            init_homedir();
            assert_eq!(os_homedir(), Some(b"C:/some/home".to_vec()));
        }
    }

    #[test]
    #[cfg(windows)]
    fn init_homedir_falls_back_to_homedrive_homepath() {
        let _lock = HOMEDIR_TEST_LOCK.lock().unwrap();
        let _guard = EnvVarGuard::set(&[
            ("HOME", None),
            ("HOMEDRIVE", Some("C:")),
            ("HOMEPATH", Some("\\Users\\test")),
        ]);
        unsafe {
            init_homedir();
            assert_eq!(os_homedir(), Some(b"C:/Users/test".to_vec()));
        }
    }

    #[test]
    #[cfg(windows)]
    fn init_homedir_falls_back_to_c_drive_when_nothing_set() {
        let _lock = HOMEDIR_TEST_LOCK.lock().unwrap();
        let _guard = EnvVarGuard::set(&[("HOME", None), ("HOMEDRIVE", None), ("HOMEPATH", None)]);
        unsafe {
            init_homedir();
            assert_eq!(os_homedir(), Some(b"C:/".to_vec()));
        }
    }

    #[test]
    fn init_homedir_resolves_percent_indirect_reference() {
        let _lock = HOMEDIR_TEST_LOCK.lock().unwrap();
        let _guard = EnvVarGuard::set(&[
            ("HOME", Some("%NERO_TEST_INDIRECT_VAR%")),
            ("NERO_TEST_INDIRECT_VAR", Some("C:/indirect/target")),
        ]);
        unsafe {
            init_homedir();
            assert_eq!(os_homedir(), Some(b"C:/indirect/target".to_vec()));
        }
    }

    #[test]
    fn init_homedir_on_real_ambient_environment_yields_absolute_path() {
        // No overrides - exercises the function against this machine's
        // real environment, just checking the general shape of the
        // result rather than an exact value (which depends on who's
        // running the tests).
        let _lock = HOMEDIR_TEST_LOCK.lock().unwrap();
        unsafe {
            init_homedir();
            let home = os_homedir().expect("init_homedir should always set something");
            assert!(!home.is_empty());
            assert!(!home.contains(&b'\\'));
        }
    }

    #[test]
    fn os_getenv_returns_none_for_unset_var() {
        assert_eq!(os_getenv(b"NERO_TEST_ENV_UNSET_VAR"), None);
    }

    #[test]
    fn os_getenv_returns_none_for_empty_name() {
        assert_eq!(os_getenv(b""), None);
    }

    #[test]
    fn setenv_getenv_unsetenv_roundtrip() {
        let name = b"NERO_TEST_ENV_ROUNDTRIP";
        // SAFETY: single test-owned variable name, not touched by other tests.
        unsafe {
            assert_eq!(os_setenv(name, b"hello", 1), 0);
            assert_eq!(os_getenv(name), Some(b"hello".to_vec()));
            assert!(os_env_exists(name, true));

            assert_eq!(os_unsetenv(name), 0);
            assert_eq!(os_getenv(name), None);
            assert!(!os_env_exists(name, false));
        }
    }

    #[test]
    #[cfg(windows)]
    fn setenv_overwrite_zero_on_windows_only_skips_if_unset() {
        // Faithful to the real upstream Windows-specific `os_setenv`
        // quirk (src/nvim/os/env.c's `#ifdef MSWIN` branch): on Windows,
        // `overwrite == 0` skips the assignment only when the variable
        // does NOT already exist; if it DOES exist, `overwrite == 0`
        // still updates it. This is the *opposite* of POSIX `setenv()`
        // semantics (which skip *existing* vars when overwrite == 0) -
        // preserved here exactly as-is rather than "fixed" to match
        // POSIX, since this is a literal translation.
        let name = b"NERO_TEST_ENV_NO_OVERWRITE_WIN";
        // SAFETY: single test-owned variable name, not touched by other tests.
        unsafe {
            // Var doesn't exist yet: overwrite=0 is a no-op.
            assert_eq!(os_setenv(name, b"first", 0), 0);
            assert_eq!(os_getenv(name), None);

            assert_eq!(os_setenv(name, b"first", 1), 0);
            assert_eq!(os_getenv(name), Some(b"first".to_vec()));

            // Now it exists: overwrite=0 still updates it (the quirk).
            assert_eq!(os_setenv(name, b"second", 0), 0);
            assert_eq!(os_getenv(name), Some(b"second".to_vec()));

            os_unsetenv(name);
        }
    }

    #[test]
    #[cfg(not(windows))]
    fn setenv_overwrite_zero_on_posix_keeps_existing_value() {
        // POSIX setenv() semantics: overwrite == 0 skips the assignment
        // when the variable already exists (src/nvim/os/env.c's `#else`
        // branch). This test doesn't run on this Windows machine, but
        // documents and would verify the other platform's behavior.
        let name = b"NERO_TEST_ENV_NO_OVERWRITE_POSIX";
        // SAFETY: single test-owned variable name, not touched by other tests.
        unsafe {
            assert_eq!(os_setenv(name, b"first", 1), 0);
            assert_eq!(os_setenv(name, b"second", 0), 0);
            assert_eq!(os_getenv(name), Some(b"first".to_vec()));
            os_unsetenv(name);
        }
    }

    #[test]
    fn empty_value_is_treated_as_unset_by_os_getenv() {
        let name = b"NERO_TEST_ENV_EMPTY_VALUE";
        // SAFETY: single test-owned variable name, not touched by other tests.
        unsafe {
            assert_eq!(os_setenv(name, b"", 1), 0);
            assert_eq!(os_getenv(name), None);
            os_unsetenv(name);
        }
    }

    #[test]
    fn os_get_pid_matches_std_process_id() {
        assert_eq!(os_get_pid(), std::process::id() as i64);
    }

    #[test]
    fn env_init_sets_nvim_testing_from_env_var() {
        // Doesn't assert a specific value (depends on the real test
        // runner's environment), just that it runs without panicking
        // and produces a bool consistent with os_env_exists.
        env_init();
        let expected = os_env_exists(b"NVIM_TEST", false);
        assert_eq!(unsafe { *NVIM_TESTING.get_mut() }, expected);
    }
}
