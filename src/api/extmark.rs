//! Translated from `src/nvim/api/extmark.h` (the header-declared
//! globals and `ns_in_win` only - the real `api/extmark.c` file, the
//! full `nvim_buf_set_extmark`/etc. API implementation, is a separate,
//! substantial phase-12 API-layer undertaking not attempted here).
//!
//! Translated: `namespace_localscope` (tracks which namespace ids are
//! window-scoped, as opposed to global - a locally-scoped namespace
//! may be "orphaned" if all window(s) it was scoped to are destroyed,
//! but stays tracked here so it's never mistaken for global scope) and
//! `ns_in_win` (checks whether a namespace is visible in a given
//! window - needed by `plines.c`'s `charsize_regular`/`decoration.c`'s
//! `decor_conceal_line`/`decor_virt_lines`, all since translated - this
//! was a small, genuinely self-contained piece worth harvesting ahead
//! of its callers at the time).
//!
//! Deferred: `namespace_ids` (the name -> id `Map<String, int>`) and
//! `next_namespace_id` - both only needed by `nvim_create_namespace`
//! (the API entry point that actually allocates/names a namespace),
//! not yet translated and not needed by `ns_in_win` itself.

use crate::buffer_defs::WinT;
use crate::globals::GlobalCell;
use crate::map::Set;

/// Non-global namespaces. A locally-scoped namespace may be "orphaned"
/// if all window(s) it was scoped to are destroyed. Such orphans are
/// tracked here to avoid being mistaken as "global scope"
/// (`namespace_localscope`).
pub static NAMESPACE_LOCALSCOPE: std::sync::LazyLock<GlobalCell<Set<u32>>> =
    std::sync::LazyLock::new(|| GlobalCell::new(Set::default()));

/// Returns true if the namespace is global or scoped in the given
/// window (`ns_in_win`).
///
/// # Safety
/// Touches the shared [`NAMESPACE_LOCALSCOPE`] global (see its own
/// doc comment / [`crate::globals::GlobalCell::get_mut`]'s safety
/// requirements).
#[must_use]
pub unsafe fn ns_in_win(ns_id: u32, wp: &WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { NAMESPACE_LOCALSCOPE.get_mut() }.contains(&ns_id) {
        return true;
    }

    wp.w_ns_set.contains(&ns_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    #[test]
    fn ns_in_win_global_namespace_is_always_visible() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };

        // ns_id 42 was never added to NAMESPACE_LOCALSCOPE, so it's
        // treated as global - visible everywhere regardless of
        // w_ns_set's own contents.
        assert!(unsafe { ns_in_win(42, &win) });
    }

    #[test]
    fn ns_in_win_local_namespace_requires_window_membership() {
        let _lock = crate::globals::global_state_test_lock();
        // SAFETY: holding global_state_test_lock() for this test's
        // whole body.
        unsafe { NAMESPACE_LOCALSCOPE.get_mut() }.put(7);

        let mut buf = BufT::default();
        let mut win_member = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win_member.w_ns_set.put(7);
        assert!(unsafe { ns_in_win(7, &win_member) });

        let win_non_member = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        assert!(!unsafe { ns_in_win(7, &win_non_member) });

        // SAFETY: same lock held for the whole test body.
        unsafe { NAMESPACE_LOCALSCOPE.get_mut() }.delete(&7);
    }
}
