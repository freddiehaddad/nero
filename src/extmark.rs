//! Extmark tracking (`extmark.c`).
//!
//! Extmarks are positions in a buffer that follow text as it is
//! edited. They are stored in the buffer's own marktree
//! (`BufT::b_marktree`, already translated in `marktree.rs`); this
//! file holds the editing-side glue that keeps them correct across
//! splices, and records enough information to undo those adjustments.
//!
//! Translated so far: [`extmark_splice_delete`],
//! [`extmark_splice_impl`], [`extmark_splice`],
//! [`extmark_splice_cols`].
//!
//! Deferred, each for a specific reason:
//!
//! `extmark_set`/`extmark_del`/`extmark_clear`/`extmark_get` all call
//! into `decoration.c`'s `decor_*` family on nearly every real path,
//! which is tied to the drawing/rendering pipeline (phase 9).
//!
//! `extmark_splice_impl`/`extmark_splice`/`extmark_splice_cols` need
//! `buf_signcols_count_range` (`decoration.c`) in addition to this
//! file's own pieces.
//!
//! `extmark_apply_undo`/`extmark_adjust`/`extmark_move_region` need
//! the `extmark_set`/`extmark_del` family above.

use crate::buffer_defs::BufT;
use crate::decoration::buf_signcols_count_range;
use crate::extmark_defs::{
    BcountT, ExtmarkOp, ExtmarkSavePos, ExtmarkSplice, ExtmarkUndoObject, ExtmarkUndoVecT,
};
use crate::marktree::{
    marktree_get_altpos, marktree_itr_current, marktree_itr_get, marktree_itr_next, marktree_splice,
    mt_end, mt_invalid, mt_invalidate, mt_lookup_key, mt_no_undo, mt_paired, mt_right,
};
use crate::marktree_defs::MarkTreeIter;
use crate::pos_defs::ColnrT;
use crate::types_defs::TriState;

/// Invalidate extmarks between a range and copy them to the undo
/// header (`extmark_splice_delete`).
///
/// Copying is useful when the operation cannot simply be reversed.
/// This does nothing on redo, and enforces the correct position when
/// undoing.
///
/// # Scope
///
/// The traversal itself, the left/right-gravity "no need to copy"
/// decisions, and the push of an [`ExtmarkSavePos`] record onto the
/// undo header are all translated in full.
///
/// The nested "actually invalidate this mark" branch is
/// `unimplemented!()`: it needs `extmark_del`, `mt_itr_rawkey` and
/// `buf_decor_remove`, none of which are translated (the first two
/// belong to this file's own deferred `extmark_set`/`_del` family,
/// the third to `decoration.c`). Nothing translated can create an
/// extmark yet, so the loop never runs a single iteration today and
/// that branch is unreachable.
///
/// # Safety
///
/// `buf.b_marktree` must be a well-formed marktree, as maintained by
/// `marktree.rs`'s own operations.
#[allow(clippy::too_many_arguments)]
pub unsafe fn extmark_splice_delete(
    buf: &mut BufT,
    l_row: i32,
    l_col: ColnrT,
    u_row: i32,
    u_col: ColnrT,
    mut uvp: Option<&mut ExtmarkUndoVecT>,
    only_copy: bool,
    op: ExtmarkOp,
) {
    let mut itr = MarkTreeIter::default();

    marktree_itr_get(&buf.b_marktree, l_row, l_col, &mut itr);
    loop {
        let mark = marktree_itr_current(&itr);
        if mark.pos.row < 0 || mark.pos.row > u_row {
            break;
        }

        // No need to copy left gravity marks at the beginning of the
        // range, and right gravity marks at the end of the range,
        // unless invalidated.
        let mut copy = true;
        if mark.pos.row == l_row && mark.pos.col - i32::from(!mt_right(&mark)) < l_col {
            copy = false;
        } else if mark.pos.row == u_row {
            if mark.pos.col > u_col + 1 {
                break;
            } else if mark.pos.col + i32::from(mt_right(&mark)) > u_col {
                copy = false;
            }
        }

        let invalidated = false;
        if !only_copy && !mt_invalid(&mark) && mt_invalidate(&mark) && !mt_end(&mark) {
            let mut enditr = itr;
            let endpos = marktree_get_altpos(&buf.b_marktree, &mark, Some(&mut enditr));
            // Invalidate unpaired marks in deleted lines, and paired
            // marks whose entire range has been deleted.
            if (!mt_paired(&mark) && mark.pos.row < u_row)
                || (mt_paired(&mark)
                    && (mark.pos.row > l_row || (mark.pos.row == l_row && mark.pos.col >= l_col))
                    && (endpos.row < u_row || (endpos.row == u_row && endpos.col <= u_col)))
            {
                unimplemented!(
                    "extmark invalidation needs extmark_del/mt_itr_rawkey/buf_decor_remove, \
                     not yet translated; unreachable while no extmark can be created"
                );
            }
        }

        // Push the mark onto the undo header.
        if copy && (only_copy || (uvp.is_some() && op == ExtmarkOp::Undo && !mt_no_undo(&mark))) {
            let pos = ExtmarkSavePos {
                mark: mt_lookup_key(&mark),
                invalidated,
                old_row: mark.pos.row,
                old_col: mark.pos.col,
            };
            if let Some(uv) = uvp.as_deref_mut() {
                uv.push(ExtmarkUndoObject::SavePos(pos));
            }
        }

        marktree_itr_next(&buf.b_marktree, &mut itr);
    }
}

