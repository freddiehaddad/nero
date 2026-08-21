//! Translated from `src/nvim/os/shell.c` (tractable core only).
//!
//! `shell.c` is mostly the libuv process-spawning engine
//! (`os_call_shell`, `do_os_system`, the `out_data_*` callbacks) plus
//! `os_expand_wildcards`, which shells out to a temporary file. All of
//! that needs the event loop and real process spawning, neither
//! translated.
//!
//! Translated: the two small argument-vector predicates
//! ([`have_wildcard`]/[`have_dollars`]) that `os_expand_wildcards`
//! consults before deciding it needs a shell at all, and
//! [`shell_argv_to_str`], which only formats. The quote-aware
//! `word_length`/`tokenize` parser chain is translated independently
//! of process spawning, as is wildcard fallback copying
//! (`save_patterns`).
//!
//! `shell_free_argv` has no equivalent: this crate models an argument
//! vector as an owned `Vec<Vec<u8>>`, so dropping it frees both the
//! individual arguments and the vector itself.
//!
//! [`shell_build_argv`] is translated too; everything remaining is
//! process-bound.

/// Whether any of `argv` contains a wildcard (`have_wildcard`).
#[must_use]
pub fn have_wildcard(files: &[Vec<u8>]) -> bool {
    files.iter().any(|f| crate::path::path_has_wildcard(f))
}

/// Whether any of `argv` contains a `$` (`have_dollars`).
#[must_use]
pub fn have_dollars(files: &[Vec<u8>]) -> bool {
    files
        .iter()
        .any(|f| crate::strings::vim_strchr(f, i32::from(b'$')).is_some())
}

/// Copy wildcard patterns while halving escaped backslashes
/// (`save_patterns`).
#[allow(dead_code)]
#[must_use]
fn save_patterns(patterns: &[Vec<u8>]) -> Vec<Vec<u8>> {
    patterns
        .iter()
        .map(|pattern| crate::charset::backslash_halve_save(pattern))
        .collect()
}

/// Return the byte length of one shell word (`word_length`).
#[must_use]
fn word_length(string: &[u8]) -> usize {
    let mut pos = 0;
    let mut inquote = false;
    while pos < string.len()
        && (inquote || !matches!(string[pos], b' ' | b'\t'))
    {
        if string[pos] == b'"' {
            inquote = !inquote;
        } else if string[pos] == b'\\'
            && inquote
            && pos + 1 < string.len()
        {
            pos += 2;
            continue;
        }
        pos += 1;
    }
    pos
}

/// Split a shell command into quote-aware words (`tokenize`).
#[must_use]
fn tokenize(mut string: &[u8]) -> Vec<Vec<u8>> {
    let mut argv = Vec::new();
    while !string.is_empty() {
        let len = word_length(string);
        argv.push(crate::strings::vim_strnsave_unquoted(
            &string[..len],
        ));
        let whitespace = crate::charset::skipwhite(&string[len..]);
        string = &string[len + whitespace..];
    }
    argv
}

/// Apply `'shellxescape'` and `'shellxquote'` to a command
/// (`shell_xescape_xquote`).
///
/// # Safety
/// Reads shared option state and forwards
/// [`crate::strings::vim_strsave_escaped_ext`]'s requirement.
#[must_use]
unsafe fn shell_xescape_xquote(cmd: &[u8]) -> Vec<u8> {
    let (shellxquote, shellxescape) = {
        let options =
            unsafe { &*crate::option_vars::OPTION_VARS.as_ptr() };
        (
            options.p_sxq.clone().unwrap_or_default(),
            options.p_sxe.clone().unwrap_or_default(),
        )
    };
    if shellxquote.is_empty() {
        return cmd.to_vec();
    }

    let escaped = if !shellxescape.is_empty() && shellxquote == b"(" {
        unsafe {
            crate::strings::vim_strsave_escaped_ext(
                cmd,
                &shellxescape,
                b'^',
                false,
            )
        }
    } else {
        cmd.to_vec()
    };

    let mut result = Vec::with_capacity(
        escaped.len() + shellxquote.len() * 2,
    );
    match shellxquote.as_slice() {
        b"(" => {
            result.push(b'(');
            result.extend_from_slice(&escaped);
            result.push(b')');
        }
        b"\"(" => {
            result.extend_from_slice(b"\"(");
            result.extend_from_slice(&escaped);
            result.extend_from_slice(b")\"");
        }
        _ => {
            result.extend_from_slice(&shellxquote);
            result.extend_from_slice(&escaped);
            result.extend_from_slice(&shellxquote);
        }
    }
    result
}

