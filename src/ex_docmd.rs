//! Translated from `src/nvim/ex_docmd.c` (partial - a tiny, deliberate
//! harvest of a few small, self-contained functions).
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
//!
//! Also translated: `ends_excmd`/`check_nextcmd`/`find_nextcmd` - tiny,
//! self-contained "where does this Ex command's text end" helpers,
//! needed by `eval.c`'s `eval0` (`crate::eval::eval`). Modeled on this
//! crate's established "return a byte offset, not a pointer" idiom
//! (matching `eval_number`/`eval7_leader`'s own convention in
//! `eval/eval.rs`): a `NUL` byte (the original's own C-string
//! terminator) is represented here as simply running past the end of
//! the given slice, so `ends_excmd`/`find_nextcmd`/`check_nextcmd` all
//! take a plain `&[u8]`/`u8` rather than a nullable pointer.

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

/// Return `true` if `c` ends an Ex command: a `NUL` (matched here by
/// callers passing `0` for "ran off the end of the slice"), `|`, `"`
/// (start of a comment), or `\n` (`ends_excmd`).
#[must_use]
pub fn ends_excmd(c: u8) -> bool {
    c == 0 || c == b'|' || c == b'"' || c == b'\n'
}

/// Return the offset, within `p`, of the character right after the
/// first `|` or `\n`, or `None` if neither is found before the end of
/// `p` (`find_nextcmd`).
#[must_use]
pub fn find_nextcmd(p: &[u8]) -> Option<usize> {
    let mut i = 0;
    loop {
        match p.get(i)? {
            b'|' | b'\n' => return Some(i + 1),
            _ => i += 1,
        }
    }
}

/// Check whether the position at offset `pos` in `p` (after skipping
/// whitespace) is a separator between Ex commands.
///
/// Returns the offset of the character right after the separator, or
/// `None` if it isn't one (`check_nextcmd`).
#[must_use]
pub fn check_nextcmd(p: &[u8]) -> Option<usize> {
    let s = crate::charset::skipwhite(p);
    match p.get(s) {
        Some(&b'|') | Some(&b'\n') => Some(s + 1),
        _ => None,
    }
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

    #[test]
    fn ends_excmd_recognizes_all_4_terminators() {
        assert!(ends_excmd(0));
        assert!(ends_excmd(b'|'));
        assert!(ends_excmd(b'"'));
        assert!(ends_excmd(b'\n'));
    }

    #[test]
    fn ends_excmd_false_for_ordinary_bytes() {
        assert!(!ends_excmd(b'x'));
        assert!(!ends_excmd(b' '));
        assert!(!ends_excmd(b'\''));
    }

    #[test]
    fn find_nextcmd_finds_pipe() {
        assert_eq!(find_nextcmd(b"echo 1|echo 2"), Some(7));
    }

    #[test]
    fn find_nextcmd_finds_newline() {
        assert_eq!(find_nextcmd(b"echo 1\necho 2"), Some(7));
    }

    #[test]
    fn find_nextcmd_none_when_absent() {
        assert_eq!(find_nextcmd(b"echo 1"), None);
    }

    #[test]
    fn check_nextcmd_none_when_not_a_separator() {
        assert_eq!(check_nextcmd(b"echo 1"), None);
        assert_eq!(check_nextcmd(b""), None);
    }

    #[test]
    fn check_nextcmd_skips_whitespace_then_finds_pipe() {
        assert_eq!(check_nextcmd(b"  | echo 2"), Some(3));
    }

    #[test]
    fn check_nextcmd_finds_newline_with_no_leading_whitespace() {
        assert_eq!(check_nextcmd(b"\necho 2"), Some(1));
    }
}
