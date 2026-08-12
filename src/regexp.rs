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
/// NFA matching deadline (`nfa_time_limit`); null disables it.
static NFA_TIME_LIMIT: crate::globals::GlobalCell<*mut crate::types_defs::ProftimeT> =
    crate::globals::GlobalCell::new(std::ptr::null_mut());
/// Optional caller-owned timeout result (`nfa_timed_out`).
static NFA_TIMED_OUT: crate::globals::GlobalCell<*mut bool> =
    crate::globals::GlobalCell::new(std::ptr::null_mut());

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

/// Allocate an empty external-submatch block with one reference
/// (`make_extmatch`).
#[allow(dead_code)]
fn make_extmatch() -> *mut crate::types_defs::RegExtmatchT {
    Box::into_raw(Box::new(crate::types_defs::RegExtmatchT {
        refcnt: 1,
        ..Default::default()
    }))
}

/// Write a four-byte big-endian regexp operand (`re_put_uint32`).
///
/// Returns the offset immediately after the written value, replacing
/// the original's advanced pointer.
#[allow(dead_code)]
fn re_put_uint32(dst: &mut [u8], value: u32) -> usize {
    dst[..4].copy_from_slice(&value.to_be_bytes());
    4
}

/// Add a reference to an external-submatch block (`ref_extmatch`).
///
/// # Safety
/// A non-null `em` must point to a live `RegExtmatchT`.
pub unsafe fn ref_extmatch(
    em: *mut crate::types_defs::RegExtmatchT,
) -> *mut crate::types_defs::RegExtmatchT {
    if !em.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*em).refcnt = (*em).refcnt.wrapping_add(1) };
    }
    em
}

/// Remove a reference and free the external-submatch block at zero
/// (`unref_extmatch`).
///
/// Rust drops every owned capture string when the block is freed,
/// replacing the original's explicit ten-element `xfree` loop.
///
/// # Safety
/// A non-null `em` must point to a live `RegExtmatchT` allocated with
/// `Box::into_raw`, and each logical reference may be released once.
pub unsafe fn unref_extmatch(em: *mut crate::types_defs::RegExtmatchT) {
    if em.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*em).refcnt = (*em).refcnt.wrapping_sub(1) };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*em).refcnt } <= 0 {
        // SAFETY: forwarded from this function's own safety doc.
        drop(unsafe { Box::from_raw(em) });
    }
}

/// Whether NFA matching exceeded its configured deadline
/// (`nfa_did_time_out`).
///
/// # Safety
/// Non-null timeout pointers must remain live for the call.
#[allow(dead_code)]
#[must_use]
unsafe fn nfa_did_time_out() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let limit = unsafe { *NFA_TIME_LIMIT.get_mut() };
    if !limit.is_null()
        // SAFETY: forwarded from this function's own safety doc.
        && crate::profile::profile_passed_limit(unsafe { *limit })
    {
        // SAFETY: forwarded from this function's own safety doc.
        let timed_out = unsafe { *NFA_TIMED_OUT.get_mut() };
        if !timed_out.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { *timed_out = true };
        }
        return true;
    }
    false
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

    struct NfaTimeoutGuard {
        limit: *mut crate::types_defs::ProftimeT,
        timed_out: *mut bool,
    }

    impl NfaTimeoutGuard {
        fn install(
            limit: *mut crate::types_defs::ProftimeT,
            timed_out: *mut bool,
        ) -> Self {
            let saved = Self {
                limit: unsafe { *NFA_TIME_LIMIT.get_mut() },
                timed_out: unsafe { *NFA_TIMED_OUT.get_mut() },
            };
            unsafe {
                *NFA_TIME_LIMIT.get_mut() = limit;
                *NFA_TIMED_OUT.get_mut() = timed_out;
            }
            saved
        }
    }

    impl Drop for NfaTimeoutGuard {
        fn drop(&mut self) {
            unsafe {
                *NFA_TIME_LIMIT.get_mut() = self.limit;
                *NFA_TIMED_OUT.get_mut() = self.timed_out;
            }
        }
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

    #[test]
    fn nfa_did_time_out_sets_the_optional_result_only_after_deadline() {
        let _lock = crate::globals::global_state_test_lock();
        let mut timed_out = false;
        let timed_out_ptr = std::ptr::addr_of_mut!(timed_out);
        let mut future = crate::profile::profile_setlimit(10_000);
        let future_ptr = std::ptr::addr_of_mut!(future);
        let _guard = NfaTimeoutGuard::install(future_ptr, timed_out_ptr);
        assert!(!unsafe { nfa_did_time_out() });
        assert!(!unsafe { *timed_out_ptr });

        let mut past = 1;
        let past_ptr = std::ptr::addr_of_mut!(past);
        unsafe { *NFA_TIME_LIMIT.get_mut() = past_ptr };
        assert!(unsafe { nfa_did_time_out() });
        assert!(unsafe { *timed_out_ptr });

        unsafe { *NFA_TIME_LIMIT.get_mut() = std::ptr::null_mut() };
        unsafe { *timed_out_ptr = false };
        assert!(!unsafe { nfa_did_time_out() });
        assert!(!unsafe { *timed_out_ptr });
    }

    #[test]
    fn make_extmatch_starts_with_one_reference_and_empty_captures() {
        let ext = make_extmatch();
        assert_eq!(unsafe { (*ext).refcnt }, 1);
        assert!(unsafe { &(*ext).matches }.iter().all(Option::is_none));
        // Constructor ownership is still held only by this test.
        drop(unsafe { Box::from_raw(ext) });
    }

    #[test]
    fn ref_extmatch_is_null_safe_and_increments_a_live_block() {
        assert!(unsafe { ref_extmatch(std::ptr::null_mut()) }.is_null());

        let ext = make_extmatch();
        assert_eq!(unsafe { ref_extmatch(ext) }, ext);
        assert_eq!(unsafe { (*ext).refcnt }, 2);
        // The final unref helper lands separately; this test still owns
        // the allocation directly.
        drop(unsafe { Box::from_raw(ext) });
    }

    #[test]
    fn re_put_uint32_writes_big_endian_and_reports_the_next_offset() {
        let mut bytes = [0xaa; 6];
        assert_eq!(re_put_uint32(&mut bytes, 0x1234_abcd), 4);
        assert_eq!(bytes, [0x12, 0x34, 0xab, 0xcd, 0xaa, 0xaa]);

        assert_eq!(re_put_uint32(&mut bytes, u32::MAX), 4);
        assert_eq!(&bytes[..4], &[0xff; 4]);
    }

    #[test]
    fn unref_extmatch_decrements_then_frees_at_zero() {
        unsafe { unref_extmatch(std::ptr::null_mut()) };

        let ext = make_extmatch();
        unsafe { (*ext).matches[2] = Some(b"capture".to_vec()) };
        unsafe { ref_extmatch(ext) };
        unsafe { unref_extmatch(ext) };
        assert_eq!(unsafe { (*ext).refcnt }, 1);
        assert_eq!(unsafe { (*ext).matches[2].as_deref() }, Some(b"capture".as_slice()));

        // Drops the block and all owned captures. Miri exercises this
        // path to catch double-free/use-after-free mistakes.
        unsafe { unref_extmatch(ext) };
    }
}
