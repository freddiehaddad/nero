//! Translated from `src/nvim/fold.c` (tractable core only).
//!
//! `fold.c` (~3500 lines) is the manual/expr/indent/marker/syntax
//! fold-computation engine - a substantial subsystem of its own
//! (fold-tree construction/updates, `foldUpdateIEMS`'s line-scanning
//! state machine, nested fold levels, etc.), not remotely close to
//! being fully translated here.
//!
//! Translated: `foldmethodIsManual`/`foldmethodIsIndent`/
//! `foldmethodIsExpr`/`foldmethodIsMarker`/`foldmethodIsSyntax`/
//! `foldmethodIsDiff` (pure `'foldmethod'` string-prefix checks),
//! `hasAnyFolding` (`terminal`/
//! `'foldenable'`/`foldmethodIsManual`/`w_folds`-emptiness check), and
//! the "there are no folds to find" fast path of `checkupdate`/
//! `hasFoldingWin`/`hasFolding`/`fold_info`/`lineFolded` (as
//! [`line_folded`]) - covering the overwhelmingly common case (a
//! window that has never had any fold created). Each of these
//! functions' OWN real fold-tree-searching logic (reached only when
//! `hasAnyFolding`/`w_foldinvalid` indicate folds might genuinely
//! exist) is `unimplemented!()`, matching this crate's established
//! "narrow, discrete, opt-in configuration branch" precedent
//! (`window.rs`'s `win_fdccol_count`, `indent.rs`'s
//! `get_breakindent_win`, `cursor.rs`'s `coladvance2` virtualedit
//! branch) - genuinely reachable only by a session that has actually
//! created a fold, which nothing in this crate can currently do
//! (fold-creation itself needs `foldUpdate`/`setManualFold`/etc., none
//! translated). `has_folding_win`/`has_folding` carry their full,
//! real signatures (`firstp`/`lastp`/`cache`/`infop`, as
//! `Option<&mut _>` out-parameters) even though the fast path
//! translated here never touches `firstp`/`lastp` (matching the
//! original's own behavior on this exact path) - kept for forward-
//! compatibility with the real fold-tree search once it exists,
//! avoiding a future signature change; `fold_info`/`line_folded` are
//! this widened signature's first real consumers.
//!
//! This precisely unblocks `cursor.c`'s `check_cursor_lnum`/
//! `check_cursor` (the `check_cursor_lnum` + `check_cursor_col` combo)
//! for the common no-folds case, and (via [`line_folded`])
//! `plines.c`'s `plines_win_nofill`.
//!
//! Also translated: `foldLevel` (as [`fold_level`], used by
//! `foldlevel()`) - re-examined and found its own two early-exit
//! branches (a same-line cache hit, an "undefined, mid-update"
//! sentinel) both depend on file-static variables that are ONLY ever
//! mutated by the not-yet-translated `foldUpdateIEMS`, so they
//! permanently sit at their own C static-zero-init defaults here,
//! making the ORIGINAL's own `if (invalid_top == 0) { checkupdate(...);
//! }` branch unconditionally taken - see [`fold_level`]'s own doc
//! comment for the full reasoning.
//!
//! Also translated: `find_wl_entry` (a pure `win.w_lines[]` valid-
//! entry scan, needing only the already-real `WlineT`/`w_lines_valid`
//! fields - returns `Option<usize>` in place of the original's own
//! `-1` sentinel); `getDeepestNesting` (+ its own
//! `getDeepestNestingRecurse` helper) - the real recursive-descent-
//! into-nested-folds body is `unimplemented!()` (no `fold_T`/
//! `fd_nested` equivalent type exists yet), but the "no folds at all"
//! fast path (`w_folds` empty) is real and exact, covering every
//! currently-reachable case; `foldAdjustCursor` (as
//! [`fold_adjust_cursor`]) - currently always a no-op, since
//! [`has_folding`] can only ever return `false` or panic today, never
//! `true` (see [`fold_adjust_cursor`]'s own doc comment).
//!
//! `cursor.c`'s own `get_cursor_rel_lnum` (a real consumer of
//! [`has_any_folding`]/[`has_folding`]) is translated over in
//! `cursor.rs` itself, now completely (previously only had its "no
//! folding" fast path; the fold-skipping loop is now real too, via
//! the same already-existing `has_folding`).
//!
//! Deferred: everything else (fold creation/opening/closing, the
//! `foldUpdateIEMS` scanning engine, `foldtext`, `:fold`-family
//! ex-commands), `foldManualAllowed` (needs
//! `emsg` - message display, not yet translated - for its own two
//! real, reachable error-message branches; otherwise a one-liner given
//! `foldmethod_is_manual`/`foldmethod_is_marker` now both exist),
//! `foldAdjustVisual` (its own "found a fold" branches are provably
//! unreachable for the same reason as `fold_adjust_cursor`'s own doc
//! comment explains, but its logic is substantially more involved -
//! `Visual.start`/`w_cursor` pointer aliasing, `'selection'`-aware
//! column adjustment - not worth hand-writing untestable speculative
//! code for; a good candidate to revisit once the real fold-tree
//! search exists).

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

