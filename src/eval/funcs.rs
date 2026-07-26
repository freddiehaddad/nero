//! Translated from `src/nvim/eval/funcs.c` (tractable core only).
//!
//! `funcs.c` (~7700 lines) implements EVERY builtin Vimscript function
//! (`len()`, `type()`, `split()`, `map()`, `sort()`, and roughly 640
//! more) plus the generated (`funcs.generated.h`) name/argument-count
//! dispatch table (`functions[]`) that `find_internal_func`/
//! `call_internal_func` search. Translating all of it is a separate,
//! massive undertaking of its own - this file starts that undertaking
//! with the dispatch mechanism itself plus a small handful of the
//! simplest, most foundational functions, to be extended incrementally
//! (matching how `eval.rs`'s own `eval0`-`eval7` chain grew).
//!
//! Translated: [`EvalFuncDefT`] (`EvalFuncDef`), [`find_internal_func`]/
//! [`call_internal_func`], and a growing set of the simplest builtins:
//! `len()`, `type()`, `empty()` - each already expressible in terms of
//! already-translated helpers ([`crate::eval::typval::tv_get_string`]/
//! `tv_list_len`/`tv_dict_len`/`tv_blob_len`) - plus a second batch:
//! the three bitwise functions `and()`/`or()`/`xor()`, `abs()`,
//! `max()`/`min()` (shared `max_min` helper, mirroring the original's
//! own structure exactly), the character/number conversions
//! `char2nr()`/`nr2char()`, and the string-to-number/float conversions
//! `str2nr()`/`str2float()` (the former, unusually, lives in
//! `strings.c` in the original rather than `funcs.c` itself - noted on
//! `f_str2nr`'s own doc comment) - plus a third batch: `str2list()`/
//! `list2str()` (character-codepoint `List` <-> `String` conversion;
//! `str2list` lives in `strings.c`, `list2str` in `eval/typval.c` -
//! neither in `funcs.c` itself, also noted on their own doc comments).
//! `list2str`/`str2list` needed a new [`crate::eval::typval::tv_list_alloc_ret`]
//! (allocate a list directly into a `rettv`, used pervasively - 83
//! call sites - by the real, untranslated `funcs.c`, so translated as
//! its own genuine function rather than inlined for this one caller).
//!
//! The real `functions[]` table is machine-generated from every
//! `f_*` function's own doc comment (`gen_eval.lua` scanning for
//! `@function`-tagged comments) into a perfect hash
//! (`find_internal_func_hash`); this crate uses a plain
//! `HashMap<&'static [u8], EvalFuncDefT>` instead - same data, a
//! different but functionally-identical lookup mechanism, matching
//! `option.rs`'s own `OPTION_HASH_ELEMS` precedent for exactly the
//! same "the original's own dispatch tree is a pure C performance
//! micro-optimization, not business logic" reasoning.
//!
//! `base_arg` (which argument position, if any, a function can be
//! called on via `expr->name()` method-call syntax) is stored for
//! future fidelity but not yet acted on - `call_internal_method`
//! (`handle_subscript`'s own "->name()" real caller) isn't translated
//! yet either.
//!
//! Also translated: a full cluster of `float_op_wrapper`-style pure-
//! float builtins - `sin()`/`cos()`/`tan()`/`asin()`/`acos()`/`atan()`/
//! `sinh()`/`cosh()`/`tanh()`/`sqrt()`/`exp()`/`log()`/`log10()`/
//! `floor()`/`ceil()`/`round()`/`trunc()` (single-argument), `atan2()`/
//! `pow()`/`fmod()` (two-argument), and `float2nr()`. The original
//! dispatches ALL of these through ONE shared C function
//! (`float_op_wrapper`), selected per-entry via an `EvalFuncData
//! .func_float` function pointer stored directly in the generated
//! `functions[]` table itself - contrary to this module's own earlier
//! doc comment (since corrected), this does NOT actually require
//! widening [`EvalFuncDefT`]/[`VimLFuncT`] at all: each `f_sin`/
//! `f_cos`/etc. below is instead its own tiny function that already
//! "bakes in" which specific `f64` method to call, delegating to a
//! shared private `float_op_wrapper`/`float_op2_wrapper` helper pair
//! that only need a plain function-pointer parameter - same data (a
//! name maps to a specific float transform), a different but
//! functionally-identical mechanism, matching this module's own
//! already-established `FUNCTIONS`-table-vs-perfect-hash precedent.
//! Needed a new [`crate::eval::typval::tv_get_float_chk`] (the
//! original's own `tv_get_float_chk`, an explicit-success/failure
//! sibling of the already-translated [`crate::eval::typval::tv_get_float`]).
//!
//! Also translated: `tolower()`/`toupper()` (via the already-existing
//! `crate::strings::strcase_save`) and `trim()` (leading/trailing
//! character trimming with an optional mask and direction, via a
//! by-hand backward-multibyte-walk using the already-existing
//! `crate::mbyte::utf_head_off`). `strcase_save`'s own trailing NUL
//! terminator (added for its own, differently-shaped C-string-flavored
//! callers) is stripped before building this crate's own `String`
//! typval. `utf_head_off` requires its own `base` argument to include
//! a trailing NUL byte - `f_trim` builds one once, up front, rather
//! than threading that requirement through its own established
//! "embedded NUL ends a C-string-modeled scan" byte-length-bounded
//! idiom used everywhere else in this module.
//!
//! Also translated: `has_key()` (via the already-existing
//! [`crate::eval::typval::tv_dict_has_key`]) and `keys()`/`values()`/
//! `items()` (a shared private `tv_dict2list` helper, mirroring the
//! original's own `tv_dict2list`/`DictListType` split exactly). Only
//! `items()`'s `Dict` case is translated - the original also accepts a
//! `String`/`List`/`Blob` via `tv_string2items`/`tv_list2items`/
//! `tv_blob2items`, none of which exist yet, so `f_items` declines
//! those cases explicitly. A dict item's own `di_key` always carries a
//! trailing NUL terminator (`DictitemT`'s own C-string-compatible
//! storage convention) that must be stripped before use as a
//! Vimscript `String` value - caught by a real test failure (asserting
//! the exact string CONTENTS, not just the list's own length) during
//! this function's own development, fixed using the same established
//! stripping idiom `eval/typval.rs`'s own `tv_dict_equal` already uses.
//!
//! Also translated: `get()` (a generic getter for `Blob`/`List`/
//! `Dict` with an optional default for a missing/out-of-range entry).
//! Only those three container types are handled - the original also
//! accepts a `Funcref`/`Partial` (returning `"func"`/`"name"`/`"dict"`/
//! `"args"`/`"arity"` introspection sub-fields via `get_func_arity`,
//! not yet translated), which `f_get` declines explicitly.
//!
//! Also translated: `index()` (the index of the first `Blob`/`List`
//! item equal to a given value, via the already-existing
//! [`crate::eval::typval::tv_equal`]).
//!
//! Also translated: `reverse()` (`Blob`/`List`/`String` in place),
//! needing a new [`crate::eval::typval::tv_blob_set`] (`eval/typval.h`'s
//! own `static inline` byte-store counterpart to the already-
//! translated `tv_blob_get`) and a new private `reverse_text` (a
//! UTF-8-aware, non-destructive character reversal using the already-
//! existing `crate::mbyte::utfc_ptr2len`).
//!
//! Also translated: `count()` (occurrences of a value in a `String`/
//! `List`/`Dict`, via three private helpers - `count_string`/
//! `count_list`/`count_dict` - mirroring the original's own identical
//! split exactly).
//!
//! Also translated: `copy()` (a shallow copy of `List`/`Dict`/`Blob`/
//! any other value, via [`crate::eval::typval::tv_list_copy`]/
//! [`crate::eval::typval::tv_dict_copy`]/[`crate::eval::typval::tv_blob_copy`]),
//! and `deepcopy()` (a RECURSIVE copy, via `eval.c`'s own
//! [`crate::eval::eval::var_item_copy`] plus real `deep=true` support
//! added to `tv_list_copy`/`tv_dict_copy` - cycle detection via an
//! optional `copy_id`/[`crate::eval::eval::get_copy_id`] is fully
//! real too, matching the original's own documented `{noref}`
//! semantics exactly: a container referenced more than once is only
//! copied once by default, or once per occurrence with
//! `deepcopy({expr}, 1)`).
//!
//! Also translated (from `eval/list.c`, not `funcs.c` itself):
//! `add()` (append one item to a `List`/`Blob`, returning the SAME
//! container) and `insert()` (insert one item before a given index,
//! or at the start by default, also returning the SAME container).
//! Also `remove()` (delete an item/range from a `List`/`Blob`, or a
//! key from a `Dict`), which needed 2 new `typval.rs` helpers of its
//! own: [`crate::eval::typval::tv_list_move_items`] (`eval/typval.c`'s
//! own function, moving a range of items from one list to the end of
//! another) and the whole `tv_list_remove`/`tv_blob_remove`/
//! `tv_dict_remove` trio (also `eval/typval.c`, one real function per
//! container type, exactly matching the original's own 3-way split).
//! Also `extend()`/`extendnew()` (merge one `List`/`Dict` into
//! another, in place or into a new copy), which needed a new
//! [`crate::eval::typval::tv_dict_extend`] (`eval/typval.c`) - its
//! `action="move"` case (moving items rather than copying) panics if
//! actually reached (needs a dict-item detach-without-free primitive
//! this crate doesn't have yet), but is provably unreachable from
//! `extend()`/`extendnew()` themselves (their own 3rd-argument
//! validation never allows `"move"` - the original's only `"move"`
//! caller is `window.c`'s scroll-event handling, not yet translated).
//!
//! Also `range()` (a `List` of numbers, optionally strided/counting
//! down).
//!
//! Also `repeat()` (repeat a `String`/`List`/`Blob` `{count}` times,
//! concatenated) - its `Blob` case builds the repeated buffer
//! directly via a plain `Vec<u8>` rather than the original's own
//! `ga_grow`+`tv_blob_set_range`-based approach (including a "skip
//! the copy if already all zero" micro-optimization not needed here).
//!
//! Also `join()` (from `eval/typval.c`, not `funcs.c` itself), which
//! needed a new [`crate::eval::typval::tv_list_join`] - fully real
//! for `Number`/`Float`/`String`/`Bool`/`Special` items (the
//! overwhelmingly common real-world case), but panics if an item is
//! `List`/`Dict`/`Blob`/`Funcref`/`Partial`-typed - stringifying THOSE
//! needs the full `encode_tv2echo` machinery (`eval/encode.c`, ~970
//! lines, a substantial separate undertaking not attempted here).
//!
//! Also `flatten()`/`flattennew()` (recursively replace each nested
//! `List` item, in place or into a new copy, up to an optional
//! `{maxdepth}`), which needed a new
//! [`crate::eval::typval::tv_list_flatten`] (`eval/typval.c`).
//!
//! Also `localtime()` (the current Unix timestamp, via the already-
//! existing [`crate::os::time::os_time`]).
//!
//! Also `getenv()` (via the already-existing
//! [`crate::os::env::vim_getenv`]) and `environ()` - the latter has NO
//! C implementation at all in the original (it's implemented in Lua,
//! `runtime/lua/vim/_core/vimfn.lua`'s own `M.f_environ`, calling
//! `vim.uv.os_environ()` and force-uppercasing keys on Windows only) -
//! translated directly from that Lua source using
//! `std::env::vars_os()` as the portable equivalent, matching this
//! whole mission's own scope (Neovim's Lua source, not just its C
//! source).
//!
//! Also `has({feature})` (`f_has`): checks `HAS_LIST_UNCONDITIONAL`
//! (the original's own `has_list[]` static array's platform-
//! independent entries - compile-time feature flags, not runtime
//! state) plus the handful of platform-conditional (`#ifdef`) entries
//! this crate can meaningfully determine at compile time. Every other
//! special case the original handles dynamically (`patch-N`/
//! `nvim-x.y.z` version checks, `vim_starting`/`ttyin`/`ttyout`/
//! `gui_running`/`syntax_items`/`wsl` runtime state, and provider-
//! based checks like `clipboard_working`) simply returns `0` - a
//! deliberate, justified scoping decision (not a "translate the
//! reachable path, panic on the rest" case like most other builtins):
//! `has()`'s own contract is "is this feature present", and "not
//! found" is a fully legitimate, non-crashing answer within that
//! contract, unlike e.g. `join()` where a specific argument type has
//! one well-defined correct stringification that must not be silently
//! wrong.

use crate::eval::typval_defs::{TypvalT, TypvalValue};
use crate::eval::userfunc::FnameTransError;

/// A builtin function can't be used as a method at all (`BASE_NONE`).
pub const BASE_NONE: u8 = 0;
/// A builtin function's LAST argument is the method base (`BASE_LAST`).
pub const BASE_LAST: u8 = u8::MAX;

/// A builtin Vimscript function's own implementation signature
/// (`VimLFunc`).
///
/// # Safety
/// If any `argvars[i].value`/`rettv.value` (on entry) is
/// `List`/`Dict`/`Blob`/`Partial`-typed with a non-null pointer, that
/// pointer must be valid, matching every other function in this crate
/// touching those types.
pub type VimLFuncT = unsafe fn(argvars: &[TypvalT], rettv: &mut TypvalT);

/// A builtin Vimscript function's own definition (`EvalFuncDef`).
#[derive(Clone, Copy)]
pub struct EvalFuncDefT {
    /// Minimal number of arguments (`min_argc`).
    pub min_argc: u8,
    /// Maximal number of arguments (`max_argc`).
    pub max_argc: u8,
    /// Method base arg # (1-indexed), [`BASE_NONE`] or [`BASE_LAST`]
    /// (`base_arg`).
    pub base_arg: u8,
    /// Function implementation (`func`).
    pub func: VimLFuncT,
}

/// Every translated builtin function name mapped to its own
/// definition (`functions[]`, mechanically a tiny subset of the real
/// ~641-entry table - see this module's own doc comment).
static FUNCTIONS: std::sync::LazyLock<crate::globals::GlobalCell<std::collections::HashMap<&'static [u8], EvalFuncDefT>>> =
    std::sync::LazyLock::new(|| {
        let mut m = std::collections::HashMap::new();
        m.insert(&b"len"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_len });
        m.insert(&b"type"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_type });
        m.insert(&b"empty"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_empty });
        m.insert(&b"and"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_and });
        m.insert(&b"or"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_or });
        m.insert(&b"xor"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_xor });
        m.insert(&b"abs"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_abs });
        m.insert(&b"max"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_max });
        m.insert(&b"min"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_min });
        m.insert(&b"char2nr"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_char2nr });
        m.insert(&b"nr2char"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_nr2char });
        m.insert(&b"str2float"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_str2float });
        m.insert(&b"str2nr"[..], EvalFuncDefT { min_argc: 1, max_argc: 3, base_arg: 1, func: f_str2nr });
        m.insert(&b"str2list"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_str2list });
        m.insert(&b"list2str"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_list2str });
        m.insert(&b"sin"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_sin });
        m.insert(&b"cos"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_cos });
        m.insert(&b"tan"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_tan });
        m.insert(&b"asin"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_asin });
        m.insert(&b"acos"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_acos });
        m.insert(&b"atan"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_atan });
        m.insert(&b"sinh"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_sinh });
        m.insert(&b"cosh"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_cosh });
        m.insert(&b"tanh"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_tanh });
        m.insert(&b"sqrt"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_sqrt });
        m.insert(&b"exp"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_exp });
        m.insert(&b"log"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_log });
        m.insert(&b"log10"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_log10 });
        m.insert(&b"floor"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_floor });
        m.insert(&b"ceil"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_ceil });
        m.insert(&b"round"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_round });
        m.insert(&b"trunc"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_trunc });
        m.insert(&b"atan2"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_atan2 });
        m.insert(&b"pow"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_pow });
        m.insert(&b"fmod"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_fmod });
        m.insert(&b"float2nr"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_float2nr });
        m.insert(&b"tolower"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_tolower });
        m.insert(&b"toupper"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_toupper });
        m.insert(&b"trim"[..], EvalFuncDefT { min_argc: 1, max_argc: 3, base_arg: 1, func: f_trim });
        m.insert(&b"has_key"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_has_key });
        m.insert(&b"keys"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_keys });
        m.insert(&b"values"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_values });
        m.insert(&b"items"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_items });
        m.insert(&b"get"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_get });
        m.insert(&b"index"[..], EvalFuncDefT { min_argc: 2, max_argc: 4, base_arg: 1, func: f_index });
        m.insert(&b"reverse"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_reverse });
        m.insert(&b"count"[..], EvalFuncDefT { min_argc: 2, max_argc: 4, base_arg: 1, func: f_count });
        m.insert(&b"copy"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_copy });
        m.insert(&b"deepcopy"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_deepcopy });
        m.insert(&b"add"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_add });
        m.insert(&b"insert"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_insert });
        m.insert(&b"remove"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_remove });
        m.insert(&b"extend"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_extend });
        m.insert(&b"extendnew"[..], EvalFuncDefT { min_argc: 2, max_argc: 3, base_arg: 1, func: f_extendnew });
        m.insert(&b"range"[..], EvalFuncDefT { min_argc: 1, max_argc: 3, base_arg: 1, func: f_range });
        m.insert(&b"repeat"[..], EvalFuncDefT { min_argc: 2, max_argc: 2, base_arg: 1, func: f_repeat });
        m.insert(&b"join"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_join });
        m.insert(&b"flatten"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_flatten });
        m.insert(&b"flattennew"[..], EvalFuncDefT { min_argc: 1, max_argc: 2, base_arg: 1, func: f_flattennew });
        m.insert(&b"localtime"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_localtime });
        m.insert(&b"getenv"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_getenv });
        m.insert(&b"environ"[..], EvalFuncDefT { min_argc: 0, max_argc: 0, base_arg: BASE_NONE, func: f_environ });
        m.insert(&b"has"[..], EvalFuncDefT { min_argc: 1, max_argc: 1, base_arg: 1, func: f_has });
        crate::globals::GlobalCell::new(m)
    });

/// Find a builtin function's own definition by name (`find_internal_func`).
#[must_use]
pub fn find_internal_func(name: &[u8]) -> Option<EvalFuncDefT> {
    // SAFETY: no overlapping live access - see this crate's established
    // GlobalCell::get_mut convention.
    unsafe { FUNCTIONS.get_mut() }.get(name).copied()
}

/// Call a builtin function by name (`call_internal_func`).
///
/// # Safety
/// Forwarded from the dispatched-to [`VimLFuncT`]'s own safety doc.
#[must_use]
pub unsafe fn call_internal_func(fname: &[u8], argvars: &[TypvalT], rettv: &mut TypvalT) -> FnameTransError {
    let Some(fdef) = find_internal_func(fname) else {
        return FnameTransError::Unknown;
    };
    if argvars.len() < fdef.min_argc as usize {
        return FnameTransError::TooFew;
    }
    if argvars.len() > fdef.max_argc as usize {
        return FnameTransError::TooMany;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (fdef.func)(argvars, rettv) };
    FnameTransError::None
}

/// `len({expr})` - length of a string/number/`Blob`/`List`/`Dict`
/// (`f_len`).
///
/// The original's `E701: Invalid type for len()` error (for a
/// `Float`/`Bool`/`Special`/`Partial`/`Func`/`Unknown` argument) is
/// omitted - message display, not tractable; `rettv` is simply left
/// untouched (still its caller's own default-initialized `Number(0)`),
/// matching this crate's established "skip the display, keep an
/// otherwise-harmless default" policy for similar leaf-level gaps.
///
/// # Safety
/// If `argvars[0].value` is `List`/`Dict`/`Blob`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_len(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(match &argvars[0].value {
        TypvalValue::String(_) | TypvalValue::Number(_) => {
            crate::eval::typval::tv_get_string(&argvars[0]).len() as crate::eval::typval_defs::VarnumberT
        }
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::Blob(b) => crate::eval::typval_defs::VarnumberT::from(unsafe { crate::eval::typval::tv_blob_len(*b) }),
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::List(l) => crate::eval::typval_defs::VarnumberT::from(unsafe { crate::eval::typval::tv_list_len(*l) }),
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::Dict(d) => {
            crate::eval::typval_defs::VarnumberT::from(crate::eval::typval::tv_dict_len(unsafe { d.as_ref() }))
        }
        _ => return,
    });
}

/// `type({expr})` - a number identifying `{expr}`'s own type
/// (`f_type`). The returned numbers are fixed, documented,
/// externally-observable values (`v:t_*` constants), not internal
/// implementation details.
fn f_type(argvars: &[TypvalT], rettv: &mut TypvalT) {
    use crate::eval::typval_defs::var_type_result;
    let n = match &argvars[0].value {
        TypvalValue::Number(_) => var_type_result::NUMBER,
        TypvalValue::String(_) => var_type_result::STRING,
        TypvalValue::Partial(_) | TypvalValue::Func(_) => var_type_result::FUNC,
        TypvalValue::List(_) => var_type_result::LIST,
        TypvalValue::Dict(_) => var_type_result::DICT,
        TypvalValue::Float(_) => var_type_result::FLOAT,
        TypvalValue::Bool(_) => var_type_result::BOOL,
        TypvalValue::Special(_) => var_type_result::SPECIAL,
        TypvalValue::Blob(_) => var_type_result::BLOB,
        TypvalValue::Unknown => {
            debug_assert!(false, "f_type(UNKNOWN) - internal_error in the original");
            -1
        }
    };
    rettv.value = TypvalValue::Number(crate::eval::typval_defs::VarnumberT::from(n));
}

