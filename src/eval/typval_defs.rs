//! Translated from `src/nvim/eval/typval_defs.h` (partial: the numeric
//! type aliases needed by `undo_defs.h`, plus `scid_T`/`sctx_T` needed by
//! `buffer_defs.h`'s `winopt_T`, and `Callback`/`CallbackType` needed by
//! `buffer_defs.h`'s buffer-local-options block, phase 3; plus, added
//! later, the remaining fully self-contained enums/constants with no
//! pointer fields and no cross-struct dependency on `list_T`/`dict_T`/
//! `ufunc_T` themselves - `VarType`/`VAR_TYPE_*`, `VarLockStatus`,
//! `ScopeType`, `BoolVarValue`, `SpecialVarValue`, `DictItemFlags`,
//! `ListLenSpecials`, `DO_NOT_FREE_CNT`, `MAX_FUNC_ARGS`/
//! `VAR_SHORT_LEN`/`FIXVAR_CNT`).
//!
//! The bulk of this header (the `typval_T` tagged union representing every
//! Vimscript value type, `list_T`, `dict_T`, `partial_T`'s real fields,
//! etc.) is substantial and belongs with the eval engine as a unit
//! (phase 5) - deferred, not started. `dict_T`/`partial_T` themselves are
//! forward-declared here as opaque placeholders (same convention as
//! `types_defs.rs`'s cross-cutting placeholder list) purely so that
//! `Callback`/`ScopeDictDictItem`/`ChangedtickDictItem` - real types other
//! not-yet-translated files reference by pointer/value - can exist now
//! without faking their eventual real contents.

use crate::pos_defs::LinenrT;
use crate::types_defs::LuaRef;

/// Placeholder for `dict_T` (`struct dictvar_S`) - the Vimscript
/// Dictionary type. Needs `typval_T`/`dictitem_T`, deferred to the eval
/// engine as a unit (phase 5).
pub struct DictT {
    _private: (),
}

/// Placeholder for `list_T` (`struct listvar_S`) - the Vimscript List
/// type. Needs `typval_T`/`listitem_T`, deferred to the eval engine as a
/// unit (phase 5).
pub struct ListT {
    _private: (),
}

/// Placeholder for `partial_T` (`struct partial_S`) - a Vimscript partial
/// (a function reference bound to some arguments/a dict `self`). Needs
/// `typval_T`, deferred to the eval engine as a unit (phase 5).
pub struct PartialT {
    _private: (),
}

/// Type used for the `changedtick_di` member in `buf_T`
/// (`ChangedtickDictItem`, a `TV_DICTITEM_STRUCT(sizeof("changedtick"))`
/// instance). Exists upstream primarily so that literals of the relevant
/// type can be made; every `TV_DICTITEM_STRUCT` instantiation embeds a
/// `typval_T di_tv` field directly (by value, not by pointer), so this
/// can't be modeled even partially until `typval_T` itself exists -
/// deferred to the eval engine as a unit (phase 5). Derives `Default` (a
/// trivial zero-sized value for now) so it stays embeddable by value in
/// `buf_T` (-> `FileBuffer`) exactly like the original, rather than
/// forcing a pointer-based workaround purely to dodge that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChangedtickDictItem {
    _private: (),
}

/// Structure to hold a scope dictionary (e.g. `b:`/`w:`/`t:`), pretending
/// to `find_var_in_ht()` (not yet translated) to be a `dictitem_T`
/// (`ScopeDictDictItem`, a `TV_DICTITEM_STRUCT(1)` instance). Same
/// `typval_T`-embeds-by-value blocker as [`ChangedtickDictItem`] - deferred
/// to the eval engine as a unit (phase 5); same trivial `Default` for the
/// same by-value-embedding reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScopeDictDictItem {
    _private: (),
}

/// Discriminant for which kind of callback a [`Callback`] holds
/// (`CallbackType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CallbackType {
    #[default]
    None,
    Funcref,
    Partial,
    Lua,
}

