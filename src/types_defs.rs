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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum TriState {
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
//   Loop       -> struct loop,        src/nvim/event/loop.h    (phase 11)
//   regprog_T  -> struct regprog,     src/nvim/regexp_defs.h   (phase 7)
//   synstate_T -> struct syn_state,   src/nvim/syntax_defs.h   (phase 8)
//   Terminal   -> struct terminal,    src/nvim/terminal.h      (phase 14)
//   win_T      -> struct window_S,    src/nvim/buffer_defs.h   (phase 3/8)
//   qf_info_T  -> struct qf_info_S,   src/nvim/quickfix.c      (phase 8)
//   mapblock_T -> struct mapblock,    src/nvim/mapping_defs.h  (phase 7)
// (mapblock_T/qf_info_T are actually forward-declared in their own headers,
// not types_defs.h itself, unlike the others above - but this crate keeps
// all such opaque cross-cutting placeholders here regardless of exactly
// which original header contains the forward declaration, since Rust has
// no forward-declaration mechanism of its own to mirror precisely.)
// MTNode (struct mtnode_s) is no longer a placeholder here: it is now
// translated for real in `src/nvim/marktree_defs.h` -> `crate::marktree_defs::MtNode`.
// buf_T (struct file_buffer) is likewise no longer a placeholder: it is
// now translated for real as `crate::buffer_defs::BufT` (kept under the
// same name, since `buf_T` - not `FileBuffer` - is the name actually used
// throughout the rest of the original codebase; matches this crate's
// "prefer the real typedef name" convention, e.g. `wininfo_S` -> `WinInfo`).

/// Placeholder for `Loop` (`struct loop`) - see `src/nvim/event/loop.h` (phase 11).
pub struct LoopT {
    _private: (),
}
/// Placeholder for `regprog_T` (`struct regprog`) - see `src/nvim/regexp_defs.h` (phase 7).
pub struct RegprogT {
    _private: (),
}
/// Placeholder for `synstate_T` (`struct syn_state`) - see `src/nvim/syntax_defs.h` (phase 8).
pub struct SynstateT {
    _private: (),
}
/// Placeholder for `Terminal` (`struct terminal`) - see `src/nvim/terminal.h` (phase 14).
pub struct TerminalT {
    _private: (),
}
/// Placeholder for `win_T` (`struct window_S`) - see `src/nvim/buffer_defs.h` (phase 3/8).
pub struct WinT {
    _private: (),
}
/// Placeholder for `qf_info_T` (`struct qf_info_S`) - see `src/nvim/quickfix.c` (phase 8).
pub struct QfInfoT {
    _private: (),
}
/// Placeholder for `mapblock_T` (`struct mapblock`) - see `src/nvim/mapping_defs.h` (phase 7).
pub struct MapblockT {
    _private: (),
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
}
