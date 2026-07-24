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
//! `clearpos`); `u_save_line_buf`/`u_save_line`/`u_saveline` (the "U"
//! command's single-line save mechanism - now tractable now that
//! `memline.c`'s `ml_get_buf` exists and `GLOBALS.curwin`/`WinT.
//! w_buffer`/`w_cursor` are all already present); `get_undolevel`/
//! `u_get_headentry`/`u_getbot`/`u_sync` (re-examined: `u_get_headentry`/
//! `u_getbot`'s own `iemsg()` calls are genuinely reachable "corrupted
//! undo list"/"line missing" defensive checks, not internal-only
//! invariants, but they fit the same `mf_write`/`ml_open`/`ml_find_line`
//! policy already established elsewhere in this crate: the message
//! *display* is skipped since `message.c`'s pipeline isn't tractable
//! yet, while the exact same state/fallback behavior as the original is
//! kept); `ex_undojoin` (now tractable now that
//! `crate::ex_cmds_defs::ExargT` exists - its own
//! `emsg("E790: undojoin is not allowed after undo")` is a real,
//! reachable user error, not an internal-invariant check, but the
//! message *display* is skipped the same way while the exact
//! early-return control flow is kept, matching `u_get_headentry`/
//! `u_getbot`'s own established treatment above); `undo_allowed`
//! (needed `ex_docmd.c`'s `expr_map_locked`, harvested into a brand new
//! `src/ex_docmd.rs` - a tiny, deliberate one-function slice of that
//! ~8600-line file, see its own module doc); **`u_savecommon`, the
//! real "start a new undo save point" entry point, previously flagged
//! across many earlier sessions as needing its own dedicated pass** -
//! re-investigated and found `u_freeheader`/`u_freebranch` (its
//! own long-cited blockers) were ALREADY translated (from an earlier
//! session, simply never circled back to) - every other dependency
//! (`change_warning`, `os_time`, `virtual_active`, `getviscol`,
//! `zero_fmark_additional_data` - a small new private helper -
//! `u_get_headentry`/`u_getbot`/`u_save_line_buf`) also already
//! existed. The size==1 "reuse an already-saved single-line entry"
//! optimization (searching up to the last 10 entries) is translated
//! using `Vec::remove`/`Vec::insert(0, ...)` in place of the original's
//! `ue_next`-pointer-splicing "move to the head" surgery - sound
//! because this crate's own established convention (see
//! `u_get_headentry`'s doc comment) already treats index 0 as "the
//! head"/most-recently-added entry, exactly matching where a
//! newly-spliced-to-the-front entry would land. The interrupt-check
//! inside the line-copy loop (`fast_breakcheck()`+`got_int`) is
//! simplified to a direct `got_int` check every iteration (documented
//! on `u_savecommon` itself: behaviorally identical today, since
//! nothing translated can set `got_int` asynchronously mid-loop yet -
//! no signal handler, no terminal input polling). Also added the
//! thin `u_save_buf`/`u_save`/`u_save_cursor`/`u_savesub`/`u_inssub`/
//! `u_savedel` wrapper family (all mechanical, no design freedom of
//! their own) now that `u_savecommon` is real.
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `u_check_tree`/`u_check`: `#ifdef U_DEBUG`-only consistency
//!   checkers, need `emsg`/`smsg`/`semsg` (`message.c`) and the
//!   debug-only `uh_magic`/`ue_magic` fields (no equivalent debug-build
//!   concept established in this crate yet, same reasoning as
//!   marktree.rs's deferred `mt_inspect*` debug functions).
//! - `u_undo*`/`u_redo*`/`undo_time`/`u_doit`: the actual undo/redo
//!   state-machine entry points (distinct from the now-translated
//!   `u_save*` family, which only *records* undo information) - need
//!   `autocmd.c` triggers plus a substantial amount of cursor/screen
//!   restoration logic - not (re-)examined in detail yet.
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
//! - `u_undoline` (the "U" command's actual undo/redo-toggle logic,
//!   distinct from the now-translated `u_saveline` save-side helper):
//!   needs `extmark_splice_cols` (the extmark subsystem, not yet
//!   translated), `changed_bytes` (`change.c`, not yet translated
//!   beyond `file_ff_differs`), `check_cursor_col` (`cursor.c`, not
//!   yet translated), and `beep_flush` (the message/display
//!   subsystem). `u_savecommon` itself is no longer this function's
//!   blocker now that it's real.
//! - `ex_undolist`: needs the real message-display pipeline
//!   (`msg_start`/`msg_puts_hl`/`msg_putchar`/`msg_end`/`msg`,
//!   `message.c`, not tractable) - `exarg_T` existing isn't enough to
//!   unblock this one, unlike its sibling `ex_undojoin` above.


use crate::buffer_defs::BufT;
use crate::globals::GlobalCell;
use crate::mark_defs::{FmarkT, NMARKS};
use crate::pos_defs::LinenrT;
use crate::undo_defs::{uh_flags, UEntry, UHeader, UhLink};

/// Repeat the previous undo/redo when `undo_undoes` is set, used by
/// `u_savecommon`/`u_doit`. File-static in the original (`undo_undoes`).
static UNDO_UNDOES: GlobalCell<bool> = GlobalCell::new(false);

/// Return true when undo is allowed. Otherwise (real message display
/// skipped - see this function's own doc comment) return false
/// (`undo_allowed`).
///
/// The original's 3 `emsg(_(...))` calls (`e_modifiable`/`e_sandbox`/
/// `e_textlock`) are all real, reachable user-facing errors - not
/// internal-invariant checks - but message *display* is skipped
/// (`message.c`'s pipeline is still not tractable), matching the
/// established `u_get_headentry`/`u_getbot`/`ex_undojoin` policy: the
/// exact same early-return control flow (and thus the exact same
/// `false` return value) is kept.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` (touched transitively via `expr_map_locked`).
#[must_use]
pub unsafe fn undo_allowed(buf: &BufT) -> bool {
    // Don't allow changes when 'modifiable' is off.
    if buf.b_p_ma == 0 {
        // emsg(_(e_modifiable)) omitted - see this function's own doc.
        return false;
    }

    // In the sandbox it's not allowed to change the text.
    if unsafe { crate::globals::GLOBALS.get_mut() }.sandbox != 0 {
        // emsg(_(e_sandbox)) omitted - see this function's own doc.
        return false;
    }

    // Don't allow changes in the buffer while editing the cmdline. The
    // caller of getcmdline() may get confused.
    if unsafe { crate::globals::GLOBALS.get_mut() }.textlock != 0
        // SAFETY: forwarded from this function's own safety doc.
        || unsafe { crate::ex_docmd::expr_map_locked() }
    {
        // emsg(_(e_textlock)) omitted - see this function's own doc.
        return false;
    }

    true
}

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

/// Allocate memory and copy line `lnum` from `buf` into it
/// (`u_save_line_buf`).
///
/// The original's separate allocation step (`xstrdup(ml_get_buf(...))`)
/// is redundant here: [`crate::memline::ml_get_buf`] already returns a
/// freshly-owned `Vec<u8>` (this crate's own established convention -
/// see `memline.rs`'s own module doc comment for why - rather than the
/// original's transient, cache-aliasing `char *`), so no separate copy
/// is needed.
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT`.
#[must_use]
pub unsafe fn u_save_line_buf(buf: &mut BufT, lnum: LinenrT) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::ml_get_buf(buf, lnum) }
}

/// Allocate memory and copy `curbuf`'s line `lnum` into it
/// (`u_save_line`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`. Also see [`u_save_line_buf`]'s own safety doc.
#[must_use]
pub unsafe fn u_save_line(lnum: LinenrT) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { u_save_line_buf(curbuf, lnum) }
}

