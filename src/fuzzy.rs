//! Translated from `src/nvim/fuzzy.c` (partial): the core fzy-derived
//! fuzzy-matching scoring algorithm.
//!
//! Portions of the original are adapted from fzy
//! (<https://github.com/jhawthorn/fzy>), Copyright (c) 2014 John Hawthorn,
//! licensed under the MIT License (see the original file's header for the
//! full license text) - preserved here since this translation is itself
//! derived from that code via neovim's C port of it.
//!
//! Translated: `has_match`, `compute_bonus_codepoint`, `setup_match_struct`
//! (as `MatchStruct::new`), `match_row`, `match_positions`, `fuzzy_match`,
//! `fuzzy_match_str`.
//!
//! The original's Unicode handling (`utf_ptr2char`/`utfc_ptr2len`/
//! `mb_tolower`/`mb_toupper`/`mb_isupper`/`mb_islower`, all `mbyte.c`, not
//! yet translated) is subsumed here by Rust's native, already-Unicode-aware
//! `str`/`char` (`str::chars()`, `char::to_lowercase()`, `char::is_uppercase()`,
//! `char::is_lowercase()`) - this is the same "native equivalent already
//! solves the C-specific problem" pattern as `math.rs`'s use of
//! `f64::classify()`/`u64::trailing_zeros()`, not a design change.
//!
//! `compute_bonus_codepoint`'s `vim_iswordc(c)` check (customizable via the
//! `'iskeyword'` option, needing the not-yet-translated chartab/option
//! system) is approximated here as "alphanumeric or underscore", which is
//! exactly neovim's *default* `'iskeyword'` value
//! (`"@,48-57,_,192-255"` = letters + digits + `_` + Latin-1 supplement) for
//! the common ASCII+underscore case - not a behavioral simplification for
//! the default configuration, though it won't track a customized
//! `'iskeyword'` until the option system exists.
//!
//! Deferred: `fuzzy_match_in_list`/`fuzzy_match_str_with_pos`/
//! `fuzzy_match_str_in_line`/`search_for_fuzzy_match`/`f_matchfuzzy`/
//! `f_matchfuzzypos`/everything operating on `list_T`/`typval_T`/`buf_T`/
//! `garray_T` (eval engine phase 5, buffer phase 3) or `pos_T`-based buffer
//! search.

const SCORE_MAX: f64 = f64::INFINITY;
const SCORE_MIN: f64 = f64::NEG_INFINITY;
const SCORE_SCALE: f64 = 1000.0;

/// `FUZZY_MATCH_MAX_LEN`/`MATCH_MAX_LEN`: max characters that can be matched.
pub const FUZZY_MATCH_MAX_LEN: usize = 1024;
/// `FUZZY_SCORE_NONE`: invalid fuzzy score.
pub const FUZZY_SCORE_NONE: i32 = i32::MIN;

const SCORE_GAP_LEADING: f64 = -0.005;
const SCORE_GAP_TRAILING: f64 = -0.005;
const SCORE_GAP_INNER: f64 = -0.01;
const SCORE_MATCH_CONSECUTIVE: f64 = 1.0;
const SCORE_MATCH_SLASH: f64 = 0.9;
const SCORE_MATCH_WORD: f64 = 0.8;
const SCORE_MATCH_CAPITAL: f64 = 0.7;
const SCORE_MATCH_DOT: f64 = 0.6;

#[inline]
fn is_word_sep(c: char) -> bool {
    c == '-' || c == '_' || c == ' '
}
#[inline]
fn is_path_sep(c: char) -> bool {
    c == '/'
}
#[inline]
fn is_dot(c: char) -> bool {
    c == '.'
}

/// Approximates `vim_iswordc` for the default `'iskeyword'` - see module
/// docs.
#[inline]
fn is_word_char_default(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[inline]
fn to_lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Returns true if every character of `needle` occurs in `haystack`, in
/// order (case-insensitively, matching uppercase-of-needle too) (`has_match`).
fn has_match(needle: &str, haystack: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let mut h_chars = haystack.chars();
    'outer: for n_char in needle.chars() {
        let n_upper = n_char.to_uppercase().next().unwrap_or(n_char);
        for h_char in h_chars.by_ref() {
            if n_char == h_char || n_upper == h_char {
                continue 'outer;
            }
        }
        return false; // ran out of haystack before matching n_char
    }
    true
}

