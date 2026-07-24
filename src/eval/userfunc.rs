//! Translated from `src/nvim/eval/userfunc.c` (tractable core only).
//!
//! `userfunc.c` (~4300 lines) is the user-defined-function subsystem:
//! defining/parsing/calling/profiling Vimscript functions, `funccall_T`
//! construction/teardown, and so on - almost none of that is attempted
//! here.
//!
//! Translated: `func_ptr_ref`/`func_ptr_unref` - the two `ufunc_T`
//! reference-counting primitives that operate directly on a
//! `ufunc_T*`.
//!
//! Also translated: `func_hashtab` itself (the file-static registry of
//! all named functions, keyed by `uf_name`), `func_init`,
//! `func_tbl_get`, `find_func`, `func_remove`, plus a NEW (not in the
//! original) [`func_hashtab_add`] helper - see `FuncHashtab`'s own
//! doc comment for the design (mirrors `DictT`'s already-established
//! `dv_hashtab`/`dv_index` pair, for exactly the same reason: `uf_name`
//! is an owned `Vec<u8>` here, not a true C flexible array member, so
//! `HIKEY2UF`'s pointer-arithmetic recovery has no safe Rust
//! equivalent).
//!
//! Also translated: `func_name_refcount`, `func_ref`/`func_unref` (the
//! string-based siblings of `func_ptr_ref`/`func_ptr_unref` - only
//! numbered functions (`"123"`) and lambdas (`"<lambda>42"`) are
//! genuinely refcounted BY NAME; ordinary named functions aren't, so
//! these are usually no-ops). Both `name` parameters are bare bytes
//! with NO trailing NUL (matching `find_func`'s own convention, NOT
//! `UfuncT.uf_name`'s NUL-terminated storage form - callers reading a
//! name out of `uf_name` must strip the trailing NUL first, exactly
//! like `eval/typval.rs`'s `tv_dict_item_remove` already does for the
//! analogous `DictitemT.di_key` case). The original's
//! `internal_error("func_ref()")`/`internal_error("func_unref()")` -
//! reached only when a NUMBERED function reference has no backing
//! `UfuncT`, a genuine "should never happen" condition - become
//! `debug_assert!`s, matching this crate's established policy for
//! internal invariant violations (e.g. `mf_put`). The original's
//! further `EXITFREE`-build/`entered_free_all_mem`-shutdown carve-out
//! (which downgrades `func_unref`'s case from an abort to a silent
//! no-op specifically during process teardown) has no equivalent here
//! - this crate has no such shutdown-tracking state - so it's moot.
//!
//! `func_ptr_unref`'s "reference count hit zero, and the function
//! isn't mid-call" branch now calls the real `func_clear_free`
//! (`func_clear` + `func_free`).
//!
//! This needed several new pieces. `fc_flags` holds the `FC_*`
//! bit-flags. `fc_referenced` is trivial - every field it reads
//! already has real values. `previous_funccal` is a NEW file-static
//! `GlobalCell<*mut FunccallT>`, mirroring `func_hashtab`'s own
//! treatment - a bare pointer, not a hashtable, since
//! `previous_funccal` is just a singly-linked list via `fc_caller`.
//!
//! `free_funccal`/`free_funccal_contents` needed the most care: the
//! latter clears `fc_l_vars`/`fc_l_avars`'s hashtabs via
//! [`crate::eval::typval::tv_dict_free_contents`] and
//! `fc_l_varlist`'s items via
//! [`crate::eval::typval::tv_list_free_contents`] - both already
//! dispatch through `tv_clear_simple` per item, which this crate
//! treats as a faithful substitute for the original's
//! `encode_vim_to_nothing`-based `tv_clear`/`vars_clear_ext` for any
//! well-formed, acyclic value (the same guarantee Vimscript's own
//! reference-counted value model already provides).
//!
//! `func_clear_items` clears `uf_args`/`uf_def_args`/`uf_lines` via
//! plain `GarrayT::ga_clear()` rather than the original's
//! `ga_clear_strings`/`GA_DEEP_CLEAR_PTR` per-item `xfree` - correct
//! given nothing in this crate currently populates those arrays with
//! real heap-owned entries. Its `FC_LUAREF` branch is
//! `unimplemented!()` since nothing can currently set that flag
//! either, needing the Lua host (phase 13).
//!
//! Also translated (found via a full function-name diff of this file
//! against the real C source, the same methodology that mined
//! `eval/typval.c` out over several previous sessions): a new
//! `CURRENT_FUNCCAL` file-static (mirroring `PREVIOUS_FUNCCAL`'s own
//! design) plus `create_funccal`/`remove_funccal`/
//! `current_func_returned`/`can_free_funccal`/`free_unref_funccal`
//! (the funccall-lifecycle family built around it), and the small
//! `funccall_T *cookie`-based debugger-hook accessors `func_name`/
//! `func_breakpoint`/`func_dbg_tick`/`func_level`/`func_has_abort`/
//! `func_has_ended` (the last needing a new `aborted_in_try` in
//! `crate::ex_eval`, itself trivial - just reads the already-real
//! `GLOBALS.force_abort`).
//!
//! Also translated: `func_is_global`/`cat_func_name`/
//! `printable_func_name`/`function_list_modified` - small name-
//! formatting/hash-table-change-detection helpers. `cat_func_name`
//! returns a freshly-owned, never-truncated `Vec<u8>` rather than
//! writing into a caller-provided fixed-size buffer (see its own doc
//! comment for how it handles `uf_name`'s trailing-NUL ambiguity).
//! `function_list_modified` and `cat_func_name` are both translated
//! ahead of their real callers (`:function` listing code, none of
//! which is translated yet), matching this crate's established
//! precedent for small, self-contained, no-design-freedom functions.
//!
//! Also translated: `get_funccal`/`get_funccal_local_dict`/
//! `get_funccal_local_ht`/`get_funccal_local_var`/
//! `get_funccal_args_dict`/`get_funccal_args_ht`/`get_funccal_args_var`
//! (the `CURRENT_FUNCCAL`-plus-`debug_backtrace_level`-aware accessor
//! family - `debug_backtrace_level` already existed in `globals.rs`,
//! part of `globals.h`'s own translation from phase 3) and
//! `add_nr_var` (adds a read-only/fixed number variable such as `a:0`
//! to a dict, layered on the already-real `tv_dict_add`).
//! `get_funccal_local_var`/`get_funccal_args_var` return
//! `*mut ScopeDictDictItem` rather than the original's `dictitem_T*` -
//! see their own doc comments for why (the original relies on a
//! shared-layout reinterpret-cast this crate's distinct
//! `ScopeDictDictItem`/`DictitemT` types don't support, and don't need
//! to, since nothing calls these two yet).
//!
//! Also translated: `builtin_function` (whether a name looks like a
//! builtin, not a user-defined, function name - collapses the
//! original's `len == -1`-sentinel `strchr`/`memchr` dual path into
//! one already-bounded slice scan) and `check_user_func_argcount`
//! (validates a call's argument count against `fp`'s own arity),
//! plus a new [`FnameTransError`] enum (`eval/userfunc.h`'s small,
//! 9-variant `FnameTransError`).
//!
//! Also translated: `save_funccal`/`restore_funccal`/
//! `get_current_funccal`/`set_current_funccal` - a save/restore stack
//! for `CURRENT_FUNCCAL`, used when a totally different execution
//! context (autocommands, `:source`) needs to run with a temporarily
//! empty current funccall. New [`FuncCalEntryT`] (`funccal_entry_T`,
//! note the single "l" - a real, deliberate upstream spelling
//! distinction from `funccall_T`, preserved exactly) and a new
//! `FUNCCAL_STACK` file-static. `restore_funccal`'s
//! `iemsg("INTERNAL: ...")` on an empty stack becomes a
//! `debug_assert!`, matching this crate's established internal-
//! invariant-violation policy.
//!
//! Also translated: the previously-missing `get_funccal_args_ht` (its
//! own module-doc mention from an earlier pass never actually made it
//! into real code - caught and fixed by a fresh full function-name
//! diff re-scan) and `cleanup_function_call` - the real function-call
//! epilogue: frees `fc` outright if nothing keeps it alive, or else
//! bumps the reference count of any escaping `a:`/`l:` value (via the
//! already-real `tv_copy`) and links `fc` onto `PREVIOUS_FUNCCAL` for
//! later garbage collection. `free_funccal_contents`'s own doc comment
//! had already anticipated this exact function by name as its real
//! precondition, well before it was written. The original's own
//! `tv_copy(&di->di_tv, &di->di_tv)` self-copy idiom (explicitly
//! documented as supported by `tv_copy` itself) is translated as
//! clone-then-`tv_copy` instead of a literal simultaneous `&`/`&mut`
//! aliasing of the same memory - Rust's aliasing rules don't permit
//! the latter, unlike C, even though the net effect is identical. A
//! new `CLEANUP_FUNCTION_CALL_MADE_COPY` file-static mirrors the
//! original's own function-local `static int made_copy`. The
//! GC-nudge threshold (`4096 * 1024 / sizeof(*fc)`) keeps the same
//! formula but necessarily yields a different concrete number here,
//! since `size_of::<FunccallT>()` has a structurally different Rust
//! layout than the real C `funccall_T` - a heuristic memory-pressure
//! trip point, not a correctness-affecting value.
//!
//! Also translated: `can_add_defer` (whether currently inside a
//! function call - layered directly on the already-real
//! `get_current_funccal`). `add_defer`/`handle_defer_one`/
//! `invoke_all_defer` (`:defer`'s own storage/invocation) are NOT
//! translated - they need a new `DeferT` struct plus a change to
//! `FunccallT.fc_defer`'s current bare `GarrayT` type, and ultimately
//! the same `call_func` dispatch machinery most of this file's
//! remaining functions need.
//!
//! Also translated: `register_closure` - registers a `ufunc_T` (a
//! lambda/closure) as scoped to the current funccall, unreffing
//! whatever it was previously scoped to first via the already-real
//! `funccal_unref`. Has a genuine, explicitly-documented precondition:
//! `CURRENT_FUNCCAL.fc_ufuncs` must already be `ga_init`'d with
//! `itemsize == size_of::<*mut UfuncT>()`, matching `call_user_func`'s
//! own real setup step (not yet translated) - `create_funccal` alone
//! faithfully leaves `fc_ufuncs` at its bare zero-initialized state,
//! exactly like the original's own `create_funccal`/`call_user_func`
//! split, so tests exercising `register_closure` must set this up
//! explicitly first.

use crate::ascii_defs::ascii_isdigit;
use crate::eval::typval_defs::{
    DictT, DictitemT, FunccallT, ScopeDictDictItem, TypvalT, TypvalValue, UfuncT, VarLockStatus,
    VarnumberT, dict_item_flags,
};
use crate::globals::GlobalCell;
use crate::hashtab::hashitem_empty;
use crate::hashtab_defs::HashtabT;
use crate::vim_defs::OK;
use std::collections::HashMap;
use std::sync::LazyLock;

/// The function hash table (`func_hashtab`) plus this crate's own side
/// index (`index`) - mirroring `DictT`'s `dv_hashtab`/`dv_index` pair
/// (`eval/typval_defs.rs`), for exactly the same reason: `UfuncT.uf_name`
/// is an owned `Vec<u8>` here (not a true C flexible array member), so
/// `HIKEY2UF`'s `hi_key - offsetof(ufunc_T, uf_name)` pointer-arithmetic
/// recovery has no safe Rust equivalent. `index` maps each live item's
/// `hi_key` address (the `uf_name` buffer's own pointer, as a `usize`)
/// to its owning `*mut UfuncT`, populated/depopulated in lockstep with
/// `ht` by [`func_hashtab_add`]/[`func_remove`] - never read/written
/// independently of `ht`, exactly like `dv_index`.
///
/// `uf_name` must be NUL-terminated for any `UfuncT` added here,
/// matching `DictitemT.di_key`'s own established convention (`hi_key`
/// is ultimately read back as a C string by `hashtab.rs`'s internals).
struct FuncHashtab {
    ht: HashtabT,
    index: HashMap<usize, *mut UfuncT>,
}

impl Default for FuncHashtab {
    fn default() -> Self {
        FuncHashtab { ht: HashtabT::hash_init(), index: HashMap::new() }
    }
}

/// `func_hashtab` itself - a file-static in the original, kept private
/// here too (only reachable through this module's own `pub fn`s,
/// matching the encapsulation boundary the original itself draws via
/// `func_tbl_get()`).
static FUNC_HASHTAB: LazyLock<GlobalCell<FuncHashtab>> =
    LazyLock::new(|| GlobalCell::new(FuncHashtab::default()));

/// (Re-)initialize the function hash table (`func_init`).
pub fn func_init() {
    *unsafe { FUNC_HASHTAB.get_mut() } = FuncHashtab::default();
}

/// Return the function hash table (`func_tbl_get`).
pub fn func_tbl_get() -> *mut HashtabT {
    unsafe { &mut FUNC_HASHTAB.get_mut().ht as *mut HashtabT }
}