/// `empty({expr})` - whether `{expr}` is "empty" (`f_empty`).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Dict`/`Partial`-typed with a
/// non-null pointer, that pointer must be valid.
unsafe fn f_empty(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let n = match &argvars[0].value {
        TypvalValue::String(s) => s.as_deref().is_none_or(<[u8]>::is_empty),
        TypvalValue::Func(s) => s.as_deref().is_none_or(<[u8]>::is_empty),
        TypvalValue::Partial(p) => p.is_null(),
        TypvalValue::Number(n) => *n == 0,
        TypvalValue::Float(f) => *f == 0.0,
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::List(l) => (unsafe { crate::eval::typval::tv_list_len(*l) }) == 0,
        TypvalValue::Dict(d) => crate::eval::typval::tv_dict_len(unsafe { d.as_ref() }) == 0,
        TypvalValue::Bool(b) => *b == crate::eval::typval_defs::BoolVarValue::False,
        // v:null is the only real SpecialVarValue variant in this
        // crate (v:true/v:false moved to the separate BoolVarValue
        // type upstream too) - always "empty", matching the original's
        // own `n = argvars[0].vval.v_special == kSpecialVarNull`.
        TypvalValue::Special(s) => *s == crate::eval::typval_defs::SpecialVarValue::Null,
        TypvalValue::Blob(b) => {
            // SAFETY: forwarded from this function's own safety doc.
            (unsafe { crate::eval::typval::tv_blob_len(*b) }) == 0
        }
        TypvalValue::Unknown => {
            debug_assert!(false, "f_empty(UNKNOWN) - internal_error in the original");
            true
        }
    };
    // Unlike a "real" Vimscript Bool, empty() has always returned a
    // plain Number (0/1) for historical compatibility - it relies on
    // call_func's own "default rettv is number zero" pre-initialization
    // in the original (only ever overwriting `.vval.v_number`, never
    // touching `.v_type`); this crate's own TypvalValue enum has no
    // such split-tag/union hazard, so this is just a direct, explicit
    // Number assignment instead.
    rettv.value = TypvalValue::Number(crate::eval::typval_defs::VarnumberT::from(n));
}

/// `and({expr}, {expr})` - bitwise AND of two numbers (`f_and`).
fn f_and(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let a = crate::eval::typval::tv_get_number_chk(&argvars[0], None);
    let b = crate::eval::typval::tv_get_number_chk(&argvars[1], None);
    rettv.value = TypvalValue::Number(a & b);
}

/// `or({expr}, {expr})` - bitwise OR of two numbers (`f_or`).
fn f_or(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let a = crate::eval::typval::tv_get_number_chk(&argvars[0], None);
    let b = crate::eval::typval::tv_get_number_chk(&argvars[1], None);
    rettv.value = TypvalValue::Number(a | b);
}

/// `xor({expr}, {expr})` - bitwise XOR of two numbers (`f_xor`).
fn f_xor(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let a = crate::eval::typval::tv_get_number_chk(&argvars[0], None);
    let b = crate::eval::typval::tv_get_number_chk(&argvars[1], None);
    rettv.value = TypvalValue::Number(a ^ b);
}

/// `abs({expr})` - absolute value of a Number or Float (`f_abs`).
///
/// The original's own `Float` branch goes through `float_op_wrapper`/
/// `fabs` - not needed here (matching `TypvalValue::Float`'s own
/// payload directly and calling `f64::abs` is identical, and this
/// crate's `VimLFuncT` has no function-pointer-passing equivalent of
/// `EvalFuncData` yet anyway, see this module's own doc comment).
///
/// The non-`Float` branch uses `wrapping_neg` rather than a bare
/// negation/`i64::abs`: the original's own `n > 0 ? n : -n` has no
/// special case for `n == VARNUMBER_MIN` (unlike e.g. `num_divide`,
/// which explicitly special-cases the analogous `VARNUMBER_MIN / -1`
/// overflow) - `-VARNUMBER_MIN` is real, if obscure, signed-overflow
/// UB in the original C too. `wrapping_neg` reproduces the same
/// wrap-back-to-`VARNUMBER_MIN` result standard two's-complement
/// hardware actually produces for this, rather than introducing a NEW
/// panic (a bare `-n`/`n.abs()` would panic in a debug build) the
/// original never has.
fn f_abs(argvars: &[TypvalT], rettv: &mut TypvalT) {
    if let TypvalValue::Float(f) = argvars[0].value {
        rettv.value = TypvalValue::Float(f.abs());
        return;
    }
    let mut error = false;
    let n = crate::eval::typval::tv_get_number_chk(&argvars[0], Some(&mut error));
    rettv.value = TypvalValue::Number(if error {
        -1
    } else if n > 0 {
        n
    } else {
        n.wrapping_neg()
    });
}

/// Get the maximal/minimal Number value in a `List` or `Dict`
/// (`max_min`). Only assigns `rettv.value`'s own Number payload -
/// returns `0` for an empty `List`/`Dict`. A non-`List`/`Dict` `tv`
/// (the original's own `E712: Argument of max()/min() must be a List
/// or Dictionary` case) leaves `rettv` untouched, matching this
/// crate's established "skip the display, keep the caller's own
/// default" policy - as does a `List`/`Dict` item that isn't
/// Number-shaped (the original's own `errmsg already given` case).
///
/// # Safety
/// If `tv.value` is `List`/`Dict`-typed with a non-null pointer, that
/// pointer must be valid.
unsafe fn max_min(tv: &TypvalT, rettv: &mut TypvalT, domax: bool) {
    use crate::eval::typval_defs::{VARNUMBER_MAX, VARNUMBER_MIN};

    let mut n = if domax { VARNUMBER_MIN } else { VARNUMBER_MAX };
    match &tv.value {
        TypvalValue::List(l) => {
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { crate::eval::typval::tv_list_len(*l) } == 0 {
                return;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let mut item = unsafe { crate::eval::typval::tv_list_first(*l) };
            while !item.is_null() {
                let mut error = false;
                // SAFETY: `item` is a live node reached by walking the
                // list just validated above.
                let i = crate::eval::typval::tv_get_number_chk(unsafe { &(*item).li_tv }, Some(&mut error));
                if error {
                    return;
                }
                if if domax { i > n } else { i < n } {
                    n = i;
                }
                // SAFETY: forwarded from this function's own safety doc.
                item = unsafe { (*item).li_next };
            }
        }
        TypvalValue::Dict(d) => {
            // SAFETY: forwarded from this function's own safety doc.
            if crate::eval::typval::tv_dict_len(unsafe { d.as_ref() }) == 0 {
                return;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let items: Vec<*mut crate::eval::typval_defs::DictitemT> = unsafe { &**d }.dv_index.values().copied().collect();
            for item in items {
                let mut error = false;
                // SAFETY: `item` came from the dict's own live index above.
                let i = crate::eval::typval::tv_get_number_chk(unsafe { &(*item).di_tv }, Some(&mut error));
                if error {
                    return;
                }
                if if domax { i > n } else { i < n } {
                    n = i;
                }
            }
        }
        _ => return,
    }
    rettv.value = TypvalValue::Number(n);
}

/// `max({expr})` - maximal Number value in a `List` or `Dict` (`f_max`).
///
/// # Safety
/// Forwarded from [`max_min`]'s own safety doc.
unsafe fn f_max(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { max_min(&argvars[0], rettv, true) };
}

/// `min({expr})` - minimal Number value in a `List` or `Dict` (`f_min`).
///
/// # Safety
/// Forwarded from [`max_min`]'s own safety doc.
unsafe fn f_min(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { max_min(&argvars[0], rettv, false) };
}

/// `char2nr({expr} [, {utf8}])` - the character value of the first
/// character in `{expr}` (`f_char2nr`).
///
/// The optional second argument (`{utf8}`) is validated (must be
/// Number-shaped) but never actually consulted for behavior, matching
/// the original exactly: nvim always decodes UTF-8 internally
/// regardless of this argument's value, so it exists only for
/// external-compatibility argument-shape validation. Unlike the
/// original's own `argvars[1].v_type != VAR_UNKNOWN` sentinel check
/// (a fixed-size array slot left at its default `VAR_UNKNOWN` when a
/// trailing optional argument is omitted), `argvars` here is already
/// exactly as long as the number of arguments the caller actually
/// passed ([`call_internal_func`] already validated it's within
/// `min_argc..=max_argc`) - so "was the 2nd argument passed" is simply
/// `argvars.len() > 1`. Every other function here with a trailing
/// optional argument follows this same substitution.
///
/// A genuinely empty `{expr}` is handled explicitly
/// (`crate::mbyte::utf_ptr2char` panics on an empty slice, relying on
/// the original's own C-string NUL terminator as an implicit "first
/// byte" to decode instead - this crate's `tv_get_string` returns a
/// truly empty `Vec<u8>` for an empty Vimscript string with no such
/// terminator to fall back on; matches `path.rs`'s own
/// `utf_ptr2char_or_nul` precedent for the identical hazard).
fn f_char2nr(argvars: &[TypvalT], rettv: &mut TypvalT) {
    if argvars.len() > 1 && !crate::eval::typval::tv_check_num(&argvars[1]) {
        return;
    }
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let n = if s.is_empty() { 0 } else { crate::mbyte::utf_ptr2char(&s) };
    rettv.value = TypvalValue::Number(crate::eval::typval_defs::VarnumberT::from(n));
}

/// `nr2char({expr} [, {utf8}])` - a string holding the single
/// character whose codepoint is `{expr}` (`f_nr2char`). See
/// [`f_char2nr`]'s own doc comment for why the optional second
/// argument only validates its own shape without otherwise affecting
/// behavior, and why `argvars.len() > 1` replaces the original's
/// `argvars[1].v_type != VAR_UNKNOWN` sentinel check.
///
/// The original's own `E5070`/`E5071` out-of-range messages are
/// omitted (message display, not tractable) - `rettv` is simply left
/// untouched on either error, matching this crate's established
/// "skip the display, keep an otherwise-harmless default" policy.
fn f_nr2char(argvars: &[TypvalT], rettv: &mut TypvalT) {
    if argvars.len() > 1 && !crate::eval::typval::tv_check_num(&argvars[1]) {
        return;
    }
    let mut error = false;
    let num = crate::eval::typval::tv_get_number_chk(&argvars[0], Some(&mut error));
    if error || num < 0 || num > i64::from(i32::MAX) {
        return;
    }
    let mut buf = [0u8; crate::mbyte_defs::MB_MAXCHAR];
    let len = crate::mbyte::utf_char2bytes(num as i32, &mut buf);
    rettv.value = TypvalValue::String(Some(buf[..len as usize].to_vec()));
}

/// `str2float({expr})` - convert `{expr}` to a Float (`f_str2float`).
///
/// Hex-float syntax (e.g. `"0x1.8p3"`) inherits
/// [`crate::eval::eval::string2float`]'s own documented
/// `unimplemented!()` for that input shape. Unlike `eval_number`'s own
/// pre-validated caller (which can never actually reach that path),
/// an arbitrary user string passed to `str2float()` genuinely could
/// start with `"0x"` - a real, if narrow, reachability gap inherited
/// from `string2float` itself, not a new one introduced here.
fn f_str2float(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let skip = crate::charset::skipwhite(&s);
    let mut p = &s[skip..];
    let isneg = p.first() == Some(&b'-');
    if matches!(p.first(), Some(&b'+') | Some(&b'-')) {
        let skip2 = crate::charset::skipwhite(&p[1..]);
        p = &p[1 + skip2..];
    }
    let (mut value, _consumed) = crate::eval::eval::string2float(p);
    if isneg {
        value *= -1.0;
    }
    rettv.value = TypvalValue::Float(value);
}

/// `str2nr({string} [, {base} [, {quoted}]])` - convert `{string}` to
/// a Number in a given base (`f_str2nr`).
///
/// Unusually, the original keeps this function (and `str2list()`) in
/// `strings.c` rather than `funcs.c` itself - noted here since every
/// other function in this module maps to `funcs.c`.
///
/// The original's own `E475: Invalid argument` message (for an
/// invalid `{base}`) is omitted, matching this crate's established
/// "skip the display, keep an otherwise-harmless default" policy -
/// `rettv` is simply left untouched. See [`f_char2nr`]'s own doc
/// comment for why `argvars.len() > N` replaces the original's
/// `argvars[N].v_type != VAR_UNKNOWN` sentinel checks for the two
/// trailing optional arguments here.
fn f_str2nr(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let mut base = 10;
    let mut what = 0;
    if argvars.len() > 1 {
        base = crate::eval::typval::tv_get_number(&argvars[1]);
        if base != 2 && base != 8 && base != 10 && base != 16 {
            return;
        }
        if argvars.len() > 2 && crate::eval::typval::tv_get_bool(&argvars[2]) != 0 {
            what |= crate::charset::STR2NR_QUOTE;
        }
    }

    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let skip = crate::charset::skipwhite(&s);
    let mut p = &s[skip..];
    let isneg = p.first() == Some(&b'-');
    if matches!(p.first(), Some(&b'+') | Some(&b'-')) {
        let skip2 = crate::charset::skipwhite(&p[1..]);
        p = &p[1 + skip2..];
    }
    match base {
        2 => what |= crate::charset::STR2NR_BIN | crate::charset::STR2NR_FORCE,
        8 => what |= crate::charset::STR2NR_OCT | crate::charset::STR2NR_OOCT | crate::charset::STR2NR_FORCE,
        16 => what |= crate::charset::STR2NR_HEX | crate::charset::STR2NR_FORCE,
        _ => {}
    }
    let mut n = 0;
    crate::charset::vim_str2nr(p, None, None, what, Some(&mut n), None, 0, false, None);
    // Text after the number is silently ignored.
    rettv.value = TypvalValue::Number(if isneg { -n } else { n });
}

/// `str2list({string} [, {utf8}])` - convert `{string}` into a `List`
/// of character codepoints (`f_str2list`). Unusually lives in
/// `strings.c` in the original, like [`f_str2nr`] (see its own doc
/// comment).
///
/// A NUL byte inside `{string}` ends the conversion early, matching
/// the original's own `for (; *p != NUL; ...)` loop exactly - not a
/// shortcut: `tv_get_string` can return a `Vec<u8>` containing an
/// embedded NUL (unlike the original's own NUL-terminated `char *`,
/// which could never represent one to begin with), so this crate's
/// translation must check for it explicitly to reproduce the
/// identical stopping behavior (and to avoid an infinite loop:
/// `utf_ptr2len` itself returns `0` for a NUL byte, matching how the
/// original's loop condition would never even call it on one).
///
/// # Safety
/// `rettv`'s own value must not (yet) hold a `List`/`Dict`/`Blob`/
/// `Partial` pointer that anything else still depends on - this
/// function unconditionally overwrites it.
unsafe fn f_str2list(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let l = unsafe {
        crate::eval::typval::tv_list_alloc_ret(rettv, crate::eval::typval_defs::ListLenSpecials::Unknown as isize)
    };
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let mut pos = 0;
    while pos < s.len() && s[pos] != 0 {
        let c = crate::mbyte::utf_ptr2char(&s[pos..]);
        // SAFETY: `l` was just allocated above by this same function.
        unsafe { crate::eval::typval::tv_list_append_number(l, i64::from(c)) };
        pos += crate::mbyte::utf_ptr2len(&s[pos..]) as usize;
    }
}

/// `list2str({list} [, {utf8}])` - convert a `List` of character
/// codepoints into a `String` (`f_list2str`). Unusually lives in
/// `eval/typval.c` in the original, neither `strings.c` nor
/// `funcs.c` (see [`f_str2nr`]'s own doc comment for the general
/// note).
///
/// The original's own `E475: Invalid argument` (a non-`List` argument)
/// is omitted, matching this crate's established "skip the display,
/// keep an otherwise-harmless default" policy - `rettv` is simply left
/// as an empty `String`, matching the original's own pre-set
/// `rettv->vval.v_string = NULL` default for this exact error path
/// too.
///
/// # Safety
/// If `argvars[0].value` is `List`-typed with a non-null pointer, that
/// pointer must be valid.
unsafe fn f_list2str(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::String(None);
    let TypvalValue::List(l) = &argvars[0].value else {
        return;
    };
    let l = *l;
    if l.is_null() {
        return; // empty list results in empty string
    }

    let mut result = Vec::new();
    let mut buf = [0u8; crate::mbyte_defs::MB_MAXBYTES];
    // SAFETY: forwarded from this function's own safety doc.
    let mut item = unsafe { crate::eval::typval::tv_list_first(l) };
    while !item.is_null() {
        // SAFETY: `item` is a live node reached by walking the list
        // just validated above.
        let n = crate::eval::typval::tv_get_number(unsafe { &(*item).li_tv });
        let buflen = crate::mbyte::utf_char2bytes(n as i32, &mut buf);
        result.extend_from_slice(&buf[..buflen as usize]);
        // SAFETY: forwarded from this function's own safety doc.
        item = unsafe { (*item).li_next };
    }
    rettv.value = TypvalValue::String(Some(result));
}

/// Apply a single-argument `f64` math function to `argvars[0]`,
/// storing the result as a `Float` (`float_op_wrapper`).
///
/// The original dispatches EVERY one of `sin()`/`cos()`/`sqrt()`/etc.
/// through this ONE shared C function, selected via a `func_float`
/// function pointer stored in each entry of the generated `functions[]`
/// table itself (`EvalFuncData.func_float`). This crate's own
/// `VimLFuncT` has no such extra per-entry payload - instead, each
/// `f_sin`/`f_cos`/etc. below is its own tiny function that already
/// "bakes in" which `f64` method to call, so `float_op_wrapper` only
/// needs `argvars`/`rettv` plus a plain function pointer parameter,
/// not a whole widened table-entry shape. Same data (a name maps to a
/// specific one-argument float transform), a different but
/// functionally-identical mechanism - matching this module's own
/// established `FUNCTIONS`-table-vs-perfect-hash precedent.
///
/// Returns `0.0` if `argvars[0]` isn't Number/Float-shaped, matching
/// the original's own fallback exactly (its own `E808` message is
/// omitted, see `tv_get_float_chk`'s own doc comment).
fn float_op_wrapper(argvars: &[TypvalT], rettv: &mut TypvalT, f: fn(f64) -> f64) {
    let result = crate::eval::typval::tv_get_float_chk(&argvars[0]).map_or(0.0, f);
    rettv.value = TypvalValue::Float(result);
}

/// Apply a two-argument `f64` math function to `argvars[0]`/
/// `argvars[1]`, storing the result as a `Float` - shared by
/// `atan2()`/`pow()`/`fmod()`, each of which hand-expands the
/// equivalent of [`float_op_wrapper`] inline in the original (there is
/// no separate, named 2-argument sibling of `float_op_wrapper` in the
/// original itself).
fn float_op2_wrapper(argvars: &[TypvalT], rettv: &mut TypvalT, f: fn(f64, f64) -> f64) {
    use crate::eval::typval::tv_get_float_chk;
    let result = match (tv_get_float_chk(&argvars[0]), tv_get_float_chk(&argvars[1])) {
        (Some(fx), Some(fy)) => f(fx, fy),
        _ => 0.0,
    };
    rettv.value = TypvalValue::Float(result);
}

/// `sin({expr})` (`f_sin`).
fn f_sin(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::sin);
}

/// `cos({expr})` (`f_cos`).
fn f_cos(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::cos);
}

/// `tan({expr})` (`f_tan`).
fn f_tan(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::tan);
}

/// `asin({expr})` (`f_asin`).
fn f_asin(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::asin);
}

/// `acos({expr})` (`f_acos`).
fn f_acos(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::acos);
}

/// `atan({expr})` (`f_atan`).
fn f_atan(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::atan);
}

/// `sinh({expr})` (`f_sinh`).
fn f_sinh(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::sinh);
}

/// `cosh({expr})` (`f_cosh`).
fn f_cosh(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::cosh);
}

/// `tanh({expr})` (`f_tanh`).
fn f_tanh(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::tanh);
}

/// `sqrt({expr})` (`f_sqrt`).
fn f_sqrt(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::sqrt);
}

/// `exp({expr})` (`f_exp`).
fn f_exp(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::exp);
}

/// `log({expr})` - natural logarithm (`f_log`).
fn f_log(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::ln);
}

/// `log10({expr})` - base-10 logarithm (`f_log10`).
fn f_log10(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::log10);
}

/// `floor({expr})` (`f_floor`).
fn f_floor(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::floor);
}

/// `ceil({expr})` (`f_ceil`).
fn f_ceil(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::ceil);
}

/// `round({expr})` - round half away from zero (`f_round`). Rust's own
/// `f64::round` uses the identical "round half away from zero" rule as
/// the original's own C `round()`, unlike `f64::round_ties_even`.
fn f_round(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::round);
}

/// `trunc({expr})` (`f_trunc`).
fn f_trunc(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op_wrapper(argvars, rettv, f64::trunc);
}

/// `atan2({expr1}, {expr2})` (`f_atan2`).
fn f_atan2(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op2_wrapper(argvars, rettv, f64::atan2);
}

/// `pow({x}, {y})` (`f_pow`).
fn f_pow(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op2_wrapper(argvars, rettv, f64::powf);
}

/// `fmod({expr1}, {expr2})` (`f_fmod`). Rust's own `%` operator on
/// `f64` is specified to match C's `fmod()` exactly (truncating
/// division's remainder, sign matching the dividend) - NOT the
/// distinct, round-to-nearest-even IEEE 754 `remainder()` operation
/// (`f64::rem_euclid`'s own always-nonnegative result is a third,
/// again-different operation) - verified directly with a dedicated
/// test using a negative dividend, the one input shape where all
/// three would disagree.
fn f_fmod(argvars: &[TypvalT], rettv: &mut TypvalT) {
    float_op2_wrapper(argvars, rettv, |a, b| a % b);
}

/// `float2nr({expr})` - convert a Float to a Number, clamping to the
/// representable `Number` range rather than following the original's
/// own `(varnumber_T)f` cast for a wildly out-of-range Float, which is
/// real (if effectively unreachable given the explicit clamp checks
/// immediately before it) signed-overflow UB in C (`f_float2nr`).
///
/// A `NaN` input reaches the final `f as VarnumberT` branch (both
/// clamp comparisons are false for `NaN`, matching the original's own
/// identical fallthrough) - Rust's own `as` cast saturates `NaN` to
/// `0` (well-defined, unlike the original's own UB for this exact
/// input), which this translation keeps rather than inventing new
/// behavior to paper over a case the original itself never defines.
fn f_float2nr(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let Some(f) = crate::eval::typval::tv_get_float_chk(&argvars[0]) else {
        return;
    };
    let max = crate::eval::typval_defs::VARNUMBER_MAX as f64;
    let n = if f <= -max + f64::EPSILON {
        -crate::eval::typval_defs::VARNUMBER_MAX
    } else if f >= max - f64::EPSILON {
        crate::eval::typval_defs::VARNUMBER_MAX
    } else {
        f as crate::eval::typval_defs::VarnumberT
    };
    rettv.value = TypvalValue::Number(n);
}

/// `tolower({string})` - convert to lowercase (`f_tolower`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (forwarded from
/// `crate::strings::strcase_save`'s own safety doc).
unsafe fn f_tolower(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    // SAFETY: forwarded from this function's own safety doc.
    let mut result = unsafe { crate::strings::strcase_save(&s, false) };
    result.pop(); // strip strcase_save's own trailing NUL terminator.
    rettv.value = TypvalValue::String(Some(result));
}

