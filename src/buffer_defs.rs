//! Translated from `src/nvim/buffer_defs.h` (partial - `buf_T`/`win_T`
//! themselves (`struct file_buffer`/`struct window_S`, each several
//! hundred lines and referencing memline internals, quickfix state, etc.
//! not yet translated) are substantial and deliberately deferred to a
//! dedicated pass rather than rushed).
//!
//! Translated: `bufref_T`, the `VALID_*`/`BF_*` bit-flag constants,
//! `disptick_T`, `taggy_T`, `winopt_T`, `WinInfo` (`struct wininfo_S`),
//! `syn_time_T`, `synblock_T`, `BufUpdateCallbacks`, and the
//! `BUF_HAS_*`/`MAX_MAPHASH` constants.
//!
//! Deferred: everything referencing `buf_T`'s actual fields (`struct
//! file_buffer` itself, `ChangedtickDictItem` which needs the eval
//! engine's `typval_T`), `file_buffer`/`window_S` themselves, `tabpage_S`,
//! `frame_S`, and all the window-layout/diff/match/float-config types
//! further down the original file.

use crate::eval::typval_defs::SctxT;
use crate::garray_defs::GarrayT;
use crate::hashtab_defs::HashtabT;
use crate::mark_defs::FmarkT;
use crate::types_defs::{BufT, LuaRef, OptInt, ProftimeT, WinT};

/// Reference to a buffer that stores the value of `buf_free_count`.
/// `bufref_valid()` (not yet translated) only needs to check `buf` when the
/// count differs (`bufref_T`).
#[derive(Debug, Clone, Copy)]
pub struct BufrefT {
    pub br_buf: *mut BufT,
    pub br_fnum: i32,
    pub br_buf_free_count: i32,
}

/// `GETFILE_SUCCESS(x)`
#[inline]
pub fn getfile_success(x: i32) -> bool {
    x <= 0
}

/// Flags for `w_valid` (kept as plain `u8` bit-flag constants, same
/// reasoning as `HL_*`/`MarkMoveRes` elsewhere in this crate).
///
/// These are set when something in a window structure becomes invalid,
/// except when the cursor is moved. Callers must call `check_cursor_moved()`
/// (not yet translated) before testing one of the flags. These are reset
/// when that thing has been updated and is valid again.
///
/// Every function that invalidates one of these must call one of the
/// `invalidate_*` functions (not yet translated).
///
/// `w_valid` is supposed to be used only in `drawscreen.c`. From other
/// files, use the functions that set or reset the flags.
///
/// ```text
/// VALID_BOTLINE    VALID_BOTLINE_AP
///     on       on      w_botline valid
///     off      on      w_botline approximated
///     off      off     w_botline not valid
///     on       off     not possible
/// ```
pub mod w_valid {
    /// `w_wrow` (window row) is valid
    pub const VALID_WROW: u8 = 0x01;
    /// `w_wcol` (window col) is valid
    pub const VALID_WCOL: u8 = 0x02;
    /// `w_virtcol` (file col) is valid
    pub const VALID_VIRTCOL: u8 = 0x04;
    /// `w_cline_height` and `w_cline_folded` valid
    pub const VALID_CHEIGHT: u8 = 0x08;
    /// `w_cline_row` is valid
    pub const VALID_CROW: u8 = 0x10;
    /// `w_botline` and `w_empty_rows` are valid
    pub const VALID_BOTLINE: u8 = 0x20;
    /// `w_botline` is approximated
    pub const VALID_BOTLINE_AP: u8 = 0x40;
    /// `w_topline` is valid (for cursor position)
    pub const VALID_TOPLINE: u8 = 0x80;
}

/// flags for `b_flags` (kept as plain `u32` bit-flag constants).
pub mod b_flags {
    /// buffer has been recovered
    pub const BF_RECOVERED: u32 = 0x01;
    /// need to check readonly when loading file into buffer (set by `":e"`,
    /// may be reset by `":buf"`)
    pub const BF_CHECK_RO: u32 = 0x02;
    /// file has never been loaded into buffer, many variables still need
    /// to be set
    pub const BF_NEVERLOADED: u32 = 0x04;
    /// Set when file name is changed after starting to edit, reset when
    /// file is written out.
    pub const BF_NOTEDITED: u32 = 0x08;
    /// file didn't exist when editing started
    pub const BF_NEW: u32 = 0x10;
    /// Warned for `BF_NEW` and file created
    pub const BF_NEW_W: u32 = 0x20;
    /// got errors while reading the file
    pub const BF_READERR: u32 = 0x40;
    /// dummy buffer, only used internally
    pub const BF_DUMMY: u32 = 0x80;
    /// `'syntax'` option was set
    pub const BF_SYN_SET: u32 = 0x200;

