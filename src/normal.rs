//! Translated from `src/nvim/normal.c` (tractable core only).
//!
//! `normal.c` (~6600 lines) is the Normal-mode command-dispatch engine
//! (the giant `normal_cmd`/`nv_*` handler table) - almost none of it
//! is tractable, since it needs real buffer modification, the redraw
//! pipeline, the regex engine, and the whole rest of the editing
//! subsystem, none of which are translated yet.
//!
//! Translated: [`is_ident`] - a small, pure, self-contained C-style-
//! comment/string-literal scanner. Translated ahead of its own real
//! caller (`find_decl`, the `"gd"`/`"gD"` variable-declaration search,
//! not translated - needs `find_ident_under_cursor`/`searchit`, the
//! real regex engine), matching this crate's established "small,
//! self-contained, no design freedom to get wrong" precedent.
//!
//! Deferred: everything else in the file.

/// Step `pp` back one character, undoing an exclusive-selection
/// adjustment (`unadjust_for_sel_inner`).
///
/// @return whether the position moved to the PREVIOUS line, which the
///         caller needs to know because the column it lands on is the
///         line's length rather than a real character position.
///
/// Three cases, in priority order: a virtual-space offset is given
/// back first, then a real column, and only then does the position
/// wrap to the previous line. A position already at the very start of
/// the buffer stays put.
///
/// After stepping a real column back, the position is re-aligned to a
/// character boundary, and under `'virtualedit'` its `coladd` is
/// recomputed from the character's own screen width - so a step onto a
/// TAB lands at the right visual place.
///
/// # Safety
/// Reads `GLOBALS.curwin`/`GLOBALS.curbuf`, which must be valid and
/// non-null. Forwarded from [`crate::mark::mark_mb_adjustpos`]/
/// [`crate::plines::getvcol`]/[`crate::memline::ml_get_len`]'s own
/// safety docs.
pub unsafe fn unadjust_for_sel_inner(pp: &mut crate::pos_defs::PosT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select_exclu_adj = false;

    if pp.coladd > 0 {
        pp.coladd -= 1;
    } else if pp.col > 0 {
        pp.col -= 1;
        // SAFETY: forwarded from this function's own safety doc.
        let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::mark::mark_mb_adjustpos(&mut *curbuf, pp) };

        // SAFETY: forwarded from this function's own safety doc.
        let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        // SAFETY: forwarded from this function's own safety doc.
        if crate::state::virtual_active(unsafe { &*curwin }) {
            let (mut cs, mut ce) = (0, 0);
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                crate::plines::getvcol(curwin, pp, Some(&mut cs), None, Some(&mut ce), 0);
            }
            pp.coladd = ce - cs;
        }
    } else if pp.lnum > 1 {
        pp.lnum -= 1;
        // SAFETY: forwarded from this function's own safety doc.
        pp.col = unsafe { crate::memline::ml_get_len(pp.lnum) };
        return true;
    }

    false
}

/// Decide whether Select mode should start instead of Visual mode
/// (`may_start_select`).
///
/// Select mode is entered only when the command character appears in
/// `'selectmode'` AND the command was genuinely user-initiated: either
/// it is `'o'`, or nothing is being replayed from the stuff buffer and
/// the typeahead really was typed. A mapping or a replayed register
/// therefore gives plain Visual mode, matching the original.
///
/// # Safety
/// Mutates `GLOBALS.Visual`; reads `OPTION_VARS.p_slm`.
pub unsafe fn may_start_select(c: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let in_slm = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_slm
        .as_deref()
        .is_some_and(|slm| u8::try_from(c).is_ok_and(|b| slm.contains(&b)));

    let user_initiated = c == i32::from(b'o')
        || (crate::input::stuff_empty() && crate::input::typebuf_typed());

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select = user_initiated && in_slm;
}

