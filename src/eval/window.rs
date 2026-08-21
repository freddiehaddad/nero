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

/// Return the window and containing tab for handle `id`
/// (`win_id2wp_tp`).
///
/// # Safety
/// The global tab/window linked lists must contain valid live
/// pointers for the duration of the traversal.
#[must_use]
pub unsafe fn win_id2wp_tp(
    id: i32,
    mut tab_out: Option<&mut *mut crate::buffer_defs::TabpageT>,
) -> *mut crate::buffer_defs::WinT {
    let mut tab =
        unsafe { (*crate::globals::GLOBALS.as_ptr()).first_tabpage };
    while !tab.is_null() {
        let mut window = unsafe { (*tab).tp_firstwin };
        while !window.is_null() {
            if unsafe { (*window).handle } == id {
                if let Some(output) = &mut tab_out {
                    **output = tab;
                }
                return window;
            }
            window = unsafe { (*window).w_next };
        }
        tab = unsafe { (*tab).tp_next };
    }
    std::ptr::null_mut()
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

    #[test]
    fn win_id2wp_tp_finds_a_window_and_its_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second_window = crate::buffer_defs::WinT {
            handle: 22,
            ..Default::default()
        };
        let second_window_ptr =
            std::ptr::addr_of_mut!(second_window);
        let mut first_window = crate::buffer_defs::WinT {
            handle: 11,
            ..Default::default()
        };
        let first_window_ptr = std::ptr::addr_of_mut!(first_window);

        let mut second_tab = crate::buffer_defs::TabpageT {
            tp_firstwin: second_window_ptr,
            ..Default::default()
        };
        let second_tab_ptr = std::ptr::addr_of_mut!(second_tab);
        let mut first_tab = crate::buffer_defs::TabpageT {
            tp_next: second_tab_ptr,
            tp_firstwin: first_window_ptr,
            ..Default::default()
        };
        let first_tab_ptr = std::ptr::addr_of_mut!(first_tab);
        let _tabs = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.first_tabpage,
                first_tab_ptr,
            )
        };

        let mut found_tab = std::ptr::null_mut();
        assert_eq!(
            unsafe { win_id2wp_tp(22, Some(&mut found_tab)) },
            second_window_ptr
        );
        assert_eq!(found_tab, second_tab_ptr);
    }

    #[test]
    fn win_id2wp_tp_returns_null_without_touching_output_when_missing() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tab = crate::buffer_defs::TabpageT::default();
        let tab_ptr = std::ptr::addr_of_mut!(tab);
        let _tabs = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.first_tabpage,
                tab_ptr,
            )
        };
        let mut output = std::ptr::dangling_mut();
        assert!(unsafe { win_id2wp_tp(99, Some(&mut output)) }.is_null());
        assert_eq!(output, std::ptr::dangling_mut());
    }
}
