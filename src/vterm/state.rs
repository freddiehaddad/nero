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

pub fn on_osc<
    C: VTermStateCallbacks,
    S: VTermSelectionCallbacks,
    F: VTermStateFallbacks,
>(
    state: &mut VTermState,
    callbacks: &mut C,
    selection_callbacks: Option<&mut S>,
    fallbacks: &mut F,
    command: i32,
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
) -> i32 {
    let set_string = |state: &mut VTermState,
                      callbacks: &mut C,
                      prop: crate::vterm_defs::VTermProp| {
        let value = crate::vterm_defs::VTermValue::String(fragment);
        let _ = vterm_state_set_termprop(state, Some(callbacks), prop, &value);
    };
    match command {
        0 => {
            set_string(state, callbacks, crate::vterm_defs::VTermProp::IconName);
            set_string(state, callbacks, crate::vterm_defs::VTermProp::Title);
        }
        1 => {
            set_string(state, callbacks, crate::vterm_defs::VTermProp::IconName);
        }
        2 => {
            set_string(state, callbacks, crate::vterm_defs::VTermProp::Title);
        }
        52 => {
            if let Some(selection_callbacks) = selection_callbacks {
                osc_selection(state, selection_callbacks, fragment);
            }
        }
        _ => {}
    }
    on_osc_fallback(fallbacks, command, fragment)
}

pub fn on_osc_52<C: VTermSelectionCallbacks, F: VTermStateFallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    fallbacks: &mut F,
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
) -> i32 {
    osc_selection(state, callbacks, fragment);
    on_osc_fallback(fallbacks, 52, fragment)
}

#[cfg(test)]
mod osc_52_tests {
    use super::*;
    #[test]
    fn osc_52_routes_to_selection_and_fallback() {
        let mut state = VTermState::new(1, 1);
        state.selection_buflen = 8;
        let mut selection = ();
        let fragment = crate::vterm_defs::VTermStringFragment {
            bytes: b"c;?", initial: true, final_fragment: true,
            terminator: crate::vterm_defs::VTermTerminator::Bel,
        };
        assert_eq!(on_osc_52(&mut state, &mut selection, &mut (), fragment), 0);
        assert_eq!(state.selection_temp.state, VTermSelectionState::Query);
    }
}

#[cfg(test)]
mod osc_title_tests {
    use super::*;
    #[derive(Default)]
    struct Capture(Vec<crate::vterm_defs::VTermProp>);
    impl VTermStateCallbacks for Capture {
        fn set_term_prop(
            &mut self,
            prop: crate::vterm_defs::VTermProp,
            _: &crate::vterm_defs::VTermValue<'_>,
        ) -> bool {
            self.0.push(prop);
            true
        }
    }
    #[test]
    fn osc_zero_sets_icon_and_title() {
        let mut state = VTermState::new(1, 1);
        let mut capture = Capture::default();
        let fragment = crate::vterm_defs::VTermStringFragment {
            bytes: b"x", initial: true, final_fragment: true,
            terminator: crate::vterm_defs::VTermTerminator::Bel,
        };
        assert_eq!(
            on_osc(
                &mut state,
                &mut capture,
                None::<&mut ()>,
                &mut (),
                0,
                fragment,
            ),
            0
        );
        assert_eq!(
            capture.0,
            [
                crate::vterm_defs::VTermProp::IconName,
                crate::vterm_defs::VTermProp::Title,
            ]
        );
    }

    #[test]
    fn osc_fifty_two_routes_to_selection_before_fallback() {
        #[derive(Default)]
        struct Selection(bool);
        impl VTermSelectionCallbacks for Selection {
            fn query(&mut self, _: crate::vterm_defs::VTermSelectionMask) -> bool {
                self.0 = true;
                true
            }
        }
        let mut state = VTermState::new(1, 1);
        let mut selection = Selection::default();
        let fragment = crate::vterm_defs::VTermStringFragment {
            bytes: b"c;?",
            initial: true,
            final_fragment: true,
            terminator: crate::vterm_defs::VTermTerminator::Bel,
        };
        assert_eq!(
            on_osc(
                &mut state,
                &mut (),
                Some(&mut selection),
                &mut (),
                52,
                fragment,
            ),
            0
        );
        assert!(selection.0);
    }
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
    pub default_fg: crate::vterm_defs::VTermColor,
    pub default_bg: crate::vterm_defs::VTermColor,
    pub colors: [crate::vterm_defs::VTermColor; 16],
    pub bold_is_highbright: bool,
    pub ctrl8bit: bool,
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
    pub encodings: [crate::vterm::encoding::VTermEncodingInstance; 4],
    pub encoding_utf8: crate::vterm::encoding::VTermEncodingInstance,
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

    fn bell(&mut self) -> bool {
        false
    }

    fn set_pen_attr(
        &mut self,
        _attr: crate::vterm_defs::VTermAttr,
        _value: &crate::vterm_defs::VTermValue<'_>,
    ) -> bool {
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

    fn theme(&mut self, _dark: &mut bool) -> bool {
        false
    }

    fn sb_clear(&mut self) -> bool {
        false
    }
}

impl VTermStateCallbacks for () {}

pub struct VTermStateHost<C> {
    pub state: VTermState,
    pub callbacks: Option<C>,
}

pub fn vterm_state_set_callbacks<C>(
    host: &mut VTermStateHost<C>,
    callbacks: Option<C>,
) {
    host.callbacks = callbacks;
}

pub struct VTermStateFallbackHost<F> {
    pub fallbacks: Option<F>,
}

pub struct VTermSelectionHost<C> {
    pub state: VTermState,
    pub callbacks: Option<C>,
}

pub fn vterm_state_set_selection_callbacks<C>(
    host: &mut VTermSelectionHost<C>,
    callbacks: Option<C>,
    buffer: Option<Vec<u8>>,
    buflen: usize,
) {
    host.state.set_selection_buffer(buffer, buflen);
    host.callbacks = callbacks;
}

#[cfg(test)]
mod selection_callback_setter_tests {
    use super::*;
    #[test]
    fn selection_callback_setter_installs_callback_and_buffer() {
        let mut host = VTermSelectionHost {
            state: VTermState::new(1, 1),
            callbacks: None::<u8>,
        };
        vterm_state_set_selection_callbacks(&mut host, Some(3), None, 4);
        assert_eq!(host.callbacks, Some(3));
        assert_eq!(host.state.selection_buffer, Some(vec![0; 4]));
    }
}

pub fn vterm_state_set_unrecognised_fallbacks<F>(
    host: &mut VTermStateFallbackHost<F>,
    fallbacks: Option<F>,
) {
    host.fallbacks = fallbacks;
}

pub fn vterm_state_set_termprop<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: Option<&mut C>,
    prop: crate::vterm_defs::VTermProp,
    value: &crate::vterm_defs::VTermValue<'_>,
) -> i32 {
    if let Some(callbacks) = callbacks
        && !callbacks.set_term_prop(prop, value)
    {
        return 0;
    }
    state.set_termprop(prop, value, true)
}

#[cfg(test)]
mod state_termprop_entry_tests {
    use super::*;
    #[test]
    fn state_termprop_entry_gates_storage_on_callback() {
        let mut state = VTermState::new(1, 1);
        assert_eq!(
            vterm_state_set_termprop::<()>(
                &mut state,
                None,
                crate::vterm_defs::VTermProp::CursorVisible,
                &crate::vterm_defs::VTermValue::Boolean(1),
            ),
            1
        );
        assert!(state.mode.cursor_visible);
    }
}

#[cfg(test)]
mod state_fallback_setter_tests {
    use super::*;
    #[test]
    fn state_fallback_setter_installs_and_removes_fallbacks() {
        let mut host = VTermStateFallbackHost {
            fallbacks: None::<u8>,
        };
        vterm_state_set_unrecognised_fallbacks(&mut host, Some(4));
        assert_eq!(host.fallbacks, Some(4));
        vterm_state_set_unrecognised_fallbacks(&mut host, None);
        assert_eq!(host.fallbacks, None);
    }
}

#[cfg(test)]
mod state_callback_setter_tests {
    use super::*;
    #[test]
    fn state_callback_setter_installs_and_removes_callbacks() {
        let mut host = VTermStateHost {
            state: VTermState::new(1, 1),
            callbacks: None::<u8>,
        };
        vterm_state_set_callbacks(&mut host, Some(7));
        assert_eq!(host.callbacks, Some(7));
        vterm_state_set_callbacks(&mut host, None);
        assert_eq!(host.callbacks, None);
    }
}

#[allow(dead_code)]
struct StatePenCallback<'a, C>(&'a mut C);

impl<C: VTermStateCallbacks> crate::vterm::pen::VTermPenCallbacks for StatePenCallback<'_, C> {
    fn set_pen_attr(
        &mut self,
        attr: crate::vterm_defs::VTermAttr,
        value: &crate::vterm_defs::VTermValue<'_>,
    ) -> bool {
        self.0.set_pen_attr(attr, value)
    }
}

#[cfg(test)]
mod state_pen_adapter_tests {
    use super::*;
    struct Capture(bool);
    impl VTermStateCallbacks for Capture {
        fn set_pen_attr(
            &mut self,
            _: crate::vterm_defs::VTermAttr,
            _: &crate::vterm_defs::VTermValue<'_>,
        ) -> bool {
            self.0 = true;
            true
        }
    }
    #[test]
    fn state_pen_adapter_forwards_attribute() {
        let mut capture = Capture(false);
        let mut adapter = StatePenCallback(&mut capture);
        assert!(crate::vterm::pen::VTermPenCallbacks::set_pen_attr(
            &mut adapter,
            crate::vterm_defs::VTermAttr::Bold,
            &crate::vterm_defs::VTermValue::Boolean(1),
        ));
        assert!(capture.0);
    }
}

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

pub fn decaln<C: VTermStateCallbacks>(state: &VTermState, callbacks: &mut C) {
    let e = crate::grid::schar_from_ascii(b'E');
    for row in 0..state.rows {
        for col in 0..state.row_width(row) {
            putglyph(
                state,
                callbacks,
                e,
                1,
                crate::vterm_defs::VTermPos { row, col },
                state.protected_cell,
            );
        }
    }
}

#[cfg(test)]
mod decaln_tests {
    use super::*;
    #[derive(Default)]
    struct Capture(usize);
    impl VTermStateCallbacks for Capture {
        fn put_glyph(
            &mut self,
            info: &crate::vterm_defs::VTermGlyphInfo,
            _: crate::vterm_defs::VTermPos,
        ) -> bool {
            assert_eq!(info.schar, crate::grid::schar_from_ascii(b'E'));
            self.0 += 1;
            true
        }
    }
    #[test]
    fn decaln_fills_every_visible_cell_with_e() {
        let state = VTermState::new(2, 3);
        let mut capture = Capture::default();
        decaln(&state, &mut capture);
        assert_eq!(capture.0, 6);
    }
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

pub fn set_origin_mode<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    value: i32,
) {
    let old = state.pos;
    state.mode.origin = value != 0;
    state.pos.row = if state.mode.origin {
        state.scrollregion_top
    } else {
        0
    };
    state.pos.col = if state.mode.origin {
        state.scrollregion_left()
    } else {
        0
    };
    updatecursor(state, callbacks, old, true);
}

fn set_state_bool_prop<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    prop: crate::vterm_defs::VTermProp,
    value: i32,
) -> i32 {
    vterm_state_set_termprop(
        state,
        Some(callbacks),
        prop,
        &crate::vterm_defs::VTermValue::Boolean(value),
    )
}

fn set_state_int_prop<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    prop: crate::vterm_defs::VTermProp,
    value: i32,
) -> i32 {
    vterm_state_set_termprop(
        state,
        Some(callbacks),
        prop,
        &crate::vterm_defs::VTermValue::Number(value),
    )
}

pub fn set_dec_mode<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    number: i32,
    value: i32,
) -> bool {
    match number {
        1 | 7 | 2004 => {
            state.set_dec_basic_mode(number, value);
        }
        5 => {
            set_state_bool_prop(
                state,
                callbacks,
                crate::vterm_defs::VTermProp::Reverse,
                value,
            );
        }
        6 => set_origin_mode(state, callbacks, value),
        12 => {
            set_state_bool_prop(
                state,
                callbacks,
                crate::vterm_defs::VTermProp::CursorBlink,
                value,
            );
        }
        25 => {
            set_state_bool_prop(
                state,
                callbacks,
                crate::vterm_defs::VTermProp::CursorVisible,
                value,
            );
        }
        69 => {
            state.mode.leftrightmargin = value != 0;
            if value != 0 {
                for row in 0..state.rows {
                    state.set_lineinfo(row, FORCE, DWL_OFF, DHL_OFF, |row, new, old| {
                        callbacks.set_line_info(row, new, old)
                    });
                }
            }
        }
        1000 | 1002 | 1003 => {
            let mouse = if value == 0 {
                crate::vterm_defs::VTERM_PROP_MOUSE_NONE
            } else if number == 1000 {
                crate::vterm_defs::VTERM_PROP_MOUSE_CLICK
            } else if number == 1002 {
                crate::vterm_defs::VTERM_PROP_MOUSE_DRAG
            } else {
                crate::vterm_defs::VTERM_PROP_MOUSE_MOVE
            };
            set_state_int_prop(
                state,
                callbacks,
                crate::vterm_defs::VTermProp::Mouse,
                mouse,
            );
        }
        1004 => {
            set_state_bool_prop(
                state,
                callbacks,
                crate::vterm_defs::VTermProp::FocusReport,
                value,
            );
            state.mode.report_focus = value != 0;
        }
        1005 | 1006 | 1015 => {
            state.set_mouse_protocol_mode(number, value);
        }
        1047 | 1049 => {
            if set_state_bool_prop(
                state,
                callbacks,
                crate::vterm_defs::VTermProp::AltScreen,
                value,
            ) != 0
                && value != 0
            {
                erase(
                    state,
                    callbacks,
                    crate::vterm_defs::VTermRect {
                        start_row: 0,
                        end_row: state.rows,
                        start_col: 0,
                        end_col: state.cols,
                    },
                    false,
                );
            }
            if number == 1049 {
                savecursor(state, value);
            }
        }
        1048 => savecursor(state, value),
        2026 => {
            set_state_bool_prop(
                state,
                callbacks,
                crate::vterm_defs::VTermProp::SyncOutput,
                value,
            );
        }
        2031 => {
            set_state_bool_prop(
                state,
                callbacks,
                crate::vterm_defs::VTermProp::ThemeUpdates,
                value,
            );
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod set_dec_mode_tests {
    use super::*;

    #[derive(Default)]
    struct Capture {
        props: Vec<crate::vterm_defs::VTermProp>,
        moves: usize,
        erases: usize,
    }

    impl VTermStateCallbacks for Capture {
        fn set_term_prop(
            &mut self,
            prop: crate::vterm_defs::VTermProp,
            _: &crate::vterm_defs::VTermValue<'_>,
        ) -> bool {
            self.props.push(prop);
            true
        }

        fn move_cursor(
            &mut self,
            _: crate::vterm_defs::VTermPos,
            _: crate::vterm_defs::VTermPos,
            _: bool,
        ) -> bool {
            self.moves += 1;
            true
        }

        fn erase(&mut self, _: crate::vterm_defs::VTermRect, _: bool) -> bool {
            self.erases += 1;
            true
        }
    }

    #[test]
    fn dec_modes_update_screen_origin_cursor_and_input_modes() {
        let mut state = VTermState::new(5, 8);
        state.scrollregion_top = 2;
        state.pos = crate::vterm_defs::VTermPos { row: 4, col: 3 };
        let mut capture = Capture::default();

        assert!(set_dec_mode(&mut state, &mut capture, 5, 1));
        assert!(state.mode.screen);
        assert!(set_dec_mode(&mut state, &mut capture, 6, 1));
        assert_eq!(state.pos.row, 2);
        assert_eq!(capture.moves, 1);
        assert!(set_dec_mode(&mut state, &mut capture, 7, 0));
        assert!(!state.mode.autowrap);
        assert!(set_dec_mode(&mut state, &mut capture, 12, 1));
        assert!(state.mode.cursor_blink);
        assert!(set_dec_mode(&mut state, &mut capture, 25, 1));
        assert!(state.mode.cursor_visible);
        assert!(set_dec_mode(&mut state, &mut capture, 2004, 1));
        assert!(state.mode.bracketpaste);
    }

    #[test]
    fn dec_modes_update_margins_mouse_and_reporting() {
        let mut state = VTermState::new(3, 8);
        state.lineinfos[0][1].doublewidth = true;
        let mut capture = Capture::default();

        assert!(set_dec_mode(&mut state, &mut capture, 69, 1));
        assert!(state.mode.leftrightmargin);
        assert!(!state.lineinfos[0][1].doublewidth);
        assert!(set_dec_mode(&mut state, &mut capture, 1002, 1));
        assert_eq!(
            state.mouse_flags,
            crate::vterm::mouse::MOUSE_WANT_CLICK | crate::vterm::mouse::MOUSE_WANT_DRAG
        );
        assert!(set_dec_mode(&mut state, &mut capture, 1006, 1));
        assert_eq!(
            state.mouse_protocol,
            crate::vterm::mouse::VTermMouseProtocol::Sgr
        );
        assert!(set_dec_mode(&mut state, &mut capture, 1004, 1));
        assert!(state.mode.report_focus);
        assert!(set_dec_mode(&mut state, &mut capture, 2026, 1));
        assert!(state.mode.synchronized_output);
        assert!(set_dec_mode(&mut state, &mut capture, 2031, 1));
        assert!(state.mode.theme_updates);
    }

    #[test]
    fn dec_alt_screen_and_saved_cursor_modes_roundtrip() {
        let mut state = VTermState::new(3, 8);
        state.pos = crate::vterm_defs::VTermPos { row: 1, col: 2 };
        let mut capture = Capture::default();

        assert!(set_dec_mode(&mut state, &mut capture, 1049, 1));
        assert!(state.mode.alt_screen);
        assert_eq!(state.saved.pos, crate::vterm_defs::VTermPos { row: 1, col: 2 });
        assert_eq!(capture.erases, 1);
        state.pos = crate::vterm_defs::VTermPos { row: 2, col: 5 };
        assert!(set_dec_mode(&mut state, &mut capture, 1049, 0));
        assert!(!state.mode.alt_screen);
        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 1, col: 2 });
    }

    #[test]
    fn dec_mode_rejects_unknown_numbers() {
        let mut state = VTermState::new(1, 1);
        assert!(!set_dec_mode(&mut state, &mut (), 9999, 1));
    }
}

#[cfg(test)]
mod origin_mode_tests {
    use super::*;
    #[test]
    fn origin_mode_moves_cursor_to_region_or_origin() {
        let mut state = VTermState::new(10, 20);
        state.scrollregion_top = 2;
        state.mode.leftrightmargin = true;
        state.scrollregion_left = 3;
        let mut callbacks = ();
        set_origin_mode(&mut state, &mut callbacks, 1);
        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 2, col: 3 });
        set_origin_mode(&mut state, &mut callbacks, 0);
        assert_eq!(state.pos, Default::default());
    }
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

