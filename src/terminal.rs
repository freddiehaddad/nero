//! Translated from `src/nvim/terminal.c` (tractable core only).
//!
//! `terminal.c` (~2900 lines) implements the `:terminal` buffer: a
//! libvterm screen driven by a PTY, wired into neovim's buffer,
//! window, redraw and event-loop machinery. Nearly all of it depends
//! on the `Terminal`/`VTerm*` types (libvterm is a C library this
//! crate does not bind), the event loop, or the channel layer - none
//! translated.
//!
//! Translated: [`is_filter_char`] - a pure classification of one
//! character against the `'termpastefilter'` option flags, needing
//! only [`crate::option_vars`]'s already-real `tpf_flags` and
//! `opt_tpf_flag` constants, with no dependency on any terminal
//! state at all; [`terminal_buf`] - the terminal's owning buffer
//! handle; [`terminal_running`] - whether the terminal is still open;
//! [`terminal_suspended`] - whether the child process is suspended;
//! `row_to_linenr`/`linenr_to_row` - terminal-row/buffer-line
//! conversion; `is_focused` - current terminal focus detection.
//! `convert_modifiers` translates Neovim's key modifier state and
//! modifier-encoded key codes to libvterm's modifier mask;
//! `terminal_check_cursor` follows the terminal cursor and viewport;
//! `get_rgb` converts a libvterm color to Neovim's packed RGB value;
//! [`terminal_set_streamed_paste`] transitions bracketed paste state;
//! `terminal_focus` emits libvterm focus reports.
//!
//! Deferred: everything else - the terminal lifecycle
//! (`terminal_open`/`terminal_close`/`terminal_destroy`), input and
//! output (`terminal_send`/`terminal_receive`/`terminal_paste`), the
//! libvterm screen callbacks, and the redraw/cursor integration.

use crate::option_vars::opt_tpf_flag;
use crate::types_defs::{HandleT, TerminalT};
use crate::vterm_defs::VTermModifier;

/// Returns the handle of the buffer that owns `term` (`terminal_buf`).
#[must_use]
pub fn terminal_buf(term: &TerminalT) -> HandleT {
    term.buf_handle
}

/// Whether the terminal's child process is still running
/// (`terminal_running`).
#[must_use]
pub fn terminal_running(term: &TerminalT) -> bool {
    !term.closed
}

/// Whether the terminal's child process is suspended
/// (`terminal_suspended`).
#[must_use]
pub fn terminal_suspended(term: &TerminalT) -> bool {
    term.suspended
}

#[must_use]
pub fn terminal_set_streamed_paste(
    term: &mut TerminalT,
    mode: crate::vterm_defs::VTermKeyboardMode,
    streamed: bool,
) -> Vec<u8> {
    let output = if term.streamed_paste == streamed {
        Vec::new()
    } else if streamed {
        crate::vterm::keyboard::vterm_keyboard_start_paste(mode)
    } else {
        crate::vterm::keyboard::vterm_keyboard_end_paste(mode)
    };
    term.streamed_paste = streamed;
    output
}

/// Converts a terminal screen row to its buffer line number
/// (`row_to_linenr`).
#[allow(dead_code)]
#[must_use]
fn row_to_linenr(term: &TerminalT, row: i32) -> i32 {
    if row == i32::MAX {
        i32::MAX
    } else {
        row.wrapping_add(term.sb_current as i32).wrapping_add(1)
    }
}

/// Converts a buffer line number to its terminal screen row
/// (`linenr_to_row`).
#[allow(dead_code)]
#[must_use]
fn linenr_to_row(term: &TerminalT, linenr: i32) -> i32 {
    linenr.wrapping_sub(term.sb_current as i32).wrapping_sub(1)
}

#[allow(dead_code)]
#[must_use]
fn get_rgb(
    state: &crate::vterm::state::VTermState,
    mut color: crate::vterm_defs::VTermColor,
) -> i32 {
    state.convert_color_to_rgb(&mut color);
    crate::macros_defs::rgb_(
        u32::from(color.red),
        u32::from(color.green),
        u32::from(color.blue),
    ) as i32
}

#[allow(dead_code)]
#[must_use]
fn terminal_focus(
    state: &crate::vterm::state::VTermState,
    focus: bool,
) -> Vec<u8> {
    if focus {
        crate::vterm::state::vterm_state_focus_in(state, state.ctrl8bit)
    } else {
        crate::vterm::state::vterm_state_focus_out(state, state.ctrl8bit)
    }
}

