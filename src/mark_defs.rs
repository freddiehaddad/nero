//! Translated from `src/nvim/mark_defs.h`.
//!
//! The original's `#include "mark_defs.h.inline.generated.h"` is the usual
//! cross-translation-unit inline-declaration generator with no Rust
//! equivalent (see `src/nvim/ascii_defs.rs`'s note on the same pattern).

use crate::os::time_defs::Timestamp;
use crate::pos_defs::{ColnrT, LinenrT, PosT, MAXLNUM};
use crate::types_defs::AdditionalData;

/// Flags for outcomes when moving to a mark (`MarkMoveRes`). Kept as plain
/// `u32` bit-flag constants (combined via bitwise OR), not a Rust `enum`,
/// same reasoning as `HL_*` in `highlight_defs.rs`.
pub mod mark_move_res {
    /// Successful move.
    pub const SUCCESS: u32 = 1;
    /// Failed to move.
    pub const FAILED: u32 = 2;
    /// Switched curbuf.
    pub const SWITCHED_BUF: u32 = 4;
    /// Changed the cursor col.
    pub const CHANGED_COL: u32 = 8;
    /// Changed the cursor line.
    pub const CHANGED_LINE: u32 = 16;
    /// Changed the cursor.
    pub const CHANGED_CURSOR: u32 = 32;
    /// Changed the view.
    pub const CHANGED_VIEW: u32 = 64;
}

/// Flags to configure the movement to a mark (`MarkMove`).
pub mod mark_move {
    /// Move cursor to the beginning of the line.
    pub const BEGIN_LINE: u32 = 1;
    /// Leave context mark when moving the cursor.
    pub const CONTEXT: u32 = 2;
    /// Don't leave a context mark.
    pub const NO_CONTEXT: u32 = 4;
    /// Set the mark view after moving.
    pub const SET_VIEW: u32 = 8;
    /// Special case, don't leave context mark when switching buffer.
    pub const JUMP_LIST: u32 = 16;
}

/// Options when getting a mark (`MarkGet`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkGet {
    /// Only return marks that belong to the buffer.
    BufLocal,
    /// Return all types of marks.
    All,
    /// Return all types of marks but don't resolve fnum (global marks).
    AllNoResolve,
}

/// Options when adjusting marks (`MarkAdjustMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkAdjustMode {
    /// Normal mode commands, etc.
    Normal,
    /// Changing lines from the API.
    Api,
    /// Terminal scrollback.
    Term,
}

/// Number of possible numbered global marks (`EXTRA_MARKS`).
pub const EXTRA_MARKS: i32 = b'9' as i32 - b'0' as i32 + 1;
/// Maximum possible number of letter marks (`NMARKS`).
pub const NMARKS: i32 = b'z' as i32 - b'a' as i32 + 1;
/// Total possible number of global marks (`NGLOBALMARKS`).
pub const NGLOBALMARKS: i32 = NMARKS + EXTRA_MARKS;
/// Total possible number of local marks (`NLOCALMARKS`).
///
/// That is uppercase marks plus `'"'`, `'^'` and `'.'`. There are other
/// local marks, but they are not saved in ShaDa files.
pub const NLOCALMARKS: i32 = NMARKS + 3;
/// Max value of local mark (`NMARK_LOCAL_MAX`) - index of `'~'`.
pub const NMARK_LOCAL_MAX: i32 = 126;
/// Maximum number of marks in jump list (`JUMPLISTSIZE`).
pub const JUMPLISTSIZE: i32 = 100;
/// Maximum number of tags in tag stack (`TAGSTACKSIZE`).
pub const TAGSTACKSIZE: i32 = 20;

/// Represents the view in which a mark was created (`fmarkv_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FmarkvT {
    /// Amount of lines from the mark lnum to the top of the window. Use
    /// [`MAXLNUM`] to indicate that the mark does not have a view.
    pub topline_offset: LinenrT,
    pub skipcol: ColnrT,
}

impl Default for FmarkvT {
    /// `INIT_FMARKV`
    fn default() -> Self {
        FmarkvT {
            topline_offset: MAXLNUM,
            skipcol: 0,
        }
    }
}

