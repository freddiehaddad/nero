//! Translated from `src/nvim/memfile.c` (partial).
//!
//! Translated: the pure in-memory block-allocation/free-list/hash-map
//! bookkeeping - `mf_new_page_size`, `mf_alloc_bhdr`, `mf_free_bhdr`,
//! `mf_ins_free`, `mf_rem_free`, `mf_new`, `mf_put` (its internal-error
//! `iemsg()` call on a "should never happen" invariant violation is
//! translated as a `debug_assert!` instead - see `mf_put`'s own doc
//! comment), `mf_free`, `mf_trans_add`, `mf_trans_del`, `mf_need_trans`,
//! `mf_free_fnames`, `mf_fullname`, `mf_set_fnames` (via
//! `crate::path::full_name_save`).
//!
//! Deferred (each needs real disk I/O or another not-yet-translated
//! subsystem):
//! - `mf_open`/`mf_open_file`/`mf_close`/`mf_close_file`/`mf_do_open`/
//!   `mf_read`/`mf_write`/`mf_sync`: need real file I/O beyond what
//!   `os/fs.rs` covers so far (byte-level read/write/seek). Planned:
//!   `MemfileT.mf_fd` should become `Option<std::fs::File>` (currently
//!   `i32`, matching the C field 1:1) when these are translated - see
//!   the session plan/stored memory for the reasoning.
//! - `mf_get`: calls `mf_read()` on a cache miss, so can't be completed
//!   without it.
//! - `mf_release_all`: calls `mf_close()`.

use crate::memfile_defs::{BhData, BhdrT, BlocknrT, MemfileT, MfdirtyT, BH_DIRTY, BH_LOCKED};
use crate::memory::{xfree, xmalloc};
use crate::vim_defs::OK;

/// `mf_new_page_size`.
pub fn mf_new_page_size(mfp: &mut MemfileT, new_size: u32) {
    mfp.mf_page_size = new_size;
}

/// Allocate a new `bhdr_T` with `page_count` pages of (zeroed by
/// `Vec::new`/`resize` default, matching `xmalloc`'s "just allocates,
/// doesn't zero" semantics being immediately overwritten by
/// `mf_new`/`mf_get`'s own explicit zero-fill anyway) data
/// (`mf_alloc_bhdr`).
///
/// Returns a raw pointer (`Box::into_raw`), matching the original's
/// `xmalloc(sizeof(bhdr_T))`: the header itself is heap-allocated and
/// managed manually (stored in `mf_hash`/the free list, freed later by
/// [`mf_free_bhdr`]), same convention as `MtNode` elsewhere in this
/// crate.
#[must_use]
pub fn mf_alloc_bhdr(mfp: &MemfileT, page_count: u32) -> *mut BhdrT {
    let data = xmalloc(mfp.mf_page_size as usize * page_count as usize);
    Box::into_raw(Box::new(BhdrT {
        bh_bnum: 0,
        bh_data: BhData::Data(data),
        bh_page_count: page_count,
        bh_flags: 0,
    }))
}

/// Free a block header and its block memory (`mf_free_bhdr`).
///
/// # Safety
/// `hp` must be a valid pointer previously returned by
/// [`mf_alloc_bhdr`] (or otherwise `Box::into_raw`-allocated as a
/// `BhdrT`), and must not be used again after this call - same
/// requirement as the original's `xfree(hp)`.
pub unsafe fn mf_free_bhdr(hp: *mut BhdrT) {
    // SAFETY: see function-level safety doc.
    let hp = unsafe { Box::from_raw(hp) };
    xfree(hp); // drops the Box (and its BhData::Data(Vec<u8>), if any)
}

/// Insert a block in the free list (`mf_ins_free`).
///
/// # Safety
/// `hp` must be a valid, currently-used-list `BhdrT` pointer (per the
/// original's own contract: inserting it repurposes `bh_data` to store
/// the free-list `next` pointer, discarding whatever data it held).
pub unsafe fn mf_ins_free(mfp: &mut MemfileT, hp: *mut BhdrT) {
    // SAFETY: see function-level safety doc.
    unsafe { (*hp).bh_data = BhData::FreeNext(mfp.mf_free_first) };
    mfp.mf_free_first = hp;
}

