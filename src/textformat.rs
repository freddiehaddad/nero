//! Translated from `src/nvim/textformat.c` (tractable core only).
//!
//! `textformat.c` (~900 lines) implements automatic/manual text
//! formatting (the `gq`/`gw` operators, `'formatoptions'`-driven
//! auto-wrap while inserting text). Almost every function needs real
//! buffer modification (`u_save`/`ml_replace`/`del_lines`/
//! `changed_lines`), cursor-column virtual-width computation
//! (`getvcol`/`coladvance`, already real elsewhere but not wired
//! through this file's own more involved comment-leader/paragraph
//! logic yet), and the insert-mode state machine - none of which this
//! file's own functions currently reach.
//!
//! Translated: `has_format_option` (a pure `'formatoptions'` membership
//! check, `'paste'`-gated); `ends_in_white` (reads whether a given
//! line ends in whitespace, via `memline.rs`'s already-real `ml_get`/
//! `ml_get_len`); `comp_textwidth` (effective `'textwidth'`
//! computation, falling back to `'wrapmargin'`-derived window width
//! via `window.rs`'s already-real `win_fdccol_count`).
//!
//! Deferred: everything else in the file (`fmt_check_par`/
//! `same_leader`/`paragraph_start`/`auto_format`/`op_format`/
//! `fex_format`/etc.) - all need real buffer-line mutation and/or the
//! comment-leader (`'comments'`) parsing machinery, not yet
//! translated.

use crate::ascii_defs::ascii_iswhite;
use crate::globals::GLOBALS;
use crate::pos_defs::LinenrT;

/// Whether the comment leader of line `lnum + 1` matches the one of
/// line `lnum`, so the two lines may be joined when formatting
/// (`same_leader`).
///
/// The `'comments'` flags of each leader decide this before the text
/// is even compared: `f` (first-line-only) allows joining only when
/// the second line has no leader at all, `e` (end) never allows it,
/// and `s` (start) allows it only when there is text after the leader
/// and the second line carries the `m` (middle) flag.
///
/// # Safety
/// `GLOBALS.curbuf` must be valid, with a live memline holding both
/// `lnum` and `lnum + 1`.
pub unsafe fn same_leader(
    lnum: LinenrT,
    leader1_len: i32,
    leader1_flags: Option<&[u8]>,
    leader2_len: i32,
    leader2_flags: Option<&[u8]>,
) -> bool {
    if leader1_len == 0 {
        return leader2_len == 0;
    }

    if let Some(flags1) = leader1_flags {
        for &p in flags1.iter().take_while(|&&c| c != 0 && c != b':') {
            if p == crate::option_vars::COM_FIRST {
                return leader2_len == 0;
            }
            if p == crate::option_vars::COM_END {
                return false;
            }
            if p == crate::option_vars::COM_START {
                // SAFETY: forwarded from this function's own safety doc.
                let line_len = unsafe { crate::memline::ml_get_len(lnum) };
                if line_len <= leader1_len {
                    return false;
                }
                let Some(flags2) = leader2_flags else {
                    return false;
                };
                if leader2_len == 0 {
                    return false;
                }
                return flags2
                    .iter()
                    .take_while(|&&c| c != 0 && c != b':')
                    .any(|&c| c == crate::option_vars::COM_MIDDLE);
            }
        }
    }

    // Get the current line and the next one, then compare the leaders.
    // SAFETY: forwarded from this function's own safety doc.
    let line1 = unsafe { crate::memline::ml_get(lnum) };
    // SAFETY: forwarded from this function's own safety doc.
    let line2 = unsafe { crate::memline::ml_get(lnum + 1) };

    let mut idx1 = 0usize;
    while ascii_iswhite(i32::from(line1.get(idx1).copied().unwrap_or(0))) {
        idx1 += 1;
    }
    let mut idx2 = 0i32;
    while idx2 < leader2_len {
        let c2 = line2.get(idx2 as usize).copied().unwrap_or(0);
        if ascii_iswhite(i32::from(c2)) {
            while ascii_iswhite(i32::from(line1.get(idx1).copied().unwrap_or(0))) {
                idx1 += 1;
            }
        } else {
            let c1 = line1.get(idx1).copied().unwrap_or(0);
            idx1 += 1;
            if c1 != c2 {
                break;
            }
        }
        idx2 += 1;
    }

    idx2 == leader2_len && idx1 == leader1_len as usize
}

