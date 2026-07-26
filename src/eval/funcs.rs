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
//! [`call_internal_func`], and three of the simplest builtins: `len()`,
//! `type()`, `empty()` - each already expressible in terms of already-
//! translated helpers ([`crate::eval::typval::tv_get_string`]/
//! `tv_list_len`/`tv_dict_len`/`tv_blob_len`).
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

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: crate::eval::typval_defs::VarnumberT) -> TypvalT {
        TypvalT { value: TypvalValue::Number(n), ..Default::default() }
    }

    fn string(s: &[u8]) -> TypvalT {
        TypvalT { value: TypvalValue::String(Some(s.to_vec())), ..Default::default() }
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
}