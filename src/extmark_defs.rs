//! Translated from `src/nvim/extmark_defs.h`, plus the undo-object
//! types from `extmark.h` (`ExtmarkSplice`/`ExtmarkMove`/
//! `ExtmarkSavePos`/`UndoObjectType`/`ExtmarkType`). Those live here
//! rather than in a dedicated `extmark.rs`, which does not exist yet -
//! matching this crate's established "embed a header with no other
//! translated members directly, documented" precedent (`charset.h`).

/// `bcount_t`: a byte count. TODO(bfredl, kept from the original): good
/// enough name for now.
pub type BcountT = isize;

/// `ExtmarkSplice`: a region replaced by another, in rows/cols/bytes.
///
/// `old_*` describes the extent removed and `new_*` the extent
/// inserted, both measured from `start_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExtmarkSplice {
    pub start_row: i32,
    pub start_col: crate::pos_defs::ColnrT,
    pub old_row: i32,
    pub old_col: crate::pos_defs::ColnrT,
    pub new_row: i32,
    pub new_col: crate::pos_defs::ColnrT,
    pub start_byte: BcountT,
    pub old_byte: BcountT,
    pub new_byte: BcountT,
}

/// `ExtmarkMove`: marks adjusted after a `:move` operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExtmarkMove {
    pub start_row: i32,
    pub start_col: i32,
    pub extent_row: i32,
    pub extent_col: i32,
    pub new_row: i32,
    pub new_col: i32,
    pub start_byte: BcountT,
    pub extent_byte: BcountT,
    pub new_byte: BcountT,
}

/// `ExtmarkSavePos`: an extmark's position before it was updated, so
/// the update can be undone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExtmarkSavePos {
    /// raw mark id of the marktree
    pub mark: u64,
    pub old_row: i32,
    pub old_col: crate::pos_defs::ColnrT,
    pub invalidated: bool,
}

/// `UndoObjectType`: which variant an [`ExtmarkUndoObject`] holds.
///
/// Kept as its own enum for fidelity with the original, which stores
/// it as a separate discriminant field alongside the union. Rust's
/// enum carries its own discriminant, so [`ExtmarkUndoObject::kind`]
/// derives this rather than storing it twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoObjectType {
    Splice,
    Move,
    Update,
    SavePos,
    Clear,
}

/// `ExtmarkUndoObject`/`struct undo_object`: one undoable extmark
/// operation.
///
/// The original is a `UndoObjectType` tag beside a `union` of the
/// three payload structs, with `kExtmarkUpdate`/`kExtmarkClear`
/// carrying no payload at all. A Rust enum expresses exactly that
/// pairing while making the tag and payload impossible to disagree -
/// reading `data.splice` on a `kExtmarkMove` object is representable
/// in C but not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtmarkUndoObject {
    Splice(ExtmarkSplice),
    Move(ExtmarkMove),
    Update,
    SavePos(ExtmarkSavePos),
    Clear,
}

impl ExtmarkUndoObject {
    /// The original's own `type` field.
    #[must_use]
    pub fn kind(&self) -> UndoObjectType {
        match self {
            ExtmarkUndoObject::Splice(_) => UndoObjectType::Splice,
            ExtmarkUndoObject::Move(_) => UndoObjectType::Move,
            ExtmarkUndoObject::Update => UndoObjectType::Update,
            ExtmarkUndoObject::SavePos(_) => UndoObjectType::SavePos,
            ExtmarkUndoObject::Clear => UndoObjectType::Clear,
        }
    }

    /// The splice payload, or `None` for any other variant - the
    /// checked form of the original's bare `item->data.splice`.
    #[must_use]
    pub fn as_splice_mut(&mut self) -> Option<&mut ExtmarkSplice> {
        match self {
            ExtmarkUndoObject::Splice(splice) => Some(splice),
            _ => None,
        }
    }
}

/// `ExtmarkType`: which kind of decoration an extmark carries.
///
/// A bit set rather than a plain enum, so one value can describe
/// several kinds at once.
pub mod extmark_type {
    pub const NONE: u32 = 0x1;
    pub const SIGN: u32 = 0x2;
    pub const SIGN_HL: u32 = 0x4;
    pub const VIRT_TEXT: u32 = 0x8;
    pub const VIRT_LINES: u32 = 0x10;
    pub const HIGHLIGHT: u32 = 0x20;
}

/// `extmark_undo_vec_t`: `kvec_t(ExtmarkUndoObject)`, a growable vector.
pub type ExtmarkUndoVecT = Vec<ExtmarkUndoObject>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_reports_the_originals_type_tag() {
        assert_eq!(
            ExtmarkUndoObject::Splice(ExtmarkSplice::default()).kind(),
            UndoObjectType::Splice
        );
        assert_eq!(
            ExtmarkUndoObject::Move(ExtmarkMove::default()).kind(),
            UndoObjectType::Move
        );
        assert_eq!(ExtmarkUndoObject::Update.kind(), UndoObjectType::Update);
        assert_eq!(
            ExtmarkUndoObject::SavePos(ExtmarkSavePos::default()).kind(),
            UndoObjectType::SavePos
        );
        assert_eq!(ExtmarkUndoObject::Clear.kind(), UndoObjectType::Clear);
    }

    #[test]
    fn as_splice_mut_only_yields_a_splice() {
        let mut splice = ExtmarkUndoObject::Splice(ExtmarkSplice::default());
        assert!(splice.as_splice_mut().is_some());

        // The original can read `data.splice` off any object; here the
        // tag and payload cannot disagree, so this is None instead.
        let mut moved = ExtmarkUndoObject::Move(ExtmarkMove::default());
        assert!(moved.as_splice_mut().is_none());
        assert!(ExtmarkUndoObject::Update.as_splice_mut().is_none());
        assert!(ExtmarkUndoObject::Clear.as_splice_mut().is_none());
    }

    #[test]
    fn as_splice_mut_edits_in_place() {
        // The merge path in `extmark_splice_impl` mutates the last
        // recorded splice, so the borrow has to write through.
        let mut obj = ExtmarkUndoObject::Splice(ExtmarkSplice::default());
        obj.as_splice_mut().unwrap().new_col += 5;
        match obj {
            ExtmarkUndoObject::Splice(splice) => assert_eq!(splice.new_col, 5),
            _ => panic!("expected a splice"),
        }
    }

    #[test]
    fn extmark_type_flags_are_distinct_bits() {
        use extmark_type::{HIGHLIGHT, NONE, SIGN, SIGN_HL, VIRT_LINES, VIRT_TEXT};
        let all = [NONE, SIGN, SIGN_HL, VIRT_TEXT, VIRT_LINES, HIGHLIGHT];
        for (i, a) in all.iter().enumerate() {
            assert_eq!(a.count_ones(), 1, "{a:#x} is not a single bit");
            for b in &all[i + 1..] {
                assert_eq!(a & b, 0, "{a:#x} and {b:#x} overlap");
            }
        }
    }

    #[test]
    fn an_undo_vec_holds_mixed_variants() {
        let uvec: ExtmarkUndoVecT = vec![
            ExtmarkUndoObject::Splice(ExtmarkSplice::default()),
            ExtmarkUndoObject::Clear,
        ];
        assert_eq!(uvec.len(), 2);
        assert_eq!(uvec[1].kind(), UndoObjectType::Clear);
    }
}

// Undo/redo extmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtmarkOp {
    /// Extmarks shouldn't be moved.
    Noop,
    /// Operation should be reversible/undoable.
    Undo,
    /// Operation should not be reversible.
    NoUndo,
    /// Operation should be undoable, but not redoable.
    UndoNoRedo,
}
