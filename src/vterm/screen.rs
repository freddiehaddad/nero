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
    pub lineinfo: [Option<Vec<crate::vterm_defs::VTermLineInfo>>; 2],
    pub active_buffer: usize,
    pub sb_buffer: Vec<crate::vterm_defs::VTermScreenCell>,
    pub pen: ScreenPen,
}

/// Screen callback surface (`VTermScreenCallbacks`).
pub trait VTermScreenCallbacks {
    fn damage(&mut self, _rect: crate::vterm_defs::VTermRect) -> bool {
        false
    }

    fn move_rect(
        &mut self,
        _destination: crate::vterm_defs::VTermRect,
        _source: crate::vterm_defs::VTermRect,
    ) -> bool {
        false
    }

    fn move_cursor(
        &mut self,
        _position: crate::vterm_defs::VTermPos,
        _old_position: crate::vterm_defs::VTermPos,
        _visible: bool,
    ) -> bool {
        false
    }

    fn bell(&mut self) -> bool {
        false
    }

    fn set_term_prop(
        &mut self,
        _prop: crate::vterm_defs::VTermProp,
        _value: &crate::vterm_defs::VTermValue<'_>,
    ) -> bool {
        false
    }

    fn resize(&mut self, _rows: i32, _cols: i32) -> bool {
        false
    }

    fn theme(&mut self, _dark: &mut bool) -> bool {
        false
    }

    fn scrollback_push(&mut self, _cells: &[crate::vterm_defs::VTermScreenCell]) -> bool {
        false
    }

    fn scrollback_pop(&mut self, _cells: &mut [crate::vterm_defs::VTermScreenCell]) -> bool {
        false
    }

    fn scrollback_clear(&mut self) -> bool {
        false
    }
}

impl VTermScreenCallbacks for () {}

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
        lineinfo: [
            Some(vec![
                crate::vterm_defs::VTermLineInfo::default();
                usize::try_from(rows).unwrap_or(0)
            ]),
            None,
        ],
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

/// Returns one active-buffer cell (`getcell`).
#[must_use]
pub fn getcell(screen: &VTermScreen, row: i32, col: i32) -> Option<&ScreenCell> {
    if row < 0 || row >= screen.rows || col < 0 || col >= screen.cols {
        return None;
    }
    let index = usize::try_from(screen.cols * row + col).ok()?;
    screen.buffers[screen.active_buffer].as_ref()?.get(index)
}

/// Mutable counterpart used by screen callbacks.
pub fn getcell_mut(
    screen: &mut VTermScreen,
    row: i32,
    col: i32,
) -> Option<&mut ScreenCell> {
    if row < 0 || row >= screen.rows || col < 0 || col >= screen.cols {
        return None;
    }
    let index = usize::try_from(screen.cols * row + col).ok()?;
    screen.buffers[screen.active_buffer].as_mut()?.get_mut(index)
}

/// Allocates and clears a screen buffer (`alloc_buffer`).
#[must_use]
pub fn alloc_buffer(screen: &VTermScreen, rows: i32, cols: i32) -> Vec<ScreenCell> {
    let cell_count = usize::try_from(rows)
        .ok()
        .and_then(|rows| usize::try_from(cols).ok().and_then(|cols| rows.checked_mul(cols)))
        .unwrap_or(0);
    vec![
        ScreenCell {
            schar: 0,
            pen: screen.pen,
        };
        cell_count
    ]
}

/// Returns the first trailing blank column (`line_popcount`).
#[must_use]
pub fn line_popcount(
    buffer: &[ScreenCell],
    row: i32,
    _rows: i32,
    cols: i32,
) -> i32 {
    let mut col = cols - 1;
    while col >= 0 {
        let index = usize::try_from(row * cols + col).expect("valid screen index");
        if buffer[index].schar != 0 {
            break;
        }
        col -= 1;
    }
    col + 1
}

/// Enables or disables screen reflow (`vterm_screen_enable_reflow`).
pub fn vterm_screen_enable_reflow(screen: &mut VTermScreen, reflow: bool) {
    screen.reflow = reflow;
}

/// Back-compatible reflow setter (`vterm_screen_set_reflow`).
pub fn vterm_screen_set_reflow(screen: &mut VTermScreen, reflow: bool) {
    vterm_screen_enable_reflow(screen, reflow);
}

/// Lazily allocates the alternate screen
/// (`vterm_screen_enable_altscreen`).
pub fn vterm_screen_enable_altscreen(screen: &mut VTermScreen, altscreen: i32) {
    if screen.buffers[1].is_none() && altscreen != 0 {
        let buffer = alloc_buffer(screen, screen.rows, screen.cols);
        screen.buffers[1] = Some(buffer);
    }
}

