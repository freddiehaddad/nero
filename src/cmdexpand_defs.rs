//! Translated from `src/nvim/cmdexpand_defs.h`.
//!
//! Translated: `xp_prefix_T`, `EXPAND_BUF_LEN`, the `XP_BS_*` bit-flag
//! constants, the `EXPAND_*` "values for `xp_context`" family (as
//! [`ExpandContext`] - mechanically transcribed via a throwaway Python
//! script rather than hand-typed, the same technique already used for
//! `option_defs.rs`'s `OptIndex`/`ex_cmds_defs.rs`'s `CmdIdxT`;
//! verified via the script's own sanity assertions (71 total entries,
//! zero duplicate values) AND an independent, separately-derived
//! line-number-based cross-check for a handful of entries - including
//! correctly accounting for `EXPAND_DISASSEMBLE`, a commented-out
//! enum value in the original that does NOT consume a discriminant),
//! `expand_T` itself (as [`ExpandT`]), and `CompleteListItemGetter`.
//!
//! `expand_T` (used for cmdline completion state) needed no design
//! decisions beyond this crate's already-established conventions:
//! `xp_buf: [u8; EXPAND_BUF_LEN]` stays a fixed-size embedded array
//! (matching `globals.rs`'s own `IObuff`/`NameBuff` precedent for a
//! shared, address-stable scratch buffer, not a growable `Vec`);
//! `xp_files`/`xp_files_abbr`/`xp_files_kind`/`xp_files_menu`/
//! `xp_files_info` (the original's nullable parallel `char **` arrays)
//! become `Option<Vec<Vec<u8>>>`; `xp_shell` (conditionally compiled
//! out on Windows in the original, via `#ifndef BACKSLASH_IN_FILENAME`)
//! is always present here - this crate has no existing precedent for
//! platform-conditional *struct fields* (only platform-conditional
//! *tests*), and there is no reason a single extra `bool` needs to be
//! removed on one platform.

/// Whether/which prefix a boolean option's completion candidate uses
/// (`xp_prefix_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XpPrefixT {
    /// prefix not used (`XP_PREFIX_NONE`).
    #[default]
    None,
    /// `"no"` prefix for bool option (`XP_PREFIX_NO`).
    No,
    /// `"inv"` prefix for bool option (`XP_PREFIX_INV`).
    Inv,
}

/// Size of the scratch buffer for a single returned completion match
/// (`EXPAND_BUF_LEN`).
pub const EXPAND_BUF_LEN: usize = 256;

/// Values for `xp_backslash` (`XP_BS_*`).
pub mod xp_bs {
    /// nothing special for backslashes (`XP_BS_NONE`).
    pub const NONE: i32 = 0;
    /// uses one backslash before a space (`XP_BS_ONE`).
    pub const ONE: i32 = 0x1;
    /// uses three backslashes before a space (`XP_BS_THREE`).
    pub const THREE: i32 = 0x2;
    /// commas need to be escaped with a backslash (`XP_BS_COMMA`).
    pub const COMMA: i32 = 0x4;
}

