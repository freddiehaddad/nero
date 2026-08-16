//! Translated from `src/nvim/vterm/state.c`.

/// Primary Device Attributes response (`vterm_primary_device_attr`).
pub static VTERM_PRIMARY_DEVICE_ATTR: &[u8] = b"61;22;52";

pub const NO_FORCE: i32 = 0;
pub const FORCE: i32 = 1;
pub const DWL_OFF: i32 = 0;
pub const DWL_ON: i32 = 1;
pub const DHL_OFF: i32 = 0;
pub const DHL_TOP: i32 = 1;
pub const DHL_BOTTOM: i32 = 2;

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

/// Temporary selection decode fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermSelectionTemp {
    pub mask: u16,
    pub state: VTermSelectionState,
    pub recv_partial: u32,
    pub send_partial: u32,
}

/// Core geometry and cursor fields of `VTermState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VTermState {
    pub rows: i32,
    pub cols: i32,
    pub pos: crate::vterm_defs::VTermPos,
    pub at_phantom: bool,
    pub scrollregion_top: i32,
    pub scrollregion_bottom: i32,
    pub scrollregion_left: i32,
    pub scrollregion_right: i32,
    pub tabstops: Vec<u8>,
    pub lineinfos: [Vec<crate::vterm_defs::VTermLineInfo>; 2],
    pub active_lineinfo: usize,
    pub mode: VTermStateMode,
}

/// State callback surface (`VTermStateCallbacks`).
pub trait VTermStateCallbacks {
    fn put_glyph(
        &mut self,
        _info: &crate::vterm_defs::VTermGlyphInfo,
        _position: crate::vterm_defs::VTermPos,
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

    fn scroll_rect(
        &mut self,
        _rect: crate::vterm_defs::VTermRect,
        _downward: i32,
        _rightward: i32,
    ) -> bool {
        false
    }

    fn move_rect(
        &mut self,
        _destination: crate::vterm_defs::VTermRect,
        _source: crate::vterm_defs::VTermRect,
    ) -> bool {
        false
    }

    fn erase(&mut self, _rect: crate::vterm_defs::VTermRect, _selective: bool) -> bool {
        false
    }

    fn init_pen(&mut self) -> bool {
        false
    }

    fn set_line_info(
        &mut self,
        _row: i32,
        _new_info: &crate::vterm_defs::VTermLineInfo,
        _old_info: &crate::vterm_defs::VTermLineInfo,
    ) -> bool {
        false
    }

    fn set_term_prop(
        &mut self,
        _prop: crate::vterm_defs::VTermProp,
        _value: &crate::vterm_defs::VTermValue<'_>,
    ) -> bool {
        false
    }
}

impl VTermStateCallbacks for () {}

/// Emits one glyph through state callbacks (`putglyph`).
pub fn putglyph<C: VTermStateCallbacks>(
    state: &VTermState,
    callbacks: &mut C,
    schar: crate::types_defs::ScharT,
    width: i32,
    position: crate::vterm_defs::VTermPos,
    protected_cell: bool,
) {
    let lineinfo = state.lineinfos[state.active_lineinfo][position.row as usize];
    let info = crate::vterm_defs::VTermGlyphInfo {
        schar,
        width,
        protected_cell,
        dwl: lineinfo.doublewidth,
        dhl: lineinfo.doubleheight,
    };
    let _ = callbacks.put_glyph(&info, position);
}

/// Notifies cursor movement and optionally clears phantom state
/// (`updatecursor`).
pub fn updatecursor<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    old_position: crate::vterm_defs::VTermPos,
    cancel_phantom: bool,
) {
    if state.pos == old_position {
        return;
    }
    if cancel_phantom {
        state.at_phantom = false;
    }
    let _ = callbacks.move_cursor(state.pos, old_position, state.mode.cursor_visible);
}

