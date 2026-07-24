//! Translated from `src/nvim/eval/executor.c` (in full).
//!
//! `executor.c` (~240 lines) implements Vimscript's compound-
//! assignment operators (`+=`/`-=`/`*=`/`/=`/`%=`/`.=`) - a small,
//! fully self-contained file. Translated in full: `eexe_mod_op` (the
//! public entry point) plus its private helpers `tv_op_blob`/
//! `tv_op_list`/`tv_op_number`/`tv_op_string`/`tv_op_nr_or_string`/
//! `tv_op_float`.
//!
//! Every dependency already existed: `tv_blob_len`/`tv_list_ref`/
//! `tv_list_extend`/`tv_get_number`/`tv_get_string`/`tv_clear_simple`
//! (`eval/typval.rs`), `num_divide`/`num_modulus`/`grow_string_tv`
//! (`eval/eval.rs`), `concat_str`/`vim_strchr` (`strings.rs`).
//!
//! `tv_clear_simple` (not the original's full, recursion-capable
//! `tv_clear` - see `eval/typval.rs`'s own module doc for why that
//! stays deferred) suffices for every `tv_clear(tv1)` call site here:
//! `tv1`'s type immediately before each such call is always Number,
//! String, or Float (this file's own dispatch, `eexe_mod_op`'s
//! `match tv1.value`, never reaches `tv_op_number`/`tv_op_string` with
//! a List/Dict/Blob/Partial-typed `tv1`), none of which need recursive
//! clearing.
//!
//! The original's `semsg(_(e_letwrong), op)` calls (real, reachable
//! user errors reporting exactly which operator was used wrongly) are
//! omitted - message display, not tractable - while the exact same
//! `OK`/`FAIL` return value is kept, matching this crate's established
//! policy throughout.

use crate::eval::eval::{grow_string_tv, num_divide, num_modulus};
use crate::eval::typval::{tv_blob_len, tv_clear_simple, tv_get_number, tv_get_string, tv_list_extend, tv_list_ref};
use crate::eval::typval_defs::{TypvalT, TypvalValue};
use crate::vim_defs::{FAIL, OK};

/// Handle `"blob1 += blob2"` (`tv_op_blob`).
///
/// # Safety
/// If `tv1`/`tv2`'s value is `Blob`-typed with a non-null pointer, that
/// pointer must be a valid, live `crate::eval::typval_defs::BlobT`.
unsafe fn tv_op_blob(tv1: &mut TypvalT, tv2: &TypvalT, op: u8) -> i32 {
    let TypvalValue::Blob(b2) = &tv2.value else {
        return FAIL;
    };
    let b2 = *b2;
    if op != b'+' {
        return FAIL;
    }

    // Blob += Blob
    if b2.is_null() {
        return OK;
    }

    let TypvalValue::Blob(b1_slot) = &mut tv1.value else {
        // eexe_mod_op only calls tv_op_blob when tv1 is itself a Blob.
        unreachable!("tv_op_blob called with a non-Blob tv1");
    };
    if b1_slot.is_null() {
        *b1_slot = b2;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*b2).bv_refcount += 1 };
        return OK;
    }
    let b1 = *b1_slot;

    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { tv_blob_len(b2) };
    if len > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            let b2_data = (*b2).bv_ga.ga_data.clone();
            (*b1).bv_ga.ga_data.extend_from_slice(&b2_data);
            (*b1).bv_ga.ga_len += len;
        }
    }

    OK
}

