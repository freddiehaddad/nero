//! Translated from `src/nvim/memline.c` (partial, but now covers the
//! real B-tree read path, not just "open a brand new empty memline").
//!
//! Translated: `ml_open` (the whole function is translated, but two of
//! its own sub-paths are narrower than the original - see below),
//! `ml_new_ptr`, `ml_new_data`, `set_b0_fname` (only the
//! `buf.b_ffname.is_none()` fast path - see the note below),
//! `add_b0_fenc`, `long_to_char`/`char_to_long`, `ml_add_stack`,
//! `ml_lineadd`, `ml_flush_line` (only its "nothing to flush" fast
//! path - see below), **`ml_find_line`** (the real B-tree traversal:
//! the "is the wanted line in the already-locked block" fast path, the
//! `ML_FIND` stack-reuse search, and the full downward walk through
//! pointer blocks to a data block, including negative-block-number
//! translation mid-traversal), **`ml_get`/`ml_get_buf`** (via
//! `ml_get_buf_impl`, `will_change == false` only - see below).
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
//! `PointerBlock`/`DataBlock` now have real *read* accessors too
//! (`block_id`, `pb_count`/`pb_pointer`, `db_txt_end`/`db_index`), not
//! just the write-side ones from `ml_open` - designed once
//! `ml_find_line`/`ml_get_buf_impl` made clear exactly what reading
//! them back actually needs, rather than guessed at ahead of time.
//!
//! `ml_get`/`ml_get_buf`'s return type deliberately deviates from the
//! original's `char *` (a transient pointer, valid "only until the
//! next call" per the original's own documented contract, aliasing
//! either `buf.b_ml.ml_line_ptr`'s cache or a shared `"???"` fallback
//! buffer): this returns a freshly copied `Vec<u8>` each time instead.
//! The internal cache (`ml_line_lnum`/`ml_line_ptr`/`ml_line_textlen`)
//! is still maintained exactly as the original does (repeated
//! same-`lnum` calls still skip `ml_find_line`, byte-for-byte matching
//! the original's own caching logic), just copied out for the caller
//! rather than aliased - sidestepping the original's "invalidated by
//! the next call" hazard entirely, which isn't expressible (or
//! desirable) in safe Rust for a value a caller may want to hold past
//! its next `ml_get*` call. The returned bytes include the line's
//! trailing NUL byte, matching the original's own NUL-terminated-
//! C-string representation exactly (`ml_get_buf_len` is the function
//! that strips it via `ml_line_textlen - 1`, not `ml_get`/`ml_get_buf`
//! themselves).
//!
//! `CHECK(c, s)` (used in the original's `ml_find_line`/`ml_add_stack`)
//! is `#define CHECK(c, s) do {} while (0)` in the real, compiled
//! source (the `if (c) emsg(s)` alternative is commented out) - a
//! total no-op even in upstream neovim, so it's simply omitted here
//! rather than translated as a `debug_assert!` (there is nothing to
//! assert; the original itself never checks anything at that spot).
//!
//! Deferred (each needs another not-yet-translated subsystem):
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
//! - `ml_flush_line`'s dirty-line writeback (in-block `memmove`-based
//!   editing, or delete-then-append when the new line doesn't fit) and
//!   `ml_get_buf_mut`/`ml_get_buf_impl`'s `will_change == true` path:
//!   both need `ml_updatechunk`/`ml_delete_int`/`ml_append_int`, none
//!   of which exist yet. Nothing in this crate can currently mark a
//!   cached line dirty (`ml_replace`/`ml_append` aren't translated
//!   either), so `ML_LINE_DIRTY` is never actually set on any real
//!   call path here - this is the exact behavior a real, read-only
//!   workflow exhibits, not a narrowed approximation.
//! - `ml_setname`, `ml_close`/`ml_close_all`/`ml_close_notmod`,
//!   `ml_recover`, `ml_append`/`ml_replace`/`ml_delete` (the write-side
//!   counterparts to the traversal now translated) and everything else
//!   in this ~4300-line file: each is its own substantial undertaking,
//!   genuinely blocked on subsystems not yet translated (autocmd.c,
//!   the display/fold subsystem, `list_T`/eval engine, etc.) - not
//!   attempted this pass.
//! - The real, user-facing `iemsg`/`siemsg` calls inside `ml_find_line`
//!   (pointer-block-id-wrong, line-count-wrong) and `ml_get_buf_impl`
//!   (invalid line number, cannot find line) are omitted rather than
//!   translated as `debug_assert!`s - unlike `ml_open`'s block-number
//!   asserts, these ARE reachable for a genuinely corrupted/inconsistent
//!   tree, not internal-invariant-only conditions - but message.c's
//!   display pipeline itself remains not tractable, so only the state
//!   changes (falling back to `"???"`, returning `None`/null) are kept,
//!   matching the same "message display is a skippable side effect,
//!   state changes aren't" policy already validated for
//!   `mf_put`/`mf_write`/`mf_sync`.
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
use crate::memfile::{mf_get, mf_new, mf_open, mf_put, mf_sync, mf_trans_del};
use crate::memfile_defs::{BhdrT, BlocknrT, MemfileT};
use crate::memline_defs::{InfoptrT, ML_EMPTY, ML_LINE_DIRTY, ML_LOCKED_DIRTY, ML_LOCKED_POS};
use crate::option::get_fileformat;
use crate::option_vars::OPTION_VARS;
use crate::os::env::os_get_hostname;
use crate::pos_defs::LinenrT;
use crate::vim_defs::{FAIL, OK};

