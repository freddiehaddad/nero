//! Translated from `src/nvim/types_defs.h`.

use std::os::raw::c_char;

/// dummy to pass an ACL to a function (`vim_acl_T`)
pub type VimAclT = *mut std::ffi::c_void;

/// if data[0] is 0xFF, then data[1..4] is a 24-bit index (in machine endianness)
/// otherwise it must be a UTF-8 string of length maximum 4 (no NUL when n=4)
pub type ScharT = u32;
pub type SattrT = i32;
/// must be at least as big as the biggest of schar_T, sattr_T, colnr_T
pub type SscratchT = i32;

/// Includes final NUL. MAX_MCO is no longer used, but at least 4*(MAX_MCO+1)+1=29
/// ensures we can fit all composed chars which did fit before.
pub const MAX_SCHAR_SIZE: usize = 32;

/// Opaque handle used by API clients to refer to various objects in vim
pub type HandleT = i32;

/// Opaque handle to a lua value. Must be freed with `api_free_luaref` when
/// not needed anymore! `LUA_NOREF` represents a missing reference, i.e. to
/// indicate an absent callback etc.
pub type LuaRef = i32;

/// Type used for Vimscript `VAR_FLOAT` values
pub type FloatT = f64;

/// Forward-declared in the original header; the real definition lives in
/// `src/nvim/msgpack_rpc/*` (not yet translated - phase 11).
pub struct MsgpackRpcRequestHandler {
    _private: (),
}

/// vimfn metadata defined in `src/nvim/eval.lua`.
pub union EvalFuncData {
    pub func_float: Option<extern "C" fn(FloatT) -> FloatT>,
    /// Vimscript bridge to API fn (eval=true in eval.lua).
    pub func_api: *const MsgpackRpcRequestHandler,
    /// Lua-implemented vimfn.
    pub func_lua: *const c_char,
    pub null: *mut std::ffi::c_void,
}

pub type Ns = HandleT;

pub type ProftimeT = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i8)]
pub enum TriState {
    #[default]
    None = -1,
    False = 0,
    True = 1,
}

/// `TRISTATE_TO_BOOL(val, default)` macro.
#[inline]
pub fn tristate_to_bool(val: TriState, default: bool) -> bool {
    match val {
        TriState::True => true,
        TriState::False => false,
        TriState::None => default,
    }
}

/// `TRISTATE_FROM_INT(val)` macro.
#[inline]
pub fn tristate_from_int(val: i64) -> TriState {
    if val == 0 {
        TriState::False
    } else if val >= 1 {
        TriState::True
    } else {
        TriState::None
    }
}

pub type OptInt = i64;

/// Number of display cells for a sign in the signcolumn (`SIGN_WIDTH`).
pub const SIGN_WIDTH: i32 = 2;

// The following are opaque forward declarations in the original C header;
// each becomes a real type when its owning file is translated. Kept as
// opaque placeholder structs until then - never silently faked, just not
// yet implemented:
//   Loop        -> struct loop,        src/nvim/event/loop.h    (phase 11)
//   regprog_T   -> struct regprog,     src/nvim/regexp_defs.h   (phase 7)
//   regmatch_T  -> struct regmatch,    src/nvim/regexp_defs.h   (phase 7)
//   synstate_T  -> struct syn_state,   src/nvim/syntax_defs.h   (phase 8)
//   Terminal    -> struct terminal,    src/nvim/terminal.h      (phase 14)
//   qf_info_T   -> struct qf_info_S,   src/nvim/quickfix.c      (phase 8)
//   mapblock_T  -> struct mapblock,    src/nvim/mapping_defs.h  (phase 7)
//   matchitem_T -> struct matchitem,   src/nvim/buffer_defs.h   (phase 7,
//                                      needs regmmatch_T)
//   AutoPatCmd  -> struct AutoPatCmd_S, src/nvim/autocmd_defs.h (phase 6)
//   expand_T    -> struct expand,      src/nvim/cmdexpand_defs.h (phase 7)
// (mapblock_T/qf_info_T/matchitem_T are actually forward-declared in their
// own headers, not types_defs.h itself, unlike the others above - but this
// crate keeps all such opaque cross-cutting placeholders here regardless
// of exactly which original header contains the forward declaration,
// since Rust has no forward-declaration mechanism of its own to mirror
// precisely.)
// MTNode (struct mtnode_s) is no longer a placeholder here: it is now
// translated for real in `src/nvim/marktree_defs.h` -> `crate::marktree_defs::MtNode`.
// buf_T (struct file_buffer) and win_T (struct window_S) are likewise no
// longer placeholders: they are now translated for real as
// `crate::buffer_defs::BufT`/`crate::buffer_defs::WinT` (kept under the
// same names, since `buf_T`/`win_T` - not `FileBuffer`/`Window` - are the
// names actually used throughout the rest of the original codebase;
// matches this crate's "prefer the real typedef name" convention, e.g.
// `wininfo_S` -> `WinInfo`). tabpage_T (struct tabpage_S) is likewise no
// longer a placeholder: now that dict_T has real fields, it is translated
// for real as `crate::buffer_defs::TabpageT`. ufunc_T (struct ufunc_S) is
// likewise no longer a placeholder: it has no flexible-array-member
// complication of its own (unlike dictitem_T/its own uf_name field, which
// is instead an owned Vec<u8> here, same treatment), and is translated for
// real as `crate::eval::typval_defs::UfuncT` (its own home, alongside
// `PartialT`/`DictT`/`ListT` - the rest of the eval engine's value types -
// rather than here, since it is eval-engine-scoped, not a cross-cutting
// type genuinely needed by unrelated subsystems the way the others above
// still are).