/// Handle `"list1 += list2"` (`tv_op_list`).
///
/// # Safety
/// If `tv1`/`tv2`'s value is `List`-typed with a non-null pointer, that
/// pointer must be a valid, live
/// `crate::eval::typval_defs::ListT`.
unsafe fn tv_op_list(tv1: &mut TypvalT, tv2: &TypvalT, op: u8) -> i32 {
    let TypvalValue::List(l2) = &tv2.value else {
        return FAIL;
    };
    let l2 = *l2;
    if op != b'+' {
        return FAIL;
    }

    // List += List
    if l2.is_null() {
        return OK;
    }

    let TypvalValue::List(l1_slot) = &mut tv1.value else {
        // eexe_mod_op only calls tv_op_list when tv1 is itself a List.
        unreachable!("tv_op_list called with a non-List tv1");
    };
    if l1_slot.is_null() {
        *l1_slot = l2;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_list_ref(l2) };
    } else {
        let l1 = *l1_slot;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_list_extend(l1, l2, std::ptr::null_mut()) };
    }

    OK
}

/// Handle number operations: `"nr += nr"`, `"nr -= nr"`, `"nr *= nr"`,
/// `"nr /= nr"`, `"nr %= nr"` (`tv_op_number`).
///
/// # Safety
/// Same as [`tv_clear_simple`]'s own safety doc, applied to `tv1` -
/// sound here since `tv1`'s type on entry is always Number or String
/// (see this module's own doc comment).
unsafe fn tv_op_number(tv1: &mut TypvalT, tv2: &TypvalT, op: u8) -> i32 {
    let n = tv_get_number(tv1);
    if let TypvalValue::Float(tv2_f) = tv2.value {
        if op == b'%' {
            return FAIL;
        }
        let mut f = n as f64;
        match op {
            b'+' => f += tv2_f,
            b'-' => f -= tv2_f,
            b'*' => f *= tv2_f,
            b'/' => f /= tv2_f,
            _ => {}
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(tv1) };
        tv1.value = TypvalValue::Float(f);
    } else {
        let n2 = tv_get_number(tv2);
        let result = match op {
            b'+' => n.wrapping_add(n2),
            b'-' => n.wrapping_sub(n2),
            b'*' => n.wrapping_mul(n2),
            b'/' => num_divide(n, n2),
            b'%' => num_modulus(n, n2),
            _ => n,
        };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_clear_simple(tv1) };
        tv1.value = TypvalValue::Number(result);
    }

    OK
}

/// Handle `"str1 .= str2"` (`tv_op_string`).
///
/// # Safety
/// Same as [`tv_op_number`]'s own safety doc.
unsafe fn tv_op_string(tv1: &mut TypvalT, tv2: &TypvalT) -> i32 {
    if matches!(tv2.value, TypvalValue::Float(_)) {
        return FAIL;
    }

    // str .= str. tv_get_string (not _chk) matches the original's
    // tv_get_string_buf: a type error yields "" rather than failing
    // this whole operation (the emsg() the original fires either way
    // is omitted - see this module's own doc comment).
    let s2 = tv_get_string(tv2);
    if grow_string_tv(tv1, &s2) {
        return OK;
    }

    let s1 = tv_get_string(tv1);
    let s = crate::strings::concat_str(&s1, &s2);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_clear_simple(tv1) };
    tv1.value = TypvalValue::String(Some(s));

    OK
}

