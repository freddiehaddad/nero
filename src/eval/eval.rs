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
//! Also translated: `eval_addlist` - the last "leaf" arithmetic
//! function in this file, tractable once `eval/typval.rs` gained
//! `tv_list_copy`/`tv_list_extend`/`tv_list_concat` (harvested
//! specifically as this function's own dependency chain; needed a new
//! opaque `crate::types_defs::VimconvT` placeholder for
//! `tv_list_copy`'s `conv` parameter, which is only ever read by the
//! not-yet-translated `deep`-copy path). Like `eval_addblob`, releases
//! only `tv1`'s old list reference in the success path (via the
//! now-real `tv_list_unref`, called directly since `tv1` is already
//! known to be `List`-typed) - `tv2` is left for the caller (`eval5`);
//! unlike `eval_addblob`, the error path (when `tv_list_concat` fails)
//! releases BOTH operands, matching the original's own
//! `tv_clear(tv1); tv_clear(tv2);` exactly.
//!
//! **`eval.c`'s entire "leaf" arithmetic family (functions needing no
//! parser/lvalue machinery) is now complete.**
//!
//! Also translated, as the first real building blocks of `eval7`
//! itself (the innermost, primary-expression level of the recursive-
//! descent grammar) - each genuinely self-contained, needing no other
//! part of `eval7`/the parser to exist first:
//! - [`eval7_leader`]: applies a collected run of leading `!`/`-`
//!   (ignoring `+`) to an already-parsed operand, walking backward
//!   byte-by-byte exactly like the original's own pointer walk
//!   (including silently skipping interleaved whitespace/`+` bytes).
//!   Preserves a real subtlety: once a `!` converts a `Float` operand
//!   to a number/bool, any FURTHER leader operators in the same walk
//!   apply to the now-integer value, not the original float - modeled
//!   with a mutable `is_float` flag that can only ever flip
//!   `true -> false`. `eval7` itself calls this function TWICE (once
//!   right after a number literal with `numeric_only = true`, stopping
//!   early at any `!`; once at the very end with `numeric_only =
//!   false` to finish the job) - both calls are exercised directly in
//!   this module's own tests.
//! - [`string2float`]/`strtod_c_locale`: a from-scratch, hand-verified
//!   `strtod()`-equivalent (whitespace/sign/`"inf"`/`"infinity"`/
//!   `"nan(...)"`/decimal-with-exponent forms), needed since Rust's
//!   standard library has no "parse the longest valid prefix, report
//!   how much was consumed" primitive. Verified against 30 real glibc
//!   `strtod()` reference outputs via a WSL C program - this also
//!   caught a real, faithfully-replicated QUIRK in the original's own
//!   code (not a bug in this translation): its hand-rolled `"inf"`/
//!   `"-inf"`/`"nan"` 3-4-byte prefix shortcuts intercept a bare,
//!   unsigned `"INFINITY"`/`"nan(123)"` BEFORE the general fallback's
//!   own longer-form parsing ever runs, so those only consume 3 bytes,
//!   not 8/12 - only a LEADING SIGN bypasses the shortcuts and reaches
//!   the fallback's full long-form handling (see [`string2float`]'s
//!   own doc comment). Hex-float syntax (`0x1.8p3`, which real
//!   `strtod()` DOES parse) is deliberately not implemented - a
//!   substantial undertaking of its own, and unreachable in practice
//!   today since this function's only real caller, [`eval_number`],
//!   never passes it such input (see [`eval_number`]'s own doc
//!   comment for why) - panics via `unimplemented!()` if ever reached
//!   rather than silently returning a wrong value.
//! - [`eval_number`]: parses a decimal/hex/octal/binary integer, a
//!   float, or a `0z`-prefixed blob literal, needing
//!   `charset.rs`'s already-real `skipdigits`/`vim_str2nr`/`hex2nr`
//!   and `eval/typval.rs`'s already-real `tv_blob_alloc`/
//!   `tv_blob_free`/`tv_blob_set_ret`/`garray.rs`'s `ga_append`.
//!   Returns `(status, bytes_consumed)` rather than mutating a shared
//!   `char **arg` pointer in place, matching this crate's own
//!   established "return updated position info" idiom (e.g.
//!   `eval7_leader` above) over replicating C pointer-aliasing
//!   directly. `bytes_consumed` is well-defined as `0` on `FAIL`,
//!   matching the original's own "`*arg` only advances on success"
//!   structure.
//!
//! Deferred: everything else - the actual parser/lvalue/loop/call
//! machinery is a separate, substantial undertaking of its own. In
//! particular, `eval7` itself is not yet translated: it still needs
//! `eval_string`/`eval_lit_string` (quoted string literals),
//! `eval_list`/`eval_dict`/`eval_lit_dict` (list/dict literals),
//! `get_lambda_tv` (lambda expressions), `eval_option` (needs the real
//! `options[]` table, a separate MAJOR undertaking), `eval_env_var`/
//! `eval_interp_string` (`$VAR`/interpolated strings), `get_reg_contents`
//! (`@register`, needs `register.c`), `get_name_len`/`eval_func`/
//! `eval_variable` (variable/function-name lookup, needs the funccal
//! stack), and `handle_subscript` (`[...]`/`.`/`->`/`(...)` chaining) -
//! before it can itself be translated even partially.

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

