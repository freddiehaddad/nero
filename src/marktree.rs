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
//! `split_node`, `marktree_putp_aux`, `marktree_put_key`, `marktree_put`);
//! core iterators (`marktree_itr_get(_ext)`/`first`/`last`/`next(_skip)`/
//! `prev`/`node_done`/`pos`/`current`, `itr_eq`); id-based lookup
//! (`marktree_lookup(_ns)`, `marktree_itr_set_node`/`fix_pos`,
//! `marktree_get_alt(pos)`); `marktree_intersect_pair`; and the full
//! deletion/rebalancing path (`merge_node`, `pivot_left`/`pivot_right`,
//! `marktree_del_itr`, `marktree_revise_meta`). Also translated, as
//! `#[cfg(test)]`-only (since they exist to validate the above rather
//! than being needed by any other translated file yet): the original's
//! own tree-invariant checker `marktree_check`/`marktree_check_node`, and
//! its `marktree_put_test`/`marktree_del_pair_test` unit-test helpers.
//!
//! Not yet translated (deferred - each deserving its own dedicated pass):
//! `marktree_splice` (the text-edit position-update algorithm) and its
//! helpers (`check_damage`/`swap_keys`); `marktree_move`/
//! `marktree_move_region`/`marktree_restore_pair`; the filter/overlap
//! iterator variants (`marktree_itr_get_filter`, `step_out_filter`,
//! `next_filter`, `check_filter`, `get_overlap`, `step_overlap` - used by
//! extmark.c's decoration-filtering, not needed before that file); and
//! the debug-only `mt_inspect*`/`marktree_check_intersections` functions.

// Several helpers here are `static` (private) in the original, matching
// the non-`pub` visibility kept here. Some have no caller yet since the
// rest of marktree.c's algorithm isn't translated - #[allow(dead_code)]
// instead of prematurely making them `pub` (which would misrepresent the
// original's intended visibility) or deleting them (which would lose
// verified, tested translation work ahead of its eventual use).
#![allow(dead_code)]

use crate::decoration_defs::{DecorHighlightInline, DecorInline, DecorVirtText};
use crate::map::Map;
use crate::marktree_defs::{
    Intersection, MarkTree, MarkTreeIter, MetaFilter, MetaIndex, MtKey, MtNode, MtPos, K_MT_META_COUNT,
};
use crate::types_defs::MtDamagePair;
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

/// `merge_node`: merges children `p.ptr[i]` and `p.ptr[i+1]` (`x`/`y`) of
/// `p`, along with the parent key `p.key[i]` that separated them, into a
/// single node (`x`, grown; `y` is freed). Returns the merged node.
///
/// # Safety
/// `p` must be a valid internal node with `p.ptr[i]`/`p.ptr[i+1]` valid
/// nodes such that `x.n + 1 + y.n <= 2*T - 1` (i.e. merging them doesn't
/// overflow a node - the original relies on both being minimally full,
/// `T - 1` keys each, plus the one parent key, totaling exactly `2*T -
/// 1`, the maximum).
unsafe fn merge_node(b: &mut MarkTree, p: *mut MtNode, i: i32) -> *mut MtNode {
    const T: i32 = crate::marktree_defs::MT_BRANCH_FACTOR as i32;
    // SAFETY: caller guarantees `p.ptr[i]`/`p.ptr[i+1]` are valid.
    let x = unsafe { node_child(p, i as usize) };
    let y = unsafe { node_child(p, i as usize + 1) };
    let mut mi = Intersection::new();
    // SAFETY: `x`/`y` valid.
    unsafe { intersect_merge(&mut mi, &mut (*x).intersect, &mut (*y).intersect) };

    // SAFETY: `x`/`p` valid; `x.n` is a fresh (default-valued) slot.
    let x_n = unsafe { (*x).n };
    unsafe { (*x).key[x_n as usize] = std::mem::take(&mut (*p).key[i as usize]) };
    unsafe { refkey(b, x, x_n as usize) };
    if i > 0 {
        // SAFETY: `p` valid; `i > 0`.
        let base = unsafe { (*p).key[i as usize - 1].pos };
        unsafe { relative(base, &mut (*x).key[x_n as usize].pos) };
    }

    // SAFETY: `x` valid.
    let meta_inc = meta_describe_key(unsafe { &(*x).key[x_n as usize] });

    // SAFETY: `x`/`y` valid; destination slots in `x` (indices `x_n+1..`)
    // are fresh, never-yet-used slots, and `y`'s keys are being genuinely
    // moved out (`y` is freed at the end of this function).
    let y_n = unsafe { (*y).n };
    for k in 0..y_n {
        unsafe {
            (*x).key[(x_n + 1 + k) as usize] = std::mem::take(&mut (*y).key[k as usize]);
            refkey(b, x, (x_n + 1 + k) as usize);
            let base = (*x).key[x_n as usize].pos;
            unrelative(base, &mut (*x).key[(x_n + 1 + k) as usize].pos);
        }
    }
    // SAFETY: `x` valid.
    if unsafe { (*x).level } != 0 {
        // SAFETY: `x`/`y` valid internal nodes.
        unsafe {
            let inner_y = (*y).inner.as_ref().expect("internal node must have `inner`");
            let y_ptrs: Vec<*mut MtNode> = inner_y.i_ptr[..=(y_n as usize)].to_vec();
            let y_metas: Vec<[u32; K_MT_META_COUNT]> = inner_y.i_meta[..=(y_n as usize)].to_vec();
            let inner_x = (*x).inner.as_mut().expect("internal node must have `inner`");
            for (offset, (&ptr, &meta)) in y_ptrs.iter().zip(y_metas.iter()).enumerate() {
                inner_x.i_ptr[x_n as usize + 1 + offset] = ptr;
                inner_x.i_meta[x_n as usize + 1 + offset] = meta;
            }
            let x_ids = (*x).intersect.clone();
            for k in 0..=x_n {
                for &id in &x_ids {
                    intersect_node(&mut (*node_child(x, k as usize)).intersect, id);
                }
            }
            let y_ids = (*y).intersect.clone();
            for ky in 0..=y_n {
                let k = x_n + ky + 1;
                let child = node_child(x, k as usize);
                (*child).parent = x;
                (*child).p_idx = k as i16;
                for &id in &y_ids {
                    intersect_node(&mut (*child).intersect, id);
                }
            }
        }
    }
    // SAFETY: `x` valid.
    unsafe { (*x).n += y_n + 1 };
    {
        // SAFETY: `p` valid.
        let mut meta_i = unsafe { node_meta(p, i as usize) };
        let meta_ip1 = unsafe { node_meta(p, i as usize + 1) };
        for m in 0..K_MT_META_COUNT {
            // x now contains everything of y plus old p->key[i].
            meta_i[m] += meta_ip1[m] + meta_inc[m];
        }
        unsafe { node_set_meta(p, i as usize, meta_i) };
    }

    // SAFETY: `p` valid; shifting out the now-removed key/child at index i/i+1.
    let p_n = unsafe { (*p).n };
    let shift_count = (p_n - i - 1) as usize;
    for j in (i as usize)..(i as usize + shift_count) {
        // SAFETY: `p` valid.
        unsafe {
            let moved = std::mem::take(&mut (*p).key[j + 1]);
            (*p).key[j] = moved;
        }
    }
    unsafe {
        let inner = (*p).inner.as_mut().expect("internal node must have `inner`");
        inner.i_ptr.copy_within((i as usize + 2)..(i as usize + 2 + shift_count), i as usize + 1);
        inner.i_meta.copy_within((i as usize + 2)..(i as usize + 2 + shift_count), i as usize + 1);
    }
    for j in (i + 1)..p_n {
        // SAFETY: `p` valid internal node.
        let child = unsafe { node_child(p, j as usize) };
        unsafe { (*child).p_idx = j as i16 };
    }
    unsafe { (*p).n -= 1 };
    unsafe { marktree_free_node(b, y) };

    // SAFETY: `x` valid; replacing its (now-exhausted) intersect set with
    // the common part computed at the very start (`kvi_move` in the
    // original - a plain move/replace here, no manual buffer-ownership
    // dance needed since `Intersection` is just a `Vec`).
    unsafe { (*x).intersect = mi };

    x
}

/// `pivot_right`: steals one key from `p.ptr[i]` (`x`) and gives it to
/// `p.ptr[i+1]` (`y`), rotating `p.key[i]` through the middle (used when
/// `y` underflowed but its left sibling `x` has a spare key to lend).
///
/// `p_pos` (the absolute position of the key just before `x`, or a dummy
/// key strictly less than any key inside `x` if `x` is the first leaf) is
/// unused in the original's own implementation (confirmed against the
/// upstream source - not a translation omission), kept only for signature
/// fidelity with its caller.
///
/// # Safety
/// `p` must be valid with `p.ptr[i]`/`p.ptr[i+1]` valid nodes, `x.n > T -
/// 1` (has a spare key to give up).
unsafe fn pivot_right(b: &mut MarkTree, p_pos: MtPos, p: *mut MtNode, i: i32) {
    let _ = p_pos; // unused in the original too - kept for signature parity
    // SAFETY: caller guarantees `p.ptr[i]`/`p.ptr[i+1]` are valid.
    let x = unsafe { node_child(p, i as usize) };
    let y = unsafe { node_child(p, i as usize + 1) };

    // Shift y's keys right by one to make room at index 0 (move, not
    // clone/copy: `y.key[]` slots being shifted own real data).
    let y_n = unsafe { (*y).n };
    for j in (0..y_n).rev() {
        // SAFETY: `y` valid.
        unsafe {
            let moved = std::mem::take(&mut (*y).key[j as usize]);
            (*y).key[j as usize + 1] = moved;
        }
    }
    // SAFETY: `y` valid.
    if unsafe { (*y).level } != 0 {
        unsafe {
            let inner = (*y).inner.as_mut().expect("internal node must have `inner`");
            inner.i_ptr.copy_within(0..(y_n as usize + 1), 1);
            inner.i_meta.copy_within(0..(y_n as usize + 1), 1);
            for j in 1..=(y_n as usize + 1) {
                (*inner.i_ptr[j]).p_idx = j as i16;
            }
        }
    }

    // SAFETY: `p`/`x`/`y` valid.
    unsafe {
        (*y).key[0] = std::mem::take(&mut (*p).key[i as usize]);
        refkey(b, y, 0);
        let x_n = (*x).n;
        (*p).key[i as usize] = std::mem::take(&mut (*x).key[x_n as usize - 1]);
        refkey(b, p, i as usize);
    }

    let meta_inc_y = meta_describe_key(unsafe { &(*y).key[0] });
    let meta_inc_x = meta_describe_key(unsafe { &(*p).key[i as usize] });
    {
        let mut meta_i = unsafe { node_meta(p, i as usize) };
        let mut meta_ip1 = unsafe { node_meta(p, i as usize + 1) };
        for m in 0..K_MT_META_COUNT {
            meta_ip1[m] += meta_inc_y[m];
            meta_i[m] -= meta_inc_x[m];
        }
        unsafe {
            node_set_meta(p, i as usize, meta_i);
            node_set_meta(p, i as usize + 1, meta_ip1);
        }
    }

    // SAFETY: `x` valid.
    if unsafe { (*x).level } != 0 {
        unsafe {
            let x_n = (*x).n;
            let moved_child = node_child(x, x_n as usize);
            let moved_meta = node_meta(x, x_n as usize);
            node_set_child(y, 0, moved_child);
            node_set_meta(y, 0, moved_meta);
            let mut meta_i = node_meta(p, i as usize);
            let mut meta_ip1 = node_meta(p, i as usize + 1);
            for m in 0..K_MT_META_COUNT {
                meta_ip1[m] += moved_meta[m];
                meta_i[m] -= moved_meta[m];
            }
            node_set_meta(p, i as usize, meta_i);
            node_set_meta(p, i as usize + 1, meta_ip1);
            (*moved_child).parent = y;
            (*moved_child).p_idx = 0;
        }
    }
    unsafe {
        (*x).n -= 1;
        (*y).n += 1;
    }
    if i > 0 {
        // SAFETY: `p` valid; `i > 0`.
        unsafe {
            let base = (*p).key[i as usize - 1].pos;
            unrelative(base, &mut (*p).key[i as usize].pos);
        }
    }
    // SAFETY: `p`/`y` valid.
    unsafe {
        let base = (*p).key[i as usize].pos;
        relative(base, &mut (*y).key[0].pos);
        let y_n = (*y).n;
        for k in 1..y_n {
            let base = (*y).key[0].pos;
            unrelative(base, &mut (*y).key[k as usize].pos);
        }
    }

    // Repair intersections of x.
    // SAFETY: `x` valid.
    if unsafe { (*x).level } != 0 {
        // SAFETY: `x`/`y` valid; `y.ptr[0]` was just moved from `x` above.
        unsafe {
            let mut d = Intersection::new();
            let y_ptr0 = node_child(y, 0);
            intersect_mov(&(*x).intersect, &mut (*y).intersect, &mut (*y_ptr0).intersect, &mut d);
            if !d.is_empty() {
                let y_n = (*y).n;
                for yi in 1..=y_n {
                    let child = node_child(y, yi as usize);
                    intersect_add(&mut (*child).intersect, &d);
                }
            }
            bubble_up(x);
        }
    } else {
        // If the last element of x used to be an end node, check if it now
        // covers all of x.
        // SAFETY: `p`/`x`/`y` valid.
        unsafe {
            if mt_end(&(*p).key[i as usize]) {
                let pi = pseudo_index(x, 0); // note: sloppy pseudo-index
                let start_id = mt_lookup_key_side(&(*p).key[i as usize], false);
                let pi_start = pseudo_index_for_id(b, start_id, true);
                if pi_start > 0 && pi_start < pi {
                    intersect_node(&mut (*x).intersect, start_id);
                }
            }
            if mt_start(&(*y).key[0]) {
                // No need for a check, just delete it if it was there.
                let id = mt_lookup_key(&(*y).key[0]);
                unintersect_node(&mut (*y).intersect, id, false);
            }
        }
    }
}