// Block IDs (memline.c's own anonymous `enum`).
const DATA_ID: u16 = ((b'd' as u16) << 8) + b'a' as u16;
const PTR_ID: u16 = ((b'p' as u16) << 8) + b't' as u16;
const BLOCK0_ID0: u8 = b'b';
const BLOCK0_ID1: u8 = b'0';

// db_index[] flags (memline.c's own #defines): the top bit of the
// `unsigned` index entry marks a line for the `:global` command's
// "line has a mark" bookkeeping; the rest is the plain byte offset.
const DB_MARKED: u32 = 1 << 31;
const DB_INDEX_MASK: u32 = !DB_MARKED;

// Arguments for ml_find_line() (memline.c's own anonymous `enum`).
const ML_DELETE: i32 = 0x11;
const ML_INSERT: i32 = 0x12;
const ML_FIND: i32 = 0x13;
const ML_FLUSH: i32 = 0x02;
/// `ML_SIMPLE(x)`: true for `ML_DELETE`/`ML_INSERT`/`ML_FIND`, false
/// for `ML_FLUSH`.
fn ml_simple(action: i32) -> bool {
    action & 0x10 != 0
}

/// `STACK_INCR`: number of entries added to `ml_stack` at a time.
const STACK_INCR: i32 = 5;

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

