//! Translated from `src/nvim/undo.c` (partial).
//!
//! Translated: the pure in-memory undo-tree/state bookkeeping -
//! `u_freeheader`, `u_freebranch`, `u_freeentries` (folds the original's
//! separate `u_freeentry()` per-entry loop into Rust's automatic `Drop`
//! for `UHeader.uh_entries: Vec<UEntry>`, so there is no standalone
//! `u_freeentry` here), `u_clearall`, `u_blockfree`,
//! `u_clearallandblockfree`, `u_unchanged`, `u_unch_branch`,
//! `u_update_save_nr`, `u_clearline`; `bufIsChanged`/`anyBufIsChanged`/
//! `curbufIsChanged` (as `buf_is_changed`/`any_buf_is_changed`/
//! `curbuf_is_changed` - adapted to snake_case, matching this crate's
//! usual convention even though the originals are themselves
//! camelCase) - now tractable now that `change.c`'s `file_ff_differs`
//! exists (needed `memline.c`'s `ml_get_buf`); `u_find_first_changed`
//! (now tractable now that `memline.c`'s `ml_get_buf` exists - only
//! needed `UHeader.uh_entries`/`ue_top`/`ue_bot`/`ue_array`, already
//! present in `undo_defs.rs`, plus `mark_defs.rs`'s already-translated
//! `clearpos`).
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `u_check_tree`/`u_check`: `#ifdef U_DEBUG`-only consistency
//!   checkers, need `emsg`/`smsg`/`semsg` (`message.c`) and the
//!   debug-only `uh_magic`/`ue_magic` fields (no equivalent debug-build
//!   concept established in this crate yet, same reasoning as
//!   marktree.rs's deferred `mt_inspect*` debug functions).
//! - `u_save*`/`u_undo*`/`u_redo*`/`undo_time`: need `autocmd.c`
//!   triggers (`memline.c`'s write side, `ml_replace`/`ml_delete`/
//!   `ml_append`, now exists, but these are still substantial
//!   undertakings in their own right - not (re-)examined yet).
//! - `u_compute_hash`/`u_get_undo_file_name`/`u_write_undo`/
//!   `u_read_undo`/`serialize_*`/`unserialize_*`: undo-FILE
//!   persistence. Re-checked after `os/fs.rs` gained real `os_open` -
//!   the "blocked on the libuv FFI-vs-crate decision" framing was
//!   stale (real file I/O is no longer the blocker), but these are
//!   still genuinely blocked for other reasons: `u_write_undo` alone
//!   needs real user-facing `smsg`/verbose messages (`message.c`,
//!   still not tractable), `os_getperm` (still deferred, see
//!   `os/fs.rs`'s own module doc), `'undodir'`-based directory search
//!   (`u_get_undo_file_name`), and its own serialization format
//!   (`serialize_header`/`serialize_uhp`/etc., not yet examined) -
//!   still a substantial, separate undertaking.
//! - `u_saveline`/`u_save_line`/`u_save_line_buf`/`u_undoline`: needed
//!   `ml_replace` (`memline.c`'s write side) - now exists, worth a
//!   fresh look, not yet (re-)examined this pass.
//! - `ex_undolist`/`ex_undojoin`: need `exarg_T` (blocked on the
//!   `ex_cmds.lua`-generated `cmdidx_T`, same blocker as `mark.c`'s
//!   `ex_*` functions).
//! - `u_get_headentry`/`u_getbot`: need `iemsg()` (`message.c`).

use crate::buffer_defs::BufT;
use crate::pos_defs::LinenrT;
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

