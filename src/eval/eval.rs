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
//! has a SECOND real caller, `eval/executor.c`'s own `tv_op_string`
//! (now translated, see `eval/executor.rs`).
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
//! - [`eval_lit_string`] (+ a private `find_lit_string_end`
//!   helper): parses a `'str''ing'` literal
//!   (single-quoted, `''` reducing to a literal `'`). Deliberately
//!   scans/copies at the byte level rather than replicating the
//!   original's multi-byte-character-aware pointer walk - see its own
//!   doc comment for why this is provably equivalent for well-formed
//!   UTF-8 input (`'` can never appear as part of a multi-byte
//!   sequence). Only the `interpolate = false` case is modeled
//!   (`eval7`'s own only call site) - see its own doc comment.
//!
//! Also translated: `EvalargT`/`EVAL_EVALUATE`/`clear_evalarg`
//! (`evalarg_T`, `eval.h`), `ExprType` (`exprtype_T`, `eval.h`), and
//! the FULL `eval0`-`eval7` recursive-descent parser/evaluator chain
//! itself, plus `typval_compare` and a minimal `handle_subscript`.
//! `arg: &[u8]` is always "the remaining unconsumed input, indexed
//! from 0" (never a full original buffer plus a cursor), and every
//! function returns `(status, bytes_consumed)` - matching this
//! module's own already-established `eval_number`/`eval7_leader`
//! idiom - rather than mutating a shared `char **arg` in place.
//! `evalarg: Option<&mut EvalargT>` models the original's nullable
//! `evalarg_T *const evalarg`; the ternary/`||`/`&&` operators'
//! "construct a local, zero-flags `evalarg_T` when none was passed"
//! fallback is translated literally (a local `EvalargT::default()`,
//! used in place of the caller's `None`, for exactly the scope the
//! original's own `local_evalarg` covers).
//!
//! `eval7` started deliberately minimal and has grown since: number/
//! float/`0z`-blob literals (`eval_number`), single-quoted string
//! literals (`eval_lit_string`), double-quoted string literals
//! (`eval_string` - full except `\<C-...>` special-key escapes, see
//! its own doc comment), list/dict/literal-dict literals (`eval_list`/
//! `eval_dict`/`eval_lit_dict` - `eval_dict`'s own deferred `{expr}`-
//! vs-dict-literal speculative pre-check aside, see its own doc
//! comment), leading `!`/`-`/`+` (`eval7_leader`), parenthesized
//! sub-expressions (recursing into `eval1`), plain variable references
//! (`get_name_len`/`eval_variable`/`check_vars`), option values
//! (`eval_option`/`find_option_var_end`, now that `option.rs`'s real
//! `options[]` table/`find_option`/`get_option_value`/`get_varp_from`
//! engine all exist), environment variables (`eval_env_var`/
//! `get_env_len`, now that `charset.rs`'s `vim_isidc` exists and
//! `os/env.rs`'s `vim_getenv` covers the common "real OS environment
//! variable" case), interpolated strings (`eval_interp_string`,
//! `$"..."`/`$'...'`, including embedded `{expr}` via
//! `crate::eval::vars::eval_one_expr_in_str`), and register contents
//! (`get_reg_contents`, now that `register.rs` exists - every named/
//! numbered register is genuinely empty today since nothing yanks/
//! deletes/puts yet, not a stub) are all real. Only genuinely
//! substantial remaining pieces still panic via `unimplemented!()`,
//! each with its own specific, documented reason: lambda expressions
//! (`get_lambda_tv`, detected via the new
//! `crate::eval::userfunc::is_lambda_start` and declined cleanly,
//! rather than being misparsed as a dict literal, needs closure/
//! lambda compilation), a `$VAR` whose value `vim_getenv` can't
//! resolve directly (needs `expand_env_save`'s own `~`/`~user`/
//! `` `=expr` ``-handling fallback, see `os/env.rs`'s own doc comment).
//! Function calls (`eval_func`) are now real for BUILTIN functions only:
//! `call_func` dispatches through `builtin_function`/`find_internal_func`
//! into `crate::eval::funcs`'s new `FUNCTIONS` table (62 functions so
//! far, including a full cluster of `float_op_wrapper`-style math
//! functions (`sin()`/`cos()`/`sqrt()`/`pow()`/etc.) alongside the
//! original handful - the start of a long tail, `eval/funcs.c` itself
//! implements ~641 builtins; see `eval/funcs.rs`'s own module doc
//! comment for the full current list). A user-function-SHAPED name
//! (not recognized as builtin) still correctly, gracefully `FAIL`s
//! today (`find_func` finds nothing, since nothing parses `:function`
//! yet) -
//! genuinely correct, not a stub; only if `find_func` somehow ever
//! returned a real, non-null `UfuncT` would `call_func` reach its own
//! `unimplemented!()` (needs `call_user_func_check`, the whole
//! Ex-command execution engine, still unattempted). A byte that
//! would make `get_name_len` itself report "no name here at all" (e.g.
//! trailing garbage or an unbalanced closing delimiter) is instead a
//! real, graceful `FAIL` - exactly matching `get_name_len`'s own
//! behavior for such input, not a deferred gap.
//! `handle_subscript` mirrors this same minimality: only the real
//! "nothing follows" fast path (no `[`/`.`/`(`/`->` continuation)
//! returns successfully; anything that would actually need
//! `eval_index`/`call_func_rettv`/`eval_method`/`eval_lambda`
//! (all substantial, separate undertakings) panics via
//! `unimplemented!()` instead. Its own `preceded_by_whitespace`
//! parameter replaces the original's `!ascii_iswhite(*(*arg - 1))`
//! check (looking at the byte immediately BEFORE `arg`, which this
//! module's own "remaining slice, indexed from 0" idiom can't express
//! directly) - see its own doc comment for why this is exactly
//! equivalent.
//!
//! `eval1` (ternary `?:`/`??`) through `eval6` (`*`/`/`/`%`) are
//! translated IN FULL: every dependency they need (the whole "leaf"
//! arithmetic family above, `typval_compare`, `tv_check_num`/
//! `tv_check_str`, `mb_strcmp_ic`, `p_ic`) already existed. `eval0`
//! (the top-level entry point) is translated in full too, using the
//! new `crate::ex_docmd::ends_excmd`/`check_nextcmd` (harvested
//! alongside, `ex_docmd.rs`'s own tiny-harvest precedent) - its `eap:
//! Option<&mut ExargT>` parameter is modeled but has no real caller
//! yet (nothing translated constructs a real `ExargT` and calls
//! `eval0` through it), so this is genuinely a standalone expression
//! evaluator today, not yet wired into any real Ex-command context
//! (`:echo`/`:let`/`:if`, none of which are translated).
//!
//! `typval_compare` is translated in full except `EXPR_MATCH`/
//! `EXPR_NOMATCH` (`=~`/`!~`) against two strings - needs
//! `pattern_match`, the real regex engine (`regexp.c`), confirmed
//! globally blocked (matches `search.c`'s own already-documented
//! status) - `unimplemented!()`s only when actually reached (neither
//! operand Blob/List/Dict/Func/Float/Number-typed, i.e. a genuine
//! string/Bool/Special `=~`/`!~` comparison).
//!
//! Found and fixed, directly tied to building `eval5` (its real
//! caller, for the first time): `eval_concat_str`'s type-error path
//! only released `tv1` (always a no-op in practice) and silently
//! skipped releasing `tv2`'s own reference when `tv2` genuinely can't
//! be stringified (e.g. `"str" . [1, 2, 3]`) - a real, if narrow,
//! reference-leak divergence from the original's own `tv_clear(tv1);
//! tv_clear(tv2);`, now fixed to release both.
//!
//! Also translated: the GC mark-phase's `set_ref_in_ht`/
//! `set_ref_in_list_items`/`set_ref_in_item_dict`/
//! `set_ref_in_item_list`/`set_ref_in_item_partial`/`set_ref_in_item`
//! family (there is no separate `eval/gc.c` - this logic lives
//! directly in `eval.c`). Marks every list/dict/partial/named-function
//! transitively reachable from a value with a `copy_id`, using an
//! explicit worklist ([`crate::eval::typval_defs::HtStackT`]/
//! [`crate::eval::typval_defs::ListStackT`], allocated via
//! `Box::into_raw`/`Box::from_raw`) instead of recursion, to avoid
//! stack overflow on deeply-nested structures - verified directly via
//! a dedicated test walking 20,000 levels of dict-in-dict nesting.
//! `set_ref_in_ht`/`set_ref_in_item_dict` take `*mut DictT` rather than
//! the original's bare `*mut hashtab_T`, matching `vars_clear_ext`'s
//! own already-established precedent (`eval/vars.rs`) for the exact
//! same `dv_index`-vs-`TV_DICT_HI2DI` reason. The original's
//! `QUEUE_FOREACH` dict-watcher notification inside
//! `set_ref_in_item_dict` is omitted - `DictT` has no `watchers` field
//! yet (the same accepted gap already documented on `DictT` itself).

use crate::charset::skipwhite;
use crate::eval::typval_defs::{TypvalT, TypvalValue, VarLockStatus, VarnumberT, VARNUMBER_MAX, VARNUMBER_MIN};
use crate::option_defs::OptIndex;
use crate::vim_defs::{FAIL, OK};

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
/// `tv1` is assumed already stringifiable (`eval5`, this function's
/// only real caller, only calls it after confirming that via
/// `tv_check_str` whenever evaluation is actually happening), matching
/// the original's own "s1 already checked" comment.
///
/// On that type-error path, the original releases BOTH operands
/// (`tv_clear(tv1); tv_clear(tv2);`) - this crate's own earlier
/// translation only released `tv1` (itself always a no-op release in
/// practice, per the paragraph above) and silently skipped `tv2`,
/// which genuinely CAN be a `List`/`Dict`/`Blob`/`Partial` needing a
/// real refcount release (e.g. `"str" . [1, 2, 3]`) - found and fixed
/// here, directly tied to building `eval5` (this function's real
/// caller) for the first time.
///
/// # Safety
/// If `tv1`/`tv2`'s value is `List`/`Dict`/`Blob`/`Partial`-typed with
/// a non-null pointer, that pointer must be valid - forwarded to
/// `eval/typval.rs`'s `tv_clear_simple`'s own contract, used here to
/// release `tv1`'s old value when it can't be grown in place, and to
/// release both operands on the type-error path.
pub unsafe fn eval_concat_str(tv1: &mut TypvalT, tv2: &TypvalT) -> bool {
    use crate::eval::typval::{tv_clear_simple, tv_get_string, tv_get_string_chk};

    let s1 = tv_get_string(tv1);
    let Some(s2) = tv_get_string_chk(tv2) else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            tv_clear_simple(tv1);
            tv_clear_simple(tv2);
        }
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

/// Return `pt`'s own name if set, else its underlying function's name,
/// or `None` for a null `pt` (`partial_name`).
///
/// # Safety
/// `pt`, if non-null, must be a valid pointer to a live
/// [`crate::eval::typval_defs::PartialT`] whose own `pt_func`, if
/// non-null, must be a valid pointer to a live
/// [`crate::eval::typval_defs::UfuncT`].
#[must_use]
pub unsafe fn partial_name(pt: *const crate::eval::typval_defs::PartialT) -> Option<Vec<u8>> {
    if pt.is_null() {
        return None;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let pt = unsafe { &*pt };
    if let Some(name) = &pt.pt_name {
        return Some(name.clone());
    }
    if !pt.pt_func.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        return Some(unsafe { &*pt.pt_func }.uf_name.clone());
    }
    None
}

/// Compare two `Func`/`Partial` values for equality (`func_equal`).
///
/// # Safety
/// If `tv1`/`tv2`'s value is `Partial`-typed with a non-null pointer,
/// that pointer must be a valid, live
/// [`crate::eval::typval_defs::PartialT`] (see [`partial_name`]'s own
/// safety doc); its own `pt_dict`, if non-null, must be a valid, live
/// [`crate::eval::typval_defs::DictT`], recursively satisfying
/// `tv_dict_equal`'s own safety contract; every entry of `pt_argv`
/// must satisfy `tv_equal`'s own safety contract.
#[must_use]
pub unsafe fn func_equal(tv1: &TypvalT, tv2: &TypvalT, ic: bool) -> bool {
    use crate::eval::typval::{tv_dict_equal, tv_equal};
    use crate::eval::typval_defs::PartialT;

    // empty and NULL function name considered the same
    let partial_of = |tv: &TypvalT| -> *const PartialT {
        match &tv.value {
            TypvalValue::Partial(p) => *p,
            _ => std::ptr::null(),
        }
    };
    let name_of = |tv: &TypvalT, p: *const PartialT| -> Option<Vec<u8>> {
        match &tv.value {
            TypvalValue::Func(name) => name.clone(),
            // SAFETY: forwarded from this function's own safety doc.
            _ => unsafe { partial_name(p) },
        }
    };

    let p1 = partial_of(tv1);
    let p2 = partial_of(tv2);
    let s1 = name_of(tv1, p1).filter(|s| !s.is_empty());
    let s2 = name_of(tv2, p2).filter(|s| !s.is_empty());
    match (&s1, &s2) {
        (None, None) => {}
        (None, Some(_)) | (Some(_), None) => return false,
        (Some(a), Some(b)) => {
            if a != b {
                return false;
            }
        }
    }

    // empty dict and NULL dict is different
    // SAFETY: forwarded from this function's own safety doc.
    let d1 = if p1.is_null() { std::ptr::null_mut() } else { unsafe { (*p1).pt_dict } };
    // SAFETY: forwarded from this function's own safety doc.
    let d2 = if p2.is_null() { std::ptr::null_mut() } else { unsafe { (*p2).pt_dict } };
    if d1.is_null() || d2.is_null() {
        if d1 != d2 {
            return false;
        }
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { tv_dict_equal(d1, d2, ic) } {
            return false;
        }
    }

    // empty list and no list considered the same
    // SAFETY: forwarded from this function's own safety doc.
    let argv1: &[TypvalT] = if p1.is_null() { &[] } else { unsafe { &(*p1).pt_argv } };
    // SAFETY: forwarded from this function's own safety doc.
    let argv2: &[TypvalT] = if p2.is_null() { &[] } else { unsafe { &(*p2).pt_argv } };
    if argv1.len() != argv2.len() {
        return false;
    }
    for (a1, a2) in argv1.iter().zip(argv2.iter()) {
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { tv_equal(a1, a2, ic) } {
            return false;
        }
    }

    true
}

/// Function-local counter for [`get_copy_id`] (`eval.c`'s own
/// function-local `static int current_copyID = 0`), matching
/// [`EVAL7_RECURSE`]'s already-established translation of the same C
/// idiom.
static CURRENT_COPY_ID: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// Amount [`get_copy_id`] advances by on each call (`COPYID_INC`,
/// `eval.h`) - advances by 2 (not 1) because the last bit is reserved
/// for `previous_funccal`'s own separate marking use, normally
/// ignored when comparing copy IDs (not yet relevant to this crate -
/// no funccal-stack GC marking exists yet - but kept for numeric
/// fidelity with the original).
const COPYID_INC: i32 = 2;

/// Get the next (unique) copy ID (`get_copyID`).
///
/// Used for traversing nested structures, e.g. when serializing them
/// or garbage collecting (neither translated yet) - the original's
/// own doc comment.
#[must_use]
pub fn get_copy_id() -> i32 {
    // SAFETY: GlobalCell accessed through this crate's established
    // single-threaded-main-loop convention.
    let id = unsafe { *CURRENT_COPY_ID.get_mut() } + COPYID_INC;
    unsafe { *CURRENT_COPY_ID.get_mut() = id };
    id
}

/// Recursion depth counter for [`var_item_copy`] (`eval.c`'s own
/// function-local `static int recurse = 0`), matching
/// [`EVAL7_RECURSE`]'s already-established translation of the same C
/// idiom.
static VAR_ITEM_COPY_RECURSE: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// Maximum nesting of lists/dicts allowed when making a copy
/// (`DICT_MAXNEST`) - `eval.c`, `eval/typval.c`, and `eval/vars.c` all
/// separately `#define` the same constant in the original.
const DICT_MAXNEST: i32 = 100;

/// Make a copy of an item (`var_item_copy`).
///
/// Lists and Dictionaries are also copied.
///
/// `conv`, if non-null, converts all copied strings - accepted for
/// signature fidelity but never dereferenced here: EVERY real call
/// site in the whole original codebase (`f_copy`/`f_deepcopy`, and
/// every other direct [`crate::eval::typval::tv_list_copy`]/
/// [`crate::eval::typval::tv_dict_copy`] caller) always passes `NULL`
/// for it, so the original's own `conv != NULL` `string_convert`
/// branch is provably unreachable dead code for any real caller -
/// matching `tv_list_copy`'s own already-established doc comment
/// reasoning for the exact same parameter.
///
/// `deep`, if true, copies the container AND every contained
/// container, recursively. `copy_id`, if non-zero, reuses an earlier
/// copy of the same container instead of making a second, separate
/// one - so deep-copying `[inner, inner]` (the same list referenced
/// twice) produces `[copy, copy]` (the same identity twice) rather
/// than two independent copies; not used when `deep` is false. Not
/// dereferenced/consulted at all for `Blob`s, matching the original's
/// own `tv_blob_copy` call (a blob has no nested containers to
/// recurse into, so it is always fully "deep" already).
///
/// Recursion is limited to `DICT_MAXNEST` levels, matching the
/// original's own guard against runaway/cyclic structures - the
/// original's own `emsg(_(e_variable_nested_too_deep_for_making_copy))`
/// is omitted (message display, not tractable; the identical `FAIL`
/// return is kept), matching this whole module's own established
/// convention for user-facing error text (see [`eval7`]'s own
/// recursion-limit handling for the same convention already in use).
///
/// # Safety
/// If `from`'s (or, recursively, any of its own nested items') value
/// is `List`/`Dict`/`Blob`/`Partial`-typed with a non-null pointer,
/// that pointer must be valid.
pub unsafe fn var_item_copy(
    conv: *const crate::types_defs::VimconvT,
    from: &TypvalT,
    to: &mut TypvalT,
    deep: bool,
    copy_id: i32,
) -> i32 {
    use crate::eval::typval::{tv_blob_copy, tv_copy, tv_dict_copy, tv_list_copy, tv_list_copyid, tv_list_latest_copy, tv_list_ref};

    // SAFETY: GlobalCell accessed through this crate's established
    // single-threaded-main-loop convention.
    let recurse = unsafe { *VAR_ITEM_COPY_RECURSE.get_mut() };
    if recurse >= DICT_MAXNEST {
        return FAIL;
    }
    unsafe { *VAR_ITEM_COPY_RECURSE.get_mut() = recurse + 1 };

    let mut ret = OK;

    match &from.value {
        TypvalValue::Number(_)
        | TypvalValue::Float(_)
        | TypvalValue::Func(_)
        | TypvalValue::Partial(_)
        | TypvalValue::Bool(_)
        | TypvalValue::Special(_)
        | TypvalValue::String(_) => {
            // String's own "conv == NULL" branch is always taken here
            // (see this function's own doc comment) - always a plain
            // tv_copy, matching every other scalar type.
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_copy(from, to) };
        }
        TypvalValue::List(l) => {
            let l = *l;
            to.v_lock = VarLockStatus::Unlocked;
            let copy = if l.is_null() {
                std::ptr::null_mut()
            } else if copy_id != 0 && unsafe { tv_list_copyid(l) } == copy_id {
                // Use the copy made earlier.
                let latest = unsafe { tv_list_latest_copy(l) };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_list_ref(latest) };
                latest
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_list_copy(conv, l, deep, copy_id) }
            };
            to.value = TypvalValue::List(copy);
            if copy.is_null() && !l.is_null() {
                ret = FAIL;
            }
        }
        TypvalValue::Blob(b) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_blob_copy(*b, to) };
        }
        TypvalValue::Dict(d) => {
            let d = *d;
            to.v_lock = VarLockStatus::Unlocked;
            let copy = if d.is_null() {
                std::ptr::null_mut()
            } else if copy_id != 0 && unsafe { (*d).dv_copy_id } == copy_id {
                // Use the copy made earlier.
                let latest = unsafe { (*d).dv_copydict };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { (*latest).dv_refcount += 1 };
                latest
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_dict_copy(conv, d, deep, copy_id) }
            };
            to.value = TypvalValue::Dict(copy);
            if copy.is_null() && !d.is_null() {
                ret = FAIL;
            }
        }
        TypvalValue::Unknown => {
            debug_assert!(false, "var_item_copy(UNKNOWN) - internal_error in the original");
            ret = FAIL;
        }
    }

    unsafe { *VAR_ITEM_COPY_RECURSE.get_mut() = recurse };
    ret
}

/// Mark all lists/dicts referenced through every item in `d` with
/// `copy_id`, using an explicit worklist instead of recursion, to
/// avoid stack overflow on deeply-nested structures (`set_ref_in_ht`).
///
/// Takes `*mut DictT` rather than the original's bare `*mut
/// hashtab_T` - see [`crate::eval::typval_defs::HtStackT`]'s own doc
/// comment for why (the same reason already established for
/// `vars_clear_ext` in `eval/vars.rs`).
///
/// # Safety
/// `d` must be a valid, non-null pointer to a live
/// [`crate::eval::typval_defs::DictT`], and every item transitively
/// reachable from it (through nested lists/dicts/partials) must be
/// valid. `list_stack`, if non-null, must point to a valid `*mut
/// ListStackT` slot.
pub unsafe fn set_ref_in_ht(
    d: *mut crate::eval::typval_defs::DictT,
    copy_id: i32,
    list_stack: *mut *mut crate::eval::typval_defs::ListStackT,
) -> bool {
    use crate::eval::typval_defs::{DictitemT, HtStackT};

    let mut abort = false;
    let mut ht_stack: *mut HtStackT = std::ptr::null_mut();
    let mut cur_d = d;

    loop {
        if !abort {
            // SAFETY: forwarded from this function's own safety doc.
            let items: Vec<*mut DictitemT> = unsafe { (*cur_d).dv_index.values().copied().collect() };
            for item in items {
                if abort {
                    break;
                }
                // SAFETY: forwarded from this function's own safety doc.
                abort = unsafe {
                    set_ref_in_item(&mut (*item).di_tv, copy_id, &mut ht_stack, list_stack)
                };
            }
        }

        if ht_stack.is_null() {
            break;
        }

        // SAFETY: `ht_stack` is a live node previously pushed by
        // `set_ref_in_item_dict`, forwarded from this function's own
        // safety doc.
        cur_d = unsafe { (*ht_stack).ht };
        let tempitem = ht_stack;
        // SAFETY: forwarded from this function's own safety doc.
        ht_stack = unsafe { (*tempitem).prev };
        // SAFETY: `tempitem` was allocated via `Box::into_raw` by
        // `set_ref_in_item_dict`.
        drop(unsafe { Box::from_raw(tempitem) });
    }

    abort
}

/// Mark all lists/dicts referenced through every item in `l` with
/// `copy_id`, using an explicit worklist instead of recursion
/// (`set_ref_in_list_items`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live
/// [`crate::eval::typval_defs::ListT`], and every item transitively
/// reachable from it must be valid. `ht_stack`, if non-null, must
/// point to a valid `*mut HtStackT` slot.
pub unsafe fn set_ref_in_list_items(
    l: *mut crate::eval::typval_defs::ListT,
    copy_id: i32,
    ht_stack: *mut *mut crate::eval::typval_defs::HtStackT,
) -> bool {
    use crate::eval::typval_defs::ListStackT;

    let mut abort = false;
    let mut list_stack: *mut ListStackT = std::ptr::null_mut();
    let mut cur_l = l;

    loop {
        // SAFETY: forwarded from this function's own safety doc.
        let mut cur_item = unsafe { (*cur_l).lv_first };
        while !cur_item.is_null() {
            if abort {
                break;
            }
            // SAFETY: forwarded from this function's own safety doc.
            abort = unsafe {
                set_ref_in_item(&mut (*cur_item).li_tv, copy_id, ht_stack, &mut list_stack)
            };
            // SAFETY: forwarded from this function's own safety doc.
            cur_item = unsafe { (*cur_item).li_next };
        }

        if list_stack.is_null() {
            break;
        }

        // SAFETY: `list_stack` is a live node previously pushed by
        // `set_ref_in_item_list`, forwarded from this function's own
        // safety doc.
        cur_l = unsafe { (*list_stack).list };
        let tempitem = list_stack;
        // SAFETY: forwarded from this function's own safety doc.
        list_stack = unsafe { (*tempitem).prev };
        // SAFETY: `tempitem` was allocated via `Box::into_raw` by
        // `set_ref_in_item_list`.
        drop(unsafe { Box::from_raw(tempitem) });
    }

    abort
}

/// Mark the dict `dd` with `copy_id` (`set_ref_in_item_dict`). Also
/// see [`set_ref_in_item`].
///
/// The original's `QUEUE_FOREACH(w, &dd->watchers, ...)` dict-watcher
/// notification is omitted - `DictT` has no `watchers` field at all
/// yet (needs a `QUEUE` intrusive-linked-list translation first, the
/// same accepted gap already documented on `DictT` itself in
/// `eval/typval_defs.rs`).
///
/// # Safety
/// `dd`, if non-null, must be a valid pointer to a live
/// [`crate::eval::typval_defs::DictT`]. `ht_stack`, if non-null, must
/// point to a valid `*mut HtStackT` slot; `list_stack`, if non-null,
/// must point to a valid `*mut ListStackT` slot.
unsafe fn set_ref_in_item_dict(
    dd: *mut crate::eval::typval_defs::DictT,
    copy_id: i32,
    ht_stack: *mut *mut crate::eval::typval_defs::HtStackT,
    list_stack: *mut *mut crate::eval::typval_defs::ListStackT,
) -> bool {
    use crate::eval::typval_defs::HtStackT;

    if dd.is_null() || unsafe { (*dd).dv_copy_id } == copy_id {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*dd).dv_copy_id = copy_id };
    if ht_stack.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { set_ref_in_ht(dd, copy_id, list_stack) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let newitem = Box::into_raw(Box::new(HtStackT { ht: dd, prev: unsafe { *ht_stack } }));
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *ht_stack = newitem };

    false
}

/// Mark the list `ll` with `copy_id` (`set_ref_in_item_list`). Also
/// see [`set_ref_in_item`].
///
/// # Safety
/// `ll`, if non-null, must be a valid pointer to a live
/// [`crate::eval::typval_defs::ListT`]. `ht_stack`/`list_stack`, if
/// non-null, must point to valid slots.
unsafe fn set_ref_in_item_list(
    ll: *mut crate::eval::typval_defs::ListT,
    copy_id: i32,
    ht_stack: *mut *mut crate::eval::typval_defs::HtStackT,
    list_stack: *mut *mut crate::eval::typval_defs::ListStackT,
) -> bool {
    use crate::eval::typval_defs::ListStackT;

    if ll.is_null() || unsafe { (*ll).lv_copy_id } == copy_id {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*ll).lv_copy_id = copy_id };
    if list_stack.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { set_ref_in_list_items(ll, copy_id, ht_stack) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let newitem = Box::into_raw(Box::new(ListStackT { list: ll, prev: unsafe { *list_stack } }));
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *list_stack = newitem };

    false
}