/// `toupper({string})` - convert to uppercase (`f_toupper`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (forwarded from
/// `crate::strings::strcase_save`'s own safety doc).
unsafe fn f_toupper(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    // SAFETY: forwarded from this function's own safety doc.
    let mut result = unsafe { crate::strings::strcase_save(&s, true) };
    result.pop();
    rettv.value = TypvalValue::String(Some(result));
}

/// `trim({text} [, {mask} [, {dir}]])` - trim characters from the
/// start and/or end of `{text}` (`f_trim`).
///
/// `{mask}` defaults to trimming ASCII whitespace plus the
/// non-breaking space U+00A0 (`c > ' ' && c != 0xa0` in the original -
/// "not trim-worthy" exactly when a character is neither ASCII
/// whitespace nor NBSP, matching Vim's own documented default).
/// `{dir}`: `0` (default) trims both ends, `1` leading only, `2`
/// trailing only - like the original, only ever read when `{mask}`
/// was ACTUALLY passed as a real `String` (an omitted `{mask}` means
/// `{dir}` is never consulted even if somehow present, exactly
/// mirroring the original's own nested
/// `if (argvars[1].v_type == VAR_STRING) { ... if (argvars[2]...) }`
/// structure).
///
/// A NUL byte inside `{text}`/`{mask}` ends each string early,
/// matching the original's own NUL-terminated `strlen`/`*head != NUL`
/// loop conditions exactly - the same established "embedded NUL ends
/// a C-string-modeled scan" translation `f_str2list`'s own doc comment
/// explains in more detail.
///
/// The original's own `E475: Invalid argument` (`{dir}` out of the
/// `0..=2` range) is omitted, matching this crate's established "skip
/// the display, keep an otherwise-harmless default" policy - `rettv`
/// is simply left as an empty `String`, matching the original's own
/// pre-set `rettv->vval.v_string = NULL` default for this exact error
/// path too.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (forwarded from
/// `crate::mbyte::utf_head_off`'s own safety doc).
unsafe fn f_trim(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::String(None);

    let s = crate::eval::typval::tv_get_string(&argvars[0]);
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());

    if argvars.len() > 1 && !matches!(argvars[1].value, TypvalValue::String(_)) {
        return;
    }

    let mut mask: Option<Vec<u8>> = None;
    let mut dir: crate::eval::typval_defs::VarnumberT = 0;
    if argvars.len() > 1 {
        let m = crate::eval::typval::tv_get_string(&argvars[1]);
        mask = if m.first() == Some(&0) || m.is_empty() { None } else { Some(m) };

        if argvars.len() > 2 {
            let mut error = false;
            dir = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error));
            if error || !(0..=2).contains(&dir) {
                return;
            }
        }
    }

    let is_trim_worthy = |c: i32, mask: &Option<Vec<u8>>| -> bool {
        match mask {
            None => !(c > i32::from(b' ') && c != 0xa0),
            Some(m) => {
                let mend = m.iter().position(|&b| b == 0).unwrap_or(m.len());
                let mut p = 0;
                while p < mend {
                    if crate::mbyte::utf_ptr2char(&m[p..mend]) == c {
                        return true;
                    }
                    p += crate::mbyte::utf_ptr2len(&m[p..mend]) as usize;
                }
                false
            }
        }
    };

    let mut head = 0usize;
    if dir == 0 || dir == 1 {
        while head < end {
            let c1 = crate::mbyte::utf_ptr2char(&s[head..end]);
            if !is_trim_worthy(c1, &mask) {
                break;
            }
            head += crate::mbyte::utf_ptr2len(&s[head..end]) as usize;
        }
    }

    // utf_head_off requires its own `base` to include a trailing NUL
    // byte - build one once, up front, for the trailing-trim walk
    // below (the only place this function needs it).
    let mut nul_terminated = s[..end].to_vec();
    nul_terminated.push(0);

    let mut tail = end;
    if dir == 0 || dir == 2 {
        while tail > head {
            let mut prev = tail - 1;
            // SAFETY: forwarded from this function's own safety doc.
            let head_off = unsafe { crate::mbyte::utf_head_off(&nul_terminated, prev) };
            prev -= head_off as usize;
            let c1 = crate::mbyte::utf_ptr2char(&s[prev..end]);
            if !is_trim_worthy(c1, &mask) {
                break;
            }
            tail = prev;
        }
    }

    rettv.value = TypvalValue::String(Some(s[head..tail].to_vec()));
}

/// `has_key({dict}, {key})` - whether `{dict}` has a key `{key}`
/// (`f_has_key`).
///
/// The original's own `E715: Dictionary required` (a non-`Dict`
/// argument) is omitted, matching this crate's established "skip the
/// display, keep an otherwise-harmless default" policy - `rettv` is
/// simply left untouched (still its caller's own default-initialized
/// `Number(0)`).
fn f_has_key(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let TypvalValue::Dict(d) = &argvars[0].value else {
        return;
    };
    if d.is_null() {
        return;
    }
    let key = crate::eval::typval::tv_get_string(&argvars[1]);
    // SAFETY: `d` is non-null here, and every `Dict`-typed `argvars[0]`
    // reaching this point must already carry a valid pointer, matching
    // every other function in this crate touching that type.
    let found = crate::eval::typval::tv_dict_has_key(unsafe { d.as_mut() }, &key);
    rettv.value = TypvalValue::Number(i64::from(found));
}

/// What to place in each resulting list item for [`tv_dict2list`]
/// (`DictListType`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum DictListType {
    Keys,
    Values,
    Items,
}

/// Turn a dictionary into a `List` (`tv_dict2list`).
///
/// Scoped to `argvars[0]` always being `Dict`-typed (or absent
/// entirely, i.e. a type error) - matches `keys()`/`values()`'s own
/// real shape exactly; `items()` additionally accepts a `String`/
/// `List`/`Blob` in the original (via `tv_string2items`/
/// `tv_list2items`/`tv_blob2items`), none of which are translated yet,
/// so [`f_items`] itself declines those cases explicitly rather than
/// silently mishandling them here.
///
/// The original's own `E715: Dictionary required` (a non-`Dict`
/// argument) is omitted, matching this crate's established "skip the
/// display, keep an otherwise-harmless default" policy - an empty
/// `List` is returned instead, matching the original's own
/// `tv_list_alloc_ret(rettv, 0); return;` for this exact error path.
///
/// # Safety
/// If `argvars[0].value` is `Dict`-typed with a non-null pointer, that
/// pointer must be valid.
unsafe fn tv_dict2list(argvars: &[TypvalT], rettv: &mut TypvalT, what: DictListType) {
    let TypvalValue::Dict(d) = &argvars[0].value else {
        // SAFETY: forwarded from this function's own safety doc.
        let _ = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, 0) };
        return;
    };
    let d = *d;
    let len = crate::eval::typval::tv_dict_len(unsafe { d.as_ref() });
    // SAFETY: forwarded from this function's own safety doc.
    let l = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, len as isize) };
    if d.is_null() {
        return; // NULL dict behaves like an empty dict.
    }

    // SAFETY: forwarded from this function's own safety doc.
    let items: Vec<*mut crate::eval::typval_defs::DictitemT> = unsafe { &*d }.dv_index.values().copied().collect();
    for item in items {
        // SAFETY: `item` came from the dict's own live index above.
        let di = unsafe { &*item };
        // di_key always carries a trailing NUL terminator (matching
        // hi_key's C-string contract - see tv_dict_item_alloc's own
        // doc comment), which a Vimscript String value must NOT
        // include - strip it here, matching the same established
        // idiom used elsewhere in this crate (e.g. typval.rs's own
        // tv_dict_equal).
        let key = &di.di_key[..di.di_key.len() - 1];
        match what {
            DictListType::Keys => {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_list_append_string(l, Some(key)) };
            }
            DictListType::Values => {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_list_append_tv(l, &di.di_tv) };
            }
            DictListType::Items => {
                let sub_l = crate::eval::typval::tv_list_alloc(2);
                // SAFETY: `sub_l` was just allocated above, a fresh
                // pointer not shared with anything yet.
                unsafe { crate::eval::typval::tv_list_ref(sub_l) };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_list_append_string(sub_l, Some(key)) };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_list_append_tv(sub_l, &di.di_tv) };
                let tv_item = TypvalT { value: TypvalValue::List(sub_l), ..Default::default() };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_list_append_owned_tv(l, tv_item) };
            }
        }
    }
}

/// `keys({dict})` - a `List` of `{dict}`'s own keys (`f_keys`).
///
/// # Safety
/// Forwarded from [`tv_dict2list`]'s own safety doc.
unsafe fn f_keys(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict2list(argvars, rettv, DictListType::Keys) };
}

/// `values({dict})` - a `List` of `{dict}`'s own values (`f_values`).
///
/// # Safety
/// Forwarded from [`tv_dict2list`]'s own safety doc.
unsafe fn f_values(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_dict2list(argvars, rettv, DictListType::Values) };
}

/// `items({dict})` - a `List` of `[key, value]` pairs from `{dict}`
/// (`f_items`).
///
/// Only the `Dict` case is translated - the original also accepts a
/// `String` (character-by-character), `List` (index/value pairs), or
/// `Blob` (byte-index/value pairs) via `tv_string2items`/
/// `tv_list2items`/`tv_blob2items`, none of which exist yet. Panics
/// via `unimplemented!()` for those, rather than silently returning an
/// empty/wrong result.
///
/// # Safety
/// Forwarded from [`tv_dict2list`]'s own safety doc.
unsafe fn f_items(argvars: &[TypvalT], rettv: &mut TypvalT) {
    match &argvars[0].value {
        TypvalValue::String(_) => {
            unimplemented!("f_items: a String argument needs tv_string2items, not yet translated")
        }
        TypvalValue::List(_) => {
            unimplemented!("f_items: a List argument needs tv_list2items, not yet translated")
        }
        TypvalValue::Blob(_) => {
            unimplemented!("f_items: a Blob argument needs tv_blob2items, not yet translated")
        }
        // SAFETY: forwarded from this function's own safety doc.
        _ => unsafe { tv_dict2list(argvars, rettv, DictListType::Items) },
    }
}

/// `get({list}, {idx} [, {default}])` / `get({blob}, {idx} [,
/// {default}])` / `get({dict}, {key} [, {default}])` - a generic
/// getter with an optional default for a missing/out-of-range entry
/// (`f_get`).
///
/// Only the `Blob`/`List`/`Dict` cases are translated - the original
/// also accepts a `Funcref`/`Partial` (returning its own `"func"`/
/// `"name"`/`"dict"`/`"args"`/`"arity"` introspection sub-fields, via
/// `get_func_arity`, not yet translated); panics via `unimplemented!()`
/// for that case rather than silently mishandling it. Any other type
/// (matching the original's own `semsg(_(e_listdictblobarg), ...)`
/// case) falls through to the shared "not found" tail below, exactly
/// like the original's own `tv` staying `NULL`.
///
/// The original's own Blob-success path sets `tv = rettv` (a
/// self-alias, then a same-value `tv_copy(tv, rettv)` at the very
/// end; `tv_copy`'s own doc comment explicitly permits `from`/`to`
/// pointing at the same location), then this translation instead
/// returns directly after setting `rettv`, an observably identical
/// outcome (a plain `Number` has no shared/refcounted state a self-
/// copy could otherwise matter for) without needing to alias a
/// `*const` pointer against the same `&mut TypvalT` still used later.
///
/// # Safety
/// If `argvars[0].value` is `Blob`/`List`/`Dict`-typed with a non-null
/// pointer, that pointer must be valid. Forwards [`tv_copy`]'s own
/// safety doc for `argvars[2]` (the default, if given).
unsafe fn f_get(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let mut found: Option<*const TypvalT> = None;

    match &argvars[0].value {
        TypvalValue::Blob(b) => {
            let b = *b;
            let mut error = false;
            let idx = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error)) as i32;
            if !error {
                // SAFETY: forwarded from this function's own safety doc.
                let len = unsafe { crate::eval::typval::tv_blob_len(b) };
                let idx = if idx < 0 { len + idx } else { idx };
                if idx < 0 || idx >= len {
                    rettv.value = TypvalValue::Number(-1);
                } else {
                    // SAFETY: forwarded from this function's own safety doc.
                    rettv.value =
                        TypvalValue::Number(i64::from(unsafe { crate::eval::typval::tv_blob_get(b, idx) }));
                    return;
                }
            }
        }
        TypvalValue::List(l) => {
            let l = *l;
            if !l.is_null() {
                let mut error = false;
                let idx = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error)) as i32;
                if !error {
                    // SAFETY: forwarded from this function's own safety doc.
                    let li = unsafe { crate::eval::typval::tv_list_find(l, idx) };
                    if !li.is_null() {
                        // SAFETY: `li` is a live node just returned by
                        // `tv_list_find` above.
                        found = Some(unsafe { std::ptr::addr_of!((*li).li_tv) });
                    }
                }
            }
        }
        TypvalValue::Dict(d) => {
            let d = *d;
            if !d.is_null() {
                let key = crate::eval::typval::tv_get_string(&argvars[1]);
                // SAFETY: forwarded from this function's own safety doc.
                if let Some(di) = crate::eval::typval::tv_dict_find(unsafe { d.as_mut() }, &key) {
                    // SAFETY: `di` is a live item just returned by
                    // `tv_dict_find` above.
                    found = Some(unsafe { std::ptr::addr_of!((*di).di_tv) });
                }
            }
        }
        TypvalValue::Func(_) | TypvalValue::Partial(_) => {
            unimplemented!(
                "f_get: a Funcref/Partial argument needs get_func_arity and partial \
                 introspection, not yet translated"
            );
        }
        _ => {}
    }

    match found {
        None => {
            if argvars.len() > 2 {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::eval::typval::tv_copy(&argvars[2], rettv) };
            }
        }
        // SAFETY: forwarded from this function's own safety doc; `tv`
        // was just derived above from a live List/Dict item.
        Some(tv) => unsafe { crate::eval::typval::tv_copy(&*tv, rettv) },
    }
}

/// `index({object}, {expr} [, {start} [, {ic}]])` - the index of the
/// first item in `{object}` equal to `{expr}`, or `-1` if not found
/// (`f_index`).
///
/// Any type other than `Blob`/`List` (matching the original's own
/// `emsg(_(e_listblobreq))` case) leaves `rettv` at `-1`, exactly like
/// the original's own pre-set default.
///
/// # Safety
/// If `argvars[0].value` is `Blob`/`List`-typed with a non-null
/// pointer, that pointer must be valid. Forwards
/// [`crate::eval::typval::tv_equal`]'s own safety doc for every item
/// compared against `argvars[1]`.
unsafe fn f_index(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(-1);

    if let TypvalValue::Blob(b) = &argvars[0].value {
        let b = *b;
        let mut start = 0;
        if argvars.len() > 2 {
            let mut error = false;
            start = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error)) as i32;
            if error {
                return;
            }
        }
        if b.is_null() {
            return;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let len = unsafe { crate::eval::typval::tv_blob_len(b) };
        if start < 0 {
            start = (len + start).max(0);
        }
        for idx in start..len {
            let byte_tv = TypvalT {
                // SAFETY: forwarded from this function's own safety doc.
                value: TypvalValue::Number(i64::from(unsafe { crate::eval::typval::tv_blob_get(b, idx) })),
                ..Default::default()
            };
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { crate::eval::typval::tv_equal(&byte_tv, &argvars[1], false) } {
                rettv.value = TypvalValue::Number(i64::from(idx));
                return;
            }
        }
        return;
    }

    let TypvalValue::List(l) = &argvars[0].value else {
        return;
    };
    let l = *l;
    if l.is_null() {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut item = unsafe { crate::eval::typval::tv_list_first(l) };
    let mut idx = 0;
    let mut ic = false;
    if argvars.len() > 2 {
        let mut error = false;
        let start_n = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error)) as i32;
        // SAFETY: forwarded from this function's own safety doc.
        let uidx = unsafe { crate::eval::typval::tv_list_uidx(l, start_n) };
        if error || uidx == -1 {
            item = std::ptr::null_mut();
        } else {
            idx = uidx;
            // SAFETY: forwarded from this function's own safety doc.
            item = unsafe { crate::eval::typval::tv_list_find(l, idx) };
        }
        if argvars.len() > 3 {
            ic = crate::eval::typval::tv_get_number_chk(&argvars[3], Some(&mut error)) != 0;
            if error {
                item = std::ptr::null_mut();
            }
        }
    }

    while !item.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::eval::typval::tv_equal(&(*item).li_tv, &argvars[1], ic) } {
            rettv.value = TypvalValue::Number(i64::from(idx));
            return;
        }
        // SAFETY: forwarded from this function's own safety doc.
        item = unsafe { (*item).li_next };
        idx += 1;
    }
}

/// Reverse text into a newly-allocated `Vec<u8>` (`reverse_text`).
///
/// A NUL byte inside `s` ends the reversal early, matching the
/// original's own NUL-terminated `strlen(s)` bound exactly - the same
/// established "embedded NUL ends a C-string-modeled scan" idiom used
/// elsewhere in this module (e.g. `f_str2list`'s own doc comment).
///
/// # Safety
/// Forwarded from `crate::mbyte::utfc_ptr2len`'s own safety doc.
unsafe fn reverse_text(s: &[u8]) -> Vec<u8> {
    let len = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    let mut rev = vec![0u8; len];
    let mut s_i = 0;
    let mut rev_i = len;
    while s_i < len {
        // SAFETY: forwarded from this function's own safety doc.
        let mb_len = unsafe { crate::mbyte::utfc_ptr2len(&s[s_i..len]) } as usize;
        rev_i -= mb_len;
        rev[rev_i..rev_i + mb_len].copy_from_slice(&s[s_i..s_i + mb_len]);
        s_i += mb_len;
    }
    rev
}

/// `reverse({object})` - reverse a `Blob`/`List`/`String` in place
/// (`f_reverse`).
///
/// The original's own `E1252`-style "not a List/Blob/String" argument-
/// type message is omitted, matching this crate's established "skip
/// the display, keep an otherwise-harmless default" policy - `rettv`
/// is simply left untouched for any other type. A locked `List`
/// (matching the original's own `value_check_lock` guard) is likewise
/// left untouched.
///
/// # Safety
/// If `argvars[0].value` is `Blob`/`List`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_reverse(argvars: &[TypvalT], rettv: &mut TypvalT) {
    match &argvars[0].value {
        TypvalValue::Blob(b) => {
            let b = *b;
            // SAFETY: forwarded from this function's own safety doc.
            let len = unsafe { crate::eval::typval::tv_blob_len(b) };
            for i in 0..len / 2 {
                // SAFETY: forwarded from this function's own safety doc.
                let tmp = unsafe { crate::eval::typval::tv_blob_get(b, i) };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    let other = crate::eval::typval::tv_blob_get(b, len - i - 1);
                    crate::eval::typval::tv_blob_set(b, i, other);
                    crate::eval::typval::tv_blob_set(b, len - i - 1, tmp);
                }
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_blob_set_ret(rettv, b) };
        }
        TypvalValue::String(s) => {
            // SAFETY: forwarded from this function's own safety doc.
            rettv.value = TypvalValue::String(s.as_ref().map(|s| unsafe { reverse_text(s) }));
        }
        TypvalValue::List(l) => {
            let l = *l;
            // SAFETY: forwarded from this function's own safety doc.
            let locked = unsafe { crate::eval::typval::tv_list_locked(l) };
            if !crate::eval::typval::value_check_lock(locked, None) {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    crate::eval::typval::tv_list_reverse(l);
                    crate::eval::typval::tv_list_set_ret(rettv, l);
                }
            }
        }
        _ => {}
    }
}

/// Count the number of times `needle` occurs in `haystack`
/// (`count_string`).
///
/// A `None` `haystack`/`needle`, or an empty `needle`, always counts
/// as `0` - matches the original's own `p == NULL || needle == NULL
/// || *needle == NUL` guard exactly. A NUL byte inside `haystack`
/// ends the count early, matching the original's own NUL-terminated
/// `strstr`/`while (*p != NUL)` scans - the same established
/// "embedded NUL ends a C-string-modeled scan" idiom used elsewhere in
/// this module.
fn count_string(haystack: Option<&[u8]>, needle: Option<&[u8]>, ic: bool) -> crate::eval::typval_defs::VarnumberT {
    let (Some(haystack), Some(needle)) = (haystack, needle) else {
        return 0;
    };
    if needle.is_empty() {
        return 0;
    }
    let len = haystack.iter().position(|&b| b == 0).unwrap_or(haystack.len());
    let haystack = &haystack[..len];

    let mut n = 0;
    let mut pos = 0;
    if ic {
        while pos < haystack.len() {
            if pos + needle.len() <= haystack.len()
                && crate::mbyte::mb_strnicmp(&haystack[pos..], needle, needle.len()) == 0
            {
                n += 1;
                pos += needle.len();
            } else {
                pos += crate::mbyte::utf_ptr2len(&haystack[pos..]).max(1) as usize;
            }
        }
    } else {
        while let Some(rel) = haystack[pos..].windows(needle.len()).position(|w| w == needle) {
            n += 1;
            pos += rel + needle.len();
        }
    }
    n
}

/// Count the number of times `needle` occurs in `l`, starting at
/// index `idx` (`count_list`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live `ListT`.
/// Forwards [`crate::eval::typval::tv_equal`]'s own safety doc for
/// every item compared against `needle`.
unsafe fn count_list(
    l: *mut crate::eval::typval_defs::ListT,
    needle: &TypvalT,
    idx: i32,
    ic: bool,
) -> crate::eval::typval_defs::VarnumberT {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::eval::typval::tv_list_len(l) } == 0 {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let mut item = unsafe { crate::eval::typval::tv_list_find(l, idx) };
    if item.is_null() {
        // Matches the original's own `E984: List index out of range`
        // (message display omitted).
        return 0;
    }

    let mut n = 0;
    while !item.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::eval::typval::tv_equal(&(*item).li_tv, needle, ic) } {
            n += 1;
        }
        // SAFETY: forwarded from this function's own safety doc.
        item = unsafe { (*item).li_next };
    }
    n
}