/// After reloading a buffer which was saved for `'undoreload'`: find
/// the first line that was changed and set the cursor there
/// (`u_find_first_changed`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` whose `b_u_newhead`, if non-null, is a valid
/// `UHeader` pointer, and whose `b_ml.ml_mfp`, if non-null, points to
/// a live `MemfileT`.
pub unsafe fn u_find_first_changed() {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    let uhp = curbuf.b_u_newhead;

    if !curbuf.b_u_curhead.is_null() || uhp.is_null() {
        return; // undid something in an autocmd?
    }

    // Check that the last undo block was for the whole file. `uhp.
    // uh_entries.first()` stands in for the original's `uhp->uh_entry`
    // (the first entry of a singly-linked list there; owned directly
    // in `Vec<UEntry>` order here).
    // SAFETY: forwarded from this function's own safety doc.
    let uh = unsafe { &mut *uhp };
    let Some(uep) = uh.uh_entries.first() else {
        // (matches the original's implicit assumption that uh_entry is
        // non-null when ue_top/ue_bot are read next - an empty
        // uh_entries here would itself be a pre-existing invariant
        // violation, not something reachable in practice.)
        return;
    };
    if uep.ue_top != 0 || uep.ue_bot != 0 {
        return;
    }
    // `uep.ue_array.len()` stands in for the original's separate
    // `ue_size` count (a `Vec` already tracks its own length - see
    // `UEntry`'s own doc comment in undo_defs.rs).
    let ue_size = uep.ue_array.len() as LinenrT;

    let mut lnum: LinenrT = 1;
    while lnum < curbuf.b_ml.ml_line_count && lnum <= ue_size {
        // SAFETY: forwarded from this function's own safety doc.
        let line = unsafe { crate::memline::ml_get_buf(curbuf, lnum) };
        if line != uh.uh_entries[0].ue_array[(lnum - 1) as usize] {
            crate::mark_defs::clearpos(&mut uh.uh_cursor);
            uh.uh_cursor.lnum = lnum;
            return;
        }
        lnum += 1;
    }
    if curbuf.b_ml.ml_line_count != ue_size {
        // lines added or deleted at the end, put the cursor there
        crate::mark_defs::clearpos(&mut uh.uh_cursor);
        uh.uh_cursor.lnum = lnum;
    }
}

/// Check if the `'modified'` flag is set, or `'ff'` has changed (only
/// need to check the first character, because it can only be `"dos"`,
/// `"unix"` or `"mac"`). `"nofile"` and `"scratch"` type buffers are
/// considered to always be unchanged. Prompt buffers ignore implicit
/// modifications by default, but an explicit `:set modified` still
/// makes them count as changed (`bufIsChanged`).
///
/// # Safety
/// Same as `crate::change::file_ff_differs`.
#[must_use]
pub unsafe fn buf_is_changed(buf: &mut BufT) -> bool {
    // In a "prompt" buffer we respect 'modified' if the user or a
    // plugin explicitly set it.
    if crate::buffer::bt_prompt(Some(buf)) {
        buf.b_modified_was_set
    } else {
        !crate::buffer::bt_dontwrite(Some(buf))
            && (buf.b_changed != 0
                // SAFETY: forwarded from this function's own safety doc.
                || unsafe { crate::change::file_ff_differs(buf, true) })
    }
}

/// Return true if any buffer has changes. Also buffers that are not
/// written (`anyBufIsChanged`).
///
/// # Safety
/// Walks the real `GLOBALS.firstbuf`/`b_next` linked list and
/// dereferences each node - callers must ensure every live buffer in
/// the list is a valid, properly initialized `BufT` (same requirement
/// as any other `firstbuf`/`b_next` traversal in this crate). Also see
/// [`buf_is_changed`]'s own safety doc.
#[must_use]
pub unsafe fn any_buf_is_changed() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let mut bp = unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf;
    while !bp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let b = unsafe { &mut *bp };
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { buf_is_changed(b) } {
            return true;
        }
        bp = b.b_next;
    }
    false
}