pub fn control_backspace(state: &mut VTermState) {
    if state.pos.col > 0 {
        state.pos.col -= 1;
    }
}

pub fn control_horizontal_tab(state: &mut VTermState) {
    state.tab(1, 1);
}

pub fn control_linefeed<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
) {
    linefeed(state, callbacks);
    if state.mode.newline {
        state.pos.col = 0;
    }
}

pub fn control_carriage_return(state: &mut VTermState) {
    state.pos.col = 0;
}

pub fn control_locking_shift(state: &mut VTermState, control: u8) -> bool {
    match control {
        0x0E => state.gl_set = 1,
        0x0F => state.gl_set = 0,
        _ => return false,
    }
    true
}

pub fn control_single_shift(state: &mut VTermState, control: u8) -> bool {
    match control {
        0x8E => state.gsingle_set = 2,
        0x8F => state.gsingle_set = 3,
        _ => return false,
    }
    true
}

#[must_use]
pub fn decode_text_codepoints(
    state: &mut VTermState,
    bytes: &[u8],
    utf8: bool,
) -> (Vec<u32>, usize) {
    if bytes.is_empty() {
        return (Vec::new(), 0);
    }
    let single_shift = state.gsingle_set != 0;
    let capacity = if single_shift { 1 } else { bytes.len() + 1 };
    let mut codepoints = vec![0; capacity];
    let mut count = 0;
    let mut consumed = 0;

    let selected = if single_shift {
        Some(state.gsingle_set as usize)
    } else if bytes[0] & 0x80 == 0 {
        Some(state.gl_set as usize)
    } else if utf8 {
        None
    } else {
        Some(state.gr_set as usize)
    };
    if let Some(index) = selected {
        if state.encodings[index].encoding() == crate::vterm::encoding::VTermEncoding::Utf8 {
            state
                .encoding_utf8
                .decode(&mut codepoints, &mut count, bytes, &mut consumed);
        } else {
            state.encodings[index].decode(
                &mut codepoints,
                &mut count,
                bytes,
                &mut consumed,
            );
        }
    } else {
        state
            .encoding_utf8
            .decode(&mut codepoints, &mut count, bytes, &mut consumed);
    }
    codepoints.truncate(count);
    if single_shift && !codepoints.is_empty() {
        state.gsingle_set = 0;
    }
    (codepoints, consumed)
}

#[cfg(test)]
mod decode_text_tests {
    use super::*;

    #[test]
    fn text_decoder_selects_gl_and_designated_charsets() {
        let mut state = VTermState::new(1, 1);
        assert_eq!(decode_text_codepoints(&mut state, b"A", false), (vec![65], 1));
        state.encodings[1] = crate::vterm::encoding::VTermEncodingInstance::new(
            crate::vterm::encoding::VTermEncoding::DecSpecialGraphics,
        );
        state.gl_set = 1;
        assert_eq!(
            decode_text_codepoints(&mut state, b"q", false),
            (vec![0x2500], 1)
        );
    }

    #[test]
    fn text_decoder_preserves_utf8_state_across_fragments() {
        let mut state = VTermState::new(1, 1);
        assert_eq!(
            decode_text_codepoints(&mut state, b"\xE2\x82", true),
            (Vec::new(), 2)
        );
        assert_eq!(
            decode_text_codepoints(&mut state, b"\xAC", true),
            (vec![0x20AC], 1)
        );
    }

    #[test]
    fn text_decoder_single_shift_consumes_one_character_and_resets() {
        let mut state = VTermState::new(1, 1);
        state.gsingle_set = 2;
        let (codepoints, consumed) = decode_text_codepoints(&mut state, b"AB", false);
        assert_eq!(codepoints, [65]);
        assert_eq!(consumed, 1);
        assert_eq!(state.gsingle_set, 0);
    }
}

pub fn append_grapheme(
    codepoints: &[u32],
    mut index: usize,
    buffer: &mut [u8],
    mut length: usize,
    grapheme_state: &mut crate::mbyte_defs::GraphemeState,
) -> (usize, usize) {
    loop {
        if length < buffer.len().saturating_sub(4) {
            length += usize::try_from(crate::mbyte::utf_char2bytes(
                codepoints[index] as i32,
                &mut buffer[length..],
            ))
            .unwrap_or(0);
        }
        index += 1;
        if index >= codepoints.len()
            // SAFETY: codepoints are valid decoded Unicode scalar values
            // and grapheme_state is exclusively borrowed for this scan.
            || !unsafe {
                crate::mbyte::utf_iscomposing(
                    codepoints[index - 1] as i32,
                    codepoints[index] as i32,
                    grapheme_state,
                )
            }
        {
            return (index, length);
        }
    }
}

#[must_use]
pub fn begin_grapheme_recombine(
    state: &mut VTermState,
    first_codepoint: u32,
) -> (crate::mbyte_defs::GraphemeState, usize, bool) {
    if state.pos.row == state.combine_pos.row
        && state.pos.col >= state.combine_pos.col
        && state.pos.col <= state.combine_pos.col + state.combine_width
        // SAFETY: both values are decoded Unicode codepoints and the
        // stored grapheme state is exclusively borrowed.
        && unsafe {
            crate::mbyte::utf_iscomposing(
                state.grapheme_last as i32,
                first_codepoint as i32,
                &mut state.grapheme_state,
            )
        }
    {
        state.pos.col = state.combine_pos.col;
        state.at_phantom = false;
        (state.grapheme_state, state.grapheme_len, true)
    } else {
        (crate::mbyte_defs::GRAPHEME_STATE_INIT, 0, false)
    }
}

pub fn on_text<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    bytes: &[u8],
    utf8: bool,
) -> usize {
    let old_position = state.pos;
    let (codepoints, consumed) = decode_text_codepoints(state, bytes, utf8);
    let Some(&first_codepoint) = codepoints.first() else {
        return consumed;
    };
    let (mut grapheme_state, mut grapheme_len, mut recombine) =
        begin_grapheme_recombine(state, first_codepoint);
    let mut index = 0;

    while index < codepoints.len() {
        (index, grapheme_len) = append_grapheme(
            &codepoints,
            index,
            &mut state.grapheme_buf,
            grapheme_len,
            &mut grapheme_state,
        );
        // SAFETY: grapheme_buf[..grapheme_len] contains complete UTF-8
        // characters produced by utf_char2bytes.
        let width = unsafe {
            crate::mbyte::utf_ptr2cells_len(
                &state.grapheme_buf[..grapheme_len],
                grapheme_len as i32,
            )
        };
        if state.at_phantom || state.pos.col + width > state.current_row_width() {
            linefeed(state, callbacks);
            state.pos.col = 0;
            state.at_phantom = false;
            state.lineinfos[state.active_lineinfo][state.pos.row as usize].continuation = true;
        }

        if state.mode.insert && !recombine {
            scroll(
                state,
                callbacks,
                crate::vterm_defs::VTermRect {
                    start_row: state.pos.row,
                    end_row: state.pos.row + 1,
                    start_col: state.pos.col,
                    end_col: state.current_row_width(),
                },
                0,
                -1,
            );
        }

        let schar = crate::grid::schar_from_buf(&state.grapheme_buf[..grapheme_len]);
        putglyph(
            state,
            callbacks,
            schar,
            width,
            state.pos,
            state.protected_cell,
        );

        if index == codepoints.len() {
            state.grapheme_len = grapheme_len;
            state.grapheme_last = codepoints[index - 1];
            state.grapheme_state = grapheme_state;
            state.combine_width = width;
            state.combine_pos = state.pos;
        } else {
            grapheme_len = 0;
            recombine = false;
        }

        if state.pos.col + width >= state.current_row_width() {
            if state.mode.autowrap {
                state.at_phantom = true;
            }
        } else {
            state.pos.col += width;
        }
    }

    updatecursor(state, callbacks, old_position, false);
    consumed
}

#[cfg(test)]
mod on_text_tests {
    use super::*;

    #[derive(Default)]
    struct Capture {
        glyphs: Vec<(Vec<u8>, crate::vterm_defs::VTermPos)>,
        scrolls: Vec<(crate::vterm_defs::VTermRect, i32)>,
        moves: Vec<crate::vterm_defs::VTermPos>,
    }

    impl VTermStateCallbacks for Capture {
        fn put_glyph(
            &mut self,
            info: &crate::vterm_defs::VTermGlyphInfo,
            position: crate::vterm_defs::VTermPos,
        ) -> bool {
            self.glyphs.push((crate::grid::schar_get(info.schar), position));
            true
        }

        fn scroll_rect(
            &mut self,
            rect: crate::vterm_defs::VTermRect,
            _: i32,
            rightward: i32,
        ) -> bool {
            self.scrolls.push((rect, rightward));
            true
        }

        fn move_cursor(
            &mut self,
            position: crate::vterm_defs::VTermPos,
            _: crate::vterm_defs::VTermPos,
            _: bool,
        ) -> bool {
            self.moves.push(position);
            true
        }
    }

    fn reset_state(rows: i32, cols: i32) -> VTermState {
        let mut state = VTermState::new(rows, cols);
        state.reset(false);
        state
    }

    #[test]
    fn text_emits_ascii_glyphs_and_advances_the_cursor() {
        let mut state = reset_state(2, 8);
        let mut capture = Capture::default();
        assert_eq!(on_text(&mut state, &mut capture, b"ABC", false), 3);
        assert_eq!(
            capture.glyphs,
            [
                (b"A".to_vec(), crate::vterm_defs::VTermPos { row: 0, col: 0 }),
                (b"B".to_vec(), crate::vterm_defs::VTermPos { row: 0, col: 1 }),
                (b"C".to_vec(), crate::vterm_defs::VTermPos { row: 0, col: 2 }),
            ]
        );
        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 0, col: 3 });
        assert_eq!(capture.moves, [state.pos]);
        assert_eq!(&state.grapheme_buf[..state.grapheme_len], b"C");
    }

    #[test]
    fn text_wraps_from_phantom_column_and_marks_continuation() {
        let mut state = reset_state(2, 2);
        let mut capture = Capture::default();
        assert_eq!(on_text(&mut state, &mut capture, b"ABC", false), 3);
        assert_eq!(
            capture.glyphs[2].1,
            crate::vterm_defs::VTermPos { row: 1, col: 0 }
        );
        assert!(state.lineinfos[0][1].continuation);
        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 1, col: 1 });
    }

    #[test]
    fn text_insert_mode_scrolls_before_each_new_glyph() {
        let mut state = reset_state(1, 5);
        state.mode.insert = true;
        state.pos.col = 2;
        let mut capture = Capture::default();
        assert_eq!(on_text(&mut state, &mut capture, b"X", false), 1);
        assert_eq!(
            capture.scrolls,
            [(
                crate::vterm_defs::VTermRect {
                    start_row: 0,
                    end_row: 1,
                    start_col: 2,
                    end_col: 5,
                },
                -1,
            )]
        );
    }

    #[test]
    fn text_recombines_a_mark_received_in_the_next_fragment() {
        let mut state = reset_state(1, 8);
        let mut capture = Capture::default();
        assert_eq!(on_text(&mut state, &mut capture, b"A", true), 1);
        assert_eq!(
            on_text(&mut state, &mut capture, "\u{301}".as_bytes(), true),
            2
        );
        assert_eq!(capture.glyphs[1].0, "A\u{301}".as_bytes());
        assert_eq!(
            capture.glyphs[1].1,
            crate::vterm_defs::VTermPos { row: 0, col: 0 }
        );
        assert_eq!(state.pos.col, 1);
    }
}

#[cfg(test)]
mod grapheme_recombine_tests {
    use super::*;

    #[test]
    fn recombine_rewinds_the_cursor_for_a_following_combining_mark() {
        let mut state = VTermState::new(1, 8);
        state.grapheme_buf[0] = b'A';
        state.grapheme_len = 1;
        state.grapheme_last = b'A' as u32;
        state.combine_pos = crate::vterm_defs::VTermPos { row: 0, col: 2 };
        state.combine_width = 1;
        state.pos = crate::vterm_defs::VTermPos { row: 0, col: 3 };
        state.at_phantom = true;

        let (_, length, recombine) = begin_grapheme_recombine(&mut state, 0x0301);
        assert!(recombine);
        assert_eq!(length, 1);
        assert_eq!(state.pos.col, 2);
        assert!(!state.at_phantom);
    }

    #[test]
    fn recombine_leaves_unrelated_text_at_the_current_cursor() {
        let mut state = VTermState::new(1, 8);
        state.grapheme_last = b'A' as u32;
        state.combine_pos = crate::vterm_defs::VTermPos { row: 0, col: 2 };
        state.combine_width = 1;
        state.pos = crate::vterm_defs::VTermPos { row: 0, col: 3 };
        let (_, length, recombine) = begin_grapheme_recombine(&mut state, b'B' as u32);
        assert!(!recombine);
        assert_eq!(length, 0);
        assert_eq!(state.pos.col, 3);
    }
}

#[cfg(test)]
mod append_grapheme_tests {
    use super::*;

    #[test]
    fn grapheme_builder_appends_a_base_and_combining_mark() {
        let codepoints = [b'A' as u32, 0x0301, b'B' as u32];
        let mut buffer = [0; crate::types_defs::MAX_SCHAR_SIZE];
        let mut grapheme_state = crate::mbyte_defs::GRAPHEME_STATE_INIT;
        let (index, length) =
            append_grapheme(&codepoints, 0, &mut buffer, 0, &mut grapheme_state);
        assert_eq!(index, 2);
        assert_eq!(&buffer[..length], "A\u{301}".as_bytes());

        let (index, length) =
            append_grapheme(&codepoints, index, &mut buffer, 0, &mut grapheme_state);
        assert_eq!(index, 3);
        assert_eq!(&buffer[..length], b"B");
    }
}

pub fn control_reverse_index<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
) {
    if state.pos.row == state.scrollregion_top {
        let rect = crate::vterm_defs::VTermRect {
            start_row: state.scrollregion_top,
            end_row: state.scrollregion_bottom(),
            start_col: state.scrollregion_left(),
            end_col: state.scrollregion_right(),
        };
        scroll(state, callbacks, rect, -1, 0);
    } else if state.pos.row > 0 {
        state.pos.row -= 1;
    }
}

