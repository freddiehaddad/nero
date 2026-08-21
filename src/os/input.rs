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
static INPUT_EOF: GlobalCell<bool> = GlobalCell::new(false);
static CURSORHOLD_TIME: GlobalCell<i32> = GlobalCell::new(0);
static CURSORHOLD_TB_CHANGE_CNT: GlobalCell<i32> = GlobalCell::new(0);

#[derive(Clone, Copy, Default)]
struct MultiClickState {
    num_clicks: i32,
    mouse_code: i32,
    grid: i32,
    col: i32,
    row: i32,
    time: u64,
}

static MULTICLICK: GlobalCell<MultiClickState> =
    GlobalCell::new(MultiClickState {
        num_clicks: 0,
        mouse_code: 0,
        grid: 0,
        col: 0,
        row: 0,
        time: 0,
    });

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

/// Whether a file descriptor refers to a terminal (`os_isatty`).
#[must_use]
pub fn os_isatty(fd: i32) -> bool {
    if fd < 0 {
        return false;
    }
    #[cfg(unix)]
    {
        unsafe { libc::isatty(fd) != 0 }
    }
    #[cfg(windows)]
    {
        #[link(name = "ucrt")]
        unsafe extern "C" {
            fn _isatty(fd: i32) -> i32;
        }
        unsafe { _isatty(fd) != 0 }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = fd;
        false
    }
}

/// Whether an optional event queue contains pending work
/// (`pending_events`).
///
/// # Safety
/// A non-null `events` pointer must identify a live `MultiQueue`.
#[must_use]
unsafe fn pending_events(
    events: *const crate::event::multiqueue::MultiQueue,
) -> bool {
    !events.is_null()
        && !unsafe {
            crate::event::multiqueue::multiqueue_empty(events)
        }
}

/// Whether typeahead, raw input, or queued events are pending
/// (`os_input_ready`).
///
/// # Safety
/// A non-null `events` pointer must identify a live `MultiQueue`;
/// reads shared input and global typeahead state.
#[must_use]
pub unsafe fn os_input_ready(
        events: *const crate::event::multiqueue::MultiQueue,
) -> bool {
        (unsafe { (*crate::globals::GLOBALS.as_ptr()).typebuf_was_filled })
            || unsafe { input_available() } != 0
            || unsafe { pending_events(events) }
}

/// Append one stream read into the raw input buffer
/// (`input_read_cb`).
///
/// # Safety
/// Mutates shared raw-input and EOF state.
#[allow(dead_code)]
unsafe fn input_read_cb(data: &[u8], at_eof: bool) -> usize {
    if at_eof {
            unsafe { *INPUT_EOF.get_mut() = true };
    }
    assert!(unsafe { input_space() } >= data.len());
    unsafe { input_enqueue_raw(data) };
    data.len()
}

/// Restart the CursorHold wait for a new typeahead generation
/// (`reset_cursorhold_wait`).
///
/// # Safety
/// Mutates shared CursorHold timing state.
#[allow(dead_code)]
unsafe fn reset_cursorhold_wait(tb_change_cnt: i32) {
    unsafe {
        *CURSORHOLD_TIME.get_mut() = 0;
        *CURSORHOLD_TB_CHANGE_CNT.get_mut() = tb_change_cnt;
    }
}

