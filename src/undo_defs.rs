//! Translated from `src/nvim/undo_defs.h`.

use crate::eval::typval_defs::VarnumberT;
use crate::extmark_defs::ExtmarkUndoVecT;
use crate::mark_defs::FmarkT;
use crate::os::time_defs::Timestamp;
use crate::pos_defs::{ColnrT, LinenrT, PosT};
use crate::types_defs::OptInt;

/// Size in bytes of the hash used in the undo file (`UNDO_HASH_SIZE`).
pub const UNDO_HASH_SIZE: usize = 32;

/// Structure to store info about the Visual area (`visualinfo_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VisualinfoT {
    /// Start pos of last Visual.
    pub vi_start: PosT,
    /// End position of last Visual.
    pub vi_end: PosT,
    /// `Visual.mode` of last Visual.
    pub vi_mode: i32,
    /// `MAXCOL` from `w_curswant`.
    pub vi_curswant: ColnrT,
}

/// One entry in an undo block (`u_entry_T`/`struct u_entry`).
#[derive(Debug, Clone, Default)]
pub struct UEntry {
    /// number of line above undo block
    pub ue_top: LinenrT,
    /// number of line below undo block
    pub ue_bot: LinenrT,
    /// linecount when `u_save` called
    pub ue_lcount: LinenrT,
    /// lines in the undo block (in place of the original's `ue_array`
    /// C-string-array pointer + separate `ue_size` count: `Vec` already
    /// tracks its own length).
    pub ue_array: Vec<Vec<u8>>,
    // `ue_next` (pointer to next entry in list) is handled by the owning
    // container (e.g. `Vec<UEntry>`/linked structure) once `undo.c` itself
    // is translated, rather than as a raw self-pointer here.
}

/// Which of a [`UHeader`]'s linked-list pointers is meant (`uh_next`/
/// `uh_prev`/`uh_alt_next`/`uh_alt_prev` in the original): a raw pointer to
/// the next/previous header while the undo tree is in memory, or a plain
/// sequence number while reading/writing the undo file
/// (`u_read_undo()`/`u_write_undo()`, not yet translated). The original
/// stores this as a union of `u_header_T *`/`int`; translated as a safe
/// Rust enum instead of an untagged union (unlike `DecorInlineData`/`MtKey`
/// elsewhere in this crate, `UHeader` isn't densely packed in a hot-path
/// tree - it's one header per undo block - so there's no memory-layout
/// reason to avoid a safe, self-tagged enum here).
#[derive(Debug, Clone, Copy)]
pub enum UhLink {
    Ptr(*mut UHeader),
    Seq(i32),
}

impl Default for UhLink {
    fn default() -> Self {
        UhLink::Ptr(std::ptr::null_mut())
    }
}

impl UhLink {
    /// Extracts the `.ptr` field of the original's union, matching every
    /// in-memory undo-tree traversal's implicit assumption that these
    /// links hold a pointer (not yet a sequence number) - only true while
    /// reading/writing the undo file (`u_read_undo()`/`u_write_undo()`,
    /// not yet translated) does this link temporarily become a
    /// [`UhLink::Seq`]. Panics on `Seq`, matching the original's own
    /// unchecked `.ptr` access rather than silently returning null (which
    /// would hide a real logic error at the call site instead of
    /// surfacing it).
    #[must_use]
    pub fn ptr(&self) -> *mut UHeader {
        match self {
            UhLink::Ptr(p) => *p,
            UhLink::Seq(_) => panic!("UhLink::ptr() called while link holds a Seq"),
        }
    }
}