/// Whether `term` is the terminal currently focused in Terminal mode
/// (`is_focused`).
///
/// # Safety
/// `GLOBALS.curbuf` must point to a live buffer, and global editor
/// state must not be mutated concurrently.
#[allow(dead_code)]
#[must_use]
unsafe fn is_focused(term: *const TerminalT) -> bool {
    // SAFETY: forwarded from this function's own safety contract.
    let g = unsafe { &*crate::globals::GLOBALS.as_ptr() };
    g.State & crate::state_defs::mode::TERMINAL as i32 != 0
        // SAFETY: `curbuf` is live by the caller's contract.
        && unsafe { (*g.curbuf).terminal == term.cast_mut() }
}

/// Converts Neovim key modifiers to libvterm modifiers
/// (`convert_modifiers`).
///
/// # Safety
/// Reads `GLOBALS.mod_mask`; global editor state must not be mutated
/// concurrently.
#[allow(dead_code)]
fn convert_modifiers(key: &mut i32, state: &mut VTermModifier) {
    use crate::keycodes_defs::{
        K_C_END, K_C_HOME, K_C_LEFT, K_C_RIGHT, K_S_DOWN, K_S_END, K_S_F1, K_S_F10, K_S_F11,
        K_S_F12, K_S_F2, K_S_F3, K_S_F4, K_S_F5, K_S_F6, K_S_F7, K_S_F8, K_S_F9, K_S_HOME,
        K_S_LEFT, K_S_RIGHT, K_S_TAB, K_S_UP, MOD_MASK_ALT, MOD_MASK_CTRL, MOD_MASK_SHIFT,
    };
    use crate::vterm_defs::{VTERM_MOD_ALT, VTERM_MOD_CTRL, VTERM_MOD_SHIFT};

    // SAFETY: callers uphold this function's global-state contract.
    let mod_mask = unsafe { (*crate::globals::GLOBALS.as_ptr()).mod_mask };
    if mod_mask & i32::from(MOD_MASK_SHIFT) != 0 {
        *state |= VTERM_MOD_SHIFT;
    }
    if mod_mask & i32::from(MOD_MASK_CTRL) != 0 {
        *state |= VTERM_MOD_CTRL;
        if mod_mask & i32::from(MOD_MASK_SHIFT) == 0 && (*key >= 'A' as i32 && *key <= 'Z' as i32)
        {
            *key += 'a' as i32 - 'A' as i32;
        }
    }
    if mod_mask & i32::from(MOD_MASK_ALT) != 0 {
        *state |= VTERM_MOD_ALT;
    }

    match *key {
        K_S_TAB | K_S_UP | K_S_DOWN | K_S_LEFT | K_S_RIGHT | K_S_HOME | K_S_END | K_S_F1
        | K_S_F2 | K_S_F3 | K_S_F4 | K_S_F5 | K_S_F6 | K_S_F7 | K_S_F8 | K_S_F9 | K_S_F10
        | K_S_F11 | K_S_F12 => *state |= VTERM_MOD_SHIFT,
        K_C_LEFT | K_C_RIGHT | K_C_HOME | K_C_END => *state |= VTERM_MOD_CTRL,
        _ => {}
    }
}