pub fn control_tabstop(state: &mut VTermState) {
    state.set_col_tabstop(state.pos.col);
}

pub fn control_next_line<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
) {
    linefeed(state, callbacks);
    state.pos.col = 0;
}

pub fn on_control<C: VTermStateCallbacks, F: VTermStateFallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    fallbacks: &mut F,
    control: u8,
) -> i32 {
    let old = state.pos;
    match control {
        0x07 => {
            let _ = callbacks.bell();
        }
        0x08 => control_backspace(state),
        0x09 => control_horizontal_tab(state),
        0x0A..=0x0C => control_linefeed(state, callbacks),
        0x0D => control_carriage_return(state),
        0x0E | 0x0F => {
            control_locking_shift(state, control);
        }
        0x84 => linefeed(state, callbacks),
        0x85 => control_next_line(state, callbacks),
        0x88 => control_tabstop(state),
        0x8D => control_reverse_index(state, callbacks),
        0x8E | 0x8F => {
            control_single_shift(state, control);
        }
        _ => return i32::from(fallbacks.control(control)),
    }
    updatecursor(state, callbacks, old, true);
    1
}

pub fn csi_insert_chars<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    count: i32,
) {
    if !state.is_cursor_in_scrollregion() {
        return;
    }
    let rect = crate::vterm_defs::VTermRect {
        start_row: state.pos.row,
        end_row: state.pos.row + 1,
        start_col: state.pos.col,
        end_col: if state.mode.leftrightmargin {
            state.scrollregion_right()
        } else {
            state.current_row_width()
        },
    };
    scroll(state, callbacks, rect, 0, -count.max(1));
}

pub fn csi_delete_chars<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    count: i32,
) {
    if !state.is_cursor_in_scrollregion() {
        return;
    }
    let rect = crate::vterm_defs::VTermRect {
        start_row: state.pos.row,
        end_row: state.pos.row + 1,
        start_col: state.pos.col,
        end_col: if state.mode.leftrightmargin {
            state.scrollregion_right()
        } else {
            state.current_row_width()
        },
    };
    scroll(state, callbacks, rect, 0, count.max(1));
}

pub fn csi_insert_lines<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    count: i32,
) {
    if state.is_cursor_in_scrollregion() {
        scroll(
            state,
            callbacks,
            crate::vterm_defs::VTermRect {
                start_row: state.pos.row,
                end_row: state.scrollregion_bottom(),
                start_col: state.scrollregion_left(),
                end_col: state.scrollregion_right(),
            },
            -count.max(1),
            0,
        );
    }
}

pub fn csi_delete_lines<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    count: i32,
) {
    if state.is_cursor_in_scrollregion() {
        scroll(
            state,
            callbacks,
            crate::vterm_defs::VTermRect {
                start_row: state.pos.row,
                end_row: state.scrollregion_bottom(),
                start_col: state.scrollregion_left(),
                end_col: state.scrollregion_right(),
            },
            count.max(1),
            0,
        );
    }
}

pub fn csi_insert_columns<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    count: i32,
) {
    if state.is_cursor_in_scrollregion() {
        scroll(
            state,
            callbacks,
            crate::vterm_defs::VTermRect {
                start_row: state.scrollregion_top,
                end_row: state.scrollregion_bottom(),
                start_col: state.pos.col,
                end_col: state.scrollregion_right(),
            },
            0,
            -count.max(1),
        );
    }
}

pub fn csi_delete_columns<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    count: i32,
) {
    if state.is_cursor_in_scrollregion() {
        scroll(
            state,
            callbacks,
            crate::vterm_defs::VTermRect {
                start_row: state.scrollregion_top,
                end_row: state.scrollregion_bottom(),
                start_col: state.pos.col,
                end_col: state.scrollregion_right(),
            },
            0,
            count.max(1),
        );
    }
}

#[cfg(test)]
mod csi_column_tests {
    use super::*;

    #[derive(Default)]
    struct Capture {
        rect: Option<crate::vterm_defs::VTermRect>,
        rightward: i32,
    }

    impl VTermStateCallbacks for Capture {
        fn scroll_rect(
            &mut self,
            rect: crate::vterm_defs::VTermRect,
            _: i32,
            rightward: i32,
        ) -> bool {
            self.rect = Some(rect);
            self.rightward = rightward;
            true
        }
    }

    #[test]
    fn insert_and_delete_columns_scroll_the_vertical_region() {
        let mut state = VTermState::new(10, 20);
        state.scrollregion_top = 2;
        state.scrollregion_bottom = 8;
        state.scrollregion_left = 1;
        state.scrollregion_right = 18;
        state.mode.leftrightmargin = true;
        state.pos = crate::vterm_defs::VTermPos { row: 4, col: 5 };
        let expected = crate::vterm_defs::VTermRect {
            start_row: 2,
            end_row: 8,
            start_col: 5,
            end_col: 18,
        };

        let mut capture = Capture::default();
        csi_insert_columns(&mut state, &mut capture, 3);
        assert_eq!(capture.rect, Some(expected));
        assert_eq!(capture.rightward, -3);

        csi_delete_columns(&mut state, &mut capture, 2);
        assert_eq!(capture.rect, Some(expected));
        assert_eq!(capture.rightward, 2);
    }

    #[test]
    fn column_operations_ignore_a_cursor_outside_the_scroll_region() {
        let mut state = VTermState::new(10, 20);
        state.scrollregion_top = 2;
        state.scrollregion_bottom = 8;
        state.pos = crate::vterm_defs::VTermPos { row: 1, col: 5 };
        let mut capture = Capture::default();
        csi_insert_columns(&mut state, &mut capture, 1);
        csi_delete_columns(&mut state, &mut capture, 1);
        assert_eq!(capture.rect, None);
    }
}

pub fn csi_scroll_region<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    count: i32,
) {
    scroll(
        state,
        callbacks,
        crate::vterm_defs::VTermRect {
            start_row: state.scrollregion_top,
            end_row: state.scrollregion_bottom(),
            start_col: state.scrollregion_left(),
            end_col: state.scrollregion_right(),
        },
        count,
        0,
    );
}

pub fn csi_erase_chars<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    count: i32,
) {
    let end_col = (state.pos.col + count.max(1)).min(state.current_row_width());
    erase(
        state,
        callbacks,
        crate::vterm_defs::VTermRect {
            start_row: state.pos.row,
            end_row: state.pos.row + 1,
            start_col: state.pos.col,
            end_col,
        },
        false,
    );
}

pub fn csi_erase_display<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    value: i32,
    selective: bool,
) -> bool {
    let reset_lineinfo = |state: &mut VTermState, callbacks: &mut C, row| {
        state.set_lineinfo(row, FORCE, DWL_OFF, DHL_OFF, |row, new, old| {
            callbacks.set_line_info(row, new, old)
        });
    };
    match value {
        0 => {
            if state.cols > state.pos.col {
                erase(
                    state,
                    callbacks,
                    crate::vterm_defs::VTermRect {
                        start_row: state.pos.row,
                        end_row: state.pos.row + 1,
                        start_col: state.pos.col,
                        end_col: state.cols,
                    },
                    selective,
                );
            }
            for row in state.pos.row + 1..state.rows {
                reset_lineinfo(state, callbacks, row);
            }
            if state.rows > state.pos.row + 1 {
                erase(
                    state,
                    callbacks,
                    crate::vterm_defs::VTermRect {
                        start_row: state.pos.row + 1,
                        end_row: state.rows,
                        start_col: 0,
                        end_col: state.cols,
                    },
                    selective,
                );
            }
        }
        1 => {
            for row in 0..state.pos.row {
                reset_lineinfo(state, callbacks, row);
            }
            if state.pos.row > 0 {
                erase(
                    state,
                    callbacks,
                    crate::vterm_defs::VTermRect {
                        start_row: 0,
                        end_row: state.pos.row,
                        start_col: 0,
                        end_col: state.cols,
                    },
                    selective,
                );
            }
            erase(
                state,
                callbacks,
                crate::vterm_defs::VTermRect {
                    start_row: state.pos.row,
                    end_row: state.pos.row + 1,
                    start_col: 0,
                    end_col: state.pos.col + 1,
                },
                selective,
            );
        }
        2 => {
            for row in 0..state.rows {
                reset_lineinfo(state, callbacks, row);
            }
            erase(
                state,
                callbacks,
                crate::vterm_defs::VTermRect {
                    start_row: 0,
                    end_row: state.rows,
                    start_col: 0,
                    end_col: state.cols,
                },
                selective,
            );
        }
        3 => {
            let _ = callbacks.sb_clear();
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod csi_erase_display_tests {
    use super::*;

    #[derive(Default)]
    struct Capture {
        erased: Vec<(crate::vterm_defs::VTermRect, bool)>,
        scrollback_clears: usize,
    }

    impl VTermStateCallbacks for Capture {
        fn erase(&mut self, rect: crate::vterm_defs::VTermRect, selective: bool) -> bool {
            self.erased.push((rect, selective));
            true
        }

        fn sb_clear(&mut self) -> bool {
            self.scrollback_clears += 1;
            true
        }
    }

    #[test]
    fn erase_display_below_clears_cursor_tail_and_following_rows() {
        let mut state = VTermState::new(4, 8);
        state.pos = crate::vterm_defs::VTermPos { row: 1, col: 3 };
        state.lineinfos[0][2].doublewidth = true;
        let mut capture = Capture::default();
        assert!(csi_erase_display(&mut state, &mut capture, 0, true));
        assert_eq!(
            capture.erased,
            [
                (
                    crate::vterm_defs::VTermRect {
                        start_row: 1,
                        end_row: 2,
                        start_col: 3,
                        end_col: 8,
                    },
                    true,
                ),
                (
                    crate::vterm_defs::VTermRect {
                        start_row: 2,
                        end_row: 4,
                        start_col: 0,
                        end_col: 8,
                    },
                    true,
                ),
            ]
        );
        assert!(!state.lineinfos[0][2].doublewidth);
    }

    #[test]
    fn erase_display_above_clears_preceding_rows_and_cursor_head() {
        let mut state = VTermState::new(4, 8);
        state.pos = crate::vterm_defs::VTermPos { row: 2, col: 3 };
        let mut capture = Capture::default();
        assert!(csi_erase_display(&mut state, &mut capture, 1, false));
        assert_eq!(
            capture.erased,
            [
                (
                    crate::vterm_defs::VTermRect {
                        start_row: 0,
                        end_row: 2,
                        start_col: 0,
                        end_col: 8,
                    },
                    false,
                ),
                (
                    crate::vterm_defs::VTermRect {
                        start_row: 2,
                        end_row: 3,
                        start_col: 0,
                        end_col: 4,
                    },
                    false,
                ),
            ]
        );
    }

    #[test]
    fn erase_display_all_resets_every_line_and_cell() {
        let mut state = VTermState::new(2, 3);
        state.lineinfos[0][0].doublewidth = true;
        state.lineinfos[0][1].doubleheight = 1;
        let mut capture = Capture::default();
        assert!(csi_erase_display(&mut state, &mut capture, 2, false));
        assert_eq!(capture.erased[0].0, crate::vterm_defs::VTermRect {
            start_row: 0,
            end_row: 2,
            start_col: 0,
            end_col: 3,
        });
        assert_eq!(state.lineinfos[0], vec![Default::default(); 2]);
    }

    #[test]
    fn erase_display_three_clears_scrollback_and_rejects_other_values() {
        let mut state = VTermState::new(1, 1);
        let mut capture = Capture::default();
        assert!(csi_erase_display(&mut state, &mut capture, 3, false));
        assert_eq!(capture.scrollback_clears, 1);
        assert!(!csi_erase_display(&mut state, &mut capture, 4, false));
    }
}

pub fn csi_tab_clear(state: &mut VTermState, value: i32) -> bool {
    match value {
        0 => state.clear_col_tabstop(state.pos.col),
        3 | 5 => state.tabstops.fill(0),
        1 | 2 | 4 => {}
        _ => return false,
    }
    true
}

pub fn csi_erase_line<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    value: i32,
    selective: bool,
) -> bool {
    let (start_col, end_col) = match value {
        0 => (state.pos.col, state.current_row_width()),
        1 => (0, state.pos.col + 1),
        2 => (0, state.current_row_width()),
        _ => return false,
    };
    if end_col > start_col {
        erase(
            state,
            callbacks,
            crate::vterm_defs::VTermRect {
                start_row: state.pos.row,
                end_row: state.pos.row + 1,
                start_col,
                end_col,
            },
            selective,
        );
    }
    true
}

pub fn csi_tab(state: &mut VTermState, count: i32, forward: bool) {
    state.tab(count.max(1), if forward { 1 } else { -1 });
}

#[must_use]
pub fn csi_device_status(
    state: &VTermState,
    value: i32,
    private: bool,
    ctrl8bit: bool,
) -> Vec<u8> {
    let prefix: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_CSI] } else { b"\x1b[" };
    let question = if private { "?" } else { "" };
    match value {
        5 => [prefix, format!("{question}0n").as_bytes()].concat(),
        6 => [
            prefix,
            format!("{question}{};{}R", state.pos.row + 1, state.pos.col + 1).as_bytes(),
        ]
        .concat(),
        _ => Vec::new(),
    }
}

#[must_use]
pub fn csi_theme_status<C: VTermStateCallbacks>(
    callbacks: &mut C,
    ctrl8bit: bool,
) -> Vec<u8> {
    let mut dark = false;
    if !callbacks.theme(&mut dark) {
        return Vec::new();
    }
    let prefix: &[u8] = if ctrl8bit {
        &[crate::vterm_defs::C1_CSI]
    } else {
        b"\x1b["
    };
    [prefix, format!("?997;{}n", if dark { 1 } else { 2 }).as_bytes()].concat()
}

#[cfg(test)]
mod csi_theme_status_tests {
    use super::*;

    struct Capture {
        handled: bool,
        dark: bool,
    }

    impl VTermStateCallbacks for Capture {
        fn theme(&mut self, dark: &mut bool) -> bool {
            *dark = self.dark;
            self.handled
        }
    }

    #[test]
    fn theme_status_reports_dark_light_or_no_response() {
        assert_eq!(
            csi_theme_status(
                &mut Capture {
                    handled: true,
                    dark: true,
                },
                false,
            ),
            b"\x1b[?997;1n"
        );
        assert_eq!(
            csi_theme_status(
                &mut Capture {
                    handled: true,
                    dark: false,
                },
                true,
            ),
            [vec![crate::vterm_defs::C1_CSI], b"?997;2n".to_vec()].concat()
        );
        assert!(csi_theme_status(
            &mut Capture {
                handled: false,
                dark: true,
            },
            false,
        )
        .is_empty());
    }
}

pub fn csi_repeat<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    count: i32,
) -> bool {
    let width = state.combine_width.max(1);
    let end = (state.pos.col + count.max(1)).min(state.current_row_width());
    let schar = crate::grid::schar_from_buf(&state.grapheme_buf[..state.grapheme_len]);
    while state.pos.col < end {
        putglyph(state, callbacks, schar, width, state.pos, state.protected_cell);
        state.pos.col += width;
    }
    let preserve_phantom =
        state.pos.col + width >= state.current_row_width() && state.mode.autowrap;
    if preserve_phantom {
        state.at_phantom = true;
    }
    preserve_phantom
}

pub fn csi_dec_sgr<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    args: &[crate::vterm::parser::CsiArg],
) {
    for &arg in args {
        let mapped = match crate::vterm::parser::csi_arg(arg) {
            4 => 73,
            5 => 74,
            24 => 75,
            _ => continue,
        };
        state.setpen(callbacks, &[mapped]);
    }
}

fn csi_raw_arg(
    args: &[crate::vterm::parser::CsiArg],
    index: usize,
) -> crate::vterm::parser::CsiArg {
    args.get(index)
        .copied()
        .unwrap_or(crate::vterm::parser::CSI_ARG_MISSING)
}

fn csi_arg_value(args: &[crate::vterm::parser::CsiArg], index: usize) -> i32 {
    crate::vterm::parser::csi_arg(csi_raw_arg(args, index)) as i32
}

fn csi_arg_or(args: &[crate::vterm::parser::CsiArg], index: usize, default: i32) -> i32 {
    crate::vterm::parser::csi_arg_or(csi_raw_arg(args, index), default as u32) as i32
}

fn csi_arg_count(args: &[crate::vterm::parser::CsiArg], index: usize) -> i32 {
    crate::vterm::parser::csi_arg_count(csi_raw_arg(args, index)) as i32
}