/// Set when `auto_format()` added an extra space under the cursor
/// (`did_add_space`).
pub static DID_ADD_SPACE: crate::globals::GlobalCell<bool> =
    crate::globals::GlobalCell::new(false);

/// `WHITECHAR(cc)` from `textformat.c`: `cc` is whitespace, and the
/// character just after the cursor is not a composing character (so
/// the space really is a separator rather than the base of a
/// grapheme).
///
/// # Safety
/// `GLOBALS.curwin`/`curbuf` must be valid, with a live memline.
unsafe fn whitechar(cc: i32) -> bool {
    if !ascii_iswhite(cc) {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let p = unsafe { crate::cursor::get_cursor_pos_ptr() };
    let next = if p.len() > 1 {
        crate::mbyte::utf_ptr2char(&p[1..])
    } else {
        0
    };
    !crate::mbyte::utf_iscomposing_first(next)
}

/// Remove the space `auto_format()` added under the cursor, if it is
/// no longer at the end of the line (`check_auto_format`).
///
/// # Safety
/// Same as `whitechar`, plus `crate::change::del_char`'s own.
pub unsafe fn check_auto_format(end_insert: bool) {
    // SAFETY: reading a plain `bool` global.
    if !*unsafe { DID_ADD_SPACE.get_mut() } {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let cc = unsafe { crate::cursor::gchar_cursor() };
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { whitechar(cc) } {
        // Somehow the space was removed already.
        // SAFETY: as above.
        unsafe { *DID_ADD_SPACE.get_mut() = false };
    } else {
        let mut c = i32::from(b' ');
        if !end_insert {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::cursor::inc_cursor() };
            // SAFETY: forwarded from this function's own safety doc.
            c = unsafe { crate::cursor::gchar_cursor() };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::cursor::dec_cursor() };
        }
        if c != 0 {
            // The space is no longer at the end of the line, so
            // delete it.
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::change::del_char(false) };
            // SAFETY: as above.
            unsafe { *DID_ADD_SPACE.get_mut() = false };
        }
    }
}

/// Whether format option `x` is currently in effect for `curbuf` -
/// always `false` while `'paste'` is set (`has_format_option`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn has_format_option(x: i32) -> bool {
    if unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste != 0 {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*GLOBALS.get_mut().curbuf };
    crate::strings::vim_strchr(curbuf.b_p_fo.as_deref().unwrap_or(&[]), x).is_some()
}