/// Placeholder for `Loop` (`struct loop`) - see `src/nvim/event/loop.h` (phase 11).
pub struct LoopT {
    _private: (),
}
/// Conversion mode stored in [`VimconvT`] (`ConvFlags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum ConvFlags {
    #[default]
    None = 0,
    ToUtf8 = 1,
    Latin9ToUtf8 = 2,
    ToLatin1 = 3,
    ToLatin9 = 4,
    Iconv = 5,
}

/// Character-encoding conversion state (`vimconv_T`).
#[derive(Debug)]
pub struct VimconvT {
    pub vc_type: ConvFlags,
    pub vc_factor: i32,
    pub vc_fd: *mut std::ffi::c_void,
    pub vc_fail: bool,
}

impl Default for VimconvT {
    /// `MBYTE_NONE_CONV`.
    fn default() -> Self {
        Self {
            vc_type: ConvFlags::None,
            vc_factor: 1,
            vc_fd: std::ptr::null_mut(),
            vc_fail: false,
        }
    }
}
/// Common compiled-regexp header (`struct regprog`, `regexp.c`).
#[derive(Debug)]
pub struct RegprogT {
    pub engine: *mut crate::regexp_defs::RegengineT,
    pub regflags: u32,
    /// Automatic, backtracking or NFA engine (`re_engine`).
    pub re_engine: u32,
    /// Second argument passed to `vim_regcomp()` (`re_flags`).
    pub re_flags: u32,
    /// Whether the program is currently executing (`re_in_use`).
    pub re_in_use: bool,
}

impl Default for RegprogT {
    fn default() -> Self {
        Self {
            engine: std::ptr::null_mut(),
            regflags: 0,
            re_engine: 0,
            re_flags: 0,
            re_in_use: false,
        }
    }
}
/// One entry in a saved syntax state stack (`bufstate_T`).
#[derive(Debug, Clone, Copy, Default)]
pub struct BufstateT {
    pub bs_idx: i32,
    pub bs_flags: i32,
    pub bs_seqnr: i32,
    pub bs_cchar: i32,
    pub bs_extmatch: *mut RegExtmatchT,
}

/// Number of inline saved syntax states (`SST_FIX_STATES`).
pub const SST_FIX_STATES: usize = 7;

/// Storage union in [`SynstateT`] (`sst_union`).
#[derive(Debug)]
pub enum SynstateStorage {
    Fixed([BufstateT; SST_FIX_STATES]),
    Dynamic(Vec<BufstateT>),
}

impl Default for SynstateStorage {
    fn default() -> Self {
        Self::Fixed([BufstateT::default(); SST_FIX_STATES])
    }
}