/// Remove the first block in the free list and return it
/// (`mf_rem_free`).
///
/// Caller must check that `mfp.mf_free_first` is not null.
///
/// # Safety
/// `mfp.mf_free_first` must be non-null and point at a valid `BhdrT`
/// whose `bh_data` is currently a [`BhData::FreeNext`] (i.e. it's
/// actually on the free list).
#[must_use]
pub unsafe fn mf_rem_free(mfp: &mut MemfileT) -> *mut BhdrT {
    let hp = mfp.mf_free_first;
    // SAFETY: see function-level safety doc.
    mfp.mf_free_first = match unsafe { &(*hp).bh_data } {
        BhData::FreeNext(next) => *next,
        BhData::Data(_) => panic!("mf_rem_free: mf_free_first was not on the free list"),
    };
    hp
}

/// Get a new block.
///
/// # Safety
/// Requires `mfp.mf_free_first` (if non-null) to be a well-formed free
/// list, per [`mf_rem_free`]/[`mf_ins_free`]'s own safety requirements.
///
/// @param negative    Whether a negative block number is desired (data block).
/// @param page_count  Desired number of pages.
#[must_use]
pub unsafe fn mf_new(mfp: &mut MemfileT, negative: bool, page_count: u32) -> *mut BhdrT {
    // Decide on the number to use:
    // If there is a free block, use its number.
    // Otherwise use mf_block_min for a negative number, mf_block_max for
    // a positive number.
    let freep = mfp.mf_free_first;
    let hp: *mut BhdrT;
    // SAFETY: freep, if non-null, is a valid free-list entry per this
    // function's own safety contract.
    let freep_page_count = if freep.is_null() { 0 } else { unsafe { (*freep).bh_page_count } };
    if !negative && !freep.is_null() && freep_page_count >= page_count {
        if freep_page_count > page_count {
            // If the block in the free list has more pages, take only
            // the number of pages needed and allocate a new bhdr_T with
            // data.
            hp = mf_alloc_bhdr(mfp, page_count);
            unsafe {
                (*hp).bh_bnum = (*freep).bh_bnum;
                (*freep).bh_bnum += BlocknrT::from(page_count);
                (*freep).bh_page_count -= page_count;
            }
        } else {
            // If the number of pages matches use the bhdr_T from the
            // free list and allocate the data.
            let data = xmalloc(mfp.mf_page_size as usize * page_count as usize);
            hp = unsafe { mf_rem_free(mfp) };
            unsafe { (*hp).bh_data = BhData::Data(data) };
        }
    } else {
        // get a new number
        hp = mf_alloc_bhdr(mfp, page_count);
        if negative {
            unsafe { (*hp).bh_bnum = mfp.mf_blocknr_min };
            mfp.mf_blocknr_min -= 1;
            mfp.mf_neg_count += 1;
        } else {
            unsafe { (*hp).bh_bnum = mfp.mf_blocknr_max };
            mfp.mf_blocknr_max += BlocknrT::from(page_count);
        }
    }
    unsafe {
        (*hp).bh_flags = BH_LOCKED | BH_DIRTY; // new block is always dirty
    }
    mfp.mf_dirty = MfdirtyT::Yes;
    unsafe {
        (*hp).bh_page_count = page_count;
    }
    let bnum = unsafe { (*hp).bh_bnum };
    mfp.mf_hash.insert(bnum, hp);

    // Init the data to all zero, to avoid reading uninitialized data.
    // This also avoids that the passwd file ends up in the swap file!
    unsafe {
        (*hp).bh_data.as_data_mut().fill(0);
    }

    hp
}

