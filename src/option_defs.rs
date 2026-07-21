//! Translated from `src/nvim/option_defs.h` (partial).
//!
//! Translated: `OptFlags`, `OptValType`+`OptValData` (unified into one
//! safe `OptVal` enum - see its doc comment), `OptScope`(+`OptScopeFlags`),
//! `set_op_T`.
//!
//! Deferred (real forward dependencies): `optset_T`/`opt_did_set_cb_T`
//! (needs `sctx_T`, not yet translated), `optexpand_T`/`opt_expand_cb_T`
//! (needs `regmatch_T` from `regexp_defs.h` and `expand_T` from
//! `cmdexpand_defs.h`, neither translated), `vimoption_T` and the actual
//! option table (needs all of the above plus `OptIndex` and the
//! machine-generated `options_enum.generated.h` list of ~350 options -
//! that generated file is itself produced by `src/gen/gen_options.lua`
//! from `runtime/lua/vim/_meta/options.lua`-adjacent data, a codegen
//! concern to resolve when this crate reaches `option.c` itself).

use crate::api::private::defs::NvimString;
use crate::types_defs::{OptInt, TriState};

/// Option flags (`OptFlags`). Kept as plain `u32` bit-flag constants (some
/// of which are themselves combinations of others, e.g. `REDR_ALL`), not a
/// Rust `enum`, since they combine via bitwise OR - not mutually exclusive
/// variants.
pub mod opt_flags {
    /// Environment expansion. NOTE: can never be used for local or hidden
    /// options.
    pub const EXPAND: u32 = 1 << 0;
    /// Don't expand default value.
    pub const NO_DEF_EXP: u32 = 1 << 1;
    /// Don't set to default value.
    pub const NO_DEFAULT: u32 = 1 << 2;
    /// Option has been set/reset.
    pub const WAS_SET: u32 = 1 << 3;
    /// Don't include in `:mkvimrc` output.
    pub const NO_MKRC: u32 = 1 << 4;
    /// Send option to remote UI.
    pub const UI_OPTION: u32 = 1 << 5;
    /// Redraw tabline.
    pub const REDR_TABL: u32 = 1 << 6;
    /// Redraw status lines.
    pub const REDR_STAT: u32 = 1 << 7;
    /// Redraw current window and recompute text.
    pub const REDR_WIN: u32 = 1 << 8;
    /// Redraw current buffer and recompute text.
    pub const REDR_BUF: u32 = 1 << 9;
    /// Redraw all windows and recompute text.
    pub const REDR_ALL: u32 = REDR_BUF | REDR_WIN;
    /// Clear and redraw all and recompute text.
    pub const REDR_CLEAR: u32 = REDR_ALL | REDR_STAT;
    /// Comma-separated list.
    pub const COMMA: u32 = 1 << 10;
    /// Comma-separated list that cannot have two consecutive commas.
    pub const ONE_COMMA: u32 = (1 << 11) | COMMA;
    /// Don't allow duplicate strings.
    pub const NO_DUP: u32 = 1 << 12;
    /// List of single-char flags.
    pub const FLAG_LIST: u32 = 1 << 13;
    /// Cannot change in modeline or secure mode.
    pub const SECURE: u32 = 1 << 14;
    /// Expand default value with `_()`.
    pub const GETTEXT: u32 = 1 << 15;
    /// Do not use local value for global vimrc.
    pub const NO_GLOB: u32 = 1 << 16;
    /// Only normal file name chars allowed.
    pub const NFNAME: u32 = 1 << 17;
    /// Option was set from a modeline.
    pub const INSECURE: u32 = 1 << 18;
    /// Priority for `:mkvimrc` (setting option has side effects).
    pub const PRI_MKRC: u32 = 1 << 19;
    /// Update curswant required; not needed when there is a redraw flag.
    pub const CURSWANT: u32 = 1 << 20;
    /// Only normal directory name chars allowed.
    pub const NDNAME: u32 = 1 << 21;
    /// Option only changes highlight, not text.
    pub const HL_ONLY: u32 = 1 << 22;
    /// Under control of `'modelineexpr'`.
    pub const MLE: u32 = 1 << 23;
    /// Accept a function reference or a lambda.
    pub const FUNC: u32 = 1 << 24;
    /// Values use colons to create sublists.
    pub const COLON: u32 = 1 << 25;
}

/// Option value type/value (`OptValType`+`OptValData`, unified into one
/// safe Rust enum): the original stores these as two separate fields
/// (`OptVal { OptValType type; OptValData data; }`, the latter a union of
/// `TriState`/`OptInt`/`String`) - but since the tag (`type`) and the data
/// always live right next to each other in the same `OptVal` struct
/// (unlike `DecorInlineData`/`MtKey` elsewhere in this crate, where the tag
/// lives in a *different*, external struct for compact inline storage),
/// there is no memory-layout reason to keep them as an untagged union
/// here - a safe tagged enum is a direct, lossless simplification.
///
/// Boolean options are actually tri-states because they have a third
/// "None" value (kept from the original's comment on `OptValData.boolean`).
#[derive(Debug, Clone, PartialEq)]
pub enum OptVal {
    /// Make sure Nil can't be bitshifted and used as an option type flag
    /// (kept from the original's comment on `kOptValTypeNil = -1`; not
    /// meaningful as a bit position in this translation, but the ordering/
    /// semantics are preserved).
    Nil,
    Boolean(TriState),
    Number(OptInt),
    String(NvimString),
}

/// Scopes that an option can support (`OptScope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OptScope {
    /// Request global option value.
    Global = 0,
    /// Request window-local option value.
    Win,
    /// Request buffer-local option value.
    Buf,
    /// Request tabpage-local option value.
    Tab,
}

/// Always update this whenever a new option scope is added (`kOptScopeSize`).
pub const OPT_SCOPE_SIZE: usize = OptScope::Tab as usize + 1;

pub type OptScopeFlags = u8;

/// `:set` operator types (`set_op_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOpT {
    None = 0,
    /// `"opt+=arg"`
    Adding,
    /// `"opt^=arg"`
    Prepending,
    /// `"opt-=arg"`
    Removing,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_flags_combinations_match_c_macros() {
        assert_eq!(opt_flags::REDR_ALL, opt_flags::REDR_BUF | opt_flags::REDR_WIN);
        assert_eq!(opt_flags::REDR_CLEAR, opt_flags::REDR_ALL | opt_flags::REDR_STAT);
        assert_eq!(opt_flags::ONE_COMMA, (1 << 11) | opt_flags::COMMA);
    }

    #[test]
    fn opt_scope_size_matches_c_macro() {
        assert_eq!(OPT_SCOPE_SIZE, 4); // Global, Win, Buf, Tab
    }

    #[test]
    fn opt_val_variants_are_distinguishable() {
        assert_ne!(OptVal::Nil, OptVal::Number(0));
        assert_eq!(OptVal::Number(5), OptVal::Number(5));
        assert_ne!(OptVal::Boolean(TriState::True), OptVal::Boolean(TriState::False));
    }
}
