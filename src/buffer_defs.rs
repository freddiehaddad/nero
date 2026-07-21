//! Translated from `src/nvim/buffer_defs.h` (partial - `tabpage_S`, which
//! needs `dict_T`'s real fields from the eval engine, is deliberately
//! deferred rather than rushed).
//!
//! Translated: `bufref_T`, the `VALID_*`/`BF_*` bit-flag constants,
//! `disptick_T`, `taggy_T`, `winopt_T`, `WinInfo` (`struct wininfo_S`),
//! `syn_time_T`, `synblock_T`, `BufUpdateCallbacks`, the
//! `BUF_HAS_*`/`MAX_MAPHASH` constants, `FloatAnchor`, `FloatRelative`,
//! `WinKind`, `WinSplit`, `WinStyle`, `AlignTextPos`, `BorderTextType`,
//! `WinConfig`, `pos_save_T`, `lcs_chars_T`, `fcs_chars_T`, `DB_COUNT`,
//! `diffblock_S`/`diffline_change_S`/`diffline_S` (-> `DiffT`/
//! `DifflineChangeT`/`DifflineT`), the `SNAP_*` constants, `wline_T`,
//! `struct file_buffer` (-> [`BufT`]) itself including its
//! buffer-local-options block, `FR_LEAF`/`FR_ROW`/`FR_COL` and `struct
//! frame_S` (-> [`FrameT`]), and now `struct window_S` (-> [`WinT`])
//! itself.
//!
//! Deferred: `match_T`/`llpos_T`/`matchitem_T` (need `regmmatch_T`,
//! `regexp_defs.h`, phase 7 - `matchitem_T`'s opaque placeholder,
//! `crate::types_defs::MatchitemT`, is enough for `WinT.w_match_head`'s
//! pointer field for now), and `tabpage_S` (needs `dict_T`'s real
//! fields, the eval engine).

use crate::arglist_defs::AlistT;
use crate::eval::typval_defs::{Callback, ChangedtickDictItem, DictT, ScopeDictDictItem, SctxT, VarnumberT};
use crate::garray_defs::GarrayT;
use crate::grid_defs::{GridView, ScreenGrid};
use crate::hashtab_defs::HashtabT;
use crate::map::{Map, Set};
use crate::mark_defs::{FmarkT, JUMPLISTSIZE, NMARKS, TAGSTACKSIZE, XfmarkT};
use crate::marktree_defs::MarkTree;
use crate::memline_defs::MemlineT;
use crate::os::fs_defs::FileID;
use crate::os::time_defs::Timestamp;
use crate::pos_defs::{ColnrT, LinenrT, PosT};
use crate::sign_defs::SIGN_SHOW_MAX;
use crate::statusline_defs::{StcClicks, StlClickDefinition};
use crate::types_defs::{HandleT, LuaRef, MapblockT, MatchitemT, OptInt, ProftimeT, QfInfoT, TerminalT};
use crate::undo_defs::{UHeader, VisualinfoT};

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
#[derive(Debug, Clone, Default)]
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

