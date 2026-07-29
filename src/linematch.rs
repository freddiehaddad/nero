//! Translated from `src/nvim/linematch.c`: the "linematch" algorithm used
//! by diff mode to align individual lines within a diff hunk across 2 or
//! more buffers (an `int**`-indexed dynamic-programming search over an
//! N-dimensional tensor, one dimension per compared buffer). Fully
//! translated - a single, self-contained, pure computational algorithm
//! with no dependency on any not-yet-translated subsystem.
//!
//! `mmfile_t` (the original's xdiff-derived `{ ptr, size }` byte-buffer
//! type) is represented here simply as `&[u8]` - the same "idiomatic
//! native equivalent instead of an FFI-flavored struct" translation this
//! crate already applies to `char_u *`-plus-length elsewhere (e.g.
//! `charset.rs`, `mbyte.rs`). A buffer with zero content (the original's
//! `{ .ptr = NULL, .size = 0 }`, used by `diff.c`'s caller when a diff
//! block has zero lines in some buffer) is simply the empty slice `&[]`,
//! which already behaves correctly everywhere the original relies on
//! `memchr`/pointer-arithmetic no-ops for a NULL/zero-size buffer.
//!
//! The original's `diffcmppath_T *` sibling-node pointers (each node
//! pointing at other nodes within the same single `xmalloc`ed array) are
//! translated as plain `usize` indices into the owning
//! `Vec<DiffCmpPath>` rather than raw pointers - the same index-based
//! self-referential-structure translation this crate already uses for
//! tree/graph-like data in `marktree.rs`.
//!
//! This file's only caller (`diff.c`'s `diff_read_line`/linematch
//! integration) is part of the not-yet-translated real diff-computation
//! engine (see `diff.rs`'s own module doc comment), so
//! [`linematch_nbuffers`] and its helpers are translated here as
//! free-standing, fully-tested functions, ready to be wired up once
//! `diff.c` itself is tackled.
/// Maximum number of buffers `linematch_nbuffers` can compare at once
/// (`LN_MAX_BUFS`).
pub const LN_MAX_BUFS: usize = 8;

/// `pow(2, LN_MAX_BUFS) - 1` - the maximum number of distinct non-empty
/// "which buffers advance" bitmask choices (`LN_DECISION_MAX`).
pub const LN_DECISION_MAX: usize = 255;

/// Cap on how many bytes of a single line are considered when scoring how
/// well two lines match (`MATCH_CHAR_MAX_LEN`).
const MATCH_CHAR_MAX_LEN: usize = 800;

/// Length of the first line in `m` (the byte offset of the first `b'\n'`),
/// or `m.len()` itself if `m` contains no newline at all (`line_len`).
fn line_len(m: &[u8]) -> usize {
    m.iter().position(|&b| b == b'\n').unwrap_or(m.len())
}

/// Returns the number of matching characters between the first lines of
/// `m1` and `m2`, respecting sequence order - i.e. the length of their
/// longest common subsequence, computed over at most
/// `MATCH_CHAR_MAX_LEN - 1` bytes of each (`matching_chars`).
///
/// Examples (matching the original's own doc comment):
/// `matching_chars(b"aabc", b"acba")` is 2 (`'a'` and `'b'` in common);
/// `matching_chars(b"123hello567", b"he123ll567o")` is 8 (`"123"`, `"ll"`
/// and `"567"` in common); `matching_chars(b"abcdefg", b"gfedcba")` is 1
/// (every character is in common, but only 1 at a time in sequence).
fn matching_chars(m1: &[u8], m2: &[u8]) -> i32 {
    let s1len = line_len(m1).min(MATCH_CHAR_MAX_LEN - 1);
    let s2len = line_len(m2).min(MATCH_CHAR_MAX_LEN - 1);
    let s1 = &m1[..s1len];
    let s2 = &m2[..s2len];

    // Rolling 2-row longest-common-subsequence table: `matrix[icur]` is
    // the row currently being computed (indexed by `j + 1` for column
    // `j`), `matrix[1 - icur]` is the previous row - saves memory over a
    // full `s1len x s2len` table, matching the original exactly (which
    // stores only 2 rows of the `i` axis, indexed by a toggling `icur`).
    let mut matrix = [[0i32; MATCH_CHAR_MAX_LEN]; 2];
    let mut icur = 1usize;
    for &c1 in s1 {
        icur = 1 - icur;
        let prev = 1 - icur;
        for (j, &c2) in s2.iter().enumerate() {
            // skip char in s1
            if matrix[prev][j + 1] > matrix[icur][j + 1] {
                matrix[icur][j + 1] = matrix[prev][j + 1];
            }
            // skip char in s2
            if matrix[icur][j] > matrix[icur][j + 1] {
                matrix[icur][j + 1] = matrix[icur][j];
            }
            // compare char in s1 and s2
            if c1 == c2 && matrix[prev][j] + 1 > matrix[icur][j + 1] {
                matrix[icur][j + 1] = matrix[prev][j] + 1;
            }
        }
    }
    matrix[icur][s2len]
}

