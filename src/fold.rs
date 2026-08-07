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
//! `Option<&mut _>` out-parameters) and are now fully translated,
//! including the fold-tree search and the displayed-line cache;
//! `fold_info`/`line_folded` are their first real consumers.
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
//! `getDeepestNestingRecurse` helper), fully translated now that
//! `FoldT` exists; `foldAdjustCursor` (as
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
//! Also translated: `foldManualAllowed` (as [`fold_manual_allowed`]) -
//! its own real `emsg("E350"/"E351")` display (on the `false` path) is
//! skipped, matching this crate's established policy; the boolean
//! result itself (via already-real `foldmethod_is_manual`/
//! `foldmethod_is_marker`) is kept exactly.
//!
//! Deferred: everything else (fold creation/opening/closing, the
//! `foldUpdateIEMS` scanning engine, `foldtext`, `:fold`-family
//! ex-commands), `foldAdjustVisual` (its own "found a fold" branches
//! are provably unreachable for the same reason as
//! `fold_adjust_cursor`'s own doc comment explains, but its logic is
//! substantially more involved - `Visual.start`/`w_cursor` pointer
//! aliasing, `'selection'`-aware column adjustment - not worth
//! hand-writing untestable speculative code for; a good candidate to
//! revisit once the real fold-tree search exists).

use crate::buffer_defs::WinT;

/// Non-zero while fold updates are suppressed (`disable_fold_update`).
///
/// Set around operations that would otherwise trigger a fold
/// recomputation at a point where the buffer is in an inconsistent
/// state.
pub static DISABLE_FOLD_UPDATE: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);

/// Set by any operation that changes the fold structure, so the
/// caller knows the folds need saving/redrawing (`fold_changed`).
pub static FOLD_CHANGED: crate::globals::GlobalCell<bool> =
    crate::globals::GlobalCell::new(false);

/// Result flags reported by `setManualFold`/`setManualFoldWin`.
pub mod done {
    /// Nothing was found or changed (`DONE_NOTHING`).
    pub const DONE_NOTHING: i32 = 0;
    /// A fold was actually opened or closed (`DONE_ACTION`).
    pub const DONE_ACTION: i32 = 1;
    /// A fold was found at the given line (`DONE_FOLD`).
    pub const DONE_FOLD: i32 = 2;
}

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

/// Update the folds of window `wp` for the line range `top..bot`
/// (`foldUpdate`).
///
/// # Scope
///
/// Every guard is real and translated: the `disable_fold_update`
/// suppression, the Insert-mode skip for non-`"indent"` fold methods,
/// the pending-diff-redraw skip, the "mark existing folds in range as
/// maybe-small" pass, and the fold-method dispatch.
///
/// Two branches behind those guards are `unimplemented!()`, and both
/// are unreachable today:
///
/// The maybe-small pass needs `foldFind`; `w_folds` is always empty,
/// because nothing translated can create a fold yet.
///
/// `foldUpdateIEMS` is only reached when `'foldmethod'` is one of
/// `"indent"`/`"expr"`/`"marker"`/`"diff"`/`"syntax"`. `wo_fdm` is
/// `None` for every window this crate can build, so all five
/// predicates are false and the real default, `"manual"`, applies.
///
/// # Safety
/// Touches `crate::globals::GLOBALS` (for the current editor mode).
pub unsafe fn fold_update(wp: &mut WinT, top: crate::pos_defs::LinenrT, bot: crate::pos_defs::LinenrT) {
    // SAFETY: reading plain scalar globals, matching this crate's
    // established `GlobalCell::get_mut` convention.
    let disabled = *unsafe { DISABLE_FOLD_UPDATE.get_mut() } != 0;
    // SAFETY: as above.
    let state = unsafe { crate::globals::GLOBALS.get_mut() }.State as u32;
    if disabled || (state & crate::state_defs::mode::INSERT != 0 && !foldmethod_is_indent(wp)) {
        return;
    }

    // SAFETY: as above.
    if *unsafe { crate::diff::NEED_DIFF_REDRAW.get_mut() } {
        // Will be updated later.
        return;
    }

    if !wp.w_folds.is_empty() {
        // Mark all folds from top to bot (or bot to top) as
        // maybe-small.
        unimplemented!(
            "marking folds maybe-small needs foldFind, not yet translated; \
             unreachable while no fold can be created"
        );
    }

    if foldmethod_is_indent(wp)
        || foldmethod_is_expr(wp)
        || foldmethod_is_marker(wp)
        || foldmethod_is_diff(wp)
        || foldmethod_is_syntax(wp)
    {
        unimplemented!(
            "foldUpdateIEMS is not yet translated; unreachable while 'foldmethod' \
             cannot be set away from its real default of \"manual\""
        );
    }

    let _ = (top, bot);
}

