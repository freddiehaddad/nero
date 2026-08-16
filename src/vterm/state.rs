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

/// Selection callbacks (`VTermSelectionCallbacks`).
pub trait VTermSelectionCallbacks {
    fn set(
        &mut self,
        _mask: crate::vterm_defs::VTermSelectionMask,
        _fragment: crate::vterm_defs::VTermStringFragment<'_>,
    ) -> bool {
        false
    }
    fn query(&mut self, _mask: crate::vterm_defs::VTermSelectionMask) -> bool {
        false
    }
}

impl VTermSelectionCallbacks for () {}

/// Unrecognized parser fallbacks (`VTermStateFallbacks`).
pub trait VTermStateFallbacks {
    fn control(&mut self, _control: u8) -> bool { false }
    fn csi(
        &mut self,
        _leader: Option<&[u8]>,
        _args: &[crate::vterm::parser::CsiArg],
        _intermed: Option<&[u8]>,
        _command: u8,
    ) -> bool { false }
    fn osc(
        &mut self,
        _command: i32,
        _fragment: crate::vterm_defs::VTermStringFragment<'_>,
    ) -> bool { false }
    fn dcs(
        &mut self,
        _command: &[u8],
        _fragment: crate::vterm_defs::VTermStringFragment<'_>,
    ) -> bool { false }
    fn apc(&mut self, _fragment: crate::vterm_defs::VTermStringFragment<'_>) -> bool { false }
    fn pm(&mut self, _fragment: crate::vterm_defs::VTermStringFragment<'_>) -> bool { false }
    fn sos(&mut self, _fragment: crate::vterm_defs::VTermStringFragment<'_>) -> bool { false }
}

impl VTermStateFallbacks for () {}

pub fn on_apc<F: VTermStateFallbacks>(
    fallbacks: &mut F,
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
) -> i32 {
    i32::from(fallbacks.apc(fragment))
}

pub fn on_pm<F: VTermStateFallbacks>(
    fallbacks: &mut F,
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
) -> i32 {
    i32::from(fallbacks.pm(fragment))
}

pub fn on_sos<F: VTermStateFallbacks>(
    fallbacks: &mut F,
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
) -> i32 {
    i32::from(fallbacks.sos(fragment))
}

pub fn on_dcs_fallback<F: VTermStateFallbacks>(
    fallbacks: &mut F,
    command: &[u8],
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
) -> i32 {
    i32::from(fallbacks.dcs(command, fragment))
}

pub fn on_osc_fallback<F: VTermStateFallbacks>(
    fallbacks: &mut F,
    command: i32,
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
) -> i32 {
    i32::from(fallbacks.osc(command, fragment))
}

#[cfg(test)]
mod apc_fallback_tests {
    use super::*;
    #[test]
    fn apc_returns_fallback_result() {
        let fragment = crate::vterm_defs::VTermStringFragment {
            bytes: b"x", initial: true, final_fragment: true,
            terminator: crate::vterm_defs::VTermTerminator::St,
        };
        assert_eq!(on_apc(&mut (), fragment), 0);
        assert_eq!(on_pm(&mut (), fragment), 0);
        assert_eq!(on_sos(&mut (), fragment), 0);
        assert_eq!(on_dcs_fallback(&mut (), b"q", fragment), 0);
        assert_eq!(on_osc_fallback(&mut (), 9, fragment), 0);
    }
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
    pub pen: crate::vterm::pen::VTermPen,
    pub saved: VTermSavedState,
    pub selection_buffer: Option<Vec<u8>>,
    pub selection_buflen: usize,
    pub mouse_flags: i32,
    pub mouse_col: i32,
    pub mouse_row: i32,
    pub mouse_buttons: i32,
    pub mouse_protocol: crate::vterm::mouse::VTermMouseProtocol,
    pub grapheme_buf: [u8; crate::types_defs::MAX_SCHAR_SIZE],
    pub grapheme_len: usize,
    pub grapheme_last: u32,
    pub grapheme_state: crate::mbyte_defs::GraphemeState,
    pub combine_width: i32,
    pub combine_pos: crate::vterm_defs::VTermPos,
    pub encodings: [crate::vterm::encoding::VTermEncoding; 4],
    pub gl_set: i32,
    pub gr_set: i32,
    pub gsingle_set: i32,
    pub protected_cell: bool,
    pub selection_temp: VTermSelectionTemp,
    pub key_encoding_stacks: [crate::vterm_defs::VTermKeyEncodingStack; 2],
    pub decrqss: [u8; 4],
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

/// Saves or restores cursor, pen, and cursor presentation
/// (`savecursor`).
pub fn savecursor(state: &mut VTermState, save: i32) {
    if save != 0 {
        state.saved.pos = state.pos;
        state.saved.mode.cursor_visible = state.mode.cursor_visible;
        state.saved.mode.cursor_blink = state.mode.cursor_blink;
        state.saved.mode.cursor_shape = state.mode.cursor_shape;
        state.saved.pen = state.pen;
    } else {
        state.pos = state.saved.pos;
        state.mode.cursor_visible = state.saved.mode.cursor_visible;
        state.mode.cursor_blink = state.saved.mode.cursor_blink;
        state.mode.cursor_shape = state.saved.mode.cursor_shape;
        state.pen = state.saved.pen;
    }
}

/// Emits focus-in when focus reporting is enabled
/// (`vterm_state_focus_in`).
#[must_use]
pub fn vterm_state_focus_in(state: &VTermState, ctrl8bit: bool) -> Vec<u8> {
    if !state.mode.report_focus {
        return Vec::new();
    }
    if ctrl8bit {
        vec![crate::vterm_defs::C1_CSI, b'I']
    } else {
        b"\x1b[I".to_vec()
    }
}

/// Emits focus-out when focus reporting is enabled
/// (`vterm_state_focus_out`).
#[must_use]
pub fn vterm_state_focus_out(state: &VTermState, ctrl8bit: bool) -> Vec<u8> {
    if !state.mode.report_focus {
        return Vec::new();
    }
    if ctrl8bit {
        vec![crate::vterm_defs::C1_CSI, b'O']
    } else {
        b"\x1b[O".to_vec()
    }
}

/// Returns active row metadata (`vterm_state_get_lineinfo`).
#[must_use]
pub fn vterm_state_get_lineinfo(
    state: &VTermState,
    row: i32,
) -> Option<&crate::vterm_defs::VTermLineInfo> {
    state.lineinfos[state.active_lineinfo].get(row as usize)
}

/// Encodes the active kitty keyboard flags response
/// (`request_key_encoding_flags`).
#[must_use]
pub fn request_key_encoding_flags(state: &VTermState, ctrl8bit: bool) -> Vec<u8> {
    let bits = state.key_encoding_stacks[state.active_screen_index()]
        .current()
        .bits();
    let prefix: &[u8] = if ctrl8bit {
        &[crate::vterm_defs::C1_CSI]
    } else {
        b"\x1b["
    };
    [prefix, format!("?{bits}u").as_bytes()].concat()
}

/// Builds the secondary version response (`request_version_string`).
#[must_use]
pub fn request_version_string(ctrl8bit: bool) -> Vec<u8> {
    let start: &[u8] = if ctrl8bit {
        &[crate::vterm_defs::C1_DCS]
    } else {
        b"\x1bP"
    };
    let end: &[u8] = if ctrl8bit {
        &[crate::vterm_defs::C1_ST]
    } else {
        b"\x1b\\"
    };
    [
        start,
        format!(
            ">|libvterm({}.{})",
            crate::vterm_defs::VTERM_VERSION_MAJOR,
            crate::vterm_defs::VTERM_VERSION_MINOR
        )
        .as_bytes(),
        end,
    ]
    .concat()
}

#[must_use]
pub fn request_vertical_margins(state: &VTermState, ctrl8bit: bool) -> Vec<u8> {
    let start: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_DCS] } else { b"\x1bP" };
    let end: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_ST] } else { b"\x1b\\" };
    [
        start,
        format!(
            "1$r{};{}r",
            state.scrollregion_top + 1,
            state.scrollregion_bottom()
        )
        .as_bytes(),
        end,
    ]
    .concat()
}