/// Count the number of times `needle` occurs among `d`'s own values
/// (`count_dict`).
///
/// # Safety
/// `d`, if non-null, must be a valid pointer to a live `DictT`.
/// Forwards [`crate::eval::typval::tv_equal`]'s own safety doc for
/// every value compared against `needle`.
unsafe fn count_dict(
    d: *mut crate::eval::typval_defs::DictT,
    needle: &TypvalT,
    ic: bool,
) -> crate::eval::typval_defs::VarnumberT {
    if d.is_null() {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let items: Vec<*mut crate::eval::typval_defs::DictitemT> = unsafe { &*d }.dv_index.values().copied().collect();
    let mut n = 0;
    for item in items {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::eval::typval::tv_equal(&(*item).di_tv, needle, ic) } {
            n += 1;
        }
    }
    n
}

/// `count({comp}, {expr} [, {ic} [, {start}]])` - count the number of
/// times `{expr}` occurs in `{comp}` (a `String`/`List`/`Dict`)
/// (`f_count`).
///
/// `{start}` (the 4th argument) is only ever consulted for a `List`
/// when `{ic}` (the 3rd argument) was ALSO passed, matching the
/// original's own nested `argvars[2].v_type != VAR_UNKNOWN &&
/// argvars[3].v_type != VAR_UNKNOWN` check exactly (the same "a
/// trailing optional argument is only read when an EARLIER one was
/// also given" shape [`f_trim`]'s own doc comment already explains).
/// For a `Dict`, giving BOTH `{ic}` and `{start}` is the original's
/// own `E118`-style error case (a start index makes no sense for a
/// `Dict`) - message display omitted, `rettv` simply stays `0`.
///
/// Any type other than `String`/`List`/`Dict` (matching the original's
/// own `semsg(...)` case) also leaves `rettv` at `0`.
///
/// # Safety
/// If `argvars[0].value` is `List`/`Dict`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_count(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let mut error = false;
    let mut ic = 0;
    if argvars.len() > 2 {
        ic = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error)) as i32;
    }

    let mut n: crate::eval::typval_defs::VarnumberT = 0;
    if !error {
        match &argvars[0].value {
            TypvalValue::String(haystack) => {
                let needle = crate::eval::typval::tv_get_string_chk(&argvars[1]);
                n = count_string(haystack.as_deref(), needle.as_deref(), ic != 0);
            }
            TypvalValue::List(l) => {
                let l = *l;
                let mut idx = 0;
                if argvars.len() > 3 {
                    idx = crate::eval::typval::tv_get_number_chk(&argvars[3], Some(&mut error));
                }
                if !error {
                    // SAFETY: forwarded from this function's own safety doc.
                    n = unsafe { count_list(l, &argvars[1], idx as i32, ic != 0) };
                }
            }
            TypvalValue::Dict(d) => {
                let d = *d;
                if !d.is_null() && argvars.len() <= 3 {
                    // SAFETY: forwarded from this function's own safety doc.
                    n = unsafe { count_dict(d, &argvars[1], ic != 0) };
                }
            }
            _ => {}
        }
    }
    rettv.value = TypvalValue::Number(n);
}

/// `copy({expr})` - make a shallow copy of `{expr}` (`f_copy`).
///
/// Mirrors the original's own `var_item_copy(NULL, &argvars[0], rettv,
/// false, 0)` call exactly - `conv=NULL`/`deep=false`/`copyID=0` are
/// the ONLY values `copy()` itself ever passes (only `deepcopy()`,
/// not yet translated, ever passes `deep=true`/a real `copyID`), so
/// this directly inlines `var_item_copy`'s own switch for exactly this
/// fixed parameter set, rather than translating the full, more
/// general `var_item_copy` (whose `copyID != 0` "use-the-copy-made-
/// earlier" branches are unreachable dead code for this specific,
/// always-`copyID`-`0` caller).
///
/// `List`/`Dict` assign [`crate::eval::typval::tv_list_copy`]/
/// [`crate::eval::typval::tv_dict_copy`]'s own result DIRECTLY (not
/// via `tv_list_set_ret`/`tv_dict_set_ret`, which would ref-count a
/// SECOND time) - both copy functions already set the new container's
/// own refcount to `1` internally themselves, exactly matching the
/// original's own direct `to->vval.v_list = tv_list_copy(...)`/
/// `to->vval.v_dict = tv_dict_copy(...)` field assignments (no
/// separate `tv_list_ref`/`dv_refcount++` at this call site either).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Dict`/`Blob`/`Partial`-typed with
/// a non-null pointer, that pointer must be valid.
unsafe fn f_copy(argvars: &[TypvalT], rettv: &mut TypvalT) {
    match &argvars[0].value {
        TypvalValue::List(l) => {
            // SAFETY: forwarded from this function's own safety doc.
            let copy = unsafe { crate::eval::typval::tv_list_copy(std::ptr::null(), *l, false, 0) };
            rettv.value = TypvalValue::List(copy);
            rettv.v_lock = crate::eval::typval_defs::VarLockStatus::Unlocked;
        }
        TypvalValue::Blob(b) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_blob_copy(*b, rettv) };
        }
        TypvalValue::Dict(d) => {
            // SAFETY: forwarded from this function's own safety doc.
            let copy = unsafe { crate::eval::typval::tv_dict_copy(std::ptr::null(), *d, false, 0) };
            rettv.value = TypvalValue::Dict(copy);
            rettv.v_lock = crate::eval::typval_defs::VarLockStatus::Unlocked;
        }
        TypvalValue::Unknown => {
            debug_assert!(false, "f_copy(UNKNOWN) - internal_error in the original");
        }
        _ => {
            // Number/Float/Func/Partial/Bool/Special/String: always a
            // plain tv_copy in the original too (String's own
            // "conv == NULL" branch is always taken here).
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_copy(&argvars[0], rettv) };
        }
    }
}

/// `deepcopy({expr} [, {noref}])` - make a deep copy of `{expr}`
/// (`f_deepcopy`).
///
/// Unlike [`f_copy`], nested `List`/`Dict` items are ALSO copied,
/// recursively, via [`crate::eval::eval::var_item_copy`].
///
/// `{noref}` (default `0`/omitted) controls cycle handling: when
/// `0`, a container referenced more than once (e.g. `let l = [1]` /
/// `let outer = [l, l]`) is only copied ONCE - every reference in the
/// result points to that SAME single copy, matching the original
/// list's own sharing exactly (and correctly handling genuine cycles,
/// e.g. a list containing itself). When `{noref}` is `1`, every
/// occurrence gets its OWN separate copy instead - which means a
/// genuine cycle would recurse forever, hence [`DICT_MAXNEST`]'s
/// recursion limit turning that runaway recursion into a clean `FAIL`
/// instead of a stack overflow (matching the original's own
/// documented "a cyclic reference causes deepcopy() to fail" note for
/// `{noref}=1`).
///
/// # Safety
/// Forwarded from [`crate::eval::eval::var_item_copy`]'s own safety
/// doc, applied recursively through every nested List/Dict item
/// reachable from `argvars[0]`.
unsafe fn f_deepcopy(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // argvars.len() > 1 replaces the original's own argvars[1].v_type
    // != VAR_UNKNOWN sentinel check (this crate's argvars is already
    // exactly as long as what was actually passed, unlike the
    // original's own fixed-size, sentinel-padded array) -
    // tv_check_for_opt_bool_arg itself indexes argvars[1] directly,
    // so it must only ever be called when that index genuinely
    // exists; skipping it entirely when the optional arg is absent
    // produces the identical net effect, since tv_check_for_opt_bool_arg's
    // own first check is precisely "argvars[1] unknown -> OK" anyway.
    if argvars.len() > 1 && crate::eval::typval::tv_check_for_opt_bool_arg(argvars, 1) == crate::vim_defs::FAIL {
        return;
    }

    let mut noref: crate::eval::typval_defs::VarnumberT = 0;
    if argvars.len() > 1 {
        noref = crate::eval::typval::tv_get_bool_chk(&argvars[1], None);
    }

    let copy_id = if noref == 0 { crate::eval::eval::get_copy_id() } else { 0 };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::eval::var_item_copy(std::ptr::null(), &argvars[0], rettv, true, copy_id) };
}

/// `add({object}, {expr})` - append `{expr}` to `{object}` (a `List`
/// or `Blob`) (`f_add`, `eval/list.c`).
///
/// Returns the resulting `List`/`Blob` - the SAME container, mutated
/// in place (not a copy) - or `1` ("failed") if `{object}` is neither
/// (the original's own `emsg(_(e_listblobreq))` is omitted - message
/// display, not tractable; the identical default `rettv` value is
/// kept).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Blob`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_add(argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(1); // Default: failed.
    match &argvars[0].value {
        TypvalValue::List(l) => {
            let l = *l;
            // SAFETY: forwarded from this function's own safety doc.
            let locked = unsafe { crate::eval::typval::tv_list_locked(l) };
            if !crate::eval::typval::value_check_lock(locked, None) {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    crate::eval::typval::tv_list_append_tv(l, &argvars[1]);
                    crate::eval::typval::tv_copy(&argvars[0], rettv);
                }
            }
        }
        TypvalValue::Blob(b) => {
            let b = *b;
            if !b.is_null() {
                // SAFETY: forwarded from this function's own safety doc.
                let locked = unsafe { (*b).bv_lock };
                if !crate::eval::typval::value_check_lock(locked, None) {
                    let mut error = false;
                    let n = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error));
                    if !error {
                        // SAFETY: forwarded from this function's own safety doc.
                        unsafe {
                            (*b).bv_ga.ga_append(n as u8);
                            crate::eval::typval::tv_copy(&argvars[0], rettv);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// `insert({object}, {item} [, {idx}])` - insert `{item}` into
/// `{object}` (a `List` or `Blob`) before index `{idx}` (default `0`,
/// i.e. at the start) (`f_insert`, `eval/list.c`).
///
/// Returns the resulting `List`/`Blob` (the SAME container, mutated
/// in place) via [`crate::eval::typval::tv_copy`], or leaves `rettv`
/// at its caller-provided default (`Number(0)`) on any failure -
/// unlike [`f_add`], this function never sets its own explicit
/// "failed" sentinel, exactly matching the original's own structure
/// (every error path is a bare early `return`).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Blob`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_insert(argvars: &[TypvalT], rettv: &mut TypvalT) {
    match &argvars[0].value {
        TypvalValue::Blob(b) => {
            let b = *b;
            if b.is_null() {
                return;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let locked = unsafe { (*b).bv_lock };
            if crate::eval::typval::value_check_lock(locked, None) {
                return;
            }

            // SAFETY: forwarded from this function's own safety doc.
            let len = unsafe { crate::eval::typval::tv_blob_len(b) };
            let mut before: i32 = 0;
            // argvars.len() > 2 replaces the original's own
            // argvars[2].v_type != VAR_UNKNOWN sentinel check.
            if argvars.len() > 2 {
                let mut error = false;
                before = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error)) as i32;
                if error {
                    return; // type error; errmsg already given in the original.
                }
                if before < 0 || before > len {
                    return; // semsg(_(e_invarg2), ...) omitted - see this module's own doc comment.
                }
            }
            let mut error = false;
            let val = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error));
            if error {
                return;
            }
            if !(0..=255).contains(&val) {
                return; // semsg(_(e_invarg2), ...) omitted.
            }

            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*b).bv_ga.ga_grow(1) };
            // SAFETY: ga_grow just ensured ga_data has at least
            // len+1 bytes of real capacity - shifting [before, len)
            // right by one within that capacity (mirroring the
            // original's own memmove) is safe.
            unsafe {
                let ga_data = &mut (*b).bv_ga.ga_data;
                for i in (before..len).rev() {
                    ga_data[(i + 1) as usize] = ga_data[i as usize];
                }
                ga_data[before as usize] = val as u8;
                (*b).bv_ga.ga_len += 1;
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_copy(&argvars[0], rettv) };
        }
        TypvalValue::List(_) => {
            // SAFETY: forwarded from this function's own safety doc.
            let TypvalValue::List(mut l) = argvars[0].value else { unreachable!() };
            // SAFETY: forwarded from this function's own safety doc.
            let locked = unsafe { crate::eval::typval::tv_list_locked(l) };
            if crate::eval::typval::value_check_lock(locked, None) {
                return;
            }

            let mut before: crate::eval::typval_defs::VarnumberT = 0;
            if argvars.len() > 2 {
                let mut error = false;
                before = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error));
                if error {
                    return;
                }
            }

            let mut item = std::ptr::null_mut();
            // SAFETY: forwarded from this function's own safety doc.
            if before != i64::from(unsafe { crate::eval::typval::tv_list_len(l) }) {
                // SAFETY: forwarded from this function's own safety doc.
                item = unsafe { crate::eval::typval::tv_list_find(l, before as i32) };
                if item.is_null() {
                    l = std::ptr::null_mut(); // semsg(_(e_list_index_out_of_range_nr), ...) omitted.
                }
            }
            if !l.is_null() {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    crate::eval::typval::tv_list_insert_tv(l, &argvars[1], item);
                    crate::eval::typval::tv_copy(&argvars[0], rettv);
                }
            }
        }
        _ => {
            // semsg(_(e_listblobarg), "insert()") omitted - message
            // display, not tractable; rettv is simply left at its
            // caller-provided default.
        }
    }
}

/// `remove({object}, {idx} [, {end}])` - remove an item (or a range)
/// from a `List`/`Blob` at `{idx}` (`{end}`, if given), or
/// `remove({dict}, {key})` - remove the entry `{key}` from a `Dict`
/// (`f_remove`, `eval/list.c`). Dispatches to
/// [`crate::eval::typval::tv_list_remove`]/
/// [`crate::eval::typval::tv_blob_remove`]/
/// [`crate::eval::typval::tv_dict_remove`] by `argvars[0]`'s own type -
/// every other type leaves `rettv` at its caller-provided default
/// (the original's own `semsg(_(e_listdictblobarg), "remove()")` is
/// omitted, message display not tractable).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Dict`/`Blob`-typed with a
/// non-null pointer, that pointer must be valid, with every item
/// genuinely allocated via the matching `_alloc` helper.
unsafe fn f_remove(argvars: &[TypvalT], rettv: &mut TypvalT) {
    match &argvars[0].value {
        TypvalValue::Dict(_) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_dict_remove(argvars, rettv) };
        }
        TypvalValue::Blob(_) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_blob_remove(argvars, rettv) };
        }
        TypvalValue::List(_) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::eval::typval::tv_list_remove(argvars, rettv) };
        }
        _ => {}
    }
}

/// The `List` case of `extend()`/`extendnew()` (`extend_list`,
/// `eval/list.c`).
///
/// `is_new` selects `extendnew()`'s own "copy first" behavior
/// (`true`) vs. `extend()`'s own in-place mutation (`false`) - when
/// `true`, `l1` is [`crate::eval::typval::tv_list_copy`]'d before
/// extending, and the copy is moved DIRECTLY into `rettv` (no extra
/// [`crate::eval::typval::tv_copy`]-based refcount increment, since
/// `tv_list_copy` already set the new list's own refcount to `1`,
/// exactly matching the original's own direct `rettv->vval.v_list =
/// l1` field assignment).
///
/// # Safety
/// `argvars[0]`/`argvars[1]`'s values must be `List`-typed with valid
/// pointers (non-null, or null - a null `argvars[1]` is a no-op,
/// matching the original's own `l2 == NULL` early-return).
unsafe fn extend_list(argvars: &[TypvalT], is_new: bool, rettv: &mut TypvalT) {
    let TypvalValue::List(mut l1) = argvars[0].value else { unreachable!() };

    // SAFETY: forwarded from this function's own safety doc.
    if !is_new && crate::eval::typval::value_check_lock(unsafe { crate::eval::typval::tv_list_locked(l1) }, None) {
        return;
    }

    if is_new {
        // SAFETY: forwarded from this function's own safety doc.
        l1 =
            unsafe { crate::eval::typval::tv_list_copy(std::ptr::null(), l1, false, crate::eval::eval::get_copy_id()) };
        if l1.is_null() {
            return;
        }
    }

    let TypvalValue::List(l2) = argvars[1].value else { unreachable!() };
    if !l2.is_null() {
        let mut item = std::ptr::null_mut();
        // argvars.len() > 2 replaces the original's own
        // argvars[2].v_type != VAR_UNKNOWN sentinel check.
        if argvars.len() > 2 {
            let mut error = false;
            let before = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error));
            if error {
                if is_new {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { crate::eval::typval::tv_list_unref(l1) };
                }
                return; // type error; errmsg already given in the original.
            }
            // SAFETY: forwarded from this function's own safety doc.
            if before != i64::from(unsafe { crate::eval::typval::tv_list_len(l1) }) {
                // SAFETY: forwarded from this function's own safety doc.
                item = unsafe { crate::eval::typval::tv_list_find(l1, before as i32) };
                if item.is_null() {
                    if is_new {
                        // SAFETY: forwarded from this function's own safety doc.
                        unsafe { crate::eval::typval::tv_list_unref(l1) };
                    }
                    return; // semsg(_(e_list_index_out_of_range_nr), ...) omitted.
                }
            }
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_list_extend(l1, l2, item) };
    }

    if is_new {
        rettv.value = TypvalValue::List(l1);
        rettv.v_lock = crate::eval::typval_defs::VarLockStatus::Unlocked;
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_copy(&argvars[0], rettv) };
    }
}

/// The `Dict` case of `extend()`/`extendnew()` (`extend_dict`,
/// `eval/list.c`). See [`extend_list`]'s own doc comment for the
/// shared `is_new` reasoning (identical here, for `Dict` instead of
/// `List`).
///
/// # Safety
/// `argvars[0]`/`argvars[1]`'s values must be `Dict`-typed with valid
/// pointers.
unsafe fn extend_dict(argvars: &[TypvalT], is_new: bool, rettv: &mut TypvalT) {
    let TypvalValue::Dict(mut d1) = argvars[0].value else { unreachable!() };
    if d1.is_null() {
        // The original's own value_check_lock(VAR_FIXED, ...) call
        // here always returns true (VAR_FIXED is always "locked") -
        // its only real effect is the error message it would display
        // (omitted, see this module's own doc comment), so a null d1
        // always just returns, unconditionally.
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    if !is_new && crate::eval::typval::value_check_lock(unsafe { (*d1).dv_lock }, None) {
        return;
    }

    if is_new {
        // SAFETY: forwarded from this function's own safety doc.
        d1 =
            unsafe { crate::eval::typval::tv_dict_copy(std::ptr::null(), d1, false, crate::eval::eval::get_copy_id()) };
        if d1.is_null() {
            return;
        }
    }

    let TypvalValue::Dict(d2) = argvars[1].value else { unreachable!() };
    if !d2.is_null() {
        let mut action: Vec<u8> = b"force".to_vec();
        if argvars.len() > 2 {
            let Some(action_str) = crate::eval::typval::tv_get_string_chk(&argvars[2]) else {
                if is_new {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { crate::eval::typval::tv_dict_unref(d1) };
                }
                return; // type error; errmsg already given in the original.
            };
            if !matches!(action_str.as_slice(), b"keep" | b"force" | b"error") {
                if is_new {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { crate::eval::typval::tv_dict_unref(d1) };
                }
                return; // semsg(_(e_invarg2), ...) omitted.
            }
            action = action_str;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_dict_extend(d1, d2, &action) };
    }

    if is_new {
        rettv.value = TypvalValue::Dict(d1);
        rettv.v_lock = crate::eval::typval_defs::VarLockStatus::Unlocked;
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_copy(&argvars[0], rettv) };
    }
}

/// Shared `extend()`/`extendnew()` dispatch by `argvars[0]`/`[1]`'s
/// own types (`extend`, `eval/list.c`'s own private static helper).
///
/// # Safety
/// Forwards [`extend_list`]/[`extend_dict`]'s own safety docs.
unsafe fn extend(argvars: &[TypvalT], is_new: bool, rettv: &mut TypvalT) {
    match (&argvars[0].value, &argvars[1].value) {
        // SAFETY: forwarded from this function's own safety doc.
        (TypvalValue::List(_), TypvalValue::List(_)) => unsafe { extend_list(argvars, is_new, rettv) },
        // SAFETY: forwarded from this function's own safety doc.
        (TypvalValue::Dict(_), TypvalValue::Dict(_)) => unsafe { extend_dict(argvars, is_new, rettv) },
        _ => {
            // semsg(_(e_listdictarg), is_new ? "extendnew()" : "extend()")
            // omitted - message display, not tractable; rettv is
            // simply left at its caller-provided default.
        }
    }
}

/// `extend({expr1}, {expr2} [, {expr3}])` - extend `{expr1}` (a `List`
/// or `Dict`) in place with `{expr2}`'s own items (`f_extend`,
/// `eval/list.c`). Returns the SAME (mutated) `{expr1}`.
///
/// # Safety
/// Forwards [`extend`]'s own safety doc.
unsafe fn f_extend(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { extend(argvars, false, rettv) };
}

/// `extendnew({expr1}, {expr2} [, {expr3}])` - like [`f_extend`], but
/// `{expr1}` is copied first, leaving the original untouched
/// (`f_extendnew`, `eval/list.c`). Returns the NEW, extended copy.
///
/// # Safety
/// Forwards [`extend`]'s own safety doc.
unsafe fn f_extendnew(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { extend(argvars, true, rettv) };
}

/// `range({expr} [, {max} [, {stride}]])` - a `List` of numbers
/// (`f_range`, `funcs.c`).
///
/// One argument: `[0, 1, ..., {expr} - 1]`. Two: `[{expr}, {expr} + 1,
/// ..., {max}]`. Three: like two, but stepping by `{stride}` each time
/// (which may be negative, counting down).
///
/// Every error path (a type error reading any argument, a zero
/// `{stride}`, or `{start}` past `{end}` for the given `{stride}`'s
/// own direction) is a bare early return leaving `rettv` at its
/// caller-provided default - the original's own `emsg` calls are
/// omitted, message display not tractable (see this module's own doc
/// comment).
unsafe fn f_range(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let mut error = false;
    let mut start = crate::eval::typval::tv_get_number_chk(&argvars[0], Some(&mut error));
    let mut stride: crate::eval::typval_defs::VarnumberT = 1;
    let end;
    // argvars.len() > 1 replaces the original's own argvars[1].v_type
    // != VAR_UNKNOWN sentinel check.
    if argvars.len() <= 1 {
        end = start - 1;
        start = 0;
    } else {
        end = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error));
        if argvars.len() > 2 {
            stride = crate::eval::typval::tv_get_number_chk(&argvars[2], Some(&mut error));
        }
    }

    if error {
        return; // type error; errmsg already given in the original.
    }
    if stride == 0 {
        return; // emsg(_("E726: Stride is zero")) omitted.
    }
    if if stride > 0 { end + 1 < start } else { end - 1 > start } {
        return; // emsg(_("E727: Start past end")) omitted.
    }

    // SAFETY: rettv is a plain `&mut TypvalT`, always safe to write
    // into; tv_list_alloc_ret's own `len` hint is unused (see its own
    // doc comment), so an oddly-signed `(end - start) / stride` here
    // is harmless either way.
    let list = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, ((end - start) / stride) as isize) };
    let mut i = start;
    while if stride > 0 { i <= end } else { i >= end } {
        // SAFETY: `list` was just allocated above by this same call.
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, i) };
        i += stride;
    }
}