/// Mirrors a state pen attribute into the screen pen (`setpenattr`).
pub fn setpenattr(
    screen: &mut VTermScreen,
    attr: crate::vterm_defs::VTermAttr,
    value: &crate::vterm_defs::VTermValue<'_>,
) -> i32 {
    match (attr, value) {
        (crate::vterm_defs::VTermAttr::Bold, crate::vterm_defs::VTermValue::Boolean(value)) => {
            screen.pen.bold = *value != 0;
        }
        (
            crate::vterm_defs::VTermAttr::Underline,
            crate::vterm_defs::VTermValue::Number(value),
        ) => screen.pen.underline = *value as u8 & 0x03,
        (crate::vterm_defs::VTermAttr::Italic, crate::vterm_defs::VTermValue::Boolean(value)) => {
            screen.pen.italic = *value != 0;
        }
        (crate::vterm_defs::VTermAttr::Blink, crate::vterm_defs::VTermValue::Boolean(value)) => {
            screen.pen.blink = *value != 0;
        }
        (
            crate::vterm_defs::VTermAttr::Reverse,
            crate::vterm_defs::VTermValue::Boolean(value),
        ) => screen.pen.reverse = *value != 0,
        (
            crate::vterm_defs::VTermAttr::Conceal,
            crate::vterm_defs::VTermValue::Boolean(value),
        ) => screen.pen.conceal = *value != 0,
        (crate::vterm_defs::VTermAttr::Strike, crate::vterm_defs::VTermValue::Boolean(value)) => {
            screen.pen.strike = *value != 0;
        }
        (crate::vterm_defs::VTermAttr::Font, crate::vterm_defs::VTermValue::Number(value)) => {
            screen.pen.font = *value as u8 & 0x0F;
        }
        (
            crate::vterm_defs::VTermAttr::Foreground,
            crate::vterm_defs::VTermValue::Color(value),
        ) => screen.pen.fg = *value,
        (
            crate::vterm_defs::VTermAttr::Background,
            crate::vterm_defs::VTermValue::Color(value),
        ) => screen.pen.bg = *value,
        (crate::vterm_defs::VTermAttr::Small, crate::vterm_defs::VTermValue::Boolean(value)) => {
            screen.pen.small = *value != 0;
        }
        (
            crate::vterm_defs::VTermAttr::Baseline,
            crate::vterm_defs::VTermValue::Number(value),
        ) => screen.pen.baseline = *value as u8 & 0x03,
        (crate::vterm_defs::VTermAttr::Uri, crate::vterm_defs::VTermValue::Number(value)) => {
            screen.pen.uri = *value;
        }
        (crate::vterm_defs::VTermAttr::Dim, crate::vterm_defs::VTermValue::Boolean(value)) => {
            screen.pen.dim = *value != 0;
        }
        (
            crate::vterm_defs::VTermAttr::Overline,
            crate::vterm_defs::VTermValue::Boolean(value),
        ) => screen.pen.overline = *value != 0,
        _ => return 0,
    }
    1
}

pub fn setlineinfo<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
    row: i32,
    new_info: crate::vterm_defs::VTermLineInfo,
    old_info: crate::vterm_defs::VTermLineInfo,
) -> i32 {
    if new_info.doublewidth != old_info.doublewidth
        || new_info.doubleheight != old_info.doubleheight
    {
        for col in 0..screen.cols {
            let cell = getcell_mut(screen, row, col).expect("valid line cell");
            cell.pen.dwl = new_info.doublewidth;
            cell.pen.dhl = new_info.doubleheight;
        }
        let rect = crate::vterm_defs::VTermRect {
            start_row: row,
            end_row: row + 1,
            start_col: 0,
            end_col: if new_info.doublewidth {
                screen.cols / 2
            } else {
                screen.cols
            },
        };
        damagerect(screen, callbacks, rect);
        if new_info.doublewidth {
            let _ = erase_internal(
                screen,
                crate::vterm_defs::VTermRect {
                    start_col: screen.cols / 2,
                    end_col: screen.cols,
                    ..rect
                },
                false,
            );
        }
    }
    1
}

pub fn settermprop<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
    prop: crate::vterm_defs::VTermProp,
    value: &crate::vterm_defs::VTermValue<'_>,
) -> i32 {
    match (prop, value) {
        (
            crate::vterm_defs::VTermProp::AltScreen,
            crate::vterm_defs::VTermValue::Boolean(enabled),
        ) => {
            if *enabled != 0 && screen.buffers[1].is_none() {
                return 0;
            }
            screen.active_buffer = usize::from(*enabled != 0);
            if *enabled == 0 {
                damagescreen(screen, callbacks);
            }
        }
        (
            crate::vterm_defs::VTermProp::Reverse,
            crate::vterm_defs::VTermValue::Boolean(reverse),
        ) => {
            screen.global_reverse = *reverse != 0;
            damagescreen(screen, callbacks);
        }
        _ => {}
    }
    let _ = callbacks.set_term_prop(prop, value);
    1
}

#[cfg(test)]
mod settermprop_screen_tests {
    use super::*;
    #[test]
    fn screen_properties_switch_buffers_and_reverse() {
        let mut screen = screen_new(1, 1);
        let mut callbacks = ();
        assert_eq!(
            settermprop(
                &mut screen,
                &mut callbacks,
                crate::vterm_defs::VTermProp::AltScreen,
                &crate::vterm_defs::VTermValue::Boolean(1),
            ),
            0
        );
        vterm_screen_enable_altscreen(&mut screen, 1);
        assert_eq!(
            settermprop(
                &mut screen,
                &mut callbacks,
                crate::vterm_defs::VTermProp::AltScreen,
                &crate::vterm_defs::VTermValue::Boolean(1),
            ),
            1
        );
        assert_eq!(screen.active_buffer, 1);
        settermprop(
            &mut screen,
            &mut callbacks,
            crate::vterm_defs::VTermProp::Reverse,
            &crate::vterm_defs::VTermValue::Boolean(1),
        );
        assert!(screen.global_reverse);
    }
}

#[cfg(test)]
mod setlineinfo_screen_tests {
    use super::*;
    #[test]
    fn setlineinfo_updates_cells_and_erases_hidden_half() {
        let mut screen = screen_new(1, 4);
        for col in 0..4 {
            getcell_mut(&mut screen, 0, col).unwrap().schar = 42;
        }
        let mut callbacks = ();
        setlineinfo(
            &mut screen,
            &mut callbacks,
            0,
            crate::vterm_defs::VTermLineInfo {
                doublewidth: true,
                ..Default::default()
            },
            Default::default(),
        );
        assert!(getcell(&screen, 0, 0).unwrap().pen.dwl);
        assert_eq!(getcell(&screen, 0, 2).unwrap().schar, 0);
    }
}

