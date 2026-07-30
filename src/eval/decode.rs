//! Translated from `src/nvim/eval/decode.c` (tractable core only).
//!
//! `decode.c` (~1200 lines) implements JSON decoding
//! (`json_decode_string`/`f_json_decode`) and msgpack decoding
//! (`mpack_parse_typval`/`unpack_typval`), both built on a shared
//! stack-based parser (`ContainerStack`/`ValuesStack`) - none of that
//! parsing machinery is translated here; it needs a genuine parser
//! state machine, a substantial separate undertaking.
//!
//! Translated: [`decode_string`] - converts an already-decoded raw
//! byte string into the right Vimscript value (`String`, or `Blob`
//! when it contains an embedded NUL byte a `String` typval can't
//! represent). Every one of its own real dependencies
//! (`tv_blob_alloc_ret`, plain NUL-byte scanning) already existed;
//! translated ahead of its own real callers (`json_decode_string`/
//! `mpack_parse_typval`'s own string-handling branches, neither
//! translated yet), matching this crate's established "small,
//! self-contained, no design freedom to get wrong" precedent for
//! translating ahead of a real caller.
//!
//! `decode_create_map_special_dict` (the OTHER function in this same
//! source region) remains deferred: it needs `create_special_dict` ->
//! `eval_msgpack_type_lists` (`eval/vars.c`), the per-type `List`
//! table `evalvars_init`'s own still-deferred `v:msgpack_types` piece
//! populates - genuinely blocked on that specific, already-identified
//! gap, not a new one.

use crate::eval::typval::tv_blob_alloc_ret;
use crate::eval::typval_defs::{TypvalT, TypvalValue};

/// Convert an already-decoded byte string into the right Vimscript
/// value: a `Blob` when `force_blob` is set OR `s` contains an
/// embedded NUL byte (which a `String` typval can't represent),
/// otherwise a `String` (`decode_string`).
///
/// Unlike the original's own `s_allocated: bool` parameter (which
/// decides between taking ownership of an already heap-allocated `s`
/// versus copying a borrowed one via `xmemdupz`), this crate's
/// `s: Vec<u8>` is ALWAYS already an owned buffer with nothing left
/// to copy - that distinction, and the original's `s_allocated ==
/// false` copy path, has no equivalent here; `s` is always moved
/// directly into the result.
#[must_use]
pub fn decode_string(s: Vec<u8>, force_blob: bool) -> TypvalT {
    let use_blob = force_blob || s.contains(&0);
    if use_blob {
        let mut tv = TypvalT::default();
        let b = tv_blob_alloc_ret(&mut tv);
        let len = s.len() as i32;
        // SAFETY: `b` was just allocated above by `tv_blob_alloc_ret`,
        // a fresh pointer not shared with anything else yet.
        unsafe {
            (*b).bv_ga.ga_data = s;
            (*b).bv_ga.ga_len = len;
        }
        tv
    } else {
        TypvalT { value: TypvalValue::String(Some(s)), ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::typval::tv_blob_unref;

    #[test]
    fn decode_string_of_a_plain_byte_string_is_a_string() {
        let tv = decode_string(b"hello".to_vec(), false);
        assert_eq!(tv.value, TypvalValue::String(Some(b"hello".to_vec())));
    }

    #[test]
    fn decode_string_of_an_empty_byte_string_is_an_empty_string() {
        let tv = decode_string(Vec::new(), false);
        assert_eq!(tv.value, TypvalValue::String(Some(Vec::new())));
    }

    #[test]
    fn decode_string_with_an_embedded_nul_becomes_a_blob() {
        let tv = decode_string(vec![b'a', 0, b'b'], false);
        let TypvalValue::Blob(b) = tv.value else { panic!("expected a Blob") };
        assert!(!b.is_null());
        unsafe {
            assert_eq!((*b).bv_ga.ga_data, vec![b'a', 0, b'b']);
            assert_eq!((*b).bv_ga.ga_len, 3);
            assert_eq!((*b).bv_refcount, 1);
            tv_blob_unref(b);
        }
    }

    #[test]
    fn decode_string_force_blob_makes_a_blob_even_without_a_nul() {
        let tv = decode_string(b"abc".to_vec(), true);
        let TypvalValue::Blob(b) = tv.value else { panic!("expected a Blob") };
        unsafe {
            assert_eq!((*b).bv_ga.ga_data, b"abc".to_vec());
            assert_eq!((*b).bv_ga.ga_len, 3);
            tv_blob_unref(b);
        }
    }

    #[test]
    fn decode_string_force_blob_of_an_empty_string_is_an_empty_blob() {
        let tv = decode_string(Vec::new(), true);
        let TypvalValue::Blob(b) = tv.value else { panic!("expected a Blob") };
        unsafe {
            assert_eq!((*b).bv_ga.ga_len, 0);
            tv_blob_unref(b);
        }
    }
}