/// Make a copy of list `tv1` and append list `tv2` (`eval_addlist`).
///
/// Returns `false` on failure (releasing both `tv1`/`tv2`, matching
/// the original's own `tv_clear(tv1); tv_clear(tv2);` in that path
/// exactly) - in practice always reachable-but-unexercised today,
/// since `eval/typval.rs`'s `tv_list_concat`/`tv_list_copy` never
/// actually fail for the `deep == false` path this crate can
/// currently reach. On success, only `tv1`'s OLD list reference is
/// released (via the now-real `tv_list_unref`, called directly since
/// `tv1` is already known to be `List`-typed) - `tv2` is left for the
/// caller (`eval5`, not yet translated), matching [`eval_addblob`]'s
/// own asymmetric cleanup pattern.
///
/// # Safety
/// `tv1`/`tv2` must both be `TypvalValue::List`-typed (matching the
/// original's own contract - the caller, Vimscript's `+` operator
/// dispatch in `eval5`, is responsible for checking this BEFORE
/// calling); any non-null list pointer they hold must be valid.
pub unsafe fn eval_addlist(tv1: &mut TypvalT, tv2: &TypvalT) -> bool {
    use crate::eval::typval::{tv_list_concat, tv_list_unref};

    let TypvalValue::List(l1) = tv1.value else {
        unreachable!("eval_addlist: tv1 must be List-typed (caller's own contract)")
    };
    let TypvalValue::List(l2) = tv2.value else {
        unreachable!("eval_addlist: tv2 must be List-typed (caller's own contract)")
    };

    let mut var3 = TypvalT::default();
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { tv_list_concat(l1, l2, &mut var3) } {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            tv_list_unref(l1);
            tv_list_unref(l2);
        }
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_list_unref(l1) };
    *tv1 = var3;

    true
}

/// Apply the leading `!`/`-` before an `eval7` expression to `rettv`,
/// walking backward from `*end_leader` towards the start of `leader`
/// (`eval7_leader`).
///
/// `leader` is the full leader byte range `eval7` collects ahead of a
/// number/expression (e.g. `!  -  ` before a number - may contain
/// interleaved whitespace and `+`, both silently skipped byte-by-byte,
/// exactly like the original's own pointer walk, which examines every
/// byte between `start_leader`/`end_leader`, not just "meaningful"
/// leader tokens). `*end_leader` is how many of `leader`'s bytes
/// (counting from its start) are still "in scope" to process -
/// `eval7` itself passes `leader.len()` initially (its own
/// `end_leader` pointer starts equal to `start_leader + leader.len()`);
/// updated in place to reflect how far the backward walk got before
/// stopping, so `leader[..*end_leader]` is left over, unconsumed, for
/// a later `numeric_only == false` call to handle - exactly matching
/// `eval7`'s own two call sites (once with `numeric_only = true` right
/// after parsing a number literal, once with `numeric_only = false` at
/// the very end, after subscript handling).
///
/// `numeric_only`: if `true`, only handle `+`/`-`; stop (without
/// consuming) at the first `!` found while walking backward.
///
/// Mirrors a real subtlety in the original rather than simplifying it
/// away: once a `!` flips a `Float` operand to boolean/number (setting
/// `rettv->v_type = VAR_BOOL` in the original), any FURTHER `-`/`!` in
/// the same walk operate on the now-integer `val`, not the original
/// float `f` - modeled here with a local, mutable `is_float` that can
/// only ever flip `true -> false`, never back. The original's
/// intermediate `VAR_BOOL` tag is itself always immediately
/// overwritten by the final `VAR_NUMBER`/`VAR_FLOAT` assignment after
/// the loop (the only other place `v_type` is read is the `==
/// VAR_FLOAT` checks, for which `VAR_BOOL` and `VAR_NUMBER` behave
/// identically) - so this translation never actually constructs a
/// `TypvalValue::Bool`, matching that observation.
///
/// # Safety
/// If `rettv`'s value is `List`/`Dict`/`Blob`/`Partial`/`Func`-typed
/// with a non-null pointer, that pointer must be valid - forwarded
/// from `tv_clear_simple`'s own safety doc, needed here since this
/// function always ends by releasing whatever `rettv` previously held
/// before overwriting it with the leader-applied result (or, on
/// error, before returning `FAIL`).
pub unsafe fn eval7_leader(
    rettv: &mut TypvalT,
    numeric_only: bool,
    leader: &[u8],
    end_leader: &mut usize,
) -> i32 {
    let mut error = false;
    let mut val: VarnumberT = 0;
    let mut f = 0.0_f64;
    let mut is_float = matches!(rettv.value, TypvalValue::Float(_));

    if let TypvalValue::Float(fl) = rettv.value {
        f = fl;
    } else {
        val = crate::eval::typval::tv_get_number_chk(rettv, Some(&mut error));
    }

    if error {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::typval::tv_clear_simple(rettv) };
        return crate::vim_defs::FAIL;
    }

    while *end_leader > 0 {
        *end_leader -= 1;
        match leader[*end_leader] {
            b'!' => {
                if numeric_only {
                    *end_leader += 1;
                    break;
                }
                if is_float {
                    is_float = false;
                    val = VarnumberT::from(f == 0.0);
                } else {
                    val = VarnumberT::from(val == 0);
                }
            }
            b'-' => {
                if is_float {
                    f = -f;
                } else {
                    val = -val;
                }
            }
            _ => {}
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::typval::tv_clear_simple(rettv) };
    rettv.value = if is_float { TypvalValue::Float(f) } else { TypvalValue::Number(val) };

    crate::vim_defs::OK
}