    /// Mask to check for flags that prevent normal writing (`BF_WRITE_MASK`).
    pub const BF_WRITE_MASK: u32 = BF_NOTEDITED + BF_NEW + BF_READERR;
}

/// display tick type (`disptick_T`)
pub type DisptickT = u64;

/// Used to store the information about a `:tag` command (`taggy_T`).
#[derive(Debug, Clone)]
pub struct TaggyT {
    /// tag name
    pub tagname: Vec<u8>,
    /// cursor position BEFORE `":tag"`
    pub fmark: FmarkT,
    /// match number
    pub cur_match: i32,
    /// buffer number used for `cur_match`
    pub cur_fnum: i32,
    /// used with `'tagfunc'`
    pub user_data: Option<Vec<u8>>,
}

/// Structure that contains all options that are local to a window
/// (`winopt_T`). Used twice in a window: for the current buffer and for
/// all buffers. Also used in [`WinInfo`].
///
/// The original's `#define w_p_arab w_onebuf_opt.wo_arab`-style aliases
/// (one per field, for use as `win->w_p_arab` once embedded in
/// `window_S`) are not translated here: they only make sense once
/// `window_S`/`WinT` itself exists (`buffer_defs.h`'s `struct window_S`,
/// not yet translated) to embed this struct and give the alias a home.
#[derive(Debug, Clone, Default)]
pub struct WinoptT {
    /// `'arabic'`
    pub wo_arab: i32,
    /// `'breakindent'`
    pub wo_bri: i32,
    /// `'breakindentopt'`
    pub wo_briopt: Option<Vec<u8>>,
    /// `'diff'`
    pub wo_diff: i32,
    /// `'foldcolumn'`
    pub wo_fdc: Option<Vec<u8>>,
    /// `'eventignorewin'`
    pub wo_eiw: Option<Vec<u8>>,
    /// `'fdc'` saved for diff mode
    pub wo_fdc_save: Option<Vec<u8>>,
    /// `'foldenable'`
    pub wo_fen: i32,
    /// `'foldenable'` saved for diff mode
    pub wo_fen_save: i32,
    /// `'foldignore'`
    pub wo_fdi: Option<Vec<u8>>,
    /// `'foldlevel'`
    pub wo_fdl: OptInt,
    /// `'foldlevel'` state saved for diff mode
    pub wo_fdl_save: OptInt,
    /// `'foldmethod'`
    pub wo_fdm: Option<Vec<u8>>,
    /// `'fdm'` saved for diff mode
    pub wo_fdm_save: Option<Vec<u8>>,
    /// `'foldminlines'`
    pub wo_fml: OptInt,
    /// `'foldnestmax'`
    pub wo_fdn: OptInt,
    /// `'foldexpr'`
    pub wo_fde: Option<Vec<u8>>,
    /// `'foldtext'`
    pub wo_fdt: Option<Vec<u8>>,
    /// `'foldmarker'`
    pub wo_fmr: Option<Vec<u8>>,
    /// `'linebreak'`
    pub wo_lbr: i32,
    /// `'list'`
    pub wo_list: i32,
    /// `'number'`
    pub wo_nu: i32,
    /// `'relativenumber'`
    pub wo_rnu: i32,
    /// `'virtualedit'`
    pub wo_ve: Option<Vec<u8>>,
    /// flags for `'virtualedit'`
    pub wo_ve_flags: u32,
    /// `'numberwidth'`
    pub wo_nuw: OptInt,
    /// `'winfixbuf'`
    pub wo_wfb: i32,
    /// `'winfixheight'`
    pub wo_wfh: i32,
    /// `'winfixwidth'`
    pub wo_wfw: i32,
    /// `'winpinned'`
    pub wo_wp: i32,
    /// `'previewwindow'`
    pub wo_pvw: i32,
    /// `'lhistory'`
    pub wo_lhi: OptInt,
    /// `'rightleft'`
    pub wo_rl: i32,
    /// `'rightleftcmd'`
    pub wo_rlc: Option<Vec<u8>>,
    /// `'scroll'`
    pub wo_scr: OptInt,
    /// `'smoothscroll'`
    pub wo_sms: i32,
    /// `'spell'`
    pub wo_spell: i32,
    /// `'cursorcolumn'`
    pub wo_cuc: i32,
    /// `'cursorline'`
    pub wo_cul: i32,
    /// `'cursorlineopt'`
    pub wo_culopt: Option<Vec<u8>>,
    /// `'colorcolumn'`
    pub wo_cc: Option<Vec<u8>>,
    /// `'showbreak'`
    pub wo_sbr: Option<Vec<u8>>,
    /// `'statuscolumn'`
    pub wo_stc: Option<Vec<u8>>,
    /// `'statusline'`
    pub wo_stl: Option<Vec<u8>>,
    /// `'winbar'`
    pub wo_wbr: Option<Vec<u8>>,
    /// `'scrollbind'`
    pub wo_scb: i32,
    /// options were saved for starting diff mode
    pub wo_diff_saved: i32,
    /// `'scrollbind'` saved for diff mode
    pub wo_scb_save: i32,
    /// `'wrap'`
    pub wo_wrap: i32,
    /// `'wrap'` state saved for diff mode
    pub wo_wrap_save: i32,
    /// `'concealcursor'`
    pub wo_cocu: Option<Vec<u8>>,
    /// `'conceallevel'`
    pub wo_cole: OptInt,
    /// `'cursorbind'`
    pub wo_crb: i32,
    /// `'cursorbind'` state saved for diff mode
    pub wo_crb_save: i32,
    /// `'signcolumn'`
    pub wo_scl: Option<Vec<u8>>,
    /// `'sidescrolloff'` local value
    pub wo_siso: OptInt,
    /// `'scrolloff'` local value
    pub wo_so: OptInt,
    /// `'scrolloffpad'` local value
    pub wo_sop: OptInt,
    /// `'winhighlight'`
    pub wo_winhl: Option<Vec<u8>>,
    /// `'listchars'`
    pub wo_lcs: Option<Vec<u8>>,
    /// `'fillchars'`
    pub wo_fcs: Option<Vec<u8>>,
    /// `'winblend'`
    pub wo_winbl: OptInt,
    /// flags for `'wrap'` (a few options have local flags for
    /// `kOptFlagInsecure`)
    pub wo_wrap_flags: u32,
    /// flags for `'statusline'`
    pub wo_stl_flags: u32,
    /// flags for `'winbar'`
    pub wo_wbr_flags: u32,
    /// flags for `'foldexpr'`
    pub wo_fde_flags: u32,
    /// flags for `'foldtext'`
    pub wo_fdt_flags: u32,
    /// SCTXs for window-local options (`sctx_T wo_script_ctx[kWinOptCount]`
    /// in the original). `kWinOptCount` is a codegen-derived constant (the
    /// number of window-local options in the master options table,
    /// `src/gen/*.lua`) that isn't available yet (the "codegen concern"
    /// flagged and deferred since phase 1) - a growable `Vec` stands in
    /// for the original's fixed-size array until that's resolved, since
    /// nothing here depends on it being a fixed size rather than however
    /// many entries happen to be pushed.
    pub wo_script_ctx: Vec<SctxT>,
}

