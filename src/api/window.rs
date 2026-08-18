//! Translated from `src/nvim/api/window.c` (tractable subset only -
//! most of this file needs `Arena`/`Object`-conversion machinery,
//! `Dict`-scoped-variable access, `apply_autocmds`-driven buffer
//! switching, and the redraw pipeline, none of which are wired into
//! this API layer yet).
//!
//! Translated: [`nvim_win_get_buf`], [`nvim_win_get_height`],
//! [`nvim_win_get_width`], [`nvim_win_get_number`],
//! [`nvim_win_is_valid`], [`nvim_win_set_hl_ns`] - thin, real
//! `find_window_by_handle` plus direct field access, with no other
//! subsystem dependency.
//!
//! Deferred (each needs a real, not-yet-translated subsystem beyond
//! `find_window_by_handle` itself): `nvim_win_set_buf` (needs
//! `apply_autocmds`'s real buffer-switch semantics plus
//! `win_set_buf`), `nvim_win_get_cursor`/`nvim_win_set_cursor` (need
//! `Array`/`ArrayOf` conversion), `nvim_win_set_height`/
//! `nvim_win_set_width` (need `win_setheight`/`win_setwidth`, the
//! real frame-resizing algorithms), `nvim_win_get_var`/
//! `nvim_win_set_var`/`nvim_win_del_var` (need `dict_get_value`/
//! `dict_set_var`, the API layer's `Object`-to-`typval_T` bridge),
//! `nvim_win_get_position` (needs `Array`), `nvim_win_get_tabpage`
//! (needs `win_find_tabpage`), `nvim_win_close`/`nvim_win_hide`
//! (real window-closing machinery).

use crate::api::private::defs::{Array, Boolean, Buffer, Error, Integer, Object, Window};
use crate::api::private::helpers::find_window_by_handle;

/// Get the buffer handle shown in window `win` (`0` for the current
/// window), or `0` on failure (`nvim_win_get_buf`).
///
/// # Safety
/// Forwarded from [`find_window_by_handle`]'s own safety doc.
pub unsafe fn nvim_win_get_buf(win: Window, err: &mut Error) -> Buffer {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { find_window_by_handle(win, err) };
    if w.is_null() {
        return 0;
    }
    // SAFETY: `w` is non-null per the check above, and every real
    // `WinT` always has a live `w_buffer`.
    unsafe { (*(*w).w_buffer).handle }
}

/// Get the `(1, 0)`-indexed buffer-relative cursor position
/// (`nvim_win_get_cursor`).
///
/// # Safety
/// Forwarded from [`find_window_by_handle`].
#[must_use]
pub unsafe fn nvim_win_get_cursor(win: Window, err: &mut Error) -> Array {
    let window = unsafe { find_window_by_handle(win, err) };
    if window.is_null() {
        return Vec::new();
    }
    vec![
        Object::Integer(i64::from(unsafe { (*window).w_cursor.lnum })),
        Object::Integer(i64::from(unsafe { (*window).w_cursor.col })),
    ]
}

/// Get the height (row count) of window `win` (`0` for the current
/// window), or `0` on failure (`nvim_win_get_height`).
///
/// # Safety
/// Forwarded from [`find_window_by_handle`]'s own safety doc.
pub unsafe fn nvim_win_get_height(win: Window, err: &mut Error) -> Integer {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { find_window_by_handle(win, err) };
    if w.is_null() {
        return 0;
    }
    // SAFETY: `w` is non-null per the check above.
    unsafe { i64::from((*w).w_height) }
}

/// Get the width (column count) of window `win` (`0` for the current
/// window), or `0` on failure (`nvim_win_get_width`).
///
/// # Safety
/// Forwarded from [`find_window_by_handle`]'s own safety doc.
pub unsafe fn nvim_win_get_width(win: Window, err: &mut Error) -> Integer {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { find_window_by_handle(win, err) };
    if w.is_null() {
        return 0;
    }
    // SAFETY: `w` is non-null per the check above.
    unsafe { i64::from((*w).w_width) }
}

/// Get the 1-based, current-tabpage-relative window number of window
/// `win` (`0` for the current window), or `0` on failure/if `win` is
/// not counted per [`crate::window::win_has_winnr`]
/// (`nvim_win_get_number`).
///
/// # Safety
/// Forwarded from [`find_window_by_handle`]'s own safety doc.
pub unsafe fn nvim_win_get_number(win: Window, err: &mut Error) -> Integer {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { find_window_by_handle(win, err) };
    if w.is_null() {
        return 0;
    }
    // SAFETY: `w` is non-null per the check above.
    let handle = unsafe { (*w).handle };
    // SAFETY: forwarded from this function's own safety doc.
    let (_tabnr, winnr) = unsafe { crate::window::win_get_tabwin(handle) };
    i64::from(winnr)
}