/// Add `fp` to the function hash table, keyed by its own `uf_name`.
///
/// NEW: not a separate function in the original, which inlines
/// `hash_add(&func_hashtab, UF2HIKEY(fp))` directly at its 3 real call
/// sites (inside `get_lambda_tv`/user-function-definition/lua-function
/// registration code, none of which are translated yet). Factored out
/// here so `ht`/`FuncHashtab::index` are always updated together,
/// atomically - exactly the same reasoning that already justifies
/// `dv_index` existing at all, and why `tv_dict_add` wraps `hash_add`
/// rather than leaving every caller to call `hash_add` directly.
///
/// Returns `FAIL` if a function with the same name already exists
/// (matching [`HashtabT::hash_add`]'s own contract), `OK` otherwise.
///
/// # Safety
/// `fp` must be a valid, non-null pointer to a live [`UfuncT`] whose
/// `uf_name` is NUL-terminated, outliving this hashtable entry, and
/// not already present in this table.
pub unsafe fn func_hashtab_add(fp: *mut UfuncT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let key_ptr = unsafe { (*fp).uf_name.as_mut_ptr() as *mut std::os::raw::c_char };
    let table = unsafe { FUNC_HASHTAB.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let rc = unsafe { table.ht.hash_add(key_ptr) };
    if rc == OK {
        table.index.insert(key_ptr as usize, fp);
    }
    rc
}

/// Find a function by name, return pointer to it in ufuncs
/// (`find_func`).
///
/// Returns a null pointer for an unknown function.
#[must_use]
pub fn find_func(name: &[u8]) -> *mut UfuncT {
    let table = unsafe { FUNC_HASHTAB.get_mut() };
    let hi = table.ht.hash_find(name);
    if hashitem_empty(hi) {
        return std::ptr::null_mut();
    }
    table
        .index
        .get(&(hi.hi_key as usize))
        .copied()
        .unwrap_or(std::ptr::null_mut())
}

/// Remove the function from the function hash table. If the function
/// was deleted while it still has references this was already done
/// (`func_remove`).
///
/// Returns `true` if the entry was deleted, `false` if it wasn't
/// found.
///
/// # Safety
/// `fp` must be a valid, non-null pointer to a live [`UfuncT`]
/// previously added via [`func_hashtab_add`] (or not present at all,
/// in which case this is a no-op returning `false`).
pub unsafe fn func_remove(fp: *mut UfuncT) -> bool {
    let table = unsafe { FUNC_HASHTAB.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let name: &[u8] = unsafe { &(*fp).uf_name };
    // hash_find/hash_remove take the bare key bytes (no trailing NUL) -
    // strip it, mirroring tv_dict_item_remove's own established
    // convention for the same reason.
    let name = &name[..name.len().saturating_sub(1)];
    let key_ptr = {
        let hi = table.ht.hash_find(name);
        if hashitem_empty(hi) {
            return false;
        }
        hi.hi_key as usize
    };
    table.ht.hash_remove(name);
    table.index.remove(&key_ptr);
    true
}

/// `keycodes.h`'s `K_SPECIAL` (0x80) - the lead byte marking an
/// internally-encoded special key, also used as the first byte of a
/// script-local function's encoded `<SNR>` name prefix. Defined
/// locally here (no `keycodes.rs` exists yet) since `func_is_global`
/// is currently this crate's only real dependency on it.
const K_SPECIAL: u8 = 0x80;

/// Returns `true` if `ufunc` is a global function (`func_is_global`).
fn func_is_global(ufunc: &UfuncT) -> bool {
    ufunc.uf_name.first() != Some(&K_SPECIAL)
}

/// Copy the function name of `fp`, taking care of script-local
/// function names (`cat_func_name`).
///
/// Unlike the original (writes into a caller-provided fixed-size
/// `buf`/`bufsize`, truncating the result if it doesn't fit), returns
/// a freshly-owned, never-truncated `Vec<u8>` - Rust's own `Vec` has
/// no such fixed-size constraint to work around. A single trailing
/// NUL byte on `fp.uf_name` (present once a function has been added
/// to the hash table via [`func_hashtab_add`], per its own documented
/// precondition) is stripped first if present, since a NUL can never
/// be a function name's own meaningful last byte.
#[must_use]
pub fn cat_func_name(fp: &UfuncT) -> Vec<u8> {
    let name = fp.uf_name.as_slice();
    let clean = match name.last() {
        Some(0) => &name[..name.len() - 1],
        _ => name,
    };
    if !func_is_global(fp) && clean.len() > 3 {
        let mut result = b"<SNR>".to_vec();
        result.extend_from_slice(&clean[3..]);
        result
    } else {
        clean.to_vec()
    }
}

/// Returns `fp`'s "print name": `uf_name_exp` if set (populated when
/// `uf_name` itself starts with the internally-encoded `<SNR>`
/// prefix), otherwise `uf_name` verbatim (`printable_func_name`).
#[must_use]
pub fn printable_func_name(fp: &UfuncT) -> Vec<u8> {
    fp.uf_name_exp.clone().unwrap_or_else(|| fp.uf_name.clone())
}

/// When `prev_ht_changed` does not equal the function hash table's own
/// current change counter, give an error (skipped - message display,
/// not tractable) and return `true`; otherwise return `false`
/// (`function_list_modified`).
///
/// Used by `:function` listing code (not yet translated) to detect a
/// callback deleting/redefining functions mid-listing. Translated
/// ahead of that real caller since it is small, self-contained, and
/// mechanically correct with no design freedom to get wrong, matching
/// this crate's established precedent for such functions.
#[must_use]
pub fn function_list_modified(prev_ht_changed: i32) -> bool {
    let table = unsafe { FUNC_HASHTAB.get_mut() };
    prev_ht_changed != table.ht.ht_changed
}

/// Whether `name` could be a builtin function name: starts with a
/// lower-case letter and doesn't contain `':'` at index 1 or the
/// autoload separator (`'#'`, `AUTOLOAD_CHAR`) anywhere
/// (`builtin_function`).
///
/// Unlike the original (a `len == -1` sentinel selecting between
/// `strchr`'s whole-NUL-terminated-string scan and `memchr`'s
/// `len`-bounded scan), `name` is a plain, already-bounded byte slice,
/// with the caller responsible for slicing it to the relevant length
/// first, matching this crate's usual "a slice already carries its
/// own bounds" simplification (e.g. `path_fnamencmp`/`vim_strnsize`).
#[must_use]
pub fn builtin_function(name: &[u8]) -> bool {
    let Some(&first) = name.first() else { return false };
    if !crate::macros_defs::ascii_islower(first as i32) || name.get(1) == Some(&b':') {
        return false;
    }
    !name.contains(&b'#')
}

/// Errors for when calling a function (`FnameTransError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnameTransError {
    Unknown = 0,
    TooMany = 1,
    TooFew = 2,
    Script = 3,
    Dict = 4,
    None = 5,
    Other = 6,
    Deleted = 7,
    /// Function cannot be used as a method (`FCERR_NOTMETHOD`).
    NotMethod = 8,
}

/// Check the argument count for user function `fp`
/// (`check_user_func_argcount`).
///
/// @return [`FnameTransError::Unknown`] if OK, [`FnameTransError::TooFew`]
/// or [`FnameTransError::TooMany`] otherwise.
#[must_use]
pub fn check_user_func_argcount(fp: &UfuncT, argcount: i32) -> FnameTransError {
    let regular_args = fp.uf_args.ga_len;
    if argcount < regular_args - fp.uf_def_args.ga_len {
        FnameTransError::TooFew
    } else if fp.uf_varargs == 0 && argcount > regular_args {
        FnameTransError::TooMany
    } else {
        FnameTransError::Unknown
    }
}

/// Whether a function name is genuinely refcounted BY NAME
/// (`func_name_refcount`): numbered functions (`"123"`) and lambdas
/// (`"<lambda>42"`) are; an ordinary named function's `ufunc_T` lives
/// for the whole script's lifetime once defined, so it isn't.
fn func_name_refcount(name: &[u8]) -> bool {
    match name.first() {
        Some(&b) => ascii_isdigit(b as i32) || (b == b'<' && name.get(1) == Some(&b'l')),
        None => false,
    }
}

/// Count a reference to a Function (`func_ref`) - the string-based
/// sibling of [`func_ptr_ref`]. See this module's own doc comment for
/// `name`'s expected bare-bytes form and the `debug_assert!`
/// translation policy for the original's internal-error report.
pub fn func_ref(name: Option<&[u8]>) {
    let Some(name) = name else { return };
    if !func_name_refcount(name) {
        return;
    }
    let fp = find_func(name);
    if !fp.is_null() {
        // SAFETY: `find_func` only ever returns a pointer it looked up
        // from `FUNC_HASHTAB`'s own table, or null.
        unsafe { func_ptr_ref(fp) };
    } else {
        debug_assert!(
            !ascii_isdigit(name[0] as i32),
            "func_ref: numbered function not found - internal error (original: func_ref())"
        );
    }
}

/// Unreference a Function (`func_unref`) - the string-based sibling of
/// [`func_ptr_unref`]. See this module's own doc comment for `name`'s
/// expected bare-bytes form and the `debug_assert!` translation policy
/// for the original's internal-error report.
pub fn func_unref(name: Option<&[u8]>) {
    let Some(name) = name else { return };
    if !func_name_refcount(name) {
        return;
    }
    let fp = find_func(name);
    debug_assert!(
        !(fp.is_null() && ascii_isdigit(name[0] as i32)),
        "func_unref: numbered function not found - internal error (original: func_unref())"
    );
    // SAFETY: `fp` is either null (func_ptr_unref's own null check
    // handles this, matching the original's own unconditional call)
    // or a valid pointer just looked up from `FUNC_HASHTAB`'s table.
    unsafe { func_ptr_unref(fp) };
}

/// Function flags, values for `uf_flags` (`FC_*`, `eval/userfunc.h`).
pub mod fc_flags {
    /// abort function on error (`FC_ABORT`).
    pub const ABORT: i32 = 0x01;
    /// function accepts range (`FC_RANGE`).
    pub const RANGE: i32 = 0x02;
    /// Dict function, uses "self" (`FC_DICT`).
    pub const DICT: i32 = 0x04;
    /// closure, uses outer scope variables (`FC_CLOSURE`).
    pub const CLOSURE: i32 = 0x08;
    /// `:delfunction` used while `uf_refcount > 0` (`FC_DELETED`).
    pub const DELETED: i32 = 0x10;
    /// function redefined while `uf_refcount > 0` (`FC_REMOVED`).
    pub const REMOVED: i32 = 0x20;
    /// function defined in the sandbox (`FC_SANDBOX`).
    pub const SANDBOX: i32 = 0x40;
    /// no `a:` variables in lambda (`FC_NOARGS`).
    pub const NOARGS: i32 = 0x200;
    /// luaref callback (`FC_LUAREF`).
    pub const LUAREF: i32 = 0x800;
}

/// Check whether funccall is still referenced outside (`fc_referenced`).
///
/// It is supposed to be referenced if either it is referenced itself
/// or if `l:`, `a:`, or `a:000` are referenced, as all these are
/// statically (by-value, in this crate) allocated within the funccall
/// structure.
#[must_use]
fn fc_referenced(fc: &FunccallT) -> bool {
    fc.fc_l_varlist.lv_refcount != crate::eval::typval_defs::DO_NOT_FREE_CNT
        || fc.fc_l_vars.dv_refcount != crate::eval::typval_defs::DO_NOT_FREE_CNT
        || fc.fc_l_avars.dv_refcount != crate::eval::typval_defs::DO_NOT_FREE_CNT
        || fc.fc_refcount > 0
}

/// `previous_funccal`: file-static list of funccalls no longer current
/// but still possibly referenced, linked via `fc_caller`. Kept as a
/// bare `*mut FunccallT` (not a hashtable, unlike `FUNC_HASHTAB`) since
/// the original itself is just a singly-linked list.
static PREVIOUS_FUNCCAL: LazyLock<GlobalCell<*mut FunccallT>> =
    LazyLock::new(|| GlobalCell::new(std::ptr::null_mut()));

/// `current_funccal`: file-static pointer to the funccall currently
/// being executed, or null when not inside any user function.
static CURRENT_FUNCCAL: LazyLock<GlobalCell<*mut FunccallT>> =
    LazyLock::new(|| GlobalCell::new(std::ptr::null_mut()));

/// A save/restore point for `CURRENT_FUNCCAL` (`funccal_entry_T`,
/// spelled with a single "l" like `current_funccal`/`funccal_stack`
/// themselves, unlike `funccall_T`/`FunccallT` - a real, deliberate
/// upstream spelling distinction, preserved here).
///
/// `top_funccal` is `*mut FunccallT` here (the original's `void
/// *top_funccal` is only untyped due to header include-order
/// constraints - it always actually holds a `funccall_T*`).
///
/// Caller-allocated, matching the original's own convention (a real
/// call site typically stack-allocates one of these before calling
/// [`save_funccal`]).
#[derive(Debug, Default)]
pub struct FuncCalEntryT {
    pub top_funccal: *mut FunccallT,
    pub next: *mut FuncCalEntryT,
}

/// `funccal_stack`: file-static singly-linked stack of
/// [`FuncCalEntryT`]s pushed by [`save_funccal`]/popped by
/// [`restore_funccal`].
static FUNCCAL_STACK: LazyLock<GlobalCell<*mut FuncCalEntryT>> =
    LazyLock::new(|| GlobalCell::new(std::ptr::null_mut()));

/// Save the current function call pointer, and set it to `None` -
/// used when executing autocommands and for ":source" (`save_funccal`).
///
/// # Safety
/// `entry` must be a valid, non-null pointer to storage that outlives
/// its later removal from the stack via [`restore_funccal`].
pub unsafe fn save_funccal(entry: *mut FuncCalEntryT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*entry).top_funccal = *CURRENT_FUNCCAL.get_mut();
        (*entry).next = *FUNCCAL_STACK.get_mut();
        *FUNCCAL_STACK.get_mut() = entry;
        *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut();
    }
}

