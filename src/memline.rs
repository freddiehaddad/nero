//! Translated from `src/nvim/memline.c` (partial, but now covers the
//! *complete* B-tree read and write core - including block-splitting
//! on append and block-removal on delete - for a real, working
//! in-memory line store, not just "open a brand new empty memline").
//!
//! Translated: `ml_open` (the whole function is translated, but two of
//! its own sub-paths are narrower than the original - see below),
//! `ml_new_ptr`, `ml_new_data`, `set_b0_fname` (only the
//! `buf.b_ffname.is_none()` fast path - see the note below),
//! `add_b0_fenc`, `long_to_char`/`char_to_long`, `ml_add_stack`,
//! `ml_lineadd`, `ml_add_deleted_len`/`ml_add_deleted_len_buf` (the
//! common, non-`update_need_codepoints` path), `ml_updatechunk` (a
//! no-op stub - see below), **`ml_find_line`** (the real B-tree
//! traversal: the "is the wanted line in the already-locked block"
//! fast path, the `ML_FIND` stack-reuse search, and the full downward
//! walk through pointer blocks to a data block, including
//! negative-block-number translation mid-traversal), **`ml_get`/
//! `ml_get_buf`/`ml_get_buf_mut`** (via `ml_get_buf_impl`, both
//! `will_change` values), `ml_get_buf_len`/`ml_get_len`/
//! `ml_get_pos_len`/`ml_get_pos`, **`ml_append_int`/`ml_append_flush`/
//! `ml_append_flags`/`ml_append`/`ml_append_buf`** now in full,
//! including block-splitting (see below), **`ml_replace_buf_len`
//! /`ml_replace`** (also serving `ml_replace_len`/`ml_replace_buf`'s
//! role, since a Rust byte slice already knows its own length), and
//! **`ml_flush_line`** now in full (both the in-place `memmove`-based
//! rewrite and the delete-then-append fallback), and **`ml_delete_int`/
//! `ml_delete_flags`/`ml_delete`/`ml_delete_buf`** now in full,
//! including block-removal (see below).
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
//! (`block_id`, `pb_count`/`pb_pointer`, `db_txt_end`/`db_index`/
//! `db_line_count`), not just the write-side ones from `ml_open` -
//! designed once `ml_find_line`/`ml_get_buf_impl` made clear exactly
//! what reading them back actually needs, rather than guessed at ahead
//! of time.
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
//! themselves) - every `line: &[u8]` parameter accepted by
//! `ml_append*`/`ml_replace*` in this module follows the same
//! convention: it must already include its own trailing NUL (e.g. an
//! empty line is `b"\0"`, one byte, not `b""`, zero bytes - a real
//! translation bug this session, caught by a test expecting an empty
//! line back out and instead getting nothing).
//!
//! `CHECK(c, s)` (used in the original's `ml_find_line`/`ml_add_stack`)
//! is `#define CHECK(c, s) do {} while (0)` in the real, compiled
//! source (the `if (c) emsg(s)` alternative is commented out) - a
//! total no-op even in upstream neovim, so it's simply omitted here
//! rather than translated as a `debug_assert!` (there is nothing to
//! assert; the original itself never checks anything at that spot).
//!
//! **`ml_delete_int`'s block-removal path is now translated in full**:
//! when deleting a line empties its data block, the block is freed and
//! its entry removed from the parent pointer block; if that pointer
//! block itself becomes empty, the same removal cascades one level
//! further up, and so on toward the root. Verified with two tests
//! against a real 2-data-block memline: removing the *last* pointer
//! entry (no shift needed) and removing an *earlier* entry, forcing
//! the following entry to shift down - both passed on the first real
//! run. Not separately tested: a 3+-level tree where a pointer block's
//! own removal cascades to its parent (the loop structure is a direct,
//! line-by-line translation of the original's own upward walk, and
//! `ml_lineadd` already exercises an analogous "walk the stack"
//! pattern elsewhere, but a synthetic 3-level tree wasn't constructed
//! to exercise this specific case directly - noted here rather than
//! silently overclaiming full coverage).
//!
//! **`ml_append_int`'s block-splitting path is now translated in
//! full**, including the "insert in front of the next block" redirect
//! and the "block 1 becomes the new root" special case (the tree
//! gaining an extra level when the actual root, block 1, itself needs
//! to split - since the root has no parent to insert a sibling entry
//! into, its own content is relocated into a fresh child block first,
//! then that child is split normally). Verified with two tests: a
//! single data-block split (with a page size generous enough that the
//! pointer block itself never needs to grow) and a repeated-append
//! stress test deliberately using a small `pb_count_max` (3) to force
//! several data-block splits *and* at least one root-pointer-block
//! split within 12 appends, confirmed by reading back every appended
//! line afterward in order with its exact original content - proving
//! the tree, now several levels deep, stays fully navigable.
//!
//! **A genuinely adversarial page size (32 bytes) was tried first and
//! discovered to hang** during this work - not a translation bug, but
//! a real, inherent property of the original algorithm: with
//! `pb_count_max == 1` (the maximum a pointer block this tiny can
//! ever hold), inserting a second data block reference always
//! requires *another* "block 1 becomes root" cycle, forever, since no
//! pointer block at any level can ever hold more than one child. Real
//! neovim never configures a page this small (always >= 4096 bytes,
//! giving `pb_count_max` in the hundreds), so this never arises in
//! practice - confirmed by adding temporary `eprintln!` tracing to
//! pinpoint the exact repeating state before concluding it was a test
//! configuration issue, not a bug, and re-choosing a still-small-but-
//! realistic page size (80 bytes, `pb_count_max == 3`) instead.
//!
//! `ml_updatechunk` (the `line2byte()`/`byte2line()` fast-lookup chunk
//! cache) is a no-op stub: a pure performance optimization with its
//! own nontrivial chunk-splitting logic, and neither `line2byte`/
//! `byte2line` is translated yet to observe its absence.
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
//! - `ml_setname`, `ml_close`/`ml_close_all`/`ml_close_notmod`,
//!   `ml_recover` and everything else in this ~4300-line file: each is
//!   its own substantial undertaking, genuinely blocked on subsystems
//!   not yet translated (autocmd.c, the display/fold subsystem,
//!   `list_T`/eval engine, etc.) - not attempted this pass.
//! - The real, user-facing `iemsg`/`siemsg`/`set_keep_msg` calls inside
//!   `ml_find_line`, `ml_get_buf_impl`, and `ml_delete_int` (pointer-
//!   block-id-wrong, invalid line number, cannot find line, "No lines
//!   in buffer") are omitted rather than translated as
//!   `debug_assert!`s - unlike `ml_open`'s block-number asserts, these
//!   ARE reachable for a genuinely corrupted/inconsistent tree (or, for
//!   `set_keep_msg`, just a normal user message), not internal-
//!   invariant-only conditions - but message.c's display pipeline
//!   itself remains not tractable, so only the state changes (falling
//!   back to `"???"`, returning `None`/null, setting `ML_EMPTY`) are
//!   kept, matching the same "message display is a skippable side
//!   effect, state changes aren't" policy already validated for
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
use crate::globals::{GlobalCell, GLOBALS};
use crate::memfile::{mf_free, mf_get, mf_new, mf_open, mf_put, mf_sync, mf_trans_del};
use crate::memfile_defs::{BhdrT, BlocknrT, MemfileT};
use crate::memline_defs::{
    InfoptrT, ML_ALLOCATED, ML_CHNK_ADDLINE, ML_CHNK_DELLINE, ML_CHNK_UPDLINE, ML_EMPTY,
    ML_LINE_DIRTY, ML_LOCKED_DIRTY, ML_LOCKED_POS,
};
use crate::option::get_fileformat;
use crate::option_vars::OPTION_VARS;
use crate::os::env::os_get_hostname;
use crate::pos_defs::LinenrT;
use crate::vim_defs::{FAIL, OK};

// Arguments for ml_append_int()/ml_new_data() (memline.h's own
// anonymous `enum`).
const ML_APPEND_NEW: i32 = 1; // starting to edit a new file
const ML_APPEND_MARK: i32 = 2; // mark the new line

// Flags for ml_delete_int() (memline.h's own anonymous `enum`).
const ML_DEL_MESSAGE: i32 = 1; // may give a "No lines in buffer" message
                                // (ML_DEL_UNDO = 2 is commented out/unused upstream too)

