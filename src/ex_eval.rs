//! Translated from `src/nvim/ex_eval.c` (tractable core only).
//!
//! `ex_eval.c` (~2000 lines) implements `:try`/`:catch`/`:finally`/
//! `:throw` exception handling for Ex commands. Only 4 small,
//! self-contained functions are translated here: [`aborting`]/
//! [`should_abort`] (needed by
//! [`crate::autocmd::apply_autocmds_retval`], their first real
//! caller), [`aborted_in_try`] (needed by `eval/userfunc.c`'s
//! `func_has_ended`), and [`has_loop_cmd`] (whether a `:` command,
//! after skipping leading modifiers/whitespace/`:`, starts a
//! `:while`/`:for` loop - needs `ex_docmd.c`'s `modifier_len`,
//! `crate::ex_docmd`). All 4 need only already-existing `GLOBALS`
//! fields (`did_emsg`/`force_abort`/`got_int`/`did_throw`/`trylevel`/
//! `emsg_silent`) or the small, self-contained `modifier_len` - no
//! `:try`/`:catch` parsing or exception-stack machinery is needed for
//! any of them.
//!
//! Deferred: everything else in this file (the actual `:try`/`:catch`/
//! `:throw` command handlers, `cstack_T` exception-stack management,
//! `did_emsg_cumul`, etc.) - genuinely substantial, needs the `:try`
//! command parser (not yet translated).

use crate::vim_defs::FAIL;

/// Set while `force_abort` is being held back (`cause_abort`).
/// File-static in the original.
///
/// When several errors appear in a row, setting `force_abort` is
/// delayed until the failing command returns, and this flag records
/// that situation meanwhile. It matters when `force_abort` was set
/// during a function call inside an expression: aborting the
/// expression itself produces no messages, but parsing errors during
/// the evaluation must still be reported, even inside a `:try`.
static CAUSE_ABORT: crate::globals::GlobalCell<bool> = crate::globals::GlobalCell::new(false);

/// Restore `force_abort` from the held-back `CAUSE_ABORT` flag
/// (`update_force_abort`).
///
/// `force_abort` is temporarily reset by the first `emsg()` during an
/// expression evaluation, with `cause_abort` standing in for it; this
/// puts it back, which can be needed before the throw point for the
/// error message is reached.
///
/// # Safety
/// Must not run concurrently with any other access to `CAUSE_ABORT`
/// or `crate::globals::GLOBALS`.
pub unsafe fn update_force_abort() {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *CAUSE_ABORT.get_mut() } {
        // SAFETY: as above.
        unsafe { crate::globals::GLOBALS.get_mut() }.force_abort = true;
    }
}

/// Set or clear `CAUSE_ABORT`.
///
/// The original's callers (`cause_errthrow`/`do_errthrow`/
/// `enter_cleanup`/`leave_cleanup`) assign the file-static directly;
/// none of them is translated yet, so this accessor exists so the
/// flag can be driven once they are.
///
/// # Safety
/// Same as [`update_force_abort`].
pub unsafe fn set_cause_abort(flag: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *CAUSE_ABORT.get_mut() = flag };
}

/// Read `CAUSE_ABORT`.
///
/// # Safety
/// Same as [`update_force_abort`].
#[must_use]
pub unsafe fn cause_abort() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *CAUSE_ABORT.get_mut() }
}

/// Returns `true` when immediately aborting on error, or when an
/// interrupt occurred or an exception was thrown but not caught
/// (`aborting`).
#[must_use]
pub fn aborting() -> bool {
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    (g.did_emsg != 0 && g.force_abort) || g.got_int || g.did_throw
}

/// Saves the current exception state in `estate`
/// (`exception_state_save`).
///
/// Used around calls that run unrelated script code - timer callbacks
/// and deferred functions - so an exception in flight there cannot be
/// confused with one belonging to the interrupted code.
///
/// # Safety
/// Reads `crate::globals::GLOBALS`.
pub unsafe fn exception_state_save(estate: &mut crate::ex_eval_defs::ExceptionStateT) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    estate.estate_current_exception = g.current_exception;
    estate.estate_did_throw = g.did_throw;
    estate.estate_need_rethrow = g.need_rethrow;
    estate.estate_trylevel = g.trylevel;
    estate.estate_did_emsg = g.did_emsg;
}

