//! Translated from `src/nvim/memline.c` (partial: just enough of the
//! "open a brand new, empty memline" happy path in `ml_open` to make
//! it real, plus the small, self-contained internal helpers it needs).
//!
//! Translated: `ml_open` (the whole function is translated, but two of
//! its own sub-paths are narrower than the original - see below),
//! `ml_new_ptr`, `ml_new_data`, `set_b0_fname` (only the
//! `buf.b_ffname.is_none()` fast path - seethe note below),
//! `add_b0_fenc`, `long_to_char`/`char_to_long` (`long_to_char` is used
//! by `ml_open`'s block-0 setup; `char_to_long` isn't exercised by
//! anything yet but is translated alongside it since it's the same
//! small, self-contained pair in the original).
//!
//! **`ZeroBlock`/`PointerBlock`/`DataBlock` are NOT translated as
//! `#[repr(C)]` structs reinterpreting a raw byte buffer** (the
//! original's own approach, e.g. `ZeroBlock *b0p = hp->bh_data;`) -
//! that would be unsound in Rust applied to a `Vec<u8>` (alignment/
//! provenance rules), and would also inherit the original's own
//! genuine, deliberate cross-platform quirk: `ZeroBlock.b0_magic_long`
//! is a native C `long` (4 bytes under MSVC, 8 bytes under 64-bit
//! Unix), and `DataBlock.db_line_count` similarly a native `long` -
//! meaning the *exact* on-disk byte layout already isn't identical
//! across every real neovim build, even upstream (`b0_magic_long`
//! exists specifically so a *different*-layout build can detect the
//! mismatch when reading a foreign swapfile, not to guarantee a
//! perfectly uniform format). Since bit-for-bit swapfile compatibility
//! with a foreign, real neovim binary isn't a goal of this crate (only
//! internal correctness - nero can read back what nero itself wrote),
//! both fields are instead given one fixed, documented width here
//! (8 bytes/`i64`) on every platform this crate targets, sidestepping
//! the ambiguity entirely rather than replicating it. `PointerEntry`/
//! `PointerBlock`/`DataBlock`'s other fields (`blocknr_T` = `i64`,
//! `linenr_T` = `i32`, plain C `int` = `i32`) are already fixed-width
//! in the original (not `long`), so no analogous choice was needed for
//! them - their byte offsets below match the original's own natural
//! struct layout (computed by hand, then cross-checked against
//! `PB_COUNT_MAX`/`HEADER_SIZE`'s definitions in the real source).
//!
//! Deferred (each needs another not-yet-translated subsystem, or is
//! simply out of scope for the "brand new empty buffer" path this pass
//! covers):
//! - Reading/parsing an existing `ZeroBlock`/`PointerBlock`/`DataBlock`
//!   back out of a byte buffer (needed by `ml_recover`/`ml_get`/
//!   `ml_find_line`/etc.) - only the *write* direction needed by a
//!   freshly-created buffer is covered here. A real "unpack" design
//!   should be revisited once one of those functions is actually
//!   tackled, informed by what THEY specifically need (this pass
//!   deliberately doesn't guess ahead at that shape).
//! - `set_b0_fname`'s `buf.b_ffname.is_some()` branch: needs
//!   `home_replace` (path.c - tractability not yet checked),
//!   `os_fileinfo`/`os_fileinfo_inode` (the still-deferred `FileInfo`
//!   struct), and `buf_store_file_info` (`buffer.c`, not yet
//!   translated). A buffer with no file name yet (e.g. a freshly
//!   started `nvim` with no file argument, or `:enew`) - the common
//!   case `ml_open` itself is originally documented for - takes the
//!   `b0_fname[0] = NUL` fast path instead, which IS fully translated.
//! - `set_b0_dir_flag`/`swapfile_proc_running`: not called by `ml_open`
//!   itself (used by `ml_recover`/`findswapname`), out of scope here.
//! - `ml_setname`, `ml_close`/`ml_close_all`/`ml_close_notmod`,
//!   `ml_recover`, `ml_get`/`ml_get_buf`, `ml_find_line`, `ml_append`/
//!   `ml_replace`/`ml_delete` and everything else in this ~4300-line
//!   file: each is its own substantial undertaking, genuinely blocked
//!   on subsystems not yet translated (autocmd.c, the display/fold
//!   subsystem, `list_T`/eval engine, etc.) - not attempted this pass.
//!
//! `ml_open`'s two `iemsg(_("E298: ..."))` calls ("Didn't get block nr
//! N?") are translated as `debug_assert!`s rather than deferred
//! entirely (matching the policy already established for
//! `memfile.c`'s `mf_put`): given this function's own exact,
//! deterministic call sequence - a brand-new `mf_open(None, 0)`
//! followed immediately by exactly 3 `mf_new`/`ml_new_ptr`/
//! `ml_new_data` calls with no other block allocations in between -
//! the freshly assigned block numbers are always 0, 1, 2 in that
//! order; this is a genuine "should never happen" invariant for this call
//! pattern specifically, not a recoverable, reachable error condition.