#[must_use]
pub fn request_horizontal_margins(state: &VTermState, ctrl8bit: bool) -> Vec<u8> {
    let start: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_DCS] } else { b"\x1bP" };
    let end: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_ST] } else { b"\x1b\\" };
    [
        start,
        format!(
            "1$r{};{}s",
            state.scrollregion_left() + 1,
            state.scrollregion_right()
        )
        .as_bytes(),
        end,
    ]
    .concat()
}

#[must_use]
pub fn request_cursor_style(state: &VTermState, ctrl8bit: bool) -> Vec<u8> {
    let mut reply = match i32::from(state.mode.cursor_shape) {
        crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BLOCK => 2,
        crate::vterm_defs::VTERM_PROP_CURSORSHAPE_UNDERLINE => 4,
        crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BAR_LEFT => 6,
        _ => 0,
    };
    if state.mode.cursor_blink {
        reply -= 1;
    }
    let start: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_DCS] } else { b"\x1bP" };
    let end: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_ST] } else { b"\x1b\\" };
    [start, format!("1$r{reply} q").as_bytes(), end].concat()
}

#[must_use]
pub fn request_protected_cell(state: &VTermState, ctrl8bit: bool) -> Vec<u8> {
    let start: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_DCS] } else { b"\x1bP" };
    let end: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_ST] } else { b"\x1b\\" };
    [
        start,
        format!("1$r{}\"q", if state.protected_cell { 1 } else { 2 }).as_bytes(),
        end,
    ]
    .concat()
}

