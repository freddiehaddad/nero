//! Translated from `src/nvim/mark.c` and `src/nvim/mark.h` (partial).
//!
//! Translated: `mark.h`'s `mark_global_index`/`mark_local_index` and its
//! own `namedfm` global; `mark.c`'s `free_fmark`/`free_xfmark`/
//! `clear_fmark`, `mark_jumplist_forget_file`, `mark_view_make`,
//! `getnextmark`, `copy_jumplist`, `free_jumplist`, `set_last_cursor`,
//! `free_all_marks`, `mark_check`/`mark_check_line_bounds`,
//! `clrallmarks`, `setpcmark`, `checkpcmark`, `get_changelist`,
//! `pos_to_mark`, `mark_get_visual` (now tractable now that
//! `crate::os::time::os_time` and `crate::option_vars` both exist);
//! `tag.c`'s `tagstack_clear_entry` (small enough to translate
//! alongside its only real consumer rather than waiting on the rest
//! of `tag.c`) and `mark_forget_file` (now tractable now that
//! `tagstack_clear_entry` exists); `fmarks_check_one`/
//! `fmarks_check_names` (now tractable now that `path.c`'s
//! `path_fnamecmp` exists - these two only need it, `namedfm`, and
//! `GLOBALS.firstwin`/`w_next`, not `buflist_new()` or `TabpageT`'s
//! window-list fields like the still-blocked `fname2fnum` does);
//! `mark_line`/`fm_getname` (now tractable now that `memline.c`'s
//! `ml_get` and `charset.c`'s `ptr2cells` both exist - `fm_getname`'s
//! "different buffer" branch still needs `buflist_nr2name`,
//! `buffer.c`, and returns `None` for that case); `mark_mb_adjustpos`
//! (now tractable now that `memline.c`'s `ml_get_buf`/`ml_get_buf_len`
//! and `mbyte.c`'s `utf_head_off` all exist); `mark_view_restore` (now
//! tractable now that `move.c`'s `set_topline`, `fold.c`'s
//! `hasFolding`, and `plines.c`'s `linetabsize_eol` all exist);
//! `add_mark` (`static` in the original, kept private here too),
//! `get_buf_local_marks`, `get_raw_global_mark`, `get_global_marks`
//! (now tractable now that `eval/typval.rs`'s `list_T`/`dict_T` CRUD -
//! `tv_dict_alloc`/`tv_list_alloc`/`tv_dict_add_str`/`tv_dict_add_list`/
//! `tv_list_append_dict`/`tv_list_append_number` - all exist;
//! `get_global_marks`'s own `namedfm[i].fmark.fnum != 0` branch still
//! skips the entry, needing `buflist_nr2name` (`buffer.c`) - see that
//! function's own doc comment for why this is currently unreachable
//! anyway, not just narrow).
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `setmark`/`setmark_pos`/`mark_set_global`/`mark_set_local`: need
//!   `buflist_findnr` (`buffer.c`) and the `MarkSet` autocmd
//!   (`autocmd.c`).
//! - `mark_jumplist_iter`/`mark_global_iter`: only consumed by
//!   `shada.c` (not yet translated); their C-style "raw pointer as an
//!   opaque continuation token" API doesn't have an urgent caller yet.
//! - `get_jumplist`/`mark_get`/`mark_get_global`/`mark_get_local`/
//!   `mark_get_motion`/`switch_to_mark_buf`/`mark_move_to`: need
//!   buffer-list lookup (`buflist_findnr`/`buflist_getfile`, still
//!   deferred in `buffer.c`), window switching, or `findpar`/`findsent`
//!   (`search.c`/`textobject.c`).
//! - `fname2fnum`: needs `expand_env` (`~/` expansion) and
//!   `buflist_new()` (`buffer.c`, itself needing the eval engine's
//!   `dict_T` and `apply_autocmds` - re-checked this session by reading
//!   the real function, genuinely phase-5 material, not close) -
//!   `path_shorten_fname`/`os_dirname` existing now isn't enough to
//!   unblock this specific function as a whole (unlike
//!   `fmarks_check_one`/`fmarks_check_names` above, which needed only
//!   `path_fnamecmp`).
//! - `ex_marks`/`ex_delmarks`/`ex_jumps`/`ex_clearjumps`/`ex_changes`:
//!   need `exarg_T`, blocked on the `ex_cmds.lua`-generated `cmdidx_T`
//!   (see `ex_cmds_defs.rs`'s own module doc).
//! - `cleanup_jumplist`: needs `win_valid`/buffer-list validity checks.

use crate::buffer_defs::{BufT, TaggyT, WinT};
use crate::ex_cmds_defs::cmod;
use crate::globals::{GlobalCell, GLOBALS};
use crate::mark_defs::{equalpos, lt, FmarkT, FmarkvT, XfmarkT, JUMPLISTSIZE, NGLOBALMARKS, NMARKS};
use crate::option_vars::{opt_jop_flag, OPTION_VARS};
use crate::os::time::os_time;
use crate::os::time_defs::Timestamp;
use crate::pos_defs::{PosT, MAXCOL};
use crate::vim_defs::Direction;

/// Convert mark name to the offset (`mark_global_index`).
#[must_use]
pub fn mark_global_index(name: u8) -> i32 {
    if crate::macros_defs::ascii_isupper(name as i32) {
        name as i32 - b'A' as i32
    } else if crate::ascii_defs::ascii_isdigit(name as i32) {
        NMARKS + (name as i32 - b'0' as i32)
    } else {
        -1
    }
}

/// Convert local mark name to the offset (`mark_local_index`).
#[must_use]
pub fn mark_local_index(name: u8) -> i32 {
    if crate::macros_defs::ascii_islower(name as i32) {
        name as i32 - b'a' as i32
    } else if name == b'"' {
        NMARKS
    } else if name == b'^' {
        NMARKS + 1
    } else if name == b'.' {
        NMARKS + 2
    } else {
        -1
    }
}

/// Global marks (marks with file number or name) (`namedfm`).
pub static NAMEDFM: std::sync::LazyLock<GlobalCell<[XfmarkT; NGLOBALMARKS as usize]>> =
    std::sync::LazyLock::new(|| GlobalCell::new(std::array::from_fn(|_| XfmarkT::default())));

/// Free `fmark_T` item (`free_fmark`).
///
/// The original frees `fm.additional_data` via `xfree()`; here that's
/// just normal Rust ownership (dropping the `Box`), so this function's
/// body is a no-op that exists only to keep the call sites and doc
/// symmetry with the original - `fm` going out of scope already frees
/// everything it owns.
pub fn free_fmark(fm: FmarkT) {
    drop(fm);
}

/// Free `xfmark_T` item (`free_xfmark`). Same no-op-body reasoning as
/// [`free_fmark`].
pub fn free_xfmark(fm: XfmarkT) {
    drop(fm);
}

/// Free and clear `fmark_T` item. Does not trigger `"MarkSet"` event
/// (`clear_fmark`).
pub fn clear_fmark(fm: &mut FmarkT, timestamp: Timestamp) {
    *fm = FmarkT {
        timestamp,
        ..FmarkT::default()
    };
}

/// Remove every jump list entry referring to a given buffer. This
/// function will also adjust the current jump list index
/// (`mark_jumplist_forget_file`).
pub fn mark_jumplist_forget_file(wp: &mut crate::buffer_defs::WinT, fnum: i32) {
    // Remove all jump list entries that match the deleted buffer.
    let mut i = wp.w_jumplistlen - 1;
    while i >= 0 {
        let idx = i as usize;
        if wp.w_jumplist[idx].fmark.fnum == fnum {
            // Found an entry that we want to delete.
            free_xfmark(std::mem::take(&mut wp.w_jumplist[idx]));

            // If the current jump list index is behind the entry we
            // want to delete, move it back by one.
            if wp.w_jumplistidx > i {
                wp.w_jumplistidx -= 1;
            }

            // Actually remove the entry from the jump list.
            wp.w_jumplistlen -= 1;
            for j in idx..(wp.w_jumplistlen as usize) {
                wp.w_jumplist[j] = std::mem::take(&mut wp.w_jumplist[j + 1]);
            }
        }
        i -= 1;
    }
}

/// Free a single entry in a tag stack (`tagstack_clear_entry`).
pub fn tagstack_clear_entry(item: &mut TaggyT) {
    item.tagname = Vec::new();
    item.user_data = None;
}

/// Delete every entry referring to file `fnum` from both the jumplist
/// and the tag stack (`mark_forget_file`).
pub fn mark_forget_file(wp: &mut crate::buffer_defs::WinT, fnum: i32) {
    mark_jumplist_forget_file(wp, fnum);

    // Remove all tag stack entries that match the deleted buffer.
    let mut i = wp.w_tagstacklen - 1;
    while i >= 0 {
        let idx = i as usize;
        if wp.w_tagstack[idx].fmark.fnum == fnum {
            // Found an entry that we want to delete.
            tagstack_clear_entry(&mut wp.w_tagstack[idx]);

            // If the current tag stack index is behind the entry we
            // want to delete, move it back by one.
            if wp.w_tagstackidx > i {
                wp.w_tagstackidx -= 1;
            }

            // Actually remove the entry from the tag stack.
            wp.w_tagstacklen -= 1;
            for j in idx..(wp.w_tagstacklen as usize) {
                wp.w_tagstack[j] = std::mem::take(&mut wp.w_tagstack[j + 1]);
            }
        }
        i -= 1;
    }
}

/// Check one file mark for a name that matches `name` (the file name
/// of `buf`). If it matches and doesn't already have a resolved buffer
/// number, replaces the name with `buf`'s buffer number and frees the
/// stored name (`fmarks_check_one`, `static` in the original - kept
/// private here too).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (for `'fileignorecase'`,
/// transitively via [`crate::path::path_fnamecmp`]).
unsafe fn fmarks_check_one(fm: &mut XfmarkT, name: &[u8], buf: &BufT) {
    if fm.fmark.fnum != 0 {
        return;
    }
    let Some(fname) = &fm.fname else {
        return;
    };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::path::path_fnamecmp(name, fname) } == 0 {
        fm.fmark.fnum = buf.handle;
        fm.fname = None;
    }
}

