//! Translated from `src/nvim/marktree.c` (partial - position-arithmetic
//! helpers only; the B-tree algorithm itself - node search/insert/split/
//! merge/rebalance, iterator traversal - is substantial (75.6KB) and
//! deserves a dedicated, focused translation pass rather than being rushed
//! alongside many other files).
//!
//! Tree data structure for storing marks at (row, col) positions and
//! updating them to arbitrary text changes. Derivative work of `kbtree` in
//! klib (BSD-style license, John-Mark Gurney / Attractive Chaos - see the
//! original file's header comment for the full notice, preserved there),
//! with neovim's own changes under Apache v2, matching this project's
//! overall license.
//!
//! Marks are inserted using `marktree_put`. Text changes are processed
//! using `marktree_splice`. All read and delete operations use the
//! iterator: `marktree_itr_get` to put an iterator at a given position, or
//! `marktree_lookup` to look up a mark by its id; `marktree_itr_current`
//! and `marktree_itr_next`/`prev` to read marks in a loop;
//! `marktree_del_itr` deletes the current mark of the iterator. None of
//! this is translated yet - only the two pairs of position-arithmetic
//! helpers below, which have no dependency on the tree structure itself.

// These helpers are `static` (private) in the original, matching the
// non-`pub` visibility kept here. They have no caller yet since the rest
// of marktree.c's algorithm isn't translated - #[allow(dead_code)] instead
// of prematurely making them `pub` (which would misrepresent the
// original's intended visibility) or deleting them (which would lose
// verified, tested translation work ahead of its eventual use).
#![allow(dead_code)]

use crate::marktree_defs::MtPos;

/// `pos_leq`: is `a` less than or equal to `b`?
#[inline]
fn pos_leq(a: MtPos, b: MtPos) -> bool {
    a.row < b.row || (a.row == b.row && a.col <= b.col)
}

/// `pos_less`: is `a` strictly less than `b`?
#[inline]
fn pos_less(a: MtPos, b: MtPos) -> bool {
    !pos_leq(b, a)
}

/// Rewrites `val` to be relative to `base` (`relative`): the marktree's key
/// insight for efficient bulk position updates during text edits is that
/// most positions are stored relative to an ancestor node's position
/// rather than absolute, so shifting that ancestor updates all descendants
/// implicitly. `base` must be `<= val` (checked, matching the original's
/// `assert`).
fn relative(base: MtPos, val: &mut MtPos) {
    assert!(pos_leq(base, *val), "relative: base must be <= val");
    if val.row == base.row {
        val.row = 0;
        val.col -= base.col;
    } else {
        val.row -= base.row;
    }
}

/// Inverse of [`relative`]: rewrites `val` (currently relative to `base`)
/// back to an absolute position.
fn unrelative(base: MtPos, val: &mut MtPos) {
    if val.row == 0 {
        val.row = base.row;
        val.col += base.col;
    } else {
        val.row += base.row;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(row: i32, col: i32) -> MtPos {
        MtPos::new(row, col)
    }

    #[test]
    fn pos_leq_and_less_match_lexicographic_row_then_col_order() {
        assert!(pos_leq(p(1, 5), p(1, 5)));
        assert!(pos_leq(p(1, 5), p(1, 6)));
        assert!(pos_leq(p(1, 9), p(2, 0)));
        assert!(!pos_leq(p(2, 0), p(1, 9)));

        assert!(pos_less(p(1, 5), p(1, 6)));
        assert!(!pos_less(p(1, 5), p(1, 5)));
        assert!(pos_less(p(1, 9), p(2, 0)));
    }

    #[test]
    fn relative_and_unrelative_are_inverses_same_row() {
        let base = p(10, 20);
        let mut v = p(10, 25);
        relative(base, &mut v);
        assert_eq!(v, p(0, 5));
        unrelative(base, &mut v);
        assert_eq!(v, p(10, 25));
    }

    #[test]
    fn relative_and_unrelative_are_inverses_different_row() {
        let base = p(10, 20);
        let mut v = p(15, 3);
        relative(base, &mut v);
        assert_eq!(v, p(5, 3)); // row becomes an offset, col untouched
        unrelative(base, &mut v);
        assert_eq!(v, p(15, 3));
    }

    #[test]
    #[should_panic(expected = "relative: base must be <= val")]
    fn relative_panics_if_base_is_after_val() {
        let mut v = p(1, 0);
        relative(p(2, 0), &mut v);
    }
}
