//! Translated from `src/nvim/help.c` (tractable core only).
//!
//! `help.c` manages neovim's `:help` system - almost entirely dependent
//! on tag-file search (`find_tags`) and the Lua help core
//! (`nlua_call_typval("vim._core.help", ...)`), neither translated.
//!
//! Translated: [`check_help_lang`], [`help_heuristic`], and
//! [`help_compare`] - pure parsing/ranking helpers needing only
//! already-translated ASCII primitives.
//!
//! Deferred: everything else - `find_help_tags` (needs `find_tags`/the
//! Lua help core), `cleanup_help_tags`/`prepare_help_buffer`/
//! `get_local_additions` (need help-buffer setup and option mutation),
//! `ex_help`/`ex_helpclose`/`ex_helptags`/`helptags_one`/`do_helptags`/
//! `helptags_cb` (the whole help/tagfile pipeline).

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

/// Compare encoded help-tag matches (`help_compare`).
///
/// Each value is `{tagname}\0{six-digit heuristic}\0`. The heuristic
/// sorts first, with the tag name as a deterministic tie-breaker.
#[must_use]
pub fn help_compare(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    fn parts(value: &[u8]) -> (&[u8], &[u8]) {
        let split = value
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(value.len());
        let name = &value[..split];
        let score_start = split.saturating_add(1).min(value.len());
        let score_rest = &value[score_start..];
        let score_end = score_rest
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(score_rest.len());
        (name, &score_rest[..score_end])
    }

    let (left_name, left_score) = parts(left);
    let (right_name, right_score) = parts(right);
    left_score
        .cmp(right_score)
        .then_with(|| left_name.cmp(right_name))
}

/// Strips redundant `"@xx"` language suffixes from help tag matches
/// (`cleanup_help_tags`).
///
/// Two suffixes are removed. A tag ending in `"@en"` loses it when no
/// other match is the same tag in a different language, since English
/// is then the only option. Separately, the language at the head of
/// `'helplang'` is stripped, because that is the language the user
/// already asked for - unless it is English, which the first rule
/// covers.
///
/// The original truncates each name in place by writing a NUL over
/// the `'@'`; the names are owned `Vec`s here, so they are truncated
/// instead. `num_file` is dropped, since the slice carries its own
/// length.
pub fn cleanup_help_tags(file: &mut [Vec<u8>], helplang: Option<&[u8]>) {
    // The preferred language's suffix, e.g. b"@de". English is left
    // out: the "@en" pass below already handles it, and stripping it
    // here would hide a tag that only exists in English.
    let preferred: Option<[u8; 3]> = match helplang {
        Some(hlg) if hlg.len() >= 2 && !(hlg[0] == b'e' && hlg[1] == b'n') => {
            Some([b'@', hlg[0], hlg[1]])
        }
        _ => None,
    };

    for i in 0..file.len() {
        let Some(len) = file[i].len().checked_sub(3).filter(|&l| l > 0) else {
            continue;
        };
        if &file[i][len..] != b"@en" {
            continue;
        }
        // Sorting is by priority, so the same tag in another language
        // can be anywhere; every entry has to be checked.
        let has_other_language = (0..file.len()).any(|j| {
            j != i && file[j].len() == len + 3 && file[j][..=len] == file[i][..=len]
        });
        if !has_other_language {
            file[i].truncate(len);
        }
    }

    let Some(preferred) = preferred else {
        return;
    };
    for name in file.iter_mut() {
        let Some(len) = name.len().checked_sub(3).filter(|&l| l > 0) else {
            continue;
        };
        if name[len..] == preferred {
            name.truncate(len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- cleanup_help_tags ----

    fn tags(v: &[&[u8]]) -> Vec<Vec<u8>> {
        v.iter().map(|s| s.to_vec()).collect()
    }

    /// A tag that only exists in English loses its "@en": there is no
    /// other language to disambiguate it from.
    #[test]
    fn cleanup_help_tags_strips_a_lone_en_suffix() {
        let mut file = tags(&[b"gets@en"]);
        cleanup_help_tags(&mut file, None);
        assert_eq!(file, tags(&[b"gets"]));
    }

    /// But when the same tag also exists in another language the
    /// "@en" has to stay, otherwise the two become indistinguishable.
    #[test]
    fn cleanup_help_tags_keeps_en_when_another_language_has_the_tag() {
        let mut file = tags(&[b"gets@en", b"gets@de"]);
        cleanup_help_tags(&mut file, None);
        assert_eq!(file, tags(&[b"gets@en", b"gets@de"]), "both must survive");
    }

    /// Matches are ordered by priority, not grouped by tag, so the
    /// other-language entry may sit anywhere in the list.
    #[test]
    fn cleanup_help_tags_searches_the_whole_list_for_another_language() {
        let mut file = tags(&[b"gets@en", b"other", b"unrelated@de", b"gets@de"]);
        cleanup_help_tags(&mut file, None);
        assert_eq!(file[0], b"gets@en".to_vec(), "the @de entry is last but still found");
    }

    /// A same-length entry that differs before the "@" is a different
    /// tag, so it must not count as a translation.
    #[test]
    fn cleanup_help_tags_does_not_match_a_different_tag_of_the_same_length() {
        let mut file = tags(&[b"gets@en", b"gots@de"]);
        cleanup_help_tags(&mut file, None);
        assert_eq!(file[0], b"gets".to_vec(), "gots is a different tag");
    }

    /// The language the user asked for is redundant on the tag name.
    #[test]
    fn cleanup_help_tags_strips_the_preferred_language() {
        let mut file = tags(&[b"gets@de", b"other@fr"]);
        cleanup_help_tags(&mut file, Some(b"de"));
        assert_eq!(file, tags(&[b"gets", b"other@fr"]), "only the preferred one goes");
    }

    /// 'helplang' of "en" adds nothing: the "@en" rule already covers
    /// English, and stripping it here would hide an English-only tag
    /// that has a translation.
    #[test]
    fn cleanup_help_tags_treats_helplang_en_as_no_preference() {
        let mut file = tags(&[b"gets@en", b"gets@de"]);
        cleanup_help_tags(&mut file, Some(b"en"));
        assert_eq!(file, tags(&[b"gets@en", b"gets@de"]));
    }

    /// Only the first entry of 'helplang' is used, and names too short
    /// to carry a suffix are left alone.
    #[test]
    fn cleanup_help_tags_uses_only_the_first_helplang_entry() {
        let mut file = tags(&[b"gets@de", b"gets2@fr", b"ab"]);
        cleanup_help_tags(&mut file, Some(b"de,fr"));
        assert_eq!(file, tags(&[b"gets", b"gets2@fr", b"ab"]));
    }

    #[test]
    fn cleanup_help_tags_handles_an_empty_list() {
        let mut file: Vec<Vec<u8>> = Vec::new();
        cleanup_help_tags(&mut file, Some(b"de"));
        assert!(file.is_empty());
    }

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
    fn help_compare_orders_by_heuristic_before_name() {
        assert_eq!(
            help_compare(b"zzz\x00000001\x00", b"aaa\x00000010\x00"),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn help_compare_uses_name_as_a_tie_breaker() {
        assert_eq!(
            help_compare(b"alpha\x00000100\x00", b"beta\x00000100\x00"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            help_compare(b"same\x00000100\x00", b"same\x00000100\x00"),
            std::cmp::Ordering::Equal
        );
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
