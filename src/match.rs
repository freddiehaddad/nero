//! Translated from `src/nvim/match.c` (tractable core only).
//!
//! `match.c` implements the `:match`/`matchadd()`/`matchaddpos()`
//! highlighting-match subsystem, keyed on `WinT.w_match_head` (a
//! linked list of `matchitem_T` entries). `matchitem_T` itself is
//! still an opaque placeholder (`crate::types_defs::MatchitemT`, see
//! `buffer_defs.rs`'s own doc comment) - it needs `regmmatch_T`/
//! `regexp_defs.h` (the real regex engine, phase 7), so this crate
//! cannot yet construct or read a real match entry's own fields.
//!
//! However, since NOTHING currently translated can ever populate
//! `w_match_head` (it starts, and can only currently stay, `NULL`),
//! every function whose real body's own "iterate existing matches"
//! loop is gated on `w_match_head != NULL` degrades to its own
//! always-taken "no matches exist" fast path - the SAME
//! "always-real-fast-path" pattern already established elsewhere in
//! this crate (e.g. `autocmd.rs`'s `AUTOCMDS`). Translated on this
//! basis: `get_optional_window` (`eval/funcs.c`), `clear_matches`/
//! `f_clearmatches`/`f_getmatches`/`get_match`/`f_matcharg`
//! (`matcharg()` - its own `m != NULL` branch, needing `syn_id2name`/
//! the highlight-group registry, is never reached since `get_match`
//! always returns `null`).
//!
//! Deferred: `matchadd()`/`matchaddpos()`/`matchdelete()`/`getmatches()`'s
//! own item-conversion loop body, `:match`/`:2match`/`:3match`, and
//! everything else needing real `matchitem_T` fields.

use crate::buffer_defs::WinT;
use crate::eval::typval_defs::TypvalT;

/// Resolve the optional `{win}` argument at `argvars[idx]`
/// (`curwin` if omitted) (`get_optional_window`, `eval/funcs.c`).
///
/// The original's own `emsg(_(e_invalwindow))` display, for an
/// explicitly-provided-but-unresolvable window, is omitted (matching
/// this crate's established policy) - the `null` return value itself
/// is kept.
///
/// # Safety
/// Touches `crate::globals::GLOBALS`, with the usual "no overlapping
/// live access" requirement. Forwarded from
/// [`crate::window::find_win_by_nr_or_id`]'s own safety doc.
#[must_use]
pub unsafe fn get_optional_window(argvars: &[TypvalT], idx: usize) -> *mut WinT {
    let Some(arg) = argvars.get(idx) else {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::window::find_win_by_nr_or_id(arg) }
}

/// Record whether the cursor sits inside the match `shl`
/// (`check_cur_search_hl`), used to pick the `CurSearch` highlight.
///
/// The match may span several lines: `linecount` is how many lines it
/// covers, derived from sub-match 0's own start and end positions,
/// whose line numbers are RELATIVE to `shl.lnum`.
///
/// The column tests apply only at the edges. On the first line the
/// cursor must be at or after the start column, and on the last line
/// strictly before the end column; on any line in between, the whole
/// line is inside the match and the column is irrelevant. The end
/// column is exclusive, the start column inclusive.
pub fn check_cur_search_hl(wp: &WinT, shl: &mut crate::buffer_defs::MatchT) {
    let linecount = shl.rm.endpos[0].lnum - shl.rm.startpos[0].lnum;
    let cursor = wp.w_cursor;

    shl.has_cursor = cursor.lnum >= shl.lnum
        && cursor.lnum <= shl.lnum + linecount
        && (cursor.lnum > shl.lnum || cursor.col >= shl.rm.startpos[0].col)
        && (cursor.lnum < shl.lnum + linecount || cursor.col < shl.rm.endpos[0].col);
}

