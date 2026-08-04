//! Translated from `src/nvim/os/stdpaths.c` (tractable core only).
//!
//! Translated: `get_appname`, `appname_is_valid`; `XDGVarType`/
//! `xdg_env_vars`/`xdg_defaults`/`xdg_defaults_env_vars`,
//! `stdpaths_get_xdg_var`/`get_xdg_home` (now tractable now that
//! `os/env.rs`'s `expand_env_save` is real).
//!
//! `get_appname`'s simplification: the original also writes its result
//! into the shared `NameBuff` scratch buffer (`crate::globals::GLOBALS`)
//! and returns a pointer into it; this translation instead returns a
//! fresh owned `Vec<u8>`, since nothing translated so far relies on
//! `NameBuff` being updated as a side effect of calling `get_appname` -
//! consistent with this crate's general preference for owned values
//! over C's "return a pointer to a reused static buffer" idiom wherever
//! no currently-translated caller actually depends on the sharing.
//!
//! `get_xdg_home`'s own `IObuff`-scratch-buffer use (to build
//! `"$NVIM_APPNAME[-data]"` before joining) is likewise replaced with
//! a plain local `Vec<u8>`, matching the same established preference.
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `xdg_remove_duplicate` (deduplicates `:`/`;`-separated dir lists,
//!   needed only by the `config_dirs`/`data_dirs` variants of
//!   `stdpath()`) and the `kXDGRuntimeDir` branch's `vim_gettempdir`
//!   (a persistent session-lifetime temp-directory subsystem) -
//!   [`stdpaths_get_xdg_var`] only models the `env_val`-set and
//!   `fallback`-string branches for now, `unimplemented!()`-ing the
//!   other two.
//! - `stdpaths_user_cache_subpath`/`stdpaths_user_conf_subpath`/
//!   `stdpaths_user_data_subpath`/`stdpaths_user_state_subpath`: thin
//!   wrappers over [`get_xdg_home`] + `concat_fnames_realloc` - no real
//!   caller among currently-translated code yet, not translated ahead
//!   of one.

use crate::memory::memchrsub;
use crate::os::env::os_getenv;
use crate::path::{path_is_absolute, path_to_slash};

/// Which XDG base-directory variable is meant (`XDGVarType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XdgVarType {
    ConfigHome,
    DataHome,
    CacheHome,
    StateHome,
    RuntimeDir,
    ConfigDirs,
    DataDirs,
}

/// The real environment-variable name for each [`XdgVarType`]
/// (`xdg_env_vars[]`).
#[must_use]
fn xdg_env_var(idx: XdgVarType) -> &'static [u8] {
    match idx {
        XdgVarType::ConfigHome => b"XDG_CONFIG_HOME",
        XdgVarType::DataHome => b"XDG_DATA_HOME",
        XdgVarType::CacheHome => b"XDG_CACHE_HOME",
        XdgVarType::StateHome => b"XDG_STATE_HOME",
        XdgVarType::RuntimeDir => b"XDG_RUNTIME_DIR",
        XdgVarType::ConfigDirs => b"XDG_CONFIG_DIRS",
        XdgVarType::DataDirs => b"XDG_DATA_DIRS",
    }
}

/// The fallback default (needing `~`/`$VAR` expansion) for each
/// [`XdgVarType`], platform-conditional exactly as the original's own
/// `#ifdef MSWIN` (`xdg_defaults[]`). `None` for `RuntimeDir` (decided
/// by the not-yet-translated `vim_mktempdir()` instead) and, on
/// Windows, for `ConfigDirs`/`DataDirs` too (no multi-directory
/// default there).
#[must_use]
fn xdg_default(idx: XdgVarType) -> Option<&'static [u8]> {
    if cfg!(windows) {
        match idx {
            XdgVarType::ConfigHome | XdgVarType::DataHome | XdgVarType::StateHome => Some(b"~/AppData/Local"),
            XdgVarType::CacheHome => Some(b"~/AppData/Local/Temp"),
            XdgVarType::RuntimeDir | XdgVarType::ConfigDirs | XdgVarType::DataDirs => None,
        }
    } else {
        match idx {
            XdgVarType::ConfigHome => Some(b"~/.config"),
            XdgVarType::DataHome => Some(b"~/.local/share"),
            XdgVarType::CacheHome => Some(b"~/.cache"),
            XdgVarType::StateHome => Some(b"~/.local/state"),
            XdgVarType::RuntimeDir => None,
            XdgVarType::ConfigDirs => Some(b"/etc/xdg/"),
            XdgVarType::DataDirs => Some(b"/usr/local/share/:/usr/share/"),
        }
    }
}

