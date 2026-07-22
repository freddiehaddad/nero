//! Translated from `src/nvim/undo.c` (partial).
//!
//! Translated: the pure in-memory undo-tree/state bookkeeping -
//! `u_freeheader`, `u_freebranch`, `u_freeentries` (folds the original's
//! separate `u_freeentry()` per-entry loop into Rust's automatic `Drop`
//! for `UHeader.uh_entries: Vec<UEntry>`, so there is no standalone
//! `u_freeentry` here), `u_clearall`, `u_blockfree`,
//! `u_clearallandblockfree`, `u_unchanged`, `u_unch_branch`,
//! `u_update_save_nr`, `u_clearline`.
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `u_check_tree`/`u_check`: `#ifdef U_DEBUG`-only consistency
//!   checkers, need `emsg`/`smsg`/`semsg` (`message.c`) and the
//!   debug-only `uh_magic`/`ue_magic` fields (no equivalent debug-build
//!   concept established in this crate yet, same reasoning as
//!   marktree.rs's deferred `mt_inspect*` debug functions).
//! - `u_save*`/`u_undo*`/`u_redo*`/`undo_time`: need `ml_replace`/
//!   `ml_delete`/`ml_append` (`memline.c`, itself blocked on real file
//!   I/O for its own core paths) and autocmd triggers (`autocmd.c`).
//! - `u_compute_hash`/`u_get_undo_file_name`/`u_write_undo`/
//!   `u_read_undo`/`serialize_*`/`unserialize_*`: undo-FILE
//!   persistence, needs real file I/O (`os/fs.c`, blocked on the
//!   libuv FFI-vs-crate decision, phase 11) and SHA-256 hashing of file
//!   state (have `sha256.rs`, but not wired to real file reads yet).
//! - `u_saveline`/`u_save_line`/`u_save_line_buf`/`u_undoline`: need
//!   `ml_get_buf`/`ml_replace` (`memline.c`).
//! - `bufIsChanged`/`anyBufIsChanged`/`curbufIsChanged`: `bt_prompt`/
//!   `bt_dontwrite` now exist (`crate::buffer`), but `file_ff_differs`
//!   (`change.c`) still needs `ml_get_buf` (`memline.c`) for one early-
//!   return branch (`ignore_empty && BF_NEW && ml_line_count == 1`), so
//!   these three remain blocked as a whole.
//! - `ex_undolist`/`ex_undojoin`: need `exarg_T` (blocked on the
//!   `ex_cmds.lua`-generated `cmdidx_T`, same blocker as `mark.c`'s
//!   `ex_*` functions).
//! - `u_find_first_changed`: needs `ml_get_buf` (`memline.c`).
//! - `u_get_headentry`/`u_getbot`: need `iemsg()` (`message.c`).

use crate::buffer_defs::BufT;
use crate::undo_defs::{uh_flags, UHeader, UhLink};

/// Free one header `uhp` and its entry list and adjust the pointers
/// (`u_freeheader`).
///
/// @param uhpp  if not NULL reset when freeing this header
///
/// # Safety
/// `uhp` must be a valid, currently-linked `UHeader` pointer (originally
/// heap-allocated via `Box::into_raw`, per this crate's established
/// convention for manually-managed intrusive tree/list nodes, e.g.
/// `MtNode`). `uhpp`, if non-null, must point at a valid `*mut UHeader`
/// slot.
pub unsafe fn u_freeheader(buf: &mut BufT, uhp: *mut UHeader, uhpp: *mut *mut UHeader) {
    unsafe {
        // When there is an alternate redo list free that branch
        // completely, because we can never go there.
        let uh_alt_next = (*uhp).uh_alt_next.ptr();
        if !uh_alt_next.is_null() {
            u_freebranch(buf, uh_alt_next, uhpp);
        }

        let uh_alt_prev = (*uhp).uh_alt_prev.ptr();
        if !uh_alt_prev.is_null() {
            (*uh_alt_prev).uh_alt_next = UhLink::Ptr(std::ptr::null_mut());
        }

        // Update the links in the list to remove the header.
        let uh_next = (*uhp).uh_next.ptr();
        if uh_next.is_null() {
            buf.b_u_oldhead = (*uhp).uh_prev.ptr();
        } else {
            (*uh_next).uh_prev = (*uhp).uh_prev;
        }

        let uh_prev = (*uhp).uh_prev.ptr();
        if uh_prev.is_null() {
            buf.b_u_newhead = (*uhp).uh_next.ptr();
        } else {
            let mut uhap = uh_prev;
            while !uhap.is_null() {
                (*uhap).uh_next = (*uhp).uh_next;
                uhap = (*uhap).uh_alt_next.ptr();
            }
        }

        u_freeentries(buf, uhp, uhpp);
    }
}