/// Handle `"tv1 += tv2"`, `"tv1 -= tv2"`, `"tv1 *= tv2"`, `"tv1 /= tv2"`,
/// `"tv1 %= tv2"`, and `"tv1 .= tv2"` (`tv_op_nr_or_string`).
///
/// # Safety
/// Same as [`tv_op_number`]'s own safety doc.
unsafe fn tv_op_nr_or_string(tv1: &mut TypvalT, tv2: &TypvalT, op: u8) -> i32 {
    if matches!(tv2.value, TypvalValue::List(_)) {
        return FAIL;
    }

    if crate::strings::vim_strchr(b"+-*/%", i32::from(op)).is_some() {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { tv_op_number(tv1, tv2, op) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tv_op_string(tv1, tv2) }
}

/// Handle `"f1 += f2"`, `"f1 -= f2"`, `"f1 *= f2"`, `"f1 /= f2"`
/// (`tv_op_float`).
fn tv_op_float(tv1: &mut TypvalT, tv2: &TypvalT, op: u8) -> i32 {
    if op == b'%' || op == b'.' {
        return FAIL;
    }
    let f = match &tv2.value {
        TypvalValue::Float(f) => *f,
        TypvalValue::Number(_) | TypvalValue::String(_) => tv_get_number(tv2) as f64,
        _ => return FAIL,
    };

    let TypvalValue::Float(f1) = &mut tv1.value else {
        // eexe_mod_op only calls tv_op_float when tv1 is itself a Float.
        unreachable!("tv_op_float called with a non-Float tv1");
    };
    match op {
        b'+' => *f1 += f,
        b'-' => *f1 -= f,
        b'*' => *f1 *= f,
        b'/' => *f1 /= f,
        _ => {}
    }

    OK
}

/// Handle `tv1 += tv2`, `-=`, `*=`, `/=`, `%=`, `.=` (`eexe_mod_op`).
///
/// # Safety
/// If `tv1`/`tv2`'s value is `List`/`Dict`/`Blob`/`Partial`-typed with
/// a non-null pointer, that pointer must be a valid, live
/// `ListT`/`DictT`/`BlobT`/`PartialT`.
pub unsafe fn eexe_mod_op(tv1: &mut TypvalT, tv2: &TypvalT, op: u8) -> i32 {
    // Can't do anything with a Funcref or Dict on the right. v:true and
    // friends only work with "..=".
    if matches!(tv2.value, TypvalValue::Func(_) | TypvalValue::Dict(_))
        || (matches!(tv2.value, TypvalValue::Bool(_) | TypvalValue::Special(_)) && op == b'.')
    {
        // semsg(_(e_letwrong), op) omitted - see this module's own doc
        // comment.
        return FAIL;
    }

    let retval = match &tv1.value {
        TypvalValue::Dict(_)
        | TypvalValue::Func(_)
        | TypvalValue::Partial(_)
        | TypvalValue::Bool(_)
        | TypvalValue::Special(_) => FAIL,
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::Blob(_) => unsafe { tv_op_blob(tv1, tv2, op) },
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::List(_) => unsafe { tv_op_list(tv1, tv2, op) },
        // SAFETY: forwarded from this function's own safety doc.
        TypvalValue::Number(_) | TypvalValue::String(_) => unsafe { tv_op_nr_or_string(tv1, tv2, op) },
        TypvalValue::Float(_) => tv_op_float(tv1, tv2, op),
        TypvalValue::Unknown => unreachable!("eexe_mod_op called with an Unknown tv1"),
    };

    // semsg(_(e_letwrong), op) omitted on the retval != OK path too -
    // see this module's own doc comment.
    retval
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::typval_defs::VarLockStatus;

    fn number_tv(n: crate::eval::typval_defs::VarnumberT) -> TypvalT {
        TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Number(n) }
    }

    fn string_tv(s: &[u8]) -> TypvalT {
        TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::String(Some(s.to_vec())) }
    }

    fn float_tv(f: f64) -> TypvalT {
        TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Float(f) }
    }

    // ---- eexe_mod_op: top-level reject cases ----------------------------

    #[test]
    fn eexe_mod_op_rejects_func_or_dict_on_the_right() {
        let mut tv1 = number_tv(1);
        let tv2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Func(None) };
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &tv2, b'+') }, FAIL);

        let tv2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Dict(std::ptr::null_mut()) };
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &tv2, b'+') }, FAIL);
    }

    #[test]
    fn eexe_mod_op_rejects_bool_or_special_on_the_right_only_for_dot_equals() {
        let mut tv1 = string_tv(b"x");
        let tv2 = TypvalT {
            v_lock: VarLockStatus::Unlocked,
            value: TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True),
        };
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &tv2, b'.') }, FAIL);

        // Not rejected for a non-'.' operator (though tv1 being String
        // means tv_op_number still runs and treats Bool via
        // tv_get_number).
        let mut tv1 = number_tv(1);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &tv2, b'+') }, OK);
    }

    #[test]
    fn eexe_mod_op_rejects_when_tv1_is_dict_func_partial_bool_or_special() {
        let tv2 = number_tv(1);
        for value in [
            TypvalValue::Dict(std::ptr::null_mut()),
            TypvalValue::Func(None),
            TypvalValue::Partial(std::ptr::null_mut()),
            TypvalValue::Bool(crate::eval::typval_defs::BoolVarValue::True),
            TypvalValue::Special(crate::eval::typval_defs::SpecialVarValue::Null),
        ] {
            let mut tv1 = TypvalT { v_lock: VarLockStatus::Unlocked, value };
            assert_eq!(unsafe { eexe_mod_op(&mut tv1, &tv2, b'+') }, FAIL);
        }
    }

    // ---- tv_op_blob -------------------------------------------------------

    #[test]
    fn eexe_mod_op_blob_plus_blob_appends_bytes() {
        let mut b1 = crate::eval::typval_defs::BlobT::default();
        b1.bv_ga.ga_data = vec![1, 2, 3];
        b1.bv_ga.ga_len = 3;
        let mut b2 = crate::eval::typval_defs::BlobT::default();
        b2.bv_ga.ga_data = vec![4, 5];
        b2.bv_ga.ga_len = 2;

        let mut tv1 =
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(&mut b1 as *mut _) };
        let tv2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(&mut b2 as *mut _) };

        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &tv2, b'+') }, OK);
        assert_eq!(b1.bv_ga.ga_data, vec![1, 2, 3, 4, 5]);
        assert_eq!(b1.bv_ga.ga_len, 5);
    }

    #[test]
    fn eexe_mod_op_blob_plus_null_blob_is_a_noop_ok() {
        let mut b1 = crate::eval::typval_defs::BlobT::default();
        let mut tv1 =
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(&mut b1 as *mut _) };
        let tv2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(std::ptr::null_mut()) };

        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &tv2, b'+') }, OK);
    }

    #[test]
    fn eexe_mod_op_blob_minus_blob_fails() {
        let mut b1 = crate::eval::typval_defs::BlobT::default();
        let mut b2 = crate::eval::typval_defs::BlobT::default();
        let mut tv1 =
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(&mut b1 as *mut _) };
        let tv2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(&mut b2 as *mut _) };

        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &tv2, b'-') }, FAIL);
    }

    #[test]
    fn eexe_mod_op_blob_plus_non_blob_fails() {
        let mut b1 = crate::eval::typval_defs::BlobT::default();
        let mut tv1 =
            TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(&mut b1 as *mut _) };
        let tv2 = number_tv(1);

        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &tv2, b'+') }, FAIL);
    }

    // ---- tv_op_list ---------------------------------------------------

    #[test]
    fn eexe_mod_op_list_plus_list_extends_in_place() {
        let _lock = crate::globals::global_state_test_lock();
        let l1 = crate::eval::typval::tv_list_alloc(2);
        let l2 = crate::eval::typval::tv_list_alloc(1);
        unsafe {
            crate::eval::typval::tv_list_append_number(l1, 1);
            crate::eval::typval::tv_list_append_number(l2, 2);

            let mut tv1 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(l1) };
            let tv2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(l2) };

            assert_eq!(eexe_mod_op(&mut tv1, &tv2, b'+'), OK);
            assert_eq!(crate::eval::typval::tv_list_len(l1), 2);

            crate::eval::typval::tv_list_free(l1);
            crate::eval::typval::tv_list_free(l2);
        }
    }

    // ---- tv_op_number ---------------------------------------------------

    #[test]
    fn eexe_mod_op_number_arithmetic() {
        let mut tv1 = number_tv(10);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &number_tv(3), b'+') }, OK);
        assert_eq!(tv1.value, TypvalValue::Number(13));

        let mut tv1 = number_tv(10);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &number_tv(3), b'-') }, OK);
        assert_eq!(tv1.value, TypvalValue::Number(7));

        let mut tv1 = number_tv(10);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &number_tv(3), b'*') }, OK);
        assert_eq!(tv1.value, TypvalValue::Number(30));

        let mut tv1 = number_tv(10);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &number_tv(3), b'/') }, OK);
        assert_eq!(tv1.value, TypvalValue::Number(3));

        let mut tv1 = number_tv(10);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &number_tv(3), b'%') }, OK);
        assert_eq!(tv1.value, TypvalValue::Number(1));
    }

    #[test]
    fn eexe_mod_op_number_becomes_float_when_tv2_is_float() {
        let mut tv1 = number_tv(10);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &float_tv(0.5), b'+') }, OK);
        assert_eq!(tv1.value, TypvalValue::Float(10.5));
    }

    #[test]
    fn eexe_mod_op_number_percent_float_fails() {
        let mut tv1 = number_tv(10);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &float_tv(0.5), b'%') }, FAIL);
    }

    #[test]
    fn eexe_mod_op_number_string_arithmetic_parses_numeric_string() {
        let mut tv1 = number_tv(10);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &string_tv(b"5"), b'+') }, OK);
        assert_eq!(tv1.value, TypvalValue::Number(15));
    }

    // ---- tv_op_string ---------------------------------------------------

    #[test]
    fn eexe_mod_op_string_dot_equals_concatenates() {
        let mut tv1 = string_tv(b"foo");
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &string_tv(b"bar"), b'.') }, OK);
        assert_eq!(tv1.value, TypvalValue::String(Some(b"foobar".to_vec())));
    }

    #[test]
    fn eexe_mod_op_string_dot_equals_number_stringifies() {
        let mut tv1 = string_tv(b"x=");
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &number_tv(42), b'.') }, OK);
        assert_eq!(tv1.value, TypvalValue::String(Some(b"x=42".to_vec())));
    }

    #[test]
    fn eexe_mod_op_string_dot_equals_float_fails() {
        let mut tv1 = string_tv(b"x");
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &float_tv(1.0), b'.') }, FAIL);
    }

    #[test]
    fn eexe_mod_op_number_op_on_non_numeric_string_tv1_parses_as_zero() {
        // "abc" + 1: tv_get_number("abc") is 0 (vim_str2nr's own
        // established non-numeric-string-parses-as-zero behavior) -
        // this ALWAYS succeeds (OK), it just computes 0 + 1 = 1,
        // matching the original's own permissive numeric coercion.
        let mut tv1 = string_tv(b"abc");
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &number_tv(1), b'+') }, OK);
        assert_eq!(tv1.value, TypvalValue::Number(1));
    }

    #[test]
    fn eexe_mod_op_list_operand_on_the_right_fails_for_string_op() {
        let mut tv1 = string_tv(b"x");
        let tv2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::List(std::ptr::null_mut()) };
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &tv2, b'.') }, FAIL);
    }

    // ---- tv_op_float ---------------------------------------------------

    #[test]
    fn eexe_mod_op_float_arithmetic() {
        let mut tv1 = float_tv(1.5);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &float_tv(0.5), b'+') }, OK);
        assert_eq!(tv1.value, TypvalValue::Float(2.0));

        let mut tv1 = float_tv(1.5);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &number_tv(1), b'-') }, OK);
        assert_eq!(tv1.value, TypvalValue::Float(0.5));
    }

    #[test]
    fn eexe_mod_op_float_percent_or_dot_fails() {
        let mut tv1 = float_tv(1.5);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &float_tv(0.5), b'%') }, FAIL);
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &string_tv(b"x"), b'.') }, FAIL);
    }

    #[test]
    fn eexe_mod_op_float_with_non_numeric_operand_fails() {
        let mut tv1 = float_tv(1.5);
        let tv2 = TypvalT { v_lock: VarLockStatus::Unlocked, value: TypvalValue::Blob(std::ptr::null_mut()) };
        assert_eq!(unsafe { eexe_mod_op(&mut tv1, &tv2, b'+') }, FAIL);
    }
}
