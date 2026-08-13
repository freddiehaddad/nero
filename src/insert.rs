//! Translated from `src/nvim/insert.c` (tractable core only).
//!
//! `insert.c` (~4400 lines) is Insert mode's own state machine: entry/
//! exit, key handling, backspace, digraphs, the replace-mode "pop"
//! stack, and much more - almost none of that is attempted here, since
//! it needs real buffer modification, the redraw pipeline, and
//! Insert-mode-specific global state (`stop_insert`/`Insstart`/etc.)
//! none of which are translated yet.
//!
//! Translated: [`get_nolist_virtcol`] - the value `w_virtcol` would
//! have if `'list'` were off, unless `'cpo'` contains the `'L'`
//! flag. Every real dependency (`getvcol_nolist`/`validate_virtcol`/
//! `vim_strchr`, `option_vars::CPO_LISTWM`) already existed;
//! translated ahead of its own real callers (`ins_tab`/several
//! others in this same file, none translated), matching this crate's
//! established "small, self-contained, no design freedom to get
//! wrong" precedent for translating ahead of a real caller.
//!
//! Also translated: [`beginline`]/[`oneright`]/[`oneleft`] - cursor-
//! movement helpers used well beyond Insert mode itself (also real
//! callers in `normal.c`/`ops.c`, neither translated yet). Needed
//! `move.c`'s `adjust_skipcol` (harvested alongside, in `move.rs`,
//! its own real home - see that file's own module doc), plus already-
//! real `state.c`'s `virtual_active`, `cursor.c`'s `coladvance`/
//! `getviscol`/`get_cursor_line_ptr`/`get_cursor_pos_ptr`, `mbyte.c`'s
//! `utf_ptr2char`/`utfc_ptr2len`/`mb_adjust_cursor`, `charset.c`'s
//! `vim_isprintc`/`ptr2cells`, and `option.c`'s `get_ve_flags`.

use crate::pos_defs::ColnrT;
use crate::vim_defs::{FAIL, OK};

/// Bytes displaced by Replace mode (`replace_stack`).
static REPLACE_STACK: crate::globals::GlobalCell<Vec<u8>> =
    crate::globals::GlobalCell::new(Vec::new());

/// Push replaced bytes onto the Replace-mode stack (`replace_push`).
///
/// # Safety
/// Must not run concurrently with another replacement-stack operation;
/// reads `GLOBALS.replace_offset`.
pub unsafe fn replace_push(bytes: &[u8]) {
    // SAFETY: forwarded from this function's own safety doc.
    let offset = unsafe { crate::globals::GLOBALS.get_mut() }.replace_offset;
    let Ok(offset) = usize::try_from(offset) else {
        return;
    };
    // SAFETY: forwarded from this function's own safety doc.
    let stack = unsafe { REPLACE_STACK.get_mut() };
    if stack.len() < offset {
        return;
    }
    let position = stack.len() - offset;
    stack.splice(position..position, bytes.iter().copied());
}

/// Push a NUL separator onto the replacement stack
/// (`replace_push_nul`).
///
/// # Safety
/// Forwarded from [`replace_push`].
pub unsafe fn replace_push_nul() {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { replace_push(&[crate::ascii_defs::NUL]) };
}

/// Join two replacement-stack entries by removing the selected NUL
/// separator (`replace_join`).
///
/// `offset == 0` removes the topmost separator, `1` the next one
/// down, and so on.
///
/// # Safety
/// Must not run concurrently with another replacement-stack operation.
pub unsafe fn replace_join(mut offset: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let stack = unsafe { REPLACE_STACK.get_mut() };
    for index in (0..stack.len()).rev() {
        if stack[index] == crate::ascii_defs::NUL {
            let remove = offset <= 0;
            offset -= 1;
            if remove {
                stack.remove(index);
                return;
            }
        }
    }
}

/// Truncate trailing whitespace from an Insert-mode line
/// (`truncate_spaces`).
///
/// `line` must include writable storage at `line[length]` for the new
/// NUL terminator.
///
/// # Safety
/// Reads `GLOBALS.State`; forwarded from [`replace_join`].
pub unsafe fn truncate_spaces(line: &mut [u8], length: usize) {
    let mut index = length as isize - 1;
    while index >= 0
        && crate::ascii_defs::ascii_iswhite(i32::from(line[index as usize]))
    {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::globals::GLOBALS.get_mut() }.State
            & crate::state_defs::mode::REPLACE_FLAG as i32
            != 0
        {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { replace_join(0) };
        }
        index -= 1;
    }
    line[(index + 1) as usize] = crate::ascii_defs::NUL;
}

/// Peek at the replacement stack and pop its top byte only when it is
/// NUL (`replace_pop_if_nul`).
///
/// # Safety
/// Must not run concurrently with another replacement-stack operation.
#[allow(dead_code)]
unsafe fn replace_pop_if_nul() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let stack = unsafe { REPLACE_STACK.get_mut() };
    let ch = stack.last().copied().map_or(-1, i32::from);
    if ch == i32::from(crate::ascii_defs::NUL) {
        stack.pop();
    }
    ch
}

/// Get the value `w_virtcol` would have if `'list'` were off, unless
/// `'cpo'` contains the `'L'` flag (`get_nolist_virtcol`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` - same requirement as
/// [`crate::plines::getvcol_nolist`]/[`crate::move::validate_virtcol`].
#[must_use]
pub unsafe fn get_nolist_virtcol() -> ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &mut *curwin };

    // check validity of cursor in current buffer
    if win.w_buffer.is_null()
        // SAFETY: forwarded from this function's own safety doc.
        || unsafe { (*win.w_buffer).b_ml.ml_mfp }.is_null()
        // SAFETY: forwarded from this function's own safety doc.
        || win.w_cursor.lnum > unsafe { (*win.w_buffer).b_ml.ml_line_count }
    {
        return 0;
    }

    if win.w_onebuf_opt.wo_list != 0
        // SAFETY: forwarded from this function's own safety doc.
        && crate::strings::vim_strchr(
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo.as_deref().unwrap_or(&[]),
            i32::from(crate::option_vars::CPO_LISTWM),
        )
        .is_none()
    {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::plines::getvcol_nolist(&mut win.w_cursor) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::r#move::validate_virtcol(curwin) };
    win.w_virtcol
}

/// `beginline`'s own bit flags (`insert.h`'s anonymous enum:
/// `BL_WHITE`/`BL_SOL`/`BL_FIX`).
mod bl {
    /// Cursor on first non-white in the line.
    pub const WHITE: i32 = 1;
    /// Use `'startofline'`.
    pub const SOL: i32 = 2;
    /// Don't leave cursor on a NUL.
    pub const FIX: i32 = 4;
}
pub use bl::{FIX as BL_FIX, SOL as BL_SOL, WHITE as BL_WHITE};

/// Trigger `event` and take care of fixing undo
/// (`ins_apply_autocmds`).
///
/// If the autocommand changed the buffer then `u_savesub` may have
/// been called, leaving undo unprepared to start a new line; an empty
/// `u_save` fixes that. Not done when leaving Insert mode, where a new
/// line is not about to be started anyway.
///
/// # Safety
/// `GLOBALS.curbuf`/`GLOBALS.curwin` must point at live objects, and
/// [`crate::undo::u_save`]'s own safety doc applies.
pub unsafe fn ins_apply_autocmds(event: crate::autocmd_defs::EventT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    let tick = crate::buffer::buf_get_changedtick(unsafe { &*curbuf });

    // SAFETY: forwarded from this function's own safety doc.
    let r = crate::autocmd::apply_autocmds(event, None, None, false, Some(unsafe { &*curbuf }));

    // SAFETY: forwarded from this function's own safety doc.
    let tick_now = crate::buffer::buf_get_changedtick(unsafe { &*curbuf });
    if event != crate::autocmd_defs::EventT::InsertLeave && tick != tick_now {
        // SAFETY: forwarded from this function's own safety doc.
        let lnum = unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_cursor.lnum };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::undo::u_save(lnum, lnum + 1) };
    }

    r
}