/// Adjust extmarks for a text edit, given the edit's absolute byte
/// offset (`extmark_splice_impl`).
///
/// The edit replaces the `old_row`/`old_col`-sized extent starting at
/// `start_row`/`start_col` with a `new_row`/`new_col`-sized one.
/// `undo` selects whether the adjustment is recorded so it can be
/// reversed later.
///
/// # Scope
///
/// Translated in full: the buffer-update notification, the
/// copy-and-invalidate pass over deleted marks, the sign-count
/// bracketing around the marktree splice, the splice itself, and the
/// undo bookkeeping including the original's small same-line
/// insert/delete merge optimisation.
///
/// # Safety
///
/// `buf` must be a well-formed buffer with a valid marktree and undo
/// state, as maintained by `marktree.rs` and `undo.rs`.
#[allow(clippy::too_many_arguments)]
pub unsafe fn extmark_splice_impl(
    buf: &mut BufT,
    start_row: i32,
    start_col: ColnrT,
    start_byte: BcountT,
    old_row: i32,
    old_col: ColnrT,
    old_byte: BcountT,
    new_row: i32,
    new_col: ColnrT,
    new_byte: BcountT,
    undo: ExtmarkOp,
) {
    buf.deleted_bytes2 = 0;
    crate::buffer_updates::buf_updates_send_splice(
        buf, start_row, start_col, start_byte, old_row, old_col, old_byte, new_row, new_col,
        new_byte,
    );

    if old_row > 0 || old_col > 0 {
        // Copy and invalidate marks that would be affected by the delete.
        let end_row = start_row + old_row;
        let end_col = if old_row != 0 { 0 } else { start_col } + old_col;
        // SAFETY: forwarded from this function's own safety doc.
        let uhp = unsafe { crate::undo::u_force_get_undo_header(buf) };
        // SAFETY: `uhp` is either null or a live header allocated by
        // `u_force_get_undo_header`, living in its own allocation
        // rather than inside `buf`.
        let uvp: Option<&mut ExtmarkUndoVecT> = if uhp.is_null() {
            None
        } else {
            Some(unsafe { &mut (*uhp).uh_extmark })
        };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            extmark_splice_delete(buf, start_row, start_col, end_row, end_col, uvp, false, undo);
        }
    }

    // Remove signs inside the edited region from `b_signcols.count`,
    // and add them back after splicing.
    if old_row > 0 || new_row > 0 {
        let count = if buf.b_prev_line_count > 0 {
            buf.b_prev_line_count
        } else {
            buf.b_ml.ml_line_count
        };
        buf_signcols_count_range(
            buf,
            start_row,
            (count - 1).min(start_row + old_row),
            0,
            TriState::True,
        );
        buf.b_prev_line_count = 0;
    }

    marktree_splice(
        &mut buf.b_marktree,
        start_row,
        start_col,
        old_row,
        old_col,
        new_row,
        new_col,
    );

    if old_row > 0 || new_row > 0 {
        let row2 = (buf.b_ml.ml_line_count - 1).min(start_row + new_row);
        buf_signcols_count_range(buf, start_row, row2, 0, TriState::None);
    }

    if undo == ExtmarkOp::Undo {
        // SAFETY: forwarded from this function's own safety doc.
        let uhp = unsafe { crate::undo::u_force_get_undo_header(buf) };
        if uhp.is_null() {
            return;
        }

        // Merge small (within-line) inserts with each other, and
        // small deletes with each other. This mirrors the original's
        // own deliberately rudimentary merge.
        let mut merged = false;
        // SAFETY: as above - `uhp` is a live, separately-allocated
        // header.
        let uh_extmark = unsafe { &mut (*uhp).uh_extmark };
        if old_row == 0
            && new_row == 0
            && !uh_extmark.is_empty()
            && let Some(splice) = uh_extmark.last_mut().and_then(ExtmarkUndoObject::as_splice_mut)
            && splice.start_row == start_row
            && splice.old_row == 0
            && splice.new_row == 0
        {
            if old_col == 0
                && start_col >= splice.start_col
                && start_col <= splice.start_col + splice.new_col
            {
                splice.new_col += new_col;
                splice.new_byte += new_byte;
                merged = true;
            } else if new_col == 0 && start_col == splice.start_col + splice.new_col {
                splice.old_col += old_col;
                splice.old_byte += old_byte;
                merged = true;
            } else if new_col == 0 && start_col + old_col == splice.start_col {
                splice.start_col = start_col;
                splice.start_byte = start_byte;
                splice.old_col += old_col;
                splice.old_byte += old_byte;
                merged = true;
            }
        }

        if !merged {
            uh_extmark.push(ExtmarkUndoObject::Splice(ExtmarkSplice {
                start_row,
                start_col,
                start_byte,
                old_row,
                old_col,
                old_byte,
                new_row,
                new_col,
                new_byte,
            }));
        }
    }
}

