//! Translated from `src/nvim/regexp.c` (tractable leaves only).
//!
//! The regular-expression compiler and executor are not translated.
//! This module starts with the independent replacement-text case
//! conversion helpers and compiled-program flag accessors.

/// Compiled program can match a newline (`RF_HASNL`).
const RF_HASNL: u32 = 4;
/// Whether the previous `vim_regcomp()` saw an end-of-line item
/// (`had_eol`).
static HAD_EOL: crate::globals::GlobalCell<bool> = crate::globals::GlobalCell::new(false);
/// Whether `'cpoptions'` contains the literal flag (`reg_cpo_lit`).
static REG_CPO_LIT: crate::globals::GlobalCell<bool> =
    crate::globals::GlobalCell::new(false);

/// Refresh regexp-specific `'cpoptions'` flags (`get_cpo_flags`).
///
/// # Safety
/// Reads `OPTION_VARS.p_cpo` and mutates `REG_CPO_LIT`.
#[allow(dead_code)]
unsafe fn get_cpo_flags() {
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { REG_CPO_LIT.get_mut() } = opts
        .p_cpo
        .as_deref()
        .is_some_and(|value| {
            crate::strings::vim_strchr(
                value,
                i32::from(crate::option_vars::CPO_LITERAL),
            )
            .is_some()
        });
}

/// Compare an NFA numeric position operand (`nfa_re_num_cmp`).
///
/// `op == 1` means position greater than value, `op == 2` means
/// position less than value, and every other value means equality.
#[allow(dead_code)]
#[must_use]
fn nfa_re_num_cmp(value: u64, op: i32, position: u64) -> bool {
    if op == 1 {
        position > value
    } else if op == 2 {
        position < value
    } else {
        value == position
    }
}

/// Whether the compiled program can match a line break
/// (`re_multiline`).
///
/// The original returns the flag bit itself (`0` or `RF_HASNL`), not a
/// normalized boolean.
#[must_use]
pub fn re_multiline(prog: &crate::types_defs::RegprogT) -> i32 {
    (prog.regflags & RF_HASNL) as i32
}

/// Whether the previous regexp compilation saw `$`
/// (`vim_regcomp_had_eol`).
///
/// # Safety
/// Must not run concurrently with regexp compilation.
#[must_use]
pub unsafe fn vim_regcomp_had_eol() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    i32::from(unsafe { *HAD_EOL.get_mut() })
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

    struct HadEolGuard(bool);

    struct CpoGuard {
        saved_option: Option<Vec<u8>>,
        saved_literal: bool,
    }

    impl CpoGuard {
        fn set(value: Option<Vec<u8>>) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let saved_option = std::mem::replace(&mut opts.p_cpo, value);
            let saved_literal = unsafe { *REG_CPO_LIT.get_mut() };
            Self { saved_option, saved_literal }
        }
    }

    impl Drop for CpoGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo =
                self.saved_option.take();
            unsafe { *REG_CPO_LIT.get_mut() = self.saved_literal };
        }
    }

    impl HadEolGuard {
        fn set(value: bool) -> Self {
            let saved = unsafe { *HAD_EOL.get_mut() };
            unsafe { *HAD_EOL.get_mut() = value };
            Self(saved)
        }
    }

    impl Drop for HadEolGuard {
        fn drop(&mut self) {
            unsafe { *HAD_EOL.get_mut() = self.0 };
        }
    }

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

    #[test]
    fn vim_regcomp_had_eol_reflects_the_last_compile_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = HadEolGuard::set(false);
        assert_eq!(unsafe { vim_regcomp_had_eol() }, 0);
        unsafe { *HAD_EOL.get_mut() = true };
        assert_eq!(unsafe { vim_regcomp_had_eol() }, 1);
    }

    #[test]
    fn get_cpo_flags_tracks_only_the_literal_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = CpoGuard::set(Some(b"aB".to_vec()));
        unsafe { get_cpo_flags() };
        assert!(!unsafe { *REG_CPO_LIT.get_mut() });

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo =
            Some(b"al".to_vec());
        unsafe { get_cpo_flags() };
        assert!(unsafe { *REG_CPO_LIT.get_mut() });

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo = None;
        unsafe { get_cpo_flags() };
        assert!(!unsafe { *REG_CPO_LIT.get_mut() });
    }

    #[test]
    fn nfa_re_num_cmp_handles_greater_less_and_equal_modes() {
        assert!(nfa_re_num_cmp(4, 1, 5));
        assert!(!nfa_re_num_cmp(5, 1, 4));
        assert!(nfa_re_num_cmp(5, 2, 4));
        assert!(!nfa_re_num_cmp(4, 2, 5));
        assert!(nfa_re_num_cmp(7, 0, 7));
        assert!(nfa_re_num_cmp(u64::MAX, 99, u64::MAX));
        assert!(!nfa_re_num_cmp(7, 0, 8));
    }
}
