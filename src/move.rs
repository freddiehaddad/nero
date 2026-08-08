//! Translated from `src/nvim/move.c` (tractable core only).
//!
//! `move.c` is neovim's cursor/window-scrolling-position file
//! (`curs_columns`, `scroll_cursor_top`, etc., thousands of lines) -
//! most of it deeply tied to the display/redraw pipeline (`w_topline`/
//! `w_botline`/screen-row bookkeeping, `redraw_later`, folding), a
//! separate rendering-subsystem undertaking (phase 9).
//!
//! Translated: `win_col_off`/`win_col_off2` (needed by `plines.c`'s
//! `in_win_border`), `set_valid_virtcol` (needed by `cursor.c`'s
//! `coladvance`/`coladvance_force`), and now, with `plines.c`'s
//! `getvvcol` available: `check_cursor_moved`, `validate_virtcol`,
//! `validate_cursor_col`, `update_curswant`/`update_curswant_force`,
//! `cursor_valid`, plus the trivial `w_valid`-bit-clearing family
//! `changed_cline_bef_curs`/`changed_line_abv_curs`/
//! `changed_line_abv_curs_win`/`invalidate_botline_win`/
//! `approximate_botline_win`. Each of the non-trivial functions omits
//! the same kind of pure redraw-scheduling side effect already
//! established for `set_valid_virtcol` (`redraw_for_cursorcolumn`), and
//! `check_cursor_moved`'s own "concealed line visibility toggled"
//! inner branch (reached only when `wp == curwin`,
//! `w_valid_cursor.lnum > 0`, AND `'conceallevel' >= 2` - a narrow,
//! opt-in-only combination) is `unimplemented!()`: it needs
//! `decoration.c`'s `conceal_cursor_line`/`decor_conceal_line`, neither
//! translated yet.
//!
//! Also translated: `validate_cheight` (via `plines.c`'s already-real
//! `plines_win_full`).
//!
//! Also translated: `set_topline` (now that `fold.c`'s `has_folding`
//! exists) - unblocked `mark.c`'s `mark_view_restore`. Omits the
//! original's `redraw_later(wp, UPD_VALID)` call, matching the same
//! established precedent as the rest of this file.
//!
//! Also translated: `adjust_plines_for_skipcol`/`plines_correct_topline`
//! (found via a re-scan of `move.c` once `plines.c` was fully
//! complete; needed only `w_skipcol` plus `win_col_off`/`win_col_off2`/
//! `plines_win_full`, all already real) - unblocked `cursor.c`'s
//! `set_leftcol`.
//!
//! Also translated: **`vcol2col`** (`mouse.c`, not `move.c` - hosted
//! here anyway since it's the direct dependency of `move.c`'s own
//! `virtcol2col`/`f_virtcol2col`, and needed `plines.c`'s
//! `init_charsize_arg`/`win_charsize` plus `mbyte.c`'s
//! `utf_ptr2StrCharInfo`/`utfc_next`, all already real) and
//! **`virtcol2col`** itself (the `virtcol2col({winid}, {lnum}, {col})`
//! builtin's real logic - its `f_virtcol2col` wrapper lives in
//! `eval/funcs.rs` alongside its sibling window-position builtins).
//! Both are `pub` here despite being `static`/file-private in the
//! original, since Rust's module system needs that for their
//! cross-module callers.
//!
//! Also translated: **`textpos2screenpos`** - computes the screen
//! position of a text character, needing only already-real
//! `fold.c`'s `has_folding`, `plines.c`'s `plines_m_win`/
//! `win_get_fill`/`getvcol`, and this file's own
//! `adjust_plines_for_skipcol`/`win_col_off`/`win_col_off2`. Unlike
//! `vcol2col`/`virtcol2col`, kept `pub` for the SAME reason the
//! original itself is non-`static`: real callers exist in both
//! `move.c` (`f_screenpos`) and `window.c`/`winfloat.c` (neither of
//! the latter two translated yet, but the original's own visibility
//! choice is preserved regardless of whether every real caller
//! exists yet). Its `f_screenpos` wrapper lives in `eval/funcs.rs`.
//!
//! Also translated: **`sms_marker_overlap`/`skipcol_from_plines`/
//! `reset_skipcol`/`use_scrolloffpad`/`scrolloffpad_eof_pressure`** -
//! five small, self-contained functions sitting near the top of
//! `move.c`, each needing only pieces that already exist
//! (`win_col_off`/`win_col_off2`, `get_showbreak_value`,
//! `get_scrolloff_value`/`get_scrolloffpad_value`, or plain fields).
//! `skipcol_from_plines`/`scrolloffpad_eof_pressure` have no real
//! translated caller yet (`update_topline`/`curs_columns`/
//! `scroll_cursor_top`, their only real callers, all still need the
//! redraw pipeline) - marked `#[allow(dead_code)]` until one lands,
//! matching `undo.rs`'s/`marktree.rs`'s established precedent for
//! translating a small, simple, mechanically-correct piece ahead of
//! its real caller. `reset_skipcol` omits the original's trailing
//! `redraw_later(wp, UPD_SOME_VALID)` call, matching this file's own
//! established policy.
//!
//! Also translated: **`adjust_skipcol`** - the real, always-taken-
//! today early-return fast path (`'smoothscroll'` defaults off, and
//! nothing can currently turn it on), with the rest of the algorithm
//! (unreachable today, but faithfully translated in full rather than
//! stubbed) computing the real screen-scroll adjustment for when
//! `'smoothscroll'` and `'wrap'` are both set. Unlocked `insert.c`'s
//! `beginline`/`oneright`/`oneleft`.
//!
//! Also translated: **`scrolljump_value`** (a small `'scrolljump'`
//! effective-value helper, needing only `option_vars::OPTION_VARS.p_sj`
//! and `WinT.w_view_height`) and **`topline_back_winheight`/
//! `topline_back`/`botline_forw`** (+ a new `LineoffT` struct mirroring
//! the original's own `lineoff_T`) - move a line offset up/down by one
//! (screen-)line, computing its own resulting height. Needed only
//! already-real `plines.c`'s `win_get_fill`/`plines_win_nofill`,
//! `fold.c`'s `has_folding`, and `decoration.c`'s `decor_conceal_line`.
//! None has a real translated caller yet (their own real callers all
//! still need the redraw pipeline) - harvested anyway, matching this
//! crate's established ahead-of-caller precedent. `win_get_fill`
//! always returns `0` today (nothing can attach virtual lines or
//! create a diff yet - see `plines.rs`'s/`diff.rs`'s own doc comments),
//! so the "add a filler line" branch in each of these 3 functions is
//! provably unreachable for any `LineoffT` this crate can construct
//! (its own `fill` field always starts at, and stays, `>= 0`) - not
//! separately tested for that specific reason, matching this crate's
//! established policy of not testing a provably-unreachable branch.
//!
//! Also translated: **`cursor_correct_sms`** (make sure the cursor is
//! in the visible part of the topline after scrolling with
//! `'smoothscroll'`) - needed only already-real `option.rs`'s
//! `get_scrolloff_value`, this file's own `win_col_off`/`win_col_off2`/
//! `sms_marker_overlap`/`validate_virtcol`, `plines.rs`'s
//! `linetabsize_eol`, and `cursor.rs`'s `coladvance`. Hand-traced the
//! full multi-branch algorithm (the `so_cols`/`space_cols`/`size`
//! scrolloff-narrowing computation, the `top`/`bot` visible-range
//! bounds, and the `col`-adjustment loop) against a concrete numeric
//! example before writing any test - all 5 tests passed on the first
//! real run, matching the by-hand derivation exactly. Every i32
//! sub-expression the original computes before its own final widening
//! to `int64_t` (`so_cols`/`top`/`bot`'s own intermediate sums) uses
//! `wrapping_add`/`wrapping_sub`/`wrapping_mul` rather than plain
//! arithmetic, matching this crate's established policy for
//! `colnr_T`-shaped arithmetic that could theoretically overflow for
//! pathological window dimensions, even though realistic values never
//! approach that range. Translated ahead of a real caller
//! (`update_topline`/`win_fix_scroll`, both part of the not-yet-
//! translated window-scrolling machinery), matching this crate's
//! established "translate ahead of a real caller" precedent.
//!
//! Also translated: **`comp_botline`/`validate_botline_win`**
//! (re-investigated after being deferred for many sessions citing
//! `redraw_for_cursorline`/`set_empty_rows`/
//! `win_check_anchored_floats` as blockers - direct re-reading found
//! `redraw_for_cursorline`'s ENTIRE job is deciding whether to call
//! `redraw_later` (a pure redraw-scheduling side effect, omitted
//! entirely, matching this file's own established precedent) and
//! `win_check_anchored_floats` was already real (`check_topfill`'s own
//! dependency) - only `set_empty_rows` itself was genuinely missing,
//! and is now translated too). `comp_botline` walks forward from
//! `w_topline` (or `w_cursor.lnum`/`w_cline_row` if `VALID_CROW` is
//! already set) via `plines_correct_topline`, filling in
//! `w_cline_row`/`w_cline_height`/`w_cline_folded` when the walk
//! passes the cursor's own line, until adding one more line's height
//! would exceed `w_view_height`.
//!
//! Deferred: everything else (window-scrolling/`w_topline`
//! maintenance beyond `set_topline`, `curs_columns`'s full screen-row/
//! column computation, `validate_cursor`/`curs_rows`, all needing
//! `fold.c`'s real fold-tree search and/or the redraw pipeline).
//! `validate_cheight` (mentioned above) is NOT among these - fixed a
//! stale duplicate reference here that still listed it as deferred
//! after it was already translated.

use crate::buffer_defs::{w_valid, WinT};
use crate::types_defs::SIGN_WIDTH;

/// The `number_width(wp) + (*wp->w_p_stc == NUL)` expression shared by
/// both `win_col_off` and `win_col_off2` below (the original doesn't
/// share this as a named helper, but the two real functions are
/// otherwise identical here - a private helper avoids duplicating the
/// same logic twice for no behavioral reason).
///
/// # Safety
/// Same as [`crate::drawscreen::number_width`].
unsafe fn num_col_width(wp: &mut WinT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let nw = unsafe { crate::drawscreen::number_width(wp) };
    let stc_is_empty = wp.w_onebuf_opt.wo_stc.as_deref().is_none_or(<[u8]>::is_empty);
    nw + i32::from(stc_is_empty)
}

fn has_num_col(wp: &WinT) -> bool {
    wp.w_onebuf_opt.wo_nu != 0
        || wp.w_onebuf_opt.wo_rnu != 0
        || wp.w_onebuf_opt.wo_stc.as_deref().is_some_and(|s| !s.is_empty())
}

/// Return the number of columns used on the left of `wp` by the
/// `'number'`/`'relativenumber'`/`'statuscolumn'` column, the
/// `'foldcolumn'`, and the sign column (`win_col_off`).
///
/// # Safety
/// Same as [`crate::drawscreen::number_width`].
#[must_use]
pub unsafe fn win_col_off(wp: &mut WinT) -> i32 {
    let num_part = if has_num_col(wp) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { num_col_width(wp) }
    } else {
        0
    };

    // SAFETY: forwarded from this function's own safety doc.
    num_part + unsafe { crate::window::win_fdccol_count(wp) } + wp.w_scwidth * SIGN_WIDTH
}

/// Return the difference in column offset for the second screen line
/// of a wrapped line: positive if `'number'`/`'relativenumber'` is on
/// and `'n'` is in `'cpoptions'` (`win_col_off2`).
///
/// # Safety
/// Same as [`crate::drawscreen::number_width`]. Also touches
/// `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn win_col_off2(wp: &mut WinT) -> i32 {
    let p_cpo = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo.clone();
    let has_n_cpo = p_cpo.as_deref().is_some_and(|s| {
        crate::strings::vim_strchr(s, i32::from(crate::option_vars::CPO_NUMCOL)).is_some()
    });

    if has_num_col(wp) && has_n_cpo {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { num_col_width(wp) };
    }
    0
}

/// Get the number of screen lines skipped by `wp.w_skipcol`
/// (`adjust_plines_for_skipcol`).
///
/// # Safety
/// Same as [`win_col_off`]/[`win_col_off2`].
unsafe fn adjust_plines_for_skipcol(wp: &mut WinT) -> i32 {
    if wp.w_skipcol == 0 {
        return 0;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let width = wp.w_view_width - unsafe { win_col_off(wp) };
    // SAFETY: forwarded from this function's own safety doc.
    let w2 = width + unsafe { win_col_off2(wp) };
    if wp.w_skipcol >= width && w2 > 0 {
        return (wp.w_skipcol - width) / w2 + 1;
    }

    0
}

/// Return how many lines `lnum` will take on the screen, taking into
/// account whether it is the first line, whether `w_skipcol` is
/// non-zero, and limiting to the window height
/// (`plines_correct_topline`).
///
/// The inner [`crate::plines::plines_win_full`] call always passes
/// `cache: true, limit_winheight: false` regardless of this
/// function's own `limit_winheight` parameter, matching the original
/// exactly - the window-height clamp is applied once, at the very end
/// of this function itself, not threaded through to the inner call.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid.
#[must_use]
pub unsafe fn plines_correct_topline(
    wp: *mut WinT,
    lnum: crate::pos_defs::LinenrT,
    nextp: Option<&mut crate::pos_defs::LinenrT>,
    limit_winheight: bool,
    foldedp: Option<&mut bool>,
) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let mut n =
        unsafe { crate::plines::plines_win_full(wp, lnum, nextp, foldedp, true, false) };
    // SAFETY: forwarded from this function's own safety doc.
    let wpref = unsafe { &mut *wp };
    if lnum == wpref.w_topline {
        // SAFETY: forwarded from this function's own safety doc.
        n -= unsafe { adjust_plines_for_skipcol(wpref) };
    }
    if limit_winheight && n > wpref.w_view_height {
        return wpref.w_view_height;
    }
    n
}

