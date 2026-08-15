//! Translated from `src/nvim/vterm/mouse.c` (protocol encoding core).

/// Terminal mouse reporting protocol (`mouse_protocol`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum VTermMouseProtocol {
    #[default]
    X10 = 0,
    Utf8 = 1,
    Sgr = 2,
    Rxvt = 3,
}

pub const MOUSE_WANT_CLICK: i32 = 0x01;
pub const MOUSE_WANT_DRAG: i32 = 0x02;
pub const MOUSE_WANT_MOVE: i32 = 0x04;

/// Mouse-related fields of `VTermState`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermMouseState {
    pub col: i32,
    pub row: i32,
    pub buttons: i32,
    pub flags: i32,
    pub protocol: VTermMouseProtocol,
    pub ctrl8bit: bool,
}

fn push_control(output: &mut Vec<u8>, ctrl: u8, ctrl8bit: bool) {
    if ctrl >= 0x80 && !ctrl8bit {
        output.extend_from_slice(&[0x1B, ctrl - 0x40]);
    } else {
        output.push(ctrl);
    }
}

fn push_utf8(output: &mut Vec<u8>, codepoint: i32) {
    let mut bytes = [0; 6];
    let len = crate::mbyte::utf_char2bytes(codepoint, &mut bytes) as usize;
    output.extend_from_slice(&bytes[..len]);
}

/// Encodes one terminal mouse report (`output_mouse`).
///
/// The C callback pushes these bytes through its `VTerm` output
/// callback. This equivalent returns the exact report bytes until that
/// output host is translated.
#[must_use]
pub fn output_mouse(
    protocol: VTermMouseProtocol,
    mut code: i32,
    pressed: bool,
    modifiers: i32,
    mut col: i32,
    mut row: i32,
    ctrl8bit: bool,
) -> Vec<u8> {
    let modifiers = modifiers << 2;
    let mut output = Vec::new();

    match protocol {
        VTermMouseProtocol::X10 => {
            if col + 0x21 > 0xFF {
                col = 0xFF - 0x21;
            }
            if row + 0x21 > 0xFF {
                row = 0xFF - 0x21;
            }
            if !pressed {
                code = 3;
            }
            if code & 0x80 != 0 {
                return output;
            }
            push_control(&mut output, crate::vterm_defs::C1_CSI, ctrl8bit);
            output.extend_from_slice(&[
                b'M',
                ((code | modifiers) + 0x20) as u8,
                (col + 0x21) as u8,
                (row + 0x21) as u8,
            ]);
        }
        VTermMouseProtocol::Utf8 => {
            if !pressed {
                code = 3;
            }
            push_control(&mut output, crate::vterm_defs::C1_CSI, ctrl8bit);
            output.push(b'M');
            push_utf8(&mut output, (code | modifiers) + 0x20);
            push_utf8(&mut output, col + 0x21);
            push_utf8(&mut output, row + 0x21);
        }
        VTermMouseProtocol::Sgr => {
            push_control(&mut output, crate::vterm_defs::C1_CSI, ctrl8bit);
            output.extend_from_slice(
                format!(
                    "<{};{};{}{}",
                    code | modifiers,
                    col + 1,
                    row + 1,
                    if pressed { 'M' } else { 'm' }
                )
                .as_bytes(),
            );
        }
        VTermMouseProtocol::Rxvt => {
            if !pressed {
                code = 3;
            }
            push_control(&mut output, crate::vterm_defs::C1_CSI, ctrl8bit);
            output.extend_from_slice(
                format!("{};{};{}M", code | modifiers, col + 1, row + 1).as_bytes(),
            );
        }
    }
    output
}

/// Updates the mouse position and emits a drag/motion report when
/// requested (`vterm_mouse_move`).
pub fn vterm_mouse_move(
    state: &mut VTermMouseState,
    row: i32,
    col: i32,
    modifiers: crate::vterm_defs::VTermModifier,
) -> Vec<u8> {
    if col == state.col && row == state.row {
        return Vec::new();
    }

    state.col = col;
    state.row = row;

    if (state.flags & MOUSE_WANT_DRAG != 0 && state.buttons != 0)
        || state.flags & MOUSE_WANT_MOVE != 0
    {
        if state.buttons != 0 {
            let button = state.buttons.trailing_zeros() as i32 + 1;
            if button < 4 {
                return output_mouse(
                    state.protocol,
                    button - 1 + 0x20,
                    true,
                    i32::from(modifiers),
                    col,
                    row,
                    state.ctrl8bit,
                );
            }
            if (8..12).contains(&button) {
                return output_mouse(
                    state.protocol,
                    button - 8 + 0x80 + 0x20,
                    true,
                    i32::from(modifiers),
                    col,
                    row,
                    state.ctrl8bit,
                );
            }
        } else {
            return output_mouse(
                state.protocol,
                3 + 0x20,
                true,
                i32::from(modifiers),
                col,
                row,
                state.ctrl8bit,
            );
        }
    }
    Vec::new()
}

