//! Translated from `src/nvim/ex_cmds.c` (partial - a tiny, deliberate
//! harvest of a single small, self-contained function).
//!
//! `ex_cmds.c` (~7000 lines) implements most `:`-command handlers
//! (`:copy`, `:move`, `:sort`, `:write`, etc.) - a whole separate,
//! substantial phase-6 undertaking, not attempted here.
//!
//! Translated: `check_secure` - needed as a dependency by
//! `eval/fs.c`'s `f_delete`/`f_rename`/`f_filecopy` (none of the
//! latter two translated yet, but `f_delete` needs this one directly).
//! Harvested here on its own rather than waiting for the rest of this
//! file, matching the same "one tractable function ahead of a huge
//! file" precedent already used for `ex_docmd.rs`.

/// Return `true` (and disallow the caller's own operation) when
/// running with `'secure'`/`'-Z'`-style restrictions or inside the
/// sandbox (`check_secure`). The original's own real `emsg()` display
/// is omitted (message display, not tractable), matching this crate's
/// established "skip the message, keep the state" policy - but the
/// real, observable side effect (bumping `secure` from `1` to `2`,
/// so a SECOND, later call site can tell the restriction was actually
/// triggered at least once) is kept exactly.
///
/// # Safety
/// Touches `crate::globals::GLOBALS`.
#[must_use]
pub unsafe fn check_secure() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if globals.secure != 0 {
        globals.secure = 2;
        return true;
    }

    if globals.sandbox != 0 {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn globals_test_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::globals::global_state_test_lock()
    }

    #[test]
    fn check_secure_false_when_neither_secure_nor_sandbox() {
        let _guard = globals_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.secure = 0;
        globals.sandbox = 0;
        assert!(!unsafe { check_secure() });
    }

    #[test]
    fn check_secure_true_and_bumps_secure_to_two() {
        let _guard = globals_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.secure = 1;
        globals.sandbox = 0;
        assert!(unsafe { check_secure() });
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.secure, 2);
        // Restore for any other test relying on the default.
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
    }

    #[test]
    fn check_secure_true_when_sandboxed_but_leaves_secure_untouched() {
        let _guard = globals_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.secure = 0;
        globals.sandbox = 1;
        assert!(unsafe { check_secure() });
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.secure, 0);
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;
    }
}