#[cfg(test)]
mod protected_cell_request_tests {
    use super::*;
    #[test]
    fn protected_cell_response_maps_boolean() {
        let mut state = VTermState::new(1, 1);
        assert_eq!(request_protected_cell(&state, false), b"\x1bP1$r2\"q\x1b\\");
        state.protected_cell = true;
        assert_eq!(request_protected_cell(&state, false), b"\x1bP1$r1\"q\x1b\\");
    }
}

#[cfg(test)]
mod cursor_style_request_tests {
    use super::*;
    #[test]
    fn cursor_style_response_maps_shape_and_blink() {
        let mut state = VTermState::new(1, 1);
        state.mode.cursor_shape = crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BAR_LEFT as u8;
        assert_eq!(request_cursor_style(&state, false), b"\x1bP1$r6 q\x1b\\");
        state.mode.cursor_blink = true;
        assert_eq!(request_cursor_style(&state, false), b"\x1bP1$r5 q\x1b\\");
    }
}

#[cfg(test)]
mod horizontal_margin_request_tests {
    use super::*;
    #[test]
    fn horizontal_margin_response_is_one_based() {
        let mut state = VTermState::new(24, 80);
        state.mode.leftrightmargin = true;
        state.scrollregion_left = 4;
        state.scrollregion_right = 70;
        assert_eq!(request_horizontal_margins(&state, false), b"\x1bP1$r5;70s\x1b\\");
    }
}

#[cfg(test)]
mod vertical_margin_request_tests {
    use super::*;
    #[test]
    fn vertical_margin_response_is_one_based() {
        let mut state = VTermState::new(24, 80);
        state.scrollregion_top = 2;
        state.scrollregion_bottom = 20;
        assert_eq!(request_vertical_margins(&state, false), b"\x1bP1$r3;20r\x1b\\");
    }
}

#[cfg(test)]
mod version_request_tests {
    use super::*;
    #[test]
    fn version_request_uses_vendored_version_and_control_width() {
        assert_eq!(request_version_string(false), b"\x1bP>|libvterm(0.3)\x1b\\");
        assert_eq!(
            request_version_string(true),
            [
                vec![crate::vterm_defs::C1_DCS],
                b">|libvterm(0.3)".to_vec(),
                vec![crate::vterm_defs::C1_ST],
            ]
            .concat()
        );
    }
}

