//! Translated from `src/nvim/match.c` (tractable core only).
//!
//! `match.c` implements the `:match`/`matchadd()`/`matchaddpos()`
//! highlighting-match subsystem, keyed on `WinT.w_match_head` (a
//! linked list of `matchitem_T` entries).
//!
//! `matchitem_T` is now a REAL struct
//! ([`crate::buffer_defs::MatchitemT`]): its former blocker,
//! `regmmatch_T`, is itself real (see `regexp_defs.rs`), holding the
//! still-opaque compiled program only as a pointer nothing here
//! dereferences. So a match entry's fields can now be read and built,
//! and [`get_match`] is a real walk of the list rather than a fast
//! path.
//!
//! What remains missing is not the TYPE but the machinery that
//! populates the list: nothing currently translated adds an entry, so
//! `w_match_head` is still empty in practice. Functions whose real
//! body needs more than the walk itself therefore still take their
//! "no matches exist" path, each guarded by a `debug_assert!` on
//! `w_match_head` being empty so a future real list cannot be
//! silently ignored: `clear_matches`/`f_clearmatches`/`f_getmatches`
//! (needs the item-to-Dict conversion), `f_matcharg` (its reporting
//! branch needs `syn_id2name`, the highlight-group registry), and
//! `get_prevcol_hl_flag`/`get_search_match_hl` (their loops over the
//! list).
//!
//! Also translated: `get_optional_window` (`eval/funcs.c`), and the
//! search-highlight helpers `check_cur_search_hl`/
//! `get_prevcol_hl_flag`/`get_search_match_hl`, unblocked by
//! `match_T`.
//!
//! Deferred: `matchadd()`/`matchaddpos()`/`matchdelete()`,
//! `getmatches()`'s own item-conversion loop body, and
//! `:match`/`:2match`/`:3match` - the list-BUILDING side.

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

/// Whether `prevcol` sits where match `hl` would highlight a
/// character just past the end of the line.
///
/// Shared by [`get_prevcol_hl_flag`] between the search highlight and
/// each entry in the window's match list, which the original spells
/// out twice with identical conditions.
fn prevcol_starts_match(hl: &crate::buffer_defs::MatchT, prevcol: crate::pos_defs::ColnrT) -> bool {
    !hl.is_addpos
        && (prevcol == hl.startcol
            || (prevcol > hl.startcol && hl.endcol == crate::pos_defs::MAXCOL))
}

/// Whether a character just past the end of the line should be
/// highlighted (`get_prevcol_hl_flag`).
///
/// True when a match started exactly at the end of the line, or
/// continues into the next line (so the match includes the line
/// break). The search highlight is checked first, then every entry in
/// the window's own match list.
///
/// # Safety
/// `wp` must be a valid reference to a live `WinT`, and its
/// `w_match_head` chain must consist of live `MatchitemT`s.
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

    if prevcol_starts_match(search_hl, prevcol) {
        return true;
    }

    let mut cur = wp.w_match_head;
    while !cur.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let item = unsafe { &*cur };
        if prevcol_starts_match(&item.mit_hl, prevcol) {
            return true;
        }
        cur = item.mit_next;
    }
    false
}

/// Priority of the `'hlsearch'` highlight, against which a match
/// list entry's own `mit_priority` is ordered (`SEARCH_HL_PRIORITY`).
pub const SEARCH_HL_PRIORITY: i32 = 0;

