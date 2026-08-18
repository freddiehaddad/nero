//! Translated from `src/nvim/api/extmark.h` (the header-declared
//! globals and `ns_in_win`) plus `api/extmark.c`'s own
//! `nvim_create_namespace` (the full `nvim_buf_set_extmark`/etc. API
//! implementation remains a separate, substantial phase-12 API-layer
//! undertaking not attempted here).
//!
//! Translated: `namespace_localscope` (tracks which namespace ids are
//! window-scoped, as opposed to global - a locally-scoped namespace
//! may be "orphaned" if all window(s) it was scoped to are destroyed,
//! but stays tracked here so it's never mistaken for global scope),
//! `ns_in_win` (checks whether a namespace is visible in a given
//! window - needed by `plines.c`'s `charsize_regular`/`decoration.c`'s
//! `decor_conceal_line`/`decor_virt_lines`, all since translated - this
//! was a small, genuinely self-contained piece worth harvesting ahead
//! of its callers at the time), and [`nvim_create_namespace`] (via the
//! now-real `NAMESPACE_IDS`/`NEXT_NAMESPACE_ID` file-statics), plus
//! [`nvim_get_namespaces`].

use crate::api::private::defs::{Dict, Integer, KeyValuePair, NvimString, Object};
use crate::buffer_defs::WinT;
use crate::globals::GlobalCell;
use crate::map::{Map, Set};

/// Non-global namespaces. A locally-scoped namespace may be "orphaned"
/// if all window(s) it was scoped to are destroyed. Such orphans are
/// tracked here to avoid being mistaken as "global scope"
/// (`namespace_localscope`).
pub static NAMESPACE_LOCALSCOPE: std::sync::LazyLock<GlobalCell<Set<u32>>> =
    std::sync::LazyLock::new(|| GlobalCell::new(Set::default()));

/// Name -> id mapping for non-anonymous namespaces (`namespace_ids`).
pub static NAMESPACE_IDS: std::sync::LazyLock<GlobalCell<Map<Vec<u8>, i32>>> =
    std::sync::LazyLock::new(|| GlobalCell::new(Map::default()));

/// The next namespace id to allocate; matches the original's own
/// `next_namespace_id` (which starts at `1`, since `handle_T` id `0`
/// is never handed out - `nvim_create_namespace`'s own `id > 0` check
/// relies on this).
pub static NEXT_NAMESPACE_ID: std::sync::LazyLock<GlobalCell<i32>> =
    std::sync::LazyLock::new(|| GlobalCell::new(1));

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

/// Create a new namespace, or get an existing one by `name`
/// (`nvim_create_namespace`). An empty `name` always creates a new,
/// anonymous namespace, never registered in [`NAMESPACE_IDS`] (so a
/// later call with an empty name can never "find" it again).
///
/// # Safety
/// Touches the shared [`NAMESPACE_IDS`]/[`NEXT_NAMESPACE_ID`] globals.
pub unsafe fn nvim_create_namespace(name: &NvimString) -> Integer {
    // `Map::get_or_default` matches the original's own `map_get`
    // semantics exactly: a per-type default (`0` for `i32`) when the
    // key isn't present - including when `name` is empty, since an
    // empty name is never actually inserted into the map (see below),
    // so this lookup always "misses" for it, just like the original.
    // SAFETY: forwarded from this function's own safety doc.
    let mut id = unsafe { NAMESPACE_IDS.get_mut() }.get_or_default(name);
    if id > 0 {
        return i64::from(id);
    }

    // SAFETY: forwarded from this function's own safety doc.
    let next_id = unsafe { NEXT_NAMESPACE_ID.get_mut() };
    id = *next_id;
    *next_id += 1;
    if !name.is_empty() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { NAMESPACE_IDS.get_mut() }.insert(name.clone(), id);
    }
    i64::from(id)
}

/// Return all existing named namespaces (`nvim_get_namespaces`).
///
/// Anonymous namespaces are absent because they are never inserted
/// into [`NAMESPACE_IDS`].
///
/// # Safety
/// Reads the shared [`NAMESPACE_IDS`] registry.
#[must_use]
pub unsafe fn nvim_get_namespaces() -> Dict {
    unsafe { NAMESPACE_IDS.get_mut() }
        .iter()
        .map(|(name, id)| KeyValuePair {
            key: name.clone(),
            value: Object::Integer(i64::from(*id)),
        })
        .collect()
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

    /// `NAMESPACE_IDS`/`NEXT_NAMESPACE_ID` are real, session-lifetime
    /// registries shared across every test in the process - each test
    /// below uses its own uniquely-named namespace (never reused by
    /// any other test in this file) to avoid any cross-test
    /// collision, and asserts RELATIVE behavior (same name -> same
    /// id, different name -> different id) rather than exact numeric
    /// ids, since the counter's own starting point depends on
    /// whichever other tests already ran first in the same process.
    #[test]
    fn nvim_create_namespace_same_name_returns_the_same_id() {
        let _lock = crate::globals::global_state_test_lock();
        let name: NvimString = b"nero-test-ns-alpha".to_vec();
        let first = unsafe { nvim_create_namespace(&name) };
        let second = unsafe { nvim_create_namespace(&name) };
        assert_eq!(first, second);
        assert!(first > 0);
    }

    #[test]
    fn nvim_create_namespace_different_names_return_different_ids() {
        let _lock = crate::globals::global_state_test_lock();
        let a: NvimString = b"nero-test-ns-beta".to_vec();
        let b: NvimString = b"nero-test-ns-gamma".to_vec();
        let id_a = unsafe { nvim_create_namespace(&a) };
        let id_b = unsafe { nvim_create_namespace(&b) };
        assert_ne!(id_a, id_b);
    }

    #[test]
    fn nvim_get_namespaces_returns_named_but_not_anonymous_namespaces() {
        let _lock = crate::globals::global_state_test_lock();
        let name: NvimString = b"nero-test-ns-listing".to_vec();
        let named = unsafe { nvim_create_namespace(&name) };
        let anonymous = unsafe { nvim_create_namespace(&Vec::new()) };

        let namespaces = unsafe { nvim_get_namespaces() };

        assert!(namespaces.iter().any(|pair| {
            pair.key == name && matches!(pair.value, Object::Integer(id) if id == named)
        }));
        assert!(!namespaces
            .iter()
            .any(|pair| matches!(pair.value, Object::Integer(id) if id == anonymous)));
    }

    #[test]
    fn nvim_create_namespace_empty_name_always_creates_a_new_id() {
        let _lock = crate::globals::global_state_test_lock();
        let empty: NvimString = Vec::new();
        let first = unsafe { nvim_create_namespace(&empty) };
        let second = unsafe { nvim_create_namespace(&empty) };
        // An anonymous namespace is never registered by name, so 2
        // calls with an empty name always allocate 2 DIFFERENT ids -
        // unlike the same-name case above.
        assert_ne!(first, second);
        assert!(first > 0);
        assert!(second > 0);
    }
}