/// `compute_bonus_codepoint`
fn compute_bonus_codepoint(last_c: char, c: char) -> f64 {
    if c.is_ascii_alphanumeric() || is_word_char_default(c) {
        if is_path_sep(last_c) {
            return SCORE_MATCH_SLASH;
        }
        if is_word_sep(last_c) {
            return SCORE_MATCH_WORD;
        }
        if is_dot(last_c) {
            return SCORE_MATCH_DOT;
        }
        if c.is_uppercase() && last_c.is_lowercase() {
            return SCORE_MATCH_CAPITAL;
        }
    }
    0.0
}

struct MatchStruct {
    lower_needle: Vec<char>,
    lower_haystack: Vec<char>,
    match_bonus: Vec<f64>,
}

impl MatchStruct {
    /// `setup_match_struct`
    fn new(needle: &str, haystack: &str) -> Self {
        let lower_needle: Vec<char> = needle.chars().take(FUZZY_MATCH_MAX_LEN).map(to_lower).collect();

        let mut lower_haystack = Vec::with_capacity(FUZZY_MATCH_MAX_LEN.min(haystack.len()));
        let mut match_bonus = Vec::with_capacity(lower_haystack.capacity());
        let mut prev_c = '/';
        for c in haystack.chars().take(FUZZY_MATCH_MAX_LEN) {
            lower_haystack.push(to_lower(c));
            match_bonus.push(compute_bonus_codepoint(prev_c, c));
            prev_c = c;
        }

        MatchStruct {
            lower_needle,
            lower_haystack,
            match_bonus,
        }
    }
}

/// `match_row`: fills row `i` of the `D`/`M` dynamic-programming matrices.
fn match_row(
    m: &MatchStruct,
    i: usize,
    curr_d: &mut [f64],
    curr_m: &mut [f64],
    last_d: &[f64],
    last_m: &[f64],
) {
    let n = m.lower_needle.len();
    let mlen = m.lower_haystack.len();

    let mut prev_score = SCORE_MIN;
    let gap_score = if i == n - 1 { SCORE_GAP_TRAILING } else { SCORE_GAP_INNER };

    let mut prev_m = SCORE_MIN;
    let mut prev_d = SCORE_MIN;

    for j in 0..mlen {
        if m.lower_needle[i] == m.lower_haystack[j] {
            let mut score = SCORE_MIN;
            if i == 0 {
                score = (j as f64) * SCORE_GAP_LEADING + m.match_bonus[j];
            } else if j > 0 {
                // i > 0 && j > 0
                score = (prev_m + m.match_bonus[j]).max(
                    // consecutive match, doesn't stack with match_bonus
                    prev_d + SCORE_MATCH_CONSECUTIVE,
                );
            }
            prev_d = last_d[j];
            prev_m = last_m[j];
            curr_d[j] = score;
            prev_score = score.max(prev_score + gap_score);
            curr_m[j] = prev_score;
        } else {
            prev_d = last_d[j];
            prev_m = last_m[j];
            curr_d[j] = SCORE_MIN;
            prev_score += gap_score;
            curr_m[j] = prev_score;
        }
    }
}