/// The line number where the first `:global`-command mark may be. If
/// it is 0 there are no marks at all (`lowest_marked`, `static` in the
/// original - kept private here too, matching the original's
/// "current buffer only" scoping via a plain module-level `GlobalCell`
/// rather than a per-buffer field).
static LOWEST_MARKED: GlobalCell<LinenrT> = GlobalCell::new(0);

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
fn db_line_count(buf: &[u8]) -> i64 {
    i64::from_ne_bytes(buf[16..24].try_into().unwrap())
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

/// Sets `buf.deleted_bytes`/`deleted_bytes2` bookkeeping for text about
/// to be deleted, for the current buffer (`ml_add_deleted_len`).
pub fn ml_add_deleted_len(ptr: &[u8], len: Option<usize>) {
    // SAFETY: touches GLOBALS.curbuf - same requirement as every other
    // function that does so.
    let curbuf = unsafe { &mut *GLOBALS.get_mut().curbuf };
    ml_add_deleted_len_buf(curbuf, ptr, len);
}

/// Sets `buf.deleted_bytes`/`deleted_bytes2` bookkeeping for text about
/// to be deleted (`ml_add_deleted_len_buf`).
///
/// `len`: `None` matches the original's `-1` ("use `ptr`'s own natural
/// length"); `Some(n)` is capped to `ptr.len()` like the original caps
/// to `strlen(ptr)`.
///
/// Deferred: the `buf.update_need_codepoints` branch (needs
/// `mb_utflen`, not yet translated) - `update_need_codepoints` defaults
/// to `false` and nothing in this crate sets it yet, so this is the
/// exact behavior of every real call site so far, not an
/// approximation.
pub fn ml_add_deleted_len_buf(buf: &mut BufT, ptr: &[u8], len: Option<usize>) {
    // SAFETY: touches GLOBALS - same requirement as every other
    // function that does so.
    if unsafe { GLOBALS.get_mut() }.inhibit_delete_count != 0 {
        return;
    }
    let maxlen = ptr.len();
    let len = len.map_or(maxlen, |l| l.min(maxlen));
    buf.deleted_bytes += len + 1;
    buf.deleted_bytes2 += len + 1;
    // (buf.update_need_codepoints branch deferred - see doc comment.)
}

/// Keep information for finding the byte offset of a line
/// (`ml_updatechunk`).
///
/// Deferred entirely (a no-op stub): this is a pure performance cache
/// for `line2byte()`/`byte2line()` (neither translated yet, so nothing
/// can observe its absence), with its own nontrivial chunk-splitting
/// logic (`MLCS_MAXL`/`MLCS_MINL`). `buf.b_ml.ml_usedchunks` simply
/// stays at its `Default`-initialized `0` forever, which is a
/// different (but equally inert) sentinel from the original's `-1`
/// "disabled" state - fine since nothing reads `ml_usedchunks`/
/// `ml_chunksize` yet either.
fn ml_updatechunk(_buf: &mut BufT, _line: LinenrT, _len: i32, _updtype: i32) {}

/// Append a line after `lnum` (`lnum` can be 0) (`ml_append_int`).
///
/// Includes the "insert in front of the next block" redirect and the
/// full block-splitting algorithm (creating a new data block and,
/// transitively, splitting pointer blocks up to the root when they too
/// are full, including the "block 1 becomes the new root" special
/// case).
///
/// `len_arg`: the number of bytes of `line` to use, including its own
/// trailing NUL; `0` uses all of `line` as-is (the original instead
/// computes `strlen(line) + 1`, scanning for an embedded NUL - not
/// replicated here, since Rust slices have no implicit terminator to
/// scan for; every caller in this crate always passes an exact,
/// correct `len_arg` instead of relying on the `0` shorthand for
/// anything other than "`line` is already exactly sized").
///
/// @return `FAIL`/`OK`.
///
/// # Safety
/// `buf.b_ml.ml_mfp` must be a valid, non-null pointer to a live
/// `MemfileT` (true for any buffer `ml_open` has succeeded on).
unsafe fn ml_append_int(buf: &mut BufT, lnum: LinenrT, line: &[u8], len_arg: i32, flags: i32) -> i32 {
    if lnum > buf.b_ml.ml_line_count || buf.b_ml.ml_mfp.is_null() {
        return FAIL; // lnum out of range
    }

    // SAFETY: LOWEST_MARKED is a plain GlobalCell<i32>, matching the
    // original's own single-threaded-editor assumption.
    let lowest_marked = unsafe { LOWEST_MARKED.get_mut() };
    if *lowest_marked != 0 && *lowest_marked > lnum {
        *lowest_marked = lnum + 1;
    }

    let len = if len_arg == 0 { line.len() as i32 } else { len_arg };
    let mut space_needed = i64::from(len) + i64::from(INDEX_SIZE);

    let page_size = i64::from(unsafe { &*buf.b_ml.ml_mfp }.mf_page_size);

    // Find the data block containing the previous line. This also
    // fills the stack with the blocks from the root to the data block
    // (bumping each visited pointer block's own line count along the
    // way) and releases any locked block.
    // SAFETY: forwarded from this function's own safety doc.
    let mut hp = unsafe { ml_find_line(buf, if lnum == 0 { 1 } else { lnum }, ML_INSERT) };
    if hp.is_null() {
        return FAIL;
    }

    buf.b_ml.ml_flags &= !ML_EMPTY;

    // index for lnum in data block; if lnum == 0, got line one
    // instead, correct db_idx (careful, it is negative!)
    let mut db_idx: i32 = if lnum == 0 { -1 } else { lnum - buf.b_ml.ml_locked_low };
    // line count (number of indexes in current block) before the insertion
    let mut line_count = buf.b_ml.ml_locked_high - buf.b_ml.ml_locked_low;

    // SAFETY: hp is a valid, just-locked data block.
    let mut dp = unsafe { (*hp).bh_data.as_data_mut() };

    // If
    // - there is not enough room in the current block
    // - appending to the last line in the block
    // - not appending to the last line in the file
    // insert in front of the next block.
    if i64::from(db_free(dp)) < space_needed
        && db_idx == line_count - 1
        && lnum < buf.b_ml.ml_line_count
    {
        // Now that the line is not going to be inserted in the block
        // that we expected, the line count has to be adjusted in the
        // pointer blocks by using ml_locked_lineadd.
        buf.b_ml.ml_locked_lineadd -= 1;
        buf.b_ml.ml_locked_high -= 1;
        // SAFETY: forwarded from this function's own safety doc.
        hp = unsafe { ml_find_line(buf, lnum + 1, ML_INSERT) };
        if hp.is_null() {
            return FAIL;
        }

        db_idx = -1; // careful, it is negative!
        // get line count before the insertion
        line_count = buf.b_ml.ml_locked_high - buf.b_ml.ml_locked_low;
        debug_assert_eq!(buf.b_ml.ml_locked_low, lnum + 1, "locked_low != lnum + 1");

        // SAFETY: hp is a valid, just-locked data block.
        dp = unsafe { (*hp).bh_data.as_data_mut() };
    }

    if buf.b_prev_line_count == 0 {
        buf.b_prev_line_count = buf.b_ml.ml_line_count;
    }
    buf.b_ml.ml_line_count += 1;

    if i64::from(db_free(dp)) >= space_needed {
        // enough room in data block
        // Insert the new line in an existing data block, or in the
        // data block allocated above.
        let new_txt_start = db_txt_start(dp) - (len as u32);
        set_db_txt_start(dp, new_txt_start);
        let new_free = db_free(dp) - (space_needed as u32);
        set_db_free(dp, new_free);
        let new_line_count = db_line_count(dp) + 1;
        set_db_line_count(dp, new_line_count);

        // move the text of the lines that follow to the front and
        // adjust the indexes of the lines that follow
        if line_count > db_idx + 1 {
            // there are following lines
            // Offset is the start of the previous line. This will
            // become the character just after the new line.
            let offset: u32 = if db_idx < 0 {
                db_txt_end(dp)
            } else {
                db_index(dp, db_idx as usize) & DB_INDEX_MASK
            };
            let src_start = new_txt_start + (len as u32);
            dp.copy_within(src_start as usize..offset as usize, new_txt_start as usize);
            for i in (db_idx + 1..line_count).rev() {
                let v = db_index(dp, i as usize) - (len as u32);
                set_db_index(dp, (i + 1) as usize, v);
            }
            set_db_index(dp, (db_idx + 1) as usize, offset - (len as u32));
        } else {
            // add line at the end (which is the start of the text)
            set_db_index(dp, (db_idx + 1) as usize, new_txt_start);
        }

        // copy the text into the block
        let dest = db_index(dp, (db_idx + 1) as usize) as usize;
        dp[dest..dest + len as usize].copy_from_slice(&line[..len as usize]);
        if flags & ML_APPEND_MARK != 0 {
            let v = db_index(dp, (db_idx + 1) as usize) | DB_MARKED;
            set_db_index(dp, (db_idx + 1) as usize, v);
        }

        // Mark the block dirty.
        buf.b_ml.ml_flags |= ML_LOCKED_DIRTY;
        if flags & ML_APPEND_NEW == 0 {
            buf.b_ml.ml_flags |= ML_LOCKED_POS;
        }
    } else {
        // not enough space in data block: create a new data block and
        // copy some lines into it, then insert an entry in the pointer
        // block. If that pointer block is also full, go up another
        // block, and so on, up to the root if necessary. The line
        // counts in the pointer blocks have already been adjusted by
        // ml_find_line().
        //
        // We are going to allocate a new data block. Depending on the
        // situation it will be put to the left or right of the
        // existing block. If possible we put the new line in the left
        // block and move the lines after it to the right block.
        // Otherwise the new line is also put in the right block. This
        // method is more efficient when inserting a lot of lines at
        // one place.
        let lines_moved: i32;
        let mut data_moved: i32 = 0;
        let mut total_moved: i32 = 0;
        let in_left: bool;
        if db_idx < 0 {
            // left block is new, right block is existing
            lines_moved = 0;
            in_left = true;
            // space_needed does not change
        } else {
            // left block is existing, right block is new
            lines_moved = line_count - db_idx - 1;
            if lines_moved == 0 {
                in_left = false; // put new line in right block
                                  // space_needed does not change
            } else {
                data_moved = (db_index(dp, db_idx as usize) & DB_INDEX_MASK) as i32 - db_txt_start(dp) as i32;
                total_moved = data_moved + lines_moved * (INDEX_SIZE as i32);
                if i64::from(db_free(dp)) + i64::from(total_moved) >= space_needed {
                    in_left = true; // put new line in left block
                    space_needed = i64::from(total_moved);
                } else {
                    in_left = false; // put new line in right block
                    space_needed += i64::from(total_moved);
                }
            }
        }

        let page_count = ((space_needed + i64::from(DATA_HEADER_SIZE)) + page_size - 1) / page_size;
        // SAFETY: forwarded from this function's own safety doc.
        let mfp_new = unsafe { &mut *buf.b_ml.ml_mfp };
        // SAFETY: forwarded from this function's own safety doc.
        let mut hp_new = unsafe { ml_new_data(mfp_new, flags & ML_APPEND_NEW != 0, page_count as u32) };

        let (hp_left, hp_right, mut line_count_left, mut line_count_right);
        if db_idx < 0 {
            // left block is new
            hp_left = hp_new;
            hp_right = hp;
            line_count_left = 0;
            line_count_right = line_count;
        } else {
            // right block is new
            hp_left = hp;
            hp_right = hp_new;
            line_count_left = line_count;
            line_count_right = 0;
        }
        let bnum_left_orig = unsafe { (*hp_left).bh_bnum };
        let mut bnum_right = unsafe { (*hp_right).bh_bnum };
        let page_count_left_orig = unsafe { (*hp_left).bh_page_count } as i32;
        let mut page_count_right = unsafe { (*hp_right).bh_page_count } as i32;
        let mut bnum_left = bnum_left_orig;
        let mut page_count_left = page_count_left_orig;

        // May move the new line into the right/new block.
        if !in_left {
            // SAFETY: hp_right is a valid, just-allocated or
            // just-found data block.
            let dp_right = unsafe { (*hp_right).bh_data.as_data_mut() };
            let new_txt_start = db_txt_start(dp_right) - (len as u32);
            set_db_txt_start(dp_right, new_txt_start);
            let new_free = db_free(dp_right) - (len as u32) - INDEX_SIZE;
            set_db_free(dp_right, new_free);
            set_db_index(dp_right, 0, new_txt_start);
            if flags & ML_APPEND_MARK != 0 {
                let v = db_index(dp_right, 0) | DB_MARKED;
                set_db_index(dp_right, 0, v);
            }
            dp_right[new_txt_start as usize..new_txt_start as usize + len as usize]
                .copy_from_slice(&line[..len as usize]);
            line_count_right += 1;
        }
        // may move lines from the left/old block to the right/new one.
        if lines_moved != 0 {
            // SAFETY: hp_left is a valid data block.
            let left_txt_start = db_txt_start(unsafe { (*hp_left).bh_data.as_data() });
            let moved_bytes = {
                // SAFETY: hp_left is a valid data block.
                let dp_left_ro = unsafe { (*hp_left).bh_data.as_data() };
                dp_left_ro[left_txt_start as usize..left_txt_start as usize + data_moved as usize].to_vec()
            };
            // SAFETY: hp_right is valid.
            let dp_right = unsafe { (*hp_right).bh_data.as_data_mut() };
            let new_txt_start = db_txt_start(dp_right) - (data_moved as u32);
            set_db_txt_start(dp_right, new_txt_start);
            let new_free = db_free(dp_right) - (total_moved as u32);
            set_db_free(dp_right, new_free);
            dp_right[new_txt_start as usize..new_txt_start as usize + data_moved as usize]
                .copy_from_slice(&moved_bytes);
            let offset = new_txt_start as i32 - left_txt_start as i32;

            // update indexes in the new block
            for (to, from) in (line_count_right..).zip((db_idx + 1)..line_count_left) {
                let v = (db_index(dp, from as usize) as i32 + offset) as u32;
                set_db_index(dp_right, to as usize, v);
            }

            // SAFETY: hp_left is valid.
            let dp_left = unsafe { (*hp_left).bh_data.as_data_mut() };
            let new_left_txt_start = db_txt_start(dp_left) + (data_moved as u32);
            set_db_txt_start(dp_left, new_left_txt_start);
            let new_left_free = db_free(dp_left) + (total_moved as u32);
            set_db_free(dp_left, new_left_free);

            line_count_right += lines_moved;
            line_count_left -= lines_moved;
        }

        // May move the new line into the left (old or new) block.
        if in_left {
            // SAFETY: hp_left is valid.
            let dp_left = unsafe { (*hp_left).bh_data.as_data_mut() };
            let new_txt_start = db_txt_start(dp_left) - (len as u32);
            set_db_txt_start(dp_left, new_txt_start);
            let new_free = db_free(dp_left) - (len as u32) - INDEX_SIZE;
            set_db_free(dp_left, new_free);
            set_db_index(dp_left, line_count_left as usize, new_txt_start);
            if flags & ML_APPEND_MARK != 0 {
                let v = db_index(dp_left, line_count_left as usize) | DB_MARKED;
                set_db_index(dp_left, line_count_left as usize, v);
            }
            dp_left[new_txt_start as usize..new_txt_start as usize + len as usize]
                .copy_from_slice(&line[..len as usize]);
            line_count_left += 1;
        }

        let (mut lnum_left, mut lnum_right);
        if db_idx < 0 {
            // left block is new
            lnum_left = lnum + 1;
            lnum_right = 0;
        } else {
            // right block is new
            lnum_left = 0;
            lnum_right = if in_left { lnum + 2 } else { lnum + 1 };
        }
        // SAFETY: hp_left/hp_right are valid.
        unsafe {
            set_db_line_count((*hp_left).bh_data.as_data_mut(), i64::from(line_count_left));
            set_db_line_count((*hp_right).bh_data.as_data_mut(), i64::from(line_count_right));
        }

        // release the two data blocks
        // The new one (hp_new) already has a correct blocknumber.
        // The old one (hp, in ml_locked) gets a positive blocknumber
        // if we changed it and we are not editing a new file.
        if lines_moved != 0 || in_left {
            buf.b_ml.ml_flags |= ML_LOCKED_DIRTY;
        }
        if flags & ML_APPEND_NEW == 0 && db_idx >= 0 && in_left {
            buf.b_ml.ml_flags |= ML_LOCKED_POS;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { mf_put(&mut *buf.b_ml.ml_mfp, hp_new, true, false) };

        // flush the old data block
        // set ml_locked_lineadd to 0, because the updating of the
        // pointer blocks is done below
        let lineadd = buf.b_ml.ml_locked_lineadd;
        buf.b_ml.ml_locked_lineadd = 0;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { ml_find_line(buf, 0, ML_FLUSH) }; // flush data block

        // update pointer blocks for the new data block
        let mut stack_idx = buf.b_ml.ml_stack_top - 1;
        while stack_idx >= 0 {
            let ip = buf.b_ml.ml_stack[stack_idx as usize];
            let pb_idx = ip.ip_index;
            // SAFETY: forwarded from this function's own safety doc.
            hp = unsafe { mf_get(&mut *buf.b_ml.ml_mfp, ip.ip_bnum, 1) };
            if hp.is_null() {
                return FAIL;
            }
            // SAFETY: hp is a valid, just-locked block; must be a
            // pointer block.
            let pp_ro = unsafe { (*hp).bh_data.as_data() };
            if block_id(pp_ro) != PTR_ID {
                // (iemsg(E317: pointer block id wrong 3) omitted -
                // display-pipeline-bound, see this module's own doc
                // comment; the state change below still happens.)
                // SAFETY: forwarded.
                unsafe { mf_put(&mut *buf.b_ml.ml_mfp, hp, false, false) };
                return FAIL;
            }

            // TODO(vim): If the pointer block is full and we are
            // adding at the end try to insert in front of the next
            // block. Block not full, add one entry.
            // SAFETY: hp is valid.
            let pp = unsafe { (*hp).bh_data.as_data_mut() };
            let pb_count_max = pb_count_max(unsafe { &*buf.b_ml.ml_mfp }.mf_page_size);
            if pb_count(pp) < pb_count_max {
                let old_count = pb_count(pp);
                if pb_idx + 1 < i32::from(old_count) {
                    for i in (pb_idx + 1..i32::from(old_count)).rev() {
                        let e = pb_pointer(pp, i as usize);
                        pointer_block_set_entry(pp, (i + 1) as usize, e);
                    }
                }
                pointer_block_set_count(pp, old_count + 1);
                let old_pe_old_lnum_left = pb_pointer(pp, pb_idx as usize).pe_old_lnum;
                pointer_block_set_entry(
                    pp,
                    pb_idx as usize,
                    PointerEntry {
                        pe_bnum: bnum_left,
                        pe_line_count: line_count_left,
                        pe_old_lnum: if lnum_left != 0 { lnum_left } else { old_pe_old_lnum_left },
                        pe_page_count: page_count_left,
                    },
                );
                let old_pe_old_lnum_right = pb_pointer(pp, (pb_idx + 1) as usize).pe_old_lnum;
                pointer_block_set_entry(
                    pp,
                    (pb_idx + 1) as usize,
                    PointerEntry {
                        pe_bnum: bnum_right,
                        pe_line_count: line_count_right,
                        pe_old_lnum: if lnum_right != 0 { lnum_right } else { old_pe_old_lnum_right },
                        pe_page_count: page_count_right,
                    },
                );

                // SAFETY: forwarded from this function's own safety doc.
                unsafe { mf_put(&mut *buf.b_ml.ml_mfp, hp, true, false) };
                buf.b_ml.ml_stack_top = stack_idx + 1; // truncate stack

                if lineadd != 0 {
                    buf.b_ml.ml_stack_top -= 1;
                    // fix line count for rest of blocks in the stack
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { ml_lineadd(buf, lineadd) };
                    // fix stack itself
                    buf.b_ml.ml_stack[buf.b_ml.ml_stack_top as usize].ip_high += lineadd;
                    buf.b_ml.ml_stack_top += 1;
                }

                // We are finished, break the loop here.
                break;
            }

            // pointer block full
            //
            // split the pointer block
            // allocate a new pointer block
            // move some of the pointer into the new block
            // prepare for updating the parent block
            let mut ip_index_update = false;
            loop {
                // do this twice when splitting block 1
                // SAFETY: forwarded from this function's own safety doc.
                hp_new = unsafe { ml_new_ptr(&mut *buf.b_ml.ml_mfp) };
                if hp_new.is_null() {
                    // TODO(vim): try to fix tree
                    return FAIL;
                }

                if unsafe { (*hp).bh_bnum } != 1 {
                    break;
                }

                // if block 1 becomes full the tree is given an extra
                // level. The pointers from block 1 are moved into the
                // new block. block 1 is updated to point to the new
                // block, then continue to split the new block.
                // SAFETY: hp/hp_new are both valid pointer blocks of
                // exactly page_size bytes.
                unsafe {
                    let src = (*hp).bh_data.as_data().to_vec();
                    (*hp_new).bh_data.as_data_mut()[..src.len()].copy_from_slice(&src);
                }
                let ml_line_count = buf.b_ml.ml_line_count;
                // SAFETY: hp is valid.
                let pp_block1 = unsafe { (*hp).bh_data.as_data_mut() };
                pointer_block_set_count(pp_block1, 1);
                pointer_block_set_entry(
                    pp_block1,
                    0,
                    PointerEntry {
                        pe_bnum: unsafe { (*hp_new).bh_bnum },
                        pe_line_count: ml_line_count,
                        pe_old_lnum: 1,
                        pe_page_count: 1,
                    },
                );
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { mf_put(&mut *buf.b_ml.ml_mfp, hp, true, false) }; // release block 1
                hp = hp_new; // new block is to be split
                debug_assert_eq!(stack_idx, 0, "stack_idx should be 0");
                ip_index_update = true;
                stack_idx += 1; // do block 1 again later
            }
            if ip_index_update {
                buf.b_ml.ml_stack[stack_idx as usize - 1].ip_index = 0;
            }

            // move the pointers after the current one to the new
            // block. If there are none, the new entry will be in the
            // new block. `pb_idx` is deliberately the SAME value
            // captured at the very top of the outer loop, never
            // re-read here - matching the original exactly (the
            // `ip->ip_index = 0` assignment above only updates the
            // persistent stack bookkeeping for future lookups, it does
            // not feed back into this iteration's own `pb_idx`).
            // SAFETY: hp is valid.
            let pp = unsafe { (*hp).bh_data.as_data_mut() };
            let cur_count = i32::from(pb_count(pp));
            total_moved = cur_count - pb_idx - 1;
            if total_moved != 0 {
                for i in 0..total_moved {
                    let e = pb_pointer(pp, (pb_idx + 1 + i) as usize);
                    // SAFETY: hp_new is valid.
                    unsafe { pointer_block_set_entry((*hp_new).bh_data.as_data_mut(), i as usize, e) };
                }
                // SAFETY: hp_new is valid.
                unsafe { pointer_block_set_count((*hp_new).bh_data.as_data_mut(), total_moved as u16) };
                pointer_block_set_count(pp, (cur_count - (total_moved - 1)) as u16);
                let old_pe_old_lnum_right = pb_pointer(pp, (pb_idx + 1) as usize).pe_old_lnum;
                pointer_block_set_entry(
                    pp,
                    (pb_idx + 1) as usize,
                    PointerEntry {
                        pe_bnum: bnum_right,
                        pe_line_count: line_count_right,
                        pe_old_lnum: if lnum_right != 0 { lnum_right } else { old_pe_old_lnum_right },
                        pe_page_count: page_count_right,
                    },
                );
            } else {
                // SAFETY: hp_new is valid.
                unsafe {
                    let hp_new_buf = (*hp_new).bh_data.as_data_mut();
                    pointer_block_set_count(hp_new_buf, 1);
                    pointer_block_set_entry(
                        hp_new_buf,
                        0,
                        PointerEntry {
                            pe_bnum: bnum_right,
                            pe_line_count: line_count_right,
                            pe_old_lnum: lnum_right,
                            pe_page_count: page_count_right,
                        },
                    );
                }
            }
            let old_pe_old_lnum_left = pb_pointer(pp, pb_idx as usize).pe_old_lnum;
            pointer_block_set_entry(
                pp,
                pb_idx as usize,
                PointerEntry {
                    pe_bnum: bnum_left,
                    pe_line_count: line_count_left,
                    pe_old_lnum: if lnum_left != 0 { lnum_left } else { old_pe_old_lnum_left },
                    pe_page_count: page_count_left,
                },
            );
            lnum_left = 0;
            lnum_right = 0;

            // recompute line counts
            // SAFETY: hp_new is valid.
            let pp_new_ro = unsafe { (*hp_new).bh_data.as_data() };
            line_count_right = 0;
            for i in 0..i32::from(pb_count(pp_new_ro)) {
                line_count_right += pb_pointer(pp_new_ro, i as usize).pe_line_count;
            }
            let pp_ro = unsafe { (*hp).bh_data.as_data() };
            line_count_left = 0;
            for i in 0..i32::from(pb_count(pp_ro)) {
                line_count_left += pb_pointer(pp_ro, i as usize).pe_line_count;
            }

            bnum_left = unsafe { (*hp).bh_bnum };
            bnum_right = unsafe { (*hp_new).bh_bnum };
            page_count_left = 1;
            page_count_right = 1;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                mf_put(&mut *buf.b_ml.ml_mfp, hp, true, false);
                mf_put(&mut *buf.b_ml.ml_mfp, hp_new, true, false);
            }

            stack_idx -= 1;
        }

        // Safety check: fallen out of for loop?
        if stack_idx < 0 {
            // (iemsg(E318: Updated too many blocks?) omitted -
            // display-pipeline-bound; the state change below still
            // happens.)
            buf.b_ml.ml_stack_top = 0; // invalidate stack
        }
    }

    // The line was inserted below 'lnum'.
    ml_updatechunk(buf, lnum + 1, len, ML_CHNK_ADDLINE);
    OK
}

/// Flush any pending change and call [`ml_append_int`]
/// (`ml_append_flush`).
///
/// @return `FAIL`/`OK`.
///
/// # Safety
/// Same as [`ml_append_int`].
unsafe fn ml_append_flush(buf: &mut BufT, lnum: LinenrT, line: &[u8], len: i32, flags: i32) -> i32 {
    if lnum > buf.b_ml.ml_line_count {
        return FAIL; // lnum out of range
    }
    if buf.b_ml.ml_line_lnum != 0 {
        // This may also invoke ml_append_int().
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { ml_flush_line(buf, false) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_append_int(buf, lnum, line, len, flags) }
}

/// Append a line after `lnum` (may be 0 to insert a line in front of
/// the file) in `curbuf`, using `flags` directly (`ml_append_flags`).
///
/// Deferred: the original's `curbuf->b_ml.ml_mfp == NULL &&
/// open_buffer(...)` recovery path (`open_buffer`, `fileio.c`, not
/// translated) - this crate's callers are expected to have already
/// opened the memline via `ml_open`.
///
/// @return `FAIL`/`OK`.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`. Also see `ml_append_int`'s own safety doc.
#[must_use]
pub unsafe fn ml_append_flags(lnum: LinenrT, line: &[u8], len: i32, flags: i32) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *GLOBALS.get_mut().curbuf };
    if curbuf.b_ml.ml_mfp.is_null() {
        return FAIL;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_append_flush(curbuf, lnum, line, len, flags) }
}

