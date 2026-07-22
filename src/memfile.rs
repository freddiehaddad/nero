//! Translated from `src/nvim/memfile.c` (partial).
//!
//! Translated: the pure in-memory block-allocation/free-list/hash-map
//! bookkeeping - `mf_new_page_size`, `mf_alloc_bhdr`, `mf_free_bhdr`,
//! `mf_ins_free`, `mf_rem_free`, `mf_new`, `mf_put` (its internal-error
//! `iemsg()` call on a "should never happen" invariant violation is
//! translated as a `debug_assert!` instead - see `mf_put`'s own doc
//! comment), `mf_free`, `mf_trans_add`, `mf_trans_del`, `mf_need_trans`,
//! `mf_free_fnames`, `mf_fullname`, `mf_set_fnames` (via
//! `crate::path::full_name_save`), `mf_read`, `mf_write`, `mf_close`,
//! `mf_sync` (`memfile.h`'s `MFS_*` flags moved here alongside it -
//! same "header content lives with its .c file's translation"
//! convention already used for `mark.h`), `mf_set_dirty`, `mf_open`,
//! `mf_open_file`, `mf_do_open`, `mf_get`.
//!
//! `mf_read`/`mf_write`/`mf_close` each call `PERROR()`/`emsg()`
//! (`message.c`) on their error paths purely as a side effect before
//! returning/continuing - this doesn't change any of their own control
//! flow (they already know what to do regardless of whether the
//! message displays), so the message display itself is omitted here
//! rather than blocking translation on `message.c`; `did_swapwrite_msg`
//! (`crate::globals::GLOBALS`, a real `EXTERN` global, not a stub) is
//! still updated faithfully by `mf_write`, since it's genuine program
//! state read elsewhere (`input.c`), not just message-display
//! plumbing. `mf_write` additionally omits its retry-with-reopen-on-
//! failure fallback (recovering from e.g. a disconnected network
//! drive): that needs `mf_do_open`'s flag-translation logic, which now
//! exists, but wiring the retry loop itself is left for a future pass
//! (documented on `mf_write`'s own doc comment as a narrow, explicit
//! gap, not silently dropped). `mf_do_open` itself likewise omits
//! `emsg()`'s "E300: swap file already exists" message text under the
//! same policy - the real, observable effect (`mf_fd` staying `None`)
//! is preserved faithfully.
//!
//! `mf_close` takes `mfp: MemfileT` *by value* (unlike this file's
//! other functions, which take `&mut MemfileT`, since `mf_open` now
//! constructs an owned `MemfileT` to hand off). This still matches the
//! original's own "frees `mfp` itself" contract: the caller can no
//! longer use `mfp` after calling this, exactly like the original's
//! pointer becomes dangling after `mf_close()` - Rust's ordinary
//! `Drop` at the end of the function body plays the role of the
//! original's explicit `xfree(mfp)`.
//!
//! `mf_sync` (translated) omits its interruptibility mechanism
//! (`os_char_avail()`/`os_breakcheck()`, needing `os/input.c`/the event
//! loop, phase 11): the original clears `got_int` at entry specifically
//! so a *pre-existing* interrupt flag never aborts the sync early, and
//! nothing else in this simplified version ever sets it back to `true`
//! mid-sync (since `os_breakcheck()` itself is what would do that), so
//! the final `if got_int { break; }` check can never trigger here and
//! is correctly omitted rather than silently changed.
//!
//! `mf_open` omits the original's page-size-from-device-block-size
//! optimization (`os_fileinfo_fd`/`os_fileinfo_blocksize`, needing the
//! still-deferred `FileInfo` struct): `mf_page_size` always keeps its
//! `MEMFILE_PAGE_SIZE` default instead. This is purely a performance
//! tuning step - any page size is equally *correct* - so it is
//! documented as an accepted, narrow gap rather than blocking `mf_open`
//! on the full `FileInfo` translation.
//!
//! Deferred (each needs another not-yet-translated subsystem):
//! - `mf_close_file`: needs `ml_get_buf` (`memline.c`) for its
//!   `getlines` branch.
//! - `mf_release_all`: calls `mf_close()`/`mf_write()` (done) but also
//!   iterates `first_buffer`'s buffer list (`globals.h`) and
//!   `curbuf`/window state not yet wired up to real multi-buffer
//!   support, plus `memline.c`'s `ml_open_file`.

use crate::memfile_defs::{BhData, BhdrT, BlocknrT, MemfileT, MfdirtyT, BH_DIRTY, BH_LOCKED};
use crate::memory::{xfree, xmalloc};
use crate::vim_defs::{FAIL, OK};
use std::io::{Read, Seek, SeekFrom, Write};

/// Default page size for a new memfile, in bytes (`MEMFILE_PAGE_SIZE`,
/// `memfile.c`).
pub const MEMFILE_PAGE_SIZE: u32 = 4096;

/// Minimum page size this crate would accept from the underlying
/// device's block size (`MIN_SWAP_PAGE_SIZE`, `memfile.h`) - not
/// currently read anywhere, since the block-size-detection step that
/// would consult it is deferred (see the module doc comment); kept
/// here for documentation/future use, matching the original header's
/// own declared constant.
pub const MIN_SWAP_PAGE_SIZE: u64 = 1048;
/// Maximum page size this crate would accept from the underlying
/// device's block size (`MAX_SWAP_PAGE_SIZE`, `memfile.h`) - see
/// [`MIN_SWAP_PAGE_SIZE`].
pub const MAX_SWAP_PAGE_SIZE: u64 = 50000;

/// Flags for [`mf_sync`] (`memfile.h`'s anonymous `enum`).
pub mod mfs_flag {
    /// Also sync blocks with negative numbers (`MFS_ALL`).
    pub const ALL: i32 = 1;
    /// Stop syncing when a character is available (`MFS_STOP`) - not
    /// enforced by this crate's `mf_sync` yet, see the module doc
    /// comment.
    pub const STOP: i32 = 2;
    /// Flush the file to disk (`MFS_FLUSH`).
    pub const FLUSH: i32 = 4;
    /// Only write block 0 (`MFS_ZERO`).
    pub const ZERO: i32 = 8;
}

