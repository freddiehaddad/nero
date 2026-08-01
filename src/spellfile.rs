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
//!
//! Deferred: everything else - the whole `.spl`/`.sug`/`.aff`/`.dic`
//! binary/text format read/write machinery, and the word/affix-tree
//! storage (`spellinfo_T`, `afffile_T`) it all builds on.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
