//! Translated from `src/nvim/mouse.c` (tractable core only).
//!
//! `mouse.c` is neovim's mouse-event-handling file (click dispatch,
//! window/tab selection, drag-to-select) - almost entirely tied to
//! real mouse input (`os/input.c`'s event loop, not translated) and
//! the window-layout/redraw machinery.
//!
//! Translated: `get_mouse_class` (word-selection character
//! classification: blank/punctuation-group/word/multi-byte-word) and
//! its own 2 real callers, `find_start_of_word`/`find_end_of_word`
//! (move a position to the start/end of the word it's in, used by
//! double-click word selection) - needing only already-real
//! `mbyte.c`'s `utf_byte2len`/`mb_get_class`/`utf_head_off`/
//! `utfc_ptr2len`, `charset.c`'s `vim_iswordc`, `strings.c`'s
//! `vim_strchr`, and `memline.c`'s `ml_get` (the current-buffer-
//! implicit form, already translated).
//!
//! Also translated: [`mouse_model_popup`]/[`reset_dragwin`]/
//! [`set_mouse_topline`], along with the `dragwin`/`orig_topline`/
//! `orig_topfill` file-statics they own. `mouse_model_popup` tests
//! only the FIRST byte of `'mousemodel'` for `'p'`, which is what
//! makes one check cover both `"popup"` and `"popup_setpos"` - and
//! why an empty value reads as false. `setmouse` stays deferred
//! (needs `ui_cursor_shape`/`ui_check_mouse`).
//!
//! Deferred: everything else - `move_tab_to_mouse` (needs
//! `tab_page_click_defs`/`tabpage_move`, tabline-click-region state,
//! not translated) and the entire real mouse-click dispatch/dragging
//! machinery (needs `os/input.c`'s event loop).

/// Get class of a character for selection: same class means same word
/// (`get_mouse_class`).
///
/// - `0`: blank
/// - `1`: punctuation groups
/// - `2`: normal word character
/// - `>2`: multi-byte word character
///
/// `p` is expected to be a line-suffix slice that (like every other
/// line buffer in this crate) includes its own trailing NUL byte; an
/// empty slice is treated the same as a NUL byte (class `0`) rather
/// than panicking, since no real caller can produce one anyway (a
/// valid column into a real line always has at least the line's own
/// terminator left to read).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` (forwarded from
/// [`crate::mbyte::mb_get_class`]'s/[`crate::charset::vim_iswordc`]'s
/// own safety docs, only reached for multi-byte/word-character input
/// respectively).
unsafe fn get_mouse_class(p: &[u8]) -> i32 {
    let b0 = p.first().copied().unwrap_or(0);
    if crate::mbyte::utf_byte2len(b0) > 1 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::mbyte::mb_get_class(p) };
    }

    let c = i32::from(b0);
    if c == i32::from(b' ') || c == i32::from(b'\t') {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::charset::vim_iswordc(c) } {
        return 2;
    }

    // There are a few special cases where we want certain combinations
    // of characters to be considered as a single word. These are
    // things like "->", "/ *", "*=", "+=", "&=", "<=", ">=", "!=" etc.
    // Otherwise, each character is in its own class.
    if c != 0 && crate::strings::vim_strchr(b"-+*/%<>&|^!=", c).is_some() {
        return 1;
    }
    c
}

/// The window currently being dragged (`dragwin`), file-static in the
/// original. Null when no drag is in progress.
static DRAGWIN: crate::globals::GlobalCell<*mut crate::buffer_defs::WinT> =
    crate::globals::GlobalCell::new(std::ptr::null_mut());

/// `w_topline` of the window at the start of a mouse selection
/// (`orig_topline`), file-static in the original.
static ORIG_TOPLINE: crate::globals::GlobalCell<crate::pos_defs::LinenrT> =
    crate::globals::GlobalCell::new(0);

/// `w_topfill` of the window at the start of a mouse selection
/// (`orig_topfill`), file-static in the original.
static ORIG_TOPFILL: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// Whether `'mousemodel'` is `"popup"` or `"popup_setpos"`
/// (`mouse_model_popup`).
///
/// The original tests only the FIRST byte for `'p'`, which is what
/// makes one check cover both values; an empty option is therefore
/// false.
///
/// # Safety
/// Must not run concurrently with any write to
/// `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn mouse_model_popup() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let mousem = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousem.as_deref();
    mousem.and_then(<[u8]>::first) == Some(&b'p')
}

