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
//! (`marktree_alloc_node`/`marktree_free_node`/`marktree_free_subtree`);
//! the id2node index helpers (`refkey`/`lookup_id2node`); the in-node
//! binary search (`marktree_getp_aux`); the meta-counting helpers
//! (`meta_describe_key(_inc)`/`meta_describe_node`/`meta_has`); and the
//! `Intersection` (sorted `Vec<u64>`) set-operation helpers
//! (`intersection_has`/`intersect_node`/`unintersect_node`/
//! `intersect_merge`/`intersect_mov`/`intersect_common`/`intersect_add`/
//! `intersect_sub`).
//!
//! Not yet translated (deferred - this is the hard part of the B-tree:
//! node splitting/merging/rebalancing with parent-pointer surgery,
//! deserving its own dedicated pass): `split_node`, `marktree_putp_aux`,
//! `marktree_put`, `marktree_put_key`, `bubble_up`, `merge_node`,
//! `pivot_left`/`pivot_right`, `marktree_del_itr`, `marktree_revise_meta`,
//! all `marktree_itr_*` iterator functions, `marktree_splice` and its
//! helpers (`check_damage`/`swap_keys`), `marktree_move`/
//! `marktree_move_region`/`marktree_restore_pair`, the id-based lookup
//! functions (`pseudo_index`/`pseudo_index_for_id`/`marktree_lookup`/
//! `marktree_lookup_ns`/`marktree_get_alt(pos)`), `marktree_intersect_pair`,
//! and the debug-only `marktree_check*`/`mt_inspect*`/`marktree_put_test`/
//! `marktree_del_pair_test` functions.

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
}
