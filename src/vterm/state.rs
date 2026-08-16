//! Translated from `src/nvim/vterm/state.c`.

/// Primary Device Attributes response (`vterm_primary_device_attr`).
pub static VTERM_PRIMARY_DEVICE_ATTR: &[u8] = b"61;22;52";

/// Terminal mode bitfields embedded in `VTermState`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermStateMode {
    pub keypad: bool,
    pub cursor: bool,
    pub autowrap: bool,
    pub insert: bool,
    pub newline: bool,
    pub cursor_visible: bool,
    pub cursor_blink: bool,
    pub cursor_shape: u8,
    pub alt_screen: bool,
    pub origin: bool,
    pub screen: bool,
    pub leftrightmargin: bool,
    pub bracketpaste: bool,
    pub report_focus: bool,
    pub theme_updates: bool,
    pub synchronized_output: bool,
}

/// Cursor-related mode fields saved by DEC 1048/1049.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermSavedMode {
    pub cursor_visible: bool,
    pub cursor_blink: bool,
    pub cursor_shape: u8,
}

/// Saved cursor and pen state (`state->saved`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermSavedState {
    pub pos: crate::vterm_defs::VTermPos,
    pub pen: crate::vterm::pen::VTermPen,
    pub mode: VTermSavedMode,
}

/// Selection parser state from `state->tmp.selection`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum VTermSelectionState {
    #[default]
    Initial = 0,
    Selected = 1,
    Query = 2,
    SetInitial = 3,
    Set = 4,
    Invalid = 5,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primary_device_attributes_match_state_c() {
        assert_eq!(VTERM_PRIMARY_DEVICE_ATTR, b"61;22;52");
    }

    #[test]
    fn state_modes_default_to_zeroed_bitfields() {
        assert_eq!(VTermStateMode::default(), VTermStateMode {
            keypad: false, cursor: false, autowrap: false, insert: false,
            newline: false, cursor_visible: false, cursor_blink: false,
            cursor_shape: 0, alt_screen: false, origin: false, screen: false,
            leftrightmargin: false, bracketpaste: false, report_focus: false,
            theme_updates: false, synchronized_output: false,
        });
    }

    #[test]
    fn saved_modes_default_to_zeroed_bitfields() {
        assert_eq!(VTermSavedMode::default(), VTermSavedMode {
            cursor_visible: false,
            cursor_blink: false,
            cursor_shape: 0,
        });
    }

    #[test]
    fn saved_state_defaults_to_zeroed_cursor_and_pen() {
        assert_eq!(VTermSavedState::default(), VTermSavedState {
            pos: crate::vterm_defs::VTermPos::default(),
            pen: crate::vterm::pen::VTermPen::default(),
            mode: VTermSavedMode::default(),
        });
    }

    #[test]
    fn selection_state_discriminants_match_internal_enum() {
        assert_eq!(VTermSelectionState::Initial as u8, 0);
        assert_eq!(VTermSelectionState::Selected as u8, 1);
        assert_eq!(VTermSelectionState::Query as u8, 2);
        assert_eq!(VTermSelectionState::SetInitial as u8, 3);
        assert_eq!(VTermSelectionState::Set as u8, 4);
        assert_eq!(VTermSelectionState::Invalid as u8, 5);
    }
}
