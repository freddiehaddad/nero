//! Translated from `src/nvim/ex_cmds_defs.h` (partial).
//!
//! Translated: the `EX_*`/`BAD_*`/`FORCE_*`/`EXFLAG_*`/`CMOD_*`
//! constants, `cmd_addr_T`, `cmdmod_T`, `SubReplacementString`.
//!
//! Deferred: `exarg_T`/`CommandDefinition`/`ex_func_T`/
//! `ex_preview_func_T`/`LineGetter` need `cmdidx_T`, which
//! `ex_cmds_enum.generated.h` generates from `src/nvim/ex_cmds.lua`'s
//! master Ex-command table (~500 entries) - a real codegen-table
//! translation job of its own (this project's item 5 of "confirmed
//! decisions": translate the generator's *output*, not build a codegen
//! pipeline), not yet done. `cmdmod_T.cmod_filter_regmatch` uses the new
//! `crate::types_defs::RegmatchT` opaque placeholder (`regexp_defs.h`,
//! phase 7) rather than waiting for the full regexp engine.

use crate::os::time_defs::Timestamp;
use crate::types_defs::{AdditionalData, OptInt, RegmatchT};

/// Flags for `CommandDefinition`/`exarg_T.argt` (kept as plain `u32`
/// bit-flag constants, same reasoning as `HL_*`/`MarkMoveRes` elsewhere
/// in this crate).
pub mod ex_flags {
    /// allow a linespecs
    pub const RANGE: u32 = 0x001;
    /// allow a ! after the command name
    pub const BANG: u32 = 0x002;
    /// allow extra args after command name
    pub const EXTRA: u32 = 0x004;
    /// expand wildcards in extra part
    pub const XFILE: u32 = 0x008;
    /// extra part is a single argument (no split on whitespace)
    pub const NOSPC: u32 = 0x010;
    /// default file range is 1,$
    pub const DFLALL: u32 = 0x020;
    /// extend range to include whole fold also when less than two
    /// numbers given
    pub const WHOLEFOLD: u32 = 0x040;
    /// argument required
    pub const NEEDARG: u32 = 0x080;
    /// check for trailing vertical bar
    pub const TRLBAR: u32 = 0x100;
    /// allow "x for register designation
    pub const REGSTR: u32 = 0x200;
    /// allow count in argument, after command
    pub const COUNT: u32 = 0x400;
    /// no trailing comment allowed
    pub const NOTRLCOM: u32 = 0x800;
    /// zero line number allowed
    pub const ZEROR: u32 = 0x1000;
    /// do not remove CTRL-V from argument
    pub const CTRLV: u32 = 0x2000;
    /// allow "+command" argument
    pub const CMDARG: u32 = 0x4000;
    /// accepts buffer name
    pub const BUFNAME: u32 = 0x8000;
    /// accepts unlisted buffer too
    pub const BUFUNL: u32 = 0x10000;
    /// allow "++opt=val" argument
    pub const ARGOPT: u32 = 0x20000;
    /// allowed in the sandbox
    pub const SBOXOK: u32 = 0x40000;
    /// Command is allowed when curbuf is `b_ro_locked` (e.g. during a
    /// quickfix or diff critical section). Legacy name: `EX_CMDWIN`.
    /// Implies [`LOCK_OK`].
    pub const BUFLOCK_OK: u32 = 0x80000;
    /// forbidden in non-`'modifiable'` buffer
    pub const MODIFY: u32 = 0x100000;
    /// allow flags after count in argument
    pub const FLAGS: u32 = 0x200000;
    /// Command allowed when `textlock` is set. `BUFLOCK_OK` is
    /// per-buffer.
    pub const LOCK_OK: u32 = 0x1000000;
    /// keep sctx of where command was invoked
    pub const KEEPSCRIPT: u32 = 0x4000000;
    /// allow incremental command preview
    pub const PREVIEW: u32 = 0x8000000;
    /// completion: keep spaces in arg lead
    pub const ARGSPACE: u32 = 0x40000000;
    /// multiple extra files allowed
    pub const FILES: u32 = XFILE | EXTRA;
    /// 1 file, defaults to current file
    pub const FILE1: u32 = FILES | NOSPC;
    /// one extra word allowed
    pub const WORD1: u32 = EXTRA | NOSPC;
}

/// Values for `cmd_addr_type` (`cmd_addr_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CmdAddrT {
    /// buffer line numbers
    #[default]
    Lines,
    /// window number
    Windows,
    /// argument number
    Arguments,
    /// buffer number of loaded buffer
    LoadedBuffers,
    /// buffer number
    Buffers,
    /// tab page number
    Tabs,
    /// Tab page that only relative
    TabsRelative,
    /// quickfix list valid entry number
    QuickfixValid,
    /// quickfix list entry number
    Quickfix,
    /// positive count or zero, defaults to 1
    Unsigned,
    /// something else, use line number for '$', '%', etc.
    Other,
    /// no range used
    None,
}

// Behavior for bad character, "++bad=" argument.
/// replace it with '?' (default) (`BAD_REPLACE`).
pub const BAD_REPLACE: u8 = b'?';
/// leave it (`BAD_KEEP`).
pub const BAD_KEEP: i32 = -1;
/// erase it (`BAD_DROP`).
pub const BAD_DROP: i32 = -2;

