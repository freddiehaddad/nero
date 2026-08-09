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
//!
//! Also translated: [`SaveStateT`] (`save_state_T`) +
//! [`save_current_state`]/[`restore_current_state`] - save/restore the
//! current State/typeahead/operator-pending state around a temporary
//! switch to Normal mode (e.g. `:normal`). Needed only already-real
//! `GLOBALS` fields (`msg_scroll`/`restart_edit`/`msg_didout`/`State`/
//! `finish_op`/`opcount`/`reg_executing`/`pending_end_reg_executing`/
//! `force_restart_edit`) plus `input.c`'s newly-translated
//! `save_typeahead`/`restore_typeahead` (`crate::input`). The
//! original's own `ui_cursor_shape()` call in `restore_current_state`
//! is omitted (a pure UI-redraw hint, see that function's own doc
//! comment). Translated ahead of their real callers (`exec_normal` in
//! this same file, and `menu.c`'s `ex_emenu`-adjacent code - neither
//! translated yet), matching this crate's established "small, simple,
//! mechanically correct piece ahead of its real caller" precedent.
//!
//! Also translated: [`set_no_hlsearch`] - toggles the "temporarily
//! don't highlight search matches" flag and keeps `v:hlsearch` in
//! sync, needing only the already-real `GLOBALS.Search.no_hlsearch`/
//! `OPTION_VARS.p_hls`/`eval::vars::set_vim_var_nr`. Also
//! [`ex_nohlsearch`] (`:nohlsearch`) - a genuine, complete Ex-command
//! handler (not just a helper ahead of one), matching this crate's
//! established `mark.rs`'s `ex_clearjumps` precedent for translating a
//! real `ex_*` handler ahead of the still-unpopulated `cmdnames[]`
//! dispatch table. Its own `redraw_all_later(UPD_SOME_VALID)` call is
//! omitted (pure redraw scheduling, matching this crate's established
//! `redraw_later`-omission precedent).

use crate::buffer_defs::b_flags;

/// `:fold` - create a fold over the command's own line range
/// (`ex_fold`).
///
/// A no-op unless manual folding is currently allowed, which is what
/// keeps `:fold` from fighting an automatic `'foldmethod'`.
///
/// The original builds its two positions with column 1; that column
/// is never read by `fold_create` (only `lnum` is), so it is
/// reproduced rather than relied upon.
///
/// # Safety
/// Forwarded from [`crate::fold::fold_manual_allowed`]/
/// [`crate::fold::fold_create`]'s own safety docs;
/// `GLOBALS.curwin` must be valid and non-null.
pub unsafe fn ex_fold(eap: &crate::ex_cmds_defs::ExargT) {
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { crate::fold::fold_manual_allowed(true) } {
        return;
    }
    let start = crate::pos_defs::PosT { lnum: eap.line1, col: 1, coladd: 0 };
    let end = crate::pos_defs::PosT { lnum: eap.line2, col: 1, coladd: 0 };
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::fold::fold_create(curwin, start, end) };
}

/// `:foldopen` and `:foldclose` (`ex_foldopen`).
///
/// Which of the two is being run is decided by the command index, and
/// `!` makes the operation recursive.
///
/// # Safety
/// Forwarded from [`crate::fold::op_fold_range`]'s own safety doc.
pub unsafe fn ex_foldopen(eap: &crate::ex_cmds_defs::ExargT) {
    let start = crate::pos_defs::PosT { lnum: eap.line1, col: 1, coladd: 0 };
    let end = crate::pos_defs::PosT { lnum: eap.line2, col: 1, coladd: 0 };
    let opening = eap.cmdidx == crate::ex_cmds_defs::CmdIdxT::foldopen;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::fold::op_fold_range(start, end, opening, eap.forceit) };
}

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

