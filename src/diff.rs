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
//! `DIFF_*` flag constants, [`diffopt_filler`]/[`diffopt_closeoff`]/
//! [`diffopt_horizontal`]/[`diffopt_hiddenoff`] (pure bit tests);
//! [`diff_check_with_linestatus`]/
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
//! assumed away; and `diff_buf_idx`/[`diff_mode_buf`] - `diff_buf_idx`
//! is a plain linear scan through `TabpageT.tp_diffbuf[]` (already a
//! real field), and `diff_mode_buf` walks every tabpage via
//! `GLOBALS.first_tabpage`/`tp_next` (the same walk already
//! established by `window.rs`'s `win_valid_any_tab`) - genuinely
//! COMPLETE translations, not fast-path-only, since nothing about
//! either depends on a real diff actually existing.
//!
//! Also translated: [`diff_get_corresponding_line`]/[`diff_lnum_win`]
//! (plus the private `diff_get_corresponding_line_int`) - real,
//! faithful translations of their "current buffer isn't a diff buffer"
//! early-return path (always taken today since `diff_buf_idx` always
//! returns `DB_COUNT`, matching the same reasoning as
//! `diff_check_with_linestatus`), translated ahead of any real caller
//! (none of `winfloat.c`/`move.c`/`window.c`'s own diff-aware
//! scroll-binding callers are translated yet).
//!
//! Also translated: [`diff_infold`] - its own `idx`/`other`-computing
//! loop over `tp_diffbuf[]` is real, complete logic (not stubbed),
//! since it only reads already-real fields; only its OWN early-return
//! condition (`idx == -1 || !other`) happens to always be taken today,
//! for the same underlying reason as the functions above.
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

/// Return `true` if `'diffopt'` contains `"horizontal"`
/// (`diffopt_horizontal`).
#[must_use]
pub fn diffopt_horizontal() -> bool {
    (unsafe { *DIFF_FLAGS.get_mut() }) & diff_flag::HORIZONTAL != 0
}

/// Return `true` if `'diffopt'` contains `"hiddenoff"`
/// (`diffopt_hiddenoff`).
#[must_use]
pub fn diffopt_hiddenoff() -> bool {
    (unsafe { *DIFF_FLAGS.get_mut() }) & diff_flag::HIDDEN_OFF != 0
}

/// Return the diff status of `lnum` in window `wp`'s buffer,
/// optionally reporting a line-status code via `linestatus`
/// (`diff_check_with_linestatus`). This should only be used for
/// windows where `'diff'` is set.
///
/// Only the "no diffs at all in this tab page" early-return path is
/// translated (see this module's own doc comment) - the real diff-
/// hunk search (the `tp_first_diff` linked-list walk, now using the
/// already-real `diff_buf_idx`) is `unimplemented!()`, unreachable
/// in practice today since nothing in this crate can create a diff.
/// `lnum` is accepted for signature fidelity (the real function's own
/// later "lnum must be a buffer line" safety check, and the diff-hunk
/// search itself, both need it) but genuinely unused by the
/// early-return path translated here.
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

/// Return the index of `buf` in `tp`'s `tp_diffbuf[]` array, or
/// [`crate::buffer_defs::DB_COUNT`] if `buf` isn't currently
/// registered there (`diff_buf_idx`).
///
/// # Safety
/// `tp` must be a valid, non-null pointer to a live
/// [`crate::buffer_defs::TabpageT`].
fn diff_buf_idx(buf: *mut crate::buffer_defs::BufT, tp: *mut crate::buffer_defs::TabpageT) -> usize {
    // SAFETY: forwarded from this function's own safety doc.
    let tp = unsafe { &*tp };
    tp.tp_diffbuf
        .iter()
        .position(|&b| b == buf)
        .unwrap_or(crate::buffer_defs::DB_COUNT)
}