pub fn on_csi<C: VTermStateCallbacks, F: VTermStateFallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    fallbacks: &mut F,
    leader: Option<&[u8]>,
    args: &[crate::vterm::parser::CsiArg],
    intermed: Option<&[u8]>,
    command: u8,
) -> (i32, Vec<u8>) {
    let leader_byte = match leader.unwrap_or_default() {
        [] => 0,
        [byte @ (b'?' | b'>' | b'<' | b'=')] => *byte,
        _ => return (0, Vec::new()),
    };
    let intermed_byte = match intermed.unwrap_or_default() {
        [] => 0,
        [byte @ (b' ' | b'!' | b'"' | b'$' | b'\'')] => *byte,
        _ => return (0, Vec::new()),
    };
    let old_position = state.pos;
    let mut cancel_phantom = true;
    let mut output = Vec::new();

    match (leader_byte, intermed_byte, command) {
        (0, 0, b'@') => csi_insert_chars(state, callbacks, csi_arg_count(args, 0)),
        (0, 0, command @ (b'A'..=b'F' | b'a' | b'e' | b'j' | b'k')) => {
            state.csi_move_cursor(command, csi_arg_count(args, 0));
        }
        (0, 0, command @ (b'G' | b'`')) => {
            state.csi_position(command, 1, csi_arg_or(args, 0, 1));
        }
        (0, 0, command @ (b'H' | b'f')) => {
            state.csi_position(
                command,
                csi_arg_or(args, 0, 1),
                csi_arg_or(args, 1, 1),
            );
        }
        (0, 0, b'I') => csi_tab(state, csi_arg_count(args, 0), true),
        (leader @ (0 | b'?'), 0, b'J') => {
            csi_erase_display(
                state,
                callbacks,
                csi_arg_or(args, 0, 0),
                leader == b'?',
            );
        }
        (leader @ (0 | b'?'), 0, b'K') => {
            if !csi_erase_line(
                state,
                callbacks,
                csi_arg_or(args, 0, 0),
                leader == b'?',
            ) {
                return (0, Vec::new());
            }
        }
        (0, 0, b'L') => csi_insert_lines(state, callbacks, csi_arg_count(args, 0)),
        (0, 0, b'M') => csi_delete_lines(state, callbacks, csi_arg_count(args, 0)),
        (0, 0, b'P') => csi_delete_chars(state, callbacks, csi_arg_count(args, 0)),
        (0, 0, b'S') => {
            csi_scroll_region(state, callbacks, csi_arg_count(args, 0));
        }
        (0, 0, b'T') => {
            csi_scroll_region(state, callbacks, -csi_arg_count(args, 0));
        }
        (0, 0, b'X') => csi_erase_chars(state, callbacks, csi_arg_count(args, 0)),
        (0, 0, b'Z') => csi_tab(state, csi_arg_count(args, 0), false),
        (0, 0, b'b') => {
            cancel_phantom = !csi_repeat(state, callbacks, csi_arg_count(args, 0));
        }
        (0, 0, b'c') => {
            if csi_arg_or(args, 0, 0) == 0 {
                output = primary_device_attributes(state.ctrl8bit);
            }
        }
        (b'>', 0, b'c') => output = secondary_device_attributes(state.ctrl8bit),
        (0, 0, b'd') => {
            state.csi_position(b'd', csi_arg_or(args, 0, 1), 1);
        }
        (0, 0, b'g') => {
            if !csi_tab_clear(state, csi_arg_or(args, 0, 0)) {
                return (0, Vec::new());
            }
        }
        (leader @ (0 | b'?'), 0, b'h') => {
            csi_set_modes(state, callbacks, args, leader == b'?', true);
        }
        (leader @ (0 | b'?'), 0, b'l') => {
            csi_set_modes(state, callbacks, args, leader == b'?', false);
        }
        (0, 0, b'm') => state.setpen(callbacks, args),
        (b'?', 0, b'm') => csi_dec_sgr(state, callbacks, args),
        (leader @ (0 | b'?'), 0, b'n') => {
            let value = csi_arg_or(args, 0, 0);
            output = if value == 996 {
                csi_theme_status(callbacks, state.ctrl8bit)
            } else {
                csi_device_status(state, value, leader == b'?', state.ctrl8bit)
            };
        }
        (0, b'!', b'p') => csi_soft_reset(state, callbacks),
        (b'?', b'$', b'p') => {
            output = request_dec_mode(state, csi_arg_value(args, 0), state.ctrl8bit);
        }
        (b'>', 0, b'q') => output = request_version_string(state.ctrl8bit),
        (0, b' ', b'q') => {
            csi_set_cursor_style(state, callbacks, csi_arg_or(args, 0, 1));
        }
        (0, b'"', b'q') => {
            csi_set_character_protection(state, csi_arg_or(args, 0, 0));
        }
        (0, 0, b'r') => {
            let bottom = args
                .get(1)
                .copied()
                .filter(|&arg| !crate::vterm::parser::csi_arg_is_missing(arg))
                .map(|arg| crate::vterm::parser::csi_arg(arg) as i32);
            state.set_vertical_margins(csi_arg_or(args, 0, 1), bottom);
        }
        (0, 0, b's') => {
            let right = args
                .get(1)
                .copied()
                .filter(|&arg| !crate::vterm::parser::csi_arg_is_missing(arg))
                .map(|arg| crate::vterm::parser::csi_arg(arg) as i32);
            state.set_horizontal_margins(csi_arg_or(args, 0, 1), right);
        }
        (leader @ (b'?' | b'>' | b'<' | b'='), 0, b'u') => {
            output = csi_key_encoding(state, leader, args, state.ctrl8bit)
                .expect("validated key-encoding leader");
        }
        (0, b'\'', b'}') => {
            csi_insert_columns(state, callbacks, csi_arg_count(args, 0));
        }
        (0, b'\'', b'~') => {
            csi_delete_columns(state, callbacks, csi_arg_count(args, 0));
        }
        _ => {
            return (
                i32::from(fallbacks.csi(leader, args, intermed, command)),
                Vec::new(),
            );
        }
    }

    if state.mode.origin {
        state.pos.row = state
            .pos
            .row
            .clamp(state.scrollregion_top, state.scrollregion_bottom() - 1);
        state.pos.col = state
            .pos
            .col
            .clamp(state.scrollregion_left(), state.scrollregion_right() - 1);
    } else {
        state.pos.row = state.pos.row.clamp(0, state.rows - 1);
        state.pos.col = state.pos.col.clamp(0, state.current_row_width() - 1);
    }
    updatecursor(state, callbacks, old_position, cancel_phantom);
    (1, output)
}

#[cfg(test)]
mod on_csi_tests {
    use super::*;

    #[derive(Default)]
    struct Capture {
        moves: Vec<crate::vterm_defs::VTermPos>,
        props: Vec<crate::vterm_defs::VTermProp>,
    }

    impl VTermStateCallbacks for Capture {
        fn move_cursor(
            &mut self,
            position: crate::vterm_defs::VTermPos,
            _: crate::vterm_defs::VTermPos,
            _: bool,
        ) -> bool {
            self.moves.push(position);
            true
        }

        fn set_term_prop(
            &mut self,
            prop: crate::vterm_defs::VTermProp,
            _: &crate::vterm_defs::VTermValue<'_>,
        ) -> bool {
            self.props.push(prop);
            true
        }
    }

    #[derive(Default)]
    struct Fallback(usize);

    impl VTermStateFallbacks for Fallback {
        fn csi(
            &mut self,
            _: Option<&[u8]>,
            _: &[crate::vterm::parser::CsiArg],
            _: Option<&[u8]>,
            _: u8,
        ) -> bool {
            self.0 += 1;
            true
        }
    }

    #[test]
    fn csi_dispatch_moves_clamps_and_notifies_the_cursor() {
        let mut state = VTermState::new(4, 5);
        state.pos = crate::vterm_defs::VTermPos { row: 1, col: 2 };
        let mut capture = Capture::default();
        assert_eq!(
            on_csi(
                &mut state,
                &mut capture,
                &mut (),
                None,
                &[20],
                None,
                b'C',
            ),
            (1, Vec::new())
        );
        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 1, col: 4 });
        assert_eq!(capture.moves, [state.pos]);
    }

    #[test]
    fn csi_dispatch_handles_modes_pen_margins_and_presentation() {
        let mut state = VTermState::new(6, 10);
        state.initialize_pen_colors();
        let mut capture = Capture::default();
        assert_eq!(
            on_csi(
                &mut state,
                &mut capture,
                &mut (),
                Some(b"?"),
                &[7, 25],
                None,
                b'h',
            )
            .0,
            1
        );
        assert!(state.mode.autowrap);
        assert!(state.mode.cursor_visible);
        assert_eq!(
            on_csi(
                &mut state,
                &mut capture,
                &mut (),
                None,
                &[1, 3],
                None,
                b'm',
            )
            .0,
            1
        );
        assert!(state.pen.bold);
        assert!(state.pen.italic);
        assert_eq!(
            on_csi(
                &mut state,
                &mut capture,
                &mut (),
                None,
                &[2, 5],
                None,
                b'r',
            )
            .0,
            1
        );
        assert_eq!((state.scrollregion_top, state.scrollregion_bottom), (1, 5));
        assert_eq!(
            on_csi(
                &mut state,
                &mut capture,
                &mut (),
                None,
                &[6],
                Some(b" "),
                b'q',
            )
            .0,
            1
        );
        assert_eq!(
            state.mode.cursor_shape,
            crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BAR_LEFT as u8
        );
    }

    #[test]
    fn csi_dispatch_returns_device_and_version_responses() {
        let mut state = VTermState::new(4, 5);
        state.pos = crate::vterm_defs::VTermPos { row: 1, col: 2 };
        let mut capture = Capture::default();
        assert_eq!(
            on_csi(
                &mut state,
                &mut capture,
                &mut (),
                Some(b"?"),
                &[6],
                None,
                b'n',
            )
            .1,
            b"\x1b[?2;3R"
        );
        assert_eq!(
            on_csi(
                &mut state,
                &mut capture,
                &mut (),
                Some(b">"),
                &[],
                None,
                b'q',
            )
            .1,
            b"\x1bP>|libvterm(0.3)\x1b\\"
        );
    }

    #[test]
    fn csi_dispatch_routes_unknown_sequences_to_fallbacks() {
        let mut state = VTermState::new(1, 1);
        let mut fallback = Fallback::default();
        assert_eq!(
            on_csi(
                &mut state,
                &mut (),
                &mut fallback,
                None,
                &[],
                None,
                b'?',
            ),
            (1, Vec::new())
        );
        assert_eq!(fallback.0, 1);
        assert_eq!(
            on_csi(
                &mut state,
                &mut (),
                &mut fallback,
                Some(b"??"),
                &[],
                None,
                b'?',
            ),
            (0, Vec::new())
        );
        assert_eq!(fallback.0, 1);
    }
}

#[must_use]
pub fn csi_key_encoding(
    state: &mut VTermState,
    leader: u8,
    args: &[crate::vterm::parser::CsiArg],
    ctrl8bit: bool,
) -> Option<Vec<u8>> {
    let first = args
        .first()
        .copied()
        .unwrap_or(crate::vterm::parser::CSI_ARG_MISSING);
    match leader {
        b'?' => Some(request_key_encoding_flags(state, ctrl8bit)),
        b'>' => {
            push_key_encoding_flags(
                state,
                crate::vterm::parser::csi_arg_or(first, 0) as i32,
            );
            Some(Vec::new())
        }
        b'<' => {
            pop_key_encoding_flags(
                state,
                crate::vterm::parser::csi_arg_or(first, 1) as i32,
            );
            Some(Vec::new())
        }
        b'=' => {
            let mode = args
                .get(1)
                .copied()
                .filter(|&arg| !crate::vterm::parser::csi_arg_is_missing(arg))
                .map_or(1, |arg| crate::vterm::parser::csi_arg(arg) as i32);
            set_key_encoding_flags(
                state,
                crate::vterm::parser::csi_arg_or(first, 0) as i32,
                mode,
            );
            Some(Vec::new())
        }
        _ => None,
    }
}

pub fn csi_set_cursor_style<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    value: i32,
) {
    let (blink, shape) = match value {
        0 | 1 => (1, crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BLOCK),
        2 => (0, crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BLOCK),
        3 => (1, crate::vterm_defs::VTERM_PROP_CURSORSHAPE_UNDERLINE),
        4 => (0, crate::vterm_defs::VTERM_PROP_CURSORSHAPE_UNDERLINE),
        5 => (1, crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BAR_LEFT),
        6 => (0, crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BAR_LEFT),
        _ => return,
    };
    set_state_bool_prop(
        state,
        callbacks,
        crate::vterm_defs::VTermProp::CursorBlink,
        blink,
    );
    set_state_int_prop(
        state,
        callbacks,
        crate::vterm_defs::VTermProp::CursorShape,
        shape,
    );
}

pub fn csi_set_character_protection(state: &mut VTermState, value: i32) {
    state.set_character_protection(value);
}

pub fn csi_set_modes<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    args: &[crate::vterm::parser::CsiArg],
    private: bool,
    set: bool,
) {
    let value = i32::from(set);
    if private {
        for &arg in args {
            if !crate::vterm::parser::csi_arg_is_missing(arg) {
                set_dec_mode(
                    state,
                    callbacks,
                    crate::vterm::parser::csi_arg(arg) as i32,
                    value,
                );
            }
        }
    } else if let Some(&arg) = args.first()
        && !crate::vterm::parser::csi_arg_is_missing(arg)
    {
        state.set_mode(crate::vterm::parser::csi_arg(arg) as i32, value);
    }
}

#[cfg(test)]
mod csi_mode_tests {
    use super::*;

    #[test]
    fn standard_mode_commands_use_the_first_argument() {
        let mut state = VTermState::new(1, 1);
        csi_set_modes(&mut state, &mut (), &[4, 20], false, true);
        assert!(state.mode.insert);
        assert!(!state.mode.newline);
        csi_set_modes(&mut state, &mut (), &[4], false, false);
        assert!(!state.mode.insert);
    }

    #[test]
    fn private_mode_commands_apply_every_argument() {
        #[derive(Default)]
        struct Accept;
        impl VTermStateCallbacks for Accept {
            fn set_term_prop(
                &mut self,
                _: crate::vterm_defs::VTermProp,
                _: &crate::vterm_defs::VTermValue<'_>,
            ) -> bool {
                true
            }
        }

        let mut state = VTermState::new(1, 1);
        csi_set_modes(&mut state, &mut Accept, &[7, 25, 2004], true, true);
        assert!(state.mode.autowrap);
        assert!(state.mode.cursor_visible);
        assert!(state.mode.bracketpaste);
        csi_set_modes(&mut state, &mut Accept, &[7, 25, 2004], true, false);
        assert!(!state.mode.autowrap);
        assert!(!state.mode.cursor_visible);
        assert!(!state.mode.bracketpaste);
    }
}

#[cfg(test)]
mod csi_presentation_tests {
    use super::*;

    #[derive(Default)]
    struct Capture(Vec<crate::vterm_defs::VTermProp>);

    impl VTermStateCallbacks for Capture {
        fn set_term_prop(
            &mut self,
            prop: crate::vterm_defs::VTermProp,
            _: &crate::vterm_defs::VTermValue<'_>,
        ) -> bool {
            self.0.push(prop);
            true
        }
    }

    #[test]
    fn cursor_style_maps_blink_and_shape_properties() {
        let mut state = VTermState::new(1, 1);
        let mut capture = Capture::default();
        csi_set_cursor_style(&mut state, &mut capture, 3);
        assert!(state.mode.cursor_blink);
        assert_eq!(
            state.mode.cursor_shape,
            crate::vterm_defs::VTERM_PROP_CURSORSHAPE_UNDERLINE as u8
        );
        assert_eq!(
            capture.0,
            [
                crate::vterm_defs::VTermProp::CursorBlink,
                crate::vterm_defs::VTermProp::CursorShape,
            ]
        );

        csi_set_cursor_style(&mut state, &mut capture, 6);
        assert!(!state.mode.cursor_blink);
        assert_eq!(
            state.mode.cursor_shape,
            crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BAR_LEFT as u8
        );
    }

    #[test]
    fn character_protection_maps_decsca_values() {
        let mut state = VTermState::new(1, 1);
        csi_set_character_protection(&mut state, 1);
        assert!(state.protected_cell);
        csi_set_character_protection(&mut state, 2);
        assert!(!state.protected_cell);
    }
}

