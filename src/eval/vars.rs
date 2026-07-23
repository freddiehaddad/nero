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
//! The original's `QUEUE_INIT(&dict->watchers)` is omitted - `DictT`
//! has no `watchers` field at all yet (needs a `QUEUE` intrusive-
//! linked-list translation first, same accepted gap as documented on
//! `DictT` itself in `eval/typval_defs.rs`).
//!
//! Deferred: `new_script_vars` (needs `script_items`, a growable
//! registry of `ScriptitemT` indexed by script ID - not yet
//! translated, `runtime.c`/`eval/vars.c`'s own global state) and
//! everything else in this file (variable get/set/unlet, `:let`
//! parsing, etc.).

use crate::eval::typval_defs::{
    dict_item_flags, DictT, ScopeDictDictItem, ScopeType, TypvalValue, VarLockStatus,
    DO_NOT_FREE_CNT,
};

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
}
