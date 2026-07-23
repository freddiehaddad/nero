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
//! Deferred: everything else - the actual parser/lvalue/loop/call
//! machinery is a separate, substantial undertaking of its own. The
//! remaining "leaf" arithmetic helpers (`eval_addlist` needs
//! `tv_list_concat`, not yet translated; `eval_concat_str`/
//! `grow_string_tv`/`eval_addsub_number`/`eval_multdiv_number` are
//! reasonable next candidates) are not yet examined in detail.

use crate::eval::typval_defs::{VarnumberT, VARNUMBER_MAX, VARNUMBER_MIN};

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
pub unsafe fn eval_addblob(
    tv1: &mut crate::eval::typval_defs::TypvalT,
    tv2: &crate::eval::typval_defs::TypvalT,
) {
    use crate::eval::typval::{tv_blob_alloc, tv_blob_len, tv_blob_set_ret};
    use crate::eval::typval_defs::TypvalValue;

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
    // via `tv_blob_unref` rather than the crate's generic (and
    // private) `tv_clear_simple` dispatcher, since `tv1` is already
    // known to be `Blob`-typed from the pattern match above.
    unsafe { crate::eval::typval::tv_blob_unref(b1) };
    // SAFETY: `b` is a valid pointer just allocated above.
    unsafe { tv_blob_set_ret(tv1, b) };
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
        use crate::eval::typval_defs::{TypvalT, TypvalValue};

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
        use crate::eval::typval_defs::{TypvalT, TypvalValue};

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
        use crate::eval::typval_defs::{TypvalT, TypvalValue};

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
}
