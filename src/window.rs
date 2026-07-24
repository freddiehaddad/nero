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
//! Deferred: everything else in the file.

use crate::buffer_defs::WinT;

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
}
