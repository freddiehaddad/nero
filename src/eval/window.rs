//! Window-related Vimscript helpers from `src/nvim/eval/window.c`.

/// Whether a window participates in window-number lookup
/// (`win_has_winnr`).
///
/// # Safety
/// `wp` and `tp` must be valid live pointers; reads shared current
/// window/tab pointers.
#[must_use]
pub unsafe fn win_has_winnr(
    wp: *const crate::buffer_defs::WinT,
    tp: *const crate::buffer_defs::TabpageT,
) -> bool {
    assert!(!wp.is_null());
    assert!(!tp.is_null());
    let globals = crate::globals::GLOBALS.as_ptr();
    let tab_current = if tp == unsafe { (*globals).curtab } {
        unsafe { (*globals).curwin }
    } else {
        unsafe { (*tp).tp_curwin }
    };
    wp == tab_current
        || (!unsafe { (*wp).w_config.hide }
            && unsafe { (*wp).w_config.focusable })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_window_always_has_a_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut window = crate::buffer_defs::WinT::default();
        window.w_config.hide = true;
        window.w_config.focusable = false;
        let window = std::ptr::addr_of_mut!(window);
        let mut tab = crate::buffer_defs::TabpageT {
            tp_curwin: window,
            ..Default::default()
        };
        let tab = std::ptr::addr_of_mut!(tab);
        let _curtab = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.curtab,
                tab,
            )
        };
        let _curwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.curwin,
                window,
            )
        };
        assert!(unsafe { win_has_winnr(window, tab) });
    }

    #[test]
    fn another_tabs_current_window_has_a_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut window = crate::buffer_defs::WinT::default();
        window.w_config.hide = true;
        window.w_config.focusable = false;
        let window = std::ptr::addr_of_mut!(window);
        let mut tab = crate::buffer_defs::TabpageT {
            tp_curwin: window,
            ..Default::default()
        };
        let tab = std::ptr::addr_of_mut!(tab);
        let _curtab = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.curtab,
                std::ptr::null_mut(),
            )
        };
        assert!(unsafe { win_has_winnr(window, tab) });
    }

    #[test]
    fn visible_focusable_windows_have_window_numbers() {
        let _lock = crate::globals::global_state_test_lock();
        let mut window = crate::buffer_defs::WinT::default();
        let window_ptr = std::ptr::addr_of_mut!(window);
        let mut tab = crate::buffer_defs::TabpageT::default();
        let tab_ptr = std::ptr::addr_of_mut!(tab);
        let _curtab = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.curtab,
                std::ptr::null_mut(),
            )
        };
        assert!(unsafe { win_has_winnr(window_ptr, tab_ptr) });

        window.w_config.hide = true;
        assert!(!unsafe { win_has_winnr(window_ptr, tab_ptr) });
        window.w_config.hide = false;
        window.w_config.focusable = false;
        assert!(!unsafe { win_has_winnr(window_ptr, tab_ptr) });
    }
}