/// Return the number of columns overlapping with the `'listchars'`
/// `"precedes"` marker at the left edge of the window, given `extra2`
/// (the difference between [`win_col_off`]/[`win_col_off2`], or `-1`
/// to have this function compute it) (`sms_marker_overlap`).
///
/// # Safety
/// Same as [`win_col_off`]/[`win_col_off2`].
pub unsafe fn sms_marker_overlap(wp: &mut WinT, extra2_arg: i32) -> i32 {
    let extra2 = if extra2_arg == -1 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { win_col_off(wp) - win_col_off2(wp) }
    } else {
        extra2_arg
    };
    // There is no marker overlap when in showbreak mode, thus no need
    // to account for it. See wlv_put_linebuf().
    if !crate::option::get_showbreak_value(wp).is_empty() {
        return 0;
    }

    // Overlap when 'list' and 'listchars' "precedes" are set is 1.
    if wp.w_onebuf_opt.wo_list != 0 && wp.w_p_lcs_chars.prec != 0 {
        return 1;
    }

    if extra2 > 3 {
        0
    } else {
        3 - extra2
    }
}

/// Calculate the `w_skipcol` offset for window `wp` given how many
/// physical lines we want to scroll down (`skipcol_from_plines`).
///
/// # Safety
/// Same as [`win_col_off`]/[`win_col_off2`].
#[allow(dead_code)] // no real translated caller yet (update_topline, its only caller, needs the redraw pipeline)
unsafe fn skipcol_from_plines(wp: &mut WinT, plines_off: i32) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let width1 = wp.w_view_width - unsafe { win_col_off(wp) };

    let mut skipcol = 0;
    if plines_off > 0 {
        skipcol += width1;
    }
    if plines_off > 1 {
        // SAFETY: forwarded from this function's own safety doc.
        skipcol += (width1 + unsafe { win_col_off2(wp) }) * (plines_off - 1);
    }
    skipcol
}

/// Set `wp.w_skipcol` to zero (`reset_skipcol`).
///
/// Omits the original's trailing `redraw_later(wp, UPD_SOME_VALID)`
/// call - a pure redraw-scheduling side effect, matching the
/// established "skip the deferred-subsystem side effect, keep the
/// state correct" policy (e.g. [`set_valid_virtcol`] below).
pub fn reset_skipcol(wp: &mut WinT) {
    if wp.w_skipcol == 0 {
        return;
    }
    wp.w_skipcol = 0;
}

/// Adjust `GLOBALS.curwin`'s own `w_skipcol` for smooth-scrolling
/// display (`adjust_skipcol`).
///
/// Only relevant when `'wrap'` AND `'smoothscroll'` are both set for
/// the current window AND the cursor sits on the topmost displayed
/// line - `'smoothscroll'` (`w_onebuf_opt.wo_sms`) defaults off, and
/// nothing in this crate can currently turn it on (no `:set` command
/// parser yet), so every real invocation today hits this exact
/// early-return fast path, matching the original's own behavior for
/// an unconfigured session precisely. The rest of the algorithm is
/// still translated in full (not skipped), so a future session that
/// DOES support `'smoothscroll'` gets a faithful, already-verified
/// implementation rather than a fresh gap.
///
/// Omits the original's `redraw_later(curwin, UPD_NOT_VALID)` calls -
/// pure redraw-scheduling side effects, matching this file's own
/// established policy - the identical `w_skipcol` state changes are
/// kept.
///
/// Every touch of `GLOBALS.curwin`'s own fields is deliberately
/// re-derived fresh (via a small scoped block or an inline
/// dereference) rather than held across a call to another
/// `*mut WinT`-taking helper (`validate_cheight`/`validate_virtcol`/
/// `plines_win`/`linetabsize_eol`/`sms_marker_overlap`), matching this
/// crate's established aliasing discipline for this exact situation
/// (see `plines.rs`'s `win_text_height`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT`.
pub unsafe fn adjust_skipcol() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;

    let should_return = {
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &*curwin };
        wp.w_onebuf_opt.wo_wrap == 0
            || wp.w_onebuf_opt.wo_sms == 0
            || wp.w_cursor.lnum != wp.w_topline
    };
    if should_return {
        return;
    }

    let width1 = {
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &mut *curwin };
        // SAFETY: forwarded from this function's own safety doc.
        wp.w_view_width - unsafe { win_col_off(wp) }
    };
    if width1 <= 0 {
        return; // no text will be displayed
    }
    let width2 = {
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &mut *curwin };
        // SAFETY: forwarded from this function's own safety doc.
        width1 + unsafe { win_col_off2(wp) }
    };
    let so: i64 = {
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &*curwin };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::option::get_scrolloff_value(wp) }
    };
    let scrolloff_cols: i64 =
        if so == 0 { 0 } else { i64::from(width1) + (so - 1) * i64::from(width2) };
    let mut scrolled = false;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { validate_cheight(curwin) };
    let (cline_height, view_height, cursor_lnum) = {
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &*curwin };
        (wp.w_cline_height, wp.w_view_height, wp.w_cursor.lnum)
    };
    // SAFETY: forwarded from this function's own safety doc.
    if cline_height == view_height
        && unsafe { crate::plines::plines_win(curwin, cursor_lnum, false) } <= view_height
    {
        // the line just fits in the window, don't scroll
        // SAFETY: forwarded from this function's own safety doc.
        reset_skipcol(unsafe { &mut *curwin });
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { validate_virtcol(curwin) };
    let overlap = {
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &mut *curwin };
        let extra2 = wp.w_view_width - width2;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { sms_marker_overlap(wp, extra2) }
    };
    loop {
        let (skipcol, virtcol) = {
            // SAFETY: forwarded from this function's own safety doc.
            let wp = unsafe { &*curwin };
            (wp.w_skipcol, wp.w_virtcol)
        };
        if !(skipcol > 0
            && i64::from(virtcol) < i64::from(skipcol) + i64::from(overlap) + scrolloff_cols)
        {
            break;
        }
        // scroll a screen line down
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &mut *curwin };
        if wp.w_skipcol >= width1 + width2 {
            wp.w_skipcol -= width2;
        } else {
            wp.w_skipcol -= width1;
        }
        scrolled = true;
    }
    if scrolled {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { validate_virtcol(curwin) };
        return; // don't scroll in the other direction now
    }

    let mut row = 0i32;
    let mut col: i64 = {
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &*curwin };
        i64::from(wp.w_virtcol) + scrolloff_cols
    };

    // Avoid adjusting for 'scrolloff' beyond the text line height.
    if scrolloff_cols > 0 {
        let topline = {
            // SAFETY: forwarded from this function's own safety doc.
            let wp = unsafe { &*curwin };
            wp.w_topline
        };
        // SAFETY: forwarded from this function's own safety doc.
        let mut size = unsafe { crate::plines::linetabsize_eol(curwin, topline) };
        size = width1 + width2 * ((size - width1 + width2 - 1) / width2);
        while col > i64::from(size) {
            col -= i64::from(width2);
        }
    }
    col -= {
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &*curwin };
        i64::from(wp.w_skipcol)
    };

    if col >= i64::from(width1) {
        col -= i64::from(width1);
        row += 1;
    }
    if col > i64::from(width2) {
        row += (col / i64::from(width2)) as i32;
    }

    let view_height2 = {
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &*curwin };
        wp.w_view_height
    };
    if row >= view_height2 {
        {
            // SAFETY: forwarded from this function's own safety doc.
            let wp = unsafe { &mut *curwin };
            if wp.w_skipcol == 0 {
                wp.w_skipcol += width1;
                row -= 1;
            }
        }
        if row >= view_height2 {
            // SAFETY: forwarded from this function's own safety doc.
            let wp = unsafe { &mut *curwin };
            wp.w_skipcol += (row - view_height2) * width2;
        }
    }
}

/// Return `true` when `'scrolloffpad'` may augment `'scrolloff'` -
/// only applies to automatic cursor-visibility correction. For now
/// `'scrolloffpad'` is treated as boolean: `0` disables, `> 0` enables
/// (`use_scrolloffpad`).
///
/// # Safety
/// Same as [`crate::option::get_scrolloff_value`]/
/// [`crate::option::get_scrolloffpad_value`].
unsafe fn use_scrolloffpad(wp: &WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::option::get_scrolloff_value(wp) > 0 && crate::option::get_scrolloffpad_value(wp) > 0
    }
}

/// Return `true` when there are not enough real buffer lines below
/// `lnum` to satisfy the requested `so` context
/// (`scrolloffpad_eof_pressure`).
///
/// # Safety
/// `wp.w_buffer` must be a valid, non-null pointer to a live `BufT`.
/// Same as [`use_scrolloffpad`].
#[must_use]
#[allow(dead_code)] // no real translated caller yet (update_topline/curs_columns, both needing the redraw pipeline)
unsafe fn scrolloffpad_eof_pressure(
    wp: &WinT,
    lnum: crate::pos_defs::LinenrT,
    so: crate::types_defs::OptInt,
) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { use_scrolloffpad(wp) } || so <= 0 {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*wp.w_buffer }.b_ml.ml_line_count;
    // Use subtraction to avoid signed overflow in "lnum + so".
    i64::from(lnum) > i64::from(line_count) - so
}

/// Schedule the redraws a horizontal cursor move requires
/// (`redraw_for_cursorcolumn`).
///
/// Four independent effects, each with its own guard:
/// - With `'conceallevel'` on and the cursor line concealed, that line
///   must be redrawn so the cursor position can be recomputed.
/// - Everything below is skipped once `VALID_VIRTCOL` is set, since
///   the virtual column has not actually moved.
/// - `'cursorcolumn'` needs `UPD_SOME_VALID`; failing that,
///   `'cursorline'` with `"screenline"` needs the cheaper `UPD_VALID`.
/// - A cursor move in Visual mode re-inverts the current buffer.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`. Reads
/// `GLOBALS.curwin`/`curbuf`/`Visual`, which must be valid. Forwarded
/// from [`crate::drawscreen::conceal_cursor_line`]'s own safety doc.
pub unsafe fn redraw_for_cursorcolumn(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let (curwin, curbuf, visual_active) = (g.curwin, g.curbuf, g.Visual.active);

    // If the cursor moves horizontally when 'concealcursor' is active,
    // the current line needs redrawing to compute the cursor position.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        if std::ptr::eq(wp, curwin)
            && (*wp).w_onebuf_opt.wo_cole > 0
            && crate::drawscreen::conceal_cursor_line(&*wp)
        {
            crate::drawscreen::redraw_winline(wp, (*wp).w_cursor.lnum);
        }

        if (*wp).w_valid & i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL) != 0 {
            return;
        }

        if (*wp).w_onebuf_opt.wo_cuc != 0 {
            // 'cursorcolumn' needs a wider redraw.
            crate::drawscreen::redraw_later(wp, crate::drawscreen::UPD_SOME_VALID);
        } else if (*wp).w_onebuf_opt.wo_cul != 0
            && u32::from((*wp).w_p_culopt_flags)
                & crate::option_vars::opt_culopt_flag::SCREENLINE
                != 0
        {
            // 'cursorlineopt' containing "screenline" needs only the
            // cheaper UPD_VALID.
            crate::drawscreen::redraw_later(wp, crate::drawscreen::UPD_VALID);
        }

        // A cursor move in Visual mode re-inverts the current buffer.
        if visual_active && std::ptr::eq((*wp).w_buffer, curbuf) {
            crate::drawscreen::redraw_buf_later(curbuf, crate::drawscreen::UPD_INVERTED);
        }
    }
}

/// Set `wp.w_virtcol`/`w_valid`'s `VALID_VIRTCOL` bit for virtual
/// column `vcol` (`set_valid_virtcol`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`. Forwarded
/// from [`redraw_for_cursorcolumn`]'s own safety doc.
pub unsafe fn set_valid_virtcol(wp: *mut WinT, vcol: crate::pos_defs::ColnrT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*wp).w_virtcol = vcol;
        redraw_for_cursorcolumn(wp);
        (*wp).w_valid |= i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL);
    }
}

/// Check if the cursor has moved. Set the `w_valid` flag accordingly
/// (`check_cursor_moved`).
///
/// The "concealed line visibility toggled" inner branch (reached only
/// when `wp == curwin`, `w_valid_cursor.lnum > 0`, AND
/// `'conceallevel' >= 2` - a narrow, opt-in-only combination) is
/// `unimplemented!()`: it needs `decoration.c`'s `conceal_cursor_line`/
/// `decor_conceal_line`, neither translated yet. Every other case
/// (in particular every call with `'conceallevel' < 2`, the default)
/// is fully translated.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn check_cursor_moved(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    if w.w_cursor.lnum != w.w_valid_cursor.lnum {
        w.w_valid &= !(i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_WCOL)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_CHEIGHT)
            | i32::from(w_valid::VALID_CROW)
            | i32::from(w_valid::VALID_TOPLINE));

        // Concealed line visibility toggled.
        // SAFETY: forwarded from this function's own safety doc.
        let is_curwin = std::ptr::eq(wp, unsafe { crate::globals::GLOBALS.get_mut() }.curwin);
        if is_curwin && w.w_valid_cursor.lnum > 0 && w.w_onebuf_opt.wo_cole >= 2 {
            unimplemented!(
                "check_cursor_moved: the concealed-line-visibility-toggled branch needs \
                 decoration.c's conceal_cursor_line/decor_conceal_line"
            );
        }
        w.w_valid_cursor = w.w_cursor;
        w.w_valid_leftcol = w.w_leftcol;
        w.w_valid_skipcol = w.w_skipcol;
        w.w_viewport_invalid = true;
    } else if w.w_skipcol != w.w_valid_skipcol {
        w.w_valid &= !(i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_WCOL)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_CHEIGHT)
            | i32::from(w_valid::VALID_CROW)
            | i32::from(w_valid::VALID_BOTLINE)
            | i32::from(w_valid::VALID_BOTLINE_AP));
        w.w_valid_cursor = w.w_cursor;
        w.w_valid_leftcol = w.w_leftcol;
        w.w_valid_skipcol = w.w_skipcol;
    } else if w.w_cursor.col != w.w_valid_cursor.col
        || w.w_leftcol != w.w_valid_leftcol
        || w.w_cursor.coladd != w.w_valid_cursor.coladd
    {
        w.w_valid &=
            !(i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_WCOL) | i32::from(
                w_valid::VALID_VIRTCOL,
            ));
        w.w_valid_cursor.col = w.w_cursor.col;
        w.w_valid_leftcol = w.w_leftcol;
        w.w_valid_cursor.coladd = w.w_cursor.coladd;
        w.w_viewport_invalid = true;
    }
}