/// Replaces active kitty keyboard flags (`set_key_encoding_flags`).
pub fn set_key_encoding_flags(state: &mut VTermState, arg: i32, mode: i32) {
    let set = mode != 3;
    let mut flags = crate::vterm_defs::VTermKeyEncodingFlags::default();
    let apply = |mask: u8| -> bool { arg & i32::from(mask) != 0 && set };
    flags.disambiguate = apply(crate::vterm_defs::KEY_ENCODING_DISAMBIGUATE);
    flags.report_events = apply(crate::vterm_defs::KEY_ENCODING_REPORT_EVENTS);
    flags.report_alternate = apply(crate::vterm_defs::KEY_ENCODING_REPORT_ALTERNATE);
    flags.report_all_keys = apply(crate::vterm_defs::KEY_ENCODING_REPORT_ALL_KEYS);
    flags.report_associated = apply(crate::vterm_defs::KEY_ENCODING_REPORT_ASSOCIATED);
    let index = state.active_screen_index();
    let top = usize::from(state.key_encoding_stacks[index].size) - 1;
    state.key_encoding_stacks[index].items[top] = flags;
}

/// Pushes keyboard flags, evicting the oldest full-stack entry
/// (`push_key_encoding_flags`).
pub fn push_key_encoding_flags(state: &mut VTermState, arg: i32) {
    let index = state.active_screen_index();
    let stack = &mut state.key_encoding_stacks[index];
    if usize::from(stack.size) == stack.items.len() {
        stack.items.copy_within(1.., 0);
    } else {
        stack.size += 1;
    }
    set_key_encoding_flags(state, arg, 1);
}

/// Pops keyboard flag entries (`pop_key_encoding_flags`).
pub fn pop_key_encoding_flags(state: &mut VTermState, arg: i32) {
    let index = state.active_screen_index();
    let stack = &mut state.key_encoding_stacks[index];
    if arg >= i32::from(stack.size) {
        stack.size = 1;
        stack.items[0] = Default::default();
    } else if arg > 0 {
        stack.size -= arg as u8;
    }
}

/// Decodes one base64 sextet (`unbase64one`).
#[must_use]
pub const fn unbase64one(byte: u8) -> u8 {
    match byte {
        b'A'..=b'Z' => byte - b'A',
        b'a'..=b'z' => byte - b'a' + 26,
        b'0'..=b'9' => byte - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => 0xFF,
    }
}

/// Parses OSC 52 selection designators.
pub fn parse_selection_mask(temp: &mut VTermSelectionTemp, bytes: &[u8]) -> usize {
    let mut consumed = 0;
    while temp.state == VTermSelectionState::Initial && consumed < bytes.len() {
        match bytes[consumed] {
            b'c' => temp.mask |= crate::vterm_defs::VTERM_SELECTION_CLIPBOARD as u16,
            b'p' => temp.mask |= crate::vterm_defs::VTERM_SELECTION_PRIMARY as u16,
            b'q' => temp.mask |= crate::vterm_defs::VTERM_SELECTION_SECONDARY as u16,
            b's' => temp.mask |= crate::vterm_defs::VTERM_SELECTION_SELECT as u16,
            b'0'..=b'7' => {
                temp.mask |= (crate::vterm_defs::VTERM_SELECTION_CUT0
                    << (bytes[consumed] - b'0')) as u16;
            }
            b';' => {
                temp.state = VTermSelectionState::Selected;
                if temp.mask == 0 {
                    temp.mask = (crate::vterm_defs::VTERM_SELECTION_SELECT
                        | crate::vterm_defs::VTERM_SELECTION_CUT0) as u16;
                }
            }
            _ => {}
        }
        consumed += 1;
    }
    consumed
}

/// Decodes one OSC 52 base64 fragment while preserving partial sextets.
pub fn decode_selection_base64(
    temp: &mut VTermSelectionTemp,
    bytes: &[u8],
    capacity: usize,
) -> Option<(Vec<u8>, usize)> {
    let mut output = Vec::with_capacity(capacity);
    let mut value = temp.recv_partial & 0x03_FFFF;
    let mut sextets = (temp.recv_partial >> 24) as usize;
    temp.recv_partial = 0;
    let mut consumed = 0;
    while capacity.saturating_sub(output.len()) >= 3 && consumed < bytes.len() {
        if bytes[consumed] == b'=' {
            if sextets == 2 {
                output.push((value >> 4) as u8);
            } else if sextets == 3 {
                output.push((value >> 10) as u8);
                output.push((value >> 2) as u8);
            }
            while consumed < bytes.len() && bytes[consumed] == b'=' {
                consumed += 1;
            }
            sextets = 0;
            value = 0;
        } else {
            let decoded = unbase64one(bytes[consumed]);
            if decoded == 0xFF {
                temp.state = VTermSelectionState::Invalid;
                return None;
            }
            value = (value << 6) | u32::from(decoded);
            sextets += 1;
            consumed += 1;
            if sextets == 4 {
                output.extend_from_slice(&[
                    (value >> 16) as u8,
                    (value >> 8) as u8,
                    value as u8,
                ]);
                value = 0;
                sextets = 0;
            }
        }
    }
    if sextets != 0 {
        temp.recv_partial = ((sextets as u32) << 24) | value;
    }
    Some((output, consumed))
}