/// `":edit ++bin file"` (`FORCE_BIN`).
pub const FORCE_BIN: i32 = 1;
/// `":edit ++nobin file"` (`FORCE_NOBIN`).
pub const FORCE_NOBIN: i32 = 2;

/// Values for `exarg_T.flags` (`EXFLAG_*`).
pub mod exflag {
    /// 'l': list
    pub const LIST: i32 = 0x01;
    /// '#': number
    pub const NR: i32 = 0x02;
    /// 'p': print
    pub const PRINT: i32 = 0x04;
}

/// Command modifier flags for [`CmdmodT.cmod_flags`](CmdmodT::cmod_flags)
/// (`CMOD_*`).
pub mod cmod {
    /// `":sandbox"`
    pub const SANDBOX: i32 = 0x0001;
    /// `":silent"`
    pub const SILENT: i32 = 0x0002;
    /// `":silent!"`
    pub const ERRSILENT: i32 = 0x0004;
    /// `":unsilent"`
    pub const UNSILENT: i32 = 0x0008;
    /// `":noautocmd"`
    pub const NOAUTOCMD: i32 = 0x0010;
    /// `":hide"`
    pub const HIDE: i32 = 0x0020;
    /// `":browse"` - invoke file dialog
    pub const BROWSE: i32 = 0x0040;
    /// `":confirm"` - invoke yes/no dialog
    pub const CONFIRM: i32 = 0x0080;
    /// `":keepalt"`
    pub const KEEPALT: i32 = 0x0100;
    /// `":keepmarks"`
    pub const KEEPMARKS: i32 = 0x0200;
    /// `":keepjumps"`
    pub const KEEPJUMPS: i32 = 0x0400;
    /// `":lockmarks"`
    pub const LOCKMARKS: i32 = 0x0800;
    /// `":keeppatterns"`
    pub const KEEPPATTERNS: i32 = 0x1000;
    /// `":noswapfile"`
    pub const NOSWAPFILE: i32 = 0x2000;
}

/// Command modifiers `":vertical"`, `":browse"`, `":confirm"`, `":hide"`,
/// etc. set a flag. This needs to be saved for recursive commands, put
/// them in a structure for easy manipulation (`cmdmod_T`).
#[derive(Debug, Clone, Default)]
pub struct CmdmodT {
    /// `CMOD_*` flags
    pub cmod_flags: i32,

    /// flags for `win_split()`
    pub cmod_split: i32,
    /// > 0 when `":tab"` was used
    pub cmod_tab: i32,
    pub cmod_filter_pat: Option<Vec<u8>>,
    /// set by `:filter /pat/`
    pub cmod_filter_regmatch: RegmatchT,
    /// set for `:filter!`
    pub cmod_filter_force: bool,

    /// 0 if not set, > 0 to set `'verbose'` to `cmod_verbose - 1`
    pub cmod_verbose: i32,

    // Values for undo_cmdmod() (not yet translated).
    /// saved value of `'eventignore'`
    pub cmod_save_ei: Option<Vec<u8>>,
    /// set when "sandbox" was incremented
    pub cmod_did_sandbox: i32,
    /// if `'verbose'` was set: value of `p_verbose` plus one
    pub cmod_verbose_save: OptInt,
    /// if non-zero: saved value of `msg_silent + 1`
    pub cmod_save_msg_silent: i32,
    /// for restoring `msg_scroll`
    pub cmod_save_msg_scroll: i32,
    /// incremented when `emsg_silent` is (comment is truncated
    /// mid-sentence in the original C source too - not a translation
    /// error here).
    pub cmod_did_esilent: i32,
}

/// Previous `:substitute` replacement string definition
/// (`SubReplacementString`).
#[derive(Debug, Clone, Default)]
pub struct SubReplacementString {
    /// Previous replacement string.
    pub sub: Option<Vec<u8>>,
    /// Time when it was last set.
    pub timestamp: Timestamp,
    /// Additional data left from ShaDa file.
    pub additional_data: Option<Box<AdditionalData>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ex_files_combines_xfile_and_extra() {
        assert_eq!(ex_flags::FILES, ex_flags::XFILE | ex_flags::EXTRA);
        assert_eq!(ex_flags::FILE1, ex_flags::FILES | ex_flags::NOSPC);
        assert_eq!(ex_flags::WORD1, ex_flags::EXTRA | ex_flags::NOSPC);
    }

    #[test]
    fn cmd_addr_default_is_lines() {
        assert_eq!(CmdAddrT::default(), CmdAddrT::Lines);
    }

    #[test]
    fn bad_char_constants_match_c_macros() {
        assert_eq!(BAD_REPLACE, b'?');
        assert_eq!(BAD_KEEP, -1);
        assert_eq!(BAD_DROP, -2);
    }

    #[test]
    fn cmdmod_default_is_zeroed() {
        let cm = CmdmodT::default();
        assert_eq!(cm.cmod_flags, 0);
        assert!(cm.cmod_filter_pat.is_none());
        assert!(!cm.cmod_filter_force);
    }

    #[test]
    fn sub_replacement_string_default_has_no_previous_sub() {
        let s = SubReplacementString::default();
        assert!(s.sub.is_none());
        assert!(s.additional_data.is_none());
    }
}