/// Return true if the current buffer has changed (`curbufIsChanged`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`. Also see [`buf_is_changed`]'s own safety doc.
#[must_use]
pub unsafe fn curbuf_is_changed() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { buf_is_changed(curbuf) }
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

    #[test]
    fn buf_is_changed_prompt_buffer_respects_modified_was_set() {
        let mut buf = BufT {
            b_p_bt: Some(b"prompt".to_vec()),
            b_modified_was_set: true,
            b_changed: 0, // deliberately 0: prompt buffers ignore this
            ..Default::default()
        };
        assert!(unsafe { buf_is_changed(&mut buf) });

        buf.b_modified_was_set = false;
        assert!(!unsafe { buf_is_changed(&mut buf) });
    }

    #[test]
    fn buf_is_changed_dontwrite_buffer_is_never_changed() {
        let mut buf = BufT {
            b_p_bt: Some(b"nofile".to_vec()),
            b_changed: 1, // even with b_changed set...
            b_flags: crate::buffer_defs::b_flags::BF_NEVERLOADED as i32,
            ..Default::default()
        };
        assert!(!unsafe { buf_is_changed(&mut buf) });
    }

    #[test]
    fn buf_is_changed_normal_buffer_follows_b_changed() {
        // BF_NEVERLOADED short-circuits file_ff_differs to false,
        // avoiding the need for a real memline in this test.
        let mut buf = BufT {
            b_flags: crate::buffer_defs::b_flags::BF_NEVERLOADED as i32,
            b_changed: 1,
            ..Default::default()
        };
        assert!(unsafe { buf_is_changed(&mut buf) });

        buf.b_changed = 0;
        assert!(!unsafe { buf_is_changed(&mut buf) });
    }

    #[test]
    fn curbuf_is_changed_matches_buf_is_changed_on_curbuf() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT {
            b_flags: crate::buffer_defs::b_flags::BF_NEVERLOADED as i32,
            b_changed: 1,
            ..Default::default()
        };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        assert!(unsafe { curbuf_is_changed() });

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
    }

    #[test]
    fn any_buf_is_changed_walks_the_buffer_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf2 = BufT {
            b_flags: crate::buffer_defs::b_flags::BF_NEVERLOADED as i32,
            b_changed: 1,
            ..Default::default()
        };
        let mut buf1 = BufT {
            b_flags: crate::buffer_defs::b_flags::BF_NEVERLOADED as i32,
            b_changed: 0,
            b_next: &mut buf2 as *mut BufT,
            ..Default::default()
        };
        let prev_firstbuf = unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf = &mut buf1 as *mut BufT;

        assert!(unsafe { any_buf_is_changed() });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf = prev_firstbuf;
    }

    #[test]
    fn any_buf_is_changed_false_when_no_buffer_changed() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf1 = BufT {
            b_flags: crate::buffer_defs::b_flags::BF_NEVERLOADED as i32,
            b_changed: 0,
            ..Default::default()
        };
        let prev_firstbuf = unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf = &mut buf1 as *mut BufT;

        assert!(!unsafe { any_buf_is_changed() });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf = prev_firstbuf;
    }

    /// Builds a working 2-line memline ("one\0", "two\0") on a fresh
    /// buffer, for `u_find_first_changed` tests.
    fn buf_with_two_lines() -> BufT {
        let mut buf = BufT::default();
        unsafe {
            assert_eq!(crate::memline::ml_open(&mut buf), crate::vim_defs::OK);
            assert_eq!(
                crate::memline::ml_replace_buf_len(&mut buf, 1, b"one\0"),
                crate::vim_defs::OK
            );
            assert_eq!(
                crate::memline::ml_append_buf(&mut buf, 1, b"two\0", 4, false),
                crate::vim_defs::OK
            );
        }
        assert_eq!(buf.b_ml.ml_line_count, 2);
        buf
    }

    /// Builds a working 3-line memline ("one\0", "two\0", "three\0") on
    /// a fresh buffer, for `u_find_first_changed` tests that need a
    /// non-last differing line (the original's own loop bound,
    /// `lnum < ml_line_count`, never inspects the very last line when
    /// `ue_size == ml_line_count` - see undo.c:2804).
    fn buf_with_three_lines() -> BufT {
        let mut buf = BufT::default();
        unsafe {
            assert_eq!(crate::memline::ml_open(&mut buf), crate::vim_defs::OK);
            assert_eq!(
                crate::memline::ml_replace_buf_len(&mut buf, 1, b"one\0"),
                crate::vim_defs::OK
            );
            assert_eq!(
                crate::memline::ml_append_buf(&mut buf, 1, b"two\0", 4, false),
                crate::vim_defs::OK
            );
            assert_eq!(
                crate::memline::ml_append_buf(&mut buf, 2, b"three\0", 6, false),
                crate::vim_defs::OK
            );
        }
        assert_eq!(buf.b_ml.ml_line_count, 3);
        buf
    }

    fn close_buf_with_memline(buf: BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn u_find_first_changed_leaves_cursor_untouched_when_nothing_differs() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_two_lines();
        let uhp = new_header();
        unsafe {
            (*uhp).uh_entries.push(UEntry {
                ue_top: 0,
                ue_bot: 0,
                ue_lcount: 0,
                ue_array: vec![b"one\0".to_vec(), b"two\0".to_vec()],
            });
        }
        buf.b_u_newhead = uhp;
        buf.b_u_curhead = std::ptr::null_mut();

        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;
        unsafe { u_find_first_changed() };
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;

        // same line count, all lines matched: cursor position untouched
        // (stays at UHeader::default()'s zeroed pos_T).
        assert_eq!(unsafe { (*uhp).uh_cursor.lnum }, 0);

        unsafe {
            drop(Box::from_raw(uhp));
        }
        close_buf_with_memline(buf);
    }

    #[test]
    fn u_find_first_changed_finds_the_differing_line() {
        let _lock = crate::globals::global_state_test_lock();
        // 3 lines, difference on the middle line: the original's own
        // loop bound (`lnum < ml_line_count`) never inspects the very
        // last line when `ue_size == ml_line_count` (see undo.c:2804),
        // so the differing line must not be the last one here.
        let mut buf = buf_with_three_lines();
        let uhp = new_header();
        unsafe {
            (*uhp).uh_entries.push(UEntry {
                ue_top: 0,
                ue_bot: 0,
                ue_lcount: 0,
                ue_array: vec![
                    b"one\0".to_vec(),
                    b"CHANGED\0".to_vec(),
                    b"three\0".to_vec(),
                ],
            });
        }
        buf.b_u_newhead = uhp;
        buf.b_u_curhead = std::ptr::null_mut();

        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;
        unsafe { u_find_first_changed() };
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;

        assert_eq!(unsafe { (*uhp).uh_cursor.lnum }, 2);

        unsafe {
            drop(Box::from_raw(uhp));
        }
        close_buf_with_memline(buf);
    }

    #[test]
    fn u_find_first_changed_handles_lines_added_at_the_end() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_two_lines();
        let uhp = new_header();
        unsafe {
            // ue_array only recorded 1 line - the buffer gained a line
            // at the end (matching lines stay matching).
            (*uhp).uh_entries.push(UEntry {
                ue_top: 0,
                ue_bot: 0,
                ue_lcount: 0,
                ue_array: vec![b"one\0".to_vec()],
            });
        }
        buf.b_u_newhead = uhp;
        buf.b_u_curhead = std::ptr::null_mut();

        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;
        unsafe { u_find_first_changed() };
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;

        // loop stops at lnum=2 (lnum <= ue_size(1) fails first), line
        // counts differ (2 != 1), so cursor is placed at lnum=2.
        assert_eq!(unsafe { (*uhp).uh_cursor.lnum }, 2);

        unsafe {
            drop(Box::from_raw(uhp));
        }
        close_buf_with_memline(buf);
    }

    #[test]
    fn u_find_first_changed_is_noop_when_curhead_is_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_two_lines();
        let uhp = new_header();
        unsafe {
            (*uhp).uh_entries.push(UEntry {
                ue_top: 0,
                ue_bot: 0,
                ue_lcount: 0,
                ue_array: vec![b"DIFFERENT\0".to_vec(), b"two\0".to_vec()],
            });
        }
        buf.b_u_newhead = uhp;
        // Non-null b_u_curhead means "undid something in an autocmd" -
        // bail out immediately without touching uh_cursor.
        buf.b_u_curhead = new_header();

        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;
        unsafe { u_find_first_changed() };
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;

        assert_eq!(unsafe { (*uhp).uh_cursor.lnum }, 0);

        unsafe {
            drop(Box::from_raw(uhp));
            drop(Box::from_raw(buf.b_u_curhead));
        }
        close_buf_with_memline(buf);
    }

    #[test]
    fn u_find_first_changed_is_noop_when_last_undo_block_was_not_whole_file() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_two_lines();
        let uhp = new_header();
        unsafe {
            // ue_top != 0 means this undo block wasn't for the whole
            // file - bail out without touching uh_cursor.
            (*uhp).uh_entries.push(UEntry {
                ue_top: 1,
                ue_bot: 0,
                ue_lcount: 0,
                ue_array: vec![b"DIFFERENT\0".to_vec(), b"two\0".to_vec()],
            });
        }
        buf.b_u_newhead = uhp;
        buf.b_u_curhead = std::ptr::null_mut();

        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;
        unsafe { u_find_first_changed() };
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;

        assert_eq!(unsafe { (*uhp).uh_cursor.lnum }, 0);

        unsafe {
            drop(Box::from_raw(uhp));
        }
        close_buf_with_memline(buf);
    }
}