/// Return `true` if `buf` is being diffed in any tab page
/// (`diff_mode_buf`).
///
/// # Safety
/// `crate::globals::GLOBALS.first_tabpage` must be null or a valid
/// pointer to a live [`crate::buffer_defs::TabpageT`], and every
/// tabpage transitively reachable through `tp_next` from there must
/// likewise be valid.
#[must_use]
pub unsafe fn diff_mode_buf(buf: *mut crate::buffer_defs::BufT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        if diff_buf_idx(buf, tp) != crate::buffer_defs::DB_COUNT {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    false
}

/// Find the corresponding line in a diff (`diff_get_corresponding_line_int`).
///
/// Only the "no diffs at all" early-return path is translated (see
/// this module's own doc comment) - the real diff-block search
/// (walking `tp_first_diff`) is `unimplemented!()`, unreachable in
/// practice today since `diff_buf_idx` always returns `DB_COUNT`
/// (nothing in this crate can currently register a buffer as a diff
/// buffer).
///
/// # Safety
/// `GLOBALS.curbuf`/`curwin`/`curtab` must each be a valid, non-null
/// pointer to a live value.
unsafe fn diff_get_corresponding_line_int(
    buf1: *mut crate::buffer_defs::BufT,
    lnum1: crate::pos_defs::LinenrT,
) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let idx1 = diff_buf_idx(buf1, g.curtab);
    let idx2 = diff_buf_idx(g.curbuf, g.curtab);

    // SAFETY: forwarded from this function's own safety doc.
    let tp_first_diff_is_null = unsafe { &*g.curtab }.tp_first_diff.is_null();

    if idx1 == crate::buffer_defs::DB_COUNT
        || idx2 == crate::buffer_defs::DB_COUNT
        || tp_first_diff_is_null
    {
        return lnum1;
    }

    unimplemented!(
        "diff::diff_get_corresponding_line_int: the real diff-block search is not yet \
         translated - unreachable in practice today since diff_buf_idx always returns \
         DB_COUNT, see this module's own doc comment"
    );
}

/// Find the corresponding line in a diff, clamped so it never lands
/// past the end of the current buffer (`diff_get_corresponding_line`).
/// Translated ahead of a real caller (none of `winfloat.c`/
/// `move.c`/`window.c`'s own diff-aware scroll-binding callers are
/// translated yet), matching this crate's established "small,
/// self-contained piece ahead of the surrounding engine" precedent.
///
/// # Safety
/// Same as `diff_get_corresponding_line_int`.
#[must_use]
pub unsafe fn diff_get_corresponding_line(
    buf1: *mut crate::buffer_defs::BufT,
    lnum1: crate::pos_defs::LinenrT,
) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { diff_get_corresponding_line_int(buf1, lnum1) };
    // don't end up past the end of the file
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    lnum.min(curbuf.b_ml.ml_line_count)
}

/// For line `lnum` in the current window, find the equivalent line
/// number in window `wp`, compensating for inserted/deleted lines
/// (`diff_lnum_win`).
///
/// Only the "current buffer isn't a diff buffer" safety-check
/// early-return is translated - always taken today since
/// `diff_buf_idx` always returns `DB_COUNT` (see this module's own
/// doc comment). The real diff-block search is `unimplemented!()`.
///
/// # Safety
/// `GLOBALS.curbuf`/`curtab` must each be a valid, non-null pointer to
/// a live value.
#[must_use]
pub unsafe fn diff_lnum_win(
    _lnum: crate::pos_defs::LinenrT,
    _wp: *mut WinT,
) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let idx = diff_buf_idx(g.curbuf, g.curtab);

    if idx == crate::buffer_defs::DB_COUNT {
        // safety check
        return 0;
    }

    unimplemented!(
        "diff::diff_lnum_win: the real diff-block search is not yet translated - unreachable \
         in practice today since diff_buf_idx always returns DB_COUNT, see this module's own \
         doc comment"
    );
}

