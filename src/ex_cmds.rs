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

/// Find the first unquoted `|` in `cmd` (`find_pipe`).
///
/// Quoted sections (inside `"`) and backslash-escaped bytes do not
/// count, so a `|` inside a quoted shell argument is not mistaken for
/// a command separator.
///
/// @return the byte offset of the separator, or `None` when there is
///         none. The original returns a pointer into `cmd`.
///
/// Only non-Unix builds use this - Unix passes the whole command to
/// the shell, which does its own splitting - matching the original's
/// own `#ifndef UNIX` guard.
#[cfg(not(unix))]
#[must_use]
pub fn find_pipe(cmd: &[u8]) -> Option<usize> {
    let mut inquote = false;
    let mut p = 0;
    while p < cmd.len() && cmd[p] != 0 {
        if !inquote && cmd[p] == b'|' {
            return Some(p);
        }
        if cmd[p] == b'"' {
            inquote = !inquote;
        } else if crate::charset::rem_backslash(&cmd[p..]) {
            p += 1;
        }
        p += 1;
    }
    None
}

/// Honour an `:write ++p` argument by creating any missing parent
/// directories (`handle_mkdir_p_arg`).
///
/// @return [`crate::vim_defs::FAIL`] when the directories could not be
///         created, [`crate::vim_defs::OK`] otherwise - including when
///         `++p` was not given at all.
#[must_use]
pub fn handle_mkdir_p_arg(eap: &crate::ex_cmds_defs::ExargT, fname: &[u8]) -> i32 {
    if eap.mkdir_p && crate::os::fs::os_file_mkdir(fname, 0o755) < 0 {
        return crate::vim_defs::FAIL;
    }
    crate::vim_defs::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn globals_test_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::globals::global_state_test_lock()
    }

    #[cfg(not(unix))]
    #[test]
    fn find_pipe_finds_an_unquoted_bar() {
        assert_eq!(find_pipe(b"sort | uniq"), Some(5));
        assert_eq!(find_pipe(b"|first"), Some(0));
    }

    #[cfg(not(unix))]
    #[test]
    fn find_pipe_is_none_without_a_bar() {
        assert_eq!(find_pipe(b"sort file"), None);
        assert_eq!(find_pipe(b""), None);
    }

    #[cfg(not(unix))]
    #[test]
    fn find_pipe_ignores_a_quoted_bar() {
        // A bar inside a quoted argument is part of that argument, not
        // a command separator.
        assert_eq!(find_pipe(br#"grep "a|b" file"#), None);
        // ...but one after the quotes close still counts.
        assert_eq!(find_pipe(br#"grep "a|b" | wc"#), Some(11));
    }

    #[cfg(not(unix))]
    #[test]
    fn find_pipe_stops_at_an_embedded_nul() {
        // The original scans a NUL-terminated string, so a NUL ends
        // the scan even with more bytes in the slice.
        assert_eq!(find_pipe(b"abc\0|def"), None);
    }

    #[test]
    fn handle_mkdir_p_arg_is_ok_when_the_flag_is_not_set() {
        // Without ++p nothing is created at all, so a path that could
        // never be made is still fine.
        let eap = crate::ex_cmds_defs::ExargT { mkdir_p: false, ..Default::default() };
        assert_eq!(handle_mkdir_p_arg(&eap, b""), crate::vim_defs::OK);
    }

    #[test]
    fn handle_mkdir_p_arg_creates_missing_parents() {
        let dir = std::env::temp_dir().join("nero_mkdir_p_test").join("a").join("b");
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("nero_mkdir_p_test"));
        let target = dir.join("file.txt");

        let eap = crate::ex_cmds_defs::ExargT { mkdir_p: true, ..Default::default() };
        let r = handle_mkdir_p_arg(&eap, target.to_str().unwrap().as_bytes());

        let made = dir.is_dir();
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("nero_mkdir_p_test"));

        assert_eq!(r, crate::vim_defs::OK);
        assert!(made, "the parent directories of the file must exist afterwards");
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
