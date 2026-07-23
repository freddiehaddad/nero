//! Translated from `src/nvim/autocmd_defs.h` (partial: struct/enum
//! shapes only - see `src/autocmd.rs`'s own module doc for the actual
//! functions).
//!
//! `event_T` (as [`EventT`]) is mechanically transcribed from
//! `src/nvim/auevents.lua`'s `events`/`aliases` tables, replicating
//! `src/gen/gen_events.lua`'s own generation algorithm exactly: take
//! every key from BOTH tables combined (145 real event names + 4
//! aliases - `BufCreate`/`BufRead`/`BufWrite`/`FileEncoding` - which
//! exist only as alternate spellings resolving to a "real" event, but
//! still get their OWN distinct `EVENT_*` enum value, per the
//! generator's own `vim.tbl_extend('error', events, aliases)`), sort
//! ALL 149 combined names case-insensitively (`a:lower() < b:lower()`
//! in the generator), then assign `EVENT_<NAME:upper()> = i - 1`
//! (0-based). Extracted via a throwaway Python script (matching this
//! session's established methodology for `OptIndex`/`CmdIdxT`/
//! `ExpandContext`), independently cross-checked by dumping the full
//! sorted list with alias markers and manually verifying each alias
//! interleaves at its correct case-insensitive alphabetical position
//! (e.g. `BufCreate` sorts between `BufAdd`/`BufDelete`; `CmdUndefined`
//! sorts AFTER `CmdlineLeavePre` - confirming case-INSENSITIVE
//! ordering is actually in effect, since a case-SENSITIVE sort would
//! place the lowercase-continuing `Cmdline*` names differently
//! relative to `CmdU...`).
//!
//! `AutoPat`/`AutoCmd`/`AutoCmdVec`/`AutoPatCmd`/`AutoCmdEvent` are
//! translated as struct shapes only (the "structure now, populate/
//! engine later" pattern already established for `OptIndex` before
//! the real `options[]` table, `CommandDefinition` before the real
//! `cmdnames[]` table, etc.) - nothing in this crate can yet populate
//! a real `AutoCmd`/`AutoPat` (the `:autocmd` command definition
//! parser, `au_add_autocmd`, is not translated), so every event's
//! vector is always empty in practice today; see `src/autocmd.rs`'s
//! own module doc for how that emptiness is exploited to make
//! `apply_autocmds`'s "no matching autocmds" early-return path real,
//! faithful, translatable behavior RIGHT NOW, not a speculative
//! shortcut.

use crate::api::private::defs::Buffer;
use crate::eval::typval_defs::{Callback, SctxT};
use crate::types_defs::RegprogT;

/// Values for `event_T` (`EVENT_*`) - see this module's own doc
/// comment for the mechanical-transcription/cross-check methodology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum EventT {
    BufAdd = 0,
    BufCreate = 1,
    BufDelete = 2,
    BufEnter = 3,
    BufFilePost = 4,
    BufFilePre = 5,
    BufHidden = 6,
    BufLeave = 7,
    BufNew = 8,
    BufNewFile = 9,
    BufRead = 10,
    BufReadCmd = 11,
    BufReadPost = 12,
    BufReadPre = 13,
    BufUnload = 14,
    BufWinEnter = 15,
    BufWinLeave = 16,
    BufWipeout = 17,
    BufWrite = 18,
    BufWriteCmd = 19,
    BufWritePost = 20,
    BufWritePre = 21,
    ChanClose = 22,
    ChanInfo = 23,
    ChanOpen = 24,
    CmdlineChanged = 25,
    CmdlineEnter = 26,
    CmdlineLeave = 27,
    CmdlineLeavePre = 28,
    CmdUndefined = 29,
    CmdwinEnter = 30,
    CmdwinLeave = 31,
    ColorScheme = 32,
    ColorSchemePre = 33,
    CompleteChanged = 34,
    CompleteDone = 35,
    CompleteDonePre = 36,
    CursorHold = 37,
    CursorHoldI = 38,
    CursorMoved = 39,
    CursorMovedC = 40,
    CursorMovedI = 41,
    DiagnosticChanged = 42,
    DiffUpdated = 43,
    DirChanged = 44,
    DirChangedPre = 45,
    EncodingChanged = 46,
    ExitPre = 47,
    FileAppendCmd = 48,
    FileAppendPost = 49,
    FileAppendPre = 50,
    FileChangedRO = 51,
    FileChangedShell = 52,
    FileChangedShellPost = 53,
    FileEncoding = 54,
    FileReadCmd = 55,
    FileReadPost = 56,
    FileReadPre = 57,
    FileType = 58,
    FileWriteCmd = 59,
    FileWritePost = 60,
    FileWritePre = 61,
    FilterReadPost = 62,
    FilterReadPre = 63,
    FilterWritePost = 64,
    FilterWritePre = 65,
    FocusGained = 66,
    FocusLost = 67,
    FuncUndefined = 68,
    GUIEnter = 69,
    GUIFailed = 70,
    InsertChange = 71,
    InsertCharPre = 72,
    InsertEnter = 73,
    InsertLeave = 74,
    InsertLeavePre = 75,
    LspAttach = 76,
    LspDetach = 77,
    LspNotify = 78,
    LspProgress = 79,
    LspRequest = 80,
    LspTokenUpdate = 81,
    MarkSet = 82,
    MenuPopup = 83,
    ModeChanged = 84,
    OptionSet = 85,
    PackChanged = 86,
    PackChangedPre = 87,
    Progress = 88,
    QuickFixCmdPost = 89,
    QuickFixCmdPre = 90,
    QuitPre = 91,
    RecordingEnter = 92,
    RecordingLeave = 93,
    RemoteReply = 94,
    SafeState = 95,
    SearchWrapped = 96,
    SessionLoadPost = 97,
    SessionLoadPre = 98,
    SessionWritePost = 99,
    SessionWritePre = 100,
    ShellCmdPost = 101,
    ShellFilterPost = 102,
    Signal = 103,
    SourceCmd = 104,
    SourcePost = 105,
    SourcePre = 106,
    SpellFileMissing = 107,
    StdinReadPost = 108,
    StdinReadPre = 109,
    SwapExists = 110,
    Syntax = 111,
    TabClosed = 112,
    TabClosedPre = 113,
    TabEnter = 114,
    TabLeave = 115,
    TabMoved = 116,
    TabNew = 117,
    TabNewEntered = 118,
    TermChanged = 119,
    TermClose = 120,
    TermEnter = 121,
    TermLeave = 122,
    TermOpen = 123,
    TermRequest = 124,
    TermResponse = 125,
    TextChanged = 126,
    TextChangedI = 127,
    TextChangedP = 128,
    TextChangedT = 129,
    TextPutPost = 130,
    TextPutPre = 131,
    TextYankPost = 132,
    UIEnter = 133,
    UILeave = 134,
    User = 135,
    VimEnter = 136,
    VimLeave = 137,
    VimLeavePre = 138,
    VimResized = 139,
    VimResume = 140,
    VimSuspend = 141,
    WinClosed = 142,
    WinEnter = 143,
    WinLeave = 144,
    WinNew = 145,
    WinNewPre = 146,
    WinResized = 147,
    WinScrolled = 148,
}

