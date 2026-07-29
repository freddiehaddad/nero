//! Translated from `src/nvim/cmdexpand.c` (tractable core only).
//!
//! `cmdexpand.c` (~4000 lines) implements neovim's command-line
//! completion (`<Tab>` completion, wildmenu) - almost entirely
//! dependent on the completion-context/expansion machinery
//! (`ExpandOne`/ `nextwild`/the `expand_T` state machine), none of
//! which is translated.
//!
//! Translated: [`cmdline_fuzzy_complete`] and [`sort_func_compare`] -
//! both pure functions needing only already-translated option fields
//! (`option_vars.rs`) or plain byte-string comparison.
//!
//! Deferred: everything else - `nextwild`/`copy_substring_from_pos`/
//! `is_regex_match`/`concat_pattern_with_buffer_match`/
//! `expand_pattern_in_buf` (the completion/search machinery),
//! `wildescape`/`ExpandEscape` (need `vim_strsave_fnameescape`/
//! `escape_fname`/`tilde_replace`, not translated).

/// Whether fuzzy completion for cmdline completion is enabled AND
/// `fuzzystr` is not empty - an empty search pattern should never use
/// fuzzy matching (`cmdline_fuzzy_complete`).
#[must_use]
pub fn cmdline_fuzzy_complete(fuzzystr: &[u8]) -> bool {
    let wop_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.wop_flags;
    (wop_flags & crate::option_vars::opt_wop_flag::FUZZY) != 0 && !fuzzystr.is_empty()
}

/// Comparator for sorting cmdline completion matches: `<SNR>`-style
/// (or any other `<`-prefixed) names sort to the end; otherwise, a
/// plain lexicographic byte comparison (`sort_func_compare`).
///
/// Returns a negative/zero/positive `i32`, matching `strcmp`'s own
/// convention - this crate's established comparator-function
/// translation shape (e.g. `path::path_fnamencmp`).
#[must_use]
pub fn sort_func_compare(s1: &[u8], s2: &[u8]) -> i32 {
    let p1_is_bracketed = s1.first() == Some(&b'<');
    let p2_is_bracketed = s2.first() == Some(&b'<');
    if !p1_is_bracketed && p2_is_bracketed {
        return -1;
    }
    if p1_is_bracketed && !p2_is_bracketed {
        return 1;
    }
    match s1.cmp(s2) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_wop_fuzzy(enabled: bool) -> u32 {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let old = opts.wop_flags;
        opts.wop_flags = if enabled { crate::option_vars::opt_wop_flag::FUZZY } else { 0 };
        old
    }

    #[test]
    fn cmdline_fuzzy_complete_false_when_disabled() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_wop_fuzzy(false);
        assert!(!cmdline_fuzzy_complete(b"foo"));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.wop_flags = old;
    }

    #[test]
    fn cmdline_fuzzy_complete_true_when_enabled_and_nonempty() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_wop_fuzzy(true);
        assert!(cmdline_fuzzy_complete(b"foo"));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.wop_flags = old;
    }

    #[test]
    fn cmdline_fuzzy_complete_false_when_pattern_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_wop_fuzzy(true);
        assert!(!cmdline_fuzzy_complete(b""));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.wop_flags = old;
    }

    #[test]
    fn sort_func_compare_bracketed_names_sort_last() {
        assert!(sort_func_compare(b"foo", b"<SNR>1_bar") < 0);
        assert!(sort_func_compare(b"<SNR>1_bar", b"foo") > 0);
    }

    #[test]
    fn sort_func_compare_both_bracketed_uses_strcmp() {
        assert_eq!(sort_func_compare(b"<SNR>1_a", b"<SNR>1_a"), 0);
        assert!(sort_func_compare(b"<SNR>1_a", b"<SNR>1_b") < 0);
    }

    #[test]
    fn sort_func_compare_neither_bracketed_uses_strcmp() {
        assert_eq!(sort_func_compare(b"abc", b"abc"), 0);
        assert!(sort_func_compare(b"abc", b"abd") < 0);
        assert!(sort_func_compare(b"abd", b"abc") > 0);
    }
}
