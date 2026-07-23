//! Translated from `src/nvim/eval.c` (tractable core only).
//!
//! `eval.c` (~7000 lines) is THE Vimscript expression evaluator/parser
//! itself: the full recursive-descent grammar (`eval0`/`eval1`
//! ternary/`eval2` `||`/`eval3` `&&`/`eval4` comparisons/`eval5`
//! `+`/`-`/`.`/`eval6` `*`/`/`/`%`/`eval7` unary/primary/literal
//! parsing), lvalue resolution (`get_lval`/`set_var_lval`), `:for`
//! loop iteration, function-call/method-call/index/slice evaluation,
//! and dozens of other pieces - genuinely the single largest remaining
//! undertaking in the whole eval engine, not attempted as a whole
//! here. This module lives at `crate::eval::eval` (an `eval::eval`
//! submodule, not a top-level `src/eval.rs`) purely because
//! `src/nvim/eval.c`'s own name collides with this crate's
//! already-established `crate::eval` module (grouping `eval/typval.c`/
//! `eval/vars.c`/`eval/userfunc.c`, all genuinely the same subsystem),
//! NOT a claim that the original file itself lives under
//! `src/nvim/eval/`.
//!
//! Translated so far: `num_divide`/`num_modulus` - the only two
//! functions in the entire file with zero dependency on the parser,
//! lvalue machinery, or any not-yet-translated piece; they operate
//! purely on two already-evaluated [`VarnumberT`]s. Harvested first as
//! the natural, lowest-risk entry point into this file, matching this
//! session's established "translate the reachable leaves before the
//! engine that calls them" pattern (e.g. `option_defs.rs`'s `OptIndex`
//! before the real `options[]` engine).
//!
//! Also translated: `eval_addblob` - tractable once `eval/typval.rs`
//! gained `tv_blob_len`/`tv_blob_set_ret` (`eval/typval.h`'s own
//! `static inline` helpers, harvested alongside this function since it
//! was their only caller). Like all the `eval_add*`/`eval_*div*`
//! sibling functions in this file, it takes ALREADY-typed-and-evaluated
//! operands - the caller (`eval5`, not yet translated) is responsible
//! for checking both are `Blob`-typed before calling this; that
//! precondition is documented, not re-checked here, matching the
//! original's own lack of a runtime type check at this layer.
//!
//! Also translated: `grow_string_tv`/`eval_concat_str`. `grow_string_tv`
//! is the original's manual `xrealloc`-in-place performance
//! optimization to avoid a separate allocate+copy+free when growing a
//! Vimscript string - Rust's own `Vec<u8>::extend_from_slice` already
//! provides this transparently, so this translation is a thin,
//! faithful wrapper rather than a manual realloc, but is still its OWN
//! function (not inlined into `eval_concat_str`) since the original
//! has a SECOND real caller, `eval/executor.c` (not yet translated).
//! `eval_concat_str` needed `eval/typval.rs`'s `tv_clear_simple`
//! widened from private to `pub(crate)` - unlike `eval_addblob`, it
//! doesn't statically know tv1's type ahead of time (only tv2 is
//! constrained to be stringifiable), so it needs the same generic
//! "release whatever `tv1` used to hold" dispatch `tv_dict_item_free`/
//! `partial_free` already use, not a type-specific `tv_*_unref` call.
//!
//! Also translated: `eval_addsub_number`/`eval_multdiv_number` (as
//! [`eval_addsub_number`]/[`eval_multdiv_number`], each taking a small
//! new `AddSubOp`/`MulDivOp` enum in place of the original's `int op`
//! holding a literal ASCII operator character). These two sibling
//! functions have genuinely different internal structures in the
//! original despite similar purposes - `eval_addsub_number` clears
//! `tv1` once at the very end (after fully processing both operands),
//! while `eval_multdiv_number` clears `tv1` early and clears `tv2`
//! itself unconditionally in the success path too (since its own
//! caller, `eval6`, never does so, unlike `eval5`'s treatment of
//! `eval_addsub_number`'s family) - each function's own doc comment
//! preserves its own exact clearing contract precisely rather than
//! forcing a shared shape. `eval_multdiv_number`'s float-division
//! branch simplifies the original's elaborate manual zero/sign/NaN
//! special-casing down to plain `f1 / f2`: that logic exists only to
//! dodge an AddressSanitizer false-positive (the function itself is
//! `FUNC_ATTR_NO_SANITIZE_UNDEFINED`), since IEEE 754 float division by
//! zero is already well-defined (not UB) in both C and Rust, producing
//! the identical result either way.
//!
//! Deferred: everything else - the actual parser/lvalue/loop/call
//! machinery is a separate, substantial undertaking of its own.
//! `eval_addlist` (the only remaining sibling "leaf" function) needs
//! `tv_list_concat`, not yet translated.

