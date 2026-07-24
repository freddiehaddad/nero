//! Translated from `src/nvim/cursor.c` (tractable core only).
//!
//! `cursor.c` (~520 lines) is the cursor-position validity/movement
//! utility file. Most of it needs the screen-column/character-width
//! subsystem (`plines.c`'s `getvcol`/`getvvcol`/`init_charsize_arg`/
//! `win_charsize`, none translated - a substantial subsystem of its
//! own, comparable in scope to `mbyte.c` but for on-screen character
//! width rather than byte-level decoding), the fold subsystem
//! (`fold.c`'s `hasFolding`/`hasFoldingWin`), or the redraw pipeline
//! (`redraw_later`/`changed_cline_bef_curs`).
//!
//! Translated: `check_pos`, `check_visual_pos`, `get_cursor_line_ptr`/
//! `get_cursor_pos_ptr`/`get_cursor_line_len`/`get_cursor_pos_len`
//! (thin `curwin`/`curbuf` + `memline.c`'s `ml_get_buf`/`ml_get_buf_len`
//! wrappers), `gchar_cursor`/`char_before_cursor` (+ `mbyte.c`'s
//! `utf_ptr2char`/`utf_head_off`), `pchar_cursor`, `adjust_cursor_col`,
//! `inc_cursor`/`dec_cursor` (thin wrappers over `memline.c`'s newly
//! translated `inc`/`dec`, operating on `curwin.w_cursor`). None of
//! these have a real caller translated yet either (their actual
//! callers are all in `normal.c`/`insert.c`/`ops.c`/`search.c`/etc.,
//! future phases) - translated anyway as this file's own genuinely
//! tractable core, matching the established per-file "harvest what's
//! mechanically composable from already-translated primitives, defer
//! the rest" treatment used for `mark.c`/`undo.c`/`buffer.c`/
//! `change.c`, since these are simple, faithful 1:1 wrappers with no
//! design freedom of their own to get wrong.
//!
//! `pchar_cursor` needs one deliberate representation note:
//! `memline.rs`'s `ml_get_buf_mut` (unlike the original's own
//! `ml_get_buf_mut`, which returns a raw pointer directly into the
//! live `ml_line_ptr` heap buffer, so writing through it mutates that
//! buffer automatically) returns a *disconnected copy* - see its own
//! doc comment ("this is very limited"). To actually persist a byte
//! write (matching `ml_flush_line`'s own later read of
//! `buf.b_ml.ml_line_ptr` as the source of truth), `pchar_cursor` here
//! calls `ml_get_buf_mut` first (for its real side effects: loading
//! the line into the cache, marking it dirty, and the undo
//! bookkeeping via `ml_add_deleted_len_buf`), then writes the new byte
//! directly into `buf.b_ml.ml_line_ptr`'s own slot - the actual
//! persistent storage this crate uses for "the currently locked/cached
//! line", equivalent to the original's pointer-based mutation.
//!
//! Also translated, now that `plines.c`'s `getvcol`/`getvvcol`/
//! character-width machinery and `state.c`'s `virtual_active` both
//! exist: `getviscol`/`getviscol2` (thin `getvvcol` wrappers over
//! `curwin.w_cursor`), `coladvance`/`coladvance2`/`getvpos` (advance
//! the cursor to a target screen column). `coladvance2`'s
//! `win_charsize(cstype, col, ci.ptr, ci.chr.value, &csarg)` dispatch
//! is inlined directly (matching `getvcol`'s own approach) rather than
//! adding a standalone `win_charsize` wrapper, since `charsize_regular`
//! needs an explicit byte-offset `col` parameter (this crate's
//! replacement for the original's `cur - csarg->line` pointer
//! subtraction) that's naturally available here as `ci.pos` - a real
//! call site finally resolved the ambiguity `plines.rs`'s own module
//! doc flagged when `win_charsize` was investigated and skipped there.
//!
//! `coladvance2`'s own `'virtualedit'`-space-filling branch (reached
//! only when `virtual_active(wp)` is true, `addspaces` is set, AND the
//! computed column doesn't already land on the target - a genuinely
//! narrow, opt-in-only combination) is `unimplemented!()`: it needs
//! `change.c`'s `inserted_bytes` plus a real `ml_replace` call wired
//! through the undo/redraw-notification machinery, neither of which
//! this crate has assembled yet. `coladvance_force` (which always
//! passes `addspaces = true`) is still fully correct for every case
//! that DOESN'T hit that inner branch (i.e. whenever `'virtualedit'`
//! is unset, the overwhelmingly common case) - matches this crate's
//! established "narrow, discrete, opt-in configuration branch" `unimplemented!()`
//! precedent (`window.rs`'s `win_fdccol_count`, `indent.rs`'s
//! `get_breakindent_win`).
//!
//! **`check_cursor_col`** itself (unlike `check_cursor`, its combo
//! caller) turned out to need NO fold.c dependency at all - re-read
//! directly rather than assumed blocked alongside `check_cursor`/
//! `check_cursor_lnum`, and translated in full (col-bounds clamping
//! for Normal/Insert/Terminal/Visual/`'virtualedit'` modes, plus the
//! `'virtualedit'=all` coladd fine-tuning via `getvcol`). One
//! genuine, if pathological, overflow risk was caught and fixed the
//! same way as `mbyte.rs`'s `utf_ptr2char`: `oldcoladd = col + coladd`
//! uses `wrapping_add` (and the later `oldcoladd - new_col` uses
//! `wrapping_sub`) since `col == MAXCOL` paired with a nonzero
//! `coladd` - not a state legitimate callers ever produce, but not
//! provably unreachable either - would otherwise panic in a debug
//! build instead of silently producing a nonsensical-but-harmless
//! value like the original's own C `int` arithmetic would.
//!
//! Also translated, now that `fold.rs`'s "no folds in this window"
//! fast path exists: `check_cursor_lnum`, `check_cursor` (the
//! `check_cursor_lnum` + `check_cursor_col` combo), and
//! `get_cursor_rel_lnum`. Each defers to `fold::has_folding`/
//! `fold::has_any_folding`'s own `unimplemented!()` for the "folding
//! might genuinely be active" case (see `fold.rs`'s module doc) -
//! correct and complete for the overwhelmingly common no-folds case,
//! panicking (not silently wrong) otherwise.
//!
//! Deferred: `set_leftcol` (needs `redraw_later`/`changed_cline_bef_curs`,
//! drawscreen.c's redraw-tracking side).

