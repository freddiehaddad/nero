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

    #[test]
    fn selection_temp_defaults_to_zeroed_union_member() {
        assert_eq!(VTermSelectionTemp::default(), VTermSelectionTemp {
            mask: 0,
            state: VTermSelectionState::Initial,
            recv_partial: 0,
            send_partial: 0,
        });
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
}