/// Mark the partial `pt` with `copy_id` (`set_ref_in_item_partial`).
/// Also see [`set_ref_in_item`].
///
/// # Safety
/// `pt`, if non-null, must be a valid pointer to a live
/// [`crate::eval::typval_defs::PartialT`] whose own `pt_func`, if
/// non-null, points at a live `UfuncT`, and whose `pt_dict`, if
/// non-null, points at a live `DictT`. `ht_stack`/`list_stack`, if
/// non-null, must point to valid slots.
unsafe fn set_ref_in_item_partial(
    pt: *mut crate::eval::typval_defs::PartialT,
    copy_id: i32,
    ht_stack: *mut *mut crate::eval::typval_defs::HtStackT,
    list_stack: *mut *mut crate::eval::typval_defs::ListStackT,
) -> bool {
    if pt.is_null() || unsafe { (*pt).pt_copy_id } == copy_id {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*pt).pt_copy_id = copy_id };

    // SAFETY: forwarded from this function's own safety doc.
    let mut abort = unsafe {
        crate::eval::userfunc::set_ref_in_func((*pt).pt_name.as_deref(), (*pt).pt_func, copy_id)
    };

    // SAFETY: forwarded from this function's own safety doc.
    let pt_dict = unsafe { (*pt).pt_dict };
    if !pt_dict.is_null() {
        let mut dtv = TypvalT { value: TypvalValue::Dict(pt_dict), ..Default::default() };
        // SAFETY: forwarded from this function's own safety doc.
        abort = abort || unsafe { set_ref_in_item(&mut dtv, copy_id, ht_stack, list_stack) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let pt_argv = unsafe { &mut (*pt).pt_argv };
    for arg in pt_argv.iter_mut() {
        // SAFETY: forwarded from this function's own safety doc.
        abort = abort || unsafe { set_ref_in_item(arg, copy_id, ht_stack, list_stack) };
    }

    abort
}

/// Mark all lists/dicts referenced through `tv` with `copy_id`
/// (`set_ref_in_item`).
///
/// # Safety
/// If `tv`'s value is `List`/`Dict`/`Blob`/`Partial`-typed with a
/// non-null pointer, that pointer (and everything transitively
/// reachable from it) must be valid. `ht_stack`/`list_stack`, if
/// non-null, must point to valid slots.
pub unsafe fn set_ref_in_item(
    tv: &mut TypvalT,
    copy_id: i32,
    ht_stack: *mut *mut crate::eval::typval_defs::HtStackT,
    list_stack: *mut *mut crate::eval::typval_defs::ListStackT,
) -> bool {
    match &tv.value {
        TypvalValue::Dict(d) => {
            let d = *d;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { set_ref_in_item_dict(d, copy_id, ht_stack, list_stack) }
        }
        TypvalValue::List(l) => {
            let l = *l;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { set_ref_in_item_list(l, copy_id, ht_stack, list_stack) }
        }
        TypvalValue::Func(name) => {
            let name = name.clone();
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                crate::eval::userfunc::set_ref_in_func(name.as_deref(), std::ptr::null_mut(), copy_id)
            }
        }
        TypvalValue::Partial(p) => {
            let p = *p;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { set_ref_in_item_partial(p, copy_id, ht_stack, list_stack) }
        }
        TypvalValue::Unknown
        | TypvalValue::Bool(_)
        | TypvalValue::Special(_)
        | TypvalValue::Float(_)
        | TypvalValue::Number(_)
        | TypvalValue::String(_)
        | TypvalValue::Blob(_) => false,
    }
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

/// Evaluate a double-quoted string constant (`eval_string`).
///
/// `interpolate`, when `true`, means `arg` already points PAST the
/// opening quote (the caller - [`eval_interp_string`] - already
/// consumed it) and reduces `"{{"`/`"}}"` to `"{"`/`"}"` in the output
/// while stopping the scan at a bare (non-doubled) `"{"`, leaving
/// `arg`'s consumed offset pointing AT whichever of `"`/`"{"` stopped
/// it - the caller decides what to do next (advance past the quote,
/// or recurse for an embedded expression), matching the original's own
/// division of responsibility exactly. `false` (`eval7`'s own direct
/// call site) is the simpler, already-well-tested case: `arg` points
/// AT the opening quote itself, and the scan only ever stops at the
/// closing quote.
///
/// Scans/copies at the BYTE level rather than the original's own
/// multi-byte-character-aware `MB_PTR_ADV`/`mb_copy_char` walk - same
/// "no ASCII test byte (`\`, `"`, `{`, `}`, or any recognized escape
/// letter) can appear as part of a multi-byte UTF-8 sequence" reasoning
/// [`eval_lit_string`]'s own doc comment already establishes: copying
/// one raw byte at a time in the "not a recognized escape"/"plain
/// character" cases produces byte-identical output to the original's
/// own `mb_copy_char` for any well-formed UTF-8 input, since a
/// multi-byte character's own continuation bytes (always `>= 0x80`)
/// just get individually "copied" as their own plain-character
/// iterations instead of as one multi-byte unit - the final output
/// byte sequence is identical either way.
///
/// Returns `(status, bytes_consumed)`, matching this module's own
/// established idiom.
///
/// # Deferred
/// `\<C-...>`-style special-key escapes need `find_special_key`/
/// `trans_special`, which need the ENTIRE `keycodes.c` subsystem - key-
/// name tables, modifier parsing, a whole generated
/// `keycode_names.generated.h` - a substantial, separate undertaking
/// of its own, not a small add-on. Rather than silently mishandle it,
/// `unimplemented!()`s the MOMENT a `\<` is encountered anywhere in the
/// string (even during the first, string-end-finding pass, exactly
/// where the original itself would first need `find_special_key`).
/// Every OTHER escape form (`\b`/`\e`/`\f`/`\n`/`\r`/`\t`, hex/Unicode/
/// octal, and the literal default fallback) is translated in full.
#[must_use]
pub fn eval_string(arg: &[u8], rettv: &mut TypvalT, evaluate: bool, interpolate: bool) -> (i32, usize) {
    use crate::ascii_defs::{ascii_isxdigit, BS, CAR, ESC, FF, NL, TAB};
    use crate::charset::hex2nr;
    use crate::mbyte::utf_char2bytes;

    // First pass: find the end of the string, or (when interpolating)
    // the start of an embedded {expr} (this crate's own Vec<u8>-based
    // output never needs the original's own pre-computed allocation
    // size, so nothing else is tracked here).
    let off = usize::from(!interpolate);
    let mut p = off;
    while arg.get(p).is_some_and(|&c| c != b'"') {
        if arg[p] == b'\\' && arg.get(p + 1).is_some() {
            p += 1;
            if arg[p] == b'<' {
                unimplemented!(
                    "eval_string: \\<C-...>-style special-key escapes need find_special_key/\
                     trans_special, the whole keycodes.c subsystem - not yet translated"
                );
            }
        } else if interpolate && matches!(arg[p], b'{' | b'}') {
            if arg[p] == b'{' && arg.get(p + 1) != Some(&b'{') {
                // start of an embedded expression.
                break;
            }
            p += 1;
            if arg[p - 1] == b'}' && arg.get(p) != Some(&b'}') {
                // a stray, unescaped single '}' is an error.
                return (FAIL, 0);
            }
        }
        p += 1;
    }

    if arg.get(p) != Some(&b'"') && !(interpolate && arg.get(p) == Some(&b'{')) {
        // semsg(_("E114: Missing quote: %s"), *arg) omitted - message
        // display, not tractable; the identical FAIL is kept.
        return (FAIL, 0);
    }

    if !evaluate {
        return (OK, p + off);
    }

    // Second pass: copy the string, handling backslashed characters.
    let mut s = Vec::new();
    let mut q = off;
    while arg.get(q).is_some_and(|&c| c != b'"') {
        if arg[q] == b'\\' {
            q += 1;
            match arg.get(q) {
                Some(&b'b') => {
                    s.push(BS);
                    q += 1;
                }
                Some(&b'e') => {
                    s.push(ESC);
                    q += 1;
                }
                Some(&b'f') => {
                    s.push(FF);
                    q += 1;
                }
                Some(&b'n') => {
                    s.push(NL);
                    q += 1;
                }
                Some(&b'r') => {
                    s.push(CAR);
                    q += 1;
                }
                Some(&b't') => {
                    s.push(TAB);
                    q += 1;
                }
                // hex: "\x1", "\x12"; Unicode: "\u0023"
                Some(&c) if matches!(c, b'X' | b'x' | b'u' | b'U')
                    && arg.get(q + 1).is_some_and(|&d| ascii_isxdigit(i32::from(d))) =>
                {
                    let is_x = c.eq_ignore_ascii_case(&b'X');
                    let max_digits = if is_x {
                        2
                    } else if c == b'u' {
                        4
                    } else {
                        8
                    };
                    let mut nr: i32 = 0;
                    let mut n = max_digits;
                    loop {
                        n -= 1;
                        if n < 0 {
                            break;
                        }
                        if arg.get(q + 1).is_some_and(|&d| ascii_isxdigit(i32::from(d))) {
                            q += 1;
                            nr = nr.wrapping_shl(4).wrapping_add(hex2nr(i32::from(arg[q])));
                        } else {
                            break;
                        }
                    }
                    q += 1;
                    if is_x {
                        s.push(nr as u8);
                    } else {
                        // For "\u" store the number according to
                        // 'encoding'.
                        let mut buf = [0u8; crate::mbyte_defs::MB_MAXCHAR + 1];
                        let n = utf_char2bytes(nr, &mut buf);
                        s.extend_from_slice(&buf[..n as usize]);
                    }
                }
                // octal: "\1", "\12", "\123"
                Some(&c) if (b'0'..=b'7').contains(&c) => {
                    let mut val = i32::from(c - b'0');
                    q += 1;
                    if arg.get(q).is_some_and(|&d| (b'0'..=b'7').contains(&d)) {
                        val = (val << 3) + i32::from(arg[q] - b'0');
                        q += 1;
                        if arg.get(q).is_some_and(|&d| (b'0'..=b'7').contains(&d)) {
                            val = (val << 3) + i32::from(arg[q] - b'0');
                            q += 1;
                        }
                    }
                    s.push(val as u8);
                }
                // Special key, e.g.: "\<C-W>" - the first pass above
                // already unimplemented!()s before this second pass
                // could ever be reached with a '<' here; kept only so
                // this match stays exhaustive/documents the original's
                // own case.
                Some(&b'<') => {
                    unimplemented!(
                        "eval_string: \\<C-...>-style special-key escapes need \
                         find_special_key/trans_special - not yet translated"
                    );
                }
                // default: copy the byte literally (see this
                // function's own doc comment for why byte-level
                // copying is equivalent to mb_copy_char here) - also
                // covers the "\\x"/"\\u"/etc. with no digit following"
                // case (the guarded arm above didn't match, so control
                // falls through to here with `q` still pointing at
                // that same escape letter, exactly matching the
                // original's own "no digit -> fall through unchanged,
                // let the NEXT plain-character copy pick it up"
                // behavior).
                _ => {
                    if let Some(&c) = arg.get(q) {
                        s.push(c);
                        q += 1;
                    }
                }
            }
        } else if interpolate && matches!(arg[q], b'{' | b'}') {
            if arg[q] == b'{' && arg.get(q + 1) != Some(&b'{') {
                // start of an embedded expression - the first pass
                // already validated a stray '}' can't reach here.
                break;
            }
            q += 1; // reduce "{{" to "{" and "}}" to "}".
            if let Some(&c) = arg.get(q) {
                s.push(c);
                q += 1;
            }
        } else if let Some(&c) = arg.get(q) {
            s.push(c);
            q += 1;
        }
    }
    rettv.value = TypvalValue::String(Some(s));
    let mut end = q;
    if arg.get(end) == Some(&b'"') && !interpolate {
        end += 1;
    }
    (OK, end)
}

/// Scans `arg` for the byte offset where a `'literal'` string ends:
/// either the closing, un-escaped `'` (treating `''` as an escaped
/// literal quote), or - when `interpolate` is `true` - a bare
/// (non-doubled) `'{'` marking the start of an embedded expression
/// (with `'{{'`/`'}}'` reduced to a single `{`/`}` along the way).
/// `arg` starts at the opening `'` itself when `!interpolate`
/// (matching [`eval_lit_string`]'s own `off` convention), or already
/// past it when `interpolate`.
///
/// Shared by both of [`eval_lit_string`]'s own passes (first: "is this
/// a valid, closed literal string, and how long is it"; second, only
/// when `evaluate`: "copy its content", relying on this function's own
/// returned stop position as the copy loop's upper bound). Returns
/// `None` on any error: running off the end of `arg` without finding a
/// closing `'` (or, interpolating, a `{`), or (interpolating only) a
/// stray, unescaped single `}` - both collapse to the same `None`/
/// `FAIL` here, matching this crate's established "skip the display,
/// keep the state" policy (the original reports two different
/// messages for these, neither of which is tractable to display).
fn find_lit_string_end(arg: &[u8], interpolate: bool) -> Option<usize> {
    let mut p = usize::from(!interpolate);
    loop {
        match *arg.get(p)? {
            b'\'' => {
                if arg.get(p + 1) != Some(&b'\'') {
                    return Some(p);
                }
                p += 2;
            }
            c @ (b'{' | b'}') if interpolate => {
                if c == b'{' {
                    if arg.get(p + 1) != Some(&b'{') {
                        return Some(p);
                    }
                } else if arg.get(p + 1) != Some(&b'}') {
                    // a stray, unescaped single '}' is an error.
                    return None;
                }
                p += 2;
            }
            _ => p += 1,
        }
    }
}

/// Allocate a variable for a `'str''ing'` constant (`eval_lit_string`).
///
/// `interpolate`, when `true`, means `arg` already points PAST the
/// opening quote (the caller - [`eval_interp_string`] - already
/// consumed it) and reduces `"{{"`/`"}}"` to `"{"`/`"}"` in the output
/// while stopping the scan at a bare (non-doubled) `"{"`, leaving the
/// consumed offset pointing AT whichever of `'`/`{` stopped it - the
/// caller decides what to do next, matching [`eval_string`]'s own
/// identical division of responsibility (see its own doc comment).
/// `false` (`eval7`'s own direct call site) is the simpler,
/// already-well-tested case: `arg` points AT the opening quote itself,
/// and the scan only ever stops at the closing quote.
///
/// Scans/copies at the BYTE level (see `find_lit_string_end`)
/// rather than replicating the original's own multi-byte-character-
/// aware `MB_PTR_ADV`/`mb_copy_char` walk: `'`/`{`/`}` are all plain
/// ASCII bytes, and valid UTF-8 continuation/lead bytes are always
/// `>= 0x80`, so none of them can ever appear as part of a multi-byte
/// sequence - a byte-level scan finds the exact same stop positions,
/// and a byte-level copy produces byte-identical output, as the
/// original's character-aware walk would for any well-formed UTF-8
/// input.
///
/// Returns the parse status (`OK`/`FAIL`) and the number of bytes of
/// `arg` consumed (matching this module's own `eval_number`/
/// `eval7_leader` "return updated position info" idiom); well-defined
/// as `0` on `FAIL`.
///
/// # Deferred
/// The real `semsg(_("E115: Missing quote: %s"), *arg)` call on the
/// "no closing quote" error path is omitted - needs `message.c`'s
/// display pipeline - while the identical `FAIL` status is kept,
/// matching this crate's established "skip the display, keep the
/// state" policy.
#[must_use]
pub fn eval_lit_string(arg: &[u8], rettv: &mut TypvalT, evaluate: bool, interpolate: bool) -> (i32, usize) {
    let Some(close) = find_lit_string_end(arg, interpolate) else {
        return (crate::vim_defs::FAIL, 0);
    };

    let off = usize::from(!interpolate);
    if !evaluate {
        return (crate::vim_defs::OK, close + off);
    }

    let mut s = Vec::with_capacity(close.saturating_sub(off));
    let mut q = off;
    while q < close {
        // Any `'` (or, interpolating, `{`/`}`) seen here (before
        // reaching `close`, the position of the real stop point) must
        // be the first half of an escaped "''"/"{{"/"}}" pair - skip
        // it, keeping only the second character as a literal one
        // (`find_lit_string_end`'s own construction guarantees this:
        // any non-doubled occurrence would already have been the stop
        // point itself, or an error).
        if arg[q] == b'\'' || (interpolate && matches!(arg[q], b'{' | b'}')) {
            q += 1;
        }
        s.push(arg[q]);
        q += 1;
    }
    rettv.value = TypvalValue::String(Some(s));
    (crate::vim_defs::OK, close + off)
}

/// `eval.h`'s `AUTOLOAD_CHAR` (`'#'`) - the separator marking an
/// autoload-style function/variable name. `pub(crate)` since more than
/// one module (`eval/vars.rs`'s `valid_varname`) needs the same real
/// constant.
pub(crate) const AUTOLOAD_CHAR: u8 = b'#';

/// Whether character `c` can be used in a variable or function name
/// (`eval_isnamec`).
#[must_use]
pub fn eval_isnamec(c: i32) -> bool {
    crate::macros_defs::ascii_isalnum(c)
        || c == i32::from(b'_')
        || c == i32::from(b':')
        || c == i32::from(AUTOLOAD_CHAR)
}

/// Whether character `c` can be used as the FIRST character in a
/// variable or function name, excluding `'{'`/`'}'` (`eval_isnamec1`).
#[must_use]
pub fn eval_isnamec1(c: i32) -> bool {
    crate::macros_defs::ascii_isalpha(c) || c == i32::from(b'_')
}

/// Flag for expression evaluation: when missing (`eval_flags == 0`),
/// don't actually evaluate - only parse (`EVAL_EVALUATE`).
pub const EVAL_EVALUATE: i32 = 1;

/// Passed to an `eval*` function to enable evaluation (`evalarg_T`).
///
/// `eval_getline`/`eval_cookie` (copied from an `exarg_T` when
/// "getline" is `getsourceline`, for `:source`-driven multi-line
/// expression continuation) and `eval_tofree` (the "keep the original
/// command line" bookkeeping `clear_evalarg` releases) are modeled
/// but never populated by anything in this crate today - nothing
/// translated drives expression evaluation from a real `:source`
/// context yet, matching `ExargT.ea_getline`'s own current status.
#[derive(Debug, Default)]
pub struct EvalargT {
    /// `EVAL_*` flag values (`eval_flags`).
    pub eval_flags: i32,
    /// Copied from `exarg_T` when "getline" is `getsourceline`. Can be
    /// `None` (`eval_getline`).
    pub eval_getline: Option<crate::ex_cmds_defs::LineGetter>,
    /// Argument for `eval_getline()` (`eval_cookie`).
    pub eval_cookie: *mut std::ffi::c_void,
    /// Pointer to the last line obtained with `getsourceline()`
    /// (`eval_tofree`).
    pub eval_tofree: Option<Vec<u8>>,
}

/// After using `evalarg` filled from `eap`: free the memory
/// (`clear_evalarg`).
///
/// A real, faithful (if currently always-a-no-op) translation: nothing
/// in this crate populates `eval_tofree` yet, so the original's
/// `xfree`/command-line-swap logic never actually has anything to do -
/// kept as a real function anyway (small, simple, no design freedom to
/// get wrong) rather than omitted, ready for whenever a future
/// `:source`-driven caller populates `eval_tofree` for real.
pub fn clear_evalarg(evalarg: Option<&mut EvalargT>, eap: Option<&mut crate::ex_cmds_defs::ExargT>) {
    let Some(evalarg) = evalarg else { return };
    let Some(tofree) = evalarg.eval_tofree.take() else { return };

    if let Some(eap) = eap {
        // We may need to keep the original command line, e.g. for
        // ":let" it has the variable names. But we may also need the
        // new one, "nextcmd" points into it. Keep both.
        eap.cmdline_tofree = eap.arg.take();
        eap.arg = Some(tofree);
    }
    // else: xfree(evalarg->eval_tofree) - Rust's own drop of `tofree`
    // (already taken above) already does this.
}

/// Types for expressions (`exprtype_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExprType {
    #[default]
    Unknown,
    /// `==`
    Equal,
    /// `!=`
    Nequal,
    /// `>`
    Greater,
    /// `>=`
    Gequal,
    /// `<`
    Smaller,
    /// `<=`
    Sequal,
    /// `=~`
    Match,
    /// `!~`
    Nomatch,
    /// `is`
    Is,
    /// `isnot`
    Isnot,
}

/// Compare `typ1` and `typ2`. Put the result in `typ1`
/// (`typval_compare`).
///
/// Returns `false` on a type error, `true` on success - matching the
/// original's `FAIL`/`OK` (a plain `bool`, not `crate::vim_defs::OK`/
/// `FAIL` directly, matching `eval_addsub_number`/`eval_multdiv_number`/
/// `eval_addlist`'s own "no position/consumed-byte tracking needed"
/// precedent - this function, like those, operates on two already-
/// evaluated operands).
///
/// # Deferred
/// `ExprType::Match`/`ExprType::Nomatch` (`=~`/`!~`) against two
/// strings need `pattern_match`, the real regex engine (`regexp.c`) -
/// confirmed globally blocked, matching `search.c`'s own already-
/// documented status. `unimplemented!()`s only when actually reached:
/// this requires neither operand to be Blob/List/Dict/Func/Float/
/// Number-typed (each of those has its own earlier, dedicated branch
/// that either handles `=~`/`!~` as a hard `FAIL` - matching the
/// original's own "invalid operation" `emsg` for those types - or
/// simply never reaches the string-comparison fallback at all).
///
/// # Safety
/// If `typ1`/`typ2`'s value is `List`/`Dict`/`Blob`/`Partial`-typed
/// with a non-null pointer, that pointer must be a valid, live value,
/// recursively satisfying the same contract for anything it
/// (in)directly contains - forwarded from `tv_equal`/`tv_list_equal`/
/// `tv_dict_equal`/`tv_blob_equal`/`tv_clear_simple`'s own safety
/// docs.
pub unsafe fn typval_compare(typ1: &mut TypvalT, typ2: &TypvalT, typ: ExprType, ic: bool) -> bool {
    use crate::eval::typval::{
        tv_blob_equal, tv_clear_simple, tv_dict_equal, tv_equal, tv_get_float, tv_get_number,
        tv_get_string, tv_is_func, tv_list_equal,
    };

    let type_is = typ == ExprType::Is || typ == ExprType::Isnot;
    let n1: VarnumberT;

    if type_is && typ1.var_type() != typ2.var_type() {
        // For "is" a different type always means false, for "isnot"
        // it means true.
        n1 = VarnumberT::from(typ == ExprType::Isnot);
    } else if matches!(typ1.value, TypvalValue::Blob(_)) || matches!(typ2.value, TypvalValue::Blob(_)) {
        if type_is {
            let mut eq = typ1.var_type() == typ2.var_type();
            if eq {
                if let (TypvalValue::Blob(b1), TypvalValue::Blob(b2)) = (&typ1.value, &typ2.value) {
                    eq = b1 == b2;
                }
            }
            n1 = VarnumberT::from(if typ == ExprType::Isnot { !eq } else { eq });
        } else if typ1.var_type() != typ2.var_type() || !matches!(typ, ExprType::Equal | ExprType::Nequal) {
            // emsg("E977: Can only compare Blob with Blob")/
            // emsg(_(e_invalblob)) omitted - message display, not
            // tractable; the identical FAIL is kept.
            unsafe { tv_clear_simple(typ1) };
            return false;
        } else {
            let (TypvalValue::Blob(b1), TypvalValue::Blob(b2)) = (&typ1.value, &typ2.value) else {
                unreachable!("typ1/typ2 already confirmed Blob-typed above")
            };
            // SAFETY: forwarded from this function's own safety doc.
            let mut eq = unsafe { tv_blob_equal(*b1, *b2) };
            if typ == ExprType::Nequal {
                eq = !eq;
            }
            n1 = VarnumberT::from(eq);
        }
    } else if matches!(typ1.value, TypvalValue::List(_)) || matches!(typ2.value, TypvalValue::List(_)) {
        if type_is {
            let mut eq = typ1.var_type() == typ2.var_type();
            if eq {
                if let (TypvalValue::List(l1), TypvalValue::List(l2)) = (&typ1.value, &typ2.value) {
                    eq = l1 == l2;
                }
            }
            n1 = VarnumberT::from(if typ == ExprType::Isnot { !eq } else { eq });
        } else if typ1.var_type() != typ2.var_type() || !matches!(typ, ExprType::Equal | ExprType::Nequal) {
            // emsg("E691: Can only compare List with List")/
            // emsg("E692: Invalid operation for List") omitted.
            unsafe { tv_clear_simple(typ1) };
            return false;
        } else {
            let (TypvalValue::List(l1), TypvalValue::List(l2)) = (&typ1.value, &typ2.value) else {
                unreachable!("typ1/typ2 already confirmed List-typed above")
            };
            // SAFETY: forwarded from this function's own safety doc.
            let mut eq = unsafe { tv_list_equal(*l1, *l2, ic) };
            if typ == ExprType::Nequal {
                eq = !eq;
            }
            n1 = VarnumberT::from(eq);
        }
    } else if matches!(typ1.value, TypvalValue::Dict(_)) || matches!(typ2.value, TypvalValue::Dict(_)) {
        if type_is {
            let mut eq = typ1.var_type() == typ2.var_type();
            if eq {
                if let (TypvalValue::Dict(d1), TypvalValue::Dict(d2)) = (&typ1.value, &typ2.value) {
                    eq = d1 == d2;
                }
            }
            n1 = VarnumberT::from(if typ == ExprType::Isnot { !eq } else { eq });
        } else if typ1.var_type() != typ2.var_type() || !matches!(typ, ExprType::Equal | ExprType::Nequal) {
            // emsg("E735: Can only compare Dictionary with Dictionary")/
            // emsg("E736: Invalid operation for Dictionary") omitted.
            unsafe { tv_clear_simple(typ1) };
            return false;
        } else {
            let (TypvalValue::Dict(d1), TypvalValue::Dict(d2)) = (&typ1.value, &typ2.value) else {
                unreachable!("typ1/typ2 already confirmed Dict-typed above")
            };
            // SAFETY: forwarded from this function's own safety doc.
            let mut eq = unsafe { tv_dict_equal(*d1, *d2, ic) };
            if typ == ExprType::Nequal {
                eq = !eq;
            }
            n1 = VarnumberT::from(eq);
        }
    } else if tv_is_func(typ1) || tv_is_func(typ2) {
        if !matches!(typ, ExprType::Equal | ExprType::Nequal | ExprType::Is | ExprType::Isnot) {
            // emsg("E694: Invalid operation for Funcrefs") omitted.
            unsafe { tv_clear_simple(typ1) };
            return false;
        }
        let typ1_null_partial = matches!(&typ1.value, TypvalValue::Partial(p) if p.is_null());
        let typ2_null_partial = matches!(&typ2.value, TypvalValue::Partial(p) if p.is_null());
        let mut eq = if typ1_null_partial || typ2_null_partial {
            // When both partials are NULL, then they are equal.
            // Otherwise they are not equal.
            matches!(
                (&typ1.value, &typ2.value),
                (TypvalValue::Partial(p1), TypvalValue::Partial(p2)) if p1 == p2
            )
        } else if type_is {
            if matches!(typ1.value, TypvalValue::Func(_)) && matches!(typ2.value, TypvalValue::Func(_)) {
                // Strings are considered the same if their value is
                // the same.
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_equal(typ1, typ2, ic) }
            } else if let (TypvalValue::Partial(p1), TypvalValue::Partial(p2)) = (&typ1.value, &typ2.value) {
                p1 == p2
            } else {
                false
            }
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_equal(typ1, typ2, ic) }
        };
        if typ == ExprType::Nequal || typ == ExprType::Isnot {
            eq = !eq;
        }
        n1 = VarnumberT::from(eq);
    } else if (matches!(typ1.value, TypvalValue::Float(_)) || matches!(typ2.value, TypvalValue::Float(_)))
        && !matches!(typ, ExprType::Match | ExprType::Nomatch)
    {
        // If one of the two variables is a float, compare as a float.
        let f1 = tv_get_float(typ1);
        let f2 = tv_get_float(typ2);
        n1 = VarnumberT::from(match typ {
            ExprType::Is | ExprType::Equal => f1 == f2,
            ExprType::Isnot | ExprType::Nequal => f1 != f2,
            ExprType::Greater => f1 > f2,
            ExprType::Gequal => f1 >= f2,
            ExprType::Smaller => f1 < f2,
            ExprType::Sequal => f1 <= f2,
            ExprType::Unknown | ExprType::Match | ExprType::Nomatch => false,
        });
    } else if (matches!(typ1.value, TypvalValue::Number(_)) || matches!(typ2.value, TypvalValue::Number(_)))
        && !matches!(typ, ExprType::Match | ExprType::Nomatch)
    {
        // If one of the two variables is a number, compare as a number.
        let a = tv_get_number(typ1);
        let b = tv_get_number(typ2);
        n1 = VarnumberT::from(match typ {
            ExprType::Is | ExprType::Equal => a == b,
            ExprType::Isnot | ExprType::Nequal => a != b,
            ExprType::Greater => a > b,
            ExprType::Gequal => a >= b,
            ExprType::Smaller => a < b,
            ExprType::Sequal => a <= b,
            ExprType::Unknown | ExprType::Match | ExprType::Nomatch => false,
        });
    } else {
        let s1 = tv_get_string(typ1);
        let s2 = tv_get_string(typ2);
        let i = if !matches!(typ, ExprType::Match | ExprType::Nomatch) {
            crate::mbyte::mb_strcmp_ic(ic, &s1, &s2)
        } else {
            0
        };
        n1 = VarnumberT::from(match typ {
            ExprType::Is | ExprType::Equal => i == 0,
            ExprType::Isnot | ExprType::Nequal => i != 0,
            ExprType::Greater => i > 0,
            ExprType::Gequal => i >= 0,
            ExprType::Smaller => i < 0,
            ExprType::Sequal => i <= 0,
            ExprType::Match | ExprType::Nomatch => {
                unimplemented!(
                    "typval_compare: '=~'/'!~' against strings need pattern_match, the real \
                     regex engine (regexp.c) - not yet translated"
                );
            }
            ExprType::Unknown => false,
        });
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_clear_simple(typ1) };
    typ1.value = TypvalValue::Number(n1);
    true
}

/// Handle `expr[expr]`/`expr.name`/`expr(expr)`/`expr->name(expr)`
/// subscript chaining after an already-parsed primary expression
/// (`handle_subscript`).
///
/// Only the common "nothing follows" fast path is modeled: if the
/// original's own `while` loop condition would be true for even its
/// very first iteration (there really is a `[`/`.`/`(`/`->`
/// continuation to handle), this panics via `unimplemented!()` - the
/// real subscript/index/method-call/function-call logic
/// (`eval_index`/`call_func_rettv`/`eval_method`/`eval_lambda`) is a
/// substantial, separate undertaking, not attempted here.
///
/// `tv_is_luafunc(rettv)` (gating the original's own `v:lua`-specific
/// lambda-name-parsing branch) is always `false` here: nothing this
/// crate can currently construct produces a `VAR_PARTIAL` bound to a
/// Lua function (lambdas/`v:lua` support are both still
/// unimplemented), so that branch is dead code in this crate today -
/// correctly omitted rather than translated ahead of any real need.
///
/// `preceded_by_whitespace` replaces the original's own
/// `!ascii_iswhite(*(*arg - 1))` check (looking at the byte
/// immediately BEFORE `arg`): this module's own "remaining slice,
/// indexed from 0" parsing idiom has no way to look backward past its
/// own start, so callers instead report whether their own most recent
/// `skipwhite` call (immediately before invoking this function)
/// actually consumed at least one byte - exactly equivalent, since a
/// `skipwhite` call consuming zero bytes means the byte immediately
/// before the (unchanged) position was not whitespace to begin with,
/// and consuming one or more means it definitely was.
///
/// Returns `(OK, 0)` for the fast path (nothing to consume - the
/// original's own loop, having never executed, leaves `*arg`
/// unmodified).
#[must_use]
pub fn handle_subscript(
    arg: &[u8],
    rettv: &TypvalT,
    evalarg: Option<&EvalargT>,
    preceded_by_whitespace: bool,
) -> (i32, usize) {
    use crate::eval::typval::tv_is_func;

    let evaluate = evalarg.is_some_and(|e| e.eval_flags & EVAL_EVALUATE != 0);

    let starts_index_dot_or_call = arg.first() == Some(&b'[')
        || (arg.first() == Some(&b'.') && matches!(rettv.value, TypvalValue::Dict(_)))
        || (arg.first() == Some(&b'(') && (!evaluate || tv_is_func(rettv)));
    let starts_method_call = arg.first() == Some(&b'-') && arg.get(1) == Some(&b'>');

    if (!preceded_by_whitespace && starts_index_dot_or_call) || starts_method_call {
        unimplemented!(
            "handle_subscript: subscript/index (eval_index), function-call \
             (call_func_rettv), and method-call (eval_method/eval_lambda) handling \
             not yet translated"
        );
    }

    (crate::vim_defs::OK, 0)
}

/// `eval.c`'s own file-static `namespace_char` - the single-character
/// scope prefixes that may legitimately precede a `:` within a plain
/// identifier (e.g. `s:` in `s:var`) without ending the name scan
/// there (`namespace_char`).
const NAMESPACE_CHAR: &[u8] = b"abglstvw";