use crate::eval::typval_defs::{TypvalT, TypvalValue, VarnumberT, VARNUMBER_MAX, VARNUMBER_MIN};

/// "n1" divided by "n2", taking care of dividing by zero
/// (`num_divide`).
#[must_use]
pub fn num_divide(n1: VarnumberT, n2: VarnumberT) -> VarnumberT {
    if n2 == 0 {
        // give an error message? - emsg/message display, not
        // tractable, matching this crate's established "skip the
        // display, keep the state" policy; the original doesn't
        // actually emit one here either (the comment is a stale
        // question, not a real call).
        if n1 == 0 {
            VARNUMBER_MIN // similar to NaN
        } else if n1 < 0 {
            -VARNUMBER_MAX
        } else {
            VARNUMBER_MAX
        }
    } else if n1 == VARNUMBER_MIN && n2 == -1 {
        // specific case: trying to do VARNUMBER_MIN / -1 results in a
        // positive number that doesn't fit in varnumber_T and causes
        // an FPE (in Rust, an overflow panic in debug builds / wrapping
        // in release - both avoided by special-casing here, matching
        // the original exactly rather than relying on either).
        VARNUMBER_MAX
    } else {
        n1 / n2
    }
}

/// "n1" modulus "n2", taking care of dividing by zero (`num_modulus`).
#[must_use]
pub fn num_modulus(n1: VarnumberT, n2: VarnumberT) -> VarnumberT {
    // Give an error when n2 is 0? - same stale-comment/no-real-call
    // situation as num_divide above.
    if n2 == 0 {
        0
    } else {
        n1 % n2
    }
}

/// Concatenate blobs `tv1` and `tv2` and store the result in `tv1`
/// (`eval_addblob`).
///
/// # Safety
/// `tv1`/`tv2` must both be `TypvalValue::Blob`-typed (matching the
/// original's own contract - the caller, Vimscript's `+` operator
/// dispatch in `eval5`, not yet translated, is responsible for
/// checking this BEFORE calling); any non-null blob pointer they hold
/// must be valid.
pub unsafe fn eval_addblob(tv1: &mut TypvalT, tv2: &TypvalT) {
    use crate::eval::typval::{tv_blob_alloc, tv_blob_len, tv_blob_set_ret};

    let TypvalValue::Blob(b1) = tv1.value else {
        unreachable!("eval_addblob: tv1 must be Blob-typed (caller's own contract)")
    };
    let TypvalValue::Blob(b2) = tv2.value else {
        unreachable!("eval_addblob: tv2 must be Blob-typed (caller's own contract)")
    };
    let b = tv_blob_alloc();

    // SAFETY: forwarded from this function's own safety doc.
    let len1 = unsafe { tv_blob_len(b1) };
    // SAFETY: forwarded from this function's own safety doc.
    let len2 = unsafe { tv_blob_len(b2) };
    let totallen = i64::from(len1) + i64::from(len2);

    if (0..=i64::from(i32::MAX)).contains(&totallen) {
        // SAFETY: `b` was just allocated via `tv_blob_alloc` above.
        let blob = unsafe { &mut *b };
        blob.bv_ga.ga_grow(totallen as i32);
        if len1 > 0 {
            // SAFETY: forwarded from this function's own safety doc.
            let b1_ref = unsafe { &*b1 };
            let src1 = b1_ref.bv_ga.ga_data[..len1 as usize].to_vec();
            blob.bv_ga.ga_data[..len1 as usize].copy_from_slice(&src1);
        }
        if len2 > 0 {
            // SAFETY: forwarded from this function's own safety doc.
            let b2_ref = unsafe { &*b2 };
            let src2 = b2_ref.bv_ga.ga_data[..len2 as usize].to_vec();
            blob.bv_ga.ga_data[len1 as usize..(len1 + len2) as usize].copy_from_slice(&src2);
        }
        blob.bv_ga.ga_len = totallen as i32;
    }

    // SAFETY: forwarded from this function's own safety doc - `b1` (if
    // non-null) is a valid pointer to release; releasing it directly
    // via `tv_blob_unref` rather than the crate's generic
    // `tv_clear_simple` dispatcher, since `tv1` is already known to be
    // `Blob`-typed from the pattern match above (contrast
    // `eval_concat_str` below, which genuinely needs the generic
    // dispatcher since it doesn't know tv1's type ahead of time).
    unsafe { crate::eval::typval::tv_blob_unref(b1) };
    // SAFETY: `b` is a valid pointer just allocated above.
    unsafe { tv_blob_set_ret(tv1, b) };
}

