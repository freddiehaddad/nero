//! Translated from `src/nvim/ex_cmds.c` (partial - a tiny, deliberate
//! harvest of a couple of small, self-contained functions).
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
//! file" precedent already used for `ex_docmd.rs`. Also
//! [`check_regexp_delim`] - whether `c` is NOT a letter (letters can't
//! delimit a `:s/pat/sub/`-style regexp), needed by `do_sub` and a
//! sibling substitution-command handler (neither translated yet) - the
//! original's own real `emsg("E146: ...")` display is omitted
//! (message display, not tractable), matching `check_secure`'s own
//! established policy just above, while the exact same `FAIL`/`OK`
//! return value is kept.

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

/// Whether `c` is a valid regexp delimiter for `:s/pat/sub/`-style
/// commands: `false` for a letter (letters can't delimit a regexp),
/// `true` otherwise (`check_regexp_delim`).
///
/// The original's own real `emsg("E146: ...")` display (for the
/// letter case) is omitted, matching [`check_secure`]'s own
/// established policy - the exact same `FAIL`/`OK`-equivalent return
/// value is kept.
#[must_use]
pub fn check_regexp_delim(c: i32) -> bool {
    !crate::macros_defs::ascii_isalpha(c)
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

    #[test]
    fn check_regexp_delim_rejects_letters() {
        assert!(!check_regexp_delim(i32::from(b'a')));
        assert!(!check_regexp_delim(i32::from(b'Z')));
    }

    #[test]
    fn check_regexp_delim_accepts_common_delimiters() {
        assert!(check_regexp_delim(i32::from(b'/')));
        assert!(check_regexp_delim(i32::from(b'#')));
        assert!(check_regexp_delim(i32::from(b',')));
    }

    #[test]
    fn check_regexp_delim_accepts_a_digit() {
        assert!(check_regexp_delim(i32::from(b'0')));
    }
}