/// Restore `CURRENT_FUNCCAL` from the top of `funccal_stack`
/// (`restore_funccal`).
///
/// The original's `iemsg("INTERNAL: restore_funccal()")` - reached
/// only when called without a matching, still-pending
/// [`save_funccal`], a genuine "should never happen" caller-contract
/// violation - becomes a `debug_assert!`, matching this crate's
/// established policy for internal invariant violations (e.g.
/// `mf_put`).
pub fn restore_funccal() {
    // SAFETY: only reads/writes the file-static FUNCCAL_STACK/
    // CURRENT_FUNCCAL cells; any non-null top was previously pushed by
    // save_funccal, per this crate's own standing invariant for these
    // two cells (matching PREVIOUS_FUNCCAL/CURRENT_FUNCCAL elsewhere
    // in this file).
    let top = unsafe { *FUNCCAL_STACK.get_mut() };
    debug_assert!(!top.is_null(), "INTERNAL: restore_funccal()");
    if !top.is_null() {
        unsafe {
            *CURRENT_FUNCCAL.get_mut() = (*top).top_funccal;
            *FUNCCAL_STACK.get_mut() = (*top).next;
        }
    }
}

/// The currently active funccall, or null if none (`get_current_funccal`).
#[must_use]
pub fn get_current_funccal() -> *mut FunccallT {
    unsafe { *CURRENT_FUNCCAL.get_mut() }
}

/// Set the currently active funccall directly, bypassing the
/// `funccal_stack` save/restore mechanism (`set_current_funccal`).
pub fn set_current_funccal(fc: *mut FunccallT) {
    unsafe { *CURRENT_FUNCCAL.get_mut() = fc };
}

/// Whether currently inside a function call (`can_add_defer`).
///
/// The original's `semsg(_(e_str_not_inside_function), "defer")` on
/// `false` is omitted (message display, not tractable yet) - the
/// boolean result itself is kept exactly. `add_defer`/
/// `handle_defer_one`/`invoke_all_defer` (`:defer`'s own storage and
/// invocation) are NOT translated here - they need a new `DeferT`
/// struct plus a change to `FunccallT.fc_defer`'s current bare
/// `GarrayT` type, and ultimately the same function-call dispatch
/// machinery (`call_func`) most of this file's remaining functions
/// need, so are left as part of that larger, separate undertaking.
#[must_use]
pub fn can_add_defer() -> bool {
    !get_current_funccal().is_null()
}

/// Free `fc` (`free_funccal`).
///
/// # Safety
/// `fc` must be a valid, non-null pointer previously allocated via
/// `Box::into_raw`. Every entry written into `fc.fc_ufuncs` must be
/// either null or a valid pointer to a live `UfuncT` (matching this
/// crate's own convention for populating it via
/// `GarrayT::ga_append_item::<*mut UfuncT>`, mirroring the original's
/// `((ufunc_T **)fc_ufuncs.ga_data)[...] = fp`); `fc.fc_func` must be
/// either null or a valid pointer to a live `UfuncT`.
unsafe fn free_funccal(fc: *mut FunccallT) {
    // SAFETY: forwarded from this function's own safety doc.
    let fc_ref = unsafe { &mut *fc };
    let count = fc_ref.fc_ufuncs.ga_len.max(0) as usize;
    // SAFETY: forwarded from this function's own safety doc - every
    // slot in `0..count` was written as a `*mut UfuncT`.
    let base = fc_ref.fc_ufuncs.ga_data.as_mut_ptr() as *mut *mut UfuncT;
    for i in 0..count {
        // SAFETY: `i < count == ga_len`, in-bounds; forwarded from this
        // function's own safety doc.
        let fp = unsafe { *base.add(i) };
        // When garbage collecting, a funccall_T may be freed before the
        // function that references it - clear its uf_scoped field. The
        // function may have been redefined and point to another
        // funccal_T, don't clear it then.
        if !fp.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let scoped_matches = unsafe { (*fp).uf_scoped == fc };
            if scoped_matches {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { (*fp).uf_scoped = std::ptr::null_mut() };
            }
        }
    }
    fc_ref.fc_ufuncs.ga_clear();

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { func_ptr_unref(fc_ref.fc_func) };
    // SAFETY: forwarded from this function's own safety doc (`fc` was
    // allocated via `Box::into_raw`).
    drop(unsafe { Box::from_raw(fc) });
}

/// Recursion-free "have we made lots of copies lately" counter for
/// [`cleanup_function_call`] - matches the original's own function-
/// local `static int made_copy`.
#[allow(dead_code)] // no real translated caller yet (cleanup_function_call's own only real caller, call_user_func, isn't translated) - tested directly, matching this crate's established convention for private helpers harvested ahead of their real caller
static CLEANUP_FUNCTION_CALL_MADE_COPY: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);

/// Clean up after a function call: free `fc` if nothing keeps it
/// alive, or else bump the reference count of any `a:`/`l:` value
/// that's escaping (e.g. returned as `a:000`, assigned to a global, or
/// captured by a closure) and link `fc` onto `PREVIOUS_FUNCCAL` for
/// later garbage collection (`cleanup_function_call`).
///
/// # Safety
/// `fc` must be a valid, non-null pointer to a live `FunccallT` whose
/// `fc_l_avars`/`fc_l_varlist` items, if any escape (kept when
/// `free_fc` ends up `false`), satisfy [`crate::eval::typval::tv_copy`]'s
/// own safety contract; if `fc` ends up freed, it must satisfy
/// [`free_funccal`]'s own safety contract.
#[allow(dead_code)] // no real translated caller yet (call_user_func, its only real caller, isn't translated) - tested directly, matching this crate's established convention for private helpers harvested ahead of their real caller
unsafe fn cleanup_function_call(fc: *mut FunccallT) {
    // SAFETY: forwarded from this function's own safety doc.
    let fc_ref = unsafe { &mut *fc };
    let may_free_fc = fc_ref.fc_refcount <= 0;
    let mut free_fc = true;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *CURRENT_FUNCCAL.get_mut() = fc_ref.fc_caller };

    // Free all l: variables if not referred.
    if may_free_fc && fc_ref.fc_l_vars.dv_refcount == crate::eval::typval_defs::DO_NOT_FREE_CNT {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::vars::vars_clear(&mut fc_ref.fc_l_vars) };
    } else {
        free_fc = false;
    }

    // If the a:000 list and the l: and a: dicts are not referenced and
    // there is no closure using it, we can free the funccall_T and
    // what's in it.
    if may_free_fc && fc_ref.fc_l_avars.dv_refcount == crate::eval::typval_defs::DO_NOT_FREE_CNT {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::vars::vars_clear_ext(&mut fc_ref.fc_l_avars, false) };
    } else {
        free_fc = false;

        // Make a copy of the a: variables, since we didn't do that
        // above. Bumps any List/Dict/Blob/Partial value's own
        // reference count (or deep-clones an owned String) via
        // tv_copy - matching the original's own `tv_copy(&di->di_tv,
        // &di->di_tv)` self-copy idiom, but routed through a fresh
        // `old` clone first: Rust's aliasing rules don't allow a live
        // `&TypvalT` and `&mut TypvalT` to the SAME location
        // simultaneously (unlike C), even though the original's own
        // doc comment on `tv_copy` explicitly allows `from`/`to` to
        // coincide. Cloning first, then calling `tv_copy(&old, &mut
        // real)`, achieves the exact same net effect (the "from" value
        // tv_copy reads is byte-for-byte identical to what a true
        // self-copy would have read) with no actual memory aliasing.
        let items: Vec<*mut DictitemT> = fc_ref.fc_l_avars.dv_index.values().copied().collect();
        for item in items {
            // SAFETY: item is a live DictitemT from fc_l_avars's own
            // dv_index.
            let old = unsafe { (*item).di_tv.clone() };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_copy(&old, &mut (*item).di_tv) };
        }
    }

    if may_free_fc && fc_ref.fc_l_varlist.lv_refcount == crate::eval::typval_defs::DO_NOT_FREE_CNT {
        fc_ref.fc_l_varlist.lv_first = std::ptr::null_mut();
    } else {
        free_fc = false;

        // Make a copy of the a:000 items, since we didn't do that
        // above - see the fc_l_avars loop above for why this clones
        // first rather than literally self-copying.
        let mut cur = fc_ref.fc_l_varlist.lv_first;
        while !cur.is_null() {
            // SAFETY: cur walks fc_l_varlist's own real lv_first/
            // li_next chain.
            let old = unsafe { (*cur).li_tv.clone() };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_copy(&old, &mut (*cur).li_tv) };
            // SAFETY: forwarded from this function's own safety doc.
            cur = unsafe { (*cur).li_next };
        }
    }

    if free_fc {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { free_funccal(fc) };
    } else {
        // "fc" is still in use. This can happen when returning
        // "a:000", assigning "l:" to a global variable or defining a
        // closure. Link "fc" in the list for garbage collection later.
        // SAFETY: forwarded from this function's own safety doc.
        fc_ref.fc_caller = unsafe { *PREVIOUS_FUNCCAL.get_mut() };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = fc };

        // SAFETY: only touches this module's own file-static cell.
        let made_copy = unsafe { CLEANUP_FUNCTION_CALL_MADE_COPY.get_mut() };
        // SAFETY: forwarded from this function's own safety doc.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        if g.want_garbage_collect {
            // If garbage collector is ready, clear count.
            *made_copy = 0;
        } else {
            *made_copy += 1;
            // The original computes this threshold from
            // `sizeof(*fc)` (the real C `funccall_T`'s own byte size)
            // - `FunccallT`'s Rust layout is structurally different
            // (owned `Vec`s/`HashMap`s instead of raw C arrays/
            // pointers), so `size_of::<FunccallT>()` gives a genuinely
            // different byte count here. This only changes the exact
            // trip point of a "we've made a lot of copies, worth ~4
            // Mbyte, nudge the GC" heuristic - not a correctness-
            // affecting value - so the same formula is kept as-is
            // rather than hand-picking a different constant.
            let threshold = (4096 * 1024) / std::mem::size_of::<FunccallT>() as i32;
            if *made_copy >= threshold {
                *made_copy = 0;
                g.want_garbage_collect = true;
            }
        }
    }
}

/// Free `fc` and what it contains (`free_funccal_contents`).
///
/// Can be called only when `fc` is kept beyond the period it was
/// called, i.e. after [`cleanup_function_call`] (its real caller,
/// `call_user_func`, isn't translated yet, so nothing currently
/// constructs a `FunccallT` this way in practice).
///
/// # Safety
/// Same as [`free_funccal`], plus every item in `fc.fc_l_vars`/
/// `fc.fc_l_avars`/`fc.fc_l_varlist` must satisfy `tv_clear_simple`'s
/// own safety contract (matching `tv_dict_free_contents`/
/// `tv_list_free_contents`'s own requirements, since that's what this
/// function calls on each embedded-by-value container).
unsafe fn free_funccal_contents(fc: *mut FunccallT) {
    // SAFETY: forwarded from this function's own safety doc. Unlike
    // `tv_dict_free`, this only clears the hashtab's ITEMS (matching
    // the original's `vars_clear`) - `fc_l_vars`/`fc_l_avars` are
    // embedded by value in `FunccallT`, not separately heap-allocated,
    // so the DictT struct itself must not be freed here.
    unsafe { crate::eval::typval::tv_dict_free_contents(&mut (*fc).fc_l_vars) };
    // SAFETY: same as above, for `fc_l_avars`.
    unsafe { crate::eval::typval::tv_dict_free_contents(&mut (*fc).fc_l_avars) };
    // SAFETY: same reasoning as above, for the by-value `fc_l_varlist`
    // - `tv_list_free_contents` only clears items, doesn't free the
    // `ListT` struct itself.
    unsafe { crate::eval::typval::tv_list_free_contents(&mut (*fc).fc_l_varlist) };

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { free_funccal(fc) };
}

/// Unreference `fc`: decrement its reference count and free it once it
/// becomes zero (`funccal_unref`). `fp` is detached from `fc`.
///
/// # Safety
/// If `fc` is non-null, it must be a valid pointer to a live
/// `FunccallT` satisfying [`free_funccal_contents`]'s own safety
/// contract; `fp`, if non-null, must be a valid pointer to a live
/// `UfuncT`.
unsafe fn funccal_unref(fc: *mut FunccallT, fp: *mut UfuncT, force: bool) {
    if fc.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*fc).fc_refcount -= 1 };
    // SAFETY: forwarded from this function's own safety doc.
    let should_free = if force {
        (unsafe { (*fc).fc_refcount }) <= 0
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        !fc_referenced(unsafe { &*fc })
    };
    if should_free {
        // Mirrors the original's `for (funccall_T **pfc = &previous_funccal;
        // *pfc != NULL; pfc = &(*pfc)->fc_caller)` pointer-to-pointer walk:
        // `link` always points at the `*mut FunccallT` SLOT that should be
        // redirected if its target turns out to be `fc` (either
        // `PREVIOUS_FUNCCAL` itself, or some node's own `fc_caller` field).
        // SAFETY: forwarded from this function's own safety doc.
        let mut link: *mut *mut FunccallT = unsafe { PREVIOUS_FUNCCAL.get_mut() as *mut *mut FunccallT };
        loop {
            // SAFETY: `link` always points at a valid `*mut FunccallT` slot.
            let node = unsafe { *link };
            if node.is_null() {
                break;
            }
            if node == fc {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { *link = (*fc).fc_caller };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { free_funccal_contents(fc) };
                return;
            }
            // SAFETY: forwarded from this function's own safety doc.
            link = unsafe { &mut (*node).fc_caller as *mut *mut FunccallT };
        }
    }
    // SAFETY: forwarded from this function's own safety doc.
    let count = unsafe { (*fc).fc_ufuncs.ga_len }.max(0) as usize;
    // SAFETY: forwarded from this function's own safety doc.
    let base = unsafe { (*fc).fc_ufuncs.ga_data.as_mut_ptr() as *mut *mut UfuncT };
    for i in 0..count {
        // SAFETY: `i < count == ga_len`, in-bounds.
        let slot = unsafe { base.add(i) };
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { *slot } == fp {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { *slot = std::ptr::null_mut() };
        }
    }
}

