//! Translated from `src/nvim/api/private/helpers.c` (tractable subset
//! only - most of this large file needs `Arena`/`Object`-conversion
//! machinery, `typval_T` <-> `Object` bridging, and the msgpack-rpc
//! dispatch layer, none of which exist yet).
//!
//! Translated: [`find_window_by_handle`]/[`find_buffer_by_handle`]/
//! [`find_tab_by_handle`] (the `window`/`buffer`/`tabpage`
//! `== 0 -> curwin`/`curbuf`/`curtab` special case, plus real,
//! structured [`Error`] population on failure - matching the
//! original's own `VALIDATE_INT`/`api_err_invalid` message format
//! exactly, since `Error` is a real, returned value future API
//! callers will read, not a skippable display side effect like
//! `emsg()`).
//! Also [`object_to_hl_id`] - String names resolve through the real
//! highlight-group registry, while Integer IDs are range-checked
//! against the current group count.
//!
//! Deferred: `api_set_error`/`api_err_invalid` themselves (both are
//! generic, variadic/printf-style message formatters; this crate uses
//! Rust's own `format!` directly at each real call site instead of
//! translating the general mechanism, matching the established
//! `fmt_g`-style "a narrow, purpose-built helper for one call site,
//! not a general `vim_snprintf`" precedent - if/when a second real
//! caller needs this, revisit whether a shared helper is worthwhile).

use crate::api::private::defs::{Buffer, Error, ErrorType, Object, Tabpage, Window};
use crate::buffer_defs::{BufT, TabpageT, WinT};

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

/// Find buffer `buffer` (a real buffer handle, or `0` for the current
/// buffer), populating `err` with a real, structured
/// `"Invalid buffer id: {buffer}"` message on failure
/// (`find_buffer_by_handle`).
///
/// # Safety
/// Forwarded from [`crate::buffer::handle_get_buffer`]'s own safety
/// doc.
pub unsafe fn find_buffer_by_handle(buffer: Buffer, err: &mut Error) -> *mut BufT {
    if buffer == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let rv = unsafe { crate::buffer::handle_get_buffer(buffer) };
    if rv.is_null() {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!("Invalid buffer id: {buffer}"));
    }
    rv
}

/// Find tab page `tabpage` (a real tabpage handle, or `0` for the
/// current tabpage), populating `err` with a real, structured
/// `"Invalid tabpage id: {tabpage}"` message on failure
/// (`find_tab_by_handle`).
///
/// # Safety
/// Forwarded from [`crate::window::handle_get_tabpage`]'s own safety
/// doc.
pub unsafe fn find_tab_by_handle(tabpage: Tabpage, err: &mut Error) -> *mut TabpageT {
    if tabpage == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let rv = unsafe { crate::window::handle_get_tabpage(tabpage) };
    if rv.is_null() {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!("Invalid tabpage id: {tabpage}"));
    }
    rv
}