/// Copies one internal cell to its public representation
/// (`vterm_screen_get_cell`).
pub fn vterm_screen_get_cell(
    screen: &VTermScreen,
    position: crate::vterm_defs::VTermPos,
    cell: &mut crate::vterm_defs::VTermScreenCell,
) -> i32 {
    let Some(internal) = getcell(screen, position.row, position.col) else {
        return 0;
    };
    cell.schar = if internal.schar == crate::types_defs::ScharT::MAX {
        0
    } else {
        internal.schar
    };
    cell.attrs.bold = internal.pen.bold;
    cell.attrs.underline = internal.pen.underline;
    cell.attrs.italic = internal.pen.italic;
    cell.attrs.blink = internal.pen.blink;
    cell.attrs.reverse = internal.pen.reverse ^ screen.global_reverse;
    cell.attrs.conceal = internal.pen.conceal;
    cell.attrs.strike = internal.pen.strike;
    cell.attrs.font = internal.pen.font;
    cell.attrs.small = internal.pen.small;
    cell.attrs.baseline = internal.pen.baseline;
    cell.attrs.dim = internal.pen.dim;
    cell.attrs.overline = internal.pen.overline;
    cell.attrs.dwl = internal.pen.dwl;
    cell.attrs.dhl = internal.pen.dhl;
    cell.fg = internal.pen.fg;
    cell.bg = internal.pen.bg;
    cell.uri = internal.pen.uri;
    cell.width = if position.col < screen.cols - 1
        && getcell(screen, position.row, position.col + 1)
            .is_some_and(|next| next.schar == crate::types_defs::ScharT::MAX)
    {
        2
    } else {
        1
    };
    1
}

fn emit_stored_damage<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
) {
    if screen.damaged.start_row != -1 {
        let _ = callbacks.damage(screen.damaged);
        screen.damaged.start_row = -1;
    }
}

/// Records or emits damage according to the merge policy
/// (`damagerect`).
pub fn damagerect<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
    rect: crate::vterm_defs::VTermRect,
) {
    let emit = match screen.damage_merge {
        crate::vterm_defs::VTermDamageSize::Cell => rect,
        crate::vterm_defs::VTermDamageSize::Row => {
            if rect.end_row > rect.start_row + 1 {
                emit_stored_damage(screen, callbacks);
                rect
            } else if screen.damaged.start_row == -1 {
                screen.damaged = rect;
                return;
            } else if rect.start_row == screen.damaged.start_row {
                screen.damaged.start_col = screen.damaged.start_col.min(rect.start_col);
                screen.damaged.end_col = screen.damaged.end_col.max(rect.end_col);
                return;
            } else {
                let emit = screen.damaged;
                screen.damaged = rect;
                emit
            }
        }
        crate::vterm_defs::VTermDamageSize::Screen
        | crate::vterm_defs::VTermDamageSize::Scroll => {
            if screen.damaged.start_row == -1 {
                screen.damaged = rect;
            } else {
                rect_expand(&mut screen.damaged, &rect);
            }
            return;
        }
        crate::vterm_defs::VTermDamageSize::NDamages => return,
    };
    let _ = callbacks.damage(emit);
}

/// Damages the whole active screen (`damagescreen`).
pub fn damagescreen<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
) {
    let rect = crate::vterm_defs::VTermRect {
        start_row: 0,
        end_row: screen.rows,
        start_col: 0,
        end_col: screen.cols,
    };
    damagerect(screen, callbacks, rect);
}

/// Writes one glyph and damages its occupied cells (`putglyph`).
pub fn putglyph<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
    info: &crate::vterm_defs::VTermGlyphInfo,
    position: crate::vterm_defs::VTermPos,
) -> i32 {
    if getcell(screen, position.row, position.col).is_none() {
        return 0;
    }
    let current_pen = screen.pen;
    {
        let cell = getcell_mut(screen, position.row, position.col).expect("checked cell");
        cell.schar = info.schar;
        if info.schar != 0 {
            cell.pen = current_pen;
        }
        cell.pen.protected_cell = info.protected_cell;
        cell.pen.dwl = info.dwl;
        cell.pen.dhl = info.dhl;
    }
    for offset in 1..info.width {
        getcell_mut(screen, position.row, position.col + offset)
            .expect("glyph width inside screen")
            .schar = crate::types_defs::ScharT::MAX;
    }
    damagerect(
        screen,
        callbacks,
        crate::vterm_defs::VTermRect {
            start_row: position.row,
            end_row: position.row + 1,
            start_col: position.col,
            end_col: position.col + info.width,
        },
    );
    1
}

#[cfg(test)]
mod putglyph_tests {
    use super::*;

    #[derive(Default)]
    struct Capture(Vec<crate::vterm_defs::VTermRect>);

    impl VTermScreenCallbacks for Capture {
        fn damage(&mut self, rect: crate::vterm_defs::VTermRect) -> bool {
            self.0.push(rect);
            true
        }
    }

    #[test]
    fn putglyph_writes_pen_continuations_and_damage() {
        let mut screen = screen_new(1, 3);
        screen.pen.bold = true;
        let mut callbacks = Capture::default();
        let info = crate::vterm_defs::VTermGlyphInfo {
            schar: 42,
            width: 2,
            protected_cell: true,
            dwl: true,
            dhl: 2,
        };
        assert_eq!(
            putglyph(
                &mut screen,
                &mut callbacks,
                &info,
                crate::vterm_defs::VTermPos { row: 0, col: 0 },
            ),
            1
        );
        let first = getcell(&screen, 0, 0).unwrap();
        assert_eq!(first.schar, 42);
        assert!(first.pen.bold);
        assert!(first.pen.protected_cell);
        assert!(first.pen.dwl);
        assert_eq!(first.pen.dhl, 2);
        assert_eq!(getcell(&screen, 0, 1).unwrap().schar, crate::types_defs::ScharT::MAX);
        assert_eq!(callbacks.0[0].end_col, 2);
    }