use crate::buffer_defs::BufT;
use crate::ex_cmds_defs::cmod;
use crate::globals::GLOBALS;
use crate::memfile::{mf_new, mf_open, mf_put, mf_sync};
use crate::memfile_defs::{BhdrT, BlocknrT, MemfileT};
use crate::memline_defs::ML_EMPTY;
use crate::option::get_fileformat;
use crate::option_vars::OPTION_VARS;
use crate::os::env::os_get_hostname;
use crate::vim_defs::{FAIL, OK};

// Block IDs (memline.c's own anonymous `enum`).
const DATA_ID: u16 = ((b'd' as u16) << 8) + b'a' as u16;
const PTR_ID: u16 = ((b'p' as u16) << 8) + b't' as u16;
const BLOCK0_ID0: u8 = b'b';
const BLOCK0_ID1: u8 = b'0';

// ZeroBlock field sizes (`memline.c`'s own anonymous `enum`s).
const B0_UNAME_SIZE: usize = 40;
const B0_HNAME_SIZE: usize = 40;
const B0_FNAME_SIZE_ORG: usize = 900;
const B0_FNAME_SIZE_NOCRYPT: usize = 898;

// ZeroBlock byte offsets, matching the original's own natural struct
// layout (see this module's own doc comment for the b0_magic_long
// width caveat).
const ZB_OFF_ID: usize = 0; // 2 bytes
const ZB_OFF_VERSION: usize = ZB_OFF_ID + 2; // 10 bytes
const ZB_OFF_PAGE_SIZE: usize = ZB_OFF_VERSION + 10; // 4 bytes
const ZB_OFF_MTIME: usize = ZB_OFF_PAGE_SIZE + 4; // 4 bytes
const ZB_OFF_INO: usize = ZB_OFF_MTIME + 4; // 4 bytes
const ZB_OFF_PID: usize = ZB_OFF_INO + 4; // 4 bytes
const ZB_OFF_UNAME: usize = ZB_OFF_PID + 4; // B0_UNAME_SIZE bytes
const ZB_OFF_HNAME: usize = ZB_OFF_UNAME + B0_UNAME_SIZE; // B0_HNAME_SIZE bytes
const ZB_OFF_FNAME: usize = ZB_OFF_HNAME + B0_HNAME_SIZE; // B0_FNAME_SIZE_ORG bytes
const ZB_OFF_MAGIC_LONG: usize = ZB_OFF_FNAME + B0_FNAME_SIZE_ORG; // 8 bytes (see doc comment)
const ZB_OFF_MAGIC_INT: usize = ZB_OFF_MAGIC_LONG + 8; // 4 bytes
const ZB_OFF_MAGIC_SHORT: usize = ZB_OFF_MAGIC_INT + 4; // 2 bytes
const ZB_OFF_MAGIC_CHAR: usize = ZB_OFF_MAGIC_SHORT + 2; // 1 byte
/// Total size of a `ZeroBlock` as this crate lays it out - must stay
/// comfortably under `memfile::MIN_SWAP_PAGE_SIZE` (matching the
/// original's own "If size of block0 changes anyway, adjust
/// MIN_SWAP_PAGE_SIZE in memfile.h!!" warning), verified by a test.
const ZERO_BLOCK_SIZE: usize = ZB_OFF_MAGIC_CHAR + 1;

// Values for the b0_dirty/b0_flags "aliased into the tail of b0_fname"
// trick (`#define b0_dirty b0_fname[B0_FNAME_SIZE_ORG - 1]` etc.).
const B0_DIRTY: u8 = 0x55;
const B0_HAS_FENC: u8 = 8;

const B0_MAGIC_LONG: i64 = 0x3031_3233;
const B0_MAGIC_INT: i32 = 0x2021_2223;
// The original truncates this to `(int16_t)B0_MAGIC_SHORT` from the
// full `0x10111213`; only the low 16 bits (`0x1213`) actually survive
// that cast, so that's what's stored here directly.
const B0_MAGIC_SHORT: i16 = 0x1213;
const B0_MAGIC_CHAR: u8 = 0x55;

