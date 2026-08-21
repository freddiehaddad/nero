//! Translated from `src/nvim/os/input.c` (blocking and raw-buffer core).
//!
//! The event-loop polling, mouse decoding, and stream
//! callbacks remain deferred. [`input_blocking`] is independently
//! complete and is needed by `nvim_get_mode`; [`input_available`]
//! exposes the dependency-free raw-buffer state.

use crate::globals::GlobalCell;

/// Whether the main loop is waiting for input (`blocking`).
static BLOCKING: GlobalCell<bool> = GlobalCell::new(false);

const READ_BUFFER_SIZE: usize = 0x0fff;
const MAX_KEY_CODE_LEN: usize = 6;
const INPUT_BUFFER_SIZE: usize =
    READ_BUFFER_SIZE * 4 + MAX_KEY_CODE_LEN;

#[derive(Clone, Copy)]
struct InputBuffer {
    #[allow(dead_code)]
    data: [u8; INPUT_BUFFER_SIZE],
    read_pos: usize,
    write_pos: usize,
}

impl InputBuffer {
    const fn new() -> Self {
        Self {
            data: [0; INPUT_BUFFER_SIZE],
            read_pos: 0,
            write_pos: 0,
        }
    }
}

static INPUT_BUFFER: GlobalCell<InputBuffer> =
    GlobalCell::new(InputBuffer::new());

/// Number of unread bytes in the raw input buffer
/// (`input_available`).
///
/// # Safety
/// Reads shared raw-input state.
#[must_use]
pub unsafe fn input_available() -> usize {
    let input = unsafe { INPUT_BUFFER.get_mut() };
    input.write_pos - input.read_pos
}

/// Remaining writable capacity at the tail of the raw input buffer
/// (`input_space`).
///
/// # Safety
/// Reads shared raw-input state.
#[allow(dead_code)]
#[must_use]
unsafe fn input_space() -> usize {
    let input = unsafe { INPUT_BUFFER.get_mut() };
    INPUT_BUFFER_SIZE - input.write_pos
}

/// Whether the main loop is blocked waiting for input
/// (`input_blocking`).
///
/// # Safety
/// Reads the shared `blocking` file-static.
#[must_use]
pub unsafe fn input_blocking() -> bool {
    unsafe { *BLOCKING.get_mut() }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct InputBufferGuard(InputBuffer);

    impl InputBufferGuard {
        unsafe fn reset() -> Self {
            Self(unsafe {
                std::mem::replace(
                    INPUT_BUFFER.get_mut(),
                    InputBuffer::new(),
                )
            })
        }
    }

    impl Drop for InputBufferGuard {
        fn drop(&mut self) {
            unsafe { *INPUT_BUFFER.get_mut() = self.0 };
        }
    }

    #[test]
    fn input_available_reports_unread_bytes() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        assert_eq!(unsafe { input_available() }, 0);

        {
            let input = unsafe { INPUT_BUFFER.get_mut() };
            input.read_pos = 3;
            input.write_pos = 9;
        }
        assert_eq!(unsafe { input_available() }, 6);
    }

    #[test]
    fn input_space_reports_tail_capacity() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        assert_eq!(unsafe { input_space() }, INPUT_BUFFER_SIZE);

        {
            let input = unsafe { INPUT_BUFFER.get_mut() };
            input.write_pos = 100;
        }
        assert_eq!(unsafe { input_space() }, INPUT_BUFFER_SIZE - 100);
    }

    #[test]
    fn input_blocking_reflects_the_file_static() {
        let _lock = crate::globals::global_state_test_lock();
        let previous = unsafe { *BLOCKING.get_mut() };
        unsafe { *BLOCKING.get_mut() = false };
        let unblocked = unsafe { input_blocking() };
        unsafe { *BLOCKING.get_mut() = true };
        let blocked = unsafe { input_blocking() };
        unsafe { *BLOCKING.get_mut() = previous };
        assert!(!unblocked);
        assert!(blocked);
    }
}
