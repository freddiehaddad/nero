//! Translated from `src/nvim/window.c` (tractable core only).
//!
//! `window.c` is neovim's window-management/layout file (thousands of
//! lines) - almost entirely dependent on window creation/splitting/
//! closing machinery and the display pipeline, not attempted here.
//! Translated: `win_fdccol_count` (needed by `move.c`'s window-column-
//! offset calculations, with one narrow, explicit gap - see its own
//! doc comment); `valid_tabpage` (walks the real
//! `GLOBALS.first_tabpage`/`tp_next` linked list, matching `undo.rs`'s
//! `any_buf_is_changed`/`firstbuf`/`b_next` walk precedent);
//! `is_bottom_win` (walks the real `WinT.w_frame`/`FrameT.fr_parent`
//! window-layout tree, all already-translated struct shapes).
//!
//! Also translated: `tabpage_win_valid`/`win_valid`/
//! `win_find_by_handle`/`win_valid_any_tab`/`win_count` - each walks
//! the real `GLOBALS.firstwin`/`WinT.w_next` window list (within a
//! single tabpage) and/or `GLOBALS.first_tabpage`/`tp_next` tabpage
//! list (across all tabpages), matching `valid_tabpage`'s own
//! established walk precedent. `win_valid_any_tab`'s inner per-tabpage
//! check reuses `tabpage_win_valid` directly rather than
//! re-implementing the same window-list walk a second time - a
//! faithful simplification, not a drift: the original's own
//! `FOR_ALL_TAB_WINDOWS(tp, wp)` macro literally expands to
//! `FOR_ALL_TABS(tp) FOR_ALL_WINDOWS_IN_TAB(wp, tp)`, i.e. exactly
//! `tabpage_win_valid`'s own single-tabpage walk nested inside an
//! outer tabpage loop.
//!
//! Deferred: everything else in the file.

use crate::buffer_defs::WinT;

/// Check if `win` is a pointer to an existing window in tabpage `tp`
/// (`tabpage_win_valid`).
///
/// # Safety
/// `tp`'s own window list (`tp_firstwin`/`w_next`, or
/// `GLOBALS.firstwin`/`w_next` when `tp == GLOBALS.curtab`) must
/// consist of valid, live `WinT` pointers.
#[must_use]
pub unsafe fn tabpage_win_valid(
    tp: *const crate::buffer_defs::TabpageT,
    win: *const WinT,
) -> bool {
    if win.is_null() {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let is_curtab = std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab);
    let mut wp = if is_curtab {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*tp }.tp_firstwin
    };
    while !wp.is_null() {
        if std::ptr::eq(wp, win) {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    false
}

/// Check if `win` is a pointer to an existing window in the current
/// tab page (`win_valid`).
///
/// # Safety
/// Same as [`tabpage_win_valid`].
#[must_use]
pub unsafe fn win_valid(win: *const WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tabpage_win_valid(curtab, win) }
}

/// Find window `handle` in the current tab page, or a null pointer if
/// not found (`win_find_by_handle`).
///
/// # Safety
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid,
/// live `WinT` pointers.
#[must_use]
pub unsafe fn win_find_by_handle(handle: crate::types_defs::HandleT) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*wp }.handle == handle {
            return wp;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    std::ptr::null_mut()
}

