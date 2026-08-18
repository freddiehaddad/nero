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

use crate::api::private::defs::{
    Array, Boolean, Buffer, Dict, Error, ErrorType, Integer, KeyValuePair, NvimString, Object,
    Tabpage, Window,
};
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

/// Get the window's zero-based `(row, col)` screen position
/// (`nvim_win_get_position`).
///
/// # Safety
/// Forwarded from [`find_window_by_handle`].
#[must_use]
pub unsafe fn nvim_win_get_position(win: Window, err: &mut Error) -> Array {
    let window = unsafe { find_window_by_handle(win, err) };
    if window.is_null() {
        return Vec::new();
    }
    vec![
        Object::Integer(i64::from(unsafe { (*window).w_winrow })),
        Object::Integer(i64::from(unsafe { (*window).w_wincol })),
    ]
}

/// Get the tabpage containing window `win`
/// (`nvim_win_get_tabpage`).
///
/// # Safety
/// Forwarded from [`find_window_by_handle`] and
/// [`crate::window::win_find_tabpage`].
#[must_use]
pub unsafe fn nvim_win_get_tabpage(win: Window, err: &mut Error) -> Tabpage {
    let window = unsafe { find_window_by_handle(win, err) };
    if window.is_null() {
        return 0;
    }

    let tab = unsafe { crate::window::win_find_tabpage(window) };
    if tab.is_null() {
        0
    } else {
        unsafe { (*tab).handle }
    }
}

/// Get a window-scoped variable (`nvim_win_get_var`).
///
/// # Safety
/// Forwarded from [`find_window_by_handle`] and the scope-dictionary
/// converter.
pub unsafe fn nvim_win_get_var(win: Window, name: &NvimString, err: &mut Error) -> Object {
    let win = unsafe { find_window_by_handle(win, err) };
    if win.is_null() {
        return Object::Nil;
    }
    unsafe { crate::api::private::helpers::dict_get_value((*win).w_vars, name, err) }
}

/// Set a window-scoped variable (`nvim_win_set_var`).
///
/// # Safety
/// Forwarded from [`find_window_by_handle`] and the checked
/// scope-dictionary writer.
pub unsafe fn nvim_win_set_var(
    win: Window,
    name: &NvimString,
    value: &Object,
    err: &mut Error,
) {
    let win = unsafe { find_window_by_handle(win, err) };
    if win.is_null() {
        return;
    }
    let _ = unsafe {
        crate::api::private::helpers::dict_set_var(
            (*win).w_vars,
            name,
            value,
            false,
            false,
            err,
        )
    };
}

