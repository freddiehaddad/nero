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
/// Three branches are modeled: the variable's own real environment
/// value (with Windows' extra `os_realpath` canonicalization for
/// `kXDGCacheHome`), the plain-string-fallback path via
/// [`crate::os::env::expand_env_save`], and - for
/// [`XdgVarType::RuntimeDir`], which has no string fallback at all -
/// the tempdir that [`crate::fileio::vim_gettempdir`] creates at
/// startup, falling back to `"/tmp/"` and trimmed of its trailing
/// separator, exactly as upstream does.
///
/// # Safety
/// Forwarded from [`crate::os::env::expand_env_save`]'s and
/// [`crate::fileio::vim_gettempdir`]'s own safety docs.
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
        // Special-case: stdpath('run') is defined at startup.
        // SAFETY: forwarded from this function's own safety doc.
        let mut dir = unsafe { crate::fileio::vim_gettempdir() }.unwrap_or_else(|| b"/tmp/".to_vec());
        // Trim the trailing slash vim_gettempdir guarantees.
        dir.truncate(if dir.len() >= 2 { dir.len() - 1 } else { 0 });
        Some(dir)
    } else {
        None
    };

    if matches!(idx, XdgVarType::DataDirs | XdgVarType::ConfigDirs)
        && let Some(v) = ret
    {
        // SAFETY: forwarded from this function's own safety doc.
        return Some(unsafe { xdg_remove_duplicate(&v) });
    }

    ret
}

/// Drop repeated entries from a separator-joined XDG directory list,
/// keeping the FIRST occurrence of each and preserving order
/// (`xdg_remove_duplicate`).
///
/// Comparison uses [`crate::path::path_fnamecmp`], so it follows the
/// platform's own filename case/separator rules rather than a plain
/// byte compare.
///
/// The original splits in place with `os_strtok`, which also collapses
/// runs of separators and drops empty leading/trailing entries; the
/// `split` here reproduces that by skipping empty tokens.
///
/// # Safety
/// Forwarded from [`crate::path::path_fnamecmp`]'s own safety doc.
#[must_use]
pub unsafe fn xdg_remove_duplicate(ret: &[u8]) -> Vec<u8> {
    let sep = crate::os::os_defs::ENV_SEPCHAR as u8;
    let mut data: Vec<&[u8]> = Vec::new();
    for token in ret.split(|&b| b == sep) {
        if token.is_empty() {
            continue;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let is_duplicate = data
            .iter()
            .any(|prev| unsafe { crate::path::path_fnamecmp(prev, token) } == 0);
        if !is_duplicate {
            data.push(token);
        }
    }

    let mut result = Vec::new();
    for (i, token) in data.iter().enumerate() {
        if i != 0 {
            result.push(sep);
        }
        result.extend_from_slice(token);
    }
    result
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

    // --- xdg_remove_duplicate ---

    #[test]
    fn xdg_remove_duplicate_keeps_the_first_of_each_repeated_entry() {
        let sep = crate::os::os_defs::ENV_SEPCHAR;
        let input = format!("/a{sep}/b{sep}/a{sep}/c{sep}/b").into_bytes();
        let want = format!("/a{sep}/b{sep}/c").into_bytes();
        assert_eq!(unsafe { xdg_remove_duplicate(&input) }, want);
    }

    #[test]
    fn xdg_remove_duplicate_leaves_an_already_unique_list_alone() {
        let sep = crate::os::os_defs::ENV_SEPCHAR;
        let input = format!("/x{sep}/y{sep}/z").into_bytes();
        assert_eq!(unsafe { xdg_remove_duplicate(&input) }, input);
    }

    #[test]
    fn xdg_remove_duplicate_drops_empty_entries_like_os_strtok() {
        // os_strtok collapses runs of separators and ignores leading/
        // trailing ones, so no empty component survives.
        let sep = crate::os::os_defs::ENV_SEPCHAR;
        let input = format!("{sep}{sep}/a{sep}{sep}/b{sep}").into_bytes();
        let want = format!("/a{sep}/b").into_bytes();
        assert_eq!(unsafe { xdg_remove_duplicate(&input) }, want);
    }

    #[test]
    fn xdg_remove_duplicate_on_a_single_entry_is_the_identity() {
        assert_eq!(unsafe { xdg_remove_duplicate(b"/only") }, b"/only".to_vec());
    }

    #[test]
    fn xdg_remove_duplicate_of_an_empty_list_is_empty() {
        assert_eq!(unsafe { xdg_remove_duplicate(b"") }, Vec::<u8>::new());
    }

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
    fn stdpaths_get_xdg_var_runtime_dir_uses_the_real_env_var_when_set() {
        let _lock = xdg_test_lock();
        let _guard = XdgEnvGuard::set(&[("XDG_RUNTIME_DIR", Some("/run/user/1000"))]);
        let result = unsafe { stdpaths_get_xdg_var(XdgVarType::RuntimeDir) };
        assert_eq!(result, Some(b"/run/user/1000".to_vec()));
    }

    #[test]
    fn stdpaths_get_xdg_var_runtime_dir_falls_back_to_the_tempdir() {
        // RuntimeDir has no string fallback at all, so an unset
        // variable resolves through vim_gettempdir() instead - the one
        // branch that kept this function's own translation incomplete
        // until fileio.c's tempdir family landed.
        let _global = crate::globals::global_state_test_lock();
        let _lock = xdg_test_lock();
        let _guard = XdgEnvGuard::set(&[("XDG_RUNTIME_DIR", None)]);

        let result = unsafe { stdpaths_get_xdg_var(XdgVarType::RuntimeDir) }.expect("always Some");
        assert!(!result.is_empty());
        // Upstream trims the trailing separator vim_gettempdir
        // guarantees, so the result is a plain directory path.
        assert_ne!(*result.last().unwrap(), b'/');
        #[cfg(windows)]
        assert_ne!(*result.last().unwrap(), b'\\');
    }

    #[test]
    #[cfg(unix)]
    fn stdpaths_get_xdg_var_config_dirs_deduplicates_the_default() {
        let _lock = xdg_test_lock();
        let _guard = XdgEnvGuard::set(&[("XDG_CONFIG_DIRS", None)]);
        // The unix default is a single directory, so deduplication
        // leaves it as-is (minus the trailing separator os_strtok
        // would have dropped).
        let result = unsafe { stdpaths_get_xdg_var(XdgVarType::ConfigDirs) };
        assert_eq!(result, Some(b"/etc/xdg/".to_vec()));
    }

    #[test]
    #[cfg(unix)]
    fn stdpaths_get_xdg_var_data_dirs_deduplicates_a_repeated_entry() {
        let _lock = xdg_test_lock();
        let _guard = XdgEnvGuard::set(&[("XDG_DATA_DIRS", Some("/usr/share:/usr/local/share:/usr/share"))]);
        let result = unsafe { stdpaths_get_xdg_var(XdgVarType::DataDirs) };
        assert_eq!(result, Some(b"/usr/share:/usr/local/share".to_vec()));
    }
}
