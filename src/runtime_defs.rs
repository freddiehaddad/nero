//! Translated from `src/nvim/runtime_defs.h` (partial).
//!
//! Translated: `etype_T`, `estack_T`, `estack_arg_T`, `DoInRuntimepathCB`.
//!
//! Deferred: `scriptvar_T`/`scriptitem_T` embed `dict_T`/`ScopeDictDictItem`
//! by value (`scriptvar_T.sv_dict: dict_T`) - needs the eval engine's
//! `typval_T` as a unit (phase 5), same blocker as `tabpage_S`.

use crate::eval::typval_defs::SctxT;
use crate::ex_eval_defs::ExceptT;
use crate::pos_defs::LinenrT;
use crate::types_defs::{AutoPatCmdT, UfuncT};

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
}