/// Apply a numeric count to `eap`'s address range (`set_cmd_count`).
///
/// For a non-line address type (`:buffer 2`, `:sleep 3`) the count IS
/// the address. For a line range the count instead extends the range
/// forwards from `line2`, saturating at `i32::MAX` rather than
/// overflowing. With `validate` the end is clamped to the buffer's
/// last line - silently, since the original notes vi gives no error
/// for an out-of-range count.
///
/// # Safety
/// When `validate` is true, `crate::globals::GLOBALS.curbuf` must be
/// a valid, non-null pointer to a live `BufT`.
pub unsafe fn set_cmd_count(
    eap: &mut crate::ex_cmds_defs::ExargT,
    count: crate::pos_defs::LinenrT,
    validate: bool,
) {
    if eap.addr_type != crate::ex_cmds_defs::CmdAddrT::Lines {
        // e.g. :buffer 2, :sleep 3
        eap.line2 = count;
        if eap.addr_count == 0 {
            eap.addr_count = 1;
        }
        return;
    }

    eap.line1 = eap.line2;
    if eap.line2 >= i32::MAX - (count - 1) {
        eap.line2 = i32::MAX;
    } else {
        eap.line2 += count - 1;
    }
    eap.addr_count += 1;
    // Be vi compatible: no error message for out of range.
    if validate {
        // SAFETY: forwarded from this function's own safety doc.
        let line_count = unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_ml.ml_line_count };
        if eap.line2 > line_count {
            eap.line2 = line_count;
        }
    }
}

/// Recognise the two Ex commands whose names may be abbreviated to a
/// single letter, `:k` and `:s` (`one_letter_cmd`).
///
/// Returns the command index when `p` starts with such an
/// abbreviation, otherwise `None`. The tangled conditions exist to
/// avoid stealing the prefixes of longer commands: `:kee[pmarks]`
/// must not become `:k`, and `:scs`/`:scr`/`:sim`/`:sil`/`:sig`/
/// `:sre` must not become `:s`.
///
/// The original indexes freely past the matched bytes, relying on the
/// C string's own NUL terminator to stop it; this reads out-of-range
/// positions as `0` instead, which is the same value that comparison
/// sees.
#[must_use]
pub fn one_letter_cmd(p: &[u8]) -> Option<crate::ex_cmds_defs::CmdIdxT> {
    let at = |i: usize| p.get(i).copied().unwrap_or(0);

    if at(0) == b'k' && (at(1) != b'e' || at(2) != b'e') {
        return Some(crate::ex_cmds_defs::CmdIdxT::k);
    }
    if at(0) == b's'
        && ((at(1) == b'c'
            && (at(2) == 0
                || (at(2) != b's'
                    && at(2) != b'r'
                    && (at(3) == 0 || (at(3) != b'i' && at(4) != b'p')))))
            || at(1) == b'g'
            || (at(1) == b'i' && at(2) != b'm' && at(2) != b'l' && at(2) != b'g')
            || at(1) == b'I'
            || (at(1) == b'r' && at(2) != b'e'))
    {
        return Some(crate::ex_cmds_defs::CmdIdxT::substitute);
    }
    None
}

/// Skip leading colons (and the whitespace around them) at the start
/// of an Ex command line, returning the byte offset just past them
/// (`skip_colon_white`).
///
/// The original takes and returns a pointer; this returns an index
/// instead, matching this crate's established index-in-place-of-
/// pointer idiom.
#[must_use]
pub fn skip_colon_white(p: &[u8], skipleadingwhite: bool) -> usize {
    let mut i = if skipleadingwhite { crate::charset::skipwhite(p) } else { 0 };

    while p.get(i) == Some(&b':') {
        i += 1;
        i += crate::charset::skipwhite(&p[i..]);
    }

    i
}

/// Whether `p` starts with a command-modifying `!`, consuming it if so
/// (`parse_bang`).
///
/// `:substitute` and its `:smagic`/`:snomagic` siblings are excluded:
/// for those a `!` is part of the pattern, not a bang modifier.
///
/// Returns `(found, consumed)` rather than advancing a `char **p`
/// out-parameter.
#[must_use]
pub fn parse_bang(cmdidx: crate::ex_cmds_defs::CmdIdxT, p: &[u8]) -> (bool, usize) {
    use crate::ex_cmds_defs::CmdIdxT;
    if p.first() == Some(&b'!')
        && cmdidx != CmdIdxT::substitute
        && cmdidx != CmdIdxT::smagic
        && cmdidx != CmdIdxT::snomagic
    {
        return (true, 1);
    }
    (false, 0)
}

