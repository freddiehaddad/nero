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
//!
//! Also translated: `modifier_len` - the length of a command modifier
//! (e.g. `silent`/`vertical`/`3tab`) at the start of a `:` command
//! line, via the small, self-contained `cmdmods[]` table (mechanically
//! transcribed 1:1). Needed by `ex_eval.c`'s `has_loop_cmd`
//! (`crate::ex_eval`).
//!
//! Also translated: [`current_win_nr`]/[`current_tab_nr`] - the window/
//! tabpage-number-within-the-current-tab-list counters backing the
//! original's own `CURRENT_WIN_NR`/`LAST_WIN_NR`/`CURRENT_TAB_NR`/
//! `LAST_TAB_NR` macros. `FOR_ALL_WINDOWS_IN_TAB(wp, curtab)`'s own
//! `((tp) == curtab) ? firstwin : (tp)->tp_firstwin` ternary always
//! resolves to `firstwin` here specifically (this function's own body
//! always passes `curtab` as `tp`), matching `window.rs`'s own
//! `win_count`'s exact traversal start point. Also
//! [`get_pressedreturn`]/[`set_pressedreturn`] - a trivial getter/
//! setter pair over a new `EX_PRESSEDRETURN` file-static (matching the
//! original's own file-static `ex_pressedreturn`), starting `false`
//! like the original.
//!
//! Also translated: [`checkforcmd`] - checks whether a command-line
//! position starts with (at least) a minimum-length abbreviation of a
//! full command name (e.g. `:sil`/`:silent` both matching `"silent"`
//! with `minlen == 3`), needed by `parse_command_modifiers`'s (not yet
//! translated) modifier-keyword recognition and `eval/userfunc.c`'s
//! `get_function_body`. Needed only `charset.rs`'s `skipwhite` and
//! `macros_defs.rs`'s `ascii_isalpha`, both already real. Returns
//! `Option<usize>`, collapsing the original's own `bool` return +
//! `char **pp` in-out pointer, matching [`check_nextcmd`]'s own
//! identical precedent.
//!
//! Also translated: [`not_exiting`] - restores `GLOBALS.exiting` after
//! a failed quit attempt and clears `v:exitreason` back to unset,
//! needed only the already-real `GLOBALS.exiting` field (its first
//! real reader/writer) and `eval::vars::set_vim_var_string`. Its own
//! callers (`before_quit_autocmds`/`before_quit_all`/`ex_cmds.c`'s
//! quit-related logic) remain untranslated, matching this crate's
//! established "small, simple, mechanically correct piece ahead of
//! its real caller" precedent.

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