/// Append a line after `lnum` (may be 0 to insert a line in front of
/// the file) in `curbuf` (`ml_append`).
///
/// `line` does not need to be allocated, but can't be another line in
/// a buffer, unlocking may make it invalid.
///
/// `newfile`: true when starting to edit a new file, meaning that
/// `pe_old_lnum` will be set for recovery.
///
/// @return `FAIL`/`OK`.
///
/// # Safety
/// Same as [`ml_append_flags`].
#[must_use]
pub unsafe fn ml_append(lnum: LinenrT, line: &[u8], len: i32, newfile: bool) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_append_flags(lnum, line, len, if newfile { ML_APPEND_NEW } else { 0 }) }
}

/// Like [`ml_append`] but for an arbitrary buffer. The buffer must
/// already have a memline (`ml_append_buf`).
///
/// @return `FAIL`/`OK`.
///
/// # Safety
/// Same as `ml_append_int` (private).
#[must_use]
pub unsafe fn ml_append_buf(buf: &mut BufT, lnum: LinenrT, line: &[u8], len: i32, newfile: bool) -> i32 {
    if buf.b_ml.ml_mfp.is_null() {
        return FAIL;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_append_flush(buf, lnum, line, len, if newfile { ML_APPEND_NEW } else { 0 }) }
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
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT`.
unsafe fn ml_flush_line(buf: &mut BufT, _noalloc: bool) {
    if buf.b_ml.ml_line_lnum == 0 || buf.b_ml.ml_mfp.is_null() {
        return; // nothing to do
    }

    if buf.b_ml.ml_flags & ML_LINE_DIRTY != 0 {
        // (The original guards against this function calling itself
        // recursively via a `static bool entered` - not needed here:
        // ml_append_int/ml_delete_int never call back into
        // ml_flush_line in this translation, so there is no recursion
        // path to guard against yet.)
        buf.flush_count += 1;

        let lnum = buf.b_ml.ml_line_lnum;
        let new_line = buf.b_ml.ml_line_ptr.clone().unwrap_or_default();
        let new_len = buf.b_ml.ml_line_textlen;

        // SAFETY: forwarded from this function's own safety doc.
        let hp = unsafe { ml_find_line(buf, lnum, ML_FIND) };
        if hp.is_null() {
            // (siemsg(E320: ...) omitted - display-pipeline-bound, see
            // this module's own doc comment.)
        } else {
            let idx = (lnum - buf.b_ml.ml_locked_low) as usize;
            // SAFETY: hp is a valid, just-locked data block.
            let dp = unsafe { (*hp).bh_data.as_data_mut() };
            let start = db_index(dp, idx) & DB_INDEX_MASK;
            let old_len: i32 = if idx == 0 {
                db_txt_end(dp) as i32 - start as i32
            } else {
                (db_index(dp, idx - 1) & DB_INDEX_MASK) as i32 - start as i32
            };
            let extra = new_len - old_len; // negative if the line got smaller

            if db_free(dp) as i32 >= extra {
                // if the new line fits in the data block, replace directly
                let count = buf.b_ml.ml_locked_high - buf.b_ml.ml_locked_low + 1;
                if extra != 0 && (idx as i32) < count - 1 {
                    // move text of the following lines
                    let txt_start = db_txt_start(dp) as i32;
                    let dst = (txt_start - extra) as usize;
                    let move_len = (start as i32 - txt_start) as usize;
                    dp.copy_within(txt_start as usize..txt_start as usize + move_len, dst);
                    // adjust pointers of this and following lines
                    for i in idx + 1..count as usize {
                        let v = (db_index(dp, i) as i32 - extra) as u32;
                        set_db_index(dp, i, v);
                    }
                }
                let v = (db_index(dp, idx) as i32 - extra) as u32;
                set_db_index(dp, idx, v);
                let new_free = (db_free(dp) as i32 - extra) as u32;
                set_db_free(dp, new_free);
                let new_txt_start = (db_txt_start(dp) as i32 - extra) as u32;
                set_db_txt_start(dp, new_txt_start);

                // copy new line into the data block
                let dest = (start as i32 - extra) as usize;
                dp[dest..dest + new_len as usize].copy_from_slice(&new_line[..new_len as usize]);
                buf.b_ml.ml_flags |= ML_LOCKED_DIRTY | ML_LOCKED_POS;
                // The else case is already covered by the insert and delete.
                if extra != 0 {
                    ml_updatechunk(buf, lnum, extra, ML_CHNK_UPDLINE);
                }
            } else {
                // Cannot do it in one data block: Delete and append.
                // Append first, because ml_delete_int() cannot delete
                // the last line in a buffer, which causes trouble for
                // a buffer that has only one line. Don't forget to
                // copy the mark!
                let marked = db_index(dp, idx) & DB_MARKED != 0;
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    ml_append_int(
                        buf,
                        lnum,
                        &new_line,
                        new_len,
                        if marked { ML_APPEND_MARK } else { 0 },
                    );
                    ml_delete_int(buf, lnum, 0);
                }
            }
        }
    }
    // (the original's `else if (ml_flags & ML_ALLOCATED) { xfree(...) }`
    // branch is unreachable here: ML_ALLOCATED is only ever set inside
    // an `#ifdef ML_GET_ALLOC_LINES` block, itself only defined for
    // AddressSanitizer builds - never the case in this crate - so
    // there is nothing to free that Rust's own `Vec<u8>` drop glue
    // doesn't already handle via `ml_line_ptr`'s ordinary ownership.)

    buf.b_ml.ml_flags &= !(ML_LINE_DIRTY | ML_ALLOCATED);
    buf.b_ml.ml_line_lnum = 0;
    buf.b_ml.ml_line_offset = 0;
}