/// Windows-only fallback environment variable name for each
/// [`XdgVarType`] (`xdg_defaults_env_vars[]`), consulted before the
/// plain string default above when the real XDG variable isn't set.
#[cfg(windows)]
#[must_use]
fn xdg_default_env_var(idx: XdgVarType) -> Option<&'static [u8]> {
    match idx {
        XdgVarType::ConfigHome | XdgVarType::DataHome | XdgVarType::StateHome => Some(b"LOCALAPPDATA"),
        XdgVarType::CacheHome => Some(b"TEMP"),
        XdgVarType::RuntimeDir | XdgVarType::ConfigDirs | XdgVarType::DataDirs => None,
    }
}

/// Gets the value of an XDG base-directory variable, or its
/// (`~`/`$VAR`-expanded) fallback default (`stdpaths_get_xdg_var`).
///
/// Only the two most common branches are modeled for real: the
/// variable's own real environment value (with Windows' extra
/// `os_realpath` canonicalization for `kXDGCacheHome`), and the
/// plain-string-fallback path via [`crate::os::env::expand_env_save`].
/// `unimplemented!()`s for [`XdgVarType::RuntimeDir`] (needs
/// `vim_mktempdir`, a persistent session-lifetime temp-directory
/// subsystem) when neither the real variable nor (on Windows) its own
/// fallback environment variable is set - not reached by any of
/// `stdpath()`'s other, more common arguments.
///
/// # Safety
/// Forwarded from [`crate::os::env::expand_env_save`]'s own safety doc.
#[must_use]
pub unsafe fn stdpaths_get_xdg_var(idx: XdgVarType) -> Option<Vec<u8>> {
    let env = xdg_env_var(idx);
    let mut env_val = os_getenv(env);

    if cfg!(windows) {
        #[cfg(windows)]
        if env_val.is_none()
            && let Some(fallback_env) = xdg_default_env_var(idx)
        {
            env_val = os_getenv(fallback_env);
        }
        #[cfg(windows)]
        if idx == XdgVarType::CacheHome
            && let Some(v) = &env_val
            && let Ok(path_str) = std::str::from_utf8(v)
            && let Some(real_path) = crate::os::fs::os_realpath(std::path::Path::new(path_str))
        {
            env_val = Some(real_path);
        }
    } else if env_val.is_none() && crate::os::env::os_env_exists(env, false) {
        // Set but empty ("FOO=" with no value): matches the
        // non-Windows original's own `xstrdup("")` fallback exactly.
        env_val = Some(Vec::new());
    }

    if let Some(v) = &mut env_val {
        path_to_slash(v);
    }

    let ret = if let Some(v) = env_val {
        Some(v)
    } else if let Some(fallback) = xdg_default(idx) {
        // SAFETY: forwarded from this function's own safety doc.
        Some(unsafe { crate::os::env::expand_env_save(fallback) })
    } else if idx == XdgVarType::RuntimeDir {
        unimplemented!(
            "stdpaths_get_xdg_var: the RuntimeDir fallback needs vim_mktempdir, not yet translated"
        );
    } else {
        None
    };

    if matches!(idx, XdgVarType::DataDirs | XdgVarType::ConfigDirs) && ret.is_some() {
        unimplemented!("stdpaths_get_xdg_var: xdg_remove_duplicate not yet translated");
    }

    ret
}

/// Concatenate `fname1`/`fname2`, adding a path separator between them
/// unless `fname1` already ends in one (or is empty) - the tractable
/// subset of `concat_fnames_realloc` this crate's own owned-`Vec<u8>`
/// idiom actually needs (no `xrealloc`/manual-free dance).
pub(crate) fn concat_fnames(mut fname1: Vec<u8>, fname2: &[u8]) -> Vec<u8> {
    if !fname1.is_empty() && !crate::path::after_pathsep(&fname1, fname1.len()) {
        fname1.push(crate::ascii_defs::PATHSEP);
    }
    fname1.extend_from_slice(fname2);
    fname1
}

