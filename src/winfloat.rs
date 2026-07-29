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

#[cfg(test)]
mod tests {
    use super::*;

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
}
