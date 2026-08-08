//! Translated from `src/nvim/macros_defs.h`.
//!
//! Many macros in the original header have no standalone Rust translation
//! because Rust's language/stdlib already provides the equivalent natively;
//! those are documented here (not silently dropped) with where the
//! equivalent is used instead:
//!
//! - `MIN`/`MAX`               -> `std::cmp::min` / `std::cmp::max` (or
//!   `Ord::min`/`Ord::max`) at each call site.
//! - `S_LEN(s)`                -> not needed; Rust `&str`/`&[u8]` slices
//!   already carry their length (`s.len()`).
//! - `ARRAY_SIZE`/`ARRAY_LAST_ENTRY` -> `slice.len()` / `slice[slice.len() - 1]`.
//! - `STR_`/`STR`              -> `stringify!()`.
//! - `FALLTHROUGH`             -> structured directly via `match` arms at
//!   each call site; Rust `match` doesn't fall through by default.
//! - `NVIM_HAS_INCLUDE`/`NVIM_HAS_ATTRIBUTE` -> N/A, no C preprocessor.
//! - `EXPECT(cond, value)`     -> no-op; stable Rust has no branch-prediction
//!   hint (nightly-only `core::intrinsics::likely`), so `cond` is used as-is.
//! - `UNREACHABLE`             -> `unreachable!()` (or `unreachable_unchecked()`
//!   in an `unsafe` block) at each call site.
//! - `PRAGMA_DIAG_PUSH_IGNORE_*` -> `#[allow(...)]` attributes at the
//!   specific site instead of a push/pop pragma pair.
//!
//! Deferred (need types/globals not yet translated - not faked):
//! - `EXTERN`/`INIT` (global-variable-definition trick from `globals.h`):
//!   deferred to when each global is translated alongside its owning
//!   subsystem; Rust models global mutable state explicitly (e.g. `static`
//!   with interior mutability) rather than via a header trick.
//! - `REPLACE_NORMAL(s)`: needs `REPLACE_FLAG`/`VREPLACE_FLAG` (state_defs.h /
//!   insert.c, phase 7).
//! - `UV_BUF_LEN`/`IO_COUNT`: platform read/write casts tied to libuv
//!   (event/, phase 11).

use crate::pos_defs::PosT;

/// `RESET_BINDING(wp)`: clear a window's `'scrollbind'` and
/// `'cursorbind'`.
///
/// Used when a window stops being tied to another - splitting off a
/// new window, or entering a buffer that should not inherit the
/// binding.
///
/// # Safety
/// `wp` must point at a live [`crate::buffer_defs::WinT`].
pub unsafe fn reset_binding(wp: *mut crate::buffer_defs::WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*wp).w_onebuf_opt.wo_scb = 0;
        (*wp).w_onebuf_opt.wo_crb = 0;
    }
}

/// `TOUPPER_LOC`/`TOLOWER_LOC`: toupper()/tolower() using the current C
/// locale. Careful: only call with a character in the range 0-255, like the
/// original.
#[inline]
pub fn toupper_loc(c: i32) -> i32 {
    unsafe { libc::toupper(c) }
}

#[inline]
pub fn tolower_loc(c: i32) -> i32 {
    unsafe { libc::tolower(c) }
}

/// `TOUPPER_ASC(c)`: ASCII-only, locale-independent.
#[inline]
pub fn toupper_asc(c: i32) -> i32 {
    if !(b'a' as i32..=b'z' as i32).contains(&c) {
        c
    } else {
        c - (b'a' as i32 - b'A' as i32)
    }
}

/// `TOLOWER_ASC(c)`: ASCII-only, locale-independent.
#[inline]
pub fn tolower_asc(c: i32) -> i32 {
    if !(b'A' as i32..=b'Z' as i32).contains(&c) {
        c
    } else {
        c + (b'a' as i32 - b'A' as i32)
    }
}

/// `ASCII_ISLOWER(c)`: like `islower()` but rejects non-ASCII. Can't be used
/// with a special key (negative value).
#[inline]
pub fn ascii_islower(c: i32) -> bool {
    (b'a' as i32..=b'z' as i32).contains(&c)
}