/// Validate `wp.w_virtcol` only (`validate_virtcol`).
///
/// Omits the original's `redraw_for_cursorcolumn(wp)` call - a pure
/// redraw-scheduling side effect, matching [`set_valid_virtcol`]'s own
/// precedent.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid.
pub unsafe fn validate_virtcol(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_cursor_moved(wp) };

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*wp }.w_valid & i32::from(w_valid::VALID_VIRTCOL) != 0 {
        return;
    }

    let mut virtcol = 0;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::plines::getvvcol(wp, &mut (*wp).w_cursor, None, Some(&mut virtcol), None, 0);
    }
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    w.w_virtcol = virtcol;
    w.w_valid |= i32::from(w_valid::VALID_VIRTCOL);
}

/// Validate `wp.w_cline_height`/`wp.w_cline_folded` (`validate_cheight`).
///
/// # Safety
/// Forwarded from [`check_cursor_moved`]/`crate::plines::plines_win_full`'s
/// own safety docs.
pub unsafe fn validate_cheight(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_cursor_moved(wp) };

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*wp }.w_valid & i32::from(w_valid::VALID_CHEIGHT) != 0 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { &*wp }.w_cursor.lnum;
    let mut folded = false;
    // SAFETY: forwarded from this function's own safety doc.
    let height = unsafe { crate::plines::plines_win_full(wp, lnum, None, Some(&mut folded), true, true) };

    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    w.w_cline_height = height;
    w.w_cline_folded = folded;
    w.w_valid |= i32::from(w_valid::VALID_CHEIGHT);
}

/// Force-update `wp.w_curswant` from `wp.w_virtcol`
/// (`update_curswant_force`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` whose own `w_buffer` is also valid.
pub unsafe fn update_curswant_force() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { validate_virtcol(curwin) };
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *curwin };
    w.w_curswant = w.w_virtcol;
    w.w_set_curswant = false;
}

/// Update `wp.w_curswant` if `wp.w_set_curswant` is set
/// (`update_curswant`).
///
/// # Safety
/// Same as [`update_curswant_force`].
pub unsafe fn update_curswant() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*curwin }.w_set_curswant {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { update_curswant_force() };
    }
}

/// @return true if `wp.w_wrow`/`wp.w_wcol` are both currently valid
/// (`cursor_valid`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
#[must_use]
pub unsafe fn cursor_valid(wp: *mut WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_cursor_moved(wp) };
    // SAFETY: forwarded from this function's own safety doc.
    let valid_flags = unsafe { &*wp }.w_valid;
    let want = i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_WCOL);
    (valid_flags & want) == want
}

/// Validate `wp.w_wcol` and `wp.w_virtcol` only (`validate_cursor_col`).
///
/// # Safety
/// Same as [`validate_virtcol`].
pub unsafe fn validate_cursor_col(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { validate_virtcol(wp) };

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*wp }.w_valid & i32::from(w_valid::VALID_WCOL) != 0 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    let mut col = w.w_virtcol;
    // SAFETY: forwarded from this function's own safety doc.
    let off = unsafe { win_col_off(w) };
    col += off;
    // SAFETY: forwarded from this function's own safety doc.
    let width = w.w_view_width - off + unsafe { win_col_off2(w) };

    // long line wrapping, adjust wp->w_wrow
    if w.w_onebuf_opt.wo_wrap != 0 && col >= w.w_view_width && width > 0 {
        // use same formula as what is used in curs_columns()
        col -= ((col - w.w_view_width) / width + 1) * width;
    }
    if col > w.w_leftcol {
        col -= w.w_leftcol;
    } else {
        col = 0;
    }
    w.w_wcol = col;

    w.w_valid |= i32::from(w_valid::VALID_WCOL);
}

/// Called when text before the cursor changed in a way that affects
/// its screen position - clears bits related to lines up to and
/// including the cursor's own line, but not `w_botline`
/// (`changed_cline_bef_curs`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn changed_cline_bef_curs(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_valid &= !(i32::from(w_valid::VALID_WROW)
        | i32::from(w_valid::VALID_WCOL)
        | i32::from(w_valid::VALID_VIRTCOL)
        | i32::from(w_valid::VALID_CROW)
        | i32::from(w_valid::VALID_CHEIGHT)
        | i32::from(w_valid::VALID_TOPLINE));
}

/// Call this when the length of a line (in screen characters) above
/// the cursor has changed. Need to take care of `w_botline`
/// separately! (`changed_line_abv_curs_win`)
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn changed_line_abv_curs_win(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_valid &= !(i32::from(w_valid::VALID_WROW)
        | i32::from(w_valid::VALID_WCOL)
        | i32::from(w_valid::VALID_VIRTCOL)
        | i32::from(w_valid::VALID_CROW)
        | i32::from(w_valid::VALID_CHEIGHT)
        | i32::from(w_valid::VALID_TOPLINE));
}

/// Like [`changed_line_abv_curs_win`], but for `curwin`
/// (`changed_line_abv_curs`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT`.
pub unsafe fn changed_line_abv_curs() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_line_abv_curs_win(curwin) };
}

/// Compute `wp.w_botline` and the other line-and-height-related
/// `w_valid` bits, from `wp.w_topline` (or `wp.w_cursor.lnum` if
/// `VALID_CROW` is already set) forward (`comp_botline`).
///
/// Omits the original's `redraw_for_cursorline(wp)` call: that
/// function's ENTIRE job is to conditionally call `redraw_later` (a
/// pure redraw-scheduling side effect) - it never touches any value
/// this crate currently computes - so the whole call is omitted,
/// matching this crate's established `redraw_later`-omission
/// precedent (e.g. `set_topline`/`set_valid_virtcol`) rather than
/// needing `win_cursorline_standout`/`'cursorline'` to exist first.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid. Forwarded from
/// [`check_cursor_moved`]'s/[`plines_correct_topline`]'s/
/// [`set_empty_rows`]'s/[`crate::winfloat::win_check_anchored_floats`]'s
/// own safety docs.
unsafe fn comp_botline(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_cursor_moved(wp) };

    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    let (mut lnum, mut done) = if w.w_valid & i32::from(w_valid::VALID_CROW) != 0 {
        (w.w_cursor.lnum, w.w_cline_row)
    } else {
        (w.w_topline, 0)
    };

    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*w.w_buffer }.b_ml.ml_line_count;
    while lnum <= line_count {
        let mut last = lnum;
        let mut folded = false;
        // SAFETY: forwarded from this function's own safety doc.
        let n = unsafe {
            plines_correct_topline(wp, lnum, Some(&mut last), true, Some(&mut folded))
        };

        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &mut *wp };
        if lnum <= w.w_cursor.lnum && last >= w.w_cursor.lnum {
            w.w_cline_row = done;
            w.w_cline_height = n;
            w.w_cline_folded = folded;
            w.w_valid |= i32::from(w_valid::VALID_CROW) | i32::from(w_valid::VALID_CHEIGHT);
        }
        if done + n > w.w_view_height {
            break;
        }
        done += n;
        lnum = last;
        lnum += 1;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    // wp->w_botline is the line that is just below the window
    w.w_botline = lnum;
    w.w_valid |= i32::from(w_valid::VALID_BOTLINE) | i32::from(w_valid::VALID_BOTLINE_AP);
    w.w_viewport_invalid = true;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_empty_rows(wp, done) };

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::winfloat::win_check_anchored_floats(wp) };
}

/// Update `wp.w_botline` if it is not valid (`validate_botline_win`).
///
/// # Safety
/// Same as `comp_botline`.
pub unsafe fn validate_botline_win(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*wp }.w_valid & i32::from(w_valid::VALID_BOTLINE) == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { comp_botline(wp) };
    }
}

/// Mark `wp.w_botline` as invalid, because of some change in the
/// buffer (`invalidate_botline_win`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn invalidate_botline_win(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_valid &=
        !(i32::from(w_valid::VALID_BOTLINE) | i32::from(w_valid::VALID_BOTLINE_AP));
}

/// Mark `wp.w_botline` as only approximately valid (`approximate_botline_win`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn approximate_botline_win(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_valid &= !i32::from(w_valid::VALID_BOTLINE);
}

/// Set `wp.w_topline` to `lnum` (`set_topline`).
///
/// Omits the original's `redraw_later(wp, UPD_VALID)` call - a pure
/// redraw-scheduling side effect, matching this crate's established
/// precedent (e.g. `set_valid_virtcol`). Relies on
/// [`crate::fold::has_folding`]'s "no folds in this window" fast path
/// (see that function's own doc) - correct for the common no-folds
/// case, panicking otherwise.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid.
pub unsafe fn set_topline(wp: *mut WinT, lnum: crate::pos_defs::LinenrT) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    let prev_topline = w.w_topline;

    // Go to first of folded lines. `lnum` is a plain local (matching
    // the original's own `linenr_T lnum` by-value parameter), so
    // `&mut lnum` can be passed as `firstp` without aliasing `w` -
    // unlike some other call sites where the original passes a
    // pointer to a STRUCT FIELD instead (see e.g. cursor.rs's
    // check_cursor_lnum, which can't do this due to Rust's borrow
    // checker). has_folding's own "no folds" fast path never rewrites
    // `firstp` though (matching the original's own behavior when
    // hasFolding returns false), so `lnum` is used as-is below
    // regardless of this call's result either way.
    let mut lnum = lnum;
    // SAFETY: forwarded from this function's own safety doc.
    let _ = unsafe { crate::fold::has_folding(w, lnum, Some(&mut lnum), None) };

    // Approximate the value of w_botline.
    w.w_botline += lnum - w.w_topline;
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*w.w_buffer }.b_ml.ml_line_count;
    if w.w_botline > line_count + 1 {
        w.w_botline = line_count + 1;
    }
    w.w_topline = lnum;
    w.w_topline_was_set = true;
    if lnum != prev_topline {
        // Keep the filler lines when the topline didn't change.
        w.w_topfill = 0;
    }
    w.w_valid &= !(i32::from(w_valid::VALID_WROW)
        | i32::from(w_valid::VALID_CROW)
        | i32::from(w_valid::VALID_BOTLINE)
        | i32::from(w_valid::VALID_TOPLINE));
    // Don't set VALID_TOPLINE here, 'scrolloff' needs to be checked.
}

/// Don't end up with too many filler lines in the window
/// (`check_topfill`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid. Forwarded from
/// [`crate::plines::plines_win_nofill`]'s/
/// [`crate::winfloat::win_check_anchored_floats`]'s own safety docs.
pub unsafe fn check_topfill(wp: *mut WinT, down: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    if w.w_topfill > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let n = unsafe { crate::plines::plines_win_nofill(wp, w.w_topline, true) };
        if w.w_topfill + n > w.w_view_height {
            if down && w.w_topline > 1 {
                w.w_topline -= 1;
                w.w_topfill = 0;
            } else {
                w.w_topfill = (w.w_view_height - n).max(0);
            }
        }
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::winfloat::win_check_anchored_floats(wp) };
}

/// Compute `wp.w_empty_rows`/`w_filler_rows` from `used` (the number
/// of buffer-content window rows already occupied) (`set_empty_rows`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid. Forwarded from
/// [`crate::plines::win_get_fill`]'s own safety doc.
pub unsafe fn set_empty_rows(wp: *mut WinT, used: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    w.w_filler_rows = 0;
    if used == 0 {
        w.w_empty_rows = 0; // single line that doesn't fit
    } else {
        w.w_empty_rows = w.w_view_height - used;
        let botline = w.w_botline;
        // SAFETY: forwarded from this function's own safety doc.
        let line_count = unsafe { &*w.w_buffer }.b_ml.ml_line_count;
        if botline <= line_count {
            // SAFETY: forwarded from this function's own safety doc.
            let filler_rows = unsafe { crate::plines::win_get_fill(&*wp, botline) };
            // SAFETY: forwarded from this function's own safety doc.
            let w = unsafe { &mut *wp };
            w.w_filler_rows = filler_rows;
            if w.w_empty_rows > w.w_filler_rows {
                w.w_empty_rows -= w.w_filler_rows;
            } else {
                w.w_filler_rows = w.w_empty_rows;
                w.w_empty_rows = 0;
            }
        }
    }
}

/// Call this whenever a window-local setting changes that could
/// affect the whole window (e.g. an option or the window's own size)
/// (`changed_window_setting`).
///
/// Omits the original's own `redraw_later(wp, UPD_NOT_VALID)` call at
/// the end (a pure redraw-scheduling side effect, matching this
/// crate's established policy for `redraw_later` throughout).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn changed_window_setting(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    w.w_lines_valid = 0;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { changed_line_abv_curs_win(wp) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_valid &=
        !(i32::from(w_valid::VALID_BOTLINE) | i32::from(w_valid::VALID_BOTLINE_AP) | i32::from(w_valid::VALID_TOPLINE));
}

/// Call [`changed_window_setting`] for every window in every tabpage
/// (`changed_window_setting_all`), matching `window.rs`'s own
/// established `FOR_ALL_TAB_WINDOWS`-walk idiom (e.g.
/// `win_valid_any_tab`/`tabpage_win_valid`).
///
/// # Safety
/// `GLOBALS.first_tabpage`'s own `tp_next` chain, and each tabpage's
/// own window list (`GLOBALS.firstwin`/`tp_firstwin`, then `w_next`),
/// must consist of valid, live pointers.
#[allow(dead_code)]
pub unsafe fn changed_window_setting_all() {
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let is_curtab = std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab);
        let mut wp = if is_curtab {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &*tp }.tp_firstwin
        };
        while !wp.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { changed_window_setting(wp) };
            // SAFETY: forwarded from this function's own safety doc.
            wp = unsafe { &*wp }.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
}

