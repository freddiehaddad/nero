//! Translated from `src/nvim/indent_c.c` (tractable core only).
//!
//! `indent_c.c` (~3700 lines) implements neovim's C-style indent engine
//! (`cindent`/`get_c_indent`) - almost entirely dependent on the
//! brace/paren/comment-skipping backtracking machinery
//! (`cin_skipcomment`/`cin_nocode`/`find_start_comment`/etc.), none of
//! which is translated.
//!
//! Translated: [`cindent_on`] and [`cin_starts_with`] - both pure
//! functions needing only already-translated option fields and
//! [`crate::charset::vim_isidc`].
//!
//! Also [`cin_is_cinword`]: the note below previously listed it as
//! blocked on `vim_iswordc`, but that function has since landed in
//! `charset.rs`, so the note was stale. It needs only `skipwhite`,
//! `copy_option_part` and the buffer's own `'cinwords'`.
//!
//! Also [`skip_string`] and [`is_pos_in_string`] - both pure, needing
//! only `ascii_isdigit` and `vim_strchr`. Two real quirks in the
//! original are preserved and pinned by tests: an UNTERMINATED string
//! does not leave the index "unmodified" as the header comment
//! suggests (the opening quote has already advanced it), and the
//! octal char-constant path is unreachable because its own digit loop
//! overshoots by one.
//!
//! Deferred: everything else - `check_linecomment` (needs
//! `cin_skipcomment`), `cin_ends_in`/`cin_is_cpp_extern_c` (need
//! `cin_skipcomment`/`cin_nocode`), and the rest of the real
//! indent-computation algorithm.

use crate::charset::vim_isidc;

/// Whether C-indenting is currently active for the current buffer
/// (`cindent_on`): `'paste'` is off, and either `'cindent'` is set or
/// `'indentexpr'` is non-empty.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` (matching `crate::indent::get_indent`'s own safety
/// doc for the same field).
#[must_use]
pub unsafe fn cindent_on() -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    let paste = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste;
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    paste == 0 && (curbuf.b_p_cin != 0 || !curbuf.b_p_inde.as_deref().unwrap_or(&[]).is_empty())
}

/// Whether `s` starts with `word` followed by a non-identifier
/// character (or nothing at all) (`cin_starts_with`).
#[must_use]
pub fn cin_starts_with(s: &[u8], word: &[u8]) -> bool {
    s.starts_with(word) && !s.get(word.len()).is_some_and(|&c| vim_isidc(i32::from(c)))
}

/// Whether `line` starts with a word from `'cinwords'`
/// (`cin_is_cinword`).
///
/// The original allocates a scratch buffer sized to `'cinwords'` and
/// hands it to `copy_option_part`; this crate's `copy_option_part`
/// returns the part it extracted, so no scratch buffer is needed.
///
/// Note the trailing check is `!vim_iswordc(line[len]) ||
/// !vim_iswordc(line[len - 1])` - an OR, not the `&&` a reader might
/// expect. So a `'cinwords'` entry that itself ENDS in a
/// non-word character matches even when the text continues with a
/// word character. Preserved exactly, and pinned by its own test.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`, matching [`cindent_on`]'s own safety doc.
#[must_use]
pub unsafe fn cin_is_cinword(line: &[u8]) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let cinw = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }
        .b_p_cinw
        .clone()
        .unwrap_or_default();
    let cinw_len = cinw.len() + 1;

    let start = crate::charset::skipwhite(line);
    let line = &line[start..];

    let mut p = 0usize;
    while p < cinw.len() {
        let (part, next) = crate::option::copy_option_part(&cinw, p, cinw_len, b",");
        p = next;
        let len = part.len();
        if len == 0 {
            continue;
        }
        if line.starts_with(&part) {
            // SAFETY: forwarded from this function's own safety doc -
            // `vim_iswordc` reads the same `curbuf` this one does.
            let (after_is_word, last_is_word) = unsafe {
                (
                    line.get(len)
                        .is_some_and(|&c| crate::charset::vim_iswordc(i32::from(c))),
                    crate::charset::vim_iswordc(i32::from(part[len - 1])),
                )
            };
            if !after_is_word || !last_is_word {
                return true;
            }
        }
    }

    false
}

