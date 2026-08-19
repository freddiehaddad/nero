//! Translated from `src/nvim/register_defs.h` (tractable core only).
//!
//! Translated: the register-index constants, `GRegFlags`, `yankreg_T`
//! (as [`YankregT`]), `yreg_mode_t` (as [`YregModeT`]), and
//! `struct block_def` (as [`BlockDefT`]). The `PUT_*` flags (`do_put`,
//! not yet translated) are deferred alongside their own
//! not-yet-translated caller.
//!
//! [`YankregT`] omits the original's own `additional_data` field
//! (ShaDa-file extra data) - this crate has no ShaDa persistence
//! subsystem translated yet, matching `eval/typval_defs.rs`'s `DictT`
//! own already-established "omit a field with no real backing
//! subsystem yet" precedent (and `search.rs`'s own `SearchPattern`,
//! which keeps its `additional_data` field only because `search.rs`'s
//! own ShaDa-adjacent callers already exist there - no such caller
//! exists for registers yet).

use crate::normal_defs::MotionType;
use crate::os::time_defs::Timestamp;

/// Registers (`enum` in the original):
/// - `0` = register for latest (unnamed) yank
/// - `1..=9` = registers `'1'` to `'9'`, for deletes
/// - `10..=35` = registers `'a'` to `'z'`
/// - `36` = delete register `'-'` (`DELETION_REGISTER`)
/// - `37` = selection register `'*'` (`STAR_REGISTER`, also
///   `NUM_SAVED_REGISTERS`: registers at or above this index are not
///   saved in a ShaDa file - not itself enforced anywhere yet, no
///   ShaDa writer exists)
/// - `38` = clipboard register `'+'` (`PLUS_REGISTER`)
pub const DELETION_REGISTER: usize = 36;
/// See [`DELETION_REGISTER`]'s own doc comment.
pub const NUM_SAVED_REGISTERS: usize = 37;
/// See [`DELETION_REGISTER`]'s own doc comment.
pub const STAR_REGISTER: usize = 37;
/// See [`DELETION_REGISTER`]'s own doc comment.
pub const PLUS_REGISTER: usize = 38;
/// Total number of register slots (`NUM_REGISTERS`).
pub const NUM_REGISTERS: usize = 39;

/// Blockwise-operator bookkeeping (`struct block_def`).
#[derive(Debug, Clone, Copy)]
pub struct BlockDefT {
    /// Extra screen columns before the first character.
    pub startspaces: i32,
    /// Extra screen columns after the last character.
    pub endspaces: i32,
    /// Number of bytes in the block's text.
    pub textlen: i32,
    /// Pointer to the first character partly or wholly in the block.
    pub textstart: *mut u8,
    /// Byte column of the block text.
    pub textcol: crate::pos_defs::ColnrT,
    /// Start virtual column of the first wholly-contained character.
    pub start_vcol: crate::pos_defs::ColnrT,
    /// Start virtual column of the first character after the block.
    pub end_vcol: crate::pos_defs::ColnrT,
    /// Whether the line is too short for the block.
    pub is_short: i32,
    /// Whether the operation began with `curswant == MAXCOL`.
    pub is_max: i32,
    /// Whether the whole block lies within one character.
    pub is_one_char: i32,
    /// Screen columns of whitespace before the block.
    pub pre_whitesp: i32,
    /// Characters of whitespace before the block.
    pub pre_whitesp_c: i32,
    /// Virtual columns occupied by the post-block character.
    pub end_char_vcols: crate::pos_defs::ColnrT,
    /// Virtual columns occupied by the pre-block character.
    pub start_char_vcols: crate::pos_defs::ColnrT,
}

impl Default for BlockDefT {
    fn default() -> Self {
        Self {
            startspaces: 0,
            endspaces: 0,
            textlen: 0,
            textstart: std::ptr::null_mut(),
            textcol: 0,
            start_vcol: 0,
            end_vcol: 0,
            is_short: 0,
            is_max: 0,
            is_one_char: 0,
            pre_whitesp: 0,
            pre_whitesp_c: 0,
            end_char_vcols: 0,
            start_char_vcols: 0,
        }
    }
}

/// Flags for `get_reg_contents` (`GRegFlags`).
pub mod greg_flags {
    /// Do not allow the expression register (`kGRegNoExpr`).
    pub const NO_EXPR: u32 = 1;
    /// Return the expression itself for the `"="` register
    /// (`kGRegExprSrc`).
    pub const EXPR_SRC: u32 = 2;
    /// Return a list rather than a joined string (`kGRegList`).
    pub const LIST: u32 = 4;
}

/// The result of `get_reg_contents` - either a joined string, or (when
/// `greg_flags::LIST` is set) a `List` of individual lines. Models the
/// original's own dual-purpose `void*` return (either a `char*` or a
/// `list_T*`, distinguished only by the caller's own `flags`) as a
/// safe Rust enum instead, matching this crate's established "C
/// void*/union becomes a safe tagged enum" precedent (e.g.
/// `Callback`, `TypvalValue`, `BhData`).
#[derive(Debug, PartialEq)]
pub enum RegContents {
    /// A joined string (`char*` in the original).
    Str(Vec<u8>),
    /// A `List` of individual lines, heap-allocated via `tv_list_alloc`
    /// (`list_T*` in the original, refcount `0`) - the caller takes
    /// ownership and must eventually store it into a `rettv` (bumping
    /// its refcount) or `tv_list_free`/`tv_list_unref` it directly.
    List(*mut crate::eval::typval_defs::ListT),
}

/// Definition of one register (`yankreg_T`).
///
/// `y_array`/`y_size` (a nullable array pointer plus its own separate
/// length in the original) are combined into one
/// `Option<Vec<Vec<u8>>>` - `None` matches `y_array == NULL` (an
/// empty/never-written register); `Vec::len()` gives `y_size` when
/// set. Each inner `Vec<u8>` is one line's own bytes (the original's
/// `String` elements).
#[derive(Debug, Clone, Default)]
pub struct YankregT {
    /// the register's lines of text, or `None` if unset/empty
    /// (`y_array` + `y_size` combined - see this struct's own doc
    /// comment)
    pub y_array: Option<Vec<Vec<u8>>>,
    /// register type (`y_type`)
    pub y_type: MotionType,
    /// register width, only valid when `y_type == MotionType::BlockWise`
    /// (`y_width`)
    pub y_width: i32,
    /// time when the register was last modified (`timestamp`)
    pub timestamp: Timestamp,
}

/// Modes for `get_yank_register` (`yreg_mode_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YregModeT {
    /// `YREG_PASTE`
    Paste,
    /// `YREG_YANK`
    Yank,
    /// `YREG_PUT`
    Put,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_def_default_matches_zero_initialized_c_storage() {
        let block = BlockDefT::default();
        assert_eq!(block.startspaces, 0);
        assert_eq!(block.endspaces, 0);
        assert_eq!(block.textlen, 0);
        assert!(block.textstart.is_null());
        assert_eq!(block.textcol, 0);
        assert_eq!(block.start_vcol, 0);
        assert_eq!(block.end_vcol, 0);
        assert_eq!(block.is_short, 0);
        assert_eq!(block.is_max, 0);
        assert_eq!(block.is_one_char, 0);
        assert_eq!(block.pre_whitesp, 0);
        assert_eq!(block.pre_whitesp_c, 0);
        assert_eq!(block.end_char_vcols, 0);
        assert_eq!(block.start_char_vcols, 0);
    }
}
