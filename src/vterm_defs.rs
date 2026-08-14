//! Translated from `src/nvim/vterm/vterm_defs.h` and
//! `vterm_keycodes_defs.h` (initial core).

/// Terminal key modifier mask (`VTermModifier`).
pub type VTermModifier = u8;
pub const VTERM_MOD_NONE: VTermModifier = 0x00;
pub const VTERM_MOD_SHIFT: VTermModifier = 0x01;
pub const VTERM_MOD_ALT: VTermModifier = 0x02;
pub const VTERM_MOD_CTRL: VTermModifier = 0x04;
pub const VTERM_ALL_MODS_MASK: VTermModifier = 0x07;

pub const KEY_ENCODING_DISAMBIGUATE: u8 = 0x01;
pub const KEY_ENCODING_REPORT_EVENTS: u8 = 0x02;
pub const KEY_ENCODING_REPORT_ALTERNATE: u8 = 0x04;
pub const KEY_ENCODING_REPORT_ALL_KEYS: u8 = 0x08;
pub const KEY_ENCODING_REPORT_ASSOCIATED: u8 = 0x10;

/// Kitty keyboard-protocol enhancement flags
/// (`VTermKeyEncodingFlags`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermKeyEncodingFlags {
    pub disambiguate: bool,
    pub report_events: bool,
    pub report_alternate: bool,
    pub report_all_keys: bool,
    pub report_associated: bool,
}

impl VTermKeyEncodingFlags {
    /// Converts this flag set to the protocol bitmask.
    #[must_use]
    pub const fn bits(self) -> u8 {
        (if self.disambiguate { KEY_ENCODING_DISAMBIGUATE } else { 0 })
            | (if self.report_events { KEY_ENCODING_REPORT_EVENTS } else { 0 })
            | (if self.report_alternate { KEY_ENCODING_REPORT_ALTERNATE } else { 0 })
            | (if self.report_all_keys { KEY_ENCODING_REPORT_ALL_KEYS } else { 0 })
            | (if self.report_associated { KEY_ENCODING_REPORT_ASSOCIATED } else { 0 })
    }
}

/// Terminal key code (`VTermKey`).
pub type VTermKey = i32;
pub const VTERM_KEY_NONE: VTermKey = 0;
pub const VTERM_KEY_ENTER: VTermKey = 1;
pub const VTERM_KEY_TAB: VTermKey = 2;
pub const VTERM_KEY_BACKSPACE: VTermKey = 3;
pub const VTERM_KEY_ESCAPE: VTermKey = 4;
pub const VTERM_KEY_UP: VTermKey = 5;
pub const VTERM_KEY_DOWN: VTermKey = 6;
pub const VTERM_KEY_LEFT: VTermKey = 7;
pub const VTERM_KEY_RIGHT: VTermKey = 8;
pub const VTERM_KEY_INS: VTermKey = 9;
pub const VTERM_KEY_DEL: VTermKey = 10;
pub const VTERM_KEY_HOME: VTermKey = 11;
pub const VTERM_KEY_END: VTermKey = 12;
pub const VTERM_KEY_PAGEUP: VTermKey = 13;
pub const VTERM_KEY_PAGEDOWN: VTermKey = 14;
pub const VTERM_KEY_FUNCTION_0: VTermKey = 256;
pub const VTERM_KEY_FUNCTION_MAX: VTermKey = VTERM_KEY_FUNCTION_0 + 255;
pub const VTERM_KEY_KP_0: VTermKey = VTERM_KEY_FUNCTION_MAX + 1;
pub const VTERM_KEY_KP_1: VTermKey = VTERM_KEY_KP_0 + 1;
pub const VTERM_KEY_KP_2: VTermKey = VTERM_KEY_KP_0 + 2;
pub const VTERM_KEY_KP_3: VTermKey = VTERM_KEY_KP_0 + 3;
pub const VTERM_KEY_KP_4: VTermKey = VTERM_KEY_KP_0 + 4;
pub const VTERM_KEY_KP_5: VTermKey = VTERM_KEY_KP_0 + 5;
pub const VTERM_KEY_KP_6: VTermKey = VTERM_KEY_KP_0 + 6;
pub const VTERM_KEY_KP_7: VTermKey = VTERM_KEY_KP_0 + 7;
pub const VTERM_KEY_KP_8: VTermKey = VTERM_KEY_KP_0 + 8;
pub const VTERM_KEY_KP_9: VTermKey = VTERM_KEY_KP_0 + 9;
pub const VTERM_KEY_KP_MULT: VTermKey = VTERM_KEY_KP_0 + 10;
pub const VTERM_KEY_KP_PLUS: VTermKey = VTERM_KEY_KP_0 + 11;
pub const VTERM_KEY_KP_COMMA: VTermKey = VTERM_KEY_KP_0 + 12;
pub const VTERM_KEY_KP_MINUS: VTermKey = VTERM_KEY_KP_0 + 13;
pub const VTERM_KEY_KP_PERIOD: VTermKey = VTERM_KEY_KP_0 + 14;
pub const VTERM_KEY_KP_DIVIDE: VTermKey = VTERM_KEY_KP_0 + 15;
pub const VTERM_KEY_KP_ENTER: VTermKey = VTERM_KEY_KP_0 + 16;
pub const VTERM_KEY_KP_EQUAL: VTermKey = VTERM_KEY_KP_0 + 17;
pub const VTERM_KEY_MAX: VTermKey = VTERM_KEY_KP_0 + 18;
pub const VTERM_N_KEYS: VTermKey = VTERM_KEY_MAX;

