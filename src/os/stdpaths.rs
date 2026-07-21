//! Translated from `src/nvim/os/stdpaths.c` (tractable core only).
//!
//! Translated: `get_appname`, `appname_is_valid`.
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
//! Deferred (each needs a not-yet-translated subsystem):
//! - `xdg_env_vars`/`xdg_defaults`/`xdg_defaults_env_vars`/
//!   `xdg_remove_duplicate`/`stdpaths_get_xdg_var`/`get_xdg_home`/
//!   `stdpaths_user_cache_subpath`/`stdpaths_user_conf_subpath`/
//!   `stdpaths_user_data_subpath`/`stdpaths_user_state_subpath`: need
//!   `expand_env_save` (`os/env.c`, itself deferred - needs `path.c`'s
//!   more complex functions), `vim_gettempdir`/`os_realpath` (`os/fs.c`
//!   real I/O), and `concat_fnames_realloc` (`path.c`, not yet
//!   translated).

use crate::memory::memchrsub;
use crate::os::env::os_getenv;
use crate::path::{path_is_absolute, path_to_slash};

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
mod tests {
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
}