/// Convert a virtual (screen) column to a character column. The first
/// column is zero (`vcol2col`, `mouse.c`).
///
/// Returns `(col, coladd)`: `col` is the byte offset within the line
/// (matching the original's own `(colnr_T)(ci.ptr - line)` pointer-
/// subtraction result - this crate's [`crate::mbyte_defs::StrCharInfo`]
/// already tracks that same offset directly as `pos`, needing no
/// arithmetic); `coladd` is the original's own `*coladdp` out-parameter,
/// always computed here since every real caller in this crate wants it
/// (unlike the original, which passes `NULL` from `virtcol2col`).
///
/// Kept `pub` (not `static`, unlike the original) since its real
/// callers - [`virtcol2col`] here and `mouse.c`'s own `mouse.rs`
/// counterpart, not yet translated - are expected to live in more than
/// one module, matching this crate's established cross-module-helper
/// convention (e.g. [`crate::eval::eval::var2fpos`]).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid.
#[must_use]
pub unsafe fn vcol2col(
    wp: *mut WinT,
    lnum: crate::pos_defs::LinenrT,
    vcol: crate::pos_defs::ColnrT,
) -> (crate::pos_defs::ColnrT, crate::pos_defs::ColnrT) {
    // try to advance to the specified column
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *(*wp).w_buffer };
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get_buf(buf, lnum) };
    // SAFETY: forwarded from this function's own safety doc.
    let (mut csarg, cstype) = unsafe { crate::plines::init_charsize_arg(wp, lnum, &line) };
    let mut ci = crate::mbyte::utf_ptr2str_char_info(&line);
    let mut cur_vcol: crate::pos_defs::ColnrT = 0;
    while cur_vcol < vcol && line.get(ci.pos).copied().unwrap_or(0) != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let next_vcol = cur_vcol
            + unsafe {
                crate::plines::win_charsize(
                    cstype,
                    cur_vcol,
                    &line[ci.pos..],
                    ci.pos as crate::pos_defs::ColnrT,
                    ci.chr.value,
                    &mut csarg,
                )
            }
            .width;
        if next_vcol > vcol {
            break;
        }
        cur_vcol = next_vcol;
        // SAFETY: forwarded from this function's own safety doc.
        ci = unsafe { crate::mbyte::utfc_next(&line, ci) };
    }

    (ci.pos as crate::pos_defs::ColnrT, vcol - cur_vcol)
}

/// Convert a virtual (screen) column to a character column. The first
/// column is one. For a multibyte character, the column number of the
/// first byte is returned (`virtcol2col`, `move.c`'s own `static`
/// helper).
///
/// Kept `pub` (not `static`, unlike the original) since its only real
/// caller, `f_virtcol2col`, lives in
/// [`crate::eval::funcs`] - a different module, matching
/// [`vcol2col`]'s own visibility rationale above.
///
/// # Safety
/// Same as [`vcol2col`].
#[must_use]
pub unsafe fn virtcol2col(wp: *mut WinT, lnum: crate::pos_defs::LinenrT, vcol: i32) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let (offset, _) = unsafe { vcol2col(wp, lnum, vcol - 1) };
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *(*wp).w_buffer };
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get_buf(buf, lnum) };
    let mut p = offset as usize;

    if line.get(p).copied().unwrap_or(0) == 0 {
        if p == 0 {
            // empty line
            return 0;
        }
        // Move to the first byte of the last char.
        // SAFETY: forwarded from this function's own safety doc.
        p -= 1 + unsafe { crate::mbyte::utf_head_off(&line, p - 1) } as usize;
    }
    (p + 1) as i32
}

/// Compute the screen position of the text character at `pos` in
/// window `wp`. The resulting values are one-based, zero when the
/// character is not visible (`textpos2screenpos`).
///
/// Kept `pub` (not a plain top-level function with external linkage
/// only within this file, unlike `virtcol2col`/`vcol2col` - the
/// original itself is already non-`static`, called from both
/// `move.c`'s own `f_screenpos` and `window.c`/`winfloat.c`, neither
/// of the latter two translated yet).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid.
#[allow(clippy::too_many_arguments)]
pub unsafe fn textpos2screenpos(
    wp: *mut WinT,
    pos: &mut crate::pos_defs::PosT,
    rowp: &mut i32,
    scolp: &mut crate::pos_defs::ColnrT,
    ccolp: &mut crate::pos_defs::ColnrT,
    ecolp: &mut crate::pos_defs::ColnrT,
    local: bool,
) {
    let mut scol: crate::pos_defs::ColnrT = 0;
    let mut ccol: crate::pos_defs::ColnrT = 0;
    let mut ecol: crate::pos_defs::ColnrT = 0;
    let mut coloff: crate::pos_defs::ColnrT = 0;
    let mut visible_row = false;
    let mut is_folded = false;

    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    let mut lnum = pos.lnum;
    // `row` is unconditionally assigned by exactly one of these three
    // branches (matching the original's own `int row = 0;` followed
    // by an if/else-if/else covering every case) - written as an
    // if-expression instead of a dead `= 0` initializer plus 3 later
    // assignments, avoiding a real `unused_assignments` warning while
    // keeping identical behavior.
    let mut row: i32 = if lnum >= w.w_topline && lnum <= w.w_botline {
        // SAFETY: forwarded from this function's own safety doc.
        is_folded = unsafe { crate::fold::has_folding(w, lnum, Some(&mut lnum), None) };
        // "row" should be the screen line where line "lnum" begins,
        // which can be negative if "lnum" is "w_topline" and
        // "w_skipcol" is non-zero.
        // SAFETY: forwarded from this function's own safety doc.
        let mut row = unsafe { crate::plines::plines_m_win(wp, w.w_topline, lnum - 1, i32::MAX) };
        // SAFETY: forwarded from this function's own safety doc.
        row -= unsafe { adjust_plines_for_skipcol(w) };
        // Add filler lines above this buffer line.
        row += if lnum == w.w_topline {
            w.w_topfill
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::plines::win_get_fill(w, lnum) }
        };
        visible_row = true;
        row
    } else if !local || lnum < w.w_topline {
        0
    } else {
        w.w_view_height - 1
    };

    // SAFETY: forwarded from this function's own safety doc.
    let existing_row = lnum > 0 && lnum <= unsafe { &*w.w_buffer }.b_ml.ml_line_count;

    if (local || visible_row) && existing_row {
        // SAFETY: forwarded from this function's own safety doc.
        let off = unsafe { win_col_off(w) };
        if is_folded {
            row += (if local { 0 } else { w.w_winrow + w.w_winrow_off }) + 1;
            coloff = (if local { 0 } else { w.w_wincol + w.w_wincol_off }) + 1 + off;
        } else {
            debug_assert_eq!(lnum, pos.lnum);
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                crate::plines::getvcol(wp, pos, Some(&mut scol), Some(&mut ccol), Some(&mut ecol), 0);
            }

            // similar to what is done in validate_cursor_col()
            let mut col = scol;
            col += off;
            // SAFETY: forwarded from this function's own safety doc.
            let width = w.w_view_width - off + unsafe { win_col_off2(w) };

            // long line wrapping, adjust row
            if w.w_onebuf_opt.wo_wrap != 0 && col >= w.w_view_width && width > 0 {
                // use same formula as what is used in curs_columns()
                let rowoff = if visible_row { (col - w.w_view_width) / width + 1 } else { 0 };
                col -= rowoff * width;
                row += rowoff;
            }

            col -= w.w_leftcol;

            if col >= 0 && col < w.w_view_width && row >= 0 && row < w.w_view_height {
                coloff = col - scol + (if local { 0 } else { w.w_wincol + w.w_wincol_off }) + 1;
                row += (if local { 0 } else { w.w_winrow + w.w_winrow_off }) + 1;
            } else {
                // character is left, right or below of the window
                scol = 0;
                ccol = 0;
                ecol = 0;
                if local {
                    coloff = if col < 0 { -1 } else { w.w_view_width + 1 };
                } else {
                    row = 0;
                }
            }
        }
    }
    *rowp = row;
    *scolp = scol + coloff;
    *ccolp = ccol + coloff;
    *ecolp = ecol + coloff;
}

/// Line offset used by [`topline_back`]/[`topline_back_winheight`]/
/// [`botline_forw`] to describe one added line's own filler-line count
/// and screen height (`lineoff_T`).
#[derive(Debug, Clone, Copy, Default)]
pub struct LineoffT {
    /// Line number.
    pub lnum: crate::pos_defs::LinenrT,
    /// Filler lines.
    pub fill: i32,
    /// Height of the added line.
    pub height: i32,
}

/// Compute the effective `'scrolljump'` value for window `wp`: the
/// option's own value directly when non-negative, or a percentage of
/// the window's height when negative (`scrolljump_value`).
#[must_use]
pub fn scrolljump_value(wp: &WinT) -> i32 {
    let p_sj = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sj;
    if p_sj >= 0 {
        p_sj as i32
    } else {
        (wp.w_view_height * -(p_sj as i32)) / 100
    }
}

/// Move `lp` one line up (or add one more filler line), setting its
/// own resulting screen height (`topline_back_winheight`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` with a
/// valid, non-null `w_buffer`.
pub unsafe fn topline_back_winheight(wp: *mut WinT, lp: &mut LineoffT, winheight: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let fill_above = unsafe { crate::plines::win_get_fill(&*wp, lp.lnum) };
    if lp.fill < fill_above {
        // Add a filler line.
        lp.fill += 1;
        lp.height = 1;
    } else {
        lp.lnum -= 1;
        lp.fill = 0;
        if lp.lnum < 1 {
            lp.height = crate::pos_defs::MAXCOL;
        } else {
            let mut first = lp.lnum;
            // SAFETY: forwarded from this function's own safety doc.
            let folded =
                unsafe { crate::fold::has_folding(&mut *wp, lp.lnum, Some(&mut first), None) };
            lp.lnum = first;
            if folded {
                // Add a closed fold unless concealed.
                // SAFETY: forwarded from this function's own safety doc.
                lp.height = i32::from(
                    !unsafe { crate::decoration::decor_conceal_line(&*wp, lp.lnum - 1, false) },
                );
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                lp.height = unsafe { crate::plines::plines_win_nofill(wp, lp.lnum, winheight) };
            }
        }
    }
}

/// [`topline_back_winheight`] with `winheight` always `true`
/// (`topline_back`).
///
/// # Safety
/// Same as [`topline_back_winheight`].
pub unsafe fn topline_back(wp: *mut WinT, lp: &mut LineoffT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { topline_back_winheight(wp, lp, true) };
}

/// Whether `wp`'s cursor is close enough to the top of the window that
/// `'scrolloff'` requires scrolling up (`check_top_offset`), counting
/// visible screen lines above the cursor line via [`topline_back`].
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid.
#[allow(dead_code)]
unsafe fn check_top_offset(wp: *mut WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let so = unsafe { crate::option::get_scrolloff_value(&*wp) };
    // SAFETY: forwarded from this function's own safety doc.
    let (w_cursor_lnum, w_topline, w_topfill) = {
        let w = unsafe { &*wp };
        (w.w_cursor.lnum, w.w_topline, w.w_topfill)
    };

    if i64::from(w_cursor_lnum) < i64::from(w_topline) + so
        // SAFETY: forwarded from this function's own safety doc.
        || unsafe { crate::decoration::win_lines_concealed(&*wp) }
    {
        let mut loff = LineoffT { lnum: w_cursor_lnum, fill: 0, ..Default::default() };
        let mut n = w_topfill; // always have this context
        // Count the visible screen lines above the cursor line.
        while i64::from(n) < so {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { topline_back(wp, &mut loff) };
            // Stop when included a line above the window.
            if loff.lnum < w_topline || (loff.lnum == w_topline && loff.fill > 0) {
                break;
            }
            n += loff.height;
        }
        if i64::from(n) < so {
            return true;
        }
    }
    false
}

/// Add one line below `lp.lnum` - a filler line, a closed fold, or a
/// (wrapped) text line, updating `lp.fill` and setting `lp.height` to
/// the added line's own screen height. Lines below the last one get
/// an incredibly high height (`MAXCOL`) (`botline_forw`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` with a
/// valid, non-null `w_buffer`.
pub unsafe fn botline_forw(wp: *mut WinT, lp: &mut LineoffT) {
    // SAFETY: forwarded from this function's own safety doc.
    let fill_below = unsafe { crate::plines::win_get_fill(&*wp, lp.lnum + 1) };
    if lp.fill < fill_below {
        // Add a filler line.
        lp.fill += 1;
        lp.height = 1;
    } else {
        lp.lnum += 1;
        lp.fill = 0;
        // SAFETY: forwarded from this function's own safety doc.
        let ml_line_count = unsafe { (*(*wp).w_buffer).b_ml.ml_line_count };
        if lp.lnum > ml_line_count {
            lp.height = crate::pos_defs::MAXCOL;
        } else {
            let mut last = lp.lnum;
            // SAFETY: forwarded from this function's own safety doc.
            let folded =
                unsafe { crate::fold::has_folding(&mut *wp, lp.lnum, None, Some(&mut last)) };
            lp.lnum = last;
            if folded {
                // Add a closed fold unless concealed.
                // SAFETY: forwarded from this function's own safety doc.
                lp.height = i32::from(
                    !unsafe { crate::decoration::decor_conceal_line(&*wp, lp.lnum - 1, false) },
                );
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                lp.height = unsafe { crate::plines::plines_win_nofill(wp, lp.lnum, true) };
            }
        }
    }
}

