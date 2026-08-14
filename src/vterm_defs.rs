//! Translated from `src/nvim/vterm/vterm_defs.h` and
//! `vterm_keycodes_defs.h` (initial core).

/// Terminal key modifier mask (`VTermModifier`).
pub type VTermModifier = u8;
pub const VTERM_MOD_NONE: VTermModifier = 0x00;
pub const VTERM_MOD_SHIFT: VTermModifier = 0x01;
pub const VTERM_MOD_ALT: VTermModifier = 0x02;
pub const VTERM_MOD_CTRL: VTermModifier = 0x04;
pub const VTERM_ALL_MODS_MASK: VTermModifier = 0x07;

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
}
