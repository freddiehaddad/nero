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
//! already-real `getdigits_int`/`ascii_isdigit`. `did_set_spellsuggest`
//! (`optionstr.rs`) is now this function's real caller. `spell_suggest`
//! itself (the only reader of `sps_flags`/`sps_limit`) remains not
//! translated.
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
//! Also translated: `bytes2offset` - the exact inverse of
//! `crate::spellfile::offset2bytes` (already real), a pure,
//! self-contained byte-decoding algorithm with no dependency on the
//! `slang_T` machinery at all. No real caller yet either (deep inside
//! the same untranslated spell-suggestion engine) - harvested ahead of
//! it for the same reason as `spell_check_sps` above.
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

/// One spelling suggestion (`suggest_T`).
#[allow(dead_code)]
#[derive(Debug, Default)]
struct SuggestT {
    /// Suggested word; `st_wordlen` is derived from this owned buffer.
    st_word: Vec<u8>,
    st_orglen: i32,
    st_score: i32,
    st_altscore: i32,
    st_salscore: bool,
    st_had_bonus: bool,
    /// Opaque pointer to the spelling language used for sound folding.
    st_slang: *mut std::ffi::c_void,
}

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

/// Decode a byte-offset value encoded by
/// [`crate::spellfile::offset2bytes`] (`bytes2offset`), the opposite
/// of that function. Returns the decoded offset and the number of
/// bytes consumed (`(nr, consumed)`), in place of the original's own
/// `char **pp` advancing-pointer out-parameter, matching this crate's
/// established C-out-parameter-to-owned-return convention.
///
/// No real caller is translated yet (its own real caller, deep inside
/// `spellsuggest.c`'s substantial, untranslated real spell-suggestion
/// engine, needs `slang_T` spell-language data) - harvested ahead of
/// it, matching this crate's established precedent for a small,
/// self-contained function with no design freedom of its own.
#[must_use]
pub fn bytes2offset(p: &[u8]) -> (i32, usize) {
    let mut i = 0usize;
    let c = i32::from(p[i]);
    i += 1;
    let mut nr;
    if c & 0x80 == 0x00 {
        // 1 byte
        nr = c - 1;
    } else if c & 0xc0 == 0x80 {
        // 2 bytes
        nr = (c & 0x3f) - 1;
        nr = nr * 255 + (i32::from(p[i]) - 1);
        i += 1;
    } else if c & 0xe0 == 0xc0 {
        // 3 bytes
        nr = (c & 0x1f) - 1;
        nr = nr * 255 + (i32::from(p[i]) - 1);
        i += 1;
        nr = nr * 255 + (i32::from(p[i]) - 1);
        i += 1;
    } else {
        // 4 bytes
        nr = (c & 0x0f) - 1;
        nr = nr * 255 + (i32::from(p[i]) - 1);
        i += 1;
        nr = nr * 255 + (i32::from(p[i]) - 1);
        i += 1;
        nr = nr * 255 + (i32::from(p[i]) - 1);
        i += 1;
    }
    (nr, i)
}

/// Mix of upper and lower case, e.g. `macaRONI` (`WF_MIXCAP`).
///
/// Defined locally in `spellsuggest.c` rather than in `spell_defs.h`
/// alongside the other `WF_*` flags, so it lives here rather than in
/// [`crate::spell`].
pub const WF_MIXCAP: i32 = 0x20;

