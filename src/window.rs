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
//! Also translated: `win_has_winnr`/`win_get_tabwin`/`win_findbuf`
//! (all real `window.c` functions), plus `win_id2win`/`win_getid`/
//! `get_winnr` (originally `static` helpers in `eval/window.c`,
//! hosted here alongside their own `window.c` dependencies rather
//! than in `eval/funcs.rs` - same "helper logic lives near its own
//! dependencies, the builtin Vimscript-facing wrapper lives in
//! `funcs.rs`" precedent as `state.rs`'s `get_mode`/`funcs.rs`'s
//! `f_mode`). All 6 need the same window/tabpage-list walk already
//! established above, plus `WinT.w_config`'s already-translated
//! `hide`/`focusable` fields for `win_has_winnr`'s own floating-
//! window-aware numbering check. `get_winnr`'s own digit+direction
//! argument form (e.g. `winnr("3j")`) needs `win_vert_neighbor`/
//! `win_horz_neighbor` (real window-layout geometry) - now translated
//! (see below), so the full digit+direction form is real too.
//!
//! Also translated: `frame2win` (a trivial leaf-of-a-frame-tree walk)
//! and `win_vert_neighbor`/`win_horz_neighbor` (`window.c`'s own
//! frame-tree neighbor-navigation algorithms for `winnr("3j")`-style
//! window movement) - both walk the already-translated `FrameT`'s
//! `fr_parent`/`fr_prev`/`fr_next`/`fr_child`/`fr_layout`/`fr_win`
//! fields plus `WinT.w_wincol`/`w_wcol` (vertical)/`w_winrow`/`w_wrow`
//! (horizontal) for the "which child is under the cursor" sub-search -
//! all fields already existed. Neither needs `win_goto` (which would
//! also redraw/switch real editor focus, not translated) - they just
//! COMPUTE a candidate window, matching `get_winnr`'s own read-only
//! use exactly.
//!
//! Also translated: `frame_fixed_height`/`frame_fixed_width` (whether
//! a frame's height/width should not be changed because of
//! `'winfixheight'`/`'winfixwidth'` - a leaf reflects its own window's
//! option value directly; a `FR_ROW`/`FR_COL` frame is fixed if
//! ANY/ALL of its children are, per the original's own exact
//! structure, needing only already-real `WinT.w_onebuf_opt.wo_wfh`/
//! `wo_wfw` fields). Translated ahead of their real callers
//! (`win_equal_rec`/`frame_new_height`/`frame_new_width`, part of the
//! larger window-resizing/equalization subsystem, not translated yet)
//! since both are small, self-contained, and have no design freedom
//! to get wrong - matching this crate's established "translate ahead
//! of a real caller" precedent.
//!
//! Also translated: `frame_minheight`/`frame_minwidth` (the minimal
//! height/width a frame needs, using `'winminheight'`/`'winminwidth'`,
//! or `'winheight'`/`'winwidth'` for a specific "next current window",
//! via already-real `OPTION_VARS.p_wh`/`p_wmh`/`p_wiw`/`p_wmw` and
//! `WinT.w_winbar_height`/`w_hsep_height`/`w_status_height`/
//! `w_vsep_width`). Introduces [`NOWIN`], a real, non-null sentinel
//! pointer value (`(win_T *)-1` in the original, distinct from both
//! null and any genuine `*mut WinT`) meaning "don't reserve at least
//! one line/column for the current window", the original's own real
//! 3-way distinction (null/`NOWIN`/a real window) for the
//! `next_curwin` parameter, kept as a raw-pointer comparison rather
//! than an `Option`-based redesign, matching the original's own
//! genuine pointer-identity semantics exactly. `FR_ROW` sums
//! (side-by-side widths add up) for `frame_minheight` but takes the
//! max for `frame_minwidth` (a column-of-rows only needs its LARGEST
//! single child's width), the same ROW/COL role-swap already
//! established for `frame_fixed_height`/`frame_fixed_width`.
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
use crate::eval::typval_defs::{TypvalT, TypvalValue};

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

/// Find all windows (across all tab pages) currently showing buffer
/// `bufnr`, returning their handles in tab/window order (`win_findbuf`,
/// `eval/window.c`).
///
/// # Safety
/// Same requirement as [`win_get_tabwin`], plus each window's
/// `w_buffer` must be a valid, non-null pointer to a live `BufT`.
#[must_use]
pub unsafe fn win_findbuf(bufnr: i32) -> Vec<crate::types_defs::HandleT> {
    let mut found = Vec::new();
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
            let w = unsafe { &*wp };
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { &*w.w_buffer }.handle == bufnr {
                found.push(w.handle);
            }
            wp = w.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    found
}

/// Get the window number for `arg` in tab page `tp` (`get_winnr`,
/// `eval/window.c`). `arg == None` means no argument was given (the
/// common case - the CURRENT window's own number, `0` for a hidden/
/// non-focusable window unless it IS the current window).
///
/// `arg == None`, `b"$"` (last window), `b"#"` (previous window), and
/// the digit+direction form (e.g. `b"3j"`, via [`win_vert_neighbor`]/
/// [`win_horz_neighbor`]) are all modeled; any other unrecognized
/// `arg` returns `0` (matching the original's own `invalid_arg` path,
/// whose real `semsg` display is omitted - message display, not
/// tractable).
///
/// # Safety
/// `tp` must be a valid, non-null pointer to a live `TabpageT`, and
/// its own window list (`tp_firstwin`/`w_next`, or
/// `GLOBALS.firstwin`/`w_next` when `tp == GLOBALS.curtab`) must
/// consist of valid, live pointers.
#[must_use]
pub unsafe fn get_winnr(tp: *const crate::buffer_defs::TabpageT, arg: Option<&[u8]>) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let is_curtab = std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab);
    let mut twin = if is_curtab {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*tp }.tp_curwin
    };

    let mut nr = 1;
    if let Some(arg) = arg {
        if arg == b"$" {
            twin = if is_curtab {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::globals::GLOBALS.get_mut() }.lastwin
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &*tp }.tp_lastwin
            };
        } else if arg == b"#" {
            twin = if is_curtab {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::globals::GLOBALS.get_mut() }.prevwin
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &*tp }.tp_prevwin
            };
            if twin.is_null() {
                nr = 0;
            }
        } else {
            let (count, consumed) = crate::charset::getdigits_int(arg, false, 0);
            let count = if count <= 0 { 1 } else { count };
            let dir = &arg[consumed..];
            let mut invalid_arg = false;
            if dir == b"j" {
                // SAFETY: forwarded from this function's own safety doc.
                twin = unsafe { win_vert_neighbor(tp, twin, false, count) };
            } else if dir == b"k" {
                // SAFETY: forwarded from this function's own safety doc.
                twin = unsafe { win_vert_neighbor(tp, twin, true, count) };
            } else if dir == b"h" {
                // SAFETY: forwarded from this function's own safety doc.
                twin = unsafe { win_horz_neighbor(tp, twin, true, count) };
            } else if dir == b"l" {
                // SAFETY: forwarded from this function's own safety doc.
                twin = unsafe { win_horz_neighbor(tp, twin, false, count) };
            } else {
                invalid_arg = true;
            }
            if invalid_arg {
                nr = 0;
            }
        }
    // SAFETY: forwarded from this function's own safety doc.
    } else if !unsafe { win_has_winnr(twin, tp) } {
        nr = 0;
    }

    if nr <= 0 {
        return 0;
    }

    nr = 0;
    let mut wp = if is_curtab {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*tp }.tp_firstwin
    };
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        nr += i32::from(unsafe { win_has_winnr(wp, tp) });
        if std::ptr::eq(wp, twin) {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    if wp.is_null() {
        nr = 0;
    }
    nr
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

/// Get the 1-based index of tab page `ftp` (`tabpage_index`). When
/// `ftp` is not found in the list (including `ftp` being null, which
/// never matches any real tab page), returns the total number of tab
/// pages plus one - matching the original's own documented contract
/// exactly (used by `tabpagenr("$")`'s own `tabpage_index(NULL) - 1`
/// idiom to get a plain tab page COUNT).
///
/// # Safety
/// Same as [`valid_tabpage`].
#[must_use]
pub unsafe fn tabpage_index(ftp: *const crate::buffer_defs::TabpageT) -> i32 {
    let mut i = 1;
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() && !std::ptr::eq(tp, ftp) {
        i += 1;
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    i
}

/// Find tab page number `n` (first one is `1`), or the current tab
/// page when `n == 0`. Returns a null pointer when not found
/// (`find_tabpage`).
///
/// # Safety
/// Same as [`valid_tabpage`].
#[must_use]
pub unsafe fn find_tabpage(n: i32) -> *mut crate::buffer_defs::TabpageT {
    if n == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    }
    let mut i = 1;
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() && i != n {
        i += 1;
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    tp
}

/// Lowest real window handle used as a genuine window ID rather than
/// a window NUMBER within the current tab page (`LOWEST_WIN_ID`,
/// `window.h`).
pub const LOWEST_WIN_ID: i32 = 1000;

/// Find window number `nr` in tabpage `tp` (`NULL` meaning `curtab`)
/// (`find_win_by_nr`, `eval/window.c`). `nr == 0` means `curwin`;
/// `nr >= `[`LOWEST_WIN_ID`] is treated as a real window ID (handle)
/// instead of a plain window number.
///
/// # Safety
/// `tp` (if non-null) must be a valid, live `TabpageT` pointer, and
/// its own `tp_firstwin`/`w_next` chain must consist of valid, live
/// pointers - same for `GLOBALS.firstwin`/`curtab`/`curwin` when `tp`
/// is null.
#[must_use]
pub unsafe fn find_win_by_nr(vp: &TypvalT, tp: *mut crate::buffer_defs::TabpageT) -> *mut WinT {
    let mut error = false;
    let mut nr = crate::eval::typval::tv_get_number_chk(vp, Some(&mut error));
    if error || nr < 0 {
        return std::ptr::null_mut();
    }
    if nr == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    }

    // This method accepts NULL as an alias for curtab.
    // SAFETY: forwarded from this function's own safety doc.
    let tp = if tp.is_null() { unsafe { crate::globals::GLOBALS.get_mut() }.curtab } else { tp };

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
        let handle = i64::from(unsafe { &*wp }.handle);
        if nr >= i64::from(LOWEST_WIN_ID) {
            if handle == nr {
                return wp;
            }
        } else {
            nr -= 1;
            if nr <= 0 {
                return wp;
            }
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    std::ptr::null_mut()
}

/// Return the window and tab pointer of window handle `id`, `NULL`
/// when not found (`win_id2wp_tp`, `eval/window.c`).
///
/// # Safety
/// `GLOBALS.first_tabpage`'s own `tp_next` chain, and each tabpage's
/// own `tp_firstwin`/`w_next` chain, must consist of valid, live
/// pointers.
#[must_use]
pub unsafe fn win_id2wp_tp(id: i32) -> (*mut WinT, *mut crate::buffer_defs::TabpageT) {
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
                return (wp, tp);
            }
            // SAFETY: forwarded from this function's own safety doc.
            wp = unsafe { &*wp }.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    (std::ptr::null_mut(), std::ptr::null_mut())
}

/// [`win_id2wp_tp`] without the tabpage out-value (`win_id2wp`,
/// `eval/window.c`).
///
/// # Safety
/// Forwarded from [`win_id2wp_tp`]'s own safety doc.
#[must_use]
pub unsafe fn win_id2wp(id: i32) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { win_id2wp_tp(id) }.0
}

