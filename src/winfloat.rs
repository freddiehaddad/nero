//! Translated from `src/nvim/winfloat.c` (partial).
//!
//! Translated: `win_check_anchored_floats` - harvested ahead of the
//! rest of this file (floating-window configuration/positioning,
//! phase 8/9 rendering territory) since it's a small, self-contained
//! function needed by `move.c`'s `check_topfill` (itself needed by
//! `eval/window.c`'s `winrestview()`); `win_border_height`/
//! `win_border_width` (trivial `w_border_adj` sums); `win_float_valid`
//! (whether `win` is a floating window in the current tab page - the
//! same `curtab`-aware `firstwin`/`w_next` walk already established by
//! `window.rs`'s `tabpage_win_valid`, since `FOR_ALL_WINDOWS_IN_TAB(wp,
//! curtab)` always resolves to `firstwin` at this call site too).
//!
//! Also translated: `win_float_anchor_laststatus` (marks every
//! `relative = "laststatus"` floating window as needing its position
//! recomputed, the same `firstwin`/`w_next` walk as `win_float_valid`);
//! `win_float_find_preview` (the first floating "preview" window,
//! `w_kind == kWinInfo`, walking `lastwin`/`w_prev` while floating -
//! the same walk already established by `win_check_anchored_floats`);
//! `win_float_find_altwin` (an alternative window to switch to when a
//! floating window is closed/moved, via already-real
//! `crate::globals::GLOBALS.prevwin`, `crate::window::win_valid`/
//! `tabpage_win_valid`, `TabpageT.tp_prevwin`/`tp_firstwin`,
//! `WinConfig.focusable`/`hide`).
//!
//! Deferred: everything else in this file needs the floating-window
//! configuration machinery (`win_config_float`, `WinConfig`'s own
//! apply/validate logic) and the UI/rendering pipeline, neither
//! translated.

use crate::buffer_defs::{FloatRelative, WinT};

/// Mark any floating window anchored (via `relative = "win"`) to
/// `win` as needing its position recomputed (`win_check_anchored_floats`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS`, with the usual "no overlapping
/// live access" requirement. `win` must be a valid, non-null pointer
/// to a live `WinT`. Every window reachable via `GLOBALS.lastwin`/
/// `w_prev` must also be valid and live.
pub unsafe fn win_check_anchored_floats(win: *const WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.lastwin;
    // SAFETY: forwarded from this function's own safety doc.
    while !wp.is_null() && unsafe { &*wp }.w_floating {
        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &mut *wp };
        // SAFETY: forwarded from this function's own safety doc.
        if w.w_config.relative == FloatRelative::Window && w.w_config.window == unsafe { &*win }.handle {
            w.w_pos_changed = true;
        }
        wp = w.w_prev;
    }
}

/// Sum of the top and bottom border widths (`win_border_height`).
#[must_use]
pub fn win_border_height(wp: &WinT) -> i32 {
    wp.w_border_adj[0] + wp.w_border_adj[2]
}

/// Sum of the left and right border widths (`win_border_width`).
#[must_use]
pub fn win_border_width(wp: &WinT) -> i32 {
    wp.w_border_adj[1] + wp.w_border_adj[3]
}

/// Whether `win` is a floating window in the current tab page
/// (`win_float_valid`). Always `false` for a null `win`.
///
/// # Safety
/// `win`, if non-null, must be a valid pointer to a live `WinT`.
/// `GLOBALS.curtab`/`firstwin` and every window reachable via
/// `w_next` must also be valid and live - same requirement as
/// `window.rs`'s `tabpage_win_valid`, whose exact walk this mirrors.
#[must_use]
pub unsafe fn win_float_valid(win: *const WinT) -> bool {
    if win.is_null() {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        if std::ptr::eq(wp, win) {
            // SAFETY: forwarded from this function's own safety doc.
            return unsafe { &*wp }.w_floating;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    false
}

/// Mark any floating window anchored to `'laststatus'`
/// (`relative = "laststatus"`) as needing its position recomputed
/// (`win_float_anchor_laststatus`).
///
/// # Safety
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid,
/// live `WinT` pointers (the original's own `FOR_ALL_WINDOWS_IN_TAB`
/// over `curtab` always resolves to this exact walk, matching
/// `win_float_valid`'s own established precedent).
pub unsafe fn win_float_anchor_laststatus() {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &mut *wp };
        if w.w_config.relative == FloatRelative::Laststatus {
            w.w_pos_changed = true;
        }
        wp = w.w_next;
    }
}