/// `0`/`^`/`_` and friends: move to the start of the line
/// (`nv_beginline`).
///
/// Folds are only opened when the move was actually TYPED and no
/// operator is pending - an operator's own motion should not disturb
/// the fold state, and a mapped or replayed key is not a user
/// navigation.
///
/// # Safety
/// Reads `GLOBALS`/`OPTION_VARS` and forwards
/// [`crate::insert::beginline`]/[`crate::fold::fold_open_cursor`]'s
/// own safety docs.
pub unsafe fn nv_beginline(cap: &mut crate::normal_defs::CmdargT) {
    // SAFETY: cap.oap is a raw pointer in this crate's CmdargT.
    unsafe {
        (*cap.oap).motion_type = crate::normal_defs::MotionType::CharWise;
        (*cap.oap).inclusive = false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::insert::beginline(cap.arg) };

    // SAFETY: forwarded from this function's own safety doc.
    let fdo_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.fdo_flags;
    // SAFETY: forwarded from this function's own safety doc.
    let key_typed = unsafe { crate::globals::GLOBALS.get_mut() }.KeyTyped;
    // SAFETY: cap.oap is a raw pointer, see above.
    let op_type = unsafe { (*cap.oap).op_type };

    if fdo_flags & crate::option_vars::opt_fdo_flag::HOR != 0
        && key_typed
        && op_type == crate::ops_defs::OpType::Nop as i32
    {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::fold::fold_open_cursor() };
    }

    // Don't move the cursor past eol (only necessary in a
    // one-character line).
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.ins_at_eol = false;
}

/// The virtual top line of `wp` (`get_vtopline`).
///
/// Counts the screen lines the buffer occupies down to `w_topline`,
/// minus the filler lines currently shown above it - so the result is
/// a scroll position comparable across windows with different fold and
/// diff-filler states, which is what `'scrollbind'` needs.
///
/// # Safety
/// Forwarded from [`crate::plines::plines_m_win_fill`]'s own safety
/// doc.
#[must_use]
pub unsafe fn get_vtopline(wp: &crate::buffer_defs::WinT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::plines::plines_m_win_fill(wp, 1, wp.w_topline) - wp.w_topfill }
}

/// Command character that doesn't do anything, and does NOT start
/// `edit()` afterwards (`nv_ignore`).
pub fn nv_ignore(cap: &mut crate::normal_defs::CmdargT) {
    // Don't call edit() now.
    cap.retval |= crate::normal_defs::ca_flags::COMMAND_BUSY;
}

/// Command character that doesn't do anything, but unlike
/// [`nv_ignore`] DOES start `edit()` afterwards (`nv_nop`).
///
/// Used for `:startinsert` executed while starting up. The empty body
/// is the whole point: leaving `retval` untouched is what allows
/// `edit()` to run.
pub fn nv_nop(_cap: &mut crate::normal_defs::CmdargT) {}