/// Redraw the line spell-checking asked for, if any
/// (`check_spell_redraw`).
///
/// The redraw may be skipped again, so the pending line number is
/// cleared first.
///
/// # Safety
/// `GLOBALS.curwin` must point at a live `WinT`, and
/// [`crate::drawscreen::redraw_winline`]'s own safety doc applies.
pub unsafe fn check_spell_redraw() {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    if g.spell_redraw_lnum == 0 {
        return;
    }
    let lnum = g.spell_redraw_lnum;
    g.spell_redraw_lnum = 0;
    let curwin = g.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::drawscreen::redraw_winline(curwin, lnum) };
}

/// Like [`crate::change::del_char`], but never move before column
/// `limit_col` (`del_char_after_col`).
///
/// Only matters when there are composing characters: adjusting the
/// cursor to a character start can walk back past `limit_col`, so it
/// is walked forward again when that happens.
///
/// @return whether something was deleted.
///
/// # Safety
/// `GLOBALS.curwin` and its buffer must be live. Forwards
/// [`crate::mbyte::mb_adjust_cursor`]/
/// [`crate::cursor::get_cursor_pos_ptr`]/
/// [`crate::change::del_bytes`]/[`crate::change::del_char`]'s own
/// safety docs.
pub unsafe fn del_char_after_col(limit_col: i32) -> bool {
    if limit_col < 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::change::del_char(false) };
        return true;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let ecol = unsafe { (*curwin).w_cursor.col } + 1;

    // Make sure the cursor is at the start of a character, but skip
    // forward again when going too far back because of a composing
    // character.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::mbyte::mb_adjust_cursor() };
    loop {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { (*curwin).w_cursor.col } >= limit_col {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let l = crate::mbyte::utf_ptr2len(&unsafe { crate::cursor::get_cursor_pos_ptr() });
        if l == 0 {
            // End of line.
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*curwin).w_cursor.col += l };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let at_nul = unsafe { crate::cursor::get_cursor_pos_ptr() }.first().is_none_or(|&c| c == 0);
    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { (*curwin).w_cursor.col };
    if at_nul || col == ecol {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::change::del_bytes(ecol - col, false, true) };
    true
}

/// Whether a new undo point is still needed for the current insert
/// (`ins_need_undo_get`).
///
/// # Safety
/// Reads `GLOBALS`.
#[must_use]
pub unsafe fn ins_need_undo_get() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.Ins.need_undo
}

/// Whether C-indenting may still be applied for the current insert
/// (`get_can_cindent`).
///
/// # Safety
/// Reads `GLOBALS`.
#[must_use]
pub unsafe fn get_can_cindent() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.Ins.can_cindent
}

/// Set whether C-indenting may still be applied (`set_can_cindent`).
///
/// # Safety
/// Mutates `GLOBALS`.
pub unsafe fn set_can_cindent(val: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.Ins.can_cindent = val;
}

/// The effective prompt for `buf` (`buf_prompt_text`).
///
/// Falls back to `"% "` when the buffer has no prompt of its own,
/// matching the original's own default.
///
/// # Safety
/// `buf` must point at a live `BufT`.
#[must_use]
pub unsafe fn buf_prompt_text(buf: *const crate::buffer_defs::BufT) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { &(*buf).b_prompt_text } {
        Some(text) => text.clone(),
        None => b"% ".to_vec(),
    }
}

/// The effective prompt for the current buffer (`prompt_text`).
///
/// # Safety
/// `GLOBALS.curbuf` must point at a live `BufT`.
#[must_use]
pub unsafe fn prompt_text() -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { buf_prompt_text(curbuf) }
}

/// Whether the cursor is in the editable part of the prompt line
/// (`prompt_curpos_editable`).
///
/// # Safety
/// `GLOBALS.curwin`/`GLOBALS.curbuf` must point at live objects.
#[must_use]
pub unsafe fn prompt_curpos_editable() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let (curwin, curbuf) = (g.curwin, g.curbuf);
    // SAFETY: forwarded from this function's own safety doc.
    let cursor = unsafe { (*curwin).w_cursor };
    // SAFETY: forwarded from this function's own safety doc.
    let start = unsafe { (*curbuf).b_prompt_start.mark };

    cursor.lnum > start.lnum || (cursor.lnum == start.lnum && cursor.col >= start.col)
}

/// Stop displaying the "$" of a change operator, and redraw the line
/// it was on (`undisplay_dollar`).
///
/// # Safety
/// `GLOBALS.curwin` must point at a live `WinT`, and
/// [`crate::drawscreen::redraw_winline`]'s own safety doc applies.
pub unsafe fn undisplay_dollar() {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    if g.dollar_vcol < 0 {
        return;
    }
    g.dollar_vcol = -1;
    let curwin = g.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { (*curwin).w_cursor.lnum };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::drawscreen::redraw_winline(curwin, lnum) };
}

/// The last inserted text (`last_insert`, a file-static `String` in
/// `insert.c`).
///
/// `set_last_insert` fills this in; `get_last_insert` reads it back,
/// skipping [`LAST_INSERT_SKIP`] leading bytes.
static LAST_INSERT: crate::globals::GlobalCell<Option<Vec<u8>>> = crate::globals::GlobalCell::new(None);

/// Number of bytes in front of the previous insert to skip when
/// reading [`LAST_INSERT`] back (`last_insert_skip`).
static LAST_INSERT_SKIP: crate::globals::GlobalCell<usize> = crate::globals::GlobalCell::new(0);

/// Store a single character as the "last insert", for the replace
/// command (`set_last_insert`).
///
/// A control character is stored with a leading CTRL-V, matching how
/// it would have to be typed; a trailing `<Esc>` always terminates the
/// stored text.
///
/// # Safety
/// Mutates the `LAST_INSERT`/`LAST_INSERT_SKIP` file-statics.
pub unsafe fn set_last_insert(c: i32) {
    let mut s = Vec::new();
    // Use the CTRL-V only when entering a special char.
    if c < i32::from(b' ') || c == i32::from(crate::ascii_defs::DEL) {
        s.push(crate::ascii_defs::CTRL_V);
    }
    let mut tmp = [0u8; crate::mbyte_defs::MB_MAXBYTES * 3 + 5];
    let n = crate::keycodes::add_char2buf(c, &mut tmp);
    s.extend_from_slice(&tmp[..n]);
    s.push(crate::ascii_defs::ESC);

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        *LAST_INSERT.get_mut() = Some(s);
        *LAST_INSERT_SKIP.get_mut() = 0;
    }
}

/// The last inserted text (`get_last_insert`), past whatever leading
/// bytes were already present before the insert started.
///
/// # Safety
/// Reads the `LAST_INSERT`/`LAST_INSERT_SKIP` file-statics.
#[must_use]
pub unsafe fn get_last_insert() -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    let stored = unsafe { LAST_INSERT.get_mut() }.as_ref()?;
    // SAFETY: forwarded from this function's own safety doc.
    let skip = unsafe { *LAST_INSERT_SKIP.get_mut() };
    Some(stored.get(skip..).unwrap_or_default().to_vec())
}

/// The last inserted text with a trailing `<Esc>` removed
/// (`get_last_insert_save`).
///
/// # Safety
/// Same as [`get_last_insert`].
#[must_use]
pub unsafe fn get_last_insert_save() -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    let mut s = unsafe { get_last_insert() }?;
    if s.last() == Some(&crate::ascii_defs::ESC) {
        s.pop();
    }
    Some(s)
}

/// Releases the stored last-insert text (`free_last_insert`).
///
/// The skip count is deliberately retained: the original clears only
/// the owned `String`, not its separate `last_insert_skip` static.
///
/// # Safety
/// Mutates the `LAST_INSERT` file-static.
pub unsafe fn free_last_insert() {
    unsafe { *LAST_INSERT.get_mut() = None };
}

/// Clear the last-insert state, for tests only.
///
/// The original has no such function - a real session simply never
/// unsets `last_insert` once set. Tests need it so one test's stored
/// insert cannot leak into another's assumptions about the shared
/// file-static.
///
/// # Safety
/// Mutates the [`LAST_INSERT`]/[`LAST_INSERT_SKIP`] file-statics.
#[cfg(test)]
pub(crate) unsafe fn reset_last_insert_for_test() {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        *LAST_INSERT.get_mut() = None;
        *LAST_INSERT_SKIP.get_mut() = 0;
    }
}