#[cfg(test)]
mod selection_decode_tests {
    use super::*;
    #[test]
    fn selection_base64_decodes_and_preserves_partial_input() {
        let mut temp = VTermSelectionTemp::default();
        let (first, used) = decode_selection_base64(&mut temp, b"SG", 3).unwrap();
        assert!(first.is_empty());
        assert_eq!(used, 2);
        let (second, used) = decode_selection_base64(&mut temp, b"k=", 3).unwrap();
        assert_eq!(second, b"Hi");
        assert_eq!(used, 2);
        assert_eq!(temp.recv_partial, 0);
        assert!(decode_selection_base64(&mut temp, b"?", 3).is_none());
        assert_eq!(temp.state, VTermSelectionState::Invalid);
    }
}

#[cfg(test)]
mod selection_mask_tests {
    use super::*;
    #[test]
    fn selection_mask_parses_designators_and_default() {
        let mut temp = VTermSelectionTemp::default();
        assert_eq!(parse_selection_mask(&mut temp, b"cp3;rest"), 4);
        assert_ne!(temp.mask & crate::vterm_defs::VTERM_SELECTION_CLIPBOARD as u16, 0);
        assert_ne!(temp.mask & (crate::vterm_defs::VTERM_SELECTION_CUT0 << 3) as u16, 0);
        let mut default = VTermSelectionTemp::default();
        parse_selection_mask(&mut default, b";");
        assert_eq!(
            default.mask,
            (crate::vterm_defs::VTERM_SELECTION_SELECT
                | crate::vterm_defs::VTERM_SELECTION_CUT0) as u16
        );
    }
}

#[cfg(test)]
mod unbase64_tests {
    use super::*;
    #[test]
    fn unbase64_decodes_all_alphabet_boundaries() {
        assert_eq!(unbase64one(b'A'), 0);
        assert_eq!(unbase64one(b'Z'), 25);
        assert_eq!(unbase64one(b'a'), 26);
        assert_eq!(unbase64one(b'z'), 51);
        assert_eq!(unbase64one(b'0'), 52);
        assert_eq!(unbase64one(b'9'), 61);
        assert_eq!(unbase64one(b'+'), 62);
        assert_eq!(unbase64one(b'/'), 63);
        assert_eq!(unbase64one(b'?'), 0xFF);
    }
}

#[cfg(test)]
mod pop_key_flags_tests {
    use super::*;
    #[test]
    fn pop_key_flags_shrinks_or_resets_stack() {
        let mut state = VTermState::new(1, 1);
        state.key_encoding_stacks[0].size = 3;
        state.key_encoding_stacks[0].items[0].disambiguate = true;
        pop_key_encoding_flags(&mut state, 1);
        assert_eq!(state.key_encoding_stacks[0].size, 2);
        pop_key_encoding_flags(&mut state, 2);
        assert_eq!(state.key_encoding_stacks[0].size, 1);
        assert_eq!(state.key_encoding_stacks[0].items[0].bits(), 0);
    }
}

#[cfg(test)]
mod push_key_flags_tests {
    use super::*;
    #[test]
    fn push_key_flags_grows_then_evicts_oldest() {
        let mut state = VTermState::new(1, 1);
        push_key_encoding_flags(&mut state, 1);
        assert_eq!(state.key_encoding_stacks[0].size, 2);
        assert_eq!(state.key_encoding_stacks[0].current().bits(), 1);
        state.key_encoding_stacks[0].size = 16;
        state.key_encoding_stacks[0].items[0].report_events = true;
        push_key_encoding_flags(&mut state, 4);
        assert_eq!(state.key_encoding_stacks[0].size, 16);
        assert!(!state.key_encoding_stacks[0].items[0].report_events);
        assert_eq!(state.key_encoding_stacks[0].current().bits(), 4);
    }
}