/// Window info stored with a buffer (`struct wininfo_S`, typedef'd as
/// `WinInfo`).
///
/// Two types of info are kept for a buffer which are associated with a
/// specific window: (1) each window can have a different line number
/// associated with a buffer; (2) the window-local options for a buffer
/// work in a similar way. The window-info is kept in a list at
/// `b_wininfo` (`file_buffer`, not yet translated). It is kept in
/// most-recently-used order.
#[derive(Debug, Clone, Default)]
pub struct WinInfo {
    /// pointer to window that did set `wi_mark`
    pub wi_win: *mut WinT,
    /// last cursor mark in the file
    pub wi_mark: FmarkT,
    /// true when `wi_opt` has useful values
    pub wi_optset: bool,
    /// local window options
    pub wi_opt: WinoptT,
    /// copy of `w_fold_manual`
    pub wi_fold_manual: bool,
    /// clone of `w_folds`
    pub wi_folds: GarrayT,
    /// copy of `w_changelistidx`
    pub wi_changelistidx: i32,
}

/// Used for `:syntime`: timing of executing a syntax pattern
/// (`syn_time_T`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SynTimeT {
    /// total time used
    pub total: ProftimeT,
    /// time of slowest call
    pub slowest: ProftimeT,
    /// nr of times used
    pub count: i32,
    /// nr of times matched
    pub match_: i32,
}

