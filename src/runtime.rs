//! Translated from `src/nvim/runtime.c` (tractable core only).
//!
//! `runtime.c` (~2900 lines) is the runtime-path/script-sourcing
//! subsystem (`:runtime`, `:source`, `'runtimepath'` traversal,
//! per-script `:profile` reporting) - almost all of it needs real file
//! I/O, the expression evaluator, or the Lua host, none attempted
//! here.
//!
//! Translated: `script_items`/`SCRIPT_ITEM` (as `SCRIPT_ITEMS`/
//! [`script_item`] - the growable registry of all sourced scripts,
//! indexed by script ID) and `new_script_item` - tractable now that
//! `runtime_defs.rs`'s `ScriptitemT` has real fields and
//! `eval/vars.rs`'s `new_script_vars` exists. A plain
//! `Vec<*mut ScriptitemT>` here rather than a generic `GarrayT` (this
//! crate's usual byte-oriented growable-array translation for
//! untyped `void*`-shaped `garray_T` uses) since this particular
//! `garray_T` is always accessed through its own
//! `scriptitem_T **`-typed `SCRIPT_ITEM` macro, never through
//! `garray_T`'s generic byte-size-parameterized API - the same
//! "translate the registry using whatever Rust collection actually
//! matches its real usage" reasoning `eval/userfunc.rs`'s own
//! `FuncHashtab` already established for `func_hashtab`.
//!
//! `last_current_SID` (a C function-local `static` inside
//! `new_script_item` itself) becomes its own file-static
//! `LAST_CURRENT_SID`, mirroring `buffer.rs`'s own
//! `TOP_FILE_NUM`/`BUF_FREE_COUNT` treatment for the same kind of
//! per-file counter.
//!
//! Neither `SCRIPT_ITEMS` nor the `ScriptitemT`/`ScriptvarT`/`DictT`
//! values it points at are ever freed by this crate, matching the
//! original exactly: scripts accumulate for the whole nvim session
//! and are only torn down by `free_all_script_vars`
//! (`#ifdef EXITFREE`-gated shutdown cleanup, same accepted gap as
//! `eval/vars.c`'s own `evalvars_clear`) - not translated. This is a
//! genuine "leak for the process lifetime" design in the original
//! itself, not an oversight here.
//!
//! Deferred: everything else in this file (runtime-path search,
//! `:runtime`, `:scriptnames`, per-script `:profile` reporting,
//! script unloading/`GA_DEEP_CLEAR` teardown, etc.) - each needs real
//! file I/O and/or the expression evaluator.

use crate::eval::typval_defs::ScidT;
use crate::globals::GlobalCell;
use crate::runtime_defs::ScriptitemT;

/// `script_items` - the growable registry of all sourced scripts,
/// indexed by script ID minus one (`SCRIPT_ITEM(id)` in the original).
/// See this module's own doc comment for why this is a plain
/// `Vec<*mut ScriptitemT>` rather than a `GarrayT`.
///
/// Kept private, matching `eval/userfunc.rs`'s own `FUNC_HASHTAB`
/// encapsulation boundary - only reachable through this module's own
/// `pub fn`s ([`script_item`], [`new_script_item`]).
static SCRIPT_ITEMS: GlobalCell<Vec<*mut ScriptitemT>> = GlobalCell::new(Vec::new());

/// `last_current_SID` - see this module's own doc comment.
static LAST_CURRENT_SID: GlobalCell<ScidT> = GlobalCell::new(0);

/// Look up the script item for script ID `id` (`SCRIPT_ITEM(id)`).
///
/// # Panics
/// Panics if `id` is out of range (less than 1, or greater than the
/// number of scripts created so far via [`new_script_item`]) - the
/// original's own unchecked-array-access has no bounds check either,
/// so an out-of-range `id` is already a caller bug there too; this
/// just fails loudly instead of reading out of bounds.
#[must_use]
pub fn script_item(id: ScidT) -> *mut ScriptitemT {
    // SAFETY: SCRIPT_ITEMS is only ever read/written through this
    // module's own functions, none of which hold a live reference
    // across another call into this same cell.
    let items = unsafe { SCRIPT_ITEMS.get_mut() };
    items[(id - 1) as usize]
}

