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
//! Also translated: `win_has_winnr`/`win_get_tabwin` (both real
//! `window.c` functions), plus `win_id2win`/`win_getid` (originally
//! `static` helpers in `eval/window.c`, hosted here alongside their
//! own `window.c` dependencies rather than in `eval/funcs.rs` - same
//! "helper logic lives near its own dependencies, the builtin
//! Vimscript-facing wrapper lives in `funcs.rs`" precedent as
//! `state.rs`'s `get_mode`/`funcs.rs`'s `f_mode`). All 4 need the
//! same window/tabpage-list walk already established above, plus
//! `WinT.w_config`'s already-translated `hide`/`focusable` fields for
//! `win_has_winnr`'s own floating-window-aware numbering check.
//!
//! Also translated: `check_can_set_curbuf_disabled`/
//! `check_can_set_curbuf_forceit` (`'winfixbuf'` checks) - each omits
//! the original's real `emsg` call, matching the established "skip the
//! deferred-subsystem side effect, keep the state/return value
//! correct" policy.
//!
//! Also translated, from `window.h` (not `window.c` - a tiny, self-
//! contained enum needed by `option.c`'s `check_num_option_bounds`):
//! `MIN_COLUMNS`/`MIN_LINES`/`STATUS_HEIGHT`.
//!
//! Deferred: everything else in the file.

use crate::buffer_defs::WinT;

/// minimal columns for screen (`MIN_COLUMNS`).
pub const MIN_COLUMNS: i32 = 12;
/// minimal lines for screen (`MIN_LINES`).
pub const MIN_LINES: i32 = 2;
/// height of a status line under a window (`STATUS_HEIGHT`).
pub const STATUS_HEIGHT: i32 = 1;

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

/// Whether window `wp` "counts" toward window numbering in tab page
/// `tp` (`win_has_winnr`). `tp`'s own current window always counts;
/// otherwise, only a non-hidden, focusable window counts (a floating
/// window can be configured, via `w_config`, to not participate in
/// window numbering).
///
/// # Safety
/// `wp`/`tp` must be valid, non-null pointers to live `WinT`/
/// `TabpageT`.
#[must_use]
pub unsafe fn win_has_winnr(wp: *const WinT, tp: *const crate::buffer_defs::TabpageT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let is_curtab = std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab);
    let tab_curwin = if is_curtab {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*tp }.tp_curwin
    };
    // SAFETY: forwarded from this function's own safety doc.
    let w_config = &unsafe { &*wp }.w_config;
    std::ptr::eq(wp, tab_curwin) || (!w_config.hide && w_config.focusable)
}

/// Get the window number (within the CURRENT tab page only) for
/// window handle `id`, or `0` if not found (or found but not counted,
/// per [`win_has_winnr`]) (`win_id2win`, `eval/window.c`).
///
/// # Safety
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid,
/// live `WinT` pointers.
#[must_use]
pub unsafe fn win_id2win(id: crate::types_defs::HandleT) -> i32 {
    let mut nr = 1;
    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*wp }.handle == id {
            // SAFETY: forwarded from this function's own safety doc.
            return if unsafe { win_has_winnr(wp, curtab) } { nr } else { 0 };
        }
        // SAFETY: forwarded from this function's own safety doc.
        nr += i32::from(unsafe { win_has_winnr(wp, curtab) });
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    0
}

/// Get the tab number and window number (`(tabnr, winnr)`) for window
/// handle `id`, both `0` if not found - or found, but not counted per
/// [`win_has_winnr`] (`win_get_tabwin`, `window.c`).
///
/// # Safety
/// `GLOBALS.first_tabpage`'s own `tp_next` chain, and each tabpage's
/// own window list, must consist of valid, live pointers.
#[must_use]
pub unsafe fn win_get_tabwin(id: crate::types_defs::HandleT) -> (i32, i32) {
    let mut tnum = 1;
    let mut wnum = 1;
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
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
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { &*wp }.handle == id {
                // SAFETY: forwarded from this function's own safety doc.
                return if unsafe { win_has_winnr(wp, tp) } { (tnum, wnum) } else { (0, 0) };
            }
            // SAFETY: forwarded from this function's own safety doc.
            wnum += i32::from(unsafe { win_has_winnr(wp, tp) });
            // SAFETY: forwarded from this function's own safety doc.
            wp = unsafe { &*wp }.w_next;
        }
        tnum += 1;
        wnum = 1;
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    (0, 0)
}