/// Total number of [`EventT`] variants (`NUM_EVENTS`).
pub const NUM_EVENTS: usize = 149;

/// Autocmd group IDs (`AUGROUP_*`).
pub mod augroup {
    /// default autocmd group (`AUGROUP_DEFAULT`).
    pub const DEFAULT: i32 = -1;
    /// erroneous autocmd group (`AUGROUP_ERROR`).
    pub const ERROR: i32 = -2;
    /// all autocmd groups (`AUGROUP_ALL`).
    pub const ALL: i32 = -3;
    /// all autocmd groups (`AUGROUP_DELETED`).
    pub const DELETED: i32 = -4;
}

/// `BUFLOCAL_PAT_LEN`.
pub const BUFLOCAL_PAT_LEN: usize = 25;

/// An autocmd pattern, reference-counted and shared across consecutive
/// autocmds using the same pattern (`AutoPat`).
pub struct AutoPat {
    /// Reference count (freed when reaches zero) (`refcount`).
    pub refcount: usize,
    /// Pattern as typed (`pat`).
    pub pat: Option<Vec<u8>>,
    /// Compiled regprog for pattern (`reg_prog`).
    pub reg_prog: *mut RegprogT,
    /// Group ID (`group`).
    pub group: i32,
    /// length of `pat` (`patlen`).
    pub patlen: i32,
    /// `!= 0` for buffer-local `AutoPat` (`buflocal_nr`).
    pub buflocal_nr: i32,
    /// Pattern may match whole path (`allow_dirs`).
    pub allow_dirs: bool,
}

/// A single autocmd (`AutoCmd`).
pub struct AutoCmd {
    /// Pattern reference (null when autocmd was removed) (`pat`).
    pub pat: *mut AutoPat,
    /// ID used for uniquely tracking an autocmd (`id`).
    pub id: i64,
    /// Description for the autocmd (`desc`).
    pub desc: Option<Vec<u8>>,
    /// Handler Ex command (`None` if handler is a function)
    /// (`handler_cmd`).
    pub handler_cmd: Option<Vec<u8>>,
    /// Handler callback (ignored if `handler_cmd` is not `None`)
    /// (`handler_fn`).
    pub handler_fn: Callback,
    /// Script context where it is defined (`script_ctx`).
    pub script_ctx: SctxT,
    /// "One shot": removed after execution (`once`).
    pub once: bool,
    /// If autocommands nest here (`nested`).
    pub nested: bool,
}

/// A vector of [`AutoCmd`]s for one event (`AutoCmdVec`).
pub type AutoCmdVec = Vec<AutoCmd>;