/// Move the cursor up `n` lines in window `wp` (`cursor_up_inner`).
///
/// Takes care of closed folds; skips over concealed lines when
/// `skip_conceal` is set.
///
/// # Safety
/// `wp` must point at a live `WinT`. Forwards
/// [`crate::decoration::win_lines_concealed`]/
/// [`crate::decoration::decor_conceal_line`]/
/// [`crate::fold::has_folding`]'s own safety docs, and reads
/// `GLOBALS`/`OPTION_VARS`.
pub unsafe fn cursor_up_inner(wp: *mut crate::buffer_defs::WinT, n: crate::pos_defs::LinenrT, skip_conceal: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut lnum = unsafe { (*wp).w_cursor.lnum };
    let mut n = n;

    // SAFETY: forwarded from this function's own safety doc.
    let concealed = unsafe { crate::decoration::win_lines_concealed(&*wp) };

    if n >= lnum {
        lnum = 1;
    } else if concealed {
        // Count each sequence of folded lines as one logical line.

        // Go to the start of the current fold.
        // SAFETY: forwarded from this function's own safety doc.
        let _ = unsafe { crate::fold::has_folding(&mut *wp, lnum, Some(&mut lnum), None) };

        while n > 0 {
            n -= 1;
            // Move up one line.
            lnum -= 1;
            if lnum <= 1 {
                break;
            }
            // SAFETY: forwarded from this function's own safety doc.
            if skip_conceal && unsafe { crate::decoration::decor_conceal_line(&*wp, lnum - 1, true) } {
                n += 1;
            }
            // If we entered a fold, move to the beginning, unless in
            // Insert mode or when 'foldopen' contains "all": it will
            // open in a moment.
            // SAFETY: forwarded from this function's own safety doc.
            let state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
            // SAFETY: forwarded from this function's own safety doc.
            let fdo_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.fdo_flags;
            if n > 0
                || !((state as u32 & crate::state_defs::mode::INSERT) != 0
                    || fdo_flags & crate::option_vars::opt_fdo_flag::ALL != 0)
            {
                // SAFETY: forwarded from this function's own safety doc.
                let _ = unsafe { crate::fold::has_folding(&mut *wp, lnum, Some(&mut lnum), None) };
            }
        }
        lnum = lnum.max(1);
    } else {
        lnum -= n;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*wp).w_cursor.lnum = lnum };
}

/// Move the cursor down `n` lines in window `wp` (`cursor_down_inner`).
///
/// Takes care of closed folds; skips over concealed lines when
/// `skip_conceal` is set.
///
/// # Safety
/// Same as [`cursor_up_inner`], and `wp`'s own `w_buffer` must point
/// at a live `BufT`.
pub unsafe fn cursor_down_inner(wp: *mut crate::buffer_defs::WinT, n: i32, skip_conceal: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut lnum = unsafe { (*wp).w_cursor.lnum };
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { (*(*wp).w_buffer).b_ml.ml_line_count };
    let mut n = n;

    // SAFETY: forwarded from this function's own safety doc.
    let concealed = unsafe { crate::decoration::win_lines_concealed(&*wp) };

    if lnum + n >= line_count {
        lnum = line_count;
    } else if concealed {
        // Count each sequence of folded lines as one logical line.
        while n > 0 {
            n -= 1;
            let mut last = 0;
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { crate::fold::has_folding_win(&mut *wp, lnum, None, Some(&mut last), true, None) } {
                lnum = last + 1;
            } else {
                lnum += 1;
            }
            if lnum >= line_count {
                break;
            }
            // SAFETY: forwarded from this function's own safety doc.
            if skip_conceal && unsafe { crate::decoration::decor_conceal_line(&*wp, lnum - 1, true) } {
                n += 1;
            }
        }
        lnum = lnum.min(line_count);
    } else {
        lnum += n;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*wp).w_cursor.lnum = lnum };
}

/// Move the cursor to the start of the line, according to `flags`
/// (a combination of [`BL_WHITE`]/[`BL_SOL`]/[`BL_FIX`]) (`beginline`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` whose own `w_buffer` is also valid - same
/// requirement as [`crate::cursor::get_cursor_line_ptr`]/
/// `crate::move::adjust_skipcol`.
pub unsafe fn beginline(flags: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    let p_sol = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sol;

    if (flags & BL_SOL) != 0 && p_sol == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let curswant = unsafe { &*curwin }.w_curswant;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::cursor::coladvance(curwin, curswant) };
    } else {
        {
            // SAFETY: forwarded from this function's own safety doc.
            let wp = unsafe { &mut *curwin };
            wp.w_cursor.col = 0;
            wp.w_cursor.coladd = 0;
        }

        if (flags & (BL_WHITE | BL_SOL)) != 0 {
            // SAFETY: forwarded from this function's own safety doc.
            let line = unsafe { crate::cursor::get_cursor_line_ptr() };
            let mut i: usize = 0;
            while line
                .get(i)
                .is_some_and(|&b| crate::ascii_defs::ascii_iswhite(i32::from(b)))
                && !((flags & BL_FIX) != 0 && line.get(i + 1) == Some(&0))
            {
                i += 1;
            }
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &mut *curwin }.w_cursor.col += i as ColnrT;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *curwin }.w_set_curswant = true;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::r#move::adjust_skipcol() };
}

/// Move the cursor one character right, unless it would land on the
/// NUL past the end of the line (unless `'virtualedit'` contains
/// `"onemore"`) (`oneright`).
///
/// Returns [`OK`] if the cursor moved, [`FAIL`] at the window/line
/// edge.
///
/// # Safety
/// Same requirement as [`beginline`].
pub unsafe fn oneright() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;

    // SAFETY: forwarded from this function's own safety doc.
    let is_virtual = crate::state::virtual_active(unsafe { &*curwin });
    if is_virtual {
        // SAFETY: forwarded from this function's own safety doc.
        let prevpos = unsafe { &*curwin }.w_cursor;

        // Adjust for multi-wide char (excluding TAB)
        // SAFETY: forwarded from this function's own safety doc.
        let ptr = unsafe { crate::cursor::get_cursor_pos_ptr() };
        // SAFETY: forwarded from this function's own safety doc.
        let viscol = unsafe { crate::cursor::getviscol() };
        let extra = if ptr.first() != Some(&crate::ascii_defs::TAB)
            // SAFETY: forwarded from this function's own safety doc.
            && unsafe { crate::charset::vim_isprintc(crate::mbyte::utf_ptr2char(&ptr)) }
        {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::charset::ptr2cells(&ptr) }
        } else {
            1
        };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::cursor::coladvance(curwin, viscol + extra) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *curwin }.w_set_curswant = true;
        // Return OK if the cursor moved, FAIL otherwise (at window edge).
        // SAFETY: forwarded from this function's own safety doc.
        let now = unsafe { &*curwin }.w_cursor;
        return if prevpos.col != now.col || prevpos.coladd != now.coladd { OK } else { FAIL };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let ptr = unsafe { crate::cursor::get_cursor_pos_ptr() };
    if ptr.first() == Some(&0) {
        return FAIL; // already at the very end
    }

    // SAFETY: forwarded from this function's own safety doc.
    let l = unsafe { crate::mbyte::utfc_ptr2len(&ptr) };

    // move "l" bytes right, but don't end up on the NUL, unless
    // 'virtualedit' contains "onemore".
    // SAFETY: forwarded from this function's own safety doc.
    let ve_flags = crate::option::get_ve_flags(unsafe { &*curwin });
    if ptr.get(l as usize) == Some(&0)
        && (ve_flags & crate::option_vars::opt_ve_flag::ONEMORE) == 0
    {
        return FAIL;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let wp = unsafe { &mut *curwin };
    wp.w_cursor.col += l;
    wp.w_set_curswant = true;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::r#move::adjust_skipcol() };
    OK
}

