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

/// Owned translation of `VTermScreen`'s data fields.
#[derive(Debug)]
pub struct VTermScreen {
    pub damage_merge: crate::vterm_defs::VTermDamageSize,
    pub damaged: crate::vterm_defs::VTermRect,
    pub pending_scrollrect: crate::vterm_defs::VTermRect,
    pub pending_scroll_downward: i32,
    pub pending_scroll_rightward: i32,
    pub rows: i32,
    pub cols: i32,
    pub global_reverse: bool,
    pub reflow: bool,
    pub buffers: [Option<Vec<ScreenCell>>; 2],
    pub active_buffer: usize,
    pub sb_buffer: Vec<crate::vterm_defs::VTermScreenCell>,
    pub pen: ScreenPen,
}

/// Creates a screen with its primary buffer (`screen_new`).
#[must_use]
pub fn screen_new(rows: i32, cols: i32) -> VTermScreen {
    let cell_count = usize::try_from(rows)
        .ok()
        .and_then(|rows| usize::try_from(cols).ok().and_then(|cols| rows.checked_mul(cols)))
        .unwrap_or(0);
    VTermScreen {
        damage_merge: crate::vterm_defs::VTermDamageSize::Cell,
        damaged: crate::vterm_defs::VTermRect {
            start_row: -1,
            ..Default::default()
        },
        pending_scrollrect: crate::vterm_defs::VTermRect {
            start_row: -1,
            ..Default::default()
        },
        pending_scroll_downward: 0,
        pending_scroll_rightward: 0,
        rows,
        cols,
        global_reverse: false,
        reflow: false,
        buffers: [Some(vec![ScreenCell::default(); cell_count]), None],
        active_buffer: 0,
        sb_buffer: vec![
            crate::vterm_defs::VTermScreenCell::default();
            usize::try_from(cols).unwrap_or(0)
        ],
        pen: ScreenPen::default(),
    }
}

/// Clears a cell with the screen's current pen (`clearcell`).
pub fn clearcell(screen: &VTermScreen, cell: &mut ScreenCell) {
    cell.schar = 0;
    cell.pen = screen.pen;
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

/// Whether two rectangles have identical edges (`rect_equal`).
#[must_use]
pub const fn rect_equal(
    first: &crate::vterm_defs::VTermRect,
    second: &crate::vterm_defs::VTermRect,
) -> bool {
    first.start_row == second.start_row
        && first.start_col == second.start_col
        && first.end_row == second.end_row
        && first.end_col == second.end_col
}

/// Whether `small` is entirely inside `big` (`rect_contains`).
#[must_use]
pub const fn rect_contains(
    big: &crate::vterm_defs::VTermRect,
    small: &crate::vterm_defs::VTermRect,
) -> bool {
    small.start_row >= big.start_row
        && small.start_col >= big.start_col
        && small.end_row <= big.end_row
        && small.end_col <= big.end_col
}

/// Whether rectangles overlap according to libvterm's edge-inclusive
/// test (`rect_intersects`).
#[must_use]
pub const fn rect_intersects(
    first: &crate::vterm_defs::VTermRect,
    second: &crate::vterm_defs::VTermRect,
) -> bool {
    !(first.start_row > second.end_row
        || second.start_row > first.end_row
        || first.start_col > second.end_col
        || second.start_col > first.end_col)
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
    fn screen_new_initializes_primary_buffer_and_damage_sentinels() {
        let screen = screen_new(3, 4);
        assert_eq!(screen.damage_merge, crate::vterm_defs::VTermDamageSize::Cell);
        assert_eq!(screen.damaged.start_row, -1);
        assert_eq!(screen.pending_scrollrect.start_row, -1);
        assert_eq!((screen.rows, screen.cols), (3, 4));
        assert!(!screen.global_reverse);
        assert!(!screen.reflow);
        assert_eq!(screen.active_buffer, 0);
        assert_eq!(screen.buffers[0].as_ref().unwrap().len(), 12);
        assert!(screen.buffers[1].is_none());
        assert_eq!(screen.sb_buffer.len(), 4);
        assert_eq!(screen.pen, ScreenPen::default());
    }

    #[test]
    fn clearcell_blanks_the_character_and_copies_current_pen() {
        let mut screen = screen_new(1, 1);
        screen.pen.bold = true;
        screen.pen.uri = 42;
        let mut cell = ScreenCell {
            schar: 123,
            pen: ScreenPen::default(),
        };
        clearcell(&screen, &mut cell);
        assert_eq!(cell.schar, 0);
        assert_eq!(cell.pen, screen.pen);
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

    #[test]
    fn rect_equal_compares_all_four_edges() {
        let rect = crate::vterm_defs::VTermRect {
            start_row: 1,
            end_row: 2,
            start_col: 3,
            end_col: 4,
        };
        assert!(rect_equal(&rect, &rect));
        for changed in [
            crate::vterm_defs::VTermRect { start_row: 0, ..rect },
            crate::vterm_defs::VTermRect { end_row: 3, ..rect },
            crate::vterm_defs::VTermRect { start_col: 2, ..rect },
            crate::vterm_defs::VTermRect { end_col: 5, ..rect },
        ] {
            assert!(!rect_equal(&rect, &changed));
        }
    }

    #[test]
    fn rect_contains_accepts_shared_edges_and_rejects_each_overhang() {
            let big = crate::vterm_defs::VTermRect {
                start_row: 1,
                end_row: 10,
                start_col: 2,
                end_col: 20,
            };
            assert!(rect_contains(&big, &big));
            assert!(rect_contains(
                &big,
                &crate::vterm_defs::VTermRect {
                    start_row: 3,
                    end_row: 8,
                    start_col: 4,
                    end_col: 15,
                },
            ));
            for small in [
                crate::vterm_defs::VTermRect { start_row: 0, ..big },
                crate::vterm_defs::VTermRect { end_row: 11, ..big },
                crate::vterm_defs::VTermRect { start_col: 1, ..big },
                crate::vterm_defs::VTermRect { end_col: 21, ..big },
            ] {
                assert!(!rect_contains(&big, &small));
            }
    }

    #[test]
    fn rect_intersects_counts_touching_edges_as_overlap() {
            let first = crate::vterm_defs::VTermRect {
                start_row: 0,
                end_row: 5,
                start_col: 0,
                end_col: 5,
            };
            assert!(rect_intersects(
                &first,
                &crate::vterm_defs::VTermRect {
                    start_row: 5,
                    end_row: 10,
                    start_col: 5,
                    end_col: 10,
                },
            ));
            assert!(!rect_intersects(
                &first,
                &crate::vterm_defs::VTermRect {
                    start_row: 6,
                    end_row: 10,
                    start_col: 0,
                    end_col: 5,
                },
            ));
            assert!(!rect_intersects(
                &first,
                &crate::vterm_defs::VTermRect {
                    start_row: 0,
                    end_row: 5,
                    start_col: 6,
                    end_col: 10,
                },
            ));
    }
}