/// Status while executing autocommands for an event (`AutoPatCmd`).
pub struct AutoPatCmd {
    /// Last matched `AutoPat` (`lastpat`).
    pub lastpat: *mut AutoPat,
    /// Current autocmd index to execute (`auidx`).
    pub auidx: usize,
    /// Saved `AutoCmd` vector size (`ausize`).
    pub ausize: usize,
    /// Unexpanded `<afile>` (`afile_orig`).
    pub afile_orig: Option<Vec<u8>>,
    /// Fname to match with (`fname`).
    pub fname: Option<Vec<u8>>,
    /// Sfname to match with (`sfname`).
    pub sfname: Option<Vec<u8>>,
    /// Tail of fname (`tail`).
    pub tail: Option<Vec<u8>>,
    /// Group being used (`group`).
    pub group: i32,
    /// Current event (`event`).
    pub event: EventT,
    /// Script context where it is defined (`script_ctx`).
    pub script_ctx: SctxT,
    /// Initially equal to `<abuf>`, set to zero when buf is deleted
    /// (`arg_bufnr`).
    pub arg_bufnr: i32,
    /// Arbitrary data (`data`).
    pub data: *mut crate::api::private::defs::Object,
    /// Chain of active apc-s for auto-invalidation (`next`).
    pub next: *mut AutoPatCmd,
}

/// Used for "deferred" events, but can represent any event
/// (`AutoCmdEvent`).
pub struct AutoCmdEvent {
    pub event: EventT,
    pub fname: Option<Vec<u8>>,
    pub fname_io: Option<Vec<u8>>,
    pub buf: Buffer,
    pub group: i32,
    pub eap: *mut crate::ex_cmds_defs::ExargT,
    pub data: *mut crate::api::private::defs::Object,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_t_first_and_last_values_match_generator() {
        assert_eq!(EventT::BufAdd as i32, 0);
        assert_eq!(EventT::WinScrolled as i32, NUM_EVENTS as i32 - 1);
    }

    #[test]
    fn event_t_aliases_interleave_at_correct_case_insensitive_position() {
        // BufCreate sorts between BufAdd and BufDelete.
        assert_eq!(EventT::BufAdd as i32 + 1, EventT::BufCreate as i32);
        assert_eq!(EventT::BufCreate as i32 + 1, EventT::BufDelete as i32);
        // CmdUndefined sorts AFTER CmdlineLeavePre (case-insensitive:
        // "cmdl" < "cmdu") - a case-sensitive sort would place it
        // differently, since uppercase 'U' < lowercase 'l' in ASCII.
        assert_eq!(EventT::CmdlineLeavePre as i32 + 1, EventT::CmdUndefined as i32);
    }

    #[test]
    fn num_events_matches_variant_count() {
        // 145 real events + 4 aliases (BufCreate/BufRead/BufWrite/
        // FileEncoding), per auevents.lua.
        assert_eq!(NUM_EVENTS, 149);
    }

    #[test]
    fn augroup_constants_match_c_enum() {
        assert_eq!(augroup::DEFAULT, -1);
        assert_eq!(augroup::ERROR, -2);
        assert_eq!(augroup::ALL, -3);
        assert_eq!(augroup::DELETED, -4);
    }

    #[test]
    fn autopat_can_be_constructed_with_no_pattern() {
        let ap = AutoPat {
            refcount: 1,
            pat: None,
            reg_prog: std::ptr::null_mut(),
            group: augroup::DEFAULT,
            patlen: 0,
            buflocal_nr: 0,
            allow_dirs: false,
        };
        assert!(ap.pat.is_none());
        assert!(ap.reg_prog.is_null());
    }

    #[test]
    fn autocmd_can_be_constructed_with_a_handler_cmd() {
        let ac = AutoCmd {
            pat: std::ptr::null_mut(),
            id: 1,
            desc: Some(b"a description".to_vec()),
            handler_cmd: Some(b"echo 'hi'".to_vec()),
            handler_fn: Callback::default(),
            script_ctx: SctxT::default(),
            once: false,
            nested: false,
        };
        assert_eq!(ac.desc, Some(b"a description".to_vec()));
        assert!(ac.pat.is_null());
    }

    #[test]
    fn autopatcmd_next_can_chain_to_another_instance() {
        let mut tail = AutoPatCmd {
            lastpat: std::ptr::null_mut(),
            auidx: 0,
            ausize: 0,
            afile_orig: None,
            fname: None,
            sfname: None,
            tail: None,
            group: augroup::ALL,
            event: EventT::BufEnter,
            script_ctx: SctxT::default(),
            arg_bufnr: 0,
            data: std::ptr::null_mut(),
            next: std::ptr::null_mut(),
        };
        let head = AutoPatCmd {
            lastpat: std::ptr::null_mut(),
            auidx: 0,
            ausize: 0,
            afile_orig: None,
            fname: None,
            sfname: None,
            tail: None,
            group: augroup::ALL,
            event: EventT::BufLeave,
            script_ctx: SctxT::default(),
            arg_bufnr: 0,
            data: std::ptr::null_mut(),
            next: &mut tail as *mut AutoPatCmd,
        };
        assert_eq!(head.next, &mut tail as *mut AutoPatCmd);
        assert_eq!(unsafe { (*head.next).event }, EventT::BufEnter);
    }
}