/// `eval.h`'s `find_name_end`/[`find_name_end`] flag: include `[`/`.`
/// in the name scan (used only by `get_lval`'s lvalue-resolution code,
/// not yet translated - see [`find_name_end`]'s own doc comment for
/// why it's not modeled here) (`FNE_INCL_BR`).
#[allow(dead_code)]
const FNE_INCL_BR: i32 = 1;
/// `eval.h`'s `find_name_end` flag: check that the name starts with a
/// valid character (`FNE_CHECK_START`).
const FNE_CHECK_START: i32 = 2;

/// Get the length of the name of a variable or function, handling (at
/// most) a single scope-prefix colon (e.g. `"s:"` in `"s:var"`, but
/// NOT `"n:"` in a slice `[n:]`) (`get_id_len`).
///
/// Returns `(name_len, consumed)`: `name_len` is the length of the
/// name itself; `consumed` is `name_len` plus any trailing whitespace
/// (matching the original's own internal `*arg = skipwhite(p)` before
/// returning) - `(0, 0)` if no valid name is found at all.
#[must_use]
pub fn get_id_len(arg: &[u8]) -> (usize, usize) {
    let mut p = 0;
    while arg.get(p).is_some_and(|&c| eval_isnamec(i32::from(c))) {
        if arg[p] == b':' {
            let len = p;
            if len > 1 || (len == 1 && crate::strings::vim_strchr(NAMESPACE_CHAR, i32::from(arg[0])).is_none())
            {
                break;
            }
        }
        p += 1;
    }
    if p == 0 {
        return (0, 0);
    }
    let ws = skipwhite(&arg[p..]);
    (p, p + ws)
}

/// Find the end of a variable or function name, taking care of magic
/// braces (`find_name_end`).
///
/// Scans at the BYTE level rather than the original's own multi-byte-
/// character-aware `MB_PTR_ADV` walk: every character class tested
/// here (`eval_isnamec`, `'`, `"`, `:`, `[`, `]`, `{`, `}`) is a plain
/// ASCII byte, and valid UTF-8 continuation/lead bytes are always
/// `>= 0x80`, so a raw byte can never appear as part of a multi-byte
/// sequence - a byte-level scan finds the exact same boundary the
/// original's character-aware walk would, for any well-formed UTF-8
/// input (same reasoning as `eval_lit_string`'s own doc comment).
///
/// `FNE_INCL_BR` (letting `[`/`.` continue the name scan, used only
/// by `get_lval`'s lvalue-resolution code) is NOT modeled - `flags` is
/// only ever checked against `FNE_CHECK_START` here, since nothing
/// in this crate can pass `FNE_INCL_BR` yet ([`get_name_len`], the
/// only real caller, never does).
///
/// Returns `(end, magic_braces)`: `end` is the byte offset just past
/// the name (`0` if there is no valid name at all - matching the
/// original's own "return arg unchanged"); `magic_braces`, if
/// `Some((expr_start, expr_end))`, means a `{...}` span was found
/// within the name (byte offsets of its own `{`/`}`, `expr_end == 0`
/// if the closing `}` was never found) - expanding it needs
/// `make_expanded_name`, not yet translated (see [`get_name_len`]'s
/// own doc comment for how it handles this).
#[must_use]
pub fn find_name_end(arg: &[u8], flags: i32) -> (usize, Option<(usize, usize)>) {
    if (flags & FNE_CHECK_START) != 0
        && !eval_isnamec1(i32::from(arg.first().copied().unwrap_or(0)))
        && arg.first() != Some(&b'{')
    {
        return (0, None);
    }

    let mut mb_nest = 0i32;
    let mut br_nest = 0i32;
    let mut expr_start = None;
    let mut expr_end = None;

    let mut p = 0;
    while p < arg.len() {
        let c = arg[p];
        if !(eval_isnamec(i32::from(c)) || c == b'{' || mb_nest != 0 || br_nest != 0) {
            break;
        }

        let mut unterminated = false;
        if c == b'\'' {
            // skip over 'string' to avoid counting [ and ] inside it.
            p += 1;
            while p < arg.len() && arg[p] != b'\'' {
                p += 1;
            }
            unterminated = p >= arg.len();
        } else if c == b'"' {
            // skip over "str\"ing" to avoid counting [ and ] inside it.
            p += 1;
            while p < arg.len() && arg[p] != b'"' {
                if arg[p] == b'\\' && p + 1 < arg.len() {
                    p += 1;
                }
                p += 1;
            }
            unterminated = p >= arg.len();
        } else if br_nest == 0 && mb_nest == 0 && c == b':' {
            // "s:" is start of "s:var", but "n:" is not and can be
            // used in slice "[n:]". Also "xx:" is not a namespace.
            // But {ns}: is.
            let len = p;
            if (len > 1 && arg[len - 1] != b'}')
                || (len == 1 && crate::strings::vim_strchr(NAMESPACE_CHAR, i32::from(arg[0])).is_none())
            {
                break;
            }
        }
        if unterminated {
            break;
        }

        if mb_nest == 0 {
            if arg[p] == b'[' {
                br_nest += 1;
            } else if arg[p] == b']' {
                br_nest -= 1;
            }
        }
        if br_nest == 0 {
            if arg[p] == b'{' {
                mb_nest += 1;
                if expr_start.is_none() {
                    expr_start = Some(p);
                }
            } else if arg[p] == b'}' {
                mb_nest -= 1;
                if mb_nest == 0 && expr_end.is_none() {
                    expr_end = Some(p);
                }
            }
        }

        p += 1;
    }

    (p, expr_start.map(|s| (s, expr_end.unwrap_or(0))))
}

/// Get the length of the name of a variable or function
/// (`get_name_len`).
///
/// Only the name itself is recognized - does not handle `.key` or
/// `[idx]` (that's [`handle_subscript`]'s own job, afterward).
///
/// Magic-braces name expansion (`foo{expr}bar`) is detected (via
/// [`find_name_end`]) but only actually EXPANDED when `evaluate` is
/// `false` (where the original itself doesn't need to expand
/// anything - it only needs to know how much input the whole
/// construct occupies syntactically, via `find_name_end`'s own end
/// position). When `evaluate` is `true`, expanding it for real needs
/// `make_expanded_name`, not yet translated - `unimplemented!()`s in
/// that specific case only.
///
/// The original's `K_SPECIAL`/`KS_EXTRA`/`KE_SNR` hard-coded-`<SNR>`-
/// byte-sequence fast path (an internal representation used by
/// already-substituted function names, not something ordinary
/// Vimscript source text contains) is not modeled - unreachable today
/// since nothing in this crate constructs such a byte sequence yet.
///
/// Returns `(name_len, consumed)`: `name_len` is the length of the
/// name itself (`arg[0..name_len]`, e.g. all 5 bytes of `"s:foo"`, but
/// NOT including trailing whitespace); `consumed` is how far to
/// advance the overall parse position (name length plus any trailing
/// whitespace) - `(0, 0)` if no valid name is found at all (`len <= 0`
/// in the original; the original's own `semsg(_(e_invexpr2), *arg)` in
/// that case, gated on `verbose`, is omitted - message display, not
/// tractable).
#[must_use]
pub fn get_name_len(arg: &[u8], evaluate: bool) -> (usize, usize) {
    let script_len = crate::eval::userfunc::eval_fname_script(arg);
    let after_script = &arg[script_len..];

    let flags = if script_len > 0 { 0 } else { FNE_CHECK_START };
    let (end, magic) = find_name_end(after_script, flags);
    if magic.is_some() {
        if !evaluate {
            let ws = skipwhite(&after_script[end..]);
            return (script_len + end, script_len + end + ws);
        }
        unimplemented!(
            "get_name_len: magic-braces name expansion (foo{{expr}}bar) needs \
             make_expanded_name, not yet translated"
        );
    }

    let (id_len, id_consumed) = get_id_len(after_script);
    let name_len = script_len + id_len;
    if name_len == 0 {
        return (0, 0);
    }
    (name_len, script_len + id_consumed)
}

/// Get the key for `#{key: val}` into `tv` (`get_literal_key`).
///
/// Returns `(status, consumed)` - `FAIL`/`0` when there is no valid
/// key (matching the original's own check).
#[must_use]
fn get_literal_key(arg: &[u8], tv: &mut TypvalT) -> (i32, usize) {
    let is_key_byte = |c: u8| crate::macros_defs::ascii_isalnum(i32::from(c)) || c == b'_' || c == b'-';

    if !arg.first().is_some_and(|&c| is_key_byte(c)) {
        return (FAIL, 0);
    }
    let mut p = 0;
    while arg.get(p).is_some_and(|&c| is_key_byte(c)) {
        p += 1;
    }
    tv.value = TypvalValue::String(Some(arg[..p].to_vec()));
    let ws = skipwhite(&arg[p..]);
    (OK, p + ws)
}

/// Allocate a variable for a List and fill it from `arg` (`eval_list`).
///
/// `arg` must point to the `[`.
///
/// # Safety
/// Forwarded from [`eval1`]/`tv_list_append_owned_tv`/`tv_list_free`/
/// `tv_list_set_ret`'s own safety docs.
pub unsafe fn eval_list(arg: &[u8], rettv: &mut TypvalT, mut evalarg: Option<&mut EvalargT>) -> (i32, usize) {
    use crate::eval::typval::{tv_list_alloc, tv_list_append_owned_tv, tv_list_free, tv_list_set_ret};
    use crate::eval::typval_defs::{ListLenSpecials, VarLockStatus};

    let evaluate = evalarg.as_deref().is_some_and(|e| e.eval_flags & EVAL_EVALUATE != 0);
    let l = if evaluate { tv_list_alloc(ListLenSpecials::ShouldKnow as isize) } else { std::ptr::null_mut() };

    let mut pos = 1;
    pos += skipwhite(&arg[pos..]);

    while !matches!(arg.get(pos), Some(b']') | None) {
        let mut tv = TypvalT::default();
        // SAFETY: forwarded from this function's own safety doc.
        let (ret, consumed) = unsafe { eval1(&arg[pos..], &mut tv, evalarg.as_deref_mut()) };
        pos += consumed;
        if ret == FAIL {
            if evaluate {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_list_free(l) };
            }
            return (FAIL, pos);
        }
        if evaluate {
            tv.v_lock = VarLockStatus::Unlocked;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_list_append_owned_tv(l, tv) };
        }

        let had_comma = arg.get(pos) == Some(&b',');
        if had_comma {
            pos += 1;
            pos += skipwhite(&arg[pos..]);
        }
        if arg.get(pos) == Some(&b']') {
            break;
        }
        if !had_comma {
            // semsg(_("E696: Missing comma in List: %s"), *arg)
            // omitted - message display, not tractable; the identical
            // FAIL is kept.
            if evaluate {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_list_free(l) };
            }
            return (FAIL, pos);
        }
    }

    if arg.get(pos) != Some(&b']') {
        // semsg(_(e_list_end), *arg) omitted.
        if evaluate {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_list_free(l) };
        }
        return (FAIL, pos);
    }

    pos += 1;
    pos += skipwhite(&arg[pos..]);
    if evaluate {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_list_set_ret(rettv, l) };
    }
    (OK, pos)
}

/// Allocate a variable for a Dictionary and fill it from `arg`
/// (`eval_dict`).
///
/// `arg` must point to the `{` (or, for `literal == true`, one byte
/// PAST the `#` - i.e. still at the `{` - matching `eval7`'s own call
/// site, which skips the `#` itself before calling this).
///
/// # Deferred
/// The original's own "is this really a curly-name expression
/// `{expr}`, not a dict literal" speculative pre-check (only relevant
/// for `literal == false`, calling `eval1` in throwaway, non-erroring
/// parse-only mode) is NOT modeled: it would need to speculatively
/// PARSE an arbitrary sub-expression that might land on a genuinely
/// not-yet-implemented `eval7` form (e.g. a function call or a
/// double-quoted string), which would panic via `unimplemented!()`
/// rather than gracefully reporting "not a valid expression" the way
/// the original's own real error handling does. A real, if narrow and
/// rare, gap: a bare `{expr}` curly-name variable/function-name
/// construction (itself not supported elsewhere in this crate either)
/// will incorrectly attempt - and normally fail to parse as - a dict
/// literal, instead of being correctly recognized as a curly-name
/// expression.
///
/// # Safety
/// Forwarded from [`eval1`]/`tv_dict_alloc`/`tv_dict_free`/
/// `tv_dict_find`/`tv_dict_item_alloc`/`tv_dict_add`/
/// `tv_dict_item_free`/`tv_dict_set_ret`/`tv_clear_simple`'s own
/// safety docs.
pub unsafe fn eval_dict(
    arg: &[u8],
    rettv: &mut TypvalT,
    mut evalarg: Option<&mut EvalargT>,
    literal: bool,
) -> (i32, usize) {
    use crate::eval::typval::{
        tv_clear_simple, tv_dict_add, tv_dict_alloc, tv_dict_find, tv_dict_free, tv_dict_item_alloc,
        tv_dict_item_free, tv_dict_set_ret, tv_get_string_chk,
    };
    use crate::eval::typval_defs::VarLockStatus;

    let evaluate = evalarg.as_deref().is_some_and(|e| e.eval_flags & EVAL_EVALUATE != 0);
    let d = if evaluate { tv_dict_alloc() } else { std::ptr::null_mut() };

    let mut pos = 1;
    pos += skipwhite(&arg[pos..]);

    while !matches!(arg.get(pos), Some(b'}') | None) {
        let mut tvkey = TypvalT::default();
        let (ret, consumed) = if literal {
            get_literal_key(&arg[pos..], &mut tvkey)
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { eval1(&arg[pos..], &mut tvkey, evalarg.as_deref_mut()) }
        };
        pos += consumed;
        if ret == FAIL {
            if evaluate {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_dict_free(d) };
            }
            return (FAIL, pos);
        }

        if arg.get(pos) != Some(&b':') {
            // semsg(_("E720: Missing colon in Dictionary: %s"), *arg)
            // omitted.
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_clear_simple(&tvkey) };
            if evaluate {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_dict_free(d) };
            }
            return (FAIL, pos);
        }

        let mut key: Option<Vec<u8>> = None;
        if evaluate {
            key = tv_get_string_chk(&tvkey);
            if key.is_none() {
                // "key" is None when tv_get_string_chk() would have
                // reported a type error.
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    tv_clear_simple(&tvkey);
                    tv_dict_free(d);
                }
                return (FAIL, pos);
            }
        }

        pos += 1;
        pos += skipwhite(&arg[pos..]);
        let mut tv = TypvalT::default();
        // SAFETY: forwarded from this function's own safety doc.
        let (ret2, consumed2) = unsafe { eval1(&arg[pos..], &mut tv, evalarg.as_deref_mut()) };
        pos += consumed2;
        if ret2 == FAIL {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_clear_simple(&tvkey) };
            if evaluate {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_dict_free(d) };
            }
            return (FAIL, pos);
        }

        if evaluate {
            let key = key.expect("key is Some when evaluate is true, checked above");
            // SAFETY: forwarded from this function's own safety doc.
            if tv_dict_find(unsafe { d.as_mut() }, &key).is_some() {
                // semsg(_("E721: Duplicate key in Dictionary: \"%s\""),
                // key) omitted.
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    tv_clear_simple(&tvkey);
                    tv_clear_simple(&tv);
                    tv_dict_free(d);
                }
                return (FAIL, pos);
            }
            let item = tv_dict_item_alloc(&key);
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                (*item).di_tv = tv;
                (*item).di_tv.v_lock = VarLockStatus::Unlocked;
                if tv_dict_add(&mut *d, item) == FAIL {
                    tv_dict_item_free(item);
                }
            }
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(&tvkey) };

        let had_comma = arg.get(pos) == Some(&b',');
        if had_comma {
            pos += 1;
            pos += skipwhite(&arg[pos..]);
        }
        if arg.get(pos) == Some(&b'}') {
            break;
        }
        if !had_comma {
            // semsg(_("E722: Missing comma in Dictionary: %s"), *arg)
            // omitted.
            if evaluate {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_dict_free(d) };
            }
            return (FAIL, pos);
        }
    }

    if arg.get(pos) != Some(&b'}') {
        // semsg(_("E723: Missing end of Dictionary '}': %s"), *arg)
        // omitted.
        if evaluate {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_dict_free(d) };
        }
        return (FAIL, pos);
    }

    pos += 1;
    pos += skipwhite(&arg[pos..]);
    if evaluate {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_dict_set_ret(rettv, d) };
    }
    (OK, pos)
}

/// Skip over the name of an option variable: `"&option"`, `"&g:option"`,
/// or `"&l:option"` (`find_option_var_end`).
///
/// `arg` must point to the `&` (or `+`, for `has("+option")` - the
/// leading byte itself is never inspected, only skipped, matching the
/// original's own unconditional `p++`).
///
/// Returns `(name_start, consumed, opt_idx, opt_flags)`: `name_start`
/// is the offset where the bare option name itself begins (i.e. past
/// any `&`/`g:`/`l:` prefix - `arg[name_start..consumed]` is the bare
/// name, matching what the original's own `*arg` points to right
/// after this function returns); both are `0` if no option name was
/// found at all (matching the original's own `NULL` return, which
/// leaves `*arg` UNCHANGED - equivalent to "0 bytes consumed" in this
/// crate's own idiom).
#[must_use]
pub fn find_option_var_end(arg: &[u8]) -> (usize, usize, OptIndex, u32) {
    let mut p = 1;
    let opt_flags = if arg.get(p) == Some(&b'g') && arg.get(p + 1) == Some(&b':') {
        p += 2;
        crate::option_defs::opt_set_flags::OPT_GLOBAL
    } else if arg.get(p) == Some(&b'l') && arg.get(p + 1) == Some(&b':') {
        p += 2;
        crate::option_defs::opt_set_flags::OPT_LOCAL
    } else {
        0
    };

    match crate::option::find_option_end(&arg[p..]) {
        (Some(consumed), opt_idx) => (p, p + consumed, opt_idx, opt_flags),
        (None, _) => (0, 0, OptIndex::Invalid, opt_flags),
    }
}

/// Get an option value (`eval_option`).
///
/// `arg` must point to the `&` or `+` before the option name (`+` is
/// used for `has("+option")` - the `working` distinction only matters
/// for [`crate::option::is_option_hidden`]'s own narrow "hidden AND
/// not evaluating" early-return path below).
///
/// Returns `(status, consumed)`, matching this module's own
/// established idiom - `FAIL`/`0` when `arg` doesn't start with a
/// valid option name at all.
///
/// # Safety
/// Forwarded from [`crate::option::get_option_value`]'s own safety
/// doc.
#[must_use]
pub unsafe fn eval_option(arg: &[u8], rettv: Option<&mut TypvalT>, evaluate: bool) -> (i32, usize) {
    let working = arg.first() == Some(&b'+');

    let (name_start, consumed, opt_idx, opt_flags) = find_option_var_end(arg);
    if consumed == 0 {
        // semsg(_("E112: Option name missing: %s"), *arg) omitted when
        // rettv.is_some() - message display, not tractable; the
        // identical FAIL is kept.
        return (FAIL, 0);
    }

    if !evaluate {
        return (OK, consumed);
    }

    let is_tty_opt = crate::option::is_tty_option(&arg[name_start..consumed]);

    let ret;
    if opt_idx == OptIndex::Invalid && !is_tty_opt {
        // semsg(_("E113: Unknown option: %s"), *arg) omitted when
        // rettv.is_some().
        ret = FAIL;
    } else if let Some(rettv) = rettv {
        // SAFETY: forwarded from this function's own safety doc.
        let value = if is_tty_opt {
            crate::option::get_tty_option(&arg[name_start..consumed])
        } else {
            unsafe { crate::option::get_option_value(opt_idx, opt_flags) }
        };
        debug_assert!(value.value_type() != crate::option_defs::OptValType::Nil);
        *rettv = crate::eval::vars::optval_as_tv(value, true);
        ret = OK;
    } else if working && !is_tty_opt && crate::option::is_option_hidden(opt_idx) {
        ret = FAIL;
    } else {
        ret = OK;
    }

    (ret, consumed)
}

/// Get the length of an environment-variable name (`get_env_len`).
///
/// `arg` must point to the first byte of the candidate name (i.e.
/// past the leading `$`). Returns `0` if `arg` doesn't start with an
/// identifier character at all (matching the original's own "no name
/// found" `p == *arg` check) - the original's own "advance `*arg` past
/// the name" side effect becomes moot in this crate's own idiom (the
/// returned length already IS the amount consumed).
#[must_use]
pub fn get_env_len(arg: &[u8]) -> usize {
    let mut p = 0;
    while p < arg.len() && crate::charset::vim_isidc(i32::from(arg[p])) {
        p += 1;
    }
    p
}

/// Get an environment variable's value: `$VAR` (`eval_env_var`).
///
/// `arg` must point to the `$` itself - the leading byte is skipped
/// unconditionally, matching the original's own `(*arg)++` (never
/// inspected, only skipped, same treatment as
/// [`find_option_var_end`]'s own leading `&`/`+`).
///
/// Unlike [`eval_option`], a missing/empty name only `FAIL`s when
/// `evaluate` is `true` - in parse-only mode (`evaluate == false`) this
/// always succeeds, consuming however many identifier characters
/// follow (possibly zero), matching the original's own `len == 0`
/// check being nested strictly inside its `if (evaluate)` block.
///
/// # Safety
/// Forwarded from [`crate::os::env::vim_getenv`]'s own safety doc
/// (Windows `$HOME` path only).
#[must_use]
pub unsafe fn eval_env_var(arg: &[u8], rettv: &mut TypvalT, evaluate: bool) -> (i32, usize) {
    let name_start = 1;
    let len = get_env_len(&arg[name_start..]);
    let consumed = name_start + len;

    if evaluate {
        if len == 0 {
            // semsg-free FAIL - invalid empty name; message display
            // isn't reachable from here regardless (not tractable).
            return (FAIL, consumed);
        }
        let name = &arg[name_start..name_start + len];

        // First try vim_getenv(), fast for normal environment vars -
        // its own None already covers the original's combined "NULL
        // or empty" check (os_getenv's own established treatment).
        // SAFETY: forwarded from this function's own safety doc.
        let string = match unsafe { crate::os::env::vim_getenv(name) } {
            Some(s) => Some(s),
            None => {
                // Next try expanding things like $VIM and ${HOME} -
                // needs expand_env_save/expand_env_esc (~/, ~user/,
                // `=expr`, Unix-style ${VAR} braces), a substantial
                // separate undertaking beyond vim_getenv's own scope,
                // not yet translated.
                unimplemented!(
                    "eval_env_var: expand_env_save (the $VIM/${{HOME}}-style path-expansion \
                     fallback for a name vim_getenv couldn't resolve) not yet translated"
                );
            }
        };

        rettv.value = TypvalValue::String(string);
        rettv.v_lock = VarLockStatus::Unlocked;
    }

    (OK, consumed)
}

/// Evaluate a function call: `name(args)` (`eval_func`).
///
/// `arg` must point to the `(` itself. `name` is the already-scanned
/// function/variable name ([`get_name_len`]'s own result).
///
/// Only the path reachable from [`eval7`]'s own real call site is
/// modeled: `basetv` (the base of an `expr->method()` call) is always
/// implicitly absent here - only method-call syntax
/// (`handle_subscript`'s own "->name()" case, not yet translated)
/// would ever supply one. `aborting()` (stopping evaluation early on
/// an uncaught exception/interrupt, needing the whole exception-
/// handling subsystem, `ex_eval.c`) is omitted - the identical
/// `OK`/`FAIL` status from `get_func_tv` is kept regardless, matching
/// this crate's established "skip the display/early-stop refinement,
/// keep the underlying state" policy for similar untranslated checks
/// (e.g. `eval0`'s own dropped `did_emsg`/`called_emsg` gating).
///
/// Also omitted: the original's own "if evaluate is false, rettv->v_type
/// was not set by get_func_tv, but handle_subscript() needs it set to
/// parse a further v:lua-Partial chain" fix-up - moot here, since
/// nero's own `handle_subscript` doesn't inspect `rettv`'s type this
/// way (it already `unimplemented!()`s for any real chained-call
/// parsing regardless of what `rettv` holds beforehand).
///
/// # Safety
/// Forwarded from [`crate::eval::vars::check_vars`]/
/// [`crate::eval::userfunc::deref_func_name`]/
/// [`crate::eval::userfunc::get_func_tv`]'s own safety docs.
#[must_use]
pub unsafe fn eval_func(
    arg: &[u8],
    evalarg: Option<&mut EvalargT>,
    name: &[u8],
    rettv: &mut TypvalT,
    evaluate: bool,
) -> (i32, usize) {
    if !evaluate {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::vars::check_vars(name) };
    }

    // If "name" is a variable of type Funcref/Partial, use its
    // contents instead.
    // SAFETY: forwarded from this function's own safety doc.
    let (resolved_name, _found_var) = unsafe { crate::eval::userfunc::deref_func_name(name, !evaluate) };

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::userfunc::get_func_tv(&resolved_name, rettv, arg, evalarg, evaluate) }
}

/// Recursion depth counter for [`eval7`] - the original's own
/// function-local `static int recurse = 0`, translated as a
/// `GlobalCell`, matching `eval/typval.rs`'s `tv_equal`'s established
/// treatment of the same C idiom.
static EVAL7_RECURSE: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// The recursion depth limit [`eval7`] enforces (`recurse == 1000` in
/// the original on non-MSVC builds - this crate has no MSVC-specific
/// smaller-stack concern, so only that branch is modeled).
const EVAL7_MAX_RECURSE: i32 = 1000;

/// Handle sixth level expression: number/blob/single-quoted-string
/// literals and parenthesized sub-expressions, plus leading
/// `!`/`-`/`+` and trailing subscript chaining (`eval7`).
///
/// See this module's own doc comment for exactly which primary-
/// expression forms are (and aren't) modeled, and why.
///
/// `arg` must point to the first non-white of the expression, and
/// `want_string` is `true` after a `.` operator (float vs. string
/// concatenation - only meaningful for the number-literal case, see
/// `eval_number`'s own doc comment).
///
/// # Safety
/// If `rettv`'s (or an intermediate value's) value is
/// `List`/`Dict`/`Blob`/`Partial`/`Func`-typed with a non-null
/// pointer, that pointer must be valid - forwarded from
/// `tv_clear_simple`/`eval1`/`eval7_leader`'s own safety docs.
pub unsafe fn eval7(
    arg: &[u8],
    rettv: &mut TypvalT,
    mut evalarg: Option<&mut EvalargT>,
    want_string: bool,
) -> (i32, usize) {
    use crate::eval::typval::tv_clear_simple;

    let evaluate = evalarg.as_deref().is_some_and(|e| e.eval_flags & EVAL_EVALUATE != 0);

    // Initialise variable so tv_clear can't mistake this for a string
    // and free a string that isn't there.
    rettv.value = TypvalValue::Unknown;

    // Skip '!', '-' and '+' characters. They are handled later.
    let mut pos = 0;
    while matches!(arg.get(pos), Some(&b'!') | Some(&b'-') | Some(&b'+')) {
        pos += 1;
        pos += skipwhite(&arg[pos..]);
    }
    let leader = &arg[0..pos];
    let mut leader_remaining = leader.len();

    // Limit recursion to 1000 levels. At least at 10000 we run out of
    // stack and crash.
    let recurse = unsafe { *EVAL7_RECURSE.get_mut() };
    if recurse == EVAL7_MAX_RECURSE {
        // semsg(_(e_expression_too_recursive_str), *arg) omitted -
        // message display, not tractable; the identical FAIL is kept.
        return (FAIL, pos);
    }
    unsafe { *EVAL7_RECURSE.get_mut() = recurse + 1 };

    let mut ret;

    match arg.get(pos) {
        // Number constant.
        Some(b'0'..=b'9') => {
            let (r, consumed) = eval_number(&arg[pos..], rettv, evaluate, want_string);
            ret = r;
            pos += consumed;

            // Apply prefixed "-" and "+" now. Matters especially when
            // "->" follows.
            if ret == OK && evaluate && leader_remaining > 0 {
                // SAFETY: forwarded from this function's own safety doc.
                ret = unsafe { eval7_leader(rettv, true, leader, &mut leader_remaining) };
            }
        }
        // String constant: "string".
        Some(b'"') => {
            let (r, consumed) = eval_string(&arg[pos..], rettv, evaluate, false);
            ret = r;
            pos += consumed;
        }
        // Literal string constant: 'str''ing'.
        Some(b'\'') => {
            let (r, consumed) = eval_lit_string(&arg[pos..], rettv, evaluate, false);
            ret = r;
            pos += consumed;
        }
        // List: [expr, expr]
        Some(b'[') => {
            let (r, consumed) = unsafe { eval_list(&arg[pos..], rettv, evalarg.as_deref_mut()) };
            ret = r;
            pos += consumed;
        }
        // Literal Dictionary: #{key: val, key: val} - eval_lit_dict's
        // own body is folded directly into this guard + call (its
        // real body is only this exact "arg[1] == '{'" check plus a
        // call to eval_dict(..., literal=true), with no other real
        // caller of its own) - a bare '#' NOT followed immediately by
        // '{' falls through to the final `_` arm below (name/function
        // resolution), matching the original's own NOTDONE-cascade
        // exactly (get_name_len's own FNE_CHECK_START rejects '#' as
        // a name-starter, correctly FAILing rather than panicking).
        Some(b'#') if arg.get(pos + 1) == Some(&b'{') => {
            pos += 1;
            let (r, consumed) = unsafe { eval_dict(&arg[pos..], rettv, evalarg.as_deref_mut(), true) };
            ret = r;
            pos += consumed;
        }
        // Lambda: {arg, arg -> expr}. Dictionary: {'key': val, 'key': val}
        Some(b'{') => {
            if crate::eval::userfunc::is_lambda_start(&arg[pos..]) {
                unimplemented!(
                    "eval7: lambda expressions (get_lambda_tv) not yet translated - needs \
                     lambda/closure compilation, a substantial separate undertaking"
                );
            }
            let (r, consumed) = unsafe { eval_dict(&arg[pos..], rettv, evalarg.as_deref_mut(), false) };
            ret = r;
            pos += consumed;
        }
        // Option value: &name
        Some(b'&') => {
            // SAFETY: forwarded from this function's own safety doc.
            let (r, consumed) = unsafe { eval_option(&arg[pos..], Some(rettv), evaluate) };
            ret = r;
            pos += consumed;
        }
        // Environment variable: $VAR. Interpolated string: $"..."/$'...'.
        Some(b'$') => {
            if matches!(arg.get(pos + 1), Some(&b'"') | Some(&b'\'')) {
                // SAFETY: forwarded from this function's own safety doc.
                let (r, consumed) = unsafe { eval_interp_string(&arg[pos..], rettv, evaluate) };
                ret = r;
                pos += consumed;
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                let (r, consumed) = unsafe { eval_env_var(&arg[pos..], rettv, evaluate) };
                ret = r;
                pos += consumed;
            }
        }
        // Register contents: @r.
        Some(b'@') => {
            pos += 1;
            let regname = arg.get(pos).copied().map_or(0, i32::from);
            if evaluate {
                // SAFETY: forwarded from this function's own safety doc.
                let contents =
                    unsafe { crate::register::get_reg_contents(regname, crate::register_defs::greg_flags::EXPR_SRC) };
                rettv.value = TypvalValue::String(contents);
            }
            if arg.get(pos).is_some() {
                pos += 1;
            }
            ret = OK;
        }
        // Nested expression: (expression).
        Some(b'(') => {
            pos += 1;
            pos += skipwhite(&arg[pos..]);

            let (r, consumed) = unsafe { eval1(&arg[pos..], rettv, evalarg.as_deref_mut()) };
            pos += consumed;
            ret = r;

            if arg.get(pos) == Some(&b')') {
                pos += 1;
            } else if ret == OK {
                // emsg(_("E110: Missing ')'")) omitted - message
                // display, not tractable; the identical FAIL/tv_clear
                // is kept.
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_clear_simple(rettv) };
                ret = FAIL;
            }
        }
        // Must be a variable or function name, or a genuine parse
        // failure (matching get_name_len's own "len <= 0 -> FAIL" for
        // anything that doesn't even start like a name - e.g. trailing
        // garbage or an unbalanced closing delimiter).
        _ => {
            let (name_len, name_consumed) = get_name_len(&arg[pos..], evaluate);
            if name_len == 0 {
                ret = FAIL;
            } else {
                let name = &arg[pos..pos + name_len];
                pos += name_consumed;

                if arg.get(pos) == Some(&b'(') {
                    // "name(..."  recursive!
                    // SAFETY: forwarded from this function's own safety doc.
                    let (r, consumed) = unsafe { eval_func(&arg[pos..], evalarg.as_deref_mut(), name, rettv, evaluate) };
                    ret = r;
                    pos += consumed;
                } else if evaluate {
                    // get value of variable
                    // SAFETY: forwarded from this function's own safety doc.
                    ret = unsafe { crate::eval::vars::eval_variable(name, Some(rettv), true, false) };
                } else {
                    // skip the name. The original's further "if
                    // rettv->v_type == VAR_UNKNOWN && !evaluate &&
                    // strnequal(s, "v:lua.", 6)" v:lua-Partial fallback
                    // is Lua-specific (phase 13) and unreachable here
                    // regardless: `rettv` was just set to `Unknown`
                    // at the top of this function and evaluate is
                    // false in this branch, so the ONLY way it could
                    // matter is if this crate could already construct
                    // a v:lua-bound Partial some other way, which it
                    // cannot yet.
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { crate::eval::vars::check_vars(name) };
                    ret = OK;
                }
            }
        }
    }

    let ws_skip = skipwhite(&arg[pos..]);
    pos += ws_skip;

    // Handle following '[', '(' and '.' for expr[expr], expr.name,
    // expr(expr), expr->name(expr).
    if ret == OK {
        let (r, consumed) = handle_subscript(&arg[pos..], rettv, evalarg.as_deref(), ws_skip > 0);
        ret = r;
        pos += consumed;
    }

    // Apply logical NOT and unary '-', from right to left, ignore '+'.
    if ret == OK && evaluate && leader_remaining > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        ret = unsafe { eval7_leader(rettv, false, leader, &mut leader_remaining) };
    }

    unsafe { *EVAL7_RECURSE.get_mut() -= 1 };
    (ret, pos)
}

