//! Translated from `src/nvim/spellsuggest.c` (tractable core only).
//!
//! `spellsuggest.c` (~3600 lines) is the real spell-suggestion engine
//! (`"z="`, `spellsuggest()`) - almost every function needs the
//! `slang_T` spell-language-data structures (soundfolding tables,
//! affix data) and/or the Lua `vim.ui.select()` picker, neither
//! translated.
//!
//! Translated: `spell_check_sps` (parse the `'spellsuggest'` option
//! string into the `sps_flags`/`sps_limit` file-statics), via
//! `option.c`'s already-real `copy_option_part` and `charset.c`'s
//! already-real `getdigits_int`/`ascii_isdigit`. No real caller yet
//! (`spell_suggest`, the only reader of `sps_flags`/`sps_limit`, and
//! `did_set_spellsuggest`, its only OTHER real caller, are both not
//! translated) - translated ahead of them anyway, matching this
//! crate's established "translate a small, simple, mechanically-
//! correct piece ahead of the surrounding engine" precedent.
//!
//! Deliberately restructured to compute into LOCAL `new_flags`/
//! `new_limit` values, only committing them to the shared
//! `SPS_FLAGS`/`SPS_LIMIT` statics on success (or resetting them to
//! their safe defaults on failure) - the original mutates the REAL
//! globals directly inside its own parsing loop, but ALWAYS resets
//! both to the same safe defaults on any failure path regardless of
//! what an earlier, successfully-parsed part may have set them to -
//! so this is a provably observably-identical simplification, not a
//! behavior change.
//!
//! Deferred: everything else in the file.

use crate::ascii_defs::ascii_isdigit;
use crate::charset::getdigits_int;
use crate::globals::GlobalCell;
use crate::option::copy_option_part;
use crate::os::os_defs::MAXPATHL;
use crate::vim_defs::{FAIL, OK};

/// Values for `sps_flags`.
pub const SPS_BEST: i32 = 1;
/// Values for `sps_flags`.
pub const SPS_FAST: i32 = 2;
/// Values for `sps_flags`.
pub const SPS_DOUBLE: i32 = 4;

/// Flags from `'spellsuggest'` (`sps_flags`).
static SPS_FLAGS: GlobalCell<i32> = GlobalCell::new(SPS_BEST);
/// Max number of suggestions given, from `'spellsuggest'`
/// (`sps_limit`).
static SPS_LIMIT: GlobalCell<i32> = GlobalCell::new(9999);

/// # Safety
/// Single-threaded test/editor state, matching every other
/// `GlobalCell`-backed file-static in this crate.
#[must_use]
pub unsafe fn sps_flags() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *SPS_FLAGS.get_mut() }
}

/// # Safety
/// Same as [`sps_flags`].
#[must_use]
pub unsafe fn sps_limit() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *SPS_LIMIT.get_mut() }
}

