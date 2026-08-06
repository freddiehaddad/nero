//! Translated from `src/nvim/textobject.c` (tractable core only).
//!
//! `textobject.c` (~2500 lines) implements the word/sentence/paragraph/
//! quoted-string/bracket text-object motions (`aw`/`iw`, `as`/`is`,
//! `ap`/`ip`, `a"`/`i"`, etc.) plus their underlying single-motion
//! primitives (`{`/`}`/`(`/`)`). Most of it is substantial (word-class
//! scanning state machines, quote-matching, bracket-matching) and not
//! attempted here.
//!
//! Translated: `inmacro`/`start_ps` (`startPS`)/[`findpar`] - the
//! `{`/`}` (paragraph/section) motion primitive, needed by `mark.c`'s
//! `mark_get_motion` (the `'{'`/`'}'` marks-that-are-really-motions).
//!
//! Also translated: [`findsent`] - the sibling `(`/`)` sentence-motion
//! primitive, needing `memline.rs`'s new `gchar_pos` accessor. Hand-
//! traced against several concrete multi-sentence examples (plain
//! `". "` boundaries, trailing quote/bracket characters after
//! punctuation, an empty-line skip, and the `'cpoptions'` `CPO_ENDOFSENT`
//! double-space requirement) before writing any test - every scenario
//! passed on the first real run. `mark.rs`'s own `mark_get_motion`
//! therefore now handles `(`/`)` for real too, alongside `{`/`}`.
//!
//! Deferred: everything else in the file.

#[cfg(test)]
use crate::buffer_defs::WinT;
use crate::globals::GLOBALS;
use crate::pos_defs::LinenrT;
#[cfg(test)]
use crate::pos_defs::PosT;
use crate::vim_defs::Direction;

/// Check if the string at `s` is an nroff macro that is in option
/// `opt` (`inmacro`). Both are pairs-of-characters tables (e.g.
/// `'sections'`/`'paragraphs'`'s own real string values), matching the
/// original's own `char[2]`-at-a-time scan exactly - a byte past the
/// end of either slice is treated as NUL (0), matching the original's
/// own NUL-terminated-C-string semantics for this exact situation
/// (nothing here ever reads past a genuine "no more macro pairs to
/// check"/"end of `s`" condition).
#[must_use]
fn inmacro(opt: &[u8], s: &[u8]) -> bool {
    let s0 = s.first().copied().unwrap_or(0);
    let s1 = s.get(1).copied().unwrap_or(0);

    let mut i = 0usize;
    loop {
        if i >= opt.len() {
            return false; // macro[0] == NUL
        }
        let m0 = opt[i];
        let m1 = opt.get(i + 1).copied().unwrap_or(0);

        let match0 = m0 == s0 || (m0 == b' ' && (s0 == 0 || s0 == b' '));
        let match1 = m1 == s1 || ((m1 == 0 || m1 == b' ') && (s0 == 0 || s1 == 0 || s1 == b' '));
        if match0 && match1 {
            return true; // break - macro[0] (m0) is still != NUL here
        }

        // macro++ (the loop body's own increment)
        i += 1;
        if i >= opt.len() {
            return false; // macro[0] == NUL
        }
        // the `for` loop's own increment clause
        i += 1;
    }
}

