//! Translated from `src/nvim/memfile_defs.h`.

use crate::map::Map;

/// A block number (`blocknr_T`).
///
/// Blocks numbered from 0 upwards have been assigned a place in the
/// actual file. The block number is equal to the page number in the
/// file. The blocks with negative numbers are currently in memory only.
pub type BlocknrT = i64;

/// The payload of a [`BhdrT`] (`bh_data` in the original: a `void *`
/// that is reinterpreted depending on the block's state).
///
/// The original's `bh_data` is a single `void *` field serving two
/// entirely different purposes depending on which list the block is on
/// (per the struct's own doc comment): for a block on the *used* list,
/// it points at the block's actual allocated data; for a block on the
/// *free* list ("no block of memory allocated"), `mf_ins_free`/
/// `mf_rem_free` (`memfile.c`) instead store the free list's own `next`
/// pointer directly in this same field (`hp->bh_data =
/// mfp->mf_free_first;`). Translated as a proper tagged Rust enum -
/// each variant directly carries its own payload - rather than
/// replicating the untyped-pointer-reinterpretation trick, same
/// reasoning as `Callback`/`UhLink`/`EsInfo` elsewhere in this crate:
/// block headers are individually heap-allocated (not a hot, densely
/// packed array), so there is no memory-layout reason to keep the
/// original's type-erased `void *` here.
pub enum BhData {
    /// Allocated data buffer (block is on the used list / newly
    /// allocated).
    Data(Vec<u8>),
    /// Next block in the free list (block is on the free list, no data
    /// allocated).
    FreeNext(*mut BhdrT),
}

impl BhData {
    /// Returns the data buffer, panicking if this is a
    /// [`BhData::FreeNext`] - mirrors the original's implicit assumption
    /// (dereferencing `bh_data` as a data pointer) that callers only do
    /// this for used-list blocks.
    #[must_use]
    pub fn as_data(&self) -> &[u8] {
        match self {
            BhData::Data(v) => v,
            BhData::FreeNext(_) => panic!("BhData::as_data called on a free-list entry"),
        }
    }

    /// Mutable version of [`BhData::as_data`].
    #[must_use]
    pub fn as_data_mut(&mut self) -> &mut Vec<u8> {
        match self {
            BhData::Data(v) => v,
            BhData::FreeNext(_) => panic!("BhData::as_data_mut called on a free-list entry"),
        }
    }
}

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
    /// pointer to memory (for used block), or the free-list `next`
    /// pointer (for a free block) - see [`BhData`].
    pub bh_data: BhData,
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
    /// open file handle, or `None` for memory-only (`mf_fd`).
    ///
    /// The original stores a raw `int` file descriptor (`-1` meaning
    /// "no file"); this crate uses `Option<std::fs::File>` instead -
    /// the idiomatic Rust equivalent of the same resource (an owned,
    /// automatically-closed file handle), matching this crate's
    /// established pattern of using a properly-typed Rust construct
    /// instead of a C primitive when it's a strict improvement with no
    /// loss of real behavior (e.g. `Option<Vec<u8>>` for `char*`,
    /// `BhData`'s tagged enum for `void*`).
    pub mf_fd: Option<std::fs::File>,
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

    #[test]
    fn bh_data_as_data_reads_and_writes_through() {
        let mut d = BhData::Data(vec![1, 2, 3]);
        assert_eq!(d.as_data(), &[1, 2, 3]);
        d.as_data_mut().push(4);
        assert_eq!(d.as_data(), &[1, 2, 3, 4]);
    }

    #[test]
    fn bh_data_free_next_holds_a_pointer() {
        let mut other = BhdrT {
            bh_bnum: 0,
            bh_data: BhData::Data(Vec::new()),
            bh_page_count: 0,
            bh_flags: 0,
        };
        let d = BhData::FreeNext(&mut other as *mut BhdrT);
        match d {
            BhData::FreeNext(p) => assert!(!p.is_null()),
            BhData::Data(_) => panic!("expected FreeNext variant"),
        }
    }

    #[test]
    #[should_panic(expected = "as_data called on a free-list entry")]
    fn bh_data_as_data_panics_on_free_next() {
        let d = BhData::FreeNext(std::ptr::null_mut());
        let _ = d.as_data();
    }
}