/// Find a window: using a Window ID in any tab page, or using a
/// number in the current tab page (`find_win_by_nr_or_id`,
/// `eval/window.c`).
///
/// # Safety
/// Forwarded from [`win_id2wp`]/[`find_win_by_nr`]'s own safety docs.
#[must_use]
pub unsafe fn find_win_by_nr_or_id(vp: &TypvalT) -> *mut WinT {
    let nr = crate::eval::typval::tv_get_number_chk(vp, None);
    if nr >= i64::from(LOWEST_WIN_ID) {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { win_id2wp(crate::eval::typval::tv_get_number(vp) as i32) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { find_win_by_nr(vp, std::ptr::null_mut()) }
}

/// Find a window given by `wvp` (a window number or ID, `Unknown`
/// meaning `curwin`) within tabpage `tvp` (a tab number, `Unknown`
/// meaning `curtab`) (`find_tabwin`, `eval/window.c`).
///
/// # Safety
/// Forwarded from [`find_tabpage`]/[`find_win_by_nr`]'s own safety
/// docs.
#[must_use]
pub unsafe fn find_tabwin(wvp: &TypvalT, tvp: &TypvalT) -> *mut WinT {
    if matches!(wvp.value, TypvalValue::Unknown) {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    }

    let tp = if matches!(tvp.value, TypvalValue::Unknown) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab
    } else {
        let n = crate::eval::typval::tv_get_number(tvp) as i32;
        if n >= 0 {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { find_tabpage(n) }
        } else {
            std::ptr::null_mut()
        }
    };

    if tp.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { find_win_by_nr(wvp, tp) }
}

/// Get the leaf window contained within frame `frp` (`frame2win`) - a
/// non-leaf frame's own `fr_child` chain always bottoms out at a leaf
/// (`fr_win.is_some()`) eventually, matching the original's own
/// unconditional `while (frp->fr_win == NULL) { frp = frp->fr_child; }`
/// loop (no bounds/null check - a well-formed frame tree, which is all
/// this crate can ever construct, always terminates).
///
/// # Safety
/// `frp` must be a valid, non-null pointer to a live `FrameT`, and its
/// own `fr_child` chain must consist of valid, live `FrameT` pointers
/// down to a leaf.
#[must_use]
pub unsafe fn frame2win(mut frp: *const crate::buffer_defs::FrameT) -> *mut WinT {
    loop {
        // SAFETY: forwarded from this function's own safety doc.
        let fr = unsafe { &*frp };
        if !fr.fr_win.is_null() {
            return fr.fr_win;
        }
        frp = fr.fr_child;
    }
}

/// Return `true` if the height of frame `frp` should not be changed
/// because of `'winfixheight'` (`frame_fixed_height`). A leaf frame is
/// fixed height exactly when its own window's `'winfixheight'` is
/// set; a `FR_ROW` (side-by-side) frame is fixed height if ANY child
/// is; a `FR_COL` (stacked) frame is fixed height only if ALL children
/// are - matching the original's own `FOR_ALL_FRAMES` walk over
/// `fr_child`/`fr_next` exactly (translated as 2 separate, explicit
/// loops rather than one parameterized "any vs all" loop, matching
/// the original's own 2-branch structure directly rather than a
/// cleverer-but-less-obviously-correct consolidation).
///
/// # Safety
/// `frp` must be a valid, non-null pointer to a live `FrameT`, whose
/// own `fr_child`/`fr_next` chain (if any) consists entirely of valid,
/// live `FrameT` pointers, and whose `fr_win` (if non-null) is a
/// valid, live `WinT` pointer.
#[must_use]
pub unsafe fn frame_fixed_height(frp: *const crate::buffer_defs::FrameT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*frp };
    if !fr.fr_win.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { &*fr.fr_win }.w_onebuf_opt.wo_wfh != 0;
    }
    if fr.fr_layout == crate::buffer_defs::FR_ROW {
        // Fixed height if ONE of the frames in the row is fixed height.
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { frame_fixed_height(child) } {
                return true;
            }
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        return false;
    }
    // fr.fr_layout == FR_COL: fixed height if ALL of the frames in the
    // column are fixed height.
    let mut child = fr.fr_child;
    while !child.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { frame_fixed_height(child) } {
            return false;
        }
        // SAFETY: forwarded from this function's own safety doc.
        child = unsafe { &*child }.fr_next;
    }
    true
}

/// Return `true` if the width of frame `frp` should not be changed
/// because of `'winfixwidth'` (`frame_fixed_width`) - the `FR_COL`/
/// `FR_ROW` "any"/"all" roles are swapped relative to
/// [`frame_fixed_height`] (a `FR_COL` frame is fixed width if ANY
/// child is; a `FR_ROW` frame only if ALL are), matching the
/// original's own exact structure.
///
/// # Safety
/// Same as [`frame_fixed_height`].
#[must_use]
pub unsafe fn frame_fixed_width(frp: *const crate::buffer_defs::FrameT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*frp };
    if !fr.fr_win.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { &*fr.fr_win }.w_onebuf_opt.wo_wfw != 0;
    }
    if fr.fr_layout == crate::buffer_defs::FR_COL {
        // Fixed width if ONE of the frames in the column is fixed width.
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { frame_fixed_width(child) } {
                return true;
            }
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        return false;
    }
    // fr.fr_layout == FR_ROW: fixed width if ALL of the frames in the
    // row are fixed width.
    let mut child = fr.fr_child;
    while !child.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { frame_fixed_width(child) } {
            return false;
        }
        // SAFETY: forwarded from this function's own safety doc.
        child = unsafe { &*child }.fr_next;
    }
    true
}

