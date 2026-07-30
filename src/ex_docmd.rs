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
//!
//! Also translated: `find_cmdline_var` - a small, self-contained
//! `spec_str[]` table lookup (`%`/`#`/`<cword>`/`<sfile>`/etc.), needed
//! by `strings.c`'s `vim_strsave_shellescape()` (`shellescape()`).
//!
//! Also translated: `set_ref_in_findfunc` - marks the global
//! `'findfunc'` callback (`ffu_cb`) with a GC `copy_id` so it survives
//! garbage collection. `ffu_cb` stays `Callback::None` forever today
//! (see `FFU_CB`'s own doc comment) - matches every real,
//! unconfigured session.

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

/// A special cmdline variable recognized by [`find_cmdline_var`]
/// (`SPEC_*`, `ex_docmd.c`). `Client` (`SPEC_CLIENT`) is commented out
/// in the original itself and not modeled here either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdlineSpecialVar {
    /// `%` - current file name (`SPEC_PERC`).
    Perc,
    /// `#` - alternate file name (`SPEC_HASH`).
    Hash,
    /// `<cword>` - cursor word (`SPEC_CWORD`).
    Cword,
    /// `<cWORD>` - cursor WORD (`SPEC_CCWORD`).
    Ccword,
    /// `<cexpr>` - expr under cursor (`SPEC_CEXPR`).
    Cexpr,
    /// `<cfile>` - cursor path name (`SPEC_CFILE`).
    Cfile,
    /// `<sfile>` - `:so` file name (`SPEC_SFILE`).
    Sfile,
    /// `<slnum>` - `:so` file line number (`SPEC_SLNUM`).
    Slnum,
    /// `<stack>` - call stack (`SPEC_STACK`).
    Stack,
    /// `<script>` - script file name (`SPEC_SCRIPT`).
    Script,
    /// `<afile>` - autocommand file name (`SPEC_AFILE`).
    Afile,
    /// `<abuf>` - autocommand buffer number (`SPEC_ABUF`).
    Abuf,
    /// `<amatch>` - autocommand match name (`SPEC_AMATCH`).
    Amatch,
    /// `<sflnum>` - script file line number (`SPEC_SFLNUM`).
    Sflnum,
    /// `<SID>` - script ID, `<SNR>123_` (`SPEC_SID`).
    Sid,
}

/// The exact string each [`CmdlineSpecialVar`] variant matches,
/// mirroring `find_cmdline_var`'s own `spec_str[]` table 1:1 (same
/// order, same literal text).
const CMDLINE_SPEC_STRS: &[(CmdlineSpecialVar, &[u8])] = &[
    (CmdlineSpecialVar::Perc, b"%"),
    (CmdlineSpecialVar::Hash, b"#"),
    (CmdlineSpecialVar::Cword, b"<cword>"),
    (CmdlineSpecialVar::Ccword, b"<cWORD>"),
    (CmdlineSpecialVar::Cexpr, b"<cexpr>"),
    (CmdlineSpecialVar::Cfile, b"<cfile>"),
    (CmdlineSpecialVar::Sfile, b"<sfile>"),
    (CmdlineSpecialVar::Slnum, b"<slnum>"),
    (CmdlineSpecialVar::Stack, b"<stack>"),
    (CmdlineSpecialVar::Script, b"<script>"),
    (CmdlineSpecialVar::Afile, b"<afile>"),
    (CmdlineSpecialVar::Abuf, b"<abuf>"),
    (CmdlineSpecialVar::Amatch, b"<amatch>"),
    (CmdlineSpecialVar::Sflnum, b"<sflnum>"),
    (CmdlineSpecialVar::Sid, b"<SID>"),
];

/// Check whether `src` starts with a special cmdline variable (`%`,
/// `#`, `<cword>`, `<sfile>`, etc.) - if so, returns which one and how
/// many bytes it occupies; `None` otherwise (`find_cmdline_var`).
#[must_use]
pub fn find_cmdline_var(src: &[u8]) -> Option<(CmdlineSpecialVar, usize)> {
    for &(spec, s) in CMDLINE_SPEC_STRS {
        if src.starts_with(s) {
            return Some((spec, s.len()));
        }
    }
    None
}

/// The `'findfunc'` callback (`ffu_cb`, a file-static `Callback`).
/// Nothing in this crate can currently set a real value here - see
/// `ops.rs`'s `OPFUNC_CB` for the identical reasoning (needs
/// `option_set_callback_func`, not translated).
static FFU_CB: crate::globals::GlobalCell<crate::eval::typval_defs::Callback> =
    crate::globals::GlobalCell::new(crate::eval::typval_defs::Callback::None);

/// Mark the global `'findfunc'` callback with `copy_id` so that it is
/// not garbage collected (`set_ref_in_findfunc`).
///
/// # Safety
/// Same as [`crate::eval::eval::set_ref_in_callback`].
pub unsafe fn set_ref_in_findfunc(copy_id: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let cb = unsafe { &*FFU_CB.as_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::eval::set_ref_in_callback(cb, copy_id, std::ptr::null_mut(), std::ptr::null_mut()) }
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

    #[test]
    fn find_cmdline_var_recognizes_perc_and_hash() {
        assert_eq!(find_cmdline_var(b"%rest"), Some((CmdlineSpecialVar::Perc, 1)));
        assert_eq!(find_cmdline_var(b"#rest"), Some((CmdlineSpecialVar::Hash, 1)));
    }

    #[test]
    fn find_cmdline_var_recognizes_angle_bracket_forms() {
        assert_eq!(find_cmdline_var(b"<cword> rest"), Some((CmdlineSpecialVar::Cword, 7)));
        assert_eq!(find_cmdline_var(b"<cWORD> rest"), Some((CmdlineSpecialVar::Ccword, 7)));
        assert_eq!(find_cmdline_var(b"<sfile>"), Some((CmdlineSpecialVar::Sfile, 7)));
        assert_eq!(find_cmdline_var(b"<SID>"), Some((CmdlineSpecialVar::Sid, 5)));
    }

    #[test]
    fn find_cmdline_var_none_for_plain_text() {
        assert_eq!(find_cmdline_var(b"plain text"), None);
        assert_eq!(find_cmdline_var(b""), None);
        assert_eq!(find_cmdline_var(b"<unknown>"), None);
    }

    #[test]
    fn find_cmdline_var_prefix_match_only_checks_the_start() {
        // "<cword>" is a real prefix of "<cwordXYZ>" - starts_with
        // matches regardless of what follows.
        assert_eq!(find_cmdline_var(b"<cword>XYZ"), Some((CmdlineSpecialVar::Cword, 7)));
    }

    #[test]
    fn set_ref_in_findfunc_is_always_false_since_ffu_cb_stays_none() {
        // Nothing in this crate can populate FFU_CB with a real
        // callback yet (needs option_set_callback_func) - it always
        // stays Callback::None, matching a real, unconfigured session.
        assert!(!unsafe { set_ref_in_findfunc(1) });
    }
}