/// A callback into Vimscript or Lua: a plain function name (`funcref`), a
/// partial (function + bound arguments/dict), or a Lua function reference
/// (`Callback`).
///
/// The original is an untagged C union (`data`) plus a separate
/// `CallbackType type` tag; translated as a proper safe Rust enum (each
/// variant directly carries its own payload) rather than replicating the
/// union + separate tag split, matching this crate's established
/// convention for small tagged unions where there's no hot-path memory
/// layout reason not to (e.g. `UhLink` in `undo_defs.rs`). The `Partial`
/// variant stays a raw pointer to the not-yet-translated [`PartialT`],
/// same as e.g. `SynblockT.b_syn_linecont_prog: *mut RegprogT` elsewhere.
#[derive(Debug, Clone)]
pub enum Callback {
    /// `kCallbackNone` / `CALLBACK_NONE`.
    None,
    /// `kCallbackFuncref`: plain function name (`char *funcref`).
    Funcref(Vec<u8>),
    /// `kCallbackPartial` (`partial_T *partial`).
    Partial(*mut PartialT),
    /// `kCallbackLua` (`LuaRef luaref`).
    Lua(LuaRef),
}

impl Default for Callback {
    /// `CALLBACK_INIT`/`CALLBACK_NONE`.
    fn default() -> Self {
        Callback::None
    }
}

impl Callback {
    /// `CallbackType` of this callback, mirroring the original's separate
    /// `.type` tag (`callback_is_none()`-style checks elsewhere use this).
    #[must_use]
    pub fn kind(&self) -> CallbackType {
        match self {
            Callback::None => CallbackType::None,
            Callback::Funcref(_) => CallbackType::Funcref,
            Callback::Partial(_) => CallbackType::Partial,
            Callback::Lua(_) => CallbackType::Lua,
        }
    }
}

pub type VarnumberT = i64;
pub type UvarnumberT = u64;

/// Maximal possible value of a [`VarnumberT`] variable.
pub const VARNUMBER_MAX: VarnumberT = i64::MAX;
/// Minimal possible value of a [`VarnumberT`] variable.
pub const VARNUMBER_MIN: VarnumberT = i64::MIN;
/// Maximal possible value of a [`UvarnumberT`] variable
/// (`UVARNUMBER_MAX`).
pub const UVARNUMBER_MAX: UvarnumberT = u64::MAX;

/// Refcount for a dict or list that should never be freed
/// (`DO_NOT_FREE_CNT`).
pub const DO_NOT_FREE_CNT: i32 = i32::MAX / 2;

/// Additional values for `tv_list_alloc()`'s `len` argument
/// (`ListLenSpecials`; `tv_list_alloc` itself is not yet translated -
/// needs `list_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListLenSpecials {
    /// List length is not known in advance - there's neither a way to
    /// know how many elements will be needed nor any educated guess
    /// (`kListLenUnknown`).
    Unknown = -1,
    /// List length *should* be known, but is actually not - all
    /// occurrences of this should eventually be removed; it's only
    /// used where determining the length would need a refactor
    /// (`kListLenShouldKnow`).
    ShouldKnow = -2,
    /// List length may be known in advance, but determining it looks
    /// impractical (`kListLenMayKnow`).
    MayKnow = -3,
}

/// Bool variable values (`BoolVarValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolVarValue {
    /// `v:false` (`kBoolVarFalse`).
    False,
    /// `v:true` (`kBoolVarTrue`).
    True,
}

/// Special variable values (`SpecialVarValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialVarValue {
    /// `v:null` (`kSpecialVarNull`).
    Null,
}

/// Variable lock status for `typval_T.v_lock` (`VarLockStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VarLockStatus {
    /// Not locked (`VAR_UNLOCKED`).
    #[default]
    Unlocked = 0,
    /// User lock, can be unlocked (`VAR_LOCKED`).
    Locked = 1,
    /// Locked forever (`VAR_FIXED`).
    Fixed = 2,
}