/// Get the window handle for window number `winnr` in tab number
/// `tabnr` (`win_getid`, `eval/window.c`). `winnr == None` means "the
/// current window" (returns its handle directly, `tabnr` ignored,
/// matching the original's own `argvars[0].v_type == VAR_UNKNOWN`
/// early return). `0` if `winnr <= 0` or not found; `-1` if `tabnr`
/// doesn't resolve to a real tab page.
///
/// # Safety
/// Same requirement as [`win_get_tabwin`].
#[must_use]
pub unsafe fn win_getid(winnr: Option<i32>, tabnr: Option<i32>) -> crate::types_defs::HandleT {
    let Some(mut winnr) = winnr else {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.handle;
    };
    if winnr <= 0 {
        return 0;
    }

    let (tp, mut wp) = match tabnr {
        None => {
            // SAFETY: forwarded from this function's own safety doc.
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            (g.curtab, g.firstwin)
        }
        Some(mut tabnr) => {
            // SAFETY: forwarded from this function's own safety doc.
            let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
            while !tp.is_null() {
                tabnr -= 1;
                if tabnr == 0 {
                    break;
                }
                // SAFETY: forwarded from this function's own safety doc.
                tp = unsafe { &*tp }.tp_next;
            }
            if tp.is_null() {
                return -1;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let is_curtab = std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab);
            let wp = if is_curtab {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &*tp }.tp_firstwin
            };
            (tp, wp)
        }
    };

    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        winnr -= i32::from(unsafe { win_has_winnr(wp, tp) });
        if winnr == 0 {
            // SAFETY: forwarded from this function's own safety doc.
            return unsafe { &*wp }.handle;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    0
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

/// Check if the current window is allowed to move to a different
/// buffer (`check_can_set_curbuf_disabled`).
///
/// @return `false` if the window has `'winfixbuf'` set, `true`
/// otherwise.
///
/// Omits the original's real
/// `emsg(_(e_winfixbuf_cannot_go_to_buffer))` call - matching the
/// established "skip the deferred-subsystem side effect, keep the
/// state/return value correct" policy used throughout this crate.
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT`.
#[must_use]
pub unsafe fn check_can_set_curbuf_disabled() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &*crate::globals::GLOBALS.get_mut().curwin };
    curwin.w_onebuf_opt.wo_wfb == 0
}

/// Check if the current window is allowed to move to a different
/// buffer (`check_can_set_curbuf_forceit`).
///
/// @param forceit if `true`, always allowed. If `false` and
/// `'winfixbuf'` is enabled, not allowed.
///
/// Omits the original's real `emsg` call, matching
/// [`check_can_set_curbuf_disabled`].
///
/// # Safety
/// Same as [`check_can_set_curbuf_disabled`].
#[must_use]
pub unsafe fn check_can_set_curbuf_forceit(forceit: bool) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &*crate::globals::GLOBALS.get_mut().curwin };
    forceit || curwin.w_onebuf_opt.wo_wfb == 0
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
    fn min_columns_min_lines_status_height_match_c_enum() {
        assert_eq!(MIN_COLUMNS, 12);
        assert_eq!(MIN_LINES, 2);
        assert_eq!(STATUS_HEIGHT, 1);
    }

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

    #[test]
    fn check_can_set_curbuf_disabled_true_when_winfixbuf_unset() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_wfb = 0;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(unsafe { check_can_set_curbuf_disabled() });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn check_can_set_curbuf_disabled_false_when_winfixbuf_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_wfb = 1;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(!unsafe { check_can_set_curbuf_disabled() });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn check_can_set_curbuf_forceit_true_when_forced_even_with_winfixbuf() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_wfb = 1;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(unsafe { check_can_set_curbuf_forceit(true) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn check_can_set_curbuf_forceit_false_when_not_forced_and_winfixbuf_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_wfb = 1;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(!unsafe { check_can_set_curbuf_forceit(false) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    /// Points `GLOBALS.firstwin`/`GLOBALS.curtab`/`GLOBALS.curwin` at
    /// the given values for the guard's lifetime, restoring all three
    /// previous values on drop - extends `WindowListGuard`'s own
    /// precedent to additionally cover `curwin`, needed by
    /// `win_has_winnr`'s own "is this the tab's current window" check.
    /// Takes `win` ONCE (used for both `firstwin` and `curwin`, since
    /// every real caller wants "the only window IS the current
    /// window") rather than as two separate parameters - deliberately
    /// avoiding a second, independent `&mut win_variable` reborrow at
    /// each call site, which would invalidate the raw pointer already
    /// handed to the first `GLOBALS` field under Stacked Borrows (a
    /// real bug caught here for real by Miri during development; see
    /// `eval/vars.rs`'s own `Box::as_mut()`-then-reborrow precedent for
    /// the same class of bug).
    struct CurwinListGuard {
        prev_firstwin: *mut WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_curwin: *mut WinT,
    }

    impl CurwinListGuard {
        fn set(win: *mut WinT, tp: *mut crate::buffer_defs::TabpageT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = CurwinListGuard {
                prev_firstwin: globals.firstwin,
                prev_curtab: globals.curtab,
                prev_curwin: globals.curwin,
            };
            globals.firstwin = win;
            globals.curtab = tp;
            globals.curwin = win;
            guard
        }
    }

    impl Drop for CurwinListGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = self.prev_firstwin;
            globals.curtab = self.prev_curtab;
            globals.curwin = self.prev_curwin;
        }
    }

    fn focusable_win(handle: crate::types_defs::HandleT) -> WinT {
        WinT {
            handle,
            w_config: crate::buffer_defs::WinConfig { focusable: true, hide: false, ..Default::default() },
            ..Default::default()
        }
    }

    // ---- win_has_winnr ----

    #[test]
    fn win_has_winnr_true_for_the_tab_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        // Deliberately NOT focusable/not-hidden - being the current
        // window always counts, regardless of w_config.
        let mut win = WinT { handle: 1, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT::default();
        // Compute both raw pointers ONCE, before any guard call, and
        // reuse the SAME pointer values everywhere below - a second,
        // independent `&mut win`/`&mut tp` reborrow after the first
        // has already been handed to a GLOBALS field would invalidate
        // it under Stacked Borrows (see CurwinListGuard's own doc
        // comment for the real bug this avoids).
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert!(unsafe { win_has_winnr(win_ptr, tp_ptr) });
    }

    #[test]
    fn win_has_winnr_true_for_a_focusable_non_hidden_non_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curwin = WinT { handle: 1, ..Default::default() };
        let mut other = focusable_win(2);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let curwin_ptr = &mut curwin as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(curwin_ptr, tp_ptr);

        assert!(unsafe { win_has_winnr(&mut other as *mut WinT, tp_ptr) });
    }

    #[test]
    fn win_has_winnr_false_for_a_hidden_non_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curwin = WinT { handle: 1, ..Default::default() };
        let mut other = WinT {
            handle: 2,
            w_config: crate::buffer_defs::WinConfig { focusable: true, hide: true, ..Default::default() },
            ..Default::default()
        };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let curwin_ptr = &mut curwin as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(curwin_ptr, tp_ptr);

        assert!(!unsafe { win_has_winnr(&mut other as *mut WinT, tp_ptr) });
    }

    // ---- win_id2win ----

    #[test]
    fn win_id2win_finds_the_matching_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = focusable_win(3);
        let mut second = WinT { w_next: &mut third as *mut WinT, ..focusable_win(2) };
        let mut first = WinT { w_next: &mut second as *mut WinT, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(first_ptr, tp_ptr);

        assert_eq!(unsafe { win_id2win(1) }, 1);
        assert_eq!(unsafe { win_id2win(2) }, 2);
        assert_eq!(unsafe { win_id2win(3) }, 3);
    }

    #[test]
    fn win_id2win_returns_0_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert_eq!(unsafe { win_id2win(999) }, 0);
    }

    // ---- win_get_tabwin ----

    #[test]
    fn win_get_tabwin_finds_a_window_in_the_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let mut first = WinT { w_next: &mut second as *mut WinT, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(tp_ptr);
        let _guard = CurwinListGuard::set(first_ptr, tp_ptr);

        assert_eq!(unsafe { win_get_tabwin(2) }, (1, 2));
    }

    #[test]
    fn win_get_tabwin_finds_a_window_in_a_non_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut other_win = focusable_win(5);
        let other_win_ptr = &mut other_win as *mut WinT;
        let mut other_tp =
            crate::buffer_defs::TabpageT { tp_firstwin: other_win_ptr, tp_curwin: other_win_ptr, ..Default::default() };
        let mut cur_win = focusable_win(1);
        let other_tp_ptr = &mut other_tp as *mut crate::buffer_defs::TabpageT;
        let mut cur_tp = crate::buffer_defs::TabpageT { tp_next: other_tp_ptr, ..Default::default() };
        // GLOBALS.first_tabpage must chain cur_tp -> other_tp for the
        // walk to find the second tab.
        let cur_win_ptr = &mut cur_win as *mut WinT;
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(cur_tp_ptr);
        let _guard = CurwinListGuard::set(cur_win_ptr, cur_tp_ptr);

        assert_eq!(unsafe { win_get_tabwin(5) }, (2, 1));
    }

    #[test]
    fn win_get_tabwin_returns_0_0_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(tp_ptr);
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert_eq!(unsafe { win_get_tabwin(999) }, (0, 0));
    }

    // ---- win_getid ----

    #[test]
    fn win_getid_with_no_args_returns_curwin_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(42);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert_eq!(unsafe { win_getid(None, None) }, 42);
    }

    #[test]
    fn win_getid_with_winnr_only_uses_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let mut first = WinT { w_next: &mut second as *mut WinT, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(first_ptr, tp_ptr);

        assert_eq!(unsafe { win_getid(Some(2), None) }, 2);
    }

    #[test]
    fn win_getid_with_winnr_0_or_negative_returns_0() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert_eq!(unsafe { win_getid(Some(0), None) }, 0);
        assert_eq!(unsafe { win_getid(Some(-1), None) }, 0);
    }

    #[test]
    fn win_getid_with_a_tabnr_that_does_not_exist_returns_minus_1() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(tp_ptr);
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert_eq!(unsafe { win_getid(Some(1), Some(99)) }, -1);
    }
}