/// Move the cursor one character left (`oneleft`).
///
/// Returns [`OK`] if the cursor moved, [`FAIL`] at the start of the
/// line.
///
/// # Safety
/// Same requirement as [`beginline`].
pub unsafe fn oneleft() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;

    // SAFETY: forwarded from this function's own safety doc.
    let is_virtual = crate::state::virtual_active(unsafe { &*curwin });
    if is_virtual {
        // SAFETY: forwarded from this function's own safety doc.
        let v = unsafe { crate::cursor::getviscol() };
        if v == 0 {
            return FAIL;
        }

        // We might get stuck on 'showbreak', skip over it.
        let mut width = 1;
        loop {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::cursor::coladvance(curwin, v - width) };
            // getviscol() is slow, skip it when 'showbreak' is empty,
            // 'breakindent' is not set and there are no multi-byte
            // characters
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { crate::cursor::getviscol() } < v {
                break;
            }
            width += 1;
        }

        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*curwin }.w_cursor.coladd == 1 {
            // Adjust for multi-wide char (not a TAB)
            // SAFETY: forwarded from this function's own safety doc.
            let ptr = unsafe { crate::cursor::get_cursor_pos_ptr() };
            if ptr.first() != Some(&crate::ascii_defs::TAB)
                // SAFETY: forwarded from this function's own safety doc.
                && unsafe { crate::charset::vim_isprintc(crate::mbyte::utf_ptr2char(&ptr)) }
                // SAFETY: forwarded from this function's own safety doc.
                && unsafe { crate::charset::ptr2cells(&ptr) } > 1
            {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &mut *curwin }.w_cursor.coladd = 0;
            }
        }

        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *curwin }.w_set_curswant = true;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::r#move::adjust_skipcol() };
        return OK;
    }

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { &*curwin }.w_cursor.col == 0 {
        return FAIL;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let wp = unsafe { &mut *curwin };
    wp.w_set_curswant = true;
    wp.w_cursor.col -= 1;

    // if the character on the left of the current cursor is a
    // multi-byte character, move to its first byte
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::mbyte::mb_adjust_cursor() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::r#move::adjust_skipcol() };
    OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{BufT, WinT};
    use crate::globals::global_state_test_lock;
    use crate::memline_defs::MemlineT;

    struct ReplaceStackGuard(Vec<u8>);

    impl ReplaceStackGuard {
        fn install(value: Vec<u8>) -> Self {
            Self(std::mem::replace(unsafe { REPLACE_STACK.get_mut() }, value))
        }
    }

    impl Drop for ReplaceStackGuard {
        fn drop(&mut self) {
            *unsafe { REPLACE_STACK.get_mut() } = std::mem::take(&mut self.0);
        }
    }

    #[test]
    fn replace_pop_if_nul_only_removes_a_nul_top_byte() {
        let _lock = global_state_test_lock();
        let _guard = ReplaceStackGuard::install(Vec::new());
        assert_eq!(unsafe { replace_pop_if_nul() }, -1);

        unsafe { REPLACE_STACK.get_mut() }.extend_from_slice(&[b'a', 0]);
        assert_eq!(unsafe { replace_pop_if_nul() }, 0);
        assert_eq!(unsafe { REPLACE_STACK.get_mut() }.as_slice(), b"a");

        assert_eq!(unsafe { replace_pop_if_nul() }, i32::from(b'a'));
        assert_eq!(unsafe { REPLACE_STACK.get_mut() }.as_slice(), b"a");
    }

    #[test]
    fn replace_push_inserts_below_the_configured_top_offset() {
        let _lock = global_state_test_lock();
        let _guard = ReplaceStackGuard::install(b"abcd".to_vec());
        let _offset = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.replace_offset, 0)
        };

        unsafe { replace_push(b"XY") };
        assert_eq!(unsafe { REPLACE_STACK.get_mut() }.as_slice(), b"abcdXY");

        unsafe { crate::globals::GLOBALS.get_mut() }.replace_offset = 2;
        unsafe { replace_push(b"!") };
        assert_eq!(unsafe { REPLACE_STACK.get_mut() }.as_slice(), b"abcd!XY");

        unsafe { crate::globals::GLOBALS.get_mut() }.replace_offset = 99;
        unsafe { replace_push(b"ignored") };
        assert_eq!(unsafe { REPLACE_STACK.get_mut() }.as_slice(), b"abcd!XY");
    }

    #[test]
    fn replace_push_nul_uses_the_same_offset_insertion_rule() {
        let _lock = global_state_test_lock();
        let _guard = ReplaceStackGuard::install(b"ab".to_vec());
        let _offset = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.replace_offset, 1)
        };

        unsafe { replace_push_nul() };
        assert_eq!(unsafe { REPLACE_STACK.get_mut() }.as_slice(), b"a\0b");
    }

    #[test]
    fn replace_join_removes_the_requested_separator_from_the_top() {
        let _lock = global_state_test_lock();
        let _guard = ReplaceStackGuard::install(b"a\0b\0c".to_vec());

        unsafe { replace_join(0) };
        assert_eq!(unsafe { REPLACE_STACK.get_mut() }.as_slice(), b"a\0bc");

        unsafe { REPLACE_STACK.get_mut() }.splice(.., b"a\0b\0c".iter().copied());
        unsafe { replace_join(1) };
        assert_eq!(unsafe { REPLACE_STACK.get_mut() }.as_slice(), b"ab\0c");

        let unchanged = unsafe { REPLACE_STACK.get_mut() }.clone();
        unsafe { replace_join(9) };
        assert_eq!(*unsafe { REPLACE_STACK.get_mut() }, unchanged);
    }

    #[test]
    fn truncate_spaces_removes_trailing_ascii_whitespace() {
        let _lock = global_state_test_lock();
        let _state = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.State,
                crate::state_defs::mode::INSERT as i32,
            )
        };
        let mut line = *b"abc \t  \0";
        unsafe { truncate_spaces(&mut line, 7) };
        assert_eq!(&line[..4], b"abc\0");
    }

    #[test]
    fn truncate_spaces_joins_replace_entries_for_each_removed_byte() {
        let _lock = global_state_test_lock();
        let _stack = ReplaceStackGuard::install(b"a\0b\0c".to_vec());
        let _state = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.State,
                crate::state_defs::mode::REPLACE as i32,
            )
        };
        let mut line = *b"x  \0";
        unsafe { truncate_spaces(&mut line, 3) };
        assert_eq!(&line[..2], b"x\0");
        assert_eq!(unsafe { REPLACE_STACK.get_mut() }.as_slice(), b"abc");
    }

    struct CurwinGuard {
        previous: *mut WinT,
    }

    impl CurwinGuard {
        fn set(win: &mut WinT) -> Self {
            // SAFETY: single-threaded test, lock held by the caller.
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let previous = g.curwin;
            g.curwin = win;
            CurwinGuard { previous }
        }
    }

    impl Drop for CurwinGuard {
        fn drop(&mut self) {
            // SAFETY: restoring the previous value on drop.
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.previous;
        }
    }

    /// Points `GLOBALS.curtab` at `tp` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime
    /// (matching `move.rs`'s/`plines.rs`'s own identically-named
    /// helper - needed defensively for any test reaching
    /// `coladvance`/`getviscol`, which may transitively touch
    /// `win_get_fill`/`diff_check_fill`'s own `curtab` read).
    struct CurtabGuard {
        previous: *mut crate::buffer_defs::TabpageT,
    }

    impl CurtabGuard {
        fn set(new_curtab: *mut crate::buffer_defs::TabpageT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
            unsafe { crate::globals::GLOBALS.get_mut() }.curtab = new_curtab;
            CurtabGuard { previous }
        }
    }

    impl Drop for CurtabGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curtab = self.previous;
        }
    }

    /// RAII guard restoring `GLOBALS.curbuf`/`curwin` on drop (even on
    /// panic) - self-locking, matching `eval/funcs.rs`'s own
    /// identically-named, identically-shaped precedent. Needed by
    /// `beginline`/`oneright`/`oneleft`'s own tests: unlike
    /// `get_nolist_virtcol` (which reads `win.w_buffer` directly),
    /// `get_cursor_line_ptr`/`get_cursor_pos_ptr`/`mb_adjust_cursor`
    /// all read `GLOBALS.curbuf` SEPARATELY from `GLOBALS.curwin`.
    struct CurbufCurwinGuard {
        prev_curbuf: *mut BufT,
        prev_curwin: *mut WinT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurbufCurwinGuard {
        fn set(buf: *mut BufT, win: *mut WinT) -> Self {
            let _lock = global_state_test_lock();
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard =
                CurbufCurwinGuard { prev_curbuf: globals.curbuf, prev_curwin: globals.curwin, _lock };
            globals.curbuf = buf;
            globals.curwin = win;
            guard
        }
    }

    impl Drop for CurbufCurwinGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.curbuf = self.prev_curbuf;
            globals.curwin = self.prev_curwin;
        }
    }

    fn buf_with_one_real_line() -> BufT {
        BufT {
            b_ml: MemlineT { ml_mfp: std::ptr::NonNull::dangling().as_ptr(), ml_line_count: 1, ..Default::default() },
            ..Default::default()
        }
    }

    /// Opens a REAL memline (unlike [`buf_with_one_real_line`]'s
    /// dangling placeholder) with `content` as its one line -
    /// needed by any test that actually reads line text (`beginline`/
    /// `oneright`/`oneleft` all do, via `get_cursor_line_ptr`/
    /// `get_cursor_pos_ptr`). Callers must close the returned buffer's
    /// memfile before the test ends (see existing `move.rs`
    /// precedent) to avoid leaking the backing `MemfileT`.
    fn buf_with_real_line(content: &[u8]) -> BufT {
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, OK);
        assert_eq!(unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, content) }, OK);
        buf
    }

    fn close_real_line_buf(buf: BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn get_nolist_virtcol_is_zero_when_w_buffer_is_null() {
        let _lock = global_state_test_lock();
        let mut win = WinT { w_buffer: std::ptr::null_mut(), ..Default::default() };
        let _guard = CurwinGuard::set(&mut win);
        assert_eq!(unsafe { get_nolist_virtcol() }, 0);
    }

    #[test]
    fn get_nolist_virtcol_is_zero_when_ml_mfp_is_null() {
        let _lock = global_state_test_lock();
        let mut buf = BufT { b_ml: MemlineT { ml_mfp: std::ptr::null_mut(), ..Default::default() }, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf, ..Default::default() };
        let _guard = CurwinGuard::set(&mut win);
        assert_eq!(unsafe { get_nolist_virtcol() }, 0);
    }

    #[test]
    fn get_nolist_virtcol_is_zero_when_cursor_past_the_last_line() {
        let _lock = global_state_test_lock();
        let mut buf = buf_with_one_real_line();
        let mut win = WinT {
            w_buffer: &mut buf,
            w_cursor: crate::pos_defs::PosT { lnum: 5, ..Default::default() },
            ..Default::default()
        };
        let _guard = CurwinGuard::set(&mut win);
        assert_eq!(unsafe { get_nolist_virtcol() }, 0);
    }

    #[test]
    fn get_nolist_virtcol_uses_w_virtcol_when_list_is_off() {
        let _lock = global_state_test_lock();
        let mut buf = buf_with_one_real_line();
        let cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let mut win = WinT {
            w_buffer: &mut buf,
            w_cursor: cursor,
            // Must match w_cursor exactly, or validate_virtcol's own
            // internal check_cursor_moved call clears VALID_VIRTCOL
            // (among other bits) right back out before ever reaching
            // its own "already valid" fast-path check below.
            w_valid_cursor: cursor,
            w_virtcol: 42,
            w_valid: i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL),
            ..Default::default()
        };
        win.w_onebuf_opt.wo_list = 0;
        let _guard = CurwinGuard::set(&mut win);
        assert_eq!(unsafe { get_nolist_virtcol() }, 42);
    }

    #[test]
    fn get_nolist_virtcol_uses_w_virtcol_when_cpo_contains_l_flag() {
        let _lock = global_state_test_lock();
        let mut buf = buf_with_one_real_line();
        let cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let mut win = WinT {
            w_buffer: &mut buf,
            w_cursor: cursor,
            // Same reasoning as the sibling test above.
            w_valid_cursor: cursor,
            w_virtcol: 7,
            w_valid: i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL),
            ..Default::default()
        };
        win.w_onebuf_opt.wo_list = 1;
        let _guard = CurwinGuard::set(&mut win);
        let previous_cpo = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo = Some(b"aBL".to_vec());
        assert_eq!(unsafe { get_nolist_virtcol() }, 7);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cpo = previous_cpo;
    }

    // --- ins_apply_autocmds ---

    #[test]
    fn ins_apply_autocmds_reports_no_autocommand_was_executed() {
        // Nothing in this crate can register a real autocommand yet,
        // so apply_autocmds reports false and leaves the buffer
        // untouched - which also means the changedtick is unchanged
        // and the undo-fixing branch is not taken.
        //
        // Boxed, not stack-allocated: these pointers go into GLOBALS.
        let _lock = global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut win = Box::new(WinT { w_buffer: buf_ptr, ..Default::default() });
        win.w_cursor.lnum = 1;
        let win_ptr = std::ptr::addr_of_mut!(*win);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_buf, prev_win) = (g.curbuf, g.curwin);
        g.curbuf = buf_ptr;
        g.curwin = win_ptr;

        let tick_before = crate::buffer::buf_get_changedtick(&buf);
        let r = unsafe { ins_apply_autocmds(crate::autocmd_defs::EventT::InsertEnter) };
        let tick_after = crate::buffer::buf_get_changedtick(&buf);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curbuf = prev_buf;
        g.curwin = prev_win;

        assert!(!r);
        assert_eq!(tick_before, tick_after, "no autocommand ran, so nothing changed");
    }

    #[test]
    fn ins_apply_autocmds_skips_the_undo_fix_when_leaving_insert_mode() {
        // InsertLeave is excluded from the undo fix-up regardless of
        // whether the tick moved, since no new line is about to start.
        let _lock = global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut win = Box::new(WinT { w_buffer: buf_ptr, ..Default::default() });
        win.w_cursor.lnum = 1;
        let win_ptr = std::ptr::addr_of_mut!(*win);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_buf, prev_win) = (g.curbuf, g.curwin);
        g.curbuf = buf_ptr;
        g.curwin = win_ptr;

        let r = unsafe { ins_apply_autocmds(crate::autocmd_defs::EventT::InsertLeave) };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curbuf = prev_buf;
        g.curwin = prev_win;

        assert!(!r);
    }

    #[test]
    fn check_spell_redraw_is_a_noop_when_no_line_is_pending() {
        // A zero pending line means nothing to redraw, so curwin is
        // never dereferenced on this path.
        let _lock = global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.spell_redraw_lnum;
        g.spell_redraw_lnum = 0;

        unsafe { check_spell_redraw() };
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.spell_redraw_lnum, 0);

        unsafe { crate::globals::GLOBALS.get_mut() }.spell_redraw_lnum = prev;
    }

    #[test]
    fn check_spell_redraw_clears_the_pending_line_before_redrawing() {
        // The pending line must be cleared first, since the redraw may
        // itself be skipped again.
        let _lock = global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        let win_ptr = std::ptr::addr_of_mut!(win);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_win, prev_lnum) = (g.curwin, g.spell_redraw_lnum);
        g.curwin = win_ptr;
        g.spell_redraw_lnum = 4;

        unsafe { check_spell_redraw() };
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.spell_redraw_lnum, 0);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = prev_win;
        g.spell_redraw_lnum = prev_lnum;
    }

    /// Like [`buf_with_real_line`], but boxed and with a real undo
    /// header installed.
    ///
    /// Boxing matters: `del_bytes` stores the buffer/window pointers
    /// in globals, so they must have stable heap addresses rather than
    /// stack ones. The undo header matters because `del_bytes` reaches
    /// `u_save_cursor`, which otherwise tries to CREATE undo state
    /// this fixture does not build. Mirrors `change.rs`'s own
    /// `del_fixture`, which is the proven-stable shape for this.
    fn del_buf(content: &[u8]) -> Box<BufT> {
        let mut buf = Box::new(BufT::default());
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, content) },
            OK
        );
        buf.b_u_curhead = Box::into_raw(Box::new(crate::undo_defs::UHeader::default()));
        buf
    }

    fn close_del_buf(mut buf: Box<BufT>) {
        unsafe {
            if !buf.b_u_curhead.is_null() {
                drop(Box::from_raw(buf.b_u_curhead));
                buf.b_u_curhead = std::ptr::null_mut();
            }
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    /// A boxed window positioned for the deletion fixtures above.
    fn del_win(buf_ptr: *mut BufT, col: crate::pos_defs::ColnrT) -> Box<WinT> {
        Box::new(WinT {
            w_buffer: buf_ptr,
            w_cursor: crate::pos_defs::PosT { lnum: 1, col, coladd: 0 },
            w_topline: 1,
            w_botline: 2,
            ..Default::default()
        })
    }

    /// Sets everything the change/redraw machinery reads, not just
    /// curbuf/curwin. `del_bytes` reaches `changed_bytes`, which walks
    /// the window list from `firstwin` - leaving that pointing at a
    /// window from an earlier test hangs the walk, so it must be set
    /// too. Mirrors `change.rs`'s own `ChangedGuard` for the same
    /// reason.
    struct DelGuard {
        prev_curwin: *mut WinT,
        prev_curbuf: *mut BufT,
        prev_firstwin: *mut WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_state: i32,
        prev_must_redraw: i32,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl DelGuard {
        fn set(win: *mut WinT, buf: *mut BufT, tab: *mut crate::buffer_defs::TabpageT) -> Self {
            let _lock = global_state_test_lock();
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = DelGuard {
                prev_curwin: g.curwin,
                prev_curbuf: g.curbuf,
                prev_firstwin: g.firstwin,
                prev_curtab: g.curtab,
                prev_state: g.State,
                prev_must_redraw: g.must_redraw,
                _lock,
            };
            g.curwin = win;
            g.curbuf = buf;
            g.firstwin = win;
            g.curtab = tab;
            g.State = crate::state_defs::mode::NORMAL as i32;
            g.must_redraw = 0;
            g.cmdmod.cmod_flags = 0;
            guard
        }
    }

    impl Drop for DelGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.curwin = self.prev_curwin;
            g.curbuf = self.prev_curbuf;
            g.firstwin = self.prev_firstwin;
            g.curtab = self.prev_curtab;
            g.State = self.prev_state;
            g.must_redraw = self.prev_must_redraw;
        }
    }

    #[test]
    fn del_char_after_col_deletes_the_character_under_the_cursor() {
        let mut tp = crate::buffer_defs::TabpageT::default();
        let mut buf = del_buf(b"abcd\0");
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut win = del_win(buf_ptr, 1);
        let win_ptr = std::ptr::addr_of_mut!(*win);
        let _guard = DelGuard::set(win_ptr, buf_ptr, &mut tp);

        // limit_col 0 is <= the cursor column, so the walk-forward loop
        // does nothing and the byte under the cursor is deleted.
        assert!(unsafe { del_char_after_col(0) });
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"acd\0".to_vec());

        drop(_guard);
        close_del_buf(buf);
    }

    #[test]
    fn del_char_after_col_with_a_negative_limit_deletes_unconditionally() {
        // A negative limit takes the plain del_char path, with no
        // column clamping at all.
        let mut tp = crate::buffer_defs::TabpageT::default();
        let mut buf = del_buf(b"abcd\0");
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut win = del_win(buf_ptr, 0);
        let win_ptr = std::ptr::addr_of_mut!(*win);
        let _guard = DelGuard::set(win_ptr, buf_ptr, &mut tp);

        assert!(unsafe { del_char_after_col(-1) });
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"bcd\0".to_vec());

        drop(_guard);
        close_del_buf(buf);
    }

    #[test]
    fn del_char_after_col_reports_nothing_deleted_at_end_of_line() {
        // Sitting on the terminating NUL, there is nothing to delete -
        // and this path returns before reaching del_bytes at all.
        let mut tp = crate::buffer_defs::TabpageT::default();
        let mut buf = del_buf(b"ab\0");
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut win = del_win(buf_ptr, 2);
        let win_ptr = std::ptr::addr_of_mut!(*win);
        let _guard = DelGuard::set(win_ptr, buf_ptr, &mut tp);

        assert!(!unsafe { del_char_after_col(0) });
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"ab\0".to_vec());

        drop(_guard);
        close_del_buf(buf);
    }

    // --- insert-state accessors ---

    #[test]
    fn ins_need_undo_get_reflects_the_insert_state_flag() {
        let _lock = global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.Ins.need_undo;

        g.Ins.need_undo = true;
        assert!(unsafe { ins_need_undo_get() });
        unsafe { crate::globals::GLOBALS.get_mut() }.Ins.need_undo = false;
        assert!(!unsafe { ins_need_undo_get() });

        unsafe { crate::globals::GLOBALS.get_mut() }.Ins.need_undo = prev;
    }

    #[test]
    fn can_cindent_round_trips_through_its_setter() {
        let _lock = global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.Ins.can_cindent;

        unsafe { set_can_cindent(true) };
        assert!(unsafe { get_can_cindent() });
        unsafe { set_can_cindent(false) };
        assert!(!unsafe { get_can_cindent() });

        unsafe { crate::globals::GLOBALS.get_mut() }.Ins.can_cindent = prev;
    }

    #[test]
    fn buf_prompt_text_falls_back_to_the_default_prompt() {
        let buf = BufT::default();
        assert_eq!(unsafe { buf_prompt_text(&buf) }, b"% ".to_vec());
    }

    #[test]
    fn buf_prompt_text_reports_a_buffers_own_prompt() {
        let buf = BufT { b_prompt_text: Some(b"> ".to_vec()), ..Default::default() };
        assert_eq!(unsafe { buf_prompt_text(&buf) }, b"> ".to_vec());
    }

    #[test]
    fn prompt_text_reads_the_current_buffer() {
        let _lock = global_state_test_lock();
        let mut buf = BufT { b_prompt_text: Some(b"$ ".to_vec()), ..Default::default() };
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.curbuf;
        g.curbuf = buf_ptr;

        assert_eq!(unsafe { prompt_text() }, b"$ ".to_vec());

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

    #[test]
    fn prompt_curpos_editable_compares_against_the_prompt_start_mark() {
        let _lock = global_state_test_lock();
        let mut buf = BufT::default();
        buf.b_prompt_start.mark = crate::pos_defs::PosT { lnum: 3, col: 4, coladd: 0 };
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        let win_ptr = std::ptr::addr_of_mut!(win);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_buf, prev_win) = (g.curbuf, g.curwin);
        g.curbuf = buf_ptr;
        g.curwin = win_ptr;

        // Before the prompt line entirely.
        unsafe { (*win_ptr).w_cursor = crate::pos_defs::PosT { lnum: 2, col: 99, coladd: 0 } };
        assert!(!unsafe { prompt_curpos_editable() });

        // On the prompt line, but inside the prompt text itself.
        unsafe { (*win_ptr).w_cursor = crate::pos_defs::PosT { lnum: 3, col: 3, coladd: 0 } };
        assert!(!unsafe { prompt_curpos_editable() });

        // Exactly at the first editable column.
        unsafe { (*win_ptr).w_cursor = crate::pos_defs::PosT { lnum: 3, col: 4, coladd: 0 } };
        assert!(unsafe { prompt_curpos_editable() });

        // Past the prompt line.
        unsafe { (*win_ptr).w_cursor = crate::pos_defs::PosT { lnum: 4, col: 0, coladd: 0 } };
        assert!(unsafe { prompt_curpos_editable() });

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curbuf = prev_buf;
        g.curwin = prev_win;
    }

    #[test]
    fn undisplay_dollar_is_a_noop_when_no_dollar_is_shown() {
        // A negative dollar_vcol means nothing is displayed, so the
        // redraw is skipped entirely - which also means curwin is
        // never dereferenced on this path.
        let _lock = global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.dollar_vcol;
        g.dollar_vcol = -1;

        unsafe { undisplay_dollar() };
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.dollar_vcol, -1);

        unsafe { crate::globals::GLOBALS.get_mut() }.dollar_vcol = prev;
    }

    #[test]
    fn undisplay_dollar_clears_the_column_and_redraws() {
        let _lock = global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor.lnum = 1;
        let win_ptr = std::ptr::addr_of_mut!(win);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_win, prev_dollar) = (g.curwin, g.dollar_vcol);
        g.curwin = win_ptr;
        g.dollar_vcol = 12;

        unsafe { undisplay_dollar() };

        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.dollar_vcol, -1);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = prev_win;
        g.dollar_vcol = prev_dollar;
    }

    // --- last_insert family ---

    struct LastInsertStateGuard {
        value: Option<Vec<u8>>,
        skip: usize,
    }

    impl LastInsertStateGuard {
        fn save() -> Self {
            Self {
                value: unsafe { LAST_INSERT.get_mut() }.clone(),
                skip: unsafe { *LAST_INSERT_SKIP.get_mut() },
            }
        }
    }

    impl Drop for LastInsertStateGuard {
        fn drop(&mut self) {
            unsafe {
                *LAST_INSERT.get_mut() = self.value.take();
                *LAST_INSERT_SKIP.get_mut() = self.skip;
            }
        }
    }

    #[test]
    fn free_last_insert_releases_the_stored_text() {
        let _lock = global_state_test_lock();
        let _g = LastInsertStateGuard::save();
        unsafe { set_last_insert(i32::from(b'a')) };

        unsafe { free_last_insert() };

        assert_eq!(unsafe { get_last_insert() }, None);
    }

    #[test]
    fn free_last_insert_does_not_reset_the_separate_skip_count() {
        let _lock = global_state_test_lock();
        let _g = LastInsertStateGuard::save();
        unsafe {
            *LAST_INSERT.get_mut() = Some(b"prefix".to_vec());
            *LAST_INSERT_SKIP.get_mut() = 4;
            free_last_insert();
        }

        assert_eq!(unsafe { *LAST_INSERT_SKIP.get_mut() }, 4);
    }

    #[test]
    fn last_insert_is_unset_until_something_records_one() {
        let _lock = global_state_test_lock();
        unsafe { reset_last_insert_for_test() };
        assert_eq!(unsafe { get_last_insert() }, None);
        assert_eq!(unsafe { get_last_insert_save() }, None);
    }

    #[test]
    fn set_last_insert_stores_a_plain_character_with_a_trailing_esc() {
        let _lock = global_state_test_lock();
        unsafe { reset_last_insert_for_test() };
        unsafe { set_last_insert(i32::from(b'a')) };

        assert_eq!(unsafe { get_last_insert() }, Some(vec![b'a', crate::ascii_defs::ESC]));
        // get_last_insert_save drops the terminating <Esc>.
        assert_eq!(unsafe { get_last_insert_save() }, Some(vec![b'a']));
        unsafe { reset_last_insert_for_test() };
    }

    #[test]
    fn set_last_insert_prefixes_a_control_character_with_ctrl_v() {
        // A control character could not be typed literally, so it is
        // stored the way it would have to be entered.
        let _lock = global_state_test_lock();
        unsafe { reset_last_insert_for_test() };
        unsafe { set_last_insert(9) }; // TAB

        assert_eq!(
            unsafe { get_last_insert_save() },
            Some(vec![crate::ascii_defs::CTRL_V, 9])
        );
        unsafe { reset_last_insert_for_test() };
    }

    #[test]
    fn set_last_insert_prefixes_del_with_ctrl_v_too() {
        // DEL is the one non-control byte that also takes the CTRL-V
        // prefix, matching the original's own `c == DEL` test.
        let _lock = global_state_test_lock();
        unsafe { reset_last_insert_for_test() };
        unsafe { set_last_insert(i32::from(crate::ascii_defs::DEL)) };

        assert_eq!(
            unsafe { get_last_insert_save() },
            Some(vec![crate::ascii_defs::CTRL_V, crate::ascii_defs::DEL])
        );
        unsafe { reset_last_insert_for_test() };
    }

    #[test]
    fn set_last_insert_stores_a_multibyte_character() {
        let _lock = global_state_test_lock();
        unsafe { reset_last_insert_for_test() };
        unsafe { set_last_insert(0xe9) }; // e-acute, UTF-8 C3 A9

        assert_eq!(unsafe { get_last_insert_save() }, Some(vec![0xc3, 0xa9]));
        unsafe { reset_last_insert_for_test() };
    }

    #[test]
    fn set_last_insert_escapes_a_utf8_byte_equal_to_k_special() {
        // `add_char2buf` escapes any stored byte that happens to equal
        // K_SPECIAL (0x80), so the text stays replayable through the
        // typeahead buffer. U+4E00 encodes as E4 B8 80, whose last
        // byte is exactly K_SPECIAL - so three bytes are stored in its
        // place, matching the original rather than the raw UTF-8.
        let _lock = global_state_test_lock();
        unsafe { reset_last_insert_for_test() };
        unsafe { set_last_insert(0x4e00) };

        assert_eq!(
            unsafe { get_last_insert_save() },
            Some(vec![
                0xe4,
                0xb8,
                crate::keycodes_defs::K_SPECIAL,
                crate::keycodes_defs::KS_SPECIAL,
                crate::keycodes_defs::KE_FILLER,
            ])
        );
        unsafe { reset_last_insert_for_test() };
    }

    #[test]
    fn set_last_insert_replaces_any_earlier_value() {
        let _lock = global_state_test_lock();
        unsafe { reset_last_insert_for_test() };
        unsafe { set_last_insert(i32::from(b'a')) };
        unsafe { set_last_insert(i32::from(b'b')) };

        assert_eq!(unsafe { get_last_insert_save() }, Some(vec![b'b']));
        unsafe { reset_last_insert_for_test() };
    }

    // --- cursor_up_inner / cursor_down_inner ---

    /// Neither function reads the buffer text, so a bare `BufT` with
    /// a hand-set `ml_line_count` is enough. `w_p_cole` stays at its
    /// default 0, so `win_lines_concealed` is false and the plain
    /// (non-fold, non-conceal) arithmetic branch is the one taken -
    /// which is also the only reachable branch today, since nothing
    /// in this crate can create a fold or a concealed line.
    fn cursor_move_win(line_count: crate::pos_defs::LinenrT, start: crate::pos_defs::LinenrT) -> (Box<BufT>, Box<WinT>) {
        let mut buf = Box::new(BufT::default());
        buf.b_ml.ml_line_count = line_count;
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut win = Box::new(WinT { w_buffer: buf_ptr, ..Default::default() });
        win.w_cursor.lnum = start;
        (buf, win)
    }

    #[test]
    fn cursor_up_inner_moves_up_by_n_lines() {
        let _lock = global_state_test_lock();
        let (_buf, mut win) = cursor_move_win(10, 7);
        let win_ptr = std::ptr::addr_of_mut!(*win);
        unsafe { cursor_up_inner(win_ptr, 3, false) };
        assert_eq!(win.w_cursor.lnum, 4);
    }

    #[test]
    fn cursor_up_inner_clamps_at_the_first_line() {
        // n >= lnum takes the "go to line 1" branch outright.
        let _lock = global_state_test_lock();
        let (_buf, mut win) = cursor_move_win(10, 3);
        let win_ptr = std::ptr::addr_of_mut!(*win);
        unsafe { cursor_up_inner(win_ptr, 99, false) };
        assert_eq!(win.w_cursor.lnum, 1);
    }

    #[test]
    fn cursor_up_inner_from_the_first_line_stays_there() {
        let _lock = global_state_test_lock();
        let (_buf, mut win) = cursor_move_win(10, 1);
        let win_ptr = std::ptr::addr_of_mut!(*win);
        unsafe { cursor_up_inner(win_ptr, 1, false) };
        assert_eq!(win.w_cursor.lnum, 1);
    }

    #[test]
    fn cursor_up_inner_with_zero_lines_does_not_move() {
        // n == 0 is strictly less than any valid lnum, so the plain
        // subtraction branch applies and subtracts nothing.
        let _lock = global_state_test_lock();
        let (_buf, mut win) = cursor_move_win(10, 5);
        let win_ptr = std::ptr::addr_of_mut!(*win);
        unsafe { cursor_up_inner(win_ptr, 0, false) };
        assert_eq!(win.w_cursor.lnum, 5);
    }

    #[test]
    fn cursor_down_inner_moves_down_by_n_lines() {
        let _lock = global_state_test_lock();
        let (_buf, mut win) = cursor_move_win(10, 2);
        let win_ptr = std::ptr::addr_of_mut!(*win);
        unsafe { cursor_down_inner(win_ptr, 3, false) };
        assert_eq!(win.w_cursor.lnum, 5);
    }

    #[test]
    fn cursor_down_inner_clamps_at_the_last_line() {
        let _lock = global_state_test_lock();
        let (_buf, mut win) = cursor_move_win(10, 4);
        let win_ptr = std::ptr::addr_of_mut!(*win);
        unsafe { cursor_down_inner(win_ptr, 99, false) };
        assert_eq!(win.w_cursor.lnum, 10);
    }

    #[test]
    fn cursor_down_inner_landing_exactly_on_the_last_line_takes_the_clamp_branch() {
        // lnum + n == line_count satisfies ">=", so the clamp branch
        // runs rather than the plain addition - same result here, but
        // it is the branch the original itself takes.
        let _lock = global_state_test_lock();
        let (_buf, mut win) = cursor_move_win(10, 6);
        let win_ptr = std::ptr::addr_of_mut!(*win);
        unsafe { cursor_down_inner(win_ptr, 4, false) };
        assert_eq!(win.w_cursor.lnum, 10);
    }

    #[test]
    fn cursor_down_inner_with_zero_lines_does_not_move() {
        let _lock = global_state_test_lock();
        let (_buf, mut win) = cursor_move_win(10, 5);
        let win_ptr = std::ptr::addr_of_mut!(*win);
        unsafe { cursor_down_inner(win_ptr, 0, false) };
        assert_eq!(win.w_cursor.lnum, 5);
    }

    // --- beginline ---

    #[test]
    fn beginline_no_flags_moves_to_column_zero() {
        let mut tp = crate::buffer_defs::TabpageT::default();
        let mut buf = buf_with_real_line(b"  hello\0");
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 5, coladd: 3 };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);
        let _curtab_guard = CurtabGuard::set(&mut tp);

        unsafe { beginline(0) };

        assert_eq!(win.w_cursor.col, 0);
        assert_eq!(win.w_cursor.coladd, 0);
        assert!(win.w_set_curswant);
        close_real_line_buf(buf);
    }

    #[test]
    fn beginline_white_skips_leading_whitespace() {
        let mut tp = crate::buffer_defs::TabpageT::default();
        let mut buf = buf_with_real_line(b"  hello\0");
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 6, coladd: 0 };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);
        let _curtab_guard = CurtabGuard::set(&mut tp);

        unsafe { beginline(BL_WHITE) };

        assert_eq!(win.w_cursor.col, 2); // lands on 'h', past the 2 leading spaces
        close_real_line_buf(buf);
    }

    #[test]
    fn beginline_white_fix_stops_before_trailing_nul_when_all_whitespace() {
        let mut tp = crate::buffer_defs::TabpageT::default();
        // A line that is ENTIRELY whitespace - without BL_FIX, the
        // scan would walk all the way to the trailing NUL; WITH
        // BL_FIX, it must stop one byte short (on the last space).
        let mut buf = buf_with_real_line(b"   \0");
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);
        let _curtab_guard = CurtabGuard::set(&mut tp);

        unsafe { beginline(BL_WHITE | BL_FIX) };

        assert_eq!(win.w_cursor.col, 2); // stops on the LAST space, not the NUL
        close_real_line_buf(buf);
    }

    #[test]
    fn beginline_white_without_fix_reaches_the_trailing_nul_when_all_whitespace() {
        let mut tp = crate::buffer_defs::TabpageT::default();
        let mut buf = buf_with_real_line(b"   \0");
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);
        let _curtab_guard = CurtabGuard::set(&mut tp);

        unsafe { beginline(BL_WHITE) };

        assert_eq!(win.w_cursor.col, 3); // walks all the way to the NUL
        close_real_line_buf(buf);
    }

    #[test]
    fn beginline_sol_with_default_p_sol_restores_curswant_column() {
        // 'startofline' defaults OFF (p_sol == 0) - matching the
        // original's own `(flags & BL_SOL) && !p_sol` condition, this
        // takes the coladvance(curwin, w_curswant) branch, NOT the
        // "jump to column 0" branch. Set p_sol explicitly (not just
        // relying on its default) to stay correct regardless of test
        // execution order.
        let mut tp = crate::buffer_defs::TabpageT::default();
        let mut buf = buf_with_real_line(b"hello\0");
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        win.w_curswant = 3;
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);
        let _curtab_guard = CurtabGuard::set(&mut tp);
        let previous_sol = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sol;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sol = 0;

        unsafe { beginline(BL_SOL) };

        assert_eq!(win.w_cursor.col, 3); // NOT 0 - restored to w_curswant instead
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sol = previous_sol;
        close_real_line_buf(buf);
    }

    // --- oneright ---

    #[test]
    fn oneright_fails_at_the_end_of_the_line() {
        let mut buf = buf_with_real_line(b"hi\0");
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 }; // on the NUL
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);

        assert_eq!(unsafe { oneright() }, FAIL);
        assert_eq!(win.w_cursor.col, 2);
        close_real_line_buf(buf);
    }

    #[test]
    fn oneright_moves_one_byte_for_an_ascii_char() {
        let mut buf = buf_with_real_line(b"hi\0");
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);

        assert_eq!(unsafe { oneright() }, OK);
        assert_eq!(win.w_cursor.col, 1);
        assert!(win.w_set_curswant);
        close_real_line_buf(buf);
    }

    #[test]
    fn oneright_fails_on_the_last_char_without_virtualedit_onemore() {
        // Moving right from the last real character would land
        // exactly ON the trailing NUL - FAIL unless 've' contains
        // "onemore" (defaults off).
        let mut buf = buf_with_real_line(b"hi\0");
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 }; // on 'i'
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);

        assert_eq!(unsafe { oneright() }, FAIL);
        assert_eq!(win.w_cursor.col, 1);
        close_real_line_buf(buf);
    }

    #[test]
    fn oneright_moves_a_multibyte_char_by_its_full_byte_length() {
        // "é" (U+00E9) is 2 bytes in UTF-8, followed by another
        // ASCII byte so landing after it doesn't hit the NUL.
        let mut buf = buf_with_real_line("é!\0".as_bytes());
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);

        assert_eq!(unsafe { oneright() }, OK);
        assert_eq!(win.w_cursor.col, 2); // moved past both UTF-8 bytes of 'é'
        close_real_line_buf(buf);
    }

    // --- oneleft ---

    #[test]
    fn oneleft_fails_at_column_zero() {
        let mut buf = buf_with_real_line(b"hi\0");
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);

        assert_eq!(unsafe { oneleft() }, FAIL);
        assert_eq!(win.w_cursor.col, 0);
        close_real_line_buf(buf);
    }

    #[test]
    fn oneleft_moves_one_byte_for_an_ascii_char() {
        let mut buf = buf_with_real_line(b"hi\0");
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);

        assert_eq!(unsafe { oneleft() }, OK);
        assert_eq!(win.w_cursor.col, 1);
        assert!(win.w_set_curswant);
        close_real_line_buf(buf);
    }

    #[test]
    fn oneleft_lands_on_the_first_byte_of_a_multibyte_char() {
        // Starting right after "é" (2 UTF-8 bytes) and moving left
        // must land on 'é''s OWN first byte (col 0), not its second
        // (continuation) byte - mb_adjust_cursor's own job.
        let mut buf = buf_with_real_line("é!\0".as_bytes());
        let buf_ptr = &mut buf as *mut BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 }; // on '!'
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurbufCurwinGuard::set(buf_ptr, win_ptr);

        assert_eq!(unsafe { oneleft() }, OK);
        assert_eq!(win.w_cursor.col, 0); // 'é''s own first byte, not byte 1
        close_real_line_buf(buf);
    }
}
