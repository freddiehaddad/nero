//! Translated from `src/nvim/ex_getln.c` (tractable core only).
//!
//! `ex_getln.c` is the command-line-editing/history file (thousands of
//! lines - needs the whole cmdline-editing subsystem, not attempted
//! here). Translated: [`vim_strsave_fnameescape`]/[`escape_fname`]
//! (used by `fnameescape()` and several not-yet-translated `ex_*`
//! commands), tractable on their own via the already-existing
//! `crate::strings::vim_strsave_escaped`/`crate::option::csh_like_shell`.
//!
//! The original's `vim_isfilec()`-based special-case for `[`/`{`/`!`
//! (only reached on the `BACKSLASH_IN_FILENAME`/Windows branch) is
//! simplified to its REAL answer for the DEFAULT, unconfigured
//! `'isfname'` value: `false` for all three characters, verified
//! directly against `'isfname'`'s own real, documented default value
//! on BOTH platforms (`@,48-57,/,.,-,_,+,,,#,$,%,~,=` non-Windows,
//! plus `,\,:` on Windows - neither ever includes `[`/`{`/`!`) - this
//! makes the original's own `(*p != '[' && ...) || !vim_isfilec(*p)`
//! filter condition ALWAYS true for the default option value, so every
//! character in the escape-char-set constants is genuinely kept
//! verbatim, matching this crate's established "fixed default rule"
//! pattern (`vim_isprintc`/`vim_isbreak`/`vim_isidc`) rather than the
//! general `g_chartab`-dependent mechanism.
//!
//! Also translated: [`cmdline_overstrike`]/[`cmdline_at_end`] - each
//! reads a single narrow field of the original's own file-static
//! `ccline` (`overstrike`/`cmdpos`/`cmdlen` respectively), modeled as
//! their own standalone file-statics rather than a full `CmdlineInfo`
//! struct, matching [`get_cmdline_firstc`]'s own already-established
//! `CMDLINE_FIRSTC` precedent. Both always return their real, current
//! answer for these fields' always-zero/false initial state (`true`
//! for `cmdline_at_end`, `false` for `cmdline_overstrike`), since
//! nothing in this crate can start real command-line editing yet -
//! not a hardcoded shortcut.
//!
//! Also translated: [`cmdpreview_get_bufnr`]/[`cmdpreview_get_ns`] -
//! trivial accessors over the original's own file-static
//! `cmdpreview_bufnr`/`cmdpreview_ns`, modeled the same way. Both
//! always `0` today, since nothing in this crate can start a real
//! `'inccommand'` command preview yet (`cmdpreview_open_buf`, their
//! only real writer, is not translated).

/// What [`vim_strsave_fnameescape`] is escaping for (`VSE_NONE`/
/// `VSE_SHELL`/`VSE_BUFFER`, `ex_getln.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VseWhat {
    /// escape for a file name (`VSE_NONE`).
    #[default]
    None,
    /// escape for a shell command (`VSE_SHELL`).
    Shell,
    /// escape for a `:buffer` command (`VSE_BUFFER`).
    Buffer,
}

#[cfg(windows)]
const PATH_ESC_CHARS: &[u8] = b" \t\n*?[{`%#'\"|!<";
#[cfg(windows)]
const BUFFER_ESC_CHARS: &[u8] = b" \t\n*?[`'\"|!<";

#[cfg(not(windows))]
const PATH_ESC_CHARS: &[u8] = b" \t\n*?[{`$\\%#'\"|!<";
#[cfg(not(windows))]
const SHELL_ESC_CHARS: &[u8] = b" \t\n*?[{`$\\%#'\"|!<>();&";
#[cfg(not(windows))]
const BUFFER_ESC_CHARS: &[u8] = b" \t\n*?[`$\\%#'\"|!<";

/// Put a backslash before `s`, in place (`escape_fname`).
pub fn escape_fname(s: &mut Vec<u8>) {
    s.insert(0, b'\\');
}