use crate::buffer_defs::{BufT, WinT};
use crate::pos_defs::{ColnrT, LinenrT, MAXCOL, PosT};

/// Make sure `pos.lnum` and `pos.col` are valid in `buf`. This allows
/// for the col to be on the NUL byte (`check_pos`).
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT` (same requirement as `crate::memline::ml_get_buf_len`).
pub unsafe fn check_pos(buf: &mut BufT, pos: &mut PosT) {
    pos.lnum = pos.lnum.min(buf.b_ml.ml_line_count);
    if pos.col > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        pos.col = pos.col.min(unsafe { crate::memline::ml_get_buf_len(buf, pos.lnum) });
    }
}

/// Make sure `win.w_cursor.col` is valid. Special handling of
/// insert-mode (`check_cursor_col`).
///
/// # Safety
/// `win` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid. Touches `crate::globals::GLOBALS` and
/// `crate::option_vars::OPTION_VARS`.
pub unsafe fn check_cursor_col(win: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let (oldcol, oldcoladd, lnum) = unsafe {
        let w = &*win;
        // wrapping_add: matches the original's plain `+` (C signed
        // overflow is UB but wraps in practice on every real target);
        // `col == MAXCOL && coladd > 0` is not a state legitimate
        // callers produce (MAXCOL is always paired with coladd == 0,
        // e.g. coladvance2's own MAXCOL branch), but this must not
        // panic if it's ever reached anyway - same reasoning as
        // mbyte.rs's utf_ptr2char overflow fix.
        (w.w_cursor.col, w.w_cursor.col.wrapping_add(w.w_cursor.coladd), w.w_cursor.lnum)
    };
    // SAFETY: forwarded from this function's own safety doc.
    let cur_ve_flags = unsafe { crate::option::get_ve_flags(&*win) };

    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *(*win).w_buffer };
    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { crate::memline::ml_get_buf_len(buf, lnum) };

    if len == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *win }.w_cursor.col = 0;
    } else if oldcol >= len {
        let (state, restart_edit, visual_active) = {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            (g.State, g.restart_edit, g.Visual.active)
        };
        let sel_first_byte = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_sel
            .as_deref()
            .and_then(|s| s.first())
            .copied();
        // SAFETY: forwarded from this function's own safety doc.
        let is_virtual_active = unsafe { crate::state::virtual_active(&*win) };

        // Allow cursor past end-of-line when:
        // - in Insert mode or restarting Insert mode
        // - in Terminal mode
        // - in Visual mode and 'selection' isn't "old"
        // - 'virtualedit' is set
        if (state & crate::state_defs::mode::INSERT as i32) != 0
            || restart_edit != 0
            || (state & crate::state_defs::mode::TERMINAL as i32) != 0
            || (visual_active && sel_first_byte != Some(b'o'))
            || (cur_ve_flags & crate::option_vars::opt_ve_flag::ONEMORE) != 0
            || is_virtual_active
        {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &mut *win }.w_cursor.col = len;
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &mut *win }.w_cursor.col = len - 1;
            // Move the cursor to the head byte.
            // SAFETY: forwarded from this function's own safety doc.
            let buf = unsafe { &mut *(*win).w_buffer };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::mark::mark_mb_adjustpos(buf, &mut (*win).w_cursor) };
        }
    } else if oldcol < 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *win }.w_cursor.col = 0;
    }

    // If virtual editing is on, we can leave the cursor on the old
    // position, only we must set it to virtual. But don't do it when
    // at the end of the line.
    if oldcol == MAXCOL {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *win }.w_cursor.coladd = 0;
    } else if cur_ve_flags == crate::option_vars::opt_ve_flag::ALL {
        // SAFETY: forwarded from this function's own safety doc.
        let new_col = unsafe { &*win }.w_cursor.col;
        if oldcoladd > new_col {
            let mut coladd = oldcoladd.wrapping_sub(new_col);

            if new_col + 1 < len {
                debug_assert!(coladd > 0);
                let mut cs = 0;
                let mut ce = 0;
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    crate::plines::getvcol(
                        win,
                        &mut (*win).w_cursor,
                        Some(&mut cs),
                        None,
                        Some(&mut ce),
                        0,
                    );
                }
                coladd = coladd.min(ce - cs);
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &mut *win }.w_cursor.coladd = coladd;
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &mut *win }.w_cursor.coladd = 0;
        }
    }
}

/// Check if `Visual.start` position is valid, correct it if not. Can
/// be called when in Visual mode and a change has been made
/// (`check_visual_pos`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` whose `b_ml.ml_mfp`, if non-null, points to a live
/// `MemfileT`.
pub unsafe fn check_visual_pos() {
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }.b_ml.ml_line_count;
    let start_lnum = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start.lnum;

    if start_lnum > line_count {
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.Visual.start.lnum = line_count;
        g.Visual.start.col = 0;
        g.Visual.start.coladd = 0;
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let len = unsafe { crate::memline::ml_get_len(start_lnum) };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        if g.Visual.start.col > len {
            g.Visual.start.col = len;
            g.Visual.start.coladd = 0;
        }
    }
}

