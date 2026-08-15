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

/// Expands `destination` to contain `source` (`rect_expand`).
pub fn rect_expand(
    destination: &mut crate::vterm_defs::VTermRect,
    source: &crate::vterm_defs::VTermRect,
) {
    destination.start_row = destination.start_row.min(source.start_row);
    destination.start_col = destination.start_col.min(source.start_col);
    destination.end_row = destination.end_row.max(source.end_row);
    destination.end_col = destination.end_col.max(source.end_col);
}

/// Clips `destination` to `bounds` and prevents negative dimensions
/// (`rect_clip`).
pub fn rect_clip(
    destination: &mut crate::vterm_defs::VTermRect,
    bounds: &crate::vterm_defs::VTermRect,
) {
    destination.start_row = destination.start_row.max(bounds.start_row);
    destination.start_col = destination.start_col.max(bounds.start_col);
    destination.end_row = destination.end_row.min(bounds.end_row);
    destination.end_col = destination.end_col.min(bounds.end_col);
    if destination.end_row < destination.start_row {
        destination.end_row = destination.start_row;
    }
    if destination.end_col < destination.start_col {
        destination.end_col = destination.start_col;
    }
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

    #[test]
    fn rect_expand_grows_each_edge_only_when_needed() {
        let mut destination = crate::vterm_defs::VTermRect {
            start_row: 2,
            end_row: 5,
            start_col: 3,
            end_col: 7,
        };
        rect_expand(
            &mut destination,
            &crate::vterm_defs::VTermRect {
                start_row: 1,
                end_row: 6,
                start_col: 4,
                end_col: 5,
            },
        );
        assert_eq!(destination, crate::vterm_defs::VTermRect {
            start_row: 1,
            end_row: 6,
            start_col: 3,
            end_col: 7,
        });
    }

    #[test]
    fn rect_clip_constrains_edges_and_collapses_disjoint_dimensions() {
        let bounds = crate::vterm_defs::VTermRect {
            start_row: 2,
            end_row: 8,
            start_col: 3,
            end_col: 9,
        };
        let mut destination = crate::vterm_defs::VTermRect {
            start_row: 0,
            end_row: 10,
            start_col: 1,
            end_col: 12,
        };
        rect_clip(&mut destination, &bounds);
        assert_eq!(destination, bounds);

        destination = crate::vterm_defs::VTermRect {
            start_row: 20,
            end_row: 30,
            start_col: -5,
            end_col: 1,
        };
        rect_clip(&mut destination, &bounds);
        assert_eq!(destination, crate::vterm_defs::VTermRect {
            start_row: 20,
            end_row: 20,
            start_col: 3,
            end_col: 3,
        });
    }
}