/// Save the line `lnum` for the "U" command (`u_saveline`, `static` in
/// the original - kept private here too).
///
/// Its only real caller in the original, `u_undoline`, is deferred
/// (needs `u_savecommon`/`extmark_splice_cols`/`changed_bytes`/
/// `check_cursor_col`/`beep_flush` - see this module's own doc
/// comment) - `#[allow(dead_code)]` until that lands, matching the
/// precedent already established for `marktree.rs`'s `itr_eq`.
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT`. Also see [`u_save_line_buf`]'s own safety doc.
#[allow(dead_code)]
unsafe fn u_saveline(buf: &mut BufT, lnum: LinenrT) {
    if lnum == buf.b_u_line_lnum {
        return; // line is already saved
    }
    if lnum < 1 || lnum > buf.b_ml.ml_line_count {
        return; // should never happen
    }
    u_clearline(buf);
    buf.b_u_line_lnum = lnum;
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &*crate::globals::GLOBALS.get_mut().curwin };
    if std::ptr::eq(curwin.w_buffer, buf) && curwin.w_cursor.lnum == lnum {
        buf.b_u_line_colnr = curwin.w_cursor.col;
    } else {
        buf.b_u_line_colnr = 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    buf.b_u_line_ptr = Some(unsafe { u_save_line_buf(buf, lnum) });
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

/// Get the `'undolevels'` value for the current buffer (`get_undolevel`).
///
/// Touches `crate::option_vars::OPTION_VARS` for the global fallback.
fn get_undolevel(buf: &BufT) -> crate::types_defs::OptInt {
    if buf.b_p_ul == crate::option_vars::NO_LOCAL_UNDOLEVEL {
        // SAFETY: OPTION_VARS is always initialized before any editor
        // code runs, matching every other OPTION_VARS access in this
        // crate.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul
    } else {
        buf.b_p_ul
    }
}

/// Get the index (in place of the original's raw pointer - see
/// `uh_entries`'s own doc comment) of the last added entry. If it's not
/// valid, give an error message and return `None` (`u_get_headentry`).
///
/// The original's `iemsg(_(e_undo_list_corrupt))` call is omitted here
/// (`message.c`'s display pipeline is still not tractable - see this
/// module's own doc comment), matching the established
/// `mf_write`/`ml_open`/`ml_find_line` policy: the *state*/return value
/// stays exactly as faithful as the original's (`None`, matching its
/// `NULL`), only the user-visible message text is skipped.
fn u_get_headentry(buf: &BufT) -> Option<usize> {
    if buf.b_u_newhead.is_null() {
        return None;
    }
    // SAFETY: b_u_newhead was just checked non-null above, and every
    // live UHeader pointer in this crate is a valid Box::into_raw
    // allocation (see u_freeheader's own safety doc).
    let uh = unsafe { &*buf.b_u_newhead };
    // uh_entries.is_empty() stands in for the original's `uh_entry ==
    // NULL`, matching the same uh_entries.first()-as-uh_entry
    // convention already established in u_find_first_changed.
    if uh.uh_entries.is_empty() {
        return None;
    }
    Some(0)
}

/// Compute the line number of the previous `u_save()`'s `ue_bot`. Only
/// called when `b_u_synced` is false (`u_getbot`).
fn u_getbot(buf: &mut BufT) {
    if u_get_headentry(buf).is_none() {
        // check for corrupt undo list
        return;
    }

    // SAFETY: u_get_headentry just confirmed buf.b_u_newhead is
    // non-null and points at a live UHeader.
    let uh = unsafe { &mut *buf.b_u_newhead };
    if let Some(idx) = uh.uh_getbot_entry {
        // The new ue_bot is computed from the number of lines that has
        // been inserted (0 - deleted) since calling u_save. This is
        // equal to the old line count subtracted from the current
        // line count.
        let ue_lcount = uh.uh_entries[idx].ue_lcount;
        let ue_top = uh.uh_entries[idx].ue_top;
        let ue_size = uh.uh_entries[idx].ue_array.len() as LinenrT;
        let extra = buf.b_ml.ml_line_count - ue_lcount;
        let mut ue_bot = ue_top + ue_size + 1 + extra;
        if ue_bot < 1 || ue_bot > buf.b_ml.ml_line_count {
            // iemsg(_(e_undo_line_missing)) omitted - see this
            // function's own doc comment above. Assume all lines
            // deleted, will get all the old lines back without
            // deleting the current ones.
            ue_bot = ue_top + 1;
        }
        uh.uh_entries[idx].ue_bot = ue_bot;

        uh.uh_getbot_entry = None;
    }

    buf.b_u_synced = true;
}

/// Stop adding to the current entry list (`u_sync`).
///
/// @param force  if true, also sync when `no_u_sync` is set.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
pub unsafe fn u_sync(force: bool) {
    // Skip it when already synced or syncing is disabled.
    // SAFETY: forwarded from this function's own safety doc.
    let no_u_sync = unsafe { crate::globals::GLOBALS.get_mut() }.no_u_sync;
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    if curbuf.b_u_synced || (!force && no_u_sync > 0) {
        return;
    }

    if get_undolevel(curbuf) < 0 {
        curbuf.b_u_synced = true; // no entries, nothing to do
    } else {
        u_getbot(curbuf); // compute ue_bot of previous u_save
        curbuf.b_u_curhead = std::ptr::null_mut();
    }
}

/// Clear each named/Visual-marker `additional_data` field (ShaDa
/// "extra data") before copying `fmarks` into a new undo header
/// (`zero_fmark_additional_data`).
fn zero_fmark_additional_data(fmarks: &mut [FmarkT; NMARKS as usize]) {
    for fmark in fmarks.iter_mut() {
        fmark.additional_data = None;
    }
}