/// @return pointer to cursor line (`get_cursor_line_ptr`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin`/`curbuf` must be valid, non-null
/// pointers to live `WinT`/`BufT`, and `curbuf.b_ml.ml_mfp`, if
/// non-null, must point to a live `MemfileT`.
#[must_use]
pub unsafe fn get_cursor_line_ptr() -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.lnum;
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::ml_get_buf(curbuf, lnum) }
}

/// @return pointer to cursor position (`get_cursor_pos_ptr`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
#[must_use]
pub unsafe fn get_cursor_pos_ptr() -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { get_cursor_line_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col as usize;
    line[col..].to_vec()
}

/// @return length (excluding the NUL) of the cursor line
/// (`get_cursor_line_len`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
#[must_use]
pub unsafe fn get_cursor_line_len() -> ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.lnum;
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::ml_get_buf_len(curbuf, lnum) }
}

/// @return length (excluding the NUL) of the cursor position
/// (`get_cursor_pos_len`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
#[must_use]
pub unsafe fn get_cursor_pos_len() -> ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { get_cursor_line_len() };
    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col;
    len - col
}

/// @return the character under the cursor (`gchar_cursor`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
#[must_use]
pub unsafe fn gchar_cursor() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let p = unsafe { get_cursor_pos_ptr() };
    crate::mbyte::utf_ptr2char(&p)
}

/// Return the character immediately before the cursor
/// (`char_before_cursor`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`]. Also touches
/// `crate::option_vars::OPTION_VARS` (via `crate::mbyte::utf_head_off`).
#[must_use]
pub unsafe fn char_before_cursor() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col;
    if col == 0 {
        return -1;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { get_cursor_line_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let prev_len = unsafe { crate::mbyte::utf_head_off(&line, (col - 1) as usize) } + 1;
    crate::mbyte::utf_ptr2char(&line[(col - prev_len) as usize..])
}

/// Write a character at the current cursor position. It is directly
/// written into the block (`pchar_cursor`).
///
/// See this module's own doc comment for why this writes into
/// `buf.b_ml.ml_line_ptr` directly rather than through
/// `ml_get_buf_mut`'s own (disconnected-copy) return value.
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
pub unsafe fn pchar_cursor(c: u8) {
    // SAFETY: forwarded from this function's own safety doc.
    let (lnum, col) = {
        let curwin = unsafe { &*crate::globals::GLOBALS.get_mut().curwin };
        (curwin.w_cursor.lnum, curwin.w_cursor.col as usize)
    };
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    // Called only for its side effects (loading+dirtying the cached
    // line, undo bookkeeping) - the returned copy itself is discarded,
    // see this module's own doc comment for why the real write below
    // goes through `ml_line_ptr` directly instead.
    // SAFETY: forwarded from this function's own safety doc.
    let _ = unsafe { crate::memline::ml_get_buf_mut(curbuf, lnum) };
    if let Some(line) = curbuf.b_ml.ml_line_ptr.as_mut() {
        line[col] = c;
    }
}

/// Make sure `curwin.w_cursor` is not on the NUL at the end of the
/// line. Allow it when in Visual mode and `'selection'` is not "old"
/// (`adjust_cursor_col`).
///
/// # Safety
/// Same as [`get_cursor_line_ptr`].
pub unsafe fn adjust_cursor_col() {
    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col;
    let visual_active = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active;
    let sel_first_byte = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_sel
        .as_deref()
        .and_then(|s| s.first())
        .copied();

    if col > 0
        && (!visual_active || sel_first_byte == Some(b'o'))
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { gchar_cursor() } == 0
    {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col -= 1;
    }
}

/// Increment the cursor position. See [`crate::memline::inc`] for
/// return values (`inc_cursor`).
///
/// # Safety
/// Same as [`crate::memline::inc`]. `crate::globals::GLOBALS.curwin`
/// must additionally be a valid, non-null pointer to a live `WinT`.
pub unsafe fn inc_cursor() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::inc(&mut curwin.w_cursor) }
}