/// Block zero: portable, hand-packed byte layout for the first block
/// of a swapfile (`ZeroBlock`). See this module's own doc comment for
/// why this isn't a `#[repr(C)]` struct reinterpreting raw bytes.
///
/// Only models what `ml_open`'s "brand new buffer" path needs to
/// *write*; there is no corresponding read/parse direction yet (see
/// the module doc comment).
pub struct ZeroBlock {
    pub b0_page_size: u32,
    pub b0_mtime: i64,
    pub b0_ino: i64,
    pub b0_pid: i64,
    pub b0_uname: Vec<u8>,
    pub b0_hname: Vec<u8>,
    /// Raw `b0_fname` byte area, including its aliased trailing
    /// `b0_dirty`/`b0_flags` bytes (see [`ZeroBlock::b0_dirty`]/
    /// [`ZeroBlock::b0_flags`]) and, when present, a trailing
    /// `'fileencoding'` (see `add_b0_fenc`).
    pub b0_fname: [u8; B0_FNAME_SIZE_ORG],
}

impl ZeroBlock {
    /// A fresh block 0 with every field zeroed/empty (`b0_fname[0] =
    /// NUL`, matching the "no file name" fast path).
    #[must_use]
    pub fn new(page_size: u32) -> Self {
        ZeroBlock {
            b0_page_size: page_size,
            b0_mtime: 0,
            b0_ino: 0,
            b0_pid: 0,
            b0_uname: Vec::new(),
            b0_hname: Vec::new(),
            b0_fname: [0u8; B0_FNAME_SIZE_ORG],
        }
    }

    /// `#define b0_dirty b0_fname[B0_FNAME_SIZE_ORG - 1]`.
    #[must_use]
    pub fn b0_dirty(&self) -> u8 {
        self.b0_fname[B0_FNAME_SIZE_ORG - 1]
    }
    pub fn set_b0_dirty(&mut self, v: u8) {
        self.b0_fname[B0_FNAME_SIZE_ORG - 1] = v;
    }

    /// `#define b0_flags b0_fname[B0_FNAME_SIZE_ORG - 2]`.
    #[must_use]
    pub fn b0_flags(&self) -> u8 {
        self.b0_fname[B0_FNAME_SIZE_ORG - 2]
    }
    pub fn set_b0_flags(&mut self, v: u8) {
        self.b0_fname[B0_FNAME_SIZE_ORG - 2] = v;
    }

    /// Packs this `ZeroBlock` into `buf` (a block's raw page bytes),
    /// matching the original's direct field assignments through a
    /// `ZeroBlock *b0p` pointer cast.
    ///
    /// # Panics
    /// If `buf` is smaller than `ZERO_BLOCK_SIZE` (never true for a
    /// real block, whose size is always `mf_page_size >=
    /// MIN_SWAP_PAGE_SIZE > ZERO_BLOCK_SIZE`).
    pub fn write_into(&self, buf: &mut [u8]) {
        assert!(buf.len() >= ZERO_BLOCK_SIZE, "block buffer too small for ZeroBlock");
        buf[ZB_OFF_ID] = BLOCK0_ID0;
        buf[ZB_OFF_ID + 1] = BLOCK0_ID1;

        // "VIM " + up to 5 more bytes from the oldest-compatible
        // version string (`Versions[0]`, not itself translated - see
        // xstpcpy/xstrlcpy's exact truncation semantics reproduced
        // here) - 10 bytes total, matching b0_version's declared size.
        let version = b"VIM 8.1\0\0\0";
        buf[ZB_OFF_VERSION..ZB_OFF_VERSION + 10].copy_from_slice(version);

        buf[ZB_OFF_PAGE_SIZE..ZB_OFF_PAGE_SIZE + 4]
            .copy_from_slice(&long_to_char(i64::from(self.b0_page_size)));
        buf[ZB_OFF_MTIME..ZB_OFF_MTIME + 4].copy_from_slice(&long_to_char(self.b0_mtime));
        buf[ZB_OFF_INO..ZB_OFF_INO + 4].copy_from_slice(&long_to_char(self.b0_ino));
        buf[ZB_OFF_PID..ZB_OFF_PID + 4].copy_from_slice(&long_to_char(self.b0_pid));

        write_truncated(&mut buf[ZB_OFF_UNAME..ZB_OFF_UNAME + B0_UNAME_SIZE], &self.b0_uname);
        write_truncated(&mut buf[ZB_OFF_HNAME..ZB_OFF_HNAME + B0_HNAME_SIZE], &self.b0_hname);
        buf[ZB_OFF_FNAME..ZB_OFF_FNAME + B0_FNAME_SIZE_ORG].copy_from_slice(&self.b0_fname);

        buf[ZB_OFF_MAGIC_LONG..ZB_OFF_MAGIC_LONG + 8].copy_from_slice(&B0_MAGIC_LONG.to_ne_bytes());
        buf[ZB_OFF_MAGIC_INT..ZB_OFF_MAGIC_INT + 4].copy_from_slice(&B0_MAGIC_INT.to_ne_bytes());
        buf[ZB_OFF_MAGIC_SHORT..ZB_OFF_MAGIC_SHORT + 2]
            .copy_from_slice(&B0_MAGIC_SHORT.to_ne_bytes());
        buf[ZB_OFF_MAGIC_CHAR] = B0_MAGIC_CHAR;
    }
}