/// Reset the window being dragged (`reset_dragwin`), called when
/// switching tab page.
///
/// # Safety
/// Must not run concurrently with any other access to `DRAGWIN`.
pub unsafe fn reset_dragwin() {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { DRAGWIN.get_mut() } = std::ptr::null_mut();
}

/// Remember a window's top line, so a double click still works after
/// jumping to another window (`set_mouse_topline`).
///
/// # Safety
/// Must not run concurrently with any other access to `ORIG_TOPLINE`
/// or `ORIG_TOPFILL`.
pub unsafe fn set_mouse_topline(wp: &crate::buffer_defs::WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { ORIG_TOPLINE.get_mut() } = wp.w_topline;
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { ORIG_TOPFILL.get_mut() } = wp.w_topfill;
}

/// Move `pos` back to the start of the word it's in
/// (`find_start_of_word`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` (forwarded from [`crate::memline::ml_get`]'s/
/// `get_mouse_class`'s/[`crate::mbyte::utf_head_off`]'s own safety
/// docs).
pub unsafe fn find_start_of_word(pos: &mut crate::pos_defs::PosT) {
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get(pos.lnum) };
    // SAFETY: forwarded from this function's own safety doc.
    let cclass = unsafe { get_mouse_class(&line[pos.col as usize..]) };

    while pos.col > 0 {
        let mut col = pos.col - 1;
        // SAFETY: forwarded from this function's own safety doc.
        col -= unsafe { crate::mbyte::utf_head_off(&line, col as usize) };
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { get_mouse_class(&line[col as usize..]) } != cclass {
            break;
        }
        pos.col = col;
    }
}