/// `u_header_T`/`struct u_header`.
#[derive(Debug, Clone, Default)]
pub struct UHeader {
    /// pointer to next undo header in list, or its sequence number
    pub uh_next: UhLink,
    /// pointer to previous header in list, or its sequence number
    pub uh_prev: UhLink,
    /// pointer to next header for alt. redo, or its sequence number
    pub uh_alt_next: UhLink,
    /// pointer to previous header for alt. redo, or its sequence number
    pub uh_alt_prev: UhLink,
    /// sequence number, higher == newer undo
    pub uh_seq: i32,
    /// used by `undo_time()`
    pub uh_walk: i32,
    /// entries, in place of the original's `uh_entry` raw linked-list-of-
    /// `u_entry_T` pointers (owned directly here, in append order).
    pub uh_entries: Vec<UEntry>,
    /// index into `uh_entries` of the entry where `ue_bot` must be set (in
    /// place of the original's raw `u_entry_T *uh_getbot_entry` pointer -
    /// an index is the natural, sound Rust equivalent of "a pointer to one
    /// specific entry already owned by `uh_entries`", since a raw pointer
    /// into a `Vec`'s backing storage could be invalidated by reallocation
    /// whenever the vec grows). `None` means "not set"/already consumed,
    /// matching the original's `NULL`.
    pub uh_getbot_entry: Option<usize>,
    /// cursor position before saving
    pub uh_cursor: PosT,
    pub uh_cursor_vcol: ColnrT,
    /// see [`uh_flags`]
    pub uh_flags: i32,
    /// marks before undo/after redo (`NMARKS`-sized in the original: see
    /// `crate::mark_defs::NMARKS`)
    pub uh_namedm: Vec<FmarkT>,
    /// info to move extmarks
    pub uh_extmark: ExtmarkUndoVecT,
    /// Visual areas before undo/after redo
    pub uh_visual: VisualinfoT,
    /// timestamp when the change was made
    pub uh_time: Timestamp,
    /// set when the file was saved after the changes in this block
    pub uh_save_nr: i32,
}

/// values for `uh_flags`.
pub mod uh_flags {
    /// `b_changed` flag before undo/after redo
    pub const CHANGED: i32 = 0x01;
    /// buffer was empty
    pub const EMPTYBUF: i32 = 0x02;
    /// buffer was reloaded
    pub const RELOAD: i32 = 0x04;
}

/// Checkpoint of buffer undo state, for "undo-invisible" speculative
/// edits: `u_checkpoint()` detaches the undo tree so subsequent edits
/// build a disposable one; `u_rollback()` reverts those edits and
/// reattaches the checkpointed tree. Used by `'inccommand'` preview
/// (`UndoCheckpoint`).
#[derive(Debug, Clone, Default)]
pub struct UndoCheckpoint {
    pub uc_oldhead: *mut UHeader,
    pub uc_newhead: *mut UHeader,
    pub uc_curhead: *mut UHeader,
    pub uc_numhead: i32,
    pub uc_synced: bool,
    pub uc_seq_last: i32,
    pub uc_save_nr_last: i32,
    pub uc_seq_cur: i32,
    pub uc_time_cur: Timestamp,
    pub uc_save_nr_cur: i32,
    /// in place of the original's raw `char *uc_line_ptr` + separate
    /// `uc_line_lnum`/`uc_line_colnr` (kept alongside, since they index
    /// into buffer state this struct doesn't own).
    pub uc_line_ptr: Option<Vec<u8>>,
    pub uc_line_lnum: LinenrT,
    pub uc_line_colnr: ColnrT,
    /// saved `'undolevels'`
    pub uc_undolevels: OptInt,
    /// saved `b:changedtick`
    pub uc_changedtick: VarnumberT,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uh_link_default_is_null_ptr_variant() {
        match UhLink::default() {
            UhLink::Ptr(p) => assert!(p.is_null()),
            UhLink::Seq(_) => panic!("default UhLink should be the null Ptr variant"),
        }
    }

    #[test]
    fn uheader_default_has_no_entries_and_no_getbot_entry() {
        let uh = UHeader::default();
        assert!(uh.uh_entries.is_empty());
        assert!(uh.uh_getbot_entry.is_none());
    }

    #[test]
    fn undo_hash_size_matches_c_constant() {
        assert_eq!(UNDO_HASH_SIZE, 32);
    }

    #[test]
    fn uh_flags_are_distinct_bits() {
        assert_eq!(uh_flags::CHANGED, 0x01);
        assert_eq!(uh_flags::EMPTYBUF, 0x02);
        assert_eq!(uh_flags::RELOAD, 0x04);
        assert_eq!(uh_flags::CHANGED & uh_flags::EMPTYBUF, 0);
    }

    #[test]
    fn undo_checkpoint_default_has_null_head_pointers() {
        let uc = UndoCheckpoint::default();
        assert!(uc.uc_oldhead.is_null());
        assert!(uc.uc_newhead.is_null());
        assert!(uc.uc_curhead.is_null());
        assert!(!uc.uc_synced);
    }
}