/// Free an alternate branch and any following alternate branches
/// (`u_freebranch`).
///
/// @param uhpp  if not NULL reset when freeing this header
///
/// # Safety
/// Same requirement as [`u_freeheader`].
pub unsafe fn u_freebranch(buf: &mut BufT, uhp: *mut UHeader, uhpp: *mut *mut UHeader) {
    unsafe {
        // If this is the top branch we may need to use u_freeheader() to
        // update all the pointers.
        if uhp == buf.b_u_oldhead {
            while !buf.b_u_oldhead.is_null() {
                u_freeheader(buf, buf.b_u_oldhead, uhpp);
            }
            return;
        }

        let uh_alt_prev = (*uhp).uh_alt_prev.ptr();
        if !uh_alt_prev.is_null() {
            (*uh_alt_prev).uh_alt_next = UhLink::Ptr(std::ptr::null_mut());
        }

        let mut next = uhp;
        while !next.is_null() {
            let tofree = next;
            let uh_alt_next = (*tofree).uh_alt_next.ptr();
            if !uh_alt_next.is_null() {
                u_freebranch(buf, uh_alt_next, uhpp); // recursive
            }
            next = (*tofree).uh_prev.ptr();
            u_freeentries(buf, tofree, uhpp);
        }
    }
}

/// Free all the undo entries for one header and the header itself. This
/// means that `uhp` is invalid when returning (`u_freeentries`).
///
/// @param uhpp  if not NULL reset when freeing this header
///
/// # Safety
/// `uhp` must be a valid `UHeader` pointer originally allocated via
/// `Box::into_raw` (see [`u_freeheader`]'s doc comment); it must not be
/// used again after this call.
pub unsafe fn u_freeentries(buf: &mut BufT, uhp: *mut UHeader, uhpp: *mut *mut UHeader) {
    // Check for pointers to the header that become invalid now.
    if buf.b_u_curhead == uhp {
        buf.b_u_curhead = std::ptr::null_mut();
    }
    if buf.b_u_newhead == uhp {
        buf.b_u_newhead = std::ptr::null_mut(); // freeing the newest entry
    }
    unsafe {
        if !uhpp.is_null() && uhp == *uhpp {
            *uhpp = std::ptr::null_mut();
        }

        // Frees uh_entries (each owning its lines) and uh_extmark
        // automatically via Drop when the reconstructed Box is dropped -
        // matching the original's manual per-entry u_freeentry() loop
        // plus kv_destroy(uh_extmark)/xfree(uhp).
        drop(Box::from_raw(uhp));
    }
    buf.b_u_numhead -= 1;
}

/// Invalidate the undo buffer; called when storage has already been
/// released (`u_clearall`).
pub fn u_clearall(buf: &mut BufT) {
    buf.b_u_newhead = std::ptr::null_mut();
    buf.b_u_oldhead = std::ptr::null_mut();
    buf.b_u_curhead = std::ptr::null_mut();
    buf.b_u_synced = true;
    buf.b_u_numhead = 0;
    buf.b_u_line_ptr = None;
    buf.b_u_line_lnum = 0;
}

/// Free all allocated memory blocks for the buffer `buf` (`u_blockfree`).
pub fn u_blockfree(buf: &mut BufT) {
    while !buf.b_u_oldhead.is_null() {
        let previous_oldhead = buf.b_u_oldhead;
        // SAFETY: b_u_oldhead is a well-formed, currently-linked UHeader
        // pointer (an established invariant of a real BufT's undo tree;
        // for a freshly-`Default`-constructed BufT as used in this
        // file's own tests, every header pushed onto the list is
        // Box::into_raw-allocated, matching u_freeheader's contract).
        unsafe { u_freeheader(buf, buf.b_u_oldhead, std::ptr::null_mut()) };
        assert_ne!(buf.b_u_oldhead, previous_oldhead);
    }
    buf.b_u_line_ptr = None; // xfree(buf->b_u_line_ptr)
}