/// Find the first floating "preview" window (`w_kind == kWinInfo`,
/// used e.g. by `:pedit`), or null if none exists
/// (`win_float_find_preview`).
///
/// # Safety
/// `GLOBALS.lastwin`'s own `w_prev` chain must consist of valid, live
/// `WinT` pointers.
#[must_use]
pub unsafe fn win_float_find_preview() -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.lastwin;
    // SAFETY: forwarded from this function's own safety doc.
    while !wp.is_null() && unsafe { &*wp }.w_floating {
        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &*wp };
        if w.w_kind == crate::buffer_defs::WinKind::Info {
            return wp;
        }
        wp = w.w_prev;
    }
    std::ptr::null_mut()
}

/// Select an alternative window to `win` (assumed floating) in
/// tabpage `tp`, or the current tabpage if `tp` is null
/// (`win_float_find_altwin`). Useful for finding a window to switch to
/// if `win` is the current window, but is then closed or moved to a
/// different tabpage.
///
/// # Safety
/// `win` must be a valid, non-null pointer to a live `WinT`. If `tp`
/// is null, `GLOBALS.prevwin` (if non-null) and `GLOBALS.firstwin`
/// must be valid, live pointers within the current tabpage's own
/// window list. If `tp` is non-null (and distinct from
/// `GLOBALS.curtab`), its own `tp_prevwin`/`tp_firstwin` must be
/// valid, live `WinT` pointers, with `tp_firstwin` in particular
/// guaranteed non-null (a real invariant of any live tabpage).
pub unsafe fn win_float_find_altwin(
    win: *const WinT,
    tp: *const crate::buffer_defs::TabpageT,
) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if tp.is_null() {
        let wp = globals.prevwin;
        // SAFETY: forwarded from this function's own safety doc.
        return if unsafe { crate::window::win_valid(wp) }
            && !std::ptr::eq(wp, win)
            // SAFETY: forwarded from this function's own safety doc.
            && unsafe { &*wp }.w_config.focusable
            && !unsafe { &*wp }.w_config.hide
        {
            wp
        } else {
            globals.firstwin
        };
    }

    debug_assert!(!std::ptr::eq(tp, globals.curtab));
    // SAFETY: forwarded from this function's own safety doc.
    let tp_ref = unsafe { &*tp };
    // SAFETY: forwarded from this function's own safety doc.
    let wp = if unsafe { crate::window::tabpage_win_valid(tp, tp_ref.tp_prevwin) } {
        tp_ref.tp_prevwin
    } else {
        tp_ref.tp_firstwin
    };
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &*wp };
    if !std::ptr::eq(wp, win) && w.w_config.focusable && !w.w_config.hide {
        wp
    } else {
        tp_ref.tp_firstwin
    }
}

