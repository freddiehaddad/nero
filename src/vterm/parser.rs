//! Translated from `src/nvim/vterm/parser.c`.

pub const INTERMED_MAX: usize = 16;
pub const CSI_ARGS_MAX: usize = 32;
pub const CSI_LEADER_MAX: usize = 16;

pub type CsiArg = u32;
pub const CSI_ARG_FLAG_MORE: CsiArg = 1 << 31;
pub const CSI_ARG_MASK: CsiArg = !CSI_ARG_FLAG_MORE;
pub const CSI_ARG_MISSING: CsiArg = CSI_ARG_FLAG_MORE - 1;

/// Parser state (`enum VTermParserState`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum VTermParserState {
    #[default]
    Normal = 0,
    CsiLeader,
    CsiArgs,
    CsiIntermed,
    DcsCommand,
    OscCommand,
    Osc,
    DcsVterm,
    Apc,
    Pm,
    Sos,
}

impl VTermParserState {
    /// Equivalent to parser.c's `IS_STRING_STATE()` macro.
    #[must_use]
    pub const fn is_string(self) -> bool {
        self as u8 >= Self::OscCommand as u8
    }
}

/// CSI-specific parser storage (`parser.v.csi`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsiParserData {
    pub leader_len: usize,
    pub leader: [u8; CSI_LEADER_MAX],
    pub argi: usize,
    pub args: [CsiArg; CSI_ARGS_MAX],
}

impl Default for CsiParserData {
    fn default() -> Self {
        Self {
            leader_len: 0,
            leader: [0; CSI_LEADER_MAX],
            argi: 0,
            args: [0; CSI_ARGS_MAX],
        }
    }
}

/// DCS-specific parser storage (`parser.v.dcs`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DcsParserData {
    pub command_len: usize,
    pub command: [u8; CSI_LEADER_MAX],
}

/// Parser fields embedded in `VTerm`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VTermParser {
    pub state: VTermParserState,
    pub in_esc: bool,
    pub intermed_len: usize,
    pub intermed: [u8; INTERMED_MAX],
    pub csi: CsiParserData,
    pub osc_command: i32,
    pub dcs: DcsParserData,
    pub string_initial: bool,
    pub emit_nul: bool,
}

impl Default for VTermParser {
    fn default() -> Self {
        Self {
            state: VTermParserState::Normal,
            in_esc: false,
            intermed_len: 0,
            intermed: [0; INTERMED_MAX],
            csi: CsiParserData::default(),
            osc_command: 0,
            dcs: DcsParserData::default(),
            string_initial: false,
            emit_nul: false,
        }
    }
}

/// Whether `c` is an escape/CSI intermediate byte (`is_intermed`).
#[must_use]
pub const fn is_intermed(c: u8) -> bool {
    c >= 0x20 && c <= 0x2F
}

/// Parser callback surface (`VTermParserCallbacks`).
pub trait VTermParserCallbacks {
    /// Returns the number of text bytes consumed.
    fn text(&mut self, _bytes: &[u8]) -> usize {
        0
    }

    /// Returns true when the control was handled.
    fn control(&mut self, _control: u8) -> bool {
        false
    }

    /// Returns true when the escape sequence was handled.
    fn escape(&mut self, _bytes: &[u8]) -> bool {
        false
    }

    /// Returns true when the CSI sequence was handled.
    fn csi(
        &mut self,
        _leader: Option<&[u8]>,
        _args: &[CsiArg],
        _intermed: Option<&[u8]>,
        _command: u8,
    ) -> bool {
        false
    }

    fn osc(
        &mut self,
        _command: i32,
        _fragment: crate::vterm_defs::VTermStringFragment<'_>,
    ) -> bool {
        false
    }

    fn dcs(
        &mut self,
        _command: &[u8],
        _fragment: crate::vterm_defs::VTermStringFragment<'_>,
    ) -> bool {
        false
    }

    fn apc(&mut self, _fragment: crate::vterm_defs::VTermStringFragment<'_>) -> bool {
        false
    }

    fn pm(&mut self, _fragment: crate::vterm_defs::VTermStringFragment<'_>) -> bool {
        false
    }

    fn sos(&mut self, _fragment: crate::vterm_defs::VTermStringFragment<'_>) -> bool {
        false
    }