/// Common code for various ways to save text before a change. `top` is
/// the line above the first changed line. `bot` is the line below the
/// last changed line. `newbot` is the new bottom line; use zero when
/// not known. `reload` is true when saving for a buffer reload.
/// Careful: may trigger autocommands that reload the buffer. Returns
/// [`crate::vim_defs::FAIL`] when lines could not be saved,
/// [`crate::vim_defs::OK`] otherwise (`u_savecommon`).
///
/// The original's `emsg(_("E881: Line count changed unexpectedly"))`
/// is a real, reachable user-facing error - its message display is
/// omitted (`message.c`'s pipeline still not tractable), but the exact
/// same early `FAIL` return is kept, matching this file's established
/// policy.
///
/// `fast_breakcheck()`'s own counter-based throttling (checking a real
/// pending-interrupt flag only once every `BREAKCHECK_SKIP * 10`
/// iterations, as a performance optimization over calling
/// `os_breakcheck()` - itself needing `os/input.c`'s terminal input
/// polling, still event-loop-bound and not tractable) is simplified to
/// checking `GLOBALS.got_int` directly on every iteration instead.
/// This is behaviorally identical in this crate today: nothing
/// currently translated can set `got_int` true *during* this loop
/// (no signal handler, no terminal input polling exists yet) - it can
/// only already be `true` beforehand (e.g. a test setting it up ahead
/// of the call), in which case checking every iteration vs. every N
/// iterations produces the exact same "fails on the very first
/// iteration" result.
///
/// # Safety
/// `buf` must be reachable the same way every other function in this
/// crate assumes: `crate::globals::GLOBALS.curbuf`/`curwin` must be
/// valid, non-null pointers to live structures (touched transitively
/// via `undo_allowed`/`change_warning`/`getviscol`). Every live
/// [`UHeader`] reachable from `buf`'s undo tree must be a valid
/// `Box::into_raw` allocation (see [`u_freeheader`]'s own safety doc) -
/// `buf.b_u_newhead` specifically must be non-null whenever
/// `buf.b_u_synced` is false (an invariant the original itself
/// maintains: `b_u_synced` only ever becomes false right after a
/// header is created, a few lines into this same function, or by
/// `u_sync`/`u_undo`/`u_redo`, none of which leave `b_u_newhead` null
/// while `b_u_synced` is false).
pub unsafe fn u_savecommon(
    buf: &mut BufT,
    top: LinenrT,
    bot: LinenrT,
    newbot: LinenrT,
    reload: bool,
) -> i32 {
    if !reload {
        // When making changes is not allowed return FAIL. It's a crude
        // way to make all change commands fail.
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { undo_allowed(buf) } {
            return crate::vim_defs::FAIL;
        }

        // Saving text for undo means we are going to make a change.
        // Give a warning for a read-only file before making the
        // change, so that the FileChangedRO event can replace the
        // buffer with a read-write version (e.g. obtained from a
        // source control system).
        // SAFETY: forwarded from this function's own safety doc.
        let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        if std::ptr::eq(buf as *const BufT, curbuf.cast_const()) {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::change::change_warning(buf, 0) };
        }

        if bot > buf.b_ml.ml_line_count + 1 {
            // This happens when the FileChangedRO autocommand changes
            // the file in a way it becomes shorter.
            // emsg(_("E881: Line count changed unexpectedly")) omitted
            // - see this function's own doc comment.
            return crate::vim_defs::FAIL;
        }
    }

    let size = bot - top - 1;

    // If buf.b_u_synced is true make a new header.
    if buf.b_u_synced {
        // Need to create new entry in b_changelist.
        buf.b_new_change = true;

        // Make a new header entry. Do this first so that we don't
        // mess up the undo info when out of memory.
        let uhp: *mut UHeader = if get_undolevel(buf) >= 0 {
            Box::into_raw(Box::new(UHeader::default()))
        } else {
            std::ptr::null_mut()
        };

        // If we undid more than we redid, move the entry lists before
        // and including buf.b_u_curhead to an alternate branch.
        let mut old_curhead = buf.b_u_curhead;
        if !old_curhead.is_null() {
            // SAFETY: old_curhead is non-null, and per this function's
            // own safety doc, a valid, currently-linked UHeader.
            buf.b_u_newhead = unsafe { (*old_curhead).uh_next.ptr() };
            buf.b_u_curhead = std::ptr::null_mut();
        }

        // Free headers to keep the size right.
        while i64::from(buf.b_u_numhead) > get_undolevel(buf) && !buf.b_u_oldhead.is_null() {
            let mut uhfree = buf.b_u_oldhead;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                if uhfree == old_curhead {
                    // Can't reconnect the branch, delete all of it.
                    u_freebranch(buf, uhfree, &mut old_curhead);
                } else if (*uhfree).uh_alt_next.ptr().is_null() {
                    // There is no branch, only free one header.
                    u_freeheader(buf, uhfree, &mut old_curhead);
                } else {
                    // Free the oldest alternate branch as a whole.
                    while !(*uhfree).uh_alt_next.ptr().is_null() {
                        uhfree = (*uhfree).uh_alt_next.ptr();
                    }
                    u_freebranch(buf, uhfree, &mut old_curhead);
                }
            }
        }

        let Some(uhp) = (if uhp.is_null() { None } else { Some(uhp) }) else {
            // no undo at all
            if !old_curhead.is_null() {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { u_freebranch(buf, old_curhead, std::ptr::null_mut()) };
            }
            buf.b_u_synced = false;
            return crate::vim_defs::OK;
        };

        // SAFETY: uhp was just allocated via Box::into_raw above and is
        // not yet reachable from anywhere else - every field is
        // explicitly set below (matching the original's own explicit
        // field-by-field initialization of a freshly-xmalloc'd
        // uhp, none of it relying on the allocator's garbage content).
        unsafe {
            (*uhp).uh_prev = UhLink::Ptr(std::ptr::null_mut());
            (*uhp).uh_next = UhLink::Ptr(buf.b_u_newhead);
            (*uhp).uh_alt_next = UhLink::Ptr(old_curhead);
            if !old_curhead.is_null() {
                (*uhp).uh_alt_prev = (*old_curhead).uh_alt_prev;

                let alt_prev = (*uhp).uh_alt_prev.ptr();
                if !alt_prev.is_null() {
                    (*alt_prev).uh_alt_next = UhLink::Ptr(uhp);
                }

                (*old_curhead).uh_alt_prev = UhLink::Ptr(uhp);

                if buf.b_u_oldhead == old_curhead {
                    buf.b_u_oldhead = uhp;
                }
            } else {
                (*uhp).uh_alt_prev = UhLink::Ptr(std::ptr::null_mut());
            }

            if !buf.b_u_newhead.is_null() {
                (*buf.b_u_newhead).uh_prev = UhLink::Ptr(uhp);
            }

            buf.b_u_seq_last += 1;
            (*uhp).uh_seq = buf.b_u_seq_last;
            buf.b_u_seq_cur = (*uhp).uh_seq;
            let now = crate::os::time::os_time();
            (*uhp).uh_time = now;
            (*uhp).uh_save_nr = 0;
            buf.b_u_time_cur = now + 1;

            (*uhp).uh_walk = 0;
            // uh_entries/uh_getbot_entry already match uh_entry = NULL
            // via UHeader::default() (empty Vec / None).
            let curwin = &mut *crate::globals::GLOBALS.get_mut().curwin;
            (*uhp).uh_cursor = curwin.w_cursor; // save cursor pos. for undo
            if crate::state::virtual_active(curwin) && curwin.w_cursor.coladd > 0 {
                (*uhp).uh_cursor_vcol = crate::cursor::getviscol();
            } else {
                (*uhp).uh_cursor_vcol = -1;
            }

            // save changed and buffer empty flag for undo
            (*uhp).uh_flags = (if buf.b_changed != 0 { uh_flags::CHANGED } else { 0 })
                + (if buf.b_ml.ml_flags & crate::memline_defs::ML_EMPTY != 0 {
                    uh_flags::EMPTYBUF
                } else {
                    0
                });

            // save named marks and Visual marks for undo
            zero_fmark_additional_data(&mut buf.b_namedm);
            (*uhp).uh_namedm = buf.b_namedm.to_vec();
            (*uhp).uh_visual = buf.b_visual;
        }

        buf.b_u_newhead = uhp;

        if buf.b_u_oldhead.is_null() {
            buf.b_u_oldhead = uhp;
        }
        buf.b_u_numhead += 1;
    } else {
        if get_undolevel(buf) < 0 {
            // no undo at all
            return crate::vim_defs::OK;
        }

        // When saving a single line, and it has been saved just
        // before, it doesn't make sense saving it again. Saves a lot
        // of memory when making lots of changes inside the same line.
        // This is only possible if the previous change didn't
        // increase or decrease the number of lines. Check the ten
        // last changes. More doesn't make sense and takes too long.
        if size == 1 {
            // SAFETY: forwarded from this function's own safety doc
            // (buf.b_u_newhead is non-null whenever buf.b_u_synced is
            // false).
            let found = {
                let uh = unsafe { &*buf.b_u_newhead };
                let mut found = None;
                for i in 0..uh.uh_entries.len().min(10) {
                    let ue = &uh.uh_entries[i];
                    let ue_size = ue.ue_array.len() as LinenrT;
                    let ue_top = ue.ue_top;
                    let ue_bot = ue.ue_bot;
                    let ue_lcount = ue.ue_lcount;

                    // If lines have been inserted/deleted we give up.
                    // Also when the line was included in a multi-line
                    // save.
                    let bail = if uh.uh_getbot_entry != Some(i) {
                        (ue_top + ue_size + 1)
                            != (if ue_bot == 0 { buf.b_ml.ml_line_count + 1 } else { ue_bot })
                    } else {
                        ue_lcount != buf.b_ml.ml_line_count
                    } || (ue_size > 1
                        && top >= ue_top
                        && top + 2 <= ue_top + ue_size + 1);

                    if bail {
                        break;
                    }

                    // If it's the same line we can skip saving it again.
                    if ue_size == 1 && ue_top == top {
                        found = Some(i);
                        break;
                    }
                }
                found
            };

            if let Some(i) = found {
                if i > 0 {
                    // It's not the last entry: get ue_bot for the last
                    // entry now. Following deleted/inserted lines go
                    // to the re-used entry.
                    u_getbot(buf);
                    buf.b_u_synced = false;

                    // Move the found entry to become the last entry
                    // (index 0, matching this crate's established
                    // "index 0 == most-recently-added" convention -
                    // see u_get_headentry's own doc comment). The
                    // order of undo/redo doesn't matter for the
                    // entries it moves over, since they don't change
                    // the line count and don't include this line.
                    // SAFETY: forwarded from this function's own
                    // safety doc.
                    let uh = unsafe { &mut *buf.b_u_newhead };
                    let entry = uh.uh_entries.remove(i);
                    uh.uh_entries.insert(0, entry);
                }

                // The executed command may change the line count.
                // SAFETY: forwarded from this function's own safety
                // doc.
                let uh = unsafe { &mut *buf.b_u_newhead };
                if newbot != 0 {
                    uh.uh_entries[0].ue_bot = newbot;
                } else if bot > buf.b_ml.ml_line_count {
                    uh.uh_entries[0].ue_bot = 0;
                } else {
                    uh.uh_entries[0].ue_lcount = buf.b_ml.ml_line_count;
                    uh.uh_getbot_entry = Some(0);
                }
                return crate::vim_defs::OK;
            }
        }

        // find line number for ue_bot for previous u_save()
        u_getbot(buf);
    }

    // Add lines in front of entry list.
    let mut new_entry = UEntry { ue_top: top, ..UEntry::default() };
    let mut set_getbot_entry = false;
    if newbot != 0 {
        new_entry.ue_bot = newbot;
        // Use 0 for ue_bot if bot is below last line. Otherwise we
        // have to compute ue_bot later.
    } else if bot > buf.b_ml.ml_line_count {
        new_entry.ue_bot = 0;
    } else {
        new_entry.ue_lcount = buf.b_ml.ml_line_count;
        set_getbot_entry = true;
    }

    if size > 0 {
        for lnum in (top + 1..).take(size as usize) {
            // SAFETY: forwarded from this function's own safety doc -
            // see this function's own doc comment for why checking
            // got_int directly (without fast_breakcheck's throttling
            // counter) is faithful here.
            if unsafe { crate::globals::GLOBALS.get_mut() }.got_int {
                // u_freeentry(uep, i) in the original - here, simply
                // returning drops new_entry (and whatever partial
                // lines its ue_array already holds), matching the
                // original's own per-entry free exactly.
                return crate::vim_defs::FAIL;
            }
            // SAFETY: forwarded from this function's own safety doc.
            new_entry.ue_array.push(unsafe { u_save_line_buf(buf, lnum) });
        }
    }

    // SAFETY: forwarded from this function's own safety doc
    // (buf.b_u_newhead is non-null on both paths reaching here: the
    // just-created header above, or the pre-existing one on the
    // "reused entry didn't match" branch).
    let uh = unsafe { &mut *buf.b_u_newhead };
    uh.uh_entries.insert(0, new_entry);
    if set_getbot_entry {
        uh.uh_getbot_entry = Some(0);
    }
    if reload {
        // buffer was reloaded, notify text change subscribers
        uh.uh_flags |= uh_flags::RELOAD;
    }
    buf.b_u_synced = false;
    // SAFETY: UNDO_UNDOES is a private, crate-internal GlobalCell.
    unsafe { *UNDO_UNDOES.get_mut() = false };

    crate::vim_defs::OK
}