/// Values for `xp_context` when doing command line completion
/// (`EXPAND_*`) - see this module's own doc comment for the
/// transcription/verification methodology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExpandContext {
    Unsuccessful = -2,
    Ok = -1,
    Nothing = 0,
    Commands = 1,
    Files = 2,
    Directories = 3,
    Settings = 4,
    BoolSettings = 5,
    Tags = 6,
    OldSetting = 7,
    Help = 8,
    Buffers = 9,
    Events = 10,
    Menus = 11,
    Syntax = 12,
    Highlight = 13,
    Augroup = 14,
    UserVars = 15,
    Mappings = 16,
    TagsListfiles = 17,
    Functions = 18,
    UserFunc = 19,
    Expression = 20,
    Menunames = 21,
    UserCommands = 22,
    UserCmdFlags = 23,
    UserNargs = 24,
    UserComplete = 25,
    EnvVars = 26,
    Language = 27,
    Colors = 28,
    Compiler = 29,
    UserDefined = 30,
    UserList = 31,
    UserLua = 32,
    Shellcmd = 33,
    Sign = 34,
    Profile = 35,
    Filetype = 36,
    FilesInPath = 37,
    Ownsyntax = 38,
    Locales = 39,
    History = 40,
    User = 41,
    Syntime = 42,
    UserAddrType = 43,
    Packadd = 44,
    Messages = 45,
    Mapclear = 46,
    Arglist = 47,
    DiffBuffers = 48,
    Breakpoint = 49,
    Scriptnames = 50,
    Runtime = 51,
    StringSetting = 52,
    SettingSubtract = 53,
    Argopt = 54,
    Keymap = 55,
    DirsInCdpath = 56,
    Shellcmdline = 57,
    Findfunc = 58,
    Filetypecmd = 59,
    PatternInBuf = 60,
    Retab = 61,
    Checkhealth = 62,
    Lua = 63,
    Lsp = 64,
    Log = 65,
    Packdel = 66,
    Packupdate = 67,
    Marks = 68,
}

/// Used for completion on the command line (`expand_T`).
pub struct ExpandT {
    /// start of item to expand, guaranteed to be part of `xp_line`
    /// (`xp_pattern`).
    pub xp_pattern: Option<Vec<u8>>,
    /// type of expansion (`xp_context`).
    pub xp_context: ExpandContext,
    /// bytes in `xp_pattern` before cursor (`xp_pattern_len`).
    pub xp_pattern_len: usize,
    pub xp_prefix: XpPrefixT,
    /// completion function (`xp_arg`).
    pub xp_arg: Option<Vec<u8>>,
    /// Ref to Lua completion function (`xp_luaref`).
    pub xp_luaref: crate::types_defs::LuaRef,
    /// SCTX for completion function (`xp_script_ctx`).
    pub xp_script_ctx: crate::eval::typval_defs::SctxT,
    /// one of the [`xp_bs`] values (`xp_backslash`).
    pub xp_backslash: i32,
    /// `true` for a shell command, more characters need to be escaped
    /// (`xp_shell`) - see this module's own doc comment for why this
    /// is always present, unlike the original's conditional
    /// compilation.
    pub xp_shell: bool,
    /// number of files found by file name completion (`xp_numfiles`).
    pub xp_numfiles: i32,
    /// cursor position in line (`xp_col`).
    pub xp_col: i32,
    /// selected index in completion (`xp_selected`).
    pub xp_selected: i32,
    /// originally expanded string (`xp_orig`).
    pub xp_orig: Option<Vec<u8>>,
    /// list of files (`xp_files`).
    pub xp_files: Option<Vec<Vec<u8>>>,
    /// optional parallel array of display strings (overrides
    /// `xp_files` for the pum text); `None` if unused (`xp_files_abbr`).
    pub xp_files_abbr: Option<Vec<Vec<u8>>>,
    /// optional parallel array of "kind" strings; `None` if unused
    /// (`xp_files_kind`).
    pub xp_files_kind: Option<Vec<Vec<u8>>>,
    /// optional parallel array of "menu" strings (shown after the
    /// match); `None` if unused (`xp_files_menu`).
    pub xp_files_menu: Option<Vec<Vec<u8>>>,
    /// optional parallel array of "info" strings (shown in info
    /// popup); `None` if unused (`xp_files_info`).
    pub xp_files_info: Option<Vec<Vec<u8>>>,
    /// text being completed (`xp_line`).
    pub xp_line: Option<Vec<u8>>,
    /// buffer for returned match (`xp_buf`).
    pub xp_buf: [u8; EXPAND_BUF_LEN],
    /// Direction of search (`xp_search_dir`).
    pub xp_search_dir: crate::vim_defs::Direction,
    /// Cursor position before incsearch (`xp_pre_incsearch_pos`).
    pub xp_pre_incsearch_pos: crate::pos_defs::PosT,
}