    #[test]
    fn putglyph_zero_retains_existing_pen_and_rejects_invalid_position() {
        let mut screen = screen_new(1, 1);
        getcell_mut(&mut screen, 0, 0).unwrap().pen.italic = true;
        screen.pen.bold = true;
        let mut callbacks = Capture::default();
        let info = crate::vterm_defs::VTermGlyphInfo {
            schar: 0,
            width: 1,
            ..Default::default()
        };
        assert_eq!(
            putglyph(
                &mut screen,
                &mut callbacks,
                &info,
                crate::vterm_defs::VTermPos { row: 0, col: 0 },
            ),
            1
        );
        assert!(getcell(&screen, 0, 0).unwrap().pen.italic);
        assert!(!getcell(&screen, 0, 0).unwrap().pen.bold);
        assert_eq!(
            putglyph(
                &mut screen,
                &mut callbacks,
                &info,
                crate::vterm_defs::VTermPos { row: 1, col: 0 },
            ),
            0
        );
    }
}

/// Copies a row to the scrollback callback (`sb_pushline_from_row`).
pub fn sb_pushline_from_row<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
    row: i32,
) {
    let mut cells = vec![
        crate::vterm_defs::VTermScreenCell::default();
        usize::try_from(screen.cols).unwrap_or(0)
    ];
    for col in 0..screen.cols {
        let _ = vterm_screen_get_cell(
            screen,
            crate::vterm_defs::VTermPos { row, col },
            &mut cells[col as usize],
        );
    }
    screen.sb_buffer.clone_from(&cells);
    let _ = callbacks.scrollback_push(&screen.sb_buffer);
}

#[cfg(test)]
mod scrollback_push_tests {
    use super::*;

    #[derive(Default)]
    struct Capture(Vec<Vec<crate::vterm_defs::VTermScreenCell>>);

    impl VTermScreenCallbacks for Capture {
        fn scrollback_push(&mut self, cells: &[crate::vterm_defs::VTermScreenCell]) -> bool {
            self.0.push(cells.to_vec());
            true
        }
    }

    #[test]
    fn scrollback_push_copies_public_row_cells() {
        let mut screen = screen_new(2, 2);
        getcell_mut(&mut screen, 1, 0).unwrap().schar = 10;
        getcell_mut(&mut screen, 1, 1).unwrap().schar = 20;
        let mut capture = Capture::default();
        sb_pushline_from_row(&mut screen, &mut capture, 1);
        assert_eq!(capture.0.len(), 1);
        assert_eq!(capture.0[0][0].schar, 10);
        assert_eq!(capture.0[0][1].schar, 20);
        assert_eq!(screen.sb_buffer, capture.0[0]);
    }
}

/// Moves cells inside the active buffer (`moverect_internal`).
pub fn moverect_internal<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
    destination: crate::vterm_defs::VTermRect,
    source: crate::vterm_defs::VTermRect,
) -> i32 {
    if destination.start_row == 0
        && destination.start_col == 0
        && destination.end_col == screen.cols
        && screen.active_buffer == 0
    {
        for row in 0..source.start_row {
            sb_pushline_from_row(screen, callbacks, row);
        }
    }

    let cols = source.end_col - source.start_col;
    let downward = source.start_row - destination.start_row;
    let rows: Box<dyn Iterator<Item = i32>> = if downward < 0 {
        Box::new((destination.start_row..destination.end_row).rev())
    } else {
        Box::new(destination.start_row..destination.end_row)
    };
    let screen_cols = screen.cols as usize;
    let buffer = screen.buffers[screen.active_buffer]
        .as_mut()
        .expect("active screen buffer");
    for row in rows {
        let source_start = ((row + downward) as usize * screen_cols)
            + source.start_col as usize;
        let destination_start =
            (row as usize * screen_cols) + destination.start_col as usize;
        buffer.copy_within(
            source_start..source_start + cols as usize,
            destination_start,
        );
    }
    1
}

#[cfg(test)]
mod moverect_internal_tests {
    use super::*;

    #[test]
    fn moverect_internal_handles_overlapping_vertical_moves() {
        let mut screen = screen_new(3, 2);
        for row in 0..3 {
            for col in 0..2 {
                getcell_mut(&mut screen, row, col).unwrap().schar =
                    (row * 10 + col + 1) as u32;
            }
        }
        let mut callbacks = ();
        assert_eq!(
            moverect_internal(
                &mut screen,
                &mut callbacks,
                crate::vterm_defs::VTermRect {
                    start_row: 1,
                    end_row: 3,
                    start_col: 0,
                    end_col: 2,
                },
                crate::vterm_defs::VTermRect {
                    start_row: 0,
                    end_row: 2,
                    start_col: 0,
                    end_col: 2,
                },
            ),
            1
        );
        assert_eq!(getcell(&screen, 1, 0).unwrap().schar, 1);
        assert_eq!(getcell(&screen, 2, 0).unwrap().schar, 11);
    }
}

