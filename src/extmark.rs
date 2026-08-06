//! Extmark tracking (`extmark.c`).
//!
//! Extmarks are positions in a buffer that follow text as it is
//! edited. They are stored in the buffer's own marktree
//! (`BufT::b_marktree`, already translated in `marktree.rs`); this
//! file holds the editing-side glue that keeps them correct across
//! splices, and records enough information to undo those adjustments.
//!
//! Translated so far: [`extmark_splice_delete`].
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
use crate::extmark_defs::{ExtmarkOp, ExtmarkSavePos, ExtmarkUndoObject, ExtmarkUndoVecT};
use crate::marktree::{
    marktree_get_altpos, marktree_itr_current, marktree_itr_get, marktree_itr_next, mt_end,
    mt_invalid, mt_invalidate, mt_lookup_key, mt_no_undo, mt_paired, mt_right,
};
use crate::marktree_defs::MarkTreeIter;
use crate::pos_defs::ColnrT;

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

#[cfg(test)]
mod tests {
    use super::*;

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