    fn resize(&mut self, _rows: i32, _cols: i32) -> bool {
        false
    }
}

impl VTermParserCallbacks for () {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum C1Action {
    NoString,
    StartString,
}

#[allow(dead_code)]
impl VTermParser {
    fn do_control<C: VTermParserCallbacks>(&self, callbacks: &mut C, control: u8) {
        let _ = callbacks.control(control);
    }

    fn do_csi<C: VTermParserCallbacks>(&self, callbacks: &mut C, command: u8) {
        let leader = (self.csi.leader_len != 0)
            .then_some(&self.csi.leader[..self.csi.leader_len]);
        let intermed = (self.intermed_len != 0)
            .then_some(&self.intermed[..self.intermed_len]);
        let _ = callbacks.csi(
            leader,
            &self.csi.args[..self.csi.argi],
            intermed,
            command,
        );
    }

    fn do_escape<C: VTermParserCallbacks>(&self, callbacks: &mut C, command: u8) {
        let mut sequence = Vec::with_capacity(self.intermed_len + 1);
        sequence.extend_from_slice(&self.intermed[..self.intermed_len]);
        sequence.push(command);
        let _ = callbacks.escape(&sequence);
    }

    fn string_fragment<C: VTermParserCallbacks>(
        &mut self,
        callbacks: &mut C,
        bytes: &[u8],
        final_fragment: bool,
        terminator: crate::vterm_defs::VTermTerminator,
    ) {
        let fragment = crate::vterm_defs::VTermStringFragment {
            bytes,
            initial: self.string_initial,
            final_fragment,
            terminator,
        };

        match self.state {
            VTermParserState::Osc => {
                let _ = callbacks.osc(self.osc_command, fragment);
            }
            VTermParserState::DcsVterm => {
                let _ = callbacks.dcs(&self.dcs.command[..self.dcs.command_len], fragment);
            }
            VTermParserState::Apc => {
                let _ = callbacks.apc(fragment);
            }
            VTermParserState::Pm => {
                let _ = callbacks.pm(fragment);
            }
            VTermParserState::Sos => {
                let _ = callbacks.sos(fragment);
            }
            VTermParserState::Normal
            | VTermParserState::CsiLeader
            | VTermParserState::CsiArgs
            | VTermParserState::CsiIntermed
            | VTermParserState::DcsCommand
            | VTermParserState::OscCommand => return,
        }
        self.string_initial = false;
    }

    fn do_c1<C: VTermParserCallbacks>(
        &mut self,
        callbacks: &mut C,
        control: u8,
    ) -> C1Action {
        match control {
            0x90 => {
                self.string_initial = true;
                self.dcs.command_len = 0;
                self.state = VTermParserState::DcsCommand;
            }
            0x98 => {
                self.string_initial = true;
                self.state = VTermParserState::Sos;
                return C1Action::StartString;
            }
            0x9B => {
                self.csi.leader_len = 0;
                self.state = VTermParserState::CsiLeader;
            }
            0x9D => {
                self.osc_command = -1;
                self.string_initial = true;
                self.state = VTermParserState::OscCommand;
            }
            0x9E => {
                self.string_initial = true;
                self.state = VTermParserState::Pm;
                return C1Action::StartString;
            }
            0x9F => {
                self.string_initial = true;
                self.state = VTermParserState::Apc;
                return C1Action::StartString;
            }
            _ => self.do_control(callbacks, control),
        }
        C1Action::NoString
    }

    /// Handles one byte in `CSI_LEADER`. Returns true when consumed;
    /// false means the same byte must fall through to `CSI_ARGS`.
    fn parse_csi_leader(&mut self, byte: u8) -> bool {
        if (0x3C..=0x3F).contains(&byte) {
            if self.csi.leader_len < CSI_LEADER_MAX - 1 {
                self.csi.leader[self.csi.leader_len] = byte;
                self.csi.leader_len += 1;
            }
            return true;
        }

        self.csi.leader[self.csi.leader_len] = 0;
        self.csi.argi = 0;
        self.csi.args[0] = CSI_ARG_MISSING;
        self.state = VTermParserState::CsiArgs;
        false
    }