/// Repeat list `l` `n` times into a NEW list, set into `rettv`
/// (`repeat_list`, `funcs.c`).
///
/// # Safety
/// `l`, if non-null, must be a valid pointer to a live
/// [`crate::eval::typval_defs::ListT`].
unsafe fn repeat_list(l: *mut crate::eval::typval_defs::ListT, n: crate::eval::typval_defs::VarnumberT, rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let len_hint = if n > 0 { n * i64::from(unsafe { crate::eval::typval::tv_list_len(l) }) } else { 0 };
    // SAFETY: rettv is a plain `&mut TypvalT`, always safe to write
    // into.
    let new_list = unsafe { crate::eval::typval::tv_list_alloc_ret(rettv, len_hint as isize) };
    let mut remaining = n;
    while remaining > 0 {
        remaining -= 1;
        // SAFETY: `new_list` was just allocated above by this same
        // call; `l`, forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_list_extend(new_list, l, std::ptr::null_mut()) };
    }
}

/// Repeat blob `blob_tv`'s own bytes `n` times into a NEW blob, set
/// into `rettv` (`repeat_blob`, `funcs.c`).
///
/// Builds the repeated byte buffer directly via a plain `Vec<u8>`
/// (`bv_ga.ga_data` IS one in this crate) rather than the original's
/// own `ga_grow`+`tv_blob_set_range`-based approach (including its
/// own "skip the copy if every byte is already zero" optimization,
/// a pure C-level micro-optimization not needed here) - `funcs.c`'s
/// own general-purpose `tv_blob_set_range` (used elsewhere for list-
/// index-assignment into a blob, not yet translated) isn't needed for
/// this one, simpler, always-appending caller.
///
/// # Safety
/// `blob_tv.value` must be `Blob`-typed; if its pointer is non-null,
/// it must be valid.
unsafe fn repeat_blob(blob_tv: &TypvalT, n: crate::eval::typval_defs::VarnumberT, rettv: &mut TypvalT) {
    let TypvalValue::Blob(blob) = blob_tv.value else { unreachable!() };

    let new_blob = crate::eval::typval::tv_blob_alloc_ret(rettv);
    if blob.is_null() || n <= 0 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let slen = unsafe { crate::eval::typval::tv_blob_len(blob) };
    let len = i64::from(slen) * n;
    if len <= 0 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let src: Vec<u8> = unsafe { (&(*blob).bv_ga.ga_data)[..slen as usize].to_vec() };
    let mut data = Vec::with_capacity(len as usize);
    for _ in 0..n {
        data.extend_from_slice(&src);
    }
    // SAFETY: `new_blob` was just allocated above, a fresh pointer
    // not shared with anything yet.
    unsafe {
        (*new_blob).bv_ga.ga_data = data;
        (*new_blob).bv_ga.ga_len = len as i32;
        (*new_blob).bv_ga.ga_maxlen = len as i32;
    }
}

/// Repeat string `str_tv`'s own bytes `n` times into a NEW string, set
/// into `rettv` (`repeat_string`, `funcs.c`).
///
/// `strlen(p)` (the original's own C-string-bounded scan) is mirrored
/// by stopping at the first embedded NUL (or the full length, if
/// none) - this crate's own established "embedded NUL ends a
/// C-string-modeled scan" idiom, used throughout this module.
/// Overflow detection uses [`usize::checked_mul`] directly (a more
/// robust equivalent of the original's own `len / n != slen`
/// division-based check).
fn repeat_string(str_tv: &TypvalT, n: crate::eval::typval_defs::VarnumberT, rettv: &mut TypvalT) {
    rettv.value = TypvalValue::String(None);
    if n <= 0 {
        return;
    }

    let s = crate::eval::typval::tv_get_string(str_tv);
    let slen = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    if slen == 0 {
        return;
    }
    let Some(len) = slen.checked_mul(n as usize) else {
        return; // overflow.
    };

    let mut r = Vec::with_capacity(len);
    for _ in 0..n {
        r.extend_from_slice(&s[..slen]);
    }
    rettv.value = TypvalValue::String(Some(r));
}

/// `repeat({expr}, {count})` - repeat `{expr}` `{count}` times,
/// concatenated (`f_repeat`, `funcs.c`). `{expr}` may be a `List`
/// (dispatches to [`repeat_list`]), `Blob` ([`repeat_blob`]), or
/// anything else, stringified first ([`repeat_string`]).
///
/// # Safety
/// If `argvars[0].value` is `List`/`Blob`-typed with a non-null
/// pointer, that pointer must be valid.
unsafe fn f_repeat(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let n = crate::eval::typval::tv_get_number(&argvars[1]);
    match &argvars[0].value {
        TypvalValue::List(l) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { repeat_list(*l, n, rettv) };
        }
        TypvalValue::Blob(_) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { repeat_blob(&argvars[0], n, rettv) };
        }
        _ => repeat_string(&argvars[0], n, rettv),
    }
}

/// `join({list} [, {sep}])` - join `{list}`'s own items into a single
/// string, separated by `{sep}` (default `" "`) (`f_join`,
/// `eval/typval.c`).
///
/// A non-`List` `argvars[0]` is a bare early return leaving `rettv` at
/// its caller-provided default (the original's own `emsg(_(e_listreq))`
/// is omitted). A `{sep}` type error still sets `rettv` to a null
/// `String` (not left at the caller default) - matching the
/// original's own unconditional `rettv->v_type = VAR_STRING;` once
/// `argvars[0]` is confirmed to be a real `List`, regardless of
/// whether `{sep}` itself was valid.
///
/// # Safety
/// Forwards [`crate::eval::typval::tv_list_join`]'s own safety
/// doc/panic condition.
unsafe fn f_join(argvars: &[TypvalT], rettv: &mut TypvalT) {
    if !matches!(argvars[0].value, TypvalValue::List(_)) {
        return; // emsg(_(e_listreq)) omitted; rettv left at its caller-provided default.
    }
    let TypvalValue::List(l) = argvars[0].value else { unreachable!() };

    let sep: Vec<u8> = if argvars.len() > 1 {
        let Some(s) = crate::eval::typval::tv_get_string_chk(&argvars[1]) else {
            rettv.value = TypvalValue::String(None);
            return; // type error; errmsg already given in the original.
        };
        s
    } else {
        b" ".to_vec()
    };

    // SAFETY: forwarded from this function's own safety doc.
    let joined = unsafe { crate::eval::typval::tv_list_join(l, &sep) };
    rettv.value = TypvalValue::String(Some(joined));
}

