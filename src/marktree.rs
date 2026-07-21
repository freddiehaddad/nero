//! Translated from `src/nvim/marktree.c` (partial) and its companion
//! header `src/nvim/marktree.h` (flag constants and small inline
//! predicates - `marktree_defs.h`'s own inline functions are none; that
//! header is pure type/const definitions and lives in `marktree_defs.rs`).
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
//! `marktree_del_itr` deletes the current mark of the iterator.
//!
//! Translated so far: the `marktree.h` flag constants and key predicates;
//! position-arithmetic helpers (`pos_leq`/`pos_less`/`relative`/
//! `unrelative`/`compose`); `key_cmp`; the node lifecycle
//! (`marktree_alloc_node`/`marktree_free_node`/`marktree_free_subtree`/
//! `marktree_clear`); the id2node index helpers (`refkey`/
//! `lookup_id2node`); the in-node binary search (`marktree_getp_aux`); the
//! meta-counting helpers (`meta_describe_key(_inc)`/`meta_describe_node`/
//! `meta_has`); the `Intersection` (sorted `Vec<u64>`) set-operation
//! helpers (`intersection_has`/`intersect_node`/`unintersect_node`/
//! `intersect_merge`/`intersect_mov`/`intersect_common`/`intersect_add`/
//! `intersect_sub`); the pseudo-index helpers (`pseudo_index`/
//! `pseudo_index_for_id`); the full insert path (`bubble_up`,
//! `split_node`, `marktree_putp_aux`, `marktree_put_key`); and (as
//! `#[cfg(test)]`-only, since they exist to validate the above rather than
//! being needed by any other translated file yet) the original's own
//! tree-invariant checker `marktree_check`/`marktree_check_node` and its
//! `marktree_put_test` unit-test helper.
//!
//! Not yet translated (deferred - the remaining hard parts of the B-tree:
//! deletion/rebalancing and iteration, each deserving their own dedicated
//! pass): `marktree_put` (the paired-mark-aware public entry point - needs
//! `marktree_lookup`/`marktree_intersect_pair`, themselves needing
//! iterators), `merge_node`, `pivot_left`/`pivot_right`,
//! `marktree_del_itr`, `marktree_revise_meta`, all `marktree_itr_*`
//! iterator functions, `marktree_splice` and its helpers
//! (`check_damage`/`swap_keys`), `marktree_move`/`marktree_move_region`/
//! `marktree_restore_pair`, the id-based lookup functions
//! (`marktree_lookup`/`marktree_lookup_ns`/`marktree_get_alt(pos)`),
//! `marktree_intersect_pair`, `marktree_check_intersections` (needs
//! iterators), and the debug-only `mt_inspect*`/`marktree_del_pair_test`
//! functions.

// Several helpers here are `static` (private) in the original, matching
// the non-`pub` visibility kept here. Some have no caller yet since the
// rest of marktree.c's algorithm isn't translated - #[allow(dead_code)]
// instead of prematurely making them `pub` (which would misrepresent the
// original's intended visibility) or deleting them (which would lose
// verified, tested translation work ahead of its eventual use).
#![allow(dead_code)]

use crate::decoration_defs::{DecorHighlightInline, DecorInline, DecorVirtText};
use crate::marktree_defs::{Intersection, MarkTree, MetaFilter, MetaIndex, MtKey, MtNode, MtPos, K_MT_META_COUNT};
use std::mem::ManuallyDrop;

// --- src/nvim/marktree.h: flag bits on `MtKey.flags` ---

/// real, live mark (as opposed to a bare position with no mark attached to
/// it, see `MtKey`'s own doc comment on the `(row, col, 0)` pseudo-key
/// convention).
pub const MT_FLAG_REAL: u16 = 1 << 0;
/// this key is the end of a paired mark.
pub const MT_FLAG_END: u16 = 1 << 1;
/// this mark has a matching start/end pair (as opposed to a lone point).
pub const MT_FLAG_PAIRED: u16 = 1 << 2;
/// the other side of this paired mark was deleted; this mark must be
/// deleted very soon!
pub const MT_FLAG_ORPHANED: u16 = 1 << 3;
pub const MT_FLAG_NO_UNDO: u16 = 1 << 4;
pub const MT_FLAG_INVALIDATE: u16 = 1 << 5;
pub const MT_FLAG_INVALID: u16 = 1 << 6;
/// discriminant for the `MtKey.decor_data` union: set means `.ext` is the
/// live field, unset means `.hl` is.
pub const MT_FLAG_DECOR_EXT: u16 = 1 << 7;

// TODO(bfredl) (kept from the original): flags for decorations. These
// cover the cases where we quickly need to skip over irrelevant marks
// internally. When we refactor this more, also make all info for
// ExtmarkType included here.
pub const MT_FLAG_DECOR_HL: u16 = 1 << 8;
pub const MT_FLAG_DECOR_SIGNTEXT: u16 = 1 << 9;
// TODO(bfredl) (kept from the original): for now this means specifically
// number_hl, line_hl, cursorline_hl - needs to clean up the name.
pub const MT_FLAG_DECOR_SIGNHL: u16 = 1 << 10;
pub const MT_FLAG_DECOR_VIRT_LINES: u16 = 1 << 11;
pub const MT_FLAG_DECOR_VIRT_TEXT_INLINE: u16 = 1 << 12;
pub const MT_FLAG_DECOR_CONCEAL_LINES: u16 = 1 << 13;

// These _must_ be last to preserve ordering of marks.
pub const MT_FLAG_RIGHT_GRAVITY: u16 = 1 << 14;
pub const MT_FLAG_LAST: u16 = 1 << 15;

pub const MT_FLAG_DECOR_MASK: u16 = MT_FLAG_DECOR_EXT
    | MT_FLAG_DECOR_HL
    | MT_FLAG_DECOR_SIGNTEXT
    | MT_FLAG_DECOR_SIGNHL
    | MT_FLAG_DECOR_VIRT_LINES
    | MT_FLAG_DECOR_VIRT_TEXT_INLINE;

pub const MT_FLAG_EXTERNAL_MASK: u16 =
    MT_FLAG_DECOR_MASK | MT_FLAG_NO_UNDO | MT_FLAG_INVALIDATE | MT_FLAG_INVALID | MT_FLAG_DECOR_CONCEAL_LINES;

/// This is defined so that start and end of the same range have adjacent
/// ids (`MARKTREE_END_FLAG`).
pub const MARKTREE_END_FLAG: u64 = 1;

/// `MT_INVALID_KEY`: the sentinel invalid key, position `(-1, -1)`.
pub fn mt_invalid_key() -> MtKey {
    MtKey {
        pos: MtPos::new(-1, -1),
        ns: 0,
        id: 0,
        flags: 0,
        decor_data: crate::decoration_defs::DecorInlineData { hl: DecorHighlightInline::default() },
    }
}

#[inline]
pub fn mt_lookup_id(ns: u32, id: u32, enda: bool) -> u64 {
    ((ns as u64) << 33) | (((id as u64) << 1) | if enda { MARKTREE_END_FLAG } else { 0 })
}

#[inline]
pub fn mt_lookup_key_side(key: &MtKey, end: bool) -> u64 {
    mt_lookup_id(key.ns, key.id, end)
}

#[inline]
pub fn mt_lookup_key(key: &MtKey) -> u64 {
    mt_lookup_id(key.ns, key.id, key.flags & MT_FLAG_END != 0)
}

#[inline]
pub fn mt_paired(key: &MtKey) -> bool {
    key.flags & MT_FLAG_PAIRED != 0
}

#[inline]
pub fn mt_end(key: &MtKey) -> bool {
    key.flags & MT_FLAG_END != 0
}

#[inline]
pub fn mt_start(key: &MtKey) -> bool {
    mt_paired(key) && !mt_end(key)
}

#[inline]
pub fn mt_right(key: &MtKey) -> bool {
    key.flags & MT_FLAG_RIGHT_GRAVITY != 0
}

#[inline]
pub fn mt_no_undo(key: &MtKey) -> bool {
    key.flags & MT_FLAG_NO_UNDO != 0
}

#[inline]
pub fn mt_invalidate(key: &MtKey) -> bool {
    key.flags & MT_FLAG_INVALIDATE != 0
}

#[inline]
pub fn mt_invalid(key: &MtKey) -> bool {
    key.flags & MT_FLAG_INVALID != 0
}

#[inline]
pub fn mt_decor_any(key: &MtKey) -> bool {
    key.flags & MT_FLAG_DECOR_MASK != 0
}

#[inline]
pub fn mt_decor_sign(key: &MtKey) -> bool {
    key.flags & (MT_FLAG_DECOR_SIGNTEXT | MT_FLAG_DECOR_SIGNHL) != 0
}