#[cfg(test)]
mod csi_key_encoding_tests {
    use super::*;

    #[test]
    fn key_encoding_query_reports_current_flags() {
        let mut state = VTermState::new(1, 1);
        set_key_encoding_flags(&mut state, 3, 1);
        assert_eq!(
            csi_key_encoding(&mut state, b'?', &[], false),
            Some(b"\x1b[?3u".to_vec())
        );
    }

    #[test]
    fn key_encoding_push_pop_and_set_update_current_stack_entry() {
        let mut state = VTermState::new(1, 1);
        assert_eq!(csi_key_encoding(&mut state, b'>', &[3], false), Some(Vec::new()));
        assert_eq!(state.key_encoding_stacks[0].size, 2);
        assert_eq!(state.key_encoding_stacks[0].current().bits(), 3);

        assert_eq!(csi_key_encoding(&mut state, b'=', &[4, 1], false), Some(Vec::new()));
        assert_eq!(state.key_encoding_stacks[0].current().bits(), 4);

        assert_eq!(csi_key_encoding(&mut state, b'<', &[1], false), Some(Vec::new()));
        assert_eq!(state.key_encoding_stacks[0].size, 1);
        assert_eq!(state.key_encoding_stacks[0].current().bits(), 0);
    }

    #[test]
    fn key_encoding_rejects_unknown_leader() {
        let mut state = VTermState::new(1, 1);
        assert_eq!(csi_key_encoding(&mut state, b'!', &[], false), None);
    }
}

#[cfg(test)]
mod csi_dec_sgr_tests {
    use super::*;

    #[test]
    fn dec_sgr_maps_super_subscript_and_normal_baseline() {
        let mut state = VTermState::new(1, 1);
        state.initialize_pen_colors();
        csi_dec_sgr(&mut state, &mut (), &[4]);
        assert!(state.pen.small);
        assert_eq!(
            state.pen.baseline,
            crate::vterm_defs::VTERM_BASELINE_RAISE
        );

        csi_dec_sgr(&mut state, &mut (), &[5]);
        assert_eq!(
            state.pen.baseline,
            crate::vterm_defs::VTERM_BASELINE_LOWER
        );

        csi_dec_sgr(&mut state, &mut (), &[24]);
        assert!(!state.pen.small);
        assert_eq!(
            state.pen.baseline,
            crate::vterm_defs::VTERM_BASELINE_NORMAL
        );
    }

    #[test]
    fn dec_sgr_ignores_unrecognized_arguments() {
        let mut state = VTermState::new(1, 1);
        state.initialize_pen_colors();
        csi_dec_sgr(&mut state, &mut (), &[1, 2, 3]);
        assert_eq!(state.pen, crate::vterm::pen::VTermPen::default());
    }
}

#[cfg(test)]
mod csi_repeat_tests {
    use super::*;
    #[derive(Default)]
    struct Capture(usize);
    impl VTermStateCallbacks for Capture {
        fn put_glyph(
            &mut self,
            _: &crate::vterm_defs::VTermGlyphInfo,
            _: crate::vterm_defs::VTermPos,
        ) -> bool { self.0 += 1; true }
    }
    #[test]
    fn repeat_replays_last_grapheme_count() {
        let mut state = VTermState::new(1, 10);
        state.grapheme_buf[0] = b'x';
        state.grapheme_len = 1;
        state.combine_width = 1;
        let mut capture = Capture::default();
        csi_repeat(&mut state, &mut capture, 3);
        assert_eq!(capture.0, 3);
        assert_eq!(state.pos.col, 3);
    }
}

#[cfg(test)]
mod csi_device_status_tests {
    use super::*;
    #[test]
    fn device_status_reports_ok_and_cursor_position() {
        let mut state = VTermState::new(10, 10);
        state.pos = crate::vterm_defs::VTermPos { row: 2, col: 3 };
        assert_eq!(csi_device_status(&state, 5, false, false), b"\x1b[0n");
        assert_eq!(csi_device_status(&state, 6, true, false), b"\x1b[?3;4R");
    }
}

#[cfg(test)]
mod csi_tab_tests {
    use super::*;
    #[test]
    fn csi_tab_moves_requested_stop_count() {
        let mut state = VTermState::new(1, 20);
        for col in [4, 8, 12] { state.set_col_tabstop(col); }
        csi_tab(&mut state, 2, true);
        assert_eq!(state.pos.col, 8);
        csi_tab(&mut state, 1, false);
        assert_eq!(state.pos.col, 4);
    }
}

#[cfg(test)]
mod csi_erase_line_tests {
    use super::*;
    struct Capture(i32, i32);
    impl VTermStateCallbacks for Capture {
        fn erase(&mut self, rect: crate::vterm_defs::VTermRect, _: bool) -> bool {
            self.0 = rect.start_col; self.1 = rect.end_col; true
        }
    }
    #[test]
    fn erase_line_selects_requested_range() {
        let mut state = VTermState::new(1, 10);
        state.pos.col = 4;
        let mut capture = Capture(0, 0);
        assert!(csi_erase_line(&mut state, &mut capture, 1, false));
        assert_eq!((capture.0, capture.1), (0, 5));
    }
}

#[cfg(test)]
mod csi_tab_clear_tests {
    use super::*;
    #[test]
    fn tab_clear_removes_current_or_all_stops() {
        let mut state = VTermState::new(1, 10);
        state.set_col_tabstop(3);
        state.pos.col = 3;
        assert!(csi_tab_clear(&mut state, 0));
        assert!(!state.is_col_tabstop(3));
        state.set_col_tabstop(4);
        assert!(csi_tab_clear(&mut state, 3));
        assert!(state.tabstops.iter().all(|&byte| byte == 0));
    }
}

#[cfg(test)]
mod csi_erase_chars_tests {
    use super::*;
    struct Capture(crate::vterm_defs::VTermRect);
    impl VTermStateCallbacks for Capture {
        fn erase(&mut self, rect: crate::vterm_defs::VTermRect, _: bool) -> bool {
            self.0 = rect; true
        }
    }
    #[test]
    fn erase_chars_clamps_to_row_width() {
        let mut state = VTermState::new(1, 5);
        state.pos.col = 3;
        let mut capture = Capture(Default::default());
        csi_erase_chars(&mut state, &mut capture, 9);
        assert_eq!((capture.0.start_col, capture.0.end_col), (3, 5));
    }
}

#[cfg(test)]
mod csi_scroll_region_tests {
    use super::*;
    struct Capture(i32);
    impl VTermStateCallbacks for Capture {
        fn scroll_rect(&mut self, _: crate::vterm_defs::VTermRect, down: i32, _: i32) -> bool {
            self.0 = down; true
        }
    }
    #[test]
    fn region_scroll_forwards_signed_count() {
        let mut state = VTermState::new(3, 3);
        state.reset_scrollregions();
        let mut capture = Capture(0);
        csi_scroll_region(&mut state, &mut capture, -2);
        assert_eq!(capture.0, -2);
    }
}

#[cfg(test)]
mod csi_delete_lines_tests {
    use super::*;
    struct Capture(i32);
    impl VTermStateCallbacks for Capture {
        fn scroll_rect(&mut self, _: crate::vterm_defs::VTermRect, down: i32, _: i32) -> bool {
            self.0 = down; true
        }
    }
    #[test]
    fn delete_lines_scrolls_region_up() {
        let mut state = VTermState::new(3, 3);
        state.reset_scrollregions();
        let mut capture = Capture(0);
        csi_delete_lines(&mut state, &mut capture, 2);
        assert_eq!(capture.0, 2);
    }
}

#[cfg(test)]
mod csi_insert_lines_tests {
    use super::*;
    struct Capture(i32);
    impl VTermStateCallbacks for Capture {
        fn scroll_rect(&mut self, _: crate::vterm_defs::VTermRect, down: i32, _: i32) -> bool {
            self.0 = down; true
        }
    }
    #[test]
    fn insert_lines_scrolls_region_down() {
        let mut state = VTermState::new(3, 3);
        state.reset_scrollregions();
        let mut capture = Capture(0);
        csi_insert_lines(&mut state, &mut capture, 2);
        assert_eq!(capture.0, -2);
    }
}

#[cfg(test)]
mod csi_delete_chars_tests {
    use super::*;
    struct Capture(i32);
    impl VTermStateCallbacks for Capture {
        fn scroll_rect(&mut self, _: crate::vterm_defs::VTermRect, _: i32, rightward: i32) -> bool {
            self.0 = rightward; true
        }
    }
    #[test]
    fn delete_chars_scrolls_current_line_left() {
        let mut state = VTermState::new(2, 10);
        state.reset_scrollregions();
        let mut capture = Capture(0);
        csi_delete_chars(&mut state, &mut capture, 3);
        assert_eq!(capture.0, 3);
    }
}

#[cfg(test)]
mod csi_insert_chars_tests {
    use super::*;
    #[derive(Default)]
    struct Capture(i32);
    impl VTermStateCallbacks for Capture {
        fn scroll_rect(&mut self, _: crate::vterm_defs::VTermRect, _: i32, rightward: i32) -> bool {
            self.0 = rightward; true
        }
    }
    #[test]
    fn insert_chars_scrolls_current_line_right() {
        let mut state = VTermState::new(2, 10);
        state.reset_scrollregions();
        let mut capture = Capture::default();
        csi_insert_chars(&mut state, &mut capture, 2);
        assert_eq!(capture.0, -2);
    }
}

#[cfg(test)]
mod control_dispatch_tests {
    use super::*;
    #[test]
    fn control_dispatch_routes_standard_controls_and_fallback() {
        let mut state = VTermState::new(2, 10);
        state.pos.col = 3;
        assert_eq!(on_control(&mut state, &mut (), &mut (), 0x08), 1);
        assert_eq!(state.pos.col, 2);
        assert_eq!(on_control(&mut state, &mut (), &mut (), 0x01), 0);
    }
}

#[cfg(test)]
mod control_backspace_tests {
    use super::*;
    #[test]
    fn backspace_moves_left_without_underflow() {
        let mut state = VTermState::new(1, 10);
        state.pos.col = 2;
        control_backspace(&mut state);
        assert_eq!(state.pos.col, 1);
        state.pos.col = 0;
        control_backspace(&mut state);
        assert_eq!(state.pos.col, 0);
    }
    #[test]
    fn horizontal_tab_moves_to_next_stop() {
        let mut state = VTermState::new(1, 10);
        state.set_col_tabstop(4);
        control_horizontal_tab(&mut state);
        assert_eq!(state.pos.col, 4);
    }
    #[test]
    fn control_linefeed_honors_newline_mode() {
        let mut state = VTermState::new(2, 10);
        state.pos.col = 4;
        state.mode.newline = true;
        control_linefeed(&mut state, &mut ());
        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 1, col: 0 });
    }
    #[test]
    fn carriage_return_moves_to_column_zero() {
        let mut state = VTermState::new(1, 10);
        state.pos.col = 7;
        control_carriage_return(&mut state);
        assert_eq!(state.pos.col, 0);
    }
    #[test]
    fn control_locking_shift_selects_g0_or_g1() {
        let mut state = VTermState::new(1, 1);
        assert!(control_locking_shift(&mut state, 0x0E));
        assert_eq!(state.gl_set, 1);
        assert!(control_locking_shift(&mut state, 0x0F));
        assert_eq!(state.gl_set, 0);
    }
    #[test]
    fn control_single_shift_selects_g2_or_g3() {
        let mut state = VTermState::new(1, 1);
        assert!(control_single_shift(&mut state, 0x8E));
        assert_eq!(state.gsingle_set, 2);
        assert!(control_single_shift(&mut state, 0x8F));
        assert_eq!(state.gsingle_set, 3);
    }
    #[test]
    fn reverse_index_moves_up_or_scrolls_region() {
        let mut state = VTermState::new(3, 3);
        state.reset_scrollregions();
        state.pos.row = 2;
        control_reverse_index(&mut state, &mut ());
        assert_eq!(state.pos.row, 1);
        state.pos.row = 0;
        control_reverse_index(&mut state, &mut ());
        assert_eq!(state.pos.row, 0);
    }
    #[test]
    fn tabstop_control_sets_current_column() {
        let mut state = VTermState::new(1, 10);
        state.pos.col = 3;
        control_tabstop(&mut state);
        assert!(state.is_col_tabstop(3));
    }
    #[test]
    fn next_line_control_moves_down_and_returns() {
        let mut state = VTermState::new(2, 10);
        state.pos.col = 4;
        control_next_line(&mut state, &mut ());
        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 1, col: 0 });
    }
}

pub fn on_resize<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    rows: i32,
    cols: i32,
) -> i32 {
    let old_position = state.pos;
    state.resize_tabstops(cols);
    state.resize_lineinfos(rows);
    state.rows = rows;
    state.cols = cols;
    state.adjust_resize_phantom(cols);
    state.clamp_resize_bounds(rows, cols);
    updatecursor(state, callbacks, old_position, true);
    1
}

#[cfg(test)]
mod on_resize_tests {
    use super::*;
    #[test]
    fn on_resize_updates_storage_dimensions_and_cursor() {
        let mut state = VTermState::new(2, 10);
        state.pos = crate::vterm_defs::VTermPos { row: 1, col: 9 };
        let mut callbacks = ();
        assert_eq!(on_resize(&mut state, &mut callbacks, 4, 20), 1);
        assert_eq!((state.rows, state.cols), (4, 20));
        assert_eq!(state.lineinfos[0].len(), 4);
        assert_eq!(state.tabstops.len(), 3);
    }
}

pub fn vterm_state_reset<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    hard: bool,
) {
    state.reset(hard);
    let _ = callbacks.init_pen();
    state.reset_pen(callbacks);
    let _ = settermprop_bool(
        callbacks,
        crate::vterm_defs::VTermProp::CursorVisible,
        1,
    );
    let _ = settermprop_bool(
        callbacks,
        crate::vterm_defs::VTermProp::CursorBlink,
        1,
    );
    let _ = settermprop_int(
        callbacks,
        crate::vterm_defs::VTermProp::CursorShape,
        crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BLOCK,
    );
    if hard {
        erase(
            state,
            callbacks,
            crate::vterm_defs::VTermRect {
                start_row: 0,
                end_row: state.rows,
                start_col: 0,
                end_col: state.cols,
            },
            false,
        );
    }
}

pub fn csi_soft_reset<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
) {
    vterm_state_reset(state, callbacks, false);
}

#[cfg(test)]
mod csi_soft_reset_tests {
    use super::*;

    #[derive(Default)]
    struct Capture {
        initialized: usize,
        erased: usize,
    }

    impl VTermStateCallbacks for Capture {
        fn init_pen(&mut self) -> bool {
            self.initialized += 1;
            true
        }

        fn erase(&mut self, _: crate::vterm_defs::VTermRect, _: bool) -> bool {
            self.erased += 1;
            true
        }
    }

    #[test]
    fn soft_reset_resets_modes_without_moving_or_erasing() {
        let mut state = VTermState::new(4, 8);
        state.initialize_pen_colors();
        state.pos = crate::vterm_defs::VTermPos { row: 2, col: 3 };
        state.mode.insert = true;
        state.mode.autowrap = false;
        state.scrollregion_top = 1;
        state.scrollregion_bottom = 3;
        let mut capture = Capture::default();

        csi_soft_reset(&mut state, &mut capture);

        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 2, col: 3 });
        assert!(!state.mode.insert);
        assert!(state.mode.autowrap);
        assert_eq!(state.scrollregion_top, 0);
        assert_eq!(state.scrollregion_bottom, -1);
        assert_eq!(capture.initialized, 1);
        assert_eq!(capture.erased, 0);
    }
}

#[cfg(test)]
mod state_reset_callback_tests {
    use super::*;
    #[test]
    fn state_reset_initializes_props_and_hard_erases() {
        #[derive(Default)]
        struct Capture { props: usize, erased: usize }
        impl VTermStateCallbacks for Capture {
            fn set_term_prop(
                &mut self,
                _: crate::vterm_defs::VTermProp,
                _: &crate::vterm_defs::VTermValue<'_>,
            ) -> bool { self.props += 1; true }
            fn erase(&mut self, _: crate::vterm_defs::VTermRect, _: bool) -> bool {
                self.erased += 1; true
            }
        }
        let mut state = VTermState::new(2, 2);
        state.initialize_pen_colors();
        let mut capture = Capture::default();
        vterm_state_reset(&mut state, &mut capture, true);
        assert_eq!(capture.props, 3);
        assert_eq!(capture.erased, 1);
    }
}