/// Keeps the current window cursor and topline synchronized with its
/// terminal (`terminal_check_cursor`).
///
/// # Safety
/// `GLOBALS.curbuf` and `GLOBALS.curwin` must point to a live buffer
/// and window. The buffer must own a live `TerminalT`, the window must
/// display that buffer, and global editor state must not be mutated
/// concurrently.
#[allow(dead_code)]
unsafe fn terminal_check_cursor() {
    // Keep raw pointers across calls that independently reborrow the
    // same globals; create each reference only for its immediate use.
    let g = crate::globals::GLOBALS.as_ptr();
    // SAFETY: forwarded from this function's own safety contract.
    let curbuf = unsafe { (*g).curbuf };
    // SAFETY: forwarded from this function's own safety contract.
    let curwin = unsafe { (*g).curwin };
    // SAFETY: forwarded from this function's own safety contract.
    let term = unsafe { (*curbuf).terminal };
    // SAFETY: all three pointers are live by the caller's contract.
    let line_count = unsafe { (*curbuf).b_ml.ml_line_count };
    // SAFETY: `term` is live by the caller's contract.
    let cursor_row = unsafe { (*term).cursor.row };
    // SAFETY: all pointers are live by the caller's contract.
    unsafe {
        (*curwin).w_cursor.lnum =
            line_count.min(row_to_linenr(&*term, cursor_row));
    }

    // SAFETY: `curwin` is live by the caller's contract.
    let view_height = unsafe { (*curwin).w_view_height };
    let topline = line_count.wrapping_sub(view_height).wrapping_add(1).max(1);
    // SAFETY: `curwin` is live by the caller's contract.
    if topline != unsafe { (*curwin).w_topline } {
        // SAFETY: forwarded from this function's own safety contract.
        unsafe { crate::r#move::set_topline(curwin, topline) };
    }

    // SAFETY: `term` and global state are live by the caller's contract.
    if unsafe { (*term).suspended }
        && unsafe { (*g).State } & crate::state_defs::mode::TERMINAL as i32 != 0
    {
        // SAFETY: `curwin` is live by the caller's contract.
        unsafe {
            (*curwin).w_cursor = crate::pos_defs::PosT {
                lnum: line_count,
                ..Default::default()
            };
        }
    } else {
        // SAFETY: `curwin`, `term`, and global state are live.
        let off = if unsafe { (*g).State } & crate::state_defs::mode::TERMINAL as i32 != 0 {
            0
        } else if unsafe { (*curwin).w_onebuf_opt.wo_rl } != 0 {
            1
        } else {
            -1
        };
        // SAFETY: `term` is live.
        let col = unsafe { (*term).cursor.col }.wrapping_add(off).max(0);
        // SAFETY: forwarded from this function's own safety contract.
        let _ = unsafe { crate::cursor::coladvance(curwin, col) };
    }
}

/// Whether character `c` should be filtered out of a terminal paste,
/// according to the `'termpastefilter'` option (`is_filter_char`).
///
/// Carriage return (`0x0D`) and line feed (`0x0A`) are never
/// filtered: they map to no flag at all, so the final test is against
/// a zero mask and always fails. That is the original's behaviour and
/// is preserved deliberately - a paste must keep its line structure.
///
/// # Safety
/// Reads `crate::option_vars::OPTION_VARS` - the same requirement as
/// every other function that does so.
#[must_use]
pub unsafe fn is_filter_char(c: i32) -> bool {
    let flag: u32 = match c {
        0x08 => opt_tpf_flag::BS,
        0x09 => opt_tpf_flag::HT,
        // Line feed and carriage return are never filtered.
        0x0A | 0x0D => 0,
        0x0C => opt_tpf_flag::FF,
        0x1b => opt_tpf_flag::ESC,
        0x7F => opt_tpf_flag::DEL,
        _ => {
            if c < b' '.into() {
                opt_tpf_flag::C0
            } else if (0x80..=0x9F).contains(&c) {
                opt_tpf_flag::C1
            } else {
                0
            }
        }
    };
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tpf_flags & flag) != 0
}

/// Convert libvterm underline style to Neovim highlight flags
/// (`get_underline_hl_flag`).
#[must_use]
#[allow(dead_code)]
fn get_underline_hl_flag(
    attrs: crate::vterm_defs::VTermScreenCellAttrs,
) -> u32 {
    match attrs.underline {
        crate::vterm_defs::VTERM_UNDERLINE_OFF => 0,
        crate::vterm_defs::VTERM_UNDERLINE_SINGLE => {
            crate::highlight_defs::HL_UNDERLINE
        }
        crate::vterm_defs::VTERM_UNDERLINE_DOUBLE => {
            crate::highlight_defs::HL_UNDERDOUBLE
        }
        crate::vterm_defs::VTERM_UNDERLINE_CURLY => {
            crate::highlight_defs::HL_UNDERCURL
        }
        _ => crate::highlight_defs::HL_UNDERLINE,
    }
}