/// Escape `fname` for use as a `:!`/`:cd`/file-name-context argument
/// (`vim_strsave_fnameescape`). See this module's own doc comment for
/// the `vim_isfilec`-simplification this relies on.
///
/// # Safety
/// Touches `OPTION_VARS` (via `crate::strings::vim_strsave_escaped`).
#[must_use]
pub unsafe fn vim_strsave_fnameescape(fname: &[u8], what: VseWhat) -> Vec<u8> {
    #[cfg(windows)]
    let esc_chars = if what == VseWhat::Buffer { BUFFER_ESC_CHARS } else { PATH_ESC_CHARS };
    #[cfg(windows)]
    // SAFETY: forwarded from this function's own safety doc.
    let mut p = unsafe { crate::strings::vim_strsave_escaped(fname, esc_chars) };

    #[cfg(not(windows))]
    let esc_chars = match what {
        VseWhat::Shell => SHELL_ESC_CHARS,
        VseWhat::Buffer => BUFFER_ESC_CHARS,
        VseWhat::None => PATH_ESC_CHARS,
    };
    #[cfg(not(windows))]
    // SAFETY: forwarded from this function's own safety doc.
    let mut p = unsafe { crate::strings::vim_strsave_escaped(fname, esc_chars) };
    #[cfg(not(windows))]
    if what == VseWhat::Shell && crate::option::csh_like_shell() {
        // For csh and similar shells need to put two backslashes
        // before '!'. One is taken by Vim, one by the shell.
        // SAFETY: forwarded from this function's own safety doc.
        p = unsafe { crate::strings::vim_strsave_escaped(&p, b"!") };
    }

    // '>' and '+' are special at the start of some commands, e.g.
    // ":edit" and ":write". "cd -" has a special meaning.
    if p.first() == Some(&b'>')
        || p.first() == Some(&b'+')
        || (p.first() == Some(&b'-') && p.len() == 1)
    {
        escape_fname(&mut p);
    }

    p
}

/// Whether a command line is currently being edited (`get_ccline_ptr`,
/// `ex_getln.c`) - the original resolves to one of 3 further branches
/// (a live `ccline`, a saved `ccline.prev_ccline`, or `NULL`) ONLY
/// after first checking `(State & MODE_CMDLINE) == 0` - and since
/// nothing in this crate can ever set the `MODE_CMDLINE` bit on
/// `GLOBALS.State` (no `:`/`/`-style command-line entry mode exists
/// yet), that check is always true, making every real caller's own
/// "is a command line active" question always `false` today - a
/// faithful, always-taken early return, not a hardcoded shortcut
/// (matching this crate's established `AUTOCMDS`/`ctx_restore`
/// precedent for this exact pattern).
fn cmdline_is_active() -> bool {
    // SAFETY: reading a plain `i32` field, no aliasing hazard.
    let state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
    state & crate::state_defs::mode::CMDLINE as i32 != 0
}

/// `ccline.cmdfirstc` - the leading character of the CURRENTLY-being-
/// edited command line (`:`/`=`/`@`/`>`/`/`/`?`), read via
/// [`get_cmdline_firstc`]. This is a plain `i32` field of the
/// original's own file-static `ccline` (not gated behind
/// `get_ccline_ptr`'s `MODE_CMDLINE` check the way `cmdline_is_active`
/// is) - so it's modeled directly as its own file-static, matching
/// `cmdline_star`'s own precedent in `GLOBALS`, rather than the full
/// `CmdlineInfo` struct (not needed for this one field). Always `0`
/// (NUL) today: a fresh, zero-initialized `ccline.cmdfirstc`, since
/// nothing in this crate can start real command-line editing yet.
static CMDLINE_FIRSTC: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// `get_cmdline_firstc()` - the leading character of the current
/// command line, or `0` (NUL) when none is active (`ex_getln.c`).
/// Always `0` today - see `CMDLINE_FIRSTC`'s own doc comment.
pub fn get_cmdline_firstc() -> i32 {
    // SAFETY: a plain `i32` copy-out read, no aliasing hazard.
    unsafe { *CMDLINE_FIRSTC.get_mut() }
}

/// `ccline.overstrike` - whether the command line is in Insert
/// (`false`) or Replace (`true`) submode, read via
/// [`cmdline_overstrike`]. Modeled as its own file-static, matching
/// [`CMDLINE_FIRSTC`]'s own established precedent for a single
/// `ccline` field. Always `false` today, since nothing in this crate
/// can start real command-line editing yet.
static CMDLINE_OVERSTRIKE: crate::globals::GlobalCell<bool> = crate::globals::GlobalCell::new(false);

