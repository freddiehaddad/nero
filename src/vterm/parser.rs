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
}
