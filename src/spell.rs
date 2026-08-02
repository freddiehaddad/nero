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
//! transcribed, not the whole enum), [`find_region`] (a pure lookup
//! over a 2-byte-per-entry region-code string), and
//! [`valid_spelllang`]/[`valid_spellfile`] (re-investigated after
//! being stale-grouped below with functions genuinely needing
//! `slang_T`'s own spell-file-loaded state - these two option-value
//! validators actually need only already-existing
//! `crate::option::{valid_name, copy_option_part}`/
//! `crate::charset::vim_is_fname_char`). [`spell_check_window`]/
//! [`no_spell_checking`] (whether spell checking is enabled/usable
//! for a window - needed only already-real `WinT.w_onebuf_opt.
//! wo_spell`/`w_s`'s own `b_p_spl`/`b_langp` fields; the latter's own
//! real `emsg` display is skipped, matching this crate's established
//! policy).
//!
//! Deferred: everything else - `get_char_type`/`match_checkcompoundpattern`/
//! `can_compound`/`match_compoundrule`/`valid_word_prefix`/`captype`/
//! `spell_iswordp*`/`spell_casefold`/`check_need_cap`/`expand_spelling`,
//! all needing `slang_T`'s own spell-file-loaded state or the
//! buffer/window spell-option plumbing.

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

/// Whether `val` is a valid `'spelllang'` value (`valid_spelllang`).
#[must_use]
pub fn valid_spelllang(val: &[u8]) -> bool {
    crate::option::valid_name(val, b".-_,@")
}

/// Whether `val` is a valid `'spellfile'` value (`valid_spellfile`):
/// every comma-separated part must be a `.add`-suffixed name made up
/// entirely of valid filename characters.
#[must_use]
pub fn valid_spellfile(val: &[u8]) -> bool {
    let maxpathl = crate::os::os_defs::MAXPATHL as usize;
    let mut p = 0;
    while p < val.len() {
        let (spf_name, next_p) = crate::option::copy_option_part(val, p, maxpathl, b",");
        p = next_p;
        let l = spf_name.len();
        if l >= maxpathl - 4 || l < 4 || spf_name[l - 4..] != *b".add" {
            return false;
        }
        if !spf_name.iter().all(|&c| crate::charset::vim_is_fname_char(i32::from(c))) {
            return false;
        }
    }
    true
}

/// Whether spell checking is enabled for `wp` (`spell_check_window`):
/// `'spell'` is set AND a `'spelllang'` value is loaded.
///
/// # Safety
/// `wp.w_s` must be a valid, non-null pointer to a live
/// `crate::buffer_defs::SynblockT`.
#[must_use]
pub unsafe fn spell_check_window(wp: &crate::buffer_defs::WinT) -> bool {
    wp.w_onebuf_opt.wo_spell != 0
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { &*wp.w_s }.b_p_spl.as_deref().is_some_and(|v| !v.is_empty())
}

/// Whether spell checking is disabled (or no spell language is
/// actually loaded) for `wp` (`no_spell_checking`). The original's own
/// `emsg(_(e_no_spell))` display is skipped (`message.c`'s display
/// pipeline is not tractable), matching this crate's established
/// policy elsewhere - only the boolean OUTCOME is preserved.
///
/// # Safety
/// Same as [`spell_check_window`].
#[must_use]
pub unsafe fn no_spell_checking(wp: &crate::buffer_defs::WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { spell_check_window(wp) } || unsafe { &*wp.w_s }.b_langp.is_empty() {
        return true;
    }
    false
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

    // --- valid_spelllang ---

    #[test]
    fn valid_spelllang_empty_is_valid() {
        assert!(valid_spelllang(b""));
    }

    #[test]
    fn valid_spelllang_plain_alnum_is_valid() {
        assert!(valid_spelllang(b"en"));
    }

    #[test]
    fn valid_spelllang_allowed_punctuation_is_valid() {
        assert!(valid_spelllang(b"en_US.utf-8,de@quot"));
    }

    #[test]
    fn valid_spelllang_space_is_invalid() {
        assert!(!valid_spelllang(b"en US"));
    }

    // --- valid_spellfile ---

    #[test]
    fn valid_spellfile_empty_is_valid() {
        assert!(valid_spellfile(b""));
    }

    #[test]
    fn valid_spellfile_single_add_suffixed_part_is_valid() {
        assert!(valid_spellfile(b"en.utf-8.add"));
    }

    #[test]
    fn valid_spellfile_multiple_comma_separated_parts_all_valid() {
        assert!(valid_spellfile(b"en.add,de.add"));
    }

    #[test]
    fn valid_spellfile_bare_dot_add_is_valid() {
        // l == 4 exactly: not "< 4" (rejected), the whole part IS the
        // ".add" suffix with nothing before it - matches the
        // original's own l < 4 (strictly-less) check precisely.
        assert!(valid_spellfile(b".add"));
    }

    #[test]
    fn valid_spellfile_too_short_is_invalid() {
        assert!(!valid_spellfile(b"abc"));
    }

    #[test]
    fn valid_spellfile_wrong_suffix_is_invalid() {
        assert!(!valid_spellfile(b"foo.txt"));
    }

    #[test]
    fn valid_spellfile_one_bad_part_among_several_invalidates_the_whole_value() {
        assert!(!valid_spellfile(b"en.add,invalid"));
    }

    #[test]
    fn valid_spellfile_invalid_filename_character_is_rejected() {
        assert!(!valid_spellfile(b"a<b.add"));
    }

    // --- spell_check_window / no_spell_checking ---

    #[test]
    fn spell_check_window_true_when_spell_on_and_lang_set() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 1;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: Some(b"en".to_vec()), ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(unsafe { spell_check_window(&win) });
    }

    #[test]
    fn spell_check_window_false_when_spell_option_off() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 0;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: Some(b"en".to_vec()), ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(!unsafe { spell_check_window(&win) });
    }

    #[test]
    fn spell_check_window_false_when_spelllang_is_unset() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 1;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: None, ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(!unsafe { spell_check_window(&win) });
    }

    #[test]
    fn spell_check_window_false_when_spelllang_is_empty_string() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 1;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: Some(Vec::new()), ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(!unsafe { spell_check_window(&win) });
    }

    #[test]
    fn no_spell_checking_false_when_enabled_and_a_language_is_loaded() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 1;
        let mut syn = crate::buffer_defs::SynblockT {
            b_p_spl: Some(b"en".to_vec()),
            b_langp: crate::garray_defs::GarrayT { ga_len: 1, ..Default::default() },
            ..Default::default()
        };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(!unsafe { no_spell_checking(&win) });
    }

    #[test]
    fn no_spell_checking_true_when_spell_checking_itself_is_disabled() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 0;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: Some(b"en".to_vec()), ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(unsafe { no_spell_checking(&win) });
    }

    #[test]
    fn no_spell_checking_true_when_no_language_is_actually_loaded() {
        // spell_check_window is true (spell on, a 'spelllang' value is
        // set), but b_langp (the ACTUALLY LOADED languages) is empty -
        // matching the real-world "spelllang set but the file hasn't
        // loaded yet/failed to load" case.
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 1;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: Some(b"en".to_vec()), ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(unsafe { no_spell_checking(&win) });
    }
}