/// Sentinel value for [`frame_minheight`]/[`frame_minwidth`]'s own
/// `next_curwin` parameter, meaning "don't reserve at least one line/
/// column for the current window" (`NOWIN`, `((win_T *)-1)` in the
/// original - a real, non-null, but deliberately invalid pointer
/// value, distinguished from both a null pointer, the ordinary case,
/// and any genuine `*mut WinT`).
pub const NOWIN: *mut WinT = -1isize as *mut WinT;

/// Compute the minimal height for frame `topfrp` (`frame_minheight`),
/// using `'winminheight'`. When `next_curwin` is a real window
/// pointer, uses `'winheight'` for THAT window instead. When
/// `next_curwin` is [`NOWIN`], don't reserve at least one line for
/// the current window (`GLOBALS.curwin`).
///
/// # Safety
/// `topfrp` must be a valid, non-null pointer to a live `FrameT`,
/// whose own `fr_child`/`fr_next` chain (if any) consists entirely of
/// valid, live `FrameT` pointers, and whose `fr_win` (if non-null) is
/// a valid, live `WinT` pointer. `next_curwin` must be either
/// [`NOWIN`], null, or a valid, live `WinT` pointer. Touches
/// `crate::globals::GLOBALS`/`crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn frame_minheight(topfrp: *const crate::buffer_defs::FrameT, next_curwin: *mut WinT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*topfrp };
    if !fr.fr_win.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let win = unsafe { &*fr.fr_win };
        // Combined height of window bar and separator column or status line.
        let extra_height = win.w_winbar_height + win.w_hsep_height + win.w_status_height;
        // SAFETY: forwarded from this function's own safety doc.
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        if std::ptr::eq(fr.fr_win, next_curwin) {
            opts.p_wh as i32 + extra_height
        } else {
            let mut m = opts.p_wmh as i32 + extra_height;
            // SAFETY: forwarded from this function's own safety doc.
            let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
            if std::ptr::eq(fr.fr_win, curwin as *const WinT) && next_curwin.is_null() {
                // Current window is minimal one line high.
                if opts.p_wmh == 0 {
                    m += 1;
                }
            }
            m
        }
    } else if fr.fr_layout == crate::buffer_defs::FR_ROW {
        // get the minimal height from each frame in this row
        let mut m = 0;
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let n = unsafe { frame_minheight(child, next_curwin) };
            if n > m {
                m = n;
            }
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        m
    } else {
        // Add up the minimal heights for all frames in this column.
        let mut m = 0;
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            m += unsafe { frame_minheight(child, next_curwin) };
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        m
    }
}

/// Compute the minimal width for frame `topfrp` (`frame_minwidth`),
/// using `'winminwidth'`. When `next_curwin` is a real window
/// pointer, uses `'winwidth'` for THAT window instead. When
/// `next_curwin` is [`NOWIN`], don't reserve at least one column for
/// the current window (`GLOBALS.curwin`).
///
/// # Safety
/// Same as [`frame_minheight`].
#[must_use]
pub unsafe fn frame_minwidth(topfrp: *const crate::buffer_defs::FrameT, next_curwin: *mut WinT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*topfrp };
    if !fr.fr_win.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let win = unsafe { &*fr.fr_win };
        // SAFETY: forwarded from this function's own safety doc.
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        if std::ptr::eq(fr.fr_win, next_curwin) {
            opts.p_wiw as i32 + win.w_vsep_width
        } else {
            // window: minimal width of the window plus separator column
            let mut m = opts.p_wmw as i32 + win.w_vsep_width;
            // SAFETY: forwarded from this function's own safety doc.
            let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
            // Current window is minimal one column wide.
            if opts.p_wmw == 0 && std::ptr::eq(fr.fr_win, curwin as *const WinT) && next_curwin.is_null() {
                m += 1;
            }
            m
        }
    } else if fr.fr_layout == crate::buffer_defs::FR_COL {
        // get the minimal width from each frame in this column
        let mut m = 0;
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let n = unsafe { frame_minwidth(child, next_curwin) };
            m = m.max(n);
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        m
    } else {
        // Add up the minimal widths for all frames in this row.
        let mut m = 0;
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            m += unsafe { frame_minwidth(child, next_curwin) };
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        m
    }
}

/// Return the default value for `'scroll'` for window `wp`
/// (`win_default_scroll`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
#[must_use]
pub unsafe fn win_default_scroll(wp: *const WinT) -> crate::types_defs::OptInt {
    // SAFETY: forwarded from this function's own safety doc.
    let w_view_height = unsafe { &*wp }.w_view_height;
    crate::types_defs::OptInt::from((w_view_height / 2).max(1))
}

/// Return the number of lines used by the tab page line
/// (`tabline_height`), via `'showtabline'`.
///
/// # Safety
/// Touches `crate::globals::GLOBALS`/`crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn tabline_height() -> i32 {
    if crate::ui::ui_has(crate::ui::UiExtension::Tabline) {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let first_tabpage = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    debug_assert!(!first_tabpage.is_null());
    // SAFETY: forwarded from this function's own safety doc.
    let only_one_tab = unsafe { &*first_tabpage }.tp_next.is_null();
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_stal {
        0 => 0,
        1 if only_one_tab => 0,
        1 => 1,
        _ => 1,
    }
}

/// Return the number of lines used by the global statusline
/// (`global_stl_height`), via `'laststatus'`.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn global_stl_height() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls == 3 {
        STATUS_HEIGHT
    } else {
        0
    }
}

/// Return the minimal number of rows needed on the screen to display
/// the current number of windows for tab page `tp` (`min_rows`).
///
/// # Safety
/// `tp` must be a valid, non-null pointer to a live `TabpageT`, whose
/// own `tp_topframe` frame tree consists of valid, live pointers.
/// Touches `crate::globals::GLOBALS`/`crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn min_rows(tp: *const crate::buffer_defs::TabpageT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if globals.firstwin.is_null() {
        // not initialized yet
        return MIN_LINES;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let t = unsafe { &*tp };
    // SAFETY: forwarded from this function's own safety doc.
    let mut total = unsafe { frame_minheight(t.tp_topframe, std::ptr::null_mut()) };
    // SAFETY: forwarded from this function's own safety doc.
    total += unsafe { tabline_height() } + unsafe { global_stl_height() };
    let ch_used = if std::ptr::eq(tp, globals.curtab) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch
    } else {
        t.tp_ch_used
    };
    if ch_used > 0 {
        total += 1; // count the room for the command line
    }
    total
}

/// Return the minimal number of rows needed on the screen to display
/// the current number of windows for ALL tab pages
/// (`min_rows_for_all_tabpages`).
///
/// # Safety
/// `crate::globals::GLOBALS.first_tabpage`'s own `tp_next` chain must
/// consist of valid, live `TabpageT` pointers, each with a valid,
/// live `tp_topframe` frame tree. Touches
/// `crate::globals::GLOBALS`/`crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn min_rows_for_all_tabpages() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if globals.firstwin.is_null() {
        // not initialized yet
        return MIN_LINES;
    }

    let mut total = 0;
    let mut tp = globals.first_tabpage;
    while !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let t = unsafe { &*tp };
        // SAFETY: forwarded from this function's own safety doc.
        let mut n = unsafe { frame_minheight(t.tp_topframe, std::ptr::null_mut()) };
        let ch_used = if std::ptr::eq(tp, globals.curtab) {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch
        } else {
            t.tp_ch_used
        };
        if ch_used > 0 {
            n += 1; // count the room for the command line
        }
        total = total.max(n);
        tp = t.tp_next;
    }
    // SAFETY: forwarded from this function's own safety doc.
    total += unsafe { tabline_height() } + unsafe { global_stl_height() };
    total
}