/// Find the next occurrence of `quotechar` in `line`, starting at
/// `col` (`find_next_quote`).
///
/// Returns the column of the quote, or `None` when the end of the
/// line is reached first. A character listed in `escape` protects the
/// character after it, so an escaped quote is skipped over.
///
/// # Safety
/// Same as [`crate::mbyte::utfc_ptr2len`].
#[must_use]
pub unsafe fn find_next_quote(
    line: &[u8],
    col: i32,
    quotechar: i32,
    escape: Option<&[u8]>,
) -> Option<i32> {
    let mut col = col;
    let at = |i: i32| {
        if i < 0 {
            0
        } else {
            line.get(i as usize).copied().unwrap_or(0)
        }
    };
    loop {
        let c = i32::from(at(col));
        if c == 0 {
            return None;
        } else if escape.is_some_and(|e| crate::strings::vim_strchr(e, c).is_some()) {
            col += 1;
            if at(col) == 0 {
                return None;
            }
        } else if c == quotechar {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        col += unsafe { crate::mbyte::utfc_ptr2len(&line[(col.max(0) as usize).min(line.len())..]) };
    }
    Some(col)
}

/// Find the previous occurrence of `quotechar` in `line`, searching
/// backwards from `col_start` (`find_prev_quote`).
///
/// Returns the column of the quote, or `0` when none is found - the
/// original reports the loop's own end position rather than a
/// sentinel.
///
/// An odd run of `escape` characters immediately before a candidate
/// means that candidate is escaped, so it is skipped.
///
/// # Safety
/// Same as [`crate::mbyte::utf_head_off`].
#[must_use]
pub unsafe fn find_prev_quote(
    line: &[u8],
    col_start: i32,
    quotechar: i32,
    escape: Option<&[u8]>,
) -> i32 {
    let mut col_start = col_start;
    let at = |i: i32| {
        if i < 0 {
            0
        } else {
            line.get(i as usize).copied().unwrap_or(0)
        }
    };
    while col_start > 0 {
        col_start -= 1;
        // SAFETY: forwarded from this function's own safety doc.
        col_start -= unsafe {
            crate::mbyte::utf_head_off(line, (col_start.max(0) as usize).min(line.len()))
        };
        let mut n = 0;
        if let Some(e) = escape {
            while col_start - n > 0
                && crate::strings::vim_strchr(e, i32::from(at(col_start - n - 1))).is_some()
            {
                n += 1;
            }
        }
        if n & 1 != 0 {
            // An uneven number of escape chars, so skip it.
            col_start -= n;
        } else if i32::from(at(col_start)) == quotechar {
            break;
        }
    }
    col_start
}

/// Return `true` if line `lnum` is the start of a section or paragraph
/// (`startPS`). If `para` is `'{'`/`'}'` only check for sections. If
/// `both` is true also stop at `'}'`.
///
/// # Safety
/// Same as [`crate::memline::ml_get`] (`curbuf` must be a valid,
/// non-null pointer to a live buffer whose own memline is in a
/// well-formed state).
#[must_use]
pub unsafe fn start_ps(lnum: LinenrT, para: i32, both: bool) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let s = unsafe { crate::memline::ml_get(lnum) };
    let c0 = s.first().copied().unwrap_or(0);
    if i32::from(c0) == para || c0 == 0x0c || (both && c0 == b'}') {
        return true;
    }
    if c0 == b'.' {
        let rest = &s[1.min(s.len())..];
        // SAFETY: forwarded from this function's own safety doc.
        let opt_vars = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let p_sections = opt_vars.p_sections.as_deref().unwrap_or(&[]);
        let p_para = opt_vars.p_para.as_deref().unwrap_or(&[]);
        if inmacro(p_sections, rest) || (para == 0 && inmacro(p_para, rest)) {
            return true;
        }
    }
    false
}

/// Find the start of a paragraph or section, `count` times in
/// direction `dir` (`findpar`, the `{`/`}` motion primitive). `what`
/// is `0` (`NUL`) for a plain paragraph search, or a specific
/// character (e.g. `'{'`) to search for a section only. `both` also
/// stops at a line starting with `'}'`.
///
/// On success, sets `curwin.w_cursor` to the found position (and, for
/// a forward search landing exactly on the last line, sets `*pincl`
/// and lands the cursor on the last real character of that line
/// rather than column 0) and returns `true`. Returns `false` if the
/// buffer's start/end was reached before `count` boundaries were
/// found (except the tolerated "reached it on exactly the last
/// requested iteration" case - matches the original's own
/// `while (count--)`/`if (count) return false;` structure exactly).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` whose own `w_buffer` is also valid and
/// well-formed.
pub unsafe fn findpar(pincl: &mut bool, dir: Direction, mut count: i32, what: i32, both: bool) -> bool {
    let dir_i = dir as i32;
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let mut curr = unsafe { (*curwin).w_cursor.lnum };

    loop {
        // `while (count--)`: the condition reads `count` BEFORE the
        // decrement takes effect.
        let cond = count != 0;
        count -= 1;
        if !cond {
            break;
        }

        let mut did_skip = false;
        let mut first = true;
        loop {
            // SAFETY: forwarded from this function's own safety doc.
            let line = unsafe { crate::memline::ml_get(curr) };
            if line.first().copied().unwrap_or(0) != 0 {
                did_skip = true;
            }

            let mut fold_skipped = false;
            if first {
                let mut fold_first: LinenrT = 0;
                let mut fold_last: LinenrT = 0;
                // SAFETY: forwarded from this function's own safety doc.
                let has_fold = unsafe {
                    crate::fold::has_folding(&mut *curwin, curr, Some(&mut fold_first), Some(&mut fold_last))
                };
                if has_fold {
                    curr = (if dir_i > 0 { fold_last } else { fold_first }) + dir_i;
                    fold_skipped = true;
                }
            }

            // SAFETY: forwarded from this function's own safety doc.
            if !first && did_skip && unsafe { start_ps(curr, what, both) } {
                break;
            }

            if fold_skipped {
                curr -= dir_i;
            }
            curr += dir_i;
            // SAFETY: forwarded from this function's own safety doc.
            let line_count = unsafe { (*GLOBALS.get_mut().curbuf).b_ml.ml_line_count };
            if curr < 1 || curr > line_count {
                if count != 0 {
                    return false;
                }
                curr -= dir_i;
                break;
            }

            first = false;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::mark::setpcmark() };
    // SAFETY: forwarded from this function's own safety doc.
    let line0 = unsafe { crate::memline::ml_get(curr) };
    if both && line0.first().copied() == Some(b'}') {
        curr += 1;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*curwin).w_cursor.lnum = curr };

    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { (*GLOBALS.get_mut().curbuf).b_ml.ml_line_count };
    if curr == line_count && what != i32::from(b'}') && dir == Direction::Forward {
        // SAFETY: forwarded from this function's own safety doc.
        let line_bytes = unsafe { crate::memline::ml_get(curr) };
        // SAFETY: forwarded from this function's own safety doc.
        let len = unsafe { crate::memline::ml_get_len(curr) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*curwin).w_cursor.col = len };
        if len != 0 {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*curwin).w_cursor.col -= 1 };
            // SAFETY: forwarded from this function's own safety doc.
            let col = unsafe { (*curwin).w_cursor.col };
            // SAFETY: forwarded from this function's own safety doc.
            let head_off = unsafe { crate::mbyte::utf_head_off(&line_bytes, col as usize) };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*curwin).w_cursor.col -= head_off };
            *pincl = true;
        }
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*curwin).w_cursor.col = 0 };
    }
    true
}