#[inline]
pub fn mt_conceal_lines(key: &MtKey) -> bool {
    key.flags & MT_FLAG_DECOR_CONCEAL_LINES != 0
}

#[inline]
pub fn mt_flags(right_gravity: bool, no_undo: bool, invalidate: bool, decor_ext: bool) -> u16 {
    (if right_gravity { MT_FLAG_RIGHT_GRAVITY } else { 0 })
        | (if no_undo { MT_FLAG_NO_UNDO } else { 0 })
        | (if invalidate { MT_FLAG_INVALIDATE } else { 0 })
        | (if decor_ext { MT_FLAG_DECOR_EXT } else { 0 })
}

/// `mtpair_from`: `end` is only ever read here (`.pos`, `mt_right`), never
/// stored, so it is taken by reference rather than by value (`MtKey` is
/// not `Copy`, and consuming+dropping it here for no reason would be a
/// pointless move restriction on every caller).
pub fn mtpair_from(start: MtKey, end: &MtKey) -> crate::marktree_defs::MtPair {
    crate::marktree_defs::MtPair { end_pos: end.pos, end_right_gravity: mt_right(end), start }
}

/// `mt_decor`: builds an owned, safe [`DecorInline`] from a key's raw
/// inline union storage, using `flags & MT_FLAG_DECOR_EXT` as the tag.
/// Takes `key` by value (consuming it, matching the original's pass-by-
/// value signature) since the `Ext` branch's `DecorExt` may own a `Box`
/// that cannot be aliased the way C's implicit pointer copy would allow;
/// callers that still need `key` afterwards should `key.clone()` first
/// (see `MtKey`'s `Clone` impl).
pub fn mt_decor(key: MtKey) -> DecorInline {
    if key.flags & MT_FLAG_DECOR_EXT != 0 {
        // SAFETY: flag says `ext` is the live field.
        DecorInline::Ext(ManuallyDrop::into_inner(unsafe { key.decor_data.ext }))
    } else {
        // SAFETY: flag says `hl` is the live field.
        DecorInline::Highlight(unsafe { key.decor_data.hl })
    }
}

/// `mt_decor_virt`: non-owning access to the virtual-text pointer of an
/// `ext`-tagged key, or `None` if this key is not `ext`-tagged (mirrors
/// the original's `NULL` return, and the original's `DecorExt.vt` being a
/// non-owning read in this accessor).
pub fn mt_decor_virt(mark: &MtKey) -> Option<&DecorVirtText> {
    if mark.flags & MT_FLAG_DECOR_EXT != 0 {
        // SAFETY: flag says `ext` is the live field.
        unsafe { mark.decor_data.ext.vt.as_deref() }
    } else {
        None
    }
}


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

/// `compose`: composes `val` (itself relative to some point) onto `base`,
/// leaving the result in `base`. Used when walking down the tree
/// accumulating relative positions into an absolute one.
fn compose(base: &mut MtPos, val: MtPos) {
    if val.row == 0 {
        base.col += val.col;
    } else {
        base.row += val.row;
        base.col = val.col;
    }
}

#[allow(clippy::eq_op)]
fn mt_generic_cmp<T: PartialOrd>(a: T, b: T) -> i32 {
    // ((b) < (a)) - ((a) < (b)), matching the original macro exactly
    // (avoids relying on `Ord`, since some future callers may compare
    // floats; every actual use so far is on integers).
    (b < a) as i32 - (a < b) as i32
}

/// `key_cmp`: orders keys by `(row, col)`, then by a small mask of flags
/// (gravity/end/real/last) to break ties between marks at the same
/// position - see the original's own `TODO(bfredl)` comment, preserved
/// here, about `MT_FLAG_REAL` ideally not being needed for this.
fn key_cmp(a: &MtKey, b: &MtKey) -> i32 {
    let cmp = mt_generic_cmp(a.pos.row, b.pos.row);
    if cmp != 0 {
        return cmp;
    }
    let cmp = mt_generic_cmp(a.pos.col, b.pos.col);
    if cmp != 0 {
        return cmp;
    }

    // TODO(bfredl): MT_FLAG_REAL could go away if we fix marktree_getp_aux for real
    let cmp_mask = MT_FLAG_RIGHT_GRAVITY | MT_FLAG_END | MT_FLAG_REAL | MT_FLAG_LAST;
    mt_generic_cmp(a.flags & cmp_mask, b.flags & cmp_mask)
}

/// @return position of `k` if it exists in the node, otherwise the position
/// it should be inserted, which ranges from 0 to `x.n` _inclusively_.
/// `match_out` (optional) is set to `true` if a match (pos, gravity) was found
/// (`marktree_getp_aux`).
fn marktree_getp_aux(x: &MtNode, k: &MtKey, match_out: Option<&mut bool>) -> i32 {
    let mut dummy_match = false;
    let m = match_out.unwrap_or(&mut dummy_match);

    let mut begin: i32 = 0;
    let mut end: i32 = x.n;
    if x.n == 0 {
        *m = false;
        return -1;
    }
    while begin < end {
        let mid = (begin + end) >> 1;
        if key_cmp(&x.key[mid as usize], k) < 0 {
            begin = mid + 1;
        } else {
            end = mid;
        }
    }
    if begin == x.n {
        *m = false;
        return x.n - 1;
    }
    *m = key_cmp(k, &x.key[begin as usize]) == 0;
    if !*m {
        begin -= 1;
    }
    begin
}

/// `refkey`: refreshes the `id2node` index entry for key `i` of node `x` to
/// point at `x` itself (called after a key is copied/moved to a new
/// node/position).
///
/// # Safety
/// `x` must be a valid, non-null pointer for the lifetime of the `id2node`
/// entry being created (matches the original: `id2node` always stores raw
/// `MTNode *` pointers into the live tree).
unsafe fn refkey(b: &mut MarkTree, x: *mut MtNode, i: usize) {
    // SAFETY: caller guarantees `x` is valid; `i` must be `< (*x).n`.
    let key = unsafe { &(*x).key[i] };
    b.id2node.insert(mt_lookup_key(key), x);
}

/// `id2node` (renamed `lookup_id2node` to avoid clashing with
/// `MarkTree.id2node`, the field it reads): look up the node currently
/// storing the key with the given lookup id.
fn lookup_id2node(b: &MarkTree, id: u64) -> Option<*mut MtNode> {
    b.id2node.get(&id).copied()
}

/// `marktree_alloc_node`: allocates a new, zeroed node and hands out an
/// owning raw pointer via [`Box::into_raw`] - the original's
/// `xcalloc`-then-return-owning-pointer pattern, translated directly:
/// this whole data structure manages node lifetime manually via raw
/// pointers (parent/child links), exactly like the original, so the
/// pointer returned here is later freed with [`marktree_free_node`] via
/// [`Box::from_raw`] (never by simply dropping a safe owner).
///
/// `internal` only controls whether storage for `inner` (the
/// children-pointers/meta array, the original's variable-sized `s[]`
/// flexible array member) is allocated - it does *not* set `level`, which
/// (matching the original's `xcalloc`) always starts at `0`. Real callers
/// (e.g. `split_node`'s `z->level = y->level;`) always set `level`
/// explicitly right after allocating; this function alone does not make a
/// node "internal" in the tree-traversal sense.
fn marktree_alloc_node(b: &mut MarkTree, internal: bool) -> *mut MtNode {
    let node = MtNode {
        n: 0,
        level: 0,
        p_idx: 0,
        intersect: Intersection::new(),
        parent: std::ptr::null_mut(),
        key: std::array::from_fn(|_| MtKey::default()),
        inner: if internal { Some(Box::default()) } else { None },
    };
    b.n_nodes += 1;
    Box::into_raw(Box::new(node))
}

/// `marktree_free_node`: frees a single node (not its children - see
/// [`marktree_free_subtree`]), taking back ownership of the pointer handed
/// out by [`marktree_alloc_node`].
///
/// # Safety
/// `x` must be a currently-valid pointer originally obtained from
/// [`marktree_alloc_node`] on this same `b`, and must not be used again
/// afterwards.
unsafe fn marktree_free_node(b: &mut MarkTree, x: *mut MtNode) {
    // SAFETY: caller guarantees `x` came from `Box::into_raw` in
    // `marktree_alloc_node` and is not used again after this call.
    drop(unsafe { Box::from_raw(x) });
    b.n_nodes -= 1;
}

