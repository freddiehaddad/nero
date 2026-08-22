//! Translated from `src/nvim/eval/typval.c` (tractable core: the
//! `dict_T`/`list_T`/`blob_T` alloc/free/refcount/insertion primitives,
//! `tv_copy`, `tv_get_number`/`tv_get_bool`, and the full type-check/
//! argument-check family).
//!
//! `typval.c` (~4000 lines) is the core of the Vimscript value system:
//! `typval_T`/`list_T`/`dict_T`/`blob_T` construction, (de)serialization
//! via a shared encode-traversal abstraction, deep copying, and every
//! built-in operation on those types. Only the foundational alloc/
//! free/refcount/insertion primitives for all three container types
//! are translated here; see this module's own per-function deferral
//! notes below for the rest.
//!
//! # `dict_T`/`dictitem_T` representation (the design decision this
//! unblocks)
//!
//! See `eval/typval_defs.rs`'s `DictitemT`/`DictT` doc comments for
//! the full reasoning. In short: the original's `dictitem_T` uses a C
//! "flexible array member" (`di_key[]`) so that `hashtab_defs.rs`'s
//! `HashitemT.hi_key` can point directly at the key bytes living
//! *inside* the same allocation as the rest of the item, letting
//! `TV_DICT_HI2DI` recover the owning `dictitem_T*` via
//! `hi_key - offsetof(dictitem_T, di_key)` pointer arithmetic. Rust
//! has no safe equivalent of this (a faithful replication would need
//! a hand-rolled dynamically-sized type: manual `Layout` computation,
//! raw `alloc`/`dealloc`, fat-pointer reconstruction on every access -
//! disproportionate unsafe complexity for what is, in the original,
//! purely a one-pointer memory optimization with no observable
//! behavioral difference). So `DictitemT.di_key` is an owned `Vec<u8>`
//! instead (a separate heap allocation, matching the already-existing
//! `ChangedtickDictItem`/`ScopeDictDictItem` precedent), and `DictT`
//! carries a new `dv_index: HashMap<usize, *mut DictitemT>` side table
//! (keyed by each item's `hi_key` address) in place of
//! `TV_DICT_HI2DI` - every function below that would use that macro
//! consults `dv_index` instead.
//!
//! `DictitemT`/`DictT`/`ListT`/`ListitemT`/`BlobT` are all heap-
//! allocated via `Box::into_raw`/`Box::from_raw`, matching this
//! crate's established raw-pointer-linked convention (not
//! `Rc`/`RefCell`).
//!
//! # Translated
//! **Dict**: `tv_dict_item_alloc`(`_len`, collapsed into one function
//! taking `&[u8]`), `tv_dict_item_free`, `tv_dict_item_copy`,
//! `tv_dict_item_remove`, `tv_dict_alloc`, `tv_dict_free_contents`/
//! `tv_dict_free_dict`/`tv_dict_free`/`tv_dict_unref`, `tv_dict_find`/
//! `tv_dict_has_key`/`tv_dict_len`, `tv_dict_add` (omits the original's
//! `tv_dict_wrong_func_name` g:/l: validation - needs
//! `get_globvar_dict`/`get_funccal_local_ht`/`var_wrong_func_name`,
//! none translated, and nothing in this crate can even construct a
//! real global/local-funccall scope dict yet for that check to apply
//! to), `tv_dict_add_list`/`_dict`/`_tv`/`_nr`/`_float`/`_bool`/`_str`
//! (`_str_len`/`_allocated_str` collapsed into `tv_dict_add_str`,
//! since Rust's `&[u8]` already carries its own length - see
//! `tv_dict_add_str`'s own doc comment), `tv_dict_add_func` (needs
//! `(*fp).uf_name` NUL-terminated, matching `func_hashtab`'s own
//! storage convention - see its own doc comment), `tv_dict_get_string`/
//! `tv_dict_get_string_chk` (collapsing the original's `_buf`/`save`
//! variants the same way `tv_get_string_chk`'s own doc comment
//! explains - tractable now that `tv_get_string`/`tv_get_string_chk`
//! exist), `tv_dict_get_tv`/`tv_dict_get_number`/
//! `tv_dict_get_number_def`/`tv_dict_get_bool`.
//!
//! **List**: `tv_list_alloc`, `tv_list_item_alloc` (private, matching
//! the original's own `static`), `tv_list_free_contents`/
//! `tv_list_free_list`/`tv_list_free`/`tv_list_unref`/`tv_list_ref`,
//! `tv_list_append`/`tv_list_append_tv`/`tv_list_append_owned_tv`/
//! `tv_list_append_list`/`tv_list_append_dict`/`tv_list_append_string`
//! (`tv_list_append_allocated_string` collapsed in, same reasoning as
//! `tv_dict_add_str`)/`tv_list_append_number`, `tv_list_insert`/
//! `tv_list_insert_tv`, `tv_list_drop_items`/`tv_list_remove_items`/
//! `tv_list_item_remove`, `tv_list_watch_add`/`tv_list_watch_remove`/
//! `tv_list_watch_fix`, `tv_list_uidx`/`tv_list_find`/
//! `tv_list_find_nr`/`tv_list_find_str`/`tv_list_find_index` (private,
//! matching the original's own `static`)/`tv_list_idx_of_item`/
//! `tv_list_reverse`.
//!
//! **Blob**: `tv_blob_alloc`/`tv_blob_free`/`tv_blob_unref`,
//! `tv_blob_len`/`tv_blob_set_ret` (`eval/typval.h`'s own `static
//! inline` helpers, harvested for `eval/eval.rs`'s `eval_addblob`).
//!
//! **Partial**: `partial_free`/`partial_unref` (`eval.c`, not
//! `eval/typval.c` - kept here anyway alongside the sibling `tv_*_free`/
//! `_unref` functions, see their own doc comments for why). Releases
//! `pt_dict` (via the real `tv_dict_unref`) and each `pt_argv` entry
//! (via `tv_clear_simple`, one level); calls the real
//! `crate::eval::userfunc::func_ptr_unref`/`func_unref` to release
//! `pt_func`'s own refcount, whether `pt_name` is absent or present
//! respectively.
//!
//! **Copy**: `tv_copy` (the `VAR_FUNC` branch calls the real
//! `crate::eval::userfunc::func_ref`; the `VAR_PARTIAL` branch
//! increments the real `pt_refcount` field).
//!
//! **String conversion**: `tv_get_string`/`tv_get_string_chk` (the
//! original's `_buf`/`_buf_chk` variants aren't translated separately -
//! see [`tv_get_string_chk`]'s own doc comment for why).
//!
//! A shared private `tv_clear_simple` helper (this crate's own,
//! replacing the original's `tv_clear`'s simple-value branches - see
//! "Deferred" below) is used by both `tv_dict_item_free` and every
//! list-item-freeing function above to release a value's List/Dict/
//! Blob/Partial reference (via the real `tv_list_unref`/`tv_dict_unref`/
//! `tv_blob_unref`/`partial_unref` above) -
//! Number/String/Bool/Special/Float/Func/Unknown need no explicit
//! release at all (Rust's own ownership drops their `Vec<u8>`/etc.
//! automatically).
//!
//! `gc_first_dict`/`gc_first_list` (the original's file-static "list
//! of all live dicts/lists, for `:garbagecollect`" linked-list heads)
//! are translated as their own `GlobalCell`-backed statics, matching
//! `buffer.rs`'s `TOP_FILE_NUM`/`BUF_FREE_COUNT` precedent - the
//! linked-list bookkeeping itself (`dv_used_next`/`dv_used_prev`,
//! `lv_used_next`/`lv_used_prev`) is maintained faithfully even though
//! the actual garbage collector that would walk it is a much later
//! phase, so that phase won't need to retrofit this bookkeeping later.
//!
//! `watchers`/`lua_table_ref` are left inert: `DictT` has no
//! `watchers` field at all yet (needs a `QUEUE` intrusive-linked-list
//! translation first - see `typval_defs.rs`; `ListT`'s own `lv_watch`
//! chain *is* translated, since it's a plain raw-pointer singly-linked
//! list already modeled directly on `ListwatchT`, not a `QUEUE`), and
//! every `lua_table_ref` is always `LUA_NOREF` (the Lua host, phase
//! 13, isn't started).
//!
//! Also translated: `callback_from_typval`/`callback_free` (`eval.c`/
//! `eval/typval.c`) - the `Callback` conversion/lifecycle functions
//! used directly by `prompt_setcallback()`/`prompt_setinterrupt()`
//! (`eval/buffer.rs`), and a real step toward (but not the whole of)
//! the dict-watcher subsystem, which still additionally needs the
//! `QUEUE`-as-`Vec` `DictWatcher` design and `tv_dict_watcher_add`/
//! `_remove`/`_notify` themselves. `callback_put`/`callback_copy`/
//! `callback_to_string`/`tv_callback_equal` are also complete.
//!
//! # Deferred
//! - `tv_clear`/`tv_free` themselves: `tv_clear`'s *real* behavior is
//!   implemented via a shared encode-traversal abstraction
//!   (`encode_vim_to_nothing`, `viml_encode.c` - reused for JSON/
//!   msgpack encoding too, not just clearing) - a separate, substantial
//!   subsystem of its own, not attempted here. This module's own
//!   `tv_clear_simple` covers everything that subsystem would do
//!   *except* recursing into nested containers' own contents (List/
//!   Dict values are unref'd, i.e. their own top-level refcount is
//!   decremented and they're freed at zero, but freeing one doesn't
//!   need to recurse further here since `tv_list_free_contents`/
//!   `tv_dict_free_contents` themselves already do that recursion one
//!   level at a time via the same helper). This same reasoning is also
//!   used by `eval/userfunc.rs`'s `free_funccal_contents` in place of
//!   the original's `tv_clear`-calling `vars_clear`/`TV_LIST_ITER`
//!   loop, treated as a faithful substitute for any well-formed,
//!   acyclic value (the only kind Vimscript's reference-counted value
//!   model can produce).
//! - `tv_get_lnum` (needs `var2fpos`/`curwin`, `window.c`, for its
//!   "special string like `.`/`$`" fallback branch) remains deferred.
//!   `tv_get_number`/`tv_get_number_chk`/`tv_get_bool`/`tv_get_bool_chk`
//!   are translated now that `charset.c`'s `vim_str2nr` exists (the
//!   only real blocker for `VAR_STRING`'s branch); their own
//!   `emsg`/`semsg` calls for wrong-type values are omitted (message
//!   display, not tractable), while the error-flag/return-value
//!   behavior is kept exactly. `tv_get_string`/`tv_get_string_chk` are
//!   now translated too (collapsing the original's 4-function
//!   `_buf`/`_buf_chk` family down to 2 - see
//!   [`tv_get_string_chk`]'s own doc comment for why), using a new,
//!   narrow `fmt_g` helper (verified against 68 real glibc `printf`
//!   reference outputs) for `VAR_FLOAT`'s `%g`-formatted case - NOT a
//!   general `vim_snprintf` implementation, which remains its own
//!   separate, substantial undertaking (see `strings.rs`'s own module
//!   doc).
//! - Every other `tv_dict_*`/`tv_list_*`/`tv_blob_*` function
//!   (`tv_dict_extend`, `tv_list_copy`, `tv_list_concat`, blob
//!   byte-level accessors, iteration helpers, etc.): straightforward
//!   to add once needed, layered on top of the primitives here.
//!
//! # Type checks
//! `tv_check_str_or_nr`/`tv_check_num`/`tv_check_str` (pure type-tag
//! predicates), `tv_list_locked`/`tv_islocked`/`value_check_lock`/
//! `tv_check_lock` (needed only already-real `lv_lock`/`dv_lock`/
//! `bv_lock` fields - `value_check_lock`/`tv_check_lock` drop the
//! original's `name_len` parameter entirely, since its
//! `TV_TRANSLATE`/`TV_CSTRING` sentinel-encoding only ever affected
//! the omitted message TEXT, never the return value - see
//! [`value_check_lock`]'s own doc comment), and the full
//! `tv_check_for_*_arg` family (21 functions: argument-type guards
//! used by builtin Vimscript function implementations to validate
//! their own arguments before proceeding) - `args[idx]` indexing
//! becomes a plain Rust slice index. Every real `emsg`/`semsg` call in
//! this whole section is omitted (message display, not tractable),
//! keeping only the `bool`/`OK`/`FAIL` result, matching this module's
//! established policy throughout.
//!
//! # Locks
//! `tv_item_lock`: recursively (un)locks an item and (for `deep < 0`
//! or `> 1`) every value it contains, using a new `TV_ITEM_LOCK_RECURSE`
//! `GlobalCell` for the original's own function-local recursion-depth
//! counter (matching `tv_equal`'s own established translation of the
//! same C idiom) - its own `emsg` for exceeding `DICT_MAXNEST` is
//! omitted, keeping the identical silent-give-up control flow.
//!
//! # Indexing/searching
//! `tv_list_uidx`/`tv_list_find` (the cache-aware nearest-of-{start,
//! cached, end} search, including its `lv_idx`/`lv_idx_item` caching
//! side effect)/`tv_list_find_nr`/`tv_list_find_str`/
//! `tv_list_find_index` (private, matching the original's own
//! `static`)/`tv_list_idx_of_item`/`tv_list_reverse` (in-place
//! doubly-linked-list reversal).

use crate::eval::typval_defs::{
    dict_item_flags, Callback, DictT, DictitemT, ListLenSpecials, ListT,
    PartialT, ScopeType, TypvalT, TypvalValue, VarLockStatus, VarType,
    VarnumberT,
};
use crate::eval::gc::{GC_FIRST_DICT, GC_FIRST_LIST};
use crate::globals::GlobalCell;
use crate::vim_defs::{FAIL, OK};

/// `LUA_NOREF`: represents a missing Lua reference - `DictT`'s own
/// `lua_table_ref` is always this value until the Lua host (phase 13)
/// exists.
const LUA_NOREF: crate::types_defs::LuaRef = -1;

/// Test-only accessor: `true` if no `Dict` is currently linked into
/// the shared `GC_FIRST_DICT` registry - lets tests in OTHER modules
/// (e.g. `eval::eval`'s own `handle_subscript` tests) directly verify
/// they leave no dangling/leaked `Dict` behind, matching this
/// session's own established "check `GC_FIRST_LIST`/`GC_FIRST_DICT`
/// before/after" regression-proving convention.
#[cfg(test)]
pub(crate) fn gc_first_dict_is_empty() -> bool {
    // SAFETY: GC_FIRST_DICT is only ever read/written through this
    // accessor and the crate's own established `global_state_test_lock()`
    // discipline, matching every other read site in this module.
    unsafe { *GC_FIRST_DICT.get_mut() }.is_null()
}

/// Allocate a dictionary item. The type and value of the item
/// (`.di_tv`) still need to be initialized by the caller
/// (`tv_dict_item_alloc`/`tv_dict_item_alloc_len` - collapsed into one
/// function here, see this module's own doc comment for why).
#[must_use]
pub fn tv_dict_item_alloc(key: &[u8]) -> *mut DictitemT {
    let mut di_key = Vec::with_capacity(key.len() + 1);
    di_key.extend_from_slice(key);
    di_key.push(0); // NUL terminator, matching hi_key's C-string contract
    Box::into_raw(Box::new(DictitemT {
        di_tv: TypvalT::default(),
        di_flags: dict_item_flags::ALLOC,
        di_key,
    }))
}

/// Increase reference count for a given list. Does nothing for `NULL`
/// lists (`tv_list_ref`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT`.
pub unsafe fn tv_list_ref(l: *mut crate::eval::typval_defs::ListT) {
    if l.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*l).lv_refcount += 1 };
}

/// Set the return value of `tv` to a list, incrementing its reference
/// count (`tv_list_set_ret`, `eval/typval.h`'s own `static inline`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT`.
pub unsafe fn tv_list_set_ret(tv: &mut TypvalT, l: *mut crate::eval::typval_defs::ListT) {
    tv.value = TypvalValue::List(l);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_ref(l) };
}

/// Get the number of items in a list, or `0` if `l` is null
/// (`tv_list_len`, `eval/typval.h`'s own `static inline`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT`.
#[must_use]
pub unsafe fn tv_list_len(l: *const crate::eval::typval_defs::ListT) -> i32 {
    if l.is_null() {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*l).lv_len }
}

/// Copy typval from one location to another (`tv_copy`).
///
/// When needed, allocates a string or increases a reference count.
/// Does not make a copy of a container, but copies its reference.
///
/// It is OK for `from` and `to` to point to the same location - this
/// is used to make a copy later (matches the original's own note;
/// this translation, cloning `from`'s value up front before writing
/// `to`, naturally supports this too).
///
/// # Safety
/// If `from`'s value is `List`/`Dict`/`Blob`/`Partial`-typed with a
/// non-null pointer, that pointer must be valid (matching every other
/// function in this crate that touches those types).
pub unsafe fn tv_copy(from: &TypvalT, to: &mut TypvalT) {
    to.v_lock = VarLockStatus::Unlocked;
    to.value = from.value.clone();
    match &to.value {
        TypvalValue::Unknown => {
            // semsg(_(e_intern2), "tv_copy(UNKNOWN)") omitted (message
            // subsystem, phase 15) - this is an internal-error report
            // for a case that should never legitimately occur.
            debug_assert!(false, "tv_copy(UNKNOWN): matches the original's own internal-error report");
        }
        TypvalValue::Number(_)
        | TypvalValue::Float(_)
        | TypvalValue::Bool(_)
        | TypvalValue::Special(_)
        | TypvalValue::String(_) => {
            // Number/Float/Bool/Special: plain values, nothing extra
            // to do. String: `.clone()` above already deep-copied the
            // owned Vec<u8> bytes - matching the original's own
            // `xstrdup`, just without a manual allocation call.
        }
        TypvalValue::Func(name) => {
            // The name string itself is already deep-copied via
            // `.clone()` above; `func_ref` additionally increments the
            // named function's own `uf_refcount` (`find_func()`-backed
            // lookup), matching the original's `func_ref(to->vval.v_string)`.
            crate::eval::userfunc::func_ref(name.as_deref());
        }
        TypvalValue::Partial(p) => {
            if !p.is_null() {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { (**p).pt_refcount += 1 };
            }
        }
        TypvalValue::Blob(blob) => {
            if !blob.is_null() {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { (**blob).bv_refcount += 1 };
            }
        }
        TypvalValue::List(list) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_list_ref(*list) };
        }
        TypvalValue::Dict(dict) => {
            if !dict.is_null() {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { (**dict).dv_refcount += 1 };
            }
        }
    }
}

/// Get the number value of a Vimscript object (`tv_get_number_chk`).
///
/// Returns `vim_str2nr()`'s output for `VAR_STRING` objects, the value
/// itself for `VAR_NUMBER`, `1`/`0` for `VAR_BOOL`, `0` for
/// `VAR_SPECIAL`, or `-1` (`ret_error` is `None`) / `0` (`ret_error` is
/// `Some`) for every other type (also writing `true` to `*ret_error`
/// in that `Some` case).
///
/// The original's own `emsg(_(num_errors[tv->v_type]))`/
/// `semsg(_(e_intern2), "tv_get_number(UNKNOWN)")` calls (real,
/// reachable user/internal-error messages) are omitted - needs
/// `message.c`'s display pipeline, not tractable - while the identical
/// error-flag/return-value behavior is kept exactly, matching this
/// crate's established "skip the display, keep the state" policy
/// (e.g. `undo.rs`'s `u_get_headentry`/`ex_undojoin`).
#[must_use]
pub fn tv_get_number_chk(tv: &TypvalT, ret_error: Option<&mut bool>) -> crate::eval::typval_defs::VarnumberT {
    match &tv.value {
        TypvalValue::Func(_)
        | TypvalValue::Partial(_)
        | TypvalValue::List(_)
        | TypvalValue::Dict(_)
        | TypvalValue::Blob(_)
        | TypvalValue::Float(_)
        | TypvalValue::Unknown => {
            // emsg(_(num_errors[tv->v_type])) / semsg(...) omitted -
            // see this function's own doc comment.
            match ret_error {
                Some(e) => {
                    *e = true;
                    0
                }
                None => -1,
            }
        }
        TypvalValue::Number(n) => *n,
        TypvalValue::String(s) => {
            let mut n: crate::eval::typval_defs::VarnumberT = 0;
            if let Some(s) = s {
                crate::charset::vim_str2nr(s, None, None, crate::charset::STR2NR_ALL, Some(&mut n), None, 0, false, None);
            }
            n
        }
        TypvalValue::Bool(b) => i64::from(*b == crate::eval::typval_defs::BoolVarValue::True),
        TypvalValue::Special(_) => 0,
    }
}

/// Get the number value of a Vimscript object, without an error-flag
/// out-parameter (`tv_get_number`).
#[must_use]
pub fn tv_get_number(tv: &TypvalT) -> crate::eval::typval_defs::VarnumberT {
    let mut error = false;
    tv_get_number_chk(tv, Some(&mut error))
}

/// Get the line number from a Vimscript object, using `GLOBALS.curwin`
/// for any non-numeric position-string fallback (e.g. `"."`/`"v"`/
/// `"$"`) (`tv_get_lnum`).
///
/// # Safety
/// Touches `GLOBALS.curwin` (via [`crate::eval::eval::var2fpos`]) -
/// `GLOBALS.curwin` must be a valid, live `WinT` pointer whose
/// `w_buffer` is also valid and live.
#[must_use]
pub unsafe fn tv_get_lnum(tv: &TypvalT) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let did_emsg_before = unsafe { crate::globals::GLOBALS.get_mut() }.did_emsg;
    let mut lnum = tv_get_number_chk(tv, None) as crate::pos_defs::LinenrT;
    // SAFETY: forwarded from this function's own safety doc.
    let did_emsg_now = unsafe { crate::globals::GLOBALS.get_mut() }.did_emsg;
    if lnum <= 0 && did_emsg_before == did_emsg_now && !matches!(tv.value, TypvalValue::Number(_)) {
        // SAFETY: forwarded from this function's own safety doc.
        let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        // SAFETY: forwarded from this function's own safety doc.
        if let Some(fp) = unsafe {
            crate::eval::eval::var2fpos(tv, true, None, false, curwin)
        } {
            lnum = fp.lnum;
        }
    }
    lnum
}

/// Get the line number from a Vimscript object - unlike
/// [`tv_get_lnum`], only supports the `"$"` special string (resolved
/// via `buf`, which may be `None`, in which case `"$"` yields `0`),
/// no other position-string fallback (`tv_get_lnum_buf`).
#[must_use]
pub fn tv_get_lnum_buf(tv: &TypvalT, buf: Option<&crate::buffer_defs::BufT>) -> crate::pos_defs::LinenrT {
    if let TypvalValue::String(Some(s)) = &tv.value
        && s.as_slice() == b"$"
        && let Some(b) = buf
    {
        return b.b_ml.ml_line_count;
    }
    tv_get_number_chk(tv, None) as crate::pos_defs::LinenrT
}

/// Get the number value of a Vimscript object, interpreted as a
/// boolean (`tv_get_bool`) - literally the same computation as
/// [`tv_get_number_chk`] in the original (not a separate `bool`
/// return type: Vimscript's `varnumber_T` doubles as its boolean
/// representation).
#[must_use]
pub fn tv_get_bool(tv: &TypvalT) -> crate::eval::typval_defs::VarnumberT {
    tv_get_number_chk(tv, None)
}

/// Get the number value of a Vimscript object, interpreted as a
/// boolean, with an error-flag out-parameter (`tv_get_bool_chk`).
#[must_use]
pub fn tv_get_bool_chk(tv: &TypvalT, ret_error: Option<&mut bool>) -> crate::eval::typval_defs::VarnumberT {
    tv_get_number_chk(tv, ret_error)
}

/// Get the float value of `tv` (`tv_get_float`).
///
/// The original's own `emsg(...)` calls reporting exactly which wrong
/// type was found (`Func`/`Partial`/`String`/`List`/`Dict`/`Bool`/
/// `Special`/`Blob`/`Unknown`) are omitted (message display, not
/// tractable) - only the `0.0` fallback result is kept, matching this
/// module's established "skip the display, keep the state" policy.
#[must_use]
pub fn tv_get_float(tv: &TypvalT) -> f64 {
    match tv.value {
        TypvalValue::Number(n) => n as f64,
        TypvalValue::Float(f) => f,
        _ => 0.0,
    }
}

/// Get the float value of `tv`, with an explicit success/failure
/// result (`tv_get_float_chk`). `None` for anything other than a
/// `Number`/`Float` - the original's own `E808: Number or Float
/// required` message is omitted, matching this module's established
/// "skip the display, keep the state" policy (see [`tv_get_float`]'s
/// own doc comment for the same note on its own silent `0.0`
/// fallback).
#[must_use]
pub fn tv_get_float_chk(tv: &TypvalT) -> Option<f64> {
    match tv.value {
        TypvalValue::Number(n) => Some(n as f64),
        TypvalValue::Float(f) => Some(f),
        _ => None,
    }
}

/// Get the string value of a "stringish" Vimscript object
/// (`tv_get_string_chk`).
///
/// Returns `None` on error (`Func`/`Partial`/`List`/`Dict`/`Blob`/
/// `Unknown` - a `Funcref` is deliberately NOT stringified to its
/// name here, matching the original's own `str_errors`-driven
/// `emsg`/`NULL` for that exact case too). The `emsg(_(str_errors[...]))`
/// call itself is omitted - see this module's own "skip the display,
/// keep the state" policy (e.g. `tv_get_number_chk`'s own doc comment).
///
/// Collapses the original's 4-function family
/// (`tv_get_string_chk`/`tv_get_string`/`tv_get_string_buf`/
/// `tv_get_string_buf_chk`) down to 2: the original's `_buf`/`_buf_chk`
/// variants exist purely so the caller can supply their OWN buffer
/// instead of relying on a shared `static char mybuf[NUMBUFLEN]` (whose
/// own doc comment warns it "may be used only once, next call ...
/// may reuse it") - a Rust translation returning a freshly-owned
/// `Vec<u8>` on every call has no such shared-buffer hazard to work
/// around in the first place, so the `_buf` variants would be
/// identical to their non-`_buf` counterparts if translated verbatim;
/// omitted as pure duplication.
#[must_use]
pub fn tv_get_string_chk(tv: &TypvalT) -> Option<Vec<u8>> {
    match &tv.value {
        TypvalValue::Number(n) => Some(n.to_string().into_bytes()),
        TypvalValue::Float(f) => Some(fmt_g(*f)),
        TypvalValue::String(s) => Some(s.clone().unwrap_or_default()),
        TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True) => Some(b"v:true".to_vec()),
        TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::False) => Some(b"v:false".to_vec()),
        TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null) => Some(b"v:null".to_vec()),
        TypvalValue::Partial(_)
        | TypvalValue::Func(_)
        | TypvalValue::List(_)
        | TypvalValue::Dict(_)
        | TypvalValue::Blob(_)
        | TypvalValue::Unknown => None,
    }
}

/// Get the string value of a "stringish" Vimscript object, never
/// returning `None` (`tv_get_string`) - empty bytes instead, on the
/// same errors [`tv_get_string_chk`] itself returns `None` for. See
/// [`tv_get_string_chk`]'s own doc comment for why the original's
/// `_buf` variants aren't translated separately.
#[must_use]
pub fn tv_get_string(tv: &TypvalT) -> Vec<u8> {
    tv_get_string_chk(tv).unwrap_or_default()
}

// Type checks:

/// Check that given value is a number or string (`tv_check_str_or_nr`).
///
/// The original's own `emsg`/`semsg` calls reporting exactly which
/// wrong type was found are omitted (message display, not tractable) -
/// only the boolean result is kept, matching this module's established
/// "skip the display, keep the state" policy.
#[must_use]
pub fn tv_check_str_or_nr(tv: &TypvalT) -> bool {
    matches!(tv.value, TypvalValue::Number(_) | TypvalValue::String(_))
}

/// Check that given value is a number or can be converted to it
/// (`tv_check_num`). Same message-display omission as
/// [`tv_check_str_or_nr`].
#[must_use]
pub fn tv_check_num(tv: &TypvalT) -> bool {
    matches!(
        tv.value,
        TypvalValue::Number(_) | TypvalValue::Bool(_) | TypvalValue::Special(_) | TypvalValue::String(_)
    )
}

/// Check that given value is a Vimscript String or can be "cast" to it
/// (`tv_check_str`). Same message-display omission as
/// [`tv_check_str_or_nr`].
#[must_use]
pub fn tv_check_str(tv: &TypvalT) -> bool {
    matches!(
        tv.value,
        TypvalValue::Number(_)
            | TypvalValue::Bool(_)
            | TypvalValue::Special(_)
            | TypvalValue::String(_)
            | TypvalValue::Float(_)
    )
}

/// Return `true` when `tv` is not falsy: non-zero, non-empty string,
/// non-empty list, etc. - mostly like what JavaScript does, except
/// that an empty list and an empty dictionary are false (`tv2bool`).
///
/// # Safety
/// If `tv`'s value is `List`/`Dict`/`Blob`-typed with a non-null
/// pointer, that pointer must be a valid, live
/// `ListT`/`DictT`/`BlobT`.
#[must_use]
pub unsafe fn tv2bool(tv: &TypvalT) -> bool {
    match &tv.value {
        TypvalValue::Number(n) => *n != 0,
        TypvalValue::Float(f) => *f != 0.0,
        TypvalValue::Partial(p) => !p.is_null(),
        TypvalValue::Func(s) | TypvalValue::String(s) => s.as_ref().is_some_and(|s| !s.is_empty()),
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::List(l) => !l.is_null() && unsafe { tv_list_len(*l) } > 0,
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::Dict(d) => !d.is_null() && tv_dict_len(unsafe { d.as_ref() }) > 0,
        TypvalValue::Bool(b) => *b == crate::eval::typval_defs::BoolVarValue::True,
        TypvalValue::Special(s) => *s != crate::eval::typval_defs::SpecialVarValue::Null,
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::Blob(b) => !b.is_null() && unsafe { tv_blob_len(*b) } > 0,
        TypvalValue::Unknown => false,
    }
}

/// Get a list's lock status, or `VAR_FIXED` for a null list
/// (`tv_list_locked`, `eval/typval.h`'s own `static inline`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live
/// `crate::eval::typval_defs::ListT`.
#[must_use]
pub unsafe fn tv_list_locked(l: *const crate::eval::typval_defs::ListT) -> VarLockStatus {
    if l.is_null() {
        return VarLockStatus::Fixed;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*l).lv_lock }
}

/// Set a list's lock status (`tv_list_set_lock`, `eval/typval.h`'s own
/// `static inline`).
///
/// # Panics
/// If `l` is null and `lock` isn't [`VarLockStatus::Fixed`] - matching
/// the original's own `assert(lock == VAR_FIXED)` for this case (a
/// null list is always, unconditionally "fixed"/immutable, so setting
/// any OTHER lock status on one is a genuine caller-contract
/// violation).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live
/// `crate::eval::typval_defs::ListT`.
pub unsafe fn tv_list_set_lock(l: *mut crate::eval::typval_defs::ListT, lock: VarLockStatus) {
    if l.is_null() {
        assert_eq!(lock, VarLockStatus::Fixed);
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*l).lv_lock = lock };
}

/// Return true if typval is locked (`tv_islocked`).
///
/// # Safety
/// If `tv.value` holds a `List`/`Dict`, that pointer, if non-null, must
/// be a valid pointer to a live `ListT`/`DictT`.
#[must_use]
pub unsafe fn tv_islocked(tv: &TypvalT) -> bool {
    if tv.v_lock == VarLockStatus::Locked {
        return true;
    }
    match &tv.value {
        TypvalValue::List(l) => {
            // SAFETY: forwarded from this function's own safety doc.
            let lock = unsafe { tv_list_locked(*l) };
            lock == VarLockStatus::Locked
        }
        TypvalValue::Dict(d) => {
            // SAFETY: forwarded from this function's own safety doc.
            !d.is_null() && unsafe { &**d }.dv_lock == VarLockStatus::Locked
        }
        _ => false,
    }
}

/// Return true if variable `name` has a locked (immutable) value
/// (`value_check_lock`).
///
/// The original's own `emsg`/`semsg` calls (real, reachable user
/// errors reporting which lock kind was hit) are omitted - message
/// display not tractable - but the exact same `true`/`false` return is
/// kept. `name`/`name_len` only ever affected the omitted message
/// TEXT in the original (never the return value itself), so
/// `name_len`'s `TV_TRANSLATE`/`TV_CSTRING` sentinel-encoding has no
/// meaningful Rust equivalent here and isn't translated; `name` itself
/// is still accepted (unused) to preserve the original's "was a name
/// provided" call-site shape for any future real caller.
#[must_use]
pub fn value_check_lock(lock: VarLockStatus, _name: Option<&[u8]>) -> bool {
    lock != VarLockStatus::Unlocked
}

/// Check that a typval isn't locked, giving an error if it is
/// (`tv_check_lock`). See [`value_check_lock`]'s own doc comment for
/// why `name_len` isn't translated.
///
/// # Safety
/// If `tv.value` holds a `Blob`/`List`/`Dict`, that pointer, if
/// non-null, must be a valid pointer to a live `BlobT`/`ListT`/`DictT`.
#[must_use]
pub unsafe fn tv_check_lock(tv: &TypvalT, name: Option<&[u8]>) -> bool {
    let lock = match &tv.value {
        TypvalValue::Blob(b) => {
            if b.is_null() {
                VarLockStatus::Unlocked
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &**b }.bv_lock
            }
        }
        TypvalValue::List(l) => {
            if l.is_null() {
                VarLockStatus::Unlocked
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &**l }.lv_lock
            }
        }
        TypvalValue::Dict(d) => {
            if d.is_null() {
                VarLockStatus::Unlocked
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &**d }.dv_lock
            }
        }
        _ => VarLockStatus::Unlocked,
    };
    value_check_lock(tv.v_lock, name) || (lock != VarLockStatus::Unlocked && value_check_lock(lock, name))
}

/// Maximum nesting of lists and dicts for [`tv_item_lock`] (`DICT_MAXNEST`).
pub(crate) const DICT_MAXNEST: i32 = 100;

/// Recursion depth counter for [`tv_item_lock`] - matches the
/// original's own function-local `static int recurse`.
static TV_ITEM_LOCK_RECURSE: GlobalCell<i32> = GlobalCell::new(0);

/// Lock or unlock an item, recursively for `deep` levels (`-1` for
/// unlimited) (`tv_item_lock`).
///
/// If `check_refcount` is true, does not lock a list or dict with a
/// reference count greater than 1.
///
/// The original's own `emsg(_(e_variable_nested_too_deep_for_unlock))`
/// (a real, reachable "too deeply nested" error) is omitted - message
/// display not tractable - while the identical early-return control
/// flow (silently giving up once `DICT_MAXNEST` is hit) is kept.
///
/// # Safety
/// If `tv.value` holds a `Blob`/`List`/`Dict` with a non-null pointer,
/// that pointer must be a valid, live `BlobT`/`ListT`/`DictT`,
/// recursively satisfying this same contract for every value it
/// (in)directly contains (its own `li_tv`/`di_tv` entries).
pub unsafe fn tv_item_lock(tv: &mut TypvalT, deep: i32, lock: bool, check_refcount: bool) {
    // TODO(ZyX-I): Make this not recursive (matches the original's own
    // TODO comment).
    // SAFETY: TV_ITEM_LOCK_RECURSE is a private, crate-internal
    // GlobalCell only ever touched by this function.
    let recurse = unsafe { *TV_ITEM_LOCK_RECURSE.get_mut() };
    if recurse >= DICT_MAXNEST {
        // emsg(_(e_variable_nested_too_deep_for_unlock)) omitted - see
        // this function's own doc comment.
        return;
    }
    if deep == 0 {
        return;
    }
    unsafe { *TV_ITEM_LOCK_RECURSE.get_mut() += 1 };

    // lock/unlock the item itself
    let change_lock = |cur: VarLockStatus| -> VarLockStatus {
        match cur {
            VarLockStatus::Unlocked | VarLockStatus::Locked => {
                if lock {
                    VarLockStatus::Locked
                } else {
                    VarLockStatus::Unlocked
                }
            }
            VarLockStatus::Fixed => VarLockStatus::Fixed,
        }
    };
    tv.v_lock = change_lock(tv.v_lock);

    match &tv.value {
        TypvalValue::Blob(b) => {
            let b = *b;
            if !b.is_null() {
                // SAFETY: forwarded from this function's own safety doc.
                let refcount = unsafe { (*b).bv_refcount };
                if !(check_refcount && refcount > 1) {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (*b).bv_lock = change_lock((*b).bv_lock) };
                }
            }
        }
        TypvalValue::List(l) => {
            let l = *l;
            if !l.is_null() {
                // SAFETY: forwarded from this function's own safety doc.
                let refcount = unsafe { (*l).lv_refcount };
                if !(check_refcount && refcount > 1) {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (*l).lv_lock = change_lock((*l).lv_lock) };
                    if !(0..=1).contains(&deep) {
                        // Recursive: lock/unlock the items the List contains.
                        // SAFETY: forwarded from this function's own safety doc.
                        let mut item = unsafe { (*l).lv_first };
                        while !item.is_null() {
                            // SAFETY: forwarded from this function's own safety doc.
                            unsafe { tv_item_lock(&mut (*item).li_tv, deep - 1, lock, check_refcount) };
                            // SAFETY: forwarded from this function's own safety doc.
                            item = unsafe { (*item).li_next };
                        }
                    }
                }
            }
        }
        TypvalValue::Dict(d) => {
            let d = *d;
            if !d.is_null() {
                // SAFETY: forwarded from this function's own safety doc.
                let refcount = unsafe { (*d).dv_refcount };
                if !(check_refcount && refcount > 1) {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (*d).dv_lock = change_lock((*d).dv_lock) };
                    if !(0..=1).contains(&deep) {
                        // recursive: lock/unlock the items the Dict contains
                        // SAFETY: forwarded from this function's own safety doc.
                        let items: Vec<*mut DictitemT> =
                            unsafe { (*d).dv_index.values().copied().collect() };
                        for di in items {
                            // SAFETY: forwarded from this function's own safety doc.
                            unsafe { tv_item_lock(&mut (*di).di_tv, deep - 1, lock, check_refcount) };
                        }
                    }
                }
            }
        }
        TypvalValue::Number(_)
        | TypvalValue::Float(_)
        | TypvalValue::String(_)
        | TypvalValue::Func(_)
        | TypvalValue::Partial(_)
        | TypvalValue::Bool(_)
        | TypvalValue::Special(_) => {}
        TypvalValue::Unknown => unreachable!("tv_item_lock called with an Unknown tv"),
    }

    unsafe { *TV_ITEM_LOCK_RECURSE.get_mut() -= 1 };
}

// Argument type checks (used by builtin Vimscript function
// implementations to validate their own arguments; `args[idx]`
// indexing becomes a plain Rust slice index - `eval/typval.c`'s
// `tv_check_for_*_arg` family). Every real `emsg`/`semsg` call
// reporting exactly which argument/expected-type mismatched is
// omitted (message display, not tractable) - only the `OK`/`FAIL`
// result is kept, matching this module's established policy.

/// Give an error and return `FAIL` unless `args[idx]` is a string
/// (`tv_check_for_string_arg`).
#[must_use]
pub fn tv_check_for_string_arg(args: &[TypvalT], idx: usize) -> i32 {
    if !matches!(args[idx].value, TypvalValue::String(_)) {
        return FAIL;
    }
    OK
}

/// Give an error and return `FAIL` unless `args[idx]` is a non-empty
/// string (`tv_check_for_nonempty_string_arg`).
#[must_use]
pub fn tv_check_for_nonempty_string_arg(args: &[TypvalT], idx: usize) -> i32 {
    if tv_check_for_string_arg(args, idx) == FAIL {
        return FAIL;
    }
    let TypvalValue::String(s) = &args[idx].value else { unreachable!() };
    if s.as_deref().unwrap_or(&[]).is_empty() {
        return FAIL;
    }
    OK
}

/// Check for an optional string argument at `idx`
/// (`tv_check_for_opt_string_arg`).
#[must_use]
pub fn tv_check_for_opt_string_arg(args: &[TypvalT], idx: usize) -> i32 {
    if matches!(args[idx].value, TypvalValue::Unknown) || tv_check_for_string_arg(args, idx) != FAIL {
        OK
    } else {
        FAIL
    }
}

/// Give an error and return `FAIL` unless `args[idx]` is a number
/// (`tv_check_for_number_arg`).
#[must_use]
pub fn tv_check_for_number_arg(args: &[TypvalT], idx: usize) -> i32 {
    if !matches!(args[idx].value, TypvalValue::Number(_)) {
        return FAIL;
    }
    OK
}

/// Check for an optional number argument at `idx`
/// (`tv_check_for_opt_number_arg`).
#[must_use]
pub fn tv_check_for_opt_number_arg(args: &[TypvalT], idx: usize) -> i32 {
    if matches!(args[idx].value, TypvalValue::Unknown) || tv_check_for_number_arg(args, idx) != FAIL {
        OK
    } else {
        FAIL
    }
}

/// Give an error and return `FAIL` unless `args[idx]` is a float or a
/// number (`tv_check_for_float_or_nr_arg`).
#[must_use]
pub fn tv_check_for_float_or_nr_arg(args: &[TypvalT], idx: usize) -> i32 {
    if !matches!(args[idx].value, TypvalValue::Float(_) | TypvalValue::Number(_)) {
        return FAIL;
    }
    OK
}

/// Give an error and return `FAIL` unless `args[idx]` is a bool
/// (`tv_check_for_bool_arg`). A plain `Number` holding `0` or `1` is
/// also accepted, matching the original's own leniency.
#[must_use]
pub fn tv_check_for_bool_arg(args: &[TypvalT], idx: usize) -> i32 {
    let ok = matches!(args[idx].value, TypvalValue::Bool(_))
        || matches!(args[idx].value, TypvalValue::Number(0 | 1));
    if !ok {
        return FAIL;
    }
    OK
}

/// Check for an optional bool argument at `idx`
/// (`tv_check_for_opt_bool_arg`).
#[must_use]
pub fn tv_check_for_opt_bool_arg(args: &[TypvalT], idx: usize) -> i32 {
    if matches!(args[idx].value, TypvalValue::Unknown) {
        return OK;
    }
    tv_check_for_bool_arg(args, idx)
}

/// Give an error and return `FAIL` unless `args[idx]` is a blob
/// (`tv_check_for_blob_arg`).
#[must_use]
pub fn tv_check_for_blob_arg(args: &[TypvalT], idx: usize) -> i32 {
    if !matches!(args[idx].value, TypvalValue::Blob(_)) {
        return FAIL;
    }
    OK
}

/// Give an error and return `FAIL` unless `args[idx]` is a list
/// (`tv_check_for_list_arg`).
#[must_use]
pub fn tv_check_for_list_arg(args: &[TypvalT], idx: usize) -> i32 {
    if !matches!(args[idx].value, TypvalValue::List(_)) {
        return FAIL;
    }
    OK
}

/// Give an error and return `FAIL` unless `args[idx]` is a dict
/// (`tv_check_for_dict_arg`).
#[must_use]
pub fn tv_check_for_dict_arg(args: &[TypvalT], idx: usize) -> i32 {
    if !matches!(args[idx].value, TypvalValue::Dict(_)) {
        return FAIL;
    }
    OK
}

/// Give an error and return `FAIL` unless `args[idx]` is a non-`NULL`
/// dict (`tv_check_for_nonnull_dict_arg`).
#[must_use]
pub fn tv_check_for_nonnull_dict_arg(args: &[TypvalT], idx: usize) -> i32 {
    if tv_check_for_dict_arg(args, idx) == FAIL {
        return FAIL;
    }
    let TypvalValue::Dict(d) = args[idx].value else { unreachable!() };
    if d.is_null() {
        return FAIL;
    }
    OK
}

/// Check for an optional dict argument at `idx`
/// (`tv_check_for_opt_dict_arg`).
#[must_use]
pub fn tv_check_for_opt_dict_arg(args: &[TypvalT], idx: usize) -> i32 {
    if matches!(args[idx].value, TypvalValue::Unknown) || tv_check_for_dict_arg(args, idx) != FAIL {
        OK
    } else {
        FAIL
    }
}

/// Give an error and return `FAIL` unless `args[idx]` is a string or a
/// number (`tv_check_for_string_or_number_arg`).
#[must_use]
pub fn tv_check_for_string_or_number_arg(args: &[TypvalT], idx: usize) -> i32 {
    if !matches!(args[idx].value, TypvalValue::String(_) | TypvalValue::Number(_)) {
        return FAIL;
    }
    OK
}

/// Give an error and return `FAIL` unless `args[idx]` is a buffer
/// number (a number or a string) (`tv_check_for_buffer_arg`).
#[must_use]
pub fn tv_check_for_buffer_arg(args: &[TypvalT], idx: usize) -> i32 {
    tv_check_for_string_or_number_arg(args, idx)
}

/// Give an error and return `FAIL` unless `args[idx]` is a line
/// number (a number or a string) (`tv_check_for_lnum_arg`).
#[must_use]
pub fn tv_check_for_lnum_arg(args: &[TypvalT], idx: usize) -> i32 {
    tv_check_for_string_or_number_arg(args, idx)
}

/// Give an error and return `FAIL` unless `args[idx]` is a string or a
/// list (`tv_check_for_string_or_list_arg`).
#[must_use]
pub fn tv_check_for_string_or_list_arg(args: &[TypvalT], idx: usize) -> i32 {
    if !matches!(args[idx].value, TypvalValue::String(_) | TypvalValue::List(_)) {
        return FAIL;
    }
    OK
}

/// Give an error and return `FAIL` unless `args[idx]` is a string, a
/// list, or a blob (`tv_check_for_string_or_list_or_blob_arg`).
#[must_use]
pub fn tv_check_for_string_or_list_or_blob_arg(args: &[TypvalT], idx: usize) -> i32 {
    if !matches!(
        args[idx].value,
        TypvalValue::String(_) | TypvalValue::List(_) | TypvalValue::Blob(_)
    ) {
        return FAIL;
    }
    OK
}

/// Check for an optional string or list argument at `idx`
/// (`tv_check_for_opt_string_or_list_arg`).
#[must_use]
pub fn tv_check_for_opt_string_or_list_arg(args: &[TypvalT], idx: usize) -> i32 {
    if matches!(args[idx].value, TypvalValue::Unknown) || tv_check_for_string_or_list_arg(args, idx) != FAIL {
        OK
    } else {
        FAIL
    }
}

/// Give an error and return `FAIL` unless `args[idx]` is a string or a
/// function reference (`tv_check_for_string_or_func_arg`).
#[must_use]
pub fn tv_check_for_string_or_func_arg(args: &[TypvalT], idx: usize) -> i32 {
    if !matches!(
        args[idx].value,
        TypvalValue::Partial(_) | TypvalValue::Func(_) | TypvalValue::String(_)
    ) {
        return FAIL;
    }
    OK
}

/// Give an error and return `FAIL` unless `args[idx]` is a list or a
/// blob (`tv_check_for_list_or_blob_arg`).
#[must_use]
pub fn tv_check_for_list_or_blob_arg(args: &[TypvalT], idx: usize) -> i32 {
    if !matches!(args[idx].value, TypvalValue::List(_) | TypvalValue::Blob(_)) {
        return FAIL;
    }
    OK
}

/// Formats `value` matching C's `printf("%g", value)` with the
/// default precision of 6 significant digits - the ONLY precision
/// `typval.c` itself ever uses (`vim_snprintf(buf, NUMBUFLEN, "%g",
/// tv->vval.v_float)` inside the original's `tv_get_string_buf_chk`).
/// A narrow, purpose-built helper for that one call site, NOT a
/// generic `%g`/`vim_snprintf` implementation - the real, fully
/// general `vim_snprintf` (positional `$`-style arguments, arbitrary
/// precision/width/flags) remains its own separate, substantial
/// undertaking, see `strings.rs`'s own module doc.
///
/// Algorithm (matching the C standard's own `%g` specification):
/// round `value` to 6 significant digits via scientific notation
/// FIRST, and determine its decimal exponent X from that ALREADY-
/// ROUNDED value, not the original one (e.g. `9999999.0` rounds to
/// `1.00000e7`, giving X=7, not 6 - this changes which of the two
/// styles below is used, and was verified against real glibc `printf`
/// output: `9999999.0` prints as `"1e+07"`, not `"1e+06"` or
/// `"9999999"`). Then:
/// - if `-4 <= X < 6`: fixed-point notation with `6 - 1 - X` digits
///   after the decimal point, then strip trailing zeros (and a
///   trailing `.` if no fractional digits remain).
/// - otherwise: scientific notation with 5 digits after the decimal
///   point in the mantissa, strip trailing zeros from the mantissa
///   (and a trailing `.` if none remain), and format the exponent as
///   `e+NN`/`e-NN` (at least 2 digits, matching glibc).
///
/// Verified against 68 real `gcc`/glibc `printf("%g", ...)` reference
/// outputs before being added here, including the trickiest cases:
/// exact halfway-rounding ties whose true binary value is actually
/// just below or above the apparent decimal boundary (e.g. the
/// literal `1.999995` prints as `"1.99999"`, not `"2"`, because its
/// nearest `f64` is very slightly less than the exact decimal
/// `1.999995`), and exponent-carry-on-rounding (`9999999.0` above).
pub(crate) fn fmt_g(value: f64) -> Vec<u8> {
    if value == 0.0 {
        // Preserve the sign of zero, matching glibc (`-0.0` -> `"-0"`).
        return if value.is_sign_negative() { b"-0".to_vec() } else { b"0".to_vec() };
    }
    if value.is_nan() {
        return b"nan".to_vec();
    }
    if value.is_infinite() {
        return if value < 0.0 { b"-inf".to_vec() } else { b"inf".to_vec() };
    }

    const PRECISION: i32 = 6;
    // Round to PRECISION significant digits via scientific notation
    // first, so the exponent used for the fixed-vs-scientific decision
    // below reflects the ROUNDED value, matching glibc exactly (see
    // this function's own doc comment).
    let sci = format!("{value:.*e}", (PRECISION - 1) as usize);
    let (mantissa, exp_str) = sci.split_once('e').expect("Rust's {:e} format always includes 'e'");
    let exponent: i32 = exp_str.parse().expect("Rust's {:e} exponent is always a valid integer");

    if (-4..PRECISION).contains(&exponent) {
        let decimals = (PRECISION - 1 - exponent).max(0) as usize;
        let fixed = format!("{value:.decimals$}");
        strip_trailing_zeros(&fixed)
    } else {
        let mut out = strip_trailing_zeros(mantissa);
        out.push(b'e');
        out.push(if exponent >= 0 { b'+' } else { b'-' });
        out.extend(format!("{:02}", exponent.abs()).into_bytes());
        out
    }
}

/// Strips trailing zeros from a formatted decimal number's fractional
/// part (and the trailing `.` itself, if no fractional digits remain),
/// e.g. `"1.50000"` -> `"1.5"`, `"100.000"` -> `"100"`. Only used by
/// [`fmt_g`], matching `%g`'s own "unlike `%f`/`%e`, strip trailing
/// zeros" behavior (the C `#` flag, if given, would disable this -
/// never given at [`fmt_g`]'s one call site, so not modeled).
fn strip_trailing_zeros(s: &str) -> Vec<u8> {
    if !s.contains('.') {
        return s.as_bytes().to_vec();
    }
    s.trim_end_matches('0').trim_end_matches('.').as_bytes().to_vec()
}


/// Release whatever a single value directly owns or references. Container
/// unrefs that reach zero are drained by [`FreeWorklist`], so arbitrarily
/// deep List/Dict/Partial graphs do not recurse on the native stack. Used by
/// [`tv_dict_item_free`]/[`partial_free`]'s own `pt_argv` release, and
/// by [`partial_unref`] for `pt_dict`/`pt_func`.
///
/// `pub(crate)` (not fully private) so `eval/eval.rs`'s
/// `eval_concat_str` can reuse it too, for the same "release whatever
/// `tv1` used to hold before overwriting it with a freshly-computed
/// result" need `tv_dict_item_free`/`partial_free` already have -
/// unlike `eval_addblob`, which already statically knows its operand
/// is `Blob`-typed and so can call `tv_blob_unref` directly,
/// `eval_concat_str` may see tv1 holding ANY type (only tv2 is
/// constrained to be stringifiable), so it genuinely needs this
/// generic dispatch.
///
/// # Safety
/// If `tv`'s value is `List`/`Dict`/`Blob`/`Partial`-typed with a
/// non-null pointer, that pointer must be valid (matching every other
/// function in this crate that touches those types).
pub(crate) unsafe fn tv_clear_simple(tv: &TypvalT) {
    match &tv.value {
        TypvalValue::List(l) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_list_unref(*l) };
        }
        TypvalValue::Dict(d) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_dict_unref(*d) };
        }
        TypvalValue::Blob(b) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_blob_unref(*b) };
        }
        TypvalValue::Partial(p) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { partial_unref(*p) };
        }
        TypvalValue::Func(name) => {
            // `case VAR_FUNC: func_unref(tv->vval.v_string); FALLTHROUGH;`
            // in the original - the FALLTHROUGH into VAR_STRING's
            // `xfree` needs no equivalent here, Rust's own ownership
            // drops the owned `Vec<u8>` naturally.
            crate::eval::userfunc::func_unref(name.as_deref());
        }
        TypvalValue::Unknown
        | TypvalValue::Number(_)
        | TypvalValue::Float(_)
        | TypvalValue::Bool(_)
        | TypvalValue::Special(_)
        | TypvalValue::String(_) => {
            // Rust's own ownership drops String's owned Vec<u8>
            // naturally - no manual xfree needed, unlike the original.
        }
    }
}

#[derive(Clone, Copy)]
enum FreeTarget {
    List(*mut ListT, bool),
    Dict(*mut DictT, bool),
    Partial(*mut PartialT),
}

/// Heap-backed destruction stack shared by List, Dict, and Partial values.
///
/// `deallocating` prevents a container being destroyed from being
/// rescheduled through a self-reference or cycle. `protected` identifies
/// contents-only roots: back-references still decrement their refcount but
/// may not deallocate the root itself.
struct FreeWorklist {
    targets: Vec<FreeTarget>,
    deallocating: std::collections::HashSet<(u8, usize)>,
    protected: std::collections::HashSet<(u8, usize)>,
}

impl FreeWorklist {
    fn new(target: FreeTarget) -> Self {
        let mut work = Self {
            targets: Vec::new(),
            deallocating: std::collections::HashSet::new(),
            protected: std::collections::HashSet::new(),
        };
        work.push(target);
        work
    }

    fn key(target: FreeTarget) -> (u8, usize) {
        match target {
            FreeTarget::List(list, _) => (0, list as usize),
            FreeTarget::Dict(dict, _) => (1, dict as usize),
            FreeTarget::Partial(partial) => (2, partial as usize),
        }
    }

    fn push(&mut self, target: FreeTarget) {
        let key = Self::key(target);
        let inserted = match target {
            FreeTarget::List(_, false)
            | FreeTarget::Dict(_, false) => self.protected.insert(key),
            FreeTarget::List(_, true)
            | FreeTarget::Dict(_, true)
            | FreeTarget::Partial(_) => self.deallocating.insert(key),
        };
        if inserted {
            self.targets.push(target);
        }
    }

    unsafe fn unref_list(&mut self, list: *mut ListT) {
        if list.is_null() {
            return;
        }
        let key = (0, list as usize);
        if self.deallocating.contains(&key) {
            return;
        }
        unsafe { (*list).lv_refcount -= 1 };
        if unsafe { (*list).lv_refcount } <= 0
            && !self.protected.contains(&key)
        {
            self.push(FreeTarget::List(list, true));
        }
    }

    unsafe fn unref_dict(&mut self, dict: *mut DictT) {
        if dict.is_null() {
            return;
        }
        let key = (1, dict as usize);
        if self.deallocating.contains(&key) {
            return;
        }
        unsafe { (*dict).dv_refcount -= 1 };
        if unsafe { (*dict).dv_refcount } <= 0
            && !self.protected.contains(&key)
        {
            self.push(FreeTarget::Dict(dict, true));
        }
    }

    unsafe fn unref_partial(&mut self, partial: *mut PartialT) {
        if partial.is_null() {
            return;
        }
        let key = (2, partial as usize);
        if self.deallocating.contains(&key) {
            return;
        }
        unsafe { (*partial).pt_refcount -= 1 };
        if unsafe { (*partial).pt_refcount } <= 0 {
            self.push(FreeTarget::Partial(partial));
        }
    }

    unsafe fn clear_value(&mut self, value: &TypvalT) {
        match &value.value {
            TypvalValue::List(list) => unsafe { self.unref_list(*list) },
            TypvalValue::Dict(dict) => unsafe { self.unref_dict(*dict) },
            TypvalValue::Partial(partial) => {
                unsafe { self.unref_partial(*partial) };
            }
            _ => unsafe { tv_clear_simple(value) },
        }
    }

    unsafe fn free_list_contents(
        &mut self,
        list: *mut ListT,
        free_self: bool,
    ) {
        let mut item = unsafe { (*list).lv_first };
        while !item.is_null() {
            let next = unsafe { (*item).li_next };
            unsafe { (*list).lv_first = next };
            unsafe { self.clear_value(&(*item).li_tv) };
            drop(unsafe { Box::from_raw(item) });
            item = next;
        }
        let list_ref = unsafe { &mut *list };
        list_ref.lv_len = 0;
        list_ref.lv_idx_item = std::ptr::null_mut();
        list_ref.lv_last = std::ptr::null_mut();
        debug_assert!(
            list_ref.lv_watch.is_null(),
            "tv_list_free_contents: lv_watch should be empty"
        );
        if free_self {
            unsafe { tv_list_free_list(list) };
        }
    }

    unsafe fn free_dict_contents(
        &mut self,
        dict: *mut DictT,
        free_self: bool,
    ) {
        let dict_ref = unsafe { &mut *dict };
        let items: Vec<*mut DictitemT> =
            std::mem::take(&mut dict_ref.dv_index)
                .into_values()
                .collect();
        for watcher in &mut dict_ref.watchers {
            tv_dict_watcher_free(watcher);
        }
        dict_ref.watchers.clear();
        dict_ref.dv_hashtab = crate::hashtab_defs::HashtabT::hash_init();
        for item in items {
            unsafe { self.clear_value(&(*item).di_tv) };
            if unsafe { (*item).di_flags } & dict_item_flags::ALLOC != 0 {
                drop(unsafe { Box::from_raw(item) });
            } else {
                unsafe { (*item).di_tv = TypvalT::default() };
            }
        }
        if free_self {
            unsafe { tv_dict_free_dict(dict) };
        }
    }

    unsafe fn free_partial(&mut self, partial: *mut PartialT) {
        let boxed = unsafe { Box::from_raw(partial) };
        for argument in &boxed.pt_argv {
            unsafe { self.clear_value(argument) };
        }
        unsafe { self.unref_dict(boxed.pt_dict) };
        if boxed.pt_name.is_none() {
            unsafe {
                crate::eval::userfunc::func_ptr_unref(boxed.pt_func)
            };
        } else {
            crate::eval::userfunc::func_unref(boxed.pt_name.as_deref());
        }
    }

    unsafe fn run(&mut self) {
        while let Some(target) = self.targets.pop() {
            match target {
                FreeTarget::List(list, free_self) => {
                    unsafe { self.free_list_contents(list, free_self) };
                }
                FreeTarget::Dict(dict, free_self) => {
                    unsafe { self.free_dict_contents(dict, free_self) };
                }
                FreeTarget::Partial(partial) => {
                    unsafe { self.free_partial(partial) };
                }
            }
        }
    }
}

unsafe fn free_targets(target: FreeTarget) {
    let mut work = FreeWorklist::new(target);
    unsafe { work.run() };
}

/// Free a partial, releasing everything it owns (`partial_free`,
/// `eval.c` - kept here alongside this module's other `tv_*_unref`/
/// `_free` functions since it's small, self-contained, and exactly
/// analogous in shape to [`tv_dict_free`]/[`tv_list_free`], even
/// though `partial_T`'s real home is `eval.c`, not `eval/typval.c`).
///
/// # Safety
/// `pt` must be a valid, non-null pointer previously allocated via
/// `Box::into_raw` (nothing in this crate currently allocates a real
/// `PartialT` this way yet - every current use is a hand-built value
/// in a test - but this matches the ownership convention every other
/// heap-allocated type in this module already uses). If
/// `(*pt).pt_dict` is non-null, it must be a valid pointer to a live
/// `DictT`; if `(*pt).pt_func` is non-null (and `pt_name` is absent),
/// it must be a valid pointer to a live `UfuncT`.
unsafe fn partial_free(pt: *mut PartialT) {
    unsafe { free_targets(FreeTarget::Partial(pt)) };
}

/// Unreference a partial: decrement the reference count and free it
/// once it reaches zero (`partial_unref`, `eval.c`).
///
/// # Safety
/// Same as `partial_free` (this module's own private helper) whenever
/// `pt` is non-null; a null `pt` is always a safe no-op (matching the
/// original).
pub unsafe fn partial_unref(pt: *mut PartialT) {
    if pt.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*pt).pt_refcount -= 1 };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*pt).pt_refcount } <= 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { partial_free(pt) };
    }
}

/// Convert a `typval_T` into a [`Callback`] (`callback_from_typval`,
/// `eval.c`) - used by `prompt_setcallback()`/`prompt_setinterrupt()`
/// and the (not yet translated) dict-watcher subsystem.
///
/// The original's own `nlua_is_table_from_lua` branch (a Lua-table
/// value used as a callable) is NOT modeled: this crate's own
/// `TypvalValue` enum has no Lua-table variant at all (no Lua host is
/// embedded yet), so that branch is provably, always unreachable here
/// - not merely "the common case."
///
/// The original's own `emsg("E921: ...")` display (on failure) is
/// omitted, matching this module's established "skip the message,
/// keep the exact same success/failure result" policy. Returns
/// `Option<Callback>` rather than the original's own bare `bool` (or a
/// `Result<Callback, ()>` with no real error payload to carry) - `None`
/// is this crate's own idiomatic "failed, no further detail" shape.
///
/// # Safety
/// If `arg.value` is `Partial`-typed with a non-null pointer, that
/// pointer must be valid.
pub unsafe fn callback_from_typval(arg: &TypvalT) -> Option<Callback> {
    match &arg.value {
        TypvalValue::Partial(pt) if !pt.is_null() => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (**pt).pt_refcount += 1 };
            Some(Callback::Partial(*pt))
        }
        TypvalValue::String(Some(s))
            if s.first().is_some_and(|b| crate::ascii_defs::ascii_isdigit(i32::from(*b))) =>
        {
            None
        }
        TypvalValue::Func(Some(name)) => {
            if name.is_empty() {
                Some(Callback::None)
            } else {
                // Unlike the `String` case below, a `Func` value's own
                // name is never re-resolved via `get_scriptlocal_funcname`
                // - matching the original's own `if (arg->v_type ==
                // VAR_STRING) { ... get_scriptlocal_funcname(name) ...
                // }` guard, which only fires for `VAR_STRING`.
                crate::eval::userfunc::func_ref(Some(name));
                Some(Callback::Funcref(name.clone()))
            }
        }
        TypvalValue::String(Some(name)) => {
            if name.is_empty() {
                Some(Callback::None)
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                let funcref = unsafe { crate::eval::userfunc::get_scriptlocal_funcname(name) }.unwrap_or_else(|| name.clone());
                crate::eval::userfunc::func_ref(Some(&funcref));
                Some(Callback::Funcref(funcref))
            }
        }
        TypvalValue::Func(None) | TypvalValue::String(None) => None,
        TypvalValue::Special(_) => Some(Callback::None),
        TypvalValue::Number(0) => Some(Callback::None),
        _ => None,
    }
}

/// Free/unreference a [`Callback`]'s own held resource (`callback_free`,
/// `eval/typval.c`) - `Lua` is a documented, narrow, unreachable gap
/// (needs the Lua host's own `NLUA_CLEAR_REF`, not translated; nothing
/// in this crate can construct a real `Callback::Lua` value yet).
pub fn callback_free(callback: &mut Callback) {
    match callback {
        Callback::Funcref(name) => crate::eval::userfunc::func_unref(Some(name)),
        // SAFETY: `pt`, if non-null, must be a valid pointer previously
        // returned by a real allocation - guaranteed by every real
        // constructor of a `Callback::Partial` in this crate
        // (`callback_from_typval` above bumps `pt_refcount` on an
        // ALREADY-live `PartialT`, never allocates a fresh dangling one).
        Callback::Partial(pt) => unsafe { partial_unref(*pt) },
        Callback::Lua(_) => unimplemented!("callback_free: Lua callbacks need the Lua host, not yet translated"),
        Callback::None => {}
    }
    *callback = Callback::None;
}

/// Copy a callback, incrementing its held reference
/// (`callback_copy`).
///
/// The destination must already be cleared, matching the original
/// helper's contract. Lua callbacks remain deferred pending
/// `api_new_luaref`.
///
/// # Safety
/// A partial callback must contain a valid live pointer.
pub unsafe fn callback_copy(dest: &mut Callback, src: &Callback) {
    *dest = match src {
        Callback::Partial(partial) => {
            assert!(!partial.is_null(), "callback_copy: null partial");
            unsafe { (**partial).pt_refcount += 1 };
            Callback::Partial(*partial)
        }
        Callback::Funcref(name) => {
            crate::eval::userfunc::func_ref(Some(name));
            Callback::Funcref(name.clone())
        }
        Callback::Lua(_) => {
            unimplemented!("callback_copy: Lua callbacks need api_new_luaref")
        }
        Callback::None => Callback::None,
    };
}

/// Copy a callback into a typval, incrementing held references
/// (`callback_put`).
///
/// # Safety
/// A non-null partial pointer must point to a live [`PartialT`], and
/// function-registry state must not be mutated concurrently.
pub unsafe fn callback_put(callback: &Callback, tv: &mut TypvalT) {
    tv.value = match callback {
        Callback::Partial(partial) => {
            if !partial.is_null() {
                unsafe { (**partial).pt_refcount += 1 };
            }
            TypvalValue::Partial(*partial)
        }
        Callback::Funcref(name) => {
            crate::eval::userfunc::func_ref(Some(name));
            TypvalValue::Func(Some(name.clone()))
        }
        Callback::Lua(_) | Callback::None => {
            TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null)
        }
    };
}

/// Generate a textual callback description (`callback_to_string`).
///
/// The original writes into a 100-byte buffer, leaving room for its
/// trailing NUL; the returned logical bytes are therefore truncated to
/// at most 99 bytes. Lua callbacks remain at the Lua-host boundary.
///
/// # Safety
/// A Partial callback must contain a valid, live pointer.
#[must_use]
pub unsafe fn callback_to_string(callback: &Callback) -> Vec<u8> {
    let mut result = match callback {
        Callback::Funcref(name) => {
            let len = name
                .iter()
                .position(|&byte| byte == crate::ascii_defs::NUL)
                .unwrap_or(name.len());
            let mut text = b"<vim function: ".to_vec();
            text.extend_from_slice(&name[..len]);
            text.push(b'>');
            text
        }
        Callback::Partial(partial) => {
            assert!(!partial.is_null(), "callback_to_string: null partial");
            // SAFETY: forwarded from this function's own safety doc.
            let name = unsafe { &(**partial).pt_name };
            let name = name.as_deref().unwrap_or_default();
            let len = name
                .iter()
                .position(|&byte| byte == crate::ascii_defs::NUL)
                .unwrap_or(name.len());
            let mut text = b"<vim partial: ".to_vec();
            text.extend_from_slice(&name[..len]);
            text.push(b'>');
            text
        }
        Callback::Lua(_) => {
            unimplemented!(
                "callback_to_string: Lua callbacks need nlua_funcref_str"
            )
        }
        Callback::None => Vec::new(),
    };
    result.truncate(99);
    result
}

/// Whether two callbacks are equal (`tv_callback_equal`).
///
/// [`Callback`]'s derived equality exactly matches the original:
/// function-name bytes, Partial pointer identity, LuaRef value, or two
/// None callbacks.
#[must_use]
pub fn tv_callback_equal(cb1: &Callback, cb2: &Callback) -> bool {
    cb1 == cb2
}

fn tv_dict_watcher_free(
    watcher: &mut crate::eval::typval_defs::DictWatcher,
) {
    callback_free(&mut watcher.callback);
}

/// Add a key watcher to a Dictionary (`tv_dict_watcher_add`).
///
/// A null Dictionary is a no-op, matching the original.
///
/// # Safety
/// `dict`, if non-null, must point to a live Dictionary. The callback
/// must own the reference established by [`callback_from_typval`] or
/// [`callback_copy`].
pub unsafe fn tv_dict_watcher_add(
    dict: *mut DictT,
    key_pattern: &[u8],
    callback: Callback,
) {
    if dict.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut (*dict).watchers }.push(Box::new(
        crate::eval::typval_defs::DictWatcher {
            callback,
            key_pattern: key_pattern.to_vec(),
            busy: false,
            needs_free: false,
        },
    ));
}

/// Remove a matching watcher from a Dictionary
/// (`tv_dict_watcher_remove`).
///
/// Removal is deferred when any watcher encountered before the match is
/// busy, preserving the original queue walk's behavior.
///
/// # Safety
/// `dict`, if non-null, must point to a live Dictionary.
pub unsafe fn tv_dict_watcher_remove(
    dict: *mut DictT,
    key_pattern: &[u8],
    callback: &Callback,
) -> bool {
    if dict.is_null() {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let watchers = unsafe { &mut (*dict).watchers };
    let mut queue_is_busy = false;
    let mut found = None;
    for (idx, watcher) in watchers.iter().enumerate() {
        if watcher.busy {
            queue_is_busy = true;
        }
        if tv_callback_equal(&watcher.callback, callback)
            && watcher.key_pattern == key_pattern
        {
            found = Some(idx);
            break;
        }
    }

    let Some(idx) = found else {
        return false;
    };
    if queue_is_busy {
        watchers[idx].needs_free = true;
    } else {
        let mut watcher = watchers.remove(idx);
        tv_dict_watcher_free(&mut watcher);
    }
    true
}

/// Whether `key` matches a watcher's exact/prefix pattern
/// (`tv_dict_watcher_matches`).
#[must_use]
pub fn tv_dict_watcher_matches(
    watcher: &crate::eval::typval_defs::DictWatcher,
    key: &[u8],
) -> bool {
    if watcher.key_pattern.last() == Some(&b'*') {
        key.starts_with(
            &watcher.key_pattern[..watcher.key_pattern.len() - 1],
        )
    } else {
        key == watcher.key_pattern
    }
}

/// Whether a Dictionary has any key watchers (`tv_dict_is_watched`).
///
/// # Safety
/// `dict`, if non-null, must point to a live Dictionary.
#[must_use]
pub unsafe fn tv_dict_is_watched(dict: *const DictT) -> bool {
    !dict.is_null() && !unsafe { &(*dict).watchers }.is_empty()
}

/// Notify matching Dictionary watchers of a key change
/// (`tv_dict_watcher_notify`).
///
/// `newtv`/`oldtv` populate the callback's change Dictionary as
/// `"new"`/`"old"`. Busy watchers are skipped, and removals requested
/// while any callback is active are freed after iteration.
///
/// # Safety
/// `dict` must point to a live Dictionary. Every supplied typval and
/// watcher callback must satisfy [`tv_copy`]/
/// [`crate::eval::eval::callback_call`]'s contracts.
pub unsafe fn tv_dict_watcher_notify(
    dict: *mut DictT,
    key: &[u8],
    newtv: Option<&TypvalT>,
    oldtv: Option<&TypvalT>,
) {
    debug_assert!(!dict.is_null());
    let changes = tv_dict_alloc();
    // SAFETY: `changes` was just allocated above.
    unsafe { (*changes).dv_refcount += 1 };
    if let Some(newtv) = newtv {
        let item = tv_dict_item_alloc(b"new");
        // SAFETY: item was just allocated and newtv is valid by the
        // function contract.
        unsafe { tv_copy(newtv, &mut (*item).di_tv) };
        unsafe { tv_dict_add(&mut *changes, item) };
    }
    if let Some(oldtv) = oldtv
        && !matches!(oldtv.value, TypvalValue::Unknown)
    {
        let item = tv_dict_item_alloc(b"old");
        // SAFETY: as above.
        unsafe { tv_copy(oldtv, &mut (*item).di_tv) };
        unsafe { tv_dict_add(&mut *changes, item) };
    }

    let argv = [
        TypvalT {
            value: TypvalValue::Dict(dict),
            ..TypvalT::default()
        },
        TypvalT {
            value: TypvalValue::String(Some(key.to_vec())),
            ..TypvalT::default()
        },
        TypvalT {
            value: TypvalValue::Dict(changes),
            ..TypvalT::default()
        },
    ];

    // Hold the Dictionary alive across callback-driven mutation.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*dict).dv_refcount += 1 };
    // Boxed nodes keep these pointers stable if the Vec reallocates or
    // unrelated watchers are removed during a callback.
    let watchers: Vec<*mut crate::eval::typval_defs::DictWatcher> =
        unsafe { &mut (*dict).watchers }
            .iter_mut()
            .map(|watcher| std::ptr::from_mut(&mut **watcher))
            .collect();
    let mut any_needs_free = false;
    for watcher in watchers {
        // SAFETY: nodes are boxed and a busy watcher cannot be freed.
        if unsafe { (*watcher).busy }
            || !tv_dict_watcher_matches(unsafe { &*watcher }, key)
        {
            continue;
        }

        // SAFETY: watcher remains live through callback_call.
        unsafe { (*watcher).busy = true };
        let mut rettv = TypvalT::default();
        // SAFETY: forwarded from this function's own safety doc.
        let _ = unsafe {
            crate::eval::eval::callback_call(
                &(*watcher).callback,
                &argv,
                &mut rettv,
            )
        };
        // SAFETY: callback result is owned locally.
        unsafe { tv_clear_simple(&rettv) };
        // SAFETY: watcher is still present; self-removal was deferred.
        unsafe {
            (*watcher).busy = false;
            any_needs_free |= (*watcher).needs_free;
        }
    }

    if any_needs_free {
        // SAFETY: forwarded from this function's own safety doc.
        let watchers = unsafe { &mut (*dict).watchers };
        let mut i = 0;
        while i < watchers.len() {
            if watchers[i].needs_free {
                let mut watcher = watchers.remove(i);
                tv_dict_watcher_free(&mut watcher);
            } else {
                i += 1;
            }
        }
    }

    // SAFETY: release the temporary Dictionary hold and changes Dict.
    unsafe {
        tv_dict_unref(dict);
        tv_dict_unref(changes);
    }
}

/// Get a function from a dictionary, storing it into `result`, and
/// return whether this succeeded (`tv_dict_get_callback`).
///
/// `*result` is always set to [`Callback::None`] first, mirroring the
/// original's own unconditional `result->type = kCallbackNone;` -
/// stays that way both for the "key not found" (a real SUCCESS,
/// matching the original's own `return true;` there) and the "found,
/// but the wrong type" (a real FAILURE; the original's own
/// `emsg("E6000: ...")` display is skipped, matching this module's
/// established policy) cases. Only a genuinely successful Func/
/// String/Partial value overwrites `*result`.
///
/// `tv_clear_simple` (not the generic, recursion-capable `tv_clear`)
/// suffices for releasing the local, `tv_copy`'d working value at the
/// end: by this point it can only be a `Func`/`String`/`Partial`
/// (the earlier type check already ruled out everything else), none
/// of which need `tv_clear`'s own recursive-tree-walk machinery -
/// matching the same reasoning already established for
/// `free_funccal_contents`.
///
/// # Safety
/// `d`, if non-null, must be a valid, live [`crate::eval::typval_defs::DictT`].
/// Forwards [`crate::eval::eval::set_selfdict`]/[`callback_from_typval`]'s
/// own safety requirements for the found dictionary item's value.
pub unsafe fn tv_dict_get_callback(d: *mut DictT, key: &[u8], result: &mut Callback) -> bool {
    *result = Callback::None;

    let d_opt = if d.is_null() { None } else { Some(unsafe { &mut *d }) };
    let Some(di) = tv_dict_find(d_opt, key) else {
        return true;
    };

    // SAFETY: `di` is a valid, live dictitem pointer, just found above.
    let di_tv = unsafe { &(*di).di_tv };
    if !tv_is_func(di_tv) && !matches!(di_tv.value, TypvalValue::String(_)) {
        return false;
    }

    let mut tv = TypvalT::default();
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_copy(di_tv, &mut tv) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::eval::set_selfdict(&mut tv, d) };
    // SAFETY: forwarded from this function's own safety doc.
    let converted = unsafe { callback_from_typval(&tv) };
    let ok = converted.is_some();
    if let Some(cb) = converted {
        *result = cb;
    }
    // SAFETY: `tv` is exclusively owned here, about to go out of scope.
    unsafe { tv_clear_simple(&tv) };
    ok
}

/// Free a dictionary item, also clearing the value (`tv_dict_item_free`).
///
/// The original's `tv_clear(&item->di_tv)` is replicated via
/// `tv_clear_simple` - see that function's own doc comment for the
/// one remaining gap (`VAR_PARTIAL`).
///
/// # Safety
/// `item` must be a valid pointer previously returned by
/// [`tv_dict_item_alloc`] (or, for the "not separately allocated"
/// case - `di_flags` without [`dict_item_flags::ALLOC`] - a pointer
/// into a live, embedded `dictitem_T`-shaped struct like
/// `ChangedtickDictItem`), not yet freed, and no longer reachable from
/// any hashtable/other structure (the caller's job - see
/// [`tv_dict_item_remove`] for the usual "remove from hashtab, then
/// free" pairing this crate expects). Forwards `tv_clear_simple`'s
/// own safety requirements too.
pub unsafe fn tv_dict_item_free(item: *mut DictitemT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_clear_simple(&(*item).di_tv) };

    // SAFETY: forwarded from this function's own safety doc.
    let flags = unsafe { (*item).di_flags };
    if flags & dict_item_flags::ALLOC != 0 {
        // SAFETY: `DI_FLAGS_ALLOC` guarantees this came from
        // `tv_dict_item_alloc`'s own `Box::into_raw` - forwarded from
        // this function's own safety doc.
        drop(unsafe { Box::from_raw(item) });
    } else {
        // Not separately allocated (e.g. embedded in another struct
        // like `ChangedtickDictItem`) - clear the value in place but
        // don't free the item itself, matching the original exactly.
        // Assigning through the raw pointer runs the old value's Drop
        // (releasing any owned String/Vec) automatically.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*item).di_tv = TypvalT::default() };
    }
}

/// Make a copy of a dictionary item (`tv_dict_item_copy`).
///
/// # Safety
/// `di` must be a valid, non-null pointer to a live `DictitemT`.
/// Forwards [`tv_copy`]'s own safety requirements for any List/Dict/
/// Blob value `di` holds.
#[must_use]
pub unsafe fn tv_dict_item_copy(di: *mut DictitemT) -> *mut DictitemT {
    // SAFETY: forwarded from this function's own safety doc.
    let key: &[u8] = unsafe { &(*di).di_key };
    // `di_key` carries a trailing NUL; `tv_dict_item_alloc` appends
    // its own, so strip it here to avoid double-NUL-terminating.
    let key = &key[..key.len().saturating_sub(1)];
    let new_di = tv_dict_item_alloc(key);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_copy(&(*di).di_tv, &mut (*new_di).di_tv) };
    new_di
}

/// Remove item from dictionary and free it (`tv_dict_item_remove`).
///
/// # Safety
/// `item` must be a valid pointer currently present in `dict`
/// (previously added via [`tv_dict_add`]), matching
/// [`tv_dict_item_free`]'s own contract for the rest.
pub unsafe fn tv_dict_item_remove(dict: &mut DictT, item: *mut DictitemT) {
    // SAFETY: forwarded from this function's own safety doc.
    let key_ptr = unsafe { (*item).di_key.as_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let key: &[u8] = unsafe { &(*item).di_key };
    // Strip the trailing NUL `di_key` carries - `hash_remove` (like
    // `hash_find`) takes the bare key bytes.
    let key = &key[..key.len().saturating_sub(1)];
    dict.dv_hashtab.hash_remove(key);
    // `dv_index` is keyed by each item's `hi_key` address (the key
    // bytes' own pointer), not the item's own address - matching how
    // `tv_dict_add` inserted it.
    dict.dv_index.remove(&(key_ptr as usize));
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict_item_free(item) };
}

/// `remove({dict}, {key})` - the `Dict` case of `remove()`
/// (`tv_dict_remove`).
///
/// Removes and returns the value for `{key}`, moving it directly
/// into `rettv` (matching the original's own plain `*rettv =
/// di->di_tv` struct assignment - NOT a [`tv_copy`], so no refcount
/// change happens for a `List`/`Dict`/`Blob`-valued item) before
/// freeing the now-blanked dict item via [`tv_dict_item_remove`]
/// (whose own `tv_dict_item_free` call harmlessly no-ops on the
/// blanked value left behind by the `std::mem::take` above).
///
/// The original's own too-many-arguments check
/// (`argvars[2].v_type != VAR_UNKNOWN`) is naturally unreachable here,
/// since this function's own `max_argc` (2) already makes
/// `call_internal_func` reject a 3rd argument before this function is
/// ever reached, matching this crate's own already-established
/// "`call_internal_func` enforces `min_argc`/`max_argc` before
/// dispatch" convention.
///
/// # Safety
/// `argvars[0].value` must be `Dict`-typed; if its pointer is
/// non-null, it must be valid, with every item genuinely allocated
/// via `tv_dict_item_alloc`/`Box::into_raw`.
pub unsafe fn tv_dict_remove(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let TypvalValue::Dict(d) = argvars[0].value else { unreachable!() };
    if d.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    if value_check_lock(unsafe { (*d).dv_lock }, None) {
        return;
    }

    let Some(key) = tv_get_string_chk(&argvars[1]) else {
        return; // type error; errmsg already given in the original.
    };
    // SAFETY: forwarded from this function's own safety doc.
    let Some(di) = (unsafe { tv_dict_find(Some(&mut *d), &key) }) else {
        return; // semsg(_(e_dictkey), ...) omitted.
    };

    // SAFETY: forwarded from this function's own safety doc.
    let flags = unsafe { (*di).di_flags };
    if crate::eval::vars::var_check_fixed(flags) || crate::eval::vars::var_check_ro(flags) {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        *rettv = std::mem::take(&mut (*di).di_tv);
        tv_dict_item_remove(&mut *d, di);
        if tv_dict_is_watched(d) {
            tv_dict_watcher_notify(d, &key, None, Some(rettv));
        }
    }
}

/// Extend dictionary `d1` with items from dictionary `d2`
/// (`tv_dict_extend`).
///
/// `action`'s first byte only is checked, matching the original's own
/// `*action` comparisons: `b'e'` ("error") - a duplicate key gives an
/// error (the loop stops early, matching the original's own `break`);
/// `b'f'` ("force") - a duplicate key's value is replaced; anything
/// else ("keep") - a duplicate key is silently ignored, the existing
/// value kept.
///
/// # Panics
/// Panics if `action`'s first byte is `b'm'` ("move" - items moved
/// from `d2` into `d1` rather than copied). This needs a "detach a
/// dict item from its own dict without freeing it" primitive this
/// crate doesn't have yet (`tv_dict_item_remove` always frees).
/// Genuinely unreachable from THIS crate's own only translated caller
/// chain - `extend()`/`extendnew()`'s own 3rd-argument validation
/// only ever allows `"keep"`/`"force"`/`"error"`, never `"move"` (the
/// original's only `"move"` caller is `window.c`'s scroll-event
/// handling, not yet translated) - kept as a real parameter value
/// (not simply omitted) for signature fidelity with this genuinely
/// reusable original function.
///
/// Omits the original's own `tv_dict_wrong_func_name` check (see
/// [`tv_dict_add`]'s own doc comment for why this can never trigger
/// yet). Watcher notifications are complete.
///
/// # Safety
/// `d1`/`d2` must be valid, non-null pointers to live `DictT`s, with
/// every item genuinely allocated via `tv_dict_item_alloc`/
/// `Box::into_raw`.
pub unsafe fn tv_dict_extend(d1: *mut DictT, d2: *mut DictT, action: &[u8]) {
    let action0 = action.first().copied().unwrap_or(b'f');
    // SAFETY: forwarded from this function's own safety doc.
    let watched = unsafe { tv_dict_is_watched(d1) };

    // SAFETY: forwarded from this function's own safety doc.
    let items: Vec<*mut DictitemT> = unsafe { &*d2 }.dv_index.values().copied().collect();
    for di2 in items {
        // di_key always carries a trailing NUL terminator - strip it
        // before searching d1, matching this module's own established
        // stripping idiom used throughout.
        // SAFETY: forwarded from this function's own safety doc.
        let key: Vec<u8> = unsafe {
            let k = &(*di2).di_key;
            k[..k.len().saturating_sub(1)].to_vec()
        };
        // SAFETY: forwarded from this function's own safety doc.
        let di1 = unsafe { tv_dict_find(Some(&mut *d1), &key) };

        match di1 {
            None => {
                if action0 == b'm' {
                    unimplemented!(
                        "tv_dict_extend: action=\"move\" needs a dict-item detach-without-free \
                         primitive, not yet translated - unreachable from extend()/extendnew() \
                         themselves, see this function's own doc comment"
                    );
                }
                // SAFETY: forwarded from this function's own safety doc.
                let new_di = unsafe { tv_dict_item_copy(di2) };
                // SAFETY: forwarded from this function's own safety doc.
                if unsafe { tv_dict_add(&mut *d1, new_di) } == FAIL {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { tv_dict_item_free(new_di) };
                } else if watched {
                    // SAFETY: new_di is now owned by d1 and live.
                    unsafe {
                        tv_dict_watcher_notify(
                            d1,
                            &key,
                            Some(&(*new_di).di_tv),
                            None,
                        )
                    };
                }
            }
            Some(_) if action0 == b'e' => {
                break; // semsg(_("E737: Key already exists: %s"), ...) omitted.
            }
            Some(di1) if action0 == b'f' && di2 != di1 => {
                // SAFETY: forwarded from this function's own safety doc.
                let (locked, flags) = unsafe { ((*di1).di_tv.v_lock, (*di1).di_flags) };
                if value_check_lock(locked, None) || crate::eval::vars::var_check_ro(flags) {
                    break;
                }
                let mut oldtv = TypvalT::default();
                if watched {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { tv_copy(&(*di1).di_tv, &mut oldtv) };
                }
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    tv_clear_simple(&(*di1).di_tv);
                    tv_copy(&(*di2).di_tv, &mut (*di1).di_tv);
                }
                if watched {
                    // SAFETY: di1 remains live in d1.
                    unsafe {
                        tv_dict_watcher_notify(
                            d1,
                            &key,
                            Some(&(*di1).di_tv),
                            Some(&oldtv),
                        );
                        tv_clear_simple(&oldtv);
                    }
                }
            }
            _ => {} // action == "keep" ('k'), or di2 == di1: do nothing.
        }
    }
}

/// Allocate an empty dictionary. Caller should take care of the
/// reference count (`tv_dict_alloc`).
#[must_use]
pub fn tv_dict_alloc() -> *mut DictT {
    let d = Box::into_raw(Box::new(DictT {
        dv_lock: VarLockStatus::Unlocked,
        dv_scope: ScopeType::NoScope,
        dv_refcount: 0,
        dv_copy_id: 0,
        dv_hashtab: crate::hashtab_defs::HashtabT::hash_init(),
        dv_index: std::collections::HashMap::new(),
        dv_copydict: std::ptr::null_mut(),
        watchers: Vec::new(),
        dv_used_next: std::ptr::null_mut(),
        dv_used_prev: std::ptr::null_mut(),
        lua_table_ref: LUA_NOREF,
    }));

    // Add the dict to the list of dicts for garbage collection.
    // SAFETY: GC_FIRST_DICT is only ever read/written through this
    // module's own functions, which never hold a live reference across
    // another call into this same cell.
    let gc_first = unsafe { *GC_FIRST_DICT.get_mut() };
    if !gc_first.is_null() {
        // SAFETY: gc_first is either null (checked above) or a live
        // pointer previously produced by this same function.
        unsafe { (*gc_first).dv_used_prev = d };
    }
    // SAFETY: forwarded from this function's own reasoning above.
    unsafe { (*d).dv_used_next = gc_first };
    // SAFETY: forwarded from this function's own reasoning above.
    unsafe { *GC_FIRST_DICT.get_mut() = d };

    d
}

/// Allocate an empty dictionary with a given lock status
/// (`tv_dict_alloc_lock`).
///
/// Note: like [`tv_dict_alloc`], this touches the shared
/// `GC_FIRST_DICT` linked list - any TEST calling it must hold
/// `crate::globals::global_state_test_lock()` for its whole body.
#[must_use]
pub fn tv_dict_alloc_lock(lock: VarLockStatus) -> *mut DictT {
    let d = tv_dict_alloc();
    // SAFETY: `d` was just allocated above, not yet reachable from
    // anywhere else.
    unsafe { (*d).dv_lock = lock };
    d
}

/// Set the return value of `tv` to a dict, incrementing its reference
/// count if non-null (`tv_dict_set_ret`, `eval/typval.h`'s own
/// `static inline`).
///
/// # Safety
/// `d`, if non-null, must be a valid pointer to a live `DictT`.
pub unsafe fn tv_dict_set_ret(tv: &mut TypvalT, d: *mut DictT) {
    tv.value = TypvalValue::Dict(d);
    if !d.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*d).dv_refcount += 1 };
    }
}

/// Allocate an empty dict and put it in `ret_tv`, incrementing its
/// reference count (`tv_dict_alloc_ret`) - the dict counterpart of
/// [`tv_list_alloc_ret`].
///
/// # Safety
/// None beyond [`tv_dict_alloc`]'s own (always-safe) contract - this
/// function only ever writes into `ret_tv`, a plain `&mut TypvalT`.
pub unsafe fn tv_dict_alloc_ret(ret_tv: &mut TypvalT) -> *mut DictT {
    let d = tv_dict_alloc();
    // SAFETY: `d` was just allocated above, a fresh pointer not shared
    // anywhere else yet.
    unsafe { tv_dict_set_ret(ret_tv, d) };
    d
}

/// Set all existing keys in `dict` as read-only. Does not protect
/// against adding new keys to the dictionary
/// (`tv_dict_set_keys_readonly`).
///
/// Unlike the original (which locks `dv_hashtab`, walks it via
/// `HASHTAB_ITER` + `TV_DICT_HI2DI`), `dv_index` already gives a
/// direct list of every live item - no hashtab traversal/locking
/// needed at all, matching `tv_dict_free_contents`'s own established
/// `dv_index`-based iteration precedent.
///
/// # Safety
/// `dict` must be a valid, non-null pointer to a live `DictT` whose
/// every `dv_index` entry is a valid, live `DictitemT` pointer.
pub unsafe fn tv_dict_set_keys_readonly(dict: *mut DictT) {
    // SAFETY: forwarded from this function's own safety doc.
    let items: Vec<*mut DictitemT> = unsafe { &*dict }.dv_index.values().copied().collect();
    for item in items {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*item).di_flags |= dict_item_flags::RO | dict_item_flags::FIX };
    }
}

/// Free items contained in a dictionary (`tv_dict_free_contents`).
///
/// Nested List/Dict/Partial values are released through an explicit
/// worklist instead of recursive native calls.
///
/// # Safety
/// `d` must be a valid, non-null pointer to a live `DictT` whose every
/// item satisfies [`tv_dict_item_free`]'s own safety contract.
pub unsafe fn tv_dict_free_contents(d: *mut DictT) {
    unsafe { free_targets(FreeTarget::Dict(d, false)) };
}

/// Free a dictionary itself, ignoring items it contains. Ignores the
/// reference count (`tv_dict_free_dict`).
///
/// # Safety
/// `d` must be a valid pointer previously returned by
/// [`tv_dict_alloc`], not yet freed.
pub unsafe fn tv_dict_free_dict(d: *mut DictT) {
    // Remove the dict from the list of dicts for garbage collection.
    // SAFETY: forwarded from this function's own safety doc.
    let (used_prev, used_next) = unsafe { ((*d).dv_used_prev, (*d).dv_used_next) };
    if used_prev.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *GC_FIRST_DICT.get_mut() = used_next };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*used_prev).dv_used_next = used_next };
    }
    if !used_next.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*used_next).dv_used_prev = used_prev };
    }

    // NLUA_CLEAR_REF(d->lua_table_ref): omitted - the Lua host (phase
    // 13) isn't started, and lua_table_ref is always LUA_NOREF here.

    // SAFETY: forwarded from this function's own safety doc.
    drop(unsafe { Box::from_raw(d) });
}

/// Free a dictionary, including all items it contains. Ignores the
/// reference count (`tv_dict_free`).
///
/// Nested container destruction is iterative and stack-safe.
///
/// # Safety
/// Same as [`tv_dict_free_contents`]/[`tv_dict_free_dict`] combined.
pub unsafe fn tv_dict_free(d: *mut DictT) {
    // The original's `tv_in_free_unref_items` re-entrancy guard is
    // always false here - nothing in this crate can trigger the
    // garbage-collector's "unreferencing everything" pass that sets it
    // (that pass doesn't exist yet).
    unsafe { free_targets(FreeTarget::Dict(d, true)) };
}

/// Unreference a dictionary: decrements the reference count and frees
/// the dictionary when it becomes zero or less (`tv_dict_unref`).
///
/// # Safety
/// `d`, if non-null, must be a valid pointer previously returned by
/// [`tv_dict_alloc`], not yet freed.
pub unsafe fn tv_dict_unref(d: *mut DictT) {
    if d.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*d).dv_refcount -= 1 };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*d).dv_refcount } <= 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_dict_free(d) };
    }
}

/// Find item in dictionary (`tv_dict_find`).
///
/// Unlike the original (`ptrdiff_t len`, negative meaning
/// "NUL-terminated"), takes `key: &[u8]` directly - a Rust slice
/// always carries its own length, so there is nothing left to
/// distinguish (same reasoning as `hashtab.rs`'s own `hash_find`/
/// `hash_find_len` collapse).
#[must_use]
pub fn tv_dict_find(d: Option<&mut DictT>, key: &[u8]) -> Option<*mut DictitemT> {
    let d = d?;
    let hi = d.dv_hashtab.hash_find(key);
    if crate::hashtab::hashitem_empty(hi) {
        return None;
    }
    d.dv_index.get(&(hi.hi_key as usize)).copied()
}

/// Check if a key is present in a dictionary (`tv_dict_has_key`).
#[must_use]
pub fn tv_dict_has_key(d: Option<&mut DictT>, key: &[u8]) -> bool {
    tv_dict_find(d, key).is_some()
}

/// Get the number of items in a dictionary, or `0` if `d` is null
/// (`tv_dict_len`, `eval/typval.h`'s own `static inline`).
///
/// Uses [`DictT::dv_index`]'s own length rather than the original's
/// `dv_hashtab.ht_used` - both are always kept in exact sync (see
/// `dv_index`'s own doc comment), and `dv_index` is already this
/// crate's established "source of truth" for a dict's live items
/// (e.g. [`tv_dict_free_contents`]'s own usage above).
#[must_use]
pub fn tv_dict_len(d: Option<&DictT>) -> i32 {
    d.map_or(0, |d| d.dv_index.len() as i32)
}

/// Get a string item from a dictionary (`tv_dict_get_string`/
/// `tv_dict_get_string_buf`).
///
/// Returns `None` if `key` does not exist; if it does, always returns
/// `Some` (an empty `Vec` for a type-mismatched value, matching
/// [`tv_get_string`]'s own "always produce a value, empty on error"
/// behavior - contrast [`tv_dict_get_string_chk`], which can return
/// `None` for a found-but-wrong-type value).
///
/// Collapses the original's `tv_dict_get_string`/
/// `tv_dict_get_string_buf` pair into one function - same
/// shared-static-buffer-is-moot reasoning as [`tv_get_string_chk`]'s
/// own doc comment (the original's `save` parameter, controlling
/// whether the returned pointer is `xstrdup`'d, is dropped for the
/// same reason: every return here is already a freshly-owned `Vec`).
///
/// # Safety
/// `di_tv`'s value, once found, must be safe to read - same contract
/// every other `tv_dict_*` lookup in this module already places on
/// `d`.
#[must_use]
pub unsafe fn tv_dict_get_string(d: Option<&mut DictT>, key: &[u8]) -> Option<Vec<u8>> {
    let di = tv_dict_find(d, key)?;
    // SAFETY: forwarded from this function's own safety doc.
    Some(tv_get_string(unsafe { &(*di).di_tv }))
}

/// Get a string item from a dictionary, with a default for a missing
/// key (`tv_dict_get_string_buf_chk`).
///
/// Returns `def` if `key` does not exist, `None` if it exists but has
/// the wrong type, else the string value - matching the original
/// exactly except for the same `_buf`/`save` collapsing
/// [`tv_dict_get_string`]'s own doc comment explains.
///
/// # Safety
/// Same as [`tv_dict_get_string`].
#[must_use]
pub unsafe fn tv_dict_get_string_chk(
    d: Option<&mut DictT>,
    key: &[u8],
    def: Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    let Some(di) = tv_dict_find(d, key) else {
        return def;
    };
    // SAFETY: forwarded from this function's own safety doc.
    tv_get_string_chk(unsafe { &(*di).di_tv })
}

/// Get a typval item from a dictionary and copy it into `rettv`
/// (`tv_dict_get_tv`).
///
/// @return `OK` in case of success or `FAIL` if nothing was found.
///
/// # Safety
/// Same as [`tv_dict_get_string`], plus `rettv`'s existing value, if
/// any, must be safe to overwrite without leaking (matching
/// [`tv_copy`]'s own safety doc, forwarded here).
pub unsafe fn tv_dict_get_tv(d: Option<&mut DictT>, key: &[u8], rettv: &mut TypvalT) -> i32 {
    let Some(di) = tv_dict_find(d, key) else {
        return FAIL;
    };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_copy(&(*di).di_tv, rettv) };
    OK
}

/// Get a number item from a dictionary, or `0` if the item does not
/// exist (`tv_dict_get_number`).
///
/// # Safety
/// Same as [`tv_dict_get_string`].
#[must_use]
pub unsafe fn tv_dict_get_number(d: Option<&mut DictT>, key: &[u8]) -> crate::eval::typval_defs::VarnumberT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict_get_number_def(d, key, 0) }
}

/// Get a number item from a dictionary, or a given default value
/// (`tv_dict_get_number_def`).
///
/// # Safety
/// Same as [`tv_dict_get_string`].
#[must_use]
pub unsafe fn tv_dict_get_number_def(
    d: Option<&mut DictT>,
    key: &[u8],
    def: crate::eval::typval_defs::VarnumberT,
) -> crate::eval::typval_defs::VarnumberT {
    let Some(di) = tv_dict_find(d, key) else {
        return def;
    };
    // SAFETY: forwarded from this function's own safety doc.
    tv_get_number(unsafe { &(*di).di_tv })
}

/// Get a boolean item from a dictionary, or a given default value
/// (`tv_dict_get_bool`).
///
/// # Safety
/// Same as [`tv_dict_get_string`].
#[must_use]
pub unsafe fn tv_dict_get_bool(
    d: Option<&mut DictT>,
    key: &[u8],
    def: crate::eval::typval_defs::VarnumberT,
) -> crate::eval::typval_defs::VarnumberT {
    let Some(di) = tv_dict_find(d, key) else {
        return def;
    };
    // SAFETY: forwarded from this function's own safety doc.
    tv_get_bool(unsafe { &(*di).di_tv })
}

/// Add item to dictionary (`tv_dict_add`).
///
/// @return `FAIL` if key already exists.
///
/// Omits the original's `tv_dict_wrong_func_name` check (rejecting a
/// function-typed value added to the real `g:`/`l:` scope dict) - see
/// this module's own doc comment for why.
///
/// # Safety
/// `item` must be a valid, non-null pointer previously returned by
/// [`tv_dict_item_alloc`] (or equivalent), not already present in any
/// dictionary's hashtable.
pub unsafe fn tv_dict_add(d: &mut DictT, item: *mut DictitemT) -> i32 {
    // SAFETY: `di_key` is owned by `*item`, which the caller
    // guarantees outlives this hashtable entry (forwarded from this
    // function's own safety doc).
    let key_ptr = unsafe { (*item).di_key.as_mut_ptr() as *mut std::os::raw::c_char };
    // SAFETY: forwarded from this function's own safety doc.
    let rc = unsafe { d.dv_hashtab.hash_add(key_ptr) };
    if rc == OK {
        d.dv_index.insert(key_ptr as usize, item);
    }
    rc
}

/// Make a copy of a dictionary (`tv_dict_copy`). Returns `None` if
/// `orig` is null.
///
/// `deep=true` recursively copies every nested `List`/`Dict` item too,
/// via [`crate::eval::eval::var_item_copy`] - if that fails partway
/// through (recursion limit or a nested allocation failure), only the
/// loop stops early; the partial copy accumulated so far is still
/// returned (refcount incremented, and NOT discarded unless `got_int`
/// is *also* set), matching the original's own plain `break` (not a
/// `goto`-based abort) on this specific failure exactly - a real
/// asymmetry already present in the original relative to
/// [`tv_list_copy`]'s own full-abort behavior on the same kind of
/// failure, not a translation choice.
///
/// # Safety
/// `orig`, if non-null, must be a valid pointer to a live `DictT`.
pub unsafe fn tv_dict_copy(
    conv: *const crate::types_defs::VimconvT,
    orig: *mut DictT,
    deep: bool,
    copy_id: i32,
) -> *mut DictT {
    if orig.is_null() {
        return std::ptr::null_mut();
    }

    let copy = tv_dict_alloc();
    if copy_id != 0 {
        // Do this before adding the items, because one of the items
        // may refer back to this dict.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            (*orig).dv_copy_id = copy_id;
            (*orig).dv_copydict = copy;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    let items: Vec<*mut DictitemT> = unsafe { &*orig }.dv_index.values().copied().collect();
    for di in items {
        // SAFETY: GLOBALS is only ever accessed through this crate's
        // established single-threaded-main-loop convention.
        if unsafe { crate::globals::GLOBALS.get_mut() }.got_int {
            break;
        }
        // di_key always carries a trailing NUL terminator (matching
        // hi_key's C-string contract) - strip it before re-allocating
        // via tv_dict_item_alloc, which re-adds its own.
        // SAFETY: `di` came from the dict's own live index above.
        let key = unsafe { &(*di).di_key };
        let key = &key[..key.len() - 1];
        let new_di = tv_dict_item_alloc(key);
        if deep {
            // SAFETY: forwarded from this function's own safety doc.
            let ret = unsafe {
                crate::eval::eval::var_item_copy(conv, &(*di).di_tv, &mut (*new_di).di_tv, deep, copy_id)
            };
            if ret == FAIL {
                // xfree(new_di): new_di's own di_tv is either
                // untouched (Unknown, recursion-limit case) or a null
                // List/Dict pointer (nested-copy-failure case) at
                // this point - either way there is nothing owned to
                // release, matching the original's own plain
                // xfree(new_di) (not a full tv_dict_item_free-based
                // free) exactly.
                drop(unsafe { Box::from_raw(new_di) });
                break;
            }
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_copy(&(*di).di_tv, &mut (*new_di).di_tv) };
        }
        // SAFETY: `copy`/`new_di` are both valid, freshly-prepared
        // pointers not shared with anything yet.
        if unsafe { tv_dict_add(&mut *copy, new_di) } == FAIL {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_dict_item_free(new_di) };
            break;
        }
    }

    // SAFETY: `copy` was just allocated above by this same function.
    unsafe { (*copy).dv_refcount += 1 };
    // SAFETY: GLOBALS access, same convention as above.
    if unsafe { crate::globals::GLOBALS.get_mut() }.got_int {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_dict_unref(copy) };
        return std::ptr::null_mut();
    }

    copy
}

/// Add a list entry to a dictionary; `list`'s reference count is
/// incremented on success (`tv_dict_add_list`).
///
/// Returns `OK`/`FAIL` (`FAIL` when `key` already exists - `list`'s
/// ownership stays with the caller in that case, matching the
/// original's "detach so `tv_dict_item_free()` does not unref it" own
/// comment).
///
/// # Safety
/// `list`, if non-null, must be a valid pointer to a live `ListT`.
pub unsafe fn tv_dict_add_list(
    d: &mut DictT,
    key: &[u8],
    list: *mut crate::eval::typval_defs::ListT,
) -> i32 {
    let item = tv_dict_item_alloc(key);
    // SAFETY: `item` was just allocated above, not yet in any dict.
    unsafe { (*item).di_tv.value = TypvalValue::List(list) };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { tv_dict_add(d, item) } == FAIL {
        // SAFETY: `item` is still exclusively owned here.
        unsafe { (*item).di_tv.value = TypvalValue::List(std::ptr::null_mut()) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_dict_item_free(item) };
        return FAIL;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_ref(list) };
    OK
}

/// Add a dictionary entry to a dictionary; `dict`'s reference count is
/// incremented on success (`tv_dict_add_dict`).
///
/// # Safety
/// `dict`, if non-null, must be a valid pointer to a live `DictT`.
pub unsafe fn tv_dict_add_dict(d: &mut DictT, key: &[u8], dict: *mut DictT) -> i32 {
    let item = tv_dict_item_alloc(key);
    // SAFETY: `item` was just allocated above, not yet in any dict.
    unsafe { (*item).di_tv.value = TypvalValue::Dict(dict) };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { tv_dict_add(d, item) } == FAIL {
        // SAFETY: `item` is still exclusively owned here.
        unsafe { (*item).di_tv.value = TypvalValue::Dict(std::ptr::null_mut()) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_dict_item_free(item) };
        return FAIL;
    }
    if !dict.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*dict).dv_refcount += 1 };
    }
    OK
}

/// Add a typval entry to a dictionary; `tv` is copied (see [`tv_copy`])
/// (`tv_dict_add_tv`).
///
/// # Safety
/// Forwards [`tv_copy`]'s own safety requirements for `tv`.
pub unsafe fn tv_dict_add_tv(d: &mut DictT, key: &[u8], tv: &TypvalT) -> i32 {
    let item = tv_dict_item_alloc(key);
    // SAFETY: `item` was just allocated above, not yet in any dict;
    // forwarded from this function's own safety doc for `tv`.
    unsafe { tv_copy(tv, &mut (*item).di_tv) };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { tv_dict_add(d, item) } == FAIL {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_dict_item_free(item) };
        return FAIL;
    }
    OK
}

/// Add a number entry to a dictionary (`tv_dict_add_nr`).
pub fn tv_dict_add_nr(d: &mut DictT, key: &[u8], nr: crate::eval::typval_defs::VarnumberT) -> i32 {
    let item = tv_dict_item_alloc(key);
    // SAFETY: `item` was just allocated above, not yet in any dict.
    unsafe { (*item).di_tv.value = TypvalValue::Number(nr) };
    // SAFETY: `item` is a freshly-allocated, not-yet-shared pointer.
    if unsafe { tv_dict_add(d, item) } == FAIL {
        // SAFETY: same as above.
        unsafe { tv_dict_item_free(item) };
        return FAIL;
    }
    OK
}

/// Add a floating point number entry to a dictionary
/// (`tv_dict_add_float`).
pub fn tv_dict_add_float(d: &mut DictT, key: &[u8], nr: f64) -> i32 {
    let item = tv_dict_item_alloc(key);
    // SAFETY: `item` was just allocated above, not yet in any dict.
    unsafe { (*item).di_tv.value = TypvalValue::Float(nr) };
    // SAFETY: `item` is a freshly-allocated, not-yet-shared pointer.
    if unsafe { tv_dict_add(d, item) } == FAIL {
        // SAFETY: same as above.
        unsafe { tv_dict_item_free(item) };
        return FAIL;
    }
    OK
}

/// Add a boolean entry to a dictionary (`tv_dict_add_bool`).
pub fn tv_dict_add_bool(
    d: &mut DictT,
    key: &[u8],
    val: crate::eval::typval_defs::BoolVarValue,
) -> i32 {
    let item = tv_dict_item_alloc(key);
    // SAFETY: `item` was just allocated above, not yet in any dict.
    unsafe { (*item).di_tv.value = TypvalValue::Bool(val) };
    // SAFETY: `item` is a freshly-allocated, not-yet-shared pointer.
    if unsafe { tv_dict_add(d, item) } == FAIL {
        // SAFETY: same as above.
        unsafe { tv_dict_item_free(item) };
        return FAIL;
    }
    OK
}

/// Add a string entry to a dictionary; always deep-copies `val` into a
/// freshly owned `Vec<u8>` (`tv_dict_add_str`/`tv_dict_add_str_len`/
/// `tv_dict_add_allocated_str` collapsed into one function - Rust's
/// `&[u8]` already carries its own length, so the "how many bytes"
/// question those three separate original variants existed to answer
/// doesn't arise here, and there is no equivalent to the "adopt an
/// already-allocated buffer without copying" optimization
/// `tv_dict_add_allocated_str` provided, since every caller in this
/// crate already owns a `Vec<u8>`/`&[u8]` it can simply clone or move).
/// `None` stores an absent string, matching the original's
/// `val == NULL` case.
pub fn tv_dict_add_str(d: &mut DictT, key: &[u8], val: Option<&[u8]>) -> i32 {
    let item = tv_dict_item_alloc(key);
    // SAFETY: `item` was just allocated above, not yet in any dict.
    unsafe { (*item).di_tv.value = TypvalValue::String(val.map(<[u8]>::to_vec)) };
    // SAFETY: `item` is a freshly-allocated, not-yet-shared pointer.
    if unsafe { tv_dict_add(d, item) } == FAIL {
        // SAFETY: same as above.
        unsafe { tv_dict_item_free(item) };
        return FAIL;
    }
    OK
}

/// Add a function entry to a dictionary (`tv_dict_add_func`).
///
/// `(*fp).uf_name` is expected to be NUL-terminated (matching
/// `func_hashtab`'s own storage convention - see `eval/userfunc.rs`'s
/// module doc); the trailing NUL is stripped before storing the name
/// into the dict item's own `Func` value (which, like
/// `TypvalValue::String`, carries no NUL of its own) and before
/// calling `func_ref`.
///
/// # Safety
/// `fp` must be a valid, non-null pointer to a live `UfuncT`.
pub unsafe fn tv_dict_add_func(d: &mut DictT, key: &[u8], fp: *mut crate::eval::typval_defs::UfuncT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let raw_name: &[u8] = unsafe { &(*fp).uf_name };
    let name = &raw_name[..raw_name.len().saturating_sub(1)];
    let item = tv_dict_item_alloc(key);
    // SAFETY: `item` was just allocated above, not yet in any dict.
    unsafe { (*item).di_tv.value = TypvalValue::Func(Some(name.to_vec())) };
    // Reference before tv_dict_add() so tv_dict_item_free()'s unref
    // stays balanced on failure, matching the original's own comment
    // exactly.
    crate::eval::userfunc::func_ref(Some(name));
    // SAFETY: `item` is a freshly-allocated, not-yet-shared pointer.
    if unsafe { tv_dict_add(d, item) } == FAIL {
        // SAFETY: same as above.
        unsafe { tv_dict_item_free(item) };
        return FAIL;
    }
    OK
}

/// Allocate a blob. Caller should take care of the reference count
/// (`tv_blob_alloc`).
#[must_use]
pub fn tv_blob_alloc() -> *mut crate::eval::typval_defs::BlobT {
    let mut bv_ga = crate::garray_defs::GarrayT::default();
    bv_ga.ga_init(1, 100);
    Box::into_raw(Box::new(crate::eval::typval_defs::BlobT {
        bv_ga,
        bv_refcount: 0,
        bv_lock: VarLockStatus::Unlocked,
    }))
}

/// Free a blob. Ignores the reference count (`tv_blob_free`).
///
/// # Safety
/// `b` must be a valid pointer previously returned by [`tv_blob_alloc`],
/// not yet freed.
pub unsafe fn tv_blob_free(b: *mut crate::eval::typval_defs::BlobT) {
    // SAFETY: forwarded from this function's own safety doc.
    drop(unsafe { Box::from_raw(b) });
}

/// Unreference a blob: decrements the reference count and frees the
/// blob when it becomes zero (`tv_blob_unref`).
///
/// # Safety
/// `b`, if non-null, must be a valid pointer previously returned by
/// [`tv_blob_alloc`], not yet freed.
pub unsafe fn tv_blob_unref(b: *mut crate::eval::typval_defs::BlobT) {
    if b.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*b).bv_refcount -= 1 };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*b).bv_refcount } <= 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_blob_free(b) };
    }
}

/// Get the length of the data in the blob, in bytes (`tv_blob_len`,
/// `eval/typval.h`'s own `static inline`).
///
/// # Safety
/// `b`, if non-null, must be a valid pointer to a live
/// [`crate::eval::typval_defs::BlobT`].
#[must_use]
pub unsafe fn tv_blob_len(b: *const crate::eval::typval_defs::BlobT) -> i32 {
    if b.is_null() {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*b).bv_ga.ga_len }
}

/// Check that `n1` is a valid index into a blob of length `bloblen`
/// (`tv_blob_check_index`).
///
/// Drops the original's `quiet` effect on the return value (it only
/// ever gates the omitted `semsg()` message text, matching this
/// crate's established `_quiet`/`_verbose`-parameter precedent, e.g.
/// [`crate::eval::vars::eval_variable`]'s own `_verbose` parameter) -
/// named `_quiet` here for the same reason.
#[must_use]
pub fn tv_blob_check_index(bloblen: i32, n1: crate::eval::typval_defs::VarnumberT, _quiet: bool) -> i32 {
    if n1 < 0 || n1 > crate::eval::typval_defs::VarnumberT::from(bloblen) {
        return FAIL;
    }
    OK
}

/// Check that `n1..=n2` is a valid range for a blob of length
/// `bloblen` (`tv_blob_check_range`). See [`tv_blob_check_index`]'s
/// own doc comment for why `quiet` is renamed `_quiet`.
#[must_use]
pub fn tv_blob_check_range(
    bloblen: i32,
    n1: crate::eval::typval_defs::VarnumberT,
    n2: crate::eval::typval_defs::VarnumberT,
    _quiet: bool,
) -> i32 {
    if n2 < 0 || n2 >= crate::eval::typval_defs::VarnumberT::from(bloblen) || n2 < n1 {
        return FAIL;
    }
    OK
}

/// Get one byte from a blob (`tv_blob_get`, `eval/typval.h`'s own
/// `static inline`).
///
/// # Safety
/// `b` must be a valid, non-null pointer to a live
/// [`crate::eval::typval_defs::BlobT`], and `idx` must be in bounds
/// (`< tv_blob_len(b)`).
#[must_use]
pub unsafe fn tv_blob_get(b: *const crate::eval::typval_defs::BlobT, idx: i32) -> u8 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (&(*b).bv_ga.ga_data)[idx as usize] }
}

/// Set byte `idx` of `b`, growing the blob by one when `idx` is
/// exactly the current end (`tv_blob_set_append`).
///
/// Appending is allowed ONLY at the immediate end. Setting a byte
/// anywhere beyond that is silently ignored rather than growing the
/// blob to reach it, which would leave uninitialized bytes behind.
///
/// # Safety
/// `b` must be a valid, non-null pointer to a live
/// [`crate::eval::typval_defs::BlobT`] for the whole call.
pub unsafe fn tv_blob_set_append(b: *mut crate::eval::typval_defs::BlobT, idx: i32, byte: u8) {
    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { tv_blob_len(b) };

    // Allow for appending a byte; setting a byte beyond the end is an
    // error otherwise.
    if idx > len {
        return;
    }
    if idx == len {
        // Grow by one. `ga_len` is the authoritative length that
        // tv_blob_len reports, so it must move with `ga_data`.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            (*b).bv_ga.ga_data.push(0);
            (*b).bv_ga.ga_len += 1;
        }
    }
    // SAFETY: idx is now guaranteed < tv_blob_len(b).
    unsafe { tv_blob_set(b, idx, byte) };
}

/// Store a byte at index `idx` in a blob (`tv_blob_set`, `eval/typval.h`'s
/// own `static inline`).
///
/// # Safety
/// `b` must be a valid, non-null pointer to a live
/// [`crate::eval::typval_defs::BlobT`], and `idx` must be in bounds
/// (`< tv_blob_len(b)`).
pub unsafe fn tv_blob_set(b: *mut crate::eval::typval_defs::BlobT, idx: i32, c: u8) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (&mut (*b).bv_ga.ga_data)[idx as usize] = c };
}

/// Set the return value of `tv` to a blob (`tv_blob_set_ret`,
/// `eval/typval.h`'s own `static inline`).
///
/// # Safety
/// `b`, if non-null, must be a valid pointer to a live
/// [`crate::eval::typval_defs::BlobT`].
pub unsafe fn tv_blob_set_ret(tv: &mut TypvalT, b: *mut crate::eval::typval_defs::BlobT) {
    tv.value = TypvalValue::Blob(b);
    if !b.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*b).bv_refcount += 1 };
    }
}

/// Allocate an empty blob for a return value, setting its reference
/// count (`tv_blob_alloc_ret`).
pub fn tv_blob_alloc_ret(ret_tv: &mut TypvalT) -> *mut crate::eval::typval_defs::BlobT {
    let b = tv_blob_alloc();
    // SAFETY: `b` was just allocated above, a fresh pointer not shared
    // with anything yet.
    unsafe { tv_blob_set_ret(ret_tv, b) };
    b
}

/// Return a slice of `blob` from index `n1` to `n2` in `rettv`. The
/// length of the blob is `len`. Returns an empty blob if the indexes
/// are out of range (`tv_blob_slice`).
///
/// # Safety
/// `blob` must be a valid pointer to the live [`crate::eval::typval_defs::BlobT`]
/// currently held by `rettv.value` (the original reads `rettv->vval.v_blob`
/// directly inside the loop, so this crate's translation takes the
/// same pointer explicitly rather than re-reading it from `rettv` each
/// time - the caller must ensure both refer to the same blob).
fn tv_blob_slice(
    blob: *const crate::eval::typval_defs::BlobT,
    len: i32,
    n1: crate::eval::typval_defs::VarnumberT,
    n2: crate::eval::typval_defs::VarnumberT,
    exclusive: bool,
    rettv: &mut TypvalT,
) -> i32 {
    let len = crate::eval::typval_defs::VarnumberT::from(len);
    let mut n1 = n1;
    let mut n2 = n2;
    if n1 < 0 {
        n1 += len;
        if n1 < 0 {
            n1 = 0;
        }
    }
    if n2 < 0 {
        n2 += len;
    } else if n2 >= len {
        n2 = len - crate::eval::typval_defs::VarnumberT::from(!exclusive);
    }
    if exclusive {
        n2 -= 1;
    }
    if n1 >= len || n2 < 0 || n1 > n2 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(rettv) };
        rettv.value = TypvalValue::Blob(std::ptr::null_mut());
    } else {
        let new_blob = tv_blob_alloc();
        let new_len = (n2 - n1 + 1) as i32;
        // SAFETY: `new_blob` was just allocated above, a fresh pointer
        // not shared with anything yet.
        unsafe {
            (*new_blob).bv_ga.ga_data = vec![0u8; new_len as usize];
            (*new_blob).bv_ga.ga_len = new_len;
            (*new_blob).bv_ga.ga_maxlen = new_len;
        }
        for i in n1..=n2 {
            // SAFETY: forwarded from this function's own safety doc -
            // `blob` and `rettv.value`'s own blob pointer are the same
            // live object.
            let byte = unsafe { tv_blob_get(blob, i as i32) };
            // SAFETY: `new_blob` was just sized above to hold exactly
            // `new_len` bytes, and `i - n1` ranges over `0..new_len`.
            unsafe { tv_blob_set(new_blob, (i - n1) as i32, byte) };
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(rettv) };
        // SAFETY: `new_blob` was just allocated above, a fresh pointer
        // not shared with anything yet.
        unsafe { tv_blob_set_ret(rettv, new_blob) };
    }

    OK
}

/// Return the byte value in `blob` at index `idx` in `rettv`. If the
/// index is too big or negative that is an error. The length of the
/// blob is `len` (`tv_blob_index`).
///
/// The original's own `semsg(_(e_blobidx), idx)` is omitted, matching
/// this crate's established "skip the display, keep the identical
/// FAIL" policy.
///
/// # Safety
/// Same as `tv_blob_slice`.
fn tv_blob_index(
    blob: *const crate::eval::typval_defs::BlobT,
    len: i32,
    idx: crate::eval::typval_defs::VarnumberT,
    rettv: &mut TypvalT,
) -> i32 {
    let len = crate::eval::typval_defs::VarnumberT::from(len);
    let mut idx = idx;
    if idx < 0 {
        idx += len;
    }
    if idx < len && idx >= 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let v = i64::from(unsafe { tv_blob_get(blob, idx as i32) });
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(rettv) };
        rettv.value = TypvalValue::Number(v);
    } else {
        return FAIL;
    }

    OK
}

/// Apply `[idx]`/`[n1:n2]` indexing or slicing to a `Blob`, dispatching
/// to `tv_blob_slice`/`tv_blob_index` (`tv_blob_slice_or_index`).
///
/// # Safety
/// `blob` must be a valid pointer to the same live blob currently held
/// by `rettv.value` (matching the original's own
/// `tv_blob_len(rettv->vval.v_blob)` self-read).
pub unsafe fn tv_blob_slice_or_index(
    blob: *const crate::eval::typval_defs::BlobT,
    is_range: bool,
    n1: crate::eval::typval_defs::VarnumberT,
    n2: crate::eval::typval_defs::VarnumberT,
    exclusive: bool,
    rettv: &mut TypvalT,
) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { tv_blob_len(blob) };

    if is_range {
        tv_blob_slice(blob, len, n1, n2, exclusive, rettv)
    } else {
        tv_blob_index(blob, len, n1, rettv)
    }
}

/// Copy a blob typval to a different typval (`tv_blob_copy`).
///
/// # Safety
/// `from`, if non-null, must be a valid pointer to a live
/// [`crate::eval::typval_defs::BlobT`].
pub unsafe fn tv_blob_copy(from: *const crate::eval::typval_defs::BlobT, to: &mut TypvalT) {
    if from.is_null() {
        to.value = TypvalValue::Blob(std::ptr::null_mut());
    } else {
        let b = tv_blob_alloc_ret(to);
        // SAFETY: forwarded from this function's own safety doc.
        let (data, len) = unsafe { ((*from).bv_ga.ga_data.clone(), (*from).bv_ga.ga_len) };
        // SAFETY: `b` was just allocated above by `tv_blob_alloc_ret`.
        unsafe {
            (*b).bv_ga.ga_data = data;
            (*b).bv_ga.ga_len = len;
            (*b).bv_ga.ga_maxlen = len;
        }
    }
    to.v_lock = VarLockStatus::Unlocked;
}

/// `remove({blob}, {idx} [, {end}])` - the `Blob` case of `remove()`
/// (`tv_blob_remove`).
///
/// Removes and returns a single byte at `{idx}` (as a `Number`), or,
/// when `{end}` is also given, removes and returns every byte from
/// `{idx}` to `{end}` (inclusive) as a NEW blob. Both `{idx}`/`{end}`
/// may be negative (counting from the end), matching the original's
/// own `idx = len + idx` adjustment exactly.
///
/// A null `b` is never dereferenced for its own lock check (matching
/// the original's own short-circuiting `b != NULL && value_check_lock(...)`)
/// but otherwise flows through the SAME logic as any other blob -
/// [`tv_blob_len`] already returns `0` for a null blob, which makes
/// every possible `{idx}` fail the immediately-following range check
/// naturally, without a separate special case.
///
/// # Safety
/// `argvars[0].value` must be `Blob`-typed; if its pointer is
/// non-null, it must be valid.
pub unsafe fn tv_blob_remove(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let TypvalValue::Blob(b) = argvars[0].value else { unreachable!() };

    if !b.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let locked = unsafe { (*b).bv_lock };
        if value_check_lock(locked, None) {
            return;
        }
    }

    let mut error = false;
    let mut idx = tv_get_number_chk(&argvars[1], Some(&mut error));
    if error {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let len = i64::from(unsafe { tv_blob_len(b) });
    if idx < 0 {
        idx += len; // count from the end.
    }
    if idx < 0 || idx >= len {
        return; // semsg(_(e_blobidx), idx) omitted.
    }

    if argvars.len() <= 2 {
        // Remove one item, return its value.
        // SAFETY: forwarded from this function's own safety doc.
        let byte = unsafe { tv_blob_get(b, idx as i32) };
        rettv.value = TypvalValue::Number(i64::from(byte));
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            let ga_data = &mut (*b).bv_ga.ga_data;
            for i in idx..len - 1 {
                ga_data[i as usize] = ga_data[(i + 1) as usize];
            }
            (*b).bv_ga.ga_len -= 1;
        }
        return;
    }

    // Remove range of items, return blob with values.
    let mut error2 = false;
    let mut end = tv_get_number_chk(&argvars[2], Some(&mut error2));
    if error2 {
        return;
    }
    if end < 0 {
        end += len; // count from the end.
    }
    if end >= len || idx > end {
        return; // semsg(_(e_blobidx), end) omitted.
    }

    let cnt = (end - idx + 1) as usize;
    // SAFETY: forwarded from this function's own safety doc.
    let copied: Vec<u8> = unsafe { (&(*b).bv_ga.ga_data)[idx as usize..idx as usize + cnt].to_vec() };
    let blob = tv_blob_alloc();
    // SAFETY: `blob` was just allocated above, a fresh pointer not
    // shared with anything yet.
    unsafe {
        (*blob).bv_ga.ga_data = copied;
        (*blob).bv_ga.ga_len = cnt as i32;
        (*blob).bv_ga.ga_maxlen = cnt as i32;
        tv_blob_set_ret(rettv, blob);
    }
    // Shift the remaining tail (after `end`) left to fill the gap.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        let ga_data = &mut (*b).bv_ga.ga_data;
        for i in (end + 1)..len {
            ga_data[(idx + i - end - 1) as usize] = ga_data[i as usize];
        }
        (*b).bv_ga.ga_len -= cnt as i32;
    }
}

/// Test-only accessor: `true` if no `List` is currently linked into
/// the shared `GC_FIRST_LIST` registry - see
/// [`gc_first_dict_is_empty`]'s own doc comment for why this exists.
#[cfg(test)]
pub(crate) fn gc_first_list_is_empty() -> bool {
    // SAFETY: GC_FIRST_LIST is only ever read/written through this
    // accessor and the crate's own established `global_state_test_lock()`
    // discipline, matching every other read site in this module.
    unsafe { *GC_FIRST_LIST.get_mut() }.is_null()
}

/// Allocate an empty list. Caller should take care of the reference
/// count (`tv_list_alloc`).
///
/// `_len` (expected number of items to be populated before the list
/// becomes accessible from Vimscript) is accepted for signature
/// fidelity but unused, matching the original's own "currently does
/// nothing" note.
#[must_use]
pub fn tv_list_alloc(_len: isize) -> *mut crate::eval::typval_defs::ListT {
    let list = Box::into_raw(Box::new(crate::eval::typval_defs::ListT {
        lv_first: std::ptr::null_mut(),
        lv_last: std::ptr::null_mut(),
        lv_watch: std::ptr::null_mut(),
        lv_idx_item: std::ptr::null_mut(),
        lv_copylist: std::ptr::null_mut(),
        lv_used_next: std::ptr::null_mut(),
        lv_used_prev: std::ptr::null_mut(),
        lv_refcount: 0,
        lv_len: 0,
        lv_idx: 0,
        lv_copy_id: 0,
        lv_lock: VarLockStatus::Unlocked,
        lua_table_ref: LUA_NOREF,
    }));

    // Prepend the list to the list of lists for garbage collection.
    // SAFETY: GC_FIRST_LIST is only ever read/written through this
    // module's own functions, which never hold a live reference across
    // another call into this same cell.
    let gc_first = unsafe { *GC_FIRST_LIST.get_mut() };
    if !gc_first.is_null() {
        // SAFETY: gc_first is either null (checked above) or a live
        // pointer previously produced by this same function.
        unsafe { (*gc_first).lv_used_prev = list };
    }
    // SAFETY: forwarded from this function's own reasoning above.
    unsafe { (*list).lv_used_next = gc_first };
    // SAFETY: forwarded from this function's own reasoning above.
    unsafe { *GC_FIRST_LIST.get_mut() = list };

    list
}

/// Allocate an empty list and put it in `ret_tv`, incrementing its
/// reference count and unlocking `ret_tv` (`tv_list_alloc_ret`).
///
/// # Safety
/// None beyond [`tv_list_alloc`]'s own (always-safe) contract - this
/// function only ever writes into `ret_tv`, a plain `&mut TypvalT`.
#[must_use]
pub unsafe fn tv_list_alloc_ret(ret_tv: &mut TypvalT, len: isize) -> *mut crate::eval::typval_defs::ListT {
    let l = tv_list_alloc(len);
    // SAFETY: `l` was just allocated above, a fresh pointer not shared
    // with anything yet.
    unsafe { tv_list_set_ret(ret_tv, l) };
    ret_tv.v_lock = VarLockStatus::Unlocked;
    l
}

/// Allocate a list item. The type/value of the item (`.li_tv`) still
/// need to be initialized by the caller (`tv_list_item_alloc`).
///
/// The original's own item is a bare, uninitialized `xmalloc` (with a
/// warning to initialize `li_tv`/`li_next`/`li_prev` immediately
/// afterward) - this translation instead starts every field at a real
/// default value, since Rust has no safe equivalent of returning
/// genuinely uninitialized memory, and every real call site already
/// overwrites these fields immediately anyway.
fn tv_list_item_alloc() -> *mut crate::eval::typval_defs::ListitemT {
    Box::into_raw(Box::new(crate::eval::typval_defs::ListitemT {
        li_next: std::ptr::null_mut(),
        li_prev: std::ptr::null_mut(),
        li_tv: TypvalT::default(),
    }))
}

/// Get the first item of a list, or `None` if `l` is null or empty
/// (`tv_list_first`, `eval/typval.h`'s own `static inline`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT`.
#[must_use]
pub unsafe fn tv_list_first(l: *const crate::eval::typval_defs::ListT) -> *mut crate::eval::typval_defs::ListitemT {
    if l.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*l).lv_first }
}

// Indexing/searching:

/// Normalize an index: negative counts from the end, out-of-range
/// becomes `-1` (`tv_list_uidx`, `eval/typval.h`'s own `static
/// inline`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT`.
#[must_use]
pub unsafe fn tv_list_uidx(l: *const crate::eval::typval_defs::ListT, n: i32) -> i32 {
    // Negative index is relative to the end.
    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { tv_list_len(l) };
    let n = if n < 0 { n + len } else { n };

    // Check for index out of range.
    if n < 0 || n >= len {
        return -1;
    }
    n
}

/// Locate item with a given index in a list and return it, or null if
/// `n` is out of range (`tv_list_find`).
///
/// Caches the found index/item in `l.lv_idx`/`l.lv_idx_item`, and
/// searches outward from the closest of {start, cached index, end} -
/// matching the original's own performance optimization exactly.
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT` whose
/// `lv_idx_item`, if non-null, is reachable via `lv_first`'s own
/// `li_next` chain.
#[must_use]
pub unsafe fn tv_list_find(
    l: *mut crate::eval::typval_defs::ListT,
    n: i32,
) -> *mut crate::eval::typval_defs::ListitemT {
    if l.is_null() {
        return std::ptr::null_mut();
    }

    // SAFETY: forwarded from this function's own safety doc.
    let n = unsafe { tv_list_uidx(l, n) };
    if n == -1 {
        return std::ptr::null_mut();
    }

    // SAFETY: forwarded from this function's own safety doc.
    let list = unsafe { &mut *l };
    let (mut item, mut idx) = if !list.lv_idx_item.is_null() {
        if n < list.lv_idx / 2 {
            // Closest to the start of the list.
            (list.lv_first, 0)
        } else if n > (list.lv_idx + list.lv_len) / 2 {
            // Closest to the end of the list.
            (list.lv_last, list.lv_len - 1)
        } else {
            // Closest to the cached index.
            (list.lv_idx_item, list.lv_idx)
        }
    } else if n < list.lv_len / 2 {
        // Closest to the start of the list.
        (list.lv_first, 0)
    } else {
        // Closest to the end of the list.
        (list.lv_last, list.lv_len - 1)
    };

    while n > idx {
        // Search forward.
        // SAFETY: forwarded from this function's own safety doc.
        item = unsafe { (*item).li_next };
        idx += 1;
    }
    while n < idx {
        // Search backward.
        // SAFETY: forwarded from this function's own safety doc.
        item = unsafe { (*item).li_prev };
        idx -= 1;
    }

    // Cache the used index.
    list.lv_idx = idx;
    list.lv_idx_item = item;

    item
}

/// Get list item `l[n]` as a number (`tv_list_find_nr`).
///
/// # Safety
/// Same as [`tv_list_find`].
pub unsafe fn tv_list_find_nr(
    l: *mut crate::eval::typval_defs::ListT,
    n: i32,
    ret_error: Option<&mut bool>,
) -> crate::eval::typval_defs::VarnumberT {
    // SAFETY: forwarded from this function's own safety doc.
    let li = unsafe { tv_list_find(l, n) };
    if li.is_null() {
        if let Some(e) = ret_error {
            *e = true;
        }
        return -1;
    }
    // SAFETY: forwarded from this function's own safety doc.
    tv_get_number_chk(unsafe { &(*li).li_tv }, ret_error)
}

/// Get list item `l[n]` as a string, or `None` on error (`out of
/// range` - real, reachable error, message display skipped, matching
/// this module's established policy) (`tv_list_find_str`).
///
/// # Safety
/// Same as [`tv_list_find`].
#[must_use]
pub unsafe fn tv_list_find_str(l: *mut crate::eval::typval_defs::ListT, n: i32) -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    let li = unsafe { tv_list_find(l, n) };
    if li.is_null() {
        // semsg(_(e_list_index_out_of_range_nr), n) omitted - see this
        // module's own doc comment.
        return None;
    }
    // SAFETY: forwarded from this function's own safety doc.
    Some(tv_get_string(unsafe { &(*li).li_tv }))
}

/// Like [`tv_list_find`], but when a negative index is used that is
/// not found, use zero and set `idx` to zero. Used for the first
/// index of a range (`tv_list_find_index`).
///
/// # Safety
/// Same as [`tv_list_find`].
fn tv_list_find_index(l: *mut crate::eval::typval_defs::ListT, idx: &mut i32) -> *mut crate::eval::typval_defs::ListitemT {
    // SAFETY: forwarded from this function's own safety doc.
    let li = unsafe { tv_list_find(l, *idx) };
    if !li.is_null() {
        return li;
    }

    if *idx < 0 {
        *idx = 0;
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { tv_list_find(l, *idx) };
    }
    li
}

/// Check that `n1` is a valid index for the (only or first) index of
/// a range into list `l`, normalizing a negative index to positive;
/// returns the found item, or null on an out-of-range index
/// (`tv_list_check_range_index_one`). See [`tv_blob_check_index`]'s
/// own doc comment for why `quiet` is renamed `_quiet`.
///
/// # Safety
/// Same as `tv_list_find_index`/[`tv_list_find`].
#[must_use]
pub unsafe fn tv_list_check_range_index_one(
    l: *mut crate::eval::typval_defs::ListT,
    n1: &mut i32,
    _quiet: bool,
) -> *mut crate::eval::typval_defs::ListitemT {
    tv_list_find_index(l, n1)
}

/// Check that `n2` can be used as the second index in a range of list
/// `l`. If `n1`/`n2` is negative it is changed to the positive index.
/// `li1` is the item for item `n1` (`tv_list_check_range_index_two`).
/// See [`tv_blob_check_index`]'s own doc comment for why `quiet` is
/// renamed `_quiet`.
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT` whose
/// every item is reachable via `lv_first`'s own `li_next` chain;
/// `li1`, if non-null, must be one of those items.
pub unsafe fn tv_list_check_range_index_two(
    l: *mut crate::eval::typval_defs::ListT,
    n1: &mut i32,
    li1: *const crate::eval::typval_defs::ListitemT,
    n2: &mut i32,
    _quiet: bool,
) -> i32 {
    if *n2 < 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let ni = unsafe { tv_list_find(l, *n2) };
        if ni.is_null() {
            return FAIL;
        }
        // SAFETY: forwarded from this function's own safety doc.
        *n2 = unsafe { tv_list_idx_of_item(l, ni) };
    }

    // Check that n2 isn't before n1.
    if *n1 < 0 {
        // SAFETY: forwarded from this function's own safety doc.
        *n1 = unsafe { tv_list_idx_of_item(l, li1) };
    }
    if *n2 < *n1 {
        return FAIL;
    }
    OK
}

/// Locate `item` in a list and return its index, or `-1` if not found
/// (`tv_list_idx_of_item`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT`.
#[must_use]
pub unsafe fn tv_list_idx_of_item(
    l: *const crate::eval::typval_defs::ListT,
    item: *const crate::eval::typval_defs::ListitemT,
) -> i32 {
    if l.is_null() || item.is_null() {
        return -1;
    }
    let mut idx = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let mut li = unsafe { (*l).lv_first };
    while !li.is_null() && !std::ptr::eq(li, item) {
        // SAFETY: forwarded from this function's own safety doc.
        li = unsafe { (*li).li_next };
        idx += 1;
    }
    if li.is_null() {
        return -1;
    }
    idx
}

/// Reverse list in-place (`tv_list_reverse`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT` whose
/// every item is reachable via `lv_first`'s own `li_next` chain.
pub unsafe fn tv_list_reverse(l: *mut crate::eval::typval_defs::ListT) {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { tv_list_len(l) } <= 1 {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let list = unsafe { &mut *l };
    std::mem::swap(&mut list.lv_first, &mut list.lv_last);

    let mut li = list.lv_first;
    while !li.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { std::mem::swap(&mut (*li).li_next, &mut (*li).li_prev) };
        // SAFETY: forwarded from this function's own safety doc.
        li = unsafe { (*li).li_next };
    }

    list.lv_idx = list.lv_len - list.lv_idx - 1;
}

/// Parsed options for `sort()`/`uniq()`'s optional 2nd/3rd arguments
/// (`sortinfo_T`, `eval/typval.c`'s own file-static-by-pointer struct -
/// translated as a plain by-value struct instead, since Rust's
/// `slice::sort_by` (used by [`do_sort`]) can receive extra context
/// via a capturing closure, unlike C's raw `qsort` callback (which has
/// no spare "userdata" parameter, hence the original's own need for a
/// `sortinfo_T *sortinfo` file-static for `item_compare`/
/// `item_compare2` to reach back into) - no such static is needed
/// here at all).
#[derive(Default)]
struct SortInfo {
    item_compare_ic: bool,
    item_compare_lc: bool,
    item_compare_numeric: bool,
    item_compare_numbers: bool,
    item_compare_float: bool,
    /// `true` when a custom comparator (a `Funcref`/`Partial`, or an
    /// unrecognized non-empty string naming a function) was
    /// requested. [`do_sort`]/[`do_uniq`] `unimplemented!()` the
    /// moment they would actually need to CALL it (needs the full
    /// `call_func`/`funcexe_T` machinery, not yet translated) rather
    /// than here at parse time - matching the original's own exact
    /// timing (a custom comparator is only ever invoked lazily, once
    /// per comparison, never during argument parsing itself).
    has_custom_comparator: bool,
}

/// Parse the optional 2nd/3rd arguments to `sort()`/`uniq()`
/// (`parse_sort_uniq_args`).
///
/// The original's own `emsg()`/`semsg()` calls for a genuine type
/// error are omitted, matching this crate's established "skip the
/// display, keep the identical `FAIL`" policy.
fn parse_sort_uniq_args(argvars: &[TypvalT]) -> Result<SortInfo, ()> {
    let mut info = SortInfo::default();

    let Some(arg1) = argvars.get(1) else {
        return Ok(info);
    };
    if matches!(arg1.value, TypvalValue::Unknown) {
        return Ok(info);
    }

    match &arg1.value {
        TypvalValue::Func(_) | TypvalValue::Partial(_) => {
            info.has_custom_comparator = true;
        }
        TypvalValue::Number(nr) => {
            if *nr == 1 {
                info.item_compare_ic = true;
            } else if *nr != 0 {
                return Err(());
            }
        }
        _ => {
            let mut error = false;
            let nr = tv_get_number_chk(arg1, Some(&mut error));
            if error {
                return Err(());
            }
            if nr == 1 {
                info.item_compare_ic = true;
            } else {
                // Not a number at all (already excluded VAR_NUMBER
                // above) - a non-numeric second argument names a
                // custom comparator, unless it's one of the 5
                // recognized single-letter shorthand flags.
                let s = tv_get_string(arg1);
                if s.is_empty() {
                    // Empty string means default sort - nothing to do.
                } else if s == b"n" {
                    info.item_compare_numeric = true;
                } else if s == b"N" {
                    info.item_compare_numbers = true;
                } else if s == b"f" {
                    info.item_compare_float = true;
                } else if s == b"i" {
                    info.item_compare_ic = true;
                } else if s == b"l" {
                    info.item_compare_lc = true;
                } else {
                    info.has_custom_comparator = true;
                }
            }
        }
    }

    if argvars.len() > 2 && !matches!(argvars[2].value, TypvalValue::Unknown) {
        // Optional 3rd argument: {dict} (`item_compare_selfdict`) -
        // only matters when actually calling a custom comparator
        // (tracked via `has_custom_comparator` instead of a separate
        // field, since nothing else ever reads it).
        if tv_check_for_dict_arg(argvars, 2) == FAIL {
            return Err(());
        }
    }

    Ok(info)
}

/// The default (non-custom-function) comparator for `sort()`/`uniq()`
/// (`item_compare`).
///
/// The original's own "break ties by original index" step
/// (`item_compare`'s own `keep_zero` parameter, used only by
/// [`do_sort`]) is omitted entirely: Rust's `slice::sort_by` is
/// documented as STABLE, which already preserves the original
/// relative order of any two comparison-equal items - exactly the
/// same observable effect the original's own index-based tie-break
/// achieves by hand. [`do_uniq`]'s own real need for a genuine `0`
/// result (to detect adjacent duplicates) is unaffected either way,
/// since it never asked for tie-breaking in the first place.
///
/// The original's own `strcoll` (locale-aware comparison,
/// `item_compare_lc`) is treated identically to a plain byte
/// comparison: this crate has no real locale-switching mechanism, so
/// every session behaves as if under the default "C" locale, under
/// which `strcoll` and `strcmp` already agree exactly.
fn item_compare(tv1: &TypvalT, tv2: &TypvalT, info: &SortInfo) -> i32 {
    if info.item_compare_numbers {
        let v1 = tv_get_number(tv1);
        let v2 = tv_get_number(tv2);
        return if v1 == v2 {
            0
        } else if v1 > v2 {
            1
        } else {
            -1
        };
    }

    if info.item_compare_float {
        let v1 = tv_get_float(tv1);
        let v2 = tv_get_float(tv2);
        return if v1 == v2 {
            0
        } else if v1 > v2 {
            1
        } else {
            -1
        };
    }

    // encode_tv2string() puts quotes around a string and allocates -
    // don't do that for string VALUES themselves. Use a single quote
    // when comparing with a non-string, to do what the docs promise.
    let p1: Vec<u8> = if let TypvalValue::String(_) = tv1.value {
        if !matches!(tv2.value, TypvalValue::String(_)) || info.item_compare_numeric {
            vec![b'\'']
        } else {
            tv_get_string(tv1)
        }
    } else {
        // SAFETY: only reads tv1, no ownership concerns.
        unsafe { crate::eval::encode::encode_tv2string(tv1) }
    };
    let p2: Vec<u8> = if let TypvalValue::String(_) = tv2.value {
        if !matches!(tv1.value, TypvalValue::String(_)) || info.item_compare_numeric {
            vec![b'\'']
        } else {
            tv_get_string(tv2)
        }
    } else {
        // SAFETY: only reads tv2, no ownership concerns.
        unsafe { crate::eval::encode::encode_tv2string(tv2) }
    };

    if !info.item_compare_numeric {
        crate::mbyte::mb_strcmp_ic(info.item_compare_ic, &p1, &p2)
    } else {
        let n1 = crate::eval::eval::string2float(&p1).0;
        let n2 = crate::eval::eval::string2float(&p2).0;
        if n1 == n2 {
            0
        } else if n1 > n2 {
            1
        } else {
            -1
        }
    }
}

/// Sort a list in place using `info`'s comparison rules (`do_sort`).
///
/// The original's own array-of-pointers + `qsort` + rebuild-the-
/// linked-list dance is simplified: collect each item's raw pointer
/// into a `Vec`, `sort_by` (Rust's own guaranteed-stable sort - see
/// [`item_compare`]'s own doc comment for why this makes the
/// original's manual index-based tie-break unnecessary), then
/// re-append each item (via the already-real [`tv_list_append`],
/// which re-links an EXISTING item rather than allocating a new one)
/// in the new order.
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`.
unsafe fn do_sort(l: *mut crate::eval::typval_defs::ListT, info: &SortInfo) {
    if info.has_custom_comparator {
        unimplemented!(
            "do_sort: calling a user-supplied comparator (Funcref/Partial/named function - \
             item_compare2, needs the full call_func/funcexe_T machinery) is not yet translated"
        );
    }

    let mut items: Vec<*mut crate::eval::typval_defs::ListitemT> = Vec::new();
    // SAFETY: forwarded from this function's own safety doc.
    let mut cur = unsafe { tv_list_first(l) };
    while !cur.is_null() {
        items.push(cur);
        // SAFETY: `cur` is a live item currently linked into `l`.
        cur = unsafe { (*cur).li_next };
    }

    items.sort_by(|&a, &b| {
        // SAFETY: both are live items currently linked into `l`.
        let (tv1, tv2) = unsafe { (&(*a).li_tv, &(*b).li_tv) };
        match item_compare(tv1, tv2, info) {
            n if n < 0 => std::cmp::Ordering::Less,
            0 => std::cmp::Ordering::Equal,
            _ => std::cmp::Ordering::Greater,
        }
    });

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*l).lv_first = std::ptr::null_mut();
        (*l).lv_last = std::ptr::null_mut();
        (*l).lv_idx_item = std::ptr::null_mut();
        (*l).lv_len = 0;
    }
    for item in items {
        // SAFETY: every item was detached above (the list's own head/
        // tail/count were just cleared) before this loop re-appends
        // each one in sorted order; forwarded from this function's
        // own safety doc for `l` itself.
        unsafe { tv_list_append(l, item) };
    }
}

/// Remove adjacent duplicate items from a list in place, using
/// `info`'s comparison rules (`do_uniq`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`.
unsafe fn do_uniq(l: *mut crate::eval::typval_defs::ListT, info: &SortInfo) {
    if info.has_custom_comparator {
        unimplemented!(
            "do_uniq: calling a user-supplied comparator (Funcref/Partial/named function - \
             item_compare2, needs the full call_func/funcexe_T machinery) is not yet translated"
        );
    }

    // SAFETY: forwarded from this function's own safety doc.
    let first = unsafe { tv_list_first(l) };
    if first.is_null() {
        return;
    }
    // SAFETY: `first` is a live item currently linked into `l`.
    let mut li = unsafe { (*first).li_next };
    while !li.is_null() {
        // SAFETY: `li` is live and non-first, so it has a predecessor.
        let prev_li = unsafe { (*li).li_prev };
        // SAFETY: both are live items currently linked into `l`.
        let equal = unsafe { item_compare(&(*prev_li).li_tv, &(*li).li_tv, info) } == 0;
        if equal {
            // SAFETY: forwarded from this function's own safety doc.
            li = unsafe { tv_list_item_remove(l, li) };
        } else {
            // SAFETY: `li` is still live.
            li = unsafe { (*li).li_next };
        }
    }
}

/// The shared `sort()`/`uniq()` implementation (`do_sort_uniq`).
///
/// The original's own `semsg`/`emsg` calls (wrong-arg-type, locked-
/// list) are omitted, matching this crate's established "skip the
/// display, keep the identical state/return" policy.
///
/// # Safety
/// Forwards `do_sort`/`do_uniq`'s own safety docs for
/// `argvars[0]`'s `List`, once confirmed non-null.
pub unsafe fn do_sort_uniq(argvars: &[TypvalT], rettv: &mut TypvalT, sort: bool) {
    let TypvalValue::List(l) = &argvars[0].value else {
        return;
    };
    let l = *l;

    // SAFETY: forwarded from this function's own safety doc.
    if value_check_lock(unsafe { tv_list_locked(l) }, None) {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_set_ret(rettv, l) };

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { tv_list_len(l) } <= 1 {
        return; // short list sorts pretty quickly
    }

    let Ok(info) = parse_sort_uniq_args(argvars) else {
        return;
    };

    if sort {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { do_sort(l, &info) };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { do_uniq(l, &info) };
    }
}

/// Which of `map()`/`mapnew()`/`filter()`/`foreach()` a `filter_map`-
/// family function is being used for (`filtermap_T`, `eval/list.c`'s
/// own header - no dedicated `_defs.rs` module exists for `list.c`, so
/// this small enum is embedded directly here, same treatment as
/// `charset.h`'s `vim_isbreak` in `charset.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMapT {
    /// `map()` - replace each item in place.
    Map,
    /// `mapnew()` - build and return a new container, leaving the
    /// original untouched.
    MapNew,
    /// `filter()` - remove items in place for which `{expr2}` is
    /// falsy.
    Filter,
    /// `foreach()` - call `{expr2}` for its side effect only, the
    /// original container/its own first argument is always returned
    /// unchanged.
    Foreach,
}

/// Handle one item for `map()`/`filter()`/`foreach()`. Sets `v:val` to
/// `tv`. Caller must set `v:key` (`filter_map_one`).
///
/// `foreach()` given a raw command String needs `do_cmdline_cmd` (the
/// Ex-command execution engine) and remains explicit at that boundary.
/// Funcref callbacks use the real `eval_expr_typval` path.
///
/// Returns `(status, remove)` - `remove` is only meaningful for
/// [`FilterMapT::Filter`] (whether `{expr2}` evaluated to zero/falsy).
///
/// # Safety
/// `tv`/`expr` must be valid; forwards `tv_copy`/`eval_expr_typval`/
/// `tv_get_number_chk`/`tv_clear_simple`'s own safety requirements.
pub unsafe fn filter_map_one(
    tv: &TypvalT,
    expr: &TypvalT,
    filtermap: FilterMapT,
) -> (i32, TypvalT, bool) {
    use crate::eval::eval::eval_expr_typval;
    use crate::eval::vars::{get_vim_var_tv, VimVarIndex};

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_copy(tv, &mut *get_vim_var_tv(VimVarIndex::Val)) };

    let mut newtv = TypvalT::default();
    let mut remove = false;

    if filtermap == FilterMapT::Foreach && matches!(expr.value, TypvalValue::String(_)) {
        unimplemented!(
            "filter_map_one: foreach() given a raw command string needs do_cmdline_cmd, not yet \
             translated"
        );
    }

    // The original copies v:key/v:val into a two-item argument array
    // without incrementing references. TypvalT::clone duplicates
    // owned String bytes while preserving the same borrowed pointer
    // semantics for container variants; this array is never cleared
    // as an owner by the callback dispatcher.
    let mut argv = [
        unsafe { (&*get_vim_var_tv(VimVarIndex::Key)).clone() },
        unsafe { (&*get_vim_var_tv(VimVarIndex::Val)).clone() },
    ];
    // SAFETY: forwarded from this function's own safety doc.
    let ret =
        unsafe { eval_expr_typval(expr, false, &mut argv, &mut newtv) };
    if ret == FAIL {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(&*get_vim_var_tv(VimVarIndex::Val)) };
        return (FAIL, newtv, remove);
    }

    let mut result = OK;
    if filtermap == FilterMapT::Filter {
        let mut error = false;
        // filter(): when expr is zero remove the item.
        remove = tv_get_number_chk(&newtv, Some(&mut error)) == 0;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(&newtv) };
        // Deliberately NOT reset to TypvalT::default() here: the
        // original's own `tv_clear(newtv)` (via `encode_vim_to_nothing`)
        // only releases whatever OWNED resource the value holds
        // (List/Dict/Blob/Partial/Func refcounts) - it never touches
        // `newtv->v_type` itself. For the overwhelmingly common case
        // (an ordinary Number/Bool result), tv_clear is a complete
        // no-op, and newtv keeps its real Number/Bool value/type
        // afterward - filter_map_blob's own caller-side check
        // (`newtv.v_type != VAR_NUMBER && newtv.v_type != VAR_BOOL`)
        // depends on exactly this NOT being reset to Unknown here.
        // Caught via 2 real, reproducible test failures before this
        // fix (blob byte removal silently doing nothing) - resetting
        // to Default was an extra, incorrect step this crate's own
        // tv_clear_simple's &-not-&mut signature already hints isn't
        // what the original does.
        // On type error, nothing has been removed; return FAIL to stop
        // the loop. The error message was given by tv_get_number_chk().
        if error {
            result = FAIL;
        }
    } else if filtermap == FilterMapT::Foreach {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(&newtv) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_clear_simple(&*get_vim_var_tv(VimVarIndex::Val)) };
    (result, newtv, remove)
}

/// Implementation of `map()`/`mapnew()`/`filter()`/`foreach()` for a
/// `List`. Apply `expr` to every item in list `l` and return the
/// result in `rettv` (`filter_map_list`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT`; `expr`
/// must be valid (forwards `filter_map_one`'s own safety
/// requirements).
pub unsafe fn filter_map_list(
    l: *mut crate::eval::typval_defs::ListT,
    filtermap: FilterMapT,
    expr: &TypvalT,
    rettv: &mut TypvalT,
) {
    use crate::eval::vars::{set_vim_var_nr, set_vim_var_type, VimVarIndex};

    if filtermap == FilterMapT::MapNew {
        rettv.value = TypvalValue::List(std::ptr::null_mut());
    }
    if l.is_null()
        || (filtermap == FilterMapT::Filter && value_check_lock(unsafe { tv_list_locked(l) }, None))
    {
        return;
    }

    let mut l_ret: *mut crate::eval::typval_defs::ListT = std::ptr::null_mut();
    if filtermap == FilterMapT::MapNew {
        l_ret = unsafe { tv_list_alloc_ret(rettv, ListLenSpecials::Unknown as isize) };
    }

    // set_vim_var_nr() doesn't set the type.
    unsafe { set_vim_var_type(VimVarIndex::Key, VarType::Number) };

    let prev_lock = unsafe { tv_list_locked(l) };
    if prev_lock == VarLockStatus::Unlocked {
        unsafe { tv_list_set_lock(l, VarLockStatus::Locked) };
    }

    let mut idx: VarnumberT = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let mut li = unsafe { tv_list_first(l) };
    while !li.is_null() {
        if filtermap == FilterMapT::Map
            && value_check_lock(unsafe { (*li).li_tv.v_lock }, None)
        {
            break;
        }
        unsafe { set_vim_var_nr(VimVarIndex::Key, idx) };
        // SAFETY: forwarded from this function's own safety doc.
        let (ret, mut newtv, rem) = unsafe { filter_map_one(&(*li).li_tv, expr, filtermap) };
        if ret == FAIL {
            unsafe { tv_clear_simple(&newtv) };
            break;
        }

        if filtermap == FilterMapT::Map {
            // map(): replace the list item value.
            unsafe { tv_clear_simple(&(*li).li_tv) };
            newtv.v_lock = VarLockStatus::Unlocked;
            unsafe { (*li).li_tv = newtv };
        } else if filtermap == FilterMapT::MapNew {
            // mapnew(): append the list item value.
            unsafe { tv_list_append_owned_tv(l_ret, newtv) };
        }

        if filtermap == FilterMapT::Filter && rem {
            li = unsafe { tv_list_item_remove(l, li) };
        } else {
            li = unsafe { (*li).li_next };
        }
        idx += 1;
    }

    unsafe { tv_list_set_lock(l, prev_lock) };
}

/// Implementation of `map()`/`mapnew()`/`filter()`/`foreach()` for a
/// `Dict`. Apply `expr` to every item in dict `d` and return the
/// result in `rettv` (`filter_map_dict`).
///
/// Iterates a SNAPSHOT of `d`'s own items (via `dv_index.values()`,
/// collected into a plain `Vec` first) rather than the original's own
/// `TV_DICT_ITER`/`hash_lock`/`hash_unlock`-guarded live-hashtable
/// walk: `dv_index` is a `HashMap`, which Rust's own borrow checker
/// will not let this function mutate (via `tv_dict_item_remove`)
/// WHILE also iterating it directly - snapshotting first sidesteps
/// this cleanly, and is a faithful translation regardless, since the
/// original's own hashtable iteration order was never a Vimscript-
/// visible guarantee to begin with (matching this crate's own
/// `max_min`/`tv_dict_equal` precedent for the identical situation).
/// `hash_lock`/`hash_unlock` are correspondingly not needed either -
/// they exist in the original purely to keep a LIVE-hashtable walk
/// safe while removing entries mid-iteration, a concern the snapshot
/// approach doesn't have.
///
/// # Safety
/// `d`, if non-null, must be a valid pointer to a live `DictT`; `expr`
/// must be valid (forwards `filter_map_one`'s own safety
/// requirements).
pub unsafe fn filter_map_dict(d: *mut DictT, filtermap: FilterMapT, expr: &TypvalT, rettv: &mut TypvalT) {
    use crate::eval::vars::{get_vim_var_tv, set_vim_var_string, var_check_fixed, var_check_ro, VimVarIndex};

    if filtermap == FilterMapT::MapNew {
        rettv.value = TypvalValue::Dict(std::ptr::null_mut());
    }
    if d.is_null()
        || (filtermap == FilterMapT::Filter && value_check_lock(unsafe { (*d).dv_lock }, None))
    {
        return;
    }

    let mut d_ret: *mut DictT = std::ptr::null_mut();
    if filtermap == FilterMapT::MapNew {
        d_ret = unsafe { tv_dict_alloc_ret(rettv) };
    }

    let prev_lock = unsafe { (*d).dv_lock };
    if prev_lock == VarLockStatus::Unlocked {
        unsafe { (*d).dv_lock = VarLockStatus::Locked };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let items: Vec<*mut DictitemT> = unsafe { (*d).dv_index.values().copied().collect() };
    for di in items {
        if filtermap == FilterMapT::Map
            && (value_check_lock(unsafe { (*di).di_tv.v_lock }, None)
                || var_check_ro(unsafe { (*di).di_flags }))
        {
            break;
        }

        // di_key always carries a trailing NUL (this crate's own
        // established DictitemT.di_key convention) - strip it before
        // handing the "clean" key bytes to set_vim_var_string.
        let key: &[u8] = unsafe { &(*di).di_key };
        let key = &key[..key.len().saturating_sub(1)];
        unsafe { set_vim_var_string(VimVarIndex::Key, Some(key)) };

        // SAFETY: forwarded from this function's own safety doc.
        let (ret, mut newtv, rem) = unsafe { filter_map_one(&(*di).di_tv, expr, filtermap) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(&*get_vim_var_tv(VimVarIndex::Key)) };
        if ret == FAIL {
            unsafe { tv_clear_simple(&newtv) };
            break;
        }

        if filtermap == FilterMapT::Map {
            // map(): replace the dict item value.
            unsafe { tv_clear_simple(&(*di).di_tv) };
            newtv.v_lock = VarLockStatus::Unlocked;
            unsafe { (*di).di_tv = newtv };
        } else if filtermap == FilterMapT::MapNew {
            // mapnew(): add the item value to the new dict.
            let key: &[u8] = unsafe { &(*di).di_key };
            let key = &key[..key.len().saturating_sub(1)];
            let r = unsafe { tv_dict_add_tv(&mut *d_ret, key, &newtv) };
            unsafe { tv_clear_simple(&newtv) };
            if r == FAIL {
                break;
            }
        } else if filtermap == FilterMapT::Filter && rem {
            // filter(false): remove the item from the dict.
            if var_check_fixed(unsafe { (*di).di_flags }) || var_check_ro(unsafe { (*di).di_flags }) {
                break;
            }
            unsafe { tv_dict_item_remove(&mut *d, di) };
        }
    }

    unsafe { (*d).dv_lock = prev_lock };
}

/// Implementation of `map()`/`mapnew()`/`filter()`/`foreach()` for a
/// `Blob`. Apply `expr` to every byte in `blob_arg` and return the
/// result in `rettv` (`filter_map_blob`).
///
/// The original's own in-place byte removal
/// (`memmove(p + i, p + i + 1, ...)`) becomes a plain
/// `Vec::remove` - `GarrayT.ga_data` is already a real `Vec<u8>` in
/// this crate (see `garray_defs.rs`'s own doc comment), so there is no
/// manual pointer arithmetic to replicate.
///
/// # Safety
/// `blob_arg`, if non-null, must be a valid pointer to a live
/// `crate::eval::typval_defs::BlobT`; `expr` must be valid (forwards
/// `filter_map_one`'s own safety requirements).
pub unsafe fn filter_map_blob(
    blob_arg: *mut crate::eval::typval_defs::BlobT,
    filtermap: FilterMapT,
    expr: &TypvalT,
    rettv: &mut TypvalT,
) {
    use crate::eval::vars::{set_vim_var_nr, set_vim_var_type, VimVarIndex};

    if filtermap == FilterMapT::MapNew {
        rettv.value = TypvalValue::Blob(std::ptr::null_mut());
    }
    let b = blob_arg;
    if b.is_null()
        || (filtermap == FilterMapT::Filter && value_check_lock(unsafe { (*b).bv_lock }, None))
    {
        return;
    }

    let mut b_ret = b;
    if filtermap == FilterMapT::MapNew {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_blob_copy(b, rettv) };
        let TypvalValue::Blob(new_b) = rettv.value else {
            unreachable!("tv_blob_copy always sets rettv.value to Blob(_)")
        };
        b_ret = new_b;
    }

    // set_vim_var_nr() doesn't set the type.
    unsafe { set_vim_var_type(VimVarIndex::Key, VarType::Number) };

    let prev_lock = unsafe { (*b).bv_lock };
    if prev_lock == VarLockStatus::Unlocked {
        unsafe { (*b).bv_lock = VarLockStatus::Locked };
    }

    let mut i: i32 = 0;
    let mut idx: VarnumberT = 0;
    while i < unsafe { (*b).bv_ga.ga_len } {
        // SAFETY: forwarded from this function's own safety doc.
        let val = unsafe { tv_blob_get(b, i) };
        let tv = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Number(VarnumberT::from(val)),
        };
        unsafe { set_vim_var_nr(VimVarIndex::Key, idx) };
        // SAFETY: forwarded from this function's own safety doc.
        let (ret, newtv, rem) = unsafe { filter_map_one(&tv, expr, filtermap) };
        if ret == FAIL {
            unsafe { tv_clear_simple(&newtv) };
            break;
        }

        if filtermap != FilterMapT::Foreach {
            if !matches!(newtv.value, TypvalValue::Number(_) | TypvalValue::Bool(_)) {
                unsafe { tv_clear_simple(&newtv) };
                // emsg(_(e_invalblob)) omitted - message display, not
                // tractable; the identical break is kept.
                break;
            }
            if filtermap != FilterMapT::Filter {
                let new_val = tv_get_number(&newtv);
                if new_val != VarnumberT::from(val) {
                    unsafe { tv_blob_set(b_ret, i, new_val as u8) };
                }
            } else if rem {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    (*blob_arg).bv_ga.ga_data.remove(i as usize);
                    (*blob_arg).bv_ga.ga_len -= 1;
                }
                i -= 1;
            }
        }
        idx += 1;
        i += 1;
    }

    unsafe { (*b).bv_lock = prev_lock };
}

/// Implementation of `map()`/`mapnew()`/`filter()`/`foreach()` for a
/// `String`. Apply `expr` to every character in `str` and return the
/// result in `rettv` (`filter_map_string`).
///
/// Unlike the List/Dict/Blob variants (which mutate their argument in
/// place for `map()`/`filter()`), a String always produces a genuinely
/// NEW string - Vimscript strings are plain values, not refcounted,
/// mutable containers - matching the original's own `rettv->vval.
/// v_string = ga.ga_data` (a freshly built buffer, never the input
/// `str` itself).
///
/// `str` is iterated at the byte level (no embedded-NUL scanning, no
/// trailing NUL expected) - matching this crate's own established
/// "Vimscript `String` typval values are a plain `Vec<u8>` with no
/// implicit NUL terminator" convention (see `tv_get_string_chk`'s own
/// doc comment) - `str.len()` is the sole, authoritative stop
/// condition, exactly like the original's own NUL-terminated C string
/// stops at its own implicit terminator.
///
/// # Safety
/// `expr` must be valid (forwards `filter_map_one`'s own safety
/// requirements).
pub unsafe fn filter_map_string(str: &[u8], filtermap: FilterMapT, expr: &TypvalT, rettv: &mut TypvalT) {
    use crate::eval::vars::{set_vim_var_nr, set_vim_var_type, VimVarIndex};
    use crate::mbyte::utfc_ptr2len;

    rettv.value = TypvalValue::String(None);

    // set_vim_var_nr() doesn't set the type.
    unsafe { set_vim_var_type(VimVarIndex::Key, VarType::Number) };

    let mut ga: Vec<u8> = Vec::new();
    let mut pos: usize = 0;
    let mut idx: VarnumberT = 0;
    while pos < str.len() {
        // SAFETY: forwarded from this function's own safety doc.
        let len = unsafe { utfc_ptr2len(&str[pos..]) }.max(1) as usize;
        let tv = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::String(Some(str[pos..pos + len].to_vec())),
        };

        unsafe { set_vim_var_nr(VimVarIndex::Key, idx) };
        // SAFETY: forwarded from this function's own safety doc.
        let (ret, newtv, rem) = unsafe { filter_map_one(&tv, expr, filtermap) };
        if ret == FAIL {
            unsafe {
                tv_clear_simple(&newtv);
                tv_clear_simple(&tv);
            }
            break;
        }

        if matches!(filtermap, FilterMapT::Map | FilterMapT::MapNew) {
            if let TypvalValue::String(Some(s)) = &newtv.value {
                ga.extend_from_slice(s);
            } else {
                unsafe {
                    tv_clear_simple(&newtv);
                    tv_clear_simple(&tv);
                }
                // emsg(_(e_string_required)) omitted - message
                // display, not tractable; the identical break is kept.
                break;
            }
        } else if (filtermap == FilterMapT::Foreach || !rem)
            && let TypvalValue::String(Some(s)) = &tv.value
        {
            ga.extend_from_slice(s);
        }

        unsafe {
            tv_clear_simple(&newtv);
            tv_clear_simple(&tv);
        }

        idx += 1;
        pos += len;
    }

    rettv.value = TypvalValue::String(Some(ga));
}

/// Implementation of `map()`, `mapnew()`, `filter()` and `foreach()`
/// (`filter_map`).
///
/// [`TypvalValue::List`]/[`TypvalValue::Dict`]/[`TypvalValue::Blob`]/
/// [`TypvalValue::String`] are ALL modeled - every real container type
/// `filter()`/`map()`/`mapnew()` accept in the original.
///
/// # Safety
/// Forwards `filter_map_list`/`filter_map_dict`/`filter_map_blob`/
/// `filter_map_string`'s own safety requirements for `argvars[0]`.
pub unsafe fn filter_map(argvars: &[TypvalT], rettv: &mut TypvalT, filtermap: FilterMapT) {
    use crate::eval::vars::{prepare_vimvar, VimVarIndex};

    // map(), filter(), foreach() return the first argument, also on
    // failure.
    if filtermap != FilterMapT::MapNew && !matches!(argvars[0].value, TypvalValue::String(_)) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_copy(&argvars[0], rettv) };
    }

    match &argvars[0].value {
        TypvalValue::Blob(_) | TypvalValue::List(_) | TypvalValue::Dict(_) | TypvalValue::String(_) => {}
        _ => {
            // semsg(_(e_argument_of_str_must_be_list_string_dictionary_or_blob),
            // func_name) omitted - message display, not tractable; the
            // identical early-return (no state change beyond the
            // rettv=argvars[0] copy above) is kept.
            return;
        }
    }

    let expr = &argvars[1];
    // On type errors, the preceding call has already displayed an
    // error message. Avoid a misleading error message for an empty
    // string that was not passed as argument.
    if matches!(expr.value, TypvalValue::Unknown) {
        return;
    }

    let mut save_val = TypvalT::default();
    let mut save_key = TypvalT::default();

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        prepare_vimvar(VimVarIndex::Val, &mut save_val);
        prepare_vimvar(VimVarIndex::Key, &mut save_key);
    }

    // The original also resets did_emsg here to detect whether an
    // error occurred during evaluation of the expression - omitted
    // (message-display bookkeeping, not tractable, and filter_map_one
    // already reports evaluation failure via its own FAIL return).

    match &argvars[0].value {
        TypvalValue::List(l) => {
            let l = *l;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { filter_map_list(l, filtermap, expr, rettv) };
        }
        TypvalValue::Dict(d) => {
            let d = *d;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { filter_map_dict(d, filtermap, expr, rettv) };
        }
        TypvalValue::Blob(b) => {
            let b = *b;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { filter_map_blob(b, filtermap, expr, rettv) };
        }
        TypvalValue::String(s) => {
            let s: &[u8] = s.as_deref().unwrap_or(&[]);
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { filter_map_string(s, filtermap, expr, rettv) };
        }
        _ => unreachable!("filter_map: argvars[0] type was already validated above"),
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::eval::vars::restore_vimvar(VimVarIndex::Key, save_key);
        crate::eval::vars::restore_vimvar(VimVarIndex::Val, save_val);
    }
}

/// Advance watchers to the next item. Used just before removing an
/// item from a list (`tv_list_watch_fix`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`, and every
/// `listwatch_T` reachable via its `lv_watch` chain must be valid.
/// `item` must be a valid, non-null pointer to a live `ListitemT`.
unsafe fn tv_list_watch_fix(
    l: *mut crate::eval::typval_defs::ListT,
    item: *const crate::eval::typval_defs::ListitemT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut lw = unsafe { (*l).lv_watch };
    while !lw.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { (*lw).lw_item } == item.cast_mut() {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*lw).lw_item = (*item).li_next };
        }
        // SAFETY: forwarded from this function's own safety doc.
        lw = unsafe { (*lw).lw_next };
    }
}

/// Add a watcher to a list (`tv_list_watch_add`).
///
/// # Safety
/// `l` and `lw` must be valid, non-null pointers to a live `ListT`/
/// `ListwatchT` respectively; `lw` must outlive its presence in `l`'s
/// watcher chain (the caller's job, matching the original's own raw-
/// pointer contract).
pub unsafe fn tv_list_watch_add(
    l: *mut crate::eval::typval_defs::ListT,
    lw: *mut crate::eval::typval_defs::ListwatchT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*lw).lw_next = (*l).lv_watch };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*l).lv_watch = lw };
}

/// Remove a watcher from a list. Does not warn if the watcher was not
/// found (`tv_list_watch_remove`).
///
/// # Safety
/// Same as [`tv_list_watch_add`].
pub unsafe fn tv_list_watch_remove(
    l: *mut crate::eval::typval_defs::ListT,
    lwrem: *mut crate::eval::typval_defs::ListwatchT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut lwp: *mut *mut crate::eval::typval_defs::ListwatchT = unsafe { &mut (*l).lv_watch };
    // SAFETY: forwarded from this function's own safety doc.
    let mut lw = unsafe { (*l).lv_watch };
    while !lw.is_null() {
        if lw == lwrem {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { *lwp = (*lw).lw_next };
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        lwp = unsafe { &mut (*lw).lw_next };
        // SAFETY: forwarded from this function's own safety doc.
        lw = unsafe { (*lw).lw_next };
    }
}

/// Remove items `item` to `item2` from list `l`. Does not free the
/// listitem or the value (`tv_list_drop_items`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`; `item`/
/// `item2` must be valid, non-null pointers to items actually present
/// (in order) in `l`'s own `li_next` chain.
pub unsafe fn tv_list_drop_items(
    l: *mut crate::eval::typval_defs::ListT,
    item: *mut crate::eval::typval_defs::ListitemT,
    item2: *mut crate::eval::typval_defs::ListitemT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let item2_next = unsafe { (*item2).li_next };
    let mut ip = item;
    while ip != item2_next {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*l).lv_len -= 1 };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_list_watch_fix(l, ip) };
        // SAFETY: forwarded from this function's own safety doc.
        ip = unsafe { (*ip).li_next };
    }

    if item2_next.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*l).lv_last = (*item).li_prev };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*item2_next).li_prev = (*item).li_prev };
    }
    // SAFETY: forwarded from this function's own safety doc.
    let item_prev = unsafe { (*item).li_prev };
    if item_prev.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*l).lv_first = item2_next };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*item_prev).li_next = item2_next };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*l).lv_idx_item = std::ptr::null_mut() };
}

/// Like [`tv_list_drop_items`], but also frees all removed items
/// (`tv_list_remove_items`).
///
/// # Safety
/// Same as [`tv_list_drop_items`], plus every item from `item` to
/// `item2` (inclusive) must have been allocated via
/// `tv_list_item_alloc`/`Box::into_raw`, matching
/// `tv_clear_simple`'s own safety contract for each one's value.
pub unsafe fn tv_list_remove_items(
    l: *mut crate::eval::typval_defs::ListT,
    item: *mut crate::eval::typval_defs::ListitemT,
    item2: *mut crate::eval::typval_defs::ListitemT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_drop_items(l, item, item2) };
    let mut li = item;
    loop {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(&(*li).li_tv) };
        // SAFETY: forwarded from this function's own safety doc.
        let nli = unsafe { (*li).li_next };
        let done = li == item2;
        // SAFETY: forwarded from this function's own safety doc.
        drop(unsafe { Box::from_raw(li) });
        if done {
            break;
        }
        li = nli;
    }
}

/// Move items `item` to `item2` from list `l` to the end of list
/// `tgt_l` (`tv_list_move_items`).
///
/// # Safety
/// Same as [`tv_list_drop_items`], plus `tgt_l` must be a valid,
/// non-null pointer to a live `ListT`.
pub unsafe fn tv_list_move_items(
    l: *mut crate::eval::typval_defs::ListT,
    item: *mut crate::eval::typval_defs::ListitemT,
    item2: *mut crate::eval::typval_defs::ListitemT,
    tgt_l: *mut crate::eval::typval_defs::ListT,
    cnt: i32,
) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_drop_items(l, item, item2) };
    // SAFETY: forwarded from this function's own safety doc.
    let tgt_last = unsafe { (*tgt_l).lv_last };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*item).li_prev = tgt_last;
        (*item2).li_next = std::ptr::null_mut();
    }
    if tgt_last.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*tgt_l).lv_first = item };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*tgt_last).li_next = item };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*tgt_l).lv_last = item2;
        (*tgt_l).lv_len += cnt;
    }
}

/// `remove({list}, {idx} [, {end}])` - the `List` case of `remove()`
/// (`tv_list_remove`).
///
/// Removes and returns a single item at `{idx}` (moving its own
/// value into `rettv` directly, matching the original's own plain
/// `*rettv = *TV_LIST_ITEM_TV(item)` struct assignment - NOT a
/// [`tv_copy`], so no refcount change happens for a `List`/`Dict`/
/// `Blob`-valued item), or, when `{end}` is also given, removes and
/// returns every item from `{idx}` to `{end}` (inclusive) as a NEW
/// list built via [`tv_list_alloc_ret`]/[`tv_list_move_items`].
///
/// Every error path (locked list, a type error reading `{idx}`/
/// `{end}`, either index out of range, or `{end}` before `{idx}`) is a
/// bare early return leaving `rettv` at its caller-provided default -
/// matching the original's own `semsg`/`emsg`-then-`return` structure
/// exactly (message display itself omitted, see this module's own
/// doc comment).
///
/// # Safety
/// `argvars[0].value` must be `List`-typed with a valid, non-null
/// pointer whose items are all genuinely allocated via
/// `tv_list_item_alloc`/`Box::into_raw`.
pub unsafe fn tv_list_remove(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let TypvalValue::List(l) = argvars[0].value else { unreachable!() };
    // SAFETY: forwarded from this function's own safety doc.
    if value_check_lock(unsafe { tv_list_locked(l) }, None) {
        return;
    }

    let mut error = false;
    let idx = tv_get_number_chk(&argvars[1], Some(&mut error));
    if error {
        return; // type error; errmsg already given in the original.
    }

    // SAFETY: forwarded from this function's own safety doc.
    let item = unsafe { tv_list_find(l, idx as i32) };
    if item.is_null() {
        return; // semsg(_(e_list_index_out_of_range_nr), ...) omitted.
    }

    if argvars.len() <= 2 {
        // Remove one item, return its value.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            tv_list_drop_items(l, item, item);
            *rettv = std::mem::take(&mut (*item).li_tv);
            drop(Box::from_raw(item));
        }
        return;
    }

    let mut error2 = false;
    let end = tv_get_number_chk(&argvars[2], Some(&mut error2));
    if error2 {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let item2 = unsafe { tv_list_find(l, end as i32) };
    if item2.is_null() {
        return; // semsg(_(e_list_index_out_of_range_nr), ...) omitted.
    }

    let mut cnt: i32 = 0;
    let mut li = item;
    let mut found = false;
    loop {
        cnt += 1;
        if li == item2 {
            found = true;
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        li = unsafe { (*li).li_next };
        if li.is_null() {
            break;
        }
    }
    if !found {
        return; // emsg(_(e_invrange)) omitted - item2 wasn't found after item.
    }

    // SAFETY: forwarded from this function's own safety doc.
    let tgt = unsafe { tv_list_alloc_ret(rettv, cnt as isize) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_move_items(l, item, item2, tgt, cnt) };
}

/// Remove a list item from a list and free it (also clears the
/// value). Returns a pointer to the list item just after the removed
/// one, null if the removed item was the last one
/// (`tv_list_item_remove`).
///
/// # Safety
/// Same as [`tv_list_remove_items`], restricted to a single item.
pub unsafe fn tv_list_item_remove(
    l: *mut crate::eval::typval_defs::ListT,
    item: *mut crate::eval::typval_defs::ListitemT,
) -> *mut crate::eval::typval_defs::ListitemT {
    // SAFETY: forwarded from this function's own safety doc.
    let next_item = unsafe { (*item).li_next };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_drop_items(l, item, item) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_clear_simple(&(*item).li_tv) };
    // SAFETY: forwarded from this function's own safety doc.
    drop(unsafe { Box::from_raw(item) });
    next_item
}

/// Free items contained in a list (`tv_list_free_contents`).
///
/// Nested List/Dict/Partial values are released through an explicit
/// worklist instead of recursive native calls.
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT` whose items
/// were all allocated via `tv_list_item_alloc`/`Box::into_raw`,
/// matching `tv_clear_simple`'s own safety contract for each item's
/// value.
pub unsafe fn tv_list_free_contents(l: *mut crate::eval::typval_defs::ListT) {
    unsafe { free_targets(FreeTarget::List(l, false)) };
}

/// Free a list itself, ignoring items it contains. Ignores the
/// reference count (`tv_list_free_list`).
///
/// # Safety
/// `l` must be a valid pointer previously returned by [`tv_list_alloc`],
/// not yet freed.
pub unsafe fn tv_list_free_list(l: *mut crate::eval::typval_defs::ListT) {
    // SAFETY: forwarded from this function's own safety doc.
    let (used_prev, used_next) = unsafe { ((*l).lv_used_prev, (*l).lv_used_next) };
    if used_prev.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *GC_FIRST_LIST.get_mut() = used_next };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*used_prev).lv_used_next = used_next };
    }
    if !used_next.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*used_next).lv_used_prev = used_prev };
    }

    // NLUA_CLEAR_REF(l->lua_table_ref): omitted - the Lua host (phase
    // 13) isn't started, and lua_table_ref is always LUA_NOREF here.

    // SAFETY: forwarded from this function's own safety doc.
    drop(unsafe { Box::from_raw(l) });
}

/// Free a list, including all items it points to. Ignores the
/// reference count (`tv_list_free`).
///
/// Nested container destruction is iterative and stack-safe.
///
/// # Safety
/// Same as [`tv_list_free_contents`]/[`tv_list_free_list`] combined.
pub unsafe fn tv_list_free(l: *mut crate::eval::typval_defs::ListT) {
    // The original's `tv_in_free_unref_items` re-entrancy guard is
    // always false here - same reasoning as `tv_dict_free`.
    unsafe { free_targets(FreeTarget::List(l, true)) };
}

/// Unreference a list: decrements the reference count and frees when
/// it becomes zero or less (`tv_list_unref`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer previously returned by
/// [`tv_list_alloc`], not yet freed.
pub unsafe fn tv_list_unref(l: *mut crate::eval::typval_defs::ListT) {
    if l.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*l).lv_refcount -= 1 };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*l).lv_refcount } <= 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_list_free(l) };
    }
}

/// Append item to the end of a list (`tv_list_append`).
///
/// # Safety
/// `l`/`item` must be valid, non-null pointers to a live `ListT`/
/// `ListitemT`; `item` must not already be linked into any list.
pub unsafe fn tv_list_append(
    l: *mut crate::eval::typval_defs::ListT,
    item: *mut crate::eval::typval_defs::ListitemT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let last = unsafe { (*l).lv_last };
    if last.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*l).lv_first = item };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*l).lv_last = item };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*item).li_prev = std::ptr::null_mut() };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*last).li_next = item };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*item).li_prev = last };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*l).lv_last = item };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*l).lv_len += 1 };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*item).li_next = std::ptr::null_mut() };
}

/// Append a Vimscript value to the end of a list; `tv` is copied (see
/// [`tv_copy`]) into a freshly-allocated item (`tv_list_append_tv`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`. Forwards
/// [`tv_copy`]'s own safety requirements for `tv`.
pub unsafe fn tv_list_append_tv(l: *mut crate::eval::typval_defs::ListT, tv: &TypvalT) {
    let li = tv_list_item_alloc();
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_copy(tv, &mut (*li).li_tv) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_append(l, li) };
}

/// Append an owned string to a list (`tv_list_append_allocated_string`).
///
/// The string is MOVED into the list, not copied - the list takes
/// ownership, matching the original's own contract (its caller hands
/// over an already-allocated buffer and must not free it afterwards).
///
/// `None` appends a null string, which the original represents as a
/// `NULL v_string`.
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`.
pub unsafe fn tv_list_append_allocated_string(
    l: *mut crate::eval::typval_defs::ListT,
    str_: Option<Vec<u8>>,
) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        tv_list_append_owned_tv(
            l,
            TypvalT {
                value: TypvalValue::String(str_),
                v_lock: VarLockStatus::Unlocked,
            },
        );
    }
}

/// Like [`tv_list_append_tv`], but `tv` is moved into the list rather
/// than copied - it is no longer valid to use `tv` after this
/// function returns. Returns a pointer to the newly-owned value
/// (`tv_list_append_owned_tv`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`.
pub unsafe fn tv_list_append_owned_tv(
    l: *mut crate::eval::typval_defs::ListT,
    tv: TypvalT,
) -> *mut TypvalT {
    let li = tv_list_item_alloc();
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*li).li_tv = tv };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_append(l, li) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut (*li).li_tv as *mut TypvalT }
}

/// Append a list to a list; `itemlist`'s reference count is
/// incremented (`tv_list_append_list`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`. `itemlist`,
/// if non-null, must be a valid pointer to a live `ListT`.
pub unsafe fn tv_list_append_list(
    l: *mut crate::eval::typval_defs::ListT,
    itemlist: *mut crate::eval::typval_defs::ListT,
) {
    let tv = TypvalT {
        v_lock: VarLockStatus::Unlocked,
        value: TypvalValue::List(itemlist),
    };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_append_owned_tv(l, tv) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_ref(itemlist) };
}

/// Append a dictionary to a list; `dict`'s reference count is
/// incremented (`tv_list_append_dict`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`. `dict`, if
/// non-null, must be a valid pointer to a live `DictT`.
pub unsafe fn tv_list_append_dict(l: *mut crate::eval::typval_defs::ListT, dict: *mut DictT) {
    let tv = TypvalT {
        v_lock: VarLockStatus::Unlocked,
        value: TypvalValue::Dict(dict),
    };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_append_owned_tv(l, tv) };
    if !dict.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*dict).dv_refcount += 1 };
    }
}

/// Make a copy of `str` and append it as an item to a list
/// (`tv_list_append_string`/`tv_list_append_allocated_string` collapsed
/// into one function - Rust's `&[u8]` already carries its own length,
/// and every caller in this crate already owns a byte buffer it can
/// simply clone rather than needing the original's "adopt an
/// already-allocated buffer" optimization). `None` appends an absent
/// string, matching the original's `str == NULL` case.
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`.
pub unsafe fn tv_list_append_string(l: *mut crate::eval::typval_defs::ListT, s: Option<&[u8]>) {
    let tv = TypvalT {
        v_lock: VarLockStatus::Unlocked,
        value: TypvalValue::String(s.map(<[u8]>::to_vec)),
    };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_append_owned_tv(l, tv) };
}

/// Append a number to a list (`tv_list_append_number`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`.
pub unsafe fn tv_list_append_number(
    l: *mut crate::eval::typval_defs::ListT,
    n: crate::eval::typval_defs::VarnumberT,
) {
    let tv = TypvalT {
        v_lock: VarLockStatus::Unlocked,
        value: TypvalValue::Number(n),
    };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_append_owned_tv(l, tv) };
}

/// Insert a list item before `item` (or at the end, if `item` is
/// null) (`tv_list_insert`).
///
/// # Safety
/// `l`/`ni` must be valid, non-null pointers to a live `ListT`/
/// `ListitemT` (`ni` not already linked into any list); `item`, if
/// non-null, must be a valid pointer to an item actually present in
/// `l`.
pub unsafe fn tv_list_insert(
    l: *mut crate::eval::typval_defs::ListT,
    ni: *mut crate::eval::typval_defs::ListitemT,
    item: *mut crate::eval::typval_defs::ListitemT,
) {
    if item.is_null() {
        // Append new item at end of list.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_list_append(l, ni) };
    } else {
        // Insert new item before existing item.
        // SAFETY: forwarded from this function's own safety doc.
        let item_prev = unsafe { (*item).li_prev };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*ni).li_prev = item_prev };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*ni).li_next = item };
        if item_prev.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*l).lv_first = ni };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*l).lv_idx += 1 };
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*item_prev).li_next = ni };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*l).lv_idx_item = std::ptr::null_mut() };
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*item).li_prev = ni };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*l).lv_len += 1 };
    }
}

/// Insert a Vimscript value into a list, before `item` (or at the end,
/// if `item` is null); `tv` is copied (see [`tv_copy`]) into a
/// freshly-allocated item (`tv_list_insert_tv`).
///
/// # Safety
/// Same as [`tv_list_insert`]. Forwards [`tv_copy`]'s own safety
/// requirements for `tv`.
pub unsafe fn tv_list_insert_tv(
    l: *mut crate::eval::typval_defs::ListT,
    tv: &TypvalT,
    item: *mut crate::eval::typval_defs::ListitemT,
) {
    let ni = tv_list_item_alloc();
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_copy(tv, &mut (*ni).li_tv) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_insert(l, ni, item) };
}

/// Get a list's own copy ID, set by an earlier deep [`tv_list_copy`]
/// call (`tv_list_copyid`, `eval/typval.h`'s own `static inline`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`.
#[must_use]
pub unsafe fn tv_list_copyid(l: *const crate::eval::typval_defs::ListT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*l).lv_copy_id }
}

/// Get a list's own latest copy, set by an earlier deep
/// [`tv_list_copy`] call (`tv_list_latest_copy`, `eval/typval.h`'s
/// own `static inline`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`.
#[must_use]
pub unsafe fn tv_list_latest_copy(
    l: *const crate::eval::typval_defs::ListT,
) -> *mut crate::eval::typval_defs::ListT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*l).lv_copylist }
}

/// Make a copy of a list (`tv_list_copy`).
///
/// Returns a null pointer if `orig` is null, or on failure. The
/// refcount of the new list is set to 1.
///
/// `conv` is accepted for signature fidelity with the original but is
/// only ever read by the `deep`-copy path below (never dereferenced
/// here, matching the original's own `deep == false` behavior
/// exactly).
///
/// `deep=true` recursively copies every nested `List`/`Dict` item too,
/// via [`crate::eval::eval::var_item_copy`] - if that fails partway
/// through (recursion limit or a nested allocation failure), the
/// WHOLE copy is discarded and null is returned, matching the
/// original's own `goto tv_list_copy_error` behavior exactly (contrast
/// [`tv_dict_copy`]'s own, genuinely different, "keep the partial
/// copy" behavior on the same kind of failure - a real asymmetry
/// already present in the original, not a translation choice).
///
/// # Safety
/// `orig`, if non-null, must be a valid pointer to a live `ListT`,
/// and every item reachable via its `lv_first`/`li_next` chain must
/// have a valid `li_tv` (forwarded to [`tv_copy`]'s own contract, used
/// for the shallow-copy path, and to
/// [`crate::eval::eval::var_item_copy`]'s own contract, used for the
/// deep-copy path).
pub unsafe fn tv_list_copy(
    conv: *const crate::types_defs::VimconvT,
    orig: *mut crate::eval::typval_defs::ListT,
    deep: bool,
    copy_id: i32,
) -> *mut crate::eval::typval_defs::ListT {
    if orig.is_null() {
        return std::ptr::null_mut();
    }

    // SAFETY: forwarded from this function's own safety doc.
    let copy = tv_list_alloc(unsafe { tv_list_len(orig) } as isize);
    // SAFETY: `copy` was just allocated above, a fresh pointer not
    // shared with anything yet.
    unsafe { tv_list_ref(copy) };
    if copy_id != 0 {
        // Do this before adding the items, because one of the items
        // may refer back to this list.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            (*orig).lv_copy_id = copy_id;
            (*orig).lv_copylist = copy;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut item = unsafe { tv_list_first(orig) };
    while !item.is_null() {
        // SAFETY: GLOBALS is only ever accessed through this crate's
        // established single-threaded-main-loop convention.
        if unsafe { crate::globals::GLOBALS.get_mut() }.got_int {
            break;
        }
        let ni = tv_list_item_alloc();
        if deep {
            // SAFETY: forwarded from this function's own safety doc.
            let ret =
                unsafe { crate::eval::eval::var_item_copy(conv, &(*item).li_tv, &mut (*ni).li_tv, deep, copy_id) };
            if ret == FAIL {
                // xfree(ni): ni's own li_tv is either untouched
                // (Unknown, recursion-limit case) or a null List/Dict
                // pointer (nested-copy-failure case) at this point -
                // either way there is nothing owned to release,
                // matching the original's own plain xfree(ni) (not a
                // full tv_clear-based free) exactly.
                drop(unsafe { Box::from_raw(ni) });
                // SAFETY: `copy` was allocated by this same function
                // above and is not shared with anything else yet.
                unsafe { tv_list_unref(copy) };
                return std::ptr::null_mut();
            }
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_copy(&(*item).li_tv, &mut (*ni).li_tv) };
        }
        // SAFETY: `copy`/`ni` are both valid, freshly-prepared pointers.
        unsafe { tv_list_append(copy, ni) };
        // SAFETY: forwarded from this function's own safety doc.
        item = unsafe { (*item).li_next };
    }

    copy
}

/// Extend list `l1` with list `l2`'s items, inserted before `bef` (or
/// at the end, if `bef` is null) (`tv_list_extend`).
///
/// # Safety
/// `l1` must be a valid, non-null pointer to a live `ListT`. `l2`, if
/// non-null, must be a valid pointer to a live `ListT`. `bef`, if
/// non-null, must be a valid pointer to an item actually present in
/// `l1`.
pub unsafe fn tv_list_extend(
    l1: *mut crate::eval::typval_defs::ListT,
    l2: *mut crate::eval::typval_defs::ListT,
    bef: *mut crate::eval::typval_defs::ListitemT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut todo = unsafe { tv_list_len(l2) };

    // NULL list is equivalent to an empty list: nothing to do.
    if todo == 0 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let befbef = if bef.is_null() { std::ptr::null_mut() } else { unsafe { (*bef).li_prev } };
    // SAFETY: forwarded from this function's own safety doc.
    let saved_next = if befbef.is_null() { std::ptr::null_mut() } else { unsafe { (*befbef).li_next } };

    // We also quit the loop when we have inserted the original item
    // count of the list, to avoid a hang when extending a list with
    // itself.
    // SAFETY: forwarded from this function's own safety doc.
    let mut item = unsafe { tv_list_first(l2) };
    while !item.is_null() && todo > 0 {
        todo -= 1;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_list_insert_tv(l1, &(*item).li_tv, bef) };
        item = if item == befbef {
            saved_next
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*item).li_next }
        };
    }
}

/// Join list `l`'s own items into a single string, separated by `sep`
/// (`tv_list_join`).
///
/// Simplified into a single pass rather than the original's own
/// two-pass "stringify everything first, to precompute the total
/// allocation size" approach (via a separate `Join` struct array) -
/// Rust's `Vec` already grows efficiently via amortized doubling, so
/// that precomputation isn't needed here for correctness or
/// asymptotic performance.
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT`.
pub unsafe fn tv_list_join(l: *mut crate::eval::typval_defs::ListT, sep: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut first = true;
    // SAFETY: forwarded from this function's own safety doc.
    let mut item = unsafe { tv_list_first(l) };
    while !item.is_null() {
        if !first {
            out.extend_from_slice(sep);
        }
        first = false;
        let tv = unsafe { &(*item).li_tv };
        // SAFETY: forwarded from this function's own safety doc.
        let s = unsafe { crate::eval::encode::encode_tv2echo(tv) };
        out.extend_from_slice(&s);
        // SAFETY: forwarded from this function's own safety doc.
        item = unsafe { (*item).li_next };
    }
    out
}

/// Flatten up to `maxitems` items in `list`, starting at `first`, to
/// depth `maxdepth` (`tv_list_flatten`). When `first` is null, use the
/// first item. Does nothing if `maxdepth` is `0`.
///
/// Each `List`-typed item is replaced, in place, by its own items
/// (recursively, up to `maxdepth`) - e.g. `[1, [2, 3], 4]` flattens to
/// `[1, 2, 3, 4]`. Non-`List` items are left untouched.
///
/// # Safety
/// `list` must be a valid, non-null pointer to a live `ListT`; `first`,
/// if non-null, must be a valid pointer to an item actually present in
/// `list`'s own `li_next` chain. Every `List`-typed item's own
/// pointer, if non-null, must be valid, recursively.
pub unsafe fn tv_list_flatten(
    list: *mut crate::eval::typval_defs::ListT,
    first: *mut crate::eval::typval_defs::ListitemT,
    maxitems: i64,
    maxdepth: i64,
) {
    if maxdepth == 0 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut item = if first.is_null() { unsafe { (*list).lv_first } } else { first };

    let mut done: i64 = 0;
    while !item.is_null() && done < maxitems {
        // SAFETY: forwarded from this function's own safety doc.
        let next = unsafe { (*item).li_next };

        // SAFETY: GLOBALS is only ever accessed through this crate's
        // established single-threaded-main-loop convention.
        if unsafe { crate::globals::GLOBALS.get_mut() }.got_int {
            return;
        }

        // SAFETY: forwarded from this function's own safety doc.
        let itemlist = match unsafe { &(*item).li_tv.value } {
            TypvalValue::List(l) => Some(*l),
            _ => None,
        };
        if let Some(itemlist) = itemlist {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_list_drop_items(list, item, item) };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_list_extend(list, itemlist, next) };

            if maxdepth > 0 {
                // SAFETY: forwarded from this function's own safety doc.
                let item_prev = unsafe { (*item).li_prev };
                let new_first = if item_prev.is_null() {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (*list).lv_first }
                } else {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { (*item_prev).li_next }
                };
                // SAFETY: forwarded from this function's own safety doc.
                let itemlist_len = i64::from(unsafe { tv_list_len(itemlist) });
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_list_flatten(list, new_first, itemlist_len, maxdepth - 1) };
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_clear_simple(&(*item).li_tv) };
            // SAFETY: `item` was just dropped from `list`'s own chain
            // above by `tv_list_drop_items`, and was originally
            // allocated via `tv_list_item_alloc`/`Box::into_raw`
            // (forwarded from this function's own safety doc).
            drop(unsafe { Box::from_raw(item) });
        }

        done += 1;
        item = next;
    }
}

/// Concatenate lists into a new list (`tv_list_concat`).
///
/// Returns `false` on failure. `tv`'s value is always set to a
/// `List`-typed value (a null list, if `l1`/`l2` are both null),
/// matching the original's own `tv->v_type = VAR_LIST` assignment
/// before its own possible early failure return.
///
/// # Safety
/// `l1`/`l2`, if non-null, must be valid pointers to live `ListT`s,
/// forwarded to [`tv_list_copy`]/[`tv_list_extend`]'s own contracts.
pub unsafe fn tv_list_concat(
    l1: *mut crate::eval::typval_defs::ListT,
    l2: *mut crate::eval::typval_defs::ListT,
    tv: &mut TypvalT,
) -> bool {
    tv.v_lock = VarLockStatus::Unlocked;

    let l = if l1.is_null() && l2.is_null() {
        std::ptr::null_mut()
    } else if l1.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_list_copy(std::ptr::null(), l2, false, 0) }
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let l = unsafe { tv_list_copy(std::ptr::null(), l1, false, 0) };
        if !l.is_null() && !l2.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_list_extend(l, l2, std::ptr::null_mut()) };
        }
        l
    };

    if l.is_null() && !(l1.is_null() && l2.is_null()) {
        return false;
    }

    tv.value = TypvalValue::List(l);
    true
}

/// Return a slice of `ol` (from character/item index `n1` to `n2`,
/// inclusive) as a NEW list (`tv_list_slice`).
///
/// # Safety
/// `ol` must be a valid, non-null pointer to a live
/// [`crate::eval::typval_defs::ListT`] with at least `n2 + 1` items
/// (the caller - [`tv_list_slice_or_index`] - is responsible for the
/// same bounds-checking the original performs before calling this).
unsafe fn tv_list_slice(
    ol: *mut crate::eval::typval_defs::ListT,
    n1: crate::eval::typval_defs::VarnumberT,
    n2: crate::eval::typval_defs::VarnumberT,
) -> *mut crate::eval::typval_defs::ListT {
    let l = tv_list_alloc(isize::try_from(n2 - n1 + 1).unwrap_or(0));
    // SAFETY: forwarded from this function's own safety doc.
    let mut item = unsafe { tv_list_find(ol, n1 as i32) };
    let mut n1 = n1;
    while n1 <= n2 {
        // SAFETY: `item` is non-null for every iteration this loop
        // actually reaches, per this function's own safety doc.
        unsafe {
            tv_list_append_tv(l, &(*item).li_tv);
            item = (*item).li_next;
        }
        n1 += 1;
    }
    l
}

/// Apply `[idx]`/`[n1:n2]` indexing or slicing to a `List`
/// (`tv_list_slice_or_index`).
///
/// The original's own `semsg(_(e_list_index_out_of_range_nr), n1_arg)`
/// (a genuine, non-range out-of-bounds index) is omitted, matching
/// this crate's established "skip the display, keep the identical
/// FAIL" policy.
///
/// # Safety
/// `list` must be a valid pointer to the same live list currently held
/// by `rettv.value` (matching the original's own
/// `tv_list_len(rettv->vval.v_list)` self-read).
pub unsafe fn tv_list_slice_or_index(
    list: *mut crate::eval::typval_defs::ListT,
    range: bool,
    n1_arg: crate::eval::typval_defs::VarnumberT,
    n2_arg: crate::eval::typval_defs::VarnumberT,
    exclusive: bool,
    rettv: &mut TypvalT,
    _verbose: bool,
) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let len = crate::eval::typval_defs::VarnumberT::from(unsafe { tv_list_len(list) });
    let mut n1 = n1_arg;
    let mut n2 = n2_arg;

    if n1 < 0 {
        n1 += len;
    }
    if n1 < 0 || n1 >= len {
        // For a range we allow invalid values and return an empty
        // list. A list index out of range is an error.
        if !range {
            return FAIL;
        }
        n1 = len;
    }
    if range {
        if n2 < 0 {
            n2 += len;
        } else if n2 >= len {
            n2 = len - crate::eval::typval_defs::VarnumberT::from(!exclusive);
        }
        if exclusive {
            n2 -= 1;
        }
        if n2 < 0 || n2 + 1 < n1 {
            n2 = -1;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let l = unsafe { tv_list_slice(list, n1, n2) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(rettv) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_list_set_ret(rettv, l) };
    } else {
        // Copy the item to a temporary first, to avoid that clearing
        // the list (which may drop the original list's own reference)
        // makes it invalid before the copy completes.
        let mut var1 = TypvalT::default();
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            let found = tv_list_find(list, n1 as i32);
            tv_copy(&(*found).li_tv, &mut var1);
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(rettv) };
        *rettv = var1;
    }
    OK
}

// Comparison:

/// Return true if `tv` holds a function reference, `Func` or
/// `Partial` (`tv_is_func`, `eval/typval.h`'s own `static inline`).
#[must_use]
pub fn tv_is_func(tv: &TypvalT) -> bool {
    matches!(tv.value, TypvalValue::Func(_) | TypvalValue::Partial(_))
}

/// Recursion depth limit for [`tv_equal`] - reduced each time hit, to
/// avoid endless work on deeply-linked (not necessarily cyclic)
/// structures (`tv_equal_recurse_limit`).
static TV_EQUAL_RECURSE_LIMIT: GlobalCell<i32> = GlobalCell::new(1000);

/// Recursion depth counter shared across the whole mutually-recursive
/// `tv_equal`/`tv_list_equal`/`tv_dict_equal`/`func_equal` family -
/// matches the original's own function-local `static int
/// recursive_cnt` inside `tv_equal` (translated as a module-level
/// `GlobalCell`, since that C idiom has no direct Rust equivalent
/// usable the same way from ordinary safe-looking call sites).
static TV_EQUAL_RECURSIVE_CNT: GlobalCell<i32> = GlobalCell::new(0);

/// Check whether two lists are equal (`tv_list_equal`).
///
/// # Safety
/// `l1`/`l2`, if non-null, must be valid pointers to live
/// [`crate::eval::typval_defs::ListT`]s whose every item's `li_tv`
/// satisfies [`tv_equal`]'s own safety contract.
#[must_use]
pub unsafe fn tv_list_equal(
    l1: *const crate::eval::typval_defs::ListT,
    l2: *const crate::eval::typval_defs::ListT,
    ic: bool,
) -> bool {
    if l1 == l2 {
        return true;
    }
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { tv_list_len(l1) } != unsafe { tv_list_len(l2) } {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { tv_list_len(l1) } == 0 {
        // empty and NULL list are considered equal
        return true;
    }
    if l1.is_null() || l2.is_null() {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut item1 = unsafe { tv_list_first(l1) };
    // SAFETY: forwarded from this function's own safety doc.
    let mut item2 = unsafe { tv_list_first(l2) };
    while !item1.is_null() && !item2.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { tv_equal(&(*item1).li_tv, &(*item2).li_tv, ic) } {
            return false;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            item1 = (*item1).li_next;
            item2 = (*item2).li_next;
        }
    }
    true
}

/// Check whether two dictionaries are equal (`tv_dict_equal`).
///
/// # Safety
/// `d1`/`d2`, if non-null, must be valid pointers to live
/// [`DictT`]s whose every item's `di_tv` satisfies [`tv_equal`]'s own
/// safety contract.
#[must_use]
pub unsafe fn tv_dict_equal(d1: *mut DictT, d2: *mut DictT, ic: bool) -> bool {
    if d1 == d2 {
        return true;
    }
    if tv_dict_len(unsafe { d1.as_ref() }) != tv_dict_len(unsafe { d2.as_ref() }) {
        return false;
    }
    if tv_dict_len(unsafe { d1.as_ref() }) == 0 {
        // empty and NULL dicts are considered equal
        return true;
    }
    if d1.is_null() || d2.is_null() {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let items: Vec<*mut DictitemT> = unsafe { (*d1).dv_index.values().copied().collect() };
    for di1 in items {
        // SAFETY: forwarded from this function's own safety doc. di_key
        // always carries a trailing NUL terminator (matching hi_key's
        // C-string contract - see tv_dict_item_alloc's own doc
        // comment), which tv_dict_find's own key parameter does NOT
        // expect (it takes the "clean" logical name) - strip it here.
        let di_key = unsafe { &(*di1).di_key };
        let key = di_key[..di_key.len() - 1].to_vec();
        // SAFETY: forwarded from this function's own safety doc.
        let Some(di2) = tv_dict_find(unsafe { d2.as_mut() }, &key) else {
            return false;
        };
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { tv_equal(&(*di1).di_tv, &(*di2).di_tv, ic) } {
            return false;
        }
    }
    true
}

/// Check whether two blobs are equal (`tv_blob_equal`).
///
/// # Safety
/// `b1`/`b2`, if non-null, must be valid pointers to live
/// [`crate::eval::typval_defs::BlobT`]s.
#[must_use]
pub unsafe fn tv_blob_equal(
    b1: *const crate::eval::typval_defs::BlobT,
    b2: *const crate::eval::typval_defs::BlobT,
) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let len1 = unsafe { tv_blob_len(b1) };
    // SAFETY: forwarded from this function's own safety doc.
    let len2 = unsafe { tv_blob_len(b2) };

    // empty and NULL are considered the same
    if len1 == 0 && len2 == 0 {
        return true;
    }
    if b1 == b2 {
        return true;
    }
    if len1 != len2 {
        return false;
    }

    for i in 0..len1 {
        // SAFETY: forwarded from this function's own safety doc; i is
        // in [0, len1) and len1 == tv_blob_len(b1)/(b2), so both
        // accesses are in bounds.
        if unsafe { tv_blob_get(b1, i) } != unsafe { tv_blob_get(b2, i) } {
            return false;
        }
    }
    true
}

/// Compare two Vimscript values. Like `"=="`, but strings and numbers
/// are different, as well as floats and numbers (`tv_equal`).
///
/// Too-deeply-nested structures may be considered equal even if they
/// are not (matches the original's own documented caveat).
///
/// # Safety
/// If `tv1`/`tv2`'s value is `List`/`Dict`/`Blob`/`Partial`-typed with
/// a non-null pointer, that pointer must be a valid, live
/// `ListT`/`DictT`/`BlobT`/`PartialT`, recursively satisfying this
/// same contract for every value it (in)directly contains.
pub unsafe fn tv_equal(tv1: &TypvalT, tv2: &TypvalT, ic: bool) -> bool {
    if !(tv_is_func(tv1) && tv_is_func(tv2)) && tv1.value.var_type() != tv2.value.var_type() {
        return false;
    }

    // Catch lists and dicts that have an endless loop by limiting
    // recursiveness to a limit. We guess they are equal then.
    // SAFETY: TV_EQUAL_RECURSIVE_CNT/TV_EQUAL_RECURSE_LIMIT are
    // private, crate-internal GlobalCells only ever touched by this
    // mutually-recursive function family.
    let recursive_cnt = unsafe { *TV_EQUAL_RECURSIVE_CNT.get_mut() };
    if recursive_cnt == 0 {
        unsafe { *TV_EQUAL_RECURSE_LIMIT.get_mut() = 1000 };
    }
    if recursive_cnt >= unsafe { *TV_EQUAL_RECURSE_LIMIT.get_mut() } {
        unsafe { *TV_EQUAL_RECURSE_LIMIT.get_mut() -= 1 };
        return true;
    }

    match &tv1.value {
        TypvalValue::List(l1) => {
            let TypvalValue::List(l2) = &tv2.value else { unreachable!() };
            let (l1, l2) = (*l1, *l2);
            unsafe { *TV_EQUAL_RECURSIVE_CNT.get_mut() += 1 };
            // SAFETY: forwarded from this function's own safety doc.
            let r = unsafe { tv_list_equal(l1, l2, ic) };
            unsafe { *TV_EQUAL_RECURSIVE_CNT.get_mut() -= 1 };
            r
        }
        TypvalValue::Dict(d1) => {
            let TypvalValue::Dict(d2) = &tv2.value else { unreachable!() };
            let (d1, d2) = (*d1, *d2);
            unsafe { *TV_EQUAL_RECURSIVE_CNT.get_mut() += 1 };
            // SAFETY: forwarded from this function's own safety doc.
            let r = unsafe { tv_dict_equal(d1, d2, ic) };
            unsafe { *TV_EQUAL_RECURSIVE_CNT.get_mut() -= 1 };
            r
        }
        TypvalValue::Partial(_) | TypvalValue::Func(_) => {
            let tv1_null_partial = matches!(&tv1.value, TypvalValue::Partial(p) if p.is_null());
            let tv2_null_partial = matches!(&tv2.value, TypvalValue::Partial(p) if p.is_null());
            if tv1_null_partial || tv2_null_partial {
                return false;
            }
            unsafe { *TV_EQUAL_RECURSIVE_CNT.get_mut() += 1 };
            // SAFETY: forwarded from this function's own safety doc.
            let r = unsafe { crate::eval::eval::func_equal(tv1, tv2, ic) };
            unsafe { *TV_EQUAL_RECURSIVE_CNT.get_mut() -= 1 };
            r
        }
        TypvalValue::Blob(b1) => {
            let TypvalValue::Blob(b2) = &tv2.value else { unreachable!() };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_blob_equal(*b1, *b2) }
        }
        TypvalValue::Number(n1) => {
            let TypvalValue::Number(n2) = &tv2.value else { unreachable!() };
            n1 == n2
        }
        TypvalValue::Float(f1) => {
            let TypvalValue::Float(f2) = &tv2.value else { unreachable!() };
            f1 == f2
        }
        TypvalValue::String(_) => {
            let s1 = tv_get_string(tv1);
            let s2 = tv_get_string(tv2);
            crate::mbyte::mb_strcmp_ic(ic, &s1, &s2) == 0
        }
        TypvalValue::Bool(b1) => {
            let TypvalValue::Bool(b2) = &tv2.value else { unreachable!() };
            b1 == b2
        }
        TypvalValue::Special(s1) => {
            let TypvalValue::Special(s2) = &tv2.value else { unreachable!() };
            s1 == s2
        }
        // VAR_UNKNOWN can be the result of an invalid expression,
        // let's say it does not equal anything, not even self.
        TypvalValue::Unknown => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::typval_defs::CallbackType;
    use crate::vim_defs::FAIL;

    /// A minimal, otherwise-zeroed `ListT` for `tv_copy`/`tv_list_ref`
    /// tests - `ListT` deliberately doesn't derive `Default` (its raw
    /// pointer fields have real ownership semantics elsewhere), so
    /// tests needing a standalone instance build one explicitly.
    fn test_list() -> crate::eval::typval_defs::ListT {
        crate::eval::typval_defs::ListT {
            lv_first: std::ptr::null_mut(),
            lv_last: std::ptr::null_mut(),
            lv_watch: std::ptr::null_mut(),
            lv_idx_item: std::ptr::null_mut(),
            lv_copylist: std::ptr::null_mut(),
            lv_used_next: std::ptr::null_mut(),
            lv_used_prev: std::ptr::null_mut(),
            lv_refcount: 0,
            lv_len: 0,
            lv_idx: 0,
            lv_copy_id: 0,
            lv_lock: VarLockStatus::Unlocked,
            lua_table_ref: -1,
        }
    }

    #[test]
    fn tv_dict_item_alloc_copies_key_and_nul_terminates() {
        let item = tv_dict_item_alloc(b"hello");
        unsafe {
            assert_eq!((*item).di_key, b"hello\0");
            assert_eq!((*item).di_flags, dict_item_flags::ALLOC);
            assert!(matches!((*item).di_tv.value, TypvalValue::Unknown));
            tv_dict_item_free(item);
        }
    }

    #[test]
    fn tv_dict_item_free_clears_in_place_when_not_separately_allocated() {
        let mut item = DictitemT {
            di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(42) },
            di_flags: 0, // NOT DI_FLAGS_ALLOC
            di_key: b"x\0".to_vec(),
        };
        unsafe { tv_dict_item_free(&mut item as *mut DictitemT) };
        assert!(matches!(item.di_tv.value, TypvalValue::Unknown));
        // The item itself (a plain stack value here) is untouched/
        // still valid to read - it was never `Box::from_raw`'d.
        assert_eq!(item.di_key, b"x\0");
    }

    #[test]
    fn tv_dict_alloc_and_free_round_trip() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!((*d).dv_refcount, 0);
            assert!((*d).dv_hashtab.hash_find(b"missing").hi_key.is_null());
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_then_find_roundtrip() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let item = tv_dict_item_alloc(b"greeting");
            (*item).di_tv.value = TypvalValue::Number(7);
            assert_eq!(tv_dict_add(&mut *d, item), OK);

            let found = tv_dict_find(Some(&mut *d), b"greeting");
            assert_eq!(found, Some(item));
            assert!(matches!((*found.unwrap()).di_tv.value, TypvalValue::Number(7)));

            assert!(tv_dict_has_key(Some(&mut *d), b"greeting"));
            assert!(!tv_dict_has_key(Some(&mut *d), b"nope"));

            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_duplicate_key_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let item1 = tv_dict_item_alloc(b"k");
            assert_eq!(tv_dict_add(&mut *d, item1), OK);

            let item2 = tv_dict_item_alloc(b"k");
            assert_eq!(tv_dict_add(&mut *d, item2), FAIL);
            // item2 was never added to the dict - free it directly to
            // avoid leaking it in this test.
            tv_dict_item_free(item2);

            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_list_increments_refcount_and_stores_pointer() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        let list = tv_list_alloc(0);
        unsafe {
            assert_eq!((*list).lv_refcount, 0);
            assert_eq!(tv_dict_add_list(&mut *d, b"pos", list), OK);
            assert_eq!((*list).lv_refcount, 1);

            let found = tv_dict_find(Some(&mut *d), b"pos").unwrap();
            assert!(matches!((*found).di_tv.value, TypvalValue::List(p) if p == list));

            // Dropping the dict unrefs (not frees, since the list is
            // still independently reachable via `list` here) the list
            // once (1 -> 0), which frees it - don't touch `list` again
            // after this.
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_list_duplicate_key_leaves_ownership_with_caller() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        let list = tv_list_alloc(0);
        unsafe {
            let existing = tv_dict_item_alloc(b"k");
            assert_eq!(tv_dict_add(&mut *d, existing), OK);

            assert_eq!(tv_dict_add_list(&mut *d, b"k", list), FAIL);
            // Refcount must NOT have been incremented - ownership
            // stayed with the caller, matching the original's
            // "detach so tv_dict_item_free() does not unref it".
            assert_eq!((*list).lv_refcount, 0);

            tv_dict_free(d);
            tv_list_free(list);
        }
    }

    #[test]
    fn tv_dict_add_dict_increments_refcount_and_stores_pointer() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        let inner = tv_dict_alloc();
        unsafe {
            assert_eq!((*inner).dv_refcount, 0);
            assert_eq!(tv_dict_add_dict(&mut *d, b"nested", inner), OK);
            assert_eq!((*inner).dv_refcount, 1);

            let found = tv_dict_find(Some(&mut *d), b"nested").unwrap();
            assert!(matches!((*found).di_tv.value, TypvalValue::Dict(p) if p == inner));

            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_tv_copies_the_value() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let tv = number_tv(42);
            assert_eq!(tv_dict_add_tv(&mut *d, b"answer", &tv), OK);
            let found = tv_dict_find(Some(&mut *d), b"answer").unwrap();
            assert!(matches!((*found).di_tv.value, TypvalValue::Number(42)));
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_nr_stores_number() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_add_nr(&mut *d, b"n", 7), OK);
            let found = tv_dict_find(Some(&mut *d), b"n").unwrap();
            assert!(matches!((*found).di_tv.value, TypvalValue::Number(7)));
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_float_stores_float() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_add_float(&mut *d, b"f", 3.5), OK);
            let found = tv_dict_find(Some(&mut *d), b"f").unwrap();
            assert!(matches!((*found).di_tv.value, TypvalValue::Float(v) if v == 3.5));
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_bool_stores_bool() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(
                tv_dict_add_bool(&mut *d, b"b", crate::eval::typval_defs::BoolVarValue::True),
                OK
            );
            let found = tv_dict_find(Some(&mut *d), b"b").unwrap();
            assert!(matches!(
                (*found).di_tv.value,
                TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True)
            ));
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_str_stores_an_owned_copy() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let mut src = b"hello".to_vec();
            assert_eq!(tv_dict_add_str(&mut *d, b"s", Some(&src)), OK);
            // Mutate the source afterwards to prove it was deep-copied,
            // not aliased.
            src[0] = b'X';
            let found = tv_dict_find(Some(&mut *d), b"s").unwrap();
            assert!(matches!(&(*found).di_tv.value, TypvalValue::String(Some(v)) if v == b"hello"));
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_str_none_stores_absent_string() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_add_str(&mut *d, b"s", None), OK);
            let found = tv_dict_find(Some(&mut *d), b"s").unwrap();
            assert!(matches!(&(*found).di_tv.value, TypvalValue::String(None)));
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_str_grows_past_the_small_hashtab_array() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            for i in 0..30 {
                let key = format!("key{i}");
                assert_eq!(tv_dict_add_str(&mut *d, key.as_bytes(), Some(b"v")), OK, "failed at i={i}");
            }
            assert_eq!(tv_dict_len(d.as_ref()), 30);
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_add_func_stores_nul_stripped_name_and_refs_a_numbered_function() {
        let _lock = crate::globals::global_state_test_lock();
        crate::eval::userfunc::func_init();
        let mut fp = crate::eval::typval_defs::UfuncT {
            uf_name: b"77\0".to_vec(),
            uf_refcount: 1,
            ..Default::default()
        };
        let fp_ptr = &mut fp as *mut crate::eval::typval_defs::UfuncT;
        unsafe { crate::eval::userfunc::func_hashtab_add(fp_ptr) };
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_add_func(&mut *d, b"F", fp_ptr), OK);
            let found = tv_dict_find(Some(&mut *d), b"F").unwrap();
            // The stored name has no trailing NUL, unlike uf_name.
            assert!(matches!(&(*found).di_tv.value, TypvalValue::Func(Some(v)) if v == b"77"));
        }
        // func_ref (called by tv_dict_add_func) found "77" is a
        // numbered function and incremented its real refcount.
        assert_eq!(fp.uf_refcount, 2);
        unsafe { tv_dict_free(d) };
        // Freeing the dict item runs tv_clear_simple on its Func
        // value, calling func_unref and decrementing it back down.
        assert_eq!(fp.uf_refcount, 1);
    }

    #[test]
    fn tv_dict_add_func_with_ordinary_name_leaves_refcount_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        crate::eval::userfunc::func_init();
        let mut fp = crate::eval::typval_defs::UfuncT {
            uf_name: b"MyFunc\0".to_vec(),
            uf_refcount: 1,
            ..Default::default()
        };
        let fp_ptr = &mut fp as *mut crate::eval::typval_defs::UfuncT;
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_add_func(&mut *d, b"F", fp_ptr), OK);
            tv_dict_free(d);
        }
        // "MyFunc" isn't refcounted by name at all (ordinary named
        // functions live for the script's whole lifetime once
        // defined) - func_ref/func_unref were both no-ops.
        assert_eq!(fp.uf_refcount, 1);
    }

    #[test]
    fn tv_dict_find_returns_none_for_missing_key_and_none_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_find(Some(&mut *d), b"absent"), None);
            assert_eq!(tv_dict_find(None, b"absent"), None);
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_get_string_returns_none_for_missing_key() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_get_string(Some(&mut *d), b"absent"), None);
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_get_string_returns_the_stored_string() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_add_str(&mut *d, b"greeting", Some(b"hello")), OK);
            assert_eq!(
                tv_dict_get_string(Some(&mut *d), b"greeting"),
                Some(b"hello".to_vec())
            );
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_get_string_stringifies_a_number_value() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_add_nr(&mut *d, b"count", 42), OK);
            assert_eq!(tv_dict_get_string(Some(&mut *d), b"count"), Some(b"42".to_vec()));
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_get_string_returns_empty_not_none_for_wrong_type() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_add_dict(&mut *d, b"nested", tv_dict_alloc()), OK);
            // Found, but VAR_DICT can't stringify - tv_get_string's own
            // "always Some, empty on error" behavior, NOT None (that's
            // reserved for "key not found" here).
            assert_eq!(tv_dict_get_string(Some(&mut *d), b"nested"), Some(Vec::new()));
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_get_string_chk_returns_def_for_missing_key() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(
                tv_dict_get_string_chk(Some(&mut *d), b"absent", Some(b"fallback".to_vec())),
                Some(b"fallback".to_vec())
            );
            assert_eq!(tv_dict_get_string_chk(Some(&mut *d), b"absent", None), None);
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_get_string_chk_returns_none_for_wrong_type_even_with_def() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_add_dict(&mut *d, b"nested", tv_dict_alloc()), OK);
            // Found, but wrong type - returns None (not `def`), matching
            // the original's own tv_get_string_buf_chk error path.
            assert_eq!(
                tv_dict_get_string_chk(Some(&mut *d), b"nested", Some(b"fallback".to_vec())),
                None
            );
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_get_string_chk_returns_the_stored_string_when_found() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_add_str(&mut *d, b"greeting", Some(b"hi")), OK);
            assert_eq!(
                tv_dict_get_string_chk(Some(&mut *d), b"greeting", Some(b"fallback".to_vec())),
                Some(b"hi".to_vec())
            );
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_item_remove_removes_from_both_hashtab_and_index() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let item = tv_dict_item_alloc(b"temp");
            assert_eq!(tv_dict_add(&mut *d, item), OK);
            assert!(tv_dict_has_key(Some(&mut *d), b"temp"));

            tv_dict_item_remove(&mut *d, item);
            assert!(!tv_dict_has_key(Some(&mut *d), b"temp"));
            assert!((*d).dv_index.is_empty());

            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_free_contents_frees_every_item_and_resets_hashtab() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            for key in [b"a".as_slice(), b"b".as_slice(), b"c".as_slice()] {
                let item = tv_dict_item_alloc(key);
                assert_eq!(tv_dict_add(&mut *d, item), OK);
            }
            assert_eq!((*d).dv_index.len(), 3);

            tv_dict_free_contents(d);
            assert!((*d).dv_index.is_empty());
            assert!(!tv_dict_has_key(Some(&mut *d), b"a"));

            tv_dict_free_dict(d);
        }
    }

    #[test]
    fn tv_dict_set_keys_readonly_marks_every_existing_item() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            for key in [b"a".as_slice(), b"b".as_slice()] {
                let item = tv_dict_item_alloc(key);
                assert_eq!(tv_dict_add(&mut *d, item), OK);
            }
            // Both items start with only DI_FLAGS_ALLOC set (from
            // tv_dict_item_alloc's own separately-allocated key), not
            // yet RO/FIX.
            for item in (*d).dv_index.values() {
                assert_eq!((**item).di_flags, dict_item_flags::ALLOC);
            }

            tv_dict_set_keys_readonly(d);

            // RO|FIX are ADDED on top of the pre-existing ALLOC flag
            // (matches the original's own `|=`, not an overwrite).
            for item in (*d).dv_index.values() {
                assert_eq!(
                    (**item).di_flags,
                    dict_item_flags::ALLOC | dict_item_flags::RO | dict_item_flags::FIX
                );
            }

            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_item_free_decrements_dict_value_refcount_instead_of_panicking() {
        // Dict/List/Blob-valued items are now properly handled by
        // tv_clear_simple (calling tv_dict_unref/tv_list_unref/
        // tv_blob_unref) - only Partial still panics (see below).
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            (*d).dv_refcount = 2;
            let mut item = DictitemT {
                di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(d) },
                di_flags: 0,
                di_key: b"x\0".to_vec(),
            };
            tv_dict_item_free(&mut item as *mut DictitemT);
            assert_eq!((*d).dv_refcount, 1);
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_dict_item_free_null_partial_is_a_safe_noop() {
        // partial_unref(NULL) is always a safe no-op, matching the
        // original - no longer panics now that partial_T has real
        // fields (see partial_unref's own doc comment).
        let mut item = DictitemT {
            di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Partial(std::ptr::null_mut()) },
            di_flags: 0,
            di_key: b"x\0".to_vec(),
        };
        unsafe { tv_dict_item_free(&mut item as *mut DictitemT) };
    }

    #[test]
    fn tv_dict_item_free_decrements_partial_refcount_instead_of_panicking() {
        let pt = Box::into_raw(Box::new(crate::eval::typval_defs::PartialT {
            pt_refcount: 2,
            ..Default::default()
        }));
        let mut item = DictitemT {
            di_tv: TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Partial(pt) },
            di_flags: 0,
            di_key: b"x\0".to_vec(),
        };
        unsafe {
            tv_dict_item_free(&mut item as *mut DictitemT);
            assert_eq!((*pt).pt_refcount, 1);
            // Still refcount 1 - not freed yet, safe to free directly.
            drop(Box::from_raw(pt));
        }
    }

    #[test]
    fn multiple_dicts_maintain_the_gc_linked_list_correctly() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = tv_dict_alloc();
        let d2 = tv_dict_alloc();
        let d3 = tv_dict_alloc();
        unsafe {
            // Most-recently-allocated dict is at the head.
            assert_eq!(*GC_FIRST_DICT.get_mut(), d3);
            assert_eq!((*d3).dv_used_next, d2);
            assert_eq!((*d2).dv_used_next, d1);
            assert!((*d1).dv_used_next.is_null());
            assert!((*d3).dv_used_prev.is_null());
            assert_eq!((*d2).dv_used_prev, d3);
            assert_eq!((*d1).dv_used_prev, d2);

            // Remove the middle one; the list should re-link around it.
            tv_dict_free(d2);
            assert_eq!((*d3).dv_used_next, d1);
            assert_eq!((*d1).dv_used_prev, d3);

            tv_dict_free(d3);
            tv_dict_free(d1);
            assert!((*GC_FIRST_DICT.get_mut()).is_null());
        }
    }

    #[test]
    fn tv_get_number_returns_number_directly() {
        let tv = number_tv(42);
        assert_eq!(tv_get_number(&tv), 42);
    }

    #[test]
    fn tv_get_number_chk_parses_numeric_string() {
        let tv = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::String(Some(b"123".to_vec())),
        };
        let mut error = false;
        assert_eq!(tv_get_number_chk(&tv, Some(&mut error)), 123);
        assert!(!error);
    }

    #[test]
    fn tv_get_number_chk_parses_negative_numeric_string() {
        let tv = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::String(Some(b"-7".to_vec())),
        };
        assert_eq!(tv_get_number(&tv), -7);
    }

    #[test]
    fn tv_get_number_chk_none_string_is_zero() {
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) };
        assert_eq!(tv_get_number(&tv), 0);
    }

    #[test]
    fn tv_get_number_chk_non_numeric_string_parses_as_zero() {
        // vim_str2nr finds no leading digits - "0, no advance", not an
        // error at this layer (matches the original: no emsg happens
        // for VAR_STRING, regardless of content).
        let tv = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::String(Some(b"abc".to_vec())),
        };
        let mut error = false;
        assert_eq!(tv_get_number_chk(&tv, Some(&mut error)), 0);
        assert!(!error);
    }

    #[test]
    fn tv_get_number_chk_bool_true_and_false() {
        let t = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True),
        };
        let f = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::False),
        };
        assert_eq!(tv_get_number(&t), 1);
        assert_eq!(tv_get_number(&f), 0);
    }

    #[test]
    fn tv_get_number_chk_special_is_zero() {
        let tv = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
        };
        assert_eq!(tv_get_number(&tv), 0);
    }

    #[test]
    fn tv_get_number_chk_wrong_type_sets_error_and_returns_zero_with_ret_error() {
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) };
        let mut error = false;
        assert_eq!(tv_get_number_chk(&tv, Some(&mut error)), 0);
        assert!(error);
    }

    #[test]
    fn tv_get_number_chk_wrong_type_returns_minus_one_without_ret_error() {
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) };
        assert_eq!(tv_get_number_chk(&tv, None), -1);
    }

    #[test]
    fn tv_get_number_wrong_type_family_all_error() {
        for value in [
            TypvalValue::Func(None),
            TypvalValue::Partial(std::ptr::null_mut()),
            TypvalValue::List(std::ptr::null_mut()),
            TypvalValue::Dict(std::ptr::null_mut()),
            TypvalValue::Blob(std::ptr::null_mut()),
            TypvalValue::Float(1.5),
            TypvalValue::Unknown,
        ] {
            let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value };
            let mut error = false;
            assert_eq!(tv_get_number_chk(&tv, Some(&mut error)), 0);
            assert!(error, "expected an error flag for this value");
        }
    }

    #[test]
    fn tv_get_bool_is_same_computation_as_tv_get_number() {
        let tv = number_tv(7);
        assert_eq!(tv_get_bool(&tv), tv_get_number(&tv));
    }

    #[test]
    fn tv_get_bool_chk_forwards_error_flag() {
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) };
        let mut error = false;
        assert_eq!(tv_get_bool_chk(&tv, Some(&mut error)), 0);
        assert!(error);
    }

    #[test]
    fn tv_get_float_number_widens() {
        assert_eq!(tv_get_float(&number_tv(7)), 7.0);
    }

    #[test]
    fn tv_get_float_float_passes_through() {
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Float(2.5) };
        assert_eq!(tv_get_float(&tv), 2.5);
    }

    #[test]
    fn tv_get_float_everything_else_is_zero() {
        for value in [
            TypvalValue::String(Some(b"1.5".to_vec())),
            TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True),
            TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
            TypvalValue::List(std::ptr::null_mut()),
            TypvalValue::Dict(std::ptr::null_mut()),
            TypvalValue::Blob(std::ptr::null_mut()),
            TypvalValue::Unknown,
        ] {
            let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value };
            assert_eq!(tv_get_float(&tv), 0.0);
        }
    }

    #[test]
    fn tv_get_float_chk_number_and_float_succeed() {
        assert_eq!(tv_get_float_chk(&number_tv(7)), Some(7.0));
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Float(2.5) };
        assert_eq!(tv_get_float_chk(&tv), Some(2.5));
    }

    #[test]
    fn tv_get_float_chk_everything_else_is_none() {
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(Some(b"1.5".to_vec())) };
        assert_eq!(tv_get_float_chk(&tv), None);
    }

    #[test]
    fn tv2bool_number_nonzero_is_true() {
        assert!(unsafe { tv2bool(&number_tv(1)) });
        assert!(!unsafe { tv2bool(&number_tv(0)) });
    }

    #[test]
    fn tv2bool_float_nonzero_is_true() {
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Float(0.5) };
        assert!(unsafe { tv2bool(&tv) });
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Float(0.0) };
        assert!(!unsafe { tv2bool(&tv) });
    }

    #[test]
    fn tv2bool_string_empty_vs_nonempty() {
        let tv = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::String(Some(b"x".to_vec())),
        };
        assert!(unsafe { tv2bool(&tv) });
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(Some(Vec::new())) };
        assert!(!unsafe { tv2bool(&tv) });
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) };
        assert!(!unsafe { tv2bool(&tv) });
    }

    #[test]
    fn tv2bool_null_containers_are_false() {
        for value in [
            TypvalValue::List(std::ptr::null_mut()),
            TypvalValue::Dict(std::ptr::null_mut()),
            TypvalValue::Blob(std::ptr::null_mut()),
            TypvalValue::Partial(std::ptr::null_mut()),
        ] {
            let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value };
            assert!(!unsafe { tv2bool(&tv) });
        }
    }

    #[test]
    fn tv2bool_nonempty_list_is_true() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe { tv_list_append_number(l, 0) };
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(l) };
        assert!(unsafe { tv2bool(&tv) });
        unsafe { tv_list_unref(l) };
    }

    #[test]
    fn tv2bool_special_null_is_false() {
        let tv = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
        };
        assert!(!unsafe { tv2bool(&tv) });
    }

    #[test]
    fn tv2bool_unknown_is_false() {
        let tv = TypvalT::default();
        assert!(!unsafe { tv2bool(&tv) });
    }

    #[test]
    fn tv_get_string_chk_number_formats_as_decimal() {
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(-42) };
        assert_eq!(tv_get_string_chk(&tv), Some(b"-42".to_vec()));
    }

    #[test]
    fn tv_get_string_chk_string_returns_its_own_bytes() {
        let tv = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::String(Some(b"hello".to_vec())),
        };
        assert_eq!(tv_get_string_chk(&tv), Some(b"hello".to_vec()));
    }

    #[test]
    fn tv_get_string_chk_none_string_is_empty_not_none() {
        // Matches the original: VAR_STRING with a NULL v_string
        // returns "" (empty), NOT an error/NULL - only the
        // Func/Partial/List/Dict/Blob/Unknown branches return None.
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(None) };
        assert_eq!(tv_get_string_chk(&tv), Some(Vec::new()));
    }

    #[test]
    fn tv_get_string_chk_bool_and_special() {
        let t = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True),
        };
        let f = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::False),
        };
        let null = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
        };
        assert_eq!(tv_get_string_chk(&t), Some(b"v:true".to_vec()));
        assert_eq!(tv_get_string_chk(&f), Some(b"v:false".to_vec()));
        assert_eq!(tv_get_string_chk(&null), Some(b"v:null".to_vec()));
    }

    #[test]
    fn tv_get_string_chk_float_uses_g_formatting() {
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Float(1.5) };
        assert_eq!(tv_get_string_chk(&tv), Some(b"1.5".to_vec()));
    }

    #[test]
    fn tv_get_string_chk_wrong_type_family_all_none() {
        for value in [
            TypvalValue::Func(None),
            TypvalValue::Partial(std::ptr::null_mut()),
            TypvalValue::List(std::ptr::null_mut()),
            TypvalValue::Dict(std::ptr::null_mut()),
            TypvalValue::Blob(std::ptr::null_mut()),
            TypvalValue::Unknown,
        ] {
            let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value };
            assert_eq!(tv_get_string_chk(&tv), None, "expected None for {tv:?}");
        }
    }

    #[test]
    fn tv_get_string_returns_empty_vec_instead_of_none_on_error() {
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) };
        assert_eq!(tv_get_string(&tv), Vec::<u8>::new());
    }

    #[test]
    fn tv_get_string_matches_chk_on_success() {
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(7) };
        assert_eq!(tv_get_string(&tv), tv_get_string_chk(&tv).unwrap());
    }

    /// `fmt_g` test vectors, cross-checked against real `gcc`/glibc
    /// `printf("%g", ...)` output (see this function's own doc
    /// comment) - covers zero/signed-zero, NaN/Infinity, the
    /// fixed-vs-scientific boundary in both directions, exponent-carry
    /// on rounding (`9999999.0` -> `"1e+07"`, not `"9999999"`), and an
    /// exact-tie-that-isn't-really-a-tie case (`1.999995` -> `"1.99999"`,
    /// since its true `f64` value is very slightly below the decimal
    /// midpoint).
    #[test]
    #[allow(clippy::approx_constant)] // intentional many-digit rounding test value, not meant to represent math::PI
    fn fmt_g_matches_glibc_reference_outputs() {
        let cases: &[(f64, &[u8])] = &[
            (0.0, b"0"),
            (-0.0, b"-0"),
            (1.0, b"1"),
            (1.5, b"1.5"),
            (100.0, b"100"),
            (123456.0, b"123456"),
            (1234567.0, b"1.23457e+06"),
            (0.0001, b"0.0001"),
            (0.00001, b"1e-05"),
            (1_000_000.0, b"1e+06"),
            (3.14159265358979, b"3.14159"),
            (-2.5, b"-2.5"),
            (1e20, b"1e+20"),
            (1e-20, b"1e-20"),
            (123.456, b"123.456"),
            (0.1, b"0.1"),
            (10.0, b"10"),
            (1e300, b"1e+300"),
            (1e-300, b"1e-300"),
            (999999.0, b"999999"),
            (9999999.0, b"1e+07"),
            (0.00009999, b"9.999e-05"),
            (-1234567.0, b"-1.23457e+06"),
            (-999999.5, b"-1e+06"),
            (1.999995, b"1.99999"),
            (1.9999995, b"2"),
            (5555555.0, b"5.55556e+06"),
        ];
        for (value, expected) in cases {
            assert_eq!(fmt_g(*value), *expected, "fmt_g({value:?})");
        }
    }

    #[test]
    fn fmt_g_nan_and_infinity() {
        assert_eq!(fmt_g(f64::NAN), b"nan");
        assert_eq!(fmt_g(f64::INFINITY), b"inf");
        assert_eq!(fmt_g(f64::NEG_INFINITY), b"-inf");
    }

    #[test]
    fn strip_trailing_zeros_removes_fractional_zeros_and_dot() {
        assert_eq!(strip_trailing_zeros("1.50000"), b"1.5");
        assert_eq!(strip_trailing_zeros("100.000"), b"100");
        assert_eq!(strip_trailing_zeros("123456"), b"123456");
        assert_eq!(strip_trailing_zeros("0.000"), b"0");
    }

    #[test]
    fn tv_copy_number_resets_lock_and_copies_value() {
        let from = TypvalT { v_lock: VarLockStatus::Locked, value: TypvalValue::Number(42) };
        let mut to = TypvalT::default();
        unsafe { tv_copy(&from, &mut to) };
        assert_eq!(to.v_lock, VarLockStatus::Unlocked);
        assert!(matches!(to.value, TypvalValue::Number(42)));
    }

    #[test]
    fn tv_copy_string_deep_copies_the_bytes() {
        let from = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(Some(b"hi".to_vec())) };
        let mut to = TypvalT::default();
        unsafe { tv_copy(&from, &mut to) };
        // Mutate `to`'s string and confirm `from`'s own copy is
        // unaffected - proving this is a real deep copy, not a shared
        // reference (Rust's `Vec<u8>::clone()` already guarantees
        // this; the assertion just makes the intent explicit).
        if let TypvalValue::String(Some(s)) = &mut to.value {
            s.push(b'!');
        }
        assert!(matches!(&from.value, TypvalValue::String(Some(s)) if s == b"hi"));
        assert!(matches!(&to.value, TypvalValue::String(Some(s)) if s == b"hi!"));
    }

    #[test]
    fn tv_copy_blob_increments_shared_refcount() {
        let mut blob =
            crate::eval::typval_defs::BlobT { bv_refcount: 5, ..Default::default() };
        let from = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Blob(&mut blob as *mut crate::eval::typval_defs::BlobT),
        };
        let mut to = TypvalT::default();
        unsafe { tv_copy(&from, &mut to) };
        assert_eq!(blob.bv_refcount, 6);
        // `to` shares the SAME blob pointer as `from` (a reference
        // copy, not a container deep-copy) - matching the original's
        // own documented "copies its reference" behavior.
        assert!(matches!(to.value, TypvalValue::Blob(p) if std::ptr::eq(p, &blob)));
    }

    #[test]
    fn tv_copy_blob_null_pointer_is_a_noop() {
        let from = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(std::ptr::null_mut()) };
        let mut to = TypvalT::default();
        unsafe { tv_copy(&from, &mut to) }; // must not panic/segfault
        assert!(matches!(to.value, TypvalValue::Blob(p) if p.is_null()));
    }

    #[test]
    fn tv_copy_list_increments_shared_refcount() {
        let mut list = test_list();
        let from = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::List(&mut list as *mut crate::eval::typval_defs::ListT),
        };
        let mut to = TypvalT::default();
        unsafe { tv_copy(&from, &mut to) };
        assert_eq!(list.lv_refcount, 1);
    }

    #[test]
    fn tv_copy_dict_increments_shared_refcount() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let from = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(d) };
            let mut to = TypvalT::default();
            tv_copy(&from, &mut to);
            assert_eq!((*d).dv_refcount, 1);
            tv_dict_free(d);
        }
    }

    #[test]
    fn tv_copy_partial_null_is_a_safe_noop() {
        // A null partial is always safe to copy (no refcount touched),
        // matching the original - no longer panics now that partial_T
        // has real fields (see tv_copy's own doc comment).
        let from = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Partial(std::ptr::null_mut()),
        };
        let mut to = TypvalT::default();
        unsafe { tv_copy(&from, &mut to) };
        assert!(matches!(to.value, TypvalValue::Partial(p) if p.is_null()));
    }

    #[test]
    fn tv_copy_partial_increments_refcount() {
        let pt = Box::into_raw(Box::new(crate::eval::typval_defs::PartialT {
            pt_refcount: 1,
            ..Default::default()
        }));
        unsafe {
            let from = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Partial(pt) };
            let mut to = TypvalT::default();
            tv_copy(&from, &mut to);
            assert_eq!((*pt).pt_refcount, 2);
            assert!(matches!(to.value, TypvalValue::Partial(p) if p == pt));

            // Clean up both references directly (no real allocator/
            // partial_unref-based teardown exercised here - this test
            // is only checking the refcount arithmetic).
            (*pt).pt_refcount = 0;
            drop(Box::from_raw(pt));
        }
    }

    #[test]
    fn tv_list_ref_null_is_noop() {
        unsafe { tv_list_ref(std::ptr::null_mut()) }; // must not panic
    }

    #[test]
    fn tv_list_ref_increments_refcount() {
        let mut list = test_list();
        unsafe { tv_list_ref(&mut list as *mut crate::eval::typval_defs::ListT) };
        assert_eq!(list.lv_refcount, 1);
    }

    #[test]
    fn tv_dict_item_copy_is_a_genuinely_separate_allocation() {
        let original = tv_dict_item_alloc(b"count");
        unsafe {
            (*original).di_tv.value = TypvalValue::Number(99);

            let copy = tv_dict_item_copy(original);
            assert_ne!(original, copy);
            assert_eq!((*copy).di_key, b"count\0");
            assert!(matches!((*copy).di_tv.value, TypvalValue::Number(99)));

            // Mutating the copy doesn't affect the original.
            (*copy).di_tv.value = TypvalValue::Number(1);
            assert!(matches!((*original).di_tv.value, TypvalValue::Number(99)));

            tv_dict_item_free(original);
            tv_dict_item_free(copy);
        }
    }

    #[test]
    fn tv_dict_unref_null_is_noop() {
        unsafe { tv_dict_unref(std::ptr::null_mut()) };
    }

    #[test]
    fn tv_dict_unref_decrements_without_freeing_when_still_referenced() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            (*d).dv_refcount = 2;
            tv_dict_unref(d);
            assert_eq!((*d).dv_refcount, 1);
            tv_dict_free(d); // clean up manually since refcount never hit 0
        }
    }

    #[test]
    fn tv_blob_alloc_and_free_round_trip() {
        let b = tv_blob_alloc();
        unsafe {
            assert_eq!((*b).bv_refcount, 0);
            assert_eq!((*b).bv_ga.ga_len, 0);
            tv_blob_free(b);
        }
    }

    #[test]
    fn tv_blob_unref_null_is_noop() {
        unsafe { tv_blob_unref(std::ptr::null_mut()) };
    }

    #[test]
    fn tv_blob_unref_decrements_without_freeing_when_still_referenced() {
        let b = tv_blob_alloc();
        unsafe {
            (*b).bv_refcount = 2;
            tv_blob_unref(b);
            assert_eq!((*b).bv_refcount, 1);
            tv_blob_free(b);
        }
    }

    #[test]
    fn tv_blob_len_null_is_zero() {
        assert_eq!(unsafe { tv_blob_len(std::ptr::null()) }, 0);
    }

    #[test]
    fn tv_blob_len_reads_ga_len() {
        let b = tv_blob_alloc();
        unsafe {
            (*b).bv_ga.ga_concat_len(b"abc");
            assert_eq!(tv_blob_len(b), 3);
            tv_blob_free(b);
        }
    }

    #[test]
    fn tv_blob_check_index_accepts_zero_through_bloblen_inclusive() {
        // n1 == bloblen is valid (it's the "append/insert at end" index).
        assert_eq!(tv_blob_check_index(3, 0, false), OK);
        assert_eq!(tv_blob_check_index(3, 3, false), OK);
        assert_eq!(tv_blob_check_index(3, -1, false), FAIL);
        assert_eq!(tv_blob_check_index(3, 4, false), FAIL);
    }

    #[test]
    fn tv_blob_check_range_rejects_negative_out_of_range_or_reversed() {
        assert_eq!(tv_blob_check_range(3, 0, 2, false), OK);
        assert_eq!(tv_blob_check_range(3, 0, -1, false), FAIL);
        assert_eq!(tv_blob_check_range(3, 0, 3, false), FAIL); // n2 == bloblen is out of range.
        assert_eq!(tv_blob_check_range(3, 2, 1, false), FAIL); // n2 < n1.
    }

    #[test]
    fn tv_blob_set_ret_wires_value_and_increments_refcount() {
        let b = tv_blob_alloc();
        let mut tv = TypvalT::default();
        unsafe {
            tv_blob_set_ret(&mut tv, b);
            assert_eq!((*b).bv_refcount, 1);
            match tv.value {
                TypvalValue::Blob(p) => assert_eq!(p, b),
                _ => panic!("expected a Blob-typed value"),
            }
            tv_blob_free(b);
        }
    }

    #[test]
    fn tv_blob_set_ret_null_is_safe() {
        let mut tv = TypvalT::default();
        unsafe { tv_blob_set_ret(&mut tv, std::ptr::null_mut()) };
        assert!(matches!(tv.value, TypvalValue::Blob(p) if p.is_null()));
    }

    #[test]
    fn partial_unref_null_is_noop() {
        unsafe { partial_unref(std::ptr::null_mut()) };
    }

    #[test]
    fn partial_unref_decrements_without_freeing_when_still_referenced() {
        let pt = Box::into_raw(Box::new(crate::eval::typval_defs::PartialT {
            pt_refcount: 2,
            ..Default::default()
        }));
        unsafe {
            partial_unref(pt);
            assert_eq!((*pt).pt_refcount, 1);
            // Still referenced - free directly rather than double-unref.
            drop(Box::from_raw(pt));
        }
    }

    #[test]
    fn partial_unref_frees_and_releases_dict_at_zero_refcount() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!((*d).dv_refcount, 0);
            (*d).dv_refcount = 1;

            let pt = Box::into_raw(Box::new(crate::eval::typval_defs::PartialT {
                pt_refcount: 1,
                pt_dict: d,
                ..Default::default()
            }));
            // Refcount hits 0 here - partial_free runs, which unrefs
            // `d` (1 -> 0), freeing it too. Don't touch `pt`/`d` again
            // after this.
            partial_unref(pt);
        }
    }

    #[test]
    fn partial_unref_frees_and_clears_argv_at_zero_refcount() {
        let pt = Box::into_raw(Box::new(crate::eval::typval_defs::PartialT {
            pt_refcount: 1,
            pt_argv: vec![number_tv(1), number_tv(2)],
            ..Default::default()
        }));
        // Refcount hits 0 - partial_free runs, clearing each pt_argv
        // entry via tv_clear_simple (a no-op release for plain
        // Numbers, but still exercises the loop) and freeing `pt`
        // itself. Nothing further to assert on `pt` after this - the
        // absence of a crash/leak-sanitizer complaint is the check.
        unsafe { partial_unref(pt) };
    }

    #[test]
    fn partial_unref_releases_pt_func_refcount_when_pt_name_absent() {
        let mut fp = crate::eval::typval_defs::UfuncT { uf_refcount: 2, ..Default::default() };
        let pt = Box::into_raw(Box::new(crate::eval::typval_defs::PartialT {
            pt_refcount: 1,
            pt_name: None,
            pt_func: &mut fp as *mut crate::eval::typval_defs::UfuncT,
            ..Default::default()
        }));
        // Refcount hits 0 here - partial_free runs, which calls the
        // real func_ptr_unref on pt_func (since pt_name is absent),
        // decrementing fp's own refcount (2 -> 1, still referenced,
        // so func_ptr_unref's own unimplemented!() branch is never
        // reached).
        unsafe { partial_unref(pt) };
        assert_eq!(fp.uf_refcount, 1);
    }

    #[test]
    fn partial_unref_skips_pt_func_release_when_pt_name_present() {
        let mut fp = crate::eval::typval_defs::UfuncT { uf_refcount: 2, ..Default::default() };
        let pt = Box::into_raw(Box::new(crate::eval::typval_defs::PartialT {
            pt_refcount: 1,
            pt_name: Some(b"MyFunc".to_vec()),
            pt_func: &mut fp as *mut crate::eval::typval_defs::UfuncT,
            ..Default::default()
        }));
        // pt_name is present ("MyFunc" - an ordinary named function,
        // not a numbered function or lambda) - the real func_unref
        // runs, but func_name_refcount("MyFunc") is false (only
        // numbered functions/lambdas are refcounted by name), so it
        // returns immediately without touching fp at all. fp's own
        // refcount must stay untouched here - see the sibling test
        // below for the case where func_unref DOES fire.
        unsafe { partial_unref(pt) };
        assert_eq!(fp.uf_refcount, 2);
    }

    #[test]
    fn partial_unref_releases_by_name_when_pt_name_is_a_numbered_function() {
        let _lock = crate::globals::global_state_test_lock();
        crate::eval::userfunc::func_init();
        let mut fp = crate::eval::typval_defs::UfuncT {
            uf_refcount: 2,
            uf_name: b"123\0".to_vec(),
            ..Default::default()
        };
        let fp_ptr = &mut fp as *mut crate::eval::typval_defs::UfuncT;
        unsafe { crate::eval::userfunc::func_hashtab_add(fp_ptr) };
        let pt = Box::into_raw(Box::new(crate::eval::typval_defs::PartialT {
            pt_refcount: 1,
            pt_name: Some(b"123".to_vec()),
            pt_func: std::ptr::null_mut(),
            ..Default::default()
        }));
        // pt_name is present AND a numbered function ("123") - the
        // real func_unref looks it up via find_func (registered above)
        // and decrements ITS refcount for real (2 -> 1).
        unsafe { partial_unref(pt) };
        assert_eq!(fp.uf_refcount, 1);
    }

    // ---- callback_from_typval / callback_free -----------------------------

    #[test]
    fn callback_from_typval_partial_bumps_refcount() {
        let mut pt = PartialT { pt_refcount: 1, ..Default::default() };
        let pt_ptr = &mut pt as *mut PartialT;
        let tv = TypvalT { value: TypvalValue::Partial(pt_ptr), ..Default::default() };
        let cb = unsafe { callback_from_typval(&tv) }.unwrap();
        assert_eq!(pt.pt_refcount, 2);
        assert!(matches!(cb, Callback::Partial(p) if p == pt_ptr));
    }

    #[test]
    fn callback_from_typval_null_partial_is_an_error() {
        let tv = TypvalT { value: TypvalValue::Partial(std::ptr::null_mut()), ..Default::default() };
        assert!(unsafe { callback_from_typval(&tv) }.is_none());
    }

    #[test]
    fn callback_from_typval_a_string_starting_with_a_digit_is_an_error() {
        // Real function NAMES never start with a digit - a numeric-
        // looking string is rejected outright, matching the original's
        // own `ascii_isdigit(*arg->vval.v_string)` guard.
        let tv = TypvalT { value: TypvalValue::String(Some(b"123notafunc".to_vec())), ..Default::default() };
        assert!(unsafe { callback_from_typval(&tv) }.is_none());
    }

    #[test]
    fn callback_from_typval_ordinary_string_name_becomes_a_funcref() {
        let _lock = crate::globals::global_state_test_lock();
        let tv = TypvalT { value: TypvalValue::String(Some(b"MyFunc".to_vec())), ..Default::default() };
        let cb = unsafe { callback_from_typval(&tv) }.unwrap();
        assert!(matches!(cb, Callback::Funcref(name) if name == b"MyFunc"));
    }

    #[test]
    fn callback_from_typval_ordinary_func_name_becomes_a_funcref() {
        let _lock = crate::globals::global_state_test_lock();
        let tv = TypvalT { value: TypvalValue::Func(Some(b"MyFunc".to_vec())), ..Default::default() };
        let cb = unsafe { callback_from_typval(&tv) }.unwrap();
        assert!(matches!(cb, Callback::Funcref(name) if name == b"MyFunc"));
    }

    #[test]
    fn callback_from_typval_script_local_string_name_is_expanded() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        let (sid, _) = crate::runtime::new_script_item(None);
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid = sid;

        let tv = TypvalT { value: TypvalValue::String(Some(b"s:MyFunc".to_vec())), ..Default::default() };
        let cb = unsafe { callback_from_typval(&tv) }.unwrap();
        let expected = format!("<SNR>{sid}_MyFunc").into_bytes();
        assert!(matches!(cb, Callback::Funcref(name) if name == expected));
    }

    #[test]
    fn callback_from_typval_script_local_func_name_is_not_re_expanded() {
        // Unlike the String case, a Func value's own name is used
        // VERBATIM - matching the original's own `if (arg->v_type ==
        // VAR_STRING) { get_scriptlocal_funcname(...) }` guard, which
        // never fires for VAR_FUNC.
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        let (sid, _) = crate::runtime::new_script_item(None);
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid = sid;

        let tv = TypvalT { value: TypvalValue::Func(Some(b"s:MyFunc".to_vec())), ..Default::default() };
        let cb = unsafe { callback_from_typval(&tv) }.unwrap();
        assert!(matches!(cb, Callback::Funcref(name) if name == b"s:MyFunc"));
    }

    #[test]
    fn callback_from_typval_empty_string_is_none() {
        let tv = TypvalT { value: TypvalValue::String(Some(Vec::new())), ..Default::default() };
        assert_eq!(unsafe { callback_from_typval(&tv) }.unwrap().kind(), CallbackType::None);
    }

    #[test]
    fn callback_from_typval_null_string_is_an_error() {
        let tv = TypvalT { value: TypvalValue::String(None), ..Default::default() };
        assert!(unsafe { callback_from_typval(&tv) }.is_none());
    }

    #[test]
    fn callback_from_typval_special_is_none() {
        let tv = TypvalT {
            value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
            ..Default::default()
        };
        assert_eq!(unsafe { callback_from_typval(&tv) }.unwrap().kind(), CallbackType::None);
    }

    #[test]
    fn callback_from_typval_number_zero_is_none() {
        let tv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        assert_eq!(unsafe { callback_from_typval(&tv) }.unwrap().kind(), CallbackType::None);
    }

    #[test]
    fn callback_from_typval_nonzero_number_is_an_error() {
        let tv = TypvalT { value: TypvalValue::Number(1), ..Default::default() };
        assert!(unsafe { callback_from_typval(&tv) }.is_none());
    }

    #[test]
    fn callback_free_partial_releases_it() {
        let pt = Box::into_raw(Box::new(PartialT { pt_refcount: 1, ..Default::default() }));
        let mut cb = Callback::Partial(pt);
        callback_free(&mut cb);
        assert_eq!(cb.kind(), CallbackType::None);
        // A clean, crash-free run (no double-free/UB) IS the check -
        // matching this crate's own established `partial_unref`-style
        // convention for proving a real free happened.
    }

    #[test]
    fn callback_free_funcref_releases_a_numbered_function() {
        let _lock = crate::globals::global_state_test_lock();
        crate::eval::userfunc::func_init();
        let mut fp = crate::eval::typval_defs::UfuncT {
            uf_refcount: 2,
            uf_name: b"77\0".to_vec(),
            ..Default::default()
        };
        let fp_ptr = &mut fp as *mut crate::eval::typval_defs::UfuncT;
        unsafe { crate::eval::userfunc::func_hashtab_add(fp_ptr) };
        let mut cb = Callback::Funcref(b"77".to_vec());
        callback_free(&mut cb);
        assert_eq!(fp.uf_refcount, 1);
        assert_eq!(cb.kind(), CallbackType::None);
    }

    #[test]
    fn callback_free_none_is_a_noop() {
        let mut cb = Callback::None;
        callback_free(&mut cb);
        assert_eq!(cb.kind(), CallbackType::None);
    }

    #[test]
    fn callback_copy_partial_increments_its_reference() {
        let partial = Box::into_raw(Box::new(PartialT {
            pt_refcount: 1,
            ..Default::default()
        }));
        let mut source = Callback::Partial(partial);
        let mut dest = Callback::None;
        unsafe { callback_copy(&mut dest, &source) };
        assert_eq!(unsafe { (*partial).pt_refcount }, 2);
        callback_free(&mut dest);
        assert_eq!(unsafe { (*partial).pt_refcount }, 1);
        callback_free(&mut source);
    }

    #[test]
    fn callback_copy_funcref_increments_its_function_reference() {
        let _lock = crate::globals::global_state_test_lock();
        crate::eval::userfunc::func_init();
        let mut function = crate::eval::typval_defs::UfuncT {
            uf_refcount: 2,
            uf_name: b"88\0".to_vec(),
            ..Default::default()
        };
        let function_ptr =
            std::ptr::from_mut::<crate::eval::typval_defs::UfuncT>(
                &mut function,
            );
        unsafe { crate::eval::userfunc::func_hashtab_add(function_ptr) };
        let mut source = Callback::Funcref(b"88".to_vec());
        let mut dest = Callback::None;

        unsafe { callback_copy(&mut dest, &source) };

        assert_eq!(unsafe { (*function_ptr).uf_refcount }, 3);
        assert!(matches!(
            &dest,
            Callback::Funcref(name) if name == b"88"
        ));
        callback_free(&mut dest);
        assert_eq!(unsafe { (*function_ptr).uf_refcount }, 2);
        callback_free(&mut source);
        assert_eq!(unsafe { (*function_ptr).uf_refcount }, 1);
    }

    #[test]
    fn callback_copy_none_remains_none() {
        let mut dest = Callback::None;
        unsafe { callback_copy(&mut dest, &Callback::None) };
        assert_eq!(dest.kind(), CallbackType::None);
    }

    #[test]
    #[should_panic(expected = "Lua callbacks need api_new_luaref")]
    fn callback_copy_defers_lua_references() {
        let mut dest = Callback::None;
        unsafe { callback_copy(&mut dest, &Callback::Lua(1)) };
    }

    #[test]
    fn callback_put_copies_funcref_partial_and_none_values() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tv = TypvalT::default();
        unsafe { callback_put(&Callback::Funcref(b"MyFunc".to_vec()), &mut tv) };
        assert!(matches!(
            &tv.value,
            TypvalValue::Func(Some(name)) if name == b"MyFunc"
        ));

        let partial = Box::into_raw(Box::new(PartialT {
            pt_refcount: 1,
            ..Default::default()
        }));
        let mut partial_tv = TypvalT::default();
        let mut callback = Callback::Partial(partial);
        unsafe { callback_put(&callback, &mut partial_tv) };
        assert_eq!(unsafe { (*partial).pt_refcount }, 2);
        unsafe { tv_clear_simple(&partial_tv) };
        callback_free(&mut callback);

        let mut none_tv = TypvalT::default();
        unsafe { callback_put(&Callback::None, &mut none_tv) };
        assert_eq!(
            none_tv.value,
            TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null)
        );
    }

    // ---- tv_dict_get_callback -----------------------------------------

    #[test]
    fn tv_dict_get_callback_missing_key_returns_true_with_none() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        let mut result = Callback::Funcref(b"stale".to_vec());
        let ok = unsafe { tv_dict_get_callback(d, b"MyCb", &mut result) };
        assert!(ok, "a missing key is a real success, matching the original's own `return true;`");
        assert_eq!(result.kind(), CallbackType::None, "must be reset even on the not-found path");
        unsafe { tv_dict_unref(d) };
    }

    #[test]
    fn callback_to_string_formats_funcref_partial_and_none() {
        assert_eq!(
            unsafe {
                callback_to_string(&Callback::Funcref(
                    b"MyFunc".to_vec(),
                ))
            },
            b"<vim function: MyFunc>"
        );

        let mut partial = PartialT {
            pt_name: Some(b"BoundFunc".to_vec()),
            ..Default::default()
        };
        assert_eq!(
            unsafe {
                callback_to_string(&Callback::Partial(
                    std::ptr::addr_of_mut!(partial),
                ))
            },
            b"<vim partial: BoundFunc>"
        );
        assert_eq!(
            unsafe { callback_to_string(&Callback::None) },
            Vec::<u8>::new()
        );
    }

    #[test]
    fn callback_to_string_uses_c_string_bounds_and_truncates() {
        assert_eq!(
            unsafe {
                callback_to_string(&Callback::Funcref(
                    b"Name\0ignored".to_vec(),
                ))
            },
            b"<vim function: Name>"
        );
        let text = unsafe {
            callback_to_string(&Callback::Funcref(vec![b'x'; 200]))
        };
        assert_eq!(text.len(), 99);
        assert!(text.starts_with(b"<vim function: "));
    }

    #[test]
    #[should_panic(expected = "nlua_funcref_str")]
    fn callback_to_string_lua_needs_the_lua_host() {
        let _ = unsafe { callback_to_string(&Callback::Lua(7)) };
    }

    #[test]
    fn tv_callback_equal_matches_each_callback_variant() {
        assert!(tv_callback_equal(&Callback::None, &Callback::None));
        assert!(!tv_callback_equal(
            &Callback::None,
            &Callback::Funcref(Vec::new())
        ));
        assert!(tv_callback_equal(
            &Callback::Funcref(b"Func".to_vec()),
            &Callback::Funcref(b"Func".to_vec())
        ));
        assert!(!tv_callback_equal(
            &Callback::Funcref(b"Func".to_vec()),
            &Callback::Funcref(b"Other".to_vec())
        ));

        let mut first = PartialT::default();
        let mut second = PartialT::default();
        let first_ptr = std::ptr::addr_of_mut!(first);
        assert!(tv_callback_equal(
            &Callback::Partial(first_ptr),
            &Callback::Partial(first_ptr)
        ));
        assert!(!tv_callback_equal(
            &Callback::Partial(first_ptr),
            &Callback::Partial(std::ptr::addr_of_mut!(second))
        ));
        assert!(tv_callback_equal(
            &Callback::Lua(4),
            &Callback::Lua(4)
        ));
        assert!(!tv_callback_equal(
            &Callback::Lua(4),
            &Callback::Lua(5)
        ));
    }

    #[test]
    fn dict_watcher_add_match_and_remove_round_trip() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = tv_dict_alloc();
        let callback = Callback::Funcref(b"Watcher".to_vec());
        unsafe {
            tv_dict_watcher_add(dict, b"prefix*", callback.clone());
        }
        assert!(unsafe { tv_dict_is_watched(dict) });
        assert_eq!(unsafe { (*dict).watchers.len() }, 1);
        assert!(tv_dict_watcher_matches(
            unsafe { &(&(*dict).watchers)[0] },
            b"prefix_key"
        ));
        assert!(!tv_dict_watcher_matches(
            unsafe { &(&(*dict).watchers)[0] },
            b"other"
        ));
        assert!(unsafe {
            tv_dict_watcher_remove(dict, b"prefix*", &callback)
        });
        assert!(!unsafe { tv_dict_is_watched(dict) });
        unsafe { tv_dict_free(dict) };
    }

    #[test]
    fn dict_watcher_exact_pattern_does_not_match_a_longer_key() {
        let watcher = crate::eval::typval_defs::DictWatcher {
            callback: Callback::None,
            key_pattern: b"key".to_vec(),
            busy: false,
            needs_free: false,
        };
        assert!(tv_dict_watcher_matches(&watcher, b"key"));
        assert!(!tv_dict_watcher_matches(&watcher, b"key_more"));
    }

    #[test]
    fn dict_watcher_remove_defers_while_the_queue_is_busy() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = tv_dict_alloc();
        unsafe {
            tv_dict_watcher_add(
                dict,
                b"first",
                Callback::Funcref(b"First".to_vec()),
            );
            tv_dict_watcher_add(
                dict,
                b"second",
                Callback::Funcref(b"Second".to_vec()),
            );
            (&mut (*dict).watchers)[0].busy = true;
        }
        assert!(unsafe {
            tv_dict_watcher_remove(
                dict,
                b"second",
                &Callback::Funcref(b"Second".to_vec()),
            )
        });
        assert_eq!(unsafe { (*dict).watchers.len() }, 2);
        assert!(unsafe { (&(*dict).watchers)[1].needs_free });
        unsafe { tv_dict_free(dict) };
    }

    #[test]
    fn dict_free_contents_releases_watcher_callbacks() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = tv_dict_alloc();
        let partial = Box::into_raw(Box::new(PartialT {
            pt_refcount: 2,
            ..Default::default()
        }));
        unsafe {
            tv_dict_watcher_add(
                dict,
                b"*",
                Callback::Partial(partial),
            );
            tv_dict_free_contents(dict);
        }
        assert_eq!(unsafe { (*partial).pt_refcount }, 1);
        assert!(!unsafe { tv_dict_is_watched(dict) });
        unsafe {
            partial_unref(partial);
            tv_dict_free_dict(dict);
        }
    }

    #[test]
    fn dict_watcher_notify_calls_matching_funcref_and_restores_busy() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = tv_dict_alloc();
        unsafe { (*dict).dv_refcount += 1 };
        let item = tv_dict_item_alloc(b"watched");
        unsafe {
            (*item).di_tv.value = TypvalValue::Number(7);
            tv_dict_add(&mut *dict, item);
            tv_dict_watcher_add(
                dict,
                b"watched",
                Callback::Funcref(b"get".to_vec()),
            );
            tv_dict_watcher_notify(
                dict,
                b"watched",
                Some(&TypvalT {
                    value: TypvalValue::Number(8),
                    ..TypvalT::default()
                }),
                Some(&TypvalT {
                    value: TypvalValue::Number(7),
                    ..TypvalT::default()
                }),
            );
        }

        assert_eq!(unsafe { (*dict).watchers.len() }, 1);
        assert!(!unsafe { (&(*dict).watchers)[0].busy });
        assert!(!unsafe { (&(*dict).watchers)[0].needs_free });
        assert_eq!(unsafe { (*dict).dv_refcount }, 1);
        unsafe { tv_dict_unref(dict) };
    }

    #[test]
    fn dict_watcher_notify_skips_nonmatching_watchers() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = tv_dict_alloc();
        unsafe {
            (*dict).dv_refcount += 1;
            tv_dict_watcher_add(
                dict,
                b"wanted",
                Callback::Funcref(b"get".to_vec()),
            );
            tv_dict_watcher_notify(dict, b"other", None, None);
        }
        assert_eq!(unsafe { (*dict).watchers.len() }, 1);
        assert!(!unsafe { (&(*dict).watchers)[0].busy });
        unsafe { tv_dict_unref(dict) };
    }

    #[test]
    fn dict_watcher_notify_frees_deferred_watchers_after_iteration() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = tv_dict_alloc();
        unsafe {
            (*dict).dv_refcount += 1;
            tv_dict_watcher_add(
                dict,
                b"*",
                Callback::Funcref(b"get".to_vec()),
            );
            (&mut (*dict).watchers)[0].needs_free = true;
            tv_dict_watcher_notify(dict, b"key", None, None);
        }
        assert!(!unsafe { tv_dict_is_watched(dict) });
        unsafe { tv_dict_unref(dict) };
    }

    #[test]
    fn dict_remove_notifies_watchers() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = tv_dict_alloc();
        unsafe { (*dict).dv_refcount += 1 };
        let item = tv_dict_item_alloc(b"key");
        unsafe {
            (*item).di_tv.value = TypvalValue::Number(7);
            tv_dict_add(&mut *dict, item);
            tv_dict_watcher_add(
                dict,
                b"key",
                Callback::Funcref(b"get".to_vec()),
            );
            (&mut (*dict).watchers)[0].needs_free = true;
        }
        let args = [
            TypvalT {
                value: TypvalValue::Dict(dict),
                ..TypvalT::default()
            },
            TypvalT {
                value: TypvalValue::String(Some(b"key".to_vec())),
                ..TypvalT::default()
            },
        ];
        let mut rettv = TypvalT::default();
        unsafe { tv_dict_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(7));
        assert!(!unsafe { tv_dict_is_watched(dict) });
        unsafe { tv_dict_unref(dict) };
    }

    #[test]
    fn dict_extend_notifies_for_new_and_replaced_values() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = tv_dict_alloc();
        let d2 = tv_dict_alloc();
        unsafe {
            (*d1).dv_refcount += 1;
            (*d2).dv_refcount += 1;
            let first = tv_dict_item_alloc(b"key");
            (*first).di_tv.value = TypvalValue::Number(1);
            tv_dict_add(&mut *d1, first);
            let second = tv_dict_item_alloc(b"key");
            (*second).di_tv.value = TypvalValue::Number(2);
            tv_dict_add(&mut *d2, second);
            tv_dict_watcher_add(
                d1,
                b"*",
                Callback::Funcref(b"get".to_vec()),
            );
            (&mut (*d1).watchers)[0].needs_free = true;
            tv_dict_extend(d1, d2, b"force");
        }
        assert_eq!(
            unsafe { tv_dict_get_number(Some(&mut *d1), b"key") },
            2
        );
        assert!(!unsafe { tv_dict_is_watched(d1) });

        unsafe {
            tv_dict_watcher_add(
                d1,
                b"*",
                Callback::Funcref(b"get".to_vec()),
            );
            (&mut (*d1).watchers)[0].needs_free = true;
            let new_item = tv_dict_item_alloc(b"new");
            (*new_item).di_tv.value = TypvalValue::Number(3);
            tv_dict_add(&mut *d2, new_item);
            tv_dict_extend(d1, d2, b"keep");
        }
        assert_eq!(
            unsafe { tv_dict_get_number(Some(&mut *d1), b"new") },
            3
        );
        assert!(!unsafe { tv_dict_is_watched(d1) });

        unsafe {
            tv_dict_unref(d1);
            tv_dict_unref(d2);
        }
    }

    #[test]
    fn tv_dict_get_callback_null_dict_returns_true_with_none() {
        let mut result = Callback::Funcref(b"stale".to_vec());
        let ok = unsafe { tv_dict_get_callback(std::ptr::null_mut(), b"MyCb", &mut result) };
        assert!(ok);
        assert_eq!(result.kind(), CallbackType::None);
    }

    #[test]
    fn tv_dict_get_callback_wrong_type_returns_false_and_leaves_result_none() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        tv_dict_add_nr(unsafe { &mut *d }, b"MyCb", 42);
        let mut result = Callback::None;
        let ok = unsafe { tv_dict_get_callback(d, b"MyCb", &mut result) };
        assert!(!ok, "a Number is neither tv_is_func nor a String - a real failure");
        assert_eq!(result.kind(), CallbackType::None);
        unsafe { tv_dict_unref(d) };
    }

    #[test]
    fn tv_dict_get_callback_string_value_becomes_a_funcref() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        tv_dict_add_str(unsafe { &mut *d }, b"MyCb", Some(b"PlainFunc"));
        let mut result = Callback::None;
        let ok = unsafe { tv_dict_get_callback(d, b"MyCb", &mut result) };
        assert!(ok);
        assert!(matches!(&result, Callback::Funcref(name) if name == b"PlainFunc"));
        callback_free(&mut result);
        unsafe { tv_dict_unref(d) };
    }

    #[test]
    fn tv_dict_get_callback_func_value_becomes_a_funcref() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        let di = tv_dict_item_alloc(b"MyCb");
        unsafe { (*di).di_tv.value = TypvalValue::Func(Some(b"PlainFunc".to_vec())) };
        unsafe { tv_dict_add(&mut *d, di) };
        let mut result = Callback::None;
        let ok = unsafe { tv_dict_get_callback(d, b"MyCb", &mut result) };
        assert!(ok);
        assert!(matches!(&result, Callback::Funcref(name) if name == b"PlainFunc"));
        callback_free(&mut result);
        unsafe { tv_dict_unref(d) };
    }

    #[test]
    fn tv_dict_get_callback_binds_selfdict_for_a_dict_function() {
        // Exercises the exact real-world reason set_selfdict/
        // make_partial must treat a plain String identically to a
        // Func value (see make_partial's own doc comment) - a String
        // naming a real FC_DICT function must come back as a Partial
        // bound to `d`, not silently discarded.
        let _lock = crate::globals::global_state_test_lock();
        crate::eval::userfunc::func_init();
        let mut fp = Box::new(crate::eval::typval_defs::UfuncT {
            uf_name: b"DictFunc\0".to_vec(),
            uf_flags: crate::eval::userfunc::fc_flags::DICT,
            ..Default::default()
        });
        unsafe { crate::eval::userfunc::func_hashtab_add(fp.as_mut() as *mut crate::eval::typval_defs::UfuncT) };

        let d = tv_dict_alloc();
        unsafe { (*d).dv_refcount = 1 };
        tv_dict_add_str(unsafe { &mut *d }, b"MyCb", Some(b"DictFunc"));
        let mut result = Callback::None;
        let ok = unsafe { tv_dict_get_callback(d, b"MyCb", &mut result) };
        assert!(ok);
        let Callback::Partial(pt) = result else { panic!("expected a bound Partial") };
        assert!(!pt.is_null());
        unsafe {
            assert_eq!((*pt).pt_dict, d);
            assert!((*pt).pt_auto);
            assert_eq!((*pt).pt_name.as_deref(), Some(&b"DictFunc"[..]));
            // d's own refcount: 1 (test's own hold) + 1 (the new
            // partial's own bound hold).
            assert_eq!((*d).dv_refcount, 2);
            partial_unref(pt);
        }
        unsafe { tv_dict_unref(d) };
    }

    #[test]
    fn tv_list_alloc_and_free_round_trip() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(0);
        unsafe {
            assert_eq!((*l).lv_refcount, 0);
            assert_eq!((*l).lv_len, 0);
            assert!((*l).lv_first.is_null());
            tv_list_free(l);
        }
    }

    #[test]
    fn list_contents_cleanup_releases_a_self_reference() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(gc_first_list_is_empty());
        let list = tv_list_alloc(1);
        unsafe {
            tv_list_ref(list);
            tv_list_append_tv(
                list,
                &TypvalT {
                    value: TypvalValue::List(list),
                    ..Default::default()
                },
            );
            assert_eq!((*list).lv_refcount, 2);

            tv_list_free_contents(list);
            assert_eq!((*list).lv_refcount, 1);
            assert_eq!((*list).lv_len, 0);

            tv_list_unref(list);
        }
        assert!(gc_first_list_is_empty());
    }

    #[test]
    fn dict_contents_cleanup_releases_a_self_reference() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(gc_first_dict_is_empty());
        let dict = tv_dict_alloc();
        unsafe {
            (*dict).dv_refcount = 1;
            let item = tv_dict_item_alloc(b"self");
            (*item).di_tv.value = TypvalValue::Dict(dict);
            assert_eq!(
                tv_dict_add(&mut *dict, item),
                OK
            );
            (*dict).dv_refcount += 1;
            assert_eq!((*dict).dv_refcount, 2);

            tv_dict_free_contents(dict);
            assert_eq!((*dict).dv_refcount, 1);
            assert!((*dict).dv_index.is_empty());

            tv_dict_unref(dict);
        }
        assert!(gc_first_dict_is_empty());
    }

    #[test]
    fn deeply_nested_dict_cleanup_uses_an_explicit_worklist() {
        let _lock = crate::globals::global_state_test_lock();
        let depth = if cfg!(miri) { 512 } else { 20_000 };
        assert!(gc_first_dict_is_empty());
        let mut child = tv_dict_alloc();
        tv_dict_add_nr(unsafe { &mut *child }, b"value", 1);
        for _ in 0..depth {
            let parent = tv_dict_alloc();
            assert_eq!(
                unsafe {
                    tv_dict_add_dict(
                        &mut *parent,
                        b"child",
                        child,
                    )
                },
                OK
            );
            child = parent;
        }

        unsafe { tv_dict_unref(child) };
        assert!(gc_first_dict_is_empty());
    }

    #[test]
    fn alternating_list_dict_cleanup_uses_one_worklist() {
        enum Nested {
            List(*mut ListT),
            Dict(*mut DictT),
        }

        let _lock = crate::globals::global_state_test_lock();
        let depth = if cfg!(miri) { 512 } else { 20_000 };
        assert!(gc_first_list_is_empty());
        assert!(gc_first_dict_is_empty());
        let first = tv_list_alloc(1);
        unsafe { tv_list_append_number(first, 1) };
        let mut nested = Nested::List(first);

        for _ in 0..depth {
            nested = match nested {
                Nested::List(child) => {
                    let parent = tv_dict_alloc();
                    assert_eq!(
                        unsafe {
                            tv_dict_add_tv(
                                &mut *parent,
                                b"child",
                                &TypvalT {
                                    value: TypvalValue::List(child),
                                    ..Default::default()
                                },
                            )
                        },
                        OK
                    );
                    Nested::Dict(parent)
                }
                Nested::Dict(child) => {
                    let parent = tv_list_alloc(1);
                    unsafe {
                        tv_list_append_tv(
                            parent,
                            &TypvalT {
                                value: TypvalValue::Dict(child),
                                ..Default::default()
                            },
                        )
                    };
                    Nested::List(parent)
                }
            };
        }

        unsafe {
            match nested {
                Nested::List(list) => tv_list_unref(list),
                Nested::Dict(dict) => tv_dict_unref(dict),
            }
        }
        assert!(gc_first_list_is_empty());
        assert!(gc_first_dict_is_empty());
    }

    #[test]
    fn deeply_nested_partial_cleanup_uses_an_explicit_worklist() {
        let depth = if cfg!(miri) { 512 } else { 20_000 };
        let mut child = Box::into_raw(Box::new(PartialT::default()));
        for _ in 0..depth {
            unsafe { (*child).pt_refcount += 1 };
            child = Box::into_raw(Box::new(PartialT {
                pt_argv: vec![TypvalT {
                    value: TypvalValue::Partial(child),
                    ..Default::default()
                }],
                ..Default::default()
            }));
        }

        unsafe { partial_unref(child) };
    }

    #[test]
    fn tv_list_alloc_ret_sets_rettv_and_refs_the_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT { v_lock: VarLockStatus::Locked, ..Default::default() };
        let l = unsafe { tv_list_alloc_ret(&mut rettv, 0) };
        assert_eq!(rettv.value, TypvalValue::List(l));
        assert_eq!(rettv.v_lock, VarLockStatus::Unlocked);
        unsafe {
            assert_eq!((*l).lv_refcount, 1);
            tv_list_unref(l);
        }
    }

    #[test]
    fn tv_list_unref_null_is_noop() {
        unsafe { tv_list_unref(std::ptr::null_mut()) };
    }

    #[test]
    fn tv_list_unref_decrements_without_freeing_when_still_referenced() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(0);
        unsafe {
            (*l).lv_refcount = 2;
            tv_list_unref(l);
            assert_eq!((*l).lv_refcount, 1);
            tv_list_free(l); // clean up manually since refcount never hit 0
        }
    }

    #[test]
    fn multiple_lists_maintain_the_gc_linked_list_correctly() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = tv_list_alloc(0);
        let l2 = tv_list_alloc(0);
        unsafe {
            assert_eq!(*GC_FIRST_LIST.get_mut(), l2);
            assert_eq!((*l2).lv_used_next, l1);
            assert!((*l1).lv_used_next.is_null());

            tv_list_free(l1);
            assert!((*l2).lv_used_next.is_null());

            tv_list_free(l2);
            assert!((*GC_FIRST_LIST.get_mut()).is_null());
        }
    }

    #[test]
    fn get_func_tv_releases_a_list_argument_after_the_call() {
        // Regression test: crate::eval::userfunc::get_func_tv must
        // clear every parsed argument after call_func returns, matching
        // the original's own `while (--argcount >= 0)
        // tv_clear(&argvars[argcount]);`. Discovered via an end-to-end
        // `max([3, 7, 1])` test leaking a list into this same
        // GC_FIRST_LIST forever (deterministically breaking
        // multiple_lists_maintain_the_gc_linked_list_correctly's own
        // "GC list starts empty" assumption whenever it happened to run
        // afterward) - this pins the fix directly, checking
        // GC_FIRST_LIST's own state before/after instead of relying on
        // an unrelated test's own assertions to notice a regression.
        let _lock = crate::globals::global_state_test_lock();
        assert!(unsafe { *GC_FIRST_LIST.get_mut() }.is_null(), "no list should be live before this test");

        let mut evalarg =
            crate::eval::eval::EvalargT { eval_flags: crate::eval::eval::EVAL_EVALUATE, ..Default::default() };
        let mut rettv = TypvalT::default();
        let (ret, _consumed) = unsafe {
            crate::eval::userfunc::get_func_tv(b"max", &mut rettv, b"([3, 7, 1])", Some(&mut evalarg), true)
        };
        assert_eq!(ret, OK);
        assert_eq!(rettv.value, TypvalValue::Number(7));

        // The list literal's own list must have been fully released -
        // nothing else in the whole call holds a reference to it, so a
        // leak would leave it dangling here, still linked into
        // GC_FIRST_LIST despite being unreachable by any name.
        assert!(unsafe { *GC_FIRST_LIST.get_mut() }.is_null(), "max()'s own list argument must be released");
    }

    fn number_tv(n: crate::eval::typval_defs::VarnumberT) -> TypvalT {
        TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(n) }
    }

    #[test]
    fn tv_list_append_tv_builds_a_list_in_order() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(3);
        unsafe {
            for n in [1, 2, 3] {
                tv_list_append_tv(l, &number_tv(n));
            }

            assert_eq!((*l).lv_len, 3);
            let item1 = (*l).lv_first;
            assert!(matches!((*item1).li_tv.value, TypvalValue::Number(1)));
            let item2 = (*item1).li_next;
            assert!(matches!((*item2).li_tv.value, TypvalValue::Number(2)));
            let item3 = (*item2).li_next;
            assert!(matches!((*item3).li_tv.value, TypvalValue::Number(3)));
            assert!((*item3).li_next.is_null());
            assert_eq!((*l).lv_last, item3);

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_append_allocated_string_moves_the_string_in() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe {
            tv_list_append_allocated_string(l, Some(b"moved".to_vec()));
            assert_eq!((*l).lv_len, 1);
            let item = tv_list_find(l, 0);
            assert!(!item.is_null());
            assert!(
                matches!(&(*item).li_tv.value, TypvalValue::String(Some(s)) if s == b"moved")
            );
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_append_allocated_string_appends_a_null_string() {
        // The original represents "no string" as a NULL v_string; this
        // crate spells that None, and it must still append an ITEM.
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe {
            tv_list_append_allocated_string(l, None);
            assert_eq!((*l).lv_len, 1);
            let item = tv_list_find(l, 0);
            assert!(matches!(&(*item).li_tv.value, TypvalValue::String(None)));
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_append_owned_tv_moves_the_value_in() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe {
            let tv = TypvalT {
                v_lock: VarLockStatus::Unlocked,
                value: TypvalValue::String(Some(b"owned".to_vec())),
            };
            let stored = tv_list_append_owned_tv(l, tv);
            assert!(matches!(&(*stored).value, TypvalValue::String(Some(s)) if s == b"owned"));
            assert_eq!((*l).lv_len, 1);
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_append_dict_increments_refcount_and_stores_pointer() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!((*d).dv_refcount, 0);
            tv_list_append_dict(l, d);
            assert_eq!((*d).dv_refcount, 1);
            assert_eq!((*l).lv_len, 1);
            assert!(matches!((*(*l).lv_first).li_tv.value, TypvalValue::Dict(p) if p == d));
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_append_list_increments_refcount_and_stores_pointer() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        let inner = tv_list_alloc(0);
        unsafe {
            assert_eq!((*inner).lv_refcount, 0);
            tv_list_append_list(l, inner);
            assert_eq!((*inner).lv_refcount, 1);
            assert_eq!((*l).lv_len, 1);
            assert!(matches!((*(*l).lv_first).li_tv.value, TypvalValue::List(p) if p == inner));
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_append_string_copies_bytes() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe {
            let mut src = b"hi".to_vec();
            tv_list_append_string(l, Some(&src));
            src[0] = b'X';
            assert!(matches!(&(*(*l).lv_first).li_tv.value, TypvalValue::String(Some(s)) if s == b"hi"));
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_append_string_none_stores_absent_string() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe {
            tv_list_append_string(l, None);
            assert!(matches!(&(*(*l).lv_first).li_tv.value, TypvalValue::String(None)));
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_append_number_appends_value() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(2);
        unsafe {
            tv_list_append_number(l, 10);
            tv_list_append_number(l, 20);
            assert_eq!((*l).lv_len, 2);
            assert!(matches!((*(*l).lv_first).li_tv.value, TypvalValue::Number(10)));
            assert!(matches!((*(*l).lv_last).li_tv.value, TypvalValue::Number(20)));
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_insert_before_existing_item_and_at_end() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(2);
        unsafe {
            tv_list_append_tv(l, &number_tv(1));
            tv_list_append_tv(l, &number_tv(3));
            let item1 = (*l).lv_first;
            let item3 = (*item1).li_next;

            // Insert 2 before item3.
            tv_list_insert_tv(l, &number_tv(2), item3);
            assert_eq!((*l).lv_len, 3);
            let item2 = (*item1).li_next;
            assert!(matches!((*item2).li_tv.value, TypvalValue::Number(2)));
            assert_eq!((*item2).li_next, item3);
            assert_eq!((*item3).li_prev, item2);

            // Insert 4 at the end (item == NULL).
            tv_list_insert_tv(l, &number_tv(4), std::ptr::null_mut());
            assert_eq!((*l).lv_len, 4);
            assert_eq!((*item3).li_next, (*l).lv_last);
            assert!(matches!((*(*l).lv_last).li_tv.value, TypvalValue::Number(4)));

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_first_returns_none_for_null_and_empty_list() {
        assert!(unsafe { tv_list_first(std::ptr::null()) }.is_null());
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(0);
        unsafe {
            assert!(tv_list_first(l).is_null());
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_first_returns_lv_first() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe {
            tv_list_append_tv(l, &number_tv(9));
            assert_eq!(tv_list_first(l), (*l).lv_first);
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_len_null_is_zero() {
        assert_eq!(unsafe { tv_list_len(std::ptr::null()) }, 0);
    }

    #[test]
    fn tv_list_len_reads_lv_len() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(2);
        unsafe {
            tv_list_append_tv(l, &number_tv(1));
            tv_list_append_tv(l, &number_tv(2));
            assert_eq!(tv_list_len(l), 2);
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_copy_null_orig_is_null() {
        assert!(unsafe { tv_list_copy(std::ptr::null(), std::ptr::null_mut(), false, 0) }.is_null());
    }

    #[test]
    fn tv_list_copy_shallow_copies_items_in_order() {
        let _lock = crate::globals::global_state_test_lock();
        let orig = tv_list_alloc(3);
        unsafe {
            for n in [1, 2, 3] {
                tv_list_append_tv(orig, &number_tv(n));
            }
            let copy = tv_list_copy(std::ptr::null(), orig, false, 0);
            assert!(!copy.is_null());
            assert_eq!((*copy).lv_len, 3);
            assert_eq!((*copy).lv_refcount, 1);
            let mut item = (*copy).lv_first;
            for expected in [1, 2, 3] {
                assert!(matches!((*item).li_tv.value, TypvalValue::Number(n) if n == expected));
                item = (*item).li_next;
            }
            // The copy is a genuinely separate list, not an alias.
            assert_ne!(copy, orig);
            tv_list_free(orig);
            tv_list_free(copy);
        }
    }

    #[test]
    fn tv_list_copy_of_empty_list_is_empty_not_null() {
        let _lock = crate::globals::global_state_test_lock();
        let orig = tv_list_alloc(0);
        unsafe {
            let copy = tv_list_copy(std::ptr::null(), orig, false, 0);
            assert!(!copy.is_null());
            assert_eq!((*copy).lv_len, 0);
            tv_list_free(orig);
            tv_list_free(copy);
        }
    }

    #[test]
    fn tv_list_copy_deep_of_empty_list_does_not_panic() {
        // Matches the original's own per-item (not upfront) `deep`
        // check inside its copy loop - an empty list never reaches
        // the unimplemented!() branch.
        let _lock = crate::globals::global_state_test_lock();
        let orig = tv_list_alloc(0);
        unsafe {
            let copy = tv_list_copy(std::ptr::null(), orig, true, 0);
            assert!(!copy.is_null());
            assert_eq!((*copy).lv_len, 0);
            tv_list_free(orig);
            tv_list_free(copy);
        }
    }

    #[test]
    fn tv_list_copy_deep_copies_a_nested_list_recursively() {
        let _lock = crate::globals::global_state_test_lock();
        let inner = tv_list_alloc(1);
        unsafe { tv_list_ref(inner) };
        unsafe { tv_list_append_number(&mut *inner, 1) };
        let orig = tv_list_alloc(1);
        unsafe { tv_list_append_owned_tv(orig, TypvalT { value: TypvalValue::List(inner), ..Default::default() }) };

        unsafe {
            let copy = tv_list_copy(std::ptr::null(), orig, true, 0);
            assert!(!copy.is_null());
            let outer_item = tv_list_first(copy);
            let TypvalValue::List(inner_copy) = (*outer_item).li_tv.value else { panic!("expected a List") };
            // The nested list was ALSO copied - a genuinely separate
            // list, not the same pointer as the original's own inner
            // list.
            assert_ne!(inner_copy, inner);
            assert_eq!((*inner_copy).lv_refcount, 1);

            // Mutating the nested copy must not affect the nested
            // original.
            let inner_copy_item = tv_list_first(inner_copy);
            (*inner_copy_item).li_tv.value = TypvalValue::Number(99);
            let inner_orig_item = tv_list_first(inner);
            assert_eq!((*inner_orig_item).li_tv.value, TypvalValue::Number(1));

            tv_list_unref(orig);
            tv_list_unref(copy);
        }
    }

    #[test]
    fn tv_list_copy_deep_with_noref_makes_a_second_copy_of_a_shared_reference() {
        // Same list referenced twice ([inner, inner]) - a real
        // copyID of 0 (as "noref=1" in deepcopy() terms) makes TWO
        // separate copies, matching the original's own documented
        // deepcopy({expr}, 1) behavior exactly.
        let _lock = crate::globals::global_state_test_lock();
        let inner = tv_list_alloc(0);
        unsafe { tv_list_ref(inner) };
        let orig = tv_list_alloc(2);
        unsafe {
            tv_list_append_owned_tv(orig, TypvalT { value: TypvalValue::List(inner), ..Default::default() });
            tv_list_ref(inner);
            tv_list_append_owned_tv(orig, TypvalT { value: TypvalValue::List(inner), ..Default::default() });
        }

        unsafe {
            let copy = tv_list_copy(std::ptr::null(), orig, true, 0);
            let first = tv_list_first(copy);
            let second = (*first).li_next;
            let TypvalValue::List(first_copy) = (*first).li_tv.value else { panic!("expected a List") };
            let TypvalValue::List(second_copy) = (*second).li_tv.value else { panic!("expected a List") };
            assert_ne!(first_copy, second_copy); // two genuinely separate copies.

            tv_list_unref(orig);
            tv_list_unref(copy);
        }
    }

    #[test]
    fn tv_list_copy_deep_with_a_real_copy_id_reuses_the_same_copy_for_a_shared_reference() {
        // Same setup as the noref=1 test above, but with a real
        // (non-zero) copyID - the SAME list referenced twice now
        // produces the SAME copy twice too (identity preserved),
        // matching the original's own documented deepcopy({expr})
        // (noref omitted/0) behavior exactly.
        let _lock = crate::globals::global_state_test_lock();
        let inner = tv_list_alloc(0);
        unsafe { tv_list_ref(inner) };
        let orig = tv_list_alloc(2);
        unsafe {
            tv_list_append_owned_tv(orig, TypvalT { value: TypvalValue::List(inner), ..Default::default() });
            tv_list_ref(inner);
            tv_list_append_owned_tv(orig, TypvalT { value: TypvalValue::List(inner), ..Default::default() });
        }

        unsafe {
            let copy = tv_list_copy(std::ptr::null(), orig, true, 7);
            let first = tv_list_first(copy);
            let second = (*first).li_next;
            let TypvalValue::List(first_copy) = (*first).li_tv.value else { panic!("expected a List") };
            let TypvalValue::List(second_copy) = (*second).li_tv.value else { panic!("expected a List") };
            assert_eq!(first_copy, second_copy); // same copy reused both times.
            assert_eq!((*first_copy).lv_refcount, 2); // referenced twice.

            tv_list_unref(orig);
            tv_list_unref(copy);
        }
    }

    #[test]
    fn tv_list_copy_honors_copy_id_bookkeeping() {
        let _lock = crate::globals::global_state_test_lock();
        let orig = tv_list_alloc(0);
        unsafe {
            let copy = tv_list_copy(std::ptr::null(), orig, false, 42);
            assert_eq!((*orig).lv_copy_id, 42);
            assert_eq!((*orig).lv_copylist, copy);
            tv_list_free(orig);
            tv_list_free(copy);
        }
    }

    #[test]
    fn tv_list_copyid_and_tv_list_latest_copy_read_the_bookkeeping_fields() {
        let _lock = crate::globals::global_state_test_lock();
        let orig = tv_list_alloc(0);
        unsafe {
            let copy = tv_list_copy(std::ptr::null(), orig, false, 42);
            assert_eq!(tv_list_copyid(orig), 42);
            assert_eq!(tv_list_latest_copy(orig), copy);
            tv_list_free(orig);
            tv_list_free(copy);
        }
    }

    #[test]
    fn tv_dict_copy_null_orig_is_null() {
        assert!(unsafe { tv_dict_copy(std::ptr::null(), std::ptr::null_mut(), false, 0) }.is_null());
    }

    #[test]
    fn tv_dict_copy_shallow_copies_items() {
        let _lock = crate::globals::global_state_test_lock();
        let orig = tv_dict_alloc();
        unsafe {
            let item_a = tv_dict_item_alloc(b"a");
            (*item_a).di_tv.value = TypvalValue::Number(1);
            tv_dict_add(&mut *orig, item_a);
            let item_b = tv_dict_item_alloc(b"b");
            (*item_b).di_tv.value = TypvalValue::Number(2);
            tv_dict_add(&mut *orig, item_b);

            let copy = tv_dict_copy(std::ptr::null(), orig, false, 0);
            assert!(!copy.is_null());
            assert_eq!(tv_dict_len(copy.as_ref()), 2);
            assert_eq!((*copy).dv_refcount, 1);
            // The copy is a genuinely separate dict, not an alias.
            assert_ne!(copy, orig);

            let mut values: Vec<crate::eval::typval_defs::VarnumberT> = (*copy)
                .dv_index
                .values()
                .map(|&di| match (*di).di_tv.value {
                    TypvalValue::Number(n) => n,
                    _ => panic!("expected a Number"),
                })
                .collect();
            values.sort_unstable();
            assert_eq!(values, vec![1, 2]);

            tv_dict_unref(orig);
            tv_dict_unref(copy);
        }
    }

    #[test]
    fn tv_dict_copy_of_empty_dict_is_empty_not_null() {
        let _lock = crate::globals::global_state_test_lock();
        let orig = tv_dict_alloc();
        unsafe {
            let copy = tv_dict_copy(std::ptr::null(), orig, false, 0);
            assert!(!copy.is_null());
            assert_eq!(tv_dict_len(copy.as_ref()), 0);
            tv_dict_unref(orig);
            tv_dict_unref(copy);
        }
    }

    #[test]
    fn tv_dict_copy_deep_of_empty_dict_does_not_panic() {
        let _lock = crate::globals::global_state_test_lock();
        let orig = tv_dict_alloc();
        unsafe {
            let copy = tv_dict_copy(std::ptr::null(), orig, true, 0);
            assert!(!copy.is_null());
            assert_eq!(tv_dict_len(copy.as_ref()), 0);
            tv_dict_unref(orig);
            tv_dict_unref(copy);
        }
    }

    #[test]
    fn tv_dict_copy_deep_copies_a_nested_dict_recursively() {
        let _lock = crate::globals::global_state_test_lock();
        let inner = tv_dict_alloc();
        unsafe { (*inner).dv_refcount += 1 };
        unsafe {
            let inner_item = tv_dict_item_alloc(b"x");
            (*inner_item).di_tv.value = TypvalValue::Number(1);
            tv_dict_add(&mut *inner, inner_item);
        }
        let orig = tv_dict_alloc();
        unsafe {
            let item = tv_dict_item_alloc(b"a");
            (*item).di_tv.value = TypvalValue::Dict(inner);
            tv_dict_add(&mut *orig, item);
        }

        unsafe {
            let copy = tv_dict_copy(std::ptr::null(), orig, true, 0);
            assert!(!copy.is_null());
            let outer_item = tv_dict_find(Some(&mut *copy), b"a").unwrap();
            let TypvalValue::Dict(inner_copy) = (*outer_item).di_tv.value else { panic!("expected a Dict") };
            // The nested dict was ALSO copied - a genuinely separate
            // dict, not the same pointer as the original's own inner
            // dict.
            assert_ne!(inner_copy, inner);
            assert_eq!((*inner_copy).dv_refcount, 1);

            // Mutating the nested copy must not affect the nested
            // original.
            let inner_copy_item = tv_dict_find(Some(&mut *inner_copy), b"x").unwrap();
            (*inner_copy_item).di_tv.value = TypvalValue::Number(99);
            let inner_orig_item = tv_dict_find(Some(&mut *inner), b"x").unwrap();
            assert_eq!((*inner_orig_item).di_tv.value, TypvalValue::Number(1));

            tv_dict_unref(orig);
            tv_dict_unref(copy);
        }
    }

    #[test]
    fn tv_dict_copy_deep_with_a_real_copy_id_reuses_the_same_copy_for_a_shared_reference() {
        // Same dict referenced from two different keys - a real
        // (non-zero) copyID produces the SAME copy both times
        // (identity preserved), matching deepcopy({expr})'s own
        // documented (noref omitted/0) behavior exactly.
        let _lock = crate::globals::global_state_test_lock();
        let inner = tv_dict_alloc();
        unsafe { (*inner).dv_refcount += 1 };
        let orig = tv_dict_alloc();
        unsafe {
            let item_a = tv_dict_item_alloc(b"a");
            (*item_a).di_tv.value = TypvalValue::Dict(inner);
            tv_dict_add(&mut *orig, item_a);
            (*inner).dv_refcount += 1;
            let item_b = tv_dict_item_alloc(b"b");
            (*item_b).di_tv.value = TypvalValue::Dict(inner);
            tv_dict_add(&mut *orig, item_b);
        }

        unsafe {
            let copy = tv_dict_copy(std::ptr::null(), orig, true, 7);
            let a_item = tv_dict_find(Some(&mut *copy), b"a").unwrap();
            let b_item = tv_dict_find(Some(&mut *copy), b"b").unwrap();
            let TypvalValue::Dict(a_copy) = (*a_item).di_tv.value else { panic!("expected a Dict") };
            let TypvalValue::Dict(b_copy) = (*b_item).di_tv.value else { panic!("expected a Dict") };
            assert_eq!(a_copy, b_copy); // same copy reused both times.
            assert_eq!((*a_copy).dv_refcount, 2); // referenced twice.

            tv_dict_unref(orig);
            tv_dict_unref(copy);
        }
    }

    #[test]
    fn tv_dict_copy_honors_copy_id_bookkeeping() {
        let _lock = crate::globals::global_state_test_lock();
        let orig = tv_dict_alloc();
        unsafe {
            let copy = tv_dict_copy(std::ptr::null(), orig, false, 42);
            assert_eq!((*orig).dv_copy_id, 42);
            assert_eq!((*orig).dv_copydict, copy);
            tv_dict_unref(orig);
            tv_dict_unref(copy);
        }
    }

    #[test]
    fn tv_blob_copy_null_from_is_a_null_blob() {
        let mut to = TypvalT::default();
        unsafe { tv_blob_copy(std::ptr::null(), &mut to) };
        assert_eq!(to.value, TypvalValue::Blob(std::ptr::null_mut()));
    }

    #[test]
    fn tv_blob_copy_copies_bytes_into_a_separate_buffer() {
        let from = tv_blob_alloc();
        unsafe {
            (*from).bv_ga.ga_data = vec![1, 2, 3];
            (*from).bv_ga.ga_len = 3;
        }
        let mut to = TypvalT::default();
        unsafe { tv_blob_copy(from, &mut to) };
        let TypvalValue::Blob(b) = to.value else { panic!("expected a Blob") };
        assert_ne!(b, from);
        unsafe {
            assert_eq!((*b).bv_ga.ga_data, vec![1, 2, 3]);
            assert_eq!((*b).bv_ga.ga_len, 3);
            assert_eq!((*b).bv_refcount, 1);
            // Mutating the copy must not affect the original.
            (&mut (*b).bv_ga.ga_data)[0] = 99;
            assert_eq!((&(*from).bv_ga.ga_data)[0], 1);
            tv_blob_free(from);
            tv_blob_free(b);
        }
    }

    fn blob_of(bytes: &[u8]) -> *mut crate::eval::typval_defs::BlobT {
        let b = tv_blob_alloc();
        unsafe {
            (*b).bv_ga.ga_data = bytes.to_vec();
            (*b).bv_ga.ga_len = bytes.len() as i32;
        }
        b
    }

    #[test]
    fn tv_blob_slice_or_index_plain_index_reads_a_byte() {
        let b = blob_of(&[10, 20, 30, 40, 50]);
        let mut rettv = TypvalT { value: TypvalValue::Blob(b), ..Default::default() };
        // The success path calls tv_clear_simple(rettv) internally
        // (releasing b's own reference, exactly as the original's
        // real tv_clear(rettv) does before overwriting it with the
        // Number result) - b must NOT be freed again afterward.
        let ret = unsafe { tv_blob_slice_or_index(b, false, 2, 0, false, &mut rettv) };
        assert_eq!(ret, OK);
        assert_eq!(rettv.value, TypvalValue::Number(30));
    }

    #[test]
    fn tv_blob_slice_or_index_plain_index_negative_counts_from_the_end() {
        let b = blob_of(&[10, 20, 30, 40, 50]);
        let mut rettv = TypvalT { value: TypvalValue::Blob(b), ..Default::default() };
        // See the previous test's own comment: b is released internally
        // on this success path too.
        let ret = unsafe { tv_blob_slice_or_index(b, false, -1, 0, false, &mut rettv) };
        assert_eq!(ret, OK);
        assert_eq!(rettv.value, TypvalValue::Number(50));
    }

    #[test]
    fn tv_blob_slice_or_index_plain_index_out_of_range_fails() {
        let b = blob_of(&[10, 20, 30]);
        let mut rettv = TypvalT { value: TypvalValue::Blob(b), ..Default::default() };
        let ret = unsafe { tv_blob_slice_or_index(b, false, 10, 0, false, &mut rettv) };
        assert_eq!(ret, FAIL);
        // rettv is left untouched (b's own reference NOT released) on
        // the plain-index FAIL path - this is the ONE case among
        // these tests where b genuinely still needs freeing.
        assert_eq!(rettv.value, TypvalValue::Blob(b));
        unsafe { tv_blob_free(b) };
    }

    #[test]
    fn tv_blob_slice_or_index_inclusive_range_produces_a_sub_blob() {
        let b = blob_of(&[10, 20, 30, 40, 50]);
        let mut rettv = TypvalT { value: TypvalValue::Blob(b), ..Default::default() };
        // blob[1:3] (inclusive) -> bytes at indices 1, 2, 3.
        let ret = unsafe { tv_blob_slice_or_index(b, true, 1, 3, false, &mut rettv) };
        assert_eq!(ret, OK);
        let TypvalValue::Blob(result) = rettv.value else { panic!("expected a Blob") };
        assert_ne!(result, b);
        // b's own reference was released internally (see the plain-
        // index tests' own comment above) - only the NEW result blob
        // needs freeing here.
        unsafe {
            assert_eq!((*result).bv_ga.ga_data, vec![20, 30, 40]);
            tv_blob_free(result);
        }
    }

    #[test]
    fn tv_blob_slice_or_index_exclusive_range_drops_the_last_index() {
        let b = blob_of(&[10, 20, 30, 40, 50]);
        let mut rettv = TypvalT { value: TypvalValue::Blob(b), ..Default::default() };
        // slice()-style exclusive [1:3) -> only indices 1, 2.
        let ret = unsafe { tv_blob_slice_or_index(b, true, 1, 3, true, &mut rettv) };
        assert_eq!(ret, OK);
        let TypvalValue::Blob(result) = rettv.value else { panic!("expected a Blob") };
        unsafe {
            assert_eq!((*result).bv_ga.ga_data, vec![20, 30]);
            tv_blob_free(result);
        }
    }

    #[test]
    fn tv_blob_slice_or_index_range_with_negative_start_clamps_to_zero() {
        let b = blob_of(&[10, 20, 30]);
        let mut rettv = TypvalT { value: TypvalValue::Blob(b), ..Default::default() };
        // blob[-99:1] clamps the start to 0.
        let ret = unsafe { tv_blob_slice_or_index(b, true, -99, 1, false, &mut rettv) };
        assert_eq!(ret, OK);
        let TypvalValue::Blob(result) = rettv.value else { panic!("expected a Blob") };
        unsafe {
            assert_eq!((*result).bv_ga.ga_data, vec![10, 20]);
            tv_blob_free(result);
        }
    }

    #[test]
    fn tv_blob_slice_or_index_range_negative_end_counts_from_the_end() {
        let b = blob_of(&[10, 20, 30, 40, 50]);
        let mut rettv = TypvalT { value: TypvalValue::Blob(b), ..Default::default() };
        // blob[1:-1] -> from index 1 to the last byte, inclusive.
        let ret = unsafe { tv_blob_slice_or_index(b, true, 1, -1, false, &mut rettv) };
        assert_eq!(ret, OK);
        let TypvalValue::Blob(result) = rettv.value else { panic!("expected a Blob") };
        unsafe {
            assert_eq!((*result).bv_ga.ga_data, vec![20, 30, 40, 50]);
            tv_blob_free(result);
        }
    }

    #[test]
    fn tv_blob_slice_or_index_range_end_beyond_len_clamps_to_the_last_byte() {
        let b = blob_of(&[10, 20, 30]);
        let mut rettv = TypvalT { value: TypvalValue::Blob(b), ..Default::default() };
        let ret = unsafe { tv_blob_slice_or_index(b, true, 0, 100, false, &mut rettv) };
        assert_eq!(ret, OK);
        let TypvalValue::Blob(result) = rettv.value else { panic!("expected a Blob") };
        unsafe {
            assert_eq!((*result).bv_ga.ga_data, vec![10, 20, 30]);
            tv_blob_free(result);
        }
    }

    #[test]
    fn tv_blob_slice_or_index_range_out_of_bounds_gives_a_null_blob() {
        let b = blob_of(&[10, 20, 30]);
        let mut rettv = TypvalT { value: TypvalValue::Blob(b), ..Default::default() };
        // Start index at/past the length, in RANGE mode, is not an
        // error (unlike plain indexing) - result is a null blob. This
        // branch ALSO releases b's own reference internally (the same
        // tv_clear_simple(rettv) call runs on every success path,
        // including this "empty result" one) - nothing left to free.
        let ret = unsafe { tv_blob_slice_or_index(b, true, 10, 20, false, &mut rettv) };
        assert_eq!(ret, OK);
        assert_eq!(rettv.value, TypvalValue::Blob(std::ptr::null_mut()));
    }

    #[test]
    fn tv_list_extend_appends_l2_items_to_l1() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = tv_list_alloc(2);
        let l2 = tv_list_alloc(2);
        unsafe {
            tv_list_append_tv(l1, &number_tv(1));
            tv_list_append_tv(l1, &number_tv(2));
            tv_list_append_tv(l2, &number_tv(3));
            tv_list_append_tv(l2, &number_tv(4));

            tv_list_extend(l1, l2, std::ptr::null_mut());

            assert_eq!((*l1).lv_len, 4);
            let mut item = (*l1).lv_first;
            for expected in [1, 2, 3, 4] {
                assert!(matches!((*item).li_tv.value, TypvalValue::Number(n) if n == expected));
                item = (*item).li_next;
            }
            tv_list_free(l1);
            tv_list_free(l2);
        }
    }

    #[test]
    fn tv_list_extend_with_null_l2_is_noop() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = tv_list_alloc(1);
        unsafe {
            tv_list_append_tv(l1, &number_tv(1));
            tv_list_extend(l1, std::ptr::null_mut(), std::ptr::null_mut());
            assert_eq!((*l1).lv_len, 1);
            tv_list_free(l1);
        }
    }

    #[test]
    fn tv_list_extend_before_a_specific_item() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = tv_list_alloc(2);
        let l2 = tv_list_alloc(1);
        unsafe {
            tv_list_append_tv(l1, &number_tv(1));
            tv_list_append_tv(l1, &number_tv(3));
            tv_list_append_tv(l2, &number_tv(2));
            let item3 = (*l1).lv_last;

            tv_list_extend(l1, l2, item3);

            assert_eq!((*l1).lv_len, 3);
            let item1 = (*l1).lv_first;
            let item2 = (*item1).li_next;
            assert!(matches!((*item2).li_tv.value, TypvalValue::Number(2)));
            assert_eq!((*item2).li_next, item3);
            tv_list_free(l1);
            tv_list_free(l2);
        }
    }

    #[test]
    fn tv_list_concat_both_null_gives_null_list_but_ok() {
        let mut tv = TypvalT::default();
        let ok = unsafe { tv_list_concat(std::ptr::null_mut(), std::ptr::null_mut(), &mut tv) };
        assert!(ok);
        assert!(matches!(tv.value, TypvalValue::List(p) if p.is_null()));
        assert_eq!(tv.v_lock, VarLockStatus::Unlocked);
    }

    #[test]
    fn tv_list_concat_l1_null_copies_l2() {
        let _lock = crate::globals::global_state_test_lock();
        let l2 = tv_list_alloc(1);
        unsafe {
            tv_list_append_tv(l2, &number_tv(5));
            let mut tv = TypvalT::default();
            let ok = tv_list_concat(std::ptr::null_mut(), l2, &mut tv);
            assert!(ok);
            let TypvalValue::List(result) = tv.value else {
                panic!("expected a List-typed result");
            };
            assert_eq!((*result).lv_len, 1);
            assert_ne!(result, l2); // a genuine copy, not an alias
            tv_list_free(l2);
            tv_list_free(result);
        }
    }

    #[test]
    fn tv_list_concat_both_non_null_appends_l2_after_a_copy_of_l1() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = tv_list_alloc(1);
        let l2 = tv_list_alloc(1);
        unsafe {
            tv_list_append_tv(l1, &number_tv(1));
            tv_list_append_tv(l2, &number_tv(2));
            let mut tv = TypvalT::default();
            let ok = tv_list_concat(l1, l2, &mut tv);
            assert!(ok);
            let TypvalValue::List(result) = tv.value else {
                panic!("expected a List-typed result");
            };
            assert_eq!((*result).lv_len, 2);
            assert_ne!(result, l1); // l1 itself is untouched, a fresh copy is made
            assert!(matches!((*(*result).lv_first).li_tv.value, TypvalValue::Number(1)));
            assert!(matches!((*(*result).lv_last).li_tv.value, TypvalValue::Number(2)));
            // l1 itself still has only its own original item.
            assert_eq!((*l1).lv_len, 1);
            tv_list_free(l1);
            tv_list_free(l2);
            tv_list_free(result);
        }
    }

    fn int_list_of(nums: &[crate::eval::typval_defs::VarnumberT]) -> *mut crate::eval::typval_defs::ListT {
        let l = tv_list_alloc(nums.len() as isize);
        unsafe {
            for n in nums {
                tv_list_append_tv(l, &number_tv(*n));
            }
        }
        l
    }

    fn collect_numbers(l: *mut crate::eval::typval_defs::ListT) -> Vec<crate::eval::typval_defs::VarnumberT> {
        let mut out = Vec::new();
        let mut li = unsafe { tv_list_first(l) };
        while !li.is_null() {
            match unsafe { &(*li).li_tv.value } {
                TypvalValue::Number(n) => out.push(*n),
                other => panic!("expected Number, found {other:?}"),
            }
            li = unsafe { (*li).li_next };
        }
        out
    }

    #[test]
    fn tv_list_slice_or_index_plain_index_reads_an_item() {
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[10, 20, 30, 40, 50]);
        let mut rettv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        // The success path calls tv_clear_simple(rettv) internally
        // (releasing l's own reference, matching the original's real
        // tv_clear(rettv) before overwriting it with the Number
        // result) - l must NOT be freed again afterward.
        let ret = unsafe { tv_list_slice_or_index(l, false, 2, 0, false, &mut rettv, false) };
        assert_eq!(ret, OK);
        assert_eq!(rettv.value, TypvalValue::Number(30));
    }

    #[test]
    fn tv_list_slice_or_index_plain_index_negative_counts_from_the_end() {
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[10, 20, 30, 40, 50]);
        let mut rettv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        // See the previous test's own comment: l is released
        // internally on this success path too.
        let ret = unsafe { tv_list_slice_or_index(l, false, -1, 0, false, &mut rettv, false) };
        assert_eq!(ret, OK);
        assert_eq!(rettv.value, TypvalValue::Number(50));
    }

    #[test]
    fn tv_list_slice_or_index_plain_index_out_of_range_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[10, 20, 30]);
        let mut rettv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        let ret = unsafe { tv_list_slice_or_index(l, false, 10, 0, false, &mut rettv, false) };
        assert_eq!(ret, FAIL);
        // rettv is left untouched (l's own reference NOT released) on
        // the plain-index FAIL path - this is the ONE case among
        // these tests where l genuinely still needs freeing.
        assert_eq!(rettv.value, TypvalValue::List(l));
        unsafe { tv_list_free(l) };
    }

    #[test]
    fn tv_list_slice_or_index_inclusive_range_produces_a_new_list() {
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[10, 20, 30, 40, 50]);
        let mut rettv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        // list[1:3] (inclusive) -> items at indices 1, 2, 3.
        let ret = unsafe { tv_list_slice_or_index(l, true, 1, 3, false, &mut rettv, false) };
        assert_eq!(ret, OK);
        let TypvalValue::List(result) = rettv.value else { panic!("expected a List") };
        assert_ne!(result, l);
        assert_eq!(collect_numbers(result), vec![20, 30, 40]);
        // l's own reference was released internally (see the plain-
        // index tests' own comment above) - only the NEW result list
        // needs freeing here.
        unsafe { tv_list_free(result) };
    }

    #[test]
    fn tv_list_slice_or_index_exclusive_range_drops_the_last_index() {
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[10, 20, 30, 40, 50]);
        let mut rettv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        // slice()-style exclusive [1:3) -> only indices 1, 2.
        let ret = unsafe { tv_list_slice_or_index(l, true, 1, 3, true, &mut rettv, false) };
        assert_eq!(ret, OK);
        let TypvalValue::List(result) = rettv.value else { panic!("expected a List") };
        assert_eq!(collect_numbers(result), vec![20, 30]);
        unsafe { tv_list_free(result) };
    }

    #[test]
    fn tv_list_slice_or_index_range_negative_end_counts_from_the_end() {
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[10, 20, 30, 40, 50]);
        let mut rettv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        // list[1:-1] -> from index 1 to the last item, inclusive.
        let ret = unsafe { tv_list_slice_or_index(l, true, 1, -1, false, &mut rettv, false) };
        assert_eq!(ret, OK);
        let TypvalValue::List(result) = rettv.value else { panic!("expected a List") };
        assert_eq!(collect_numbers(result), vec![20, 30, 40, 50]);
        unsafe { tv_list_free(result) };
    }

    #[test]
    fn tv_list_slice_or_index_range_start_out_of_range_gives_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[10, 20, 30]);
        let mut rettv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        // Start index at/past the length, in RANGE mode, is not an
        // error (unlike plain indexing) - result is an empty list.
        // This branch ALSO releases l's own reference internally (the
        // same tv_clear_simple(rettv) call runs on every success
        // path) - only the new (empty) result list needs freeing.
        let ret = unsafe { tv_list_slice_or_index(l, true, 10, 20, false, &mut rettv, false) };
        assert_eq!(ret, OK);
        let TypvalValue::List(result) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { tv_list_len(result) }, 0);
        unsafe { tv_list_free(result) };
    }

    #[test]
    fn tv_list_item_remove_unlinks_middle_item_and_returns_next() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(3);
        unsafe {
            for n in [1, 2, 3] {
                tv_list_append_tv(l, &number_tv(n));
            }
            let item1 = (*l).lv_first;
            let item2 = (*item1).li_next;
            let item3 = (*item2).li_next;

            let returned = tv_list_item_remove(l, item2);
            assert_eq!(returned, item3);
            assert_eq!((*l).lv_len, 2);
            assert_eq!((*item1).li_next, item3);
            assert_eq!((*item3).li_prev, item1);
            assert_eq!((*l).lv_first, item1);
            assert_eq!((*l).lv_last, item3);

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_remove_items_removes_and_frees_a_range() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(4);
        unsafe {
            for n in [1, 2, 3, 4] {
                tv_list_append_tv(l, &number_tv(n));
            }
            let item1 = (*l).lv_first;
            let item2 = (*item1).li_next;
            let item3 = (*item2).li_next;
            let item4 = (*item3).li_next;

            // Remove the middle range (items 2 and 3).
            tv_list_remove_items(l, item2, item3);
            assert_eq!((*l).lv_len, 2);
            assert_eq!((*item1).li_next, item4);
            assert_eq!((*item4).li_prev, item1);
            assert_eq!((*l).lv_first, item1);
            assert_eq!((*l).lv_last, item4);

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_watch_fix_advances_past_a_removed_item() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(2);
        unsafe {
            tv_list_append_tv(l, &number_tv(1));
            tv_list_append_tv(l, &number_tv(2));
            let item1 = (*l).lv_first;
            let item2 = (*item1).li_next;

            let mut watch =
                crate::eval::typval_defs::ListwatchT { lw_item: item1, lw_next: std::ptr::null_mut() };
            tv_list_watch_add(l, &mut watch as *mut _);
            assert_eq!((*l).lv_watch, &mut watch as *mut _);

            // Removing item1 (which the watcher points at) should
            // advance the watcher to item2.
            tv_list_item_remove(l, item1);
            assert_eq!(watch.lw_item, item2);

            // Must remove the watcher before freeing the list -
            // tv_list_free_contents debug_asserts lv_watch is empty,
            // matching the original's own assert().
            tv_list_watch_remove(l, &mut watch as *mut _);
            assert!((*l).lv_watch.is_null());

            tv_list_free(l);
        }
    }

    fn string_tv(s: &[u8]) -> TypvalT {
        TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(Some(s.to_vec())) }
    }

    fn unknown_tv() -> TypvalT {
        TypvalT::default()
    }

    // ---- tv_check_str_or_nr / tv_check_num / tv_check_str --------------

    #[test]
    fn tv_check_str_or_nr_accepts_number_and_string_only() {
        assert!(tv_check_str_or_nr(&number_tv(1)));
        assert!(tv_check_str_or_nr(&string_tv(b"x")));
        assert!(!tv_check_str_or_nr(&TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Float(1.0)
        }));
        assert!(!tv_check_str_or_nr(&unknown_tv()));
    }

    #[test]
    fn tv_check_num_accepts_number_bool_special_string() {
        assert!(tv_check_num(&number_tv(1)));
        assert!(tv_check_num(&string_tv(b"x")));
        assert!(tv_check_num(&TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True)
        }));
        assert!(tv_check_num(&TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null)
        }));
        assert!(!tv_check_num(&TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Float(1.0)
        }));
    }

    #[test]
    fn tv_check_str_accepts_number_bool_special_string_float() {
        assert!(tv_check_str(&number_tv(1)));
        assert!(tv_check_str(&string_tv(b"x")));
        assert!(tv_check_str(&TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Float(1.0)
        }));
        assert!(!tv_check_str(&TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::List(std::ptr::null_mut())
        }));
    }

    // ---- tv_list_locked / tv_islocked / value_check_lock / tv_check_lock

    #[test]
    fn tv_list_locked_null_is_fixed() {
        assert_eq!(unsafe { tv_list_locked(std::ptr::null()) }, VarLockStatus::Fixed);
    }

    #[test]
    fn tv_list_locked_reads_lv_lock() {
        let mut l = test_list();
        l.lv_lock = VarLockStatus::Locked;
        assert_eq!(unsafe { tv_list_locked(&l as *const _) }, VarLockStatus::Locked);
    }

    #[test]
    fn tv_islocked_true_when_v_lock_locked() {
        let tv = TypvalT { v_lock: VarLockStatus::Locked, value: TypvalValue::Number(0) };
        assert!(unsafe { tv_islocked(&tv) });
    }

    #[test]
    fn tv_islocked_true_when_inner_list_locked() {
        let mut l = test_list();
        l.lv_lock = VarLockStatus::Locked;
        let tv =
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(&mut l as *mut _) };
        assert!(unsafe { tv_islocked(&tv) });
    }

    #[test]
    fn tv_islocked_false_when_nothing_locked() {
        assert!(!unsafe { tv_islocked(&number_tv(1)) });
        let tv =
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) };
        assert!(!unsafe { tv_islocked(&tv) });
    }

    #[test]
    fn value_check_lock_true_for_locked_and_fixed_false_for_unlocked() {
        assert!(!value_check_lock(VarLockStatus::Unlocked, None));
        assert!(value_check_lock(VarLockStatus::Locked, None));
        assert!(value_check_lock(VarLockStatus::Fixed, Some(b"x")));
    }

    #[test]
    fn tv_check_lock_true_when_tv_itself_locked() {
        let tv = TypvalT { v_lock: VarLockStatus::Locked, value: TypvalValue::Number(0) };
        assert!(unsafe { tv_check_lock(&tv, None) });
    }

    #[test]
    fn tv_check_lock_true_when_inner_dict_locked() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe { (*d).dv_lock = VarLockStatus::Locked };
        let tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(d) };
        assert!(unsafe { tv_check_lock(&tv, None) });
        unsafe { tv_dict_unref(d) };
    }

    #[test]
    fn tv_check_lock_false_when_nothing_locked() {
        assert!(!unsafe { tv_check_lock(&number_tv(1), None) });
    }

    // ---- tv_check_for_*_arg family --------------------------------------

    #[test]
    fn tv_check_for_string_arg_accepts_only_string() {
        let args = [string_tv(b"x"), number_tv(1)];
        assert_eq!(tv_check_for_string_arg(&args, 0), OK);
        assert_eq!(tv_check_for_string_arg(&args, 1), FAIL);
    }

    #[test]
    fn tv_check_for_nonempty_string_arg_rejects_empty_and_none() {
        let args = [string_tv(b"x"), string_tv(b""), number_tv(1)];
        assert_eq!(tv_check_for_nonempty_string_arg(&args, 0), OK);
        assert_eq!(tv_check_for_nonempty_string_arg(&args, 1), FAIL);
        assert_eq!(tv_check_for_nonempty_string_arg(&args, 2), FAIL);
    }

    #[test]
    fn tv_check_for_opt_string_arg_allows_unknown() {
        let args = [string_tv(b"x"), unknown_tv(), number_tv(1)];
        assert_eq!(tv_check_for_opt_string_arg(&args, 0), OK);
        assert_eq!(tv_check_for_opt_string_arg(&args, 1), OK);
        assert_eq!(tv_check_for_opt_string_arg(&args, 2), FAIL);
    }

    #[test]
    fn tv_check_for_number_arg_accepts_only_number() {
        let args = [number_tv(1), string_tv(b"x")];
        assert_eq!(tv_check_for_number_arg(&args, 0), OK);
        assert_eq!(tv_check_for_number_arg(&args, 1), FAIL);
    }

    #[test]
    fn tv_check_for_opt_number_arg_allows_unknown() {
        let args = [number_tv(1), unknown_tv(), string_tv(b"x")];
        assert_eq!(tv_check_for_opt_number_arg(&args, 0), OK);
        assert_eq!(tv_check_for_opt_number_arg(&args, 1), OK);
        assert_eq!(tv_check_for_opt_number_arg(&args, 2), FAIL);
    }

    #[test]
    fn tv_check_for_float_or_nr_arg_accepts_float_and_number() {
        let args = [
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Float(1.0) },
            number_tv(1),
            string_tv(b"x"),
        ];
        assert_eq!(tv_check_for_float_or_nr_arg(&args, 0), OK);
        assert_eq!(tv_check_for_float_or_nr_arg(&args, 1), OK);
        assert_eq!(tv_check_for_float_or_nr_arg(&args, 2), FAIL);
    }

    #[test]
    fn tv_check_for_bool_arg_accepts_bool_and_0_or_1_number() {
        let args = [
            TypvalT {
                v_lock: VarLockStatus::Unlocked,
                value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True),
            },
            number_tv(0),
            number_tv(1),
            number_tv(2),
            string_tv(b"x"),
        ];
        assert_eq!(tv_check_for_bool_arg(&args, 0), OK);
        assert_eq!(tv_check_for_bool_arg(&args, 1), OK);
        assert_eq!(tv_check_for_bool_arg(&args, 2), OK);
        assert_eq!(tv_check_for_bool_arg(&args, 3), FAIL); // 2 is not 0/1
        assert_eq!(tv_check_for_bool_arg(&args, 4), FAIL);
    }

    #[test]
    fn tv_check_for_opt_bool_arg_allows_unknown() {
        let args = [unknown_tv(), number_tv(0), string_tv(b"x")];
        assert_eq!(tv_check_for_opt_bool_arg(&args, 0), OK);
        assert_eq!(tv_check_for_opt_bool_arg(&args, 1), OK);
        assert_eq!(tv_check_for_opt_bool_arg(&args, 2), FAIL);
    }

    #[test]
    fn tv_check_for_blob_arg_accepts_only_blob() {
        let args = [
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(std::ptr::null_mut()) },
            number_tv(1),
        ];
        assert_eq!(tv_check_for_blob_arg(&args, 0), OK);
        assert_eq!(tv_check_for_blob_arg(&args, 1), FAIL);
    }

    #[test]
    fn tv_check_for_list_arg_accepts_only_list() {
        let args = [
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) },
            number_tv(1),
        ];
        assert_eq!(tv_check_for_list_arg(&args, 0), OK);
        assert_eq!(tv_check_for_list_arg(&args, 1), FAIL);
    }

    #[test]
    fn tv_check_for_dict_arg_accepts_only_dict() {
        let args = [
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) },
            number_tv(1),
        ];
        assert_eq!(tv_check_for_dict_arg(&args, 0), OK);
        assert_eq!(tv_check_for_dict_arg(&args, 1), FAIL);
    }

    #[test]
    fn tv_check_for_nonnull_dict_arg_rejects_null_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        let args = [
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(d) },
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) },
            number_tv(1),
        ];
        assert_eq!(tv_check_for_nonnull_dict_arg(&args, 0), OK);
        assert_eq!(tv_check_for_nonnull_dict_arg(&args, 1), FAIL);
        assert_eq!(tv_check_for_nonnull_dict_arg(&args, 2), FAIL);
        unsafe { tv_dict_unref(d) };
    }

    #[test]
    fn tv_check_for_opt_dict_arg_allows_unknown() {
        let args = [
            unknown_tv(),
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) },
            number_tv(1),
        ];
        assert_eq!(tv_check_for_opt_dict_arg(&args, 0), OK);
        assert_eq!(tv_check_for_opt_dict_arg(&args, 1), OK);
        assert_eq!(tv_check_for_opt_dict_arg(&args, 2), FAIL);
    }

    #[test]
    fn tv_check_for_string_or_number_arg_and_aliases() {
        let args = [string_tv(b"x"), number_tv(1), unknown_tv()];
        assert_eq!(tv_check_for_string_or_number_arg(&args, 0), OK);
        assert_eq!(tv_check_for_string_or_number_arg(&args, 1), OK);
        assert_eq!(tv_check_for_string_or_number_arg(&args, 2), FAIL);
        // tv_check_for_buffer_arg/tv_check_for_lnum_arg are literal
        // delegates - verify they behave identically.
        assert_eq!(tv_check_for_buffer_arg(&args, 0), OK);
        assert_eq!(tv_check_for_lnum_arg(&args, 1), OK);
        assert_eq!(tv_check_for_buffer_arg(&args, 2), FAIL);
    }

    #[test]
    fn tv_check_for_string_or_list_arg_family() {
        let args = [
            string_tv(b"x"),
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) },
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(std::ptr::null_mut()) },
            number_tv(1),
            unknown_tv(),
        ];
        assert_eq!(tv_check_for_string_or_list_arg(&args, 0), OK);
        assert_eq!(tv_check_for_string_or_list_arg(&args, 1), OK);
        assert_eq!(tv_check_for_string_or_list_arg(&args, 2), FAIL); // blob not accepted
        assert_eq!(tv_check_for_string_or_list_arg(&args, 3), FAIL);

        assert_eq!(tv_check_for_string_or_list_or_blob_arg(&args, 0), OK);
        assert_eq!(tv_check_for_string_or_list_or_blob_arg(&args, 1), OK);
        assert_eq!(tv_check_for_string_or_list_or_blob_arg(&args, 2), OK); // blob IS accepted here
        assert_eq!(tv_check_for_string_or_list_or_blob_arg(&args, 3), FAIL);

        assert_eq!(tv_check_for_opt_string_or_list_arg(&args, 0), OK);
        assert_eq!(tv_check_for_opt_string_or_list_arg(&args, 4), OK); // unknown allowed
        assert_eq!(tv_check_for_opt_string_or_list_arg(&args, 3), FAIL);
    }

    #[test]
    fn tv_check_for_string_or_func_arg_accepts_partial_func_string() {
        let args = [
            string_tv(b"x"),
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Func(None) },
            TypvalT {
                v_lock: VarLockStatus::Unlocked,
                value: TypvalValue::Partial(std::ptr::null_mut()),
            },
            number_tv(1),
        ];
        assert_eq!(tv_check_for_string_or_func_arg(&args, 0), OK);
        assert_eq!(tv_check_for_string_or_func_arg(&args, 1), OK);
        assert_eq!(tv_check_for_string_or_func_arg(&args, 2), OK);
        assert_eq!(tv_check_for_string_or_func_arg(&args, 3), FAIL);
    }

    #[test]
    fn tv_check_for_list_or_blob_arg_accepts_list_and_blob() {
        let args = [
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) },
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(std::ptr::null_mut()) },
            string_tv(b"x"),
        ];
        assert_eq!(tv_check_for_list_or_blob_arg(&args, 0), OK);
        assert_eq!(tv_check_for_list_or_blob_arg(&args, 1), OK);
        assert_eq!(tv_check_for_list_or_blob_arg(&args, 2), FAIL);
    }

    // ---- tv_is_func / tv_dict_len / tv_blob_get -------------------------

    #[test]
    fn tv_is_func_true_for_func_and_partial_only() {
        assert!(tv_is_func(&TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Func(None) }));
        assert!(tv_is_func(&TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Partial(std::ptr::null_mut())
        }));
        assert!(!tv_is_func(&number_tv(1)));
        assert!(!tv_is_func(&string_tv(b"x")));
    }

    #[test]
    fn tv_dict_len_null_is_zero() {
        assert_eq!(tv_dict_len(None), 0);
    }

    #[test]
    fn tv_dict_len_counts_real_entries() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_len(d.as_ref()), 0);
            tv_dict_add_nr(&mut *d, b"a", 1);
            tv_dict_add_nr(&mut *d, b"b", 2);
            assert_eq!(tv_dict_len(d.as_ref()), 2);
            tv_dict_unref(d);
        }
    }

    #[test]
    fn tv_blob_get_reads_the_right_byte() {
        let mut b = crate::eval::typval_defs::BlobT::default();
        b.bv_ga.ga_data = vec![10, 20, 30];
        b.bv_ga.ga_len = 3;
        unsafe {
            assert_eq!(tv_blob_get(&b as *const _, 0), 10);
            assert_eq!(tv_blob_get(&b as *const _, 2), 30);
        }
    }

    #[test]
    fn tv_blob_set_append_appends_at_the_end() {
        // Cross-verified against real nvim:
        //   let b = 0z0011 | let b[2] = 0x22  ->  0z001122
        let mut b = crate::eval::typval_defs::BlobT::default();
        b.bv_ga.ga_data = vec![0x00, 0x11];
        b.bv_ga.ga_len = 2;

        unsafe {
            tv_blob_set_append(&mut b as *mut _, 2, 0x22);
            // ga_len is the authoritative length, so it must have moved.
            assert_eq!(tv_blob_len(&b as *const _), 3);
            assert_eq!(tv_blob_get(&b as *const _, 2), 0x22);
        }
        assert_eq!(b.bv_ga.ga_data, vec![0x00, 0x11, 0x22]);
    }

    #[test]
    fn tv_blob_set_append_ignores_an_index_beyond_the_end() {
        // Cross-verified: `let b[9] = ...` on a 3-byte blob is
        // rejected and leaves the blob untouched. Growing to reach it
        // would leave uninitialized bytes behind.
        let mut b = crate::eval::typval_defs::BlobT::default();
        b.bv_ga.ga_data = vec![0x00, 0x11, 0x22];
        b.bv_ga.ga_len = 3;

        unsafe { tv_blob_set_append(&mut b as *mut _, 9, 0x33) };

        assert_eq!(unsafe { tv_blob_len(&b as *const _) }, 3);
        assert_eq!(b.bv_ga.ga_data, vec![0x00, 0x11, 0x22]);
    }

    #[test]
    fn tv_blob_set_append_overwrites_an_existing_index() {
        // Cross-verified: `let b[0] = 0xff` on 0z001122 -> 0zFF1122,
        // with no growth.
        let mut b = crate::eval::typval_defs::BlobT::default();
        b.bv_ga.ga_data = vec![0x00, 0x11, 0x22];
        b.bv_ga.ga_len = 3;

        unsafe { tv_blob_set_append(&mut b as *mut _, 0, 0xff) };

        assert_eq!(unsafe { tv_blob_len(&b as *const _) }, 3);
        assert_eq!(b.bv_ga.ga_data, vec![0xff, 0x11, 0x22]);
    }

    #[test]
    fn tv_blob_set_append_grows_an_empty_blob() {
        let mut b = crate::eval::typval_defs::BlobT::default();
        unsafe {
            tv_blob_set_append(&mut b as *mut _, 0, 0x7f);
            assert_eq!(tv_blob_len(&b as *const _), 1);
            assert_eq!(tv_blob_get(&b as *const _, 0), 0x7f);
        }
    }

    #[test]
    fn tv_blob_set_writes_the_right_byte() {
        let mut b = crate::eval::typval_defs::BlobT::default();
        b.bv_ga.ga_data = vec![10, 20, 30];
        b.bv_ga.ga_len = 3;
        unsafe {
            tv_blob_set(&mut b as *mut _, 1, 99);
            assert_eq!(tv_blob_get(&b as *const _, 0), 10);
            assert_eq!(tv_blob_get(&b as *const _, 1), 99);
            assert_eq!(tv_blob_get(&b as *const _, 2), 30);
        }
    }

    // ---- tv_list_equal ---------------------------------------------------

    #[test]
    fn tv_list_equal_null_and_empty_are_equal() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(0);
        unsafe {
            assert!(tv_list_equal(std::ptr::null(), std::ptr::null(), false));
            assert!(tv_list_equal(l, std::ptr::null(), false));
            assert!(tv_list_equal(std::ptr::null(), l, false));
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_equal_same_pointer_is_equal() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe {
            assert!(tv_list_equal(l, l, false));
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_equal_compares_items_in_order() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = tv_list_alloc(2);
        let l2 = tv_list_alloc(2);
        unsafe {
            tv_list_append_number(l1, 1);
            tv_list_append_number(l1, 2);
            tv_list_append_number(l2, 1);
            tv_list_append_number(l2, 2);
            assert!(tv_list_equal(l1, l2, false));

            tv_list_free(l1);
            tv_list_free(l2);
        }
    }

    #[test]
    fn tv_list_equal_false_for_different_length_or_content() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = tv_list_alloc(2);
        let l2 = tv_list_alloc(1);
        let l3 = tv_list_alloc(2);
        unsafe {
            tv_list_append_number(l1, 1);
            tv_list_append_number(l1, 2);
            tv_list_append_number(l2, 1);
            tv_list_append_number(l3, 1);
            tv_list_append_number(l3, 99);

            assert!(!tv_list_equal(l1, l2, false)); // different length
            assert!(!tv_list_equal(l1, l3, false)); // different content

            tv_list_free(l1);
            tv_list_free(l2);
            tv_list_free(l3);
        }
    }

    // ---- tv_dict_equal ---------------------------------------------------

    #[test]
    fn tv_dict_equal_null_and_empty_are_equal() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert!(tv_dict_equal(std::ptr::null_mut(), std::ptr::null_mut(), false));
            assert!(tv_dict_equal(d, std::ptr::null_mut(), false));
            assert!(tv_dict_equal(std::ptr::null_mut(), d, false));
            tv_dict_unref(d);
        }
    }

    #[test]
    fn tv_dict_equal_compares_keys_and_values_regardless_of_order() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = tv_dict_alloc();
        let d2 = tv_dict_alloc();
        unsafe {
            tv_dict_add_nr(&mut *d1, b"a", 1);
            tv_dict_add_nr(&mut *d1, b"b", 2);
            // Insert in the opposite order - dicts are unordered.
            tv_dict_add_nr(&mut *d2, b"b", 2);
            tv_dict_add_nr(&mut *d2, b"a", 1);

            assert!(tv_dict_equal(d1, d2, false));

            tv_dict_unref(d1);
            tv_dict_unref(d2);
        }
    }

    #[test]
    fn tv_dict_equal_false_for_different_keys_or_values() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = tv_dict_alloc();
        let d2 = tv_dict_alloc();
        let d3 = tv_dict_alloc();
        unsafe {
            tv_dict_add_nr(&mut *d1, b"a", 1);
            tv_dict_add_nr(&mut *d2, b"a", 2); // different value
            tv_dict_add_nr(&mut *d3, b"c", 1); // different key

            assert!(!tv_dict_equal(d1, d2, false));
            assert!(!tv_dict_equal(d1, d3, false));

            tv_dict_unref(d1);
            tv_dict_unref(d2);
            tv_dict_unref(d3);
        }
    }

    // ---- tv_blob_equal ---------------------------------------------------

    #[test]
    fn tv_blob_equal_null_and_empty_are_equal() {
        let empty = crate::eval::typval_defs::BlobT::default();
        unsafe {
            assert!(tv_blob_equal(std::ptr::null(), std::ptr::null()));
            assert!(tv_blob_equal(&empty as *const _, std::ptr::null()));
        }
    }

    #[test]
    fn tv_blob_equal_compares_content() {
        let mut b1 = crate::eval::typval_defs::BlobT::default();
        b1.bv_ga.ga_data = vec![1, 2, 3];
        b1.bv_ga.ga_len = 3;
        let mut b2 = crate::eval::typval_defs::BlobT::default();
        b2.bv_ga.ga_data = vec![1, 2, 3];
        b2.bv_ga.ga_len = 3;
        let mut b3 = crate::eval::typval_defs::BlobT::default();
        b3.bv_ga.ga_data = vec![1, 2, 9];
        b3.bv_ga.ga_len = 3;

        unsafe {
            assert!(tv_blob_equal(&b1 as *const _, &b2 as *const _));
            assert!(!tv_blob_equal(&b1 as *const _, &b3 as *const _));
        }
    }

    // ---- tv_equal ---------------------------------------------------------

    #[test]
    fn tv_equal_number_string_and_float_are_mutually_distinct_types() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            assert!(!tv_equal(&number_tv(1), &string_tv(b"1"), false));
            assert!(!tv_equal(
                &number_tv(1),
                &TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Float(1.0) },
                false
            ));
        }
    }

    #[test]
    fn tv_equal_number_compares_value() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            assert!(tv_equal(&number_tv(5), &number_tv(5), false));
            assert!(!tv_equal(&number_tv(5), &number_tv(6), false));
        }
    }

    #[test]
    fn tv_equal_string_respects_ignorecase_flag() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            assert!(!tv_equal(&string_tv(b"FOO"), &string_tv(b"foo"), false));
            assert!(tv_equal(&string_tv(b"FOO"), &string_tv(b"foo"), true));
        }
    }

    #[test]
    fn tv_equal_float_and_bool_and_special_compare_value() {
        let _lock = crate::globals::global_state_test_lock();
        let f1 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Float(1.5) };
        let f2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Float(1.5) };
        let f3 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Float(2.5) };
        let bool_true = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True),
        };
        let special = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
        };
        unsafe {
            assert!(tv_equal(&f1, &f2, false));
            assert!(!tv_equal(&f1, &f3, false));
            assert!(tv_equal(&bool_true, &bool_true.clone(), false));
            assert!(tv_equal(&special, &special.clone(), false));
        }
    }

    #[test]
    fn tv_equal_unknown_never_equals_anything_not_even_self() {
        let _lock = crate::globals::global_state_test_lock();
        let u = unknown_tv();
        assert!(!unsafe { tv_equal(&u, &u, false) });
    }

    #[test]
    fn tv_equal_list_delegates_to_tv_list_equal() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = tv_list_alloc(1);
        let l2 = tv_list_alloc(1);
        unsafe {
            tv_list_append_number(l1, 42);
            tv_list_append_number(l2, 42);
            let tv1 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(l1) };
            let tv2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(l2) };
            assert!(tv_equal(&tv1, &tv2, false));

            tv_list_append_number(l2, 99);
            assert!(!tv_equal(&tv1, &tv2, false));

            tv_list_free(l1);
            tv_list_free(l2);
        }
    }

    #[test]
    fn tv_equal_dict_delegates_to_tv_dict_equal() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = tv_dict_alloc();
        let d2 = tv_dict_alloc();
        unsafe {
            tv_dict_add_nr(&mut *d1, b"a", 1);
            tv_dict_add_nr(&mut *d2, b"a", 1);
            let tv1 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(d1) };
            let tv2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(d2) };
            assert!(tv_equal(&tv1, &tv2, false));

            tv_dict_add_nr(&mut *d2, b"b", 2);
            assert!(!tv_equal(&tv1, &tv2, false));

            tv_dict_unref(d1);
            tv_dict_unref(d2);
        }
    }

    #[test]
    fn tv_equal_blob_delegates_to_tv_blob_equal() {
        let _lock = crate::globals::global_state_test_lock();
        let mut b1 = crate::eval::typval_defs::BlobT::default();
        b1.bv_ga.ga_data = vec![1, 2];
        b1.bv_ga.ga_len = 2;
        let mut b2 = crate::eval::typval_defs::BlobT::default();
        b2.bv_ga.ga_data = vec![1, 2];
        b2.bv_ga.ga_len = 2;
        let tv1 =
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(&mut b1 as *mut _) };
        let tv2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(&mut b2 as *mut _) };
        assert!(unsafe { tv_equal(&tv1, &tv2, false) });
    }

    #[test]
    fn tv_equal_func_and_partial_can_cross_compare() {
        let _lock = crate::globals::global_state_test_lock();
        // A VAR_FUNC and a VAR_PARTIAL (with no dict/args, matching a
        // plain function reference) ARE allowed to compare equal when
        // their names match - matches the original's own
        // `tv_is_func(*tv1) && tv_is_func(*tv2)` bypass of the
        // strict-type-match check.
        let func_tv =
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Func(Some(b"Foo".to_vec())) };
        let mut partial = crate::eval::typval_defs::PartialT {
            pt_name: Some(b"Foo".to_vec()),
            ..Default::default()
        };
        let partial_tv = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Partial(&mut partial as *mut _),
        };
        assert!(unsafe { tv_equal(&func_tv, &partial_tv, false) });
    }

    #[test]
    fn tv_equal_null_partial_never_equals_anything() {
        let _lock = crate::globals::global_state_test_lock();
        let p1 = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Partial(std::ptr::null_mut()),
        };
        let p2 = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Partial(std::ptr::null_mut()),
        };
        assert!(!unsafe { tv_equal(&p1, &p2, false) });
    }

    #[test]
    fn tv_equal_recursion_limit_treats_very_deep_nesting_as_equal() {
        // Not practical to build 1000+ levels of real nested lists for
        // a unit test - directly exercise the recursion-limit guard
        // instead, proving it fires and self-resets afterward.
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            *TV_EQUAL_RECURSIVE_CNT.get_mut() = 5;
            *TV_EQUAL_RECURSE_LIMIT.get_mut() = 5;
        }
        // recursive_cnt(5) >= limit(5): guessed equal, limit decremented.
        assert!(unsafe { tv_equal(&number_tv(1), &number_tv(2), false) });
        assert_eq!(unsafe { *TV_EQUAL_RECURSE_LIMIT.get_mut() }, 4);

        // Reset shared state for other tests.
        unsafe {
            *TV_EQUAL_RECURSIVE_CNT.get_mut() = 0;
            *TV_EQUAL_RECURSE_LIMIT.get_mut() = 1000;
        }
    }

    // ---- tv_dict_get_tv / tv_dict_get_number(_def) / tv_dict_get_bool --

    #[test]
    fn tv_dict_get_tv_copies_the_found_value() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            tv_dict_add_nr(&mut *d, b"a", 42);
            let mut rettv = TypvalT::default();
            assert_eq!(tv_dict_get_tv(d.as_mut(), b"a", &mut rettv), OK);
            assert_eq!(rettv.value, TypvalValue::Number(42));
            tv_dict_unref(d);
        }
    }

    #[test]
    fn tv_dict_get_tv_fails_for_missing_key() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let mut rettv = TypvalT::default();
            assert_eq!(tv_dict_get_tv(d.as_mut(), b"missing", &mut rettv), FAIL);
            tv_dict_unref(d);
        }
    }

    #[test]
    fn tv_dict_get_number_and_def_use_the_default_for_a_missing_key() {
        assert_eq!(unsafe { tv_dict_get_number(None, b"x") }, 0);
        assert_eq!(unsafe { tv_dict_get_number_def(None, b"x", 99) }, 99);
    }

    #[test]
    fn tv_dict_get_number_and_def_read_a_found_value() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            tv_dict_add_nr(&mut *d, b"a", 7);
            assert_eq!(tv_dict_get_number(d.as_mut(), b"a"), 7);
            assert_eq!(tv_dict_get_number_def(d.as_mut(), b"a", 99), 7);
            tv_dict_unref(d);
        }
    }

    #[test]
    fn tv_dict_get_bool_uses_default_for_missing_key_and_reads_found_value() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            assert_eq!(tv_dict_get_bool(d.as_mut(), b"missing", 1), 1);
            tv_dict_add_nr(&mut *d, b"flag", 0);
            assert_eq!(tv_dict_get_bool(d.as_mut(), b"flag", 1), 0);
            tv_dict_unref(d);
        }
    }

    // ---- tv_list_uidx / tv_list_find(_nr/_str/_index) / idx_of_item /
    // reverse --------------------------------------------------------

    #[test]
    fn tv_list_uidx_normalizes_negative_and_rejects_out_of_range() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(3);
        unsafe {
            tv_list_append_number(l, 1);
            tv_list_append_number(l, 2);
            tv_list_append_number(l, 3);

            assert_eq!(tv_list_uidx(l, 0), 0);
            assert_eq!(tv_list_uidx(l, 2), 2);
            assert_eq!(tv_list_uidx(l, -1), 2); // last item
            assert_eq!(tv_list_uidx(l, -3), 0); // first item
            assert_eq!(tv_list_uidx(l, 3), -1); // out of range
            assert_eq!(tv_list_uidx(l, -4), -1); // out of range

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_find_locates_by_index_and_caches() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(3);
        unsafe {
            tv_list_append_number(l, 10);
            tv_list_append_number(l, 20);
            tv_list_append_number(l, 30);

            let item = tv_list_find(l, 1);
            assert!(!item.is_null());
            assert_eq!((*item).li_tv.value, TypvalValue::Number(20));
            assert_eq!((*l).lv_idx, 1);
            assert_eq!((*l).lv_idx_item, item);

            // Negative index.
            let last = tv_list_find(l, -1);
            assert_eq!((*last).li_tv.value, TypvalValue::Number(30));

            // Out of range.
            assert!(tv_list_find(l, 5).is_null());
            assert!(tv_list_find(std::ptr::null_mut(), 0).is_null());

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_find_nr_reads_number_and_reports_error_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe {
            tv_list_append_number(l, 42);
            let mut err = false;
            assert_eq!(tv_list_find_nr(l, 0, Some(&mut err)), 42);
            assert!(!err);

            let mut err = false;
            assert_eq!(tv_list_find_nr(l, 5, Some(&mut err)), -1);
            assert!(err);

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_find_str_reads_string_and_none_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe {
            tv_list_append_string(l, Some(b"hi"));
            assert_eq!(tv_list_find_str(l, 0), Some(b"hi".to_vec()));
            assert_eq!(tv_list_find_str(l, 5), None);

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_find_index_falls_back_to_zero_for_an_unfound_negative_index() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(2);
        unsafe {
            tv_list_append_number(l, 1);
            tv_list_append_number(l, 2);

            // A valid negative index is found directly.
            let mut idx = -1;
            let item = tv_list_find_index(l, &mut idx);
            assert!(!item.is_null());
            assert_eq!((*item).li_tv.value, TypvalValue::Number(2));

            // An out-of-range negative index falls back to 0.
            let mut idx = -5;
            let item = tv_list_find_index(l, &mut idx);
            assert_eq!(idx, 0);
            assert!(!item.is_null());
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_idx_of_item_finds_position_or_minus_one() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(2);
        unsafe {
            tv_list_append_number(l, 1);
            tv_list_append_number(l, 2);
            let item0 = (*l).lv_first;
            let item1 = (*item0).li_next;

            assert_eq!(tv_list_idx_of_item(l, item0), 0);
            assert_eq!(tv_list_idx_of_item(l, item1), 1);
            assert_eq!(tv_list_idx_of_item(l, std::ptr::null()), -1);
            assert_eq!(tv_list_idx_of_item(std::ptr::null(), item0), -1);

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_check_range_index_one_normalizes_negative_and_rejects_out_of_range() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(2);
        unsafe {
            tv_list_append_number(l, 10);
            tv_list_append_number(l, 20);

            let mut n1 = -1;
            let li = tv_list_check_range_index_one(l, &mut n1, false);
            assert!(!li.is_null());
            assert_eq!((*li).li_tv.value, TypvalValue::Number(20));

            let mut n1 = 5;
            assert!(tv_list_check_range_index_one(l, &mut n1, false).is_null());

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_check_range_index_two_resolves_negative_n2_and_rejects_reversed_range() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(3);
        unsafe {
            tv_list_append_number(l, 10);
            tv_list_append_number(l, 20);
            tv_list_append_number(l, 30);
            let li1 = (*l).lv_first;

            // n2 == -1 resolves to the last item's index (2).
            let mut n1 = 0;
            let mut n2 = -1;
            assert_eq!(tv_list_check_range_index_two(l, &mut n1, li1, &mut n2, false), OK);
            assert_eq!(n2, 2);

            // An out-of-range negative n2 fails.
            let mut n1 = 0;
            let mut n2 = -10;
            assert_eq!(tv_list_check_range_index_two(l, &mut n1, li1, &mut n2, false), FAIL);

            // n2 before n1 fails (reversed range).
            let mut n1 = 2;
            let mut n2 = 0;
            assert_eq!(tv_list_check_range_index_two(l, &mut n1, li1, &mut n2, false), FAIL);

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_reverse_reorders_items_and_updates_idx() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(3);
        unsafe {
            tv_list_append_number(l, 1);
            tv_list_append_number(l, 2);
            tv_list_append_number(l, 3);

            tv_list_reverse(l);

            let mut collected = Vec::new();
            let mut item = (*l).lv_first;
            while !item.is_null() {
                if let TypvalValue::Number(n) = (*item).li_tv.value {
                    collected.push(n);
                }
                item = (*item).li_next;
            }
            assert_eq!(collected, vec![3, 2, 1]);
            // lv_last must now be the original first item.
            assert_eq!((*(*l).lv_last).li_tv.value, TypvalValue::Number(1));

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_reverse_is_a_noop_for_0_or_1_items() {
        let _lock = crate::globals::global_state_test_lock();
        let empty = tv_list_alloc(0);
        let one = tv_list_alloc(1);
        unsafe {
            tv_list_append_number(one, 1);
            tv_list_reverse(empty); // must not panic
            tv_list_reverse(one);
            assert_eq!((*(*one).lv_first).li_tv.value, TypvalValue::Number(1));
            tv_list_free(empty);
            tv_list_free(one);
        }
    }

    // ---- do_sort_uniq (sort()/uniq()) ------------------------------------

    #[test]
    fn do_sort_uniq_default_comparison_is_by_string_form() {
        // Default (no flag) comparison is BY STRING FORM, not numeric:
        // "10" < "2" < "9" lexically, even though 2 < 9 < 10 numerically.
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[10, 9, 2]);
        let mut rettv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        let argvars = [TypvalT { value: TypvalValue::List(l), ..Default::default() }];
        unsafe { do_sort_uniq(&argvars, &mut rettv, true) };
        assert_eq!(collect_numbers(l), vec![10, 2, 9]);
        unsafe { tv_list_free(l) };
    }

    #[test]
    fn do_sort_uniq_numeric_flag_n_sorts_numerically() {
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[10, 9, 2]);
        let mut rettv = TypvalT::default();
        let argvars = [
            TypvalT { value: TypvalValue::List(l), ..Default::default() },
            TypvalT { value: TypvalValue::String(Some(b"n".to_vec())), ..Default::default() },
        ];
        unsafe { do_sort_uniq(&argvars, &mut rettv, true) };
        assert_eq!(collect_numbers(l), vec![2, 9, 10]);
        unsafe { tv_list_free(l) };
    }

    #[test]
    fn do_sort_uniq_ic_flag_sorts_case_insensitively() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(3);
        unsafe {
            tv_list_append_string(l, Some(b"banana"));
            tv_list_append_string(l, Some(b"Apple"));
            tv_list_append_string(l, Some(b"cherry"));
        }
        let mut rettv = TypvalT::default();
        let argvars = [
            TypvalT { value: TypvalValue::List(l), ..Default::default() },
            TypvalT { value: TypvalValue::Number(1), ..Default::default() },
        ];
        unsafe { do_sort_uniq(&argvars, &mut rettv, true) };
        let mut strs = Vec::new();
        let mut li = unsafe { tv_list_first(l) };
        while !li.is_null() {
            match unsafe { &(*li).li_tv.value } {
                TypvalValue::String(s) => strs.push(s.clone().unwrap()),
                other => panic!("expected String, found {other:?}"),
            }
            li = unsafe { (*li).li_next };
        }
        assert_eq!(strs, vec![b"Apple".to_vec(), b"banana".to_vec(), b"cherry".to_vec()]);
        unsafe { tv_list_free(l) };
    }

    #[test]
    fn do_sort_uniq_short_list_is_a_noop_but_still_sets_rettv() {
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[42]);
        let mut rettv = TypvalT::default();
        let argvars = [TypvalT { value: TypvalValue::List(l), ..Default::default() }];
        unsafe { do_sort_uniq(&argvars, &mut rettv, true) };
        assert_eq!(rettv.value, TypvalValue::List(l));
        assert_eq!(collect_numbers(l), vec![42]);
        unsafe { tv_list_free(l) };
    }

    #[test]
    fn do_sort_uniq_locked_list_is_left_completely_untouched() {
        // A locked list's own check happens BEFORE tv_list_set_ret -
        // rettv is left at its OWN prior value, not even pointed at
        // the (untouched) list.
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[3, 1, 2]);
        unsafe { (*l).lv_lock = VarLockStatus::Locked };
        let mut rettv = TypvalT { value: TypvalValue::Number(99), ..Default::default() };
        let argvars = [TypvalT { value: TypvalValue::List(l), ..Default::default() }];
        unsafe { do_sort_uniq(&argvars, &mut rettv, true) };
        assert_eq!(rettv.value, TypvalValue::Number(99));
        assert_eq!(collect_numbers(l), vec![3, 1, 2]); // unchanged
        unsafe { tv_list_free(l) };
    }

    #[test]
    fn do_sort_uniq_non_list_arg_is_a_noop() {
        let mut rettv = TypvalT { value: TypvalValue::Number(7), ..Default::default() };
        let argvars = [TypvalT { value: TypvalValue::Number(5), ..Default::default() }];
        unsafe { do_sort_uniq(&argvars, &mut rettv, true) };
        assert_eq!(rettv.value, TypvalValue::Number(7));
    }

    #[test]
    fn do_sort_uniq_custom_comparator_funcref_panics() {
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[3, 1, 2]);
        let mut rettv = TypvalT::default();
        let argvars = [
            TypvalT { value: TypvalValue::List(l), ..Default::default() },
            TypvalT { value: TypvalValue::Func(Some(b"SomeComparator".to_vec())), ..Default::default() },
        ];
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            do_sort_uniq(&argvars, &mut rettv, true)
        }));
        assert!(result.is_err(), "expected a panic (item_compare2/call_func not yet translated)");
        unsafe { tv_list_free(l) };
    }

    #[test]
    fn do_sort_uniq_uniq_removes_only_adjacent_duplicates() {
        let _lock = crate::globals::global_state_test_lock();
        let l = int_list_of(&[1, 1, 2, 2, 2, 3, 1]);
        let mut rettv = TypvalT::default();
        let argvars = [TypvalT { value: TypvalValue::List(l), ..Default::default() }];
        unsafe { do_sort_uniq(&argvars, &mut rettv, false) };
        // Only ADJACENT duplicates collapse - the trailing lone `1`
        // stays separate from the leading pair.
        assert_eq!(collect_numbers(l), vec![1, 2, 3, 1]);
        unsafe { tv_list_free(l) };
    }

    // ---- tv_item_lock ---------------------------------------------------

    #[test]
    fn tv_item_lock_locks_and_unlocks_the_typval_itself() {
        let mut tv = number_tv(1);
        unsafe { tv_item_lock(&mut tv, 1, true, false) };
        assert_eq!(tv.v_lock, VarLockStatus::Locked);
        unsafe { tv_item_lock(&mut tv, 1, false, false) };
        assert_eq!(tv.v_lock, VarLockStatus::Unlocked);
    }

    #[test]
    fn tv_item_lock_fixed_stays_fixed_regardless_of_lock_flag() {
        let mut tv = TypvalT { v_lock: VarLockStatus::Fixed, value: TypvalValue::Number(1) };
        unsafe { tv_item_lock(&mut tv, 1, true, false) };
        assert_eq!(tv.v_lock, VarLockStatus::Fixed);
        unsafe { tv_item_lock(&mut tv, 1, false, false) };
        assert_eq!(tv.v_lock, VarLockStatus::Fixed);
    }

    #[test]
    fn tv_item_lock_deep_0_is_a_complete_noop() {
        let mut tv = number_tv(1);
        unsafe { tv_item_lock(&mut tv, 0, true, false) };
        assert_eq!(tv.v_lock, VarLockStatus::Unlocked);
    }

    #[test]
    fn tv_item_lock_deep_1_locks_list_itself_but_not_its_items() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe {
            tv_list_append_tv(l, &number_tv(1));
            let mut tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(l) };

            tv_item_lock(&mut tv, 1, true, false);

            assert_eq!((*l).lv_lock, VarLockStatus::Locked);
            // deep == 1 does not recurse into the item.
            assert_eq!((*(*l).lv_first).li_tv.v_lock, VarLockStatus::Unlocked);

            tv_list_free(l);
        }
    }

    #[test]
    fn tv_item_lock_deep_negative_one_recurses_unlimited() {
        let _lock = crate::globals::global_state_test_lock();
        let inner = tv_list_alloc(1);
        let outer = tv_list_alloc(1);
        unsafe {
            tv_list_append_number(inner, 42);
            let inner_tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(inner) };
            tv_list_append_tv(outer, &inner_tv);

            let mut tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(outer) };
            tv_item_lock(&mut tv, -1, true, false);

            assert_eq!((*outer).lv_lock, VarLockStatus::Locked);
            let outer_item = (*outer).lv_first;
            assert_eq!((*outer_item).li_tv.v_lock, VarLockStatus::Locked);
            let TypvalValue::List(inner_ptr) = (*outer_item).li_tv.value else { panic!("expected List") };
            assert_eq!((*inner_ptr).lv_lock, VarLockStatus::Locked);
            // Recurses all the way to the leaf number's own v_lock too.
            assert_eq!((*(*inner_ptr).lv_first).li_tv.v_lock, VarLockStatus::Locked);

            tv_list_free(outer); // frees inner transitively via tv_list_free_contents
        }
    }

    #[test]
    fn tv_item_lock_skips_when_check_refcount_and_refcount_over_1() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(0);
        unsafe {
            (*l).lv_refcount = 2;
            let mut tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(l) };

            tv_item_lock(&mut tv, 1, true, true);

            assert_eq!((*l).lv_lock, VarLockStatus::Unlocked); // untouched
            (*l).lv_refcount = 0; // avoid tripping tv_list_free's own checks
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_item_lock_recurses_into_dict_items() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            tv_dict_add_nr(&mut *d, b"a", 1);
            let mut tv = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(d) };

            tv_item_lock(&mut tv, -1, true, false);

            assert_eq!((*d).dv_lock, VarLockStatus::Locked);
            let item = tv_dict_find(d.as_mut(), b"a").unwrap();
            assert_eq!((*item).di_tv.v_lock, VarLockStatus::Locked);

            tv_dict_unref(d);
        }
    }

    // --- tv_list_move_items / tv_list_remove ---

    #[test]
    fn tv_list_move_items_moves_a_range_to_the_end_of_the_target() {
        let _lock = crate::globals::global_state_test_lock();
        let src = tv_list_alloc(3);
        unsafe {
            tv_list_append_number(&mut *src, 1);
            tv_list_append_number(&mut *src, 2);
            tv_list_append_number(&mut *src, 3);
        }
        let tgt = tv_list_alloc(1);
        unsafe { tv_list_append_number(&mut *tgt, 99) };
        unsafe {
            let item = tv_list_find(src, 1); // the "2" item.
            tv_list_move_items(src, item, item, tgt, 1);
            assert_eq!(tv_list_len(src), 2);
            assert_eq!(tv_list_len(tgt), 2);
            let first = tv_list_first(src);
            assert_eq!((*first).li_tv.value, TypvalValue::Number(1));
            assert_eq!((*(*first).li_next).li_tv.value, TypvalValue::Number(3));
            let tfirst = tv_list_first(tgt);
            assert_eq!((*tfirst).li_tv.value, TypvalValue::Number(99));
            assert_eq!((*(*tfirst).li_next).li_tv.value, TypvalValue::Number(2));
            tv_list_free(src);
            tv_list_free(tgt);
        }
    }

    #[test]
    fn tv_list_remove_removes_a_single_item_and_returns_its_value() {
        let _lock = crate::globals::global_state_test_lock();
        let list = tv_list_alloc(3);
        unsafe {
            tv_list_append_number(&mut *list, 1);
            tv_list_append_number(&mut *list, 2);
            tv_list_append_number(&mut *list, 3);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, number_tv(1)];
        unsafe { tv_list_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));
        unsafe {
            assert_eq!(tv_list_len(list), 2);
            let item = tv_list_first(list);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            assert_eq!((*(*item).li_next).li_tv.value, TypvalValue::Number(3));
            tv_list_free(list);
        }
    }

    #[test]
    fn tv_list_remove_removes_a_range_and_returns_a_new_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = tv_list_alloc(4);
        unsafe {
            tv_list_append_number(&mut *list, 1);
            tv_list_append_number(&mut *list, 2);
            tv_list_append_number(&mut *list, 3);
            tv_list_append_number(&mut *list, 4);
        }
        let mut rettv = TypvalT::default();
        let args =
            [TypvalT { value: TypvalValue::List(list), ..Default::default() }, number_tv(1), number_tv(2)];
        unsafe { tv_list_remove(&args, &mut rettv) };
        let TypvalValue::List(removed) = rettv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(tv_list_len(list), 2);
            let item = tv_list_first(list);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            assert_eq!((*(*item).li_next).li_tv.value, TypvalValue::Number(4));

            assert_eq!(tv_list_len(removed), 2);
            let ritem = tv_list_first(removed);
            assert_eq!((*ritem).li_tv.value, TypvalValue::Number(2));
            assert_eq!((*(*ritem).li_next).li_tv.value, TypvalValue::Number(3));

            tv_list_free(list);
            tv_list_unref(removed);
        }
    }

    #[test]
    fn tv_list_remove_on_a_locked_list_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = tv_list_alloc(1);
        unsafe {
            tv_list_append_number(&mut *list, 1);
            (*list).lv_lock = VarLockStatus::Locked;
        }
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, number_tv(0)];
        unsafe { tv_list_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { tv_list_free(list) };
    }

    #[test]
    fn tv_list_remove_with_an_out_of_range_index_leaves_rettv_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = tv_list_alloc(0);
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, number_tv(5)];
        unsafe { tv_list_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { tv_list_free(list) };
    }

    #[test]
    fn tv_list_remove_with_end_before_idx_leaves_rettv_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = tv_list_alloc(2);
        unsafe {
            tv_list_append_number(&mut *list, 1);
            tv_list_append_number(&mut *list, 2);
        }
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args =
            [TypvalT { value: TypvalValue::List(list), ..Default::default() }, number_tv(1), number_tv(0)];
        unsafe { tv_list_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { tv_list_free(list) };
    }

    // --- tv_blob_remove ---

    #[test]
    fn tv_blob_remove_removes_a_single_byte_and_returns_it() {
        let blob = tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![10, 20, 30];
            (*blob).bv_ga.ga_len = 3;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, number_tv(1)];
        unsafe { tv_blob_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(20));
        unsafe {
            assert_eq!(tv_blob_len(blob), 2);
            assert_eq!(tv_blob_get(blob, 0), 10);
            assert_eq!(tv_blob_get(blob, 1), 30);
            tv_blob_free(blob);
        }
    }

    #[test]
    fn tv_blob_remove_with_a_negative_index_counts_from_the_end() {
        let blob = tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![10, 20, 30];
            (*blob).bv_ga.ga_len = 3;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, number_tv(-1)];
        unsafe { tv_blob_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(30));
        unsafe {
            assert_eq!(tv_blob_len(blob), 2);
            tv_blob_free(blob);
        }
    }

    #[test]
    fn tv_blob_remove_removes_a_range_and_returns_a_new_blob() {
        let blob = tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![1, 2, 3, 4];
            (*blob).bv_ga.ga_len = 4;
        }
        let mut rettv = TypvalT::default();
        let args =
            [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, number_tv(1), number_tv(2)];
        unsafe { tv_blob_remove(&args, &mut rettv) };
        let TypvalValue::Blob(removed) = rettv.value else { panic!("expected a Blob") };
        unsafe {
            assert_eq!(tv_blob_len(blob), 2);
            assert_eq!(tv_blob_get(blob, 0), 1);
            assert_eq!(tv_blob_get(blob, 1), 4);

            assert_eq!(tv_blob_len(removed), 2);
            assert_eq!(tv_blob_get(removed, 0), 2);
            assert_eq!(tv_blob_get(removed, 1), 3);

            tv_blob_free(blob);
            tv_blob_free(removed);
        }
    }

    #[test]
    fn tv_blob_remove_on_a_locked_blob_leaves_it_untouched() {
        let blob = tv_blob_alloc();
        unsafe {
            (*blob).bv_lock = VarLockStatus::Locked;
            (*blob).bv_ga.ga_data = vec![1];
            (*blob).bv_ga.ga_len = 1;
        }
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, number_tv(0)];
        unsafe { tv_blob_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            assert_eq!(tv_blob_len(blob), 1);
            tv_blob_free(blob);
        }
    }

    #[test]
    fn tv_blob_remove_of_a_null_blob_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Blob(std::ptr::null_mut()), ..Default::default() }, number_tv(0)];
        unsafe { tv_blob_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- tv_dict_remove ---

    #[test]
    fn tv_dict_remove_removes_a_key_and_returns_its_value() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = tv_dict_alloc();
        unsafe {
            tv_dict_add_nr(&mut *dict, b"a", 1);
            tv_dict_add_nr(&mut *dict, b"b", 2);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string_tv(b"a")];
        unsafe { tv_dict_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe {
            assert_eq!(tv_dict_len(dict.as_ref()), 1);
            assert!(tv_dict_find(dict.as_mut(), b"a").is_none());
            tv_dict_unref(dict);
        }
    }

    #[test]
    fn tv_dict_remove_of_a_missing_key_leaves_rettv_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = tv_dict_alloc();
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string_tv(b"missing")];
        unsafe { tv_dict_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { tv_dict_unref(dict) };
    }

    #[test]
    fn tv_dict_remove_on_a_locked_dict_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = tv_dict_alloc();
        unsafe {
            tv_dict_add_nr(&mut *dict, b"a", 1);
            (*dict).dv_lock = VarLockStatus::Locked;
        }
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string_tv(b"a")];
        unsafe { tv_dict_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            assert_eq!(tv_dict_len(dict.as_ref()), 1);
            tv_dict_unref(dict);
        }
    }

    #[test]
    fn tv_dict_remove_of_a_null_dict_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args =
            [TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() }, string_tv(b"a")];
        unsafe { tv_dict_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- tv_list_join ---

    #[test]
    fn tv_list_join_joins_numbers_with_a_separator() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(3);
        unsafe {
            tv_list_append_number(&mut *l, 1);
            tv_list_append_number(&mut *l, 2);
            tv_list_append_number(&mut *l, 3);
        }
        unsafe {
            assert_eq!(tv_list_join(l, b", "), b"1, 2, 3".to_vec());
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_join_of_an_empty_list_is_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(0);
        unsafe {
            assert_eq!(tv_list_join(l, b" "), Vec::<u8>::new());
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_join_of_a_null_list_is_empty() {
        unsafe { assert_eq!(tv_list_join(std::ptr::null_mut(), b" "), Vec::<u8>::new()) };
    }

    #[test]
    fn tv_list_join_of_a_single_item_has_no_separator() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        unsafe { tv_list_append_number(&mut *l, 42) };
        unsafe {
            assert_eq!(tv_list_join(l, b", "), b"42".to_vec());
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_join_stringifies_nested_values_with_echo_rules() {
        let _lock = crate::globals::global_state_test_lock();
        let inner = tv_list_alloc(2);
        unsafe {
            tv_list_append_number(inner, 1);
            tv_list_append_string(inner, Some(b"x"));
        }
        let l = tv_list_alloc(1);
        unsafe { tv_list_append_owned_tv(l, TypvalT { value: TypvalValue::List(inner), ..Default::default() }) };
        assert_eq!(
            unsafe { tv_list_join(l, b" ") },
            b"[1, 'x']".to_vec()
        );
        unsafe { tv_list_free(l) };
    }

    // --- tv_list_flatten ---

    fn append_nested_list(outer: *mut crate::eval::typval_defs::ListT, items: &[crate::eval::typval_defs::VarnumberT]) {
        let inner = tv_list_alloc(items.len() as isize);
        unsafe {
            tv_list_ref(inner);
            for &n in items {
                tv_list_append_number(&mut *inner, n);
            }
            tv_list_append_owned_tv(outer, TypvalT { value: TypvalValue::List(inner), ..Default::default() });
        }
    }

    #[test]
    fn tv_list_flatten_flattens_one_level_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(3);
        unsafe { tv_list_append_number(&mut *l, 1) };
        append_nested_list(l, &[2, 3]);
        unsafe { tv_list_append_number(&mut *l, 4) };

        unsafe {
            let len = tv_list_len(l);
            tv_list_flatten(l, std::ptr::null_mut(), i64::from(len), 999_999);
            assert_eq!(tv_list_len(l), 4);
            let mut vals = Vec::new();
            let mut item = tv_list_first(l);
            while !item.is_null() {
                let TypvalValue::Number(n) = (*item).li_tv.value else { panic!("expected a Number") };
                vals.push(n);
                item = (*item).li_next;
            }
            assert_eq!(vals, vec![1, 2, 3, 4]);
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_flatten_respects_maxdepth() {
        let _lock = crate::globals::global_state_test_lock();
        // [1, [2, [3, 4]]] with maxdepth=1 -> [1, 2, [3, 4]].
        let l = tv_list_alloc(2);
        unsafe { tv_list_append_number(&mut *l, 1) };
        let middle = tv_list_alloc(2);
        unsafe { tv_list_ref(middle) };
        unsafe { tv_list_append_number(&mut *middle, 2) };
        append_nested_list(middle, &[3, 4]);
        unsafe {
            tv_list_append_owned_tv(l, TypvalT { value: TypvalValue::List(middle), ..Default::default() });
        }

        unsafe {
            let len = tv_list_len(l);
            tv_list_flatten(l, std::ptr::null_mut(), i64::from(len), 1);
            assert_eq!(tv_list_len(l), 3); // [1, 2, [3, 4]] - 3 top-level items.
            let item = tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            let item2 = (*item).li_next;
            assert_eq!((*item2).li_tv.value, TypvalValue::Number(2));
            let item3 = (*item2).li_next;
            let TypvalValue::List(nested) = (*item3).li_tv.value else { panic!("expected a nested List") };
            assert_eq!(tv_list_len(nested), 2);
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_flatten_of_zero_maxdepth_does_nothing() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(1);
        append_nested_list(l, &[1, 2]);
        unsafe {
            let len = tv_list_len(l);
            tv_list_flatten(l, std::ptr::null_mut(), i64::from(len), 0);
            assert_eq!(tv_list_len(l), 1); // untouched.
            tv_list_free(l);
        }
    }

    #[test]
    fn tv_list_flatten_absorbs_an_empty_nested_list() {
        let _lock = crate::globals::global_state_test_lock();
        let l = tv_list_alloc(3);
        unsafe {
            tv_list_append_number(&mut *l, 1);
            let empty = tv_list_alloc(0);
            tv_list_ref(empty);
            tv_list_append_owned_tv(l, TypvalT { value: TypvalValue::List(empty), ..Default::default() });
            tv_list_append_number(&mut *l, 2);
        }
        unsafe {
            let len = tv_list_len(l);
            tv_list_flatten(l, std::ptr::null_mut(), i64::from(len), 999_999);
            assert_eq!(tv_list_len(l), 2);
            let item = tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            assert_eq!((*(*item).li_next).li_tv.value, TypvalValue::Number(2));
            tv_list_free(l);
        }
    }
}

#[cfg(test)]
mod filter_map_tests {
    use super::*;
    use crate::hashtab::hashitem_empty;

    fn string_expr(s: &[u8]) -> TypvalT {
        TypvalT { value: TypvalValue::String(Some(s.to_vec())), ..TypvalT::default() }
    }

    fn list_of(nums: &[VarnumberT]) -> *mut crate::eval::typval_defs::ListT {
        let l = tv_list_alloc(nums.len() as isize);
        for n in nums {
            unsafe { tv_list_append_number(&mut *l, *n) };
        }
        l
    }

    fn collect(l: *mut crate::eval::typval_defs::ListT) -> Vec<VarnumberT> {
        let mut out = Vec::new();
        let mut li = unsafe { tv_list_first(l) };
        while !li.is_null() {
            match unsafe { &(*li).li_tv.value } {
                TypvalValue::Number(n) => out.push(*n),
                other => panic!("expected Number, found {other:?}"),
            }
            li = unsafe { (*li).li_next };
        }
        out
    }

    #[test]
    fn filter_removes_items_that_are_falsy() {
        let _lock = crate::globals::global_state_test_lock();
        let l = list_of(&[1, 2, 3, 4, 5]);
        let argvars = [
            TypvalT { value: TypvalValue::List(l), ..TypvalT::default() },
            string_expr(b"v:val % 2 == 0"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };

        assert_eq!(collect(l), vec![2, 4]);
        // filter() returns the (mutated) first argument itself.
        assert!(matches!(rettv.value, TypvalValue::List(p) if p == l));

        unsafe { tv_list_unref(l) };
    }

    #[test]
    fn map_replaces_each_item_in_place() {
        let _lock = crate::globals::global_state_test_lock();
        let l = list_of(&[1, 2, 3]);
        let argvars = [
            TypvalT { value: TypvalValue::List(l), ..TypvalT::default() },
            string_expr(b"v:val * 10"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Map) };

        assert_eq!(collect(l), vec![10, 20, 30]);
        assert!(matches!(rettv.value, TypvalValue::List(p) if p == l));

        unsafe { tv_list_unref(l) };
    }

    #[test]
    fn mapnew_builds_a_new_list_leaving_the_original_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let l = list_of(&[1, 2, 3]);
        let argvars = [
            TypvalT { value: TypvalValue::List(l), ..TypvalT::default() },
            string_expr(b"v:val + 100"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::MapNew) };

        // Original untouched.
        assert_eq!(collect(l), vec![1, 2, 3]);
        let TypvalValue::List(l_new) = rettv.value else {
            panic!("expected a new List");
        };
        assert_ne!(l_new, l);
        assert_eq!(collect(l_new), vec![101, 102, 103]);

        unsafe {
            tv_list_unref(l);
            tv_list_unref(l_new);
        }
    }

    #[test]
    fn v_key_reflects_the_zero_based_index_during_iteration() {
        let _lock = crate::globals::global_state_test_lock();
        let l = list_of(&[10, 20, 30]);
        let argvars = [
            TypvalT { value: TypvalValue::List(l), ..TypvalT::default() },
            string_expr(b"v:val + v:key"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Map) };

        assert_eq!(collect(l), vec![10, 21, 32]);
        unsafe { tv_list_unref(l) };
    }

    #[test]
    fn filter_on_an_empty_list_is_a_no_op() {
        let _lock = crate::globals::global_state_test_lock();
        let l = list_of(&[]);
        let argvars = [
            TypvalT { value: TypvalValue::List(l), ..TypvalT::default() },
            string_expr(b"v:val > 0"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };
        assert!(collect(l).is_empty());
        unsafe { tv_list_unref(l) };
    }

    #[test]
    fn filter_stays_faithful_when_removing_the_first_item() {
        let _lock = crate::globals::global_state_test_lock();
        let l = list_of(&[0, 1, 2]);
        let argvars = [
            TypvalT { value: TypvalValue::List(l), ..TypvalT::default() },
            string_expr(b"v:val"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };
        assert_eq!(collect(l), vec![1, 2]);
        unsafe { tv_list_unref(l) };
    }

    #[test]
    fn map_on_a_locked_item_stops_the_whole_loop_early() {
        let _lock = crate::globals::global_state_test_lock();
        let l = list_of(&[1, 2, 3]);
        // Lock the SECOND item specifically.
        let second = unsafe { (*tv_list_first(l)).li_next };
        unsafe { (*second).li_tv.v_lock = VarLockStatus::Locked };

        let argvars = [
            TypvalT { value: TypvalValue::List(l), ..TypvalT::default() },
            string_expr(b"v:val * 100"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Map) };

        // First item mapped, second (locked) and third left untouched -
        // matches the original's own "value_check_lock breaks the
        // whole loop, not just skips this one item" structure.
        assert_eq!(collect(l), vec![100, 2, 3]);
        unsafe { tv_list_unref(l) };
    }

    #[test]
    fn filter_map_one_returns_fail_when_expression_evaluation_fails() {
        let _lock = crate::globals::global_state_test_lock();
        // filter_map_one has no save/restore logic of its own (that is
        // filter_map's job, per its own doc comment - "Caller must set
        // v:key") - calling it directly, as this test deliberately
        // does to isolate filter_map_one from the full filter_map
        // dispatch, still needs prepare_vimvar/restore_vimvar wrapped
        // around it, exactly like a real caller always does, or v:val's
        // slot is left permanently holding whatever tv_copy just wrote
        // into it (tv_clear_simple does nothing to reset a Number back
        // to Unknown - it only releases owned List/Dict/Blob/String
        // resources, of which a Number has none).
        let mut save_val = TypvalT::default();
        unsafe { crate::eval::vars::prepare_vimvar(crate::eval::vars::VimVarIndex::Val, &mut save_val) };

        let tv = TypvalT { value: TypvalValue::Number(1), ..TypvalT::default() };
        let expr = string_expr(b"1 +"); // deliberately invalid
        let (ret, _newtv, _rem) = unsafe { filter_map_one(&tv, &expr, FilterMapT::Filter) };
        assert_eq!(ret, FAIL);

        unsafe { crate::eval::vars::restore_vimvar(crate::eval::vars::VimVarIndex::Val, save_val) };
    }

    #[test]
    fn filter_map_one_releases_v_vals_own_temporary_reference_after_use() {
        let _lock = crate::globals::global_state_test_lock();
        // v:val must be registered (prepare_vimvar) for the expression
        // evaluator to actually find it via a real Vimscript reference -
        // matches what filter_map (the real caller) always does first.
        // prepare_vimvar also leaves v:val's slot cleared (Unknown),
        // matching the precondition filter_map_one itself relies on:
        // its own tv_copy(tv, v:val_slot) call is a plain, C-style
        // struct overwrite with NO cleanup of any PRE-EXISTING value in
        // the destination (verified directly against the real tv_copy
        // source: `memmove(&to->vval, &from->vval, ...)`, no free of
        // `to`'s old value at all) - exactly mirrored by this crate's
        // own `to.value = from.value.clone()` - so v:val's slot must
        // already be empty before filter_map_one is called, which it
        // always is in real use (either fresh from prepare_vimvar, or
        // already tv_clear_simple'd by the PREVIOUS iteration's own
        // filter_map_one call).
        let mut save_val = TypvalT::default();
        unsafe { crate::eval::vars::prepare_vimvar(crate::eval::vars::VimVarIndex::Val, &mut save_val) };

        // The per-item value itself (`tv`) is a List - tv_copy's own
        // List branch bumps its refcount when copying it into v:val.
        let l = tv_list_alloc(0);
        unsafe { tv_list_ref(l) }; // this test's own +1 ref, keeping `l` alive
        assert_eq!(unsafe { (*l).lv_refcount }, 1);

        let tv = TypvalT { value: TypvalValue::List(l), ..TypvalT::default() };
        let expr = string_expr(b"1"); // doesn't need to inspect v:val itself
        let (ret, newtv, _rem) = unsafe { filter_map_one(&tv, &expr, FilterMapT::Filter) };
        assert_eq!(ret, OK);
        unsafe { tv_clear_simple(&newtv) };

        // v:val's own temporary reference (bumped by tv_copy, released
        // by filter_map_one's own final tv_clear_simple) is gone -
        // refcount back to just this test's own +1 ref.
        assert_eq!(unsafe { (*l).lv_refcount }, 1);

        unsafe { crate::eval::vars::restore_vimvar(crate::eval::vars::VimVarIndex::Val, save_val) };
        unsafe { tv_list_unref(l) };
    }

    #[test]
    fn filter_removes_dict_entries_that_are_falsy() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            // No manual dv_refcount bump here (unlike e.g.
            // e2e_copy_builtin_function_calls's own g: variable
            // pattern, which genuinely needs one to represent that
            // separate owner) - filter_map's own tv_copy(argvars[0],
            // rettv) call is the ONLY reference this test needs to
            // balance against its own single tv_dict_unref(d) at the
            // end. A duplicate manual bump here would leave the dict
            // stuck at refcount 1 forever, permanently leaking it into
            // GC_FIRST_DICT.
            let a = tv_dict_item_alloc(b"keep");
            (*a).di_tv.value = TypvalValue::Number(1);
            tv_dict_add(&mut *d, a);
            let b = tv_dict_item_alloc(b"drop");
            (*b).di_tv.value = TypvalValue::Number(0);
            tv_dict_add(&mut *d, b);
        }
        let argvars =
            [TypvalT { value: TypvalValue::Dict(d), ..TypvalT::default() }, string_expr(b"v:val")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };

        assert_eq!(unsafe { tv_dict_len(Some(&*d)) }, 1);
        assert!(unsafe { tv_dict_find(Some(&mut *d), b"keep") }.is_some());
        assert!(unsafe { tv_dict_find(Some(&mut *d), b"drop") }.is_none());
        assert!(matches!(rettv.value, TypvalValue::Dict(p) if p == d));

        unsafe { tv_dict_unref(d) };
    }

    #[test]
    fn map_replaces_each_dict_value_using_v_key() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let a = tv_dict_item_alloc(b"a");
            (*a).di_tv.value = TypvalValue::Number(1);
            tv_dict_add(&mut *d, a);
        }
        let argvars = [
            TypvalT { value: TypvalValue::Dict(d), ..TypvalT::default() },
            string_expr(b"v:key . \":\" . v:val"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Map) };

        let item = unsafe { tv_dict_find(Some(&mut *d), b"a") }.unwrap();
        assert_eq!(unsafe { (*item).di_tv.value.clone() }, TypvalValue::String(Some(b"a:1".to_vec())));

        unsafe { tv_dict_unref(d) };
    }

    #[test]
    fn mapnew_builds_a_new_dict_leaving_the_original_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let a = tv_dict_item_alloc(b"a");
            (*a).di_tv.value = TypvalValue::Number(1);
            tv_dict_add(&mut *d, a);
        }
        let argvars = [
            TypvalT { value: TypvalValue::Dict(d), ..TypvalT::default() },
            string_expr(b"v:val + 100"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::MapNew) };

        // Original untouched.
        let orig_item = unsafe { tv_dict_find(Some(&mut *d), b"a") }.unwrap();
        assert_eq!(unsafe { (*orig_item).di_tv.value.clone() }, TypvalValue::Number(1));

        let TypvalValue::Dict(d_new) = rettv.value else { panic!("expected a new Dict") };
        assert_ne!(d_new, d);
        let new_item = unsafe { tv_dict_find(Some(&mut *d_new), b"a") }.unwrap();
        assert_eq!(unsafe { (*new_item).di_tv.value.clone() }, TypvalValue::Number(101));

        unsafe {
            tv_dict_unref(d);
            tv_dict_unref(d_new);
        }
    }

    #[test]
    fn filter_on_a_dict_declines_to_remove_a_fixed_entry() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        unsafe {
            let a = tv_dict_item_alloc(b"a");
            (*a).di_tv.value = TypvalValue::Number(0); // falsy - would be removed
            (*a).di_flags = crate::eval::typval_defs::dict_item_flags::FIX;
            tv_dict_add(&mut *d, a);
        }
        let argvars =
            [TypvalT { value: TypvalValue::Dict(d), ..TypvalT::default() }, string_expr(b"v:val")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };

        // The loop breaks (not removed) since the entry is FIX-flagged.
        assert_eq!(unsafe { tv_dict_len(Some(&*d)) }, 1);
        unsafe { tv_dict_unref(d) };
    }

    #[test]
    fn filter_on_an_empty_dict_is_a_no_op() {
        let _lock = crate::globals::global_state_test_lock();
        let d = tv_dict_alloc();
        let argvars =
            [TypvalT { value: TypvalValue::Dict(d), ..TypvalT::default() }, string_expr(b"1")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };
        assert_eq!(unsafe { tv_dict_len(Some(&*d)) }, 0);
        unsafe { tv_dict_unref(d) };
    }

    fn blob_of(bytes: &[u8]) -> *mut crate::eval::typval_defs::BlobT {
        let b = tv_blob_alloc();
        unsafe {
            (*b).bv_ga.ga_data.extend_from_slice(bytes);
            (*b).bv_ga.ga_len = bytes.len() as i32;
        }
        b
    }

    fn blob_bytes(b: *mut crate::eval::typval_defs::BlobT) -> Vec<u8> {
        unsafe { (&(*b).bv_ga.ga_data)[..(*b).bv_ga.ga_len as usize].to_vec() }
    }

    #[test]
    fn filter_removes_blob_bytes_that_are_falsy() {
        let _lock = crate::globals::global_state_test_lock();
        let b = blob_of(&[1, 0, 2, 0, 3]);
        let argvars =
            [TypvalT { value: TypvalValue::Blob(b), ..TypvalT::default() }, string_expr(b"v:val")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };

        assert_eq!(blob_bytes(b), vec![1, 2, 3]);
        assert!(matches!(rettv.value, TypvalValue::Blob(p) if p == b));

        unsafe { tv_blob_unref(b) };
    }

    #[test]
    fn map_replaces_each_blob_byte_in_place() {
        let _lock = crate::globals::global_state_test_lock();
        let b = blob_of(&[1, 2, 3]);
        let argvars = [
            TypvalT { value: TypvalValue::Blob(b), ..TypvalT::default() },
            string_expr(b"v:val + 10"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Map) };

        assert_eq!(blob_bytes(b), vec![11, 12, 13]);

        unsafe { tv_blob_unref(b) };
    }

    #[test]
    fn mapnew_builds_a_new_blob_leaving_the_original_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let b = blob_of(&[1, 2, 3]);
        let argvars = [
            TypvalT { value: TypvalValue::Blob(b), ..TypvalT::default() },
            string_expr(b"v:val + 100"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::MapNew) };

        // Original untouched.
        assert_eq!(blob_bytes(b), vec![1, 2, 3]);
        let TypvalValue::Blob(b_new) = rettv.value else { panic!("expected a new Blob") };
        assert_ne!(b_new, b);
        assert_eq!(blob_bytes(b_new), vec![101, 102, 103]);

        unsafe {
            tv_blob_unref(b);
            tv_blob_unref(b_new);
        }
    }

    #[test]
    fn v_key_reflects_the_zero_based_index_during_blob_iteration() {
        let _lock = crate::globals::global_state_test_lock();
        let b = blob_of(&[10, 20, 30]);
        let argvars = [
            TypvalT { value: TypvalValue::Blob(b), ..TypvalT::default() },
            string_expr(b"v:val + v:key"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Map) };

        assert_eq!(blob_bytes(b), vec![10, 21, 32]);
        unsafe { tv_blob_unref(b) };
    }

    #[test]
    fn filter_on_an_empty_blob_is_a_no_op() {
        let _lock = crate::globals::global_state_test_lock();
        let b = blob_of(&[]);
        let argvars =
            [TypvalT { value: TypvalValue::Blob(b), ..TypvalT::default() }, string_expr(b"1")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };
        assert_eq!(blob_bytes(b), Vec::<u8>::new());
        unsafe { tv_blob_unref(b) };
    }

    #[test]
    fn filter_stays_faithful_when_removing_the_first_blob_byte() {
        let _lock = crate::globals::global_state_test_lock();
        let b = blob_of(&[0, 1, 2]);
        let argvars =
            [TypvalT { value: TypvalValue::Blob(b), ..TypvalT::default() }, string_expr(b"v:val")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };
        assert_eq!(blob_bytes(b), vec![1, 2]);
        unsafe { tv_blob_unref(b) };
    }

    #[test]
    fn filter_removes_string_characters_that_are_falsy() {
        let _lock = crate::globals::global_state_test_lock();
        let argvars =
            [string_expr(b"abc"), string_expr(b"v:val != 'b'")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"ac".to_vec())));
    }

    #[test]
    fn map_replaces_each_string_character_with_a_string_result() {
        let _lock = crate::globals::global_state_test_lock();
        let argvars = [string_expr(b"abc"), string_expr(b"toupper(v:val)")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Map) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"ABC".to_vec())));
    }

    #[test]
    fn mapnew_on_a_string_produces_a_new_string_too() {
        let _lock = crate::globals::global_state_test_lock();
        let argvars = [string_expr(b"ab"), string_expr(b"v:val . v:val")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::MapNew) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"aabb".to_vec())));
    }

    #[test]
    fn filter_on_an_empty_string_is_a_no_op() {
        let _lock = crate::globals::global_state_test_lock();
        let argvars = [string_expr(b""), string_expr(b"1")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };
        assert_eq!(rettv.value, TypvalValue::String(Some(Vec::new())));
    }

    #[test]
    fn v_key_reflects_the_zero_based_index_during_string_iteration() {
        let _lock = crate::globals::global_state_test_lock();
        let argvars = [string_expr(b"xyz"), string_expr(b"v:key == 1 ? \"_\" : v:val")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Map) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"x_z".to_vec())));
    }

    #[test]
    fn map_on_a_string_treats_each_multi_byte_character_as_one_unit() {
        let _lock = crate::globals::global_state_test_lock();
        // A 2-character, 6-byte UTF-8 string ("日本" - two 3-byte CJK
        // characters) - v:key must advance by CHARACTER, not by byte,
        // matching utfc_ptr2len's own per-character length.
        let argvars = [
            string_expr("日本".as_bytes()),
            string_expr(b"v:key == 0 ? 'A' : 'B'"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Map) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"AB".to_vec())));
    }

    #[test]
    fn map_on_a_string_breaks_when_expr_does_not_return_a_string() {
        let _lock = crate::globals::global_state_test_lock();
        let argvars = [string_expr(b"ab"), string_expr(b"42")];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Map) };
        // Breaks on the FIRST character - result is empty, not "ab"
        // unchanged (map() on a String always builds a fresh result).
        assert_eq!(rettv.value, TypvalValue::String(Some(Vec::new())));
    }

    #[test]
    fn filter_on_a_non_container_type_is_a_graceful_no_op() {
        let _lock = crate::globals::global_state_test_lock();
        let argvars = [
            TypvalT { value: TypvalValue::Number(5), ..TypvalT::default() },
            string_expr(b"1"),
        ];
        let mut rettv = TypvalT::default();
        // Must not panic - a Number is neither List/Dict/Blob/String.
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };
        assert_eq!(rettv.value, TypvalValue::Number(5));
    }

    #[test]
    fn filter_leaves_v_val_and_v_key_unregistered_afterward() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(unsafe {
            hashitem_empty(
                (*crate::eval::vars::get_vimvar_dict()).dv_hashtab.hash_find(b"val"),
            )
        });
        assert!(unsafe {
            hashitem_empty(
                (*crate::eval::vars::get_vimvar_dict()).dv_hashtab.hash_find(b"key"),
            )
        });

        let l = list_of(&[1, 2]);
        let argvars = [
            TypvalT { value: TypvalValue::List(l), ..TypvalT::default() },
            string_expr(b"v:val"),
        ];
        let mut rettv = TypvalT::default();
        unsafe { filter(&argvars, &mut rettv, FilterMapT::Filter) };

        assert!(unsafe {
            hashitem_empty(
                (*crate::eval::vars::get_vimvar_dict()).dv_hashtab.hash_find(b"val"),
            )
        });
        assert!(unsafe {
            hashitem_empty(
                (*crate::eval::vars::get_vimvar_dict()).dv_hashtab.hash_find(b"key"),
            )
        });

        unsafe { tv_list_unref(l) };
    }

    /// Thin wrapper matching `f_filter`/`f_map`/`f_mapnew`'s own
    /// eventual shape, so every test above reads naturally as "call
    /// the builtin", without yet requiring `eval/funcs.rs`'s own
    /// registration machinery.
    unsafe fn filter(argvars: &[TypvalT], rettv: &mut TypvalT, filtermap: FilterMapT) {
        unsafe { filter_map(argvars, rettv, filtermap) };
    }
}
