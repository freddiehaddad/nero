//! Translated from `src/nvim/memory_defs.h`.

/// `ArenaMem`: an opaque handle to a linked list of consumed arena blocks.
/// `struct consumed_blk { struct consumed_blk *prev; }`. Kept as a raw
/// pointer (not `Option<Box<ConsumedBlk>>`) because `src/nvim/memory.c`'s
/// arena allocator (not yet translated - deferred, see `src/memory.rs`)
/// manages these blocks manually with pointer arithmetic (writing a
/// `ConsumedBlk` header into memory that is otherwise being used as a raw
/// byte arena), which doesn't fit Rust's ownership model without `unsafe`
/// - exactly as it doesn't fit C's type system without manual care either.
#[repr(C)]
pub struct ConsumedBlk {
    pub prev: *mut ConsumedBlk,
}

pub type ArenaMem = *mut ConsumedBlk;

/// `ARENA_BLOCK_SIZE`
pub const ARENA_BLOCK_SIZE: usize = 4096;

#[repr(C)]
pub struct Arena {
    pub cur_blk: *mut std::os::raw::c_char,
    pub pos: usize,
    pub size: usize,
}

impl Default for Arena {
    /// `ARENA_EMPTY`
    fn default() -> Self {
        Arena {
            cur_blk: std::ptr::null_mut(),
            pos: 0,
            size: 0,
        }
    }
}
