//! Translated from `src/nvim/eval/typval_defs.h` (partial: the numeric
//! type aliases needed by `undo_defs.h`, plus `scid_T`/`sctx_T` needed by
//! `buffer_defs.h`'s `winopt_T`, and `Callback`/`CallbackType` needed by
//! `buffer_defs.h`'s buffer-local-options block, phase 3).
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
}
