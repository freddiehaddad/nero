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
//!
//! `ctx_free` (frees a `Context`'s own `regs`/`jumps`/`bufs`/`gvars`/
//! `funcs` fields) needs NO Rust equivalent at all: `context_defs.rs`'s
//! `Context` already models every one of those fields as an owned
//! `Option<Vec<u8>>`/`Vec<Object>`, so Rust's own `Drop` impl already
//! performs the exact same cleanup automatically - the same reasoning
//! already established for `optval_free`/`ga_clear_strings` elsewhere
//! in this crate.

use crate::context_defs::{CtxSwitch, CtxSwitchMode, CtxWin};

/// The `ctx_win[]` pool of temporary "autocmd window" scratch windows
/// (`ctx_win_vec`, `context.h`'s `kvec_t(CtxWin)` - modeled as a plain
/// growable `Vec`, matching this crate's own established idiom for a
/// C `kvec_t`). Always empty today - nothing in this crate can
/// currently allocate a real autocmd window (`win_alloc`/the
/// window-splitting machinery needed by the not-yet-translated
/// `ctx_win_get`/`ctx_win_release` pair, `context.c`'s own
/// producer/consumer of this pool, neither translated).
pub(crate) static CTX_WIN_VEC: std::sync::LazyLock<crate::globals::GlobalCell<Vec<CtxWin>>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(Vec::new()));

/// Whether `win` is an active entry in `CTX_WIN_VEC` (the pool of
/// temporary scratch windows) (`is_ctx_win`).
///
/// # Safety
/// `win` need not be dereferenced (only ever compared by pointer
/// value against each pool entry's own `cw_win`) - safe to call with
/// any pointer, including a dangling or null one.
#[must_use]
pub fn is_ctx_win(win: *mut crate::buffer_defs::WinT) -> bool {
    // SAFETY: no overlapping live access - see this crate's
    // established GlobalCell::get_mut convention.
    unsafe { CTX_WIN_VEC.get_mut() }.iter().any(|cw| cw.cw_used && std::ptr::eq(cw.cw_win, win))
}

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

    #[test]
    fn is_ctx_win_false_when_pool_is_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        assert!(unsafe { CTX_WIN_VEC.get_mut() }.is_empty());
        assert!(!is_ctx_win(&mut win as *mut crate::buffer_defs::WinT));
    }

    #[test]
    fn is_ctx_win_true_for_a_used_entry_matching_the_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        unsafe { CTX_WIN_VEC.get_mut() }.push(CtxWin { cw_win: win_ptr, cw_used: true });

        assert!(is_ctx_win(win_ptr));

        unsafe { CTX_WIN_VEC.get_mut() }.clear();
    }

    #[test]
    fn is_ctx_win_false_for_an_unused_entry() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        unsafe { CTX_WIN_VEC.get_mut() }.push(CtxWin { cw_win: win_ptr, cw_used: false });

        assert!(!is_ctx_win(win_ptr));

        unsafe { CTX_WIN_VEC.get_mut() }.clear();
    }

    #[test]
    fn is_ctx_win_false_for_a_different_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win_a = crate::buffer_defs::WinT::default();
        let mut win_b = crate::buffer_defs::WinT::default();
        let ptr_a = &mut win_a as *mut crate::buffer_defs::WinT;
        let ptr_b = &mut win_b as *mut crate::buffer_defs::WinT;
        unsafe { CTX_WIN_VEC.get_mut() }.push(CtxWin { cw_win: ptr_a, cw_used: true });

        assert!(!is_ctx_win(ptr_b));

        unsafe { CTX_WIN_VEC.get_mut() }.clear();
    }
}