/// `match_positions`: computes the fzy match score of `needle` in
/// `haystack` (which must already be known to match, e.g. via
/// [`has_match`]), and optionally fills `positions` with the matched
/// character indices (into `haystack`'s chars).
fn match_positions(needle: &str, haystack: &str, mut positions: Option<&mut [u32]>) -> f64 {
    if needle.is_empty() {
        return SCORE_MIN;
    }

    let ms = MatchStruct::new(needle, haystack);
    let n = ms.lower_needle.len();
    let m = ms.lower_haystack.len();

    if m > FUZZY_MATCH_MAX_LEN || n > m {
        // Unreasonably large candidate: return no score
        return SCORE_MIN;
    } else if n == m {
        // If the lengths of the strings are equal (after truncation) and
        // this was only called because has_match() succeeded, the strings
        // themselves must also be equal (ignoring case) unless truncation
        // caused a coincidental length match - so check before taking the
        // shortcut.
        if ms.lower_needle == ms.lower_haystack {
            if let Some(positions) = positions.as_deref_mut() {
                for (i, p) in positions.iter_mut().enumerate().take(n) {
                    *p = i as u32;
                }
            }
            return SCORE_MAX;
        }
    }

    // D[][] stores the best score for this position ending with a match.
    // M[][] stores the best possible score at this position.
    let mut d = vec![vec![0.0f64; m]; n];
    let mut mm = vec![vec![0.0f64; m]; n];

    {
        // Row 0's `last_d`/`last_m` reads are provably dead: match_row only
        // consults `prev_d`/`prev_m` (populated from `last_d`/`last_m`) in
        // its `else if j > 0` branch, which requires `i > 0` - never true
        // for row 0. The original C aliases D[0]/M[0] to themselves for
        // this call relying on that same fact; Rust's borrow checker (for
        // good reason - it can't see the dead-ness) won't allow the actual
        // alias, so a same-sized dummy row stands in instead.
        let dummy_d = vec![0.0f64; m];
        let dummy_m = vec![0.0f64; m];
        match_row(&ms, 0, &mut d[0], &mut mm[0], &dummy_d, &dummy_m);
    }
    for i in 1..n {
        let (last_d, curr_d) = d.split_at_mut(i);
        let (last_m, curr_m) = mm.split_at_mut(i);
        match_row(&ms, i, &mut curr_d[0], &mut curr_m[0], &last_d[i - 1], &last_m[i - 1]);
    }

    // backtrace to find the positions of optimal matching
    if let Some(positions) = positions {
        let mut match_required = false;
        let mut j = m as isize - 1;
        for i in (0..n).rev() {
            while j >= 0 {
                let ju = j as usize;
                // There may be multiple paths which result in the optimal
                // weight. For simplicity, pick the first one encountered,
                // the latest in the candidate string.
                if d[i][ju] != SCORE_MIN && (match_required || d[i][ju] == mm[i][ju]) {
                    // If this score was determined using
                    // SCORE_MATCH_CONSECUTIVE, the previous character MUST
                    // be a match.
                    match_required =
                        i > 0 && ju > 0 && mm[i][ju] == d[i - 1][ju - 1] + SCORE_MATCH_CONSECUTIVE;
                    positions[i] = j as u32;
                    j -= 1;
                    break;
                }
                j -= 1;
            }
        }
    }

    mm[n - 1][m - 1]
}

/// `fuzzy_match()`.
///
/// Returns true if `pat_arg` matches `str`. Also returns the match score in
/// `out_score` and the matching character positions (into `str`'s chars)
/// in `matches`, up to `matches.len()` entries.
pub fn fuzzy_match(str_: &str, pat_arg: &str, matchseq: bool, matches: &mut [u32]) -> (bool, i32) {
    let max_matches = matches.len();
    let mut num_matches = 0usize;
    let mut out_score: i64 = 0;

    // Try matching each word in `pat_arg` in `str_` (split on whitespace,
    // unless matchseq).
    let words: Vec<&str> = if matchseq {
        vec![pat_arg.trim()]
    } else {
        pat_arg.split_whitespace().collect()
    };
    if words.is_empty() {
        return (false, FUZZY_SCORE_NONE);
    }

    for pat in words {
        if pat.is_empty() {
            continue;
        }
        let pat_chars = pat.chars().count().min(max_matches);
        if num_matches > max_matches - pat_chars {
            return (false, FUZZY_SCORE_NONE);
        }

        let mut score = FUZZY_SCORE_NONE;
        if has_match(pat, str_) {
            let fzy_score = match_positions(pat, str_, Some(&mut matches[num_matches..]));
            if fzy_score != SCORE_MIN {
                score = if fzy_score == SCORE_MAX {
                    i32::MAX
                } else if fzy_score < 0.0 {
                    (fzy_score * SCORE_SCALE - 0.5).ceil() as i32
                } else {
                    (fzy_score * SCORE_SCALE + 0.5).floor() as i32
                };
            }
        }

        if score == FUZZY_SCORE_NONE {
            return (false, FUZZY_SCORE_NONE);
        }

        if score > 0 && out_score > i64::from(i32::MAX) - i64::from(score) {
            out_score = i64::from(i32::MAX);
        } else if score < 0 && out_score < i64::from(i32::MIN) + 1 - i64::from(score) {
            out_score = i64::from(i32::MIN) + 1;
        } else {
            out_score += i64::from(score);
        }

        num_matches += pat_chars;
        if num_matches >= max_matches {
            break;
        }
    }

    (num_matches != 0, out_score as i32)
}