/// `ASCII_ISUPPER(c)`
#[inline]
pub fn ascii_isupper(c: i32) -> bool {
    (b'A' as i32..=b'Z' as i32).contains(&c)
}

/// `ASCII_ISALPHA(c)`
#[inline]
pub fn ascii_isalpha(c: i32) -> bool {
    ascii_isupper(c) || ascii_islower(c)
}

/// `ASCII_ISALNUM(c)`
#[inline]
pub fn ascii_isalnum(c: i32) -> bool {
    ascii_isalpha(c) || crate::ascii_defs::ascii_isdigit(c)
}

/// `RGB_(r, g, b)`
#[inline]
pub const fn rgb_(r: u32, g: u32, b: u32) -> u32 {
    (r << 16) | (g << 8) | b
}

/// no CR-LF translation (`WRITEBIN`)
pub const WRITEBIN: &str = "wb";
pub const READBIN: &str = "rb";
pub const APPENDBIN: &str = "ab";

/// `EMPTY_POS(a)`
#[inline]
pub fn empty_pos(a: &PosT) -> bool {
    a.lnum == 0 && a.col == 0 && a.coladd == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_binding_clears_both_bindings() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_scb = 1;
        win.w_onebuf_opt.wo_crb = 1;

        unsafe { reset_binding(&mut win) };

        assert_eq!(win.w_onebuf_opt.wo_scb, 0);
        assert_eq!(win.w_onebuf_opt.wo_crb, 0);
    }

    #[test]
    fn reset_binding_is_idempotent() {
        // Clearing an already-cleared window is a no-op, not a toggle.
        let mut win = crate::buffer_defs::WinT::default();

        unsafe { reset_binding(&mut win) };
        unsafe { reset_binding(&mut win) };

        assert_eq!(win.w_onebuf_opt.wo_scb, 0);
        assert_eq!(win.w_onebuf_opt.wo_crb, 0);
    }

    #[test]
    fn reset_binding_clears_each_independently() {
        // Only one binding set: the other must still end up cleared,
        // so a version clearing just one field would fail.
        let mut only_scb = crate::buffer_defs::WinT::default();
        only_scb.w_onebuf_opt.wo_scb = 1;
        unsafe { reset_binding(&mut only_scb) };
        assert_eq!(only_scb.w_onebuf_opt.wo_scb, 0);

        let mut only_crb = crate::buffer_defs::WinT::default();
        only_crb.w_onebuf_opt.wo_crb = 1;
        unsafe { reset_binding(&mut only_crb) };
        assert_eq!(only_crb.w_onebuf_opt.wo_crb, 0);
    }

    #[test]
    fn rgb_packs_channels() {
        assert_eq!(rgb_(0xff, 0x00, 0x00), 0x00ff_0000);
        assert_eq!(rgb_(0x00, 0xff, 0x00), 0x0000_ff00);
        assert_eq!(rgb_(0x00, 0x00, 0xff), 0x0000_00ff);
        assert_eq!(rgb_(0x12, 0x34, 0x56), 0x0012_3456);
    }

    #[test]
    fn ascii_case_predicates() {
        assert!(ascii_islower(b'a' as i32));
        assert!(!ascii_islower(b'A' as i32));
        assert!(ascii_isupper(b'A' as i32));
        assert!(ascii_isalpha(b'z' as i32));
        assert!(ascii_isalnum(b'9' as i32));
        assert!(ascii_isalnum(b'Z' as i32));
        assert!(!ascii_isalnum(b'_' as i32));
    }

    #[test]
    fn ascii_toupper_tolower() {
        assert_eq!(toupper_asc(b'a' as i32), b'A' as i32);
        assert_eq!(toupper_asc(b'A' as i32), b'A' as i32);
        assert_eq!(tolower_asc(b'A' as i32), b'a' as i32);
        assert_eq!(tolower_asc(b'a' as i32), b'a' as i32);
    }

    #[test]
    fn empty_pos_detects_zero_position() {
        assert!(empty_pos(&PosT::default()));
        assert!(!empty_pos(&PosT { lnum: 1, col: 0, coladd: 0 }));
    }
}