/// Same as [`matching_chars`], but ignores space/tab whitespace in both
/// lines first (`matching_chars_iwhite`).
fn matching_chars_iwhite(m1: &[u8], m2: &[u8]) -> i32 {
    // The original reads one byte *past* `line_len(s)` here (`i <= slen`,
    // not `i < slen`). That boundary byte is well-defined and safe to
    // read whenever `slen < s.len()` (it is simply the next real byte in
    // the buffer - typically the line's own `b'\n'` terminator, or,  if
    // `MATCH_CHAR_MAX_LEN - 1` capped `slen` short of the true line
    // length, an ordinary content byte) - both cases are reproduced
    // exactly here. The single case where the original's `s[slen]` would
    // read *past the end of the allocation* is a final line with no
    // trailing newline that also fits entirely within the cap (so
    // `slen == s.len()` exactly); `slice::get` is used instead of direct
    // indexing so that one case safely stops the loop instead of
    // reading undefined memory - there is no well-defined original
    // behavior to match there in the first place. Note that even a
    // legitimately-read boundary byte often ends up discarded anyway:
    // [`matching_chars`] re-derives its own `line_len`/cap truncation
    // over whatever `strip` returns, which removes a trailing `b'\n'`
    // carried over from here again.
    let strip = |s: &[u8]| -> Vec<u8> {
        let slen = line_len(s).min(MATCH_CHAR_MAX_LEN - 1);
        let mut out = Vec::with_capacity(slen + 1);
        for i in 0..=slen {
            let Some(&e) = s.get(i) else { break };
            if e != b' ' && e != b'\t' {
                out.push(e);
            }
        }
        out
    };
    matching_chars(&strip(m1), &strip(m2))
}

/// Counts the matching characters between every pair of participating
/// (non-`None`) lines in `sp`, normalizing the sum so that 3-or-more-way
/// matches are scored comparably to a plain 2-way match
/// (`count_n_matched_chars`).
///
/// `sp`'s `None` entries are the original's `sp[i]->ptr == NULL` check -
/// a buffer not participating in this particular comparison.
fn count_n_matched_chars(sp: &[Option<&[u8]>], iwhite: bool) -> i32 {
    let mut matched_chars = 0i32;
    let mut matched = 0i32;
    for i in 0..sp.len() {
        for j in (i + 1)..sp.len() {
            if let (Some(a), Some(b)) = (sp[i], sp[j]) {
                matched += 1;
                matched_chars += if iwhite {
                    matching_chars_iwhite(a, b)
                } else {
                    matching_chars(a, b)
                };
            }
        }
    }

    // prioritize a match of 3 (or more lines) equally to a match of 2 lines
    if matched >= 2 {
        matched_chars *= 2;
        matched_chars /= matched;
    }

    matched_chars
}

/// Flattens an N-dimensional tensor index `values` (one coordinate per
/// dimension, dimension `k` ranging `0..=diff_len[k]`) into a single flat
/// row-major index (`unwrap_indexes`).
///
/// `values` and `diff_len` must have the same length (the original's
/// shared `ndiffs` parameter).
fn unwrap_indexes(values: &[i32], diff_len: &[i32]) -> usize {
    debug_assert_eq!(values.len(), diff_len.len());

    let mut num_unwrap_scalar: usize = 1;
    for &len in diff_len {
        num_unwrap_scalar *= (len as usize) + 1;
    }

    let mut path_idx = 0;
    for k in 0..diff_len.len() {
        num_unwrap_scalar /= (diff_len[k] as usize) + 1;
        path_idx += num_unwrap_scalar * (values[k] as usize);
    }
    path_idx
}