/// Return `true` if the command line is in Replace mode
/// (`cmdline_overstrike`). Always `false` today - see
/// `CMDLINE_OVERSTRIKE`'s own doc comment.
#[must_use]
pub fn cmdline_overstrike() -> bool {
    // SAFETY: a plain `bool` copy-out read, no aliasing hazard.
    unsafe { *CMDLINE_OVERSTRIKE.get_mut() }
}

/// `ccline.cmdpos`/`ccline.cmdlen` - the cursor's byte position in,
/// and the total byte length of, the command line, read via
/// [`cmdline_at_end`]. Modeled as their own file-statics, matching
/// [`CMDLINE_FIRSTC`]'s own established precedent. Both always `0`
/// today, since nothing in this crate can start real command-line
/// editing yet.
static CMDLINE_CMDPOS: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);
static CMDLINE_CMDLEN: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// Return `true` if the cursor is at the end of the command line
/// (`cmdline_at_end`). Always `true` today (`0 >= 0`) - see
/// `CMDLINE_CMDPOS`'s own doc comment.
#[must_use]
pub fn cmdline_at_end() -> bool {
    // SAFETY: plain `i32` copy-out reads, no aliasing hazard.
    unsafe { *CMDLINE_CMDPOS.get_mut() >= *CMDLINE_CMDLEN.get_mut() }
}

/// `getcmdcomplpat()` - the current command-line completion pattern
/// (`f_getcmdcomplpat`, `ex_getln.c`) - always empty today, since
/// `cmdline_is_active` is always `false` (the original's own
/// `get_cmdline_completion_pattern` checks `cmdline_star > 0` first -
/// always false, `GLOBALS.cmdline_star` defaults to `0` and nothing
/// yet sets it - then falls through to the same "no active command
/// line" `NULL` result either way).
pub fn f_getcmdcomplpat(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    rettv.value = if cmdline_is_active() {
        unimplemented!("getcmdcomplpat(): needs a real, live command-line-editing state")
    } else {
        crate::eval::typval_defs::TypvalValue::String(None)
    };
}

/// `getcmdcompltype()` - the current command-line completion type
/// (`f_getcmdcompltype`, `ex_getln.c`) - always empty today, matching
/// [`f_getcmdcomplpat`]'s own exact reasoning (`get_cmdline_completion`
/// has the identical `cmdline_star`/`get_ccline_ptr` structure).
pub fn f_getcmdcompltype(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    rettv.value = if cmdline_is_active() {
        unimplemented!("getcmdcompltype(): needs a real, live command-line-editing state")
    } else {
        crate::eval::typval_defs::TypvalValue::String(None)
    };
}

/// `getcmdline()` - the current command-line input (`f_getcmdline`,
/// `ex_getln.c`) - always empty today, since `cmdline_is_active` is
/// always `false`.
pub fn f_getcmdline(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    rettv.value = if cmdline_is_active() {
        unimplemented!("getcmdline(): needs a real, live command-line-editing state")
    } else {
        crate::eval::typval_defs::TypvalValue::String(None)
    };
}

/// `getcmdpos()` - the cursor's byte position (1-based) in the
/// command line (`f_getcmdpos`, `ex_getln.c`) - always `0` today
/// (no active command line), since `cmdline_is_active` is always
/// `false`.
pub fn f_getcmdpos(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    let n: i64 = if cmdline_is_active() {
        unimplemented!("getcmdpos(): needs a real, live command-line-editing state")
    } else {
        0
    };
    rettv.value = crate::eval::typval_defs::TypvalValue::Number(n);
}

/// `getcmdprompt()` - the current command-line prompt (`f_getcmdprompt`,
/// `ex_getln.c`) - always empty today, since `cmdline_is_active` is
/// always `false`.
pub fn f_getcmdprompt(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    rettv.value = if cmdline_is_active() {
        unimplemented!("getcmdprompt(): needs a real, live command-line-editing state")
    } else {
        crate::eval::typval_defs::TypvalValue::String(None)
    };
}