/// Writes as much of `src` as fits into `dst`, NUL-terminated,
/// matching `xstrlcpy(dst, src, dst.len())`'s truncate-and-always-
/// terminate contract (used for `b0_uname`/`b0_hname`, which are
/// fixed-size, always-NUL-terminated fields).
fn write_truncated(dst: &mut [u8], src: &[u8]) {
    let n = src.len().min(dst.len() - 1);
    dst[..n].copy_from_slice(&src[..n]);
    dst[n..].fill(0);
}

/// Move an integer into a four byte little-endian array. Used for
/// machine independence in block zero (`long_to_char`).
///
/// The original operates on a native `long` (see this module's own
/// doc comment on the `b0_magic_long`/`db_line_count` width choice);
/// every real call site in `ml_open` only ever needs 4 bytes of actual
/// range (page size, a truncated mtime/inode/pid), so this takes
/// `i64` and packs its low 4 bytes, matching the original's own
/// (already truncating, `n & 0xff`-style) byte-at-a-time logic
/// exactly - which is, byte-for-byte, the same operation as
/// `(n as u32).to_le_bytes()`.
#[must_use]
pub fn long_to_char(n: i64) -> [u8; 4] {
    (n as u32).to_le_bytes()
}

/// Inverse of [`long_to_char`] (`char_to_long`). Not yet exercised by
/// any translated caller (nothing here reads a `ZeroBlock` back yet -
/// see the module doc comment), translated alongside `long_to_char`
/// since it's the same small, self-contained pair in the original.
#[must_use]
pub fn char_to_long(s: &[u8; 4]) -> i64 {
    i64::from(u32::from_le_bytes(*s))
}

// --- PointerBlock / PointerEntry ---

/// `offsetof(PointerBlock, pb_pointer)`: `pb_id` (2) + `pb_count` (2) +
/// `pb_count_max` (2), padded to 8-byte alignment for the
/// `PointerEntry` array that follows (its first field, `pe_bnum`, is a
/// `blocknr_T`/`i64`, needing 8-byte alignment in the original's
/// native struct layout).
const PTR_HEADER_SIZE: usize = 8;
/// `sizeof(PointerEntry)`: `pe_bnum` (8) + `pe_line_count` (4) +
/// `pe_old_lnum` (4) + `pe_page_count` (4), padded to 24 for 8-byte
/// alignment (matching the original's native struct layout, given
/// `blocknr_T`/`linenr_T`/`int` are already fixed-width - see the
/// module doc comment).
const POINTER_ENTRY_SIZE: usize = 24;

/// One entry of a `PointerBlock` (`PointerEntry`).
#[derive(Debug, Clone, Copy, Default)]
pub struct PointerEntry {
    /// block number
    pub pe_bnum: BlocknrT,
    /// number of lines in this branch
    pub pe_line_count: i32,
    /// lnum for this block (for recovery)
    pub pe_old_lnum: i32,
    /// number of pages in block `pe_bnum`
    pub pe_page_count: i32,
}

/// `PB_COUNT_MAX(mfp)`: the maximum number of [`PointerEntry`] values
/// that fit in one page.
#[must_use]
pub fn pb_count_max(page_size: u32) -> u16 {
    ((page_size as usize - PTR_HEADER_SIZE) / POINTER_ENTRY_SIZE) as u16
}

/// Initializes an empty pointer block's header (`pb_id`/`pb_count`/
/// `pb_count_max`) into `buf` (a block's raw page bytes) - the part of
/// `ml_new_ptr` that doesn't depend on `bhdr_T`/`memfile_T`.
///
/// # Panics
/// If `buf` is smaller than `PTR_HEADER_SIZE` (never true for a real
/// block).
pub fn pointer_block_init_empty(buf: &mut [u8], page_size: u32) {
    assert!(buf.len() >= PTR_HEADER_SIZE, "block buffer too small for a PointerBlock header");
    buf[0..2].copy_from_slice(&PTR_ID.to_ne_bytes());
    buf[2..4].copy_from_slice(&0u16.to_ne_bytes()); // pb_count = 0
    buf[4..6].copy_from_slice(&pb_count_max(page_size).to_ne_bytes());
    // buf[6..8] left as whatever mf_alloc_bhdr's zero-fill already put
    // there (padding, never read).
}