/// Update multi-click state for one mouse event (`check_multiclick`).
///
/// Returns `(modifier_bits, skip_event)`.
///
/// # Safety
/// Mutates shared mouse-click state and reads `'mousetime'`.
#[allow(dead_code)]
unsafe fn check_multiclick(
    code: i32,
    grid: i32,
    row: i32,
    col: i32,
) -> (u8, bool) {
    if code >= i32::from(crate::keycodes_defs::KE_MOUSEDOWN)
        && code <= i32::from(crate::keycodes_defs::KE_MOUSERIGHT)
    {
        return (0, false);
    }

    let state = unsafe { MULTICLICK.get_mut() };
    let no_move =
        state.grid == grid && state.col == col && state.row == row;
    let (_, is_click, is_drag) = crate::keycodes::get_mouse_button(code);
    if is_drag && no_move {
        return (0, true);
    }

    if is_click {
        let mouse_time = crate::os::time::os_hrtime();
        let timediff = mouse_time.wrapping_sub(state.time);
        let mousetime = (unsafe {
            (*crate::option_vars::OPTION_VARS.as_ptr()).p_mouset
        } as u64)
            .wrapping_mul(1_000_000);
        if code == state.mouse_code
            && no_move
            && timediff < mousetime
            && state.num_clicks != 4
        {
            state.num_clicks += 1;
        } else {
            state.num_clicks = 1;
        }
        state.mouse_code = code;
        state.time = mouse_time;
    } else if !no_move {
        state.mouse_code = code;
    }

    state.grid = grid;
    state.col = col;
    state.row = row;

    let modifiers = if code
        == i32::from(crate::keycodes_defs::KE_MOUSEMOVE)
    {
        0
    } else {
        match state.num_clicks {
            2 => crate::keycodes_defs::MOD_MASK_2CLICK as u8,
            3 => crate::keycodes_defs::MOD_MASK_3CLICK as u8,
            4 => crate::keycodes_defs::MOD_MASK_4CLICK as u8,
            _ => 0,
        }
    };
    (modifiers, false)
}

/// Encode and enqueue one mouse event (`input_enqueue_mouse`).
///
/// # Safety
/// Mutates shared mouse coordinates, multi-click state, and raw input
/// state.
pub unsafe fn input_enqueue_mouse(
    code: i32,
    mut modifier: u8,
    grid: i32,
    row: i32,
    col: i32,
) {
    let (multi_click, skip_event) =
        unsafe { check_multiclick(code, grid, row, col) };
    modifier |= multi_click;
    if skip_event {
        return;
    }

    let mut buf = [0u8; 6];
    let mut pos = 0;
    if modifier != 0 {
        buf[..3].copy_from_slice(&[
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::KS_MODIFIER,
            modifier,
        ]);
        pos = 3;
    }
    buf[pos..pos + 3].copy_from_slice(&[
        crate::keycodes_defs::K_SPECIAL,
        crate::keycodes_defs::KS_EXTRA,
        code as u8,
    ]);

    let globals = crate::globals::GLOBALS.as_ptr();
    unsafe {
        (*globals).mouse_grid = grid;
        (*globals).mouse_row = row;
        (*globals).mouse_col = col;
        input_enqueue_raw(&buf[..pos + 3]);
    }
}

fn parse_decimal(
    input: &[u8],
    mut pos: usize,
) -> Option<(i32, usize)> {
    while input.get(pos).is_some_and(u8::is_ascii_whitespace) {
        pos += 1;
    }
    let negative = input.get(pos) == Some(&b'-');
    if negative || input.get(pos) == Some(&b'+') {
        pos += 1;
    }
    let start = pos;
    let mut value = 0i64;
    while let Some(byte) = input.get(pos)
        && byte.is_ascii_digit()
    {
        value = value
            .checked_mul(10)?
            .checked_add(i64::from(byte - b'0'))?;
        pos += 1;
    }
    if pos == start {
        return None;
    }
    if negative {
        value = -value;
    }
    Some((i32::try_from(value).ok()?, pos))
}

fn parse_mouse_coordinates(input: &[u8]) -> Option<(i32, i32, usize)> {
    if input.first() != Some(&b'<') {
        return None;
    }
    let (col, mut pos) = parse_decimal(input, 1)?;
    if input.get(pos) != Some(&b',') {
        return None;
    }
    pos += 1;
    let (row, pos) = parse_decimal(input, pos)?;
    if input.get(pos) != Some(&b'>') {
        return None;
    }
    Some((col, row, pos + 1))
}

