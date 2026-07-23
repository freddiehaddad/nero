//! Translated from `src/nvim/ex_eval.c` (tractable core only).
//!
//! `ex_eval.c` (~2000 lines) implements `:try`/`:catch`/`:finally`/
//! `:throw` exception handling for Ex commands. Only the two small,
//! self-contained predicate functions needed by
//! [`crate::autocmd::apply_autocmds_retval`] (their first real
//! caller) are translated here: [`aborting`] and [`should_abort`].
//! Both need only already-existing `GLOBALS` fields
//! (`did_emsg`/`force_abort`/`got_int`/`did_throw`/`trylevel`/
//! `emsg_silent`) - no `:try`/`:catch` parsing or exception-stack
//! machinery is needed for either.
//!
//! Deferred: everything else in this file (the actual `:try`/`:catch`/
//! `:throw` command handlers, `cstack_T` exception-stack management,
//! `did_emsg_cumul`, etc.) - genuinely substantial, needs the `:try`
//! command parser (not yet translated).

use crate::vim_defs::FAIL;

/// Returns `true` when immediately aborting on error, or when an
/// interrupt occurred or an exception was thrown but not caught
/// (`aborting`).
#[must_use]
pub fn aborting() -> bool {
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    (g.did_emsg != 0 && g.force_abort) || g.got_int || g.did_throw
}

/// Returns `true` if a command with a subcommand resulting in
/// `retcode` should abort the script processing (`should_abort`).
#[must_use]
pub fn should_abort(retcode: i32) -> bool {
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    (retcode == FAIL && g.trylevel != 0 && g.emsg_silent == 0) || aborting()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::{global_state_test_lock, GLOBALS};
    use crate::vim_defs::OK;

    /// Resets every field `aborting`/`should_abort` read, restoring
    /// them on drop - callers must hold `global_state_test_lock()`
    /// for the guard's whole lifetime.
    struct AbortStateGuard {
        did_emsg: i32,
        force_abort: bool,
        got_int: bool,
        did_throw: bool,
        trylevel: i32,
        emsg_silent: i32,
    }

    impl AbortStateGuard {
        fn new() -> Self {
            let g = unsafe { GLOBALS.get_mut() };
            let saved = Self {
                did_emsg: g.did_emsg,
                force_abort: g.force_abort,
                got_int: g.got_int,
                did_throw: g.did_throw,
                trylevel: g.trylevel,
                emsg_silent: g.emsg_silent,
            };
            g.did_emsg = 0;
            g.force_abort = false;
            g.got_int = false;
            g.did_throw = false;
            g.trylevel = 0;
            g.emsg_silent = 0;
            saved
        }
    }

    impl Drop for AbortStateGuard {
        fn drop(&mut self) {
            let g = unsafe { GLOBALS.get_mut() };
            g.did_emsg = self.did_emsg;
            g.force_abort = self.force_abort;
            g.got_int = self.got_int;
            g.did_throw = self.did_throw;
            g.trylevel = self.trylevel;
            g.emsg_silent = self.emsg_silent;
        }
    }

    #[test]
    fn aborting_is_false_in_a_clean_state() {
        let _lock = global_state_test_lock();
        let _guard = AbortStateGuard::new();
        assert!(!aborting());
    }

    #[test]
    fn aborting_is_true_when_did_emsg_and_force_abort_both_set() {
        let _lock = global_state_test_lock();
        let _guard = AbortStateGuard::new();
        let g = unsafe { GLOBALS.get_mut() };
        g.did_emsg = 1;
        g.force_abort = true;
        assert!(aborting());
    }

    #[test]
    fn aborting_is_false_when_only_did_emsg_set_without_force_abort() {
        let _lock = global_state_test_lock();
        let _guard = AbortStateGuard::new();
        let g = unsafe { GLOBALS.get_mut() };
        g.did_emsg = 1;
        g.force_abort = false;
        assert!(!aborting());
    }

    #[test]
    fn aborting_is_true_when_got_int_set() {
        let _lock = global_state_test_lock();
        let _guard = AbortStateGuard::new();
        unsafe { GLOBALS.get_mut() }.got_int = true;
        assert!(aborting());
    }

    #[test]
    fn aborting_is_true_when_did_throw_set() {
        let _lock = global_state_test_lock();
        let _guard = AbortStateGuard::new();
        unsafe { GLOBALS.get_mut() }.did_throw = true;
        assert!(aborting());
    }

    #[test]
    fn should_abort_is_false_for_ok_retcode_in_a_clean_state() {
        let _lock = global_state_test_lock();
        let _guard = AbortStateGuard::new();
        assert!(!should_abort(OK));
    }

    #[test]
    fn should_abort_is_true_for_fail_retcode_inside_a_try_without_emsg_silent() {
        let _lock = global_state_test_lock();
        let _guard = AbortStateGuard::new();
        let g = unsafe { GLOBALS.get_mut() };
        g.trylevel = 1;
        g.emsg_silent = 0;
        assert!(should_abort(FAIL));
    }

    #[test]
    fn should_abort_is_false_for_fail_retcode_when_emsg_silent() {
        let _lock = global_state_test_lock();
        let _guard = AbortStateGuard::new();
        let g = unsafe { GLOBALS.get_mut() };
        g.trylevel = 1;
        g.emsg_silent = 1;
        assert!(!should_abort(FAIL));
    }

    #[test]
    fn should_abort_is_false_for_fail_retcode_outside_any_try() {
        let _lock = global_state_test_lock();
        let _guard = AbortStateGuard::new();
        let g = unsafe { GLOBALS.get_mut() };
        g.trylevel = 0;
        g.emsg_silent = 0;
        assert!(!should_abort(FAIL));
    }

    #[test]
    fn should_abort_defers_to_aborting_regardless_of_retcode() {
        let _lock = global_state_test_lock();
        let _guard = AbortStateGuard::new();
        unsafe { GLOBALS.get_mut() }.got_int = true;
        assert!(should_abort(OK));
    }
}