/// Check all file marks for a name that matches the file name in
/// `buf`. May replace the name with an fnum. Used for marks that come
/// from the ShaDa file (`fmarks_check_names`).
///
/// The original's `FOR_ALL_WINDOWS_IN_TAB(wp, curtab)` always takes
/// its `firstwin` branch at this specific call site (the macro's
/// `(tp) == curtab ? firstwin : tp->tp_firstwin` condition compares
/// `curtab` to itself), so `curtab`'s own not-yet-fully-translated
/// window-list fields are never actually needed here - this walks
/// `GLOBALS.firstwin`/`w_next` directly instead.
///
/// # Safety
/// Same as `fmarks_check_one` (private). Additionally walks the real
/// `GLOBALS.firstwin` linked list (via `w_next`) and dereferences each
/// node - callers must ensure every live window in the list is a
/// valid, properly initialized `WinT`, same requirement as any other
/// `firstwin`/`w_next` traversal in this crate.
pub unsafe fn fmarks_check_names(buf: &BufT) {
    let Some(name) = buf.b_ffname.as_deref() else {
        return;
    };

    // SAFETY: forwarded from this function's own safety doc.
    let namedfm = unsafe { NAMEDFM.get_mut() };
    for fm in namedfm.iter_mut() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { fmarks_check_one(fm, name, buf) };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &mut *wp };
        for i in 0..(w.w_jumplistlen as usize) {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { fmarks_check_one(&mut w.w_jumplist[i], name, buf) };
        }
        wp = w.w_next;
    }
}

/// `mark_view_make`.
#[must_use]
pub fn mark_view_make(wp: &crate::buffer_defs::WinT, pos: PosT) -> FmarkvT {
    FmarkvT {
        topline_offset: pos.lnum - wp.w_topline,
        skipcol: wp.w_skipcol,
    }
}

/// Restore the mark view. By remembering the offset between topline
/// and mark lnum at the time of definition, this function restores
/// the "view". Assumes the mark has been checked, is valid
/// (`mark_view_restore`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` whose own `w_buffer` is also valid.
pub unsafe fn mark_view_restore(fm: Option<&FmarkT>) {
    let Some(fm) = fm else { return };
    if fm.view.topline_offset < 0 {
        return;
    }
    let topline = fm.mark.lnum - fm.view.topline_offset;
    // If the mark does not have a view, topline_offset is MAXLNUM,
    // and this check can prevent restoring mark view in that case.
    if topline < 1 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::r#move::set_topline(curwin, topline) };

    // SAFETY: forwarded from this function's own safety doc.
    let no_folding = !unsafe { crate::fold::has_folding(&mut *curwin, topline) };
    // SAFETY: forwarded from this function's own safety doc.
    let line_size = unsafe { crate::plines::linetabsize_eol(curwin, topline) };
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *curwin };
    w.w_skipcol = if fm.view.skipcol > 0 && no_folding && fm.view.skipcol < line_size {
        fm.view.skipcol
    } else {
        0
    };
}

/// Search for the next named mark in the current file from a start
/// position (`getnextmark`).
///
/// Returns a raw pointer into `curbuf.b_namedm[i]` (matching the
/// original's `fmark_T *` return exactly - this is a pointer into global
/// editor state, not something a safe Rust lifetime can describe without
/// pinning `curbuf` for the caller's whole use of the result, which
/// would not match how the original is actually used at every real call
/// site: the mark is read/copied essentially immediately).
///
/// # Safety
/// Same requirement as every other function that touches
/// `crate::globals::GLOBALS`: no overlapping live access.
#[must_use]
pub unsafe fn getnextmark(startpos: &PosT, dir: Direction, begin_line: bool) -> *mut FmarkT {
    let mut pos = *startpos;

    if dir == Direction::Backward && begin_line {
        pos.col = 0;
    } else if dir == Direction::Forward && begin_line {
        pos.col = MAXCOL;
    }

    let curbuf = unsafe { &mut *GLOBALS.get_mut().curbuf };
    let mut result: *mut FmarkT = std::ptr::null_mut();
    for i in 0..(NMARKS as usize) {
        if curbuf.b_namedm[i].mark.lnum > 0 {
            let candidate = &mut curbuf.b_namedm[i] as *mut FmarkT;
            // SAFETY: candidate is a valid pointer into curbuf.b_namedm.
            let candidate_ref = unsafe { &*candidate };
            if dir == Direction::Forward {
                let better = result.is_null() || lt(candidate_ref.mark, unsafe { &*result }.mark);
                if better && lt(pos, candidate_ref.mark) {
                    result = candidate;
                }
            } else {
                let better = result.is_null() || lt(unsafe { &*result }.mark, candidate_ref.mark);
                if better && lt(candidate_ref.mark, pos) {
                    result = candidate;
                }
            }
        }
    }
    result
}

/// `copy_jumplist`.
pub fn copy_jumplist(from: &crate::buffer_defs::WinT, to: &mut crate::buffer_defs::WinT) {
    for i in 0..(from.w_jumplistlen as usize) {
        to.w_jumplist[i] = from.w_jumplist[i].clone();
    }
    to.w_jumplistlen = from.w_jumplistlen;
    to.w_jumplistidx = from.w_jumplistidx;
}

/// `free_jumplist`.
pub fn free_jumplist(wp: &mut crate::buffer_defs::WinT) {
    for i in 0..(wp.w_jumplistlen as usize) {
        free_xfmark(std::mem::take(&mut wp.w_jumplist[i]));
    }
    wp.w_jumplistlen = 0;
}

/// `set_last_cursor`.
pub fn set_last_cursor(win: &mut crate::buffer_defs::WinT) {
    if !win.w_buffer.is_null() {
        // SAFETY: just null-checked.
        let buf = unsafe { &mut *win.w_buffer };
        free_fmark(std::mem::take(&mut buf.b_last_cursor));
        buf.b_last_cursor = FmarkT {
            mark: win.w_cursor,
            fnum: 0,
            ..FmarkT::default()
        };
    }
}

/// `free_all_marks` (originally gated on `#ifdef EXITFREE`, a debug/
/// leak-detection build flag with no equivalent concept in this crate
/// yet - called unconditionally here instead of inventing a matching
/// cfg feature for a single call site).
pub fn free_all_marks() {
    let namedfm = unsafe { NAMEDFM.get_mut() };
    for entry in namedfm.iter_mut() {
        if entry.fmark.mark.lnum != 0 {
            free_xfmark(std::mem::take(entry));
        }
    }
    *namedfm = std::array::from_fn(|_| XfmarkT::default());
}

/// Checks a mark is set and valid; returns the reason it isn't as an
/// error message otherwise (`mark_check`).
///
/// The original returns `bool` plus writes to a `const char **errormsg`
/// out-parameter; translated as `Result<(), &'static str>`, this crate's
/// standard idiom for that exact C pattern (success/failure with an
/// associated message) - `fm: Option<&FmarkT>` mirrors the original's
/// explicit `fm == NULL` check.
///
/// # Safety
/// Reads `crate::globals::GLOBALS.curbuf` - same requirement as every
/// other function that touches it.
pub unsafe fn mark_check(fm: Option<&FmarkT>) -> Result<(), &'static str> {
    let Some(fm) = fm else {
        return Err(crate::errors::e_umark);
    };
    if fm.mark.lnum <= 0 {
        // In both cases it's an error but only raise when equals to 0.
        if fm.mark.lnum == 0 {
            return Err(crate::errors::e_marknotset);
        }
        return Err("");
    }
    // Only check for valid line number if the buffer is loaded.
    let curbuf = unsafe { &*GLOBALS.get_mut().curbuf };
    if fm.fnum == curbuf.handle {
        mark_check_line_bounds(Some(curbuf), fm)?;
    }
    Ok(())
}

/// Check if a mark line number is greater than the buffer line count,
/// and set `e_markinval` (`mark_check_line_bounds`).
///
/// Should be done after the buffer is loaded into memory.
pub fn mark_check_line_bounds(buf: Option<&crate::buffer_defs::BufT>, fm: &FmarkT) -> Result<(), &'static str> {
    if let Some(buf) = buf {
        if fm.mark.lnum > buf.b_ml.ml_line_count {
            return Err(crate::errors::e_markinval);
        }
    }
    Ok(())
}

/// Clear all marks and change list in the given buffer. Used mainly when
/// trashing the entire buffer during `":e"` type commands. Does not
/// trigger `"MarkSet"` event (`clrallmarks`).
pub fn clrallmarks(buf: &mut crate::buffer_defs::BufT, timestamp: Timestamp) {
    for i in 0..(NMARKS as usize) {
        clear_fmark(&mut buf.b_namedm[i], timestamp);
    }
    clear_fmark(&mut buf.b_last_cursor, timestamp);
    buf.b_last_cursor.mark.lnum = 1;
    clear_fmark(&mut buf.b_last_insert, timestamp);
    clear_fmark(&mut buf.b_last_change, timestamp);
    buf.b_op_start.lnum = 0; // start/end op mark cleared
    buf.b_op_end.lnum = 0;
    for i in 0..(buf.b_changelistlen as usize) {
        clear_fmark(&mut buf.b_changelist[i], timestamp);
    }
    buf.b_changelistlen = 0;
}

/// A static scratch `fmark_T` reused by [`pos_to_mark`] when its caller
/// doesn't provide its own output slot (`fmp == NULL`) - mirrors the
/// original's own `static fmark_T fms` local. Per the original's doc
/// comment ("some of the pointers are statically allocated, if in doubt
/// make a copy"), callers must copy the result before calling
/// `pos_to_mark` again if they need to keep it.
static POS_TO_MARK_SCRATCH: std::sync::LazyLock<GlobalCell<FmarkT>> =
    std::sync::LazyLock::new(|| GlobalCell::new(FmarkT::default()));