/// Open a new or existing memfile (`mf_open`).
///
/// `fname`:
/// - `None`: no file, use memory only.
/// - `Some(fname)`: should correspond to an existing file (or, with
///   `O_CREAT`, a file to create) - consumed either way, matching the
///   original's "must have been allocated... freed if opening fails"
///   contract (`Vec<u8>` ownership makes this automatic).
///
/// @return `Some(mfp)` on success, `None` on failure (e.g. file does
///         not exist).
#[must_use]
pub fn mf_open(fname: Option<Vec<u8>>, flags: i32) -> Option<MemfileT> {
    // Start with the "no file, memory only" defaults for every field;
    // mf_fname/mf_ffname/mf_fd are overwritten by mf_do_open below
    // when fname is Some, matching the original's own
    // field-by-field construction order.
    let mut mfp = MemfileT {
        mf_fname: None,
        mf_ffname: None,
        mf_fd: None,
        mf_flags: 0,
        mf_reopen: false,
        mf_free_first: std::ptr::null_mut(),
        mf_hash: crate::map::Map::default(),
        mf_trans: crate::map::Map::default(),
        mf_blocknr_max: 0,
        mf_blocknr_min: -1,
        mf_neg_count: 0,
        mf_infile_count: 0,
        mf_page_size: MEMFILE_PAGE_SIZE,
        mf_dirty: MfdirtyT::No,
    };

    if let Some(fname) = fname {
        // try to open the file
        if !mf_do_open(&mut mfp, fname, flags) {
            return None; // fail if file could not be opened
        }
    }
    // else: no file, use memory only - mfp already has the right
    // mf_fname/mf_ffname/mf_fd defaults (None/None/None).

    // Try to set the page size equal to device's block size: deferred
    // (see the module doc comment) - mf_page_size keeps its
    // MEMFILE_PAGE_SIZE default from construction above.

    // When recovering, the actual block size will be retrieved from
    // block 0 in ml_recover(). The size used here may be wrong,
    // therefore mf_blocknr_max must be rounded up.
    let flags_trunc_or_excl = flags & (libc::O_TRUNC | libc::O_EXCL) != 0;
    let size: Option<u64> = if mfp.mf_fd.is_none() || flags_trunc_or_excl {
        None // matches the original's `||`-chain short-circuiting
             // before ever calling vim_lseek() in these cases.
    } else {
        mfp.mf_fd.as_mut().and_then(|f| f.seek(SeekFrom::End(0)).ok())
    };
    let page_size = u64::from(mfp.mf_page_size);
    mfp.mf_blocknr_max = match size {
        Some(sz) if sz > 0 => {
            // no file or empty file otherwise falls through to 0 below.
            sz.div_ceil(page_size) as BlocknrT
        }
        _ => 0,
    };
    mfp.mf_blocknr_min = -1;
    mfp.mf_neg_count = 0;
    mfp.mf_infile_count = mfp.mf_blocknr_max;

    Some(mfp)
}

/// Open a file for an existing memfile (`mf_open_file`).
///
/// Used when `updatecount` set from 0 to some value.
///
/// `fname`: name of file to use - consumed either way, see [`mf_open`]'s
/// doc comment.
///
/// @return `OK` on success, `FAIL` if the file could not be opened.
#[must_use]
pub fn mf_open_file(mfp: &mut MemfileT, fname: Vec<u8>) -> i32 {
    if mf_do_open(mfp, fname, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL) {
        mfp.mf_dirty = MfdirtyT::Yes;
        return OK;
    }

    FAIL
}

/// Open memfile's swapfile (`mf_do_open`).
///
/// `fname` is consumed either way (stored into `mfp.mf_fname`, or
/// freed via [`mf_free_fnames`] on failure), matching the original's
/// "fname is consumed (also when error)" contract.
///
/// @return whether the `open` succeeded.
fn mf_do_open(mfp: &mut MemfileT, fname: Vec<u8>, flags: i32) -> bool {
    // fname cannot be NameBuff, because it must have been allocated -
    // N/A in this translation (a `Vec<u8>` is always independently
    // owned).
    mf_set_fnames(mfp, fname);
    debug_assert!(mfp.mf_fname.is_some());

    // Extra security check: When creating a swap file it really
    // shouldn't exist yet. If there is a symbolic link, this is most
    // likely an attack. `os_fileinfo_link`'s actual contract is just
    // "does lstat succeed" (a boolean); `std::fs::symlink_metadata`
    // gives the same answer without needing the full deferred
    // `FileInfo` struct.
    let fname_str = mfp
        .mf_fname
        .as_ref()
        .and_then(|f| std::str::from_utf8(f).ok());
    let link_exists = fname_str.is_some_and(|s| std::fs::symlink_metadata(s).is_ok());

    if flags & libc::O_CREAT != 0 && link_exists {
        mfp.mf_fd = None;
        // E300: Swap file already exists (symlink attack?) - message
        // display omitted (see the module doc comment's "message
        // display is a skippable side effect" policy); mf_fd staying
        // None is the real, observable effect callers depend on.
    } else {
        // try to open the file
        let flags = flags | crate::os::fs::O_NOFOLLOW;
        mfp.mf_flags = flags;
        mfp.mf_fd = fname_str.and_then(|s| {
            crate::os::fs::os_open(
                std::path::Path::new(s),
                flags,
                libc::S_IREAD | libc::S_IWRITE,
            )
        });
    }

    // If the file cannot be opened, use memory only.
    if mfp.mf_fd.is_none() {
        mf_free_fnames(mfp);
        return false;
    }

    // os_set_cloexec(mfp->mf_fd): not translated - std::fs::File
    // already opens close-on-exec (Unix) / with a non-inheritable
    // handle (Windows) by default, see os/fs.rs's module doc comment.

    true
}

