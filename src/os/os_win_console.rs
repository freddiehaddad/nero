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
    #[cfg(test)]
    fn GetConsoleWindow() -> WindowHandle;
    fn CreateFileA(
        filename: *const u8,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *mut std::ffi::c_void,
        creation_disposition: u32,
        flags: u32,
        template: WindowHandle,
    ) -> WindowHandle;
}

#[link(name = "ucrt")]
unsafe extern "C" {
    fn _open_osfhandle(handle: isize, flags: i32) -> i32;
    fn _close(fd: i32) -> i32;
}

/// Re-enable normal Ctrl-C processing (`os_enable_ctrl_c`).
pub fn os_enable_ctrl_c() {
    unsafe { SetConsoleCtrlHandler(std::ptr::null(), 0) };
}

/// Forget the cached console window (`os_clear_hwnd`).
pub fn os_clear_hwnd() {
    unsafe { *WINDOW.get_mut() = std::ptr::null_mut() };
}

/// Open the current console input as a CRT descriptor
/// (`os_open_conin_fd`).
#[must_use]
pub fn os_open_conin_fd() -> i32 {
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;
    const INVALID_HANDLE_VALUE: WindowHandle =
        -1isize as WindowHandle;
    let handle = unsafe {
        CreateFileA(
            c"CONIN$".as_ptr().cast(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(handle, INVALID_HANDLE_VALUE);
    let descriptor =
        unsafe { _open_osfhandle(handle as isize, libc::O_RDONLY) };
    assert_ne!(descriptor, -1);
    descriptor
}

/// Replace standard input with `CONIN$`
/// (`os_redirect_stdin_to_conin`).
pub fn os_redirect_stdin_to_conin() {
    unsafe { _close(0) };
    assert_eq!(os_open_conin_fd(), 0);
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

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call console Win32 FFI")]
    fn console_input_can_be_wrapped_when_a_console_is_attached() {
        if unsafe { GetConsoleWindow() }.is_null() {
            return;
        }
        let descriptor = os_open_conin_fd();
        assert!(descriptor >= 0);
        assert_eq!(unsafe { _close(descriptor) }, 0);
    }
}
