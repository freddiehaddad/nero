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
static EVENT_KEY_INDEX: GlobalCell<usize> = GlobalCell::new(0);

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

/// Emit the `KE_EVENT` key sequence, continuing across calls when the
/// caller's buffer is shorter than three bytes (`push_event_key`).
///
/// # Safety
/// Mutates shared event-key cursor state.
pub unsafe fn push_event_key(buf: &mut [u8]) -> usize {
    assert!(!buf.is_empty());
    const KEY: [u8; 3] = [
        crate::keycodes_defs::K_SPECIAL,
        crate::keycodes_defs::KS_EXTRA,
        crate::keycodes_defs::KE_EVENT,
    ];
    let key_index = unsafe { EVENT_KEY_INDEX.get_mut() };
    let mut written = 0;
    loop {
        buf[written] = KEY[*key_index];
        written += 1;
        *key_index = (*key_index + 1) % KEY.len();
        if *key_index == 0 || written == buf.len() {
            break;
        }
    }
    written
}

/// Detect the newest Ctrl-C in buffered input (`process_ctrl_c`).
///
/// Earlier unread bytes are discarded by advancing the read cursor to
/// the interrupt byte, matching Neovim's typeahead handling.
///
/// # Safety
/// Mutates shared raw-input and global interrupt state.
#[allow(dead_code)]
unsafe fn process_ctrl_c() {
    let globals = crate::globals::GLOBALS.as_ptr();
    if !unsafe { (*globals).ctrl_c_interrupts } {
        return;
    }

    let input = unsafe { INPUT_BUFFER.get_mut() };
    let available = input.write_pos - input.read_pos;
    let mut found = None;
    for index in (0..available).rev() {
        let absolute = input.read_pos + index;
        let byte = input.data[absolute];
        let modified_ctrl_c = byte == b'C'
            && index >= 3
            && input.data[absolute - 3]
                == crate::keycodes_defs::K_SPECIAL
            && input.data[absolute - 2]
                == crate::keycodes_defs::KS_MODIFIER
            && input.data[absolute - 1]
                == crate::keycodes_defs::MOD_MASK_CTRL as u8;
        if byte == crate::ascii_defs::CTRL_C || modified_ctrl_c {
            input.data[absolute] = crate::ascii_defs::CTRL_C;
            unsafe { (*globals).got_int = true };
            found = Some(index);
            break;
        }
    }

    if unsafe { (*globals).got_int }
        && let Some(index @ 1..) = found
    {
        input.read_pos += index;
    }
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

    struct EventKeyIndexGuard(usize);

    impl EventKeyIndexGuard {
        unsafe fn reset() -> Self {
            Self(unsafe {
                std::mem::replace(EVENT_KEY_INDEX.get_mut(), 0)
            })
        }
    }

    impl Drop for EventKeyIndexGuard {
        fn drop(&mut self) {
            unsafe { *EVENT_KEY_INDEX.get_mut() = self.0 };
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
    fn push_event_key_emits_the_full_sequence_when_it_fits() {
        let _lock = crate::globals::global_state_test_lock();
        let _index = unsafe { EventKeyIndexGuard::reset() };
        let mut buf = [0u8; 3];
        assert_eq!(unsafe { push_event_key(&mut buf) }, 3);
        assert_eq!(
            buf,
            [
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_EXTRA,
                crate::keycodes_defs::KE_EVENT,
            ]
        );
        assert_eq!(unsafe { *EVENT_KEY_INDEX.get_mut() }, 0);
    }

    #[test]
    fn push_event_key_continues_a_partial_sequence_across_calls() {
        let _lock = crate::globals::global_state_test_lock();
        let _index = unsafe { EventKeyIndexGuard::reset() };
        let mut first = [0u8; 2];
        let mut second = [0u8; 2];

        assert_eq!(unsafe { push_event_key(&mut first) }, 2);
        assert_eq!(
            first,
            [
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_EXTRA,
            ]
        );
        assert_eq!(unsafe { push_event_key(&mut second) }, 1);
        assert_eq!(second[0], crate::keycodes_defs::KE_EVENT);
        assert_eq!(unsafe { *EVENT_KEY_INDEX.get_mut() }, 0);
    }

    #[test]
    #[should_panic]
    fn push_event_key_requires_nonempty_output() {
        let _lock = crate::globals::global_state_test_lock();
        let _index = unsafe { EventKeyIndexGuard::reset() };
        let _ = unsafe { push_event_key(&mut []) };
    }

    #[test]
    fn process_ctrl_c_keeps_the_newest_interrupt_and_discards_prefix() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let _interrupts = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.ctrl_c_interrupts,
                true,
            )
        };
        let _got_int = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.got_int,
                false,
            )
        };
        unsafe {
            input_enqueue_raw(&[
                crate::ascii_defs::CTRL_C,
                b'x',
                crate::ascii_defs::CTRL_C,
                b'y',
            ]);
            process_ctrl_c();
        }

        let input = unsafe { INPUT_BUFFER.get_mut() };
        assert_eq!(input.read_pos, 2);
        assert_eq!(input.data[input.read_pos], crate::ascii_defs::CTRL_C);
        assert!(unsafe { (*crate::globals::GLOBALS.as_ptr()).got_int });
    }

    #[test]
    fn process_ctrl_c_recognizes_a_modified_ctrl_c_sequence() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let _interrupts = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.ctrl_c_interrupts,
                true,
            )
        };
        let _got_int = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.got_int,
                false,
            )
        };
        unsafe {
            input_enqueue_raw(&[
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_MODIFIER,
                crate::keycodes_defs::MOD_MASK_CTRL as u8,
                b'C',
            ]);
            process_ctrl_c();
        }

        let input = unsafe { INPUT_BUFFER.get_mut() };
        assert_eq!(input.read_pos, 3);
        assert_eq!(input.data[3], crate::ascii_defs::CTRL_C);
        assert!(unsafe { (*crate::globals::GLOBALS.as_ptr()).got_int });
    }

    #[test]
    fn process_ctrl_c_respects_the_interrupts_disabled_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let _interrupts = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.ctrl_c_interrupts,
                false,
            )
        };
        let _got_int = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.got_int,
                false,
            )
        };
        unsafe {
            input_enqueue_raw(&[b'a', crate::ascii_defs::CTRL_C]);
            process_ctrl_c();
        }

        let input = unsafe { INPUT_BUFFER.get_mut() };
        assert_eq!(input.read_pos, 0);
        assert!(!unsafe { (*crate::globals::GLOBALS.as_ptr()).got_int });
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