/// Whether a character just past the end of the line should be
/// highlighted (`get_prevcol_hl_flag`).
///
/// True when the match started exactly at the end of the line, or
/// continues into the next line (so the match includes the line
/// break).
///
/// Only the always-taken "no matches exist" fast path of the
/// `w_match_head` loop is translated (see this module's own doc
/// comment): nothing currently translated can populate that list, so
/// the loop cannot find anything and the answer rests entirely on
/// `search_hl`. A debug assertion records that expectation rather
/// than letting a future real list be silently ignored.
///
/// # Safety
/// `wp` must be a valid reference to a live `WinT`.
#[must_use]
pub unsafe fn get_prevcol_hl_flag(
    wp: &WinT,
    search_hl: &crate::buffer_defs::MatchT,
    curcol: crate::pos_defs::ColnrT,
) -> bool {
    let mut prevcol = curcol;

    // We're not really at that column when skipping some text.
    let skipped = if wp.w_onebuf_opt.wo_wrap != 0 { wp.w_skipcol } else { wp.w_leftcol };
    if skipped > prevcol {
        prevcol += 1;
    }

    debug_assert!(
        wp.w_match_head.is_null(),
        "get_prevcol_hl_flag: real matchitem_T support not yet translated"
    );

    !search_hl.is_addpos
        && (prevcol == search_hl.startcol
            || (prevcol > search_hl.startcol && search_hl.endcol == crate::pos_defs::MAXCOL))
}

/// Clear all matches for window `wp` (`clear_matches`).
///
/// Only the always-taken "no matches exist" fast path is translated
/// (see this module's own doc comment) - the original's own
/// `redraw_later(wp, UPD_SOME_VALID)` call at the end is omitted
/// (a pure redraw-scheduling side effect, matching this crate's
/// established policy), leaving this function's ENTIRE currently-
/// reachable behavior a real, faithful no-op.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn clear_matches(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    debug_assert!(unsafe { &*wp }.w_match_head.is_null(), "clear_matches: real matchitem_T support not yet translated");
}

/// `"clearmatches([{win}])"` function (`f_clearmatches`).
///
/// # Safety
/// Forwarded from [`get_optional_window`]/[`clear_matches`]'s own
/// safety docs.
pub unsafe fn f_clearmatches(argvars: &[TypvalT], _rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { get_optional_window(argvars, 0) };
    if !win.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { clear_matches(win) };
    }
}

/// `"getmatches([{win}])"` function (`f_getmatches`).
///
/// Always returns an empty `List` today: the original's own loop over
/// `wp.w_match_head` is gated on it being non-`NULL`, which - since
/// nothing in this crate can currently populate it - is always the
/// case, matching the real, correct output for any session where
/// `matchadd()`/`matchaddpos()` have never been called (the
/// overwhelmingly common state today).
///
/// # Safety
/// Forwarded from [`get_optional_window`]'s own safety doc.
pub unsafe fn f_getmatches(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: `rettv` is freshly default-initialized by the caller.
    let l = unsafe {
        crate::eval::typval::tv_list_alloc_ret(rettv, crate::eval::typval_defs::ListLenSpecials::MayKnow as isize)
    };
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { get_optional_window(argvars, 0) };
    if win.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    debug_assert!(unsafe { &*win }.w_match_head.is_null(), "f_getmatches: real matchitem_T support not yet translated");
    let _ = l;
}

/// Find match `id` for window `wp` (`get_match`). Always returns
/// `null` today: the original's own loop walks `wp.w_match_head`
/// looking for a `mit_id == id` entry, but since nothing in this
/// crate can currently populate that list, its own loop condition
/// (`cur != NULL && ...`) is false on the very first check - see this
/// module's own doc comment.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
#[must_use]
pub unsafe fn get_match(wp: *mut WinT, _id: i32) -> *mut crate::types_defs::MatchitemT {
    // SAFETY: forwarded from this function's own safety doc.
    debug_assert!(unsafe { &*wp }.w_match_head.is_null(), "get_match: real matchitem_T support not yet translated");
    std::ptr::null_mut()
}