impl Default for ExpandT {
    fn default() -> Self {
        ExpandT {
            xp_pattern: None,
            xp_context: ExpandContext::Nothing,
            xp_pattern_len: 0,
            xp_prefix: XpPrefixT::None,
            xp_arg: None,
            xp_luaref: -1,
            xp_script_ctx: crate::eval::typval_defs::SctxT::default(),
            xp_backslash: xp_bs::NONE,
            xp_shell: false,
            xp_numfiles: 0,
            xp_col: 0,
            xp_selected: 0,
            xp_orig: None,
            xp_files: None,
            xp_files_abbr: None,
            xp_files_kind: None,
            xp_files_menu: None,
            xp_files_info: None,
            xp_line: None,
            xp_buf: [0; EXPAND_BUF_LEN],
            xp_search_dir: crate::vim_defs::Direction::Forward,
            xp_pre_incsearch_pos: crate::pos_defs::PosT::default(),
        }
    }
}

/// Type used by `ExpandGeneric()` (not yet translated)
/// (`CompleteListItemGetter`).
pub type CompleteListItemGetter = fn(xp: &ExpandT, idx: i32) -> Option<Vec<u8>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xp_prefix_default_is_none() {
        assert_eq!(XpPrefixT::default(), XpPrefixT::None);
    }

    #[test]
    fn xp_bs_flags_are_distinct_bits() {
        assert_eq!(xp_bs::NONE, 0);
        assert_eq!(xp_bs::ONE, 1);
        assert_eq!(xp_bs::THREE, 2);
        assert_eq!(xp_bs::COMMA, 4);
    }

    #[test]
    fn expand_context_special_sentinels_match_c_macros() {
        assert_eq!(ExpandContext::Unsuccessful as i32, -2);
        assert_eq!(ExpandContext::Ok as i32, -1);
        assert_eq!(ExpandContext::Nothing as i32, 0);
    }

    #[test]
    fn expand_context_first_and_last_real_values_match_c_enum() {
        assert_eq!(ExpandContext::Commands as i32, 1);
        assert_eq!(ExpandContext::Marks as i32, 68);
    }

    #[test]
    fn expand_context_accounts_for_the_commented_out_disassemble_entry() {
        // EXPAND_DISASSEMBLE is commented out in the original right
        // between EXPAND_DIFF_BUFFERS and EXPAND_BREAKPOINT - it must
        // NOT consume a discriminant value.
        assert_eq!(ExpandContext::DiffBuffers as i32, 48);
        assert_eq!(ExpandContext::Breakpoint as i32, 49);
    }

    #[test]
    fn expand_default_is_zeroed_with_nothing_context_and_empty_buffers() {
        let xp = ExpandT::default();
        assert!(xp.xp_pattern.is_none());
        assert_eq!(xp.xp_context, ExpandContext::Nothing);
        assert_eq!(xp.xp_pattern_len, 0);
        assert_eq!(xp.xp_prefix, XpPrefixT::None);
        assert!(xp.xp_arg.is_none());
        assert_eq!(xp.xp_luaref, -1);
        assert_eq!(xp.xp_backslash, xp_bs::NONE);
        assert!(!xp.xp_shell);
        assert_eq!(xp.xp_numfiles, 0);
        assert!(xp.xp_files.is_none());
        assert!(xp.xp_files_abbr.is_none());
        assert_eq!(xp.xp_buf.len(), EXPAND_BUF_LEN);
        assert!(xp.xp_buf.iter().all(|&b| b == 0));
        assert_eq!(xp.xp_search_dir, crate::vim_defs::Direction::Forward);
        assert_eq!(xp.xp_pre_incsearch_pos, crate::pos_defs::PosT::default());
    }

    #[test]
    fn complete_list_item_getter_can_be_stored_and_called() {
        fn getter(_xp: &ExpandT, idx: i32) -> Option<Vec<u8>> {
            if idx == 0 {
                Some(b"first".to_vec())
            } else {
                None
            }
        }
        let f: CompleteListItemGetter = getter;
        let xp = ExpandT::default();
        assert_eq!(f(&xp, 0), Some(b"first".to_vec()));
        assert_eq!(f(&xp, 1), None);
    }
}
