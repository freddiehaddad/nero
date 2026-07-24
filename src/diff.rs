//! Translated from `src/nvim/diff.c` (tractable core only).
//!
//! `diff.c` (~3000 lines) is neovim's diff-mode engine (computing/
//! displaying/navigating diff hunks between buffers) - a substantial
//! subsystem of its own, almost entirely dependent on real diff
//! computation (internal xdiff or external `diff` invocation), not
//! attempted here.
//!
//! Translated: [`DIFF_FLAGS`] (the file-static `diff_flags` bitset,
//! translated with its own exact real default-initializer value -
//! `DIFF_INTERNAL | DIFF_FILLER | DIFF_CLOSE_OFF | DIFF_LINEMATCH |
//! DIFF_INLINE_CHAR`, matching the real `'diffopt'` default string
//! `"internal,filler,closeoff,indent-heuristic,inline:char,
//! linematch:40"` - `indent-heuristic`/`linematch:40` affect other,
//! not-yet-translated file-statics, not `diff_flags` itself), the
//! `DIFF_*` flag constants, [`diffopt_filler`]/[`diffopt_closeoff`]
//! (pure bit tests); and [`diff_check_with_linestatus`]/
//! [`diff_check_fill`] - real, faithful translations of their "no
//! diffs at all in this tab page" early-return path (`curtab.
//! tp_first_diff.is_null()`, always true today since nothing in this
//! crate can create a diff - `:diffthis`/diff-computation machinery
//! not translated), matching this session's established "translate
//! the real always-taken early-return condition, not a hardcoded
//! shortcut" pattern (e.g. `autocmd.rs`'s `apply_autocmds` bypass
//! path). The `curtab.tp_diff_invalid` check (which would call the
//! substantial, untranslated `ex_diffupdate`) is ALSO always false
//! today (nothing sets it), so it's checked for real too rather than
//! assumed away.
//!
//! Deferred: everything else in the file - real diff computation/
//! display/navigation, needing the internal xdiff algorithm or
//! external `diff` process invocation, neither translated.

use crate::buffer_defs::WinT;

/// `DIFF_*` flags for [`DIFF_FLAGS`] (`diff_flags`' own bit values).
pub mod diff_flag {
    /// display filler lines (`DIFF_FILLER`).
    pub const FILLER: i32 = 0x001;
    /// ignore empty lines (`DIFF_IBLANK`).
    pub const IBLANK: i32 = 0x002;
    /// ignore case (`DIFF_ICASE`).
    pub const ICASE: i32 = 0x004;
    /// ignore change in white space (`DIFF_IWHITE`).
    pub const IWHITE: i32 = 0x008;
    /// ignore all white space changes (`DIFF_IWHITEALL`).
    pub const IWHITEALL: i32 = 0x010;
    /// ignore change in white space at EOL (`DIFF_IWHITEEOL`).
    pub const IWHITEEOL: i32 = 0x020;
    /// horizontal splits (`DIFF_HORIZONTAL`).
    pub const HORIZONTAL: i32 = 0x040;
    /// vertical splits (`DIFF_VERTICAL`).
    pub const VERTICAL: i32 = 0x080;
    /// diffoff when hidden (`DIFF_HIDDEN_OFF`).
    pub const HIDDEN_OFF: i32 = 0x100;
    /// use internal xdiff algorithm (`DIFF_INTERNAL`).
    pub const INTERNAL: i32 = 0x200;
    /// diffoff when closing window (`DIFF_CLOSE_OFF`).
    pub const CLOSE_OFF: i32 = 0x400;
    /// follow the wrap option (`DIFF_FOLLOWWRAP`).
    pub const FOLLOWWRAP: i32 = 0x800;
    /// match most similar lines within diff (`DIFF_LINEMATCH`).
    pub const LINEMATCH: i32 = 0x1000;
    /// no inline highlight (`DIFF_INLINE_NONE`).
    pub const INLINE_NONE: i32 = 0x2000;
    /// inline highlight with simple algorithm (`DIFF_INLINE_SIMPLE`).
    pub const INLINE_SIMPLE: i32 = 0x4000;
    /// inline highlight with character diff (`DIFF_INLINE_CHAR`).
    pub const INLINE_CHAR: i32 = 0x8000;
    /// inline highlight with word diff (`DIFF_INLINE_WORD`).
    pub const INLINE_WORD: i32 = 0x10000;
    /// use `'diffanchors'` to anchor the diff (`DIFF_ANCHOR`).
    pub const ANCHOR: i32 = 0x20000;
}