/// Free all allocated memory blocks for the buffer `buf` and invalidate
/// the undo buffer (`u_clearallandblockfree`).
pub fn u_clearallandblockfree(buf: &mut BufT) {
    u_blockfree(buf);
    u_clearall(buf);
}

/// # Safety
/// `uhp`, if non-null, must be a valid, currently-linked `UHeader`
/// pointer.
unsafe fn u_unch_branch(uhp: *mut UHeader) {
    let mut uh = uhp;
    while !uh.is_null() {
        unsafe {
            (*uh).uh_flags |= uh_flags::CHANGED;
            let alt_next = (*uh).uh_alt_next.ptr();
            if !alt_next.is_null() {
                u_unch_branch(alt_next); // recursive
            }
            uh = (*uh).uh_prev.ptr();
        }
    }
}

/// Marks the buffer's whole undo tree "changed" (used e.g. when the
/// buffer as a whole is considered modified again after being marked
/// unchanged) (`u_unchanged`).
pub fn u_unchanged(buf: &mut BufT) {
    // SAFETY: buf.b_u_oldhead is a well-formed undo-tree pointer (or
    // null) per BufT's own invariant.
    unsafe { u_unch_branch(buf.b_u_oldhead) };
    buf.b_did_warn = false;
}

/// Increase the write count, store it in the last undo header, what
/// would be used for "u" (`u_update_save_nr`).
pub fn u_update_save_nr(buf: &mut BufT) {
    buf.b_u_save_nr_last += 1;
    buf.b_u_save_nr_cur = buf.b_u_save_nr_last;
    let uhp = if !buf.b_u_curhead.is_null() {
        // SAFETY: b_u_curhead is a well-formed undo-tree pointer.
        unsafe { (*buf.b_u_curhead).uh_next.ptr() }
    } else {
        buf.b_u_newhead
    };
    if !uhp.is_null() {
        // SAFETY: uhp just null-checked, well-formed per the same
        // invariant.
        unsafe { (*uhp).uh_save_nr = buf.b_u_save_nr_last };
    }
}

