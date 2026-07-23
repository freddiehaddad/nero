//! Translated from `src/nvim/eval/vars.c` (tractable core only).
//!
//! `vars.c` (~2700 lines) implements Vimscript variable get/set/unlet,
//! `:let`/`:unlet`/`:const` command execution, and the `g:`/`b:`/`w:`/
//! `t:`/`s:`/`l:`/`a:`/`v:` scope-dictionary machinery - almost all of
//! it needs the expression evaluator and `ex_cmds.lua`-generated
//! command dispatch, not attempted here.
//!
//! Translated: `init_var_dict` - the small, self-contained scope-dict
//! initializer shared by every scope (`s:`, and (per its own doc
//! comment) `b:`/`w:`/`t:` too, wherever those are eventually wired up
//! for real). Needed only already-existing pieces: `HashtabT::hash_init`,
//! `VarLockStatus`, `ScopeType`, `DO_NOT_FREE_CNT`, `dict_item_flags`.
//!
//! Also translated: `new_script_vars` - tractable now that
//! `crate::runtime`'s `script_items`/`new_script_item` exist. Builds a
//! fresh, zeroed `ScriptvarT` (matching the original's own
//! `xcalloc(1, sizeof(scriptvar_T))`, NOT
//! [`crate::eval::typval::tv_dict_alloc`], since a script-scope dict
//! has `dv_refcount == DO_NOT_FREE_CNT` and is deliberately never
//! linked into the `GC_FIRST_DICT` used-dicts list, matching the
//! original exactly), calls [`init_var_dict`] on it, then wires the
//! result into the script item at `id` via
//! `crate::runtime::script_item`.
//!
//! The original's `QUEUE_INIT(&dict->watchers)` is omitted - `DictT`
//! has no `watchers` field at all yet (needs a `QUEUE` intrusive-
//! linked-list translation first, same accepted gap as documented on
//! `DictT` itself in `eval/typval_defs.rs`).
//!
//! Deferred: everything else in this file (variable get/set/unlet,
//! `:let` parsing, etc.).

use crate::eval::typval_defs::{
    dict_item_flags, DictT, ScidT, ScopeDictDictItem, ScopeType, TypvalValue, VarLockStatus,
    DO_NOT_FREE_CNT,
};
use crate::runtime_defs::ScriptvarT;

/// `-1`, matching `LuaRef`'s "no reference" convention already
/// established (e.g. `eval/typval.rs`'s own private `LUA_NOREF`).
const LUA_NOREF: crate::types_defs::LuaRef = -1;

/// Initialize `dict` as a scope dict and set `dict_var` to point to it
/// (`init_var_dict`).
///
/// `dict`/`dict_var` are typically two sibling fields of a larger,
/// heap-allocated struct (e.g. [`crate::runtime_defs::ScriptvarT`]'s
/// `sv_dict`/`sv_var`) - `dict_var` ends up storing a raw pointer to
/// `dict`'s own address, so callers must ensure `dict` does not move
/// in memory for as long as `dict_var` (or anything that copies its
/// `di_tv` value) remains reachable - the same requirement as any
/// other long-lived `*mut DictT` elsewhere in this crate.
pub fn init_var_dict(dict: &mut DictT, dict_var: &mut ScopeDictDictItem, scope: ScopeType) {
    dict.dv_hashtab = crate::hashtab_defs::HashtabT::hash_init();
    dict.dv_lock = VarLockStatus::Unlocked;
    dict.dv_scope = scope;
    dict.dv_refcount = DO_NOT_FREE_CNT;
    dict.dv_copy_id = 0;
    dict_var.di_tv.value = TypvalValue::Dict(dict as *mut DictT);
    dict_var.di_tv.v_lock = VarLockStatus::Fixed;
    dict_var.di_flags = dict_item_flags::RO | dict_item_flags::FIX;
    dict_var.di_key = vec![0]; // empty NUL-terminated key, matching di_key[0] = NUL
    // QUEUE_INIT(&dict->watchers) omitted - see this module's own doc
    // comment.
}