/// Skip to the end of a `"string"` or a `'c'` character constant
/// (`skip_string`). If there is no string or character at `p`, `p` is
/// returned unmodified.
///
/// The original takes and returns a `const char *`; here `p` is a byte
/// INDEX into `line`, matching this crate's established
/// index-instead-of-pointer convention.
///
/// Note the original's trailing `if (!*p) p--;` "backup from NUL":
/// when the scan runs off the end of the line it steps back one byte
/// so the caller's own `p++` lands exactly on the terminator rather
/// than past it. That is reproduced here as a saturating decrement
/// when the index reaches the line's length, and is load-bearing for
/// [`is_pos_in_string`]'s own loop termination.
#[must_use]
pub fn skip_string(line: &[u8], p: usize) -> usize {
    let mut p = p;
    let at = |i: usize| line.get(i).copied().unwrap_or(0);

    // We loop, because strings may be concatenated: "date""time".
    loop {
        if at(p) == b'\'' {
            // 'c' or '\n' or '\000'
            if at(p + 1) == 0 {
                // ' at end of line
                break;
            }
            let mut i = 2usize;
            if at(p + 1) == b'\\' && at(p + 2) != 0 {
                // '\n' or '\000'
                i += 1;
                while crate::ascii_defs::ascii_isdigit(i32::from(at(p + i - 1))) {
                    i += 1;
                }
            }
            if at(p + i - 1) != 0 && at(p + i) == b'\'' {
                // check for trailing '
                p += i;
                p += 1;
                continue;
            }
        } else if at(p) == b'"' {
            // start of string
            p += 1;
            while at(p) != 0 {
                if at(p) == b'\\' && at(p + 1) != 0 {
                    p += 1;
                } else if at(p) == b'"' {
                    // end of string
                    break;
                }
                p += 1;
            }
            if at(p) == b'"' {
                p += 1;
                continue; // continue for another string
            }
        } else if at(p) == b'R' && at(p + 1) == b'"' {
            // Raw string: R"[delim](...)[delim]"
            let delim = p + 2;
            if let Some(rel) = crate::strings::vim_strchr(
                line.get(delim.min(line.len())..).unwrap_or(&[]),
                i32::from(b'('),
            ) {
                let delim_len = rel;
                p += 3;
                while at(p) != 0 {
                    if at(p) == b')'
                        && line
                            .get(p + 1..(p + 1 + delim_len).min(line.len()))
                            .is_some_and(|s| {
                                s.len() == delim_len
                                    && s == &line[delim..delim + delim_len]
                            })
                        && at(p + delim_len + 1) == b'"'
                    {
                        p += delim_len + 1;
                        break;
                    }
                    p += 1;
                }
                if at(p) == b'"' {
                    p += 1;
                    continue; // continue for another string
                }
            }
        }
        break; // no string found
    }

    if at(p) == 0 {
        // backup from NUL
        p = p.saturating_sub(1);
    }
    p
}

