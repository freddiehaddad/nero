//! Translated from `src/nvim/api/vim.c` (tractable subset only - most
//! of this huge file needs `Array`/`Dict`/`Object`-conversion
//! machinery, the msgpack-rpc dispatch layer, command execution, and
//! the Lua host, none of which are wired into this API layer yet).
//!
//! Translated: [`nvim_get_current_win`] - a real, standalone API
//! function (harvested ahead of the rest of this file, matching the
//! established "one tractable function ahead of a huge file"
//! precedent used elsewhere in this crate, e.g. `ex_docmd.rs`), and
//! also `api/tabpage.c`'s own real dependency (`nvim_tabpage_get_win`
//! calls it directly when the tabpage in question is the current
//! one).

use crate::api::private::defs::Window;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::WinT;

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
}