/// Signal that block `hp` has been modified, updating dirty state and
/// (if `infile`) translating its block number from negative to
/// positive (`mf_put`).
///
/// The original also calls `iemsg()` when `hp.bh_flags` doesn't have
/// `BH_LOCKED` set - an internal-consistency check for a "should never
/// happen" caller bug, not a recoverable/user-facing error path (the
/// function's own logic proceeds identically either way). Translated
/// as a `debug_assert!` instead of deferring the whole function on
/// `message.c`'s `iemsg()`: this matches Rust's own idiom for "this
/// invariant must always hold; a violation is a real bug", the same
/// intent as the original's internal-error message.
///
/// # Safety
/// `hp` must be a valid `BhdrT` pointer.
pub unsafe fn mf_put(mfp: &mut MemfileT, hp: *mut BhdrT, dirty: bool, infile: bool) {
    // SAFETY: caller contract (see function doc).
    let mut flags = unsafe { (*hp).bh_flags };

    debug_assert!(flags & BH_LOCKED != 0, "block was not locked");
    flags &= !BH_LOCKED;
    if dirty {
        flags |= BH_DIRTY;
        if mfp.mf_dirty != MfdirtyT::YesNosync {
            mfp.mf_dirty = MfdirtyT::Yes;
        }
    }
    unsafe { (*hp).bh_flags = flags };
    if infile {
        // SAFETY: caller contract (see function doc).
        unsafe { mf_trans_add(mfp, hp) }; // may translate negative in positive nr
    }
}

/// Signal block as no longer used (may put it in the free list)
/// (`mf_free`).
///
/// # Safety
/// `hp` must be a valid, currently-used-list `BhdrT` pointer registered
/// in `mfp.mf_hash`.
pub unsafe fn mf_free(mfp: &mut MemfileT, hp: *mut BhdrT) {
    // SAFETY: see function-level safety doc.
    let bnum = unsafe { (*hp).bh_bnum };
    unsafe {
        // free data (`BhData::Data`'s `Vec<u8>` is simply replaced/
        // dropped, matching `xfree(hp->bh_data)`)
        (*hp).bh_data = BhData::FreeNext(std::ptr::null_mut());
    }
    mfp.mf_hash.remove(&bnum); // get *hp out of the hash table
    if bnum < 0 {
        // SAFETY: hp was allocated by mf_alloc_bhdr (Box::into_raw).
        unsafe { mf_free_bhdr(hp) }; // don't want negative numbers in free list
        mfp.mf_neg_count -= 1;
    } else {
        // SAFETY: see function-level safety doc.
        unsafe { mf_ins_free(mfp, hp) }; // put *hp in the free list
    }
}

/// # Safety
/// `hp` must be a valid `BhdrT` pointer.
unsafe fn mf_trans_add(mfp: &mut MemfileT, hp: *mut BhdrT) -> i32 {
    // SAFETY: caller contract (see function doc); all internal derefs
    // stay within this function's own borrow.
    let bh_bnum = unsafe { (*hp).bh_bnum };
    if bh_bnum >= 0 {
        // it's already positive
        return OK;
    }

    // Get a new number for the block.
    // If the first item in the free list has sufficient pages, use its
    // number. Otherwise use mf_blocknr_max.
    let page_count = unsafe { (*hp).bh_page_count };
    let freep = mfp.mf_free_first;
    let new_bnum: BlocknrT;
    if !freep.is_null() && unsafe { (*freep).bh_page_count } >= page_count {
        new_bnum = unsafe { (*freep).bh_bnum };
        // If the page count of the free block was larger, reduce it.
        // If the page count matches, remove the block from the free
        // list.
        if unsafe { (*freep).bh_page_count } > page_count {
            unsafe {
                (*freep).bh_bnum += BlocknrT::from(page_count);
                (*freep).bh_page_count -= page_count;
            }
        } else {
            let freep = unsafe { mf_rem_free(mfp) };
            unsafe { mf_free_bhdr(freep) };
        }
    } else {
        new_bnum = mfp.mf_blocknr_max;
        mfp.mf_blocknr_max += BlocknrT::from(page_count);
    }

    let old_bnum = bh_bnum; // adjust number
    mfp.mf_hash.remove(&bh_bnum);
    unsafe { (*hp).bh_bnum = new_bnum };
    mfp.mf_hash.insert(new_bnum, hp);

    // Insert "np" into "mf_trans" hashtable with key "np->nt_old_bnum".
    mfp.mf_trans.insert(old_bnum, new_bnum);

    OK
}

