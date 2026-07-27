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
//! `win_horz_neighbor` (real window-layout geometry, not yet
//! translated) and panics via `unimplemented!()` if actually reached -
//! the common no-argument, `"$"`, and `"#"` cases are fully modeled.
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
/// Only `arg == None`, `b"$"` (last window), and `b"#"` (previous
/// window) are modeled - the digit+direction form (e.g. `b"3j"`)
/// needs `win_vert_neighbor`/`win_horz_neighbor` (real window-layout
/// geometry, not yet translated) and panics via `unimplemented!()` if
/// actually reached; any other unrecognized `arg` returns `0`
/// (matching the original's own `invalid_arg` path, whose real
/// `semsg` display is omitted - message display, not tractable).
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
            let _count = if count <= 0 { 1 } else { count };
            let dir = &arg[consumed..];
            if dir == b"j" || dir == b"k" || dir == b"h" || dir == b"l" {
                unimplemented!(
                    "get_winnr: digit+direction form (e.g. \"3j\") needs \
                     win_vert_neighbor/win_horz_neighbor, not yet translated"
                );
            }
            nr = 0;
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
    fn get_winnr_digit_direction_form_is_unimplemented() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win_ptr, tp_ptr, win_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        let result = std::panic::catch_unwind(|| unsafe { get_winnr(tp_ptr, Some(b"3j")) });
        assert!(result.is_err(), "expected a panic (win_vert_neighbor not yet translated)");
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
