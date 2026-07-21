//! Translated from `src/nvim/buffer_defs.h` (partial - a very small,
//! self-contained subset; `buf_T`/`win_T` themselves (`struct file_buffer`/
//! `struct window_S`, each several hundred lines and referencing option
//! state, syntax state, memline internals, etc. not yet translated) are
//! substantial and deliberately deferred to a dedicated pass rather than
//! rushed).
//!
//! Translated: `bufref_T`, the `VALID_*`/`BF_*` bit-flag constants,
//! `disptick_T`, `taggy_T`.
//!
//! Deferred: everything referencing `buf_T`'s actual fields, `winopt_T`
//! (buffer/window-local options - needs `option_defs.h`), `wininfo_S`,
//! `synblock_T`/`syn_time_T` (needs syntax state), `BufUpdateCallbacks`,
//! `file_buffer`/`window_S` themselves, `tabpage_S`, `frame_S`, and all the
//! window-layout/diff/match/float-config types further down the original
//! file.

use crate::mark_defs::FmarkT;
use crate::types_defs::BufT;

/// Reference to a buffer that stores the value of `buf_free_count`.
/// `bufref_valid()` (not yet translated) only needs to check `buf` when the
/// count differs (`bufref_T`).
#[derive(Debug, Clone, Copy)]
pub struct BufrefT {
    pub br_buf: *mut BufT,
    pub br_fnum: i32,
    pub br_buf_free_count: i32,
}

/// `GETFILE_SUCCESS(x)`
#[inline]
pub fn getfile_success(x: i32) -> bool {
    x <= 0
}

/// Flags for `w_valid` (kept as plain `u8` bit-flag constants, same
/// reasoning as `HL_*`/`MarkMoveRes` elsewhere in this crate).
///
/// These are set when something in a window structure becomes invalid,
/// except when the cursor is moved. Callers must call `check_cursor_moved()`
/// (not yet translated) before testing one of the flags. These are reset
/// when that thing has been updated and is valid again.
///
/// Every function that invalidates one of these must call one of the
/// `invalidate_*` functions (not yet translated).
///
/// `w_valid` is supposed to be used only in `drawscreen.c`. From other
/// files, use the functions that set or reset the flags.
///
/// ```text
/// VALID_BOTLINE    VALID_BOTLINE_AP
///     on       on      w_botline valid
///     off      on      w_botline approximated
///     off      off     w_botline not valid
///     on       off     not possible
/// ```
pub mod w_valid {
    /// `w_wrow` (window row) is valid
    pub const VALID_WROW: u8 = 0x01;
    /// `w_wcol` (window col) is valid
    pub const VALID_WCOL: u8 = 0x02;
    /// `w_virtcol` (file col) is valid
    pub const VALID_VIRTCOL: u8 = 0x04;
    /// `w_cline_height` and `w_cline_folded` valid
    pub const VALID_CHEIGHT: u8 = 0x08;
    /// `w_cline_row` is valid
    pub const VALID_CROW: u8 = 0x10;
    /// `w_botline` and `w_empty_rows` are valid
    pub const VALID_BOTLINE: u8 = 0x20;
    /// `w_botline` is approximated
    pub const VALID_BOTLINE_AP: u8 = 0x40;
    /// `w_topline` is valid (for cursor position)
    pub const VALID_TOPLINE: u8 = 0x80;
}

/// flags for `b_flags` (kept as plain `u32` bit-flag constants).
pub mod b_flags {
    /// buffer has been recovered
    pub const BF_RECOVERED: u32 = 0x01;
    /// need to check readonly when loading file into buffer (set by `":e"`,
    /// may be reset by `":buf"`)
    pub const BF_CHECK_RO: u32 = 0x02;
    /// file has never been loaded into buffer, many variables still need
    /// to be set
    pub const BF_NEVERLOADED: u32 = 0x04;
    /// Set when file name is changed after starting to edit, reset when
    /// file is written out.
    pub const BF_NOTEDITED: u32 = 0x08;
    /// file didn't exist when editing started
    pub const BF_NEW: u32 = 0x10;
    /// Warned for `BF_NEW` and file created
    pub const BF_NEW_W: u32 = 0x20;
    /// got errors while reading the file
    pub const BF_READERR: u32 = 0x40;
    /// dummy buffer, only used internally
    pub const BF_DUMMY: u32 = 0x80;
    /// `'syntax'` option was set
    pub const BF_SYN_SET: u32 = 0x200;

    /// Mask to check for flags that prevent normal writing (`BF_WRITE_MASK`).
    pub const BF_WRITE_MASK: u32 = BF_NOTEDITED + BF_NEW + BF_READERR;
}

/// display tick type (`disptick_T`)
pub type DisptickT = u64;

/// Used to store the information about a `:tag` command (`taggy_T`).
#[derive(Debug, Clone)]
pub struct TaggyT {
    /// tag name
    pub tagname: Vec<u8>,
    /// cursor position BEFORE `":tag"`
    pub fmark: FmarkT,
    /// match number
    pub cur_match: i32,
    /// buffer number used for `cur_match`
    pub cur_fnum: i32,
    /// used with `'tagfunc'`
    pub user_data: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getfile_success_matches_c_macro() {
        assert!(getfile_success(0));
        assert!(getfile_success(-1));
        assert!(!getfile_success(1));
    }

    #[test]
    fn bf_write_mask_combines_expected_flags() {
        assert_eq!(
            b_flags::BF_WRITE_MASK,
            b_flags::BF_NOTEDITED + b_flags::BF_NEW + b_flags::BF_READERR
        );
        assert_eq!(b_flags::BF_WRITE_MASK, 0x08 + 0x10 + 0x40);
    }

    #[test]
    fn w_valid_flags_are_distinct_bits() {
        let all = [
            w_valid::VALID_WROW,
            w_valid::VALID_WCOL,
            w_valid::VALID_VIRTCOL,
            w_valid::VALID_CHEIGHT,
            w_valid::VALID_CROW,
            w_valid::VALID_BOTLINE,
            w_valid::VALID_BOTLINE_AP,
            w_valid::VALID_TOPLINE,
        ];
        let mut seen = 0u16;
        for f in all {
            assert_eq!(seen & (f as u16), 0, "flag {f:#04x} overlaps a previous one");
            seen |= f as u16;
        }
    }
}