/// Allocate a new hashtab for a sourced script. It will be used while
/// sourcing this script and when executing functions defined in the
/// script (`new_script_vars`).
///
/// # Panics
/// Panics if `id` is out of range - see
/// [`crate::runtime::script_item`]'s own doc comment. In practice this
/// never happens: this function is only ever called by
/// `crate::runtime::new_script_item` immediately after allocating the
/// slot at `id`, exactly mirroring the original's own call site.
pub fn new_script_vars(id: ScidT) {
    let mut sv = Box::new(ScriptvarT {
        sv_var: ScopeDictDictItem::default(),
        // A fresh, zeroed DictT - matches the original's own
        // xcalloc(1, sizeof(scriptvar_T)), NOT tv_dict_alloc: a
        // script-scope dict has dv_refcount == DO_NOT_FREE_CNT (set
        // below by init_var_dict) and must NOT be linked into the
        // GC_FIRST_DICT used-dicts list (dv_used_next/dv_used_prev
        // stay null), matching the original exactly - it lives for
        // the whole session, never garbage collected via the normal
        // refcount path.
        sv_dict: DictT {
            dv_lock: VarLockStatus::Unlocked,
            dv_scope: ScopeType::NoScope,
            dv_refcount: 0,
            dv_copy_id: 0,
            dv_hashtab: crate::hashtab_defs::HashtabT::hash_init(),
            dv_index: std::collections::HashMap::new(),
            dv_copydict: std::ptr::null_mut(),
            dv_used_next: std::ptr::null_mut(),
            dv_used_prev: std::ptr::null_mut(),
            lua_table_ref: LUA_NOREF,
        },
    });
    init_var_dict(&mut sv.sv_dict, &mut sv.sv_var, ScopeType::Scope);
    let sv_ptr = Box::into_raw(sv);
    let item = crate::runtime::script_item(id);
    // SAFETY: item is a valid pointer to a live ScriptitemT - forwarded
    // from crate::runtime::script_item's own contract, guaranteed by
    // this function's own doc comment above (id is always freshly
    // allocated by runtime::new_script_item just before calling this).
    unsafe { (*item).sn_vars = sv_ptr };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_var_dict_wires_dict_var_to_point_at_dict() {
        let mut dict = DictT {
            dv_lock: VarLockStatus::Locked,
            dv_scope: ScopeType::NoScope,
            dv_refcount: 999,
            dv_copy_id: 5,
            dv_hashtab: crate::hashtab_defs::HashtabT::hash_init(),
            dv_index: std::collections::HashMap::new(),
            dv_copydict: std::ptr::null_mut(),
            dv_used_next: std::ptr::null_mut(),
            dv_used_prev: std::ptr::null_mut(),
            lua_table_ref: -1,
        };
        let mut dict_var = ScopeDictDictItem::default();

        init_var_dict(&mut dict, &mut dict_var, ScopeType::Scope);

        assert_eq!(dict.dv_lock, VarLockStatus::Unlocked);
        assert_eq!(dict.dv_scope, ScopeType::Scope);
        assert_eq!(dict.dv_refcount, DO_NOT_FREE_CNT);
        assert_eq!(dict.dv_copy_id, 0);

        assert_eq!(dict_var.di_tv.v_lock, VarLockStatus::Fixed);
        assert_eq!(
            dict_var.di_flags,
            dict_item_flags::RO | dict_item_flags::FIX
        );
        assert_eq!(dict_var.di_key, vec![0]);
        match dict_var.di_tv.value {
            TypvalValue::Dict(p) => assert_eq!(p, &mut dict as *mut DictT),
            _ => panic!("expected a Dict-typed value"),
        }
    }

    #[test]
    fn init_var_dict_matches_def_scope_too() {
        let mut dict = DictT {
            dv_lock: VarLockStatus::Unlocked,
            dv_scope: ScopeType::NoScope,
            dv_refcount: 0,
            dv_copy_id: 0,
            dv_hashtab: crate::hashtab_defs::HashtabT::hash_init(),
            dv_index: std::collections::HashMap::new(),
            dv_copydict: std::ptr::null_mut(),
            dv_used_next: std::ptr::null_mut(),
            dv_used_prev: std::ptr::null_mut(),
            lua_table_ref: -1,
        };
        let mut dict_var = ScopeDictDictItem::default();

        init_var_dict(&mut dict, &mut dict_var, ScopeType::DefScope);

        assert_eq!(dict.dv_scope, ScopeType::DefScope);
    }

    // The following tests all touch crate::runtime's shared
    // SCRIPT_ITEMS/LAST_CURRENT_SID GlobalCells (indirectly, through
    // new_script_vars's own call to crate::runtime::script_item) -
    // each acquires global_state_test_lock() for its whole body and
    // resets crate::runtime's test-only state first, matching
    // crate::runtime's own test conventions exactly.

    #[test]
    fn new_script_vars_wires_a_fresh_scope_dict_into_the_script_item() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        let (sid, item) = crate::runtime::new_script_item(None);
        // new_script_item already called new_script_vars(sid) once as
        // part of allocating the slot - call it again directly to
        // exercise this function's own behavior in isolation too
        // (mirrors init_var_dict's own "call it again with different
        // inputs" test style above).
        new_script_vars(sid);
        unsafe {
            assert!(!(*item).sn_vars.is_null());
            let sv = &*(*item).sn_vars;
            assert_eq!(sv.sv_dict.dv_scope, ScopeType::Scope);
            assert_eq!(sv.sv_dict.dv_refcount, DO_NOT_FREE_CNT);
            assert_eq!(sv.sv_dict.dv_lock, VarLockStatus::Unlocked);
            assert!(sv.sv_dict.dv_used_next.is_null());
            assert!(sv.sv_dict.dv_used_prev.is_null());
            match sv.sv_var.di_tv.value {
                TypvalValue::Dict(p) => assert_eq!(p, &sv.sv_dict as *const DictT as *mut DictT),
                _ => panic!("expected a Dict-typed value"),
            }
        }
    }

    #[test]
    #[should_panic]
    fn new_script_vars_panics_for_out_of_range_sid() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        new_script_vars(42);
    }
}