/// Append `s2` to the string in `tv1` (`grow_string_tv`).
///
/// Returns `true` if `tv1` was grown in place, `false` otherwise
/// (`tv1` isn't `String`-typed, or its value is `None`) - matches the
/// original's `OK`/`FAIL` exactly. See this module's own doc comment
/// for why this stays its own function rather than being inlined into
/// [`eval_concat_str`].
pub fn grow_string_tv(tv1: &mut TypvalT, s2: &[u8]) -> bool {
    let TypvalValue::String(Some(s1)) = &mut tv1.value else {
        return false;
    };
    s1.extend_from_slice(s2);
    true
}

/// Concatenate strings `tv1` and `tv2` and store the result in `tv1`
/// (`eval_concat_str`).
///
/// Returns `false` if `tv2` cannot be stringified (a type error) -
/// `tv1` is assumed already stringifiable (the caller, Vimscript's
/// `.`/`..` operator dispatch in `eval5`, not yet translated, only
/// calls this after confirming that), matching the original's own
/// "s1 already checked" comment.
///
/// # Safety
/// If `tv1`'s value is `List`/`Dict`/`Blob`/`Partial`-typed with a
/// non-null pointer, that pointer must be valid - forwarded to
/// `eval/typval.rs`'s `tv_clear_simple`'s own contract, used here to
/// release `tv1`'s old value when it can't be grown in place.
pub unsafe fn eval_concat_str(tv1: &mut TypvalT, tv2: &TypvalT) -> bool {
    use crate::eval::typval::{tv_clear_simple, tv_get_string, tv_get_string_chk};

    let s1 = tv_get_string(tv1);
    let Some(s2) = tv_get_string_chk(tv2) else {
        return false;
    };

    // When possible, grow the existing string in place to avoid alloc/free.
    if grow_string_tv(tv1, &s2) {
        return true;
    }

    let p = crate::strings::concat_str(&s1, &s2);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_clear_simple(tv1) };
    tv1.value = TypvalValue::String(Some(p));

    true
}

/// The two operators [`eval_addsub_number`] handles (`op` in the
/// original, an `int` holding the literal ASCII `'+'`/`'-'` - `eval5`,
/// this function's only call site, never passes anything else).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddSubOp {
    Add,
    Sub,
}

