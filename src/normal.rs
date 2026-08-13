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
//! [`op_pending`] also translates the Normal-state pending-operator
//! predicate.
//!
//! Deferred: the remaining command-dispatch engine.

/// Operator state currently being processed (`current_oap`).
static CURRENT_OAP: crate::globals::GlobalCell<*mut crate::normal_defs::OpargT> =
    crate::globals::GlobalCell::new(std::ptr::null_mut());
/// Partially typed command shown to the user (`showcmd_buf`).
static SHOWCMD_BUF: crate::globals::GlobalCell<
    [u8; crate::normal_defs::SHOWCMD_BUFLEN],
> = crate::globals::GlobalCell::new([0; crate::normal_defs::SHOWCMD_BUFLEN]);
/// Saved show-command text while waiting through a partial mapping
/// (`old_showcmd_buf`).
static OLD_SHOWCMD_BUF: crate::globals::GlobalCell<
    [u8; crate::normal_defs::SHOWCMD_BUFLEN],
> = crate::globals::GlobalCell::new([0; crate::normal_defs::SHOWCMD_BUFLEN]);
/// Saved Visual mode to restore after a temporary change
/// (`VIsual_mode_orig`).
static VISUAL_MODE_ORIG: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);

/// Restore the saved Visual mode (`restore_visual_mode`).
///
/// # Safety
/// `GLOBALS.curbuf` must point to a live buffer whenever a mode is
/// saved.
pub unsafe fn restore_visual_mode() {
    // SAFETY: forwarded from this function's own safety doc.
    let mode = unsafe { *VISUAL_MODE_ORIG.get_mut() };
    if mode != i32::from(crate::ascii_defs::NUL) {
        // SAFETY: forwarded from this function's own safety doc.
        let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*curbuf).b_visual.vi_mode = mode };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *VISUAL_MODE_ORIG.get_mut() = i32::from(crate::ascii_defs::NUL) };
    }
}

/// Set `v:count`, `v:count1`, and optionally `v:prevcount` from a
/// Normal command (`set_vcount_ca`).
///
/// # Safety
/// Forwarded from [`crate::eval::vars::set_vcount`].
#[allow(dead_code)]
unsafe fn set_vcount_ca(cap: &crate::normal_defs::CmdargT, set_prevcount: &mut bool) {
    let mut count = i64::from(cap.count0);
    if cap.opcount != 0 {
        count = i64::from(cap.opcount) * if count == 0 { 1 } else { count };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::vars::set_vcount(count, if count == 0 { 1 } else { count }, *set_prevcount) };
    *set_prevcount = false;
}

/// Save the partially typed command while waiting for mapping input
/// (`push_showcmd`).
///
/// # Safety
/// Reads option state and mutates show-command file statics.
pub unsafe fn push_showcmd() {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sc == 0 {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let current = unsafe { SHOWCMD_BUF.get_mut() };
    let end = current
        .iter()
        .position(|&byte| byte == crate::ascii_defs::NUL)
        .unwrap_or(current.len() - 1);
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { OLD_SHOWCMD_BUF.get_mut() })[..=end].copy_from_slice(&current[..=end]);
}

/// Whether an operator, count, or register name is pending
/// (`op_pending`).
///
/// # Safety
/// A non-null `CURRENT_OAP` must point to a live `OpargT`; global state
/// must not be mutated concurrently.
#[must_use]
pub unsafe fn op_pending() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let oap = unsafe { *CURRENT_OAP.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let finish_op = unsafe { crate::globals::GLOBALS.get_mut() }.finish_op;
    if oap.is_null() {
        return true;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let oap = unsafe { &*oap };
    !(!finish_op
        && oap.prev_opcount == 0
        && oap.prev_count0 == 0
        && oap.op_type == crate::ops_defs::OpType::Nop as i32
        && oap.regname == i32::from(crate::ascii_defs::NUL))
}

/// Re-evaluate whether Normal mode is in a SafeState
/// (`normal_check_safe_state`).
///
/// # Safety
/// Forwarded from [`op_pending`] and
/// [`crate::state::may_trigger_safestate`].
#[allow(dead_code)]
unsafe fn normal_check_safe_state() {
    // SAFETY: forwarded from this function's own safety doc.
    let safe = !unsafe { op_pending() }
        && unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit == 0;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::state::may_trigger_safestate(safe) };
}

