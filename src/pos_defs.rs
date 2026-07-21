//! Translated from `src/nvim/pos_defs.h`.

/// Line number type (`linenr_T`)
pub type LinenrT = i32;

/// Column number type (`colnr_T`)
pub type ColnrT = i32;

/// Maximal (invalid) line number (`MAXLNUM`)
pub const MAXLNUM: LinenrT = 0x7fff_ffff;

// MAXCOL used to be INT_MAX, but with 64 bit ints that results in running
// out of memory when trying to allocate a very long line.
/// Maximal column number (`MAXCOL`)
pub const MAXCOL: ColnrT = 0x7fff_ffff;

/// Minimum line number (`MINLNUM`)
pub const MINLNUM: LinenrT = 1;

/// Minimum column number (`MINCOL`)
pub const MINCOL: ColnrT = 1;

/// position in file or buffer (`pos_T`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PosT {
    /// line number
    pub lnum: LinenrT,
    /// column number
    pub col: ColnrT,
    pub coladd: ColnrT,
}

/// position in file or buffer, but without coladd (`lpos_T`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LposT {
    /// line number
    pub lnum: LinenrT,
    /// column number
    pub col: ColnrT,
}