/// Advances `s` forward past `lnum - 1` newlines, returning the
/// remaining buffer starting at the beginning of line `lnum`
/// (`fastforward_buf_to_lnum`).
///
/// If `s` runs out of newlines before reaching `lnum` (fewer than
/// `lnum - 1` lines remain), returns the empty slice - matching the
/// original's own `{ .ptr = NULL, .size = 0 }` result in that case.
///
/// Unlike this file's other helpers (all `static` in the original),
/// this one has real external linkage in the original C (also called
/// directly by `lua/xdiff.c`'s `vim.diff()` binding, not just internally
/// by `linematch_nbuffers`), so it is `pub` here too.
pub fn fastforward_buf_to_lnum(mut s: &[u8], lnum: crate::pos_defs::LinenrT) -> &[u8] {
    for _ in 0..(lnum - 1) {
        match s.iter().position(|&b| b == b'\n') {
            Some(pos) => s = &s[pos + 1..],
            None => {
                s = &[];
                break;
            }
        }
    }
    s
}

/// One node of the N-dimensional dynamic-programming tensor built by
/// [`linematch_nbuffers`] (`diffcmppath_T`/`struct diffcmppath_S`).
///
/// The original's sibling-node pointers (`df_decision`, each pointing at
/// another node within the very same single `xmalloc`ed array) are
/// translated as plain `usize` indices into the owning
/// `Vec<DiffCmpPath>` instead - the same index-based self-referential-
/// structure translation this crate already uses for tree/graph-like
/// data in `marktree.rs`, avoiding both unsafe code and any aliasing
/// hazard from the tensor being (in principle) accessed at two indices
/// "at once".
#[derive(Clone, Copy)]
struct DiffCmpPath {
    /// Total matched-character score of the best path(s) found so far
    /// leading to this node (`df_lev_score`).
    df_lev_score: i32,
    /// Number of valid entries in `df_choice`/`df_decision` (only the
    /// first `df_path_n` of each are meaningful) (`df_path_n`).
    df_path_n: usize,
    /// Memoized [`test_charmatch_paths`] result per `lastdecision`
    /// bitmask value; `-1` means "not yet computed" (`df_choice_mem`).
    df_choice_mem: [i32; LN_DECISION_MAX + 1],
    /// The "which buffers advanced" bitmask for each of the (tied-)best
    /// incoming path(s) (`df_choice`).
    df_choice: [i32; LN_DECISION_MAX],
    /// The predecessor node's index for each of the (tied-)best incoming
    /// path(s) (`df_decision`, originally `diffcmppath_T *[]`).
    df_decision: [usize; LN_DECISION_MAX],
    /// Which of the (tied-)best incoming paths [`test_charmatch_paths`]
    /// picked as requiring the fewest "turns" (`df_optimal_choice`).
    df_optimal_choice: usize,
}

impl DiffCmpPath {
    /// A fresh tensor node: no score, no incoming paths yet, and every
    /// `df_choice_mem` slot marked "not yet computed" - matching the
    /// original's own explicit per-node initialization loop in
    /// `linematch_nbuffers` (`xmalloc` does not zero memory, so the
    /// original always initializes every node's fields itself too).
    ///
    /// The original only initializes the first `2^ndiffs` of
    /// `df_choice_mem`'s `LN_DECISION_MAX + 1` slots (the only ones any
    /// real `ndiffs <= LN_MAX_BUFS` bitmask value can ever index); this
    /// initializes the whole array unconditionally instead, which is
    /// equivalent (the remaining slots are simply never read) and avoids
    /// threading `ndiffs` through this constructor.
    fn new() -> Self {
        Self {
            df_lev_score: 0,
            df_path_n: 0,
            df_choice_mem: [-1; LN_DECISION_MAX + 1],
            df_choice: [0; LN_DECISION_MAX],
            df_decision: [0; LN_DECISION_MAX],
            df_optimal_choice: 0,
        }
    }
}