pub fn escape_ris<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    byte: u8,
) -> usize {
    if byte != b'c' {
        return 0;
    }
    let old = state.pos;
    vterm_state_reset(state, callbacks, true);
    let _ = callbacks.move_cursor(state.pos, old, state.mode.cursor_visible);
    1
}

#[cfg(test)]
mod ris_tests {
    use super::*;
    #[test]
    fn ris_hard_resets_terminal_state() {
        let mut state = VTermState::new(2, 2);
        state.initialize_pen_colors();
        state.pos.col = 1;
        let mut callbacks = ();
        assert_eq!(escape_ris(&mut state, &mut callbacks, b'c'), 1);
        assert_eq!(state.pos, Default::default());
        assert_eq!(escape_ris(&mut state, &mut callbacks, b'x'), 0);
    }
}

pub fn on_escape<C: VTermStateCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    bytes: &[u8],
) -> usize {
    let handled = state.escape_control_width(bytes);
    if handled != 0 { return handled; }
    let handled = state.escape_line_attribute(bytes);
    if handled != 0 { return handled; }
    if bytes == b"#8" {
        decaln(state, callbacks);
        return 2;
    }
    let handled = state.escape_designate_charset(bytes);
    if handled != 0 { return handled; }
    if bytes.len() != 1 { return 0; }
    let byte = bytes[0];
    for handled in [
        state.escape_saved_cursor(byte),
        state.escape_keypad_mode(byte),
        state.escape_locking_shift(byte),
    ] {
        if handled != 0 { return handled; }
    }
    if byte == b'<' { return 1; }
    escape_ris(state, callbacks, byte)
}

#[cfg(test)]
mod escape_dispatch_tests {
    use super::*;
    #[test]
    fn escape_dispatch_routes_known_sequences() {
        let mut state = VTermState::new(1, 2);
        state.initialize_pen_colors();
        let mut callbacks = ();
        assert_eq!(on_escape(&mut state, &mut callbacks, b" G"), 2);
        assert!(state.ctrl8bit);
        assert_eq!(on_escape(&mut state, &mut callbacks, b"="), 1);
        assert!(state.mode.keypad);
        assert_eq!(on_escape(&mut state, &mut callbacks, b"?"), 0);
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

#[must_use]
pub fn request_dec_mode(state: &VTermState, number: i32, ctrl8bit: bool) -> Vec<u8> {
    let reply = state
        .dec_mode_value(number)
        .map_or(0, |enabled| if enabled { 1 } else { 2 });
    let prefix: &[u8] = if ctrl8bit {
        &[crate::vterm_defs::C1_CSI]
    } else {
        b"\x1b["
    };
    [prefix, format!("?{number};{reply}$y").as_bytes()].concat()
}

#[must_use]
pub fn primary_device_attributes(ctrl8bit: bool) -> Vec<u8> {
    let prefix: &[u8] = if ctrl8bit {
        &[crate::vterm_defs::C1_CSI]
    } else {
        b"\x1b["
    };
    [prefix, b"?", VTERM_PRIMARY_DEVICE_ATTR, b"c"].concat()
}

#[must_use]
pub fn secondary_device_attributes(ctrl8bit: bool) -> Vec<u8> {
    let prefix: &[u8] = if ctrl8bit {
        &[crate::vterm_defs::C1_CSI]
    } else {
        b"\x1b["
    };
    [prefix, b">0;100;0c"].concat()
}

#[cfg(test)]
mod secondary_device_response_tests {
    use super::*;
    #[test]
    fn secondary_device_response_matches_state_c() {
        assert_eq!(secondary_device_attributes(false), b"\x1b[>0;100;0c");
    }
}

#[cfg(test)]
mod primary_device_response_tests {
    use super::*;
    #[test]
    fn primary_device_response_uses_configurable_attribute_string() {
        assert_eq!(primary_device_attributes(false), b"\x1b[?61;22;52c");
    }
}

#[cfg(test)]
mod dec_mode_request_tests {
    use super::*;
    #[test]
    fn dec_mode_request_encodes_set_reset_and_unknown() {
        let mut state = VTermState::new(1, 1);
        state.mode.autowrap = true;
        assert_eq!(request_dec_mode(&state, 7, false), b"\x1b[?7;1$y");
        state.mode.autowrap = false;
        assert_eq!(request_dec_mode(&state, 7, false), b"\x1b[?7;2$y");
        assert_eq!(request_dec_mode(&state, 999, false), b"\x1b[?999;0$y");
    }
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

/// Accumulates the three-byte DECRQSS request key.
pub fn accumulate_decrqss(
    state: &mut VTermState,
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
) -> bool {
    if fragment.initial {
        state.decrqss = [0; 4];
    }
    let used = state.decrqss.iter().position(|&b| b == 0).unwrap_or(3);
    let take = (3 - used).min(fragment.bytes.len());
    state.decrqss[used..used + take].copy_from_slice(&fragment.bytes[..take]);
    fragment.final_fragment
}

#[must_use]
pub fn request_sgr_status(state: &VTermState, ctrl8bit: bool) -> Vec<u8> {
    let pen_state = crate::vterm::pen::VTermPenState {
        pen: state.pen,
        ..Default::default()
    };
    let args = crate::vterm::pen::vterm_state_getpen(&pen_state);
    let mut body = b"1$r".to_vec();
    for (index, arg) in args.iter().enumerate() {
        body.extend_from_slice(crate::vterm::parser::csi_arg(*arg).to_string().as_bytes());
        if index + 1 < args.len() {
            body.push(if crate::vterm::parser::csi_arg_has_more(*arg) {
                b':'
            } else {
                b';'
            });
        }
    }
    body.push(b'm');
    let start: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_DCS] } else { b"\x1bP" };
    let end: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_ST] } else { b"\x1b\\" };
    [start, &body, end].concat()
}

#[must_use]
pub fn dispatch_status_request(state: &VTermState, ctrl8bit: bool) -> Vec<u8> {
    match &state.decrqss[..3] {
        [b'm', 0, 0] => request_sgr_status(state, ctrl8bit),
        [b'r', 0, 0] => request_vertical_margins(state, ctrl8bit),
        [b's', 0, 0] => request_horizontal_margins(state, ctrl8bit),
        [b' ', b'q', 0] => request_cursor_style(state, ctrl8bit),
        [b'"', b'q', 0] => request_protected_cell(state, ctrl8bit),
        _ => {
            let start: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_DCS] } else { b"\x1bP" };
            let end: &[u8] = if ctrl8bit { &[crate::vterm_defs::C1_ST] } else { b"\x1b\\" };
            [start, b"0$r", end].concat()
        }
    }
}

pub fn on_dcs_status(
    state: &mut VTermState,
    command: &[u8],
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
    ctrl8bit: bool,
) -> Option<Vec<u8>> {
    if command != b"$q" {
        return None;
    }
    if accumulate_decrqss(state, fragment) {
        Some(dispatch_status_request(state, ctrl8bit))
    } else {
        Some(Vec::new())
    }
}

pub fn on_dcs<F: VTermStateFallbacks>(
    state: &mut VTermState,
    fallbacks: &mut F,
    command: &[u8],
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
) -> (i32, Vec<u8>) {
    if let Some(output) = on_dcs_status(state, command, fragment, state.ctrl8bit) {
        (1, output)
    } else {
        (on_dcs_fallback(fallbacks, command, fragment), Vec::new())
    }
}

#[cfg(test)]
mod on_dcs_tests {
    use super::*;

    #[derive(Default)]
    struct Fallback(usize);

    impl VTermStateFallbacks for Fallback {
        fn dcs(
            &mut self,
            _: &[u8],
            _: crate::vterm_defs::VTermStringFragment<'_>,
        ) -> bool {
            self.0 += 1;
            true
        }
    }

    #[test]
    fn dcs_dispatches_status_requests_and_fallbacks() {
        let fragment = crate::vterm_defs::VTermStringFragment {
            bytes: b"r",
            initial: true,
            final_fragment: true,
            terminator: crate::vterm_defs::VTermTerminator::St,
        };
        let mut state = VTermState::new(4, 8);
        state.reset_scrollregions();
        let mut fallback = Fallback::default();
        assert_eq!(
            on_dcs(&mut state, &mut fallback, b"$q", fragment),
            (1, b"\x1bP1$r1;4r\x1b\\".to_vec())
        );
        assert_eq!(fallback.0, 0);
        assert_eq!(
            on_dcs(&mut state, &mut fallback, b"x", fragment),
            (1, Vec::new())
        );
        assert_eq!(fallback.0, 1);
    }
}

#[cfg(test)]
mod dcs_status_tests {
    use super::*;
    #[test]
    fn dcs_status_handles_only_decrqss_command() {
        let mut state = VTermState::new(2, 80);
        state.reset_scrollregions();
        let fragment = crate::vterm_defs::VTermStringFragment {
            bytes: b"r", initial: true, final_fragment: true,
            terminator: crate::vterm_defs::VTermTerminator::St,
        };
        assert!(on_dcs_status(&mut state, b"x", fragment, false).is_none());
        assert_eq!(
            on_dcs_status(&mut state, b"$q", fragment, false).unwrap(),
            b"\x1bP1$r1;2r\x1b\\"
        );
    }
}

#[cfg(test)]
mod status_dispatch_tests {
    use super::*;
    #[test]
    fn status_dispatch_routes_known_and_unknown_keys() {
        let mut state = VTermState::new(2, 80);
        state.decrqss[..2].copy_from_slice(b" q");
        assert!(dispatch_status_request(&state, false).ends_with(b" q\x1b\\"));
        state.decrqss = *b"xyz\0";
        assert_eq!(dispatch_status_request(&state, false), b"\x1bP0$r\x1b\\");
    }
}

#[cfg(test)]
mod sgr_status_tests {
    use super::*;
    #[test]
    fn sgr_status_serializes_pen_arguments() {
        let mut state = VTermState::new(1, 1);
        state.pen.bold = true;
        state.pen.underline = crate::vterm_defs::VTERM_UNDERLINE_CURLY;
        state.pen.fg.color_type = crate::vterm_defs::VTERM_COLOR_DEFAULT_FG;
        state.pen.bg.color_type = crate::vterm_defs::VTERM_COLOR_DEFAULT_BG;
        assert_eq!(request_sgr_status(&state, false), b"\x1bP1$r1;4:3m\x1b\\");
    }
}

#[cfg(test)]
mod decrqss_accumulate_tests {
    use super::*;
    #[test]
    fn decrqss_accumulates_up_to_three_bytes_across_fragments() {
        let mut state = VTermState::new(1, 1);
        assert!(!accumulate_decrqss(
            &mut state,
            crate::vterm_defs::VTermStringFragment {
                bytes: b" ", initial: true, final_fragment: false,
                terminator: crate::vterm_defs::VTermTerminator::St,
            },
        ));
        assert!(accumulate_decrqss(
            &mut state,
            crate::vterm_defs::VTermStringFragment {
                bytes: b"qextra", initial: false, final_fragment: true,
                terminator: crate::vterm_defs::VTermTerminator::St,
            },
        ));
        assert_eq!(&state.decrqss[..3], b" qe");
    }
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

pub fn osc_selection<C: VTermSelectionCallbacks>(
    state: &mut VTermState,
    callbacks: &mut C,
    fragment: crate::vterm_defs::VTermStringFragment<'_>,
) {
    if fragment.initial {
        state.selection_temp.mask = 0;
        state.selection_temp.state = VTermSelectionState::Initial;
    }
    let consumed = parse_selection_mask(&mut state.selection_temp, fragment.bytes);
    let payload = &fragment.bytes[consumed..];
    if payload.is_empty() {
        if fragment.final_fragment {
            let _ = callbacks.set(
                u32::from(state.selection_temp.mask),
                crate::vterm_defs::VTermStringFragment {
                    bytes: b"",
                    initial: state.selection_temp.state != VTermSelectionState::Set,
                    final_fragment: true,
                    terminator: fragment.terminator,
                },
            );
        }
        return;
    }
    if state.selection_temp.state == VTermSelectionState::Selected {
        if payload[0] == b'?' {
            state.selection_temp.state = VTermSelectionState::Query;
            let _ = callbacks.query(u32::from(state.selection_temp.mask));
            return;
        }
        state.selection_temp.state = VTermSelectionState::SetInitial;
        state.selection_temp.recv_partial = 0;
    }
    if state.selection_temp.state == VTermSelectionState::Invalid {
        return;
    }
    let capacity = state.selection_buflen.max(3);
    let Some((decoded, _)) =
        decode_selection_base64(&mut state.selection_temp, payload, capacity)
    else {
        let _ = callbacks.set(
            u32::from(state.selection_temp.mask),
            crate::vterm_defs::VTermStringFragment {
                bytes: b"",
                initial: true,
                final_fragment: true,
                terminator: fragment.terminator,
            },
        );
        return;
    };
    if !decoded.is_empty() {
        let initial = state.selection_temp.state == VTermSelectionState::SetInitial;
        let _ = callbacks.set(
            u32::from(state.selection_temp.mask),
            crate::vterm_defs::VTermStringFragment {
                bytes: &decoded,
                initial,
                final_fragment: fragment.final_fragment,
                terminator: fragment.terminator,
            },
        );
        state.selection_temp.state = VTermSelectionState::Set;
    }
}

#[cfg(test)]
mod osc_selection_tests {
    use super::*;
    #[derive(Default)]
    struct Capture {
        sets: Vec<Vec<u8>>,
        queries: usize,
    }
    impl VTermSelectionCallbacks for Capture {
        fn set(
            &mut self,
            _: crate::vterm_defs::VTermSelectionMask,
            fragment: crate::vterm_defs::VTermStringFragment<'_>,
        ) -> bool {
            self.sets.push(fragment.bytes.to_vec());
            true
        }
        fn query(&mut self, _: crate::vterm_defs::VTermSelectionMask) -> bool {
            self.queries += 1;
            true
        }
    }
    #[test]
    fn osc_selection_handles_query_and_base64_set() {
        let mut state = VTermState::new(1, 1);
        state.selection_buflen = 64;
        let mut capture = Capture::default();
        osc_selection(
            &mut state,
            &mut capture,
            crate::vterm_defs::VTermStringFragment {
                bytes: b"c;?", initial: true, final_fragment: true,
                terminator: crate::vterm_defs::VTermTerminator::Bel,
            },
        );
        assert_eq!(capture.queries, 1);
        osc_selection(
            &mut state,
            &mut capture,
            crate::vterm_defs::VTermStringFragment {
                bytes: b"c;SGk=", initial: true, final_fragment: true,
                terminator: crate::vterm_defs::VTermTerminator::Bel,
            },
        );
        assert_eq!(capture.sets.last().unwrap(), b"Hi");
    }
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
            default_fg: crate::vterm_defs::VTermColor::default(),
            default_bg: crate::vterm_defs::VTermColor::default(),
            colors: [crate::vterm_defs::VTermColor::default(); 16],
            bold_is_highbright: false,
            ctrl8bit: false,
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
            encodings: std::array::from_fn(|_| {
                crate::vterm::encoding::VTermEncodingInstance::new(
                    crate::vterm::encoding::VTermEncoding::UsAscii,
                )
            }),
            encoding_utf8: crate::vterm::encoding::VTermEncodingInstance::new(
                crate::vterm::encoding::VTermEncoding::Utf8,
            ),
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

    pub fn reset_scrollregions(&mut self) {
        self.scrollregion_top = 0;
        self.scrollregion_bottom = -1;
        self.scrollregion_left = 0;
        self.scrollregion_right = -1;
    }

    pub fn reset_modes(&mut self) {
        self.mode.keypad = false;
        self.mode.cursor = false;
        self.mode.autowrap = true;
        self.mode.insert = false;
        self.mode.newline = false;
        self.mode.alt_screen = false;
        self.mode.origin = false;
        self.mode.leftrightmargin = false;
        self.mode.bracketpaste = false;
        self.mode.report_focus = false;
        self.mouse_flags = 0;
    }

    pub fn reset_tabstops(&mut self) {
        self.tabstops.fill(0);
        for col in (0..self.cols).step_by(8) {
            self.set_col_tabstop(col);
        }
    }

    pub fn reset_lineinfos(&mut self) {
        self.lineinfos[self.active_lineinfo].fill(Default::default());
    }

    pub fn hard_reset_cursor(&mut self) {
        self.pos = Default::default();
        self.at_phantom = false;
    }