/// Vimscript variable types, for use in `typval_T.v_type` (`VarType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VarType {
    /// Unknown (unspecified) value (`VAR_UNKNOWN`).
    #[default]
    Unknown = 0,
    /// Number, `.v_number` is used (`VAR_NUMBER`).
    Number = 1,
    /// String, `.v_string` is used (`VAR_STRING`).
    String = 2,
    /// Function reference, `.v_string` is used as the function name
    /// (`VAR_FUNC`).
    Func = 3,
    /// List, `.v_list` is used (`VAR_LIST`).
    List = 4,
    /// Dict, `.v_dict` is used (`VAR_DICT`).
    Dict = 5,
    /// Floating-point value, `.v_float` is used (`VAR_FLOAT`).
    Float = 6,
    /// `true`/`false`, `.v_bool` is used (`VAR_BOOL`).
    Bool = 7,
    /// Special value (null), `.v_special` is used (`VAR_SPECIAL`).
    Special = 8,
    /// Partial, `.v_partial` is used (`VAR_PARTIAL`).
    Partial = 9,
    /// Blob, `.v_blob` is used (`VAR_BLOB`).
    Blob = 10,
}

/// Type values returned by Vimscript's `type()` built-in (`VAR_TYPE_*`,
/// an anonymous `enum` of plain integer constants in the original, kept
/// that way here too rather than as a Rust `enum` type - matching this
/// crate's existing `opt_dy_flag`-style precedent for anonymous C
/// integer-constant enums). Distinct from [`VarType`]'s own
/// discriminants (note the non-contiguous `10` for `BLOB`, and that
/// `NUMBER`/`STRING`/etc. don't line up 1:1 with `VarType`'s own values
/// either - these are a completely independent numbering the original
/// itself defines separately).
pub mod var_type_result {
    pub const NUMBER: i32 = 0;
    pub const STRING: i32 = 1;
    pub const FUNC: i32 = 2;
    pub const LIST: i32 = 3;
    pub const DICT: i32 = 4;
    pub const FLOAT: i32 = 5;
    pub const BOOL: i32 = 6;
    pub const SPECIAL: i32 = 7;
    pub const BLOB: i32 = 10;
}

/// Values for `(struct dictvar_S).dv_scope` (`ScopeType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScopeType {
    /// Not a scope dictionary (`VAR_NO_SCOPE`).
    #[default]
    NoScope = 0,
    /// Scope dictionary which requires a prefix (`a:`, `v:`, ...)
    /// (`VAR_SCOPE`).
    Scope = 1,
    /// Scope dictionary which may be accessed without a prefix (`l:`,
    /// `g:`) (`VAR_DEF_SCOPE`).
    DefScope = 2,
}

/// Flags for `dictitem_T.di_flags` (`DictItemFlags`) - bit flags meant
/// to be OR'd together (e.g. read-only *and* fixed), so kept as plain
/// integer constants rather than a Rust `enum` (which would wrongly
/// imply mutually-exclusive variants) - matching this crate's
/// `MT_FLAG_*`/`opt_dy_flag` bit-flag precedent. `dictitem_T` itself
/// (which would consume these) is not yet translated.
pub mod dict_item_flags {
    /// Read-only value (`DI_FLAGS_RO`).
    pub const RO: u8 = 1;
    /// Value, read-only in the sandbox (`DI_FLAGS_RO_SBX`).
    pub const RO_SBX: u8 = 2;
    /// Fixed value: cannot be `:unlet` or `remove()`d (`DI_FLAGS_FIX`).
    pub const FIX: u8 = 4;
    /// Locked value (`DI_FLAGS_LOCK`).
    pub const LOCK: u8 = 8;
    /// Separately allocated (`DI_FLAGS_ALLOC`).
    pub const ALLOC: u8 = 16;
}

/// Maximum number of function arguments (`MAX_FUNC_ARGS`).
pub const MAX_FUNC_ARGS: usize = 20;
/// Short variable name length (`VAR_SHORT_LEN`).
pub const VAR_SHORT_LEN: usize = 20;
/// Number of fixed variables used for arguments (`FIXVAR_CNT`).
pub const FIXVAR_CNT: usize = 12;

/// Type used for script ID (`scid_T`).
pub type ScidT = i32;