/// `|`: move to a specific screen column (`nv_pipe`).
///
/// `w_set_curswant` is deliberately left false: `w_curswant` must
/// record the column the user ASKED for, not the one actually reached.
/// Those differ when the line is too short, and keeping the requested
/// column is what lets a later vertical move return to it.
///
/// # Safety
/// Reads `GLOBALS.curwin`, which must be valid and non-null. Forwarded
/// from [`crate::insert::beginline`]/[`crate::cursor::coladvance`]'s
/// own safety docs.
pub unsafe fn nv_pipe(cap: &mut crate::normal_defs::CmdargT) {
    // SAFETY: cap.oap is a raw pointer in this crate's CmdargT.
    unsafe {
        (*cap.oap).motion_type = crate::normal_defs::MotionType::CharWise;
        (*cap.oap).inclusive = false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::insert::beginline(0) };

    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    if cap.count0 > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            crate::cursor::coladvance(curwin, cap.count0 - 1);
            (*curwin).w_curswant = cap.count0 - 1;
        }
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*curwin).w_curswant = 0 };
    }

    // Keep curswant at the column we wanted to go to, not where we
    // ended up; those differ if the line is too short.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*curwin).w_set_curswant = false };
}

/// Move the cursor off a trailing NUL after an operator
/// (`adjust_cursor`).
///
/// The cursor cannot remain on the NUL when the column is past the
/// start, the selection semantics do not allow it, and `'virtualedit'`
/// permits neither free positioning nor one-past-the-end. All four
/// conditions must hold together; any one of them makes sitting on the
/// NUL legitimate.
///
/// # Safety
/// Reads `GLOBALS.curwin`/`GLOBALS.Visual`, which must be valid.
/// Forwarded from [`crate::cursor::gchar_cursor`]/
/// [`crate::mbyte::mb_adjust_cursor`]'s own safety docs.
pub unsafe fn adjust_cursor(oap: &mut crate::normal_defs::OpargT) {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*curwin).w_cursor.col } == 0 {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::cursor::gchar_cursor() } != i32::from(crate::ascii_defs::NUL) {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let visual_active = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active;
    // SAFETY: forwarded from this function's own safety doc.
    let sel_is_old = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_sel
        .as_deref()
        .is_some_and(|s| s.first() == Some(&b'o'));
    if visual_active && !sel_is_old {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    if crate::state::virtual_active(unsafe { &*curwin }) {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let ve = crate::option::get_ve_flags(unsafe { &*curwin });
    if ve & crate::option_vars::opt_ve_flag::ONEMORE != 0 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*curwin).w_cursor.col -= 1 };
    // Prevent the cursor from moving onto a trail byte.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::mbyte::mb_adjust_cursor() };
    oap.inclusive = true;
}

/// Undo the `'selection'` == `"exclusive"` adjustment on whichever
/// end of the Visual area the cursor is NOT on (`unadjust_for_sel`).
///
/// Returns whether the position backed up to the previous line.
///
/// Nothing is undone unless `'selection'` is actually exclusive AND
/// the two ends genuinely differ - a collapsed selection has nothing
/// to give back.
///
/// The end that moves is the LATER of the two: with the cursor after
/// the start it is the cursor that was pushed forward, otherwise it
/// is the start. Picking the wrong one would shrink the selection
/// from the wrong side.
///
/// # Safety
/// Reads `GLOBALS.curwin`/`GLOBALS.curbuf`/`GLOBALS.Visual`, which
/// must be valid and non-null. Forwarded from
/// [`unadjust_for_sel_inner`]'s own safety doc.
pub unsafe fn unadjust_for_sel() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    if opts.p_sel.as_deref().and_then(|s| s.first().copied()) != Some(b'e') {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let visual_start = globals.Visual.start;
    let curwin = globals.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let cursor = unsafe { &*curwin }.w_cursor;

    if crate::mark_defs::equalpos(visual_start, cursor) {
        return false;
    }

    if crate::mark_defs::lt(visual_start, cursor) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { unadjust_for_sel_inner(&mut (*curwin).w_cursor) }
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let start = unsafe {
            std::ptr::addr_of_mut!((*crate::globals::GLOBALS.as_ptr()).Visual.start)
        };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { unadjust_for_sel_inner(&mut *start) }
    }
}

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

