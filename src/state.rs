//! Translated from `src/nvim/state.c` (tractable core only).
//!
//! `state.c` (~700 lines) is the editor's main dispatch loop
//! (`state_enter`, `os_inchar`/`safe_igetc`-driven input processing,
//! `may_sync_undo`) - deeply tied to `event/loop.h`'s `MultiQueue`/
//! `event/multiqueue.c`'s event processing and the not-yet-translated
//! `input.c`/`getchar.c`. That machinery is genuine phase-11
//! (event-loop) material, not tractable here.
//!
//! Translated: `virtual_active` (whether the current mode uses virtual
//! editing, i.e. can the cursor be positioned past the end of a line -
//! needed `option.c`'s `get_ve_flags`, already translated) and
//! `get_real_state` (resolves `MODE_NORMAL`'s "real" sub-state -
//! Visual/Select/op-pending - all fields already existed via
//! `globals.rs`/`normal_defs.rs`). Both are simple, self-contained
//! global-state readers with no design freedom of their own - the
//! usual "harvest the tractable core" pattern, even without a real
//! caller yet among currently-translated code (matching the
//! established precedent, e.g. `cursor.c`'s batch last session).
//!
//! Everything else - `state_enter`, `get_mode` (needs `MODE_MAX_LENGTH`
//! and the full mode-string-building logic, `check_pending`,
//! `may_sync_undo`, `restart_edit`-related helpers, `os_breakcheck`/
//! `line_breakcheck` (need `os/signal.c`'s `SignalWatcher`) - is
//! deferred, genuinely event-loop-bound.

use crate::ascii_defs::CTRL_V;
use crate::buffer_defs::WinT;
use crate::state_defs::mode;
use crate::types_defs::TriState;

/// Return true if in the current mode we need to use virtual
/// (`virtual_active`).
#[must_use]
pub fn virtual_active(wp: &WinT) -> bool {
    let g = unsafe { crate::globals::GLOBALS.get_mut() };

    // In Terminal mode the cursor can be positioned anywhere by the
    // application.
    if g.State & mode::TERMINAL as i32 != 0 {
        return true;
    }

    let cur_ve_flags = crate::option::get_ve_flags(wp);

    if cur_ve_flags == crate::option_vars::opt_ve_flag::ALL
        || ((cur_ve_flags & crate::option_vars::opt_ve_flag::INSERT) != 0
            && (g.State & mode::INSERT as i32) != 0)
    {
        return true;
    }

    // While an operator is being executed we return "virtual_op",
    // because Visual.active has already been reset, thus we can't
    // check for "block" being used.
    if g.virtual_op != TriState::None {
        return g.virtual_op == TriState::True;
    }
    (cur_ve_flags & crate::option_vars::opt_ve_flag::BLOCK) != 0
        && g.Visual.active
        && g.Visual.mode == i32::from(CTRL_V)
}

