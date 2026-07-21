//! Translated from `src/nvim/memline_defs.h`.

use crate::memfile_defs::{BhdrT, BlocknrT, MemfileT};
use crate::pos_defs::{ColnrT, LinenrT};

/// When searching for a specific line, we remember what blocks in the
/// tree are the branches leading to that block. This is stored in
/// `ml_stack`. Each entry is a pointer to info in a block (may be data
/// block or pointer block) (`infoptr_T`: block/index pair).
#[derive(Debug, Clone, Copy, Default)]
pub struct InfoptrT {
    /// block number
    pub ip_bnum: BlocknrT,
    /// lowest lnum in this block
    pub ip_low: LinenrT,
    /// highest lnum in this block
    pub ip_high: LinenrT,
    /// index for block with current lnum
    pub ip_index: i32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChunksizeT {
    pub mlcs_numlines: i32,
    pub mlcs_totalsize: i32,
}

// Flags when calling `ml_updatechunk()`.
pub const ML_CHNK_ADDLINE: i32 = 1;
pub const ML_CHNK_DELLINE: i32 = 2;
pub const ML_CHNK_UPDLINE: i32 = 3;

// Flags for `MemlineT.ml_flags`.
/// empty buffer
pub const ML_EMPTY: i32 = 0x01;
/// cached line was changed and allocated
pub const ML_LINE_DIRTY: i32 = 0x02;
/// `ml_locked` was changed
pub const ML_LOCKED_DIRTY: i32 = 0x04;
/// `ml_locked` needs positive block number
pub const ML_LOCKED_POS: i32 = 0x08;
/// `ml_line_ptr` is an allocated copy
pub const ML_ALLOCATED: i32 = 0x10;

/// memline structure: the contents of a buffer (`memline_T`).
///
/// Essentially a tree with a branch factor of 128. Lines are stored at
/// leaf nodes. Nodes are stored on `ml_mfp` (`memfile_T`): pointer_block
/// (internal nodes), data_block (leaf nodes).
///
/// Memline also has "chunks" of 800 lines that are separate from the
/// 128-tree structure, primarily used to speed up `line2byte()` and
/// `byte2line()`.
///
/// Motivation: if you have a file that is 10000 lines long, and you
/// insert a line at linenr 1000, you don't want to move 9000 lines in
/// memory. With this structure it is roughly `N * 128` pointer moves,
/// where `N` is the height (typically 1-3).
///
/// Pointer fields (`ml_mfp`, `ml_stack`, `ml_locked`) stay raw pointers,
/// matching this crate's established convention for not-yet-translated
/// owning subsystems (`memfile.c`/`memline.c` themselves aren't
/// translated yet - only their plain-data `_defs.h` type declarations).
pub struct MemlineT {
    /// number of lines in the buffer
    pub ml_line_count: LinenrT,

    /// pointer to associated memfile
    pub ml_mfp: *mut MemfileT,

    /// stack of pointer blocks (array of IPTRs), in place of the
    /// original's raw `infoptr_T *ml_stack` pointer to a heap-allocated
    /// array of `ml_stack_size` entries (of which `ml_stack_top` are
    /// currently in use).
    pub ml_stack: Vec<InfoptrT>,
    /// current top of `ml_stack`
    pub ml_stack_top: i32,
    /// total number of entries in `ml_stack`
    pub ml_stack_size: i32,

    pub ml_flags: i32,

    // colnr_T ml_line_len; (kept as a comment in the original too)
    /// length of the cached line + NUL
    pub ml_line_textlen: ColnrT,
    /// line number of cached line, 0 if not valid
    pub ml_line_lnum: LinenrT,
    /// pointer to cached line (in place of the original's raw `char
    /// *ml_line_ptr` - see `ML_ALLOCATED` in `ml_flags` for whether this
    /// is an allocated copy or a borrow into a block, mirrored here as an
    /// owned buffer either way since Rust has no direct equivalent of "a
    /// pointer that's sometimes borrowed, sometimes owned, distinguished
    /// by a flag" without an enum - deferred until `memline.c` itself is
    /// translated and this can be modeled precisely).
    pub ml_line_ptr: Option<Vec<u8>>,
    /// cached byte offset of `ml_line_lnum`
    pub ml_line_offset: usize,
    /// fileformat of cached line
    pub ml_line_offset_ff: i32,

    /// block used by last `ml_get`
    pub ml_locked: *mut BhdrT,
    /// first line in `ml_locked`
    pub ml_locked_low: LinenrT,
    /// last line in `ml_locked`
    pub ml_locked_high: LinenrT,
    /// number of lines inserted in `ml_locked`
    pub ml_locked_lineadd: i32,
    /// in place of the original's raw `chunksize_T *ml_chunksize` pointer
    /// to a heap-allocated array of `ml_numchunks` entries.
    pub ml_chunksize: Vec<ChunksizeT>,
    pub ml_numchunks: i32,
    pub ml_usedchunks: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ml_flags_are_distinct_bits() {
        let all = [ML_EMPTY, ML_LINE_DIRTY, ML_LOCKED_DIRTY, ML_LOCKED_POS, ML_ALLOCATED];
        let mut seen = 0;
        for f in all {
            assert_eq!(seen & f, 0, "flag {f:#04x} overlaps a previous one");
            seen |= f;
        }
    }

    #[test]
    fn infoptr_default_is_zeroed() {
        let ip = InfoptrT::default();
        assert_eq!(ip.ip_bnum, 0);
        assert_eq!(ip.ip_index, 0);
    }
}
