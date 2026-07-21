//! Translated from `src/nvim/extmark_defs.h`.

/// `bcount_t`: a byte count. TODO(bfredl, kept from the original): good
/// enough name for now.
pub type BcountT = isize;

/// `ExtmarkUndoObject`/`struct undo_object`: forward-declared only in the
/// original header; the real definition lives in `extmark.c` (not yet
/// translated - phase 3). Placeholder opaque struct until then, same
/// approach as the forward-declared types in `types_defs.rs`.
#[derive(Debug, Clone, Copy)]
pub struct ExtmarkUndoObject {
    _private: (),
}

/// `extmark_undo_vec_t`: `kvec_t(ExtmarkUndoObject)`, a growable vector.
pub type ExtmarkUndoVecT = Vec<ExtmarkUndoObject>;

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