/// Lookup translation from trans list and delete the entry
/// (`mf_trans_del`).
///
/// @return  The positive new number  When found.
///          The old number           When not found.
#[must_use]
pub fn mf_trans_del(mfp: &mut MemfileT, old_nr: BlocknrT) -> BlocknrT {
    let Some(new_bnum) = mfp.mf_trans.get(&old_nr).copied() else {
        return old_nr; // not found
    };

    mfp.mf_neg_count -= 1;

    // remove entry from the trans list
    mfp.mf_trans.remove(&old_nr);

    new_bnum
}

/// Frees `mf_fname` and `mf_ffname` (`mf_free_fnames`).
pub fn mf_free_fnames(mfp: &mut MemfileT) {
    mfp.mf_fname = None;
    mfp.mf_ffname = None;
}

/// Sets the memfile's swapfile name, also computing and storing its
/// full (absolute) path (`mf_set_fnames`).
///
/// Computes `mf_ffname` before moving `fname` into `mf_fname` (the
/// original assigns `mf_fname` first, then reads it back to compute
/// `mf_ffname` - reordered here only to satisfy Rust's ownership rules
/// around moving `fname`; `full_name_save` is a pure function of its
/// input, so this has no observable behavior difference).
pub fn mf_set_fnames(mfp: &mut MemfileT, fname: Vec<u8>) {
    mfp.mf_ffname = crate::path::full_name_save(Some(&fname), false);
    mfp.mf_fname = Some(fname);
}

/// Make name of memfile's swapfile a full path. Used before doing a
/// `:cd` (`mf_fullname`).
pub fn mf_fullname(mfp: &mut MemfileT) {
    if mfp.mf_fname.is_none() || mfp.mf_ffname.is_none() {
        return;
    }
    mfp.mf_fname = mfp.mf_ffname.take();
}

