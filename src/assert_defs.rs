//! Translated from `src/nvim/assert_defs.h`.
//!
//! `STATIC_ASSERT(cond, msg)` / `STATIC_ASSERT_EXPR(cond, msg)`: C compile-time
//! assertions. Rust has a native equivalent used directly at each call site,
//! so no wrapper function/macro is needed:
//!
//! ```ignore
//! const _: () = assert!(cond, "message");
//! ```
//!
//! This is used inline wherever the original C file had a `STATIC_ASSERT`,
//! when that file is translated.

/// `STRICT_ADD(a, b, c, t)`: adds `(a + b)` and stores the result in `*c`.
/// Aborts (panics) on overflow. Requires `a`/`b` to be the same integer type;
/// `t` in the original is only used for the non-overflow-checked fallback
/// build and has no Rust equivalent (Rust always checks here).
#[macro_export]
macro_rules! strict_add {
    ($a:expr, $b:expr) => {
        ($a).checked_add($b).unwrap_or_else(|| panic!("STRICT_ADD overflow"))
    };
}

/// `STRICT_SUB(a, b, c, t)`: subtracts `(a - b)` and stores the result in
/// `*c`. Aborts (panics) on overflow.
#[macro_export]
macro_rules! strict_sub {
    ($a:expr, $b:expr) => {
        ($a).checked_sub($b).unwrap_or_else(|| panic!("STRICT_SUB overflow"))
    };
}
