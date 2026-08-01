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
//! `findsent` (the sibling `(`/`)` sentence-motion primitive) is a
//! genuinely different, substantially more involved algorithm
//! (multi-pass backward/forward scanning with `incl`/`decl`,
//! `'cpoptions'`'s `CPO_ENDOFSENT` flag) - deliberately NOT attempted
//! alongside `findpar` in the same pass, matching this crate's
//! established "don't rush a complex function" precedent. `mark.rs`'s
//! own `mark_get_motion` therefore only handles `{`/`}` for real;
//! `(`/`)` still `unimplemented!()`s, needing `findsent`.
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

/// Return `true` if line `lnum` is the start of a section or paragraph
/// (`startPS`). If `para` is `'{'`/`'}'` only check for sections. If
/// `both` is true also stop at `'}'`.
///
/// # Safety
/// Same as [`crate::memline::ml_get`] (`curbuf` must be a valid,
/// non-null pointer to a live buffer whose own memline is in a
/// well-formed state).
#[must_use]
unsafe fn start_ps(lnum: LinenrT, para: i32, both: bool) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    /// appends the trailing NUL itself before calling `ml_append_buf`,
    /// matching that verified-correct convention precisely, so that
    /// `ml_get_len`'s own `- 1` accounting (`ml_get_buf_len`) reports
    /// the real, correct content length afterward.
    unsafe fn buf_with_lines(first_line: &[u8], rest: &[&[u8]]) -> crate::buffer_defs::BufT {
        let mut buf = crate::buffer_defs::BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, first_line) },
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
}