/// Add a status line to windows at the bottom of `frp`
/// (`frame_add_statusline`). Does NOT check if there is room, matching
/// the original's own documented caveat.
///
/// # Safety
/// `frp` must be a valid, non-null pointer to a live `FrameT`, whose
/// own `fr_child`/`fr_next` chain (if any) consists entirely of valid,
/// live `FrameT` pointers, and whose `fr_win` (for a leaf) is a valid,
/// live `WinT` pointer.
pub unsafe fn frame_add_statusline(frp: *mut crate::buffer_defs::FrameT) {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*frp };
    if fr.fr_layout == crate::buffer_defs::FR_LEAF {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *fr.fr_win }.w_status_height = STATUS_HEIGHT;
    } else if fr.fr_layout == crate::buffer_defs::FR_ROW {
        // Handle all the frames in the row.
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { frame_add_statusline(child) };
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
    } else {
        debug_assert_eq!(fr.fr_layout, crate::buffer_defs::FR_COL);
        // Only need to handle the last frame in the column.
        let mut child = fr.fr_child;
        // SAFETY: forwarded from this function's own safety doc.
        while !unsafe { &*child }.fr_next.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { frame_add_statusline(child) };
    }
}

/// Get the window `count` positions above (`up == true`) or below
/// `wp` in `tp`'s frame tree (`win_vert_neighbor`). Returns `wp`
/// itself (via `foundfr` staying `wp.w_frame`) if no such neighbor
/// exists.
///
/// # Safety
/// `wp`/`tp` must be valid, non-null pointers; `wp.w_frame`'s own
/// `fr_parent`/`fr_next`/`fr_prev`/`fr_child` chains, and `tp`'s own
/// `tp_topframe`, must consist of valid, live pointers.
#[must_use]
pub unsafe fn win_vert_neighbor(
    tp: *const crate::buffer_defs::TabpageT,
    wp: *mut WinT,
    up: bool,
    count: i32,
) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &*wp };
    let mut foundfr = w.w_frame;

    if w.w_floating {
        // SAFETY: forwarded from this function's own safety doc.
        let prevwin = unsafe { crate::globals::GLOBALS.get_mut() }.prevwin;
        // SAFETY: forwarded from this function's own safety doc.
        return if unsafe { win_valid(prevwin) } && !unsafe { &*prevwin }.w_floating {
            prevwin
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
        };
    }

    let mut remaining = count;
    'outer: while remaining > 0 {
        remaining -= 1;
        // SAFETY: forwarded from this function's own safety doc.
        let mut fr = foundfr;
        let nfr;
        loop {
            if std::ptr::eq(fr, unsafe { &*tp }.tp_topframe) {
                break 'outer;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let f = unsafe { &*fr };
            let candidate = if up { f.fr_prev } else { f.fr_next };
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { &*f.fr_parent }.fr_layout == crate::buffer_defs::FR_COL && !candidate.is_null() {
                nfr = candidate;
                break;
            }
            fr = f.fr_parent;
        }

        // Now go downwards to find the bottom or top frame in it.
        let mut nfr = nfr;
        loop {
            // SAFETY: forwarded from this function's own safety doc.
            let n = unsafe { &*nfr };
            if n.fr_layout == crate::buffer_defs::FR_LEAF {
                foundfr = nfr;
                break;
            }
            let mut fr = n.fr_child;
            if n.fr_layout == crate::buffer_defs::FR_ROW {
                // Find the frame at the cursor column.
                loop {
                    // SAFETY: forwarded from this function's own safety doc.
                    let f = unsafe { &*fr };
                    if f.fr_next.is_null() {
                        break;
                    }
                    // SAFETY: forwarded from this function's own safety doc.
                    let fw = unsafe { &*frame2win(fr) };
                    if fw.w_wincol + f.fr_width > w.w_wincol + w.w_wcol {
                        break;
                    }
                    fr = f.fr_next;
                }
            }
            if n.fr_layout == crate::buffer_defs::FR_COL && up {
                loop {
                    // SAFETY: forwarded from this function's own safety doc.
                    let f = unsafe { &*fr };
                    if f.fr_next.is_null() {
                        break;
                    }
                    fr = f.fr_next;
                }
            }
            nfr = fr;
        }
    }

    if foundfr.is_null() { std::ptr::null_mut() } else { unsafe { &*foundfr }.fr_win }
}

/// Get the window `count` positions to the left (`left == true`) or
/// right of `wp` in `tp`'s frame tree (`win_horz_neighbor`). Returns
/// `wp` itself (via `foundfr` staying `wp.w_frame`) if no such
/// neighbor exists.
///
/// # Safety
/// Same requirements as [`win_vert_neighbor`].
#[must_use]
pub unsafe fn win_horz_neighbor(
    tp: *const crate::buffer_defs::TabpageT,
    wp: *mut WinT,
    left: bool,
    count: i32,
) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &*wp };
    let mut foundfr = w.w_frame;

    if w.w_floating {
        // SAFETY: forwarded from this function's own safety doc.
        let prevwin = unsafe { crate::globals::GLOBALS.get_mut() }.prevwin;
        // SAFETY: forwarded from this function's own safety doc.
        return if unsafe { win_valid(prevwin) } && !unsafe { &*prevwin }.w_floating {
            prevwin
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
        };
    }

    let mut remaining = count;
    'outer: while remaining > 0 {
        remaining -= 1;
        // SAFETY: forwarded from this function's own safety doc.
        let mut fr = foundfr;
        let nfr;
        loop {
            if std::ptr::eq(fr, unsafe { &*tp }.tp_topframe) {
                break 'outer;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let f = unsafe { &*fr };
            let candidate = if left { f.fr_prev } else { f.fr_next };
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { &*f.fr_parent }.fr_layout == crate::buffer_defs::FR_ROW && !candidate.is_null() {
                nfr = candidate;
                break;
            }
            fr = f.fr_parent;
        }

        // Now go downwards to find the leftmost or rightmost frame in it.
        let mut nfr = nfr;
        loop {
            // SAFETY: forwarded from this function's own safety doc.
            let n = unsafe { &*nfr };
            if n.fr_layout == crate::buffer_defs::FR_LEAF {
                foundfr = nfr;
                break;
            }
            let mut fr = n.fr_child;
            if n.fr_layout == crate::buffer_defs::FR_COL {
                // Find the frame at the cursor row.
                loop {
                    // SAFETY: forwarded from this function's own safety doc.
                    let f = unsafe { &*fr };
                    if f.fr_next.is_null() {
                        break;
                    }
                    // SAFETY: forwarded from this function's own safety doc.
                    let fw = unsafe { &*frame2win(fr) };
                    if fw.w_winrow + f.fr_height > w.w_winrow + w.w_wrow {
                        break;
                    }
                    fr = f.fr_next;
                }
            }
            if n.fr_layout == crate::buffer_defs::FR_ROW && left {
                loop {
                    // SAFETY: forwarded from this function's own safety doc.
                    let f = unsafe { &*fr };
                    if f.fr_next.is_null() {
                        break;
                    }
                    fr = f.fr_next;
                }
            }
            nfr = fr;
        }
    }

    if foundfr.is_null() { std::ptr::null_mut() } else { unsafe { &*foundfr }.fr_win }
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

/// Set the width/height that a window will occupy, other than what's
/// used for the 'winbar'/status/vertical-separator lines
/// (`win_set_inner_size`).
///
/// Only the "no real size change" fast path (both branches' own guard
/// conditions - `height != prev_height`/`width != wp.w_view_width` -
/// false) is translated: the "real work" bodies each need substantial
/// additional machinery not yet translated (`validate_cursor`/
/// `set_fraction`/`win_comp_scroll`/`scroll_to_fraction` for the
/// height branch; `curs_columns` for the width branch - beyond the
/// pure `redraw_later` scheduling omitted per this crate's established
/// policy) - `unimplemented!()`s if either is actually reached. This
/// crate's own real caller, `winrestview()` (via [`win_new_height`]/
/// [`win_new_width`]), always calls with the window's OWN current
/// height/width, which - for any window whose `w_view_height`/
/// `w_view_width` are already consistent with its `w_height`/`w_width`
/// (the normal, already-configured case) - never triggers either
/// branch.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn win_set_inner_size(wp: *mut WinT, _valid_cursor: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    let width = if w.w_width_request == 0 { w.w_width } else { w.w_width_request };

    let prev_height = w.w_view_height;
    let height = if w.w_height_request == 0 { (w.w_height - w.w_winbar_height).max(0) } else { w.w_height_request };

    if height != prev_height {
        unimplemented!(
            "win_set_inner_size: a real height change needs validate_cursor/set_fraction/\
             win_comp_scroll/scroll_to_fraction, not yet translated"
        );
    }

    if width != w.w_view_width {
        unimplemented!(
            "win_set_inner_size: a real width change needs curs_columns, not yet translated"
        );
    }
}