/// Reports whether the current background theme is dark
/// (`term_theme`) and returns the libvterm callback success value.
///
/// # Safety
/// Reads `OPTION_VARS.p_bg`.
#[must_use]
pub unsafe fn term_theme() -> (bool, i32) {
    let dark = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_bg
        .as_deref()
        .and_then(|value| value.first())
        == Some(&b'd');
    (dark, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_rgb_packs_direct_and_palette_colors() {
        let mut state = crate::vterm::state::VTermState::new(1, 1);
        let direct = crate::vterm_defs::VTermColor {
            color_type: crate::vterm_defs::VTERM_COLOR_RGB,
            red: 0x12,
            green: 0x34,
            blue: 0x56,
            index: 0,
        };
        assert_eq!(get_rgb(&state, direct), 0x0012_3456);

        state.colors[3] = crate::vterm_defs::VTermColor {
            color_type: crate::vterm_defs::VTERM_COLOR_RGB,
            red: 0xAA,
            green: 0xBB,
            blue: 0xCC,
            index: 0,
        };
        let indexed = crate::vterm_defs::VTermColor {
            color_type: crate::vterm_defs::VTERM_COLOR_INDEXED,
            index: 3,
            ..Default::default()
        };
        assert_eq!(get_rgb(&state, indexed), 0x00AA_BBCC);
    }

    #[test]
    fn streamed_paste_emits_markers_only_when_state_changes() {
        let mut term = TerminalT::default();
        let mode = crate::vterm_defs::VTermKeyboardMode {
            bracketpaste: true,
            ..Default::default()
        };
        assert_eq!(
            terminal_set_streamed_paste(&mut term, mode, true),
            b"\x1b[200~"
        );
        assert!(term.streamed_paste);
        assert!(terminal_set_streamed_paste(&mut term, mode, true).is_empty());
        assert_eq!(
            terminal_set_streamed_paste(&mut term, mode, false),
            b"\x1b[201~"
        );
        assert!(!term.streamed_paste);
    }

    #[test]
    fn terminal_focus_emits_reports_when_enabled() {
        let mut state = crate::vterm::state::VTermState::new(1, 1);
        assert!(terminal_focus(&state, true).is_empty());
        state.mode.report_focus = true;
        assert_eq!(terminal_focus(&state, true), b"\x1b[I");
        assert_eq!(terminal_focus(&state, false), b"\x1b[O");
        state.ctrl8bit = true;
        assert_eq!(
            terminal_focus(&state, true),
            [crate::vterm_defs::C1_CSI, b'I']
        );
    }

    struct ModMaskGuard {
        old: i32,
    }

    impl ModMaskGuard {
        fn set(value: i32) -> Self {
            // SAFETY: tests serialize all global-state access.
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let old = std::mem::replace(&mut g.mod_mask, value);
            Self { old }
        }
    }

    impl Drop for ModMaskGuard {
        fn drop(&mut self) {
            // SAFETY: the test lock remains held while this guard drops.
            unsafe { crate::globals::GLOBALS.get_mut() }.mod_mask = self.old;
        }
    }

    #[test]
    fn convert_modifiers_maps_global_modifier_bits() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ModMaskGuard::set(i32::from(
            crate::keycodes_defs::MOD_MASK_SHIFT
                | crate::keycodes_defs::MOD_MASK_CTRL
                | crate::keycodes_defs::MOD_MASK_ALT,
        ));
        let mut key = 'A' as i32;
        let mut state = crate::vterm_defs::VTERM_MOD_NONE;
        convert_modifiers(&mut key, &mut state);
        assert_eq!(key, 'A' as i32);
        assert_eq!(state, crate::vterm_defs::VTERM_ALL_MODS_MASK);
    }

    #[test]
    fn convert_modifiers_lowercases_control_uppercase_without_shift() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard =
            ModMaskGuard::set(i32::from(crate::keycodes_defs::MOD_MASK_CTRL));
        let mut key = 'Z' as i32;
        let mut state = crate::vterm_defs::VTERM_MOD_NONE;
        convert_modifiers(&mut key, &mut state);
        assert_eq!(key, 'z' as i32);
        assert_eq!(state, crate::vterm_defs::VTERM_MOD_CTRL);
    }

    #[test]
    fn convert_modifiers_infers_shift_from_shifted_key_codes() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ModMaskGuard::set(0);
        for mut key in [
            crate::keycodes_defs::K_S_TAB,
            crate::keycodes_defs::K_S_UP,
            crate::keycodes_defs::K_S_DOWN,
            crate::keycodes_defs::K_S_LEFT,
            crate::keycodes_defs::K_S_RIGHT,
            crate::keycodes_defs::K_S_HOME,
            crate::keycodes_defs::K_S_END,
            crate::keycodes_defs::K_S_F1,
            crate::keycodes_defs::K_S_F12,
        ] {
            let mut state = crate::vterm_defs::VTERM_MOD_NONE;
            convert_modifiers(&mut key, &mut state);
            assert_eq!(state, crate::vterm_defs::VTERM_MOD_SHIFT);
        }
    }

    #[test]
    fn convert_modifiers_infers_control_from_control_key_codes() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ModMaskGuard::set(0);
        for mut key in [
            crate::keycodes_defs::K_C_LEFT,
            crate::keycodes_defs::K_C_RIGHT,
            crate::keycodes_defs::K_C_HOME,
            crate::keycodes_defs::K_C_END,
        ] {
            let mut state = crate::vterm_defs::VTERM_MOD_ALT;
            convert_modifiers(&mut key, &mut state);
            assert_eq!(
                state,
                crate::vterm_defs::VTERM_MOD_ALT | crate::vterm_defs::VTERM_MOD_CTRL
            );
        }
    }

    #[test]
    fn terminal_check_cursor_places_suspended_process_on_last_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut term = TerminalT {
            sb_current: 3,
            suspended: true,
            cursor: crate::types_defs::TerminalCursorT {
                row: 4,
                col: 9,
                ..Default::default()
            },
            ..Default::default()
        };
        let term_ptr = std::ptr::addr_of_mut!(term);
        let mut buf = crate::buffer_defs::BufT::default();
        buf.b_ml.ml_line_count = 30;
        buf.terminal = term_ptr;
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let mut win = crate::buffer_defs::WinT {
            w_buffer: buf_ptr,
            w_view_height: 10,
            w_topline: 1,
            w_botline: 11,
            ..Default::default()
        };
        let win_ptr = std::ptr::addr_of_mut!(win);
        let _curbuf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr) };
        let _curwin =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curwin, win_ptr) };
        let _state = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.State,
                crate::state_defs::mode::TERMINAL as i32,
            )
        };

        // SAFETY: the test installed a mutually-linked live terminal,
        // buffer, and window and holds the global-state lock.
        unsafe { terminal_check_cursor() };

        assert_eq!(win.w_cursor, crate::pos_defs::PosT {
            lnum: 30,
            ..Default::default()
        });
        assert_eq!(win.w_topline, 21);
        assert!(win.w_topline_was_set);
    }

    #[test]
    fn terminal_check_cursor_nudges_the_cursor_by_editor_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let mut term = TerminalT {
            cursor: crate::types_defs::TerminalCursorT {
                col: 2,
                ..Default::default()
            },
            ..Default::default()
        };
        let term_ptr = std::ptr::addr_of_mut!(term);
        let mut buf = crate::buffer_defs::BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        // All subsequent buffer access uses the one raw-pointer lineage.
        unsafe {
            (*buf_ptr).terminal = term_ptr;
        }
        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = std::ptr::addr_of_mut!(win);
        unsafe {
            (*win_ptr).w_buffer = buf_ptr;
            (*win_ptr).w_view_height = 10;
            (*win_ptr).w_topline = 1;
            (*win_ptr).w_botline = 2;
        }
        let _curbuf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr) };
        let _curwin =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curwin, win_ptr) };
        let state = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.State,
                crate::state_defs::mode::TERMINAL as i32,
            )
        };
        assert_eq!(unsafe { crate::memline::ml_replace(1, b"abcd\0") }, crate::vim_defs::OK);

        unsafe { terminal_check_cursor() };
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 2);

        drop(state);
        let state = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.State,
                crate::state_defs::mode::NORMAL as i32,
            )
        };
        unsafe { terminal_check_cursor() };
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 1);

        unsafe {
            (*win_ptr).w_onebuf_opt.wo_rl = 1;
            terminal_check_cursor();
        }
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 3);

        drop(state);
        drop(_curwin);
        drop(_curbuf);
        // SAFETY: the globals no longer alias this live memline.
        unsafe { crate::memline::ml_close(&mut *buf_ptr, false) };
    }

    #[test]
    fn terminal_buf_returns_the_owning_buffer_handle() {
        let term = TerminalT {
            buf_handle: 42,
            ..Default::default()
        };
        assert_eq!(terminal_buf(&term), 42);
    }

    #[test]
    fn get_underline_hl_flag_maps_all_styles_and_defaults_to_single() {
        use crate::vterm_defs::{
            VTERM_UNDERLINE_CURLY, VTERM_UNDERLINE_DOUBLE,
            VTERM_UNDERLINE_OFF, VTERM_UNDERLINE_SINGLE,
        };
        for (underline, expected) in [
            (VTERM_UNDERLINE_OFF, 0),
            (VTERM_UNDERLINE_SINGLE, crate::highlight_defs::HL_UNDERLINE),
            (VTERM_UNDERLINE_DOUBLE, crate::highlight_defs::HL_UNDERDOUBLE),
            (VTERM_UNDERLINE_CURLY, crate::highlight_defs::HL_UNDERCURL),
            (u8::MAX, crate::highlight_defs::HL_UNDERLINE),
        ] {
            let attrs = crate::vterm_defs::VTermScreenCellAttrs {
                underline,
                ..Default::default()
            };
            assert_eq!(get_underline_hl_flag(attrs), expected);
        }
    }

    #[test]
    fn terminal_running_is_the_inverse_of_closed() {
        let mut term = TerminalT::default();
        assert!(terminal_running(&term));
        term.closed = true;
        assert!(!terminal_running(&term));
    }

    #[test]
    fn terminal_suspended_tracks_the_terminal_flag() {
        let mut term = TerminalT::default();
        assert!(!terminal_suspended(&term));
        term.suspended = true;
        assert!(terminal_suspended(&term));
    }

    #[test]
    fn row_to_linenr_offsets_rows_by_scrollback_and_one() {
        let term = TerminalT {
            sb_current: 7,
            ..Default::default()
        };
        assert_eq!(row_to_linenr(&term, 0), 8);
        assert_eq!(row_to_linenr(&term, 3), 11);
        assert_eq!(row_to_linenr(&term, -2), 6);
    }

    #[test]
    fn row_to_linenr_preserves_the_int_max_sentinel() {
        let term = TerminalT {
            sb_current: 7,
            ..Default::default()
        };
        assert_eq!(row_to_linenr(&term, i32::MAX), i32::MAX);
    }

    #[test]
    fn linenr_to_row_removes_scrollback_and_one() {
        let term = TerminalT {
            sb_current: 7,
            ..Default::default()
        };
        assert_eq!(linenr_to_row(&term, 8), 0);
        assert_eq!(linenr_to_row(&term, 11), 3);
        assert_eq!(linenr_to_row(&term, 6), -2);
    }

    #[test]
    fn linenr_to_row_inverts_ordinary_row_conversion() {
        let term = TerminalT {
            sb_current: 1024,
            ..Default::default()
        };
        for row in [-10, 0, 1, 80, 10_000] {
            assert_eq!(linenr_to_row(&term, row_to_linenr(&term, row)), row);
        }
    }

    #[test]
    fn is_focused_requires_terminal_mode_and_the_current_buffers_terminal() {
        let _lock = crate::globals::global_state_test_lock();
        let mut term = TerminalT::default();
        let term_ptr = std::ptr::addr_of_mut!(term);
        let mut other = TerminalT::default();
        let other_ptr = std::ptr::addr_of_mut!(other);
        let mut buf = crate::buffer_defs::BufT {
            terminal: term_ptr,
            ..Default::default()
        };
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let _curbuf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr) };
        let _state = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.State,
                crate::state_defs::mode::TERMINAL as i32,
            )
        };

        assert!(unsafe { is_focused(term_ptr) });
        assert!(!unsafe { is_focused(other_ptr) });

        unsafe { &mut *crate::globals::GLOBALS.as_ptr() }.State =
            crate::state_defs::mode::NORMAL as i32;
        assert!(!unsafe { is_focused(term_ptr) });
    }

    struct BackgroundGuard(Option<Vec<u8>>);

    impl BackgroundGuard {
        fn install(value: &[u8]) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let saved = opts.p_bg.replace(value.to_vec());
            Self(saved)
        }
    }

    impl Drop for BackgroundGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bg =
                self.0.take();
        }
    }

    #[test]
    fn term_theme_tracks_the_background_options_first_byte() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = BackgroundGuard::install(b"dark");
        assert_eq!(unsafe { term_theme() }, (true, 1));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bg =
            Some(b"light".to_vec());
        assert_eq!(unsafe { term_theme() }, (false, 1));
    }

    /// Sets `'termpastefilter'`'s parsed flags, restoring the previous
    /// value on drop.
    struct TpfGuard {
        prev: u32,
    }

    impl TpfGuard {
        fn set(flags: u32) -> Self {
            let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let me = Self { prev: ov.tpf_flags };
            ov.tpf_flags = flags;
            me
        }
    }

    impl Drop for TpfGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tpf_flags = self.prev;
        }
    }

    /// With no flags set nothing is filtered, whatever the character.
    #[test]
    fn is_filter_char_filters_nothing_when_no_flags_are_set() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = TpfGuard::set(0);
        for c in [0x08, 0x09, 0x0C, 0x1b, 0x7F, 0x01, 0x85, b'a'.into()] {
            assert!(!unsafe { is_filter_char(c) }, "char {c:#x} must not filter");
        }
    }

    /// Each named control character is filtered by its OWN flag and
    /// not by any other, so the mapping cannot be silently transposed.
    #[test]
    fn is_filter_char_maps_each_control_to_its_own_flag() {
        let _lock = crate::globals::global_state_test_lock();
        for (c, flag) in [
            (0x08, opt_tpf_flag::BS),
            (0x09, opt_tpf_flag::HT),
            (0x0C, opt_tpf_flag::FF),
            (0x1b, opt_tpf_flag::ESC),
            (0x7F, opt_tpf_flag::DEL),
        ] {
            let _g = TpfGuard::set(flag);
            assert!(unsafe { is_filter_char(c) }, "{c:#x} must filter under its flag");

            // Every other flag must leave it alone.
            for other in [
                opt_tpf_flag::BS,
                opt_tpf_flag::HT,
                opt_tpf_flag::FF,
                opt_tpf_flag::ESC,
                opt_tpf_flag::DEL,
            ] {
                if other == flag {
                    continue;
                }
                let _g2 = TpfGuard::set(other);
                assert!(
                    !unsafe { is_filter_char(c) },
                    "{c:#x} must not filter under flag {other:#x}"
                );
            }
        }
    }

    /// Line feed and carriage return are never filtered, even with
    /// every flag set - a paste must keep its line structure.
    #[test]
    fn is_filter_char_never_filters_newline_or_carriage_return() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = TpfGuard::set(u32::MAX);
        assert!(!unsafe { is_filter_char(0x0A) });
        assert!(!unsafe { is_filter_char(0x0D) });
    }

    /// Unnamed C0 controls fall through to the C0 flag, but the
    /// characters with their own flag do NOT - they are matched
    /// earlier.
    #[test]
    fn is_filter_char_uses_c0_only_for_unnamed_controls() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = TpfGuard::set(opt_tpf_flag::C0);

        assert!(unsafe { is_filter_char(0x01) }, "SOH is an unnamed C0");
        assert!(unsafe { is_filter_char(0x1F) }, "US is an unnamed C0");
        // These have their own flags, so C0 alone must not filter them.
        assert!(!unsafe { is_filter_char(0x08) });
        assert!(!unsafe { is_filter_char(0x09) });
        assert!(!unsafe { is_filter_char(0x1b) });
        // ...nor the never-filtered pair.
        assert!(!unsafe { is_filter_char(0x0A) });
    }

    /// The C1 range is 0x80..=0x9F inclusive at both ends, and DEL
    /// (0x7F) sits just below it with its own flag.
    #[test]
    fn is_filter_char_bounds_the_c1_range_exactly() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = TpfGuard::set(opt_tpf_flag::C1);

        assert!(unsafe { is_filter_char(0x80) }, "inclusive lower bound");
        assert!(unsafe { is_filter_char(0x9F) }, "inclusive upper bound");
        assert!(!unsafe { is_filter_char(0x7F) }, "DEL has its own flag");
        assert!(!unsafe { is_filter_char(0xA0) }, "just past the range");
    }

    /// Ordinary printable characters are never filtered.
    #[test]
    fn is_filter_char_never_filters_printable_characters() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = TpfGuard::set(u32::MAX);
        for c in [b' '.into(), b'a'.into(), b'~'.into(), 0x100, 0x20AC] {
            assert!(!unsafe { is_filter_char(c) }, "char {c:#x} must not filter");
        }
    }
}