/// `marktree_free_subtree`: recursively frees `x` and (if `x` is an
/// internal node) all of its children first.
///
/// # Safety
/// `x` must be a valid pointer into a tree rooted consistently with `b`
/// (every child pointer reachable from `x` must itself have come from
/// `marktree_alloc_node` on this `b`), and must not be used again
/// afterwards.
pub unsafe fn marktree_free_subtree(b: &mut MarkTree, x: *mut MtNode) {
    // SAFETY: caller guarantees `x` is valid; children pointers under an
    // internal node are likewise guaranteed valid by the same contract.
    let level = unsafe { (*x).level };
    if level != 0 {
        let n = unsafe { (*x).n };
        for i in 0..=n {
            let child = unsafe { (*x).inner.as_ref().unwrap().i_ptr[i as usize] };
            unsafe { marktree_free_subtree(b, child) };
        }
    }
    unsafe { marktree_free_node(b, x) };
}

/// `marktree_clear`: frees the entire tree (if any) and resets `b` to a
/// fresh, empty state.
///
/// # Safety
/// If `b.root` is non-null, it must be a valid root pointer (every
/// reachable node must have come from `marktree_alloc_node` on this `b`).
pub unsafe fn marktree_clear(b: &mut MarkTree) {
    if !b.root.is_null() {
        // SAFETY: caller guarantees `b.root` is a valid root pointer.
        unsafe { marktree_free_subtree(b, b.root) };
        b.root = std::ptr::null_mut();
    }
    // `Map<K, V>` has no `clear()` of its own (unlike `Set<K>`); replacing
    // it with a fresh, empty map is the direct equivalent of the original's
    // `map_destroy` (deallocate and reset to empty).
    b.id2node = crate::map::Map::default();
    b.n_keys = 0;
    b.meta_root = [0; K_MT_META_COUNT];
    debug_assert_eq!(b.n_nodes, 0);
}

// really meta_inc[kMTMetaCount]

/// `meta_describe_key_inc`: accumulates the "kind" counts contributed by a
/// single key into `meta_inc` (only real, non-end, non-invalid marks
/// contribute - end keys/invalid marks are described via their start key).
fn meta_describe_key_inc(meta_inc: &mut [u32; K_MT_META_COUNT], k: &MtKey) {
    if !mt_end(k) && !mt_invalid(k) {
        meta_inc[MetaIndex::Inline as usize] += u32::from(k.flags & MT_FLAG_DECOR_VIRT_TEXT_INLINE != 0);
        meta_inc[MetaIndex::Lines as usize] += u32::from(k.flags & MT_FLAG_DECOR_VIRT_LINES != 0);
        meta_inc[MetaIndex::SignHl as usize] += u32::from(k.flags & MT_FLAG_DECOR_SIGNHL != 0);
        meta_inc[MetaIndex::SignText as usize] += u32::from(k.flags & MT_FLAG_DECOR_SIGNTEXT != 0);
        meta_inc[MetaIndex::ConcealLines as usize] += u32::from(k.flags & MT_FLAG_DECOR_CONCEAL_LINES != 0);
    }
}

/// `meta_describe_key`: same as [`meta_describe_key_inc`] but starting
/// from a zeroed counter array rather than accumulating into an existing
/// one.
fn meta_describe_key(k: &MtKey) -> [u32; K_MT_META_COUNT] {
    let mut meta_inc = [0u32; K_MT_META_COUNT];
    meta_describe_key_inc(&mut meta_inc, k);
    meta_inc
}

/// `meta_describe_node`: describes the full "kind" counts for node `x`,
/// including (if `x` is internal) the already-computed counts of its
/// direct children (assumes those are correct, per the original's own
/// comment).
///
/// # Safety
/// If `x.level != 0` (internal node), `x.inner` must be `Some` and its
/// `i_meta[0..=x.n]` entries must already be correct/up to date.
fn meta_describe_node(x: &MtNode) -> [u32; K_MT_META_COUNT] {
    let mut meta_node = [0u32; K_MT_META_COUNT];
    for i in 0..x.n as usize {
        meta_describe_key_inc(&mut meta_node, &x.key[i]);
    }
    if x.level != 0 {
        let inner = x.inner.as_ref().expect("internal node must have `inner`");
        for i in 0..=(x.n as usize) {
            for (dst, src) in meta_node.iter_mut().zip(inner.i_meta[i].iter()) {
                *dst += src;
            }
        }
    }
    meta_node
}

/// `meta_has`: does `meta_count` have a nonzero count for any of the kinds
/// selected by `meta_filter` (each entry either
/// [`crate::marktree_defs::MT_FILTER_SELECT`] selected-mask or `0` to
/// ignore, per [`MetaFilter`]'s doc comment)?
fn meta_has(meta_count: &[u32; K_MT_META_COUNT], meta_filter: MetaFilter<'_>) -> bool {
    let mut count: u32 = 0;
    for m in 0..K_MT_META_COUNT {
        count = count.wrapping_add(meta_count[m] & meta_filter[m]);
    }
    count > 0
}

/// `intersection_has`: is `id` present in the sorted [`Intersection`]
/// vector `x`?
fn intersection_has(x: &Intersection, id: u64) -> bool {
    for &v in x.iter() {
        if v == id {
            return true;
        } else if v >= id {
            return false;
        }
    }
    false
}

/// `intersect_node`: inserts `id` into the sorted [`Intersection`] vector
/// `x` (`id` must not have the [`MARKTREE_END_FLAG`] bit set - only start
/// ids are ever tracked as intersections).
fn intersect_node(x: &mut Intersection, id: u64) {
    debug_assert!(id & MARKTREE_END_FLAG == 0);
    // The original inserts via a hand-rolled shift-while-scanning-backward
    // loop (optimized for the common case that `id` sorts near the end);
    // a plain sorted insert achieves the identical resulting order and is
    // the natural, idiomatic Rust equivalent for a `Vec`.
    let pos = x.partition_point(|&v| v < id);
    x.insert(pos, id);
}

/// `unintersect_node`: removes `id` from the sorted [`Intersection`]
/// vector `x`, if present. If `strict` is true and `id` was not found,
/// this indicates an invalid marktree state - the original only asserts
/// in debug builds (and even then, only outside `RELDEBUG` builds, per
/// its own `TODO(bfredl)` comment about this being seen to fail for end
/// users); mirrored here as a `debug_assert!` (compiled out in release,
/// exactly like the original's `assert` under `#ifndef RELDEBUG`).
fn unintersect_node(x: &mut Intersection, id: u64, strict: bool) {
    debug_assert!(id & MARKTREE_END_FLAG == 0);
    if let Ok(i) = x.binary_search(&id) {
        x.remove(i);
    } else if strict {
        debug_assert!(false, "unintersect_node: id not found in intersection set");
    }
}

/// `intersect_merge`: merges the sorted sets `x` and `y` into `m` (like a
/// merge-sort merge step; duplicates - an id present in both - are kept
/// only once, matching set-union semantics).
fn intersect_merge(m: &mut Intersection, x: &Intersection, y: &Intersection) {
    m.clear();
    m.reserve(x.len() + y.len());
    let (mut i, mut j) = (0usize, 0usize);
    while i < x.len() && j < y.len() {
        match x[i].cmp(&y[j]) {
            std::cmp::Ordering::Less => {
                m.push(x[i]);
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                m.push(y[j]);
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                m.push(x[i]);
                i += 1;
                j += 1;
            }
        }
    }
    m.extend_from_slice(&x[i..]);
    m.extend_from_slice(&y[j..]);
}

/// `intersect_mov`: moves every id in `y` that is `< pivot` out of `y` and
/// into `x` (used when splitting a node's intersection set across the new
/// left/right halves during a tree split/merge).
fn intersect_mov(x: &mut Intersection, y: &mut Intersection, pivot: u64) {
    let split = y.partition_point(|&v| v < pivot);
    x.extend(y.drain(..split));
}

/// `intersect_common`: `i` becomes the sorted set-intersection of `x` and
/// `y` (ids present in both).
fn intersect_common(i: &mut Intersection, x: &Intersection, y: &Intersection) {
    i.clear();
    let (mut a, mut b) = (0usize, 0usize);
    while a < x.len() && b < y.len() {
        match x[a].cmp(&y[b]) {
            std::cmp::Ordering::Less => a += 1,
            std::cmp::Ordering::Greater => b += 1,
            std::cmp::Ordering::Equal => {
                i.push(x[a]);
                a += 1;
                b += 1;
            }
        }
    }
}

/// `intersect_add`: adds every id of `y` into `x` (set union, in place).
fn intersect_add(x: &mut Intersection, y: &Intersection) {
    let merged = {
        let mut m = Intersection::new();
        intersect_merge(&mut m, x, y);
        m
    };
    *x = merged;
}

/// `intersect_sub`: removes every id of `y` from `x` (set difference, in
/// place).
fn intersect_sub(x: &mut Intersection, y: &Intersection) {
    x.retain(|v| y.binary_search(v).is_err());
}

