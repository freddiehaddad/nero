//! Translated from `src/nvim/api/private/helpers.c` (tractable subset
//! only - most of this large file needs `Arena`/`Object`-conversion
//! machinery, `typval_T` <-> `Object` bridging, and the msgpack-rpc
//! dispatch layer, none of which exist yet).
//!
//! Translated: [`find_window_by_handle`] (the `window == 0 -> curwin`
//! special case, plus real, structured [`Error`] population on
//! failure - matching the original's own `VALIDATE_INT`/
//! `api_err_invalid` message format exactly, since `Error` is a real,
//! returned value future API callers will read, not a skippable
//! display side effect like `emsg()`).
//!
//! Deferred: `find_buffer_by_handle`/`find_tabpage_by_handle` (same
//! shape, not needed yet - no other `api/*.c` file has been started),
//! `api_set_error`/`api_err_invalid` themselves (both are generic,
//! variadic/printf-style message formatters; this crate uses Rust's
//! own `format!` directly at each real call site instead of
//! translating the general mechanism, matching the established
//! `fmt_g`-style "a narrow, purpose-built helper for one call site,
//! not a general `vim_snprintf`" precedent - if/when a second real
//! caller needs this, revisit whether a shared helper is worthwhile).

use crate::api::private::defs::{Error, ErrorType, Window};
use crate::buffer_defs::WinT;

/// Find window `window` (a real window handle, or `0` for the current
/// window), populating `err` with a real, structured
/// `"Invalid window id: {window}"` message on failure
/// (`find_window_by_handle`).
///
/// # Safety
/// Forwarded from [`crate::window::handle_get_window`]'s own safety
/// doc.
pub unsafe fn find_window_by_handle(window: Window, err: &mut Error) -> *mut WinT {
    if window == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let rv = unsafe { crate::window::handle_get_window(window) };
    if rv.is_null() {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!("Invalid window id: {window}"));
    }
    rv
}

#[cfg(test)]
mod tests {
    use super::*;

    fn focusable_win(handle: crate::types_defs::HandleT) -> WinT {
        WinT {
            handle,
            w_config: crate::buffer_defs::WinConfig {
                focusable: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// A `window == 0` request always resolves to `GLOBALS.curwin`,
    /// regardless of whether it is registered in any tabpage's own
    /// window list (matching the original's own unconditional
    /// `window == 0` fast path, which never even looks at
    /// `handle_get_window`).
    #[test]
    fn find_window_by_handle_zero_resolves_to_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut w = focusable_win(42);
            let w_ptr = std::ptr::addr_of_mut!(w);
            let prev_curwin = crate::globals::GLOBALS.get_mut().curwin;
            crate::globals::GLOBALS.get_mut().curwin = w_ptr;

            let mut err = Error::default();
            let found = find_window_by_handle(0, &mut err);

            crate::globals::GLOBALS.get_mut().curwin = prev_curwin;

            assert!(std::ptr::eq(found, w_ptr));
            assert!(!err.is_set());
        }
    }

    /// A real, positive handle resolves via [`crate::window::
    /// handle_get_window`]'s own all-tabs walk, leaving `err`
    /// untouched on success.
    #[test]
    fn find_window_by_handle_resolves_a_real_handle() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut w = focusable_win(7);
            let w_ptr = std::ptr::addr_of_mut!(w);
            let mut tab = crate::buffer_defs::TabpageT::default();
            let tab_ptr = std::ptr::addr_of_mut!(tab);
            tab.tp_firstwin = w_ptr;

            let prev_firstwin = crate::globals::GLOBALS.get_mut().firstwin;
            let prev_curtab = crate::globals::GLOBALS.get_mut().curtab;
            let prev_first_tabpage = crate::globals::GLOBALS.get_mut().first_tabpage;
            crate::globals::GLOBALS.get_mut().firstwin = w_ptr;
            crate::globals::GLOBALS.get_mut().curtab = tab_ptr;
            crate::globals::GLOBALS.get_mut().first_tabpage = tab_ptr;

            let mut err = Error::default();
            let found = find_window_by_handle(7, &mut err);

            crate::globals::GLOBALS.get_mut().firstwin = prev_firstwin;
            crate::globals::GLOBALS.get_mut().curtab = prev_curtab;
            crate::globals::GLOBALS.get_mut().first_tabpage = prev_first_tabpage;

            assert!(std::ptr::eq(found, w_ptr));
            assert!(!err.is_set());
        }
    }

    /// An unrecognized handle returns null and populates `err` with
    /// the exact original message format.
    #[test]
    fn find_window_by_handle_reports_a_structured_error_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let prev_firstwin = crate::globals::GLOBALS.get_mut().firstwin;
            let prev_curtab = crate::globals::GLOBALS.get_mut().curtab;
            let prev_first_tabpage = crate::globals::GLOBALS.get_mut().first_tabpage;
            crate::globals::GLOBALS.get_mut().firstwin = std::ptr::null_mut();
            crate::globals::GLOBALS.get_mut().curtab = std::ptr::null_mut();
            crate::globals::GLOBALS.get_mut().first_tabpage = std::ptr::null_mut();

            let mut err = Error::default();
            let found = find_window_by_handle(999, &mut err);

            crate::globals::GLOBALS.get_mut().firstwin = prev_firstwin;
            crate::globals::GLOBALS.get_mut().curtab = prev_curtab;
            crate::globals::GLOBALS.get_mut().first_tabpage = prev_first_tabpage;

            assert!(found.is_null());
            assert!(err.is_set());
            assert_eq!(err.r#type, ErrorType::Validation);
            assert_eq!(err.msg.as_deref(), Some("Invalid window id: 999"));
        }
    }
}