/// Sets `pb_count` on an already-initialized pointer block.
pub fn pointer_block_set_count(buf: &mut [u8], count: u16) {
    buf[2..4].copy_from_slice(&count.to_ne_bytes());
}

/// Writes one [`PointerEntry`] at `index` into an already-initialized
/// pointer block.
///
/// # Panics
/// If `buf` is too small to hold an entry at `index` (never true for
/// `index < pb_count_max`).
pub fn pointer_block_set_entry(buf: &mut [u8], index: usize, entry: PointerEntry) {
    let off = PTR_HEADER_SIZE + index * POINTER_ENTRY_SIZE;
    assert!(buf.len() >= off + POINTER_ENTRY_SIZE, "PointerEntry index out of range");
    buf[off..off + 8].copy_from_slice(&entry.pe_bnum.to_ne_bytes());
    buf[off + 8..off + 12].copy_from_slice(&entry.pe_line_count.to_ne_bytes());
    buf[off + 12..off + 16].copy_from_slice(&entry.pe_old_lnum.to_ne_bytes());
    buf[off + 16..off + 20].copy_from_slice(&entry.pe_page_count.to_ne_bytes());
}

/// Create a new, empty pointer block (`ml_new_ptr`).
///
/// # Safety
/// `mfp` must be in a state where [`mf_new`] is safe to call (always
/// true for a live `MemfileT`).
pub unsafe fn ml_new_ptr(mfp: &mut MemfileT) -> *mut BhdrT {
    let page_size = mfp.mf_page_size;
    // SAFETY: forwarded from this function's own safety doc.
    let hp = unsafe { mf_new(mfp, false, 1) };
    // SAFETY: hp is valid (mf_new never returns null - it aborts via
    // xmalloc on OOM instead), with a data buffer of exactly
    // page_size bytes (page_count == 1).
    let buf = unsafe { (*hp).bh_data.as_data_mut() };
    pointer_block_init_empty(buf, page_size);
    hp
}

// --- DataBlock ---

/// `offsetof(DataBlock, db_index)`: `db_id` (2, padded to 4) +
/// `db_free` (4) + `db_txt_start` (4) + `db_txt_end` (4) +
/// `db_line_count` (8, this crate's fixed width - see the module doc
/// comment) = 24.
const DATA_HEADER_SIZE: u32 = 24;
/// `INDEX_SIZE` (`sizeof(unsigned)`).
const INDEX_SIZE: u32 = 4;

/// Create a new, empty data block (`ml_new_data`).
///
/// # Safety
/// Same as [`ml_new_ptr`].
pub unsafe fn ml_new_data(mfp: &mut MemfileT, negative: bool, page_count: u32) -> *mut BhdrT {
    let page_size = mfp.mf_page_size;
    // SAFETY: forwarded from this function's own safety doc.
    let hp = unsafe { mf_new(mfp, negative, page_count) };
    // SAFETY: hp is valid, with a data buffer of exactly
    // page_size * page_count bytes.
    let buf = unsafe { (*hp).bh_data.as_data_mut() };

    let txt_start = page_count * page_size;
    buf[0..2].copy_from_slice(&DATA_ID.to_ne_bytes());
    set_db_txt_start(buf, txt_start);
    set_db_txt_end(buf, txt_start);
    set_db_free(buf, txt_start - DATA_HEADER_SIZE);
    set_db_line_count(buf, 0);

    hp
}

fn db_free(buf: &[u8]) -> u32 {
    u32::from_ne_bytes(buf[4..8].try_into().unwrap())
}
fn set_db_free(buf: &mut [u8], v: u32) {
    buf[4..8].copy_from_slice(&v.to_ne_bytes());
}
fn db_txt_start(buf: &[u8]) -> u32 {
    u32::from_ne_bytes(buf[8..12].try_into().unwrap())
}
fn set_db_txt_start(buf: &mut [u8], v: u32) {
    buf[8..12].copy_from_slice(&v.to_ne_bytes());
}
fn set_db_txt_end(buf: &mut [u8], v: u32) {
    buf[12..16].copy_from_slice(&v.to_ne_bytes());
}
fn set_db_line_count(buf: &mut [u8], v: i64) {
    buf[16..24].copy_from_slice(&v.to_ne_bytes());
}
/// Writes `db_index[i] = v` (the byte offset where line `i + 1`'s text
/// starts, relative to the start of the block).
fn set_db_index(buf: &mut [u8], i: usize, v: u32) {
    let off = DATA_HEADER_SIZE as usize + i * INDEX_SIZE as usize;
    buf[off..off + 4].copy_from_slice(&v.to_ne_bytes());
}