// --- raw-pointer child/meta accessors ---
//
// The original's `#define ptr s->i_ptr` / `#define meta s->i_meta` let
// functions write `x->ptr[i]` / `x->meta[i]` directly, where `s` is the
// flexible array member `struct mtnode_inner_s s[]` (array-to-pointer decay
// makes `x->s->i_ptr` == `x->s[0].i_ptr` valid C). These helpers are the
// literal equivalent for `MtNode.inner: Option<Box<MtNodeInner>>`.

/// Reads `x->ptr[i]`.
///
/// # Safety
/// `x` must be non-null/valid and currently an internal node (`level !=
/// 0`) with at least `i + 1` live child slots - exactly the same
/// precondition the original's unchecked macro access has.
#[inline]
unsafe fn node_child(x: *mut MtNode, i: usize) -> *mut MtNode {
    // SAFETY: forwarded from caller.
    unsafe { (*x).inner.as_ref().expect("internal node must have `inner`").i_ptr[i] }
}

/// Writes `x->ptr[i] = child`.
///
/// # Safety
/// Same as [`node_child`].
#[inline]
unsafe fn node_set_child(x: *mut MtNode, i: usize, child: *mut MtNode) {
    // SAFETY: forwarded from caller.
    unsafe { (*x).inner.as_mut().expect("internal node must have `inner`").i_ptr[i] = child };
}

/// Reads `x->meta[i]` (a `Copy` array, so this returns an independent copy
/// rather than a reference - avoids any aliasing concerns entirely).
///
/// # Safety
/// Same as [`node_child`].
#[inline]
unsafe fn node_meta(x: *mut MtNode, i: usize) -> [u32; K_MT_META_COUNT] {
    // SAFETY: forwarded from caller.
    unsafe { (*x).inner.as_ref().expect("internal node must have `inner`").i_meta[i] }
}

/// Writes `x->meta[i] = meta`.
///
/// # Safety
/// Same as [`node_child`].
#[inline]
unsafe fn node_set_meta(x: *mut MtNode, i: usize, meta: [u32; K_MT_META_COUNT]) {
    // SAFETY: forwarded from caller.
    unsafe { (*x).inner.as_mut().expect("internal node must have `inner`").i_meta[i] = meta };
}

/// `bubble_up`: `x` is a node which shrunk, or is half of a split. This
/// means that intervals which previously intersected all of `x`'s
/// (current) child nodes now instead intersect `x` itself - so the part
/// common to every child's intersection set is moved up onto `x`.
///
/// # Safety
/// `x` must be a valid internal-node pointer (`level != 0`) with live
/// children in `inner.i_ptr[0..=n]`.
unsafe fn bubble_up(x: *mut MtNode) {
    // SAFETY: caller guarantees `x` is a valid internal node.
    let n = unsafe { (*x).n };
    // Due to invariants, the largest subset common to _all_ subnodes is the
    // intersection between the first and the last.
    let first = unsafe { node_child(x, 0) };
    let last = unsafe { node_child(x, n as usize) };
    let mut xi = Intersection::new();
    // SAFETY: `first`/`last` are live children of `x` (see above).
    unsafe { intersect_common(&mut xi, &(*first).intersect, &(*last).intersect) };
    if !xi.is_empty() {
        for i in 0..=(n as usize) {
            // SAFETY: `i` indexes a live child of `x`.
            let child = unsafe { node_child(x, i) };
            unsafe { intersect_sub(&mut (*child).intersect, &xi) };
        }
        // SAFETY: `x` itself is valid.
        unsafe { intersect_add(&mut (*x).intersect, &xi) };
    }
}

/// `pseudo_index`: a monotonic "position" for node `x`'s slot `i` (`i` may
/// be `x.n` to mean "just past the last key"), obtained by packing each
/// ancestor's child index into a fixed-width field while walking up to the
/// root. Used only to compare "how far left/right" two tree locations are
/// relative to each other, never as a real position.
///
/// # Safety
/// `x` must be a valid node pointer, and every `parent` pointer reachable
/// by walking up from it must likewise be valid (or null at the root).
unsafe fn pseudo_index(x: *mut MtNode, i: i32) -> u64 {
    // SAFETY: caller guarantees `x` is valid.
    let mut off = crate::marktree_defs::MT_LOG2_BRANCH as u32 * unsafe { (*x).level } as u32;
    let mut index: u64 = 0;
    let mut cur = x;
    let mut cur_i = i;
    while !cur.is_null() {
        index |= ((cur_i + 1) as u64) << off;
        off += crate::marktree_defs::MT_LOG2_BRANCH as u32;
        // SAFETY: `cur` is valid (loop invariant).
        cur_i = unsafe { (*cur).p_idx as i32 };
        cur = unsafe { (*cur).parent };
    }
    index
}

/// `pseudo_index_for_id`: the [`pseudo_index`] of the node/slot currently
/// holding lookup id `id`, or `0` if `id` isn't present (a valid
/// pseudo-index is never zero). If `sloppy` and the node is a leaf, all
/// keys in that leaf are treated as having the same index (an
/// optimization the original documents the same way).
///
/// # Safety
/// Every node reachable from `b.id2node`'s stored pointers must be valid.
unsafe fn pseudo_index_for_id(b: &MarkTree, id: u64, sloppy: bool) -> u64 {
    let n = match lookup_id2node(b, id) {
        Some(n) => n,
        None => return 0,
    };
    // SAFETY: `n` came from `id2node`, which only ever stores valid
    // pointers into this same tree.
    let (level, node_n) = unsafe { ((*n).level, (*n).n) };
    let mut i: i32 = 0;
    if level != 0 || !sloppy {
        while i < node_n {
            // SAFETY: `n` is valid; `i < node_n` is in bounds.
            let key = unsafe { &(*n).key[i as usize] };
            if mt_lookup_key(key) == id {
                break;
            }
            i += 1;
        }
        debug_assert!(i < node_n);
        if level != 0 {
            i += 1; // internal key i comes after ptr[i]
        }
    }
    unsafe { pseudo_index(n, i) }
}

