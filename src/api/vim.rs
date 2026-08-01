//! Translated from `src/nvim/api/vim.c` (tractable subset only - most
//! of this huge file needs `Array`/`Dict`/`Object`-conversion
//! machinery, the msgpack-rpc dispatch layer, command execution, and
//! the Lua host, none of which are wired into this API layer yet).
//!
//! Translated: [`nvim_get_current_win`]/[`nvim_get_current_buf`]/
//! [`nvim_get_current_tabpage`] (harvested ahead of the rest of this
//! file, matching the established "one tractable function ahead of a
//! huge file" precedent used elsewhere in this crate, e.g.
//! `ex_docmd.rs`; `nvim_get_current_win` is also `api/tabpage.c`'s own
//! real dependency - `nvim_tabpage_get_win` calls it directly when the
//! tabpage in question is the current one), and [`nvim_strwidth`] (via
//! the already-existing `mbyte.rs::mb_string2cells`).

use crate::api::private::defs::{Buffer, Error, ErrorType, Integer, NvimString, Tabpage, Window};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{BufT, TabpageT, WinT};

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
}