/// Reads `pb_count` (the number of [`PointerEntry`] values currently
/// stored).
fn pb_count(buf: &[u8]) -> u16 {
    u16::from_ne_bytes(buf[2..4].try_into().unwrap())
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

/// Reads one [`PointerEntry`] at `index`.
///
/// # Panics
/// If `buf` is too small to hold an entry at `index`.
fn pb_pointer(buf: &[u8], index: usize) -> PointerEntry {
    let off = PTR_HEADER_SIZE + index * POINTER_ENTRY_SIZE;
    assert!(buf.len() >= off + POINTER_ENTRY_SIZE, "PointerEntry index out of range");
    PointerEntry {
        pe_bnum: BlocknrT::from_ne_bytes(buf[off..off + 8].try_into().unwrap()),
        pe_line_count: i32::from_ne_bytes(buf[off + 8..off + 12].try_into().unwrap()),
        pe_old_lnum: i32::from_ne_bytes(buf[off + 12..off + 16].try_into().unwrap()),
        pe_page_count: i32::from_ne_bytes(buf[off + 16..off + 20].try_into().unwrap()),
    }
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
fn db_txt_end(buf: &[u8]) -> u32 {
    u32::from_ne_bytes(buf[12..16].try_into().unwrap())
}
fn set_db_txt_end(buf: &mut [u8], v: u32) {
    buf[12..16].copy_from_slice(&v.to_ne_bytes());
}
fn set_db_line_count(buf: &mut [u8], v: i64) {
    buf[16..24].copy_from_slice(&v.to_ne_bytes());
}
/// Reads `db_index[i]` (the byte offset where line `i + 1`'s text
/// starts, relative to the start of the block, with [`DB_MARKED`]
/// possibly set in the top bit - callers wanting the plain offset
/// should mask with [`DB_INDEX_MASK`], matching every real call site
/// in the original).
fn db_index(buf: &[u8], i: usize) -> u32 {
    let off = DATA_HEADER_SIZE as usize + i * INDEX_SIZE as usize;
    u32::from_ne_bytes(buf[off..off + 4].try_into().unwrap())
}
/// Writes `db_index[i] = v` (the byte offset where line `i + 1`'s text
/// starts, relative to the start of the block).
fn set_db_index(buf: &mut [u8], i: usize, v: u32) {
    let off = DATA_HEADER_SIZE as usize + i * INDEX_SIZE as usize;
    buf[off..off + 4].copy_from_slice(&v.to_ne_bytes());
}

/// Read the 2-byte block-type ID tag shared by both [`DataBlock`]
/// (`db_id`) and [`PointerBlock`] (`pb_id`) - both structs place it at
/// offset 0, exactly like the original's own `dp->db_id`/`pp->pb_id`
/// reads through two different struct casts over the very same bytes.
fn block_id(buf: &[u8]) -> u16 {
    u16::from_ne_bytes(buf[0..2].try_into().unwrap())
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

/// Add an entry to the info pointer stack (`ml_add_stack`).
///
/// @return the index of the new entry.
fn ml_add_stack(buf: &mut BufT) -> i32 {
    let top = buf.b_ml.ml_stack_top;

    // may have to increase the stack size
    if top == buf.b_ml.ml_stack_size {
        // (the original's CHECK(top > 0, "Stack size increases") is a
        // compiled-out no-op - `#define CHECK(c, s) do {} while (0)` -
        // in the real source, not translated.)
        buf.b_ml.ml_stack_size += STACK_INCR;
        buf.b_ml
            .ml_stack
            .resize(buf.b_ml.ml_stack_size as usize, InfoptrT::default());
    }

    buf.b_ml.ml_stack_top += 1;
    top
}

/// Update the pointer blocks on the stack for inserted/deleted lines.
/// The stack itself is also updated (`ml_lineadd`).
///
/// When an insert/delete line action fails, the line is not inserted/
/// deleted, but the pointer blocks have already been updated. That is
/// fixed here by walking through the stack.
///
/// `count` is the number of lines added, negative if lines have been
/// deleted.
///
/// # Safety
/// `buf.b_ml.ml_mfp` must be a valid, non-null pointer to a live
/// `MemfileT`.
unsafe fn ml_lineadd(buf: &mut BufT, count: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let mfp = unsafe { &mut *buf.b_ml.ml_mfp };

    let mut idx = buf.b_ml.ml_stack_top - 1;
    while idx >= 0 {
        let ip = buf.b_ml.ml_stack[idx as usize];
        // SAFETY: forwarded from this function's own safety doc.
        let hp = unsafe { mf_get(mfp, ip.ip_bnum, 1) };
        if hp.is_null() {
            break;
        }
        // SAFETY: hp is a valid, just-locked block.
        let data = unsafe { (*hp).bh_data.as_data() };
        if block_id(data) != PTR_ID {
            // SAFETY: forwarded.
            unsafe { mf_put(mfp, hp, false, false) };
            // (iemsg(E317: pointer block id wrong 2) omitted - display-
            // pipeline-bound, matching this crate's established
            // message.c policy; the state change below still happens.)
            break;
        }
        // SAFETY: hp is valid.
        let data_mut = unsafe { (*hp).bh_data.as_data_mut() };
        let mut entry = pb_pointer(data_mut, ip.ip_index as usize);
        entry.pe_line_count += count;
        pointer_block_set_entry(data_mut, ip.ip_index as usize, entry);
        buf.b_ml.ml_stack[idx as usize].ip_high += count;
        // SAFETY: forwarded.
        unsafe { mf_put(mfp, hp, true, false) };

        idx -= 1;
    }
}

/// Flush `ml_line` if necessary (`ml_flush_line`).
///
/// Only the "nothing to flush" fast path is translated: the real
/// dirty-line writeback (in-block `memmove`-based editing, or a
/// delete-then-append when the new line doesn't fit) needs
/// `ml_updatechunk`/`ml_delete_int`/`ml_append_int`, none of which
/// exist yet. Since nothing in this crate can currently mark a cached
/// line dirty (`ml_replace`/`ml_append`/`ml_get_buf_mut`'s write path
/// aren't translated either), `ML_LINE_DIRTY` is never actually set on
/// any real call path here - this is the exact behavior the original
/// itself exhibits for a read-only workflow, not a narrowed
/// approximation.
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT`.
unsafe fn ml_flush_line(buf: &mut BufT, _noalloc: bool) {
    if buf.b_ml.ml_line_lnum == 0 || buf.b_ml.ml_mfp.is_null() {
        return; // nothing to do
    }
    if buf.b_ml.ml_flags & ML_LINE_DIRTY != 0 {
        // Deferred - see this function's own doc comment. Reachable
        // only once ml_get_buf_mut/ml_replace exist.
        unimplemented!(
            "ml_flush_line: writing back a dirty cached line needs ml_updatechunk/\
             ml_delete_int/ml_append_int, not yet translated"
        );
    }
}

/// Lookup line `lnum` in a memline (`ml_find_line`).
///
/// `action`: if `ML_DELETE`/`ML_INSERT`, the line count is updated
/// while searching; if `ML_FLUSH`, only flush a locked block; if
/// `ML_FIND`, just find the line.
///
/// If the block was found it is locked and put in `ml_locked`. The
/// stack is updated to lead to the locked block.
///
/// @return null on failure, the locked block header otherwise.
///
/// # Safety
/// `buf.b_ml.ml_mfp` must be a valid, non-null pointer to a live
/// `MemfileT` (true for any buffer `ml_open` has succeeded on).
unsafe fn ml_find_line(buf: &mut BufT, lnum: LinenrT, action: i32) -> *mut BhdrT {
    // SAFETY: forwarded from this function's own safety doc.
    let mfp = unsafe { &mut *buf.b_ml.ml_mfp };

    // If there is a locked block check if the wanted line is in it.
    // If not, flush and release the locked block.
    if !buf.b_ml.ml_locked.is_null() {
        if ml_simple(action) && buf.b_ml.ml_locked_low <= lnum && buf.b_ml.ml_locked_high >= lnum {
            // remember to update pointer blocks and stack later
            if action == ML_INSERT {
                buf.b_ml.ml_locked_lineadd += 1;
                buf.b_ml.ml_locked_high += 1;
            } else if action == ML_DELETE {
                buf.b_ml.ml_locked_lineadd -= 1;
                buf.b_ml.ml_locked_high -= 1;
            }
            return buf.b_ml.ml_locked;
        }

        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            mf_put(
                mfp,
                buf.b_ml.ml_locked,
                buf.b_ml.ml_flags & ML_LOCKED_DIRTY != 0,
                buf.b_ml.ml_flags & ML_LOCKED_POS != 0,
            );
        }
        buf.b_ml.ml_locked = std::ptr::null_mut();

        // If lines have been added or deleted in the locked block, need
        // to update the line count in pointer blocks.
        if buf.b_ml.ml_locked_lineadd != 0 {
            let lineadd = buf.b_ml.ml_locked_lineadd;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { ml_lineadd(buf, lineadd) };
        }
    }

    if action == ML_FLUSH {
        // nothing else to do
        return std::ptr::null_mut();
    }

    let mut bnum: BlocknrT = 1; // start at the root of the tree
    let mut page_count: u32 = 1;
    let mut low: LinenrT = 1;
    let mut high: LinenrT = buf.b_ml.ml_line_count;

    if action == ML_FIND {
        // first try stack entries
        let mut top = buf.b_ml.ml_stack_top - 1;
        let mut found = false;
        while top >= 0 {
            let ip = buf.b_ml.ml_stack[top as usize];
            if ip.ip_low <= lnum && ip.ip_high >= lnum {
                bnum = ip.ip_bnum;
                low = ip.ip_low;
                high = ip.ip_high;
                buf.b_ml.ml_stack_top = top; // truncate stack at prev entry
                found = true;
                break;
            }
            top -= 1;
        }
        if !found {
            buf.b_ml.ml_stack_top = 0; // not found, start at the root
        }
    } else {
        // ML_DELETE or ML_INSERT
        buf.b_ml.ml_stack_top = 0; // start at the root
    }

    // search downwards in the tree until a data block is found
    let found_hp: Option<*mut BhdrT> = 'traverse: {
        loop {
            // SAFETY: forwarded from this function's own safety doc.
            let hp = unsafe { mf_get(mfp, bnum, page_count) };
            if hp.is_null() {
                break 'traverse None; // error_noblock
            }

            // update high for insert/delete
            if action == ML_INSERT {
                high += 1;
            } else if action == ML_DELETE {
                high -= 1;
            }

            // SAFETY: hp is a valid, just-locked block.
            let data = unsafe { (*hp).bh_data.as_data() };
            if block_id(data) == DATA_ID {
                // data block
                buf.b_ml.ml_locked = hp;
                buf.b_ml.ml_locked_low = low;
                buf.b_ml.ml_locked_high = high;
                buf.b_ml.ml_locked_lineadd = 0;
                buf.b_ml.ml_flags &= !(ML_LOCKED_DIRTY | ML_LOCKED_POS);
                return hp;
            }

            // must be a pointer block
            if block_id(data) != PTR_ID {
                // (iemsg(E317: pointer block id wrong) omitted - see
                // ml_lineadd's own comment on this crate's message.c
                // policy.)
                // SAFETY: forwarded.
                unsafe { mf_put(mfp, hp, false, false) };
                break 'traverse None; // error_block -> error_noblock
            }

            let top = ml_add_stack(buf); // add new entry to stack
            buf.b_ml.ml_stack[top as usize] =
                InfoptrT { ip_bnum: bnum, ip_low: low, ip_high: high, ip_index: -1 };

            let mut dirty = false;
            let count = pb_count(data);
            let mut found_idx: Option<usize> = None;
            for idx in 0..count as usize {
                let t = pb_pointer(data, idx).pe_line_count;
                low += t;
                if low > lnum {
                    buf.b_ml.ml_stack[top as usize].ip_index = idx as i32;
                    let mut entry = pb_pointer(data, idx);
                    bnum = entry.pe_bnum;
                    page_count = entry.pe_page_count as u32;
                    high = low - 1;
                    low -= t;

                    // a negative block number may have been changed
                    if bnum < 0 {
                        let bnum2 = mf_trans_del(mfp, bnum);
                        if bnum != bnum2 {
                            bnum = bnum2;
                            entry.pe_bnum = bnum2;
                            // SAFETY: hp is still locked/valid.
                            let data_mut = unsafe { (*hp).bh_data.as_data_mut() };
                            pointer_block_set_entry(data_mut, idx, entry);
                            dirty = true;
                        }
                    }
                    found_idx = Some(idx);
                    break;
                }
            }

            let Some(idx) = found_idx else {
                // past the end: something wrong!
                // (siemsg(...) omitted - see ml_lineadd's own comment.)
                // SAFETY: forwarded.
                unsafe { mf_put(mfp, hp, false, false) };
                break 'traverse None; // error_block -> error_noblock
            };

            if action == ML_DELETE {
                // SAFETY: hp is valid.
                let data_mut = unsafe { (*hp).bh_data.as_data_mut() };
                let mut entry = pb_pointer(data_mut, idx);
                entry.pe_line_count -= 1;
                pointer_block_set_entry(data_mut, idx, entry);
                dirty = true;
            } else if action == ML_INSERT {
                // SAFETY: hp is valid.
                let data_mut = unsafe { (*hp).bh_data.as_data_mut() };
                let mut entry = pb_pointer(data_mut, idx);
                entry.pe_line_count += 1;
                pointer_block_set_entry(data_mut, idx, entry);
                dirty = true;
            }
            // SAFETY: forwarded.
            unsafe { mf_put(mfp, hp, dirty, false) };
        }
    };

    debug_assert!(found_hp.is_none(), "unreachable: success path always returns directly above");

    // error_noblock: if action is ML_DELETE or ML_INSERT we have to
    // correct the tree for the incremented/decremented line counts,
    // because there won't be a line inserted/deleted after all.
    if action == ML_DELETE {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { ml_lineadd(buf, 1) };
    } else if action == ML_INSERT {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { ml_lineadd(buf, -1) };
    }
    buf.b_ml.ml_stack_top = 0;
    std::ptr::null_mut()
}

/// Get a pointer to a line in a specific buffer (`ml_get_buf_impl`).
///
/// Only the `will_change == false` path is translated (`ml_get`/
/// `ml_get_buf`'s only real caller need) - `will_change == true`
/// (`ml_get_buf_mut`) additionally needs the `ML_LINE_DIRTY`/
/// `ML_GET_ALLOC_LINES` writeback bookkeeping, deferred alongside
/// `ml_flush_line`'s dirty-line path.
///
/// Unlike the original's `char *` return (a transient pointer valid
/// "only until the next call", by the original's own documented
/// contract, into either `buf.b_ml.ml_line_ptr`'s cache or a shared
/// `"???"` fallback buffer), this returns a freshly copied `Vec<u8>`
/// each time - the internal cache (`ml_line_lnum`/`ml_line_ptr`/
/// `ml_line_textlen`) is still maintained exactly as the original
/// does (so repeated same-`lnum` calls skip `ml_find_line`, matching
/// the original's own caching behavior byte-for-byte), just copied out
/// for the caller instead of aliased, sidestepping the original's
/// "invalidated by the next call" hazard entirely (not expressible/
/// desirable in safe Rust for a value callers may want to hold past
/// their next `ml_get*` call).
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT`.
unsafe fn ml_get_buf_impl(buf: &mut BufT, lnum: LinenrT, will_change: bool) -> Vec<u8> {
    if buf.b_ml.ml_mfp.is_null() {
        // there are no lines
        buf.b_ml.ml_line_textlen = 1;
        return Vec::new();
    }

    if lnum > buf.b_ml.ml_line_count {
        // invalid line number
        // (siemsg(E1373: ...) omitted - display-pipeline-bound.)
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { ml_flush_line(buf, false) };
        buf.b_ml.ml_line_textlen = 4;
        buf.b_ml.ml_line_lnum = lnum;
        return b"???".to_vec();
    }
    let lnum = lnum.max(1); // pretend line 0 is line 1

    // See if it is the same line as requested last time. Otherwise may
    // need to flush last used line.
    if buf.b_ml.ml_line_lnum != lnum {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { ml_flush_line(buf, false) };

        // Find the data block containing the line. This also fills
        // the stack with the blocks from the root to the data block
        // and releases any locked block.
        // SAFETY: forwarded from this function's own safety doc.
        let hp = unsafe { ml_find_line(buf, lnum, ML_FIND) };
        if hp.is_null() {
            // (siemsg(E315: ...) omitted - display-pipeline-bound.)
            buf.b_ml.ml_line_textlen = 4;
            buf.b_ml.ml_line_lnum = lnum;
            return b"???".to_vec();
        }

        // SAFETY: hp is a valid, just-locked data block.
        let dp = unsafe { (*hp).bh_data.as_data() };

        let idx = (lnum - buf.b_ml.ml_locked_low) as usize;
        let start = db_index(dp, idx) & DB_INDEX_MASK;
        // The text ends where the previous line starts. The first
        // line ends at the end of the block.
        let end = if idx == 0 { db_txt_end(dp) } else { db_index(dp, idx - 1) & DB_INDEX_MASK };

        buf.b_ml.ml_line_ptr = Some(dp[start as usize..end as usize].to_vec());
        buf.b_ml.ml_line_textlen = (end - start) as i32;
        buf.b_ml.ml_line_lnum = lnum;
        buf.b_ml.ml_flags &= !(ML_LINE_DIRTY | crate::memline_defs::ML_ALLOCATED);
    }

    if will_change {
        // Deferred - see this function's own doc comment.
        unimplemented!("ml_get_buf_mut (will_change=true) is not yet translated");
    }

    buf.b_ml.ml_line_ptr.clone().unwrap_or_default()
}

/// @return a pointer to a (read-only copy of a) line in `curbuf`
/// (`ml_get`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` whose `b_ml.ml_mfp`, if non-null, points to a live
/// `MemfileT`.
#[must_use]
pub unsafe fn ml_get(lnum: LinenrT) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_get_buf_impl(curbuf, lnum, false) }
}

/// @return a pointer to a (read-only copy of a) line. Same as
/// [`ml_get`], but taking the buffer as an argument (`ml_get_buf`).
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT`.
#[must_use]
pub unsafe fn ml_get_buf(buf: &mut BufT, lnum: LinenrT) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_get_buf_impl(buf, lnum, false) }
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

    #[test]
    fn ml_get_buf_on_freshly_opened_buffer_returns_the_single_empty_line() {
        let mut buf = test_buf();
        unsafe {
            assert_eq!(ml_open(&mut buf), OK);
            // ml_open's one line is empty: just its own NUL terminator
            // (ml_get*'s returned bytes include the trailing NUL,
            // matching the original's own NUL-terminated-C-string
            // representation - ml_get_buf_len is the function that
            // strips it, not ml_get/ml_get_buf themselves).
            assert_eq!(ml_get_buf(&mut buf, 1), vec![0u8]);
            assert_eq!(buf.b_ml.ml_line_lnum, 1);
            assert_eq!(buf.b_ml.ml_line_textlen, 1);

            let mfp_owned = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp_owned, false);
        }
    }

    #[test]
    fn ml_get_buf_out_of_range_lnum_returns_questionmarks_without_panicking() {
        let mut buf = test_buf();
        unsafe {
            assert_eq!(ml_open(&mut buf), OK);
            assert_eq!(ml_get_buf(&mut buf, 99), b"???".to_vec());

            let mfp_owned = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp_owned, false);
        }
    }

    /// Manually builds a memline with a root pointer block (bnum 1)
    /// pointing at two data blocks (bnum 2: 2 lines, bnum 3: 1 line) -
    /// exercises `ml_find_line`'s real B-tree traversal (the
    /// cumulative-line-count pointer-block search, not just the
    /// single-data-block trivial case `ml_open` itself produces).
    ///
    /// Block 0 is deliberately consumed by a throwaway block first
    /// (matching `ml_open`'s own block-numbering convention: 0 =
    /// block-zero/`ZeroBlock`, 1 = root pointer block, 2+ = data
    /// blocks) - `ml_find_line` always starts its search at bnum 1.
    fn build_three_line_two_block_memline() -> BufT {
        let mut mfp = mf_open(None, 0).unwrap();
        mfp.mf_page_size = 4096;

        unsafe {
            let dummy = mf_new(&mut mfp, false, 1);
            mf_put(&mut mfp, dummy, false, false);

            let root_hp = ml_new_ptr(&mut mfp);
            assert_eq!((*root_hp).bh_bnum, 1);

            // Data block 1 (bnum 2): lines "hello", "world" - line 1
            // ("hello") sits closest to db_txt_end, line 2 ("world")
            // just before it (db_index[idx] is the START of that
            // line's text; idx 0's END is db_txt_end, every other
            // idx's END is the previous entry's start).
            let data1_hp = ml_new_data(&mut mfp, false, 1);
            assert_eq!((*data1_hp).bh_bnum, 2);
            {
                let d = (*data1_hp).bh_data.as_data_mut();
                let txt_end = db_txt_end(d);
                let hello_start = txt_end - 6; // "hello\0"
                let world_start = hello_start - 6; // "world\0"
                d[hello_start as usize..hello_start as usize + 6].copy_from_slice(b"hello\0");
                d[world_start as usize..world_start as usize + 6].copy_from_slice(b"world\0");
                set_db_index(d, 0, hello_start);
                set_db_index(d, 1, world_start);
                set_db_txt_start(d, world_start);
                set_db_line_count(d, 2);
            }
            mf_put(&mut mfp, data1_hp, true, false);

            // Data block 2 (bnum 3): line "foo".
            let data2_hp = ml_new_data(&mut mfp, false, 1);
            assert_eq!((*data2_hp).bh_bnum, 3);
            {
                let d = (*data2_hp).bh_data.as_data_mut();
                let txt_end = db_txt_end(d);
                let foo_start = txt_end - 4; // "foo\0"
                d[foo_start as usize..foo_start as usize + 4].copy_from_slice(b"foo\0");
                set_db_index(d, 0, foo_start);
                set_db_txt_start(d, foo_start);
                set_db_line_count(d, 1);
            }
            mf_put(&mut mfp, data2_hp, true, false);

            // Root pointer block: entry 0 -> 2 lines in bnum 2, entry
            // 1 -> 1 line in bnum 3.
            {
                let root_buf = (*root_hp).bh_data.as_data_mut();
                pointer_block_set_count(root_buf, 2);
                pointer_block_set_entry(
                    root_buf,
                    0,
                    PointerEntry { pe_bnum: 2, pe_page_count: 1, pe_old_lnum: 1, pe_line_count: 2 },
                );
                pointer_block_set_entry(
                    root_buf,
                    1,
                    PointerEntry { pe_bnum: 3, pe_page_count: 1, pe_old_lnum: 1, pe_line_count: 1 },
                );
            }
            mf_put(&mut mfp, root_hp, true, false);
        }

        let mut buf = BufT::default();
        buf.b_ml.ml_mfp = Box::into_raw(Box::new(mfp));
        buf.b_ml.ml_line_count = 3;
        buf
    }

    fn close_test_memline(buf: BufT) {
        unsafe {
            let mfp_owned = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp_owned, false);
        }
    }

    #[test]
    fn ml_get_buf_traverses_pointer_block_to_the_right_data_block() {
        let mut buf = build_three_line_two_block_memline();
        unsafe {
            assert_eq!(ml_get_buf(&mut buf, 1), b"hello\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 2), b"world\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 3), b"foo\0".to_vec());
        }
        close_test_memline(buf);
    }

    #[test]
    fn ml_get_buf_repeated_same_lnum_calls_return_consistent_results() {
        let mut buf = build_three_line_two_block_memline();
        unsafe {
            assert_eq!(ml_get_buf(&mut buf, 2), b"world\0".to_vec());
            // Second call for the same lnum should hit ml_find_line's
            // own "already locked" fast path and still return the
            // same bytes.
            assert_eq!(ml_get_buf(&mut buf, 2), b"world\0".to_vec());
            assert_eq!(buf.b_ml.ml_line_lnum, 2);
        }
        close_test_memline(buf);
    }

    #[test]
    fn ml_get_buf_out_of_order_access_across_blocks_stays_correct() {
        // Access lines out of order (and re-visit an earlier one) to
        // exercise ml_find_line's locked-block release/stack-reuse
        // path, not just a single monotonic forward scan.
        let mut buf = build_three_line_two_block_memline();
        unsafe {
            assert_eq!(ml_get_buf(&mut buf, 3), b"foo\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 1), b"hello\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 3), b"foo\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 2), b"world\0".to_vec());
        }
        close_test_memline(buf);
    }

    #[test]
    fn ml_get_matches_ml_get_buf_via_curbuf() {
        let _guard = crate::globals::global_state_test_lock();
        let mut buf = build_three_line_two_block_memline();
        let prev_curbuf = unsafe { GLOBALS.get_mut() }.curbuf;
        unsafe { GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        let result = unsafe { ml_get(2) };
        assert_eq!(result, b"world\0".to_vec());

        unsafe { GLOBALS.get_mut() }.curbuf = prev_curbuf;
        close_test_memline(buf);
    }
}