/// Erases through callbacks and clears following-line continuation
/// markers when erasing line ends (`erase`).
pub fn erase<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    rect: crate::vterm_defs::VTermRect,
    selective: bool,
) {
    if rect.end_col == state.cols {
        for row in rect.start_row + 1..(rect.end_row + 1).min(state.rows) {
            state.lineinfos[state.active_lineinfo][row as usize].continuation = false;
        }
    }
    let _ = callbacks.erase(rect, selective);
}

/// Scrolls a state rectangle and its line metadata (`scroll`).
pub fn scroll<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    rect: crate::vterm_defs::VTermRect,
    mut downward: i32,
    mut rightward: i32,
) {
    if downward == 0 && rightward == 0 {
        return;
    }
    let rows = rect.end_row - rect.start_row;
    downward = downward.clamp(-rows, rows);
    let cols = rect.end_col - rect.start_col;
    rightward = rightward.clamp(-cols, cols);

    if rect.start_col == 0 && rect.end_col == state.cols && rightward == 0 {
        let lineinfo = &mut state.lineinfos[state.active_lineinfo];
        if downward > 0 {
            lineinfo.copy_within(
                (rect.start_row + downward) as usize..rect.end_row as usize,
                rect.start_row as usize,
            );
            lineinfo[(rect.end_row - downward) as usize..rect.end_row as usize]
                .fill(Default::default());
        } else if downward < 0 {
            lineinfo.copy_within(
                rect.start_row as usize..(rect.end_row + downward) as usize,
                (rect.start_row - downward) as usize,
            );
            lineinfo[rect.start_row as usize..(rect.start_row - downward) as usize]
                .fill(Default::default());
        }
    }

    if callbacks.scroll_rect(rect, downward, rightward) {
        return;
    }
    let mut movement = None;
    let mut erasure = None;
    crate::vterm::core::vterm_scroll_rect(
        rect,
        downward,
        rightward,
        Some(&mut |destination, source| movement = Some((destination, source))),
        &mut |area, _| erasure = Some(area),
    );
    if let Some((destination, source)) = movement {
        let _ = callbacks.move_rect(destination, source);
    }
    if let Some(area) = erasure {
        let _ = callbacks.erase(area, false);
    }
}

/// Advances one row or scrolls at the bottom margin (`linefeed`).
pub fn linefeed<C: VTermStateCallbacks>(state: &mut VTermState, callbacks: &mut C) {
    if state.pos.row == state.scrollregion_bottom() - 1 {
        let rect = crate::vterm_defs::VTermRect {
            start_row: state.scrollregion_top,
            end_row: state.scrollregion_bottom(),
            start_col: state.scrollregion_left(),
            end_col: state.scrollregion_right(),
        };
        scroll(state, callbacks, rect, 1, 0);
    } else if state.pos.row < state.rows - 1 {
        state.pos.row += 1;
    }
}

/// Emits a boolean terminal property (`settermprop_bool`).
pub fn settermprop_bool<C: VTermStateCallbacks>(
    callbacks: &mut C,
    prop: crate::vterm_defs::VTermProp,
    value: i32,
) -> i32 {
    i32::from(callbacks.set_term_prop(
        prop,
        &crate::vterm_defs::VTermValue::Boolean(value),
    ))
}

/// Emits an integer terminal property (`settermprop_int`).
pub fn settermprop_int<C: VTermStateCallbacks>(
    callbacks: &mut C,
    prop: crate::vterm_defs::VTermProp,
    value: i32,
) -> i32 {
    i32::from(callbacks.set_term_prop(
        prop,
        &crate::vterm_defs::VTermValue::Number(value),
    ))
}

/// Emits a string terminal property (`settermprop_string`).
pub fn settermprop_string<C: VTermStateCallbacks>(
    callbacks: &mut C,
    prop: crate::vterm_defs::VTermProp,
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
) -> i32 {
    i32::from(callbacks.set_term_prop(
        prop,
        &crate::vterm_defs::VTermValue::String(fragment),
    ))
}

#[cfg(test)]
mod termprop_string_tests {
    use super::*;