/// Return `true` if `lnum` in window `wp` is hidden by folding due to
/// a closed diff (`diff_infold`).
///
/// The "does this window's own buffer appear in `tp_diffbuf[]`, and is
/// there at least one OTHER real diff buffer too" loop is translated
/// in full (not stubbed) - it's genuine, self-contained logic over
/// already-real fields, faithfully correct for any future test that
/// manually populates `tp_diffbuf`. Its own early return (`idx == -1 ||
/// !other`) is always taken today (nothing in this crate can currently
/// register a buffer in `tp_diffbuf`, so `idx` always stays `-1`). The
/// real diff-block search beyond that point is `unimplemented!()`.
///
/// # Safety
/// `crate::globals::GLOBALS.curtab` must be a valid, non-null pointer
/// to a live [`crate::buffer_defs::TabpageT`].
#[must_use]
pub unsafe fn diff_infold(wp: &WinT, _lnum: crate::pos_defs::LinenrT) -> bool {
    // Return if 'diff' isn't set.
    if wp.w_onebuf_opt.wo_diff == 0 {
        return false;
    }

    let mut idx: i32 = -1;
    let mut other = false;
    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { &*crate::globals::GLOBALS.get_mut().curtab };
    for (i, &b) in curtab.tp_diffbuf.iter().enumerate() {
        if b == wp.w_buffer {
            idx = i32::try_from(i).expect("DB_COUNT is small, always fits in an i32");
        } else if !b.is_null() {
            other = true;
        }
    }

    // return here if there are no diffs in the window
    if idx == -1 || !other {
        return false;
    }

    unimplemented!(
        "diff::diff_infold: the real diff-block search is not yet translated - unreachable in \
         practice today since idx==-1 is always true (nothing can register a buffer in \
         tp_diffbuf), see this module's own doc comment"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

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

    #[test]
    fn diffopt_horizontal_false_by_default() {
        // "horizontal" is NOT part of the real 'diffopt' default
        // string - see diff_flags_default_matches_the_real_diffopt_
        // default's own comment for why this lock is required.
        let _lock = crate::globals::global_state_test_lock();
        assert!(!diffopt_horizontal());
    }

    #[test]
    fn diffopt_horizontal_true_when_flag_set() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *DIFF_FLAGS.get_mut() };
        unsafe { *DIFF_FLAGS.get_mut() |= diff_flag::HORIZONTAL };
        assert!(diffopt_horizontal());
        unsafe { *DIFF_FLAGS.get_mut() = prev };
    }

    #[test]
    fn diffopt_hiddenoff_false_by_default() {
        // "hiddenoff" is NOT part of the real 'diffopt' default
        // string either - same locking rationale as above.
        let _lock = crate::globals::global_state_test_lock();
        assert!(!diffopt_hiddenoff());
    }

    #[test]
    fn diffopt_hiddenoff_true_when_flag_set() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *DIFF_FLAGS.get_mut() };
        unsafe { *DIFF_FLAGS.get_mut() |= diff_flag::HIDDEN_OFF };
        assert!(diffopt_hiddenoff());
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

    #[test]
    fn diff_buf_idx_finds_a_registered_buffer() {
        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[2] = buf_ptr;
        assert_eq!(diff_buf_idx(buf_ptr, &mut tp as *mut crate::buffer_defs::TabpageT), 2);
    }

    #[test]
    fn diff_buf_idx_returns_db_count_when_not_registered() {
        let mut buf = BufT::default();
        let mut other = BufT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = &mut other as *mut BufT;
        assert_eq!(
            diff_buf_idx(&mut buf as *mut BufT, &mut tp as *mut crate::buffer_defs::TabpageT),
            crate::buffer_defs::DB_COUNT
        );
    }

    /// Points `GLOBALS.first_tabpage` at `head` for the guard's
    /// lifetime, restoring the previous value on drop. Callers must
    /// hold `global_state_test_lock()` for the guard's whole lifetime.
    struct FirstTabpageGuard {
        previous: *mut crate::buffer_defs::TabpageT,
    }

    impl FirstTabpageGuard {
        fn set(head: *mut crate::buffer_defs::TabpageT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
            unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = head;
            FirstTabpageGuard { previous }
        }
    }

    impl Drop for FirstTabpageGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = self.previous;
        }
    }

    #[test]
    fn diff_mode_buf_true_when_registered_in_a_non_first_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut tp2 = crate::buffer_defs::TabpageT::default();
        tp2.tp_diffbuf[0] = buf_ptr;
        let mut tp1 = crate::buffer_defs::TabpageT {
            tp_next: &mut tp2 as *mut crate::buffer_defs::TabpageT,
            ..Default::default()
        };
        let _guard = FirstTabpageGuard::set(&mut tp1 as *mut crate::buffer_defs::TabpageT);

        assert!(unsafe { diff_mode_buf(buf_ptr) });
    }

    #[test]
    fn diff_mode_buf_false_when_not_registered_anywhere() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        assert!(!unsafe { diff_mode_buf(&mut buf as *mut BufT) });
    }

    /// Points `GLOBALS.curbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime
    /// (matches this file's own `CurtabGuard`/`FirstTabpageGuard`
    /// convention: does NOT acquire its own lock).
    struct CurbufGuard {
        previous: *mut BufT,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut BufT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    #[test]
    fn diff_get_corresponding_line_returns_lnum_unchanged_via_no_diffs_fast_path() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf1 = BufT::default();
        let mut curbuf = BufT {
            b_ml: crate::memline_defs::MemlineT { ml_line_count: 100, ..Default::default() },
            ..Default::default()
        };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _curbuf_guard = CurbufGuard::set(&mut curbuf as *mut BufT);

        assert_eq!(unsafe { diff_get_corresponding_line(&mut buf1 as *mut BufT, 42) }, 42);
    }

    #[test]
    fn diff_get_corresponding_line_clamps_to_the_buffers_own_line_count() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf1 = BufT::default();
        let mut curbuf = BufT {
            b_ml: crate::memline_defs::MemlineT { ml_line_count: 10, ..Default::default() },
            ..Default::default()
        };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _curbuf_guard = CurbufGuard::set(&mut curbuf as *mut BufT);

        // lnum1 (999) exceeds curbuf's own line count (10) - clamped.
        assert_eq!(unsafe { diff_get_corresponding_line(&mut buf1 as *mut BufT, 999) }, 10);
    }

    #[test]
    fn diff_lnum_win_returns_zero_via_no_diffs_fast_path() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curbuf = BufT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _curbuf_guard = CurbufGuard::set(&mut curbuf as *mut BufT);

        let mut wp = WinT::default();
        assert_eq!(unsafe { diff_lnum_win(5, &mut wp as *mut WinT) }, 0);
    }

    #[test]
    fn diff_infold_false_when_diff_option_is_off() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_diff: 0, ..Default::default() },
            ..Default::default()
        };
        assert!(!unsafe { diff_infold(&wp, 1) });
    }

    #[test]
    fn diff_infold_false_when_window_buffer_is_not_registered() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_diff: 1, ..Default::default() },
            ..Default::default()
        };
        assert!(!unsafe { diff_infold(&wp, 1) });
    }

    #[test]
    fn diff_infold_false_when_no_other_buffer_is_registered() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = buf_ptr;
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT {
            w_buffer: buf_ptr,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_diff: 1, ..Default::default() },
            ..Default::default()
        };
        assert!(!unsafe { diff_infold(&wp, 1) });
    }

    #[test]
    #[should_panic(expected = "the real diff-block search is not yet translated")]
    fn diff_infold_panics_when_both_idx_and_other_are_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf1 = BufT::default();
        let mut buf2 = BufT::default();
        let buf1_ptr = &mut buf1 as *mut BufT;
        let buf2_ptr = &mut buf2 as *mut BufT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = buf1_ptr;
        tp.tp_diffbuf[1] = buf2_ptr;
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT {
            w_buffer: buf1_ptr,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_diff: 1, ..Default::default() },
            ..Default::default()
        };
        let _ = unsafe { diff_infold(&wp, 1) };
    }
}