/// Replace line `lnum`, with buffering, for the current buffer
/// (`ml_replace`).
///
/// @return `FAIL`/`OK`.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`. Also see [`ml_replace_buf_len`]'s own safety doc.
#[must_use]
pub unsafe fn ml_replace(lnum: LinenrT, line: &[u8]) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_replace_buf_len(curbuf, lnum, line) }
}

/// Replace a line for an arbitrary buffer, with buffering
/// (`ml_replace_buf`/`ml_replace_buf_len`, combined: this crate's
/// `line` is already an exact-length byte slice, so there is no
/// separate "derive the length via `strlen`" step to keep apart from
/// the length-taking variant like the original has).
///
/// Does not use `line` after calling; the caller retains ownership
/// (the original's `copy`/`noalloc` parameters existed to control
/// C-level allocation/ownership transfer of a raw `char *ml_line_ptr`,
/// which Rust's own `Vec<u8>` ownership makes moot - this always
/// copies `line` into a freshly owned `Vec<u8>`, matching the
/// original's `copy = true` behavior, its only real caller shape once
/// `line` is a borrowed slice rather than an already-allocated,
/// ownership-transferable buffer).
///
/// @return `FAIL`/`OK`.
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT`.
#[must_use]
pub unsafe fn ml_replace_buf_len(buf: &mut BufT, lnum: LinenrT, line: &[u8]) -> i32 {
    // (the original's `curbuf->b_ml.ml_mfp == NULL && open_buffer(...)`
    // recovery path is deferred - see ml_append_flags's own doc.)
    if buf.b_ml.ml_mfp.is_null() {
        return FAIL;
    }

    if buf.b_ml.ml_line_lnum != lnum {
        // another line is buffered, flush it
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { ml_flush_line(buf, false) };
    }

    if !buf.update_callbacks.is_empty() {
        // SAFETY: forwarded from this function's own safety doc.
        let old = unsafe { ml_get_buf_impl(buf, lnum, false) };
        ml_add_deleted_len_buf(buf, &old, None);
    }

    buf.b_ml.ml_line_ptr = Some(line.to_vec());
    buf.b_ml.ml_line_textlen = line.len() as i32;
    buf.b_ml.ml_line_lnum = lnum;
    buf.b_ml.ml_flags = (buf.b_ml.ml_flags | ML_LINE_DIRTY) & !ML_EMPTY;

    OK
}

/// Delete line `lnum` (`ml_delete_int`).
///
/// `flags`: `ML_DEL_MESSAGE` may give a "No lines in buffer" message
/// (omitted here - display-pipeline-bound, see this module's own doc
/// comment; the state change, `ML_EMPTY`, is still applied).
///
/// @return `FAIL`/`OK`.
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT`.
unsafe fn ml_delete_int(buf: &mut BufT, lnum: LinenrT, flags: i32) -> i32 {
    let _ = flags; // ML_DEL_MESSAGE only affects the (omitted) message

    // SAFETY: LOWEST_MARKED is a plain GlobalCell<i32>, matching the
    // original's own single-threaded-editor assumption.
    let lowest_marked = unsafe { LOWEST_MARKED.get_mut() };
    if *lowest_marked != 0 && *lowest_marked > lnum {
        *lowest_marked -= 1;
    }

    // If the file becomes empty the last line is replaced by an empty line.
    if buf.b_ml.ml_line_count == 1 {
        // (set_keep_msg(_(no_lines_msg), 0) omitted for ML_DEL_MESSAGE
        // - display-pipeline-bound.)
        // The original's C string literal "" is a real, 1-byte
        // NUL-terminated buffer (strlen 0, but occupies 1 byte) - this
        // crate's own `line` convention already expects the trailing
        // NUL to be part of the slice (matching `ml_get`'s own return
        // convention), so the equivalent empty line here is `b"\0"`
        // (1 byte), not `b""` (0 bytes).
        // SAFETY: forwarded from this function's own safety doc.
        let i = unsafe { ml_replace_buf_len(buf, 1, b"\0") };
        buf.b_ml.ml_flags |= ML_EMPTY;
        return i;
    }

    if buf.b_ml.ml_mfp.is_null() {
        return FAIL;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut hp = unsafe { ml_find_line(buf, lnum, ML_DELETE) };
    if hp.is_null() {
        return FAIL;
    }

    // number of entries in the block before the delete
    let count = buf.b_ml.ml_locked_high - buf.b_ml.ml_locked_low + 2;
    let mut idx = (lnum - buf.b_ml.ml_locked_low) as usize;

    if buf.b_prev_line_count == 0 {
        buf.b_prev_line_count = buf.b_ml.ml_line_count;
    }
    buf.b_ml.ml_line_count -= 1;

    // SAFETY: hp is a valid, just-locked data block.
    let dp = unsafe { (*hp).bh_data.as_data_mut() };
    let line_start = db_index(dp, idx) & DB_INDEX_MASK;
    let line_size: i32 = if idx == 0 {
        db_txt_end(dp) as i32 - line_start as i32
    } else {
        (db_index(dp, idx - 1) & DB_INDEX_MASK) as i32 - line_start as i32
    };

    // Line should always have a NL char internally (represented as
    // NUL), even if 'noeol' is set.
    debug_assert!(line_size >= 1);
    ml_add_deleted_len_buf(
        buf,
        &dp[line_start as usize..line_start as usize + line_size as usize],
        Some((line_size - 1) as usize),
    );

    // SAFETY: forwarded from this function's own safety doc.
    let mfp = unsafe { &mut *buf.b_ml.ml_mfp };

    // Special case: if there is only one line in the data block it
    // becomes empty. Then we have to remove the entry, pointing to
    // this data block, from the pointer block. If this pointer block
    // also becomes empty, we go up another block, and so on, up to
    // the root if necessary. The line counts in the pointer blocks
    // have already been adjusted by ml_find_line().
    if count == 1 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { mf_free(mfp, hp) }; // free the data block
        buf.b_ml.ml_locked = std::ptr::null_mut();

        let mut stack_idx = buf.b_ml.ml_stack_top - 1;
        while stack_idx >= 0 {
            buf.b_ml.ml_stack_top = 0; // stack is invalid when failing
            let ip = buf.b_ml.ml_stack[stack_idx as usize];
            idx = ip.ip_index as usize;
            // SAFETY: forwarded from this function's own safety doc.
            hp = unsafe { mf_get(mfp, ip.ip_bnum, 1) };
            if hp.is_null() {
                return FAIL;
            }
            // SAFETY: hp is a valid, just-locked block.
            let pp = unsafe { (*hp).bh_data.as_data() };
            if block_id(pp) != PTR_ID {
                // (iemsg(E317: pointer block id wrong 4) omitted -
                // display-pipeline-bound, see this module's own doc
                // comment; the state change below still happens.)
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { mf_put(mfp, hp, false, false) };
                return FAIL;
            }
            // SAFETY: hp is valid.
            let pp_mut = unsafe { (*hp).bh_data.as_data_mut() };
            let new_count = i32::from(pb_count(pp_mut)) - 1;
            pointer_block_set_count(pp_mut, new_count as u16);
            if new_count == 0 {
                // the pointer block becomes empty!
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { mf_free(mfp, hp) };
            } else {
                if new_count as usize != idx {
                    // move entries after the deleted one
                    for i in idx..new_count as usize {
                        let e = pb_pointer(pp_mut, i + 1);
                        pointer_block_set_entry(pp_mut, i, e);
                    }
                }
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { mf_put(mfp, hp, true, false) };

                buf.b_ml.ml_stack_top = stack_idx; // truncate stack
                // fix line count for rest of blocks in the stack
                if buf.b_ml.ml_locked_lineadd != 0 {
                    let lineadd = buf.b_ml.ml_locked_lineadd;
                    // SAFETY: forwarded from this function's own safety doc.
                    unsafe { ml_lineadd(buf, lineadd) };
                    buf.b_ml.ml_stack[buf.b_ml.ml_stack_top as usize].ip_high += lineadd;
                }
                buf.b_ml.ml_stack_top += 1;

                break;
            }
            stack_idx -= 1;
        }
        // (CHECK(stack_idx < 0, "deleted block 1?") is a no-op even in
        // the real source - omitted, matching this crate's established
        // policy for that macro.)
    } else {
        // delete the text by moving the next lines forwards
        // SAFETY: hp is still valid; re-borrow after
        // ml_add_deleted_len_buf (which only touched `buf`, not the
        // block's own bytes).
        let dp = unsafe { (*hp).bh_data.as_data_mut() };
        let text_start = db_txt_start(dp);
        dp.copy_within(
            text_start as usize..line_start as usize,
            text_start as usize + line_size as usize,
        );

        // delete the index by moving the next indexes backwards,
        // adjusting for the text movement
        for i in idx..count as usize - 1 {
            let v = db_index(dp, i + 1) + line_size as u32;
            set_db_index(dp, i, v);
        }

        let new_free = db_free(dp) + line_size as u32 + INDEX_SIZE;
        set_db_free(dp, new_free);
        set_db_txt_start(dp, text_start + line_size as u32);
        let new_line_count = db_line_count(dp) - 1;
        set_db_line_count(dp, new_line_count);

        // mark the block dirty and make sure it is in the file (for recovery)
        buf.b_ml.ml_flags |= ML_LOCKED_DIRTY | ML_LOCKED_POS;
    }

    ml_updatechunk(buf, lnum, line_size, ML_CHNK_DELLINE);
    OK
}