/// Decrement the line pointer, crossing line boundaries as necessary.
/// See [`crate::memline::dec`] for return values (`dec_cursor`).
///
/// # Safety
/// Same as [`crate::memline::dec`]. `crate::globals::GLOBALS.curwin`
/// must additionally be a valid, non-null pointer to a live `WinT`.
pub unsafe fn dec_cursor() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::dec(&mut curwin.w_cursor) }
}

/// @return the screen position of the cursor (`getviscol`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` whose own `w_buffer` is also valid.
#[must_use]
pub unsafe fn getviscol() -> ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    let mut x = 0;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::plines::getvvcol(curwin, &mut (*curwin).w_cursor, Some(&mut x), None, None, 0);
    }
    x
}

/// @return the screen position of character `col` with a `coladd` in
/// the cursor line (`getviscol2`).
///
/// # Safety
/// Same as [`getviscol`].
#[must_use]
pub unsafe fn getviscol2(col: ColnrT, coladd: ColnrT) -> ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { &*curwin }.w_cursor.lnum;
    let mut pos = PosT { lnum, col, coladd };
    let mut x = 0;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::plines::getvvcol(curwin, &mut pos, Some(&mut x), None, None, 0) };
    x
}

/// The internal implementation shared by [`coladvance`]/
/// [`coladvance_force`]/[`getvpos`] (`coladvance2`).
///
/// Its `'virtualedit'`-space-filling branch (reached only when
/// `virtual_active(wp)` is true, `addspaces` is set, AND the computed
/// column doesn't already land on the target - a genuinely narrow,
/// opt-in-only combination) is `unimplemented!()`: it needs
/// `change.c`'s `inserted_bytes` plus a real `ml_replace` call wired
/// through the undo/redraw-notification machinery, neither assembled
/// yet. Every other case (in particular every call with
/// `'virtualedit'` unset, the overwhelmingly common case) is fully
/// translated.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid. Touches `crate::globals::GLOBALS` and
/// `crate::option_vars::OPTION_VARS`.
unsafe fn coladvance2(
    wp: *mut WinT,
    pos: &mut PosT,
    addspaces: bool,
    finetune: bool,
    wcol_arg: ColnrT,
) -> i32 {
    let mut wcol = wcol_arg;
    let mut idx: i32;
    let mut col: ColnrT;
    let mut head = 0;

    let sel_first_byte = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_sel
        .as_deref()
        .and_then(|s| s.first())
        .copied();
    let (state, restart_edit, visual_active) = {
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        (g.State, g.restart_edit, g.Visual.active)
    };
    let one_more = (state & crate::state_defs::mode::INSERT as i32) != 0
        || (state & crate::state_defs::mode::TERMINAL as i32) != 0
        || restart_edit != 0
        || (visual_active && sel_first_byte != Some(b'o'))
        || ((crate::option::get_ve_flags(unsafe { &*wp })
            & crate::option_vars::opt_ve_flag::ONEMORE)
            != 0
            && wcol < MAXCOL);
    let one_more_i32 = i32::from(one_more);

    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *(*wp).w_buffer };
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get_buf(buf, pos.lnum) };
    // SAFETY: forwarded from this function's own safety doc.
    let linelen = unsafe { crate::memline::ml_get_buf_len(buf, pos.lnum) };

    if wcol == MAXCOL {
        idx = linelen - 1 + one_more_i32;
        col = wcol;

        if (addspaces || finetune) && !visual_active {
            // SAFETY: forwarded from this function's own safety doc.
            let size = unsafe { crate::plines::linetabsize(wp, pos.lnum) } + one_more_i32;
            // SAFETY: forwarded from this function's own safety doc.
            let wpref = unsafe { &mut *wp };
            wpref.w_curswant = size;
            if wpref.w_curswant > 0 {
                wpref.w_curswant -= 1;
            }
        }
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let wpref = unsafe { &mut *wp };
        // SAFETY: forwarded from this function's own safety doc.
        let width = wpref.w_view_width - unsafe { crate::r#move::win_col_off(wpref) };
        let mut csize = 0;

        if finetune
            && wpref.w_onebuf_opt.wo_wrap != 0
            && wpref.w_view_width != 0
            && wcol >= width
            && width > 0
        {
            // SAFETY: forwarded from this function's own safety doc.
            csize = unsafe { crate::plines::linetabsize_eol(wp, pos.lnum) };
            if csize > 0 {
                csize -= 1;
            }

            if wcol / width > csize / width
                && (state & crate::state_defs::mode::INSERT as i32 == 0 || wcol > csize + 1)
            {
                wcol = (csize / width + 1) * width - 1;
            }
        }

        // SAFETY: forwarded from this function's own safety doc.
        let (mut csarg, cstype) = unsafe { crate::plines::init_charsize_arg(wp, pos.lnum, &line) };
        let mut ci = crate::mbyte::utf_ptr2str_char_info(&line);
        col = 0;
        while col <= wcol && line.get(ci.pos).copied().unwrap_or(0) != 0 {
            let cs = if cstype == crate::plines::CsType::Fast {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::plines::charsize_fast(&csarg, &line[ci.pos..], col, ci.chr.value) }
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    crate::plines::charsize_regular(
                        &mut csarg,
                        &line[ci.pos..],
                        ci.pos as i32,
                        col,
                        ci.chr.value,
                    )
                }
            };
            csize = cs.width;
            head = cs.head;
            col += cs.width;
            // SAFETY: forwarded from this function's own safety doc.
            ci = unsafe { crate::mbyte::utfc_next(&line, ci) };
        }
        idx = ci.pos as i32;

        // SAFETY: forwarded from this function's own safety doc.
        let is_virtual_active = unsafe { crate::state::virtual_active(&*wp) };
        if col > wcol || (!is_virtual_active && !one_more) {
            idx -= 1;
            csize -= head;
            col -= csize;
        }

        if is_virtual_active
            && addspaces
            && wcol >= 0
            && ((col != wcol && col != wcol + 1) || csize > 1)
        {
            // 'virtualedit' is set: the difference between wcol and
            // col needs to be filled with spaces - see this
            // function's own doc comment for why this is deferred.
            unimplemented!(
                "coladvance2: 'virtualedit' space-filling needs change.c's \
                 inserted_bytes + a real ml_replace call"
            );
        }
    }

    pos.col = idx.max(0);
    pos.coladd = 0;

    if finetune {
        if wcol == MAXCOL {
            if !one_more {
                let mut scol = 0;
                let mut ecol = 0;
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    crate::plines::getvcol(wp, pos, Some(&mut scol), None, Some(&mut ecol), 0);
                }
                pos.coladd = ecol - scol;
            }
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            let view_width = unsafe { &*wp }.w_view_width;
            let b = wcol - col;
            if b > 0 && b < (MAXCOL - 2 * view_width) {
                pos.coladd = b;
            }
            col += b;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *(*wp).w_buffer };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::mark::mark_mb_adjustpos(buf, pos) };

    if wcol < 0 || col < wcol {
        crate::vim_defs::FAIL
    } else {
        crate::vim_defs::OK
    }
}

