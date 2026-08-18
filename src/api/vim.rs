//! Translated from `src/nvim/api/vim.c` (tractable subset only - most
//! of this huge file needs `Array`/`Dict`/`Object`-conversion
//! machinery, the msgpack-rpc dispatch layer, command execution, and
//! the Lua host, none of which are wired into this API layer yet).
//!
//! Translated: [`nvim_list_bufs`], [`nvim_list_wins`], [`nvim_list_tabpages`],
//! [`nvim_get_current_win`]/
//! [`nvim_get_current_buf`]/[`nvim_get_current_tabpage`] (harvested ahead of the rest of this
//! file, matching the established "one tractable function ahead of a
//! huge file" precedent used elsewhere in this crate, e.g.
//! `ex_docmd.rs`; `nvim_get_current_win` is also `api/tabpage.c`'s own
//! real dependency - `nvim_tabpage_get_win` calls it directly when the
//! tabpage in question is the current one), and [`nvim_strwidth`] (via
//! the already-existing `mbyte.rs::mb_string2cells`).

use crate::api::private::defs::{
    Array, Buffer, Error, ErrorType, Integer, NvimString, Object, Tabpage, Window,
};

/// List every current buffer, including unlisted and unloaded buffers
/// (`nvim_list_bufs`).
///
/// # Safety
/// `GLOBALS.firstbuf` and each `b_next` link must form a live buffer
/// list.
#[must_use]
pub unsafe fn nvim_list_bufs() -> Array {
    let mut result = Vec::new();
    let mut buf = unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf;
    while !buf.is_null() {
        result.push(Object::Buffer(unsafe { (*buf).handle }));
        buf = unsafe { (*buf).b_next };
    }
    result
}

/// List every current window in every tabpage (`nvim_list_wins`).
///
/// # Safety
/// The tabpage list and every tabpage's window list must consist of
/// live pointers.
#[must_use]
pub unsafe fn nvim_list_wins() -> Array {
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let current_tab = globals.curtab;
    let current_firstwin = globals.firstwin;
    let mut tab = globals.first_tabpage;
    let mut result = Vec::new();
    while !tab.is_null() {
        let mut win = if std::ptr::eq(tab, current_tab) {
            current_firstwin
        } else {
            unsafe { (*tab).tp_firstwin }
        };
        while !win.is_null() {
            result.push(Object::Window(unsafe { (*win).handle }));
            win = unsafe { (*win).w_next };
        }
        tab = unsafe { (*tab).tp_next };
    }
    result
}

/// List every current tabpage (`nvim_list_tabpages`).
///
/// # Safety
/// `GLOBALS.first_tabpage` and each `tp_next` link must form a live
/// tabpage list.
#[must_use]
pub unsafe fn nvim_list_tabpages() -> Array {
    let mut result = Vec::new();
    let mut tab = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tab.is_null() {
        result.push(Object::Tabpage(unsafe { (*tab).handle }));
        tab = unsafe { (*tab).tp_next };
    }
    result
}

/// Get the current window's handle (`nvim_get_current_win`).
///
/// # Safety
/// `GLOBALS.curwin` must be a valid, non-null pointer to a live
/// `WinT`.
#[must_use]
pub unsafe fn nvim_get_current_win() -> Window {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.handle
}

/// Get the current buffer's handle (`nvim_get_current_buf`).
///
/// # Safety
/// `GLOBALS.curbuf` must be a valid, non-null pointer to a live
/// `BufT`.
#[must_use]
pub unsafe fn nvim_get_current_buf() -> Buffer {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }.handle
}

/// Get the current tab page's handle (`nvim_get_current_tabpage`).
///
/// # Safety
/// `GLOBALS.curtab` must be a valid, non-null pointer to a live
/// `TabpageT`.
#[must_use]
pub unsafe fn nvim_get_current_tabpage() -> Tabpage {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*crate::globals::GLOBALS.get_mut().curtab }.handle
}

/// Whether byte length `len` passes `nvim_strwidth`'s own length
/// guard (`text.size <= INT_MAX` in the original). Factored out from
/// [`nvim_strwidth`] itself so its exact boundary can be tested
/// directly - exercising the real "too long" branch end-to-end would
/// need constructing a genuine ~2 GiB `Vec<u8>`, impractical for a
/// test that may run dozens of times per flakiness check.
#[must_use]
fn text_length_ok(len: usize) -> bool {
    len <= i32::MAX as usize
}

