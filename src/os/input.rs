//! Translated from `src/nvim/os/input.c` (blocking and raw-buffer core).
//!
//! The event-loop polling, mouse decoding, and stream
//! callbacks remain deferred. [`input_blocking`] is independently
//! complete and is needed by `nvim_get_mode`; [`input_available`]/
//! [`input_enqueue_raw`] expose the dependency-free raw-buffer state.

use crate::globals::GlobalCell;

/// Whether the main loop is waiting for input (`blocking`).
static BLOCKING: GlobalCell<bool> = GlobalCell::new(false);

const READ_BUFFER_SIZE: usize = 0x0fff;
const MAX_KEY_CODE_LEN: usize = 6;
const INPUT_BUFFER_SIZE: usize =
    READ_BUFFER_SIZE * 4 + MAX_KEY_CODE_LEN;

#[derive(Clone, Copy)]
struct InputBuffer {
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

/// Append bytes to the raw input buffer (`input_enqueue_raw`).
///
/// Consumed prefix space is reclaimed first; data beyond the fixed
/// buffer capacity is silently dropped, matching the original.
///
/// # Safety
/// Mutates shared raw-input state.
pub unsafe fn input_enqueue_raw(data: &[u8]) {
    let input = unsafe { INPUT_BUFFER.get_mut() };
    if input.read_pos > 0 {
        let available = input.write_pos - input.read_pos;
        input
            .data
            .copy_within(input.read_pos..input.write_pos, 0);
        input.read_pos = 0;
        input.write_pos = available;
    }

    let to_write =
        data.len().min(INPUT_BUFFER_SIZE - input.write_pos);
    input.data[input.write_pos..input.write_pos + to_write]
        .copy_from_slice(&data[..to_write]);
    input.write_pos += to_write;
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
    fn input_enqueue_raw_appends_bytes() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        unsafe { input_enqueue_raw(b"abc") };

        let input = unsafe { INPUT_BUFFER.get_mut() };
        assert_eq!(input.read_pos, 0);
        assert_eq!(input.write_pos, 3);
        assert_eq!(&input.data[..3], b"abc");
    }

    #[test]
    fn input_enqueue_raw_reclaims_consumed_prefix_space() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        {
            let input = unsafe { INPUT_BUFFER.get_mut() };
            input.data[..6].copy_from_slice(b"abcdef");
            input.read_pos = 3;
            input.write_pos = 6;
        }

        unsafe { input_enqueue_raw(b"gh") };

        let input = unsafe { INPUT_BUFFER.get_mut() };
        assert_eq!(input.read_pos, 0);
        assert_eq!(input.write_pos, 5);
        assert_eq!(&input.data[..5], b"defgh");
    }

    #[test]
    fn input_enqueue_raw_truncates_to_fixed_capacity() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        {
            let input = unsafe { INPUT_BUFFER.get_mut() };
            input.write_pos = INPUT_BUFFER_SIZE - 2;
        }

        unsafe { input_enqueue_raw(b"abcd") };

        let input = unsafe { INPUT_BUFFER.get_mut() };
        assert_eq!(input.write_pos, INPUT_BUFFER_SIZE);
        assert_eq!(&input.data[INPUT_BUFFER_SIZE - 2..], b"ab");
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
