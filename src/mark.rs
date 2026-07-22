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
//! window-list fields like the still-blocked `fname2fnum` does).
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
//! - `mark_view_restore`: needs `set_topline`/`hasFolding`/
//!   `linetabsize_eol` (the display/fold subsystem).
//! - `fname2fnum`: needs `expand_env` (`~/` expansion) and
//!   `buflist_new()` (`buffer.c`, itself needing the eval engine's
//!   `dict_T` and `apply_autocmds` - re-checked this session by reading
//!   the real function, genuinely phase-5 material, not close) -
//!   `path_shorten_fname`/`os_dirname` existing now isn't enough to
//!   unblock this specific function as a whole (unlike
//!   `fmarks_check_one`/`fmarks_check_names` above, which needed only
//!   `path_fnamecmp`).
//! - `fm_getname`/`mark_line`: need `ml_get()` (`memline.c` - `ml_open`
//!   now exists, but `ml_get` itself needs `ml_find_line`'s real
//!   B-tree traversal algorithm, deliberately not rushed - see
//!   `memline.rs`'s own module doc comment).
//! - `ex_marks`/`ex_delmarks`/`ex_jumps`/`ex_clearjumps`/`ex_changes`:
//!   need `exarg_T`, blocked on the `ex_cmds.lua`-generated `cmdidx_T`
//!   (see `ex_cmds_defs.rs`'s own module doc).
//! - `cleanup_jumplist`: needs `win_valid`/buffer-list validity checks.
//! - `mark_mb_adjustpos`: needs `ml_get_buf()` (`memline.c`) and
//!   `utf_head_off`/`utf_ptr2char`/`ptr2cells` (`mbyte.c`).
//! - `add_mark`/`get_buf_local_marks`/`get_raw_global_mark`/
//!   `get_global_marks`: need `list_T`'s real fields (the eval engine,
//!   phase 5).

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
        let buf = BufT::default();
        let pos = PosT { lnum: 8, col: 2, coladd: 0 };
        let result = unsafe { pos_to_mark(&buf, None, pos) };
        assert!(!result.is_null());
        assert_eq!(unsafe { &*result }.mark.lnum, 8);
    }

    #[test]
    fn mark_get_visual_picks_earlier_position_for_lt_mark() {
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
}
