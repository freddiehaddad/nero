//! Translated from `src/nvim/normal.c` (tractable core only).
//!
//! `normal.c` (~6600 lines) is the Normal-mode command-dispatch engine
//! (the giant `normal_cmd`/`nv_*` handler table) - almost none of it
//! is tractable, since it needs real buffer modification, the redraw
//! pipeline, the regex engine, and the whole rest of the editing
//! subsystem, none of which are translated yet.
//!
//! Translated: [`is_ident`] - a small, pure, self-contained C-style-
//! comment/string-literal scanner. Translated ahead of its own real
//! caller (`find_decl`, the `"gd"`/`"gD"` variable-declaration search,
//! not translated - needs `find_ident_under_cursor`/`searchit`, the
//! real regex engine), matching this crate's established "small,
//! self-contained, no design freedom to get wrong" precedent.
//!
//! Deferred: everything else in the file.

/// Returns `true` if `line[offset]` is NOT inside a C-style comment or
/// string, `false` otherwise (`is_ident`).
///
/// Assumes `line` is a well-formed line (this crate's own convention:
/// includes its own trailing NUL) - running out of a malformed,
/// non-NUL-terminated slice before reaching `offset` is treated the
/// same way as hitting the terminator, matching `mbyte.c`/`indent.c`'s
/// established "ran out of slice = terminator" precedent.
#[must_use]
pub fn is_ident(line: &[u8], offset: i32) -> bool {
    let mut incomment = false;
    let mut instring: u8 = 0;
    let mut prev: u8 = 0;

    let offset = offset.max(0) as usize;
    let mut i = 0usize;
    while i < offset {
        let Some(&c) = line.get(i) else { break };
        if c == 0 {
            break;
        }

        if instring != 0 {
            if prev != b'\\' && c == instring {
                instring = 0;
            }
        } else if (c == b'"' || c == b'\'') && !incomment {
            instring = c;
        } else if incomment {
            if prev == b'*' && c == b'/' {
                incomment = false;
            }
        } else if prev == b'/' && c == b'*' {
            incomment = true;
        } else if prev == b'/' && c == b'/' {
            return false;
        }

        prev = c;
        i += 1;
    }

    !incomment && instring == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_ident_plain_code_before_offset_is_true() {
        assert!(is_ident(b"int x = 5;\0", 5));
    }

    #[test]
    fn is_ident_inside_a_double_quoted_string_is_false() {
        // offset=6 lands right after the opening quote, inside "hi".
        assert!(!is_ident(b"x = \"hi\";\0", 6));
    }

    #[test]
    fn is_ident_after_a_closed_string_is_true() {
        // offset=9 is right after the closing quote - the string has
        // ended, so this position is NOT inside it.
        assert!(is_ident(b"x = \"hi\";\0", 9));
    }

    #[test]
    fn is_ident_inside_a_single_quoted_string_is_false() {
        assert!(!is_ident(b"c = 'x';\0", 5));
    }

    #[test]
    fn is_ident_an_escaped_quote_does_not_close_the_string() {
        // `"a\"b"` bytes: 0='"',1='a',2='\\',3='"',4='b',5='"',6=NUL.
        // The backslash-escaped quote at index 3 must NOT close the
        // string; offset=4 (the 'b') is still inside it.
        assert!(!is_ident(b"\"a\\\"b\"\0", 4));
    }

    #[test]
    fn is_ident_inside_a_block_comment_is_false() {
        assert!(!is_ident(b"/* comment */ x\0", 5));
    }

    #[test]
    fn is_ident_after_a_closed_block_comment_is_true() {
        assert!(is_ident(b"/* c */ x\0", 8));
    }

    #[test]
    fn is_ident_a_line_comment_makes_everything_after_it_false() {
        // Once `//` is seen, the function returns false immediately
        // (a line comment runs to the end of the line - there is no
        // "closing" it within the same line).
        assert!(!is_ident(b"x // comment\0", 4));
        assert!(!is_ident(b"x // comment\0", 12));
    }

    #[test]
    fn is_ident_offset_zero_is_always_true() {
        // The loop never runs at all - nothing has been scanned yet,
        // so we're trivially "not inside" anything.
        assert!(is_ident(b"\"unterminated\0", 0));
    }

    #[test]
    fn is_ident_stops_at_a_truncated_non_nul_terminated_slice() {
        // No NUL terminator at all - running out of the slice before
        // reaching `offset` is treated the same as hitting one.
        assert!(is_ident(b"abc", 10));
    }
}
