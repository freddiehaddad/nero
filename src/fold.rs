//! Translated from `src/nvim/fold.c` (tractable core only).
//!
//! `fold.c` (~3500 lines) is the manual/expr/indent/marker/syntax
//! fold-computation engine - a substantial subsystem of its own
//! (fold-tree construction/updates, `foldUpdateIEMS`'s line-scanning
//! state machine, nested fold levels, etc.), not remotely close to
//! being fully translated here.
//!
//! Translated: `foldmethodIsManual`/`foldmethodIsIndent` (pure
//! `'foldmethod'` string-prefix checks), `hasAnyFolding` (`terminal`/
//! `'foldenable'`/`foldmethodIsManual`/`w_folds`-emptiness check), and
//! the "there are no folds to find" fast path of `checkupdate`/
//! `hasFoldingWin`/`hasFolding` - covering the overwhelmingly common
//! case (a window that has never had any fold created). Each of these
//! functions' OWN real fold-tree-searching logic (reached only when
//! `hasAnyFolding`/`w_foldinvalid` indicate folds might genuinely
//! exist) is `unimplemented!()`, matching this crate's established
//! "narrow, discrete, opt-in configuration branch" precedent
//! (`window.rs`'s `win_fdccol_count`, `indent.rs`'s
//! `get_breakindent_win`, `cursor.rs`'s `coladvance2` virtualedit
//! branch) - genuinely reachable only by a session that has actually
//! created a fold, which nothing in this crate can currently do
//! (fold-creation itself needs `foldUpdate`/`setManualFold`/etc., none
//! translated).
//!
//! This precisely unblocks `cursor.c`'s `check_cursor_lnum`/
//! `check_cursor` (the `check_cursor_lnum` + `check_cursor_col` combo)
//! for the common no-folds case.
//!
//! Deferred: everything else (fold creation/opening/closing, the
//! `foldUpdateIEMS` scanning engine, `foldtext`, level computation,
//! `:fold`-family ex-commands), `get_cursor_rel_lnum` (`cursor.c` -
//! its own "no folds" fast path is a one-liner given `hasAnyFolding`
//! now exists, left for `cursor.rs` itself to pick up alongside
//! `check_cursor_lnum`/`check_cursor`).

use crate::buffer_defs::WinT;

/// @return true if `'foldmethod'` is "manual" (`foldmethodIsManual`).
#[must_use]
pub fn foldmethod_is_manual(wp: &WinT) -> bool {
    wp.w_onebuf_opt.wo_fdm.as_deref().is_some_and(|s| !s.is_empty() && s.get(3) == Some(&b'u'))
}

/// @return true if `'foldmethod'` is "indent" (`foldmethodIsIndent`).
#[must_use]
pub fn foldmethod_is_indent(wp: &WinT) -> bool {
    wp.w_onebuf_opt.wo_fdm.as_deref().is_some_and(|s| s.first() == Some(&b'i'))
}

/// @return true if there may be folded lines in window `win`
/// (`hasAnyFolding`).
///
/// # Safety
/// `win.w_buffer` must be a valid, non-null pointer to a live `BufT`.
#[must_use]
pub unsafe fn has_any_folding(win: &WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*win.w_buffer };
    buf.terminal.is_null()
        && win.w_onebuf_opt.wo_fen != 0
        && (!foldmethod_is_manual(win) || !win.w_folds.is_empty())
}

/// Update the fold information, and re-calculate what needs to be
/// displayed (`checkupdate`).
///
/// The real `foldUpdate` recomputation (needed whenever
/// `win.w_foldinvalid` is true - i.e. a fold was created/invalidated
/// since the last update) is `unimplemented!()`: nothing in this crate
/// can currently set `w_foldinvalid` to true (no fold-creation
/// function is translated yet), so every real caller of this function
/// today only ever observes the already-valid (no-op) case.
pub fn checkupdate(wp: &mut WinT) {
    if !wp.w_foldinvalid {
        return;
    }
    unimplemented!(
        "fold::checkupdate: foldUpdate (the real fold-tree recomputation) is not yet translated"
    );
}

/// Search folds starting at `lnum` (`hasFoldingWin`).
///
/// Only the "no folds in this window" fast path is translated (see
/// this module's own doc comment) - the real fold-tree search,
/// reached only when [`has_any_folding`] is true, is
/// `unimplemented!()`.
///
/// # Safety
/// `win.w_buffer` must be a valid, non-null pointer to a live `BufT`.
pub unsafe fn has_folding_win(win: &mut WinT, _lnum: crate::pos_defs::LinenrT) -> bool {
    checkupdate(win);

    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { has_any_folding(win) } {
        return false;
    }
    unimplemented!(
        "fold::has_folding_win: the real fold-tree search is not yet translated (only the \
         \"hasAnyFolding() == false\" fast path is)"
    );
}

/// When returning true, `firstp`/`lastp` would be set to the first and
/// last lnum of the sequence of folded lines - not modeled here since
/// only the "no folds" (`false`-returning) fast path is translated
/// (`hasFolding`).
///
/// # Safety
/// Same as [`has_folding_win`].
#[must_use]
pub unsafe fn has_folding(win: &mut WinT, lnum: crate::pos_defs::LinenrT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { has_folding_win(win, lnum) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    #[test]
    fn foldmethod_is_manual_true_for_manual() {
        let win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fdm: Some(b"manual".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(foldmethod_is_manual(&win));
    }

    #[test]
    fn foldmethod_is_manual_false_for_indent() {
        let win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fdm: Some(b"indent".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(!foldmethod_is_manual(&win));
        assert!(foldmethod_is_indent(&win));
    }

    #[test]
    fn has_any_folding_false_when_foldenable_is_off() {
        let mut buf = BufT::default();
        let win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_fen: 0, ..Default::default() },
            ..Default::default()
        };
        assert!(!unsafe { has_any_folding(&win) });
    }

    #[test]
    fn has_any_folding_false_for_manual_with_no_folds() {
        let mut buf = BufT::default();
        let win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        // 'foldenable' is on, but 'foldmethod'=manual with an empty
        // w_folds means there are no actual folds.
        assert!(!unsafe { has_any_folding(&win) });
    }

    #[test]
    fn has_folding_win_false_fast_path_when_foldenable_is_off() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_fen: 0, ..Default::default() },
            ..Default::default()
        };
        assert!(!unsafe { has_folding_win(&mut win, 1) });
        assert!(!unsafe { has_folding(&mut win, 1) });
    }

    #[test]
    #[should_panic(expected = "the real fold-tree search is not yet translated")]
    fn has_folding_win_panics_when_folding_could_be_active() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"expr".to_vec()), // not manual -> hasAnyFolding is true
                ..Default::default()
            },
            ..Default::default()
        };
        let _ = unsafe { has_folding_win(&mut win, 1) };
    }
}