/// Add or subtract numbers `tv1` and `tv2` and store the result in
/// `tv1`. The numbers can be whole numbers or floats
/// (`eval_addsub_number`).
///
/// Returns `false` on a type error (a `List`/`Dict`/`Blob`/`Partial`
/// operand on either side, or anything else `tv_get_number_chk`
/// rejects) - matches the original's `OK`/`FAIL`. Whole-number
/// addition/subtraction uses `wrapping_add`/`wrapping_sub`, matching
/// this crate's established convention for replicating the original's
/// implicit-wrapping signed-integer-overflow C arithmetic (e.g.
/// `cursor.rs`/`hashtab.rs`/`profile.rs`) rather than Rust's own
/// panic-on-overflow-in-debug default.
///
/// # Safety
/// If `tv1`/`tv2`'s value is `List`/`Dict`/`Blob`/`Partial`-typed with
/// a non-null pointer, that pointer must be valid - forwarded to
/// `eval/typval.rs`'s `tv_clear_simple`, used to release both
/// operands' old values (`tv1`'s unconditionally, once the result type
/// is known; `tv2`'s only in the two error paths, matching the
/// original's own `tv_clear(tv2)` placement exactly - the SUCCESS path
/// leaves clearing `tv2` to the caller, `eval5`, not yet translated).
pub unsafe fn eval_addsub_number(tv1: &mut TypvalT, tv2: &TypvalT, op: AddSubOp) -> bool {
    use crate::eval::typval::{tv_clear_simple, tv_get_number_chk};

    let tv1_is_float = matches!(tv1.value, TypvalValue::Float(_));
    let tv2_is_float = matches!(tv2.value, TypvalValue::Float(_));

    let mut f1 = 0.0;
    let mut f2 = 0.0;
    let mut n1: VarnumberT = 0;
    let mut n2: VarnumberT = 0;

    if let TypvalValue::Float(f) = tv1.value {
        f1 = f;
    } else {
        let mut error = false;
        n1 = tv_get_number_chk(tv1, Some(&mut error));
        if error {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                tv_clear_simple(tv1);
                tv_clear_simple(tv2);
            }
            return false;
        }
        if tv2_is_float {
            f1 = n1 as f64;
        }
    }

    if let TypvalValue::Float(f) = tv2.value {
        f2 = f;
    } else {
        let mut error = false;
        n2 = tv_get_number_chk(tv2, Some(&mut error));
        if error {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                tv_clear_simple(tv1);
                tv_clear_simple(tv2);
            }
            return false;
        }
        if tv1_is_float {
            f2 = n2 as f64;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_clear_simple(tv1) };

    // If there is a float on either side the result is a float.
    if tv1_is_float || tv2_is_float {
        let result = match op {
            AddSubOp::Add => f1 + f2,
            AddSubOp::Sub => f1 - f2,
        };
        tv1.value = TypvalValue::Float(result);
    } else {
        let result = match op {
            AddSubOp::Add => n1.wrapping_add(n2),
            AddSubOp::Sub => n1.wrapping_sub(n2),
        };
        tv1.value = TypvalValue::Number(result);
    }

    true
}

/// The three operators [`eval_multdiv_number`] handles (`op` in the
/// original, an `int` holding the literal ASCII `'*'`/`'/'`/`'%'` -
/// `eval6`, this function's only call site, never passes anything
/// else).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MulDivOp {
    Mul,
    Div,
    Mod,
}