/// Handle fifth level expression: `*`/`/`/`%` (`eval6`).
///
/// `arg` must point to the first non-white of the expression;
/// `want_string` is `true` if `.` is string concatenation, otherwise
/// float.
///
/// # Safety
/// Forwarded from [`eval7`]/[`eval_multdiv_number`]'s own safety docs.
pub unsafe fn eval6(
    arg: &[u8],
    rettv: &mut TypvalT,
    mut evalarg: Option<&mut EvalargT>,
    want_string: bool,
) -> (i32, usize) {
    // SAFETY: forwarded from this function's own safety doc.
    let (ret, mut pos) = unsafe { eval7(arg, rettv, evalarg.as_deref_mut(), want_string) };
    if ret == FAIL {
        return (FAIL, pos);
    }

    loop {
        let op = arg.get(pos).copied();
        let mul_div_op = match op {
            Some(b'*') => MulDivOp::Mul,
            Some(b'/') => MulDivOp::Div,
            Some(b'%') => MulDivOp::Mod,
            _ => break,
        };

        let evaluate = evalarg.as_deref().is_some_and(|e| e.eval_flags & EVAL_EVALUATE != 0);

        // Get the second variable.
        pos += 1;
        pos += skipwhite(&arg[pos..]);
        let mut var2 = TypvalT::default();
        // SAFETY: forwarded from this function's own safety doc.
        let (ret2, consumed2) = unsafe { eval7(&arg[pos..], &mut var2, evalarg.as_deref_mut(), false) };
        pos += consumed2;
        if ret2 == FAIL {
            return (FAIL, pos);
        }

        if evaluate {
            // SAFETY: forwarded from this function's own safety doc.
            if !unsafe { eval_multdiv_number(rettv, &var2, mul_div_op) } {
                return (FAIL, pos);
            }
        }
    }

    (OK, pos)
}

/// Handle fourth level expression: `+`/`-`/`.`/`..` (`eval5`).
///
/// `arg` must point to the first non-white of the expression.
///
/// # Safety
/// Forwarded from [`eval6`]/[`eval_concat_str`]/[`eval_addblob`]/
/// [`eval_addlist`]/[`eval_addsub_number`]/`tv_clear_simple`'s own
/// safety docs.
pub unsafe fn eval5(arg: &[u8], rettv: &mut TypvalT, mut evalarg: Option<&mut EvalargT>) -> (i32, usize) {
    use crate::eval::typval::{tv_check_num, tv_check_str, tv_clear_simple};

    // SAFETY: forwarded from this function's own safety doc.
    let (ret, mut pos) = unsafe { eval6(arg, rettv, evalarg.as_deref_mut(), false) };
    if ret == FAIL {
        return (FAIL, pos);
    }

    // Repeat computing, until no '+', '-' or '.' is following.
    loop {
        let op = arg.get(pos).copied();
        let concat = op == Some(b'.');
        if op != Some(b'+') && op != Some(b'-') && !concat {
            break;
        }

        let evaluate = evalarg.as_deref().is_some_and(|e| e.eval_flags & EVAL_EVALUATE != 0);
        if (op != Some(b'+') || !matches!(rettv.value, TypvalValue::List(_) | TypvalValue::Blob(_)))
            && (concat || !matches!(rettv.value, TypvalValue::Float(_)))
            && evaluate
            && ((concat && !tv_check_str(rettv)) || (!concat && !tv_check_num(rettv)))
        {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_clear_simple(rettv) };
            return (FAIL, pos);
        }

        // Get the second variable.
        if concat && arg.get(pos + 1) == Some(&b'.') {
            pos += 1;
        }
        pos += 1;
        pos += skipwhite(&arg[pos..]);
        let mut var2 = TypvalT::default();
        // SAFETY: forwarded from this function's own safety doc.
        let (ret2, consumed2) = unsafe { eval6(&arg[pos..], &mut var2, evalarg.as_deref_mut(), concat) };
        pos += consumed2;
        if ret2 == FAIL {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_clear_simple(rettv) };
            return (FAIL, pos);
        }

        if evaluate {
            if concat {
                // SAFETY: forwarded from this function's own safety doc.
                if !unsafe { eval_concat_str(rettv, &var2) } {
                    return (FAIL, pos);
                }
            } else if op == Some(b'+')
                && matches!(rettv.value, TypvalValue::Blob(_))
                && matches!(var2.value, TypvalValue::Blob(_))
            {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { eval_addblob(rettv, &var2) };
            } else if op == Some(b'+')
                && matches!(rettv.value, TypvalValue::List(_))
                && matches!(var2.value, TypvalValue::List(_))
            {
                // SAFETY: forwarded from this function's own safety doc.
                if !unsafe { eval_addlist(rettv, &var2) } {
                    return (FAIL, pos);
                }
            } else {
                let addsub_op = if op == Some(b'+') { AddSubOp::Add } else { AddSubOp::Sub };
                // SAFETY: forwarded from this function's own safety doc.
                if !unsafe { eval_addsub_number(rettv, &var2, addsub_op) } {
                    return (FAIL, pos);
                }
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_clear_simple(&var2) };
        }
    }

    (OK, pos)
}

/// Handle third level expression: `==`/`=~`/`!=`/`!~`/`>`/`>=`/`<`/
/// `<=`/`is`/`isnot` (`eval4`).
///
/// `arg` must point to the first non-white of the expression.
///
/// # Safety
/// Forwarded from [`eval5`]/[`typval_compare`]/`tv_clear_simple`'s own
/// safety docs.
pub unsafe fn eval4(arg: &[u8], rettv: &mut TypvalT, mut evalarg: Option<&mut EvalargT>) -> (i32, usize) {
    use crate::eval::typval::tv_clear_simple;

    // SAFETY: forwarded from this function's own safety doc.
    let (ret, mut pos) = unsafe { eval5(arg, rettv, evalarg.as_deref_mut()) };
    if ret == FAIL {
        return (FAIL, pos);
    }

    let mut typ = ExprType::Unknown;
    let mut len = 2usize;

    match (arg.get(pos), arg.get(pos + 1)) {
        (Some(&b'='), Some(&b'=')) => typ = ExprType::Equal,
        (Some(&b'='), Some(&b'~')) => typ = ExprType::Match,
        (Some(&b'!'), Some(&b'=')) => typ = ExprType::Nequal,
        (Some(&b'!'), Some(&b'~')) => typ = ExprType::Nomatch,
        (Some(&b'>'), Some(&b'=')) => typ = ExprType::Gequal,
        (Some(&b'>'), _) => {
            typ = ExprType::Greater;
            len = 1;
        }
        (Some(&b'<'), Some(&b'=')) => typ = ExprType::Sequal,
        (Some(&b'<'), _) => {
            typ = ExprType::Smaller;
            len = 1;
        }
        (Some(&b'i'), Some(&b's')) => {
            let mut l = 2;
            if arg.get(pos + 2) == Some(&b'n') && arg.get(pos + 3) == Some(&b'o') && arg.get(pos + 4) == Some(&b't')
            {
                l = 5;
            }
            let next_is_alnum_or_underscore =
                arg.get(pos + l).is_some_and(|&c| c.is_ascii_alphanumeric() || c == b'_');
            if !next_is_alnum_or_underscore {
                typ = if l == 2 { ExprType::Is } else { ExprType::Isnot };
                len = l;
            }
        }
        _ => {}
    }

    // If there is a comparative operator, use it.
    if typ != ExprType::Unknown {
        let ic = if arg.get(pos + len) == Some(&b'?') {
            // extra question mark appended: ignore case
            len += 1;
            true
        } else if arg.get(pos + len) == Some(&b'#') {
            // extra '#' appended: match case
            len += 1;
            false
        } else {
            // nothing appended: use 'ignorecase'
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ic != 0
        };

        // Get the second variable.
        pos += len;
        pos += skipwhite(&arg[pos..]);
        let mut var2 = TypvalT::default();
        // SAFETY: forwarded from this function's own safety doc.
        let (ret2, consumed2) = unsafe { eval5(&arg[pos..], &mut var2, evalarg.as_deref_mut()) };
        pos += consumed2;
        if ret2 == FAIL {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_clear_simple(rettv) };
            return (FAIL, pos);
        }
        if evalarg.as_deref().is_some_and(|e| e.eval_flags & EVAL_EVALUATE != 0) {
            // SAFETY: forwarded from this function's own safety doc.
            let ok = unsafe { typval_compare(rettv, &var2, typ, ic) };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_clear_simple(&var2) };
            return (if ok { OK } else { FAIL }, pos);
        }
    }

    (OK, pos)
}

/// Handle second level expression: `expr3 && expr3 && expr3` (logical
/// AND) (`eval3`).
///
/// `arg` must point to the first non-white of the expression.
///
/// # Safety
/// Forwarded from [`eval4`]/`tv_get_number_chk`/`tv_clear_simple`'s
/// own safety docs.
pub unsafe fn eval3(arg: &[u8], rettv: &mut TypvalT, mut evalarg: Option<&mut EvalargT>) -> (i32, usize) {
    use crate::eval::typval::{tv_clear_simple, tv_get_number_chk};

    // SAFETY: forwarded from this function's own safety doc.
    let (ret, mut pos) = unsafe { eval4(arg, rettv, evalarg.as_deref_mut()) };
    if ret == FAIL {
        return (FAIL, pos);
    }

    // Handle the "&&" operator.
    if arg.get(pos) == Some(&b'&') && arg.get(pos + 1) == Some(&b'&') {
        let mut local_evalarg = EvalargT::default();
        let evalarg_used: &mut EvalargT = match evalarg.as_deref_mut() {
            Some(e) => e,
            None => &mut local_evalarg,
        };
        let orig_flags = evalarg_used.eval_flags;
        let evaluate = orig_flags & EVAL_EVALUATE != 0;

        let mut result = true;
        if evaluate {
            let mut error = false;
            if tv_get_number_chk(rettv, Some(&mut error)) == 0 {
                result = false;
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_clear_simple(rettv) };
            if error {
                return (FAIL, pos);
            }
        }

        // Repeat until there is no following "&&".
        while arg.get(pos) == Some(&b'&') && arg.get(pos + 1) == Some(&b'&') {
            // Get the second variable.
            pos += 2;
            pos += skipwhite(&arg[pos..]);
            evalarg_used.eval_flags = if result { orig_flags } else { orig_flags & !EVAL_EVALUATE };
            let mut var2 = TypvalT::default();
            // SAFETY: forwarded from this function's own safety doc.
            let (ret2, consumed2) = unsafe { eval4(&arg[pos..], &mut var2, Some(&mut *evalarg_used)) };
            pos += consumed2;
            if ret2 == FAIL {
                return (FAIL, pos);
            }

            // Compute the result.
            if evaluate && result {
                let mut error = false;
                if tv_get_number_chk(&var2, Some(&mut error)) == 0 {
                    result = false;
                }
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_clear_simple(&var2) };
                if error {
                    return (FAIL, pos);
                }
            }
            if evaluate {
                rettv.value = TypvalValue::Number(VarnumberT::from(result));
            }
        }

        if let Some(e) = evalarg {
            e.eval_flags = orig_flags;
        }
        // else: local_evalarg simply drops - eval_tofree is always
        // None here, matching clear_evalarg's own no-op behavior for
        // that case.
    }

    (OK, pos)
}

/// Handle first level expression: `expr2 || expr2 || expr2` (logical
/// OR) (`eval2`).
///
/// `arg` must point to the first non-white of the expression.
///
/// # Safety
/// Forwarded from [`eval3`]/`tv_get_number_chk`/`tv_clear_simple`'s
/// own safety docs.
pub unsafe fn eval2(arg: &[u8], rettv: &mut TypvalT, mut evalarg: Option<&mut EvalargT>) -> (i32, usize) {
    use crate::eval::typval::{tv_clear_simple, tv_get_number_chk};

    // SAFETY: forwarded from this function's own safety doc.
    let (ret, mut pos) = unsafe { eval3(arg, rettv, evalarg.as_deref_mut()) };
    if ret == FAIL {
        return (FAIL, pos);
    }

    // Handle the "||" operator.
    if arg.get(pos) == Some(&b'|') && arg.get(pos + 1) == Some(&b'|') {
        let mut local_evalarg = EvalargT::default();
        let evalarg_used: &mut EvalargT = match evalarg.as_deref_mut() {
            Some(e) => e,
            None => &mut local_evalarg,
        };
        let orig_flags = evalarg_used.eval_flags;
        let evaluate = orig_flags & EVAL_EVALUATE != 0;

        let mut result = false;
        if evaluate {
            let mut error = false;
            if tv_get_number_chk(rettv, Some(&mut error)) != 0 {
                result = true;
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_clear_simple(rettv) };
            if error {
                return (FAIL, pos);
            }
        }

        // Repeat until there is no following "||".
        while arg.get(pos) == Some(&b'|') && arg.get(pos + 1) == Some(&b'|') {
            // Get the second variable.
            pos += 2;
            pos += skipwhite(&arg[pos..]);
            evalarg_used.eval_flags = if !result { orig_flags } else { orig_flags & !EVAL_EVALUATE };
            let mut var2 = TypvalT::default();
            // SAFETY: forwarded from this function's own safety doc.
            let (ret2, consumed2) = unsafe { eval3(&arg[pos..], &mut var2, Some(&mut *evalarg_used)) };
            pos += consumed2;
            if ret2 == FAIL {
                return (FAIL, pos);
            }

            // Compute the result.
            if evaluate && !result {
                let mut error = false;
                if tv_get_number_chk(&var2, Some(&mut error)) != 0 {
                    result = true;
                }
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_clear_simple(&var2) };
                if error {
                    return (FAIL, pos);
                }
            }
            if evaluate {
                rettv.value = TypvalValue::Number(VarnumberT::from(result));
            }
        }

        if let Some(e) = evalarg {
            e.eval_flags = orig_flags;
        }
    }

    (OK, pos)
}

/// Handle top level expression: `expr2 ? expr1 : expr1` /
/// `expr2 ?? expr1` (`eval1`).
///
/// `arg` must point to the first non-white of the expression.
///
/// # Safety
/// Forwarded from [`eval2`]/`tv2bool`/`tv_get_number_chk`/
/// `tv_clear_simple`'s own safety docs.
pub unsafe fn eval1(arg: &[u8], rettv: &mut TypvalT, mut evalarg: Option<&mut EvalargT>) -> (i32, usize) {
    use crate::eval::typval::{tv2bool, tv_clear_simple, tv_get_number_chk};

    *rettv = TypvalT::default();

    // Get the first variable.
    // SAFETY: forwarded from this function's own safety doc.
    let (ret, mut pos) = unsafe { eval2(arg, rettv, evalarg.as_deref_mut()) };
    if ret == FAIL {
        return (FAIL, pos);
    }

    if arg.get(pos) == Some(&b'?') {
        let op_falsy = arg.get(pos + 1) == Some(&b'?');

        let mut local_evalarg = EvalargT::default();
        let evalarg_used: &mut EvalargT = match evalarg.as_deref_mut() {
            Some(e) => e,
            None => &mut local_evalarg,
        };
        let orig_flags = evalarg_used.eval_flags;
        let evaluate = orig_flags & EVAL_EVALUATE != 0;

        let mut result = false;
        if evaluate {
            let mut error = false;
            if op_falsy {
                // SAFETY: forwarded from this function's own safety doc.
                result = unsafe { tv2bool(rettv) };
            } else if tv_get_number_chk(rettv, Some(&mut error)) != 0 {
                result = true;
            }
            if error || !op_falsy || !result {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { tv_clear_simple(rettv) };
            }
            if error {
                return (FAIL, pos);
            }
        }

        // Get the second variable. Recursive!
        if op_falsy {
            pos += 1;
        }
        pos += 1;
        pos += skipwhite(&arg[pos..]);
        evalarg_used.eval_flags =
            if if op_falsy { !result } else { result } { orig_flags } else { orig_flags & !EVAL_EVALUATE };
        let mut var2 = TypvalT::default();
        // SAFETY: forwarded from this function's own safety doc.
        let (ret2, consumed2) = unsafe { eval1(&arg[pos..], &mut var2, Some(&mut *evalarg_used)) };
        pos += consumed2;
        if ret2 == FAIL {
            evalarg_used.eval_flags = orig_flags;
            return (FAIL, pos);
        }
        if !op_falsy || !result {
            *rettv = var2;
        }

        if !op_falsy {
            // Check for the ":".
            if arg.get(pos) != Some(&b':') {
                // emsg(_("E109: Missing ':' after '?'")) omitted -
                // message display, not tractable; the identical
                // FAIL/tv_clear is kept.
                if evaluate && result {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { tv_clear_simple(rettv) };
                }
                evalarg_used.eval_flags = orig_flags;
                return (FAIL, pos);
            }

            // Get the third variable. Recursive!
            pos += 1;
            pos += skipwhite(&arg[pos..]);
            evalarg_used.eval_flags = if !result { orig_flags } else { orig_flags & !EVAL_EVALUATE };
            let mut var3 = TypvalT::default();
            // SAFETY: forwarded from this function's own safety doc.
            let (ret3, consumed3) = unsafe { eval1(&arg[pos..], &mut var3, Some(&mut *evalarg_used)) };
            pos += consumed3;
            if ret3 == FAIL {
                if evaluate && result {
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { tv_clear_simple(rettv) };
                }
                evalarg_used.eval_flags = orig_flags;
                return (FAIL, pos);
            }
            if evaluate && !result {
                *rettv = var3;
            }
        }

        if let Some(e) = evalarg {
            e.eval_flags = orig_flags;
        } else {
            clear_evalarg(Some(&mut local_evalarg), None);
        }
    }

    (OK, pos)
}

/// Handle zero level expression. Calls [`eval1`] and handles the
/// trailing-argument/next-command bookkeeping (`eval0`).
///
/// `arg` need not be pre-skipped of leading whitespace (`eval0` does
/// this itself, matching the original).
///
/// # Safety
/// Forwarded from [`eval1`]'s own safety doc.
pub unsafe fn eval0(
    arg: &[u8],
    rettv: &mut TypvalT,
    mut eap: Option<&mut crate::ex_cmds_defs::ExargT>,
    evalarg: Option<&mut EvalargT>,
) -> i32 {
    use crate::eval::typval::tv_clear_simple;

    let start = skipwhite(arg);
    // SAFETY: forwarded from this function's own safety doc.
    let (ret, consumed) = unsafe { eval1(&arg[start..], rettv, evalarg) };
    let pos = start + consumed;

    let end_error = ret != FAIL && !crate::ex_docmd::ends_excmd(arg.get(pos).copied().unwrap_or(0));

    if ret == FAIL || end_error {
        if ret != FAIL {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { tv_clear_simple(rettv) };
        }
        // Report the invalid expression unless the expression
        // evaluation has been cancelled due to an aborting error, an
        // interrupt, or an exception, or we already gave a more
        // specific error - the original's own `did_emsg`/
        // `called_emsg`/`aborting()`-gated `semsg(...)` calls
        // themselves are omitted entirely (message display, not
        // tractable, and the gating condition has no other observable
        // effect once the display itself is gone), while the
        // identical FAIL/`eap.nextcmd` behavior below is kept exactly.
        if let Some(eap) = eap.as_deref_mut() {
            // Some of the expression may not have been consumed. Only
            // execute a next command if it cannot be a "||" operator.
            // The next command may be "catch".
            if let Some(next) = crate::ex_docmd::check_nextcmd(&arg[pos..]) {
                if arg.get(pos + next) != Some(&b'|') {
                    eap.nextcmd = Some(arg[pos + next..].to_vec());
                }
            }
        }
        return FAIL;
    }

    if let Some(eap) = eap {
        eap.nextcmd = crate::ex_docmd::check_nextcmd(&arg[pos..]).map(|next| arg[pos + next..].to_vec());
    }

    ret
}

/// Skip over an expression at `arg`, without evaluating it
/// (`skip_expr`).
///
/// Temporarily clears `EVAL_EVALUATE` in `evalarg` (if present),
/// restoring the original flags afterward - matches the original's own
/// save/restore of `evalarg->eval_flags`. This is actually a no-op in
/// EVERY real call within this crate today: the original's own inner
/// `eval1(pp, &rettv, NULL)` call always passes a bare `NULL` for its
/// OWN `evalarg`, never the (possibly just-cleared) one this function
/// received - so the clear/restore dance here never affects the
/// nested `eval1` call at all (translated faithfully anyway, exactly
/// matching the original's own apparently-vestigial structure, in case
/// a future change ever threads `evalarg` through to the inner call).
///
/// Returns `(status, bytes_consumed)`, matching this module's own
/// established idiom.
///
/// # Safety
/// Forwarded from [`eval1`]'s own safety doc.
#[must_use]
pub unsafe fn skip_expr(arg: &[u8], mut evalarg: Option<&mut EvalargT>) -> (i32, usize) {
    let save_flags = evalarg.as_deref().map_or(0, |e| e.eval_flags);
    if let Some(e) = &mut evalarg {
        e.eval_flags &= !EVAL_EVALUATE;
    }

    let start = skipwhite(arg);
    let mut rettv = TypvalT::default();
    // SAFETY: forwarded from this function's own safety doc.
    let (res, consumed) = unsafe { eval1(&arg[start..], &mut rettv, None) };

    if let Some(e) = &mut evalarg {
        e.eval_flags = save_flags;
    }

    (res, start + consumed)
}

/// Convert `tv` to a string (`typval2string`).
///
/// # Safety
/// If `tv`'s value is `List`/`Dict`/`Blob`/`Partial`/`Func`-typed with
/// a non-null pointer, that pointer must be valid (forwarded from
/// [`crate::eval::typval::tv_get_string`]'s own implicit requirement,
/// same as every other function in this crate touching those types).
///
/// # Deferred
/// `join_list = true` with a real `List` value needs `tv_list_join`
/// (join a list's items into newline-separated lines) - not yet
/// translated, `unimplemented!()`s only when actually reached. A
/// `List`/`Dict` value with `join_list = false` needs `encode_tv2string`
/// (`eval/encode.c`'s ~970-line JSON-like stringification engine, a
/// substantial separate undertaking) - `unimplemented!()`s there too.
/// Every other value type uses [`crate::eval::typval::tv_get_string`],
/// already real.
unsafe fn typval2string(tv: &TypvalT, join_list: bool) -> Vec<u8> {
    if join_list && matches!(tv.value, TypvalValue::List(_)) {
        unimplemented!(
            "typval2string: join_list=true with a List value needs tv_list_join, not yet translated"
        );
    }
    if matches!(tv.value, TypvalValue::List(_) | TypvalValue::Dict(_)) {
        unimplemented!(
            "typval2string: List/Dict stringification needs encode_tv2string (eval/encode.c), \
             a substantial separate undertaking, not yet translated"
        );
    }
    crate::eval::typval::tv_get_string(tv)
}

/// Top level evaluation function, returning a string, with an
/// optional `exarg_T` for multi-line-`:source` context
/// (`eval_to_string_eap`).
///
/// Only `eap = None`/`use_simple_function = false` are modeled - this
/// crate's only real caller ([`crate::eval::vars::eval_one_expr_in_str`])
/// always calls it that way; `use_simple_function = true` would need
/// `eval0_simple_funccal` (a fast-path shortcut for a bare function
/// call), not yet translated, `unimplemented!()`s if ever requested.
///
/// Returns `None` on evaluation failure, matching the original's own
/// `NULL` return (the message-display difference for an E-numbered
/// parse error is dropped, per this crate's established policy).
///
/// # Safety
/// Forwarded from [`eval0`]'s own safety doc.
#[must_use]
pub unsafe fn eval_to_string_eap(
    arg: &[u8],
    join_list: bool,
    eap: Option<&mut crate::ex_cmds_defs::ExargT>,
    use_simple_function: bool,
) -> Option<Vec<u8>> {
    if use_simple_function {
        unimplemented!(
            "eval_to_string_eap: use_simple_function=true needs eval0_simple_funccal, not yet \
             translated"
        );
    }

    let mut tv = TypvalT::default();
    let mut evalarg = EvalargT { eval_flags: EVAL_EVALUATE, ..Default::default() };
    // SAFETY: forwarded from this function's own safety doc.
    let ret = unsafe { eval0(arg, &mut tv, eap, Some(&mut evalarg)) };
    if ret == FAIL {
        return None;
    }
    // SAFETY: forwarded from this function's own safety doc (tv was
    // just populated by eval0 above).
    let retval = unsafe { typval2string(&tv, join_list) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::typval::tv_clear_simple(&tv) };
    Some(retval)
}

/// [`eval_to_string_eap`] with `eap = None` (`eval_to_string`).
///
/// # Safety
/// Forwarded from [`eval_to_string_eap`]'s own safety doc.
#[must_use]
pub unsafe fn eval_to_string(arg: &[u8], join_list: bool, use_simple_function: bool) -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { eval_to_string_eap(arg, join_list, None, use_simple_function) }
}

/// Evaluate a single or double quoted string possibly containing
/// expressions: `$"..."`/`$'...'` (`eval_interp_string`).
///
/// `arg` must point to the `$` itself - the leading byte is skipped
/// unconditionally, matching the original's own `(*arg)++` (never
/// inspected, only skipped, same treatment as [`eval_env_var`]'s own
/// leading `$`). The byte right after that is the quote character
/// itself (`"` or `'`), which is ALSO skipped unconditionally before
/// the loop begins - matching the original's own second `(*arg)++`.
///
/// # Safety
/// Forwarded from [`crate::eval::vars::eval_one_expr_in_str`]'s own
/// safety doc.
#[must_use]
pub unsafe fn eval_interp_string(arg: &[u8], rettv: &mut TypvalT, evaluate: bool) -> (i32, usize) {
    let quote = arg[1];
    let mut pos = 2;
    let mut ga: Vec<u8> = Vec::new();

    let ret = loop {
        let mut tv = TypvalT::default();
        // Get the string up to the matching quote or to a single '{'.
        let (r, consumed) = if quote == b'"' {
            eval_string(&arg[pos..], &mut tv, evaluate, true)
        } else {
            eval_lit_string(&arg[pos..], &mut tv, evaluate, true)
        };
        pos += consumed;
        if r == FAIL {
            break FAIL;
        }
        if evaluate {
            let TypvalValue::String(Some(s)) = &tv.value else {
                unreachable!("eval_string/eval_lit_string always set a String value on OK");
            };
            ga.extend_from_slice(s);
        }

        if arg.get(pos) != Some(&b'{') {
            // found terminating quote.
            pos += 1;
            break OK;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let Some(new_pos) = (unsafe { crate::eval::vars::eval_one_expr_in_str(&arg[pos..], &mut ga, evaluate) })
        else {
            break FAIL;
        };
        pos += new_pos;
    };

    rettv.value = TypvalValue::String((ret != FAIL && evaluate).then_some(ga));
    (OK, pos)
}

/// Convert a byte index within line `lnum` of `buf` to a character
/// index - `-1` on failure (an unloaded buffer, matching the
/// original) (`buf_byteidx_to_charidx`).
///
/// # Safety
/// Forwarded from [`crate::memline::ml_get_buf`]'s own safety doc.
#[must_use]
pub unsafe fn buf_byteidx_to_charidx(
    buf: &mut crate::buffer_defs::BufT,
    lnum: crate::pos_defs::LinenrT,
    byteidx: i32,
) -> i32 {
    if buf.b_ml.ml_mfp.is_null() {
        return -1;
    }
    let lnum = lnum.min(buf.b_ml.ml_line_count);
    // SAFETY: forwarded from this function's own safety doc.
    let s = unsafe { crate::memline::ml_get_buf(buf, lnum) };
    if s.first().copied().unwrap_or(0) == 0 {
        return 0;
    }

    let byteidx = usize::try_from(byteidx.max(0)).unwrap_or(0);
    let mut t = 0usize;
    let mut count = 0i32;
    while s.get(t).copied().unwrap_or(0) != 0 && t <= byteidx {
        // SAFETY: forwarded from this function's own safety doc.
        let adv = usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(&s[t..]) }).unwrap_or(0).max(1);
        t += adv;
        count += 1;
    }

    // In insert mode, when the cursor is at the end of a non-empty
    // line, byteidx points to the NUL character immediately past the
    // end of the string. In this case, add one to the character count.
    if s.get(t).copied().unwrap_or(0) == 0 && byteidx != 0 && t == byteidx {
        count += 1;
    }

    count - 1
}