/// Set the flags in the first block of the swapfile - just the
/// `buf.b_ffname.is_none()` fast path (`set_b0_fname`, partial - see
/// the module doc comment).
fn set_b0_fname(b0: &mut ZeroBlock, buf: &BufT) {
    if buf.b_ffname.is_none() {
        b0.b0_fname[0] = 0;
    }
    // The `buf.b_ffname.is_some()` branch (home_replace/os_fileinfo/
    // buf_store_file_info) is deferred - see the module doc comment.
    add_b0_fenc(b0, buf);
}

/// When there is room, add the `'fileencoding'` to block zero
/// (`add_b0_fenc`).
fn add_b0_fenc(b0: &mut ZeroBlock, buf: &BufT) {
    let size = B0_FNAME_SIZE_NOCRYPT;
    let fenc: &[u8] = buf.b_p_fenc.as_deref().unwrap_or(b"");
    let n = fenc.len();
    // strlen(b0p->b0_fname): only meaningful up to the first NUL - the
    // fast path above always leaves b0_fname[0] == 0, so this is 0 in
    // every case this pass actually exercises.
    let fname_len = b0.b0_fname.iter().take_while(|&&c| c != 0).count();
    if fname_len + n + 1 > size {
        b0.set_b0_flags(b0.b0_flags() & !B0_HAS_FENC);
    } else {
        let dst_start = size - n;
        b0.b0_fname[dst_start..size].copy_from_slice(fenc);
        b0.b0_fname[dst_start - 1] = 0;
        b0.set_b0_flags(b0.b0_flags() | B0_HAS_FENC);
    }
}