/// Convert the string to a floating point number. This uses (real
/// `strtod()`; `setlocale(LC_NUMERIC, "C")` has been used earlier to
/// make sure this always uses a decimal point (`string2float`).
///
/// Returns the parsed value and the length of the text that was
/// consumed (0 if no valid prefix exists at all).
///
/// A real, faithfully-replicated quirk (confirmed directly against
/// the original's own source, not a bug in this translation): a bare,
/// unsigned `"INFINITY"`/`"infinity"` is caught by the hand-rolled
/// 3-byte `"inf"` prefix check below and only consumes 3 bytes,
/// leaving `"INITY"`/`"inity"` unconsumed - only a LEADING SIGN (e.g.
/// `"+infinity"`) bypasses all 3 hand-rolled checks and reaches the
/// general fallback's own full 8-byte `"infinity"` form. Verified
/// against real glibc `strtod()` behavior (which does NOT have this
/// quirk on its own) specifically to confirm the quirk originates from
/// neovim's own hand-rolled checks, not from `strtod()` itself.
///
/// # Deferred
/// Hex-float syntax (e.g. `0x1.8p3`, which real `strtod()` DOES parse,
/// verified directly against glibc via a WSL reference program) is
/// not recognized here: implementing it faithfully (matching glibc's
/// exact hex-mantissa rounding behavior) is a substantial undertaking
/// of its own. Unreachable in practice today: this function's only
/// real caller so far, [`eval_number`], only ever calls this after its
/// OWN separate check has already confirmed `text` matches
/// `[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?` (see `eval_number`'s own doc
/// comment) - a shape that can never start with `0x`/`0X`. Panics via
/// `unimplemented!()` if ever reached (e.g. by some future,
/// currently-nonexistent caller), rather than silently returning a
/// wrong value.
#[must_use]
pub fn string2float(text: &[u8]) -> (f64, usize) {
    // MS-Windows does not deal with "inf" and "nan" properly - kept as
    // its own exact-prefix check, structured exactly like the
    // original: no leading whitespace/sign tolerance and no
    // word-boundary requirement after the match (matches C's
    // `STRNICMP(text, "inf", 3) == 0`, a bare "compare the first N
    // bytes only" check, which is what produces the "INFINITY" quirk
    // documented above) - checked BEFORE the general `strtod`-
    // equivalent fallback below, which handles every other case
    // (including leading whitespace/sign and the long "infinity"/
    // "nan(...)" forms).
    if text.len() >= 3 && text[..3].eq_ignore_ascii_case(b"inf") {
        return (f64::INFINITY, 3);
    }
    if text.len() >= 4 && text[..4].eq_ignore_ascii_case(b"-inf") {
        return (f64::NEG_INFINITY, 4);
    }
    if text.len() >= 3 && text[..3].eq_ignore_ascii_case(b"nan") {
        return (f64::NAN, 3);
    }

    strtod_c_locale(text)
}