/// Whether the command expects expression arguments needing special
/// parsing (`cmd_has_expr_args`).
#[must_use]
pub fn cmd_has_expr_args(cmdidx: crate::ex_cmds_defs::CmdIdxT) -> bool {
    use crate::ex_cmds_defs::CmdIdxT;
    matches!(
        cmdidx,
        CmdIdxT::execute
            | CmdIdxT::echo
            | CmdIdxT::echon
            | CmdIdxT::echomsg
            | CmdIdxT::echoerr
    )
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

/// Structure used to save the current state - used when executing
/// Normal mode commands while in any other mode (`save_state_T`).
#[derive(Debug, Clone, Default)]
pub struct SaveStateT {
    pub save_msg_scroll: i32,
    pub save_restart_edit: i32,
    pub save_msg_didout: bool,
    /// the original's own `save_State` (matching `GLOBALS.State`'s
    /// own preserved capitalization; this field itself is just a
    /// plain copy holder, not a distinctly-recognized identifier, so
    /// it uses ordinary snake_case).
    pub save_state: i32,
    pub save_finish_op: bool,
    pub save_opcount: i32,
    pub save_reg_executing: i32,
    pub save_pending_end_reg_executing: bool,
    pub tabuf: crate::input_defs::TasaveT,
}

/// Save the current State and go to Normal mode (`save_current_state`).
///
/// Returns whether the typeahead could be saved - forwarded from
/// `sst.tabuf.typebuf_valid`, which [`crate::input::save_typeahead`]
/// always sets `true` today (there is no failure path in the current
/// implementation, matching the original exactly).
///
/// # Safety
/// `crate::globals::GLOBALS` must be in a consistent state, matching
/// every other direct `GLOBALS` accessor in this crate.
pub unsafe fn save_current_state(sst: &mut SaveStateT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    sst.save_msg_scroll = g.msg_scroll;
    sst.save_restart_edit = g.restart_edit;
    sst.save_msg_didout = g.msg_didout;
    sst.save_state = g.State;
    sst.save_finish_op = g.finish_op;
    sst.save_opcount = g.opcount;
    sst.save_reg_executing = g.reg_executing;
    sst.save_pending_end_reg_executing = g.pending_end_reg_executing;

    g.msg_scroll = 0; // no msg scrolling in Normal mode
    g.restart_edit = 0; // don't go to Insert mode

    // Save the current typeahead. This is required to allow using
    // ":normal" from an event handler and makes sure we don't hang
    // when the argument ends with half a command.
    crate::input::save_typeahead(&mut sst.tabuf);
    sst.tabuf.typebuf_valid
}

/// Restore the state saved by [`save_current_state`]
/// (`restore_current_state`).
///
/// The original's own `ui_cursor_shape()` call (may update the cursor
/// shape and/or handle a cursor now concealed/unconcealed) is omitted:
/// it is a pure UI-redraw hint
/// (`ui_cursor_shape_no_check_conceal`/`conceal_check_cursor_line`,
/// both deep in the not-yet-translated rendering/UI-dispatch
/// subsystem) with no effect on any state this crate currently models,
/// matching this crate's established `redraw_later`-omission
/// precedent.
///
/// # Safety
/// Same as [`save_current_state`].
pub unsafe fn restore_current_state(sst: &mut SaveStateT) {
    // Restore the previous typeahead.
    crate::input::restore_typeahead(&mut sst.tabuf);

    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    g.msg_scroll = sst.save_msg_scroll;
    if g.force_restart_edit {
        g.force_restart_edit = false;
    } else {
        // Some function (terminal_enter()) was aware of ex_normal and
        // decided to override the value of restart_edit anyway.
        g.restart_edit = sst.save_restart_edit;
    }
    g.finish_op = sst.save_finish_op;
    g.opcount = sst.save_opcount;
    g.reg_executing = sst.save_reg_executing;
    g.pending_end_reg_executing = sst.save_pending_end_reg_executing;

    // don't reset msg_didout now
    g.msg_didout |= sst.save_msg_didout;

    // Restore the state (needed when called from a function executed
    // for 'indentexpr'). Update the mouse and cursor, they may have
    // changed.
    g.State = sst.save_state;
}

/// Set (or clear) the "temporarily don't highlight search matches"
/// flag, and keep `v:hlsearch` in sync with it (`set_no_hlsearch`).
///
/// # Safety
/// Forwarded from [`crate::eval::vars::set_vim_var_nr`]'s own safety
/// doc.
pub unsafe fn set_no_hlsearch(flag: bool) {
    // SAFETY: momentary write.
    unsafe { crate::globals::GLOBALS.get_mut() }.Search.no_hlsearch = flag;
    // SAFETY: momentary read.
    let no_hlsearch = unsafe { crate::globals::GLOBALS.get_mut() }.Search.no_hlsearch;
    // SAFETY: momentary read.
    let p_hls = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hls;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::eval::vars::set_vim_var_nr(
            crate::eval::vars::VimVarIndex::Hlsearch,
            i64::from(!no_hlsearch && p_hls != 0),
        )
    };
}