/// Decode coordinates and multi-click modifiers for one translated
/// mouse key (`handle_mouse_event`).
///
/// Returns `(input_bytes_consumed, output_size)`.
///
/// # Safety
/// Mutates shared mouse coordinates and multi-click state.
#[allow(dead_code)]
unsafe fn handle_mouse_event(
    input: &[u8],
    buf: &mut [u8],
    bufsize: usize,
) -> (usize, usize) {
    assert!(bufsize <= buf.len());
    let (mouse_code, key_type) = match bufsize {
        3 => (i32::from(buf[2]), buf[1]),
        6 => (i32::from(buf[5]), buf[4]),
        _ => (0, 0),
    };
    let mouse_key = key_type == crate::keycodes_defs::KS_EXTRA
        && ((mouse_code
            >= i32::from(crate::keycodes_defs::KE_LEFTMOUSE)
            && mouse_code
                <= i32::from(
                    crate::keycodes_defs::KE_RIGHTRELEASE,
                ))
            || (mouse_code
                >= i32::from(crate::keycodes_defs::KE_X1MOUSE)
                && mouse_code
                    <= i32::from(
                        crate::keycodes_defs::KE_X2RELEASE,
                    ))
            || (mouse_code
                >= i32::from(crate::keycodes_defs::KE_MOUSEDOWN)
                && mouse_code
                    <= i32::from(
                        crate::keycodes_defs::KE_MOUSERIGHT,
                    ))
            || mouse_code
                == i32::from(crate::keycodes_defs::KE_MOUSEMOVE));
    if !mouse_key {
        return (0, bufsize);
    }

    let globals = crate::globals::GLOBALS.as_ptr();
    let mut consumed = 0;
    if let Some((mut col, mut row, advance)) =
        parse_mouse_coordinates(input)
    {
        if col >= 0 && row >= 0 {
            let columns = unsafe { (*globals).Columns };
            let rows = unsafe { (*globals).Rows };
            if col >= columns {
                col = columns - 1;
            }
            if row >= rows {
                row = rows - 1;
            }
            unsafe {
                (*globals).mouse_grid = 0;
                (*globals).mouse_row = row;
                (*globals).mouse_col = col;
            }
        }
        consumed = advance;
    }

    let (modifiers, skip_event) = unsafe {
        check_multiclick(
            mouse_code,
            (*globals).mouse_grid,
            (*globals).mouse_row,
            (*globals).mouse_col,
        )
    };
    if skip_event {
        return (consumed, 0);
    }

    let mut output_size = bufsize;
    if modifiers != 0 {
        if buf[1] != crate::keycodes_defs::KS_MODIFIER {
            assert!(buf.len() >= bufsize + 3);
            buf.copy_within(..3, 3);
            buf[0] = crate::keycodes_defs::K_SPECIAL;
            buf[1] = crate::keycodes_defs::KS_MODIFIER;
            buf[2] = modifiers;
            output_size += 3;
        } else {
            buf[2] |= modifiers;
        }
    }
    (consumed, output_size)
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

    struct InputEofGuard(bool);

    impl InputEofGuard {
        unsafe fn reset() -> Self {
            Self(unsafe {
                std::mem::replace(INPUT_EOF.get_mut(), false)
            })
        }
    }

    impl Drop for InputEofGuard {
        fn drop(&mut self) {
            unsafe { *INPUT_EOF.get_mut() = self.0 };
        }
    }

    struct CursorholdGuard {
        time: i32,
        change_count: i32,
    }

    impl CursorholdGuard {
        unsafe fn set(time: i32, change_count: i32) -> Self {
            let previous = Self {
                time: unsafe { *CURSORHOLD_TIME.get_mut() },
                change_count: unsafe {
                    *CURSORHOLD_TB_CHANGE_CNT.get_mut()
                },
            };
            unsafe {
                *CURSORHOLD_TIME.get_mut() = time;
                *CURSORHOLD_TB_CHANGE_CNT.get_mut() = change_count;
            }
            previous
        }
    }

    impl Drop for CursorholdGuard {
        fn drop(&mut self) {
            unsafe {
                *CURSORHOLD_TIME.get_mut() = self.time;
                *CURSORHOLD_TB_CHANGE_CNT.get_mut() =
                    self.change_count;
            }
        }
    }

    struct MultiClickGuard {
        state: MultiClickState,
        mousetime: crate::types_defs::OptInt,
    }

    impl MultiClickGuard {
        unsafe fn set(
            state: MultiClickState,
            mousetime: crate::types_defs::OptInt,
        ) -> Self {
            let previous = Self {
                state: unsafe { *MULTICLICK.get_mut() },
                mousetime: unsafe {
                    (*crate::option_vars::OPTION_VARS.as_ptr())
                        .p_mouset
                },
            };
            unsafe {
                *MULTICLICK.get_mut() = state;
                (*crate::option_vars::OPTION_VARS.as_ptr()).p_mouset =
                    mousetime;
            }
            previous
        }
    }

    impl Drop for MultiClickGuard {
        fn drop(&mut self) {
            unsafe {
                *MULTICLICK.get_mut() = self.state;
                (*crate::option_vars::OPTION_VARS.as_ptr()).p_mouset =
                    self.mousetime;
            }
        }
    }

    struct MouseGlobalsGuard {
        _columns: crate::globals::GlobalFieldGuard<i32>,
        _rows: crate::globals::GlobalFieldGuard<i32>,
        _grid: crate::globals::GlobalFieldGuard<i32>,
        _row: crate::globals::GlobalFieldGuard<i32>,
        _col: crate::globals::GlobalFieldGuard<i32>,
    }

    impl MouseGlobalsGuard {
        unsafe fn set(
            columns: i32,
            rows: i32,
            grid: i32,
            row: i32,
            col: i32,
        ) -> Self {
            Self {
                _columns: unsafe {
                    crate::globals::GlobalFieldGuard::install(
                        |g| &mut g.Columns,
                        columns,
                    )
                },
                _rows: unsafe {
                    crate::globals::GlobalFieldGuard::install(
                        |g| &mut g.Rows,
                        rows,
                    )
                },
                _grid: unsafe {
                    crate::globals::GlobalFieldGuard::install(
                        |g| &mut g.mouse_grid,
                        grid,
                    )
                },
                _row: unsafe {
                    crate::globals::GlobalFieldGuard::install(
                        |g| &mut g.mouse_row,
                        row,
                    )
                },
                _col: unsafe {
                    crate::globals::GlobalFieldGuard::install(
                        |g| &mut g.mouse_col,
                        col,
                    )
                },
            }
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
    #[cfg_attr(miri, ignore = "Miri cannot call isatty FFI")]
    fn os_isatty_rejects_an_invalid_descriptor() {
        assert!(!os_isatty(-1));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call isatty FFI")]
    fn os_isatty_accepts_a_real_descriptor_without_panicking() {
        let _ = os_isatty(1);
    }

    #[test]
    fn pending_events_handles_null_empty_and_nonempty_queues() {
        assert!(!unsafe { pending_events(std::ptr::null()) });

        let queue = crate::event::multiqueue::multiqueue_new(
            None,
            std::ptr::null_mut(),
        );
        assert!(!unsafe { pending_events(queue) });
        unsafe {
            crate::event::multiqueue::multiqueue_put_event(
                queue,
                crate::event::defs::Event::default(),
            );
        }
        assert!(unsafe { pending_events(queue) });
        unsafe { crate::event::multiqueue::multiqueue_free(queue) };
    }

    #[test]
    fn os_input_ready_is_false_without_typeahead_input_or_events() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let _typebuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.typebuf_was_filled,
                false,
            )
        };
        assert!(!unsafe { os_input_ready(std::ptr::null()) });
    }

    #[test]
    fn os_input_ready_short_circuits_for_typeahead() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let _typebuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.typebuf_was_filled,
                true,
            )
        };
        let dangling = std::ptr::dangling();
        assert!(unsafe { os_input_ready(dangling) });
    }

    #[test]
    fn os_input_ready_short_circuits_for_raw_input() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let _typebuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.typebuf_was_filled,
                false,
            )
        };
        unsafe { input_enqueue_raw(b"x") };
        let dangling = std::ptr::dangling();
        assert!(unsafe { os_input_ready(dangling) });
    }

    #[test]
    fn os_input_ready_detects_a_queued_event() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let _typebuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.typebuf_was_filled,
                false,
            )
        };
        let queue = crate::event::multiqueue::multiqueue_new(
            None,
            std::ptr::null_mut(),
        );
        unsafe {
            crate::event::multiqueue::multiqueue_put_event(
                queue,
                crate::event::defs::Event::default(),
            );
        }
        assert!(unsafe { os_input_ready(queue) });
        unsafe { crate::event::multiqueue::multiqueue_free(queue) };
    }

    #[test]
    fn input_read_cb_enqueues_stream_bytes() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let _eof = unsafe { InputEofGuard::reset() };
        assert_eq!(unsafe { input_read_cb(b"abc", false) }, 3);

        let input = unsafe { INPUT_BUFFER.get_mut() };
        assert_eq!(&input.data[..3], b"abc");
        assert_eq!(input.write_pos, 3);
        assert!(!unsafe { *INPUT_EOF.get_mut() });
    }

    #[test]
    fn input_read_cb_records_end_of_file() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let _eof = unsafe { InputEofGuard::reset() };
        assert_eq!(unsafe { input_read_cb(b"", true) }, 0);
        assert!(unsafe { *INPUT_EOF.get_mut() });
    }

    #[test]
    fn reset_cursorhold_wait_clears_time_and_records_change_count() {
        let _lock = crate::globals::global_state_test_lock();
        let _cursorhold = unsafe { CursorholdGuard::set(123, 456) };
        unsafe { reset_cursorhold_wait(789) };
        assert_eq!(unsafe { *CURSORHOLD_TIME.get_mut() }, 0);
        assert_eq!(
            unsafe { *CURSORHOLD_TB_CHANGE_CNT.get_mut() },
            789
        );
    }

    #[test]
    fn check_multiclick_ignores_wheel_events() {
        let _lock = crate::globals::global_state_test_lock();
        let state = MultiClickState {
            num_clicks: 3,
            mouse_code: 17,
            grid: 1,
            col: 2,
            row: 3,
            time: 4,
        };
        let _mouse = unsafe { MultiClickGuard::set(state, 500) };
        assert_eq!(
            unsafe {
                check_multiclick(
                    i32::from(crate::keycodes_defs::KE_MOUSEDOWN),
                    9,
                    8,
                    7,
                )
            },
            (0, false)
        );
        assert_eq!(unsafe { MULTICLICK.get_mut() }.num_clicks, 3);
        assert_eq!(unsafe { MULTICLICK.get_mut() }.grid, 1);
    }

    #[test]
    fn check_multiclick_skips_a_stationary_drag() {
        let _lock = crate::globals::global_state_test_lock();
        let state = MultiClickState {
            grid: 1,
            col: 2,
            row: 3,
            ..MultiClickState::default()
        };
        let _mouse = unsafe { MultiClickGuard::set(state, 500) };
        assert_eq!(
            unsafe {
                check_multiclick(
                    i32::from(crate::keycodes_defs::KE_LEFTDRAG),
                    1,
                    3,
                    2,
                )
            },
            (0, true)
        );
    }

    #[test]
    fn check_multiclick_counts_repeated_clicks_up_to_four() {
        let _lock = crate::globals::global_state_test_lock();
        let code = i32::from(crate::keycodes_defs::KE_LEFTMOUSE);
        let state = MultiClickState {
            num_clicks: 1,
            mouse_code: code,
            grid: 1,
            col: 2,
            row: 3,
            time: crate::os::time::os_hrtime(),
        };
        let _mouse = unsafe { MultiClickGuard::set(state, 10_000) };

        assert_eq!(
            unsafe { check_multiclick(code, 1, 3, 2) }.0,
            crate::keycodes_defs::MOD_MASK_2CLICK as u8
        );
        assert_eq!(
            unsafe { check_multiclick(code, 1, 3, 2) }.0,
            crate::keycodes_defs::MOD_MASK_3CLICK as u8
        );
        assert_eq!(
            unsafe { check_multiclick(code, 1, 3, 2) }.0,
            crate::keycodes_defs::MOD_MASK_4CLICK as u8
        );
        assert_eq!(unsafe { check_multiclick(code, 1, 3, 2) }.0, 0);
        assert_eq!(unsafe { MULTICLICK.get_mut() }.num_clicks, 1);
    }

    #[test]
    fn check_multiclick_resets_when_the_mouse_moves() {
        let _lock = crate::globals::global_state_test_lock();
        let code = i32::from(crate::keycodes_defs::KE_LEFTMOUSE);
        let state = MultiClickState {
            num_clicks: 2,
            mouse_code: code,
            grid: 1,
            col: 2,
            row: 3,
            time: crate::os::time::os_hrtime(),
        };
        let _mouse = unsafe { MultiClickGuard::set(state, 10_000) };
        assert_eq!(unsafe { check_multiclick(code, 1, 3, 4) }, (0, false));
        assert_eq!(unsafe { MULTICLICK.get_mut() }.num_clicks, 1);
        assert_eq!(unsafe { MULTICLICK.get_mut() }.col, 4);
    }

    #[test]
    fn input_enqueue_mouse_writes_a_plain_mouse_sequence() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let _mouse = unsafe {
            MultiClickGuard::set(MultiClickState::default(), 500)
        };
        let _grid = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.mouse_grid,
                -1,
            )
        };
        let _row = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.mouse_row,
                -1,
            )
        };
        let _col = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.mouse_col,
                -1,
            )
        };

        unsafe {
            input_enqueue_mouse(
                i32::from(crate::keycodes_defs::KE_LEFTMOUSE),
                0,
                2,
                3,
                4,
            )
        };

        let input = unsafe { INPUT_BUFFER.get_mut() };
        assert_eq!(
            &input.data[..3],
            &[
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_EXTRA,
                crate::keycodes_defs::KE_LEFTMOUSE,
            ]
        );
        let globals = crate::globals::GLOBALS.as_ptr();
        assert_eq!(unsafe { (*globals).mouse_grid }, 2);
        assert_eq!(unsafe { (*globals).mouse_row }, 3);
        assert_eq!(unsafe { (*globals).mouse_col }, 4);
    }

    #[test]
    fn input_enqueue_mouse_prefixes_explicit_modifiers() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let _mouse = unsafe {
            MultiClickGuard::set(MultiClickState::default(), 500)
        };
        let _grid = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.mouse_grid,
                -1,
            )
        };
        let _row = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.mouse_row,
                -1,
            )
        };
        let _col = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.mouse_col,
                -1,
            )
        };
        let modifier = crate::keycodes_defs::MOD_MASK_CTRL as u8;
        unsafe {
            input_enqueue_mouse(
                i32::from(crate::keycodes_defs::KE_MOUSEDOWN),
                modifier,
                0,
                0,
                0,
            )
        };

        let input = unsafe { INPUT_BUFFER.get_mut() };
        assert_eq!(
            &input.data[..6],
            &[
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_MODIFIER,
                modifier,
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_EXTRA,
                crate::keycodes_defs::KE_MOUSEDOWN,
            ]
        );
    }

    #[test]
    fn input_enqueue_mouse_drops_a_stationary_drag() {
        let _lock = crate::globals::global_state_test_lock();
        let _buffer = unsafe { InputBufferGuard::reset() };
        let state = MultiClickState {
            grid: 1,
            row: 2,
            col: 3,
            ..MultiClickState::default()
        };
        let _mouse = unsafe { MultiClickGuard::set(state, 500) };
        unsafe {
            input_enqueue_mouse(
                i32::from(crate::keycodes_defs::KE_LEFTDRAG),
                0,
                1,
                2,
                3,
            )
        };
        assert_eq!(unsafe { input_available() }, 0);
    }

    #[test]
    fn parse_mouse_coordinates_accepts_signed_decimal_values() {
        assert_eq!(
            parse_mouse_coordinates(b"<12,-3>rest"),
            Some((12, -3, 7))
        );
        assert_eq!(
            parse_mouse_coordinates(b"< 4,+5>"),
            Some((4, 5, 7))
        );
        assert_eq!(parse_mouse_coordinates(b"12,3"), None);
        assert_eq!(parse_mouse_coordinates(b"<1,2"), None);
    }

    #[test]
    fn handle_mouse_event_ignores_nonmouse_keys() {
        let mut buf = [
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::KS_EXTRA,
            crate::keycodes_defs::KE_EVENT,
            0,
            0,
            0,
        ];
        assert_eq!(
            unsafe { handle_mouse_event(b"<1,2>", &mut buf, 3) },
            (0, 3)
        );
    }

    #[test]
    fn handle_mouse_event_parses_and_clamps_coordinates() {
        let _lock = crate::globals::global_state_test_lock();
        let _globals =
            unsafe { MouseGlobalsGuard::set(10, 5, 7, 1, 2) };
        let _mouse = unsafe {
            MultiClickGuard::set(MultiClickState::default(), 500)
        };
        let mut buf = [
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::KS_EXTRA,
            crate::keycodes_defs::KE_LEFTMOUSE,
            0,
            0,
            0,
        ];
        assert_eq!(
            unsafe { handle_mouse_event(b"<20,9>rest", &mut buf, 3) },
            (6, 3)
        );
        let globals = crate::globals::GLOBALS.as_ptr();
        assert_eq!(unsafe { (*globals).mouse_grid }, 0);
        assert_eq!(unsafe { (*globals).mouse_col }, 9);
        assert_eq!(unsafe { (*globals).mouse_row }, 4);
    }

    #[test]
    fn handle_mouse_event_inserts_multiclick_modifier() {
        let _lock = crate::globals::global_state_test_lock();
        let _globals =
            unsafe { MouseGlobalsGuard::set(80, 24, 1, 2, 3) };
        let code = i32::from(crate::keycodes_defs::KE_LEFTMOUSE);
        let state = MultiClickState {
            num_clicks: 1,
            mouse_code: code,
            grid: 1,
            row: 2,
            col: 3,
            time: crate::os::time::os_hrtime(),
        };
        let _mouse = unsafe { MultiClickGuard::set(state, 10_000) };
        let mut buf = [
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::KS_EXTRA,
            crate::keycodes_defs::KE_LEFTMOUSE,
            0,
            0,
            0,
        ];

        assert_eq!(
            unsafe { handle_mouse_event(b"", &mut buf, 3) },
            (0, 6)
        );
        assert_eq!(
            buf,
            [
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_MODIFIER,
                crate::keycodes_defs::MOD_MASK_2CLICK as u8,
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_EXTRA,
                crate::keycodes_defs::KE_LEFTMOUSE,
            ]
        );
    }

    #[test]
    fn handle_mouse_event_drops_stationary_drags() {
        let _lock = crate::globals::global_state_test_lock();
        let _globals =
            unsafe { MouseGlobalsGuard::set(80, 24, 1, 2, 3) };
        let state = MultiClickState {
            grid: 1,
            row: 2,
            col: 3,
            ..MultiClickState::default()
        };
        let _mouse = unsafe { MultiClickGuard::set(state, 500) };
        let mut buf = [
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::KS_EXTRA,
            crate::keycodes_defs::KE_LEFTDRAG,
            0,
            0,
            0,
        ];
        assert_eq!(
            unsafe { handle_mouse_event(b"", &mut buf, 3) },
            (0, 0)
        );
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
