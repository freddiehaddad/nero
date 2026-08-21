//! Dependency-free Windows console functions from
//! `src/nvim/os/os_win_console.c`.

use crate::globals::GlobalCell;

type WindowHandle = *mut std::ffi::c_void;

static ORIGINAL_TITLE: GlobalCell<[u8; 256]> = GlobalCell::new([0; 256]);
static WINDOW: GlobalCell<WindowHandle> =
    GlobalCell::new(std::ptr::null_mut());
static ORIGINAL_ICON_SMALL: GlobalCell<WindowHandle> =
    GlobalCell::new(std::ptr::null_mut());
static ORIGINAL_ICON: GlobalCell<WindowHandle> =
    GlobalCell::new(std::ptr::null_mut());

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: *const std::ffi::c_void,
        add: i32,
    ) -> i32;
    fn GetConsoleTitleA(title: *mut u8, size: u32) -> u32;
    fn SetConsoleTitleA(title: *const u8) -> i32;
    fn GetConsoleWindow() -> WindowHandle;
    fn FreeConsole() -> i32;
    fn AllocConsole() -> i32;
    fn CreateFileA(
        filename: *const u8,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *mut std::ffi::c_void,
        creation_disposition: u32,
        flags: u32,
        template: WindowHandle,
    ) -> WindowHandle;
    fn GetConsoleMode(handle: WindowHandle, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: WindowHandle, mode: u32) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn LoadImageA(
        instance: WindowHandle,
        name: *const u8,
        type_: u32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> WindowHandle;
    fn SendMessageA(
        window: WindowHandle,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize;
    fn ShowWindow(window: WindowHandle, command: i32) -> i32;
}

#[link(name = "ucrt")]
unsafe extern "C" {
    fn _open_osfhandle(handle: isize, flags: i32) -> i32;
    fn _close(fd: i32) -> i32;
    fn _get_osfhandle(fd: i32) -> isize;
}

/// Re-enable normal Ctrl-C processing (`os_enable_ctrl_c`).
pub fn os_enable_ctrl_c() {
    unsafe { SetConsoleCtrlHandler(std::ptr::null(), 0) };
}

/// Forget the cached console window (`os_clear_hwnd`).
pub fn os_clear_hwnd() {
    unsafe { *WINDOW.get_mut() = std::ptr::null_mut() };
}

/// Restore the original console icons (`os_icon_reset`).
pub fn os_icon_reset() {
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    let window = unsafe { *WINDOW.get_mut() };
    if window.is_null() {
        return;
    }
    let small = unsafe { *ORIGINAL_ICON_SMALL.get_mut() };
    if !small.is_null() {
        unsafe {
            SendMessageA(
                window,
                WM_SETICON,
                ICON_SMALL,
                small as isize,
            )
        };
    }
    let large = unsafe { *ORIGINAL_ICON.get_mut() };
    if !large.is_null() {
        unsafe {
            SendMessageA(
                window,
                WM_SETICON,
                ICON_BIG,
                large as isize,
            )
        };
    }
}

/// Load Neovim's console icon and preserve the originals
/// (`os_icon_init`).
pub fn os_icon_init() {
    const IMAGE_ICON: u32 = 1;
    const LR_LOADFROMFILE: u32 = 0x0010;
    const LR_LOADMAP3DCOLORS: u32 = 0x1000;
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    let window = unsafe { GetConsoleWindow() };
    unsafe { *WINDOW.get_mut() = window };
    if window.is_null() {
        return;
    }
    let Some(runtime) = crate::os::env::os_getenv(b"VIMRUNTIME") else {
        return;
    };
    let mut path = runtime;
    if !path.ends_with(b"/") && !path.ends_with(b"\\") {
        path.push(b'/');
    }
    path.extend_from_slice(b"neovim.ico");
    let Ok(path_string) = std::ffi::CString::new(path) else {
        return;
    };
    let path_text = String::from_utf8_lossy(path_string.as_bytes());
    if !std::path::Path::new(path_text.as_ref()).exists() {
        return;
    }
    let icon = unsafe {
        LoadImageA(
            std::ptr::null_mut(),
            path_string.as_ptr().cast(),
            IMAGE_ICON,
            64,
            64,
            LR_LOADFROMFILE | LR_LOADMAP3DCOLORS,
        )
    };
    if icon.is_null() {
        return;
    }
    unsafe {
        *ORIGINAL_ICON_SMALL.get_mut() = SendMessageA(
            window,
            WM_SETICON,
            ICON_SMALL,
            icon as isize,
        ) as WindowHandle;
        *ORIGINAL_ICON.get_mut() = SendMessageA(
            window,
            WM_SETICON,
            ICON_BIG,
            icon as isize,
        ) as WindowHandle;
    }
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

fn os_open_conout_handle() -> WindowHandle {
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;
    const INVALID_HANDLE_VALUE: WindowHandle =
        -1isize as WindowHandle;
    let handle = unsafe {
        CreateFileA(
            c"CONOUT$".as_ptr().cast(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(handle, INVALID_HANDLE_VALUE);
    handle
}

/// Replace standard input with `CONIN$`
/// (`os_redirect_stdin_to_conin`).
pub fn os_redirect_stdin_to_conin() {
    unsafe { _close(0) };
    assert_eq!(os_open_conin_fd(), 0);
}

/// Replace standard output and error with `CONOUT$`
/// (`os_redirect_stdout_stderr_to_conout`).
pub fn os_redirect_stdout_stderr_to_conout() {
    let handle = os_open_conout_handle();
    unsafe { _close(1) };
    let output = unsafe { _open_osfhandle(handle as isize, 0) };
    assert_eq!(output, 1);
    unsafe { _close(2) };
    let error = unsafe { _open_osfhandle(handle as isize, 0) };
    assert_eq!(error, 2);
}

/// Re-enable Ctrl-C processing and attach standard I/O to the current
/// console (`os_reattach_console_stdio`).
pub fn os_reattach_console_stdio() {
    os_enable_ctrl_c();
    os_redirect_stdin_to_conin();
    os_redirect_stdout_stderr_to_conout();
}

/// Detach from the current console and use a hidden private console
/// (`os_swap_to_hidden_console`).
pub fn os_swap_to_hidden_console() {
    const SW_HIDE: i32 = 0;
    unsafe {
        FreeConsole();
        AllocConsole();
        ShowWindow(GetConsoleWindow(), SW_HIDE);
    }
    os_reattach_console_stdio();
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

/// Guess the Windows terminal type (`os_tty_guess_term`).
///
/// The original also marks libuv's process-global ConEmu virtual
/// terminal support flag. This crate has no libuv runtime; terminal
/// selection itself is complete.
#[must_use]
pub fn os_tty_guess_term(
    term: Option<&[u8]>,
    output_fd: i32,
) -> Vec<u8> {
    const INVALID_HANDLE_VALUE: isize = -1;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    let conemu_ansi =
        crate::os::env::os_getenv(b"ConEmuANSI").as_deref() == Some(b"ON");
    let handle = unsafe { _get_osfhandle(output_fd) };
    let mut mode = 0u32;
    let virtual_terminal = handle != INVALID_HANDLE_VALUE
        && unsafe { GetConsoleMode(handle as WindowHandle, &mut mode) } != 0
        && unsafe {
            SetConsoleMode(
                handle as WindowHandle,
                mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
            )
        } != 0;

    term.map_or_else(
        || {
            if virtual_terminal {
                b"vtpcon".to_vec()
            } else if conemu_ansi {
                b"conemu".to_vec()
            } else {
                b"win32con".to_vec()
            }
        },
        <[u8]>::to_vec,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ConemuEnvGuard(Option<Vec<u8>>);

    impl ConemuEnvGuard {
        fn set(value: &[u8]) -> Self {
            let previous = crate::os::env::os_getenv(b"ConEmuANSI");
            unsafe {
                crate::os::env::os_setenv(b"ConEmuANSI", value, 1);
            }
            Self(previous)
        }
    }

    impl Drop for ConemuEnvGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.0.as_deref() {
                unsafe {
                    crate::os::env::os_setenv(
                        b"ConEmuANSI",
                        previous,
                        1,
                    )
                };
            } else {
                unsafe { crate::os::env::os_unsetenv(b"ConEmuANSI") };
            }
        }
    }

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
    fn icon_reset_is_a_noop_without_a_cached_window() {
        let _lock = crate::globals::global_state_test_lock();
        let previous = unsafe {
            std::mem::replace(WINDOW.get_mut(), std::ptr::null_mut())
        };
        os_icon_reset();
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

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call console Win32 FFI")]
    fn console_output_can_be_wrapped_when_a_console_is_attached() {
        if unsafe { GetConsoleWindow() }.is_null() {
            return;
        }
        let handle = os_open_conout_handle();
        let descriptor = unsafe { _open_osfhandle(handle as isize, 0) };
        assert!(descriptor >= 0);
        assert_eq!(unsafe { _close(descriptor) }, 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call console Win32 FFI")]
    fn tty_guess_preserves_explicit_term_and_has_a_console_fallback() {
        let _lock = crate::globals::global_state_test_lock();
        let _environment = ConemuEnvGuard::set(b"OFF");

        assert_eq!(
            os_tty_guess_term(Some(b"xterm-256color"), 1),
            b"xterm-256color"
        );
        assert!(matches!(
            os_tty_guess_term(None, 1).as_slice(),
            b"vtpcon" | b"win32con"
        ));
    }
}
