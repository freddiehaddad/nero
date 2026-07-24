//! Translated from `src/nvim/ex_docmd.c` (partial - a tiny, deliberate
//! harvest of one small, self-contained function).
//!
//! `ex_docmd.c` (~8600 lines) is the ex-command line parser/dispatcher
//! (`:` command execution, `do_cmdline`, the `ex_*` handler table) - a
//! whole separate, substantial phase-6 undertaking, not attempted here.
//!
//! Translated: `expr_map_locked` - needed as a dependency by
//! `undo.c`'s `undo_allowed`, `insert.c`, `ex_getln.c`, and
//! `api/win_config.c` (none of the latter 3 translated yet), so it's
//! harvested here on its own rather than waiting for the rest of this
//! file.

use crate::buffer_defs::b_flags;

/// Return true if the current buffer is locked because it is being used
/// for evaluating an expression from `'foldexpr'`, `'formatexpr'`, or
/// similar option-expression contexts, via `:normal`'s temporary
/// `expr_map_lock` counter (`expr_map_locked`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn expr_map_locked() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if globals.expr_map_lock <= 0 {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*globals.curbuf };
    curbuf.b_flags & (b_flags::BF_DUMMY as i32) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    /// Points `GLOBALS.curbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime
    /// (matches `change.rs`'s/`mark.rs`'s established `CurbufGuard`
    /// convention: does NOT acquire its own lock).
    struct CurbufGuard {
        previous: *mut BufT,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut BufT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    #[test]
    fn false_when_expr_map_lock_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        unsafe { crate::globals::GLOBALS.get_mut() }.expr_map_lock = 0;

        assert!(!unsafe { expr_map_locked() });
    }

    #[test]
    fn true_when_locked_and_curbuf_not_dummy() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        unsafe { crate::globals::GLOBALS.get_mut() }.expr_map_lock = 1;

        assert!(unsafe { expr_map_locked() });

        unsafe { crate::globals::GLOBALS.get_mut() }.expr_map_lock = 0;
    }

    #[test]
    fn false_when_locked_but_curbuf_is_dummy() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_flags: b_flags::BF_DUMMY as i32, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        unsafe { crate::globals::GLOBALS.get_mut() }.expr_map_lock = 1;

        assert!(!unsafe { expr_map_locked() });

        unsafe { crate::globals::GLOBALS.get_mut() }.expr_map_lock = 0;
    }

    #[test]
    fn false_when_expr_map_lock_negative() {
        // Matches the original's `> 0` check (not `!= 0`) - a negative
        // value must also be treated as "not locked".
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        unsafe { crate::globals::GLOBALS.get_mut() }.expr_map_lock = -1;

        assert!(!unsafe { expr_map_locked() });

        unsafe { crate::globals::GLOBALS.get_mut() }.expr_map_lock = 0;
    }
}
