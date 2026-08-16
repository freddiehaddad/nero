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
}