/// The highlight attribute for a match starting just before `col`, if
/// any (`get_search_match_hl`).
///
/// Returns `None` when nothing starts there, leaving the caller's
/// current attribute alone - the original signals this by simply not
/// writing to its `char_attr` out-parameter, which becomes a returned
/// `Option` here, matching this crate's convention.
///
/// The search highlight is not simply checked first: it is processed
/// at its PRIORITY POSITION among the window's match list, which the
/// original expresses with a single loop that visits the list in order
/// and slots `search_hl` in before the first entry whose
/// `mit_priority` exceeds [`SEARCH_HL_PRIORITY`] (or at the end).
/// Later-visited entries overwrite earlier ones, so the LAST
/// applicable attribute wins.
///
/// One asymmetry is the original's: an `is_addpos` entry from the list
/// is skipped, but `search_hl` itself is used even when its own
/// `is_addpos` is set, because the guard is
/// `shl == search_hl || !shl->is_addpos`.
///
/// # Safety
/// `wp` must be a valid reference to a live `WinT`, and its
/// `w_match_head` chain must consist of live `MatchitemT`s.
#[must_use]
pub unsafe fn get_search_match_hl(
    wp: &WinT,
    search_hl: &crate::buffer_defs::MatchT,
    col: crate::pos_defs::ColnrT,
) -> Option<i32> {
    let mut result = None;
    let mut cur = wp.w_match_head;
    let mut shl_done = false;

    while !cur.is_null() || !shl_done {
        // SAFETY: forwarded from this function's own safety doc.
        let take_search_hl = !shl_done
            && (cur.is_null() || unsafe { &*cur }.mit_priority > SEARCH_HL_PRIORITY);

        let hl: &crate::buffer_defs::MatchT = if take_search_hl {
            shl_done = true;
            search_hl
        } else {
            // SAFETY: forwarded from this function's own safety doc;
            // `cur` is non-null whenever this branch is taken.
            unsafe { &(*cur).mit_hl }
        };

        if col - 1 == hl.startcol && (take_search_hl || !hl.is_addpos) {
            result = Some(hl.attr);
        }

        if !take_search_hl && !cur.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            cur = unsafe { (*cur).mit_next };
        }
    }
    result
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

/// Find match `id` for window `wp` (`get_match`), or `null` when the
/// window has no match with that ID.
///
/// A real walk of `wp.w_match_head` now that
/// [`crate::buffer_defs::MatchitemT`] is a real struct rather than an
/// opaque placeholder; the previous always-null fast path is retired.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`, and its
/// `w_match_head` chain must consist of live `MatchitemT`s.
#[must_use]
pub unsafe fn get_match(wp: *mut WinT, id: i32) -> *mut crate::buffer_defs::MatchitemT {
    // SAFETY: forwarded from this function's own safety doc.
    let mut cur = unsafe { &*wp }.w_match_head;
    while !cur.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let item = unsafe { &*cur };
        if item.mit_id == id {
            break;
        }
        cur = item.mit_next;
    }
    cur
}