/// Like [`crate::spell::captype`], but for a `WF_KEEPCAP` word also
/// add [`crate::spell::WF_ONECAP`] when the word starts with a
/// capital, so `make_case_word` can turn `WOrd` into `Word`
/// (`badword_captype`).
///
/// Also adds [`crate::spell::WF_ALLCAP`] for a word like `WOrD`, and
/// [`WF_MIXCAP`] when both cases appear at least twice.
///
/// `end` bounds the word. The original declares both arguments
/// non-null and always passes a real end pointer, so this takes a
/// plain `usize` rather than an `Option`; it is clamped to `word`'s
/// length so a too-large bound cannot read past the slice.
///
/// Note the counting loop walks EVERY character in the range, not
/// only word characters - unlike [`crate::spell::captype`], which
/// skips non-word characters. That asymmetry is the original's.
///
/// # Safety
/// Same as [`crate::spell::captype`]: `GLOBALS.curwin` must be valid
/// and non-null, as must its `w_s` syntax block.
#[must_use]
pub unsafe fn badword_captype(word: &[u8], end: usize) -> i32 {
    let end = end.min(word.len());
    // SAFETY: forwarded from this function's own safety doc.
    let mut flags = unsafe { crate::spell::captype(word, Some(end)) };

    if (flags & crate::spell::WF_KEEPCAP) == 0 {
        return flags;
    }

    // Count the number of UPPER and lower case letters.
    let mut l = 0i32;
    let mut u = 0i32;
    let mut first = false;
    let mut p = 0usize;
    while p < end {
        let c = crate::mbyte::utf_ptr2char(&word[p..]);
        // SAFETY: `spell_is_upper` only touches `OPTION_VARS`.
        if unsafe { crate::spell::spell_is_upper(c) } {
            u += 1;
            if p == 0 {
                first = true;
            }
        } else {
            l += 1;
        }
        // SAFETY: `p` is in bounds, since `end <= word.len()`.
        p += (unsafe { crate::mbyte::utfc_ptr2len(&word[p..]) }).max(1) as usize;
    }

    // If there are more UPPER than lower case letters suggest an
    // ALLCAP word. Otherwise, if the first letter is UPPER then
    // suggest ONECAP. Exception: "ALl" most likely should be "All",
    // require three upper case letters.
    if u > l && u > 2 {
        flags |= crate::spell::WF_ALLCAP;
    } else if first {
        flags |= crate::spell::WF_ONECAP;
    }

    if u >= 2 && l >= 2 {
        // maCARONI maCAroni
        flags |= WF_MIXCAP;
    }

    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestion_owns_word_scores_flags_and_language_identity() {
        let language = std::ptr::NonNull::<u8>::dangling().as_ptr().cast();
        let suggestion = SuggestT {
            st_word: b"spelling".to_vec(),
            st_orglen: 5,
            st_score: 10,
            st_altscore: 20,
            st_salscore: true,
            st_had_bonus: true,
            st_slang: language,
        };

        assert_eq!(suggestion.st_word.len(), 8);
        assert_eq!(suggestion.st_orglen, 5);
        assert_eq!(suggestion.st_score, 10);
        assert_eq!(suggestion.st_altscore, 20);
        assert!(suggestion.st_salscore);
        assert!(suggestion.st_had_bonus);
        assert_eq!(suggestion.st_slang, language);
    }
    use crate::globals::global_state_test_lock;

    // ---- badword_captype ----

    /// A window with an initialised spell table installed as `curwin`,
    /// so `captype` classifies ASCII letters as word characters.
    /// Without the table every byte is a non-word character and
    /// `captype` returns 0, which would make these tests vacuous.
    ///
    /// Owns both allocations as raw pointers rather than live `Box`
    /// bindings, since the code under test reads through
    /// `curwin`/`w_s`.
    struct BadwordFixture {
        win: *mut crate::buffer_defs::WinT,
        syn: *mut crate::buffer_defs::SynblockT,
        prev_curwin: *mut crate::buffer_defs::WinT,
    }

    impl BadwordFixture {
        fn new() -> Self {
            unsafe { crate::spell::init_spell_chartab() };
            let syn = Box::into_raw(Box::new(crate::buffer_defs::SynblockT::default()));
            let mut win = Box::new(crate::buffer_defs::WinT::default());
            win.w_s = syn;
            let win = Box::into_raw(win);

            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_curwin = g.curwin;
            g.curwin = win;
            Self { win, syn, prev_curwin }
        }
    }

    impl Drop for BadwordFixture {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.prev_curwin;
            unsafe {
                drop(Box::from_raw(self.win));
                drop(Box::from_raw(self.syn));
            }
        }
    }

    /// Words that aren't KEEPCAP are passed straight through, with
    /// none of the extra flags applied.
    #[test]
    fn badword_captype_passes_through_non_keepcap_words() {
        let _lock = global_state_test_lock();
        let _fx = BadwordFixture::new();

        for word in [b"word".as_slice(), b"Word".as_slice(), b"WORD".as_slice()] {
            let plain = unsafe { crate::spell::captype(word, Some(word.len())) };
            assert_eq!(
                unsafe { badword_captype(word, word.len()) },
                plain,
                "non-KEEPCAP word {word:?} must be unchanged"
            );
            assert_eq!(plain & crate::spell::WF_KEEPCAP, 0);
            assert_eq!(plain & WF_MIXCAP, 0);
        }
    }

    /// "WOrd" is KEEPCAP with 2 upper and 2 lower, so it gains ONECAP
    /// (first letter is capital, and 2 is not more than 2) and
    /// MIXCAP.
    #[test]
    fn badword_captype_adds_onecap_and_mixcap_for_wo_rd() {
        let _lock = global_state_test_lock();
        let _fx = BadwordFixture::new();

        let flags = unsafe { badword_captype(b"WOrd", 4) };
        assert_ne!(flags & crate::spell::WF_KEEPCAP, 0, "still KEEPCAP");
        assert_ne!(flags & crate::spell::WF_ONECAP, 0, "starts with a capital");
        assert_ne!(flags & WF_MIXCAP, 0, "2 upper and 2 lower");
        assert_eq!(flags & crate::spell::WF_ALLCAP, 0, "not more upper than lower");
    }

    /// "WOrD" has 3 upper and 1 lower, so upper wins on both counts
    /// and it gains ALLCAP rather than ONECAP. MIXCAP needs at least
    /// two of EACH, so a single lower letter does not earn it.
    #[test]
    fn badword_captype_adds_allcap_for_wo_r_d() {
        let _lock = global_state_test_lock();
        let _fx = BadwordFixture::new();

        let flags = unsafe { badword_captype(b"WOrD", 4) };
        assert_ne!(flags & crate::spell::WF_ALLCAP, 0, "3 upper vs 1 lower");
        assert_eq!(flags & crate::spell::WF_ONECAP, 0, "ALLCAP wins over ONECAP");
        assert_eq!(flags & WF_MIXCAP, 0, "only one lower letter");
    }

    /// The documented "ALl" exception: upper does outnumber lower,
    /// but three upper case letters are required for ALLCAP, so this
    /// gets ONECAP instead.
    #[test]
    fn badword_captype_requires_three_capitals_for_allcap() {
        let _lock = global_state_test_lock();
        let _fx = BadwordFixture::new();

        let flags = unsafe { badword_captype(b"ALl", 3) };
        assert_ne!(flags & crate::spell::WF_KEEPCAP, 0);
        assert_eq!(flags & crate::spell::WF_ALLCAP, 0, "only 2 capitals");
        assert_ne!(flags & crate::spell::WF_ONECAP, 0);
    }

    /// "maCARONI" is KEEPCAP but does NOT start with a capital, so it
    /// gets neither ONECAP nor (with 6 upper vs 2 lower) misses
    /// ALLCAP - upper outnumbers lower and exceeds two.
    #[test]
    fn badword_captype_adds_allcap_and_mixcap_for_macaroni() {
        let _lock = global_state_test_lock();
        let _fx = BadwordFixture::new();

        let flags = unsafe { badword_captype(b"maCARONI", 8) };
        assert_ne!(flags & crate::spell::WF_ALLCAP, 0, "6 upper vs 2 lower");
        assert_ne!(flags & WF_MIXCAP, 0, "at least 2 of each case");
        assert_eq!(flags & crate::spell::WF_ONECAP, 0, "does not start capital");
    }

    /// `end` bounds the word: the bytes past it take no part in the
    /// counting.
    #[test]
    fn badword_captype_uses_only_the_first_end_bytes() {
        let _lock = global_state_test_lock();
        let _fx = BadwordFixture::new();

        // Counting all of "WOrdXYZ" would reach 5 upper vs 2 lower and
        // earn ALLCAP; bounded to "WOrd" it must not.
        let bounded = unsafe { badword_captype(b"WOrdXYZ", 4) };
        let full = unsafe { badword_captype(b"WOrdXYZ", 7) };
        assert_eq!(bounded & crate::spell::WF_ALLCAP, 0);
        assert_ne!(full & crate::spell::WF_ALLCAP, 0);
    }

    #[test]
    fn wf_mixcap_matches_the_original() {
        assert_eq!(WF_MIXCAP, 0x20);
    }

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

    // --- bytes2offset ---

    #[test]
    fn bytes2offset_decodes_a_1_byte_value() {
        // offset2bytes(0) == [1] (hand-verified in spellfile.rs's own
        // tests).
        assert_eq!(bytes2offset(&[1]), (0, 1));
    }

    #[test]
    fn bytes2offset_decodes_a_2_byte_value() {
        // offset2bytes(127) is 2 bytes (spellfile.rs's own
        // `offset2bytes_crosses_into_the_2_byte_encoding` test):
        // b1 = 127 % 255 + 1 = 128 > 0x7f, so nr=127 needs 2 bytes.
        // b1 = 128, rem = 0, b2 = 0 % 255 + 1 = 1.
        // Encoded as [0x80 + 1, 128] = [0x81, 0x80].
        let (nr, consumed) = bytes2offset(&[0x81, 0x80]);
        assert_eq!(nr, 127);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn bytes2offset_only_consumes_its_own_bytes_leaving_the_rest() {
        // A 1-byte value followed by unrelated trailing data - only
        // the first byte should be consumed.
        let (nr, consumed) = bytes2offset(&[1, 0xff, 0xff]);
        assert_eq!(nr, 0);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn bytes2offset_round_trips_with_offset2bytes_across_every_encoding_width() {
        // The single strongest correctness check available: for a
        // wide spread of values (one hand-picked from each of
        // offset2bytes's own 1/2/3/4-byte branches, per its real
        // b1/b2/b3/b4 arithmetic), decoding what it encoded must
        // recover the exact original value and consume every byte it
        // produced.
        for nr in [
            0,          // 1 byte (see offset2bytes(0) == [1] above)
            100,        // 1 byte
            126,        // 1 byte (offset2bytes's own upper-bound test)
            127,        // 2 bytes (offset2bytes's own crossing-point test)
            254,
            255,
            1_000,
            65_535,     // crosses into the 3-byte range
            1_000_000,  // crosses into the 4-byte range
            100_000_000,
        ] {
            let encoded = crate::spellfile::offset2bytes(nr);
            let (decoded, consumed) = bytes2offset(&encoded);
            assert_eq!(decoded, nr, "round-trip mismatch for nr={nr}, encoded={encoded:?}");
            assert_eq!(
                consumed,
                encoded.len(),
                "bytes2offset should consume every byte offset2bytes produced for nr={nr}"
            );
        }
    }
}