/// Clear the line saved for the "U" command (this is used externally for
/// crossing a line while in insert mode) (`u_clearline`).
pub fn u_clearline(buf: &mut BufT) {
    if buf.b_u_line_ptr.is_none() {
        return;
    }
    buf.b_u_line_ptr = None;
    buf.b_u_line_lnum = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::undo_defs::UEntry;

    /// Allocates a new, `Box::into_raw`-owned `UHeader`, matching the
    /// original's `xmalloc(sizeof(u_header_T))` allocation style.
    fn new_header() -> *mut UHeader {
        Box::into_raw(Box::new(UHeader::default()))
    }

    /// Builds a simple linear undo history of `n` headers on `buf`
    /// (oldest first, matching `b_u_oldhead`/`b_u_newhead`/`uh_next`/
    /// `uh_prev` semantics: `uh_prev` points toward the newest,
    /// `uh_next` points toward the oldest).
    fn push_linear_history(buf: &mut BufT, n: usize) -> Vec<*mut UHeader> {
        let mut headers = Vec::with_capacity(n);
        let mut prev: *mut UHeader = std::ptr::null_mut();
        for i in 0..n {
            let uhp = new_header();
            unsafe {
                (*uhp).uh_seq = i as i32 + 1;
                (*uhp).uh_entries.push(UEntry {
                    ue_top: 0,
                    ue_bot: 0,
                    ue_lcount: 0,
                    ue_array: vec![b"line".to_vec()],
                });
                (*uhp).uh_prev = UhLink::Ptr(std::ptr::null_mut());
                (*uhp).uh_next = UhLink::Ptr(prev);
                if !prev.is_null() {
                    (*prev).uh_prev = UhLink::Ptr(uhp);
                }
            }
            headers.push(uhp);
            prev = uhp;
        }
        buf.b_u_newhead = *headers.last().unwrap();
        buf.b_u_oldhead = headers[0];
        buf.b_u_numhead = n as i32;
        headers
    }

    #[test]
    fn u_clearall_resets_all_tree_pointers() {
        let mut buf = BufT::default();
        let headers = push_linear_history(&mut buf, 2);
        buf.b_u_curhead = headers[1];
        u_clearall(&mut buf);
        assert!(buf.b_u_oldhead.is_null());
        assert!(buf.b_u_newhead.is_null());
        assert!(buf.b_u_curhead.is_null());
        assert!(buf.b_u_synced);
        assert_eq!(buf.b_u_numhead, 0);
        assert!(buf.b_u_line_ptr.is_none());

        // Headers were never actually freed by u_clearall (matches the
        // original: "called when storage has already been released") -
        // free them manually here so this test doesn't leak.
        for h in headers {
            unsafe { drop(Box::from_raw(h)) };
        }
    }

    #[test]
    fn u_blockfree_frees_entire_linear_history() {
        let mut buf = BufT::default();
        push_linear_history(&mut buf, 3);
        assert_eq!(buf.b_u_numhead, 3);
        u_blockfree(&mut buf);
        assert!(buf.b_u_oldhead.is_null());
        // u_blockfree only frees storage; b_u_newhead/numhead are left
        // to whoever calls u_clearall next (matches the original: they
        // are two separate functions, only combined by
        // u_clearallandblockfree).
    }

    #[test]
    fn u_clearallandblockfree_frees_and_resets_everything() {
        let mut buf = BufT::default();
        push_linear_history(&mut buf, 3);
        u_clearallandblockfree(&mut buf);
        assert!(buf.b_u_oldhead.is_null());
        assert!(buf.b_u_newhead.is_null());
        assert!(buf.b_u_curhead.is_null());
        assert_eq!(buf.b_u_numhead, 0);
        assert!(buf.b_u_synced);
    }

    #[test]
    fn u_freeheader_removes_middle_entry_and_relinks() {
        let mut buf = BufT::default();
        let headers = push_linear_history(&mut buf, 3);
        // headers[0] = oldest, headers[2] = newest. uh_next points
        // toward older headers, uh_prev points toward newer ones
        // (verified against u_savecommon's own linking code: a newly
        // pushed header always becomes b_u_newhead with uh_prev = NULL
        // and uh_next = the previous newhead).
        unsafe {
            u_freeheader(&mut buf, headers[1], std::ptr::null_mut());
        }
        assert_eq!(buf.b_u_numhead, 2);
        unsafe {
            // newest's uh_next (toward older) should now skip straight
            // to oldest, and oldest's uh_prev (toward newer) should
            // skip straight to newest.
            assert_eq!((*headers[2]).uh_next.ptr(), headers[0]);
            assert_eq!((*headers[0]).uh_prev.ptr(), headers[2]);
        }
        u_blockfree(&mut buf);
    }

    #[test]
    fn u_freeheader_updates_oldhead_when_freeing_oldest() {
        let mut buf = BufT::default();
        let headers = push_linear_history(&mut buf, 2);
        unsafe {
            u_freeheader(&mut buf, headers[0], std::ptr::null_mut());
        }
        assert_eq!(buf.b_u_oldhead, headers[1]);
        assert_eq!(buf.b_u_numhead, 1);
        u_blockfree(&mut buf);
    }

    #[test]
    fn u_unchanged_marks_whole_branch_changed() {
        let mut buf = BufT::default();
        let headers = push_linear_history(&mut buf, 3);
        u_unchanged(&mut buf);
        for &h in &headers {
            unsafe {
                assert_ne!((*h).uh_flags & uh_flags::CHANGED, 0);
            }
        }
        assert!(!buf.b_did_warn);
        u_blockfree(&mut buf);
    }

    #[test]
    fn u_update_save_nr_updates_last_and_cur() {
        let mut buf = BufT::default();
        push_linear_history(&mut buf, 1);
        buf.b_u_save_nr_last = 4;
        u_update_save_nr(&mut buf);
        assert_eq!(buf.b_u_save_nr_last, 5);
        assert_eq!(buf.b_u_save_nr_cur, 5);
        unsafe {
            assert_eq!((*buf.b_u_newhead).uh_save_nr, 5);
        }
        u_blockfree(&mut buf);
    }

    #[test]
    fn u_clearline_frees_and_resets_saved_line() {
        let mut buf = BufT {
            b_u_line_ptr: Some(b"saved".to_vec()),
            b_u_line_lnum: 7,
            ..Default::default()
        };
        u_clearline(&mut buf);
        assert!(buf.b_u_line_ptr.is_none());
        assert_eq!(buf.b_u_line_lnum, 0);
    }

    #[test]
    fn u_clearline_is_noop_when_nothing_saved() {
        // should NOT be reset, since ptr is None
        let mut buf = BufT {
            b_u_line_lnum: 3,
            ..Default::default()
        };
        u_clearline(&mut buf);
        assert_eq!(buf.b_u_line_lnum, 3);
    }
}
