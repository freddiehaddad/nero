//! Translated from `src/nvim/context.c` (tractable core only).
//!
//! `context.c` (~950 lines) implements the `:mkview`/context-API
//! snapshot machinery AND the temporary window/buffer-switch
//! machinery used by autocommand execution (`ctx_switch`/
//! `ctx_restore`, e.g. to run autocmds "as if" a different
//! window/buffer were current, then switch back). Only
//! [`ctx_restore`]'s own "was `ctx_switch()` ever actually called on
//! this `CtxSwitch`?" early-return check is translated here - the
//! rest of `ctx_restore` (actually undoing a real switch) and all of
//! `ctx_switch` itself (performing one) remain `unimplemented!()`,
//! needing window/tabpage-switching machinery (`goto_tabpage_tp`,
//! `win_find_by_handle`, etc.) not yet translated.
//!
//! This one check is enough to make `src/autocmd.rs`'s
//! `apply_autocmds` family real: every call site there constructs its
//! own `CtxSwitch::default()` (`cs_mode` defaults to
//! `CtxSwitchMode::None`, matching the original's `CtxSwitch aco =
//! { 0 }`) and NEVER calls a (not-yet-translated) `ctx_switch` on it
//! before calling [`ctx_restore`] - so the early-return branch is not
//! just reachable, it is the ONLY branch ever exercised anywhere in
//! this crate today, matching [`ctx_restore`]'s own doc comment
//! (translated near-verbatim below) which explicitly names exactly
//! this "skipped `ctx_switch()`" usage pattern as a first-class,
//! intentional no-op case - not an edge case being special-cased away.

use crate::context_defs::{CtxSwitch, CtxSwitchMode};

/// Undoes `ctx_switch()`: restores the previous location (if
/// possible) and the kept state.
///
/// No-op if `cs` was zero-initialized (`cs.cs_mode ==
/// `CtxSwitchMode::None`), even if `ctx_switch()` was not called on
/// it - see this module's own doc comment for why this is the only
/// branch translated so far.
///
/// # Panics
/// Panics if `cs.cs_mode != CtxSwitchMode::None` - undoing a REAL
/// switch needs window/tabpage-switching machinery not yet
/// translated. Unreachable in practice today: nothing in this crate
/// can currently call the not-yet-translated `ctx_switch` to produce
/// a non-`None` `cs_mode` in the first place.
pub fn ctx_restore(cs: &CtxSwitch) {
    if cs.cs_mode == CtxSwitchMode::None {
        return; // zero-initialized: ctx_switch() was never called on `cs`.
    }
    unimplemented!(
        "ctx_restore: undoing a real ctx_switch() needs window/tabpage-switching machinery, \
         not yet translated - see this module's own doc comment"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctx_restore_is_a_noop_for_a_default_zeroed_ctx_switch() {
        let cs = CtxSwitch::default();
        ctx_restore(&cs); // must not panic
    }

    #[test]
    #[should_panic(expected = "undoing a real ctx_switch()")]
    fn ctx_restore_panics_for_a_non_none_mode() {
        let cs = CtxSwitch { cs_mode: CtxSwitchMode::Win, ..Default::default() };
        ctx_restore(&cs);
    }
}