/// SCript ConteXt (SCTX): identifies a script line (`sctx_T`).
///
/// When sourcing a script `sc_lnum` is zero, `sourcing_lnum` is the current
/// line number. When executing a user function `sc_lnum` is the line where
/// the function was defined, `sourcing_lnum` is the line number inside the
/// function. When stored with a function, mapping, option, etc. `sc_lnum`
/// is the line number in the script `sc_sid`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SctxT {
    /// script ID
    pub sc_sid: ScidT,
    /// sourcing sequence number
    pub sc_seq: i32,
    /// line number
    pub sc_lnum: LinenrT,
    /// only used when `sc_sid` is `SID_API_CLIENT`
    pub sc_chan: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sctx_default_is_zeroed() {
        let sctx = SctxT::default();
        assert_eq!(sctx.sc_sid, 0);
        assert_eq!(sctx.sc_seq, 0);
        assert_eq!(sctx.sc_lnum, 0);
        assert_eq!(sctx.sc_chan, 0);
    }

    #[test]
    fn callback_default_is_none_variant() {
        let cb = Callback::default();
        assert_eq!(cb.kind(), CallbackType::None);
    }

    #[test]
    fn callback_kind_matches_variant() {
        assert_eq!(Callback::Funcref(b"MyFunc".to_vec()).kind(), CallbackType::Funcref);
        assert_eq!(Callback::Lua(0).kind(), CallbackType::Lua);
        assert_eq!(Callback::Partial(std::ptr::null_mut()).kind(), CallbackType::Partial);
    }

    #[test]
    fn var_type_discriminants_match_the_original_c_enum_order() {
        assert_eq!(VarType::Unknown as i32, 0);
        assert_eq!(VarType::Number as i32, 1);
        assert_eq!(VarType::String as i32, 2);
        assert_eq!(VarType::Func as i32, 3);
        assert_eq!(VarType::List as i32, 4);
        assert_eq!(VarType::Dict as i32, 5);
        assert_eq!(VarType::Float as i32, 6);
        assert_eq!(VarType::Bool as i32, 7);
        assert_eq!(VarType::Special as i32, 8);
        assert_eq!(VarType::Partial as i32, 9);
        assert_eq!(VarType::Blob as i32, 10);
    }

    #[test]
    fn var_type_result_constants_match_the_original_including_the_blob_gap() {
        assert_eq!(var_type_result::NUMBER, 0);
        assert_eq!(var_type_result::STRING, 1);
        assert_eq!(var_type_result::FUNC, 2);
        assert_eq!(var_type_result::LIST, 3);
        assert_eq!(var_type_result::DICT, 4);
        assert_eq!(var_type_result::FLOAT, 5);
        assert_eq!(var_type_result::BOOL, 6);
        assert_eq!(var_type_result::SPECIAL, 7);
        // Note the gap: BLOB is 10, not 8 - matches the original's own
        // non-contiguous numbering exactly (verified against
        // typval_defs.h directly, not assumed).
        assert_eq!(var_type_result::BLOB, 10);
    }

    #[test]
    fn scope_type_discriminants_match_the_original() {
        assert_eq!(ScopeType::NoScope as i32, 0);
        assert_eq!(ScopeType::Scope as i32, 1);
        assert_eq!(ScopeType::DefScope as i32, 2);
    }

    #[test]
    fn var_lock_status_discriminants_match_the_original() {
        assert_eq!(VarLockStatus::Unlocked as i32, 0);
        assert_eq!(VarLockStatus::Locked as i32, 1);
        assert_eq!(VarLockStatus::Fixed as i32, 2);
    }

    #[test]
    fn list_len_specials_discriminants_match_the_original() {
        assert_eq!(ListLenSpecials::Unknown as i32, -1);
        assert_eq!(ListLenSpecials::ShouldKnow as i32, -2);
        assert_eq!(ListLenSpecials::MayKnow as i32, -3);
    }

    #[test]
    fn dict_item_flags_are_distinct_bits_that_can_be_combined() {
        use dict_item_flags::{ALLOC, FIX, LOCK, RO, RO_SBX};
        let all = [RO, RO_SBX, FIX, LOCK, ALLOC];
        for (i, &a) in all.iter().enumerate() {
            for (j, &b) in all.iter().enumerate() {
                if i != j {
                    assert_eq!(a & b, 0, "flags {a} and {b} overlap");
                }
            }
        }
        // Combining read-only + fixed is a valid, expected OR-combination.
        assert_eq!(RO | FIX, 5);
    }

    #[test]
    fn do_not_free_cnt_is_int_max_over_two() {
        assert_eq!(DO_NOT_FREE_CNT, i32::MAX / 2);
    }
}
