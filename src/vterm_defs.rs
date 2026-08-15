//! Translated from `src/nvim/vterm/vterm_defs.h` and
//! `vterm_keycodes_defs.h` (initial core).

pub const VTERM_VERSION_MAJOR: i32 = 0;
pub const VTERM_VERSION_MINOR: i32 = 3;

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

pub const C1_SS3: u8 = 0x8F;
pub const C1_DCS: u8 = 0x90;
pub const C1_CSI: u8 = 0x9B;
pub const C1_ST: u8 = 0x9C;
pub const C1_OSC: u8 = 0x9D;

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

/// Progressive keyboard-encoding flag stack
/// (`VTermKeyEncodingStack`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VTermKeyEncodingStack {
    pub items: [VTermKeyEncodingFlags; 16],
    pub size: u8,
}

/// Keyboard-related subset of `VTermState.mode` and `VTerm.mode`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermKeyboardMode {
    pub newline: bool,
    pub cursor: bool,
    pub keypad: bool,
    pub bracketpaste: bool,
    pub ctrl8bit: bool,
}

impl Default for VTermKeyEncodingStack {
    fn default() -> Self {
        Self {
            items: [VTermKeyEncodingFlags::default(); 16],
            size: 1,
        }
    }
}

impl VTermKeyEncodingStack {
    /// Returns the active top-of-stack flags.
    #[must_use]
    pub fn current(&self) -> VTermKeyEncodingFlags {
        debug_assert!(self.size > 0 && usize::from(self.size) <= self.items.len());
        self.items[usize::from(self.size) - 1]
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

/// Half-open terminal rectangle (`VTermRect`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermRect {
    pub start_row: i32,
    pub end_row: i32,
    pub start_col: i32,
    pub end_col: i32,
}

/// Moves a terminal rectangle (`vterm_rect_move`).
pub const fn vterm_rect_move(rect: &mut VTermRect, row_delta: i32, col_delta: i32) {
    rect.start_row += row_delta;
    rect.end_row += row_delta;
    rect.start_col += col_delta;
    rect.end_col += col_delta;
}

/// Control-string terminator (`VTermTerminator`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum VTermTerminator {
    Bel = 0,
    #[default]
    St = 1,
}

/// One streamed control-string fragment (`VTermStringFragment`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VTermStringFragment<'a> {
    pub bytes: &'a [u8],
    pub initial: bool,
    pub final_fragment: bool,
    pub terminator: VTermTerminator,
}