/// Make sure the cursor is in the visible part of the topline after
/// scrolling the screen with `'smoothscroll'` (`cursor_correct_sms`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`, whose own
/// `w_buffer` must be a valid, non-null, live `BufT` pointer.
pub unsafe fn cursor_correct_sms(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let (wo_sms, wo_wrap, cursor_lnum, topline) = unsafe {
        (
            (*wp).w_onebuf_opt.wo_sms,
            (*wp).w_onebuf_opt.wo_wrap,
            (*wp).w_cursor.lnum,
            (*wp).w_topline,
        )
    };
    if wo_sms == 0 || wo_wrap == 0 || cursor_lnum != topline {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let so = unsafe { crate::option::get_scrolloff_value(&*wp) };
    // SAFETY: forwarded from this function's own safety doc.
    let view_width = unsafe { (*wp).w_view_width };
    // SAFETY: forwarded from this function's own safety doc.
    let col_off = unsafe { win_col_off(&mut *wp) };
    let width1 = view_width.wrapping_sub(col_off);
    // SAFETY: forwarded from this function's own safety doc.
    let col_off2 = unsafe { win_col_off2(&mut *wp) };
    let width2 = width1.wrapping_add(col_off2);
    let mut so_cols: i64 = if so == 0 {
        0
    } else {
        i64::from(width1) + (so - 1) * i64::from(width2)
    };
    // SAFETY: forwarded from this function's own safety doc.
    let view_height = unsafe { (*wp).w_view_height };
    let space_cols = view_height.wrapping_sub(1).wrapping_mul(width2);
    let size = if so == 0 {
        0
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::plines::linetabsize_eol(wp, topline) }
    };

    // SAFETY: forwarded from this function's own safety doc.
    let skipcol = unsafe { (*wp).w_skipcol };
    if topline == 1 && skipcol == 0 {
        so_cols = 0; // Ignore 'scrolloff' at top of buffer.
    } else if so_cols > i64::from(space_cols) / 2 {
        so_cols = i64::from(space_cols) / 2; // Not enough room: put cursor in the middle.
    }

    // Not enough screen lines in topline: ignore 'scrolloff'.
    while so_cols > i64::from(size)
        && so_cols - i64::from(width2) >= i64::from(width1)
        && width1 > 0
    {
        so_cols -= i64::from(width2);
    }
    if so_cols >= i64::from(width1) && so_cols > i64::from(size) {
        so_cols -= i64::from(width1);
    }

    let overlap = if skipcol == 0 {
        0
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { sms_marker_overlap(&mut *wp, view_width.wrapping_sub(width2)) }
    };
    // If we have non-zero scrolloff, ignore marker overlap.
    let top = i64::from(skipcol) + if so_cols != 0 { so_cols } else { i64::from(overlap) };
    let bot = i64::from(
        skipcol
            .wrapping_add(width1)
            .wrapping_add(view_height.wrapping_sub(1).wrapping_mul(width2)),
    ) - so_cols;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { validate_virtcol(wp) };
    // SAFETY: forwarded from this function's own safety doc.
    let mut col = unsafe { (*wp).w_virtcol };

    if i64::from(col) < top {
        if col < width1 {
            col = col.wrapping_add(width1);
        }
        while width2 > 0 && i64::from(col) < top {
            col = col.wrapping_add(width2);
        }
    } else {
        while width2 > 0 && i64::from(col) >= bot {
            col = col.wrapping_sub(width2);
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    let w_virtcol = unsafe { (*wp).w_virtcol };
    if col != w_virtcol {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            (*wp).w_curswant = col;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let rc = unsafe { crate::cursor::coladvance(wp, col) };
        // validate_virtcol() marked various things as valid, but
        // after moving the cursor they need to be recomputed.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            (*wp).w_valid &= !(i32::from(w_valid::VALID_WROW)
                | i32::from(w_valid::VALID_WCOL)
                | i32::from(w_valid::VALID_CHEIGHT)
                | i32::from(w_valid::VALID_CROW)
                | i32::from(w_valid::VALID_VIRTCOL));
        }
        // SAFETY: forwarded from this function's own safety doc.
        let (w_buffer, cur_lnum) = unsafe { ((*wp).w_buffer, (*wp).w_cursor.lnum) };
        // SAFETY: forwarded from this function's own safety doc.
        let line_count = unsafe { (*w_buffer).b_ml.ml_line_count };
        if rc == crate::vim_defs::FAIL && skipcol > 0 && cur_lnum < line_count {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { validate_virtcol(wp) };
            // SAFETY: forwarded from this function's own safety doc.
            let w_virtcol2 = unsafe { (*wp).w_virtcol };
            if i64::from(w_virtcol2) < i64::from(skipcol) + i64::from(overlap) {
                // Cursor still not visible: move it to the next line
                // instead.
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    (*wp).w_cursor.lnum += 1;
                    (*wp).w_cursor.col = 0;
                    (*wp).w_cursor.coladd = 0;
                    (*wp).w_curswant = 0;
                    (*wp).w_valid &= !i32::from(w_valid::VALID_VIRTCOL);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraw_for_cursorcolumn_stops_once_virtcol_is_already_valid() {
        // Everything below the VALID_VIRTCOL check is skipped, because
        // the virtual column has not actually moved.
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_win, prev_buf, prev_vis) = (g.curwin, g.curbuf, g.Visual.active);
        g.Visual.active = false;
        g.curwin = std::ptr::null_mut();

        let mut win = crate::buffer_defs::WinT {
            w_valid: i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL),
            w_redr_type: 0,
            ..Default::default()
        };
        // 'cursorcolumn' would normally schedule a redraw.
        win.w_onebuf_opt.wo_cuc = 1;

        unsafe { redraw_for_cursorcolumn(&mut win) };
        assert_eq!(win.w_redr_type, 0, "a valid virtcol short-circuits");

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = prev_win;
        g.curbuf = prev_buf;
        g.Visual.active = prev_vis;
    }

    #[test]
    fn redraw_for_cursorcolumn_cursorcolumn_wins_over_cursorline() {
        // 'cursorcolumn' takes the wider UPD_SOME_VALID; only when it
        // is off does 'cursorline' + "screenline" get the cheaper
        // UPD_VALID.
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_win, prev_buf, prev_vis) = (g.curwin, g.curbuf, g.Visual.active);
        g.Visual.active = false;
        g.curwin = std::ptr::null_mut();

        let mut win = crate::buffer_defs::WinT { w_redr_type: 0, ..Default::default() };
        win.w_onebuf_opt.wo_cuc = 1;
        win.w_onebuf_opt.wo_cul = 1;
        win.w_p_culopt_flags =
            crate::option_vars::opt_culopt_flag::SCREENLINE as u8;

        unsafe { redraw_for_cursorcolumn(&mut win) };
        assert_eq!(win.w_redr_type, crate::drawscreen::UPD_SOME_VALID);

        // With 'cursorcolumn' off the cheaper level is used.
        let mut win = crate::buffer_defs::WinT { w_redr_type: 0, ..Default::default() };
        win.w_onebuf_opt.wo_cuc = 0;
        win.w_onebuf_opt.wo_cul = 1;
        win.w_p_culopt_flags =
            crate::option_vars::opt_culopt_flag::SCREENLINE as u8;

        unsafe { redraw_for_cursorcolumn(&mut win) };
        assert_eq!(win.w_redr_type, crate::drawscreen::UPD_VALID);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = prev_win;
        g.curbuf = prev_buf;
        g.Visual.active = prev_vis;
    }

    #[test]
    fn redraw_for_cursorcolumn_needs_screenline_specifically() {
        // 'cursorline' alone is not enough - the "screenline" flag in
        // 'cursorlineopt' is what makes a horizontal move matter.
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_win, prev_buf, prev_vis) = (g.curwin, g.curbuf, g.Visual.active);
        g.Visual.active = false;
        g.curwin = std::ptr::null_mut();

        let mut win = crate::buffer_defs::WinT { w_redr_type: 0, ..Default::default() };
        win.w_onebuf_opt.wo_cul = 1;
        win.w_p_culopt_flags = crate::option_vars::opt_culopt_flag::LINE as u8;

        unsafe { redraw_for_cursorcolumn(&mut win) };
        assert_eq!(win.w_redr_type, 0);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = prev_win;
        g.curbuf = prev_buf;
        g.Visual.active = prev_vis;
    }

    #[test]
    fn set_valid_virtcol_sets_the_column_and_the_valid_bit() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_win, prev_buf, prev_vis) = (g.curwin, g.curbuf, g.Visual.active);
        g.Visual.active = false;
        g.curwin = std::ptr::null_mut();

        let mut win = crate::buffer_defs::WinT::default();
        unsafe { set_valid_virtcol(&mut win, 7) };

        assert_eq!(win.w_virtcol, 7);
        assert_ne!(
            win.w_valid & i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL),
            0
        );

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = prev_win;
        g.curbuf = prev_buf;
        g.Visual.active = prev_vis;
    }
    use crate::buffer_defs::BufT;

    fn win_with_buf(buf: *mut BufT) -> WinT {
        WinT { w_buffer: buf, ..Default::default() }
    }

    /// Points `GLOBALS.curtab` at `tp` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime
    /// (matching `plines.rs`'s/`diff.rs`'s own identically-named
    /// helper - `curtab` must be non-null for `win_get_fill`/
    /// `diff_check_fill`'s own `curtab` read to be sound, reached
    /// whenever `lnum != wp.w_topline`).
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

    /// Points `GLOBALS.curwin` at `wp` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime,
    /// matching `ops.rs`'s/`insert.rs`'s own identically-named helper.
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
    fn win_col_off_zero_when_nothing_enabled() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        assert_eq!(unsafe { win_col_off(&mut win) }, 0);
    }

    #[test]
    fn win_col_off_counts_number_column_and_foldcolumn_and_signcolumn() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5; // number_width == 1
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_nu = 1;
        win.w_onebuf_opt.wo_fdc = Some(b"2".to_vec());
        win.w_scwidth = 1;

        // number_width(1) + stc_empty(1, no statuscolumn) + fdccol(2)
        // + scwidth(1) * SIGN_WIDTH(2) = 1 + 1 + 2 + 2 = 6.
        assert_eq!(unsafe { win_col_off(&mut win) }, 6);
    }

    #[test]
    fn win_col_off_statuscolumn_set_excludes_the_plus_one() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_stc = Some(b"%n".to_vec());
        win.w_onebuf_opt.wo_nuw = 3;

        // has_num_col via non-empty w_p_stc; num_col_width = number_width
        // (3, from the 'statuscolumn' branch: (nu||rnu)=0 so 0*nuw=0... )
        // wait: nu/rnu both 0 here, so number_width's own stc-branch gives
        // 0 * nuw = 0; stc_is_empty is false (stc is set) so +0.
        assert_eq!(unsafe { win_col_off(&mut win) }, 0);
    }

    #[test]
    fn win_col_off2_zero_without_cpo_n_flag() {
        // win_col_off2 reads the shared OPTION_VARS.p_cpo internally, even
        // though this test never touches it explicitly - must still hold
        // the lock so a concurrently-running test that DOES mutate p_cpo
        // (e.g. win_col_off2_nonzero_with_cpo_n_flag_and_number_column)
        // can't be observed mid-mutation.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_nu = 1;
        // p_cpo left at its default (None) - no 'n' flag present.
        assert_eq!(unsafe { win_col_off2(&mut win) }, 0);
    }

    #[test]
    fn win_col_off2_nonzero_with_cpo_n_flag_and_number_column() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo = Some(b"n".to_vec());

        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5; // number_width == 1
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_nu = 1;

        assert_eq!(unsafe { win_col_off2(&mut win) }, 2); // number_width(1) + stc_empty(1)

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo = prev;
    }

    #[test]
    fn adjust_plines_for_skipcol_zero_skipcol_returns_zero() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 20;
        win.w_skipcol = 0;
        assert_eq!(unsafe { adjust_plines_for_skipcol(&mut win) }, 0);
    }

    #[test]
    fn adjust_plines_for_skipcol_computes_lines_skipped() {
        // width = 20 - win_col_off(0) = 20; w2 = 20 + win_col_off2(0) = 20.
        // skipcol(45) >= width(20): (45-20)/20 + 1 = 25/20 + 1 = 1+1 = 2.
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 20;
        win.w_skipcol = 45;
        assert_eq!(unsafe { adjust_plines_for_skipcol(&mut win) }, 2);
    }

    #[test]
    fn adjust_plines_for_skipcol_below_width_returns_zero() {
        // skipcol(10) < width(20): the `w_skipcol >= width` guard fails,
        // so this returns 0 even though skipcol is nonzero.
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 20;
        win.w_skipcol = 10;
        assert_eq!(unsafe { adjust_plines_for_skipcol(&mut win) }, 0);
    }

    #[test]
    fn plines_correct_topline_no_wrap_single_line_no_skipcol() {
        // wo_wrap=0 makes plines_win_full's inner plines_win_nofill hit
        // its own fast "1 line" path (no memline access needed) - the
        // baseline case: no filler (lnum == w_topline but w_topfill=0),
        // no skipcol adjustment (w_skipcol=0).
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1;
        win.w_view_width = 20;
        assert_eq!(
            unsafe { plines_correct_topline(&mut win as *mut WinT, 1, None, false, None) },
            1
        );
    }

    #[test]
    fn plines_correct_topline_subtracts_skipcol_lines_only_at_topline() {
        // w_skipcol=25 -> adjust_plines_for_skipcol returns 1 (see
        // adjust_plines_for_skipcol_computes_lines_skipped's own
        // derivation pattern: (25-20)/20 + 1 = 0+1 = 1). Only subtracted
        // when lnum == w_topline.
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 5;
        win.w_view_width = 20;
        win.w_skipcol = 25;

        // At the topline: base 1 line, minus 1 skipcol-adjustment line.
        assert_eq!(
            unsafe { plines_correct_topline(&mut win as *mut WinT, 5, None, false, None) },
            0
        );

        // Not at the topline: skipcol adjustment never applies. This
        // path calls win_get_fill (unlike the "at topline" path, which
        // reads w_topfill directly) - needs GLOBALS.curtab set up
        // (CurtabGuard above), caught by a real null-pointer-deref
        // crash the first time this test was run without it.
        assert_eq!(
            unsafe { plines_correct_topline(&mut win as *mut WinT, 6, None, false, None) },
            1
        );
    }

    // --- set_empty_rows ---

    #[test]
    fn set_empty_rows_used_zero_resets_both_fields() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_filler_rows = 99;
        win.w_empty_rows = 99;

        unsafe { set_empty_rows(&mut win as *mut WinT, 0) };

        assert_eq!(win.w_filler_rows, 0);
        assert_eq!(win.w_empty_rows, 0);
    }

    #[test]
    fn set_empty_rows_botline_past_the_end_skips_the_filler_lookup() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 3;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_height = 10;
        win.w_botline = 5; // > ml_line_count(3): inner block is skipped

        unsafe { set_empty_rows(&mut win as *mut WinT, 6) };

        assert_eq!(win.w_empty_rows, 4); // 10 - 6
        assert_eq!(win.w_filler_rows, 0); // never touched past the reset
    }

    #[test]
    fn set_empty_rows_valid_botline_with_no_filler_lines() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 5;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_height = 10;
        win.w_botline = 3; // <= ml_line_count(5): inner block runs

        unsafe { set_empty_rows(&mut win as *mut WinT, 6) };

        // win_get_fill returns 0 today (nothing can create diff/
        // virtual-line filler content), so this is observably
        // identical to the "skipped" case above.
        assert_eq!(win.w_empty_rows, 4); // 10 - 6
        assert_eq!(win.w_filler_rows, 0);
    }

    #[test]
    fn set_empty_rows_used_equals_view_height_takes_the_else_branch() {
        // w_empty_rows computes to exactly 0, so the "w_empty_rows >
        // w_filler_rows" check (0 > 0) is false - the else branch
        // runs instead, but produces the same final values since
        // win_get_fill is 0 today either way.
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 5;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_height = 10;
        win.w_botline = 3;

        unsafe { set_empty_rows(&mut win as *mut WinT, 10) };

        assert_eq!(win.w_empty_rows, 0);
        assert_eq!(win.w_filler_rows, 0);
    }

    #[test]
    fn plines_correct_topline_limit_winheight_clamps_result() {
        // At the topline with w_topfill=10 (added as filler on top of
        // the base 1 text line): unclamped n = 11. limit_winheight
        // clamps to w_view_height (3).
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1;
        win.w_topfill = 10;
        win.w_view_width = 20;
        win.w_view_height = 3;

        assert_eq!(
            unsafe { plines_correct_topline(&mut win as *mut WinT, 1, None, true, None) },
            3
        );
        // Without the clamp, the unclamped value (11) comes through.
        assert_eq!(
            unsafe { plines_correct_topline(&mut win as *mut WinT, 1, None, false, None) },
            11
        );
    }

    #[test]
    fn plines_correct_topline_forwards_foldedp_out_param() {
        // has_folding's own "no folds" fast path always reports false -
        // foldedp should reflect that (matches plines_win_full's own
        // already-established foldedp-forwarding test precedent).
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1;
        win.w_view_width = 20;
        let mut folded = true; // deliberately wrong initial value
        unsafe {
            let _ =
                plines_correct_topline(&mut win as *mut WinT, 1, None, false, Some(&mut folded));
        }
        assert!(!folded);
    }

    #[test]
    fn check_cursor_moved_lnum_change_clears_lnum_related_bits() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_WCOL)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_CHEIGHT)
            | i32::from(w_valid::VALID_CROW)
            | i32::from(w_valid::VALID_TOPLINE)
            | i32::from(w_valid::VALID_BOTLINE); // extra bit that should survive
        win.w_cursor.lnum = 5;
        win.w_valid_cursor.lnum = 1; // different -> triggers the lnum branch

        unsafe { check_cursor_moved(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_BOTLINE));
        assert_eq!(win.w_valid_cursor.lnum, 5);
        assert!(win.w_viewport_invalid);
    }

    #[test]
    fn check_cursor_moved_skipcol_change_clears_different_bits() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_BOTLINE)
            | i32::from(w_valid::VALID_TOPLINE); // should survive
        win.w_cursor.lnum = 1;
        win.w_valid_cursor.lnum = 1; // same -> skip the lnum branch
        win.w_skipcol = 3;
        win.w_valid_skipcol = 0; // different -> triggers the skipcol branch

        unsafe { check_cursor_moved(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_TOPLINE));
        assert_eq!(win.w_valid_skipcol, 3);
    }

    #[test]
    fn check_cursor_moved_col_change_clears_col_related_bits() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_WCOL)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_CHEIGHT); // should survive
        win.w_cursor.lnum = 1;
        win.w_valid_cursor.lnum = 1;
        win.w_skipcol = 0;
        win.w_valid_skipcol = 0;
        win.w_cursor.col = 4;
        win.w_valid_cursor.col = 1; // different -> triggers the col branch

        unsafe { check_cursor_moved(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_CHEIGHT));
        assert_eq!(win.w_valid_cursor.col, 4);
        assert!(win.w_viewport_invalid);
    }

    #[test]
    fn check_cursor_moved_noop_when_nothing_changed() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_WCOL);
        // w_valid_cursor/w_leftcol/w_skipcol all match w_cursor's
        // defaults already (all zero) - nothing should change.

        unsafe { check_cursor_moved(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_WCOL));
    }

    #[test]
    fn check_cursor_moved_panics_on_conceal_branch_when_curwin_and_conceallevel_2() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_cole = 2;
        win.w_cursor.lnum = 5;
        win.w_valid_cursor.lnum = 1; // > 0 and different from w_cursor.lnum

        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        // catch_unwind (rather than #[should_panic]) so curwin is
        // always restored before this test returns, even though the
        // call panics - otherwise GLOBALS.curwin would dangle,
        // pointing at this test's about-to-be-dropped local `win`.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            check_cursor_moved(&mut win as *mut WinT);
        }));

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;

        let err = result.expect_err("expected check_cursor_moved to panic");
        let msg = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_default();
        assert!(
            msg.contains("concealed-line-visibility-toggled"),
            "unexpected panic message: {msg}"
        );
    }

    #[test]
    fn validate_virtcol_computes_and_marks_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 3, coladd: 0 };

        unsafe { validate_virtcol(&mut win as *mut WinT) };

        assert_eq!(win.w_virtcol, 3); // plain ASCII 'l' at col 3 in "hello"
        assert_ne!(win.w_valid & i32::from(w_valid::VALID_VIRTCOL), 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn validate_cheight_computes_and_marks_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;
        win.w_topline = 5; // != cursor lnum (1), so win_get_fill (always 0) applies
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        win.w_cline_folded = true; // pre-set, must be overwritten to false

        unsafe { validate_cheight(&mut win as *mut WinT) };

        assert_eq!(win.w_cline_height, 1); // single unwrapped, unfolded line
        assert!(!win.w_cline_folded);
        assert_ne!(win.w_valid & i32::from(w_valid::VALID_CHEIGHT), 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn validate_cheight_is_a_noop_when_already_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_CHEIGHT);
        win.w_cline_height = 42; // sentinel: must survive untouched
        win.w_cline_folded = true;

        unsafe { validate_cheight(&mut win as *mut WinT) };

        assert_eq!(win.w_cline_height, 42);
        assert!(win.w_cline_folded);
    }

    #[test]
    fn validate_cursor_col_basic_no_wrap_no_leftcol() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 };

        unsafe { validate_cursor_col(&mut win as *mut WinT) };

        assert_eq!(win.w_wcol, 2);
        assert_ne!(win.w_valid & i32::from(w_valid::VALID_WCOL), 0);
        assert_ne!(win.w_valid & i32::from(w_valid::VALID_VIRTCOL), 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn validate_cursor_col_short_circuits_when_already_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 };
        // w_valid_cursor must match w_cursor (and w_valid_leftcol/
        // w_valid_skipcol match their counterparts) so the internal
        // check_cursor_moved call (via validate_virtcol) is a true
        // no-op and doesn't clear the bits pre-marked below.
        win.w_valid_cursor = win.w_cursor;
        // Pre-mark both VALID_VIRTCOL and VALID_WCOL, with a
        // deliberately WRONG w_wcol - it must be left untouched since
        // the function should short-circuit without recomputing.
        win.w_valid = i32::from(w_valid::VALID_VIRTCOL) | i32::from(w_valid::VALID_WCOL);
        win.w_wcol = 999;

        unsafe { validate_cursor_col(&mut win as *mut WinT) };

        assert_eq!(win.w_wcol, 999);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn validate_cursor_col_clamps_to_zero_when_leftcol_exceeds_col() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 };
        win.w_leftcol = 5; // scrolled right past the cursor's own column

        unsafe { validate_cursor_col(&mut win as *mut WinT) };

        assert_eq!(win.w_wcol, 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn cursor_valid_true_when_wrow_and_wcol_both_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_WCOL);
        // Keep w_valid_cursor/w_leftcol/w_skipcol matching defaults so
        // check_cursor_moved (called internally) doesn't clear them.
        assert!(unsafe { cursor_valid(&mut win as *mut WinT) });
    }

    #[test]
    fn cursor_valid_false_when_only_wrow_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW);
        assert!(!unsafe { cursor_valid(&mut win as *mut WinT) });
    }

    #[test]
    fn update_curswant_force_copies_virtcol_and_clears_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 4, coladd: 0 };
        win.w_set_curswant = true;

        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        unsafe { update_curswant_force() };

        assert_eq!(win.w_curswant, 4);
        assert!(!win.w_set_curswant);

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn update_curswant_is_noop_when_flag_not_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_cursor = crate::pos_defs::PosT { lnum: 0, col: 4, coladd: 0 };
        win.w_set_curswant = false;
        win.w_curswant = 99;

        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        unsafe { update_curswant() };

        assert_eq!(win.w_curswant, 99); // untouched

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn changed_cline_bef_curs_clears_expected_bits_only() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_WCOL)
            | i32::from(w_valid::VALID_VIRTCOL)
            | i32::from(w_valid::VALID_CROW)
            | i32::from(w_valid::VALID_CHEIGHT)
            | i32::from(w_valid::VALID_TOPLINE)
            | i32::from(w_valid::VALID_BOTLINE); // must survive

        unsafe { changed_cline_bef_curs(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_BOTLINE));
    }

    #[test]
    fn changed_line_abv_curs_win_clears_expected_bits_only() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_TOPLINE)
            | i32::from(w_valid::VALID_BOTLINE); // must survive

        unsafe { changed_line_abv_curs_win(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_BOTLINE));
    }

    #[test]
    fn changed_line_abv_curs_operates_on_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_WROW) | i32::from(w_valid::VALID_BOTLINE);

        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        unsafe { changed_line_abv_curs() };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_BOTLINE));

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    // --- validate_botline_win / comp_botline ---

    #[test]
    fn validate_botline_win_skips_recompute_when_already_valid() {
        // No GLOBALS/lock needed: comp_botline is never reached.
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_BOTLINE);
        win.w_botline = 42; // sentinel: must stay untouched

        unsafe { validate_botline_win(&mut win as *mut WinT) };

        assert_eq!(win.w_botline, 42);
        assert_eq!(win.w_valid, i32::from(w_valid::VALID_BOTLINE));
    }

    #[test]
    fn validate_botline_win_small_buffer_fits_entirely_in_the_window() {
        // 3 lines, each contributing n=1 (wo_wrap=0's fast path -
        // real line content is never needed), all fit within a
        // 10-row window starting at the topline.
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 3;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1;
        win.w_view_width = 20;
        win.w_view_height = 10;

        unsafe { validate_botline_win(&mut win as *mut WinT) };

        // lnum runs 1, 2, 3 (each n=1, done=1,2,3), then 4 > line
        // count(3) stops the loop - w_botline is the line "just below
        // the window".
        assert_eq!(win.w_botline, 4);
        assert_eq!(
            win.w_valid & (i32::from(w_valid::VALID_BOTLINE) | i32::from(w_valid::VALID_BOTLINE_AP)),
            i32::from(w_valid::VALID_BOTLINE) | i32::from(w_valid::VALID_BOTLINE_AP)
        );
        assert!(win.w_viewport_invalid);
        // set_empty_rows(wp, 3): botline(4) > line_count(3), so the
        // filler-lookup block is skipped - matches
        // set_empty_rows_botline_past_the_end_skips_the_filler_lookup's
        // own established shape.
        assert_eq!(win.w_empty_rows, 7); // 10 - 3
        assert_eq!(win.w_filler_rows, 0);
    }

    #[test]
    fn validate_botline_win_large_buffer_stops_the_loop_early() {
        // A tiny 2-row window against a 5-line buffer: only 2 lines
        // fit, the 3rd would overflow and is never counted.
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 5;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1;
        win.w_view_width = 20;
        win.w_view_height = 2;

        unsafe { validate_botline_win(&mut win as *mut WinT) };

        // lnum=1: done 0+1=1<=2, advance to 2. lnum=2: done 1+1=2<=2,
        // advance to 3. lnum=3: done 2+1=3>2 -> break with lnum still
        // at 3 (never advanced to `last`).
        assert_eq!(win.w_botline, 3);
        // set_empty_rows(wp, 2): botline(3) <= line_count(5), so
        // win_get_fill(3) runs (needs CurtabGuard) - always 0 today.
        assert_eq!(win.w_empty_rows, 0); // 2 - 2, then the "else" branch keeps it 0
        assert_eq!(win.w_filler_rows, 0);
    }

    #[test]
    fn comp_botline_starts_from_the_cursor_line_when_valid_crow_already_set() {
        // VALID_CROW pre-set with w_cursor == w_valid_cursor (and
        // every other check_cursor_moved-compared field matching)
        // means check_cursor_moved is a total no-op here, so the
        // "start from the cursor line" branch (not "start from
        // w_topline") is the one actually exercised.
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 3;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1; // != 2/3, so win_get_fill's path is used for both
        win.w_view_width = 20;
        win.w_view_height = 10;
        win.w_cursor = crate::pos_defs::PosT { lnum: 2, col: 0, coladd: 0 };
        win.w_valid_cursor = win.w_cursor;
        win.w_valid = i32::from(w_valid::VALID_CROW);
        win.w_cline_row = 5; // arbitrary starting "done" value

        unsafe { comp_botline(&mut win as *mut WinT) };

        // lnum starts at w_cursor.lnum(2), done starts at
        // w_cline_row(5): lnum=2 (== cursor.lnum) -> w_cline_row=5,
        // w_cline_height=1, done=5+1=6, advance to 3; lnum=3 (>
        // cursor.lnum, no cline_row update) -> done=6+1=7, advance to
        // 4; lnum=4 > line_count(3) stops.
        assert_eq!(win.w_botline, 4);
        assert_eq!(win.w_cline_row, 5);
        assert_eq!(win.w_cline_height, 1);
        assert!(!win.w_cline_folded);
        assert_eq!(win.w_empty_rows, 3); // 10 - 7
    }

    #[test]
    fn comp_botline_never_updates_cline_row_when_the_loop_breaks_before_the_cursor_line() {
        // Cursor sits past where the (tiny) window's own loop breaks -
        // w_cline_row/w_cline_height must stay at their sentinel
        // values, never touched.
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 5;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1;
        win.w_view_width = 20;
        win.w_view_height = 2;
        win.w_cursor = crate::pos_defs::PosT { lnum: 4, col: 0, coladd: 0 };
        win.w_valid_cursor = win.w_cursor; // matches: check_cursor_moved is a no-op
        win.w_cline_row = 99;
        win.w_cline_height = 99;

        unsafe { comp_botline(&mut win as *mut WinT) };

        // Loop only ever reaches lnum 1, 2, 3 (breaking at 3) - cursor
        // line 4 is never visited, so the cline_row-update branch
        // never fires.
        assert_eq!(win.w_botline, 3);
        assert_eq!(win.w_cline_row, 99);
        assert_eq!(win.w_cline_height, 99);
        assert_eq!(
            win.w_valid & (i32::from(w_valid::VALID_CROW) | i32::from(w_valid::VALID_CHEIGHT)),
            0
        );
    }

    #[test]
    fn invalidate_botline_win_clears_both_botline_bits() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_BOTLINE)
            | i32::from(w_valid::VALID_BOTLINE_AP)
            | i32::from(w_valid::VALID_WROW); // must survive

        unsafe { invalidate_botline_win(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_WROW));
    }

    #[test]
    fn approximate_botline_win_clears_only_botline() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_valid = i32::from(w_valid::VALID_BOTLINE) | i32::from(w_valid::VALID_BOTLINE_AP); // must survive

        unsafe { approximate_botline_win(&mut win as *mut WinT) };

        assert_eq!(win.w_valid, i32::from(w_valid::VALID_BOTLINE_AP));
    }

    #[test]
    fn set_topline_basic_change_updates_botline_and_resets_topfill() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 100;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1;
        win.w_botline = 20;
        win.w_topfill = 5;
        win.w_valid = i32::from(w_valid::VALID_WROW)
            | i32::from(w_valid::VALID_CROW)
            | i32::from(w_valid::VALID_BOTLINE)
            | i32::from(w_valid::VALID_TOPLINE)
            | i32::from(w_valid::VALID_WCOL); // must survive

        unsafe { set_topline(&mut win as *mut WinT, 10) };

        assert_eq!(win.w_topline, 10);
        assert_eq!(win.w_botline, 29); // 20 + (10 - 1)
        assert_eq!(win.w_topfill, 0); // lnum changed
        assert!(win.w_topline_was_set);
        assert_eq!(win.w_valid, i32::from(w_valid::VALID_WCOL));
    }

    #[test]
    fn set_topline_same_line_preserves_topfill() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 100;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 5;
        win.w_botline = 20;
        win.w_topfill = 3;

        unsafe { set_topline(&mut win as *mut WinT, 5) };

        assert_eq!(win.w_topline, 5);
        assert_eq!(win.w_botline, 20); // 20 + (5 - 5)
        assert_eq!(win.w_topfill, 3); // lnum unchanged -> filler lines kept
    }

    #[test]
    fn set_topline_clamps_botline_to_line_count_plus_one() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1;
        win.w_botline = 3;

        unsafe { set_topline(&mut win as *mut WinT, 4) };

        // 3 + (4 - 1) = 6, but clamped to line_count(5) + 1 = 6 exactly
        // here - use a bigger jump to actually exercise the clamp.
        assert_eq!(win.w_botline, 6);

        win.w_topline = 4;
        win.w_botline = 6;
        unsafe { set_topline(&mut win as *mut WinT, 100) };
        // 6 + (100 - 4) = 102, clamped down to 5 + 1 = 6.
        assert_eq!(win.w_botline, 6);
    }

    #[test]
    fn sms_marker_overlap_zero_in_showbreak_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_sbr = Some(b">>".to_vec());
        assert_eq!(unsafe { sms_marker_overlap(&mut win, -1) }, 0);
    }

    #[test]
    fn sms_marker_overlap_one_when_list_and_precedes_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_list = 1;
        win.w_p_lcs_chars.prec = u32::from(b'<');
        assert_eq!(unsafe { sms_marker_overlap(&mut win, -1) }, 1);
    }

    #[test]
    fn sms_marker_overlap_uses_extra2_directly_when_given() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        // No showbreak, no list+precedes - falls through to the
        // extra2-based formula.
        assert_eq!(unsafe { sms_marker_overlap(&mut win, 5) }, 0); // > 3
        assert_eq!(unsafe { sms_marker_overlap(&mut win, 1) }, 2); // 3 - 1
        assert_eq!(unsafe { sms_marker_overlap(&mut win, 3) }, 0); // 3 - 3
    }

    #[test]
    fn sms_marker_overlap_computes_extra2_from_win_col_off_when_negative_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        // No number/foldcolumn/sign/cpo 'n' - win_col_off == win_col_off2 == 0,
        // so extra2 = 0 - 0 = 0, giving 3 - 0 = 3.
        assert_eq!(unsafe { sms_marker_overlap(&mut win, -1) }, 3);
    }

    #[test]
    fn skipcol_from_plines_zero_offset_is_zero() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 20;
        assert_eq!(unsafe { skipcol_from_plines(&mut win, 0) }, 0);
    }

    #[test]
    fn skipcol_from_plines_one_offset_is_width1() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 20;
        // width1 = 20 - win_col_off(0) = 20.
        assert_eq!(unsafe { skipcol_from_plines(&mut win, 1) }, 20);
    }

    #[test]
    fn skipcol_from_plines_multiple_offset_adds_width2_per_extra_line() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 20;
        // width1 = 20, width2 = width1 + win_col_off2(0) = 20.
        // skipcol = width1(20) + width2(20) * (3 - 1) = 20 + 40 = 60.
        assert_eq!(unsafe { skipcol_from_plines(&mut win, 3) }, 60);
    }

    #[test]
    fn reset_skipcol_zero_is_a_noop() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_skipcol = 0;
        reset_skipcol(&mut win);
        assert_eq!(win.w_skipcol, 0);
    }

    #[test]
    fn reset_skipcol_nonzero_is_cleared() {
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_skipcol = 42;
        reset_skipcol(&mut win);
        assert_eq!(win.w_skipcol, 0);
    }

    #[test]
    fn adjust_skipcol_noop_when_wrap_is_off() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_wrap = 0;
        win.w_onebuf_opt.wo_sms = 1;
        win.w_skipcol = 7;
        let _guard = CurwinGuard::set(&mut win as *mut WinT);
        unsafe { adjust_skipcol() };
        assert_eq!(win.w_skipcol, 7);
    }

    #[test]
    fn adjust_skipcol_noop_by_default_since_smoothscroll_defaults_off() {
        // This is the REAL, always-taken-today fast path: 'wrap' can
        // be on, but 'smoothscroll' (wo_sms) defaults to 0 and nothing
        // in this crate can currently turn it on.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_wrap = 1;
        win.w_onebuf_opt.wo_sms = 0;
        win.w_skipcol = 7;
        let _guard = CurwinGuard::set(&mut win as *mut WinT);
        unsafe { adjust_skipcol() };
        assert_eq!(win.w_skipcol, 7);
    }

    #[test]
    fn adjust_skipcol_noop_when_cursor_not_on_topline() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_wrap = 1;
        win.w_onebuf_opt.wo_sms = 1;
        win.w_topline = 5;
        win.w_cursor.lnum = 1; // != w_topline
        win.w_skipcol = 7;
        let _guard = CurwinGuard::set(&mut win as *mut WinT);
        unsafe { adjust_skipcol() };
        assert_eq!(win.w_skipcol, 7);
    }

    #[test]
    fn adjust_skipcol_noop_when_width1_is_not_positive() {
        // w_view_width smaller than win_col_off's own contribution
        // (a number column here) makes width1 <= 0 - another real,
        // early-return fast path.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_wrap = 1;
        win.w_onebuf_opt.wo_sms = 1;
        win.w_onebuf_opt.wo_nu = 1;
        win.w_nrwidth_line_count = -1; // force a genuine number_width recompute
        win.w_view_width = 1; // smaller than the resulting number column
        win.w_topline = 1;
        win.w_cursor.lnum = 1;
        win.w_skipcol = 7;
        let _guard = CurwinGuard::set(&mut win as *mut WinT);
        unsafe { adjust_skipcol() };
        assert_eq!(win.w_skipcol, 7);
    }

    #[test]
    fn adjust_skipcol_resets_skipcol_when_the_line_fits_the_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_wrap = 1;
        win.w_onebuf_opt.wo_sms = 1;
        win.w_view_width = 20;
        win.w_view_height = 1; // must EXACTLY match the computed w_cline_height (1 unwrapped line)
        win.w_topline = 1;
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        win.w_skipcol = 3; // pre-set, must be reset to 0
        let _guard = CurwinGuard::set(&mut win as *mut WinT);

        unsafe { adjust_skipcol() };
        assert_eq!(win.w_skipcol, 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn use_scrolloffpad_false_when_scrolloff_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_so = 0;
        win.w_onebuf_opt.wo_sop = 3;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(!unsafe { use_scrolloffpad(&win) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn use_scrolloffpad_false_when_scrolloffpad_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_so = 5;
        win.w_onebuf_opt.wo_sop = 0;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(!unsafe { use_scrolloffpad(&win) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn use_scrolloffpad_true_when_both_positive() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_so = 5;
        win.w_onebuf_opt.wo_sop = 3;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(unsafe { use_scrolloffpad(&win) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn scrolloffpad_eof_pressure_false_when_use_scrolloffpad_is_false() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 10;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_so = 0; // use_scrolloffpad() false
        win.w_onebuf_opt.wo_sop = 3;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(!unsafe { scrolloffpad_eof_pressure(&win, 9, 2) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn scrolloffpad_eof_pressure_false_when_so_not_positive() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 10;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_so = 5;
        win.w_onebuf_opt.wo_sop = 3;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(!unsafe { scrolloffpad_eof_pressure(&win, 9, 0) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn scrolloffpad_eof_pressure_true_near_end_of_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 10;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_so = 5;
        win.w_onebuf_opt.wo_sop = 3;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        // lnum(9) > line_count(10) - so(2) = 8 -> true.
        assert!(unsafe { scrolloffpad_eof_pressure(&win, 9, 2) });
        // lnum(8) > line_count(10) - so(2) = 8 -> false (not strictly greater).
        assert!(!unsafe { scrolloffpad_eof_pressure(&win, 8, 2) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    // --- vcol2col / virtcol2col ---

    #[test]
    fn vcol2col_ascii_returns_matching_byte_offset() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);

        assert_eq!(unsafe { vcol2col(&mut win as *mut WinT, 1, 0) }, (0, 0));
        assert_eq!(unsafe { vcol2col(&mut win as *mut WinT, 1, 3) }, (3, 0));

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn vcol2col_past_end_of_line_reports_overshoot_coladd() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);

        // "hello" is 5 cells wide (0..5); requesting vcol 100 stops at
        // the trailing NUL (byte offset 5), reporting the overshoot as
        // coladd (matching the original's own unclamped `*coladdp`).
        assert_eq!(unsafe { vcol2col(&mut win as *mut WinT, 1, 100) }, (5, 95));

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn vcol2col_lands_inside_a_double_width_character_reports_coladd() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        // "a" + CJK "日" (U+65E5, double-width, 3 UTF-8 bytes) + "b".
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, "a日b\0".as_bytes()) },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);

        // vcol 1 lands exactly on the CJK char's own start byte.
        assert_eq!(unsafe { vcol2col(&mut win as *mut WinT, 1, 1) }, (1, 0));
        // vcol 2 lands in the *middle* of the double-width CJK char
        // (which occupies vcols 1-2) - byte offset stays at its start
        // (1), with coladd 1 reporting the one-cell overshoot into it.
        assert_eq!(unsafe { vcol2col(&mut win as *mut WinT, 1, 2) }, (1, 1));
        // vcol 3 lands on "b", right after the CJK char (3 UTF-8 bytes
        // starting at offset 1, so "b" is at offset 4).
        assert_eq!(unsafe { vcol2col(&mut win as *mut WinT, 1, 3) }, (4, 0));

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn virtcol2col_first_column_returns_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);

        assert_eq!(unsafe { virtcol2col(&mut win as *mut WinT, 1, 1) }, 1);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn virtcol2col_past_end_of_line_returns_last_char_byte_index() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);

        // "hello" has 5 one-cell-wide characters (vcols 0-4, 1-based
        // columns 1-5); asking for column 100 clamps to the LAST
        // character's own byte index (5, 1-based - the 'o').
        assert_eq!(unsafe { virtcol2col(&mut win as *mut WinT, 1, 100) }, 5);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn virtcol2col_lands_after_double_width_character_returns_correct_index() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, "a日b\0".as_bytes()) },
            crate::vim_defs::OK
        );
        let mut win = win_with_buf(&mut buf as *mut BufT);

        // Column 4 (1-based) = vcol 3 (0-based), landing exactly on
        // "b" (byte offset 4, 0-based) - 1-based byte index 5.
        assert_eq!(unsafe { virtcol2col(&mut win as *mut WinT, 1, 4) }, 5);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn virtcol2col_empty_line_returns_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        let mut win = win_with_buf(&mut buf as *mut BufT);

        assert_eq!(unsafe { virtcol2col(&mut win as *mut WinT, 1, 1) }, 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    // --- textpos2screenpos ---

    #[test]
    fn textpos2screenpos_top_left_character_reports_row_one_col_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello\0") },
            crate::vim_defs::OK
        );
        buf.b_ml.ml_line_count = 1;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1;
        win.w_botline = 5;
        win.w_view_width = 80;
        win.w_view_height = 24;

        let mut pos = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let mut row = -99;
        let mut scol = -99;
        let mut ccol = -99;
        let mut ecol = -99;
        unsafe {
            textpos2screenpos(&mut win as *mut WinT, &mut pos, &mut row, &mut scol, &mut ccol, &mut ecol, false);
        }

        // Top-left character of the window with zero winrow/wincol/
        // number-column offsets: every reported value is 1 (the
        // 1-based screen coordinate of the window's own top-left
        // corner).
        assert_eq!((row, scol, ccol, ecol), (1, 1, 1, 1));

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn textpos2screenpos_line_outside_visible_range_not_local_reports_all_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 3; // lnum 10 below doesn't exist either
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1;
        win.w_botline = 5;
        win.w_view_width = 80;
        win.w_view_height = 24;

        let mut pos = crate::pos_defs::PosT { lnum: 10, col: 0, coladd: 0 };
        let mut row = -99;
        let mut scol = -99;
        let mut ccol = -99;
        let mut ecol = -99;
        unsafe {
            textpos2screenpos(&mut win as *mut WinT, &mut pos, &mut row, &mut scol, &mut ccol, &mut ecol, false);
        }

        // lnum 10 is beyond w_botline(5) AND beyond the buffer's own
        // line count(3) - neither "visible_row" nor "existing_row"
        // hold, so the whole coloff-computing block is skipped
        // entirely; row stays at the initial if/else-if/else's own
        // "not local, not visible" result (0), and scol/ccol/ecol
        // never get touched past their own zero initializers.
        assert_eq!((row, scol, ccol, ecol), (0, 0, 0, 0));
    }

    #[test]
    fn textpos2screenpos_column_beyond_window_width_not_local_reports_row_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"hello world\0") },
            crate::vim_defs::OK
        );
        buf.b_ml.ml_line_count = 1;
        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_topline = 1;
        win.w_botline = 5;
        win.w_view_width = 5; // narrower than the resolved column (8)
        win.w_view_height = 24;
        win.w_onebuf_opt.wo_wrap = 0;

        // Column 8 (0-based) = the 'r' in "world" - well past the
        // 5-column-wide window.
        let mut pos = crate::pos_defs::PosT { lnum: 1, col: 8, coladd: 0 };
        let mut row = -99;
        let mut scol = -99;
        let mut ccol = -99;
        let mut ecol = -99;
        unsafe {
            textpos2screenpos(&mut win as *mut WinT, &mut pos, &mut row, &mut scol, &mut ccol, &mut ecol, false);
        }

        assert_eq!((row, scol, ccol, ecol), (0, 0, 0, 0));

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn scrolljump_value_returns_the_option_directly_when_non_negative() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sj;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sj = 5;

        let win = WinT::default();
        assert_eq!(scrolljump_value(&win), 5);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sj = prev;
    }

    #[test]
    fn scrolljump_value_computes_a_percentage_of_window_height_when_negative() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sj;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sj = -50;

        let win = WinT { w_view_height: 20, ..Default::default() };
        assert_eq!(scrolljump_value(&win), 10); // (20 * 50) / 100

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sj = prev;
    }

    #[test]
    fn topline_back_winheight_moves_up_one_line_and_computes_its_height() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"one\0") },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(&mut buf, 1, b"two\0", 4, false) },
            crate::vim_defs::OK
        );

        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;

        let mut lp = LineoffT { lnum: 2, fill: 0, height: 0 };
        unsafe { topline_back_winheight(&mut win as *mut WinT, &mut lp, true) };

        assert_eq!(lp.lnum, 1);
        assert_eq!(lp.fill, 0);
        assert_eq!(lp.height, 1); // "one" fits on one screen line

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn topline_back_winheight_stops_before_line_1_with_maxcol_height() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);

        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;

        let mut lp = LineoffT { lnum: 1, fill: 0, height: 0 };
        unsafe { topline_back_winheight(&mut win as *mut WinT, &mut lp, true) };

        assert_eq!(lp.lnum, 0);
        assert_eq!(lp.height, crate::pos_defs::MAXCOL);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn topline_back_delegates_with_winheight_true() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"one\0") },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(&mut buf, 1, b"two\0", 4, false) },
            crate::vim_defs::OK
        );

        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;

        let mut lp = LineoffT { lnum: 2, fill: 0, height: 0 };
        unsafe { topline_back(&mut win as *mut WinT, &mut lp) };

        assert_eq!(lp.lnum, 1);
        assert_eq!(lp.height, 1);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn botline_forw_moves_down_one_line_and_computes_its_height() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"one\0") },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(&mut buf, 1, b"two\0", 4, false) },
            crate::vim_defs::OK
        );

        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;

        let mut lp = LineoffT { lnum: 1, fill: 0, height: 0 };
        unsafe { botline_forw(&mut win as *mut WinT, &mut lp) };

        assert_eq!(lp.lnum, 2);
        assert_eq!(lp.fill, 0);
        assert_eq!(lp.height, 1); // "two" fits on one screen line

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn botline_forw_stops_past_the_last_line_with_maxcol_height() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);

        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;

        let mut lp = LineoffT { lnum: 1, fill: 0, height: 0 };
        unsafe { botline_forw(&mut win as *mut WinT, &mut lp) };

        assert_eq!(lp.lnum, 2);
        assert_eq!(lp.height, crate::pos_defs::MAXCOL);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    // ---- cursor_correct_sms ----

    #[test]
    fn cursor_correct_sms_no_op_when_smoothscroll_is_off() {
        let mut win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_sms: 0, wo_wrap: 1, ..Default::default() },
            w_cursor: crate::pos_defs::PosT { lnum: 1, col: 5, coladd: 0 },
            w_topline: 1,
            w_skipcol: 30,
            ..Default::default()
        };

        unsafe { cursor_correct_sms(&mut win as *mut WinT) };

        assert_eq!(win.w_cursor.col, 5);
    }

    #[test]
    fn cursor_correct_sms_no_op_when_wrap_is_off() {
        let mut win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_sms: 1, wo_wrap: 0, ..Default::default() },
            w_cursor: crate::pos_defs::PosT { lnum: 1, col: 5, coladd: 0 },
            w_topline: 1,
            w_skipcol: 30,
            ..Default::default()
        };

        unsafe { cursor_correct_sms(&mut win as *mut WinT) };

        assert_eq!(win.w_cursor.col, 5);
    }

    #[test]
    fn cursor_correct_sms_no_op_when_cursor_is_not_on_topline() {
        let mut win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_sms: 1, wo_wrap: 1, ..Default::default() },
            w_cursor: crate::pos_defs::PosT { lnum: 2, col: 5, coladd: 0 },
            w_topline: 1,
            w_skipcol: 30,
            ..Default::default()
        };

        unsafe { cursor_correct_sms(&mut win as *mut WinT) };

        assert_eq!(win.w_cursor.col, 5);
        assert_eq!(win.w_cursor.lnum, 2);
    }

    #[test]
    fn cursor_correct_sms_no_op_when_already_fully_visible() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"aaaaaaaaaaaaaaaaaaaa\0") },
            crate::vim_defs::OK
        );

        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_sms = 1;
        win.w_onebuf_opt.wo_wrap = 1;
        win.w_view_width = 20;
        win.w_view_height = 10;
        // skipcol == 0 and 'scrolloff' == 0 (the default): the
        // "not-yet-scrolled" case where nothing needs to move.
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 5, coladd: 0 };
        win.w_topline = 1;
        win.w_valid = i32::from(w_valid::VALID_VIRTCOL);

        unsafe { cursor_correct_sms(&mut win as *mut WinT) };

        assert_eq!(win.w_cursor.col, 5);
        // Since col == w_virtcol, the whole "reposition" block (which
        // would clear these bits) is never entered.
        assert_eq!(win.w_valid, i32::from(w_valid::VALID_VIRTCOL));

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn cursor_correct_sms_repositions_cursor_when_scrolled_past_skipcol() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        let mut line = vec![b'a'; 60];
        line.push(0); // trailing NUL, matching this crate's own line-storage convention
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, &line) },
            crate::vim_defs::OK
        );

        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_sms = 1;
        win.w_onebuf_opt.wo_wrap = 1;
        win.w_view_width = 20;
        win.w_view_height = 10;
        win.w_skipcol = 30;
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 25, coladd: 0 };
        win.w_topline = 1;
        win.w_valid = i32::from(w_valid::VALID_VIRTCOL);
        // coladvance's own "not on a TAB" branch reads GLOBALS.curwin
        // directly (a verified upstream quirk, see coladvance's own
        // doc comment) - compute win's pointer exactly once and reuse
        // it for GLOBALS.curwin, the function call, and every
        // subsequent read, matching this crate's established Tree
        // Borrows discipline.
        let win_ptr = &mut win as *mut WinT;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = win_ptr;

        unsafe { cursor_correct_sms(win_ptr) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;

        // Hand-traced: width1=width2=20 (no col_off), skipcol=30,
        // 'scrolloff'=0 -> so_cols=0. overlap = sms_marker_overlap
        // (no 'showbreak'/'list' set) = 3 - 0 = 3. top = 30+3 = 33,
        // bot = 30+20+180-0 = 230. Original virtcol (plain ASCII
        // col 25) = 25, which is < top(33), and 25 is NOT < width1
        // (20), so the loop adds width2 once: 25+20 = 45 (>= 33,
        // stops). 45 != 25 -> coladvance(wp, 45) moves the cursor to
        // byte column 45 (plain ASCII, 1 byte per column).
        assert_eq!(unsafe { &*win_ptr }.w_cursor.col, 45);
        assert_eq!(unsafe { &*win_ptr }.w_curswant, 45);
        // validate_virtcol()'s own VALID_VIRTCOL mark, plus the 4
        // other bits validate_virtcol never touches, are all cleared
        // after the reposition.
        assert_eq!(unsafe { &*win_ptr }.w_valid, 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    // ---- changed_window_setting_all ----

    #[test]
    fn changed_window_setting_all_touches_every_window_in_every_tabpage() {
        let _lock = crate::globals::global_state_test_lock();

        let mut win_c = WinT { w_lines_valid: 5, w_valid: -1, ..Default::default() };
        let mut other_tp = crate::buffer_defs::TabpageT {
            tp_firstwin: &mut win_c as *mut WinT,
            ..Default::default()
        };
        let mut win_b = WinT { w_lines_valid: 5, w_valid: -1, ..Default::default() };
        let mut win_a = WinT {
            w_lines_valid: 5,
            w_valid: -1,
            w_next: &mut win_b as *mut WinT,
            ..Default::default()
        };
        let mut curtab = crate::buffer_defs::TabpageT {
            tp_next: &mut other_tp as *mut crate::buffer_defs::TabpageT,
            ..Default::default()
        };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_first_tabpage, prev_curtab, prev_firstwin) = (g.first_tabpage, g.curtab, g.firstwin);
        g.first_tabpage = &mut curtab as *mut crate::buffer_defs::TabpageT;
        g.curtab = &mut curtab as *mut crate::buffer_defs::TabpageT;
        g.firstwin = &mut win_a as *mut WinT; // GLOBALS.firstwin backs the CURRENT tabpage's own list

        unsafe { changed_window_setting_all() };

        for w in [&win_a, &win_b, &win_c] {
            assert_eq!(w.w_lines_valid, 0);
            assert_eq!(
                w.w_valid
                    & (i32::from(w_valid::VALID_BOTLINE)
                        | i32::from(w_valid::VALID_BOTLINE_AP)
                        | i32::from(w_valid::VALID_TOPLINE)),
                0
            );
        }

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.first_tabpage = prev_first_tabpage;
        g.curtab = prev_curtab;
        g.firstwin = prev_firstwin;
    }

    // ---- check_top_offset ----

    #[test]
    fn check_top_offset_far_from_top_returns_false_without_scanning() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::NORMAL as i32;

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);

        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_onebuf_opt.wo_so = 0; // 'scrolloff'=0
        win.w_topline = 1;
        win.w_cursor.lnum = 1; // at topline already

        assert!(!unsafe { check_top_offset(&mut win as *mut WinT) });

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn check_top_offset_near_top_returns_true_via_early_break() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::NORMAL as i32;
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"one\0") }, crate::vim_defs::OK);
        for (after, line) in [(1, b"two\0" as &[u8]), (2, b"three\0"), (3, b"four\0")] {
            assert_eq!(
                unsafe { crate::memline::ml_append_buf(&mut buf, after, line, line.len() as i32, false) },
                crate::vim_defs::OK
            );
        }

        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 10;
        win.w_onebuf_opt.wo_so = 2; // 'scrolloff'=2
        win.w_topline = 3;
        win.w_cursor.lnum = 4; // one line below topline
        win.w_topfill = 0;

        // Hand-traced: guard passes (4 < 3+2=5). Loop iteration 1:
        // topline_back moves loff to lnum=3/height=1 - not above
        // w_topline(3), so n becomes 1. Iteration 2: topline_back
        // moves loff to lnum=2/height=1 - THIS time loff.lnum(2) <
        // w_topline(3), so it breaks BEFORE adding height. Final
        // n=1 < so=2 -> true.
        assert!(unsafe { check_top_offset(&mut win as *mut WinT) });

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn check_top_offset_wrapped_line_accumulates_enough_height_in_one_step() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::NORMAL as i32;
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        // 20 'a's - at w_view_width=5 with wrap on, this line alone
        // occupies ceil(20/5)=4 screen rows.
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"aaaaaaaaaaaaaaaaaaaa\0") },
            crate::vim_defs::OK
        );
        assert_eq!(unsafe { crate::memline::ml_append_buf(&mut buf, 1, b"two\0", 4, false) }, crate::vim_defs::OK);

        let mut win = win_with_buf(&mut buf as *mut BufT);
        win.w_view_width = 5;
        win.w_view_height = 20; // large enough that limit_winheight never clamps
        win.w_onebuf_opt.wo_wrap = 1;
        win.w_onebuf_opt.wo_so = 3; // 'scrolloff'=3
        win.w_topline = 1;
        win.w_cursor.lnum = 2; // one raw line below topline - guard passes via distance
        win.w_topfill = 0;

        // Hand-traced: guard passes (2 < 1+3=4). The loop's FIRST
        // topline_back call walks from line 2 back to line 1 (still
        // >= w_topline, so no break) and computes its height via
        // plines_win_nofill as 4 (the wrapped line above) - already
        // >= so(3) after just one iteration, so the loop exits via the
        // while condition (not a break) without ever needing a second
        // step. Enough real (wrapped) height exists above the cursor,
        // so no scroll is needed.
        assert!(!unsafe { check_top_offset(&mut win as *mut WinT) });

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }
}