/// Clears the current exception state (`exception_state_clear`).
///
/// Note this only drops the state; it does not discard the exception
/// `current_exception` still points at. The original's callers pair it
/// with a save/restore, which is what preserves that exception.
///
/// # Safety
/// Mutates `crate::globals::GLOBALS`.
pub unsafe fn exception_state_clear() {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    g.current_exception = std::ptr::null_mut();
    g.did_throw = false;
    g.need_rethrow = false;
    g.trylevel = 0;
    g.did_emsg = 0;
}

/// Returns `true` if a command with a subcommand resulting in
/// `retcode` should abort the script processing (`should_abort`).
#[must_use]
pub fn should_abort(retcode: i32) -> bool {
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    (retcode == FAIL && g.trylevel != 0 && g.emsg_silent == 0) || aborting()
}

/// Returns `true` if searching for `:finally` clauses is necessary,
/// after an error (`aborted_in_try`).
///
/// This function is only called after an error. In this case,
/// `force_abort` determines whether searching for finally clauses is
/// necessary.
#[must_use]
pub fn aborted_in_try() -> bool {
    unsafe { crate::globals::GLOBALS.get_mut() }.force_abort
}

/// Check if `p`, after skipping any leading command modifiers
/// (`:silent`, `:vertical`, etc.), whitespace, and `:` separators,
/// starts a `:while` or `:for` loop command (`has_loop_cmd`).
#[must_use]
pub fn has_loop_cmd(p: &[u8]) -> bool {
    let mut i = 0;
    loop {
        while matches!(p.get(i), Some(&b' ') | Some(&b'\t') | Some(&b':')) {
            i += 1;
        }
        let len = crate::ex_docmd::modifier_len(&p[i..]);
        if len == 0 {
            break;
        }
        i += len;
    }
    let rest = &p[i..];
    (rest.first() == Some(&b'w') && rest.get(1) == Some(&b'h'))
        || (rest.first() == Some(&b'f') && rest.get(1) == Some(&b'o') && rest.get(2) == Some(&b'r'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::{global_state_test_lock, GLOBALS};
    use crate::vim_defs::OK;

    // --- exception_state_save / clear ---

    /// Restores all five exception-state globals on drop, including
    /// when a test unwinds part-way through its assertions.
    struct ExceptionStateGuard(crate::ex_eval_defs::ExceptionStateT);

    impl ExceptionStateGuard {
        fn save() -> Self {
            let mut saved = crate::ex_eval_defs::ExceptionStateT::default();
            // SAFETY: the global state test lock is held by the caller.
            unsafe { exception_state_save(&mut saved) };
            Self(saved)
        }
    }

    impl Drop for ExceptionStateGuard {
        fn drop(&mut self) {
            // SAFETY: as in `save`.
            let g = unsafe { GLOBALS.get_mut() };
            g.current_exception = self.0.estate_current_exception;
            g.did_throw = self.0.estate_did_throw;
            g.need_rethrow = self.0.estate_need_rethrow;
            g.trylevel = self.0.estate_trylevel;
            g.did_emsg = self.0.estate_did_emsg;
        }
    }

    /// Each field is given a distinct value so a save that copied the
    /// wrong global into a slot would be caught.
    #[test]
    fn exception_state_save_captures_every_field() {
        let _lock = global_state_test_lock();
        let _g = ExceptionStateGuard::save();

        let marker = 0x1234_usize as *mut crate::ex_eval_defs::ExceptT;
        unsafe {
            let g = GLOBALS.get_mut();
            g.current_exception = marker;
            g.did_throw = true;
            g.need_rethrow = false;
            g.trylevel = 7;
            g.did_emsg = 3;
        }

        let mut estate = crate::ex_eval_defs::ExceptionStateT::default();
        unsafe { exception_state_save(&mut estate) };

        assert_eq!(estate.estate_current_exception, marker);
        assert!(estate.estate_did_throw);
        assert!(!estate.estate_need_rethrow, "must not be confused with did_throw");
        assert_eq!(estate.estate_trylevel, 7);
        assert_eq!(estate.estate_did_emsg, 3, "must not be confused with trylevel");
    }

    /// Saving must not disturb the state it reads.
    #[test]
    fn exception_state_save_leaves_the_globals_alone() {
        let _lock = global_state_test_lock();
        let _g = ExceptionStateGuard::save();

        unsafe {
            let g = GLOBALS.get_mut();
            g.did_throw = true;
            g.trylevel = 2;
        }

        let mut estate = crate::ex_eval_defs::ExceptionStateT::default();
        unsafe { exception_state_save(&mut estate) };

        unsafe {
            let g = GLOBALS.get_mut();
            assert!(g.did_throw);
            assert_eq!(g.trylevel, 2);
        }
    }

    #[test]
    fn exception_state_clear_resets_every_field() {
        let _lock = global_state_test_lock();
        let _g = ExceptionStateGuard::save();

        unsafe {
            let g = GLOBALS.get_mut();
            g.current_exception = 0x1234_usize as *mut crate::ex_eval_defs::ExceptT;
            g.did_throw = true;
            g.need_rethrow = true;
            g.trylevel = 7;
            g.did_emsg = 3;

            exception_state_clear();

            let g = GLOBALS.get_mut();
            assert!(g.current_exception.is_null());
            assert!(!g.did_throw);
            assert!(!g.need_rethrow);
            assert_eq!(g.trylevel, 0);
            assert_eq!(g.did_emsg, 0);
        }
    }

    /// A cleared state can be restored from a state saved beforehand,
    /// which is the whole point of the pair.
    #[test]
    fn a_saved_state_survives_a_clear() {
        let _lock = global_state_test_lock();
        let _g = ExceptionStateGuard::save();

        let marker = 0x4321_usize as *mut crate::ex_eval_defs::ExceptT;
        let mut estate = crate::ex_eval_defs::ExceptionStateT::default();
        unsafe {
            let g = GLOBALS.get_mut();
            g.current_exception = marker;
            g.trylevel = 5;

            exception_state_save(&mut estate);
            exception_state_clear();

            assert!(GLOBALS.get_mut().current_exception.is_null());
        }

        assert_eq!(estate.estate_current_exception, marker);
        assert_eq!(estate.estate_trylevel, 5);
    }

    // --- cause_abort / update_force_abort ---

    #[test]
    fn update_force_abort_restores_force_abort_when_cause_abort_is_set() {
        let _lock = global_state_test_lock();
        unsafe {
            let prev_cause = cause_abort();
            let prev_force = GLOBALS.get_mut().force_abort;

            set_cause_abort(true);
            GLOBALS.get_mut().force_abort = false;
            update_force_abort();
            assert!(GLOBALS.get_mut().force_abort);

            set_cause_abort(prev_cause);
            GLOBALS.get_mut().force_abort = prev_force;
        }
    }

    #[test]
    fn update_force_abort_leaves_force_abort_alone_without_cause_abort() {
        // The original only ever SETS force_abort here - it never
        // clears it, so a already-set flag survives a false
        // cause_abort too.
        let _lock = global_state_test_lock();
        unsafe {
            let prev_cause = cause_abort();
            let prev_force = GLOBALS.get_mut().force_abort;

            set_cause_abort(false);
            GLOBALS.get_mut().force_abort = false;
            update_force_abort();
            assert!(!GLOBALS.get_mut().force_abort);

            GLOBALS.get_mut().force_abort = true;
            update_force_abort();
            assert!(GLOBALS.get_mut().force_abort, "never cleared");

            set_cause_abort(prev_cause);
            GLOBALS.get_mut().force_abort = prev_force;
        }
    }

    #[test]
    fn cause_abort_round_trips_through_its_setter() {
        let _lock = global_state_test_lock();
        unsafe {
            let prev = cause_abort();
            set_cause_abort(true);
            assert!(cause_abort());
            set_cause_abort(false);
            assert!(!cause_abort());
            set_cause_abort(prev);
        }
    }

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

    #[test]
    fn aborted_in_try_reflects_force_abort() {
        let _lock = global_state_test_lock();
        let _guard = AbortStateGuard::new();
        assert!(!aborted_in_try());
        unsafe { GLOBALS.get_mut() }.force_abort = true;
        assert!(aborted_in_try());
    }

    #[test]
    fn has_loop_cmd_recognizes_while_directly() {
        assert!(has_loop_cmd(b"while 1"));
    }

    #[test]
    fn has_loop_cmd_recognizes_for_directly() {
        assert!(has_loop_cmd(b"for x in y"));
    }

    #[test]
    fn has_loop_cmd_false_for_an_unrelated_command() {
        assert!(!has_loop_cmd(b"echo 1"));
    }

    #[test]
    fn has_loop_cmd_skips_a_single_modifier_and_whitespace() {
        assert!(has_loop_cmd(b"silent while 1"));
    }

    #[test]
    fn has_loop_cmd_skips_several_chained_modifiers_and_colons() {
        assert!(has_loop_cmd(b"silent:vertical: for x in y"));
    }

    #[test]
    fn has_loop_cmd_false_when_modifiers_precede_an_unrelated_command() {
        assert!(!has_loop_cmd(b"silent echo 1"));
    }
}