/// Adjust extmarks for a text edit, computing the edit's absolute
/// byte offset from the buffer itself (`extmark_splice`).
///
/// # Safety
///
/// Same as [`extmark_splice_impl`].
#[allow(clippy::too_many_arguments)]
pub unsafe fn extmark_splice(
    buf: &mut BufT,
    start_row: i32,
    start_col: ColnrT,
    old_row: i32,
    old_col: ColnrT,
    old_byte: BcountT,
    new_row: i32,
    new_col: ColnrT,
    new_byte: BcountT,
    undo: ExtmarkOp,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut offset = unsafe { crate::memline::ml_find_line_or_offset(buf, start_row + 1, None, true) };

    // On an empty buffer, editing the first line leaves that line
    // buffered, so the offset comes back negative. The buffer is not
    // really empty - the buffered line simply has not been flushed
    // (and should not be) yet - so this call is valid, just an edge
    // case.
    if offset < 0 && buf.b_ml.ml_chunksize.is_empty() {
        offset = 0;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        extmark_splice_impl(
            buf,
            start_row,
            start_col,
            offset as BcountT + start_col as BcountT,
            old_row,
            old_col,
            old_byte,
            new_row,
            new_col,
            new_byte,
            undo,
        );
    }
}

/// Adjust extmarks for an edit confined to a single line
/// (`extmark_splice_cols`).
///
/// # Safety
///
/// Same as [`extmark_splice_impl`].
pub unsafe fn extmark_splice_cols(
    buf: &mut BufT,
    start_row: i32,
    start_col: ColnrT,
    old_col: ColnrT,
    new_col: ColnrT,
    undo: ExtmarkOp,
) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        extmark_splice(
            buf,
            start_row,
            start_col,
            0,
            old_col,
            old_col as BcountT,
            0,
            new_col,
            new_col as BcountT,
            undo,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::undo_defs::UHeader;

    fn new_header() -> *mut UHeader {
        Box::into_raw(Box::new(UHeader::default()))
    }

    /// Build a buffer whose undo header already exists, so
    /// `u_force_get_undo_header` hands back a real one.
    fn buf_with_header(uhp: *mut UHeader) -> BufT {
        BufT {
            b_u_curhead: uhp,
            ..Default::default()
        }
    }

    #[test]
    fn splice_impl_records_an_undo_splice() {
        let _lock = crate::globals::global_state_test_lock();
        let uhp = new_header();
        let mut buf = buf_with_header(uhp);

        unsafe {
            extmark_splice_impl(&mut buf, 3, 5, 40, 0, 0, 0, 0, 4, 4, ExtmarkOp::Undo);
        }

        let recorded = unsafe { &(*uhp).uh_extmark };
        assert_eq!(recorded.len(), 1);
        match &recorded[0] {
            ExtmarkUndoObject::Splice(s) => {
                assert_eq!(s.start_row, 3);
                assert_eq!(s.start_col, 5);
                assert_eq!(s.start_byte, 40);
                assert_eq!(s.new_col, 4);
                assert_eq!(s.new_byte, 4);
                assert_eq!(s.old_col, 0);
            }
            other => panic!("expected a splice record, got {other:?}"),
        }
        assert_eq!(buf.deleted_bytes2, 0);

        drop(unsafe { Box::from_raw(uhp) });
    }

    #[test]
    fn splice_impl_records_nothing_for_a_no_undo_op() {
        let _lock = crate::globals::global_state_test_lock();
        let uhp = new_header();
        let mut buf = buf_with_header(uhp);

        unsafe {
            extmark_splice_impl(&mut buf, 0, 0, 0, 0, 0, 0, 0, 3, 3, ExtmarkOp::NoUndo);
        }

        assert!(unsafe { &(*uhp).uh_extmark }.is_empty());
        drop(unsafe { Box::from_raw(uhp) });
    }

    /// Two consecutive same-line inserts collapse into one record,
    /// growing the existing entry rather than appending a new one.
    #[test]
    fn splice_impl_merges_a_following_same_line_insert() {
        let _lock = crate::globals::global_state_test_lock();
        let uhp = new_header();
        let mut buf = buf_with_header(uhp);

        unsafe {
            extmark_splice_impl(&mut buf, 1, 2, 10, 0, 0, 0, 0, 3, 3, ExtmarkOp::Undo);
            // Typing directly after the first insert.
            extmark_splice_impl(&mut buf, 1, 5, 13, 0, 0, 0, 0, 2, 2, ExtmarkOp::Undo);
        }

        let recorded = unsafe { &(*uhp).uh_extmark };
        assert_eq!(recorded.len(), 1, "the second insert should have merged");
        match &recorded[0] {
            ExtmarkUndoObject::Splice(s) => {
                assert_eq!(s.start_col, 2);
                assert_eq!(s.new_col, 5);
                assert_eq!(s.new_byte, 5);
            }
            other => panic!("expected a splice record, got {other:?}"),
        }

        drop(unsafe { Box::from_raw(uhp) });
    }

    /// A delete immediately after the merged insert extends the same
    /// record's `old_col`/`old_byte` instead of appending.
    #[test]
    fn splice_impl_merges_a_following_same_line_delete() {
        let _lock = crate::globals::global_state_test_lock();
        let uhp = new_header();
        let mut buf = buf_with_header(uhp);

        unsafe {
            extmark_splice_impl(&mut buf, 0, 4, 4, 0, 0, 0, 0, 3, 3, ExtmarkOp::Undo);
            // Deleting at exactly the end of the inserted text.
            extmark_splice_impl(&mut buf, 0, 7, 7, 0, 2, 2, 0, 0, 0, ExtmarkOp::Undo);
        }

        let recorded = unsafe { &(*uhp).uh_extmark };
        assert_eq!(recorded.len(), 1, "the delete should have merged");
        match &recorded[0] {
            ExtmarkUndoObject::Splice(s) => {
                assert_eq!(s.old_col, 2);
                assert_eq!(s.old_byte, 2);
                assert_eq!(s.new_col, 3);
            }
            other => panic!("expected a splice record, got {other:?}"),
        }

        drop(unsafe { Box::from_raw(uhp) });
    }

    /// A different row cannot merge, so a second record is appended.
    #[test]
    fn splice_impl_does_not_merge_across_rows() {
        let _lock = crate::globals::global_state_test_lock();
        let uhp = new_header();
        let mut buf = buf_with_header(uhp);

        unsafe {
            extmark_splice_impl(&mut buf, 0, 0, 0, 0, 0, 0, 0, 2, 2, ExtmarkOp::Undo);
            extmark_splice_impl(&mut buf, 4, 0, 30, 0, 0, 0, 0, 2, 2, ExtmarkOp::Undo);
        }

        assert_eq!(unsafe { &(*uhp).uh_extmark }.len(), 2);
        drop(unsafe { Box::from_raw(uhp) });
    }

    /// `extmark_splice` derives the byte offset itself. With no
    /// memline flushed yet the lookup reports a negative offset, and
    /// the documented empty-chunk-cache edge case clamps it to 0.
    #[test]
    fn splice_cols_delegates_with_zero_row_extents() {
        let _lock = crate::globals::global_state_test_lock();
        let uhp = new_header();
        let mut buf = Box::new(buf_with_header(uhp));
        // Derive the pointer exactly once and reuse it, so the
        // reference the callee builds and the one stored in `GLOBALS`
        // share a single provenance.
        let buf_ptr: *mut BufT = &mut *buf;

        // `ml_find_line_or_offset` flushes through the global
        // `curbuf`, so it must point somewhere real.
        let saved = {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let saved = g.curbuf;
            g.curbuf = buf_ptr;
            saved
        };

        unsafe {
            extmark_splice_cols(&mut *buf_ptr, 0, 6, 0, 3, ExtmarkOp::Undo);
        }

        let recorded = unsafe { &(*uhp).uh_extmark };
        assert_eq!(recorded.len(), 1);
        match &recorded[0] {
            ExtmarkUndoObject::Splice(s) => {
                assert_eq!(s.old_row, 0);
                assert_eq!(s.new_row, 0);
                assert_eq!(s.start_col, 6);
                assert_eq!(s.start_byte, 6, "offset clamped to 0, plus start_col");
                assert_eq!(s.new_col, 3);
                assert_eq!(s.new_byte, 3, "byte count mirrors the column count");
            }
            other => panic!("expected a splice record, got {other:?}"),
        }

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = saved;
        drop(unsafe { Box::from_raw(uhp) });
    }

    /// An empty marktree makes `marktree_itr_get` leave the iterator
    /// pointing at nothing, so the very first `pos.row < 0` check
    /// ends the walk before the loop body runs at all. This is the
    /// only path reachable today.
    #[test]
    fn splice_delete_walks_nothing_for_an_empty_marktree() {
        let mut buf = BufT::default();
        let mut uv: ExtmarkUndoVecT = Vec::new();
        unsafe {
            extmark_splice_delete(
                &mut buf,
                0,
                0,
                10,
                10,
                Some(&mut uv),
                false,
                ExtmarkOp::Undo,
            );
        }
        assert!(uv.is_empty());
    }

    #[test]
    fn splice_delete_accepts_a_missing_undo_vector() {
        // `uvp == NULL` is a real call shape: `only_copy == false`
        // with an operation that is not undoable.
        let mut buf = BufT::default();
        unsafe {
            extmark_splice_delete(&mut buf, 0, 0, 4, 0, None, false, ExtmarkOp::NoUndo);
        }
    }

    #[test]
    fn splice_delete_only_copy_leaves_an_empty_marktree_alone() {
        let mut buf = BufT::default();
        let mut uv: ExtmarkUndoVecT = Vec::new();
        unsafe {
            extmark_splice_delete(&mut buf, 2, 3, 2, 7, Some(&mut uv), true, ExtmarkOp::Noop);
        }
        assert!(uv.is_empty());
    }
}