/// Delete line `lnum` in the current buffer, using `flags`
/// (`ml_delete_flags`; also serves the role of the original's plain
/// `ml_delete`, since `flags = 0` there).
///
/// @return `FAIL`/`OK`.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`. Also see `ml_delete_int`'s own safety doc.
#[must_use]
pub unsafe fn ml_delete_flags(lnum: LinenrT, flags: i32) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_flush_line(curbuf, false) };
    if lnum < 1 || lnum > curbuf.b_ml.ml_line_count {
        return FAIL;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_delete_int(curbuf, lnum, flags) }
}

/// Delete line `lnum` in the current buffer (`ml_delete`).
///
/// @return `FAIL`/`OK`.
///
/// # Safety
/// Same as [`ml_delete_flags`].
#[must_use]
pub unsafe fn ml_delete(lnum: LinenrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_delete_flags(lnum, 0) }
}

/// Delete line `lnum` in `buf` (`ml_delete_buf`).
///
/// `message`: show "--No lines in buffer--" message (omitted - see
/// this function's own safety-adjacent doc note on `ml_delete_int`).
///
/// @return `FAIL`/`OK`.
///
/// # Safety
/// Same as `ml_delete_int` (private).
#[must_use]
pub unsafe fn ml_delete_buf(buf: &mut BufT, lnum: LinenrT, message: bool) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_flush_line(buf, false) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_delete_int(buf, lnum, if message { ML_DEL_MESSAGE } else { 0 }) }
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
        buf.b_ml.ml_flags |= ML_LOCKED_DIRTY | ML_LOCKED_POS;
        // (the `#ifdef ML_GET_ALLOC_LINES` ML_ALLOCATED/ML_LINE_DIRTY
        // branch is ASan-only, not applicable here - see this
        // module's own doc comment.)
        let line = buf.b_ml.ml_line_ptr.clone().unwrap_or_default();
        ml_add_deleted_len_buf(buf, &line, None);
    }

    buf.b_ml.ml_line_ptr.clone().unwrap_or_default()
}