/// Returns `true` if creating/deleting a manual fold is allowed with
/// `curwin`'s current `'foldmethod'` (`foldManualAllowed`). The
/// original's own `emsg("E350"/"E351")` display (on the `false` path,
/// selected by `create`) is skipped, matching this crate's established
/// "skip the deferred-subsystem side effect, keep the state/return
/// value correct" policy - `create` is accepted anyway (unused,
/// prefixed `_`) for signature fidelity.
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT`.
#[must_use]
pub unsafe fn fold_manual_allowed(_create: bool) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &*crate::globals::GLOBALS.get_mut().curwin };
    foldmethod_is_manual(curwin) || foldmethod_is_marker(curwin)
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
/// Returns whether `lnum` is inside a *closed* fold. When it is,
/// `firstp`/`lastp` receive the first and last line of the folded
/// range (with `lastp` clamped to the buffer's line count), and
/// `infop` receives the fold's level, start line and lowest level.
/// When it is not, `firstp`/`lastp` are left untouched - matching the
/// original, which only ever writes them on the "a fold WAS found"
/// path - while `infop` still receives the level and position
/// reached by the search.
///
/// With `cache` set, the displayed-line cache (`w_lines`) is
/// consulted first, which is faster but only valid for lines
/// currently on screen.
///
/// # Safety
/// `win.w_buffer` must be a valid, non-null pointer to a live `BufT`.
pub unsafe fn has_folding_win(
    win: &mut WinT,
    lnum: crate::pos_defs::LinenrT,
    firstp: Option<&mut crate::pos_defs::LinenrT>,
    lastp: Option<&mut crate::pos_defs::LinenrT>,
    cache: bool,
    infop: Option<&mut crate::fold_defs::FoldinfoT>,
) -> bool {
    checkupdate(win);

    // Return quickly when there is no folding at all in this window.
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { has_any_folding(win) } {
        if let Some(info) = infop {
            info.fi_level = 0;
        }
        return false;
    }

    let mut had_folded = false;
    let mut first: crate::pos_defs::LinenrT = 0;
    let mut last: crate::pos_defs::LinenrT = 0;

    if cache {
        // First look in cached info for displayed lines. This is
        // probably the fastest, but it can only be used if the entry
        // is still valid.
        if let Some(x) = find_wl_entry(win, lnum) {
            first = win.w_lines[x].wl_lnum;
            last = win.w_lines[x].wl_foldend;
            had_folded = win.w_lines[x].wl_folded;
        }
    }

    let mut lnum_rel = lnum;
    let mut level = 0;
    let mut low_level = 0;
    let mut maybe_small = false;
    let mut use_level = false;

    if first == 0 {
        // Recursively search for a fold that contains "lnum".
        let win_ptr: *mut WinT = win;
        // SAFETY: `gap` walks into `win.w_folds` and then into each
        // fold's own `fd_nested`, all reached through this one
        // pointer lineage, which stays valid for the whole walk.
        let mut gap: *mut Vec<FoldT> = unsafe { &raw mut (*win_ptr).w_folds };
        loop {
            // SAFETY: as above.
            let (found, idx) = fold_find(unsafe { &*gap }, lnum_rel);
            if !found {
                break;
            }
            // SAFETY: fold_find returned an in-bounds index.
            let fp: *mut FoldT = unsafe { (&mut *gap).as_mut_ptr().add(idx) };
            // SAFETY: as above.
            let fd_top = unsafe { (*fp).fd_top };

            // Remember lowest level of fold that starts in "lnum".
            if lnum_rel == fd_top && low_level == 0 {
                low_level = level + 1;
            }

            first += fd_top;
            last += fd_top;

            // Is this fold closed?
            // SAFETY: as above; `fp` and `win_ptr` are distinct
            // objects, so the two mutable borrows do not overlap.
            had_folded = unsafe {
                check_closed(
                    win_ptr,
                    &mut *fp,
                    &mut use_level,
                    level,
                    &mut maybe_small,
                    lnum - lnum_rel,
                )
            };
            if had_folded {
                // Fold closed: set last and quit loop.
                // SAFETY: as above.
                last += unsafe { (*fp).fd_len } - 1;
                break;
            }

            // Fold found, but it's open: check nested folds. Line
            // number is relative to the containing fold.
            // SAFETY: as above.
            gap = unsafe { &raw mut (*fp).fd_nested };
            lnum_rel -= fd_top;
            level += 1;
        }
    }

    if !had_folded {
        if let Some(info) = infop {
            info.fi_level = level;
            info.fi_lnum = lnum - lnum_rel;
            info.fi_low_level = if low_level == 0 { level } else { low_level };
        }
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    last = last.min(unsafe { (*win.w_buffer).b_ml.ml_line_count });
    if let Some(lastp) = lastp {
        *lastp = last;
    }
    if let Some(firstp) = firstp {
        *firstp = first;
    }
    if let Some(info) = infop {
        info.fi_level = level + 1;
        info.fi_lnum = first;
        info.fi_low_level = if low_level == 0 { level + 1 } else { low_level };
    }
    true
}

/// When returning true, `firstp`/`lastp` are set to the first and
/// last lnum of the sequence of folded lines (`hasFolding`). They are
/// left untouched when it returns false.
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
/// already achieves.
///
/// # Safety
/// Same as [`has_any_folding`].
#[must_use]
pub unsafe fn fold_level(wp: &mut WinT, lnum: crate::pos_defs::LinenrT) -> i32 {
    checkupdate(wp);

    // Return quickly when there is no folding at all in this window.
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { has_any_folding(wp) } {
        return 0;
    }

    fold_level_win(wp, lnum)
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

/// Recursive per-fold-array step of [`get_deepest_nesting`]
/// (`getDeepestNestingRecurse`).
///
/// Returns the depth of the deepest chain of nested folds in `gap`:
/// 0 for an empty array, 1 for folds with no children, and so on.
#[must_use]
fn get_deepest_nesting_recurse(gap: &[FoldT]) -> i32 {
    let mut maxlevel = 0;
    for fp in gap {
        let level = get_deepest_nesting_recurse(&fp.fd_nested) + 1;
        maxlevel = maxlevel.max(level);
    }
    maxlevel
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
/// The `line1`/`line2` adjustment computed here is pure arithmetic on
/// the function's own parameters; the per-fold work is
/// `fold_mark_adjust_recurse`.
///
/// # Safety
/// Same as [`has_any_folding`].
pub unsafe fn fold_mark_adjust(
    wp: &mut WinT,
    line1: crate::pos_defs::LinenrT,
    line2: crate::pos_defs::LinenrT,
    amount: crate::pos_defs::LinenrT,
    amount_after: crate::pos_defs::LinenrT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
    let insert_mode = (state & crate::state_defs::mode::INSERT as i32) != 0;
    let (line1, line2) = fold_mark_adjust_effective_range(line1, line2, amount, amount_after, insert_mode);
    fold_mark_adjust_recurse(
        &mut wp.w_folds,
        line1,
        line2,
        amount,
        amount_after,
        insert_mode,
    );
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

/// Recursive per-fold-array step of [`fold_mark_adjust`]
/// (`foldMarkAdjustRecurse`).
///
/// Walks the folds at or below `line1` and shifts each one according
/// to how it overlaps the changed range, recursing into nested folds
/// whenever a fold straddles a boundary. `amount == MAXLNUM` is the
/// "lines are being deleted" signal, in which case a fold contained
/// entirely in the range is removed outright rather than moved.
///
/// The six cases are the original's own, in its own order: a fold
/// wholly above the range is untouched; one wholly below it moves by
/// `amount_after`; one wholly inside it is deleted or moved; and the
/// three straddling cases correct their nested folds first, with the
/// line numbers rebased onto the fold, before adjusting themselves.
///
/// `insert_mode` stands in for the original's own `State & MODE_INSERT`
/// read, which its caller has already performed - an inserted line at
/// the top of a fold counts as part of the fold in Insert mode, and
/// does not otherwise.
fn fold_mark_adjust_recurse(
    gap: &mut Vec<FoldT>,
    line1: crate::pos_defs::LinenrT,
    line2: crate::pos_defs::LinenrT,
    amount: crate::pos_defs::LinenrT,
    amount_after: crate::pos_defs::LinenrT,
    insert_mode: bool,
) {
    if gap.is_empty() {
        return;
    }
    let maxlnum = crate::pos_defs::MAXLNUM;

    // In Insert mode an inserted line at the top of a fold is
    // considered part of the fold, otherwise it isn't.
    let top = if insert_mode && amount == 1 && line2 == maxlnum {
        line1 + 1
    } else {
        line1
    };

    // Find the fold containing or just below "line1".
    let (_, start) = fold_find(gap, line1);

    // Adjust all folds below "line1" that are affected.
    let mut i = start;
    while i < gap.len() {
        let (fd_top, fd_len) = (gap[i].fd_top, gap[i].fd_len);
        let last = fd_top + fd_len - 1; // last line of fold

        // 1. Fold completely above line1: nothing to do.
        if last < line1 {
            i += 1;
            continue;
        }

        if fd_top > line2 {
            // 6. Fold below line2: only adjust for amount_after.
            if amount_after == 0 {
                break;
            }
            gap[i].fd_top += amount_after;
        } else if fd_top >= top && last <= line2 {
            // 4. Fold completely contained in range.
            if amount == maxlnum {
                // Deleting lines: delete the fold completely.
                delete_fold_entry(gap, i, true);
                continue; // the next fold has shifted into this slot
            }
            gap[i].fd_top += amount;
        } else if fd_top < top {
            // 2 or 3: need to correct nested folds too.
            fold_mark_adjust_recurse(
                &mut gap[i].fd_nested,
                line1 - fd_top,
                line2 - fd_top,
                amount,
                amount_after,
                insert_mode,
            );
            if last <= line2 {
                // 2. Fold contains line1, line2 is below fold.
                if amount == maxlnum {
                    gap[i].fd_len = line1 - fd_top;
                } else {
                    gap[i].fd_len += amount;
                }
            } else {
                // 3. Fold contains line1 and line2.
                gap[i].fd_len += amount_after;
            }
        } else {
            // 5. Fold is below line1 and contains line2; need to
            // correct nested folds too.
            if amount == maxlnum {
                fold_mark_adjust_recurse(
                    &mut gap[i].fd_nested,
                    0,
                    line2 - fd_top,
                    amount,
                    amount_after + (fd_top - top),
                    insert_mode,
                );
                gap[i].fd_len -= line2 - fd_top + 1;
                gap[i].fd_top = line1;
            } else {
                fold_mark_adjust_recurse(
                    &mut gap[i].fd_nested,
                    0,
                    line2 - fd_top,
                    amount,
                    amount_after - amount,
                    insert_mode,
                );
                gap[i].fd_len += amount_after - amount;
                gap[i].fd_top += amount;
            }
        }
        i += 1;
    }
}

/// A single fold (`fold_T`, defined in `fold.c` itself rather than
/// `fold_defs.h` - kept with its own `.c` file's translation, the
/// same convention already used for `mark.h`/`memfile.h`).
///
/// # Translation note
/// The original nests folds through a `garray_T`, a type-erased
/// growable array. This crate's [`crate::garray_defs::GarrayT`] backs
/// that with a `Vec<u8>`, i.e. a byte buffer with alignment 1, which
/// cannot soundly hold a `FoldT`: the writes would be unaligned, and
/// nothing would run the destructor of the nested array each fold
/// owns. So `fd_nested` is a plain `Vec<FoldT>` here - genuinely the
/// same "growable array of folds" the original means, with the item
/// type made explicit instead of erased, and with growth and
/// ownership handled by `Vec` exactly as `garray_defs.rs`'s own doc
/// comment already describes for every other growing array.
///
/// That also makes `cloneFoldGrowArray`'s deep copy an ordinary
/// `Clone`, which is why this derives it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FoldT {
    /// First line of fold; for a nested fold, relative to its parent.
    pub fd_top: crate::pos_defs::LinenrT,
    /// Number of lines in the fold.
    pub fd_len: crate::pos_defs::LinenrT,
    /// Array of nested folds.
    pub fd_nested: Vec<FoldT>,
    /// See the `fd_flags` module.
    pub fd_flags: u8,
    /// Whether the fold is smaller than `'foldminlines'`;
    /// [`crate::types_defs::TriState::None`] applies to nested folds
    /// too.
    pub fd_small: crate::types_defs::TriState,
}

/// `fold_T.fd_flags` values (`FD_OPEN`/`FD_CLOSED`/`FD_LEVEL`).
pub mod fd_flags {
    /// Fold is open (nested ones can be closed).
    pub const FD_OPEN: u8 = 0;
    /// Fold is closed.
    pub const FD_CLOSED: u8 = 1;
    /// Depends on `'foldlevel'` (nested folds too).
    pub const FD_LEVEL: u8 = 2;
}

/// Maximum fold depth (`MAX_LEVEL`).
pub const MAX_LEVEL: i32 = 20;

/// Deep-copy a fold array, including every nested fold beneath it
/// (`cloneFoldGrowArray`).
///
/// # Translation note
/// The original has to walk the array by hand, copying each
/// `fold_T`'s four scalar fields and then recursing into `fd_nested`,
/// because a `garray_T` of `garray_T`s cannot be copied wholesale -
/// a shallow `memcpy` would leave the two arrays sharing one nested
/// allocation. With [`FoldT`] carrying a real `Vec<FoldT>`, that
/// recursion is exactly what `Clone` already does, so this is
/// `from.clone()`: same deep copy, same independence between source
/// and destination, with the recursion generated rather than
/// hand-written.
///
/// It is kept as a named function rather than inlined at its call
/// sites so `copyFoldingState` and friends still translate one-to-one.
#[must_use]
pub fn clone_fold_grow_array(from: &[FoldT]) -> Vec<FoldT> {
    from.to_vec()
}

/// Free a fold array and every fold nested beneath it
/// (`deleteFoldRecurse`).
///
/// # Translation note
/// The original is a single `GA_DEEP_CLEAR` whose per-item hook
/// recurses into `fd_nested`: it exists purely because C has to walk
/// the tree by hand to free each nested `garray_T` before freeing its
/// parent. Clearing a `Vec<FoldT>` already drops every element, and
/// each element's own `Vec<FoldT>` in turn, so the recursion is
/// generated rather than hand-written - the same "Rust's ownership
/// model already does the C free dance automatically" pattern this
/// crate uses elsewhere (see `buffer_updates.rs`'s own
/// `buf_free_callbacks` note).
///
/// The original's `buf_T *bp` parameter is unused by the free itself
/// (it is threaded through only for the macro's signature), so it has
/// no counterpart here.
pub fn delete_fold_recurse(gap: &mut Vec<FoldT>) {
    gap.clear();
}

/// Remove all folding for window `win` (`clearFolding`).
pub fn clear_folding(win: &mut WinT) {
    delete_fold_recurse(&mut win.w_folds);
    win.w_foldinvalid = false;
}

/// Mark every fold in `gap` as maybe-small (`setSmallMaybe`).
///
/// [`crate::types_defs::TriState::None`] is the "not known yet, and
/// this applies to nested folds too" state, so marking a fold is
/// enough to invalidate the smallness of everything beneath it -
/// which is why this only walks one level, exactly as the original
/// does.
pub fn set_small_maybe(gap: &mut [FoldT]) {
    for fp in gap {
        fp.fd_small = crate::types_defs::TriState::None;
    }
}

/// Search for line `lnum` in the fold array `gap` (`foldFind`).
///
/// Returns `(found, idx)`. When `found` is true, `idx` is the fold
/// containing `lnum`. When it is false, `idx` is the first fold below
/// `lnum`, which - exactly as the original's own doc comment warns -
/// **can be one past the end of the array**; callers rely on that as
/// the insertion point.
///
/// # Translation note
/// The original hands back a `fold_T *` through an out-parameter,
/// including the deliberately one-past-the-end pointer on the
/// not-found path. Returning an index instead keeps that contract
/// exactly (`idx == gap.len()` is the one-past-the-end case) while
/// staying inside what a safe Rust reference may express - the same
/// choice `find_wl_entry` already makes in this file, returning an
/// index rather than the original's `-1` sentinel.
#[must_use]
pub fn fold_find(gap: &[FoldT], lnum: crate::pos_defs::LinenrT) -> (bool, usize) {
    if gap.is_empty() {
        return (false, 0);
    }

    // Perform a binary search.
    // "low" is lowest index of possible match.
    // "high" is highest index of possible match.
    let mut low: i64 = 0;
    let mut high: i64 = gap.len() as i64 - 1;
    while low <= high {
        let i = (low + high) / 2;
        let fp = &gap[i as usize];
        if fp.fd_top > lnum {
            // Fold below lnum, adjust high.
            high = i - 1;
        } else if fp.fd_top + fp.fd_len <= lnum {
            // Fold above lnum, adjust low.
            low = i + 1;
        } else {
            // lnum is inside this fold.
            return (true, i as usize);
        }
    }
    (false, low as usize)
}

/// Fold level at line number `lnum` in window `wp` (`foldLevelWin`).
///
/// Walks down the fold tree from the top level, following whichever
/// fold contains the line, until no deeper fold does; the number of
/// steps taken is the level. Nested folds store `fd_top` relative to
/// their parent, so the line number is rebased on each descent.
///
/// Returns 0 when `lnum` is in no fold at all.
#[must_use]
pub fn fold_level_win(wp: &WinT, lnum: crate::pos_defs::LinenrT) -> i32 {
    let mut lnum_rel = lnum;
    let mut level = 0;

    // Recursively search for a fold that contains "lnum".
    let mut gap: &[FoldT] = &wp.w_folds;
    loop {
        let (found, idx) = fold_find(gap, lnum_rel);
        if !found {
            break;
        }
        // Check nested folds. Line number is relative to the
        // containing fold.
        let fp = &gap[idx];
        lnum_rel -= fp.fd_top;
        gap = &fp.fd_nested;
        level += 1;
    }

    level
}

/// Remove the fold at `idx` from `gap` (`deleteFoldEntry`).
///
/// With `recursive` set - or when the fold has no children anyway -
/// the fold and everything nested under it goes. Otherwise the fold
/// alone is removed and its children are promoted one level up to
/// take its place, which is why they need their `fd_top` rebased onto
/// the parent's (nested folds store it relative to their parent).
///
/// A promoted child inherits `FD_LEVEL` from its old parent, since
/// that flag means "depends on `'foldlevel'`" and must keep applying,
/// and likewise inherits an unknown [`crate::types_defs::TriState::None`]
/// smallness, which by definition also covers nested folds.
///
/// # Translation note
/// The original does this with `ga_grow` plus three `memmove`s and an
/// `xfree`, and has to re-derive `fp` afterwards because the array
/// may have been reallocated. Owning the folds in a `Vec<FoldT>`
/// turns that into a `remove` followed by a `splice`, with no
/// reallocation hazard and no manual free of the promoted array.
pub fn delete_fold_entry(gap: &mut Vec<FoldT>, idx: usize, recursive: bool) {
    let fp = gap.remove(idx);

    if recursive || fp.fd_nested.is_empty() {
        // Recursively delete the contained folds - dropping `fp` here
        // already does exactly that (see [`delete_fold_recurse`]).
        return;
    }

    // Move nested folds one level up, to overwrite the fold that is
    // deleted.
    let mut nested = fp.fd_nested;
    for nfp in &mut nested {
        // Adjust fd_top and fd_flags for the moved folds.
        nfp.fd_top += fp.fd_top;
        if fp.fd_flags == fd_flags::FD_LEVEL {
            nfp.fd_flags = fd_flags::FD_LEVEL;
        }
        if fp.fd_small == crate::types_defs::TriState::None {
            nfp.fd_small = crate::types_defs::TriState::None;
        }
    }
    gap.splice(idx..idx, nested);
}

/// Open every fold nested inside `fpr` (`foldOpenNested`).
///
/// Only the nested folds are opened; `fpr` itself is left alone,
/// exactly as the original does - its callers set its own flag.
pub fn fold_open_nested(fpr: &mut FoldT) {
    for fp in &mut fpr.fd_nested {
        fold_open_nested(fp);
        fp.fd_flags = fd_flags::FD_OPEN;
    }
}

/// Close manually-opened folds that no longer contain `lnum`
/// (`checkCloseRec`).
///
/// Only folds that were opened by hand ([`fd_flags::FD_OPEN`]) are
/// candidates: once `level` runs out, any such fold that does not
/// contain the line is handed back to `'foldlevel'` control by
/// setting [`fd_flags::FD_LEVEL`]. A fold that still contains the
/// line is recursed into instead, with the line number rebased onto
/// it since nested folds store `fd_top` relative to their parent.
///
/// Returns whether anything was actually closed.
pub fn check_close_rec(gap: &mut [FoldT], lnum: crate::pos_defs::LinenrT, level: i32) -> bool {
    let mut retval = false;
    for fp in gap {
        // Only manually opened folds may need to be closed.
        if fp.fd_flags == fd_flags::FD_OPEN {
            if level <= 0 && (lnum < fp.fd_top || lnum >= fp.fd_top + fp.fd_len) {
                fp.fd_flags = fd_flags::FD_LEVEL;
                retval = true;
            } else {
                let top = fp.fd_top;
                retval |= check_close_rec(&mut fp.fd_nested, lnum - top, level - 1);
            }
        }
    }
    retval
}

/// Update the `fd_small` field of fold `fp` (`checkSmall`).
///
/// A fold is "small" when it covers fewer screen lines than
/// `'foldminlines'`, in which case it is not worth closing. Folds
/// longer than that in buffer lines cannot possibly be small, which
/// is the cheap early test; otherwise the screen lines are counted
/// and the count stops as soon as it exceeds the limit.
///
/// `lnum_off` offsets `fp.fd_top`, since a nested fold stores it
/// relative to its parent.
///
/// Does nothing when the smallness is already known.
///
/// # Safety
/// Same as [`crate::plines::plines_win_nofold`], which this calls for
/// each line of the fold.
pub unsafe fn check_small(wp: *mut WinT, fp: &mut FoldT, lnum_off: crate::pos_defs::LinenrT) {
    if fp.fd_small != crate::types_defs::TriState::None {
        return;
    }

    // Mark any nested folds to maybe-small.
    set_small_maybe(&mut fp.fd_nested);

    // SAFETY: forwarded from this function's own safety doc.
    let fml = unsafe { (*wp).w_onebuf_opt.wo_fml };
    if crate::types_defs::OptInt::from(fp.fd_len) > fml {
        fp.fd_small = crate::types_defs::TriState::False;
    } else {
        let mut count: crate::types_defs::OptInt = 0;
        for n in 0..fp.fd_len {
            // SAFETY: forwarded from this function's own safety doc.
            count += crate::types_defs::OptInt::from(unsafe {
                crate::plines::plines_win_nofold(wp, fp.fd_top + lnum_off + n)
            });
            if count > fml {
                fp.fd_small = crate::types_defs::TriState::False;
                return;
            }
        }
        fp.fd_small = crate::types_defs::TriState::True;
    }
}

/// Whether fold `fp` is closed, updating the state needed to check
/// folds nested inside it (`check_closed`).
///
/// `use_levelp` carries "an enclosing fold had [`fd_flags::FD_LEVEL`]"
/// down the tree: once set, this fold and everything inside it are
/// governed by `'foldlevel'` rather than their own flag.
/// `maybe_smallp` likewise carries an as-yet-unknown smallness down,
/// since [`crate::types_defs::TriState::None`] applies to nested folds
/// too.
///
/// A small fold is never actually closed, so the smallness is
/// resolved before answering.
///
/// # Safety
/// Same as [`check_small`].
pub unsafe fn check_closed(
    wp: *mut WinT,
    fp: &mut FoldT,
    use_levelp: &mut bool,
    level: i32,
    maybe_smallp: &mut bool,
    lnum_off: crate::pos_defs::LinenrT,
) -> bool {
    let mut closed = false;

    // Check if this fold is closed. If the flag is FD_LEVEL this fold
    // and all folds it contains depend on 'foldlevel'.
    if *use_levelp || fp.fd_flags == fd_flags::FD_LEVEL {
        *use_levelp = true;
        // SAFETY: forwarded from this function's own safety doc.
        if crate::types_defs::OptInt::from(level) >= unsafe { (*wp).w_onebuf_opt.wo_fdl } {
            closed = true;
        }
    } else if fp.fd_flags == fd_flags::FD_CLOSED {
        closed = true;
    }

    // Small fold isn't closed anyway.
    if fp.fd_small == crate::types_defs::TriState::None {
        *maybe_smallp = true;
    }
    if closed {
        if *maybe_smallp {
            fp.fd_small = crate::types_defs::TriState::None;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { check_small(wp, fp, lnum_off) };
        if fp.fd_small == crate::types_defs::TriState::True {
            closed = false;
        }
    }
    closed
}

/// Initialise the fold state of a new window (`foldInitWin`).
///
/// # Translation note
/// The original is a `ga_init` sizing a `garray_T` for `fold_T` items
/// with a growth step of 10. `w_folds` is a `Vec<FoldT>` here, whose
/// item size and growth are the `Vec`'s own business, so this just
/// leaves it empty - the state `ga_init` produces.
pub fn fold_init_win(new_win: &mut WinT) {
    new_win.w_folds = Vec::new();
}

/// Copy one window's folding state onto another (`copyFoldingState`).
pub fn copy_folding_state(wp_from: &WinT, wp_to: &mut WinT) {
    wp_to.w_fold_manual = wp_from.w_fold_manual;
    wp_to.w_foldinvalid = wp_from.w_foldinvalid;
    wp_to.w_folds = clone_fold_grow_array(&wp_from.w_folds);
}

/// Reverse the folds in `gap` between the indices `start` and `end`
/// inclusive (`foldReverseOrder`).
pub fn fold_reverse_order(gap: &mut [FoldT], start: usize, end: usize) {
    if start < end && end < gap.len() {
        gap[start..=end].reverse();
    }
}

/// Insert a new, empty fold at index `i` in `gap` (`foldInsert`).
///
/// Existing folds from `i` onwards shift up to make room, which the
/// original does with a `memmove` after `ga_grow`.
pub fn fold_insert(gap: &mut Vec<FoldT>, i: usize) {
    gap.insert(i.min(gap.len()), FoldT::default());
}

/// Close folds that no longer contain the cursor, when
/// `'foldclose'` asks for it (`foldCheckClose`).
///
/// `'foldclose'` can only be `"all"` right now, so any non-empty
/// value means "close them", exactly as the original's own
/// `*p_fcl == NUL` test implies.
///
/// # Safety
/// Same as [`has_any_folding`]; also touches `GLOBALS.curwin` and
/// `OPTION_VARS`.
pub unsafe fn fold_check_close() {
    // SAFETY: forwarded from this function's own safety doc.
    let fcl_empty = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_fcl
        .as_deref()
        .is_none_or(|s| s.is_empty() || s[0] == 0);
    if fcl_empty {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &mut *curwin };
    checkupdate(win);

    let lnum = win.w_cursor.lnum;
    let level = win.w_onebuf_opt.wo_fdl as i32;
    if check_close_rec(&mut win.w_folds, lnum, level) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::r#move::changed_window_setting(curwin) };
    }
}

/// Split the fold at index `i` in `gap`, which starts before `top`
/// and ends below `bot`, into two pieces: one ending above `top` and
/// the other starting below `bot` (`foldSplit`).
///
/// The caller must first have removed any nested folds between `top`
/// and `bot`, which is why the nested-fold move below only has to
/// consider folds starting at or after `bot + 1`.
///
/// Both halves have their smallness invalidated, since neither covers
/// the same lines as before.
///
/// # Translation note
/// The original moves the trailing nested folds with `ga_grow` plus
/// an index loop, then fixes both arrays' lengths by hand. With a
/// `Vec<FoldT>` that is a `split_off`, so the two arrays cannot get
/// out of step.
pub fn fold_split(
    gap: &mut Vec<FoldT>,
    i: usize,
    top: crate::pos_defs::LinenrT,
    bot: crate::pos_defs::LinenrT,
) {
    // The fold continues below bot, need to split it.
    fold_insert(gap, i + 1);

    let (fd_top, fd_len, fd_flags) = (gap[i].fd_top, gap[i].fd_len, gap[i].fd_flags);
    let new_top = bot + 1;
    // Check for wrap around (MAXLNUM, and 32bit).
    assert!(new_top > bot, "fold_split: fd_top wrapped around");

    gap[i + 1].fd_top = new_top;
    gap[i + 1].fd_len = fd_len - (new_top - fd_top);
    gap[i + 1].fd_flags = fd_flags;
    gap[i + 1].fd_small = crate::types_defs::TriState::None;
    gap[i].fd_small = crate::types_defs::TriState::None;

    // Move nested folds below bot to the new fold. There can't be any
    // between top and bot, they have been removed by the caller.
    if !gap[i].fd_nested.is_empty() {
        let (_, idx) = fold_find(&gap[i].fd_nested, new_top - fd_top);
        let mut moved = gap[i].fd_nested.split_off(idx);
        if !moved.is_empty() {
            let shift = new_top - fd_top;
            for fp in &mut moved {
                fp.fd_top -= shift;
            }
            gap[i + 1].fd_nested = moved;
        }
    }

    gap[i].fd_len = top - fd_top;
    // SAFETY: FOLD_CHANGED is a plain GlobalCell<bool>, matching the
    // original's own single-threaded-editor assumption.
    *unsafe { FOLD_CHANGED.get_mut() } = true;
}

/// Merge the fold at index `i2` in `gap` into `fp1`, which must be
/// the fold immediately above it (`foldMerge`).
///
/// Nested folds that touch across the join are merged recursively
/// first, then `fp2`'s remaining children move to the end of `fp1`'s,
/// rebased by `fp1`'s length since they are now measured from a
/// parent that starts earlier. `fp2` itself is then removed from
/// `gap`.
///
/// # Translation note
/// `fp1` and `gap` are separate parameters because the original's own
/// recursion genuinely needs them to be: it descends with `fp1`'s
/// nested array on one side and `fp2`'s on the other. Passing the
/// containing array plus an index, rather than a second fold
/// reference, is what lets `fp2` be removed at the end - the original
/// recovers that same index with pointer arithmetic.
pub fn fold_merge(fp1: &mut FoldT, gap: &mut Vec<FoldT>, i2: usize) {
    // If the last nested fold in fp1 touches the first nested fold in
    // fp2, merge them recursively.
    let (found3, i3) = fold_find(&fp1.fd_nested, fp1.fd_len - 1);
    let (found4, i4) = fold_find(&gap[i2].fd_nested, 0);
    if found3 && found4 {
        // The two nested arrays belong to different folds, so these
        // borrows genuinely do not overlap.
        let fp3 = &mut fp1.fd_nested[i3];
        let gap2 = &mut gap[i2].fd_nested;
        fold_merge(fp3, gap2, i4);
    }

    // Move nested folds in fp2 to the end of fp1.
    let fp1_len = fp1.fd_len;
    let mut moved = std::mem::take(&mut gap[i2].fd_nested);
    if !moved.is_empty() {
        for fp in &mut moved {
            fp.fd_top += fp1_len;
        }
        fp1.fd_nested.append(&mut moved);
    }

    fp1.fd_len += gap[i2].fd_len;
    // Everything nested under fp2 has already been moved out, so the
    // original's recursive delete and a plain one are equivalent here.
    delete_fold_entry(gap, i2, true);
    // SAFETY: FOLD_CHANGED is a plain GlobalCell<bool>, matching the
    // original's own single-threaded-editor assumption.
    *unsafe { FOLD_CHANGED.get_mut() } = true;
}

/// Remove folds within the range `top` to `bot` inclusive
/// (`foldRemove`).
///
/// Folds are handled by how they overlap the range: one starting
/// above `top` is truncated there, or split in two when it also ends
/// below `bot`; one contained entirely in the range is deleted; one
/// that starts inside but ends below `bot` is moved to start at
/// `bot + 1`; and the walk stops at the first fold entirely below
/// `bot`. Nested folds are handled recursively first, with the range
/// rebased onto their parent.
///
/// # Safety
/// Reads `GLOBALS.State` (for the Insert-mode test that
/// `fold_mark_adjust_recurse` needs), which the original performs
/// inside that same function.
pub unsafe fn fold_remove(
    gap: &mut Vec<FoldT>,
    top: crate::pos_defs::LinenrT,
    bot: crate::pos_defs::LinenrT,
) {
    if bot < top {
        return; // nothing to do
    }
    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
    let insert_mode = (state & crate::state_defs::mode::INSERT as i32) != 0;

    while !gap.is_empty() {
        // Find fold that includes top or a following one.
        let (found, idx) = fold_find(gap, top);
        if found && gap[idx].fd_top < top {
            // 2: or 3: need to delete nested folds.
            let fd_top = gap[idx].fd_top;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { fold_remove(&mut gap[idx].fd_nested, top - fd_top, bot - fd_top) };
            if fd_top + gap[idx].fd_len - 1 > bot {
                // 3: need to split it.
                fold_split(gap, idx, top, bot);
            } else {
                // 2: truncate fold at "top".
                gap[idx].fd_len = top - fd_top;
            }
            // SAFETY: FOLD_CHANGED is a plain GlobalCell<bool>.
            *unsafe { FOLD_CHANGED.get_mut() } = true;
            continue;
        }
        if idx >= gap.len() || gap[idx].fd_top > bot {
            // 6: Found a fold below bot, can stop looking.
            break;
        }
        if gap[idx].fd_top >= top {
            // Found an entry below top.
            // SAFETY: FOLD_CHANGED is a plain GlobalCell<bool>.
            *unsafe { FOLD_CHANGED.get_mut() } = true;
            let fd_top = gap[idx].fd_top;
            if fd_top + gap[idx].fd_len - 1 > bot {
                // 5: Make fold that includes bot start below bot.
                fold_mark_adjust_recurse(
                    &mut gap[idx].fd_nested,
                    0,
                    bot - fd_top,
                    crate::pos_defs::MAXLNUM,
                    fd_top - bot - 1,
                    insert_mode,
                );
                gap[idx].fd_len -= bot - fd_top + 1;
                gap[idx].fd_top = bot + 1;
                break;
            }

            // 4: Delete completely contained fold.
            delete_fold_entry(gap, idx, true);
        }
    }
}

/// Mark every fold in `win` invalid so they are recomputed
/// (`foldUpdateAll`).
///
/// # Safety
/// Same as [`crate::drawscreen::redraw_later`].
pub unsafe fn fold_update_all(win: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*win).w_foldinvalid = true };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::drawscreen::redraw_later(win, crate::drawscreen::UPD_NOT_VALID) };
}

/// Apply a changed `'foldlevel'` to window `wp` (`newFoldLevelWin`).
///
/// Manually created folds are handed back to `'foldlevel'` control by
/// setting every top-level fold to [`fd_flags::FD_LEVEL`]; a later
/// manual open or close will change those back to
/// [`fd_flags::FD_OPEN`]/[`fd_flags::FD_CLOSED`] for the folds that
/// stop following `'foldlevel'` again.
///
/// # Safety
/// Same as `crate::move::changed_window_setting`.
pub unsafe fn new_fold_level_win(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &mut *wp };
    checkupdate(win);
    if win.w_fold_manual {
        // Set all flags for the first level of folds to FD_LEVEL.
        for fp in &mut win.w_folds {
            fp.fd_flags = fd_flags::FD_LEVEL;
        }
        win.w_fold_manual = false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::r#move::changed_window_setting(wp) };
}

/// Cut fold `fp` short so it ends at `end` (`truncate_fold`).
///
/// The original's own comment explains the `end += 1`: it wants to
/// stop *at* `end`, while [`fold_remove`] stops *above* the line it
/// is given.
///
/// # Safety
/// Same as [`fold_remove`].
pub unsafe fn truncate_fold(fp: &mut FoldT, end: crate::pos_defs::LinenrT) {
    // I want to stop *at here*, fold_remove() stops *above* top.
    let end = end + 1;
    let fd_top = fp.fd_top;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { fold_remove(&mut fp.fd_nested, end - fd_top, crate::pos_defs::MAXLNUM) };
    fp.fd_len = end - fd_top;
}

/// Extend the Visual selection so it covers whole closed folds
/// (`foldAdjustVisual`).
///
/// The selection's start is pulled back to the first line of any
/// closed fold it begins in, and its end pushed forward to that
/// fold's last line, so a fold is never half-selected.
///
/// # Safety
/// Touches `GLOBALS` (`Visual`, `curwin`) and `OPTION_VARS`; also
/// forwarded from [`has_folding`]'s own safety doc.
pub unsafe fn fold_adjust_visual() {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let curwin = g.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    if !g.Visual.active || !unsafe { has_any_folding(&*curwin) } {
        return;
    }

    // The earlier of the Visual anchor and the cursor is the start.
    // SAFETY: forwarded from this function's own safety doc.
    let cursor_first = crate::mark_defs::ltoreq(g.Visual.start, unsafe { (*curwin).w_cursor });
    let (mut start, mut end) = if cursor_first {
        // SAFETY: forwarded from this function's own safety doc.
        (g.Visual.start, unsafe { (*curwin).w_cursor })
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        (unsafe { (*curwin).w_cursor }, g.Visual.start)
    };

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { has_folding(&mut *curwin, start.lnum, Some(&mut start.lnum), None) } {
        start.col = 0;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let end_folded = unsafe { has_folding(&mut *curwin, end.lnum, None, Some(&mut end.lnum)) };
    if end_folded {
        // SAFETY: forwarded from this function's own safety doc.
        let buf = unsafe { (*curwin).w_buffer };
        // SAFETY: forwarded from this function's own safety doc.
        end.col = unsafe { crate::memline::ml_get_buf_len(&mut *buf, end.lnum) };
        // SAFETY: forwarded from this function's own safety doc.
        let sel_o = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_sel
            .as_deref()
            .is_some_and(|s| s.first() == Some(&b'o'));
        if end.col > 0 && sel_o {
            end.col -= 1;
        }
    }

    // Write both positions back the way round they came from.
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    if cursor_first {
        g.Visual.start = start;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*curwin).w_cursor = end };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*curwin).w_cursor = start };
        g.Visual.start = end;
    }

    if end_folded {
        // Prevent cursor from moving on the trail byte.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::mbyte::mb_adjust_cursor() };
    }
}

/// Resolve a path of nested-fold indices to the fold it names.
///
/// Used in place of the original's own `fold_T *found` pointer, which
/// it keeps across a descent into nested arrays and mutates
/// afterwards - a raw pointer lineage this crate deliberately avoids
/// where an index path expresses the same thing safely.
fn fold_at_path<'a>(gap: &'a mut [FoldT], path: &[usize]) -> &'a mut FoldT {
    let (first, rest) = path.split_first().expect("fold_at_path: empty path");
    let mut fp = &mut gap[*first];
    for idx in rest {
        fp = &mut fp.fd_nested[*idx];
    }
    fp
}

/// Open or close the fold at `lnum` in window `wp`
/// (`setManualFoldWin`).
///
/// Returns the line number of the next fold to try, so the repeat
/// wrappers can walk forward, or `MAXLNUM` when there is none.
///
/// Folds that were following `'foldlevel'` are switched to manual
/// control first, taking their current open/closed state from the
/// level they sit at. Closing without `recurse` closes the *deepest*
/// open fold containing the line, which is why the descent remembers
/// the last fold it entered; opening opens the *topmost* closed one.
///
/// `donep` accumulates [`done::DONE_ACTION`]/[`done::DONE_FOLD`]. When
/// it is `None` and this is the current window, a missing fold is
/// reported to the user, matching the original.
///
/// # Safety
/// Same as `crate::move::changed_window_setting`.
pub unsafe fn set_manual_fold_win(
    wp: *mut WinT,
    mut lnum: crate::pos_defs::LinenrT,
    opening: bool,
    recurse: bool,
    donep: Option<&mut i32>,
) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &mut *wp };
    let fdl = win.w_onebuf_opt.wo_fdl;

    let mut found_path: Option<Vec<usize>> = None;
    let mut path: Vec<usize> = Vec::new();
    let mut level = 0;
    let mut use_level = false;
    let mut found_fold = false;
    let mut next = crate::pos_defs::MAXLNUM;
    let mut off = 0;
    let mut done = done::DONE_NOTHING;

    checkupdate(win);

    // Find the fold, open or close it.
    loop {
        let gap: &mut Vec<FoldT> = if path.is_empty() {
            &mut win.w_folds
        } else {
            &mut fold_at_path(&mut win.w_folds, &path).fd_nested
        };

        let (found, idx) = fold_find(gap, lnum);
        if !found {
            // If there is a following fold, continue there next time.
            if idx < gap.len() {
                next = gap[idx].fd_top + off;
            }
            break;
        }

        // lnum is inside this fold.
        found_fold = true;

        // If there is a following fold, continue there next time.
        if idx + 1 < gap.len() {
            next = gap[idx + 1].fd_top + off;
        }

        // Change from level-dependent folding to manual.
        if use_level || gap[idx].fd_flags == fd_flags::FD_LEVEL {
            use_level = true;
            gap[idx].fd_flags = if crate::types_defs::OptInt::from(level) >= fdl {
                fd_flags::FD_CLOSED
            } else {
                fd_flags::FD_OPEN
            };
            for fp2 in &mut gap[idx].fd_nested {
                fp2.fd_flags = fd_flags::FD_LEVEL;
            }
        }

        // Simple case: close recursively means closing the fold.
        if !opening && recurse {
            if gap[idx].fd_flags != fd_flags::FD_CLOSED {
                done |= done::DONE_ACTION;
                gap[idx].fd_flags = fd_flags::FD_CLOSED;
            }
            break;
        } else if gap[idx].fd_flags == fd_flags::FD_CLOSED {
            // When opening, open topmost closed fold.
            if opening {
                gap[idx].fd_flags = fd_flags::FD_OPEN;
                done |= done::DONE_ACTION;
                if recurse {
                    fold_open_nested(&mut gap[idx]);
                }
            }
            break;
        }

        // Fold is open, check nested folds.
        let fd_top = gap[idx].fd_top;
        path.push(idx);
        found_path = Some(path.clone());
        lnum -= fd_top;
        off += fd_top;
        level += 1;
    }

    if found_fold {
        // When closing and not recurse, close deepest open fold.
        if !opening && let Some(fpath) = &found_path {
            fold_at_path(&mut win.w_folds, fpath).fd_flags = fd_flags::FD_CLOSED;
            done |= done::DONE_ACTION;
        }
        win.w_fold_manual = true;
        if done & done::DONE_ACTION != 0 {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::r#move::changed_window_setting(wp) };
        }
        done |= done::DONE_FOLD;
    }
    // The original's `emsg(e_nofold)` on the "no fold here" path is
    // omitted, matching this crate's established policy of skipping
    // message display where it changes no control flow (see
    // `memfile.rs`'s own module doc for the same decision).

    if let Some(donep) = donep {
        *donep |= done;
    }

    next
}

/// Open or close the fold at `pos` in the current window, mirroring
/// the operation into other diff-mode windows (`setManualFold`).
///
/// Returns the line number of the next fold to try, as
/// [`set_manual_fold_win`] does.
///
/// # Deferred boundary
/// The original also mirrors the operation into other diff-mode
/// windows, which needs the `'scrollbind'` window option (`w_p_scb`),
/// not yet a `WinT` field. The guard that reaches it is real and
/// faithful as far as it goes - `foldmethodIsDiff(curwin)` - and is
/// unreachable today, since `wo_fdm` is `None` for every window this
/// crate can build, so `'foldmethod'` is never `"diff"`.
///
/// # Safety
/// Same as [`set_manual_fold_win`]; also touches `GLOBALS.curwin`.
pub unsafe fn set_manual_fold(
    pos: crate::pos_defs::PosT,
    opening: bool,
    recurse: bool,
    donep: Option<&mut i32>,
) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { foldmethod_is_diff(&*curwin) } {
        unimplemented!(
            "fold::set_manual_fold: mirroring into other diff-mode windows needs the \
             'scrollbind' window option (w_p_scb), not yet a WinT field - unreachable \
             while 'foldmethod' can never be \"diff\""
        );
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_manual_fold_win(curwin, pos.lnum, opening, recurse, donep) }
}

/// Open or close the fold at `pos` `count` times
/// (`setFoldRepeat`).
///
/// Stops early as soon as an iteration changes nothing, which is how
/// `zo`/`zc` with a count stop at the outermost/innermost fold.
///
/// # Safety
/// Same as [`set_manual_fold`].
pub unsafe fn set_fold_repeat(pos: crate::pos_defs::PosT, count: i32, do_open: bool) {
    for _ in 0..count {
        let mut done = done::DONE_NOTHING;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { set_manual_fold(pos, do_open, false, Some(&mut done)) };
        if done & done::DONE_ACTION == 0 {
            // The original emits "E490: No fold found" here when the
            // very first iteration found no fold at all; the message
            // itself is omitted under this crate's established policy
            // (it changes no control flow).
            break;
        }
    }
}

/// Open the fold at `pos` in the current window, `count` times
/// (`openFold`).
///
/// # Safety
/// Same as [`set_fold_repeat`].
pub unsafe fn open_fold(pos: crate::pos_defs::PosT, count: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_fold_repeat(pos, count, true) };
}

/// Close the fold at `pos` in the current window, `count` times
/// (`closeFold`).
///
/// # Safety
/// Same as [`set_fold_repeat`].
pub unsafe fn close_fold(pos: crate::pos_defs::PosT, count: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_fold_repeat(pos, count, false) };
}

/// Open the fold at `pos` and everything nested inside it
/// (`openFoldRecurse`).
///
/// # Safety
/// Same as [`set_manual_fold`].
pub unsafe fn open_fold_recurse(pos: crate::pos_defs::PosT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_manual_fold(pos, true, true, None) };
}

/// Close the fold at `pos` and everything nested inside it
/// (`closeFoldRecurse`).
///
/// # Safety
/// Same as [`set_manual_fold`].
pub unsafe fn close_fold_recurse(pos: crate::pos_defs::PosT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_manual_fold(pos, false, true, None) };
}

/// Open folds until the cursor line is no longer in a closed fold
/// (`foldOpenCursor`).
///
/// # Safety
/// Same as [`set_manual_fold`].
pub unsafe fn fold_open_cursor() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    checkupdate(unsafe { &mut *curwin });
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { has_any_folding(&*curwin) } {
        loop {
            let mut done = done::DONE_NOTHING;
            // SAFETY: forwarded from this function's own safety doc.
            let pos = unsafe { (*curwin).w_cursor };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { set_manual_fold(pos, true, false, Some(&mut done)) };
            if done & done::DONE_ACTION == 0 {
                break;
            }
        }
    }
}

/// Open or close folds in the current window across lines `firstpos`
/// to `lastpos` (`opFoldRange`), used for `zo`/`zO`/`zc`/`zC` over a
/// Visual selection.
///
/// When acting on one level only, the next line to visit is taken
/// from the fold just opened or closed, so each fold in the range is
/// touched exactly once rather than repeatedly.
///
/// # Safety
/// Same as [`set_manual_fold`].
pub unsafe fn op_fold_range(
    firstpos: crate::pos_defs::PosT,
    lastpos: crate::pos_defs::PosT,
    opening: bool,
    recurse: bool,
) {
    let mut done = done::DONE_NOTHING; // avoid error messages
    let first = firstpos.lnum;
    let last = lastpos.lnum;

    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;

    let mut lnum = first;
    while lnum <= last {
        let temp = crate::pos_defs::PosT { lnum, col: 0, coladd: 0 };
        let mut lnum_next = lnum;
        // Opening one level only: next fold to open is after the one
        // going to be opened.
        if opening && !recurse {
            // SAFETY: forwarded from this function's own safety doc.
            let _ = unsafe { has_folding(&mut *curwin, lnum, None, Some(&mut lnum_next)) };
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { set_manual_fold(temp, opening, recurse, Some(&mut done)) };
        // Closing one level only: next line to close a fold is after
        // the just-closed fold.
        if !opening && !recurse {
            // SAFETY: forwarded from this function's own safety doc.
            let _ = unsafe { has_folding(&mut *curwin, lnum, None, Some(&mut lnum_next)) };
        }
        lnum = lnum_next + 1;
    }
    // The original's `emsg(e_nofold)` when nothing was found is
    // omitted under this crate's established message-display policy;
    // `done` is still accumulated exactly as upstream.
    let _ = done;
}

/// The `'foldmarker'` end marker, plus the byte lengths of both
/// markers, as parsed by [`parse_marker`] (`foldendmarker`,
/// `foldstartmarkerlen`, `foldendmarkerlen`).
///
/// The original keeps these as three file-statics that
/// `foldlevelMarker` and the marker-writing helpers read after
/// `parseMarker` has run. Grouping them into one cell keeps that
/// "these are set together, read together" contract explicit.
#[derive(Debug, Clone, Default)]
pub struct FoldMarkers {
    /// The end marker, i.e. everything after the comma in
    /// `'foldmarker'`.
    pub end: Vec<u8>,
    /// Length of the start marker, i.e. everything before the comma.
    pub start_len: usize,
    /// Length of [`FoldMarkers::end`].
    pub end_len: usize,
}

/// Parsed `'foldmarker'` state, valid after a [`parse_marker`] call.
pub static FOLD_MARKERS: crate::globals::GlobalCell<Option<FoldMarkers>> =
    crate::globals::GlobalCell::new(None);

/// Split `wp`'s `'foldmarker'` into its start and end halves
/// (`parseMarker`).
///
/// `'foldmarker'` is a comma-separated pair, so the start marker is
/// everything before the first comma and the end marker everything
/// after it. Must be called before anything that reads
/// [`FOLD_MARKERS`].
///
/// # Safety
/// Touches the [`FOLD_MARKERS`] global; no overlapping live access.
pub unsafe fn parse_marker(wp: &WinT) {
    let fmr = wp.w_onebuf_opt.wo_fmr.as_deref().unwrap_or(b"");
    let markers = match fmr.iter().position(|&c| c == b',') {
        Some(comma) => {
            let end = fmr[comma + 1..].to_vec();
            FoldMarkers {
                start_len: comma,
                end_len: end.len(),
                end,
            }
        }
        None => {
            // The original's own `vim_strchr` would return NULL here,
            // leaving the lengths meaningless; 'foldmarker' is always
            // validated to contain a comma before it is set, so this
            // is unreachable in a real session.
            FoldMarkers {
                end: Vec::new(),
                start_len: fmr.len(),
                end_len: 0,
            }
        }
    };
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { FOLD_MARKERS.get_mut() } = Some(markers);
}

/// Recompute folds after leaving Insert mode
/// (`foldUpdateAfterInsert`).
///
/// Skipped entirely for `'foldmethod'` `"manual"`, which needs no
/// recomputation, and for `"syntax"`/`"expr"`, which the original
/// deems too slow to run automatically on every insert-leave.
///
/// # Safety
/// Same as [`fold_open_cursor`]; also touches `GLOBALS.curwin`.
pub unsafe fn fold_update_after_insert() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &*curwin };

    if foldmethod_is_manual(win) || foldmethod_is_syntax(win) || foldmethod_is_expr(win) {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { fold_update_all(curwin) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { fold_open_cursor() };
}

/// Apply a changed `'foldlevel'` to the current window
/// (`newFoldLevel`).
///
/// # Deferred boundary
/// The original also propagates the level to other diff-mode windows,
/// which needs the `'scrollbind'` window option (`w_p_scb`), not yet
/// a `WinT` field - the same gap [`set_manual_fold`] documents. The
/// guard reaching it, `foldmethodIsDiff(curwin)`, is real and
/// unreachable today, since `wo_fdm` is `None` for every window this
/// crate can build.
///
/// # Safety
/// Same as [`new_fold_level_win`]; also touches `GLOBALS.curwin`.
pub unsafe fn new_fold_level() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { new_fold_level_win(curwin) };

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { foldmethod_is_diff(&*curwin) } {
        unimplemented!(
            "fold::new_fold_level: propagating 'foldlevel' to other diff-mode windows needs \
             the 'scrollbind' window option (w_p_scb), not yet a WinT field - unreachable \
             while 'foldmethod' can never be \"diff\""
        );
    }
}

/// State passed to the per-`'foldmethod'` fold-level functions
/// (`fline_T`, defined in `fold.c` itself, so kept with its own `.c`
/// file's translation).
///
/// Each `foldlevel*` function reads the line named by `lnum + off`
/// and writes its answer into `lvl`, sometimes also setting `start`,
/// `end` or `lvl_next` for the fold-update walk that drives them.
#[derive(Debug, Clone, Default)]
pub struct FlineT {
    /// Window whose fold settings apply.
    pub wp: *mut WinT,
    /// Current line number, relative to `off`.
    pub lnum: crate::pos_defs::LinenrT,
    /// Offset between `lnum` and the real line number.
    pub off: crate::pos_defs::LinenrT,
    /// Line number used by `foldUpdateIEMSRecurse`.
    pub lnum_save: crate::pos_defs::LinenrT,
    /// Current level, `-1` when undefined.
    pub lvl: i32,
    /// Level used for the next line.
    pub lvl_next: i32,
    /// Number of folds forced to start at this line.
    pub start: i32,
    /// Level of the fold forced to end below this line.
    pub end: i32,
    /// Level of the fold forced to end above this line (a copy of the
    /// previous line's `end`).
    pub had_end: i32,
}

/// Fold level for the `"indent"` method (`foldlevelIndent`).
///
/// The level is the line's indent divided by `'shiftwidth'`, capped
/// at `'foldnestmax'`. An empty line, or one starting with a
/// character in `'foldignore'`, has no level of its own and takes
/// `-1` so the surrounding lines decide - except at the very first
/// and last lines of the buffer, which cannot be left undefined.
///
/// # Safety
/// `flp.wp` must be a valid, live [`WinT`] whose `w_buffer` is a
/// valid, live `BufT`; also forwarded from
/// [`crate::indent::get_indent_buf`]'s own safety doc.
pub unsafe fn foldlevel_indent(flp: &mut FlineT) {
    let lnum = flp.lnum + flp.off;
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { (*flp.wp).w_buffer };
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get_buf(&mut *buf, lnum) };
    let first = line.get(crate::charset::skipwhite(&line)).copied().unwrap_or(0);

    // SAFETY: forwarded from this function's own safety doc.
    let fdi_has = unsafe { (*flp.wp).w_onebuf_opt.wo_fdi.as_deref() }
        .is_some_and(|fdi| first != 0 && fdi.contains(&first));

    if first == 0 || fdi_has {
        // The first and last line can't be undefined, use level 0.
        // SAFETY: forwarded from this function's own safety doc.
        let line_count = unsafe { (*buf).b_ml.ml_line_count };
        flp.lvl = if lnum == 1 || lnum == line_count { 0 } else { -1 };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let indent = unsafe { crate::indent::get_indent_buf(&mut *buf, lnum) };
        // SAFETY: forwarded from this function's own safety doc.
        let sw = unsafe { crate::indent::get_sw_value(&*buf) };
        flp.lvl = indent / sw;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let fdn = unsafe { (*flp.wp).w_onebuf_opt.wo_fdn };
    flp.lvl = flp.lvl.min(0.max(fdn as i32));
}

/// Fold level for the `"diff"` method (`foldlevelDiff`).
///
/// Lines that diff mode folds away are level 1; everything else is
/// level 0, so a diff view collapses to just the changed regions.
///
/// # Safety
/// Same as [`crate::diff::diff_infold`]; `flp.wp` must be a valid,
/// live [`WinT`].
pub unsafe fn foldlevel_diff(flp: &mut FlineT) {
    // SAFETY: forwarded from this function's own safety doc.
    let infold = unsafe { crate::diff::diff_infold(&*flp.wp, flp.lnum + flp.off) };
    flp.lvl = i32::from(infold);
}

/// Fold level for the `"marker"` method (`foldlevelMarker`).
///
/// Scans the line for `'foldmarker'`'s start and end markers. A bare
/// start marker raises the level by one; one followed by a number
/// sets the level to that number outright, and likewise for the end
/// marker, which lowers the level for the *next* line rather than
/// this one. A line may contain several markers, so the scan
/// continues to the end.
///
/// Requires a preceding [`parse_marker`] call, and expects `flp.lvl`
/// to hold the previous line's level - which is why the original
/// warns it cannot be called twice on the same line.
///
/// # Safety
/// `flp.wp` must be a valid, live [`WinT`] whose `w_buffer` is a
/// valid, live `BufT`; [`FOLD_MARKERS`] must have been set by
/// [`parse_marker`].
pub unsafe fn foldlevel_marker(flp: &mut FlineT) {
    let start_lvl = flp.lvl;

    // SAFETY: forwarded from this function's own safety doc.
    let markers = unsafe { FOLD_MARKERS.get_mut() }
        .clone()
        .expect("foldlevel_marker: parse_marker must run first");
    // SAFETY: forwarded from this function's own safety doc.
    let fmr = unsafe { (*flp.wp).w_onebuf_opt.wo_fmr.clone() }.unwrap_or_default();
    let startmarker = &fmr[..markers.start_len.min(fmr.len())];
    let endmarker = &markers.end[..];

    // Default: no start found, next level is same as current level.
    flp.start = 0;
    flp.lvl_next = flp.lvl;

    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { (*flp.wp).w_buffer };
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get_buf(&mut *buf, flp.lnum + flp.off) };

    let mut i = 0usize;
    while i < line.len() && line[i] != 0 {
        let rest = &line[i..];
        if !startmarker.is_empty() && rest.starts_with(startmarker) {
            // Found startmarker: set flp.lvl.
            i += startmarker.len();
            match marker_number(&line, i) {
                Some(n) => {
                    flp.lvl = n;
                    flp.lvl_next = n;
                    flp.start = (n - start_lvl).max(1);
                }
                None => {
                    flp.lvl += 1;
                    flp.lvl_next += 1;
                    flp.start += 1;
                }
            }
        } else if !endmarker.is_empty() && rest.starts_with(endmarker) {
            // Found endmarker: set flp.lvl_next.
            i += endmarker.len();
            match marker_number(&line, i) {
                Some(n) => {
                    flp.lvl = n;
                    // Never start a fold with an end marker.
                    flp.lvl_next = (n - 1).min(start_lvl);
                }
                None => flp.lvl_next -= 1,
            }
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            i += (unsafe { crate::mbyte::utfc_ptr2len(&line[i..]) } as usize).max(1);
        }
    }

    // The level can't go negative, must be missing a start marker.
    flp.lvl_next = flp.lvl_next.max(0);
}

/// Reads the optional level number written straight after a fold
/// marker, as the original's own `ascii_isdigit`/`atoi` pair does.
///
/// Returns `None` when there is no digit there, or when the number is
/// not positive - the original ignores a zero or negative level and
/// falls back to the plain "one deeper" behaviour.
fn marker_number(line: &[u8], at: usize) -> Option<i32> {
    if !line.get(at).is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    let end = line[at..]
        .iter()
        .position(|c| !c.is_ascii_digit())
        .map_or(line.len(), |off| at + off);
    let n: i32 = std::str::from_utf8(&line[at..end]).ok()?.parse().ok()?;
    (n > 0).then_some(n)
}

/// Move the cursor to a fold boundary (`foldMoveTo`).
///
/// With `updown` false this is `[z`/`]z`: move to the start or end of
/// the fold the cursor is in, stopping at a closed fold. With
/// `updown` true it is `zj`/`zk`: move to the next or previous fold
/// at the same level.
///
/// Repeats `count` times, returning `OK` if the cursor moved at all
/// and `FAIL` otherwise. The first move sets the previous-context
/// mark, as any jump does.
///
/// # Safety
/// Same as [`check_closed`]; also touches `GLOBALS.curwin` and, on a
/// successful move, [`crate::mark::setpcmark`].
pub unsafe fn fold_move_to(updown: bool, dir: crate::vim_defs::Direction, count: i32) -> i32 {
    let mut retval = crate::vim_defs::FAIL;
    let forward = dir == crate::vim_defs::Direction::Forward;

    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    checkupdate(unsafe { &mut *curwin });

    // Repeat "count" times.
    for _ in 0..count {
        // Find nested folds. Stop when a fold is closed. The deepest
        // fold that moves the cursor is used.
        let mut lnum_off = 0;
        let mut path: Vec<usize> = Vec::new();
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { (*curwin).w_folds.is_empty() } {
            break;
        }
        let mut use_level = false;
        let mut maybe_small = false;
        // SAFETY: forwarded from this function's own safety doc.
        let cursor_lnum = unsafe { (*curwin).w_cursor.lnum };
        let mut lnum_found = cursor_lnum;
        let mut level = 0;
        let mut last = false;

        loop {
            // SAFETY: forwarded from this function's own safety doc.
            let gap_len = unsafe { fold_gap(curwin, &path) }.len();
            // SAFETY: forwarded from this function's own safety doc.
            let (found, found_idx) =
                fold_find(unsafe { fold_gap(curwin, &path) }, cursor_lnum - lnum_off);
            // The original steps this index one BEFORE the array when
            // moving forward past the last fold, so that `idx + 1`
            // still names the first candidate - hence the signed type.
            let mut idx = found_idx as i64;
            if !found {
                if !updown || gap_len == 0 {
                    break;
                }
                // When moving up, consider a fold above the cursor;
                // when moving down consider a fold below it.
                if forward {
                    if idx >= gap_len as i64 {
                        break;
                    }
                    idx -= 1;
                } else if idx == 0 {
                    break;
                }
                // Don't look for contained folds, they would always
                // move the cursor too far.
                last = true;
            }

            if !last {
                // Check if this fold is closed.
                // SAFETY: forwarded from this function's own safety doc.
                let closed = unsafe {
                    let fp = &mut fold_gap_mut(curwin, &path)[idx as usize];
                    check_closed(curwin, fp, &mut use_level, level, &mut maybe_small, lnum_off)
                };
                if closed {
                    last = true;
                }
                // "[z" and "]z" stop at a closed fold.
                if last && !updown {
                    break;
                }
            }

            // SAFETY: forwarded from this function's own safety doc.
            let gap = unsafe { fold_gap(curwin, &path) };
            if updown {
                if forward {
                    // To the start of the next fold, if there is one.
                    if idx + 1 < gap.len() as i64 {
                        let lnum = gap[(idx + 1) as usize].fd_top + lnum_off;
                        if lnum > cursor_lnum {
                            lnum_found = lnum;
                        }
                    }
                } else {
                    // To the end of the previous fold, if there is one.
                    if idx > 0 {
                        let prev = &gap[(idx - 1) as usize];
                        let lnum = prev.fd_top + lnum_off + prev.fd_len - 1;
                        if lnum < cursor_lnum {
                            lnum_found = lnum;
                        }
                    }
                }
            } else {
                // Open fold found: set the cursor to its start or end,
                // then check nested folds.
                let fp = &gap[idx as usize];
                if forward {
                    let lnum = fp.fd_top + lnum_off + fp.fd_len - 1;
                    if lnum > cursor_lnum {
                        lnum_found = lnum;
                    }
                } else {
                    let lnum = fp.fd_top + lnum_off;
                    if lnum < cursor_lnum {
                        lnum_found = lnum;
                    }
                }
            }

            if last {
                break;
            }

            // Check nested folds (if any).
            lnum_off += gap[idx as usize].fd_top;
            path.push(idx as usize);
            level += 1;
        }

        if lnum_found != cursor_lnum {
            if retval == crate::vim_defs::FAIL {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::mark::setpcmark() };
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                (*curwin).w_cursor.lnum = lnum_found;
                (*curwin).w_cursor.col = 0;
            }
            retval = crate::vim_defs::OK;
        } else {
            break;
        }
    }

    retval
}

/// Resolve an index path to the fold array it names, starting from
/// `wp.w_folds`.
///
/// # Safety
/// `wp` must be a valid, live [`WinT`], and `path` must name folds
/// that exist.
unsafe fn fold_gap<'a>(wp: *mut WinT, path: &[usize]) -> &'a [FoldT] {
    // SAFETY: forwarded from this function's own safety doc.
    let mut gap: &[FoldT] = unsafe { &(*wp).w_folds };
    for idx in path {
        gap = &gap[*idx].fd_nested;
    }
    gap
}

/// Mutable counterpart of [`fold_gap`].
///
/// # Safety
/// Same as [`fold_gap`].
unsafe fn fold_gap_mut<'a>(wp: *mut WinT, path: &[usize]) -> &'a mut Vec<FoldT> {
    // SAFETY: forwarded from this function's own safety doc.
    let mut gap: &mut Vec<FoldT> = unsafe { &mut (*wp).w_folds };
    for idx in path {
        gap = &mut gap[*idx].fd_nested;
    }
    gap
}

/// Move the folds covering lines `line1` to `line2` to after `dest`
/// (`foldMoveRange`), requiring `line1 <= line2 <= dest`.
///
/// The original enumerates ten cases by how each fold overlaps the
/// moved range and the destination; they are preserved here with the
/// same numbering in the comments. Folds straddling a boundary are
/// truncated or have their nested folds adjusted first.
///
/// The three reversals at the end rotate the moved folds back into
/// sorted order: reversing `[move_start, dest_index)` as a whole and
/// then each of its two parts is the standard block-swap idiom, which
/// is exactly what the original does.
///
/// # Safety
/// Same as [`truncate_fold`]; reads `GLOBALS.State` once for the
/// Insert-mode test its nested adjustments need.
pub unsafe fn fold_move_range(
    gap: &mut [FoldT],
    line1: crate::pos_defs::LinenrT,
    line2: crate::pos_defs::LinenrT,
    dest: crate::pos_defs::LinenrT,
) {
    let range_len = line2 - line1 + 1;
    let move_len = dest - line2;
    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
    let insert_mode = (state & crate::state_defs::mode::INSERT as i32) != 0;

    let fold_end = |fp: &FoldT| fp.fd_top + fp.fd_len - 1;

    let (at_start, mut idx) = fold_find(gap, line1 - 1);
    if at_start {
        let fd_top = gap[idx].fd_top;
        if fold_end(&gap[idx]) > dest {
            // Case 4 -- don't have to change this fold, but have to
            // move nested folds.
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                fold_move_range(
                    &mut gap[idx].fd_nested,
                    line1 - fd_top,
                    line2 - fd_top,
                    dest - fd_top,
                )
            };
            return;
        } else if fold_end(&gap[idx]) > line2 {
            // Case 3 -- remove nested folds between line1 and line2
            // and reduce the length of the fold by range_len. Folds
            // after this one must be dealt with.
            fold_mark_adjust_recurse(
                &mut gap[idx].fd_nested,
                line1 - fd_top,
                line2 - fd_top,
                crate::pos_defs::MAXLNUM,
                -range_len,
                insert_mode,
            );
            gap[idx].fd_len -= range_len;
        } else {
            // Case 2 -- truncate the fold *above* line1. Folds after
            // this one must be dealt with.
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { truncate_fold(&mut gap[idx], line1 - 1) };
        }
        // Look at the next fold, and treat that one as if it were the
        // first after line1 (because now it is).
        idx += 1;
    }

    if idx >= gap.len() || gap[idx].fd_top > dest {
        // No folds after line1 and before dest. Case 10.
        return;
    } else if gap[idx].fd_top > line2 {
        while idx < gap.len() && fold_end(&gap[idx]) <= dest {
            // Case 9 (for all case 9s) -- shift up.
            gap[idx].fd_top -= range_len;
            idx += 1;
        }
        if idx < gap.len() && gap[idx].fd_top <= dest {
            // Case 8 -- ensure truncated at dest, shift up.
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { truncate_fold(&mut gap[idx], dest) };
            gap[idx].fd_top -= range_len;
        }
        return;
    } else if fold_end(&gap[idx]) > dest {
        // Case 7 -- remove nested folds and shrink.
        let fd_top = gap[idx].fd_top;
        fold_mark_adjust_recurse(
            &mut gap[idx].fd_nested,
            line2 + 1 - fd_top,
            dest - fd_top,
            crate::pos_defs::MAXLNUM,
            -move_len,
            insert_mode,
        );
        gap[idx].fd_len -= move_len;
        gap[idx].fd_top += move_len;
        return;
    }

    // Case 5 or 6: what changes depends on whether there are folds
    // between the end of this fold and dest.
    let move_start = idx;
    let mut move_end = 0usize;
    while idx < gap.len() && gap[idx].fd_top <= dest {
        if gap[idx].fd_top <= line2 {
            // 5, or 6.
            if fold_end(&gap[idx]) > line2 {
                // 6, truncate before moving.
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { truncate_fold(&mut gap[idx], line2) };
            }
            gap[idx].fd_top += move_len;
            idx += 1;
            continue;
        }

        // Record the index of the first fold after the moved range.
        if move_end == 0 {
            move_end = idx;
        }

        if fold_end(&gap[idx]) > dest {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { truncate_fold(&mut gap[idx], dest) };
        }

        gap[idx].fd_top -= range_len;
        idx += 1;
    }
    let dest_index = idx;

    // All folds are now correct, but not necessarily in the correct
    // order: swap the folds in [move_end, dest_index) with those in
    // [move_start, move_end).
    if move_end == 0 {
        // There are no folds after those moved, so none were moved
        // out of order.
        return;
    }
    fold_reverse_order(gap, move_start, dest_index - 1);
    fold_reverse_order(gap, move_start, move_start + dest_index - move_end - 1);
    fold_reverse_order(gap, move_start + dest_index - move_end, dest_index - 1);
}

/// Create a fold covering `start` to `end` in window `wp`
/// (`foldCreate`).
///
/// Existing folds that fall inside the new one are moved into it as
/// nested folds, with their line numbers rebased; if the first of
/// them starts above the new fold, or the last ends below it, the new
/// fold is widened rather than changing them. The new fold is created
/// closed.
///
/// # Deferred boundary
/// With `'foldmethod'` set to `"marker"` the original writes the fold
/// markers into the buffer instead, which needs `foldCreateMarkers`
/// and the `ml_replace_buf` text-editing path, not yet translated.
/// The guard reaching it is real.
///
/// # Safety
/// Same as [`close_fold`]; `wp` must be a valid, live [`WinT`].
pub unsafe fn fold_create(
    wp: *mut WinT,
    start: crate::pos_defs::PosT,
    end: crate::pos_defs::PosT,
) {
    let mut use_level = false;
    let mut closed = false;
    let mut level = 0;

    // Reverse the range when it was given backwards.
    let (start, end) = if start.lnum > end.lnum { (end, start) } else { (start, end) };
    let mut start_rel = start;
    let mut end_rel = end;

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { foldmethod_is_marker(&*wp) } {
        unimplemented!(
            "fold::fold_create: 'foldmethod'=marker writes markers into the buffer via \
             foldCreateMarkers, which needs the ml_replace_buf text-editing path, not yet \
             translated"
        );
    }

    // SAFETY: forwarded from this function's own safety doc.
    checkupdate(unsafe { &mut *wp });
    // SAFETY: forwarded from this function's own safety doc.
    let fdl = unsafe { (*wp).w_onebuf_opt.wo_fdl };

    // Find the place to insert the new fold.
    let mut path: Vec<usize> = Vec::new();
    let mut i = 0usize;
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { fold_gap(wp, &path) }.is_empty() {
        loop {
            // SAFETY: forwarded from this function's own safety doc.
            let gap = unsafe { fold_gap(wp, &path) };
            let (found, idx) = fold_find(gap, start_rel.lnum);
            if !found {
                i = idx;
                break;
            }
            let fp = &gap[idx];
            if fp.fd_top + fp.fd_len > end_rel.lnum {
                // The new fold is completely inside this one: go one
                // level deeper.
                let fd_top = fp.fd_top;
                let (fd_flags, _) = (fp.fd_flags, ());
                start_rel.lnum -= fd_top;
                end_rel.lnum -= fd_top;
                if use_level || fd_flags == fd_flags::FD_LEVEL {
                    use_level = true;
                    if crate::types_defs::OptInt::from(level) >= fdl {
                        closed = true;
                    }
                } else if fd_flags == fd_flags::FD_CLOSED {
                    closed = true;
                }
                level += 1;
                path.push(idx);
            } else {
                // This fold and the new fold overlap: insert here and
                // move some folds inside the new fold.
                i = idx;
                break;
            }
        }
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { fold_gap(wp, &path) }.is_empty() {
            i = 0;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    let gap = unsafe { fold_gap_mut(wp, &path) };

    // Count the folds that will be contained in the new fold.
    let mut cont = 0usize;
    while i + cont < gap.len() && gap[i + cont].fd_top <= end_rel.lnum {
        cont += 1;
    }

    let mut nested: Vec<FoldT> = Vec::new();
    if cont > 0 {
        // If the first contained fold starts before the new fold, let
        // the new fold start there instead; otherwise that fold would
        // change. Likewise widen the end for a partly-contained last
        // fold.
        start_rel.lnum = start_rel.lnum.min(gap[i].fd_top);
        let last = &gap[i + cont - 1];
        end_rel.lnum = end_rel.lnum.max(last.fd_top + last.fd_len - 1);

        // Move the contained folds inside the new fold, rebasing them
        // onto it.
        nested = gap.drain(i..i + cont).collect();
        for fp in &mut nested {
            fp.fd_top -= start_rel.lnum;
        }
    }

    // Insert the new fold.
    gap.insert(
        i,
        FoldT {
            fd_top: start_rel.lnum,
            fd_len: end_rel.lnum - start_rel.lnum + 1,
            fd_nested: nested,
            fd_flags: fd_flags::FD_CLOSED,
            fd_small: crate::types_defs::TriState::None,
        },
    );

    // The new fold should be closed. If it would stay open because of
    // 'foldlevel', adjust the containing folds' flags.
    if use_level && !closed && crate::types_defs::OptInt::from(level) < fdl {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { close_fold(start, 1) };
    }
    if !use_level {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*wp).w_fold_manual = true };
    }

    // Redraw.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::r#move::changed_window_setting(wp) };
}

/// Delete the folds covering lines `start` to `end` in window `wp`
/// (`deleteFold`).
///
/// For each line in the range the deepest fold containing it is
/// removed, then the scan resumes past that fold. With `recursive`
/// set, folds nested inside a deleted one go too; otherwise they are
/// promoted a level, as [`delete_fold_entry`] describes. A line
/// inside a *closed* fold deletes that fold rather than looking
/// deeper.
///
/// # Deferred boundary
/// When `'foldmethod'` is not `"manual"` the folds live as markers in
/// the buffer text, so deleting one means editing the buffer via
/// `deleteFoldMarkers`, which needs the `ml_replace_buf` path, not
/// yet translated. The guard reaching it is real. The `changed_lines`
/// and buffer-update notification at the end are likewise only
/// reachable from that branch, since only it records the touched line
/// range.
///
/// # Safety
/// Same as [`check_closed`]; `wp` must be a valid, live [`WinT`].
pub unsafe fn delete_fold(
    wp: *mut WinT,
    start: crate::pos_defs::LinenrT,
    end: crate::pos_defs::LinenrT,
    recursive: bool,
    had_visual: bool,
) {
    let mut maybe_small = false;
    let mut lnum = start;
    let mut did_one = false;

    // SAFETY: forwarded from this function's own safety doc.
    checkupdate(unsafe { &mut *wp });
    // SAFETY: forwarded from this function's own safety doc.
    let is_manual = unsafe { foldmethod_is_manual(&*wp) };

    while lnum <= end {
        // Find the deepest fold for "lnum".
        let mut path: Vec<usize> = Vec::new();
        let mut found: Option<(Vec<usize>, usize, crate::pos_defs::LinenrT)> = None;
        let mut lnum_off = 0;
        let mut use_level = false;
        let mut level = 0;

        loop {
            // SAFETY: forwarded from this function's own safety doc.
            let (hit, idx) = fold_find(unsafe { fold_gap(wp, &path) }, lnum - lnum_off);
            if !hit {
                break;
            }
            // lnum is inside this fold, remember the info.
            found = Some((path.clone(), idx, lnum_off));

            // If "lnum" is folded, don't check nesting.
            // SAFETY: forwarded from this function's own safety doc.
            let closed = unsafe {
                let fp = &mut fold_gap_mut(wp, &path)[idx];
                check_closed(wp, fp, &mut use_level, level, &mut maybe_small, lnum_off)
            };
            if closed {
                break;
            }

            // Check nested folds.
            // SAFETY: forwarded from this function's own safety doc.
            lnum_off += unsafe { fold_gap(wp, &path) }[idx].fd_top;
            path.push(idx);
            level += 1;
        }

        match found {
            None => lnum += 1,
            Some((fpath, idx, off)) => {
                // SAFETY: forwarded from this function's own safety doc.
                let fp = &unsafe { fold_gap(wp, &fpath) }[idx];
                lnum = fp.fd_top + fp.fd_len + off;

                if is_manual {
                    // SAFETY: forwarded from this function's own safety doc.
                    delete_fold_entry(unsafe { fold_gap_mut(wp, &fpath) }, idx, recursive);
                } else {
                    unimplemented!(
                        "fold::delete_fold: deleting marker-based folds edits the buffer via \
                         deleteFoldMarkers, which needs the ml_replace_buf path, not yet \
                         translated"
                    );
                }
                did_one = true;

                // Redraw the window.
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::r#move::changed_window_setting(wp) };
            }
        }
    }

    if !did_one {
        // The original's `emsg(e_nofold)` is omitted under this
        // crate's established message-display policy.
        // Force a redraw to remove the Visual highlighting.
        if had_visual {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                crate::drawscreen::redraw_buf_later(
                    (*wp).w_buffer,
                    crate::drawscreen::UPD_INVERTED,
                )
            };
        }
    } else {
        // Deleting markers may make the cursor column invalid.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::cursor::check_cursor_col(wp) };
    }
}

/// Delete a fold marker from the end of line `lnum`, along with the
/// `'commentstring'` wrapping it when that matches (`foldDelMarker`).
///
/// A digit written straight after the marker (the explicit fold level)
/// is removed with it. When the marker is not found nothing happens
/// and no error is reported - it may simply be a missing close
/// marker, which the original tolerates.
///
/// # Safety
/// `buf` must be a valid, live `BufT` with a memline; also forwarded
/// from [`crate::undo::u_save`] and
/// [`crate::extmark::extmark_splice_cols`].
pub unsafe fn fold_del_marker(
    buf: *mut crate::buffer_defs::BufT,
    lnum: crate::pos_defs::LinenrT,
    marker: &[u8],
) {
    // The end marker may be missing and the fold extend below the
    // last line.
    // SAFETY: forwarded from this function's own safety doc.
    if lnum > unsafe { (*buf).b_ml.ml_line_count } {
        return;
    }
    if marker.is_empty() {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let cms = unsafe { (*buf).b_p_cms.clone() }.unwrap_or_default();
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get_buf(&mut *buf, lnum) };

    let mut p = 0usize;
    while p < line.len() && line[p] != 0 {
        if !line[p..].starts_with(marker) {
            p += 1;
            continue;
        }

        // Found the marker, include a digit if it's there.
        let mut len = marker.len();
        if line.get(p + len).is_some_and(u8::is_ascii_digit) {
            len += 1;
        }

        let mut start = p;
        if !cms.is_empty() && cms[0] != 0 {
            // Also delete 'commentstring' if it matches. It is split
            // at "%s" into the part before and after the marker.
            if let Some(pct) = cms.windows(2).position(|w| w == b"%s") {
                let (before, after) = (&cms[..pct], &cms[pct + 2..]);
                let after_end = after.iter().position(|&c| c == 0).unwrap_or(after.len());
                let after = &after[..after_end];
                if p >= before.len()
                    && line[p - before.len()..p] == *before
                    && line[p + len..].starts_with(after)
                {
                    start = p - before.len();
                    len += before.len() + after.len();
                }
            }
        }

        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::undo::u_save(lnum - 1, lnum + 1) } == crate::vim_defs::OK {
            // Make the new line: text before the marker, then the
            // text after it.
            let mut newline = line[..start].to_vec();
            newline.extend_from_slice(&line[start + len..]);
            // SAFETY: forwarded from this function's own safety doc.
            let _ = unsafe { crate::memline::ml_replace_buf_len(&mut *buf, lnum, &newline) };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                crate::extmark::extmark_splice_cols(
                    &mut *buf,
                    lnum - 1,
                    start as crate::pos_defs::ColnrT,
                    len as crate::pos_defs::ColnrT,
                    0,
                    crate::extmark_defs::ExtmarkOp::Undo,
                )
            };
        }
        break;
    }
}

/// Delete the start and end markers of fold `fp`, and of every fold
/// nested inside it when `recursive` is set (`deleteFoldMarkers`).
///
/// `lnum_off` offsets `fp.fd_top`, since a nested fold stores it
/// relative to its parent; the recursion accumulates it.
///
/// Requires a preceding [`parse_marker`] call, whose result names
/// both markers.
///
/// # Safety
/// Same as [`fold_del_marker`]; `wp` must be a valid, live [`WinT`].
pub unsafe fn delete_fold_markers(
    wp: *mut WinT,
    fp: &mut FoldT,
    recursive: bool,
    lnum_off: crate::pos_defs::LinenrT,
) {
    if recursive {
        let fd_top = fp.fd_top;
        for nested in &mut fp.fd_nested {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { delete_fold_markers(wp, nested, true, lnum_off + fd_top) };
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    let markers = unsafe { FOLD_MARKERS.get_mut() }
        .clone()
        .expect("delete_fold_markers: parse_marker must run first");
    // SAFETY: forwarded from this function's own safety doc.
    let fmr = unsafe { (*wp).w_onebuf_opt.wo_fmr.clone() }.unwrap_or_default();
    let startmarker = fmr[..markers.start_len.min(fmr.len())].to_vec();

    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { (*wp).w_buffer };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { fold_del_marker(buf, fp.fd_top + lnum_off, &startmarker) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        fold_del_marker(buf, fp.fd_top + lnum_off + fp.fd_len - 1, &markers.end)
    };
}

/// Append a fold marker to the end of line `lnum`, wrapping it in
/// `'commentstring'` when that is set and the line is not already
/// inside an unclosed comment (`foldAddMarker`).
///
/// # Safety
/// Same as [`fold_del_marker`].
pub unsafe fn fold_add_marker(
    buf: *mut crate::buffer_defs::BufT,
    lnum: crate::pos_defs::LinenrT,
    marker: &[u8],
) {
    // SAFETY: forwarded from this function's own safety doc.
    let cms = unsafe { (*buf).b_p_cms.clone() }.unwrap_or_default();
    let cms_end = cms.iter().position(|&c| c == 0).unwrap_or(cms.len());
    let cms = &cms[..cms_end];
    let pct = cms.windows(2).position(|w| w == b"%s");

    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get_buf(&mut *buf, lnum) };
    // ml_get_buf hands back the trailing NUL; the marker goes before
    // it, so measure the text without it.
    let line_len = line.iter().position(|&c| c == 0).unwrap_or(line.len());

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::undo::u_save(lnum - 1, lnum + 1) } != crate::vim_defs::OK {
        return;
    }

    // Check whether the line ends with an unclosed comment.
    // SAFETY: forwarded from this function's own safety doc.
    let (_, line_is_comment) = unsafe { crate::ops::skip_comment(&line[..line_len], false, false) };

    let mut newline = line[..line_len].to_vec();
    let added = match pct {
        // Append the marker to the end of the line.
        None => {
            newline.extend_from_slice(marker);
            marker.len()
        }
        Some(_) if line_is_comment => {
            newline.extend_from_slice(marker);
            marker.len()
        }
        Some(p) => {
            // Wrap the marker in 'commentstring', in place of its
            // "%s".
            newline.extend_from_slice(&cms[..p]);
            newline.extend_from_slice(marker);
            newline.extend_from_slice(&cms[p + 2..]);
            marker.len() + cms.len() - 2
        }
    };
    newline.push(0);

    // SAFETY: forwarded from this function's own safety doc.
    let _ = unsafe { crate::memline::ml_replace_buf_len(&mut *buf, lnum, &newline) };
    if added != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            crate::extmark::extmark_splice_cols(
                &mut *buf,
                lnum - 1,
                line_len as crate::pos_defs::ColnrT,
                0,
                added as crate::pos_defs::ColnrT,
                crate::extmark_defs::ExtmarkOp::Undo,
            )
        };
    }
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

    /// A window with `'foldmethod'` left at its real default. `wo_fdm`
    /// is `None` for every window this crate can build, which is what
    /// makes `fold_update`'s dispatch branch unreachable.
    fn win_default_fdm() -> WinT {
        WinT::default()
    }

    /// Save/restore the globals `fold_update` reads, so these tests
    /// cannot leak state into others.
    struct FoldUpdateGuard {
        prev_disable: i32,
        prev_need_redraw: bool,
        prev_state: i32,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl FoldUpdateGuard {
        fn set() -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = FoldUpdateGuard {
                prev_disable: *unsafe { DISABLE_FOLD_UPDATE.get_mut() },
                prev_need_redraw: *unsafe { crate::diff::NEED_DIFF_REDRAW.get_mut() },
                prev_state: g.State,
                _lock,
            };
            unsafe { *DISABLE_FOLD_UPDATE.get_mut() = 0 };
            unsafe { *crate::diff::NEED_DIFF_REDRAW.get_mut() = false };
            g.State = crate::state_defs::mode::NORMAL as i32;
            guard
        }
    }

    impl Drop for FoldUpdateGuard {
        fn drop(&mut self) {
            unsafe { *DISABLE_FOLD_UPDATE.get_mut() = self.prev_disable };
            unsafe { *crate::diff::NEED_DIFF_REDRAW.get_mut() = self.prev_need_redraw };
            unsafe { crate::globals::GLOBALS.get_mut() }.State = self.prev_state;
        }
    }

    #[test]
    fn fold_update_is_a_noop_with_the_default_foldmethod() {
        // 'foldmethod' defaults to "manual" and no fold exists, so
        // every dispatch predicate is false and the call completes
        // without reaching either deferred branch.
        let _guard = FoldUpdateGuard::set();
        let mut win = win_default_fdm();
        unsafe { fold_update(&mut win, 1, 10) };
    }

    #[test]
    fn fold_update_returns_early_when_disabled() {
        let _guard = FoldUpdateGuard::set();
        unsafe { *DISABLE_FOLD_UPDATE.get_mut() = 1 };
        // "indent" would otherwise reach the deferred dispatch.
        let mut win = win_with_fdm(b"indent");
        unsafe { fold_update(&mut win, 1, 10) };
    }

    #[test]
    fn fold_update_returns_early_in_insert_mode_for_non_indent_methods() {
        let _guard = FoldUpdateGuard::set();
        unsafe { crate::globals::GLOBALS.get_mut() }.State =
            crate::state_defs::mode::INSERT as i32;
        // "marker" would otherwise reach the deferred dispatch, but
        // Insert mode skips every method except "indent".
        let mut win = win_with_fdm(b"marker");
        unsafe { fold_update(&mut win, 1, 10) };
    }

    #[test]
    fn fold_update_returns_early_when_a_diff_redraw_is_pending() {
        let _guard = FoldUpdateGuard::set();
        unsafe { *crate::diff::NEED_DIFF_REDRAW.get_mut() = true };
        let mut win = win_with_fdm(b"expr");
        unsafe { fold_update(&mut win, 1, 10) };
    }

    #[test]
    #[should_panic(expected = "foldUpdateIEMS")]
    fn fold_update_dispatch_is_unreachable_but_documented() {
        // Proves the dispatch guard really is driven by 'foldmethod'
        // rather than hardcoded away: forcing a value nothing in this
        // crate can currently produce reaches the deferred branch.
        let _guard = FoldUpdateGuard::set();
        let mut win = win_with_fdm(b"indent");
        unsafe { fold_update(&mut win, 1, 10) };
    }

    #[test]
    fn foldmethod_is_expr_true_only_for_expr() {        assert!(foldmethod_is_expr(&win_with_fdm(b"expr")));
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
    fn has_folding_win_reports_nothing_for_a_window_with_no_actual_folds() {
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
        // hasAnyFolding is true, so the search really runs - but
        // w_folds is empty, so it finds nothing.
        let mut info = crate::fold_defs::FoldinfoT::default();
        assert!(!unsafe { has_folding_win(&mut win, 1, None, None, true, Some(&mut info)) });
        assert_eq!(info.fi_level, 0);
        // fi_lnum is `lnum - lnum_rel`, and with no fold found
        // lnum_rel never moves off lnum, so this is 0.
        assert_eq!(info.fi_lnum, 0);
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
        // has_any_folding's fast path returns 0 before lnum is ever
        // used, so any value yields the same result.
        assert_eq!(unsafe { fold_level(&mut win, 9999) }, 0);
    }

    #[test]
    fn fold_level_reports_real_levels_once_folds_exist() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                ..Default::default()
            },
            w_foldinvalid: false,
            // Same layout cross-verified against real nvim below.
            w_folds: nested_outer_inner(),
            ..Default::default()
        };
        // Past has_any_folding now that w_folds is non-empty, so this
        // exercises the real fold_level_win descent.
        assert_eq!(unsafe { fold_level(&mut win, 5) }, 0);
        assert_eq!(unsafe { fold_level(&mut win, 12) }, 1);
        assert_eq!(unsafe { fold_level(&mut win, 16) }, 2);
        assert_eq!(unsafe { fold_level(&mut win, 25) }, 0);
    }

    #[test]
    fn fold_level_is_zero_when_foldenable_is_off_even_with_folds() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                // 'nofoldenable' makes has_any_folding false, so the
                // fast path wins before any descent happens.
                wo_fen: 0,
                wo_fdm: Some(b"manual".to_vec()),
                ..Default::default()
            },
            w_foldinvalid: false,
            w_folds: nested_outer_inner(),
            ..Default::default()
        };
        assert_eq!(unsafe { fold_level(&mut win, 16) }, 0);
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
    fn get_deepest_nesting_counts_the_deepest_chain() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_foldinvalid: false,
            // A single childless fold is one level deep.
            w_folds: vec![crate::fold::FoldT::default()],
            ..Default::default()
        };
        assert_eq!(unsafe { get_deepest_nesting(&mut win) }, 1);

        // nested_fold_tree's deepest chain is outer -> second nested
        // -> its own child, i.e. three levels.
        win.w_folds = nested_fold_tree();
        assert_eq!(unsafe { get_deepest_nesting(&mut win) }, 3);
    }

    #[test]
    fn get_deepest_nesting_recurse_measures_each_branch_independently() {
        // A shallow branch alongside a deep one: the deepest wins,
        // matching the original's own MAX over every sibling.
        let gap = vec![
            FoldT::default(),
            FoldT {
                fd_nested: vec![FoldT {
                    fd_nested: vec![FoldT::default()],
                    ..Default::default()
                }],
                ..Default::default()
            },
        ];
        assert_eq!(get_deepest_nesting_recurse(&gap), 3);
        assert_eq!(get_deepest_nesting_recurse(&[]), 0);
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
    fn fold_adjust_cursor_moves_the_cursor_to_the_start_of_a_closed_fold() {
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fml: 0,
                wo_fdl: 99,
                ..Default::default()
            },
            w_foldinvalid: false,
            w_folds: vec![FoldT {
                fd_top: 20,
                fd_len: 5,
                fd_flags: fd_flags::FD_CLOSED,
                fd_small: crate::types_defs::TriState::False,
                ..Default::default()
            }],
            w_cursor: crate::pos_defs::PosT { lnum: 22, col: 0, coladd: 0 },
            ..Default::default()
        };

        unsafe { fold_adjust_cursor(&mut win) };
        assert_eq!(win.w_cursor.lnum, 20, "moved to the fold's first line");

        // A line outside any fold is left alone.
        win.w_cursor.lnum = 30;
        unsafe { fold_adjust_cursor(&mut win) };
        assert_eq!(win.w_cursor.lnum, 30);
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
        let mut gap: Vec<FoldT> = Vec::new();
        fold_mark_adjust_recurse(&mut gap, 1, 5, 2, 0, false); // must not panic
        assert!(gap.is_empty());
    }

    #[test]
    fn fold_mark_adjust_recurse_leaves_folds_above_the_range_alone() {
        // Case 1: a fold wholly above line1 is untouched.
        let mut gap = sibling_folds();
        fold_mark_adjust_recurse(&mut gap, 25, 28, 3, 0, false);
        assert_eq!(gap[0].fd_top, 10);
        assert_eq!(gap[1].fd_top, 20);
        // Case 4: the fold at 30 is wholly inside nothing here, but
        // it IS below line2, so it moves by amount_after (0 = stays).
        assert_eq!(gap[2].fd_top, 30);
    }

    #[test]
    fn fold_mark_adjust_recurse_shifts_a_fold_inside_the_range() {
        // Case 4: fold 20-24 lies entirely within 18..26, so it moves
        // by amount.
        let mut gap = sibling_folds();
        fold_mark_adjust_recurse(&mut gap, 18, 26, 3, 0, false);
        assert_eq!(gap[1].fd_top, 23);
        assert_eq!(gap[1].fd_len, 5, "its length is unchanged");
    }

    #[test]
    fn fold_mark_adjust_recurse_deletes_a_fold_whose_lines_are_all_gone() {
        // amount == MAXLNUM means the lines are being deleted, so a
        // fold contained entirely in the range goes with them.
        let mut gap = sibling_folds();
        fold_mark_adjust_recurse(&mut gap, 18, 26, crate::pos_defs::MAXLNUM, 0, false);
        assert_eq!(gap.len(), 2);
        let tops: Vec<_> = gap.iter().map(|f| f.fd_top).collect();
        assert_eq!(tops, vec![10, 30], "only the middle fold is removed");
    }

    #[test]
    fn fold_mark_adjust_recurse_deletes_several_adjacent_folds() {
        // Deleting a range covering two folds must not skip the
        // second: removing one shifts the next into the same slot.
        let mut gap = sibling_folds();
        fold_mark_adjust_recurse(&mut gap, 8, 26, crate::pos_defs::MAXLNUM, 0, false);
        assert_eq!(gap.len(), 1);
        assert_eq!(gap[0].fd_top, 30);
    }

    #[test]
    fn fold_mark_adjust_recurse_moves_folds_below_the_range_by_amount_after() {
        // Case 6: fold 30-34 is below line2, so only amount_after
        // applies to it.
        let mut gap = sibling_folds();
        fold_mark_adjust_recurse(&mut gap, 18, 26, 3, 7, false);
        assert_eq!(gap[2].fd_top, 37);
    }

    #[test]
    fn fold_mark_adjust_recurse_grows_a_fold_that_contains_the_range() {
        // Case 3: the fold contains both line1 and line2, so it keeps
        // its top and grows by amount_after.
        let mut gap = vec![FoldT {
            fd_top: 10,
            fd_len: 20,
            ..Default::default()
        }];
        fold_mark_adjust_recurse(&mut gap, 15, 18, 0, 4, false);
        assert_eq!(gap[0].fd_top, 10, "the fold's start is unaffected");
        assert_eq!(gap[0].fd_len, 24);
    }

    #[test]
    fn fold_mark_adjust_is_a_no_op_when_win_has_no_folds() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { w_folds: Vec::new(), ..Default::default() };
        // A representative spread of amount/line1/line2/amount_after
        // combinations, covering every branch of the internal
        // effective-range computation - none should panic, since
        // `w_folds` stays empty throughout.
        unsafe { fold_mark_adjust(&mut win, 5, 8, 2, 0) };
        unsafe { fold_mark_adjust(&mut win, 20, 15, 3, 0) };
        unsafe { fold_mark_adjust(&mut win, 10, 20, crate::pos_defs::MAXLNUM, -5) };
        unsafe { fold_mark_adjust(&mut win, 5, crate::pos_defs::MAXLNUM, 1, 0) };
        assert!(win.w_folds.is_empty());
    }

    #[test]
    fn fold_mark_adjust_recurse_matches_real_nvim_line_deletion() {
        // Cross-verified against real nvim: with folds at 10-14,
        // 20-24 and 30-34, deleting lines 18-26 leaves foldlevel()
        // reporting 1 at lines 10-14 (unmoved), 0 at line 18, and 1 at
        // lines 21-25 - i.e. the wholly-contained middle fold is gone
        // and the last fold has shifted down by the 9 deleted lines.
        let mut gap = sibling_folds();
        // mark_adjust's own convention for a deletion of lines 18..26:
        // amount = MAXLNUM, amount_after = -(26 - 18 + 1) = -9.
        fold_mark_adjust_recurse(
            &mut gap,
            18,
            26,
            crate::pos_defs::MAXLNUM,
            -9,
            false,
        );

        assert_eq!(gap.len(), 2, "the contained fold is deleted");
        assert_eq!(gap[0].fd_top, 10, "the fold above is untouched");
        assert_eq!(gap[0].fd_len, 5);
        assert_eq!(gap[1].fd_top, 21, "30 - 9 = 21");
        assert_eq!(gap[1].fd_len, 5);

        let win = WinT {
            w_folds: gap,
            ..Default::default()
        };
        for (lnum, level) in [(10, 1), (12, 1), (14, 1), (18, 0), (21, 1), (25, 1)] {
            assert_eq!(fold_level_win(&win, lnum), level, "line {lnum}");
        }
    }

    #[test]
    fn fold_mark_adjust_shifts_real_folds_through_the_whole_chain() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT {
            w_folds: sibling_folds(),
            ..Default::default()
        };
        // Fold 20-24 sits entirely inside 18..26, so it moves by
        // amount; the fold above is untouched and the one below moves
        // by amount_after.
        unsafe { fold_mark_adjust(&mut win, 18, 26, 3, 7) };
        let tops: Vec<_> = win.w_folds.iter().map(|f| f.fd_top).collect();
        assert_eq!(tops, vec![10, 23, 37]);
    }

    #[test]
    fn fold_open_nested_opens_children_but_not_the_fold_itself() {
        let mut fpr = FoldT {
            fd_top: 10,
            fd_len: 20,
            fd_flags: fd_flags::FD_CLOSED,
            fd_nested: vec![
                FoldT {
                    fd_flags: fd_flags::FD_CLOSED,
                    ..Default::default()
                },
                FoldT {
                    fd_flags: fd_flags::FD_LEVEL,
                    fd_nested: vec![FoldT {
                        fd_flags: fd_flags::FD_CLOSED,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        fold_open_nested(&mut fpr);

        // The original leaves the fold's own flag to its callers.
        assert_eq!(fpr.fd_flags, fd_flags::FD_CLOSED);
        assert_eq!(fpr.fd_nested[0].fd_flags, fd_flags::FD_OPEN);
        assert_eq!(fpr.fd_nested[1].fd_flags, fd_flags::FD_OPEN);
        // Every depth is opened, not just the first.
        assert_eq!(fpr.fd_nested[1].fd_nested[0].fd_flags, fd_flags::FD_OPEN);
    }

    #[test]
    fn fold_open_nested_on_a_childless_fold_is_a_noop() {
        let mut fpr = FoldT {
            fd_flags: fd_flags::FD_CLOSED,
            ..Default::default()
        };
        fold_open_nested(&mut fpr);
        assert_eq!(fpr.fd_flags, fd_flags::FD_CLOSED);
        assert!(fpr.fd_nested.is_empty());
    }

    #[test]
    fn check_close_rec_closes_a_manually_opened_fold_not_containing_the_line() {
        let mut gap = sibling_folds();
        for fp in &mut gap {
            fp.fd_flags = fd_flags::FD_OPEN;
        }
        // Line 22 is inside the middle fold only, so the other two
        // are handed back to 'foldlevel' control.
        assert!(check_close_rec(&mut gap, 22, 0));
        assert_eq!(gap[0].fd_flags, fd_flags::FD_LEVEL);
        assert_eq!(gap[1].fd_flags, fd_flags::FD_OPEN, "still contains 22");
        assert_eq!(gap[2].fd_flags, fd_flags::FD_LEVEL);
    }

    #[test]
    fn check_close_rec_ignores_folds_that_were_not_manually_opened() {
        let mut gap = sibling_folds();
        gap[0].fd_flags = fd_flags::FD_CLOSED;
        gap[1].fd_flags = fd_flags::FD_LEVEL;
        gap[2].fd_flags = fd_flags::FD_CLOSED;

        assert!(!check_close_rec(&mut gap, 22, 0), "nothing was closed");
        assert_eq!(gap[0].fd_flags, fd_flags::FD_CLOSED);
        assert_eq!(gap[1].fd_flags, fd_flags::FD_LEVEL);
        assert_eq!(gap[2].fd_flags, fd_flags::FD_CLOSED);
    }

    #[test]
    fn check_close_rec_recurses_with_a_rebased_line_number() {
        // Outer fold 10-29 open, nested fold at absolute 15-19 open.
        // Line 25 is inside the outer fold but not the nested one, so
        // only the nested fold closes - and only if the line number
        // was correctly rebased by the parent's fd_top.
        let mut gap = vec![FoldT {
            fd_top: 10,
            fd_len: 20,
            fd_flags: fd_flags::FD_OPEN,
            fd_nested: vec![FoldT {
                fd_top: 5,
                fd_len: 5,
                fd_flags: fd_flags::FD_OPEN,
                ..Default::default()
            }],
            ..Default::default()
        }];

        assert!(check_close_rec(&mut gap, 25, 1));
        assert_eq!(gap[0].fd_flags, fd_flags::FD_OPEN, "still contains 25");
        assert_eq!(gap[0].fd_nested[0].fd_flags, fd_flags::FD_LEVEL);
    }

    #[test]
    fn check_close_rec_leaves_everything_open_while_level_remains() {
        let mut gap = sibling_folds();
        for fp in &mut gap {
            fp.fd_flags = fd_flags::FD_OPEN;
        }
        // A positive level means the folds at this depth are never
        // closed outright; they are only recursed into.
        assert!(!check_close_rec(&mut gap, 22, 1));
        for fp in &gap {
            assert_eq!(fp.fd_flags, fd_flags::FD_OPEN);
        }
    }

    /// A window with an outer fold over lines 20-26 and a nested fold
    /// over 22-24, both closed - the layout used for the `zo`/`zc`
    /// behaviour cross-verified against real nvim.
    fn manual_fold_win(buf: &mut BufT) -> WinT {
        buf.b_ml.ml_line_count = 40;
        WinT {
            w_buffer: buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fml: 0,
                wo_fdl: 99,
                ..Default::default()
            },
            w_foldinvalid: false,
            w_cursor: crate::pos_defs::PosT { lnum: 22, col: 0, coladd: 0 },
            w_folds: vec![FoldT {
                fd_top: 20,
                fd_len: 7,
                fd_flags: fd_flags::FD_CLOSED,
                fd_small: crate::types_defs::TriState::False,
                fd_nested: vec![FoldT {
                    // Relative to the outer fold: absolute 22-24.
                    fd_top: 2,
                    fd_len: 3,
                    fd_flags: fd_flags::FD_CLOSED,
                    fd_small: crate::types_defs::TriState::False,
                    ..Default::default()
                }],
            }],
            ..Default::default()
        }
    }

    /// Builds a real memline holding `lines`, plus a window wired to
    /// it with the given `'shiftwidth'`, `'foldnestmax'` and
    /// `'foldignore'`.
    fn indent_level_fixture(
        lines: &[&[u8]],
        sw: i32,
        fdn: crate::types_defs::OptInt,
        fdi: &[u8],
    ) -> (Box<BufT>, Box<WinT>) {
        let mut buf = Box::new(BufT {
            b_p_ts: 8,
            b_p_sw: crate::types_defs::OptInt::from(sw),
            b_u_curhead: Box::into_raw(Box::new(crate::undo_defs::UHeader::default())),
            ..Default::default()
        });
        assert_eq!(
            unsafe { crate::memline::ml_open(&mut buf) },
            crate::vim_defs::OK
        );
        // ml_open leaves a single empty line; replace it, then append.
        let mut first = lines[0].to_vec();
        first.push(0);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, &first) },
            crate::vim_defs::OK
        );
        let buf_ptr: *mut BufT = &mut *buf;
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = buf_ptr;
        for (i, text) in lines.iter().enumerate().skip(1) {
            assert_eq!(
                unsafe { crate::memline::ml_append(i as crate::pos_defs::LinenrT, text, 0, false) },
                crate::vim_defs::OK
            );
        }
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;

        let win = Box::new(WinT {
            w_buffer: buf_ptr,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fdn: fdn,
                wo_fdi: Some(fdi.to_vec()),
                ..Default::default()
            },
            ..Default::default()
        });
        (buf, win)
    }

    fn close_indent_level_fixture(mut buf: Box<BufT>) {
        unsafe {
            if !buf.b_u_curhead.is_null() {
                drop(Box::from_raw(buf.b_u_curhead));
                buf.b_u_curhead = std::ptr::null_mut();
            }
            crate::memline::ml_close(&mut buf, false);
        }
    }

    /// Builds a real memline holding `lines`, plus a buffer with the
    /// given `'commentstring'` and an undo header so `u_save` works.
    fn del_marker_fixture(lines: &[&[u8]], cms: &[u8]) -> Box<BufT> {
        let mut buf = Box::new(BufT {
            b_p_ts: 8,
            // undo_allowed refuses to save when 'nomodifiable'.
            b_p_ma: 1,
            // The undo code's own invariant is that b_u_newhead is
            // non-null whenever b_u_synced is false; a fresh buffer
            // has no newhead, so it must start synced.
            b_u_synced: true,
            b_p_cms: Some(cms.to_vec()),
            b_u_curhead: Box::into_raw(Box::new(crate::undo_defs::UHeader::default())),
            ..Default::default()
        });
        assert_eq!(
            unsafe { crate::memline::ml_open(&mut buf) },
            crate::vim_defs::OK
        );
        let mut first = lines[0].to_vec();
        first.push(0);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, &first) },
            crate::vim_defs::OK
        );
        let buf_ptr: *mut BufT = &mut *buf;
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = buf_ptr;
        for (i, text) in lines.iter().enumerate().skip(1) {
            // NUL-terminate like the replaced first line, so every
            // line in the fixture reads back the same way.
            let mut owned = text.to_vec();
            owned.push(0);
            assert_eq!(
                unsafe { crate::memline::ml_append(i as crate::pos_defs::LinenrT, &owned, 0, false) },
                crate::vim_defs::OK
            );
        }
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
        buf
    }

    #[test]
    fn delete_fold_markers_removes_both_ends_of_a_fold() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = del_marker_fixture(&[b"start {{{", b"middle", b"end }}}"], b"");
        let buf_ptr: *mut BufT = &mut *buf;
        let mut win = WinT {
            w_buffer: buf_ptr,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fmr: Some(b"{{{,}}}".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        unsafe { parse_marker(&win) };

        let mut fp = FoldT { fd_top: 1, fd_len: 3, ..Default::default() };
        unsafe {
            let g = crate::globals::GLOBALS.get_mut();
            let (pb, pw) = (g.curbuf, g.curwin);
            g.curbuf = buf_ptr;
            g.curwin = win_ptr;
            delete_fold_markers(win_ptr, &mut fp, false, 0);
            let g = crate::globals::GLOBALS.get_mut();
            g.curbuf = pb;
            g.curwin = pw;
        }

        assert_eq!(unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 1) }, b"start \0");
        assert_eq!(unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 3) }, b"end \0");
        unsafe { *FOLD_MARKERS.get_mut() = None };
        close_indent_level_fixture(buf);
    }

    #[test]
    fn delete_fold_markers_recurses_into_nested_folds() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = del_marker_fixture(
            &[b"outer {{{", b"inner {{{", b"inner end }}}", b"outer end }}}"],
            b"",
        );
        let buf_ptr: *mut BufT = &mut *buf;
        let mut win = WinT {
            w_buffer: buf_ptr,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fmr: Some(b"{{{,}}}".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        unsafe { parse_marker(&win) };

        // Outer fold spans lines 1-4; the nested one spans 2-3, i.e.
        // fd_top 1 relative to its parent.
        let mut fp = FoldT {
            fd_top: 1,
            fd_len: 4,
            fd_nested: vec![FoldT { fd_top: 1, fd_len: 2, ..Default::default() }],
            ..Default::default()
        };
        unsafe {
            let g = crate::globals::GLOBALS.get_mut();
            let (pb, pw) = (g.curbuf, g.curwin);
            g.curbuf = buf_ptr;
            g.curwin = win_ptr;
            delete_fold_markers(win_ptr, &mut fp, true, 0);
            let g = crate::globals::GLOBALS.get_mut();
            g.curbuf = pb;
            g.curwin = pw;
        }

        for (lnum, want) in [
            (1, b"outer \0".as_ref()),
            (2, b"inner \0"),
            (3, b"inner end \0"),
            (4, b"outer end \0"),
        ] {
            assert_eq!(
                unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, lnum) },
                want,
                "line {lnum}"
            );
        }
        unsafe { *FOLD_MARKERS.get_mut() = None };
        close_indent_level_fixture(buf);
    }

    #[test]
    fn delete_fold_markers_without_recursion_leaves_nested_markers() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = del_marker_fixture(
            &[b"outer {{{", b"inner {{{", b"inner end }}}", b"outer end }}}"],
            b"",
        );
        let buf_ptr: *mut BufT = &mut *buf;
        let mut win = WinT {
            w_buffer: buf_ptr,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fmr: Some(b"{{{,}}}".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        unsafe { parse_marker(&win) };

        let mut fp = FoldT {
            fd_top: 1,
            fd_len: 4,
            fd_nested: vec![FoldT { fd_top: 1, fd_len: 2, ..Default::default() }],
            ..Default::default()
        };
        unsafe {
            let g = crate::globals::GLOBALS.get_mut();
            let (pb, pw) = (g.curbuf, g.curwin);
            g.curbuf = buf_ptr;
            g.curwin = win_ptr;
            delete_fold_markers(win_ptr, &mut fp, false, 0);
            let g = crate::globals::GLOBALS.get_mut();
            g.curbuf = pb;
            g.curwin = pw;
        }

        assert_eq!(unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 1) }, b"outer \0");
        // The nested fold's own markers survive.
        assert_eq!(unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 2) }, b"inner {{{\0");
        assert_eq!(
            unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 3) },
            b"inner end }}}\0"
        );
        unsafe { *FOLD_MARKERS.get_mut() = None };
        close_indent_level_fixture(buf);
    }

    #[test]
    fn fold_add_marker_wraps_the_marker_in_commentstring() {
        let _lock = crate::globals::global_state_test_lock();
        // Cross-verified against real nvim: with commentstring
        // "/*%s*/" and the default 'foldmarker', zf over two lines
        // yields "alpha/*{{{*/" and "beta/*}}}*/".
        let mut buf = del_marker_fixture(&[b"alpha", b"beta"], b"/*%s*/");
        let buf_ptr: *mut BufT = &mut *buf;

        unsafe { add_marker_with_curbuf(buf_ptr, 1, b"{{{") };
        unsafe { add_marker_with_curbuf(buf_ptr, 2, b"}}}") };

        assert_eq!(
            unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 1) },
            b"alpha/*{{{*/\0"
        );
        assert_eq!(
            unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 2) },
            b"beta/*}}}*/\0"
        );
        close_indent_level_fixture(buf);
    }

    #[test]
    fn fold_add_marker_appends_plainly_without_a_commentstring() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = del_marker_fixture(&[b"alpha"], b"");
        let buf_ptr: *mut BufT = &mut *buf;

        unsafe { add_marker_with_curbuf(buf_ptr, 1, b"{{{") };

        assert_eq!(
            unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 1) },
            b"alpha{{{\0"
        );
        close_indent_level_fixture(buf);
    }

    #[test]
    fn fold_add_marker_round_trips_with_fold_del_marker() {
        let _lock = crate::globals::global_state_test_lock();
        // Adding then deleting a marker must restore the line, both
        // with and without a 'commentstring' wrapper.
        for cms in [b"".as_ref(), b"/*%s*/"] {
            let mut buf = del_marker_fixture(&[b"alpha"], cms);
            let buf_ptr: *mut BufT = &mut *buf;

            unsafe { add_marker_with_curbuf(buf_ptr, 1, b"{{{") };
            unsafe { del_marker_with_curbuf(buf_ptr, 1, b"{{{") };

            assert_eq!(
                unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 1) },
                b"alpha\0",
                "cms {cms:?}"
            );
            close_indent_level_fixture(buf);
        }
    }

    /// Runs `fold_add_marker` with `curbuf` and `curwin` installed,
    /// which `u_save` requires, restoring both afterwards.
    unsafe fn add_marker_with_curbuf(buf: *mut BufT, lnum: crate::pos_defs::LinenrT, marker: &[u8]) {
        unsafe {
            let mut win = WinT {
                w_buffer: buf,
                ..Default::default()
            };
            let g = crate::globals::GLOBALS.get_mut();
            let (prev_buf, prev_win) = (g.curbuf, g.curwin);
            g.curbuf = buf;
            g.curwin = &mut win as *mut WinT;
            fold_add_marker(buf, lnum, marker);
            let g = crate::globals::GLOBALS.get_mut();
            g.curbuf = prev_buf;
            g.curwin = prev_win;
        }
    }

    /// Runs `fold_del_marker` with `curbuf` and `curwin` installed,
    /// which `u_save` requires, restoring both afterwards.
    unsafe fn del_marker_with_curbuf(buf: *mut BufT, lnum: crate::pos_defs::LinenrT, marker: &[u8]) {
        unsafe {
            let mut win = WinT {
                w_buffer: buf,
                ..Default::default()
            };
            let g = crate::globals::GLOBALS.get_mut();
            let (prev_buf, prev_win) = (g.curbuf, g.curwin);
            g.curbuf = buf;
            g.curwin = &mut win as *mut WinT;
            fold_del_marker(buf, lnum, marker);
            let g = crate::globals::GLOBALS.get_mut();
            g.curbuf = prev_buf;
            g.curwin = prev_win;
        }
    }

    #[test]
    fn fold_del_marker_removes_a_plain_marker() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = del_marker_fixture(&[b"code {{{"], b"");
        let buf_ptr: *mut BufT = &mut *buf;

        unsafe { del_marker_with_curbuf(buf_ptr, 1, b"{{{") };

        let line = unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 1) };
        assert_eq!(line, b"code \0", "ml_get_buf keeps the trailing NUL");
        close_indent_level_fixture(buf);
    }

    #[test]
    fn fold_del_marker_removes_the_level_digit_too() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = del_marker_fixture(&[b"numbered {{{3"], b"");
        let buf_ptr: *mut BufT = &mut *buf;

        unsafe { del_marker_with_curbuf(buf_ptr, 1, b"{{{") };

        let line = unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 1) };
        assert_eq!(line, b"numbered \0", "the explicit level goes with it");
        close_indent_level_fixture(buf);
    }

    #[test]
    fn fold_del_marker_removes_a_matching_commentstring() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = del_marker_fixture(&[b"with cms /*{{{*/"], b"/*%s*/");
        let buf_ptr: *mut BufT = &mut *buf;

        unsafe { del_marker_with_curbuf(buf_ptr, 1, b"{{{") };

        let line = unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 1) };
        assert_eq!(line, b"with cms \0", "the comment wrapper goes too");
        close_indent_level_fixture(buf);
    }

    #[test]
    fn fold_del_marker_leaves_a_line_without_the_marker_alone() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = del_marker_fixture(&[b"plain text"], b"");
        let buf_ptr: *mut BufT = &mut *buf;

        unsafe { del_marker_with_curbuf(buf_ptr, 1, b"{{{") };

        let line = unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 1) };
        assert_eq!(line, b"plain text\0");
        close_indent_level_fixture(buf);
    }

    #[test]
    fn fold_del_marker_ignores_a_line_past_the_end_of_the_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = del_marker_fixture(&[b"only line {{{"], b"");
        let buf_ptr: *mut BufT = &mut *buf;

        // A fold's end marker may be missing with the fold running
        // past the last line, which the original tolerates.
        unsafe { del_marker_with_curbuf(buf_ptr, 99, b"{{{") };

        let line = unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 1) };
        assert_eq!(line, b"only line {{{\0", "untouched");
        close_indent_level_fixture(buf);
    }

    #[test]
    fn fold_del_marker_removes_only_the_first_marker_on_a_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = del_marker_fixture(&[b"a {{{ b {{{"], b"");
        let buf_ptr: *mut BufT = &mut *buf;

        // The original breaks after the first match.
        unsafe { del_marker_with_curbuf(buf_ptr, 1, b"{{{") };

        let line = unsafe { crate::memline::ml_get_buf(&mut *buf_ptr, 1) };
        assert_eq!(line, b"a  b {{{\0");
        close_indent_level_fixture(buf);
    }

    #[test]
    fn delete_fold_removes_the_deepest_fold_at_a_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        win.w_folds = vec![FoldT {
            fd_top: 10,
            fd_len: 5,
            fd_flags: fd_flags::FD_OPEN,
            fd_small: crate::types_defs::TriState::False,
            ..Default::default()
        }];
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe { delete_fold(win_ptr, 12, 12, false, false) };

        assert!(unsafe { (*win_ptr).w_folds.is_empty() });
    }

    #[test]
    fn delete_fold_promotes_nested_folds_like_real_nvim_zd() {
        let _lock = crate::globals::global_state_test_lock();
        // Cross-verified against real nvim: with a fold at 12-14
        // nested inside one at 10-20, zd on line 10 leaves
        // foldlevel() reporting 0 at line 10 and 1 at line 13.
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        win.w_folds = vec![FoldT {
            fd_top: 10,
            fd_len: 11,
            fd_flags: fd_flags::FD_OPEN,
            fd_small: crate::types_defs::TriState::False,
            fd_nested: vec![FoldT {
                fd_top: 2,
                fd_len: 3,
                fd_flags: fd_flags::FD_OPEN,
                fd_small: crate::types_defs::TriState::False,
                ..Default::default()
            }],
        }];
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe { delete_fold(win_ptr, 10, 10, false, false) };

        assert_eq!(fold_level_win(unsafe { &*win_ptr }, 10), 0);
        assert_eq!(fold_level_win(unsafe { &*win_ptr }, 13), 1);
    }

    #[test]
    fn delete_fold_recursive_removes_the_nested_folds_too() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        win.w_folds = vec![FoldT {
            fd_top: 10,
            fd_len: 11,
            fd_flags: fd_flags::FD_OPEN,
            fd_small: crate::types_defs::TriState::False,
            fd_nested: vec![FoldT {
                fd_top: 2,
                fd_len: 3,
                fd_flags: fd_flags::FD_OPEN,
                fd_small: crate::types_defs::TriState::False,
                ..Default::default()
            }],
        }];
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe { delete_fold(win_ptr, 10, 10, true, false) };

        assert!(unsafe { (*win_ptr).w_folds.is_empty() });
        assert_eq!(fold_level_win(unsafe { &*win_ptr }, 13), 0);
    }

    #[test]
    fn delete_fold_over_a_range_removes_every_fold_in_it() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        win.w_folds = sibling_folds();
        for fp in &mut win.w_folds {
            fp.fd_flags = fd_flags::FD_OPEN;
            fp.fd_small = crate::types_defs::TriState::False;
        }
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe { delete_fold(win_ptr, 10, 34, false, false) };

        assert!(unsafe { (*win_ptr).w_folds.is_empty() });
    }

    #[test]
    fn delete_fold_leaves_folds_outside_the_range_alone() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        win.w_folds = sibling_folds();
        for fp in &mut win.w_folds {
            fp.fd_flags = fd_flags::FD_OPEN;
            fp.fd_small = crate::types_defs::TriState::False;
        }
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        // Only the middle fold (20-24) is in range.
        unsafe { delete_fold(win_ptr, 20, 24, false, false) };

        let tops: Vec<_> = unsafe { (*win_ptr).w_folds.iter().map(|f| f.fd_top).collect() };
        assert_eq!(tops, vec![10, 30]);
    }

    #[test]
    fn delete_fold_over_lines_with_no_folds_changes_nothing() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        win.w_folds = sibling_folds();
        for fp in &mut win.w_folds {
            fp.fd_flags = fd_flags::FD_OPEN;
            fp.fd_small = crate::types_defs::TriState::False;
        }
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe { delete_fold(win_ptr, 1, 5, false, false) };

        assert_eq!(unsafe { (*win_ptr).w_folds.len() }, 3);
    }

    #[test]
    #[should_panic(expected = "deleteFoldMarkers")]
    fn delete_fold_with_a_marker_foldmethod_is_unimplemented() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        win.w_onebuf_opt.wo_fdm = Some(b"marker".to_vec());
        win.w_folds = vec![FoldT {
            fd_top: 10,
            fd_len: 5,
            fd_flags: fd_flags::FD_OPEN,
            fd_small: crate::types_defs::TriState::False,
            ..Default::default()
        }];
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe { delete_fold(win_ptr, 12, 12, false, false) };
    }

    /// A window able to hold manual folds, with 'foldlevel' high
    /// enough that nothing closes on level alone.
    fn fold_create_win(buf: &mut BufT) -> WinT {
        buf.b_ml.ml_line_count = 40;
        WinT {
            w_buffer: buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fml: 0,
                wo_fdl: 99,
                ..Default::default()
            },
            w_foldinvalid: false,
            ..Default::default()
        }
    }

    #[test]
    fn fold_create_makes_a_closed_fold_over_the_range() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe {
            fold_create(
                win_ptr,
                crate::pos_defs::PosT { lnum: 10, col: 0, coladd: 0 },
                crate::pos_defs::PosT { lnum: 20, col: 0, coladd: 0 },
            )
        };

        let folds = unsafe { &(*win_ptr).w_folds };
        assert_eq!(folds.len(), 1);
        assert_eq!(folds[0].fd_top, 10);
        assert_eq!(folds[0].fd_len, 11, "10..20 inclusive");
        assert_eq!(folds[0].fd_flags, fd_flags::FD_CLOSED);
        assert!(unsafe { (*win_ptr).w_fold_manual });
    }

    #[test]
    fn fold_create_reverses_a_backwards_range() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe {
            fold_create(
                win_ptr,
                crate::pos_defs::PosT { lnum: 20, col: 0, coladd: 0 },
                crate::pos_defs::PosT { lnum: 10, col: 0, coladd: 0 },
            )
        };

        let folds = unsafe { &(*win_ptr).w_folds };
        assert_eq!(folds[0].fd_top, 10);
        assert_eq!(folds[0].fd_len, 11);
    }

    #[test]
    fn fold_create_absorbs_an_existing_fold_as_nested() {
        let _lock = crate::globals::global_state_test_lock();
        // Cross-verified against real nvim: with a fold at 12-14
        // already present, creating one over 10-20 leaves the outer
        // fold at level 1 and the inner at level 2, with lines past
        // the outer fold back at level 0.
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        win.w_folds = vec![FoldT {
            fd_top: 12,
            fd_len: 3,
            fd_flags: fd_flags::FD_OPEN,
            fd_small: crate::types_defs::TriState::False,
            ..Default::default()
        }];
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe {
            fold_create(
                win_ptr,
                crate::pos_defs::PosT { lnum: 10, col: 0, coladd: 0 },
                crate::pos_defs::PosT { lnum: 20, col: 0, coladd: 0 },
            )
        };

        let folds = unsafe { &(*win_ptr).w_folds };
        assert_eq!(folds.len(), 1, "the old fold is now nested");
        assert_eq!(folds[0].fd_top, 10);
        assert_eq!(folds[0].fd_nested.len(), 1);
        // Rebased onto the new parent: 12 - 10 = 2.
        assert_eq!(folds[0].fd_nested[0].fd_top, 2);

        assert_eq!(fold_level_win(unsafe { &*win_ptr }, 10), 1);
        assert_eq!(fold_level_win(unsafe { &*win_ptr }, 13), 2);
        assert_eq!(fold_level_win(unsafe { &*win_ptr }, 21), 0);
    }

    #[test]
    fn fold_create_widens_to_cover_a_partly_overlapping_fold() {
        let _lock = crate::globals::global_state_test_lock();
        // The existing fold 8-12 starts above the new fold's range,
        // so the new fold starts there instead rather than splitting
        // the existing one.
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        win.w_folds = vec![FoldT {
            fd_top: 8,
            fd_len: 5,
            fd_flags: fd_flags::FD_OPEN,
            fd_small: crate::types_defs::TriState::False,
            ..Default::default()
        }];
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe {
            fold_create(
                win_ptr,
                crate::pos_defs::PosT { lnum: 10, col: 0, coladd: 0 },
                crate::pos_defs::PosT { lnum: 20, col: 0, coladd: 0 },
            )
        };

        let folds = unsafe { &(*win_ptr).w_folds };
        assert_eq!(folds[0].fd_top, 8, "widened to the contained fold's start");
        assert_eq!(folds[0].fd_len, 13, "8..20 inclusive");
        assert_eq!(folds[0].fd_nested[0].fd_top, 0, "8 - 8");
    }

    #[test]
    fn fold_create_nests_inside_an_enclosing_fold() {
        let _lock = crate::globals::global_state_test_lock();
        // The new fold lies entirely inside an existing one, so it
        // becomes a child rather than a sibling.
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        win.w_folds = vec![FoldT {
            fd_top: 5,
            fd_len: 30,
            fd_flags: fd_flags::FD_OPEN,
            fd_small: crate::types_defs::TriState::False,
            ..Default::default()
        }];
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe {
            fold_create(
                win_ptr,
                crate::pos_defs::PosT { lnum: 10, col: 0, coladd: 0 },
                crate::pos_defs::PosT { lnum: 20, col: 0, coladd: 0 },
            )
        };

        let folds = unsafe { &(*win_ptr).w_folds };
        assert_eq!(folds.len(), 1, "still one top-level fold");
        assert_eq!(folds[0].fd_top, 5);
        assert_eq!(folds[0].fd_nested.len(), 1);
        // Relative to the enclosing fold: 10 - 5 = 5.
        assert_eq!(folds[0].fd_nested[0].fd_top, 5);
        assert_eq!(folds[0].fd_nested[0].fd_len, 11);
    }

    #[test]
    #[should_panic(expected = "foldCreateMarkers")]
    fn fold_create_with_foldmethod_marker_is_unimplemented() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = fold_create_win(&mut buf);
        win.w_onebuf_opt.wo_fdm = Some(b"marker".to_vec());
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe {
            fold_create(
                win_ptr,
                crate::pos_defs::PosT { lnum: 10, col: 0, coladd: 0 },
                crate::pos_defs::PosT { lnum: 20, col: 0, coladd: 0 },
            )
        };
    }

    #[test]
    fn fold_move_range_case_10_no_folds_between_the_range_and_dest() {
        let _lock = crate::globals::global_state_test_lock();
        // The only fold is well past dest, so nothing changes.
        let mut gap = vec![FoldT { fd_top: 50, fd_len: 5, ..Default::default() }];
        let before = gap.clone();
        unsafe { fold_move_range(&mut gap, 5, 7, 20) };
        assert_eq!(gap, before);
    }

    #[test]
    fn fold_move_range_case_9_shifts_intervening_folds_up() {
        let _lock = crate::globals::global_state_test_lock();
        // Moving lines 5..7 down to after 30 leaves the fold at 10-14
        // entirely between the range and dest, so it shifts up by the
        // three moved lines.
        let mut gap = vec![FoldT { fd_top: 10, fd_len: 5, ..Default::default() }];
        unsafe { fold_move_range(&mut gap, 5, 7, 30) };
        assert_eq!(gap.len(), 1);
        assert_eq!(gap[0].fd_top, 7, "10 - 3");
        assert_eq!(gap[0].fd_len, 5, "its length is unchanged");
    }

    #[test]
    fn fold_move_range_case_9_shifts_several_folds() {
        let _lock = crate::globals::global_state_test_lock();
        let mut gap = vec![
            FoldT { fd_top: 10, fd_len: 5, ..Default::default() },
            FoldT { fd_top: 20, fd_len: 5, ..Default::default() },
        ];
        unsafe { fold_move_range(&mut gap, 5, 7, 30) };
        let tops: Vec<_> = gap.iter().map(|f| f.fd_top).collect();
        assert_eq!(tops, vec![7, 17], "both shift up by 3");
    }

    #[test]
    fn fold_move_range_case_4_recurses_without_changing_the_outer_fold() {
        let _lock = crate::globals::global_state_test_lock();
        // The outer fold 1-40 contains the whole operation, so only
        // its nested folds move.
        let mut gap = vec![FoldT {
            fd_top: 1,
            fd_len: 40,
            fd_nested: vec![FoldT { fd_top: 10, fd_len: 5, ..Default::default() }],
            ..Default::default()
        }];
        unsafe { fold_move_range(&mut gap, 5, 7, 30) };
        assert_eq!(gap[0].fd_top, 1, "the outer fold is untouched");
        assert_eq!(gap[0].fd_len, 40);
        // The nested fold shifted up by the three moved lines.
        assert_eq!(gap[0].fd_nested[0].fd_top, 7);
    }

    #[test]
    fn fold_move_range_keeps_the_folds_sorted_after_reordering() {
        let _lock = crate::globals::global_state_test_lock();
        // A fold inside the moved range plus one after it: the moved
        // fold ends up below the other, so the reversal has to put
        // them back in order.
        let mut gap = vec![
            FoldT { fd_top: 5, fd_len: 3, ..Default::default() },
            FoldT { fd_top: 12, fd_len: 4, ..Default::default() },
        ];
        unsafe { fold_move_range(&mut gap, 5, 7, 20) };

        let tops: Vec<_> = gap.iter().map(|f| f.fd_top).collect();
        assert!(
            tops.windows(2).all(|w| w[0] <= w[1]),
            "folds must stay sorted, got {tops:?}"
        );
        assert_eq!(gap.len(), 2, "no fold is lost");
    }

    #[test]
    fn fold_move_range_case_3_shrinks_a_fold_containing_the_whole_range() {
        let _lock = crate::globals::global_state_test_lock();
        // The fold 3-9 starts above line1 and ends below line2, so the
        // moved range sits entirely inside it and the fold simply
        // loses those three lines.
        let mut gap = vec![FoldT { fd_top: 3, fd_len: 7, ..Default::default() }];
        unsafe { fold_move_range(&mut gap, 5, 7, 20) };
        assert_eq!(gap[0].fd_top, 3);
        assert_eq!(gap[0].fd_len, 4, "7 - 3");
    }

    #[test]
    fn fold_move_range_case_2_truncates_a_fold_ending_inside_the_range() {
        let _lock = crate::globals::global_state_test_lock();
        // The fold 3-6 starts above line1 and ends *within* the moved
        // range, so it is truncated to end just above line1.
        let mut gap = vec![FoldT { fd_top: 3, fd_len: 4, ..Default::default() }];
        unsafe { fold_move_range(&mut gap, 5, 7, 20) };
        assert_eq!(gap[0].fd_top, 3);
        assert_eq!(gap[0].fd_len, 2, "now covers 3-4 only");
    }

    #[test]
    fn fold_move_to_matches_real_nvim_zj_and_bracket_z() {
        let _lock = crate::globals::global_state_test_lock();
        // Cross-verified against real nvim: with open folds at 10-14
        // and 20-24, zj from line 1 lands on 10 and again on 20;
        // ]z from line 12 lands on 14 and [z on 10.
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fml: 0,
                wo_fdl: 99,
                ..Default::default()
            },
            w_foldinvalid: false,
            w_folds: vec![
                FoldT {
                    fd_top: 10,
                    fd_len: 5,
                    fd_flags: fd_flags::FD_OPEN,
                    fd_small: crate::types_defs::TriState::False,
                    ..Default::default()
                },
                FoldT {
                    fd_top: 20,
                    fd_len: 5,
                    fd_flags: fd_flags::FD_OPEN,
                    fd_small: crate::types_defs::TriState::False,
                    ..Default::default()
                },
            ],
            w_cursor: crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        let buf_ptr = win.w_buffer;
        let _guard = CurwinGuard::set(win_ptr);
        // A successful move calls setpcmark, which reads curbuf.
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = buf_ptr;

        // zj: next fold start, twice.
        assert_eq!(
            unsafe { fold_move_to(true, crate::vim_defs::Direction::Forward, 1) },
            crate::vim_defs::OK
        );
        assert_eq!(unsafe { (*win_ptr).w_cursor.lnum }, 10);
        assert_eq!(
            unsafe { fold_move_to(true, crate::vim_defs::Direction::Forward, 1) },
            crate::vim_defs::OK
        );
        assert_eq!(unsafe { (*win_ptr).w_cursor.lnum }, 20);

        // ]z: end of the fold containing the cursor.
        unsafe { (*win_ptr).w_cursor.lnum = 12 };
        assert_eq!(
            unsafe { fold_move_to(false, crate::vim_defs::Direction::Forward, 1) },
            crate::vim_defs::OK
        );
        assert_eq!(unsafe { (*win_ptr).w_cursor.lnum }, 14);

        // [z: start of that same fold.
        unsafe { (*win_ptr).w_cursor.lnum = 12 };
        assert_eq!(
            unsafe { fold_move_to(false, crate::vim_defs::Direction::Backward, 1) },
            crate::vim_defs::OK
        );
        assert_eq!(unsafe { (*win_ptr).w_cursor.lnum }, 10);

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
    }

    #[test]
    fn fold_move_to_fails_when_the_window_has_no_folds() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                ..Default::default()
            },
            w_foldinvalid: false,
            w_cursor: crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        assert_eq!(
            unsafe { fold_move_to(true, crate::vim_defs::Direction::Forward, 1) },
            crate::vim_defs::FAIL
        );
        assert_eq!(unsafe { (*win_ptr).w_cursor.lnum }, 5, "cursor is unmoved");
    }

    #[test]
    fn fold_move_to_stops_when_there_is_no_further_fold() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fml: 0,
                wo_fdl: 99,
                ..Default::default()
            },
            w_foldinvalid: false,
            w_folds: vec![FoldT {
                fd_top: 10,
                fd_len: 5,
                fd_flags: fd_flags::FD_OPEN,
                fd_small: crate::types_defs::TriState::False,
                ..Default::default()
            }],
            // Already past the only fold.
            w_cursor: crate::pos_defs::PosT { lnum: 30, col: 0, coladd: 0 },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        assert_eq!(
            unsafe { fold_move_to(true, crate::vim_defs::Direction::Forward, 1) },
            crate::vim_defs::FAIL
        );
        assert_eq!(unsafe { (*win_ptr).w_cursor.lnum }, 30);
    }

    /// Sets up `'foldmarker'` state for the marker tests and returns
    /// a window plus a real memline holding `lines`.
    fn marker_level_fixture(lines: &[&[u8]]) -> (Box<BufT>, Box<WinT>) {
        let (buf, mut win) = indent_level_fixture(lines, 8, 20, b"#");
        win.w_onebuf_opt.wo_fmr = Some(b"{{{,}}}".to_vec());
        unsafe { parse_marker(&win) };
        (buf, win)
    }

    #[test]
    fn foldlevel_marker_matches_real_nvim() {
        let _lock = crate::globals::global_state_test_lock();
        // Cross-verified against real nvim with the default
        // 'foldmarker': for the lines "plain" / "start {{{" /
        // "inner" / "end }}}" / "numbered {{{2" / "after",
        // foldlevel() reports 0, 1, 1, 1, 2, 2.
        let (buf, mut win) = marker_level_fixture(&[
            b"plain",
            b"start {{{",
            b"inner",
            b"end }}}",
            b"numbered {{{2",
            b"after",
        ]);
        let win_ptr: *mut WinT = &mut *win;

        // The scan carries the previous line's level in flp.lvl, so
        // walk the buffer the way the fold-update pass would.
        let mut lvl = 0;
        let mut levels = Vec::new();
        for lnum in 1..=6 {
            let mut flp = FlineT { wp: win_ptr, lnum, lvl, ..Default::default() };
            unsafe { foldlevel_marker(&mut flp) };
            levels.push(flp.lvl);
            lvl = flp.lvl_next;
        }
        assert_eq!(levels, vec![0, 1, 1, 1, 2, 2]);

        drop(win);
        unsafe { *FOLD_MARKERS.get_mut() = None };
        close_indent_level_fixture(buf);
    }

    #[test]
    fn foldlevel_marker_start_marker_raises_the_level_and_records_a_start() {
        let _lock = crate::globals::global_state_test_lock();
        let (buf, mut win) = marker_level_fixture(&[b"a {{{", b"b"]);
        let win_ptr: *mut WinT = &mut *win;

        let mut flp = FlineT { wp: win_ptr, lnum: 1, lvl: 0, ..Default::default() };
        unsafe { foldlevel_marker(&mut flp) };
        assert_eq!(flp.lvl, 1);
        assert_eq!(flp.lvl_next, 1);
        assert_eq!(flp.start, 1, "one fold starts here");

        drop(win);
        unsafe { *FOLD_MARKERS.get_mut() = None };
        close_indent_level_fixture(buf);
    }

    #[test]
    fn foldlevel_marker_end_marker_only_lowers_the_next_line() {
        let _lock = crate::globals::global_state_test_lock();
        let (buf, mut win) = marker_level_fixture(&[b"a }}}", b"b"]);
        let win_ptr: *mut WinT = &mut *win;

        let mut flp = FlineT { wp: win_ptr, lnum: 1, lvl: 2, ..Default::default() };
        unsafe { foldlevel_marker(&mut flp) };
        assert_eq!(flp.lvl, 2, "this line is still inside the fold");
        assert_eq!(flp.lvl_next, 1);

        drop(win);
        unsafe { *FOLD_MARKERS.get_mut() = None };
        close_indent_level_fixture(buf);
    }

    #[test]
    fn foldlevel_marker_numbered_start_sets_the_level_outright() {
        let _lock = crate::globals::global_state_test_lock();
        let (buf, mut win) = marker_level_fixture(&[b"a {{{5", b"b"]);
        let win_ptr: *mut WinT = &mut *win;

        let mut flp = FlineT { wp: win_ptr, lnum: 1, lvl: 1, ..Default::default() };
        unsafe { foldlevel_marker(&mut flp) };
        assert_eq!(flp.lvl, 5, "not one deeper, but exactly 5");
        assert_eq!(flp.lvl_next, 5);
        assert_eq!(flp.start, 4, "5 - 1 folds start here");

        drop(win);
        unsafe { *FOLD_MARKERS.get_mut() = None };
        close_indent_level_fixture(buf);
    }

    #[test]
    fn foldlevel_marker_handles_several_markers_on_one_line() {
        let _lock = crate::globals::global_state_test_lock();
        let (buf, mut win) = marker_level_fixture(&[b"a {{{ b {{{ c", b"d"]);
        let win_ptr: *mut WinT = &mut *win;

        let mut flp = FlineT { wp: win_ptr, lnum: 1, lvl: 0, ..Default::default() };
        unsafe { foldlevel_marker(&mut flp) };
        assert_eq!(flp.lvl, 2, "both start markers count");
        assert_eq!(flp.start, 2);

        drop(win);
        unsafe { *FOLD_MARKERS.get_mut() = None };
        close_indent_level_fixture(buf);
    }

    #[test]
    fn foldlevel_marker_never_goes_negative() {
        let _lock = crate::globals::global_state_test_lock();
        // An end marker with no matching start would drive the next
        // level below zero, which the original clamps.
        let (buf, mut win) = marker_level_fixture(&[b"a }}}", b"b"]);
        let win_ptr: *mut WinT = &mut *win;

        let mut flp = FlineT { wp: win_ptr, lnum: 1, lvl: 0, ..Default::default() };
        unsafe { foldlevel_marker(&mut flp) };
        assert_eq!(flp.lvl_next, 0);

        drop(win);
        unsafe { *FOLD_MARKERS.get_mut() = None };
        close_indent_level_fixture(buf);
    }

    #[test]
    fn foldlevel_diff_is_zero_when_diff_is_off() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                // 'nodiff' is diff_infold's own first early return, so
                // nothing is ever folded away.
                wo_diff: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;

        for lnum in [1, 5, 100] {
            let mut flp = FlineT { wp: win_ptr, lnum, ..Default::default() };
            unsafe { foldlevel_diff(&mut flp) };
            assert_eq!(flp.lvl, 0, "line {lnum}");
        }
    }

    #[test]
    fn foldlevel_diff_applies_the_line_offset() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_diff: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;

        // lnum + off names the real line, exactly as in
        // foldlevel_indent; with 'nodiff' the answer is 0 either way,
        // but the resolution itself must still happen.
        let mut flp = FlineT { wp: win_ptr, lnum: 3, off: 7, ..Default::default() };
        unsafe { foldlevel_diff(&mut flp) };
        assert_eq!(flp.lvl, 0);
    }

    #[test]
    fn foldlevel_indent_matches_real_nvim() {
        let _lock = crate::globals::global_state_test_lock();
        // Cross-verified against real nvim with shiftwidth=2,
        // foldnestmax=20 and foldignore=#: for the lines
        // "a" / "  b" / "    c" / "" / "  # d" / "e", foldlevel()
        // reports 0, 1, 2, 0, 0, 0.
        let (buf, mut win) = indent_level_fixture(
            &[b"a", b"  b", b"    c", b"", b"  # d", b"e"],
            2,
            20,
            b"#",
        );
        let win_ptr: *mut WinT = &mut *win;

        for (lnum, want) in [(1, 0), (2, 1), (3, 2)] {
            let mut flp = FlineT {
                wp: win_ptr,
                lnum,
                ..Default::default()
            };
            unsafe { foldlevel_indent(&mut flp) };
            assert_eq!(flp.lvl, want, "line {lnum}");
        }

        // An empty line and a 'foldignore' line are both undefined,
        // so the surrounding lines decide - except the last line,
        // which cannot be left undefined.
        let mut flp = FlineT { wp: win_ptr, lnum: 4, ..Default::default() };
        unsafe { foldlevel_indent(&mut flp) };
        assert_eq!(flp.lvl, -1, "empty line is undefined");

        let mut flp = FlineT { wp: win_ptr, lnum: 5, ..Default::default() };
        unsafe { foldlevel_indent(&mut flp) };
        assert_eq!(flp.lvl, -1, "'foldignore' line is undefined");

        drop(win);
        close_indent_level_fixture(buf);
    }

    #[test]
    fn foldlevel_indent_never_leaves_the_first_or_last_line_undefined() {
        let _lock = crate::globals::global_state_test_lock();
        let (buf, mut win) = indent_level_fixture(&[b"", b"  x", b""], 2, 20, b"#");
        let win_ptr: *mut WinT = &mut *win;

        for lnum in [1, 3] {
            let mut flp = FlineT { wp: win_ptr, lnum, ..Default::default() };
            unsafe { foldlevel_indent(&mut flp) };
            assert_eq!(flp.lvl, 0, "line {lnum} is an edge line");
        }

        drop(win);
        close_indent_level_fixture(buf);
    }

    #[test]
    fn foldlevel_indent_is_capped_by_foldnestmax() {
        let _lock = crate::globals::global_state_test_lock();
        // Indent 8 with shiftwidth 2 would be level 4, but
        // 'foldnestmax' of 2 caps it.
        let (buf, mut win) = indent_level_fixture(&[b"a", b"        deep", b"b"], 2, 2, b"#");
        let win_ptr: *mut WinT = &mut *win;

        let mut flp = FlineT { wp: win_ptr, lnum: 2, ..Default::default() };
        unsafe { foldlevel_indent(&mut flp) };
        assert_eq!(flp.lvl, 2);

        drop(win);
        close_indent_level_fixture(buf);
    }

    #[test]
    fn foldlevel_indent_applies_the_line_offset() {
        let _lock = crate::globals::global_state_test_lock();
        // lnum + off names the real line, so lnum 1 with off 2 is
        // line 3.
        let (buf, mut win) = indent_level_fixture(&[b"a", b"  b", b"    c"], 2, 20, b"#");
        let win_ptr: *mut WinT = &mut *win;

        let mut flp = FlineT { wp: win_ptr, lnum: 1, off: 2, ..Default::default() };
        unsafe { foldlevel_indent(&mut flp) };
        assert_eq!(flp.lvl, 2, "resolved to line 3");

        drop(win);
        close_indent_level_fixture(buf);
    }

    #[test]
    fn new_fold_level_hands_manual_folds_back_via_the_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                ..Default::default()
            },
            w_foldinvalid: false,
            w_fold_manual: true,
            w_folds: sibling_folds(),
            ..Default::default()
        };
        win.w_folds[0].fd_flags = fd_flags::FD_CLOSED;
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe { new_fold_level() };

        // 'foldmethod' is "manual", so the diff branch is not reached.
        for fp in unsafe { &(*win_ptr).w_folds } {
            assert_eq!(fp.fd_flags, fd_flags::FD_LEVEL);
        }
        assert!(!unsafe { (*win_ptr).w_fold_manual });
    }

    #[test]
    #[should_panic(expected = "scrollbind")]
    fn new_fold_level_reaches_the_diff_mirroring_boundary() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"diff".to_vec()),
                ..Default::default()
            },
            w_foldinvalid: false,
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe { new_fold_level() };
    }

    #[test]
    fn fold_update_after_insert_skips_the_slow_and_manual_foldmethods() {
        let _lock = crate::globals::global_state_test_lock();
        for fdm in [b"manual".as_ref(), b"syntax", b"expr"] {
            let mut buf = BufT::default();
            let mut win = WinT {
                w_buffer: &mut buf as *mut BufT,
                w_onebuf_opt: crate::buffer_defs::WinoptT {
                    wo_fen: 1,
                    wo_fdm: Some(fdm.to_vec()),
                    ..Default::default()
                },
                w_foldinvalid: false,
                ..Default::default()
            };
            let win_ptr = &mut win as *mut WinT;
            let _guard = CurwinGuard::set(win_ptr);

            unsafe { fold_update_after_insert() };

            assert!(
                !unsafe { (*win_ptr).w_foldinvalid },
                "'foldmethod'={} must not trigger a recompute",
                String::from_utf8_lossy(fdm)
            );
        }
    }

    #[test]
    #[should_panic(expected = "foldUpdate")]
    fn fold_update_after_insert_reaches_the_fold_recomputation_boundary() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                // 'foldmethod'=indent is none of the three skipped
                // methods, so the folds really are invalidated - and
                // the following foldOpenCursor then calls checkupdate,
                // which needs the not-yet-translated foldUpdate.
                wo_fen: 1,
                wo_fdm: Some(b"indent".to_vec()),
                ..Default::default()
            },
            w_foldinvalid: false,
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        unsafe { fold_update_after_insert() };
    }

    #[test]
    fn parse_marker_splits_foldmarker_at_the_comma() {
        let _lock = crate::globals::global_state_test_lock();
        // Cross-verified against real nvim: 'foldmarker' defaults to
        // the brace-triple pair and accepts custom pairs like
        // "<<<,>>>", always comma-separated.
        let win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fmr: Some(b"{{{,}}}".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe { parse_marker(&win) };
        let m = unsafe { FOLD_MARKERS.get_mut() }.clone().expect("parsed");
        assert_eq!(m.start_len, 3);
        assert_eq!(m.end, b"}}}");
        assert_eq!(m.end_len, 3);
        unsafe { *FOLD_MARKERS.get_mut() = None };
    }

    #[test]
    fn parse_marker_handles_markers_of_different_lengths() {
        let _lock = crate::globals::global_state_test_lock();
        let win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fmr: Some(b"BEGIN,END".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe { parse_marker(&win) };
        let m = unsafe { FOLD_MARKERS.get_mut() }.clone().expect("parsed");
        assert_eq!(m.start_len, 5);
        assert_eq!(m.end, b"END");
        assert_eq!(m.end_len, 3);
        unsafe { *FOLD_MARKERS.get_mut() = None };
    }

    #[test]
    fn parse_marker_splits_at_the_first_comma_only() {
        let _lock = crate::globals::global_state_test_lock();
        let win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fmr: Some(b"a,b,c".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe { parse_marker(&win) };
        let m = unsafe { FOLD_MARKERS.get_mut() }.clone().expect("parsed");
        assert_eq!(m.start_len, 1);
        assert_eq!(m.end, b"b,c", "everything after the first comma");
        unsafe { *FOLD_MARKERS.get_mut() = None };
    }

    #[test]
    fn parse_marker_without_a_comma_leaves_no_end_marker() {
        let _lock = crate::globals::global_state_test_lock();
        // 'foldmarker' is validated to contain a comma before it can
        // be set, so this is unreachable in a real session; it is
        // asserted only to pin the behaviour down.
        let win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fmr: Some(b"nocomma".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe { parse_marker(&win) };
        let m = unsafe { FOLD_MARKERS.get_mut() }.clone().expect("parsed");
        assert_eq!(m.start_len, 7);
        assert!(m.end.is_empty());
        assert_eq!(m.end_len, 0);
        unsafe { *FOLD_MARKERS.get_mut() = None };
    }

    #[test]
    fn op_fold_range_closes_every_fold_in_the_range() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fml: 0,
                wo_fdl: 99,
                ..Default::default()
            },
            w_foldinvalid: false,
            w_folds: sibling_folds(),
            ..Default::default()
        };
        for fp in &mut win.w_folds {
            fp.fd_flags = fd_flags::FD_OPEN;
            fp.fd_small = crate::types_defs::TriState::False;
        }
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        // Lines 10..34 span all three folds.
        unsafe {
            op_fold_range(
                crate::pos_defs::PosT { lnum: 10, col: 0, coladd: 0 },
                crate::pos_defs::PosT { lnum: 34, col: 0, coladd: 0 },
                false,
                false,
            )
        };

        for fp in unsafe { &(*win_ptr).w_folds } {
            assert_eq!(fp.fd_flags, fd_flags::FD_CLOSED);
        }
    }

    #[test]
    fn op_fold_range_only_touches_folds_inside_the_range() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fml: 0,
                wo_fdl: 99,
                ..Default::default()
            },
            w_foldinvalid: false,
            w_folds: sibling_folds(),
            ..Default::default()
        };
        for fp in &mut win.w_folds {
            fp.fd_flags = fd_flags::FD_OPEN;
            fp.fd_small = crate::types_defs::TriState::False;
        }
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        // Only the middle fold (20-24) lies in 20..24.
        unsafe {
            op_fold_range(
                crate::pos_defs::PosT { lnum: 20, col: 0, coladd: 0 },
                crate::pos_defs::PosT { lnum: 24, col: 0, coladd: 0 },
                false,
                false,
            )
        };

        let folds = unsafe { &(*win_ptr).w_folds };
        assert_eq!(folds[0].fd_flags, fd_flags::FD_OPEN);
        assert_eq!(folds[1].fd_flags, fd_flags::FD_CLOSED);
        assert_eq!(folds[2].fd_flags, fd_flags::FD_OPEN);
    }

    #[test]
    fn op_fold_range_over_lines_with_no_folds_changes_nothing() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fml: 0,
                wo_fdl: 99,
                ..Default::default()
            },
            w_foldinvalid: false,
            w_folds: sibling_folds(),
            ..Default::default()
        };
        for fp in &mut win.w_folds {
            fp.fd_flags = fd_flags::FD_OPEN;
            fp.fd_small = crate::types_defs::TriState::False;
        }
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);

        // Lines 1..5 are outside every fold.
        unsafe {
            op_fold_range(
                crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 },
                crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
                false,
                false,
            )
        };

        for fp in unsafe { &(*win_ptr).w_folds } {
            assert_eq!(fp.fd_flags, fd_flags::FD_OPEN);
        }
    }

    #[test]
    fn open_fold_opens_the_topmost_closed_fold_first() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = manual_fold_win(&mut buf);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);
        let pos = crate::pos_defs::PosT { lnum: 22, col: 0, coladd: 0 };

        // Every access goes through `win_ptr` rather than the local
        // `win`, since open_fold reaches the window through the same
        // pointer via GLOBALS.curwin - a direct `&mut win` here would
        // invalidate that lineage under Tree Borrows.
        //
        // Cross-verified against real nvim: with folds at 22-24 and
        // 20-26 both closed, foldclosed(22) is 20; one zo makes it
        // 22; a second zo makes it -1.
        let mut first = 0;
        assert!(unsafe { has_folding(&mut *win_ptr, 22, Some(&mut first), None) });
        assert_eq!(first, 20);

        unsafe { open_fold(pos, 1) };
        let mut first = 0;
        assert!(unsafe { has_folding(&mut *win_ptr, 22, Some(&mut first), None) });
        assert_eq!(first, 22, "the outer fold opened, revealing the inner");

        unsafe { open_fold(pos, 1) };
        assert!(
            !unsafe { has_folding(&mut *win_ptr, 22, None, None) },
            "both folds are now open"
        );
    }

    #[test]
    fn open_fold_with_a_count_opens_several_levels_at_once() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = manual_fold_win(&mut buf);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinGuard::set(win_ptr);
        let pos = crate::pos_defs::PosT { lnum: 22, col: 0, coladd: 0 };

        unsafe { open_fold(pos, 2) };
        assert!(!unsafe { has_folding(&mut *win_ptr, 22, None, None) });
    }

    #[test]
    fn open_fold_recurse_opens_every_nested_fold() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = manual_fold_win(&mut buf);
        let _guard = CurwinGuard::set(&mut win as *mut WinT);
        let pos = crate::pos_defs::PosT { lnum: 22, col: 0, coladd: 0 };

        unsafe { open_fold_recurse(pos) };
        assert_eq!(win.w_folds[0].fd_flags, fd_flags::FD_OPEN);
        assert_eq!(win.w_folds[0].fd_nested[0].fd_flags, fd_flags::FD_OPEN);
    }

    #[test]
    fn close_fold_recurse_closes_the_outer_fold() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = manual_fold_win(&mut buf);
        win.w_folds[0].fd_flags = fd_flags::FD_OPEN;
        win.w_folds[0].fd_nested[0].fd_flags = fd_flags::FD_OPEN;
        let _guard = CurwinGuard::set(&mut win as *mut WinT);
        let pos = crate::pos_defs::PosT { lnum: 22, col: 0, coladd: 0 };

        unsafe { close_fold_recurse(pos) };
        assert_eq!(win.w_folds[0].fd_flags, fd_flags::FD_CLOSED);
    }

    #[test]
    fn close_fold_closes_the_deepest_open_fold_first() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = manual_fold_win(&mut buf);
        win.w_folds[0].fd_flags = fd_flags::FD_OPEN;
        win.w_folds[0].fd_nested[0].fd_flags = fd_flags::FD_OPEN;
        let _guard = CurwinGuard::set(&mut win as *mut WinT);
        let pos = crate::pos_defs::PosT { lnum: 22, col: 0, coladd: 0 };

        unsafe { close_fold(pos, 1) };
        // The inner fold is the deepest open one containing line 22.
        assert_eq!(win.w_folds[0].fd_nested[0].fd_flags, fd_flags::FD_CLOSED);
        assert_eq!(win.w_folds[0].fd_flags, fd_flags::FD_OPEN);

        unsafe { close_fold(pos, 1) };
        assert_eq!(win.w_folds[0].fd_flags, fd_flags::FD_CLOSED);
    }

    #[test]
    fn set_manual_fold_win_marks_the_window_manually_folded() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = manual_fold_win(&mut buf);
        assert!(!win.w_fold_manual);
        let _guard = CurwinGuard::set(&mut win as *mut WinT);

        let mut done = done::DONE_NOTHING;
        unsafe { set_manual_fold_win(&mut win as *mut WinT, 22, true, false, Some(&mut done)) };

        assert!(win.w_fold_manual);
        assert_ne!(done & done::DONE_FOLD, 0, "a fold was found");
        assert_ne!(done & done::DONE_ACTION, 0, "and it was opened");
    }

    #[test]
    fn set_manual_fold_win_reports_nothing_when_no_fold_is_there() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = manual_fold_win(&mut buf);
        let _guard = CurwinGuard::set(&mut win as *mut WinT);

        let mut done = done::DONE_NOTHING;
        // Line 5 is outside every fold.
        let next =
            unsafe { set_manual_fold_win(&mut win as *mut WinT, 5, true, false, Some(&mut done)) };

        assert_eq!(done, done::DONE_NOTHING);
        assert!(!win.w_fold_manual);
        assert_eq!(next, 20, "the next fold to try starts at 20");
    }

    #[test]
    fn fold_adjust_visual_is_a_noop_when_visual_is_inactive() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = closed_fold_win(&mut buf);
        win.w_cursor = crate::pos_defs::PosT { lnum: 22, col: 3, coladd: 0 };
        let _guard = CurwinGuard::set(&mut win as *mut WinT);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_visual = g.Visual;
        g.Visual.active = false;
        g.Visual.start = crate::pos_defs::PosT { lnum: 18, col: 1, coladd: 0 };

        unsafe { fold_adjust_visual() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(g.Visual.start.lnum, 18, "nothing is touched");
        assert_eq!(win.w_cursor.lnum, 22);
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual = prev_visual;
    }

    #[test]
    fn fold_adjust_visual_is_a_noop_without_any_folding() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = closed_fold_win(&mut buf);
        // 'nofoldenable' makes has_any_folding false.
        win.w_onebuf_opt.wo_fen = 0;
        win.w_cursor = crate::pos_defs::PosT { lnum: 22, col: 3, coladd: 0 };
        let _guard = CurwinGuard::set(&mut win as *mut WinT);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_visual = g.Visual;
        g.Visual.active = true;
        g.Visual.start = crate::pos_defs::PosT { lnum: 18, col: 1, coladd: 0 };

        unsafe { fold_adjust_visual() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(g.Visual.start.lnum, 18);
        assert_eq!(win.w_cursor.lnum, 22);
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual = prev_visual;
    }

    #[test]
    fn fold_adjust_visual_pulls_the_selection_start_back_to_the_fold_start() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = closed_fold_win(&mut buf);
        // The selection starts inside the closed fold at 20-24 and
        // ends outside it, so only the start is adjusted (leaving
        // ml_get_buf_len out of this test entirely).
        win.w_cursor = crate::pos_defs::PosT { lnum: 30, col: 0, coladd: 0 };
        let _guard = CurwinGuard::set(&mut win as *mut WinT);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_visual = g.Visual;
        g.Visual.active = true;
        g.Visual.start = crate::pos_defs::PosT { lnum: 22, col: 5, coladd: 0 };

        unsafe { fold_adjust_visual() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(g.Visual.start.lnum, 20, "pulled back to the fold's start");
        assert_eq!(g.Visual.start.col, 0, "and to the start of that line");
        assert_eq!(win.w_cursor.lnum, 30, "the end is outside any fold");
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual = prev_visual;
    }

    #[test]
    fn fold_adjust_visual_handles_the_cursor_being_the_earlier_end() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = closed_fold_win(&mut buf);
        // Selection made upwards: the cursor is the start, and the
        // Visual anchor is the end.
        win.w_cursor = crate::pos_defs::PosT { lnum: 22, col: 5, coladd: 0 };
        let _guard = CurwinGuard::set(&mut win as *mut WinT);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_visual = g.Visual;
        g.Visual.active = true;
        g.Visual.start = crate::pos_defs::PosT { lnum: 30, col: 0, coladd: 0 };

        unsafe { fold_adjust_visual() };

        assert_eq!(win.w_cursor.lnum, 20, "the cursor end is pulled back");
        assert_eq!(win.w_cursor.col, 0);
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(g.Visual.start.lnum, 30, "the anchor stays put");
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual = prev_visual;
    }

    #[test]
    fn fold_update_all_invalidates_the_window_folds() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_foldinvalid: false,
            w_folds: sibling_folds(),
            ..Default::default()
        };
        unsafe { fold_update_all(&mut win as *mut WinT) };
        assert!(win.w_foldinvalid);
        // The folds themselves are left in place; only the "these
        // need recomputing" flag changes.
        assert_eq!(win.w_folds.len(), 3);
    }

    #[test]
    fn new_fold_level_win_hands_manual_folds_back_to_foldlevel() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_foldinvalid: false,
            w_fold_manual: true,
            w_folds: sibling_folds(),
            ..Default::default()
        };
        win.w_folds[0].fd_flags = fd_flags::FD_CLOSED;
        win.w_folds[1].fd_flags = fd_flags::FD_OPEN;
        // A nested fold must NOT be touched: only the first level is
        // handed back.
        win.w_folds[2].fd_nested = vec![FoldT {
            fd_flags: fd_flags::FD_CLOSED,
            ..Default::default()
        }];

        unsafe { new_fold_level_win(&mut win as *mut WinT) };

        for fp in &win.w_folds {
            assert_eq!(fp.fd_flags, fd_flags::FD_LEVEL);
        }
        assert_eq!(win.w_folds[2].fd_nested[0].fd_flags, fd_flags::FD_CLOSED);
        assert!(!win.w_fold_manual, "no longer manually controlled");
    }

    #[test]
    fn new_fold_level_win_leaves_non_manual_folds_alone() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_foldinvalid: false,
            w_fold_manual: false,
            w_folds: sibling_folds(),
            ..Default::default()
        };
        win.w_folds[0].fd_flags = fd_flags::FD_CLOSED;

        unsafe { new_fold_level_win(&mut win as *mut WinT) };

        assert_eq!(win.w_folds[0].fd_flags, fd_flags::FD_CLOSED);
    }

    #[test]
    fn truncate_fold_cuts_the_fold_to_end_at_the_given_line() {
        let _lock = crate::globals::global_state_test_lock();
        // Fold 10-29 truncated at 19 must cover 10-19 inclusive.
        let mut fp = FoldT {
            fd_top: 10,
            fd_len: 20,
            ..Default::default()
        };
        unsafe { truncate_fold(&mut fp, 19) };
        assert_eq!(fp.fd_top, 10);
        assert_eq!(fp.fd_len, 10, "stops at 19, inclusive");
    }

    #[test]
    fn truncate_fold_drops_nested_folds_past_the_new_end() {
        let _lock = crate::globals::global_state_test_lock();
        // Nested folds at absolute 12-13 and 25-26; truncating at 19
        // must keep the first and drop the second.
        let mut fp = FoldT {
            fd_top: 10,
            fd_len: 20,
            fd_nested: vec![
                FoldT { fd_top: 2, fd_len: 2, ..Default::default() },
                FoldT { fd_top: 15, fd_len: 2, ..Default::default() },
            ],
            ..Default::default()
        };

        unsafe { truncate_fold(&mut fp, 19) };

        assert_eq!(fp.fd_len, 10);
        assert_eq!(fp.fd_nested.len(), 1);
        assert_eq!(fp.fd_nested[0].fd_top, 2);
    }

    #[test]
    fn fold_remove_is_a_noop_for_a_reversed_range() {
        let _lock = crate::globals::global_state_test_lock();
        let mut gap = sibling_folds();
        let before = gap.clone();
        unsafe { fold_remove(&mut gap, 20, 10) };
        assert_eq!(gap, before);
    }

    #[test]
    fn fold_remove_deletes_folds_contained_in_the_range() {
        let _lock = crate::globals::global_state_test_lock();
        // Case 4: fold 20-24 lies entirely within 18..26.
        let mut gap = sibling_folds();
        unsafe { fold_remove(&mut gap, 18, 26) };
        let tops: Vec<_> = gap.iter().map(|f| f.fd_top).collect();
        assert_eq!(tops, vec![10, 30], "only the contained fold goes");
    }

    #[test]
    fn fold_remove_truncates_a_fold_that_starts_above_the_range() {
        let _lock = crate::globals::global_state_test_lock();
        // Case 2: fold 10-14 starts above 12 and ends inside the
        // range, so it is cut back to end at 11.
        let mut gap = vec![FoldT { fd_top: 10, fd_len: 5, ..Default::default() }];
        unsafe { fold_remove(&mut gap, 12, 20) };
        assert_eq!(gap.len(), 1);
        assert_eq!(gap[0].fd_top, 10);
        assert_eq!(gap[0].fd_len, 2, "now covers 10-11 only");
    }

    #[test]
    fn fold_remove_splits_a_fold_spanning_the_whole_range() {
        let _lock = crate::globals::global_state_test_lock();
        // Case 3: fold 10-29 starts above 15 and ends below 19, so it
        // is split into 10-14 and 20-29.
        let mut gap = vec![FoldT { fd_top: 10, fd_len: 20, ..Default::default() }];
        unsafe { fold_remove(&mut gap, 15, 19) };
        assert_eq!(gap.len(), 2);
        assert_eq!((gap[0].fd_top, gap[0].fd_len), (10, 5));
        assert_eq!((gap[1].fd_top, gap[1].fd_len), (20, 10));
    }

    #[test]
    fn fold_remove_moves_a_fold_that_ends_below_the_range() {
        let _lock = crate::globals::global_state_test_lock();
        // Case 5: fold 20-29 starts inside 18..24 but ends below it,
        // so it is moved to start just after bot.
        let mut gap = vec![FoldT { fd_top: 20, fd_len: 10, ..Default::default() }];
        unsafe { fold_remove(&mut gap, 18, 24) };
        assert_eq!(gap.len(), 1);
        assert_eq!(gap[0].fd_top, 25, "starts at bot + 1");
        assert_eq!(gap[0].fd_len, 5, "10 - (24 - 20 + 1)");
    }

    #[test]
    fn fold_remove_stops_at_the_first_fold_below_the_range() {
        let _lock = crate::globals::global_state_test_lock();
        // Case 6: nothing overlaps 15..17, so every fold survives.
        let mut gap = sibling_folds();
        let before = gap.clone();
        unsafe { fold_remove(&mut gap, 15, 17) };
        assert_eq!(gap, before);
    }

    #[test]
    fn fold_remove_recurses_into_nested_folds() {
        let _lock = crate::globals::global_state_test_lock();
        // The outer fold 10-29 survives (truncated), but its nested
        // fold at absolute 22-26 lies inside the removed range and
        // must go with it.
        let mut gap = vec![FoldT {
            fd_top: 10,
            fd_len: 20,
            fd_nested: vec![
                FoldT { fd_top: 2, fd_len: 2, ..Default::default() },
                FoldT { fd_top: 12, fd_len: 5, ..Default::default() },
            ],
            ..Default::default()
        }];

        unsafe { fold_remove(&mut gap, 20, 29) };

        assert_eq!(gap.len(), 1);
        assert_eq!(gap[0].fd_len, 10, "truncated to 10-19");
        // Only the nested fold above the removed range remains.
        assert_eq!(gap[0].fd_nested.len(), 1);
        assert_eq!(gap[0].fd_nested[0].fd_top, 2);
    }

    #[test]
    fn fold_merge_joins_two_adjacent_folds() {
        let _lock = crate::globals::global_state_test_lock();
        // Folds 10-14 and 15-19, adjacent.
        let mut fp1 = FoldT {
            fd_top: 10,
            fd_len: 5,
            ..Default::default()
        };
        let mut gap = vec![FoldT {
            fd_top: 15,
            fd_len: 5,
            ..Default::default()
        }];

        fold_merge(&mut fp1, &mut gap, 0);

        assert_eq!(fp1.fd_len, 10, "the merged fold spans both");
        assert_eq!(fp1.fd_top, 10);
        assert!(gap.is_empty(), "the second fold is removed");
    }

    #[test]
    fn fold_merge_moves_nested_folds_and_rebases_them() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fp1 = FoldT {
            fd_top: 10,
            fd_len: 5,
            fd_nested: vec![FoldT { fd_top: 1, fd_len: 1, ..Default::default() }],
            ..Default::default()
        };
        // fp2's child sits at its own line 2; once fp2's lines belong
        // to fp1, that child must be measured from fp1 instead, i.e.
        // shifted by fp1's original length of 5.
        let mut gap = vec![FoldT {
            fd_top: 15,
            fd_len: 5,
            fd_nested: vec![FoldT { fd_top: 2, fd_len: 1, ..Default::default() }],
            ..Default::default()
        }];

        fold_merge(&mut fp1, &mut gap, 0);

        assert_eq!(fp1.fd_nested.len(), 2);
        assert_eq!(fp1.fd_nested[0].fd_top, 1, "fp1's own child is unmoved");
        assert_eq!(fp1.fd_nested[1].fd_top, 7, "2 + 5");
        // Absolute position is preserved: 10 + 7 == 15 + 2.
        assert_eq!(fp1.fd_top + fp1.fd_nested[1].fd_top, 17);
    }

    #[test]
    fn fold_merge_recursively_merges_nested_folds_that_touch() {
        let _lock = crate::globals::global_state_test_lock();
        // fp1's last child ends exactly where fp2's first child
        // begins, so the two must be merged into one.
        let mut fp1 = FoldT {
            fd_top: 10,
            fd_len: 5,
            fd_nested: vec![FoldT { fd_top: 3, fd_len: 2, ..Default::default() }],
            ..Default::default()
        };
        let mut gap = vec![FoldT {
            fd_top: 15,
            fd_len: 5,
            fd_nested: vec![FoldT { fd_top: 0, fd_len: 2, ..Default::default() }],
            ..Default::default()
        }];

        fold_merge(&mut fp1, &mut gap, 0);

        assert_eq!(fp1.fd_len, 10);
        assert_eq!(fp1.fd_nested.len(), 1, "the touching children merged");
        assert_eq!(fp1.fd_nested[0].fd_top, 3);
        assert_eq!(fp1.fd_nested[0].fd_len, 4, "2 + 2");
    }

    #[test]
    fn fold_merge_leaves_non_touching_nested_folds_separate() {
        let _lock = crate::globals::global_state_test_lock();
        // fp1's child ends well before fp1 does, so nothing touches
        // across the join.
        let mut fp1 = FoldT {
            fd_top: 10,
            fd_len: 5,
            fd_nested: vec![FoldT { fd_top: 0, fd_len: 1, ..Default::default() }],
            ..Default::default()
        };
        let mut gap = vec![FoldT {
            fd_top: 15,
            fd_len: 5,
            fd_nested: vec![FoldT { fd_top: 3, fd_len: 1, ..Default::default() }],
            ..Default::default()
        }];

        fold_merge(&mut fp1, &mut gap, 0);

        assert_eq!(fp1.fd_nested.len(), 2, "both children survive");
        assert_eq!(fp1.fd_nested[1].fd_top, 8, "3 + 5");
    }

    #[test]
    fn fold_merge_preserves_sibling_folds_and_reports_the_change() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *FOLD_CHANGED.get_mut() = false };
        let mut fp1 = FoldT {
            fd_top: 10,
            fd_len: 5,
            ..Default::default()
        };
        let mut gap = vec![
            FoldT { fd_top: 15, fd_len: 5, ..Default::default() },
            FoldT { fd_top: 40, fd_len: 3, ..Default::default() },
        ];

        fold_merge(&mut fp1, &mut gap, 0);

        assert_eq!(gap.len(), 1);
        assert_eq!(gap[0].fd_top, 40, "the later sibling is untouched");
        assert!(unsafe { *FOLD_CHANGED.get_mut() });
        unsafe { *FOLD_CHANGED.get_mut() = false };
    }

    #[test]
    fn fold_split_divides_a_fold_around_the_given_range() {
        let _lock = crate::globals::global_state_test_lock();
        // One fold covering lines 10-29 (fd_top 10, fd_len 20), split
        // around lines 15..19: the first half must end above 15 and
        // the second must start below 19.
        let mut gap = vec![FoldT {
            fd_top: 10,
            fd_len: 20,
            fd_flags: fd_flags::FD_CLOSED,
            fd_small: crate::types_defs::TriState::True,
            ..Default::default()
        }];

        fold_split(&mut gap, 0, 15, 19);

        assert_eq!(gap.len(), 2);
        // First half: 10..14, i.e. top - fd_top = 5 lines.
        assert_eq!(gap[0].fd_top, 10);
        assert_eq!(gap[0].fd_len, 5);
        // Second half starts at bot + 1 and runs to the old end.
        assert_eq!(gap[1].fd_top, 20);
        assert_eq!(gap[1].fd_len, 10);
        // The flags carry over, but neither half's smallness is known
        // any more, since neither covers the same lines as before.
        assert_eq!(gap[1].fd_flags, fd_flags::FD_CLOSED);
        assert_eq!(gap[0].fd_small, crate::types_defs::TriState::None);
        assert_eq!(gap[1].fd_small, crate::types_defs::TriState::None);
    }

    #[test]
    fn fold_split_moves_trailing_nested_folds_to_the_second_half() {
        let _lock = crate::globals::global_state_test_lock();
        // Outer fold 10-29 with nested folds at absolute 12-13 and
        // 22-23 (fd_top 2 and 12, relative to the parent).
        let mut gap = vec![FoldT {
            fd_top: 10,
            fd_len: 20,
            fd_nested: vec![
                FoldT { fd_top: 2, fd_len: 2, ..Default::default() },
                FoldT { fd_top: 12, fd_len: 2, ..Default::default() },
            ],
            ..Default::default()
        }];

        fold_split(&mut gap, 0, 15, 19);

        // The nested fold before the split stays put.
        assert_eq!(gap[0].fd_nested.len(), 1);
        assert_eq!(gap[0].fd_nested[0].fd_top, 2);
        // The one after moves across and is rebased onto the new
        // parent, which starts 10 lines later: 12 - 10 = 2, still
        // absolute line 22.
        assert_eq!(gap[1].fd_nested.len(), 1);
        assert_eq!(gap[1].fd_nested[0].fd_top, 2);
        assert_eq!(gap[1].fd_top + gap[1].fd_nested[0].fd_top, 22);
    }

    #[test]
    fn fold_split_leaves_a_childless_fold_with_two_childless_halves() {
        let _lock = crate::globals::global_state_test_lock();
        let mut gap = vec![FoldT {
            fd_top: 10,
            fd_len: 20,
            ..Default::default()
        }];
        fold_split(&mut gap, 0, 15, 19);
        assert!(gap[0].fd_nested.is_empty());
        assert!(gap[1].fd_nested.is_empty());
    }

    #[test]
    fn fold_split_preserves_sibling_folds_around_it() {
        let _lock = crate::globals::global_state_test_lock();
        let mut gap = vec![
            FoldT { fd_top: 1, fd_len: 2, ..Default::default() },
            FoldT { fd_top: 10, fd_len: 20, ..Default::default() },
            FoldT { fd_top: 40, fd_len: 5, ..Default::default() },
        ];

        fold_split(&mut gap, 1, 15, 19);

        assert_eq!(gap.len(), 4);
        let tops: Vec<_> = gap.iter().map(|f| f.fd_top).collect();
        // The new half is inserted directly after the fold it came
        // from, keeping the array sorted.
        assert_eq!(tops, vec![1, 10, 20, 40]);
    }

    #[test]
    fn fold_split_reports_the_fold_structure_changed() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *FOLD_CHANGED.get_mut() = false };
        let mut gap = vec![FoldT {
            fd_top: 10,
            fd_len: 20,
            ..Default::default()
        }];
        fold_split(&mut gap, 0, 15, 19);
        assert!(unsafe { *FOLD_CHANGED.get_mut() });
        unsafe { *FOLD_CHANGED.get_mut() = false };
    }

    #[test]
    fn fold_check_close_is_a_noop_when_foldclose_is_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fdl: 0,
                ..Default::default()
            },
            w_foldinvalid: false,
            w_cursor: crate::pos_defs::PosT { lnum: 22, col: 0, coladd: 0 },
            w_folds: sibling_folds(),
            ..Default::default()
        };
        for fp in &mut win.w_folds {
            fp.fd_flags = fd_flags::FD_OPEN;
        }
        let _guard = CurwinGuard::set(&mut win as *mut WinT);

        let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let previous = ov.p_fcl.take();
        unsafe { fold_check_close() };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_fcl = previous;

        // 'foldclose' empty means never close anything.
        for fp in &win.w_folds {
            assert_eq!(fp.fd_flags, fd_flags::FD_OPEN);
        }
    }

    #[test]
    fn fold_check_close_closes_folds_not_containing_the_cursor() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 40;
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fdl: 0,
                ..Default::default()
            },
            w_foldinvalid: false,
            w_cursor: crate::pos_defs::PosT { lnum: 22, col: 0, coladd: 0 },
            w_folds: sibling_folds(),
            ..Default::default()
        };
        for fp in &mut win.w_folds {
            fp.fd_flags = fd_flags::FD_OPEN;
        }
        let _guard = CurwinGuard::set(&mut win as *mut WinT);

        let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let previous = ov.p_fcl.replace(b"all".to_vec());
        unsafe { fold_check_close() };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_fcl = previous;

        // The cursor is on line 22, inside the middle fold only.
        assert_eq!(win.w_folds[0].fd_flags, fd_flags::FD_LEVEL);
        assert_eq!(win.w_folds[1].fd_flags, fd_flags::FD_OPEN);
        assert_eq!(win.w_folds[2].fd_flags, fd_flags::FD_LEVEL);
    }
    #[test]
    fn fold_init_win_leaves_a_new_window_with_no_folds() {
        let mut win = WinT {
            w_folds: sibling_folds(),
            ..Default::default()
        };
        fold_init_win(&mut win);
        assert!(win.w_folds.is_empty());
    }

    #[test]
    fn copy_folding_state_deep_copies_the_folds_and_the_flags() {
        let from = WinT {
            w_fold_manual: true,
            w_foldinvalid: true,
            w_folds: nested_outer_inner(),
            ..Default::default()
        };
        let mut to = WinT::default();

        copy_folding_state(&from, &mut to);

        assert!(to.w_fold_manual);
        assert!(to.w_foldinvalid);
        assert_eq!(to.w_folds, from.w_folds);
        // The copy must be independent, including nested folds.
        to.w_folds[0].fd_top = 999;
        to.w_folds[0].fd_nested[0].fd_len = 999;
        assert_eq!(from.w_folds[0].fd_top, 10);
        assert_eq!(from.w_folds[0].fd_nested[0].fd_len, 5);
    }

    #[test]
    fn copy_folding_state_from_a_window_with_no_folds_clears_the_target() {
        let from = WinT::default();
        let mut to = WinT {
            w_fold_manual: true,
            w_foldinvalid: true,
            w_folds: sibling_folds(),
            ..Default::default()
        };
        copy_folding_state(&from, &mut to);
        assert!(!to.w_fold_manual);
        assert!(!to.w_foldinvalid);
        assert!(to.w_folds.is_empty());
    }

    #[test]
    fn fold_reverse_order_reverses_the_given_span_only() {
        let mut gap = vec![
            FoldT { fd_top: 1, ..Default::default() },
            FoldT { fd_top: 2, ..Default::default() },
            FoldT { fd_top: 3, ..Default::default() },
            FoldT { fd_top: 4, ..Default::default() },
        ];
        fold_reverse_order(&mut gap, 1, 2);
        let tops: Vec<_> = gap.iter().map(|f| f.fd_top).collect();
        assert_eq!(tops, vec![1, 3, 2, 4]);

        fold_reverse_order(&mut gap, 0, 3);
        let tops: Vec<_> = gap.iter().map(|f| f.fd_top).collect();
        assert_eq!(tops, vec![4, 2, 3, 1]);
    }

    #[test]
    fn fold_reverse_order_is_a_noop_for_a_degenerate_span() {
        let mut gap = sibling_folds();
        let before: Vec<_> = gap.iter().map(|f| f.fd_top).collect();
        // start == end and start > end both leave the array alone,
        // matching the original's `for (; start < end; ...)` guard.
        fold_reverse_order(&mut gap, 1, 1);
        fold_reverse_order(&mut gap, 2, 1);
        let after: Vec<_> = gap.iter().map(|f| f.fd_top).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn fold_insert_shifts_existing_folds_up() {
        let mut gap = sibling_folds();
        fold_insert(&mut gap, 1);
        assert_eq!(gap.len(), 4);
        // The new fold is empty and the old ones keep their order.
        assert_eq!(gap[1], FoldT::default());
        let tops: Vec<_> = gap.iter().map(|f| f.fd_top).collect();
        assert_eq!(tops, vec![10, 0, 20, 30]);
    }

    #[test]
    fn fold_insert_at_the_end_appends() {
        let mut gap = sibling_folds();
        fold_insert(&mut gap, 3);
        assert_eq!(gap.len(), 4);
        assert_eq!(gap[3], FoldT::default());
        assert_eq!(gap[2].fd_top, 30);
    }

    #[test]
    fn fold_insert_into_an_empty_array_creates_the_first_fold() {
        let mut gap: Vec<FoldT> = Vec::new();
        fold_insert(&mut gap, 0);
        assert_eq!(gap.len(), 1);
        assert!(gap[0].fd_nested.is_empty());
    }

    #[test]
    fn check_close_rec_on_an_empty_array_reports_nothing_closed() {
        assert!(!check_close_rec(&mut [], 5, 0));
    }

    /// A window with a single closed fold over lines 20-24, plus a
    /// buffer long enough that `last` is never clamped.
    fn closed_fold_win(buf: &mut BufT) -> WinT {
        buf.b_ml.ml_line_count = 40;
        WinT {
            w_buffer: buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fml: 0,
                wo_fdl: 99,
                ..Default::default()
            },
            w_foldinvalid: false,
            w_folds: vec![FoldT {
                fd_top: 20,
                fd_len: 5,
                fd_flags: fd_flags::FD_CLOSED,
                fd_small: crate::types_defs::TriState::False,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn has_folding_win_matches_real_nvim_foldclosed() {
        // Cross-verified against real nvim: with :20,24fold closed,
        // foldclosed() is 20 and foldclosedend() is 24 for any line
        // inside, and -1 for lines outside.
        let mut buf = BufT::default();
        let mut win = closed_fold_win(&mut buf);

        for lnum in [20, 22, 24] {
            let (mut first, mut last) = (0, 0);
            assert!(
                unsafe { has_folding(&mut win, lnum, Some(&mut first), Some(&mut last)) },
                "line {lnum} should be in a closed fold"
            );
            assert_eq!(first, 20, "line {lnum}");
            assert_eq!(last, 24, "line {lnum}");
        }

        for lnum in [19, 25] {
            let (mut first, mut last) = (0, 0);
            assert!(!unsafe {
                has_folding(&mut win, lnum, Some(&mut first), Some(&mut last))
            });
        }
    }

    #[test]
    fn has_folding_win_reports_false_for_an_open_fold_but_fills_infop() {
        let mut buf = BufT::default();
        let mut win = closed_fold_win(&mut buf);
        win.w_folds[0].fd_flags = fd_flags::FD_OPEN;

        let mut info = crate::fold_defs::FoldinfoT::default();
        assert!(!unsafe { has_folding_win(&mut win, 22, None, None, false, Some(&mut info)) });
        // The line is inside one open fold, so it is at level 1 even
        // though nothing is folded away.
        assert_eq!(info.fi_level, 1);
    }

    #[test]
    fn has_folding_win_reports_level_and_low_level_for_a_closed_fold() {
        let mut buf = BufT::default();
        let mut win = closed_fold_win(&mut buf);

        let mut info = crate::fold_defs::FoldinfoT::default();
        assert!(unsafe { has_folding_win(&mut win, 22, None, None, false, Some(&mut info)) });
        assert_eq!(info.fi_level, 1);
        assert_eq!(info.fi_lnum, 20, "the fold starts here");
        assert_eq!(info.fi_low_level, 1);
    }

    #[test]
    fn has_folding_win_clamps_last_to_the_buffer_line_count() {
        let mut buf = BufT::default();
        let mut win = closed_fold_win(&mut buf);
        // A fold running past the end of the buffer must not report a
        // last line beyond it.
        win.w_folds[0].fd_len = 100;
        unsafe { (*win.w_buffer).b_ml.ml_line_count = 22 };

        let mut last = 0;
        assert!(unsafe { has_folding(&mut win, 21, None, Some(&mut last)) });
        assert_eq!(last, 22);
    }

    #[test]
    fn has_folding_win_uses_the_displayed_line_cache_when_asked() {
        let mut buf = BufT::default();
        let mut win = closed_fold_win(&mut buf);
        // An empty w_folds would normally mean "no fold here", but a
        // valid cache entry answers first, which is the whole point
        // of the cache parameter.
        win.w_lines = vec![crate::buffer_defs::WlineT {
            wl_lnum: 5,
            wl_foldend: 9,
            wl_folded: true,
            wl_valid: true,
            ..Default::default()
        }];
        win.w_lines_valid = 1;

        let (mut first, mut last) = (0, 0);
        assert!(unsafe { has_folding(&mut win, 5, Some(&mut first), Some(&mut last)) });
        assert_eq!((first, last), (5, 9));

        // With cache disabled the same lookup falls through to the
        // real fold tree, which has nothing at line 5.
        assert!(!unsafe { has_folding_win(&mut win, 5, None, None, false, None) });
    }

    #[test]
    fn has_folding_win_still_reports_nothing_when_the_window_has_no_folds() {
        let mut buf = BufT::default();
        let mut win = closed_fold_win(&mut buf);
        win.w_folds.clear();

        let mut info = crate::fold_defs::FoldinfoT::default();
        assert!(!unsafe { has_folding_win(&mut win, 22, None, None, true, Some(&mut info)) });
        assert_eq!(info.fi_level, 0);
    }

    /// A window whose buffer has `lines` single-screen-line lines, so
    /// `plines_win_nofold` reports 1 per buffer line and `check_small`
    /// can count real screen lines.
    fn small_check_win(fml: crate::types_defs::OptInt, fdl: crate::types_defs::OptInt) -> (BufT, WinT) {
        let buf = BufT::default();
        let win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fml: fml,
                wo_fdl: fdl,
                ..Default::default()
            },
            ..Default::default()
        };
        (buf, win)
    }

    #[test]
    fn check_small_marks_a_long_fold_as_not_small_without_counting() {
        let (mut buf, mut win) = small_check_win(3, 0);
        win.w_buffer = &mut buf as *mut BufT;
        let win_ptr = &mut win as *mut WinT;
        // fd_len 5 > foldminlines 3, so the cheap early test settles
        // it and no screen lines are counted at all.
        let mut fp = FoldT {
            fd_top: 20,
            fd_len: 5,
            ..Default::default()
        };
        unsafe { check_small(win_ptr, &mut fp, 0) };
        assert_eq!(fp.fd_small, crate::types_defs::TriState::False);
    }

    #[test]
    fn check_small_leaves_an_already_known_smallness_alone() {
        let (mut buf, mut win) = small_check_win(3, 0);
        win.w_buffer = &mut buf as *mut BufT;
        let win_ptr = &mut win as *mut WinT;
        for known in [crate::types_defs::TriState::True, crate::types_defs::TriState::False] {
            let mut fp = FoldT {
                fd_top: 20,
                fd_len: 99,
                fd_small: known,
                ..Default::default()
            };
            unsafe { check_small(win_ptr, &mut fp, 0) };
            assert_eq!(fp.fd_small, known);
        }
    }

    #[test]
    fn check_small_marks_nested_folds_maybe_small() {
        let (mut buf, mut win) = small_check_win(3, 0);
        win.w_buffer = &mut buf as *mut BufT;
        let win_ptr = &mut win as *mut WinT;
        let mut fp = FoldT {
            fd_top: 20,
            fd_len: 5,
            fd_nested: vec![FoldT {
                fd_small: crate::types_defs::TriState::True,
                ..Default::default()
            }],
            ..Default::default()
        };
        unsafe { check_small(win_ptr, &mut fp, 0) };
        // The parent's smallness changed, so its children's cached
        // answers are no longer trustworthy.
        assert_eq!(
            fp.fd_nested[0].fd_small,
            crate::types_defs::TriState::None
        );
    }

    #[test]
    fn check_closed_honours_fd_closed_and_fd_open() {
        let (mut buf, mut win) = small_check_win(0, 0);
        win.w_buffer = &mut buf as *mut BufT;
        let win_ptr = &mut win as *mut WinT;

        let mut closed_fold = FoldT {
            fd_top: 20,
            fd_len: 5,
            fd_flags: fd_flags::FD_CLOSED,
            // Known not-small, so smallness cannot reopen it.
            fd_small: crate::types_defs::TriState::False,
            ..Default::default()
        };
        let (mut use_level, mut maybe_small) = (false, false);
        assert!(unsafe {
            check_closed(win_ptr, &mut closed_fold, &mut use_level, 0, &mut maybe_small, 0)
        });
        assert!(!use_level, "FD_CLOSED does not switch to level mode");

        let mut open_fold = FoldT {
            fd_flags: fd_flags::FD_OPEN,
            fd_small: crate::types_defs::TriState::False,
            ..Default::default()
        };
        let (mut use_level, mut maybe_small) = (false, false);
        assert!(!unsafe {
            check_closed(win_ptr, &mut open_fold, &mut use_level, 0, &mut maybe_small, 0)
        });
    }

    #[test]
    fn check_closed_uses_foldlevel_once_fd_level_is_seen() {
        let (mut buf, mut win) = small_check_win(0, 2);
        win.w_buffer = &mut buf as *mut BufT;
        let win_ptr = &mut win as *mut WinT;

        let mut fp = FoldT {
            fd_flags: fd_flags::FD_LEVEL,
            fd_small: crate::types_defs::TriState::False,
            ..Default::default()
        };
        // level 1 < 'foldlevel' 2: stays open, but use_level is now
        // set so nested folds inherit level control.
        let (mut use_level, mut maybe_small) = (false, false);
        assert!(!unsafe {
            check_closed(win_ptr, &mut fp, &mut use_level, 1, &mut maybe_small, 0)
        });
        assert!(use_level);

        // level 2 >= 'foldlevel' 2: closed.
        let (mut use_level, mut maybe_small) = (false, false);
        assert!(unsafe {
            check_closed(win_ptr, &mut fp, &mut use_level, 2, &mut maybe_small, 0)
        });
    }

    #[test]
    fn check_closed_inherits_level_mode_from_an_enclosing_fold() {
        let (mut buf, mut win) = small_check_win(0, 5);
        win.w_buffer = &mut buf as *mut BufT;
        let win_ptr = &mut win as *mut WinT;

        // FD_CLOSED would normally close this fold outright, but an
        // enclosing FD_LEVEL fold means 'foldlevel' decides instead -
        // and level 1 is below 'foldlevel' 5, so it stays open.
        let mut fp = FoldT {
            fd_flags: fd_flags::FD_CLOSED,
            fd_small: crate::types_defs::TriState::False,
            ..Default::default()
        };
        let (mut use_level, mut maybe_small) = (true, false);
        assert!(!unsafe {
            check_closed(win_ptr, &mut fp, &mut use_level, 1, &mut maybe_small, 0)
        });
    }

    #[test]
    fn check_closed_reports_an_unknown_smallness_upward() {
        let (mut buf, mut win) = small_check_win(0, 0);
        win.w_buffer = &mut buf as *mut BufT;
        let win_ptr = &mut win as *mut WinT;

        let mut fp = FoldT {
            fd_top: 20,
            fd_len: 5,
            fd_flags: fd_flags::FD_OPEN,
            fd_small: crate::types_defs::TriState::None,
            ..Default::default()
        };
        let (mut use_level, mut maybe_small) = (false, false);
        let _ = unsafe { check_closed(win_ptr, &mut fp, &mut use_level, 0, &mut maybe_small, 0) };
        // kNone applies to nested folds too, so it must propagate.
        assert!(maybe_small);
    }

    /// Points `GLOBALS.curwin` at `win` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime.
    struct CurwinGuard {
        previous: *mut WinT,
    }

    impl CurwinGuard {
        fn set(new_curwin: *mut WinT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = new_curwin;
            CurwinGuard { previous }
        }
    }

    impl Drop for CurwinGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.previous;
        }
    }

    #[test]
    fn fold_manual_allowed_true_for_manual_foldmethod() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fdm: Some(b"manual".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        let _guard = CurwinGuard::set(&mut win as *mut WinT);
        assert!(unsafe { fold_manual_allowed(true) });
        assert!(unsafe { fold_manual_allowed(false) });
    }

    #[test]
    fn fold_manual_allowed_true_for_marker_foldmethod() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fdm: Some(b"marker".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        let _guard = CurwinGuard::set(&mut win as *mut WinT);
        assert!(unsafe { fold_manual_allowed(true) });
    }

    #[test]
    fn fold_manual_allowed_false_for_indent_foldmethod() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fdm: Some(b"indent".to_vec()),
                ..Default::default()
            },
            ..Default::default()
        };
        let _guard = CurwinGuard::set(&mut win as *mut WinT);
        assert!(!unsafe { fold_manual_allowed(true) });
        assert!(!unsafe { fold_manual_allowed(false) });
    }

    /// Builds a two-level fold tree: one outer fold with two nested
    /// folds, the second of which itself has one nested fold.
    fn nested_fold_tree() -> Vec<FoldT> {
        vec![FoldT {
            fd_top: 10,
            fd_len: 20,
            fd_flags: fd_flags::FD_CLOSED,
            fd_small: crate::types_defs::TriState::True,
            fd_nested: vec![
                FoldT {
                    fd_top: 2,
                    fd_len: 3,
                    ..Default::default()
                },
                FoldT {
                    fd_top: 8,
                    fd_len: 5,
                    fd_flags: fd_flags::FD_LEVEL,
                    fd_nested: vec![FoldT {
                        fd_top: 1,
                        fd_len: 2,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
        }]
    }

    #[test]
    fn fold_t_defaults_to_an_open_leaf_fold_with_unknown_smallness() {
        let fold = FoldT::default();
        assert_eq!(fold.fd_top, 0);
        assert_eq!(fold.fd_len, 0);
        assert!(fold.fd_nested.is_empty());
        assert_eq!(fold.fd_flags, fd_flags::FD_OPEN);
        // kNone is the "applies to nested folds too" state, and is what
        // a freshly created fold starts in.
        assert_eq!(fold.fd_small, crate::types_defs::TriState::None);
    }

    #[test]
    fn clone_fold_grow_array_copies_every_level_of_nesting() {
        let from = nested_fold_tree();
        let to = clone_fold_grow_array(&from);

        assert_eq!(to, from, "the copy must equal the original");
        assert_eq!(to[0].fd_top, 10);
        assert_eq!(to[0].fd_flags, fd_flags::FD_CLOSED);
        assert_eq!(to[0].fd_small, crate::types_defs::TriState::True);
        // Two levels down is the part a shallow copy would get wrong.
        assert_eq!(to[0].fd_nested.len(), 2);
        assert_eq!(to[0].fd_nested[1].fd_nested.len(), 1);
        assert_eq!(to[0].fd_nested[1].fd_nested[0].fd_top, 1);
    }

    #[test]
    fn clone_fold_grow_array_produces_an_independent_copy() {
        let mut from = nested_fold_tree();
        let to = clone_fold_grow_array(&from);

        // Mutating the source at every depth must not disturb the
        // copy - the whole reason the original cannot just memcpy.
        from[0].fd_top = 999;
        from[0].fd_nested[0].fd_len = 999;
        from[0].fd_nested[1].fd_nested[0].fd_top = 999;
        from[0].fd_nested.push(FoldT::default());

        assert_eq!(to[0].fd_top, 10);
        assert_eq!(to[0].fd_nested.len(), 2);
        assert_eq!(to[0].fd_nested[0].fd_len, 3);
        assert_eq!(to[0].fd_nested[1].fd_nested[0].fd_top, 1);
    }

    #[test]
    fn clone_fold_grow_array_of_an_empty_array_is_empty() {
        // The original's own `if (GA_EMPTY(from)) return;` leaves the
        // destination as ga_init left it, i.e. empty.
        let from: Vec<FoldT> = Vec::new();
        assert!(clone_fold_grow_array(&from).is_empty());
    }

    #[test]
    fn delete_fold_recurse_empties_the_whole_tree() {
        let mut gap = nested_fold_tree();
        assert!(!gap.is_empty());
        delete_fold_recurse(&mut gap);
        assert!(gap.is_empty());
        // Idempotent, like the original on an already-cleared array.
        delete_fold_recurse(&mut gap);
        assert!(gap.is_empty());
    }

    #[test]
    fn clear_folding_removes_every_fold_and_revalidates_the_window() {
        let mut win = WinT {
            w_folds: nested_fold_tree(),
            w_foldinvalid: true,
            ..Default::default()
        };
        clear_folding(&mut win);
        assert!(win.w_folds.is_empty());
        assert!(!win.w_foldinvalid, "the fold state is valid once empty");
    }

    #[test]
    fn clear_folding_on_a_window_with_no_folds_still_clears_foldinvalid() {
        let mut win = WinT {
            w_foldinvalid: true,
            ..Default::default()
        };
        clear_folding(&mut win);
        assert!(win.w_folds.is_empty());
        assert!(!win.w_foldinvalid);
    }

    #[test]
    fn set_small_maybe_resets_only_the_top_level() {
        let mut gap = nested_fold_tree();
        gap[0].fd_small = crate::types_defs::TriState::True;
        gap[0].fd_nested[0].fd_small = crate::types_defs::TriState::False;

        set_small_maybe(&mut gap);

        assert_eq!(gap[0].fd_small, crate::types_defs::TriState::None);
        // kNone already means "applies to nested folds too", so the
        // original deliberately does not walk into them.
        assert_eq!(
            gap[0].fd_nested[0].fd_small,
            crate::types_defs::TriState::False
        );
    }

    #[test]
    fn set_small_maybe_on_an_empty_array_is_a_noop() {
        let mut gap: Vec<FoldT> = Vec::new();
        set_small_maybe(&mut gap);
        assert!(gap.is_empty());
    }

    /// Three sibling folds covering lines 10-14, 20-24 and 30-34,
    /// with the gaps between them deliberately left unfolded.
    fn sibling_folds() -> Vec<FoldT> {
        vec![
            FoldT {
                fd_top: 10,
                fd_len: 5,
                ..Default::default()
            },
            FoldT {
                fd_top: 20,
                fd_len: 5,
                ..Default::default()
            },
            FoldT {
                fd_top: 30,
                fd_len: 5,
                ..Default::default()
            },
        ]
    }

    #[test]
    fn fold_find_on_an_empty_array_finds_nothing_at_index_zero() {
        assert_eq!(fold_find(&[], 5), (false, 0));
    }

    #[test]
    fn fold_find_locates_the_fold_containing_a_line() {
        let gap = sibling_folds();
        // First and last line of each fold, plus one in the middle.
        assert_eq!(fold_find(&gap, 10), (true, 0));
        assert_eq!(fold_find(&gap, 14), (true, 0));
        assert_eq!(fold_find(&gap, 22), (true, 1));
        assert_eq!(fold_find(&gap, 30), (true, 2));
        assert_eq!(fold_find(&gap, 34), (true, 2));
    }

    #[test]
    fn fold_find_reports_the_next_fold_below_an_unfolded_line() {
        let gap = sibling_folds();
        // Before every fold.
        assert_eq!(fold_find(&gap, 1), (false, 0));
        // fd_top + fd_len is exclusive, so 15 is past the first fold.
        assert_eq!(fold_find(&gap, 15), (false, 1));
        assert_eq!(fold_find(&gap, 19), (false, 1));
        assert_eq!(fold_find(&gap, 25), (false, 2));
    }

    #[test]
    fn fold_find_reports_one_past_the_end_below_the_last_fold() {
        let gap = sibling_folds();
        // The original's own doc comment warns this index "can be
        // beyond the end of the array"; callers use it as the
        // insertion point, so it must be exactly len.
        assert_eq!(fold_find(&gap, 35), (false, 3));
        assert_eq!(fold_find(&gap, 9999), (false, 3));
        assert_eq!(gap.len(), 3);
    }

    #[test]
    fn fold_find_handles_a_single_fold_on_every_side() {
        let gap = vec![FoldT {
            fd_top: 5,
            fd_len: 2,
            ..Default::default()
        }];
        assert_eq!(fold_find(&gap, 4), (false, 0));
        assert_eq!(fold_find(&gap, 5), (true, 0));
        assert_eq!(fold_find(&gap, 6), (true, 0));
        assert_eq!(fold_find(&gap, 7), (false, 1));
    }

    #[test]
    fn fold_find_skips_zero_length_folds() {
        // fd_top + fd_len <= lnum is the "fold above lnum" test, so a
        // zero-length fold can never contain anything.
        let gap = vec![FoldT {
            fd_top: 5,
            fd_len: 0,
            ..Default::default()
        }];
        assert_eq!(fold_find(&gap, 5), (false, 1));
    }

    /// Mirrors the layout cross-checked against real nvim: an outer
    /// fold over lines 10-24 with a nested fold over lines 14-18.
    /// The nested fold's `fd_top` is relative to its parent, so 14
    /// becomes 14 - 10 = 4.
    fn nested_outer_inner() -> Vec<FoldT> {
        vec![FoldT {
            fd_top: 10,
            fd_len: 15,
            fd_nested: vec![FoldT {
                fd_top: 4,
                fd_len: 5,
                ..Default::default()
            }],
            ..Default::default()
        }]
    }

    #[test]
    fn fold_level_win_matches_real_nvim_foldlevel() {
        // Cross-verified against real nvim with an identical buffer:
        // :14,18fold then :10,24fold gives foldlevel() 0 outside,
        // 1 in the outer fold, and 2 inside the nested one.
        let win = WinT {
            w_folds: nested_outer_inner(),
            ..Default::default()
        };
        for (lnum, level) in [
            (5, 0),
            (10, 1),
            (12, 1),
            (14, 2),
            (16, 2),
            (18, 2),
            (19, 1),
            (24, 1),
            (25, 0),
        ] {
            assert_eq!(fold_level_win(&win, lnum), level, "line {lnum}");
        }
    }

    #[test]
    fn fold_level_win_is_zero_when_the_window_has_no_folds() {
        let win = WinT::default();
        assert_eq!(fold_level_win(&win, 1), 0);
        assert_eq!(fold_level_win(&win, 100), 0);
    }

    #[test]
    fn fold_level_win_counts_three_levels_of_nesting() {
        let win = WinT {
            w_folds: vec![FoldT {
                fd_top: 10,
                fd_len: 20,
                fd_nested: vec![FoldT {
                    // Relative to the outer fold: absolute line 15.
                    fd_top: 5,
                    fd_len: 10,
                    fd_nested: vec![FoldT {
                        // Relative to its parent: absolute line 18.
                        fd_top: 3,
                        fd_len: 4,
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(fold_level_win(&win, 12), 1);
        assert_eq!(fold_level_win(&win, 16), 2);
        assert_eq!(fold_level_win(&win, 19), 3);
        // Past the innermost fold but still inside the middle one.
        assert_eq!(fold_level_win(&win, 23), 2);
    }

    #[test]
    fn delete_fold_entry_recursive_removes_the_whole_subtree() {
        let mut gap = nested_outer_inner();
        assert_eq!(gap.len(), 1);
        delete_fold_entry(&mut gap, 0, true);
        assert!(gap.is_empty(), "the nested fold goes with its parent");
    }

    #[test]
    fn delete_fold_entry_promotes_nested_folds_like_real_nvim_zd() {
        // Cross-verified against real nvim: with :14,18fold then
        // :10,24fold, pressing zd on line 10 deletes only the outer
        // fold and foldlevel() then reports 0 on lines 10-13 and
        // 19-24, but 1 on lines 14-18 - i.e. the nested fold is
        // promoted and keeps its absolute position.
        let mut gap = nested_outer_inner();
        delete_fold_entry(&mut gap, 0, false);

        assert_eq!(gap.len(), 1, "the child is promoted, not deleted");
        // fd_top was 4 relative to a parent starting at 10, so the
        // promoted fold must now be at absolute line 14.
        assert_eq!(gap[0].fd_top, 14);
        assert_eq!(gap[0].fd_len, 5);

        let win = WinT {
            w_folds: gap,
            ..Default::default()
        };
        for (lnum, level) in [(10, 0), (12, 0), (14, 1), (16, 1), (18, 1), (19, 0), (24, 0)] {
            assert_eq!(fold_level_win(&win, lnum), level, "line {lnum}");
        }
    }

    #[test]
    fn delete_fold_entry_of_a_childless_fold_just_removes_it() {
        let mut gap = sibling_folds();
        delete_fold_entry(&mut gap, 1, false);
        assert_eq!(gap.len(), 2);
        // The surrounding siblings keep their order and positions.
        assert_eq!(gap[0].fd_top, 10);
        assert_eq!(gap[1].fd_top, 30);
    }

    #[test]
    fn delete_fold_entry_promotes_children_into_the_right_slot() {
        // A fold with two children, sitting between two siblings: the
        // children must land exactly where their parent was.
        let mut gap = vec![
            FoldT {
                fd_top: 1,
                fd_len: 2,
                ..Default::default()
            },
            FoldT {
                fd_top: 10,
                fd_len: 20,
                fd_nested: vec![
                    FoldT {
                        fd_top: 2,
                        fd_len: 3,
                        ..Default::default()
                    },
                    FoldT {
                        fd_top: 8,
                        fd_len: 4,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            FoldT {
                fd_top: 40,
                fd_len: 5,
                ..Default::default()
            },
        ];

        delete_fold_entry(&mut gap, 1, false);

        assert_eq!(gap.len(), 4);
        let tops: Vec<_> = gap.iter().map(|f| f.fd_top).collect();
        // 2 + 10 = 12 and 8 + 10 = 18, still sorted between 1 and 40.
        assert_eq!(tops, vec![1, 12, 18, 40]);
    }

    #[test]
    fn delete_fold_entry_propagates_fd_level_and_unknown_smallness() {
        let mut gap = vec![FoldT {
            fd_top: 10,
            fd_len: 20,
            fd_flags: fd_flags::FD_LEVEL,
            fd_small: crate::types_defs::TriState::None,
            fd_nested: vec![FoldT {
                fd_top: 2,
                fd_len: 3,
                fd_flags: fd_flags::FD_CLOSED,
                fd_small: crate::types_defs::TriState::True,
                ..Default::default()
            }],
        }];

        delete_fold_entry(&mut gap, 0, false);

        // FD_LEVEL means "depends on 'foldlevel'", so it must keep
        // applying to the promoted child; kNone likewise covers
        // nested folds by definition.
        assert_eq!(gap[0].fd_flags, fd_flags::FD_LEVEL);
        assert_eq!(gap[0].fd_small, crate::types_defs::TriState::None);
    }

    #[test]
    fn delete_fold_entry_leaves_child_flags_alone_when_the_parent_is_plain() {
        let mut gap = vec![FoldT {
            fd_top: 10,
            fd_len: 20,
            fd_flags: fd_flags::FD_OPEN,
            fd_small: crate::types_defs::TriState::False,
            fd_nested: vec![FoldT {
                fd_top: 2,
                fd_len: 3,
                fd_flags: fd_flags::FD_CLOSED,
                fd_small: crate::types_defs::TriState::True,
                ..Default::default()
            }],
        }];

        delete_fold_entry(&mut gap, 0, false);

        assert_eq!(gap[0].fd_flags, fd_flags::FD_CLOSED);
        assert_eq!(gap[0].fd_small, crate::types_defs::TriState::True);
    }
}
