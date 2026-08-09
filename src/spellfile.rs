//! Translated from `src/nvim/spellfile.c` (tractable core only).
//!
//! `spellfile.c` (~5500 lines) reads/writes the binary `.spl`/`.sug`
//! spell-file formats and the text `.aff`/`.dic` affix/dictionary
//! source files - almost entirely dependent on the large,
//! not-yet-translated affix-tree/word-tree storage
//! (`spellinfo_T`/`afffile_T`/`hashtab_T`-based tables) this whole
//! subsystem builds up from.
//!
//! Translated: 4 small, self-contained, no-design-freedom `static`
//! helper functions, each with no dependency on the affix/word-tree
//! storage itself:
//!
//! - [`str_equal`] - a `NULL`-tolerant string-equality check (used by
//!   `aff_check_string`, not yet translated, to compare 2 possibly-
//!   absent `.aff` file header values across multiple parsed files).
//! - [`sal_to_bool`] - converts a `SAL` line's boolean argument
//!   (`"1"`/`"true"`) to a real `bool` (used by `spell_read_aff`, not
//!   yet translated).
//! - [`valid_spell_word`] - whether a word contains only valid word
//!   characters (control characters and a trailing `/` are invalid;
//!   space is OK), via the already-real `crate::mbyte::
//!   utf_valid_string`/`utfc_ptr2len`. Used by `store_word`/
//!   `spell_add_word` (both `word`/`len`-shaped, i.e. `word`'s own
//!   declared length never includes a trailing NUL byte in either
//!   real call site - verified directly against both, not assumed),
//!   neither yet translated. The original's own `*p != NUL` loop-stop
//!   condition (in addition to `p < end`) is not modeled separately:
//!   both real callers pass `len == strlen(word)`, so a NUL byte can
//!   only ever appear AT `word.len()`, never strictly before it,
//!   making that extra condition provably redundant for every
//!   realistic input - not a narrowing, just an unreachable branch
//!   correctly omitted (matching this crate's own established
//!   "translate the real early-return, not a hardcoded shortcut"
//!   precedent applied to the INVERSE case: omitting a check that can
//!   never fire).
//! - [`offset2bytes`] - converts an integer offset into a minimal,
//!   NUL-byte-avoiding multi-byte encoding (similar to
//!   `utf_char2bytes`, but using 8 bits in follow-up bytes). Returns
//!   the encoded bytes directly as an owned `Vec<u8>` instead of
//!   writing through a caller-provided buffer pointer and returning
//!   just the byte count. Used by `sug_write` (the `.sug`
//!   suggestion-file writer, not yet translated).
//! - [`spell_check_msm`] - parses `'mkspellmem'` into the 3 file-static
//!   tree-compression tuning knobs (`compress_start`/`compress_inc`/
//!   `compress_added`), via `charset.c`'s already-real
//!   `ascii_isdigit`/`getdigits_int`. `did_set_mkspellmem`
//!   (`optionstr.rs`) is its real caller; the actual compression
//!   algorithm reading these 3 knobs back out remains not translated.
//!
//! Deferred: everything else - the whole `.spl`/`.sug`/`.aff`/`.dic`
//! binary/text format read/write machinery, and the word/affix-tree
//! storage (`spellinfo_T`, `afffile_T`) it all builds on.

use crate::globals::GlobalCell;

/// Size of one memory block used for the word tree (`SBLOCKSIZE`).
const SBLOCKSIZE: i32 = 16000;

/// Tunable parameter for when the tree is compressed - memory /
/// [`SBLOCKSIZE`] (`compress_start`).
static COMPRESS_START: GlobalCell<i32> = GlobalCell::new(30000);
/// Tunable parameter for when the tree is compressed - memory /
/// [`SBLOCKSIZE`] (`compress_inc`).
static COMPRESS_INC: GlobalCell<i32> = GlobalCell::new(100);
/// Tunable parameter for when the tree is compressed - word count
/// (`compress_added`).
static COMPRESS_ADDED: GlobalCell<i32> = GlobalCell::new(500_000);

/// # Safety
/// Single-threaded test/editor state, matching every other
/// `GlobalCell`-backed file-static in this crate.
#[must_use]
pub unsafe fn compress_start() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPRESS_START.get_mut() }
}

/// # Safety
/// Same as [`compress_start`].
#[must_use]
pub unsafe fn compress_inc() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPRESS_INC.get_mut() }
}