    /// Handles one byte in `CSI_ARGS`. Returns true when consumed;
    /// false means the same byte must fall through to `CSI_INTERMED`.
    fn parse_csi_args(&mut self, mut byte: u8) -> bool {
        if byte.is_ascii_digit() {
            let arg = &mut self.csi.args[self.csi.argi];
            if *arg == CSI_ARG_MISSING {
                *arg = 0;
            }
            *arg = arg.wrapping_mul(10);
            *arg = arg.wrapping_add(u32::from(byte - b'0'));
            return true;
        }
        if byte == b':' {
            self.csi.args[self.csi.argi] |= CSI_ARG_FLAG_MORE;
            byte = b';';
        }
        if byte == b';' {
            self.csi.argi += 1;
            self.csi.args[self.csi.argi] = CSI_ARG_MISSING;
            return true;
        }

        self.csi.argi += 1;
        self.intermed_len = 0;
        self.state = VTermParserState::CsiIntermed;
        false
    }
}

#[must_use]
pub const fn csi_arg_has_more(arg: CsiArg) -> bool {
    arg & CSI_ARG_FLAG_MORE != 0
}

#[must_use]
pub const fn csi_arg(arg: CsiArg) -> CsiArg {
    arg & CSI_ARG_MASK
}

#[must_use]
pub const fn csi_arg_is_missing(arg: CsiArg) -> bool {
    csi_arg(arg) == CSI_ARG_MISSING
}

#[must_use]
pub const fn csi_arg_or(arg: CsiArg, default: CsiArg) -> CsiArg {
    if csi_arg_is_missing(arg) {
        default
    } else {
        csi_arg(arg)
    }
}

#[must_use]
pub const fn csi_arg_count(arg: CsiArg) -> CsiArg {
    let value = csi_arg(arg);
    if value == CSI_ARG_MISSING || value == 0 {
        1
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type CsiCapture = (Option<Vec<u8>>, Vec<CsiArg>, Option<Vec<u8>>, u8);

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum StringCaptureKind {
        Osc(i32),
        Dcs(Vec<u8>),
        Apc,
        Pm,
        Sos,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct StringCapture {
        kind: StringCaptureKind,
        bytes: Vec<u8>,
        initial: bool,
        final_fragment: bool,
        terminator: crate::vterm_defs::VTermTerminator,
    }

    #[derive(Default)]
    struct DispatchCapture {
        controls: Vec<u8>,
        escapes: Vec<Vec<u8>>,
        csi: Vec<CsiCapture>,
        strings: Vec<StringCapture>,
    }

    impl VTermParserCallbacks for DispatchCapture {
        fn control(&mut self, control: u8) -> bool {
            self.controls.push(control);
            true
        }

        fn escape(&mut self, bytes: &[u8]) -> bool {
            self.escapes.push(bytes.to_vec());
            true
        }

        fn csi(
            &mut self,
            leader: Option<&[u8]>,
            args: &[CsiArg],
            intermed: Option<&[u8]>,
            command: u8,
        ) -> bool {
            self.csi.push((
                leader.map(<[u8]>::to_vec),
                args.to_vec(),
                intermed.map(<[u8]>::to_vec),
                command,
            ));
            true
        }

        fn osc(
            &mut self,
            command: i32,
            fragment: crate::vterm_defs::VTermStringFragment<'_>,
        ) -> bool {
            self.capture_string(StringCaptureKind::Osc(command), fragment);
            true
        }

        fn dcs(
            &mut self,
            command: &[u8],
            fragment: crate::vterm_defs::VTermStringFragment<'_>,
        ) -> bool {
            self.capture_string(StringCaptureKind::Dcs(command.to_vec()), fragment);
            true
        }

        fn apc(&mut self, fragment: crate::vterm_defs::VTermStringFragment<'_>) -> bool {
            self.capture_string(StringCaptureKind::Apc, fragment);
            true
        }

        fn pm(&mut self, fragment: crate::vterm_defs::VTermStringFragment<'_>) -> bool {
            self.capture_string(StringCaptureKind::Pm, fragment);
            true
        }

        fn sos(&mut self, fragment: crate::vterm_defs::VTermStringFragment<'_>) -> bool {
            self.capture_string(StringCaptureKind::Sos, fragment);
            true
        }
    }

    impl DispatchCapture {
        fn capture_string(
            &mut self,
            kind: StringCaptureKind,
            fragment: crate::vterm_defs::VTermStringFragment<'_>,
        ) {
            self.strings.push(StringCapture {
                kind,
                bytes: fragment.bytes.to_vec(),
                initial: fragment.initial,
                final_fragment: fragment.final_fragment,
                terminator: fragment.terminator,
            });
        }
    }

    #[test]
    fn parser_limits_match_internal_defs() {
        assert_eq!(INTERMED_MAX, 16);
        assert_eq!(CSI_ARGS_MAX, 32);
        assert_eq!(CSI_LEADER_MAX, 16);
    }

    #[test]
    fn csi_argument_helpers_match_vterm_macros() {
        assert_eq!(CSI_ARG_FLAG_MORE, 0x8000_0000);
        assert_eq!(CSI_ARG_MASK, 0x7FFF_FFFF);
        assert_eq!(CSI_ARG_MISSING, 0x7FFF_FFFF);

        let continued = CSI_ARG_FLAG_MORE | 42;
        assert!(csi_arg_has_more(continued));
        assert_eq!(csi_arg(continued), 42);
        assert!(!csi_arg_is_missing(continued));
        assert_eq!(csi_arg_or(continued, 7), 42);
        assert_eq!(csi_arg_count(continued), 42);

        assert!(csi_arg_is_missing(CSI_ARG_MISSING));
        assert_eq!(csi_arg_or(CSI_ARG_MISSING, 7), 7);
        assert_eq!(csi_arg_count(CSI_ARG_MISSING), 1);
        assert_eq!(csi_arg_count(0), 1);
    }

    #[test]
    fn parser_state_order_preserves_string_state_macro() {
        for state in [
            VTermParserState::Normal,
            VTermParserState::CsiLeader,
            VTermParserState::CsiArgs,
            VTermParserState::CsiIntermed,
            VTermParserState::DcsCommand,
        ] {
            assert!(!state.is_string(), "{state:?}");
        }
        for state in [
            VTermParserState::OscCommand,
            VTermParserState::Osc,
            VTermParserState::DcsVterm,
            VTermParserState::Apc,
            VTermParserState::Pm,
            VTermParserState::Sos,
        ] {
            assert!(state.is_string(), "{state:?}");
        }
        assert_eq!(VTermParserState::default(), VTermParserState::Normal);
    }

    #[test]
    fn parser_storage_defaults_match_vterm_build_initialization() {
        let parser = VTermParser::default();
        assert_eq!(parser.state, VTermParserState::Normal);
        assert!(!parser.in_esc);
        assert_eq!(parser.intermed_len, 0);
        assert_eq!(parser.intermed, [0; INTERMED_MAX]);
        assert_eq!(parser.csi, CsiParserData::default());
        assert_eq!(parser.dcs, DcsParserData::default());
        assert!(!parser.string_initial);
        assert!(!parser.emit_nul);
    }

    #[test]
    fn intermediate_byte_range_is_inclusive_and_exact() {
        assert!(!is_intermed(0x1F));
        assert!(is_intermed(0x20));
        assert!(is_intermed(0x27));
        assert!(is_intermed(0x2F));
        assert!(!is_intermed(0x30));
    }

    #[test]
    fn default_parser_callbacks_match_absent_c_callbacks() {
        let callbacks = &mut ();
        assert_eq!(callbacks.text(b"abc"), 0);
        assert!(!callbacks.control(0x07));
        assert!(!callbacks.escape(b"(B"));
        assert!(!callbacks.csi(None, &[], None, b'm'));
        let fragment = crate::vterm_defs::VTermStringFragment {
            bytes: b"x",
            initial: true,
            final_fragment: true,
            terminator: crate::vterm_defs::VTermTerminator::St,
        };
        assert!(!callbacks.osc(0, fragment));
        assert!(!callbacks.dcs(b"q", fragment));
        assert!(!callbacks.apc(fragment));
        assert!(!callbacks.pm(fragment));
        assert!(!callbacks.sos(fragment));
        assert!(!callbacks.resize(24, 80));
    }

    #[test]
    fn parser_dispatch_helpers_forward_exact_sequence_parts() {
        let mut parser = VTermParser::default();
        parser.csi.leader[..2].copy_from_slice(b"?>");
        parser.csi.leader_len = 2;
        parser.csi.args[..3].copy_from_slice(&[1, CSI_ARG_FLAG_MORE | 2, 3]);
        parser.csi.argi = 3;
        parser.intermed[..2].copy_from_slice(b" !");
        parser.intermed_len = 2;
        let mut capture = DispatchCapture::default();

        parser.do_control(&mut capture, 0x07);
        parser.do_escape(&mut capture, b'F');
        parser.do_csi(&mut capture, b'm');

        assert_eq!(capture.controls, [0x07]);
        assert_eq!(capture.escapes, [b" !F".to_vec()]);
        assert_eq!(
            capture.csi,
            [(
                Some(b"?>".to_vec()),
                vec![1, CSI_ARG_FLAG_MORE | 2, 3],
                Some(b" !".to_vec()),
                b'm',
            )]
        );
    }

    #[test]
    fn parser_csi_dispatch_uses_none_for_empty_optional_parts() {
        let mut parser = VTermParser::default();
        parser.csi.argi = 1;
        parser.csi.args[0] = CSI_ARG_MISSING;
        let mut capture = DispatchCapture::default();
        parser.do_csi(&mut capture, b'A');
        assert_eq!(
            capture.csi,
            [(None, vec![CSI_ARG_MISSING], None, b'A')]
        );
    }

    #[test]
    fn parser_string_fragment_dispatches_each_payload_state() {
        let cases = [
            (VTermParserState::Osc, StringCaptureKind::Osc(12)),
            (
                VTermParserState::DcsVterm,
                StringCaptureKind::Dcs(b"+q".to_vec()),
            ),
            (VTermParserState::Apc, StringCaptureKind::Apc),
            (VTermParserState::Pm, StringCaptureKind::Pm),
            (VTermParserState::Sos, StringCaptureKind::Sos),
        ];
        for (state, expected_kind) in cases {
            let mut parser = VTermParser {
                state,
                osc_command: 12,
                string_initial: true,
                ..Default::default()
            };
            parser.dcs.command[..2].copy_from_slice(b"+q");
            parser.dcs.command_len = 2;
            let mut capture = DispatchCapture::default();
            parser.string_fragment(
                &mut capture,
                b"part",
                false,
                crate::vterm_defs::VTermTerminator::St,
            );
            parser.string_fragment(
                &mut capture,
                b"end",
                true,
                crate::vterm_defs::VTermTerminator::Bel,
            );

            assert_eq!(capture.strings, [
                StringCapture {
                    kind: expected_kind.clone(),
                    bytes: b"part".to_vec(),
                    initial: true,
                    final_fragment: false,
                    terminator: crate::vterm_defs::VTermTerminator::St,
                },
                StringCapture {
                    kind: expected_kind,
                    bytes: b"end".to_vec(),
                    initial: false,
                    final_fragment: true,
                    terminator: crate::vterm_defs::VTermTerminator::Bel,
                },
            ]);
            assert!(!parser.string_initial);
        }
    }

    #[test]
    fn parser_string_fragment_ignores_command_and_nonstring_states() {
        for state in [
            VTermParserState::Normal,
            VTermParserState::CsiLeader,
            VTermParserState::CsiArgs,
            VTermParserState::CsiIntermed,
            VTermParserState::DcsCommand,
            VTermParserState::OscCommand,
        ] {
            let mut parser = VTermParser {
                state,
                string_initial: true,
                ..Default::default()
            };
            let mut capture = DispatchCapture::default();
            parser.string_fragment(
                &mut capture,
                b"ignored",
                true,
                crate::vterm_defs::VTermTerminator::St,
            );
            assert!(capture.strings.is_empty());
            assert!(parser.string_initial);
        }
    }

    #[test]
    fn parser_c1_controls_enter_the_matching_states() {
            let cases = [
                (0x90, VTermParserState::DcsCommand, C1Action::NoString),
                (0x98, VTermParserState::Sos, C1Action::StartString),
                (0x9B, VTermParserState::CsiLeader, C1Action::NoString),
                (0x9D, VTermParserState::OscCommand, C1Action::NoString),
                (0x9E, VTermParserState::Pm, C1Action::StartString),
                (0x9F, VTermParserState::Apc, C1Action::StartString),
            ];
            for (control, state, action) in cases {
                let mut parser = VTermParser::default();
                parser.csi.leader_len = 4;
                parser.dcs.command_len = 4;
                let mut capture = DispatchCapture::default();
                assert_eq!(parser.do_c1(&mut capture, control), action);
                assert_eq!(parser.state, state);
                assert!(capture.controls.is_empty());
                match state {
                    VTermParserState::DcsCommand => {
                        assert_eq!(parser.dcs.command_len, 0);
                        assert!(parser.string_initial);
                    }
                    VTermParserState::CsiLeader => assert_eq!(parser.csi.leader_len, 0),
                    VTermParserState::OscCommand => {
                        assert_eq!(parser.osc_command, -1);
                        assert!(parser.string_initial);
                    }
                    VTermParserState::Sos | VTermParserState::Pm | VTermParserState::Apc => {
                        assert!(parser.string_initial);
                    }
                    _ => unreachable!(),
                }
            }
        }

    #[test]
    fn parser_other_c1_controls_use_the_control_callback() {
            let mut parser = VTermParser::default();
            let mut capture = DispatchCapture::default();
            assert_eq!(
                parser.do_c1(&mut capture, 0x84),
                C1Action::NoString
            );
            assert_eq!(parser.state, VTermParserState::Normal);
            assert_eq!(capture.controls, [0x84]);
    }

    #[test]
    fn parser_csi_leader_collects_only_leader_bytes() {
        let mut parser = VTermParser {
            state: VTermParserState::CsiLeader,
            ..Default::default()
        };
        for byte in b"?>=" {
            assert!(parser.parse_csi_leader(*byte));
        }
        assert_eq!(&parser.csi.leader[..parser.csi.leader_len], b"?>=");
        assert_eq!(parser.state, VTermParserState::CsiLeader);

        assert!(!parser.parse_csi_leader(b'1'));
        assert_eq!(parser.state, VTermParserState::CsiArgs);
        assert_eq!(parser.csi.argi, 0);
        assert_eq!(parser.csi.args[0], CSI_ARG_MISSING);
        assert_eq!(parser.csi.leader[parser.csi.leader_len], 0);
    }

    #[test]
    fn parser_csi_leader_truncates_to_leave_a_nul_slot() {
        let mut parser = VTermParser {
            state: VTermParserState::CsiLeader,
            ..Default::default()
        };
        for _ in 0..(CSI_LEADER_MAX + 3) {
            assert!(parser.parse_csi_leader(b'?'));
        }
        assert_eq!(parser.csi.leader_len, CSI_LEADER_MAX - 1);
        assert!(parser.csi.leader[..parser.csi.leader_len]
            .iter()
            .all(|&byte| byte == b'?'));
        assert!(!parser.parse_csi_leader(b'm'));
        assert_eq!(parser.csi.leader[CSI_LEADER_MAX - 1], 0);
    }

    #[test]
    fn parser_csi_args_collects_numbers_and_missing_arguments() {
        let mut parser = VTermParser {
            state: VTermParserState::CsiArgs,
            ..Default::default()
        };
        parser.csi.args[0] = CSI_ARG_MISSING;
        for byte in b"12;3;" {
            assert!(parser.parse_csi_args(*byte));
        }
        assert_eq!(parser.csi.argi, 2);
        assert_eq!(
            &parser.csi.args[..=parser.csi.argi],
            &[12, 3, CSI_ARG_MISSING]
        );
    }

    #[test]
    fn parser_csi_args_marks_colon_subparameters() {
        let mut parser = VTermParser {
            state: VTermParserState::CsiArgs,
            ..Default::default()
        };
        parser.csi.args[0] = CSI_ARG_MISSING;
        for byte in b"4:3:2" {
            assert!(parser.parse_csi_args(*byte));
        }
        assert_eq!(
            &parser.csi.args[..=parser.csi.argi],
            &[
                CSI_ARG_FLAG_MORE | 4,
                CSI_ARG_FLAG_MORE | 3,
                2,
            ]
        );
    }

    #[test]
    fn parser_csi_args_transitions_to_intermediate_with_argument_count() {
        let mut parser = VTermParser {
            state: VTermParserState::CsiArgs,
            ..Default::default()
        };
        parser.csi.args[0] = CSI_ARG_MISSING;
        assert!(!parser.parse_csi_args(b'm'));
        assert_eq!(parser.csi.argi, 1);
        assert_eq!(parser.csi.args[0], CSI_ARG_MISSING);
        assert_eq!(parser.intermed_len, 0);
        assert_eq!(parser.state, VTermParserState::CsiIntermed);
    }
}