/// Explores every non-empty subset ("choice" bitmask) of the axes listed
/// in `paths` that could have decreased to reach the current tensor
/// coordinate `df_iters`, scores each candidate predecessor path, and
/// keeps the tensor node's best (or tied-best) incoming path(s)
/// (`try_possible_paths`).
///
/// `path_idx`/`choice` thread a backtracking subset-enumeration
/// recursion through, mirroring the original's own recursive/pointer-
/// mutation structure exactly: at each level, first try including axis
/// `paths[path_idx]` in the choice and recurse, then try excluding it
/// and recurse again - once `path_idx == paths.len()`, `choice` holds one
/// complete subset to evaluate.
#[allow(clippy::too_many_arguments)]
fn try_possible_paths(
    df_iters: &[i32],
    paths: &[usize],
    path_idx: usize,
    choice: &mut i32,
    diffcmppath: &mut [DiffCmpPath],
    diff_len: &[i32],
    diff_blk: &[&[u8]],
    iwhite: bool,
) {
    if path_idx == paths.len() {
        if *choice > 0 {
            let ndiffs = diff_len.len();
            let mut from_vals = [0i32; LN_MAX_BUFS];
            from_vals[..ndiffs].copy_from_slice(&df_iters[..ndiffs]);
            let mut current_lines: [Option<&[u8]>; LN_MAX_BUFS] = [None; LN_MAX_BUFS];
            for k in 0..ndiffs {
                if *choice & (1 << k) != 0 {
                    from_vals[k] -= 1;
                    current_lines[k] = Some(fastforward_buf_to_lnum(diff_blk[k], df_iters[k]));
                }
            }
            let unwrapped_idx_from = unwrap_indexes(&from_vals[..ndiffs], diff_len);
            let unwrapped_idx_to = unwrap_indexes(df_iters, diff_len);
            let matched_chars = count_n_matched_chars(&current_lines[..ndiffs], iwhite);
            let score = diffcmppath[unwrapped_idx_from].df_lev_score + matched_chars;
            if score > diffcmppath[unwrapped_idx_to].df_lev_score {
                diffcmppath[unwrapped_idx_to].df_path_n = 1;
                diffcmppath[unwrapped_idx_to].df_decision[0] = unwrapped_idx_from;
                diffcmppath[unwrapped_idx_to].df_choice[0] = *choice;
                diffcmppath[unwrapped_idx_to].df_lev_score = score;
            } else if score == diffcmppath[unwrapped_idx_to].df_lev_score {
                let k = diffcmppath[unwrapped_idx_to].df_path_n;
                diffcmppath[unwrapped_idx_to].df_path_n += 1;
                diffcmppath[unwrapped_idx_to].df_decision[k] = unwrapped_idx_from;
                diffcmppath[unwrapped_idx_to].df_choice[k] = *choice;
            }
        }
        return;
    }
    let bit_place = paths[path_idx];
    *choice |= 1 << bit_place;
    try_possible_paths(
        df_iters, paths, path_idx + 1, choice, diffcmppath, diff_len, diff_blk, iwhite,
    );
    *choice &= !(1 << bit_place);
    try_possible_paths(
        df_iters, paths, path_idx + 1, choice, diffcmppath, diff_len, diff_blk, iwhite,
    );
}

/// Recursively iterates every coordinate of the N-dimensional tensor
/// (nesting `ch_dim` from `0` up to `diff_len.len()`, each level looping
/// its own axis from `0` to `diff_len[ch_dim]` inclusive), and at each
/// coordinate, explores every non-empty subset of "which axes could have
/// decreased to reach here" via [`try_possible_paths`]
/// (`populate_tensor`).
fn populate_tensor(
    df_iters: &mut [i32],
    ch_dim: usize,
    diffcmppath: &mut [DiffCmpPath],
    diff_len: &[i32],
    diff_blk: &[&[u8]],
    iwhite: bool,
) {
    let ndiffs = diff_len.len();
    if ch_dim == ndiffs {
        let mut paths = [0usize; LN_MAX_BUFS];
        let mut npaths = 0;
        for (j, &iter) in df_iters.iter().enumerate().take(ndiffs) {
            if iter > 0 {
                paths[npaths] = j;
                npaths += 1;
            }
        }
        let mut choice = 0i32;
        let unwrapped_idx_to = unwrap_indexes(df_iters, diff_len);
        diffcmppath[unwrapped_idx_to].df_lev_score = -1;
        try_possible_paths(
            df_iters,
            &paths[..npaths],
            0,
            &mut choice,
            diffcmppath,
            diff_len,
            diff_blk,
            iwhite,
        );
        return;
    }

    for i in 0..=diff_len[ch_dim] {
        df_iters[ch_dim] = i;
        populate_tensor(df_iters, ch_dim + 1, diffcmppath, diff_len, diff_blk, iwhite);
    }
}

