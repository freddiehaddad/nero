//! Translated from `src/nvim/mark.c` and `src/nvim/mark.h` (partial).
//!
//! Translated: `mark.h`'s `mark_global_index`/`mark_local_index` and its
//! own `namedfm` global; `mark.c`'s `free_fmark`/`free_xfmark`/
//! `clear_fmark`, `mark_jumplist_forget_file`, `mark_view_make`,
//! `getnextmark`, `copy_jumplist`, `free_jumplist`, `set_last_cursor`,
//! `free_all_marks`, `mark_check`/`mark_check_line_bounds`,
//! `clrallmarks`.
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `setmark`/`setmark_pos`/`setpcmark`/`mark_set_global`/
//!   `mark_set_local`/the `SET_FMARK`/`RESET_FMARK`/`SET_XFMARK`/
//!   `RESET_XFMARK` macros: need `os_time()` (`os/time.c`, not yet
//!   translated - only `os/time_defs.h`'s types are done) and the
//!   `MarkSet` autocmd (`autocmd.c`). `setpcmark()` additionally needs
//!   `jop_flags` (`'jumpoptions'`, the options system, phase 4).
//! - `mark_jumplist_iter`/`mark_global_iter`: only consumed by
//!   `shada.c` (not yet translated); their C-style "raw pointer as an
//!   opaque continuation token" API doesn't have an urgent caller yet.
//! - `mark_forget_file`: needs `tagstack_clear_entry` (`tag.c`).
//! - `get_jumplist`/`get_changelist`/`mark_get*`/`pos_to_mark`/
//!   `switch_to_mark_buf`/`mark_move_to`: need buffer-list lookup
//!   (`buflist_findnr`, `buffer.c`) and/or window switching.
//! - `mark_view_restore`: needs `set_topline`/`hasFolding`/
//!   `linetabsize_eol` (the display/fold subsystem).
//! - `fname2fnum`/`fmarks_check_names`/`fmarks_check_one`: need path
//!   resolution (`expand_env`/`os_dirname`/`path_shorten_fname`) and
//!   `buflist_new()` (`buffer.c`).
//! - `fm_getname`/`mark_line`: need `ml_get()` (`memline.c`).
//! - `ex_marks`/`ex_delmarks`/`ex_jumps`/`ex_clearjumps`/`ex_changes`:
//!   need `exarg_T`, blocked on the `ex_cmds.lua`-generated `cmdidx_T`
//!   (see `ex_cmds_defs.rs`'s own module doc).
//! - `cleanup_jumplist`: needs `win_valid`/buffer-list validity checks.
//! - `mark_mb_adjustpos`: needs `ml_get_buf()` (`memline.c`) and
//!   `utf_head_off`/`utf_ptr2char`/`ptr2cells` (`mbyte.c`).
//! - `add_mark`/`get_buf_local_marks`/`get_raw_global_mark`/
//!   `get_global_marks`: need `list_T`'s real fields (the eval engine,
//!   phase 5).

use crate::globals::{GlobalCell, GLOBALS};
use crate::mark_defs::{lt, FmarkT, FmarkvT, XfmarkT, NGLOBALMARKS, NMARKS};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{BufT, WinT};

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

    /// RAII guard restoring `GLOBALS.curbuf` on drop (including on test
    /// panic via unwinding), so a failed assertion never leaves a
    /// dangling pointer behind for a later test to observe.
    struct CurbufGuard {
        previous: *mut BufT,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut BufT) -> Self {
            let previous = unsafe { GLOBALS.get_mut() }.curbuf;
            unsafe { GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous }
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
        {
            let namedfm = unsafe { NAMEDFM.get_mut() };
            namedfm[0].fmark.mark.lnum = 5;
        }
        free_all_marks();
        let namedfm = unsafe { NAMEDFM.get_mut() };
        assert_eq!(namedfm[0].fmark.mark.lnum, 0);
    }
}