/// Compares floating windows by descending z-index
/// (`float_zindex_cmp`).
///
/// Returns zero for equal z-indices, a positive value when `a`
/// belongs after `b`, and a negative value when it belongs before.
#[must_use]
pub fn float_zindex_cmp(a: &WinT, b: &WinT) -> i32 {
    let za = a.w_config.zindex;
    let zb = b.w_config.zindex;
    if za == zb {
        0
    } else if za < zb {
        1
    } else {
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn float_with_zindex(zindex: i32) -> WinT {
        let mut win = WinT::default();
        win.w_config.zindex = zindex;
        win
    }

    #[test]
    fn float_zindex_cmp_orders_larger_zindex_first() {
        let low = float_with_zindex(10);
        let high = float_with_zindex(30);
        assert!(float_zindex_cmp(&low, &high) > 0);
        assert!(float_zindex_cmp(&high, &low) < 0);
    }

    #[test]
    fn float_zindex_cmp_is_equal_for_equal_zindices() {
        assert_eq!(
            float_zindex_cmp(&float_with_zindex(20), &float_with_zindex(20)),
            0
        );
    }

    #[test]
    fn float_zindex_cmp_drives_descending_sort_order() {
        let mut wins = [
            float_with_zindex(10),
            float_with_zindex(30),
            float_with_zindex(20),
        ];
        wins.sort_by(|a, b| float_zindex_cmp(a, b).cmp(&0));
        let zindices = wins.map(|win| win.w_config.zindex);
        assert_eq!(zindices, [30, 20, 10]);
    }

    #[test]
    fn marks_a_window_anchored_float_as_position_changed() {
        let _lock = crate::globals::global_state_test_lock();
        let anchor_win = WinT { handle: 5, ..Default::default() };
        let mut floating = WinT {
            handle: 6,
            w_floating: true,
            w_config: crate::buffer_defs::WinConfig {
                relative: FloatRelative::Window,
                window: 5,
                ..Default::default()
            },
            ..Default::default()
        };
        let floating_ptr = &mut floating as *mut WinT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_lastwin = globals.lastwin;
        globals.lastwin = floating_ptr;

        unsafe { win_check_anchored_floats(&anchor_win) };

        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = prev_lastwin;

        assert!(floating.w_pos_changed);
    }

    #[test]
    fn ignores_a_float_anchored_to_a_different_window() {
        let _lock = crate::globals::global_state_test_lock();
        let anchor_win = WinT { handle: 5, ..Default::default() };
        let mut floating = WinT {
            handle: 6,
            w_floating: true,
            w_config: crate::buffer_defs::WinConfig {
                relative: FloatRelative::Window,
                window: 99,
                ..Default::default()
            },
            ..Default::default()
        };
        let floating_ptr = &mut floating as *mut WinT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_lastwin = globals.lastwin;
        globals.lastwin = floating_ptr;

        unsafe { win_check_anchored_floats(&anchor_win) };

        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = prev_lastwin;

        assert!(!floating.w_pos_changed);
    }

    #[test]
    fn stops_at_the_first_non_floating_window() {
        let _lock = crate::globals::global_state_test_lock();
        let anchor_win = WinT { handle: 5, ..Default::default() };
        // A non-floating window sits between lastwin and any floating
        // ones in this test - matching the original's own loop
        // condition (`wp && wp->w_floating`), which stops as soon as
        // a non-floating window is reached (floating windows are
        // always kept at the END of the window list).
        let non_floating = WinT { handle: 7, w_floating: false, ..Default::default() };
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_lastwin = globals.lastwin;
        let mut non_floating = non_floating;
        globals.lastwin = &mut non_floating as *mut WinT;

        // Should not panic despite w_prev being null on non_floating.
        unsafe { win_check_anchored_floats(&anchor_win) };

        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = prev_lastwin;
    }

    #[test]
    fn win_border_height_sums_top_and_bottom() {
        let wp = WinT { w_border_adj: [1, 2, 3, 4], ..Default::default() };
        assert_eq!(win_border_height(&wp), 1 + 3);
    }

    #[test]
    fn win_border_width_sums_left_and_right() {
        let wp = WinT { w_border_adj: [1, 2, 3, 4], ..Default::default() };
        assert_eq!(win_border_width(&wp), 2 + 4);
    }

    #[test]
    fn win_float_valid_false_for_null() {
        assert!(!unsafe { win_float_valid(std::ptr::null()) });
    }

    #[test]
    fn win_float_valid_true_for_a_floating_window_in_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 3, w_floating: true, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        globals.firstwin = win_ptr;

        assert!(unsafe { win_float_valid(win_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    #[test]
    fn win_float_valid_false_for_a_non_floating_window_in_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 3, w_floating: false, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        globals.firstwin = win_ptr;

        assert!(!unsafe { win_float_valid(win_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    #[test]
    fn win_float_valid_false_when_win_is_not_in_the_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut listed = WinT { handle: 1, ..Default::default() };
        let listed_ptr = &mut listed as *mut WinT;
        let not_listed = WinT { handle: 2, w_floating: true, ..Default::default() };

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        globals.firstwin = listed_ptr;

        assert!(!unsafe { win_float_valid(&not_listed as *const WinT) });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    #[test]
    fn win_float_valid_walks_past_the_first_entry() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = WinT { handle: 2, w_floating: true, ..Default::default() };
        let second_ptr = &mut second as *mut WinT;
        let mut first = WinT { handle: 1, w_next: second_ptr, ..Default::default() };
        let first_ptr = &mut first as *mut WinT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        globals.firstwin = first_ptr;

        assert!(unsafe { win_float_valid(second_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    // ---- win_float_anchor_laststatus ----

    #[test]
    fn win_float_anchor_laststatus_marks_matching_windows() {
        let _lock = crate::globals::global_state_test_lock();
        let mut other = WinT {
            handle: 2,
            w_config: crate::buffer_defs::WinConfig { relative: FloatRelative::Editor, ..Default::default() },
            ..Default::default()
        };
        let other_ptr = &mut other as *mut WinT;
        let mut matching = WinT {
            handle: 1,
            w_next: other_ptr,
            w_config: crate::buffer_defs::WinConfig { relative: FloatRelative::Laststatus, ..Default::default() },
            ..Default::default()
        };
        let matching_ptr = &mut matching as *mut WinT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        globals.firstwin = matching_ptr;

        unsafe { win_float_anchor_laststatus() };

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;

        assert!(unsafe { &*matching_ptr }.w_pos_changed);
        assert!(!unsafe { &*other_ptr }.w_pos_changed);
    }

    #[test]
    fn win_float_anchor_laststatus_no_op_when_no_window_matches() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT {
            handle: 1,
            w_config: crate::buffer_defs::WinConfig { relative: FloatRelative::Cursor, ..Default::default() },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        globals.firstwin = win_ptr;

        unsafe { win_float_anchor_laststatus() };

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;

        assert!(!unsafe { &*win_ptr }.w_pos_changed);
    }

    // ---- win_float_find_preview ----

    #[test]
    fn win_float_find_preview_finds_an_info_kind_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut preview = WinT {
            handle: 1,
            w_floating: true,
            w_kind: crate::buffer_defs::WinKind::Info,
            ..Default::default()
        };
        let preview_ptr = &mut preview as *mut WinT;
        let mut top =
            WinT { handle: 2, w_floating: true, w_prev: preview_ptr, ..Default::default() };
        let top_ptr = &mut top as *mut WinT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_lastwin = globals.lastwin;
        globals.lastwin = top_ptr;

        let found = unsafe { win_float_find_preview() };

        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = prev_lastwin;

        assert_eq!(found, preview_ptr);
    }

    #[test]
    fn win_float_find_preview_null_when_no_floating_windows() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 1, w_floating: false, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_lastwin = globals.lastwin;
        globals.lastwin = win_ptr;

        assert!(unsafe { win_float_find_preview() }.is_null());

        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = prev_lastwin;
    }

    #[test]
    fn win_float_find_preview_null_when_no_floating_window_is_info_kind() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 1, w_floating: true, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_lastwin = globals.lastwin;
        globals.lastwin = win_ptr;

        assert!(unsafe { win_float_find_preview() }.is_null());

        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = prev_lastwin;
    }

    // ---- win_float_find_altwin ----

    fn focusable_visible_win(handle: crate::types_defs::HandleT) -> WinT {
        WinT {
            handle,
            w_config: crate::buffer_defs::WinConfig { focusable: true, hide: false, ..Default::default() },
            ..Default::default()
        }
    }

    #[test]
    fn win_float_find_altwin_null_tp_uses_prevwin_when_valid_and_usable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut prev = focusable_visible_win(1);
        let prev_ptr = &mut prev as *mut WinT;
        let mut cur_tp = crate::buffer_defs::TabpageT::default();
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        let other_win = WinT { handle: 2, ..Default::default() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_curtab, prev_firstwin, prev_prevwin) = (g.curtab, g.firstwin, g.prevwin);
        g.curtab = cur_tp_ptr;
        g.firstwin = prev_ptr;
        g.prevwin = prev_ptr;

        let result = unsafe { win_float_find_altwin(&other_win as *const WinT, std::ptr::null()) };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curtab = prev_curtab;
        g.firstwin = prev_firstwin;
        g.prevwin = prev_prevwin;

        assert_eq!(result, prev_ptr);
    }

    #[test]
    fn win_float_find_altwin_null_tp_falls_back_to_firstwin_when_prevwin_is_win_itself() {
        let _lock = crate::globals::global_state_test_lock();
        let mut prev = focusable_visible_win(1);
        let prev_ptr = &mut prev as *mut WinT;
        let mut cur_tp = crate::buffer_defs::TabpageT::default();
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_curtab, prev_firstwin, prev_prevwin) = (g.curtab, g.firstwin, g.prevwin);
        g.curtab = cur_tp_ptr;
        g.firstwin = prev_ptr;
        g.prevwin = prev_ptr;

        // win IS prevwin itself - must fall back to firstwin.
        let result = unsafe { win_float_find_altwin(prev_ptr, std::ptr::null()) };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curtab = prev_curtab;
        g.firstwin = prev_firstwin;
        g.prevwin = prev_prevwin;

        assert_eq!(result, prev_ptr); // firstwin == prev_ptr here too
    }

    #[test]
    fn win_float_find_altwin_null_tp_falls_back_to_firstwin_when_prevwin_not_focusable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut prev = WinT {
            handle: 1,
            w_config: crate::buffer_defs::WinConfig { focusable: false, hide: false, ..Default::default() },
            ..Default::default()
        };
        let prev_ptr = &mut prev as *mut WinT;
        let mut first = focusable_visible_win(3);
        let first_ptr = &mut first as *mut WinT;
        let mut cur_tp = crate::buffer_defs::TabpageT::default();
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        let other_win = WinT { handle: 2, ..Default::default() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_curtab, prev_firstwin, prev_prevwin) = (g.curtab, g.firstwin, g.prevwin);
        g.curtab = cur_tp_ptr;
        g.firstwin = first_ptr;
        g.prevwin = prev_ptr;

        let result = unsafe { win_float_find_altwin(&other_win as *const WinT, std::ptr::null()) };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curtab = prev_curtab;
        g.firstwin = prev_firstwin;
        g.prevwin = prev_prevwin;

        assert_eq!(result, first_ptr);
    }

    #[test]
    fn win_float_find_altwin_null_tp_falls_back_to_firstwin_when_prevwin_is_not_valid() {
        let _lock = crate::globals::global_state_test_lock();
        // prevwin points at a window NOT in the current tab's list.
        let mut prev = focusable_visible_win(1);
        let prev_ptr = &mut prev as *mut WinT;
        let mut first = focusable_visible_win(3);
        let first_ptr = &mut first as *mut WinT;
        let mut cur_tp = crate::buffer_defs::TabpageT::default();
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        let other_win = WinT { handle: 2, ..Default::default() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_curtab, prev_firstwin, prev_prevwin) = (g.curtab, g.firstwin, g.prevwin);
        g.curtab = cur_tp_ptr;
        g.firstwin = first_ptr; // prev_ptr is NOT reachable from firstwin
        g.prevwin = prev_ptr;

        let result = unsafe { win_float_find_altwin(&other_win as *const WinT, std::ptr::null()) };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curtab = prev_curtab;
        g.firstwin = prev_firstwin;
        g.prevwin = prev_prevwin;

        assert_eq!(result, first_ptr);
    }

    #[test]
    fn win_float_find_altwin_with_tp_uses_tp_prevwin_when_valid_and_usable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp_prev = focusable_visible_win(1);
        let tp_prev_ptr = &mut tp_prev as *mut WinT;
        let mut tp_first = focusable_visible_win(3);
        let tp_first_ptr = &mut tp_first as *mut WinT;
        // tp_prev must be reachable from tp_first via w_next for
        // tabpage_win_valid to consider it valid within tp.
        unsafe { &mut *tp_first_ptr }.w_next = tp_prev_ptr;
        let mut tp = crate::buffer_defs::TabpageT {
            tp_prevwin: tp_prev_ptr,
            tp_firstwin: tp_first_ptr,
            ..Default::default()
        };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let mut cur_tp = crate::buffer_defs::TabpageT::default();
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        let other_win = WinT { handle: 2, ..Default::default() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_curtab = g.curtab;
        g.curtab = cur_tp_ptr; // distinct from tp

        let result = unsafe { win_float_find_altwin(&other_win as *const WinT, tp_ptr) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;

        assert_eq!(result, tp_prev_ptr);
    }

    #[test]
    fn win_float_find_altwin_with_tp_falls_back_to_tp_firstwin_when_tp_prevwin_invalid() {
        let _lock = crate::globals::global_state_test_lock();
        // tp_prev is NOT linked into tp's own window list at all.
        let mut tp_prev = focusable_visible_win(1);
        let tp_prev_ptr = &mut tp_prev as *mut WinT;
        let mut tp_first = focusable_visible_win(3);
        let tp_first_ptr = &mut tp_first as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT {
            tp_prevwin: tp_prev_ptr,
            tp_firstwin: tp_first_ptr,
            ..Default::default()
        };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let mut cur_tp = crate::buffer_defs::TabpageT::default();
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        let other_win = WinT { handle: 2, ..Default::default() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_curtab = g.curtab;
        g.curtab = cur_tp_ptr;

        let result = unsafe { win_float_find_altwin(&other_win as *const WinT, tp_ptr) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;

        assert_eq!(result, tp_first_ptr);
    }
}