/// Whether `line[col]` is inside a C string (`is_pos_in_string`).
#[must_use]
pub fn is_pos_in_string(line: &[u8], col: usize) -> bool {
    let mut p = 0usize;
    while line.get(p).is_some_and(|&c| c != 0) && p < col {
        p = skip_string(line, p);
        p += 1;
    }
    p > col
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    /// RAII guard installing `buf` as `curbuf`, restoring the previous
    /// pointer on drop, and holding `global_state_test_lock` for its
    /// whole lifetime - matching `indent.rs`'s own `CursorTestGuard`
    /// precedent.
    struct CurbufGuard {
        prev_curbuf: *mut BufT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurbufGuard {
        fn set(buf: *mut BufT) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = CurbufGuard { prev_curbuf: globals.curbuf, _lock };
            globals.curbuf = buf;
            guard
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.prev_curbuf;
        }
    }

    fn cindent_on_with(paste: i32, cin: i32, inde: Option<Vec<u8>>) -> bool {
        let mut buf = BufT { b_p_cin: cin, b_p_inde: inde, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf);
        let old_paste = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste = paste;
        let result = unsafe { cindent_on() };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste = old_paste;
        result
    }

    /// Runs `cin_is_cinword` with `cinw` installed as the buffer's own
    /// `'cinwords'`.
    fn cinword_with(cinw: &[u8], line: &[u8]) -> bool {
        let mut buf = BufT {
            b_p_cinw: Some(cinw.to_vec()),
            ..Default::default()
        };
        let _guard = CurbufGuard::set(&mut buf);
        unsafe { cin_is_cinword(line) }
    }

    /// The real default `'cinwords'`, read off a live nvim binary.
    const DEFAULT_CINW: &[u8] = b"if,else,while,do,for,switch";

    // ---- skip_string / is_pos_in_string ----

    #[test]
    fn skip_string_leaves_a_non_string_position_alone() {
        // Nothing string-like at `p`, so the index comes back unchanged.
        assert_eq!(skip_string(b"abc", 0), 0);
        assert_eq!(skip_string(b"abc", 1), 1);
    }

    #[test]
    fn skip_string_skips_a_double_quoted_string() {
        // `"abc"` - lands on the CLOSING quote, not past it, because
        // of the original's own "backup from NUL" step.
        assert_eq!(skip_string(b"\"abc\"", 0), 4);
    }

    #[test]
    fn skip_string_advances_through_an_unterminated_string() {
        // The header comment says an absent string returns the
        // argument unmodified, but that is only true when `p` does
        // not START a string. Once the opening quote is seen the
        // original has already advanced `p`, and an unterminated
        // string runs to the end of the line - then the "backup from
        // NUL" step leaves it on the last byte. Quirk preserved.
        assert_eq!(skip_string(b"\"abc", 0), 3);
    }

    #[test]
    fn skip_string_skips_concatenated_strings_as_one_run() {
        // "date""time" - the loop continues across the join.
        let line = b"\"date\"\"time\"";
        assert_eq!(skip_string(line, 0), line.len() - 1);
    }

    #[test]
    fn skip_string_skips_a_char_constant_and_its_escapes() {
        assert_eq!(skip_string(b"'a'", 0), 2);
        // '\n' - the backslash form.
        assert_eq!(skip_string(b"'\\n'", 0), 3);
        // A lone quote at end of line is not a char constant.
        assert_eq!(skip_string(b"'", 0), 0);
    }

    #[test]
    fn skip_string_does_not_recognize_an_octal_char_constant() {
        // The original's own digit loop overshoots: it advances `i`
        // while `p[i - 1]` is a digit, so it stops with `p[i - 1]`
        // ON the closing quote and `p[i]` one PAST it. The trailing
        // check then tests the wrong byte and fails, leaving `'\000'`
        // unrecognized despite the comment naming it. Faithfully
        // preserved rather than "fixed" - the non-octal `'\n'` form
        // above shows the same code path working as intended.
        assert_eq!(skip_string(b"'\\000'", 0), 0);
        assert_eq!(skip_string(b"'\\0'", 0), 0);
    }

    #[test]
    fn skip_string_handles_an_escaped_quote_inside_a_string() {
        // "a\"b" - the escaped quote must not end the string.
        let line = b"\"a\\\"b\"";
        assert_eq!(skip_string(line, 0), line.len() - 1);
    }

    #[test]
    fn skip_string_skips_a_cpp_raw_string() {
        // R"x(hi)x" - the delimiter is `x`, so only `)x"` ends it.
        let line = b"R\"x(hi)x\"";
        assert_eq!(skip_string(line, 0), line.len() - 1);
        // With no `(` there is no delimiter, so it is not a raw string.
        assert_eq!(skip_string(b"R\"xy", 0), 0);
    }

    #[test]
    fn is_pos_in_string_detects_a_position_inside_a_string() {
        // `"abc"` - index 2 is the `b`, inside the string.
        assert!(is_pos_in_string(b"\"abc\"", 2));
        // `x"y"` - index 2 is the `y`, inside the string.
        assert!(is_pos_in_string(b"x\"y\"", 2));
    }

    #[test]
    fn is_pos_in_string_is_false_outside_a_string() {
        // Plain text is never inside a string.
        assert!(!is_pos_in_string(b"abc", 1));
        // The OPENING quote itself is not "inside" - the loop's own
        // `(p - line) < col` bound exits before scanning it.
        assert!(!is_pos_in_string(b"\"abc\"", 0));
        // Text before a string starts.
        assert!(!is_pos_in_string(b"x\"y\"", 0));
    }

    #[test]
    fn cin_is_cinword_matches_a_keyword_followed_by_a_non_word_char() {
        assert!(cinword_with(DEFAULT_CINW, b"if (x)"));
        assert!(cinword_with(DEFAULT_CINW, b"else"));
        assert!(cinword_with(DEFAULT_CINW, b"do {"));
        assert!(cinword_with(DEFAULT_CINW, b"switch (c)"));
    }

    #[test]
    fn cin_is_cinword_skips_leading_whitespace() {
        assert!(cinword_with(DEFAULT_CINW, b"   while (1)"));
        assert!(cinword_with(DEFAULT_CINW, b"\t\tfor (;;)"));
    }

    #[test]
    fn cin_is_cinword_rejects_a_longer_identifier() {
        // "ifx"/"iffy" start with "if" but continue with word chars,
        // so they are NOT cinwords.
        for line in [&b"ifx"[..], b"iffy", b"switching", b"doing", b"forx"] {
            assert!(
                !cinword_with(DEFAULT_CINW, line),
                "{:?} should not match",
                std::string::String::from_utf8_lossy(line)
            );
        }
    }

    #[test]
    fn cin_is_cinword_rejects_an_unrelated_line() {
        assert!(!cinword_with(DEFAULT_CINW, b"return 0;"));
        assert!(!cinword_with(DEFAULT_CINW, b""));
        // An empty 'cinwords' can never match.
        assert!(!cinword_with(b"", b"if (x)"));
    }

    #[test]
    fn cin_is_cinword_or_condition_lets_a_non_word_final_char_match_anything() {
        // The trailing test is `!vim_iswordc(line[len]) ||
        // !vim_iswordc(line[len - 1])` - an OR, not the `&&` a reader
        // might expect. So an entry ending in a NON-word character
        // matches even when the text continues with a word char.
        assert!(cinword_with(b"if.", b"if.x"));
        // The same entry without its trailing '.' does not.
        assert!(!cinword_with(b"if", b"ifx"));
    }

    #[test]
    fn cindent_on_true_when_cin_set_and_not_pasting() {
        assert!(cindent_on_with(0, 1, None));
    }

    #[test]
    fn cindent_on_true_when_indentexpr_set_and_not_pasting() {
        assert!(cindent_on_with(0, 0, Some(b"MyIndent()".to_vec())));
    }

    #[test]
    fn cindent_on_false_when_neither_cin_nor_indentexpr_set() {
        assert!(!cindent_on_with(0, 0, None));
    }

    #[test]
    fn cindent_on_false_while_pasting_even_if_cin_set() {
        assert!(!cindent_on_with(1, 1, None));
    }

    #[test]
    fn cin_starts_with_exact_match() {
        assert!(cin_starts_with(b"case", b"case"));
    }

    #[test]
    fn cin_starts_with_followed_by_non_identifier() {
        assert!(cin_starts_with(b"case 1:", b"case"));
        assert!(cin_starts_with(b"enum{", b"enum"));
    }

    #[test]
    fn cin_starts_with_rejects_longer_identifier() {
        // "casement" starts with "case" but is followed by an
        // identifier character, so it's a different word entirely.
        assert!(!cin_starts_with(b"casement", b"case"));
    }

    #[test]
    fn cin_starts_with_rejects_wrong_prefix() {
        assert!(!cin_starts_with(b"default:", b"case"));
    }
}