/// A `strtod()`-equivalent prefix parser: consumes as much of a valid
/// C-locale floating point literal as possible from the start of
/// `text`, returning the parsed value and the number of bytes
/// consumed (0 if no valid prefix exists at all) - the fallback
/// [`string2float`] uses once its own 3 hand-rolled `"inf"`/`"-inf"`/
/// `"nan"` exact-prefix checks don't match. Verified against 30 real
/// glibc `strtod()` reference outputs (whitespace/sign handling,
/// `"5."`/`".5"`/bare `"."`, exponents with/without a following digit,
/// `"infinity"`/`"nan(...)"` long forms, trailing garbage, empty
/// input) via a WSL C reference program.
fn strtod_c_locale(text: &[u8]) -> (f64, usize) {
    let mut i = 0;
    while i < text.len() && crate::ascii_defs::ascii_isspace(i32::from(text[i])) {
        i += 1;
    }
    let sign_pos = i;

    let mut j = i;
    if j < text.len() && (text[j] == b'+' || text[j] == b'-') {
        j += 1;
    }
    let is_negative = text.get(sign_pos) == Some(&b'-');

    if let Some(rest) = text.get(j..) {
        if rest.len() >= 8 && rest[..8].eq_ignore_ascii_case(b"infinity") {
            let value = if is_negative { f64::NEG_INFINITY } else { f64::INFINITY };
            return (value, j + 8);
        }
        if rest.len() >= 3 && rest[..3].eq_ignore_ascii_case(b"inf") {
            let value = if is_negative { f64::NEG_INFINITY } else { f64::INFINITY };
            return (value, j + 3);
        }
        if rest.len() >= 3 && rest[..3].eq_ignore_ascii_case(b"nan") {
            let mut end = j + 3;
            // Optional "(n-char-sequence)" suffix.
            if text.get(end) == Some(&b'(') {
                let mut k = end + 1;
                while k < text.len() && (text[k] == b'_' || text[k].is_ascii_alphanumeric()) {
                    k += 1;
                }
                if text.get(k) == Some(&b')') {
                    end = k + 1;
                }
            }
            return (f64::NAN, end);
        }
    }

    if j + 1 < text.len() && text[j] == b'0' && (text[j + 1] == b'x' || text[j + 1] == b'X') {
        unimplemented!(
            "strtod_c_locale: hex float syntax (0x1.8p3) is not supported - unreachable in \
             practice today, see string2float's own doc comment"
        );
    }

    // Decimal number: digits, optional ".digits", optional
    // "[eE][sign]digits" - must have at least one digit somewhere in
    // the integer or fractional part.
    let digits_start = j;
    let mut k = j;
    while k < text.len() && text[k].is_ascii_digit() {
        k += 1;
    }
    let int_digits = k - digits_start;

    let mut frac_digits = 0;
    if k < text.len() && text[k] == b'.' {
        let after_dot = k + 1;
        let mut m = after_dot;
        while m < text.len() && text[m].is_ascii_digit() {
            m += 1;
        }
        frac_digits = m - after_dot;
        if frac_digits > 0 {
            k = m;
        } else if int_digits > 0 {
            // "5." is a valid float (5.0) even with no fractional
            // digits, as long as there was at least one integer
            // digit - matches strtod exactly (verified: consumed=2
            // for "5.").
            k = after_dot;
        }
    }

    if int_digits == 0 && frac_digits == 0 {
        return (0.0, 0);
    }

    let mut exp_end = k;
    if k < text.len() && (text[k] == b'e' || text[k] == b'E') {
        let mut m = k + 1;
        if m < text.len() && (text[m] == b'+' || text[m] == b'-') {
            m += 1;
        }
        let exp_digits_start = m;
        while m < text.len() && text[m].is_ascii_digit() {
            m += 1;
        }
        if m > exp_digits_start {
            exp_end = m;
        }
        // else: a trailing "e"/"e+"/"e-" with no exponent digits is
        // NOT part of the number (matches strtod: consumed=1 for
        // "5e", i.e. just "5").
    }

    let matched = &text[digits_start..exp_end];
    // `matched` contains only ASCII digits/'.'/'e'/'E'/exponent sign
    // by construction, so it's always valid UTF-8 and always a
    // syntactically valid Rust float literal.
    let s = std::str::from_utf8(matched).expect("matched is ASCII-only by construction");
    let magnitude: f64 = s.parse().expect("matched is a valid float literal by construction");

    let value = if is_negative { -magnitude } else { magnitude };
    (value, exp_end)
}