    pub fn reset(&mut self, hard: bool) {
        self.reset_scrollregions();
        self.reset_modes();
        self.reset_tabstops();
        self.active_lineinfo = 0;
        self.reset_lineinfos();
        self.encodings = std::array::from_fn(|_| {
            crate::vterm::encoding::VTermEncodingInstance::new(
                crate::vterm::encoding::VTermEncoding::UsAscii,
            )
        });
        self.encoding_utf8.reset();
        self.gl_set = 0;
        self.gr_set = 1;
        self.gsingle_set = 0;
        self.protected_cell = false;
        if hard {
            self.hard_reset_cursor();
        }
    }

    pub fn set_mode(&mut self, number: i32, value: i32) -> bool {
        match number {
            4 => self.mode.insert = value != 0,
            20 => self.mode.newline = value != 0,
            _ => return false,
        }
        true
    }

    pub fn escape_control_width(&mut self, bytes: &[u8]) -> usize {
        match bytes {
            [b' ', b'F'] => self.ctrl8bit = false,
            [b' ', b'G'] => self.ctrl8bit = true,
            _ => return 0,
        }
        2
    }

    pub fn escape_line_attribute(&mut self, bytes: &[u8]) -> usize {
        let (dwl, dhl) = match bytes {
            [b'#', b'3'] => (DWL_ON, DHL_TOP),
            [b'#', b'4'] => (DWL_ON, DHL_BOTTOM),
            [b'#', b'5'] => (DWL_OFF, DHL_OFF),
            [b'#', b'6'] => (DWL_ON, DHL_OFF),
            _ => return 0,
        };
        if !self.mode.leftrightmargin {
            self.set_lineinfo(self.pos.row, NO_FORCE, dwl, dhl, |_, _, _| true);
        }
        2
    }

    pub fn escape_designate_charset(&mut self, bytes: &[u8]) -> usize {
        let [selector @ (b'(' | b')' | b'*' | b'+'), designation] = bytes else {
            return 0;
        };
        let set = usize::from(*selector - b'(');
        if let Some(encoding) = crate::vterm::encoding::vterm_lookup_encoding(
            crate::vterm::encoding::VTermEncodingType::Single94,
            *designation,
        ) {
            self.encodings[set] =
                crate::vterm::encoding::VTermEncodingInstance::new(encoding);
        }
        2
    }

    pub fn escape_keypad_mode(&mut self, byte: u8) -> usize {
        match byte {
            b'=' => self.mode.keypad = true,
            b'>' => self.mode.keypad = false,
            _ => return 0,
        }
        1
    }

    pub fn escape_locking_shift(&mut self, byte: u8) -> usize {
        match byte {
            b'n' => self.gl_set = 2,
            b'o' => self.gl_set = 3,
            b'~' => self.gr_set = 1,
            b'}' => self.gr_set = 2,
            b'|' => self.gr_set = 3,
            _ => return 0,
        }
        1
    }

    pub fn escape_saved_cursor(&mut self, byte: u8) -> usize {
        match byte {
            b'7' => savecursor(self, 1),
            b'8' => savecursor(self, 0),
            _ => return 0,
        }
        1
    }

    pub fn set_dec_basic_mode(&mut self, number: i32, value: i32) -> bool {
        let enabled = value != 0;
        match number {
            1 => self.mode.cursor = enabled,
            7 => self.mode.autowrap = enabled,
            2004 => self.mode.bracketpaste = enabled,
            _ => return false,
        }
        true
    }

    pub fn set_leftrightmargin_mode(&mut self, value: i32) {
        self.mode.leftrightmargin = value != 0;
        if self.mode.leftrightmargin {
            self.lineinfos[self.active_lineinfo].fill(Default::default());
        }
    }

    pub fn set_mouse_tracking_mode(&mut self, number: i32, value: i32) -> bool {
        let mode = if value == 0 {
            crate::vterm_defs::VTERM_PROP_MOUSE_NONE
        } else {
            match number {
                1000 => crate::vterm_defs::VTERM_PROP_MOUSE_CLICK,
                1002 => crate::vterm_defs::VTERM_PROP_MOUSE_DRAG,
                1003 => crate::vterm_defs::VTERM_PROP_MOUSE_MOVE,
                _ => return false,
            }
        };
        self.set_termprop(
            crate::vterm_defs::VTermProp::Mouse,
            &crate::vterm_defs::VTermValue::Number(mode),
            true,
        ) != 0
    }

    pub fn set_mouse_protocol_mode(&mut self, number: i32, value: i32) -> bool {
        self.mouse_protocol = if value == 0 {
            crate::vterm::mouse::VTermMouseProtocol::X10
        } else {
            match number {
                1005 => crate::vterm::mouse::VTermMouseProtocol::Utf8,
                1006 => crate::vterm::mouse::VTermMouseProtocol::Sgr,
                1015 => crate::vterm::mouse::VTermMouseProtocol::Rxvt,
                _ => return false,
            }
        };
        true
    }

    pub fn csi_move_cursor(&mut self, command: u8, count: i32) -> bool {
        let count = count.max(1);
        match command {
            b'A' | b'k' => self.pos.row -= count,
            b'B' | b'e' => self.pos.row += count,
            b'C' | b'a' => self.pos.col += count,
            b'D' | b'j' => self.pos.col -= count,
            b'E' => { self.pos.col = 0; self.pos.row += count; }
            b'F' => { self.pos.col = 0; self.pos.row -= count; }
            _ => return false,
        }
        self.at_phantom = false;
        true
    }

    pub fn csi_position(&mut self, command: u8, row: i32, col: i32) -> bool {
        match command {
            b'G' | b'`' => self.pos.col = col.max(1) - 1,
            b'd' => {
                self.pos.row = row.max(1) - 1;
                if self.mode.origin { self.pos.row += self.scrollregion_top; }
            }
            b'H' | b'f' => {
                self.pos.row = row.max(1) - 1;
                self.pos.col = col.max(1) - 1;
                if self.mode.origin {
                    self.pos.row += self.scrollregion_top;
                    self.pos.col += self.scrollregion_left();
                }
            }
            _ => return false,
        }
        self.at_phantom = false;
        true
    }

    pub fn set_vertical_margins(&mut self, top: i32, bottom: Option<i32>) {
        self.scrollregion_top = (top.max(1) - 1).clamp(0, self.rows);
        self.scrollregion_bottom = bottom.unwrap_or(-1).clamp(-1, self.rows);
        if self.scrollregion_top == 0 && self.scrollregion_bottom == self.rows {
            self.scrollregion_bottom = -1;
        }
        if self.scrollregion_bottom() <= self.scrollregion_top {
            self.scrollregion_top = 0;
            self.scrollregion_bottom = -1;
        }
        self.pos = if self.mode.origin {
            crate::vterm_defs::VTermPos {
                row: self.scrollregion_top,
                col: self.scrollregion_left(),
            }
        } else {
            Default::default()
        };
    }

    pub fn set_horizontal_margins(&mut self, left: i32, right: Option<i32>) {
        self.scrollregion_left = (left.max(1) - 1).clamp(0, self.cols);
        self.scrollregion_right = right.unwrap_or(-1).clamp(-1, self.cols);
        if self.scrollregion_left == 0 && self.scrollregion_right == self.cols {
            self.scrollregion_right = -1;
        }
        if self.scrollregion_right > -1 && self.scrollregion_right <= self.scrollregion_left {
            self.scrollregion_left = 0;
            self.scrollregion_right = -1;
        }
        self.pos = if self.mode.origin {
            crate::vterm_defs::VTermPos {
                row: self.scrollregion_top,
                col: self.scrollregion_left(),
            }
        } else {
            Default::default()
        };
    }

    pub fn set_cursor_style(&mut self, value: i32) {
        match value {
            0 | 1 => {
                self.mode.cursor_blink = true;
                self.mode.cursor_shape =
                    crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BLOCK as u8;
            }
            2 => {
                self.mode.cursor_blink = false;
                self.mode.cursor_shape =
                    crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BLOCK as u8;
            }
            3 | 4 => {
                self.mode.cursor_blink = value == 3;
                self.mode.cursor_shape =
                    crate::vterm_defs::VTERM_PROP_CURSORSHAPE_UNDERLINE as u8;
            }
            5 | 6 => {
                self.mode.cursor_blink = value == 5;
                self.mode.cursor_shape =
                    crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BAR_LEFT as u8;
            }
            _ => {}
        }
    }

    pub fn set_character_protection(&mut self, value: i32) {
        match value {
            0 | 2 => self.protected_cell = false,
            1 => self.protected_cell = true,
            _ => {}
        }
    }

    #[must_use]
    pub fn dec_mode_value(&self, number: i32) -> Option<bool> {
        Some(match number {
            1 => self.mode.cursor,
            5 => self.mode.screen,
            6 => self.mode.origin,
            7 => self.mode.autowrap,
            12 => self.mode.cursor_blink,
            25 => self.mode.cursor_visible,
            69 => self.mode.leftrightmargin,
            1000 => self.mouse_flags == crate::vterm::mouse::MOUSE_WANT_CLICK,
            1002 => {
                self.mouse_flags
                    == crate::vterm::mouse::MOUSE_WANT_CLICK
                        | crate::vterm::mouse::MOUSE_WANT_DRAG
            }
            1003 => {
                self.mouse_flags
                    == crate::vterm::mouse::MOUSE_WANT_CLICK
                        | crate::vterm::mouse::MOUSE_WANT_MOVE
            }
            1004 => self.mode.report_focus,
            1005 => self.mouse_protocol == crate::vterm::mouse::VTermMouseProtocol::Utf8,
            1006 => self.mouse_protocol == crate::vterm::mouse::VTermMouseProtocol::Sgr,
            1015 => self.mouse_protocol == crate::vterm::mouse::VTermMouseProtocol::Rxvt,
            1047 => self.mode.alt_screen,
            2004 => self.mode.bracketpaste,
            2026 => self.mode.synchronized_output,
            2031 => self.mode.theme_updates,
            _ => return None,
        })
    }

    pub fn initialize_pen_colors(&mut self) {
        let mut pen_state = crate::vterm::pen::VTermPenState::default();
        crate::vterm::pen::vterm_state_newpen(&mut pen_state);
        self.default_fg = pen_state.default_fg;
        self.default_bg = pen_state.default_bg;
        self.colors = pen_state.palette.colors;
    }

    pub fn set_default_colors(
        &mut self,
        foreground: Option<&crate::vterm_defs::VTermColor>,
        background: Option<&crate::vterm_defs::VTermColor>,
    ) {
        let mut pen_state = crate::vterm::pen::VTermPenState {
            default_fg: self.default_fg,
            default_bg: self.default_bg,
            ..Default::default()
        };
        crate::vterm::pen::vterm_state_set_default_colors(
            &mut pen_state,
            foreground,
            background,
        );
        self.default_fg = pen_state.default_fg;
        self.default_bg = pen_state.default_bg;
    }

    pub fn set_palette_color(
        &mut self,
        index: i32,
        color: &crate::vterm_defs::VTermColor,
    ) {
        if let Ok(index) = usize::try_from(index)
            && let Some(slot) = self.colors.get_mut(index)
        {
            *slot = *color;
        }
    }

    pub fn convert_color_to_rgb(&self, color: &mut crate::vterm_defs::VTermColor) {
        let state = crate::vterm::pen::VTermPenState {
            palette: crate::vterm::pen::VTermPalette {
                colors: self.colors,
            },
            ..Default::default()
        };
        crate::vterm::pen::vterm_state_convert_color_to_rgb(&state, color);
    }

    pub fn set_penattr<C: VTermStateCallbacks>(
        &mut self,
        callbacks: &mut C,
        attr: crate::vterm_defs::VTermAttr,
        value_type: crate::vterm_defs::VTermValueType,
        value: &crate::vterm_defs::VTermValue<'_>,
    ) -> i32 {
        let mut pen_state = crate::vterm::pen::VTermPenState {
            pen: self.pen,
            ..Default::default()
        };
        let mut adapter = StatePenCallback(callbacks);
        let result = crate::vterm::pen::vterm_state_set_penattr(
            &mut pen_state,
            attr,
            value_type,
            Some(value),
            &mut adapter,
        );
        self.pen = pen_state.pen;
        result
    }

    pub fn setpen<C: VTermStateCallbacks>(
        &mut self,
        callbacks: &mut C,
        args: &[crate::vterm::parser::CsiArg],
    ) {
        let mut pen_state = crate::vterm::pen::VTermPenState {
            pen: self.pen,
            default_fg: self.default_fg,
            default_bg: self.default_bg,
            palette: crate::vterm::pen::VTermPalette { colors: self.colors },
            bold_is_highbright: self.bold_is_highbright,
            ..Default::default()
        };
        let mut adapter = StatePenCallback(callbacks);
        crate::vterm::pen::vterm_state_setpen(&mut pen_state, args, &mut adapter);
        self.pen = pen_state.pen;
    }

    pub fn reset_pen<C: VTermStateCallbacks>(&mut self, callbacks: &mut C) {
        let mut pen_state = crate::vterm::pen::VTermPenState {
            pen: self.pen,
            default_fg: self.default_fg,
            default_bg: self.default_bg,
            palette: crate::vterm::pen::VTermPalette {
                colors: self.colors,
            },
            bold_is_highbright: self.bold_is_highbright,
            ..Default::default()
        };
        let mut adapter = StatePenCallback(callbacks);
        crate::vterm::pen::vterm_state_resetpen(&mut pen_state, &mut adapter);
        self.pen = pen_state.pen;
    }

    /// Resizes tabstop storage, preserving old columns and defaulting
    /// new columns every eight cells (`on_resize`).
    pub fn resize_tabstops(&mut self, cols: i32) {
        if cols == self.cols {
            return;
        }
        let mut new_tabstops = vec![0; usize::try_from(cols).unwrap_or(0).div_ceil(8)];
        let common = self.cols.min(cols);
        for col in 0..common {
            if self.is_col_tabstop(col) {
                new_tabstops[(col >> 3) as usize] |= 1 << (col & 7);
            }
        }
        for col in common..cols {
            if col % 8 == 0 {
                new_tabstops[(col >> 3) as usize] |= 1 << (col & 7);
            }
        }
        self.tabstops = new_tabstops;
    }

    pub fn resize_lineinfos(&mut self, rows: i32) {
        let rows = usize::try_from(rows).unwrap_or(0);
        for lineinfo in &mut self.lineinfos {
            lineinfo.resize(rows, Default::default());
        }
    }

    pub fn clamp_resize_bounds(&mut self, rows: i32, cols: i32) {
        if self.scrollregion_bottom > -1 {
            self.scrollregion_bottom = self.scrollregion_bottom.min(rows);
        }
        if self.scrollregion_right > -1 {
            self.scrollregion_right = self.scrollregion_right.min(cols);
        }
        self.pos.row = self.pos.row.clamp(0, rows - 1);
        self.pos.col = self.pos.col.clamp(0, cols - 1);
    }

    pub fn adjust_resize_phantom(&mut self, cols: i32) {
        if self.at_phantom && self.pos.col < cols - 1 {
            self.at_phantom = false;
            self.pos.col += 1;
        }
    }
}

#[cfg(test)]
mod resize_tabstop_tests {
    use super::*;
    #[test]
    fn resize_tabstops_preserves_old_and_initializes_new_columns() {
        let mut state = VTermState::new(1, 10);
        state.set_col_tabstop(3);
        state.resize_tabstops(20);
        assert!(state.is_col_tabstop(3));
        assert!(state.is_col_tabstop(16));
        assert!(!state.is_col_tabstop(15));
        state.cols = 20;
        state.resize_tabstops(5);
        assert!(state.is_col_tabstop(3));
        assert_eq!(state.tabstops.len(), 1);
    }

    #[cfg(test)]
    mod resize_lineinfo_tests {
        use super::*;
        #[test]
        fn resize_lineinfos_preserves_common_rows_and_clears_new_rows() {
            let mut state = VTermState::new(2, 10);
            state.lineinfos[0][1].doublewidth = true;
            state.resize_lineinfos(4);
            assert!(state.lineinfos[0][1].doublewidth);
            assert_eq!(state.lineinfos[0][3], Default::default());
            assert_eq!(state.lineinfos[1].len(), 4);
            state.resize_lineinfos(1);
            assert_eq!(state.lineinfos[0].len(), 1);
        }

        #[cfg(test)]
        mod resize_bound_tests {
            use super::*;
            #[test]
            fn resize_bounds_clamp_regions_and_cursor() {
                let mut state = VTermState::new(24, 80);
                state.scrollregion_bottom = 20;
                state.scrollregion_right = 70;
                state.pos = crate::vterm_defs::VTermPos { row: 30, col: -2 };
                state.clamp_resize_bounds(10, 40);
                assert_eq!((state.scrollregion_bottom, state.scrollregion_right), (10, 40));
                assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 9, col: 0 });
            }

