//! Translated from `src/nvim/state_defs.h`.

/// A polymorphic editor "mode" state: a pair of callbacks checked/executed
/// by the main loop (`struct vim_state`, typedef'd as `VimState`).
///
/// Kept as plain function-pointer fields (matching the original's
/// `state_check_callback`/`state_execute_callback` typedefs) rather than
/// committing to a trait-object or enum-based design - that's a real
/// architectural decision for `state.c`'s dispatch loop (not yet
/// translated) to make, not something to invent ahead of time here.
#[derive(Debug, Clone, Copy, Default)]
pub struct VimState {
    pub check: Option<StateCheckCallback>,
    pub execute: Option<StateExecuteCallback>,
}

/// `state_check_callback`
pub type StateCheckCallback = fn(&mut VimState) -> i32;
/// `state_execute_callback`
pub type StateExecuteCallback = fn(&mut VimState, i32) -> i32;

/// Values for `State` (kept as plain `u32` bit-flag constants, same
/// reasoning as `HL_*`/`MarkMoveRes` elsewhere in this crate).
///
/// The lower bits up to `0x80` are used to distinguish normal/visual/
/// op_pending/cmdline/insert/replace/terminal mode. This is used for
/// mapping. If none of these bits are set, no mapping is done. See the
/// comment above `do_map()` (not yet translated). The upper bits are
/// used to distinguish between other states and variants of the base
/// modes.
pub mod mode {
    /// Normal mode, command expected.
    pub const NORMAL: u32 = 0x01;
    /// Visual mode - use `get_real_state()`.
    pub const VISUAL: u32 = 0x02;
    /// Normal mode, operator is pending - use `get_real_state()`.
    pub const OP_PENDING: u32 = 0x04;
    /// Editing the command line.
    pub const CMDLINE: u32 = 0x08;
    /// Insert mode, also for Replace mode.
    pub const INSERT: u32 = 0x10;
    /// Language mapping, can be combined with `MODE_INSERT` and
    /// `MODE_CMDLINE`.
    pub const LANGMAP: u32 = 0x20;
    /// Select mode, use `get_real_state()`.
    pub const SELECT: u32 = 0x40;
    /// Terminal mode.
    pub const TERMINAL: u32 = 0x80;

    /// all mode bits used for mapping
    pub const MAP_ALL_MODES: u32 = 0xff;

    /// Replace mode flag.
    pub const REPLACE_FLAG: u32 = 0x100;
    pub const REPLACE: u32 = REPLACE_FLAG | INSERT;
    /// Virtual-replace mode flag.
    pub const VREPLACE_FLAG: u32 = 0x200;
    pub const VREPLACE: u32 = REPLACE_FLAG | VREPLACE_FLAG | INSERT;
    pub const LREPLACE: u32 = REPLACE_FLAG | LANGMAP;

    /// Normal mode, busy with a command.
    pub const NORMAL_BUSY: u32 = 0x1000 | NORMAL;
    /// waiting for return or command
    pub const HITRETURN: u32 = 0x2000 | NORMAL;
    /// Asking if you want --more--
    pub const ASKMORE: u32 = 0x3000;
    /// window size has changed
    pub const SETWSIZE: u32 = 0x4000;
    /// executing an external command
    pub const EXTERNCMD: u32 = 0x5000;
    /// show matching paren
    pub const SHOWMATCH: u32 = 0x6000 | INSERT;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vim_state_default_has_no_callbacks() {
        let s = VimState::default();
        assert!(s.check.is_none());
        assert!(s.execute.is_none());
    }

    #[test]
    fn mode_composite_flags_match_c_macros() {
        assert_eq!(mode::REPLACE, 0x100 | 0x10);
        assert_eq!(mode::VREPLACE, 0x100 | 0x200 | 0x10);
        assert_eq!(mode::LREPLACE, 0x100 | 0x20);
        assert_eq!(mode::NORMAL_BUSY, 0x1000 | 0x01);
        assert_eq!(mode::SHOWMATCH, 0x6000 | 0x10);
    }

    #[test]
    fn mode_base_flags_are_distinct_bits() {
        let all = [
            mode::NORMAL,
            mode::VISUAL,
            mode::OP_PENDING,
            mode::CMDLINE,
            mode::INSERT,
            mode::LANGMAP,
            mode::SELECT,
            mode::TERMINAL,
        ];
        let mut seen = 0;
        for f in all {
            assert_eq!(seen & f, 0, "flag {f:#04x} overlaps a previous one");
            seen |= f;
        }
        assert_eq!(seen, mode::MAP_ALL_MODES);
    }
}