/// Calculate the number of display cells `text` occupies
/// (`nvim_strwidth`), or `0` with a real, structured `Error` when
/// `text` is longer than `i32::MAX` bytes (matching the original's
/// own `VALIDATE_S`/`api_err_invalid` message format exactly:
/// `"Invalid text length: '(too long)'"`).
///
/// # Safety
/// Forwarded from [`crate::mbyte::mb_string2cells`]'s own safety doc.
pub unsafe fn nvim_strwidth(text: &NvimString, err: &mut Error) -> Integer {
    if !text_length_ok(text.len()) {
        err.r#type = ErrorType::Validation;
        err.msg = Some("Invalid text length: '(too long)'".to_string());
        return 0;
    }

    // Matches the original's own unchecked `(Integer)mb_string2cells(...)`
    // cast - `mb_string2cells`'s own result can be at most roughly
    // `2 * text.len()` (every character occupies at most 2 display
    // cells), which is always well within `i64`'s range given the
    // `i32::MAX` bound on `text.len()` just above.
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { crate::mbyte::mb_string2cells(text) }) as i64
}

/// Get the active highlight namespace (`nvim_get_hl_ns`).
///
/// `winid == None` selects the global namespace; a window ID returns
/// that window's local namespace.
///
/// # Safety
/// Forwarded from `find_window_by_handle` when `winid` is present, and
/// otherwise reads shared highlight state.
pub unsafe fn nvim_get_hl_ns(winid: Option<Window>, err: &mut Error) -> Integer {
    if let Some(winid) = winid {
        let win = unsafe { crate::api::private::helpers::find_window_by_handle(winid, err) };
        if win.is_null() {
            0
        } else {
            i64::from(unsafe { (*win).w_ns_hl })
        }
    } else {
        i64::from(unsafe { *crate::highlight::NS_HL_GLOBAL.get_mut() })
    }
}

/// Set the active global highlight namespace (`nvim_set_hl_ns`).
///
/// # Safety
/// Mutates shared highlight namespace/provider/group state and
/// schedules a redraw for every live window.
pub unsafe fn nvim_set_hl_ns(ns_id: Integer, err: &mut Error) {
    if ns_id < 0 {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!("Invalid 'namespace': {ns_id}"));
        return;
    }
    unsafe { *crate::highlight::NS_HL_GLOBAL.get_mut() = ns_id as i32 };
    let _ = unsafe { crate::highlight::hl_check_ns() };
    unsafe { crate::drawscreen::redraw_all_later(crate::drawscreen::UPD_NOT_VALID) };
}