/// `split_node`: `x` must be an internal node that is not full; `x.ptr[i]`
/// (aliased here as `y`) must be a full node (`y.n == 2*T - 1`). Splits
/// `y` into `y` (kept, shrunk to the left half) and a freshly allocated
/// `z` (the right half), promoting `y`'s middle key up into `x` at index
/// `i`. `next` is the key about to be inserted (by the caller, right after
/// this split) - only consulted to detect the tricky case of splitting a
/// node in between a pair's start and end key (see the original's own
/// comment, preserved below).
///
/// # Safety
/// `x` must be a valid internal, non-full node pointer; `x.ptr[i]` must be
/// a valid, full node pointer.
unsafe fn split_node(b: &mut MarkTree, x: *mut MtNode, i: i32, next: &MtKey) {
    const T: i32 = crate::marktree_defs::MT_BRANCH_FACTOR as i32;

    // SAFETY: caller guarantees `x.ptr[i]` is valid.
    let y = unsafe { node_child(x, i as usize) };
    // SAFETY: `y` is valid (see above).
    let y_level = unsafe { (*y).level };
    let z = marktree_alloc_node(b, y_level != 0);
    // SAFETY: `z` was just allocated by us.
    unsafe {
        (*z).level = y_level;
        (*z).n = T - 1;
    }

    // Tricky: we might split a node in between inserting the start node and
    // the end node of the same pair. Then we must not intersect this id yet
    // (done later in marktree_intersect_pair).
    let last_start =
        if mt_end(next) { mt_lookup_id(next.ns, next.id, false) } else { MARKTREE_END_FLAG };

    // No alloc in the common case (less than 4 intersects).
    // SAFETY: `y`/`z` are both valid.
    unsafe { (*z).intersect = (*y).intersect.clone() };

    if y_level == 0 {
        // SAFETY: `y` is valid, a leaf.
        let pi = unsafe { pseudo_index(y, 0) }; // note: sloppy pseudo-index
        for j in 0..T {
            // SAFETY: `y` is valid; `j < T <= y.n` (y is full, n == 2T-1).
            let k = unsafe { &(*y).key[j as usize] };
            let pi_end = pseudo_index_for_id(b, mt_lookup_id(k.ns, k.id, true), true);
            if mt_start(k) && pi_end > pi && mt_lookup_key(k) != last_start {
                let id = mt_lookup_id(k.ns, k.id, false);
                // SAFETY: `z` is valid.
                unsafe { intersect_node(&mut (*z).intersect, id) };
            }
        }

        // Note: y->key[T-1] is moved up and thus checked for both.
        for j in (T - 1)..(T * 2 - 1) {
            // SAFETY: same as above.
            let k = unsafe { &(*y).key[j as usize] };
            let pi_start = pseudo_index_for_id(b, mt_lookup_id(k.ns, k.id, false), true);
            if mt_end(k) && pi_start > 0 && pi_start < pi {
                let id = mt_lookup_id(k.ns, k.id, false);
                // SAFETY: `y` is valid.
                unsafe { intersect_node(&mut (*y).intersect, id) };
            }
        }
    }

    // Move y's upper (T-1) keys into z (a genuine move, not a clone: the
    // original's memcpy just copies bytes and never reads y->key[T..] again
    // once y->n shrinks to T-1 below - matching that, we swap in harmless
    // defaults at the vacated slots rather than aliasing/cloning them).
    for j in 0..(T - 1) {
        // SAFETY: `y`/`z` valid; indices in bounds (y is full, z freshly
        // allocated with default keys at every slot).
        unsafe {
            (*z).key[j as usize] = std::mem::take(&mut (*y).key[(T + j) as usize]);
        }
        // SAFETY: `z` is valid; `refkey` requires `i < (*z).n`, and z.n ==
        // T-1 was set above.
        unsafe { refkey(b, z, j as usize) };
    }
    if y_level != 0 {
        // SAFETY: `y`/`z` valid internal nodes (y_level != 0 implies both
        // have `inner` populated - z was allocated with `internal = true`
        // above since we passed `y_level != 0`).
        unsafe {
            for j in 0..T {
                let child = node_child(y, (T + j) as usize);
                node_set_child(z, j as usize, child);
                node_set_meta(z, j as usize, node_meta(y, (T + j) as usize));
            }
            for j in 0..T {
                let child = node_child(z, j as usize);
                (*child).parent = z;
                (*child).p_idx = j as i16;
            }
        }
    }
    // SAFETY: `y` valid.
    unsafe { (*y).n = T - 1 };

    // SAFETY: `x` valid internal node, not full (caller guarantee); shifting
    // children/meta right by one starting at i+1 to make room for z at i+1.
    let old_n = unsafe { (*x).n };
    let shift_count = (old_n - i) as usize;
    unsafe {
        let inner = (*x).inner.as_mut().expect("internal node must have `inner`");
        inner.i_ptr.copy_within((i as usize + 1)..(i as usize + 1 + shift_count), i as usize + 2);
        inner.i_meta.copy_within((i as usize + 1)..(i as usize + 1 + shift_count), i as usize + 2);
    }
    unsafe { node_set_child(x, i as usize + 1, z) };
    // SAFETY: `z` valid.
    let z_meta = unsafe { meta_describe_node(&*z) };
    unsafe { node_set_meta(x, i as usize + 1, z_meta) };
    unsafe { (*z).parent = x }; // == y->parent
    for j in (i + 1)..(old_n + 2) {
        // SAFETY: `x` valid; `j` indexes a live (possibly just-shifted)
        // child slot of `x`.
        let child = unsafe { node_child(x, j as usize) };
        unsafe { (*child).p_idx = j as i16 };
    }

    // Shift x's keys right by one starting at i (move, not clone - see the
    // note above `z`'s key move; must go highest-index-first since dst > src).
    for j in (i..old_n).rev() {
        // SAFETY: `x` valid; indices in bounds.
        unsafe {
            let moved = std::mem::take(&mut (*x).key[j as usize]);
            (*x).key[j as usize + 1] = moved;
        }
    }

    // Move key to internal layer.
    // SAFETY: `x`/`y` valid; slot `i` of x.key was just vacated above.
    unsafe {
        (*x).key[i as usize] = std::mem::take(&mut (*y).key[(T - 1) as usize]);
    }
    unsafe { refkey(b, x, i as usize) };
    unsafe { (*x).n += 1 };

    // SAFETY: `x` valid.
    let meta_inc = meta_describe_key(unsafe { &(*x).key[i as usize] });
    {
        let mut meta_i = unsafe { node_meta(x, i as usize) };
        let meta_ip1 = unsafe { node_meta(x, i as usize + 1) };
        for m in 0..K_MT_META_COUNT {
            // y used to contain all of z and x->key[i]; discount those.
            meta_i[m] -= meta_ip1[m] + meta_inc[m];
        }
        unsafe { node_set_meta(x, i as usize, meta_i) };
    }

    // SAFETY: `x`/`z` valid.
    let pivot_pos = unsafe { (*x).key[i as usize].pos };
    for j in 0..(T - 1) {
        unsafe { relative(pivot_pos, &mut (*z).key[j as usize].pos) };
    }
    if i > 0 {
        // SAFETY: `x` valid; `i - 1 >= 0`.
        unsafe {
            let base = (*x).key[i as usize - 1].pos;
            unrelative(base, &mut (*x).key[i as usize].pos);
        }
    }

    if y_level != 0 {
        // SAFETY: `y`/`z` valid internal nodes.
        unsafe {
            bubble_up(y);
            bubble_up(z);
        }
    }
}

/// `marktree_putp_aux`: inserts `k` (with position already relative to
/// whatever base applies to `x`) into the subtree rooted at `x`, which
/// must not be full. `meta_inc` is the (already-computed, read-only) meta
/// contribution of `k` itself, added into every ancestor's cached child
/// meta count on the way back up.
///
/// # Safety
/// `x` must be a valid, non-full node pointer.
unsafe fn marktree_putp_aux(b: &mut MarkTree, x: *mut MtNode, mut k: MtKey, meta_inc: &[u32; K_MT_META_COUNT]) {
    const T: i32 = crate::marktree_defs::MT_BRANCH_FACTOR as i32;
    // TODO(bfredl) (kept from the original): ugh, make sure this is the
    // _last_ valid (pos, gravity) position, to minimize movement.
    // SAFETY: `x` is valid (caller guarantee).
    let mut i = unsafe { marktree_getp_aux(&*x, &k, None) } + 1;
    // SAFETY: `x` is valid.
    let level = unsafe { (*x).level };
    if level == 0 {
        // SAFETY: `x` valid; `old_n` read before mutation.
        let old_n = unsafe { (*x).n };
        if i != old_n {
            for j in (i..old_n).rev() {
                unsafe {
                    let moved = std::mem::take(&mut (*x).key[j as usize]);
                    (*x).key[j as usize + 1] = moved;
                }
            }
        }
        unsafe { (*x).key[i as usize] = k };
        unsafe { refkey(b, x, i as usize) };
        unsafe { (*x).n += 1 };
    } else {
        // SAFETY: `x` valid internal node.
        let child = unsafe { node_child(x, i as usize) };
        // SAFETY: `child` valid.
        if unsafe { (*child).n } == 2 * T - 1 {
            unsafe { split_node(b, x, i, &k) };
            // SAFETY: `x` valid; slot `i` now holds the freshly promoted key.
            if key_cmp(&k, unsafe { &(*x).key[i as usize] }) > 0 {
                i += 1;
            }
        }
        if i > 0 {
            // SAFETY: `x` valid.
            let base = unsafe { (*x).key[i as usize - 1].pos };
            relative(base, &mut k.pos);
        }
        // SAFETY: `x` valid; `x.ptr[i]` valid (possibly re-fetched after
        // split_node above changed which node lives at slot i).
        let child = unsafe { node_child(x, i as usize) };
        unsafe { marktree_putp_aux(b, child, k, meta_inc) };
        // SAFETY: `x` valid.
        let mut meta_i = unsafe { node_meta(x, i as usize) };
        for m in 0..K_MT_META_COUNT {
            meta_i[m] += meta_inc[m];
        }
        unsafe { node_set_meta(x, i as usize, meta_i) };
    }
}

/// `marktree_put_key`: inserts a single raw key `k` into the tree (sets
/// `MT_FLAG_REAL` - "let's be real" per the original's own comment).
/// `marktree_put` (the paired-mark-aware public entry point) is not yet
/// translated - it additionally needs `marktree_lookup`/
/// `marktree_intersect_pair`, deferred to the iterator/lookup stage.
pub fn marktree_put_key(b: &mut MarkTree, mut k: MtKey) {
    const T: i32 = crate::marktree_defs::MT_BRANCH_FACTOR as i32;
    k.flags |= MT_FLAG_REAL;
    if b.root.is_null() {
        b.root = marktree_alloc_node(b, true);
    }
    let mut r = b.root;
    // SAFETY: `r` is non-null (just ensured above) and valid.
    if unsafe { (*r).n } == 2 * T - 1 {
        let s = marktree_alloc_node(b, true);
        b.root = s;
        // SAFETY: `s`/`r` valid.
        unsafe {
            (*s).level = (*r).level + 1;
            (*s).n = 0;
            node_set_child(s, 0, r);
            node_set_meta(s, 0, b.meta_root);
            (*r).parent = s;
            (*r).p_idx = 0;
        }
        unsafe { split_node(b, s, 0, &k) };
        r = s;
    }

    let meta_inc = meta_describe_key(&k);
    // SAFETY: `r` is a valid, non-full root-or-new-root node.
    unsafe { marktree_putp_aux(b, r, k, &meta_inc) };
    for (dst, src) in b.meta_root.iter_mut().zip(meta_inc.iter()) {
        *dst += src;
    }
    b.n_keys += 1;
}