/// `:nohlsearch` - temporarily disable search-match highlighting until
/// the next search (`ex_nohlsearch`).
///
/// The original's own `redraw_all_later(UPD_SOME_VALID)` call is
/// omitted - a pure redraw-scheduling side effect, matching this
/// crate's established `redraw_later`-omission precedent.
///
/// # Safety
/// Same as [`set_no_hlsearch`].
pub unsafe fn ex_nohlsearch(_eap: &crate::ex_cmds_defs::ExargT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_no_hlsearch(true) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    // ---- ex_fold / ex_foldopen ----

    /// Restores `curwin` on drop, even through a panic.
    struct ExCurwinGuard {
        previous: *mut crate::buffer_defs::WinT,
    }

    impl ExCurwinGuard {
        fn set(new_curwin: *mut crate::buffer_defs::WinT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = new_curwin;
            Self { previous }
        }
    }

    impl Drop for ExCurwinGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.previous;
        }
    }

    /// A window with manual folding enabled, matching `fold.rs`'s own
    /// fixture for `fold_create`.
    fn manual_fold_win(buf: &mut BufT) -> crate::buffer_defs::WinT {
        buf.b_ml.ml_line_count = 40;
        crate::buffer_defs::WinT {
            w_buffer: std::ptr::from_mut(buf),
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_fen: 1,
                wo_fdm: Some(b"manual".to_vec()),
                wo_fml: 0,
                wo_fdl: 99,
                ..Default::default()
            },
            w_foldinvalid: false,
            ..Default::default()
        }
    }

    fn fold_eap(
        cmdidx: crate::ex_cmds_defs::CmdIdxT,
        line1: crate::pos_defs::LinenrT,
        line2: crate::pos_defs::LinenrT,
        forceit: bool,
    ) -> crate::ex_cmds_defs::ExargT {
        crate::ex_cmds_defs::ExargT { cmdidx, line1, line2, forceit, ..Default::default() }
    }

    #[test]
    fn ex_fold_creates_a_fold_over_the_command_range() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = manual_fold_win(&mut buf);
        let win_ptr = std::ptr::from_mut(&mut win);
        let _g = ExCurwinGuard::set(win_ptr);

        assert!(win.w_folds.is_empty());
        unsafe { ex_fold(&fold_eap(crate::ex_cmds_defs::CmdIdxT::fold, 5, 10, false)) };

        assert_eq!(win.w_folds.len(), 1);
        assert_eq!(win.w_folds[0].fd_top, 5);
        assert_eq!(win.w_folds[0].fd_len, 6, "lines 5..=10 inclusive");
    }

    /// Manual folding must be ALLOWED first; with an automatic
    /// 'foldmethod' the command is a no-op rather than silently
    /// creating a fold the method would fight over.
    #[test]
    fn ex_fold_is_a_no_op_when_manual_folding_is_not_allowed() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = manual_fold_win(&mut buf);
        // 'foldmethod' is no longer manual/marker.
        win.w_onebuf_opt.wo_fdm = Some(b"indent".to_vec());
        let win_ptr = std::ptr::from_mut(&mut win);
        let _g = ExCurwinGuard::set(win_ptr);

        unsafe { ex_fold(&fold_eap(crate::ex_cmds_defs::CmdIdxT::fold, 5, 10, false)) };
        assert!(win.w_folds.is_empty(), "no fold may be created");
    }

    /// `:foldopen` opens and `:foldclose` closes - the direction comes
    /// from the command index, so an implementation ignoring it would
    /// get one of these backwards.
    #[test]
    fn ex_foldopen_direction_follows_the_command_index() {
        use crate::ex_cmds_defs::CmdIdxT;
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = manual_fold_win(&mut buf);
        let win_ptr = std::ptr::from_mut(&mut win);
        let _g = ExCurwinGuard::set(win_ptr);

        // Build a real fold to act on, then confirm each command
        // moves it in its own direction.
        unsafe { ex_fold(&fold_eap(CmdIdxT::fold, 5, 10, false)) };
        assert_eq!(win.w_folds.len(), 1);

        unsafe { ex_foldopen(&fold_eap(CmdIdxT::foldopen, 5, 10, false)) };
        assert_eq!(
            win.w_folds[0].fd_flags,
            crate::fold::fd_flags::FD_OPEN,
            ":foldopen must open"
        );

        unsafe { ex_foldopen(&fold_eap(CmdIdxT::foldclose, 5, 10, false)) };
        assert_eq!(
            win.w_folds[0].fd_flags,
            crate::fold::fd_flags::FD_CLOSED,
            ":foldclose must close"
        );
    }

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

    // --- set_cmd_count ---

    #[test]
    fn set_cmd_count_extends_a_line_range_forwards() {
        // Cross-verified against real nvim: `2delete 3` removes lines
        // 2-4, i.e. the count extends the range forwards from line2.
        use crate::ex_cmds_defs::{CmdAddrT, ExargT};
        let mut eap = ExargT {
            addr_type: CmdAddrT::Lines,
            line1: 1,
            line2: 2,
            addr_count: 1,
            ..Default::default()
        };
        unsafe { set_cmd_count(&mut eap, 3, false) };
        assert_eq!((eap.line1, eap.line2), (2, 4));
        assert_eq!(eap.addr_count, 2);
    }

    #[test]
    fn set_cmd_count_is_the_address_itself_for_a_non_line_type() {
        // e.g. `:buffer 2` - the count IS the address.
        use crate::ex_cmds_defs::{CmdAddrT, ExargT};
        let mut eap = ExargT {
            addr_type: CmdAddrT::Buffers,
            line1: 0,
            line2: 0,
            addr_count: 0,
            ..Default::default()
        };
        unsafe { set_cmd_count(&mut eap, 2, false) };
        assert_eq!(eap.line2, 2);
        assert_eq!(eap.addr_count, 1, "a bare count still counts as one address");

        // An existing address count is left alone.
        let mut eap = ExargT {
            addr_type: CmdAddrT::Buffers,
            addr_count: 5,
            ..Default::default()
        };
        unsafe { set_cmd_count(&mut eap, 7, false) };
        assert_eq!((eap.line2, eap.addr_count), (7, 5));
    }

    #[test]
    fn set_cmd_count_saturates_instead_of_overflowing() {
        use crate::ex_cmds_defs::{CmdAddrT, ExargT};
        let mut eap = ExargT {
            addr_type: CmdAddrT::Lines,
            line2: i32::MAX - 2,
            ..Default::default()
        };
        unsafe { set_cmd_count(&mut eap, 100, false) };
        assert_eq!(eap.line2, i32::MAX);
    }

    #[test]
    fn set_cmd_count_clamps_to_the_buffer_when_validating() {
        // Cross-verified against real nvim: `2delete 99` on a 3-line
        // buffer silently clamps rather than erroring.
        use crate::ex_cmds_defs::{CmdAddrT, ExargT};
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        buf.b_ml.ml_line_count = 3;
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.curbuf;
        g.curbuf = &mut buf;

        let mut eap = ExargT {
            addr_type: CmdAddrT::Lines,
            line2: 2,
            ..Default::default()
        };
        unsafe { set_cmd_count(&mut eap, 99, true) };
        assert_eq!(eap.line2, 3, "clamped to the last line");

        // Without validate the out-of-range value survives.
        let mut eap = ExargT {
            addr_type: CmdAddrT::Lines,
            line2: 2,
            ..Default::default()
        };
        unsafe { set_cmd_count(&mut eap, 99, false) };
        assert_eq!(eap.line2, 100);

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

    // --- one_letter_cmd ---

    #[test]
    fn one_letter_cmd_recognizes_k_but_not_keepmarks() {
        use crate::ex_cmds_defs::CmdIdxT;
        assert_eq!(one_letter_cmd(b"k"), Some(CmdIdxT::k));
        assert_eq!(one_letter_cmd(b"ka"), Some(CmdIdxT::k));
        // "ke" alone is still :k - only a second 'e' rules it out,
        // which is what protects :keepmarks/:keepalt etc.
        assert_eq!(one_letter_cmd(b"ke"), Some(CmdIdxT::k));
        assert_eq!(one_letter_cmd(b"kee"), None);
        assert_eq!(one_letter_cmd(b"keepmarks"), None);
    }

    #[test]
    fn one_letter_cmd_recognizes_the_substitute_abbreviations() {
        use crate::ex_cmds_defs::CmdIdxT;
        for s in [&b"sg"[..], b"sI", b"sr", b"si", b"sc"] {
            assert_eq!(
                one_letter_cmd(s),
                Some(CmdIdxT::substitute),
                "{} abbreviates :substitute",
                String::from_utf8_lossy(s)
            );
        }
    }

    #[test]
    fn one_letter_cmd_leaves_the_longer_s_commands_alone() {
        // These prefixes belong to real commands (:scscope, :scriptnames,
        // :simalt, :silent, :sign, :sread-style), so they must not be
        // stolen by the :s abbreviation.
        for s in [&b"scs"[..], b"scr", b"sim", b"sil", b"sig", b"sre"] {
            assert_eq!(
                one_letter_cmd(s),
                None,
                "{} must not become :substitute",
                String::from_utf8_lossy(s)
            );
        }
    }

    #[test]
    fn one_letter_cmd_is_none_for_anything_else() {
        assert_eq!(one_letter_cmd(b""), None);
        assert_eq!(one_letter_cmd(b"echo"), None);
        assert_eq!(one_letter_cmd(b"s"), None, "a bare s is not handled here");
    }

    // --- skip_colon_white / parse_bang / cmd_has_expr_args ---

    #[test]
    fn skip_colon_white_consumes_colons_and_surrounding_space() {
        assert_eq!(skip_colon_white(b"  :: echo", true), 5);
        assert_eq!(skip_colon_white(b":::x", true), 3);
        assert_eq!(skip_colon_white(b"echo", true), 0);
    }

    #[test]
    fn skip_colon_white_can_skip_the_leading_whitespace_pass() {
        // With skipleadingwhite false a leading space stops it dead,
        // since the loop only advances past colons.
        assert_eq!(skip_colon_white(b"  :echo", false), 0);
        assert_eq!(skip_colon_white(b":  :echo", false), 4);
    }

    #[test]
    fn skip_colon_white_on_empty_input_is_zero() {
        assert_eq!(skip_colon_white(b"", true), 0);
        assert_eq!(skip_colon_white(b"   ", true), 3);
    }

    #[test]
    fn parse_bang_consumes_a_leading_bang() {
        let (found, used) = parse_bang(crate::ex_cmds_defs::CmdIdxT::edit, b"! rest");
        assert!(found);
        assert_eq!(used, 1);
    }

    #[test]
    fn parse_bang_ignores_the_bang_for_substitute_and_friends() {
        // Cross-verified against real nvim: `s!a!X!` uses `!` as the
        // pattern delimiter, so it must not be eaten as a modifier.
        for idx in [
            crate::ex_cmds_defs::CmdIdxT::substitute,
            crate::ex_cmds_defs::CmdIdxT::smagic,
            crate::ex_cmds_defs::CmdIdxT::snomagic,
        ] {
            let (found, used) = parse_bang(idx, b"!a!X!");
            assert!(!found, "{idx:?} must keep its own delimiter");
            assert_eq!(used, 0);
        }
    }

    #[test]
    fn parse_bang_is_false_without_a_bang() {
        let (found, used) = parse_bang(crate::ex_cmds_defs::CmdIdxT::edit, b" file");
        assert!(!found);
        assert_eq!(used, 0);
        assert_eq!(parse_bang(crate::ex_cmds_defs::CmdIdxT::edit, b""), (false, 0));
    }

    #[test]
    fn cmd_has_expr_args_is_true_only_for_the_five_expression_commands() {
        for idx in [
            crate::ex_cmds_defs::CmdIdxT::execute,
            crate::ex_cmds_defs::CmdIdxT::echo,
            crate::ex_cmds_defs::CmdIdxT::echon,
            crate::ex_cmds_defs::CmdIdxT::echomsg,
            crate::ex_cmds_defs::CmdIdxT::echoerr,
        ] {
            assert!(cmd_has_expr_args(idx), "{idx:?} takes expression args");
        }
        for idx in [
            crate::ex_cmds_defs::CmdIdxT::edit,
            crate::ex_cmds_defs::CmdIdxT::substitute,
            crate::ex_cmds_defs::CmdIdxT::append,
        ] {
            assert!(!cmd_has_expr_args(idx), "{idx:?} does not");
        }
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

    // --- save_current_state / restore_current_state ---

    /// Snapshot of every `GLOBALS` field `save_current_state`/
    /// `restore_current_state` touch, so each test can restore the
    /// exact pre-test values afterward (this process-wide state is
    /// shared with every other test in the crate).
    struct StateSnapshot {
        msg_scroll: i32,
        restart_edit: i32,
        msg_didout: bool,
        state: i32,
        finish_op: bool,
        opcount: i32,
        reg_executing: i32,
        pending_end_reg_executing: bool,
        force_restart_edit: bool,
    }

    impl StateSnapshot {
        fn capture() -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            StateSnapshot {
                msg_scroll: g.msg_scroll,
                restart_edit: g.restart_edit,
                msg_didout: g.msg_didout,
                state: g.State,
                finish_op: g.finish_op,
                opcount: g.opcount,
                reg_executing: g.reg_executing,
                pending_end_reg_executing: g.pending_end_reg_executing,
                force_restart_edit: g.force_restart_edit,
            }
        }

        fn restore(self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.msg_scroll = self.msg_scroll;
            g.restart_edit = self.restart_edit;
            g.msg_didout = self.msg_didout;
            g.State = self.state;
            g.finish_op = self.finish_op;
            g.opcount = self.opcount;
            g.reg_executing = self.reg_executing;
            g.pending_end_reg_executing = self.pending_end_reg_executing;
            g.force_restart_edit = self.force_restart_edit;
        }
    }

    #[test]
    fn save_current_state_captures_globals_and_resets_msg_scroll_and_restart_edit() {
        let _lock = crate::globals::global_state_test_lock();
        let snap = StateSnapshot::capture();

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.msg_scroll = 5;
        g.restart_edit = 3;
        g.msg_didout = true;
        g.State = 0x10;
        g.finish_op = true;
        g.opcount = 7;
        g.reg_executing = i32::from(b'a');
        g.pending_end_reg_executing = true;

        let mut sst = SaveStateT::default();
        let ok = unsafe { save_current_state(&mut sst) };

        assert!(ok, "save_typeahead always sets typebuf_valid true today");
        assert_eq!(sst.save_msg_scroll, 5);
        assert_eq!(sst.save_restart_edit, 3);
        assert!(sst.save_msg_didout);
        assert_eq!(sst.save_state, 0x10);
        assert!(sst.save_finish_op);
        assert_eq!(sst.save_opcount, 7);
        assert_eq!(sst.save_reg_executing, i32::from(b'a'));
        assert!(sst.save_pending_end_reg_executing);

        let g2 = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(g2.msg_scroll, 0, "no msg scrolling in Normal mode");
        assert_eq!(g2.restart_edit, 0, "don't go to Insert mode");

        snap.restore();
    }

    #[test]
    fn restore_current_state_round_trips_through_save_current_state() {
        let _lock = crate::globals::global_state_test_lock();
        let snap = StateSnapshot::capture();

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.msg_scroll = 9;
        g.restart_edit = 4;
        g.State = 0x20;
        g.force_restart_edit = false;

        let mut sst = SaveStateT::default();
        unsafe { save_current_state(&mut sst) };

        // Simulate state changing during Normal-mode command execution.
        let g2 = unsafe { crate::globals::GLOBALS.get_mut() };
        g2.msg_scroll = 999;
        g2.restart_edit = 999;
        g2.State = 0xff;

        unsafe { restore_current_state(&mut sst) };

        let g3 = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(g3.msg_scroll, 9);
        assert_eq!(g3.restart_edit, 4);
        assert_eq!(g3.State, 0x20);

        snap.restore();
    }

    #[test]
    fn restore_current_state_force_restart_edit_overrides_saved_restart_edit() {
        let _lock = crate::globals::global_state_test_lock();
        let snap = StateSnapshot::capture();

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.restart_edit = 100;
        g.force_restart_edit = true;

        let mut sst = SaveStateT { save_restart_edit: 42, ..Default::default() };
        unsafe { restore_current_state(&mut sst) };

        let g2 = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(g2.restart_edit, 100, "restart_edit untouched since force_restart_edit was true");
        assert!(!g2.force_restart_edit, "force_restart_edit itself is always cleared");

        snap.restore();
    }

    #[test]
    fn restore_current_state_msg_didout_is_ored_in_not_overwritten() {
        let _lock = crate::globals::global_state_test_lock();
        let snap = StateSnapshot::capture();

        unsafe { crate::globals::GLOBALS.get_mut() }.msg_didout = true;
        let mut sst = SaveStateT { save_msg_didout: false, ..Default::default() };
        unsafe { restore_current_state(&mut sst) };
        assert!(
            unsafe { crate::globals::GLOBALS.get_mut() }.msg_didout,
            "true | false == true, stays true"
        );

        snap.restore();
    }

    // --- set_no_hlsearch / ex_nohlsearch ---

    fn reset_hlsearch_state() {
        unsafe { crate::globals::GLOBALS.get_mut() }.Search.no_hlsearch = false;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hls = 1;
    }

    #[test]
    fn set_no_hlsearch_true_disables_hlsearch_regardless_of_p_hls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_hlsearch_state();

        unsafe { set_no_hlsearch(true) };

        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.Search.no_hlsearch);
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_nr(crate::eval::vars::VimVarIndex::Hlsearch) },
            0
        );
        reset_hlsearch_state();
    }

    #[test]
    fn set_no_hlsearch_false_with_p_hls_set_enables_v_hlsearch() {
        let _lock = crate::globals::global_state_test_lock();
        reset_hlsearch_state();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hls = 1;

        unsafe { set_no_hlsearch(false) };

        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.Search.no_hlsearch);
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_nr(crate::eval::vars::VimVarIndex::Hlsearch) },
            1
        );
        reset_hlsearch_state();
    }

    #[test]
    fn set_no_hlsearch_false_with_p_hls_unset_leaves_v_hlsearch_false() {
        let _lock = crate::globals::global_state_test_lock();
        reset_hlsearch_state();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hls = 0;

        unsafe { set_no_hlsearch(false) };

        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_nr(crate::eval::vars::VimVarIndex::Hlsearch) },
            0
        );
        reset_hlsearch_state();
    }

    #[test]
    fn ex_nohlsearch_sets_no_hlsearch_true() {
        let _lock = crate::globals::global_state_test_lock();
        reset_hlsearch_state();
        let eap = crate::ex_cmds_defs::ExargT::default();

        unsafe { ex_nohlsearch(&eap) };

        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.Search.no_hlsearch);
        reset_hlsearch_state();
    }
}
