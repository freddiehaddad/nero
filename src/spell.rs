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
//!
//! Also translated: [`SpelltabT`]/[`clear_spell_chartab`]/
//! [`init_spell_chartab`] (from `spell_defs.h`/`spell.c` - no
//! dedicated `spell_defs.rs` module exists yet, matching
//! `charset.h`'s own "embedded directly, documented" precedent for a
//! header with no other translated members). Needed only already-real
//! `mbyte.c`'s `utf_fold`/`mb_toupper`/`mb_isupper`/`mb_islower` -
//! translated ahead of their own real caller (`spell_iswordp`/
//! `spell_iswordp_nmw`, needing `mb_get_class`/`utf_class`'s own real
//! `curbuf->b_chartab`, still not translated - the SAME `g_chartab`
//! blocker documented throughout this crate's `charset.rs`), matching
//! this crate's established "small, self-contained, no design freedom
//! to get wrong" precedent.

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

/// The tables used for recognizing word characters according to
/// spelling. These are only used for the first 256 characters of
/// `'encoding'` (`spelltab_T`, `spell_defs.h`).
pub struct SpelltabT {
    /// flags: is word char (`st_isw`)
    pub st_isw: [bool; 256],
    /// flags: is uppercase char (`st_isu`)
    pub st_isu: [bool; 256],
    /// chars: folded case (`st_fold`)
    pub st_fold: [u8; 256],
    /// chars: upper case (`st_upper`)
    pub st_upper: [u8; 256],
}

impl SpelltabT {
    /// A brand-new, all-zeroed table (matching the original's own
    /// static/zero-initialized `spelltab_T spelltab;` - every field
    /// starts `false`/`0` until [`clear_spell_chartab`]/
    /// [`init_spell_chartab`] populates it for real).
    #[must_use]
    pub const fn new() -> Self {
        SpelltabT { st_isw: [false; 256], st_isu: [false; 256], st_fold: [0; 256], st_upper: [0; 256] }
    }
}

impl Default for SpelltabT {
    fn default() -> Self {
        Self::new()
    }
}

/// The crate-wide `spelltab_T spelltab` global instance, matching
/// this crate's established `GlobalCell`-backed file-static
/// convention. Const-constructible (every field is a plain array),
/// so no `LazyLock` wrapper is needed.
static SPELLTAB: crate::globals::GlobalCell<SpelltabT> =
    crate::globals::GlobalCell::new(SpelltabT::new());

/// Whether a spell file has customized the chartab
/// (`did_set_spelltab`, `spell.h`'s own real cross-file `extern bool`,
/// also read/written by `spellfile.c`, not translated, so this is
/// currently only ever written by [`init_spell_chartab`] itself).
static DID_SET_SPELLTAB: crate::globals::GlobalCell<bool> = crate::globals::GlobalCell::new(false);

/// Resets `sp` to its baseline ASCII-only word-character table
/// (`clear_spell_chartab`): digits/letters are word characters,
/// `A`-`Z`/`a`-`z` fold to their opposite case, everything else
/// starts as "not a word character" with an identity fold/upper
/// mapping.
pub fn clear_spell_chartab(sp: &mut SpelltabT) {
    sp.st_isw = [false; 256];
    sp.st_isu = [false; 256];

    for i in 0..256usize {
        sp.st_fold[i] = i as u8;
        sp.st_upper[i] = i as u8;
    }

    for i in b'0'..=b'9' {
        sp.st_isw[i as usize] = true;
    }
    for i in b'A'..=b'Z' {
        sp.st_isw[i as usize] = true;
        sp.st_isu[i as usize] = true;
        sp.st_fold[i as usize] = i + 0x20;
    }
    for i in b'a'..=b'z' {
        sp.st_isw[i as usize] = true;
        sp.st_upper[i as usize] = i - 0x20;
    }
}

