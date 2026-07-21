//! Translated from `src/nvim/marktree_defs.h` (types only - the B-tree
//! algorithm itself, `marktree.c`, 75.6KB, is substantial and deferred to
//! its own dedicated translation pass).

use crate::decoration_defs::DecorInlineData;
use crate::map::Map;

pub const MT_MAX_DEPTH: usize = 20;
pub const MT_BRANCH_FACTOR: usize = 10;
// note max branch is actually 2*MT_BRANCH_FACTOR
// and strictly this is ceil(log2(2*MT_BRANCH_FACTOR + 1))
// as we need a pseudo-index for "right before this node"
pub const MT_LOG2_BRANCH: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MtPos {
    pub row: i32,
    pub col: i32,
}

impl MtPos {
    /// `MTPos(r, c)` macro constructor.
    #[inline]
    pub const fn new(row: i32, col: i32) -> Self {
        MtPos { row, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaIndex {
    Inline,
    Lines,
    SignHl,
    SignText,
    ConcealLines,
    /// sentinel, must be last
    Count,
}
pub const K_MT_META_COUNT: usize = MetaIndex::Count as usize;

/// `kMTFilterSelect`
pub const MT_FILTER_SELECT: u32 = u32::MAX;

/// a filter should be set to [`MT_FILTER_SELECT`] for the selected kinds,
/// zero otherwise (`MetaFilter`)
pub type MetaFilter<'a> = &'a [u32];

/// Internal storage (`MTKey`).
///
/// NB: actual marks have `flags > 0`, so `(row, col, 0)` can be used as a
/// pseudo-key for "space before (row,col)".
///
/// No `Clone`/`Copy` (yet): unlike a standalone [`DecorInlineData`], this
/// struct *does* have its tag co-located (`flags`, right alongside
/// `decor_data`), so a safe manual `Clone` reading the `MT_FLAG_DECOR_EXT`
/// bit is possible in principle - but that flag's exact bit value is
/// defined in `marktree.c`/`decoration.c` (not yet translated), and
/// guessing it here risks a real memory-safety bug (reading `ext` when
/// `hl` is actually stored, or vice versa). Deferred until that context
/// exists.
pub struct MtKey {
    pub pos: MtPos,
    pub ns: u32,
    pub id: u32,
    pub flags: u16,
    /// "ext" tag in `flags` - see [`DecorInlineData`]'s doc comment for why
    /// this stays a raw union rather than a safe enum.
    pub decor_data: DecorInlineData,
}

pub struct MtPair {
    pub start: MtKey,
    pub end_pos: MtPos,
    pub end_right_gravity: bool,
}

/// `Intersection`: `kvec_withinit_t(uint64_t, 4)` - a small-size-optimized
/// growable vector (inline storage for up to 4 elements before spilling to
/// the heap). Modeled directly as `Vec<u64>`: unlike `HashtabT`'s
/// small-array optimization (which needed a *self-referential* pointer
/// into its own inline array - the actual hazard that forced a redesign
/// there), `kvec_withinit_t` has no such self-reference; it is a pure
/// performance optimization with no soundness implication here, so it is
/// not worth its own custom small-vec type before an actual performance
/// need is demonstrated.
pub type Intersection = Vec<u64>;

/// Part of the original's `mtnode_s`, only meaningful for inner nodes:
/// pointers to children plus their meta counts (`mtnode_inner_s`).
pub struct MtNodeInner {
    pub i_ptr: [*mut MtNode; 2 * MT_BRANCH_FACTOR],
    pub i_meta: [[u32; K_MT_META_COUNT]; 2 * MT_BRANCH_FACTOR],
}

/// A marktree B-tree node (`mtnode_s`/`MTNode`).
///
/// Kept as a raw-pointer-linked structure (`parent`, and children via
/// `inner`), exactly like the original: this is a from-scratch B-tree doing
/// manual node splitting/merging in `marktree.c` (not yet translated), and
/// pointer-based parent/child navigation is inherent to that algorithm, not
/// an incidental C detail to "fix" away.
///
/// The original's `struct mtnode_inner_s s[];` C99 flexible array member
/// (only actually allocated for inner, non-leaf nodes) becomes an
/// `Option<Box<MtNodeInner>>` here: `None` for leaf nodes, matching the
/// same "only present sometimes" shape without needing an unsafely-sized
/// allocation trick Rust doesn't have a direct equivalent for.
pub struct MtNode {
    pub n: i32,
    pub level: i16,
    /// index in parent
    pub p_idx: i16,
    pub intersect: Intersection,
    pub parent: *mut MtNode,
    pub key: [MtKey; 2 * MT_BRANCH_FACTOR - 1],
    pub inner: Option<Box<MtNodeInner>>,
}

/// A marktree traversal/search iterator (`MarkTreeIter`).
pub struct MarkTreeIter {
    pub pos: MtPos,
    pub lvl: i32,
    pub x: *mut MtNode,
    pub i: i32,
    pub s: [MarkTreeIterFrame; MT_MAX_DEPTH],
    pub intersect_idx: usize,
    pub intersect_pos: MtPos,
    pub intersect_pos_x: MtPos,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MarkTreeIterFrame {
    pub oldcol: i32,
    pub i: i32,
}

impl MarkTreeIter {
    /// `marktree_itr_valid(itr)`
    #[inline]
    pub fn is_valid(&self) -> bool {
        !self.x.is_null()
    }
}

/// The marktree itself (`MarkTree`).
pub struct MarkTree {
    pub root: *mut MtNode,
    pub meta_root: [u32; K_MT_META_COUNT],
    pub n_keys: usize,
    pub n_nodes: usize,
    /// `PMap(uint64_t) id2node[1]` - a single-element array in the
    /// original purely so it can be addressed like an embedded struct
    /// (`&x->id2node[0]`) while still being a distinct allocation-free
    /// member; a plain field achieves the same in Rust.
    pub id2node: Map<u64, *mut MtNode>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtpos_constructor_matches_macro() {
        let p = MtPos::new(3, 4);
        assert_eq!(p.row, 3);
        assert_eq!(p.col, 4);
        assert_eq!(p, MtPos { row: 3, col: 4 });
    }

    #[test]
    fn marktree_iter_validity_tracks_null_pointer() {
        let mut itr = MarkTreeIter {
            pos: MtPos::default(),
            lvl: 0,
            x: std::ptr::null_mut(),
            i: 0,
            s: [MarkTreeIterFrame::default(); MT_MAX_DEPTH],
            intersect_idx: 0,
            intersect_pos: MtPos::default(),
            intersect_pos_x: MtPos::default(),
        };
        assert!(!itr.is_valid());
        // A well-defined non-null-but-not-dereferenced pointer, since
        // is_valid() only checks nullness and never dereferences `x`.
        itr.x = std::ptr::NonNull::<MtNode>::dangling().as_ptr();
        assert!(itr.is_valid());
    }
}