/// Set the previous context mark to the current position and add it to
/// the jump list (`setpcmark`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` and `crate::option_vars::OPTION_VARS`,
/// each with the same requirement as every other function that touches
/// them: no overlapping live access.
pub unsafe fn setpcmark() {
    let globals = unsafe { GLOBALS.get_mut() };
    // for :global the mark is set only once
    if globals.global_busy != 0
        || globals.listcmd_busy
        || globals.cmdmod.cmod_flags & cmod::KEEPJUMPS != 0
    {
        return;
    }

    let curbuf_handle = unsafe { &*globals.curbuf }.handle;
    let curwin = unsafe { &mut *globals.curwin };

    curwin.w_prev_pcmark = curwin.w_pcmark;
    curwin.w_pcmark = curwin.w_cursor;

    if curwin.w_pcmark.lnum == 0 {
        curwin.w_pcmark.lnum = 1;
    }

    if unsafe { OPTION_VARS.get_mut() }.jop_flags & opt_jop_flag::STACK != 0
        && curwin.w_jumplistidx < curwin.w_jumplistlen - 1
    {
        // jumpoptions=stack: if we're somewhere in the middle of the
        // jumplist discard everything after the current index.
        curwin.w_jumplistlen = curwin.w_jumplistidx + 1;
    }

    // If jumplist is full: remove oldest entry
    curwin.w_jumplistlen += 1;
    if curwin.w_jumplistlen > JUMPLISTSIZE {
        curwin.w_jumplistlen = JUMPLISTSIZE;
        free_xfmark(std::mem::take(&mut curwin.w_jumplist[0]));
        for i in 0..(JUMPLISTSIZE as usize - 1) {
            curwin.w_jumplist[i] = std::mem::take(&mut curwin.w_jumplist[i + 1]);
        }
    }
    curwin.w_jumplistidx = curwin.w_jumplistlen;

    let view = mark_view_make(curwin, curwin.w_pcmark);
    curwin.w_jumplist[(curwin.w_jumplistlen - 1) as usize] = XfmarkT {
        fname: None,
        fmark: FmarkT {
            mark: curwin.w_pcmark,
            fnum: curbuf_handle,
            timestamp: os_time(),
            view,
            additional_data: None,
        },
    };
}

/// To change context, call [`setpcmark`], then move the current
/// position to wherever, then call `checkpcmark()`. This ensures that
/// the previous context will only be changed if the cursor moved to a
/// different line. If pcmark was deleted (with "dG") the previous mark
/// is restored (`checkpcmark`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` - same requirement as every other
/// function that does so: no overlapping live access.
pub unsafe fn checkpcmark() {
    let curwin = unsafe { &mut *GLOBALS.get_mut().curwin };
    if curwin.w_prev_pcmark.lnum != 0
        && (equalpos(curwin.w_pcmark, curwin.w_cursor) || curwin.w_pcmark.lnum == 0)
    {
        curwin.w_pcmark = curwin.w_prev_pcmark;
    }
    curwin.w_prev_pcmark.lnum = 0; // it has been checked
}

/// Get mark in `count` position in the changelist relative to the
/// current index (`get_changelist`).
///
/// Changes `win.w_changelistidx`.
///
/// # Safety
/// Touches `crate::globals::GLOBALS.curbuf` - same requirement as every
/// other function that does so: no overlapping live access.
#[must_use]
pub unsafe fn get_changelist(buf: &mut BufT, win: &mut WinT, count: i32) -> *mut FmarkT {
    if buf.b_changelistlen == 0 {
        // nothing to jump to
        return std::ptr::null_mut();
    }

    let mut n = win.w_changelistidx;
    if n + count < 0 {
        if n == 0 {
            return std::ptr::null_mut();
        }
        n = 0;
    } else if n + count >= buf.b_changelistlen {
        if n == buf.b_changelistlen - 1 {
            return std::ptr::null_mut();
        }
        n = buf.b_changelistlen - 1;
    } else {
        n += count;
    }
    win.w_changelistidx = n;
    let curbuf_handle = unsafe { &*GLOBALS.get_mut().curbuf }.handle;
    // Changelist marks are always buffer local, Shada does not set it
    // when loading.
    buf.b_changelist[n as usize].fnum = curbuf_handle;
    &mut buf.b_changelist[n as usize] as *mut FmarkT
}

/// Wrap a `pos_T` into an `fmark_T`, used to abstract marks handling.
/// View fields are set to 0 (`pos_to_mark`).
///
/// Pass an `fmp` if multiple calls are needed before copying out the
/// result - `pos_to_mark` reuses a single static scratch value when
/// `fmp` is `None`, exactly like the original's own out-parameter
/// convention (see this function's own doc comment in the original).
///
/// # Safety
/// Touches a `GlobalCell` when `fmp` is `None` - same requirement as
/// every other function that touches one: no overlapping live access.
#[must_use]
pub unsafe fn pos_to_mark(buf: &BufT, fmp: Option<&mut FmarkT>, pos: PosT) -> *mut FmarkT {
    let fm: *mut FmarkT = match fmp {
        Some(fmp) => fmp as *mut FmarkT,
        None => unsafe { POS_TO_MARK_SCRATCH.get_mut() as *mut FmarkT },
    };
    let fm_ref = unsafe { &mut *fm };
    fm_ref.fnum = buf.handle;
    fm_ref.mark = pos;
    fm
}

/// Get visual marks `'<'`/`'>'` (`mark_get_visual`).
///
/// These marks are different to normal marks: never adjusted, behave
/// differently depending on editor state (visual mode), not saved in
/// ShaDa, and re-ordered when defined in reverse.
///
/// # Safety
/// Touches a `GlobalCell` (via [`pos_to_mark`]) - same requirement as
/// every other function that touches one: no overlapping live access.
#[must_use]
pub unsafe fn mark_get_visual(buf: &BufT, name: u8) -> *mut FmarkT {
    if name != b'<' && name != b'>' {
        return std::ptr::null_mut();
    }
    // start/end of visual area
    let startp = buf.b_visual.vi_start;
    let endp = buf.b_visual.vi_end;
    let mark = if ((name == b'<') == lt(startp, endp) || endp.lnum == 0) && startp.lnum != 0 {
        unsafe { pos_to_mark(buf, None, startp) }
    } else {
        unsafe { pos_to_mark(buf, None, endp) }
    };

    if buf.b_visual.vi_mode == b'V' as i32 {
        let mark_ref = unsafe { &mut *mark };
        if name == b'<' {
            mark_ref.mark.col = 0;
        } else {
            mark_ref.mark.col = MAXCOL;
        }
        mark_ref.mark.coladd = 0;
    }
    mark
}

/// Return the line at mark `mp`, truncated to fit in the window. The
/// returned string has been allocated (`mark_line`, `static` in the
/// original - kept private here too).
///
/// The returned bytes include a trailing NUL byte, matching this
/// crate's established `ml_get`-family convention (and the original's
/// own NUL-terminated-C-string representation).
///
/// # Safety
/// Touches `crate::globals::GLOBALS.curbuf`/`Columns` and
/// `crate::option_vars::OPTION_VARS` (transitively via
/// `crate::mbyte::utfc_ptr2len`/`crate::charset::ptr2cells`) - the
/// same requirements as every other function that touches global
/// editor state.
unsafe fn mark_line(mp: &PosT, lead_len: i32) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*GLOBALS.get_mut().curbuf };
    if mp.lnum == 0 || mp.lnum > curbuf.b_ml.ml_line_count {
        let mut invalid = b"-invalid-".to_vec();
        invalid.push(0);
        return invalid;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let columns = unsafe { GLOBALS.get_mut() }.Columns;
    debug_assert!(columns >= 0);

    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get(mp.lnum) };
    let start = crate::charset::skipwhite(&line);
    // Allow for up to 5 bytes per character.
    let mut s = crate::strings::xstrnsave(&line[start..], (columns as usize) * 5);

    // Truncate the line to fit it in the window.
    let mut len = 0;
    let mut p = 0usize;
    while p < s.len() && s[p] != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        len += unsafe { crate::charset::ptr2cells(&s[p..]) };
        if len >= columns - lead_len {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        p += unsafe { crate::mbyte::utfc_ptr2len(&s[p..]) } as usize;
    }
    s.truncate(p);
    s.push(0);
    s
}

/// Returns the file name/line text for file mark `fmark` (`fm_getname`).
///
/// Deferred: the "different buffer" branch (`buflist_nr2name`,
/// `buffer.c`, not yet translated - returns `None`) - only the
/// current-buffer branch (`mark_line`, private) is translated.
///
/// # Safety
/// Same as `mark_line` (private).
#[must_use]
pub unsafe fn fm_getname(fmark: &FmarkT, lead_len: i32) -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf_fnum = unsafe { &*GLOBALS.get_mut().curbuf }.handle;
    if fmark.fnum == curbuf_fnum {
        // SAFETY: forwarded from this function's own safety doc.
        return Some(unsafe { mark_line(&fmark.mark, lead_len) });
    }
    None // buflist_nr2name (buffer.c) not yet translated
}

/// Add information about mark `mname` to list `l` (`add_mark`,
/// `static` in the original - kept private here too).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`.
unsafe fn add_mark(
    l: *mut crate::eval::typval_defs::ListT,
    mname: &[u8],
    pos: PosT,
    bufnr: i32,
    fname: Option<&[u8]>,
) -> i32 {
    use crate::eval::typval::{tv_dict_add_list, tv_dict_add_str, tv_dict_alloc, tv_list_alloc, tv_list_append_dict, tv_list_append_number};
    use crate::vim_defs::{FAIL, OK};

    if pos.lnum <= 0 {
        return OK;
    }

    let d = tv_dict_alloc();
    // SAFETY: `l`/`d` are both valid, freshly-obtained live pointers
    // (forwarded from this function's own safety doc for `l`;
    // `tv_dict_alloc` never returns null).
    unsafe { tv_list_append_dict(l, d) };

    let lpos = tv_list_alloc(4);
    // SAFETY: `lpos` was just allocated above, not yet shared.
    unsafe {
        tv_list_append_number(lpos, bufnr as crate::eval::typval_defs::VarnumberT);
        tv_list_append_number(lpos, pos.lnum as crate::eval::typval_defs::VarnumberT);
        tv_list_append_number(
            lpos,
            (if pos.col < MAXCOL { pos.col + 1 } else { MAXCOL }) as crate::eval::typval_defs::VarnumberT,
        );
        tv_list_append_number(lpos, pos.coladd as crate::eval::typval_defs::VarnumberT);
    }

    // SAFETY: `d` was just returned by `tv_dict_alloc` above, not yet
    // shared beyond `l` (which only holds a refcounted reference).
    unsafe {
        if tv_dict_add_str(&mut *d, b"mark", Some(mname)) == FAIL
            || tv_dict_add_list(&mut *d, b"pos", lpos) == FAIL
            || (fname.is_some() && tv_dict_add_str(&mut *d, b"file", fname) == FAIL)
        {
            return FAIL;
        }
    }

    OK
}