/// Multiply, divide, or compute the modulo of numbers `tv1` and `tv2`
/// and store the result in `tv1`. The numbers can be whole numbers or
/// floats (`eval_multdiv_number`).
///
/// Returns `false` on a type error, or when `op` is [`MulDivOp::Mod`]
/// and either operand is a `Float` (`%` has no float form - the
/// original's own `emsg("E804: Cannot use '%' with Float")`, whose
/// message display is skipped per this crate's established policy,
/// while the `FAIL` return is kept exactly).
///
/// Float division by zero uses plain IEEE 754 `f64` division directly
/// (`f1 / f2`), NOT the original's elaborate manual
/// zero/sign/NaN-vs-Infinity special-casing: that logic exists only to
/// dodge an AddressSanitizer false-positive on float division by zero
/// (the function itself is marked `FUNC_ATTR_NO_SANITIZE_UNDEFINED`,
/// and its own comment says exactly this - "Division by zero triggers
/// error from AddressSanitizer") - float division by zero is
/// well-defined by IEEE 754 (and therefore not UB in either C or
/// Rust), producing the identical `NaN`/`+Infinity`/`-Infinity` result
/// the manual special-casing computes by hand, for every sign/zero
/// combination. Whole-number multiplication uses `wrapping_mul`,
/// matching this crate's established overflow convention (see
/// [`eval_addsub_number`]'s own doc comment); whole-number division
/// and modulo reuse the already-real [`num_divide`]/[`num_modulus`].
///
/// # Safety
/// If `tv1`/`tv2`'s value is `List`/`Dict`/`Blob`/`Partial`-typed with
/// a non-null pointer, that pointer must be valid - forwarded to
/// `eval/typval.rs`'s `tv_clear_simple`. Unlike [`eval_addsub_number`],
/// THIS function clears `tv2` itself in the success path too (matching
/// the original exactly: `eval6`, this function's only caller, never
/// clears `tv2` on its own, unlike `eval5`'s treatment of
/// [`eval_addsub_number`]'s sibling functions).
pub unsafe fn eval_multdiv_number(tv1: &mut TypvalT, tv2: &TypvalT, op: MulDivOp) -> bool {
    use crate::eval::typval::{tv_clear_simple, tv_get_number_chk};

    let mut use_float = matches!(tv1.value, TypvalValue::Float(_));
    let mut f1 = 0.0;
    let mut n1: VarnumberT = 0;
    let mut error = false;

    if let TypvalValue::Float(f) = tv1.value {
        f1 = f;
    } else {
        n1 = tv_get_number_chk(tv1, Some(&mut error));
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_clear_simple(tv1) };
    if error {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(tv2) };
        return false;
    }

    let mut f2 = 0.0;
    let mut n2: VarnumberT = 0;
    if let TypvalValue::Float(f) = tv2.value {
        if !use_float {
            f1 = n1 as f64;
            use_float = true;
        }
        f2 = f;
    } else {
        n2 = tv_get_number_chk(tv2, Some(&mut error));
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(tv2) };
        if error {
            return false;
        }
        if use_float {
            f2 = n2 as f64;
        }
    }

    // Compute the result. When either side is a float the result is a
    // float.
    if use_float {
        let result = match op {
            MulDivOp::Mul => f1 * f2,
            // Well-defined by IEEE 754 in both C and Rust - see this
            // function's own doc comment for why no manual
            // zero/sign/NaN special-casing is needed here.
            MulDivOp::Div => f1 / f2,
            MulDivOp::Mod => {
                // "%" with Float - emsg(...) skipped, see this
                // function's own doc comment.
                return false;
            }
        };
        tv1.value = TypvalValue::Float(result);
    } else {
        let result = match op {
            MulDivOp::Mul => n1.wrapping_mul(n2),
            MulDivOp::Div => num_divide(n1, n2),
            MulDivOp::Mod => num_modulus(n1, n2),
        };
        tv1.value = TypvalValue::Number(result);
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn num_divide_ordinary_case() {
        assert_eq!(num_divide(10, 3), 3);
        assert_eq!(num_divide(-10, 3), -3);
        assert_eq!(num_divide(10, -3), -3);
    }

    #[test]
    fn num_divide_by_zero_with_zero_numerator_is_min() {
        assert_eq!(num_divide(0, 0), VARNUMBER_MIN);
    }

    #[test]
    fn num_divide_by_zero_with_negative_numerator_is_negated_max() {
        assert_eq!(num_divide(-5, 0), -VARNUMBER_MAX);
    }

    #[test]
    fn num_divide_by_zero_with_positive_numerator_is_max() {
        assert_eq!(num_divide(5, 0), VARNUMBER_MAX);
    }

    #[test]
    fn num_divide_min_by_negative_one_is_max_not_overflow() {
        // VARNUMBER_MIN / -1 would overflow i64 (panicking in debug,
        // wrapping in release) - the original special-cases this to
        // avoid the FPE its own C division would otherwise trigger;
        // this test would panic in a debug build if that special case
        // were ever removed.
        assert_eq!(num_divide(VARNUMBER_MIN, -1), VARNUMBER_MAX);
    }

    #[test]
    fn num_modulus_ordinary_case() {
        assert_eq!(num_modulus(10, 3), 1);
        assert_eq!(num_modulus(-10, 3), -1);
    }

    #[test]
    fn num_modulus_by_zero_is_zero() {
        assert_eq!(num_modulus(5, 0), 0);
        assert_eq!(num_modulus(0, 0), 0);
    }

    #[test]
    fn eval_addblob_concatenates_bytes_in_order() {
        use crate::eval::typval::{tv_blob_alloc, tv_blob_free};

        let b1 = tv_blob_alloc();
        let b2 = tv_blob_alloc();
        unsafe {
            (*b1).bv_ga.ga_concat_len(b"hello");
            (*b2).bv_ga.ga_concat_len(b" world");
        }
        let mut tv1 = TypvalT { value: TypvalValue::Blob(b1), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::Blob(b2), ..Default::default() };

        unsafe {
            eval_addblob(&mut tv1, &tv2);
            let TypvalValue::Blob(result) = tv1.value else {
                panic!("expected a Blob-typed result");
            };
            let result_ref = &*result;
            assert_eq!(result_ref.bv_ga.ga_len, 11);
            assert_eq!(&result_ref.bv_ga.ga_data[..11], b"hello world");
            assert_eq!(result_ref.bv_refcount, 1);
            tv_blob_free(result);
            // tv1's original b1 was released internally by eval_addblob
            // (via tv_blob_unref, refcount 0 -> freed) - only b2 (read,
            // never released here, matching the original's own
            // asymmetric tv1-only tv_clear) needs manual cleanup.
            tv_blob_free(b2);
        }
    }

    #[test]
    fn eval_addblob_with_one_empty_operand() {
        use crate::eval::typval::{tv_blob_alloc, tv_blob_free};

        let b1 = tv_blob_alloc();
        let b2 = tv_blob_alloc();
        unsafe {
            (*b1).bv_ga.ga_concat_len(b"data");
        }
        let mut tv1 = TypvalT { value: TypvalValue::Blob(b1), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::Blob(b2), ..Default::default() };

        unsafe {
            eval_addblob(&mut tv1, &tv2);
            let TypvalValue::Blob(result) = tv1.value else {
                panic!("expected a Blob-typed result");
            };
            let result_ref = &*result;
            assert_eq!(result_ref.bv_ga.ga_len, 4);
            assert_eq!(&result_ref.bv_ga.ga_data[..4], b"data");
            tv_blob_free(result);
            tv_blob_free(b2);
        }
    }

    #[test]
    fn eval_addblob_both_empty_gives_empty_result() {
        use crate::eval::typval::{tv_blob_alloc, tv_blob_free};

        let b1 = tv_blob_alloc();
        let b2 = tv_blob_alloc();
        let mut tv1 = TypvalT { value: TypvalValue::Blob(b1), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::Blob(b2), ..Default::default() };

        unsafe {
            eval_addblob(&mut tv1, &tv2);
            let TypvalValue::Blob(result) = tv1.value else {
                panic!("expected a Blob-typed result");
            };
            assert_eq!((*result).bv_ga.ga_len, 0);
            tv_blob_free(result);
            tv_blob_free(b2);
        }
    }

    #[test]
    fn grow_string_tv_appends_in_place() {
        let mut tv1 = TypvalT { value: TypvalValue::String(Some(b"hello".to_vec())), ..Default::default() };
        assert!(grow_string_tv(&mut tv1, b" world"));
        assert!(matches!(&tv1.value, TypvalValue::String(Some(s)) if s == b"hello world"));
    }

    #[test]
    fn grow_string_tv_fails_for_non_string() {
        let mut tv1 = TypvalT { value: TypvalValue::Number(42), ..Default::default() };
        assert!(!grow_string_tv(&mut tv1, b"abc"));
        // Unchanged on failure.
        assert!(matches!(tv1.value, TypvalValue::Number(42)));
    }

    #[test]
    fn grow_string_tv_fails_for_none_string() {
        let mut tv1 = TypvalT { value: TypvalValue::String(None), ..Default::default() };
        assert!(!grow_string_tv(&mut tv1, b"abc"));
    }

    #[test]
    fn eval_concat_str_grows_tv1_in_place_when_both_are_strings() {
        let mut tv1 = TypvalT { value: TypvalValue::String(Some(b"foo".to_vec())), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::String(Some(b"bar".to_vec())), ..Default::default() };
        let ok = unsafe { eval_concat_str(&mut tv1, &tv2) };
        assert!(ok);
        assert!(matches!(&tv1.value, TypvalValue::String(Some(s)) if s == b"foobar"));
    }

    #[test]
    fn eval_concat_str_stringifies_a_non_string_tv1() {
        // tv1 is Number-typed - can't grow in place, so falls back to
        // concat_str + a fresh String-typed value.
        let mut tv1 = TypvalT { value: TypvalValue::Number(7), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::String(Some(b"up".to_vec())), ..Default::default() };
        let ok = unsafe { eval_concat_str(&mut tv1, &tv2) };
        assert!(ok);
        assert!(matches!(&tv1.value, TypvalValue::String(Some(s)) if s == b"7up"));
    }

    #[test]
    fn eval_concat_str_stringifies_a_float_tv2() {
        let mut tv1 = TypvalT { value: TypvalValue::String(Some(b"pi=".to_vec())), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::Float(1.5), ..Default::default() };
        let ok = unsafe { eval_concat_str(&mut tv1, &tv2) };
        assert!(ok);
        assert!(matches!(&tv1.value, TypvalValue::String(Some(s)) if s == b"pi=1.5"));
    }

    #[test]
    fn eval_concat_str_returns_false_when_tv2_is_unstringifiable() {
        let mut tv1 = TypvalT { value: TypvalValue::String(Some(b"foo".to_vec())), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() };
        let ok = unsafe { eval_concat_str(&mut tv1, &tv2) };
        assert!(!ok);
    }

    #[test]
    fn eval_concat_str_releases_tv1s_old_list_when_it_cannot_grow_in_place() {
        use crate::eval::typval::tv_list_alloc;

        // tv1 starts as a List with refcount 2 - eval_concat_str must
        // release one reference (via tv_clear_simple's generic
        // dispatch, since it doesn't know tv1's type ahead of time)
        // before overwriting tv1 with the concatenated string. Using
        // refcount 2 (not 1) so the list survives the release and can
        // still be safely dereferenced afterward to confirm the
        // decrement actually happened, rather than being silently
        // skipped.
        let l = tv_list_alloc(crate::eval::typval_defs::ListLenSpecials::Unknown as isize);
        unsafe { (*l).lv_refcount = 2 };
        let mut tv1 = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::String(Some(b"str".to_vec())), ..Default::default() };

        let ok = unsafe { eval_concat_str(&mut tv1, &tv2) };
        assert!(ok);
        assert!(matches!(&tv1.value, TypvalValue::String(Some(s)) if s == b"str"));
        unsafe {
            assert_eq!((*l).lv_refcount, 1);
            crate::eval::typval::tv_list_unref(l);
        }
    }

    fn number(n: VarnumberT) -> TypvalT {
        TypvalT { value: TypvalValue::Number(n), ..Default::default() }
    }

    fn float(f: f64) -> TypvalT {
        TypvalT { value: TypvalValue::Float(f), ..Default::default() }
    }

    #[test]
    fn eval_addsub_number_adds_two_numbers() {
        let mut tv1 = number(3);
        let tv2 = number(4);
        assert!(unsafe { eval_addsub_number(&mut tv1, &tv2, AddSubOp::Add) });
        assert!(matches!(tv1.value, TypvalValue::Number(7)));
    }

    #[test]
    fn eval_addsub_number_subtracts_two_numbers() {
        let mut tv1 = number(10);
        let tv2 = number(4);
        assert!(unsafe { eval_addsub_number(&mut tv1, &tv2, AddSubOp::Sub) });
        assert!(matches!(tv1.value, TypvalValue::Number(6)));
    }

    #[test]
    fn eval_addsub_number_number_plus_float_promotes_to_float() {
        let mut tv1 = number(3);
        let tv2 = float(0.5);
        assert!(unsafe { eval_addsub_number(&mut tv1, &tv2, AddSubOp::Add) });
        assert!(matches!(tv1.value, TypvalValue::Float(f) if f == 3.5));
    }

    #[test]
    fn eval_addsub_number_float_minus_number_promotes_to_float() {
        let mut tv1 = float(3.5);
        let tv2 = number(1);
        assert!(unsafe { eval_addsub_number(&mut tv1, &tv2, AddSubOp::Sub) });
        assert!(matches!(tv1.value, TypvalValue::Float(f) if f == 2.5));
    }

    #[test]
    fn eval_addsub_number_float_plus_float() {
        let mut tv1 = float(1.5);
        let tv2 = float(2.25);
        assert!(unsafe { eval_addsub_number(&mut tv1, &tv2, AddSubOp::Add) });
        assert!(matches!(tv1.value, TypvalValue::Float(f) if f == 3.75));
    }

    #[test]
    fn eval_addsub_number_wraps_on_overflow_like_the_original_c_arithmetic() {
        let mut tv1 = number(VARNUMBER_MAX);
        let tv2 = number(1);
        assert!(unsafe { eval_addsub_number(&mut tv1, &tv2, AddSubOp::Add) });
        assert!(matches!(tv1.value, TypvalValue::Number(n) if n == VARNUMBER_MIN));
    }

    #[test]
    fn eval_addsub_number_type_error_on_tv1_releases_both_and_returns_false() {
        let mut tv1 = TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() };
        let tv2 = number(1);
        assert!(!unsafe { eval_addsub_number(&mut tv1, &tv2, AddSubOp::Add) });
    }

    #[test]
    fn eval_addsub_number_type_error_on_tv2_returns_false() {
        let mut tv1 = number(1);
        let tv2 = TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() };
        assert!(!unsafe { eval_addsub_number(&mut tv1, &tv2, AddSubOp::Add) });
    }

    #[test]
    fn eval_multdiv_number_multiplies_two_numbers() {
        let mut tv1 = number(6);
        let tv2 = number(7);
        assert!(unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Mul) });
        assert!(matches!(tv1.value, TypvalValue::Number(42)));
    }

    #[test]
    fn eval_multdiv_number_divides_two_numbers() {
        let mut tv1 = number(20);
        let tv2 = number(4);
        assert!(unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Div) });
        assert!(matches!(tv1.value, TypvalValue::Number(5)));
    }

    #[test]
    fn eval_multdiv_number_modulus_two_numbers() {
        let mut tv1 = number(10);
        let tv2 = number(3);
        assert!(unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Mod) });
        assert!(matches!(tv1.value, TypvalValue::Number(1)));
    }

    #[test]
    fn eval_multdiv_number_integer_division_by_zero_uses_num_divide_clamp() {
        // Matches num_divide's own "similar to NaN" sentinel behavior,
        // not a panic - whole-number division by zero is NOT the same
        // code path as float division by zero in this function.
        let mut tv1 = number(5);
        let tv2 = number(0);
        assert!(unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Div) });
        assert!(matches!(tv1.value, TypvalValue::Number(n) if n == VARNUMBER_MAX));
    }

    #[test]
    fn eval_multdiv_number_float_multiplication() {
        let mut tv1 = float(1.5);
        let tv2 = float(2.0);
        assert!(unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Mul) });
        assert!(matches!(tv1.value, TypvalValue::Float(f) if f == 3.0));
    }

    #[test]
    fn eval_multdiv_number_float_division_by_zero_gives_infinity_not_panic() {
        let mut tv1 = float(1.0);
        let tv2 = float(0.0);
        assert!(unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Div) });
        assert!(matches!(tv1.value, TypvalValue::Float(f) if f.is_infinite() && f > 0.0));
    }

    #[test]
    fn eval_multdiv_number_float_division_by_zero_negative_numerator_gives_neg_infinity() {
        let mut tv1 = float(-1.0);
        let tv2 = float(0.0);
        assert!(unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Div) });
        assert!(matches!(tv1.value, TypvalValue::Float(f) if f.is_infinite() && f < 0.0));
    }

    #[test]
    fn eval_multdiv_number_float_zero_division_by_zero_gives_nan() {
        let mut tv1 = float(0.0);
        let tv2 = float(0.0);
        assert!(unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Div) });
        assert!(matches!(tv1.value, TypvalValue::Float(f) if f.is_nan()));
    }

    #[test]
    fn eval_multdiv_number_modulus_with_float_is_rejected() {
        let mut tv1 = float(5.0);
        let tv2 = number(2);
        assert!(!unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Mod) });
    }

    #[test]
    fn eval_multdiv_number_wraps_on_overflow_like_the_original_c_arithmetic() {
        let mut tv1 = number(VARNUMBER_MAX);
        let tv2 = number(2);
        assert!(unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Mul) });
        assert!(matches!(tv1.value, TypvalValue::Number(n) if n == VARNUMBER_MAX.wrapping_mul(2)));
    }

    #[test]
    fn eval_multdiv_number_type_error_on_tv1_returns_false() {
        let mut tv1 = TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() };
        let tv2 = number(1);
        assert!(!unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Mul) });
    }

    #[test]
    fn eval_multdiv_number_type_error_on_tv2_returns_false() {
        let mut tv1 = number(1);
        let tv2 = TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() };
        assert!(!unsafe { eval_multdiv_number(&mut tv1, &tv2, MulDivOp::Mul) });
    }
}
