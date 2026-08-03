//! Translated from `src/nvim/version.c` (tractable core only).
//!
//! `version.c` (~4300 lines) backs `:version`/`:intro`/`nvim --version`
//! and Vimscript's `has('nvim-X.Y.Z')`/`has('patch-N')` queries - mostly
//! either large build-generated string constants (`longVersion`, etc.)
//! or the `included_patchsets`/`num_patches` tables (a multi-thousand-
//! entry, per-Vim-version list of individually-tracked upstream Vim
//! patch numbers - not transcribed here in full: far too large and
//! low-value to hand-copy faithfully for `has_vim_patch`'s own sake,
//! and it is itself hand-maintained/regenerated upstream over time).
//!
//! Translated: [`has_nvim_version`] and [`min_vim_version`] - both need
//! only the small `vim_versions` table (5 entries) and the current
//! `NVIM_VERSION_MAJOR`/`MINOR`/`PATCH` constants (baked in here as
//! their current real values from this checkout's own
//! `CMakeLists.txt`, since they are build-generated in the original
//! too - see [`NVIM_VERSION_MAJOR`]'s own doc comment).
//!
//! Also translated: [`highest_patch`] - the ORIGINAL's own
//! `included_patchsets[0][0]` is a single, fixed, compile-time-constant
//! leading value of the first row (the `"801"` row, whose own highest
//! tracked patch is `2424` in this checkout's own current
//! `version.c`) - `highest_patch()` needs ONLY this one leading value,
//! not the whole multi-thousand-entry table, so it is transcribed
//! directly as `HIGHEST_PATCH` rather than deferred alongside the
//! full table `has_vim_patch` would need. This directly unblocks
//! `eval/vars.rs`'s own `evalvars_init`, whose `v:version`/
//! `v:versionlong` startup values need exactly `min_vim_version`/
//! `highest_patch` and nothing else.
//!
//! Also translated: [`may_show_intro`] - a pure boolean CHECK deciding
//! whether the intro screen should currently be shown (`drawscreen.c`'s
//! own real caller then decides separately whether to actually render
//! it) - re-investigated after an earlier, over-broad module-doc note
//! had conflated this check with the intro screen's own real
//! rendering (a genuine, still-deferred, rendering-pipeline concern);
//! `may_show_intro` itself has no rendering dependency at all.
//!
//! Deferred: `has_vim_patch` (needs the FULL `included_patchsets`
//! table, not just its own leading value, to check whether a
//! SPECIFIC patch number is included for a given Vim version - no
//! real caller yet, `has('patch-N')` isn't itself translated), the
//! version/build-info string constants, and the intro screen's own
//! real rendering (needs the drawing/screen-grid pipeline).

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

/// `included_patchsets[0][0]` - the highest individual Vim patch
/// number tracked for `vim_versions[0]` (Vim `8.1`), matching this
/// checkout's own current `version.c` (`(const int[]) { // 801`
/// `2424, 2423, ...`). See this module's own doc comment for why only
/// this single leading value is transcribed, not the whole table.
const HIGHEST_PATCH: i32 = 2424;

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

/// The highest individual Vim patch number Nvim has ever tracked
/// (`highest_patch`). See `HIGHEST_PATCH`'s own doc comment for why
/// this is a single transcribed constant rather than derived from the
/// full `included_patchsets` table.
#[must_use]
pub fn highest_patch() -> i32 {
    HIGHEST_PATCH
}

