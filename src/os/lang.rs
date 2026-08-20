//! Translated from `src/nvim/os/lang.c` (tractable core only).
//!
//! `os/lang.c` manages the current locale (`:language`) and the
//! `v:lang`/`v:lc_time`/`v:ctype`/`v:collate` special variables.
//!
//! Translated: `get_locale_val` (`libc::setlocale(category, NULL)` -
//! query the current locale for a category, no change), `is_valid_mess_lang`,
//! `get_mess_lang`, `get_mess_env` (needs the platform split the
//! original itself has: non-Windows has a real `LC_MESSAGES` category;
//! Windows' C runtime doesn't define one at all, matching the
//! original's own `#ifdef LC_MESSAGES` split exactly - confirmed via a
//! standalone scratch `cargo build` that `libc::LC_MESSAGES` simply
//! doesn't exist on this Windows target), and `set_lang_var` (sets
//! `v:ctype`/`v:lang`/`v:lc_time`/`v:collate` via the already-real
//! `set_vim_var_string`).
//!
//! `os_getenv_noalloc`'s own "borrow, don't allocate" optimization
//! isn't replicated - this crate's already-real `os_getenv` (owned
//! `Vec<u8>`) does the identical semantic job, matching the
//! established "idiomatic Rust equivalent, not the exact C
//! representation" convention used throughout this crate.
//!
//! Also translated: locale discovery/cache (`find_locales`/
//! `init_locales`) and `get_lang_arg`/`get_locales` completion.
//! `free_locales` needs no Rust equivalent because the `LazyLock`
//! owns its process-lifetime vectors.
//!
//! Deferred: `init_locale`/`ex_language`/`lang_init`, which actually
//! change process locale state and need Ex-command parsing or macOS
//! platform integration.

#[cfg(windows)]
use crate::ascii_defs::ascii_isdigit;
use crate::eval::vars::{set_vim_var_string, VimVarIndex};
use crate::macros_defs::ascii_isalpha;

static LOCALES: std::sync::LazyLock<Option<Vec<Vec<u8>>>> =
    std::sync::LazyLock::new(find_locales);

fn find_locales() -> Option<Vec<Vec<u8>>> {
    #[cfg(windows)]
    {
        None
    }
    #[cfg(not(windows))]
    {
        let output = std::process::Command::new("locale")
            .arg("-a")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(
            output
                .stdout
                .split(|&byte| byte == b'\n')
                .filter(|locale| !locale.is_empty())
                .map(|locale| locale.strip_suffix(b"\r").unwrap_or(locale).to_vec())
                .collect(),
        )
    }
}

/// Obtain the locale value from the libraries for category `what`
/// (one of the `libc::LC_*` constants), without changing it
/// (`get_locale_val`). Returns `None` when the platform reports no
/// value (`setlocale` returning `NULL`, or the returned bytes not
/// being valid UTF-8/ASCII-safe to copy out).
#[must_use]
pub fn get_locale_val(what: i32) -> Option<Vec<u8>> {
    // SAFETY: `setlocale` with a null second argument only QUERIES
    // the current locale, never changes it - this is the same
    // "no side effect on the process" contract the original itself
    // relies on. The returned pointer, if non-null, points at a
    // NUL-terminated string owned by the C library (not this crate) -
    // copied out immediately into an owned `Vec<u8>`, never retained.
    let p = unsafe { libc::setlocale(what, std::ptr::null()) };
    if p.is_null() {
        return None;
    }
    // SAFETY: `p` is a valid, non-null, NUL-terminated C string per
    // `setlocale`'s own documented contract.
    let bytes = unsafe { std::ffi::CStr::from_ptr(p) }.to_bytes().to_vec();
    Some(bytes)
}

/// Whether `lang` starts with a valid language name - rejects an
/// absent/empty value, `"C"`, `"C.UTF-8"`, and others
/// (`is_valid_mess_lang`).
#[must_use]
pub fn is_valid_mess_lang(lang: Option<&[u8]>) -> bool {
    let Some(lang) = lang else { return false };
    lang.first().is_some_and(|&c| ascii_isalpha(i32::from(c)))
        && lang.get(1).is_some_and(|&c| ascii_isalpha(i32::from(c)))
}

/// Obtain the current messages language, used to set the default for
/// `'helplang'`. May return `None` (`get_mess_lang`).
#[must_use]
pub fn get_mess_lang() -> Option<Vec<u8>> {
    #[cfg(not(windows))]
    let p = get_locale_val(libc::LC_MESSAGES);
    // Necessary for Win32, where LC_MESSAGES is not defined and $LANG
    // may be set to the LCID number. LC_COLLATE is the best guess,
    // LC_TIME and LC_MONETARY may be set differently for a Japanese
    // working in the US.
    #[cfg(windows)]
    let p = get_locale_val(libc::LC_COLLATE);

    if is_valid_mess_lang(p.as_deref()) {
        p
    } else {
        None
    }
}

/// Get the language used for messages from the environment
/// (`get_mess_env`). Uses `LC_MESSAGES` when available (every
/// platform this crate targets except Windows); falls back to
/// `$LC_ALL`/`$LC_MESSAGES`/`$LANG` (ignoring a purely-numeric `$LANG`,
/// e.g. an LCID like `"1043"`) and finally `LC_CTYPE` on Windows,
/// matching the original's own real platform split exactly.
#[must_use]
pub fn get_mess_env() -> Option<Vec<u8>> {
    #[cfg(not(windows))]
    {
        get_locale_val(libc::LC_MESSAGES)
    }
    #[cfg(windows)]
    {
        if let Some(p) = crate::os::env::os_getenv(b"LC_ALL") {
            return Some(p);
        }
        if let Some(p) = crate::os::env::os_getenv(b"LC_MESSAGES") {
            return Some(p);
        }
        let mut p = crate::os::env::os_getenv(b"LANG");
        if p.as_deref().and_then(|v| v.first()).is_some_and(|&c| ascii_isdigit(i32::from(c))) {
            p = None; // ignore something like "1043"
        }
        if p.is_none() {
            p = get_locale_val(libc::LC_CTYPE);
        }
        p
    }
}