/// Check the `'spellsuggest'` option. Returns `FAIL` if it's wrong,
/// setting `sps_flags`/`sps_limit` (`spell_check_sps`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` and this module's own
/// `SPS_FLAGS`/`SPS_LIMIT` - no overlapping live access, same as
/// every other function touching either.
#[must_use]
pub unsafe fn spell_check_sps() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let p_sps = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_sps
        .clone()
        .unwrap_or_default();

    let mut new_flags = 0;
    let mut new_limit = 9999;
    let mut pos = 0;

    while pos < p_sps.len() {
        let (buf, next_pos) = copy_option_part(&p_sps, pos, MAXPATHL as usize, b",");
        pos = next_pos;

        let mut f = 0;
        if buf.first().is_some_and(|&c| ascii_isdigit(i32::from(c))) {
            let (limit, consumed) = getdigits_int(&buf, true, 0);
            new_limit = limit;
            if buf.get(consumed).is_some_and(|&c| !ascii_isdigit(i32::from(c))) {
                f = -1;
            }
            // Note: Keep this in sync with opt_sps_values.
        } else if buf == b"best" {
            f = SPS_BEST;
        } else if buf == b"fast" {
            f = SPS_FAST;
        } else if buf == b"double" {
            f = SPS_DOUBLE;
        } else if !buf.starts_with(b"expr:")
            && !buf.starts_with(b"file:")
            && (!buf.starts_with(b"timeout:")
                || (!buf.get(8).is_some_and(|&c| ascii_isdigit(i32::from(c)))
                    && !(buf.get(8) == Some(&b'-')
                        && buf.get(9).is_some_and(|&c| ascii_isdigit(i32::from(c))))))
        {
            f = -1;
        }

        if f == -1 || (new_flags != 0 && f != 0) {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                *SPS_FLAGS.get_mut() = SPS_BEST;
                *SPS_LIMIT.get_mut() = 9999;
            }
            return FAIL;
        }
        if f != 0 {
            new_flags = f;
        }
    }

    if new_flags == 0 {
        new_flags = SPS_BEST;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        *SPS_FLAGS.get_mut() = new_flags;
        *SPS_LIMIT.get_mut() = new_limit;
    }
    OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    fn set_p_sps(value: &[u8]) {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sps = Some(value.to_vec());
    }

    #[test]
    fn empty_option_defaults_to_best_and_9999() {
        let _lock = global_state_test_lock();
        set_p_sps(b"");
        assert_eq!(unsafe { spell_check_sps() }, OK);
        assert_eq!(unsafe { sps_flags() }, SPS_BEST);
        assert_eq!(unsafe { sps_limit() }, 9999);
    }

    #[test]
    fn plain_best() {
        let _lock = global_state_test_lock();
        set_p_sps(b"best");
        assert_eq!(unsafe { spell_check_sps() }, OK);
        assert_eq!(unsafe { sps_flags() }, SPS_BEST);
    }

    #[test]
    fn plain_fast() {
        let _lock = global_state_test_lock();
        set_p_sps(b"fast");
        assert_eq!(unsafe { spell_check_sps() }, OK);
        assert_eq!(unsafe { sps_flags() }, SPS_FAST);
    }

    #[test]
    fn plain_double() {
        let _lock = global_state_test_lock();
        set_p_sps(b"double");
        assert_eq!(unsafe { spell_check_sps() }, OK);
        assert_eq!(unsafe { sps_flags() }, SPS_DOUBLE);
    }

    #[test]
    fn a_bare_number_sets_the_limit_and_keeps_best() {
        let _lock = global_state_test_lock();
        set_p_sps(b"42");
        assert_eq!(unsafe { spell_check_sps() }, OK);
        assert_eq!(unsafe { sps_limit() }, 42);
        assert_eq!(unsafe { sps_flags() }, SPS_BEST);
    }

    #[test]
    fn number_and_flag_combined_via_comma() {
        let _lock = global_state_test_lock();
        set_p_sps(b"double,10");
        assert_eq!(unsafe { spell_check_sps() }, OK);
        assert_eq!(unsafe { sps_flags() }, SPS_DOUBLE);
        assert_eq!(unsafe { sps_limit() }, 10);
    }

    #[test]
    fn expr_prefix_is_accepted_without_setting_a_flag() {
        let _lock = global_state_test_lock();
        set_p_sps(b"expr:MySuggest()");
        assert_eq!(unsafe { spell_check_sps() }, OK);
        // No SPS_* flag was set by "expr:...", so it falls through to
        // the SPS_BEST default.
        assert_eq!(unsafe { sps_flags() }, SPS_BEST);
    }

    #[test]
    fn file_prefix_is_accepted() {
        let _lock = global_state_test_lock();
        set_p_sps(b"file:/tmp/suggestions");
        assert_eq!(unsafe { spell_check_sps() }, OK);
    }

    #[test]
    fn timeout_with_digits_is_accepted() {
        let _lock = global_state_test_lock();
        set_p_sps(b"timeout:123");
        assert_eq!(unsafe { spell_check_sps() }, OK);
    }

    #[test]
    fn timeout_with_negative_number_is_accepted() {
        let _lock = global_state_test_lock();
        set_p_sps(b"timeout:-5");
        assert_eq!(unsafe { spell_check_sps() }, OK);
    }

    #[test]
    fn timeout_without_a_number_fails() {
        let _lock = global_state_test_lock();
        set_p_sps(b"timeout:abc");
        assert_eq!(unsafe { spell_check_sps() }, FAIL);
        // Reset to safe defaults on failure.
        assert_eq!(unsafe { sps_flags() }, SPS_BEST);
        assert_eq!(unsafe { sps_limit() }, 9999);
    }

    #[test]
    fn an_unrecognized_word_fails() {
        let _lock = global_state_test_lock();
        set_p_sps(b"nonsense");
        assert_eq!(unsafe { spell_check_sps() }, FAIL);
    }

    #[test]
    fn a_number_with_trailing_garbage_fails() {
        let _lock = global_state_test_lock();
        set_p_sps(b"12x");
        assert_eq!(unsafe { spell_check_sps() }, FAIL);
    }

    #[test]
    fn two_conflicting_flags_fail() {
        let _lock = global_state_test_lock();
        set_p_sps(b"best,fast");
        assert_eq!(unsafe { spell_check_sps() }, FAIL);
    }

    #[test]
    fn a_failure_resets_a_previously_successful_limit() {
        // Verifies the "compute locally, commit only on success"
        // simplification is observably identical to the original's
        // own "mutate directly, then reset on failure" behavior: even
        // though "10" alone would successfully set the limit to 10,
        // the later ",bogus" part fails the WHOLE parse, so the
        // final state must be the safe defaults, not 10.
        let _lock = global_state_test_lock();
        set_p_sps(b"10,bogus");
        assert_eq!(unsafe { spell_check_sps() }, FAIL);
        assert_eq!(unsafe { sps_limit() }, 9999);
        assert_eq!(unsafe { sps_flags() }, SPS_BEST);
    }
}