/// Save the lines between `top` and `bot` for both the `"u"` and `"U"`
/// command. `top` may be 0 and `bot` may be
/// `buf.b_ml.ml_line_count + 1`. Careful: may trigger autocommands
/// that reload the buffer. Returns `FAIL` when lines could not be
/// saved, `OK` otherwise (`u_save_buf`).
///
/// # Safety
/// Same as [`u_savecommon`].
pub unsafe fn u_save_buf(buf: &mut BufT, top: LinenrT, bot: LinenrT) -> i32 {
    if top >= bot || bot > buf.b_ml.ml_line_count + 1 {
        return crate::vim_defs::FAIL; // rely on caller to do error messages
    }

    if top + 2 == bot {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { u_saveline(buf, top + 1) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { u_savecommon(buf, top, bot, 0, false) }
}

/// Save the lines between `top` and `bot`, for `GLOBALS.curbuf`
/// (`u_save`).
///
/// # Safety
/// Same as [`u_save_buf`], plus `crate::globals::GLOBALS.curbuf` must
/// be a valid, non-null pointer to a live `BufT`.
pub unsafe fn u_save(top: LinenrT, bot: LinenrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { u_save_buf(curbuf, top, bot) }
}

/// Save the current line and both lines adjacent to the cursor, for
/// the `"o"`/`"O"` commands and the like. Careful: may trigger
/// autocommands that reload the buffer. Returns `OK` or `FAIL`
/// (`u_save_cursor`).
///
/// # Safety
/// Same as [`u_save`], plus `crate::globals::GLOBALS.curwin` must be a
/// valid, non-null pointer to a live `WinT`.
pub unsafe fn u_save_cursor() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let cur = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.lnum;
    let top = if cur > 0 { cur - 1 } else { 0 };
    let bot = cur + 1;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { u_save(top, bot) }
}

/// Save the line `lnum` (used by `":s"` and `"~"` command). The line
/// is replaced, so the new bottom line is `lnum + 1`. Careful: may
/// trigger autocommands that reload the buffer. Returns `FAIL` when
/// lines could not be saved, `OK` otherwise (`u_savesub`).
///
/// # Safety
/// Same as [`u_savecommon`], plus `crate::globals::GLOBALS.curbuf`
/// must be a valid, non-null pointer to a live `BufT`.
pub unsafe fn u_savesub(lnum: LinenrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { u_savecommon(curbuf, lnum - 1, lnum + 1, lnum + 1, false) }
}

/// A new line is inserted before line `lnum` (used by `":s"` command).
/// The line is inserted, so the new bottom line is `lnum + 1`.
/// Careful: may trigger autocommands that reload the buffer. Returns
/// `FAIL` when lines could not be saved, `OK` otherwise (`u_inssub`).
///
/// # Safety
/// Same as [`u_savesub`].
pub unsafe fn u_inssub(lnum: LinenrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { u_savecommon(curbuf, lnum - 1, lnum, lnum + 1, false) }
}

/// Save the lines `lnum` - `lnum + nlines` (used by delete command).
/// The lines are deleted, so the new bottom line is `lnum`, unless the
/// buffer becomes empty. Careful: may trigger autocommands that
/// reload the buffer. Returns `FAIL` when lines could not be saved,
/// `OK` otherwise (`u_savedel`).
///
/// # Safety
/// Same as [`u_savesub`].
pub unsafe fn u_savedel(lnum: LinenrT, nlines: LinenrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    let newbot = if nlines == curbuf.b_ml.ml_line_count { 2 } else { lnum };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { u_savecommon(curbuf, lnum - 1, lnum + nlines, newbot, false) }
}

/// `":undojoin"`: join the next change with the previous undo entry,
/// so they undo together (`ex_undojoin`). Now tractable now that
/// `crate::ex_cmds_defs::ExargT` exists.
///
/// The original's `emsg(_("E790: undojoin is not allowed after undo"))`
/// is a real, reachable user-facing error (not an internal-invariant
/// check) - its message display is omitted (`message.c`'s display
/// pipeline is still not tractable), but the identical control-flow
/// effect (return without unsyncing) is kept exactly, matching this
/// crate's established "skip the display, keep the state" policy.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
pub unsafe fn ex_undojoin(_eap: &crate::ex_cmds_defs::ExargT) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    if curbuf.b_u_newhead.is_null() {
        return; // nothing changed before
    }
    if !curbuf.b_u_curhead.is_null() {
        // emsg(_("E790: undojoin is not allowed after undo")) omitted -
        // see this function's own doc comment.
        return;
    }
    if !curbuf.b_u_synced {
        return; // already unsynced
    }
    if get_undolevel(curbuf) < 0 {
        return; // no entries, nothing to do
    }
    curbuf.b_u_synced = false; // Append next change to last entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pos_defs::PosT;
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

    #[test]
    fn u_save_line_buf_returns_the_exact_line_bytes() {
        // buf_with_two_lines() calls ml_open, which touches shared
        // GLOBALS.got_int internally via mf_sync - must hold the lock
        // like every other GlobalCell-touching test.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_two_lines();
        assert_eq!(unsafe { u_save_line_buf(&mut buf, 1) }, b"one\0".to_vec());
        assert_eq!(unsafe { u_save_line_buf(&mut buf, 2) }, b"two\0".to_vec());
        close_buf_with_memline(buf);
    }

    #[test]
    fn u_save_line_matches_u_save_line_buf_via_curbuf() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_two_lines();
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        assert_eq!(unsafe { u_save_line(2) }, b"two\0".to_vec());

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        close_buf_with_memline(buf);
    }

    #[test]
    fn u_saveline_is_noop_when_line_already_saved() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_two_lines();
        buf.b_u_line_lnum = 1;
        buf.b_u_line_ptr = Some(b"sentinel\0".to_vec());
        buf.b_u_line_colnr = 42;

        unsafe { u_saveline(&mut buf, 1) };

        // unchanged: lnum matched buf.b_u_line_lnum, early return.
        assert_eq!(buf.b_u_line_ptr, Some(b"sentinel\0".to_vec()));
        assert_eq!(buf.b_u_line_colnr, 42);
        close_buf_with_memline(buf);
    }

    #[test]
    fn u_saveline_is_noop_for_out_of_range_lnum() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_two_lines();

        unsafe { u_saveline(&mut buf, 0) };
        assert!(buf.b_u_line_ptr.is_none());
        unsafe { u_saveline(&mut buf, 99) };
        assert!(buf.b_u_line_ptr.is_none());

        close_buf_with_memline(buf);
    }

    #[test]
    fn u_saveline_sets_colnr_to_zero_when_curwin_does_not_match() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_two_lines();
        // w_buffer intentionally left null - does not match `buf`.
        let mut win = crate::buffer_defs::WinT {
            w_cursor: PosT { lnum: 2, col: 7, coladd: 0 },
            ..Default::default()
        };

        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut crate::buffer_defs::WinT;

        unsafe { u_saveline(&mut buf, 2) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;

        assert_eq!(buf.b_u_line_lnum, 2);
        assert_eq!(buf.b_u_line_colnr, 0);
        assert_eq!(buf.b_u_line_ptr, Some(b"two\0".to_vec()));
        close_buf_with_memline(buf);
    }

    #[test]
    fn u_saveline_uses_cursor_col_when_curwin_matches() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_two_lines();
        let mut win = crate::buffer_defs::WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 2, col: 7, coladd: 0 },
            ..Default::default()
        };

        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut crate::buffer_defs::WinT;

        unsafe { u_saveline(&mut buf, 2) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;

        assert_eq!(buf.b_u_line_lnum, 2);
        assert_eq!(buf.b_u_line_colnr, 7);
        assert_eq!(buf.b_u_line_ptr, Some(b"two\0".to_vec()));
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_undolevel_uses_buffer_local_value_when_set() {
        let buf = BufT { b_p_ul: 5, ..Default::default() };
        assert_eq!(get_undolevel(&buf), 5);
    }

    #[test]
    fn get_undolevel_falls_back_to_global_when_no_local_value() {
        let _lock = crate::globals::global_state_test_lock();
        let buf = BufT {
            b_p_ul: crate::option_vars::NO_LOCAL_UNDOLEVEL,
            ..Default::default()
        };
        let prev_p_ul = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul = 42;

        assert_eq!(get_undolevel(&buf), 42);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul = prev_p_ul;
    }

    #[test]
    fn u_get_headentry_none_when_newhead_is_null() {
        let buf = BufT::default();
        assert_eq!(u_get_headentry(&buf), None);
    }

    #[test]
    fn u_get_headentry_none_when_entries_empty() {
        let uhp = new_header(); // UHeader::default() has an empty uh_entries
        let buf = BufT { b_u_newhead: uhp, ..Default::default() };
        assert_eq!(u_get_headentry(&buf), None);
        unsafe {
            drop(Box::from_raw(uhp));
        }
    }

    #[test]
    fn u_get_headentry_some_when_entries_present() {
        let uhp = new_header();
        unsafe {
            (*uhp).uh_entries.push(UEntry {
                ue_top: 0,
                ue_bot: 0,
                ue_lcount: 0,
                ue_array: vec![b"one\0".to_vec()],
            });
        }
        let buf = BufT { b_u_newhead: uhp, ..Default::default() };
        assert_eq!(u_get_headentry(&buf), Some(0));
        unsafe {
            drop(Box::from_raw(uhp));
        }
    }

    #[test]
    fn u_getbot_noop_when_no_getbot_entry_pending() {
        let uhp = new_header();
        unsafe {
            (*uhp).uh_entries.push(UEntry {
                ue_top: 0,
                ue_bot: 0,
                ue_lcount: 0,
                ue_array: vec![b"one\0".to_vec()],
            });
            // uh_getbot_entry left at its default None.
        }
        let mut buf = BufT {
            b_u_newhead: uhp,
            b_u_synced: false,
            ..Default::default()
        };

        u_getbot(&mut buf);

        // Still marked synced (the original always sets b_u_synced =
        // true at the very end, regardless of whether uh_getbot_entry
        // was set), but the entry itself is untouched.
        assert!(buf.b_u_synced);
        assert_eq!(unsafe { (&(*uhp).uh_entries)[0].ue_bot }, 0);
        unsafe {
            drop(Box::from_raw(uhp));
        }
    }

    #[test]
    fn u_getbot_computes_bot_from_line_count_delta() {
        let uhp = new_header();
        unsafe {
            // ue_top=0, ue_size (ue_array.len())=2, ue_lcount=3 (line
            // count at the time u_save was called - i.e. the buffer
            // already had a 3rd line beyond the 2 saved ones); the
            // buffer has since gained one more line (now 4), so
            // extra=4-3=1 and ue_bot = 0 + 2 + 1 + 1 = 4, which is
            // exactly ml_line_count - in range.
            (*uhp).uh_entries.push(UEntry {
                ue_top: 0,
                ue_bot: 0,
                ue_lcount: 3,
                ue_array: vec![b"one\0".to_vec(), b"two\0".to_vec()],
            });
            (*uhp).uh_getbot_entry = Some(0);
        }
        let mut buf = BufT {
            b_u_newhead: uhp,
            b_u_synced: false,
            ..Default::default()
        };
        buf.b_ml.ml_line_count = 4;

        u_getbot(&mut buf);

        assert_eq!(unsafe { (&(*uhp).uh_entries)[0].ue_bot }, 4);
        assert_eq!(unsafe { (*uhp).uh_getbot_entry }, None);
        assert!(buf.b_u_synced);
        unsafe {
            drop(Box::from_raw(uhp));
        }
    }

    #[test]
    fn u_getbot_falls_back_when_computed_bot_out_of_range() {
        let uhp = new_header();
        unsafe {
            // ue_top=0, ue_size=2, ue_lcount=2, and the buffer's line
            // count has since dropped to 1 (extra = 1 - 2 = -1), giving
            // ue_bot = 0 + 2 + 1 + (-1) = 2, which is > ml_line_count
            // (1) - out of range, so the original's own defensive
            // fallback (ue_bot = ue_top + 1) applies instead.
            (*uhp).uh_entries.push(UEntry {
                ue_top: 0,
                ue_bot: 0,
                ue_lcount: 2,
                ue_array: vec![b"one\0".to_vec(), b"two\0".to_vec()],
            });
            (*uhp).uh_getbot_entry = Some(0);
        }
        let mut buf = BufT {
            b_u_newhead: uhp,
            b_u_synced: false,
            ..Default::default()
        };
        buf.b_ml.ml_line_count = 1;

        u_getbot(&mut buf);

        assert_eq!(unsafe { (&(*uhp).uh_entries)[0].ue_bot }, 1); // ue_top + 1
        unsafe {
            drop(Box::from_raw(uhp));
        }
    }

    #[test]
    fn u_sync_noop_when_already_synced() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_u_synced: true, ..Default::default() };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        unsafe { u_sync(false) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        // b_u_curhead untouched (still null, not merely "still null
        // because it started that way" - verified no code ran by
        // constructing this test with b_u_synced already true).
        assert!(buf.b_u_curhead.is_null());
    }

    #[test]
    fn u_sync_noop_when_no_u_sync_set_and_not_forced() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_u_synced: false, ..Default::default() };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let prev_no_u_sync = unsafe { crate::globals::GLOBALS.get_mut() }.no_u_sync;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;
        unsafe { crate::globals::GLOBALS.get_mut() }.no_u_sync = 1;

        unsafe { u_sync(false) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.no_u_sync = prev_no_u_sync;
        assert!(!buf.b_u_synced); // untouched - bailed out early
    }

    #[test]
    fn u_sync_sets_synced_directly_when_undolevel_negative() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT {
            b_u_synced: false,
            b_p_ul: -1, // undo disabled for this buffer
            ..Default::default()
        };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let prev_no_u_sync = unsafe { crate::globals::GLOBALS.get_mut() }.no_u_sync;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;
        unsafe { crate::globals::GLOBALS.get_mut() }.no_u_sync = 0;

        unsafe { u_sync(false) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.no_u_sync = prev_no_u_sync;
        assert!(buf.b_u_synced);
        assert!(buf.b_u_curhead.is_null()); // u_getbot's path never ran
    }

    #[test]
    fn u_sync_calls_getbot_and_clears_curhead_when_undolevel_non_negative() {
        let _lock = crate::globals::global_state_test_lock();
        let uhp = new_header();
        unsafe {
            // ue_top=0, ue_size=1, ue_lcount=2 (>= ue_top+ue_size+1),
            // ml_line_count unchanged at 2: extra=0, ue_bot =
            // 0 + 1 + 1 + 0 = 2, exactly ml_line_count - in range.
            (*uhp).uh_entries.push(UEntry {
                ue_top: 0,
                ue_bot: 0,
                ue_lcount: 2,
                ue_array: vec![b"one\0".to_vec()],
            });
            (*uhp).uh_getbot_entry = Some(0);
        }
        let mut buf = BufT {
            b_u_synced: false,
            b_p_ul: 1000, // undo enabled
            b_u_newhead: uhp,
            b_u_curhead: uhp, // deliberately non-null, to prove u_sync clears it
            ..Default::default()
        };
        buf.b_ml.ml_line_count = 2;

        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let prev_no_u_sync = unsafe { crate::globals::GLOBALS.get_mut() }.no_u_sync;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;
        unsafe { crate::globals::GLOBALS.get_mut() }.no_u_sync = 0;

        unsafe { u_sync(false) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.no_u_sync = prev_no_u_sync;

        assert!(buf.b_u_synced);
        assert!(buf.b_u_curhead.is_null());
        // u_getbot really ran: ue_bot computed (0 + 1 + 1 + 0 = 2) and
        // uh_getbot_entry consumed.
        assert_eq!(unsafe { (&(*uhp).uh_entries)[0].ue_bot }, 2);
        assert_eq!(unsafe { (*uhp).uh_getbot_entry }, None);

        unsafe {
            drop(Box::from_raw(uhp));
        }
    }

    #[test]
    fn ex_undojoin_noop_when_no_prior_changes() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_u_newhead: std::ptr::null_mut(), b_u_synced: true, ..Default::default() };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        let eap = crate::ex_cmds_defs::ExargT::default();
        unsafe { ex_undojoin(&eap) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        assert!(buf.b_u_synced); // untouched
    }

    #[test]
    fn ex_undojoin_noop_when_curhead_non_null() {
        let _lock = crate::globals::global_state_test_lock();
        let uhp = new_header();
        let mut buf = BufT {
            b_u_newhead: uhp,
            b_u_curhead: uhp, // undo was performed
            b_u_synced: true,
            ..Default::default()
        };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        let eap = crate::ex_cmds_defs::ExargT::default();
        unsafe { ex_undojoin(&eap) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        assert!(buf.b_u_synced); // untouched - "E790" path, no state change
        unsafe { drop(Box::from_raw(uhp)) };
    }

    #[test]
    fn ex_undojoin_noop_when_already_unsynced() {
        let _lock = crate::globals::global_state_test_lock();
        let uhp = new_header();
        let mut buf = BufT {
            b_u_newhead: uhp,
            b_u_curhead: std::ptr::null_mut(),
            b_u_synced: false, // already unsynced
            ..Default::default()
        };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        let eap = crate::ex_cmds_defs::ExargT::default();
        unsafe { ex_undojoin(&eap) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        assert!(!buf.b_u_synced); // still false - nothing to do
        unsafe { drop(Box::from_raw(uhp)) };
    }

    #[test]
    fn ex_undojoin_noop_when_undolevel_negative() {
        let _lock = crate::globals::global_state_test_lock();
        let uhp = new_header();
        let mut buf = BufT {
            b_u_newhead: uhp,
            b_u_curhead: std::ptr::null_mut(),
            b_u_synced: true,
            b_p_ul: -1, // undo disabled for this buffer
            ..Default::default()
        };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        let eap = crate::ex_cmds_defs::ExargT::default();
        unsafe { ex_undojoin(&eap) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        assert!(buf.b_u_synced); // untouched
        unsafe { drop(Box::from_raw(uhp)) };
    }

    #[test]
    fn ex_undojoin_unsyncs_when_all_conditions_met() {
        let _lock = crate::globals::global_state_test_lock();
        let uhp = new_header();
        let mut buf = BufT {
            b_u_newhead: uhp,
            b_u_curhead: std::ptr::null_mut(),
            b_u_synced: true,
            b_p_ul: 1000, // undo enabled
            ..Default::default()
        };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        let eap = crate::ex_cmds_defs::ExargT::default();
        unsafe { ex_undojoin(&eap) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        assert!(!buf.b_u_synced); // flipped - next change joins the last entry
        unsafe { drop(Box::from_raw(uhp)) };
    }

    // ---- undo_allowed --------------------------------------------------

    #[test]
    fn undo_allowed_false_when_not_modifiable() {
        let _lock = crate::globals::global_state_test_lock();
        let buf = BufT { b_p_ma: 0, ..Default::default() };
        assert!(!unsafe { undo_allowed(&buf) });
    }

    #[test]
    fn undo_allowed_false_in_sandbox() {
        let _lock = crate::globals::global_state_test_lock();
        let buf = BufT { b_p_ma: 1, ..Default::default() };
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 1;
        let allowed = unsafe { undo_allowed(&buf) };
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;
        assert!(!allowed);
    }

    #[test]
    fn undo_allowed_false_when_textlock_set() {
        let _lock = crate::globals::global_state_test_lock();
        let buf = BufT { b_p_ma: 1, ..Default::default() };
        unsafe { crate::globals::GLOBALS.get_mut() }.textlock = 1;
        let allowed = unsafe { undo_allowed(&buf) };
        unsafe { crate::globals::GLOBALS.get_mut() }.textlock = 0;
        assert!(!allowed);
    }

    #[test]
    fn undo_allowed_false_when_expr_map_locked() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ma: 1, ..Default::default() };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;
        unsafe { crate::globals::GLOBALS.get_mut() }.expr_map_lock = 1;

        let allowed = unsafe { undo_allowed(&buf) };

        unsafe { crate::globals::GLOBALS.get_mut() }.expr_map_lock = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        assert!(!allowed);
    }

    #[test]
    fn undo_allowed_true_when_nothing_blocks_it() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ma: 1, ..Default::default() };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        let allowed = unsafe { undo_allowed(&buf) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        assert!(allowed);
    }

    // ---- zero_fmark_additional_data ------------------------------------

    #[test]
    fn zero_fmark_additional_data_clears_every_slot() {
        let mut marks: [FmarkT; NMARKS as usize] = std::array::from_fn(|_| FmarkT::default());
        marks[3].additional_data =
            Some(Box::new(crate::types_defs::AdditionalData { nitems: 0, nbytes: 0 }));
        zero_fmark_additional_data(&mut marks);
        assert!(marks.iter().all(|m| m.additional_data.is_none()));
    }

    // ---- u_savecommon ---------------------------------------------------

    /// Opens a real memline for a freshly-boxed `BufT`, points a
    /// freshly-boxed `WinT` at it, and sets `GLOBALS.curbuf`/`curwin`
    /// to both - restoring the previous values and freeing everything
    /// (including any undo tree left on the buffer, and the real
    /// memline itself) on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's entire lifetime
    /// (matches this file's own `CurbufGuard`-style established
    /// "compose with an externally-held lock" convention).
    struct TestBufWin {
        buf: *mut BufT,
        win: *mut crate::buffer_defs::WinT,
        prev_curbuf: *mut BufT,
        prev_curwin: *mut crate::buffer_defs::WinT,
    }

    impl TestBufWin {
        fn new() -> Self {
            let buf = Box::into_raw(Box::new(BufT::default()));
            assert_eq!(unsafe { crate::memline::ml_open(&mut *buf) }, crate::vim_defs::OK);
            // A real freshly-opened buffer starts with b_u_synced ==
            // true (no pending unsynced change yet) - BufT::default()
            // itself defaults it to false (matching this crate's
            // established "Default mirrors raw C zero-init, not the
            // real post-open state" convention, same category as
            // b_p_ma) - set explicitly here so every test built on
            // this helper starts from the real invariant, rather than
            // needing to remember to set it individually everywhere.
            unsafe { &mut *buf }.b_u_synced = true;
            let win = Box::into_raw(Box::new(crate::buffer_defs::WinT {
                w_buffer: buf,
                ..Default::default()
            }));

            let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = buf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = win;
            TestBufWin { buf, win, prev_curbuf, prev_curwin }
        }

        fn buf(&mut self) -> &mut BufT {
            unsafe { &mut *self.buf }
        }
    }

    impl Drop for TestBufWin {
        fn drop(&mut self) {
            unsafe {
                crate::globals::GLOBALS.get_mut().curbuf = self.prev_curbuf;
                crate::globals::GLOBALS.get_mut().curwin = self.prev_curwin;

                let buf = &mut *self.buf;
                u_blockfree(buf); // frees any undo tree the test built

                let mfp = Box::from_raw(buf.b_ml.ml_mfp);
                crate::memfile::mf_close(*mfp, false);

                drop(Box::from_raw(self.win));
                drop(Box::from_raw(self.buf));
            }
        }
    }

    #[test]
    fn u_savecommon_reload_creates_new_header_and_saves_lines() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        // ml_open gives 1 empty line; append 2 more so ml_line_count == 3.
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(ctx.buf(), 1, b"line2\0", 6, false) },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(ctx.buf(), 2, b"line3\0", 6, false) },
            crate::vim_defs::OK
        );
        // Replace line 1's content so all 3 lines are distinguishable.
        assert_eq!(unsafe { crate::memline::ml_replace(1, b"line1\0") }, crate::vim_defs::OK);
        ctx.buf().b_u_synced = true;

        let ret = unsafe { u_savecommon(ctx.buf(), 0, 4, 0, true) };

        assert_eq!(ret, crate::vim_defs::OK);
        let buf = ctx.buf();
        assert_eq!(buf.b_u_numhead, 1);
        assert_eq!(buf.b_u_oldhead, buf.b_u_newhead);
        assert!(!buf.b_u_synced);

        let uhp = buf.b_u_newhead;
        unsafe {
            assert_eq!((*uhp).uh_seq, 1);
            assert_eq!((*uhp).uh_entries.len(), 1);
            assert_eq!((&(*uhp).uh_entries)[0].ue_top, 0);
            assert_eq!((&(*uhp).uh_entries)[0].ue_bot, 0); // bot(4) > ml_line_count(3)
            assert_eq!(
                (&(*uhp).uh_entries)[0].ue_array,
                vec![b"line1\0".to_vec(), b"line2\0".to_vec(), b"line3\0".to_vec()]
            );
            assert_ne!((*uhp).uh_flags & uh_flags::RELOAD, 0);
            assert_eq!((*uhp).uh_cursor, crate::pos_defs::PosT::default());
            assert_eq!((*uhp).uh_cursor_vcol, -1); // virtual_active() is false by default
            assert_eq!((*uhp).uh_namedm.len(), NMARKS as usize);
        }
    }

    #[test]
    fn u_savecommon_undo_disabled_returns_ok_without_creating_a_header() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_u_synced = true;
        ctx.buf().b_p_ul = -1; // undo disabled (any negative value)

        let ret = unsafe { u_savecommon(ctx.buf(), 0, 2, 0, true) };

        assert_eq!(ret, crate::vim_defs::OK);
        let buf = ctx.buf();
        assert!(buf.b_u_newhead.is_null());
        assert_eq!(buf.b_u_numhead, 0);
        assert!(!buf.b_u_synced); // still flipped, matching the original
    }

    #[test]
    fn u_savecommon_evicts_oldest_header_when_over_undolevel() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ul = 1; // keep only 1 undo level

        // First save creates header #1 (becomes b_u_oldhead).
        ctx.buf().b_u_synced = true;
        assert_eq!(unsafe { u_savecommon(ctx.buf(), 0, 2, 0, true) }, crate::vim_defs::OK);
        let first_uhp = ctx.buf().b_u_oldhead;
        assert_eq!(ctx.buf().b_u_numhead, 1);

        // Second save creates header #2; since numhead(2) > undolevel(1)
        // once the new header exists... actually the eviction check
        // happens BEFORE the new header is linked in, comparing the
        // OLD numhead(1) against undolevel(1): 1 > 1 is false, so no
        // eviction on the second save either - confirmed by re-reading
        // the original's own loop condition, which checks numhead
        // *before* incrementing it for the new header.
        ctx.buf().b_u_synced = true;
        assert_eq!(unsafe { u_savecommon(ctx.buf(), 0, 2, 0, true) }, crate::vim_defs::OK);
        assert_eq!(ctx.buf().b_u_numhead, 2);

        // Third save: numhead(2) > undolevel(1) is true - evicts the
        // oldest header (first_uhp) before creating the third.
        ctx.buf().b_u_synced = true;
        assert_eq!(unsafe { u_savecommon(ctx.buf(), 0, 2, 0, true) }, crate::vim_defs::OK);
        assert_eq!(ctx.buf().b_u_numhead, 2); // one evicted, one added: net zero
        assert_ne!(ctx.buf().b_u_oldhead, first_uhp); // the original oldest is gone
    }

    #[test]
    fn u_savecommon_got_int_mid_copy_fails_without_corrupting_state() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(ctx.buf(), 1, b"line2\0", 6, false) },
            crate::vim_defs::OK
        );
        ctx.buf().b_u_synced = true;
        unsafe { crate::globals::GLOBALS.get_mut() }.got_int = true;

        let ret = unsafe { u_savecommon(ctx.buf(), 0, 3, 0, true) };

        unsafe { crate::globals::GLOBALS.get_mut() }.got_int = false;
        assert_eq!(ret, crate::vim_defs::FAIL);
        // A header WAS allocated and linked in (matches the original:
        // it allocates uhp before the interrupt is ever checked), but
        // it has zero entries since the copy loop bailed immediately.
        let buf = ctx.buf();
        assert_eq!(buf.b_u_numhead, 1);
        unsafe {
            assert!((*buf.b_u_newhead).uh_entries.is_empty());
        }
        assert_eq!(buf.b_ml.ml_line_count, 2); // buffer content itself untouched
    }

    #[test]
    fn u_savecommon_reuses_matching_single_line_head_entry() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1; // needed for the second (non-reload) call's undo_allowed check
        // First save: single-line save at top=0 (i.e. line 1), size 1.
        ctx.buf().b_u_synced = true;
        assert_eq!(unsafe { u_savecommon(ctx.buf(), 0, 2, 2, true) }, crate::vim_defs::OK);
        let uhp = ctx.buf().b_u_newhead;
        unsafe {
            assert_eq!((*uhp).uh_entries.len(), 1);
            assert_eq!((&(*uhp).uh_entries)[0].ue_bot, 2); // newbot != 0
        }

        // Second save at the SAME line (top=0, size 1 again) - since
        // b_u_synced is now false (set by the first call), this takes
        // the "reuse" path and should NOT push a second entry.
        let ret = unsafe { u_savecommon(ctx.buf(), 0, 2, 3, false) };

        assert_eq!(ret, crate::vim_defs::OK);
        unsafe {
            assert_eq!((*uhp).uh_entries.len(), 1); // still just 1 - reused, not appended
            assert_eq!((&(*uhp).uh_entries)[0].ue_bot, 3); // updated to the new newbot
        }
    }

    #[test]
    fn u_savecommon_reuse_moves_older_matching_entry_to_front() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1; // needed for the non-reload calls' undo_allowed check
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(ctx.buf(), 1, b"line2\0", 6, false) },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(ctx.buf(), 2, b"line3\0", 6, false) },
            crate::vim_defs::OK
        );

        // First save: single-line save of line 1 (top=0, bot=2, size 1).
        ctx.buf().b_u_synced = true;
        assert_eq!(unsafe { u_savecommon(ctx.buf(), 0, 2, 2, true) }, crate::vim_defs::OK);
        // Second save: single-line save of line 3 (top=2, bot=4, size 1) -
        // becomes entries[0], pushing line 1's entry to entries[1].
        assert_eq!(unsafe { u_savecommon(ctx.buf(), 2, 4, 4, false) }, crate::vim_defs::OK);
        let uhp = ctx.buf().b_u_newhead;
        unsafe {
            assert_eq!((*uhp).uh_entries.len(), 2);
            assert_eq!((&(*uhp).uh_entries)[0].ue_top, 2); // line 3's entry (most recent)
            assert_eq!((&(*uhp).uh_entries)[1].ue_top, 0); // line 1's entry (older)
        }

        // Third save: single-line save of line 1 again (top=0, bot=2) -
        // matches entries[1] (index 1, not 0), so it must be moved to
        // the front.
        let ret = unsafe { u_savecommon(ctx.buf(), 0, 2, 5, false) };

        assert_eq!(ret, crate::vim_defs::OK);
        unsafe {
            assert_eq!((*uhp).uh_entries.len(), 2); // still 2 - reused, not appended
            assert_eq!((&(*uhp).uh_entries)[0].ue_top, 0); // line 1's entry, now at front
            assert_eq!((&(*uhp).uh_entries)[0].ue_bot, 5); // updated to the new newbot
            assert_eq!((&(*uhp).uh_entries)[1].ue_top, 2); // line 3's entry, now second
        }
    }

    #[test]
    fn u_savecommon_reuse_bails_when_multiline_save_overlaps_top() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1; // needed for the non-reload calls' undo_allowed check
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(ctx.buf(), 1, b"line2\0", 6, false) },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(ctx.buf(), 2, b"line3\0", 6, false) },
            crate::vim_defs::OK
        );

        // First save: a 2-line save (top=0, bot=3, size 2) covering
        // lines 1-2.
        ctx.buf().b_u_synced = true;
        assert_eq!(unsafe { u_savecommon(ctx.buf(), 0, 3, 3, true) }, crate::vim_defs::OK);
        let uhp = ctx.buf().b_u_newhead;
        unsafe {
            assert_eq!((*uhp).uh_entries.len(), 1);
        }

        // Second save: single-line save of line 1 (top=0, bot=2, size
        // 1) - overlaps the existing multi-line entry's own top
        // (ue_size>1 && top(0) >= ue_top(0) && top+2(2) <= ue_top+
        // ue_size+1(3)), so the reuse search must bail (break) rather
        // than matching, and a SECOND entry gets pushed instead.
        let ret = unsafe { u_savecommon(ctx.buf(), 0, 2, 2, false) };

        assert_eq!(ret, crate::vim_defs::OK);
        unsafe {
            assert_eq!((*uhp).uh_entries.len(), 2); // a new entry was pushed, not reused
            assert_eq!((&(*uhp).uh_entries)[0].ue_top, 0);
            assert_eq!((&(*uhp).uh_entries)[0].ue_array.len(), 1);
        }
    }

    #[test]
    fn u_savecommon_not_reload_fails_when_not_modifiable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 0;

        let ret = unsafe { u_savecommon(ctx.buf(), 0, 2, 0, false) };

        assert_eq!(ret, crate::vim_defs::FAIL);
        assert!(ctx.buf().b_u_newhead.is_null()); // nothing was saved
    }

    #[test]
    fn u_savecommon_not_reload_fails_when_bot_exceeds_line_count_plus_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1;
        // ml_open's fresh buffer has exactly 1 line - bot=3 is 2 more
        // than ml_line_count(1) + 1(2), so this is a genuine
        // "changed unexpectedly" case (E881).
        let ret = unsafe { u_savecommon(ctx.buf(), 0, 3, 0, false) };

        assert_eq!(ret, crate::vim_defs::FAIL);
        assert!(ctx.buf().b_u_newhead.is_null());
    }

    #[test]
    fn u_savecommon_not_reload_succeeds_and_triggers_change_warning() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1;
        ctx.buf().b_p_ro = 1; // readonly - exercises change_warning's own real path

        let ret = unsafe { u_savecommon(ctx.buf(), 0, 2, 0, false) };

        assert_eq!(ret, crate::vim_defs::OK);
        assert!(ctx.buf().b_did_warn); // change_warning fired for real
        assert_eq!(ctx.buf().b_u_numhead, 1);

        // Reset: VIMVARS is shared, process-wide state (change_warning
        // sets v:warningmsg for real).
        unsafe {
            crate::eval::vars::set_vim_var_string(
                crate::eval::vars::VimVarIndex::Warningmsg,
                None,
            )
        };
    }

    // ---- u_save_buf/u_save/u_save_cursor/u_savesub/u_inssub/u_savedel --

    #[test]
    fn u_save_buf_fails_when_top_not_less_than_bot() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        assert_eq!(unsafe { u_save_buf(ctx.buf(), 2, 2) }, crate::vim_defs::FAIL);
    }

    #[test]
    fn u_save_buf_fails_when_bot_exceeds_line_count_plus_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        // A fresh buffer has 1 line; bot=3 exceeds line_count(1)+1(2).
        assert_eq!(unsafe { u_save_buf(ctx.buf(), 0, 3) }, crate::vim_defs::FAIL);
    }

    #[test]
    fn u_save_buf_saves_a_single_replaced_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1;

        // top+2 == bot (0+2==2): exercises the u_saveline pre-save too.
        let ret = unsafe { u_save_buf(ctx.buf(), 0, 2) };

        assert_eq!(ret, crate::vim_defs::OK);
        assert_eq!(ctx.buf().b_u_numhead, 1);
    }

    #[test]
    fn u_save_delegates_to_curbuf() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1;

        assert_eq!(unsafe { u_save(0, 2) }, crate::vim_defs::OK);
        assert_eq!(ctx.buf().b_u_numhead, 1);
    }

    #[test]
    fn u_save_cursor_saves_around_the_cursor_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1;
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(ctx.buf(), 1, b"line2\0", 6, false) },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(ctx.buf(), 2, b"line3\0", 6, false) },
            crate::vim_defs::OK
        );
        unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin }.w_cursor.lnum = 2;

        let ret = unsafe { u_save_cursor() };

        assert_eq!(ret, crate::vim_defs::OK);
        let uhp = ctx.buf().b_u_newhead;
        unsafe {
            // top = cur-1 = 1, bot = cur+1 = 3: saves just line 2.
            assert_eq!((&(*uhp).uh_entries)[0].ue_top, 1);
            assert_eq!((&(*uhp).uh_entries)[0].ue_array, vec![b"line2\0".to_vec()]);
        }
    }

    #[test]
    fn u_savesub_saves_the_single_replaced_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1;

        let ret = unsafe { u_savesub(1) };

        assert_eq!(ret, crate::vim_defs::OK);
        let uhp = ctx.buf().b_u_newhead;
        unsafe {
            assert_eq!((&(*uhp).uh_entries)[0].ue_top, 0);
            assert_eq!((&(*uhp).uh_entries)[0].ue_bot, 2); // newbot = lnum+1
        }
    }

    #[test]
    fn u_inssub_saves_zero_lines_for_a_pure_insertion() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1;

        // u_inssub(1): top=0, bot=1, newbot=2 - size = bot-top-1 = 0,
        // matching "a new line is inserted, nothing existing changes".
        let ret = unsafe { u_inssub(1) };

        assert_eq!(ret, crate::vim_defs::OK);
        let uhp = ctx.buf().b_u_newhead;
        unsafe {
            assert_eq!((&(*uhp).uh_entries)[0].ue_top, 0);
            assert_eq!((&(*uhp).uh_entries)[0].ue_bot, 2);
            assert!((&(*uhp).uh_entries)[0].ue_array.is_empty());
        }
    }

    #[test]
    fn u_savedel_uses_newbot_2_when_deleting_the_whole_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1;
        // A fresh buffer has exactly 1 line - deleting all of it means
        // nlines == ml_line_count, taking the "newbot = 2" branch.
        let ret = unsafe { u_savedel(1, 1) };

        assert_eq!(ret, crate::vim_defs::OK);
        let uhp = ctx.buf().b_u_newhead;
        unsafe {
            assert_eq!((&(*uhp).uh_entries)[0].ue_bot, 2);
        }
    }

    #[test]
    fn u_savedel_uses_lnum_as_newbot_for_a_partial_delete() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ctx = TestBufWin::new();
        ctx.buf().b_p_ma = 1;
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(ctx.buf(), 1, b"line2\0", 6, false) },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(ctx.buf(), 2, b"line3\0", 6, false) },
            crate::vim_defs::OK
        );

        // Deleting just line 2 (nlines=1) out of a 3-line buffer:
        // nlines(1) != ml_line_count(3), so newbot = lnum(2).
        let ret = unsafe { u_savedel(2, 1) };

        assert_eq!(ret, crate::vim_defs::OK);
        let uhp = ctx.buf().b_u_newhead;
        unsafe {
            assert_eq!((&(*uhp).uh_entries)[0].ue_bot, 2);
        }
    }
}
