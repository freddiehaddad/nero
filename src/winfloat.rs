//! Translated from `src/nvim/winfloat.c` (partial).
//!
//! Translated: `win_check_anchored_floats` - harvested ahead of the
//! rest of this file (floating-window configuration/positioning,
//! phase 8/9 rendering territory) since it's a small, self-contained
//! function needed by `move.c`'s `check_topfill` (itself needed by
//! `eval/window.c`'s `winrestview()`).
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
}