/// Include the character under the cursor for `'selection'` ==
/// `"exclusive"` (`adjust_for_sel`).
///
/// With an exclusive selection the character the cursor sits on is not
/// part of the Visual area, so an inclusive operator has to step the
/// cursor forward and become exclusive itself. The `select_exclu_adj`
/// flag records that this happened, so it can be undone afterwards.
///
/// # Safety
/// Reads `GLOBALS.Visual`/`GLOBALS.curwin`, which must be valid.
/// Forwarded from [`crate::cursor::gchar_cursor`]/
/// [`crate::cursor::inc_cursor`]'s own safety docs.
pub unsafe fn adjust_for_sel(cap: &mut crate::normal_defs::CmdargT) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    // SAFETY: cap.oap is a raw pointer in this crate's CmdargT,
    // matching the original's own `oparg_T *oap` member.
    let inclusive = unsafe { (*cap.oap).inclusive };
    if !g.Visual.active || !inclusive {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let exclusive = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_sel
        .as_deref()
        .is_some_and(|s| s.first() == Some(&b'e'));
    if !exclusive {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::cursor::gchar_cursor() } == i32::from(crate::ascii_defs::NUL) {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let cursor = unsafe { (*g.curwin).w_cursor };
    if !crate::mark_defs::lt(g.Visual.start, cursor) {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::cursor::inc_cursor() };
    // SAFETY: cap.oap is a raw pointer, see above.
    unsafe { (*cap.oap).inclusive = false };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select_exclu_adj = true;
}

/// Clear a pending operator (`clearop`).
///
/// Resets both the operator argument's own fields AND the global
/// `motion_force`, which is separate state the caller would otherwise
/// have to remember to clear itself.
///
/// # Safety
/// Must not run concurrently with any other access to
/// `crate::globals::GLOBALS`.
pub unsafe fn clearop(oap: &mut crate::normal_defs::OpargT) {
    oap.op_type = crate::ops_defs::OpType::Nop as i32;
    oap.regname = 0;
    oap.motion_force = i32::from(crate::ascii_defs::NUL);
    oap.use_reg_one = false;
    oap.restore_cursor = false;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.motion_force = i32::from(crate::ascii_defs::NUL);
}

/// Swap the ends of the Visual selection (`v_swap_corners`).
///
/// For `O` in blockwise Visual mode this swaps the LEFT/RIGHT corners
/// rather than the start/end, so the cursor moves to the opposite
/// side of the block on the same line. If that swap would leave the
/// cursor exactly where it started (the block is one column wide, or
/// the cursor was already at that corner), the ends are swapped
/// instead so `O` still does something.
///
/// Every other case is the plain `o` behaviour: exchange the cursor
/// with `Visual.start`.
///
/// With `'selection'` "exclusive" the right edge sits one column
/// further right, which the two `p_sel` checks account for.
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT`. Forwarded from [`crate::plines::getvcols`] and
/// [`crate::cursor::coladvance`]'s own safety docs.
pub unsafe fn v_swap_corners(cmdchar: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let visual_mode = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.mode;

    if cmdchar != i32::from(b'O') || visual_mode != i32::from(crate::ascii_defs::CTRL_V) {
        // SAFETY: forwarded from this function's own safety doc.
        let old_cursor = unsafe { (*curwin).w_cursor };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        // SAFETY: as above.
        unsafe { (*curwin).w_cursor = g.Visual.start };
        g.Visual.start = old_cursor;
        // SAFETY: as above.
        unsafe { (*curwin).w_set_curswant = true };
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let old_cursor = unsafe { (*curwin).w_cursor };
    let (mut left, mut right) = (0, 0);
    let mut pos1 = old_cursor;
    // SAFETY: as above.
    let mut pos2 = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::plines::getvcols(curwin, &mut pos1, &mut pos2, &mut left, &mut right, 0) };

    // SAFETY: momentary read of a plain option global.
    let sel_exclusive = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_sel
        .as_deref()
        .and_then(<[u8]>::first)
        .copied()
        == Some(b'e');

    // SAFETY: forwarded from this function's own safety doc.
    let visual_start_lnum = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start.lnum;
    // SAFETY: as above.
    unsafe { (*curwin).w_cursor.lnum = visual_start_lnum };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::cursor::coladvance(curwin, left) };
    // SAFETY: as above.
    unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start = unsafe { (*curwin).w_cursor };

    // SAFETY: as above.
    unsafe {
        (*curwin).w_cursor.lnum = old_cursor.lnum;
        (*curwin).w_curswant = right;
    };
    // 'selection' "exclusive" and cursor at the right-bottom corner:
    // move it right one column.
    if old_cursor.lnum >= visual_start_lnum && sel_exclusive {
        // SAFETY: as above.
        unsafe { (*curwin).w_curswant += 1 };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::cursor::coladvance(curwin, (*curwin).w_curswant) };

    // SAFETY: forwarded from this function's own safety doc.
    let cursor = unsafe { (*curwin).w_cursor };
    // SAFETY: as above.
    let virt = unsafe { crate::state::virtual_active(&*curwin) };
    if cursor.col == old_cursor.col && (!virt || cursor.coladd == old_cursor.coladd) {
        // The swap changed nothing, so swap the ends instead.
        // SAFETY: as above.
        let visual_start_lnum = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start.lnum;
        // SAFETY: as above.
        unsafe { (*curwin).w_cursor.lnum = visual_start_lnum };
        if old_cursor.lnum <= visual_start_lnum && sel_exclusive {
            right += 1;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::cursor::coladvance(curwin, right) };
        // SAFETY: as above.
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start = unsafe { (*curwin).w_cursor };

        // SAFETY: as above.
        unsafe { (*curwin).w_cursor.lnum = old_cursor.lnum };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::cursor::coladvance(curwin, left) };
        // SAFETY: as above.
        unsafe { (*curwin).w_curswant = left };
    }
}

/// Rewrite a shifted cursor key in `cap` to its unshifted form
/// (`unshift_special`).
///
/// The shift is not simply discarded: `simplify_key` folds it back
/// into the global `mod_mask`, so a mapping can still see it.
///
/// # Safety
/// Must not run concurrently with any other access to
/// `crate::globals::GLOBALS` (touches `mod_mask`).
pub unsafe fn unshift_special(cap: &mut crate::normal_defs::CmdargT) {
    use crate::keycodes_defs as kc;
    cap.cmdchar = match cap.cmdchar {
        c if c == kc::K_S_RIGHT => kc::K_RIGHT,
        c if c == kc::K_S_LEFT => kc::K_LEFT,
        c if c == kc::K_S_UP => kc::K_UP,
        c if c == kc::K_S_DOWN => kc::K_DOWN,
        c if c == kc::K_S_HOME => kc::K_HOME,
        c if c == kc::K_S_END => kc::K_END,
        other => other,
    };
    // SAFETY: forwarded from this function's own safety doc.
    let mod_mask = &mut unsafe { crate::globals::GLOBALS.get_mut() }.mod_mask;
    cap.cmdchar = crate::keycodes::simplify_key(cap.cmdchar, mod_mask);
}

/// Whether the current buffer's `'comments'` option defines a C-style
/// (`//` or `/*`) comment leader (`buf_has_cstyle_comments`).
///
/// Each comma-separated part of `'comments'` is `flags:leader`; this
/// looks for a leader starting `/` followed by `/` or `*`.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn buf_has_cstyle_comments() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let com = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }
        .b_p_com
        .clone()
        .unwrap_or_default();

    let mut list = 0usize;
    while list < com.len() && com[list] != crate::ascii_defs::NUL {
        let (part_buf, next) = crate::option::copy_option_part(
            &com,
            list,
            crate::option_vars::COM_MAX_LEN as usize,
            b",",
        );
        list = next;
        // Flags and comment leader are separated by a colon.
        if let Some(colon) = crate::strings::vim_strchr(&part_buf, i32::from(b':'))
            && part_buf.get(colon + 1) == Some(&b'/')
            && matches!(part_buf.get(colon + 2), Some(&b'/') | Some(&b'*'))
        {
            return true;
        }
    }
    false
}

