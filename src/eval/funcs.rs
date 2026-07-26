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
}