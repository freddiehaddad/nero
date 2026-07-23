//! Translated from `src/nvim/runtime_defs.h` (partial).
//!
//! Translated: `etype_T`, `estack_T`, `estack_arg_T`, `DoInRuntimepathCB`,
//! and now `scriptvar_T`/`scriptitem_T` (as [`ScriptvarT`]/
//! [`ScriptitemT`]) - tractable now that the eval engine's `dict_T`/
//! `ScopeDictDictItem` both have real fields. `sv_dict: DictT` is
//! embedded *by value* here, not behind a pointer like every other use
//! of `DictT` in this crate so far - this works cleanly with the
//! existing `tv_dict_*` API in `eval/typval.rs` (which already only
//! ever takes a `*mut DictT`, never `Box<DictT>`), since `&mut
//! sv.sv_dict as *mut DictT` is just as valid a pointer as one obtained
//! via `tv_dict_alloc`'s own `Box::into_raw`, regardless of whether the
//! `DictT` happens to be independently heap-allocated or embedded
//! inside another struct's own memory - the first small-scale proof of
//! this, since scaled up to `funccall_T`'s own larger by-value
//! `dict_T`/`list_T` embedding (now also real, see
//! `eval/typval_defs.rs`'s own module doc).
//!
//! `ScriptitemT.sn_vars: *mut ScriptvarT` and everything else about
//! actually *constructing*/*looking up* a script item (`new_script_vars`,
//! `SCRIPT_ITEM`/`script_items` - a growable registry indexed by script
//! ID) remain deferred - see `eval/vars.rs`'s own module doc.

use crate::eval::typval_defs::{DictT, ScopeDictDictItem, SctxT, UfuncT};
use crate::ex_eval_defs::ExceptT;
use crate::pos_defs::LinenrT;
use crate::types_defs::{AutoPatCmdT, ProftimeT};

/// Discriminant for [`EstackT::es_info`] (`etype_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EtypeT {
    /// toplevel
    #[default]
    Top,
    /// sourcing script, use `es_info.sctx`
    Script,
    /// user function, use `es_info.ufunc`
    Ufunc,
    /// autocommand, use `es_info.aucmd`
    Aucmd,
    /// modeline, use `es_info.sctx`
    Modeline,
    /// exception, use `es_info.exception`
    Except,
    /// command line argument
    Args,
    /// environment variable
    Env,
    /// internal operation
    Internal,
    /// loading spell file
    Spell,
}

/// The payload of an [`EstackT`] entry, tagged by [`EtypeT`] (`es_info`
/// in the original: a C union of `sctx_T*`/`ufunc_T*`/`AutoPatCmd*`/
/// `except_T*`).
///
/// Translated as a proper Rust enum (each variant directly carries its
/// own pointer) rather than replicating the union + separate
/// `es_type` tag split, same reasoning as `Callback`/`UhLink` elsewhere
/// in this crate: there's no hot-path memory-layout reason here to avoid
/// a safe, self-tagged enum (one `estack_T` entry per execution-stack
/// frame, not a densely packed hot structure).
#[derive(Debug, Clone, Copy)]
pub enum EsInfo {
    /// script and modeline info
    Sctx(*mut SctxT),
    /// function info
    Ufunc(*mut UfuncT),
    /// autocommand info
    Aucmd(*mut AutoPatCmdT),
    /// exception info
    Except(*mut ExceptT),
}

impl Default for EsInfo {
    fn default() -> Self {
        EsInfo::Sctx(std::ptr::null_mut())
    }
}

/// Entry in the execution stack "exestack" (`estack_T`).
#[derive(Debug, Clone, Copy, Default)]
pub struct EstackT {
    /// replaces `"sourcing_lnum"`
    pub es_lnum: LinenrT,
    /// replaces `"sourcing_name"` (kept as a raw, possibly-null pointer:
    /// unlike an owned `char *name` field with an explicit "allocated"
    /// comment elsewhere in this crate, this one's ownership depends on
    /// `estack.c`'s push/pop logic, not yet translated - so a raw
    /// pointer is the honest choice for now, not an assumed owned copy).
    pub es_name: *mut u8,
    pub es_type: EtypeT,
    pub es_info: EsInfo,
}

/// Argument for `estack_sfile()` (not yet translated) (`estack_arg_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EstackArgT {
    #[default]
    None,
    Sfile,
    Stack,
    Script,
}

/// `DoInRuntimepathCB`.
pub type DoInRuntimepathCb = fn(i32, *mut *mut u8, bool, *mut std::ffi::c_void) -> bool;

/// Holds the hashtab with variables local to each sourced script
/// (`scriptvar_T`). See this module's own doc comment for the
/// by-value `DictT` embedding rationale.
///
/// Derives neither `Debug` nor `Default`, matching `DictT`'s own
/// convention (which derives neither either, embedded here by value) -
/// a "proper" `ScriptvarT` needs `eval/vars.rs`'s `init_var_dict` to
/// wire `sv_var` to point at `sv_dict`, the same reason `DictT` itself
/// avoids a naive `Default::default()`/derived-`Clone` shorthand that
/// could invite constructing one without that wiring.
pub struct ScriptvarT {
    /// Variable for `s:` scope (`sv_var`).
    pub sv_var: ScopeDictDictItem,
    /// `s:` variables themselves (`sv_dict`).
    pub sv_dict: DictT,
}