/// Convert a character index within line `lnum` of `buf` to a byte
/// index (`buf_charidx_to_byteidx`).
///
/// # Safety
/// Forwarded from [`crate::memline::ml_get_buf`]'s own safety doc.
#[must_use]
pub unsafe fn buf_charidx_to_byteidx(
    buf: &mut crate::buffer_defs::BufT,
    lnum: crate::pos_defs::LinenrT,
    charidx: i32,
) -> i32 {
    if buf.b_ml.ml_mfp.is_null() {
        return -1;
    }
    let lnum = lnum.min(buf.b_ml.ml_line_count);
    // SAFETY: forwarded from this function's own safety doc.
    let s = unsafe { crate::memline::ml_get_buf(buf, lnum) };

    // Convert the character offset to a byte offset.
    let mut t = 0usize;
    let mut charidx = charidx;
    while s.get(t).copied().unwrap_or(0) != 0 {
        charidx -= 1;
        if charidx <= 0 {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let adv = usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(&s[t..]) }).unwrap_or(0).max(1);
        t += adv;
    }

    t as i32
}

/// Convert `tv` into a file position for window `wp`
/// (`var2fpos`).
///
/// Supports the `[lnum, col, coladd]` `List` form and the `.`
/// (cursor)/`v` (Visual start)/`$` (last line or column, depending on
/// `dollar_lnum`) special strings. The `'x` mark form needs
/// `mark_get` (`mark.c`, not yet translated) and the `w0`/`w$`
/// (first/last visible line) forms need `update_topline`/
/// `validate_botline_win` (the redraw pipeline, not yet translated) -
/// both `unimplemented!()` if actually reached (neither is reachable
/// from any currently-translated caller).
///
/// # Safety
/// `wp` must point to a valid, live `WinT` whose `w_buffer` is also
/// valid and live. Forwarded from [`crate::memline::ml_get_buf`]'s own
/// safety doc.
#[must_use]
pub unsafe fn var2fpos(
    tv: &TypvalT,
    dollar_lnum: bool,
    charcol: bool,
    wp: *mut crate::buffer_defs::WinT,
) -> Option<crate::pos_defs::PosT> {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &*wp };
    // SAFETY: forwarded from this function's own safety doc.
    let bp = unsafe { &mut *w.w_buffer };

    if let TypvalValue::List(l) = &tv.value {
        if l.is_null() {
            return None;
        }
        let mut error = false;
        // SAFETY: forwarded from this function's own safety doc.
        let lnum = unsafe { crate::eval::typval::tv_list_find_nr(*l, 0, Some(&mut error)) };
        if error || lnum <= 0 || lnum > i64::from(bp.b_ml.ml_line_count) {
            return None;
        }
        let lnum = lnum as crate::pos_defs::LinenrT;

        let mut error2 = false;
        // SAFETY: forwarded from this function's own safety doc.
        let mut col = unsafe { crate::eval::typval::tv_list_find_nr(*l, 1, Some(&mut error2)) };
        if error2 {
            return None;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let line = unsafe { crate::memline::ml_get_buf(bp, lnum) };
        let content = if line.last().copied() == Some(0) { &line[..line.len() - 1] } else { &line[..] };
        let len = if charcol {
            // SAFETY: forwarded from this function's own safety doc.
            i64::from(unsafe { crate::mbyte::mb_charlen(content) })
        } else {
            content.len() as i64
        };

        // We accept "$" for the column number: last column.
        // SAFETY: forwarded from this function's own safety doc.
        let li = unsafe { crate::eval::typval::tv_list_find(*l, 1) };
        if !li.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            if let TypvalValue::String(Some(s)) = &unsafe { &*li }.li_tv.value {
                if s == b"$" {
                    col = len + 1;
                }
            }
        }

        // Accept a position up to the NUL after the line.
        if col == 0 || col > len + 1 {
            return None;
        }
        col -= 1;

        // Get the virtual offset.  Defaults to zero.
        let mut error3 = false;
        // SAFETY: forwarded from this function's own safety doc.
        let coladd = unsafe { crate::eval::typval::tv_list_find_nr(*l, 2, Some(&mut error3)) };
        let coladd = if error3 { 0 } else { coladd as crate::pos_defs::ColnrT };

        return Some(crate::pos_defs::PosT { lnum, col: col as crate::pos_defs::ColnrT, coladd });
    }

    // SAFETY: forwarded from this function's own safety doc.
    let name = crate::eval::typval::tv_get_string_chk(tv)?;

    let mut pos = crate::pos_defs::PosT { lnum: 0, col: 0, coladd: 0 };
    if name.first() == Some(&b'.') {
        // cursor
        pos = w.w_cursor;
    } else if name.as_slice() == b"v" {
        // Visual start
        // SAFETY: forwarded from this function's own safety doc.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        pos = if g.Visual.active && std::ptr::eq(wp, g.curwin) { g.Visual.start } else { w.w_cursor };
    } else if name.first() == Some(&b'\'') {
        unimplemented!(
            "var2fpos: mark ({name:?}) needs mark_get (mark.c), not yet translated",
        );
    }

    if pos.lnum != 0 {
        if charcol {
            // SAFETY: forwarded from this function's own safety doc.
            pos.col = unsafe { buf_byteidx_to_charidx(bp, pos.lnum, pos.col) };
        }
        return Some(pos);
    }

    pos.coladd = 0;

    if name.first() == Some(&b'w') && dollar_lnum {
        unimplemented!(
            "var2fpos: \"w0\"/\"w$\" need update_topline/validate_botline_win (the redraw \
             pipeline), not yet translated"
        );
    } else if name.first() == Some(&b'$') {
        // last column or line
        if dollar_lnum {
            pos.lnum = bp.b_ml.ml_line_count;
            pos.col = 0;
        } else {
            pos.lnum = w.w_cursor.lnum;
            // SAFETY: forwarded from this function's own safety doc.
            let line = unsafe { crate::memline::ml_get_buf(bp, w.w_cursor.lnum) };
            let content = if line.last().copied() == Some(0) { &line[..line.len() - 1] } else { &line[..] };
            pos.col = if charcol {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::mbyte::mb_charlen(content) }
            } else {
                content.len() as crate::pos_defs::ColnrT
            };
        }
        return Some(pos);
    }
    None
}