/// Saved syntax state at the start of one line (`synstate_T`).
#[derive(Debug, Default)]
pub struct SynstateT {
    pub sst_next: *mut SynstateT,
    pub sst_lnum: crate::pos_defs::LinenrT,
    pub sst_storage: SynstateStorage,
    pub sst_next_flags: i32,
    pub sst_stacksize: i32,
    pub sst_next_list: *const i16,
    pub sst_tick: crate::buffer_defs::DisptickT,
    pub sst_change_lnum: crate::pos_defs::LinenrT,
}
/// Cursor state cached by `struct terminal`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerminalCursorT {
    pub row: i32,
    pub col: i32,
    pub shape: i32,
    pub visible: bool,
    pub blink: bool,
}

/// Partial translation of `Terminal` (`struct terminal`).
#[derive(Debug, Default)]
pub struct TerminalT {
    /// Lines currently stored in scrollback (`sb_current`).
    pub sb_current: usize,
    /// Buffer handle owning this terminal (`buf_handle`).
    pub buf_handle: HandleT,
    /// Whether the child process is suspended (`suspended`).
    pub suspended: bool,
    /// Whether the terminal has closed (`closed`).
    pub closed: bool,
    /// Cursor position and presentation requested by libvterm (`cursor`).
    pub cursor: TerminalCursorT,
}
/// Quickfix/location list stack (`qf_info_T`, `struct qf_info_S`) -
/// a stack of quickfix/location lists.
#[derive(Debug, Default)]
pub struct QfInfoT {
    /// Reference count, used only for location lists (`qf_refcount`).
    ///
    /// A location list window referencing this list makes it 2,
    /// otherwise 1; the list is freed when it reaches 0.
    pub qf_refcount: i32,
    /// Current number of lists (`qf_listcount`).
    pub qf_listcount: i32,
    /// Index of the current error list (`qf_curlist`).
    pub qf_curlist: i32,
    /// Maximum number of lists (`qf_maxcount`).
    pub qf_maxcount: i32,
    /// The lists themselves (`qf_lists`).
    pub qf_lists: Vec<crate::quickfix::QfListT>,
    /// Whether this is a quickfix or location list stack (`qfl_type`).
    pub qfl_type: crate::quickfix::QfltypeT,
    /// Quickfix window buffer number (`qf_bufnr`).
    pub qf_bufnr: i32,
}
/// One mapping or abbreviation (`mapblock_T`).
#[derive(Debug)]
pub struct MapblockT {
    pub m_next: *mut MapblockT,
    pub m_alt: *mut MapblockT,
    pub m_keys: Vec<u8>,
    pub m_str: Option<Vec<u8>>,
    pub m_orig_str: Option<Vec<u8>>,
    pub m_luaref: LuaRef,
    pub m_keylen: i32,
    pub m_mode: i32,
    pub m_simplified: i32,
    pub m_noremap: i32,
    pub m_silent: u8,
    pub m_nowait: u8,
    pub m_expr: u8,
    pub m_script_ctx: crate::eval::typval_defs::SctxT,
    pub m_desc: Option<Vec<u8>>,
    pub m_replace_keycodes: bool,
}

impl Default for MapblockT {
    fn default() -> Self {
        Self {
            m_next: std::ptr::null_mut(),
            m_alt: std::ptr::null_mut(),
            m_keys: Vec::new(),
            m_str: None,
            m_orig_str: None,
            m_luaref: -1,
            m_keylen: 0,
            m_mode: 0,
            m_simplified: 0,
            m_noremap: 0,
            m_silent: 0,
            m_nowait: 0,
            m_expr: 0,
            m_script_ctx: crate::eval::typval_defs::SctxT::default(),
            m_desc: None,
            m_replace_keycodes: false,
        }
    }
}
/// Placeholder for `AutoPatCmd` (`struct AutoPatCmd_S`) - see
/// `src/nvim/autocmd_defs.h` (phase 6).
pub struct AutoPatCmdT {
    _private: (),
}
/// Placeholder for `regmatch_T` (`struct regmatch`) - see
/// `src/nvim/regexp_defs.h` (phase 7). Derives `Default` (a trivial
/// zero-sized value for now) since `cmdmod_T.cmod_filter_regmatch`
/// embeds it by value, same reasoning as `ChangedtickDictItem`/
/// `ScopeDictDictItem` in `eval/typval_defs.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RegmatchT {
    _private: (),
}
/// External regexp captures (`reg_extmatch_T`).
#[derive(Debug, Default)]
pub struct RegExtmatchT {
    pub refcnt: i16,
    pub matches: [Option<Vec<u8>>; crate::regexp_defs::NSUBEXP],
}