/// Convert an API object to a highlight-group ID (`object_to_hl_id`).
///
/// String names create the group when it does not exist, matching
/// `syn_check_group`; Integer IDs are accepted only when they identify
/// an existing, 1-based group. Other object types set a validation
/// error and return `0`.
///
/// # Safety
/// Forwarded from the highlight-group registry functions.
pub unsafe fn object_to_hl_id(obj: &Object, what: &str, err: &mut Error) -> i32 {
    match obj {
        Object::String(name) => {
            if name.is_empty() {
                0
            } else {
                unsafe { crate::highlight_group::syn_check_group(name) }
            }
        }
        Object::Integer(id) => {
            let id = *id as i32;
            if (1..=unsafe { crate::highlight_group::highlight_num_groups() }).contains(&id) {
                id
            } else {
                0
            }
        }
        _ => {
            err.r#type = ErrorType::Validation;
            err.msg = Some(format!("Invalid 'hl_group': expected {what}"));
            0
        }
    }
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

    struct HighlightTableGuard {
        items: Vec<crate::highlight_group::HlGroup>,
        names: crate::map::Map<Vec<u8>, i32>,
    }

    impl HighlightTableGuard {
        fn empty() -> Self {
            let table = unsafe { crate::highlight_group::HL_TABLE.get_mut() };
            let items = std::mem::take(&mut table.items);
            let names = std::mem::replace(
                unsafe { crate::highlight_group::HIGHLIGHT_UNAMES.get_mut() },
                crate::map::Map::new(),
            );
            Self { items, names }
        }
    }

    impl Drop for HighlightTableGuard {
        fn drop(&mut self) {
            unsafe { crate::highlight_group::HL_TABLE.get_mut() }.items =
                std::mem::take(&mut self.items);
            *unsafe { crate::highlight_group::HIGHLIGHT_UNAMES.get_mut() } =
                std::mem::replace(&mut self.names, crate::map::Map::new());
        }
    }

    #[test]
    fn object_to_hl_id_creates_a_named_group_and_accepts_its_integer_id() {
        let _lock = crate::globals::global_state_test_lock();
        let _groups = HighlightTableGuard::empty();
        let mut err = Error::default();

        assert_eq!(
            unsafe { object_to_hl_id(&Object::String(b"Border".to_vec()), "highlight", &mut err) },
            1
        );
        assert!(!err.is_set());
        assert_eq!(
            unsafe { object_to_hl_id(&Object::Integer(1), "highlight", &mut err) },
            1
        );
        assert_eq!(
            unsafe { object_to_hl_id(&Object::Integer(2), "highlight", &mut err) },
            0
        );
    }

    #[test]
    fn object_to_hl_id_accepts_an_empty_name_as_no_group() {
        let _lock = crate::globals::global_state_test_lock();
        let _groups = HighlightTableGuard::empty();
        let mut err = Error::default();

        assert_eq!(
            unsafe { object_to_hl_id(&Object::String(Vec::new()), "highlight", &mut err) },
            0
        );
        assert!(!err.is_set());
        assert_eq!(unsafe { crate::highlight_group::highlight_num_groups() }, 0);
    }

    #[test]
    fn object_to_hl_id_rejects_an_unexpected_object_type() {
        let _lock = crate::globals::global_state_test_lock();
        let _groups = HighlightTableGuard::empty();
        let mut err = Error::default();

        assert_eq!(
            unsafe { object_to_hl_id(&Object::Boolean(true), "border highlight", &mut err) },
            0
        );
        assert!(err.is_set());
        assert_eq!(err.r#type, ErrorType::Validation);
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

    /// A `buffer == 0` request always resolves to `GLOBALS.curbuf`,
    /// regardless of whether it is registered in `GLOBALS.lastbuf`'s
    /// own list (matching the original's own unconditional
    /// `buffer == 0` fast path).
    #[test]
    fn find_buffer_by_handle_zero_resolves_to_curbuf() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut b = BufT { handle: 42, ..Default::default() };
            let b_ptr = std::ptr::addr_of_mut!(b);
            let prev_curbuf = crate::globals::GLOBALS.get_mut().curbuf;
            crate::globals::GLOBALS.get_mut().curbuf = b_ptr;

            let mut err = Error::default();
            let found = find_buffer_by_handle(0, &mut err);

            crate::globals::GLOBALS.get_mut().curbuf = prev_curbuf;

            assert!(std::ptr::eq(found, b_ptr));
            assert!(!err.is_set());
        }
    }

    /// A real, positive handle resolves via [`crate::buffer::
    /// handle_get_buffer`]'s own `lastbuf`/`b_prev` walk, leaving
    /// `err` untouched on success.
    #[test]
    fn find_buffer_by_handle_resolves_a_real_handle() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut b = BufT { handle: 7, ..Default::default() };
            let b_ptr = std::ptr::addr_of_mut!(b);
            let prev_lastbuf = crate::globals::GLOBALS.get_mut().lastbuf;
            crate::globals::GLOBALS.get_mut().lastbuf = b_ptr;

            let mut err = Error::default();
            let found = find_buffer_by_handle(7, &mut err);

            crate::globals::GLOBALS.get_mut().lastbuf = prev_lastbuf;

            assert!(std::ptr::eq(found, b_ptr));
            assert!(!err.is_set());
        }
    }

    /// An unrecognized buffer handle returns null and populates `err`
    /// with the exact original message format.
    #[test]
    fn find_buffer_by_handle_reports_a_structured_error_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let prev_lastbuf = crate::globals::GLOBALS.get_mut().lastbuf;
            crate::globals::GLOBALS.get_mut().lastbuf = std::ptr::null_mut();

            let mut err = Error::default();
            let found = find_buffer_by_handle(999, &mut err);

            crate::globals::GLOBALS.get_mut().lastbuf = prev_lastbuf;

            assert!(found.is_null());
            assert!(err.is_set());
            assert_eq!(err.r#type, ErrorType::Validation);
            assert_eq!(err.msg.as_deref(), Some("Invalid buffer id: 999"));
        }
    }

    /// `find_buffer_by_handle(0, ...)`'s special case never even
    /// consults `handle_get_buffer`/`GLOBALS.lastbuf` - unlike
    /// `buflist_findnr(0)`, which resolves `0` to
    /// `curwin.w_alt_fnum` first. This test proves the two functions'
    /// `0`-handling genuinely differs: a `lastbuf` list containing
    /// ONLY a different, unrelated buffer (handle 5, not curbuf's own
    /// handle 42) must not affect `find_buffer_by_handle(0, ...)`'s
    /// result at all.
    #[test]
    fn find_buffer_by_handle_zero_ignores_lastbuf_unlike_buflist_findnr() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut curbuf = BufT { handle: 42, ..Default::default() };
            let curbuf_ptr = std::ptr::addr_of_mut!(curbuf);
            let mut other = BufT { handle: 5, ..Default::default() };
            let other_ptr = std::ptr::addr_of_mut!(other);

            let prev_curbuf = crate::globals::GLOBALS.get_mut().curbuf;
            let prev_lastbuf = crate::globals::GLOBALS.get_mut().lastbuf;
            crate::globals::GLOBALS.get_mut().curbuf = curbuf_ptr;
            crate::globals::GLOBALS.get_mut().lastbuf = other_ptr;

            let mut err = Error::default();
            let found = find_buffer_by_handle(0, &mut err);

            crate::globals::GLOBALS.get_mut().curbuf = prev_curbuf;
            crate::globals::GLOBALS.get_mut().lastbuf = prev_lastbuf;

            assert!(std::ptr::eq(found, curbuf_ptr));
            assert!(!std::ptr::eq(found, other_ptr));
            assert!(!err.is_set());
        }
    }

    /// A `tabpage == 0` request always resolves to `GLOBALS.curtab`,
    /// regardless of whether it is registered in
    /// `GLOBALS.first_tabpage`'s own list (matching the original's
    /// own unconditional `tabpage == 0` fast path).
    #[test]
    fn find_tab_by_handle_zero_resolves_to_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut tab = TabpageT { handle: 42, ..Default::default() };
            let tab_ptr = std::ptr::addr_of_mut!(tab);
            let prev_curtab = crate::globals::GLOBALS.get_mut().curtab;
            crate::globals::GLOBALS.get_mut().curtab = tab_ptr;

            let mut err = Error::default();
            let found = find_tab_by_handle(0, &mut err);

            crate::globals::GLOBALS.get_mut().curtab = prev_curtab;

            assert!(std::ptr::eq(found, tab_ptr));
            assert!(!err.is_set());
        }
    }

    /// A real, positive handle resolves via [`crate::window::
    /// handle_get_tabpage`]'s own tabpage-list walk, leaving `err`
    /// untouched on success.
    #[test]
    fn find_tab_by_handle_resolves_a_real_handle() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut tab = TabpageT { handle: 7, ..Default::default() };
            let tab_ptr = std::ptr::addr_of_mut!(tab);
            let prev_first_tabpage = crate::globals::GLOBALS.get_mut().first_tabpage;
            crate::globals::GLOBALS.get_mut().first_tabpage = tab_ptr;

            let mut err = Error::default();
            let found = find_tab_by_handle(7, &mut err);

            crate::globals::GLOBALS.get_mut().first_tabpage = prev_first_tabpage;

            assert!(std::ptr::eq(found, tab_ptr));
            assert!(!err.is_set());
        }
    }

    /// An unrecognized tabpage handle returns null and populates
    /// `err` with the exact original message format.
    #[test]
    fn find_tab_by_handle_reports_a_structured_error_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let prev_first_tabpage = crate::globals::GLOBALS.get_mut().first_tabpage;
            crate::globals::GLOBALS.get_mut().first_tabpage = std::ptr::null_mut();

            let mut err = Error::default();
            let found = find_tab_by_handle(999, &mut err);

            crate::globals::GLOBALS.get_mut().first_tabpage = prev_first_tabpage;

            assert!(found.is_null());
            assert!(err.is_set());
            assert_eq!(err.r#type, ErrorType::Validation);
            assert_eq!(err.msg.as_deref(), Some("Invalid tabpage id: 999"));
        }
    }
}
