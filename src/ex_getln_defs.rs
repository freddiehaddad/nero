//! Translated from `src/nvim/ex_getln_defs.h`.
//!
//! The command-line editing state shared between `getcmdline()`,
//! `redrawcmdline()` and the many small accessors around them. This
//! type is the single blocker recorded against roughly a dozen
//! `ex_getln.c` functions, so it is translated here on its own.
//!
//! Translated: `CmdlineColorChunk`, `CmdlineColors`, `ColoredCmdline`,
//! `CmdRedraw` and `CmdlineInfo` itself.

use crate::eval::typval_defs::Callback;

/// A region of the command line sharing one highlight
/// (`CmdlineColorChunk`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CmdlineColorChunk {
    /// Colored chunk start (`start`).
    pub start: i32,
    /// Colored chunk end, exclusive and greater than `start` (`end`).
    pub end: i32,
    /// Highlight id (`hl_id`).
    pub hl_id: i32,
}

/// All colors for one command line (`CmdlineColors`).
///
/// The original is a `kvec_t`, whose own length/capacity bookkeeping a
/// `Vec` already provides.
pub type CmdlineColors = Vec<CmdlineColorChunk>;

/// Command-line coloring, holding both what the colors are and what
/// has already been colored (`ColoredCmdline`).
///
/// The second part is what lets repeated coloring-callback calls be
/// suppressed when nothing has changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColoredCmdline {
    /// ID of the prompt which was colored last (`prompt_id`).
    pub prompt_id: u32,
    /// Exactly what was colored last time, if anything (`cmdbuff`).
    pub cmdbuff: Option<Vec<u8>>,
    /// Last colors (`colors`).
    pub colors: CmdlineColors,
}

/// How much command-line state must still be sent to an external UI
/// (`CmdRedraw`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CmdRedraw {
    /// Nothing to send (`kCmdRedrawNone`).
    #[default]
    None = 0,
    /// Only the cursor position changed (`kCmdRedrawPos`).
    Pos = 1,
    /// The whole command line must be resent (`kCmdRedrawAll`).
    All = 2,
}

/// Variables shared between `getcmdline()`, `redrawcmdline()` and
/// others (`CmdlineInfo`).
///
/// These are grouped into a struct precisely so the whole set can be
/// saved and restored around CTRL-R `=`, which re-enters command-line
/// editing recursively.
///
/// The original's `cmdbufflen` (the ALLOCATED size of `cmdbuff`, as
/// distinct from `cmdlen`, the number of characters actually in it)
/// has no equivalent here: a `Vec`'s own capacity is exactly that, so
/// tracking it separately would be duplicate state that could drift.
/// `cmdlen` IS kept, since it is real, observable content length
/// rather than allocation bookkeeping.
#[derive(Debug, Default)]
pub struct CmdlineInfo {
    /// The command line buffer itself (`cmdbuff`).
    pub cmdbuff: Option<Vec<u8>>,
    /// Number of chars in the command line (`cmdlen`).
    pub cmdlen: i32,
    /// Current cursor position (`cmdpos`).
    pub cmdpos: i32,
    /// Cursor column on screen (`cmdspos`).
    pub cmdspos: i32,
    /// `':'`, `'/'`, `'?'`, `'='`, `'>'` or NUL (`cmdfirstc`).
    pub cmdfirstc: i32,
    /// Number of spaces before the command line (`cmdindent`).
    pub cmdindent: i32,
    /// Message shown in front of the command line (`cmdprompt`).
    pub cmdprompt: Option<Vec<u8>>,
    /// Highlight id for the prompt (`hl_id`).
    pub hl_id: i32,
    /// Typing mode on the command line, shared by `getcmdline()` and
    /// `put_on_cmdline()` (`overstrike`).
    pub overstrike: i32,
    /// The expansion state in use; its own `xp_pattern` may point into
    /// `cmdbuff` (`xpc`).
    pub xpc: *mut crate::cmdexpand_defs::ExpandT,
    /// Type of expansion (`xp_context`).
    pub xp_context: i32,
    /// User-defined expansion argument (`xp_arg`).
    pub xp_arg: Option<Vec<u8>>,
    /// Whether this was invoked for the `input()` function
    /// (`input_fn`).
    pub input_fn: i32,
    /// Whether the command line was replaced externally, e.g. by
    /// `setcmdline()` (`cmdbuff_replaced`).
    pub cmdbuff_replaced: bool,
    /// Prompt number, used to disable coloring on errors
    /// (`prompt_id`).
    pub prompt_id: u32,
    /// Callback used for coloring user input (`highlight_callback`).
    pub highlight_callback: Callback,
    /// Last command-line colors (`last_colors`).
    pub last_colors: ColoredCmdline,
    /// Current command-line level (`level`).
    pub level: i32,
    /// Saved command-line state this one was entered from
    /// (`prev_ccline`).
    pub prev_ccline: *mut CmdlineInfo,
    /// Last `putcmdline` char, kept for redraws (`special_char`).
    pub special_char: u8,
    /// Shift state of the last `putcmdline` char (`special_shift`).
    pub special_shift: bool,
    /// Redraw still owed to an external command line
    /// (`redraw_state`).
    pub redraw_state: CmdRedraw,
    /// Return after one key press, for a button prompt (`one_key`).
    pub one_key: bool,
    /// Set when the mouse was clicked in the prompt (`mouse_used`).
    pub mouse_used: *mut bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_redraw_discriminants_match_the_original() {
        assert_eq!(CmdRedraw::None as i32, 0);
        assert_eq!(CmdRedraw::Pos as i32, 1);
        assert_eq!(CmdRedraw::All as i32, 2);
    }

    #[test]
    fn a_default_cmdline_info_is_empty_and_unlinked() {
        let ccline = CmdlineInfo::default();
        assert_eq!(ccline.cmdbuff, None);
        assert_eq!(ccline.cmdlen, 0);
        assert_eq!(ccline.cmdpos, 0);
        assert_eq!(ccline.cmdfirstc, 0, "NUL means no command-line type yet");
        assert_eq!(ccline.level, 0);
        assert!(ccline.xpc.is_null());
        assert!(ccline.prev_ccline.is_null());
        assert!(ccline.mouse_used.is_null());
        assert_eq!(ccline.redraw_state, CmdRedraw::None);
        assert!(!ccline.one_key);
    }

    #[test]
    fn colored_cmdline_starts_with_nothing_colored() {
        let c = ColoredCmdline::default();
        assert_eq!(c.prompt_id, 0);
        assert_eq!(c.cmdbuff, None);
        assert!(c.colors.is_empty());
    }

    #[test]
    fn a_color_chunk_carries_its_own_range_and_highlight() {
        let chunk = CmdlineColorChunk { start: 2, end: 5, hl_id: 7 };
        assert!(chunk.end > chunk.start, "the end is exclusive and past the start");
        assert_eq!(chunk.hl_id, 7);
    }
}