/// `marktree_put_test`: convenience entry point used by the original's own
/// unit tests, translated as a thin wrapper matching its shape. Only the
/// unpaired case (`end_row < 0`) is currently reachable, since
/// [`marktree_put`] (needed for `end_row >= 0`) is not yet translated.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn marktree_put_test(
    b: &mut MarkTree,
    ns: u32,
    id: u32,
    row: i32,
    col: i32,
    right_gravity: bool,
    meta_inline: bool,
) {
    let mut flags = mt_flags(right_gravity, false, false, false);
    // The specific choice is irrelevant here, we pick one counted decor
    // type to test the counting and filtering logic.
    if meta_inline {
        flags |= MT_FLAG_DECOR_VIRT_TEXT_INLINE;
    }
    let key = MtKey {
        pos: MtPos::new(row, col),
        ns,
        id,
        flags,
        decor_data: crate::decoration_defs::DecorInlineData { hl: DecorHighlightInline::default() },
    };
    marktree_put_key(b, key);
}

/// `marktree_check`: validates every documented invariant of the tree
/// (used by the original's own test suite; translated here to serve the
/// same role for this crate's tests, rather than inventing new validation
/// logic). No-ops if `b.root` is null (an empty tree).
#[cfg(test)]
pub fn marktree_check(b: &MarkTree) {
    if b.root.is_null() {
        assert_eq!(b.n_keys, 0);
        assert_eq!(b.n_nodes, 0);
        return;
    }
    let mut last = MtPos::default();
    let mut last_right = false;
    // SAFETY: `b.root` is non-null (checked above) and, by the tree's own
    // invariant, valid.
    let nkeys = unsafe { marktree_check_node(b, b.root, &mut last, &mut last_right, &b.meta_root) };
    assert_eq!(b.n_keys, nkeys);
}