/// Get existing block `nr` with `page_count` pages (`mf_get`).
///
/// Caller should first check a negative `nr` with [`mf_trans_del`].
///
/// @return null if not found.
///
/// # Safety
/// Every `*mut BhdrT` reachable via `mfp.mf_hash` must be a valid
/// pointer (allocated via `mf_alloc_bhdr`) - true for every block this
/// crate allocates via `mf_alloc_bhdr`/`mf_new`.
#[must_use]
pub unsafe fn mf_get(mfp: &mut MemfileT, nr: BlocknrT, page_count: u32) -> *mut BhdrT {
    // check block number exists
    if nr >= mfp.mf_blocknr_max || nr <= mfp.mf_blocknr_min {
        return std::ptr::null_mut();
    }

    // see if it is in the cache
    let hp = match mfp.mf_hash.get(&nr).copied() {
        None => {
            // not in the hash list
            if nr < 0 || nr >= mfp.mf_infile_count {
                // can't be in the file
                return std::ptr::null_mut();
            }

            // could check here if the block is in the free list

            if page_count == 0 {
                return std::ptr::null_mut();
            }
            let hp = mf_alloc_bhdr(mfp, page_count);

            // SAFETY: mf_alloc_bhdr always returns a valid,
            // just-allocated pointer (xmalloc-backed, aborts on OOM
            // rather than returning null).
            unsafe {
                (*hp).bh_bnum = nr;
                (*hp).bh_flags = 0;
                (*hp).bh_page_count = page_count;
            }
            // SAFETY: caller contract (see function doc); hp was just
            // allocated above with a data buffer of exactly
            // mf_page_size * page_count bytes.
            if unsafe { mf_read(mfp, hp) } == FAIL {
                // cannot read the block
                unsafe { mf_free_bhdr(hp) };
                return std::ptr::null_mut();
            }
            hp
        }
        Some(hp) => {
            // SAFETY: caller contract (see function doc).
            let bnum = unsafe { (*hp).bh_bnum };
            mfp.mf_hash.remove(&bnum);
            hp
        }
    };

    // SAFETY: caller contract (see function doc).
    unsafe {
        (*hp).bh_flags |= BH_LOCKED;
        let bnum = (*hp).bh_bnum;
        mfp.mf_hash.insert(bnum, hp);
    }

    hp
}

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

/// Read a block from disk (`mf_read`).
///
/// @return `OK` on success, `FAIL` on failure (no file, seek error, or
///         short read - see the module doc comment for why the
///         original's `PERROR()` message display is omitted here).
///
/// # Safety
/// `hp` must be a valid `BhdrT` pointer whose `bh_data` is a
/// [`BhData::Data`] buffer of exactly `mfp.mf_page_size * bh_page_count`
/// bytes (true for every block this crate allocates via
/// [`mf_alloc_bhdr`]/[`mf_new`]).
pub unsafe fn mf_read(mfp: &mut MemfileT, hp: *mut BhdrT) -> i32 {
    let Some(file) = mfp.mf_fd.as_mut() else {
        return FAIL; // there is no file, can't read
    };

    let page_size = mfp.mf_page_size;
    // SAFETY: caller contract (see function doc).
    let bh_bnum = unsafe { (*hp).bh_bnum };
    let offset = (page_size as u64) * (bh_bnum as u64);
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return FAIL; // Seek error in swap file read
    }

    // SAFETY: caller contract (see function doc).
    let buf = unsafe { (*hp).bh_data.as_data_mut() };
    if file.read_exact(buf).is_err() {
        return FAIL; // Read error in swap file
    }

    OK
}

