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
//! of process spawning.
//!
//! `shell_free_argv` has no equivalent: this crate models an argument
//! vector as an owned `Vec<Vec<u8>>`, so dropping it frees both the
//! individual arguments and the vector itself.
//!
//! Deferred: everything process-bound.

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
#[allow(dead_code)]
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