/// Set the width of window `wp` (`win_new_width`).
///
/// # Safety
/// Forwarded from [`win_set_inner_size`]'s own safety doc.
pub unsafe fn win_new_width(wp: *mut WinT, width: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_width = width.max(0);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_pos_changed = true;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { win_set_inner_size(wp, true) };
}

/// Set the height of window `wp` (`win_new_height`).
///
/// # Safety
/// Forwarded from [`win_set_inner_size`]'s own safety doc.
pub unsafe fn win_new_height(wp: *mut WinT, height: i32) {
    let height = height.max(0);
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    if w.w_height == height {
        // nothing to do
        return;
    }
    w.w_height = height;
    w.w_pos_changed = true;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { win_set_inner_size(wp, true) };
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

    // ---- tabpage_index / find_tabpage ----

    #[test]
    fn tabpage_index_finds_head_of_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = FirstTabpageGuard::set(tp_ptr);

        assert_eq!(unsafe { tabpage_index(tp_ptr) }, 1);
    }

    #[test]
    fn tabpage_index_finds_a_later_entry() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = crate::buffer_defs::TabpageT::default();
        let third_ptr = &mut third as *mut crate::buffer_defs::TabpageT;
        let mut second = crate::buffer_defs::TabpageT { tp_next: third_ptr, ..Default::default() };
        let second_ptr = &mut second as *mut crate::buffer_defs::TabpageT;
        let mut first = crate::buffer_defs::TabpageT { tp_next: second_ptr, ..Default::default() };
        let first_ptr = &mut first as *mut crate::buffer_defs::TabpageT;
        let _guard = FirstTabpageGuard::set(first_ptr);

        assert_eq!(unsafe { tabpage_index(first_ptr) }, 1);
        assert_eq!(unsafe { tabpage_index(second_ptr) }, 2);
        assert_eq!(unsafe { tabpage_index(third_ptr) }, 3);
    }

    #[test]
    fn tabpage_index_returns_count_plus_one_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = crate::buffer_defs::TabpageT::default();
        let mut first =
            crate::buffer_defs::TabpageT { tp_next: &mut second as *mut crate::buffer_defs::TabpageT, ..Default::default() };
        let first_ptr = &mut first as *mut crate::buffer_defs::TabpageT;
        let _guard = FirstTabpageGuard::set(first_ptr);

        // A null pointer never matches any real tab page - same as
        // the original's own tabpagenr("$") = tabpage_index(NULL) - 1
        // idiom (2 tabs -> index 3 -> "$" = 2).
        assert_eq!(unsafe { tabpage_index(std::ptr::null()) }, 3);
    }

    #[test]
    fn find_tabpage_zero_returns_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = tp_ptr;

        assert_eq!(unsafe { find_tabpage(0) }, tp_ptr);

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn find_tabpage_finds_by_1_based_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = crate::buffer_defs::TabpageT::default();
        let second_ptr = &mut second as *mut crate::buffer_defs::TabpageT;
        let mut first = crate::buffer_defs::TabpageT { tp_next: second_ptr, ..Default::default() };
        let first_ptr = &mut first as *mut crate::buffer_defs::TabpageT;
        let _guard = FirstTabpageGuard::set(first_ptr);

        assert_eq!(unsafe { find_tabpage(1) }, first_ptr);
        assert_eq!(unsafe { find_tabpage(2) }, second_ptr);
    }

    #[test]
    fn find_tabpage_returns_null_when_out_of_range() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        assert!(unsafe { find_tabpage(99) }.is_null());
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

    // ---- win_findbuf ----

    fn win_with_buffer(handle: crate::types_defs::HandleT, buf: *mut crate::buffer_defs::BufT) -> WinT {
        WinT { handle, w_buffer: buf, ..focusable_win(handle) }
    }

    #[test]
    fn win_findbuf_finds_windows_in_the_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf_a = crate::buffer_defs::BufT { handle: 10, ..Default::default() };
        let mut buf_b = crate::buffer_defs::BufT { handle: 20, ..Default::default() };
        let buf_a_ptr = &mut buf_a as *mut crate::buffer_defs::BufT;
        let buf_b_ptr = &mut buf_b as *mut crate::buffer_defs::BufT;
        let mut second = win_with_buffer(2, buf_b_ptr);
        let mut first = WinT { w_next: &mut second as *mut WinT, ..win_with_buffer(1, buf_a_ptr) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(tp_ptr);
        let _guard = CurwinListGuard::set(first_ptr, tp_ptr);

        assert_eq!(unsafe { win_findbuf(10) }, vec![1]);
        assert_eq!(unsafe { win_findbuf(20) }, vec![2]);
    }

    #[test]
    fn win_findbuf_returns_empty_for_an_unknown_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 10, ..Default::default() };
        let mut win = win_with_buffer(1, &mut buf as *mut crate::buffer_defs::BufT);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(tp_ptr);
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert!(unsafe { win_findbuf(999) }.is_empty());
    }

    #[test]
    fn win_findbuf_finds_a_window_in_a_non_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut other_buf = crate::buffer_defs::BufT { handle: 30, ..Default::default() };
        let mut other_win = win_with_buffer(7, &mut other_buf as *mut crate::buffer_defs::BufT);
        let other_win_ptr = &mut other_win as *mut WinT;
        let mut other_tp =
            crate::buffer_defs::TabpageT { tp_firstwin: other_win_ptr, tp_curwin: other_win_ptr, ..Default::default() };
        let mut cur_buf = crate::buffer_defs::BufT { handle: 10, ..Default::default() };
        let mut cur_win = win_with_buffer(1, &mut cur_buf as *mut crate::buffer_defs::BufT);
        let other_tp_ptr = &mut other_tp as *mut crate::buffer_defs::TabpageT;
        let mut cur_tp = crate::buffer_defs::TabpageT { tp_next: other_tp_ptr, ..Default::default() };
        let cur_win_ptr = &mut cur_win as *mut WinT;
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(cur_tp_ptr);
        let _guard = CurwinListGuard::set(cur_win_ptr, cur_tp_ptr);

        assert_eq!(unsafe { win_findbuf(30) }, vec![7]);
    }

    // ---- get_winnr ----

    /// Points `GLOBALS.firstwin`/`curtab`/`curwin`/`lastwin`/`prevwin`
    /// at the given values for the guard's lifetime, restoring all
    /// previous values on drop - a `get_winnr`-specific fixture since
    /// (unlike every other function tested above) it needs `firstwin`
    /// and `curwin` to legitimately be TWO DIFFERENT windows (to prove
    /// the counting walk finds a non-head current window).
    struct WinnrGlobalsGuard {
        prev_firstwin: *mut WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_curwin: *mut WinT,
        prev_lastwin: *mut WinT,
        prev_prevwin: *mut WinT,
    }

    impl WinnrGlobalsGuard {
        fn set(firstwin: *mut WinT, tp: *mut crate::buffer_defs::TabpageT, curwin: *mut WinT, lastwin: *mut WinT, prevwin: *mut WinT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = WinnrGlobalsGuard {
                prev_firstwin: globals.firstwin,
                prev_curtab: globals.curtab,
                prev_curwin: globals.curwin,
                prev_lastwin: globals.lastwin,
                prev_prevwin: globals.prevwin,
            };
            globals.firstwin = firstwin;
            globals.curtab = tp;
            globals.curwin = curwin;
            globals.lastwin = lastwin;
            globals.prevwin = prevwin;
            guard
        }
    }

    impl Drop for WinnrGlobalsGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = self.prev_firstwin;
            globals.curtab = self.prev_curtab;
            globals.curwin = self.prev_curwin;
            globals.lastwin = self.prev_lastwin;
            globals.prevwin = self.prev_prevwin;
        }
    }

    #[test]
    fn get_winnr_no_arg_returns_the_current_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = focusable_win(3);
        let third_ptr = &mut third as *mut WinT;
        let mut second = WinT { w_next: third_ptr, ..focusable_win(2) };
        let second_ptr = &mut second as *mut WinT;
        let mut first = WinT { w_next: second_ptr, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(first_ptr, tp_ptr, second_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert_eq!(unsafe { get_winnr(tp_ptr, None) }, 2);
    }

    #[test]
    fn get_winnr_dollar_arg_returns_the_last_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = focusable_win(3);
        let third_ptr = &mut third as *mut WinT;
        let mut second = WinT { w_next: third_ptr, ..focusable_win(2) };
        let mut first = WinT { w_next: &mut second as *mut WinT, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(first_ptr, tp_ptr, first_ptr, third_ptr, std::ptr::null_mut());

        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"$")) }, 3);
    }

    #[test]
    fn get_winnr_hash_arg_returns_the_previous_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let second_ptr = &mut second as *mut WinT;
        let mut first = WinT { w_next: second_ptr, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(first_ptr, tp_ptr, first_ptr, std::ptr::null_mut(), second_ptr);

        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"#")) }, 2);
    }

    #[test]
    fn get_winnr_hash_arg_returns_0_when_no_previous_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win_ptr, tp_ptr, win_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"#")) }, 0);
    }

    #[test]
    fn get_winnr_unrecognized_arg_returns_0() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win_ptr, tp_ptr, win_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"xyz")) }, 0);
        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"3")) }, 0);
    }

    #[test]
    fn get_winnr_digit_direction_form_navigates_to_a_real_neighbor() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win1 = focusable_win(1);
        let mut win2 = focusable_win(2);
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mut leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let leaf1_ptr = &mut leaf1 as *mut crate::buffer_defs::FrameT;
        // SAFETY: `leaf2_ptr`/`leaf1_ptr` are valid, live pointers into
        // this test's own locals.
        unsafe { (*leaf2_ptr).fr_prev = leaf1_ptr };
        let mut col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: leaf1_ptr,
            ..Default::default()
        };
        let col_ptr = &mut col as *mut crate::buffer_defs::FrameT;
        // SAFETY: forwarded from the earlier comment.
        unsafe {
            (*leaf1_ptr).fr_parent = col_ptr;
            (*leaf2_ptr).fr_parent = col_ptr;
            (*win1_ptr).w_frame = leaf1_ptr;
            (*win2_ptr).w_frame = leaf2_ptr;
            // Separate, plain window-LIST linkage (distinct from the
            // frame-tree linkage above) - needed by get_winnr's own
            // trailing "count up to twin" walk from `firstwin`.
            (*win1_ptr).w_next = win2_ptr;
        }
        let mut tp = crate::buffer_defs::TabpageT { tp_topframe: col_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win1_ptr, tp_ptr, win1_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        // From win1 (top), "1j" (down) reaches win2.
        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"1j")) }, 2);
        // From win1 (top), "1k" (up) has no neighbor - stays on win1.
        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"1k")) }, 1);
    }

    #[test]
    fn win_vert_neighbor_returns_wp_itself_when_no_neighbor_exists() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let mut leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let leaf_ptr = &mut leaf as *mut crate::buffer_defs::FrameT;
        // SAFETY: `win_ptr` is a valid, live pointer into this test's
        // own local.
        unsafe { (*win_ptr).w_frame = leaf_ptr };
        let tp = crate::buffer_defs::TabpageT { tp_topframe: leaf_ptr, ..Default::default() };

        let found = unsafe { win_vert_neighbor(&tp, win_ptr, true, 1) };
        assert_eq!(found, win_ptr);
    }

    #[test]
    fn win_horz_neighbor_finds_the_right_neighbor_in_a_row_split() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win1 = focusable_win(1);
        let mut win2 = focusable_win(2);
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mut leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let leaf1_ptr = &mut leaf1 as *mut crate::buffer_defs::FrameT;
        // SAFETY: `leaf1_ptr`/`leaf2_ptr` are valid, live pointers into
        // this test's own locals.
        unsafe { (*leaf2_ptr).fr_prev = leaf1_ptr };
        let mut row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: leaf1_ptr,
            ..Default::default()
        };
        let row_ptr = &mut row as *mut crate::buffer_defs::FrameT;
        // SAFETY: forwarded from the earlier comment.
        unsafe {
            (*leaf1_ptr).fr_parent = row_ptr;
            (*leaf2_ptr).fr_parent = row_ptr;
            (*win1_ptr).w_frame = leaf1_ptr;
            (*win2_ptr).w_frame = leaf2_ptr;
        }
        let tp = crate::buffer_defs::TabpageT { tp_topframe: row_ptr, ..Default::default() };

        let found = unsafe { win_horz_neighbor(&tp, win1_ptr, false, 1) };
        assert_eq!(found, win2_ptr);
    }

    #[test]
    fn frame2win_walks_down_to_the_leaf() {
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let leaf_ptr = &leaf as *const crate::buffer_defs::FrameT as *mut crate::buffer_defs::FrameT;
        let root =
            crate::buffer_defs::FrameT { fr_layout: crate::buffer_defs::FR_ROW, fr_child: leaf_ptr, ..Default::default() };
        assert_eq!(unsafe { frame2win(&root) }, win_ptr);
    }

    // ---- frame_fixed_height / frame_fixed_width ----

    /// Builds a `WinT` with a specific `'winfixheight'`/`'winfixwidth'`
    /// value pre-set.
    fn win_with_fixed(wfh: i32, wfw: i32) -> WinT {
        let mut w = focusable_win(1);
        w.w_onebuf_opt.wo_wfh = wfh;
        w.w_onebuf_opt.wo_wfw = wfw;
        w
    }

    #[test]
    fn frame_fixed_height_leaf_reflects_the_window_option() {
        let mut fixed_win = win_with_fixed(1, 0);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let fixed_leaf = crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, ..Default::default() };
        assert!(unsafe { frame_fixed_height(&fixed_leaf) });

        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;
        let free_leaf = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        assert!(!unsafe { frame_fixed_height(&free_leaf) });
    }

    #[test]
    fn frame_fixed_height_row_is_true_if_any_child_is_fixed() {
        let mut fixed_win = win_with_fixed(1, 0);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(unsafe { frame_fixed_height(&row) });
    }

    #[test]
    fn frame_fixed_height_row_is_false_if_no_child_is_fixed() {
        let mut free_win1 = win_with_fixed(0, 0);
        let free_win1_ptr = &mut free_win1 as *mut WinT;
        let mut free_win2 = win_with_fixed(0, 0);
        let free_win2_ptr = &mut free_win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: free_win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: free_win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(!unsafe { frame_fixed_height(&row) });
    }

    #[test]
    fn frame_fixed_height_col_needs_every_child_fixed() {
        let mut fixed_win = win_with_fixed(1, 0);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;

        // One fixed, one not: FR_COL as a whole is NOT fixed.
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mixed_leaf1 =
            crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let mixed_col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &mixed_leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(!unsafe { frame_fixed_height(&mixed_col) });

        // Both fixed: FR_COL as a whole IS fixed.
        let mut fixed_win2 = win_with_fixed(1, 0);
        let fixed_win2_ptr = &mut fixed_win2 as *mut WinT;
        let mut both_leaf2 = crate::buffer_defs::FrameT { fr_win: fixed_win2_ptr, ..Default::default() };
        let both_leaf2_ptr = &mut both_leaf2 as *mut crate::buffer_defs::FrameT;
        let both_leaf1 = crate::buffer_defs::FrameT {
            fr_win: fixed_win_ptr,
            fr_next: both_leaf2_ptr,
            ..Default::default()
        };
        let both_col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &both_leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(unsafe { frame_fixed_height(&both_col) });
    }

    #[test]
    fn frame_fixed_width_leaf_reflects_the_window_option() {
        let mut fixed_win = win_with_fixed(0, 1);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let fixed_leaf = crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, ..Default::default() };
        assert!(unsafe { frame_fixed_width(&fixed_leaf) });

        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;
        let free_leaf = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        assert!(!unsafe { frame_fixed_width(&free_leaf) });
    }

    #[test]
    fn frame_fixed_width_col_is_true_if_any_child_is_fixed() {
        // FR_COL's "any" role is the OPPOSITE of frame_fixed_height's
        // own FR_ROW-is-"any" - verifying the roles are genuinely
        // swapped, not accidentally identical to frame_fixed_height.
        let mut fixed_win = win_with_fixed(0, 1);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(unsafe { frame_fixed_width(&col) });
    }

    #[test]
    fn frame_fixed_width_row_needs_every_child_fixed() {
        let mut fixed_win = win_with_fixed(0, 1);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(!unsafe { frame_fixed_width(&row) });
    }

    // ---- frame_minheight / frame_minwidth ----

    /// RAII guard temporarily setting `OPTION_VARS.p_wh`/`p_wmh`/
    /// `p_wiw`/`p_wmw`, restoring the previous values on drop. Caller
    /// must hold `global_state_test_lock()` for the whole lifetime.
    struct MinSizeOptsGuard {
        prev_wh: crate::types_defs::OptInt,
        prev_wmh: crate::types_defs::OptInt,
        prev_wiw: crate::types_defs::OptInt,
        prev_wmw: crate::types_defs::OptInt,
    }
    impl MinSizeOptsGuard {
        fn set(wh: i64, wmh: i64, wiw: i64, wmw: i64) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard = MinSizeOptsGuard {
                prev_wh: opts.p_wh,
                prev_wmh: opts.p_wmh,
                prev_wiw: opts.p_wiw,
                prev_wmw: opts.p_wmw,
            };
            opts.p_wh = wh;
            opts.p_wmh = wmh;
            opts.p_wiw = wiw;
            opts.p_wmw = wmw;
            guard
        }
    }
    impl Drop for MinSizeOptsGuard {
        fn drop(&mut self) {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            opts.p_wh = self.prev_wh;
            opts.p_wmh = self.prev_wmh;
            opts.p_wiw = self.prev_wiw;
            opts.p_wmw = self.prev_wmw;
        }
    }

    fn win_with_extras(handle: crate::types_defs::HandleT) -> WinT {
        WinT {
            handle,
            w_winbar_height: 1,
            w_hsep_height: 2,
            w_status_height: 3,
            w_vsep_width: 4,
            ..Default::default()
        }
    }

    #[test]
    fn frame_minheight_leaf_uses_winheight_for_next_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // extra_height = 1 + 2 + 3 = 6; p_wh(10) + 6 = 16.
        assert_eq!(unsafe { frame_minheight(&leaf, win_ptr) }, 16);
    }

    #[test]
    fn frame_minheight_leaf_uses_winminheight_for_a_non_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win = win_with_extras(1);
        let mut other = win_with_extras(2);
        let win_ptr = &mut win as *mut WinT;
        let other_ptr = &mut other as *mut WinT;
        let _guard = CurwinListGuard::set(other_ptr, std::ptr::null_mut());
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // Not next_curwin, not curwin: p_wmh(2) + extra_height(6) = 8.
        assert_eq!(unsafe { frame_minheight(&leaf, other_ptr) }, 8);
    }

    #[test]
    fn frame_minheight_current_window_gets_a_plus_one_bump_when_winminheight_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 0, 20, 5);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinListGuard::set(win_ptr, std::ptr::null_mut());
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // curwin, next_curwin == NULL, p_wmh == 0: 0 + 6 + 1 = 7.
        assert_eq!(unsafe { frame_minheight(&leaf, std::ptr::null_mut()) }, 7);
    }

    #[test]
    fn frame_minheight_current_window_no_bump_when_winminheight_is_nonzero() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 3, 20, 5);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinListGuard::set(win_ptr, std::ptr::null_mut());
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // p_wmh == 3 (nonzero): no +1 bump. 3 + 6 = 9.
        assert_eq!(unsafe { frame_minheight(&leaf, std::ptr::null_mut()) }, 9);
    }

    #[test]
    fn frame_minheight_nowin_suppresses_the_current_window_bump() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 0, 20, 5);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinListGuard::set(win_ptr, std::ptr::null_mut());
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // NOWIN is non-null, so the "next_curwin.is_null()" bump-gate
        // is false even though this IS curwin and p_wmh == 0.
        assert_eq!(unsafe { frame_minheight(&leaf, NOWIN) }, 6);
    }

    #[test]
    fn frame_minheight_row_takes_the_maximum_of_its_children() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win1 = win_with_extras(1); // minheight (not curwin/next): 2+6=8
        let mut win2 = WinT { handle: 2, w_winbar_height: 0, w_hsep_height: 0, w_status_height: 0, ..Default::default() }; // 2+0=2
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &leaf2 as *const _ as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert_eq!(unsafe { frame_minheight(&row, std::ptr::null_mut()) }, 8);
    }

    #[test]
    fn frame_minheight_col_sums_its_children() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win1 = win_with_extras(1); // 2+6=8
        let mut win2 = WinT { handle: 2, w_winbar_height: 0, w_hsep_height: 0, w_status_height: 0, ..Default::default() }; // 2+0=2
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &leaf2 as *const _ as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert_eq!(unsafe { frame_minheight(&col, std::ptr::null_mut()) }, 10); // 8 + 2
    }

    #[test]
    fn frame_minwidth_leaf_uses_winwidth_for_next_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // p_wiw(20) + w_vsep_width(4) = 24.
        assert_eq!(unsafe { frame_minwidth(&leaf, win_ptr) }, 24);
    }

    #[test]
    fn frame_minwidth_current_window_gets_a_plus_one_bump_when_winminwidth_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 0);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinListGuard::set(win_ptr, std::ptr::null_mut());
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // 0 + 4 + 1 = 5.
        assert_eq!(unsafe { frame_minwidth(&leaf, std::ptr::null_mut()) }, 5);
    }

    #[test]
    fn frame_minwidth_col_is_the_any_case_taking_the_maximum() {
        // FR_COL's role for width is the "max" case, the OPPOSITE of
        // frame_minheight's own FR_ROW-is-"max" - verifying the roles
        // are genuinely swapped, matching frame_fixed_width's own
        // swapped ROW/COL convention.
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win1 = win_with_extras(1); // p_wmw(5) + w_vsep_width(4) = 9
        let mut win2 = WinT { handle: 2, w_vsep_width: 0, ..Default::default() }; // 5 + 0 = 5
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &leaf2 as *const _ as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert_eq!(unsafe { frame_minwidth(&col, std::ptr::null_mut()) }, 9);
    }

    #[test]
    fn frame_minwidth_row_sums_its_children() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win1 = win_with_extras(1); // 9
        let mut win2 = WinT { handle: 2, w_vsep_width: 0, ..Default::default() }; // 5
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &leaf2 as *const _ as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert_eq!(unsafe { frame_minwidth(&row, std::ptr::null_mut()) }, 14); // 9 + 5
    }

    // ---- win_default_scroll / tabline_height / global_stl_height / min_rows(_for_all_tabpages) ----

    #[test]
    fn win_default_scroll_halves_the_view_height() {
        let win = WinT { w_view_height: 20, ..Default::default() };
        assert_eq!(unsafe { win_default_scroll(&win) }, 10);
    }

    #[test]
    fn win_default_scroll_never_returns_less_than_one() {
        let win = WinT { w_view_height: 0, ..Default::default() };
        assert_eq!(unsafe { win_default_scroll(&win) }, 1);
        let win2 = WinT { w_view_height: 1, ..Default::default() };
        assert_eq!(unsafe { win_default_scroll(&win2) }, 1);
    }

    /// RAII guard temporarily setting `OPTION_VARS.p_stal`/`p_ls` and
    /// `GLOBALS.first_tabpage`, restoring the previous values on drop.
    /// Caller must hold `global_state_test_lock()`.
    struct TablineGlobalsGuard {
        prev_stal: crate::types_defs::OptInt,
        prev_ls: crate::types_defs::OptInt,
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
    }
    impl TablineGlobalsGuard {
        fn set(stal: i64, ls: i64, first_tabpage: *mut crate::buffer_defs::TabpageT) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = TablineGlobalsGuard {
                prev_stal: opts.p_stal,
                prev_ls: opts.p_ls,
                prev_first_tabpage: globals.first_tabpage,
            };
            opts.p_stal = stal;
            opts.p_ls = ls;
            globals.first_tabpage = first_tabpage;
            guard
        }
    }
    impl Drop for TablineGlobalsGuard {
        fn drop(&mut self) {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            opts.p_stal = self.prev_stal;
            opts.p_ls = self.prev_ls;
            globals.first_tabpage = self.prev_first_tabpage;
        }
    }

    #[test]
    fn tabline_height_zero_when_showtabline_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = TablineGlobalsGuard::set(0, 0, tp_ptr);
        assert_eq!(unsafe { tabline_height() }, 0);
    }

    #[test]
    fn tabline_height_one_only_shown_with_multiple_tabs() {
        let _lock = crate::globals::global_state_test_lock();
        let mut only_tab = crate::buffer_defs::TabpageT::default();
        let only_tab_ptr = &mut only_tab as *mut crate::buffer_defs::TabpageT;
        {
            let _guard = TablineGlobalsGuard::set(1, 0, only_tab_ptr);
            assert_eq!(unsafe { tabline_height() }, 0);
        }

        let mut second_tab = crate::buffer_defs::TabpageT::default();
        let second_tab_ptr = &mut second_tab as *mut crate::buffer_defs::TabpageT;
        let mut first_tab =
            crate::buffer_defs::TabpageT { tp_next: second_tab_ptr, ..Default::default() };
        let first_tab_ptr = &mut first_tab as *mut crate::buffer_defs::TabpageT;
        let _guard = TablineGlobalsGuard::set(1, 0, first_tab_ptr);
        assert_eq!(unsafe { tabline_height() }, 1);
    }

    #[test]
    fn tabline_height_always_one_when_showtabline_is_two() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = TablineGlobalsGuard::set(2, 0, tp_ptr);
        assert_eq!(unsafe { tabline_height() }, 1);
    }

    #[test]
    fn global_stl_height_one_only_when_laststatus_is_three() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = TablineGlobalsGuard::set(0, 3, tp_ptr);
        assert_eq!(unsafe { global_stl_height() }, STATUS_HEIGHT);

        let _guard2 = TablineGlobalsGuard::set(0, 2, tp_ptr);
        assert_eq!(unsafe { global_stl_height() }, 0);
    }

    #[test]
    fn min_rows_not_initialized_yet_returns_min_lines() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        globals.firstwin = std::ptr::null_mut();
        let tp = crate::buffer_defs::TabpageT::default();
        assert_eq!(unsafe { min_rows(&tp) }, MIN_LINES);
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    #[test]
    fn min_rows_for_all_tabpages_not_initialized_yet_returns_min_lines() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        globals.firstwin = std::ptr::null_mut();
        assert_eq!(unsafe { min_rows_for_all_tabpages() }, MIN_LINES);
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    #[test]
    fn min_rows_combines_frame_minheight_tabline_and_statusline() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win = win_with_extras(1); // minheight (not curwin/next): p_wmh(2)+extra(6)=8
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT {
            tp_topframe: &leaf as *const _ as *mut crate::buffer_defs::FrameT,
            tp_ch_used: 0, // not curtab's own p_ch, and 0 here means no +1 bump
            ..Default::default()
        };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        let prev_curtab = globals.curtab;
        let prev_first_tabpage = globals.first_tabpage;
        globals.firstwin = win_ptr; // just needs to be non-null
        globals.curtab = std::ptr::null_mut(); // tp itself is NOT curtab
        // tabline_height (called internally by min_rows) asserts
        // first_tabpage is non-null - it need not be tp itself.
        globals.first_tabpage = tp_ptr;
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_stal = 0; // tabline_height == 0
        opts.p_ls = 0; // global_stl_height == 0

        // 8 (frame_minheight) + 0 (tabline) + 0 (statusline) + 0 (no
        // command-line room, since tp_ch_used == 0) = 8.
        assert_eq!(unsafe { min_rows(tp_ptr) }, 8);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.firstwin = prev_firstwin;
        globals.curtab = prev_curtab;
        globals.first_tabpage = prev_first_tabpage;
    }

    #[test]
    fn min_rows_for_all_tabpages_takes_the_maximum_across_tabs() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win1 = win_with_extras(1); // 2+6=8
        let mut win2 = WinT { handle: 2, w_winbar_height: 0, w_hsep_height: 0, w_status_height: 0, ..Default::default() }; // 2+0=2
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let leaf1 = crate::buffer_defs::FrameT { fr_win: win1_ptr, ..Default::default() };
        let leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let mut tab2 = crate::buffer_defs::TabpageT {
            tp_topframe: &leaf2 as *const _ as *mut crate::buffer_defs::FrameT,
            tp_ch_used: 0,
            ..Default::default()
        };
        let tab2_ptr = &mut tab2 as *mut crate::buffer_defs::TabpageT;
        let mut tab1 = crate::buffer_defs::TabpageT {
            tp_topframe: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            tp_next: tab2_ptr,
            tp_ch_used: 0,
            ..Default::default()
        };
        let tab1_ptr = &mut tab1 as *mut crate::buffer_defs::TabpageT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        let prev_curtab = globals.curtab;
        let prev_first_tabpage = globals.first_tabpage;
        globals.firstwin = win1_ptr;
        globals.curtab = std::ptr::null_mut();
        globals.first_tabpage = tab1_ptr;
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_stal = 0;
        opts.p_ls = 0;

        // max(8, 2) + 0 + 0 = 8.
        assert_eq!(unsafe { min_rows_for_all_tabpages() }, 8);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.firstwin = prev_firstwin;
        globals.curtab = prev_curtab;
        globals.first_tabpage = prev_first_tabpage;
    }

    // ---- frame_add_statusline ----

    #[test]
    fn frame_add_statusline_leaf_sets_the_window_status_height() {
        let mut win = WinT { w_status_height: 0, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let leaf_ptr = &mut leaf as *mut crate::buffer_defs::FrameT;
        unsafe { frame_add_statusline(leaf_ptr) };
        assert_eq!(unsafe { &*win_ptr }.w_status_height, STATUS_HEIGHT);
    }

    #[test]
    fn frame_add_statusline_row_sets_every_child() {
        let mut win1 = WinT { handle: 1, w_status_height: 0, ..Default::default() };
        let mut win2 = WinT { handle: 2, w_status_height: 0, ..Default::default() };
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mut leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let mut row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &mut leaf1 as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        let row_ptr = &mut row as *mut crate::buffer_defs::FrameT;
        unsafe { frame_add_statusline(row_ptr) };
        assert_eq!(unsafe { &*win1_ptr }.w_status_height, STATUS_HEIGHT);
        assert_eq!(unsafe { &*win2_ptr }.w_status_height, STATUS_HEIGHT);
    }

    #[test]
    fn frame_add_statusline_col_only_sets_the_last_child() {
        let mut win1 = WinT { handle: 1, w_status_height: 0, ..Default::default() };
        let mut win2 = WinT { handle: 2, w_status_height: 0, ..Default::default() };
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mut leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let mut col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &mut leaf1 as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        let col_ptr = &mut col as *mut crate::buffer_defs::FrameT;
        unsafe { frame_add_statusline(col_ptr) };
        // Only the LAST frame in the column gets a status line - the
        // first is left untouched (matching the original's own
        // "Only need to handle the last frame in the column" comment).
        assert_eq!(unsafe { &*win1_ptr }.w_status_height, 0);
        assert_eq!(unsafe { &*win2_ptr }.w_status_height, STATUS_HEIGHT);
    }

    // ---- find_tabwin ----

    fn unknown_tv() -> TypvalT {
        TypvalT::default()
    }

    fn num_tv(n: crate::eval::typval_defs::VarnumberT) -> TypvalT {
        TypvalT { value: TypvalValue::Number(n), ..Default::default() }
    }

    #[test]
    fn find_tabwin_no_args_returns_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win_ptr, tp_ptr, win_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert_eq!(unsafe { find_tabwin(&unknown_tv(), &unknown_tv()) }, win_ptr);
    }

    #[test]
    fn find_tabwin_window_number_only_uses_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let second_ptr = &mut second as *mut WinT;
        let mut first = WinT { w_next: second_ptr, ..focusable_win(1) };
        let first_ptr = &mut first as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(first_ptr, tp_ptr, first_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert_eq!(unsafe { find_tabwin(&num_tv(2), &unknown_tv()) }, second_ptr);
    }

    #[test]
    fn find_tabwin_with_a_negative_tabnr_returns_null() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win_ptr, tp_ptr, win_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert!(unsafe { find_tabwin(&num_tv(1), &num_tv(-1)) }.is_null());
    }

    #[test]
    fn find_tabwin_tab_zero_means_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win_ptr, tp_ptr, win_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        // Tab 0 -> find_tabpage(0) -> curtab, matching find_tabpage's
        // own already-established "0 means curtab" convention.
        assert_eq!(unsafe { find_tabwin(&num_tv(1), &num_tv(0)) }, win_ptr);
    }
}