#[cfg(test)]
mod set_key_flags_tests {
    use super::*;
    #[test]
    fn set_key_flags_sets_or_resets_requested_bits() {
        let mut state = VTermState::new(1, 1);
        set_key_encoding_flags(&mut state, 3, 1);
        assert_eq!(state.key_encoding_stacks[0].current().bits(), 3);
        set_key_encoding_flags(&mut state, 1, 3);
        assert_eq!(state.key_encoding_stacks[0].current().bits(), 0);
    }
}

#[cfg(test)]
mod key_flag_request_tests {
    use super::*;
    #[test]
    fn key_flag_request_uses_active_stack_and_control_width() {
        let mut state = VTermState::new(1, 1);
        state.key_encoding_stacks[1].items[0].report_events = true;
        state.mode.alt_screen = true;
        assert_eq!(request_key_encoding_flags(&state, false), b"\x1b[?2u");
        assert_eq!(
            request_key_encoding_flags(&state, true),
            [vec![crate::vterm_defs::C1_CSI], b"?2u".to_vec()].concat()
        );
    }
}

#[cfg(test)]
mod get_lineinfo_tests {
    use super::*;
    #[test]
    fn get_lineinfo_uses_active_array_and_bounds_checks() {
        let mut state = VTermState::new(2, 80);
        state.lineinfos[1][1].doublewidth = true;
        state.active_lineinfo = 1;
        assert!(vterm_state_get_lineinfo(&state, 1).unwrap().doublewidth);
        assert!(vterm_state_get_lineinfo(&state, 2).is_none());
    }
}

#[cfg(test)]
mod focus_out_tests {
    use super::*;
    #[test]
    fn focus_out_honors_reporting_and_control_width() {
        let mut state = VTermState::new(24, 80);
        assert!(vterm_state_focus_out(&state, false).is_empty());
        state.mode.report_focus = true;
        assert_eq!(vterm_state_focus_out(&state, false), b"\x1b[O");
        assert_eq!(
            vterm_state_focus_out(&state, true),
            [crate::vterm_defs::C1_CSI, b'O']
        );
    }
}

#[cfg(test)]
mod focus_in_tests {
    use super::*;
    #[test]
    fn focus_in_honors_reporting_and_control_width() {
        let mut state = VTermState::new(24, 80);
        assert!(vterm_state_focus_in(&state, false).is_empty());
        state.mode.report_focus = true;
        assert_eq!(vterm_state_focus_in(&state, false), b"\x1b[I");
        assert_eq!(
            vterm_state_focus_in(&state, true),
            [crate::vterm_defs::C1_CSI, b'I']
        );
    }
}