/// `fuzzy_match_str`: fuzzy match `pat` against the whole of `str_` (no
/// whitespace-splitting) and return just the score, or `0` if either is
/// empty (matching the original's early-return for `NULL`).
pub fn fuzzy_match_str(str_: &str, pat: &str) -> i32 {
    if str_.is_empty() || pat.is_empty() {
        return 0;
    }
    let mut matchpos = [0u32; FUZZY_MATCH_MAX_LEN];
    let (_, score) = fuzzy_match(str_, pat, true, &mut matchpos);
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_match_finds_subsequence() {
        assert!(has_match("abc", "axbxcx"));
        // The original's check is `n_char == h_char || mb_toupper(n_char) == h_char`,
        // which is intentionally asymmetric: a lowercase needle char matches
        // either case in the haystack (uppercasing it can only ever equal an
        // uppercase haystack char, so lowercase-vs-lowercase already matches
        // via the first branch, and lowercase-vs-uppercase matches via the
        // second) - but an uppercase needle char, whose uppercase is itself,
        // only ever matches an uppercase haystack char.
        assert!(has_match("abc", "AXBXCX")); // lowercase needle matches uppercase haystack
        assert!(!has_match("ABC", "axbxcx")); // uppercase needle does NOT match lowercase haystack
        assert!(has_match("ABC", "AXBXCX")); // uppercase needle matches uppercase haystack
        assert!(!has_match("abcd", "axbxcx"));
        assert!(!has_match("", "abc"));
    }

    #[test]
    fn fuzzy_match_str_scores_exact_match_highest() {
        let exact = fuzzy_match_str("abc", "abc");
        let fuzzy = fuzzy_match_str("axbxcx", "abc");
        assert!(exact > 0);
        assert!(fuzzy > 0);
        assert!(exact > fuzzy, "exact match should score higher than a loose one");
    }

    #[test]
    fn fuzzy_match_str_no_match_returns_none_sentinel() {
        assert_eq!(fuzzy_match_str("xyz", "abc"), FUZZY_SCORE_NONE);
    }

    #[test]
    fn fuzzy_match_str_empty_inputs_return_zero() {
        assert_eq!(fuzzy_match_str("", "abc"), 0);
        assert_eq!(fuzzy_match_str("abc", ""), 0);
    }

    #[test]
    fn fuzzy_match_rewards_word_boundaries() {
        // "fb" should score higher against "foo_bar" (matches after a word
        // separator) than against "ffbb" (no boundary bonus).
        let mut m1 = [0u32; 8];
        let (matched1, score_boundary) = fuzzy_match("foo_bar", "fb", true, &mut m1);
        let mut m2 = [0u32; 8];
        let (matched2, score_no_boundary) = fuzzy_match("ffbb", "fb", true, &mut m2);
        assert!(matched1 && matched2);
        assert!(score_boundary > score_no_boundary);
    }

    #[test]
    fn fuzzy_match_reports_positions() {
        let mut matches = [0u32; 8];
        let (matched, _) = fuzzy_match("abc", "ac", true, &mut matches);
        assert!(matched);
        assert_eq!(matches[0], 0); // 'a' at index 0
        assert_eq!(matches[1], 2); // 'c' at index 2
    }

    #[test]
    fn fuzzy_match_splits_pattern_on_whitespace_unless_matchseq() {
        let mut matches = [0u32; 16];
        // matchseq=false: "foo bar" is two words, both must match somewhere.
        let (matched, _) = fuzzy_match("xxfooxxbarxx", "foo bar", false, &mut matches);
        assert!(matched);
        // matchseq=true: "foo bar" (with the literal space) must match as one sequence.
        let (matched_seq, _) = fuzzy_match("xxfooxxbarxx", "foo bar", true, &mut matches);
        assert!(!matched_seq);
    }
}