/// "G", "gg", CTRL-END, CTRL-HOME (`nv_goto`).
///
/// `cap.arg` is true for "G" (go to the last line); false means "gg"
/// (go to the first line). A count overrides either default, clamped
/// to the buffer's own line range.
///
/// # Safety
/// Reads `GLOBALS`/`OPTION_VARS` and forwards
/// [`crate::mark::setpcmark`]/[`crate::insert::beginline`]/
/// [`crate::fold::fold_open_cursor`]'s own safety docs. `cap.oap` must
/// point at a live [`crate::normal_defs::OpargT`].
pub unsafe fn nv_goto(cap: &mut crate::normal_defs::CmdargT) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    // SAFETY: curbuf is the live current buffer.
    let line_count = unsafe { (*curbuf).b_ml.ml_line_count };

    let mut lnum = if cap.arg != 0 { line_count } else { 1 };

    // SAFETY: cap.oap is a raw pointer in this crate's CmdargT.
    unsafe { (*cap.oap).motion_type = crate::normal_defs::MotionType::LineWise };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::mark::setpcmark() };

    // When a count is given, use it instead of the default lnum.
    if cap.count0 != 0 {
        lnum = cap.count0;
    }
    lnum = lnum.clamp(1, line_count);

    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: curwin is the live current window.
    unsafe { (*curwin).w_cursor.lnum = lnum };

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::insert::beginline(crate::insert::BL_SOL | crate::insert::BL_FIX) };

    // SAFETY: forwarded from this function's own safety doc.
    let fdo_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.fdo_flags;
    // SAFETY: forwarded from this function's own safety doc.
    let key_typed = unsafe { crate::globals::GLOBALS.get_mut() }.KeyTyped;
    // SAFETY: cap.oap is a raw pointer, see above.
    let op_type = unsafe { (*cap.oap).op_type };

    if fdo_flags & crate::option_vars::opt_fdo_flag::JUMP != 0
        && key_typed
        && op_type == crate::ops_defs::OpType::Nop as i32
    {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::fold::fold_open_cursor() };
    }
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

/// Whether a byte belongs to a balloon-evaluation item
/// (`find_is_eval_item`).
#[allow(dead_code)]
fn find_is_eval_item(
    text: &[u8],
    index: usize,
    col: &mut i32,
    brackets: &mut i32,
    direction: crate::vim_defs::Direction,
) -> bool {
    let byte = text.get(index).copied().unwrap_or(0);
    if (byte == b']' && direction == crate::vim_defs::Direction::Backward)
        || (byte == b'[' && direction == crate::vim_defs::Direction::Forward)
    {
        *brackets += 1;
    }
    if *brackets > 0 {
        if (byte == b'[' && direction == crate::vim_defs::Direction::Backward)
            || (byte == b']' && direction == crate::vim_defs::Direction::Forward)
        {
            *brackets -= 1;
        }
        return true;
    }
    if byte == b'.' {
        return true;
    }

    let is_arrow = match direction {
        crate::vim_defs::Direction::Backward => {
            byte == b'>'
                && index
                    .checked_sub(1)
                    .and_then(|previous| text.get(previous))
                    == Some(&b'-')
        }
        crate::vim_defs::Direction::Forward => {
            byte == b'-' && text.get(index + 1) == Some(&b'>')
        }
        _ => false,
    };
    if is_arrow {
        *col += direction as i32;
        return true;
    }
    false
}