/// Like [`ml_get_buf`], but allow the line to be mutated in place.
/// This is very limited - generally [`ml_replace_buf_len`] should be
/// used to modify a line (`ml_get_buf_mut`).
///
/// @return a pointer to a line in the buffer.
///
/// # Safety
/// Same as [`ml_get_buf`].
#[must_use]
pub unsafe fn ml_get_buf_mut(buf: &mut BufT, lnum: LinenrT) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_get_buf_impl(buf, lnum, true) }
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

/// @return a pointer to position `pos` in `curbuf` (`ml_get_pos`).
///
/// # Safety
/// Same as [`ml_get`]. Additionally, `pos.col` must be a valid byte
/// offset within the line at `pos.lnum` (matching the original's own
/// unchecked pointer arithmetic - out-of-range values are the
/// caller's responsibility, same as every other position-taking
/// function in this crate so far).
#[must_use]
pub unsafe fn ml_get_pos(pos: &crate::pos_defs::PosT) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { ml_get(pos.lnum) };
    line[pos.col as usize..].to_vec()
}

/// @return length (excluding the NUL) of the given line in `buf`
/// (`ml_get_buf_len`).
///
/// # Safety
/// Same as [`ml_get_buf`].
#[must_use]
pub unsafe fn ml_get_buf_len(buf: &mut BufT, lnum: LinenrT) -> crate::pos_defs::ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { ml_get_buf(buf, lnum) };
    if line.first() == Some(&0) {
        return 0;
    }
    // ml_get_buf (via ml_get_buf_impl) always sets ml_line_textlen to
    // match the just-returned line's own byte length (including its
    // trailing NUL), so this mirrors the original's own
    // `buf.b_ml.ml_line_textlen - 1` exactly, rather than just
    // computing `line.len() - 1` directly - keeping the same
    // "trust the cache" shape as the original, in case a future
    // change to that cache's maintenance needs to stay in sync here
    // too.
    debug_assert!(buf.b_ml.ml_line_textlen > 0);
    buf.b_ml.ml_line_textlen - 1
}