/// # Safety
/// Same as [`compress_start`].
#[must_use]
pub unsafe fn compress_added() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPRESS_ADDED.get_mut() }
}

/// Whether `s1` and `s2` are equal - both being absent (`None`) also
/// counts as equal (`str_equal`).
///
/// Modeled with `Option<&[u8]>` instead of 2 raw, possibly-null
/// `char*` pointers - a real, faithful translation of the original's
/// own explicit `NULL`-tolerant contract (no `FUNC_ATTR_NONNULL_*` on
/// either parameter), matching this crate's established
/// `Option<&[u8]>`-for-a-genuinely-nullable-C-pointer convention.
#[must_use]
pub fn str_equal(s1: Option<&[u8]>, s2: Option<&[u8]>) -> bool {
    match (s1, s2) {
        (None, None) => true,
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Converts a boolean argument in a `SAL` line to `true` or `false`
/// (`sal_to_bool`).
#[must_use]
pub fn sal_to_bool(s: &[u8]) -> bool {
    s == b"1" || s == b"true"
}

/// Returns `true` if `word` contains valid word characters. Control
/// characters and a trailing `/` are invalid. Space is OK
/// (`valid_spell_word`). See this module's own doc comment for why
/// the original's own `*p != NUL` extra loop-stop condition is not
/// modeled separately.
#[must_use]
pub fn valid_spell_word(word: &[u8]) -> bool {
    if !crate::mbyte::utf_valid_string(word) {
        return false;
    }
    let mut p = 0;
    while p < word.len() {
        let c = word[p];
        if c < b' ' || (c == b'/' && p + 1 == word.len()) {
            return false;
        }
        // SAFETY: `word[p]` is confirmed non-NUL (both real callers
        // pass a `word`/`len` pair where a NUL can only ever appear
        // AT `word.len()`, never strictly before it - see this
        // module's own doc comment), so `utfc_ptr2len` always returns
        // >= 1 here (no infinite-loop risk from a zero advance);
        // `.max(1)` is a defensive net for any other, less-verified
        // future caller.
        let char_len = unsafe { crate::mbyte::utfc_ptr2len(&word[p..]) };
        p += char_len.max(1) as usize;
    }
    true
}

/// Convert an offset into a minimal number of bytes, avoiding NUL
/// bytes - similar to `utf_char2bytes`, but using 8 bits in follow-up
/// bytes (`offset2bytes`). Returns the encoded bytes directly as an
/// owned `Vec<u8>` instead of writing through a caller-provided
/// buffer pointer and separately returning just the byte count.
#[must_use]
pub fn offset2bytes(nr: i32) -> Vec<u8> {
    // Split the number in parts of base 255 - avoids NUL bytes.
    let b1 = nr % 255 + 1;
    let rem = nr / 255;
    let b2 = rem % 255 + 1;
    let rem = rem / 255;
    let b3 = rem % 255 + 1;
    let b4 = rem / 255 + 1;

    if b4 > 1 || b3 > 0x1f {
        // 4 bytes
        vec![(0xe0 + b4) as u8, b3 as u8, b2 as u8, b1 as u8]
    } else if b3 > 1 || b2 > 0x3f {
        // 3 bytes
        vec![(0xc0 + b3) as u8, b2 as u8, b1 as u8]
    } else if b2 > 1 || b1 > 0x7f {
        // 2 bytes
        vec![(0x80 + b2) as u8, b1 as u8]
    } else {
        // 1 byte
        vec![b1 as u8]
    }
}

/// Parse the `'mkspellmem'` option value ("start,inc,added", 3 comma-
/// separated digit runs) into `COMPRESS_START`/`COMPRESS_INC`/
/// `COMPRESS_ADDED` (`spell_check_msm`). Returns `FAIL` if the value
/// is malformed or fails a sanity check (any part is zero, or
/// `incr > start`) - leaving the 3 file-statics untouched in that
/// case, matching the original's own "only assign on the final,
/// fully-validated `OK` path" structure exactly.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` and this module's own
/// `COMPRESS_START`/`COMPRESS_INC`/`COMPRESS_ADDED` - no overlapping
/// live access, same as every other function touching either.
#[must_use]
pub unsafe fn spell_check_msm() -> i32 {
    use crate::vim_defs::{FAIL, OK};

    // SAFETY: forwarded from this function's own safety doc.
    let p_msm = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_msm.clone();
    let s: &[u8] = p_msm.as_deref().unwrap_or(&[]);
    let mut pos = 0usize;

    if !s.first().is_some_and(|&c| crate::ascii_defs::ascii_isdigit(i32::from(c))) {
        return FAIL;
    }
    let (val1, consumed) = crate::charset::getdigits_int(&s[pos..], true, 0);
    pos += consumed;
    let start = (val1 * 10) / (SBLOCKSIZE / 102);
    if s.get(pos) != Some(&b',') {
        return FAIL;
    }
    pos += 1;

    if !s.get(pos).is_some_and(|&c| crate::ascii_defs::ascii_isdigit(i32::from(c))) {
        return FAIL;
    }
    let (val2, consumed) = crate::charset::getdigits_int(&s[pos..], true, 0);
    pos += consumed;
    let incr = (val2 * 102) / (SBLOCKSIZE / 10);
    if s.get(pos) != Some(&b',') {
        return FAIL;
    }
    pos += 1;

    if !s.get(pos).is_some_and(|&c| crate::ascii_defs::ascii_isdigit(i32::from(c))) {
        return FAIL;
    }
    let (val3, consumed) = crate::charset::getdigits_int(&s[pos..], true, 0);
    pos += consumed;
    let added = val3 * 1024;
    if pos != s.len() {
        return FAIL;
    }

    if start == 0 || incr == 0 || added == 0 || incr > start {
        return FAIL;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        *COMPRESS_START.get_mut() = start;
        *COMPRESS_INC.get_mut() = incr;
        *COMPRESS_ADDED.get_mut() = added;
    }
    OK
}

/// Turn a multi-byte string into a wide character string
/// (`mb_str2wide`).
///
/// The result is NUL-terminated, matching the original's trailing
/// `res[i] = NUL`, so callers that scan for a terminator still work.
///
/// Note what the original does with combining characters, which is
/// preserved exactly: it reads the BASE character with
/// `utf_ptr2char` but advances by `utfc_ptr2len`, which spans any
/// combining characters that follow. Those are therefore consumed
/// without being emitted, so the result can be shorter than the
/// string's character count.
///
/// The original sizes its allocation up front from `mb_charlen`; a
/// growable `Vec` makes that unnecessary, which also sidesteps the
/// original's over-allocation whenever combining characters are
/// dropped.
///
/// # Safety
/// Touches `OPTION_VARS` via [`crate::mbyte::utfc_ptr2len`] - the
/// same requirement as every other function that does so.
#[must_use]
pub unsafe fn mb_str2wide(s: &[u8]) -> Vec<i32> {
    let mut res = Vec::new();
    let mut p = 0usize;
    while p < s.len() && s[p] != 0 {
        res.push(crate::mbyte::utf_ptr2char(&s[p..]));
        // SAFETY: forwarded from this function's own safety doc. The
        // `max(1)` keeps a malformed byte from stalling the scan.
        p += (unsafe { crate::mbyte::utfc_ptr2len(&s[p..]) }).max(1) as usize;
    }
    res.push(0);
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- mb_str2wide ---

    #[test]
    fn mb_str2wide_converts_ascii_and_nul_terminates() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { mb_str2wide(b"abc") }, vec![0x61, 0x62, 0x63, 0]);
    }

    /// An empty string yields just the terminator, not an empty
    /// vector.
    #[test]
    fn mb_str2wide_returns_only_the_terminator_for_an_empty_string() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { mb_str2wide(b"") }, vec![0]);
    }

    /// Conversion stops at an embedded NUL, matching the original's
    /// `*p != NUL` loop condition.
    #[test]
    fn mb_str2wide_stops_at_an_embedded_nul() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { mb_str2wide(b"ab\0cd") }, vec![0x61, 0x62, 0]);
    }

    /// Multi-byte characters become single code points, so the result
    /// is shorter than the byte length.
    #[test]
    fn mb_str2wide_decodes_multibyte_characters_to_code_points() {
        let _lock = crate::globals::global_state_test_lock();
        // "é" (U+00E9, 2 bytes) then "€" (U+20AC, 3 bytes).
        let s = "aé€".as_bytes();
        assert_eq!(s.len(), 6, "6 bytes but 3 characters");
        assert_eq!(unsafe { mb_str2wide(s) }, vec![0x61, 0xE9, 0x20AC, 0]);
    }

    /// A combining character is consumed by the advance but never
    /// emitted, since the original reads the base character with
    /// `utf_ptr2char` while advancing by `utfc_ptr2len`. Only the
    /// base "a" survives.
    #[test]
    fn mb_str2wide_drops_combining_characters() {
        let _lock = crate::globals::global_state_test_lock();
        // "a" followed by U+0301 COMBINING ACUTE ACCENT.
        let s = "a\u{0301}b".as_bytes();
        assert_eq!(s.len(), 4);
        assert_eq!(
            unsafe { mb_str2wide(s) },
            vec![0x61, 0x62, 0],
            "the combining accent is consumed, not emitted"
        );
    }

    // --- str_equal ---

    #[test]
    fn str_equal_both_none_is_equal() {
        assert!(str_equal(None, None));
    }

    #[test]
    fn str_equal_one_none_is_not_equal() {
        assert!(!str_equal(None, Some(b"x")));
        assert!(!str_equal(Some(b"x"), None));
    }

    #[test]
    fn str_equal_same_content_is_equal() {
        assert!(str_equal(Some(b"utf-8"), Some(b"utf-8")));
    }

    #[test]
    fn str_equal_different_content_is_not_equal() {
        assert!(!str_equal(Some(b"utf-8"), Some(b"latin1")));
    }

    // --- sal_to_bool ---

    #[test]
    fn sal_to_bool_recognizes_1_and_true() {
        assert!(sal_to_bool(b"1"));
        assert!(sal_to_bool(b"true"));
    }

    #[test]
    fn sal_to_bool_anything_else_is_false() {
        assert!(!sal_to_bool(b"0"));
        assert!(!sal_to_bool(b"false"));
        assert!(!sal_to_bool(b""));
        assert!(!sal_to_bool(b"True")); // case-sensitive, matching strcmp
    }

    // --- valid_spell_word ---

    #[test]
    fn valid_spell_word_plain_ascii_word() {
        assert!(valid_spell_word(b"hello"));
    }

    #[test]
    fn valid_spell_word_with_a_space_is_ok() {
        assert!(valid_spell_word(b"hello world"));
    }

    #[test]
    fn valid_spell_word_control_character_is_invalid() {
        assert!(!valid_spell_word(b"hel\x01lo"));
    }

    #[test]
    fn valid_spell_word_trailing_slash_is_invalid() {
        assert!(!valid_spell_word(b"hello/"));
    }

    #[test]
    fn valid_spell_word_embedded_slash_not_at_the_end_is_ok() {
        // Only a TRAILING slash (the very last byte) is rejected -
        // matching the original's own `p[0]=='/' && p[1]==NUL` check
        // precisely (not just "any slash").
        assert!(valid_spell_word(b"a/b"));
    }

    #[test]
    fn valid_spell_word_empty_is_valid() {
        assert!(valid_spell_word(b""));
    }

    #[test]
    fn valid_spell_word_invalid_utf8_is_invalid() {
        assert!(!valid_spell_word(&[0xff, 0xfe]));
    }

    #[test]
    fn valid_spell_word_multibyte_character_is_ok() {
        // "café" - a genuine multi-byte (2-byte 'é') word, no control
        // characters or trailing slash.
        assert!(valid_spell_word("café".as_bytes()));
    }

    // --- offset2bytes ---

    #[test]
    fn offset2bytes_small_value_is_one_byte() {
        // b1 = 0 % 255 + 1 = 1 <= 0x7f, so this is the 1-byte path.
        assert_eq!(offset2bytes(0), vec![1u8]);
    }

    #[test]
    fn offset2bytes_never_produces_a_nul_byte() {
        // Hand-picked values spanning every branch (1/2/3/4-byte).
        for nr in [0, 100, 200, 254, 255, 1000, 65_535, 1_000_000, 100_000_000] {
            let bytes = offset2bytes(nr);
            assert!(!bytes.is_empty());
            assert!(!bytes.contains(&0), "offset2bytes({nr}) produced a NUL byte: {bytes:?}");
        }
    }

    #[test]
    fn offset2bytes_crosses_into_the_2_byte_encoding() {
        // b1 = 0x7f + 1 = 0x80 > 0x7f, so nr=127 must be 2 bytes (the
        // 1-byte path's own upper limit is b1 <= 0x7f, i.e. nr <= 126).
        assert_eq!(offset2bytes(126).len(), 1);
        assert_eq!(offset2bytes(127).len(), 2);
    }

    // --- spell_check_msm ---

    fn set_p_msm(value: Option<&[u8]>) -> Option<Vec<u8>> {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_msm.clone();
        opts.p_msm = value.map(<[u8]>::to_vec);
        prev
    }

    fn reset_compress() -> (i32, i32, i32) {
        let prev = (
            unsafe { compress_start() },
            unsafe { compress_inc() },
            unsafe { compress_added() },
        );
        unsafe {
            *COMPRESS_START.get_mut() = 30000;
            *COMPRESS_INC.get_mut() = 100;
            *COMPRESS_ADDED.get_mut() = 500_000;
        }
        prev
    }

    fn restore_compress(prev: (i32, i32, i32)) {
        unsafe {
            *COMPRESS_START.get_mut() = prev.0;
            *COMPRESS_INC.get_mut() = prev.1;
            *COMPRESS_ADDED.get_mut() = prev.2;
        }
    }

    #[test]
    fn spell_check_msm_the_real_default_value_is_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_msm = set_p_msm(Some(b"460000,2000,500"));
        let prev_compress = reset_compress();

        assert_eq!(unsafe { spell_check_msm() }, crate::vim_defs::OK);
        // Hand-traced: start = (460000*10)/(16000/102) = 4600000/156 =
        // 29487; incr = (2000*102)/(16000/10) = 204000/1600 = 127;
        // added = 500*1024 = 512000.
        assert_eq!(unsafe { compress_start() }, 29487);
        assert_eq!(unsafe { compress_inc() }, 127);
        assert_eq!(unsafe { compress_added() }, 512_000);

        set_p_msm(prev_msm.as_deref());
        restore_compress(prev_compress);
    }

    #[test]
    fn spell_check_msm_missing_comma_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_msm = set_p_msm(Some(b"460000"));
        assert_eq!(unsafe { spell_check_msm() }, crate::vim_defs::FAIL);
        set_p_msm(prev_msm.as_deref());
    }

    #[test]
    fn spell_check_msm_non_digit_start_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_msm = set_p_msm(Some(b"-1,2,3"));
        assert_eq!(unsafe { spell_check_msm() }, crate::vim_defs::FAIL);
        set_p_msm(prev_msm.as_deref());
    }

    #[test]
    fn spell_check_msm_zero_start_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_msm = set_p_msm(Some(b"0,1,1"));
        assert_eq!(unsafe { spell_check_msm() }, crate::vim_defs::FAIL);
        set_p_msm(prev_msm.as_deref());
    }

    #[test]
    fn spell_check_msm_incr_greater_than_start_fails() {
        let _lock = crate::globals::global_state_test_lock();
        // start = (2000*10)/156 = 128; incr = (2100*102)/1600 = 133;
        // 133 > 128, so this must fail.
        let prev_msm = set_p_msm(Some(b"2000,2100,500"));
        assert_eq!(unsafe { spell_check_msm() }, crate::vim_defs::FAIL);
        set_p_msm(prev_msm.as_deref());
    }

    #[test]
    fn spell_check_msm_missing_third_number_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_msm = set_p_msm(Some(b"1,2,"));
        assert_eq!(unsafe { spell_check_msm() }, crate::vim_defs::FAIL);
        set_p_msm(prev_msm.as_deref());
    }

    #[test]
    fn spell_check_msm_trailing_garbage_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_msm = set_p_msm(Some(b"460000,2000,500x"));
        assert_eq!(unsafe { spell_check_msm() }, crate::vim_defs::FAIL);
        set_p_msm(prev_msm.as_deref());
    }

    #[test]
    fn spell_check_msm_failure_leaves_compress_values_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_msm = set_p_msm(Some(b"bogus"));
        let prev_compress = reset_compress();

        assert_eq!(unsafe { spell_check_msm() }, crate::vim_defs::FAIL);
        assert_eq!(unsafe { compress_start() }, 30000);
        assert_eq!(unsafe { compress_inc() }, 100);
        assert_eq!(unsafe { compress_added() }, 500_000);

        set_p_msm(prev_msm.as_deref());
        restore_compress(prev_compress);
    }
}
