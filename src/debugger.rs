//! Translated from `src/nvim/debugger.c` (tractable core only).
//!
//! `debugger.c` implements the `:debug`/breakpoint (`:breakadd`/
//! `:breakdel`/`:breaklist`) command-line debugger. Almost every
//! function needs `do_debug` (the real interactive debug-mode REPL,
//! reading commands via the not-yet-translated command-line input
//! subsystem) and/or `dbg_breakp` (the breakpoint list, a `garray_T`
//! of `struct debuggy` entries not yet translated), plus real message
//! display.
//!
//! Translated: `ex_debuggreedy` (the `:debuggreedy` Ex-command
//! handler - just flips the `debug_greedy` file-static based on
//! whether an address was given). No real caller yet (`do_debug`,
//! its only reader, isn't translated) - translated ahead of it
//! anyway, matching this crate's established "translate a small,
//! simple, mechanically-correct piece ahead of the surrounding
//! engine" precedent.
//!
//! Deferred: everything else in the file.

use crate::ex_cmds_defs::ExargT;
use crate::globals::GlobalCell;

/// Whether `:debug` mode reads input directly rather than via
/// `typeahead`, set by `:debuggreedy`/`:0debuggreedy`
/// (`debug_greedy`).
static DEBUG_GREEDY: GlobalCell<bool> = GlobalCell::new(false);

/// `":debuggreedy"` (`ex_debuggreedy`). With no address given, or an
/// explicit NONZERO address (e.g. `:5debuggreedy`), enables greedy
/// mode; an explicit address of exactly `0` (`:0debuggreedy`) is the
/// one combination that disables it - matching the original's own
/// real `eap->addr_count == 0 || eap->line2 != 0` condition exactly
/// (a real, deliberate upstream quirk: `:0debuggreedy` is NOT treated
/// the same as no address at all).
///
/// # Safety
/// Single-threaded test/editor state, matching every other
/// `GlobalCell`-backed file-static in this crate.
pub unsafe fn ex_debuggreedy(eap: &ExargT) {
    let greedy = eap.addr_count == 0 || eap.line2 != 0;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *DEBUG_GREEDY.get_mut() = greedy };
}

/// # Safety
/// Same as [`ex_debuggreedy`].
#[must_use]
pub unsafe fn debug_greedy() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *DEBUG_GREEDY.get_mut() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    #[test]
    fn no_address_enables_greedy_mode() {
        let _lock = global_state_test_lock();
        let eap = ExargT { addr_count: 0, line2: 0, ..ExargT::default() };
        unsafe { ex_debuggreedy(&eap) };
        assert!(unsafe { debug_greedy() });
    }

    #[test]
    fn explicit_zero_address_disables_greedy_mode() {
        let _lock = global_state_test_lock();
        // `:0debuggreedy` - a real address was given (addr_count != 0)
        // AND it's exactly 0 (line2 == 0) - the one combination where
        // BOTH of ex_debuggreedy's own disjuncts are false, so this
        // explicitly turns greedy mode OFF (a real, deliberate
        // upstream quirk: an explicit ":0" is not the same as "no
        // address at all", verified by tracing the exact boolean
        // condition rather than assuming either "some address" or
        // "line2 == 0" alone determines the outcome).
        unsafe { *DEBUG_GREEDY.get_mut() = true };
        let eap = ExargT { addr_count: 1, line2: 0, ..ExargT::default() };
        unsafe { ex_debuggreedy(&eap) };
        assert!(!unsafe { debug_greedy() });
    }

    #[test]
    fn a_nonzero_address_enables_greedy_mode() {
        let _lock = global_state_test_lock();
        // `:5debuggreedy` - an address WAS given, but it's nonzero, so
        // the `line2 != 0` disjunct is true - greedy mode turns ON.
        let eap = ExargT { addr_count: 1, line2: 5, ..ExargT::default() };
        unsafe { ex_debuggreedy(&eap) };
        assert!(unsafe { debug_greedy() });
    }

    #[test]
    fn toggling_back_and_forth() {
        let _lock = global_state_test_lock();
        let on = ExargT { addr_count: 0, line2: 0, ..ExargT::default() };
        let off = ExargT { addr_count: 1, line2: 0, ..ExargT::default() };
        unsafe { ex_debuggreedy(&on) };
        assert!(unsafe { debug_greedy() });
        unsafe { ex_debuggreedy(&off) };
        assert!(!unsafe { debug_greedy() });
        unsafe { ex_debuggreedy(&on) };
        assert!(unsafe { debug_greedy() });
    }
}