/// `pivot_left`: the mirror image of [`pivot_right`] - steals one key
/// from `p.ptr[i+1]` (`y`) and gives it to `p.ptr[i]` (`x`), rotating
/// `p.key[i]` through the middle.
///
/// # Safety
/// `p` must be valid with `p.ptr[i]`/`p.ptr[i+1]` valid nodes, `y.n > T -
/// 1` (has a spare key to give up).
unsafe fn pivot_left(b: &mut MarkTree, p_pos: MtPos, p: *mut MtNode, i: i32) {
    let _ = p_pos; // unused in the original too - kept for signature parity
    // SAFETY: caller guarantees `p.ptr[i]`/`p.ptr[i+1]` are valid.
    let x = unsafe { node_child(p, i as usize) };
    let y = unsafe { node_child(p, i as usize + 1) };

    // Reverse from how we "always" do it, but pivot_left is just the
    // inverse of pivot_right, so reverse it literally (per the original's
    // own comment).
    let y_n = unsafe { (*y).n };
    for k in 1..y_n {
        // SAFETY: `y` valid.
        unsafe {
            let base = (*y).key[0].pos;
            relative(base, &mut (*y).key[k as usize].pos);
        }
    }
    // SAFETY: `p`/`y` valid.
    unsafe {
        let base = (*p).key[i as usize].pos;
        unrelative(base, &mut (*y).key[0].pos);
    }
    if i > 0 {
        // SAFETY: `p` valid; `i > 0`.
        unsafe {
            let base = (*p).key[i as usize - 1].pos;
            relative(base, &mut (*p).key[i as usize].pos);
        }
    }

    // SAFETY: `p`/`x`/`y` valid.
    unsafe {
        let x_n = (*x).n;
        (*x).key[x_n as usize] = std::mem::take(&mut (*p).key[i as usize]);
        refkey(b, x, x_n as usize);
        (*p).key[i as usize] = std::mem::take(&mut (*y).key[0]);
        refkey(b, p, i as usize);
    }

    let meta_inc_x = meta_describe_key(unsafe { &(*x).key[(*x).n as usize] });
    let meta_inc_y = meta_describe_key(unsafe { &(*p).key[i as usize] });
    {
        let mut meta_i = unsafe { node_meta(p, i as usize) };
        let mut meta_ip1 = unsafe { node_meta(p, i as usize + 1) };
        for m in 0..K_MT_META_COUNT {
            meta_i[m] += meta_inc_x[m];
            meta_ip1[m] -= meta_inc_y[m];
        }
        unsafe {
            node_set_meta(p, i as usize, meta_i);
            node_set_meta(p, i as usize + 1, meta_ip1);
        }
    }

    // SAFETY: `x` valid.
    if unsafe { (*x).level } != 0 {
        unsafe {
            let x_n = (*x).n;
            let moved_child = node_child(y, 0);
            let moved_meta = node_meta(y, 0);
            node_set_child(x, x_n as usize + 1, moved_child);
            node_set_meta(x, x_n as usize + 1, moved_meta);
            let mut meta_i = node_meta(p, i as usize);
            let mut meta_ip1 = node_meta(p, i as usize + 1);
            for m in 0..K_MT_META_COUNT {
                meta_ip1[m] -= moved_meta[m];
                meta_i[m] += moved_meta[m];
            }
            node_set_meta(p, i as usize, meta_i);
            node_set_meta(p, i as usize + 1, meta_ip1);
            (*moved_child).parent = x;
            (*moved_child).p_idx = x_n as i16 + 1;
        }
    }
    // Shift y's keys/children left by one (removing the now-moved index 0).
    // SAFETY: `y` valid.
    for j in 0..(y_n - 1) {
        unsafe {
            let moved = std::mem::take(&mut (*y).key[j as usize + 1]);
            (*y).key[j as usize] = moved;
        }
    }
    if unsafe { (*y).level } != 0 {
        unsafe {
            let inner = (*y).inner.as_mut().expect("internal node must have `inner`");
            inner.i_ptr.copy_within(1..=(y_n as usize), 0);
            inner.i_meta.copy_within(1..=(y_n as usize), 0);
            for j in 0..(y_n as usize) {
                // note: last item deleted
                (*inner.i_ptr[j]).p_idx = j as i16;
            }
        }
    }
    unsafe {
        (*x).n += 1;
        (*y).n -= 1;
    }

    // Repair intersections of x, y.
    // SAFETY: `x` valid.
    if unsafe { (*x).level } != 0 {
        // SAFETY: `x`/`y` valid; `x.ptr[x.n-1]` (post-increment, the last
        // slot) was just moved from `y` above.
        unsafe {
            let mut d = Intersection::new();
            let x_n = (*x).n;
            let x_last = node_child(x, x_n as usize - 1);
            intersect_mov(&(*y).intersect, &mut (*x).intersect, &mut (*x_last).intersect, &mut d);
            if !d.is_empty() {
                for xi in 0..(x_n - 1) {
                    // ptr[x.n - 1] deliberately skipped
                    let child = node_child(x, xi as usize);
                    intersect_add(&mut (*child).intersect, &d);
                }
            }
            bubble_up(y);
        }
    } else {
        // If the first element of y used to be a start node, check if it
        // now covers all of y.
        // SAFETY: `p`/`x`/`y` valid.
        unsafe {
            if mt_start(&(*p).key[i as usize]) {
                let pi = pseudo_index(y, 0); // note: sloppy pseudo-index
                let end_id = mt_lookup_key_side(&(*p).key[i as usize], true);
                let pi_end = pseudo_index_for_id(b, end_id, true);
                if pi_end > pi {
                    let id = mt_lookup_key(&(*p).key[i as usize]);
                    intersect_node(&mut (*y).intersect, id);
                }
            }
            let x_n = (*x).n;
            if mt_end(&(*x).key[x_n as usize - 1]) {
                // No need for a check, just delete it if it was there.
                let id = mt_lookup_key_side(&(*x).key[x_n as usize - 1], false);
                unintersect_node(&mut (*x).intersect, id, false);
            }
        }
    }
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

/// `intersect_merge`: similar to [`intersect_common`] but *also* mutates
/// `x` and `y` in place to retain only the items *not* in common - i.e.
/// simultaneously computes `m = x & y`, `x = x - m`, `y = y - m` (all
/// sorted, all in one linear merge pass, matching the original's own doc
/// comment: "similar to intersect_common but modify x and y in place to
/// retain only the items which are NOT in common").
fn intersect_merge(m: &mut Intersection, x: &mut Intersection, y: &mut Intersection) {
    let (mut xi, mut yi) = (0usize, 0usize);
    let (mut xn, mut yn) = (0usize, 0usize);
    while xi < x.len() && yi < y.len() {
        match x[xi].cmp(&y[yi]) {
            std::cmp::Ordering::Equal => {
                m.push(x[xi]);
                xi += 1;
                yi += 1;
            }
            std::cmp::Ordering::Less => {
                x[xn] = x[xi];
                xn += 1;
                xi += 1;
            }
            std::cmp::Ordering::Greater => {
                y[yn] = y[yi];
                yn += 1;
                yi += 1;
            }
        }
    }
    if xi < x.len() {
        x.copy_within(xi.., xn);
        xn += x.len() - xi;
    }
    if yi < y.len() {
        y.copy_within(yi.., yn);
        yn += y.len() - yi;
    }
    x.truncate(xn);
    y.truncate(yn);
}

/// `intersect_mov`: `w` used to be a child of `x` but it is now a child of
/// `y`; adjusts intersections accordingly. `x` is read-only ("immutable
/// in the context of intersect_mov", per the original's own unit-test
/// comment, `intersect_mov_test`, kept as this function's own test
/// vectors below); `y` and `w` are mutated in place. `d` (out-only)
/// accumulates intersections which should additionally be added to the
/// *old* children of `y`.
fn intersect_mov(x: &Intersection, y: &mut Intersection, w: &mut Intersection, d: &mut Intersection) {
    let (mut wi, mut yi, mut wn, mut yn, mut xi) = (0usize, 0usize, 0usize, 0usize, 0usize);
    while wi < w.len() || xi < x.len() {
        if wi < w.len() && (xi >= x.len() || x[xi] >= w[wi]) {
            if xi < x.len() && x[xi] == w[wi] {
                xi += 1;
            }
            // Now w < x strictly.
            while yi < y.len() && y[yi] < w[wi] {
                d.push(y[yi]);
                yi += 1;
            }
            if yi < y.len() && y[yi] == w[wi] {
                y[yn] = y[yi];
                yn += 1;
                yi += 1;
                wi += 1;
            } else {
                w[wn] = w[wi];
                wn += 1;
                wi += 1;
            }
        } else {
            // x < w strictly.
            while yi < y.len() && y[yi] < x[xi] {
                d.push(y[yi]);
                yi += 1;
            }
            if yi < y.len() && y[yi] == x[xi] {
                y[yn] = y[yi];
                yn += 1;
                yi += 1;
                xi += 1;
            } else {
                // Add x[xi] at w[wn], inserting (shifting up) if wi == wn.
                if wi == wn {
                    w.insert(wn, x[xi]);
                    wn += 1;
                    wi += 1; // no need to consider the added element again
                } else {
                    debug_assert!(wn < wi);
                    w[wn] = x[xi];
                    wn += 1;
                }
                xi += 1;
            }
        }
    }
    if yi < y.len() {
        // Move remaining items to d.
        d.extend_from_slice(&y[yi..]);
    }
    w.truncate(wn);
    y.truncate(yn);
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

/// `intersect_add`: in-place union, `x |= y` (translated directly from
/// the original's own dedicated algorithm, not composed from
/// [`intersect_merge`] - unlike that function, `intersect_add` never
/// needs the "common part" output, and never mutates `y`).
fn intersect_add(x: &mut Intersection, y: &Intersection) {
    let (mut xi, mut yi) = (0usize, 0usize);
    while xi < x.len() && yi < y.len() {
        if x[xi] == y[yi] {
            xi += 1;
            yi += 1;
        } else if y[yi] < x[xi] {
            x.insert(xi, y[yi]);
            xi += 1; // newly added element
            yi += 1;
        } else {
            xi += 1;
        }
    }
    if yi < y.len() {
        x.extend_from_slice(&y[yi..]);
    }
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

/// `marktree_put`: inserts `key` (unpaired if `end_row < 0`, otherwise
/// paired with a second, `MT_FLAG_END`-tagged key at `(end_row, end_col)`,
/// with the pair's spanned nodes marked as intersecting via
/// `marktree_intersect_pair`). This is the public, paired-mark-aware
/// entry point that `extmark.c` (not yet translated) uses; `key`'s
/// caller-supplied flags must not reach outside `MT_FLAG_EXTERNAL_MASK |
/// MT_FLAG_RIGHT_GRAVITY` (checked, matching the original's `assert`,
/// which - like other plain C `assert()`s outside the debug-only
/// `marktree_check*` family - is translated as `debug_assert!`, compiled
/// out in release builds exactly like the original under `NDEBUG`).
pub fn marktree_put(b: &mut MarkTree, mut key: MtKey, end_row: i32, end_col: i32, end_right: bool) {
    debug_assert_eq!(key.flags & !(MT_FLAG_EXTERNAL_MASK | MT_FLAG_RIGHT_GRAVITY), 0);
    if end_row >= 0 {
        key.flags |= MT_FLAG_PAIRED;
    }

    if end_row < 0 {
        marktree_put_key(b, key);
        return;
    }

    // Clone before marktree_put_key consumes `key`: the original C passes
    // `key` by value, leaving the caller's own copy (here, `key` itself,
    // since we build `end_key` before moving `key` away) unaffected by the
    // MT_FLAG_REAL bit marktree_put_key adds to its own internal copy.
    let mut end_key = key.clone();
    end_key.flags = (key.flags & !MT_FLAG_RIGHT_GRAVITY)
        | MT_FLAG_END
        | if end_right { MT_FLAG_RIGHT_GRAVITY } else { 0 };
    end_key.pos = MtPos::new(end_row, end_col);

    let key_id = mt_lookup_key(&key);
    let end_key_id = mt_lookup_key(&end_key);

    marktree_put_key(b, key);
    marktree_put_key(b, end_key);

    let mut itr = MarkTreeIter::default();
    let mut end_itr = MarkTreeIter::default();
    marktree_lookup(b, key_id, Some(&mut itr));
    marktree_lookup(b, end_key_id, Some(&mut end_itr));

    marktree_intersect_pair(b, key_id, &mut itr, &end_itr, false);
}

/// `rawkey`: `itr->x->key[itr->i]` (the original's macro).
///
/// # Safety
/// `itr.x` must be non-null/valid, and `itr.i` a valid key index into it
/// (`0 <= itr.i < (*itr.x).n`).
#[inline]
unsafe fn rawkey(itr: &MarkTreeIter) -> &MtKey {
    // SAFETY: forwarded from caller.
    unsafe { &(*itr.x).key[itr.i as usize] }
}

/// `marktree_itr_get`: places `itr` at the first key `>= (row, col)`.
pub fn marktree_itr_get(b: &MarkTree, row: i32, col: i32, itr: &mut MarkTreeIter) -> bool {
    marktree_itr_get_ext(b, MtPos::new(row, col), itr, false, false, None, None)
}

/// `marktree_itr_get_ext`: the general form behind [`marktree_itr_get`].
/// `last`: find the last key `<= p` instead of the first `>= p`.
/// `gravity`: match only keys with `MT_FLAG_RIGHT_GRAVITY` set.
/// `oldbase` (optional, indexed by tree level): filled in with the
/// accumulated absolute position at each level descended through.
/// `meta_filter` (optional): stop descending into a subtree that
/// provably can't contain any key matching the filter.
pub fn marktree_itr_get_ext(
    b: &MarkTree,
    p: MtPos,
    itr: &mut MarkTreeIter,
    last: bool,
    gravity: bool,
    mut oldbase: Option<&mut [MtPos]>,
    meta_filter: Option<MetaFilter<'_>>,
) -> bool {
    if b.n_keys == 0 {
        itr.x = std::ptr::null_mut();
        return false;
    }

    let mut k = MtKey {
        pos: p,
        ns: 0,
        id: 0,
        flags: if gravity { MT_FLAG_RIGHT_GRAVITY } else { 0 },
        decor_data: crate::decoration_defs::DecorInlineData { hl: DecorHighlightInline::default() },
    };
    if last && !gravity {
        k.flags = MT_FLAG_LAST;
    }
    itr.pos = MtPos::default();
    itr.x = b.root;
    itr.lvl = 0;
    if let Some(ob) = oldbase.as_deref_mut() {
        ob[itr.lvl as usize] = itr.pos;
    }
    loop {
        // SAFETY: `itr.x` starts as `b.root` (non-null since `n_keys > 0`)
        // and is only ever reassigned to a valid child pointer below.
        itr.i = unsafe { marktree_getp_aux(&*itr.x, &k, None) } + 1;

        // SAFETY: `itr.x` valid.
        if unsafe { (*itr.x).level } == 0 {
            break;
        }
        if let Some(mf) = meta_filter {
            // SAFETY: `itr.x` valid internal node.
            if !meta_has(&unsafe { node_meta(itr.x, itr.i as usize) }, mf) {
                // This takes us to the internal position after the first
                // rejected node.
                break;
            }
        }

        itr.s[itr.lvl as usize].i = itr.i;
        itr.s[itr.lvl as usize].oldcol = itr.pos.col;

        if itr.i > 0 {
            // SAFETY: `itr.x` valid.
            let base = unsafe { (*itr.x).key[itr.i as usize - 1].pos };
            compose(&mut itr.pos, base);
            relative(base, &mut k.pos);
        }
        // SAFETY: `itr.x` valid internal node.
        itr.x = unsafe { node_child(itr.x, itr.i as usize) };
        itr.lvl += 1;
        if let Some(ob) = oldbase.as_deref_mut() {
            ob[itr.lvl as usize] = itr.pos;
        }
    }

    if last {
        marktree_itr_prev(b, itr)
    } else if itr.i >= unsafe { (*itr.x).n } {
        // No need for `meta_filter` here, this just goes up one step.
        marktree_itr_next_skip(itr, true, false, None, None)
    } else {
        true
    }
}

/// `marktree_itr_first`: places `itr` at the very first key in the tree.
pub fn marktree_itr_first(b: &MarkTree, itr: &mut MarkTreeIter) -> bool {
    if b.n_keys == 0 {
        itr.x = std::ptr::null_mut();
        return false;
    }
    itr.x = b.root;
    itr.i = 0;
    itr.lvl = 0;
    itr.pos = MtPos::new(0, 0);
    // SAFETY: `itr.x` is `b.root`, non-null (`n_keys > 0` checked above),
    // and stays valid as it's only reassigned to a valid child below.
    while unsafe { (*itr.x).level } > 0 {
        itr.s[itr.lvl as usize].i = 0;
        itr.s[itr.lvl as usize].oldcol = 0;
        itr.lvl += 1;
        itr.x = unsafe { node_child(itr.x, 0) };
    }
    true
}

/// `marktree_itr_last`: places `itr` at the very last key in the tree.
/// Returns `bool` here (the original declares `int` but only ever returns
/// `true`/`false` - a faithful behavioral match).
// gives the first key that is greater or equal to p (kept verbatim from
// the original's own doc comment, even though - per the implementation -
// this takes no position parameter at all and always walks to the
// rightmost leaf; presumably stale documentation upstream).
pub fn marktree_itr_last(b: &MarkTree, itr: &mut MarkTreeIter) -> bool {
    if b.n_keys == 0 {
        itr.x = std::ptr::null_mut();
        return false;
    }
    itr.pos = MtPos::new(0, 0);
    itr.x = b.root;
    itr.lvl = 0;
    loop {
        // SAFETY: `itr.x` valid (loop invariant).
        itr.i = unsafe { (*itr.x).n };

        if unsafe { (*itr.x).level } == 0 {
            break;
        }

        itr.s[itr.lvl as usize].i = itr.i;
        itr.s[itr.lvl as usize].oldcol = itr.pos.col;

        debug_assert!(itr.i > 0);
        // SAFETY: `itr.x` valid; `itr.i > 0`.
        let base = unsafe { (*itr.x).key[itr.i as usize - 1].pos };
        compose(&mut itr.pos, base);

        itr.x = unsafe { node_child(itr.x, itr.i as usize) };
        itr.lvl += 1;
    }
    itr.i -= 1;
    true
}

/// `marktree_itr_next`: advances `itr` to the next key (in tree order).
pub fn marktree_itr_next(b: &MarkTree, itr: &mut MarkTreeIter) -> bool {
    let _ = b; // unused in the original too - kept for public signature parity
    marktree_itr_next_skip(itr, false, false, None, None)
}

/// `marktree_itr_next_skip`: the general form behind [`marktree_itr_next`]
/// (`static`/private in the original, so - unlike the public iterator
/// functions - its unused `MarkTree *b` parameter is simply dropped here;
/// nothing outside this file calls it directly).
fn marktree_itr_next_skip(
    itr: &mut MarkTreeIter,
    mut skip: bool,
    preload: bool,
    mut oldbase: Option<&mut [MtPos]>,
    meta_filter: Option<MetaFilter<'_>>,
) -> bool {
    if itr.x.is_null() {
        return false;
    }
    itr.i += 1;
    // SAFETY: `itr.x` non-null (checked above) and valid.
    let level = unsafe { (*itr.x).level };
    if let Some(mf) = meta_filter {
        if level > 0 && !meta_has(&unsafe { node_meta(itr.x, itr.i as usize) }, mf) {
            skip = true;
        }
    }
    if level == 0 || skip {
        // SAFETY: `itr.x` valid.
        let n = unsafe { (*itr.x).n };
        if preload && level == 0 && skip {
            // Skip rest of this leaf node.
            itr.i = n;
        } else if itr.i < n {
            return true;
        }
        // We ran out of non-internal keys. Go up until we find an internal key.
        loop {
            // SAFETY: `itr.x` valid.
            if itr.i < unsafe { (*itr.x).n } {
                break;
            }
            itr.x = unsafe { (*itr.x).parent };
            if itr.x.is_null() {
                return false;
            }
            itr.lvl -= 1;
            itr.i = itr.s[itr.lvl as usize].i;
            if itr.i > 0 {
                // SAFETY: `itr.x` valid; `itr.i > 0`.
                let key_row = unsafe { (*itr.x).key[itr.i as usize - 1].pos.row };
                itr.pos.row -= key_row;
                itr.pos.col = itr.s[itr.lvl as usize].oldcol;
            }
        }
    } else {
        // We stood at an "internal" key. Go down to the first non-internal
        // key after it.
        // SAFETY: `itr.x` valid.
        while unsafe { (*itr.x).level } > 0 {
            // Internal key, there is always a child after.
            if itr.i > 0 {
                itr.s[itr.lvl as usize].oldcol = itr.pos.col;
                // SAFETY: `itr.x` valid.
                let base = unsafe { (*itr.x).key[itr.i as usize - 1].pos };
                compose(&mut itr.pos, base);
            }
            if itr.i == 0 {
                if let Some(ob) = oldbase.as_deref_mut() {
                    let lvl = itr.lvl as usize;
                    ob[lvl + 1] = ob[lvl];
                }
            }
            itr.s[itr.lvl as usize].i = itr.i;
            // SAFETY: `itr.x` valid internal node.
            debug_assert_eq!(unsafe { (*node_child(itr.x, itr.i as usize)).parent }, itr.x);
            itr.lvl += 1;
            itr.x = unsafe { node_child(itr.x, itr.i as usize) };
            // SAFETY: `itr.x` valid (just reassigned).
            if preload && unsafe { (*itr.x).level } != 0 {
                itr.i = -1;
                break;
            }
            itr.i = 0;
            if let Some(mf) = meta_filter {
                if unsafe { (*itr.x).level } != 0 && !meta_has(&unsafe { node_meta(itr.x, 0) }, mf) {
                    // `itr.x` has filtered keys but `ptr[0]` does not, don't
                    // enter the latter.
                    break;
                }
            }
        }
    }
    true
}

/// `marktree_itr_prev`: moves `itr` to the previous key (in tree order).
pub fn marktree_itr_prev(b: &MarkTree, itr: &mut MarkTreeIter) -> bool {
    let _ = b; // unused in the original too - kept for public signature parity
    if itr.x.is_null() {
        return false;
    }
    // SAFETY: `itr.x` non-null (checked above) and valid.
    if unsafe { (*itr.x).level } == 0 {
        itr.i -= 1;
        if itr.i >= 0 {
            return true;
        }
        // We ran out of non-internal keys. Go up until we find a non-internal key.
        loop {
            itr.x = unsafe { (*itr.x).parent };
            if itr.x.is_null() {
                return false;
            }
            itr.lvl -= 1;
            itr.i = itr.s[itr.lvl as usize].i - 1;
            if itr.i >= 0 {
                // SAFETY: `itr.x` valid; `itr.i >= 0`.
                let key_row = unsafe { (*itr.x).key[itr.i as usize].pos.row };
                itr.pos.row -= key_row;
                itr.pos.col = itr.s[itr.lvl as usize].oldcol;
                break;
            }
        }
    } else {
        // We stood at an "internal" key. Go down to the last non-internal
        // key before it.
        // SAFETY: `itr.x` valid.
        while unsafe { (*itr.x).level } > 0 {
            // Internal key, there is always a child before.
            if itr.i > 0 {
                itr.s[itr.lvl as usize].oldcol = itr.pos.col;
                // SAFETY: `itr.x` valid.
                let base = unsafe { (*itr.x).key[itr.i as usize - 1].pos };
                compose(&mut itr.pos, base);
            }
            itr.s[itr.lvl as usize].i = itr.i;
            // SAFETY: `itr.x` valid internal node.
            debug_assert_eq!(unsafe { (*node_child(itr.x, itr.i as usize)).parent }, itr.x);
            itr.x = unsafe { node_child(itr.x, itr.i as usize) };
            // SAFETY: `itr.x` valid (just reassigned).
            itr.i = unsafe { (*itr.x).n };
            itr.lvl += 1;
        }
        itr.i -= 1;
    }
    true
}

/// `marktree_itr_node_done`: is `itr` at the last key of its current node?
pub fn marktree_itr_node_done(itr: &MarkTreeIter) -> bool {
    // SAFETY: only dereferenced when non-null (short-circuit `||`).
    itr.x.is_null() || itr.i == unsafe { (*itr.x).n } - 1
}

/// `marktree_itr_pos`: the current key's absolute position.
///
/// # Safety
/// `itr` must be valid (`marktree_itr_valid`/`MarkTreeIter::is_valid`).
pub unsafe fn marktree_itr_pos(itr: &MarkTreeIter) -> MtPos {
    // SAFETY: forwarded from caller.
    let mut pos = unsafe { rawkey(itr) }.pos;
    unrelative(itr.pos, &mut pos);
    pos
}

/// `marktree_itr_current`: the current key (with its position resolved to
/// absolute), or [`mt_invalid_key`] if `itr` is not valid.
pub fn marktree_itr_current(itr: &MarkTreeIter) -> MtKey {
    if !itr.x.is_null() {
        // SAFETY: `itr.x` non-null (checked above).
        let mut key = unsafe { rawkey(itr) }.clone();
        key.pos = unsafe { marktree_itr_pos(itr) };
        key
    } else {
        mt_invalid_key()
    }
}

/// `itr_eq`: do `itr1`/`itr2` currently refer to the exact same key slot?
///
/// # Safety
/// Both iterators must be valid.
#[allow(dead_code)]
unsafe fn itr_eq(itr1: &MarkTreeIter, itr2: &MarkTreeIter) -> bool {
    // SAFETY: forwarded from caller; a pointer-identity comparison,
    // matching the original's `&rawkey(itr1) == &rawkey(itr2)`.
    std::ptr::eq(unsafe { rawkey(itr1) }, unsafe { rawkey(itr2) })
}

/// `marktree_lookup`: looks up the mark with lookup id `id` (see
/// `mt_lookup_id`/`mt_lookup_key`), optionally positioning `itr` there.
/// Returns [`mt_invalid_key`] if not found.
pub fn marktree_lookup(b: &MarkTree, id: u64, mut itr: Option<&mut MarkTreeIter>) -> MtKey {
    let n = match lookup_id2node(b, id) {
        Some(n) => n,
        None => {
            if let Some(itr) = itr.as_deref_mut() {
                itr.x = std::ptr::null_mut();
            }
            return mt_invalid_key();
        }
    };
    // SAFETY: `n` came from `id2node`, which only ever stores valid
    // pointers into this same tree.
    let node_n = unsafe { (*n).n };
    for i in 0..node_n {
        let key = unsafe { &(*n).key[i as usize] };
        if mt_lookup_key(key) == id {
            // SAFETY: `n`/`i` valid (just confirmed above).
            return unsafe { marktree_itr_set_node(b, itr, n, i) };
        }
    }
    unreachable!("marktree_lookup: id2node pointed at a node without the looked-up key");
}

/// `marktree_lookup_ns`: [`marktree_lookup`] by `(ns, id, end)` instead of
/// a raw lookup id.
pub fn marktree_lookup_ns(
    b: &MarkTree,
    ns: u32,
    id: u32,
    end: bool,
    itr: Option<&mut MarkTreeIter>,
) -> MtKey {
    marktree_lookup(b, mt_lookup_id(ns, id, end), itr)
}

/// `marktree_itr_set_node`: returns the (absolute-position) key at slot
/// `i` of node `n`, optionally positioning `itr` there by walking back up
/// to the root to fill in `itr.s`/`itr.lvl`/`itr.pos`.
///
/// # Safety
/// `n` must be a valid node pointer within `b`'s tree, and `i` a valid key
/// index into it.
pub unsafe fn marktree_itr_set_node(
    b: &MarkTree,
    mut itr: Option<&mut MarkTreeIter>,
    n: *mut MtNode,
    i: i32,
) -> MtKey {
    // SAFETY: caller guarantees `n`/`i` are valid.
    let mut key = unsafe { (*n).key[i as usize].clone() };
    if let Some(itr) = itr.as_deref_mut() {
        itr.i = i;
        itr.x = n;
        // SAFETY: `b.root`/`n` valid.
        itr.lvl = unsafe { (*b.root).level as i32 - (*n).level as i32 };
    }
    let mut n = n;
    // SAFETY: walking up valid parent pointers.
    while !unsafe { (*n).parent }.is_null() {
        let p = unsafe { (*n).parent };
        let i = unsafe { (*n).p_idx as i32 };
        debug_assert_eq!(unsafe { node_child(p, i as usize) }, n);

        if let Some(itr) = itr.as_deref_mut() {
            let lvl = unsafe { (*b.root).level as i32 - (*p).level as i32 };
            itr.s[lvl as usize].i = i;
        }
        if i > 0 {
            // SAFETY: `p` valid.
            let base = unsafe { (*p).key[i as usize - 1].pos };
            unrelative(base, &mut key.pos);
        }
        n = p;
    }
    // Last use of `itr` in this function, so take it by value rather than
    // re-borrowing via `as_deref_mut()` again.
    if let Some(itr) = itr {
        marktree_itr_fix_pos(b, itr);
    }
    key
}

/// `marktree_get_altpos`: the position of [`marktree_get_alt`].
pub fn marktree_get_altpos(b: &MarkTree, mark: &MtKey, itr: Option<&mut MarkTreeIter>) -> MtPos {
    marktree_get_alt(b, mark, itr).pos
}

/// @return alt mark for a paired mark or mark itself for unpaired mark
pub fn marktree_get_alt(b: &MarkTree, mark: &MtKey, itr: Option<&mut MarkTreeIter>) -> MtKey {
    if mt_paired(mark) {
        marktree_lookup_ns(b, mark.ns, mark.id, !mt_end(mark), itr)
    } else {
        mark.clone()
    }
}

/// `marktree_itr_fix_pos`: recomputes `itr.pos` from scratch by walking
/// down from the root following `itr.s[0..itr.lvl]`'s recorded child
/// indices (used after [`marktree_itr_set_node`] establishes `itr.s`/
/// `itr.lvl` but not yet `itr.pos`).
fn marktree_itr_fix_pos(b: &MarkTree, itr: &mut MarkTreeIter) {
    itr.pos = MtPos::default();
    let mut x = b.root;
    for lvl in 0..itr.lvl {
        itr.s[lvl as usize].oldcol = itr.pos.col;
        let i = itr.s[lvl as usize].i;
        if i > 0 {
            // SAFETY: `x` valid (loop invariant, established by the walk
            // itself and the caller's contract that `itr.s`/`itr.lvl`
            // describe a real path from `b.root`).
            let base = unsafe { (*x).key[i as usize - 1].pos };
            compose(&mut itr.pos, base);
        }
        // SAFETY: `x` valid.
        debug_assert_ne!(unsafe { (*x).level }, 0);
        x = unsafe { node_child(x, i as usize) };
    }
    debug_assert_eq!(x, itr.x);
}

/// `iat`: the original's `iat` macro (`#define iat(itr, l, q) ((l ==
/// itr->lvl) ? itr->i + q : itr->s[l].i)`).
///
/// Reads iterator `itr`'s child-index at tree level `l`: its *current*
/// level's index (offset by `q`) if `l == itr.lvl`, or an ancestor
/// level's recorded index otherwise.
#[inline]
fn iat(itr: &MarkTreeIter, l: i32, q: i32) -> i32 {
    if l == itr.lvl {
        itr.i + q
    } else {
        itr.s[l as usize].i
    }
}

/// `marktree_intersect_pair`: marks (or, if `delete`, unmarks) every
/// internal node strictly between `itr` and `end_itr` as intersecting
/// `id` - the mechanism that makes "does any pair span this position"
/// queries O(1) rather than needing to scan every pair.
///
/// @param itr mutated
/// @param end_itr not mutated
// The `itr.lvl > lvl` and `iat(...) < iat(...)` branches both set `skip =
// true` - clippy's `if_same_then_else` flags this as suspicious, but it's
// a faithful match to the original's own `else if (...) { skip = true; }
// else { if (...) { skip = true; } else { lvl++; } }` structure: two
// independently-meaningful conditions that happen to trigger the same
// action, not an accidental copy-paste duplication.
#[allow(clippy::if_same_then_else)]
pub fn marktree_intersect_pair(
    b: &MarkTree,
    id: u64,
    itr: &mut MarkTreeIter,
    end_itr: &MarkTreeIter,
    delete: bool,
) {
    let _ = b; // unused in the original too - kept for public signature parity
    let mut lvl = 0;
    let maxlvl = itr.lvl.min(end_itr.lvl);
    while lvl < maxlvl {
        if itr.s[lvl as usize].i > end_itr.s[lvl as usize].i {
            return; // empty range
        } else if itr.s[lvl as usize].i < end_itr.s[lvl as usize].i {
            break; // work to do
        }
        lvl += 1;
    }
    if lvl == maxlvl && iat(itr, lvl, 1) > iat(end_itr, lvl, 0) {
        return; // empty range
    }

    while !itr.x.is_null() {
        let mut skip = false;
        if itr.x == end_itr.x {
            // SAFETY: `itr.x` non-null (loop condition).
            if unsafe { (*itr.x).level } == 0 || itr.i >= end_itr.i {
                break;
            } else {
                skip = true;
            }
        } else if itr.lvl > lvl {
            skip = true;
        } else if iat(itr, lvl, 1) < iat(end_itr, lvl, 1) {
            skip = true;
        } else {
            lvl += 1;
        }

        if skip {
            // SAFETY: `itr.x` non-null.
            if unsafe { (*itr.x).level } != 0 {
                // `itr.i + 1` must be computed in `i32` first (matching the
                // original's `int` arithmetic) before casting to `usize`:
                // `itr.i` can be `-1` here (see marktree_itr_next_skip's
                // `preload` branch), and `(itr.i as usize) + 1` would wrap
                // around instead of correctly yielding `0`.
                // SAFETY: `itr.x` valid internal node.
                let x = unsafe { node_child(itr.x, (itr.i + 1) as usize) };
                // SAFETY: `x` is a valid, live child node.
                if delete {
                    unsafe { unintersect_node(&mut (*x).intersect, id, true) };
                } else {
                    unsafe { intersect_node(&mut (*x).intersect, id) };
                }
            }
        }
        marktree_itr_next_skip(itr, skip, true, None, None);
    }
}

/// `marktree_move`: moves the mark `itr` currently points at to `(row,
/// col)`. If the move stays within the same leaf node and doesn't cross
/// the position of an adjacent internal key (the common, fast case), the
/// key is reordered in place; otherwise this falls back to a full delete
/// plus re-insert (restoring pair intersection tracking afterward if the
/// mark was paired, via [`marktree_restore_pair`]).
///
/// @param itr iterator is invalid after call
///
/// # Safety
/// `itr` must be valid.
pub unsafe fn marktree_move(b: &mut MarkTree, itr: &mut MarkTreeIter, row: i32, col: i32) {
    // SAFETY: caller guarantees `itr` is valid.
    let mut key = unsafe { rawkey(itr) }.clone();
    let x = itr.x;
    // SAFETY: `x` valid.
    if unsafe { (*x).level } == 0 {
        let mut internal = false;
        let mut newpos = MtPos::new(row, col);
        // SAFETY: `x` valid.
        if !unsafe { (*x).parent }.is_null() {
            // Strictly _after_ key before `x` (not optimal when x is very
            // first leaf of the entire tree, but that's fine).
            if pos_less(itr.pos, newpos) {
                relative(itr.pos, &mut newpos);
                // Strictly before the end of x (this could be made
                // sharper by finding the internal key just after x, but meh).
                // SAFETY: `x` valid; a leaf always has `n >= 1`.
                let last_key_pos = unsafe { (*x).key[(*x).n as usize - 1].pos };
                if pos_less(newpos, last_key_pos) {
                    internal = true;
                }
            }
        } else {
            // Tree is one node. newpos thus is already "relative" itr->pos.
            internal = true;
        }

        if internal {
            if key.pos.row == newpos.row && key.pos.col == newpos.col {
                return;
            }
            key.pos = newpos;
            let mut is_match = false;
            // Tricky: could minimize movement in either direction better.
            // SAFETY: `x` valid.
            let mut new_i = unsafe { marktree_getp_aux(&*x, &key, Some(&mut is_match)) };
            if !is_match {
                new_i += 1;
            }
            // SAFETY: `x` valid; all indices below are in-bounds slots.
            unsafe {
                if new_i == itr.i {
                    (*x).key[itr.i as usize].pos = newpos;
                } else if new_i < itr.i {
                    for j in (new_i..itr.i).rev() {
                        let moved = std::mem::take(&mut (*x).key[j as usize]);
                        (*x).key[j as usize + 1] = moved;
                    }
                    (*x).key[new_i as usize] = key;
                } else {
                    // new_i > itr.i
                    for j in itr.i..(new_i - 1) {
                        let moved = std::mem::take(&mut (*x).key[j as usize + 1]);
                        (*x).key[j as usize] = moved;
                    }
                    (*x).key[new_i as usize - 1] = key;
                }
            }
            return;
        }
    }
    let other = unsafe { marktree_del_itr(b, itr, false) };
    key.pos = MtPos::new(row, col);

    marktree_put_key(b, key.clone());

    if other != 0 {
        marktree_restore_pair(b, &key);
    }
    itr.x = std::ptr::null_mut(); // itr might become invalid by put
}

/// `marktree_restore_pair`: re-establishes intersection tracking for a
/// paired mark `key` whose start/end both currently exist (un-orphaning
/// both sides). If either side isn't found (e.g. the other side is
/// itself waiting to be restored by a subsequent call), this is a silent
/// no-op - the original's own documented behavior.
pub fn marktree_restore_pair(b: &mut MarkTree, key: &MtKey) {
    let mut itr = MarkTreeIter::default();
    let mut end_itr = MarkTreeIter::default();
    marktree_lookup(b, mt_lookup_key_side(key, false), Some(&mut itr));
    marktree_lookup(b, mt_lookup_key_side(key, true), Some(&mut end_itr));
    if itr.x.is_null() || end_itr.x.is_null() {
        // This could happen if the other end is waiting to be restored
        // later; this function will be called again for the other end.
        return;
    }
    // SAFETY: both iterators just confirmed non-null/valid above.
    unsafe {
        (*itr.x).key[itr.i as usize].flags &= !MT_FLAG_ORPHANED;
        (*end_itr.x).key[end_itr.i as usize].flags &= !MT_FLAG_ORPHANED;
    }

    marktree_intersect_pair(b, mt_lookup_key_side(key, false), &mut itr, &end_itr, false);
}

/// `marktree_del_itr`: deletes the mark the iterator currently points at,
/// leaving `itr` valid and pointing at the key *after* the deleted one.
/// If the deleted mark was one side of a pair, returns the lookup id of
/// the other side (0 if it was unpaired or already orphaned).
///
/// The original's own "INITIATING DELETION PROTOCOL" comment (preserved
/// on the C side) documents the six repair steps: (1) construct a valid
/// iterator to the key to delete; (2) if it's an "internal" key, step to
/// an adjacent leaf key to find a real (leaf-level) "auxiliary key" to
/// delete instead; (3) delete that leaf key; (4) if step 2 applied,
/// splice the auxiliary key up into the internal slot it vacated,
/// adjusting relative positions/meta counts along the way; (5) repair
/// undersized nodes by stealing from a sibling (`pivot_left`/
/// `pivot_right`) or merging (`merge_node`), walking up as needed; (6) if
/// the root ends up empty, replace it with its sole child (or, if the
/// tree is now empty, invalidate the iterator).
///
/// @param rev should be true if we plan to iterate _backwards_ and
/// delete stuff before this key. Most of the time this is false (the
/// recommended strategy is to always iterate forward) - and `rev = true`
/// is in fact unimplemented upstream (`abort()`), so it is left
/// untranslated here too (`unimplemented!()`) rather than guessing a
/// behavior neovim itself has never defined.
///
/// # Safety
/// `itr` must be a valid iterator (`itr.is_valid()`) into `b`'s tree.
pub unsafe fn marktree_del_itr(b: &mut MarkTree, itr: &mut MarkTreeIter, rev: bool) -> u64 {
    const T: i32 = crate::marktree_defs::MT_BRANCH_FACTOR as i32;
    let mut adjustment: i32 = 0;

    let cur = itr.x;
    let curi = itr.i;
    // SAFETY: caller guarantees `itr` is valid.
    let id = unsafe { mt_lookup_key(&(*cur).key[curi as usize]) };

    // SAFETY: `itr` valid.
    let raw = unsafe { rawkey(itr) }.clone();
    let mut other: u64 = 0;
    if mt_paired(&raw) && raw.flags & MT_FLAG_ORPHANED == 0 {
        other = mt_lookup_key_side(&raw, !mt_end(&raw));

        let mut other_itr = MarkTreeIter::default();
        marktree_lookup(b, other, Some(&mut other_itr));
        // `rawkey(other_itr).flags |= MT_FLAG_ORPHANED;` in the original -
        // this mutating use needs direct raw-pointer field access, since
        // this module's `rawkey()` helper returns a shared `&MtKey` (read
        // access only).
        // SAFETY: `other_itr` was just set to a valid position by the
        // successful lookup above (a paired mark's other side always
        // exists while this side isn't orphaned yet).
        unsafe {
            (*other_itr.x).key[other_itr.i as usize].flags |= MT_FLAG_ORPHANED;
        }
        // Remove intersect markers. NB: must match exactly!
        if mt_start(&raw) {
            let mut this_itr = *itr; // mutated copy (MarkTreeIter: Copy)
            marktree_intersect_pair(b, id, &mut this_itr, &other_itr, true);
        } else {
            marktree_intersect_pair(b, other, &mut other_itr, itr, true);
        }
    }

    // 2.
    // SAFETY: `itr.x` valid.
    if unsafe { (*itr.x).level } != 0 {
        if rev {
            unimplemented!("marktree_del_itr: rev=true is unimplemented upstream too (abort())");
        } else {
            // Steal previous node.
            marktree_itr_prev(b, itr);
            adjustment = -1;
        }
    }

    // 3.
    let x = itr.x;
    // SAFETY: `x` valid.
    debug_assert_eq!(unsafe { (*x).level }, 0);
    let mut intkey = unsafe { (*x).key[itr.i as usize].clone() };

    let mut meta_inc = meta_describe_key(&intkey);
    // SAFETY: `x` valid.
    let x_n = unsafe { (*x).n };
    if x_n > itr.i + 1 {
        for j in (itr.i as usize)..(x_n as usize - 1) {
            unsafe {
                let moved = std::mem::take(&mut (*x).key[j + 1]);
                (*x).key[j] = moved;
            }
        }
    }
    unsafe { (*x).n -= 1 };

    b.n_keys -= 1;
    b.id2node.remove(&id);

    // 4.
    if adjustment == -1 {
        let mut ilvl = itr.lvl - 1;
        let mut lnode = x;
        let mut start_id: u64 = 0;
        let mut did_bubble = false;
        if mt_end(&intkey) {
            start_id = mt_lookup_key_side(&intkey, false);
        }
        loop {
            // SAFETY: `lnode` valid.
            let p = unsafe { (*lnode).parent };
            assert!(ilvl >= 0, "marktree_del_itr: ilvl underflow (corrupt iterator state)");
            let i = itr.s[ilvl as usize].i;
            // SAFETY: `p` valid.
            debug_assert_eq!(unsafe { node_child(p, i as usize) }, lnode);
            if i > 0 {
                // SAFETY: `p` valid.
                let base = unsafe { (*p).key[i as usize - 1].pos };
                unrelative(base, &mut intkey.pos);
            }

            if p != cur && start_id != 0 {
                // SAFETY: `p` valid.
                let p0 = unsafe { node_child(p, 0) };
                if unsafe { intersection_has(&(*p0).intersect, start_id) } {
                    // If not the first time, we need to undo the addition
                    // in the previous step (`intersect_node` just below).
                    let last = i32::from(lnode != x);
                    // SAFETY: `p` valid.
                    let p_n = unsafe { (*p).n };
                    for k in 0..(p_n + last) {
                        let child = unsafe { node_child(p, k as usize) };
                        unsafe { unintersect_node(&mut (*child).intersect, start_id, true) };
                    }
                    unsafe { intersect_node(&mut (*p).intersect, start_id) };
                    did_bubble = true;
                }
            }

            {
                // SAFETY: `p`/`lnode` valid.
                let p_idx = unsafe { (*lnode).p_idx };
                let mut meta_p = unsafe { node_meta(p, p_idx as usize) };
                for m in 0..K_MT_META_COUNT {
                    meta_p[m] -= meta_inc[m];
                }
                unsafe { node_set_meta(p, p_idx as usize, meta_p) };
            }

            lnode = p;
            ilvl -= 1;
            if lnode == cur {
                break;
            }
        }

        // SAFETY: `cur` valid.
        let deleted_orig = unsafe { (*cur).key[curi as usize].clone() };
        meta_inc = meta_describe_key(&deleted_orig);
        unsafe { (*cur).key[curi as usize] = intkey.clone() };
        unsafe { refkey(b, cur, curi as usize) };
        // SAFETY: `cur` valid.
        if unsafe { mt_end(&(*cur).key[curi as usize]) } && !did_bubble {
            let pi = unsafe { pseudo_index(x, 0) }; // note: sloppy pseudo-index
            let pi_start = unsafe { pseudo_index_for_id(b, start_id, true) };
            if pi_start > 0 && pi_start < pi {
                unsafe { intersect_node(&mut (*x).intersect, start_id) };
            }
        }

        let mut deleted = deleted_orig;
        relative(intkey.pos, &mut deleted.pos);
        // SAFETY: `cur` valid internal node.
        let mut y = unsafe { node_child(cur, curi as usize + 1) };
        if deleted.pos.row != 0 || deleted.pos.col != 0 {
            while !y.is_null() {
                // SAFETY: `y` valid.
                let y_n = unsafe { (*y).n };
                for k in 0..y_n {
                    unsafe { unrelative(deleted.pos, &mut (*y).key[k as usize].pos) };
                }
                y = if unsafe { (*y).level } != 0 { unsafe { node_child(y, 0) } } else { std::ptr::null_mut() };
            }
        }
        itr.i -= 1;
    }

    let mut lnode = cur;
    // SAFETY: `lnode` valid; walks up valid parent pointers.
    while !unsafe { (*lnode).parent }.is_null() {
        let parent = unsafe { (*lnode).parent };
        let p_idx = unsafe { (*lnode).p_idx };
        let mut meta_p = unsafe { node_meta(parent, p_idx as usize) };
        for m in 0..K_MT_META_COUNT {
            meta_p[m] -= meta_inc[m];
        }
        unsafe { node_set_meta(parent, p_idx as usize, meta_p) };
        lnode = parent;
    }
    for (dst, &inc) in b.meta_root.iter_mut().zip(meta_inc.iter()) {
        debug_assert!(*dst >= inc);
        *dst -= inc;
    }

    // 5.
    let mut itr_dirty = false;
    let mut rlvl = itr.lvl - 1;
    // `lasti`: the original's `int *lasti = &itr->i;`, later possibly
    // repointed to `&itr->s[rlvl].i` - translated as a raw pointer for the
    // same reason the rest of this module uses them (an intrusive,
    // manually-tracked reference that outlives any single safe borrow's
    // natural scope across loop iterations).
    let mut lasti: *mut i32 = &mut itr.i;
    let mut ppos = itr.pos;
    let mut x = x;
    while x != b.root {
        debug_assert!(rlvl >= 0);
        // SAFETY: `x` valid.
        let p = unsafe { (*x).parent };
        if unsafe { (*x).n } >= T - 1 {
            // We are done: if this node is fine the rest of the tree will be.
            break;
        }
        let pi = itr.s[rlvl as usize].i;
        // SAFETY: `p` valid.
        debug_assert_eq!(unsafe { node_child(p, pi as usize) }, x);
        if pi > 0 {
            // SAFETY: `p` valid.
            let key_row = unsafe { (*p).key[pi as usize - 1].pos.row };
            ppos.row -= key_row;
            ppos.col = itr.s[rlvl as usize].oldcol;
        }
        // ppos is now the pos of p.

        // SAFETY: `p` valid.
        let left_spare = pi > 0 && unsafe { (*node_child(p, pi as usize - 1)).n } > T - 1;
        let right_spare = pi < unsafe { (*p).n } && unsafe { (*node_child(p, pi as usize + 1)).n } > T - 1;
        if left_spare {
            unsafe {
                *lasti += 1;
            }
            itr_dirty = true;
            // Steal one key from the left neighbour.
            unsafe { pivot_right(b, ppos, p, pi - 1) };
            break;
        } else if right_spare {
            // Steal one key from right neighbour.
            unsafe { pivot_left(b, ppos, p, pi) };
            break;
        } else if pi > 0 {
            // SAFETY: `p` valid.
            debug_assert_eq!(unsafe { (*node_child(p, pi as usize - 1)).n }, T - 1);
            unsafe {
                *lasti += T;
            }
            x = unsafe { merge_node(b, p, pi - 1) };
            if std::ptr::eq(lasti, &itr.i as *const i32 as *mut i32) {
                // TRICKY: we merged the node the iterator was on.
                itr.x = x;
            }
            itr.s[rlvl as usize].i -= 1;
            itr_dirty = true;
        } else {
            // SAFETY: `p` valid.
            debug_assert!(pi < unsafe { (*p).n } && unsafe { (*node_child(p, pi as usize + 1)).n } == T - 1);
            unsafe { merge_node(b, p, pi) };
            // No iter adjustment needed.
        }
        lasti = &mut itr.s[rlvl as usize].i;
        rlvl -= 1;
        x = p;
    }

    // 6.
    // SAFETY: `b.root` valid.
    if unsafe { (*b.root).n } == 0 {
        if itr.lvl > 0 {
            for l in 0..(itr.lvl as usize - 1) {
                itr.s[l] = itr.s[l + 1];
            }
            itr.lvl -= 1;
        }
        // SAFETY: `b.root` valid.
        if unsafe { (*b.root).level } != 0 {
            let oldroot = b.root;
            // SAFETY: `oldroot` valid internal node.
            b.root = unsafe { node_child(oldroot, 0) };
            for m in 0..K_MT_META_COUNT {
                debug_assert_eq!(b.meta_root[m], unsafe { node_meta(oldroot, 0)[m] });
            }
            unsafe { (*b.root).parent = std::ptr::null_mut() };
            unsafe { marktree_free_node(b, oldroot) };
        } else {
            // No items, nothing for iterator to point to.
            itr.x = std::ptr::null_mut();
        }
    }

    if !itr.x.is_null() && itr_dirty {
        marktree_itr_fix_pos(b, itr);
    }

    // BONUS STEP: fix the iterator, so that it points to the key afterwards.
    // TODO(bfredl) (kept from the original): with "rev" should point before.
    if adjustment == -1 {
        // Tricky: we stand at the deleted space in the previous leaf node.
        // But the inner key is now the previous key we stole, so we need
        // to skip that one as well.
        marktree_itr_next(b, itr);
        marktree_itr_next(b, itr);
    } else if !itr.x.is_null() && itr.i >= unsafe { (*itr.x).n } {
        // We deleted the last key of a leaf node; go to the inner key after that.
        debug_assert_eq!(unsafe { (*itr.x).level }, 0);
        marktree_itr_next(b, itr);
    }

    other
}

/// `marktree_revise_meta`: after a key's flags have changed in a way that
/// might alter its meta-kind contribution (e.g. toggling a decoration
/// flag on an already-inserted key in place), recomputes the difference
/// against `old_key` and propagates it up through every ancestor's cached
/// meta counts (and `b.meta_root`) - avoiding a full node-by-node
/// `meta_describe_node` recount.
///
/// # Safety
/// `itr` must be a valid iterator into `b`'s tree.
pub unsafe fn marktree_revise_meta(b: &mut MarkTree, itr: &MarkTreeIter, old_key: &MtKey) {
    let meta_old = meta_describe_key(old_key);
    // SAFETY: caller guarantees `itr` is valid.
    let meta_new = meta_describe_key(unsafe { rawkey(itr) });

    if meta_old == meta_new {
        return;
    }

    let mut lnode = itr.x;
    // SAFETY: `lnode` valid; walks up valid parent pointers.
    while !unsafe { (*lnode).parent }.is_null() {
        let parent = unsafe { (*lnode).parent };
        let p_idx = unsafe { (*lnode).p_idx };
        let mut meta_p = unsafe { node_meta(parent, p_idx as usize) };
        for m in 0..K_MT_META_COUNT {
            meta_p[m] = (meta_p[m] as i64 + meta_new[m] as i64 - meta_old[m] as i64) as u32;
        }
        unsafe { node_set_meta(parent, p_idx as usize, meta_p) };
        lnode = parent;
    }

    for m in 0..K_MT_META_COUNT {
        b.meta_root[m] = (b.meta_root[m] as i64 + meta_new[m] as i64 - meta_old[m] as i64) as u32;
    }
}

/// `check_damage`: records that the key `itr1` currently refers to (which
/// must be one side of a paired mark) has effectively moved to where
/// `itr2` now points - used by [`swap_keys`] while splicing, so pair
/// intersections can be repaired afterward.
///
/// # Safety
/// `itr1`/`itr2` must be valid.
unsafe fn check_damage(damage: &mut Map<u64, MtDamagePair>, itr1: &MarkTreeIter, itr2: &MarkTreeIter) {
    // SAFETY: forwarded from caller.
    let start_id = mt_lookup_key_side(unsafe { rawkey(itr1) }, false);
    let mut p = damage.get_or_default(&start_id);
    let me = if unsafe { mt_end(rawkey(itr1)) } { &mut p.end } else { &mut p.start };
    debug_assert!(me.new.is_null());
    *me = crate::types_defs::MtDamage { old: itr1.x, new: itr2.x, old_i: itr1.i, new_i: itr2.i };
    damage.insert(start_id, p);
}

/// `swap_keys`: swaps the full key content (but not position) between the
/// slots `itr1`/`itr2` currently refer to, propagating meta-count deltas
/// up to their common ancestor if they live in different nodes, and
/// recording pair-movement info in `damage` for either side that is
/// paired (via [`check_damage`]).
///
/// # Safety
/// `itr1`/`itr2` must be valid and refer to two genuinely different key
/// slots (never the same slot - this is guaranteed by `marktree_splice`,
/// the only caller, which never invokes this when `itr_eq(itr1, itr2)`).
unsafe fn swap_keys(
    b: &mut MarkTree,
    itr1: &mut MarkTreeIter,
    itr2: &mut MarkTreeIter,
    damage: &mut Map<u64, MtDamagePair>,
) {
    // SAFETY: `itr1`/`itr2` valid.
    if unsafe { (*itr1.x).level } != 0 || itr1.x != itr2.x {
        if unsafe { mt_paired(rawkey(itr1)) } {
            unsafe { check_damage(damage, itr1, itr2) };
        }
        if unsafe { mt_paired(rawkey(itr2)) } {
            unsafe { check_damage(damage, itr2, itr1) };
        }
    }

    if itr1.x != itr2.x {
        let meta_inc_1 = meta_describe_key(unsafe { rawkey(itr1) });
        let meta_inc_2 = meta_describe_key(unsafe { rawkey(itr2) });

        if meta_inc_1 != meta_inc_2 {
            let mut x1 = itr1.x;
            let mut x2 = itr2.x;
            while x1 != x2 {
                // SAFETY: `x1`/`x2` valid; the root uniquely has the
                // highest level, so as long as `x1 != x2`, whichever of
                // the two has the (weakly) lower level cannot be the
                // root, and so must have a valid parent.
                if unsafe { (*x1).level } <= unsafe { (*x2).level } {
                    let p_idx = unsafe { (*x1).p_idx };
                    let parent = unsafe { (*x1).parent };
                    let mut meta_node = unsafe { node_meta(parent, p_idx as usize) };
                    for (dst, (&i1, &i2)) in meta_node.iter_mut().zip(meta_inc_1.iter().zip(meta_inc_2.iter())) {
                        // Unsigned modular arithmetic, matching the
                        // original's `uint32_t` `+=`/`-` exactly (the
                        // subtraction may "underflow" and wrap, but the
                        // final wrapping-add result is the same either way).
                        *dst = dst.wrapping_add(i2.wrapping_sub(i1));
                    }
                    unsafe { node_set_meta(parent, p_idx as usize, meta_node) };
                    x1 = parent;
                }
                if unsafe { (*x2).level } < unsafe { (*x1).level } {
                    let p_idx = unsafe { (*x2).p_idx };
                    let parent = unsafe { (*x2).parent };
                    let mut meta_node = unsafe { node_meta(parent, p_idx as usize) };
                    for (dst, (&i1, &i2)) in meta_node.iter_mut().zip(meta_inc_1.iter().zip(meta_inc_2.iter())) {
                        *dst = dst.wrapping_add(i1.wrapping_sub(i2));
                    }
                    unsafe { node_set_meta(parent, p_idx as usize, meta_node) };
                    x2 = parent;
                }
            }
        }
    }

    // SAFETY: `itr1`/`itr2` valid.
    let pos1 = unsafe { (*itr1.x).key[itr1.i as usize].pos };
    let pos2 = unsafe { (*itr2.x).key[itr2.i as usize].pos };
    if itr1.x == itr2.x {
        // SAFETY: same node - a plain slice swap (never the same index,
        // per this function's safety contract).
        unsafe { (*itr1.x).key.swap(itr1.i as usize, itr2.i as usize) };
    } else {
        // SAFETY: `itr1.x`/`itr2.x` are different allocations, so taking
        // one `&mut` into each simultaneously is sound.
        unsafe {
            std::mem::swap(&mut (*itr1.x).key[itr1.i as usize], &mut (*itr2.x).key[itr2.i as usize]);
        }
    }
    unsafe {
        (*itr1.x).key[itr1.i as usize].pos = pos1;
        (*itr2.x).key[itr2.i as usize].pos = pos2;
        refkey(b, itr1.x, itr1.i as usize);
        refkey(b, itr2.x, itr2.i as usize);
    }
}

/// `marktree_splice`: updates every mark's position to account for a text
/// edit replacing the `old_extent`-sized region starting at
/// `(start_line, start_col)` with a `new_extent`-sized region. Returns
/// `true` if any mark actually moved.
///
/// Follows the original's own documented strategy ("messing things up
/// and fix them later"): first (if the edit deletes text) every mark
/// inside the deleted region is collapsed onto the start position
/// (swapping right-gravity marks past the boundary with the last
/// non-right-gravity mark in the deleted range, via `swap_keys`, so
/// deleted-region marks end up in a well-defined relative order rather
/// than arbitrarily jumbled); then every mark after the edited region has
/// its position shifted by the row/col delta between the old and new
/// extents; finally, paired marks whose start/end nodes changed
/// ("damage") have their intersection tracking repaired.
pub fn marktree_splice(
    b: &mut MarkTree,
    start_line: i32,
    start_col: i32,
    old_extent_line: i32,
    old_extent_col: i32,
    new_extent_line: i32,
    new_extent_col: i32,
) -> bool {
    let start = MtPos::new(start_line, start_col);
    let mut old_extent = MtPos::new(old_extent_line, old_extent_col);
    let mut new_extent = MtPos::new(new_extent_line, new_extent_col);

    let mut may_delete = old_extent.row != 0 || old_extent.col != 0;
    let same_line = old_extent.row == 0 && new_extent.row == 0;
    unrelative(start, &mut old_extent);
    unrelative(start, &mut new_extent);
    let mut itr = MarkTreeIter::default();
    let mut enditr = MarkTreeIter::default();

    let mut oldbase = [MtPos::default(); crate::marktree_defs::MT_MAX_DEPTH];

    marktree_itr_get_ext(b, start, &mut itr, false, true, Some(&mut oldbase), None);
    if itr.x.is_null() {
        return false;
    }
    let delta = MtPos::new(new_extent.row - old_extent.row, new_extent.col - old_extent.col);

    if may_delete {
        // SAFETY: `itr` is valid (non-null `x`, checked above).
        let ipos = unsafe { marktree_itr_pos(&itr) };
        if !pos_leq(old_extent, ipos)
            || (old_extent.row == ipos.row && old_extent.col == ipos.col && !unsafe { mt_right(rawkey(&itr)) })
        {
            marktree_itr_get_ext(b, old_extent, &mut enditr, true, true, None, None);
            debug_assert!(!enditr.x.is_null());
            // "assert" (itr <= enditr).
        } else {
            may_delete = false;
        }
    }

    let mut past_right = false;
    let mut moved = false;
    let mut damage: Map<u64, MtDamagePair> = Map::default();

    // Follow the general strategy of messing things up and fix them later.
    // "oldbase" carries the information needed to calculate old position of
    // children.
    if may_delete {
        'outer: while !itr.x.is_null() && !past_right {
            let mut loc_start = start;
            let mut loc_old = old_extent;
            relative(itr.pos, &mut loc_start);
            relative(oldbase[itr.lvl as usize], &mut loc_old);

            // `continue_same_node:` in the original.
            loop {
                // SAFETY: `itr.x` non-null (outer loop condition).
                if !pos_leq(unsafe { rawkey(&itr) }.pos, loc_old) {
                    break 'outer;
                }

                if unsafe { mt_right(rawkey(&itr)) } {
                    while !unsafe { itr_eq(&itr, &enditr) } && unsafe { mt_right(rawkey(&enditr)) } {
                        marktree_itr_prev(b, &mut enditr);
                    }
                    if !unsafe { mt_right(rawkey(&enditr)) } {
                        unsafe { swap_keys(b, &mut itr, &mut enditr, &mut damage) };
                    } else {
                        // Matches the original's own `past_right = true; //
                        // NOLINT (void)past_right;` - the assignment is
                        // immediately followed by `break`, so it's
                        // observably redundant here too (kept for
                        // documentation/clarity, exactly like upstream).
                        #[allow(unused_assignments)]
                        {
                            past_right = true;
                        }
                        break 'outer;
                    }
                }

                if unsafe { itr_eq(&itr, &enditr) } {
                    // Actually, will be past_right after this key.
                    past_right = true;
                }

                moved = true;
                // SAFETY: `itr.x` valid.
                if unsafe { (*itr.x).level } != 0 {
                    let mut next_base = unsafe { rawkey(&itr) }.pos;
                    unrelative(oldbase[itr.lvl as usize], &mut next_base);
                    oldbase[itr.lvl as usize + 1] = next_base;
                    unsafe { (*itr.x).key[itr.i as usize].pos = loc_start };
                    marktree_itr_next_skip(&mut itr, false, false, Some(&mut oldbase), None);
                    break;
                } else {
                    unsafe { (*itr.x).key[itr.i as usize].pos = loc_start };
                    let x_n = unsafe { (*itr.x).n };
                    if itr.i < x_n - 1 {
                        itr.i += 1;
                        if !past_right {
                            continue;
                        } else {
                            break;
                        }
                    } else {
                        marktree_itr_next(b, &mut itr);
                        break;
                    }
                }
            }
        }

        'outer2: while !itr.x.is_null() {
            let mut loc_new = new_extent;
            relative(itr.pos, &mut loc_new);
            let mut limit = old_extent;
            relative(oldbase[itr.lvl as usize], &mut limit);

            // `past_continue_same_node:` in the original.
            loop {
                if pos_leq(limit, unsafe { rawkey(&itr) }.pos) {
                    break 'outer2;
                }

                let oldpos = unsafe { rawkey(&itr) }.pos;
                unsafe { (*itr.x).key[itr.i as usize].pos = loc_new };
                moved = true;
                // SAFETY: `itr.x` valid.
                if unsafe { (*itr.x).level } != 0 {
                    let mut next_base = oldpos;
                    unrelative(oldbase[itr.lvl as usize], &mut next_base);
                    oldbase[itr.lvl as usize + 1] = next_base;
                    marktree_itr_next_skip(&mut itr, false, false, Some(&mut oldbase), None);
                    break;
                } else {
                    let x_n = unsafe { (*itr.x).n };
                    if itr.i < x_n - 1 {
                        itr.i += 1;
                        continue;
                    } else {
                        marktree_itr_next(b, &mut itr);
                        break;
                    }
                }
            }
        }
    }

    while !itr.x.is_null() {
        // SAFETY: `itr.x` non-null (loop condition).
        unsafe { unrelative(oldbase[itr.lvl as usize], &mut (*itr.x).key[itr.i as usize].pos) };
        let realrow = unsafe { (*itr.x).key[itr.i as usize].pos.row };
        debug_assert!(realrow >= old_extent.row);
        let mut done = false;
        if realrow == old_extent.row {
            if delta.col != 0 {
                unsafe { (*itr.x).key[itr.i as usize].pos.col += delta.col };
            }
        } else if same_line {
            // Optimization: column only adjustment can skip remaining rows.
            done = true;
        }
        if delta.row != 0 {
            unsafe { (*itr.x).key[itr.i as usize].pos.row += delta.row };
            moved = true;
        }
        unsafe { relative(itr.pos, &mut (*itr.x).key[itr.i as usize].pos) };
        if done {
            break;
        }
        marktree_itr_next_skip(&mut itr, true, false, None, None);
    }

    let entries: Vec<(u64, MtDamagePair)> = damage.iter().map(|(&k, &v)| (k, v)).collect();
    for (start_id, d) in entries {
        if !d.start.old.is_null() && !d.end.old.is_null() {
            // Both ends of pair did move.
            // SAFETY: `d.start.old`/`d.end.old`/`d.start.new`/`d.end.new`
            // were recorded from live iterator positions in this same
            // splice call and remain valid node pointers.
            unsafe {
                marktree_itr_set_node(b, Some(&mut itr), d.start.old, d.start.old_i);
                marktree_itr_set_node(b, Some(&mut enditr), d.end.old, d.end.old_i);
            }
            marktree_intersect_pair(b, start_id, &mut itr, &enditr, true);
            unsafe {
                marktree_itr_set_node(b, Some(&mut itr), d.start.new, d.start.new_i);
                marktree_itr_set_node(b, Some(&mut enditr), d.end.new, d.end.new_i);
            }
            marktree_intersect_pair(b, start_id, &mut itr, &enditr, false);
        } else if !d.start.old.is_null() {
            // Only start did move.
            let mut endpos = MarkTreeIter::default();
            marktree_lookup(b, start_id | MARKTREE_END_FLAG, Some(&mut endpos));
            if !endpos.x.is_null() {
                // SAFETY: `d.start.old`/`d.start.new` valid (see above).
                unsafe { marktree_itr_set_node(b, Some(&mut itr), d.start.old, d.start.old_i) };
                enditr = endpos;
                marktree_intersect_pair(b, start_id, &mut itr, &enditr, true);
                unsafe { marktree_itr_set_node(b, Some(&mut itr), d.start.new, d.start.new_i) };
                enditr = endpos;
                marktree_intersect_pair(b, start_id, &mut itr, &enditr, false);
            }
        } else if !d.end.old.is_null() {
            // Only end did move.
            let mut startpos = MarkTreeIter::default();
            marktree_lookup(b, start_id, Some(&mut startpos));
            if !startpos.x.is_null() {
                itr = startpos;
                // SAFETY: `d.end.old`/`d.end.new` valid (see above).
                unsafe { marktree_itr_set_node(b, Some(&mut enditr), d.end.old, d.end.old_i) };
                marktree_intersect_pair(b, start_id, &mut itr, &enditr, true);
                itr = startpos;
                unsafe { marktree_itr_set_node(b, Some(&mut enditr), d.end.new, d.end.new_i) };
                marktree_intersect_pair(b, start_id, &mut itr, &enditr, false);
            }
        }
    }

    moved
}

/// `marktree_move_region`: moves every mark within the region
/// `[start, start+extent)` to start at `new_row`/`new_col` instead - used
/// for operations like folding a range of text elsewhere (e.g. `:move`).
/// Implemented directly on top of already-translated primitives: save
/// every mark inside the region (deleting them from the tree), splice the
/// old region out, splice a same-sized region in at the new location, then
/// re-insert the saved marks (relative to their original offsets within
/// the region) at the new location, restoring pair intersection tracking
/// for any that were paired.
pub fn marktree_move_region(
    b: &mut MarkTree,
    start_row: i32,
    start_col: crate::pos_defs::ColnrT,
    extent_row: i32,
    extent_col: crate::pos_defs::ColnrT,
    new_row: i32,
    new_col: crate::pos_defs::ColnrT,
) {
    let start = MtPos::new(start_row, start_col);
    let size = MtPos::new(extent_row, extent_col);
    let mut end = size;
    unrelative(start, &mut end);
    let mut itr = MarkTreeIter::default();
    marktree_itr_get_ext(b, start, &mut itr, false, true, None, None);
    let mut saved: Vec<MtKey> = Vec::new();
    while !itr.x.is_null() {
        let mut k = marktree_itr_current(&itr);
        if !pos_leq(k.pos, end) || (k.pos.row == end.row && k.pos.col == end.col && mt_right(&k)) {
            break;
        }
        relative(start, &mut k.pos);
        saved.push(k);
        // SAFETY: `itr` is valid (non-null, loop condition).
        unsafe { marktree_del_itr(b, &mut itr, false) };
    }

    marktree_splice(b, start.row, start.col, size.row, size.col, 0, 0);
    let new = MtPos::new(new_row, new_col);
    marktree_splice(b, new.row, new.col, 0, 0, size.row, size.col);

    for mut item in saved {
        unrelative(new, &mut item.pos);
        let restore_key = item.clone();
        marktree_put_key(b, item);
        if mt_paired(&restore_key) {
            // Other end might be later in `saved`; this will safely bail
            // out then.
            marktree_restore_pair(b, &restore_key);
        }
    }
}

/// `marktree_put_test`: convenience entry point used by the original's own
/// unit tests, translated as a thin wrapper matching its shape - now that
/// `marktree_put` itself is translated, this supports the paired case too
/// (`end_row >= 0`).
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn marktree_put_test(
    b: &mut MarkTree,
    ns: u32,
    id: u32,
    row: i32,
    col: i32,
    right_gravity: bool,
    end_row: i32,
    end_col: i32,
    end_right: bool,
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
    marktree_put(b, key, end_row, end_col, end_right);
}

/// `marktree_del_pair_test`: convenience entry point used by the
/// original's own unit tests - deletes both sides of a paired mark
/// identified by `(ns, id)`.
#[cfg(test)]
fn marktree_del_pair_test(b: &mut MarkTree, ns: u32, id: u32) {
    let mut itr = MarkTreeIter::default();
    marktree_lookup_ns(b, ns, id, false, Some(&mut itr));

    // SAFETY: `itr` was just positioned by the successful lookup above.
    let other = unsafe { marktree_del_itr(b, &mut itr, false) };
    assert_ne!(other, 0, "marktree_del_pair_test: mark (ns={ns}, id={id}) must be a paired mark");
    marktree_lookup(b, other, Some(&mut itr));
    // SAFETY: `itr` was just positioned by the successful lookup above.
    unsafe { marktree_del_itr(b, &mut itr, false) };
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
        // An internal node always has n >= 1 (the original assumes this
        // unguarded too - even the root, if internal, is never left with
        // n == 0: marktree_put_key's root-growth always immediately
        // split_node's the new root, which ends with n += 1).
        debug_assert!(n > 0);
        // SAFETY: `x` valid; `n > 0` (see above).
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
    fn intersect_merge_extracts_common_part_and_shrinks_both_inputs() {
        // "similar to intersect_common but modify x and y in place to
        // retain only the items which are NOT in common" (original's own
        // doc comment) - m gets the common part, x/y keep only their
        // unique-to-themselves elements.
        let mut x: Intersection = vec![1, 3, 5];
        let mut y: Intersection = vec![2, 3, 6];
        let mut m = Intersection::new();
        intersect_merge(&mut m, &mut x, &mut y);
        assert_eq!(m, vec![3]);
        assert_eq!(x, vec![1, 5]);
        assert_eq!(y, vec![2, 6]);
    }

    #[test]
    fn intersect_merge_disjoint_sets_leaves_both_unchanged() {
        let mut x: Intersection = vec![1, 3, 5];
        let mut y: Intersection = vec![2, 4, 6];
        let mut m = Intersection::new();
        intersect_merge(&mut m, &mut x, &mut y);
        assert!(m.is_empty());
        assert_eq!(x, vec![1, 3, 5]);
        assert_eq!(y, vec![2, 4, 6]);
    }

    /// Ground truth for `intersect_mov` taken directly from the original's
    /// own unit test (`test/unit/marktree_spec.lua`, "'intersect_mov'
    /// function works correctly", via the C-side `intersect_mov_test`
    /// helper) - verifying this Rust translation against the exact same
    /// input/output vectors upstream neovim itself uses to test the C
    /// version.
    fn mov(x: &[u64], y: &[u64], w: &[u64]) -> (Vec<u64>, Vec<u64>) {
        let x: Intersection = x.to_vec();
        let mut y: Intersection = y.to_vec();
        let mut w: Intersection = w.to_vec();
        let mut d = Intersection::new();
        intersect_mov(&x, &mut y, &mut w, &mut d);
        (w, d)
    }

    #[test]
    fn intersect_mov_matches_upstream_unit_test_vectors() {
        assert_eq!(mov(&[], &[2, 3], &[2, 3]), (vec![], vec![]));
        assert_eq!(mov(&[], &[], &[2, 3]), (vec![2, 3], vec![]));
        assert_eq!(mov(&[2, 3], &[], &[]), (vec![2, 3], vec![]));
        assert_eq!(mov(&[], &[2, 3], &[]), (vec![], vec![2, 3]));

        assert_eq!(mov(&[1, 2, 5], &[2, 3], &[3]), (vec![1, 5], vec![]));
        assert_eq!(mov(&[1, 2, 5], &[5, 10], &[10]), (vec![1, 2], vec![]));
        assert_eq!(mov(&[1, 2], &[5, 10], &[10]), (vec![1, 2], vec![5]));
        assert_eq!(
            mov(&[1, 3, 5, 7, 9], &[2, 4, 6, 8, 10], &[]),
            (vec![1, 3, 5, 7, 9], vec![2, 4, 6, 8, 10])
        );
        assert_eq!(
            mov(&[1, 3, 5, 7, 9], &[2, 4, 6, 8, 10], &[4, 8]),
            (vec![1, 3, 5, 7, 9], vec![2, 6, 10])
        );
        assert_eq!(
            mov(&[1, 3, 4, 6, 7, 9], &[2, 3, 5, 6, 8, 9], &[]),
            (vec![1, 4, 7], vec![2, 5, 8])
        );
        assert_eq!(
            mov(&[1, 3, 4, 6, 7, 9], &[2, 3, 5, 6, 8, 9], &[2, 5, 8]),
            (vec![1, 4, 7], vec![])
        );
        assert_eq!(
            mov(&[1, 3, 4, 6, 7, 9], &[2, 3, 5, 6, 8, 9], &[0, 2, 5, 8, 10]),
            (vec![0, 1, 4, 7, 10], vec![])
        );
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
            marktree_put_test(&mut tree, 0, i as u32, row, col, false, -1, -1, false, false);
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
            marktree_put_test(&mut tree, 0, i as u32, i, 0, false, -1, -1, false, false);
        }
        assert_eq!(tree.n_keys, 25);
        marktree_check(&tree);
        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn put_key_many_keys_ascending_multiple_levels() {
        let mut tree = MarkTree::default();
        for i in 0..300 {
            marktree_put_test(&mut tree, 0, i as u32, i, 0, i % 2 == 0, -1, -1, false, false);
        }
        assert_eq!(tree.n_keys, 300);
        marktree_check(&tree);
        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn put_key_many_keys_shuffled_order_maintains_invariants() {
        let mut tree = MarkTree::default();
        for (id, row) in shuffled_range(300).into_iter().enumerate() {
            marktree_put_test(&mut tree, 0, id as u32, row, 0, false, -1, -1, false, false);
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
            marktree_put_test(&mut tree, 0, id, 5, 5, id % 2 == 0, -1, -1, false, false);
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
            marktree_put_test(&mut tree, 0, i as u32, i, 0, false, -1, -1, false, i % 3 == 0);
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
                marktree_put_test(&mut tree, ns, id, row, ns as i32, false, -1, -1, false, false);
                id += 1;
            }
        }
        assert_eq!(tree.n_keys, 200);
        marktree_check(&tree);
        unsafe { marktree_clear(&mut tree) };
    }

    // --- iterator tests ---

    #[test]
    fn itr_first_next_visits_all_keys_in_ascending_order() {
        let mut tree = MarkTree::default();
        for (id, &row) in shuffled_range(150).iter().enumerate() {
            marktree_put_test(&mut tree, 0, id as u32, row, 0, false, -1, -1, false, false);
        }
        marktree_check(&tree);

        let mut itr = MarkTreeIter::default();
        assert!(marktree_itr_first(&tree, &mut itr));
        let mut rows = Vec::new();
        loop {
            let key = marktree_itr_current(&itr);
            assert!(key.pos.row >= 0, "current key must be valid while iterating");
            rows.push(key.pos.row);
            if !marktree_itr_next(&tree, &mut itr) {
                break;
            }
        }
        assert_eq!(rows.len(), 150);
        let mut expected: Vec<i32> = (0..150).collect();
        expected.sort_unstable();
        assert_eq!(rows, expected);
        // Iterator is exhausted: current() must now report invalid.
        assert!(itr.x.is_null());
        assert_eq!(marktree_itr_current(&itr).pos.row, -1);

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn itr_last_prev_visits_all_keys_in_descending_order() {
        let mut tree = MarkTree::default();
        for (id, &row) in shuffled_range(150).iter().enumerate() {
            marktree_put_test(&mut tree, 0, id as u32, row, 0, false, -1, -1, false, false);
        }
        marktree_check(&tree);

        let mut itr = MarkTreeIter::default();
        assert!(marktree_itr_last(&tree, &mut itr));
        let mut rows = Vec::new();
        loop {
            let key = marktree_itr_current(&itr);
            rows.push(key.pos.row);
            if !marktree_itr_prev(&tree, &mut itr) {
                break;
            }
        }
        assert_eq!(rows.len(), 150);
        let mut expected: Vec<i32> = (0..150).rev().collect();
        assert_eq!(rows, expected);
        // and verify the ascending set matches too, for good measure.
        expected.sort_unstable();
        rows.sort_unstable();
        assert_eq!(rows, expected);

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn itr_get_finds_first_key_at_or_after_gap_position() {
        let mut tree = MarkTree::default();
        // Keys at rows 0, 10, 20, ..., 190 (gaps in between).
        for id in 0..20 {
            marktree_put_test(&mut tree, 0, id, id as i32 * 10, 0, false, -1, -1, false, false);
        }
        marktree_check(&tree);

        // Querying a position in a gap should land on the next key >= it.
        let mut itr = MarkTreeIter::default();
        assert!(marktree_itr_get(&tree, 25, 0, &mut itr));
        assert_eq!(marktree_itr_current(&itr).pos, MtPos::new(30, 0));

        // Querying exactly on a key should land on that key.
        let mut itr2 = MarkTreeIter::default();
        assert!(marktree_itr_get(&tree, 100, 0, &mut itr2));
        assert_eq!(marktree_itr_current(&itr2).pos, MtPos::new(100, 0));

        // Querying past the last key finds nothing further.
        let mut itr3 = MarkTreeIter::default();
        assert!(!marktree_itr_get(&tree, 1000, 0, &mut itr3));

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn itr_current_on_default_iterator_is_invalid() {
        let itr = MarkTreeIter::default();
        assert!(!itr.is_valid());
        let key = marktree_itr_current(&itr);
        assert_eq!(key.pos, MtPos::new(-1, -1));
    }

    #[test]
    fn lookup_finds_inserted_keys_by_ns_and_id() {
        let mut tree = MarkTree::default();
        for id in 0..40u32 {
            marktree_put_test(&mut tree, 3, id, id as i32, id as i32 * 2, id % 2 == 0, -1, -1, false, false);
        }
        marktree_check(&tree);

        for id in 0..40u32 {
            let mut itr = MarkTreeIter::default();
            let key = marktree_lookup_ns(&tree, 3, id, false, Some(&mut itr));
            assert_eq!(key.pos, MtPos::new(id as i32, id as i32 * 2));
            assert_eq!(key.ns, 3);
            assert_eq!(key.id, id);
            assert_eq!(mt_right(&key), id % 2 == 0);
            assert!(itr.is_valid());
            // The iterator's own current-key view must agree.
            assert_eq!(marktree_itr_current(&itr).pos, key.pos);
        }

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn lookup_returns_invalid_key_for_unknown_id() {
        let mut tree = MarkTree::default();
        marktree_put_test(&mut tree, 0, 0, 5, 5, false, -1, -1, false, false);
        marktree_check(&tree);

        let mut itr = MarkTreeIter::default();
        let key = marktree_lookup_ns(&tree, 0, 999, false, Some(&mut itr));
        assert_eq!(key.pos, MtPos::new(-1, -1));
        assert!(!itr.is_valid());

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn get_alt_returns_self_for_unpaired_mark() {
        let mut tree = MarkTree::default();
        marktree_put_test(&mut tree, 0, 7, 3, 3, false, -1, -1, false, false);
        marktree_check(&tree);

        let mark = marktree_lookup_ns(&tree, 0, 7, false, None);
        assert!(!mt_paired(&mark));
        let alt = marktree_get_alt(&tree, &mark, None);
        assert_eq!(alt.pos, mark.pos);
        assert_eq!(marktree_get_altpos(&tree, &mark, None), mark.pos);

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn get_alt_finds_matching_pair_for_manually_paired_marks() {
        // marktree_put (the paired-mark-aware entry point) isn't translated
        // yet, but MT_FLAG_PAIRED/MT_FLAG_END plus two marktree_put_key
        // calls with matching (ns, id) let us exercise marktree_get_alt's
        // actual cross-lookup behavior directly.
        let mut tree = MarkTree::default();
        let start = MtKey {
            pos: MtPos::new(2, 0),
            ns: 5,
            id: 42,
            flags: MT_FLAG_PAIRED,
            decor_data: crate::decoration_defs::DecorInlineData { hl: DecorHighlightInline::default() },
        };
        let end = MtKey {
            pos: MtPos::new(9, 0),
            ns: 5,
            id: 42,
            flags: MT_FLAG_PAIRED | MT_FLAG_END,
            decor_data: crate::decoration_defs::DecorInlineData { hl: DecorHighlightInline::default() },
        };
        marktree_put_key(&mut tree, start);
        marktree_put_key(&mut tree, end);
        marktree_check(&tree);

        let start_mark = marktree_lookup_ns(&tree, 5, 42, false, None);
        assert!(mt_paired(&start_mark));
        assert!(!mt_end(&start_mark));
        let alt = marktree_get_alt(&tree, &start_mark, None);
        assert_eq!(alt.pos, MtPos::new(9, 0));
        assert!(mt_end(&alt));
        assert_eq!(marktree_get_altpos(&tree, &start_mark, None), MtPos::new(9, 0));

        let end_mark = marktree_lookup_ns(&tree, 5, 42, true, None);
        let alt_of_end = marktree_get_alt(&tree, &end_mark, None);
        assert_eq!(alt_of_end.pos, MtPos::new(2, 0));
        assert!(!mt_end(&alt_of_end));

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn put_test_paired_mark_creates_start_and_end_keys() {
        let mut tree = MarkTree::default();
        marktree_put_test(&mut tree, 0, 1, 2, 0, false, 9, 3, true, false);
        assert_eq!(tree.n_keys, 2);
        marktree_check(&tree);

        let start = marktree_lookup_ns(&tree, 0, 1, false, None);
        assert_eq!(start.pos, MtPos::new(2, 0));
        assert!(mt_paired(&start));
        assert!(!mt_end(&start));

        let end = marktree_lookup_ns(&tree, 0, 1, true, None);
        assert_eq!(end.pos, MtPos::new(9, 3));
        assert!(mt_paired(&end));
        assert!(mt_end(&end));
        assert!(mt_right(&end));

        assert_eq!(marktree_get_altpos(&tree, &start, None), MtPos::new(9, 3));
        assert_eq!(marktree_get_altpos(&tree, &end, None), MtPos::new(2, 0));

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn put_test_paired_mark_spanning_many_nodes_maintains_invariants() {
        // Enough unrelated keys to force multiple levels, then a single
        // wide pair spanning most of them - exercises marktree_intersect_pair
        // (via marktree_put) across real internal-node boundaries.
        let mut tree = MarkTree::default();
        for i in 0..300 {
            marktree_put_test(&mut tree, 0, i as u32 + 1, i, 0, false, -1, -1, false, false);
        }
        marktree_put_test(&mut tree, 1, 9999, 5, 0, false, 290, 0, false, false);
        assert_eq!(tree.n_keys, 302);
        marktree_check(&tree);

        let start = marktree_lookup_ns(&tree, 1, 9999, false, None);
        let end = marktree_lookup_ns(&tree, 1, 9999, true, None);
        assert_eq!(start.pos, MtPos::new(5, 0));
        assert_eq!(end.pos, MtPos::new(290, 0));
        assert_eq!(marktree_get_altpos(&tree, &start, None), MtPos::new(290, 0));

        // A key strictly between the pair's start/end should find the pair
        // among its ancestor chain's intersection sets (this is exactly
        // what marktree_itr_get_overlap/marktree_itr_step_overlap - not yet
        // translated - would use at query time; here we walk up manually
        // via itr.x.parent to confirm marktree_intersect_pair actually
        // marked something).
        let mut itr = MarkTreeIter::default();
        assert!(marktree_itr_get(&tree, 150, 0, &mut itr));
        let pair_start_id = mt_lookup_key(&start);
        let mut found = false;
        let mut x = itr.x;
        // SAFETY: itr.x and its ancestor chain are valid live nodes in `tree`.
        unsafe {
            while !x.is_null() {
                if intersection_has(&(*x).intersect, pair_start_id) {
                    found = true;
                    break;
                }
                x = (*x).parent;
            }
        }
        assert!(found, "expected the wide pair to be marked as intersecting somewhere on the ancestor chain of a key between its start and end");

        unsafe { marktree_clear(&mut tree) };
    }

    // --- deletion tests ---

    #[test]
    fn del_itr_removes_single_key_and_updates_bookkeeping() {
        let mut tree = MarkTree::default();
        for i in 0..10 {
            marktree_put_test(&mut tree, 0, i as u32, i, 0, false, -1, -1, false, false);
        }
        marktree_check(&tree);

        let mut itr = MarkTreeIter::default();
        assert!(marktree_itr_get(&tree, 5, 0, &mut itr));
        assert_eq!(marktree_itr_current(&itr).pos, MtPos::new(5, 0));
        // SAFETY: itr is a valid, freshly-positioned iterator into `tree`.
        let other = unsafe { marktree_del_itr(&mut tree, &mut itr, false) };
        assert_eq!(other, 0, "unpaired mark's other-side return must be 0");

        assert_eq!(tree.n_keys, 9);
        marktree_check(&tree);
        // The deleted key must no longer be findable.
        assert_eq!(marktree_lookup_ns(&tree, 0, 5, false, None).pos, MtPos::new(-1, -1));
        // The iterator must now point at the next key (row 6).
        assert_eq!(marktree_itr_current(&itr).pos, MtPos::new(6, 0));

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn del_itr_deleting_last_key_leaves_iterator_invalid() {
        let mut tree = MarkTree::default();
        marktree_put_test(&mut tree, 0, 0, 1, 0, false, -1, -1, false, false);
        marktree_check(&tree);

        let mut itr = MarkTreeIter::default();
        assert!(marktree_itr_get(&tree, 1, 0, &mut itr));
        // SAFETY: itr is valid.
        unsafe { marktree_del_itr(&mut tree, &mut itr, false) };
        assert_eq!(tree.n_keys, 0);
        // The root is a persistent leaf, kept (empty, n=0) rather than
        // freed/nulled when the tree becomes empty via deletion - this is
        // the original's own real behavior (an alloc/free-churn avoidance
        // optimization), confirmed by marktree_check_node's own invariant
        // assert allowing `x.n == 0` specifically for the root
        // (`assert(x->n >= (x != b->root ? T - 1 : 0));`). `root == NULL`
        // only represents the *pristine, never-inserted-into* state.
        assert!(!tree.root.is_null());
        assert_eq!(tree.n_nodes, 1);
        marktree_check(&tree);
        assert!(itr.x.is_null());

        unsafe { marktree_clear(&mut tree) };
    }

    /// Repeatedly deletes every key from a tree (one at a time, by
    /// position via a fresh `marktree_itr_get` lookup each time - matching
    /// how a real caller might delete arbitrary marks rather than only
    /// ever consuming the tree via a single forward sweep), verifying
    /// `marktree_check`'s full invariant set after *every single*
    /// deletion. This exercises every rebalancing path (steal-left,
    /// steal-right, merge-left, merge-right, root-shrink) many times over
    /// across a real, evolving tree shape - the same style of stress test
    /// already used for the insert/split path.
    fn del_all_keys_stress_test(rows: &[i32]) {
        let mut tree = MarkTree::default();
        for (id, &row) in rows.iter().enumerate() {
            marktree_put_test(&mut tree, 0, id as u32, row, 0, false, -1, -1, false, false);
        }
        assert_eq!(tree.n_keys, rows.len());
        marktree_check(&tree);

        let mut remaining = rows.len();
        for &row in rows {
            let mut itr = MarkTreeIter::default();
            assert!(marktree_itr_get(&tree, row, 0, &mut itr), "row {row} should still be found before deleting it");
            assert_eq!(marktree_itr_current(&itr).pos, MtPos::new(row, 0));
            // SAFETY: itr was just freshly positioned by marktree_itr_get above.
            unsafe { marktree_del_itr(&mut tree, &mut itr, false) };
            remaining -= 1;
            assert_eq!(tree.n_keys, remaining);
            marktree_check(&tree);
        }
        assert_eq!(tree.n_keys, 0);
        // The root persists as a single empty leaf rather than being freed
        // (see del_itr_deleting_last_key_leaves_iterator_invalid's comment
        // for why) - so the correct "no leaks" check is n_nodes == 1 (that
        // one persistent leaf), not 0.
        assert!(!tree.root.is_null());
        assert_eq!(tree.n_nodes, 1, "every node except the persistent empty root leaf must have been freed, no leaks");
    }

    #[test]
    fn del_itr_stress_ascending_insert_ascending_delete() {
        let rows: Vec<i32> = (0..300).collect();
        del_all_keys_stress_test(&rows);
    }

    #[test]
    fn del_itr_stress_ascending_insert_descending_delete() {
        let rows: Vec<i32> = (0..300).rev().collect();
        del_all_keys_stress_test(&rows);
    }

    #[test]
    fn del_itr_stress_shuffled_insert_shuffled_delete() {
        let rows = shuffled_range(300);
        del_all_keys_stress_test(&rows);
    }

    #[test]
    fn del_itr_stress_small_tree_all_orders() {
        // Small trees exercise root-shrink and single-leaf edge cases more
        // densely than the 300-key stress tests (which spend most of
        // their life multi-level).
        for n in [1, 2, 3, 5, 10, 19, 20, 21, 39, 40, 41] {
            let ascending: Vec<i32> = (0..n).collect();
            del_all_keys_stress_test(&ascending);
            let descending: Vec<i32> = (0..n).rev().collect();
            del_all_keys_stress_test(&descending);
        }
    }

    #[test]
    fn del_itr_paired_mark_returns_other_side_and_orphans_it() {
        let mut tree = MarkTree::default();
        // Some padding keys so the pair isn't trivially alone in one leaf.
        for i in 0..30 {
            marktree_put_test(&mut tree, 0, i as u32 + 100, i, 1, false, -1, -1, false, false);
        }
        marktree_put_test(&mut tree, 0, 1, 2, 0, false, 20, 0, true, false);
        marktree_check(&tree);

        let mut start_itr = MarkTreeIter::default();
        let start = marktree_lookup_ns(&tree, 0, 1, false, Some(&mut start_itr));
        assert_eq!(start.pos, MtPos::new(2, 0));
        let end_id_before = mt_lookup_key(&marktree_lookup_ns(&tree, 0, 1, true, None));

        // SAFETY: start_itr is a valid, freshly-positioned iterator.
        let other = unsafe { marktree_del_itr(&mut tree, &mut start_itr, false) };
        assert_eq!(other, end_id_before, "deleting one side of a pair must return the other side's lookup id");
        assert_eq!(tree.n_keys, 30 + 1, "only the start key should be gone (30 padding + 1 remaining end key)");
        marktree_check(&tree);

        // The remaining (end) side must now be orphaned.
        let end_after = marktree_lookup_ns(&tree, 0, 1, true, None);
        assert_eq!(end_after.pos, MtPos::new(20, 0));
        assert!(end_after.flags & MT_FLAG_ORPHANED != 0, "surviving side of a broken pair must be marked orphaned");

        // Deleting the now-orphaned remaining side must not try to
        // cross-delete anything else (other == 0, since MT_FLAG_ORPHANED
        // short-circuits the pair-lookup branch).
        let mut end_itr = MarkTreeIter::default();
        marktree_lookup_ns(&tree, 0, 1, true, Some(&mut end_itr));
        // SAFETY: end_itr is a valid, freshly-positioned iterator.
        let other2 = unsafe { marktree_del_itr(&mut tree, &mut end_itr, false) };
        assert_eq!(other2, 0);
        assert_eq!(tree.n_keys, 30);
        marktree_check(&tree);

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn del_itr_meta_root_tracks_remaining_inline_decor_count() {
        let mut tree = MarkTree::default();
        for i in 0..60 {
            marktree_put_test(&mut tree, 0, i as u32, i, 0, false, -1, -1, false, i % 2 == 0);
        }
        marktree_check(&tree);
        assert_eq!(tree.meta_root[MetaIndex::Inline as usize], 30);

        // Delete all the even (inline-flagged) keys.
        for i in (0..60).step_by(2) {
            let mut itr = MarkTreeIter::default();
            assert!(marktree_itr_get(&tree, i, 0, &mut itr));
            // SAFETY: itr freshly positioned above.
            unsafe { marktree_del_itr(&mut tree, &mut itr, false) };
        }
        marktree_check(&tree);
        assert_eq!(tree.n_keys, 30);
        assert_eq!(tree.meta_root[MetaIndex::Inline as usize], 0);

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn del_pair_test_removes_both_sides_of_a_pair() {
        let mut tree = MarkTree::default();
        for i in 0..30 {
            marktree_put_test(&mut tree, 0, i as u32 + 100, i, 1, false, -1, -1, false, false);
        }
        marktree_put_test(&mut tree, 2, 7, 3, 0, false, 25, 0, false, false);
        assert_eq!(tree.n_keys, 32);
        marktree_check(&tree);

        marktree_del_pair_test(&mut tree, 2, 7);
        assert_eq!(tree.n_keys, 30);
        marktree_check(&tree);
        assert_eq!(marktree_lookup_ns(&tree, 2, 7, false, None).pos, MtPos::new(-1, -1));
        assert_eq!(marktree_lookup_ns(&tree, 2, 7, true, None).pos, MtPos::new(-1, -1));

        unsafe { marktree_clear(&mut tree) };
    }

    // --- splice (text-edit position update) tests ---

    #[test]
    fn splice_pure_line_insert_shifts_marks_at_or_after_start() {
        let mut tree = MarkTree::default();
        for i in 0..10 {
            marktree_put_test(&mut tree, 0, i as u32, i, 0, false, -1, -1, false, false);
        }
        marktree_check(&tree);

        // Insert 2 new lines at row 5 (old_extent=0 rows deleted, new_extent=2 rows added).
        let moved = marktree_splice(&mut tree, 5, 0, 0, 0, 2, 0);
        assert!(moved);
        marktree_check(&tree);

        // Rows 0..=5 are unaffected: row 5 has default (left) gravity, so
        // a mark exactly at the insertion point stays attached to what's
        // *before* it and does not get pushed forward by the new lines -
        // standard editor mark-gravity semantics (matches
        // marktree_itr_get_ext's `gravity=true` search: it deliberately
        // lands *past* a left-gravity key at the exact search position).
        for i in 0..=5 {
            assert_eq!(marktree_lookup_ns(&tree, 0, i as u32, false, None).pos, MtPos::new(i, 0), "row {i} at/before the insert point must be unchanged (left gravity)");
        }
        for i in 6..10 {
            assert_eq!(
                marktree_lookup_ns(&tree, 0, i as u32, false, None).pos,
                MtPos::new(i + 2, 0),
                "row {i} strictly after the insert point must shift down by 2"
            );
        }

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn splice_pure_line_delete_collapses_and_shifts() {
        let mut tree = MarkTree::default();
        for i in 0..10 {
            marktree_put_test(&mut tree, 0, i as u32, i, 0, false, -1, -1, false, false);
        }
        marktree_check(&tree);

        // Delete 2 lines starting at row 3 (rows 3,4 are removed; row 5+ shift up by 2).
        let moved = marktree_splice(&mut tree, 3, 0, 2, 0, 0, 0);
        assert!(moved);
        marktree_check(&tree);
        assert_eq!(tree.n_keys, 10, "splice itself never removes marks, only repositions them");

        for i in 0..3 {
            assert_eq!(marktree_lookup_ns(&tree, 0, i as u32, false, None).pos, MtPos::new(i, 0), "row {i} before the deleted region must be unchanged");
        }
        // Marks that were inside the deleted region (rows 3, 4) collapse
        // onto the start of the edit.
        for i in 3..5 {
            assert_eq!(
                marktree_lookup_ns(&tree, 0, i as u32, false, None).pos,
                MtPos::new(3, 0),
                "row {i} was inside the deleted region and must collapse to the edit start"
            );
        }
        for i in 5..10 {
            assert_eq!(
                marktree_lookup_ns(&tree, 0, i as u32, false, None).pos,
                MtPos::new(i - 2, 0),
                "row {i} after the deleted region must shift up by 2"
            );
        }

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn splice_column_only_edit_on_same_line() {
        let mut tree = MarkTree::default();
        marktree_put_test(&mut tree, 0, 0, 5, 2, false, -1, -1, false, false);
        marktree_put_test(&mut tree, 0, 1, 5, 10, false, -1, -1, false, false);
        marktree_put_test(&mut tree, 0, 2, 6, 0, false, -1, -1, false, false);
        marktree_check(&tree);

        // Replace 3 columns with 5 columns at (5, 4): a pure same-line
        // column edit (old_extent/new_extent both have row == 0).
        //
        // Note: `marktree_splice`'s `moved` return value only reflects
        // *row*-level shifts (it's set exclusively in the `delta.row`
        // branch of the final position-fixup pass, per the original C -
        // a faithfully-translated, if perhaps surprising, real property
        // of the upstream algorithm) - so a pure column-only edit that
        // doesn't delete/collapse any existing mark can legitimately
        // return `false` even though a mark's column does shift, as
        // verified directly below via position checks instead.
        marktree_splice(&mut tree, 5, 4, 0, 3, 0, 5);
        marktree_check(&tree);

        // Mark before the edit column is untouched.
        assert_eq!(marktree_lookup_ns(&tree, 0, 0, false, None).pos, MtPos::new(5, 2));
        // Mark after the edit on the same line shifts by delta.col = 5-3 = 2.
        assert_eq!(marktree_lookup_ns(&tree, 0, 1, false, None).pos, MtPos::new(5, 12));
        // Mark on a later row is untouched (same_line optimization).
        assert_eq!(marktree_lookup_ns(&tree, 0, 2, false, None).pos, MtPos::new(6, 0));

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn splice_no_op_when_before_all_marks_returns_false_if_nothing_moves() {
        let mut tree = MarkTree::default();
        marktree_put_test(&mut tree, 0, 0, 5, 0, false, -1, -1, false, false);
        marktree_check(&tree);

        // An edit entirely after every existing mark: marktree_itr_get_ext
        // finds no key at/after the edit start, so nothing can move.
        let moved = marktree_splice(&mut tree, 100, 0, 0, 0, 1, 0);
        assert!(!moved);
        marktree_check(&tree);
        assert_eq!(marktree_lookup_ns(&tree, 0, 0, false, None).pos, MtPos::new(5, 0));

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn splice_paired_mark_spanning_edit_maintains_invariants() {
        let mut tree = MarkTree::default();
        for i in 0..40 {
            marktree_put_test(&mut tree, 0, i as u32 + 1000, i, 0, false, -1, -1, false, false);
        }
        // A pair spanning rows 5..35; an edit inside that range should
        // move only one side into the edited region while the other stays
        // outside - exercising the check_damage/swap_keys repair path.
        marktree_put_test(&mut tree, 1, 1, 5, 0, false, 35, 0, false, false);
        marktree_check(&tree);

        let moved = marktree_splice(&mut tree, 20, 0, 5, 0, 1, 0);
        assert!(moved);
        marktree_check(&tree);

        let start = marktree_lookup_ns(&tree, 1, 1, false, None);
        let end = marktree_lookup_ns(&tree, 1, 1, true, None);
        assert!(mt_paired(&start) && mt_paired(&end));
        assert_eq!(marktree_get_altpos(&tree, &start, None), end.pos);

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn splice_stress_sequence_of_edits_maintains_invariants() {
        let mut tree = MarkTree::default();
        for i in 0..200 {
            marktree_put_test(&mut tree, 0, i as u32, i, 0, i % 3 == 0, -1, -1, false, false);
        }
        // A few paired marks scattered through the tree too.
        for p in 0..10 {
            let s = p * 15;
            marktree_put_test(&mut tree, 1, p as u32, s, 0, false, s + 8, 0, true, false);
        }
        marktree_check(&tree);

        // A deterministic sequence of inserts/deletes/replacements at
        // varying positions, each re-verified via marktree_check.
        let edits: &[(i32, i32, i32, i32, i32, i32)] = &[
            (10, 0, 0, 0, 3, 0),  // insert 3 lines at row 10
            (50, 0, 5, 0, 0, 0),  // delete 5 lines at row 50
            (5, 2, 0, 4, 0, 0),   // insert 4 columns mid-line
            (100, 0, 2, 0, 2, 0), // replace 2 lines with 2 lines (net zero row delta)
            (0, 0, 1, 0, 0, 0),   // delete the very first line
            (150, 0, 0, 0, 10, 0), // insert 10 lines further down
            (30, 0, 20, 0, 1, 0), // collapse a large deleted range to 1 line
        ];
        for &(sl, sc, oel, oec, nel, nec) in edits {
            marktree_splice(&mut tree, sl, sc, oel, oec, nel, nec);
            marktree_check(&tree);
        }
        // marktree_splice never changes the number of marks, only
        // positions: 200 individual marks + 10 pairs (2 keys each) = 220.
        assert_eq!(tree.n_keys, 220);

        unsafe { marktree_clear(&mut tree) };
    }

    // --- marktree_move / marktree_move_region tests ---

    #[test]
    fn move_within_same_leaf_reorders_in_place() {
        let mut tree = MarkTree::default();
        for i in 0..10 {
            marktree_put_test(&mut tree, 0, i as u32, i, 0, false, -1, -1, false, false);
        }
        marktree_check(&tree);

        // Move the mark at row 2 to row 7 (still within the single leaf
        // this small tree lives in).
        let mut itr = MarkTreeIter::default();
        assert!(marktree_itr_get(&tree, 2, 0, &mut itr));
        // SAFETY: itr freshly positioned above.
        unsafe { marktree_move(&mut tree, &mut itr, 7, 3) };
        marktree_check(&tree);

        assert_eq!(marktree_lookup_ns(&tree, 0, 2, false, None).pos, MtPos::new(7, 3));
        // Every other mark must be untouched.
        for i in [0, 1, 3, 4, 5, 6, 7, 8, 9] {
            assert_eq!(marktree_lookup_ns(&tree, 0, i as u32, false, None).pos, MtPos::new(i, 0), "row {i} must be unaffected by moving a different mark");
        }

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn move_across_nodes_falls_back_to_delete_and_reinsert() {
        let mut tree = MarkTree::default();
        for i in 0..300 {
            marktree_put_test(&mut tree, 0, i as u32, i, 0, false, -1, -1, false, false);
        }
        marktree_check(&tree);

        // Move a mark from near the start to near the end - almost
        // certainly crosses node boundaries in a 300-key multi-level tree.
        let mut itr = MarkTreeIter::default();
        assert!(marktree_itr_get(&tree, 5, 0, &mut itr));
        // SAFETY: itr freshly positioned above.
        unsafe { marktree_move(&mut tree, &mut itr, 290, 9) };
        assert_eq!(tree.n_keys, 300, "move must not change the mark count");
        marktree_check(&tree);
        assert_eq!(marktree_lookup_ns(&tree, 0, 5, false, None).pos, MtPos::new(290, 9));

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn move_paired_mark_restores_pair_tracking() {
        let mut tree = MarkTree::default();
        for i in 0..100 {
            marktree_put_test(&mut tree, 0, i as u32 + 10, i, 0, false, -1, -1, false, false);
        }
        marktree_put_test(&mut tree, 5, 1, 10, 0, false, 40, 0, false, false);
        marktree_check(&tree);

        // Look up the pair's start mark specifically by (ns, id) rather
        // than by position: an individual mark (ns=0, id=20, from the
        // padding loop above) also happens to sit at (10, 0), and
        // `marktree_itr_get`'s position search has no way to distinguish
        // same-position keys by identity - only `marktree_lookup_ns` can.
        let mut itr = MarkTreeIter::default();
        marktree_lookup_ns(&tree, 5, 1, false, Some(&mut itr));
        assert!(itr.is_valid());
        // SAFETY: itr freshly positioned above.
        unsafe { marktree_move(&mut tree, &mut itr, 80, 0) };
        marktree_check(&tree);

        let start = marktree_lookup_ns(&tree, 5, 1, false, None);
        let end = marktree_lookup_ns(&tree, 5, 1, true, None);
        assert_eq!(start.pos, MtPos::new(80, 0));
        assert_eq!(end.pos, MtPos::new(40, 0));
        assert!(mt_paired(&start) && mt_paired(&end));
        assert_eq!(marktree_get_altpos(&tree, &start, None), end.pos);

        unsafe { marktree_clear(&mut tree) };
    }

    #[test]
    fn move_region_relocates_marks_inside_region_and_shifts_others() {
        let mut tree = MarkTree::default();
        for i in 0..30 {
            marktree_put_test(&mut tree, 0, i as u32, i, 0, false, -1, -1, false, false);
        }
        marktree_check(&tree);

        // Move the 5-row region starting at row 10 (rows 10..15 in extent
        // terms) to start at row 100 instead.
        marktree_move_region(&mut tree, 10, 0, 5, 0, 100, 0);
        marktree_check(&tree);
        assert_eq!(tree.n_keys, 30, "move_region must not change the mark count");

        // Marks strictly before the region are untouched.
        for i in 0..10 {
            assert_eq!(marktree_lookup_ns(&tree, 0, i as u32, false, None).pos, MtPos::new(i, 0));
        }
        // A mark position denotes a *gap* immediately before that point,
        // and (default/left) gravity means "stick with the gap's left
        // side" - so the collected/moved region is asymmetric:
        // exclusive of its own start (row 10 sticks to what's before it,
        // i.e. outside the region - matching marktree_splice's own
        // gravity-aware search boundary) but *inclusive* of its end (row
        // 15 sticks to the content just before it, i.e. the region's own
        // last bit of content) - standard editor mark-gravity semantics.
        assert_eq!(marktree_lookup_ns(&tree, 0, 10, false, None).pos, MtPos::new(10, 0));
        for i in 11..=15 {
            assert_eq!(marktree_lookup_ns(&tree, 0, i as u32, false, None).pos, MtPos::new(100 + (i - 10), 0));
        }
        // Marks strictly after the region shift up by the region's own
        // size (5 rows removed from their original location).
        for i in 16..30 {
            assert_eq!(marktree_lookup_ns(&tree, 0, i as u32, false, None).pos, MtPos::new(i - 5, 0));
        }

        unsafe { marktree_clear(&mut tree) };
    }
}