/// Set the fast-callback highlight namespace
/// (`nvim_set_hl_ns_fast`).
///
/// Unlike [`nvim_set_hl_ns`], the original deliberately performs no
/// nonnegative validation.
///
/// # Safety
/// Mutates shared highlight namespace/provider/group state.
pub unsafe fn nvim_set_hl_ns_fast(ns_id: Integer) {
    unsafe { *crate::highlight::NS_HL_FAST.get_mut() = ns_id as i32 };
    let _ = unsafe { crate::highlight::hl_check_ns() };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{BufT, TabpageT, WinT};

    #[test]
    fn nvim_list_bufs_returns_every_linked_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = BufT {
            handle: 72,
            ..Default::default()
        };
        let second_ptr = std::ptr::addr_of_mut!(second);
        let mut first = BufT {
            handle: 71,
            b_next: second_ptr,
            ..Default::default()
        };
        let first_ptr = std::ptr::addr_of_mut!(first);
        let _firstbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.firstbuf, first_ptr)
        };

        let buffers = unsafe { nvim_list_bufs() };

        assert!(matches!(
            buffers.as_slice(),
            [Object::Buffer(71), Object::Buffer(72)]
        ));
    }

    #[test]
    fn nvim_list_bufs_returns_empty_for_an_empty_buffer_list() {
        let _lock = crate::globals::global_state_test_lock();
        let _firstbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.firstbuf,
                std::ptr::null_mut(),
            )
        };
        assert!(unsafe { nvim_list_bufs() }.is_empty());
    }

    #[test]
    fn nvim_list_wins_returns_windows_from_every_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut other_win = WinT {
            handle: 83,
            ..Default::default()
        };
        let other_win_ptr = std::ptr::addr_of_mut!(other_win);
        let mut current_second = WinT {
            handle: 82,
            ..Default::default()
        };
        let current_second_ptr = std::ptr::addr_of_mut!(current_second);
        let mut current_first = WinT {
            handle: 81,
            w_next: current_second_ptr,
            ..Default::default()
        };
        let current_first_ptr = std::ptr::addr_of_mut!(current_first);
        let mut other_tab = TabpageT {
            handle: 92,
            tp_firstwin: other_win_ptr,
            ..Default::default()
        };
        let other_tab_ptr = std::ptr::addr_of_mut!(other_tab);
        let mut current_tab = TabpageT {
            handle: 91,
            tp_next: other_tab_ptr,
            ..Default::default()
        };
        let current_tab_ptr = std::ptr::addr_of_mut!(current_tab);
        let _first_tabpage = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.first_tabpage,
                current_tab_ptr,
            )
        };
        let _curtab = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.curtab, current_tab_ptr)
        };
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.firstwin,
                current_first_ptr,
            )
        };

        let windows = unsafe { nvim_list_wins() };

        assert!(matches!(
            windows.as_slice(),
            [Object::Window(81), Object::Window(82), Object::Window(83)]
        ));
    }

    #[test]
    fn nvim_list_wins_returns_empty_without_tabpages() {
        let _lock = crate::globals::global_state_test_lock();
        let _first_tabpage = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.first_tabpage,
                std::ptr::null_mut(),
            )
        };
        assert!(unsafe { nvim_list_wins() }.is_empty());
    }

    #[test]
    fn nvim_list_tabpages_returns_every_linked_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = TabpageT {
            handle: 102,
            ..Default::default()
        };
        let second_ptr = std::ptr::addr_of_mut!(second);
        let mut first = TabpageT {
            handle: 101,
            tp_next: second_ptr,
            ..Default::default()
        };
        let first_ptr = std::ptr::addr_of_mut!(first);
        let _first_tabpage = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.first_tabpage, first_ptr)
        };

        let tabpages = unsafe { nvim_list_tabpages() };

        assert!(matches!(
            tabpages.as_slice(),
            [Object::Tabpage(101), Object::Tabpage(102)]
        ));
    }

    #[test]
    fn nvim_list_tabpages_returns_empty_for_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let _first_tabpage = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.first_tabpage,
                std::ptr::null_mut(),
            )
        };
        assert!(unsafe { nvim_list_tabpages() }.is_empty());
    }

    struct HighlightNamespaceGuard {
        global: i32,
        win: i32,
        fast: i32,
        active: i32,
        need_changed: bool,
        firstwin: *mut WinT,
        first_tabpage: *mut TabpageT,
        curtab: *mut TabpageT,
    }

    impl HighlightNamespaceGuard {
        fn set(global: i32, win: i32, fast: i32, active: i32) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = Self {
                global: unsafe { *crate::highlight::NS_HL_GLOBAL.get_mut() },
                win: unsafe { *crate::highlight::NS_HL_WIN.get_mut() },
                fast: unsafe { *crate::highlight::NS_HL_FAST.get_mut() },
                active: unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() },
                need_changed: globals.need_highlight_changed,
                firstwin: globals.firstwin,
                first_tabpage: globals.first_tabpage,
                curtab: globals.curtab,
            };
            unsafe {
                *crate::highlight::NS_HL_GLOBAL.get_mut() = global;
                *crate::highlight::NS_HL_WIN.get_mut() = win;
                *crate::highlight::NS_HL_FAST.get_mut() = fast;
                *crate::highlight::NS_HL_ACTIVE.get_mut() = active;
            }
            globals.need_highlight_changed = false;
            globals.firstwin = std::ptr::null_mut();
            globals.first_tabpage = std::ptr::null_mut();
            globals.curtab = std::ptr::null_mut();
            guard
        }
    }

    impl Drop for HighlightNamespaceGuard {
        fn drop(&mut self) {
            unsafe {
                *crate::highlight::NS_HL_GLOBAL.get_mut() = self.global;
                *crate::highlight::NS_HL_WIN.get_mut() = self.win;
                *crate::highlight::NS_HL_FAST.get_mut() = self.fast;
                *crate::highlight::NS_HL_ACTIVE.get_mut() = self.active;
                let globals = crate::globals::GLOBALS.get_mut();
                globals.need_highlight_changed = self.need_changed;
                globals.firstwin = self.firstwin;
                globals.first_tabpage = self.first_tabpage;
                globals.curtab = self.curtab;
            }
        }
    }

    #[test]
    fn nvim_get_current_win_returns_curwin_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 42, ..Default::default() };
        let win_ptr = std::ptr::addr_of_mut!(win);

        // SAFETY: single-threaded test, GLOBALS restored below.
        unsafe {
            let prev_curwin = crate::globals::GLOBALS.get_mut().curwin;
            crate::globals::GLOBALS.get_mut().curwin = win_ptr;

            assert_eq!(nvim_get_current_win(), 42);

            crate::globals::GLOBALS.get_mut().curwin = prev_curwin;
        }
    }

    #[test]
    fn nvim_get_current_buf_returns_curbuf_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { handle: 43, ..Default::default() };
        let buf_ptr = std::ptr::addr_of_mut!(buf);

        // SAFETY: single-threaded test, GLOBALS restored below.
        unsafe {
            let prev_curbuf = crate::globals::GLOBALS.get_mut().curbuf;
            crate::globals::GLOBALS.get_mut().curbuf = buf_ptr;

            assert_eq!(nvim_get_current_buf(), 43);

            crate::globals::GLOBALS.get_mut().curbuf = prev_curbuf;
        }
    }

    #[test]
    fn nvim_get_current_tabpage_returns_curtab_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tab = TabpageT { handle: 44, ..Default::default() };
        let tab_ptr = std::ptr::addr_of_mut!(tab);

        // SAFETY: single-threaded test, GLOBALS restored below.
        unsafe {
            let prev_curtab = crate::globals::GLOBALS.get_mut().curtab;
            crate::globals::GLOBALS.get_mut().curtab = tab_ptr;

            assert_eq!(nvim_get_current_tabpage(), 44);

            crate::globals::GLOBALS.get_mut().curtab = prev_curtab;
        }
    }

    #[test]
    fn nvim_strwidth_sums_ascii_widths() {
        let mut err = Error::default();
        let text: NvimString = b"hello".to_vec();
        // SAFETY: pure ASCII input, no OPTION_VARS-dependent branch.
        assert_eq!(unsafe { nvim_strwidth(&text, &mut err) }, 5);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_strwidth_counts_a_double_width_char_as_two() {
        let mut err = Error::default();
        let text: NvimString = "一".as_bytes().to_vec();
        // SAFETY: same as above.
        assert_eq!(unsafe { nvim_strwidth(&text, &mut err) }, 2);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_strwidth_zero_for_empty_string() {
        let mut err = Error::default();
        let text: NvimString = Vec::new();
        // SAFETY: same as above.
        assert_eq!(unsafe { nvim_strwidth(&text, &mut err) }, 0);
        assert!(!err.is_set());
    }

    /// `text_length_ok` (factored out of `nvim_strwidth` itself)
    /// lets the exact `i32::MAX` boundary be tested directly, without
    /// needing a genuine ~2 GiB `Vec<u8>` allocation to exercise the
    /// real "too long" branch end-to-end.
    #[test]
    fn text_length_ok_true_at_the_i32_max_boundary() {
        assert!(text_length_ok(i32::MAX as usize));
    }

    #[test]
    fn text_length_ok_false_just_past_the_i32_max_boundary() {
        assert!(!text_length_ok(i32::MAX as usize + 1));
    }

    #[test]
    fn nvim_get_hl_ns_returns_global_or_window_namespace() {
        let _lock = crate::globals::global_state_test_lock();
        let _namespace = HighlightNamespaceGuard::set(6, -1, -1, -1);
        let mut err = Error::default();
        assert_eq!(unsafe { nvim_get_hl_ns(None, &mut err) }, 6);
        assert!(!err.is_set());

        let mut win = WinT {
            handle: 42,
            w_ns_hl: 9,
            ..Default::default()
        };
        let win_ptr = std::ptr::addr_of_mut!(win);
        let mut tab = TabpageT::default();
        let tab_ptr = std::ptr::addr_of_mut!(tab);
        unsafe {
            let globals = crate::globals::GLOBALS.get_mut();
            globals.firstwin = win_ptr;
            globals.first_tabpage = tab_ptr;
            globals.curtab = tab_ptr;
        }
        assert_eq!(unsafe { nvim_get_hl_ns(Some(42), &mut err) }, 9);

        assert_eq!(unsafe { nvim_get_hl_ns(Some(99), &mut err) }, 0);
        assert_eq!(err.msg.as_deref(), Some("Invalid window id: 99"));
    }

    #[test]
    fn nvim_set_hl_ns_validates_and_selects_global_namespace_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let _namespace = HighlightNamespaceGuard::set(7, -1, -1, 7);
        let mut err = Error::default();

        unsafe { nvim_set_hl_ns(-1, &mut err) };
        assert_eq!(err.r#type, ErrorType::Validation);
        assert_eq!(err.msg.as_deref(), Some("Invalid 'namespace': -1"));
        assert_eq!(unsafe { *crate::highlight::NS_HL_GLOBAL.get_mut() }, 7);

        err = Error::default();
        unsafe { nvim_set_hl_ns(0, &mut err) };
        assert!(!err.is_set());
        assert_eq!(unsafe { *crate::highlight::NS_HL_GLOBAL.get_mut() }, 0);
        assert_eq!(unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() }, 0);
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.need_highlight_changed);
    }

    #[test]
    fn nvim_set_hl_ns_fast_accepts_negative_namespace() {
        let _lock = crate::globals::global_state_test_lock();
        let _namespace = HighlightNamespaceGuard::set(0, -1, 3, 3);

        unsafe { nvim_set_hl_ns_fast(-1) };

        assert_eq!(unsafe { *crate::highlight::NS_HL_FAST.get_mut() }, -1);
        assert_eq!(unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() }, 0);
    }
}
