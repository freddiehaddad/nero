//! Translated from `src/nvim/eval/typval.c` (tractable core: the
//! `dict_T`/`list_T`/`blob_T` alloc/free/refcount/insertion primitives,
//! `tv_copy`, and `tv_get_number`/`tv_get_bool`).
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
//! `tv_dict_has_key`, `tv_dict_add` (omits the original's
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
//! exist).
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
//! `tv_list_watch_fix`.
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

use crate::eval::typval_defs::{dict_item_flags, DictT, DictitemT, PartialT, ScopeType, TypvalT, TypvalValue, VarLockStatus};
use crate::globals::GlobalCell;
use crate::vim_defs::{FAIL, OK};

/// `LUA_NOREF`: represents a missing Lua reference - `DictT`'s own
/// `lua_table_ref` is always this value until the Lua host (phase 13)
/// exists.
const LUA_NOREF: crate::types_defs::LuaRef = -1;

/// `gc_first_dict`: head of the linked list of all live dictionaries
/// (via `dv_used_next`/`dv_used_prev`), maintained for a future
/// `:garbagecollect` implementation - see this module's own doc
/// comment.
static GC_FIRST_DICT: GlobalCell<*mut DictT> = GlobalCell::new(std::ptr::null_mut());

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
fn fmt_g(value: f64) -> Vec<u8> {
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


/// Release a value's contents one level deep - not the original's
/// fully recursive `tv_clear` (a separate, substantial subsystem not
/// attempted here), just enough to correctly release whatever a
/// single `typval_T` itself directly owns/references. Used by
/// [`tv_dict_item_free`]/[`partial_free`]'s own `pt_argv` release, and
/// by [`partial_unref`] for `pt_dict`/`pt_func`.
///
/// # Safety
/// If `tv`'s value is `List`/`Dict`/`Blob`/`Partial`-typed with a
/// non-null pointer, that pointer must be valid (matching every other
/// function in this crate that touches those types).
unsafe fn tv_clear_simple(tv: &TypvalT) {
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

/// Free a partial, releasing everything it owns (`partial_free`,
/// `eval.c` - kept here alongside this module's other `tv_*_unref`/
/// `_free` functions since it's small, self-contained, and exactly
/// analogous in shape to [`tv_dict_free`]/[`tv_list_free`], even
/// though `partial_T`'s real home is `eval.c`, not `eval/typval.c`).
///
/// # Deferred
/// `pt_argv`'s items are released one level via [`tv_clear_simple`]
/// (matching this module's own established policy for container
/// contents - not the original's fully recursive `tv_clear`, which
/// itself is a separate, substantial `encode_vim_to_nothing`-based
/// subsystem, not attempted here).
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
    // SAFETY: forwarded from this function's own safety doc.
    let boxed = unsafe { Box::from_raw(pt) };
    for argv in &boxed.pt_argv {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(argv) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict_unref(boxed.pt_dict) };
    if boxed.pt_name.is_none() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::userfunc::func_ptr_unref(boxed.pt_func) };
    } else {
        crate::eval::userfunc::func_unref(boxed.pt_name.as_deref());
    }
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

/// Free items contained in a dictionary (`tv_dict_free_contents`).
///
/// # Safety
/// `d` must be a valid, non-null pointer to a live `DictT` whose every
/// item satisfies [`tv_dict_item_free`]'s own safety contract.
pub unsafe fn tv_dict_free_contents(d: *mut DictT) {
    // SAFETY: forwarded from this function's own safety doc.
    let dict = unsafe { &mut *d };
    // Unlike the original (which locks dv_hashtab, walks it via
    // HASHTAB_ITER + TV_DICT_HI2DI, and removes each item one at a
    // time), dv_index already gives a direct list of every live item -
    // no hashtab traversal/locking needed at all.
    let items: Vec<*mut DictitemT> = dict.dv_index.values().copied().collect();
    for item in items {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_dict_item_free(item) };
    }
    dict.dv_index.clear();
    dict.dv_hashtab = crate::hashtab_defs::HashtabT::hash_init();
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
/// # Safety
/// Same as [`tv_dict_free_contents`]/[`tv_dict_free_dict`] combined.
pub unsafe fn tv_dict_free(d: *mut DictT) {
    // The original's `tv_in_free_unref_items` re-entrancy guard is
    // always false here - nothing in this crate can trigger the
    // garbage-collector's "unreferencing everything" pass that sets it
    // (that pass doesn't exist yet).
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict_free_contents(d) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict_free_dict(d) };
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

/// `gc_first_list`: head of the linked list of all live lists (via
/// `lv_used_next`/`lv_used_prev`), maintained for a future
/// `:garbagecollect` implementation - same reasoning as
/// [`GC_FIRST_DICT`].
static GC_FIRST_LIST: GlobalCell<*mut crate::eval::typval_defs::ListT> =
    GlobalCell::new(std::ptr::null_mut());

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
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT` whose items
/// were all allocated via `tv_list_item_alloc`/`Box::into_raw`,
/// matching `tv_clear_simple`'s own safety contract for each item's
/// value.
pub unsafe fn tv_list_free_contents(l: *mut crate::eval::typval_defs::ListT) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut item = unsafe { (*l).lv_first };
    while !item.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let next = unsafe { (*item).li_next };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*l).lv_first = next };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(&(*item).li_tv) };
        // SAFETY: forwarded from this function's own safety doc.
        drop(unsafe { Box::from_raw(item) });
        item = next;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let l_ref = unsafe { &mut *l };
    l_ref.lv_len = 0;
    l_ref.lv_idx_item = std::ptr::null_mut();
    l_ref.lv_last = std::ptr::null_mut();
    debug_assert!(l_ref.lv_watch.is_null(), "tv_list_free_contents: lv_watch should be empty");
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
/// # Safety
/// Same as [`tv_list_free_contents`]/[`tv_list_free_list`] combined.
pub unsafe fn tv_list_free(l: *mut crate::eval::typval_defs::ListT) {
    // The original's `tv_in_free_unref_items` re-entrancy guard is
    // always false here - same reasoning as `tv_dict_free`.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_free_contents(l) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_free_list(l) };
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