/// Given the tensor node at `node_idx`, memoized-recursively finds the
/// minimum number of "turns" (bitmask changes between consecutive steps)
/// needed along one of its best-scoring incoming paths back to the
/// tensor's origin, breaking ties among multiple equally-good-scoring
/// incoming paths in favor of fewer turns, and records the winning
/// choice in that node's `df_optimal_choice` (`test_charmatch_paths`).
fn test_charmatch_paths(diffcmppath: &mut [DiffCmpPath], node_idx: usize, lastdecision: i32) -> i32 {
    let cached = diffcmppath[node_idx].df_choice_mem[lastdecision as usize];
    if cached != -1 {
        return cached;
    }

    let path_n = diffcmppath[node_idx].df_path_n;
    let result = if path_n == 0 {
        0
    } else {
        let mut minimum_turns = i32::MAX;
        let mut best_choice = 0usize;
        for i in 0..path_n {
            let decision = diffcmppath[node_idx].df_decision[i];
            let choice = diffcmppath[node_idx].df_choice[i];
            let t = test_charmatch_paths(diffcmppath, decision, choice)
                + i32::from(lastdecision != choice);
            if t < minimum_turns {
                best_choice = i;
                minimum_turns = t;
            }
        }
        diffcmppath[node_idx].df_optimal_choice = best_choice;
        minimum_turns
    };

    diffcmppath[node_idx].df_choice_mem[lastdecision as usize] = result;
    result
}