/// `"matcharg({nr})"` function (`f_matcharg`) - the highlight group
/// name and pattern for match `{nr}` (`1`-`3`, for `:match`/`:2match`/
/// `:3match`), as a 2-element `List`.
///
/// Since [`get_match`] always returns `null` today, this is ALWAYS a
/// `[v:null, v:null]`-equivalent (2 null strings) List for `{nr}` in
/// `1..=3` - the original's own `m != NULL` branch (needing
/// `syn_id2name`, the highlight-group registry, not translated) is
/// never reached. An out-of-range `{nr}` gets an empty List, matching
/// the original's own `tv_list_alloc_ret(rettv, 0)` for that case.
///
/// # Safety
/// Touches `crate::globals::GLOBALS.curwin`. Forwarded from
/// [`get_match`]'s own safety doc.
pub unsafe fn f_matcharg(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let id = crate::eval::typval::tv_get_number(&argvars[0]);
    let in_range = (1..=3).contains(&id);
    // SAFETY: `rettv` is freshly default-initialized by the caller.
    let l = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, if in_range { 2 } else { 0 }) };
    if in_range {
        // SAFETY: forwarded from this function's own safety doc.
        let win = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        // SAFETY: forwarded from this function's own safety doc.
        let m = unsafe { get_match(win, id as i32) };
        debug_assert!(m.is_null(), "f_matcharg: real matchitem_T support not yet translated");
        // SAFETY: `l` was just freshly allocated above.
        unsafe {
            crate::eval::typval::tv_list_append_string(l, None);
            crate::eval::typval::tv_list_append_string(l, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::typval_defs::TypvalValue;

    // --- get_prevcol_hl_flag ---

    /// A search-highlight match with the given start and end columns.
    fn prevcol_hl(
        startcol: crate::pos_defs::ColnrT,
        endcol: crate::pos_defs::ColnrT,
    ) -> crate::buffer_defs::MatchT {
        crate::buffer_defs::MatchT { startcol, endcol, ..Default::default() }
    }

    /// True exactly when the column reaches the match's start column.
    #[test]
    fn get_prevcol_hl_flag_is_true_at_the_match_start_column() {
        let wp = WinT::default();
        let shl = prevcol_hl(5, 9);

        assert!(!unsafe { get_prevcol_hl_flag(&wp, &shl, 4) }, "before the start");
        assert!(unsafe { get_prevcol_hl_flag(&wp, &shl, 5) }, "at the start");
        // Past the start only counts when the match runs to MAXCOL.
        assert!(!unsafe { get_prevcol_hl_flag(&wp, &shl, 6) }, "past, but not MAXCOL");
    }

    /// A match ending at MAXCOL continues into the next line, so any
    /// column past its start highlights.
    #[test]
    fn get_prevcol_hl_flag_is_true_past_the_start_when_the_match_runs_to_maxcol() {
        let wp = WinT::default();
        let shl = prevcol_hl(5, crate::pos_defs::MAXCOL);
        assert!(unsafe { get_prevcol_hl_flag(&wp, &shl, 6) });
        assert!(!unsafe { get_prevcol_hl_flag(&wp, &shl, 4) }, "still not before");
    }

    /// A position added by `matchaddpos()` never gets this treatment.
    #[test]
    fn get_prevcol_hl_flag_is_false_for_an_addpos_match() {
        let wp = WinT::default();
        let mut shl = prevcol_hl(5, 9);
        shl.is_addpos = true;
        assert!(!unsafe { get_prevcol_hl_flag(&wp, &shl, 5) });
    }

    /// With `'wrap'` set the skipped text is `w_skipcol`; the column
    /// is bumped by one when it lies before that, which can bring it
    /// up to the match start.
    #[test]
    fn get_prevcol_hl_flag_uses_skipcol_when_wrapping() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_wrap = 1;
        wp.w_skipcol = 5;
        wp.w_leftcol = 0; // would NOT trigger the bump
        let shl = prevcol_hl(5, 9);

        // curcol 4 is bumped to 5 by skipcol, reaching the start.
        assert!(unsafe { get_prevcol_hl_flag(&wp, &shl, 4) });
    }

    /// Without `'wrap'` it is `w_leftcol` instead, so a `w_skipcol`
    /// that would have bumped is ignored.
    #[test]
    fn get_prevcol_hl_flag_uses_leftcol_when_not_wrapping() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_wrap = 0;
        wp.w_skipcol = 5; // ignored in this mode
        wp.w_leftcol = 0;
        let shl = prevcol_hl(5, 9);

        assert!(
            !unsafe { get_prevcol_hl_flag(&wp, &shl, 4) },
            "skipcol must not apply when 'wrap' is off"
        );

        // With leftcol set the bump happens again.
        wp.w_leftcol = 5;
        assert!(unsafe { get_prevcol_hl_flag(&wp, &shl, 4) });
    }

    // --- check_cur_search_hl ---

    /// A match starting at `lnum`, spanning `linecount` further lines,
    /// from `start_col` to `end_col` (end exclusive).
    fn hl_match(
        lnum: crate::pos_defs::LinenrT,
        linecount: crate::pos_defs::LinenrT,
        start_col: crate::pos_defs::ColnrT,
        end_col: crate::pos_defs::ColnrT,
    ) -> crate::buffer_defs::MatchT {
        let mut shl = crate::buffer_defs::MatchT { lnum, ..Default::default() };
        // Sub-match 0 positions are relative to the match's own first
        // line, so the start line is always 0.
        shl.rm.startpos[0] = crate::pos_defs::LposT { lnum: 0, col: start_col };
        shl.rm.endpos[0] = crate::pos_defs::LposT { lnum: linecount, col: end_col };
        shl
    }

    fn win_at(
        lnum: crate::pos_defs::LinenrT,
        col: crate::pos_defs::ColnrT,
    ) -> WinT {
        let mut wp = WinT::default();
        wp.w_cursor.lnum = lnum;
        wp.w_cursor.col = col;
        wp
    }

    fn has_cursor(
        cursor_lnum: crate::pos_defs::LinenrT,
        cursor_col: crate::pos_defs::ColnrT,
        shl: &crate::buffer_defs::MatchT,
    ) -> bool {
        let wp = win_at(cursor_lnum, cursor_col);
        let mut shl = *shl;
        check_cur_search_hl(&wp, &mut shl);
        shl.has_cursor
    }

    /// On a single-line match the start column is INCLUSIVE and the
    /// end column EXCLUSIVE.
    #[test]
    fn check_cur_search_hl_bounds_a_single_line_match_by_column() {
        let shl = hl_match(10, 0, 4, 8);

        assert!(!has_cursor(10, 3, &shl), "before the start column");
        assert!(has_cursor(10, 4, &shl), "start column is inclusive");
        assert!(has_cursor(10, 7, &shl), "last column inside");
        assert!(!has_cursor(10, 8, &shl), "end column is exclusive");
    }

    #[test]
    fn check_cur_search_hl_rejects_lines_outside_the_match() {
        let shl = hl_match(10, 2, 4, 8);
        assert!(!has_cursor(9, 6, &shl), "line before the match");
        assert!(!has_cursor(13, 6, &shl), "line after the match");
    }

    /// On the FIRST line only the start column applies, so a column
    /// past the end column is still inside a multi-line match.
    #[test]
    fn check_cur_search_hl_applies_only_the_start_column_on_the_first_line() {
        let shl = hl_match(10, 2, 4, 8);
        assert!(!has_cursor(10, 3, &shl), "before the start column");
        assert!(has_cursor(10, 4, &shl), "at the start column");
        assert!(has_cursor(10, 99, &shl), "end column must not apply here");
    }

    /// On the LAST line only the end column applies, so a column
    /// before the start column is still inside.
    #[test]
    fn check_cur_search_hl_applies_only_the_end_column_on_the_last_line() {
        let shl = hl_match(10, 2, 4, 8);
        assert!(has_cursor(12, 0, &shl), "start column must not apply here");
        assert!(has_cursor(12, 7, &shl), "last column inside");
        assert!(!has_cursor(12, 8, &shl), "end column is exclusive");
    }

    /// On a line strictly between the first and last, the whole line
    /// is inside regardless of column.
    #[test]
    fn check_cur_search_hl_ignores_the_column_on_a_middle_line() {
        let shl = hl_match(10, 2, 4, 8);
        assert!(has_cursor(11, 0, &shl));
        assert!(has_cursor(11, 999, &shl));
    }

    /// The flag is cleared as well as set, so a stale `true` from a
    /// previous match does not survive.
    #[test]
    fn check_cur_search_hl_clears_a_stale_flag() {
        let mut shl = hl_match(10, 0, 4, 8);
        shl.has_cursor = true;

        let wp = win_at(20, 0); // nowhere near the match
        check_cur_search_hl(&wp, &mut shl);
        assert!(!shl.has_cursor);
    }

    fn focusable_win(handle: crate::types_defs::HandleT) -> WinT {
        WinT {
            handle,
            w_config: crate::buffer_defs::WinConfig { focusable: true, hide: false, ..Default::default() },
            ..Default::default()
        }
    }

    fn num(n: i64) -> TypvalT {
        TypvalT { value: TypvalValue::Number(n), ..Default::default() }
    }

    struct WinGlobalsGuard {
        prev_firstwin: *mut WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_curwin: *mut WinT,
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl WinGlobalsGuard {
        fn set(win: *mut WinT, tp: *mut crate::buffer_defs::TabpageT) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = WinGlobalsGuard {
                prev_firstwin: globals.firstwin,
                prev_curtab: globals.curtab,
                prev_curwin: globals.curwin,
                prev_first_tabpage: globals.first_tabpage,
                _lock,
            };
            globals.firstwin = win;
            globals.curtab = tp;
            globals.curwin = win;
            globals.first_tabpage = tp;
            guard
        }
    }

    impl Drop for WinGlobalsGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = self.prev_firstwin;
            globals.curtab = self.prev_curtab;
            globals.curwin = self.prev_curwin;
            globals.first_tabpage = self.prev_first_tabpage;
        }
    }

    #[test]
    fn get_optional_window_no_arg_returns_curwin() {
        let mut win = focusable_win(7);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, &mut tp);

        assert_eq!(unsafe { get_optional_window(&[], 0) }, win_ptr);
    }

    #[test]
    fn get_optional_window_explicit_arg_resolves_by_number() {
        let mut win = focusable_win(7);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let _guard = WinGlobalsGuard::set(win_ptr, &mut tp);

        assert_eq!(unsafe { get_optional_window(&[num(1)], 0) }, win_ptr);
    }

    #[test]
    fn get_optional_window_unresolvable_returns_null() {
        let mut win = focusable_win(7);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win as *mut WinT, &mut tp);

        assert!(unsafe { get_optional_window(&[num(999)], 0) }.is_null());
    }

    #[test]
    fn clear_matches_is_a_no_op_when_no_matches_exist() {
        let mut win = focusable_win(7);
        assert!(win.w_match_head.is_null());
        unsafe { clear_matches(&mut win as *mut WinT) };
        assert!(win.w_match_head.is_null());
    }

    #[test]
    fn f_clearmatches_no_args_targets_curwin() {
        let mut win = focusable_win(7);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win as *mut WinT, &mut tp);

        let mut rettv = TypvalT::default();
        unsafe { f_clearmatches(&[], &mut rettv) };
    }

    #[test]
    fn f_getmatches_returns_an_empty_list() {
        let mut win = focusable_win(7);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win as *mut WinT, &mut tp);

        let mut rettv = TypvalT::default();
        unsafe { f_getmatches(&[], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 0);
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn f_getmatches_unresolvable_window_still_returns_an_empty_list() {
        let mut win = focusable_win(7);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win as *mut WinT, &mut tp);

        let mut rettv = TypvalT::default();
        unsafe { f_getmatches(&[num(999)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 0);
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn get_match_is_always_null_when_no_matches_exist() {
        let mut win = focusable_win(7);
        assert!(win.w_match_head.is_null());
        assert!(unsafe { get_match(&mut win as *mut WinT, 1) }.is_null());
    }

    #[test]
    fn f_matcharg_in_range_returns_a_2_element_list_of_null_strings() {
        let mut win = focusable_win(7);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win as *mut WinT, &mut tp);

        for id in 1..=3 {
            let mut rettv = TypvalT::default();
            unsafe { f_matcharg(&[num(id)], &mut rettv) };
            let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
            unsafe {
                assert_eq!((*l).lv_len, 2);
                let first = crate::eval::typval::tv_list_first(l);
                assert_eq!((*first).li_tv.value, TypvalValue::String(None));
                let second = (*first).li_next;
                assert_eq!((*second).li_tv.value, TypvalValue::String(None));
                crate::eval::typval::tv_list_unref(l);
            }
        }
    }

    #[test]
    fn f_matcharg_out_of_range_returns_an_empty_list() {
        let mut win = focusable_win(7);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WinGlobalsGuard::set(&mut win as *mut WinT, &mut tp);

        for id in [0, 4, -1] {
            let mut rettv = TypvalT::default();
            unsafe { f_matcharg(&[num(id)], &mut rettv) };
            let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
            unsafe {
                assert_eq!((*l).lv_len, 0);
                crate::eval::typval::tv_list_unref(l);
            }
        }
    }
}
