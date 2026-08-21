//! Dependency-free Windows console functions from
//! `src/nvim/os/os_win_console.c`.

use crate::globals::GlobalCell;

type WindowHandle = *mut std::ffi::c_void;

static ORIGINAL_TITLE: GlobalCell<[u8; 256]> = GlobalCell::new([0; 256]);
static WINDOW: GlobalCell<WindowHandle> =
    GlobalCell::new(std::ptr::null_mut());

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: *const std::ffi::c_void,
        add: i32,
    ) -> i32;
    fn GetConsoleTitleA(title: *mut u8, size: u32) -> u32;
    fn SetConsoleTitleA(title: *const u8) -> i32;
}

/// Re-enable normal Ctrl-C processing (`os_enable_ctrl_c`).
pub fn os_enable_ctrl_c() {
    unsafe { SetConsoleCtrlHandler(std::ptr::null(), 0) };
}

/// Forget the cached console window (`os_clear_hwnd`).
pub fn os_clear_hwnd() {
    unsafe { *WINDOW.get_mut() = std::ptr::null_mut() };
}

/// Save the current console title (`os_title_save`).
pub fn os_title_save() {
    let title = unsafe { ORIGINAL_TITLE.get_mut() };
    title.fill(0);
    unsafe { GetConsoleTitleA(title.as_mut_ptr(), title.len() as u32) };
}

/// Restore the saved console title (`os_title_reset`).
pub fn os_title_reset() {
    unsafe { SetConsoleTitleA(ORIGINAL_TITLE.get_mut().as_ptr()) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_hwnd_resets_cached_window() {
        let _lock = crate::globals::global_state_test_lock();
        let previous = unsafe {
            std::mem::replace(
                WINDOW.get_mut(),
                std::ptr::dangling_mut(),
            )
        };
        os_clear_hwnd();
        assert!(unsafe { *WINDOW.get_mut() }.is_null());
        unsafe { *WINDOW.get_mut() = previous };
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call console Win32 FFI")]
    fn console_control_and_title_functions_tolerate_missing_console() {
        let _lock = crate::globals::global_state_test_lock();
        let previous = unsafe { *ORIGINAL_TITLE.get_mut() };
        os_enable_ctrl_c();
        os_title_save();
        os_title_reset();
        unsafe { *ORIGINAL_TITLE.get_mut() = previous };
    }
}