/// Check whether `p` starts with (at least) `len` characters of the
/// full command name `cmd`, followed by a non-alphabetic byte (a
/// genuine command-name boundary, not a longer, different word merely
/// sharing the same prefix) (`checkforcmd`).
///
/// Returns the offset within `p`, after skipping any trailing
/// whitespace, of the byte right after the match - or `None` if `p`
/// doesn't match. Collapses the original's own `bool` return + `char
/// **pp` in-out pointer into a single `Option<usize>`, matching this
/// crate's established "offset, not pointer" idiom for this exact
/// class of C string-scanning function ([`check_nextcmd`]'s own
/// identical precedent). Running off the end of `p` is treated the
/// same as hitting the original's own C-string `NUL` terminator
/// (matching [`modifier_len`]'s own identical `p.get(j)` idiom): it is
/// never alphabetic, so a `p` that ends exactly at the matched prefix
/// (e.g. `p == b"sil"` against `cmd == b"silent"`) still counts as a
/// full match.
#[must_use]
pub fn checkforcmd(p: &[u8], cmd: &[u8], len: usize) -> Option<usize> {
    let mut i = 0;
    while i < cmd.len() {
        if p.get(i) != Some(&cmd[i]) {
            break;
        }
        i += 1;
    }
    if i >= len && !p.get(i).is_some_and(|&c| crate::macros_defs::ascii_isalpha(i32::from(c))) {
        Some(i + crate::charset::skipwhite(&p[i..]))
    } else {
        None
    }
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

/// A single entry in [`CMDMODS`] (`struct cmdmod` - an internal,
/// file-static type in the original, not to be confused with the
/// public `cmdmod_T`/`CmdmodT` from `ex_cmds_defs.h`).
struct CmdMod {
    /// Full modifier name (`name`).
    name: &'static [u8],
    /// Minimum number of leading characters that must match for this
    /// to count as an abbreviation of `name` (`minlen`).
    minlen: usize,
    /// Whether an optional leading count is accepted (e.g. `:3tab`,
    /// `:123verbose`) (`has_count`).
    has_count: bool,
}

/// The command-modifier name table (`cmdmods[]`), mechanically
/// transcribed from the original 1:1 (same order, same literal
/// text/lengths).
const CMDMODS: &[CmdMod] = &[
    CmdMod { name: b"aboveleft", minlen: 3, has_count: false },
    CmdMod { name: b"belowright", minlen: 3, has_count: false },
    CmdMod { name: b"botright", minlen: 2, has_count: false },
    CmdMod { name: b"browse", minlen: 3, has_count: false },
    CmdMod { name: b"confirm", minlen: 4, has_count: false },
    CmdMod { name: b"filter", minlen: 4, has_count: false },
    CmdMod { name: b"hide", minlen: 3, has_count: false },
    CmdMod { name: b"horizontal", minlen: 3, has_count: false },
    CmdMod { name: b"keepalt", minlen: 5, has_count: false },
    CmdMod { name: b"keepjumps", minlen: 5, has_count: false },
    CmdMod { name: b"keepmarks", minlen: 3, has_count: false },
    CmdMod { name: b"keeppatterns", minlen: 5, has_count: false },
    CmdMod { name: b"leftabove", minlen: 5, has_count: false },
    CmdMod { name: b"lockmarks", minlen: 3, has_count: false },
    CmdMod { name: b"noautocmd", minlen: 3, has_count: false },
    CmdMod { name: b"noswapfile", minlen: 3, has_count: false },
    CmdMod { name: b"rightbelow", minlen: 6, has_count: false },
    CmdMod { name: b"sandbox", minlen: 3, has_count: false },
    CmdMod { name: b"silent", minlen: 3, has_count: false },
    CmdMod { name: b"tab", minlen: 3, has_count: true },
    CmdMod { name: b"topleft", minlen: 2, has_count: false },
    CmdMod { name: b"unsilent", minlen: 3, has_count: false },
    CmdMod { name: b"verbose", minlen: 4, has_count: true },
    CmdMod { name: b"vertical", minlen: 4, has_count: false },
];

/// Length of a command modifier (including an optional leading count)
/// at the start of `cmd`, or `0` when it isn't one (`modifier_len`).
///
/// Returns a byte length within `cmd` itself (not a pointer), matching
/// this crate's established "offset, not pointer" idiom for this exact
/// class of C string-scanning function.
#[must_use]
pub fn modifier_len(cmd: &[u8]) -> usize {
    // An optional leading count (e.g. the "3" in ":3tab") followed by
    // whitespace - `p_start` is 0 when there's no such count, matching
    // the original's own `p == cmd` check below.
    let mut p_start = 0;
    if cmd.first().is_some_and(|&c| crate::ascii_defs::ascii_isdigit(i32::from(c))) {
        let after_digits = 1 + crate::charset::skipdigits(&cmd[1..]);
        p_start = after_digits + crate::charset::skipwhite(&cmd[after_digits..]);
    }
    let p = &cmd[p_start..];

    for m in CMDMODS {
        // The length of the matching prefix between `p` and `m.name`,
        // capped at `m.name.len()` - `.zip()`'s own "stop at the
        // shorter iterator" behavior already replicates the original's
        // `p[j] != NUL` bound (a Rust slice has no implicit NUL
        // terminator to run past in the first place).
        let j = p.iter().zip(m.name.iter()).take_while(|(a, b)| a == b).count();
        if j >= m.minlen
            && !p.get(j).is_some_and(|&c| crate::macros_defs::ascii_isalpha(i32::from(c)))
            && (p_start == 0 || m.has_count)
        {
            return j + p_start;
        }
    }
    0
}

/// Returns the window number of `win` within the current tab page, or
/// the total number of windows if `win` is null (`current_win_nr`).
///
/// # Safety
/// `crate::globals::GLOBALS.firstwin`'s own `w_next` chain must
/// consist of valid, live `WinT` pointers.
#[must_use]
pub unsafe fn current_win_nr(win: *const crate::buffer_defs::WinT) -> i32 {
    let mut nr = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        nr += 1;
        if std::ptr::eq(wp, win) {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    nr
}

/// Returns the tabpage number of `tab` within the global tabpage list,
/// or the total number of tabpages if `tab` is null (`current_tab_nr`).
///
/// # Safety
/// `crate::globals::GLOBALS.first_tabpage`'s own `tp_next` chain must
/// consist of valid, live `TabpageT` pointers.
#[must_use]
pub unsafe fn current_tab_nr(tab: *const crate::buffer_defs::TabpageT) -> i32 {
    let mut nr = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        nr += 1;
        if std::ptr::eq(tp, tab) {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    nr
}

/// `ex_pressedreturn` - whether the last Ex command was entered as an
/// empty command line (pressing `<CR>` at the `:` prompt), file-static
/// in the original. Starts `false`, matching the original.
static EX_PRESSEDRETURN: crate::globals::GlobalCell<bool> = crate::globals::GlobalCell::new(false);

/// Returns whether the last Ex command was an empty command line
/// (`get_pressedreturn`).
#[must_use]
pub fn get_pressedreturn() -> bool {
    // SAFETY: momentary read.
    unsafe { *EX_PRESSEDRETURN.get_mut() }
}

/// Sets whether the last Ex command was an empty command line
/// (`set_pressedreturn`).
pub fn set_pressedreturn(val: bool) {
    // SAFETY: momentary write.
    unsafe { *EX_PRESSEDRETURN.get_mut() = val };
}

/// Call this if we thought we were going to exit, but we won't
/// (because of an error). Restores `GLOBALS.exiting` to `save_exiting`
/// and clears `v:exitreason` back to unset (`not_exiting`).
///
/// # Safety
/// Same as [`crate::eval::vars::set_vim_var_string`].
pub unsafe fn not_exiting(save_exiting: bool) {
    // SAFETY: momentary write.
    unsafe { crate::globals::GLOBALS.get_mut() }.exiting = save_exiting;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::vars::set_vim_var_string(crate::eval::vars::VimVarIndex::Exitreason, None) };
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
    fn checkforcmd_minimal_abbreviation_matches() {
        // ":sil" is the minimum abbreviation of "silent" (minlen 3).
        assert_eq!(checkforcmd(b"sil foo", b"silent", 3), Some(4));
    }

    #[test]
    fn checkforcmd_full_word_matches() {
        assert_eq!(checkforcmd(b"silent foo", b"silent", 3), Some(7));
    }

    #[test]
    fn checkforcmd_below_minlen_does_not_match() {
        // "si" is only 2 characters - below the required minlen of 3.
        assert_eq!(checkforcmd(b"si foo", b"silent", 3), None);
    }

    #[test]
    fn checkforcmd_wrong_continuation_does_not_match() {
        // "silx" shares "sil" with "silent" but continues with a
        // different letter - not a valid abbreviation of "silent".
        assert_eq!(checkforcmd(b"silx foo", b"silent", 3), None);
    }

    #[test]
    fn checkforcmd_no_match_at_all() {
        assert_eq!(checkforcmd(b"vertical foo", b"silent", 3), None);
    }

    #[test]
    fn checkforcmd_matches_when_input_ends_exactly_at_minlen() {
        // Running off the end of `p` counts the same as the original's
        // own C-string NUL terminator - not alphabetic.
        assert_eq!(checkforcmd(b"sil", b"silent", 3), Some(3));
    }

    #[test]
    fn checkforcmd_skips_multiple_trailing_spaces() {
        assert_eq!(checkforcmd(b"sil   foo", b"silent", 3), Some(6));
    }

    #[test]
    fn set_ref_in_findfunc_is_always_false_since_ffu_cb_stays_none() {
        // Nothing in this crate can populate FFU_CB with a real
        // callback yet (needs option_set_callback_func) - it always
        // stays Callback::None, matching a real, unconfigured session.
        assert!(!unsafe { set_ref_in_findfunc(1) });
    }

    #[test]
    fn modifier_len_full_name_before_a_space() {
        assert_eq!(modifier_len(b"silent echo 1"), 6);
    }

    #[test]
    fn modifier_len_abbreviated_to_its_minlen() {
        assert_eq!(modifier_len(b"sil echo 1"), 3);
    }

    #[test]
    fn modifier_len_shorter_than_minlen_is_not_recognized() {
        assert_eq!(modifier_len(b"si echo 1"), 0);
    }

    #[test]
    fn modifier_len_zero_for_a_plain_command() {
        assert_eq!(modifier_len(b"echo 1"), 0);
    }

    #[test]
    fn modifier_len_followed_by_more_alpha_chars_is_not_a_match() {
        // "silentx" isn't "silent" followed by a non-alpha boundary -
        // the whole prefix must be immediately followed by something
        // that isn't itself a letter (e.g. space, end of input, digit).
        assert_eq!(modifier_len(b"silentx"), 0);
    }

    #[test]
    fn modifier_len_at_the_very_end_of_the_slice() {
        // No trailing byte at all after "silent" - out-of-bounds reads
        // must behave like the original's own implicit NUL check.
        assert_eq!(modifier_len(b"silent"), 6);
    }

    #[test]
    fn modifier_len_with_count_on_a_count_capable_modifier() {
        assert_eq!(modifier_len(b"3tab split"), 4);
        assert_eq!(modifier_len(b"123verbose echo 1"), 10);
    }

    #[test]
    fn modifier_len_with_count_on_a_non_count_capable_modifier_is_rejected() {
        // 'silent' has no count support - "3silent" must not match it.
        assert_eq!(modifier_len(b"3silent echo"), 0);
    }

    #[test]
    fn modifier_len_two_word_modifier_names() {
        assert_eq!(modifier_len(b"belowright split"), 10);
        assert_eq!(modifier_len(b"rightbelow split"), 10);
    }

    // --- current_win_nr / current_tab_nr ---

    struct FirstwinGuard {
        previous: *mut crate::buffer_defs::WinT,
    }

    impl FirstwinGuard {
        fn set(head: *mut crate::buffer_defs::WinT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
            unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = head;
            FirstwinGuard { previous }
        }
    }

    impl Drop for FirstwinGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = self.previous;
        }
    }

    struct FirstTabpageGuard {
        previous: *mut crate::buffer_defs::TabpageT,
    }

    impl FirstTabpageGuard {
        fn set(head: *mut crate::buffer_defs::TabpageT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
            unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = head;
            FirstTabpageGuard { previous }
        }
    }

    impl Drop for FirstTabpageGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = self.previous;
        }
    }

    #[test]
    fn current_win_nr_null_counts_every_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = crate::buffer_defs::WinT::default();
        let mut second =
            crate::buffer_defs::WinT { w_next: &mut third as *mut _, ..Default::default() };
        let mut first =
            crate::buffer_defs::WinT { w_next: &mut second as *mut _, ..Default::default() };
        let _guard = FirstwinGuard::set(&mut first as *mut _);

        assert_eq!(unsafe { current_win_nr(std::ptr::null()) }, 3);
    }

    #[test]
    fn current_win_nr_zero_for_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = FirstwinGuard::set(std::ptr::null_mut());
        assert_eq!(unsafe { current_win_nr(std::ptr::null()) }, 0);
    }

    #[test]
    fn current_win_nr_stops_at_the_matching_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = crate::buffer_defs::WinT::default();
        let mut second =
            crate::buffer_defs::WinT { w_next: &mut third as *mut _, ..Default::default() };
        let mut first =
            crate::buffer_defs::WinT { w_next: &mut second as *mut _, ..Default::default() };
        let _guard = FirstwinGuard::set(&mut first as *mut _);

        assert_eq!(unsafe { current_win_nr(&first as *const _) }, 1);
        assert_eq!(unsafe { current_win_nr(&second as *const _) }, 2);
        assert_eq!(unsafe { current_win_nr(&third as *const _) }, 3);
    }

    #[test]
    fn current_tab_nr_null_counts_every_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = crate::buffer_defs::TabpageT::default();
        let mut second =
            crate::buffer_defs::TabpageT { tp_next: &mut third as *mut _, ..Default::default() };
        let mut first =
            crate::buffer_defs::TabpageT { tp_next: &mut second as *mut _, ..Default::default() };
        let _guard = FirstTabpageGuard::set(&mut first as *mut _);

        assert_eq!(unsafe { current_tab_nr(std::ptr::null()) }, 3);
    }

    #[test]
    fn current_tab_nr_stops_at_the_matching_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = crate::buffer_defs::TabpageT::default();
        let mut first =
            crate::buffer_defs::TabpageT { tp_next: &mut second as *mut _, ..Default::default() };
        let _guard = FirstTabpageGuard::set(&mut first as *mut _);

        assert_eq!(unsafe { current_tab_nr(&first as *const _) }, 1);
        assert_eq!(unsafe { current_tab_nr(&second as *const _) }, 2);
    }

    // --- get_pressedreturn / set_pressedreturn ---

    #[test]
    fn pressedreturn_starts_false_and_round_trips() {
        let _lock = crate::globals::global_state_test_lock();
        set_pressedreturn(false);
        assert!(!get_pressedreturn());
        set_pressedreturn(true);
        assert!(get_pressedreturn());
        set_pressedreturn(false);
        assert!(!get_pressedreturn());
    }

    // --- not_exiting ---

    #[test]
    fn not_exiting_restores_globals_exiting_to_the_saved_value() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.exiting = true;
        unsafe { not_exiting(false) };
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.exiting);

        unsafe { crate::globals::GLOBALS.get_mut() }.exiting = false;
        unsafe { not_exiting(true) };
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.exiting);

        // Leave GLOBALS.exiting false for other tests sharing this
        // process-wide state.
        unsafe { crate::globals::GLOBALS.get_mut() }.exiting = false;
    }

    #[test]
    fn not_exiting_clears_v_exitreason() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            crate::eval::vars::set_vim_var_string(
                crate::eval::vars::VimVarIndex::Exitreason,
                Some(b"quit"),
            )
        };
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_str(crate::eval::vars::VimVarIndex::Exitreason) },
            b"quit"
        );

        unsafe { not_exiting(false) };

        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_str(crate::eval::vars::VimVarIndex::Exitreason) },
            Vec::<u8>::new()
        );
    }
}