/// Initializes the chartab used for spelling (`init_spell_chartab`).
/// Called once while starting up. The default is ASCII-only (via
/// [`clear_spell_chartab`]); for bytes 128..256 (`'encoding'`-
/// dependent), uses the real case-folding/upper-casing functions.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via `mbyte::mb_toupper`/
/// `mb_isupper`/`mb_islower`).
pub unsafe fn init_spell_chartab() {
    unsafe {
        *DID_SET_SPELLTAB.get_mut() = false;
    }
    // SAFETY: a plain exclusive borrow of this file's own static.
    let sp = unsafe { SPELLTAB.get_mut() };
    clear_spell_chartab(sp);
    for i in 128i32..256 {
        let f = crate::mbyte::utf_fold(i);
        // SAFETY: forwarded from this function's own safety doc.
        let u = unsafe { crate::mbyte::mb_toupper(i) };
        // SAFETY: forwarded from this function's own safety doc.
        let isu = unsafe { crate::mbyte::mb_isupper(i) };
        // SAFETY: forwarded from this function's own safety doc.
        let isl = unsafe { crate::mbyte::mb_islower(i) };
        sp.st_isu[i as usize] = isu;
        sp.st_isw[i as usize] = isu || isl;
        sp.st_fold[i as usize] = if f < 256 { f as u8 } else { i as u8 };
        sp.st_upper[i as usize] = if u < 256 { u as u8 } else { i as u8 };
    }
}

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

    // --- clear_spell_chartab / init_spell_chartab ---

    #[test]
    fn clear_spell_chartab_digits_are_word_chars() {
        let mut sp = SpelltabT::default();
        clear_spell_chartab(&mut sp);
        assert!(sp.st_isw[b'5' as usize]);
        assert!(!sp.st_isw[b' ' as usize]);
    }

    #[test]
    fn clear_spell_chartab_uppercase_letters_are_word_and_upper() {
        let mut sp = SpelltabT::default();
        clear_spell_chartab(&mut sp);
        assert!(sp.st_isw[b'A' as usize]);
        assert!(sp.st_isu[b'A' as usize]);
        assert_eq!(sp.st_fold[b'A' as usize], b'a');
    }

    #[test]
    fn clear_spell_chartab_lowercase_letters_are_word_not_upper() {
        let mut sp = SpelltabT::default();
        clear_spell_chartab(&mut sp);
        assert!(sp.st_isw[b'a' as usize]);
        assert!(!sp.st_isu[b'a' as usize]);
        assert_eq!(sp.st_upper[b'a' as usize], b'A');
    }

    #[test]
    fn clear_spell_chartab_non_letter_gets_identity_fold_and_upper() {
        let mut sp = SpelltabT::default();
        clear_spell_chartab(&mut sp);
        assert!(!sp.st_isw[b' ' as usize]);
        assert_eq!(sp.st_fold[b' ' as usize], b' ');
        assert_eq!(sp.st_upper[b' ' as usize], b' ');
    }

    #[test]
    fn clear_spell_chartab_resets_a_previously_dirty_table() {
        // A table that was previously populated by init_spell_chartab
        // (or hand-corrupted) is fully reset, not just "added to".
        let mut sp = SpelltabT { st_isw: [true; 256], ..SpelltabT::default() };
        clear_spell_chartab(&mut sp);
        assert!(!sp.st_isw[b'!' as usize]);
    }

    #[test]
    fn init_spell_chartab_resets_did_set_spelltab_and_matches_clear_for_ascii() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *DID_SET_SPELLTAB.get_mut() = true };

        unsafe { init_spell_chartab() };

        assert!(!unsafe { *DID_SET_SPELLTAB.get_mut() });
        // Bytes 0..128 are untouched by init_spell_chartab's own loop
        // (which only runs 128..256) - they must still match a plain
        // clear_spell_chartab call exactly.
        let sp = unsafe { SPELLTAB.get_mut() };
        let mut expected = SpelltabT::default();
        clear_spell_chartab(&mut expected);
        assert_eq!(&sp.st_isw[0..128], &expected.st_isw[0..128]);
        assert_eq!(&sp.st_fold[0..128], &expected.st_fold[0..128]);
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
