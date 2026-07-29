//! Translated from `src/nvim/help.c` (tractable core only).
//!
//! `help.c` manages neovim's `:help` system - almost entirely dependent
//! on tag-file search (`find_tags`) and the Lua help core
//! (`nlua_call_typval("vim._core.help", ...)`), neither translated.
//!
//! Translated: [`check_help_lang`] and [`help_heuristic`] - both pure
//! functions needing only [`crate::macros_defs`]'s already-translated
//! ASCII character-class helpers.
//!
//! Deferred: everything else - `find_help_tags` (needs `find_tags`/the
//! Lua help core), `cleanup_help_tags`/`prepare_help_buffer`/
//! `get_local_additions` (need help-buffer setup and option mutation),
//! `ex_help`/`ex_helpclose`/`ex_helptags`/`helptags_one`/`do_helptags`/
//! `helptags_cb` (the whole help/tagfile pipeline), `help_compare`
//! (a `qsort()` comparator wrapping [`help_heuristic`] - trivial once
//! this crate has real sortable tag-match data to apply it to).

use crate::macros_defs::{ascii_isalnum, ascii_isalpha};

/// Looks for a language specifier in the form `"@xx"` at the end of
/// `arg`. Returns `Some((rest, lang))` where `rest` is `arg` with the
/// `"@xx"` suffix removed and `lang` is the 2-letter language code, or
/// `None` if `arg` doesn't end with one (`check_help_lang`).
///
/// The original mutates `arg` in place (overwriting the `'@'` with a
/// NUL) and returns a pointer into the same buffer, past that NUL, for
/// the language code. Since nothing here needs the two resulting C
/// strings to share the same backing allocation, this returns two
/// borrowed sub-slices of the original (immutable) `arg` instead - the
/// same information, without needing an in-place mutation.
#[must_use]
pub fn check_help_lang(arg: &[u8]) -> Option<(&[u8], &[u8])> {
    let len = arg.len();
    if len >= 3
        && arg[len - 3] == b'@'
        && ascii_isalpha(i32::from(arg[len - 2]))
        && ascii_isalpha(i32::from(arg[len - 1]))
    {
        return Some((&arg[..len - 3], &arg[len - 2..]));
    }
    None
}

/// Returns a heuristic indicating how well `matched_string` matches a
/// help search - the smaller the number, the better the match
/// (`help_heuristic`). Priority order, best to worst: fewer
/// alphanumeric characters, fewer total characters, an earlier match
/// position, and a match NOT starting with `"+"` (a feature name rather
/// than the command/subject itself).
///
/// `offset` is the position within `matched_string` where the match
/// begins; `wrong_case` is whether the match required ignoring case.
///
/// The original indexes `matched_string[offset]`/`[offset - 1]`
/// assuming `offset` is always a valid in-bounds match position (true
/// for every real caller, none of which are translated yet); this
/// additionally guards `offset` against `matched_string`'s own length
/// first, so an out-of-range `offset` safely falls through to the
/// `offset > 2` branch instead of panicking.
#[must_use]
pub fn help_heuristic(matched_string: &[u8], mut offset: i32, wrong_case: bool) -> i32 {
    let num_letters =
        matched_string.iter().filter(|&&c| ascii_isalnum(i32::from(c))).count() as i32;

    // Multiply the number of letters by 100 to give it a much bigger
    // weighting than the number of characters.
    // If there only is a match while ignoring case, add 5000.
    // If the match starts in the middle of a word, add 10000 to put it
    // somewhere in the last half.
    // If the match is more than 2 chars from the start, multiply by 200
    // to put it after matches at the start.
    if offset > 0
        && (offset as usize) < matched_string.len()
        && ascii_isalnum(i32::from(matched_string[offset as usize]))
        && ascii_isalnum(i32::from(matched_string[offset as usize - 1]))
    {
        offset += 10000;
    } else if offset > 2 {
        offset *= 200;
    }
    if wrong_case {
        offset += 5000;
    }
    // Features are less interesting than the subjects themselves, but
    // "+" alone is not a feature.
    if matched_string.first() == Some(&b'+') && matched_string.get(1).is_some_and(|&c| c != 0) {
        offset += 100;
    }
    100 * num_letters + matched_string.len() as i32 + offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_help_lang_finds_a_language_suffix() {
        assert_eq!(check_help_lang(b"gets@en"), Some((b"gets".as_slice(), b"en".as_slice())));
    }

    #[test]
    fn check_help_lang_no_suffix_is_none() {
        assert_eq!(check_help_lang(b"gets"), None);
    }

    #[test]
    fn check_help_lang_rejects_non_alpha_suffix() {
        // "@12" isn't alphabetic, so it's not a language suffix.
        assert_eq!(check_help_lang(b"gets@12"), None);
    }

    #[test]
    fn check_help_lang_too_short_is_none() {
        assert_eq!(check_help_lang(b"@e"), None);
        assert_eq!(check_help_lang(b""), None);
    }

    #[test]
    fn help_heuristic_more_letters_scores_worse() {
        let short = help_heuristic(b"cat", 0, false);
        let long = help_heuristic(b"category", 0, false);
        assert!(long > short);
    }

    #[test]
    fn help_heuristic_wrong_case_adds_5000() {
        let right = help_heuristic(b"cat", 0, false);
        let wrong = help_heuristic(b"cat", 0, true);
        assert_eq!(wrong - right, 5000);
    }

    #[test]
    fn help_heuristic_mid_word_match_adds_10000() {
        // offset=1 lands between two alnum characters in "cats".
        let mid = help_heuristic(b"cats", 1, false);
        let start = help_heuristic(b"cats", 0, false);
        assert_eq!(mid - start, 10000 + 1 /* the offset difference itself */);
    }

    #[test]
    fn help_heuristic_leading_plus_with_more_adds_100() {
        let with_plus = help_heuristic(b"+feature", 0, false);
        let without_plus = help_heuristic(b"feature", 0, false);
        // "+feature": 7 alnum letters, len 8, offset 0, +100 penalty ->
        // 700 + 8 + 100 = 808. "feature": 7 letters, len 7, no penalty
        // -> 700 + 7 + 0 = 707. Difference is 101 (100 penalty + 1 for
        // the extra '+' character's own length contribution).
        assert_eq!(with_plus, 808);
        assert_eq!(without_plus, 707);
        assert_eq!(with_plus - without_plus, 101);
    }

    #[test]
    fn help_heuristic_lone_plus_is_not_penalized() {
        // "+" alone (matched_string[1] is the end of the slice) should
        // NOT get the feature penalty: 0 letters ('+' isn't alnum), len
        // 1, offset 0, no penalty -> 100*0 + 1 + 0 = 1.
        let lone_plus = help_heuristic(b"+", 0, false);
        assert_eq!(lone_plus, 1);
    }

    #[test]
    fn help_heuristic_out_of_bounds_offset_is_safe() {
        // Must not panic; falls through to the `offset > 2` branch.
        let result = help_heuristic(b"ab", 10, false);
        assert_eq!(result, 100 * 2 + 2 + (10 * 200));
    }
}