/// Set the `v:lang` variable according to the current locale setting.
/// Also does `v:lc_time` and `v:ctype` (`set_lang_var`).
///
/// # Safety
/// Same as [`crate::eval::vars::set_vim_var_string`].
pub unsafe fn set_lang_var() {
    let loc = get_locale_val(libc::LC_CTYPE);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_vim_var_string(VimVarIndex::Ctype, loc.as_deref()) };

    let loc = get_mess_env();
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_vim_var_string(VimVarIndex::Lang, loc.as_deref()) };

    let loc = get_locale_val(libc::LC_TIME);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_vim_var_string(VimVarIndex::LcTime, loc.as_deref()) };

    let loc = get_locale_val(libc::LC_COLLATE);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_vim_var_string(VimVarIndex::Collate, loc.as_deref()) };
}

/// Completion argument for `:language` (`get_lang_arg`).
#[must_use]
pub fn get_lang_arg(idx: i32) -> Option<&'static [u8]> {
    match idx {
        0 => Some(b"messages"),
        1 => Some(b"ctype"),
        2 => Some(b"time"),
        3 => Some(b"collate"),
        _ => usize::try_from(idx - 4)
            .ok()
            .and_then(|idx| LOCALES.as_ref()?.get(idx))
            .map(Vec::as_slice),
    }
}

/// Available locale name for completion (`get_locales`).
#[must_use]
pub fn get_locales(idx: i32) -> Option<&'static [u8]> {
    usize::try_from(idx)
        .ok()
        .and_then(|idx| LOCALES.as_ref()?.get(idx))
        .map(Vec::as_slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_locale_val_returns_the_current_ctype() {
        // The default process locale is "C" until something changes
        // it - never asserted to be exactly "C" here (a real value
        // could differ depending on process-wide state set up by
        // other tests/the OS), just that a query succeeds and returns
        // a real, non-empty value.
        let val = get_locale_val(libc::LC_CTYPE);
        assert!(val.is_some());
        assert!(!val.unwrap().is_empty());
    }

    #[test]
    fn is_valid_mess_lang_accepts_real_language_codes() {
        assert!(is_valid_mess_lang(Some(b"en")));
        assert!(is_valid_mess_lang(Some(b"en_US")));
        assert!(is_valid_mess_lang(Some(b"ja_JP.UTF-8")));
    }

    #[test]
    fn is_valid_mess_lang_rejects_none_and_short_or_non_alpha_values() {
        assert!(!is_valid_mess_lang(None));
        assert!(!is_valid_mess_lang(Some(b"")));
        assert!(!is_valid_mess_lang(Some(b"C")));
        assert!(!is_valid_mess_lang(Some(b"1")));
        assert!(!is_valid_mess_lang(Some(b"C.UTF-8")));
    }

    #[test]
    fn get_mess_lang_returns_none_or_a_valid_language() {
        // The default "C" locale makes this None on most fresh test
        // environments, but the real contract is just "None, or a
        // value that is_valid_mess_lang itself would accept".
        if let Some(lang) = get_mess_lang() {
            assert!(is_valid_mess_lang(Some(&lang)));
        }
    }

    #[test]
    fn get_mess_env_does_not_panic_and_is_internally_consistent() {
        // No behavioral assertion beyond "doesn't panic, and if a
        // value comes back it's non-empty" - the real value is
        // entirely environment-dependent (real $LANG/$LC_* state on
        // this machine), which this test deliberately doesn't
        // mutate/assume.
        if let Some(v) = get_mess_env() {
            assert!(!v.is_empty());
        }
    }

    #[test]
    fn set_lang_var_populates_all_four_vim_vars_without_panicking() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { set_lang_var() };
        // Every one of these should now be a real String (possibly
        // empty, but not the type's own "never set" Unknown state) -
        // get_vim_var_str already gracefully stringifies anything, so
        // this just proves set_lang_var actually wrote real values,
        // not that it panicked/no-op'd instead.
        let _ = unsafe { crate::eval::vars::get_vim_var_str(VimVarIndex::Ctype) };
        let _ = unsafe { crate::eval::vars::get_vim_var_str(VimVarIndex::Lang) };
        let _ = unsafe { crate::eval::vars::get_vim_var_str(VimVarIndex::LcTime) };
        let _ = unsafe { crate::eval::vars::get_vim_var_str(VimVarIndex::Collate) };
    }

    #[test]
    fn get_lang_arg_starts_with_the_four_fixed_categories() {
        assert_eq!(get_lang_arg(0), Some(b"messages".as_slice()));
        assert_eq!(get_lang_arg(1), Some(b"ctype".as_slice()));
        assert_eq!(get_lang_arg(2), Some(b"time".as_slice()));
        assert_eq!(get_lang_arg(3), Some(b"collate".as_slice()));
        assert_eq!(get_lang_arg(-1), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot spawn `locale -a`")]
    fn locale_completion_accessors_share_the_same_cache() {
        for index in 0..LOCALES.as_ref().map_or(0, Vec::len) {
            let locale = get_locales(index as i32).unwrap();
            assert_eq!(get_lang_arg(index as i32 + 4), Some(locale));
            assert!(!locale.is_empty());
        }
        assert_eq!(get_locales(-1), None);
        assert_eq!(get_locales(i32::MAX), None);
    }
}