/// Structure defining a single local mark (`fmark_T`).
#[derive(Debug, Clone)]
pub struct FmarkT {
    /// Cursor position.
    pub mark: PosT,
    /// File number.
    pub fnum: i32,
    /// Time when this mark was last set.
    pub timestamp: Timestamp,
    /// View the mark was created on.
    pub view: FmarkvT,
    /// Additional data from ShaDa file.
    pub additional_data: Option<Box<AdditionalData>>,
}

impl Default for FmarkT {
    /// `INIT_FMARK`
    fn default() -> Self {
        FmarkT {
            mark: PosT::default(),
            fnum: 0,
            timestamp: 0,
            view: FmarkvT::default(),
            additional_data: None,
        }
    }
}

/// Structure defining an extended mark (mark with file name attached)
/// (`xfmark_T`).
#[derive(Debug, Clone, Default)]
pub struct XfmarkT {
    /// Actual mark.
    pub fmark: FmarkT,
    /// File name, used when `fnum == 0`.
    pub fname: Option<Vec<u8>>,
}

/// Returns true if position `a` is before (less than) position `b` (`lt`).
#[inline]
pub fn lt(a: PosT, b: PosT) -> bool {
    if a.lnum != b.lnum {
        a.lnum < b.lnum
    } else if a.col != b.col {
        a.col < b.col
    } else {
        a.coladd < b.coladd
    }
}

/// `equalpos`
#[inline]
pub fn equalpos(a: PosT, b: PosT) -> bool {
    a.lnum == b.lnum && a.col == b.col && a.coladd == b.coladd
}

/// `ltoreq`
#[inline]
pub fn ltoreq(a: PosT, b: PosT) -> bool {
    lt(a, b) || equalpos(a, b)
}

/// `clearpos`
#[inline]
pub fn clearpos(a: &mut PosT) {
    a.lnum = 0;
    a.col = 0;
    a.coladd = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(lnum: LinenrT, col: ColnrT, coladd: ColnrT) -> PosT {
        PosT { lnum, col, coladd }
    }

    #[test]
    fn lt_compares_lnum_then_col_then_coladd() {
        assert!(lt(pos(1, 0, 0), pos(2, 0, 0)));
        assert!(!lt(pos(2, 0, 0), pos(1, 0, 0)));
        assert!(lt(pos(1, 1, 0), pos(1, 2, 0)));
        assert!(lt(pos(1, 1, 0), pos(1, 1, 1)));
        assert!(!lt(pos(1, 1, 1), pos(1, 1, 1)));
    }

    #[test]
    fn equalpos_requires_all_three_fields_equal() {
        assert!(equalpos(pos(1, 2, 3), pos(1, 2, 3)));
        assert!(!equalpos(pos(1, 2, 3), pos(1, 2, 4)));
    }

    #[test]
    fn ltoreq_is_lt_or_equal() {
        assert!(ltoreq(pos(1, 1, 1), pos(1, 1, 1)));
        assert!(ltoreq(pos(1, 1, 0), pos(1, 1, 1)));
        assert!(!ltoreq(pos(1, 1, 2), pos(1, 1, 1)));
    }

    #[test]
    fn clearpos_zeroes_all_fields() {
        let mut p = pos(5, 6, 7);
        clearpos(&mut p);
        assert_eq!(p, pos(0, 0, 0));
    }

    #[test]
    fn fmarkv_default_matches_init_fmarkv_macro() {
        let v = FmarkvT::default();
        assert_eq!(v.topline_offset, MAXLNUM);
        assert_eq!(v.skipcol, 0);
    }

    #[test]
    fn fmark_default_matches_init_fmark_macro() {
        let m = FmarkT::default();
        assert_eq!(m.mark, PosT::default());
        assert_eq!(m.fnum, 0);
        assert_eq!(m.timestamp, 0);
        assert_eq!(m.view, FmarkvT::default());
        assert!(m.additional_data.is_none());
    }

    #[test]
    fn mark_count_constants_match_c_macros() {
        assert_eq!(EXTRA_MARKS, 10); // '0'..='9'
        assert_eq!(NMARKS, 26); // 'a'..='z'
        assert_eq!(NGLOBALMARKS, 36);
        assert_eq!(NLOCALMARKS, 29);
    }
}