/// Finds an optimal line-by-line alignment of a diff hunk across 2 or
/// more buffers (`linematch_nbuffers`).
///
/// `diff_blk[k]` is the entire remaining diff-block content for buffer
/// `k` (its lines' own text, each newline-terminated) and `diff_len[k]`
/// is how many lines it contains; `diff_blk` and `diff_len` must have
/// the same length (the original's `ndiffs`, at most [`LN_MAX_BUFS`] -
/// checked with `debug_assert!`, matching the original's own
/// release-mode-compiled-out `assert()`; unlike the original, exceeding
/// `LN_MAX_BUFS` in a release build safely panics here on the first
/// fixed-size-array access rather than silently overflowing a stack
/// array).
///
/// Returns a sequence of bitmask "decisions": each entry describes which
/// buffers should each contribute one line to the next aligned row (bit
/// `k` set means buffer `k` advances), in forward playback order.
///
/// For an explanation of the algorithm itself (a dynamic-programming
/// search over an N-dimensional tensor, one dimension per compared
/// buffer), see the original's own lengthy doc comment on
/// `linematch_nbuffers` in `linematch.c`.
pub fn linematch_nbuffers(diff_blk: &[&[u8]], diff_len: &[i32], iwhite: bool) -> Vec<i32> {
    let ndiffs = diff_len.len();
    debug_assert_eq!(diff_blk.len(), ndiffs);
    debug_assert!(ndiffs <= LN_MAX_BUFS);
    debug_assert!(diff_len.iter().all(|&n| n >= 0));

    let memsize: usize = diff_len.iter().map(|&n| (n as usize) + 1).product();
    let mut diffcmppath = vec![DiffCmpPath::new(); memsize];

    let mut df_iters = [0i32; LN_MAX_BUFS];
    populate_tensor(
        &mut df_iters[..ndiffs],
        0,
        &mut diffcmppath,
        diff_len,
        diff_blk,
        iwhite,
    );

    let u = unwrap_indexes(diff_len, diff_len);
    test_charmatch_paths(&mut diffcmppath, u, 0);

    let mut decisions = Vec::new();
    let mut current_idx = u;
    while diffcmppath[current_idx].df_path_n > 0 {
        let j = diffcmppath[current_idx].df_optimal_choice;
        decisions.push(diffcmppath[current_idx].df_choice[j]);
        current_idx = diffcmppath[current_idx].df_decision[j];
    }
    decisions.reverse();
    decisions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_len_stops_at_newline() {
        assert_eq!(line_len(b"hello\nworld"), 5);
    }

    #[test]
    fn line_len_is_whole_buffer_without_newline() {
        assert_eq!(line_len(b"hello"), 5);
        assert_eq!(line_len(b""), 0);
    }

    #[test]
    fn matching_chars_examples_from_original_doc_comment() {
        assert_eq!(matching_chars(b"aabc", b"acba"), 2);
        assert_eq!(matching_chars(b"123hello567", b"he123ll567o"), 8);
        assert_eq!(matching_chars(b"abcdefg", b"gfedcba"), 1);
    }

    #[test]
    fn matching_chars_identical_lines_match_fully() {
        assert_eq!(matching_chars(b"identical", b"identical"), 9);
    }

    #[test]
    fn matching_chars_only_considers_first_line() {
        // Only "abc" (before the '\n') should be compared, not "zzz".
        assert_eq!(matching_chars(b"abc\nzzz", b"abc"), 3);
    }

    #[test]
    fn matching_chars_no_overlap_is_zero() {
        assert_eq!(matching_chars(b"abc", b"xyz"), 0);
    }

    #[test]
    fn matching_chars_iwhite_ignores_space_and_tab() {
        // "a b\tc" stripped of whitespace is "abc"; the trailing '\n'
        // carried over from both inputs' own line boundary is discarded
        // again by `matching_chars`'s own re-truncation (see that
        // function's doc comment), so the result is exactly as if
        // comparing "abc" against "abc".
        assert_eq!(matching_chars_iwhite(b"a b\tc\n", b"abc\n"), 3);
    }

    #[test]
    fn matching_chars_iwhite_final_line_without_newline_is_safe() {
        // No trailing newline and the line fits entirely within the cap:
        // the original would read one byte past the end here. Must not
        // panic, and must still compare the real content correctly.
        assert_eq!(matching_chars_iwhite(b"abc", b"abc"), 3);
    }

    #[test]
    fn count_n_matched_chars_two_buffers() {
        assert_eq!(
            count_n_matched_chars(&[Some(b"abc".as_slice()), Some(b"abc".as_slice())], false),
            3
        );
    }

    #[test]
    fn count_n_matched_chars_skips_absent_buffers() {
        // Only the Some/Some pair (index 0 and 2) should be compared.
        let sp = [Some(b"abc".as_slice()), None, Some(b"abc".as_slice())];
        assert_eq!(count_n_matched_chars(&sp, false), 3);
    }

    #[test]
    fn count_n_matched_chars_normalizes_three_way_match() {
        // 3 participating buffers -> 3 pairs, each scoring 3 -> summed
        // to 9, then normalized by `*2/3` (matched == 3) -> 6.
        let sp = [
            Some(b"abc".as_slice()),
            Some(b"abc".as_slice()),
            Some(b"abc".as_slice()),
        ];
        assert_eq!(count_n_matched_chars(&sp, false), 6);
    }

    #[test]
    fn count_n_matched_chars_none_participating_is_zero() {
        assert_eq!(count_n_matched_chars(&[None, None], false), 0);
    }

    #[test]
    fn count_n_matched_chars_respects_iwhite() {
        // Both lines share a space character at a "corresponding" slot:
        // raw comparison counts it as 1 matching character, but with
        // iwhite stripping the space from both first, "ab" vs "xy" have
        // nothing in common.
        let sp = [Some(b"a b".as_slice()), Some(b"x y".as_slice())];
        assert_eq!(count_n_matched_chars(&sp, false), 1);
        assert_eq!(count_n_matched_chars(&sp, true), 0);
    }

    #[test]
    fn unwrap_indexes_origin_is_zero() {
        assert_eq!(unwrap_indexes(&[0, 0, 0], &[3, 2, 1]), 0);
    }

    #[test]
    fn unwrap_indexes_end_is_last_cell() {
        // The final coordinate (diff_len itself) must unwrap to the very
        // last flat index: product(diff_len[k] + 1) - 1.
        let diff_len = [3, 2, 1];
        let memsize: usize = diff_len.iter().map(|&n| (n as usize) + 1).product();
        assert_eq!(unwrap_indexes(&diff_len, &diff_len), memsize - 1);
    }

    #[test]
    fn unwrap_indexes_matches_manual_row_major_2d() {
        // A 2D (3+1) x (2+1) tensor: index (i, j) should unwrap to
        // i * (2 + 1) + j, the standard row-major formula.
        let diff_len = [3, 2];
        for i in 0..=3 {
            for j in 0..=2 {
                assert_eq!(unwrap_indexes(&[i, j], &diff_len), (i as usize) * 3 + (j as usize));
            }
        }
    }

    #[test]
    fn fastforward_buf_to_lnum_lnum_1_is_a_no_op() {
        assert_eq!(fastforward_buf_to_lnum(b"one\ntwo\nthree", 1), b"one\ntwo\nthree");
    }

    #[test]
    fn fastforward_buf_to_lnum_skips_to_the_right_line() {
        assert_eq!(fastforward_buf_to_lnum(b"one\ntwo\nthree", 2), b"two\nthree");
        assert_eq!(fastforward_buf_to_lnum(b"one\ntwo\nthree", 3), b"three");
    }

    #[test]
    fn fastforward_buf_to_lnum_past_the_end_is_empty() {
        // Only 3 lines exist; asking for line 4 (or beyond) runs out of
        // newlines before getting there.
        assert_eq!(fastforward_buf_to_lnum(b"one\ntwo\nthree", 4), b"");
        assert_eq!(fastforward_buf_to_lnum(b"one\ntwo\nthree", 100), b"");
    }

    #[test]
    fn fastforward_buf_to_lnum_empty_buffer() {
        assert_eq!(fastforward_buf_to_lnum(b"", 1), b"");
        assert_eq!(fastforward_buf_to_lnum(b"", 2), b"");
    }

    #[test]
    fn linematch_nbuffers_no_buffers_is_empty() {
        assert_eq!(linematch_nbuffers(&[], &[], false), Vec::<i32>::new());
    }

    #[test]
    fn linematch_nbuffers_both_empty_is_empty() {
        // Two buffers with zero lines each: nothing to align.
        assert_eq!(linematch_nbuffers(&[b"", b""], &[0, 0], false), Vec::<i32>::new());
    }

    #[test]
    fn linematch_nbuffers_two_identical_single_lines_align_together() {
        // Both buffers have exactly 1 identical line - the only sensible
        // alignment has both buffers contribute together in a single row
        // (bit 0 and bit 1 both set = 3). Hand-traced against the
        // algorithm's own DP recurrence before writing this assertion.
        let decisions = linematch_nbuffers(&[b"hello\n", b"hello\n"], &[1, 1], false);
        assert_eq!(decisions, vec![3]);
    }

    #[test]
    fn linematch_nbuffers_one_buffer_has_no_lines() {
        // Buffer 0 has 1 line, buffer 1 has none - the only possible
        // decision is buffer 0 advancing alone (bit 0 only = 1).
        let decisions = linematch_nbuffers(&[b"hello\n", b""], &[1, 0], false);
        assert_eq!(decisions, vec![1]);
    }

    #[test]
    fn linematch_nbuffers_two_matching_lines_each() {
        // Both buffers have the same 2 identical lines - expect both
        // rows to align together (buffer 0 and 1 both advancing each
        // step).
        let decisions = linematch_nbuffers(&[b"aaa\nbbb\n", b"aaa\nbbb\n"], &[2, 2], false);
        assert_eq!(decisions, vec![3, 3]);
    }

    #[test]
    fn linematch_nbuffers_three_buffers_all_identical() {
        // 3-way comparison, all identical single lines - all 3 buffers
        // should advance together (bits 0, 1, 2 set = 7).
        let decisions =
            linematch_nbuffers(&[b"same\n", b"same\n", b"same\n"], &[1, 1, 1], false);
        assert_eq!(decisions, vec![7]);
    }

    #[test]
    fn linematch_nbuffers_decisions_account_for_every_line() {
        // However the lines get grouped into rows, every buffer's own
        // line count must be exactly covered once each decision bit is
        // tallied up across the whole returned sequence - a structural
        // invariant that must hold for any valid input, regardless of
        // the specific grouping chosen.
        let diff_len = [3, 2];
        let decisions = linematch_nbuffers(&[b"a1\na2\na3\n", b"b1\nb2\n"], &diff_len, false);
        let mut counts = [0i32; 2];
        for &d in &decisions {
            for (k, count) in counts.iter_mut().enumerate() {
                if d & (1 << k) != 0 {
                    *count += 1;
                }
            }
        }
        assert_eq!(counts, diff_len);
    }
}