/// `diff_flags` - the parsed bit-flag form of `'diffopt'`. A file-
/// static in the original; translated as a `pub` `GlobalCell` since
/// (unlike most of this crate's file-statics) a real, currently-
/// reachable caller (this module's own [`diffopt_filler`]/
/// [`diffopt_closeoff`]) reads it. Initialized to the EXACT value the
/// original's own static initializer uses (see this module's own doc
/// comment) - not zero, since `'diffopt'`'s real default is NOT empty.
pub static DIFF_FLAGS: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(
    diff_flag::INTERNAL | diff_flag::FILLER | diff_flag::CLOSE_OFF | diff_flag::LINEMATCH
        | diff_flag::INLINE_CHAR,
);

/// Return `true` if `'diffopt'` contains `"closeoff"` (`diffopt_closeoff`).
#[must_use]
pub fn diffopt_closeoff() -> bool {
    (unsafe { *DIFF_FLAGS.get_mut() }) & diff_flag::CLOSE_OFF != 0
}

/// Return `true` if `'diffopt'` contains `"filler"` (`diffopt_filler`).
#[must_use]
pub fn diffopt_filler() -> bool {
    (unsafe { *DIFF_FLAGS.get_mut() }) & diff_flag::FILLER != 0
}

/// Return the diff status of `lnum` in window `wp`'s buffer,
/// optionally reporting a line-status code via `linestatus`
/// (`diff_check_with_linestatus`). This should only be used for
/// windows where `'diff'` is set.
///
/// Only the "no diffs at all in this tab page" early-return path is
/// translated (see this module's own doc comment) - the real diff-
/// hunk search (`diff_buf_idx`/the `tp_first_diff` linked-list walk)
/// is `unimplemented!()`, unreachable in practice today since nothing
/// in this crate can create a diff. `lnum` is accepted for signature
/// fidelity (the real function's own later "lnum must be a buffer
/// line" safety check, and the diff-hunk search itself, both need it)
/// but genuinely unused by the early-return path translated here.
///
/// # Safety
/// `crate::globals::GLOBALS.curtab` must be a valid, non-null pointer
/// to a live `TabpageT`.
#[must_use]
pub unsafe fn diff_check_with_linestatus(
    wp: &WinT,
    _lnum: crate::pos_defs::LinenrT,
    linestatus: Option<&mut i32>,
) -> i32 {
    if let Some(ls) = linestatus {
        *ls = 0;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { &*crate::globals::GLOBALS.get_mut().curtab };

    if curtab.tp_diff_invalid != 0 {
        // update after a big change - needs the real, substantial
        // ex_diffupdate, not yet translated. Unreachable in practice
        // today: nothing in this crate can currently set
        // tp_diff_invalid to a nonzero value.
        unimplemented!(
            "diff::diff_check_with_linestatus: ex_diffupdate is not yet translated - \
             unreachable in practice today since tp_diff_invalid is always 0"
        );
    }

    // no diffs at all
    if curtab.tp_first_diff.is_null() || wp.w_onebuf_opt.wo_diff == 0 {
        return 0;
    }

    unimplemented!(
        "diff::diff_check_with_linestatus: the real diff-hunk search is not yet translated - \
         unreachable in practice today since tp_first_diff is always null, see this module's \
         own doc comment"
    );
}

/// See [`diff_check_with_linestatus`] (`diff_check_fill`).
///
/// # Safety
/// Same as [`diff_check_with_linestatus`].
#[must_use]
pub unsafe fn diff_check_fill(wp: &WinT, lnum: crate::pos_defs::LinenrT) -> i32 {
    // be quick when there are no filler lines
    if !diffopt_filler() {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let n = unsafe { diff_check_with_linestatus(wp, lnum, None) };
    n.max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_flags_default_matches_the_real_diffopt_default() {
        // "internal,filler,closeoff,indent-heuristic,inline:char,
        // linematch:40" - matching diff.c's own static initializer.
        // Must hold the lock: DIFF_FLAGS is shared GlobalCell state
        // that other tests in this module temporarily mutate (see
        // diffopt_filler_false_when_flag_cleared/
        // diff_check_fill_returns_zero_when_diffopt_filler_disabled).
        let _lock = crate::globals::global_state_test_lock();
        let flags = unsafe { *DIFF_FLAGS.get_mut() };
        assert_eq!(
            flags,
            diff_flag::INTERNAL
                | diff_flag::FILLER
                | diff_flag::CLOSE_OFF
                | diff_flag::LINEMATCH
                | diff_flag::INLINE_CHAR
        );
    }

    #[test]
    fn diffopt_filler_true_by_default() {
        // See diff_flags_default_matches_the_real_diffopt_default's
        // own comment for why this lock is required.
        let _lock = crate::globals::global_state_test_lock();
        assert!(diffopt_filler());
    }

    #[test]
    fn diffopt_closeoff_true_by_default() {
        // See diff_flags_default_matches_the_real_diffopt_default's
        // own comment for why this lock is required.
        let _lock = crate::globals::global_state_test_lock();
        assert!(diffopt_closeoff());
    }

    #[test]
    fn diffopt_filler_false_when_flag_cleared() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *DIFF_FLAGS.get_mut() };
        unsafe { *DIFF_FLAGS.get_mut() &= !diff_flag::FILLER };
        assert!(!diffopt_filler());
        unsafe { *DIFF_FLAGS.get_mut() = prev };
    }

    /// Points `GLOBALS.curtab` at `tp` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime.
    struct CurtabGuard {
        previous: *mut crate::buffer_defs::TabpageT,
    }

    impl CurtabGuard {
        fn set(new_curtab: *mut crate::buffer_defs::TabpageT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
            unsafe { crate::globals::GLOBALS.get_mut() }.curtab = new_curtab;
            CurtabGuard { previous }
        }
    }

    impl Drop for CurtabGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curtab = self.previous;
        }
    }

    #[test]
    fn diff_check_with_linestatus_returns_zero_when_no_diffs_at_all() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let wp = WinT::default();
        let mut linestatus = 42;
        assert_eq!(
            unsafe { diff_check_with_linestatus(&wp, 1, Some(&mut linestatus)) },
            0
        );
        assert_eq!(linestatus, 0);
    }

    #[test]
    fn diff_check_with_linestatus_returns_zero_when_window_not_in_diff_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let wp = WinT { w_onebuf_opt: crate::buffer_defs::WinoptT { wo_diff: 0, ..Default::default() }, ..Default::default() };
        assert_eq!(unsafe { diff_check_with_linestatus(&wp, 1, None) }, 0);
    }

    #[test]
    fn diff_check_fill_returns_zero_when_diffopt_filler_disabled() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *DIFF_FLAGS.get_mut() };
        unsafe { *DIFF_FLAGS.get_mut() &= !diff_flag::FILLER };

        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT::default();
        assert_eq!(unsafe { diff_check_fill(&wp, 1) }, 0);

        unsafe { *DIFF_FLAGS.get_mut() = prev };
    }

    #[test]
    fn diff_check_fill_returns_zero_via_no_diffs_fast_path() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT::default();
        // diffopt_filler() is true by default, so this exercises
        // diff_check_with_linestatus's own "no diffs at all" path.
        assert!(diffopt_filler());
        assert_eq!(unsafe { diff_check_fill(&wp, 1) }, 0);
    }

    #[test]
    #[should_panic(expected = "ex_diffupdate")]
    fn diff_check_with_linestatus_panics_when_tp_diff_invalid_is_set() {
        // Not achievable via any real translated function yet (nothing
        // can set tp_diff_invalid) - pokes it directly to prove the
        // real, faithfully-translated check is in place, independent
        // of how tp_diff_invalid eventually gets set.
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT { tp_diff_invalid: 1, ..Default::default() };
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT::default();
        let _ = unsafe { diff_check_with_linestatus(&wp, 1, None) };
    }
}