/// `marktree_check_node`: recursively validates node `x` and its subtree,
/// returning the total number of real keys found (including `x`'s own).
/// `last`/`last_right` thread the previous key's absolute position/gravity
/// through an in-order traversal so each new key can be checked `>=` it;
/// `meta_node_ref` is the meta count the *parent* has cached for `x`,
/// cross-checked against a freshly recomputed count.
///
/// # Safety
/// `x` must be a valid node pointer whose entire subtree is valid.
#[cfg(test)]
unsafe fn marktree_check_node(
    b: &MarkTree,
    x: *mut MtNode,
    last: &mut MtPos,
    last_right: &mut bool,
    meta_node_ref: &[u32; K_MT_META_COUNT],
) -> usize {
    const T: i32 = crate::marktree_defs::MT_BRANCH_FACTOR as i32;
    // SAFETY: caller guarantees `x` is valid.
    let (n, level) = unsafe { ((*x).n, (*x).level) };
    assert!(n < 2 * T);
    // TODO(bfredl) (kept from the original): too strict if checking "in
    // repair" post-delete tree.
    assert!(n >= if x != b.root { T - 1 } else { 0 });
    let mut n_keys = n as usize;

    for i in 0..n {
        if level != 0 {
            let child = unsafe { node_child(x, i as usize) };
            let child_meta = unsafe { node_meta(x, i as usize) };
            n_keys += unsafe { marktree_check_node(b, child, last, last_right, &child_meta) };
        } else {
            *last = MtPos::default();
        }
        if i > 0 {
            // SAFETY: `x` valid.
            let base = unsafe { (*x).key[i as usize - 1].pos };
            unrelative(base, last);
        }
        // SAFETY: `x` valid.
        let key_pos = unsafe { (*x).key[i as usize].pos };
        assert!(pos_leq(*last, key_pos));
        if last.row == key_pos.row && last.col == key_pos.col {
            let key_right = unsafe { mt_right(&(*x).key[i as usize]) };
            assert!(!*last_right || key_right);
        }
        *last_right = unsafe { mt_right(&(*x).key[i as usize]) };
        assert!(key_pos.col >= 0);
        let key_id = unsafe { mt_lookup_key(&(*x).key[i as usize]) };
        assert_eq!(lookup_id2node(b, key_id), Some(x));
    }

    if level != 0 {
        let last_child = unsafe { node_child(x, n as usize) };
        let last_meta = unsafe { node_meta(x, n as usize) };
        n_keys += unsafe { marktree_check_node(b, last_child, last, last_right, &last_meta) };
        // SAFETY: `x` valid; `n >= 1` for an internal node.
        let base = unsafe { (*x).key[n as usize - 1].pos };
        unrelative(base, last);

        for i in 0..=(n as usize) {
            let child = unsafe { node_child(x, i) };
            // SAFETY: `child` valid.
            assert_eq!(unsafe { (*child).parent }, x);
            assert_eq!(unsafe { (*child).p_idx }, i as i16);
            assert_eq!(unsafe { (*child).level }, level - 1);
            // PARANOIA: check no double node ref.
            for j in 0..i {
                assert_ne!(child, unsafe { node_child(x, j) });
            }
        }
    } else if n > 0 {
        // SAFETY: `x` valid.
        *last = unsafe { (*x).key[n as usize - 1].pos };
    }

    // SAFETY: `x` valid.
    let meta_node = unsafe { meta_describe_node(&*x) };
    assert_eq!(meta_node_ref, &meta_node);

    n_keys
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

    #[test]
    fn compose_same_row_adds_col_only() {
        let mut base = p(5, 10);
        compose(&mut base, p(0, 3));
        assert_eq!(base, p(5, 13));
    }

    #[test]
    fn compose_nonzero_row_shifts_row_and_replaces_col() {
        let mut base = p(5, 10);
        compose(&mut base, p(2, 3));
        assert_eq!(base, p(7, 3));
    }

    fn key_at(row: i32, col: i32, flags: u16) -> MtKey {
        MtKey { pos: p(row, col), ns: 0, id: 0, flags, decor_data: crate::decoration_defs::DecorInlineData { hl: DecorHighlightInline::default() } }
    }

    #[test]
    fn key_cmp_orders_by_row_then_col() {
        assert_eq!(key_cmp(&key_at(1, 5, 0), &key_at(1, 5, 0)), 0);
        assert!(key_cmp(&key_at(1, 5, 0), &key_at(1, 6, 0)) < 0);
        assert!(key_cmp(&key_at(2, 0, 0), &key_at(1, 9, 0)) > 0);
    }

    #[test]
    fn key_cmp_breaks_ties_using_flag_mask() {
        // Same position: MT_FLAG_RIGHT_GRAVITY set should sort after unset.
        let a = key_at(1, 1, 0);
        let b = key_at(1, 1, MT_FLAG_RIGHT_GRAVITY);
        assert!(key_cmp(&a, &b) < 0);
        assert!(key_cmp(&b, &a) > 0);
        // Flags outside the comparison mask (e.g. MT_FLAG_NO_UNDO) must not
        // affect ordering.
        let c = key_at(1, 1, MT_FLAG_NO_UNDO);
        assert_eq!(key_cmp(&a, &c), 0);
    }

    fn leaf_node_with_keys(keys: &[(i32, i32)]) -> MtNode {
        let mut node = MtNode {
            n: keys.len() as i32,
            level: 0,
            p_idx: 0,
            intersect: Intersection::new(),
            parent: std::ptr::null_mut(),
            key: std::array::from_fn(|_| MtKey::default()),
            inner: None,
        };
        for (i, &(row, col)) in keys.iter().enumerate() {
            node.key[i] = key_at(row, col, MT_FLAG_REAL);
        }
        node
    }

    #[test]
    fn getp_aux_on_empty_node_returns_minus_one_no_match() {
        let node = leaf_node_with_keys(&[]);
        let mut m = true;
        let idx = marktree_getp_aux(&node, &key_at(0, 0, MT_FLAG_REAL), Some(&mut m));
        assert_eq!(idx, -1);
        assert!(!m);
    }

    #[test]
    fn getp_aux_finds_exact_match() {
        let node = leaf_node_with_keys(&[(1, 0), (2, 0), (3, 0)]);
        let mut m = false;
        let idx = marktree_getp_aux(&node, &key_at(2, 0, MT_FLAG_REAL), Some(&mut m));
        assert_eq!(idx, 1);
        assert!(m);
    }

    #[test]
    fn getp_aux_returns_insertion_point_when_absent() {
        let node = leaf_node_with_keys(&[(1, 0), (3, 0), (5, 0)]);
        let mut m = true;
        // (2,0) isn't present; should return the position of the largest
        // key strictly less than it (index 0, i.e. (1,0)).
        let idx = marktree_getp_aux(&node, &key_at(2, 0, MT_FLAG_REAL), Some(&mut m));
        assert_eq!(idx, 0);
        assert!(!m);
        // A key larger than everything present returns x.n - 1.
        let idx2 = marktree_getp_aux(&node, &key_at(9, 0, MT_FLAG_REAL), None);
        assert_eq!(idx2, 2);
    }

    #[test]
    fn meta_describe_key_counts_only_real_non_end_non_invalid_marks() {
        let real_inline = key_at(0, 0, MT_FLAG_REAL | MT_FLAG_DECOR_VIRT_TEXT_INLINE);
        let counts = meta_describe_key(&real_inline);
        assert_eq!(counts[MetaIndex::Inline as usize], 1);
        assert_eq!(counts[MetaIndex::Lines as usize], 0);

        // End keys don't contribute (described via their start key instead).
        let end_inline = key_at(0, 0, MT_FLAG_REAL | MT_FLAG_END | MT_FLAG_DECOR_VIRT_TEXT_INLINE);
        let counts_end = meta_describe_key(&end_inline);
        assert_eq!(counts_end[MetaIndex::Inline as usize], 0);

        // Invalid keys don't contribute either.
        let invalid_inline = key_at(0, 0, MT_FLAG_REAL | MT_FLAG_INVALID | MT_FLAG_DECOR_VIRT_TEXT_INLINE);
        let counts_invalid = meta_describe_key(&invalid_inline);
        assert_eq!(counts_invalid[MetaIndex::Inline as usize], 0);
    }

    #[test]
    fn meta_describe_node_sums_leaf_keys() {
        let mut node = leaf_node_with_keys(&[(0, 0), (1, 0)]);
        node.key[0] = key_at(0, 0, MT_FLAG_REAL | MT_FLAG_DECOR_SIGNHL);
        node.key[1] = key_at(1, 0, MT_FLAG_REAL | MT_FLAG_DECOR_SIGNHL);
        let counts = meta_describe_node(&node);
        assert_eq!(counts[MetaIndex::SignHl as usize], 2);
    }

    #[test]
    fn meta_has_respects_filter_mask() {
        let mut counts = [0u32; K_MT_META_COUNT];
        counts[MetaIndex::Lines as usize] = 3;
        let select_lines: [u32; K_MT_META_COUNT] =
            std::array::from_fn(|i| if i == MetaIndex::Lines as usize { crate::marktree_defs::MT_FILTER_SELECT } else { 0 });
        let select_signhl: [u32; K_MT_META_COUNT] =
            std::array::from_fn(|i| if i == MetaIndex::SignHl as usize { crate::marktree_defs::MT_FILTER_SELECT } else { 0 });
        assert!(meta_has(&counts, &select_lines));
        assert!(!meta_has(&counts, &select_signhl));
    }

    #[test]
    fn intersection_has_matches_sorted_semantics() {
        let x: Intersection = vec![10, 20, 30];
        assert!(intersection_has(&x, 20));
        assert!(!intersection_has(&x, 25));
        assert!(!intersection_has(&x, 5)); // less than everything
        assert!(!intersection_has(&x, 40)); // greater than everything
    }

    #[test]
    fn intersect_node_keeps_sorted_order() {
        let mut x: Intersection = vec![10, 30];
        intersect_node(&mut x, 20);
        assert_eq!(x, vec![10, 20, 30]);
        intersect_node(&mut x, 40);
        assert_eq!(x, vec![10, 20, 30, 40]);
        intersect_node(&mut x, 4); // ids are start-ids (even; MARKTREE_END_FLAG unset)
        assert_eq!(x, vec![4, 10, 20, 30, 40]);
    }

    #[test]
    fn unintersect_node_removes_present_id_only() {
        let mut x: Intersection = vec![10, 20, 30];
        unintersect_node(&mut x, 20, true);
        assert_eq!(x, vec![10, 30]);
        // Removing an absent id with strict=false is a silent no-op.
        unintersect_node(&mut x, 98, false);
        assert_eq!(x, vec![10, 30]);
    }

    #[test]
    fn intersect_merge_unions_two_sorted_sets_dedup() {
        let x: Intersection = vec![1, 3, 5];
        let y: Intersection = vec![2, 3, 6];
        let mut m = Intersection::new();
        intersect_merge(&mut m, &x, &y);
        assert_eq!(m, vec![1, 2, 3, 5, 6]);
    }

    #[test]
    fn intersect_mov_moves_ids_below_pivot() {
        let mut x: Intersection = vec![1];
        let mut y: Intersection = vec![2, 5, 10, 15];
        intersect_mov(&mut x, &mut y, 10);
        assert_eq!(x, vec![1, 2, 5]);
        assert_eq!(y, vec![10, 15]);
    }

    #[test]
    fn intersect_common_is_set_intersection() {
        let x: Intersection = vec![1, 2, 3, 4];
        let y: Intersection = vec![2, 4, 6];
        let mut i = Intersection::new();
        intersect_common(&mut i, &x, &y);
        assert_eq!(i, vec![2, 4]);
    }

    #[test]
    fn intersect_add_is_union_in_place() {
        let mut x: Intersection = vec![1, 3];
        let y: Intersection = vec![2, 3, 4];
        intersect_add(&mut x, &y);
        assert_eq!(x, vec![1, 2, 3, 4]);
    }

    #[test]
    fn intersect_sub_is_set_difference_in_place() {
        let mut x: Intersection = vec![1, 2, 3, 4];
        let y: Intersection = vec![2, 4];
        intersect_sub(&mut x, &y);
        assert_eq!(x, vec![1, 3]);
    }

    #[test]
    fn mt_lookup_id_packs_ns_id_and_end_bit() {
        let start = mt_lookup_id(7, 3, false);
        let end = mt_lookup_id(7, 3, true);
        // "defined so that start and end of the same range have adjacent ids"
        assert_eq!(end, start + 1);
        // Different ns/id must not collide.
        assert_ne!(mt_lookup_id(7, 3, false), mt_lookup_id(7, 4, false));
        assert_ne!(mt_lookup_id(7, 3, false), mt_lookup_id(8, 3, false));
    }

    #[test]
    fn mt_key_predicates_read_expected_bits() {
        let paired_start = key_at(0, 0, MT_FLAG_REAL | MT_FLAG_PAIRED);
        assert!(mt_paired(&paired_start));
        assert!(mt_start(&paired_start));
        assert!(!mt_end(&paired_start));

        let paired_end = key_at(0, 0, MT_FLAG_REAL | MT_FLAG_PAIRED | MT_FLAG_END);
        assert!(mt_end(&paired_end));
        assert!(!mt_start(&paired_end)); // paired but not "start" since it IS the end

        let right = key_at(0, 0, MT_FLAG_RIGHT_GRAVITY);
        assert!(mt_right(&right));
        assert!(!mt_right(&paired_start));

        let signed = key_at(0, 0, MT_FLAG_DECOR_SIGNHL);
        assert!(mt_decor_any(&signed));
        assert!(mt_decor_sign(&signed));
        assert!(!mt_conceal_lines(&signed));

        let conceal = key_at(0, 0, MT_FLAG_DECOR_CONCEAL_LINES);
        assert!(mt_conceal_lines(&conceal));
    }

    #[test]
    fn mt_flags_builds_expected_bitmask() {
        assert_eq!(mt_flags(true, false, false, false), MT_FLAG_RIGHT_GRAVITY);
        assert_eq!(mt_flags(false, true, false, false), MT_FLAG_NO_UNDO);
        assert_eq!(mt_flags(false, false, true, false), MT_FLAG_INVALIDATE);
        assert_eq!(mt_flags(false, false, false, true), MT_FLAG_DECOR_EXT);
        assert_eq!(
            mt_flags(true, true, true, true),
            MT_FLAG_RIGHT_GRAVITY | MT_FLAG_NO_UNDO | MT_FLAG_INVALIDATE | MT_FLAG_DECOR_EXT
        );
    }

    #[test]
    fn mtpair_from_copies_start_and_reads_end_by_reference() {
        let start = key_at(1, 1, MT_FLAG_REAL);
        let end = key_at(5, 5, MT_FLAG_RIGHT_GRAVITY | MT_FLAG_END);
        let pair = mtpair_from(start.clone(), &end);
        assert_eq!(pair.start.pos, p(1, 1));
        assert_eq!(pair.end_pos, p(5, 5));
        assert!(pair.end_right_gravity);
        // `end` must still be usable afterwards (taken by reference).
        assert_eq!(end.pos, p(5, 5));
    }

    #[test]
    fn mt_decor_reads_highlight_branch_when_ext_flag_unset() {
        let key = key_at(0, 0, 0);
        match mt_decor(key) {
            DecorInline::Highlight(_) => {}
            DecorInline::Ext(_) => panic!("expected Highlight branch when MT_FLAG_DECOR_EXT unset"),
        }
    }

    #[test]
    fn mt_decor_reads_ext_branch_when_flag_set_and_clone_deep_copies_box() {
        let vt = crate::decoration_defs::DecorVirtText { col: 42, ..Default::default() };
        let ext = crate::decoration_defs::DecorExt { sh_idx: 99, vt: Some(Box::new(vt)) };
        let key = MtKey {
            pos: p(0, 0),
            ns: 1,
            id: 2,
            flags: MT_FLAG_DECOR_EXT,
            decor_data: crate::decoration_defs::DecorInlineData { ext: std::mem::ManuallyDrop::new(ext) },
        };

        // Clone must read the `ext` branch (not `hl`) since the flag is set.
        let cloned = key.clone();
        assert_eq!(cloned.flags, MT_FLAG_DECOR_EXT);

        // mt_decor_virt gives non-owning access without consuming `key`.
        let virt = mt_decor_virt(&key).expect("ext key should have virt text");
        assert_eq!(virt.col, 42);

        // Consuming mt_decor(key) yields the Ext branch with the same data,
        // and the Box was genuinely deep-cloned (independent allocation),
        // verified via Debug-format equality of the cloned key's ext data.
        match mt_decor(cloned) {
            DecorInline::Ext(e) => {
                assert_eq!(e.sh_idx, 99);
                assert_eq!(format!("{:?}", e.vt), format!("{:?}", ext_vt_for_test(42)));
            }
            DecorInline::Highlight(_) => panic!("expected Ext branch when MT_FLAG_DECOR_EXT set"),
        }
    }

    fn ext_vt_for_test(col: i32) -> Option<Box<crate::decoration_defs::DecorVirtText>> {
        Some(Box::new(crate::decoration_defs::DecorVirtText { col, ..Default::default() }))
    }

    #[test]
    fn node_alloc_free_lifecycle_tracks_n_nodes() {
        let mut tree = MarkTree::default();
        assert_eq!(tree.n_nodes, 0);

        let leaf = marktree_alloc_node(&mut tree, false);
        assert_eq!(tree.n_nodes, 1);
        // SAFETY: leaf is freshly allocated, non-null, and not used after free.
        unsafe {
            assert!((*leaf).inner.is_none());
            marktree_free_node(&mut tree, leaf);
        }
        assert_eq!(tree.n_nodes, 0);

        let internal = marktree_alloc_node(&mut tree, true);
        // SAFETY: internal is freshly allocated and not used after free.
        unsafe {
            assert!((*internal).inner.is_some());
            marktree_free_node(&mut tree, internal);
        }
    }

    #[test]
    fn free_subtree_recursively_frees_children_and_updates_n_nodes() {
        let mut tree = MarkTree::default();
        let root = marktree_alloc_node(&mut tree, true);
        let child0 = marktree_alloc_node(&mut tree, false);
        let child1 = marktree_alloc_node(&mut tree, false);
        // SAFETY: all three nodes are freshly allocated on `tree`; we wire
        // up a minimal 1-key, 2-child internal node by hand, matching the
        // invariant marktree_free_subtree relies on (n=1 => n+1=2 children).
        // Note: marktree_alloc_node's `internal` flag only controls whether
        // storage for `inner` is allocated (matching the original's
        // ILEN-vs-sizeof(MTNode) choice) - `level` itself always starts at
        // 0 (like the original's xcalloc) and must be set explicitly by
        // the caller, exactly as real callers (e.g. split_node) do.
        unsafe {
            (*root).level = 1;
            (*root).n = 1;
            let inner = (*root).inner.as_mut().unwrap();
            inner.i_ptr[0] = child0;
            inner.i_ptr[1] = child1;
        }
        assert_eq!(tree.n_nodes, 3);
        // SAFETY: root's subtree (root + child0 + child1) is a fully valid,
        // self-contained tree fragment allocated on `tree`, not used after.
        unsafe {
            marktree_free_subtree(&mut tree, root);
        }
        assert_eq!(tree.n_nodes, 0);
    }

    #[test]
    fn refkey_and_lookup_id2node_round_trip() {
        let mut tree = MarkTree::default();
        let node = marktree_alloc_node(&mut tree, false);
        // SAFETY: node is freshly allocated with n=0; we set one real key
        // at index 0 before refkey reads it.
        unsafe {
            (*node).n = 1;
            (*node).key[0] = key_at(3, 4, MT_FLAG_REAL);
            refkey(&mut tree, node, 0);
        }
        let id = mt_lookup_key(&key_at(3, 4, MT_FLAG_REAL));
        assert_eq!(lookup_id2node(&tree, id), Some(node));
        assert_eq!(lookup_id2node(&tree, id + 12345), None);
        // SAFETY: node has no children (leaf) and was allocated on `tree`.
        unsafe {
            marktree_free_node(&mut tree, node);
        }
    }

    /// Deterministic pseudo-shuffle of `0..n` (no `rand` dependency needed):
    /// multiplying by a prime coprime to `n` modulo `n` is a bijection, so
    /// this always visits every value in `0..n` exactly once, just not in
    /// ascending order - enough to exercise non-monotonic insertion orders.
    fn shuffled_range(n: i32) -> Vec<i32> {
        const MULT: i32 = 7919;
        (0..n).map(|i| (i * MULT) % n).collect()
    }

    #[test]
    fn put_key_small_scattered_batch_maintains_invariants() {
        let mut tree = MarkTree::default();
        for (i, &(row, col)) in [(5, 0), (1, 0), (9, 3), (3, 7), (7, 2)].iter().enumerate() {
            marktree_put_test(&mut tree, 0, i as u32, row, col, false, false);
        }
        assert_eq!(tree.n_keys, 5);
        marktree_check(&tree);
        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn put_key_forces_leaf_split_and_maintains_invariants() {
        // A leaf holds up to 2*T-1 = 19 keys; 25 forces at least one split.
        let mut tree = MarkTree::default();
        for i in 0..25 {
            marktree_put_test(&mut tree, 0, i as u32, i, 0, false, false);
        }
        assert_eq!(tree.n_keys, 25);
        marktree_check(&tree);
        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn put_key_many_keys_ascending_multiple_levels() {
        let mut tree = MarkTree::default();
        for i in 0..300 {
            marktree_put_test(&mut tree, 0, i as u32, i, 0, i % 2 == 0, false);
        }
        assert_eq!(tree.n_keys, 300);
        marktree_check(&tree);
        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn put_key_many_keys_shuffled_order_maintains_invariants() {
        let mut tree = MarkTree::default();
        for (id, row) in shuffled_range(300).into_iter().enumerate() {
            marktree_put_test(&mut tree, 0, id as u32, row, 0, false, false);
        }
        assert_eq!(tree.n_keys, 300);
        marktree_check(&tree);
        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn put_key_duplicate_positions_ordered_by_gravity_flag() {
        let mut tree = MarkTree::default();
        // Several distinct marks at the exact same (row, col): key_cmp must
        // break the tie consistently (by flags) without violating the
        // overall ordering invariant that marktree_check enforces.
        for id in 0..10 {
            marktree_put_test(&mut tree, 0, id, 5, 5, id % 2 == 0, false);
        }
        assert_eq!(tree.n_keys, 10);
        marktree_check(&tree);
        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn put_key_meta_root_tracks_inline_decor_count() {
        let mut tree = MarkTree::default();
        for i in 0..30 {
            // Every third key is marked as carrying inline virtual text.
            marktree_put_test(&mut tree, 0, i as u32, i, 0, false, i % 3 == 0);
        }
        marktree_check(&tree);
        let expected_inline = (0..30).filter(|i| i % 3 == 0).count() as u32;
        assert_eq!(tree.meta_root[MetaIndex::Inline as usize], expected_inline);
        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn put_key_across_many_namespaces_maintains_invariants() {
        let mut tree = MarkTree::default();
        let mut id = 0u32;
        for ns in 0..5 {
            for row in 0..40 {
                marktree_put_test(&mut tree, ns, id, row, ns as i32, false, false);
                id += 1;
            }
        }
        assert_eq!(tree.n_keys, 200);
        marktree_check(&tree);
        unsafe { marktree_clear(&mut tree) };
    }
}