pub type VTermAttrMask = u32;
pub const VTERM_ATTR_BOLD_MASK: VTermAttrMask = 1 << 0;
pub const VTERM_ATTR_UNDERLINE_MASK: VTermAttrMask = 1 << 1;
pub const VTERM_ATTR_ITALIC_MASK: VTermAttrMask = 1 << 2;
pub const VTERM_ATTR_BLINK_MASK: VTermAttrMask = 1 << 3;
pub const VTERM_ATTR_REVERSE_MASK: VTermAttrMask = 1 << 4;
pub const VTERM_ATTR_STRIKE_MASK: VTermAttrMask = 1 << 5;
pub const VTERM_ATTR_FONT_MASK: VTermAttrMask = 1 << 6;
pub const VTERM_ATTR_FOREGROUND_MASK: VTermAttrMask = 1 << 7;
pub const VTERM_ATTR_BACKGROUND_MASK: VTermAttrMask = 1 << 8;
pub const VTERM_ATTR_CONCEAL_MASK: VTermAttrMask = 1 << 9;
pub const VTERM_ATTR_SMALL_MASK: VTermAttrMask = 1 << 10;
pub const VTERM_ATTR_BASELINE_MASK: VTermAttrMask = 1 << 11;
pub const VTERM_ATTR_URI_MASK: VTermAttrMask = 1 << 12;
pub const VTERM_ATTR_DIM_MASK: VTermAttrMask = 1 << 13;
pub const VTERM_ATTR_OVERLINE_MASK: VTermAttrMask = 1 << 14;
pub const VTERM_ALL_ATTRS_MASK: VTermAttrMask = (1 << 15) - 1;

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
    fn vterm_version_matches_vendored_header() {
        assert_eq!(VTERM_VERSION_MAJOR, 0);
        assert_eq!(VTERM_VERSION_MINOR, 3);
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
    fn c1_control_bytes_match_internal_defs() {
        assert_eq!([C1_SS3, C1_DCS, C1_CSI, C1_ST, C1_OSC], [
            0x8F, 0x90, 0x9B, 0x9C, 0x9D,
        ]);
    }

    #[test]
    fn vterm_position_preserves_row_and_column() {
        assert_eq!(VTermPos::default(), VTermPos { row: 0, col: 0 });
        assert_eq!(VTermPos { row: 12, col: 34 }.row, 12);
        assert_eq!(VTermPos { row: 12, col: 34 }.col, 34);
    }

    #[test]
    fn vterm_rect_move_offsets_all_four_edges() {
        let mut rect = VTermRect {
            start_row: 2,
            end_row: 8,
            start_col: 3,
            end_col: 10,
        };
        vterm_rect_move(&mut rect, -1, 4);
        assert_eq!(rect, VTermRect {
            start_row: 1,
            end_row: 7,
            start_col: 7,
            end_col: 14,
        });
    }

    #[test]
    fn string_fragment_preserves_slice_flags_and_terminator() {
        let fragment = VTermStringFragment {
            bytes: b"payload",
            initial: true,
            final_fragment: false,
            terminator: VTermTerminator::Bel,
        };
        assert_eq!(fragment.bytes, b"payload");
        assert!(fragment.initial);
        assert!(!fragment.final_fragment);
        assert_eq!(fragment.terminator, VTermTerminator::Bel);
        assert_eq!(VTermTerminator::Bel as i32, 0);
        assert_eq!(VTermTerminator::St as i32, 1);
        assert_eq!(VTermTerminator::default(), VTermTerminator::St);
    }

    #[test]
    fn attribute_masks_match_vterm_defs_header() {
        assert_eq!(
            [
                VTERM_ATTR_BOLD_MASK,
                VTERM_ATTR_UNDERLINE_MASK,
                VTERM_ATTR_ITALIC_MASK,
                VTERM_ATTR_BLINK_MASK,
                VTERM_ATTR_REVERSE_MASK,
                VTERM_ATTR_STRIKE_MASK,
                VTERM_ATTR_FONT_MASK,
                VTERM_ATTR_FOREGROUND_MASK,
                VTERM_ATTR_BACKGROUND_MASK,
                VTERM_ATTR_CONCEAL_MASK,
                VTERM_ATTR_SMALL_MASK,
                VTERM_ATTR_BASELINE_MASK,
                VTERM_ATTR_URI_MASK,
                VTERM_ATTR_DIM_MASK,
                VTERM_ATTR_OVERLINE_MASK,
            ],
            [
                1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096,
                8192, 16384,
            ]
        );
        assert_eq!(VTERM_ALL_ATTRS_MASK, 0x7FFF);
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

    #[test]
    fn key_encoding_stack_starts_with_one_zeroed_entry() {
        let stack = VTermKeyEncodingStack::default();
        assert_eq!(stack.size, 1);
        assert_eq!(stack.current(), VTermKeyEncodingFlags::default());
        assert_eq!(stack.items.len(), 16);
    }

    #[test]
    fn key_encoding_stack_current_reads_the_top_entry() {
        let mut stack = VTermKeyEncodingStack::default();
        stack.items[0].disambiguate = true;
        stack.items[1].report_events = true;
        stack.size = 2;
        assert_eq!(
            stack.current(),
            VTermKeyEncodingFlags {
                report_events: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn keyboard_mode_defaults_to_all_modes_disabled() {
        assert_eq!(VTermKeyboardMode::default(), VTermKeyboardMode {
            newline: false,
            cursor: false,
            keypad: false,
            bracketpaste: false,
            ctrl8bit: false,
        });
    }
}