/// Allocate a variable for a number constant. Also deals with `"0z"`
/// for blob (`eval_number`).
///
/// Returns the parse status (`OK`/`FAIL`) and the number of bytes of
/// `arg` consumed (well-defined as `0` on `FAIL`, matching the
/// original's own "`*arg` is only advanced on success" structure - the
/// blob-literal odd-hex-digit-count error path and the `vim_str2nr`
/// `len == 0` error path both return `FAIL` BEFORE their own
/// respective `*arg = bp;`/`*arg += len;` assignment).
///
/// # Preconditions
/// `arg` must be non-empty with `arg[0]` a decimal digit (`b'0'..=
/// b'9'`) - the caller's own responsibility, matching `eval7`'s own
/// switch-on-first-byte dispatch that only reaches this function for
/// such input in the first place. Not itself re-validated here (a
/// non-digit `arg[0]` just makes the final `vim_str2nr` fallback
/// report `len == 0`/`FAIL`, exactly as harmless as the original's own
/// lack of a redundant check).
///
/// # Deferred
/// The real, user-facing `emsg`/`semsg` calls on both error paths (odd
/// hex digit count in a blob literal; totally unparseable decimal/hex/
/// octal/binary number) are omitted - needs `message.c`'s display
/// pipeline, not tractable - while the identical error-status behavior
/// is kept exactly, matching this crate's established "skip the
/// display, keep the state" policy.
#[must_use]
pub fn eval_number(arg: &[u8], rettv: &mut TypvalT, evaluate: bool, want_string: bool) -> (i32, usize) {
    use crate::ascii_defs::{ascii_isdigit, ascii_isxdigit};
    use crate::macros_defs::ascii_isalpha;

    let mut p = crate::charset::skipdigits(arg.get(1..).unwrap_or(&[])) + 1;
    let mut get_float = false;

    if !want_string
        && arg.get(p) == Some(&b'.')
        && arg.get(p + 1).is_some_and(|&c| ascii_isdigit(i32::from(c)))
    {
        get_float = true;
        p += 2 + crate::charset::skipdigits(arg.get(p + 2..).unwrap_or(&[]));
        if matches!(arg.get(p), Some(&b'e') | Some(&b'E')) {
            p += 1;
            if matches!(arg.get(p), Some(&b'-') | Some(&b'+')) {
                p += 1;
            }
            if !arg.get(p).is_some_and(|&c| ascii_isdigit(i32::from(c))) {
                get_float = false;
            } else {
                p += 1 + crate::charset::skipdigits(arg.get(p + 1..).unwrap_or(&[]));
            }
        }
        if arg.get(p).is_some_and(|&c| ascii_isalpha(i32::from(c)) || c == b'.') {
            get_float = false;
        }
    }

    if get_float {
        let (f, len) = string2float(arg);
        if evaluate {
            rettv.value = TypvalValue::Float(f);
        }
        (crate::vim_defs::OK, len)
    } else if arg.first() == Some(&b'0') && matches!(arg.get(1), Some(&b'z') | Some(&b'Z')) {
        // Blob constant: 0z0123456789abcdef
        let blob = if evaluate { crate::eval::typval::tv_blob_alloc() } else { std::ptr::null_mut() };

        let mut bp = 2;
        while let Some(&hi) =
            arg.get(bp).filter(|&&c| ascii_isxdigit(i32::from(c)))
        {
            let lo_is_hex = arg.get(bp + 1).is_some_and(|&c| ascii_isxdigit(i32::from(c)));
            if !lo_is_hex {
                if !blob.is_null() {
                    // SAFETY: freshly allocated by tv_blob_alloc above,
                    // not yet shared with anything else (rettv was
                    // never wired up on this early-error path).
                    unsafe { crate::eval::typval::tv_blob_free(blob) };
                }
                return (crate::vim_defs::FAIL, 0);
            }
            let lo = arg[bp + 1];
            if !blob.is_null() {
                let byte = ((crate::charset::hex2nr(i32::from(hi)) << 4)
                    + crate::charset::hex2nr(i32::from(lo))) as u8;
                // SAFETY: forwarded from tv_blob_alloc's own contract -
                // `blob` was just allocated above and is exclusively
                // owned here so far.
                unsafe { (*blob).bv_ga.ga_append(byte) };
            }
            bp += 2;
            if arg.get(bp) == Some(&b'.') && arg.get(bp + 1).is_some_and(|&c| ascii_isxdigit(i32::from(c)))
            {
                bp += 1;
            }
        }
        if !blob.is_null() {
            // SAFETY: forwarded from tv_blob_alloc's own contract.
            unsafe { crate::eval::typval::tv_blob_set_ret(rettv, blob) };
        }
        (crate::vim_defs::OK, bp)
    } else {
        // decimal, hex or octal number
        let mut len: i32 = 0;
        let mut n: VarnumberT = 0;
        crate::charset::vim_str2nr(
            arg,
            None,
            Some(&mut len),
            crate::charset::STR2NR_ALL,
            Some(&mut n),
            None,
            0,
            true,
            None,
        );
        if len == 0 {
            return (crate::vim_defs::FAIL, 0);
        }
        if evaluate {
            rettv.value = TypvalValue::Number(n);
        }
        (crate::vim_defs::OK, len as usize)
    }
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

    #[test]
    fn eval_addlist_concatenates_lists_in_order() {
        use crate::eval::typval::{tv_list_alloc, tv_list_append_tv, tv_list_free};

        let _lock = crate::globals::global_state_test_lock();
        let l1 = tv_list_alloc(1);
        let l2 = tv_list_alloc(1);
        unsafe {
            tv_list_append_tv(l1, &number(1));
            tv_list_append_tv(l2, &number(2));
        }
        let mut tv1 = TypvalT { value: TypvalValue::List(l1), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::List(l2), ..Default::default() };

        unsafe {
            let ok = eval_addlist(&mut tv1, &tv2);
            assert!(ok);
            let TypvalValue::List(result) = tv1.value else {
                panic!("expected a List-typed result");
            };
            assert_ne!(result, l1); // l1 itself was released, this is a fresh copy
            assert_eq!((*result).lv_len, 2);
            assert!(matches!((*(*result).lv_first).li_tv.value, TypvalValue::Number(1)));
            assert!(matches!((*(*result).lv_last).li_tv.value, TypvalValue::Number(2)));
            tv_list_free(l2);
            tv_list_free(result);
        }
    }

    #[test]
    fn eval_addlist_releases_tv1s_old_list_reference() {
        use crate::eval::typval::{tv_list_alloc, tv_list_free};

        let _lock = crate::globals::global_state_test_lock();
        // l1 with refcount 2 - eval_addlist must release exactly one
        // reference (the copy it internally makes is independent), so
        // l1 survives with refcount 1 afterward, still safely
        // dereferencable to confirm the release genuinely happened.
        let l1 = tv_list_alloc(0);
        unsafe { (*l1).lv_refcount = 2 };
        let l2 = tv_list_alloc(0);
        let mut tv1 = TypvalT { value: TypvalValue::List(l1), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::List(l2), ..Default::default() };

        unsafe {
            assert!(eval_addlist(&mut tv1, &tv2));
            assert_eq!((*l1).lv_refcount, 1);
            let TypvalValue::List(result) = tv1.value else {
                panic!("expected a List-typed result");
            };
            tv_list_free(l1);
            tv_list_free(l2);
            tv_list_free(result);
        }
    }

    #[test]
    fn eval_addlist_both_empty_gives_empty_result() {
        use crate::eval::typval::{tv_list_alloc, tv_list_free};

        let _lock = crate::globals::global_state_test_lock();
        let l1 = tv_list_alloc(0);
        let l2 = tv_list_alloc(0);
        let mut tv1 = TypvalT { value: TypvalValue::List(l1), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::List(l2), ..Default::default() };

        unsafe {
            assert!(eval_addlist(&mut tv1, &tv2));
            let TypvalValue::List(result) = tv1.value else {
                panic!("expected a List-typed result");
            };
            assert_eq!((*result).lv_len, 0);
            tv_list_free(l2);
            tv_list_free(result);
        }
    }

    #[test]
    fn eval7_leader_single_minus_negates_number() {
        let mut tv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let leader = b"-";
        let mut end_leader = leader.len();
        assert_eq!(unsafe { eval7_leader(&mut tv, true, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert!(matches!(tv.value, TypvalValue::Number(-5)));
        assert_eq!(end_leader, 0);
    }

    #[test]
    fn eval7_leader_double_minus_cancels_out() {
        let mut tv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let leader = b"--";
        let mut end_leader = leader.len();
        assert_eq!(unsafe { eval7_leader(&mut tv, true, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert!(matches!(tv.value, TypvalValue::Number(5)));
        assert_eq!(end_leader, 0);
    }

    #[test]
    fn eval7_leader_minus_negates_float() {
        let mut tv = TypvalT { value: TypvalValue::Float(2.5), ..Default::default() };
        let leader = b"-";
        let mut end_leader = leader.len();
        assert_eq!(unsafe { eval7_leader(&mut tv, true, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert!(matches!(tv.value, TypvalValue::Float(f) if f == -2.5));
    }

    #[test]
    fn eval7_leader_bang_on_zero_number_gives_one() {
        let mut tv = TypvalT { value: TypvalValue::Number(0), ..Default::default() };
        let leader = b"!";
        let mut end_leader = leader.len();
        assert_eq!(unsafe { eval7_leader(&mut tv, false, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert!(matches!(tv.value, TypvalValue::Number(1)));
    }

    #[test]
    fn eval7_leader_bang_on_nonzero_number_gives_zero() {
        let mut tv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let leader = b"!";
        let mut end_leader = leader.len();
        assert_eq!(unsafe { eval7_leader(&mut tv, false, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert!(matches!(tv.value, TypvalValue::Number(0)));
    }

    #[test]
    fn eval7_leader_bang_on_zero_float_gives_number_one() {
        // A `!` on a float converts it to a number/bool result, unlike
        // `-` which stays float - see this function's own doc comment.
        let mut tv = TypvalT { value: TypvalValue::Float(0.0), ..Default::default() };
        let leader = b"!";
        let mut end_leader = leader.len();
        assert_eq!(unsafe { eval7_leader(&mut tv, false, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert!(matches!(tv.value, TypvalValue::Number(1)));
    }

    #[test]
    fn eval7_leader_bang_on_nonzero_float_gives_number_zero() {
        let mut tv = TypvalT { value: TypvalValue::Float(4.25), ..Default::default() };
        let leader = b"!";
        let mut end_leader = leader.len();
        assert_eq!(unsafe { eval7_leader(&mut tv, false, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert!(matches!(tv.value, TypvalValue::Number(0)));
    }

    #[test]
    fn eval7_leader_numeric_only_stops_before_bang_leaving_it_unconsumed() {
        // leader "!-" before a number: numeric_only=true (eval7's first
        // call, right after parsing the number literal) applies the
        // '-' but stops at '!', leaving it for a later, numeric_only =
        // false call - exactly matching eval7's own two call sites.
        let mut tv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let leader = b"!-";
        let mut end_leader = leader.len();
        assert_eq!(unsafe { eval7_leader(&mut tv, true, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert!(matches!(tv.value, TypvalValue::Number(-5)));
        // The leading "!" (index 0) is unconsumed: end_leader == 1.
        assert_eq!(end_leader, 1);
    }

    #[test]
    fn eval7_leader_two_stage_call_matches_eval7s_own_pattern() {
        // Simulates eval7's exact two-call sequence for "!-5": first
        // numeric_only=true (stops at '!'), then numeric_only=false
        // with the SAME leader and the updated end_leader.
        let mut tv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let leader = b"!-";
        let mut end_leader = leader.len();
        assert_eq!(unsafe { eval7_leader(&mut tv, true, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert_eq!(end_leader, 1);

        assert_eq!(unsafe { eval7_leader(&mut tv, false, leader, &mut end_leader) }, crate::vim_defs::OK);
        // NOT(-5) == 0 (matches real Vimscript "!-5" semantics).
        assert!(matches!(tv.value, TypvalValue::Number(0)));
        assert_eq!(end_leader, 0);
    }

    #[test]
    fn eval7_leader_backward_order_minus_then_bang_on_a_float() {
        // leader "-!" (both consumed in one numeric_only=false call):
        // walking backward hits '!' first (index 1, closest to the
        // number), THEN '-' (index 0) - matching real Vimscript
        // "-!2.5" == -(NOT 2.5) == -0 == 0.
        let mut tv = TypvalT { value: TypvalValue::Float(2.5), ..Default::default() };
        let leader = b"-!";
        let mut end_leader = leader.len();
        assert_eq!(unsafe { eval7_leader(&mut tv, false, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert!(matches!(tv.value, TypvalValue::Number(0)));
        assert_eq!(end_leader, 0);
    }

    #[test]
    fn eval7_leader_skips_interleaved_whitespace_and_plus_bytes() {
        // "eval7" collects '+' and whitespace into the leader region
        // too (its own leader-collection loop matches '!'/'-'/'+' and
        // calls skipwhite after each) - both are silently no-ops
        // during the backward walk.
        let mut tv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let leader = b"-  +  ";
        let mut end_leader = leader.len();
        assert_eq!(unsafe { eval7_leader(&mut tv, true, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert!(matches!(tv.value, TypvalValue::Number(-5)));
        assert_eq!(end_leader, 0);
    }

    #[test]
    fn eval7_leader_empty_leader_is_a_noop_besides_number_coercion() {
        let mut tv = TypvalT { value: TypvalValue::Number(7), ..Default::default() };
        let leader: &[u8] = b"";
        let mut end_leader = 0;
        assert_eq!(unsafe { eval7_leader(&mut tv, false, leader, &mut end_leader) }, crate::vim_defs::OK);
        assert!(matches!(tv.value, TypvalValue::Number(7)));
    }

    #[test]
    fn eval7_leader_returns_fail_on_number_conversion_error() {
        // TypvalValue::Unknown is one of tv_get_number_chk's own
        // documented error cases (no real value to convert).
        let mut tv = TypvalT { value: TypvalValue::Unknown, ..Default::default() };
        let leader = b"-";
        let mut end_leader = leader.len();
        assert_eq!(
            unsafe { eval7_leader(&mut tv, true, leader, &mut end_leader) },
            crate::vim_defs::FAIL
        );
    }

    /// Every case here was cross-checked against real glibc
    /// `strtod()` via a WSL C reference program (see this module's
    /// `string2float`/`strtod_c_locale` doc comments).
    #[test]
    fn string2float_matches_real_strtod_reference_outputs() {
        let cases: &[(&[u8], f64, usize)] = &[
            (b"5", 5.0, 1),
            (b"5.5", 5.5, 3),
            (b"-5.5", -5.5, 4),
            (b"+5.5", 5.5, 4),
            (b"  5.5", 5.5, 5),
            (b"5.", 5.0, 2),
            (b".5", 0.5, 2),
            (b".", 0.0, 0),
            (b"5e10", 5e10, 4),
            (b"5e-10", 5e-10, 5),
            (b"5e+10", 5e10, 5),
            (b"5e", 5.0, 1),
            (b"5.5.5", 5.5, 3),
            (b"abc", 0.0, 0),
            (b"+abc", 0.0, 0),
            (b"inf", f64::INFINITY, 3),
            (b"-inf", f64::NEG_INFINITY, 4),
            (b"nan", f64::NAN, 3),
            (b"INF", f64::INFINITY, 3),
            // "INFINITY"/"infinity" (no sign) are caught by the
            // hand-rolled "inf" 3-char prefix check BEFORE the general
            // strtod-equivalent fallback ever sees them - a real,
            // faithfully-replicated quirk of the original (confirmed
            // directly in its source: `STRNICMP(text, "inf", 3) == 0`
            // matches "INF..." regardless of what follows), NOT a bug
            // in this translation. Only a LEADING SIGN (e.g.
            // "+infinity" below) bypasses all 3 hand-rolled checks and
            // reaches the fallback's own full 8-char "infinity" form.
            (b"INFINITY", f64::INFINITY, 3),
            (b"infinity", f64::INFINITY, 3),
            (b"+infinity", f64::INFINITY, 9),
            // Same hand-rolled-shortcut quirk as "INFINITY" above: a
            // bare "nan(123)" is caught by the 3-byte "nan" check
            // before the fallback's own "(n-char-sequence)" suffix
            // logic ever runs. A leading sign bypasses it, exactly
            // like "+infinity" above.
            (b"nan(123)", f64::NAN, 3),
            (b"+nan(123)", f64::NAN, 9),
            (b"1.2e3xyz", 1200.0, 5),
            (b"  -inf", f64::NEG_INFINITY, 6),
            (b"  +5", 5.0, 4),
            (b"9.87654321098765", 9.87654321098765, 16),
            (b"", 0.0, 0),
        ];

        for &(input, expected_value, expected_len) in cases {
            let (value, len) = string2float(input);
            assert_eq!(len, expected_len, "input={:?}", std::str::from_utf8(input));
            if expected_value.is_nan() {
                assert!(value.is_nan(), "input={:?}", std::str::from_utf8(input));
            } else {
                assert_eq!(value, expected_value, "input={:?}", std::str::from_utf8(input));
            }
        }
    }

    /// Exercises `strtod_c_locale`'s own `"nan(...)"` n-char-sequence
    /// suffix parsing directly (bypassing `string2float`'s hand-rolled
    /// bare-"nan" shortcut, which would otherwise intercept these
    /// before the suffix logic runs - see
    /// `string2float_matches_real_strtod_reference_outputs`'s own
    /// comment for why). Verified against real glibc `strtod()` via
    /// the same WSL reference program.
    #[test]
    fn strtod_c_locale_parses_nan_suffix_variants() {
        let (v, len) = strtod_c_locale(b"nan()");
        assert!(v.is_nan());
        assert_eq!(len, 5);

        let (v, len) = strtod_c_locale(b"nan(abc_123)");
        assert!(v.is_nan());
        assert_eq!(len, 12);
    }

    #[test]
    #[should_panic(expected = "hex float syntax")]
    fn string2float_panics_on_hex_float_syntax() {
        // Real strtod DOES parse "0x1.8p3" as 12.0 (verified via the
        // same WSL reference program) - deliberately not replicated,
        // see string2float's own doc comment for why.
        let _ = string2float(b"0x1.8p3");
    }

    #[test]
    fn eval_number_parses_plain_decimal() {
        let mut tv = TypvalT::default();
        // Note: strict vim_str2nr rejects a number immediately followed
        // by an alphanumeric char (e.g. "123abc" would FAIL, matching
        // real Vimscript rejecting that as a likely typo) - a trailing
        // non-alnum delimiter is used here instead.
        let (ret, len) = eval_number(b"123+456", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 3);
        assert!(matches!(tv.value, TypvalValue::Number(123)));
    }

    #[test]
    fn eval_number_parses_hex() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"0x1A", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 4);
        assert!(matches!(tv.value, TypvalValue::Number(26)));
    }

    #[test]
    fn eval_number_parses_octal() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"017", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 3);
        assert!(matches!(tv.value, TypvalValue::Number(15)));
    }

    #[test]
    fn eval_number_parses_simple_float() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"1.5", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 3);
        assert!(matches!(tv.value, TypvalValue::Float(f) if f == 1.5));
    }

    #[test]
    fn eval_number_parses_float_with_exponent() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"1.5e10", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 6);
        assert!(matches!(tv.value, TypvalValue::Float(f) if f == 1.5e10));
    }

    #[test]
    fn eval_number_float_followed_by_second_dot_is_not_a_float() {
        // "1.2.3" - the trailing second '.' after the fractional part
        // makes eval_number reject the float interpretation entirely
        // (matches "let vers = 1.2.3" not being parsed as a float,
        // per the original's own doc comment) - falls through to
        // vim_str2nr, which stops at the first '.'.
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"1.2.3", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 1);
        assert!(matches!(tv.value, TypvalValue::Number(1)));
    }

    #[test]
    fn eval_number_float_followed_by_alpha_is_not_a_float() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"1.2x", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 1);
        assert!(matches!(tv.value, TypvalValue::Number(1)));
    }

    #[test]
    fn eval_number_want_string_suppresses_float_detection() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"1.5", &mut tv, true, true);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 1);
        assert!(matches!(tv.value, TypvalValue::Number(1)));
    }

    #[test]
    fn eval_number_evaluate_false_still_computes_length_but_not_value() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"1.5e2", &mut tv, false, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 5);
        assert!(matches!(tv.value, TypvalValue::Unknown));
    }

    #[test]
    fn eval_number_parses_blob_literal() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"0z0102", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 6);
        let TypvalValue::Blob(b) = tv.value else {
            panic!("expected a Blob-typed result");
        };
        assert!(!b.is_null());
        unsafe {
            let bv_ga = &(*b).bv_ga;
            assert_eq!(bv_ga.ga_len, 2);
            assert_eq!(bv_ga.ga_data[0], 0x01);
            assert_eq!(bv_ga.ga_data[1], 0x02);
            assert_eq!((*b).bv_refcount, 1);
            crate::eval::typval::tv_blob_free(b);
        }
    }

    #[test]
    fn eval_number_parses_blob_literal_with_embedded_dot_separator() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"0z01.0203", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 9);
        let TypvalValue::Blob(b) = tv.value else {
            panic!("expected a Blob-typed result");
        };
        unsafe {
            let bv_ga = &(*b).bv_ga;
            assert_eq!(bv_ga.ga_len, 3);
            assert_eq!(&bv_ga.ga_data[..bv_ga.ga_len as usize], &[0x01, 0x02, 0x03]);
            crate::eval::typval::tv_blob_free(b);
        }
    }

    #[test]
    fn eval_number_blob_odd_hex_digit_count_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"0z012", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::FAIL);
        assert_eq!(len, 0);
        // rettv untouched on this error path.
        assert!(matches!(tv.value, TypvalValue::Unknown));
    }

    #[test]
    fn eval_number_blob_evaluate_false_does_not_allocate_but_still_computes_length() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_number(b"0z0102", &mut tv, false, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 6);
        assert!(matches!(tv.value, TypvalValue::Unknown));
    }
}