/// Convert a `List` `tv` argument into a file position, buffer number,
/// and `curswant` (`list2fpos`).
///
/// `arg` must be `[fnum, lnum, col, coladd, curswant]` (`fnum` present
/// only when `fnump` is `Some`; `coladd`/`curswant` optional).
///
/// The `charcol=true` (character-position, used by `setcharpos()`/
/// `setcursorcharpos()`) path is tractable ONLY when `fnump` is
/// `None`: the original resolves the target buffer via
/// `buflist_findnr(fnump == NULL ? curbuf->b_fnum : *fnump)`, and
/// looking up your OWN buffer number always finds `curbuf` itself, so
/// this case substitutes `GLOBALS.curbuf` directly, skipping the
/// not-yet-translated `buflist_findnr` lookup entirely (provably
/// equivalent for this specific sub-case, via
/// [`buf_charidx_to_byteidx`]). When `fnump` is `Some` (an EXPLICIT
/// buffer number, possibly a DIFFERENT buffer than `curbuf`), this
/// same shortcut does not apply - `unimplemented!()`s if reached
/// (`set_position`'s own call for `setpos()`/`setcharpos()` always
/// passes `fnump = Some`, so `setcharpos()` itself is still not
/// registered as a builtin for this reason; `set_cursorpos`'s own
/// List-argument call for `cursor()`/`setcursorcharpos()` always
/// passes `fnump = None`, so THAT call site works for both).
///
/// # Safety
/// Forwarded from [`crate::eval::typval::tv_list_find_nr`]'s own
/// safety doc, plus [`crate::globals::GLOBALS`]'s usual "no
/// overlapping live access" requirement when `fnump` is `Some` and the
/// list's own `fnum` entry is `0` (current buffer), or when
/// `charcol` is `true` and `fnump` is `None`.
pub unsafe fn list2fpos(
    arg: &TypvalT,
    posp: &mut crate::pos_defs::PosT,
    fnump: Option<&mut i32>,
    curswantp: Option<&mut crate::pos_defs::ColnrT>,
    charcol: bool,
) -> i32 {
    let TypvalValue::List(l) = arg.value else { return FAIL };
    if l.is_null() {
        return FAIL;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let list_len = i64::from(unsafe { crate::eval::typval::tv_list_len(l) });
    let fnump_was_some = fnump.is_some();
    let min_len = if fnump_was_some { 3 } else { 2 };
    let max_len = if fnump_was_some { 5 } else { 4 };
    if list_len < min_len || list_len > max_len {
        return FAIL;
    }

    let mut i = 0i32;
    if let Some(fnump) = fnump {
        // SAFETY: forwarded from this function's own safety doc.
        let n = unsafe { crate::eval::typval::tv_list_find_nr(l, i, None) };
        i += 1;
        if n < 0 {
            return FAIL;
        }
        let n = if n == 0 {
            // SAFETY: forwarded from this function's own safety doc.
            i64::from(unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }.handle)
        } else {
            n
        };
        *fnump = n as i32;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let n = unsafe { crate::eval::typval::tv_list_find_nr(l, i, None) };
    i += 1;
    if n < 0 {
        return FAIL;
    }
    posp.lnum = n as crate::pos_defs::LinenrT;

    // SAFETY: forwarded from this function's own safety doc.
    let n = unsafe { crate::eval::typval::tv_list_find_nr(l, i, None) };
    i += 1;
    if n < 0 {
        return FAIL;
    }
    let n = if charcol {
        if fnump_was_some {
            unimplemented!(
                "list2fpos: character-position conversion for an explicit fnum needs \
                 buflist_findnr, not yet translated"
            );
        }
        // fnump == NULL means the original resolves via
        // `buflist_findnr(curbuf->b_fnum)`, which always finds curbuf
        // itself (looking up your own buffer number always succeeds) -
        // substitute GLOBALS.curbuf directly, skipping the not-yet-
        // translated buflist_findnr lookup entirely (provably
        // equivalent for this specific, fnump-absent case).
        // SAFETY: forwarded from this function's own safety doc.
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        // SAFETY: forwarded from this function's own safety doc.
        let buf = unsafe { &mut *globals.curbuf };
        let lnum = if posp.lnum == 0 {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &*globals.curwin }.w_cursor.lnum
        } else {
            posp.lnum
        };
        // SAFETY: forwarded from this function's own safety doc.
        i64::from(unsafe { buf_charidx_to_byteidx(buf, lnum, n as i32) }) + 1
    } else {
        n
    };
    posp.col = n as crate::pos_defs::ColnrT;

    // SAFETY: forwarded from this function's own safety doc.
    let n = unsafe { crate::eval::typval::tv_list_find_nr(l, i, None) };
    posp.coladd = if n < 0 { 0 } else { n as crate::pos_defs::ColnrT };

    if let Some(curswantp) = curswantp {
        // SAFETY: forwarded from this function's own safety doc.
        *curswantp = unsafe { crate::eval::typval::tv_list_find_nr(l, i + 1, None) } as crate::pos_defs::ColnrT;
    }

    OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::typval_defs::VarLockStatus;

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
    fn eval_concat_str_releases_tv2_when_it_is_unstringifiable() {
        use crate::eval::typval::tv_list_alloc;

        let _lock = crate::globals::global_state_test_lock();

        // tv2 is a real, non-null List with refcount 1 - the type-error
        // path must still release it (matching the original's own
        // `tv_clear(tv2)`), not silently leak its reference - this is
        // the exact bug found and fixed while building eval5 (this
        // function's real caller): an earlier draft only released tv1
        // (always a no-op in practice) and skipped tv2 entirely.
        let l = tv_list_alloc(crate::eval::typval_defs::ListLenSpecials::Unknown as isize);
        unsafe { (*l).lv_refcount = 1 };
        let mut tv1 = TypvalT { value: TypvalValue::String(Some(b"foo".to_vec())), ..Default::default() };
        let tv2 = TypvalT { value: TypvalValue::List(l), ..Default::default() };

        let ok = unsafe { eval_concat_str(&mut tv1, &tv2) };
        assert!(!ok);
        // The list was freed at refcount 0 - re-allocate at the same
        // spirit to confirm no crash/leak-sanitizer complaint would
        // have fired (the absence of a use-after-free crash under
        // Miri/ASan IS the check here, matching this crate's own
        // established style for refcount-release tests).
    }

    #[test]
    fn eval_concat_str_releases_tv1s_old_list_when_it_cannot_grow_in_place() {
        use crate::eval::typval::tv_list_alloc;

        let _lock = crate::globals::global_state_test_lock();

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

    #[test]
    fn eval_string_plain_no_escapes() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = eval_string(b"\"hello\"", &mut tv, true, false);
        assert_eq!(ret, OK);
        assert_eq!(consumed, 7);
        assert_eq!(tv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    fn eval_string_common_control_escapes() {
        let mut tv = TypvalT::default();
        let (ret, _) = eval_string(b"\"\\n\\t\\r\\b\\e\\f\"", &mut tv, true, false);
        assert_eq!(ret, OK);
        assert_eq!(
            tv.value,
            TypvalValue::String(Some(vec![
                crate::ascii_defs::NL,
                crate::ascii_defs::TAB,
                crate::ascii_defs::CAR,
                crate::ascii_defs::BS,
                crate::ascii_defs::ESC,
                crate::ascii_defs::FF,
            ]))
        );
    }

    #[test]
    fn eval_string_hex_escape() {
        let mut tv = TypvalT::default();
        let (ret, _) = eval_string(b"\"\\x41\"", &mut tv, true, false);
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"A".to_vec())));
    }

    #[test]
    fn eval_string_hex_escape_single_digit() {
        // "\x1" - only 1 hex digit given, max for \x is 2, so it stops
        // early rather than consuming a 2nd non-hex character.
        let mut tv = TypvalT::default();
        let (ret, _) = eval_string(b"\"\\x1\"", &mut tv, true, false);
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(vec![1])));

        let mut tv2 = TypvalT::default();
        let (ret2, _) = eval_string(b"\"\\x1g\"", &mut tv2, true, false);
        assert_eq!(ret2, OK);
        assert_eq!(tv2.value, TypvalValue::String(Some(vec![1, b'g'])));
    }

    #[test]
    fn eval_string_unicode_escape_4_digit() {
        let mut tv = TypvalT::default();
        let (ret, _) = eval_string(b"\"\\u0041\"", &mut tv, true, false);
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"A".to_vec())));
    }

    #[test]
    fn eval_string_unicode_escape_8_digit() {
        let mut tv = TypvalT::default();
        let (ret, _) = eval_string(b"\"\\U00000041\"", &mut tv, true, false);
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"A".to_vec())));
    }

    #[test]
    fn eval_string_unicode_escape_multibyte_output() {
        let mut tv = TypvalT::default();
        // U+00E9 (é) encodes as 2 UTF-8 bytes.
        let (ret, _) = eval_string(b"\"\\u00e9\"", &mut tv, true, false);
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some("é".as_bytes().to_vec())));
    }

    #[test]
    fn eval_string_octal_escape() {
        let mut tv = TypvalT::default();
        // Octal 101 == decimal 65 == 'A'.
        let (ret, _) = eval_string(b"\"\\101\"", &mut tv, true, false);
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"A".to_vec())));
    }

    #[test]
    fn eval_string_octal_escape_single_digit() {
        let mut tv = TypvalT::default();
        let (ret, _) = eval_string(b"\"\\7\"", &mut tv, true, false);
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(vec![7])));
    }

    #[test]
    fn eval_string_default_escape_copies_literal_char() {
        let mut tv = TypvalT::default();
        let (ret, _) = eval_string(b"\"\\q\"", &mut tv, true, false);
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"q".to_vec())));
    }

    #[test]
    fn eval_string_hex_escape_with_no_digit_falls_back_to_literal_letter() {
        // "\x" with no HEX-digit following (note: 'b' through 'f' ARE
        // valid hex digits, so 'g' is used here to genuinely NOT
        // qualify): the backslash is dropped and the escape letter
        // itself becomes a plain character.
        let mut tv = TypvalT::default();
        let (ret, _) = eval_string(b"\"a\\xg\"", &mut tv, true, false);
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"axg".to_vec())));
    }

    #[test]
    fn eval_string_missing_quote_fails() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = eval_string(b"\"abc", &mut tv, true, false);
        assert_eq!(ret, FAIL);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn eval_string_parse_only_mode_still_computes_length() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = eval_string(b"\"hello\\nworld\"", &mut tv, false, false);
        assert_eq!(ret, OK);
        assert_eq!(consumed, 14);
        assert_eq!(tv.value, TypvalValue::Unknown);
    }

    #[test]
    fn eval_string_special_key_escape_is_unimplemented() {
        let mut tv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            eval_string(b"\"\\<C-W>\"", &mut tv, true, false)
        }));
        assert!(result.is_err(), "expected a panic (find_special_key/trans_special not yet translated)");
    }

    #[test]
    fn eval_string_special_key_escape_panics_even_when_not_evaluating() {
        // The FIRST pass (finding the string's end) also needs
        // find_special_key whenever '\<' appears, even in parse-only
        // mode - matching the original exactly.
        let mut tv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            eval_string(b"\"\\<C-W>\"", &mut tv, false, false)
        }));
        assert!(result.is_err());
    }

    #[test]
    fn eval_lit_string_parses_simple_literal() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_lit_string(b"'hello'", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 7);
        assert!(matches!(tv.value, TypvalValue::String(Some(ref s)) if s == b"hello"));
    }

    #[test]
    fn eval_lit_string_reduces_escaped_quote_pair() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_lit_string(b"'ab''cd'", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 8);
        assert!(matches!(tv.value, TypvalValue::String(Some(ref s)) if s == b"ab'cd"));
    }

    #[test]
    fn eval_lit_string_handles_multiple_escaped_quote_pairs() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_lit_string(b"''''''", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 6);
        assert!(matches!(tv.value, TypvalValue::String(Some(ref s)) if s == b"''"));
    }

    #[test]
    fn eval_lit_string_empty_literal() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_lit_string(b"''", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 2);
        assert!(matches!(tv.value, TypvalValue::String(Some(ref s)) if s.is_empty()));
    }

    #[test]
    fn eval_lit_string_stops_at_first_unescaped_quote_leaving_trailer() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_lit_string(b"'abc' . 'def'", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 5);
        assert!(matches!(tv.value, TypvalValue::String(Some(ref s)) if s == b"abc"));
    }

    #[test]
    fn eval_lit_string_missing_closing_quote_fails() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_lit_string(b"'abc", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::FAIL);
        assert_eq!(len, 0);
        assert!(matches!(tv.value, TypvalValue::Unknown));
    }

    #[test]
    fn eval_lit_string_missing_closing_quote_after_escaped_pair_fails() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_lit_string(b"'ab''", &mut tv, true, false);
        assert_eq!(ret, crate::vim_defs::FAIL);
        assert_eq!(len, 0);
    }

    #[test]
    fn eval_lit_string_evaluate_false_still_computes_length_but_not_value() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_lit_string(b"'ab''cd'", &mut tv, false, false);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(len, 8);
        assert!(matches!(tv.value, TypvalValue::Unknown));
    }

    #[test]
    fn eval_lit_string_evaluate_false_on_unclosed_string_still_fails() {
        let mut tv = TypvalT::default();
        let (ret, len) = eval_lit_string(b"'abc", &mut tv, false, false);
        assert_eq!(ret, crate::vim_defs::FAIL);
        assert_eq!(len, 0);
    }

    // ---- eval_string / eval_lit_string with interpolate = true ------

    #[test]
    fn eval_string_interpolate_plain_no_braces() {
        // arg is already PAST the opening quote (interpolate's own
        // calling convention).
        let mut tv = TypvalT::default();
        let (ret, consumed) = eval_string(b"hello\"", &mut tv, true, true);
        assert_eq!(ret, OK);
        // Stops AT the closing quote, does not skip past it.
        assert_eq!(consumed, 5);
        assert_eq!(tv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    fn eval_string_interpolate_reduces_doubled_braces() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = eval_string(b"a{{b}}c\"", &mut tv, true, true);
        assert_eq!(ret, OK);
        assert_eq!(consumed, 7); // position of the closing quote
        assert_eq!(tv.value, TypvalValue::String(Some(b"a{b}c".to_vec())));
    }

    #[test]
    fn eval_string_interpolate_stops_at_embedded_expression() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = eval_string(b"abc{expr}\"", &mut tv, true, true);
        assert_eq!(ret, OK);
        assert_eq!(consumed, 3); // position of the '{'
        assert_eq!(tv.value, TypvalValue::String(Some(b"abc".to_vec())));
    }

    #[test]
    fn eval_string_interpolate_stray_closing_curly_fails() {
        let mut tv = TypvalT::default();
        let (ret, _) = eval_string(b"abc}def\"", &mut tv, true, true);
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn eval_string_interpolate_missing_quote_fails() {
        let mut tv = TypvalT::default();
        let (ret, _) = eval_string(b"abc", &mut tv, true, true);
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn eval_string_interpolate_parse_only_mode_stops_at_brace_too() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = eval_string(b"abc{expr}\"", &mut tv, false, true);
        assert_eq!(ret, OK);
        assert_eq!(consumed, 3);
        assert_eq!(tv.value, TypvalValue::Unknown);
    }

    #[test]
    fn eval_lit_string_interpolate_plain_no_braces() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = eval_lit_string(b"hello'", &mut tv, true, true);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(consumed, 5);
        assert_eq!(tv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    fn eval_lit_string_interpolate_reduces_doubled_braces() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = eval_lit_string(b"a{{b}}c'", &mut tv, true, true);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(consumed, 7);
        assert_eq!(tv.value, TypvalValue::String(Some(b"a{b}c".to_vec())));
    }

    #[test]
    fn eval_lit_string_interpolate_stops_at_embedded_expression() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = eval_lit_string(b"abc{expr}'", &mut tv, true, true);
        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(consumed, 3);
        assert_eq!(tv.value, TypvalValue::String(Some(b"abc".to_vec())));
    }

    #[test]
    fn eval_lit_string_interpolate_stray_closing_curly_fails() {
        let mut tv = TypvalT::default();
        let (ret, _) = eval_lit_string(b"abc}def'", &mut tv, true, true);
        assert_eq!(ret, crate::vim_defs::FAIL);
    }

    #[test]
    fn eval_lit_string_interpolate_missing_quote_fails() {
        let mut tv = TypvalT::default();
        let (ret, _) = eval_lit_string(b"abc", &mut tv, true, true);
        assert_eq!(ret, crate::vim_defs::FAIL);
    }

    // ---- eval_isnamec / eval_isnamec1 --------------------------------

    #[test]
    fn eval_isnamec1_true_for_letters_and_underscore() {
        assert!(eval_isnamec1(i32::from(b'a')));
        assert!(eval_isnamec1(i32::from(b'Z')));
        assert!(eval_isnamec1(i32::from(b'_')));
    }

    #[test]
    fn eval_isnamec1_false_for_digits_colon_and_autoload_char() {
        assert!(!eval_isnamec1(i32::from(b'0')));
        assert!(!eval_isnamec1(i32::from(b':')));
        assert!(!eval_isnamec1(i32::from(b'#')));
    }

    #[test]
    fn eval_isnamec_true_for_alnum_underscore_colon_and_autoload_char() {
        assert!(eval_isnamec(i32::from(b'a')));
        assert!(eval_isnamec(i32::from(b'9')));
        assert!(eval_isnamec(i32::from(b'_')));
        assert!(eval_isnamec(i32::from(b':')));
        assert!(eval_isnamec(i32::from(b'#')));
    }

    #[test]
    fn eval_isnamec_false_for_other_punctuation() {
        assert!(!eval_isnamec(i32::from(b'-')));
        assert!(!eval_isnamec(i32::from(b' ')));
    }

    // ---- partial_name -----------------------------------------------

    #[test]
    fn partial_name_null_is_none() {
        assert_eq!(unsafe { partial_name(std::ptr::null()) }, None);
    }

    #[test]
    fn partial_name_uses_pt_name_when_set() {
        let pt = crate::eval::typval_defs::PartialT {
            pt_name: Some(b"MyFunc".to_vec()),
            ..Default::default()
        };
        assert_eq!(unsafe { partial_name(&pt as *const _) }, Some(b"MyFunc".to_vec()));
    }

    #[test]
    fn partial_name_falls_back_to_pt_func_uf_name() {
        let mut uf = crate::eval::typval_defs::UfuncT { uf_name: b"Underlying".to_vec(), ..Default::default() };
        let pt = crate::eval::typval_defs::PartialT {
            pt_name: None,
            pt_func: &mut uf as *mut _,
            ..Default::default()
        };
        assert_eq!(unsafe { partial_name(&pt as *const _) }, Some(b"Underlying".to_vec()));
    }

    #[test]
    fn partial_name_none_when_neither_name_nor_func_set() {
        let pt = crate::eval::typval_defs::PartialT::default();
        assert_eq!(unsafe { partial_name(&pt as *const _) }, None);
    }

    // ---- func_equal ---------------------------------------------------

    fn func_tv(name: &[u8]) -> TypvalT {
        TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Func(Some(name.to_vec())) }
    }

    #[test]
    fn func_equal_true_for_same_name() {
        assert!(unsafe { func_equal(&func_tv(b"Foo"), &func_tv(b"Foo"), false) });
    }

    #[test]
    fn func_equal_false_for_different_names() {
        assert!(!unsafe { func_equal(&func_tv(b"Foo"), &func_tv(b"Bar"), false) });
    }

    #[test]
    fn func_equal_empty_name_and_no_name_considered_the_same() {
        // A VAR_FUNC with an empty string name vs. a VAR_PARTIAL with
        // no pt_name and no pt_func - both resolve to "no name" and
        // are considered equal, matching the original's own "empty
        // and NULL function name considered the same" comment.
        let empty_func = func_tv(b"");
        let pt = crate::eval::typval_defs::PartialT::default();
        let partial_tv = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Partial(&pt as *const _ as *mut _),
        };
        assert!(unsafe { func_equal(&empty_func, &partial_tv, false) });
    }

    #[test]
    fn func_equal_compares_partial_dicts() {
        let _lock = crate::globals::global_state_test_lock();
        let d1 = crate::eval::typval::tv_dict_alloc();
        let d2 = crate::eval::typval::tv_dict_alloc();
        unsafe {
            crate::eval::typval::tv_dict_add_nr(&mut *d1, b"x", 1);
            crate::eval::typval::tv_dict_add_nr(&mut *d2, b"x", 1);

            let pt1 = crate::eval::typval_defs::PartialT {
                pt_name: Some(b"Foo".to_vec()),
                pt_dict: d1,
                ..Default::default()
            };
            let pt2 = crate::eval::typval_defs::PartialT {
                pt_name: Some(b"Foo".to_vec()),
                pt_dict: d2,
                ..Default::default()
            };
            let tv1 = TypvalT {
                v_lock: VarLockStatus::Unlocked,
                value: TypvalValue::Partial(&pt1 as *const _ as *mut _),
            };
            let tv2 = TypvalT {
                v_lock: VarLockStatus::Unlocked,
                value: TypvalValue::Partial(&pt2 as *const _ as *mut _),
            };
            assert!(func_equal(&tv1, &tv2, false));

            crate::eval::typval::tv_dict_add_nr(&mut *d2, b"y", 2);
            assert!(!func_equal(&tv1, &tv2, false));

            crate::eval::typval::tv_dict_unref(d1);
            crate::eval::typval::tv_dict_unref(d2);
        }
    }

    #[test]
    fn func_equal_compares_partial_argv() {
        let pt1 = crate::eval::typval_defs::PartialT {
            pt_name: Some(b"Foo".to_vec()),
            pt_argv: vec![TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(1) }],
            ..Default::default()
        };
        let pt2 = crate::eval::typval_defs::PartialT {
            pt_name: Some(b"Foo".to_vec()),
            pt_argv: vec![TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(1) }],
            ..Default::default()
        };
        let pt3 = crate::eval::typval_defs::PartialT {
            pt_name: Some(b"Foo".to_vec()),
            pt_argv: vec![TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(2) }],
            ..Default::default()
        };
        let tv1 = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Partial(&pt1 as *const _ as *mut _),
        };
        let tv2 = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Partial(&pt2 as *const _ as *mut _),
        };
        let tv3 = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Partial(&pt3 as *const _ as *mut _),
        };
        assert!(unsafe { func_equal(&tv1, &tv2, false) });
        assert!(!unsafe { func_equal(&tv1, &tv3, false) });
    }

    // ---- get_copy_id ----------------------------------------------------

    #[test]
    fn get_copy_id_is_monotonically_increasing_by_copyid_inc() {
        let _lock = crate::globals::global_state_test_lock();
        let a = get_copy_id();
        let b = get_copy_id();
        assert_eq!(b, a + COPYID_INC);
    }

    // ---- var_item_copy ----------------------------------------------------

    #[test]
    fn var_item_copy_of_a_number_is_a_plain_value_copy() {
        let mut to = TypvalT::default();
        let from = TypvalT { value: TypvalValue::Number(42), ..Default::default() };
        let ret = unsafe { var_item_copy(std::ptr::null(), &from, &mut to, false, 0) };
        assert_eq!(ret, OK);
        assert_eq!(to.value, TypvalValue::Number(42));
    }

    #[test]
    fn var_item_copy_of_a_null_list_is_a_null_list() {
        let mut to = TypvalT::default();
        let from = TypvalT { value: TypvalValue::List(std::ptr::null_mut()), ..Default::default() };
        let ret = unsafe { var_item_copy(std::ptr::null(), &from, &mut to, false, 0) };
        assert_eq!(ret, OK);
        assert_eq!(to.value, TypvalValue::List(std::ptr::null_mut()));
    }

    #[test]
    fn var_item_copy_shallow_of_a_list_does_not_recurse_into_nested_items() {
        let _lock = crate::globals::global_state_test_lock();
        let inner = crate::eval::typval::tv_list_alloc(0);
        unsafe { crate::eval::typval::tv_list_ref(inner) };
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_append_owned_tv(
                list,
                TypvalT { value: TypvalValue::List(inner), ..Default::default() },
            )
        };
        let mut to = TypvalT::default();
        let from = TypvalT { value: TypvalValue::List(list), ..Default::default() };
        let ret = unsafe { var_item_copy(std::ptr::null(), &from, &mut to, false, 0) };
        assert_eq!(ret, OK);
        let TypvalValue::List(copy) = to.value else { panic!("expected a List") };
        assert_ne!(copy, list); // the outer list is still a genuine copy.
        unsafe {
            let item = crate::eval::typval::tv_list_first(copy);
            let TypvalValue::List(inner_in_copy) = (*item).li_tv.value else { panic!("expected a List") };
            // Shallow copy: the nested list is the SAME pointer, not
            // itself copied.
            assert_eq!(inner_in_copy, inner);
            crate::eval::typval::tv_list_unref(list);
            crate::eval::typval::tv_list_unref(copy);
        }
    }

    #[test]
    fn var_item_copy_deep_recursion_limit_returns_fail() {
        let _lock = crate::globals::global_state_test_lock();
        // Simulate having already recursed to the limit, matching the
        // original's own "static int recurse" function-local state.
        unsafe { *VAR_ITEM_COPY_RECURSE.get_mut() = DICT_MAXNEST };
        let mut to = TypvalT::default();
        let from = TypvalT { value: TypvalValue::Number(1), ..Default::default() };
        let ret = unsafe { var_item_copy(std::ptr::null(), &from, &mut to, true, 0) };
        assert_eq!(ret, FAIL);
        // Untouched - the recursion-limit check returns before doing
        // anything else, matching the original exactly.
        assert!(matches!(to.value, TypvalValue::Unknown));
        unsafe { *VAR_ITEM_COPY_RECURSE.get_mut() = 0 };
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "var_item_copy(UNKNOWN)")]
    fn var_item_copy_of_unknown_panics_in_debug() {
        let mut to = TypvalT::default();
        let from = TypvalT::default();
        unsafe { var_item_copy(std::ptr::null(), &from, &mut to, false, 0) };
    }

    // ---- set_ref_in_item / set_ref_in_ht / set_ref_in_list_items --------

    #[test]
    fn set_ref_in_item_plain_values_are_always_a_noop() {
        let mut tv = TypvalT { value: TypvalValue::Number(42), ..Default::default() };
        assert!(!unsafe { set_ref_in_item(&mut tv, 1, std::ptr::null_mut(), std::ptr::null_mut()) });
        let mut tv = TypvalT { value: TypvalValue::String(Some(b"x".to_vec())), ..Default::default() };
        assert!(!unsafe { set_ref_in_item(&mut tv, 1, std::ptr::null_mut(), std::ptr::null_mut()) });
    }

    #[test]
    fn set_ref_in_item_dict_null_is_a_noop() {
        let mut tv = TypvalT {
            value: TypvalValue::Dict(std::ptr::null_mut()),
            ..Default::default()
        };
        assert!(!unsafe { set_ref_in_item(&mut tv, 1, std::ptr::null_mut(), std::ptr::null_mut()) });
    }

    #[test]
    fn set_ref_in_ht_marks_a_nested_dict_but_not_itself() {
        let _lock = crate::globals::global_state_test_lock();
        // set_ref_in_ht(d, ...) marks dicts/lists FOUND AS VALUES of
        // d's own items (matching set_ref_in_item_dict, which is what
        // actually sets dv_copy_id) - it never marks `d` itself, only
        // whatever `d`'s items reference.
        let nested = crate::eval::typval::tv_dict_alloc();
        let d = crate::eval::typval::tv_dict_alloc();
        let item = crate::eval::typval::tv_dict_item_alloc(b"x");
        unsafe { (*item).di_tv.value = TypvalValue::Dict(nested) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *d, item) };

        let aborted = unsafe { set_ref_in_ht(d, 7, std::ptr::null_mut()) };
        assert!(!aborted);
        assert_eq!(unsafe { (*nested).dv_copy_id }, 7);
        assert_eq!(unsafe { (*d).dv_copy_id }, 0);

        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn set_ref_in_ht_returns_false_for_a_dict_with_only_plain_values() {
        let _lock = crate::globals::global_state_test_lock();
        let d = crate::eval::typval::tv_dict_alloc();
        let item = crate::eval::typval::tv_dict_item_alloc(b"x");
        unsafe { (*item).di_tv.value = TypvalValue::Number(1) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *d, item) };

        let aborted = unsafe { set_ref_in_ht(d, 7, std::ptr::null_mut()) };
        assert!(!aborted);

        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn set_ref_in_ht_short_circuits_a_dict_reached_twice_from_the_same_parent() {
        let _lock = crate::globals::global_state_test_lock();
        // A "diamond": parent has 2 items both referencing the SAME
        // child dict - proves the dv_copy_id-based short-circuit
        // check works (without needing a genuine reference cycle,
        // which this crate's plain refcounting can't safely free yet
        // - that needs the sweep phase, not yet built).
        let child = crate::eval::typval::tv_dict_alloc();
        unsafe { (*child).dv_refcount = 2 }; // 2 items will reference it

        let parent = crate::eval::typval::tv_dict_alloc();
        let item_a = crate::eval::typval::tv_dict_item_alloc(b"a");
        unsafe { (*item_a).di_tv.value = TypvalValue::Dict(child) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *parent, item_a) };
        let item_b = crate::eval::typval::tv_dict_item_alloc(b"b");
        unsafe { (*item_b).di_tv.value = TypvalValue::Dict(child) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *parent, item_b) };

        let aborted = unsafe { set_ref_in_ht(parent, 3, std::ptr::null_mut()) };
        assert!(!aborted);
        assert_eq!(unsafe { (*child).dv_copy_id }, 3);

        unsafe { crate::eval::typval::tv_dict_free(parent) };
    }

    #[test]
    fn set_ref_in_ht_worklist_handles_deep_linear_nesting_without_stack_overflow() {
        let _lock = crate::globals::global_state_test_lock();
        // A long, non-cyclic chain (dict[N] contains dict[N-1] contains
        // ... contains dict[0]) - proves the explicit worklist
        // (ht_stack) avoids recursion-depth-proportional stack usage,
        // the whole reason set_ref_in_ht/set_ref_in_item_dict exist in
        // this worklist shape rather than a naive recursive walk.
        const DEPTH: usize = 20_000;
        let mut chain: Vec<*mut crate::eval::typval_defs::DictT> = Vec::with_capacity(DEPTH + 1);
        let mut current = crate::eval::typval::tv_dict_alloc();
        chain.push(current);
        for _ in 0..DEPTH {
            let outer = crate::eval::typval::tv_dict_alloc();
            let item = crate::eval::typval::tv_dict_item_alloc(b"inner");
            unsafe { (*item).di_tv.value = TypvalValue::Dict(current) };
            unsafe { crate::eval::typval::tv_dict_add(&mut *outer, item) };
            current = outer;
            chain.push(current);
        }

        let aborted = unsafe { set_ref_in_ht(current, 99, std::ptr::null_mut()) };
        assert!(!aborted);

        // Free every dict shell/item directly and iteratively, rather
        // than via a single tv_dict_free(current) call - that would
        // otherwise cascade recursively through
        // tv_dict_unref -> tv_dict_free at each nested level, a
        // PRE-EXISTING characteristic of this crate's plain
        // refcounting-based free (unrelated to this new
        // set_ref_in_ht/set_ref_in_item_dict code, which is itself
        // genuinely worklist-based, not recursive) that would itself
        // stack-overflow at this depth if left to cascade on its own.
        for d in chain {
            let items: Vec<_> = unsafe { (*d).dv_index.values().copied().collect() };
            for item in items {
                drop(unsafe { Box::from_raw(item) });
            }
            unsafe { crate::eval::typval::tv_dict_free_dict(d) };
        }
    }

    #[test]
    fn set_ref_in_list_items_marks_a_fresh_lists_nested_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let inner_dict = crate::eval::typval::tv_dict_alloc();
        let list = crate::eval::typval::tv_list_alloc(1);
        unsafe { crate::eval::typval::tv_list_ref(list) };
        unsafe {
            crate::eval::typval::tv_list_append_owned_tv(
                list,
                TypvalT { value: TypvalValue::Dict(inner_dict), ..Default::default() },
            )
        };

        let aborted = unsafe { set_ref_in_list_items(list, 5, std::ptr::null_mut()) };
        assert!(!aborted);
        assert_eq!(unsafe { (*inner_dict).dv_copy_id }, 5);

        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    // --- typval_compare ---

    fn evaluate_evalarg() -> EvalargT {
        EvalargT { eval_flags: EVAL_EVALUATE, ..Default::default() }
    }

    /// Parses and fully evaluates `s` via the whole `eval0`-`eval7`
    /// chain, returning `(status, rettv)`.
    fn eval_str(s: &[u8]) -> (i32, TypvalT) {
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let ret = unsafe { eval0(s, &mut rettv, None, Some(&mut evalarg)) };
        (ret, rettv)
    }

    #[test]
    fn typval_compare_number_relational_operators() {
        let cases: &[(ExprType, VarnumberT, VarnumberT, bool)] = &[
            (ExprType::Equal, 1, 1, true),
            (ExprType::Equal, 1, 2, false),
            (ExprType::Nequal, 1, 2, true),
            (ExprType::Greater, 2, 1, true),
            (ExprType::Greater, 1, 2, false),
            (ExprType::Gequal, 1, 1, true),
            (ExprType::Smaller, 1, 2, true),
            (ExprType::Sequal, 1, 1, true),
        ];
        for &(typ, a, b, expected) in cases {
            let mut t1 = TypvalT { value: TypvalValue::Number(a), ..Default::default() };
            let t2 = TypvalT { value: TypvalValue::Number(b), ..Default::default() };
            assert!(unsafe { typval_compare(&mut t1, &t2, typ, false) });
            assert_eq!(t1.value, TypvalValue::Number(VarnumberT::from(expected)), "{typ:?}({a}, {b})");
        }
    }

    #[test]
    fn typval_compare_float_relational_operators() {
        let mut t1 = TypvalT { value: TypvalValue::Float(1.5), ..Default::default() };
        let t2 = TypvalT { value: TypvalValue::Float(2.5), ..Default::default() };
        assert!(unsafe { typval_compare(&mut t1, &t2, ExprType::Smaller, false) });
        assert_eq!(t1.value, TypvalValue::Number(1));
    }

    #[test]
    fn typval_compare_string_case_sensitivity() {
        let t1 = TypvalT { value: TypvalValue::String(Some(b"ABC".to_vec())), ..Default::default() };
        let t2 = TypvalT { value: TypvalValue::String(Some(b"abc".to_vec())), ..Default::default() };

        let mut ic = t1.clone();
        assert!(unsafe { typval_compare(&mut ic, &t2, ExprType::Equal, true) });
        assert_eq!(ic.value, TypvalValue::Number(1));

        let mut cs = t1.clone();
        assert!(unsafe { typval_compare(&mut cs, &t2, ExprType::Equal, false) });
        assert_eq!(cs.value, TypvalValue::Number(0));
    }

    #[test]
    fn typval_compare_is_isnot_different_types_is_always_false_true() {
        let mut t1 = TypvalT { value: TypvalValue::Number(1), ..Default::default() };
        let t2 = TypvalT { value: TypvalValue::String(Some(b"1".to_vec())), ..Default::default() };
        assert!(unsafe { typval_compare(&mut t1, &t2, ExprType::Is, false) });
        assert_eq!(t1.value, TypvalValue::Number(0));

        let mut t1 = TypvalT { value: TypvalValue::Number(1), ..Default::default() };
        assert!(unsafe { typval_compare(&mut t1, &t2, ExprType::Isnot, false) });
        assert_eq!(t1.value, TypvalValue::Number(1));
    }

    #[test]
    fn typval_compare_blob_equal_nequal() {
        use crate::eval::typval::{tv_blob_alloc, tv_blob_free};

        let b1 = tv_blob_alloc();
        let b2 = tv_blob_alloc();
        unsafe {
            (*b1).bv_ga.ga_append(1);
            (*b2).bv_ga.ga_append(1);
        }
        let mut t1 = TypvalT { value: TypvalValue::Blob(b1), ..Default::default() };
        let t2 = TypvalT { value: TypvalValue::Blob(b2), ..Default::default() };
        // typval_compare's own internal tv_clear_simple(typ1) releases
        // b1 (a fresh tv_blob_alloc() starts at refcount 0, so this
        // single unref frees it immediately) - do NOT free b1 again.
        assert!(unsafe { typval_compare(&mut t1, &t2, ExprType::Equal, false) });
        assert_eq!(t1.value, TypvalValue::Number(1));

        unsafe { tv_blob_free(b2) };
    }

    #[test]
    fn typval_compare_blob_is_uses_pointer_identity() {
        use crate::eval::typval::{tv_blob_alloc, tv_blob_free};

        let b1 = tv_blob_alloc();
        let b2 = tv_blob_alloc();
        let mut t1 = TypvalT { value: TypvalValue::Blob(b1), ..Default::default() };
        let t2 = TypvalT { value: TypvalValue::Blob(b2), ..Default::default() };
        // Two distinct, empty blobs: equal by content ("==") but NOT
        // the same object ("is"). typval_compare's own tv_clear_simple
        // already releases b1 (see typval_compare_blob_equal_nequal's
        // own comment) - do not free b1 again.
        assert!(unsafe { typval_compare(&mut t1, &t2, ExprType::Is, false) });
        assert_eq!(t1.value, TypvalValue::Number(0));

        unsafe { tv_blob_free(b2) };
    }

    #[test]
    fn typval_compare_blob_relational_operator_is_a_type_error() {
        use crate::eval::typval::{tv_blob_alloc, tv_blob_free};

        let b1 = tv_blob_alloc();
        let b2 = tv_blob_alloc();
        let mut t1 = TypvalT { value: TypvalValue::Blob(b1), ..Default::default() };
        let t2 = TypvalT { value: TypvalValue::Blob(b2), ..Default::default() };
        // The type-error path ALSO calls tv_clear_simple(typ1),
        // releasing b1 - do not free b1 again.
        assert!(!unsafe { typval_compare(&mut t1, &t2, ExprType::Greater, false) });

        unsafe { tv_blob_free(b2) };
    }

    #[test]
    fn typval_compare_list_equal_and_is() {
        use crate::eval::typval::{tv_list_alloc, tv_list_append_number, tv_list_unref};

        let _lock = crate::globals::global_state_test_lock();

        // "==" (equal by content).
        let l1 = tv_list_alloc(1);
        let l2 = tv_list_alloc(1);
        unsafe {
            tv_list_append_number(l1, 5);
            tv_list_append_number(l2, 5);
        }
        let mut t1 = TypvalT { value: TypvalValue::List(l1), ..Default::default() };
        let t2 = TypvalT { value: TypvalValue::List(l2), ..Default::default() };
        // typval_compare's own tv_clear_simple(typ1) frees l1 (a fresh
        // tv_list_alloc() starts at refcount 0) - do not reuse or free
        // l1 again.
        assert!(unsafe { typval_compare(&mut t1, &t2, ExprType::Equal, false) });
        assert_eq!(t1.value, TypvalValue::Number(1));

        // "is" (pointer identity) - needs a FRESH list for typ1, since
        // l1 was already freed by the call above.
        let l3 = tv_list_alloc(1);
        unsafe { tv_list_append_number(l3, 5) };
        let mut t3 = TypvalT { value: TypvalValue::List(l3), ..Default::default() };
        assert!(unsafe { typval_compare(&mut t3, &t2, ExprType::Is, false) });
        assert_eq!(t3.value, TypvalValue::Number(0), "equal-content lists are not the same object");

        unsafe { tv_list_unref(l2) };
    }

    #[test]
    fn typval_compare_list_invalid_operation_is_a_type_error() {
        use crate::eval::typval::{tv_list_alloc, tv_list_unref};

        let _lock = crate::globals::global_state_test_lock();
        let l1 = tv_list_alloc(0);
        let l2 = tv_list_alloc(0);
        let mut t1 = TypvalT { value: TypvalValue::List(l1), ..Default::default() };
        let t2 = TypvalT { value: TypvalValue::List(l2), ..Default::default() };
        // The type-error path also calls tv_clear_simple(typ1),
        // freeing l1 - do not free l1 again.
        assert!(!unsafe { typval_compare(&mut t1, &t2, ExprType::Smaller, false) });

        unsafe { tv_list_unref(l2) };
    }

    #[test]
    fn typval_compare_dict_equal_and_invalid_op() {
        use crate::eval::typval::{tv_dict_add_nr, tv_dict_alloc, tv_dict_free};

        let _lock = crate::globals::global_state_test_lock();
        let d1 = tv_dict_alloc();
        let d2 = tv_dict_alloc();
        unsafe {
            tv_dict_add_nr(&mut *d1, b"a", 1);
            tv_dict_add_nr(&mut *d2, b"a", 1);
        }
        let mut t1 = TypvalT { value: TypvalValue::Dict(d1), ..Default::default() };
        let t2 = TypvalT { value: TypvalValue::Dict(d2), ..Default::default() };
        // typval_compare's own tv_clear_simple(typ1) frees d1 - do not
        // reuse or free d1 again.
        assert!(unsafe { typval_compare(&mut t1, &t2, ExprType::Equal, false) });
        assert_eq!(t1.value, TypvalValue::Number(1));

        // Needs a FRESH dict for typ1, since d1 was already freed
        // above - this call's own type-error path frees IT too.
        let d3 = tv_dict_alloc();
        unsafe { tv_dict_add_nr(&mut *d3, b"a", 1) };
        let mut t3 = TypvalT { value: TypvalValue::Dict(d3), ..Default::default() };
        assert!(!unsafe { typval_compare(&mut t3, &t2, ExprType::Greater, false) });

        unsafe { tv_dict_free(d2) };
    }

    #[test]
    fn typval_compare_match_against_strings_is_unimplemented() {
        let result = std::panic::catch_unwind(|| {
            let mut t1 = TypvalT { value: TypvalValue::String(Some(b"abc".to_vec())), ..Default::default() };
            let t2 = TypvalT { value: TypvalValue::String(Some(b"a.c".to_vec())), ..Default::default() };
            unsafe { typval_compare(&mut t1, &t2, ExprType::Match, false) }
        });
        assert!(result.is_err(), "expected a panic (pattern_match not yet translated)");
    }

    // --- handle_subscript ---

    #[test]
    fn handle_subscript_nothing_follows_is_ok() {
        let rettv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let mut evalarg = evaluate_evalarg();
        let (ret, consumed) = handle_subscript(b"", &rettv, Some(&evalarg), false);
        assert_eq!(ret, OK);
        assert_eq!(consumed, 0);
        let (ret, consumed) = handle_subscript(b" + 1", &rettv, Some(&mut evalarg).map(|e| &*e), false);
        assert_eq!(ret, OK);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn handle_subscript_index_bracket_panics() {
        let rettv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let evalarg = evaluate_evalarg();
        let result = std::panic::catch_unwind(|| handle_subscript(b"[0]", &rettv, Some(&evalarg), false));
        assert!(result.is_err());
    }

    #[test]
    fn handle_subscript_whitespace_before_bracket_suppresses_it() {
        // "5 [0]" - a space before "[" means it's NOT treated as a
        // subscript continuation (matches the original's own
        // whitespace-sensitivity), so this must NOT panic.
        let rettv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let evalarg = evaluate_evalarg();
        let (ret, consumed) = handle_subscript(b"[0]", &rettv, Some(&evalarg), true);
        assert_eq!(ret, OK);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn handle_subscript_dot_only_continues_for_a_dict() {
        let rettv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let evalarg = evaluate_evalarg();
        // "." after a Number (not a Dict) does not continue.
        let (ret, consumed) = handle_subscript(b".foo", &rettv, Some(&evalarg), false);
        assert_eq!(ret, OK);
        assert_eq!(consumed, 0);

        let dict_rettv = TypvalT { value: TypvalValue::Dict(std::ptr::null_mut()), ..Default::default() };
        let result = std::panic::catch_unwind(|| handle_subscript(b".foo", &dict_rettv, Some(&evalarg), false));
        assert!(result.is_err());
    }

    #[test]
    fn handle_subscript_arrow_method_call_panics_even_with_preceding_whitespace() {
        let rettv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let evalarg = evaluate_evalarg();
        // "->" continues regardless of preceding whitespace (matches
        // the original's own `|| (**arg == '-' && (*arg)[1] == '>')`
        // being a separate OR-branch, not gated by the whitespace
        // check at all).
        let result = std::panic::catch_unwind(|| handle_subscript(b"->len()", &rettv, Some(&evalarg), true));
        assert!(result.is_err());
    }

    // --- clear_evalarg ---

    #[test]
    fn clear_evalarg_none_is_a_no_op() {
        clear_evalarg(None, None);
    }

    #[test]
    fn clear_evalarg_with_no_tofree_is_a_no_op() {
        let mut evalarg = EvalargT::default();
        clear_evalarg(Some(&mut evalarg), None);
        assert!(evalarg.eval_tofree.is_none());
    }

    #[test]
    fn clear_evalarg_with_tofree_and_no_eap_just_drops() {
        let mut evalarg = EvalargT { eval_tofree: Some(b"stale".to_vec()), ..Default::default() };
        clear_evalarg(Some(&mut evalarg), None);
        assert!(evalarg.eval_tofree.is_none());
    }

    #[test]
    fn clear_evalarg_with_tofree_and_eap_swaps_into_cmdline_tofree() {
        let mut evalarg = EvalargT { eval_tofree: Some(b"new_line".to_vec()), ..Default::default() };
        let mut eap = crate::ex_cmds_defs::ExargT { arg: Some(b"old_line".to_vec()), ..Default::default() };
        clear_evalarg(Some(&mut evalarg), Some(&mut eap));
        assert!(evalarg.eval_tofree.is_none());
        assert_eq!(eap.cmdline_tofree, Some(b"old_line".to_vec()));
        assert_eq!(eap.arg, Some(b"new_line".to_vec()));
    }

    // --- eval0-eval7 end-to-end ---

    #[test]
    fn e2e_plain_number() {
        let (ret, tv) = eval_str(b"42");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::Number(42));
    }

    #[test]
    fn e2e_unary_minus() {
        let (ret, tv) = eval_str(b"-5");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::Number(-5));
    }

    #[test]
    fn e2e_unary_not() {
        assert_eq!(eval_str(b"!0").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"!5").1.value, TypvalValue::Number(0));
    }

    #[test]
    fn e2e_double_negation() {
        assert_eq!(eval_str(b"--5").1.value, TypvalValue::Number(5));
    }

    #[test]
    fn e2e_arithmetic_precedence() {
        // "*" binds tighter than "+".
        let (ret, tv) = eval_str(b"1 + 2 * 3");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::Number(7));
    }

    #[test]
    fn e2e_parenthesized_overrides_precedence() {
        let (ret, tv) = eval_str(b"(1 + 2) * 3");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::Number(9));
    }

    #[test]
    fn e2e_nested_parens() {
        assert_eq!(eval_str(b"((1 + 2))").1.value, TypvalValue::Number(3));
    }

    #[test]
    fn e2e_division_and_modulo() {
        assert_eq!(eval_str(b"10 / 3").1.value, TypvalValue::Number(3));
        assert_eq!(eval_str(b"10 % 3").1.value, TypvalValue::Number(1));
    }

    #[test]
    fn e2e_float_arithmetic() {
        let (ret, tv) = eval_str(b"1.5 + 2.5");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::Float(4.0));
    }

    #[test]
    fn e2e_string_concatenation() {
        let (ret, tv) = eval_str(b"'a' . 'b'");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"ab".to_vec())));
    }

    #[test]
    fn e2e_literal_string_escaped_quote() {
        assert_eq!(eval_str(b"'a''b'").1.value, TypvalValue::String(Some(b"a'b".to_vec())));
    }

    #[test]
    fn e2e_comparison_operators() {
        assert_eq!(eval_str(b"1 == 1").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"1 != 2").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"1 < 2").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"2 > 1").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"1 <= 1").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"1 >= 2").1.value, TypvalValue::Number(0));
    }

    #[test]
    fn e2e_is_isnot() {
        assert_eq!(eval_str(b"1 is 1").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"1 isnot 2").1.value, TypvalValue::Number(1));
    }

    #[test]
    fn e2e_logical_and_or() {
        assert_eq!(eval_str(b"1 && 0").1.value, TypvalValue::Number(0));
        assert_eq!(eval_str(b"1 && 1").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"1 || 0").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"0 || 0").1.value, TypvalValue::Number(0));
    }

    #[test]
    fn e2e_ternary() {
        assert_eq!(eval_str(b"1 ? 2 : 3").1.value, TypvalValue::Number(2));
        assert_eq!(eval_str(b"0 ? 2 : 3").1.value, TypvalValue::Number(3));
    }

    #[test]
    fn e2e_falsy_coalescing() {
        assert_eq!(eval_str(b"0 ?? 5").1.value, TypvalValue::Number(5));
        assert_eq!(eval_str(b"3 ?? 5").1.value, TypvalValue::Number(3));
    }

    #[test]
    fn e2e_blob_literal() {
        let (ret, tv) = eval_str(b"0z0011");
        assert_eq!(ret, OK);
        let TypvalValue::Blob(b) = tv.value else { panic!("expected a Blob") };
        assert!(!b.is_null());
        unsafe {
            assert_eq!((*b).bv_ga.ga_len, 2);
            assert_eq!((&(*b).bv_ga.ga_data)[0], 0x00);
            assert_eq!((&(*b).bv_ga.ga_data)[1], 0x11);
            crate::eval::typval::tv_blob_free(b);
        }
    }

    #[test]
    fn e2e_number_plus_numeric_string() {
        assert_eq!(eval_str(b"1 + '2'").1.value, TypvalValue::Number(3));
    }

    #[test]
    fn e2e_case_insensitive_string_equality() {
        assert_eq!(eval_str(b"'ABC' ==? 'abc'").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"'ABC' ==# 'abc'").1.value, TypvalValue::Number(0));
    }

    #[test]
    fn e2e_trailing_garbage_fails() {
        let (ret, _) = eval_str(b"1 2");
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn e2e_unbalanced_paren_fails() {
        let (ret, _) = eval_str(b"(1 + 2");
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn e2e_empty_input_fails() {
        let (ret, _) = eval_str(b"");
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn e2e_leading_and_trailing_whitespace_is_skipped() {
        assert_eq!(eval_str(b"  42  ").1.value, TypvalValue::Number(42));
    }

    #[test]
    fn e2e_eval0_sets_nextcmd_on_success() {
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let mut eap = crate::ex_cmds_defs::ExargT::default();
        let ret = unsafe { eval0(b"1 | echo 2", &mut rettv, Some(&mut eap), Some(&mut evalarg)) };
        assert_eq!(ret, OK);
        assert_eq!(rettv.value, TypvalValue::Number(1));
        // check_nextcmd only skips whitespace BEFORE the separator, not
        // after it - matches the original's own `return s + 1;`
        // (pointing right after "|", not further whitespace-skipped).
        assert_eq!(eap.nextcmd, Some(b" echo 2".to_vec()));
    }

    #[test]
    fn e2e_eval0_sets_nextcmd_on_a_failure_right_before_a_separator() {
        // An unbalanced paren fails to parse, stopping exactly at the
        // "|" - eval0's own FAIL path still finds and sets nextcmd in
        // this case (matches the original's own "some of the
        // expression may not have been consumed" comment).
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let mut eap = crate::ex_cmds_defs::ExargT::default();
        let ret = unsafe { eval0(b"(1 | echo 3", &mut rettv, Some(&mut eap), Some(&mut evalarg)) };
        assert_eq!(ret, FAIL);
        assert_eq!(eap.nextcmd, Some(b" echo 3".to_vec()));
    }

    #[test]
    fn e2e_eval0_does_not_search_past_unrelated_trailing_garbage_for_a_separator() {
        // check_nextcmd only checks whether the very next non-
        // whitespace character (right where parsing stopped) is a
        // separator - it does NOT search further ahead through
        // unrelated text to find a LATER "|", matching the original's
        // own narrow contract exactly.
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let mut eap = crate::ex_cmds_defs::ExargT::default();
        let ret = unsafe { eval0(b"1 2 | echo 3", &mut rettv, Some(&mut eap), Some(&mut evalarg)) };
        assert_eq!(ret, FAIL);
        assert_eq!(eap.nextcmd, None);
    }

    #[test]
    fn e2e_parse_only_mode_does_not_populate_rettv() {
        // evalarg with EVAL_EVALUATE unset: parse-only, rettv stays
        // VAR_UNKNOWN (matches the module doc's own "the functions may
        // return OK, but the rettv will be of type VAR_UNKNOWN"
        // documented contract).
        let mut rettv = TypvalT::default();
        let mut evalarg = EvalargT::default();
        let ret = unsafe { eval0(b"1 + 2", &mut rettv, None, Some(&mut evalarg)) };
        assert_eq!(ret, OK);
        assert_eq!(rettv.value, TypvalValue::Unknown);
    }

    #[test]
    fn e2e_short_circuit_and_does_not_panic_on_unimplemented_rhs() {
        // "0 && ..." must short-circuit WITHOUT actually evaluating a
        // real value for the right-hand side, but "eval4" (its own
        // recursive call) still needs to fully PARSE the right-hand
        // side syntactically - a plain number literal on the RHS is
        // always safely parseable regardless of whether it's actually
        // evaluated, so this specifically avoids any subscript/name-
        // lookup syntax this module's own eval7 doesn't yet support.
        assert_eq!(eval_str(b"0 && 5").1.value, TypvalValue::Number(0));
        assert_eq!(eval_str(b"1 || 5").1.value, TypvalValue::Number(1));
    }

    // --- get_id_len ---

    #[test]
    fn get_id_len_plain_identifier() {
        assert_eq!(get_id_len(b"foo_bar123 rest"), (10, 11));
    }

    #[test]
    fn get_id_len_single_char_namespace_prefix_continues_the_scan() {
        // "s:" (a valid namespace_char prefix) continues into the rest
        // of the identifier as ONE combined name.
        assert_eq!(get_id_len(b"s:myvar"), (7, 7));
    }

    #[test]
    fn get_id_len_non_namespace_single_char_colon_stops_before_it() {
        // "n:" is not a valid namespace prefix (matches a slice like
        // "[n:]") - the scan stops right before the colon.
        assert_eq!(get_id_len(b"n:foo"), (1, 1));
    }

    #[test]
    fn get_id_len_multi_char_prefix_before_colon_stops_there_too() {
        // "xx:" (more than 1 char before the colon) is never a
        // namespace prefix, regardless of content.
        assert_eq!(get_id_len(b"xx:foo"), (2, 2));
    }

    #[test]
    fn get_id_len_empty_is_zero() {
        assert_eq!(get_id_len(b""), (0, 0));
    }

    #[test]
    fn get_id_len_has_no_first_character_restriction_of_its_own() {
        // Unlike find_name_end's own FNE_CHECK_START gate (which uses
        // eval_isnamec1 to reject a leading digit), get_id_len itself
        // just scans while eval_isnamec holds for EVERY character,
        // including the first - a leading digit is only ever rejected
        // earlier, by find_name_end's own check inside get_name_len's
        // real call chain.
        assert_eq!(get_id_len(b"1foo"), (4, 4));
    }

    #[test]
    fn get_id_len_stops_at_the_first_non_isnamec_byte() {
        assert_eq!(get_id_len(b"+foo"), (0, 0));
    }

    // --- find_name_end ---

    #[test]
    fn find_name_end_plain_name_no_magic_braces() {
        let (end, magic) = find_name_end(b"foo_bar(", 0);
        assert_eq!(end, 7);
        assert_eq!(magic, None);
    }

    #[test]
    fn find_name_end_fne_check_start_rejects_invalid_first_byte() {
        assert_eq!(find_name_end(b"1foo", FNE_CHECK_START), (0, None));
        assert_eq!(find_name_end(b"", FNE_CHECK_START), (0, None));
    }

    #[test]
    fn find_name_end_detects_magic_braces_span() {
        let (end, magic) = find_name_end(b"foo{expr}bar ", 0);
        assert_eq!(end, 12);
        assert_eq!(magic, Some((3, 8)));
    }

    #[test]
    fn find_name_end_unterminated_magic_braces_has_zero_expr_end() {
        let (_end, magic) = find_name_end(b"foo{expr", 0);
        assert_eq!(magic, Some((3, 0)));
    }

    #[test]
    fn find_name_end_skips_quoted_content_without_counting_brackets() {
        // A "'" inside the (already-started, via a leading "{") magic-
        // braces span must not have its own embedded "[" mistaken for
        // a real bracket-nest change.
        let (end, magic) = find_name_end(b"foo{'[oops'}bar", 0);
        assert_eq!(magic, Some((3, 11)));
        assert_eq!(end, 15);
    }

    // --- get_name_len ---

    #[test]
    fn get_name_len_plain_global_scoped_name() {
        assert_eq!(get_name_len(b"g:foo ", true), (5, 6));
    }

    #[test]
    fn get_name_len_script_prefixed_name() {
        assert_eq!(get_name_len(b"s:myvar", true), (7, 7));
    }

    #[test]
    fn get_name_len_no_valid_name_is_zero() {
        assert_eq!(get_name_len(b")", true), (0, 0));
        assert_eq!(get_name_len(b"", true), (0, 0));
    }

    #[test]
    fn get_name_len_magic_braces_when_not_evaluating_is_real() {
        let (name_len, consumed) = get_name_len(b"foo{expr}bar(", false);
        assert_eq!(name_len, 12);
        assert_eq!(consumed, 12);
    }

    #[test]
    fn get_name_len_magic_braces_when_evaluating_is_unimplemented() {
        let result = std::panic::catch_unwind(|| get_name_len(b"foo{expr}bar", true));
        assert!(result.is_err(), "expected a panic (make_expanded_name not yet translated)");
    }

    // --- find_option_var_end / eval_option ---

    #[test]
    fn find_option_var_end_bare_name() {
        let (name_start, consumed, opt_idx, opt_flags) = find_option_var_end(b"&ignorecase rest");
        assert_eq!(name_start, 1);
        assert_eq!(consumed, 11);
        assert_eq!(opt_idx, OptIndex::Ignorecase);
        assert_eq!(opt_flags, 0);
    }

    #[test]
    fn find_option_var_end_short_abbreviation() {
        let (name_start, consumed, opt_idx, opt_flags) = find_option_var_end(b"&ic");
        assert_eq!(name_start, 1);
        assert_eq!(consumed, 3);
        assert_eq!(opt_idx, OptIndex::Ignorecase);
        assert_eq!(opt_flags, 0);
    }

    #[test]
    fn find_option_var_end_global_scope_prefix() {
        let (name_start, consumed, opt_idx, opt_flags) = find_option_var_end(b"&g:ignorecase");
        // name_start skips past "&g:" (3 bytes) - the bare name itself
        // must NOT include the "g:" prefix (is_tty_option/get_tty_option
        // need the bare name only).
        assert_eq!(name_start, 3);
        assert_eq!(consumed, 13);
        assert_eq!(&b"&g:ignorecase"[name_start..consumed], b"ignorecase");
        assert_eq!(opt_idx, OptIndex::Ignorecase);
        assert_eq!(opt_flags, crate::option_defs::opt_set_flags::OPT_GLOBAL);
    }

    #[test]
    fn find_option_var_end_local_scope_prefix() {
        let (name_start, consumed, opt_idx, opt_flags) = find_option_var_end(b"&l:tabstop");
        assert_eq!(name_start, 3);
        assert_eq!(&b"&l:tabstop"[name_start..consumed], b"tabstop");
        assert_eq!(opt_idx, OptIndex::Tabstop);
        assert_eq!(opt_flags, crate::option_defs::opt_set_flags::OPT_LOCAL);
    }

    #[test]
    fn find_option_var_end_tty_option_has_invalid_idx_but_real_end() {
        let (name_start, consumed, opt_idx, opt_flags) = find_option_var_end(b"&term");
        assert_eq!(name_start, 1);
        assert_eq!(consumed, 5);
        assert_eq!(opt_idx, OptIndex::Invalid);
        assert_eq!(opt_flags, 0);
    }

    #[test]
    fn find_option_var_end_unrecognized_alpha_name_still_consumes() {
        // Alpha-shaped but unrecognized - find_option_end still reports
        // an end (just with an Invalid index), matching the original.
        let (name_start, consumed, opt_idx, _) = find_option_var_end(b"&notarealoption=x");
        assert_eq!(name_start, 1);
        assert_eq!(consumed, 15);
        assert_eq!(opt_idx, OptIndex::Invalid);
    }

    #[test]
    fn find_option_var_end_non_alpha_start_consumes_nothing() {
        assert_eq!(find_option_var_end(b"&123"), (0, 0, OptIndex::Invalid, 0));
        assert_eq!(find_option_var_end(b"&"), (0, 0, OptIndex::Invalid, 0));
    }

    /// RAII-free helper installing `buf`/`win` as `GLOBALS.curbuf`/
    /// `curwin` for the duration of `f`, then restoring the previous
    /// pointers - matching `option.rs`'s own established inline
    /// save/restore pattern for tests exercising `get_option_value`'s
    /// `GLOBALS.curbuf`/`curwin` dependency (not a full guard type,
    /// since this module has no existing guard to reuse and the
    /// pattern is only needed by a handful of tests here).
    fn with_curbuf_curwin<R>(
        buf: &mut crate::buffer_defs::BufT,
        win: &mut crate::buffer_defs::WinT,
        f: impl FnOnce() -> R,
    ) -> R {
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_buf = globals.curbuf;
        let prev_win = globals.curwin;
        globals.curbuf = buf as *mut crate::buffer_defs::BufT;
        globals.curwin = win as *mut crate::buffer_defs::WinT;

        let result = f();

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curbuf = prev_buf;
        globals.curwin = prev_win;
        result
    }

    #[test]
    fn eval_option_boolean_option_becomes_a_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT::default();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ic = 1;

        with_curbuf_curwin(&mut buf, &mut win, || {
            let mut rettv = TypvalT::default();
            let (ret, consumed) = unsafe { eval_option(b"&ignorecase", Some(&mut rettv), true) };
            assert_eq!(ret, OK);
            assert_eq!(consumed, 11);
            // numbool=true: even a boolean option evaluates to a plain
            // Number, matching real Vimscript's `&ignorecase == 1`.
            assert_eq!(rettv.value, TypvalValue::Number(1));
        });

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ic = 0;
    }

    #[test]
    fn eval_option_number_option() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { b_p_ts: 4, ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();

        with_curbuf_curwin(&mut buf, &mut win, || {
            let mut rettv = TypvalT::default();
            let (ret, consumed) = unsafe { eval_option(b"&tabstop", Some(&mut rettv), true) };
            assert_eq!(ret, OK);
            assert_eq!(consumed, 8);
            assert_eq!(rettv.value, TypvalValue::Number(4));
        });
    }

    #[test]
    fn eval_option_string_option() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT::default();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ambw = Some(b"double".to_vec());

        with_curbuf_curwin(&mut buf, &mut win, || {
            let mut rettv = TypvalT::default();
            let (ret, _) = unsafe { eval_option(b"&ambiwidth", Some(&mut rettv), true) };
            assert_eq!(ret, OK);
            assert_eq!(rettv.value, TypvalValue::String(Some(b"double".to_vec())));
        });

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ambw = None;
    }

    #[test]
    fn eval_option_tty_option() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT::default();

        with_curbuf_curwin(&mut buf, &mut win, || {
            let mut rettv = TypvalT::default();
            // 'term' defaults to "nvim" when unset (get_tty_option's
            // own established default) - no need to set_tty_option
            // first.
            let (ret, consumed) = unsafe { eval_option(b"&term", Some(&mut rettv), true) };
            assert_eq!(ret, OK);
            assert_eq!(consumed, 5);
            assert_eq!(rettv.value, TypvalValue::String(Some(b"nvim".to_vec())));
        });
    }

    #[test]
    fn eval_option_unknown_name_fails() {
        let mut rettv = TypvalT::default();
        let (ret, _) = unsafe { eval_option(b"&notarealoption", Some(&mut rettv), true) };
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn eval_option_no_name_at_all_fails_with_zero_consumed() {
        let mut rettv = TypvalT::default();
        let (ret, consumed) = unsafe { eval_option(b"&", Some(&mut rettv), true) };
        assert_eq!(ret, FAIL);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn eval_option_parse_only_mode_does_not_touch_rettv() {
        let mut rettv = TypvalT::default();
        let (ret, consumed) = unsafe { eval_option(b"&ignorecase", Some(&mut rettv), false) };
        assert_eq!(ret, OK);
        assert_eq!(consumed, 11);
        // evaluate == false: rettv is untouched, still its Default.
        assert_eq!(rettv.value, TypvalValue::Unknown);
    }

    #[test]
    fn eval_option_has_feature_semantics_via_none_rettv() {
        // rettv == None models has("+feature")'s own call pattern:
        // a valid, non-hidden option succeeds without needing a real
        // value.
        let (ret, _) = unsafe { eval_option(b"+ignorecase", None, true) };
        assert_eq!(ret, OK);
    }

    #[test]
    fn eval_option_has_feature_fails_for_a_hidden_option() {
        // OptIndex::Aleph is immutable/hidden (see option.rs's own
        // is_option_hidden test) - has("+aleph")-style lookup fails.
        let (ret, _) = unsafe { eval_option(b"+aleph", None, true) };
        assert_eq!(ret, FAIL);
    }

    // --- get_env_len / eval_env_var ---

    #[test]
    fn get_env_len_scans_identifier_characters_only() {
        assert_eq!(get_env_len(b"FOO_BAR baz"), 7);
        assert_eq!(get_env_len(b""), 0);
        assert_eq!(get_env_len(b"!bad"), 0);
        // No "must not start with a digit" rule - vim_isidc doesn't
        // distinguish position, matching the original's own get_env_len.
        assert_eq!(get_env_len(b"1st"), 3);
    }

    /// Serializes tests that set a real, well-known environment
    /// variable name via `eval_env_var`, matching `os/env.rs`'s own
    /// `homedir_test_lock` precedent for the same reason (Rust's
    /// multi-threaded test runner would otherwise race these against
    /// concurrent tests touching the same name).
    fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn eval_env_var_reads_a_real_environment_variable() {
        let _lock = env_test_lock();
        // SAFETY: serialized via env_test_lock.
        unsafe { std::env::set_var("NERO_TEST_EVAL_ENV_VAR", "hello") };

        let mut rettv = TypvalT::default();
        let (ret, consumed) = unsafe { eval_env_var(b"$NERO_TEST_EVAL_ENV_VAR", &mut rettv, true) };
        assert_eq!(ret, OK);
        assert_eq!(consumed, b"$NERO_TEST_EVAL_ENV_VAR".len());
        assert_eq!(rettv.value, TypvalValue::String(Some(b"hello".to_vec())));

        // SAFETY: serialized via env_test_lock.
        unsafe { std::env::remove_var("NERO_TEST_EVAL_ENV_VAR") };
    }

    #[test]
    fn eval_env_var_empty_name_fails_when_evaluating() {
        let mut rettv = TypvalT::default();
        let (ret, consumed) = unsafe { eval_env_var(b"$", &mut rettv, true) };
        assert_eq!(ret, FAIL);
        assert_eq!(consumed, 1); // the leading '$' is still consumed
    }

    #[test]
    fn eval_env_var_empty_name_still_succeeds_in_parse_only_mode() {
        // Unlike eval_option, the "empty name" FAIL is nested strictly
        // inside "if evaluate" in the original - parse-only mode always
        // succeeds, whatever get_env_len found (even nothing).
        let mut rettv = TypvalT::default();
        let (ret, consumed) = unsafe { eval_env_var(b"$", &mut rettv, false) };
        assert_eq!(ret, OK);
        assert_eq!(consumed, 1);
        assert_eq!(rettv.value, TypvalValue::Unknown); // untouched
    }

    #[test]
    fn eval_env_var_unset_variable_hits_the_expand_env_save_gap() {
        let _lock = env_test_lock();
        // SAFETY: serialized via env_test_lock.
        unsafe { std::env::remove_var("NERO_TEST_EVAL_ENV_VAR_UNSET") };

        let result = std::panic::catch_unwind(|| {
            let mut rettv = TypvalT::default();
            unsafe { eval_env_var(b"$NERO_TEST_EVAL_ENV_VAR_UNSET", &mut rettv, true) }
        });
        assert!(result.is_err(), "expected a panic (expand_env_save not yet translated)");
    }

    #[test]
    fn e2e_environment_variable_reference() {
        let _lock = env_test_lock();
        // SAFETY: serialized via env_test_lock.
        unsafe { std::env::set_var("NERO_TEST_EVAL_ENV_VAR_E2E", "world") };

        let (ret, tv) = eval_str(b"$NERO_TEST_EVAL_ENV_VAR_E2E");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"world".to_vec())));

        // SAFETY: serialized via env_test_lock.
        unsafe { std::env::remove_var("NERO_TEST_EVAL_ENV_VAR_E2E") };
    }

    #[test]
    fn e2e_interpolated_string_no_embedded_expression() {
        let (ret, tv) = eval_str(b"$\"plain\"");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"plain".to_vec())));
    }

    // --- eval0-eval7 end-to-end: variables ---

    /// Resets global-variable/funccal state shared across these tests,
    /// matching `eval/vars.rs`'s own private `reset_shared_state`
    /// helper (can't reuse it directly - different module - but the
    /// same 2 steps are all that's needed for these plain-`g:`-only
    /// tests).
    fn reset_globals_for_test() {
        crate::eval::userfunc::set_current_funccal(std::ptr::null_mut());
        unsafe { crate::eval::vars::vars_clear(&mut *crate::eval::vars::get_globvar_dict()) };
    }

    #[test]
    fn e2e_global_variable_reference() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let item = crate::eval::typval::tv_dict_item_alloc(b"answer");
        unsafe { (*item).di_tv.value = TypvalValue::Number(42) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *crate::eval::vars::get_globvar_dict(), item) };

        let (ret, tv) = eval_str(b"g:answer");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::Number(42));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_global_variable_in_an_expression() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let item = crate::eval::typval::tv_dict_item_alloc(b"x");
        unsafe { (*item).di_tv.value = TypvalValue::Number(10) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *crate::eval::vars::get_globvar_dict(), item) };

        assert_eq!(eval_str(b"g:x + 5").1.value, TypvalValue::Number(15));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_undefined_variable_fails() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let (ret, _) = eval_str(b"g:does_not_exist");
        assert_eq!(ret, FAIL);

        reset_globals_for_test();
    }

    #[test]
    fn e2e_undefined_function_call_fails_gracefully() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();
        crate::eval::userfunc::func_init();

        // "Foo" is user-function-shaped (starts uppercase) - find_func
        // finds nothing (nothing can define a function yet), so this
        // correctly, gracefully FAILs rather than panicking.
        let (ret, _) = eval_str(b"Foo()");
        assert_eq!(ret, FAIL);

        reset_globals_for_test();
    }

    #[test]
    fn e2e_user_defined_function_call_is_unimplemented_when_actually_found() {
        // Register a bare UfuncT directly in the function hash table
        // to prove the (currently unreachable in practice, since
        // nothing parses `:function`) "found a real user function"
        // branch panics rather than silently doing the wrong thing.
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();
        crate::eval::userfunc::func_init();

        let mut fp = Box::new(crate::eval::typval_defs::UfuncT {
            uf_name: b"TestUserFunc\0".to_vec(),
            ..Default::default()
        });
        let fp_ptr = fp.as_mut() as *mut crate::eval::typval_defs::UfuncT;
        assert_eq!(unsafe { crate::eval::userfunc::func_hashtab_add(fp_ptr) }, OK);

        let result = std::panic::catch_unwind(|| eval_str(b"TestUserFunc()"));
        assert!(result.is_err(), "expected a panic (call_user_func_check not yet translated)");

        crate::eval::userfunc::func_init();
        reset_globals_for_test();
    }

    #[test]
    fn e2e_len_builtin_function_call_on_a_string() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let (ret, tv) = eval_str(b"len(\"hello\")");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::Number(5));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_type_builtin_function_call_on_a_number() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let (ret, tv) = eval_str(b"type(42)");
        assert_eq!(ret, OK);
        assert_eq!(
            tv.value,
            TypvalValue::Number(crate::eval::typval_defs::var_type_result::NUMBER.into())
        );

        reset_globals_for_test();
    }

    #[test]
    fn e2e_empty_builtin_function_call_on_strings() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"empty(\"\")").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"empty(\"x\")").1.value, TypvalValue::Number(0));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_builtin_function_call_used_inside_a_larger_expression() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        // len("hello") + 1 == 6 - proves a builtin call's result feeds
        // straight back into the enclosing eval5/eval6 arithmetic chain,
        // not just standing alone as a top-level expression.
        assert_eq!(eval_str(b"len(\"hello\") + 1").1.value, TypvalValue::Number(6));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_builtin_function_call_with_wrong_argument_count_fails() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        // len() takes exactly one argument.
        let (ret, _) = eval_str(b"len()");
        assert_eq!(ret, FAIL);
        let (ret, _) = eval_str(b"len(1, 2)");
        assert_eq!(ret, FAIL);

        reset_globals_for_test();
    }

    #[test]
    fn e2e_and_or_xor_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"and(0x0C, 0x0A)").1.value, TypvalValue::Number(0x08));
        assert_eq!(eval_str(b"or(0x0C, 0x0A)").1.value, TypvalValue::Number(0x0E));
        assert_eq!(eval_str(b"xor(0x0C, 0x0A)").1.value, TypvalValue::Number(0x06));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_abs_builtin_function_call() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"abs(-5)").1.value, TypvalValue::Number(5));
        assert_eq!(eval_str(b"abs(5)").1.value, TypvalValue::Number(5));
        assert_eq!(eval_str(b"abs(-5.5)").1.value, TypvalValue::Float(5.5));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_max_min_builtin_function_calls_on_a_list() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"max([3, 7, 1])").1.value, TypvalValue::Number(7));
        assert_eq!(eval_str(b"min([3, 7, 1])").1.value, TypvalValue::Number(1));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_char2nr_and_nr2char_builtin_function_calls_round_trip() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"char2nr(\"A\")").1.value, TypvalValue::Number(65));
        assert_eq!(eval_str(b"nr2char(65)").1.value, TypvalValue::String(Some(b"A".to_vec())));
        assert_eq!(eval_str(b"char2nr(nr2char(char2nr(\"Z\")))").1.value, TypvalValue::Number(90));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_str2float_and_str2nr_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"str2float(\"2.5\")").1.value, TypvalValue::Float(2.5));
        assert_eq!(eval_str(b"str2nr(\"42\")").1.value, TypvalValue::Number(42));
        assert_eq!(eval_str(b"str2nr(\"0x1A\", 16)").1.value, TypvalValue::Number(26));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_str2list_and_list2str_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let (ret, tv) = eval_str(b"str2list(\"AB\")");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 2);
        unsafe { crate::eval::typval::tv_list_unref(l) };

        assert_eq!(
            eval_str(b"list2str(str2list(\"hi\"))").1.value,
            TypvalValue::String(Some(b"hi".to_vec()))
        );

        reset_globals_for_test();
    }

    #[test]
    fn e2e_float_math_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"sqrt(100)").1.value, TypvalValue::Float(10.0));
        assert_eq!(eval_str(b"floor(1.5)").1.value, TypvalValue::Float(1.0));
        // Epsilon comparison, not exact equality: pow() is a
        // libm-delegated transcendental function whose last-bit
        // rounding can legitimately differ slightly between
        // execution environments (e.g. Miri's own interpreter vs the
        // native platform's libm) - see eval::funcs::tests's own
        // pow_of_known_values comment for the concrete example this
        // was caught from.
        let TypvalValue::Float(pow_result) = eval_str(b"pow(2, 8)").1.value else { panic!("expected a Float") };
        assert!((pow_result - 256.0).abs() < 1e-9, "{pow_result} not close to 256.0");
        assert_eq!(eval_str(b"float2nr(3.9)").1.value, TypvalValue::Number(3));
        // sqrt(sin(0) + 4) == 2.0 - proves a float builtin's result
        // feeds correctly into an enclosing arithmetic expression AND
        // into another builtin call's own argument position.
        assert_eq!(eval_str(b"sqrt(sin(0) + 4)").1.value, TypvalValue::Float(2.0));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_tolower_toupper_trim_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"toupper(\"Hello\")").1.value, TypvalValue::String(Some(b"HELLO".to_vec())));
        assert_eq!(eval_str(b"tolower(\"Hello\")").1.value, TypvalValue::String(Some(b"hello".to_vec())));
        assert_eq!(eval_str(b"trim(\"  hi  \")").1.value, TypvalValue::String(Some(b"hi".to_vec())));
        assert_eq!(
            eval_str(b"trim(\"xxhixx\", \"x\")").1.value,
            TypvalValue::String(Some(b"hi".to_vec()))
        );

        reset_globals_for_test();
    }

    #[test]
    fn e2e_has_key_keys_values_items_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"has_key(#{a: 1}, \"a\")").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"has_key(#{a: 1}, \"b\")").1.value, TypvalValue::Number(0));

        let (ret, tv) = eval_str(b"keys(#{a: 1})");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 1);
        unsafe {
            let item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::String(Some(b"a".to_vec())));
            crate::eval::typval::tv_list_unref(l);
        }

        let (ret, tv) = eval_str(b"values(#{a: 42})");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 1);
        unsafe {
            let item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(42));
            crate::eval::typval::tv_list_unref(l);
        }

        reset_globals_for_test();
    }

    #[test]
    fn e2e_get_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"get([1, 2, 3], 1)").1.value, TypvalValue::Number(2));
        assert_eq!(eval_str(b"get([1, 2, 3], 99, \"default\")").1.value, TypvalValue::String(Some(b"default".to_vec())));
        assert_eq!(eval_str(b"get(#{a: 5}, \"a\")").1.value, TypvalValue::Number(5));
        assert_eq!(eval_str(b"get(#{a: 5}, \"missing\", -1)").1.value, TypvalValue::Number(-1));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_index_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"index([10, 20, 30], 20)").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"index([10, 20, 30], 99)").1.value, TypvalValue::Number(-1));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_reverse_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"reverse(\"hello\")").1.value, TypvalValue::String(Some(b"olleh".to_vec())));

        let (ret, tv) = eval_str(b"reverse([1, 2, 3])");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            let item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(3));
            crate::eval::typval::tv_list_unref(l);
        }

        reset_globals_for_test();
    }

    #[test]
    fn e2e_count_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"count(\"ababab\", \"ab\")").1.value, TypvalValue::Number(3));
        assert_eq!(eval_str(b"count([1, 2, 1, 1], 1)").1.value, TypvalValue::Number(3));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_copy_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        // A List argument read through a real g: variable (not a
        // fresh literal) - exercises copy()'s own result surviving
        // get_func_tv's argument-cleanup loop (which only clears the
        // ARGUMENT typvals, never rettv) exactly as this whole arc's
        // own get_func_tv fix intended, and proves the result is a
        // genuinely separate list, not an alias of g:mylist's own.
        let list = crate::eval::typval::tv_list_alloc(3);
        unsafe {
            crate::eval::typval::tv_list_ref(list);
            crate::eval::typval::tv_list_append_number(&mut *list, 1);
            crate::eval::typval::tv_list_append_number(&mut *list, 2);
            crate::eval::typval::tv_list_append_number(&mut *list, 3);
        }
        let list_item = crate::eval::typval::tv_dict_item_alloc(b"mylist");
        unsafe { (*list_item).di_tv.value = TypvalValue::List(list) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *crate::eval::vars::get_globvar_dict(), list_item) };

        let (ret, tv) = eval_str(b"copy(g:mylist)");
        assert_eq!(ret, OK);
        let TypvalValue::List(list_copy) = tv.value else { panic!("expected a List") };
        assert_ne!(list_copy, list); // a genuinely separate list, not an alias.
        unsafe {
            assert_eq!((*list_copy).lv_refcount, 1);
            let copy_item = crate::eval::typval::tv_list_first(list_copy);
            (*copy_item).li_tv.value = TypvalValue::Number(99);
            // g:mylist's own List must be untouched by that mutation.
            let orig_item = crate::eval::typval::tv_list_first(list);
            assert_eq!((*orig_item).li_tv.value, TypvalValue::Number(1));
            crate::eval::typval::tv_list_unref(list_copy);
        }

        reset_globals_for_test(); // releases g:mylist's own List reference.

        // A Dict argument, similarly.
        let dict = crate::eval::typval::tv_dict_alloc();
        unsafe {
            (*dict).dv_refcount += 1;
            let a = crate::eval::typval::tv_dict_item_alloc(b"a");
            (*a).di_tv.value = TypvalValue::Number(1);
            crate::eval::typval::tv_dict_add(&mut *dict, a);
        }
        let dict_item = crate::eval::typval::tv_dict_item_alloc(b"mydict");
        unsafe { (*dict_item).di_tv.value = TypvalValue::Dict(dict) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *crate::eval::vars::get_globvar_dict(), dict_item) };

        let (ret, tv) = eval_str(b"copy(g:mydict)");
        assert_eq!(ret, OK);
        let TypvalValue::Dict(dict_copy) = tv.value else { panic!("expected a Dict") };
        assert_ne!(dict_copy, dict); // a genuinely separate dict, not an alias.
        unsafe {
            assert_eq!((*dict_copy).dv_refcount, 1);
            let copy_item = crate::eval::typval::tv_dict_find(Some(&mut *dict_copy), b"a").unwrap();
            (*copy_item).di_tv.value = TypvalValue::Number(99);
            // g:mydict's own Dict must be untouched by that mutation.
            let orig_item = crate::eval::typval::tv_dict_find(Some(&mut *dict), b"a").unwrap();
            assert_eq!((*orig_item).di_tv.value, TypvalValue::Number(1));
            crate::eval::typval::tv_dict_unref(dict_copy);
        }

        reset_globals_for_test(); // releases g:mydict's own Dict reference.
    }

    #[test]
    fn e2e_deepcopy_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        // A nested list LITERAL through the real parser chain - deep
        // copy must recurse into the nested list too, not just the
        // outer one.
        let (ret, tv) = eval_str(b"deepcopy([[1, 2], [3, 4]])");
        assert_eq!(ret, OK);
        let TypvalValue::List(outer_copy) = tv.value else { panic!("expected a List") };
        unsafe {
            let first = crate::eval::typval::tv_list_first(outer_copy);
            let TypvalValue::List(inner) = (*first).li_tv.value else { panic!("expected a List") };
            let inner_item = crate::eval::typval::tv_list_first(inner);
            assert_eq!((*inner_item).li_tv.value, TypvalValue::Number(1));
            crate::eval::typval::tv_list_unref(outer_copy);
        }

        // A shared reference via a real g: variable - deepcopy(x)
        // (noref omitted/0) reuses the SAME copy for both
        // occurrences; deepcopy(x, 1) makes two separate copies.
        let inner = crate::eval::typval::tv_list_alloc(0);
        unsafe { crate::eval::typval::tv_list_ref(inner) };
        let outer = crate::eval::typval::tv_list_alloc(2);
        unsafe { crate::eval::typval::tv_list_ref(outer) };
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
        let outer_item = crate::eval::typval::tv_dict_item_alloc(b"outer");
        unsafe { (*outer_item).di_tv.value = TypvalValue::List(outer) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *crate::eval::vars::get_globvar_dict(), outer_item) };

        let (ret, tv) = eval_str(b"deepcopy(g:outer)");
        assert_eq!(ret, OK);
        let TypvalValue::List(shared_copy) = tv.value else { panic!("expected a List") };
        unsafe {
            let first = crate::eval::typval::tv_list_first(shared_copy);
            let second = (*first).li_next;
            let TypvalValue::List(a) = (*first).li_tv.value else { panic!("expected a List") };
            let TypvalValue::List(b) = (*second).li_tv.value else { panic!("expected a List") };
            assert_eq!(a, b); // noref=0 (default): same copy reused.
            crate::eval::typval::tv_list_unref(shared_copy);
        }

        let (ret, tv) = eval_str(b"deepcopy(g:outer, 1)");
        assert_eq!(ret, OK);
        let TypvalValue::List(separate_copy) = tv.value else { panic!("expected a List") };
        unsafe {
            let first = crate::eval::typval::tv_list_first(separate_copy);
            let second = (*first).li_next;
            let TypvalValue::List(a) = (*first).li_tv.value else { panic!("expected a List") };
            let TypvalValue::List(b) = (*second).li_tv.value else { panic!("expected a List") };
            assert_ne!(a, b); // noref=1: two separate copies.
            crate::eval::typval::tv_list_unref(separate_copy);
        }

        reset_globals_for_test(); // releases g:outer's own List reference.
    }

    #[test]
    fn e2e_add_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let (ret, tv) = eval_str(b"add([1, 2], 3)");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 3);
            crate::eval::typval::tv_list_unref(l);
        }

        assert_eq!(eval_str(b"add(5, 1)").1.value, TypvalValue::Number(1)); // not a List/Blob.

        reset_globals_for_test();
    }

    #[test]
    fn e2e_insert_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let (ret, tv) = eval_str(b"insert([2, 3], 1)");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            let item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(1));
            crate::eval::typval::tv_list_unref(l);
        }

        let (ret, tv) = eval_str(b"insert([1, 3], 2, 1)");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            let item = crate::eval::typval::tv_list_first(l);
            let item2 = (*item).li_next;
            assert_eq!((*item2).li_tv.value, TypvalValue::Number(2));
            crate::eval::typval::tv_list_unref(l);
        }

        reset_globals_for_test();
    }

    #[test]
    fn e2e_remove_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"remove([1, 2, 3], 1)").1.value, TypvalValue::Number(2));

        let (ret, tv) = eval_str(b"remove([1, 2, 3], 0, 1)");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 2);
            crate::eval::typval::tv_list_unref(l);
        }

        assert_eq!(eval_str(b"remove(#{a: 1, b: 2}, 'a')").1.value, TypvalValue::Number(1));

        let blob_item = crate::eval::typval::tv_blob_alloc();
        unsafe {
            (*blob_item).bv_ga.ga_data = vec![10, 20];
            (*blob_item).bv_ga.ga_len = 2;
            (*blob_item).bv_refcount += 1;
        }
        let item = crate::eval::typval::tv_dict_item_alloc(b"myblob");
        unsafe { (*item).di_tv.value = TypvalValue::Blob(blob_item) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *crate::eval::vars::get_globvar_dict(), item) };
        assert_eq!(eval_str(b"remove(g:myblob, 0)").1.value, TypvalValue::Number(10));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_extend_and_extendnew_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let (ret, tv) = eval_str(b"extend([1, 2], [3, 4])");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 4);
            crate::eval::typval::tv_list_unref(l);
        }

        let (ret, tv) = eval_str(b"extend(#{a: 1}, #{b: 2})");
        assert_eq!(ret, OK);
        let TypvalValue::Dict(d) = tv.value else { panic!("expected a Dict") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_dict_len(d.as_ref()), 2);
            crate::eval::typval::tv_dict_unref(d);
        }

        // extendnew() through a real g: variable, confirming the
        // original is genuinely untouched.
        let orig = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_ref(orig);
            crate::eval::typval::tv_list_append_number(&mut *orig, 1);
        }
        let item = crate::eval::typval::tv_dict_item_alloc(b"mylist");
        unsafe { (*item).di_tv.value = TypvalValue::List(orig) };
        unsafe { crate::eval::typval::tv_dict_add(&mut *crate::eval::vars::get_globvar_dict(), item) };

        let (ret, tv) = eval_str(b"extendnew(g:mylist, [2])");
        assert_eq!(ret, OK);
        let TypvalValue::List(new_list) = tv.value else { panic!("expected a List") };
        assert_ne!(new_list, orig);
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(new_list), 2);
            assert_eq!(crate::eval::typval::tv_list_len(orig), 1); // g:mylist itself untouched.
            crate::eval::typval::tv_list_unref(new_list);
        }

        reset_globals_for_test();
    }

    #[test]
    fn e2e_range_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let (ret, tv) = eval_str(b"range(3)");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            let item = crate::eval::typval::tv_list_first(l);
            assert_eq!((*item).li_tv.value, TypvalValue::Number(0));
            crate::eval::typval::tv_list_unref(l);
        }

        let (ret, tv) = eval_str(b"range(2, 8, 3)");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 3); // 2, 5, 8.
            crate::eval::typval::tv_list_unref(l);
        }

        reset_globals_for_test();
    }

    #[test]
    fn e2e_repeat_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"repeat('ab', 3)").1.value, TypvalValue::String(Some(b"ababab".to_vec())));

        let (ret, tv) = eval_str(b"repeat([1, 2], 2)");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 4);
            crate::eval::typval::tv_list_unref(l);
        }

        reset_globals_for_test();
    }

    #[test]
    fn e2e_join_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        assert_eq!(eval_str(b"join([1, 2, 3])").1.value, TypvalValue::String(Some(b"1 2 3".to_vec())));
        assert_eq!(eval_str(b"join(['a', 'b'], '-')").1.value, TypvalValue::String(Some(b"a-b".to_vec())));

        reset_globals_for_test();
    }

    #[test]
    fn e2e_flatten_and_flattennew_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let (ret, tv) = eval_str(b"flatten([1, [2, 3], 4])");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 4);
            crate::eval::typval::tv_list_unref(l);
        }

        let (ret, tv) = eval_str(b"flatten([1, [2, [3, 4]]], 1)");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 3); // [1, 2, [3, 4]].
            crate::eval::typval::tv_list_unref(l);
        }

        let (ret, tv) = eval_str(b"flattennew([1, [2, 3]])");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        unsafe {
            assert_eq!(crate::eval::typval::tv_list_len(l), 3);
            crate::eval::typval::tv_list_unref(l);
        }

        reset_globals_for_test();
    }

    #[test]
    fn e2e_localtime_builtin_function_call() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        let (ret, tv) = eval_str(b"localtime()");
        assert_eq!(ret, OK);
        let TypvalValue::Number(n) = tv.value else { panic!("expected a Number") };
        assert!(n > 1_577_836_800);

        reset_globals_for_test();
    }

    #[test]
    fn e2e_getenv_and_environ_builtin_function_calls() {
        let _lock = crate::globals::global_state_test_lock();
        reset_globals_for_test();

        // SAFETY: unique to this test, never touched elsewhere.
        unsafe { std::env::set_var("NERO_TEST_E2E_ENV_VAR", "e2e-value") };
        assert_eq!(
            eval_str(b"getenv('NERO_TEST_E2E_ENV_VAR')").1.value,
            TypvalValue::String(Some(b"e2e-value".to_vec()))
        );
        // SAFETY: forwarded from the set_var call above.
        unsafe { std::env::remove_var("NERO_TEST_E2E_ENV_VAR") };

        assert_eq!(
            eval_str(b"getenv('NERO_TEST_E2E_DEFINITELY_UNSET_VAR')").1.value,
            TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null)
        );

        // environ()'s own full enumeration is not safely reentrant
        // against ANY concurrent env-var mutation from another thread
        // (a well-known, platform-specific hazard, worse on Linux/
        // glibc than Windows - see f_environ's own test module doc
        // comment in funcs.rs) - deliberately checked here with no
        // nearby set_var/remove_var mutation, just a non-empty-dict
        // sanity check.
        let (ret, tv) = eval_str(b"environ()");
        assert_eq!(ret, OK);
        let TypvalValue::Dict(d) = tv.value else { panic!("expected a Dict") };
        unsafe {
            assert!(crate::eval::typval::tv_dict_len(d.as_ref()) > 0);
            crate::eval::typval::tv_dict_unref(d);
        }

        reset_globals_for_test();
    }

    // --- get_literal_key ---

    #[test]
    fn get_literal_key_simple() {
        let mut tv = TypvalT::default();
        // No whitespace directly after "abc" (the very next byte is
        // ':'), so nothing extra is skipped - consumed == the key's
        // own length exactly.
        let (ret, consumed) = get_literal_key(b"abc: 1}", &mut tv);
        assert_eq!(ret, OK);
        assert_eq!(consumed, 3);
        assert_eq!(tv.value, TypvalValue::String(Some(b"abc".to_vec())));
    }

    #[test]
    fn get_literal_key_skips_trailing_whitespace() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = get_literal_key(b"abc  : 1}", &mut tv);
        assert_eq!(ret, OK);
        assert_eq!(consumed, 5);
        assert_eq!(tv.value, TypvalValue::String(Some(b"abc".to_vec())));
    }

    #[test]
    fn get_literal_key_allows_dash_and_underscore() {
        let mut tv = TypvalT::default();
        let (ret, _) = get_literal_key(b"my-key_2:", &mut tv);
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"my-key_2".to_vec())));
    }

    #[test]
    fn get_literal_key_invalid_start_fails() {
        let mut tv = TypvalT::default();
        assert_eq!(get_literal_key(b" abc", &mut tv), (FAIL, 0));
        assert_eq!(get_literal_key(b"", &mut tv), (FAIL, 0));
    }

    // --- eval_list ---

    fn list_item(l: *mut crate::eval::typval_defs::ListT, n: i32) -> TypvalValue {
        let item = unsafe { crate::eval::typval::tv_list_find(l, n) };
        assert!(!item.is_null(), "expected an item at index {n}");
        unsafe { (*item).li_tv.value.clone() }
    }

    #[test]
    fn eval_list_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, consumed) = unsafe { eval_list(b"[]", &mut rettv, Some(&mut evalarg)) };
        assert_eq!(ret, OK);
        assert_eq!(consumed, 2);
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn eval_list_multiple_elements() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, consumed) = unsafe { eval_list(b"[1, 2, 3]", &mut rettv, Some(&mut evalarg)) };
        assert_eq!(ret, OK);
        assert_eq!(consumed, 9);
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 3);
        assert_eq!(list_item(l, 0), TypvalValue::Number(1));
        assert_eq!(list_item(l, 1), TypvalValue::Number(2));
        assert_eq!(list_item(l, 2), TypvalValue::Number(3));
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn eval_list_trailing_comma_allowed() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, _) = unsafe { eval_list(b"[1, 2,]", &mut rettv, Some(&mut evalarg)) };
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = rettv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 2);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn eval_list_missing_comma_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, _) = unsafe { eval_list(b"[1 2]", &mut rettv, Some(&mut evalarg)) };
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn eval_list_unterminated_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, _) = unsafe { eval_list(b"[1, 2", &mut rettv, Some(&mut evalarg)) };
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn eval_list_parse_only_mode_produces_no_real_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = EvalargT::default();
        let (ret, consumed) = unsafe { eval_list(b"[1, 2, 3]", &mut rettv, Some(&mut evalarg)) };
        assert_eq!(ret, OK);
        assert_eq!(consumed, 9);
        assert_eq!(rettv.value, TypvalValue::Unknown);
    }

    // --- eval_dict ---

    #[test]
    fn eval_dict_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, consumed) = unsafe { eval_dict(b"{}", &mut rettv, Some(&mut evalarg), false) };
        assert_eq!(ret, OK);
        assert_eq!(consumed, 2);
        let TypvalValue::Dict(d) = rettv.value else { panic!("expected a Dict") };
        assert_eq!(crate::eval::typval::tv_dict_len(unsafe { d.as_ref() }), 0);
        unsafe { crate::eval::typval::tv_dict_unref(d) };
    }

    #[test]
    fn eval_dict_literal_simple() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, consumed) = unsafe { eval_dict(b"{a: 1, b: 2}", &mut rettv, Some(&mut evalarg), true) };
        assert_eq!(ret, OK);
        assert_eq!(consumed, 12);
        let TypvalValue::Dict(d) = rettv.value else { panic!("expected a Dict") };
        assert_eq!(crate::eval::typval::tv_dict_len(unsafe { d.as_ref() }), 2);
        let item_a = crate::eval::typval::tv_dict_find(unsafe { d.as_mut() }, b"a").unwrap();
        assert_eq!(unsafe { (*item_a).di_tv.value.clone() }, TypvalValue::Number(1));
        unsafe { crate::eval::typval::tv_dict_unref(d) };
    }

    #[test]
    fn eval_dict_non_literal_string_key() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, _) = unsafe { eval_dict(b"{'x': 42}", &mut rettv, Some(&mut evalarg), false) };
        assert_eq!(ret, OK);
        let TypvalValue::Dict(d) = rettv.value else { panic!("expected a Dict") };
        let item = crate::eval::typval::tv_dict_find(unsafe { d.as_mut() }, b"x").unwrap();
        assert_eq!(unsafe { (*item).di_tv.value.clone() }, TypvalValue::Number(42));
        unsafe { crate::eval::typval::tv_dict_unref(d) };
    }

    #[test]
    fn eval_dict_duplicate_key_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, _) = unsafe { eval_dict(b"{a: 1, a: 2}", &mut rettv, Some(&mut evalarg), true) };
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn eval_dict_missing_colon_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, _) = unsafe { eval_dict(b"{a 1}", &mut rettv, Some(&mut evalarg), true) };
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn eval_dict_missing_comma_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, _) = unsafe { eval_dict(b"{a: 1 b: 2}", &mut rettv, Some(&mut evalarg), true) };
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn eval_dict_unterminated_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, _) = unsafe { eval_dict(b"{a: 1", &mut rettv, Some(&mut evalarg), true) };
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn eval_dict_trailing_comma_allowed() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = TypvalT::default();
        let mut evalarg = evaluate_evalarg();
        let (ret, _) = unsafe { eval_dict(b"{a: 1,}", &mut rettv, Some(&mut evalarg), true) };
        assert_eq!(ret, OK);
        let TypvalValue::Dict(d) = rettv.value else { panic!("expected a Dict") };
        assert_eq!(crate::eval::typval::tv_dict_len(unsafe { d.as_ref() }), 1);
        unsafe { crate::eval::typval::tv_dict_unref(d) };
    }

    // --- eval0-eval7 end-to-end: list/dict literals ---

    #[test]
    fn e2e_list_literal() {
        let _lock = crate::globals::global_state_test_lock();
        let (ret, tv) = eval_str(b"[1, 2, 3]");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 3);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn e2e_empty_list_literal() {
        let _lock = crate::globals::global_state_test_lock();
        let (ret, tv) = eval_str(b"[]");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 0);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn e2e_list_equality_comparison() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(eval_str(b"[1, 2] == [1, 2]").1.value, TypvalValue::Number(1));
        assert_eq!(eval_str(b"[1, 2] == [1, 3]").1.value, TypvalValue::Number(0));
    }

    #[test]
    fn e2e_nested_list_literal() {
        let _lock = crate::globals::global_state_test_lock();
        let (ret, tv) = eval_str(b"[[1, 2], [3, 4]]");
        assert_eq!(ret, OK);
        let TypvalValue::List(l) = tv.value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(l) }, 2);
        let inner = list_item(l, 0);
        let TypvalValue::List(inner_l) = inner else { panic!("expected an inner List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(inner_l) }, 2);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn e2e_literal_dict() {
        let _lock = crate::globals::global_state_test_lock();
        let (ret, tv) = eval_str(b"#{a: 1, b: 2}");
        assert_eq!(ret, OK);
        let TypvalValue::Dict(d) = tv.value else { panic!("expected a Dict") };
        assert_eq!(crate::eval::typval::tv_dict_len(unsafe { d.as_ref() }), 2);
        unsafe { crate::eval::typval::tv_dict_unref(d) };
    }

    #[test]
    fn e2e_regular_dict_with_string_key() {
        let _lock = crate::globals::global_state_test_lock();
        let (ret, tv) = eval_str(b"{'a': 1}");
        assert_eq!(ret, OK);
        let TypvalValue::Dict(d) = tv.value else { panic!("expected a Dict") };
        let item = crate::eval::typval::tv_dict_find(unsafe { d.as_mut() }, b"a").unwrap();
        assert_eq!(unsafe { (*item).di_tv.value.clone() }, TypvalValue::Number(1));
        unsafe { crate::eval::typval::tv_dict_unref(d) };
    }

    #[test]
    fn e2e_lambda_syntax_is_unimplemented() {
        let result = std::panic::catch_unwind(|| eval_str(b"{a -> a}"));
        assert!(result.is_err(), "expected a panic (get_lambda_tv not yet translated)");
    }

    #[test]
    fn e2e_bare_hash_without_brace_fails_gracefully() {
        // "#" not immediately followed by "{" is not a literal-dict
        // attempt at all - falls through to name resolution, which
        // correctly FAILs ('#' is not a valid name-starter), matching
        // the original's own NOTDONE-cascade rather than panicking.
        let (ret, _) = eval_str(b"#foo");
        assert_eq!(ret, FAIL);
    }

    #[test]
    fn e2e_double_quoted_string_literal() {
        let (ret, tv) = eval_str(b"\"hello world\"");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"hello world".to_vec())));
    }

    #[test]
    fn e2e_double_quoted_string_with_escapes() {
        let (ret, tv) = eval_str(b"\"line1\\nline2\\ttab\"");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"line1\nline2\ttab".to_vec())));
    }

    #[test]
    fn e2e_double_quoted_string_concatenation() {
        let (ret, tv) = eval_str(b"\"foo\" . \"bar\"");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"foobar".to_vec())));
    }

    #[test]
    fn e2e_double_quoted_string_hex_escape_in_expression() {
        assert_eq!(eval_str(b"\"\\x41\" == 'A'").1.value, TypvalValue::Number(1));
    }

    #[test]
    fn e2e_double_quoted_string_special_key_escape_is_unimplemented() {
        let result = std::panic::catch_unwind(|| eval_str(b"\"\\<C-W>\""));
        assert!(result.is_err(), "expected a panic (find_special_key/trans_special not yet translated)");
    }

    #[test]
    fn e2e_option_value_boolean() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT::default();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ic = 1;

        with_curbuf_curwin(&mut buf, &mut win, || {
            let (ret, tv) = eval_str(b"&ignorecase");
            assert_eq!(ret, OK);
            assert_eq!(tv.value, TypvalValue::Number(1));
        });

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ic = 0;
    }

    #[test]
    fn e2e_option_value_number_in_an_expression() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { b_p_ts: 4, ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();

        with_curbuf_curwin(&mut buf, &mut win, || {
            assert_eq!(eval_str(b"&tabstop + 1").1.value, TypvalValue::Number(5));
        });
    }

    #[test]
    fn e2e_option_value_unknown_name_fails() {
        let (ret, _) = eval_str(b"&notarealoption");
        assert_eq!(ret, FAIL);
    }

    // --- register contents: @r ---

    #[test]
    fn e2e_register_black_hole_is_an_empty_string() {
        let (ret, tv) = eval_str(b"@_");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(Vec::new())));
    }

    #[test]
    fn e2e_register_unnamed_is_null_when_nothing_has_ever_yanked() {
        let _lock = crate::globals::global_state_test_lock();
        let (ret, tv) = eval_str(b"@\"");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(None));
    }

    #[test]
    fn e2e_register_expr_register_returns_source_text_unevaluated() {
        let _lock = crate::globals::global_state_test_lock();
        crate::register::set_expr_line(Some(b"1 + 1".to_vec()));
        let (ret, tv) = eval_str(b"@=");
        assert_eq!(ret, OK);
        // eval7's own real call always uses kGRegExprSrc - "@=" in an
        // expression yields the last @= assignment's own SOURCE TEXT,
        // not its evaluated result (a real, if surprising, upstream
        // quirk - verified directly against eval.c's own case '@').
        assert_eq!(tv.value, TypvalValue::String(Some(b"1 + 1".to_vec())));
        crate::register::set_expr_line(None);
    }

    #[test]
    fn e2e_register_invalid_name_still_succeeds_with_a_null_string() {
        // Unlike eval_option/eval_env_var, eval7's own '@' case never
        // sets `ret = FAIL` at all - an invalid register name still
        // reports OK, just with a null string value (matching the
        // original's own unconditional `break` with no FAIL path).
        let (ret, tv) = eval_str(b"@!");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(None));
    }

    #[test]
    fn e2e_register_bare_at_with_nothing_following() {
        let _lock = crate::globals::global_state_test_lock();
        let (ret, tv) = eval_str(b"@");
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(None));
    }

    // --- skip_expr ---

    #[test]
    fn skip_expr_skips_a_simple_number() {
        // 3, not 2: eval7's own trailing-whitespace lookahead for a
        // possible subscript ('['/'('/'.') unconditionally consumes
        // the space after "42" too, whether or not one actually
        // follows - a pre-existing eval7 behavior, not specific to
        // skip_expr itself.
        let (ret, consumed) = unsafe { skip_expr(b"42 rest", None) };
        assert_eq!(ret, OK);
        assert_eq!(consumed, 3);
    }

    #[test]
    fn skip_expr_does_not_evaluate() {
        // &notarealoption FAILs when actually evaluated (eval_option's
        // own validity check), but skip_expr always evaluates with
        // EVAL_EVALUATE cleared (forwarded as a bare None to eval1,
        // regardless of what's passed in) - proving it never reaches
        // that check.
        let (ret, _) = unsafe { skip_expr(b"&notarealoption", None) };
        assert_eq!(ret, OK);
    }

    #[test]
    fn skip_expr_restores_evalarg_flags_afterward() {
        let mut evalarg = EvalargT { eval_flags: EVAL_EVALUATE, ..Default::default() };
        let (ret, _) = unsafe { skip_expr(b"42", Some(&mut evalarg)) };
        assert_eq!(ret, OK);
        assert_eq!(evalarg.eval_flags, EVAL_EVALUATE);
    }

    // --- eval_to_string / typval2string ---

    #[test]
    fn eval_to_string_number() {
        assert_eq!(unsafe { eval_to_string(b"42", false, false) }, Some(b"42".to_vec()));
    }

    #[test]
    fn eval_to_string_string_literal() {
        assert_eq!(unsafe { eval_to_string(b"\"hello\"", false, false) }, Some(b"hello".to_vec()));
    }

    #[test]
    fn eval_to_string_expression() {
        assert_eq!(unsafe { eval_to_string(b"1 + 1", false, false) }, Some(b"2".to_vec()));
    }

    #[test]
    fn eval_to_string_failure_returns_none() {
        assert_eq!(unsafe { eval_to_string(b"&notarealoption", false, false) }, None);
    }

    #[test]
    fn eval_to_string_use_simple_function_is_unimplemented() {
        let result = std::panic::catch_unwind(|| unsafe { eval_to_string(b"42", false, true) });
        assert!(result.is_err(), "expected a panic (eval0_simple_funccal not yet translated)");
    }

    #[test]
    fn eval_to_string_list_value_is_unimplemented() {
        let _lock = crate::globals::global_state_test_lock();
        // Tests typval2string directly (rather than through the full
        // eval_to_string/eval0 pipeline that would evaluate "[1, 2]"
        // as a real expression) so the allocated list can be cleanly
        // released regardless of the panic below - going through
        // eval_to_string would leak the list into GC_FIRST_LIST
        // forever, since the panic unwinds past any point where this
        // test could reach eval_to_string's own internal `tv` to clean
        // it up, corrupting every later GC-linked-list test in this
        // process.
        let list = crate::eval::typval::tv_list_alloc(0);
        let tv = TypvalT { value: TypvalValue::List(list), ..Default::default() };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { typval2string(&tv, false) }));
        assert!(result.is_err(), "expected a panic (encode_tv2string not yet translated)");
        // SAFETY: list was freshly allocated above and never shared
        // with anything else; typval2string never takes ownership.
        unsafe { crate::eval::typval::tv_list_unref(list) };
    }

    // --- eval_interp_string ---

    #[test]
    fn eval_interp_string_no_embedded_expression() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = unsafe { eval_interp_string(b"$\"hello\"", &mut tv, true) };
        assert_eq!(ret, OK);
        assert_eq!(consumed, 8);
        assert_eq!(tv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    fn eval_interp_string_literal_quote_variant() {
        let mut tv = TypvalT::default();
        let (ret, consumed) = unsafe { eval_interp_string(b"$'hello'", &mut tv, true) };
        assert_eq!(ret, OK);
        assert_eq!(consumed, 8);
        assert_eq!(tv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    fn eval_interp_string_single_embedded_expression() {
        let mut tv = TypvalT::default();
        let (ret, _) = unsafe { eval_interp_string(b"$\"value: {1+1}\"", &mut tv, true) };
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"value: 2".to_vec())));
    }

    #[test]
    fn eval_interp_string_doubled_braces_reduce() {
        let mut tv = TypvalT::default();
        let (ret, _) = unsafe { eval_interp_string(b"$\"a{{b}}c\"", &mut tv, true) };
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(Some(b"a{b}c".to_vec())));
    }

    #[test]
    fn eval_interp_string_missing_quote_still_reports_ok_but_null_string() {
        // Matches the original's own surprising-but-literal behavior:
        // eval_interp_string() always `return OK;` at its very end,
        // even when the inner parsing loop hit FAIL - only the
        // resulting STRING becomes null (this crate's own None) rather
        // than the original's own not-NUL-terminated partial buffer.
        let mut tv = TypvalT::default();
        let (ret, _) = unsafe { eval_interp_string(b"$\"abc", &mut tv, true) };
        assert_eq!(ret, OK);
        assert_eq!(tv.value, TypvalValue::String(None));
    }

    #[test]
    fn e2e_interpolated_string_with_embedded_expression() {
        assert_eq!(eval_str(b"$\"1 + 1 = {1 + 1}\"").1.value, TypvalValue::String(Some(b"1 + 1 = 2".to_vec())));
    }

    #[test]
    fn e2e_interpolated_string_single_quoted_with_embedded_expression() {
        assert_eq!(eval_str(b"$'val={40 + 2}'").1.value, TypvalValue::String(Some(b"val=42".to_vec())));
    }

    // --- list2fpos ---

    #[test]
    fn list2fpos_resolves_fnum_lnum_col_coladd_and_curswant() {
        let l = crate::eval::typval::tv_list_alloc(5);
        for n in [7, 3, 2, 1, 10] {
            unsafe { crate::eval::typval::tv_list_append_number(l, n) };
        }
        let tv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        let mut pos = crate::pos_defs::PosT::default();
        let mut fnum = 0;
        let mut curswant = -1;
        let rc = unsafe { list2fpos(&tv, &mut pos, Some(&mut fnum), Some(&mut curswant), false) };
        assert_eq!(rc, OK);
        assert_eq!(fnum, 7);
        assert_eq!(pos, crate::pos_defs::PosT { lnum: 3, col: 2, coladd: 1 });
        assert_eq!(curswant, 10);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn list2fpos_fnum_zero_resolves_to_current_buffer() {
        let mut buf = crate::buffer_defs::BufT { handle: 42, ..Default::default() };
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_curbuf = globals.curbuf;
        globals.curbuf = &mut buf as *mut crate::buffer_defs::BufT;

        let l = crate::eval::typval::tv_list_alloc(3);
        for n in [0, 5, 1] {
            unsafe { crate::eval::typval::tv_list_append_number(l, n) };
        }
        let tv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        let mut pos = crate::pos_defs::PosT::default();
        let mut fnum = 0;
        let rc = unsafe { list2fpos(&tv, &mut pos, Some(&mut fnum), None, false) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;

        assert_eq!(rc, OK);
        assert_eq!(fnum, 42);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn list2fpos_too_short_list_fails() {
        let l = crate::eval::typval::tv_list_alloc(2);
        for n in [1, 2] {
            unsafe { crate::eval::typval::tv_list_append_number(l, n) };
        }
        let tv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        let mut pos = crate::pos_defs::PosT::default();
        let mut fnum = 0;
        let rc = unsafe { list2fpos(&tv, &mut pos, Some(&mut fnum), None, false) };
        assert_eq!(rc, FAIL);
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn list2fpos_non_list_arg_fails() {
        let tv = TypvalT { value: TypvalValue::Number(5), ..Default::default() };
        let mut pos = crate::pos_defs::PosT::default();
        let rc = unsafe { list2fpos(&tv, &mut pos, None, None, false) };
        assert_eq!(rc, FAIL);
    }

    #[test]
    #[should_panic(expected = "buflist_findnr")]
    fn list2fpos_charcol_true_panics_needs_buflist_findnr() {
        // fnum=1 (nonzero) so GLOBALS.curbuf is never touched - this
        // test intentionally exercises ONLY the charcol=true panic,
        // not fnum-zero resolution (which needs a real buffer set up).
        let l = crate::eval::typval::tv_list_alloc(3);
        for n in [1, 5, 3] {
            unsafe { crate::eval::typval::tv_list_append_number(l, n) };
        }
        let tv = TypvalT { value: TypvalValue::List(l), ..Default::default() };
        let mut pos = crate::pos_defs::PosT::default();
        let mut fnum = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            list2fpos(&tv, &mut pos, Some(&mut fnum), None, true)
        }));
        unsafe { crate::eval::typval::tv_list_unref(l) };
        match result {
            Ok(_) => panic!("expected a panic, but list2fpos returned normally"),
            Err(e) => std::panic::resume_unwind(e),
        }
    }

    // --- setmark_pos ---

    #[test]
    fn setmark_pos_negative_char_fails() {
        let pos = crate::pos_defs::PosT::default();
        let rc = unsafe { crate::mark::setmark_pos(-1, &pos, 0, None) };
        assert_eq!(rc, FAIL);
    }

    #[test]
    fn setmark_pos_quote_char_sets_pcmark_directly_for_a_non_cursor_pointer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_curwin = globals.curwin;
        globals.curwin = &mut win as *mut crate::buffer_defs::WinT;

        let pos = crate::pos_defs::PosT { lnum: 4, col: 1, coladd: 0 };
        let rc = unsafe { crate::mark::setmark_pos(i32::from(b'\''), &pos, 0, None) };

        let w = unsafe { &*globals.curwin };
        assert_eq!(rc, OK);
        assert_eq!(w.w_pcmark, pos);

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }
}

