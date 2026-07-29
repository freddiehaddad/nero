//! Translated from `src/nvim/spell.c` (tractable core only).
//!
//! `spell.c` (~3700 lines) implements neovim's spell-checking engine -
//! almost entirely dependent on the spell-file/word-tree/suggestion
//! machinery (`slang_T`, `matchinf_T`, etc.), none of which is
//! translated.
//!
//! Translated: [`byte_in_str`] (a pure `strchr`-equivalent),
//! [`spell_valid_case`] (a pure bitflag check over a small subset of
//! the original's `WF_*` word-flag bits - only the 4 needed here are
//! transcribed, not the whole enum), and [`find_region`] (a pure
//! lookup over a 2-byte-per-entry region-code string).
//!
//! Deferred: everything else - `get_char_type`/`match_checkcompoundpattern`/
//! `can_compound`/`match_compoundrule`/`valid_word_prefix`/`captype`/
//! `spell_iswordp*`/`spell_casefold`/`check_need_cap`/`expand_spelling`/
//! `valid_spelllang`/`valid_spellfile`, all needing `slang_T`'s own
//! spell-file-loaded state or the buffer/window spell-option plumbing.

/// word has one capital (or all capitals) (`WF_ONECAP`).
pub const WF_ONECAP: i32 = 0x02;
/// word must be all capitals (`WF_ALLCAP`).
pub const WF_ALLCAP: i32 = 0x04;
/// keep-case word, all-cap not allowed (`WF_FIXCAP`).
pub const WF_FIXCAP: i32 = 0x40;
/// keep-case word (`WF_KEEPCAP`).
pub const WF_KEEPCAP: i32 = 0x80;

/// word valid in all regions (`REGION_ALL`).
pub const REGION_ALL: i32 = 0xff;

/// Whether byte value `n` appears anywhere in `s` (`byte_in_str`).
///
/// Like `strchr`, but independent of locale, per the original's own
/// doc comment.
#[must_use]
pub fn byte_in_str(s: &[u8], n: i32) -> bool {
    s.iter().any(|&b| i32::from(b) == n)
}

/// Checks case flags for a word - `true` if the word (`wordflags`) has
/// the case required by its spell-tree entry (`treeflags`)
/// (`spell_valid_case`).
#[must_use]
pub fn spell_valid_case(wordflags: i32, treeflags: i32) -> bool {
    (wordflags == WF_ALLCAP && (treeflags & WF_FIXCAP) == 0)
        || ((treeflags & (WF_ALLCAP | WF_KEEPCAP)) == 0
            && ((treeflags & WF_ONECAP) == 0 || (wordflags & WF_ONECAP) != 0))
}

/// Finds `region` (exactly 2 bytes) within `rp` (a sequence of 2-byte
/// region codes), returning its 0-based index (counting in 2-byte
/// units) if found, or [`REGION_ALL`] if not (`find_region`).
#[must_use]
pub fn find_region(rp: &[u8], region: [u8; 2]) -> i32 {
    let mut i = 0;
    loop {
        if i >= rp.len() {
            return REGION_ALL;
        }
        if rp[i] == region[0] && rp.get(i + 1) == Some(&region[1]) {
            return (i / 2) as i32;
        }
        i += 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_in_str_finds_present_byte() {
        assert!(byte_in_str(b"hello", i32::from(b'e')));
        assert!(byte_in_str(b"hello", i32::from(b'o')));
    }

    #[test]
    fn byte_in_str_missing_byte_is_false() {
        assert!(!byte_in_str(b"hello", i32::from(b'z')));
        assert!(!byte_in_str(b"", i32::from(b'a')));
    }

    #[test]
    fn spell_valid_case_allcap_word_matches_unless_fixcap() {
        assert!(spell_valid_case(WF_ALLCAP, 0));
        // WF_FIXCAP alone in treeflags doesn't block a match here: the
        // first disjunct fails (fixcap is set), but the second disjunct
        // is satisfied anyway since treeflags has neither ALLCAP nor
        // KEEPCAP nor ONECAP set. FIXCAP only actually blocks a match
        // when treeflags ALSO has ALLCAP or KEEPCAP set (verified by
        // hand-deriving the real bitwise formula after this assertion
        // first failed unexpectedly).
        assert!(spell_valid_case(WF_ALLCAP, WF_FIXCAP));
        assert!(!spell_valid_case(WF_ALLCAP, WF_FIXCAP | WF_ALLCAP));
    }

    #[test]
    fn spell_valid_case_plain_word_matches_plain_tree_entry() {
        assert!(spell_valid_case(0, 0));
    }

    #[test]
    fn spell_valid_case_onecap_word_matches_onecap_tree_entry() {
        assert!(spell_valid_case(WF_ONECAP, WF_ONECAP));
        // A plain (lowercase) word does NOT match a ONECAP-required entry.
        assert!(!spell_valid_case(0, WF_ONECAP));
    }

    #[test]
    fn spell_valid_case_rejects_allcap_or_keepcap_tree_entries_otherwise() {
        assert!(!spell_valid_case(0, WF_ALLCAP));
        assert!(!spell_valid_case(0, WF_KEEPCAP));
    }

    #[test]
    fn find_region_finds_exact_match() {
        assert_eq!(find_region(b"usgbde", *b"us"), 0);
        assert_eq!(find_region(b"usgbde", *b"gb"), 1);
        assert_eq!(find_region(b"usgbde", *b"de"), 2);
    }

    #[test]
    fn find_region_not_found_is_region_all() {
        assert_eq!(find_region(b"usgbde", *b"xx"), REGION_ALL);
        assert_eq!(find_region(b"", *b"us"), REGION_ALL);
    }
}