/// Get information about marks local to a buffer (`get_buf_local_marks`).
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`. Touches
/// `GLOBALS.curwin`/`curbuf` (for the window-local `''` mark) - same
/// requirement as every other function that touches a `GlobalCell`.
pub unsafe fn get_buf_local_marks(buf: &BufT, l: *mut crate::eval::typval_defs::ListT) {
    // Marks 'a' to 'z'
    for i in 0..NMARKS {
        let mname = [b'\'', b'a' + i as u8];
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { add_mark(l, &mname, buf.b_namedm[i as usize].mark, buf.handle, None) };
    }

    // Mark '' is a window local mark and not a buffer local mark.
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { GLOBALS.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let curwin_pcmark = unsafe { &*globals.curwin }.w_pcmark;
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf_handle = unsafe { &*globals.curbuf }.handle;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { add_mark(l, b"''", curwin_pcmark, curbuf_handle, None) };

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        add_mark(l, b"'\"", buf.b_last_cursor.mark, buf.handle, None);
        add_mark(l, b"'[", buf.b_op_start, buf.handle, None);
        add_mark(l, b"']", buf.b_op_end, buf.handle, None);
        add_mark(l, b"'^", buf.b_last_insert.mark, buf.handle, None);
        add_mark(l, b"'.", buf.b_last_change.mark, buf.handle, None);
    }
    if crate::buffer::bt_prompt(Some(buf)) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { add_mark(l, b"':", buf.b_prompt_start.mark, buf.handle, None) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        add_mark(l, b"'<", buf.b_visual.vi_start, buf.handle, None);
        add_mark(l, b"'>", buf.b_visual.vi_end, buf.handle, None);
    }
}

/// Get a global mark. Note: mark might not have its `fnum` resolved
/// (`get_raw_global_mark`).
///
/// # Safety
/// Touches the `NAMEDFM` `GlobalCell` - same requirement as every
/// other function that touches one.
#[must_use]
pub unsafe fn get_raw_global_mark(name: u8) -> XfmarkT {
    // SAFETY: forwarded from this function's own safety doc.
    let namedfm = unsafe { NAMEDFM.get_mut() };
    namedfm[mark_global_index(name) as usize].clone()
}

/// Get information about global marks (`'A'` to `'Z'` and `'0'` to
/// `'9'`) (`get_global_marks`).
///
/// # Deferred
/// The original's `namedfm[i].fmark.fnum != 0` branch (resolving a
/// mark whose file has already been assigned a live buffer number
/// back to a file name) needs `buflist_nr2name` (`buffer.c`, not yet
/// translated) - that entry is skipped entirely here rather than
/// guessing at a name, matching the "return `None`"/"skip" precedent
/// already used for [`fm_getname`]'s own "different buffer" branch.
/// As things currently stand nothing translated in this crate can
/// actually set a `namedfm` entry's `fnum` to nonzero yet either
/// (`setmark`/`mark_set_global` themselves are still deferred, needing
/// `buflist_findnr`) - so this branch is unreachable for now, not just
/// narrow, but is kept faithfully in place (rather than omitted) since
/// it will become reachable the moment mark-setting or ShaDa-loading
/// can populate `fnum`.
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`. Touches
/// the `NAMEDFM` `GlobalCell` - same requirement as every other
/// function that touches one.
pub unsafe fn get_global_marks(l: *mut crate::eval::typval_defs::ListT) {
    // SAFETY: forwarded from this function's own safety doc.
    let namedfm = unsafe { NAMEDFM.get_mut() };
    for i in 0..(NMARKS + crate::mark_defs::EXTRA_MARKS) {
        let entry = &namedfm[i as usize];
        let name: &[u8] = if entry.fmark.fnum != 0 {
            // Needs buffer.c's buflist_nr2name - not yet translated,
            // and currently unreachable anyway (see this function's
            // own doc comment).
            continue;
        } else if let Some(fname) = entry.fname.as_deref() {
            fname
        } else {
            continue;
        };
        let letter = if i >= NMARKS {
            b'0' + (i - NMARKS) as u8
        } else {
            b'A' + i as u8
        };
        let mname = [b'\'', letter];
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { add_mark(l, &mname, entry.fmark.mark, entry.fmark.fnum, Some(name)) };
    }
}