/// Open a new memline for `buf` (`ml_open`).
///
/// @return `OK`/`FAIL`.
///
/// # Safety
/// `buf.b_ml` must not already own a live `ml_mfp` (true for any
/// buffer that hasn't had `ml_open` called on it yet, the only
/// scenario this translation covers).
pub unsafe fn ml_open(buf: &mut BufT) -> i32 {
    // init fields in memline struct
    buf.b_ml.ml_stack_size = 0;
    buf.b_ml.ml_stack = Vec::new();
    buf.b_ml.ml_stack_top = 0;
    buf.b_ml.ml_locked = std::ptr::null_mut();
    buf.b_ml.ml_line_lnum = 0;
    buf.b_ml.ml_line_offset = 0;
    buf.b_ml.ml_chunksize = Vec::new();
    buf.b_ml.ml_usedchunks = 0;

    // SAFETY: touches the GLOBALS singleton - no overlapping live
    // access (matches every other function that does so).
    if unsafe { GLOBALS.get_mut() }.cmdmod.cmod_flags & cmod::NOSWAPFILE != 0 {
        buf.b_p_swf = 0;
    }

    // When 'updatecount' is non-zero swapfile may be opened later.
    let p_uc = unsafe { OPTION_VARS.get_mut() }.p_uc;
    buf.b_may_swap = buf.terminal.is_null() && p_uc != 0 && buf.b_p_swf != 0;

    // Open the memfile. No swapfile is created yet.
    let Some(mfp) = mf_open(None, 0) else {
        buf.b_ml.ml_mfp = std::ptr::null_mut();
        return FAIL;
    };
    let mfp = Box::into_raw(Box::new(mfp));

    buf.b_ml.ml_mfp = mfp;
    buf.b_ml.ml_flags = ML_EMPTY;
    buf.b_ml.ml_line_count = 1;

    // SAFETY: mfp was just constructed above and is exclusively
    // referenced from here on (matches this function's own contract);
    // all the mf_*/ml_new_* calls below only ever touch this one mfp.
    unsafe {
        // fill block0 struct and write page 0
        let hp = mf_new(&mut *mfp, false, 1);
        debug_assert_eq!((*hp).bh_bnum, 0, "E298: Didn't get block nr 0?");

        let page_size = (*mfp).mf_page_size;
        let mut b0 = ZeroBlock::new(page_size);

        if !buf.b_spell {
            b0.set_b0_dirty(if buf.b_changed != 0 { B0_DIRTY } else { 0 });
            b0.set_b0_flags((get_fileformat(buf) + 1) as u8);
            set_b0_fname(&mut b0, buf);
            b0.b0_uname = crate::os::users::os_get_username().unwrap_or_else(|e| e);
            b0.b0_hname = os_get_hostname();
            b0.b0_pid = crate::os::env::os_get_pid();
        }
        b0.write_into((*hp).bh_data.as_data_mut());

        // Always sync block number 0 to disk, so we can check the file
        // name in the swapfile in findswapname(). Don't do this for
        // help files or a spell buffer though.
        mf_put(&mut *mfp, hp, true, false);
        if !buf.b_help && !buf.b_spell {
            mf_sync(&mut *mfp, 0);
        }

        // Fill in root pointer block and write page 1.
        let hp = ml_new_ptr(&mut *mfp);
        debug_assert_eq!((*hp).bh_bnum, 1, "E298: Didn't get block nr 1?");
        let buf_bytes = (*hp).bh_data.as_data_mut();
        pointer_block_set_count(buf_bytes, 1);
        pointer_block_set_entry(
            buf_bytes,
            0,
            PointerEntry { pe_bnum: 2, pe_page_count: 1, pe_old_lnum: 1, pe_line_count: 1 },
        );
        mf_put(&mut *mfp, hp, true, false);

        // Allocate first data block and create an empty line 1.
        let hp = ml_new_data(&mut *mfp, false, 1);
        debug_assert_eq!((*hp).bh_bnum, 2, "E298: Didn't get block nr 2?");
        let buf_bytes = (*hp).bh_data.as_data_mut();
        let new_txt_start = db_txt_start(buf_bytes) - 1; // at end of block
        let new_free = db_free(buf_bytes) - 1 - INDEX_SIZE;
        set_db_txt_start(buf_bytes, new_txt_start);
        set_db_index(buf_bytes, 0, new_txt_start);
        set_db_free(buf_bytes, new_free);
        set_db_line_count(buf_bytes, 1);
        buf_bytes[new_txt_start as usize] = 0; // empty line
    }

    OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_block_size_fits_within_min_swap_page_size() {
        // Matches the original's own "If size of block0 changes
        // anyway, adjust MIN_SWAP_PAGE_SIZE in memfile.h!!" warning -
        // verify it still fits with this crate's own field widths.
        assert!(ZERO_BLOCK_SIZE < crate::memfile::MIN_SWAP_PAGE_SIZE as usize);
    }

    #[test]
    fn zero_block_offsets_match_hand_derived_layout() {
        // Cross-checked by hand against memline.c's ZeroBlock: 2 + 10
        // + 4 + 4 + 4 + 4 + 40 + 40 + 900 = 1008 before the magic
        // fields (already 8-byte aligned, so no compiler padding is
        // needed there regardless of b0_magic_long's width).
        assert_eq!(ZB_OFF_FNAME, 108);
        assert_eq!(ZB_OFF_MAGIC_LONG, 1008);
        assert_eq!(ZB_OFF_MAGIC_INT, 1016);
        assert_eq!(ZB_OFF_MAGIC_SHORT, 1020);
        assert_eq!(ZB_OFF_MAGIC_CHAR, 1022);
        assert_eq!(ZERO_BLOCK_SIZE, 1023);
    }

    #[test]
    fn long_to_char_char_to_long_roundtrip() {
        // Both functions operate on a fixed-width u32 (see this
        // module's doc comment on the b0_magic_long/db_line_count
        // width choice) - char_to_long reconstructs the 4 bytes as an
        // UNSIGNED 32-bit value (matching the original's own
        // byte-at-a-time, never-sign-extending reconstruction, which
        // is genuinely what a 64-bit `long` build of the original
        // itself produces too - only a 32-bit `long` build would see
        // -1 come back for an all-1-bits input, one more instance of
        // the same native-long-width ambiguity this crate sidesteps
        // by always using a fixed width instead).
        for n in [0i64, 1, -1, 4096, i32::MAX as i64, i32::MIN as i64] {
            let bytes = long_to_char(n);
            assert_eq!(char_to_long(&bytes), i64::from(n as u32));
        }
    }

    #[test]
    fn long_to_char_matches_hand_traced_little_endian_bytes() {
        // 0x30313233 -> bytes [0x33, 0x32, 0x31, 0x30] little-endian.
        assert_eq!(long_to_char(0x3031_3233), [0x33, 0x32, 0x31, 0x30]);
    }

    #[test]
    fn zero_block_dirty_and_flags_alias_the_last_two_fname_bytes() {
        let mut b0 = ZeroBlock::new(4096);
        b0.set_b0_dirty(0x55);
        b0.set_b0_flags(3);
        assert_eq!(b0.b0_fname[B0_FNAME_SIZE_ORG - 1], 0x55);
        assert_eq!(b0.b0_fname[B0_FNAME_SIZE_ORG - 2], 3);
        assert_eq!(b0.b0_dirty(), 0x55);
        assert_eq!(b0.b0_flags(), 3);
    }

    #[test]
    fn zero_block_write_into_places_id_and_magic_correctly() {
        let b0 = ZeroBlock::new(4096);
        let mut buf = vec![0xAAu8; 4096];
        b0.write_into(&mut buf);
        assert_eq!(buf[0], BLOCK0_ID0);
        assert_eq!(buf[1], BLOCK0_ID1);
        assert_eq!(&buf[2..6], b"VIM \0"[..4].as_ref()); // "VIM " prefix
        assert_eq!(
            i64::from_ne_bytes(buf[ZB_OFF_MAGIC_LONG..ZB_OFF_MAGIC_LONG + 8].try_into().unwrap()),
            B0_MAGIC_LONG
        );
        assert_eq!(buf[ZB_OFF_MAGIC_CHAR], B0_MAGIC_CHAR);
    }

    #[test]
    fn pb_count_max_matches_hand_computed_value_for_a_4096_page() {
        // (4096 - 8) / 24 = 170.333... -> 170.
        assert_eq!(pb_count_max(4096), 170);
    }

    #[test]
    fn pointer_block_roundtrip_via_raw_bytes() {
        let mut buf = vec![0u8; 4096];
        pointer_block_init_empty(&mut buf, 4096);
        assert_eq!(u16::from_ne_bytes(buf[0..2].try_into().unwrap()), PTR_ID);
        assert_eq!(u16::from_ne_bytes(buf[2..4].try_into().unwrap()), 0);
        assert_eq!(u16::from_ne_bytes(buf[4..6].try_into().unwrap()), 170);

        pointer_block_set_count(&mut buf, 1);
        pointer_block_set_entry(
            &mut buf,
            0,
            PointerEntry { pe_bnum: 2, pe_line_count: 1, pe_old_lnum: 1, pe_page_count: 1 },
        );
        assert_eq!(u16::from_ne_bytes(buf[2..4].try_into().unwrap()), 1);
        let off = PTR_HEADER_SIZE;
        assert_eq!(i64::from_ne_bytes(buf[off..off + 8].try_into().unwrap()), 2);
        assert_eq!(i32::from_ne_bytes(buf[off + 8..off + 12].try_into().unwrap()), 1);
        assert_eq!(i32::from_ne_bytes(buf[off + 12..off + 16].try_into().unwrap()), 1);
        assert_eq!(i32::from_ne_bytes(buf[off + 16..off + 20].try_into().unwrap()), 1);
    }

    #[test]
    fn ml_new_ptr_and_ml_new_data_produce_correct_blocks() {
        let mut mfp = mf_open(None, 0).unwrap();
        mfp.mf_page_size = 4096;
        unsafe {
            let ptr_hp = ml_new_ptr(&mut mfp);
            assert_eq!((*ptr_hp).bh_bnum, 0);
            let buf = (*ptr_hp).bh_data.as_data();
            assert_eq!(u16::from_ne_bytes(buf[0..2].try_into().unwrap()), PTR_ID);

            let data_hp = ml_new_data(&mut mfp, false, 1);
            assert_eq!((*data_hp).bh_bnum, 1);
            let buf = (*data_hp).bh_data.as_data();
            assert_eq!(u16::from_ne_bytes(buf[0..2].try_into().unwrap()), DATA_ID);
            assert_eq!(db_txt_start(buf), 4096);
            assert_eq!(db_free(buf), 4096 - DATA_HEADER_SIZE);

            crate::memfile::mf_free_bhdr(ptr_hp);
            crate::memfile::mf_free_bhdr(data_hp);
        }
    }

    fn test_buf() -> BufT {
        BufT::default()
    }

    #[test]
    fn ml_open_on_a_fresh_buffer_succeeds_and_wires_up_memline() {
        let mut buf = test_buf();
        let ret = unsafe { ml_open(&mut buf) };
        assert_eq!(ret, OK);
        assert!(!buf.b_ml.ml_mfp.is_null());
        assert_eq!(buf.b_ml.ml_flags, ML_EMPTY);
        assert_eq!(buf.b_ml.ml_line_count, 1);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn ml_open_writes_a_readable_block_zero() {
        let mut buf = test_buf();
        unsafe {
            assert_eq!(ml_open(&mut buf), OK);
            let mfp = &mut *buf.b_ml.ml_mfp;
            let hp = mfp.mf_hash.get(&0).copied().expect("block 0 should be cached");
            let data = (*hp).bh_data.as_data();
            assert_eq!(data[ZB_OFF_ID], BLOCK0_ID0);
            assert_eq!(data[ZB_OFF_ID + 1], BLOCK0_ID1);

            let mfp_owned = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp_owned, false);
        }
    }
}
