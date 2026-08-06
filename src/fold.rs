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

/// Recursive per-fold-array step of [`fold_mark_adjust`]
/// (`foldMarkAdjustRecurse`).
///
/// The real recursive line-number-adjustment-within-nested-folds body
/// is `unimplemented!()` - nothing translated can create a fold yet
/// (matching [`fold_mark_adjust`]'s own doc comment) - but the "no
/// folds at all" fast path (an empty `gap`) is real and exact: the
/// original's own `if (gap->ga_len == 0) return;` is its own very
/// first statement, taken unconditionally today.
fn fold_mark_adjust_recurse(
    gap: &[FoldT],
    _line1: crate::pos_defs::LinenrT,
    _line2: crate::pos_defs::LinenrT,
    _amount: crate::pos_defs::LinenrT,
    _amount_after: crate::pos_defs::LinenrT,
) {
    if gap.is_empty() {
        return;
    }
    unimplemented!(
        "fold::fold_mark_adjust_recurse: recursing into nested folds needs the fold-tree \
         machinery, not yet translated"
    );
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
        let gap: Vec<FoldT> = Vec::new();
        fold_mark_adjust_recurse(&gap, 1, 5, 2, 0); // must not panic
    }

    #[test]
    #[should_panic(expected = "needs the fold-tree machinery")]
    fn fold_mark_adjust_recurse_panics_when_gap_is_non_empty() {
        let gap = vec![crate::fold::FoldT::default()];
        fold_mark_adjust_recurse(&gap, 1, 5, 2, 0);
    }

    #[test]
    fn fold_mark_adjust_is_a_no_op_when_win_has_no_folds() {
        let _lock = crate::globals::global_state_test_lock();
        let win = WinT { w_folds: Vec::new(), ..Default::default() };
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
    #[should_panic(expected = "needs the fold-tree machinery")]
    fn fold_mark_adjust_panics_when_win_has_real_folds() {
        let _lock = crate::globals::global_state_test_lock();
        let win = WinT {
            w_folds: vec![crate::fold::FoldT::default()],
            ..Default::default()
        };
        unsafe { fold_mark_adjust(&win, 1, 5, 2, 0) };
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
}