    #[test]
    fn settermprop_string_forwards_fragment() {
        struct Capture(Vec<u8>);
        impl VTermStateCallbacks for Capture {
            fn set_term_prop(
                &mut self,
                _: crate::vterm_defs::VTermProp,
                value: &crate::vterm_defs::VTermValue<'_>,
            ) -> bool {
                let crate::vterm_defs::VTermValue::String(fragment) = value else {
                    return false;
                };
                self.0 = fragment.bytes.to_vec();
                true
            }
        }
        let mut capture = Capture(Vec::new());
        assert_eq!(
            settermprop_string(
                &mut capture,
                crate::vterm_defs::VTermProp::Title,
                crate::vterm_defs::VTermStringFragment {
                    bytes: b"title",
                    initial: true,
                    final_fragment: true,
                    terminator: crate::vterm_defs::VTermTerminator::Bel,
                },
            ),
            1
        );
        assert_eq!(capture.0, b"title");
    }
}

impl VTermState {
    #[must_use]
    pub fn new(rows: i32, cols: i32) -> Self {
        Self {
            rows,
            cols,
            pos: crate::vterm_defs::VTermPos::default(),
            at_phantom: false,
            scrollregion_top: 0,
            scrollregion_bottom: 0,
            scrollregion_left: 0,
            scrollregion_right: 0,
            tabstops: vec![0; usize::try_from(cols).unwrap_or(0).div_ceil(8)],
            lineinfos: [
                vec![
                    crate::vterm_defs::VTermLineInfo::default();
                    usize::try_from(rows).unwrap_or(0)
                ],
                vec![
                    crate::vterm_defs::VTermLineInfo::default();
                    usize::try_from(rows).unwrap_or(0)
                ],
            ],
            active_lineinfo: 0,
            mode: VTermStateMode::default(),
        }
    }

    /// Effective bottom edge (`SCROLLREGION_BOTTOM`).
    #[must_use]
    pub const fn scrollregion_bottom(&self) -> i32 {
        if self.scrollregion_bottom > -1 {
            self.scrollregion_bottom
        } else {
            self.rows
        }
    }

    /// Effective left edge (`SCROLLREGION_LEFT`).
    #[must_use]
    pub const fn scrollregion_left(&self) -> i32 {
        if self.mode.leftrightmargin {
            self.scrollregion_left
        } else {
            0
        }
    }

    /// Effective right edge (`SCROLLREGION_RIGHT`).
    #[must_use]
    pub const fn scrollregion_right(&self) -> i32 {
        if self.mode.leftrightmargin && self.scrollregion_right > -1 {
            self.scrollregion_right
        } else {
            self.cols
        }
    }

    /// Width of one terminal row (`ROWWIDTH`).
    #[must_use]
    pub fn row_width(&self, row: i32) -> i32 {
        if self.lineinfos[self.active_lineinfo][row as usize].doublewidth {
            self.cols / 2
        } else {
            self.cols
        }
    }

    /// Width of the cursor's current row (`THISROWWIDTH`).
    #[must_use]
    pub fn current_row_width(&self) -> i32 {
        self.row_width(self.pos.row)
    }

    /// Sets one tab stop bit (`set_col_tabstop`).
    pub fn set_col_tabstop(&mut self, col: i32) {
        let mask = 1u8 << (col & 7);
        self.tabstops[(col >> 3) as usize] |= mask;
    }

    /// Clears one tab stop bit (`clear_col_tabstop`).
    pub fn clear_col_tabstop(&mut self, col: i32) {
        let mask = 1u8 << (col & 7);
        self.tabstops[(col >> 3) as usize] &= !mask;
    }

    /// Tests one tab stop bit (`is_col_tabstop`).
    #[must_use]
    pub fn is_col_tabstop(&self, col: i32) -> bool {
        let mask = 1u8 << (col & 7);
        self.tabstops[(col >> 3) as usize] & mask != 0
    }

    /// Whether the cursor lies inside the effective scroll region
    /// (`is_cursor_in_scrollregion`).
    #[must_use]
    pub fn is_cursor_in_scrollregion(&self) -> bool {
        self.pos.row >= self.scrollregion_top
            && self.pos.row < self.scrollregion_bottom()
            && self.pos.col >= self.scrollregion_left()
            && self.pos.col < self.scrollregion_right()
    }