/// Move `pos` forward to the end of the word it's in. When
/// `'selection'` is `"exclusive"`, the position is just after the word
/// (`find_end_of_word`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` (forwarded from [`crate::memline::ml_get`]'s/
/// `get_mouse_class`'s/[`crate::mbyte::utf_head_off`]'s/
/// [`crate::mbyte::utfc_ptr2len`]'s own safety docs).
pub unsafe fn find_end_of_word(pos: &mut crate::pos_defs::PosT) {
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get(pos.lnum) };

    let sel_is_exclusive = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_sel
        .as_deref()
        .and_then(|s| s.first())
        .copied()
        == Some(b'e');
    if sel_is_exclusive && pos.col > 0 {
        pos.col -= 1;
        // SAFETY: forwarded from this function's own safety doc.
        pos.col -= unsafe { crate::mbyte::utf_head_off(&line, pos.col as usize) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let cclass = unsafe { get_mouse_class(&line[pos.col as usize..]) };
    while line[pos.col as usize] != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let col = pos.col + unsafe { crate::mbyte::utfc_ptr2len(&line[pos.col as usize..]) };
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { get_mouse_class(&line[col as usize..]) } != cclass {
            if sel_is_exclusive {
                pos.col = col;
            }
            break;
        }
        pos.col = col;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;
    use crate::pos_defs::PosT;

    /// Saves and restores `'mousemodel'` across a test.
    struct MousemGuard {
        saved: Option<Vec<u8>>,
    }

    impl MousemGuard {
        fn set(value: Option<&[u8]>) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let saved = opts.p_mousem.take();
            opts.p_mousem = value.map(<[u8]>::to_vec);
            Self { saved }
        }
    }

    impl Drop for MousemGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousem = self.saved.take();
        }
    }

    #[test]
    fn mouse_model_popup_matches_both_popup_variants() {
        let _lock = crate::globals::global_state_test_lock();
        for value in [&b"popup"[..], &b"popup_setpos"[..]] {
            let _guard = MousemGuard::set(Some(value));
            assert!(unsafe { mouse_model_popup() }, "{value:?}");
        }
    }

    #[test]
    fn mouse_model_popup_rejects_the_other_models() {
        let _lock = crate::globals::global_state_test_lock();
        for value in [&b"extend"[..], &b"mac"[..]] {
            let _guard = MousemGuard::set(Some(value));
            assert!(!unsafe { mouse_model_popup() }, "{value:?}");
        }
    }

    #[test]
    fn mouse_model_popup_is_false_for_an_empty_or_absent_value() {
        let _lock = crate::globals::global_state_test_lock();
        // The original indexes byte 0 of a NUL-terminated string, so
        // an empty value reads the terminator and is not 'p'.
        let _guard = MousemGuard::set(Some(b""));
        assert!(!unsafe { mouse_model_popup() });
        drop(_guard);

        let _guard = MousemGuard::set(None);
        assert!(!unsafe { mouse_model_popup() });
    }

    #[test]
    fn reset_dragwin_clears_the_dragged_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        unsafe { *DRAGWIN.get_mut() = &mut win as *mut crate::buffer_defs::WinT };

        unsafe { reset_dragwin() };
        assert!(unsafe { *DRAGWIN.get_mut() }.is_null());
    }

    #[test]
    fn set_mouse_topline_records_both_fields() {
        let _lock = crate::globals::global_state_test_lock();
        let (pl, pf) = unsafe { (*ORIG_TOPLINE.get_mut(), *ORIG_TOPFILL.get_mut()) };

        let win = crate::buffer_defs::WinT { w_topline: 42, w_topfill: 3, ..Default::default() };
        unsafe { set_mouse_topline(&win) };
        assert_eq!(unsafe { *ORIG_TOPLINE.get_mut() }, 42);
        assert_eq!(unsafe { *ORIG_TOPFILL.get_mut() }, 3);

        unsafe {
            *ORIG_TOPLINE.get_mut() = pl;
            *ORIG_TOPFILL.get_mut() = pf;
        }
    }

    /// Points `GLOBALS.curbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime.
    struct CurbufGuard {
        previous: *mut BufT,
    }

    impl CurbufGuard {
        fn set(buf: *mut BufT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = buf;
            CurbufGuard { previous }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    fn buf_with_lines(lines: &[&[u8]]) -> BufT {
        let mut buf = BufT::default();
        unsafe {
            assert_eq!(crate::memline::ml_open(&mut buf), crate::vim_defs::OK);
        }
        for (i, line) in lines.iter().enumerate() {
            let mut owned = line.to_vec();
            owned.push(0);
            let lnum = (i + 1) as crate::pos_defs::LinenrT;
            unsafe {
                if i == 0 {
                    assert_eq!(crate::memline::ml_replace_buf_len(&mut buf, 1, &owned), crate::vim_defs::OK);
                } else {
                    assert_eq!(
                        crate::memline::ml_append_buf(&mut buf, lnum - 1, &owned, owned.len() as i32, false),
                        crate::vim_defs::OK
                    );
                }
            }
        }
        buf
    }

    fn close_test_buf(buf: BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    // --- get_mouse_class ---

    #[test]
    fn get_mouse_class_blank_for_space_and_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"hello"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        assert_eq!(unsafe { get_mouse_class(b" x") }, 0);
        assert_eq!(unsafe { get_mouse_class(b"\tx") }, 0);
        close_test_buf(buf);
    }

    #[test]
    fn get_mouse_class_word_for_ascii_letter() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"hello"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        assert_eq!(unsafe { get_mouse_class(b"hello") }, 2);
        close_test_buf(buf);
    }

    #[test]
    fn get_mouse_class_punctuation_group_for_special_chars() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"hello"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        for &c in b"-+*/%<>&|^!=" {
            assert_eq!(unsafe { get_mouse_class(&[c]) }, 1, "byte {c:#x} should be class 1");
        }
        close_test_buf(buf);
    }

    #[test]
    fn get_mouse_class_own_class_for_other_ascii_punctuation() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"hello"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        // '.' is not in the special punctuation-group list, so it's
        // its own class (its own byte value).
        assert_eq!(unsafe { get_mouse_class(b".") }, i32::from(b'.'));
        close_test_buf(buf);
    }

    #[test]
    fn get_mouse_class_multibyte_delegates_to_mb_get_class() {
        let _lock = crate::globals::global_state_test_lock();
        // CJK character (U+4E00, "one") - a multi-byte word character,
        // matching mb_get_class_tab's own established "utf_class_tab"
        // dispatch (class > 2).
        let mut buf = buf_with_lines(&["\u{4e00}".as_bytes()]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        let line = unsafe { crate::memline::ml_get(1) };
        assert!(unsafe { get_mouse_class(&line) } > 2);
        close_test_buf(buf);
    }

    #[test]
    fn get_mouse_class_nul_byte_is_blank() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"hello"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        assert_eq!(unsafe { get_mouse_class(b"\0") }, 0);
        assert_eq!(unsafe { get_mouse_class(b"") }, 0);
        close_test_buf(buf);
    }

    // --- find_start_of_word ---

    #[test]
    fn find_start_of_word_moves_back_to_the_beginning() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"foo bar baz"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        // Start in the middle of "bar" (columns 4-6), land on column 4.
        let mut pos = PosT { lnum: 1, col: 5, coladd: 0 };
        unsafe { find_start_of_word(&mut pos) };
        assert_eq!(pos.col, 4);
        close_test_buf(buf);
    }

    #[test]
    fn find_start_of_word_already_at_start_is_a_noop() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"foo bar"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        let mut pos = PosT { lnum: 1, col: 4, coladd: 0 }; // start of "bar"
        unsafe { find_start_of_word(&mut pos) };
        assert_eq!(pos.col, 4);
        close_test_buf(buf);
    }

    #[test]
    fn find_start_of_word_stops_at_a_class_change_not_just_column_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"foo bar"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        // Column 6 is inside "bar" (columns 4-6); must stop at column
        // 4 (the space at column 3 is a different class), not walk
        // all the way to column 0.
        let mut pos = PosT { lnum: 1, col: 6, coladd: 0 };
        unsafe { find_start_of_word(&mut pos) };
        assert_eq!(pos.col, 4);
        close_test_buf(buf);
    }

    // --- find_end_of_word ---

    #[test]
    fn find_end_of_word_inclusive_selection_lands_on_the_last_char() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"foo bar baz"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = Some(b"inclusive".to_vec());
        // Start at column 4 ('b' of "bar"); "bar" spans columns 4-6.
        let mut pos = PosT { lnum: 1, col: 4, coladd: 0 };
        unsafe { find_end_of_word(&mut pos) };
        assert_eq!(pos.col, 6);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = None;
        close_test_buf(buf);
    }

    #[test]
    fn find_end_of_word_exclusive_selection_lands_just_past_the_word() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"foo bar baz"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = Some(b"exclusive".to_vec());
        // Start in the MIDDLE of "bar" (column 5), not at its very
        // first column: with exclusive selection the original's own
        // real body decrements pos.col by 1 BEFORE establishing
        // cclass, so starting exactly at a word's first column would
        // instead re-anchor on the PRECEDING character's own class
        // (the space just before it) - a real, deliberate original
        // behavior (not tested here), not a bug.
        let mut pos = PosT { lnum: 1, col: 5, coladd: 0 };
        unsafe { find_end_of_word(&mut pos) };
        // "exclusive" selection stops one column PAST the word (the
        // space at column 7), not on the word's own last character.
        assert_eq!(pos.col, 7);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = None;
        close_test_buf(buf);
    }

    #[test]
    fn find_end_of_word_exclusive_selection_starting_at_a_words_first_column_re_anchors_backward() {
        // A real, deliberate quirk of the original: with exclusive
        // selection, pos.col is decremented by 1 BEFORE cclass is
        // established - starting exactly at a word's own first
        // column (4, "bar"'s own 'b') steps back onto the PRECEDING
        // character (the space at column 3), re-anchoring cclass on
        // THAT character's class (blank) instead of the word's own.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"foo bar baz"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = Some(b"exclusive".to_vec());
        let mut pos = PosT { lnum: 1, col: 4, coladd: 0 };
        unsafe { find_end_of_word(&mut pos) };
        // Walking forward from the blank at column 3 stops the moment
        // a non-blank class is found again - column 4 itself ('b').
        assert_eq!(pos.col, 4);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = None;
        close_test_buf(buf);
    }

    #[test]
    fn find_end_of_word_stops_at_the_end_of_the_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_lines(&[b"foo bar"]);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = Some(b"inclusive".to_vec());
        let mut pos = PosT { lnum: 1, col: 4, coladd: 0 }; // start of "bar", the last word
        unsafe { find_end_of_word(&mut pos) };
        assert_eq!(pos.col, 6); // last real character of the line
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = None;
        close_test_buf(buf);
    }
}
