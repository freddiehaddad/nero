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
}