/// Write a block to disk (`mf_write`).
///
/// We don't want gaps in the file. Write the blocks in front of `hp`
/// to extend the file. If block `mfp.mf_infile_count` is not in the
/// hash list, it has been freed - fill the space in the file with
/// (duplicated) data from the current block, exactly like the
/// original (not zeros - simpler/more portable than a sparse-file
/// hole).
///
/// Two things are intentionally simplified from the original, both
/// documented in the module doc comment: the `PERROR()`/`emsg()`
/// message displays on each failure path are omitted (control flow is
/// unaffected either way; `did_swapwrite_msg` bookkeeping is still
/// updated faithfully), and the retry-with-reopen-on-failure fallback
/// (recovering from e.g. a disconnected network drive) is not
/// implemented, collapsing the original's two-attempt loop to one.
///
/// @return `OK` on success, `FAIL` on failure (no file, couldn't
///         translate a negative block number, seek error, or write
///         error).
///
/// # Safety
/// `hp` must be a valid `BhdrT` pointer, as must every block header
/// reachable via `mfp.mf_hash`.
pub unsafe fn mf_write(mfp: &mut MemfileT, hp: *mut BhdrT) -> i32 {
    if mfp.mf_fd.is_none() && !mfp.mf_reopen {
        // there is no file and there was no file, can't write
        return FAIL;
    }

    // SAFETY: caller contract (see function doc).
    if unsafe { (*hp).bh_bnum } < 0 {
        // must assign file block number
        if unsafe { mf_trans_add(mfp, hp) } == FAIL {
            return FAIL;
        }
    }

    let page_size = mfp.mf_page_size;

    loop {
        // SAFETY: caller contract (see function doc).
        let hp_bnum = unsafe { (*hp).bh_bnum };
        let mut nr = hp_bnum;
        // `None` means "freed block, fill with dummy data"; `Some(hp)`
        // is the common case (writing the block we were asked to write).
        let hp2: Option<*mut BhdrT> = if nr > mfp.mf_infile_count {
            // beyond end of file
            nr = mfp.mf_infile_count;
            mfp.mf_hash.get(&nr).copied() // None caught below
        } else {
            Some(hp)
        };

        let offset = (page_size as u64) * (nr as u64);
        // page_count/data source: 1 page of hp's own data when filling a
        // gap (hp2 == None, matching the original: it still reads from
        // hp->bh_data, just capped to a single page), else hp2's real
        // page count/data.
        let page_count = match hp2 {
            // SAFETY: hp2 is valid per this function's own safety doc.
            Some(p) => unsafe { (*p).bh_page_count },
            None => 1,
        };
        let size = (page_size * page_count) as usize;
        let data_ptr = hp2.unwrap_or(hp);
        // SAFETY: data_ptr (hp2 or hp) is valid per this function's own
        // safety doc; this borrow is of *data_ptr, unrelated to mfp, so
        // it coexists fine with the `mfp.mf_fd.as_mut()` borrow below.
        let full_data: &[u8] = unsafe { (*data_ptr).bh_data.as_data() };
        let data = &full_data[..size];

        let write_ok = match mfp.mf_fd.as_mut() {
            Some(file) => {
                file.seek(SeekFrom::Start(offset)).is_ok() && file.write_all(data).is_ok()
            }
            None => false,
        };

        if !write_ok {
            // Avoid repeating the error message, this mostly happens when
            // the disk is full. We give the message again only after a
            // successful write or when hitting a key (message display
            // itself omitted here - see this function's own doc comment).
            unsafe { crate::globals::GLOBALS.get_mut() }.did_swapwrite_msg = true;
            return FAIL;
        }

        unsafe { crate::globals::GLOBALS.get_mut() }.did_swapwrite_msg = false;
        if let Some(hp2) = hp2 {
            // written a non-dummy block
            unsafe { (*hp2).bh_flags &= !BH_DIRTY };
        }
        if nr + BlocknrT::from(page_count) > mfp.mf_infile_count {
            // appended to file
            mfp.mf_infile_count = nr + BlocknrT::from(page_count);
        }
        if nr == hp_bnum {
            // written the desired block
            break;
        }
    }
    OK
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

/// Sync changed parts of a memfile to disk (`mf_sync`).
///
/// Sync from last to first (may reduce the probability of an
/// inconsistent file). If a write fails, it is very likely caused by a
/// full filesystem. Then we only try to write blocks within the
/// existing file. If that also fails then we give up.
///
/// @param flags  See [`mfs_flag`].
///
/// @return `OK` on success, `FAIL` on failure.
///
/// # Safety
/// Every `*mut BhdrT` reachable via `mfp.mf_hash` must be a valid
/// pointer (allocated via `mf_alloc_bhdr`) - true for every block this
/// crate allocates via `mf_alloc_bhdr`/`mf_new`.
pub unsafe fn mf_sync(mfp: &mut MemfileT, flags: i32) -> i32 {
    if mfp.mf_fd.is_none() {
        // there is no file, nothing to do
        mfp.mf_dirty = MfdirtyT::No;
        return FAIL;
    }

    // Only a CTRL-C while writing will break us here, not one typed
    // previously - see the module doc comment for why the original's
    // os_char_avail()/os_breakcheck() interruptibility check (and the
    // `if got_int { break; }` that depends on it) is correctly omitted
    // rather than silently changed, given got_int is cleared here and
    // this simplified version never sets it back to true mid-sync.
    let got_int_save = unsafe { crate::globals::GLOBALS.get_mut() }.got_int;
    unsafe { crate::globals::GLOBALS.get_mut() }.got_int = false;

    let mut status = OK;
    // note, "last" block is typically earlier in the hash list in the
    // original; this crate's Map iteration order isn't guaranteed to
    // match, but mf_sync's correctness doesn't depend on a specific
    // order, only that every qualifying dirty block eventually gets
    // written.
    let bhdrs: Vec<*mut BhdrT> = mfp.mf_hash.iter().map(|(_, v)| *v).collect();
    let hash_was_empty = bhdrs.is_empty();
    'sync: for hp in bhdrs {
        // SAFETY: caller contract (see function doc).
        let (bh_bnum, bh_flags) = unsafe { ((*hp).bh_bnum, (*hp).bh_flags) };
        if (flags & mfs_flag::ALL != 0 || bh_bnum >= 0)
            && (bh_flags & BH_DIRTY != 0)
            && (status == OK || (bh_bnum >= 0 && bh_bnum < mfp.mf_infile_count))
        {
            if flags & mfs_flag::ZERO != 0 && bh_bnum != 0 {
                continue;
            }
            if unsafe { mf_write(mfp, hp) } == FAIL {
                if status == FAIL {
                    // double error: quit syncing
                    break 'sync;
                }
                status = FAIL;
            }
        }
    }

    // If the whole list is flushed, the memfile is not dirty anymore.
    // In case of an error, dirty flag is also set, to avoid trying all
    // the time.
    if hash_was_empty || status == FAIL {
        mfp.mf_dirty = MfdirtyT::No;
    }

    if flags & mfs_flag::FLUSH != 0 {
        // SAFETY: caller contract (see function doc).
        if mfp
            .mf_fd
            .as_ref()
            .is_some_and(|file| crate::os::fs::os_fsync(file) != 0)
        {
            status = FAIL;
        }
    }

    unsafe { crate::globals::GLOBALS.get_mut() }.got_int |= got_int_save;

    status
}

/// Set dirty flag for all blocks in a memory file with a positive
/// block number. These are blocks that need to be written to a newly
/// created swapfile (`mf_set_dirty`).
///
/// # Safety
/// Every `*mut BhdrT` reachable via `mfp.mf_hash` must be a valid
/// pointer (allocated via `mf_alloc_bhdr`) - true for every block this
/// crate allocates via `mf_alloc_bhdr`/`mf_new`.
pub unsafe fn mf_set_dirty(mfp: &mut MemfileT) {
    for (_, &hp) in mfp.mf_hash.iter() {
        // SAFETY: caller contract (see function doc).
        unsafe {
            if (*hp).bh_bnum > 0 {
                (*hp).bh_flags |= BH_DIRTY;
            }
        }
    }
    mfp.mf_dirty = MfdirtyT::Yes;
}