/// These are items normally related to a buffer. But when using
/// `":ownsyntax"` a window may have its own instance (`synblock_T`).
///
/// Pointer fields to not-yet-translated types (`regprog_T`, `synstate_T`)
/// stay raw pointers, matching this crate's established convention for
/// opaque forward-declared types (see `types_defs.rs`'s placeholder
/// list) - they become safely typed once those owning files are
/// translated (phase 7/8).
pub struct SynblockT {
    /// syntax keywords hash table
    pub b_keywtab: HashtabT,
    /// idem, ignore case
    pub b_keywtab_ic: HashtabT,
    /// true when error occurred in HL
    pub b_syn_error: bool,
    /// true when `'redrawtime'` reached
    pub b_syn_slow: bool,
    /// ignore case for `:syn` cmds
    pub b_syn_ic: i32,
    /// how to compute foldlevel on a line
    pub b_syn_foldlevel: i32,
    /// `SYNSPL_*` values
    pub b_syn_spell: i32,
    /// table for syntax patterns
    pub b_syn_patterns: GarrayT,
    /// table for syntax clusters
    pub b_syn_clusters: GarrayT,
    /// `@Spell` cluster ID or 0
    pub b_spell_cluster_id: i32,
    /// `@NoSpell` cluster ID or 0
    pub b_nospell_cluster_id: i32,
    /// true when there is an item with a `"containedin"` argument
    pub b_syn_containedin: i32,
    /// flags about how to sync
    pub b_syn_sync_flags: i32,
    /// group to sync on
    pub b_syn_sync_id: i16,
    /// minimal sync lines offset
    pub b_syn_sync_minlines: crate::pos_defs::LinenrT,
    /// maximal sync lines offset
    pub b_syn_sync_maxlines: crate::pos_defs::LinenrT,
    /// offset for multi-line pattern
    pub b_syn_sync_linebreaks: crate::pos_defs::LinenrT,
    /// line continuation pattern
    pub b_syn_linecont_pat: Option<Vec<u8>>,
    /// line continuation program
    pub b_syn_linecont_prog: *mut crate::types_defs::RegprogT,
    pub b_syn_linecont_time: SynTimeT,
    /// ignore-case flag for `b_syn_linecont_pat`
    pub b_syn_linecont_ic: i32,
    /// for `":syntax include"`
    pub b_syn_topgrp: i32,
    /// auto-conceal for `:syn` cmds
    pub b_syn_conceal: i32,
    /// number of patterns with the `HL_FOLD` flag set
    pub b_syn_folditems: i32,

    // b_sst_array[] contains the state stack for a number of lines, for
    // the start of that line (col == 0). This avoids having to recompute
    // the syntax state too often. It is allocated to hold the state for
    // all displayed lines, and states for 1 out of about 20 other lines.
    /// pointer to an array of `synstate_T`
    pub b_sst_array: *mut crate::types_defs::SynstateT,
    /// number of entries in `b_sst_array[]`
    pub b_sst_len: i32,
    /// pointer to first used entry in `b_sst_array[]` or null
    pub b_sst_first: *mut crate::types_defs::SynstateT,
    /// pointer to first free entry in `b_sst_array[]` or null
    pub b_sst_firstfree: *mut crate::types_defs::SynstateT,
    /// number of free entries
    pub b_sst_freecount: i32,
    /// entries after this lnum need to be checked for validity (`MAXLNUM`
    /// means no check needed)
    pub b_sst_check_lnum: crate::pos_defs::LinenrT,
    /// last display tick
    pub b_sst_lasttick: DisptickT,

    /// Cache for `in_id_list()`; see `idl_cache_T` in `syntax.c`.
    pub b_idlist_cache: *mut std::ffi::c_void,

    // for spell checking
    /// list of pointers to `slang_T`, see `spell.c`
    pub b_langp: GarrayT,
    /// flags: is midword char
    pub b_spell_ismw: [bool; 256],
    /// multi-byte midword chars
    pub b_spell_ismw_mb: Option<Vec<u8>>,
    /// `'spellcapcheck'`
    pub b_p_spc: Option<Vec<u8>>,
    /// program for `'spellcapcheck'`
    pub b_cap_prog: *mut crate::types_defs::RegprogT,
    /// `'spellfile'`
    pub b_p_spf: Option<Vec<u8>>,
    /// `'spelllang'`
    pub b_p_spl: Option<Vec<u8>>,
    /// `'spelloptions'`
    pub b_p_spo: Option<Vec<u8>>,
    /// `'spelloptions'` flags
    pub b_p_spo_flags: u32,
    /// all CJK letters as OK
    pub b_cjk: i32,
    /// syntax `'iskeyword'` option
    pub b_syn_chartab: [u8; 32],
    /// `'iskeyword'` option
    pub b_syn_isk: Option<Vec<u8>>,
}

