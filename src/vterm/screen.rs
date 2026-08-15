//! Translated from `src/nvim/vterm/screen.c`.

pub const UNICODE_SPACE: u32 = 0x20;
pub const UNICODE_LINEFEED: u32 = 0x0A;

/// Internal screen pen (`ScreenPen`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScreenPen {
    pub fg: crate::vterm_defs::VTermColor,
    pub bg: crate::vterm_defs::VTermColor,
    pub uri: i32,
    pub bold: bool,
    pub underline: u8,
    pub italic: bool,
    pub blink: bool,
    pub reverse: bool,
    pub conceal: bool,
    pub strike: bool,
    pub font: u8,
    pub small: bool,
    pub baseline: u8,
    pub dim: bool,
    pub overline: bool,
    pub protected_cell: bool,
    pub dwl: bool,
    pub dhl: u8,
}

/// Internal representation of one screen cell (`ScreenCell`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScreenCell {
    pub schar: crate::types_defs::ScharT,
    pub pen: ScreenPen,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_pen_defaults_match_zeroed_internal_pen() {
        assert_eq!(ScreenPen::default(), ScreenPen {
            fg: crate::vterm_defs::VTermColor::default(),
            bg: crate::vterm_defs::VTermColor::default(),
            uri: 0,
            bold: false,
            underline: 0,
            italic: false,
            blink: false,
            reverse: false,
            conceal: false,
            strike: false,
            font: 0,
            small: false,
            baseline: 0,
            dim: false,
            overline: false,
            protected_cell: false,
            dwl: false,
            dhl: 0,
        });
        assert_eq!(UNICODE_SPACE, 0x20);
        assert_eq!(UNICODE_LINEFEED, 0x0A);
    }

    #[test]
    fn screen_cell_defaults_to_blank_with_zeroed_pen() {
        assert_eq!(ScreenCell::default(), ScreenCell {
            schar: 0,
            pen: ScreenPen::default(),
        });
    }
}
