//! Translated from `src/nvim/version.c` (tractable core only).
//!
//! `version.c` (~4300 lines) backs `:version`/`:intro`/`nvim --version`
//! and Vimscript's `has('nvim-X.Y.Z')`/`has('patch-N')` queries - mostly
//! either large build-generated string constants (`longVersion`, etc.)
//! or the `included_patchsets`/`num_patches` tables (a multi-thousand-
//! entry, per-Vim-version list of individually-tracked upstream Vim
//! patch numbers - not transcribed here: far too large and low-value
//! to hand-copy faithfully, and it is itself build-generated in the
//! original).
//!
//! Translated: [`has_nvim_version`] and [`min_vim_version`] - both need
//! only the small `vim_versions` table (5 entries) and the current
//! `NVIM_VERSION_MAJOR`/`MINOR`/`PATCH` constants (baked in here as
//! their current real values from this checkout's own
//! `CMakeLists.txt`, since they are build-generated in the original
//! too - see [`NVIM_VERSION_MAJOR`]'s own doc comment).
//!
//! Deferred: everything else - `highest_patch`/`has_vim_patch` (need
//! the `included_patchsets` table), the version/build-info string
//! constants, `may_show_intro`/intro-screen display (needs the
//! rendering pipeline).

/// Current Nvim major version (`NVIM_VERSION_MAJOR`), matching this
/// checkout's own `CMakeLists.txt` (`set(NVIM_VERSION_MAJOR 0)`) - a
/// build-generated constant in the original (`auto/versiondef.h`),
/// baked in directly here since there is no equivalent build-generation
/// step in this crate.
pub const NVIM_VERSION_MAJOR: i32 = 0;
/// Current Nvim minor version (`NVIM_VERSION_MINOR`), matching
/// `CMakeLists.txt`'s `set(NVIM_VERSION_MINOR 13)`.
pub const NVIM_VERSION_MINOR: i32 = 13;
/// Current Nvim patch version (`NVIM_VERSION_PATCH`), matching
/// `CMakeLists.txt`'s `set(NVIM_VERSION_PATCH 0)`.
pub const NVIM_VERSION_PATCH: i32 = 0;

/// Vim major*100+minor versions Nvim has tracked patches against
/// (`vim_versions`).
const VIM_VERSIONS: &[i32] = &[801, 802, 900, 901, 902];

/// Parses as many leading ASCII digits as possible from the start of
/// `s` into an `i32`, saturating rather than overflowing/panicking on
/// pathological input (an `atoi`-equivalent, given `s` is already known
/// to start with a digit at every real call site below).
fn atoi_prefix(s: &[u8]) -> i32 {
    let mut n: i32 = 0;
    for &b in s {
        if !b.is_ascii_digit() {
            break;
        }
        n = n.saturating_mul(10).saturating_add(i32::from(b - b'0'));
    }
    n
}

/// Compares a version string like `"1.3.42"` to the current Nvim
/// version - `true` if Nvim is at or above the given version
/// (`has_nvim_version`).
#[must_use]
pub fn has_nvim_version(version_str: &[u8]) -> bool {
    if !version_str.first().is_some_and(u8::is_ascii_digit) {
        return false;
    }
    let major = atoi_prefix(version_str);
    let mut minor = 0;
    let mut patch = 0;

    if let Some(dot1) = version_str.iter().position(|&b| b == b'.') {
        let after_dot1 = &version_str[dot1 + 1..];
        if !after_dot1.first().is_some_and(u8::is_ascii_digit) {
            return false;
        }
        minor = atoi_prefix(after_dot1);

        if let Some(dot2) = after_dot1.iter().position(|&b| b == b'.') {
            let after_dot2 = &after_dot1[dot2 + 1..];
            if !after_dot2.first().is_some_and(u8::is_ascii_digit) {
                return false;
            }
            patch = atoi_prefix(after_dot2);
        }
    }

    major < NVIM_VERSION_MAJOR
        || (major == NVIM_VERSION_MAJOR
            && (minor < NVIM_VERSION_MINOR
                || (minor == NVIM_VERSION_MINOR && patch <= NVIM_VERSION_PATCH)))
}

/// The oldest Vim major.minor version (encoded as `major*100 + minor`)
/// Nvim tracks patches against (`min_vim_version`).
#[must_use]
pub fn min_vim_version() -> i32 {
    VIM_VERSIONS[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_nvim_version_older_major_is_true() {
        assert!(has_nvim_version(b"0.1.0"));
    }

    #[test]
    fn has_nvim_version_newer_major_is_false() {
        assert!(!has_nvim_version(b"1.0.0"));
    }

    #[test]
    fn has_nvim_version_exact_match_is_true() {
        assert!(has_nvim_version(b"0.13.0"));
    }

    #[test]
    fn has_nvim_version_newer_patch_is_false() {
        assert!(!has_nvim_version(b"0.13.1"));
    }

    #[test]
    fn has_nvim_version_older_patch_is_true() {
        assert!(has_nvim_version(b"0.12.99"));
    }

    #[test]
    fn has_nvim_version_major_only() {
        // "0" means 0.0.0, which is <= the current version.
        assert!(has_nvim_version(b"0"));
    }

    #[test]
    fn has_nvim_version_major_minor_only() {
        assert!(has_nvim_version(b"0.13"));
        assert!(!has_nvim_version(b"0.14"));
    }

    #[test]
    fn has_nvim_version_non_digit_start_is_false() {
        assert!(!has_nvim_version(b"v0.13.0"));
        assert!(!has_nvim_version(b""));
    }

    #[test]
    fn has_nvim_version_non_digit_after_dot_is_false() {
        assert!(!has_nvim_version(b"0.x.0"));
    }

    #[test]
    fn min_vim_version_is_the_oldest_tracked_version() {
        assert_eq!(min_vim_version(), 801);
    }
}