/// Info about a sourced script (`scriptitem_T`).
#[derive(Debug, Default)]
pub struct ScriptitemT {
    /// stores `s:` variables for this script (`sn_vars`).
    pub sn_vars: *mut ScriptvarT,
    /// script file name (`sn_name`).
    pub sn_name: Option<Vec<u8>>,
    /// `true` for a lua script (`sn_lua`).
    pub sn_lua: bool,
    /// `true` when script is/was profiled (`sn_prof_on`).
    pub sn_prof_on: bool,
    /// forceit: profile functions in this script (`sn_pr_force`).
    pub sn_pr_force: bool,
    /// time set when going into first child (`sn_pr_child`).
    pub sn_pr_child: ProftimeT,
    /// nesting for `sn_pr_child` (`sn_pr_nest`).
    pub sn_pr_nest: i32,
    // profiling the script as a whole.
    /// nr of times sourced (`sn_pr_count`).
    pub sn_pr_count: i32,
    /// time spent in script + children (`sn_pr_total`).
    pub sn_pr_total: ProftimeT,
    /// time spent in script itself (`sn_pr_self`).
    pub sn_pr_self: ProftimeT,
    /// time at script start (`sn_pr_start`).
    pub sn_pr_start: ProftimeT,
    /// time in children after script start (`sn_pr_children`).
    pub sn_pr_children: ProftimeT,
    // profiling the script per line.
    /// things stored for every line (`sn_prl_ga`).
    pub sn_prl_ga: crate::garray_defs::GarrayT,
    /// start time for current line (`sn_prl_start`).
    pub sn_prl_start: ProftimeT,
    /// time spent in children for this line (`sn_prl_children`).
    pub sn_prl_children: ProftimeT,
    /// wait start time for current line (`sn_prl_wait`).
    pub sn_prl_wait: ProftimeT,
    /// index of line being timed; -1 if none (`sn_prl_idx`).
    pub sn_prl_idx: LinenrT,
    /// line being timed was executed (`sn_prl_execed`).
    pub sn_prl_execed: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etype_default_is_top() {
        assert_eq!(EtypeT::default(), EtypeT::Top);
    }

    #[test]
    fn es_info_default_is_null_sctx_variant() {
        match EsInfo::default() {
            EsInfo::Sctx(p) => assert!(p.is_null()),
            _ => panic!("default EsInfo should be the Sctx variant"),
        }
    }

    #[test]
    fn estack_default_is_zeroed() {
        let es = EstackT::default();
        assert_eq!(es.es_lnum, 0);
        assert_eq!(es.es_type, EtypeT::Top);
        assert!(es.es_name.is_null());
    }

    #[test]
    fn estack_arg_default_is_none() {
        assert_eq!(EstackArgT::default(), EstackArgT::None);
    }

    #[test]
    fn scriptitem_default_is_zeroed_with_null_sn_vars() {
        let si = ScriptitemT::default();
        assert!(si.sn_vars.is_null());
        assert!(si.sn_name.is_none());
        assert!(!si.sn_lua);
        assert!(!si.sn_prof_on);
        assert_eq!(si.sn_pr_count, 0);
        assert_eq!(si.sn_prl_ga.ga_len, 0);
        assert_eq!(si.sn_prl_idx, 0);
    }

    #[test]
    fn scriptvar_can_be_wired_via_init_var_dict_and_linked_from_scriptitem() {
        let mut sv = ScriptvarT {
            sv_var: ScopeDictDictItem::default(),
            sv_dict: DictT {
                dv_lock: crate::eval::typval_defs::VarLockStatus::Unlocked,
                dv_scope: crate::eval::typval_defs::ScopeType::NoScope,
                dv_refcount: 0,
                dv_copy_id: 0,
                dv_hashtab: crate::hashtab_defs::HashtabT::hash_init(),
                dv_index: std::collections::HashMap::new(),
                dv_copydict: std::ptr::null_mut(),
                dv_used_next: std::ptr::null_mut(),
                dv_used_prev: std::ptr::null_mut(),
                lua_table_ref: -1,
            },
        };
        crate::eval::vars::init_var_dict(
            &mut sv.sv_dict,
            &mut sv.sv_var,
            crate::eval::typval_defs::ScopeType::Scope,
        );
        assert_eq!(sv.sv_dict.dv_scope, crate::eval::typval_defs::ScopeType::Scope);

        let mut si = ScriptitemT { sn_vars: &mut sv as *mut ScriptvarT, ..Default::default() };
        assert!(!si.sn_vars.is_null());
        // SAFETY: sv is still alive (a local, in scope for the whole
        // test body), and si.sn_vars was just set to point at it.
        unsafe {
            assert_eq!((*si.sn_vars).sv_dict.dv_scope, crate::eval::typval_defs::ScopeType::Scope);
        }
        si.sn_vars = std::ptr::null_mut(); // avoid a dangling reference lingering
    }
}
