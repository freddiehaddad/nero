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