/// Whether `win` (`0` for the current window) refers to a currently
/// valid window (`nvim_win_is_valid`). Unlike the other functions in
/// this file, a failed lookup is not itself an error - it simply
/// means `win` is invalid, matching the original's own
/// `stub`-`Error`-then-discard pattern.
///
/// # Safety
/// Forwarded from [`find_window_by_handle`]'s own safety doc.
#[must_use]
pub unsafe fn nvim_win_is_valid(win: Window) -> Boolean {
    let mut stub = Error::default();
    // SAFETY: forwarded from this function's own safety doc.
    !unsafe { find_window_by_handle(win, &mut stub) }.is_null()
}

/// Set the highlight namespace for window `win`
/// (`nvim_win_set_hl_ns`).
///
/// Namespace `-1` inherits the global namespace.
///
/// # Safety
/// Forwarded from [`find_window_by_handle`] and
/// [`crate::drawscreen::redraw_later`].
pub unsafe fn nvim_win_set_hl_ns(win: Window, ns_id: Integer, err: &mut Error) {
    let window = unsafe { find_window_by_handle(win, err) };
    if window.is_null() {
        return;
    }
    if ns_id < -1 {
        err.r#type = crate::api::private::defs::ErrorType::Validation;
        err.msg = Some("Invalid 'namespace'".to_string());
        return;
    }
    unsafe {
        (*window).w_ns_hl = ns_id as i32;
        (*window).w_hl_needs_update = 1;
        crate::drawscreen::redraw_later(window, crate::drawscreen::UPD_NOT_VALID);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{BufT, TabpageT, WinT};

    struct WinFixture {
        _buf: Box<BufT>,
        win: Box<WinT>,
        _tab: Box<TabpageT>,
        prev_firstwin: *mut WinT,
        prev_curwin: *mut WinT,
        prev_curtab: *mut TabpageT,
        prev_first_tabpage: *mut TabpageT,
    }

    impl WinFixture {
        fn new(handle: crate::types_defs::HandleT) -> Self {
            let mut buf = Box::new(BufT::default());
            buf.handle = handle * 100;
            let buf_ptr = std::ptr::addr_of_mut!(*buf);

            let mut win = Box::new(WinT::default());
            win.handle = handle;
            win.w_buffer = buf_ptr;
            win.w_height = 24;
            win.w_width = 80;
            win.w_config.focusable = true;
            let win_ptr = std::ptr::addr_of_mut!(*win);

            let mut tab = Box::new(TabpageT::default());
            tab.tp_firstwin = win_ptr;
            let tab_ptr = std::ptr::addr_of_mut!(*tab);

            // SAFETY: single-threaded test, GLOBALS restored in Drop.
            unsafe {
                let g = crate::globals::GLOBALS.get_mut();
                let prev_firstwin = g.firstwin;
                let prev_curwin = g.curwin;
                let prev_curtab = g.curtab;
                let prev_first_tabpage = g.first_tabpage;
                g.firstwin = win_ptr;
                g.curwin = win_ptr;
                g.curtab = tab_ptr;
                g.first_tabpage = tab_ptr;
                WinFixture {
                    _buf: buf,
                    win,
                    _tab: tab,
                    prev_firstwin,
                    prev_curwin,
                    prev_curtab,
                    prev_first_tabpage,
                }
            }
        }
    }

    impl Drop for WinFixture {
        fn drop(&mut self) {
            // SAFETY: restoring exactly what `new` overwrote.
            unsafe {
                let g = crate::globals::GLOBALS.get_mut();
                g.firstwin = self.prev_firstwin;
                g.curwin = self.prev_curwin;
                g.curtab = self.prev_curtab;
                g.first_tabpage = self.prev_first_tabpage;
            }
        }
    }

    struct RawWinFixture {
        buf: *mut BufT,
        win: *mut WinT,
        tab: *mut TabpageT,
        prev_firstwin: *mut WinT,
        prev_curwin: *mut WinT,
        prev_curtab: *mut TabpageT,
        prev_first_tabpage: *mut TabpageT,
    }

    impl RawWinFixture {
        fn new(handle: crate::types_defs::HandleT) -> Self {
            let mut buf = Box::new(BufT::default());
            buf.handle = handle * 100;
            let buf = Box::into_raw(buf);

            let mut win = Box::new(WinT::default());
            win.handle = handle;
            win.w_buffer = buf;
            win.w_config.focusable = true;
            let win = Box::into_raw(win);

            let mut tab = Box::new(TabpageT::default());
            tab.tp_firstwin = win;
            let tab = Box::into_raw(tab);

            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let fixture = Self {
                buf,
                win,
                tab,
                prev_firstwin: globals.firstwin,
                prev_curwin: globals.curwin,
                prev_curtab: globals.curtab,
                prev_first_tabpage: globals.first_tabpage,
            };
            globals.firstwin = win;
            globals.curwin = win;
            globals.curtab = tab;
            globals.first_tabpage = tab;
            fixture
        }
    }

    impl Drop for RawWinFixture {
        fn drop(&mut self) {
            unsafe {
                let globals = crate::globals::GLOBALS.get_mut();
                globals.firstwin = self.prev_firstwin;
                globals.curwin = self.prev_curwin;
                globals.curtab = self.prev_curtab;
                globals.first_tabpage = self.prev_first_tabpage;
                drop(Box::from_raw(self.tab));
                drop(Box::from_raw(self.win));
                drop(Box::from_raw(self.buf));
            }
        }
    }

    #[test]
    fn nvim_win_get_buf_returns_the_real_buffer_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = WinFixture::new(5);
        let handle = fx.win.handle;
        let mut err = Error::default();
        // SAFETY: `fx` sets up a valid GLOBALS.firstwin/curwin.
        let buf = unsafe { nvim_win_get_buf(handle, &mut err) };
        assert_eq!(buf, 500);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_win_get_buf_zero_uses_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = WinFixture::new(6);
        let mut err = Error::default();
        // SAFETY: `_fx` sets up a valid GLOBALS.curwin.
        let buf = unsafe { nvim_win_get_buf(0, &mut err) };
        assert_eq!(buf, 600);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_win_get_cursor_returns_one_based_line_and_zero_based_column() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = RawWinFixture::new(13);
        let win = fx.win;
        unsafe {
            (*win).w_cursor = crate::pos_defs::PosT {
                lnum: 7,
                col: 11,
                coladd: 3,
            };
        }
        let handle = unsafe { (*win).handle };
        let mut err = Error::default();

        let position = unsafe { nvim_win_get_cursor(handle, &mut err) };

        assert!(!err.is_set());
        assert!(matches!(position.as_slice(), [Object::Integer(7), Object::Integer(11)]));
    }

    #[test]
    fn nvim_win_get_cursor_returns_empty_for_an_unknown_window() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = WinFixture::new(14);
        let mut err = Error::default();
        assert!(unsafe { nvim_win_get_cursor(99, &mut err) }.is_empty());
        assert_eq!(err.msg.as_deref(), Some("Invalid window id: 99"));
    }

    #[test]
    fn nvim_win_get_height_and_width_read_real_fields() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = WinFixture::new(7);
        let handle = fx.win.handle;
        let mut err = Error::default();
        // SAFETY: `fx` sets up a valid GLOBALS.firstwin/curwin.
        let height = unsafe { nvim_win_get_height(handle, &mut err) };
        // SAFETY: same as above.
        let width = unsafe { nvim_win_get_width(handle, &mut err) };
        assert_eq!(height, 24);
        assert_eq!(width, 80);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_win_get_number_reports_1_for_the_only_window() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = WinFixture::new(8);
        let handle = fx.win.handle;
        let mut err = Error::default();
        // SAFETY: `fx` sets up a valid single-window tabpage.
        let number = unsafe { nvim_win_get_number(handle, &mut err) };
        assert_eq!(number, 1);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_win_is_valid_true_for_a_real_window_false_otherwise() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = WinFixture::new(9);
        let handle = fx.win.handle;
        // SAFETY: `fx` sets up a valid GLOBALS.firstwin/curwin.
        assert!(unsafe { nvim_win_is_valid(handle) });
        // SAFETY: same as above.
        assert!(!unsafe { nvim_win_is_valid(handle + 1) });
    }

    #[test]
    fn unknown_handle_returns_a_structured_error_and_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = WinFixture::new(10);
        let mut err = Error::default();
        // SAFETY: `_fx` sets up a valid GLOBALS.firstwin/curwin, but
        // `9999` is a genuinely unrecognized handle.
        let buf = unsafe { nvim_win_get_buf(9999, &mut err) };
        assert_eq!(buf, 0);
        assert!(err.is_set());
    }

    #[test]
    fn nvim_win_set_hl_ns_sets_namespace_marks_update_and_redraw() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = RawWinFixture::new(11);
        let handle = unsafe { (*fx.win).handle };
        let win_ptr = fx.win;
        let mut err = Error::default();

        unsafe { nvim_win_set_hl_ns(handle, 7, &mut err) };

        assert!(!err.is_set());
        assert_eq!(unsafe { (*win_ptr).w_ns_hl }, 7);
        assert_eq!(unsafe { (*win_ptr).w_hl_needs_update }, 1);
        assert_eq!(
            unsafe { (*win_ptr).w_redr_type },
            crate::drawscreen::UPD_NOT_VALID
        );

        unsafe { nvim_win_set_hl_ns(handle, -1, &mut err) };
        assert_eq!(unsafe { (*win_ptr).w_ns_hl }, -1);
    }

    #[test]
    fn nvim_win_set_hl_ns_rejects_too_negative_or_unknown_window() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = RawWinFixture::new(12);
        let handle = unsafe { (*fx.win).handle };
        let win_ptr = fx.win;
        let mut err = Error::default();

        unsafe { nvim_win_set_hl_ns(handle, -2, &mut err) };
        assert_eq!(
            err.r#type,
            crate::api::private::defs::ErrorType::Validation
        );
        assert_eq!(err.msg.as_deref(), Some("Invalid 'namespace'"));
        assert_eq!(unsafe { (*win_ptr).w_ns_hl }, 0);

        err = Error::default();
        unsafe { nvim_win_set_hl_ns(99, 1, &mut err) };
        assert_eq!(err.msg.as_deref(), Some("Invalid window id: 99"));
    }
}