/// @return length (excluding the NUL) of the given line in `curbuf`
/// (`ml_get_len`).
///
/// # Safety
/// Same as [`ml_get`].
#[must_use]
pub unsafe fn ml_get_len(lnum: LinenrT) -> crate::pos_defs::ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_get_buf_len(curbuf, lnum) }
}

/// @return length (excluding the NUL) of the text after position `pos`
/// in `curbuf` (`ml_get_pos_len`).
///
/// # Safety
/// Same as [`ml_get`].
#[must_use]
pub unsafe fn ml_get_pos_len(pos: &crate::pos_defs::PosT) -> crate::pos_defs::ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { ml_get_len(pos.lnum) - pos.col }
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

    /// Manually builds a minimal, working memline (dummy block 0, root
    /// pointer block at bnum 1, one data block at bnum 2 with a single
    /// empty line) with a custom `page_size` - for tests that need
    /// precise control over data/pointer block capacity to force
    /// `ml_append_int`'s block-splitting path.
    fn buf_with_custom_page_size(page_size: u32) -> BufT {
        let mut mfp = mf_open(None, 0).unwrap();
        mfp.mf_page_size = page_size;

        unsafe {
            let dummy = mf_new(&mut mfp, false, 1); // consume bnum 0
            mf_put(&mut mfp, dummy, false, false);

            let root_hp = ml_new_ptr(&mut mfp);
            assert_eq!((*root_hp).bh_bnum, 1);

            let data_hp = ml_new_data(&mut mfp, false, 1);
            assert_eq!((*data_hp).bh_bnum, 2);
            // One line already present: a lone NUL byte at the very end.
            {
                let d = (*data_hp).bh_data.as_data_mut();
                let txt_end = db_txt_end(d);
                let start = txt_end - 1;
                d[start as usize] = 0;
                set_db_index(d, 0, start);
                set_db_txt_start(d, start);
                let new_free = db_free(d) - 1 - INDEX_SIZE;
                set_db_free(d, new_free);
                set_db_line_count(d, 1);
            }
            mf_put(&mut mfp, data_hp, true, false);

            {
                let root_buf = (*root_hp).bh_data.as_data_mut();
                pointer_block_set_count(root_buf, 1);
                pointer_block_set_entry(
                    root_buf,
                    0,
                    PointerEntry { pe_bnum: 2, pe_page_count: 1, pe_old_lnum: 1, pe_line_count: 1 },
                );
            }
            mf_put(&mut mfp, root_hp, true, false);
        }

        let mut buf = BufT::default();
        buf.b_ml.ml_mfp = Box::into_raw(Box::new(mfp));
        buf.b_ml.ml_line_count = 1;
        buf
    }

    #[test]
    fn ml_append_int_splits_a_full_data_block_into_two() {
        // page_size=128: pb_count_max = (128-8)/24 = 5 (plenty of
        // headroom - this test only needs the DATA block to split, not
        // the pointer block).
        let mut buf = buf_with_custom_page_size(128);
        unsafe {
            // Fill line 1 with a line that leaves very little free
            // space in the data block (data capacity for a 1-page,
            // 128-byte block is 128-24=104 bytes; a 90-byte line
            // leaves only 104-90-INDEX_SIZE(4)=10 bytes free).
            let big_line = vec![b'a'; 89]
                .into_iter()
                .chain(std::iter::once(0u8))
                .collect::<Vec<u8>>();
            assert_eq!(big_line.len(), 90);
            assert_eq!(ml_replace_buf_len(&mut buf, 1, &big_line), OK);

            // Appending a 20-byte line needs 24 bytes (20 + INDEX_SIZE)
            // - more than the 10 bytes left, forcing a real split.
            let new_line = vec![b'b'; 19]
                .into_iter()
                .chain(std::iter::once(0u8))
                .collect::<Vec<u8>>();
            assert_eq!(new_line.len(), 20);
            let ret = ml_append_buf(&mut buf, 1, &new_line, new_line.len() as i32, false);
            assert_eq!(ret, OK);

            assert_eq!(buf.b_ml.ml_line_count, 2);
            assert_eq!(ml_get_buf(&mut buf, 1), big_line);
            assert_eq!(ml_get_buf(&mut buf, 2), new_line);

            // Root pointer block should now have 2 entries (the split
            // created a new data block, referenced as a sibling).
            let mfp = &mut *buf.b_ml.ml_mfp;
            let root_hp = mf_get(mfp, 1, 1);
            assert!(!root_hp.is_null());
            let root_buf = (*root_hp).bh_data.as_data();
            assert_eq!(pb_count(root_buf), 2);
            let e0 = pb_pointer(root_buf, 0);
            let e1 = pb_pointer(root_buf, 1);
            assert_eq!(e0.pe_line_count + e1.pe_line_count, 2);
            mf_put(mfp, root_hp, false, false);

            let mfp_owned = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp_owned, false);
        }
    }

    #[test]
    fn ml_append_int_grows_tree_through_repeated_splits_including_block_1_becomes_root() {
        // page_size=80: pb_count_max = (80-8)/24 = 3 - small enough
        // that appending enough long lines will force the root
        // pointer block to fill up and split (triggering the "block 1
        // becomes the new root" special case), but not so small that
        // the tree grows unboundedly (unlike page_size=32, where
        // pb_count_max=1 forces the tree to grow forever - verified
        // this is a genuine property of the original algorithm, not a
        // translation bug, via a throwaway debug reproduction before
        // choosing this test's page size).
        let mut buf = buf_with_custom_page_size(80);
        // Each appended line is long enough that only 1 line fits per
        // data block (80-24=56 byte capacity per page), forcing a new
        // data block - and therefore a new pointer-block entry - on
        // every single append.
        let make_line = |n: u8| -> Vec<u8> {
            let mut v = vec![b'0' + (n % 10); 40];
            v.push(0);
            v
        };
        unsafe {
            assert_eq!(ml_replace_buf_len(&mut buf, 1, &make_line(0)), OK);
            for i in 1..12 {
                let line = make_line(i as u8);
                let ret = ml_append_buf(&mut buf, i as LinenrT, &line, line.len() as i32, false);
                assert_eq!(ret, OK, "append {i} failed");
            }
            assert_eq!(buf.b_ml.ml_line_count, 12);

            // All 12 lines must still be retrievable, in order, with
            // their exact original content - proving the tree (now
            // several levels deep) is still fully navigable after
            // however many splits (including at least one "block 1
            // becomes root" event, given pb_count_max=3 and 12 data
            // blocks) occurred along the way.
            for i in 1..=12 {
                assert_eq!(ml_get_buf(&mut buf, i as LinenrT), make_line((i - 1) as u8), "line {i}");
            }

            let mfp_owned = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp_owned, false);
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
    fn ml_delete_removes_the_last_pointer_entry_when_its_block_empties() {
        // Deleting "foo" (lnum 3) is the only line in data block 3
        // (bnum 3), which is the LAST entry in the root pointer block
        // - exercises the "no shift needed" branch (new_count == idx).
        let mut buf = build_three_line_two_block_memline();
        unsafe {
            let _guard = crate::globals::global_state_test_lock();
            let prev_curbuf = GLOBALS.get_mut().curbuf;
            GLOBALS.get_mut().curbuf = &mut buf as *mut BufT;

            assert_eq!(ml_delete(3), OK);
            assert_eq!(buf.b_ml.ml_line_count, 2);
            assert_eq!(ml_get(1), b"hello\0".to_vec());
            assert_eq!(ml_get(2), b"world\0".to_vec());

            GLOBALS.get_mut().curbuf = prev_curbuf;

            // Root pointer block should now have exactly 1 entry left
            // (bnum 2, 2 lines) - bnum 3's data block was freed and
            // its pointer entry removed.
            let mfp = &mut *buf.b_ml.ml_mfp;
            let root_hp = mf_get(mfp, 1, 1);
            assert!(!root_hp.is_null());
            let root_buf = (*root_hp).bh_data.as_data();
            assert_eq!(pb_count(root_buf), 1);
            let entry0 = pb_pointer(root_buf, 0);
            assert_eq!(entry0.pe_bnum, 2);
            assert_eq!(entry0.pe_line_count, 2);
            mf_put(mfp, root_hp, false, false);
        }
        close_test_memline(buf);
    }

    #[test]
    fn ml_delete_shifts_later_pointer_entries_down_when_an_earlier_block_empties() {
        // Delete "hello" (lnum 1, normal shrink: block 2 goes from 2
        // lines to 1), then delete the new lnum 1 ("world", now the
        // ONLY line in block 2) - this empties block 2, whose pointer
        // entry is index 0 (NOT the last entry, since bnum 3/"foo"
        // still occupies index 1) - exercises the "shift entries down"
        // branch (new_count != idx).
        let mut buf = build_three_line_two_block_memline();
        unsafe {
            let _guard = crate::globals::global_state_test_lock();
            let prev_curbuf = GLOBALS.get_mut().curbuf;
            GLOBALS.get_mut().curbuf = &mut buf as *mut BufT;

            assert_eq!(ml_delete(1), OK); // delete "hello"
            assert_eq!(buf.b_ml.ml_line_count, 2);
            assert_eq!(ml_get(1), b"world\0".to_vec());
            assert_eq!(ml_get(2), b"foo\0".to_vec());

            assert_eq!(ml_delete(1), OK); // delete "world" - empties block 2
            assert_eq!(buf.b_ml.ml_line_count, 1);
            assert_eq!(ml_get(1), b"foo\0".to_vec());

            GLOBALS.get_mut().curbuf = prev_curbuf;

            // Root pointer block should now have exactly 1 entry,
            // shifted down from index 1 to index 0 (bnum 3, 1 line).
            let mfp = &mut *buf.b_ml.ml_mfp;
            let root_hp = mf_get(mfp, 1, 1);
            assert!(!root_hp.is_null());
            let root_buf = (*root_hp).bh_data.as_data();
            assert_eq!(pb_count(root_buf), 1);
            let entry0 = pb_pointer(root_buf, 0);
            assert_eq!(entry0.pe_bnum, 3);
            assert_eq!(entry0.pe_line_count, 1);
            mf_put(mfp, root_hp, false, false);
        }
        close_test_memline(buf);
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

    #[test]
    fn ml_get_buf_len_excludes_the_trailing_nul() {
        let mut buf = build_three_line_two_block_memline();
        unsafe {
            assert_eq!(ml_get_buf_len(&mut buf, 1), 5); // "hello"
            assert_eq!(ml_get_buf_len(&mut buf, 2), 5); // "world"
            assert_eq!(ml_get_buf_len(&mut buf, 3), 3); // "foo"
        }
        close_test_memline(buf);
    }

    #[test]
    fn ml_get_buf_len_is_zero_for_an_empty_line() {
        let mut buf = test_buf();
        unsafe {
            assert_eq!(ml_open(&mut buf), OK);
            assert_eq!(ml_get_buf_len(&mut buf, 1), 0);

            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn ml_get_len_and_ml_get_pos_len_via_curbuf() {
        let _guard = crate::globals::global_state_test_lock();
        let mut buf = build_three_line_two_block_memline();
        let prev_curbuf = unsafe { GLOBALS.get_mut() }.curbuf;
        unsafe { GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        unsafe {
            assert_eq!(ml_get_len(2), 5); // "world"
            let pos = crate::pos_defs::PosT { lnum: 2, col: 2, coladd: 0 };
            assert_eq!(ml_get_pos_len(&pos), 3); // "world"[2..] = "rld"
            assert_eq!(ml_get_pos(&pos), b"rld\0".to_vec());
        }

        unsafe { GLOBALS.get_mut() }.curbuf = prev_curbuf;
        close_test_memline(buf);
    }

    #[test]
    fn ml_append_replace_delete_full_roundtrip() {
        let mut buf = test_buf();
        unsafe {
            assert_eq!(ml_open(&mut buf), OK);
            // starts with 1 empty line
            assert_eq!(ml_get_buf(&mut buf, 1), vec![0u8]);

            // append "hello\0" after line 1
            assert_eq!(ml_append_buf(&mut buf, 1, b"hello\0", 6, false), OK);
            assert_eq!(buf.b_ml.ml_line_count, 2);
            assert_eq!(ml_get_buf(&mut buf, 1), vec![0u8]);
            assert_eq!(ml_get_buf(&mut buf, 2), b"hello\0".to_vec());

            // append "world\0" after line 2 (at the very end)
            assert_eq!(ml_append_buf(&mut buf, 2, b"world\0", 6, false), OK);
            assert_eq!(buf.b_ml.ml_line_count, 3);
            assert_eq!(ml_get_buf(&mut buf, 3), b"world\0".to_vec());

            // insert "middle\0" between "hello" and "world"
            assert_eq!(ml_append_buf(&mut buf, 2, b"middle\0", 7, false), OK);
            assert_eq!(buf.b_ml.ml_line_count, 4);
            assert_eq!(ml_get_buf(&mut buf, 1), vec![0u8]);
            assert_eq!(ml_get_buf(&mut buf, 2), b"hello\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 3), b"middle\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 4), b"world\0".to_vec());

            // insert a brand new first line (lnum=0)
            assert_eq!(ml_append_buf(&mut buf, 0, b"first\0", 6, false), OK);
            assert_eq!(buf.b_ml.ml_line_count, 5);
            assert_eq!(ml_get_buf(&mut buf, 1), b"first\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 2), vec![0u8]);
            assert_eq!(ml_get_buf(&mut buf, 3), b"hello\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 4), b"middle\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 5), b"world\0".to_vec());

            // replace the empty line 2 with a longer "second\0" (grows)
            assert_eq!(ml_replace_buf_len(&mut buf, 2, b"second\0"), OK);
            assert_eq!(ml_get_buf(&mut buf, 2), b"second\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 1), b"first\0".to_vec()); // unaffected
            assert_eq!(ml_get_buf(&mut buf, 3), b"hello\0".to_vec()); // unaffected

            // replace "hello\0" with a shorter "hi\0" (shrinks)
            assert_eq!(ml_replace_buf_len(&mut buf, 3, b"hi\0"), OK);
            assert_eq!(ml_get_buf(&mut buf, 3), b"hi\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 4), b"middle\0".to_vec()); // unaffected
            assert_eq!(ml_get_buf(&mut buf, 2), b"second\0".to_vec()); // unaffected

            // replace with an identical-length line (extra == 0 path)
            assert_eq!(ml_replace_buf_len(&mut buf, 4, b"MIDDLE\0"), OK);
            assert_eq!(ml_get_buf(&mut buf, 4), b"MIDDLE\0".to_vec());

            // delete "MIDDLE\0"
            assert_eq!(ml_delete_buf(&mut buf, 4, false), OK);
            assert_eq!(buf.b_ml.ml_line_count, 4);
            assert_eq!(ml_get_buf(&mut buf, 1), b"first\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 2), b"second\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 3), b"hi\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 4), b"world\0".to_vec()); // shifted up

            // delete the first line
            assert_eq!(ml_delete_buf(&mut buf, 1, false), OK);
            assert_eq!(buf.b_ml.ml_line_count, 3);
            assert_eq!(ml_get_buf(&mut buf, 1), b"second\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 2), b"hi\0".to_vec());
            assert_eq!(ml_get_buf(&mut buf, 3), b"world\0".to_vec());

            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn ml_delete_last_line_replaces_with_empty_and_sets_ml_empty() {
        let mut buf = test_buf();
        unsafe {
            assert_eq!(ml_open(&mut buf), OK);
            assert_eq!(ml_append_buf(&mut buf, 1, b"only\0", 5, false), OK);
            assert_eq!(buf.b_ml.ml_line_count, 2);

            // delete the original (now-empty) first line, then the
            // "only" line, driving the buffer down to its last line.
            assert_eq!(ml_delete_buf(&mut buf, 1, false), OK);
            assert_eq!(buf.b_ml.ml_line_count, 1);
            assert_eq!(ml_get_buf(&mut buf, 1), b"only\0".to_vec());
            assert_eq!(buf.b_ml.ml_flags & ML_EMPTY, 0);

            // deleting the last remaining line replaces it with an
            // empty line instead of removing it entirely.
            assert_eq!(ml_delete_buf(&mut buf, 1, false), OK);
            assert_eq!(buf.b_ml.ml_line_count, 1);
            assert_eq!(ml_get_buf(&mut buf, 1), vec![0u8]);
            assert_ne!(buf.b_ml.ml_flags & ML_EMPTY, 0);

            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn ml_append_out_of_range_lnum_fails() {
        let mut buf = test_buf();
        unsafe {
            assert_eq!(ml_open(&mut buf), OK);
            assert_eq!(ml_append_buf(&mut buf, 99, b"x\0", 2, false), FAIL);
            // state unchanged
            assert_eq!(buf.b_ml.ml_line_count, 1);

            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn ml_delete_out_of_range_lnum_fails() {
        let mut buf = test_buf();
        unsafe {
            assert_eq!(ml_open(&mut buf), OK);

            let _guard = crate::globals::global_state_test_lock();
            let prev_curbuf = GLOBALS.get_mut().curbuf;
            GLOBALS.get_mut().curbuf = &mut buf as *mut BufT;
            assert_eq!(ml_delete(0), FAIL);
            assert_eq!(ml_delete(99), FAIL);
            GLOBALS.get_mut().curbuf = prev_curbuf;
            assert_eq!(buf.b_ml.ml_line_count, 1);

            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn ml_append_marks_line_dirty_and_ml_get_buf_mut_returns_same_bytes() {
        let mut buf = test_buf();
        unsafe {
            assert_eq!(ml_open(&mut buf), OK);
            assert_eq!(ml_append_buf(&mut buf, 1, b"abc\0", 4, false), OK);
            let via_mut = ml_get_buf_mut(&mut buf, 2);
            assert_eq!(via_mut, b"abc\0".to_vec());
            assert_ne!(buf.b_ml.ml_flags & ML_LOCKED_DIRTY, 0);

            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn ml_replace_via_curbuf_matches_ml_get() {
        let mut buf = test_buf();
        unsafe {
            assert_eq!(ml_open(&mut buf), OK);

            let _guard = crate::globals::global_state_test_lock();
            let prev_curbuf = GLOBALS.get_mut().curbuf;
            GLOBALS.get_mut().curbuf = &mut buf as *mut BufT;
            assert_eq!(ml_replace(1, b"xyz\0"), OK);
            assert_eq!(ml_get(1), b"xyz\0".to_vec());
            GLOBALS.get_mut().curbuf = prev_curbuf;

            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }
}