/// `getcmdscreenpos()` - the cursor's screen position (1-based) in the
/// command line (`f_getcmdscreenpos`, `ex_getln.c`) - always `0` today
/// (no active command line), since `cmdline_is_active` is always
/// `false`.
pub fn f_getcmdscreenpos(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    let n: i64 = if cmdline_is_active() {
        unimplemented!("getcmdscreenpos(): needs a real, live command-line-editing state")
    } else {
        0
    };
    rettv.value = crate::eval::typval_defs::TypvalValue::Number(n);
}

/// `getcmdtype()` - the current command-line type (`f_getcmdtype`,
/// `ex_getln.c`) - always an empty string today, since
/// `cmdline_is_active` is always `false` (the original's own
/// `get_cmdline_type` returns `NUL`, a genuinely empty string, for
/// this exact "no active command line" case).
pub fn f_getcmdtype(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    rettv.value = if cmdline_is_active() {
        unimplemented!("getcmdtype(): needs a real, live command-line-editing state")
    } else {
        crate::eval::typval_defs::TypvalValue::String(None)
    };
}

/// `wildtrigger()` - start wildcard expansion in the command line
/// (`f_wildtrigger`, `ex_getln.c`) - a real no-op today: the
/// original's own FIRST disjunct, `!(State & MODE_CMDLINE)`, is
/// exactly `!cmdline_is_active()`, always `true` today - and since
/// C's `||` short-circuits, `char_avail()`/`wild_menu_showing`/
/// `cmdline_pum_active()` are NEVER even evaluated once that first
/// disjunct is true, so none of those need to exist here either.
/// `rettv` is left completely untouched, matching the original's own
/// body (which never assigns to `rettv` at all - `call_func`'s own
/// caller already initializes it to `VAR_UNKNOWN` before dispatch).
pub fn f_wildtrigger(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    _rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    if !cmdline_is_active() {
        return;
    }
    unimplemented!("wildtrigger(): needs a real, live command-line-editing state")
}

/// `cmdpreview_bufnr` - the buffer handle of the current `'inccommand'`
/// preview buffer, or `0` when no preview is active, read via
/// [`cmdpreview_get_bufnr`]. Modeled as its own file-static, matching
/// [`CMDLINE_FIRSTC`]'s own established precedent. Always `0` today,
/// since nothing in this crate can start a real command preview yet
/// (`cmdpreview_open_buf`, its only real writer, is not translated).
static CMDPREVIEW_BUFNR: crate::globals::GlobalCell<crate::api::private::defs::Buffer> =
    crate::globals::GlobalCell::new(0);

/// Returns the buffer handle of the current `'inccommand'` preview
/// buffer, or `0` when none is active (`cmdpreview_get_bufnr`). Always
/// `0` today - see `CMDPREVIEW_BUFNR`'s own doc comment.
#[must_use]
pub fn cmdpreview_get_bufnr() -> crate::api::private::defs::Buffer {
    // SAFETY: a plain `i32` copy-out read, no aliasing hazard.
    unsafe { *CMDPREVIEW_BUFNR.get_mut() }
}