#[cfg(test)]
mod savecursor_tests {
    use super::*;
    #[test]
    fn savecursor_roundtrips_cursor_pen_and_modes() {
        let mut state = VTermState::new(24, 80);
        state.pos = crate::vterm_defs::VTermPos { row: 3, col: 4 };
        state.pen.bold = true;
        state.mode.cursor_visible = true;
        state.mode.cursor_blink = true;
        state.mode.cursor_shape = 2;
        savecursor(&mut state, 1);
        state.pos = Default::default();
        state.pen = Default::default();
        state.mode.cursor_visible = false;
        state.mode.cursor_blink = false;
        state.mode.cursor_shape = 0;
        savecursor(&mut state, 0);
        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 3, col: 4 });
        assert!(state.pen.bold);
        assert!(state.mode.cursor_visible);
        assert!(state.mode.cursor_blink);
        assert_eq!(state.mode.cursor_shape, 2);
    }
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
            pen: crate::vterm::pen::VTermPen::default(),
            saved: VTermSavedState::default(),
            selection_buffer: None,
            selection_buflen: 0,
            mouse_flags: 0,
            mouse_col: 0,
            mouse_row: 0,
            mouse_buttons: 0,
            mouse_protocol: crate::vterm::mouse::VTermMouseProtocol::X10,
            grapheme_buf: [0; crate::types_defs::MAX_SCHAR_SIZE],
            grapheme_len: 0,
            grapheme_last: 0,
            grapheme_state: crate::mbyte_defs::GRAPHEME_STATE_INIT,
            combine_width: 0,
            combine_pos: crate::vterm_defs::VTermPos { row: -1, col: 0 },
            encodings: [crate::vterm::encoding::VTermEncoding::UsAscii; 4],
            gl_set: 0,
            gr_set: 1,
            gsingle_set: 0,
            protected_cell: false,
            selection_temp: VTermSelectionTemp::default(),
            key_encoding_stacks: std::array::from_fn(|_| Default::default()),
            decrqss: [0; 4],
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

    /// Installs selection storage (`vterm_state_set_selection_callbacks`
    /// buffer portion).
    pub fn set_selection_buffer(&mut self, buffer: Option<Vec<u8>>, buflen: usize) {
        self.selection_buffer = if buflen != 0 {
            Some(buffer.unwrap_or_else(|| vec![0; buflen]))
        } else {
            buffer
        };
        self.selection_buflen = buflen;
    }

    /// Stores one accepted terminal property
    /// (`vterm_state_set_termprop`).
    pub fn set_termprop(
        &mut self,
        prop: crate::vterm_defs::VTermProp,
        value: &crate::vterm_defs::VTermValue<'_>,
        accepted: bool,
    ) -> i32 {
        if !accepted {
            return 0;
        }
        match (prop, value) {
            (crate::vterm_defs::VTermProp::Title | crate::vterm_defs::VTermProp::IconName, _) => 1,
            (crate::vterm_defs::VTermProp::CursorVisible, crate::vterm_defs::VTermValue::Boolean(v)) => {
                self.mode.cursor_visible = *v != 0; 1
            }
            (crate::vterm_defs::VTermProp::CursorBlink, crate::vterm_defs::VTermValue::Boolean(v)) => {
                self.mode.cursor_blink = *v != 0; 1
            }
            (crate::vterm_defs::VTermProp::CursorShape, crate::vterm_defs::VTermValue::Number(v)) => {
                self.mode.cursor_shape = *v as u8 & 3; 1
            }
            (crate::vterm_defs::VTermProp::Reverse, crate::vterm_defs::VTermValue::Boolean(v)) => {
                self.mode.screen = *v != 0; 1
            }
            (crate::vterm_defs::VTermProp::AltScreen, crate::vterm_defs::VTermValue::Boolean(v)) => {
                self.mode.alt_screen = *v != 0;
                self.active_lineinfo = usize::from(self.mode.alt_screen);
                1
            }
            (crate::vterm_defs::VTermProp::Mouse, crate::vterm_defs::VTermValue::Number(v)) => {
                self.mouse_flags = 0;
                if *v != 0 { self.mouse_flags |= crate::vterm::mouse::MOUSE_WANT_CLICK; }
                if *v == crate::vterm_defs::VTERM_PROP_MOUSE_DRAG { self.mouse_flags |= crate::vterm::mouse::MOUSE_WANT_DRAG; }
                if *v == crate::vterm_defs::VTERM_PROP_MOUSE_MOVE { self.mouse_flags |= crate::vterm::mouse::MOUSE_WANT_MOVE; }
                1
            }
            (crate::vterm_defs::VTermProp::FocusReport, crate::vterm_defs::VTermValue::Boolean(v)) => {
                self.mode.report_focus = *v != 0; 1
            }
            (crate::vterm_defs::VTermProp::ThemeUpdates, crate::vterm_defs::VTermValue::Boolean(v)) => {
                self.mode.theme_updates = *v != 0; 1
            }
            (crate::vterm_defs::VTermProp::SyncOutput, crate::vterm_defs::VTermValue::Boolean(v)) => {
                self.mode.synchronized_output = *v != 0; 1
            }
            _ => 0,
        }
    }

    #[must_use]
    pub const fn active_screen_index(&self) -> usize {
        if self.mode.alt_screen { 1 } else { 0 }
    }
}