/// Create a new script item and allocate script-local vars
/// (`new_script_item`).
///
/// Returns `(sid, item)`: the new item's script ID and a pointer to
/// the created [`ScriptitemT`] - collapsing the original's
/// `scid_T *sid_out` out-parameter into part of the return value,
/// matching this crate's usual preference for a single meaningful
/// return over a C-style out-parameter.
///
/// `name` is `None` for an anonymous `:source` (matching the
/// original's `NULL`).
pub fn new_script_item(name: Option<Vec<u8>>) -> (ScidT, *mut ScriptitemT) {
    let sid = {
        // SAFETY: forwarded from script_item's own reasoning.
        let counter = unsafe { LAST_CURRENT_SID.get_mut() };
        *counter += 1;
        *counter
    };
    // SAFETY: forwarded from script_item's own reasoning.
    let items = unsafe { SCRIPT_ITEMS.get_mut() };
    while (items.len() as ScidT) < sid {
        let si = Box::into_raw(Box::new(ScriptitemT::default()));
        items.push(si);
        let new_sid = items.len() as ScidT;
        crate::eval::vars::new_script_vars(new_sid);
    }
    let si = items[(sid - 1) as usize];
    // SAFETY: si was just allocated via Box::into_raw above (in this
    // call, or a previous one if sid was already covered by the while
    // loop not running) and is never freed by this crate - see this
    // module's own doc comment.
    unsafe { (*si).sn_name = name };
    (sid, si)
}

/// The number of script items created so far via [`new_script_item`]
/// (`script_items.ga_len`) - the highest valid `id` [`script_item`]
/// will accept.
#[must_use]
pub fn script_item_count() -> ScidT {
    // SAFETY: forwarded from script_item's own reasoning.
    unsafe { SCRIPT_ITEMS.get_mut() }.len() as ScidT
}

/// Test-only: resets [`SCRIPT_ITEMS`]/[`LAST_CURRENT_SID`] to empty so
/// each test (in this module, or `eval::vars`'s own tests exercising
/// [`new_script_item`]/`new_script_vars` together) starts from a clean
/// slate. Unlike `eval::userfunc::func_init` (a real `pub fn`
/// translating the original's own `func_init`), the original has no
/// equivalent "re-init script_items" function - scripts accumulate for
/// the whole nvim session - so this helper is test-only, not a
/// translation of anything. `pub(crate)` (not `pub`) since it must
/// never be reachable from real, non-test code.
#[cfg(test)]
pub(crate) fn tests_reset_for_test() {
    // SAFETY: forwarded from script_item's own reasoning; every caller
    // holds global_state_test_lock() for its whole body, serializing
    // access across tests.
    unsafe {
        *SCRIPT_ITEMS.get_mut() = Vec::new();
        *LAST_CURRENT_SID.get_mut() = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    #[test]
    fn new_script_item_assigns_sequential_sids() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (sid1, _item1) = new_script_item(Some(b"first.vim".to_vec()));
        let (sid2, _item2) = new_script_item(Some(b"second.vim".to_vec()));
        assert_eq!(sid2, sid1 + 1);
    }

    #[test]
    fn new_script_item_sets_name_and_initializes_sn_vars() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (_sid, item) = new_script_item(Some(b"myscript.vim".to_vec()));
        unsafe {
            assert_eq!((*item).sn_name, Some(b"myscript.vim".to_vec()));
            assert!(!(*item).sn_vars.is_null());
        }
    }

    #[test]
    fn new_script_item_anonymous_source_has_no_name() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (_sid, item) = new_script_item(None);
        unsafe {
            assert!((*item).sn_name.is_none());
        }
    }

    #[test]
    fn script_item_looks_up_by_sid() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (sid, item) = new_script_item(None);
        assert_eq!(script_item(sid), item);
    }

    #[test]
    fn new_script_item_first_sid_is_one() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        let (sid, _item) = new_script_item(None);
        assert_eq!(sid, 1);
    }

    #[test]
    #[should_panic]
    fn script_item_panics_for_out_of_range_sid() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        new_script_item(None);
        let _ = script_item(99);
    }

    #[test]
    fn script_item_count_zero_when_none_registered() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        assert_eq!(script_item_count(), 0);
    }

    #[test]
    fn script_item_count_matches_the_number_created() {
        let _lock = global_state_test_lock();
        tests_reset_for_test();
        new_script_item(None);
        new_script_item(None);
        new_script_item(None);
        assert_eq!(script_item_count(), 3);
    }
}