/// Register `fp` as using the current funccal as its scope, for
/// closures (`register_closure`).
///
/// # Safety
/// `fp` must be a valid, non-null pointer to a live `UfuncT` whose
/// `uf_scoped`, if non-null, satisfies [`funccal_unref`]'s own safety
/// contract. `CURRENT_FUNCCAL` must currently be non-null, with its
/// own `fc_ufuncs` already initialized via `GarrayT::ga_init` using
/// `itemsize == size_of::<*mut UfuncT>()` (matching `call_user_func`'s
/// own real setup - not yet translated - which always runs this
/// `ga_init` step before any code path that could reach
/// `register_closure`; `create_funccal` alone deliberately leaves
/// `fc_ufuncs` at its bare zero-initialized state, faithfully matching
/// the original's own `create_funccal`/`call_user_func` split).
#[allow(dead_code)] // no real translated caller yet (get_lambda_tv/closure-flagged user-function definitions, neither translated) - tested directly, matching this crate's established convention for private helpers harvested ahead of their real caller
unsafe fn register_closure(fp: *mut UfuncT) {
    // SAFETY: forwarded from this function's own safety doc.
    let current = unsafe { *CURRENT_FUNCCAL.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*fp).uf_scoped } == current {
        // no change
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let old_scoped = unsafe { (*fp).uf_scoped };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { funccal_unref(old_scoped, fp, false) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*fp).uf_scoped = current;
        (*current).fc_refcount += 1;
        (*current).fc_ufuncs.ga_grow(1);
        let base = (*current).fc_ufuncs.ga_data.as_mut_ptr() as *mut *mut UfuncT;
        let idx = (*current).fc_ufuncs.ga_len;
        *base.add(idx as usize) = fp;
        (*current).fc_ufuncs.ga_len += 1;
    }
}

/// Allocate a `FunccallT`, link it into `current_funccal`, and fill in
/// `fp`/`rettv` (`create_funccal`).
///
/// Must be followed by one call to [`remove_funccal`] or
/// `cleanup_function_call` (private - not reachable from outside this
/// module).
///
/// # Safety
/// `fp` must be a valid, non-null pointer to a live `UfuncT`.
#[must_use]
pub unsafe fn create_funccal(fp: *mut UfuncT, rettv: *mut TypvalT) -> *mut FunccallT {
    let fc = Box::into_raw(Box::new(FunccallT::default()));
    // SAFETY: `fc` was just allocated above, not yet reachable from
    // anywhere else.
    unsafe {
        (*fc).fc_caller = *CURRENT_FUNCCAL.get_mut();
        *CURRENT_FUNCCAL.get_mut() = fc;
        (*fc).fc_func = fp;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { func_ptr_ref(fp) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*fc).fc_rettv = rettv };
    fc
}

/// Restore `current_funccal` to its caller, freeing the funccall that
/// was current (`remove_funccal`).
///
/// # Safety
/// `crate::globals::GLOBALS`-independent: `CURRENT_FUNCCAL` must
/// currently be non-null (matching the original's own unchecked
/// dereference), and the current funccall must satisfy
/// `free_funccal`'s own safety contract.
pub unsafe fn remove_funccal() {
    // SAFETY: forwarded from this function's own safety doc.
    let fc = unsafe { *CURRENT_FUNCCAL.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *CURRENT_FUNCCAL.get_mut() = (*fc).fc_caller };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { free_funccal(fc) };
}

/// Returns `true` when a function was ended by a `":return"` command
/// (`current_func_returned`).
///
/// # Safety
/// `CURRENT_FUNCCAL` must currently be non-null (matching the
/// original's own unchecked dereference of `current_funccal`).
#[must_use]
pub unsafe fn current_func_returned() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*(*CURRENT_FUNCCAL.get_mut())).fc_returned != 0 }
}

/// Returns `true` if `fc` may be freed: nothing (`l:`, `a:`, `a:000`,
/// or `fc` itself) still references it as of `copy_id`
/// (`can_free_funccal`).
///
/// # Safety
/// `fc` must be a valid, non-null pointer to a live `FunccallT`.
#[must_use]
unsafe fn can_free_funccal(fc: *const FunccallT, copy_id: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let fc = unsafe { &*fc };
    fc.fc_l_varlist.lv_copy_id != copy_id
        && fc.fc_l_vars.dv_copy_id != copy_id
        && fc.fc_l_avars.dv_copy_id != copy_id
        && fc.fc_copy_id != copy_id
}

/// Free all `previous_funccal` entries no longer referenced as of
/// `copy_id` (`free_unref_funccal`). The `testing` parameter is the
/// original's own (unused by this crate's translation, since it only
/// gates a debug-build-only extra check not modeled here).
///
/// # Safety
/// Every entry reachable from `PREVIOUS_FUNCCAL`'s own `fc_caller`
/// chain must be a valid, live `FunccallT` satisfying
/// `free_funccal_contents`'s own safety contract.
#[must_use]
pub unsafe fn free_unref_funccal(copy_id: i32, _testing: i32) -> bool {
    let mut did_free = false;

    // Mirrors the original's `for (funccall_T **pfc = &previous_funccal;
    // *pfc != NULL;)` pointer-to-pointer walk - see funccal_unref's own
    // comment for why `link` is modeled this way.
    // SAFETY: forwarded from this function's own safety doc.
    let mut link: *mut *mut FunccallT = unsafe { PREVIOUS_FUNCCAL.get_mut() as *mut *mut FunccallT };
    loop {
        // SAFETY: `link` always points at a valid `*mut FunccallT` slot.
        let node = unsafe { *link };
        if node.is_null() {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { can_free_funccal(node, copy_id) } {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { *link = (*node).fc_caller };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { free_funccal_contents(node) };
            did_free = true;
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            link = unsafe { &mut (*node).fc_caller as *mut *mut FunccallT };
        }
    }

    did_free
}

/// Get function call environment based on backtrace debug level
/// (`get_funccal`).
///
/// # Safety
/// `CURRENT_FUNCCAL` must currently be non-null, and every entry
/// reachable from its own `fc_caller` chain must be a valid, live
/// `FunccallT` (matching the original's own unchecked dereference).
#[must_use]
pub unsafe fn get_funccal() -> *mut FunccallT {
    // SAFETY: forwarded from this function's own safety doc.
    let mut funccal = unsafe { *CURRENT_FUNCCAL.get_mut() };
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    if g.debug_backtrace_level > 0 {
        // NOT a Rust `for i in 0..g.debug_backtrace_level` range (which
        // would snapshot the upper bound once) - the original's C
        // `for` loop re-checks `i < debug_backtrace_level` on every
        // iteration, and the loop body itself can lower
        // `debug_backtrace_level` mid-walk (the "overflow" branch
        // below), which must shorten the walk immediately, not just on
        // some future call. A manual `while` reproduces that exactly.
        let mut i = 0;
        while i < g.debug_backtrace_level {
            // SAFETY: forwarded from this function's own safety doc.
            let temp_funccal = unsafe { (*funccal).fc_caller };
            if !temp_funccal.is_null() {
                funccal = temp_funccal;
            } else {
                // Backtrace level overflow - reset to max.
                g.debug_backtrace_level = i;
            }
            i += 1;
        }
    }
    funccal
}

/// @return dict used for local variables in the current funccal, or
/// null if there is no current funccal (`get_funccal_local_dict`).
#[must_use]
pub fn get_funccal_local_dict() -> *mut DictT {
    // SAFETY: only reads CURRENT_FUNCCAL/the (crate-invariant) live
    // fc_caller chain, exactly like get_funccal itself.
    let current = unsafe { *CURRENT_FUNCCAL.get_mut() };
    // SAFETY: current just checked non-null.
    if current.is_null() || unsafe { (*current).fc_l_vars.dv_refcount } == 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: CURRENT_FUNCCAL just checked non-null above, satisfying
    // get_funccal's own safety precondition.
    unsafe { &mut (*get_funccal()).fc_l_vars as *mut DictT }
}

/// @return the `l:` scope variable, or null if there is no current
/// funccal (`get_funccal_local_var`).
///
/// Returns a `*mut ScopeDictDictItem` rather than the original's
/// `dictitem_T*`: the original casts `&fc->fc_l_vars_var` (a
/// `scope_dictitem_T`, the exact same field layout as `dictitem_T`
/// with a fixed-size key buffer) to `dictitem_T*`, relying on that
/// shared layout - `ScopeDictDictItem`/`DictitemT` are distinct Rust
/// types here (see `ScopeDictDictItem`'s own doc comment), so no such
/// reinterpret-cast is attempted; callers needing a `*mut DictitemT`
/// specifically don't exist yet.
#[must_use]
pub fn get_funccal_local_var() -> *mut ScopeDictDictItem {
    // SAFETY: only reads CURRENT_FUNCCAL/the (crate-invariant) live
    // fc_caller chain, exactly like get_funccal itself.
    let current = unsafe { *CURRENT_FUNCCAL.get_mut() };
    // SAFETY: current just checked non-null.
    if current.is_null() || unsafe { (*current).fc_l_vars.dv_refcount } == 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: CURRENT_FUNCCAL just checked non-null above, satisfying
    // get_funccal's own safety precondition.
    unsafe { &mut (*get_funccal()).fc_l_vars_var as *mut ScopeDictDictItem }
}

/// @return the hashtable used for local variables in the current
/// funccal, or null if there is no current funccal
/// (`get_funccal_local_ht`).
#[must_use]
pub fn get_funccal_local_ht() -> *mut HashtabT {
    let d = get_funccal_local_dict();
    if d.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: get_funccal_local_dict only ever returns null or a
    // pointer to a live DictT's own fc_l_vars field.
    unsafe { &mut (*d).dv_hashtab as *mut HashtabT }
}

/// @return the dict used for arguments in the current funccal, or
/// null if there is no current funccal (`get_funccal_args_dict`).
#[must_use]
pub fn get_funccal_args_dict() -> *mut DictT {
    // SAFETY: only reads CURRENT_FUNCCAL/the (crate-invariant) live
    // fc_caller chain, exactly like get_funccal itself.
    let current = unsafe { *CURRENT_FUNCCAL.get_mut() };
    // SAFETY: current just checked non-null. Matches the original's
    // own (slightly surprising, but faithfully preserved) check
    // against fc_l_vars's refcount here too, not fc_l_avars's.
    if current.is_null() || unsafe { (*current).fc_l_vars.dv_refcount } == 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: CURRENT_FUNCCAL just checked non-null above, satisfying
    // get_funccal's own safety precondition.
    unsafe { &mut (*get_funccal()).fc_l_avars as *mut DictT }
}

/// @return the `a:` scope variable, or null if there is no current
/// funccal (`get_funccal_args_var`).
///
/// See [`get_funccal_local_var`]'s own doc comment for why this
/// returns `*mut ScopeDictDictItem` rather than the original's
/// `dictitem_T*`.
#[must_use]
pub fn get_funccal_args_var() -> *mut ScopeDictDictItem {
    // SAFETY: only reads CURRENT_FUNCCAL/the (crate-invariant) live
    // fc_caller chain, exactly like get_funccal itself.
    let current = unsafe { *CURRENT_FUNCCAL.get_mut() };
    // SAFETY: current just checked non-null. Matches the original's
    // own check against fc_l_vars's refcount here too, not
    // fc_l_avars's.
    if current.is_null() || unsafe { (*current).fc_l_vars.dv_refcount } == 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: CURRENT_FUNCCAL just checked non-null above, satisfying
    // get_funccal's own safety precondition.
    unsafe { &mut (*get_funccal()).fc_l_avars_var as *mut ScopeDictDictItem }
}

/// @return the hashtable used for arguments in the current funccal, or
/// null if there is no current funccal (`get_funccal_args_ht`).
#[must_use]
pub fn get_funccal_args_ht() -> *mut HashtabT {
    let d = get_funccal_args_dict();
    if d.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: get_funccal_args_dict only ever returns null or a
    // pointer to a live DictT's own fc_l_avars field.
    unsafe { &mut (*d).dv_hashtab as *mut HashtabT }
}

/// Add a number variable `name` to dict `dp` with value `nr`
/// (`add_nr_var`).
///
/// # Safety
/// `v` must be a valid, non-null pointer to a live `DictitemT`, not
/// already present in `dp`'s hashtable, outliving the resulting entry.
pub unsafe fn add_nr_var(dp: &mut DictT, v: *mut DictitemT, name: &[u8], nr: VarnumberT) {
    // SAFETY: forwarded from this function's own safety doc.
    let v_ref = unsafe { &mut *v };
    v_ref.di_key = name.to_vec();
    v_ref.di_key.push(0);
    v_ref.di_flags = dict_item_flags::RO | dict_item_flags::FIX;
    v_ref.di_tv = TypvalT { v_lock: VarLockStatus::Fixed, value: TypvalValue::Number(nr) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::typval::tv_dict_add(dp, v) };
}

/// @return the name of the executed function for a funccall cookie
/// (`func_name`).
///
/// # Safety
/// `cookie` must be a valid, non-null pointer to a live `FunccallT`
/// whose own `fc_func`, if non-null, points at a live `UfuncT`.
#[must_use]
pub unsafe fn func_name(cookie: *const FunccallT) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*(*cookie).fc_func }.uf_name.clone()
}