            #[cfg(test)]
            mod resize_phantom_tests {
                use super::*;
                #[test]
                fn resize_phantom_advances_when_new_width_has_room() {
                    let mut state = VTermState::new(1, 10);
                    state.pos.col = 8;
                    state.at_phantom = true;
                    state.adjust_resize_phantom(12);
                    assert_eq!(state.pos.col, 9);
                    assert!(!state.at_phantom);
                    state.at_phantom = true;
                    state.pos.col = 11;
                    state.adjust_resize_phantom(12);
                    assert!(state.at_phantom);
                }
            }
        }
    }
}

#[must_use]
pub fn vterm_state_new(rows: i32, cols: i32) -> VTermState {
    let mut state = VTermState::new(rows, cols);
    state.initialize_pen_colors();
    state
}

pub fn vterm_state_free(state: VTermState) {
    drop(state);
}

pub struct VTermStateOwner {
    pub rows: i32,
    pub cols: i32,
    pub state: Option<VTermState>,
}

pub fn vterm_obtain_state(owner: &mut VTermStateOwner) -> &mut VTermState {
    owner
        .state
        .get_or_insert_with(|| vterm_state_new(owner.rows, owner.cols))
}

#[cfg(test)]
mod obtain_state_tests {
    use super::*;
    #[test]
    fn obtain_state_lazily_constructs_and_reuses_state() {
        let mut owner = VTermStateOwner {
            rows: 2,
            cols: 3,
            state: None,
        };
        vterm_obtain_state(&mut owner).pos.col = 1;
        assert_eq!(vterm_obtain_state(&mut owner).pos.col, 1);
    }
}

#[cfg(test)]
mod state_new_wrapper_tests {
    use super::*;
    #[test]
    fn state_new_wrapper_initializes_pen_colors() {
        let state = vterm_state_new(24, 80);
        assert!(state.default_fg.is_default_fg());
        assert_eq!(state.tabstops.len(), 10);
    }
    #[test]
    fn state_free_consumes_owned_state() {
        vterm_state_free(vterm_state_new(1, 1));
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
        assert!(state.encodings.iter().all(|encoding| {
            encoding.encoding() == crate::vterm::encoding::VTermEncoding::UsAscii
        }));
        assert_eq!(
            state.encoding_utf8.encoding(),
            crate::vterm::encoding::VTermEncoding::Utf8
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
    #[test]
    fn reset_scrollregions_restores_unbounded_defaults() {
        let mut state = VTermState::new(2, 2);
        state.scrollregion_top = 1;
        state.scrollregion_bottom = 1;
        state.scrollregion_left = 1;
        state.scrollregion_right = 1;
        state.reset_scrollregions();
        assert_eq!(
            (
                state.scrollregion_top,
                state.scrollregion_bottom,
                state.scrollregion_left,
                state.scrollregion_right,
            ),
            (0, -1, 0, -1)
        );
    }
    #[test]
    fn reset_modes_restores_terminal_defaults() {
        let mut state = VTermState::new(1, 1);
        state.mode = VTermStateMode {
            keypad: true,
            report_focus: true,
            ..Default::default()
        };
        state.mouse_flags = 7;
        state.reset_modes();
        assert!(state.mode.autowrap);
        assert!(!state.mode.keypad);
        assert!(!state.mode.report_focus);
        assert_eq!(state.mouse_flags, 0);
    }
    #[test]
    fn set_mode_handles_insert_and_newline_modes() {
        let mut state = VTermState::new(1, 1);
        assert!(state.set_mode(4, 1));
        assert!(state.mode.insert);
        assert!(state.set_mode(20, 1));
        assert!(state.mode.newline);
        assert!(!state.set_mode(99, 1));
    }
    #[test]
    fn escape_control_width_handles_seven_and_eight_bit_modes() {
        let mut state = VTermState::new(1, 1);
        assert_eq!(state.escape_control_width(b" G"), 2);
        assert!(state.ctrl8bit);
        assert_eq!(state.escape_control_width(b" F"), 2);
        assert!(!state.ctrl8bit);
        assert_eq!(state.escape_control_width(b" X"), 0);
    }
    #[test]
    fn escape_line_attribute_sets_doublewidth_and_height() {
        let mut state = VTermState::new(1, 80);
        assert_eq!(state.escape_line_attribute(b"#3"), 2);
        assert!(state.lineinfos[0][0].doublewidth);
        assert_eq!(state.lineinfos[0][0].doubleheight, 1);
        assert_eq!(state.escape_line_attribute(b"#5"), 2);
        assert_eq!(state.lineinfos[0][0], Default::default());
    }
    #[test]
    fn escape_designate_charset_updates_selected_slot() {
        let mut state = VTermState::new(1, 1);
        assert_eq!(state.escape_designate_charset(b")0"), 2);
        assert_eq!(
            state.encodings[1].encoding(),
            crate::vterm::encoding::VTermEncoding::DecSpecialGraphics
        );
        assert_eq!(state.escape_designate_charset(b"x0"), 0);
    }
    #[test]
    fn escape_keypad_mode_toggles_application_keypad() {
        let mut state = VTermState::new(1, 1);
        assert_eq!(state.escape_keypad_mode(b'='), 1);
        assert!(state.mode.keypad);
        assert_eq!(state.escape_keypad_mode(b'>'), 1);
        assert!(!state.mode.keypad);
        assert_eq!(state.escape_keypad_mode(b'?'), 0);
    }
    #[test]
    fn escape_locking_shift_selects_gl_and_gr_sets() {
        let mut state = VTermState::new(1, 1);
        assert_eq!(state.escape_locking_shift(b'o'), 1);
        assert_eq!(state.gl_set, 3);
        assert_eq!(state.escape_locking_shift(b'}'), 1);
        assert_eq!(state.gr_set, 2);
        assert_eq!(state.escape_locking_shift(b'x'), 0);
    }
    #[test]
    fn escape_saved_cursor_roundtrips_position() {
        let mut state = VTermState::new(1, 10);
        state.pos.col = 4;
        assert_eq!(state.escape_saved_cursor(b'7'), 1);
        state.pos.col = 0;
        assert_eq!(state.escape_saved_cursor(b'8'), 1);
        assert_eq!(state.pos.col, 4);
        assert_eq!(state.escape_saved_cursor(b'9'), 0);
    }
    #[test]
    fn set_dec_basic_mode_handles_cursor_wrap_and_paste() {
        let mut state = VTermState::new(1, 1);
        assert!(state.set_dec_basic_mode(1, 1));
        assert!(state.set_dec_basic_mode(7, 1));
        assert!(state.set_dec_basic_mode(2004, 1));
        assert!(state.mode.cursor && state.mode.autowrap && state.mode.bracketpaste);
        assert!(!state.set_dec_basic_mode(2, 1));
    }
    #[test]
    fn leftrightmargin_mode_clears_doublewidth_rows_on_enable() {
        let mut state = VTermState::new(2, 10);
        state.lineinfos[0][0].doublewidth = true;
        state.set_leftrightmargin_mode(1);
        assert!(state.mode.leftrightmargin);
        assert_eq!(state.lineinfos[0][0], Default::default());
    }
    #[test]
    fn mouse_tracking_mode_maps_click_drag_and_move() {
        let mut state = VTermState::new(1, 1);
        assert!(state.set_mouse_tracking_mode(1002, 1));
        assert_eq!(
            state.mouse_flags,
            crate::vterm::mouse::MOUSE_WANT_CLICK | crate::vterm::mouse::MOUSE_WANT_DRAG
        );
        assert!(state.set_mouse_tracking_mode(1002, 0));
        assert_eq!(state.mouse_flags, 0);
        assert!(!state.set_mouse_tracking_mode(999, 1));
    }
    #[test]
    fn mouse_protocol_mode_maps_extensions_and_disable() {
        let mut state = VTermState::new(1, 1);
        assert!(state.set_mouse_protocol_mode(1006, 1));
        assert_eq!(state.mouse_protocol, crate::vterm::mouse::VTermMouseProtocol::Sgr);
        assert!(state.set_mouse_protocol_mode(1006, 0));
        assert_eq!(state.mouse_protocol, crate::vterm::mouse::VTermMouseProtocol::X10);
        assert!(!state.set_mouse_protocol_mode(999, 1));
    }
    #[test]
    fn dec_mode_value_reports_known_modes() {
        let mut state = VTermState::new(1, 1);
        state.mode.autowrap = true;
        state.mouse_flags =
            crate::vterm::mouse::MOUSE_WANT_CLICK | crate::vterm::mouse::MOUSE_WANT_DRAG;
        assert_eq!(state.dec_mode_value(7), Some(true));
        assert_eq!(state.dec_mode_value(1002), Some(true));
        assert_eq!(state.dec_mode_value(999), None);
    }
    #[test]
    fn csi_move_cursor_handles_relative_commands() {
        let mut state = VTermState::new(10, 10);
        state.pos = crate::vterm_defs::VTermPos { row: 5, col: 5 };
        assert!(state.csi_move_cursor(b'A', 2));
        assert_eq!(state.pos.row, 3);
        assert!(state.csi_move_cursor(b'k', 1));
        assert_eq!(state.pos.row, 2);
        assert!(state.csi_move_cursor(b'E', 1));
        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 3, col: 0 });
        assert!(!state.csi_move_cursor(b'Z', 1));
    }
    #[test]
    fn csi_position_handles_absolute_and_origin_relative_coordinates() {
        let mut state = VTermState::new(10, 10);
        assert!(state.csi_position(b'H', 3, 4));
        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 2, col: 3 });
        state.mode.origin = true;
        state.scrollregion_top = 2;
        state.mode.leftrightmargin = true;
        state.scrollregion_left = 1;
        state.csi_position(b'f', 1, 1);
        assert_eq!(state.pos, crate::vterm_defs::VTermPos { row: 2, col: 1 });
    }
    #[test]
    fn vertical_margins_validate_and_home_cursor() {
        let mut state = VTermState::new(24, 80);
        state.pos.col = 4;
        state.set_vertical_margins(3, Some(20));
        assert_eq!((state.scrollregion_top, state.scrollregion_bottom), (2, 20));
        assert_eq!(state.pos, Default::default());
        state.set_vertical_margins(20, Some(10));
        assert_eq!((state.scrollregion_top, state.scrollregion_bottom), (0, -1));
    }
    #[test]
    fn horizontal_margins_validate_and_home_cursor() {
        let mut state = VTermState::new(24, 80);
        state.set_horizontal_margins(5, Some(70));
        assert_eq!((state.scrollregion_left, state.scrollregion_right), (4, 70));
        state.set_horizontal_margins(70, Some(5));
        assert_eq!((state.scrollregion_left, state.scrollregion_right), (0, -1));
    }
    #[test]
    fn cursor_style_maps_blinking_and_steady_shapes() {
        let mut state = VTermState::new(1, 1);
        state.set_cursor_style(3);
        assert!(state.mode.cursor_blink);
        assert_eq!(
            state.mode.cursor_shape,
            crate::vterm_defs::VTERM_PROP_CURSORSHAPE_UNDERLINE as u8
        );
        state.set_cursor_style(6);
        assert!(!state.mode.cursor_blink);
        assert_eq!(
            state.mode.cursor_shape,
            crate::vterm_defs::VTERM_PROP_CURSORSHAPE_BAR_LEFT as u8
        );
    }
    #[test]
    fn character_protection_maps_decsca_values() {
        let mut state = VTermState::new(1, 1);
        state.set_character_protection(1);
        assert!(state.protected_cell);
        state.set_character_protection(2);
        assert!(!state.protected_cell);
    }
    #[test]
    fn initialize_pen_colors_sets_defaults_and_ansi_palette() {
        let mut state = VTermState::new(1, 1);
        state.initialize_pen_colors();
        assert!(state.default_fg.is_default_fg());
        assert!(state.default_bg.is_default_bg());
        assert_eq!((state.colors[1].red, state.colors[1].green), (224, 0));
    }
    #[test]
    fn state_default_color_wrapper_sets_metadata() {
        let mut state = VTermState::new(1, 1);
        let color = crate::vterm_defs::VTermColor {
            red: 1, green: 2, blue: 3, ..Default::default()
        };
        state.set_default_colors(Some(&color), None);
        assert!(state.default_fg.is_default_fg());
        assert_eq!(state.default_fg.red, 1);
    }
    #[test]
    fn state_palette_color_wrapper_bounds_checks() {
        let mut state = VTermState::new(1, 1);
        let color = crate::vterm_defs::VTermColor {
            red: 9, ..Default::default()
        };
        state.set_palette_color(3, &color);
        assert_eq!(state.colors[3].red, 9);
        state.set_palette_color(16, &Default::default());
        assert_eq!(state.colors[3].red, 9);
    }
    #[test]
    fn state_color_conversion_uses_state_palette() {
        let mut state = VTermState::new(1, 1);
        state.initialize_pen_colors();
        let mut color = crate::vterm_defs::VTermColor::default();
        crate::vterm_defs::vterm_color_indexed(&mut color, 2);
        state.convert_color_to_rgb(&mut color);
        assert_eq!((color.red, color.green, color.blue), (0, 224, 0));
    }
    #[test]
    fn state_penattr_wrapper_updates_current_pen() {
        let mut state = VTermState::new(1, 1);
        let mut callbacks = ();
        assert_eq!(
            state.set_penattr(
                &mut callbacks,
                crate::vterm_defs::VTermAttr::Bold,
                crate::vterm_defs::VTermValueType::Bool,
                &crate::vterm_defs::VTermValue::Boolean(1),
            ),
            1
        );
        assert!(state.pen.bold);
    }
    #[test]
    fn state_setpen_applies_sgr_arguments() {
        let mut state = VTermState::new(1, 1);
        state.initialize_pen_colors();
        state.setpen(&mut (), &[1, 3, 4]);
        assert!(state.pen.bold);
        assert!(state.pen.italic);
        assert_eq!(state.pen.underline, crate::vterm_defs::VTERM_UNDERLINE_SINGLE);
    }
    #[test]
    fn reset_pen_restores_state_default_colors() {
        let mut state = VTermState::new(1, 1);
        state.initialize_pen_colors();
        state.pen.bold = true;
        let mut callbacks = ();
        state.reset_pen(&mut callbacks);
        assert!(!state.pen.bold);
        assert_eq!(state.pen.fg, state.default_fg);
        assert_eq!(state.pen.bg, state.default_bg);
    }
    #[test]
    fn reset_tabstops_sets_every_eighth_column() {
        let mut state = VTermState::new(1, 20);
        state.tabstops.fill(0xFF);
        state.reset_tabstops();
        assert!(state.is_col_tabstop(0));
        assert!(state.is_col_tabstop(8));
        assert!(state.is_col_tabstop(16));
        assert!(!state.is_col_tabstop(7));
    }
    #[test]
    fn reset_lineinfos_clears_active_rows_only() {
        let mut state = VTermState::new(2, 2);
        state.lineinfos[0][0].doublewidth = true;
        state.lineinfos[1][0].doublewidth = true;
        state.reset_lineinfos();
        assert!(!state.lineinfos[0][0].doublewidth);
        assert!(state.lineinfos[1][0].doublewidth);
    }
    #[test]
    fn hard_reset_cursor_clears_position_and_phantom() {
        let mut state = VTermState::new(1, 1);
        state.pos = crate::vterm_defs::VTermPos { row: 3, col: 4 };
        state.at_phantom = true;
        state.hard_reset_cursor();
        assert_eq!(state.pos, Default::default());
        assert!(!state.at_phantom);
    }
    #[test]
    fn state_reset_restores_soft_and_hard_defaults() {
        let mut state = VTermState::new(2, 10);
        state.pos.col = 5;
        state.protected_cell = true;
        state.mode.insert = true;
        state.reset(false);
        assert_eq!(state.pos.col, 5);
        assert!(!state.protected_cell);
        assert!(!state.mode.insert);
        assert!(state.mode.autowrap);
        state.reset(true);
        assert_eq!(state.pos, Default::default());
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
        assert!(!callbacks.bell());
        assert!(!callbacks.set_pen_attr(
            crate::vterm_defs::VTermAttr::Bold,
            &crate::vterm_defs::VTermValue::Boolean(1),
        ));
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
        assert_eq!(state.default_fg, crate::vterm_defs::VTermColor::default());
        assert_eq!(state.default_bg, crate::vterm_defs::VTermColor::default());
        assert_eq!(state.colors, [crate::vterm_defs::VTermColor::default(); 16]);
        assert!(!state.bold_is_highbright);
        assert!(!state.ctrl8bit);
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
