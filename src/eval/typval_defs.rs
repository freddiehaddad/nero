//! Translated from `src/nvim/eval/typval_defs.h` (partial: only the two
//! numeric type aliases needed by `undo_defs.h`, phase 3).
//!
//! The bulk of this header (the `typval_T` tagged union representing every
//! Vimscript value type, `list_T`, `dict_T`, `partial_T`, callback types,
//! etc.) is substantial and belongs with the eval engine as a unit
//! (phase 5) - deferred, not started.

pub type VarnumberT = i64;
pub type UvarnumberT = u64;

/// Maximal possible value of a [`VarnumberT`] variable.
pub const VARNUMBER_MAX: VarnumberT = i64::MAX;
/// Minimal possible value of a [`VarnumberT`] variable.
pub const VARNUMBER_MIN: VarnumberT = i64::MIN;