/// Go to column `wcol`, and add/insert white space as necessary to get
/// the cursor in that column. The caller must have saved the cursor
/// line for undo! (`coladvance_force`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` whose own `w_buffer` is also valid.
pub unsafe fn coladvance_force(wcol: ColnrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let rc = unsafe { coladvance2(curwin, &mut (*curwin).w_cursor, true, false, wcol) };

    if wcol == MAXCOL {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *curwin }.w_valid &= !i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL);
    } else {
        // Virtcol is valid.
        // SAFETY: forwarded from this function's own safety doc.
        crate::r#move::set_valid_virtcol(unsafe { &mut *curwin }, wcol);
    }
    rc
}

/// Try to advance the Cursor to the specified screen column. If
/// virtual editing: fine tune the cursor position. Note that all
/// virtual positions off the end of a line should share a
/// `wp.w_cursor.col` value (this is equal to the line's own length),
/// beginning at coladd 0 (`coladvance`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid. `crate::globals::GLOBALS.curwin` must
/// also be valid (see this function's own note about a verified
/// upstream quirk).
pub unsafe fn coladvance(wp: *mut WinT, wcol: ColnrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let rc = unsafe { getvpos(wp, &mut (*wp).w_cursor, wcol) };

    if wcol == MAXCOL || rc == crate::vim_defs::FAIL {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *wp }.w_valid &= !i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL);
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let (buf_ptr, lnum, col) = unsafe {
            let wpref = &*wp;
            (wpref.w_buffer, wpref.w_cursor.lnum, wpref.w_cursor.col)
        };
        // SAFETY: forwarded from this function's own safety doc.
        let buf = unsafe { &mut *buf_ptr };
        // SAFETY: forwarded from this function's own safety doc.
        let line = unsafe { crate::memline::ml_get_buf(buf, lnum) };
        if line.get(col as usize).copied() != Some(crate::ascii_defs::TAB) {
            // Virtcol is valid when not on a TAB.
            //
            // NOTE: the original genuinely calls
            // `set_valid_virtcol(curwin, wcol)` here - the GLOBAL
            // current window, not `wp` - a verified upstream quirk
            // (matches `option.rs`'s `get_scrolloffpad_value`
            // precedent), kept exactly as-is rather than "fixed" to
            // use `wp`.
            // SAFETY: forwarded from this function's own safety doc.
            let curwin = unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin };
            crate::r#move::set_valid_virtcol(curwin, wcol);
        }
    }
    rc
}