/// `MODE_VISUAL`, `MODE_SELECT` and `MODE_OP_PENDING` State are never
/// set, they are equal to `MODE_NORMAL` State with a condition. This
/// function returns the real State (`get_real_state`).
#[must_use]
pub fn get_real_state() -> i32 {
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    if g.State & mode::NORMAL as i32 != 0 {
        if g.Visual.active {
            if g.Visual.select {
                return mode::SELECT as i32;
            }
            return mode::VISUAL as i32;
        } else if g.finish_op {
            return mode::OP_PENDING as i32;
        }
    }
    g.State
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::{global_state_test_lock, GLOBALS};

    fn default_win() -> WinT {
        WinT::default()
    }

    #[test]
    fn virtual_active_true_in_terminal_mode() {
        let _lock = global_state_test_lock();
        let win = default_win();
        unsafe { GLOBALS.get_mut() }.State = mode::TERMINAL as i32;
        assert!(virtual_active(&win));
        unsafe { GLOBALS.get_mut() }.State = 0;
    }

    #[test]
    fn virtual_active_false_by_default() {
        let _lock = global_state_test_lock();
        let win = default_win();
        unsafe { GLOBALS.get_mut() }.State = mode::NORMAL as i32;
        assert!(!virtual_active(&win));
    }

    #[test]
    fn virtual_active_true_with_ve_all() {
        let _lock = global_state_test_lock();
        let mut win = default_win();
        win.w_onebuf_opt.wo_ve_flags = crate::option_vars::opt_ve_flag::ALL;
        unsafe { GLOBALS.get_mut() }.State = mode::NORMAL as i32;
        assert!(virtual_active(&win));
    }

    #[test]
    fn virtual_active_true_with_ve_insert_in_insert_mode() {
        let _lock = global_state_test_lock();
        let mut win = default_win();
        win.w_onebuf_opt.wo_ve_flags = crate::option_vars::opt_ve_flag::INSERT;
        unsafe { GLOBALS.get_mut() }.State = mode::INSERT as i32;
        assert!(virtual_active(&win));
        unsafe { GLOBALS.get_mut() }.State = 0;
    }

    #[test]
    fn virtual_active_respects_virtual_op_override() {
        let _lock = global_state_test_lock();
        let win = default_win();
        unsafe { GLOBALS.get_mut() }.State = mode::NORMAL as i32;
        unsafe { GLOBALS.get_mut() }.virtual_op = TriState::True;
        let result = virtual_active(&win);
        unsafe { GLOBALS.get_mut() }.virtual_op = TriState::None;
        assert!(result);
    }

    #[test]
    fn virtual_active_true_with_ve_block_in_visual_block_mode() {
        let _lock = global_state_test_lock();
        let mut win = default_win();
        win.w_onebuf_opt.wo_ve_flags = crate::option_vars::opt_ve_flag::BLOCK;
        {
            let g = unsafe { GLOBALS.get_mut() };
            g.State = mode::NORMAL as i32;
            g.Visual.active = true;
            g.Visual.mode = i32::from(CTRL_V);
        }
        let result = virtual_active(&win);
        {
            let g = unsafe { GLOBALS.get_mut() };
            g.Visual.active = false;
            g.Visual.mode = 0;
        }
        assert!(result);
    }

    #[test]
    fn virtual_active_false_with_ve_block_in_charwise_visual_mode() {
        let _lock = global_state_test_lock();
        let mut win = default_win();
        win.w_onebuf_opt.wo_ve_flags = crate::option_vars::opt_ve_flag::BLOCK;
        {
            let g = unsafe { GLOBALS.get_mut() };
            g.State = mode::NORMAL as i32;
            g.Visual.active = true;
            g.Visual.mode = i32::from(b'v');
        }
        let result = virtual_active(&win);
        {
            let g = unsafe { GLOBALS.get_mut() };
            g.Visual.active = false;
            g.Visual.mode = 0;
        }
        assert!(!result);
    }

    #[test]
    fn get_real_state_plain_normal_mode() {
        let _lock = global_state_test_lock();
        unsafe { GLOBALS.get_mut() }.State = mode::NORMAL as i32;
        assert_eq!(get_real_state(), mode::NORMAL as i32);
    }

    #[test]
    fn get_real_state_visual_mode() {
        let _lock = global_state_test_lock();
        {
            let g = unsafe { GLOBALS.get_mut() };
            g.State = mode::NORMAL as i32;
            g.Visual.active = true;
            g.Visual.select = false;
        }
        let result = get_real_state();
        unsafe { GLOBALS.get_mut() }.Visual.active = false;
        assert_eq!(result, mode::VISUAL as i32);
    }

    #[test]
    fn get_real_state_select_mode() {
        let _lock = global_state_test_lock();
        {
            let g = unsafe { GLOBALS.get_mut() };
            g.State = mode::NORMAL as i32;
            g.Visual.active = true;
            g.Visual.select = true;
        }
        let result = get_real_state();
        {
            let g = unsafe { GLOBALS.get_mut() };
            g.Visual.active = false;
            g.Visual.select = false;
        }
        assert_eq!(result, mode::SELECT as i32);
    }

    #[test]
    fn get_real_state_op_pending_mode() {
        let _lock = global_state_test_lock();
        {
            let g = unsafe { GLOBALS.get_mut() };
            g.State = mode::NORMAL as i32;
            g.finish_op = true;
        }
        let result = get_real_state();
        unsafe { GLOBALS.get_mut() }.finish_op = false;
        assert_eq!(result, mode::OP_PENDING as i32);
    }

    #[test]
    fn get_real_state_passes_through_non_normal_state() {
        let _lock = global_state_test_lock();
        unsafe { GLOBALS.get_mut() }.State = mode::INSERT as i32;
        assert_eq!(get_real_state(), mode::INSERT as i32);
        unsafe { GLOBALS.get_mut() }.State = 0;
    }
}
