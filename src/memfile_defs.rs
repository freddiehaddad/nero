//! Translated from `src/nvim/memfile_defs.h`.

use crate::map::Map;

/// A block number (`blocknr_T`).
///
/// Blocks numbered from 0 upwards have been assigned a place in the
/// actual file. The block number is equal to the page number in the
/// file. The blocks with negative numbers are currently in memory only.
pub type BlocknrT = i64;

/// A block header (`bhdr_T`).
///
/// There is a block header for each previously used block in the
/// memfile.
///
/// The block may be linked in the used list OR in the free list.
///
/// The used list is a doubly linked list, most recently used block
/// first. The blocks in the used list have a block of memory allocated.
/// The free list is a single linked list, not sorted. The blocks in the
/// free list have no block of memory allocated and the contents of the
/// block in the file (if any) is irrelevant.
pub struct BhdrT {
    /// key used in hash table
    pub bh_bnum: BlocknrT,
    /// pointer to memory (for used block); in place of the original's raw
    /// `void *bh_data` - the pointee's exact type depends on which kind of
    /// block this is (data vs. pointer block, `memline.c`, not yet
    /// translated), matching C's own type-erased usage here.
    pub bh_data: *mut std::ffi::c_void,
    /// number of pages in this block
    pub bh_page_count: u32,
    /// `BH_DIRTY` or `BH_LOCKED`
    pub bh_flags: u32,
}

pub const BH_DIRTY: u32 = 1;
pub const BH_LOCKED: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MfdirtyT {
    /// no dirty blocks
    #[default]
    No = 0,
    /// there are dirty blocks
    Yes,
    /// there are dirty blocks, do not sync yet
    YesNosync,
}

/// A memory file (`memfile_T`).
pub struct MemfileT {
    /// name of the file
    pub mf_fname: Option<Vec<u8>>,
    /// idem, full path
    pub mf_ffname: Option<Vec<u8>>,
    /// file descriptor
    pub mf_fd: i32,
    /// flags used when opening this memfile
    pub mf_flags: i32,
    /// `mf_fd` was closed, retry opening
    pub mf_reopen: bool,
    /// first block header in free list
    pub mf_free_first: *mut BhdrT,

    /// The used blocks are kept in `mf_hash`, used to quickly find a
    /// block in the used list.
    pub mf_hash: Map<i64, *mut BhdrT>,

    /// When a block with a negative number is flushed to the file, it
    /// gets a positive number. Because the reference to the block is
    /// still the negative number, we remember the translation to the new
    /// positive number.
    pub mf_trans: Map<i64, i64>,

    /// highest positive block number + 1
    pub mf_blocknr_max: BlocknrT,
    /// lowest negative block number - 1
    pub mf_blocknr_min: BlocknrT,
    /// number of negative blocks numbers
    pub mf_neg_count: BlocknrT,
    /// number of pages in the file
    pub mf_infile_count: BlocknrT,
    /// number of bytes in a page
    pub mf_page_size: u32,
    pub mf_dirty: MfdirtyT,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mfdirty_default_is_no() {
        assert_eq!(MfdirtyT::default(), MfdirtyT::No);
        assert_eq!(MfdirtyT::YesNosync as i32, 2);
    }

    #[test]
    fn bh_flags_are_distinct_bits() {
        assert_ne!(BH_DIRTY, BH_LOCKED);
        assert_eq!(BH_DIRTY & BH_LOCKED, 0);
    }
}