/// Emits a user move callback or damages the destination
/// (`moverect_user`).
pub fn moverect_user<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
    destination: crate::vterm_defs::VTermRect,
    source: crate::vterm_defs::VTermRect,
) -> i32 {
    if screen.damage_merge != crate::vterm_defs::VTermDamageSize::Scroll {
        vterm_screen_flush_damage(screen, callbacks);
    }
    if callbacks.move_rect(destination, source) {
        return 1;
    }
    damagerect(screen, callbacks, destination);
    1
}

#[cfg(test)]
mod moverect_user_tests {
    use super::*;

    #[derive(Default)]
    struct Capture {
        handle: bool,
        moves: usize,
        damage: Vec<crate::vterm_defs::VTermRect>,
    }

    impl VTermScreenCallbacks for Capture {
        fn move_rect(
            &mut self,
            _destination: crate::vterm_defs::VTermRect,
            _source: crate::vterm_defs::VTermRect,
        ) -> bool {
            self.moves += 1;
            self.handle
        }

        fn damage(&mut self, rect: crate::vterm_defs::VTermRect) -> bool {
            self.damage.push(rect);
            true
        }
    }

    #[test]
    fn moverect_user_uses_callback_then_damage_fallback() {
        let mut screen = screen_new(3, 3);
        let destination = crate::vterm_defs::VTermRect {
            start_row: 0,
            end_row: 1,
            start_col: 0,
            end_col: 1,
        };
        let source = crate::vterm_defs::VTermRect {
            start_row: 1,
            end_row: 2,
            ..destination
        };
        let mut capture = Capture::default();
        assert_eq!(
            moverect_user(&mut screen, &mut capture, destination, source),
            1
        );
        assert_eq!(capture.moves, 1);
        assert_eq!(capture.damage, [destination]);

        capture.handle = true;
        capture.damage.clear();
        moverect_user(&mut screen, &mut capture, destination, source);
        assert_eq!(capture.moves, 2);
        assert!(capture.damage.is_empty());
    }
}

/// Clears cells inside a rectangle (`erase_internal`).
pub fn erase_internal(
    screen: &mut VTermScreen,
    rect: crate::vterm_defs::VTermRect,
    selective: bool,
) -> i32 {
    let foreground = screen.pen.fg;
    let background = screen.pen.bg;
    for row in rect.start_row..screen.rows.min(rect.end_row) {
        let info = screen.lineinfo[screen.active_buffer]
            .as_ref()
            .and_then(|rows| rows.get(row as usize))
            .copied()
            .unwrap_or_default();
        for col in rect.start_col..rect.end_col {
            let Some(cell) = getcell_mut(screen, row, col) else {
                continue;
            };
            if selective && cell.pen.protected_cell {
                continue;
            }
            cell.schar = 0;
            cell.pen = ScreenPen {
                fg: foreground,
                bg: background,
                dwl: info.doublewidth,
                dhl: info.doubleheight,
                ..Default::default()
            };
        }
    }
    1
}

#[cfg(test)]
mod erase_internal_tests {
    use super::*;

    #[test]
    fn erase_internal_resets_cells_preserves_protected_and_line_layout() {
        let mut screen = screen_new(1, 2);
        screen.pen.fg.red = 1;
        screen.pen.bg.blue = 2;
        screen.lineinfo[0].as_mut().unwrap()[0] = crate::vterm_defs::VTermLineInfo {
            doublewidth: true,
            doubleheight: 2,
            ..Default::default()
        };
        for col in 0..2 {
            let cell = getcell_mut(&mut screen, 0, col).unwrap();
            cell.schar = 42;
            cell.pen.bold = true;
        }
        getcell_mut(&mut screen, 0, 1).unwrap().pen.protected_cell = true;
        let rect = crate::vterm_defs::VTermRect {
            start_row: 0,
            end_row: 1,
            start_col: 0,
            end_col: 2,
        };
        assert_eq!(erase_internal(&mut screen, rect, true), 1);
        let first = getcell(&screen, 0, 0).unwrap();
        assert_eq!(first.schar, 0);
        assert!(!first.pen.bold);
        assert_eq!(first.pen.fg.red, 1);
        assert_eq!(first.pen.bg.blue, 2);
        assert!(first.pen.dwl);
        assert_eq!(first.pen.dhl, 2);
        assert_eq!(getcell(&screen, 0, 1).unwrap().schar, 42);
    }
}

/// Reports an erased rectangle as damage (`erase_user`).
pub fn erase_user<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
    rect: crate::vterm_defs::VTermRect,
    _selective: bool,
) -> i32 {
    damagerect(screen, callbacks, rect);
    1
}

#[cfg(test)]
mod erase_user_tests {
    use super::*;

    #[derive(Default)]
    struct Capture(Vec<crate::vterm_defs::VTermRect>);

    impl VTermScreenCallbacks for Capture {
        fn damage(&mut self, rect: crate::vterm_defs::VTermRect) -> bool {
            self.0.push(rect);
            true
        }
    }

    #[test]
    fn erase_user_reports_damage_and_ignores_selective_flag() {
        let mut screen = screen_new(2, 2);
        let rect = crate::vterm_defs::VTermRect {
            start_row: 0,
            end_row: 1,
            start_col: 0,
            end_col: 2,
        };
        let mut capture = Capture::default();
        assert_eq!(erase_user(&mut screen, &mut capture, rect, true), 1);
        assert_eq!(capture.0, [rect]);
    }
}

/// Erases cells then reports non-selective damage (`erase`).
pub fn erase<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
    rect: crate::vterm_defs::VTermRect,
    selective: bool,
) -> i32 {
    let _ = erase_internal(screen, rect, selective);
    erase_user(screen, callbacks, rect, false)
}

#[cfg(test)]
mod erase_tests {
    use super::*;

    #[derive(Default)]
    struct Capture(Vec<crate::vterm_defs::VTermRect>);