/// Returns `true` if `line[offset]` is NOT inside a C-style comment or
/// string, `false` otherwise (`is_ident`).
///
/// Assumes `line` is a well-formed line (this crate's own convention:
/// includes its own trailing NUL) - running out of a malformed,
/// non-NUL-terminated slice before reaching `offset` is treated the
/// same way as hitting the terminator, matching `mbyte.c`/`indent.c`'s
/// established "ran out of slice = terminator" precedent.
#[must_use]
pub fn is_ident(line: &[u8], offset: i32) -> bool {
    let mut incomment = false;
    let mut instring: u8 = 0;
    let mut prev: u8 = 0;

    let offset = offset.max(0) as usize;
    let mut i = 0usize;
    while i < offset {
        let Some(&c) = line.get(i) else { break };
        if c == 0 {
            break;
        }

        if instring != 0 {
            if prev != b'\\' && c == instring {
                instring = 0;
            }
        } else if (c == b'"' || c == b'\'') && !incomment {
            instring = c;
        } else if incomment {
            if prev == b'*' && c == b'/' {
                incomment = false;
            }
        } else if prev == b'/' && c == b'*' {
            incomment = true;
        } else if prev == b'/' && c == b'/' {
            return false;
        }

        prev = c;
        i += 1;
    }

    !incomment && instring == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unadjust_for_sel_inner_gives_back_virtual_space_first() {
        // A coladd offset is surrendered before any real column.
        let _lock = crate::globals::global_state_test_lock();
        let prev_adj =
            unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select_exclu_adj;
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select_exclu_adj = true;

        let mut pp = crate::pos_defs::PosT { lnum: 5, col: 3, coladd: 2 };
        assert!(!unsafe { unadjust_for_sel_inner(&mut pp) });
        assert_eq!(pp.coladd, 1);
        assert_eq!(pp.col, 3, "the real column must be untouched");
        assert_eq!(pp.lnum, 5);
        // The flag is always cleared, whichever branch ran.
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select_exclu_adj);

        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select_exclu_adj = prev_adj;
    }

    #[test]
    fn unadjust_for_sel_inner_at_the_buffer_start_stays_put() {
        // Nothing to step back to, so the position is unchanged and no
        // line wrap is reported.
        let _lock = crate::globals::global_state_test_lock();
        let prev_adj =
            unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select_exclu_adj;
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select_exclu_adj = true;

        let mut pp = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        assert!(!unsafe { unadjust_for_sel_inner(&mut pp) });
        assert_eq!(pp.lnum, 1);
        assert_eq!(pp.col, 0);
        assert_eq!(pp.coladd, 0);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select_exclu_adj);

        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select_exclu_adj = prev_adj;
    }

    #[test]
    fn may_start_select_needs_the_char_listed_in_selectmode() {
        // Select mode only starts when the command character actually
        // appears in 'selectmode'.
        let _lock = crate::globals::global_state_test_lock();
        let prev_slm = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_slm.clone();
        let prev_sel = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select;

        // 'o' is always treated as user-initiated, so this isolates
        // the 'selectmode' membership test.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_slm = Some(b"o".to_vec());
        unsafe { may_start_select(i32::from(b'o')) };
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select);

        // Not listed: plain Visual mode.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_slm = Some(b"k".to_vec());
        unsafe { may_start_select(i32::from(b'o')) };
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select);

        // Empty 'selectmode' never starts Select mode.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_slm = Some(Vec::new());
        unsafe { may_start_select(i32::from(b'o')) };
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select);

        // Unset likewise.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_slm = None;
        unsafe { may_start_select(i32::from(b'o')) };
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_slm = prev_slm;
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select = prev_sel;
    }

    #[test]
    fn may_start_select_handles_a_non_byte_command_char() {
        // Special keys are negative or above 0xff; neither can appear
        // in 'selectmode', and neither must panic on conversion.
        let _lock = crate::globals::global_state_test_lock();
        let prev_slm = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_slm.clone();
        let prev_sel = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select;

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_slm = Some(b"okm".to_vec());
        for c in [-1_i32, 0x1000] {
            unsafe { may_start_select(c) };
            assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select);
        }

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_slm = prev_slm;
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.select = prev_sel;
    }

    #[test]
    fn nv_beginline_moves_to_column_zero_and_sets_the_motion_type() {
        // beginline(0) takes the plain branch: cursor to column 0.
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_win, prev_typed, prev_eol) = (g.curwin, g.KeyTyped, g.ins_at_eol);
        let prev_fdo = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.fdo_flags;

        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT { w_buffer: &mut buf, ..Default::default() };
        win.w_cursor.lnum = 1;
        win.w_cursor.col = 7;
        win.w_cursor.coladd = 2;

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = &mut win;
        g.ins_at_eol = true;
        // No fold-open: 'foldopen' has no "hor" flag here, so the
        // fold_open_cursor path is not taken.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.fdo_flags = 0;

        let mut oap = crate::normal_defs::OpargT {
            motion_type: crate::normal_defs::MotionType::LineWise,
            inclusive: true,
            ..Default::default()
        };
        let mut cap =
            crate::normal_defs::CmdargT { oap: &mut oap, arg: 0, ..Default::default() };

        unsafe { nv_beginline(&mut cap) };

        assert_eq!(oap.motion_type, crate::normal_defs::MotionType::CharWise);
        assert!(!oap.inclusive);
        assert_eq!(win.w_cursor.col, 0);
        assert_eq!(win.w_cursor.coladd, 0);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.ins_at_eol);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = prev_win;
        g.KeyTyped = prev_typed;
        g.ins_at_eol = prev_eol;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.fdo_flags = prev_fdo;
    }

    #[test]
    fn nv_ignore_sets_command_busy_and_nv_nop_does_not() {
        // The difference between the two IS the flag: nv_ignore
        // suppresses the following edit(), nv_nop deliberately allows
        // it. An empty nv_nop body is correct, not an oversight.
        let mut oap = crate::normal_defs::OpargT::default();
        let mut cap = crate::normal_defs::CmdargT { oap: &mut oap, ..Default::default() };

        nv_ignore(&mut cap);
        assert_ne!(cap.retval & crate::normal_defs::ca_flags::COMMAND_BUSY, 0);

        cap.retval = 0;
        nv_nop(&mut cap);
        assert_eq!(cap.retval, 0, "nv_nop must leave retval alone");
    }

    #[test]
    fn ca_flag_values_match_the_original() {
        assert_eq!(crate::normal_defs::ca_flags::COMMAND_BUSY, 1);
        assert_eq!(crate::normal_defs::ca_flags::NO_ADJ_OP_END, 2);
    }

    #[test]
    fn get_vtopline_subtracts_the_filler_lines() {
        // The filler lines shown ABOVE w_topline are not part of the
        // buffer's own screen-line count, so they are subtracted.
        //
        // plines_m_win_fill reaches diff.rs, which dereferences
        // GLOBALS.curtab, so a real tabpage must be installed first.
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let prev_tab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = &mut tp;

        let mut buf = crate::buffer_defs::BufT::default();
        buf.b_ml.ml_line_count = 100;
        let win = crate::buffer_defs::WinT {
            w_buffer: &mut buf,
            w_topline: 1,
            w_topfill: 0,
            ..Default::default()
        };
        let without_fill = unsafe { get_vtopline(&win) };

        let win_filled = crate::buffer_defs::WinT {
            w_buffer: &mut buf,
            w_topline: 1,
            w_topfill: 3,
            ..Default::default()
        };
        assert_eq!(unsafe { get_vtopline(&win_filled) }, without_fill - 3);

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_tab;
    }

    #[test]
    fn adjust_for_sel_does_nothing_outside_visual_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_active = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active;
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active = false;

        let mut oap = crate::normal_defs::OpargT { inclusive: true, ..Default::default() };
        let mut cap = crate::normal_defs::CmdargT { oap: &mut oap, ..Default::default() };
        unsafe { adjust_for_sel(&mut cap) };

        assert!(oap.inclusive, "an inclusive operator must be left alone");

        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active = prev_active;
    }

    #[test]
    fn adjust_for_sel_does_nothing_for_an_exclusive_operator() {
        // Only an INCLUSIVE operator needs adjusting; an already
        // exclusive one is left untouched whatever 'selection' says.
        let _lock = crate::globals::global_state_test_lock();
        let prev_active = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active;
        let prev_sel = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel.clone();
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active = true;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel =
            Some(b"exclusive".to_vec());

        let mut oap = crate::normal_defs::OpargT { inclusive: false, ..Default::default() };
        let mut cap = crate::normal_defs::CmdargT { oap: &mut oap, ..Default::default() };
        unsafe { adjust_for_sel(&mut cap) };

        assert!(!oap.inclusive);

        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active = prev_active;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = prev_sel;
    }

    #[test]
    fn adjust_for_sel_does_nothing_for_inclusive_selection() {
        // 'selection' must start with 'e' (exclusive); the default
        // "inclusive" leaves everything alone.
        let _lock = crate::globals::global_state_test_lock();
        let prev_active = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active;
        let prev_sel = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel.clone();
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active = true;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel =
            Some(b"inclusive".to_vec());

        let mut oap = crate::normal_defs::OpargT { inclusive: true, ..Default::default() };
        let mut cap = crate::normal_defs::CmdargT { oap: &mut oap, ..Default::default() };
        unsafe { adjust_for_sel(&mut cap) };

        assert!(oap.inclusive, "inclusive 'selection' needs no adjustment");

        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active = prev_active;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = prev_sel;
    }

    // --- v_swap_corners ---

    /// Installs `win` as curwin and seeds `Visual.start`, restoring
    /// both on drop.
    struct VisualGuard {
        prev_win: *mut crate::buffer_defs::WinT,
        prev_visual: crate::normal_defs::VisualState,
    }

    impl VisualGuard {
        fn set(win: *mut crate::buffer_defs::WinT, start: crate::pos_defs::PosT, mode: i32) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = VisualGuard { prev_win: g.curwin, prev_visual: g.Visual };
            g.curwin = win;
            g.Visual.start = start;
            g.Visual.mode = mode;
            guard
        }
    }

    impl Drop for VisualGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.curwin = self.prev_win;
            g.Visual = self.prev_visual;
        }
    }

    #[test]
    fn v_swap_corners_plain_o_exchanges_the_cursor_and_visual_start() {
        // Cross-verified against real nvim: over a 3-line blockwise
        // selection started at line 1, `o` moves the cursor to line 1
        // (the opposite END of the selection).
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT {
            w_cursor: crate::pos_defs::PosT { lnum: 3, col: 3, coladd: 0 },
            ..Default::default()
        };
        let win_ptr: *mut crate::buffer_defs::WinT = &mut win;
        let start = crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 };
        let _guard = VisualGuard::set(win_ptr, start, i32::from(b'v'));

        unsafe { v_swap_corners(i32::from(b'o')) };

        assert_eq!(unsafe { (*win_ptr).w_cursor.lnum }, 1);
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 1);
        let vs = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start;
        assert_eq!((vs.lnum, vs.col), (3, 3), "the old cursor becomes the start");
        assert!(unsafe { (*win_ptr).w_set_curswant });
    }

    #[test]
    fn v_swap_corners_uses_the_plain_swap_for_o_outside_blockwise() {
        // Capital O only takes the corner-swapping path in blockwise
        // mode; in charwise Visual it behaves like `o`.
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT {
            w_cursor: crate::pos_defs::PosT { lnum: 5, col: 2, coladd: 0 },
            ..Default::default()
        };
        let win_ptr: *mut crate::buffer_defs::WinT = &mut win;
        let start = crate::pos_defs::PosT { lnum: 2, col: 7, coladd: 0 };
        let _guard = VisualGuard::set(win_ptr, start, i32::from(b'v'));

        unsafe { v_swap_corners(i32::from(b'O')) };

        assert_eq!(unsafe { (*win_ptr).w_cursor.lnum }, 2);
        assert_eq!(unsafe { (*win_ptr).w_cursor.col }, 7);
        let vs = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start;
        assert_eq!((vs.lnum, vs.col), (5, 2));
    }

    // --- unshift_special / buf_has_cstyle_comments ---

    #[test]
    fn unshift_special_maps_each_shifted_cursor_key_to_its_plain_form() {
        let _lock = crate::globals::global_state_test_lock();
        use crate::keycodes_defs as kc;
        for (shifted, plain) in [
            (kc::K_S_RIGHT, kc::K_RIGHT),
            (kc::K_S_LEFT, kc::K_LEFT),
            (kc::K_S_UP, kc::K_UP),
            (kc::K_S_DOWN, kc::K_DOWN),
            (kc::K_S_HOME, kc::K_HOME),
            (kc::K_S_END, kc::K_END),
        ] {
            let mut cap = crate::normal_defs::CmdargT { cmdchar: shifted, ..Default::default() };
            unsafe { unshift_special(&mut cap) };
            assert_eq!(cap.cmdchar, plain);
        }
    }

    #[test]
    fn unshift_special_leaves_an_unshifted_key_alone() {
        let _lock = crate::globals::global_state_test_lock();
        let mut cap =
            crate::normal_defs::CmdargT { cmdchar: i32::from(b'x'), ..Default::default() };
        unsafe { unshift_special(&mut cap) };
        assert_eq!(cap.cmdchar, i32::from(b'x'));
    }

    #[test]
    fn buf_has_cstyle_comments_finds_a_slash_leader() {
        // Cross-verified against real nvim: the default 'comments'
        // contains both "s1:/*" and "://".
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_com: Some(b"s1:/*,mb:*,ex:*/,://,b:#".to_vec()),
            ..Default::default()
        };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.curbuf;
        g.curbuf = &mut buf;

        assert!(unsafe { buf_has_cstyle_comments() });

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

    #[test]
    fn buf_has_cstyle_comments_is_false_without_one() {
        let _lock = crate::globals::global_state_test_lock();
        // Hash and quote leaders only - no `/` followed by `/` or `*`.
        let mut buf = crate::buffer_defs::BufT {
            b_p_com: Some(b"b:#,:%,n:>,fb:-".to_vec()),
            ..Default::default()
        };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.curbuf;
        g.curbuf = &mut buf;

        assert!(!unsafe { buf_has_cstyle_comments() });

        // An empty 'comments' likewise has nothing to find.
        unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_p_com = Some(Vec::new()) };
        assert!(!unsafe { buf_has_cstyle_comments() });

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

    #[test]
    fn buf_has_cstyle_comments_needs_the_slash_right_after_the_colon() {
        // A leader of "*" alone must not count, even though a
        // C-comment continuation uses it.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_com: Some(b"mb:*,ex:*/".to_vec()),
            ..Default::default()
        };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.curbuf;
        g.curbuf = &mut buf;

        assert!(!unsafe { buf_has_cstyle_comments() });

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

    #[test]
    fn clearop_resets_every_operator_field() {
        let _lock = crate::globals::global_state_test_lock();
        let mut oap = crate::normal_defs::OpargT {
            op_type: crate::ops_defs::OpType::Delete as i32,
            regname: i32::from(b'a'),
            motion_force: i32::from(b'v'),
            use_reg_one: true,
            restore_cursor: true,
            ..Default::default()
        };

        unsafe { clearop(&mut oap) };

        assert_eq!(oap.op_type, crate::ops_defs::OpType::Nop as i32);
        assert_eq!(oap.regname, 0);
        assert_eq!(oap.motion_force, 0);
        assert!(!oap.use_reg_one);
        assert!(!oap.restore_cursor);
    }

    #[test]
    fn clearop_also_clears_the_global_motion_force() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = globals.motion_force;
        globals.motion_force = i32::from(b'v');

        let mut oap = crate::normal_defs::OpargT::default();
        unsafe { clearop(&mut oap) };

        // The global is separate state from the oparg's own field, so
        // clearing only the latter would leave a stale force behind.
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.motion_force, 0);

        unsafe { crate::globals::GLOBALS.get_mut() }.motion_force = prev;
    }

    #[test]
    fn clearop_on_an_already_clear_oparg_is_idempotent() {
        let _lock = crate::globals::global_state_test_lock();
        let mut oap = crate::normal_defs::OpargT::default();
        unsafe { clearop(&mut oap) };
        unsafe { clearop(&mut oap) };
        assert_eq!(oap.op_type, crate::ops_defs::OpType::Nop as i32);
    }

    #[test]
    fn is_ident_plain_code_before_offset_is_true() {
        assert!(is_ident(b"int x = 5;\0", 5));
    }

    #[test]
    fn is_ident_inside_a_double_quoted_string_is_false() {
        // offset=6 lands right after the opening quote, inside "hi".
        assert!(!is_ident(b"x = \"hi\";\0", 6));
    }

    #[test]
    fn is_ident_after_a_closed_string_is_true() {
        // offset=9 is right after the closing quote - the string has
        // ended, so this position is NOT inside it.
        assert!(is_ident(b"x = \"hi\";\0", 9));
    }

    #[test]
    fn is_ident_inside_a_single_quoted_string_is_false() {
        assert!(!is_ident(b"c = 'x';\0", 5));
    }

    #[test]
    fn is_ident_an_escaped_quote_does_not_close_the_string() {
        // `"a\"b"` bytes: 0='"',1='a',2='\\',3='"',4='b',5='"',6=NUL.
        // The backslash-escaped quote at index 3 must NOT close the
        // string; offset=4 (the 'b') is still inside it.
        assert!(!is_ident(b"\"a\\\"b\"\0", 4));
    }

    #[test]
    fn is_ident_inside_a_block_comment_is_false() {
        assert!(!is_ident(b"/* comment */ x\0", 5));
    }

    #[test]
    fn is_ident_after_a_closed_block_comment_is_true() {
        assert!(is_ident(b"/* c */ x\0", 8));
    }

    #[test]
    fn is_ident_a_line_comment_makes_everything_after_it_false() {
        // Once `//` is seen, the function returns false immediately
        // (a line comment runs to the end of the line - there is no
        // "closing" it within the same line).
        assert!(!is_ident(b"x // comment\0", 4));
        assert!(!is_ident(b"x // comment\0", 12));
    }

    #[test]
    fn is_ident_offset_zero_is_always_true() {
        // The loop never runs at all - nothing has been scanned yet,
        // so we're trivially "not inside" anything.
        assert!(is_ident(b"\"unterminated\0", 0));
    }

    #[test]
    fn is_ident_stops_at_a_truncated_non_nul_terminated_slice() {
        // No NUL terminator at all - running out of the slice before
        // reaching `offset` is treated the same as hitting one.
        assert!(is_ident(b"abc", 10));
    }
}
