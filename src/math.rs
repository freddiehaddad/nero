//! Translated from `src/nvim/math.c` (`src/nvim/math.h` has no manually
//! written content - it's just `#include "math.h.generated.h"`, the
//! cross-translation-unit inline-declaration generator with no Rust
//! equivalent, same as noted in `src/nvim/ascii_defs.rs`).

use crate::vim_defs::{FAIL, OK};

/// C's `fpclassify()` result categories (`<math.h>`), used by `xfpclassify`.
/// Rust's `f64::classify()` (`std::num::FpCategory`) is the native,
/// standard-library equivalent of the same IEEE-754 classification C's
/// `fpclassify()`/`FP_*` macros provide, so it's used directly here instead
/// of re-implementing the manual bit-manipulation the original falls back
/// to when no compiler builtin is available.
pub const FP_INFINITE: i32 = 1;
pub const FP_NAN: i32 = 2;
pub const FP_NORMAL: i32 = 3;
pub const FP_SUBNORMAL: i32 = 4;
pub const FP_ZERO: i32 = 5;

/// `xfpclassify`
#[inline]
pub fn xfpclassify(d: f64) -> i32 {
    match d.classify() {
        std::num::FpCategory::Nan => FP_NAN,
        std::num::FpCategory::Infinite => FP_INFINITE,
        std::num::FpCategory::Zero => FP_ZERO,
        std::num::FpCategory::Subnormal => FP_SUBNORMAL,
        std::num::FpCategory::Normal => FP_NORMAL,
    }
}

/// `xisinf`
#[inline]
pub fn xisinf(d: f64) -> bool {
    d.is_infinite()
}

/// `xisnan`
#[inline]
pub fn xisnan(d: f64) -> bool {
    d.is_nan()
}

/// Count trailing zeroes at the end of bit field (`xctz`).
///
/// The original falls back to a compiler builtin
/// (`__builtin_ctzll`/`_BitScanForward64`) or, failing that, a manual bit
/// loop; `u64::trailing_zeros()` is Rust's native equivalent of that same
/// compiler builtin (including returning the bit width, 64, for `x == 0`,
/// matching the original's explicit `x == 0` special case).
#[inline]
pub fn xctz(x: u64) -> u32 {
    x.trailing_zeros()
}

/// Count number of set bits in bit field (`xpopcount`).
///
/// Native equivalent of the original's `__builtin_popcountll` (with a
/// manual fallback loop when unavailable): `u64::count_ones()`.
#[inline]
pub fn xpopcount(x: u64) -> u32 {
    x.count_ones()
}

/// For overflow detection, add a digit safely to an int value
/// (`vim_append_digit_int`).
#[inline]
pub fn vim_append_digit_int(value: &mut i32, digit: i32) -> i32 {
    let x = *value;
    if x > (i32::MAX - digit) / 10 {
        return FAIL;
    }
    *value = x * 10 + digit;
    OK
}

/// Return something that fits into an int (`trim_to_int`).
#[inline]
pub fn trim_to_int(x: i64) -> i32 {
    if x > i32::MAX as i64 {
        i32::MAX
    } else if x < i32::MIN as i64 {
        i32::MIN
    } else {
        x as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fpclassify_matches_c_categories() {
        assert_eq!(xfpclassify(0.0), FP_ZERO);
        assert_eq!(xfpclassify(1.0), FP_NORMAL);
        assert_eq!(xfpclassify(f64::NAN), FP_NAN);
        assert_eq!(xfpclassify(f64::INFINITY), FP_INFINITE);
        assert_eq!(xfpclassify(5e-324), FP_SUBNORMAL);
    }

    #[test]
    fn isinf_isnan() {
        assert!(xisinf(f64::INFINITY));
        assert!(xisinf(f64::NEG_INFINITY));
        assert!(!xisinf(1.0));
        assert!(xisnan(f64::NAN));
        assert!(!xisnan(1.0));
    }

    #[test]
    fn ctz_matches_c_semantics() {
        assert_eq!(xctz(0), 64); // 8 * sizeof(uint64_t)
        assert_eq!(xctz(1), 0);
        assert_eq!(xctz(8), 3);
        assert_eq!(xctz(1u64 << 40), 40);
    }

    #[test]
    fn popcount_counts_set_bits() {
        assert_eq!(xpopcount(0), 0);
        assert_eq!(xpopcount(0b1011), 3);
        assert_eq!(xpopcount(u64::MAX), 64);
    }

    #[test]
    fn append_digit_detects_overflow() {
        let mut v = 0;
        assert_eq!(vim_append_digit_int(&mut v, 5), OK);
        assert_eq!(v, 5);
        let mut near_max = i32::MAX - 1;
        assert_eq!(vim_append_digit_int(&mut near_max, 9), FAIL);
    }

    #[test]
    fn trim_to_int_clamps() {
        assert_eq!(trim_to_int(10), 10);
        assert_eq!(trim_to_int(i64::MAX), i32::MAX);
        assert_eq!(trim_to_int(i64::MIN), i32::MIN);
    }
}
