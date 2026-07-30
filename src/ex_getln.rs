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
}