    /// Moves across configured tab stops (`tab`).
    pub fn tab(&mut self, mut count: i32, direction: i32) {
        while count > 0 {
            if direction > 0 {
                if self.pos.col >= self.current_row_width() - 1 {
                    return;
                }
                self.pos.col += 1;
            } else if direction < 0 {
                if self.pos.col < 1 {
                    return;
                }
                self.pos.col -= 1;
            }
            if self.is_col_tabstop(self.pos.col) {
                count -= 1;
            }
        }
    }

    /// Updates row double-width/height metadata (`set_lineinfo`).
    pub fn set_lineinfo(
        &mut self,
        row: i32,
        force: i32,
        dwl: i32,
        dhl: i32,
        accept: impl FnOnce(
            i32,
            &crate::vterm_defs::VTermLineInfo,
            &crate::vterm_defs::VTermLineInfo,
        ) -> bool,
    ) {
        let old = self.lineinfos[self.active_lineinfo][row as usize];
        let mut info = old;
        if dwl == DWL_OFF {
            info.doublewidth = false;
        } else if dwl == DWL_ON {
            info.doublewidth = true;
        }
        if dhl == DHL_OFF {
            info.doubleheight = 0;
        } else if dhl == DHL_TOP {
            info.doubleheight = 1;
        } else if dhl == DHL_BOTTOM {
            info.doubleheight = 2;
        }
        if accept(row, &info, &old) || force != 0 {
            self.lineinfos[self.active_lineinfo][row as usize] = info;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_callbacks_decline_events() {
        let callbacks = &mut ();
        assert!(!callbacks.put_glyph(&Default::default(), Default::default()));
        assert!(!callbacks.move_cursor(Default::default(), Default::default(), true));
        assert!(!callbacks.scroll_rect(Default::default(), 1, 0));
        assert!(!callbacks.move_rect(Default::default(), Default::default()));
        assert!(!callbacks.erase(Default::default(), false));
        assert!(!callbacks.init_pen());
        assert!(!callbacks.set_line_info(0, &Default::default(), &Default::default()));
        assert!(!callbacks.set_term_prop(
            crate::vterm_defs::VTermProp::CursorVisible,
            &crate::vterm_defs::VTermValue::Boolean(1),
        ));
    }

    #[test]
    fn state_putglyph_builds_info_from_line_metadata() {
        struct Capture(Option<crate::vterm_defs::VTermGlyphInfo>);
        impl VTermStateCallbacks for Capture {
            fn put_glyph(
                &mut self,
                info: &crate::vterm_defs::VTermGlyphInfo,
                _: crate::vterm_defs::VTermPos,
            ) -> bool {
                self.0 = Some(*info);
                true
            }
        }
        let mut state = VTermState::new(2, 80);
        state.lineinfos[0][1].doublewidth = true;
        state.lineinfos[0][1].doubleheight = 2;
        let mut capture = Capture(None);
        putglyph(
            &state,
            &mut capture,
            42,
            2,
            crate::vterm_defs::VTermPos { row: 1, col: 3 },
            true,
        );
        assert_eq!(capture.0.unwrap(), crate::vterm_defs::VTermGlyphInfo {
            schar: 42,
            width: 2,
            protected_cell: true,
            dwl: true,
            dhl: 2,
        });
    }

    #[test]
    fn updatecursor_ignores_unchanged_and_forwards_changed_position() {
        #[derive(Default)]
        struct Capture(usize);
        impl VTermStateCallbacks for Capture {
            fn move_cursor(
                &mut self,
                _: crate::vterm_defs::VTermPos,
                _: crate::vterm_defs::VTermPos,
                _: bool,
            ) -> bool {
                self.0 += 1;
                true
            }
        }
        let mut state = VTermState::new(2, 80);
        state.at_phantom = true;
        let mut capture = Capture::default();
        let unchanged = state.pos;
        updatecursor(&mut state, &mut capture, unchanged, true);
        assert_eq!(capture.0, 0);
        assert!(state.at_phantom);
        let old = state.pos;
        state.pos.col = 1;
        updatecursor(&mut state, &mut capture, old, true);
        assert_eq!(capture.0, 1);
        assert!(!state.at_phantom);
    }

    #[test]
    fn state_erase_clears_following_continuations_at_line_end() {
        let mut state = VTermState::new(4, 80);
        for info in &mut state.lineinfos[0] {
            info.continuation = true;
        }
        let mut callbacks = ();
        erase(
            &mut state,
            &mut callbacks,
            crate::vterm_defs::VTermRect {
                start_row: 1,
                end_row: 3,
                start_col: 4,
                end_col: 80,
            },
            false,
        );
        assert!(state.lineinfos[0][1].continuation);
        assert!(!state.lineinfos[0][2].continuation);
        assert!(!state.lineinfos[0][3].continuation);
    }

    #[test]
    fn state_scroll_clamps_offsets_moves_lineinfo_and_falls_back() {
        #[derive(Default)]
        struct Capture {
            moved: usize,
            erased: usize,
        }

        impl VTermStateCallbacks for Capture {
            fn move_rect(&mut self, _: crate::vterm_defs::VTermRect, _: crate::vterm_defs::VTermRect) -> bool {
                self.moved += 1;
                true
            }
            fn erase(&mut self, _: crate::vterm_defs::VTermRect, _: bool) -> bool {
                self.erased += 1;
                true
            }
        }
        let mut state = VTermState::new(4, 10);
        state.lineinfos[0][1].continuation = true;
        let mut capture = Capture::default();
        scroll(
            &mut state,
            &mut capture,
            crate::vterm_defs::VTermRect {
                start_row: 0,
                end_row: 4,
                start_col: 0,
                end_col: 10,
            },
            1,
            0,
        );
        assert!(state.lineinfos[0][0].continuation);
        assert_eq!(state.lineinfos[0][3], Default::default());
        assert_eq!((capture.moved, capture.erased), (1, 1));
    }

    #[test]
    fn linefeed_advances_or_scrolls_at_bottom_margin() {
        #[derive(Default)]
        struct Capture(usize);
        impl VTermStateCallbacks for Capture {
            fn scroll_rect(&mut self, _: crate::vterm_defs::VTermRect, _: i32, _: i32) -> bool {
                self.0 += 1;
                true
            }
        }
        let mut state = VTermState::new(4, 10);
        state.scrollregion_bottom = -1;
        state.pos.row = 1;
        let mut capture = Capture::default();
        linefeed(&mut state, &mut capture);
        assert_eq!(state.pos.row, 2);
        assert_eq!(capture.0, 0);
        state.pos.row = 3;
        linefeed(&mut state, &mut capture);
        assert_eq!(state.pos.row, 3);
        assert_eq!(capture.0, 1);
    }

    #[test]
    fn settermprop_bool_forwards_boolean_value() {
        struct Capture(Option<i32>);
        impl VTermStateCallbacks for Capture {
            fn set_term_prop(
                &mut self,
                _: crate::vterm_defs::VTermProp,
                value: &crate::vterm_defs::VTermValue<'_>,
            ) -> bool {
                let crate::vterm_defs::VTermValue::Boolean(value) = value else {
                    return false;
                };
                self.0 = Some(*value);
                true
            }
        }
        let mut capture = Capture(None);
        assert_eq!(
            settermprop_bool(
                &mut capture,
                crate::vterm_defs::VTermProp::CursorVisible,
                2,
            ),
            1
        );
        assert_eq!(capture.0, Some(2));
    }

    #[test]
    fn settermprop_int_forwards_number_value() {
        struct Capture(Option<i32>);
        impl VTermStateCallbacks for Capture {
            fn set_term_prop(
                &mut self,
                _: crate::vterm_defs::VTermProp,
                value: &crate::vterm_defs::VTermValue<'_>,
            ) -> bool {
                let crate::vterm_defs::VTermValue::Number(value) = value else {
                    return false;
                };
                self.0 = Some(*value);
                true
            }
        }
        let mut capture = Capture(None);
        assert_eq!(
            settermprop_int(
                &mut capture,
                crate::vterm_defs::VTermProp::CursorShape,
                3,
            ),
            1
        );
        assert_eq!(capture.0, Some(3));
    }

    #[test]
    fn primary_device_attributes_match_state_c() {
        assert_eq!(VTERM_PRIMARY_DEVICE_ATTR, b"61;22;52");
        assert_eq!([NO_FORCE, FORCE], [0, 1]);
        assert_eq!([DWL_OFF, DWL_ON], [0, 1]);
        assert_eq!([DHL_OFF, DHL_TOP, DHL_BOTTOM], [0, 1, 2]);
    }

    #[test]
    fn state_modes_default_to_zeroed_bitfields() {
        assert_eq!(
            VTermStateMode::default(),
            VTermStateMode {
                keypad: false,
                cursor: false,
                autowrap: false,
                insert: false,
                newline: false,
                cursor_visible: false,
                cursor_blink: false,
                cursor_shape: 0,
                alt_screen: false,
                origin: false,
                screen: false,
                leftrightmargin: false,
                bracketpaste: false,
                report_focus: false,
                theme_updates: false,
                synchronized_output: false,
            }
        );
    }

    #[test]
    fn saved_modes_default_to_zeroed_bitfields() {
        assert_eq!(
            VTermSavedMode::default(),
            VTermSavedMode {
                cursor_visible: false,
                cursor_blink: false,
                cursor_shape: 0,
            }
        );
    }

    #[test]
    fn saved_state_defaults_to_zeroed_cursor_and_pen() {
        assert_eq!(
            VTermSavedState::default(),
            VTermSavedState {
                pos: crate::vterm_defs::VTermPos::default(),
                pen: crate::vterm::pen::VTermPen::default(),
                mode: VTermSavedMode::default(),
            }
        );
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

    #[test]
    fn selection_temp_defaults_to_zeroed_union_member() {
        assert_eq!(
            VTermSelectionTemp::default(),
            VTermSelectionTemp {
                mask: 0,
                state: VTermSelectionState::Initial,
                recv_partial: 0,
                send_partial: 0,
            }
        );
    }

    #[test]
    fn state_new_allocates_tabstops_and_both_lineinfo_arrays() {
        let state = VTermState::new(24, 80);
        assert_eq!((state.rows, state.cols), (24, 80));
        assert_eq!(state.tabstops.len(), 10);
        assert_eq!(state.lineinfos[0].len(), 24);
        assert_eq!(state.lineinfos[1].len(), 24);
        assert_eq!(state.active_lineinfo, 0);
        assert_eq!(state.pos, crate::vterm_defs::VTermPos::default());
    }

    #[test]
    fn scrollregion_bottom_uses_explicit_or_unbounded_edge() {
        let mut state = VTermState::new(24, 80);
        state.scrollregion_bottom = -1;
        assert_eq!(state.scrollregion_bottom(), 24);
        state.scrollregion_bottom = 10;
        assert_eq!(state.scrollregion_bottom(), 10);
    }

    #[test]
    fn scrollregion_left_requires_leftrightmargin_mode() {
        let mut state = VTermState::new(24, 80);
        state.scrollregion_left = 5;
        assert_eq!(state.scrollregion_left(), 0);
        state.mode.leftrightmargin = true;
        assert_eq!(state.scrollregion_left(), 5);
    }

    #[test]
    fn scrollregion_right_requires_mode_and_explicit_edge() {
        let mut state = VTermState::new(24, 80);
        state.scrollregion_right = 70;
        assert_eq!(state.scrollregion_right(), 80);
        state.mode.leftrightmargin = true;
        assert_eq!(state.scrollregion_right(), 70);
        state.scrollregion_right = -1;
        assert_eq!(state.scrollregion_right(), 80);
    }

    #[test]
    fn row_width_halves_doublewidth_rows() {
        let mut state = VTermState::new(24, 81);
        assert_eq!(state.row_width(3), 81);
        state.lineinfos[0][3].doublewidth = true;
        assert_eq!(state.row_width(3), 40);
    }

    #[test]
    fn current_row_width_uses_cursor_row() {
        let mut state = VTermState::new(3, 80);
        state.lineinfos[0][2].doublewidth = true;
        state.pos.row = 2;
        assert_eq!(state.current_row_width(), 40);
        state.pos.row = 1;
        assert_eq!(state.current_row_width(), 80);
    }

    #[test]
    fn set_col_tabstop_sets_the_matching_bit() {
        let mut state = VTermState::new(1, 16);
        state.set_col_tabstop(0);
        state.set_col_tabstop(9);
        assert_eq!(state.tabstops, [0b0000_0001, 0b0000_0010]);
    }

    #[test]
    fn clear_col_tabstop_clears_only_the_matching_bit() {
        let mut state = VTermState::new(1, 16);
        state.tabstops = vec![0xFF, 0xFF];
        state.clear_col_tabstop(0);
        state.clear_col_tabstop(9);
        assert_eq!(state.tabstops, [0xFE, 0xFD]);
    }

    #[test]
    fn is_col_tabstop_reads_the_matching_bit() {
        let mut state = VTermState::new(1, 16);
        state.set_col_tabstop(7);
        state.set_col_tabstop(8);
        assert!(state.is_col_tabstop(7));
        assert!(state.is_col_tabstop(8));
        assert!(!state.is_col_tabstop(6));
        assert!(!state.is_col_tabstop(9));
    }

    #[test]
    fn cursor_scrollregion_test_checks_all_four_bounds() {
        let mut state = VTermState::new(24, 80);
        state.scrollregion_top = 2;
        state.scrollregion_bottom = 20;
        state.mode.leftrightmargin = true;
        state.scrollregion_left = 3;
        state.scrollregion_right = 70;
        state.pos = crate::vterm_defs::VTermPos { row: 2, col: 3 };
        assert!(state.is_cursor_in_scrollregion());
        for pos in [
            crate::vterm_defs::VTermPos { row: 1, col: 3 },
            crate::vterm_defs::VTermPos { row: 20, col: 3 },
            crate::vterm_defs::VTermPos { row: 2, col: 2 },
            crate::vterm_defs::VTermPos { row: 2, col: 70 },
        ] {
            state.pos = pos;
            assert!(!state.is_cursor_in_scrollregion());
        }
    }

    #[test]
    fn tab_moves_forward_and_backward_between_stops() {
        let mut state = VTermState::new(1, 20);
        for col in [0, 4, 8, 12, 16] {
            state.set_col_tabstop(col);
        }
        state.pos.col = 1;
        state.tab(2, 1);
        assert_eq!(state.pos.col, 8);
        state.tab(1, -1);
        assert_eq!(state.pos.col, 4);
    }

    #[test]
    fn tab_stops_at_row_edges() {
        let mut state = VTermState::new(1, 10);
        state.pos.col = 8;
        state.tab(1, 1);
        assert_eq!(state.pos.col, 9);
        state.tab(1, 1);
        assert_eq!(state.pos.col, 9);
        state.pos.col = 0;
        state.tab(1, -1);
        assert_eq!(state.pos.col, 0);
    }

    #[test]
    fn set_lineinfo_honors_callback_or_force_and_ignore_values() {
        let mut state = VTermState::new(2, 80);
        state.set_lineinfo(0, NO_FORCE, DWL_ON, DHL_TOP, |_, _, _| false);
        assert_eq!(state.lineinfos[0][0], Default::default());
        state.set_lineinfo(0, FORCE, DWL_ON, DHL_TOP, |_, _, _| false);
        assert!(state.lineinfos[0][0].doublewidth);
        assert_eq!(state.lineinfos[0][0].doubleheight, 1);
        state.set_lineinfo(0, NO_FORCE, -1, -1, |_, new, old| new == old);
        assert!(state.lineinfos[0][0].doublewidth);
        assert_eq!(state.lineinfos[0][0].doubleheight, 1);
    }
}