#[cfg(test)]
mod termprop_state_tests {
    use super::*;
    #[test]
    fn termprop_updates_modes_mouse_and_active_lineinfo() {
        let mut state = VTermState::new(2, 80);
        assert_eq!(
            state.set_termprop(
                crate::vterm_defs::VTermProp::AltScreen,
                &crate::vterm_defs::VTermValue::Boolean(1),
                true,
            ),
            1
        );
        assert_eq!((state.gl_set, state.gr_set, state.gsingle_set), (0, 1, 0));
        assert!(!state.protected_cell);
        assert_eq!(state.selection_temp, VTermSelectionTemp::default());
        assert_eq!(state.key_encoding_stacks[0].size, 1);
        assert_eq!(state.key_encoding_stacks[1].size, 1);
        assert_eq!(state.decrqss, [0; 4]);
        assert_eq!(state.combine_width, 0);
        assert_eq!(state.combine_pos.row, -1);
        assert_eq!(
            state.encodings,
            [crate::vterm::encoding::VTermEncoding::UsAscii; 4]
        );
        assert_eq!(state.grapheme_buf, [0; crate::types_defs::MAX_SCHAR_SIZE]);
        assert_eq!(state.grapheme_len, 0);
        assert_eq!(state.grapheme_last, 0);
        assert_eq!(
            state.grapheme_state,
            crate::mbyte_defs::GRAPHEME_STATE_INIT
        );
        assert!(state.mode.alt_screen);
        assert_eq!(state.active_lineinfo, 1);
        state.set_termprop(
            crate::vterm_defs::VTermProp::Mouse,
            &crate::vterm_defs::VTermValue::Number(
                crate::vterm_defs::VTERM_PROP_MOUSE_MOVE,
            ),
            true,
        );
        assert_eq!(
            state.mouse_flags,
            crate::vterm::mouse::MOUSE_WANT_CLICK | crate::vterm::mouse::MOUSE_WANT_MOVE
        );
        assert_eq!(
            state.set_termprop(
                crate::vterm_defs::VTermProp::CursorVisible,
                &crate::vterm_defs::VTermValue::Boolean(1),
                false,
            ),
            0
        );
        assert!(!state.mode.cursor_visible);
    }

    #[test]
    fn active_screen_index_tracks_altscreen_mode() {
        let mut state = VTermState::new(1, 1);
        assert_eq!(state.active_screen_index(), 0);
        state.mode.alt_screen = true;
        assert_eq!(state.active_screen_index(), 1);
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
    fn default_selection_callbacks_decline_operations() {
        let mut callbacks = ();
        let fragment = crate::vterm_defs::VTermStringFragment {
            bytes: b"",
            initial: true,
            final_fragment: true,
            terminator: crate::vterm_defs::VTermTerminator::St,
        };
        assert!(!callbacks.set(0, fragment));
        assert!(!callbacks.query(0));
    }

    #[test]
    fn default_state_fallbacks_decline_sequences() {
        let mut fallbacks = ();
        let fragment = crate::vterm_defs::VTermStringFragment {
            bytes: b"",
            initial: true,
            final_fragment: true,
            terminator: crate::vterm_defs::VTermTerminator::St,
        };
        assert!(!fallbacks.control(0x80));
        assert!(!fallbacks.csi(None, &[], None, b'm'));
        assert!(!fallbacks.osc(3, fragment));
        assert!(!fallbacks.dcs(b"q", fragment));
        assert!(!fallbacks.apc(fragment));
        assert!(!fallbacks.pm(fragment));
        assert!(!fallbacks.sos(fragment));
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
        assert_eq!(state.pen, crate::vterm::pen::VTermPen::default());
        assert_eq!(state.saved, VTermSavedState::default());
        assert!(state.selection_buffer.is_none());
        assert_eq!(state.selection_buflen, 0);
        assert_eq!((state.mouse_row, state.mouse_col), (0, 0));
        assert_eq!(state.mouse_buttons, 0);
        assert_eq!(
            state.mouse_protocol,
            crate::vterm::mouse::VTermMouseProtocol::X10
        );
    }

    #[test]
    fn selection_buffer_uses_supplied_or_allocated_storage() {
        let mut state = VTermState::new(1, 1);
        state.set_selection_buffer(None, 4);
        assert_eq!(state.selection_buffer, Some(vec![0; 4]));
        assert_eq!(state.selection_buflen, 4);
        state.set_selection_buffer(Some(vec![1, 2]), 2);
        assert_eq!(state.selection_buffer, Some(vec![1, 2]));
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