    impl VTermScreenCallbacks for Capture {
        fn damage(&mut self, rect: crate::vterm_defs::VTermRect) -> bool {
            self.0.push(rect);
            true
        }
    }

    #[test]
    fn erase_combines_internal_selectivity_with_unconditional_damage() {
        let mut screen = screen_new(1, 1);
        let cell = getcell_mut(&mut screen, 0, 0).unwrap();
        cell.schar = 42;
        cell.pen.protected_cell = true;
        let rect = crate::vterm_defs::VTermRect {
            start_row: 0,
            end_row: 1,
            start_col: 0,
            end_col: 1,
        };
        let mut capture = Capture::default();
        assert_eq!(erase(&mut screen, &mut capture, rect, true), 1);
        assert_eq!(getcell(&screen, 0, 0).unwrap().schar, 42);
        assert_eq!(capture.0, [rect]);
    }
}

/// Forwards cursor movement to the screen callback (`movecursor`).
pub fn movecursor<C: VTermScreenCallbacks>(
    callbacks: &mut C,
    position: crate::vterm_defs::VTermPos,
    old_position: crate::vterm_defs::VTermPos,
    visible: bool,
) -> i32 {
    i32::from(callbacks.move_cursor(position, old_position, visible))
}

#[cfg(test)]
mod movecursor_tests {
    use super::*;
    struct Capture(bool);
    impl VTermScreenCallbacks for Capture {
        fn move_cursor(
            &mut self,
            _: crate::vterm_defs::VTermPos,
            _: crate::vterm_defs::VTermPos,
            visible: bool,
        ) -> bool {
            self.0 = visible;
            true
        }
    }
    #[test]
    fn movecursor_returns_callback_result() {
        assert_eq!(movecursor(&mut (), Default::default(), Default::default(), true), 0);
        let mut capture = Capture(false);
        assert_eq!(movecursor(&mut capture, Default::default(), Default::default(), true), 1);
        assert!(capture.0);
    }
}

/// Forwards a terminal bell (`bell`).
pub fn bell<C: VTermScreenCallbacks>(callbacks: &mut C) -> i32 {
    i32::from(callbacks.bell())
}

#[cfg(test)]
mod bell_tests {
    use super::*;
    struct Capture;
    impl VTermScreenCallbacks for Capture {
        fn bell(&mut self) -> bool { true }
    }
    #[test]
    fn bell_returns_callback_result() {
        assert_eq!(bell(&mut ()), 0);
        assert_eq!(bell(&mut Capture), 1);
    }
}

/// Forwards a theme query, succeeding when no callback exists (`theme`).
pub fn theme<C: VTermScreenCallbacks>(
    callbacks: Option<&mut C>,
    dark: &mut bool,
) -> i32 {
    callbacks.map_or(1, |callbacks| i32::from(callbacks.theme(dark)))
}

#[cfg(test)]
mod theme_tests {
    use super::*;
    struct Capture;
    impl VTermScreenCallbacks for Capture {
        fn theme(&mut self, dark: &mut bool) -> bool {
            *dark = true;
            true
        }
    }
    #[test]
    fn theme_defaults_to_success_and_forwards_callback() {
        let mut dark = false;
        assert_eq!(theme::<Capture>(None, &mut dark), 1);
        assert!(!dark);
        assert_eq!(theme(Some(&mut Capture), &mut dark), 1);
        assert!(dark);
    }
}

/// Forwards scrollback clearing (`sb_clear`).
pub fn sb_clear<C: VTermScreenCallbacks>(callbacks: Option<&mut C>) -> i32 {
    callbacks.map_or(0, |callbacks| i32::from(callbacks.scrollback_clear()))
}

#[cfg(test)]
mod sb_clear_tests {
    use super::*;
    struct Capture;
    impl VTermScreenCallbacks for Capture {
        fn scrollback_clear(&mut self) -> bool { true }
    }
    #[test]
    fn sb_clear_returns_callback_result_or_zero() {
        assert_eq!(sb_clear::<Capture>(None), 0);
        assert_eq!(sb_clear(Some(&mut Capture)), 1);
    }
}

/// Flushes accumulated damage (`vterm_screen_flush_damage`, damage
/// portion; pending scroll emission is translated with scrollrect).
pub fn vterm_screen_flush_damage<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
) {
    emit_stored_damage(screen, callbacks);
}