/// Return the Nvim-specific XDG directory subpath: `"{xdg_dir}/
/// $NVIM_APPNAME[-data]"` (`get_xdg_home`).
///
/// # Safety
/// Forwarded from [`stdpaths_get_xdg_var`]'s own safety doc.
#[must_use]
pub unsafe fn get_xdg_home(idx: XdgVarType) -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    let dir = unsafe { stdpaths_get_xdg_var(idx) }?;
    let mut appname = get_appname(false);
    // Windows: avoid storing configuration and data files in the same
    // path (matches the original's own `#ifdef MSWIN` exactly).
    if cfg!(windows) && matches!(idx, XdgVarType::DataHome | XdgVarType::StateHome) {
        appname.extend_from_slice(b"-data");
    }
    Some(concat_fnames(dir, &appname))
}

/// Gets the value of `$NVIM_APPNAME`, or `"nvim"` if not set
/// (`get_appname`).
///
/// @param namelike Return a "name-like" value (no path separators).
///
/// @return `$NVIM_APPNAME` value, forward-slash-normalized.
#[must_use]
pub fn get_appname(namelike: bool) -> Vec<u8> {
    let mut name = os_getenv(b"NVIM_APPNAME").unwrap_or_else(|| b"nvim".to_vec());

    path_to_slash(&mut name);

    if namelike {
        // Appname may be a relative path, replace slashes to make it name-like.
        memchrsub(&mut name, b'/', b'-');
        memchrsub(&mut name, b'\\', b'-');
    }

    name
}

