//! Translated from `src/nvim/regexp.c` (tractable leaves only).
//!
//! The regular-expression compiler and executor are not translated.
//! This module starts with the independent replacement-text case
//! conversion helpers and compiled-program flag accessors.

/// Compiled program can match a newline (`RF_HASNL`).
const RF_HASNL: u32 = 4;

/// Whether the compiled program can match a line break
/// (`re_multiline`).
///
/// The original returns the flag bit itself (`0` or `RF_HASNL`), not a
/// normalized boolean.
#[must_use]
pub fn re_multiline(prog: &crate::types_defs::RegprogT) -> i32 {
    (prog.regflags & RF_HASNL) as i32
}

/// Uppercase one replacement character (`do_upper`).
///
/// # Safety
/// Forwarded from [`crate::mbyte::mb_toupper`].
#[allow(dead_code)]
unsafe fn do_upper(dst: &mut i32, c: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    *dst = unsafe { crate::mbyte::mb_toupper(c) };
}

/// Lowercase one replacement character (`do_lower`).
///
/// # Safety
/// Forwarded from [`crate::mbyte::mb_tolower`].
#[allow(dead_code)]
unsafe fn do_lower(dst: &mut i32, c: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    *dst = unsafe { crate::mbyte::mb_tolower(c) };
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CasemapGuard(u32);

    impl CasemapGuard {
        fn keep_ascii() -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let saved = opts.cmp_flags;
            opts.cmp_flags |= crate::option_vars::opt_cmp_flag::KEEPASCII;
            Self(saved)
        }
    }

    impl Drop for CasemapGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cmp_flags = self.0;
        }
    }

    #[test]
    fn do_upper_writes_the_ascii_uppercase_character() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = CasemapGuard::keep_ascii();
        let mut dst = 0;
        unsafe { do_upper(&mut dst, i32::from(b'a')) };
        assert_eq!(dst, i32::from(b'A'));

        unsafe { do_upper(&mut dst, i32::from(b'!')) };
        assert_eq!(dst, i32::from(b'!'));
    }

    #[test]
    fn do_lower_writes_the_ascii_lowercase_character() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = CasemapGuard::keep_ascii();
        let mut dst = 0;
        unsafe { do_lower(&mut dst, i32::from(b'Z')) };
        assert_eq!(dst, i32::from(b'z'));

        unsafe { do_lower(&mut dst, i32::from(b'?')) };
        assert_eq!(dst, i32::from(b'?'));
    }

    #[test]
    fn re_multiline_returns_the_newline_flag_bit_itself() {
        let mut prog = crate::types_defs::RegprogT::default();
        assert_eq!(re_multiline(&prog), 0);

        prog.regflags = RF_HASNL;
        assert_eq!(re_multiline(&prog), RF_HASNL as i32);

        prog.regflags = RF_HASNL | 0x80;
        assert_eq!(re_multiline(&prog), RF_HASNL as i32);
    }
}