/// Return in `pos` the position of the cursor advanced to screen
/// column `wcol` (`getvpos`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid.
pub unsafe fn getvpos(wp: *mut WinT, pos: &mut PosT, wcol: ColnrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let is_virtual_active = unsafe { crate::state::virtual_active(&*wp) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { coladvance2(wp, pos, false, is_virtual_active, wcol) }
}

/// Make sure `win.w_cursor.lnum` is valid (`check_cursor_lnum`).
///
/// # Safety
/// `win` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid.
pub unsafe fn check_cursor_lnum(win: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let (buf_ptr, cursor_lnum) = unsafe {
        let w = &*win;
        (w.w_buffer, w.w_cursor.lnum)
    };
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*buf_ptr }.b_ml.ml_line_count;

    if cursor_lnum > line_count {
        // If there is a closed fold at the end of the file, put the
        // cursor in its first line. Otherwise in the last line.
        //
        // The original passes `&win->w_cursor.lnum` as `firstp` here,
        // but that's only ever written on the "a fold WAS found" path
        // - `unimplemented!()` today (see fold.rs's own module doc) -
        // so `None` is equivalent in practice; using it directly would
        // also alias `wref` with a mutable borrow of its own field,
        // which Rust's borrow checker (correctly) rejects even though
        // C's pointer aliasing allows it.
        // SAFETY: forwarded from this function's own safety doc.
        let wref = unsafe { &mut *win };
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { crate::fold::has_folding(wref, line_count, None, None) } {
            wref.w_cursor.lnum = line_count;
        }
    }
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*win }.w_cursor.lnum <= 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *win }.w_cursor.lnum = 1;
    }
}

/// Make sure `wp.w_cursor` is on a valid character (`check_cursor`).
///
/// # Safety
/// Same as [`check_cursor_lnum`]/[`check_cursor_col`].
pub unsafe fn check_cursor(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_cursor_lnum(wp) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_cursor_col(wp) };
}