/// Find the start of the next sentence, `count` times in direction
/// `dir` (`findsent`, the `(`/`)` motion primitive). See `":h
/// sentence"` for the precise definition of a "sentence" text object.
///
/// On success, sets `curwin.w_cursor` to the found position and
/// returns `true`. Returns `false` if the buffer's start/end was
/// reached before `count` boundaries were found (except the tolerated
/// "reached it on exactly the last requested iteration" case, matching
/// [`findpar`]'s own established convention for the identical
/// `while (count--)`/`if (count) return false;` structure).
///
/// # Safety
/// Same as [`findpar`] (touches `curwin`/`curbuf` throughout).
pub unsafe fn findsent(dir: Direction, mut count: i32) -> bool {
    let mut noskip = false; // do not skip blanks

    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let mut pos = unsafe { (*curwin).w_cursor };

    let func: unsafe fn(&mut crate::pos_defs::PosT) -> i32 = if dir == Direction::Forward {
        crate::memline::incl
    } else {
        crate::memline::decl
    };

    loop {
        // `while (count--)`: the condition reads `count` BEFORE the
        // decrement takes effect.
        let cond = count != 0;
        count -= 1;
        if !cond {
            break;
        }

        let prev_pos = pos;

        'body: {
            // if on an empty line, skip up to a non-empty line
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { crate::memline::gchar_pos(&pos) } == 0 {
                loop {
                    // SAFETY: forwarded from this function's own safety doc.
                    if unsafe { func(&mut pos) } == -1 {
                        break;
                    }
                    // SAFETY: forwarded from this function's own safety doc.
                    if unsafe { crate::memline::gchar_pos(&pos) } != 0 {
                        break;
                    }
                }
                if dir == Direction::Forward {
                    break 'body; // goto found
                }
                // if on the start of a paragraph or a section and
                // searching forward, go to the next line
            } else if dir == Direction::Forward
                && pos.col == 0
                // SAFETY: forwarded from this function's own safety doc.
                && unsafe { start_ps(pos.lnum, 0, false) }
            {
                // SAFETY: forwarded from this function's own safety doc.
                let line_count = unsafe { (*GLOBALS.get_mut().curbuf).b_ml.ml_line_count };
                if pos.lnum == line_count {
                    return false;
                }
                pos.lnum += 1;
                break 'body; // goto found
            } else if dir == Direction::Backward {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::memline::decl(&mut pos) };
            }

            // go back to the previous non-white non-punctuation
            // character
            let mut found_dot = false;
            loop {
                // SAFETY: forwarded from this function's own safety doc.
                let c = unsafe { crate::memline::gchar_pos(&pos) };
                if !(crate::ascii_defs::ascii_iswhite(c)
                    || crate::strings::vim_strchr(b".!?)]\"'", c).is_some())
                {
                    break;
                }
                let mut tpos = pos;
                // SAFETY: forwarded from this function's own safety doc.
                let dec_failed = unsafe { crate::memline::decl(&mut tpos) } == -1;
                // SAFETY: forwarded from this function's own safety doc.
                let tpos_line_empty = unsafe { crate::memline::ml_get_len(tpos.lnum) } == 0;
                if dec_failed || (tpos_line_empty && dir == Direction::Forward) {
                    break;
                }
                if found_dot {
                    break;
                }
                if crate::strings::vim_strchr(b".!?", c).is_some() {
                    found_dot = true;
                }
                if crate::strings::vim_strchr(b")]\"'", c).is_some()
                    && crate::strings::vim_strchr(
                        b".!?)]\"'",
                        // SAFETY: forwarded from this function's own safety doc.
                        unsafe { crate::memline::gchar_pos(&tpos) },
                    )
                    .is_none()
                {
                    break;
                }
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::memline::decl(&mut pos) };
            }

            // remember the line where the search started
            let startlnum = pos.lnum;
            // SAFETY: forwarded from this function's own safety doc.
            let opt_vars = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let cpo_j =
                crate::strings::vim_strchr(opt_vars.p_cpo.as_deref().unwrap_or(&[]), i32::from(b'J')).is_some();

            // find end of sentence
            loop {
                // SAFETY: forwarded from this function's own safety doc.
                let c = unsafe { crate::memline::gchar_pos(&pos) };
                // SAFETY: forwarded from this function's own safety doc.
                if c == 0 || (pos.col == 0 && unsafe { start_ps(pos.lnum, 0, false) }) {
                    if dir == Direction::Backward && pos.lnum != startlnum {
                        pos.lnum += 1;
                    }
                    break;
                }
                if c == i32::from(b'.') || c == i32::from(b'!') || c == i32::from(b'?') {
                    let mut tpos = pos;
                    let mut c2;
                    loop {
                        // SAFETY: forwarded from this function's own safety doc.
                        c2 = unsafe { crate::memline::inc(&mut tpos) };
                        if c2 == -1 {
                            break;
                        }
                        // SAFETY: forwarded from this function's own safety doc.
                        c2 = unsafe { crate::memline::gchar_pos(&tpos) };
                        if crate::strings::vim_strchr(b")]\"'", c2).is_none() {
                            break;
                        }
                    }
                    // SAFETY: forwarded from this function's own safety doc.
                    let take_branch = c2 == -1
                        || (!cpo_j && (c2 == i32::from(b' ') || c2 == i32::from(b'\t')))
                        || c2 == 0
                        || (cpo_j
                            && c2 == i32::from(b' ')
                            && unsafe { crate::memline::inc(&mut tpos) } >= 0
                            && unsafe { crate::memline::gchar_pos(&tpos) } == i32::from(b' '));
                    if take_branch {
                        pos = tpos;
                        // SAFETY: forwarded from this function's own safety doc.
                        if unsafe { crate::memline::gchar_pos(&pos) } == 0 {
                            // skip NUL at EOL
                            // SAFETY: forwarded from this function's own safety doc.
                            unsafe { crate::memline::inc(&mut pos) };
                        }
                        break;
                    }
                }
                // SAFETY: forwarded from this function's own safety doc.
                if unsafe { func(&mut pos) } == -1 {
                    if count != 0 {
                        return false;
                    }
                    noskip = true;
                    break;
                }
            }
        } // found:

        // skip white space
        if !noskip {
            loop {
                // SAFETY: forwarded from this function's own safety doc.
                let c = unsafe { crate::memline::gchar_pos(&pos) };
                if c != i32::from(b' ') && c != i32::from(b'\t') {
                    break;
                }
                // SAFETY: forwarded from this function's own safety doc.
                if unsafe { crate::memline::incl(&mut pos) } == -1 {
                    break;
                }
            }
        }

        if crate::mark_defs::equalpos(prev_pos, pos) {
            // didn't actually move, advance one character and try
            // again
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { func(&mut pos) } == -1 {
                if count != 0 {
                    return false;
                }
                break;
            }
            count += 1;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::mark::setpcmark() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*curwin).w_cursor = pos };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_next_quote_finds_the_first_unescaped_quote() {
        let line = b"say \"hi\" now\0";
        assert_eq!(
            unsafe { find_next_quote(line, 0, i32::from(b'"'), None) },
            Some(4)
        );
        // Starting past the first quote finds the closing one.
        assert_eq!(
            unsafe { find_next_quote(line, 5, i32::from(b'"'), None) },
            Some(7)
        );
    }

    #[test]
    fn find_next_quote_returns_none_at_the_end_of_the_line() {
        let line = b"no quotes here\0";
        assert_eq!(
            unsafe { find_next_quote(line, 0, i32::from(b'"'), None) },
            None
        );
    }

    #[test]
    fn find_next_quote_skips_an_escaped_quote() {
        // The backslash protects the quote after it, so the match is
        // the LATER one.
        let line = b"a\\\"b\"c\0";
        assert_eq!(
            unsafe { find_next_quote(line, 0, i32::from(b'"'), Some(b"\\")) },
            Some(4)
        );
        // Without an escape list that same quote does match.
        assert_eq!(
            unsafe { find_next_quote(line, 0, i32::from(b'"'), None) },
            Some(2)
        );
    }

    #[test]
    fn find_next_quote_returns_none_when_an_escape_ends_the_line() {
        let line = b"ab\\\0";
        assert_eq!(
            unsafe { find_next_quote(line, 0, i32::from(b'"'), Some(b"\\")) },
            None
        );
    }

    #[test]
    fn find_prev_quote_searches_backwards() {
        let line = b"say \"hi\" now\0";
        // From just past the closing quote, the previous one is it.
        assert_eq!(unsafe { find_prev_quote(line, 7, i32::from(b'"'), None) }, 4);
        // From the closing quote itself, the previous is the opening.
        assert_eq!(unsafe { find_prev_quote(line, 4, i32::from(b'"'), None) }, 0);
    }

    #[test]
    fn find_prev_quote_skips_an_escaped_quote() {
        // col 3 is an escaped quote; searching back from past it lands
        // on the unescaped one at col 0 instead.
        let line = b"\"a\\\"b\0";
        assert_eq!(
            unsafe { find_prev_quote(line, 4, i32::from(b'"'), Some(b"\\")) },
            0
        );
    }

    #[test]
    fn find_prev_quote_reports_zero_when_none_is_found() {
        // The original returns the loop's own end position rather
        // than a sentinel.
        let line = b"no quotes\0";
        assert_eq!(unsafe { find_prev_quote(line, 5, i32::from(b'"'), None) }, 0);
    }

    #[test]
    fn inmacro_matches_exact_pair() {
        assert!(inmacro(b"IP", b"IPfoo"));
    }

    #[test]
    fn inmacro_matches_second_pair() {
        assert!(inmacro(b"IPLQ", b"LQfoo"));
    }

    #[test]
    fn inmacro_space_in_option_matches_space_or_end_of_string() {
        // "P " (space as the 2nd char of a pair) matches "P" followed by
        // either a real space or the end of the string.
        assert!(inmacro(b"P ", b"P"));
        assert!(inmacro(b"P ", b"P "));
        assert!(!inmacro(b"P ", b"Px"));
    }

    #[test]
    fn inmacro_no_match_returns_false() {
        assert!(!inmacro(b"IPLQ", b"XXfoo"));
    }

    #[test]
    fn inmacro_empty_option_never_matches() {
        assert!(!inmacro(b"", b"IP"));
    }

    // ---- start_ps / findpar ----

    /// Opens `buf` (real block 0/data block allocation via `ml_open`)
    /// with `first_line` as line 1, then appends each of `rest` in
    /// order. Callers must hold
    /// `crate::globals::global_state_test_lock()` for their whole test
    /// body (touches `mf_sync` internally via `ml_open`) and clean up
    /// via [`close_buf`].
    ///
    /// Unlike `ops.rs`'s own same-named helper (whose `ml_append_buf`
    /// calls omit the trailing NUL, apparently a narrow, pre-existing
    /// inconsistency with `memline.rs`'s own established convention,
    /// verified directly against `memline.rs`'s own test suite - e.g.
    /// `ml_append_buf(&mut buf, 1, b"hello\0", 6, false)` - which
    /// passes a slice that ALREADY includes its own trailing NUL, with
    /// `len` covering the WHOLE slice including it), this helper
    /// appends the trailing NUL itself before calling `ml_append_buf`
    /// AND `ml_replace_buf_len` (for `first_line` too - both functions
    /// store `line` byte-for-byte and derive `ml_line_textlen` from its
    /// own length, so BOTH need the same trailing-NUL convention,
    /// confirmed directly by reproducing a real out-of-bounds panic in
    /// `findsent`'s own tests when `first_line` was passed without one,
    /// since `ml_get_buf_len`'s own `ml_line_textlen - 1` accounting
    /// otherwise reports one byte short of the real content length),
    /// matching `memline.rs`'s own verified-correct convention exactly.
    unsafe fn buf_with_lines(first_line: &[u8], rest: &[&[u8]]) -> crate::buffer_defs::BufT {
        let mut buf = crate::buffer_defs::BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        let mut first_owned = first_line.to_vec();
        first_owned.push(0);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, &first_owned) },
            crate::vim_defs::OK
        );
        for (after, line) in (1..).zip(rest.iter()) {
            let mut owned = line.to_vec();
            owned.push(0);
            assert_eq!(
                unsafe { crate::memline::ml_append_buf(&mut buf, after, &owned, owned.len() as i32, false) },
                crate::vim_defs::OK
            );
        }
        buf
    }

    unsafe fn close_buf(buf: crate::buffer_defs::BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    /// RAII guard pointing `GLOBALS.curwin`/`curbuf` at `win`/`buf` for
    /// the guard's lifetime, restoring the previous values on drop
    /// (including through a panic). Also resets `cmdmod.cmod_flags`/
    /// `global_busy`/`listcmd_busy` to their own "nothing special
    /// active" defaults (`setpcmark`, called by `findpar`, reads all
    /// three). Does NOT self-lock (unlike `mark.rs`'s own
    /// `MarkTestGuard`): every test here also needs the SAME lock held
    /// across its own earlier `buf_with_lines` call (which touches
    /// `GLOBALS` transitively via `ml_open`/`mf_sync`), so callers
    /// must acquire `global_state_test_lock()` themselves and hold it
    /// for the whole test body, matching `mark.rs`'s own
    /// `FirstwinGuard`/`NamedfmGuard` precedent for this exact
    /// composability reason.
    struct TextobjectTestGuard {
        prev_curwin: *mut WinT,
        prev_curbuf: *mut crate::buffer_defs::BufT,
        prev_cmod_flags: i32,
        prev_global_busy: i32,
        prev_listcmd_busy: bool,
    }

    impl TextobjectTestGuard {
        fn set(win: *mut WinT, buf: *mut crate::buffer_defs::BufT) -> Self {
            let g = unsafe { GLOBALS.get_mut() };
            let guard = TextobjectTestGuard {
                prev_curwin: g.curwin,
                prev_curbuf: g.curbuf,
                prev_cmod_flags: g.cmdmod.cmod_flags,
                prev_global_busy: g.global_busy,
                prev_listcmd_busy: g.listcmd_busy,
            };
            g.curwin = win;
            g.curbuf = buf;
            g.cmdmod.cmod_flags = 0;
            g.global_busy = 0;
            g.listcmd_busy = false;
            guard
        }
    }

    impl Drop for TextobjectTestGuard {
        fn drop(&mut self) {
            let g = unsafe { GLOBALS.get_mut() };
            g.curwin = self.prev_curwin;
            g.curbuf = self.prev_curbuf;
            g.cmdmod.cmod_flags = self.prev_cmod_flags;
            g.global_busy = self.prev_global_busy;
            g.listcmd_busy = self.prev_listcmd_busy;
        }
    }

    #[test]
    fn start_ps_true_for_empty_line_when_para_is_nul() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = unsafe { buf_with_lines(b"hello", &[b""]) };
        let mut win = WinT { w_buffer: &mut buf as *mut _, ..Default::default() };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        assert!(unsafe { start_ps(2, 0, false) });
        assert!(!unsafe { start_ps(1, 0, false) }); // "hello" is not empty

        unsafe { close_buf(buf) };
    }

    #[test]
    fn start_ps_true_for_formfeed_and_matching_brace() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = unsafe { buf_with_lines(b"\x0cpage break", &[b"}end"]) };
        let mut win = WinT { w_buffer: &mut buf as *mut _, ..Default::default() };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        assert!(unsafe { start_ps(1, i32::from(b'{'), false) }); // formfeed always matches
        assert!(!unsafe { start_ps(2, i32::from(b'{'), false) }); // '}' only matches when both=true
        assert!(unsafe { start_ps(2, i32::from(b'{'), true) });

        unsafe { close_buf(buf) };
    }

    #[test]
    fn findpar_forward_stops_at_the_next_blank_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = unsafe {
            buf_with_lines(
                b"para one line 1",
                &[b"para one line 2", b"", b"para two line 1", b"para two line 2"],
            )
        };
        buf.b_ml.ml_line_count = 5;
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        let mut pincl = false;
        let ok = unsafe { findpar(&mut pincl, Direction::Forward, 1, 0, false) };

        assert!(ok);
        assert!(!pincl);
        let curwin = unsafe { &*GLOBALS.get_mut().curwin };
        assert_eq!(curwin.w_cursor, PosT { lnum: 3, col: 0, coladd: 0 });

        unsafe { close_buf(buf) };
    }

    #[test]
    fn findpar_forward_count_2_skips_past_the_first_boundary() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = unsafe {
            buf_with_lines(
                b"para one",
                &[b"", b"para two", b"", b"para three"],
            )
        };
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        let mut pincl = false;
        let ok = unsafe { findpar(&mut pincl, Direction::Forward, 2, 0, false) };

        assert!(ok);
        let curwin = unsafe { &*GLOBALS.get_mut().curwin };
        assert_eq!(curwin.w_cursor.lnum, 4); // the SECOND blank line

        unsafe { close_buf(buf) };
    }

    #[test]
    fn findpar_backward_search() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = unsafe {
            buf_with_lines(
                b"para one line 1",
                &[b"para one line 2", b"", b"para two line 1", b"para two line 2"],
            )
        };
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 5, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        let mut pincl = false;
        let ok = unsafe { findpar(&mut pincl, Direction::Backward, 1, 0, false) };

        assert!(ok);
        let curwin = unsafe { &*GLOBALS.get_mut().curwin };
        assert_eq!(curwin.w_cursor.lnum, 3); // the blank line, searching backward

        unsafe { close_buf(buf) };
    }

    #[test]
    fn findpar_reaching_the_end_before_count_is_satisfied_fails() {
        let _lock = crate::globals::global_state_test_lock();
        // No blank line anywhere - only 1 real paragraph boundary exists
        // (the buffer's own end/start), so requesting count=2 forward
        // boundaries must fail (the first "boundary" reached is the
        // buffer's own end on a NON-final requested iteration).
        let mut buf = unsafe { buf_with_lines(b"line one", &[b"line two", b"line three"]) };
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        let mut pincl = false;
        let ok = unsafe { findpar(&mut pincl, Direction::Forward, 2, 0, false) };

        assert!(!ok);

        unsafe { close_buf(buf) };
    }

    #[test]
    fn findpar_forward_landing_on_the_last_line_sets_pincl_and_adjusts_column() {
        let _lock = crate::globals::global_state_test_lock();
        // No blank line - findpar walks all the way to the buffer's own
        // last line (count=1's own "tolerated boundary" case), landing
        // exactly on it - triggering the special "last line, forward,
        // what != '}'" branch.
        let mut buf = unsafe { buf_with_lines(b"line one", &[b"line two", b"abc"]) };
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        let mut pincl = false;
        let ok = unsafe { findpar(&mut pincl, Direction::Forward, 1, 0, false) };

        assert!(ok);
        assert!(pincl);
        let curwin = unsafe { &*GLOBALS.get_mut().curwin };
        assert_eq!(curwin.w_cursor.lnum, 3);
        assert_eq!(curwin.w_cursor.col, 2); // "abc" - last real char is index 2

        unsafe { close_buf(buf) };
    }

    #[test]
    fn findpar_both_true_stops_at_a_line_starting_with_close_brace() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = unsafe { buf_with_lines(b"text", &[b"}end of section"]) };
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        let mut pincl = false;
        let ok = unsafe { findpar(&mut pincl, Direction::Forward, 1, 0, true) };

        assert!(ok);
        let curwin = unsafe { &*GLOBALS.get_mut().curwin };
        assert_eq!(curwin.w_cursor.lnum, 3); // both=true: curr++ past the '}' line

        unsafe { close_buf(buf) };
    }

    // ---- findsent ----

    /// Sets `OPTION_VARS.p_cpo` to `new_cpo` for the guard's whole
    /// lifetime, restoring the previous value on drop - `findsent`
    /// reads `'cpoptions'`'s own `CPO_ENDOFSENT` (`'J'`) flag.
    struct CpoGuard {
        prev: Option<Vec<u8>>,
    }

    impl CpoGuard {
        fn set(new_cpo: Option<&[u8]>) -> Self {
            let opt_vars = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard = CpoGuard { prev: opt_vars.p_cpo.take() };
            opt_vars.p_cpo = new_cpo.map(<[u8]>::to_vec);
            guard
        }
    }

    impl Drop for CpoGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo = self.prev.take();
        }
    }

    #[test]
    fn findsent_forward_moves_to_the_next_sentence_start() {
        let _lock = crate::globals::global_state_test_lock();
        let _cpo = CpoGuard::set(None);
        let mut buf = unsafe { buf_with_lines(b"Hello world. Foo bar.", &[]) };
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        let ok = unsafe { findsent(Direction::Forward, 1) };

        assert!(ok);
        let curwin = unsafe { &*GLOBALS.get_mut().curwin };
        // "Hello world. Foo bar." - col 13 is 'F', the start of the
        // next sentence (hand-traced: the '.' at col 11 is followed by
        // a single space at col 12, which the default (non-cpo-J)
        // rule accepts as a real sentence end; the trailing
        // "skip white space" step then advances past that one space).
        assert_eq!(curwin.w_cursor, PosT { lnum: 1, col: 13, coladd: 0 });

        unsafe { close_buf(buf) };
    }

    #[test]
    fn findsent_backward_moves_to_the_previous_sentence_start() {
        let _lock = crate::globals::global_state_test_lock();
        let _cpo = CpoGuard::set(None);
        let mut buf = unsafe { buf_with_lines(b"Hello world. Foo bar.", &[]) };
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 1, col: 13, coladd: 0 }, // start of "Foo bar."
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        let ok = unsafe { findsent(Direction::Backward, 1) };

        assert!(ok);
        let curwin = unsafe { &*GLOBALS.get_mut().curwin };
        assert_eq!(curwin.w_cursor, PosT { lnum: 1, col: 0, coladd: 0 }); // "Hello..."

        unsafe { close_buf(buf) };
    }

    #[test]
    fn findsent_backward_at_buffer_start_with_count_1_is_a_graceful_noop() {
        let _lock = crate::globals::global_state_test_lock();
        let _cpo = CpoGuard::set(None);
        let mut buf = unsafe { buf_with_lines(b"Hello.", &[]) };
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        // Hand-traced: searching backward from the very start of the
        // buffer with count=1 does NOT fail - `decl` failing on its
        // OWN last requested iteration (count already at 0) is
        // tolerated exactly like `findpar`'s own established
        // "reached it on exactly the last iteration" convention,
        // leaving the cursor unmoved and returning `true`.
        let ok = unsafe { findsent(Direction::Backward, 1) };

        assert!(ok);
        let curwin = unsafe { &*GLOBALS.get_mut().curwin };
        assert_eq!(curwin.w_cursor, PosT { lnum: 1, col: 0, coladd: 0 });

        unsafe { close_buf(buf) };
    }

    #[test]
    fn findsent_backward_at_buffer_start_with_count_2_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let _cpo = CpoGuard::set(None);
        let mut buf = unsafe { buf_with_lines(b"Hello.", &[]) };
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        // Unlike count=1, requesting a SECOND backward sentence boundary
        // genuinely fails: `decl` fails on the FIRST of the two
        // requested iterations (count still nonzero afterward), which
        // is the real, non-tolerated failure path.
        let ok = unsafe { findsent(Direction::Backward, 2) };

        assert!(!ok);

        unsafe { close_buf(buf) };
    }

    #[test]
    fn findsent_cpo_endofsent_requires_a_double_space_after_the_period() {
        let _lock = crate::globals::global_state_test_lock();
        let _cpo = CpoGuard::set(Some(b"J"));
        // A single space after the period is NOT enough when 'cpoptions'
        // includes 'J' - the search must continue past it (there's no
        // real sentence break here at all, so `)` just goes to the very
        // end of the line/buffer instead - the last `char`'s own
        // `func` call fails at end-of-buffer, tolerated since count=1).
        let mut buf = unsafe { buf_with_lines(b"Hello world. Foo bar.", &[]) };
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        let ok = unsafe { findsent(Direction::Forward, 1) };

        assert!(ok);
        let curwin = unsafe { &*GLOBALS.get_mut().curwin };
        // Reaches the end of the (single-line) buffer without ever
        // finding a real "double space" sentence break.
        assert_eq!(curwin.w_cursor.lnum, 1);
        assert_eq!(curwin.w_cursor.col, 21); // one past the final '.', at the buffer's own end (col 20 is '.', col 21 is the trailing NUL/EOL position)

        unsafe { close_buf(buf) };
    }

    #[test]
    fn findsent_cpo_endofsent_accepts_a_genuine_double_space() {
        let _lock = crate::globals::global_state_test_lock();
        let _cpo = CpoGuard::set(Some(b"J"));
        let mut buf = unsafe { buf_with_lines(b"Hello world.  Foo bar.", &[]) };
        let mut win = WinT {
            w_buffer: &mut buf as *mut _,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = TextobjectTestGuard::set(&mut win as *mut WinT, &mut buf as *mut _);

        let ok = unsafe { findsent(Direction::Forward, 1) };

        assert!(ok);
        let curwin = unsafe { &*GLOBALS.get_mut().curwin };
        // "Hello world.  Foo bar." - 2 spaces after the period (cols
        // 12-13), "Foo" starts at col 14.
        assert_eq!(curwin.w_cursor, PosT { lnum: 1, col: 14, coladd: 0 });

        unsafe { close_buf(buf) };
    }
}