/// `AdditionalData`: `nitems`/`nbytes` header followed by a C flexible array
/// member (`char data[]`). Rust has no flexible array members, so the
/// trailing bytes are modeled separately wherever this is actually
/// allocated/used (translated precisely when a consuming file is reached).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdditionalData {
    pub nitems: u32,
    pub nbytes: u32,
}

/// Used by marktree.c `marktree_splice`. Need to keep track of marks which
/// moved in order to repair intersections.
#[derive(Debug, Clone, Copy)]
pub struct MtDamage {
    pub old: *mut crate::marktree_defs::MtNode,
    pub new: *mut crate::marktree_defs::MtNode,
    pub old_i: i32,
    pub new_i: i32,
}

impl Default for MtDamage {
    /// `MTDAMAGE_INIT`
    fn default() -> Self {
        MtDamage {
            old: std::ptr::null_mut(),
            new: std::ptr::null_mut(),
            old_i: 0,
            new_i: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MtDamagePair {
    pub start: MtDamage,
    pub end: MtDamage,
}

/// `StringBuilder`: `kvec_t(char)`, a growable byte buffer.
pub type StringBuilder = Vec<u8>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vimconv_default_matches_mbyte_none_conv() {
        let conversion = VimconvT::default();
        assert_eq!(conversion.vc_type, ConvFlags::None);
        assert_eq!(conversion.vc_factor, 1);
        assert!(conversion.vc_fd.is_null());
        assert!(!conversion.vc_fail);
        assert_eq!(ConvFlags::Iconv as i32, 5);
    }

    #[test]
    fn synstate_default_uses_the_fixed_seven_entry_stack() {
        let state = SynstateT::default();
        assert!(state.sst_next.is_null());
        assert_eq!(state.sst_lnum, 0);
        assert_eq!(state.sst_stacksize, 0);
        assert!(state.sst_next_list.is_null());
        let SynstateStorage::Fixed(stack) = state.sst_storage else {
            panic!("default syntax state must use fixed storage");
        };
        assert_eq!(stack.len(), SST_FIX_STATES);
        assert!(stack.iter().all(|item| item.bs_extmatch.is_null()));
    }

    #[test]
    fn mapblock_default_has_no_links_or_owned_mapping_text() {
        let mapping = MapblockT::default();
        assert!(mapping.m_next.is_null());
        assert!(mapping.m_alt.is_null());
        assert!(mapping.m_keys.is_empty());
        assert!(mapping.m_str.is_none());
        assert!(mapping.m_orig_str.is_none());
        assert!(mapping.m_desc.is_none());
        assert_eq!(mapping.m_luaref, -1);
        assert_eq!(mapping.m_mode, 0);
        assert!(!mapping.m_replace_keycodes);
    }

    #[test]
    fn terminal_cursor_default_matches_zero_initialized_terminal() {
        let term = TerminalT::default();
        assert_eq!(term.cursor, TerminalCursorT::default());
        assert_eq!(term.cursor.row, 0);
        assert_eq!(term.cursor.col, 0);
        assert_eq!(term.cursor.shape, 0);
        assert!(!term.cursor.visible);
        assert!(!term.cursor.blink);
    }

    #[test]
    fn tristate_to_bool_matches_macro() {
        assert!(tristate_to_bool(TriState::True, false));
        assert!(!tristate_to_bool(TriState::False, true));
        assert!(tristate_to_bool(TriState::None, true));
        assert!(!tristate_to_bool(TriState::None, false));
    }

    #[test]
    fn tristate_from_int_matches_macro() {
        assert_eq!(tristate_from_int(0), TriState::False);
        assert_eq!(tristate_from_int(1), TriState::True);
        assert_eq!(tristate_from_int(5), TriState::True);
        assert_eq!(tristate_from_int(-1), TriState::None);
    }

    #[test]
    fn regprog_default_matches_a_zeroed_compiled_program_header() {
        let prog = RegprogT::default();
        assert!(prog.engine.is_null());
        assert_eq!(prog.regflags, 0);
        assert_eq!(prog.re_engine, 0);
        assert_eq!(prog.re_flags, 0);
        assert!(!prog.re_in_use);
    }
}