/// @return the address holding the next breakpoint line for a
/// funccall cookie (`func_breakpoint`).
///
/// # Safety
/// `cookie` must be a valid, non-null pointer to a live `FunccallT`.
#[must_use]
pub unsafe fn func_breakpoint(cookie: *mut FunccallT) -> *mut crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut (*cookie).fc_breakpoint as *mut _ }
}

/// @return the address holding the debug tick for a funccall cookie
/// (`func_dbg_tick`).
///
/// # Safety
/// `cookie` must be a valid, non-null pointer to a live `FunccallT`.
#[must_use]
pub unsafe fn func_dbg_tick(cookie: *mut FunccallT) -> *mut i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut (*cookie).fc_dbg_tick as *mut _ }
}

/// @return the nesting level for a funccall cookie (`func_level`).
///
/// # Safety
/// `cookie` must be a valid, non-null pointer to a live `FunccallT`.
#[must_use]
pub unsafe fn func_level(cookie: *const FunccallT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*cookie }.fc_level
}

/// @return true if `cookie` indicates a function which `"abort"`s on
/// errors (`func_has_abort`).
///
/// # Safety
/// `cookie` must be a valid, non-null pointer to a live `FunccallT`
/// whose own `fc_func`, if non-null, points at a live `UfuncT`.
#[must_use]
pub unsafe fn func_has_abort(cookie: *const FunccallT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*(*cookie).fc_func }.uf_flags & fc_flags::ABORT != 0
}

/// @return true if the currently active function should be ended,
/// because a return was encountered or an error occurred. Used inside
/// a `":while"` (`func_has_ended`).
///
/// # Safety
/// Same as [`func_has_abort`].
#[must_use]
pub unsafe fn func_has_ended(cookie: *const FunccallT) -> bool {
    // Ignore the "abort" flag if the abortion behavior has been
    // changed due to an error inside a try conditional.
    // SAFETY: forwarded from this function's own safety doc.
    let has_abort = unsafe { func_has_abort(cookie) };
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    (has_abort && g.did_emsg != 0 && !crate::ex_eval::aborted_in_try())
        // SAFETY: forwarded from this function's own safety doc.
        || unsafe { &*cookie }.fc_returned != 0
}

/// Free all things that a function contains. Does not free the
/// function itself; use [`func_free`] for that (`func_clear_items`).
///
/// # Safety
/// `fp` must be a valid, non-null pointer to a live `UfuncT`.
unsafe fn func_clear_items(fp: *mut UfuncT) {
    // SAFETY: forwarded from this function's own safety doc.
    let fp_ref = unsafe { &mut *fp };
    // The original's `ga_clear_strings`/`GA_DEEP_CLEAR_PTR` also
    // xfree()s each individual string entry - a no-op here, since
    // nothing in this crate currently populates uf_args/uf_def_args/
    // uf_lines with real heap-owned entries (no function-definition
    // parser exists yet); `.ga_clear()` alone is exactly what
    // ga_clear_strings degrades to when there's nothing to free.
    fp_ref.uf_args.ga_clear();
    fp_ref.uf_def_args.ga_clear();
    fp_ref.uf_lines.ga_clear();

    if fp_ref.uf_flags & fc_flags::LUAREF != 0 {
        unimplemented!(
            "func_clear_items: api_free_luaref needs the Lua host (phase 13), not yet \
             translated - unreachable today since nothing can set FC_LUAREF yet"
        );
    }

    fp_ref.uf_tml_count.clear();
    fp_ref.uf_tml_total.clear();
    fp_ref.uf_tml_self.clear();
}

/// Free all things that a function contains. Does not free the
/// function itself; use [`func_free`] for that (`func_clear`).
///
/// # Safety
/// `fp` must be a valid, non-null pointer to a live `UfuncT` whose
/// `uf_scoped`, if non-null, satisfies [`funccal_unref`]'s own safety
/// contract.
unsafe fn func_clear(fp: *mut UfuncT, force: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*fp).uf_cleared } {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*fp).uf_cleared = true };

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { func_clear_items(fp) };
    // SAFETY: forwarded from this function's own safety doc.
    let scoped = unsafe { (*fp).uf_scoped };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { funccal_unref(scoped, fp, force) };
}

/// Free a function and remove it from the list of functions. Does not
/// free what a function contains; call [`func_clear`] first
/// (`func_free`).
///
/// # Safety
/// `fp` must be a valid, non-null pointer previously allocated via
/// `Box::into_raw`.
unsafe fn func_free(fp: *mut UfuncT) {
    // Only remove it when not done already, otherwise we would remove
    // a newer version of the function.
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*fp).uf_flags } & (fc_flags::DELETED | fc_flags::REMOVED) == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { func_remove(fp) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*fp).uf_name_exp = None };
    // SAFETY: forwarded from this function's own safety doc (`fp` was
    // allocated via `Box::into_raw`).
    drop(unsafe { Box::from_raw(fp) });
}

/// Free all things that a function contains and free the function
/// itself (`func_clear_free`).
///
/// # Safety
/// `fp` must be a valid, non-null pointer previously allocated via
/// `Box::into_raw`, satisfying [`func_clear`]'s own safety contract.
unsafe fn func_clear_free(fp: *mut UfuncT, force: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { func_clear(fp, force) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { func_free(fp) };
}

/// Count a reference to a function (`func_ptr_ref`).
///
/// # Safety
/// `fp`, if non-null, must be a valid pointer to a live `UfuncT`.
pub unsafe fn func_ptr_ref(fp: *mut UfuncT) {
    if !fp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*fp).uf_refcount += 1 };
    }
}