/// Whether the intro message should currently be shown
/// (`may_show_intro`) - `true` exactly when: `curbuf` is empty and
/// unnamed, `curbuf`/`curwin` are the very first buffer/window ever
/// created, `curwin` is the only (non-floating) window in its tab,
/// and `'shortmess'` doesn't include the `I` flag.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf`/`curwin` must be valid, non-null
/// pointers to live `BufT`/`WinT`s. `curbuf.b_ml.ml_mfp`, if non-null,
/// must be a valid pointer to a live `MemfileT` (touched transitively
/// via `crate::buffer::buf_is_empty`). `GLOBALS.firstwin`'s own
/// `w_next` chain must consist of valid, live `WinT` pointers (touched
/// transitively via `crate::window::one_window`).
#[must_use]
pub unsafe fn may_show_intro() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *g.curbuf };
    crate::buffer::buf_is_empty(curbuf)
        && curbuf.b_fname.is_none()
        && curbuf.handle == 1
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { &*g.curwin }.handle == crate::window::LOWEST_WIN_ID
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { crate::window::one_window(g.curwin, std::ptr::null()) }
        && crate::strings::vim_strchr(
            // SAFETY: momentary read, cloned out immediately.
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm.as_deref().unwrap_or(b""),
            i32::from(crate::option_vars::shm::INTRO),
        )
        .is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RAII guard saving/restoring `GLOBALS.curbuf`/`curwin`/
    /// `firstwin` together (all 3 touched by `may_show_intro`).
    /// Callers must hold `crate::globals::global_state_test_lock()`
    /// for the guard's whole lifetime (matching this crate's
    /// established "compose with an externally-held lock" pattern).
    struct MayShowIntroGuard {
        prev_curbuf: *mut crate::buffer_defs::BufT,
        prev_curwin: *mut crate::buffer_defs::WinT,
        prev_firstwin: *mut crate::buffer_defs::WinT,
    }

    impl MayShowIntroGuard {
        fn set(
            buf: *mut crate::buffer_defs::BufT,
            win: *mut crate::buffer_defs::WinT,
        ) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_curbuf = g.curbuf;
            let prev_curwin = g.curwin;
            let prev_firstwin = g.firstwin;
            g.curbuf = buf;
            g.curwin = win;
            g.firstwin = win;
            MayShowIntroGuard { prev_curbuf, prev_curwin, prev_firstwin }
        }
    }

    impl Drop for MayShowIntroGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.curbuf = self.prev_curbuf;
            g.curwin = self.prev_curwin;
            g.firstwin = self.prev_firstwin;
        }
    }

    fn reset_p_shm() {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm = None;
    }

    #[test]
    fn may_show_intro_true_for_a_fresh_empty_unnamed_buffer_alone_in_its_window() {
        let _lock = crate::globals::global_state_test_lock();
        reset_p_shm();
        let mut buf = crate::buffer_defs::BufT { handle: 1, ..Default::default() };
        unsafe { assert_eq!(crate::memline::ml_open(&mut buf), crate::vim_defs::OK) };
        let mut win = crate::buffer_defs::WinT {
            handle: crate::window::LOWEST_WIN_ID,
            ..Default::default()
        };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        let _guard = MayShowIntroGuard::set(buf_ptr, win_ptr);

        assert!(unsafe { may_show_intro() });

        drop(_guard);
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn may_show_intro_false_when_buffer_has_a_name() {
        let _lock = crate::globals::global_state_test_lock();
        reset_p_shm();
        let mut buf = crate::buffer_defs::BufT {
            handle: 1,
            b_fname: Some(b"foo.txt".to_vec()),
            ..Default::default()
        };
        let mut win = crate::buffer_defs::WinT {
            handle: crate::window::LOWEST_WIN_ID,
            ..Default::default()
        };
        let _guard = MayShowIntroGuard::set(
            &mut buf as *mut crate::buffer_defs::BufT,
            &mut win as *mut crate::buffer_defs::WinT,
        );

        // buf.b_ml.ml_line_count defaults to 0 (not 1), so buf_is_empty
        // short-circuits to false without ever touching the (null)
        // ml_mfp - no ml_open needed for this branch.
        assert!(!unsafe { may_show_intro() });
    }

    #[test]
    fn may_show_intro_false_when_buffer_handle_is_not_1() {
        let _lock = crate::globals::global_state_test_lock();
        reset_p_shm();
        let mut buf = crate::buffer_defs::BufT { handle: 2, ..Default::default() };
        let mut win = crate::buffer_defs::WinT {
            handle: crate::window::LOWEST_WIN_ID,
            ..Default::default()
        };
        let _guard = MayShowIntroGuard::set(
            &mut buf as *mut crate::buffer_defs::BufT,
            &mut win as *mut crate::buffer_defs::WinT,
        );
        assert!(!unsafe { may_show_intro() });
    }

    #[test]
    fn may_show_intro_false_when_window_handle_is_not_lowest() {
        let _lock = crate::globals::global_state_test_lock();
        reset_p_shm();
        let mut buf = crate::buffer_defs::BufT { handle: 1, ..Default::default() };
        let mut win = crate::buffer_defs::WinT {
            handle: crate::window::LOWEST_WIN_ID + 1,
            ..Default::default()
        };
        let _guard = MayShowIntroGuard::set(
            &mut buf as *mut crate::buffer_defs::BufT,
            &mut win as *mut crate::buffer_defs::WinT,
        );
        assert!(!unsafe { may_show_intro() });
    }

    #[test]
    fn may_show_intro_false_when_not_the_only_window() {
        let _lock = crate::globals::global_state_test_lock();
        reset_p_shm();
        let mut buf = crate::buffer_defs::BufT { handle: 1, ..Default::default() };
        let mut win2 = crate::buffer_defs::WinT { handle: 2, ..Default::default() };
        let win2_ptr = &mut win2 as *mut crate::buffer_defs::WinT;
        let mut win1 = crate::buffer_defs::WinT {
            handle: crate::window::LOWEST_WIN_ID,
            w_next: win2_ptr,
            ..Default::default()
        };
        let _guard = MayShowIntroGuard::set(
            &mut buf as *mut crate::buffer_defs::BufT,
            &mut win1 as *mut crate::buffer_defs::WinT,
        );
        assert!(!unsafe { may_show_intro() });
    }

    #[test]
    fn may_show_intro_false_when_shortmess_includes_intro_flag() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm = Some(b"I".to_vec());
        let mut buf = crate::buffer_defs::BufT { handle: 1, ..Default::default() };
        unsafe { assert_eq!(crate::memline::ml_open(&mut buf), crate::vim_defs::OK) };
        let mut win = crate::buffer_defs::WinT {
            handle: crate::window::LOWEST_WIN_ID,
            ..Default::default()
        };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        let _guard = MayShowIntroGuard::set(buf_ptr, win_ptr);

        assert!(!unsafe { may_show_intro() });

        drop(_guard);
        reset_p_shm();
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

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

    #[test]
    fn highest_patch_matches_the_first_row_leading_value() {
        assert_eq!(highest_patch(), 2424);
    }
}