/// `"matcharg({nr})"` function (`f_matcharg`) - the highlight group
/// name and pattern for match `{nr}` (`1`-`3`, for `:match`/`:2match`/
/// `:3match`), as a 2-element `List`.
///
/// Always a `[v:null, v:null]`-equivalent (2 null strings) List for
/// `{nr}` in `1..=3`. The original's own `m != NULL` branch needs
/// `syn_id2name` (the highlight-group registry, not translated), so it
/// cannot be filled in yet - and nothing translated populates
/// `w_match_head`, so [`get_match`] finds nothing to report anyway.
/// An out-of-range `{nr}` gets an empty List, matching the original's
/// own `tv_list_alloc_ret(rettv, 0)` for that case.
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
        // The real reporting branch needs `syn_id2name`; assert on the
        // condition that actually holds today - an empty match list -
        // rather than on `get_match`'s result, which is now a real
        // walk and no longer null by construction.
        debug_assert!(
            unsafe { &*win }.w_match_head.is_null(),
            "f_matcharg: reporting a real match needs syn_id2name"
        );
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

    // --- get_match ---

    /// A window owning a real match list, all allocations held as raw
    /// pointers so writes through the walked chain cannot invalidate
    /// a live `Box` tag.
    struct MatchListFixture {
        win: *mut WinT,
        items: Vec<*mut crate::buffer_defs::MatchitemT>,
    }

    impl MatchListFixture {
        /// One entry per ID, linked through `mit_next` in order.
        fn new(ids: &[i32]) -> Self {
            let items: Vec<*mut crate::buffer_defs::MatchitemT> = ids
                .iter()
                .map(|&mit_id| {
                    Box::into_raw(Box::new(crate::buffer_defs::MatchitemT {
                        mit_id,
                        ..Default::default()
                    }))
                })
                .collect();
            for i in 0..items.len().saturating_sub(1) {
                let cur = items[i];
                unsafe { &mut *cur }.mit_next = items[i + 1];
            }

            let win = Box::into_raw(Box::new(WinT {
                w_match_head: items.first().copied().unwrap_or(std::ptr::null_mut()),
                ..Default::default()
            }));
            Self { win, items }
        }
    }

    impl Drop for MatchListFixture {
        fn drop(&mut self) {
            unsafe {
                drop(Box::from_raw(self.win));
                for &it in &self.items {
                    drop(Box::from_raw(it));
                }
            }
        }
    }

    #[test]
    fn get_match_returns_null_for_an_empty_list() {
        let fx = MatchListFixture::new(&[]);
        assert!(unsafe { get_match(fx.win, 1) }.is_null());
    }

    #[test]
    fn get_match_returns_null_when_no_entry_has_that_id() {
        let fx = MatchListFixture::new(&[4, 5, 6]);
        assert!(unsafe { get_match(fx.win, 7) }.is_null());
    }

    /// Finds the entry with the matching ID wherever it sits in the
    /// chain - first, middle or last.
    #[test]
    fn get_match_finds_the_entry_anywhere_in_the_chain() {
        let fx = MatchListFixture::new(&[4, 5, 6]);
        assert_eq!(unsafe { get_match(fx.win, 4) }, fx.items[0], "first");
        assert_eq!(unsafe { get_match(fx.win, 5) }, fx.items[1], "middle");
        assert_eq!(unsafe { get_match(fx.win, 6) }, fx.items[2], "last");
    }

    /// With duplicate IDs the FIRST match wins, since the walk stops
    /// as soon as it finds one.
    #[test]
    fn get_match_returns_the_first_of_two_duplicate_ids() {
        let fx = MatchListFixture::new(&[9, 9]);
        assert_eq!(unsafe { get_match(fx.win, 9) }, fx.items[0]);
    }

    // --- get_search_match_hl ---

    /// The attribute applies to the column just AFTER the match's
    /// start column, since the original compares `col - 1`.
    #[test]
    fn get_search_match_hl_returns_the_attribute_one_past_the_start_column() {
        let wp = WinT::default();
        let shl = crate::buffer_defs::MatchT {
            startcol: 5,
            attr: 42,
            ..Default::default()
        };

        assert_eq!(unsafe { get_search_match_hl(&wp, &shl, 6) }, Some(42));
        assert_eq!(unsafe { get_search_match_hl(&wp, &shl, 5) }, None, "off by one");
        assert_eq!(unsafe { get_search_match_hl(&wp, &shl, 7) }, None);
    }

    /// On this path `shl` IS `search_hl`, so the original's
    /// `shl == search_hl || !shl->is_addpos` guard is unconditionally
    /// true and an addpos match still yields its attribute - unlike
    /// `get_prevcol_hl_flag`, where addpos does suppress.
    #[test]
    fn get_search_match_hl_is_not_suppressed_by_addpos() {
        let wp = WinT::default();
        let shl = crate::buffer_defs::MatchT {
            startcol: 5,
            attr: 7,
            is_addpos: true,
            ..Default::default()
        };
        assert_eq!(unsafe { get_search_match_hl(&wp, &shl, 6) }, Some(7));
    }

    /// A zero attribute is still a real answer, distinct from "no
    /// match starts here".
    #[test]
    fn get_search_match_hl_distinguishes_a_zero_attribute_from_no_match() {
        let wp = WinT::default();
        let shl = crate::buffer_defs::MatchT { startcol: 0, attr: 0, ..Default::default() };
        assert_eq!(unsafe { get_search_match_hl(&wp, &shl, 1) }, Some(0));
        assert_eq!(unsafe { get_search_match_hl(&wp, &shl, 2) }, None);
    }

    /// A list entry is visited too, not just the search highlight.
    #[test]
    fn get_search_match_hl_finds_an_entry_in_the_window_list() {
        let fx = MatchListFixture::new(&[1]);
        let search_hl = crate::buffer_defs::MatchT {
            startcol: 100,
            attr: 1,
            ..Default::default()
        };

        let first = fx.items[0];
        unsafe { &mut *first }.mit_hl.startcol = 5;
        unsafe { &mut *first }.mit_hl.attr = 77;

        let wp = unsafe { &*fx.win };
        assert_eq!(unsafe { get_search_match_hl(wp, &search_hl, 6) }, Some(77));
    }

    /// An `is_addpos` entry from the LIST is skipped - unlike
    /// `search_hl` itself, which is used even when its own
    /// `is_addpos` is set.
    #[test]
    fn get_search_match_hl_skips_an_addpos_entry_from_the_list() {
        let fx = MatchListFixture::new(&[1]);
        let search_hl = crate::buffer_defs::MatchT {
            startcol: 100,
            attr: 1,
            ..Default::default()
        };

        let first = fx.items[0];
        unsafe { &mut *first }.mit_hl.startcol = 5;
        unsafe { &mut *first }.mit_hl.attr = 77;
        unsafe { &mut *first }.mit_hl.is_addpos = true;

        let wp = unsafe { &*fx.win };
        assert_eq!(unsafe { get_search_match_hl(wp, &search_hl, 6) }, None);
    }

    /// The search highlight is processed at its PRIORITY position,
    /// not simply first or last, and the last applicable attribute
    /// wins. A low-priority entry is visited BEFORE `search_hl`, so
    /// `search_hl` overrides it; a high-priority entry is visited
    /// AFTER, so it overrides `search_hl`.
    #[test]
    fn get_search_match_hl_orders_the_search_highlight_by_priority() {
        let search_hl = crate::buffer_defs::MatchT {
            startcol: 5,
            attr: 1,
            ..Default::default()
        };

        // Priority 0 is NOT greater than SEARCH_HL_PRIORITY, so this
        // entry is visited first and search_hl wins.
        let low = MatchListFixture::new(&[1]);
        let low_item = low.items[0];
        unsafe { &mut *low_item }.mit_priority = SEARCH_HL_PRIORITY;
        unsafe { &mut *low_item }.mit_hl.startcol = 5;
        unsafe { &mut *low_item }.mit_hl.attr = 77;
        assert_eq!(
            unsafe { get_search_match_hl(&*low.win, &search_hl, 6) },
            Some(1),
            "search_hl is visited last and wins"
        );

        // Priority 1 IS greater, so search_hl is slotted in first and
        // the entry overrides it.
        let high = MatchListFixture::new(&[1]);
        let high_item = high.items[0];
        unsafe { &mut *high_item }.mit_priority = SEARCH_HL_PRIORITY + 1;
        unsafe { &mut *high_item }.mit_hl.startcol = 5;
        unsafe { &mut *high_item }.mit_hl.attr = 77;
        assert_eq!(
            unsafe { get_search_match_hl(&*high.win, &search_hl, 6) },
            Some(77),
            "the higher-priority entry is visited last and wins"
        );
    }

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

    /// An entry in the window's own match list also triggers the
    /// flag, not just the search highlight - this is the loop the
    /// previous fast path skipped entirely.
    #[test]
    fn get_prevcol_hl_flag_finds_a_match_in_the_window_list() {
        let fx = MatchListFixture::new(&[1, 2]);
        // The search highlight itself must not match.
        let search_hl = prevcol_hl(100, 200);

        // Give the SECOND list entry the interesting columns, so a
        // walk that only looked at the head would miss it.
        let second = fx.items[1];
        unsafe { &mut *second }.mit_hl.startcol = 5;
        unsafe { &mut *second }.mit_hl.endcol = 9;

        let wp = unsafe { &*fx.win };
        assert!(unsafe { get_prevcol_hl_flag(wp, &search_hl, 5) });
        assert!(!unsafe { get_prevcol_hl_flag(wp, &search_hl, 4) });
    }

    /// An addpos entry in the list is skipped, exactly as an addpos
    /// search highlight is.
    #[test]
    fn get_prevcol_hl_flag_skips_an_addpos_entry_in_the_list() {
        let fx = MatchListFixture::new(&[1]);
        let search_hl = prevcol_hl(100, 200);

        let first = fx.items[0];
        unsafe { &mut *first }.mit_hl.startcol = 5;
        unsafe { &mut *first }.mit_hl.endcol = 9;
        unsafe { &mut *first }.mit_hl.is_addpos = true;

        let wp = unsafe { &*fx.win };
        assert!(!unsafe { get_prevcol_hl_flag(wp, &search_hl, 5) });
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