/// @return true if `'foldmethod'` is "expr" (`foldmethodIsExpr`).
#[must_use]
pub fn foldmethod_is_expr(wp: &WinT) -> bool {
    wp.w_onebuf_opt.wo_fdm.as_deref().is_some_and(|s| !s.is_empty() && s.get(1) == Some(&b'x'))
}

/// @return true if `'foldmethod'` is "marker" (`foldmethodIsMarker`).
#[must_use]
pub fn foldmethod_is_marker(wp: &WinT) -> bool {
    wp.w_onebuf_opt.wo_fdm.as_deref().is_some_and(|s| !s.is_empty() && s.get(2) == Some(&b'r'))
}

/// @return true if `'foldmethod'` is "syntax" (`foldmethodIsSyntax`).
#[must_use]
pub fn foldmethod_is_syntax(wp: &WinT) -> bool {
    wp.w_onebuf_opt.wo_fdm.as_deref().is_some_and(|s| s.first() == Some(&b's'))
}

/// @return true if `'foldmethod'` is "diff" (`foldmethodIsDiff`).
#[must_use]
pub fn foldmethod_is_diff(wp: &WinT) -> bool {
    wp.w_onebuf_opt.wo_fdm.as_deref().is_some_and(|s| s.first() == Some(&b'd'))
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
/// `unimplemented!()`. On this fast path, `firstp`/`lastp` are left
/// untouched (matching the original's own behavior: they're only ever
/// written on the "a fold WAS found" path) and `infop`'s `fi_level` is
/// set to `0` (matching the original's own `if (infop != NULL) {
/// infop->fi_level = 0; }` on this exact path) - every other
/// `FoldinfoT` field is left at whatever `infop` already held, again
/// matching the original (which writes only `fi_level` here). `cache`
/// is accepted for signature fidelity but genuinely unused: the
/// original doesn't read it until after the `hasAnyFolding` check
/// either.
///
/// # Safety
/// `win.w_buffer` must be a valid, non-null pointer to a live `BufT`.
pub unsafe fn has_folding_win(
    win: &mut WinT,
    _lnum: crate::pos_defs::LinenrT,
    _firstp: Option<&mut crate::pos_defs::LinenrT>,
    _lastp: Option<&mut crate::pos_defs::LinenrT>,
    _cache: bool,
    infop: Option<&mut crate::fold_defs::FoldinfoT>,
) -> bool {
    checkupdate(win);

    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { has_any_folding(win) } {
        if let Some(info) = infop {
            info.fi_level = 0;
        }
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
/// (`hasFolding`). On this fast path, `firstp`/`lastp` are left
/// untouched - see [`has_folding_win`]'s own doc comment.
///
/// # Safety
/// Same as [`has_folding_win`].
#[must_use]
pub unsafe fn has_folding(
    win: &mut WinT,
    lnum: crate::pos_defs::LinenrT,
    firstp: Option<&mut crate::pos_defs::LinenrT>,
    lastp: Option<&mut crate::pos_defs::LinenrT>,
) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { has_folding_win(win, lnum, firstp, lastp, true, None) }
}

/// Count the number of lines that are folded at line number `lnum`.
/// Normally `lnum` is the first line of a possible fold, and the
/// returned number is the number of lines in the fold. Doesn't use
/// caching from the displayed window (`fold_info`).
///
/// Only the "no folds in this window" fast path is reachable today
/// (see [`has_folding_win`]'s own doc comment) - always returns
/// `FoldinfoT { fi_lines: 0, .. }` in practice, matching the
/// original's own `else { info.fi_lines = 0; }` branch.
///
/// # Safety
/// Same as [`has_folding_win`].
#[must_use]
pub unsafe fn fold_info(win: &mut WinT, lnum: crate::pos_defs::LinenrT) -> crate::fold_defs::FoldinfoT {
    let mut info = crate::fold_defs::FoldinfoT::default();
    let mut last: crate::pos_defs::LinenrT = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let folded = unsafe { has_folding_win(win, lnum, None, Some(&mut last), false, None) };
    info.fi_lines = if folded { last - lnum + 1 } else { 0 };
    info
}

/// Low level function to check if a line is folded. Doesn't use any
/// caching (`lineFolded`).
///
/// Only the "no folds in this window" fast path is reachable today
/// (see [`has_folding_win`]'s own doc comment) - always returns
/// `false` in practice.
///
/// # Safety
/// Same as [`has_folding_win`].
#[must_use]
pub unsafe fn line_folded(win: &mut WinT, lnum: crate::pos_defs::LinenrT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { fold_info(win, lnum) }.fi_lines != 0
}

/// The fold level of line `lnum` in the CURRENT window (`foldLevel`,
/// used by `foldlevel()`). `wp` stands in for `curwin` (the original
/// itself hard-codes `curwin`, never takes a parameter) - callers
/// pass `GLOBALS.curwin` directly.
///
/// The original's own two early-exit branches
/// (`lnum == prev_lnum && prev_lnum_lvl >= 0` - a same-line cache hit;
/// `lnum >= invalid_top && lnum <= invalid_bot` - "undefined, mid-
/// update" sentinel `-1`) are never modeled: both depend on file-static
/// `prev_lnum`/`prev_lnum_lvl`/`invalid_top`/`invalid_bot` variables
/// that are ONLY ever mutated by `foldUpdateIEMS` (the real fold-tree
/// recomputation engine, not translated) - they permanently sit at
/// their own C static-zero-init defaults (`prev_lnum = 0`,
/// `prev_lnum_lvl = -1`, `invalid_top = 0`) in this crate today, which
/// means the ORIGINAL's own `if (invalid_top == 0) { checkupdate(...);
/// }` branch (not the two `else if`s) is unconditionally taken every
/// time - exactly what calling [`checkupdate`] unconditionally here
/// already achieves. `_lnum` is accepted (matching this module's own
/// `has_folding_win`-family precedent for signature fidelity) but
/// genuinely unused on this fast path - the real fold-tree level
/// search past `hasAnyFolding` needs `lnum` for real, but that whole
/// branch is `unimplemented!()` here.
///
/// # Safety
/// Same as [`has_any_folding`].
#[must_use]
pub unsafe fn fold_level(wp: &mut WinT, _lnum: crate::pos_defs::LinenrT) -> i32 {
    checkupdate(wp);

    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { has_any_folding(wp) } {
        return 0;
    }
    unimplemented!("fold::fold_level: the real fold-tree level search is not yet translated");
}

/// Find an entry in `win.w_lines` for buffer line `lnum`. Only valid
/// entries (`wl_valid`) are considered - entries where it is `false`
/// may have a stale `wl_lnum`/`wl_foldend` (`find_wl_entry`).
///
/// Returns the index of the entry, or `None` if not found (in place
/// of the original's own `-1` sentinel).
#[must_use]
pub fn find_wl_entry(win: &WinT, lnum: crate::pos_defs::LinenrT) -> Option<usize> {
    for (i, wl) in win.w_lines.iter().enumerate().take(win.w_lines_valid as usize) {
        if wl.wl_valid {
            if lnum < wl.wl_lnum {
                return None;
            }
            if lnum <= wl.wl_foldend {
                return Some(i);
            }
        }
    }
    None
}

/// Get the lowest `'foldlevel'` value that makes the deepest nested
/// fold in window `wp` (`getDeepestNesting`).
///
/// # Safety
/// Same as [`has_any_folding`].
#[must_use]
pub unsafe fn get_deepest_nesting(wp: &mut WinT) -> i32 {
    checkupdate(wp);
    get_deepest_nesting_recurse(&wp.w_folds)
}

/// Recursive per-`garray_T` step of [`get_deepest_nesting`]
/// (`getDeepestNestingRecurse`).
///
/// The real recursive-descent-into-nested-folds body is
/// `unimplemented!()` - this crate has no `fold_T`/`fd_nested`
/// equivalent type yet (nothing can create folds, so `w_folds` is
/// always empty in practice, matching [`has_any_folding`]'s own
/// established reasoning) - but the "no folds at all" fast path (an
/// empty `gap`) is real and exact: the original's own `for` loop over
/// `gap->ga_len` entries simply never executes for an empty array,
/// returning `maxlevel`'s untouched initial value of `0`.
#[must_use]
fn get_deepest_nesting_recurse(gap: &crate::garray_defs::GarrayT) -> i32 {
    if gap.is_empty() {
        return 0;
    }
    unimplemented!(
        "fold::get_deepest_nesting_recurse: no fold_T/fd_nested equivalent type exists yet to \
         recurse into"
    );
}

/// Move the cursor to the first line of a closed fold (`foldAdjustCursor`).
///
/// Currently always a no-op: [`has_folding`] can only ever return
/// `false` (leaving `wp.w_cursor.lnum` untouched) or panic via
/// [`has_folding_win`]'s own `unimplemented!()` (when
/// [`has_any_folding`] is true) - it can never return `true` today,
/// since that would require the not-yet-translated real fold-tree
/// search to have run and found something. This makes
/// `foldAdjustCursor`'s own sibling, `foldAdjustVisual`, provably
/// unreachable in its "found a fold" branches for the same reason -
/// deliberately not translated here to avoid writing speculative,
/// untestable logic (see this module's own doc comment).
///
/// # Safety
/// Same as [`has_any_folding`].
pub unsafe fn fold_adjust_cursor(wp: &mut WinT) {
    let lnum = wp.w_cursor.lnum;
    let mut new_lnum = lnum;
    // The original itself discards `hasFolding`'s own return value
    // here too, relying only on its `firstp` out-parameter side
    // effect.
    // SAFETY: forwarded from this function's own safety doc.
    let _ = unsafe { has_folding(wp, lnum, Some(&mut new_lnum), None) };
    wp.w_cursor.lnum = new_lnum;
}

/// Adjust fold info in window `wp` for a change in line numbers
/// (`foldMarkAdjust`). Must be called BEFORE actually changing the
/// line count (matching `mark_adjust_buf`'s own doc comment, its one
/// real caller).
///
/// Only ever reaches `fold_mark_adjust_recurse`'s "no folds at all"
/// fast path today (an empty `wp.w_folds`), for the same reason
/// [`get_deepest_nesting`]'s own doc comment already explains: nothing
/// in this crate can currently create a fold. The `line1`/`line2`
/// adjustment computed here is real and exact regardless (it's pure
/// arithmetic on the function's own parameters, no fold-tree access
/// involved) - only the final recursive step is limited to the empty
/// case.
///
/// # Safety
/// Same as [`has_any_folding`].
pub unsafe fn fold_mark_adjust(
    wp: &WinT,
    line1: crate::pos_defs::LinenrT,
    line2: crate::pos_defs::LinenrT,
    amount: crate::pos_defs::LinenrT,
    amount_after: crate::pos_defs::LinenrT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
    let insert_mode = (state & crate::state_defs::mode::INSERT as i32) != 0;
    let (line1, line2) = fold_mark_adjust_effective_range(line1, line2, amount, amount_after, insert_mode);
    fold_mark_adjust_recurse(&wp.w_folds, line1, line2, amount, amount_after);
}

/// Computes `foldMarkAdjust`'s own local `line1`/`line2` adjustments
/// (the part of the original function that doesn't touch the fold
/// tree at all - pure arithmetic on the function's own parameters),
/// extracted into its own directly-testable helper since
/// `fold_mark_adjust_recurse`'s current "no folds at all" fast path
/// discards both values without ever observing them.
#[must_use]
fn fold_mark_adjust_effective_range(
    mut line1: crate::pos_defs::LinenrT,
    mut line2: crate::pos_defs::LinenrT,
    amount: crate::pos_defs::LinenrT,
    amount_after: crate::pos_defs::LinenrT,
    insert_mode: bool,
) -> (crate::pos_defs::LinenrT, crate::pos_defs::LinenrT) {
    // If deleting marks from line1 to line2, but not deleting all those
    // lines, set line2 so that only deleted lines have their folds removed.
    if amount == crate::pos_defs::MAXLNUM && line2 >= line1 && line2 - line1 >= -amount_after {
        line2 = line1 - amount_after - 1;
    }
    if line2 < line1 {
        line2 = line1;
    }
    // If appending a line in Insert mode, it should be included in the fold
    // just above the line.
    if insert_mode && amount == 1 && line2 == crate::pos_defs::MAXLNUM {
        line1 -= 1;
    }
    (line1, line2)
}

/// Recursive per-`garray_T` step of [`fold_mark_adjust`]
/// (`foldMarkAdjustRecurse`).
///
/// The real recursive line-number-adjustment-within-nested-folds body
/// is `unimplemented!()` - this crate has no `fold_T`/nested-fold
/// equivalent type yet (matching [`fold_mark_adjust`]'s own doc
/// comment) - but the "no folds at all" fast path (an empty `gap`) is
/// real and exact: the original's own `if (gap->ga_len == 0) return;`
/// is its own very first statement, taken unconditionally today.
fn fold_mark_adjust_recurse(
    gap: &crate::garray_defs::GarrayT,
    _line1: crate::pos_defs::LinenrT,
    _line2: crate::pos_defs::LinenrT,
    _amount: crate::pos_defs::LinenrT,
    _amount_after: crate::pos_defs::LinenrT,
) {
    if gap.is_empty() {
        return;
    }
    unimplemented!(
        "fold::fold_mark_adjust_recurse: no fold_T/nested-fold equivalent type exists yet to \
         recurse into"
    );
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

    /// Builds a `WinT` with `'foldmethod'` set to `fdm` for exercising
    /// the `foldmethod_is_*` predicate family.
    fn win_with_fdm(fdm: &[u8]) -> WinT {
        WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_fdm: Some(fdm.to_vec()), ..Default::default() },
            ..Default::default()
        }
    }

    #[test]
    fn foldmethod_is_expr_true_only_for_expr() {
        assert!(foldmethod_is_expr(&win_with_fdm(b"expr")));
        assert!(!foldmethod_is_expr(&win_with_fdm(b"manual")));
        assert!(!foldmethod_is_expr(&win_with_fdm(b"indent")));
        assert!(!foldmethod_is_expr(&win_with_fdm(b"marker")));
        assert!(!foldmethod_is_expr(&win_with_fdm(b"syntax")));
        assert!(!foldmethod_is_expr(&win_with_fdm(b"diff")));
    }

    #[test]
    fn foldmethod_is_marker_true_only_for_marker() {
        assert!(foldmethod_is_marker(&win_with_fdm(b"marker")));
        assert!(!foldmethod_is_marker(&win_with_fdm(b"manual")));
        assert!(!foldmethod_is_marker(&win_with_fdm(b"indent")));
        assert!(!foldmethod_is_marker(&win_with_fdm(b"expr")));
        assert!(!foldmethod_is_marker(&win_with_fdm(b"syntax")));
        assert!(!foldmethod_is_marker(&win_with_fdm(b"diff")));
    }

    #[test]
    fn foldmethod_is_syntax_true_only_for_syntax() {
        assert!(foldmethod_is_syntax(&win_with_fdm(b"syntax")));
        assert!(!foldmethod_is_syntax(&win_with_fdm(b"manual")));
        assert!(!foldmethod_is_syntax(&win_with_fdm(b"indent")));
        assert!(!foldmethod_is_syntax(&win_with_fdm(b"expr")));
        assert!(!foldmethod_is_syntax(&win_with_fdm(b"marker")));
        assert!(!foldmethod_is_syntax(&win_with_fdm(b"diff")));
    }

    #[test]
    fn foldmethod_is_diff_true_only_for_diff() {
        assert!(foldmethod_is_diff(&win_with_fdm(b"diff")));
        assert!(!foldmethod_is_diff(&win_with_fdm(b"manual")));
        assert!(!foldmethod_is_diff(&win_with_fdm(b"indent")));
        assert!(!foldmethod_is_diff(&win_with_fdm(b"expr")));
        assert!(!foldmethod_is_diff(&win_with_fdm(b"marker")));
        assert!(!foldmethod_is_diff(&win_with_fdm(b"syntax")));
    }

    #[test]
    fn foldmethod_is_all_false_when_wo_fdm_is_unset() {
        let win = WinT::default();
        assert!(!foldmethod_is_manual(&win));
        assert!(!foldmethod_is_indent(&win));
        assert!(!foldmethod_is_expr(&win));
        assert!(!foldmethod_is_marker(&win));
        assert!(!foldmethod_is_syntax(&win));
        assert!(!foldmethod_is_diff(&win));
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
        assert!(!unsafe { has_folding_win(&mut win, 1, None, None, true, None) });
        assert!(!unsafe { has_folding(&mut win, 1, None, None) });
    }

    #[test]
    fn has_folding_win_leaves_firstp_lastp_untouched_and_sets_infop_level_zero() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_fen: 0, ..Default::default() },
            ..Default::default()
        };
        let mut first: crate::pos_defs::LinenrT = 111;
        let mut last: crate::pos_defs::LinenrT = 222;
        let mut info = crate::fold_defs::FoldinfoT { fi_level: 99, ..Default::default() };
        let folded = unsafe {
            has_folding_win(&mut win, 1, Some(&mut first), Some(&mut last), true, Some(&mut info))
        };
        assert!(!folded);
        // firstp/lastp untouched on this fast path.
        assert_eq!(first, 111);
        assert_eq!(last, 222);
        // infop's fi_level IS set to 0 on this fast path (matching the
        // original's own behavior); other fields untouched.
        assert_eq!(info.fi_level, 0);
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
        let _ = unsafe { has_folding_win(&mut win, 1, None, None, true, None) };
    }

    #[test]
    fn fold_info_no_folds_gives_zero_fi_lines() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_fen: 0, ..Default::default() },
            ..Default::default()
        };
        assert_eq!(unsafe { fold_info(&mut win, 5) }.fi_lines, 0);
    }

    #[test]
    fn line_folded_false_when_no_folds() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_fen: 0, ..Default::default() },
            ..Default::default()
        };
        assert!(!unsafe { line_folded(&mut win, 5) });
    }

    #[test]
    fn fold_level_is_zero_when_no_folds() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            // 'foldenable' on but 'foldmethod'=manual with an empty
            // w_folds - matches has_any_folding_false_for_manual_with_no_folds's
            // own established setup for a genuine "no folds" case
            // (wo_fen=1 alone, with the default non-"manual"
            // foldmethod, would make has_any_folding return true).
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                ..Default::default()
            },
            w_foldinvalid: false,
            ..Default::default()
        };
        assert_eq!(unsafe { fold_level(&mut win, 1) }, 0);
    }

    #[test]
    fn fold_level_is_zero_regardless_of_lnum() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                ..Default::default()
            },
            w_foldinvalid: false,
            ..Default::default()
        };
        // _lnum is genuinely unused on this fast path (see fold_level's
        // own doc comment) - any value yields the same 0 result.
        assert_eq!(unsafe { fold_level(&mut win, 9999) }, 0);
    }

    /// Builds a `WlineT` for `find_wl_entry` tests.
    fn wline(lnum: crate::pos_defs::LinenrT, foldend: crate::pos_defs::LinenrT, valid: bool) -> crate::buffer_defs::WlineT {
        crate::buffer_defs::WlineT { wl_lnum: lnum, wl_foldend: foldend, wl_valid: valid, ..Default::default() }
    }

    #[test]
    fn find_wl_entry_returns_none_for_an_empty_w_lines() {
        let win = WinT::default();
        assert_eq!(find_wl_entry(&win, 5), None);
    }

    #[test]
    fn find_wl_entry_finds_the_matching_range() {
        let win = WinT {
            w_lines: vec![wline(1, 3, true), wline(4, 4, true), wline(5, 8, true)],
            w_lines_valid: 3,
            ..Default::default()
        };
        assert_eq!(find_wl_entry(&win, 1), Some(0));
        assert_eq!(find_wl_entry(&win, 3), Some(0));
        assert_eq!(find_wl_entry(&win, 4), Some(1));
        assert_eq!(find_wl_entry(&win, 6), Some(2));
        assert_eq!(find_wl_entry(&win, 8), Some(2));
    }

    #[test]
    fn find_wl_entry_returns_none_when_lnum_is_before_the_first_entry() {
        let win = WinT {
            w_lines: vec![wline(5, 8, true)],
            w_lines_valid: 1,
            ..Default::default()
        };
        assert_eq!(find_wl_entry(&win, 2), None);
    }

    #[test]
    fn find_wl_entry_returns_none_when_lnum_is_past_the_last_entry() {
        let win = WinT {
            w_lines: vec![wline(1, 3, true), wline(4, 8, true)],
            w_lines_valid: 2,
            ..Default::default()
        };
        assert_eq!(find_wl_entry(&win, 9), None);
    }

    #[test]
    fn find_wl_entry_skips_invalid_entries() {
        // An invalid entry's wl_lnum/wl_foldend may be stale garbage
        // (e.g. left over from before the buffer changed) - find_wl_entry
        // must not let it terminate the scan early via its "lnum <
        // wl_lnum" check, matching the original's own unconditional
        // "if (win->w_lines[i].wl_valid)" per-entry guard.
        let win = WinT {
            w_lines: vec![wline(100, 200, false), wline(1, 5, true)],
            w_lines_valid: 2,
            ..Default::default()
        };
        assert_eq!(find_wl_entry(&win, 3), Some(1));
    }

    #[test]
    fn find_wl_entry_only_scans_up_to_w_lines_valid() {
        // A trailing entry beyond w_lines_valid must be ignored, even
        // if it would otherwise match.
        let win = WinT {
            w_lines: vec![wline(1, 3, true), wline(4, 8, true)],
            w_lines_valid: 1,
            ..Default::default()
        };
        assert_eq!(find_wl_entry(&win, 6), None);
    }

    #[test]
    fn get_deepest_nesting_is_zero_when_no_folds_exist() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_foldinvalid: false,
            ..Default::default()
        };
        assert_eq!(unsafe { get_deepest_nesting(&mut win) }, 0);
    }

    #[test]
    #[should_panic(expected = "no fold_T/fd_nested equivalent type exists yet")]
    fn get_deepest_nesting_panics_once_a_fold_actually_exists() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_foldinvalid: false,
            w_folds: crate::garray_defs::GarrayT { ga_len: 1, ..Default::default() },
            ..Default::default()
        };
        let _ = unsafe { get_deepest_nesting(&mut win) };
    }

    #[test]
    fn fold_adjust_cursor_leaves_cursor_lnum_unchanged_when_no_folds() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_fen: 0, ..Default::default() },
            w_cursor: crate::pos_defs::PosT { lnum: 7, col: 3, coladd: 0 },
            ..Default::default()
        };
        unsafe { fold_adjust_cursor(&mut win) };
        assert_eq!(win.w_cursor.lnum, 7);
        // fold_adjust_cursor only ever touches lnum (matching the
        // original's own &wp->w_cursor.lnum out-parameter) - col/coladd
        // must be left completely untouched.
        assert_eq!(win.w_cursor.col, 3);
    }

    #[test]
    #[should_panic(expected = "the real fold-tree search is not yet translated")]
    fn fold_adjust_cursor_panics_when_folding_could_be_active() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"expr".to_vec()),
                ..Default::default()
            },
            w_cursor: crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        unsafe { fold_adjust_cursor(&mut win) };
    }

    #[test]
    fn fold_mark_adjust_effective_range_normal_insert_leaves_range_as_is_when_ordered() {
        // amount != MAXLNUM (a plain insert/adjustment, not a delete) and
        // line1 <= line2 already - neither special-case branch fires.
        assert_eq!(fold_mark_adjust_effective_range(5, 8, 2, 0, false), (5, 8));
    }

    #[test]
    fn fold_mark_adjust_effective_range_clamps_line2_up_to_line1_when_reversed() {
        // line2 < line1 with a non-delete amount: line2 is clamped up to
        // line1 (the second `if` branch), independent of the first.
        assert_eq!(fold_mark_adjust_effective_range(20, 15, 3, 0, false), (20, 20));
    }

    #[test]
    fn fold_mark_adjust_effective_range_delete_narrows_line2_when_net_removal_is_smaller() {
        // Hand-traced: line1=10, line2=20 (11 lines nominally marked),
        // amount=MAXLNUM (delete), amount_after=-5 (only 5 lines actually
        // net-removed, e.g. some replacement content was inserted in their
        // place). The original's own guard:
        //   line2 - line1 (10) >= -amount_after (5) -> true
        //   => line2 = line1 - amount_after - 1 = 10 - (-5) - 1 = 14
        assert_eq!(
            fold_mark_adjust_effective_range(10, 20, crate::pos_defs::MAXLNUM, -5, false),
            (10, 14)
        );
    }

    #[test]
    fn fold_mark_adjust_effective_range_delete_leaves_line2_when_narrowing_condition_is_false() {
        // Hand-traced against the module's own doc-comment example:
        // "Delete lines 34 and 35: mark_adjust(34, 35, MAXLNUM, -2)".
        // line2 - line1 (1) >= -amount_after (2) -> false, so line2 stays
        // 35 unchanged (and line2 (35) is not < line1 (34) either).
        assert_eq!(
            fold_mark_adjust_effective_range(34, 35, crate::pos_defs::MAXLNUM, -2, false),
            (34, 35)
        );
    }

    #[test]
    fn fold_mark_adjust_effective_range_insert_mode_appended_line_includes_the_line_above() {
        // Hand-traced: amount=1, line2=MAXLNUM (append), insert_mode=true
        // -> line1 -= 1, so the appended line is included in the fold
        // just above it.
        assert_eq!(
            fold_mark_adjust_effective_range(5, crate::pos_defs::MAXLNUM, 1, 0, true),
            (4, crate::pos_defs::MAXLNUM)
        );
    }

    #[test]
    fn fold_mark_adjust_effective_range_insert_mode_branch_requires_insert_mode() {
        // Same shape as the previous test, but insert_mode=false - line1
        // must NOT be decremented.
        assert_eq!(
            fold_mark_adjust_effective_range(5, crate::pos_defs::MAXLNUM, 1, 0, false),
            (5, crate::pos_defs::MAXLNUM)
        );
    }

    #[test]
    fn fold_mark_adjust_recurse_is_a_no_op_when_gap_is_empty() {
        let gap = crate::garray_defs::GarrayT::default();
        fold_mark_adjust_recurse(&gap, 1, 5, 2, 0); // must not panic
    }

    #[test]
    #[should_panic(expected = "no fold_T/nested-fold equivalent type exists yet")]
    fn fold_mark_adjust_recurse_panics_when_gap_is_non_empty() {
        let gap = crate::garray_defs::GarrayT { ga_len: 1, ..Default::default() };
        fold_mark_adjust_recurse(&gap, 1, 5, 2, 0);
    }

    #[test]
    fn fold_mark_adjust_is_a_no_op_when_win_has_no_folds() {
        let _lock = crate::globals::global_state_test_lock();
        let win = WinT { w_folds: crate::garray_defs::GarrayT::default(), ..Default::default() };
        // A representative spread of amount/line1/line2/amount_after
        // combinations, covering every branch of the internal
        // effective-range computation - none should panic, since
        // `w_folds` stays empty throughout.
        unsafe { fold_mark_adjust(&win, 5, 8, 2, 0) };
        unsafe { fold_mark_adjust(&win, 20, 15, 3, 0) };
        unsafe { fold_mark_adjust(&win, 10, 20, crate::pos_defs::MAXLNUM, -5) };
        unsafe { fold_mark_adjust(&win, 5, crate::pos_defs::MAXLNUM, 1, 0) };
    }

    #[test]
    #[should_panic(expected = "no fold_T/nested-fold equivalent type exists yet")]
    fn fold_mark_adjust_panics_when_win_has_real_folds() {
        let _lock = crate::globals::global_state_test_lock();
        let win = WinT {
            w_folds: crate::garray_defs::GarrayT { ga_len: 1, ..Default::default() },
            ..Default::default()
        };
        unsafe { fold_mark_adjust(&win, 1, 5, 2, 0) };
    }
}