/// Build the argument vector for the configured shell
/// (`shell_build_argv`).
///
/// # Safety
/// Reads shared shell option state and forwards
/// `shell_xescape_xquote`'s requirement.
#[must_use]
pub unsafe fn shell_build_argv(
    cmd: Option<&[u8]>,
    extra_args: Option<&[u8]>,
) -> Vec<Vec<u8>> {
    let (shell, shellcmdflag) = {
        let options =
            unsafe { &*crate::option_vars::OPTION_VARS.as_ptr() };
        (
            options.p_sh.clone().unwrap_or_default(),
            options.p_shcf.clone().unwrap_or_default(),
        )
    };
    let mut argv = tokenize(&shell);
    if let Some(extra_args) = extra_args {
        argv.push(extra_args.to_vec());
    }
    if let Some(cmd) = cmd {
        argv.extend(tokenize(&shellcmdflag));
        argv.push(unsafe { shell_xescape_xquote(cmd) });
    }
    assert!(!argv.is_empty());
    argv
}

/// The size of the buffer `shell_argv_to_str` formats into, including
/// the terminating NUL (the original's own `xcalloc(256, ...)`).
const ARGV_STR_MAXSIZE: usize = 256;

/// Join shell arguments into a single quoted string, for display
/// (`shell_argv_to_str`).
///
/// Each argument is wrapped in single quotes and separated by a space.
/// If the result would exceed the original's own fixed 256-byte
/// buffer it is truncated and ends in an ellipsis.
///
/// The length limit is kept rather than simplified away: it is real,
/// observable behaviour for a long command line, not an artefact of
/// the original's fixed-size buffer being awkward to express here.
#[must_use]
pub fn shell_argv_to_str(argv: &[Vec<u8>]) -> Vec<u8> {
    if argv.is_empty() {
        return Vec::new();
    }

    // Build unbounded first, then apply the original's own truncation,
    // so the "did it overflow" decision is made on the same bytes the
    // original would have produced.
    let mut rv = Vec::new();
    for arg in argv {
        rv.push(b'\'');
        rv.extend_from_slice(arg);
        rv.extend_from_slice(b"' ");
    }

    // The original's xstrlcat reports the length it WOULD have needed,
    // including the NUL; overflow is when that reaches the buffer size.
    if rv.len() < ARGV_STR_MAXSIZE {
        // Drop the trailing separator space.
        rv.pop();
        rv
    } else {
        // Command too long, show an ellipsis.
        rv.truncate(ARGV_STR_MAXSIZE - 4);
        rv.extend_from_slice(b"...");
        rv
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ShellQuoteGuard {
        shellxquote: Option<Vec<u8>>,
        shellxescape: Option<Vec<u8>>,
    }

    impl ShellQuoteGuard {
        unsafe fn set(
            shellxquote: &[u8],
            shellxescape: &[u8],
        ) -> Self {
            let options =
                unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let previous = Self {
                shellxquote: options.p_sxq.clone(),
                shellxescape: options.p_sxe.clone(),
            };
            options.p_sxq = Some(shellxquote.to_vec());
            options.p_sxe = Some(shellxescape.to_vec());
            previous
        }
    }

    impl Drop for ShellQuoteGuard {
        fn drop(&mut self) {
            let options =
                unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            options.p_sxq = self.shellxquote.take();
            options.p_sxe = self.shellxescape.take();
        }
    }

    struct ShellCommandGuard {
        shell: Option<Vec<u8>>,
        shellcmdflag: Option<Vec<u8>>,
    }

    impl ShellCommandGuard {
        unsafe fn set(shell: &[u8], shellcmdflag: &[u8]) -> Self {
            let options =
                unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let previous = Self {
                shell: options.p_sh.clone(),
                shellcmdflag: options.p_shcf.clone(),
            };
            options.p_sh = Some(shell.to_vec());
            options.p_shcf = Some(shellcmdflag.to_vec());
            previous
        }
    }

    impl Drop for ShellCommandGuard {
        fn drop(&mut self) {
            let options =
                unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            options.p_sh = self.shell.take();
            options.p_shcf = self.shellcmdflag.take();
        }
    }

    #[test]
    fn have_wildcard_finds_one_anywhere_in_the_vector() {
        assert!(!have_wildcard(&[b"plain".to_vec(), b"also_plain".to_vec()]));
        // Found on the SECOND entry, so a scan that stopped early fails.
        assert!(have_wildcard(&[b"plain".to_vec(), b"star*".to_vec()]));
        assert!(!have_wildcard(&[]));
    }

    #[test]
    fn have_dollars_finds_one_anywhere_in_the_vector() {
        assert!(!have_dollars(&[b"plain".to_vec(), b"also_plain".to_vec()]));
        assert!(have_dollars(&[b"plain".to_vec(), b"$HOME".to_vec()]));
        assert!(!have_dollars(&[]));
    }

    #[test]
    fn save_patterns_copies_and_halves_backslashes() {
        let patterns =
            [br"a\ b".to_vec(), br"c\\d".to_vec(), br"tail\".to_vec()];
        let middle = if cfg!(windows) {
            br"c\\d".to_vec()
        } else {
            br"c\d".to_vec()
        };
        assert_eq!(
            save_patterns(&patterns),
            [
                b"a b".to_vec(),
                middle,
                br"tail\".to_vec(),
            ]
        );
        assert_eq!(patterns[0], br"a\ b");
    }

    #[test]
    fn save_patterns_of_an_empty_list_is_empty() {
        assert_eq!(save_patterns(&[]), Vec::<Vec<u8>>::new());
    }

    #[test]
    fn word_length_stops_at_unquoted_whitespace() {
        assert_eq!(word_length(b"shell -c"), 5);
        assert_eq!(word_length(b"word\tanother"), 4);
    }

    #[test]
    fn word_length_includes_quoted_whitespace() {
        let input = br#""C:\Program Files\shell" -c"#;
        assert_eq!(
            word_length(input),
            br#""C:\Program Files\shell""#.len()
        );
    }

    #[test]
    fn word_length_skips_an_escaped_character_inside_quotes() {
        let input = br#""a\" b" tail"#;
        assert_eq!(word_length(input), br#""a\" b""#.len());
    }

    #[test]
    fn word_length_consumes_unterminated_quotes_and_empty_input() {
        assert_eq!(word_length(br#""unterminated word"#), 18);
        assert_eq!(word_length(b""), 0);
    }

    #[test]
    fn tokenize_splits_unquoted_words() {
        assert_eq!(
            tokenize(b"shell -c command"),
            [b"shell".to_vec(), b"-c".to_vec(), b"command".to_vec()]
        );
    }

    #[test]
    fn tokenize_removes_quotes_and_quoted_escapes() {
        assert_eq!(
            tokenize(br#""C:\Program Files\shell" "a\" b""#),
            [
                br#"C:\Program Files\shell"#.to_vec(),
                br#"a" b"#.to_vec(),
            ]
        );
    }

    #[test]
    fn tokenize_preserves_the_original_leading_whitespace_quirk() {
        assert_eq!(
            tokenize(b"  shell"),
            [Vec::<u8>::new(), b"shell".to_vec()]
        );
        assert_eq!(tokenize(b"  "), [Vec::<u8>::new()]);
    }

    #[test]
    fn tokenize_of_an_empty_string_is_empty() {
        assert_eq!(tokenize(b""), Vec::<Vec<u8>>::new());
    }

    #[test]
    fn shell_xescape_xquote_copies_when_shellxquote_is_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let _options = unsafe { ShellQuoteGuard::set(b"", b"&") };
        assert_eq!(
            unsafe { shell_xescape_xquote(b"echo &") },
            b"echo &"
        );
    }

    #[test]
    fn shell_xescape_xquote_parenthesizes_and_escapes() {
        let _lock = crate::globals::global_state_test_lock();
        let _options = unsafe { ShellQuoteGuard::set(b"(", b"&|") };
        assert_eq!(
            unsafe { shell_xescape_xquote(b"echo & | more") },
            b"(echo ^& ^| more)"
        );
    }

    #[test]
    fn shell_xescape_xquote_handles_quote_parenthesis_form() {
        let _lock = crate::globals::global_state_test_lock();
        let _options = unsafe { ShellQuoteGuard::set(b"\"(", b"&") };
        assert_eq!(
            unsafe { shell_xescape_xquote(b"echo &") },
            b"\"(echo &)\""
        );
    }

    #[test]
    fn shell_xescape_xquote_wraps_with_other_quote_strings() {
        let _lock = crate::globals::global_state_test_lock();
        let _options = unsafe { ShellQuoteGuard::set(b"\"", b"&") };
        assert_eq!(
            unsafe { shell_xescape_xquote(b"echo &") },
            b"\"echo &\""
        );
    }

    #[test]
    fn shell_build_argv_builds_an_interactive_shell_command() {
        let _lock = crate::globals::global_state_test_lock();
        let _options = unsafe {
            ShellCommandGuard::set(
                br#""C:\Program Files\shell" -login"#,
                b"-c",
            )
        };
        assert_eq!(
            unsafe { shell_build_argv(None, None) },
            [
                br#"C:\Program Files\shell"#.to_vec(),
                b"-login".to_vec(),
            ]
        );
    }

    #[test]
    fn shell_build_argv_adds_extra_flags_and_a_quoted_command() {
        let _lock = crate::globals::global_state_test_lock();
        let _command =
            unsafe { ShellCommandGuard::set(b"bash", b"-c") };
        let _quotes = unsafe { ShellQuoteGuard::set(b"(", b"&") };

        assert_eq!(
            unsafe {
                shell_build_argv(
                    Some(b"echo &"),
                    Some(b"--noprofile"),
                )
            },
            [
                b"bash".to_vec(),
                b"--noprofile".to_vec(),
                b"-c".to_vec(),
                b"(echo ^&)".to_vec(),
            ]
        );
    }

    #[test]
    fn shell_build_argv_ignores_shellcmdflag_without_a_command() {
        let _lock = crate::globals::global_state_test_lock();
        let _options =
            unsafe { ShellCommandGuard::set(b"bash", b"-c") };
        assert_eq!(
            unsafe { shell_build_argv(None, Some(b"--noprofile")) },
            [b"bash".to_vec(), b"--noprofile".to_vec()]
        );
    }

    #[test]
    #[should_panic]
    fn shell_build_argv_requires_a_configured_shell() {
        let _lock = crate::globals::global_state_test_lock();
        let _options = unsafe { ShellCommandGuard::set(b"", b"-c") };
        let _ = unsafe { shell_build_argv(None, None) };
    }

    #[test]
    fn shell_argv_to_str_quotes_and_joins() {
        assert_eq!(
            shell_argv_to_str(&[b"/bin/sh".to_vec(), b"-c".to_vec(), b"echo hi".to_vec()]),
            b"'/bin/sh' '-c' 'echo hi'".to_vec()
        );
    }

    #[test]
    fn shell_argv_to_str_of_an_empty_vector_is_empty() {
        assert_eq!(shell_argv_to_str(&[]), Vec::<u8>::new());
    }

    #[test]
    fn shell_argv_to_str_of_one_argument_has_no_trailing_space() {
        assert_eq!(shell_argv_to_str(&[b"only".to_vec()]), b"'only'".to_vec());
    }

    #[test]
    fn shell_argv_to_str_truncates_a_long_command_with_an_ellipsis() {
        // Far past the 256-byte buffer.
        let argv: Vec<Vec<u8>> = (0..50).map(|i| format!("argument{i}").into_bytes()).collect();
        let s = shell_argv_to_str(&argv);

        assert_eq!(s.len(), ARGV_STR_MAXSIZE - 1, "fills the buffer minus its NUL");
        assert!(s.ends_with(b"..."), "{:?}", String::from_utf8_lossy(&s));
    }

    #[test]
    fn shell_argv_to_str_just_under_the_limit_is_not_truncated() {
        // 250 bytes of content plus quotes stays inside the buffer.
        let arg = vec![b'x'; 250];
        let s = shell_argv_to_str(std::slice::from_ref(&arg));
        assert!(!s.ends_with(b"..."));
        assert_eq!(s.len(), 252, "two quotes around 250 bytes");
    }
}