/// Ensure that `$NVIM_APPNAME` is valid. Must be a name or relative path
/// (`appname_is_valid`).
#[must_use]
pub fn appname_is_valid() -> bool {
    let appname = get_appname(false);
    // TODO(justinmk): on Windows, path_is_absolute says "/" is NOT
    // absolute. Should it? (matches the original's own TODO comment)
    !(path_is_absolute(&appname)
        || appname == b"/"
        || appname == b"\\"
        || appname == b"."
        || appname == b".."
        || contains_subslice(&appname, b"/..")
        || contains_subslice(&appname, b"../"))
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // NVIM_APPNAME is process-global state shared by all threads; Rust's
    // default test runner uses multiple threads, so no test here can
    // safely set/unset it without racing other concurrently-running
    // tests in this crate (including other files' tests). Instead, each
    // test only reads the *current* value, whatever it is, and checks
    // internal consistency of get_appname/appname_is_valid against it.

    #[test]
    fn get_appname_defaults_to_nvim_when_unset() {
        if crate::os::env::os_env_exists(b"NVIM_APPNAME", true) {
            return; // set by the ambient environment; skip.
        }
        assert_eq!(get_appname(false), b"nvim");
        assert_eq!(get_appname(true), b"nvim");
    }

    #[test]
    fn get_appname_namelike_replaces_slashes() {
        // Directly exercise the slash-replacement logic without
        // touching the real environment.
        let mut name = b"sub/dir\\name".to_vec();
        memchrsub(&mut name, b'/', b'-');
        memchrsub(&mut name, b'\\', b'-');
        assert_eq!(name, b"sub-dir-name");
    }

    #[test]
    fn appname_is_valid_rejects_dot_and_dotdot() {
        // appname_is_valid itself depends on the ambient environment's
        // NVIM_APPNAME, so exercise the pure helper directly instead:
        assert!(contains_subslice(b"foo/../bar", b"/.."));
        assert!(contains_subslice(b"../bar", b"../"));
        assert!(!contains_subslice(b"foobar", b"/.."));
    }

    #[test]
    fn appname_is_valid_is_consistent_with_get_appname() {
        // Whatever the ambient NVIM_APPNAME is (or its "nvim" default),
        // it must not contain the invalid patterns checked above.
        let valid = appname_is_valid();
        let appname = get_appname(false);
        let expected = !(path_is_absolute(&appname)
            || appname == b"/"
            || appname == b"\\"
            || appname == b"."
            || appname == b".."
            || contains_subslice(&appname, b"/..")
            || contains_subslice(&appname, b"../"));
        assert_eq!(valid, expected);
    }

    // --- stdpaths_get_xdg_var / get_xdg_home ---

    /// Serializes tests that mutate a real `$XDG_*` environment
    /// variable (can't be namespaced per-test the way arbitrary
    /// `NERO_TEST_*` names can), matching `os/env.rs`'s own
    /// `homedir_test_lock`/`EnvVarGuard` precedent for the same
    /// reason. `pub(crate)` since `eval::funcs`'s own `f_stdpath`
    /// tests read the SAME ambient `$XDG_*` state (via `get_xdg_home`)
    /// and must serialize against this exact lock too, not a separate
    /// one.
    static XDG_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    pub(crate) fn xdg_test_lock() -> std::sync::MutexGuard<'static, ()> {
        XDG_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    struct XdgEnvGuard {
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl XdgEnvGuard {
        fn set(vars: &[(&'static str, Option<&str>)]) -> Self {
            let saved = vars.iter().map(|(name, _)| (*name, std::env::var_os(name))).collect();
            for (name, value) in vars {
                // SAFETY: serialized via xdg_test_lock, held by every
                // caller of this helper.
                unsafe {
                    match value {
                        Some(v) => std::env::set_var(name, v),
                        None => std::env::remove_var(name),
                    }
                }
            }
            XdgEnvGuard { saved }
        }
    }

    impl Drop for XdgEnvGuard {
        fn drop(&mut self) {
            for (name, value) in &self.saved {
                // SAFETY: serialized via xdg_test_lock, held by every
                // caller of `XdgEnvGuard::set`.
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
    fn stdpaths_get_xdg_var_uses_the_real_env_var_when_set() {
        let _lock = xdg_test_lock();
        let _guard = XdgEnvGuard::set(&[("XDG_CONFIG_HOME", Some("/custom/config"))]);
        let result = unsafe { stdpaths_get_xdg_var(XdgVarType::ConfigHome) };
        assert_eq!(result, Some(b"/custom/config".to_vec()));
    }

    #[test]
    #[cfg(unix)]
    fn stdpaths_get_xdg_var_falls_back_to_the_expanded_default_on_unix() {
        let _homedir_lock = crate::os::env::tests::homedir_test_lock();
        let _lock = xdg_test_lock();
        let _guard = XdgEnvGuard::set(&[("XDG_CONFIG_HOME", None), ("HOME", Some("/home/alice"))]);
        unsafe { crate::os::env::init_homedir() };
        let result = unsafe { stdpaths_get_xdg_var(XdgVarType::ConfigHome) };
        assert_eq!(result, Some(b"/home/alice/.config".to_vec()));
    }

    #[test]
    #[cfg(unix)]
    fn get_xdg_home_appends_the_real_appname_on_unix() {
        let _homedir_lock = crate::os::env::tests::homedir_test_lock();
        let _lock = xdg_test_lock();
        let _guard = XdgEnvGuard::set(&[("XDG_CONFIG_HOME", Some("/custom/config"))]);
        let appname = get_appname(false);
        let result = unsafe { get_xdg_home(XdgVarType::ConfigHome) };
        let mut expected = b"/custom/config/".to_vec();
        expected.extend_from_slice(&appname);
        assert_eq!(result, Some(expected));
    }

    #[test]
    #[cfg(unix)]
    fn get_xdg_home_data_home_does_not_append_dash_data_on_unix() {
        // The "-data" suffix (avoiding storing config/data files in
        // the same path) is a Windows-only detail - matches the
        // original's own #ifdef MSWIN exactly.
        let _homedir_lock = crate::os::env::tests::homedir_test_lock();
        let _lock = xdg_test_lock();
        let _guard = XdgEnvGuard::set(&[("XDG_DATA_HOME", Some("/custom/data"))]);
        let appname = get_appname(false);
        let result = unsafe { get_xdg_home(XdgVarType::DataHome) };
        let mut expected = b"/custom/data/".to_vec();
        expected.extend_from_slice(&appname);
        assert_eq!(result, Some(expected));
    }

    #[test]
    #[cfg(unix)]
    fn stdpaths_get_xdg_var_config_dirs_is_unimplemented_when_using_the_default() {
        let _lock = xdg_test_lock();
        let _guard = XdgEnvGuard::set(&[("XDG_CONFIG_DIRS", None)]);
        let result = std::panic::catch_unwind(|| unsafe { stdpaths_get_xdg_var(XdgVarType::ConfigDirs) });
        assert!(result.is_err(), "expected a panic (xdg_remove_duplicate not yet translated)");
    }
}