/// Constructs `VTERM_KEY_FUNCTION(n)`.
#[must_use]
pub const fn vterm_key_function(n: i32) -> VTermKey {
    VTERM_KEY_FUNCTION_0 + n
}

/// Zero-based terminal screen position (`VTermPos`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermPos {
    pub row: i32,
    pub col: i32,
}

/// Underline disabled (`VTERM_UNDERLINE_OFF`).
pub const VTERM_UNDERLINE_OFF: u8 = 0;
/// Single underline (`VTERM_UNDERLINE_SINGLE`).
pub const VTERM_UNDERLINE_SINGLE: u8 = 1;
/// Double underline (`VTERM_UNDERLINE_DOUBLE`).
pub const VTERM_UNDERLINE_DOUBLE: u8 = 2;
/// Curly underline (`VTERM_UNDERLINE_CURLY`).
pub const VTERM_UNDERLINE_CURLY: u8 = 3;

/// Display attributes for one terminal screen cell
/// (`VTermScreenCellAttrs`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermScreenCellAttrs {
    pub bold: bool,
    pub underline: u8,
    pub italic: bool,
    pub blink: bool,
    pub reverse: bool,
    pub conceal: bool,
    pub strike: bool,
    pub font: u8,
    pub dwl: bool,
    pub dhl: u8,
    pub small: bool,
    pub baseline: u8,
    pub dim: bool,
    pub overline: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_cell_attributes_default_to_all_off() {
        assert_eq!(VTermScreenCellAttrs::default(), VTermScreenCellAttrs {
            bold: false,
            underline: VTERM_UNDERLINE_OFF,
            italic: false,
            blink: false,
            reverse: false,
            conceal: false,
            strike: false,
            font: 0,
            dwl: false,
            dhl: 0,
            small: false,
            baseline: 0,
            dim: false,
            overline: false,
        });
        assert_eq!(
            [
                VTERM_UNDERLINE_OFF,
                VTERM_UNDERLINE_SINGLE,
                VTERM_UNDERLINE_DOUBLE,
                VTERM_UNDERLINE_CURLY,
            ],
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn vterm_modifier_bits_match_keycodes_header() {
        assert_eq!(VTERM_MOD_NONE, 0);
        assert_eq!(VTERM_MOD_SHIFT, 1);
        assert_eq!(VTERM_MOD_ALT, 2);
        assert_eq!(VTERM_MOD_CTRL, 4);
        assert_eq!(
            VTERM_MOD_SHIFT | VTERM_MOD_ALT | VTERM_MOD_CTRL,
            VTERM_ALL_MODS_MASK
        );
    }

    #[test]
    fn key_encoding_flags_match_protocol_bits() {
        assert_eq!(VTermKeyEncodingFlags::default().bits(), 0);
        assert_eq!(
            VTermKeyEncodingFlags {
                disambiguate: true,
                report_events: true,
                report_alternate: true,
                report_all_keys: true,
                report_associated: true,
            }
            .bits(),
            0x1F
        );
        assert_eq!(
            [
                KEY_ENCODING_DISAMBIGUATE,
                KEY_ENCODING_REPORT_EVENTS,
                KEY_ENCODING_REPORT_ALTERNATE,
                KEY_ENCODING_REPORT_ALL_KEYS,
                KEY_ENCODING_REPORT_ASSOCIATED,
            ],
            [1, 2, 4, 8, 16]
        );
    }

    #[test]
    fn vterm_position_preserves_row_and_column() {
        assert_eq!(VTermPos::default(), VTermPos { row: 0, col: 0 });
        assert_eq!(VTermPos { row: 12, col: 34 }.row, 12);
        assert_eq!(VTermPos { row: 12, col: 34 }.col, 34);
    }

    #[test]
    fn vterm_key_discriminants_match_keycodes_header() {
        assert_eq!(VTERM_KEY_NONE, 0);
        assert_eq!(VTERM_KEY_PAGEDOWN, 14);
        assert_eq!(vterm_key_function(0), 256);
        assert_eq!(vterm_key_function(255), VTERM_KEY_FUNCTION_MAX);
        assert_eq!(VTERM_KEY_KP_0, 512);
        assert_eq!(VTERM_KEY_KP_EQUAL, 529);
        assert_eq!(VTERM_KEY_MAX, 530);
        assert_eq!(VTERM_N_KEYS, VTERM_KEY_MAX);
    }
}