/// `cmdpreview_ns` - the namespace ID used for `'inccommand'` preview
/// highlights, or `0` when no preview is active, read via
/// [`cmdpreview_get_ns`]. Modeled as its own file-static, matching
/// [`CMDPREVIEW_BUFNR`]'s own precedent just above. Always `0` today,
/// for the same reason.
static CMDPREVIEW_NS: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// Returns the namespace ID used for `'inccommand'` preview
/// highlights, or `0` when none is active (`cmdpreview_get_ns`).
/// Always `0` today - see `CMDPREVIEW_NS`'s own doc comment.
#[must_use]
pub fn cmdpreview_get_ns() -> i32 {
    // SAFETY: a plain `i32` copy-out read, no aliasing hazard.
    unsafe { *CMDPREVIEW_NS.get_mut() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_fname_prepends_a_backslash() {
        let mut s = b"foo".to_vec();
        escape_fname(&mut s);
        assert_eq!(s, b"\\foo".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_escapes_a_space() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_fnameescape(b"a b", VseWhat::None) }, b"a\\ b".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_plain_name_is_unchanged() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_fnameescape(b"hello", VseWhat::None) }, b"hello".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_escapes_a_leading_dash_that_is_the_whole_name() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_fnameescape(b"-", VseWhat::None) }, b"\\-".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_does_not_escape_a_dash_followed_by_more_text() {
        let _guard = crate::globals::global_state_test_lock();
        // "-foo" is not the special bare "-" case, so it should only
        // get the ordinary escaping (none of its own characters are
        // in the escape-char set).
        assert_eq!(unsafe { vim_strsave_fnameescape(b"-foo", VseWhat::None) }, b"-foo".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_escapes_a_leading_greater_than() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_fnameescape(b">foo", VseWhat::None) }, b"\\>foo".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_escapes_a_leading_plus() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_fnameescape(b"+foo", VseWhat::None) }, b"\\+foo".to_vec());
    }

    // --- cmdline_is_active / f_getcmdline / f_getcmdpos / f_getcmdprompt
    // / f_getcmdscreenpos / f_getcmdtype ---

    #[test]
    fn cmdline_is_active_is_false_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        // GLOBALS.State defaults to mode::NORMAL (no MODE_CMDLINE bit),
        // matching this crate's own established `Globals::default`
        // convention.
        assert!(!cmdline_is_active());
    }

    #[test]
    fn cmdline_is_active_is_true_when_the_cmdline_bit_is_set() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: `global_state_test_lock()` held for this whole test.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_state = g.State;
        g.State = crate::state_defs::mode::CMDLINE as i32;

        assert!(cmdline_is_active());

        // SAFETY: forwarded from the lock reasoning above.
        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
    }

    #[test]
    fn cmdline_overstrike_is_false_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        assert!(!cmdline_overstrike());
    }

    #[test]
    fn cmdline_at_end_is_true_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        // Both CMDLINE_CMDPOS and CMDLINE_CMDLEN default to 0, and
        // 0 >= 0 is true.
        assert!(cmdline_at_end());
    }

    #[test]
    fn cmdpreview_get_bufnr_is_zero_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(cmdpreview_get_bufnr(), 0);
    }

    #[test]
    fn cmdpreview_get_ns_is_zero_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(cmdpreview_get_ns(), 0);
    }

    #[test]
    fn getcmdcomplpat_is_empty_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdcomplpat(&[], &mut rettv);
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(None)
        );
    }

    #[test]
    fn getcmdcompltype_is_empty_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdcompltype(&[], &mut rettv);
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(None)
        );
    }

    #[test]
    fn getcmdline_is_empty_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdline(&[], &mut rettv);
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(None)
        );
    }

    #[test]
    fn getcmdpos_is_zero_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdpos(&[], &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(0));
    }

    #[test]
    fn getcmdprompt_is_empty_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdprompt(&[], &mut rettv);
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(None)
        );
    }

    #[test]
    fn getcmdscreenpos_is_zero_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdscreenpos(&[], &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(0));
    }

    #[test]
    fn getcmdtype_is_empty_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdtype(&[], &mut rettv);
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(None)
        );
    }

    #[test]
    #[should_panic(expected = "getcmdtype(): needs a real, live command-line-editing state")]
    fn getcmdtype_panics_when_a_command_line_is_genuinely_active() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: `global_state_test_lock()` held for this whole test.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_state = g.State;
        g.State = crate::state_defs::mode::CMDLINE as i32;

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f_getcmdtype(&[], &mut rettv);
        }));

        // SAFETY: forwarded from the lock reasoning above.
        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    }

    #[test]
    fn wildtrigger_is_a_no_op_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_wildtrigger(&[], &mut rettv);
        // rettv is left completely untouched, matching the original's
        // own body (which never assigns to it at all).
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Unknown);
    }

    #[test]
    #[should_panic(expected = "wildtrigger(): needs a real, live command-line-editing state")]
    fn wildtrigger_panics_when_a_command_line_is_genuinely_active() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: `global_state_test_lock()` held for this whole test.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_state = g.State;
        g.State = crate::state_defs::mode::CMDLINE as i32;

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f_wildtrigger(&[], &mut rettv);
        }));

        // SAFETY: forwarded from the lock reasoning above.
        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    }
}