/// Unreference a function: decrement the reference count and free it
/// when it becomes zero (and the function isn't mid-call)
/// (`func_ptr_unref`).
///
/// # Safety
/// `fp`, if non-null, must be a valid pointer to a live `UfuncT`. If
/// the reference count hits zero and `uf_calls == 0`, `fp` (and,
/// transitively, `(*fp).uf_scoped` if non-null) must satisfy
/// `func_clear_free`'s own safety contract - in particular, `fp`
/// must have been allocated via `Box::into_raw`.
pub unsafe fn func_ptr_unref(fp: *mut UfuncT) {
    if fp.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*fp).uf_refcount -= 1 };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*fp).uf_refcount } <= 0 {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { (*fp).uf_calls } == 0 {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { func_clear_free(fp, false) };
        }
        // Otherwise: still being called (`uf_calls != 0`) - freed
        // later when `uf_calls` becomes zero, matching the original's
        // own comment exactly. Nothing more to do here.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vim_defs::FAIL;

    #[test]
    fn func_ptr_ref_null_is_noop() {
        unsafe { func_ptr_ref(std::ptr::null_mut()) };
    }

    #[test]
    fn func_ptr_ref_increments_refcount() {
        let mut fp = UfuncT { uf_refcount: 1, ..Default::default() };
        unsafe { func_ptr_ref(&mut fp as *mut UfuncT) };
        assert_eq!(fp.uf_refcount, 2);
    }

    #[test]
    fn func_ptr_unref_null_is_noop() {
        unsafe { func_ptr_unref(std::ptr::null_mut()) };
    }

    #[test]
    fn func_ptr_unref_decrements_without_freeing_when_still_referenced() {
        let mut fp = UfuncT { uf_refcount: 2, ..Default::default() };
        unsafe { func_ptr_unref(&mut fp as *mut UfuncT) };
        assert_eq!(fp.uf_refcount, 1);
    }

    #[test]
    fn func_ptr_unref_noop_when_hits_zero_but_still_being_called() {
        let mut fp = UfuncT { uf_refcount: 1, uf_calls: 3, ..Default::default() };
        // Refcount hits 0, but uf_calls != 0 - matches the original's
        // "freed later when uf_calls becomes zero" comment, so this
        // must NOT hit the unimplemented!() branch.
        unsafe { func_ptr_unref(&mut fp as *mut UfuncT) };
        assert_eq!(fp.uf_refcount, 0);
        assert_eq!(fp.uf_calls, 3);
    }

    #[test]
    fn func_ptr_unref_frees_when_hits_zero_and_not_being_called() {
        // uf_scoped defaults to null, so funccal_unref is a no-op;
        // func_free's func_remove call still touches FUNC_HASHTAB
        // (uf_name is empty here, a harmless miss), so this needs the
        // shared lock + a clean table, matching every other test that
        // exercises func_free's real body.
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let fp = Box::into_raw(Box::new(UfuncT { uf_refcount: 1, uf_calls: 0, ..Default::default() }));
        // Refcount hits 0, not mid-call - the real func_clear_free
        // chain runs to completion and frees fp. Nothing further to
        // assert on fp after this - the absence of a crash/leak-
        // sanitizer complaint is the check (matching this crate's own
        // partial_unref_frees_and_clears_argv_at_zero_refcount
        // precedent).
        unsafe { func_ptr_unref(fp) };
    }

    #[test]
    fn fc_referenced_true_for_a_freshly_zeroed_funccall() {
        // A plain Default::default() FunccallT matches xcalloc, BEFORE
        // create_funccal's own init_var_dict calls would mark the
        // by-value scopes DO_NOT_FREE_CNT - so dv_refcount/lv_refcount
        // are plain 0 here, which already differs from
        // DO_NOT_FREE_CNT, making fc_referenced report true
        // unconditionally in this state (faithfully - the original
        // would too, for the same raw xcalloc'd struct).
        let fc = FunccallT::default();
        assert!(fc_referenced(&fc));
    }

    #[test]
    fn fc_referenced_false_when_scopes_are_do_not_free_and_refcount_is_zero() {
        let mut fc = FunccallT::default();
        fc.fc_l_vars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        fc.fc_l_avars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        fc.fc_l_varlist.lv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        fc.fc_refcount = 0;
        assert!(!fc_referenced(&fc));
    }

    #[test]
    fn fc_referenced_true_when_fc_refcount_positive_even_with_do_not_free_scopes() {
        let mut fc = FunccallT::default();
        fc.fc_l_vars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        fc.fc_l_avars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        fc.fc_l_varlist.lv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        fc.fc_refcount = 3;
        assert!(fc_referenced(&fc));
    }

    /// Builds a `FunccallT` whose `fc_ufuncs` contains exactly one
    /// entry (`fp_ptr`), matching the pointer-array convention
    /// `free_funccal`/`funccal_unref` expect.
    fn funccall_with_one_ufunc_entry(fp_ptr: *mut UfuncT) -> FunccallT {
        let mut fc = FunccallT::default();
        fc.fc_ufuncs.ga_itemsize = std::mem::size_of::<*mut UfuncT>() as i32;
        unsafe { fc.fc_ufuncs.ga_append_item::<*mut UfuncT>(fp_ptr) };
        fc
    }

    #[test]
    fn funccal_unref_null_fc_is_noop() {
        let mut fp = UfuncT::default();
        unsafe { funccal_unref(std::ptr::null_mut(), &mut fp as *mut UfuncT, false) };
    }

    #[test]
    fn funccal_unref_nulls_matching_fc_ufuncs_entry_when_still_referenced() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fp = UfuncT::default();
        let fp_ptr = &mut fp as *mut UfuncT;
        let mut fc = funccall_with_one_ufunc_entry(fp_ptr);
        fc.fc_refcount = 5; // decrements to 4, still > 0 - fc_referenced() is true
        let fc_ptr = &mut fc as *mut FunccallT;
        unsafe { funccal_unref(fc_ptr, fp_ptr, false) };
        assert_eq!(fc.fc_refcount, 4);
        let base = fc.fc_ufuncs.ga_data.as_ptr() as *const *mut UfuncT;
        assert!(unsafe { *base }.is_null());
    }

    #[test]
    fn funccal_unref_frees_and_unlinks_head_of_previous_funccal() {
        let _lock = crate::globals::global_state_test_lock();
        // Reset PREVIOUS_FUNCCAL to a known-empty state - it's a
        // shared GlobalCell like FUNC_HASHTAB, just with no func_init()
        // -style public reset helper (nothing outside this module
        // needs one yet).
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };

        let mut fc = Box::new(FunccallT {
            fc_refcount: 1, // decrements to 0
            ..Default::default()
        });
        fc.fc_l_vars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        fc.fc_l_avars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        fc.fc_l_varlist.lv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        // Register fc as the current previous_funccal head - mirrors
        // what a real (not yet translated) remove_funccal() caller
        // would have done before funccal_unref ever runs.
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = fc_ptr };
        // funccal_unref takes ownership (frees fc via Box::from_raw
        // internally) - forget the Box here so Rust's own Drop
        // doesn't also try to free it (would double-free).
        std::mem::forget(fc);

        unsafe { funccal_unref(fc_ptr, std::ptr::null_mut(), false) };

        // fc_caller was null, so unlinking fc from the list-of-one
        // leaves previous_funccal null again.
        assert!(unsafe { *PREVIOUS_FUNCCAL.get_mut() }.is_null());
    }

    #[test]
    fn funccal_unref_frees_and_unlinks_a_non_head_previous_funccal_entry() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };

        // Build a 2-entry previous_funccal list: head -> target -> null.
        let mut target = Box::new(FunccallT { fc_refcount: 1, ..Default::default() });
        target.fc_l_vars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        target.fc_l_avars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        target.fc_l_varlist.lv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        let target_ptr = target.as_mut() as *mut FunccallT;
        std::mem::forget(target);

        let mut head = Box::new(FunccallT { fc_caller: target_ptr, ..Default::default() });
        let head_ptr = head.as_mut() as *mut FunccallT;
        // head itself must stay alive/inspectable after this test, so
        // it is NOT forgotten - it was never handed to funccal_unref,
        // only referenced via its own fc_caller field.

        unsafe { *PREVIOUS_FUNCCAL.get_mut() = head_ptr };

        unsafe { funccal_unref(target_ptr, std::ptr::null_mut(), false) };

        // previous_funccal's head is unchanged; head's own fc_caller
        // now skips over the freed target (was null, since target's
        // own fc_caller was never set).
        assert_eq!(unsafe { *PREVIOUS_FUNCCAL.get_mut() }, head_ptr);
        assert!(unsafe { (*head_ptr).fc_caller }.is_null());

        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    // ---- register_closure ------------------------------------------------

    #[test]
    fn register_closure_is_a_noop_when_already_scoped_to_current_funccal() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        let mut fc = Box::new(FunccallT { fc_refcount: 1, ..Default::default() });
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        unsafe { *CURRENT_FUNCCAL.get_mut() = fc_ptr };

        let mut fp = UfuncT { uf_scoped: fc_ptr, ..Default::default() };

        unsafe { register_closure(&mut fp as *mut UfuncT) };

        // No change: fc_refcount/fc_ufuncs untouched.
        assert_eq!(unsafe { (*fc_ptr).fc_refcount }, 1);
        assert_eq!(unsafe { (*fc_ptr).fc_ufuncs.ga_len }, 0);

        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    #[test]
    fn register_closure_links_fp_into_current_funccal_and_bumps_refcount() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        let mut fc = Box::new(FunccallT { fc_refcount: 1, ..Default::default() });
        // Matches call_user_func's own real setup (not yet translated)
        // - see register_closure's own safety doc for why this is a
        // real, stated precondition here.
        fc.fc_ufuncs.ga_init(std::mem::size_of::<*mut UfuncT>() as i32, 1);
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        unsafe { *CURRENT_FUNCCAL.get_mut() = fc_ptr };

        let mut fp = UfuncT::default(); // uf_scoped starts null

        unsafe { register_closure(&mut fp as *mut UfuncT) };

        assert_eq!(fp.uf_scoped, fc_ptr);
        assert_eq!(unsafe { (*fc_ptr).fc_refcount }, 2);
        assert_eq!(unsafe { (*fc_ptr).fc_ufuncs.ga_len }, 1);
        let base = unsafe { (*fc_ptr).fc_ufuncs.ga_data.as_ptr() as *const *mut UfuncT };
        assert_eq!(unsafe { *base }, &mut fp as *mut UfuncT);

        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    #[test]
    fn register_closure_unrefs_the_previous_scope_before_switching() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };

        // old_fc: a distinct, already-current-elsewhere funccal that fp
        // used to be scoped to; refcount 1 so funccal_unref's own
        // "hits zero, not referenced, and present on previous_funccal"
        // path frees it outright once register_closure unrefs it.
        let mut old_fc = Box::new(FunccallT { fc_refcount: 1, ..Default::default() });
        old_fc.fc_l_vars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        old_fc.fc_l_avars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        old_fc.fc_l_varlist.lv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        let old_fc_ptr = old_fc.as_mut() as *mut FunccallT;
        std::mem::forget(old_fc); // funccal_unref frees it below.
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = old_fc_ptr };

        let mut new_fc = Box::new(FunccallT { fc_refcount: 1, ..Default::default() });
        new_fc.fc_ufuncs.ga_init(std::mem::size_of::<*mut UfuncT>() as i32, 1);
        let new_fc_ptr = new_fc.as_mut() as *mut FunccallT;
        unsafe { *CURRENT_FUNCCAL.get_mut() = new_fc_ptr };

        let mut fp = UfuncT { uf_scoped: old_fc_ptr, ..Default::default() };

        unsafe { register_closure(&mut fp as *mut UfuncT) };

        // old_fc was unreferenced (refcount hit 0, unreferenced) and
        // freed, unlinking it from previous_funccal.
        assert!(unsafe { *PREVIOUS_FUNCCAL.get_mut() }.is_null());
        // fp is now scoped to new_fc instead.
        assert_eq!(fp.uf_scoped, new_fc_ptr);
        assert_eq!(unsafe { (*new_fc_ptr).fc_refcount }, 2);

        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    // ---- cleanup_function_call ------------------------------------------

    #[test]
    fn cleanup_function_call_frees_fc_when_fully_unreferenced() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        let mut caller = Box::new(FunccallT::default());
        let caller_ptr = caller.as_mut() as *mut FunccallT;

        let mut fc = Box::new(FunccallT { fc_refcount: 0, fc_caller: caller_ptr, ..Default::default() });
        fc.fc_l_vars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        fc.fc_l_avars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        fc.fc_l_varlist.lv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        std::mem::forget(fc); // cleanup_function_call frees it below.

        unsafe { cleanup_function_call(fc_ptr) };

        // current_funccal restored to fc's own (real) caller - not left
        // pointing at the now-freed fc.
        assert_eq!(unsafe { *CURRENT_FUNCCAL.get_mut() }, caller_ptr);
        // fc was fully freeable, so it must NOT have been linked onto
        // previous_funccal.
        assert!(unsafe { *PREVIOUS_FUNCCAL.get_mut() }.is_null());

        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    #[test]
    fn cleanup_function_call_keeps_fc_alive_when_still_referenced() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };

        // fc_refcount > 0 - may_free_fc is false, so every branch below
        // takes its "still in use" path regardless of the individual
        // dv_refcount/lv_refcount values.
        let fc = Box::into_raw(Box::new(FunccallT { fc_refcount: 1, ..Default::default() }));

        unsafe { cleanup_function_call(fc) };

        // Linked onto previous_funccal for later GC, not freed.
        assert_eq!(unsafe { *PREVIOUS_FUNCCAL.get_mut() }, fc);
        assert!(unsafe { (*fc).fc_caller }.is_null()); // was the only entry

        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { drop(Box::from_raw(fc)) };
    }

    #[test]
    fn cleanup_function_call_bumps_an_escaping_avars_lists_own_refcount() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };

        let list = crate::eval::typval::tv_list_alloc(0);
        unsafe { crate::eval::typval::tv_list_ref(list) };
        assert_eq!(unsafe { (*list).lv_refcount }, 1);

        let mut fc = Box::new(FunccallT { fc_refcount: 1, ..Default::default() });
        let item = crate::eval::typval::tv_dict_item_alloc(b"1");
        unsafe { (*item).di_tv.value = TypvalValue::List(list) };
        unsafe { crate::eval::typval::tv_dict_add(&mut fc.fc_l_avars, item) };
        let fc_ptr = Box::into_raw(fc);

        unsafe { cleanup_function_call(fc_ptr) };

        // The escaping a: dict's own List value had its reference count
        // bumped by tv_copy - it is no longer solely owned by fc alone.
        assert_eq!(unsafe { (*list).lv_refcount }, 2);

        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { crate::eval::typval::tv_list_unref(list) };
        unsafe { crate::eval::typval::tv_list_unref(list) };
        unsafe { drop(Box::from_raw(fc_ptr)) };
    }

    #[test]
    fn cleanup_function_call_bumps_an_escaping_varlist_items_own_list_refcount() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };

        let inner_list = crate::eval::typval::tv_list_alloc(0);
        unsafe { crate::eval::typval::tv_list_ref(inner_list) };
        assert_eq!(unsafe { (*inner_list).lv_refcount }, 1);

        let mut fc = Box::new(FunccallT { fc_refcount: 1, ..Default::default() });
        let mut li = Box::new(crate::eval::typval_defs::ListitemT {
            li_next: std::ptr::null_mut(),
            li_prev: std::ptr::null_mut(),
            li_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(inner_list) },
        });
        let li_ptr = li.as_mut() as *mut _;
        std::mem::forget(li);
        fc.fc_l_varlist.lv_first = li_ptr;
        fc.fc_l_varlist.lv_last = li_ptr;
        fc.fc_l_varlist.lv_len = 1;
        let fc_ptr = Box::into_raw(fc);

        unsafe { cleanup_function_call(fc_ptr) };

        assert_eq!(unsafe { (*inner_list).lv_refcount }, 2);

        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { crate::eval::typval::tv_list_unref(inner_list) };
        unsafe { crate::eval::typval::tv_list_unref(inner_list) };
        unsafe { drop(Box::from_raw(li_ptr)) };
        unsafe { drop(Box::from_raw(fc_ptr)) };
    }

    #[test]
    fn cleanup_function_call_triggers_garbage_collect_once_made_copy_threshold_hit() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { crate::globals::GLOBALS.get_mut() }.want_garbage_collect = false;

        let threshold = (4096 * 1024) / std::mem::size_of::<FunccallT>() as i32;
        *unsafe { CLEANUP_FUNCTION_CALL_MADE_COPY.get_mut() } = threshold - 1;

        let fc = Box::into_raw(Box::new(FunccallT { fc_refcount: 1, ..Default::default() }));
        unsafe { cleanup_function_call(fc) };

        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.want_garbage_collect);
        assert_eq!(*unsafe { CLEANUP_FUNCTION_CALL_MADE_COPY.get_mut() }, 0);

        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { crate::globals::GLOBALS.get_mut() }.want_garbage_collect = false;
        unsafe { drop(Box::from_raw(fc)) };
    }

    #[test]
    fn cleanup_function_call_resets_made_copy_when_garbage_collect_already_wanted() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { crate::globals::GLOBALS.get_mut() }.want_garbage_collect = true;
        *unsafe { CLEANUP_FUNCTION_CALL_MADE_COPY.get_mut() } = 5;

        let fc = Box::into_raw(Box::new(FunccallT { fc_refcount: 1, ..Default::default() }));
        unsafe { cleanup_function_call(fc) };

        assert_eq!(*unsafe { CLEANUP_FUNCTION_CALL_MADE_COPY.get_mut() }, 0);

        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { crate::globals::GLOBALS.get_mut() }.want_garbage_collect = false;
        unsafe { drop(Box::from_raw(fc)) };
    }

    // ---- create_funccal / remove_funccal / current_func_returned -------

    #[test]
    fn create_funccal_links_into_current_funccal_and_refs_fp() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        let mut fp = UfuncT { uf_refcount: 1, ..Default::default() };
        let mut rettv = TypvalT::default();

        let fc = unsafe { create_funccal(&mut fp as *mut UfuncT, &mut rettv as *mut TypvalT) };

        assert_eq!(unsafe { *CURRENT_FUNCCAL.get_mut() }, fc);
        assert!(unsafe { (*fc).fc_caller }.is_null()); // was the first/only funccall
        assert_eq!(unsafe { (*fc).fc_func }, &mut fp as *mut UfuncT);
        assert_eq!(unsafe { (*fc).fc_rettv }, &mut rettv as *mut TypvalT);
        assert_eq!(fp.uf_refcount, 2); // func_ptr_ref incremented it

        // Clean up: mirrors remove_funccal's own body without calling
        // free_funccal (which would try to free the stack-allocated fp
        // via func_ptr_unref - fp here isn't heap-allocated).
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        fp.uf_refcount -= 1;
        unsafe { drop(Box::from_raw(fc)) };
    }

    #[test]
    fn remove_funccal_restores_caller_and_frees() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        let fp = Box::into_raw(Box::new(UfuncT { uf_refcount: 1, ..Default::default() }));
        let mut rettv = TypvalT::default();

        let outer = unsafe { create_funccal(fp, &mut rettv as *mut TypvalT) };
        let inner = unsafe { create_funccal(fp, &mut rettv as *mut TypvalT) };
        assert_eq!(unsafe { *CURRENT_FUNCCAL.get_mut() }, inner);
        assert_eq!(unsafe { (*inner).fc_caller }, outer);

        unsafe { remove_funccal() }; // frees inner, restores outer as current

        assert_eq!(unsafe { *CURRENT_FUNCCAL.get_mut() }, outer);

        unsafe { remove_funccal() }; // frees outer
        assert!(unsafe { *CURRENT_FUNCCAL.get_mut() }.is_null());

        // fp's refcount round-tripped back to 1 (2 refs added, 2
        // released) - free it directly since func_ptr_unref would need
        // FUNC_HASHTAB wiring this test doesn't set up.
        unsafe { assert_eq!((*fp).uf_refcount, 1) };
        unsafe { drop(Box::from_raw(fp)) };
    }

    #[test]
    fn current_func_returned_reflects_fc_returned() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        let mut fp = UfuncT { uf_refcount: 1, ..Default::default() };
        let mut rettv = TypvalT::default();
        let fc = unsafe { create_funccal(&mut fp as *mut UfuncT, &mut rettv as *mut TypvalT) };

        assert!(!unsafe { current_func_returned() });
        unsafe { (*fc).fc_returned = 1 };
        assert!(unsafe { current_func_returned() });

        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        fp.uf_refcount -= 1;
        unsafe { drop(Box::from_raw(fc)) };
    }

    // ---- get_funccal / get_funccal_local_*/get_funccal_args_* / add_nr_var

    #[test]
    fn get_funccal_returns_current_funccal_when_backtrace_level_zero() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { crate::globals::GLOBALS.get_mut() }.debug_backtrace_level = 0;
        let mut fc = Box::new(FunccallT::default());
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        unsafe { *CURRENT_FUNCCAL.get_mut() = fc_ptr };

        assert_eq!(unsafe { get_funccal() }, fc_ptr);

        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    #[test]
    fn get_funccal_walks_fc_caller_chain_by_backtrace_level() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };

        let mut outer = Box::new(FunccallT::default());
        let outer_ptr = outer.as_mut() as *mut FunccallT;
        let mut inner = Box::new(FunccallT { fc_caller: outer_ptr, ..Default::default() });
        let inner_ptr = inner.as_mut() as *mut FunccallT;
        unsafe { *CURRENT_FUNCCAL.get_mut() = inner_ptr };

        unsafe { crate::globals::GLOBALS.get_mut() }.debug_backtrace_level = 1;
        assert_eq!(unsafe { get_funccal() }, outer_ptr);

        // Requesting a level deeper than the chain resets
        // debug_backtrace_level to how far it actually got (matching
        // the original's own "backtrace level overflow" comment).
        unsafe { crate::globals::GLOBALS.get_mut() }.debug_backtrace_level = 5;
        assert_eq!(unsafe { get_funccal() }, outer_ptr);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.debug_backtrace_level, 1);

        unsafe { crate::globals::GLOBALS.get_mut() }.debug_backtrace_level = 0;
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    #[test]
    fn get_funccal_local_dict_and_ht_null_without_a_current_funccal() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        assert!(get_funccal_local_dict().is_null());
        assert!(get_funccal_local_ht().is_null());
        assert!(get_funccal_local_var().is_null());
    }

    #[test]
    fn get_funccal_local_dict_and_ht_null_when_fc_l_vars_unreferenced() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fc = Box::new(FunccallT::default()); // fc_l_vars.dv_refcount == 0
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        unsafe { *CURRENT_FUNCCAL.get_mut() = fc_ptr };

        assert!(get_funccal_local_dict().is_null());
        assert!(get_funccal_local_ht().is_null());

        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    #[test]
    fn get_funccal_local_dict_and_ht_and_var_point_at_the_real_fields() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fc = Box::new(FunccallT::default());
        fc.fc_l_vars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        unsafe { *CURRENT_FUNCCAL.get_mut() = fc_ptr };

        let d = get_funccal_local_dict();
        assert_eq!(d, unsafe { &mut (*fc_ptr).fc_l_vars as *mut DictT });

        let ht = get_funccal_local_ht();
        assert_eq!(ht, unsafe { &mut (*d).dv_hashtab as *mut HashtabT });

        let v = get_funccal_local_var();
        assert_eq!(v, unsafe { &mut (*fc_ptr).fc_l_vars_var as *mut ScopeDictDictItem });

        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    #[test]
    fn get_funccal_args_dict_and_ht_and_var_null_without_a_current_funccal() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        assert!(get_funccal_args_dict().is_null());
        assert!(get_funccal_args_ht().is_null());
        assert!(get_funccal_args_var().is_null());
    }

    #[test]
    fn get_funccal_args_dict_and_ht_and_var_point_at_the_real_fields() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fc = Box::new(FunccallT::default());
        // The original gates get_funccal_args_dict/_ht/_var on
        // fc_l_vars's own refcount, not fc_l_avars's - preserved here.
        fc.fc_l_vars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        unsafe { *CURRENT_FUNCCAL.get_mut() = fc_ptr };

        let d = get_funccal_args_dict();
        assert_eq!(d, unsafe { &mut (*fc_ptr).fc_l_avars as *mut DictT });

        let ht = get_funccal_args_ht();
        assert_eq!(ht, unsafe { &mut (*d).dv_hashtab as *mut HashtabT });

        let v = get_funccal_args_var();
        assert_eq!(v, unsafe { &mut (*fc_ptr).fc_l_avars_var as *mut ScopeDictDictItem });

        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    #[test]
    fn add_nr_var_sets_key_flags_value_and_registers_in_the_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let dict_ptr = crate::eval::typval::tv_dict_alloc();
        let item_ptr = crate::eval::typval::tv_dict_item_alloc(b"0");
        unsafe { add_nr_var(&mut *dict_ptr, item_ptr, b"0", 42) };

        assert_eq!(unsafe { &(*item_ptr).di_key }, b"0\0");
        assert_eq!(
            unsafe { (*item_ptr).di_flags },
            dict_item_flags::RO | dict_item_flags::FIX
        );
        assert_eq!(unsafe { (*item_ptr).di_tv.v_lock }, VarLockStatus::Fixed);
        assert!(matches!(unsafe { (*item_ptr).di_tv.value.clone() }, TypvalValue::Number(42)));

        let found = crate::eval::typval::tv_dict_find(Some(unsafe { &mut *dict_ptr }), b"0");
        assert_eq!(found, Some(item_ptr));

        unsafe { crate::eval::typval::tv_dict_free(dict_ptr) };
    }

    // ---- can_free_funccal / free_unref_funccal --------------------------

    #[test]
    fn can_free_funccal_true_when_no_scope_matches_copy_id() {
        let fc = FunccallT { fc_copy_id: 1, ..Default::default() };
        assert!(unsafe { can_free_funccal(&fc as *const FunccallT, 5) });
    }

    #[test]
    fn can_free_funccal_false_when_any_scope_matches_copy_id() {
        let mut fc = FunccallT { fc_copy_id: 5, ..Default::default() };
        assert!(!unsafe { can_free_funccal(&fc as *const FunccallT, 5) });

        fc.fc_copy_id = 1;
        fc.fc_l_vars.dv_copy_id = 5;
        assert!(!unsafe { can_free_funccal(&fc as *const FunccallT, 5) });

        fc.fc_l_vars.dv_copy_id = 1;
        fc.fc_l_avars.dv_copy_id = 5;
        assert!(!unsafe { can_free_funccal(&fc as *const FunccallT, 5) });

        fc.fc_l_avars.dv_copy_id = 1;
        fc.fc_l_varlist.lv_copy_id = 5;
        assert!(!unsafe { can_free_funccal(&fc as *const FunccallT, 5) });
    }

    #[test]
    fn free_unref_funccal_frees_eligible_and_keeps_ineligible_entries() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };

        // freeable: copy_id doesn't match anywhere.
        let mut freeable = Box::new(FunccallT { fc_copy_id: 1, ..Default::default() });
        freeable.fc_l_vars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        freeable.fc_l_avars.dv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        freeable.fc_l_varlist.lv_refcount = crate::eval::typval_defs::DO_NOT_FREE_CNT;
        let freeable_ptr = freeable.as_mut() as *mut FunccallT;
        std::mem::forget(freeable); // ownership passes to free_unref_funccal

        // kept: fc_copy_id matches, so can_free_funccal is false.
        let mut kept = Box::new(FunccallT { fc_copy_id: 5, fc_caller: freeable_ptr, ..Default::default() });
        let kept_ptr = kept.as_mut() as *mut FunccallT;
        // kept stays alive/inspectable, so it is NOT forgotten.

        unsafe { *PREVIOUS_FUNCCAL.get_mut() = kept_ptr };

        let did_free = unsafe { free_unref_funccal(5, 0) };

        assert!(did_free);
        assert_eq!(unsafe { *PREVIOUS_FUNCCAL.get_mut() }, kept_ptr); // head kept
        assert!(unsafe { (*kept_ptr).fc_caller }.is_null()); // freeable unlinked

        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    #[test]
    fn free_unref_funccal_false_when_nothing_freed() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *PREVIOUS_FUNCCAL.get_mut() = std::ptr::null_mut() };
        assert!(!unsafe { free_unref_funccal(1, 0) });
    }

    // ---- funccall-cookie debugger-hook accessors -------------------------

    #[test]
    fn func_name_reads_fc_func_uf_name() {
        let mut fp = UfuncT { uf_name: b"MyFunc".to_vec(), ..Default::default() };
        let fc = FunccallT { fc_func: &mut fp as *mut UfuncT, ..Default::default() };
        assert_eq!(unsafe { func_name(&fc as *const FunccallT) }, b"MyFunc".to_vec());
    }

    #[test]
    fn func_breakpoint_dbg_tick_and_level_read_and_write_through() {
        let mut fc = FunccallT { fc_breakpoint: 7, fc_dbg_tick: 3, fc_level: 2, ..Default::default() };
        unsafe {
            assert_eq!(*func_breakpoint(&mut fc as *mut FunccallT), 7);
            assert_eq!(*func_dbg_tick(&mut fc as *mut FunccallT), 3);
            assert_eq!(func_level(&fc as *const FunccallT), 2);

            *func_breakpoint(&mut fc as *mut FunccallT) = 42;
            *func_dbg_tick(&mut fc as *mut FunccallT) = 99;
        }
        assert_eq!(fc.fc_breakpoint, 42);
        assert_eq!(fc.fc_dbg_tick, 99);
    }

    #[test]
    fn func_has_abort_reflects_uf_flags() {
        let mut fp = UfuncT { uf_flags: fc_flags::ABORT, ..Default::default() };
        let fc = FunccallT { fc_func: &mut fp as *mut UfuncT, ..Default::default() };
        assert!(unsafe { func_has_abort(&fc as *const FunccallT) });

        fp.uf_flags = fc_flags::RANGE;
        assert!(!unsafe { func_has_abort(&fc as *const FunccallT) });
    }

    #[test]
    fn func_has_ended_true_when_fc_returned() {
        let mut fp = UfuncT::default();
        let fc = FunccallT { fc_func: &mut fp as *mut UfuncT, fc_returned: 1, ..Default::default() };
        assert!(unsafe { func_has_ended(&fc as *const FunccallT) });
    }

    #[test]
    fn func_has_ended_true_when_abort_flag_and_did_emsg_and_not_aborted_in_try() {
        let _lock = crate::globals::global_state_test_lock();
        let saved_did_emsg = unsafe { crate::globals::GLOBALS.get_mut() }.did_emsg;
        let saved_force_abort = unsafe { crate::globals::GLOBALS.get_mut() }.force_abort;
        unsafe { crate::globals::GLOBALS.get_mut() }.did_emsg = 1;
        unsafe { crate::globals::GLOBALS.get_mut() }.force_abort = false;

        let mut fp = UfuncT { uf_flags: fc_flags::ABORT, ..Default::default() };
        let fc = FunccallT { fc_func: &mut fp as *mut UfuncT, ..Default::default() };
        assert!(unsafe { func_has_ended(&fc as *const FunccallT) });

        unsafe { crate::globals::GLOBALS.get_mut() }.did_emsg = saved_did_emsg;
        unsafe { crate::globals::GLOBALS.get_mut() }.force_abort = saved_force_abort;
    }

    #[test]
    fn func_has_ended_false_when_aborted_in_try_suppresses_the_abort_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let saved_did_emsg = unsafe { crate::globals::GLOBALS.get_mut() }.did_emsg;
        let saved_force_abort = unsafe { crate::globals::GLOBALS.get_mut() }.force_abort;
        unsafe { crate::globals::GLOBALS.get_mut() }.did_emsg = 1;
        unsafe { crate::globals::GLOBALS.get_mut() }.force_abort = true; // aborted_in_try() -> true

        let mut fp = UfuncT { uf_flags: fc_flags::ABORT, ..Default::default() };
        let fc = FunccallT { fc_func: &mut fp as *mut UfuncT, ..Default::default() };
        assert!(!unsafe { func_has_ended(&fc as *const FunccallT) });

        unsafe { crate::globals::GLOBALS.get_mut() }.did_emsg = saved_did_emsg;
        unsafe { crate::globals::GLOBALS.get_mut() }.force_abort = saved_force_abort;
    }

    // The following tests all touch the shared FUNC_HASHTAB GlobalCell -
    // each acquires global_state_test_lock() for its whole body and
    // calls func_init() first, so leftover entries from a previous test
    // (or a concurrently-running one, absent the lock) can't leak in.

    #[test]
    fn func_init_starts_with_an_empty_table() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        assert!(find_func(b"anything").is_null());
    }

    #[test]
    fn func_hashtab_add_then_find_func_round_trips() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"MyFunc\0".to_vec(), ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        let rc = unsafe { func_hashtab_add(fp_ptr) };
        assert_eq!(rc, OK);
        let found = find_func(b"MyFunc");
        assert_eq!(found, fp_ptr);
    }

    #[test]
    fn find_func_returns_null_for_unknown_name() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"Known\0".to_vec(), ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        unsafe { func_hashtab_add(fp_ptr) };
        assert!(find_func(b"Unknown").is_null());
    }

    #[test]
    fn func_hashtab_add_rejects_duplicate_name() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp1 = Box::new(UfuncT { uf_name: b"Dup\0".to_vec(), ..Default::default() });
        let mut fp2 = Box::new(UfuncT { uf_name: b"Dup\0".to_vec(), ..Default::default() });
        let rc1 = unsafe { func_hashtab_add(fp1.as_mut() as *mut UfuncT) };
        let rc2 = unsafe { func_hashtab_add(fp2.as_mut() as *mut UfuncT) };
        assert_eq!(rc1, OK);
        assert_eq!(rc2, FAIL);
    }

    #[test]
    fn func_remove_deletes_a_present_entry_and_reports_true() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"Removable\0".to_vec(), ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        unsafe { func_hashtab_add(fp_ptr) };
        assert!(!find_func(b"Removable").is_null());
        let removed = unsafe { func_remove(fp_ptr) };
        assert!(removed);
        assert!(find_func(b"Removable").is_null());
    }

    #[test]
    fn func_remove_reports_false_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"NeverAdded\0".to_vec(), ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        let removed = unsafe { func_remove(fp_ptr) };
        assert!(!removed);
    }

    #[test]
    fn func_tbl_get_returns_a_usable_pointer_reflecting_ht_used() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"Counted\0".to_vec(), ..Default::default() });
        unsafe { func_hashtab_add(fp.as_mut() as *mut UfuncT) };
        let ht_ptr = func_tbl_get();
        assert!(!ht_ptr.is_null());
        assert_eq!(unsafe { (*ht_ptr).ht_used }, 1);
    }

    #[test]
    fn func_is_global_true_for_ordinary_name() {
        let fp = UfuncT { uf_name: b"MyFunc\0".to_vec(), ..Default::default() };
        assert!(func_is_global(&fp));
    }

    #[test]
    fn func_is_global_false_for_script_local_name() {
        // <SNR> is encoded as K_SPECIAL KS_EXTRA KE_SNR - only the
        // leading K_SPECIAL byte (0x80) matters to func_is_global.
        let fp = UfuncT { uf_name: vec![0x80, 0x00, 0x00, b'1', b'_', b'F', b'o', b'o', 0], ..Default::default() };
        assert!(!func_is_global(&fp));
    }

    #[test]
    fn cat_func_name_global_name_passes_through() {
        let fp = UfuncT { uf_name: b"MyFunc\0".to_vec(), ..Default::default() };
        assert_eq!(cat_func_name(&fp), b"MyFunc");
    }

    #[test]
    fn cat_func_name_script_local_name_gets_snr_prefix() {
        // K_SPECIAL, then 2 more encoding bytes, then "1_Foo" - matches
        // the original's own "uf_name + 3" skip.
        let fp = UfuncT { uf_name: vec![0x80, 0x00, 0x00, b'1', b'_', b'F', b'o', b'o', 0], ..Default::default() };
        assert_eq!(cat_func_name(&fp), b"<SNR>1_Foo");
    }

    #[test]
    fn cat_func_name_short_script_local_name_falls_back_to_raw() {
        // uflen (here: clean-name length) <= 3, so the "!func_is_global
        // && uflen > 3" branch is skipped - falls through to the plain
        // "%s" formatting of the whole (still K_SPECIAL-prefixed) name,
        // exactly matching the original's own else-branch.
        let fp = UfuncT { uf_name: vec![0x80, 0x00, 0x00, 0], ..Default::default() };
        assert_eq!(cat_func_name(&fp), vec![0x80, 0x00, 0x00]);
    }

    #[test]
    fn cat_func_name_without_trailing_nul_is_handled() {
        // An unregistered UfuncT (never passed through
        // func_hashtab_add) may not have a trailing NUL yet - must not
        // be treated as if the last real byte were a terminator.
        let fp = UfuncT { uf_name: b"MyFunc".to_vec(), ..Default::default() };
        assert_eq!(cat_func_name(&fp), b"MyFunc");
    }

    #[test]
    fn printable_func_name_uses_uf_name_exp_when_set() {
        let fp = UfuncT {
            uf_name: vec![0x80, 0x00, 0x00, b'1', b'_', b'F', b'o', b'o', 0],
            uf_name_exp: Some(b"<SNR>1_Foo".to_vec()),
            ..Default::default()
        };
        assert_eq!(printable_func_name(&fp), b"<SNR>1_Foo");
    }

    #[test]
    fn printable_func_name_falls_back_to_uf_name_when_unset() {
        let fp = UfuncT { uf_name: b"MyFunc\0".to_vec(), uf_name_exp: None, ..Default::default() };
        assert_eq!(printable_func_name(&fp), b"MyFunc\0");
    }

    #[test]
    fn function_list_modified_false_when_unchanged() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let prev = unsafe { (*func_tbl_get()).ht_changed };
        assert!(!function_list_modified(prev));
    }

    #[test]
    fn function_list_modified_true_after_a_real_hashtab_change() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let prev = unsafe { (*func_tbl_get()).ht_changed };
        let mut fp = Box::new(UfuncT { uf_name: b"Changed\0".to_vec(), ..Default::default() });
        unsafe { func_hashtab_add(fp.as_mut() as *mut UfuncT) };
        assert!(function_list_modified(prev));
    }

    #[test]
    fn builtin_function_true_for_ordinary_lowercase_name() {
        assert!(builtin_function(b"len"));
    }

    #[test]
    fn builtin_function_false_for_empty_name() {
        assert!(!builtin_function(b""));
    }

    #[test]
    fn builtin_function_false_for_uppercase_first_letter() {
        assert!(!builtin_function(b"Len"));
    }

    #[test]
    fn builtin_function_false_for_script_local_colon_form() {
        // name[1] == ':' - e.g. a Lua-style "s:Foo" prefix character.
        assert!(!builtin_function(b"s:Foo"));
    }

    #[test]
    fn builtin_function_false_when_containing_autoload_separator() {
        assert!(!builtin_function(b"foo#bar"));
    }

    #[test]
    fn builtin_function_single_char_name_is_fine() {
        // name[1] doesn't exist (matches the original's own read of a
        // C string's NUL terminator, which is never ':').
        assert!(builtin_function(b"x"));
    }

    #[test]
    fn check_user_func_argcount_unknown_when_within_arity() {
        let fp = UfuncT {
            uf_args: crate::garray_defs::GarrayT { ga_len: 3, ..Default::default() },
            uf_def_args: crate::garray_defs::GarrayT { ga_len: 1, ..Default::default() },
            ..Default::default()
        };
        // regular_args=3, def_args=1 -> minimum required = 2.
        assert_eq!(check_user_func_argcount(&fp, 2), FnameTransError::Unknown);
        assert_eq!(check_user_func_argcount(&fp, 3), FnameTransError::Unknown);
    }

    #[test]
    fn check_user_func_argcount_toofew_below_minimum() {
        let fp = UfuncT {
            uf_args: crate::garray_defs::GarrayT { ga_len: 3, ..Default::default() },
            uf_def_args: crate::garray_defs::GarrayT { ga_len: 1, ..Default::default() },
            ..Default::default()
        };
        assert_eq!(check_user_func_argcount(&fp, 1), FnameTransError::TooFew);
    }

    #[test]
    fn check_user_func_argcount_toomany_without_varargs() {
        let fp = UfuncT {
            uf_args: crate::garray_defs::GarrayT { ga_len: 2, ..Default::default() },
            uf_varargs: 0,
            ..Default::default()
        };
        assert_eq!(check_user_func_argcount(&fp, 3), FnameTransError::TooMany);
    }

    #[test]
    fn check_user_func_argcount_extra_args_ok_with_varargs() {
        let fp = UfuncT {
            uf_args: crate::garray_defs::GarrayT { ga_len: 2, ..Default::default() },
            uf_varargs: 1,
            ..Default::default()
        };
        assert_eq!(check_user_func_argcount(&fp, 10), FnameTransError::Unknown);
    }

    // ---- save_funccal / restore_funccal / get_current_funccal / --------
    // ---- set_current_funccal --------------------------------------------

    #[test]
    fn save_and_restore_funccal_round_trips_current_funccal() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *FUNCCAL_STACK.get_mut() = std::ptr::null_mut() };

        let mut fc = Box::new(FunccallT::default());
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        unsafe { *CURRENT_FUNCCAL.get_mut() = fc_ptr };

        let mut entry = FuncCalEntryT::default();
        unsafe { save_funccal(&mut entry as *mut FuncCalEntryT) };

        // save_funccal clears CURRENT_FUNCCAL and remembers the old one.
        assert!(get_current_funccal().is_null());
        assert_eq!(entry.top_funccal, fc_ptr);

        restore_funccal();
        assert_eq!(get_current_funccal(), fc_ptr);
        assert!(unsafe { *FUNCCAL_STACK.get_mut() }.is_null());

        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    #[test]
    fn save_funccal_stacks_nested_entries_in_lifo_order() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *FUNCCAL_STACK.get_mut() = std::ptr::null_mut() };

        let mut outer_fc = Box::new(FunccallT::default());
        let outer_fc_ptr = outer_fc.as_mut() as *mut FunccallT;
        unsafe { *CURRENT_FUNCCAL.get_mut() = outer_fc_ptr };

        let mut outer_entry = FuncCalEntryT::default();
        unsafe { save_funccal(&mut outer_entry as *mut FuncCalEntryT) };
        assert!(get_current_funccal().is_null());

        // A second save (e.g. a nested autocommand) with nothing
        // current in between - top_funccal captures null here.
        let mut inner_entry = FuncCalEntryT::default();
        unsafe { save_funccal(&mut inner_entry as *mut FuncCalEntryT) };
        assert!(inner_entry.top_funccal.is_null());

        restore_funccal(); // pops inner_entry
        assert!(get_current_funccal().is_null());

        restore_funccal(); // pops outer_entry
        assert_eq!(get_current_funccal(), outer_fc_ptr);

        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
    }

    #[test]
    #[cfg_attr(debug_assertions, ignore)]
    fn restore_funccal_on_empty_stack_is_a_harmless_noop_in_release() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        unsafe { *FUNCCAL_STACK.get_mut() = std::ptr::null_mut() };
        // Matches the original's own "iemsg + do nothing else" path -
        // this crate skips the message display but must not corrupt
        // CURRENT_FUNCCAL. Only reachable in --release, where
        // debug_assert! compiles out entirely (see the dedicated
        // panic test for the debug-build behavior).
        restore_funccal();
        assert!(get_current_funccal().is_null());
    }

    #[test]
    #[cfg_attr(not(debug_assertions), ignore)]
    #[should_panic(expected = "INTERNAL: restore_funccal()")]
    fn restore_funccal_on_empty_stack_panics_in_debug_builds() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *FUNCCAL_STACK.get_mut() = std::ptr::null_mut() };
        restore_funccal();
    }

    #[test]
    fn get_and_set_current_funccal_round_trip() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *CURRENT_FUNCCAL.get_mut() = std::ptr::null_mut() };
        assert!(get_current_funccal().is_null());

        let mut fc = Box::new(FunccallT::default());
        let fc_ptr = fc.as_mut() as *mut FunccallT;
        set_current_funccal(fc_ptr);
        assert_eq!(get_current_funccal(), fc_ptr);

        set_current_funccal(std::ptr::null_mut());
        assert!(get_current_funccal().is_null());
    }

    #[test]
    fn can_add_defer_false_without_a_current_funccal() {
        let _lock = crate::globals::global_state_test_lock();
        set_current_funccal(std::ptr::null_mut());
        assert!(!can_add_defer());
    }

    #[test]
    fn can_add_defer_true_with_a_current_funccal() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fc = Box::new(FunccallT::default());
        set_current_funccal(fc.as_mut() as *mut FunccallT);
        assert!(can_add_defer());
        set_current_funccal(std::ptr::null_mut());
    }

    #[test]
    fn func_name_refcount_true_for_numbered_and_lambda_names() {
        assert!(func_name_refcount(b"123"));
        assert!(func_name_refcount(b"<lambda>42"));
    }

    #[test]
    fn func_name_refcount_false_for_ordinary_names_and_empty() {
        assert!(!func_name_refcount(b"MyFunc"));
        assert!(!func_name_refcount(b""));
        // '<' alone, without a following 'l', doesn't count either.
        assert!(!func_name_refcount(b"<SNR>1_Foo"));
    }

    #[test]
    fn func_ref_none_is_noop() {
        func_ref(None);
    }

    #[test]
    fn func_ref_ordinary_name_is_noop_even_if_unregistered() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        // "MyFunc" isn't refcounted by name at all - must return
        // immediately without ever calling find_func/debug_assert!.
        func_ref(Some(b"MyFunc"));
    }

    #[test]
    fn func_ref_increments_refcount_for_a_registered_numbered_function() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp = Box::new(UfuncT { uf_name: b"42\0".to_vec(), uf_refcount: 1, ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        unsafe { func_hashtab_add(fp_ptr) };
        func_ref(Some(b"42"));
        assert_eq!(fp.uf_refcount, 2);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "func_ref: numbered function not found")]
    fn func_ref_panics_for_unregistered_numbered_function() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        func_ref(Some(b"999"));
    }

    #[test]
    fn func_unref_none_is_noop() {
        func_unref(None);
    }

    #[test]
    fn func_unref_ordinary_name_is_noop_even_if_unregistered() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        func_unref(Some(b"MyFunc"));
    }

    #[test]
    fn func_unref_decrements_refcount_for_a_registered_lambda() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        let mut fp =
            Box::new(UfuncT { uf_name: b"<lambda>1\0".to_vec(), uf_refcount: 2, ..Default::default() });
        let fp_ptr = fp.as_mut() as *mut UfuncT;
        unsafe { func_hashtab_add(fp_ptr) };
        func_unref(Some(b"<lambda>1"));
        assert_eq!(fp.uf_refcount, 1);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "func_unref: numbered function not found")]
    fn func_unref_panics_for_unregistered_numbered_function() {
        let _lock = crate::globals::global_state_test_lock();
        func_init();
        func_unref(Some(b"999"));
    }
}