/// Close a memory file and optionally delete the associated file
/// (`mf_close`).
///
/// @param del_file  Whether to delete the associated file.
///
/// # Safety
/// Every `*mut BhdrT` reachable via `mfp.mf_hash`/`mfp.mf_free_first`
/// must be a valid, uniquely-owned pointer (allocated via
/// `mf_alloc_bhdr`, not aliased elsewhere) - true for every block this
/// crate allocates via `mf_alloc_bhdr`/`mf_new`.
pub unsafe fn mf_close(mut mfp: MemfileT, del_file: bool) {
    // Closing the file (if any) is just dropping it - no separate
    // close() call/error check needed (see the module doc comment for
    // why the original's emsg() on a close error is omitted).
    mfp.mf_fd = None;

    if del_file {
        if let Some(fname) = &mfp.mf_fname {
            if let Ok(fname_str) = std::str::from_utf8(fname) {
                crate::os::fs::os_remove(std::path::Path::new(fname_str));
            }
        }
    }

    // free entries in used list (`map_foreach_value`) - no need to
    // remove them one by one, `mfp.mf_hash` itself is dropped when this
    // function returns (matching the original's `map_destroy` right
    // after this same loop).
    let bhdrs: Vec<*mut BhdrT> = mfp.mf_hash.iter().map(|(_, v)| *v).collect();
    for hp in bhdrs {
        unsafe { mf_free_bhdr(hp) };
    }

    // free entries in free list
    while !mfp.mf_free_first.is_null() {
        let hp = unsafe { mf_rem_free(&mut mfp) };
        unsafe { mf_free_bhdr(hp) };
    }

    mf_free_fnames(&mut mfp);
    // `mfp` itself is dropped here at the end of scope, matching the
    // original's `xfree(mfp)`.
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
            mf_fd: None,
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

    /// A unique temp file path under the system temp dir, removed on
    /// drop (RAII) - same pattern as `crate::os::fs`'s own test helper.
    struct TempFilePath {
        path: std::path::PathBuf,
    }

    impl TempFilePath {
        fn new(name: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut path = std::env::temp_dir();
            path.push(format!(
                "nero_memfile_test_{name}_{}_{unique}",
                std::process::id()
            ));
            TempFilePath { path }
        }
    }

    impl Drop for TempFilePath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn open_rw_truncate(path: &std::path::Path) -> std::fs::File {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap()
    }

    #[test]
    fn mf_read_fails_when_no_file() {
        let mut mfp = test_mfp();
        unsafe {
            let hp = mf_new(&mut mfp, false, 1);
            assert_eq!(mf_read(&mut mfp, hp), FAIL);
            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_write_fails_when_no_file_and_not_reopening() {
        let mut mfp = test_mfp();
        unsafe {
            let hp = mf_new(&mut mfp, false, 1);
            assert_eq!(mf_write(&mut mfp, hp), FAIL);
            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_write_then_mf_read_roundtrip() {
        let tmp = TempFilePath::new("rw_roundtrip");
        let mut mfp = MemfileT {
            mf_page_size: 16,
            mf_fd: Some(open_rw_truncate(&tmp.path)),
            ..default_memfile()
        };

        unsafe {
            let hp = mf_new(&mut mfp, false, 1); // bnum 0
            assert_eq!((*hp).bh_bnum, 0);
            (*hp)
                .bh_data
                .as_data_mut()
                .copy_from_slice(b"0123456789ABCDEF");

            assert_eq!(mf_write(&mut mfp, hp), OK);
            assert_eq!(mfp.mf_infile_count, 1); // one page written
            assert_eq!((*hp).bh_flags & BH_DIRTY, 0); // cleared by mf_write

            // Corrupt the in-memory buffer, then read it back from disk
            // to verify mf_write really persisted the right bytes at the
            // right offset.
            (*hp).bh_data.as_data_mut().fill(0);
            assert_eq!(mf_read(&mut mfp, hp), OK);
            assert_eq!((*hp).bh_data.as_data(), b"0123456789ABCDEF");

            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_read_fails_past_end_of_file() {
        let tmp = TempFilePath::new("read_past_eof");
        let mut mfp = MemfileT {
            mf_page_size: 16,
            mf_fd: Some(open_rw_truncate(&tmp.path)), // empty file
            ..default_memfile()
        };

        unsafe {
            let hp = mf_new(&mut mfp, false, 1);
            assert_eq!(mf_read(&mut mfp, hp), FAIL); // nothing written yet
            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_write_fills_gaps_with_dummy_data_for_freed_blocks() {
        let tmp = TempFilePath::new("gap_fill");
        let mut mfp = MemfileT {
            mf_page_size: 8,
            mf_fd: Some(open_rw_truncate(&tmp.path)),
            ..default_memfile()
        };
        // Simulate blocks 0,1,2 having existed in memory only and
        // already been freed (so mf_hash has no entries for them),
        // while nothing has ever actually been written to disk yet.
        mfp.mf_blocknr_max = 3;

        unsafe {
            let hp = mf_new(&mut mfp, false, 1);
            assert_eq!((*hp).bh_bnum, 3);
            (*hp).bh_data.as_data_mut().copy_from_slice(b"REALDATA");

            assert_eq!(mf_write(&mut mfp, hp), OK);
            // Caught up to block 3 (0-indexed) + 1 page = 4 pages total.
            assert_eq!(mfp.mf_infile_count, 4);

            let file_len = std::fs::metadata(&tmp.path).unwrap().len();
            assert_eq!(file_len, 8 * 4);

            // The real block (bnum 3) must read back correctly.
            assert_eq!(mf_read(&mut mfp, hp), OK);
            assert_eq!((*hp).bh_data.as_data(), b"REALDATA");

            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_write_sets_did_swapwrite_msg_on_failure_and_clears_on_success() {
        // GLOBALS.did_swapwrite_msg is shared process-wide state; guard
        // it like every other GLOBALS-touching test in this crate.
        //
        // Use a *read-only*-opened file so the actual write attempt
        // inside mf_write's loop fails (as opposed to the early "no
        // file at all" check up front, which returns FAIL without ever
        // touching did_swapwrite_msg - verified against the original's
        // own source: that check precedes the loop entirely).
        let tmp = TempFilePath::new("swapwrite_msg");
        std::fs::write(&tmp.path, []).unwrap(); // create an empty file first
        let mut mfp = MemfileT {
            mf_page_size: 8,
            mf_fd: Some(std::fs::File::open(&tmp.path).unwrap()), // read-only
            ..default_memfile()
        };
        unsafe {
            let hp = mf_new(&mut mfp, false, 1);
            let previous = crate::globals::GLOBALS.get_mut().did_swapwrite_msg;

            assert_eq!(mf_write(&mut mfp, hp), FAIL);
            assert!(crate::globals::GLOBALS.get_mut().did_swapwrite_msg);

            // Re-open read-write: the next successful write must clear
            // the flag back to false.
            mfp.mf_fd = Some(open_rw_truncate(&tmp.path));
            assert_eq!(mf_write(&mut mfp, hp), OK);
            assert!(!crate::globals::GLOBALS.get_mut().did_swapwrite_msg);

            crate::globals::GLOBALS.get_mut().did_swapwrite_msg = previous;
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
    fn mf_close_deletes_file_when_requested() {
        let tmp = TempFilePath::new("close_delete");
        std::fs::write(&tmp.path, b"anything").unwrap();
        let mut mfp = MemfileT {
            mf_page_size: 8,
            mf_fd: Some(std::fs::File::open(&tmp.path).unwrap()),
            mf_fname: Some(tmp.path.to_string_lossy().into_owned().into_bytes()),
            ..default_memfile()
        };
        // A used-list block and a free-list block, to exercise both
        // cleanup loops without panicking or leaking (verified only by
        // the test completing normally - no memory sanitizer available
        // here, but this at least exercises every code path).
        unsafe {
            let used = mf_new(&mut mfp, false, 1);
            let freed = mf_new(&mut mfp, false, 1);
            mf_free(&mut mfp, freed); // moves `freed` onto the free list
            assert!(!mfp.mf_free_first.is_null());
            let _ = used; // still registered in mf_hash

            mf_close(mfp, true);
        }

        assert!(!tmp.path.exists(), "mf_close(del_file=true) should remove the file");
    }

    #[test]
    fn mf_close_keeps_file_when_not_requested() {
        let tmp = TempFilePath::new("close_keep");
        std::fs::write(&tmp.path, b"anything").unwrap();
        let mfp = MemfileT {
            mf_page_size: 8,
            mf_fd: Some(std::fs::File::open(&tmp.path).unwrap()),
            mf_fname: Some(tmp.path.to_string_lossy().into_owned().into_bytes()),
            ..default_memfile()
        };
        unsafe {
            mf_close(mfp, false);
        }
        assert!(tmp.path.exists(), "mf_close(del_file=false) should keep the file");
    }

    #[test]
    fn mf_sync_fails_and_clears_dirty_when_no_file() {
        let mut mfp = test_mfp();
        mfp.mf_dirty = MfdirtyT::Yes;
        assert_eq!(unsafe { mf_sync(&mut mfp, 0) }, FAIL);
        assert_eq!(mfp.mf_dirty, MfdirtyT::No);
    }

    #[test]
    fn mf_sync_writes_only_positive_bnum_blocks_without_mfs_all() {
        let tmp = TempFilePath::new("sync_positive_only");
        let mut mfp = MemfileT {
            mf_page_size: 8,
            mf_fd: Some(open_rw_truncate(&tmp.path)),
            ..default_memfile()
        };
        unsafe {
            let pos = mf_new(&mut mfp, false, 1); // bnum 0, BH_LOCKED|BH_DIRTY
            let neg = mf_new(&mut mfp, true, 1); // negative bnum, BH_LOCKED|BH_DIRTY
            // mf_put unlocks (mf_sync doesn't care about BH_LOCKED, but
            // this matches realistic usage where blocks are unlocked
            // before being considered for sync).
            mf_put(&mut mfp, pos, true, false);
            mf_put(&mut mfp, neg, true, false);

            assert_eq!(mf_sync(&mut mfp, 0), OK); // no MFS_ALL
            assert_eq!((*pos).bh_flags & BH_DIRTY, 0, "positive block should be synced");
            assert_eq!(
                (*neg).bh_flags & BH_DIRTY,
                BH_DIRTY,
                "negative block should NOT be synced without MFS_ALL"
            );
            assert_eq!(mfp.mf_infile_count, 1); // only the positive block written

            mf_free(&mut mfp, pos);
            mf_free(&mut mfp, neg);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_sync_with_mfs_all_writes_negative_bnum_blocks_too() {
        let tmp = TempFilePath::new("sync_mfs_all");
        let mut mfp = MemfileT {
            mf_page_size: 8,
            mf_fd: Some(open_rw_truncate(&tmp.path)),
            ..default_memfile()
        };
        unsafe {
            let neg = mf_new(&mut mfp, true, 1);
            mf_put(&mut mfp, neg, true, false);
            assert!((*neg).bh_bnum < 0);

            assert_eq!(mf_sync(&mut mfp, mfs_flag::ALL), OK);

            // MFS_ALL should have caused mf_write -> mf_trans_add to
            // assign it a real (non-negative) file position.
            assert!((*neg).bh_bnum >= 0);
            assert_eq!((*neg).bh_flags & BH_DIRTY, 0);

            mf_free(&mut mfp, neg);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_sync_with_mfs_zero_only_writes_block_zero() {
        let tmp = TempFilePath::new("sync_mfs_zero");
        let mut mfp = MemfileT {
            mf_page_size: 8,
            mf_fd: Some(open_rw_truncate(&tmp.path)),
            ..default_memfile()
        };
        unsafe {
            let b0 = mf_new(&mut mfp, false, 1); // bnum 0
            let b1 = mf_new(&mut mfp, false, 1); // bnum 1
            mf_put(&mut mfp, b0, true, false);
            mf_put(&mut mfp, b1, true, false);

            assert_eq!(mf_sync(&mut mfp, mfs_flag::ZERO), OK);
            assert_eq!((*b0).bh_flags & BH_DIRTY, 0, "block 0 should be synced");
            assert_eq!(
                (*b1).bh_flags & BH_DIRTY,
                BH_DIRTY,
                "block 1 should NOT be synced under MFS_ZERO"
            );

            mf_free(&mut mfp, b0);
            mf_free(&mut mfp, b1);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_sync_leaves_dirty_flag_unchanged_when_hash_nonempty_and_ok() {
        // Verified against the real `map_foreach_value` macro expansion
        // (src/nvim/map_defs.h): it assigns `hp` to each hash entry's
        // value in turn, so after the loop `hp` holds the *last*
        // iterated entry (not NULL) whenever the hash is non-empty -
        // regardless of whether every write actually succeeded. So
        // `if (hp == NULL || status == FAIL) mfp->mf_dirty = MF_DIRTY_NO;`
        // really means "clear dirty only if the hash was empty, or on
        // failure" - NOT "clear dirty when everything synced OK". A
        // successful sync of a non-empty memfile leaves mf_dirty
        // unchanged, however counterintuitive that looks next to this
        // function's own "the memfile is not dirty anymore" comment.
        let tmp = TempFilePath::new("sync_leaves_dirty");
        let mut mfp = MemfileT {
            mf_page_size: 8,
            mf_fd: Some(open_rw_truncate(&tmp.path)),
            mf_dirty: MfdirtyT::Yes,
            ..default_memfile()
        };
        unsafe {
            let hp = mf_new(&mut mfp, false, 1);
            mf_put(&mut mfp, hp, true, false);

            assert_eq!(mf_sync(&mut mfp, mfs_flag::ALL), OK);
            assert_eq!((*hp).bh_flags & BH_DIRTY, 0); // the block itself IS clean now
            assert_eq!(mfp.mf_dirty, MfdirtyT::Yes); // but mf_dirty is untouched

            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_sync_clears_dirty_flag_when_hash_is_empty() {
        let tmp = TempFilePath::new("sync_empty_hash");
        let mut mfp = MemfileT {
            mf_page_size: 8,
            mf_fd: Some(open_rw_truncate(&tmp.path)),
            mf_dirty: MfdirtyT::Yes,
            ..default_memfile()
        };
        // No blocks at all: matches the original's `hp == NULL` case.
        unsafe {
            assert_eq!(mf_sync(&mut mfp, mfs_flag::ALL), OK);
        }
        assert_eq!(mfp.mf_dirty, MfdirtyT::No);
    }

    #[test]
    fn mf_sync_with_mfs_flush_calls_fsync_successfully() {
        let tmp = TempFilePath::new("sync_flush");
        let mut mfp = MemfileT {
            mf_page_size: 8,
            mf_fd: Some(open_rw_truncate(&tmp.path)),
            ..default_memfile()
        };
        unsafe {
            let hp = mf_new(&mut mfp, false, 1);
            mf_put(&mut mfp, hp, true, false);

            assert_eq!(mf_sync(&mut mfp, mfs_flag::ALL | mfs_flag::FLUSH), OK);

            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_sync_preserves_pre_existing_got_int() {
        let mut mfp = test_mfp(); // no file -> early FAIL return
        unsafe {
            let globals = crate::globals::GLOBALS.get_mut();
            let previous = globals.got_int;
            globals.got_int = true;

            assert_eq!(mf_sync(&mut mfp, 0), FAIL);

            // Early-return path (no file) doesn't even reach the
            // got_int save/restore dance, so got_int must be completely
            // untouched here.
            assert!(crate::globals::GLOBALS.get_mut().got_int);
            crate::globals::GLOBALS.get_mut().got_int = previous;
        }
    }

    #[test]
    fn mf_set_dirty_marks_only_positive_bnum_blocks() {
        // mf_set_dirty's condition is `bh_bnum > 0` (strictly positive),
        // so block 0 itself is excluded - allocate one extra block to
        // get a bnum of 1, matching the original's exact condition.
        let mut mfp = test_mfp();
        unsafe {
            let zero = mf_new(&mut mfp, false, 1); // bnum 0
            let one = mf_new(&mut mfp, false, 1); // bnum 1
            let neg = mf_new(&mut mfp, true, 1); // negative bnum
            // Clear the BH_DIRTY that mf_new sets by default, to
            // isolate mf_set_dirty's own effect.
            (*zero).bh_flags &= !BH_DIRTY;
            (*one).bh_flags &= !BH_DIRTY;
            (*neg).bh_flags &= !BH_DIRTY;
            mfp.mf_dirty = MfdirtyT::No;

            mf_set_dirty(&mut mfp);

            assert_eq!((*zero).bh_flags & BH_DIRTY, 0, "bnum 0 is not > 0, must stay untouched");
            assert_eq!((*one).bh_flags & BH_DIRTY, BH_DIRTY);
            assert_eq!((*neg).bh_flags & BH_DIRTY, 0, "negative bnum block must stay untouched");
            assert_eq!(mfp.mf_dirty, MfdirtyT::Yes);

            mf_free(&mut mfp, zero);
            mf_free(&mut mfp, one);
            mf_free(&mut mfp, neg);
            drain_free_list(&mut mfp);
        }
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

    fn path_bytes(path: &std::path::Path) -> Vec<u8> {
        path.to_string_lossy().into_owned().into_bytes()
    }

    #[test]
    fn mf_open_with_no_fname_is_memory_only() {
        let mfp = mf_open(None, 0).expect("memory-only open never fails");
        assert!(mfp.mf_fname.is_none());
        assert!(mfp.mf_ffname.is_none());
        assert!(mfp.mf_fd.is_none());
        assert_eq!(mfp.mf_blocknr_max, 0);
        assert_eq!(mfp.mf_blocknr_min, -1);
        assert_eq!(mfp.mf_neg_count, 0);
        assert_eq!(mfp.mf_infile_count, 0);
        assert_eq!(mfp.mf_page_size, MEMFILE_PAGE_SIZE);
    }

    #[test]
    fn mf_open_rdonly_existing_file_computes_blocknr_max() {
        let tmp = TempFilePath::new("open_rdonly");
        // Exactly 2 pages (using the default MEMFILE_PAGE_SIZE, since
        // mf_open always starts from that default - see the module
        // doc comment on the deferred block-size-from-device step).
        std::fs::write(&tmp.path, vec![0u8; (MEMFILE_PAGE_SIZE * 2) as usize]).unwrap();

        let mfp =
            mf_open(Some(path_bytes(&tmp.path)), libc::O_RDONLY).expect("existing file should open");
        assert!(mfp.mf_fd.is_some());
        assert_eq!(mfp.mf_page_size, MEMFILE_PAGE_SIZE);
        assert_eq!(mfp.mf_blocknr_max, 2);
        assert_eq!(mfp.mf_infile_count, 2);
        assert_eq!(mfp.mf_blocknr_min, -1);
        assert_eq!(mfp.mf_neg_count, 0);
    }

    #[test]
    fn mf_open_rdonly_missing_file_fails() {
        let tmp = TempFilePath::new("open_rdonly_missing");
        assert!(mf_open(Some(path_bytes(&tmp.path)), libc::O_RDONLY).is_none());
    }

    #[test]
    fn mf_open_empty_file_has_zero_blocknr_max() {
        let tmp = TempFilePath::new("open_empty");
        std::fs::write(&tmp.path, b"").unwrap();
        let mfp = mf_open(Some(path_bytes(&tmp.path)), libc::O_RDONLY).unwrap();
        assert_eq!(mfp.mf_blocknr_max, 0);
        assert_eq!(mfp.mf_infile_count, 0);
    }

    #[test]
    fn mf_open_file_creates_a_new_file_and_marks_dirty() {
        let tmp = TempFilePath::new("open_file_new");
        let mut mfp = default_memfile();
        assert_eq!(mf_open_file(&mut mfp, path_bytes(&tmp.path)), OK);
        assert!(mfp.mf_fd.is_some());
        assert_eq!(mfp.mf_dirty, MfdirtyT::Yes);
        assert!(tmp.path.exists());
    }

    #[test]
    fn mf_open_file_fails_if_file_already_exists() {
        let tmp = TempFilePath::new("open_file_exists");
        std::fs::write(&tmp.path, b"pre-existing").unwrap();
        let mut mfp = default_memfile();

        assert_eq!(mf_open_file(&mut mfp, path_bytes(&tmp.path)), FAIL);
        assert!(mfp.mf_fd.is_none());
        // freed via mf_free_fnames on the failure path.
        assert!(mfp.mf_fname.is_none());
        assert!(mfp.mf_ffname.is_none());
        // The pre-existing file must be untouched.
        assert_eq!(std::fs::read(&tmp.path).unwrap(), b"pre-existing");
    }

    #[test]
    fn mf_get_returns_null_for_out_of_range_block_number() {
        let mut mfp = test_mfp();
        mfp.mf_blocknr_max = 5;
        mfp.mf_blocknr_min = -5;
        unsafe {
            assert!(mf_get(&mut mfp, 5, 1).is_null()); // == max
            assert!(mf_get(&mut mfp, -5, 1).is_null()); // == min
            assert!(mf_get(&mut mfp, 10, 1).is_null()); // > max
        }
    }

    #[test]
    fn mf_get_returns_null_for_uncached_block_beyond_infile_count() {
        let mut mfp = test_mfp();
        mfp.mf_blocknr_max = 100;
        mfp.mf_infile_count = 2; // only blocks < 2 are "in the file"
        unsafe {
            assert!(mf_get(&mut mfp, 5, 1).is_null());
        }
    }

    #[test]
    fn mf_get_returns_cached_block_and_keeps_it_in_the_hash() {
        let mut mfp = test_mfp();
        unsafe {
            let hp = mf_new(&mut mfp, false, 1); // bnum 0, already in mf_hash
            assert!(mfp.mf_hash.get(&0).is_some());

            let got = mf_get(&mut mfp, 0, 1);
            assert_eq!(got, hp);
            assert_ne!((*got).bh_flags & BH_LOCKED, 0);
            // Removed then reinserted - still present afterward.
            assert!(mfp.mf_hash.get(&0).is_some());

            mf_free(&mut mfp, hp);
            drain_free_list(&mut mfp);
        }
    }

    #[test]
    fn mf_get_reads_an_uncached_in_file_block_from_disk() {
        let tmp = TempFilePath::new("get_from_disk");
        let mut mfp = MemfileT {
            mf_page_size: 16,
            mf_fd: Some(open_rw_truncate(&tmp.path)),
            ..default_memfile()
        };

        unsafe {
            let hp = mf_new(&mut mfp, false, 1); // bnum 0
            (*hp)
                .bh_data
                .as_data_mut()
                .copy_from_slice(b"0123456789ABCDEF");
            assert_eq!(mf_write(&mut mfp, hp), OK);
            assert_eq!(mfp.mf_infile_count, 1);

            // Simulate the block header having been evicted from the
            // in-memory cache (e.g. by the not-yet-translated
            // mf_release_all) while its data remains on disk -
            // mf_get's cache-miss path must then re-read it.
            mfp.mf_hash.remove(&0);
            mf_free_bhdr(hp);

            let got = mf_get(&mut mfp, 0, 1);
            assert!(!got.is_null());
            assert_eq!((*got).bh_data.as_data(), b"0123456789ABCDEF");
            assert_ne!((*got).bh_flags & BH_LOCKED, 0);

            mf_free(&mut mfp, got);
            drain_free_list(&mut mfp);
        }
    }

    #[cfg(unix)]
    #[test]
    fn mf_do_open_refuses_a_preexisting_symlink_when_creating() {
        let tmp = TempFilePath::new("do_open_symlink_unix");
        let target = tmp.path.with_extension("target");
        std::fs::write(&target, b"attacker-controlled").unwrap();
        std::os::unix::fs::symlink(&target, &tmp.path).unwrap();

        let mut mfp = default_memfile();
        // O_CREAT is set (via mf_open_file), so the symlink-attack
        // pre-check (os_fileinfo_link-equivalent) must refuse to
        // proceed, matching the original's E300 path.
        assert_eq!(mf_open_file(&mut mfp, path_bytes(&tmp.path)), FAIL);
        assert!(mfp.mf_fd.is_none());

        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(&tmp.path);
    }
}