/// Set `v:operator` for an operator type (`set_op_var`).
///
/// # Safety
/// Forwarded from [`crate::eval::vars::set_vim_var_string`].
#[allow(dead_code)]
unsafe fn set_op_var(optype: crate::ops_defs::OpType) {
    use crate::eval::vars::VimVarIndex;
    if optype == crate::ops_defs::OpType::Nop {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::eval::vars::set_vim_var_string(VimVarIndex::Op, None) };
        return;
    }

    let mut opchars = vec![crate::ops::get_op_char(optype)];
    let extra = crate::ops::get_extra_op_char(optype);
    if extra != crate::ascii_defs::NUL {
        opchars.push(extra);
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::eval::vars::set_vim_var_string(VimVarIndex::Op, Some(&opchars))
    };
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

    struct CurrentOapGuard(*mut crate::normal_defs::OpargT);

    struct SafeStateReset;

    struct OperatorVarGuard(crate::eval::typval_defs::TypvalValue);

    impl OperatorVarGuard {
        fn save() -> Self {
            let value = unsafe {
                std::mem::replace(
                    &mut (*crate::eval::vars::get_vim_var_tv(
                        crate::eval::vars::VimVarIndex::Op,
                    ))
                    .value,
                    crate::eval::typval_defs::TypvalValue::String(None),
                )
            };
            Self(value)
        }
    }

    impl Drop for OperatorVarGuard {
        fn drop(&mut self) {
            let saved = std::mem::replace(
                &mut self.0,
                crate::eval::typval_defs::TypvalValue::Unknown,
            );
            unsafe {
                (*crate::eval::vars::get_vim_var_tv(
                    crate::eval::vars::VimVarIndex::Op,
                ))
                .value = saved;
            }
        }
    }

    impl Drop for SafeStateReset {
        fn drop(&mut self) {
            crate::state::state_no_longer_safe();
        }
    }

    struct ShowcmdGuard {
        option: i32,
        current: [u8; crate::normal_defs::SHOWCMD_BUFLEN],
        old: [u8; crate::normal_defs::SHOWCMD_BUFLEN],
    }

    struct VisualModeGuard(i32);

    struct VcountGuard {
        count: i64,
        count1: i64,
        prevcount: i64,
    }

    impl VcountGuard {
        fn capture() -> Self {
            use crate::eval::vars::VimVarIndex;
            Self {
                count: unsafe { crate::eval::vars::get_vim_var_nr(VimVarIndex::Count) },
                count1: unsafe { crate::eval::vars::get_vim_var_nr(VimVarIndex::Count1) },
                prevcount: unsafe {
                    crate::eval::vars::get_vim_var_nr(VimVarIndex::Prevcount)
                },
            }
        }
    }

    impl Drop for VcountGuard {
        fn drop(&mut self) {
            use crate::eval::vars::VimVarIndex;
            unsafe {
                crate::eval::vars::set_vim_var_nr(VimVarIndex::Count, self.count);
                crate::eval::vars::set_vim_var_nr(VimVarIndex::Count1, self.count1);
                crate::eval::vars::set_vim_var_nr(VimVarIndex::Prevcount, self.prevcount);
            }
        }
    }

    impl VisualModeGuard {
        fn set(value: i32) -> Self {
            let saved = unsafe { *VISUAL_MODE_ORIG.get_mut() };
            unsafe { *VISUAL_MODE_ORIG.get_mut() = value };
            Self(saved)
        }
    }

    impl Drop for VisualModeGuard {
        fn drop(&mut self) {
            unsafe { *VISUAL_MODE_ORIG.get_mut() = self.0 };
        }
    }

    impl ShowcmdGuard {
        fn capture() -> Self {
            Self {
                option: unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sc,
                current: *unsafe { SHOWCMD_BUF.get_mut() },
                old: *unsafe { OLD_SHOWCMD_BUF.get_mut() },
            }
        }
    }

    impl Drop for ShowcmdGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sc = self.option;
            *unsafe { SHOWCMD_BUF.get_mut() } = self.current;
            *unsafe { OLD_SHOWCMD_BUF.get_mut() } = self.old;
        }
    }

    impl CurrentOapGuard {
        fn install(value: *mut crate::normal_defs::OpargT) -> Self {
            let saved = unsafe { *CURRENT_OAP.get_mut() };
            unsafe { *CURRENT_OAP.get_mut() = value };
            Self(saved)
        }
    }

    impl Drop for CurrentOapGuard {
        fn drop(&mut self) {
            unsafe { *CURRENT_OAP.get_mut() = self.0 };
        }
    }

    #[test]
    fn op_pending_detects_operator_count_register_and_finish_state() {
        let _lock = crate::globals::global_state_test_lock();
        let mut oap = crate::normal_defs::OpargT::default();
        let oap_ptr = std::ptr::addr_of_mut!(oap);
        let _oap = CurrentOapGuard::install(oap_ptr);
        let _finish = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.finish_op, false)
        };

        assert!(!unsafe { op_pending() });
        unsafe { (*oap_ptr).prev_count0 = 2 };
        assert!(unsafe { op_pending() });
        unsafe {
            (*oap_ptr).prev_count0 = 0;
            (*oap_ptr).regname = i32::from(b'a');
        }
        assert!(unsafe { op_pending() });
        unsafe {
            (*oap_ptr).regname = 0;
            crate::globals::GLOBALS.get_mut().finish_op = true;
        }
        assert!(unsafe { op_pending() });
        unsafe { *CURRENT_OAP.get_mut() = std::ptr::null_mut() };
        assert!(unsafe { op_pending() });
    }

    #[test]
    fn normal_check_safe_state_requires_no_operator_or_restart() {
        let _lock = crate::globals::global_state_test_lock();
        let _safe = SafeStateReset;
        let mut buffer = crate::buffer_defs::BufT::default();
        let buffer_ptr = std::ptr::addr_of_mut!(buffer);
        let _buffer = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.curbuf,
                buffer_ptr,
            )
        };
        let mut oap = crate::normal_defs::OpargT::default();
        let oap_ptr = std::ptr::addr_of_mut!(oap);
        let _oap = CurrentOapGuard::install(oap_ptr);
        let _finish = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.finish_op,
                false,
            )
        };
        let _restart = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.restart_edit,
                0,
            )
        };
        let _state = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.State,
                crate::state_defs::mode::NORMAL as i32,
            )
        };
        let _busy = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.global_busy,
                0,
            )
        };
        let _debug = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.debug_mode,
                false,
            )
        };

        crate::state::state_no_longer_safe();
        unsafe { normal_check_safe_state() };
        assert!(crate::state::get_was_safe_state());

        unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit = 1;
        unsafe { normal_check_safe_state() };
        assert!(!crate::state::get_was_safe_state());
    }

    #[test]
    fn push_showcmd_copies_through_nul_only_when_showcmd_is_enabled() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShowcmdGuard::capture();
        let current = unsafe { SHOWCMD_BUF.get_mut() };
        current.fill(0x44);
        current[..4].copy_from_slice(b"12d\0");
        unsafe { OLD_SHOWCMD_BUF.get_mut() }.fill(0xaa);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sc = 0;
        unsafe { push_showcmd() };
        assert_eq!((unsafe { OLD_SHOWCMD_BUF.get_mut() })[0], 0xaa);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sc = 1;
        unsafe { push_showcmd() };
        assert_eq!(&(unsafe { OLD_SHOWCMD_BUF.get_mut() })[..4], b"12d\0");
        assert_eq!((unsafe { OLD_SHOWCMD_BUF.get_mut() })[4], 0xaa);
    }

    #[test]
    fn restore_visual_mode_applies_and_clears_a_saved_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        buf.b_visual.vi_mode = i32::from(b'v');
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let _buf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr) };
        let _mode = VisualModeGuard::set(i32::from(b'V'));

        unsafe { restore_visual_mode() };

        assert_eq!(unsafe { (*buf_ptr).b_visual.vi_mode }, i32::from(b'V'));
        assert_eq!(unsafe { *VISUAL_MODE_ORIG.get_mut() }, 0);

        unsafe { (*buf_ptr).b_visual.vi_mode = i32::from(b'v') };
        unsafe { restore_visual_mode() };
        assert_eq!(unsafe { (*buf_ptr).b_visual.vi_mode }, i32::from(b'v'));
    }

    #[test]
    fn set_vcount_ca_multiplies_counts_and_sets_prevcount_once() {
        use crate::eval::vars::VimVarIndex;
        let _lock = crate::globals::global_state_test_lock();
        let _guard = VcountGuard::capture();
        unsafe {
            crate::eval::vars::set_vim_var_nr(VimVarIndex::Count, 5);
            crate::eval::vars::set_vim_var_nr(VimVarIndex::Count1, 5);
            crate::eval::vars::set_vim_var_nr(VimVarIndex::Prevcount, 0);
        }
        let cap = crate::normal_defs::CmdargT {
            count0: 3,
            opcount: 4,
            ..Default::default()
        };
        let mut set_prevcount = true;

        unsafe { set_vcount_ca(&cap, &mut set_prevcount) };
        assert!(!set_prevcount);
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_nr(VimVarIndex::Count) },
            12
        );
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_nr(VimVarIndex::Count1) },
            12
        );
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_nr(VimVarIndex::Prevcount) },
            5
        );

        let zero = crate::normal_defs::CmdargT::default();
        unsafe { set_vcount_ca(&zero, &mut set_prevcount) };
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_nr(VimVarIndex::Count) },
            0
        );
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_nr(VimVarIndex::Count1) },
            1
        );
        assert_eq!(
            unsafe { crate::eval::vars::get_vim_var_nr(VimVarIndex::Prevcount) },
            5
        );
    }

    #[test]
    fn nv_pipe_records_the_requested_column_not_the_reached_one() {
        // w_set_curswant stays false so w_curswant keeps the column
        // the user ASKED for. Those differ when the line is too short
        // (cross-verified: `99|` on a 10-char line lands at col 10),
        // and keeping the request is what lets a later vertical move
        // return to it.
        //
        // coladvance reads the real cursor line, so the buffer needs
        // actual memline content rather than a bare default.
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_win, prev_buf) = (g.curwin, g.curbuf);

        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT {
            w_buffer: &mut buf,
            w_set_curswant: true,
            ..Default::default()
        };
        win.w_cursor.lnum = 1;

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = &mut win;
        g.curbuf = &mut buf;
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"abcdefghij\0") },
            crate::vim_defs::OK
        );

        let mut oap = crate::normal_defs::OpargT {
            motion_type: crate::normal_defs::MotionType::LineWise,
            inclusive: true,
            ..Default::default()
        };
        let mut cap =
            crate::normal_defs::CmdargT { oap: &mut oap, count0: 5, ..Default::default() };

        unsafe { nv_pipe(&mut cap) };

        assert_eq!(oap.motion_type, crate::normal_defs::MotionType::CharWise);
        assert!(!oap.inclusive);
        // count0 - 1, i.e. the 0-based column asked for.
        assert_eq!(win.w_curswant, 4);
        assert_eq!(win.w_cursor.col, 4);
        assert!(!win.w_set_curswant, "the requested column must be preserved");

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = prev_win;
        g.curbuf = prev_buf;
    }

    #[test]
    fn nv_pipe_with_no_count_goes_to_column_zero() {
        // Cross-verified: `1|` lands at column 1 (0-based 0), and a
        // bare `|` behaves the same way.
        let _lock = crate::globals::global_state_test_lock();
        let prev_win = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;

        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT {
            w_buffer: &mut buf,
            w_curswant: 9,
            ..Default::default()
        };
        win.w_cursor.lnum = 1;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win;

        let mut oap = crate::normal_defs::OpargT::default();
        let mut cap =
            crate::normal_defs::CmdargT { oap: &mut oap, count0: 0, ..Default::default() };
        unsafe { nv_pipe(&mut cap) };

        assert_eq!(win.w_curswant, 0);
        assert!(!win.w_set_curswant);

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_win;
    }

    // --- nv_goto ---

    /// A buffer with a real memline holding one line, for the
    /// `beginline` call `nv_goto` makes (which reads the line's own
    /// text). Mirrors `insert.rs`'s own equivalent test helper.
    fn goto_buf_with_lines(lines: &[&[u8]]) -> crate::buffer_defs::BufT {
        let mut buf = crate::buffer_defs::BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, lines[0]) },
            crate::vim_defs::OK
        );
        for (i, line) in lines.iter().enumerate().skip(1) {
            assert_eq!(
                unsafe { crate::memline::ml_append_buf(&mut buf, i as i32, line, 0, false) },
                crate::vim_defs::OK
            );
        }
        buf
    }

    fn goto_close_buf(buf: crate::buffer_defs::BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    /// Run `nv_goto` against a 3-line buffer and report where the
    /// cursor lands, given `arg` ("G" vs "gg") and a count.
    fn run_nv_goto(arg: i32, count0: i32) -> i32 {
        let mut tp = crate::buffer_defs::TabpageT::default();
        let mut buf = goto_buf_with_lines(&[b"aaa\0", b"  bbb\0", b"ccc\0"]);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = crate::buffer_defs::WinT { w_buffer: buf_ptr, ..Default::default() };
        win.w_cursor.lnum = 1;
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_win, prev_buf, prev_tab) = (g.curwin, g.curbuf, g.curtab);
        g.curwin = win_ptr;
        g.curbuf = buf_ptr;
        g.curtab = &mut tp;
        // Keep the fold-open branch out of the picture.
        let prev_fdo = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.fdo_flags;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.fdo_flags = 0;

        let mut oap = crate::normal_defs::OpargT {
            motion_type: crate::normal_defs::MotionType::CharWise,
            ..Default::default()
        };
        let mut cap = crate::normal_defs::CmdargT { oap: &mut oap, arg, count0, ..Default::default() };

        unsafe { nv_goto(&mut cap) };

        assert_eq!(
            oap.motion_type,
            crate::normal_defs::MotionType::LineWise,
            "G/gg is always a linewise motion"
        );
        let landed = win.w_cursor.lnum;

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.fdo_flags = prev_fdo;
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = prev_win;
        g.curbuf = prev_buf;
        g.curtab = prev_tab;
        goto_close_buf(buf);
        landed
    }

    #[test]
    fn nv_goto_with_arg_set_goes_to_the_last_line() {
        // "G" with no count.
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(run_nv_goto(1, 0), 3);
    }

    #[test]
    fn nv_goto_without_arg_goes_to_the_first_line() {
        // "gg" with no count.
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(run_nv_goto(0, 0), 1);
    }

    #[test]
    fn nv_goto_count_overrides_either_default() {
        // A count wins over both the "G" and the "gg" default.
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(run_nv_goto(1, 2), 2);
        assert_eq!(run_nv_goto(0, 2), 2);
    }

    #[test]
    fn nv_goto_clamps_a_count_past_the_last_line() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(run_nv_goto(0, 99), 3);
    }

    #[test]
    fn nv_goto_treats_a_zero_count_as_absent_rather_than_clamping_to_one() {
        // count0 == 0 means "no count given", so the "G" default
        // (last line) still applies - it is NOT clamped up to 1.
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(run_nv_goto(1, 0), 3);
    }

    #[test]
    fn nv_goto_clamps_a_negative_count_up_to_the_first_line() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(run_nv_goto(1, -5), 1);
    }

    #[test]
    fn adjust_cursor_leaves_column_zero_alone() {
        // The column must be past the start; at column 0 there is
        // nowhere to step back to, so sitting on a NUL is legitimate.
        let _lock = crate::globals::global_state_test_lock();
        let prev_win = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;

        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT { w_buffer: &mut buf, ..Default::default() };
        win.w_cursor.lnum = 1;
        win.w_cursor.col = 0;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win;

        let mut oap = crate::normal_defs::OpargT { inclusive: false, ..Default::default() };
        unsafe { adjust_cursor(&mut oap) };

        assert_eq!(win.w_cursor.col, 0);
        assert!(!oap.inclusive, "nothing was adjusted, so nothing is inclusive");

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_win;
    }

    // ---- unadjust_for_sel ----

    /// Saves/restores the globals `unadjust_for_sel` reads and writes.
    struct SelGuard {
        p_sel: Option<Vec<u8>>,
        visual: crate::normal_defs::VisualState,
        curwin: *mut crate::buffer_defs::WinT,
        curbuf: *mut crate::buffer_defs::BufT,
    }

    impl SelGuard {
        fn save() -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            Self {
                p_sel: unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel.take(),
                visual: g.Visual,
                curwin: g.curwin,
                curbuf: g.curbuf,
            }
        }
    }

    impl Drop for SelGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = self.p_sel.take();
            g.Visual = self.visual;
            g.curwin = self.curwin;
            g.curbuf = self.curbuf;
        }
    }

    /// Installs an exclusive selection running from `start` to the
    /// cursor, and returns the boxed window/buffer keeping them alive.
    fn exclusive_selection(
        start: crate::pos_defs::PosT,
        cursor: crate::pos_defs::PosT,
    ) -> (Box<crate::buffer_defs::WinT>, Box<crate::buffer_defs::BufT>) {
        let mut buf = Box::new(crate::buffer_defs::BufT::default());
        buf.b_ml.ml_line_count = 100;
        let mut win = Box::new(crate::buffer_defs::WinT {
            w_cursor: cursor,
            w_buffer: std::ptr::null_mut(),
            ..Default::default()
        });
        win.w_buffer = std::ptr::from_mut(&mut *buf);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = Some(b"exclusive".to_vec());
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = std::ptr::from_mut(&mut *win);
        g.curbuf = std::ptr::from_mut(&mut *buf);
        g.Visual.start = start;
        (win, buf)
    }

    /// With the cursor AFTER the start, it is the cursor that was
    /// pushed forward, so the cursor is what steps back.
    ///
    /// Both ends carry a `coladd`, so `unadjust_for_sel_inner` takes
    /// its virtual-space branch and never reads buffer text - keeping
    /// this test about WHICH end moves, which is all this function
    /// decides.
    #[test]
    fn unadjust_for_sel_backs_up_the_cursor_when_it_is_the_later_end() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = SelGuard::save();
        let (win, _buf) = exclusive_selection(
            crate::pos_defs::PosT { lnum: 5, col: 2, coladd: 3 },
            crate::pos_defs::PosT { lnum: 5, col: 7, coladd: 3 },
        );

        assert!(!unsafe { unadjust_for_sel() });
        assert_eq!(win.w_cursor.coladd, 2, "the cursor steps back");
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start.coladd,
            3,
            "the start must be untouched"
        );
    }

    /// With the cursor BEFORE the start, the start is the later end
    /// and is what steps back instead. An implementation that always
    /// moved the cursor would shrink the selection from the wrong
    /// side.
    #[test]
    fn unadjust_for_sel_backs_up_the_start_when_it_is_the_later_end() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = SelGuard::save();
        let (win, _buf) = exclusive_selection(
            crate::pos_defs::PosT { lnum: 5, col: 7, coladd: 3 },
            crate::pos_defs::PosT { lnum: 5, col: 2, coladd: 3 },
        );

        assert!(!unsafe { unadjust_for_sel() });
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start.coladd,
            2,
            "the start steps back"
        );
        assert_eq!(win.w_cursor.coladd, 3, "the cursor must be untouched");
    }

    /// An inclusive 'selection' never adjusted anything, so there is
    /// nothing to undo.
    #[test]
    fn unadjust_for_sel_does_nothing_unless_selection_is_exclusive() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = SelGuard::save();
        let (win, _buf) = exclusive_selection(
            crate::pos_defs::PosT { lnum: 5, col: 2, coladd: 3 },
            crate::pos_defs::PosT { lnum: 5, col: 7, coladd: 3 },
        );
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = Some(b"inclusive".to_vec());

        assert!(!unsafe { unadjust_for_sel() });
        assert_eq!(win.w_cursor.coladd, 3, "nothing may move");
    }

    /// A collapsed selection has nothing to give back, even with an
    /// exclusive 'selection'.
    #[test]
    fn unadjust_for_sel_does_nothing_when_both_ends_are_equal() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = SelGuard::save();
        let same = crate::pos_defs::PosT { lnum: 5, col: 4, coladd: 3 };
        let (win, _buf) = exclusive_selection(same, same);

        assert!(!unsafe { unadjust_for_sel() });
        assert_eq!(win.w_cursor.coladd, 3);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.Visual.start.coladd, 3);
    }

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
    fn find_is_eval_item_tracks_brackets_dots_and_arrows() {
        use crate::vim_defs::Direction::{Backward, Forward};

        let mut col = 0;
        let mut brackets = 0;
        assert!(find_is_eval_item(b"[x]", 0, &mut col, &mut brackets, Forward));
        assert_eq!(brackets, 1);
        assert!(find_is_eval_item(b"[x]", 1, &mut col, &mut brackets, Forward));
        assert!(find_is_eval_item(b"[x]", 2, &mut col, &mut brackets, Forward));
        assert_eq!(brackets, 0);

        assert!(find_is_eval_item(b"s.var", 1, &mut col, &mut brackets, Forward));
        col = 2;
        assert!(find_is_eval_item(b"s->var", 1, &mut col, &mut brackets, Forward));
        assert_eq!(col, 3);
        assert!(find_is_eval_item(b"s->var", 2, &mut col, &mut brackets, Backward));
        assert_eq!(col, 2);
        assert!(!find_is_eval_item(b"word", 1, &mut col, &mut brackets, Forward));
    }

    #[test]
    fn set_op_var_publishes_single_and_double_character_operators() {
        let _lock = crate::globals::global_state_test_lock();
        let _operator = OperatorVarGuard::save();

        unsafe { set_op_var(crate::ops_defs::OpType::Delete) };
        assert_eq!(
            unsafe {
                crate::eval::vars::get_vim_var_str(
                    crate::eval::vars::VimVarIndex::Op,
                )
            },
            b"d"
        );

        unsafe { set_op_var(crate::ops_defs::OpType::Format) };
        assert_eq!(
            unsafe {
                crate::eval::vars::get_vim_var_str(
                    crate::eval::vars::VimVarIndex::Op,
                )
            },
            b"gq"
        );

        unsafe { set_op_var(crate::ops_defs::OpType::Nop) };
        assert!(
            unsafe {
                crate::eval::vars::get_vim_var_str(
                    crate::eval::vars::VimVarIndex::Op,
                )
            }
            .is_empty()
        );
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