/// Return `lnum`'s line-count distance from `wp.w_cursor.lnum`, fold-aware
/// (`get_cursor_rel_lnum`).
///
/// Only the "no folding" fast path (`lnum == cursor ||
/// !has_any_folding(wp)`) is translated - the fold-skipping loop,
/// reached only when [`crate::fold::has_any_folding`] returns true, is
/// `unimplemented!()` directly in this function (a separate case from
/// `fold::has_folding`'s own internal panic, not reached here at all
/// since this function never calls `has_folding`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also valid.
#[must_use]
pub unsafe fn get_cursor_rel_lnum(wp: *mut WinT, lnum: crate::pos_defs::LinenrT) -> LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let wref = unsafe { &*wp };
    let cursor = wref.w_cursor.lnum;
    // SAFETY: forwarded from this function's own safety doc.
    if lnum == cursor || !unsafe { crate::fold::has_any_folding(wref) } {
        return lnum - cursor;
    }
    unimplemented!(
        "get_cursor_rel_lnum: the fold-skipping loop is not yet translated (only the \
         \"hasAnyFolding() == false\" fast path is)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::WinT;
    use crate::globals::{global_state_test_lock, GLOBALS};

    /// RAII guard installing `win`/`buf` as curwin/curbuf, restoring
    /// the previous pointers on drop (including on test panic via
    /// unwinding). Holds `global_state_test_lock` for its entire
    /// lifetime, matching `mark.rs`'s own `MarkTestGuard`/`CurbufGuard`
    /// precedent - needed since `ml_open` (used to build the test
    /// memline below) touches shared `GLOBALS.got_int` internally.
    struct CursorTestGuard {
        prev_curwin: *mut WinT,
        prev_curbuf: *mut BufT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CursorTestGuard {
        fn set(win: *mut WinT, buf: *mut BufT) -> Self {
            let _lock = global_state_test_lock();
            let globals = unsafe { GLOBALS.get_mut() };
            let guard = CursorTestGuard {
                prev_curwin: globals.curwin,
                prev_curbuf: globals.curbuf,
                _lock,
            };
            globals.curwin = win;
            globals.curbuf = buf;
            guard
        }
    }

    impl Drop for CursorTestGuard {
        fn drop(&mut self) {
            let globals = unsafe { GLOBALS.get_mut() };
            globals.curwin = self.prev_curwin;
            globals.curbuf = self.prev_curbuf;
        }
    }

    /// Installs `win`/`buf` as curwin/curbuf (acquiring the lock
    /// first, per `mark.rs`'s `open_and_set_curbuf` precedent), then
    /// opens a fresh memline for `buf` and replaces line 1 with
    /// `line`. Callers must close `buf.b_ml.ml_mfp` themselves after
    /// the guard is dropped.
    fn open_and_set_test_buf(win: &mut WinT, buf: &mut BufT, line: &[u8]) -> CursorTestGuard {
        let guard = CursorTestGuard::set(win as *mut WinT, buf as *mut BufT);
        assert_eq!(unsafe { crate::memline::ml_open(buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(buf, 1, line) },
            crate::vim_defs::OK
        );
        guard
    }

    fn close_buf_with_memline(buf: BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn check_pos_clamps_lnum_and_col() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        let mut pos = PosT { lnum: 99, col: 0, coladd: 0 };
        unsafe { check_pos(&mut buf, &mut pos) };
        assert_eq!(pos.lnum, 1); // clamped to ml_line_count

        let mut pos2 = PosT { lnum: 1, col: 99, coladd: 0 };
        unsafe { check_pos(&mut buf, &mut pos2) };
        assert_eq!(pos2.col, 2); // clamped to ml_get_buf_len("hi") == 2

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_pos_leaves_col_zero_untouched() {
        // col == 0 is never clamped (matches the original's own `if
        // (pos->col > 0)` guard) - even on a fully out-of-range lnum.
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        let mut pos = PosT { lnum: 1, col: 0, coladd: 0 };
        unsafe { check_pos(&mut buf, &mut pos) };
        assert_eq!(pos.col, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_col_empty_line_clamps_to_zero() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 5, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"\0");

        unsafe { check_cursor_col(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.col, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_col_within_bounds_is_unchanged() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 2, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        unsafe { check_cursor_col(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.col, 2);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_col_out_of_bounds_normal_mode_clamps_to_last_char() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 10, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        // Default GLOBALS.State is MODE_NORMAL (not Insert/Terminal),
        // no Visual, no 've' - so the cursor clamps to the LAST real
        // character (len-1), not one past it.
        unsafe { check_cursor_col(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.col, 4);
        assert_eq!(win.w_cursor.coladd, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_col_out_of_bounds_insert_mode_clamps_to_len() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 10, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        let g = unsafe { GLOBALS.get_mut() };
        let prev_state = g.State;
        g.State = crate::state_defs::mode::INSERT as i32;

        unsafe { check_cursor_col(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.col, 5); // one past the last char

        unsafe { GLOBALS.get_mut() }.State = prev_state;
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_col_negative_clamps_to_zero() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: -1, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        unsafe { check_cursor_col(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.col, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_col_maxcol_resets_coladd() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            // col == MAXCOL with a nonzero coladd is not a state
            // legitimate callers produce (coladvance2 always pairs
            // MAXCOL with coladd == 0), but this deliberately uses a
            // nonzero coladd anyway to exercise the wrapping_add fix
            // (`MAXCOL + coladd` would otherwise overflow i32) and
            // confirm the wrapped, nonsensical value still safely
            // resets to 0 via the `oldcol == MAXCOL` branch rather
            // than panicking or producing garbage.
            w_cursor: PosT { lnum: 1, col: MAXCOL, coladd: 7 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        unsafe { check_cursor_col(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.col, 4); // clamped like any other out-of-range col
        assert_eq!(win.w_cursor.coladd, 0); // reset because oldcol == MAXCOL

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_col_ve_all_preserves_coladd_on_last_character() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 4, coladd: 3 }, // 'o', the LAST char
            ..Default::default()
        };
        win.w_onebuf_opt.wo_ve_flags = crate::option_vars::opt_ve_flag::ALL;
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        // new_col + 1 == len, so the getvcol-based clamp is skipped -
        // coladd is preserved exactly as oldcoladd - new_col.
        unsafe { check_cursor_col(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.col, 4);
        assert_eq!(win.w_cursor.coladd, 3);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_col_ve_all_clamps_coladd_via_getvcol_mid_line() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 2, coladd: 5 }, // 'l', NOT the last char
            ..Default::default()
        };
        win.w_onebuf_opt.wo_ve_flags = crate::option_vars::opt_ve_flag::ALL;
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        // new_col + 1 < len, so getvcol clamps coladd to (ce - cs) for
        // the character at the new column - a plain 1-cell ASCII
        // character has ce == cs, so coladd is clamped all the way to 0.
        unsafe { check_cursor_col(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.col, 2);
        assert_eq!(win.w_cursor.coladd, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_visual_pos_clamps_lnum_and_col() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        unsafe { GLOBALS.get_mut() }.Visual.start = PosT { lnum: 99, col: 5, coladd: 3 };
        unsafe { check_visual_pos() };
        let start = unsafe { GLOBALS.get_mut() }.Visual.start;
        assert_eq!(start, PosT { lnum: 1, col: 0, coladd: 0 });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_visual_pos_clamps_col_when_lnum_valid() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        unsafe { GLOBALS.get_mut() }.Visual.start = PosT { lnum: 1, col: 99, coladd: 3 };
        unsafe { check_visual_pos() };
        let start = unsafe { GLOBALS.get_mut() }.Visual.start;
        assert_eq!(start, PosT { lnum: 1, col: 2, coladd: 0 }); // ml_get_len("hi") == 2

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn cursor_line_and_pos_accessors_match_expectations() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 2, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        assert_eq!(unsafe { get_cursor_line_ptr() }, b"hello\0".to_vec());
        assert_eq!(unsafe { get_cursor_pos_ptr() }, b"llo\0".to_vec());
        assert_eq!(unsafe { get_cursor_line_len() }, 5);
        assert_eq!(unsafe { get_cursor_pos_len() }, 3);
        assert_eq!(unsafe { gchar_cursor() }, i32::from(b'l'));

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn char_before_cursor_returns_minus_one_at_col_zero() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 0, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        assert_eq!(unsafe { char_before_cursor() }, -1);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn char_before_cursor_returns_preceding_ascii_char() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 2, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        // "hello": col 2 is the second 'l', so the char immediately
        // before it (col 1) is 'e'.
        assert_eq!(unsafe { char_before_cursor() }, i32::from(b'e'));

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn pchar_cursor_writes_directly_into_the_line() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 1, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        unsafe { pchar_cursor(b'A') };
        assert_eq!(unsafe { get_cursor_line_ptr() }, b"hAllo\0".to_vec());

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn adjust_cursor_col_steps_back_off_trailing_nul() {
        let mut buf = BufT::default();
        // Cursor sitting on the line's own NUL (col == line length).
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 5, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        unsafe { adjust_cursor_col() };
        assert_eq!(unsafe { &*GLOBALS.get_mut().curwin }.w_cursor.col, 4);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn adjust_cursor_col_is_noop_when_not_on_nul() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 2, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        unsafe { adjust_cursor_col() };
        assert_eq!(unsafe { &*GLOBALS.get_mut().curwin }.w_cursor.col, 2);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn inc_cursor_and_dec_cursor_move_curwin_w_cursor() {
        let mut buf = BufT::default();
        let mut win = WinT { w_cursor: PosT { lnum: 1, col: 0, coladd: 0 }, ..Default::default() };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        assert_eq!(unsafe { inc_cursor() }, 0);
        assert_eq!(unsafe { &*GLOBALS.get_mut().curwin }.w_cursor.col, 1);

        assert_eq!(unsafe { dec_cursor() }, 0);
        assert_eq!(unsafe { &*GLOBALS.get_mut().curwin }.w_cursor.col, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn getviscol_plain_ascii_middle_position() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 2, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        assert_eq!(unsafe { getviscol() }, 2);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn getviscol2_explicit_col_and_coladd() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        assert_eq!(unsafe { getviscol2(3, 0) }, 3);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn coladvance_lands_on_target_column() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        let rc = unsafe { coladvance(&mut win as *mut WinT, 2) };
        assert_eq!(rc, crate::vim_defs::OK);
        assert_eq!(win.w_cursor.col, 2);
        assert_eq!(win.w_cursor.coladd, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn coladvance_maxcol_lands_on_last_real_character() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        let rc = unsafe { coladvance(&mut win as *mut WinT, MAXCOL) };
        assert_eq!(rc, crate::vim_defs::OK);
        // "hello" is 5 chars (indices 0..4); with one_more=false
        // (default State is MODE_NORMAL, not Insert/Terminal, no 've'),
        // the cursor lands on the LAST real character, not past it.
        assert_eq!(win.w_cursor.col, 4);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn coladvance_force_basic_via_curwin() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        let rc = unsafe { coladvance_force(3) };
        assert_eq!(rc, crate::vim_defs::OK);
        assert_eq!(unsafe { &*GLOBALS.get_mut().curwin }.w_cursor.col, 3);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn getvpos_does_not_mutate_the_actual_cursor() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        let mut pos = PosT { lnum: 1, col: 0, coladd: 0 };
        let rc = unsafe { getvpos(&mut win as *mut WinT, &mut pos, 3) };
        assert_eq!(rc, crate::vim_defs::OK);
        assert_eq!(pos.col, 3);
        // The real cursor (win.w_cursor) is untouched.
        assert_eq!(win.w_cursor.col, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_lnum_clamps_to_last_line_when_too_high() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 99, col: 0, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        unsafe { check_cursor_lnum(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.lnum, 1); // only 1 line in this buffer

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_lnum_clamps_to_one_when_non_positive() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 0, col: 0, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        unsafe { check_cursor_lnum(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.lnum, 1);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_lnum_leaves_valid_lnum_untouched() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hi\0");

        unsafe { check_cursor_lnum(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.lnum, 1);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn check_cursor_clamps_both_lnum_and_col() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 99, col: 99, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        unsafe { check_cursor(&mut win as *mut WinT) };
        assert_eq!(win.w_cursor.lnum, 1); // only 1 line
        assert_eq!(win.w_cursor.col, 4); // clamped to the last real char

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_cursor_rel_lnum_basic_distance_with_no_folds() {
        let mut buf = BufT::default();
        let mut win = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_cursor: PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        };
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"one\0");
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(&mut buf, 1, b"two\0", 4, false) },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(&mut buf, 2, b"three\0", 6, false) },
            crate::vim_defs::OK
        );

        assert_eq!(unsafe { get_cursor_rel_lnum(&mut win as *mut WinT, 3) }, 2);
        assert_eq!(unsafe { get_cursor_rel_lnum(&mut win as *mut WinT, 1) }, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }
}