/// Optional range controls for [`nvim_win_text_height`].
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct WinTextHeightOpts {
        pub start_row: Option<Integer>,
        pub start_vcol: Option<Integer>,
        pub end_row: Option<Integer>,
        pub end_vcol: Option<Integer>,
        pub max_height: Option<Integer>,
    }

    fn text_height_pair(key: &[u8], value: Integer) -> KeyValuePair {
        KeyValuePair {
            key: key.to_vec(),
            value: Object::Integer(value),
        }
    }

    /// Get screen-line height information for a window range
    /// (`nvim_win_text_height`).
    ///
    /// # Safety
    /// Forwarded from [`find_window_by_handle`] and
    /// [`crate::plines::win_text_height`].
    #[must_use]
    pub unsafe fn nvim_win_text_height(
        win: Window,
        opts: WinTextHeightOpts,
        err: &mut Error,
    ) -> Dict {
        let window = unsafe { find_window_by_handle(win, err) };
        if window.is_null() {
            return Vec::new();
        }
        let buf = unsafe { (*window).w_buffer };
        let line_count = unsafe { (*buf).b_ml.ml_line_count };
        let mut start_lnum = 1;
        let mut end_lnum = line_count;
        let mut start_vcol = -1;
        let mut end_vcol = -1;
        let mut oob = false;

        if let Some(start_row) = opts.start_row {
            start_lnum = crate::api::private::helpers::normalize_index(
                unsafe { &*buf },
                start_row,
                false,
                &mut oob,
            ) as crate::pos_defs::LinenrT;
        }
        if let Some(end_row) = opts.end_row {
            end_lnum = crate::api::private::helpers::normalize_index(
                unsafe { &*buf },
                end_row,
                false,
                &mut oob,
            ) as crate::pos_defs::LinenrT;
        }
        if oob {
            err.r#type = ErrorType::Validation;
            err.msg = Some("Line index out of bounds".to_string());
            return Vec::new();
        }
        if start_lnum > end_lnum {
            err.r#type = ErrorType::Validation;
            err.msg = Some("'start_row' is higher than 'end_row'".to_string());
            return Vec::new();
        }

        if let Some(value) = opts.start_vcol {
            if opts.start_row.is_none() {
                err.r#type = ErrorType::Validation;
                err.msg = Some("'start_vcol' specified without 'start_row'".to_string());
                return Vec::new();
            }
            if !(0..=i64::from(crate::pos_defs::MAXCOL)).contains(&value) {
                err.r#type = ErrorType::Validation;
                err.msg = Some("Invalid 'start_vcol': out of range".to_string());
                return Vec::new();
            }
            start_vcol = value;
        }
        if let Some(value) = opts.end_vcol {
            if opts.end_row.is_none() {
                err.r#type = ErrorType::Validation;
                err.msg = Some("'end_vcol' specified without 'end_row'".to_string());
                return Vec::new();
            }
            if !(0..=i64::from(crate::pos_defs::MAXCOL)).contains(&value) {
                err.r#type = ErrorType::Validation;
                err.msg = Some("Invalid 'end_vcol': out of range".to_string());
                return Vec::new();
            }
            end_vcol = value;
        }
        let max = match opts.max_height {
            Some(value) if value <= 0 => {
                err.r#type = ErrorType::Validation;
                err.msg = Some("Invalid 'max_height': out of range".to_string());
                return Vec::new();
            }
            Some(value) => value,
            None => i64::MAX,
        };
        if start_lnum == end_lnum
            && start_vcol >= 0
            && end_vcol >= 0
            && start_vcol > end_vcol
        {
            err.r#type = ErrorType::Validation;
            err.msg = Some("'start_vcol' is higher than 'end_vcol'".to_string());
            return Vec::new();
        }

        let mut fill = 0;
        let mut all = unsafe {
            crate::plines::win_text_height(
                window,
                start_lnum,
                start_vcol,
                &mut end_lnum,
                &mut end_vcol,
                Some(&mut fill),
                max,
            )
        };
        if opts.end_row.is_none() {
            let end_fill =
                i64::from(unsafe { crate::plines::win_get_fill(&*window, line_count + 1) });
            fill += end_fill;
            all += end_fill;
        }
        vec![
            text_height_pair(b"all", all),
            text_height_pair(b"fill", fill),
            text_height_pair(b"end_row", i64::from(end_lnum - 1)),
            text_height_pair(b"end_vcol", end_vcol),
        ]
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
    fn nvim_win_get_position_returns_zero_based_screen_coordinates() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = RawWinFixture::new(15);
        unsafe {
            (*fx.win).w_winrow = 4;
            (*fx.win).w_wincol = 12;
        }
        let mut err = Error::default();

        let position =
            unsafe { nvim_win_get_position((*fx.win).handle, &mut err) };

        assert!(!err.is_set());
        assert!(matches!(position.as_slice(), [Object::Integer(4), Object::Integer(12)]));
    }

    #[test]
    fn nvim_win_get_position_returns_empty_for_an_unknown_window() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = WinFixture::new(16);
        let mut err = Error::default();
        assert!(unsafe { nvim_win_get_position(99, &mut err) }.is_empty());
        assert_eq!(err.msg.as_deref(), Some("Invalid window id: 99"));
    }

    #[test]
    fn nvim_win_get_tabpage_returns_the_containing_tab_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = RawWinFixture::new(17);
        unsafe { (*fx.tab).handle = 70 };
        let mut err = Error::default();

        let tabpage =
            unsafe { nvim_win_get_tabpage((*fx.win).handle, &mut err) };

        assert_eq!(tabpage, 70);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_win_get_tabpage_returns_zero_for_an_unknown_window() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = WinFixture::new(18);
        let mut err = Error::default();
        assert_eq!(unsafe { nvim_win_get_tabpage(99, &mut err) }, 0);
        assert_eq!(err.msg.as_deref(), Some("Invalid window id: 99"));
    }

    #[test]
    fn nvim_win_get_var_returns_a_real_window_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = RawWinFixture::new(21);
        let dict = crate::eval::typval::tv_dict_alloc();
        assert_eq!(
            unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, b"answer", 42) },
            crate::vim_defs::OK
        );
        unsafe { (*fx.win).w_vars = dict };
        let mut err = Error::default();
        let value =
            unsafe { nvim_win_get_var((*fx.win).handle, &b"answer".to_vec(), &mut err) };
        assert!(matches!(value, Object::Integer(42)));
        assert!(!err.is_set());
        unsafe {
            (*fx.win).w_vars = std::ptr::null_mut();
            crate::eval::typval::tv_dict_unref(dict);
        }
    }

    #[test]
    fn nvim_win_set_var_stores_a_window_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = RawWinFixture::new(22);
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe { (*fx.win).w_vars = dict };
        let mut err = Error::default();
        unsafe {
            nvim_win_set_var(
                (*fx.win).handle,
                &b"value".to_vec(),
                &Object::String(b"text".to_vec()),
                &mut err,
            )
        };
        let value =
            unsafe { nvim_win_get_var((*fx.win).handle, &b"value".to_vec(), &mut err) };
        assert!(matches!(value, Object::String(ref text) if text == b"text"));
        assert!(!err.is_set());
        let item = unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), b"value") }
            .expect("window variable");
        unsafe {
            crate::eval::typval::tv_dict_item_remove(&mut *dict, item);
            (*fx.win).w_vars = std::ptr::null_mut();
            crate::eval::typval::tv_dict_unref(dict);
        }
    }

    fn integer_in_dict(dict: &Dict, key: &[u8]) -> Integer {
        dict.iter()
            .find(|pair| pair.key == key)
            .and_then(|pair| match pair.value {
                Object::Integer(value) => Some(value),
                _ => None,
            })
            .expect("integer key exists")
    }

    #[test]
    fn nvim_win_text_height_returns_real_screen_line_counts() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = RawWinFixture::new(19);
        unsafe {
            assert_eq!(crate::memline::ml_open(&mut *fx.buf), crate::vim_defs::OK);
            assert_eq!(
                crate::memline::ml_replace_buf_len(&mut *fx.buf, 1, b"hello\0"),
                crate::vim_defs::OK
            );
            assert_eq!(
                crate::memline::ml_append_buf(&mut *fx.buf, 1, b"hello\0", 6, false),
                crate::vim_defs::OK
            );
            assert_eq!(
                crate::memline::ml_append_buf(&mut *fx.buf, 2, b"hello\0", 6, false),
                crate::vim_defs::OK
            );
            (*fx.win).w_view_width = 10;
            (*fx.win).w_onebuf_opt.wo_wrap = 1;
        }
        let mut err = Error::default();

        let result = unsafe {
            nvim_win_text_height(
                (*fx.win).handle,
                WinTextHeightOpts {
                    end_row: Some(2),
                    ..Default::default()
                },
                &mut err,
            )
        };

        assert!(!err.is_set());
        assert_eq!(integer_in_dict(&result, b"all"), 3);
        assert_eq!(integer_in_dict(&result, b"fill"), 0);
        assert_eq!(integer_in_dict(&result, b"end_row"), 2);
        assert_eq!(integer_in_dict(&result, b"end_vcol"), 5);
        unsafe {
            let mfp = (*fx.buf).b_ml.ml_mfp;
            (*fx.buf).b_ml.ml_mfp = std::ptr::null_mut();
            crate::memfile::mf_close(*Box::from_raw(mfp), false);
        }
    }

    #[test]
    fn nvim_win_text_height_rejects_start_vcol_without_start_row() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = RawWinFixture::new(20);
        unsafe { (*fx.buf).b_ml.ml_line_count = 1 };
        let mut err = Error::default();
        assert!(
            unsafe {
                nvim_win_text_height(
                    (*fx.win).handle,
                    WinTextHeightOpts {
                        start_vcol: Some(0),
                        ..Default::default()
                    },
                    &mut err,
                )
            }
            .is_empty()
        );
        assert_eq!(
            err.msg.as_deref(),
            Some("'start_vcol' specified without 'start_row'")
        );
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