/// Check if `win` is a pointer to an existing window in ANY tab page
/// (`win_valid_any_tab`).
///
/// # Safety
/// `GLOBALS.first_tabpage`'s own `tp_next` chain, and each tabpage's
/// own window list, must consist of valid, live pointers.
#[must_use]
pub unsafe fn win_valid_any_tab(win: *const WinT) -> bool {
    if win.is_null() {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { tabpage_win_valid(tp, win) } {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    false
}

/// Return the number of windows in the current tab page (`win_count`).
///
/// # Safety
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid,
/// live `WinT` pointers.
#[must_use]
pub unsafe fn win_count() -> i32 {
    let mut count = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        count += 1;
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    count
}

/// Return the width, in columns, of `wp`'s `'foldcolumn'`
/// (`win_fdccol_count`).
///
/// # Panics
/// The original supports `'foldcolumn'` set to `"auto"`/`"auto:N"`,
/// which needs `fold.c`'s real `getDeepestNesting` (walking the actual
/// fold-nesting data structure, not yet translated) to compute how
/// many columns are actually needed. That specific case is not
/// silently approximated (which would produce a genuinely wrong
/// column count, unlike e.g. `mf_write`'s omitted message displays,
/// which never affect state) - it panics instead, loudly, exactly
/// where the real gap is. The common, default case (`'foldcolumn'`
/// set to a plain digit, `"0"`..`"9"`) is fully supported.
#[must_use]
pub fn win_fdccol_count(wp: &WinT) -> i32 {
    let fdc = wp.w_onebuf_opt.wo_fdc.as_deref().unwrap_or(b"0");

    if fdc.starts_with(b"auto") {
        unimplemented!(
            "'foldcolumn'=auto needs fold.c's real getDeepestNesting, not yet translated"
        );
    }

    i32::from(fdc.first().copied().unwrap_or(b'0')) - i32::from(b'0')
}

/// Check that `tpc` points to a valid tab page (`valid_tabpage`).
///
/// # Safety
/// `crate::globals::GLOBALS.first_tabpage`'s own `tp_next` chain must
/// consist of valid, live `TabpageT` pointers (matching this crate's
/// usual global-linked-list-walk requirement, e.g. `undo.rs`'s
/// `any_buf_is_changed`).
#[must_use]
pub unsafe fn valid_tabpage(tpc: *const crate::buffer_defs::TabpageT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        if std::ptr::eq(tp, tpc) {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    false
}

/// Check if `wp` is at the bottom of its column of windows - i.e.
/// there are no windows below it (`is_bottom_win`).
///
/// # Safety
/// `wp.w_frame`'s own `fr_parent` chain must consist of valid, live
/// `FrameT` pointers.
#[must_use]
pub unsafe fn is_bottom_win(wp: &WinT) -> bool {
    let mut frp = wp.w_frame;
    loop {
        // SAFETY: forwarded from this function's own safety doc.
        let fr = unsafe { &*frp };
        if fr.fr_parent.is_null() {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let parent = unsafe { &*fr.fr_parent };
        if parent.fr_layout == crate::buffer_defs::FR_COL && !fr.fr_next.is_null() {
            return false;
        }
        frp = fr.fr_parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win_fdccol_count_defaults_to_zero_when_unset() {
        let win = WinT::default();
        assert_eq!(win_fdccol_count(&win), 0);
    }

    #[test]
    fn win_fdccol_count_reads_the_configured_digit() {
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_fdc = Some(b"3".to_vec());
        assert_eq!(win_fdccol_count(&win), 3);
    }

    #[test]
    #[should_panic(expected = "getDeepestNesting")]
    fn win_fdccol_count_auto_panics_with_a_clear_message() {
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_fdc = Some(b"auto".to_vec());
        let _ = win_fdccol_count(&win);
    }

    /// Points `GLOBALS.first_tabpage` at `head` for the guard's
    /// lifetime, restoring the previous value on drop. Callers must
    /// hold `global_state_test_lock()` for the guard's whole lifetime.
    struct FirstTabpageGuard {
        previous: *mut crate::buffer_defs::TabpageT,
    }

    impl FirstTabpageGuard {
        fn set(head: *mut crate::buffer_defs::TabpageT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
            unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = head;
            FirstTabpageGuard { previous }
        }
    }

    impl Drop for FirstTabpageGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = self.previous;
        }
    }

    #[test]
    fn valid_tabpage_true_for_head_of_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        assert!(unsafe { valid_tabpage(&tp as *const crate::buffer_defs::TabpageT) });
    }

    #[test]
    fn valid_tabpage_true_for_a_later_list_entry() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tail = crate::buffer_defs::TabpageT::default();
        let mut head = crate::buffer_defs::TabpageT {
            tp_next: &mut tail as *mut crate::buffer_defs::TabpageT,
            ..Default::default()
        };
        let _guard = FirstTabpageGuard::set(&mut head as *mut crate::buffer_defs::TabpageT);

        assert!(unsafe { valid_tabpage(&tail as *const crate::buffer_defs::TabpageT) });
    }

    #[test]
    fn valid_tabpage_false_for_a_pointer_not_in_the_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let stray = crate::buffer_defs::TabpageT::default();
        assert!(!unsafe { valid_tabpage(&stray as *const crate::buffer_defs::TabpageT) });
    }

    #[test]
    fn valid_tabpage_false_for_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = FirstTabpageGuard::set(std::ptr::null_mut());

        let stray = crate::buffer_defs::TabpageT::default();
        assert!(!unsafe { valid_tabpage(&stray as *const crate::buffer_defs::TabpageT) });
    }

    #[test]
    fn is_bottom_win_true_for_a_single_top_level_frame() {
        let mut frame = crate::buffer_defs::FrameT::default();
        let win = WinT { w_frame: &mut frame as *mut crate::buffer_defs::FrameT, ..Default::default() };
        assert!(unsafe { is_bottom_win(&win) });
    }

    #[test]
    fn is_bottom_win_false_when_a_col_sibling_frame_follows() {
        // frame is one of two children in a FR_COL (vertically-
        // stacked) parent, with a sibling AFTER it (fr_next != NULL) -
        // meaning there's a window below.
        let mut sibling = crate::buffer_defs::FrameT::default();
        let mut parent = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            ..Default::default()
        };
        let mut frame = crate::buffer_defs::FrameT {
            fr_parent: &mut parent as *mut crate::buffer_defs::FrameT,
            fr_next: &mut sibling as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        let win = WinT { w_frame: &mut frame as *mut crate::buffer_defs::FrameT, ..Default::default() };
        assert!(!unsafe { is_bottom_win(&win) });
    }

    #[test]
    fn is_bottom_win_true_when_last_in_a_col_of_frames() {
        // Same FR_COL parent, but frame is the LAST child (fr_next ==
        // NULL) - it's the bottom one.
        let mut parent = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            ..Default::default()
        };
        let mut frame = crate::buffer_defs::FrameT {
            fr_parent: &mut parent as *mut crate::buffer_defs::FrameT,
            fr_next: std::ptr::null_mut(),
            ..Default::default()
        };
        let win = WinT { w_frame: &mut frame as *mut crate::buffer_defs::FrameT, ..Default::default() };
        assert!(unsafe { is_bottom_win(&win) });
    }

    #[test]
    fn is_bottom_win_true_when_parent_is_a_row_not_a_column() {
        // A FR_ROW (side-by-side) parent never blocks "bottom" status,
        // regardless of fr_next - only FR_COL siblings matter.
        let mut sibling = crate::buffer_defs::FrameT::default();
        let mut parent = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            ..Default::default()
        };
        let mut frame = crate::buffer_defs::FrameT {
            fr_parent: &mut parent as *mut crate::buffer_defs::FrameT,
            fr_next: &mut sibling as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        let win = WinT { w_frame: &mut frame as *mut crate::buffer_defs::FrameT, ..Default::default() };
        assert!(unsafe { is_bottom_win(&win) });
    }

    #[test]
    fn is_bottom_win_checks_the_whole_ancestor_chain() {
        // frame's own immediate parent is FR_ROW (doesn't block), but
        // the GRANDPARENT is FR_COL with a sibling after the middle
        // frame - still not at the bottom.
        let mut grandparent_sibling = crate::buffer_defs::FrameT::default();
        let mut grandparent = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            ..Default::default()
        };
        let mut middle = crate::buffer_defs::FrameT {
            fr_parent: &mut grandparent as *mut crate::buffer_defs::FrameT,
            fr_next: &mut grandparent_sibling as *mut crate::buffer_defs::FrameT,
            fr_layout: crate::buffer_defs::FR_ROW,
            ..Default::default()
        };
        let mut frame = crate::buffer_defs::FrameT {
            fr_parent: &mut middle as *mut crate::buffer_defs::FrameT,
            fr_next: std::ptr::null_mut(),
            ..Default::default()
        };
        let win = WinT { w_frame: &mut frame as *mut crate::buffer_defs::FrameT, ..Default::default() };
        assert!(!unsafe { is_bottom_win(&win) });
    }

    /// Points `GLOBALS.firstwin`/`GLOBALS.curtab` at the given values
    /// for the guard's lifetime, restoring both previous values on
    /// drop. Callers must hold `global_state_test_lock()` for the
    /// guard's whole lifetime (matching `FirstTabpageGuard`'s own
    /// precedent, extended to cover both globals these new functions
    /// touch together).
    struct WindowListGuard {
        prev_firstwin: *mut WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
    }

    impl WindowListGuard {
        fn set(firstwin: *mut WinT, curtab: *mut crate::buffer_defs::TabpageT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard =
                WindowListGuard { prev_firstwin: globals.firstwin, prev_curtab: globals.curtab };
            globals.firstwin = firstwin;
            globals.curtab = curtab;
            guard
        }
    }

    impl Drop for WindowListGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = self.prev_firstwin;
            globals.curtab = self.prev_curtab;
        }
    }

    #[test]
    fn tabpage_win_valid_false_for_null_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(std::ptr::null_mut(), &mut tp);
        assert!(!unsafe { tabpage_win_valid(&tp, std::ptr::null()) });
    }

    #[test]
    fn tabpage_win_valid_true_via_globals_firstwin_when_tp_is_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        // GLOBALS.firstwin (NOT tp.tp_firstwin, deliberately left null)
        // is used because tp == curtab.
        let _guard = WindowListGuard::set(&mut win as *mut WinT, &mut tp);
        assert!(unsafe { tabpage_win_valid(&tp, &win) });
    }

    #[test]
    fn tabpage_win_valid_true_via_tp_firstwin_when_tp_is_not_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let mut other_tp = crate::buffer_defs::TabpageT::default();
        let tp = crate::buffer_defs::TabpageT {
            tp_firstwin: &mut win as *mut WinT,
            ..Default::default()
        };
        // curtab is a DIFFERENT tabpage - tp's own tp_firstwin is used,
        // not GLOBALS.firstwin (left null here).
        let _guard = WindowListGuard::set(std::ptr::null_mut(), &mut other_tp);
        assert!(unsafe { tabpage_win_valid(&tp, &win) });
    }

    #[test]
    fn tabpage_win_valid_false_for_a_window_not_in_the_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let stray = WinT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut win as *mut WinT, &mut tp);
        assert!(!unsafe { tabpage_win_valid(&tp, &stray) });
    }

    #[test]
    fn win_valid_delegates_to_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut win as *mut WinT, &mut tp);
        assert!(unsafe { win_valid(&win) });

        let stray = WinT::default();
        assert!(!unsafe { win_valid(&stray) });
    }

    #[test]
    fn win_find_by_handle_finds_a_matching_handle_in_the_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = WinT { handle: 7, ..Default::default() };
        let mut first =
            WinT { handle: 3, w_next: &mut second as *mut WinT, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut first as *mut WinT, &mut tp);

        assert!(std::ptr::eq(unsafe { win_find_by_handle(7) }, &second as *const WinT));
        assert!(std::ptr::eq(unsafe { win_find_by_handle(3) }, &first as *const WinT));
    }

    #[test]
    fn win_find_by_handle_null_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 3, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut win as *mut WinT, &mut tp);

        assert!(unsafe { win_find_by_handle(99) }.is_null());
    }

    #[test]
    fn win_valid_any_tab_finds_a_window_in_a_non_curtab_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let mut other_tp = crate::buffer_defs::TabpageT {
            tp_firstwin: &mut win as *mut WinT,
            ..Default::default()
        };
        let mut curtab = crate::buffer_defs::TabpageT {
            tp_next: &mut other_tp as *mut crate::buffer_defs::TabpageT,
            ..Default::default()
        };
        let _first_tabpage_guard =
            FirstTabpageGuard::set(&mut curtab as *mut crate::buffer_defs::TabpageT);
        // GLOBALS.firstwin is empty for curtab itself - win only exists
        // in the SECOND tabpage's own tp_firstwin.
        let _window_list_guard =
            WindowListGuard::set(std::ptr::null_mut(), &mut curtab as *mut _);

        assert!(unsafe { win_valid_any_tab(&win) });
    }

    #[test]
    fn win_valid_any_tab_false_when_null_or_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _first_tabpage_guard =
            FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _window_list_guard = WindowListGuard::set(std::ptr::null_mut(), &mut tp as *mut _);

        assert!(!unsafe { win_valid_any_tab(std::ptr::null()) });
        let stray = WinT::default();
        assert!(!unsafe { win_valid_any_tab(&stray) });
    }

    #[test]
    fn win_count_counts_the_current_tabpage_window_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = WinT::default();
        let mut second = WinT { w_next: &mut third as *mut WinT, ..Default::default() };
        let mut first = WinT { w_next: &mut second as *mut WinT, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut first as *mut WinT, &mut tp);

        assert_eq!(unsafe { win_count() }, 3);
    }

    #[test]
    fn win_count_zero_for_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(std::ptr::null_mut(), &mut tp);

        assert_eq!(unsafe { win_count() }, 0);
    }
}