/// Flushes pending damage and changes its merge policy
/// (`vterm_screen_set_damage_merge`).
pub fn vterm_screen_set_damage_merge<C: VTermScreenCallbacks>(
    screen: &mut VTermScreen,
    callbacks: &mut C,
    size: crate::vterm_defs::VTermDamageSize,
) {
    vterm_screen_flush_damage(screen, callbacks);
    screen.damage_merge = size;
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

    #[derive(Default)]
    struct DamageCapture(Vec<crate::vterm_defs::VTermRect>);

    impl VTermScreenCallbacks for DamageCapture {
        fn damage(&mut self, rect: crate::vterm_defs::VTermRect) -> bool {
            self.0.push(rect);
            true
        }
    }

    #[test]
    fn default_screen_callbacks_decline_every_event() {
        let callbacks = &mut ();
        assert!(!callbacks.damage(crate::vterm_defs::VTermRect::default()));
        assert!(!callbacks.move_rect(
            crate::vterm_defs::VTermRect::default(),
            crate::vterm_defs::VTermRect::default(),
        ));
        assert!(!callbacks.move_cursor(
            crate::vterm_defs::VTermPos::default(),
            crate::vterm_defs::VTermPos::default(),
            true,
        ));
        assert!(!callbacks.bell());
        assert!(!callbacks.set_term_prop(
            crate::vterm_defs::VTermProp::Reverse,
            &crate::vterm_defs::VTermValue::Boolean(1),
        ));
        assert!(!callbacks.resize(24, 80));
        let mut dark = false;
        assert!(!callbacks.theme(&mut dark));
        assert!(!callbacks.scrollback_push(&[]));
        assert!(!callbacks.scrollback_pop(&mut []));
        assert!(!callbacks.scrollback_clear());
    }

    #[test]
    fn damagerect_emits_cells_and_accumulates_screen_damage() {
        let first = crate::vterm_defs::VTermRect {
            start_row: 1,
            end_row: 2,
            start_col: 3,
            end_col: 4,
        };
        let second = crate::vterm_defs::VTermRect {
            start_row: 0,
            end_row: 4,
            start_col: 5,
            end_col: 8,
        };
        let mut screen = screen_new(5, 10);
        let mut capture = DamageCapture::default();
        damagerect(&mut screen, &mut capture, first);
        assert_eq!(capture.0, [first]);

        screen.damage_merge = crate::vterm_defs::VTermDamageSize::Screen;
        damagerect(&mut screen, &mut capture, first);
        damagerect(&mut screen, &mut capture, second);
        assert_eq!(capture.0, [first]);
        assert_eq!(screen.damaged, crate::vterm_defs::VTermRect {
            start_row: 0,
            end_row: 4,
            start_col: 3,
            end_col: 8,
        });
    }

    #[test]
    fn damagerect_merges_same_row_and_rotates_different_rows() {
        let mut screen = screen_new(5, 10);
        screen.damage_merge = crate::vterm_defs::VTermDamageSize::Row;
        let mut capture = DamageCapture::default();
        damagerect(
            &mut screen,
            &mut capture,
            crate::vterm_defs::VTermRect {
                start_row: 1,
                end_row: 2,
                start_col: 4,
                end_col: 6,
            },
        );
        damagerect(
            &mut screen,
            &mut capture,
            crate::vterm_defs::VTermRect {
                start_row: 1,
                end_row: 2,
                start_col: 2,
                end_col: 8,
            },
        );
        assert_eq!((screen.damaged.start_col, screen.damaged.end_col), (2, 8));
        damagerect(
            &mut screen,
            &mut capture,
            crate::vterm_defs::VTermRect {
                start_row: 2,
                end_row: 3,
                start_col: 0,
                end_col: 1,
            },
        );
        assert_eq!(capture.0.len(), 1);
        assert_eq!(capture.0[0].start_row, 1);
        assert_eq!(screen.damaged.start_row, 2);
    }

    #[test]
    fn damagescreen_uses_full_current_dimensions() {
        let mut screen = screen_new(24, 80);
        let mut capture = DamageCapture::default();
        damagescreen(&mut screen, &mut capture);
        assert_eq!(capture.0, [crate::vterm_defs::VTermRect {
            start_row: 0,
            end_row: 24,
            start_col: 0,
            end_col: 80,
        }]);
    }

    #[test]
    fn screen_flush_damage_emits_once_and_resets_sentinel() {
        let mut screen = screen_new(5, 10);
        screen.damage_merge = crate::vterm_defs::VTermDamageSize::Screen;
        let mut capture = DamageCapture::default();
        let rect = crate::vterm_defs::VTermRect {
            start_row: 1,
            end_row: 3,
            start_col: 2,
            end_col: 4,
        };
        damagerect(&mut screen, &mut capture, rect);
        assert!(capture.0.is_empty());
        vterm_screen_flush_damage(&mut screen, &mut capture);
        assert_eq!(capture.0, [rect]);
        assert_eq!(screen.damaged.start_row, -1);
        vterm_screen_flush_damage(&mut screen, &mut capture);
        assert_eq!(capture.0, [rect]);
    }

    #[test]
    fn screen_set_damage_merge_flushes_before_changing_policy() {
        let mut screen = screen_new(5, 10);
        screen.damage_merge = crate::vterm_defs::VTermDamageSize::Screen;
        let mut capture = DamageCapture::default();
        let rect = crate::vterm_defs::VTermRect {
            start_row: 1,
            end_row: 2,
            start_col: 3,
            end_col: 4,
        };
        damagerect(&mut screen, &mut capture, rect);
        vterm_screen_set_damage_merge(
            &mut screen,
            &mut capture,
            crate::vterm_defs::VTermDamageSize::Row,
        );
        assert_eq!(capture.0, [rect]);
        assert_eq!(screen.damaged.start_row, -1);
        assert_eq!(screen.damage_merge, crate::vterm_defs::VTermDamageSize::Row);
    }

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
        assert_eq!(screen.lineinfo[0].as_ref().unwrap().len(), 3);
        assert!(screen.lineinfo[1].is_none());
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
    fn getcell_indexes_active_buffer_and_rejects_out_of_bounds() {
        let mut screen = screen_new(2, 3);
        getcell_mut(&mut screen, 1, 2).unwrap().schar = 42;
        assert_eq!(getcell(&screen, 1, 2).unwrap().schar, 42);
        for (row, col) in [(-1, 0), (0, -1), (2, 0), (0, 3)] {
            assert!(getcell(&screen, row, col).is_none());
            assert!(getcell_mut(&mut screen, row, col).is_none());
        }
    }

    #[test]
    fn alloc_buffer_clears_every_cell_with_current_pen() {
            let mut screen = screen_new(1, 1);
            screen.pen.italic = true;
            screen.pen.uri = 7;
            let buffer = alloc_buffer(&screen, 2, 3);
            assert_eq!(buffer.len(), 6);
            assert!(buffer.iter().all(|cell| {
                cell.schar == 0 && cell.pen == screen.pen
            }));
            assert!(alloc_buffer(&screen, -1, 3).is_empty());
    }

    #[test]
    fn line_popcount_finds_first_trailing_blank_column() {
        let mut buffer = vec![ScreenCell::default(); 8];
        assert_eq!(line_popcount(&buffer, 0, 2, 4), 0);
        buffer[0].schar = 1;
        assert_eq!(line_popcount(&buffer, 0, 2, 4), 1);
        buffer[2].schar = 1;
        assert_eq!(line_popcount(&buffer, 0, 2, 4), 3);
        buffer[7].schar = 1;
        assert_eq!(line_popcount(&buffer, 1, 2, 4), 4);
    }

    #[test]
    fn screen_enable_reflow_tracks_requested_state() {
        let mut screen = screen_new(2, 3);
        assert!(!screen.reflow);
        vterm_screen_enable_reflow(&mut screen, true);
        assert!(screen.reflow);
        vterm_screen_enable_reflow(&mut screen, false);
        assert!(!screen.reflow);
    }

    #[test]
    fn screen_set_reflow_alias_forwards_to_enable_reflow() {
        let mut screen = screen_new(2, 3);
        vterm_screen_set_reflow(&mut screen, true);
        assert!(screen.reflow);
        vterm_screen_set_reflow(&mut screen, false);
        assert!(!screen.reflow);
    }

    #[test]
    fn screen_enable_altscreen_allocates_once_without_switching_active_buffer() {
        let mut screen = screen_new(2, 3);
        vterm_screen_enable_altscreen(&mut screen, 0);
        assert!(screen.buffers[1].is_none());
        vterm_screen_enable_altscreen(&mut screen, 1);
        assert_eq!(screen.buffers[1].as_ref().unwrap().len(), 6);
        assert_eq!(screen.active_buffer, 0);
        screen.buffers[1].as_mut().unwrap()[0].schar = 42;
        vterm_screen_enable_altscreen(&mut screen, 1);
        assert_eq!(screen.buffers[1].as_ref().unwrap()[0].schar, 42);
    }

    #[test]
    fn screen_setpenattr_updates_all_typed_pen_fields() {
        let mut screen = screen_new(1, 1);
        let color = crate::vterm_defs::VTermColor {
            red: 1,
            green: 2,
            blue: 3,
            ..Default::default()
        };
        for (attr, value) in [
            (
                crate::vterm_defs::VTermAttr::Bold,
                crate::vterm_defs::VTermValue::Boolean(1),
            ),
            (
                crate::vterm_defs::VTermAttr::Underline,
                crate::vterm_defs::VTermValue::Number(6),
            ),
            (
                crate::vterm_defs::VTermAttr::Font,
                crate::vterm_defs::VTermValue::Number(18),
            ),
            (
                crate::vterm_defs::VTermAttr::Foreground,
                crate::vterm_defs::VTermValue::Color(color),
            ),
            (
                crate::vterm_defs::VTermAttr::Uri,
                crate::vterm_defs::VTermValue::Number(42),
            ),
        ] {
            assert_eq!(setpenattr(&mut screen, attr, &value), 1);
        }
        assert!(screen.pen.bold);
        assert_eq!(screen.pen.underline, 2);
        assert_eq!(screen.pen.font, 2);
        assert_eq!(screen.pen.fg, color);
        assert_eq!(screen.pen.uri, 42);
        assert_eq!(
            setpenattr(
                &mut screen,
                crate::vterm_defs::VTermAttr::NAttrs,
                &crate::vterm_defs::VTermValue::Number(0),
            ),
            0
        );
    }

    #[test]
    fn screen_get_cell_copies_pen_attributes_and_detects_wide_cells() {
        let mut screen = screen_new(1, 3);
        screen.global_reverse = true;
        let cell = getcell_mut(&mut screen, 0, 0).unwrap();
        cell.schar = 42;
        cell.pen = ScreenPen {
            bold: true,
            underline: 3,
            reverse: true,
            dwl: true,
            dhl: 2,
            uri: 7,
            ..Default::default()
        };
        getcell_mut(&mut screen, 0, 1).unwrap().schar = crate::types_defs::ScharT::MAX;
        let mut external = crate::vterm_defs::VTermScreenCell::default();
        assert_eq!(
            vterm_screen_get_cell(
                &screen,
                crate::vterm_defs::VTermPos { row: 0, col: 0 },
                &mut external,
            ),
            1
        );
        assert_eq!(external.schar, 42);
        assert!(external.attrs.bold);
        assert_eq!(external.attrs.underline, 3);
        assert!(!external.attrs.reverse);
        assert!(external.attrs.dwl);
        assert_eq!(external.attrs.dhl, 2);
        assert_eq!(external.uri, 7);
        assert_eq!(external.width, 2);
    }

    #[test]
    fn screen_get_cell_hides_continuation_and_rejects_invalid_position() {
        let mut screen = screen_new(1, 1);
        getcell_mut(&mut screen, 0, 0).unwrap().schar = crate::types_defs::ScharT::MAX;
        let mut external = crate::vterm_defs::VTermScreenCell::default();
        assert_eq!(
            vterm_screen_get_cell(
                &screen,
                crate::vterm_defs::VTermPos { row: 0, col: 0 },
                &mut external,
            ),
            1
        );
        assert_eq!(external.schar, 0);
        assert_eq!(external.width, 1);
        assert_eq!(
            vterm_screen_get_cell(
                &screen,
                crate::vterm_defs::VTermPos { row: 1, col: 0 },
                &mut external,
            ),
            0
        );
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