/// Return true if there are any translations pending for memfile
/// (`mf_need_trans`).
#[must_use]
pub fn mf_need_trans(mfp: &MemfileT) -> bool {
    mfp.mf_fname.is_some() && mfp.mf_neg_count > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Map;

    fn test_mfp() -> MemfileT {
        MemfileT {
            mf_page_size: 4096,
            ..default_memfile()
        }
    }

    fn default_memfile() -> MemfileT {
        MemfileT {
            mf_fname: None,
            mf_ffname: None,
            mf_fd: -1,
            mf_flags: 0,
            mf_reopen: false,
            mf_free_first: std::ptr::null_mut(),
            mf_hash: Map::default(),
            mf_trans: Map::default(),
            mf_blocknr_max: 0,
            mf_blocknr_min: -1,
            mf_neg_count: 0,
            mf_infile_count: 0,
            mf_page_size: 4096,
            mf_dirty: MfdirtyT::No,
        }
    }

    /// Drains and truly deallocates every block still on `mfp`'s free
    /// list. `mf_free()` only moves positive-numbered blocks onto the
    /// free list (matching the original: nothing actually deallocates a
    /// memfile's free list except `mf_close()`, not yet translated) -
    /// tests that end with a positive-numbered block freed via
    /// `mf_free()` call this so they don't leak past their own scope.
    ///
    /// # Safety
    /// `mfp.mf_free_first`, if non-null, must be a well-formed free
    /// list (true for every test in this module).
    unsafe fn drain_free_list(mfp: &mut MemfileT) {
        while !mfp.mf_free_first.is_null() {
            let hp = unsafe { mf_rem_free(mfp) };
            unsafe { mf_free_bhdr(hp) };
        }
    }

    #[test]
    fn mf_new_page_size_sets_field() {
        let mut mfp = test_mfp();
        mf_new_page_size(&mut mfp, 8192);
        assert_eq!(mfp.mf_page_size, 8192);
    }

    #[test]
    fn mf_alloc_and_free_bhdr_roundtrip() {
        let mfp = test_mfp();
        let hp = mf_alloc_bhdr(&mfp, 2);
        unsafe {
            assert_eq!((*hp).bh_page_count, 2);
            assert_eq!((*hp).bh_data.as_data().len(), 4096 * 2);
            mf_free_bhdr(hp);
        }
    }

    #[test]
    fn mf_ins_and_rem_free_roundtrip() {
        let mut mfp = test_mfp();
        let hp1 = mf_alloc_bhdr(&mfp, 1);
        let hp2 = mf_alloc_bhdr(&mfp, 1);
        unsafe {
            (*hp1).bh_bnum = 10;
            (*hp2).bh_bnum = 20;
            mf_ins_free(&mut mfp, hp1);
            mf_ins_free(&mut mfp, hp2);
            // Most-recently-inserted comes out first (single-linked
            // stack-like list, matching the original's own semantics).
            let out1 = mf_rem_free(&mut mfp);
            assert_eq!((*out1).bh_bnum, 20);
            let out2 = mf_rem_free(&mut mfp);
            assert_eq!((*out2).bh_bnum, 10);
            assert!(mfp.mf_free_first.is_null());
            mf_free_bhdr(out1);
            mf_free_bhdr(out2);
        }
    }

    #[test]
    fn mf_new_allocates_positive_and_negative_numbers() {
        let mut mfp = test_mfp();
        unsafe {
            let neg = mf_new(&mut mfp, true, 1);
            assert_eq!((*neg).bh_bnum, -1);
            assert_eq!(mfp.mf_neg_count, 1);
            assert_eq!(mfp.mf_blocknr_min, -2);

            let pos = mf_new(&mut mfp, false, 1);
            assert_eq!((*pos).bh_bnum, 0);
            assert_eq!(mfp.mf_blocknr_max, 1);

            assert_eq!(mfp.mf_hash.get(&-1).copied(), Some(neg));
            assert_eq!(mfp.mf_hash.get(&0).copied(), Some(pos));

            // data should be zero-filled
            assert!((*neg).bh_data.as_data().iter().all(|&b| b == 0));

            mf_free(&mut mfp, neg); // negative bnum: truly deallocated
            mf_free(&mut mfp, pos); // positive bnum: moved to free list
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_new_reuses_free_list_entry_of_matching_size() {
        let mut mfp = test_mfp();
        unsafe {
            let hp = mf_new(&mut mfp, false, 2);
            let bnum = (*hp).bh_bnum;
            mf_free(&mut mfp, hp);
            assert!(!mfp.mf_free_first.is_null());

            // Requesting the same page count should reuse the freed
            // block's number rather than growing mf_blocknr_max again.
            let hp2 = mf_new(&mut mfp, false, 2);
            assert_eq!((*hp2).bh_bnum, bnum);
            mf_free(&mut mfp, hp2);

            // hp2 (same underlying allocation as hp, reused via the free
            // list) is back on the free list now, not actually
            // deallocated - matching the original's own manual-lifetime
            // model (nothing frees a memfile's free list except
            // mf_close(), not yet translated). Drain it here so this
            // test doesn't leak the block past its own scope.
            assert!(!mfp.mf_free_first.is_null());
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_put_clears_locked_and_sets_dirty() {
        let mut mfp = test_mfp();
        unsafe {
            let hp = mf_new(&mut mfp, false, 1); // starts BH_LOCKED | BH_DIRTY
            (*hp).bh_flags &= !BH_DIRTY; // clear dirty to isolate mf_put's own effect
            mfp.mf_dirty = MfdirtyT::No;

            mf_put(&mut mfp, hp, true, false);

            assert_eq!((*hp).bh_flags & BH_LOCKED, 0); // unlocked
            assert_eq!((*hp).bh_flags & BH_DIRTY, BH_DIRTY); // dirty
            assert_eq!(mfp.mf_dirty, MfdirtyT::Yes);

            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_put_preserves_yes_nosync_dirty_state() {
        let mut mfp = test_mfp();
        unsafe {
            let hp = mf_new(&mut mfp, false, 1);
            mfp.mf_dirty = MfdirtyT::YesNosync;

            mf_put(&mut mfp, hp, true, false);

            // MF_DIRTY_YES_NOSYNC must not be downgraded to plain Yes.
            assert_eq!(mfp.mf_dirty, MfdirtyT::YesNosync);

            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_put_not_dirty_leaves_dirty_state_unchanged() {
        let mut mfp = test_mfp();
        unsafe {
            let hp = mf_new(&mut mfp, false, 1);
            mfp.mf_dirty = MfdirtyT::No;

            mf_put(&mut mfp, hp, false, false);

            assert_eq!(mfp.mf_dirty, MfdirtyT::No);
            assert_eq!((*hp).bh_flags & BH_DIRTY, BH_DIRTY); // untouched from mf_new

            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_put_infile_translates_negative_block_number() {
        let mut mfp = test_mfp();
        unsafe {
            let hp = mf_new(&mut mfp, true, 1); // negative bnum
            assert!((*hp).bh_bnum < 0);

            mf_put(&mut mfp, hp, true, true);

            // infile=true should have run mf_trans_add, giving it a
            // fresh non-negative number.
            assert!((*hp).bh_bnum >= 0);

            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_trans_add_translates_negative_to_positive() {
        let mut mfp = test_mfp();
        unsafe {
            let hp = mf_new(&mut mfp, true, 1);
            let old_bnum = (*hp).bh_bnum;
            assert!(old_bnum < 0);
            let ret = mf_trans_add(&mut mfp, hp);
            assert_eq!(ret, OK);
            assert!((*hp).bh_bnum >= 0);
            assert_eq!(mfp.mf_hash.get(&(*hp).bh_bnum).copied(), Some(hp));
            assert!(mfp.mf_hash.get(&old_bnum).is_none());

            let new_bnum = mf_trans_del(&mut mfp, old_bnum);
            assert_eq!(new_bnum, (*hp).bh_bnum);
            // entry removed after lookup
            assert!(mfp.mf_trans.get(&old_bnum).is_none());

            mf_free(&mut mfp, hp); // hp.bh_bnum is now positive: free list
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_trans_add_is_noop_for_already_positive_block() {
        let mut mfp = test_mfp();
        unsafe {
            let hp = mf_new(&mut mfp, false, 1);
            let bnum_before = (*hp).bh_bnum;
            let ret = mf_trans_add(&mut mfp, hp);
            assert_eq!(ret, OK);
            assert_eq!((*hp).bh_bnum, bnum_before);
            mf_free(&mut mfp, hp); // positive bnum: free list
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_trans_del_returns_old_nr_when_not_found() {
        let mut mfp = test_mfp();
        assert_eq!(mf_trans_del(&mut mfp, -42), -42);
    }

    #[test]
    fn mf_need_trans_requires_fname_and_neg_count() {
        let mut mfp = test_mfp();
        assert!(!mf_need_trans(&mfp));
        mfp.mf_fname = Some(b"swap".to_vec());
        assert!(!mf_need_trans(&mfp));
        mfp.mf_neg_count = 1;
        assert!(mf_need_trans(&mfp));
    }

    #[test]
    fn mf_free_fnames_clears_both_names() {
        let mut mfp = test_mfp();
        mfp.mf_fname = Some(b"a".to_vec());
        mfp.mf_ffname = Some(b"/full/a".to_vec());
        mf_free_fnames(&mut mfp);
        assert!(mfp.mf_fname.is_none());
        assert!(mfp.mf_ffname.is_none());
    }

    #[test]
    fn mf_fullname_replaces_fname_with_ffname() {
        let mut mfp = test_mfp();
        mfp.mf_fname = Some(b"a".to_vec());
        mfp.mf_ffname = Some(b"/full/a".to_vec());
        mf_fullname(&mut mfp);
        assert_eq!(mfp.mf_fname, Some(b"/full/a".to_vec()));
        assert!(mfp.mf_ffname.is_none());
    }

    #[test]
    fn mf_fullname_noop_when_either_name_missing() {
        let mut mfp = test_mfp();
        mf_fullname(&mut mfp); // both None
        assert!(mfp.mf_fname.is_none());
    }

    #[test]
    fn mf_set_fnames_sets_fname_and_computes_absolute_ffname() {
        let mut mfp = test_mfp();
        mf_set_fnames(&mut mfp, b"swap.tmp".to_vec());
        assert_eq!(mfp.mf_fname, Some(b"swap.tmp".to_vec()));
        let ffname = mfp.mf_ffname.expect("full_name_save should succeed for a plain relative name");
        assert!(crate::path::path_is_absolute(&ffname));
        assert!(ffname.ends_with(b"swap.tmp"));
    }
}