/// Shared `flatten()`/`flattennew()` implementation (`flatten_common`,
/// `funcs.c`). `make_copy` selects `flattennew()`'s own "copy first"
/// behavior (`true`) vs. `flatten()`'s own in-place mutation
/// (`false`) - matching [`extend_list`]'s own identical `is_new`
/// reasoning (though here, unlike `extend_list`, `rettv`'s own List
/// value is assigned UPFRONT, before flattening even begins, exactly
/// matching the original's own statement order).
///
/// # Safety
/// If `argvars[0].value` is `List`-typed with a non-null pointer, it
/// must be valid, recursively (forwarded to
/// [`crate::eval::typval::tv_list_flatten`]'s own safety doc).
unsafe fn flatten_common(argvars: &[TypvalT], make_copy: bool, rettv: &mut TypvalT) {
    if !matches!(argvars[0].value, TypvalValue::List(_)) {
        return; // semsg(_(e_listarg), "flatten()") omitted; rettv left at its caller-provided default.
    }

    let mut maxdepth: i64 = 999_999;
    // argvars.len() > 1 replaces the original's own argvars[1].v_type
    // != VAR_UNKNOWN sentinel check.
    if argvars.len() > 1 {
        let mut error = false;
        maxdepth = crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error));
        if error {
            return; // type error; errmsg already given in the original.
        }
        if maxdepth < 0 {
            return; // emsg(_("E900: maxdepth must be non-negative number")) omitted.
        }
    }

    let TypvalValue::List(mut list) = argvars[0].value else { unreachable!() };
    rettv.value = TypvalValue::List(list);
    if list.is_null() {
        return;
    }

    if make_copy {
        // SAFETY: forwarded from this function's own safety doc.
        list = unsafe {
            crate::eval::typval::tv_list_copy(std::ptr::null(), list, false, crate::eval::eval::get_copy_id())
        };
        rettv.value = TypvalValue::List(list);
        if list.is_null() {
            return;
        }
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        if crate::eval::typval::value_check_lock(unsafe { crate::eval::typval::tv_list_locked(list) }, None) {
            return;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_list_ref(list) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { crate::eval::typval::tv_list_len(list) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::typval::tv_list_flatten(list, std::ptr::null_mut(), i64::from(len), maxdepth) };
}

/// `flatten({list} [, {maxdepth}])` - flatten `{list}` in place, up to
/// `{maxdepth}` levels (default effectively unlimited) (`f_flatten`,
/// `funcs.c`). Returns the SAME (mutated) `{list}`.
///
/// # Safety
/// Forwards [`flatten_common`]'s own safety doc.
unsafe fn f_flatten(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { flatten_common(argvars, false, rettv) };
}

/// `flattennew({list} [, {maxdepth}])` - like [`f_flatten`], but
/// `{list}` is copied first, leaving the original untouched
/// (`f_flattennew`, `funcs.c`). Returns the NEW, flattened copy.
///
/// # Safety
/// Forwards [`flatten_common`]'s own safety doc.
unsafe fn f_flattennew(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { flatten_common(argvars, true, rettv) };
}

/// `localtime()` - the current time, measured in seconds since 1970-01-01
/// (`f_localtime`, `funcs.c`).
fn f_localtime(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    rettv.value = TypvalValue::Number(crate::os::time::os_time() as crate::eval::typval_defs::VarnumberT);
}

/// `getenv({name})` - the value of environment variable `{name}`, or
/// `v:null` if unset (`f_getenv`, `funcs.c`), via the already-existing
/// [`crate::os::env::vim_getenv`].
///
/// # Safety
/// Forwards `vim_getenv`'s own safety doc (Windows `$HOME` path only).
/// Also panics if `{name}` is `"VIM"`/`"VIMRUNTIME"` and that variable
/// isn't ACTUALLY set in the real environment - `vim_getenv`'s own,
/// pre-existing, already-documented gap (its `$VIM`/`$VIMRUNTIME`
/// runtime-directory auto-discovery fallback isn't translated yet),
/// not something this function itself introduces.
unsafe fn f_getenv(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let name = crate::eval::typval::tv_get_string(&argvars[0]);
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { crate::os::env::vim_getenv(&name) } {
        Some(value) => rettv.value = TypvalValue::String(Some(value)),
        None => rettv.value = TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
    }
}

/// `environ()` - all environment variables as a `Dict` (`f_environ`).
///
/// Unlike every other builtin in this module, this one has NO C
/// implementation at all - the original itself implements it in Lua
/// (`runtime/lua/vim/_core/vimfn.lua`'s own `M.f_environ`), calling
/// `vim.uv.os_environ()` (libuv's own environment-enumeration) and,
/// on Windows only, force-uppercasing every key (matching legacy Vim
/// behavior, `#39443`). Translated directly from that Lua source
/// (this whole mission's own scope explicitly includes Neovim's Lua
/// source, not just its C source) using `std::env::vars_os()` as the
/// portable equivalent of `os_environ()` - lossy-UTF-8-converted to
/// `Vec<u8>`, matching [`crate::os::env::os_getenv`]'s own already-
/// established conversion for the exact same underlying `OsString`
/// values.
fn f_environ(_argvars: &[TypvalT], rettv: &mut TypvalT) {
    let d = crate::eval::typval::tv_dict_alloc();
    for (key, val) in std::env::vars_os() {
        let mut key = key.to_string_lossy().into_owned();
        if cfg!(windows) {
            key = key.to_uppercase();
        }
        let val = val.to_string_lossy().into_owned();
        // SAFETY: `d` was just allocated above, a fresh pointer not
        // shared with anything yet.
        unsafe { crate::eval::typval::tv_dict_add_str(&mut *d, key.as_bytes(), Some(val.as_bytes())) };
    }
    // SAFETY: `d` was just allocated above, a fresh pointer not
    // shared with anything yet.
    unsafe { crate::eval::typval::tv_dict_set_ret(rettv, d) };
}

/// The unconditional (platform-independent) entries of `funcs.c`'s own
/// `has_list[]` static array, used by [`f_has`] - compile-time feature
/// flags (this build supports capability X), not runtime state, hence
/// unconditional regardless of THIS crate's own current translation
/// progress on any given subsystem (matching the original's own
/// "always present" nature for these, e.g. `has('spell')` is `1` in
/// any real Nvim build even with `'spell'` off for the current
/// buffer).
const HAS_LIST_UNCONDITIONAL: &[&[u8]] = &[
    b"autochdir",
    b"arabic",
    b"autocmd",
    b"browsefilter",
    b"byte_offset",
    b"cindent",
    b"cmdline_compl",
    b"cmdline_hist",
    b"cmdwin",
    b"comments",
    b"conceal",
    b"cursorbind",
    b"cursorshape",
    b"dialog_con",
    b"diff",
    b"digraphs",
    b"eval",
    b"ex_extra",
    b"extra_search",
    b"file_in_path",
    b"filterpipe",
    b"find_in_path",
    b"float",
    b"folding",
    b"gettext",
    b"iconv",
    b"insert_expand",
    b"jumplist",
    b"keymap",
    b"lambda",
    b"langmap",
    b"libcall",
    b"linebreak",
    b"lispindent",
    b"listcmds",
    b"localmap",
    b"menu",
    b"mksession",
    b"modify_fname",
    b"mouse",
    b"multi_byte",
    b"multi_lang",
    b"nanotime",
    b"num64",
    b"packages",
    b"path_extra",
    b"persistent_undo",
    b"profile",
    b"reltime",
    b"quickfix",
    b"rightleft",
    b"scrollbind",
    b"showcmd",
    b"cmdline_info",
    b"shada",
    b"signs",
    b"smartindent",
    b"startuptime",
    b"statusline",
    b"spell",
    b"syntax",
    b"tablineat",
    b"tag_binary",
    b"termguicolors",
    b"termresponse",
    b"textobjects",
    b"timers",
    b"title",
    b"user-commands",
    b"user_commands",
    b"vartabs",
    b"vertsplit",
    b"vimscript-1",
    b"virtualedit",
    b"visual",
    b"visualextra",
    b"vreplace",
    b"wildignore",
    b"wildmenu",
    b"windows",
    b"winaltkeys",
    b"writebackup",
    b"nvim",
];

/// `has({feature})` - whether `{feature}` is supported (`f_has`,
/// `funcs.c`), case-insensitively.
///
/// Checks `HAS_LIST_UNCONDITIONAL` plus the original's own platform-
/// conditional (`#ifdef`) entries this crate CAN meaningfully
/// determine at compile time (`unix`/`linux`/`win32`/`win64`/`mac`
/// family/`fork`/`system`/`fname_case`).
///
/// Every other special case the original handles dynamically
/// (`patch-N`/`nvim-x.y.z` version checks, `vim_starting`/`ttyin`/
/// `ttyout`/`gui_running`/`syntax_items`/`wsl` runtime state, and
/// provider-based checks like `clipboard_working`/`pythonx`) simply
/// returns `0` here - not a translation gap that panics, since
/// `has()`'s own contract ("is this obscure feature present") already
/// naturally accommodates "not present" as a fully valid, non-error
/// answer for a real Nvim build too (e.g. `has('gui_running')` is `0`
/// in any terminal-only session, which describes EVERY session this
/// crate can currently produce anyway).
fn f_has(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let name = crate::eval::typval::tv_get_string(&argvars[0]);

    let platform_conditional: &[&[u8]] = if cfg!(windows) {
        &[&b"win32"[..], &b"system"[..]]
    } else if cfg!(target_os = "macos") {
        &[&b"unix"[..], &b"fork"[..], &b"mac"[..], &b"macunix"[..], &b"osx"[..], &b"osxdarwin"[..]]
    } else if cfg!(target_os = "linux") {
        &[&b"unix"[..], &b"fork"[..], &b"linux"[..], &b"fname_case"[..]]
    } else if cfg!(unix) {
        &[&b"unix"[..], &b"fork"[..]]
    } else {
        &[]
    };
    let win64_matches =
        cfg!(all(windows, target_pointer_width = "64")) && name.eq_ignore_ascii_case(b"win64");

    let found = win64_matches
        || platform_conditional.iter().any(|f| f.eq_ignore_ascii_case(&name))
        || HAS_LIST_UNCONDITIONAL.iter().any(|f| f.eq_ignore_ascii_case(&name));
    rettv.value = TypvalValue::Number(i64::from(found));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: crate::eval::typval_defs::VarnumberT) -> TypvalT {
        TypvalT { value: TypvalValue::Number(n), ..Default::default() }
    }

    fn string(s: &[u8]) -> TypvalT {
        TypvalT { value: TypvalValue::String(Some(s.to_vec())), ..Default::default() }
    }

    fn float(f: f64) -> TypvalT {
        TypvalT { value: TypvalValue::Float(f), ..Default::default() }
    }

    // --- find_internal_func / call_internal_func ---

    #[test]
    fn find_internal_func_finds_a_translated_builtin() {
        assert!(find_internal_func(b"len").is_some());
        assert!(find_internal_func(b"type").is_some());
        assert!(find_internal_func(b"empty").is_some());
    }

    #[test]
    fn find_internal_func_unknown_name_is_none() {
        assert!(find_internal_func(b"not_a_real_function").is_none());
    }

    #[test]
    fn call_internal_func_unknown_name_fails() {
        let mut rettv = TypvalT::default();
        let result = unsafe { call_internal_func(b"not_a_real_function", &[], &mut rettv) };
        assert_eq!(result, FnameTransError::Unknown);
    }

    #[test]
    fn call_internal_func_too_few_arguments_fails() {
        let mut rettv = TypvalT::default();
        let result = unsafe { call_internal_func(b"len", &[], &mut rettv) };
        assert_eq!(result, FnameTransError::TooFew);
    }

    #[test]
    fn call_internal_func_too_many_arguments_fails() {
        let mut rettv = TypvalT::default();
        let args = [num(1), num(2)];
        let result = unsafe { call_internal_func(b"len", &args, &mut rettv) };
        assert_eq!(result, FnameTransError::TooMany);
    }

    #[test]
    fn call_internal_func_dispatches_correctly() {
        let mut rettv = TypvalT::default();
        let args = [string(b"hello")];
        let result = unsafe { call_internal_func(b"len", &args, &mut rettv) };
        assert_eq!(result, FnameTransError::None);
        assert_eq!(rettv.value, TypvalValue::Number(5));
    }

    // --- f_len ---

    #[test]
    fn len_of_a_string() {
        let mut rettv = TypvalT::default();
        unsafe { f_len(&[string(b"hello")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(5));
    }

    #[test]
    fn len_of_a_number_uses_its_string_form() {
        let mut rettv = TypvalT::default();
        unsafe { f_len(&[num(12345)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(5));
    }

    #[test]
    fn len_of_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_len(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(3));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn len_of_a_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        let item = crate::eval::typval::tv_dict_item_alloc(b"a");
        unsafe {
            (*item).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_len(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn len_of_an_empty_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }];
        unsafe { f_len(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { crate::eval::typval::tv_blob_free(blob) };
    }

    // --- f_type ---

    #[test]
    fn type_of_each_scalar_variant() {
        use crate::eval::typval_defs::var_type_result;
        let mut rettv = TypvalT::default();
        f_type(&[num(1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::NUMBER.into()));
        f_type(&[string(b"s")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::STRING.into()));
        f_type(&[TypvalT { value: TypvalValue::Float(1.0), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::FLOAT.into()));
        f_type(
            &[TypvalT { value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True), ..Default::default() }],
            &mut rettv,
        );
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::BOOL.into()));
        f_type(
            &[TypvalT {
                value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
                ..Default::default()
            }],
            &mut rettv,
        );
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::SPECIAL.into()));
    }

    #[test]
    fn type_of_list_dict_func_blob() {
        use crate::eval::typval_defs::var_type_result;
        let mut rettv = TypvalT::default();
        f_type(&[TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::LIST.into()));
        f_type(&[TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::DICT.into()));
        f_type(&[TypvalT { value: TypvalValue::Func(None), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::FUNC.into()));
        f_type(&[TypvalT { value: TypvalValue::Partial(std::ptr::null_mut()), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::FUNC.into()));
        f_type(&[TypvalT { value: TypvalValue::Blob(std::ptr::null_mut()), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(var_type_result::BLOB.into()));
    }

    // --- f_empty ---

    #[test]
    fn empty_string_and_number_and_float() {
        let mut rettv = TypvalT::default();
        unsafe { f_empty(&[string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { f_empty(&[string(b"x")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { f_empty(&[num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { f_empty(&[num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn empty_returns_a_number_not_a_bool() {
        // A real, historical quirk: empty() returns a plain Number,
        // not a Vimscript Bool, even though it's conceptually boolean.
        let mut rettv = TypvalT::default();
        unsafe { f_empty(&[num(0)], &mut rettv) };
        assert!(matches!(rettv.value, TypvalValue::Number(_)));
    }

    #[test]
    fn empty_list_and_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT::default();
        unsafe { f_empty(&[TypvalT { value: TypvalValue::List(list), ..Default::default() }], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        unsafe { f_empty(&[TypvalT { value: TypvalValue::List(list), ..Default::default() }], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn empty_null_partial_is_empty() {
        let mut rettv = TypvalT::default();
        unsafe {
            f_empty(&[TypvalT { value: TypvalValue::Partial(std::ptr::null_mut()), ..Default::default() }], &mut rettv);
        }
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn empty_special_null_is_empty() {
        let mut rettv = TypvalT::default();
        unsafe {
            f_empty(
                &[TypvalT {
                    value: TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
                    ..Default::default()
                }],
                &mut rettv,
            );
        }
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn empty_bool_true_and_false() {
        let mut rettv = TypvalT::default();
        unsafe {
            f_empty(
                &[TypvalT {
                    value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True),
                    ..Default::default()
                }],
                &mut rettv,
            );
        }
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe {
            f_empty(
                &[TypvalT {
                    value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::False),
                    ..Default::default()
                }],
                &mut rettv,
            );
        }
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    // --- f_and / f_or / f_xor ---

    #[test]
    fn and_of_two_numbers() {
        let mut rettv = TypvalT::default();
        f_and(&[num(0b1100), num(0b1010)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0b1000));
    }

    #[test]
    fn or_of_two_numbers() {
        let mut rettv = TypvalT::default();
        f_or(&[num(0b1100), num(0b1010)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0b1110));
    }

    #[test]
    fn xor_of_two_numbers() {
        let mut rettv = TypvalT::default();
        f_xor(&[num(0b1100), num(0b1010)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0b0110));
    }

    // --- f_abs ---

    #[test]
    fn abs_of_a_positive_and_negative_number() {
        let mut rettv = TypvalT::default();
        f_abs(&[num(5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(5));
        f_abs(&[num(-5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(5));
        f_abs(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn abs_of_a_float() {
        let mut rettv = TypvalT::default();
        f_abs(&[TypvalT { value: TypvalValue::Float(-3.5), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(3.5));
    }

    #[test]
    fn abs_of_varnumber_min_wraps_rather_than_panics() {
        let mut rettv = TypvalT::default();
        f_abs(&[num(crate::eval::typval_defs::VARNUMBER_MIN)], &mut rettv);
        // Matches real two's-complement hardware behavior for the
        // original's own unchecked `-n` on this exact input - see
        // f_abs's own doc comment.
        assert_eq!(rettv.value, TypvalValue::Number(crate::eval::typval_defs::VARNUMBER_MIN));
    }

    #[test]
    fn abs_of_a_non_numeric_type_is_negative_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let list = crate::eval::typval::tv_list_alloc(0);
        f_abs(&[TypvalT { value: TypvalValue::List(list), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-1));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    // --- max_min / f_max / f_min ---

    #[test]
    fn max_of_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
            crate::eval::typval::tv_list_append_number(&mut *list, 7);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_max(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(7));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn min_of_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
            crate::eval::typval::tv_list_append_number(&mut *list, 7);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_min(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn max_of_an_empty_list_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_max(&args, &mut rettv) };
        // rettv is left untouched (0 is max_min's own pre-set default,
        // not written by this crate - the caller is responsible for
        // it, matching call_func's own "default rettv is number zero"
        // precondition; here it's set explicitly to prove that).
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn max_of_a_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item_a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item_a).di_tv.value = TypvalValue::Number(2);
            crate::eval::typval::tv_dict_add(&mut *dict, item_a);
            let item_b = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*item_b).di_tv.value = TypvalValue::Number(9);
            crate::eval::typval::tv_dict_add(&mut *dict, item_b);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_max(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(9));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn min_of_an_empty_dict_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_min(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn max_of_a_non_list_non_dict_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_max(&[string(b"not a list or dict")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    #[test]
    fn max_of_a_list_with_a_non_numeric_item_leaves_rettv_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            let inner = crate::eval::typval::tv_list_alloc(0);
            crate::eval::typval::tv_list_append_list(&mut *list, inner);
        }
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_max(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    // --- f_char2nr ---

    #[test]
    fn char2nr_of_ascii_and_multibyte() {
        let mut rettv = TypvalT::default();
        f_char2nr(&[string(b"A")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(i64::from(b'A')));

        f_char2nr(&[string("日".as_bytes())], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0x65E5));
    }

    #[test]
    fn char2nr_of_an_empty_string_is_zero() {
        let mut rettv = TypvalT::default();
        f_char2nr(&[string(b"")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn char2nr_only_uses_the_first_character() {
        let mut rettv = TypvalT::default();
        f_char2nr(&[string(b"BC")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(i64::from(b'B')));
    }

    #[test]
    fn char2nr_accepts_a_valid_second_argument() {
        let mut rettv = TypvalT::default();
        f_char2nr(&[string(b"A"), num(1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(i64::from(b'A')));
    }

    #[test]
    fn char2nr_rejects_a_non_numeric_second_argument() {
        // tv_check_num's own accepted set includes String (a Vimscript
        // string can coerce to a number) - a genuinely rejected shape
        // needs a List/Dict/Blob/Funcref/Partial instead.
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        f_char2nr(&[string(b"A"), TypvalT { value: TypvalValue::List(list), ..Default::default() }], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    // --- f_nr2char ---

    #[test]
    fn nr2char_of_ascii_and_multibyte() {
        let mut rettv = TypvalT::default();
        f_nr2char(&[num(i64::from(b'A'))], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(Some(b"A".to_vec())));

        f_nr2char(&[num(0x65E5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::String(Some("日".as_bytes().to_vec())));
    }

    #[test]
    fn nr2char_negative_number_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        f_nr2char(&[num(-1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    #[test]
    fn nr2char_too_large_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        f_nr2char(&[num(i64::from(i32::MAX) + 1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- f_str2float ---

    #[test]
    fn str2float_parses_a_plain_float() {
        let mut rettv = TypvalT::default();
        f_str2float(&[string(b"12.375")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(12.375));
    }

    #[test]
    fn str2float_handles_a_leading_sign_and_whitespace() {
        let mut rettv = TypvalT::default();
        f_str2float(&[string(b"  -2.5")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-2.5));

        f_str2float(&[string(b"+ 2.5")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(2.5));
    }

    // --- f_str2nr ---

    #[test]
    fn str2nr_default_base_is_decimal() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"0123")], &mut rettv);
        // Base 10 explicitly does NOT treat a leading zero as octal.
        assert_eq!(rettv.value, TypvalValue::Number(123));
    }

    #[test]
    fn str2nr_negative_decimal() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"-42")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-42));
    }

    #[test]
    fn str2nr_hex_base_ignores_leading_0x() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"0x1A"), num(16)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(26));
    }

    #[test]
    fn str2nr_octal_base_ignores_leading_0o() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"0o17"), num(8)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(15));
    }

    #[test]
    fn str2nr_binary_base_ignores_leading_0b() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"0b101"), num(2)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(5));
    }

    #[test]
    fn str2nr_invalid_base_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        f_str2nr(&[string(b"42"), num(3)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    #[test]
    fn str2nr_quoted_ignores_embedded_single_quotes() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"1'000'000"), num(10), num(1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1_000_000));
    }

    #[test]
    fn str2nr_without_quoted_flag_stops_at_the_quote() {
        let mut rettv = TypvalT::default();
        f_str2nr(&[string(b"1'000"), num(10)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    // --- f_str2list ---

    #[test]
    fn str2list_of_ascii_and_multibyte() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_str2list(&[string("AB日".as_bytes())], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 3);
        unsafe {
            let mut item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(i64::from(b'A')));
            item = (*item).li_next;
            assert_eq!((*item).li_tv.value, TypvalValue::Number(i64::from(b'B')));
            item = (*item).li_next;
            assert_eq!((*item).li_tv.value, TypvalValue::Number(0x65E5));
        }
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn str2list_of_an_empty_string_is_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_str2list(&[string(b"")], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn str2list_stops_at_an_embedded_nul() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_str2list(&[string(b"AB\0CD")], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 2);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    // --- f_list2str ---

    #[test]
    fn list2str_of_ascii_and_multibyte() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, i64::from(b'A'));
            crate::eval::typval::tv_list_append_number(&mut *list, i64::from(b'B'));
            crate::eval::typval::tv_list_append_number(&mut *list, 0x65E5);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_list2str(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some("AB日".as_bytes().to_vec())));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn list2str_of_a_null_list_is_an_empty_string() {
        let mut rettv = TypvalT::default();
        unsafe { f_list2str(&[TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() }], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn list2str_of_a_non_list_is_an_empty_string() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_list2str(&[string(b"not a list")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn str2list_and_list2str_round_trip() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_str2list(&[string("hello 世界".as_bytes())], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };

        let mut rettv2 = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(l), ..Default::default() }];
        unsafe { f_list2str(&args, &mut rettv2) };
        assert_eq!(rettv2.value, TypvalValue::String(Some("hello 世界".as_bytes().to_vec())));

        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    // --- new-builtin table registration ---

    #[test]
    fn new_builtins_are_all_registered() {
        for name in [
            "and",
            "or",
            "xor",
            "abs",
            "max",
            "min",
            "char2nr",
            "nr2char",
            "str2float",
            "str2nr",
            "str2list",
            "list2str",
            "sin",
            "cos",
            "tan",
            "asin",
            "acos",
            "atan",
            "sinh",
            "cosh",
            "tanh",
            "sqrt",
            "exp",
            "log",
            "log10",
            "floor",
            "ceil",
            "round",
            "trunc",
            "atan2",
            "pow",
            "fmod",
            "float2nr",
            "tolower",
            "toupper",
            "trim",
            "has_key",
            "keys",
            "values",
            "items",
            "get",
            "index",
            "reverse",
            "count",
            "copy",
            "deepcopy",
            "add",
            "insert",
            "remove",
            "extend",
            "extendnew",
            "range",
            "repeat",
            "join",
            "flatten",
            "flattennew",
            "localtime",
            "getenv",
            "environ",
            "has",
        ] {
            assert!(find_internal_func(name.as_bytes()).is_some(), "{name} should be registered");
        }
    }

    // --- single-argument float math builtins ---

    #[test]
    fn sin_cos_tan_of_zero() {
        let mut rettv = TypvalT::default();
        f_sin(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
        f_cos(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(1.0));
        f_tan(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    #[test]
    fn asin_acos_atan_of_known_values() {
        let mut rettv = TypvalT::default();
        f_asin(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
        f_acos(&[num(1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
        f_atan(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    #[test]
    fn sinh_cosh_tanh_of_zero() {
        let mut rettv = TypvalT::default();
        f_sinh(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
        f_cosh(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(1.0));
        f_tanh(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    #[test]
    fn sqrt_of_a_perfect_square() {
        let mut rettv = TypvalT::default();
        f_sqrt(&[num(100)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(10.0));
    }

    #[test]
    fn sqrt_of_a_negative_number_is_nan() {
        let mut rettv = TypvalT::default();
        f_sqrt(&[float(-4.0)], &mut rettv);
        let TypvalValue::Float(f) = rettv.value else { panic!("expected a Float") };
        assert!(f.is_nan());
    }

    #[test]
    fn exp_of_zero_is_one() {
        let mut rettv = TypvalT::default();
        f_exp(&[num(0)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(1.0));
    }

    #[test]
    fn log_is_the_natural_logarithm() {
        let mut rettv = TypvalT::default();
        f_log(&[float(1.0_f64.exp())], &mut rettv);
        let TypvalValue::Float(f) = rettv.value else { panic!("expected a Float") };
        assert!((f - 1.0).abs() < 1e-9);
    }

    #[test]
    fn log10_of_known_powers_of_ten() {
        // Epsilon comparison, not exact equality: log10 is a
        // libm-delegated transcendental function whose last-bit
        // rounding can legitimately differ slightly between Miri's
        // interpreter and the native platform's own libm (confirmed:
        // Miri gives log10(1000) as 2.9999999999999996, not exactly
        // 3.0) - this is an execution-environment precision quirk, not
        // a logic bug, so the test itself should tolerate it rather
        // than assert bit-exact equality.
        fn assert_close(actual: &TypvalValue, expected: f64) {
            let TypvalValue::Float(f) = *actual else { panic!("expected a Float") };
            assert!((f - expected).abs() < 1e-9, "{f} not close to {expected}");
        }

        let mut rettv = TypvalT::default();
        f_log10(&[num(1000)], &mut rettv);
        assert_close(&rettv.value, 3.0);
        f_log10(&[float(0.01)], &mut rettv);
        assert_close(&rettv.value, -2.0);
    }

    #[test]
    fn floor_ceil_round_trunc_of_a_fraction() {
        let mut rettv = TypvalT::default();
        f_floor(&[float(1.856)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(1.0));
        f_ceil(&[float(1.856)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(2.0));
        f_round(&[float(4.5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(5.0));
        f_trunc(&[float(1.856)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(1.0));
    }

    #[test]
    fn floor_ceil_round_trunc_of_negative_numbers() {
        // Matches the original's own documented examples exactly:
        // ceil(-5.456) == -5.0, round(-4.5) == -5.0 (away from zero).
        let mut rettv = TypvalT::default();
        f_ceil(&[float(-5.456)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-5.0));
        f_floor(&[float(-5.456)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-6.0));
        f_round(&[float(-4.5)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-5.0));
        f_trunc(&[float(-5.456)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-5.0));
    }

    #[test]
    fn float_op_wrapper_functions_return_zero_for_non_numeric_input() {
        let mut rettv = TypvalT::default();
        f_sin(&[string(b"not a number")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    // --- two-argument float math builtins ---

    #[test]
    fn atan2_of_known_values() {
        let mut rettv = TypvalT::default();
        f_atan2(&[num(0), num(1)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    #[test]
    fn pow_of_known_values() {
        // Epsilon comparison, not exact equality - see
        // log10_of_known_powers_of_ten's own comment: pow(2, 10) is
        // 1024.0000000000002 under Miri's own libm, not exactly
        // 1024.0, a real (if tiny) execution-environment precision
        // difference for this transcendental function, not a bug.
        let mut rettv = TypvalT::default();
        f_pow(&[num(2), num(10)], &mut rettv);
        let TypvalValue::Float(f) = rettv.value else { panic!("expected a Float") };
        assert!((f - 1024.0).abs() < 1e-9, "{f} not close to 1024.0");
    }

    #[test]
    fn fmod_matches_truncating_c_style_remainder_not_ieee_remainder() {
        // fmod(-5.0, 3.0) == -2.0 (truncating division, sign matches
        // the dividend) - this is the ONE input shape that would
        // distinguish fmod from f64::rem_euclid (which would give 1.0)
        // and from the IEEE round-to-even `remainder()` operation
        // (which would also give 1.0 here) - proves this crate's `%`
        // really does match C's fmod, not a different remainder op.
        let mut rettv = TypvalT::default();
        f_fmod(&[float(-5.0), num(3)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(-2.0));
        f_fmod(&[float(5.0), num(3)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(2.0));
    }

    #[test]
    fn two_arg_float_functions_return_zero_when_either_argument_is_non_numeric() {
        let mut rettv = TypvalT::default();
        f_pow(&[num(2), string(b"x")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
        f_pow(&[string(b"x"), num(2)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Float(0.0));
    }

    // --- f_float2nr ---

    #[test]
    fn float2nr_truncates_toward_zero() {
        let mut rettv = TypvalT::default();
        f_float2nr(&[float(3.9)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(3));
        f_float2nr(&[float(-3.9)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-3));
    }

    #[test]
    fn float2nr_clamps_a_huge_positive_float_to_varnumber_max() {
        let mut rettv = TypvalT::default();
        f_float2nr(&[float(1e300)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(crate::eval::typval_defs::VARNUMBER_MAX));
    }

    #[test]
    fn float2nr_clamps_a_huge_negative_float_to_negative_varnumber_max() {
        let mut rettv = TypvalT::default();
        f_float2nr(&[float(-1e300)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(-crate::eval::typval_defs::VARNUMBER_MAX));
    }

    #[test]
    fn float2nr_non_numeric_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        f_float2nr(&[string(b"not a number")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- f_tolower / f_toupper ---

    #[test]
    fn tolower_converts_ascii_and_multibyte() {
        let mut rettv = TypvalT::default();
        unsafe { f_tolower(&[string(b"Hello WORLD")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello world".to_vec())));

        unsafe { f_tolower(&[string("HÉLLO".as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some("héllo".as_bytes().to_vec())));
    }

    #[test]
    fn toupper_converts_ascii_and_multibyte() {
        let mut rettv = TypvalT::default();
        unsafe { f_toupper(&[string(b"Hello world")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"HELLO WORLD".to_vec())));

        unsafe { f_toupper(&[string("héllo".as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some("HÉLLO".as_bytes().to_vec())));
    }

    #[test]
    fn tolower_toupper_of_an_empty_string() {
        let mut rettv = TypvalT::default();
        unsafe { f_tolower(&[string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(Vec::new())));
        unsafe { f_toupper(&[string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(Vec::new())));
    }

    // --- f_trim ---

    #[test]
    fn trim_default_mask_trims_both_ends() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string(b"  hello world  ")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello world".to_vec())));
    }

    #[test]
    fn trim_default_mask_trims_non_breaking_space_too() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string("\u{a0}hi\u{a0}".as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hi".to_vec())));
    }

    #[test]
    fn trim_leading_only() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string(b"  hello  "), string(b" "), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello  ".to_vec())));
    }

    #[test]
    fn trim_trailing_only() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string(b"  hello  "), string(b" "), num(2)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"  hello".to_vec())));
    }

    #[test]
    fn trim_custom_mask() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string(b"xxhelloxx"), string(b"x")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    fn trim_empty_mask_falls_back_to_default() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string(b"  hi  "), string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hi".to_vec())));
    }

    #[test]
    fn trim_invalid_dir_resets_rettv_to_an_empty_string() {
        // Unlike most other builtins here (which leave rettv at its
        // caller-provided default on error), f_trim's own C original
        // unconditionally resets rettv to an empty String BEFORE any
        // argument validation (`rettv->v_type = VAR_STRING; rettv->
        // vval.v_string = NULL;` at the very top) - so an invalid dir
        // still leaves rettv String(None), not the pre-call value.
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_trim(&[string(b"hi"), string(b" "), num(3)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn trim_dir_is_ignored_when_mask_argument_is_absent() {
        // dir is only ever consulted when mask was ACTUALLY passed as a
        // real String - this can't literally be exercised with a 3rd
        // positional arg without a 2nd (max_argc requires args in
        // order), so this documents the invariant via the table's own
        // arity instead: trim() cannot be called with a dir but no
        // mask at all, matching the original's own argument order.
        assert_eq!(find_internal_func(b"trim").unwrap().min_argc, 1);
    }

    #[test]
    fn trim_multibyte_text_and_mask() {
        let mut rettv = TypvalT::default();
        unsafe { f_trim(&[string("日日hello日日".as_bytes()), string("日".as_bytes())], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    // --- f_has_key ---

    #[test]
    fn has_key_finds_a_present_key_and_rejects_a_missing_one() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string(b"a")];
        f_has_key(&args, &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));

        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string(b"nope")];
        f_has_key(&args, &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));

        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn has_key_null_dict_is_false() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [
            TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() },
            string(b"a"),
        ];
        f_has_key(&args, &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- f_keys / f_values / f_items ---

    fn make_test_dict() -> *mut crate::eval::typval_defs::DictT {
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item_a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item_a).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item_a);
            let item_b = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*item_b).di_tv.value = TypvalValue::Number(2);
            crate::eval::typval::tv_dict_add(&mut *dict, item_b);
        }
        dict
    }

    #[test]
    fn keys_returns_a_list_of_key_strings() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = make_test_dict();
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_keys(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 2);
        let mut keys: Vec<Vec<u8>> = Vec::new();
        unsafe {
            let mut item = crate::eval::typval::tv_list_first(l);
            while !item.is_null() {
                let TypvalValue::String(Some(s)) = &(*item).li_tv.value else { panic!("expected a String") };
                keys.push(s.clone());
                item = (*item).li_next;
            }
        }
        keys.sort();
        assert_eq!(keys, vec![b"a".to_vec(), b"b".to_vec()]);
        unsafe {
            crate::eval::typval::tv_list_unref(l);
            crate::eval::typval::tv_dict_unref(dict);
        }
    }

    #[test]
    fn values_returns_a_list_of_the_dicts_own_values() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = make_test_dict();
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_values(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 2);
        let mut values: Vec<crate::eval::typval_defs::VarnumberT> = Vec::new();
        unsafe {
            let mut item = crate::eval::typval::tv_list_first(l);
            while !item.is_null() {
                let TypvalValue::Number(n) = (*item).li_tv.value else { panic!("expected a Number") };
                values.push(n);
                item = (*item).li_next;
            }
        }
        values.sort_unstable();
        assert_eq!(values, vec![1, 2]);
        unsafe {
            crate::eval::typval::tv_list_unref(l);
            crate::eval::typval::tv_dict_unref(dict);
        }
    }

    #[test]
    fn items_returns_a_list_of_key_value_pairs() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item_a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item_a).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item_a);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_items(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 1);
        unsafe {
            let pair_item = crate::eval::typval::tv_list_first(l);
            let TypvalValue::List(pair) = (*pair_item).li_tv.value else { panic!("expected a List") };
            assert_eq!(crate::eval::typval::tv_list_len(pair), 2);
            let key_item = crate::eval::typval::tv_list_first(pair);
            assert_eq!((*key_item).li_tv.value, TypvalValue::String(Some(b"a".to_vec())));
            let value_item = (*key_item).li_next;
            assert_eq!((*value_item).li_tv.value, TypvalValue::Number(1));

            crate::eval::typval::tv_list_unref(l);
            crate::eval::typval::tv_dict_unref(dict);
        }
    }

    #[test]
    fn keys_values_items_of_a_null_dict_are_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let args = [TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() }];
        for f in [f_keys, f_values] {
            let mut rettv = TypvalT::default();
            unsafe { f(&args, &mut rettv) };
            let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
            assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
            unsafe { crate::eval::typval::tv_list_unref(l) };
        }
    }

    #[test]
    fn keys_of_a_non_dict_is_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_keys(&[num(5)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn items_of_a_list_argument_is_unimplemented() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        let mut rettv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            f_items(&args, &mut rettv);
        }));
        assert!(result.is_err(), "expected a panic (tv_list2items not yet translated)");
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    // --- f_get ---

    #[test]
    fn get_from_a_list_in_range_and_out_of_range() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 10);
            crate::eval::typval::tv_list_append_number(&mut *list, 20);
            crate::eval::typval::tv_list_append_number(&mut *list, 30);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(20));

        // Negative index counts from the end.
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(-1)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(30));

        // Out of range with a default.
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(99), string(b"missing")];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"missing".to_vec())));

        // Out of range without a default.
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(99)];
        rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));

        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn get_from_a_null_list_uses_the_default() {
        let mut rettv = TypvalT::default();
        let args = [
            TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() },
            num(0),
            num(42),
        ];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(42));
    }

    #[test]
    fn get_from_a_dict_present_and_missing_key() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item).di_tv.value = TypvalValue::Number(7);
            crate::eval::typval::tv_dict_add(&mut *dict, item);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string(b"a")];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(7));

        let args =
            [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string(b"missing"), num(-1)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));

        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn get_from_a_blob_in_range_and_out_of_range() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![10, 20, 30];
            (*blob).bv_ga.ga_len = 3;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(1)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(20));

        // Out of range with no default -> -1 (blob's own sentinel).
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(99)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));

        // Out of range WITH a default -> the default overrides the -1
        // sentinel, matching the original's own shared tail exactly.
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(99), num(77)];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(77));

        unsafe { crate::eval::typval::tv_blob_free(blob) };
    }

    #[test]
    fn get_of_a_non_container_falls_back_to_the_default() {
        let mut rettv = TypvalT::default();
        let args = [num(5), num(0), string(b"fallback")];
        unsafe { f_get(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"fallback".to_vec())));
    }

    #[test]
    fn get_of_a_funcref_is_unimplemented() {
        let args = [TypvalT { value: TypvalValue::Func(Some(b"len".to_vec())), ..Default::default() }, string(b"name")];
        let mut rettv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            f_get(&args, &mut rettv);
        }));
        assert!(result.is_err(), "expected a panic (get_func_arity/partial introspection not yet translated)");
    }

    // --- f_index ---

    #[test]
    fn index_finds_a_present_number_in_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 10);
            crate::eval::typval::tv_list_append_number(&mut *list, 20);
            crate::eval::typval::tv_list_append_number(&mut *list, 30);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(20)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn index_of_a_missing_value_is_negative_one() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(99)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn index_with_a_start_position() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(4);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 5);
            crate::eval::typval::tv_list_append_number(&mut *list, 5);
            crate::eval::typval::tv_list_append_number(&mut *list, 5);
            crate::eval::typval::tv_list_append_number(&mut *list, 5);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(5), num(2)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn index_case_insensitive_string_match() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_string(list, Some(b"Hello")) };
        let mut rettv = TypvalT::default();
        // Case-sensitive: no match.
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, string(b"hello"), num(0)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
        // Case-insensitive: matches.
        let args =
            [TypvalT { value: TypvalValue::List(list), ..Default::default() }, string(b"hello"), num(0), num(1)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn index_in_a_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![10, 20, 30];
            (*blob).bv_ga.ga_len = 3;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(20)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));

        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(99)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));

        unsafe { crate::eval::typval::tv_blob_free(blob) };
    }

    #[test]
    fn index_of_a_non_list_non_blob_is_negative_one() {
        let mut rettv = TypvalT::default();
        unsafe { f_index(&[string(b"not a list"), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn index_of_a_null_list_is_negative_one() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() }, num(1)];
        unsafe { f_index(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    // --- f_reverse / reverse_text ---

    #[test]
    fn reverse_text_reverses_ascii() {
        let result = unsafe { reverse_text(b"hello") };
        assert_eq!(result, b"olleh");
    }

    #[test]
    fn reverse_text_keeps_multibyte_characters_intact() {
        let result = unsafe { reverse_text("ab日本cd".as_bytes()) };
        assert_eq!(result, "dc本日ba".as_bytes());
    }

    #[test]
    fn reverse_text_stops_at_an_embedded_nul() {
        let result = unsafe { reverse_text(b"ab\0cd") };
        assert_eq!(result, b"ba");
    }

    #[test]
    fn reverse_of_a_string() {
        let mut rettv = TypvalT::default();
        unsafe { f_reverse(&[string(b"hello")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"olleh".to_vec())));
    }

    #[test]
    fn reverse_of_a_null_string() {
        let mut rettv = TypvalT::default();
        unsafe { f_reverse(&[TypvalT { value: TypvalValue::String(None), ..Default::default() }], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn reverse_of_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_reverse(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(l, list); // reversed in place, same pointer returned.
        unsafe {
            let mut item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(3));
            item = (*item).li_next;
            assert_eq!((*item).li_tv.value, TypvalValue::Number(2));
            item = (*item).li_next;
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn reverse_of_a_locked_list_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
            (*list).lv_lock = crate::eval::typval_defs::VarLockStatus::Locked;
        }
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_reverse(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            let item = crate::eval::typval::tv_list_first(list);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1)); // unchanged order.
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn reverse_of_a_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![1, 2, 3, 4];
            (*blob).bv_ga.ga_len = 4;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }];
        unsafe { f_reverse(&args, &mut rettv) };
        let TypvalValue::Blob(b) = rettv.value else { panic!("expected a Blob") };
        assert_eq!(b, blob);
        unsafe {
            assert_eq!((*b).bv_ga.ga_data, vec![4, 3, 2, 1]);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn reverse_of_an_odd_length_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![1, 2, 3];
            (*blob).bv_ga.ga_len = 3;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }];
        unsafe { f_reverse(&args, &mut rettv) };
        unsafe {
            assert_eq!((*blob).bv_ga.ga_data, vec![3, 2, 1]);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn reverse_of_a_number_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_reverse(&[num(5)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- f_count ---

    #[test]
    fn count_in_a_string_case_sensitive_and_insensitive() {
        let mut rettv = TypvalT::default();
        unsafe { f_count(&[string(b"ababab"), string(b"ab")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(3));

        unsafe { f_count(&[string(b"ABabAB"), string(b"ab")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));

        unsafe { f_count(&[string(b"ABabAB"), string(b"ab"), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(3));
    }

    #[test]
    fn count_in_a_string_with_an_empty_needle_is_zero() {
        let mut rettv = TypvalT::default();
        unsafe { f_count(&[string(b"hello"), string(b"")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn count_in_a_string_stops_at_an_embedded_nul() {
        let mut rettv = TypvalT::default();
        unsafe { f_count(&[string(b"ab\0abab"), string(b"ab")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn count_in_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(4);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1)];
        unsafe { f_count(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(3));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn count_in_a_list_starting_at_an_index() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(4);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
        }
        let mut rettv = TypvalT::default();
        // ic=0, start=2 - both must be given for start to take effect.
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1), num(0), num(2)];
        unsafe { f_count(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn count_in_a_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item_a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item_a).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item_a);
            let item_b = crate::eval::typval::tv_dict_item_alloc(b"b");
            (*item_b).di_tv.value = TypvalValue::Number(2);
            crate::eval::typval::tv_dict_add(&mut *dict, item_b);
            let item_c = crate::eval::typval::tv_dict_item_alloc(b"c");
            (*item_c).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item_c);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, num(1)];
        unsafe { f_count(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(2));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn count_in_a_dict_with_both_ic_and_start_is_an_error() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, item);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, num(1), num(0), num(0)];
        unsafe { f_count(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn count_of_a_null_dict_is_zero() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() }, num(1)];
        unsafe { f_count(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn count_of_a_non_container_is_zero() {
        let mut rettv = TypvalT::default();
        unsafe { f_count(&[num(5), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_copy ---

    #[test]
    fn copy_of_a_list_is_a_genuinely_separate_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_copy(&args, &mut rettv) };
        let TypvalValue::List(copy) = rettv.value else { panic!("expected a List") };
        assert_ne!(copy, list); // a genuinely separate list, not an alias.
        assert_eq!(rettv.v_lock, crate::eval::typval_defs::VarLockStatus::Unlocked);
        unsafe {
            assert_eq!((*copy).lv_refcount, 1); // set exactly once, not double-counted.
            assert_eq!((*list).lv_refcount, 0); // the original's own refcount is untouched.

            // Mutating the copy must not affect the original.
            let item = crate::eval::typval::tv_list_first(copy);
            (*item).li_tv.value = TypvalValue::Number(99);
            let orig_item = crate::eval::typval::tv_list_first(list);
            assert_eq!((*orig_item).li_tv.value, TypvalValue::Number(1));

            crate::eval::typval::tv_list_unref(copy);
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn copy_of_a_null_list_is_a_null_list() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() }];
        unsafe { f_copy(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::List(std::ptr::null_mut()));
    }

    #[test]
    fn copy_of_a_dict_is_a_genuinely_separate_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = make_test_dict();
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }];
        unsafe { f_copy(&args, &mut rettv) };
        let TypvalValue::Dict(copy) = rettv.value else { panic!("expected a Dict") };
        assert_ne!(copy, dict); // a genuinely separate dict, not an alias.
        assert_eq!(rettv.v_lock, crate::eval::typval_defs::VarLockStatus::Unlocked);
        unsafe {
            assert_eq!((*copy).dv_refcount, 1); // set exactly once, not double-counted.
            assert_eq!((*dict).dv_refcount, 0); // the original's own refcount is untouched.
            assert_eq!(crate::eval::typval::tv_dict_len(copy.as_ref()), 2);

            // Mutating the copy must not affect the original.
            let item = crate::eval::typval::tv_dict_find(Some(&mut *copy), b"a").unwrap();
            (*item).di_tv.value = TypvalValue::Number(99);
            let orig_item = crate::eval::typval::tv_dict_find(Some(&mut *dict), b"a").unwrap();
            assert_eq!((*orig_item).di_tv.value, TypvalValue::Number(1));

            crate::eval::typval::tv_dict_unref(copy);
            crate::eval::typval::tv_dict_unref(dict);
        }
    }

    #[test]
    fn copy_of_a_null_dict_is_a_null_dict() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() }];
        unsafe { f_copy(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Dict(std::ptr::null_mut()));
    }

    #[test]
    fn copy_of_a_blob_is_a_genuinely_separate_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![1, 2, 3];
            (*blob).bv_ga.ga_len = 3;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }];
        unsafe { f_copy(&args, &mut rettv) };
        let TypvalValue::Blob(copy) = rettv.value else { panic!("expected a Blob") };
        assert_ne!(copy, blob); // a genuinely separate blob, not an alias.
        unsafe {
            assert_eq!((*copy).bv_ga.ga_data, vec![1, 2, 3]);
            assert_eq!((*copy).bv_refcount, 1);
            assert_eq!((*blob).bv_refcount, 0);

            // Mutating the copy must not affect the original.
            (&mut (*copy).bv_ga.ga_data)[0] = 99;
            assert_eq!((&(*blob).bv_ga.ga_data)[0], 1);

            crate::eval::typval::tv_blob_free(copy);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn copy_of_a_number_is_a_plain_value_copy() {
        let mut rettv = TypvalT::default();
        unsafe { f_copy(&[num(42)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(42));
    }

    #[test]
    fn copy_of_a_string_is_a_plain_value_copy() {
        let mut rettv = TypvalT::default();
        unsafe { f_copy(&[string(b"hello")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "f_copy(UNKNOWN)")]
    fn copy_of_unknown_panics_in_debug() {
        let mut rettv = TypvalT::default();
        unsafe { f_copy(&[TypvalT::default()], &mut rettv) };
    }

    // --- f_deepcopy ---

    #[test]
    fn deepcopy_of_a_nested_list_copies_recursively() {
        let _lock = crate::globals::global_state_test_lock();
        let inner = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_ref(inner);
            crate::eval::typval::tv_list_append_number(&mut *inner, 1);
        }
        let outer = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_append_owned_tv(
                outer,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            )
        };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(outer), ..Default::default() }];
        unsafe { f_deepcopy(&args, &mut rettv) };
        let TypvalValue::List(outer_copy) = rettv.value else { panic!("expected a List") };
        assert_ne!(outer_copy, outer);
        unsafe {
            let item = crate::eval::typval::tv_list_first(outer_copy);
            let TypvalValue::List(inner_copy) = (*item).li_tv.value else { panic!("expected a List") };
            // Deep copy: the nested list is ALSO a genuinely separate
            // copy, not the same pointer as the original's own inner
            // list (contrast copy()'s own shallow behavior).
            assert_ne!(inner_copy, inner);

            // Mutating the nested copy must not affect the nested
            // original.
            let inner_copy_item = crate::eval::typval::tv_list_first(inner_copy);
            (*inner_copy_item).li_tv.value = TypvalValue::Number(99);
            let inner_orig_item = crate::eval::typval::tv_list_first(inner);
            assert_eq!((*inner_orig_item).li_tv.value, TypvalValue::Number(1));

            crate::eval::typval::tv_list_unref(outer);
            crate::eval::typval::tv_list_unref(outer_copy);
        }
    }

    #[test]
    fn deepcopy_of_a_nested_dict_copies_recursively() {
        let _lock = crate::globals::global_state_test_lock();
        let inner = crate::eval::typval::tv_dict_alloc();
        unsafe {
            (*inner).dv_refcount += 1;
            let item = crate::eval::typval::tv_dict_item_alloc(b"x");
            (*item).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *inner, item);
        }
        let outer = crate::eval::typval::tv_dict_alloc();
        unsafe {
            let item = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*item).di_tv.value = TypvalValue::Dict(inner);
            crate::eval::typval::tv_dict_add(&mut *outer, item);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(outer), ..Default::default() }];
        unsafe { f_deepcopy(&args, &mut rettv) };
        let TypvalValue::Dict(outer_copy) = rettv.value else { panic!("expected a Dict") };
        assert_ne!(outer_copy, outer);
        unsafe {
            let item = crate::eval::typval::tv_dict_find(Some(&mut *outer_copy), b"a").unwrap();
            let TypvalValue::Dict(inner_copy) = (*item).di_tv.value else { panic!("expected a Dict") };
            assert_ne!(inner_copy, inner);

            let inner_copy_item = crate::eval::typval::tv_dict_find(Some(&mut *inner_copy), b"x").unwrap();
            (*inner_copy_item).di_tv.value = TypvalValue::Number(99);
            let inner_orig_item = crate::eval::typval::tv_dict_find(Some(&mut *inner), b"x").unwrap();
            assert_eq!((*inner_orig_item).di_tv.value, TypvalValue::Number(1));

            crate::eval::typval::tv_dict_unref(outer);
            crate::eval::typval::tv_dict_unref(outer_copy);
        }
    }

    #[test]
    fn deepcopy_without_noref_reuses_the_same_copy_for_a_shared_reference() {
        // deepcopy(x) (noref omitted, defaults to 0): the SAME list
        // referenced twice produces the SAME copy both times.
        let _lock = crate::globals::global_state_test_lock();
        let inner = crate::eval::typval::tv_list_alloc(0);
        unsafe { crate::eval::typval::tv_list_ref(inner) };
        let outer = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_owned_tv(
                outer,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
            crate::eval::typval::tv_list_ref(inner);
            crate::eval::typval::tv_list_append_owned_tv(
                outer,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(outer), ..Default::default() }];
        unsafe { f_deepcopy(&args, &mut rettv) };
        let TypvalValue::List(outer_copy) = rettv.value else { panic!("expected a List") };
        unsafe {
            let first = crate::eval::typval::tv_list_first(outer_copy);
            let second = (*first).li_next;
            let TypvalValue::List(first_copy) = (*first).li_tv.value else { panic!("expected a List") };
            let TypvalValue::List(second_copy) = (*second).li_tv.value else { panic!("expected a List") };
            assert_eq!(first_copy, second_copy);

            crate::eval::typval::tv_list_unref(outer);
            crate::eval::typval::tv_list_unref(outer_copy);
        }
    }

    #[test]
    fn deepcopy_with_noref_1_makes_separate_copies_for_a_shared_reference() {
        // deepcopy(x, 1): every occurrence of the same referenced
        // list gets its OWN separate copy instead.
        let _lock = crate::globals::global_state_test_lock();
        let inner = crate::eval::typval::tv_list_alloc(0);
        unsafe { crate::eval::typval::tv_list_ref(inner) };
        let outer = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_owned_tv(
                outer,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
            crate::eval::typval::tv_list_ref(inner);
            crate::eval::typval::tv_list_append_owned_tv(
                outer,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(outer), ..Default::default() }, num(1)];
        unsafe { f_deepcopy(&args, &mut rettv) };
        let TypvalValue::List(outer_copy) = rettv.value else { panic!("expected a List") };
        unsafe {
            let first = crate::eval::typval::tv_list_first(outer_copy);
            let second = (*first).li_next;
            let TypvalValue::List(first_copy) = (*first).li_tv.value else { panic!("expected a List") };
            let TypvalValue::List(second_copy) = (*second).li_tv.value else { panic!("expected a List") };
            assert_ne!(first_copy, second_copy);

            crate::eval::typval::tv_list_unref(outer);
            crate::eval::typval::tv_list_unref(outer_copy);
        }
    }

    #[test]
    fn deepcopy_of_a_number_is_a_plain_value_copy() {
        let mut rettv = TypvalT::default();
        unsafe { f_deepcopy(&[num(42)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(42));
    }

    #[test]
    fn deepcopy_with_only_the_required_argument_does_not_panic() {
        // argvars.len() == 1 - the optional noref argument is simply
        // absent, not an in-bounds Unknown-typed sentinel like the
        // original's own fixed-size argvars array would have.
        let mut rettv = TypvalT::default();
        unsafe { f_deepcopy(&[num(7)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(7));
    }

    // --- f_add ---

    #[test]
    fn add_appends_to_a_list_and_returns_the_same_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        // Not ref'd upfront - f_add's own successful-path tv_copy(&argvars[0],
        // rettv) is the ONE reference this test needs to release at
        // the end, matching f_reverse's own already-established test
        // pattern (tv_list_set_ret/tv_copy both increment refcount
        // exactly once on their own success path).
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(2)];
        unsafe { f_add(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(l, list); // same list, mutated in place.
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 2);
            let item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            let item2 = (*item).li_next;
            assert_eq!((*item2).li_tv.value, TypvalValue::Number(2));
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn add_appends_a_list_as_a_single_nested_item_not_concatenated() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        // `inner` is not ref'd upfront either - f_add's own
        // tv_list_append_tv(l, &argvars[1]) internally tv_copy's
        // argvars[1] (List(inner)), incrementing inner's refcount
        // exactly once; unreffing `list` at the end (which then frees
        // it, given `list` itself is also only ref'd via f_add's own
        // rettv copy) cascades into releasing that same reference via
        // tv_list_free_contents, so inner needs no separate unref here.
        let inner = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT::default();
        let args = [
            TypvalT { value: TypvalValue::List(list), ..Default::default() },
            TypvalT { value: TypvalValue::List(inner), ..Default::default() },
        ];
        unsafe { f_add(&args, &mut rettv) };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(list), 2);
            let item = crate::eval::typval::tv_list_first(list);
            let item2 = (*item).li_next;
            let TypvalValue::List(nested) = (*item2).li_tv.value else { panic!("expected a nested List") };
            assert_eq!(nested, inner); // appended by reference, not flattened.
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn add_to_a_locked_list_leaves_it_untouched_and_returns_1() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        unsafe {
            crate::eval::typval::tv_list_ref(list);
            (*list).lv_lock = crate::eval::typval_defs::VarLockStatus::Locked;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1)];
        unsafe { f_add(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1)); // failed.
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(list), 0);
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn add_appends_a_byte_to_a_blob_and_returns_the_same_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_refcount += 1;
            (*blob).bv_ga.ga_data = vec![1, 2];
            (*blob).bv_ga.ga_len = 2;
            (*blob).bv_ga.ga_maxlen = 2;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(3)];
        unsafe { f_add(&args, &mut rettv) };
        let TypvalValue::Blob(b) = rettv.value else { panic!("expected a Blob") };
        assert_eq!(b, blob);
        unsafe {
            // ga_grow may over-allocate ga_data's own capacity beyond
            // ga_len (matching the original growarray's own amortized-
            // growth strategy) - tv_blob_len/tv_blob_get, not a raw
            // ga_data comparison, are the correct way to inspect a
            // blob's own LOGICAL contents.
            assert_eq!(crate::eval::typval::tv_blob_len(b), 3);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 0), 1);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 1), 2);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 2), 3);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn add_to_a_null_blob_returns_1() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(std::ptr::null_mut()), ..Default::default() }, num(1)];
        unsafe { f_add(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn add_to_a_non_list_non_blob_returns_1() {
        let mut rettv = TypvalT::default();
        unsafe { f_add(&[num(5), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    // --- f_insert ---

    #[test]
    fn insert_into_a_list_at_the_start_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 2) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(l, list);
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 2);
            let item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            let item2 = (*item).li_next;
            assert_eq!((*item2).li_tv.value, TypvalValue::Number(2));
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn insert_into_a_list_before_a_given_index() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(2), num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        unsafe {
            let item = crate::eval::typval::tv_list_first(list);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            let item2 = (*item).li_next;
            assert_eq!((*item2).li_tv.value, TypvalValue::Number(2));
            let item3 = (*item2).li_next;
            assert_eq!((*item3).li_tv.value, TypvalValue::Number(3));
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn insert_at_the_end_when_idx_equals_length() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(2), num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(list), 2);
            let item = crate::eval::typval::tv_list_first(list);
            let item2 = (*item).li_next;
            assert_eq!((*item2).li_tv.value, TypvalValue::Number(2));
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn insert_into_a_locked_list_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        unsafe { (*list).lv_lock = crate::eval::typval_defs::VarLockStatus::Locked };
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999)); // untouched.
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(list), 0);
            crate::eval::typval::tv_list_unref(list);
        }
    }

    #[test]
    fn insert_with_an_out_of_range_index_leaves_rettv_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(1), num(5)];
        unsafe { f_insert(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn insert_into_a_blob_at_the_start_by_default() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![2, 3];
            (*blob).bv_ga.ga_len = 2;
            (*blob).bv_ga.ga_maxlen = 2;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        let TypvalValue::Blob(b) = rettv.value else { panic!("expected a Blob") };
        assert_eq!(b, blob);
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(b), 3);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 0), 1);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 1), 2);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 2), 3);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn insert_into_a_blob_before_a_given_index() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![1, 3];
            (*blob).bv_ga.ga_len = 2;
            (*blob).bv_ga.ga_maxlen = 2;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(2), num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(blob), 3);
            assert_eq!(crate::eval::typval::tv_blob_get(blob, 0), 1);
            assert_eq!(crate::eval::typval::tv_blob_get(blob, 1), 2);
            assert_eq!(crate::eval::typval::tv_blob_get(blob, 2), 3);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn insert_into_a_locked_blob_leaves_it_untouched() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_lock = crate::eval::typval_defs::VarLockStatus::Locked;
            (*blob).bv_ga.ga_data = vec![1];
            (*blob).bv_ga.ga_len = 1;
        }
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(2)];
        unsafe { f_insert(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(blob), 1);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn insert_with_an_out_of_range_blob_value_leaves_rettv_untouched() {
        let blob = crate::eval::typval::tv_blob_alloc();
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(256)];
        unsafe { f_insert(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe { crate::eval::typval::tv_blob_free(blob) };
    }

    #[test]
    fn insert_into_a_null_blob_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::Blob(std::ptr::null_mut()), ..Default::default() }, num(1)];
        unsafe { f_insert(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    #[test]
    fn insert_into_a_non_list_non_blob_leaves_rettv_at_default() {
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        unsafe { f_insert(&[num(5), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_remove ---
    //
    // f_remove is a thin dispatch wrapper over
    // crate::eval::typval::tv_list_remove/tv_blob_remove/tv_dict_remove,
    // each already thoroughly tested directly in typval.rs's own test
    // module - these tests focus on confirming the dispatch itself
    // routes to the right one, not re-covering every edge case again.

    #[test]
    fn remove_dispatches_to_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(0)];
        unsafe { f_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(list), 1);
            crate::eval::typval::tv_list_free(list);
        }
    }

    #[test]
    fn remove_dispatches_to_a_blob() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![10, 20];
            (*blob).bv_ga.ga_len = 2;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(0)];
        unsafe { f_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(10));
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(blob), 1);
            crate::eval::typval::tv_blob_free(blob);
        }
    }

    #[test]
    fn remove_dispatches_to_a_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, b"a", 42) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Dict(dict), ..Default::default() }, string(b"a")];
        unsafe { f_remove(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(42));
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn remove_on_a_non_list_non_dict_non_blob_leaves_rettv_at_default() {
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        unsafe { f_remove(&[num(5), num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // --- f_extend / f_extendnew ---

    #[test]
    fn extend_merges_two_lists_in_place() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *l1, 1);
            crate::eval::typval::tv_list_append_number(&mut *l1, 2);
        }
        let l2 = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *l2, 3);
            crate::eval::typval::tv_list_append_number(&mut *l2, 4);
        }
        let mut rettv = TypvalT::default();
        let args =
            [TypvalT { value: TypvalValue::List(l1), ..Default::default() }, TypvalT { value: TypvalValue::List(l2), ..Default::default() }];
        unsafe { f_extend(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(l, l1); // same list, mutated in place.
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 4);
            let mut vals = Vec::new();
            let mut item = crate::eval::typval::tv_list_first(l);
            while !item.is_null() {
                vals.push((*item).li_tv.value.clone());
                item = (*item).li_next;
            }
            assert_eq!(vals, vec![TypvalValue::Number(1), TypvalValue::Number(2), TypvalValue::Number(3), TypvalValue::Number(4)]);
            crate::eval::typval::tv_list_unref(l1);
            crate::eval::typval::tv_list_unref(l2);
        }
    }

    #[test]
    fn extend_inserts_before_a_given_index() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *l1, 1);
            crate::eval::typval::tv_list_append_number(&mut *l1, 4);
        }
        let l2 = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *l2, 2);
            crate::eval::typval::tv_list_append_number(&mut *l2, 3);
        }
        let mut rettv = TypvalT::default();
        let args = [
            TypvalT { value: TypvalValue::List(l1), ..Default::default() },
            TypvalT { value: TypvalValue::List(l2), ..Default::default() },
            num(1),
        ];
        unsafe { f_extend(&args, &mut rettv) };
        unsafe {
            let mut vals = Vec::new();
            let mut item = crate::eval::typval::tv_list_first(l1);
            while !item.is_null() {
                vals.push((*item).li_tv.value.clone());
                item = (*item).li_next;
            }
            assert_eq!(vals, vec![TypvalValue::Number(1), TypvalValue::Number(2), TypvalValue::Number(3), TypvalValue::Number(4)]);
            crate::eval::typval::tv_list_unref(l1);
            crate::eval::typval::tv_list_unref(l2);
        }
    }

    #[test]
    fn extend_of_a_locked_list_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = crate::eval::typval::tv_list_alloc(0);
        unsafe { (*l1).lv_lock = crate::eval::typval_defs::VarLockStatus::Locked };
        let l2 = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args =
            [TypvalT { value: TypvalValue::List(l1), ..Default::default() }, TypvalT { value: TypvalValue::List(l2), ..Default::default() }];
        unsafe { f_extend(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            crate::eval::typval::tv_list_unref(l1);
            crate::eval::typval::tv_list_unref(l2);
        }
    }

    #[test]
    fn extend_merges_two_dicts_with_default_force_action() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d1, b"a", 1) };
        let d2 = crate::eval::typval::tv_dict_alloc();
        unsafe {
            crate::eval::typval::tv_dict_add_nr(&mut *d2, b"a", 2);
            crate::eval::typval::tv_dict_add_nr(&mut *d2, b"b", 3);
        }
        let mut rettv = TypvalT::default();
        let args =
            [TypvalT { value: TypvalValue::Dict(d1), ..Default::default() }, TypvalT { value: TypvalValue::Dict(d2), ..Default::default() }];
        unsafe { f_extend(&args, &mut rettv) };
        let TypvalValue::Dict(d) = rettv.value else { panic!("expected a Dict") };
        assert_eq!(d, d1);
        unsafe {
            let a = crate::eval::typval::tv_dict_find(Some(&mut *d1), b"a").unwrap();
            assert_eq!((*a).di_tv.value, TypvalValue::Number(2)); // overwritten by "force".
            let b = crate::eval::typval::tv_dict_find(Some(&mut *d1), b"b").unwrap();
            assert_eq!((*b).di_tv.value, TypvalValue::Number(3));
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extend_dict_with_keep_action_preserves_existing_values() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d1, b"a", 1) };
        let d2 = crate::eval::typval::tv_dict_alloc();
        unsafe {
            crate::eval::typval::tv_dict_add_nr(&mut *d2, b"a", 2);
            crate::eval::typval::tv_dict_add_nr(&mut *d2, b"b", 3);
        }
        let mut rettv = TypvalT::default();
        let args = [
            TypvalT { value: TypvalValue::Dict(d1), ..Default::default() },
            TypvalT { value: TypvalValue::Dict(d2), ..Default::default() },
            string(b"keep"),
        ];
        unsafe { f_extend(&args, &mut rettv) };
        unsafe {
            let a = crate::eval::typval::tv_dict_find(Some(&mut *d1), b"a").unwrap();
            assert_eq!((*a).di_tv.value, TypvalValue::Number(1)); // kept, not overwritten.
            let b = crate::eval::typval::tv_dict_find(Some(&mut *d1), b"b").unwrap();
            assert_eq!((*b).di_tv.value, TypvalValue::Number(3)); // still added.
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extend_dict_with_error_action_and_a_duplicate_key_leaves_the_value_unchanged() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d1, b"a", 1) };
        let d2 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d2, b"a", 2) };
        let mut rettv = TypvalT::default();
        let args = [
            TypvalT { value: TypvalValue::Dict(d1), ..Default::default() },
            TypvalT { value: TypvalValue::Dict(d2), ..Default::default() },
            string(b"error"),
        ];
        unsafe { f_extend(&args, &mut rettv) };
        unsafe {
            let a = crate::eval::typval::tv_dict_find(Some(&mut *d1), b"a").unwrap();
            assert_eq!((*a).di_tv.value, TypvalValue::Number(1));
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extend_of_a_locked_dict_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        unsafe { (*d1).dv_lock = crate::eval::typval_defs::VarLockStatus::Locked };
        let d2 = crate::eval::typval::tv_dict_alloc();
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args =
            [TypvalT { value: TypvalValue::Dict(d1), ..Default::default() }, TypvalT { value: TypvalValue::Dict(d2), ..Default::default() }];
        unsafe { f_extend(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extend_with_mismatched_types_leaves_rettv_at_default() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = crate::eval::typval::tv_list_alloc(0);
        let d2 = crate::eval::typval::tv_dict_alloc();
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        let args =
            [TypvalT { value: TypvalValue::List(l1), ..Default::default() }, TypvalT { value: TypvalValue::Dict(d2), ..Default::default() }];
        unsafe { f_extend(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe {
            crate::eval::typval::tv_list_unref(l1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extend_dict_with_an_invalid_action_string_leaves_rettv_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        let d2 = crate::eval::typval::tv_dict_alloc();
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [
            TypvalT { value: TypvalValue::Dict(d1), ..Default::default() },
            TypvalT { value: TypvalValue::Dict(d2), ..Default::default() },
            string(b"bogus"),
        ];
        unsafe { f_extend(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
        unsafe {
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn extendnew_returns_a_new_list_leaving_the_original_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *l1, 1);
            crate::eval::typval::tv_list_append_number(&mut *l1, 2);
        }
        let l2 = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *l2, 3) };
        let mut rettv = TypvalT::default();
        let args =
            [TypvalT { value: TypvalValue::List(l1), ..Default::default() }, TypvalT { value: TypvalValue::List(l2), ..Default::default() }];
        unsafe { f_extendnew(&args, &mut rettv) };
        let TypvalValue::List(new_list) = rettv.value else { panic!("expected a List") };
        assert_ne!(new_list, l1); // a genuinely separate list.
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(new_list), 3);
            assert_eq!(crate::eval::typval::tv_list_len(l1), 2); // original untouched.
            crate::eval::typval::tv_list_unref(l1);
            crate::eval::typval::tv_list_unref(l2);
            crate::eval::typval::tv_list_unref(new_list);
        }
    }

    #[test]
    fn extendnew_returns_a_new_dict_leaving_the_original_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d1, b"a", 1) };
        let d2 = crate::eval::typval::tv_dict_alloc();
        unsafe { crate::eval::typval::tv_dict_add_nr(&mut *d2, b"b", 2) };
        let mut rettv = TypvalT::default();
        let args =
            [TypvalT { value: TypvalValue::Dict(d1), ..Default::default() }, TypvalT { value: TypvalValue::Dict(d2), ..Default::default() }];
        unsafe { f_extendnew(&args, &mut rettv) };
        let TypvalValue::Dict(new_dict) = rettv.value else { panic!("expected a Dict") };
        assert_ne!(new_dict, d1);
        unsafe {
            assert_eq!(crate::eval::typval::tv_dict_len(new_dict.as_ref()), 2);
            assert_eq!(crate::eval::typval::tv_dict_len(d1.as_ref()), 1); // original untouched.
            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
            crate::eval::typval::tv_dict_unref(new_dict);
        }
    }

    // --- f_range ---

    fn list_values(l: *mut crate::eval::typval_defs::ListT) -> Vec<crate::eval::typval_defs::VarnumberT> {
        let mut vals = Vec::new();
        unsafe {
            let mut item = crate::eval::typval::tv_list_first(l);
            while !item.is_null() {
                let TypvalValue::Number(n) = (*item).li_tv.value else { panic!("expected a Number") };
                vals.push(n);
                item = (*item).li_next;
            }
        }
        vals
    }

    #[test]
    fn range_with_one_argument_produces_zero_to_n_minus_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_range(&[num(4)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(list_values(l), vec![0, 1, 2, 3]);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn range_with_one_argument_of_zero_produces_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_range(&[num(0)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(list_values(l), Vec::<crate::eval::typval_defs::VarnumberT>::new());
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn range_with_two_arguments_is_inclusive_of_the_end() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_range(&[num(2), num(5)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(list_values(l), vec![2, 3, 4, 5]);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn range_with_a_stride_steps_by_that_amount() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_range(&[num(0), num(10), num(3)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(list_values(l), vec![0, 3, 6, 9]);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn range_with_a_negative_stride_counts_down() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        unsafe { f_range(&[num(5), num(1), num(-2)], &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(list_values(l), vec![5, 3, 1]);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn range_with_a_zero_stride_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_range(&[num(0), num(5), num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    #[test]
    fn range_with_start_past_end_leaves_rettv_untouched() {
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        unsafe { f_range(&[num(5), num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(999));
    }

    // --- f_repeat ---

    #[test]
    fn repeat_string_repeats_n_times() {
        let mut rettv = TypvalT::default();
        unsafe { f_repeat(&[string(b"ab"), num(3)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"ababab".to_vec())));
    }

    #[test]
    fn repeat_string_with_zero_or_negative_count_is_empty() {
        let mut rettv = TypvalT::default();
        unsafe { f_repeat(&[string(b"ab"), num(0)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
        unsafe { f_repeat(&[string(b"ab"), num(-1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn repeat_string_stops_at_an_embedded_nul() {
        let mut rettv = TypvalT::default();
        unsafe { f_repeat(&[string(b"ab\0cd"), num(2)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"abab".to_vec())));
    }

    #[test]
    fn repeat_of_a_number_stringifies_first() {
        let mut rettv = TypvalT::default();
        unsafe { f_repeat(&[num(12), num(2)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"1212".to_vec())));
    }

    #[test]
    fn repeat_list_repeats_n_times() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(3)];
        unsafe { f_repeat(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_ne!(l, list);
        unsafe {
            assert_eq!(list_values(l), vec![1, 2, 1, 2, 1, 2]);
            crate::eval::typval::tv_list_unref(list);
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn repeat_list_with_zero_count_is_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(0)];
        unsafe { f_repeat(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 0);
            crate::eval::typval::tv_list_unref(list);
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn repeat_blob_repeats_n_times() {
        let blob = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob).bv_ga.ga_data = vec![1, 2];
            (*blob).bv_ga.ga_len = 2;
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(blob), ..Default::default() }, num(2)];
        unsafe { f_repeat(&args, &mut rettv) };
        let TypvalValue::Blob(b) = rettv.value else { panic!("expected a Blob") };
        assert_ne!(b, blob);
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(b), 4);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 0), 1);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 1), 2);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 2), 1);
            assert_eq!(crate::eval::typval::tv_blob_get(b, 3), 2);
            crate::eval::typval::tv_blob_free(blob);
            crate::eval::typval::tv_blob_free(b);
        }
    }

    #[test]
    fn repeat_blob_of_a_null_blob_is_an_empty_blob() {
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::Blob(std::ptr::null_mut()), ..Default::default() }, num(3)];
        unsafe { f_repeat(&args, &mut rettv) };
        let TypvalValue::Blob(b) = rettv.value else { panic!("expected a Blob") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_blob_len(b), 0);
            crate::eval::typval::tv_blob_free(b);
        }
    }

    // --- f_join ---

    #[test]
    fn join_with_default_separator_uses_a_space() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_join(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"1 2 3".to_vec())));
        unsafe { crate::eval::typval::tv_list_free(list) };
    }

    #[test]
    fn join_with_a_custom_separator() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe {
            crate::eval::typval::tv_list_append_string(list, Some(b"a"));
            crate::eval::typval::tv_list_append_string(list, Some(b"b"));
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, string(b", ")];
        unsafe { f_join(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"a, b".to_vec())));
        unsafe { crate::eval::typval::tv_list_free(list) };
    }

    #[test]
    fn join_of_a_non_list_leaves_rettv_at_default() {
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        unsafe { f_join(&[num(5)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn join_with_a_type_error_separator_sets_a_null_string() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let inner_sep = crate::eval::typval::tv_list_alloc(0);
        let args = [
            TypvalT { value: TypvalValue::List(list), ..Default::default() },
            TypvalT { value: TypvalValue::List(inner_sep), ..Default::default() },
        ];
        unsafe { f_join(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
        unsafe {
            crate::eval::typval::tv_list_free(list);
            crate::eval::typval::tv_list_free(inner_sep);
        }
    }

    // --- f_flatten / f_flattennew ---

    #[test]
    fn flatten_flattens_in_place_and_returns_the_same_list() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(2);
        unsafe { crate::eval::typval::tv_list_append_number(&mut *list, 1) };
        let inner = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_ref(inner);
            crate::eval::typval::tv_list_append_number(&mut *inner, 2);
            crate::eval::typval::tv_list_append_owned_tv(
                list,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_flatten(&args, &mut rettv) };
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(l, list);
        unsafe {
            assert_eq!(list_values(l), vec![1, 2]);
            crate::eval::typval::tv_list_unref(l);
        }
    }

    #[test]
    fn flatten_of_a_locked_list_leaves_it_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(0);
        unsafe { (*list).lv_lock = crate::eval::typval_defs::VarLockStatus::Locked };
        let mut rettv = TypvalT { value: TypvalValue::Number(999), ..Default::default() };
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_flatten(&args, &mut rettv) };
        // rettv still gets the List value assigned upfront (matching
        // the original's own unconditional rettv assignment before
        // the lock check), even though flattening itself never runs.
        assert_eq!(rettv.value, TypvalValue::List(list));
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    #[test]
    fn flatten_with_a_negative_maxdepth_leaves_rettv_at_default() {
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        let list = crate::eval::typval::tv_list_alloc(0);
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }, num(-1)];
        unsafe { f_flatten(&args, &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
        unsafe { crate::eval::typval::tv_list_free(list) };
    }

    #[test]
    fn flatten_of_a_non_list_leaves_rettv_at_default() {
        let mut rettv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        unsafe { f_flatten(&[num(5)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn flattennew_returns_a_new_list_leaving_the_original_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let list = crate::eval::typval::tv_list_alloc(1);
        let inner = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_ref(inner);
            crate::eval::typval::tv_list_append_number(&mut *inner, 1);
            crate::eval::typval::tv_list_append_owned_tv(
                list,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            );
        }
        let mut rettv = TypvalT::default();
        let args = [TypvalT { value: TypvalValue::List(list), ..Default::default() }];
        unsafe { f_flattennew(&args, &mut rettv) };
        let TypvalValue::List(new_list) = rettv.value else { panic!("expected a List") };
        assert_ne!(new_list, list);
        unsafe {
            assert_eq!(list_values(new_list), vec![1]);
            assert_eq!(crate::eval::typval::tv_list_len(list), 1); // original still nested, untouched.
            let orig_item = crate::eval::typval::tv_list_first(list);
            assert!(matches!((*orig_item).li_tv.value, TypvalValue::List(_)));
            crate::eval::typval::tv_list_unref(list);
            crate::eval::typval::tv_list_unref(new_list);
        }
    }

    // --- f_localtime ---

    #[test]
    fn localtime_returns_a_positive_unix_timestamp() {
        let mut rettv = TypvalT::default();
        f_localtime(&[], &mut rettv);
        let TypvalValue::Number(n) = rettv.value else { panic!("expected a Number") };
        // Any real wall-clock time is comfortably past 2020-01-01
        // (1577836800) - a loose sanity bound, not a flaky exact-time
        // check.
        assert!(n > 1_577_836_800);
    }

    // --- f_getenv / f_environ ---
    //
    // Each test below uses a uniquely-named test-only environment
    // variable (never a well-known name like PATH/HOME/VIM) so it
    // cannot race with any other test in the whole crate that also
    // touches process-wide environment state, regardless of parallel
    // execution - avoiding the need for a shared cross-module lock.

    #[test]
    fn getenv_returns_the_value_of_a_set_variable() {
        // SAFETY: NERO_TEST_GETENV_UNIQUE_VAR is unique to this test.
        unsafe { std::env::set_var("NERO_TEST_GETENV_UNIQUE_VAR", "hello") };
        let mut rettv = TypvalT::default();
        unsafe { f_getenv(&[string(b"NERO_TEST_GETENV_UNIQUE_VAR")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello".to_vec())));
        // SAFETY: forwarded from the set_var call above.
        unsafe { std::env::remove_var("NERO_TEST_GETENV_UNIQUE_VAR") };
    }

    #[test]
    fn getenv_returns_null_for_an_unset_variable() {
        let mut rettv = TypvalT::default();
        unsafe { f_getenv(&[string(b"NERO_TEST_GETENV_DEFINITELY_UNSET_VAR")], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null));
    }

    #[test]
    fn environ_returns_a_non_empty_dict() {
        // Missing this lock was the ACTUAL root cause of a
        // hashtab-capacity panic reproduced on native Linux (not the
        // env-mutation hazard suspected initially, which was a real
        // but separate concern also worth avoiding - see below):
        // tv_dict_alloc touches the crate-wide GC_FIRST_DICT linked
        // list, so running concurrently with another dict-touching
        // test (without this lock serializing them) is a genuine data
        // race on that shared global list - this was caught by
        // multiple_dicts_maintain_the_gc_linked_list_correctly also
        // failing deterministically alongside this test on native
        // Linux (never reproduced on Windows, latency/scheduling-
        // dependent like most data races).
        let _lock = crate::globals::global_state_test_lock();
        // Also deliberately does NOT mutate the environment (no
        // set_var/remove_var) - environ()'s own full enumeration is
        // separately not safely reentrant against ANY concurrent
        // env-var mutation from another thread (a well-known,
        // platform-specific hazard, worse on Linux/glibc than
        // Windows). Checking for a non-empty Dict (rather than
        // mutating and checking a specific fresh key) avoids that
        // hazard too, while still verifying real enumeration
        // happened.
        let mut rettv = TypvalT::default();
        f_environ(&[], &mut rettv);
        let TypvalValue::Dict(d) = rettv.value else { panic!("expected a Dict") };
        unsafe {
            assert!(crate::eval::typval::tv_dict_len(d.as_ref()) > 0);
            crate::eval::typval::tv_dict_unref(d);
        }
    }

    // --- f_has ---

    #[test]
    fn has_finds_an_always_present_feature() {
        let mut rettv = TypvalT::default();
        f_has(&[string(b"eval")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn has_is_case_insensitive() {
        let mut rettv = TypvalT::default();
        f_has(&[string(b"NVIM")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn has_returns_zero_for_an_unknown_feature() {
        let mut rettv = TypvalT::default();
        f_has(&[string(b"totally_not_a_real_feature")], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn has_reports_the_current_platform() {
        let mut rettv = TypvalT::default();
        let name: &[u8] = if cfg!(windows) { b"win32" } else { b"unix" };
        f_has(&[string(name)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn has_does_not_report_the_other_platform() {
        let mut rettv = TypvalT::default();
        let name: &[u8] = if cfg!(windows) { b"unix" } else { b"win32" };
        f_has(&[string(name)], &mut rettv);
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn has_win64_only_matches_on_64_bit_windows() {
        let mut rettv = TypvalT::default();
        f_has(&[string(b"win64")], &mut rettv);
        let expect_true = cfg!(all(windows, target_pointer_width = "64"));
        assert_eq!(rettv.value, TypvalValue::Number(i64::from(expect_true)));
    }
}