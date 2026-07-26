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
//! needed `option.c`'s `get_ve_flags`, already translated),
//! `get_real_state` (resolves `MODE_NORMAL`'s "real" sub-state -
//! Visual/Select/op-pending - all fields already existed via
//! `globals.rs`/`normal_defs.rs`), and `get_mode` (the `mode()`
//! builtin's own state-to-string formatter - re-investigated and found
//! to be a pure state reader with NO event-loop interaction of its
//! own, unlike `state_enter` itself; see `get_mode`'s own doc comment
//! for exactly which of its many branches are decidable today vs.
//! genuinely unreachable). These are simple, self-contained
//! global-state readers with no design freedom of their own - the
//! usual "harvest the tractable core" pattern, even without a real
//! caller yet among currently-translated code (matching the
//! established precedent, e.g. `cursor.c`'s batch last session).
//!
//! Everything else - `state_enter`, `check_pending`, `may_sync_undo`,
//! `restart_edit`-related helpers, `os_breakcheck`/`line_breakcheck`
//! (need `os/signal.c`'s `SignalWatcher`) - is deferred, genuinely
//! event-loop-bound.

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

/// Determine the current mode as a short byte string, e.g. `b"n"`
/// (Normal), `b"i"` (Insert), `b"v"` (Visual) (`get_mode`).
///
/// Re-investigated after this module's own earlier doc comment
/// mis-classified it as "genuinely event-loop-bound, deferred
/// alongside `state_enter`" - `get_mode` itself is a pure state-to-
/// string formatter with no event-loop interaction of its own. Two of
/// its real predicates, `ins_compl_active()`/
/// `ctrl_x_mode_not_defined_yet()` (`insexpand.c`, not translated),
/// are modeled as their real, verified default/idle values
/// (`compl_started == false`; `ctrl_x_mode == CTRL_X_NORMAL`, NOT
/// `CTRL_X_NOT_DEFINED_YET`) - nothing in this crate can currently
/// change either away from that default (no insert-completion
/// subsystem exists), so "false" is the correct, decidable answer
/// today, not a guess. The `MODE_CMDLINE`-dependent branches (needing
/// `ex_getln.c`'s `get_cmdline_info()`/`cmdline_overstrike()`, neither
/// translated) panic via `unimplemented!()` if ever actually reached -
/// unlike the two predicates above, nothing in this crate can
/// currently construct a `State` value with the `MODE_CMDLINE` bit set
/// at all (no command-line editing subsystem exists), so there is no
/// real default to fall back on for those specific branches.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn get_mode() -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let mut buf = Vec::new();

    // The 4th OR operand (`(State & MODE_CMDLINE) && get_cmdline_info()
    // .one_key`) is short-circuited exactly like the original: the
    // `unimplemented!()` only actually runs if the CMDLINE bit is set,
    // which nothing in this crate can currently do.
    #[allow(clippy::nonminimal_bool)]
    if g.State == mode::HITRETURN as i32
        || g.State == mode::ASKMORE as i32
        || g.State == mode::SETWSIZE as i32
        || (g.State & mode::CMDLINE as i32 != 0
            && unimplemented!(
                "get_mode: MODE_CMDLINE's one_key state needs ex_getln.c's get_cmdline_info, not yet translated"
            ))
    {
        buf.push(b'r');
        if g.State == mode::ASKMORE as i32 {
            buf.push(b'm');
        } else if g.State & mode::CMDLINE as i32 != 0 {
            buf.push(b'?');
        }
    } else if g.State == mode::EXTERNCMD as i32 {
        buf.push(b'!');
    } else if g.State & mode::INSERT as i32 != 0 {
        if g.State & mode::VREPLACE_FLAG as i32 != 0 {
            buf.push(b'R');
            buf.push(b'v');
        } else if g.State & mode::REPLACE_FLAG as i32 != 0 {
            buf.push(b'R');
        } else {
            buf.push(b'i');
        }
        // ins_compl_active()/ctrl_x_mode_not_defined_yet() (insexpand.c)
        // both real, verified false in their default/idle state - see
        // this function's own doc comment.
    } else if g.State & mode::CMDLINE as i32 != 0 || g.exmode_active {
        buf.push(b'c');
        if g.exmode_active {
            buf.push(b'v');
        }
        if g.State & mode::CMDLINE as i32 != 0
            && unimplemented!(
                "get_mode: MODE_CMDLINE's overstrike state needs ex_getln.c's cmdline_overstrike, not yet translated"
            )
        {
            buf.push(b'r');
        }
    } else if g.State & mode::TERMINAL as i32 != 0 {
        buf.push(b't');
    } else if g.Visual.active {
        if g.Visual.select {
            buf.push((g.Visual.mode as u8).wrapping_add(b's').wrapping_sub(b'v'));
        } else {
            buf.push(g.Visual.mode as u8);
            if g.Visual.restart_select != 0 {
                buf.push(b's');
            }
        }
    } else {
        buf.push(b'n');
        if g.finish_op {
            buf.push(b'o');
            buf.push(g.motion_force as u8);
        } else if !unsafe { (*g.curbuf).terminal }.is_null() {
            buf.push(b't');
            if g.restart_edit == i32::from(b'I') {
                buf.push(b'T');
            }
        } else if g.restart_edit == i32::from(b'I') || g.restart_edit == i32::from(b'R') || g.restart_edit == i32::from(b'V')
        {
            buf.push(b'i');
            buf.push(g.restart_edit as u8);
        }
    }

    buf
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

    /// RAII guard temporarily installing a real `GLOBALS.curbuf`
    /// (`get_mode`'s own Normal-mode branch reads `curbuf.terminal`).
    /// Self-locking, matching this crate's established per-file
    /// `CurbufGuard` convention.
    struct CurbufGuard {
        previous: *mut crate::buffer_defs::BufT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut crate::buffer_defs::BufT) -> Self {
            let _lock = global_state_test_lock();
            let previous = unsafe { GLOBALS.get_mut() }.curbuf;
            unsafe { GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous, _lock }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    fn reset_mode_globals() {
        let g = unsafe { GLOBALS.get_mut() };
        g.State = mode::NORMAL as i32;
        g.Visual = crate::normal_defs::VisualState::default();
        g.finish_op = false;
        g.motion_force = 0;
        g.exmode_active = false;
        g.restart_edit = 0;
    }

    #[test]
    fn get_mode_normal_default() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        let result = unsafe { get_mode() };
        reset_mode_globals();
        assert_eq!(result, b"n".to_vec());
    }

    #[test]
    fn get_mode_insert() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        unsafe { GLOBALS.get_mut() }.State = mode::INSERT as i32;
        let result = unsafe { get_mode() };
        reset_mode_globals();
        assert_eq!(result, b"i".to_vec());
    }

    #[test]
    fn get_mode_replace() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        unsafe { GLOBALS.get_mut() }.State = mode::REPLACE as i32;
        let result = unsafe { get_mode() };
        reset_mode_globals();
        assert_eq!(result, b"R".to_vec());
    }

    #[test]
    fn get_mode_vreplace() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        unsafe { GLOBALS.get_mut() }.State = mode::VREPLACE as i32;
        let result = unsafe { get_mode() };
        reset_mode_globals();
        assert_eq!(result, b"Rv".to_vec());
    }

    #[test]
    fn get_mode_visual_charwise() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        {
            let g = unsafe { GLOBALS.get_mut() };
            g.Visual.active = true;
            g.Visual.mode = i32::from(b'v');
        }
        let result = unsafe { get_mode() };
        // Every test in this block resets ALL touched globals back to
        // default at the end (not just at the start) - a real,
        // reproducible flaky failure in the PRE-EXISTING
        // get_real_state_op_pending_mode test (which doesn't defend
        // itself against a leaked Visual.active) was caught this way:
        // that test only sets State/finish_op, silently relying on
        // Visual.active already being false.
        reset_mode_globals();
        assert_eq!(result, b"v".to_vec());
    }

    #[test]
    fn get_mode_select_mode_derives_from_visual_mode() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        {
            let g = unsafe { GLOBALS.get_mut() };
            g.Visual.active = true;
            g.Visual.select = true;
            g.Visual.mode = i32::from(b'v');
        }
        // 'v' + 's' - 'v' == 's' (charwise Select).
        let result = unsafe { get_mode() };
        reset_mode_globals();
        assert_eq!(result, b"s".to_vec());
    }

    #[test]
    fn get_mode_op_pending() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        {
            let g = unsafe { GLOBALS.get_mut() };
            g.finish_op = true;
            g.motion_force = i32::from(b'l');
        }
        // The "else" (Normal-mode) branch always pushes 'n' first,
        // then 'o' + motion_force when finish_op is set.
        let result = unsafe { get_mode() };
        reset_mode_globals();
        assert_eq!(result, b"nol".to_vec());
    }

    #[test]
    fn get_mode_terminal_via_curbuf() {
        // Never dereferenced by get_mode - only checked for nullness.
        let mut buf = crate::buffer_defs::BufT {
            terminal: std::ptr::dangling_mut::<crate::types_defs::TerminalT>(),
            ..Default::default()
        };
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        // Same leading 'n' as get_mode_op_pending, then 't' for the
        // curbuf.terminal check.
        let result = unsafe { get_mode() };
        reset_mode_globals();
        assert_eq!(result, b"nt".to_vec());
    }

    #[test]
    fn get_mode_exmode() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        unsafe { GLOBALS.get_mut() }.exmode_active = true;
        let result = unsafe { get_mode() };
        reset_mode_globals();
        assert_eq!(result, b"cv".to_vec());
    }

    #[test]
    fn get_mode_hitreturn() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        unsafe { GLOBALS.get_mut() }.State = mode::HITRETURN as i32;
        let result = unsafe { get_mode() };
        reset_mode_globals();
        assert_eq!(result, b"r".to_vec());
    }

    #[test]
    fn get_mode_askmore() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        unsafe { GLOBALS.get_mut() }.State = mode::ASKMORE as i32;
        let result = unsafe { get_mode() };
        reset_mode_globals();
        assert_eq!(result, b"rm".to_vec());
    }

    #[test]
    fn get_mode_externcmd() {
        let mut buf = crate::buffer_defs::BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);
        reset_mode_globals();
        unsafe { GLOBALS.get_mut() }.State = mode::EXTERNCMD as i32;
        let result = unsafe { get_mode() };
        reset_mode_globals();
        assert_eq!(result, b"!".to_vec());
    }
}