/// Callbacks registered via `nvim_buf_attach` (`BufUpdateCallbacks`).
#[derive(Debug, Clone, Copy)]
pub struct BufUpdateCallbacks {
    pub on_lines: LuaRef,
    pub on_bytes: LuaRef,
    pub on_changedtick: LuaRef,
    pub on_detach: LuaRef,
    pub on_reload: LuaRef,
    pub utf_sizes: bool,
    pub preview: bool,
}

impl Default for BufUpdateCallbacks {
    /// `BUF_UPDATE_CALLBACKS_INIT`
    fn default() -> Self {
        // `LUA_NOREF`: represents a missing Lua reference (see `types_defs.rs`'s
        // doc comment on `LuaRef`), matching the local-const convention
        // already used for this same constant in `decoration_defs.rs`.
        const LUA_NOREF: LuaRef = -1;
        BufUpdateCallbacks {
            on_lines: LUA_NOREF,
            on_bytes: LUA_NOREF,
            on_changedtick: LUA_NOREF,
            on_detach: LUA_NOREF,
            on_reload: LUA_NOREF,
            utf_sizes: false,
            preview: false,
        }
    }
}

pub const BUF_HAS_QF_ENTRY: i32 = 1;
pub const BUF_HAS_LL_ENTRY: i32 = 2;

/// Maximum number of maphash blocks we will have (`MAX_MAPHASH`).
pub const MAX_MAPHASH: i32 = 256;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getfile_success_matches_c_macro() {
        assert!(getfile_success(0));
        assert!(getfile_success(-1));
        assert!(!getfile_success(1));
    }

    #[test]
    fn bf_write_mask_combines_expected_flags() {
        assert_eq!(
            b_flags::BF_WRITE_MASK,
            b_flags::BF_NOTEDITED + b_flags::BF_NEW + b_flags::BF_READERR
        );
        assert_eq!(b_flags::BF_WRITE_MASK, 0x08 + 0x10 + 0x40);
    }

    #[test]
    fn w_valid_flags_are_distinct_bits() {
        let all = [
            w_valid::VALID_WROW,
            w_valid::VALID_WCOL,
            w_valid::VALID_VIRTCOL,
            w_valid::VALID_CHEIGHT,
            w_valid::VALID_CROW,
            w_valid::VALID_BOTLINE,
            w_valid::VALID_BOTLINE_AP,
            w_valid::VALID_TOPLINE,
        ];
        let mut seen = 0u16;
        for f in all {
            assert_eq!(seen & (f as u16), 0, "flag {f:#04x} overlaps a previous one");
            seen |= f as u16;
        }
    }

    #[test]
    fn winopt_default_has_no_script_ctx_entries() {
        let wo = WinoptT::default();
        assert_eq!(wo.wo_arab, 0);
        assert!(wo.wo_briopt.is_none());
        assert!(wo.wo_script_ctx.is_empty());
    }

    #[test]
    fn wininfo_default_has_null_window_and_default_opts() {
        let wi = WinInfo::default();
        assert!(wi.wi_win.is_null());
        assert!(!wi.wi_optset);
        assert!(wi.wi_folds.is_empty());
        assert_eq!(wi.wi_opt.wo_arab, 0);
    }

    #[test]
    fn syn_time_default_is_zeroed() {
        let st = SynTimeT::default();
        assert_eq!(st.total, 0);
        assert_eq!(st.slowest, 0);
        assert_eq!(st.count, 0);
        assert_eq!(st.match_, 0);
    }

    #[test]
    fn buf_update_callbacks_default_has_no_refs() {
        let cb = BufUpdateCallbacks::default();
        assert_eq!(cb.on_lines, -1);
        assert_eq!(cb.on_bytes, -1);
        assert_eq!(cb.on_changedtick, -1);
        assert_eq!(cb.on_detach, -1);
        assert_eq!(cb.on_reload, -1);
        assert!(!cb.utf_sizes);
        assert!(!cb.preview);
    }

    #[test]
    fn buf_has_entry_flags_are_distinct_bits() {
        assert_ne!(BUF_HAS_QF_ENTRY, BUF_HAS_LL_ENTRY);
        assert_eq!(BUF_HAS_QF_ENTRY & BUF_HAS_LL_ENTRY, 0);
    }
}