/// Updates the pressed-button mask and emits a button report
/// (`vterm_mouse_button`).
pub fn vterm_mouse_button(
    state: &mut VTermMouseState,
    button: i32,
    pressed: bool,
    modifiers: crate::vterm_defs::VTermModifier,
) -> Vec<u8> {
    let old_buttons = state.buttons;

    if (button > 0 && button <= 3) || (8..=11).contains(&button) {
        let bit = 1 << (button - 1);
        if pressed {
            state.buttons |= bit;
        } else {
            state.buttons &= !bit;
        }
    }

    if state.buttons == old_buttons && !(4..=7).contains(&button) {
        return Vec::new();
    }
    if state.flags == 0 {
        return Vec::new();
    }

    let code = if button < 4 {
        button - 1
    } else if button < 8 {
        button - 4 + 0x40
    } else if button < 12 {
        button - 8 + 0x80
    } else {
        return Vec::new();
    };
    output_mouse(
        state.protocol,
        code,
        pressed,
        i32::from(modifiers),
        state.col,
        state.row,
        state.ctrl8bit,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_state_defaults_match_vterm_state_initialization() {
        let state = VTermMouseState::default();
        assert_eq!(state.col, 0);
        assert_eq!(state.row, 0);
        assert_eq!(state.buttons, 0);
        assert_eq!(state.flags, 0);
        assert_eq!(state.protocol, VTermMouseProtocol::X10);
        assert!(!state.ctrl8bit);
        assert_eq!(
            [MOUSE_WANT_CLICK, MOUSE_WANT_DRAG, MOUSE_WANT_MOVE],
            [1, 2, 4]
        );
    }

    #[test]
    fn x10_mouse_reports_press_release_clamping_and_modifiers() {
        assert_eq!(
            output_mouse(VTermMouseProtocol::X10, 0, true, 0, 4, 5, false),
            [b"\x1b[M".as_slice(), &[0x20, 0x25, 0x26]].concat()
        );
        assert_eq!(
            output_mouse(VTermMouseProtocol::X10, 0, false, 1, 4, 5, false),
            [b"\x1b[M".as_slice(), &[0x27, 0x25, 0x26]].concat()
        );
        assert_eq!(
            output_mouse(VTermMouseProtocol::X10, 0, true, 0, 999, 999, false),
            [b"\x1b[M".as_slice(), &[0x20, 0xFF, 0xFF]].concat()
        );
        assert!(
            output_mouse(VTermMouseProtocol::X10, 0x80, true, 0, 0, 0, false)
                .is_empty()
        );
    }

    #[test]
    fn utf8_mouse_reports_large_coordinates_as_utf8() {
        let output = output_mouse(VTermMouseProtocol::Utf8, 0, true, 0, 300, 400, false);
        let mut expected = b"\x1b[M ".to_vec();
        push_utf8(&mut expected, 333);
        push_utf8(&mut expected, 433);
        assert_eq!(output, expected);
        assert_eq!(
            output_mouse(VTermMouseProtocol::Utf8, 0, false, 0, 0, 0, false),
            b"\x1b[M#!!"
        );
    }

    #[test]
    fn sgr_mouse_reports_press_release_and_one_based_coordinates() {
        assert_eq!(
            output_mouse(VTermMouseProtocol::Sgr, 2, true, 1, 4, 5, false),
            b"\x1b[<6;5;6M"
        );
        assert_eq!(
            output_mouse(VTermMouseProtocol::Sgr, 2, false, 1, 4, 5, false),
            b"\x1b[<6;5;6m"
        );
    }

    #[test]
    fn rxvt_mouse_reports_release_as_button_three() {
        assert_eq!(
            output_mouse(VTermMouseProtocol::Rxvt, 2, true, 1, 4, 5, false),
            b"\x1b[6;5;6M"
        );
        assert_eq!(
            output_mouse(VTermMouseProtocol::Rxvt, 2, false, 1, 4, 5, false),
            b"\x1b[7;5;6M"
        );
    }

    #[test]
    fn mouse_reports_can_use_eight_bit_csi() {
        assert_eq!(
            output_mouse(VTermMouseProtocol::Sgr, 0, true, 0, 0, 0, true),
            [vec![crate::vterm_defs::C1_CSI], b"<0;1;1M".to_vec()].concat()
        );
    }

    #[test]
    fn mouse_move_updates_position_without_reporting_when_not_requested() {
        let mut state = VTermMouseState::default();
        assert!(vterm_mouse_move(&mut state, 5, 7, 0).is_empty());
        assert_eq!((state.row, state.col), (5, 7));
        assert!(vterm_mouse_move(&mut state, 5, 7, 0).is_empty());
    }

    #[test]
    fn mouse_move_reports_motion_without_buttons() {
        let mut state = VTermMouseState {
            flags: MOUSE_WANT_MOVE,
            protocol: VTermMouseProtocol::Sgr,
            ..Default::default()
        };
        assert_eq!(
            vterm_mouse_move(
                &mut state,
                2,
                3,
                crate::vterm_defs::VTERM_MOD_SHIFT,
            ),
            b"\x1b[<39;4;3M"
        );
    }

    #[test]
    fn mouse_move_reports_drag_for_the_lowest_pressed_button() {
        let mut state = VTermMouseState {
            buttons: (1 << 2) | (1 << 0),
            flags: MOUSE_WANT_CLICK | MOUSE_WANT_DRAG,
            protocol: VTermMouseProtocol::Sgr,
            ..Default::default()
        };
        assert_eq!(
            vterm_mouse_move(&mut state, 1, 4, 0),
            b"\x1b[<32;5;2M"
        );
    }

    #[test]
    fn mouse_move_reports_extended_buttons_and_ignores_other_buttons() {
        let mut state = VTermMouseState {
            buttons: 1 << 7,
            flags: MOUSE_WANT_DRAG,
            protocol: VTermMouseProtocol::Sgr,
            ..Default::default()
        };
        assert_eq!(
            vterm_mouse_move(&mut state, 1, 4, 0),
            b"\x1b[<160;5;2M"
        );

        state.buttons = 1 << 3;
        assert!(vterm_mouse_move(&mut state, 2, 4, 0).is_empty());
    }

    #[test]
    fn mouse_button_tracks_primary_and_extended_button_bits() {
        let mut state = VTermMouseState::default();
        assert!(vterm_mouse_button(&mut state, 1, true, 0).is_empty());
        assert_eq!(state.buttons, 1);
        assert!(vterm_mouse_button(&mut state, 3, true, 0).is_empty());
        assert_eq!(state.buttons, 0b101);
        assert!(vterm_mouse_button(&mut state, 8, true, 0).is_empty());
        assert_eq!(state.buttons, 0b1000_0101);
        assert!(vterm_mouse_button(&mut state, 1, false, 0).is_empty());
        assert_eq!(state.buttons, 0b1000_0100);
    }

    #[test]
    fn mouse_button_reports_primary_press_and_release() {
        let mut state = VTermMouseState {
            col: 4,
            row: 5,
            flags: MOUSE_WANT_CLICK,
            protocol: VTermMouseProtocol::Sgr,
            ..Default::default()
        };
        assert_eq!(
            vterm_mouse_button(&mut state, 2, true, 0),
            b"\x1b[<1;5;6M"
        );
        assert_eq!(
            vterm_mouse_button(&mut state, 2, false, 0),
            b"\x1b[<1;5;6m"
        );
        assert!(vterm_mouse_button(&mut state, 2, false, 0).is_empty());
    }

    #[test]
    fn mouse_button_reports_wheel_without_tracking_it() {
        let mut state = VTermMouseState {
            col: 1,
            row: 2,
            flags: MOUSE_WANT_CLICK,
            protocol: VTermMouseProtocol::Sgr,
            ..Default::default()
        };
        assert_eq!(
            vterm_mouse_button(
                &mut state,
                5,
                true,
                crate::vterm_defs::VTERM_MOD_CTRL,
            ),
            b"\x1b[<81;2;3M"
        );
        assert_eq!(state.buttons, 0);
    }

    #[test]
    fn mouse_button_reports_extended_buttons_and_ignores_out_of_range() {
        let mut state = VTermMouseState {
            flags: MOUSE_WANT_CLICK,
            protocol: VTermMouseProtocol::Sgr,
            ..Default::default()
        };
        assert_eq!(
            vterm_mouse_button(&mut state, 9, true, 0),
            b"\x1b[<129;1;1M"
        );
        assert!(vterm_mouse_button(&mut state, 12, true, 0).is_empty());
        assert!(vterm_mouse_button(&mut state, 0, true, 0).is_empty());
    }
}