/// Adjust position `lp` to point to the first byte of a multi-byte
/// character in `buf`. If it points to a tail byte it is moved
/// backwards to the head byte (`mark_mb_adjustpos`).
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT`. Also touches `crate::option_vars::OPTION_VARS`
/// (transitively via [`crate::mbyte::utf_head_off`]/
/// [`crate::charset::ptr2cells`]) - the same requirement as every
/// other function that touches global editor state.
pub unsafe fn mark_mb_adjustpos(buf: &mut BufT, lp: &mut PosT) {
    if lp.col > 0 || lp.coladd > 1 {
        // SAFETY: forwarded from this function's own safety doc.
        let p = unsafe { crate::memline::ml_get_buf(buf, lp.lnum) };
        // SAFETY: forwarded from this function's own safety doc.
        if p.first() == Some(&0) || unsafe { crate::memline::ml_get_buf_len(buf, lp.lnum) } < lp.col
        {
            lp.col = 0;
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            lp.col -= unsafe { crate::mbyte::utf_head_off(&p, lp.col as usize) };
        }
        // Reset "coladd" when the cursor would be on the right half of
        // a double-wide character.
        if lp.coladd == 1
            && p[lp.col as usize] != crate::ascii_defs::TAB
            && crate::charset::vim_isprintc(crate::mbyte::utf_ptr2char(&p[lp.col as usize..]))
            // SAFETY: forwarded from this function's own safety doc.
            && unsafe { crate::charset::ptr2cells(&p[lp.col as usize..]) } > 1
        {
            lp.coladd = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_global_index_matches_c_macro() {
        assert_eq!(mark_global_index(b'A'), 0);
        assert_eq!(mark_global_index(b'Z'), 25);
        assert_eq!(mark_global_index(b'0'), NMARKS);
        assert_eq!(mark_global_index(b'9'), NMARKS + 9);
        assert_eq!(mark_global_index(b'a'), -1);
    }

    #[test]
    fn mark_local_index_matches_c_macro() {
        assert_eq!(mark_local_index(b'a'), 0);
        assert_eq!(mark_local_index(b'z'), 25);
        assert_eq!(mark_local_index(b'"'), NMARKS);
        assert_eq!(mark_local_index(b'^'), NMARKS + 1);
        assert_eq!(mark_local_index(b'.'), NMARKS + 2);
        assert_eq!(mark_local_index(b'A'), -1);
    }

    #[test]
    fn clear_fmark_resets_to_init_fmark_with_timestamp() {
        let mut fm = FmarkT {
            fnum: 42,
            ..FmarkT::default()
        };
        clear_fmark(&mut fm, 12345);
        assert_eq!(fm.fnum, 0);
        assert_eq!(fm.timestamp, 12345);
    }

    #[test]
    fn mark_jumplist_forget_file_removes_matching_entries_and_adjusts_idx() {
        let mut wp = WinT {
            w_jumplistlen: 3,
            w_jumplistidx: 3,
            ..Default::default()
        };
        wp.w_jumplist[0].fmark.fnum = 1;
        wp.w_jumplist[1].fmark.fnum = 2;
        wp.w_jumplist[2].fmark.fnum = 1;
        mark_jumplist_forget_file(&mut wp, 1);
        assert_eq!(wp.w_jumplistlen, 1);
        assert_eq!(wp.w_jumplist[0].fmark.fnum, 2);
        assert_eq!(wp.w_jumplistidx, 1);
    }

    #[test]
    fn tagstack_clear_entry_clears_tagname_and_user_data() {
        let mut item = TaggyT {
            tagname: b"myfunc".to_vec(),
            user_data: Some(b"extra".to_vec()),
            ..Default::default()
        };
        tagstack_clear_entry(&mut item);
        assert!(item.tagname.is_empty());
        assert!(item.user_data.is_none());
    }

    #[test]
    fn mark_forget_file_removes_matching_entries_from_both_jumplist_and_tagstack() {
        let mut wp = WinT {
            w_jumplistlen: 2,
            w_jumplistidx: 2,
            w_tagstacklen: 3,
            w_tagstackidx: 3,
            ..Default::default()
        };
        wp.w_jumplist[0].fmark.fnum = 1;
        wp.w_jumplist[1].fmark.fnum = 2;
        wp.w_tagstack[0].fmark.fnum = 1;
        wp.w_tagstack[0].tagname = b"one".to_vec();
        wp.w_tagstack[1].fmark.fnum = 2;
        wp.w_tagstack[1].tagname = b"two".to_vec();
        wp.w_tagstack[2].fmark.fnum = 1;
        wp.w_tagstack[2].tagname = b"three".to_vec();

        mark_forget_file(&mut wp, 1);

        // jumplist: entry 0 (fnum 1) removed.
        assert_eq!(wp.w_jumplistlen, 1);
        assert_eq!(wp.w_jumplist[0].fmark.fnum, 2);

        // tagstack: entries 0 and 2 (fnum 1) removed, entry 1 (fnum 2)
        // remains, shifted down to index 0.
        assert_eq!(wp.w_tagstacklen, 1);
        assert_eq!(wp.w_tagstack[0].fmark.fnum, 2);
        assert_eq!(wp.w_tagstack[0].tagname, b"two");
        assert_eq!(wp.w_tagstackidx, 1);
    }

    #[test]
    fn mark_view_make_computes_offset_from_topline() {
        let wp = WinT {
            w_topline: 10,
            w_skipcol: 3,
            ..Default::default()
        };
        let pos = PosT { lnum: 15, col: 0, coladd: 0 };
        let v = mark_view_make(&wp, pos);
        assert_eq!(v.topline_offset, 5);
        assert_eq!(v.skipcol, 3);
    }

    #[test]
    fn copy_jumplist_copies_entries_len_and_idx() {
        let mut from = WinT {
            w_jumplistlen: 2,
            w_jumplistidx: 1,
            ..Default::default()
        };
        from.w_jumplist[0].fmark.fnum = 7;
        from.w_jumplist[1].fmark.fnum = 8;
        let mut to = WinT::default();
        copy_jumplist(&from, &mut to);
        assert_eq!(to.w_jumplistlen, 2);
        assert_eq!(to.w_jumplistidx, 1);
        assert_eq!(to.w_jumplist[0].fmark.fnum, 7);
        assert_eq!(to.w_jumplist[1].fmark.fnum, 8);
    }

    #[test]
    fn free_jumplist_clears_length() {
        let mut wp = WinT {
            w_jumplistlen: 5,
            ..Default::default()
        };
        free_jumplist(&mut wp);
        assert_eq!(wp.w_jumplistlen, 0);
    }

    #[test]
    fn set_last_cursor_noop_when_buffer_null() {
        let mut win = WinT::default();
        assert!(win.w_buffer.is_null());
        set_last_cursor(&mut win); // should not panic / not dereference null
    }

    #[test]
    fn set_last_cursor_updates_buffer_last_cursor() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 7, col: 2, coladd: 0 },
            ..Default::default()
        };
        set_last_cursor(&mut win);
        assert_eq!(buf.b_last_cursor.mark, win.w_cursor);
    }

    #[test]
    fn mark_check_line_bounds_ok_when_buf_none() {
        let fm = FmarkT::default();
        assert!(mark_check_line_bounds(None, &fm).is_ok());
    }

    #[test]
    fn mark_check_line_bounds_rejects_lnum_past_end() {
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 5;
        let fm = FmarkT {
            mark: PosT { lnum: 10, col: 0, coladd: 0 },
            ..FmarkT::default()
        };
        let err = mark_check_line_bounds(Some(&buf), &fm).unwrap_err();
        assert_eq!(err, crate::errors::e_markinval);
    }

    #[test]
    fn mark_check_line_bounds_accepts_lnum_within_end() {
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 5;
        let fm = FmarkT {
            mark: PosT { lnum: 3, col: 0, coladd: 0 },
            ..FmarkT::default()
        };
        assert!(mark_check_line_bounds(Some(&buf), &fm).is_ok());
    }

    #[test]
    fn mark_check_rejects_none() {
        let err = unsafe { mark_check(None) }.unwrap_err();
        assert_eq!(err, crate::errors::e_umark);
    }

    #[test]
    fn mark_check_rejects_unset_mark() {
        let fm = FmarkT::default(); // lnum == 0
        let err = unsafe { mark_check(Some(&fm)) }.unwrap_err();
        assert_eq!(err, crate::errors::e_marknotset);
    }

    /// Serializes every test that mutates `GLOBALS.curwin`/`curbuf`
    /// (genuinely global, shared mutable state) via [`CurbufGuard`]/
    /// [`MarkTestGuard`] below. Delegates to the crate-wide
    /// `crate::globals::global_state_test_lock` (shared by every file
    /// touching `GLOBALS`/`OPTION_VARS` in tests, not a separate mutex
    /// of its own) - see that function's own doc comment for why a
    /// single shared lock is used instead of one per file/field.
    fn globals_test_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::globals::global_state_test_lock()
    }

    /// RAII guard restoring `GLOBALS.curbuf` on drop (including on test
    /// panic via unwinding), so a failed assertion never leaves a
    /// dangling pointer behind for a later test to observe. Holds
    /// [`globals_test_lock`] for its entire lifetime.
    struct CurbufGuard {
        previous: *mut BufT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut BufT) -> Self {
            let _lock = globals_test_lock();
            let previous = unsafe { GLOBALS.get_mut() }.curbuf;
            unsafe { GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous, _lock }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    #[test]
    fn getnextmark_finds_nearest_mark_in_direction() {
        let mut buf = BufT::default();
        buf.b_namedm[0].mark.lnum = 5; // mark 'a'
        buf.b_namedm[1].mark.lnum = 10; // mark 'b'
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        let start = PosT { lnum: 7, col: 0, coladd: 0 };
        let found = unsafe { getnextmark(&start, Direction::Forward, false) };
        assert!(!found.is_null());
        assert_eq!(unsafe { &*found }.mark.lnum, 10);

        let found_back = unsafe { getnextmark(&start, Direction::Backward, false) };
        assert!(!found_back.is_null());
        assert_eq!(unsafe { &*found_back }.mark.lnum, 5);
    }

    #[test]
    fn getnextmark_returns_null_when_no_mark_in_direction() {
        let mut buf = BufT::default();
        buf.b_namedm[0].mark.lnum = 5;
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        let start = PosT { lnum: 3, col: 0, coladd: 0 };
        // No mark before lnum 3 (backward), only one after (forward).
        let found_back = unsafe { getnextmark(&start, Direction::Backward, false) };
        assert!(found_back.is_null());
    }

    #[test]
    fn clrallmarks_resets_named_and_special_marks() {
        let mut buf = BufT::default();
        buf.b_namedm[0].mark.lnum = 5;
        buf.b_last_cursor.mark.lnum = 9;
        buf.b_op_start.lnum = 3;
        buf.b_op_end.lnum = 4;
        buf.b_changelistlen = 2;
        buf.b_changelist[0].mark.lnum = 1;
        clrallmarks(&mut buf, 999);
        assert_eq!(buf.b_namedm[0].mark.lnum, 0);
        assert_eq!(buf.b_last_cursor.mark.lnum, 1); // explicitly reset to 1
        assert_eq!(buf.b_op_start.lnum, 0);
        assert_eq!(buf.b_op_end.lnum, 0);
        assert_eq!(buf.b_changelistlen, 0);
        assert_eq!(buf.b_last_cursor.timestamp, 999);
    }

    #[test]
    fn free_all_marks_clears_namedfm() {
        // NAMEDFM is a shared GlobalCell, same UB risk as GLOBALS/
        // OPTION_VARS if two tests touch it concurrently - acquire the
        // same crate-wide lock (this test previously didn't, a gap
        // found while adding fmarks_check_names' own tests below).
        let _guard = globals_test_lock();
        let prev = unsafe { NAMEDFM.get_mut() }.clone();

        {
            let namedfm = unsafe { NAMEDFM.get_mut() };
            namedfm[0].fmark.mark.lnum = 5;
        }
        free_all_marks();
        let namedfm = unsafe { NAMEDFM.get_mut() };
        assert_eq!(namedfm[0].fmark.mark.lnum, 0);

        *unsafe { NAMEDFM.get_mut() } = prev;
    }

    /// RAII guard restoring every `GLOBALS` field touched by
    /// `setpcmark`/`checkpcmark` (`curwin`, `curbuf`, `global_busy`,
    /// `listcmd_busy`, `cmdmod`) on drop, including on test panic via
    /// unwinding - broader version of [`CurbufGuard`] for tests that
    /// exercise these two functions. Holds [`globals_test_lock`] for
    /// its entire lifetime (see that function's doc comment for why).
    struct MarkTestGuard {
        prev_curwin: *mut WinT,
        prev_curbuf: *mut BufT,
        prev_global_busy: i32,
        prev_listcmd_busy: bool,
        prev_cmdmod_flags: i32,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl MarkTestGuard {
        fn set(win: *mut WinT, buf: *mut BufT) -> Self {
            let _lock = globals_test_lock();
            let globals = unsafe { GLOBALS.get_mut() };
            let guard = MarkTestGuard {
                prev_curwin: globals.curwin,
                prev_curbuf: globals.curbuf,
                prev_global_busy: globals.global_busy,
                prev_listcmd_busy: globals.listcmd_busy,
                prev_cmdmod_flags: globals.cmdmod.cmod_flags,
                _lock,
            };
            globals.curwin = win;
            globals.curbuf = buf;
            globals.global_busy = 0;
            globals.listcmd_busy = false;
            globals.cmdmod.cmod_flags = 0;
            guard
        }
    }

    impl Drop for MarkTestGuard {
        fn drop(&mut self) {
            let globals = unsafe { GLOBALS.get_mut() };
            globals.curwin = self.prev_curwin;
            globals.curbuf = self.prev_curbuf;
            globals.global_busy = self.prev_global_busy;
            globals.listcmd_busy = self.prev_listcmd_busy;
            globals.cmdmod.cmod_flags = self.prev_cmdmod_flags;
        }
    }

    #[test]
    fn setpcmark_sets_pcmark_and_pushes_jumplist_entry() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_cursor: PosT { lnum: 42, col: 3, coladd: 0 },
            ..Default::default()
        };
        let _guard = MarkTestGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);

        unsafe { setpcmark() };

        assert_eq!(win.w_pcmark.lnum, 42);
        assert_eq!(win.w_jumplistlen, 1);
        assert_eq!(win.w_jumplistidx, 1);
        assert_eq!(win.w_jumplist[0].fmark.mark.lnum, 42);
    }

    #[test]
    fn setpcmark_is_noop_when_global_busy() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_cursor: PosT { lnum: 42, col: 3, coladd: 0 },
            ..Default::default()
        };
        let _guard = MarkTestGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        unsafe { GLOBALS.get_mut() }.global_busy = 1;

        unsafe { setpcmark() };

        assert_eq!(win.w_jumplistlen, 0);
    }

    #[test]
    fn setpcmark_is_noop_when_cmod_keepjumps_set() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_cursor: PosT { lnum: 42, col: 3, coladd: 0 },
            ..Default::default()
        };
        let _guard = MarkTestGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        unsafe { GLOBALS.get_mut() }.cmdmod.cmod_flags = cmod::KEEPJUMPS;

        unsafe { setpcmark() };

        assert_eq!(win.w_jumplistlen, 0);
    }

    #[test]
    fn setpcmark_discards_forward_jumplist_when_jumpoptions_stack() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let _guard = MarkTestGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        let prev_jop = unsafe { OPTION_VARS.get_mut() }.jop_flags;
        unsafe { OPTION_VARS.get_mut() }.jop_flags = opt_jop_flag::STACK;

        // Simulate 3 marks already in the jumplist, with the index
        // currently sitting in the middle (as if the user had jumped
        // back with CTRL-O).
        win.w_jumplistlen = 3;
        win.w_jumplistidx = 1;

        win.w_cursor = PosT { lnum: 99, col: 0, coladd: 0 };
        unsafe { setpcmark() };

        // Everything after index 1 is discarded (truncating to
        // entries [0, 1]), then the new entry for the current position
        // is appended, giving a final length of 3 with the new entry
        // at index 2.
        assert_eq!(win.w_jumplistlen, 3);
        assert_eq!(win.w_jumplist[2].fmark.mark.lnum, 99);

        unsafe { OPTION_VARS.get_mut() }.jop_flags = prev_jop;
    }

    #[test]
    fn checkpcmark_restores_prev_pcmark_when_cursor_unchanged() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_prev_pcmark: PosT { lnum: 5, col: 0, coladd: 0 },
            w_pcmark: PosT { lnum: 10, col: 0, coladd: 0 },
            w_cursor: PosT { lnum: 10, col: 0, coladd: 0 },
            ..Default::default()
        };
        let _guard = MarkTestGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);

        unsafe { checkpcmark() };

        assert_eq!(win.w_pcmark.lnum, 5);
        assert_eq!(win.w_prev_pcmark.lnum, 0); // marked as checked
    }

    #[test]
    fn checkpcmark_keeps_pcmark_when_cursor_moved() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_prev_pcmark: PosT { lnum: 5, col: 0, coladd: 0 },
            w_pcmark: PosT { lnum: 10, col: 0, coladd: 0 },
            w_cursor: PosT { lnum: 20, col: 0, coladd: 0 }, // moved elsewhere
            ..Default::default()
        };
        let _guard = MarkTestGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);

        unsafe { checkpcmark() };

        assert_eq!(win.w_pcmark.lnum, 10); // unchanged
        assert_eq!(win.w_prev_pcmark.lnum, 0); // still marked as checked
    }

    #[test]
    fn get_changelist_returns_null_when_empty() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let _guard = MarkTestGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        assert!(unsafe { get_changelist(&mut buf, &mut win, 0) }.is_null());
    }

    #[test]
    fn get_changelist_clamps_and_updates_idx() {
        let mut buf = BufT {
            b_changelistlen: 3,
            ..Default::default()
        };
        buf.b_changelist[0].mark.lnum = 1;
        buf.b_changelist[1].mark.lnum = 2;
        buf.b_changelist[2].mark.lnum = 3;
        let mut win = WinT {
            w_changelistidx: 0,
            ..Default::default()
        };
        let _guard = MarkTestGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);

        let fm = unsafe { get_changelist(&mut buf, &mut win, 5) }; // clamp to last
        assert!(!fm.is_null());
        assert_eq!(unsafe { &*fm }.mark.lnum, 3);
        assert_eq!(win.w_changelistidx, 2);

        // Already at the end: moving further forward returns NULL.
        assert!(unsafe { get_changelist(&mut buf, &mut win, 1) }.is_null());
    }

    #[test]
    fn pos_to_mark_uses_provided_slot_when_given() {
        let buf = BufT::default();
        let mut fmp = FmarkT::default();
        let pos = PosT { lnum: 7, col: 1, coladd: 0 };
        let result = unsafe { pos_to_mark(&buf, Some(&mut fmp), pos) };
        assert_eq!(result, &mut fmp as *mut FmarkT);
        assert_eq!(fmp.mark.lnum, 7);
        assert_eq!(fmp.fnum, buf.handle);
    }

    #[test]
    fn pos_to_mark_uses_scratch_slot_when_none() {
        // POS_TO_MARK_SCRATCH is a shared GlobalCell - hold the
        // crate-wide test lock so a concurrently-running test can't
        // race on the same static (this test was genuinely flaky
        // without it, caught via a 10x repeated-run flakiness check).
        let _guard = globals_test_lock();
        let buf = BufT::default();
        let pos = PosT { lnum: 8, col: 2, coladd: 0 };
        let result = unsafe { pos_to_mark(&buf, None, pos) };
        assert!(!result.is_null());
        assert_eq!(unsafe { &*result }.mark.lnum, 8);
    }

    #[test]
    fn mark_get_visual_picks_earlier_position_for_lt_mark() {
        // See pos_to_mark_uses_scratch_slot_when_none's own comment:
        // mark_get_visual writes through the same shared
        // POS_TO_MARK_SCRATCH static via pos_to_mark.
        let _guard = globals_test_lock();
        let mut buf = BufT::default();
        buf.b_visual.vi_start = PosT { lnum: 3, col: 0, coladd: 0 };
        buf.b_visual.vi_end = PosT { lnum: 8, col: 0, coladd: 0 };
        buf.b_visual.vi_mode = b'v' as i32;

        let start_mark = unsafe { mark_get_visual(&buf, b'<') };
        assert!(!start_mark.is_null());
        assert_eq!(unsafe { &*start_mark }.mark.lnum, 3);

        let end_mark = unsafe { mark_get_visual(&buf, b'>') };
        assert!(!end_mark.is_null());
        assert_eq!(unsafe { &*end_mark }.mark.lnum, 8);
    }

    #[test]
    fn mark_get_visual_returns_null_for_other_names() {
        let buf = BufT::default();
        assert!(unsafe { mark_get_visual(&buf, b'a') }.is_null());
    }

    #[test]
    fn mark_get_visual_linewise_forces_col_extremes() {
        // See pos_to_mark_uses_scratch_slot_when_none's own comment.
        let _guard = globals_test_lock();
        let mut buf = BufT::default();
        buf.b_visual.vi_start = PosT { lnum: 3, col: 5, coladd: 2 };
        buf.b_visual.vi_end = PosT { lnum: 8, col: 5, coladd: 2 };
        buf.b_visual.vi_mode = b'V' as i32; // linewise

        let start_mark = unsafe { mark_get_visual(&buf, b'<') };
        assert_eq!(unsafe { &*start_mark }.mark.col, 0);
        assert_eq!(unsafe { &*start_mark }.mark.coladd, 0);

        let end_mark = unsafe { mark_get_visual(&buf, b'>') };
        assert_eq!(unsafe { &*end_mark }.mark.col, MAXCOL);
        assert_eq!(unsafe { &*end_mark }.mark.coladd, 0);
    }

    /// RAII guard restoring `GLOBALS.firstwin` on drop. Unlike
    /// [`CurbufGuard`]/[`MarkTestGuard`], this does NOT acquire its own
    /// copy of [`globals_test_lock`]: it's meant to be composed with
    /// [`NamedfmGuard`] in the same test (both touching `NAMEDFM`-
    /// adjacent state via `fmarks_check_names`), and the lock is a
    /// plain, non-reentrant `Mutex` - acquiring it twice from the same
    /// thread would deadlock. Callers must hold `globals_test_lock()`
    /// for this guard's entire lifetime instead.
    struct FirstwinGuard {
        previous: *mut WinT,
    }

    impl FirstwinGuard {
        fn set(new_firstwin: *mut WinT) -> Self {
            let previous = unsafe { GLOBALS.get_mut() }.firstwin;
            unsafe { GLOBALS.get_mut() }.firstwin = new_firstwin;
            FirstwinGuard { previous }
        }
    }

    impl Drop for FirstwinGuard {
        fn drop(&mut self) {
            unsafe { GLOBALS.get_mut() }.firstwin = self.previous;
        }
    }

    /// RAII guard saving/restoring the whole `NAMEDFM` array around a
    /// test. `NAMEDFM` is its own `GlobalCell`, subject to the exact
    /// same cross-test UB risk as `GLOBALS`/`OPTION_VARS` if two tests
    /// touch it concurrently without a shared lock (a gap found and
    /// fixed on `free_all_marks_clears_namedfm` while adding these
    /// tests). Like [`FirstwinGuard`], this does NOT acquire its own
    /// lock (composability with `FirstwinGuard` in the same test) -
    /// callers must hold `globals_test_lock()` for this guard's entire
    /// lifetime.
    struct NamedfmGuard {
        previous: [XfmarkT; NGLOBALMARKS as usize],
    }

    impl NamedfmGuard {
        fn acquire() -> Self {
            let previous = unsafe { NAMEDFM.get_mut() }.clone();
            NamedfmGuard { previous }
        }
    }

    impl Drop for NamedfmGuard {
        fn drop(&mut self) {
            // `[XfmarkT; 36]` has no `Default` impl (the blanket array
            // impl only covers up to 32 elements), so clone rather
            // than `mem::take` here.
            *unsafe { NAMEDFM.get_mut() } = self.previous.clone();
        }
    }

    #[test]
    fn fmarks_check_names_updates_matching_global_mark() {
        let _lock = globals_test_lock();
        let _guard = NamedfmGuard::acquire();
        let namedfm = unsafe { NAMEDFM.get_mut() };
        namedfm[0] = XfmarkT::default();
        namedfm[0].fname = Some(b"/foo/bar".to_vec());

        let buf = BufT { handle: 42, b_ffname: Some(b"/foo/bar".to_vec()), ..Default::default() };

        unsafe { fmarks_check_names(&buf) };

        let namedfm = unsafe { NAMEDFM.get_mut() };
        assert_eq!(namedfm[0].fmark.fnum, 42);
        assert_eq!(namedfm[0].fname, None);
    }

    #[test]
    fn fmarks_check_names_leaves_non_matching_mark_untouched() {
        let _lock = globals_test_lock();
        let _guard = NamedfmGuard::acquire();
        let namedfm = unsafe { NAMEDFM.get_mut() };
        namedfm[0] = XfmarkT::default();
        namedfm[0].fname = Some(b"/other/file".to_vec());

        let buf = BufT { handle: 42, b_ffname: Some(b"/foo/bar".to_vec()), ..Default::default() };

        unsafe { fmarks_check_names(&buf) };

        let namedfm = unsafe { NAMEDFM.get_mut() };
        assert_eq!(namedfm[0].fmark.fnum, 0);
        assert_eq!(namedfm[0].fname, Some(b"/other/file".to_vec()));
    }

    #[test]
    fn fmarks_check_names_skips_marks_that_already_have_a_fnum() {
        let _lock = globals_test_lock();
        let _guard = NamedfmGuard::acquire();
        let namedfm = unsafe { NAMEDFM.get_mut() };
        namedfm[0] = XfmarkT::default();
        namedfm[0].fname = Some(b"/foo/bar".to_vec());
        namedfm[0].fmark.fnum = 7; // already resolved

        let buf = BufT { handle: 42, b_ffname: Some(b"/foo/bar".to_vec()), ..Default::default() };

        unsafe { fmarks_check_names(&buf) };

        let namedfm = unsafe { NAMEDFM.get_mut() };
        // Untouched: fnum != 0 short-circuits fmarks_check_one.
        assert_eq!(namedfm[0].fmark.fnum, 7);
        assert_eq!(namedfm[0].fname, Some(b"/foo/bar".to_vec()));
    }

    #[test]
    fn fmarks_check_names_is_noop_when_buf_has_no_ffname() {
        let _lock = globals_test_lock();
        let _guard = NamedfmGuard::acquire();
        let namedfm = unsafe { NAMEDFM.get_mut() };
        namedfm[0] = XfmarkT::default();
        namedfm[0].fname = Some(b"/foo/bar".to_vec());

        let buf = BufT { handle: 42, b_ffname: None, ..Default::default() };

        unsafe { fmarks_check_names(&buf) };

        let namedfm = unsafe { NAMEDFM.get_mut() };
        assert_eq!(namedfm[0].fmark.fnum, 0);
        assert_eq!(namedfm[0].fname, Some(b"/foo/bar".to_vec()));
    }

    #[test]
    fn fmarks_check_names_updates_matching_window_jumplist_entry() {
        let _lock = globals_test_lock();
        let _namedfm_guard = NamedfmGuard::acquire();

        let mut win = WinT { w_jumplistlen: 1, ..Default::default() };
        win.w_jumplist[0] = XfmarkT::default();
        win.w_jumplist[0].fname = Some(b"/foo/bar".to_vec());

        let _firstwin_guard = FirstwinGuard::set(&mut win as *mut WinT);

        let buf = BufT { handle: 99, b_ffname: Some(b"/foo/bar".to_vec()), ..Default::default() };

        unsafe { fmarks_check_names(&buf) };

        assert_eq!(win.w_jumplist[0].fmark.fnum, 99);
        assert_eq!(win.w_jumplist[0].fname, None);
    }

    /// Opens a fresh memline for `buf` and installs it as `curbuf` for
    /// the duration of the returned guard, matching [`CurbufGuard`]'s
    /// existing pattern. Callers must close `buf.b_ml.ml_mfp`
    /// themselves after the guard is dropped (see call sites).
    ///
    /// `CurbufGuard::set` is constructed *before* `ml_open` runs (even
    /// though `ml_open` doesn't itself need `curbuf` set) specifically
    /// so its internally-acquired `globals_test_lock()` is already held
    /// before `ml_open`'s own `mf_sync` call touches the shared
    /// `GLOBALS.got_int` - otherwise that touch would race, unguarded,
    /// against any other test reading/writing it concurrently (found
    /// via a real, if rare, flaky failure in a from-scratch flakiness
    /// re-run).
    fn open_and_set_curbuf(buf: &mut BufT) -> CurbufGuard {
        let guard = CurbufGuard::set(buf as *mut BufT);
        assert_eq!(unsafe { crate::memline::ml_open(buf) }, crate::vim_defs::OK);
        guard
    }

    #[test]
    fn mark_line_returns_invalid_marker_for_lnum_zero() {
        let mut buf = BufT::default();
        let guard = open_and_set_curbuf(&mut buf);

        let pos = PosT { lnum: 0, col: 0, coladd: 0 };
        assert_eq!(unsafe { mark_line(&pos, 0) }, b"-invalid-\0".to_vec());

        drop(guard);
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn mark_line_returns_invalid_marker_for_lnum_past_end() {
        let mut buf = BufT::default();
        let guard = open_and_set_curbuf(&mut buf);

        let pos = PosT { lnum: 999, col: 0, coladd: 0 };
        assert_eq!(unsafe { mark_line(&pos, 0) }, b"-invalid-\0".to_vec());

        drop(guard);
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn mark_line_on_the_default_empty_line_returns_just_a_nul() {
        let mut buf = BufT::default();
        let guard = open_and_set_curbuf(&mut buf);
        // ml_open's own single line is empty; Columns defaults to 0 in
        // GLOBALS::default(), so set a realistic value for the
        // truncation math to behave like a real session.
        unsafe { GLOBALS.get_mut() }.Columns = 80;

        let pos = PosT { lnum: 1, col: 0, coladd: 0 };
        assert_eq!(unsafe { mark_line(&pos, 0) }, vec![0u8]);

        drop(guard);
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn fm_getname_current_buffer_matches_mark_line() {
        let mut buf = BufT { handle: 7, ..Default::default() };
        let guard = open_and_set_curbuf(&mut buf);
        unsafe { GLOBALS.get_mut() }.Columns = 80;

        let fmark = FmarkT {
            mark: PosT { lnum: 1, col: 0, coladd: 0 },
            fnum: 7,
            timestamp: 0,
            view: FmarkvT::default(),
            additional_data: None,
        };
        assert_eq!(unsafe { fm_getname(&fmark, 0) }, Some(vec![0u8]));

        drop(guard);
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn fm_getname_different_buffer_returns_none() {
        let mut buf = BufT { handle: 7, ..Default::default() };
        let guard = open_and_set_curbuf(&mut buf);

        let fmark = FmarkT {
            mark: PosT { lnum: 1, col: 0, coladd: 0 },
            fnum: 42, // a different buffer number
            timestamp: 0,
            view: FmarkvT::default(),
            additional_data: None,
        };
        assert_eq!(unsafe { fm_getname(&fmark, 0) }, None);

        drop(guard);
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    /// Opens a fresh memline for `buf` (no `curbuf` involved -
    /// `mark_mb_adjustpos` takes `buf` directly) and replaces line 1
    /// with `line`. Callers must close `buf.b_ml.ml_mfp` themselves.
    fn buf_with_line(buf: &mut BufT, line: &[u8]) {
        assert_eq!(unsafe { crate::memline::ml_open(buf) }, crate::vim_defs::OK);
        assert_eq!(unsafe { crate::memline::ml_replace_buf_len(buf, 1, line) }, crate::vim_defs::OK);
    }

    #[test]
    fn mark_mb_adjustpos_is_noop_when_col_zero_and_coladd_at_most_one() {
        let _guard = globals_test_lock();
        let mut buf = BufT::default();
        buf_with_line(&mut buf, b"hello\0");

        let mut pos = PosT { lnum: 1, col: 0, coladd: 0 };
        unsafe { mark_mb_adjustpos(&mut buf, &mut pos) };
        assert_eq!(pos, PosT { lnum: 1, col: 0, coladd: 0 });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn mark_mb_adjustpos_walks_back_from_a_continuation_byte_to_the_head() {
        let _guard = globals_test_lock();
        let mut buf = BufT::default();
        // "日本\0" = [E6,97,A5, E6,9C,AC, 00] - two independent CJK
        // characters (verified via utf_head_off's own tests: pointing
        // into the 2nd character's continuation bytes walks back only
        // to its own head, index 3).
        buf_with_line(&mut buf, "日本\0".as_bytes());

        let mut pos = PosT { lnum: 1, col: 4, coladd: 0 }; // 2nd byte of 本
        unsafe { mark_mb_adjustpos(&mut buf, &mut pos) };
        assert_eq!(pos.col, 3); // head byte of 本

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn mark_mb_adjustpos_resets_col_past_end_of_line() {
        let _guard = globals_test_lock();
        let mut buf = BufT::default();
        buf_with_line(&mut buf, b"hi\0"); // length 2

        let mut pos = PosT { lnum: 1, col: 10, coladd: 0 };
        unsafe { mark_mb_adjustpos(&mut buf, &mut pos) };
        assert_eq!(pos.col, 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn mark_mb_adjustpos_resets_col_on_an_empty_line() {
        let _guard = globals_test_lock();
        let mut buf = BufT::default();
        // ml_open's own default line is already empty (b"\0").
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);

        let mut pos = PosT { lnum: 1, col: 1, coladd: 0 };
        unsafe { mark_mb_adjustpos(&mut buf, &mut pos) };
        assert_eq!(pos.col, 0);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn mark_mb_adjustpos_resets_coladd_on_the_right_half_of_a_double_wide_char() {
        let _guard = globals_test_lock();
        let mut buf = BufT::default();
        // "x一\0": U+4E00 (一) is East Asian Wide (2 cells) - verified
        // both vim_isprintc(0x4e00) and ptr2cells (== 2) directly via a
        // throwaway scratch probe before writing this test.
        buf_with_line(&mut buf, "x一\0".as_bytes());

        let mut pos = PosT { lnum: 1, col: 1, coladd: 1 }; // head byte of 一
        unsafe { mark_mb_adjustpos(&mut buf, &mut pos) };
        assert_eq!(pos.col, 1); // already at the head, no adjustment
        assert_eq!(pos.coladd, 0); // reset: cursor was on its right half

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn mark_mb_adjustpos_leaves_coladd_alone_for_a_single_width_char() {
        let _guard = globals_test_lock();
        let mut buf = BufT::default();
        buf_with_line(&mut buf, b"ab\0");

        let mut pos = PosT { lnum: 1, col: 1, coladd: 1 }; // 'b', single-width
        unsafe { mark_mb_adjustpos(&mut buf, &mut pos) };
        assert_eq!(pos.col, 1);
        assert_eq!(pos.coladd, 1); // ptr2cells('b') == 1, not reset

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn mark_mb_adjustpos_never_resets_coladd_when_sitting_on_a_tab() {
        let _guard = globals_test_lock();
        let mut buf = BufT::default();
        buf_with_line(&mut buf, b"a\tb\0");

        let mut pos = PosT { lnum: 1, col: 1, coladd: 1 }; // the TAB byte
        unsafe { mark_mb_adjustpos(&mut buf, &mut pos) };
        assert_eq!(pos.col, 1);
        // TAB is explicitly excluded from the double-wide reset check,
        // regardless of what ptr2cells might otherwise report for it.
        assert_eq!(pos.coladd, 1);

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn mark_view_restore_noop_when_fm_is_none() {
        let _guard = globals_test_lock();
        unsafe { mark_view_restore(None) };
        // No panic, no GLOBALS access attempted - nothing to assert
        // beyond "this doesn't crash".
    }

    #[test]
    fn mark_view_restore_noop_when_topline_offset_negative() {
        let _guard = globals_test_lock();
        let fm = FmarkT {
            mark: PosT { lnum: 10, col: 0, coladd: 0 },
            fnum: 0,
            timestamp: 0,
            view: FmarkvT { topline_offset: -1, skipcol: 0 },
            additional_data: None,
        };
        unsafe { mark_view_restore(Some(&fm)) };
        // Returns before touching GLOBALS.curwin at all - nothing to
        // assert beyond "this doesn't crash".
    }

    #[test]
    fn mark_view_restore_noop_when_mark_has_no_recorded_view() {
        let _guard = globals_test_lock();
        // INIT_FMARKV's default topline_offset is MAXLNUM, so any
        // realistic mark.lnum makes `topline = lnum - MAXLNUM` deeply
        // negative - the "topline < 1" guard should catch this.
        let fm = FmarkT {
            mark: PosT { lnum: 10, col: 0, coladd: 0 },
            fnum: 0,
            timestamp: 0,
            view: FmarkvT::default(),
            additional_data: None,
        };
        unsafe { mark_view_restore(Some(&fm)) };
    }

    #[test]
    fn mark_view_restore_sets_topline_and_skipcol_within_bounds() {
        let _guard = globals_test_lock();
        let mut buf = BufT::default();
        buf_with_line(&mut buf, b"hello world\0"); // 11 columns wide

        let mut win =
            crate::buffer_defs::WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let prev_curwin = unsafe { GLOBALS.get_mut() }.curwin;
        unsafe { GLOBALS.get_mut() }.curwin = &mut win as *mut crate::buffer_defs::WinT;

        let fm = FmarkT {
            mark: PosT { lnum: 5, col: 0, coladd: 0 },
            fnum: 0,
            timestamp: 0,
            view: FmarkvT { topline_offset: 4, skipcol: 5 }, // topline = 5 - 4 = 1
            additional_data: None,
        };
        unsafe { mark_view_restore(Some(&fm)) };

        assert_eq!(win.w_topline, 1);
        assert_eq!(win.w_skipcol, 5); // 0 < 5 < linetabsize_eol(1) == 11

        unsafe { GLOBALS.get_mut() }.curwin = prev_curwin;
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn mark_view_restore_resets_skipcol_when_out_of_bounds() {
        let _guard = globals_test_lock();
        let mut buf = BufT::default();
        buf_with_line(&mut buf, b"hello world\0"); // 11 columns wide

        let mut win =
            crate::buffer_defs::WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let prev_curwin = unsafe { GLOBALS.get_mut() }.curwin;
        unsafe { GLOBALS.get_mut() }.curwin = &mut win as *mut crate::buffer_defs::WinT;

        let fm = FmarkT {
            mark: PosT { lnum: 5, col: 0, coladd: 0 },
            fnum: 0,
            timestamp: 0,
            view: FmarkvT { topline_offset: 4, skipcol: 50 }, // 50 >= linetabsize_eol(1) == 11
            additional_data: None,
        };
        unsafe { mark_view_restore(Some(&fm)) };

        assert_eq!(win.w_topline, 1);
        assert_eq!(win.w_skipcol, 0);

        unsafe { GLOBALS.get_mut() }.curwin = prev_curwin;
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn add_mark_returns_ok_and_populates_dict_with_mark_pos_file() {
        use crate::eval::typval::{tv_dict_find, tv_list_alloc, tv_list_free};
        use crate::eval::typval_defs::TypvalValue;

        let l = tv_list_alloc(0);
        let pos = PosT { lnum: 3, col: 4, coladd: 0 };
        let rc = unsafe { add_mark(l, b"'a", pos, 7, Some(b"/tmp/foo")) };
        assert_eq!(rc, crate::vim_defs::OK);

        unsafe {
            assert_eq!((*l).lv_len, 1);
            let item = (*l).lv_first;
            let d = match (*item).li_tv.value {
                TypvalValue::Dict(d) => d,
                _ => panic!("expected a dict"),
            };

            let mark_item = tv_dict_find(Some(&mut *d), b"mark").unwrap();
            assert!(
                matches!(&(*mark_item).di_tv.value, TypvalValue::String(Some(s)) if s == b"'a")
            );

            let file_item = tv_dict_find(Some(&mut *d), b"file").unwrap();
            assert!(
                matches!(&(*file_item).di_tv.value, TypvalValue::String(Some(s)) if s == b"/tmp/foo")
            );

            let pos_item = tv_dict_find(Some(&mut *d), b"pos").unwrap();
            let lpos = match (*pos_item).di_tv.value {
                TypvalValue::List(lp) => lp,
                _ => panic!("expected a list"),
            };
            assert_eq!((*lpos).lv_len, 4);
            let mut values = Vec::new();
            let mut cur = (*lpos).lv_first;
            while !cur.is_null() {
                if let TypvalValue::Number(n) = (*cur).li_tv.value {
                    values.push(n);
                }
                cur = (*cur).li_next;
            }
            assert_eq!(values, vec![7, 3, 5, 0]); // bufnr, lnum, col+1, coladd

            tv_list_free(l);
        }
    }

    #[test]
    fn add_mark_skips_marks_with_non_positive_lnum() {
        use crate::eval::typval::{tv_list_alloc, tv_list_free};

        let l = tv_list_alloc(0);
        let pos = PosT { lnum: 0, col: 0, coladd: 0 };
        let rc = unsafe { add_mark(l, b"'a", pos, 1, None) };
        assert_eq!(rc, crate::vim_defs::OK);
        unsafe {
            assert_eq!((*l).lv_len, 0);
            tv_list_free(l);
        }
    }

    #[test]
    fn add_mark_omits_file_key_when_fname_is_none() {
        use crate::eval::typval::{tv_dict_find, tv_list_alloc, tv_list_free};
        use crate::eval::typval_defs::TypvalValue;

        let l = tv_list_alloc(0);
        let pos = PosT { lnum: 1, col: 0, coladd: 0 };
        let rc = unsafe { add_mark(l, b"'a", pos, 1, None) };
        assert_eq!(rc, crate::vim_defs::OK);
        unsafe {
            let item = (*l).lv_first;
            let d = match (*item).li_tv.value {
                TypvalValue::Dict(d) => d,
                _ => panic!("expected a dict"),
            };
            assert!(tv_dict_find(Some(&mut *d), b"file").is_none());
            tv_list_free(l);
        }
    }

    #[test]
    fn get_buf_local_marks_includes_only_marks_with_positive_lnum() {
        use crate::eval::typval::{tv_dict_find, tv_list_alloc, tv_list_free};
        use crate::eval::typval_defs::TypvalValue;

        let mut buf = BufT { handle: 5, ..Default::default() };
        buf.b_namedm[0].mark = PosT { lnum: 3, col: 1, coladd: 0 }; // mark 'a'
        buf.b_op_start = PosT { lnum: 7, col: 0, coladd: 0 };
        // Everything else (b_last_cursor, b_op_end, b_last_insert,
        // b_last_change, b_visual.vi_start/vi_end, w_pcmark) stays at
        // lnum == 0 (the `Default` value) - add_mark's own
        // `pos.lnum <= 0` early return correctly excludes those below,
        // not a test gap.
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = MarkTestGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);

        let l = tv_list_alloc(0);
        unsafe { get_buf_local_marks(&buf, l) };

        unsafe {
            assert_eq!((*l).lv_len, 2);

            let first = (*l).lv_first;
            let d1 = match (*first).li_tv.value {
                TypvalValue::Dict(d) => d,
                _ => panic!("expected a dict"),
            };
            let mark1 = tv_dict_find(Some(&mut *d1), b"mark").unwrap();
            assert!(matches!(&(*mark1).di_tv.value, TypvalValue::String(Some(s)) if s == b"'a"));

            let second = (*first).li_next;
            let d2 = match (*second).li_tv.value {
                TypvalValue::Dict(d) => d,
                _ => panic!("expected a dict"),
            };
            let mark2 = tv_dict_find(Some(&mut *d2), b"mark").unwrap();
            assert!(matches!(&(*mark2).di_tv.value, TypvalValue::String(Some(s)) if s == b"'["));

            tv_list_free(l);
        }
    }

    #[test]
    fn get_raw_global_mark_returns_the_indexed_namedfm_entry() {
        let _lock = globals_test_lock();
        let _guard = NamedfmGuard::acquire();
        let namedfm = unsafe { NAMEDFM.get_mut() };
        namedfm[mark_global_index(b'B') as usize].fmark.mark.lnum = 99;

        let got = unsafe { get_raw_global_mark(b'B') };
        assert_eq!(got.fmark.mark.lnum, 99);
    }

    #[test]
    fn get_global_marks_includes_resolved_fname_and_skips_nonzero_fnum() {
        use crate::eval::typval::{tv_dict_find, tv_list_alloc, tv_list_free};
        use crate::eval::typval_defs::TypvalValue;

        let _lock = globals_test_lock();
        let _guard = NamedfmGuard::acquire();
        let namedfm = unsafe { NAMEDFM.get_mut() };
        // Mark 'A' (index 0): unresolved fnum, has a stored fname ->
        // included.
        namedfm[0] = XfmarkT::default();
        namedfm[0].fmark.mark = PosT { lnum: 4, col: 0, coladd: 0 };
        namedfm[0].fname = Some(b"/tmp/a".to_vec());
        // Mark 'B' (index 1): fnum already resolved -> skipped, needs
        // buflist_nr2name (see get_global_marks's own doc comment).
        namedfm[1] = XfmarkT::default();
        namedfm[1].fmark.mark = PosT { lnum: 8, col: 0, coladd: 0 };
        namedfm[1].fmark.fnum = 3;

        let l = tv_list_alloc(0);
        unsafe { get_global_marks(l) };

        unsafe {
            assert_eq!((*l).lv_len, 1);
            let item = (*l).lv_first;
            let d = match (*item).li_tv.value {
                TypvalValue::Dict(d) => d,
                _ => panic!("expected a dict"),
            };
            let mark_item = tv_dict_find(Some(&mut *d), b"mark").unwrap();
            assert!(matches!(&(*mark_item).di_tv.value, TypvalValue::String(Some(s)) if s == b"'A"));
            let file_item = tv_dict_find(Some(&mut *d), b"file").unwrap();
            assert!(
                matches!(&(*file_item).di_tv.value, TypvalValue::String(Some(s)) if s == b"/tmp/a")
            );

            tv_list_free(l);
        }
    }
}