impl Default for SynblockT {
    /// Manual (not derived): `b_spell_ismw: [bool; 256]` exceeds the array
    /// length Rust's standard library provides a blanket `Default` impl
    /// for (`0..=32`), and `b_keywtab`/`b_keywtab_ic` (`HashtabT`) are
    /// constructed via `hashtab.c`'s own translated `hash_init()`
    /// (`hashtab::hash_init()`), not a `Default` impl - `hashtab_defs.rs`
    /// deliberately has none, matching the original's split between pure
    /// type declarations and the algorithm that initializes them.
    fn default() -> Self {
        SynblockT {
            b_keywtab: HashtabT::hash_init(),
            b_keywtab_ic: HashtabT::hash_init(),
            b_syn_error: false,
            b_syn_slow: false,
            b_syn_ic: 0,
            b_syn_foldlevel: 0,
            b_syn_spell: 0,
            b_syn_patterns: GarrayT::default(),
            b_syn_clusters: GarrayT::default(),
            b_spell_cluster_id: 0,
            b_nospell_cluster_id: 0,
            b_syn_containedin: 0,
            b_syn_sync_flags: 0,
            b_syn_sync_id: 0,
            b_syn_sync_minlines: 0,
            b_syn_sync_maxlines: 0,
            b_syn_sync_linebreaks: 0,
            b_syn_linecont_pat: None,
            b_syn_linecont_prog: std::ptr::null_mut(),
            b_syn_linecont_time: SynTimeT::default(),
            b_syn_linecont_ic: 0,
            b_syn_topgrp: 0,
            b_syn_conceal: 0,
            b_syn_folditems: 0,
            b_sst_array: std::ptr::null_mut(),
            b_sst_len: 0,
            b_sst_first: std::ptr::null_mut(),
            b_sst_firstfree: std::ptr::null_mut(),
            b_sst_freecount: 0,
            b_sst_check_lnum: 0,
            b_sst_lasttick: 0,
            b_idlist_cache: std::ptr::null_mut(),
            b_langp: GarrayT::default(),
            b_spell_ismw: [false; 256],
            b_spell_ismw_mb: None,
            b_p_spc: None,
            b_cap_prog: std::ptr::null_mut(),
            b_p_spf: None,
            b_p_spl: None,
            b_p_spo: None,
            b_p_spo_flags: 0,
            b_cjk: 0,
            b_syn_chartab: [0; 32],
            b_syn_isk: None,
        }
    }
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

/// `FloatAnchor`: kept as a plain `i32` with named bit constants (matches
/// the original's own `typedef int FloatAnchor;` plus a separate unnamed
/// `enum` for the bit values, rather than a genuine C enum).
pub type FloatAnchor = i32;
pub const K_FLOAT_ANCHOR_EAST: FloatAnchor = 1;
pub const K_FLOAT_ANCHOR_SOUTH: FloatAnchor = 2;

/// Keep in sync with `float_relative_str[]` in `nvim_win_get_config()`
/// (`FloatRelative`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FloatRelative {
    #[default]
    Editor = 0,
    Window = 1,
    Cursor = 2,
    Mouse = 3,
    Tabline = 4,
    Laststatus = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WinKind {
    #[default]
    Normal = 0,
    Info,
}

/// Keep in sync with `win_split_str[]` in `nvim_win_get_config()`
/// (`api/win_config.c`) (`WinSplit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WinSplit {
    #[default]
    Left = 0,
    Right = 1,
    Above = 2,
    Below = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WinStyle {
    #[default]
    Unused = 0,
    /// Minimal UI: no number column, eob markers, etc.
    Minimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignTextPos {
    #[default]
    Left = 0,
    Center = 1,
    Right = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderTextType {
    #[default]
    Title = 0,
    Footer = 1,
}

/// See `:help nvim_open_win()` for documentation (`WinConfig`).
#[derive(Debug, Clone)]
pub struct WinConfig {
    pub window: crate::api::private::defs::Window,
    pub bufpos: crate::pos_defs::LposT,
    pub height: i32,
    pub width: i32,
    pub row: f64,
    pub col: f64,
    pub anchor: FloatAnchor,
    pub relative: FloatRelative,
    pub external: bool,
    pub focusable: bool,
    pub mouse: bool,
    pub split: WinSplit,
    pub zindex: i32,
    pub style: WinStyle,
    pub border: bool,
    pub shadow: bool,
    pub border_chars: [[crate::types_defs::ScharT; crate::types_defs::MAX_SCHAR_SIZE]; 8],
    pub border_hl_ids: [i32; 8],
    pub border_attr: [i32; 8],
    pub title: bool,
    pub title_pos: AlignTextPos,
    pub title_chunks: crate::decoration_defs::VirtText,
    pub title_width: i32,
    pub footer: bool,
    pub footer_pos: AlignTextPos,
    pub footer_chunks: crate::decoration_defs::VirtText,
    pub footer_width: i32,
    pub noautocmd: bool,
    pub fixed: bool,
    pub hide: bool,
    pub _cmdline_offset: i32,
}

impl Default for WinConfig {
    /// `WIN_CONFIG_INIT`
    fn default() -> Self {
        WinConfig {
            window: 0,
            bufpos: crate::pos_defs::LposT { lnum: -1, col: 0 },
            height: 0,
            width: 0,
            row: 0.0,
            col: 0.0,
            anchor: 0,
            relative: FloatRelative::Editor,
            external: false,
            focusable: true,
            mouse: true,
            split: WinSplit::Left,
            zindex: crate::grid_defs::K_ZINDEX_FLOAT_DEFAULT,
            style: WinStyle::Unused,
            border: false,
            shadow: false,
            border_chars: [[0; crate::types_defs::MAX_SCHAR_SIZE]; 8],
            border_hl_ids: [0; 8],
            border_attr: [0; 8],
            title: false,
            title_pos: AlignTextPos::Left,
            title_chunks: Vec::new(),
            title_width: 0,
            footer: false,
            footer_pos: AlignTextPos::Left,
            footer_chunks: Vec::new(),
            footer_width: 0,
            noautocmd: false,
            fixed: false,
            hide: false,
            _cmdline_offset: i32::MAX,
        }
    }
}

/// Structure to store last cursor position and topline (`pos_save_T`).
/// Used by `check_lnums()` and `reset_lnums()` (not yet translated).
#[derive(Debug, Clone, Copy, Default)]
pub struct PosSaveT {
    /// original topline value
    pub w_topline_save: i32,
    /// corrected topline value
    pub w_topline_corr: i32,
    /// original cursor position
    pub w_cursor_save: crate::pos_defs::PosT,
    /// corrected cursor position
    pub w_cursor_corr: crate::pos_defs::PosT,
}

/// Characters from the `'listchars'` option (`lcs_chars_T`).
#[derive(Debug, Clone, Default)]
pub struct LcsCharsT {
    pub eol: crate::types_defs::ScharT,
    pub ext: crate::types_defs::ScharT,
    pub prec: crate::types_defs::ScharT,
    pub nbsp: crate::types_defs::ScharT,
    pub space: crate::types_defs::ScharT,
    /// first tab character
    pub tab1: crate::types_defs::ScharT,
    /// second tab character
    pub tab2: crate::types_defs::ScharT,
    /// third tab character
    pub tab3: crate::types_defs::ScharT,
    pub leadtab1: crate::types_defs::ScharT,
    pub leadtab2: crate::types_defs::ScharT,
    pub leadtab3: crate::types_defs::ScharT,
    pub lead: crate::types_defs::ScharT,
    pub trail: crate::types_defs::ScharT,
    /// in place of the original's raw `schar_T *multispace` pointer to a
    /// heap-allocated array (one entry per display column of a multi-space
    /// run, per `'listchars'`'s `multispace:` sub-option).
    pub multispace: Option<Vec<crate::types_defs::ScharT>>,
    pub leadmultispace: Option<Vec<crate::types_defs::ScharT>>,
    pub conceal: crate::types_defs::ScharT,
}

/// Characters from the `'fillchars'` option (`fcs_chars_T`).
#[derive(Debug, Clone, Copy, Default)]
pub struct FcsCharsT {
    pub stl: crate::types_defs::ScharT,
    pub stlnc: crate::types_defs::ScharT,
    pub wbr: crate::types_defs::ScharT,
    pub horiz: crate::types_defs::ScharT,
    pub horizup: crate::types_defs::ScharT,
    pub horizdown: crate::types_defs::ScharT,
    pub vert: crate::types_defs::ScharT,
    pub vertleft: crate::types_defs::ScharT,
    pub vertright: crate::types_defs::ScharT,
    pub verthoriz: crate::types_defs::ScharT,
    pub fold: crate::types_defs::ScharT,
    /// when fold is open
    pub foldopen: crate::types_defs::ScharT,
    /// when fold is closed
    pub foldclosed: crate::types_defs::ScharT,
    /// continuous fold marker
    pub foldsep: crate::types_defs::ScharT,
    pub foldinner: crate::types_defs::ScharT,
    pub diff: crate::types_defs::ScharT,
    pub msgsep: crate::types_defs::ScharT,
    pub eob: crate::types_defs::ScharT,
    pub lastline: crate::types_defs::ScharT,
    pub trunc: crate::types_defs::ScharT,
    pub truncrl: crate::types_defs::ScharT,
}

// --- Stuff for diff mode. ---

/// up to four buffers can be diff'ed (`DB_COUNT`)
pub const DB_COUNT: usize = 8;

/// Each diffblock defines where a block of lines starts in each of the
/// buffers and how many lines it occupies in that buffer (`diffblock_S`,
/// typedef'd as `diff_T`). When the lines are missing in the buffer the
/// `df_count[]` is zero. This is all counted in buffer lines.
///
/// Usually there is always at least one unchanged line in between the
/// diffs as otherwise it would have been included in the diff above or
/// below it. When linematch or diff anchors are used, this is no longer
/// guaranteed, and we may have adjacent diff blocks. In all cases they
/// will not overlap, although it is possible to have multiple 0-count
/// diff blocks at the same line. `df_lnum[] + df_count[]` is the lnum
/// below the change. When in one buffer lines have been inserted, in the
/// other buffer `df_lnum[]` is the line below the insertion and
/// `df_count[]` is zero. When appending lines at the end of the buffer,
/// `df_lnum[]` is one beyond the end!
///
/// This is using a linked list (`df_next`, in place of the original's raw
/// `diff_T *`, matching this crate's established convention for intrusive
/// linked structures elsewhere, e.g. `MtNode`), because the number of
/// differences is expected to be reasonably small. The list is sorted on
/// lnum. Each diffblock also contains a cached list of inline diff of
/// changes within the block, used for highlighting.
pub struct DiffT {
    pub df_next: *mut DiffT,
    /// line number in buffer
    pub df_lnum: [crate::pos_defs::LinenrT; DB_COUNT],
    /// nr of inserted/changed lines
    pub df_count: [crate::pos_defs::LinenrT; DB_COUNT],
    /// has the linematch algorithm ran on this diff hunk to divide it into
    /// smaller diff hunks?
    pub is_linematched: bool,
    /// has cached list of inline changes
    pub has_changes: bool,
    /// list of inline changes (`diffline_change_T`)
    pub df_changes: GarrayT,
}

/// Each entry stores a single inline change within a diff block. Line
/// numbers are recorded as relative offsets, and columns are byte
/// offsets, not character counts. Ranges are `[start,end)`, with the end
/// being exclusive (`diffline_change_S`, typedef'd as `diffline_change_T`).
#[derive(Debug, Clone, Copy, Default)]
pub struct DifflineChangeT {
    /// byte offset of start of range in the line
    pub dc_start: [crate::pos_defs::ColnrT; DB_COUNT],
    /// 1 past byte offset of end of range in line
    pub dc_end: [crate::pos_defs::ColnrT; DB_COUNT],
    /// starting line offset
    pub dc_start_lnum_off: [i32; DB_COUNT],
    /// end line offset
    pub dc_end_lnum_off: [i32; DB_COUNT],
}

/// Describes a single line's list of inline changes. Use
/// `diff_change_parse()` (not yet translated) to parse this
/// (`diffline_S`, typedef'd as `diffline_T`).
#[derive(Debug, Clone, Default)]
pub struct DifflineT {
    /// in place of the original's raw `diffline_change_T *changes` pointer
    /// to a heap-allocated array of `num_changes` entries.
    pub changes: Vec<DifflineChangeT>,
    pub bufidx: i32,
    pub lineoff: i32,
}

pub const SNAP_HELP_IDX: usize = 0;
pub const SNAP_AUCMD_IDX: usize = 1;
pub const SNAP_QUICKFIX_IDX: usize = 2;
pub const SNAP_COUNT: usize = 3;

/// Structure to cache info for displayed lines in `w_lines[]` (`wline_T`).
/// Each logical line has one entry. The entry tells how the logical line
/// is currently displayed in the window. This is updated when displaying
/// the window. When the display is changed (e.g. when clearing the
/// screen) `w_lines_valid` is changed to exclude invalid entries. When
/// making changes to the buffer, `wl_valid` is reset to indicate `wl_size`
/// may not reflect what is actually in the buffer. When `wl_valid` is
/// false, the entries can only be used to count the number of displayed
/// lines used; `wl_lnum` and `wl_lastlnum` are invalid too.
#[derive(Debug, Clone, Copy, Default)]
pub struct WlineT {
    /// buffer line number for logical line
    pub wl_lnum: crate::pos_defs::LinenrT,
    /// height in screen lines
    pub wl_size: u16,
    /// true values are valid for text in buffer
    pub wl_valid: bool,
    /// true when this is a range of folded lines
    pub wl_folded: bool,
    /// last buffer line number for folded line
    pub wl_foldend: crate::pos_defs::LinenrT,
    /// last buffer line number for logical line
    pub wl_lastlnum: crate::pos_defs::LinenrT,
}

// Values for b_p_iminsert and b_p_imsearch.
/// Use b_p_iminsert value for search (`B_IMODE_USE_INSERT`).
pub const B_IMODE_USE_INSERT: OptInt = -1;
/// Input via none (`B_IMODE_NONE`).
pub const B_IMODE_NONE: OptInt = 0;
/// Input via langmap (`B_IMODE_LMAP`).
pub const B_IMODE_LMAP: OptInt = 1;
pub const B_IMODE_LAST: OptInt = 1;

// Flags for b_kmap_state.
/// `'keymap'` was set, call `keymap_init()` (`KEYMAP_INIT`).
pub const KEYMAP_INIT: i16 = 1;
/// `'keymap'` mappings have been loaded (`KEYMAP_LOADED`).
pub const KEYMAP_LOADED: i16 = 2;

/// Per-line sign-count bookkeeping, kept for `'signcolumn'` display
/// (originally an anonymous `struct { ... } b_signcols;` inside
/// `file_buffer` - given a name, `BufSigncolsT`, since Rust has no
/// anonymous-struct-field syntax).
#[derive(Debug, Clone, Copy, Default)]
pub struct BufSigncolsT {
    /// maximum number of signs on a single line
    pub max: i32,
    /// value of max when the buffer was last drawn
    pub last_max: i32,
    /// number of lines with number of signs
    pub count: [i32; SIGN_SHOW_MAX as usize],
    /// whether `'signcolumn'` is displayed in an `"auto:n>1"` configured
    /// window; `b_signcols` calculation is skipped if false.
    pub autom: bool,
}

/// Structure that holds information about one file (`struct file_buffer`,
/// typedef'd as `buf_T`).
///
/// Several windows can share a single buffer. A buffer is unallocated if
/// there is no memfile for it. A buffer is new if the associated file has
/// never been loaded yet.
///
/// Pointer fields to not-yet-translated owning subsystems (`b_next`/
/// `b_prev` intrusive list links, `b_u_*` undo header pointers,
/// `b_maphash`/`b_first_abbr` mapblock_T pointers, `terminal`,
/// `additional_data`, `b_vars`) stay raw pointers, matching this crate's
/// established convention (see `marktree.rs`'s module docs) rather than
/// inventing an owning-container redesign the original doesn't have.
pub struct BufT {
    /// unique id for the buffer (buffer number). The original also names
    /// this field `b_fnum` via `#define b_fnum handle`; Rust has no field
    /// alias mechanism, so callers use `handle` directly (a same-named
    /// accessor method can be added if a real translated caller needs the
    /// `b_fnum` spelling specifically).
    pub handle: HandleT,

    /// associated memline (also contains line count)
    pub b_ml: MemlineT,

    /// links in list of buffers
    pub b_next: *mut BufT,
    pub b_prev: *mut BufT,

    /// nr of windows open on this buffer
    pub b_nwindows: i32,

    /// various `BF_*` flags (see [`b_flags`])
    pub b_flags: i32,
    /// Buffer is being closed or referenced, don't let autocommands wipe
    /// it out.
    pub b_locked: i32,
    /// Buffer is being closed, don't allow opening it in more windows.
    pub b_locked_split: i32,
    /// Non-zero when the buffer can't be changed. Used for FileChangedRO.
    pub b_ro_locked: i32,

    /// full path file name, allocated (`NULL` for no name).
    pub b_ffname: Option<Vec<u8>>,
    /// short file name, allocated, may be equal to `b_ffname`.
    pub b_sfname: Option<Vec<u8>>,
    /// current file name: in the original, points to `b_ffname` or
    /// `b_sfname` (an alias, not a separate allocation); modeled here as
    /// its own owned copy for now since the aliasing is only meaningful
    /// once the code that sets these (`buffer.c`, not yet translated)
    /// exists to establish exactly when each alias applies.
    pub b_fname: Option<Vec<u8>>,

    pub file_id_valid: bool,
    pub file_id: FileID,

    /// `'modified'`: Set to true if something in the file has been changed
    /// and not written out.
    pub b_changed: i32,

    /// Change-identifier incremented for each change, including undo.
    /// This is a dict item used to store `b:changedtick`.
    pub changedtick_di: ChangedtickDictItem,

    /// `b:changedtick` when `TextChanged` was last triggered.
    pub b_last_changedtick: VarnumberT,
    /// `b:changedtick` for `TextChangedI`/`TextChangedT`.
    pub b_last_changedtick_i: VarnumberT,
    /// `b:changedtick` for `TextChangedP`.
    pub b_last_changedtick_pum: VarnumberT,

    /// Set to true if we are in the middle of saving the buffer.
    pub b_saving: bool,

    // Changes to a buffer require updating of the display. To minimize
    // the work, remember changes made and update everything at once.
    /// true when there are changes since the last time the display was
    /// updated
    pub b_mod_set: bool,
    /// topmost lnum that was changed
    pub b_mod_top: LinenrT,
    /// lnum below last changed line, AFTER the change
    pub b_mod_bot: LinenrT,
    /// number of extra buffer lines inserted; negative when lines were
    /// deleted
    pub b_mod_xlines: LinenrT,
    /// list of last used info for each window (`kvec_t(WinInfo *)`)
    pub b_wininfo: Vec<*mut WinInfo>,
    /// last display tick syntax was updated
    pub b_mod_tick_syn: DisptickT,
    /// last display tick decoration providers were invoked
    pub b_mod_tick_decor: DisptickT,

    /// last change time of original file
    pub b_mtime: i64,
    /// nanoseconds of last change time
    pub b_mtime_ns: i64,
    /// last change time when reading
    pub b_mtime_read: i64,
    /// nanoseconds of last read time
    pub b_mtime_read_ns: i64,
    /// size of original file in bytes
    pub b_orig_size: u64,
    /// mode of original file
    pub b_orig_mode: i32,
    /// time when the buffer was last used; used for viminfo
    pub b_last_used: Timestamp,

    /// current named marks (mark.c)
    pub b_namedm: [FmarkT; NMARKS as usize],

    // These variables are set when Visual.active becomes false.
    pub b_visual: VisualinfoT,
    /// `b_visual.vi_mode` for `visualmode()`
    pub b_visual_mode_eval: i32,

    /// cursor position when last unloading this buffer
    pub b_last_cursor: FmarkT,
    /// where Insert mode was left
    pub b_last_insert: FmarkT,
    /// position of last change: `'.` mark
    pub b_last_change: FmarkT,

    // The changelist contains old change positions.
    pub b_changelist: [FmarkT; JUMPLISTSIZE as usize],
    /// number of active entries
    pub b_changelistlen: i32,
    /// set by `u_savecommon()`
    pub b_new_change: bool,

    /// Character table, only used in charset.c for `'iskeyword'`: bitset
    /// with `4*64=256` bits, 1 bit per character 0-255.
    pub b_chartab: [u64; 4],

    /// Table used for mappings local to a buffer.
    pub b_maphash: [*mut MapblockT; MAX_MAPHASH as usize],
    /// First abbreviation local to a buffer.
    pub b_first_abbr: *mut MapblockT,
    /// User commands local to the buffer.
    pub b_ucmds: GarrayT,
    /// start and end of an operator, also used for `'[` and `']`
    pub b_op_start: PosT,
    /// used for `Ins.start_orig`
    pub b_op_start_orig: PosT,
    pub b_op_end: PosT,

    /// Have we read ShaDa marks yet?
    pub b_marks_read: bool,

    /// did `":set modified"`
    pub b_modified_was_set: bool,
    /// `FileType` event found
    pub b_did_filetype: bool,
    /// value for `did_filetype` when starting to execute autocommands
    pub b_keep_filetype: bool,

    /// Set by the apply_autocmds_group function if the given event is
    /// equal to `EVENT_FILETYPE`. Used by `readfile()` to determine
    /// whether read autocommands triggered `EVENT_FILETYPE`.
    ///
    /// Relying on this value requires one to reset it prior calling
    /// `apply_autocmds_group()`.
    pub b_au_did_filetype: bool,

    // The following are only used in undo.c.
    /// pointer to oldest header
    pub b_u_oldhead: *mut UHeader,
    /// pointer to newest header; may not be valid if `b_u_curhead` is not
    /// `NULL`
    pub b_u_newhead: *mut UHeader,
    /// pointer to current header
    pub b_u_curhead: *mut UHeader,
    /// current number of headers
    pub b_u_numhead: i32,
    /// entry lists are synced
    pub b_u_synced: bool,
    /// last used undo sequence number
    pub b_u_seq_last: i32,
    /// counter for last file write
    pub b_u_save_nr_last: i32,
    /// `uh_seq` of header below which we are now
    pub b_u_seq_cur: i32,
    /// `uh_time` of header below which we are now
    pub b_u_time_cur: Timestamp,
    /// file write nr after which we are now
    pub b_u_save_nr_cur: i32,

    // Variables for the "U" command in undo.c.
    /// saved line for "U" command
    pub b_u_line_ptr: Option<Vec<u8>>,
    /// line number of line in `u_line`
    pub b_u_line_lnum: LinenrT,
    /// optional column number
    pub b_u_line_colnr: ColnrT,

    /// `^N`/`^P` have scanned this buffer
    pub b_scanned: bool,

    // Flags for use of ":lmap" and IM control.
    /// input mode for insert
    pub b_p_iminsert: OptInt,
    /// input mode for search
    pub b_p_imsearch: OptInt,

    /// using "lmap" mappings (see [`KEYMAP_INIT`]/[`KEYMAP_LOADED`])
    pub b_kmap_state: i16,
    /// the keymap table
    pub b_kmap_ga: GarrayT,

    // Options local to a buffer. They are here because their value
    // depends on the type of file or contents of the file being edited.
    /// set when options initialized
    pub b_p_initialized: bool,

    /// SCTXs for buffer-local options (`sctx_T b_p_script_ctx[kBufOptCount]`
    /// in the original). Same `Vec`-stands-in-for-codegen-sized-array
    /// reasoning as `WinoptT.wo_script_ctx` (`kBufOptCount` is a
    /// codegen-derived constant not available without running
    /// `src/gen/*.lua`, flagged and deferred since phase 1).
    pub b_p_script_ctx: Vec<SctxT>,

    /// `'autocomplete'`
    pub b_p_ac: i32,
    /// `'autoindent'`
    pub b_p_ai: i32,
    /// `b_p_ai` saved for paste mode
    pub b_p_ai_nopaste: i32,
    /// `'backupcopy'`
    pub b_p_bkc: Option<Vec<u8>>,
    /// flags for `'backupcopy'`
    pub b_bkc_flags: u32,
    /// `'copyindent'`
    pub b_p_ci: i32,
    /// `'binary'`
    pub b_p_bin: i32,
    /// `'bomb'`
    pub b_p_bomb: i32,
    /// `'bufhidden'`
    pub b_p_bh: Option<Vec<u8>>,
    /// `'buftype'`
    pub b_p_bt: Option<Vec<u8>>,
    /// `'busy'`
    pub b_p_busy: OptInt,
    /// quickfix exists for buffer
    pub b_has_qf_entry: i32,
    /// `'buflisted'`
    pub b_p_bl: i32,
    /// `'channel'`
    pub b_p_channel: OptInt,
    /// `'cindent'`
    pub b_p_cin: i32,
    /// `'cinoptions'`
    pub b_p_cino: Option<Vec<u8>>,
    /// `'cinkeys'`
    pub b_p_cink: Option<Vec<u8>>,
    /// `'cinwords'`
    pub b_p_cinw: Option<Vec<u8>>,
    /// `'cinscopedecls'`
    pub b_p_cinsd: Option<Vec<u8>>,
    /// `'comments'`
    pub b_p_com: Option<Vec<u8>>,
    /// `'commentstring'`
    pub b_p_cms: Option<Vec<u8>>,
    /// `'completeopt'` local value
    pub b_p_cot: Option<Vec<u8>>,
    /// flags for `'completeopt'`
    pub b_cot_flags: u32,
    /// `'complete'`
    pub b_p_cpt: Option<Vec<u8>>,
    /// `'completeslash'` (Windows-only: `#ifdef BACKSLASH_IN_FILENAME` in
    /// the original, which is defined exactly on Windows builds - see
    /// `os/win_defs.rs`/`os/os_defs.rs`'s existing `#[cfg(windows)]`
    /// precedent for the same macro).
    #[cfg(windows)]
    pub b_p_csl: Option<Vec<u8>>,
    /// `F{func}` in `'complete'` callback (`Callback *b_p_cpt_cb` in the
    /// original: a heap array of `b_p_cpt_count` entries - folded into a
    /// single `Vec`, dropping the separate redundant count field, same as
    /// `UEntry.ue_array`/`ue_size` in `undo_defs.rs`).
    pub b_p_cpt_cb: Vec<Callback>,
    /// `'completefunc'`
    pub b_p_cfu: Option<Vec<u8>>,
    /// `'completefunc'` callback
    pub b_cfu_cb: Callback,
    /// `'omnifunc'`
    pub b_p_ofu: Option<Vec<u8>>,
    /// `'omnifunc'` callback
    pub b_ofu_cb: Callback,
    /// `'tagfunc'` option value
    pub b_p_tfu: Option<Vec<u8>>,
    /// `'tagfunc'` callback
    pub b_tfu_cb: Callback,
    /// `'findfunc'` option value
    pub b_p_ffu: Option<Vec<u8>>,
    /// `'findfunc'` callback
    pub b_ffu_cb: Callback,
    /// `'endoffile'`
    pub b_p_eof: i32,
    /// `'endofline'`
    pub b_p_eol: i32,
    /// `'fixendofline'`
    pub b_p_fixeol: i32,
    /// `'expandtab'`
    pub b_p_et: i32,
    /// `b_p_et` saved for binary mode
    pub b_p_et_nobin: i32,
    /// `b_p_et` saved for paste mode
    pub b_p_et_nopaste: i32,
    /// `'fileencoding'`
    pub b_p_fenc: Option<Vec<u8>>,
    /// `'fileformat'`
    pub b_p_ff: Option<Vec<u8>>,
    /// `'filetype'`
    pub b_p_ft: Option<Vec<u8>>,
    /// `'formatoptions'`
    pub b_p_fo: Option<Vec<u8>>,
    /// `'formatlistpat'`
    pub b_p_flp: Option<Vec<u8>>,
    /// `'infercase'`
    pub b_p_inf: i32,
    /// `'iskeyword'`
    pub b_p_isk: Option<Vec<u8>>,
    /// `'define'` local value
    pub b_p_def: Option<Vec<u8>>,
    /// `'include'`
    pub b_p_inc: Option<Vec<u8>>,
    /// `'includeexpr'`
    pub b_p_inex: Option<Vec<u8>>,
    /// flags for `'includeexpr'`
    pub b_p_inex_flags: u32,
    /// `'indentexpr'`
    pub b_p_inde: Option<Vec<u8>>,
    /// flags for `'indentexpr'`
    pub b_p_inde_flags: u32,
    /// `'indentkeys'`
    pub b_p_indk: Option<Vec<u8>>,
    /// `'formatprg'`
    pub b_p_fp: Option<Vec<u8>>,
    /// `'formatexpr'`
    pub b_p_fex: Option<Vec<u8>>,
    /// flags for `'formatexpr'`
    pub b_p_fex_flags: u32,
    /// `'fsync'`
    pub b_p_fs: i32,
    /// `'keywordprg'`
    pub b_p_kp: Option<Vec<u8>>,
    /// `'lisp'`
    pub b_p_lisp: i32,
    /// `'lispoptions'`
    pub b_p_lop: Option<Vec<u8>>,
    /// `'makeencoding'`
    pub b_p_menc: Option<Vec<u8>>,
    /// `'matchpairs'`
    pub b_p_mps: Option<Vec<u8>>,
    /// `'modeline'`
    pub b_p_ml: i32,
    /// `b_p_ml` saved for binary mode
    pub b_p_ml_nobin: i32,
    /// `'modifiable'`
    pub b_p_ma: i32,
    /// `'nrformats'`
    pub b_p_nf: Option<Vec<u8>>,
    /// `'preserveindent'`
    pub b_p_pi: i32,
    /// `'quoteescape'`
    pub b_p_qe: Option<Vec<u8>>,
    /// `'readonly'`
    pub b_p_ro: i32,
    /// `'shiftwidth'`
    pub b_p_sw: OptInt,
    /// `'scrollback'`
    pub b_p_scbk: OptInt,
    /// `'smartindent'`
    pub b_p_si: i32,
    /// `'softtabstop'`
    pub b_p_sts: OptInt,
    /// `b_p_sts` saved for paste mode
    pub b_p_sts_nopaste: OptInt,
    /// `'suffixesadd'`
    pub b_p_sua: Option<Vec<u8>>,
    /// `'swapfile'`
    pub b_p_swf: i32,
    /// `'synmaxcol'`
    pub b_p_smc: OptInt,
    /// `'syntax'`
    pub b_p_syn: Option<Vec<u8>>,
    /// `'tabstop'`
    pub b_p_ts: OptInt,
    /// `'textwidth'`
    pub b_p_tw: OptInt,
    /// `b_p_tw` saved for binary mode
    pub b_p_tw_nobin: OptInt,
    /// `b_p_tw` saved for paste mode
    pub b_p_tw_nopaste: OptInt,
    /// `'wrapmargin'`
    pub b_p_wm: OptInt,
    /// `b_p_wm` saved for binary mode
    pub b_p_wm_nobin: OptInt,
    /// `b_p_wm` saved for paste mode
    pub b_p_wm_nopaste: OptInt,
    /// `'varsofttabstop'`
    pub b_p_vsts: Option<Vec<u8>>,
    /// `'varsofttabstop'` in internal format
    pub b_p_vsts_array: Option<Vec<ColnrT>>,
    /// `b_p_vsts` saved for paste mode
    pub b_p_vsts_nopaste: Option<Vec<u8>>,
    /// `'vartabstop'`
    pub b_p_vts: Option<Vec<u8>>,
    /// `'vartabstop'` in internal format
    pub b_p_vts_array: Option<Vec<ColnrT>>,
    /// `'keymap'`
    pub b_p_keymap: Option<Vec<u8>>,

    // Local values for options which are normally global.
    /// `'grepformat'` local value
    pub b_p_gefm: Option<Vec<u8>>,
    /// `'grepprg'` local value
    pub b_p_gp: Option<Vec<u8>>,
    /// `'makeprg'` local value
    pub b_p_mp: Option<Vec<u8>>,
    /// `'errorformat'` local value
    pub b_p_efm: Option<Vec<u8>>,
    /// `'equalprg'` local value
    pub b_p_ep: Option<Vec<u8>>,
    /// `'path'` local value
    pub b_p_path: Option<Vec<u8>>,
    /// `'autoread'` local value
    pub b_p_ar: i32,
    /// `'tags'` local value
    pub b_p_tags: Option<Vec<u8>>,
    /// `'tagcase'` local value
    pub b_p_tc: Option<Vec<u8>>,
    /// flags for `'tagcase'`
    pub b_tc_flags: u32,
    /// `'dictionary'` local value
    pub b_p_dict: Option<Vec<u8>>,
    /// `'diffanchors'` local value
    pub b_p_dia: Option<Vec<u8>>,
    /// `'thesaurus'` local value
    pub b_p_tsr: Option<Vec<u8>>,
    /// `'thesaurusfunc'` local value
    pub b_p_tsrfu: Option<Vec<u8>>,
    /// `'thesaurusfunc'` callback
    pub b_tsrfu_cb: Callback,
    /// `'undolevels'` local value
    pub b_p_ul: OptInt,
    /// `'undofile'`
    pub b_p_udf: i32,
    /// `'lispwords'` local value
    pub b_p_lw: Option<Vec<u8>>,
    // end of buffer options

    // Values set from b_p_cino.
    pub b_ind_level: i32,
    pub b_ind_open_imag: i32,
    pub b_ind_no_brace: i32,
    pub b_ind_first_open: i32,
    pub b_ind_open_extra: i32,
    pub b_ind_close_extra: i32,
    pub b_ind_open_left_imag: i32,
    pub b_ind_jump_label: i32,
    pub b_ind_case: i32,
    pub b_ind_case_code: i32,
    pub b_ind_case_break: i32,
    pub b_ind_param: i32,
    pub b_ind_func_type: i32,
    pub b_ind_comment: i32,
    pub b_ind_in_comment: i32,
    pub b_ind_in_comment2: i32,
    pub b_ind_cpp_baseclass: i32,
    pub b_ind_continuation: i32,
    pub b_ind_unclosed: i32,
    pub b_ind_unclosed2: i32,
    pub b_ind_unclosed_noignore: i32,
    pub b_ind_unclosed_wrapped: i32,
    pub b_ind_unclosed_whiteok: i32,
    pub b_ind_matching_paren: i32,
    pub b_ind_paren_prev: i32,
    pub b_ind_maxparen: i32,
    pub b_ind_maxcomment: i32,
    pub b_ind_scopedecl: i32,
    pub b_ind_scopedecl_code: i32,
    pub b_ind_java: i32,
    pub b_ind_js: i32,
    pub b_ind_keep_case_label: i32,
    pub b_ind_hash_comment: i32,
    pub b_ind_cpp_namespace: i32,
    pub b_ind_if_for_while: i32,
    pub b_ind_cpp_extern_c: i32,
    pub b_ind_pragma: i32,

    /// non-zero lnum when last line of next binary write should not have
    /// an end-of-line
    pub b_no_eol_lnum: LinenrT,

    /// last line had eof (CTRL-Z) when it was read
    pub b_start_eof: i32,
    /// last line had eol when it was read
    pub b_start_eol: i32,
    /// first char of `'ff'` when edit started
    pub b_start_ffc: i32,
    /// `'fileencoding'` when edit started or `NULL`
    pub b_start_fenc: Option<Vec<u8>>,
    /// `"++bad="` argument when edit started or 0
    pub b_bad_char: i32,
    /// `'bomb'` when it was read
    pub b_start_bomb: i32,

    /// Variable for "b:" Dict.
    pub b_bufvar: ScopeDictDictItem,
    /// b: scope Dict.
    pub b_vars: *mut DictT,

    // When a buffer is created, it starts without a swap file. b_may_swap
    // is then set to indicate that a swap file may be opened later. It is
    // reset if a swap file could not be opened.
    pub b_may_swap: bool,
    /// Set to true if user has been warned on first change of a read-only
    /// file
    pub b_did_warn: bool,

    // Two special kinds of buffers:
    // help buffer  - used for help files, won't use a swap file.
    // spell buffer - used for spell info, never displayed and doesn't
    //                have a file name.
    /// true for help file buffer (when set `b_p_bt` is "help")
    pub b_help: bool,
    /// True for a spell file buffer, most fields are not used!
    pub b_spell: bool,

    /// set by `prompt_setprompt()`
    pub b_prompt_text: Option<Vec<u8>>,
    /// set by `prompt_setcallback()`
    pub b_prompt_callback: Callback,
    /// set by `prompt_setinterrupt()`
    pub b_prompt_interrupt: Callback,
    /// `prompt_appendlines()` should start a newline
    pub b_prompt_append_new_line: bool,
    /// value for `restart_edit` when entering a prompt buffer window.
    pub b_prompt_insert: i32,
    /// Start of the editable area of a prompt buffer.
    pub b_prompt_start: FmarkT,

    /// Info related to syntax highlighting. `w_s` normally points to this,
    /// but some windows may use a different `synblock_T`.
    pub b_s: SynblockT,

    pub b_signcols: BufSigncolsT,

    /// Terminal instance associated with the buffer
    pub terminal: *mut TerminalT,

    /// Additional data from shada file if any.
    pub additional_data: *mut crate::types_defs::AdditionalData,

    /// modes where CTRL-C is mapped
    pub b_mapped_ctrl_c: i32,

    /// `MarkTree b_marktree[1]` in the original: a single-element-array
    /// idiom for "embed by value, but the surrounding code always takes
    /// its address" - translated as a plain by-value field, since Rust
    /// references make the address-of-an-owned-field idiom unnecessary.
    pub b_marktree: MarkTree,
    /// extmark namespaces (`Map(uint32_t, uint32_t) b_extmark_ns[1]` in
    /// the original - same single-element-array-for-by-value idiom as
    /// `b_marktree` above).
    pub b_extmark_ns: Map<u32, u32>,

    /// Store the line count as it was before appending or inserting
    /// lines. Used to determine a valid range before splicing marks, when
    /// the line count has already changed.
    pub b_prev_line_count: i32,

    /// array of channel ids which have asked to receive updates for this
    /// buffer.
    pub update_channels: Vec<u64>,
    /// array of lua callbacks for buffer updates.
    pub update_callbacks: Vec<BufUpdateCallbacks>,

    /// whether an update callback has requested codepoint size of deleted
    /// regions.
    pub update_need_codepoints: bool,

    // Measurements of the deleted or replaced region since the last
    // update event. Some consumers of buffer changes need to know the
    // byte size (like treesitter) or the corresponding UTF-32/UTF-16 size
    // (like LSP) of the deleted text.
    pub deleted_bytes: usize,
    pub deleted_bytes2: usize,
    pub deleted_codepoints: usize,
    pub deleted_codeunits: usize,

    /// The number of times the current line has been flushed in the
    /// memline.
    pub flush_count: i32,
}

impl Default for BufT {
    /// A purely mechanical, structural default (every field zero/empty/
    /// null) - **not** a translation of `buflist_new()`'s real "freshly
    /// created buffer" initial state. That real initial state includes
    /// e.g. copying the current global option values into every `b_p_*`
    /// field, which needs `option.c`'s options table (phase 4, not yet
    /// translated). Reflecting a few individually-recalled "realistic"
    /// option defaults here (e.g. `'tabstop'`'s well-known default of 8)
    /// while leaving the rest at 0 would misleadingly suggest more
    /// fidelity than this impl actually has - so every field is
    /// deliberately left at its plain zero value for now.
    fn default() -> Self {
        BufT {
            handle: 0,
            b_ml: MemlineT::default(),
            b_next: std::ptr::null_mut(),
            b_prev: std::ptr::null_mut(),
            b_nwindows: 0,
            b_flags: 0,
            b_locked: 0,
            b_locked_split: 0,
            b_ro_locked: 0,
            b_ffname: None,
            b_sfname: None,
            b_fname: None,
            file_id_valid: false,
            file_id: FileID::default(),
            b_changed: 0,
            changedtick_di: ChangedtickDictItem::default(),
            b_last_changedtick: 0,
            b_last_changedtick_i: 0,
            b_last_changedtick_pum: 0,
            b_saving: false,
            b_mod_set: false,
            b_mod_top: 0,
            b_mod_bot: 0,
            b_mod_xlines: 0,
            b_wininfo: Vec::new(),
            b_mod_tick_syn: 0,
            b_mod_tick_decor: 0,
            b_mtime: 0,
            b_mtime_ns: 0,
            b_mtime_read: 0,
            b_mtime_read_ns: 0,
            b_orig_size: 0,
            b_orig_mode: 0,
            b_last_used: 0,
            b_namedm: std::array::from_fn(|_| FmarkT::default()),
            b_visual: VisualinfoT::default(),
            b_visual_mode_eval: 0,
            b_last_cursor: FmarkT::default(),
            b_last_insert: FmarkT::default(),
            b_last_change: FmarkT::default(),
            b_changelist: std::array::from_fn(|_| FmarkT::default()),
            b_changelistlen: 0,
            b_new_change: false,
            b_chartab: [0; 4],
            b_maphash: [std::ptr::null_mut(); MAX_MAPHASH as usize],
            b_first_abbr: std::ptr::null_mut(),
            b_ucmds: GarrayT::default(),
            b_op_start: PosT::default(),
            b_op_start_orig: PosT::default(),
            b_op_end: PosT::default(),
            b_marks_read: false,
            b_modified_was_set: false,
            b_did_filetype: false,
            b_keep_filetype: false,
            b_au_did_filetype: false,
            b_u_oldhead: std::ptr::null_mut(),
            b_u_newhead: std::ptr::null_mut(),
            b_u_curhead: std::ptr::null_mut(),
            b_u_numhead: 0,
            b_u_synced: false,
            b_u_seq_last: 0,
            b_u_save_nr_last: 0,
            b_u_seq_cur: 0,
            b_u_time_cur: 0,
            b_u_save_nr_cur: 0,
            b_u_line_ptr: None,
            b_u_line_lnum: 0,
            b_u_line_colnr: 0,
            b_scanned: false,
            b_p_iminsert: 0,
            b_p_imsearch: 0,
            b_kmap_state: 0,
            b_kmap_ga: GarrayT::default(),
            b_p_initialized: false,
            b_p_script_ctx: Vec::new(),
            b_p_ac: 0,
            b_p_ai: 0,
            b_p_ai_nopaste: 0,
            b_p_bkc: None,
            b_bkc_flags: 0,
            b_p_ci: 0,
            b_p_bin: 0,
            b_p_bomb: 0,
            b_p_bh: None,
            b_p_bt: None,
            b_p_busy: 0,
            b_has_qf_entry: 0,
            b_p_bl: 0,
            b_p_channel: 0,
            b_p_cin: 0,
            b_p_cino: None,
            b_p_cink: None,
            b_p_cinw: None,
            b_p_cinsd: None,
            b_p_com: None,
            b_p_cms: None,
            b_p_cot: None,
            b_cot_flags: 0,
            b_p_cpt: None,
            #[cfg(windows)]
            b_p_csl: None,
            b_p_cpt_cb: Vec::new(),
            b_p_cfu: None,
            b_cfu_cb: Callback::default(),
            b_p_ofu: None,
            b_ofu_cb: Callback::default(),
            b_p_tfu: None,
            b_tfu_cb: Callback::default(),
            b_p_ffu: None,
            b_ffu_cb: Callback::default(),
            b_p_eof: 0,
            b_p_eol: 0,
            b_p_fixeol: 0,
            b_p_et: 0,
            b_p_et_nobin: 0,
            b_p_et_nopaste: 0,
            b_p_fenc: None,
            b_p_ff: None,
            b_p_ft: None,
            b_p_fo: None,
            b_p_flp: None,
            b_p_inf: 0,
            b_p_isk: None,
            b_p_def: None,
            b_p_inc: None,
            b_p_inex: None,
            b_p_inex_flags: 0,
            b_p_inde: None,
            b_p_inde_flags: 0,
            b_p_indk: None,
            b_p_fp: None,
            b_p_fex: None,
            b_p_fex_flags: 0,
            b_p_fs: 0,
            b_p_kp: None,
            b_p_lisp: 0,
            b_p_lop: None,
            b_p_menc: None,
            b_p_mps: None,
            b_p_ml: 0,
            b_p_ml_nobin: 0,
            b_p_ma: 0,
            b_p_nf: None,
            b_p_pi: 0,
            b_p_qe: None,
            b_p_ro: 0,
            b_p_sw: 0,
            b_p_scbk: 0,
            b_p_si: 0,
            b_p_sts: 0,
            b_p_sts_nopaste: 0,
            b_p_sua: None,
            b_p_swf: 0,
            b_p_smc: 0,
            b_p_syn: None,
            b_p_ts: 0,
            b_p_tw: 0,
            b_p_tw_nobin: 0,
            b_p_tw_nopaste: 0,
            b_p_wm: 0,
            b_p_wm_nobin: 0,
            b_p_wm_nopaste: 0,
            b_p_vsts: None,
            b_p_vsts_array: None,
            b_p_vsts_nopaste: None,
            b_p_vts: None,
            b_p_vts_array: None,
            b_p_keymap: None,
            b_p_gefm: None,
            b_p_gp: None,
            b_p_mp: None,
            b_p_efm: None,
            b_p_ep: None,
            b_p_path: None,
            b_p_ar: 0,
            b_p_tags: None,
            b_p_tc: None,
            b_tc_flags: 0,
            b_p_dict: None,
            b_p_dia: None,
            b_p_tsr: None,
            b_p_tsrfu: None,
            b_tsrfu_cb: Callback::default(),
            b_p_ul: 0,
            b_p_udf: 0,
            b_p_lw: None,
            b_ind_level: 0,
            b_ind_open_imag: 0,
            b_ind_no_brace: 0,
            b_ind_first_open: 0,
            b_ind_open_extra: 0,
            b_ind_close_extra: 0,
            b_ind_open_left_imag: 0,
            b_ind_jump_label: 0,
            b_ind_case: 0,
            b_ind_case_code: 0,
            b_ind_case_break: 0,
            b_ind_param: 0,
            b_ind_func_type: 0,
            b_ind_comment: 0,
            b_ind_in_comment: 0,
            b_ind_in_comment2: 0,
            b_ind_cpp_baseclass: 0,
            b_ind_continuation: 0,
            b_ind_unclosed: 0,
            b_ind_unclosed2: 0,
            b_ind_unclosed_noignore: 0,
            b_ind_unclosed_wrapped: 0,
            b_ind_unclosed_whiteok: 0,
            b_ind_matching_paren: 0,
            b_ind_paren_prev: 0,
            b_ind_maxparen: 0,
            b_ind_maxcomment: 0,
            b_ind_scopedecl: 0,
            b_ind_scopedecl_code: 0,
            b_ind_java: 0,
            b_ind_js: 0,
            b_ind_keep_case_label: 0,
            b_ind_hash_comment: 0,
            b_ind_cpp_namespace: 0,
            b_ind_if_for_while: 0,
            b_ind_cpp_extern_c: 0,
            b_ind_pragma: 0,
            b_no_eol_lnum: 0,
            b_start_eof: 0,
            b_start_eol: 0,
            b_start_ffc: 0,
            b_start_fenc: None,
            b_bad_char: 0,
            b_start_bomb: 0,
            b_bufvar: ScopeDictDictItem::default(),
            b_vars: std::ptr::null_mut(),
            b_may_swap: false,
            b_did_warn: false,
            b_help: false,
            b_spell: false,
            b_prompt_text: None,
            b_prompt_callback: Callback::default(),
            b_prompt_interrupt: Callback::default(),
            b_prompt_append_new_line: false,
            b_prompt_insert: 0,
            b_prompt_start: FmarkT::default(),
            b_s: SynblockT::default(),
            b_signcols: BufSigncolsT::default(),
            terminal: std::ptr::null_mut(),
            additional_data: std::ptr::null_mut(),
            b_mapped_ctrl_c: 0,
            b_marktree: MarkTree::default(),
            b_extmark_ns: Map::default(),
            b_prev_line_count: 0,
            update_channels: Vec::new(),
            update_callbacks: Vec::new(),
            update_need_codepoints: false,
            deleted_bytes: 0,
            deleted_bytes2: 0,
            deleted_codepoints: 0,
            deleted_codeunits: 0,
            flush_count: 0,
        }
    }
}

/// leaf frame, contains a window (`FR_LEAF`).
pub const FR_LEAF: u8 = 0;
/// frame with a row of windows (`FR_ROW`).
pub const FR_ROW: u8 = 1;
/// frame with a column of windows (`FR_COL`).
pub const FR_COL: u8 = 2;

/// Windows are kept in a tree of frames. Each frame has a column
/// (`FR_COL`) or row (`FR_ROW`) layout or is a leaf, which has a window
/// (`struct frame_S`, typedef'd as `frame_T`).
///
/// `fr_child`/`fr_win` are mutually exclusive in the original (an inner
/// frame has children, a leaf frame has a window); kept as two raw
/// pointer fields, not an enum, to stay a direct field-for-field mirror
/// of the original since nothing here enforces the invariant in Rust's
/// type system any more strongly than the original C did.
pub struct FrameT {
    /// `FR_LEAF`, `FR_COL` or `FR_ROW`
    pub fr_layout: u8,
    pub fr_width: i32,
    /// new width used in `win_equal_rec()`
    pub fr_newwidth: i32,
    pub fr_height: i32,
    /// new height used in `win_equal_rec()`
    pub fr_newheight: i32,
    /// containing frame or `NULL`
    pub fr_parent: *mut FrameT,
    /// frame right or below in same parent, `NULL` for last
    pub fr_next: *mut FrameT,
    /// frame left or above in same parent, `NULL` for first
    pub fr_prev: *mut FrameT,
    /// first contained frame
    pub fr_child: *mut FrameT,
    /// window that fills this frame; for a snapshot set to the current
    /// window
    pub fr_win: *mut WinT,
}

impl Default for FrameT {
    fn default() -> Self {
        FrameT {
            fr_layout: FR_LEAF,
            fr_width: 0,
            fr_newwidth: 0,
            fr_height: 0,
            fr_newheight: 0,
            fr_parent: std::ptr::null_mut(),
            fr_next: std::ptr::null_mut(),
            fr_prev: std::ptr::null_mut(),
            fr_child: std::ptr::null_mut(),
            fr_win: std::ptr::null_mut(),
        }
    }
}

/// Structure which contains all information that belongs to a window
/// (`struct window_S`, typedef'd as `win_T`).
///
/// All row numbers are relative to the start of the window, except
/// `w_winrow`.
///
/// Kept under the name `WinT` (matching `win_T`, the name used
/// throughout the rest of the original codebase), replacing the opaque
/// placeholder that previously lived in `types_defs.rs` - same treatment
/// as `BufT`/`struct file_buffer` above.
pub struct WinT {
    /// unique identifier for the window
    pub handle: HandleT,

    /// buffer we are a window into (used often; the original keeps it as
    /// the first struct member for a performance reason that no longer
    /// applies to a Rust struct, kept here only for field-order fidelity
    /// with the original).
    pub w_buffer: *mut BufT,

    /// for `":ownsyntax"`
    pub w_s: *mut SynblockT,

    pub w_ns_hl: i32,
    pub w_ns_hl_winhl: i32,
    pub w_ns_hl_active: i32,
    pub w_ns_hl_attr: *mut i32,

    pub w_ns_set: Set<u32>,

    /// `'winhighlight'` normal id
    pub w_hl_id_normal: i32,
    /// `'winhighlight'` normal final attrs
    pub w_hl_attr_normal: i32,
    /// `'winhighlight'` `NormalNC` final attrs
    pub w_hl_attr_normalnc: i32,

    /// attrs need to be recalculated
    pub w_hl_needs_update: i32,

    /// link to previous window
    pub w_prev: *mut WinT,
    /// link to next window
    pub w_next: *mut WinT,
    /// don't let autocommands close the window
    pub w_locked: i32,

    /// frame containing this window
    pub w_frame: *mut FrameT,

    /// cursor position in buffer
    pub w_cursor: PosT,

    /// Column we want to be at. This is used to try to stay in the same
    /// column for up/down cursor motions.
    pub w_curswant: ColnrT,

    /// If set, then update `w_curswant` the next time through
    /// `cursupdate()` to the current virtual column
    pub w_set_curswant: bool,

    /// Where `'cursorline'` should be drawn, can be different from
    /// `w_cursor.lnum` for closed folds.
    pub w_cursorline: LinenrT,
    /// where last `'cursorline'` was drawn
    pub w_last_cursorline: LinenrT,

    // The next seven are used to update the visual part.
    /// last known `Visual.mode`
    pub w_old_visual_mode: u8,
    /// last known end of visual part
    pub w_old_cursor_lnum: LinenrT,
    /// first column for block visual part
    pub w_old_cursor_fcol: ColnrT,
    /// last column for block visual part
    pub w_old_cursor_lcol: ColnrT,
    /// last known start of visual part
    pub w_old_visual_lnum: LinenrT,
    /// last known start of visual part
    pub w_old_visual_col: ColnrT,
    /// last known value of Curswant
    pub w_old_curswant: ColnrT,

    /// cursor lnum when `'rnu'` was last redrawn
    pub w_last_cursor_lnum_rnu: LinenrT,

    /// `'listchars'` characters. Defaults set in `set_chars_option()`.
    pub w_p_lcs_chars: LcsCharsT,

    /// `'fillchars'` characters. Defaults set in `set_chars_option()`.
    pub w_p_fcs_chars: FcsCharsT,

    // "w_topline", "w_leftcol" and "w_skipcol" specify the offsets for
    // displaying the buffer.
    /// buffer line number of the line at the top of the window
    pub w_topline: LinenrT,
    /// flag set to true when topline is set, e.g. by `winrestview()`
    pub w_topline_was_set: bool,
    /// number of filler lines above `w_topline`
    pub w_topfill: i32,
    /// `w_topfill` at last redraw
    pub w_old_topfill: i32,
    /// true when filler lines are actually below `w_topline` (at end of
    /// file)
    pub w_botfill: bool,
    /// `w_botfill` at last redraw
    pub w_old_botfill: bool,
    /// screen column number of the left most character in the window;
    /// used when `'wrap'` is off
    pub w_leftcol: ColnrT,
    /// starting screen column for the first line in the window; used
    /// when `'wrap'` is on; does not include `win_col_off()`
    pub w_skipcol: ColnrT,

    // Six fields that are only used when there is a WinScrolled
    // autocommand.
    /// last known value for `w_topline`
    pub w_last_topline: LinenrT,
    /// last known value for `w_topfill`
    pub w_last_topfill: i32,
    /// last known value for `w_leftcol`
    pub w_last_leftcol: ColnrT,
    /// last known value for `w_skipcol`
    pub w_last_skipcol: ColnrT,
    /// last known value for `w_width`
    pub w_last_width: i32,
    /// last known value for `w_height`
    pub w_last_height: i32,

    // Layout of the window in the screen. May need to add
    // "msg_scrolled" to "w_winrow" in rare situations.
    /// first row of window in screen
    pub w_winrow: i32,
    /// number of rows in window, excluding status/command line(s)
    pub w_height: i32,
    /// previous winrow used for `'splitkeep'`
    pub w_prev_winrow: i32,
    /// previous height used for `'splitkeep'`
    pub w_prev_height: i32,
    /// number of status lines (0 or 1)
    pub w_status_height: i32,
    /// number of window bars (0 or 1)
    pub w_winbar_height: i32,
    /// Leftmost column of window in screen.
    pub w_wincol: i32,
    /// Width of window, excluding separation.
    pub w_width: i32,
    /// Number of horizontal separator rows (0 or 1)
    pub w_hsep_height: i32,
    /// Number of vertical separator columns (0 or 1).
    pub w_vsep_width: i32,
    /// backup of cursor pos and topline
    pub w_save_cursor: PosSaveT,
    /// if true cursor may be invalid
    pub w_do_win_fix_cursor: bool,

    /// offset from winrow to the inner window area
    pub w_winrow_off: i32,
    /// offset from wincol to the inner window area; this includes float
    /// border but excludes special columns implemented in `win_line()`
    /// (i.e. signs, folds, numbers)
    pub w_wincol_off: i32,

    // Size of the window viewport. This is the area usable to draw
    // columns and buffer contents.
    pub w_view_height: i32,
    pub w_view_width: i32,
    // External UI request. If non-zero, the inner size will use this.
    pub w_height_request: i32,
    pub w_width_request: i32,

    /// top, right, bottom, left
    pub w_border_adj: [i32; 4],
    // outer size of window grid, including border
    pub w_height_outer: i32,
    pub w_width_outer: i32,

    // === start of cached values ====
    // Recomputing is minimized by storing the result of computations.
    // Use functions in screen.c to check if they are valid and to
    // update. w_valid is a bitfield of flags, which indicate if specific
    // values are valid or need to be recomputed.
    pub w_valid: i32,
    /// last known position of `w_cursor`, used to adjust `w_valid`
    pub w_valid_cursor: PosT,
    /// last known `w_leftcol`
    pub w_valid_leftcol: ColnrT,
    /// last known `w_skipcol`
    pub w_valid_skipcol: ColnrT,

    pub w_viewport_invalid: bool,
    /// topline when the viewport was last updated
    pub w_viewport_last_topline: LinenrT,
    /// botline when the viewport was last updated
    pub w_viewport_last_botline: LinenrT,
    /// topfill when the viewport was last updated
    pub w_viewport_last_topfill: LinenrT,
    /// skipcol when the viewport was last updated
    pub w_viewport_last_skipcol: LinenrT,

    // w_cline_height is the number of physical lines taken by the
    // buffer line that the cursor is on. We use this to avoid extra
    // calls to plines_win().
    /// current size of cursor line
    pub w_cline_height: i32,
    /// cursor line is folded
    pub w_cline_folded: bool,

    /// starting row of the cursor line
    pub w_cline_row: i32,

    /// column number of the cursor in the buffer line, as opposed to the
    /// column number we're at on the screen. This makes a difference on
    /// lines which span more than one screen line or when `w_leftcol` is
    /// non-zero
    pub w_virtcol: ColnrT,

    // w_wrow and w_wcol specify the cursor position in the window. This
    // is related to positions in the window, not in the display or
    // buffer, thus w_wrow is relative to w_winrow.
    /// cursor position in window
    pub w_wrow: i32,
    pub w_wcol: i32,
    /// screen cells concealed before `w_wcol` on the cursor's screen
    /// line, set by `win_line()`
    pub w_wcol_conceal_off: i32,

    /// number of the line below the bottom of the window
    pub w_botline: LinenrT,
    /// number of ~ rows in window
    pub w_empty_rows: i32,
    /// number of filler rows at the end of the window
    pub w_filler_rows: i32,

    // Info about the lines currently in the window is remembered to
    // avoid recomputing it every time. The allocated size of w_lines[]
    // is Rows. Only the w_lines_valid entries are actually valid. When
    // the display is up-to-date w_lines[0].wl_lnum is equal to w_topline
    // and w_lines[w_lines_valid - 1].wl_lnum is equal to w_botline.
    // Between changing text and updating the display w_lines[]
    // represents what is currently displayed. wl_valid is reset to
    // indicate this. This is used for efficient redrawing.
    /// number of valid entries
    pub w_lines_valid: i32,
    /// in place of the original's raw `wline_T *w_lines` pointer to a
    /// heap-allocated array; `w_lines_size` (the *allocated* capacity,
    /// "Rows"-sized) is dropped as redundant with `Vec::len()` - unlike
    /// `w_lines_valid` above, which is a distinct *logical validity*
    /// count (may be less than the allocated/current length) and is
    /// kept as its own field.
    pub w_lines: Vec<WlineT>,

    /// array of nested folds
    pub w_folds: GarrayT,
    /// when true: some folds are opened/closed manually
    pub w_fold_manual: bool,
    /// when true: folding needs to be recomputed
    pub w_foldinvalid: bool,
    /// width of `'number'` and `'relativenumber'` column being used
    pub w_nrwidth: i32,
    /// width of `'signcolumn'`
    pub w_scwidth: i32,
    /// minimum width or `SCL_NO`/`SCL_NUM`
    pub w_minscwidth: i32,
    /// maximum width or `SCL_NO`/`SCL_NUM`
    pub w_maxscwidth: i32,
    // === end of cached values ===
    /// type of redraw to be performed on win
    pub w_redr_type: i32,
    /// number of window lines to update when `w_redr_type` is
    /// `UPD_REDRAW_TOP`
    pub w_upd_rows: i32,
    /// when != 0: first line needing redraw
    pub w_redraw_top: LinenrT,
    /// when != 0: last line needing redraw
    pub w_redraw_bot: LinenrT,
    /// if true statusline/winbar must be redrawn
    pub w_redr_status: bool,
    /// if true border must be redrawn
    pub w_redr_border: bool,
    /// if true `'statuscolumn'` must be redrawn
    pub w_redr_statuscol: bool,
    /// when window was last drawn.
    pub w_display_tick: DisptickT,

    // Remember what is shown in the 'statusline'-format elements.
    /// cursor position when last redrawn
    pub w_stl_cursor: PosT,
    /// virtcol when last redrawn
    pub w_stl_virtcol: ColnrT,
    /// topline when last redrawn
    pub w_stl_topline: LinenrT,
    /// line count when last redrawn
    pub w_stl_line_count: LinenrT,
    /// topfill when last redrawn
    pub w_stl_topfill: i32,
    /// true if elements show 0-1 (empty line) (`char` in the original,
    /// but assigned exactly from a `bool` at every real call site -
    /// `curwin->w_stl_empty = (char)empty_line;` in `drawscreen.c` -
    /// verified before choosing `bool` over a `u8`/byte translation).
    pub w_stl_empty: bool,
    /// `reg_recording` when last redrawn
    pub w_stl_recording: i32,
    /// `get_real_state()` when last redrawn
    pub w_stl_state: i32,
    /// `Visual.mode` when last redrawn
    pub w_stl_visual_mode: i32,
    /// `Visual.start` when last redrawn
    pub w_stl_visual_pos: PosT,

    /// alternate file (for # and CTRL-^)
    pub w_alt_fnum: i32,

    /// pointer to arglist for this window
    pub w_alist: *mut AlistT,
    /// current index in argument list (can be out of range!)
    pub w_arg_idx: i32,
    /// editing another file than `w_arg_idx`
    pub w_arg_idx_invalid: bool,

    /// absolute path of local directory or `NULL`
    pub w_localdir: Option<Vec<u8>>,
    /// previous directory
    pub w_prevdir: Option<Vec<u8>>,

    // Options local to a window. They are local because they influence
    // the layout of the window or depend on the window layout. There
    // are two values: w_onebuf_opt is local to the buffer currently in
    // this window, w_allbuf_opt is for all buffers in this window.
    pub w_onebuf_opt: WinoptT,
    pub w_allbuf_opt: WinoptT,

    /// array of columns to highlight or `NULL`
    pub w_p_cc_cols: Option<Vec<i32>>,
    /// flags for cursorline highlighting
    pub w_p_culopt_flags: u8,

    /// minimum width for breakindent
    pub w_briopt_min: i32,
    /// additional shift for breakindent
    pub w_briopt_shift: i32,
    /// sbr in `'briopt'`
    pub w_briopt_sbr: bool,
    /// additional indent for lists
    pub w_briopt_list: i32,
    /// indent for specific column
    pub w_briopt_vcol: i32,

    pub w_scbind_pos: i32,

    /// Variable for "w:" dict.
    pub w_winvar: ScopeDictDictItem,
    /// Dict with w: variables.
    pub w_vars: *mut DictT,

    // The w_prev_pcmark field is used to check whether we really did
    // jump to a new line after setting the w_pcmark. If not, then we
    // revert to using the previous w_pcmark.
    /// previous context mark
    pub w_pcmark: PosT,
    /// previous `w_pcmark`
    pub w_prev_pcmark: PosT,

    // The jumplist contains old cursor positions.
    pub w_jumplist: [XfmarkT; JUMPLISTSIZE as usize],
    /// number of active entries
    pub w_jumplistlen: i32,
    /// current position
    pub w_jumplistidx: i32,

    /// current position in `b_changelist`
    pub w_changelistidx: i32,

    /// head of match list
    pub w_match_head: *mut MatchitemT,
    /// next match ID
    pub w_next_match_id: i32,

    // The tagstack grows from 0 upwards:
    // entry 0: older
    // entry 1: newer
    // entry 2: newest
    /// the tag stack
    pub w_tagstack: [TaggyT; TAGSTACKSIZE as usize],
    /// idx just below active entry
    pub w_tagstackidx: i32,
    /// number of tags on stack
    pub w_tagstacklen: i32,

    /// area to draw on, excluding borders and winbar
    pub w_grid: GridView,
    /// the grid specific to the window
    pub w_grid_alloc: ScreenGrid,
    /// true if window position changed
    pub w_pos_changed: bool,
    /// whether the window is floating
    pub w_floating: bool,
    /// mutually-exclusive window role
    pub w_kind: WinKind,
    pub w_config: WinConfig,

    // w_fraction is the fractional row of the cursor within the window,
    // from 0 at the top row to FRACTION_MULT at the last row.
    // w_prev_fraction_row was the actual cursor row when w_fraction was
    // last calculated.
    pub w_fraction: i32,
    pub w_prev_fraction_row: i32,

    /// line count when `ml_nrwidth_width` was computed.
    pub w_nrwidth_line_count: LinenrT,
    /// line count when `'statuscolumn'` width was computed.
    pub w_statuscol_line_count: LinenrT,
    /// nr of chars to print line count.
    pub w_nrwidth_width: i32,

    /// Location list for this window
    pub w_llist: *mut QfInfoT,
    /// Location list reference used in the location list window. In a
    /// non-location list window, `w_llist_ref` is `NULL`.
    pub w_llist_ref: *mut QfInfoT,

    /// Status line click definitions (in place of the original's raw
    /// `StlClickDefinition *w_status_click_defs` pointer + separate
    /// `w_status_click_defs_size` - folded into a single `Vec`, dropping
    /// the now-redundant size field, same as `UEntry.ue_array`/`ue_size`
    /// in `undo_defs.rs`).
    pub w_status_click_defs: Vec<StlClickDefinition>,
    /// Window bar click definitions (same `Vec`-folding as
    /// `w_status_click_defs` above).
    pub w_winbar_click_defs: Vec<StlClickDefinition>,
    /// Map of statuscolumn click definitions, indexed by `v:lnum` and
    /// `v:virtnum` (`Map(int, StcClicks) w_statuscol_click_defs[1]` in
    /// the original - the same single-element-array-for-by-value idiom
    /// as `BufT.b_marktree`/`b_extmark_ns`, translated as a plain
    /// by-value field).
    pub w_statuscol_click_defs: Map<i32, StcClicks>,
}

impl Default for WinT {
    /// A purely mechanical, structural default, same caveat as
    /// `BufT`'s: not a translation of `win_alloc()`'s real "freshly
    /// created window" initial state (which needs e.g. option.c's
    /// defaults table for `w_onebuf_opt`/`w_allbuf_opt`, phase 4).
    fn default() -> Self {
        WinT {
            handle: 0,
            w_buffer: std::ptr::null_mut(),
            w_s: std::ptr::null_mut(),
            w_ns_hl: 0,
            w_ns_hl_winhl: 0,
            w_ns_hl_active: 0,
            w_ns_hl_attr: std::ptr::null_mut(),
            w_ns_set: Set::default(),
            w_hl_id_normal: 0,
            w_hl_attr_normal: 0,
            w_hl_attr_normalnc: 0,
            w_hl_needs_update: 0,
            w_prev: std::ptr::null_mut(),
            w_next: std::ptr::null_mut(),
            w_locked: 0,
            w_frame: std::ptr::null_mut(),
            w_cursor: PosT::default(),
            w_curswant: 0,
            w_set_curswant: false,
            w_cursorline: 0,
            w_last_cursorline: 0,
            w_old_visual_mode: 0,
            w_old_cursor_lnum: 0,
            w_old_cursor_fcol: 0,
            w_old_cursor_lcol: 0,
            w_old_visual_lnum: 0,
            w_old_visual_col: 0,
            w_old_curswant: 0,
            w_last_cursor_lnum_rnu: 0,
            w_p_lcs_chars: LcsCharsT::default(),
            w_p_fcs_chars: FcsCharsT::default(),
            w_topline: 0,
            w_topline_was_set: false,
            w_topfill: 0,
            w_old_topfill: 0,
            w_botfill: false,
            w_old_botfill: false,
            w_leftcol: 0,
            w_skipcol: 0,
            w_last_topline: 0,
            w_last_topfill: 0,
            w_last_leftcol: 0,
            w_last_skipcol: 0,
            w_last_width: 0,
            w_last_height: 0,
            w_winrow: 0,
            w_height: 0,
            w_prev_winrow: 0,
            w_prev_height: 0,
            w_status_height: 0,
            w_winbar_height: 0,
            w_wincol: 0,
            w_width: 0,
            w_hsep_height: 0,
            w_vsep_width: 0,
            w_save_cursor: PosSaveT::default(),
            w_do_win_fix_cursor: false,
            w_winrow_off: 0,
            w_wincol_off: 0,
            w_view_height: 0,
            w_view_width: 0,
            w_height_request: 0,
            w_width_request: 0,
            w_border_adj: [0; 4],
            w_height_outer: 0,
            w_width_outer: 0,
            w_valid: 0,
            w_valid_cursor: PosT::default(),
            w_valid_leftcol: 0,
            w_valid_skipcol: 0,
            w_viewport_invalid: false,
            w_viewport_last_topline: 0,
            w_viewport_last_botline: 0,
            w_viewport_last_topfill: 0,
            w_viewport_last_skipcol: 0,
            w_cline_height: 0,
            w_cline_folded: false,
            w_cline_row: 0,
            w_virtcol: 0,
            w_wrow: 0,
            w_wcol: 0,
            w_wcol_conceal_off: 0,
            w_botline: 0,
            w_empty_rows: 0,
            w_filler_rows: 0,
            w_lines_valid: 0,
            w_lines: Vec::new(),
            w_folds: GarrayT::default(),
            w_fold_manual: false,
            w_foldinvalid: false,
            w_nrwidth: 0,
            w_scwidth: 0,
            w_minscwidth: 0,
            w_maxscwidth: 0,
            w_redr_type: 0,
            w_upd_rows: 0,
            w_redraw_top: 0,
            w_redraw_bot: 0,
            w_redr_status: false,
            w_redr_border: false,
            w_redr_statuscol: false,
            w_display_tick: 0,
            w_stl_cursor: PosT::default(),
            w_stl_virtcol: 0,
            w_stl_topline: 0,
            w_stl_line_count: 0,
            w_stl_topfill: 0,
            w_stl_empty: false,
            w_stl_recording: 0,
            w_stl_state: 0,
            w_stl_visual_mode: 0,
            w_stl_visual_pos: PosT::default(),
            w_alt_fnum: 0,
            w_alist: std::ptr::null_mut(),
            w_arg_idx: 0,
            w_arg_idx_invalid: false,
            w_localdir: None,
            w_prevdir: None,
            w_onebuf_opt: WinoptT::default(),
            w_allbuf_opt: WinoptT::default(),
            w_p_cc_cols: None,
            w_p_culopt_flags: 0,
            w_briopt_min: 0,
            w_briopt_shift: 0,
            w_briopt_sbr: false,
            w_briopt_list: 0,
            w_briopt_vcol: 0,
            w_scbind_pos: 0,
            w_winvar: ScopeDictDictItem::default(),
            w_vars: std::ptr::null_mut(),
            w_pcmark: PosT::default(),
            w_prev_pcmark: PosT::default(),
            w_jumplist: std::array::from_fn(|_| XfmarkT::default()),
            w_jumplistlen: 0,
            w_jumplistidx: 0,
            w_changelistidx: 0,
            w_match_head: std::ptr::null_mut(),
            w_next_match_id: 0,
            w_tagstack: std::array::from_fn(|_| TaggyT::default()),
            w_tagstackidx: 0,
            w_tagstacklen: 0,
            w_grid: GridView::default(),
            w_grid_alloc: ScreenGrid::default(),
            w_pos_changed: false,
            w_floating: false,
            w_kind: WinKind::default(),
            w_config: WinConfig::default(),
            w_fraction: 0,
            w_prev_fraction_row: 0,
            w_nrwidth_line_count: 0,
            w_statuscol_line_count: 0,
            w_nrwidth_width: 0,
            w_llist: std::ptr::null_mut(),
            w_llist_ref: std::ptr::null_mut(),
            w_status_click_defs: Vec::new(),
            w_winbar_click_defs: Vec::new(),
            w_statuscol_click_defs: Map::default(),
        }
    }
}

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

    #[test]
    fn win_config_default_matches_win_config_init() {
        let wc = WinConfig::default();
        assert_eq!(wc.height, 0);
        assert_eq!(wc.width, 0);
        assert_eq!(wc.bufpos.lnum, -1);
        assert_eq!(wc.bufpos.col, 0);
        assert_eq!(wc.row, 0.0);
        assert_eq!(wc.col, 0.0);
        assert_eq!(wc.relative, FloatRelative::Editor);
        assert!(!wc.external);
        assert!(wc.focusable);
        assert!(wc.mouse);
        assert_eq!(wc.split, WinSplit::Left);
        assert_eq!(wc.zindex, crate::grid_defs::K_ZINDEX_FLOAT_DEFAULT);
        assert_eq!(wc.style, WinStyle::Unused);
        assert!(!wc.noautocmd);
        assert!(!wc.hide);
        assert!(!wc.fixed);
        assert_eq!(wc._cmdline_offset, i32::MAX);
    }

    #[test]
    fn float_relative_default_is_editor() {
        assert_eq!(FloatRelative::default(), FloatRelative::Editor);
        assert_eq!(FloatRelative::Laststatus as i32, 5);
    }

    #[test]
    fn win_split_discriminants_match_c_enum() {
        assert_eq!(WinSplit::Left as i32, 0);
        assert_eq!(WinSplit::Right as i32, 1);
        assert_eq!(WinSplit::Above as i32, 2);
        assert_eq!(WinSplit::Below as i32, 3);
    }

    #[test]
    fn pos_save_default_is_zeroed() {
        let ps = PosSaveT::default();
        assert_eq!(ps.w_topline_save, 0);
        assert_eq!(ps.w_cursor_save.lnum, 0);
    }

    #[test]
    fn lcs_and_fcs_chars_default_have_no_multispace() {
        let lcs = LcsCharsT::default();
        assert!(lcs.multispace.is_none());
        assert!(lcs.leadmultispace.is_none());
        let fcs = FcsCharsT::default();
        assert_eq!(fcs.stl, 0);
    }

    #[test]
    fn diffline_change_default_is_zeroed() {
        let dc = DifflineChangeT::default();
        assert_eq!(dc.dc_start, [0; DB_COUNT]);
        assert_eq!(dc.dc_end, [0; DB_COUNT]);
    }

    #[test]
    fn diffline_default_has_no_changes() {
        let dl = DifflineT::default();
        assert!(dl.changes.is_empty());
        assert_eq!(dl.bufidx, 0);
    }

    #[test]
    fn wline_default_is_invalid_and_zeroed() {
        let wl = WlineT::default();
        assert!(!wl.wl_valid);
        assert!(!wl.wl_folded);
        assert_eq!(wl.wl_lnum, 0);
    }

    #[test]
    fn snap_indices_are_distinct() {
        let all = [SNAP_HELP_IDX, SNAP_AUCMD_IDX, SNAP_QUICKFIX_IDX];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j]);
            }
        }
        assert_eq!(SNAP_COUNT, 3);
    }

    #[test]
    fn synblock_default_has_empty_hashtabs_and_zeroed_arrays() {
        let sb = SynblockT::default();
        assert_eq!(sb.b_keywtab.ht_used, 0);
        assert_eq!(sb.b_keywtab_ic.ht_used, 0);
        assert!(sb.b_spell_ismw.iter().all(|&b| !b));
        assert!(sb.b_syn_chartab.iter().all(|&b| b == 0));
        assert!(sb.b_syn_linecont_prog.is_null());
    }

    #[test]
    fn buf_signcols_default_is_zeroed() {
        let sc = BufSigncolsT::default();
        assert_eq!(sc.max, 0);
        assert_eq!(sc.last_max, 0);
        assert!(sc.count.iter().all(|&c| c == 0));
        assert!(!sc.autom);
        assert_eq!(sc.count.len(), SIGN_SHOW_MAX as usize);
    }

    #[test]
    fn imode_and_keymap_constants_match_c_macros() {
        assert_eq!(B_IMODE_USE_INSERT, -1);
        assert_eq!(B_IMODE_NONE, 0);
        assert_eq!(B_IMODE_LMAP, 1);
        assert_eq!(B_IMODE_LAST, 1);
        assert_eq!(KEYMAP_INIT, 1);
        assert_eq!(KEYMAP_LOADED, 2);
    }

    #[test]
    fn file_buffer_default_has_null_links_and_empty_collections() {
        let buf = BufT::default();
        assert_eq!(buf.handle, 0);
        assert!(buf.b_next.is_null());
        assert!(buf.b_prev.is_null());
        assert!(buf.b_ffname.is_none());
        assert!(!buf.file_id_valid);
        assert_eq!(buf.file_id, crate::os::fs_defs::FileID::empty());
        assert!(buf.b_wininfo.is_empty());
        assert_eq!(buf.b_namedm.len(), NMARKS as usize);
        assert_eq!(buf.b_changelist.len(), JUMPLISTSIZE as usize);
        assert_eq!(buf.b_maphash.len(), MAX_MAPHASH as usize);
        assert!(buf.b_maphash.iter().all(|p| p.is_null()));
        assert!(buf.b_first_abbr.is_null());
        assert!(buf.b_u_oldhead.is_null());
        assert!(buf.terminal.is_null());
        assert!(buf.additional_data.is_null());
        assert!(buf.b_vars.is_null());
        assert_eq!(buf.b_marktree.n_keys, 0);
        assert_eq!(buf.b_extmark_ns.len(), 0);
        assert!(buf.update_channels.is_empty());
        assert!(buf.update_callbacks.is_empty());
        assert_eq!(buf.deleted_bytes, 0);
        assert_eq!(buf.flush_count, 0);
    }

    #[test]
    fn file_buffer_default_callbacks_are_none() {
        let buf = BufT::default();
        assert_eq!(buf.b_cfu_cb.kind(), crate::eval::typval_defs::CallbackType::None);
        assert_eq!(buf.b_ofu_cb.kind(), crate::eval::typval_defs::CallbackType::None);
        assert_eq!(buf.b_tfu_cb.kind(), crate::eval::typval_defs::CallbackType::None);
        assert_eq!(buf.b_ffu_cb.kind(), crate::eval::typval_defs::CallbackType::None);
        assert_eq!(buf.b_prompt_callback.kind(), crate::eval::typval_defs::CallbackType::None);
        assert!(buf.b_p_cpt_cb.is_empty());
    }

    #[test]
    fn frame_default_is_leaf_with_null_pointers() {
        let f = FrameT::default();
        assert_eq!(f.fr_layout, FR_LEAF);
        assert!(f.fr_parent.is_null());
        assert!(f.fr_next.is_null());
        assert!(f.fr_prev.is_null());
        assert!(f.fr_child.is_null());
        assert!(f.fr_win.is_null());
    }

    #[test]
    fn frame_layout_constants_are_distinct() {
        assert_eq!(FR_LEAF, 0);
        assert_eq!(FR_ROW, 1);
        assert_eq!(FR_COL, 2);
    }

    #[test]
    fn win_default_has_null_links_and_empty_collections() {
        let win = WinT::default();
        assert_eq!(win.handle, 0);
        assert!(win.w_buffer.is_null());
        assert!(win.w_s.is_null());
        assert!(win.w_prev.is_null());
        assert!(win.w_next.is_null());
        assert!(win.w_frame.is_null());
        assert_eq!(win.w_ns_set.len(), 0);
        assert!(win.w_lines.is_empty());
        assert_eq!(win.w_lines_valid, 0);
        assert_eq!(win.w_jumplist.len(), JUMPLISTSIZE as usize);
        assert_eq!(win.w_tagstack.len(), TAGSTACKSIZE as usize);
        assert!(win.w_alist.is_null());
        assert!(win.w_localdir.is_none());
        assert!(win.w_vars.is_null());
        assert!(win.w_match_head.is_null());
        assert!(win.w_llist.is_null());
        assert!(win.w_llist_ref.is_null());
        assert!(win.w_status_click_defs.is_empty());
        assert!(win.w_winbar_click_defs.is_empty());
        assert_eq!(win.w_statuscol_click_defs.len(), 0);
        assert_eq!(win.w_kind, WinKind::Normal);
        assert!(!win.w_floating);
    }

    #[test]
    fn win_default_cursor_and_visual_state_is_zeroed() {
        let win = WinT::default();
        assert_eq!(win.w_cursor, PosT::default());
        assert_eq!(win.w_old_visual_mode, 0);
        assert!(!win.w_set_curswant);
        assert_eq!(win.w_config.height, 0);
        assert!(win.w_config.focusable);
        assert!(win.w_config.mouse);
    }
}