/// Whether line `lnum` (in `curbuf`) ends in a whitespace character
/// (`ends_in_white`).
///
/// # Safety
/// Same as [`crate::memline::ml_get`].
#[must_use]
pub unsafe fn ends_in_white(lnum: LinenrT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let s = unsafe { crate::memline::ml_get(lnum) };
    if s.first() == Some(&0) {
        // Empty line (just the trailing NUL, matching this crate's
        // established `ml_get` return convention).
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let l = unsafe { crate::memline::ml_get_len(lnum) } - 1;
    ascii_iswhite(i32::from(s[l as usize]))
}

/// Find out the textwidth to use for formatting: `'textwidth'` if set,
/// else `curwin.w_view_width - 'wrapmargin'` (minus the fold/sign/
/// number-column margins and, if `ff` (force formatting, for the
/// `"gq"` operator) still zero, the window width capped at 79
/// (`comp_textwidth`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin`/`curbuf` must be valid, non-null
/// pointers to live `WinT`/`BufT` values.
#[must_use]
pub unsafe fn comp_textwidth(ff: bool) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { GLOBALS.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*g.curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &*g.curwin };

    let mut textwidth = curbuf.b_p_tw as i32;
    if textwidth == 0 && curbuf.b_p_wm != 0 {
        // The width is the window width minus 'wrapmargin' minus all
        // the things that add to the margin.
        textwidth = curwin.w_view_width - curbuf.b_p_wm as i32;
        textwidth -= crate::window::win_fdccol_count(curwin);
        textwidth -= curwin.w_scwidth;

        if curwin.w_onebuf_opt.wo_nu != 0 || curwin.w_onebuf_opt.wo_rnu != 0 {
            textwidth -= 8;
        }
    }
    textwidth = textwidth.max(0);
    if ff && textwidth == 0 {
        textwidth = (curwin.w_view_width - 1).min(79);
    }
    textwidth
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{BufT, WinT};
    use crate::globals::global_state_test_lock;

    /// RAII guard installing `win`/`buf` as curwin/curbuf, restoring
    /// the previous pointers on drop (even on test panic, via
    /// unwinding) - matching `cursor.rs`'s own `CursorTestGuard`/
    /// `mark.rs`'s `CurbufGuard` precedent. Holds
    /// `global_state_test_lock` for its whole lifetime, since `ml_open`
    /// (used by `buf_with_line` below) touches shared
    /// `GLOBALS.got_int` internally.
    struct CurbufWinGuard {
        prev_buf: *mut BufT,
        prev_win: *mut WinT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurbufWinGuard {
        fn set(win: *mut WinT, buf: *mut BufT) -> Self {
            let _lock = global_state_test_lock();
            let g = unsafe { GLOBALS.get_mut() };
            let guard = Self { prev_buf: g.curbuf, prev_win: g.curwin, _lock };
            g.curbuf = buf;
            g.curwin = win;
            guard
        }
    }

    impl Drop for CurbufWinGuard {
        fn drop(&mut self) {
            let g = unsafe { GLOBALS.get_mut() };
            g.curbuf = self.prev_buf;
            g.curwin = self.prev_win;
        }
    }

    /// Installs `win`/`buf` as curwin/curbuf, opens a real memline for
    /// `buf`, and replaces line 1 with `line` (matching `cursor.rs`'s
    /// own `open_and_set_test_buf` precedent - `ml_get`/`ml_get_len`
    /// need a real, memfile-backed buffer, not just a hand-poked
    /// cache, since `ml_get_buf_impl`'s very first check is `if
    /// buf.b_ml.ml_mfp.is_null() { return empty }`).
    fn open_and_set_test_buf(win: &mut WinT, buf: &mut BufT, line: &[u8]) -> CurbufWinGuard {
        let guard = CurbufWinGuard::set(win as *mut WinT, buf as *mut BufT);
        assert_eq!(unsafe { crate::memline::ml_open(buf) }, crate::vim_defs::OK);
        let mut with_nul = line.to_vec();
        with_nul.push(0);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(buf, 1, &with_nul) },
            crate::vim_defs::OK
        );
        guard
    }

    fn close_buf_with_memline(buf: BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    /// Like [`open_and_set_test_buf`], but builds several lines, which
    /// `same_leader` needs since it compares line `lnum` with `lnum + 1`.
    fn open_and_set_test_buf_lines(
        win: &mut WinT,
        buf: &mut BufT,
        lines: &[&[u8]],
    ) -> CurbufWinGuard {
        let guard = CurbufWinGuard::set(win as *mut WinT, buf as *mut BufT);
        assert_eq!(unsafe { crate::memline::ml_open(buf) }, crate::vim_defs::OK);
        for (i, line) in lines.iter().enumerate() {
            let mut with_nul = line.to_vec();
            with_nul.push(0);
            if i == 0 {
                assert_eq!(
                    unsafe { crate::memline::ml_replace_buf_len(buf, 1, &with_nul) },
                    crate::vim_defs::OK
                );
            } else {
                assert_eq!(
                    unsafe {
                        crate::memline::ml_append_buf(
                            buf,
                            i as LinenrT,
                            &with_nul,
                            with_nul.len() as i32,
                            false,
                        )
                    },
                    crate::vim_defs::OK
                );
            }
        }
        guard
    }

    #[test]
    fn same_leader_with_no_first_leader_requires_no_second_one() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"a", b"b"]);

        assert!(unsafe { same_leader(1, 0, None, 0, None) });
        assert!(!unsafe { same_leader(1, 0, None, 2, Some(b"m")) });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn same_leader_first_flag_requires_no_second_leader() {
        // 'f' means the leader only appears on the first line, so the
        // lines can be joined only if the second has no leader.
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"// a", b"// b"]);

        assert!(unsafe { same_leader(1, 2, Some(b"f"), 0, None) });
        assert!(!unsafe { same_leader(1, 2, Some(b"f"), 2, Some(b"f")) });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn same_leader_end_flag_never_joins() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"*/", b"*/"]);

        assert!(!unsafe { same_leader(1, 2, Some(b"e"), 2, Some(b"e")) });
        assert!(!unsafe { same_leader(1, 2, Some(b"e"), 0, None) });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn same_leader_start_flag_needs_text_and_a_middle_second_leader() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        // Line 1 is longer than its 2-byte leader, so there IS text
        // after it.
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"/* x", b" * y"]);

        assert!(unsafe { same_leader(1, 2, Some(b"s"), 2, Some(b"m")) });
        // No 'm' flag on the second leader.
        assert!(!unsafe { same_leader(1, 2, Some(b"s"), 2, Some(b"e")) });
        // No second leader at all.
        assert!(!unsafe { same_leader(1, 2, Some(b"s"), 0, None) });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn same_leader_start_flag_rejects_a_leader_only_line() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        // Line 1 is exactly its own leader, so there is no text after
        // it and the lines cannot be joined.
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"/*", b" * y"]);

        assert!(!unsafe { same_leader(1, 2, Some(b"s"), 2, Some(b"m")) });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn same_leader_compares_the_leader_text_when_no_flag_decides() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"# a", b"# b"]);

        // Matching one-byte leaders.
        assert!(unsafe { same_leader(1, 1, Some(b""), 1, Some(b"")) });
        // The second line's leader text differs from the first's.
        let mut buf2 = BufT::default();
        let mut win2 = WinT::default();
        drop(guard);
        close_buf_with_memline(buf);
        let guard2 = open_and_set_test_buf_lines(&mut win2, &mut buf2, &[b"# a", b"; b"]);
        assert!(!unsafe { same_leader(1, 1, Some(b""), 1, Some(b"")) });

        drop(guard2);
        close_buf_with_memline(buf2);
    }

    #[test]
    fn check_auto_format_is_a_noop_when_no_space_was_added() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"a b");
        let prev = *unsafe { DID_ADD_SPACE.get_mut() };
        unsafe { *DID_ADD_SPACE.get_mut() = false };

        unsafe { check_auto_format(true) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"a b\0");

        unsafe { *DID_ADD_SPACE.get_mut() = prev };
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_auto_format_clears_the_flag_when_the_space_is_already_gone() {
        // The cursor is on 'b', not whitespace, so the space this
        // would have removed was clearly removed by something else.
        let mut buf = BufT::default();
        let mut win = WinT {
            w_cursor: crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"a b");
        let prev = *unsafe { DID_ADD_SPACE.get_mut() };
        unsafe { *DID_ADD_SPACE.get_mut() = true };

        unsafe { check_auto_format(true) };

        assert!(!*unsafe { DID_ADD_SPACE.get_mut() }, "flag cleared");
        assert_eq!(
            unsafe { crate::memline::ml_get(1) },
            b"a b\0",
            "line untouched"
        );

        unsafe { *DID_ADD_SPACE.get_mut() = prev };
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_auto_format_keeps_a_trailing_space_at_end_insert() {
        // With end_insert, `c` stays ' ' (never NUL), so the space is
        // deleted - this is the "still at the end of the line" case
        // the original handles by NOT consulting the next character.
        let mut buf = BufT::default();
        let mut win = WinT {
            w_cursor: crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 },
            ..Default::default()
        };
        buf.b_u_curhead = Box::into_raw(Box::new(crate::undo_defs::UHeader::default()));
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"a b");
        let prev = *unsafe { DID_ADD_SPACE.get_mut() };
        unsafe { *DID_ADD_SPACE.get_mut() = true };

        unsafe { check_auto_format(true) };

        assert!(!*unsafe { DID_ADD_SPACE.get_mut() });
        assert_eq!(
            unsafe { crate::memline::ml_get(1) },
            b"ab\0",
            "the added space was removed"
        );

        unsafe { *DID_ADD_SPACE.get_mut() = prev };
        drop(guard);
        unsafe {
            drop(Box::from_raw(buf.b_u_curhead));
            buf.b_u_curhead = std::ptr::null_mut();
        }
        close_buf_with_memline(buf);
    }

    #[test]
    fn has_format_option_true_when_char_present_in_fo() {
        let mut buf = BufT { b_p_fo: Some(b"tcq".to_vec()), ..Default::default() };
        let mut win = WinT::default();
        let _guard = CurbufWinGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        assert!(unsafe { has_format_option(b'q' as i32) });
        assert!(!unsafe { has_format_option(b'a' as i32) });
    }

    #[test]
    fn has_format_option_false_when_paste_is_set() {
        let mut buf = BufT { b_p_fo: Some(b"tcq".to_vec()), ..Default::default() };
        let mut win = WinT::default();
        let _guard = CurbufWinGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        let saved_paste = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste = 1;
        let result = unsafe { has_format_option(b'q' as i32) };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste = saved_paste;
        assert!(!result);
    }

    #[test]
    fn has_format_option_false_when_fo_is_none() {
        // BufT::default()'s own b_p_fo is already None - nothing to
        // set, this test just confirms that default state directly.
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let _guard = CurbufWinGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        assert!(!unsafe { has_format_option(b'q' as i32) });
    }

    #[test]
    fn ends_in_white_true_for_trailing_space() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello ");
        assert!(unsafe { ends_in_white(1) });
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn ends_in_white_false_for_no_trailing_space() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello");
        assert!(!unsafe { ends_in_white(1) });
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn ends_in_white_false_for_empty_line() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"");
        assert!(!unsafe { ends_in_white(1) });
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn ends_in_white_true_for_trailing_tab() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\t");
        assert!(unsafe { ends_in_white(1) });
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn comp_textwidth_uses_b_p_tw_when_set() {
        let mut buf = BufT { b_p_tw: 72, ..Default::default() };
        let mut win = WinT { w_view_width: 80, ..Default::default() };
        let _guard = CurbufWinGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        assert_eq!(unsafe { comp_textwidth(false) }, 72);
    }

    #[test]
    fn comp_textwidth_falls_back_to_wrapmargin_when_tw_is_zero() {
        let mut buf = BufT { b_p_wm: 10, ..Default::default() };
        let mut win = WinT { w_view_width: 80, ..Default::default() };
        let _guard = CurbufWinGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        // 80 - 10 (wrapmargin) - 0 (fdccol) - 0 (scwidth) - 0 (no
        // number/relativenumber) = 70.
        assert_eq!(unsafe { comp_textwidth(false) }, 70);
    }

    #[test]
    fn comp_textwidth_subtracts_8_when_number_column_shown() {
        let mut buf = BufT { b_p_wm: 10, ..Default::default() };
        let mut win = WinT { w_view_width: 80, ..Default::default() };
        win.w_onebuf_opt.wo_nu = 1;
        let _guard = CurbufWinGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        assert_eq!(unsafe { comp_textwidth(false) }, 62);
    }

    #[test]
    fn comp_textwidth_never_goes_negative() {
        let mut buf = BufT { b_p_wm: 200, ..Default::default() };
        let mut win = WinT { w_view_width: 80, ..Default::default() };
        let _guard = CurbufWinGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        assert_eq!(unsafe { comp_textwidth(false) }, 0);
    }

    #[test]
    fn comp_textwidth_force_formatting_caps_at_79() {
        // b_p_tw/b_p_wm are both already 0 in BufT::default() - no
        // override needed, the wrapmargin branch is skipped entirely.
        let mut buf = BufT::default();
        let mut win = WinT { w_view_width: 100, ..Default::default() };
        let _guard = CurbufWinGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        // textwidth stays 0, then `ff` kicks in: min(100-1, 79) = 79.
        assert_eq!(unsafe { comp_textwidth(true) }, 79);
    }

    #[test]
    fn comp_textwidth_force_formatting_uses_window_width_when_narrow() {
        let mut buf = BufT::default();
        let mut win = WinT { w_view_width: 40, ..Default::default() };
        let _guard = CurbufWinGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        assert_eq!(unsafe { comp_textwidth(true) }, 39);
    }
}
