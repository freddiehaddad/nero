//! Translated from `src/nvim/ops.c` (tractable core only).
//!
//! `ops.c` is neovim's register/yank/paste/shift/format/case-change
//! operator-execution file (thousands of lines) - almost entirely
//! dependent on the register-storage subsystem, real buffer
//! modification, and the eval engine, not attempted here.
//!
//! Translated: [`OPCHARS`] (the `opchars[][3]` table mapping each
//! [`crate::ops_defs::OpType`] to its one/two command characters and
//! `OPF_*` flags) and its five small, genuinely self-contained
//! consumers: [`get_op_type`] (chars -> `OpType`, the reverse lookup),
//! [`get_op_char`]/[`get_extra_op_char`] (`OpType` -> its first/second
//! command character), [`op_on_lines`]/[`op_is_change`] (`OpType` ->
//! whether it always works on whole lines / changes text). None of
//! these need any not-yet-translated subsystem - just the table
//! itself.
//!
//! Also translated: [`reset_lbr`]/[`restore_lbr`] (temporarily
//! disable/restore `'linebreak'` around an operation, e.g. `"gw"`,
//! that needs plain-width column arithmetic - only touch `curwin`'s
//! own plain fields) and [`clear_oparg`] (a `CLEAR_POINTER` one-liner
//! now that `normal_defs.rs`'s `OpargT` exists).
//!
//! Also translated: [`set_ref_in_opfunc`] - marks the global
//! `'operatorfunc'` callback (`OPFUNC_CB`) with a GC `copy_id` so it
//! survives garbage collection, via `eval/eval.rs`'s
//! `set_ref_in_callback`. `OPFUNC_CB` stays `Callback::None` forever
//! today (see its own doc comment) - matches every real, unconfigured
//! session, since nothing here can populate a real `'operatorfunc'`
//! value yet.
//!
//! Also translated: `is_ex_cmdchar` (a `static` predicate - whether a
//! `cmdarg_T`'s `cmdchar` started a `:`-command-line-shaped operator,
//! e.g. `:'<,'>d` or a `<Cmd>` mapping). Trivial and self-contained
//! (just compares `cap.cmdchar` against `':'`/`K_COMMAND`), but its
//! only real caller, `op_function` (the `g@` operator-function
//! dispatcher), is not translated - kept `#[allow(dead_code)]` for now,
//! matching `marktree.rs`'s own `itr_eq` precedent for a small, simple,
//! no-design-freedom function translated ahead of its real caller.
//!
//! Also translated: [`skip_comment`] - unblocked by `change.c`'s
//! `get_last_leader_offset`/`get_leader_len`. Returns `(new_offset,
//! is_comment)` instead of the original's own advanced-or-unchanged
//! `char *line` return, matching this crate's byte-offset-instead-of-
//! pointer idiom (`0` means "unchanged", matching every real
//! non-advancing return path). Its only real caller
//! (`do_join`/`ops.c`'s own line-joining logic) is not translated -
//! translated ahead of it, matching the same precedent as
//! `is_ex_cmdchar` above.
//!
//! Also translated: `get_vts`/`get_vts_sum` (variable-tabstop-array
//! width/cumulative-sum lookups, matching `indent.rs`'s own
//! established "no leading count element" `vts` slice convention) and
//! their real consumers `get_new_sw_indent`/`get_new_vts_indent` (pure
//! new-indent computation for `'<'`/`'>'` shift commands, needing only
//! already-real `crate::indent::get_indent`/`crate::math::trim_to_int`).
//! Their own real caller, `shift_line` (which actually WRITES the new
//! indent via `change_indent`/`set_indent`, neither translated), stays
//! blocked - translated ahead of it, matching the established
//! "translate ahead of a real caller" precedent. `shift_block` (the
//! Visual-block-mode sibling) needs substantially more machinery
//! (`block_prep`, `init_charsize_arg`-driven column arithmetic, real
//! buffer modification) and remains fully blocked too.
//!
//! Also translated: [`mb_adjust_opend`] - extends an inclusive
//! `oparg_T.end.col` to the last byte of the (possibly multi-byte,
//! possibly composing-character) character it currently points into,
//! needing only already-real `crate::memline::ml_get`/
//! `crate::mbyte::utf_head_off`/`utfc_ptr2len`. Its own real callers
//! (`op_delete`/`op_yank`/etc., none translated) remain blocked -
//! translated ahead of them, matching the established "translate
//! ahead of a real caller" precedent.
//!
//! Also translated: `line_count_info` (word/char/byte counting for one
//! line, used by `cursor_pos_info`'s "g CTRL-G" word/char/byte-count
//! display) and [`get_region_bytecount`] (a buffer-region's total byte
//! count, used by `op_delete`'s own "deleting characters between
//! lines" branch). Both are pure computation needing only already-real
//! `crate::ascii_defs::ascii_isspace`/`crate::mbyte::utfc_ptr2len`/
//! `crate::memline::ml_get_buf_len`. Their own real callers
//! (`cursor_pos_info`, `op_delete`) remain blocked (the former needs
//! the eval engine's `dict_T` output plus the message-display pipeline
//! for `:g CTRL-G`'s own status line; the latter needs real buffer
//! splicing/`truncate_line`) - translated ahead of them, matching the
//! established "translate ahead of a real caller" precedent.
//!
//! Also translated: `get_op_vcol` (a `static` helper - computes a
//! blockwise-Visual operator's virtual-column extent and converts it
//! into real character positions in `oap.start`/`oap.end`), needing
//! only already-real `crate::mark::mark_mb_adjustpos`/
//! `crate::plines::getvvcol`/`crate::cursor::coladvance`. Its own real
//! caller (`do_pending_operator`, the "an operator finished, act on
//! it" dispatcher) is not translated - kept `#[allow(dead_code)]` for
//! now, matching the same "translate ahead of a real caller"
//! precedent.
//!
//! Deferred: everything else in the file.

use crate::ascii_defs::ascii_isspace;
use crate::ops_defs::OpType;

/// `OPF_*` flags for [`OPCHARS`]' third element (`OPF_LINES`/
/// `OPF_CHANGE`).
pub mod opf_flag {
    /// operator always works on lines (`OPF_LINES`).
    pub const LINES: u8 = 1;
    /// operator changes text (`OPF_CHANGE`).
    pub const CHANGE: u8 = 2;
}

/// The names of operators (`opchars`). Each entry is `(char1, char2,
/// flags)`; `char2` is `0` (`NUL`) when the operator has only one
/// command character. Indexed by [`OpType`] - the order must
/// correspond exactly (mechanically transcribed from the original in
/// the same order, cross-checked entry-by-entry against
/// `OpType`'s own doc comments, which quote the same command strings).
pub const OPCHARS: [(u8, u8, u8); 30] = [
    (0, 0, 0),                                       // OP_NOP
    (b'd', 0, opf_flag::CHANGE),                     // OP_DELETE
    (b'y', 0, 0),                                     // OP_YANK
    (b'c', 0, opf_flag::CHANGE),                     // OP_CHANGE
    (b'<', 0, opf_flag::LINES | opf_flag::CHANGE),   // OP_LSHIFT
    (b'>', 0, opf_flag::LINES | opf_flag::CHANGE),   // OP_RSHIFT
    (b'!', 0, opf_flag::LINES | opf_flag::CHANGE),   // OP_FILTER
    (b'g', b'~', opf_flag::CHANGE),                  // OP_TILDE
    (b'=', 0, opf_flag::LINES | opf_flag::CHANGE),   // OP_INDENT
    (b'g', b'q', opf_flag::LINES | opf_flag::CHANGE), // OP_FORMAT
    (b':', 0, opf_flag::LINES),                      // OP_COLON
    (b'g', b'U', opf_flag::CHANGE),                  // OP_UPPER
    (b'g', b'u', opf_flag::CHANGE),                  // OP_LOWER
    (b'J', 0, opf_flag::LINES | opf_flag::CHANGE),   // OP_JOIN
    (b'g', b'J', opf_flag::LINES | opf_flag::CHANGE), // OP_JOIN_NS
    (b'g', b'?', opf_flag::CHANGE),                  // OP_ROT13
    (b'r', 0, opf_flag::CHANGE),                     // OP_REPLACE
    (b'I', 0, opf_flag::CHANGE),                     // OP_INSERT
    (b'A', 0, opf_flag::CHANGE),                     // OP_APPEND
    (b'z', b'f', 0),                                  // OP_FOLD
    (b'z', b'o', opf_flag::LINES),                   // OP_FOLDOPEN
    (b'z', b'O', opf_flag::LINES),                   // OP_FOLDOPENREC
    (b'z', b'c', opf_flag::LINES),                   // OP_FOLDCLOSE
    (b'z', b'C', opf_flag::LINES),                   // OP_FOLDCLOSEREC
    (b'z', b'd', opf_flag::LINES),                   // OP_FOLDDEL
    (b'z', b'D', opf_flag::LINES),                   // OP_FOLDDELREC
    (b'g', b'w', opf_flag::LINES | opf_flag::CHANGE), // OP_FORMAT2
    (b'g', b'@', opf_flag::CHANGE),                  // OP_FUNCTION
    (crate::ascii_defs::CTRL_A, 0, opf_flag::CHANGE), // OP_NR_ADD
    (crate::ascii_defs::CTRL_X, 0, opf_flag::CHANGE), // OP_NR_SUB
];

/// The `OpType` variants in the same order as [`OPCHARS`], so
/// `OPCHARS[i]` corresponds to `OP_TYPE_ORDER[i]`.
const OP_TYPE_ORDER: [OpType; 30] = [
    OpType::Nop,
    OpType::Delete,
    OpType::Yank,
    OpType::Change,
    OpType::Lshift,
    OpType::Rshift,
    OpType::Filter,
    OpType::Tilde,
    OpType::Indent,
    OpType::Format,
    OpType::Colon,
    OpType::Upper,
    OpType::Lower,
    OpType::Join,
    OpType::JoinNs,
    OpType::Rot13,
    OpType::Replace,
    OpType::Insert,
    OpType::Append,
    OpType::Fold,
    OpType::Foldopen,
    OpType::Foldopenrec,
    OpType::Foldclose,
    OpType::Foldcloserec,
    OpType::Folddel,
    OpType::Folddelrec,
    OpType::Format2,
    OpType::Function,
    OpType::NrAdd,
    OpType::NrSub,
];

/// Translate a command name into an operator type. Must only be
/// called with a valid operator name (`get_op_type`).
///
/// # Panics
/// If `char1`/`char2` don't match any entry in [`OPCHARS`] - matches
/// the original's own `internal_error("get_op_type()")` call (a
/// caller's-own-contract violation, per this function's own doc
/// comment: "Must only be called with a valid operator name!").
#[must_use]
pub fn get_op_type(char1: i32, char2: i32) -> OpType {
    if char1 == i32::from(b'r') {
        return OpType::Replace;
    }
    if char1 == i32::from(b'~') {
        return OpType::Tilde;
    }
    if char1 == i32::from(b'g') && char2 == i32::from(crate::ascii_defs::CTRL_A) {
        return OpType::NrAdd;
    }
    if char1 == i32::from(b'g') && char2 == i32::from(crate::ascii_defs::CTRL_X) {
        return OpType::NrSub;
    }
    if char1 == i32::from(b'z') && char2 == i32::from(b'y') {
        return OpType::Yank;
    }

    for (i, &(c1, c2, _)) in OPCHARS.iter().enumerate() {
        if i32::from(c1) == char1 && i32::from(c2) == char2 {
            return OP_TYPE_ORDER[i];
        }
    }
    panic!("get_op_type: invalid operator name char1={char1} char2={char2} (caller's own contract)");
}

/// Return `true` if operator `op` always works on whole lines
/// (`op_on_lines`).
#[must_use]
pub fn op_on_lines(op: OpType) -> bool {
    OPCHARS[op as usize].2 & opf_flag::LINES != 0
}

/// Return `true` if operator `op` changes text (`op_is_change`).
#[must_use]
pub fn op_is_change(op: OpType) -> bool {
    OPCHARS[op as usize].2 & opf_flag::CHANGE != 0
}

/// Get first operator command character; may be `'g'` or `'z'` if
/// there is another command character (`get_op_char`).
#[must_use]
pub fn get_op_char(optype: OpType) -> u8 {
    OPCHARS[optype as usize].0
}

/// Get second operator command character (`get_extra_op_char`).
#[must_use]
pub fn get_extra_op_char(optype: OpType) -> u8 {
    OPCHARS[optype as usize].1
}

/// Set `curwin.w_p_lbr` (`'linebreak'`) to `false` and take care of
/// side effects (`reset_lbr`).
///
/// @return `true` if `'linebreak'` was set (and thus actually reset).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT`.
pub unsafe fn reset_lbr() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin };
    if curwin.w_onebuf_opt.wo_lbr == 0 {
        return false;
    }
    // changing 'linebreak' may require w_virtcol to be updated
    curwin.w_onebuf_opt.wo_lbr = 0;
    curwin.w_valid &= !(i32::from(crate::buffer_defs::w_valid::VALID_WROW)
        | i32::from(crate::buffer_defs::w_valid::VALID_WCOL)
        | i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL));
    true
}

/// Restore `curwin.w_p_lbr` (`'linebreak'`) and take care of side
/// effects (`restore_lbr`).
///
/// # Safety
/// Same as [`reset_lbr`].
pub unsafe fn restore_lbr(lbr_saved: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin };
    if curwin.w_onebuf_opt.wo_lbr != 0 || !lbr_saved {
        return;
    }

    // changing 'linebreak' may require w_virtcol to be updated
    curwin.w_onebuf_opt.wo_lbr = 1;
    curwin.w_valid &= !(i32::from(crate::buffer_defs::w_valid::VALID_WROW)
        | i32::from(crate::buffer_defs::w_valid::VALID_WCOL)
        | i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL));
}

/// Clear all fields of `oap` back to their zero/default values
/// (`clear_oparg`, `CLEAR_POINTER(oap)` in the original).
///
/// `OpargT::default()` is a genuine byte-for-byte equivalent of
/// zeroing the whole struct: every field is a plain scalar/`Copy`
/// type, and [`crate::normal_defs::MotionType`]'s own `Default`
/// (`CharWise`) is verified to be the C enum's own `0` variant (see
/// `normal_defs.rs`'s own test asserting this).
pub fn clear_oparg(oap: &mut crate::normal_defs::OpargT) {
    *oap = crate::normal_defs::OpargT::default();
}

/// Insert `c` at the cursor in Replace mode, then step back onto the
/// character just written (`replace_character`).
///
/// `State` is switched to `MODE_REPLACE` only for the duration of the
/// insert and restored afterwards, so the caller's own mode is
/// unaffected.
///
/// # Deferred boundary
/// Switching into Replace mode is exactly what makes
/// [`crate::change::ins_char`]'s own Replace branch reachable, and
/// that branch needs `replace_push`/`replace_push_nul`, not yet
/// translated - so this function panics there today. The translation
/// itself is complete and faithful; only the downstream dependency is
/// missing.
///
/// # Safety
/// Forwarded from [`crate::change::ins_char`]/
/// [`crate::cursor::dec_cursor`]'s own safety docs; also touches
/// `crate::globals::GLOBALS.State`.
pub unsafe fn replace_character(c: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let n = unsafe { crate::globals::GLOBALS.get_mut() }.State;

    // SAFETY: as above.
    unsafe { crate::globals::GLOBALS.get_mut() }.State =
        crate::state_defs::mode::REPLACE as i32;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::change::ins_char(c) };
    // SAFETY: as above.
    unsafe { crate::globals::GLOBALS.get_mut() }.State = n;
    // Backup to the replaced character.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::cursor::dec_cursor() };
}

/// The `'operatorfunc'` callback (`opfunc_cb`, a file-static
/// `Callback`). Nothing in this crate can currently set a real value
/// here: doing so needs `option_set_callback_func`, itself needing
/// `eval_expr`/the full `:set`-parsing `Callback` machinery, none
/// translated - so this stays [`crate::eval::typval_defs::Callback::None`]
/// forever today, matching every real, unconfigured session before
/// `'operatorfunc'` is ever assigned.
static OPFUNC_CB: crate::globals::GlobalCell<crate::eval::typval_defs::Callback> =
    crate::globals::GlobalCell::new(crate::eval::typval_defs::Callback::None);

/// Release the global `'operatorfunc'` callback
/// (`free_operatorfunc_option`).
///
/// The original guards this in `#ifdef EXITFREE`, i.e. it exists only
/// to hand memory back cleanly on exit so leak checkers stay quiet.
/// `OPFUNC_CB` is `Callback::None` in every session this crate can
/// currently build (see its own doc comment), so today this is a
/// well-defined no-op - but it is translated in full rather than
/// stubbed, so it already does the right thing the moment
/// `'operatorfunc'` becomes settable.
///
/// # Safety
/// Same as [`crate::eval::typval::callback_free`]: `OPFUNC_CB` must
/// hold a callback whose referent is still live and not aliased.
pub unsafe fn free_operatorfunc_option() {
    // SAFETY: forwarded from this function's own safety doc - the
    // GlobalCell is not aliased across this call.
    let cb = unsafe { OPFUNC_CB.get_mut() };
    crate::eval::typval::callback_free(cb);
}

/// Mark the global `'operatorfunc'` callback with `copy_id` so that it
/// is not garbage collected (`set_ref_in_opfunc`).
///
/// # Safety
/// Same as [`crate::eval::eval::set_ref_in_callback`].
pub unsafe fn set_ref_in_opfunc(copy_id: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let cb = unsafe { &*OPFUNC_CB.as_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::eval::set_ref_in_callback(cb, copy_id, std::ptr::null_mut(), std::ptr::null_mut()) }
}

/// Whether `cap.cmdchar` started a `:`-command-line-shaped operator
/// invocation (`is_ex_cmdchar`) - either a real `:` (e.g. `:'<,'>d`)
/// or a `<Cmd>` mapping (`K_COMMAND`).
///
/// `static` in the original; kept private-but-reachable here
/// (`#[allow(dead_code)]`) since its only real caller, `op_function`,
/// is not translated yet - matches `marktree.rs`'s own `itr_eq`
/// precedent for a small, simple function with no design freedom of
/// its own, translated ahead of its real caller.
#[allow(dead_code)]
#[must_use]
fn is_ex_cmdchar(cap: &crate::normal_defs::CmdargT) -> bool {
    cap.cmdchar == i32::from(b':') || cap.cmdchar == crate::keycodes_defs::K_COMMAND
}

/// If `process` is `true` and `line` begins with a comment leader
/// (possibly after some white space), returns the byte offset into
/// `line` right after it (`0` if unchanged). Also reports whether the
/// current line ends with an unclosed comment (`skip_comment`).
///
/// @param line - line to be processed
/// @param process - if `false`, only checks whether the line ends
///   with an unclosed comment
/// @param include_space - whether to skip space following the comment
///   leader
///
/// @return `(new_offset, is_comment)`.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` (touched transitively via
/// `crate::change::get_last_leader_offset`/`get_leader_len`, and
/// directly here to re-scan `b_p_com`'s own flags).
#[allow(dead_code)]
pub unsafe fn skip_comment(line: &[u8], process: bool, include_space: bool) -> (usize, bool) {
    use crate::option_vars::COM_END;

    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    let com: &[u8] = curbuf.b_p_com.as_deref().unwrap_or(&[]);

    let mut comment_flags: usize = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let leader_offset =
        unsafe { crate::change::get_last_leader_offset(line, Some(&mut comment_flags)) };

    let mut is_comment = false;
    if leader_offset.is_some() {
        // Let's check whether the line ends with an unclosed comment.
        // If the last comment leader has COM_END in flags, there's no
        // comment.
        let mut cf = comment_flags;
        while crate::change::byte_at(com, cf) != 0 {
            if crate::change::byte_at(com, cf) == COM_END || crate::change::byte_at(com, cf) == b':' {
                break;
            }
            cf += 1;
        }
        if crate::change::byte_at(com, cf) != COM_END {
            is_comment = true;
        }
    }

    if !process {
        return (0, is_comment);
    }

    // SAFETY: forwarded from this function's own safety doc.
    let lead_len =
        unsafe { crate::change::get_leader_len(line, Some(&mut comment_flags), false, include_space) };

    if lead_len == 0 {
        return (0, is_comment);
    }

    // Find COM_END or a colon, whichever comes first.
    let mut cf = comment_flags;
    while crate::change::byte_at(com, cf) != 0 {
        if crate::change::byte_at(com, cf) == COM_END || crate::change::byte_at(com, cf) == b':' {
            break;
        }
        cf += 1;
    }

    // If we found a colon, we are not processing a line starting with
    // the closing part of a three-part comment - that's good, we
    // don't want to remove those (it would be annoying).
    if crate::change::byte_at(com, cf) == b':' || crate::change::byte_at(com, cf) == 0 {
        return (lead_len, is_comment);
    }
    (0, is_comment)
}

/// Return the tabstop width at `index` (1-based) of the variable
/// tabstop array `vts` (`get_vts`). If `index` exceeds the array's own
/// length, the LAST tabstop width is returned.
///
/// `vts` mirrors this crate's own established "no leading count
/// element" convention (`indent.rs`'s `tabstop_padding`) - a plain
/// slice instead of the original's own `vts_array[0]`-prefixed C
/// array. Callers must ensure `vts` is non-empty whenever `index >= 1`
/// is reachable (matching the original's own implicit assumption via
/// `vts_array[0]` indexing) - `index < 1` never touches `vts` at all.
fn get_vts(vts: &[crate::pos_defs::ColnrT], index: i32) -> i32 {
    if index < 1 {
        0
    } else if (index as usize) <= vts.len() {
        vts[(index - 1) as usize]
    } else {
        vts[vts.len() - 1]
    }
}

/// Return the sum of all the tabstops through the `index`-th, in the
/// variable tabstop array `vts` (`get_vts_sum`). Same `vts` convention
/// as [`get_vts`].
fn get_vts_sum(vts: &[crate::pos_defs::ColnrT], index: i32) -> i32 {
    let mut sum: i32 = 0;
    let mut i: i32 = 1;
    while i <= index && (i as usize) <= vts.len() {
        sum += vts[(i - 1) as usize];
        i += 1;
    }
    if i <= index {
        sum += vts[vts.len() - 1] * (index - vts.len() as i32);
    }
    sum
}

/// Compute the new indent for shifting the current line by `amount`
/// `'shiftwidth'`s, when a fixed width (`'shiftwidth'`/`'tabstop'`, not
/// `'vartabstop'`) determines the shift size (`get_new_sw_indent`).
///
/// @param left true if shift is to the left
/// @param round true if the new indent is to be rounded to a tabstop
/// @param amount number of shifts
///
/// # Safety
/// Same as `crate::indent::get_indent`.
#[allow(dead_code)]
unsafe fn get_new_sw_indent(left: bool, round: bool, amount: i64, sw_val: i64) -> i64 {
    // SAFETY: forwarded from this function's own safety doc.
    let mut count: i64 = i64::from(unsafe { crate::indent::get_indent() });

    if round {
        let mut i: i64 = i64::from(crate::math::trim_to_int(count / sw_val));
        let j: i64 = i64::from(crate::math::trim_to_int(count % sw_val));
        let mut amount = amount;
        if j != 0 && left {
            amount -= 1;
        }
        if left {
            i = (i - amount).max(0);
        } else {
            i += amount;
        }
        count = i * sw_val;
    } else if left {
        count = (count - sw_val * amount).max(0);
    } else {
        count += sw_val * amount;
    }

    count
}

/// Compute the new indent for shifting the current line by `amount`
/// `'shiftwidth'`s, when `'vartabstop'` (variable tabstops) determines
/// the shift size (`get_new_vts_indent`). Same `vts` convention as
/// [`get_vts`].
///
/// # Safety
/// Same as `crate::indent::get_indent`.
#[allow(dead_code)]
unsafe fn get_new_vts_indent(
    left: bool,
    round: bool,
    amount: i32,
    vts: &[crate::pos_defs::ColnrT],
) -> i64 {
    // SAFETY: forwarded from this function's own safety doc.
    let indent: i64 = i64::from(unsafe { crate::indent::get_indent() });
    let mut vtsi: i32 = 0;
    let mut vts_indent: i32 = 0;
    let mut ts: i32 = 0;

    // Find the tabstop at or to the left of the current indent.
    while i64::from(vts_indent) <= indent {
        vtsi += 1;
        ts = get_vts(vts, vtsi);
        vts_indent += ts;
    }
    vts_indent -= ts;
    vtsi -= 1;

    // Extra indent spaces to the right of the tabstop.
    let offset: i64 = indent - i64::from(vts_indent);

    if round {
        if left {
            if offset == 0 {
                i64::from(get_vts_sum(vts, vtsi - amount))
            } else {
                i64::from(get_vts_sum(vts, vtsi - (amount - 1)))
            }
        } else {
            i64::from(get_vts_sum(vts, vtsi + amount))
        }
    } else if left {
        if amount > vtsi {
            0
        } else {
            i64::from(get_vts_sum(vts, vtsi - amount)) + offset
        }
    } else {
        i64::from(get_vts_sum(vts, vtsi + amount)) + offset
    }
}

/// If `oap.inclusive`, extend `oap.end.col` to the LAST byte of the
/// (possibly multi-byte, possibly composing-character) character it
/// currently points into (`mb_adjust_opend`) - operators normally work
/// on a single base character's own leading byte; this fixes up the
/// end position so a multi-byte character isn't only partially
/// included.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` whose `b_ml.ml_mfp`, if non-null, points to a live
/// `MemfileT` (forwarded from `crate::memline::ml_get`'s own safety
/// doc).
#[allow(dead_code)]
pub unsafe fn mb_adjust_opend(oap: &mut crate::normal_defs::OpargT) {
    if !oap.inclusive {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get(oap.end.lnum) };
    let col = oap.end.col as usize;
    if line.get(col).copied().unwrap_or(0) != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let head_off = unsafe { crate::mbyte::utf_head_off(&line, col) } as usize;
        let new_start = col - head_off;
        // SAFETY: forwarded from this function's own safety doc.
        let char_len = unsafe { crate::mbyte::utfc_ptr2len(&line[new_start..]) };
        oap.end.col = new_start as crate::pos_defs::ColnrT + char_len - 1;
    }
}

/// Compute a block-wise operator's virtual-column extent and convert
/// it into real character positions (`get_op_vcol`, a `static` helper
/// in `ops.c`). Only meaningful in blockwise-Visual mode
/// (`Visual.mode == Ctrl_V`); every other mode leaves `oap` untouched.
///
/// Its only real caller, `do_pending_operator` (the "an operator
/// finished, act on it" dispatcher), is not translated - kept
/// `#[allow(dead_code)]` for now, matching this file's own established
/// "translate ahead of a real caller" precedent (`is_ex_cmdchar`,
/// `skip_comment`).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` whose own `w_buffer` is also valid.
#[allow(dead_code)]
unsafe fn get_op_vcol(
    oap: &mut crate::normal_defs::OpargT,
    redo_visual_vcol: crate::pos_defs::ColnrT,
    initial: bool,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let w_view_width = unsafe { (*curwin).w_view_width };

    // SAFETY: forwarded from this function's own safety doc.
    let visual_mode = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.mode;
    if visual_mode != i32::from(crate::ascii_defs::CTRL_V) || (!initial && oap.end.col < w_view_width) {
        return;
    }

    oap.motion_type = crate::normal_defs::MotionType::BlockWise;

    // Prevent from moving onto a trail byte.
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *(*curwin).w_buffer };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::mark::mark_mb_adjustpos(buf, &mut oap.end) };

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::plines::getvvcol(
            curwin,
            &mut oap.start,
            Some(&mut oap.start_vcol),
            None,
            Some(&mut oap.end_vcol),
            0,
        );
    }

    // SAFETY: forwarded from this function's own safety doc.
    let redo_busy = unsafe { crate::globals::GLOBALS.get_mut() }.Visual.redo_busy;
    if !redo_busy {
        let mut start: crate::pos_defs::ColnrT = 0;
        let mut end: crate::pos_defs::ColnrT = 0;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            crate::plines::getvvcol(curwin, &mut oap.end, Some(&mut start), None, Some(&mut end), 0);
        }

        oap.start_vcol = oap.start_vcol.min(start);
        if end > oap.end_vcol {
            // SAFETY: forwarded from this function's own safety doc.
            let sel_is_exclusive = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
                .p_sel
                .as_deref()
                .and_then(<[u8]>::first)
                == Some(&b'e');
            if initial && sel_is_exclusive && start >= 1 && start > oap.end_vcol {
                oap.end_vcol = start - 1;
            } else {
                oap.end_vcol = end;
            }
        }
    }

    // If '$' was used, get oap.end_vcol from the longest line.
    // SAFETY: forwarded from this function's own safety doc.
    let w_curswant = unsafe { (*curwin).w_curswant };
    if w_curswant == crate::pos_defs::MAXCOL {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*curwin).w_cursor.col = crate::pos_defs::MAXCOL };
        oap.end_vcol = 0;
        let mut lnum = oap.start.lnum;
        while lnum <= oap.end.lnum {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*curwin).w_cursor.lnum = lnum };
            let mut end: crate::pos_defs::ColnrT = 0;
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                crate::plines::getvvcol(curwin, &mut (*curwin).w_cursor, None, None, Some(&mut end), 0);
            }
            oap.end_vcol = oap.end_vcol.max(end);
            lnum += 1;
        }
    } else if redo_busy {
        oap.end_vcol = oap.start_vcol + redo_visual_vcol - 1;
    }

    // Correct oap.end.col and oap.start.col to be the upper-left and
    // lower-right corner of the block area.
    //
    // (Actually, this does convert column positions into character
    // positions.)
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*curwin).w_cursor.lnum = oap.end.lnum };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::cursor::coladvance(curwin, oap.end_vcol) };
    // SAFETY: forwarded from this function's own safety doc.
    oap.end = unsafe { (*curwin).w_cursor };

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*curwin).w_cursor = oap.start };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::cursor::coladvance(curwin, oap.start_vcol) };
    // SAFETY: forwarded from this function's own safety doc.
    oap.start = unsafe { (*curwin).w_cursor };
}

/// When the cursor is on the NUL past the end of the line and it
/// should not be there, move it left (`adjust_cursor_eol`).
///
/// Note a real, verified upstream quirk found while translating this:
/// the `adj_cursor` guard REQUIRES `(cur_ve_flags &
/// opt_ve_flag::ALL) == 0` - but `opt_ve_flag::ALL`/`BLOCK`/`INSERT`
/// all share bit `0x04` (`BLOCK = 0x05`, `INSERT = 0x06`, both
/// `0x04`-inclusive), so `cur_ve_flags == opt_ve_flag::ALL` exactly
/// (0x04) already implies that bit is set, making the guard's OWN
/// `(cur_ve_flags & opt_ve_flag::ALL) == 0` check false whenever
/// `cur_ve_flags == opt_ve_flag::ALL` - i.e. the function can NEVER
/// reach its own later `if cur_ve_flags == opt_ve_flag::ALL` branch.
/// This is a genuine property of the real upstream C source (not a
/// translation artifact - verified against the real `kOptVeFlagAll`/
/// `kOptVeFlagBlock`/`kOptVeFlagInsert` values in
/// `option_vars.generated.h`), so the branch is translated faithfully
/// as dead code rather than removed, matching this crate's mission to
/// translate literally, quirks included.
///
/// Calls `crate::cursor::gchar_cursor()` lazily, only when
/// `w_cursor.col > 0` (matching the original's own short-circuiting
/// `&&` chain exactly) - NOT unconditionally, since it needs a valid
/// buffer read that the original itself never performs when the
/// cursor is already at column 0.
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` whose own `w_buffer` is also valid.
#[allow(dead_code)]
pub unsafe fn adjust_cursor_eol() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let cur_ve_flags = unsafe { crate::option::get_ve_flags(&*curwin) };

    // SAFETY: forwarded from this function's own safety doc.
    let w_cursor_col = unsafe { (*curwin).w_cursor.col };
    // SAFETY: forwarded from this function's own safety doc.
    let (state, restart_edit) = {
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        (g.State, g.restart_edit)
    };

    // SAFETY: forwarded from this function's own safety doc.
    let adj_cursor = w_cursor_col > 0
        && unsafe { crate::cursor::gchar_cursor() } == 0
        && (cur_ve_flags & crate::option_vars::opt_ve_flag::ONEMORE) == 0
        && (cur_ve_flags & crate::option_vars::opt_ve_flag::ALL) == 0
        && !(restart_edit != 0 || (state & crate::state_defs::mode::INSERT as i32) != 0);

    if !adj_cursor {
        return;
    }

    // Put the cursor on the last character in the line.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::cursor::dec_cursor() };

    if cur_ve_flags == crate::option_vars::opt_ve_flag::ALL {
        let mut scol: crate::pos_defs::ColnrT = 0;
        let mut ecol: crate::pos_defs::ColnrT = 0;

        // Coladd is set to the width of the last character.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            crate::plines::getvcol(
                curwin,
                &mut (*curwin).w_cursor,
                Some(&mut scol),
                None,
                Some(&mut ecol),
                0,
            );
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*curwin).w_cursor.coladd = ecol - scol + 1 };
    }
}

/// Count words/chars/bytes in `line` (`line_count_info`). Return
/// value is the byte count; word count for the line is added to
/// `*wc`. Char count is added to `*cc`.
///
/// Only examines the first `limit` characters in the line, stopping
/// if it encounters the end of the line (matching this crate's own
/// "line slices include their own trailing NUL" convention, e.g.
/// `crate::memline::ml_get`'s return value). In that case, `eol_size`
/// is added to the character count to account for the size of the
/// EOL character.
#[allow(dead_code)]
fn line_count_info(line: &[u8], wc: &mut i64, cc: &mut i64, limit: i64, eol_size: i32) -> i64 {
    let mut i: i64 = 0;
    let mut words: i64 = 0;
    let mut chars: i64 = 0;
    let mut is_word = false;

    while i < limit && line.get(i as usize).copied().unwrap_or(0) != 0 {
        let c = line[i as usize];
        if is_word {
            if ascii_isspace(i32::from(c)) {
                words += 1;
                is_word = false;
            }
        } else if !ascii_isspace(i32::from(c)) {
            is_word = true;
        }
        chars += 1;
        // SAFETY: `line[i as usize]` is confirmed non-NUL by the loop
        // condition above, so `utfc_ptr2len` always returns >= 1 here
        // (no infinite-loop risk from a zero advance).
        i += i64::from(unsafe { crate::mbyte::utfc_ptr2len(&line[i as usize..]) });
    }

    if is_word {
        words += 1;
    }
    *wc += words;

    // Add eol_size if the end of line was reached before hitting limit.
    if i < limit && line.get(i as usize).copied().unwrap_or(0) == 0 {
        i += i64::from(eol_size);
        chars += i64::from(eol_size);
    }
    *cc += chars;
    i
}

/// Replace the single byte at `lp` with `c` (`pbyte`).
///
/// The column is clamped when it lies past the end of the line, so a
/// stale position cannot write out of bounds.
///
/// # Adaptation
///
/// The original writes straight through the `char *` that
/// `ml_get_buf_mut` hands back. In this crate `ml_get_buf_mut` returns
/// an owned copy rather than a live pointer into the memline, so the
/// byte is changed in that copy and stored back with
/// `ml_replace_buf_len`. Same observable result, and the same forced
/// adaptation already documented for `change::del_bytes`.
///
/// # Safety
/// `GLOBALS.curbuf` must be valid, with a live memline holding
/// `lp.lnum`.
pub unsafe fn pbyte(mut lp: crate::pos_defs::PosT, c: i32) {
    debug_assert!(c <= i32::from(u8::MAX));
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    let mut p = unsafe { crate::memline::ml_get_buf_mut(&mut *curbuf, lp.lnum) };
    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { &*curbuf }.b_ml.ml_line_textlen;

    // Safety check.
    if lp.col >= len {
        lp.col = if len > 1 { len - 2 } else { 0 };
    }
    if let Some(slot) = p.get_mut(lp.col as usize) {
        *slot = c as u8;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let _ = unsafe { crate::memline::ml_replace_buf_len(&mut *curbuf, lp.lnum, &p) };

    // SAFETY: reading a plain scalar global.
    if *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() } == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            crate::extmark::extmark_splice_cols(
                &mut *curbuf,
                lp.lnum - 1,
                lp.col,
                1,
                1,
                crate::extmark_defs::ExtmarkOp::Undo,
            );
        }
    }
}

/// Swap the case of, or rot13, the character at `pos`, according to
/// `op_type` (`swapchar`).
///
/// Returns `true` when the character actually changed. Only ASCII is
/// rot13'd; a non-ASCII character is left alone for that operator.
///
/// # Safety
/// Same as [`pbyte`], plus `crate::change::del_bytes`'s own.
pub unsafe fn swapchar(op_type: OpType, pos: &crate::pos_defs::PosT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let c = unsafe { crate::memline::gchar_pos(pos) };

    // Only do rot13 encoding for ASCII characters.
    if c >= 0x80 && op_type == OpType::Rot13 {
        return false;
    }

    /// `ROT13(c, a)` from `ops.c`.
    fn rot13(c: i32, a: i32) -> i32 {
        ((c - a) + 13) % 26 + a
    }

    let mut nc = c;
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::mbyte::mb_islower(c) } {
        if op_type == OpType::Rot13 {
            nc = rot13(c, i32::from(b'a'));
        } else if op_type != OpType::Lower {
            // SAFETY: forwarded from this function's own safety doc.
            nc = unsafe { crate::mbyte::mb_toupper(c) };
        }
    // SAFETY: forwarded from this function's own safety doc.
    } else if unsafe { crate::mbyte::mb_isupper(c) } {
        if op_type == OpType::Rot13 {
            nc = rot13(c, i32::from(b'A'));
        } else if op_type != OpType::Upper {
            // SAFETY: forwarded from this function's own safety doc.
            nc = unsafe { crate::mbyte::mb_tolower(c) };
        }
    }

    if nc == c {
        return false;
    }

    if c >= 0x80 || nc >= 0x80 {
        // SAFETY: forwarded from this function's own safety doc.
        let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        // SAFETY: forwarded from this function's own safety doc.
        let sp = unsafe { &*curwin }.w_cursor;

        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *curwin }.w_cursor = *pos;
        // Don't use del_char(): it also removes composing chars.
        // SAFETY: forwarded from this function's own safety doc.
        let p = unsafe { crate::cursor::get_cursor_pos_ptr() };
        let n = crate::mbyte::utf_ptr2len(&p);
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::change::del_bytes(n, false, false) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::change::ins_char(nc) };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *curwin }.w_cursor = sp;
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { pbyte(*pos, nc) };
    }
    true
}

/// Shift the current line one `'shiftwidth'` right or left
/// (`shift_line`).
///
/// The shift size comes from `'shiftwidth'`, falling back to
/// `'tabstop'` when that is zero, or to `'vartabstop'` when that is
/// defined too.
///
/// # Scope
///
/// The `State & VREPLACE_FLAG` branch is `unimplemented!()`, behind a
/// real guard that is unreachable today: it needs `indent.c`'s
/// `change_indent`, and nothing translated can enter virtual Replace
/// mode - there is no `edit()` loop yet - so `State` never carries
/// that flag in a real session.
///
/// # Safety
/// Same as [`crate::indent::set_indent`].
pub unsafe fn shift_line(left: bool, round: bool, amount: i32, call_changed_bytes: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    let (sw_val, ts_val, vts) = {
        let b = unsafe { &*curbuf };
        (b.b_p_sw, b.b_p_ts, b.b_p_vts_array.clone())
    };
    let vts = vts.as_deref();

    let count = if sw_val != 0 {
        // 'shiftwidth' is not zero; use it as the shift size.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { get_new_sw_indent(left, round, i64::from(amount), sw_val) }
    } else if vts.is_none_or(<[crate::pos_defs::ColnrT]>::is_empty) {
        // 'shiftwidth' is zero and 'vartabstop' is empty; use
        // 'tabstop' as the shift size.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { get_new_sw_indent(left, round, i64::from(amount), ts_val) }
    } else {
        // 'shiftwidth' is zero and 'vartabstop' is defined; use
        // 'vartabstop' to determine the new indent.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { get_new_vts_indent(left, round, amount, vts.unwrap_or(&[])) }
    };

    // Set the new indent.
    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { crate::globals::GLOBALS.get_mut() }.State as u32;
    if state & crate::state_defs::mode::VREPLACE_FLAG != 0 {
        unimplemented!(
            "virtual Replace mode needs indent.c's change_indent, not yet translated; \
             unreachable while nothing can enter Replace mode"
        );
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::indent::set_indent(
            crate::math::trim_to_int(count),
            if call_changed_bytes {
                crate::indent::sin_flag::CHANGED
            } else {
                0
            },
        );
    }
}

/// Swap the case of, or rot13, `length` bytes starting at `pos`
/// (`swapchars`).
///
/// Returns `true` when any character actually changed. `length` counts
/// bytes, not characters, so a multi-byte character consumes its whole
/// width from the budget.
///
/// # Safety
/// Same as [`swapchar`].
pub unsafe fn swapchars(op_type: OpType, pos: &mut crate::pos_defs::PosT, length: i32) -> bool {
    let mut did_change = false;

    let mut todo = length;
    while todo > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let p = unsafe { crate::memline::ml_get_pos(pos) };
        // SAFETY: forwarded from this function's own safety doc.
        let len = unsafe { crate::mbyte::utfc_ptr2len(&p) };

        // We're counting bytes, not characters.
        if len > 0 {
            todo -= len - 1;
        }
        // SAFETY: forwarded from this function's own safety doc.
        did_change |= unsafe { swapchar(op_type, pos) };
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::memline::inc(pos) } == -1 {
            // At the end of the file.
            break;
        }
        todo -= 1;
    }

    did_change
}

/// Get the byte count of a buffer region, end-exclusive
/// (`get_region_bytecount`).
///
/// # Safety
/// Same as `crate::memline::ml_get_buf_len`.
#[allow(dead_code)]
pub unsafe fn get_region_bytecount(
    buf: &mut crate::buffer_defs::BufT,
    start_lnum: crate::pos_defs::LinenrT,
    end_lnum: crate::pos_defs::LinenrT,
    start_col: crate::pos_defs::ColnrT,
    end_col: crate::pos_defs::ColnrT,
) -> crate::extmark_defs::BcountT {
    let max_lnum = buf.b_ml.ml_line_count;
    if start_lnum > max_lnum {
        return 0;
    }
    if start_lnum == end_lnum {
        return (end_col - start_col) as crate::extmark_defs::BcountT;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let mut deleted_bytes: crate::extmark_defs::BcountT =
        (unsafe { crate::memline::ml_get_buf_len(buf, start_lnum) } - start_col + 1)
            as crate::extmark_defs::BcountT;

    let mut i: crate::pos_defs::LinenrT = 1;
    while i < end_lnum - start_lnum {
        if start_lnum + i > max_lnum {
            return deleted_bytes;
        }
        // SAFETY: forwarded from this function's own safety doc.
        deleted_bytes += (unsafe { crate::memline::ml_get_buf_len(buf, start_lnum + i) } + 1)
            as crate::extmark_defs::BcountT;
        i += 1;
    }
    if end_lnum > max_lnum {
        return deleted_bytes;
    }
    deleted_bytes + end_col as crate::extmark_defs::BcountT
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::WinT;

    #[test]
    fn get_op_type_single_char_operators() {
        assert_eq!(get_op_type(i32::from(b'd'), 0), OpType::Delete);
        assert_eq!(get_op_type(i32::from(b'<'), 0), OpType::Lshift);
        assert_eq!(get_op_type(i32::from(b':'), 0), OpType::Colon);
    }

    #[test]
    fn get_op_type_two_char_operators() {
        assert_eq!(get_op_type(i32::from(b'g'), i32::from(b'~')), OpType::Tilde);
        assert_eq!(get_op_type(i32::from(b'g'), i32::from(b'q')), OpType::Format);
        assert_eq!(get_op_type(i32::from(b'z'), i32::from(b'f')), OpType::Fold);
    }

    #[test]
    fn get_op_type_special_cased_replace_and_tilde() {
        // 'r'/'~' are special-cased BEFORE the table scan, ignoring
        // char2 entirely (matches the original's own early returns).
        assert_eq!(get_op_type(i32::from(b'r'), i32::from(b'X')), OpType::Replace);
        assert_eq!(get_op_type(i32::from(b'~'), i32::from(b'Y')), OpType::Tilde);
    }

    #[test]
    fn get_op_type_special_cased_nr_add_and_sub() {
        assert_eq!(
            get_op_type(i32::from(b'g'), i32::from(crate::ascii_defs::CTRL_A)),
            OpType::NrAdd
        );
        assert_eq!(
            get_op_type(i32::from(b'g'), i32::from(crate::ascii_defs::CTRL_X)),
            OpType::NrSub
        );
    }

    #[test]
    fn get_op_type_special_cased_zy_yank() {
        assert_eq!(get_op_type(i32::from(b'z'), i32::from(b'y')), OpType::Yank);
    }

    #[test]
    #[should_panic(expected = "invalid operator name")]
    fn get_op_type_panics_on_unrecognized_chars() {
        let _ = get_op_type(i32::from(b'Q'), i32::from(b'Q'));
    }

    #[test]
    fn get_op_char_and_get_extra_op_char_roundtrip_the_table() {
        assert_eq!(get_op_char(OpType::Delete), b'd');
        assert_eq!(get_extra_op_char(OpType::Delete), 0);
        assert_eq!(get_op_char(OpType::Tilde), b'g');
        assert_eq!(get_extra_op_char(OpType::Tilde), b'~');
        assert_eq!(get_op_char(OpType::NrAdd), crate::ascii_defs::CTRL_A);
    }

    #[test]
    fn op_on_lines_true_for_line_operators() {
        assert!(op_on_lines(OpType::Lshift));
        assert!(op_on_lines(OpType::Colon));
        assert!(op_on_lines(OpType::Foldopen));
    }

    #[test]
    fn op_on_lines_false_for_non_line_operators() {
        assert!(!op_on_lines(OpType::Delete));
        assert!(!op_on_lines(OpType::Yank));
        assert!(!op_on_lines(OpType::Fold));
    }

    #[test]
    fn op_is_change_true_for_change_operators() {
        assert!(op_is_change(OpType::Delete));
        assert!(op_is_change(OpType::Tilde));
        assert!(op_is_change(OpType::NrAdd));
    }

    #[test]
    fn op_is_change_false_for_non_change_operators() {
        assert!(!op_is_change(OpType::Nop));
        assert!(!op_is_change(OpType::Yank));
        assert!(!op_is_change(OpType::Colon));
        assert!(!op_is_change(OpType::Fold));
    }

    #[test]
    fn opchars_and_op_type_order_have_matching_lengths() {
        assert_eq!(OPCHARS.len(), OP_TYPE_ORDER.len());
        assert_eq!(OPCHARS.len(), 30);
    }

    /// Points `GLOBALS.curwin` at `win` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime.
    struct CurwinGuard {
        previous: *mut crate::buffer_defs::WinT,
    }

    impl CurwinGuard {
        fn set(new_curwin: *mut crate::buffer_defs::WinT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = new_curwin;
            CurwinGuard { previous }
        }
    }

    impl Drop for CurwinGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.previous;
        }
    }

    #[test]
    fn reset_lbr_false_and_untouched_when_linebreak_not_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT {
            w_valid: i32::from(crate::buffer_defs::w_valid::VALID_WROW)
                | i32::from(crate::buffer_defs::w_valid::VALID_WCOL)
                | i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL),
            ..Default::default()
        };
        win.w_onebuf_opt.wo_lbr = 0;
        let _guard = CurwinGuard::set(&mut win as *mut crate::buffer_defs::WinT);

        assert!(!unsafe { reset_lbr() });
        assert_eq!(win.w_onebuf_opt.wo_lbr, 0);
        // w_valid untouched - the early return happens before it's read.
        assert_eq!(
            win.w_valid,
            i32::from(crate::buffer_defs::w_valid::VALID_WROW)
                | i32::from(crate::buffer_defs::w_valid::VALID_WCOL)
                | i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL)
        );
    }

    #[test]
    fn reset_lbr_true_clears_linebreak_and_valid_bits() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT {
            w_valid: i32::from(crate::buffer_defs::w_valid::VALID_WROW)
                | i32::from(crate::buffer_defs::w_valid::VALID_WCOL)
                | i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL)
                | i32::from(crate::buffer_defs::w_valid::VALID_BOTLINE), // must survive
            ..Default::default()
        };
        win.w_onebuf_opt.wo_lbr = 1;
        let _guard = CurwinGuard::set(&mut win as *mut crate::buffer_defs::WinT);

        assert!(unsafe { reset_lbr() });
        assert_eq!(win.w_onebuf_opt.wo_lbr, 0);
        assert_eq!(win.w_valid, i32::from(crate::buffer_defs::w_valid::VALID_BOTLINE));
    }

    #[test]
    fn restore_lbr_noop_when_not_saved() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_lbr = 0;
        let _guard = CurwinGuard::set(&mut win as *mut crate::buffer_defs::WinT);

        unsafe { restore_lbr(false) };
        assert_eq!(win.w_onebuf_opt.wo_lbr, 0); // untouched
    }

    #[test]
    fn restore_lbr_noop_when_linebreak_already_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT {
            w_valid: i32::from(crate::buffer_defs::w_valid::VALID_WROW), // must survive
            ..Default::default()
        };
        win.w_onebuf_opt.wo_lbr = 1;
        let _guard = CurwinGuard::set(&mut win as *mut crate::buffer_defs::WinT);

        unsafe { restore_lbr(true) };
        assert_eq!(win.w_onebuf_opt.wo_lbr, 1);
        assert_eq!(win.w_valid, i32::from(crate::buffer_defs::w_valid::VALID_WROW));
    }

    #[test]
    fn restore_lbr_sets_linebreak_and_clears_valid_bits_when_saved() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT {
            w_valid: i32::from(crate::buffer_defs::w_valid::VALID_WROW)
                | i32::from(crate::buffer_defs::w_valid::VALID_WCOL)
                | i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL)
                | i32::from(crate::buffer_defs::w_valid::VALID_BOTLINE), // must survive
            ..Default::default()
        };
        win.w_onebuf_opt.wo_lbr = 0;
        let _guard = CurwinGuard::set(&mut win as *mut crate::buffer_defs::WinT);

        unsafe { restore_lbr(true) };
        assert_eq!(win.w_onebuf_opt.wo_lbr, 1);
        assert_eq!(win.w_valid, i32::from(crate::buffer_defs::w_valid::VALID_BOTLINE));
    }

    #[test]
    fn clear_oparg_zeroes_every_field() {
        let mut oap = crate::normal_defs::OpargT {
            op_type: 5,
            regname: i32::from(b'a'),
            motion_type: crate::normal_defs::MotionType::LineWise,
            motion_force: i32::from(b'V'),
            use_reg_one: true,
            inclusive: true,
            end_adjusted: true,
            restore_cursor: true,
            line_count: 42,
            empty: true,
            is_visual: true,
            start_vcol: 3,
            end_vcol: 7,
            prev_opcount: 2,
            prev_count0: 9,
            excl_tr_ws: true,
            ..Default::default()
        };
        clear_oparg(&mut oap);
        assert_eq!(oap.op_type, 0);
        assert_eq!(oap.regname, 0);
        assert_eq!(oap.motion_type, crate::normal_defs::MotionType::CharWise);
        assert!(!oap.use_reg_one);
        assert!(!oap.inclusive);
        assert!(!oap.is_visual);
        assert_eq!(oap.line_count, 0);
        assert_eq!(oap.prev_opcount, 0);
        assert!(!oap.excl_tr_ws);
    }

    #[test]
    fn set_ref_in_opfunc_is_always_false_since_opfunc_cb_stays_none() {
        // Nothing in this crate can populate OPFUNC_CB with a real
        // callback yet (needs option_set_callback_func) - it always
        // stays Callback::None, matching a real, unconfigured session.
        assert!(!unsafe { set_ref_in_opfunc(1) });
    }

    #[test]
    fn free_operatorfunc_option_is_a_noop_while_opfunc_cb_stays_none() {
        let _lock = crate::globals::global_state_test_lock();
        // Same reasoning as the test above: OPFUNC_CB is Callback::None
        // in every session this crate can build, so freeing it must be
        // well defined and leave it None - including when called twice,
        // as an exit path may well do.
        unsafe { free_operatorfunc_option() };
        unsafe { free_operatorfunc_option() };
        // Still None, so it still has nothing for the GC to mark.
        assert!(!unsafe { set_ref_in_opfunc(1) });
    }

    #[test]
    fn is_ex_cmdchar_true_for_colon() {
        let cap = crate::normal_defs::CmdargT {
            cmdchar: i32::from(b':'),
            ..Default::default()
        };
        assert!(is_ex_cmdchar(&cap));
    }

    #[test]
    fn is_ex_cmdchar_true_for_k_command() {
        let cap = crate::normal_defs::CmdargT {
            cmdchar: crate::keycodes_defs::K_COMMAND,
            ..Default::default()
        };
        assert!(is_ex_cmdchar(&cap));
    }

    #[test]
    fn is_ex_cmdchar_false_for_an_ordinary_command_char() {
        let cap = crate::normal_defs::CmdargT {
            cmdchar: i32::from(b'd'),
            ..Default::default()
        };
        assert!(!is_ex_cmdchar(&cap));
    }

    // ---- skip_comment ----

    /// Points `GLOBALS.curbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime (this
    /// guard does NOT acquire its own lock, matching `change.rs`'s own
    /// established convention for this exact helper).
    struct CurbufGuard {
        previous: *mut crate::buffer_defs::BufT,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut crate::buffer_defs::BufT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    fn buf_with_com(com: &[u8]) -> crate::buffer_defs::BufT {
        crate::buffer_defs::BufT { b_p_com: Some(com.to_vec()), ..Default::default() }
    }

    #[test]
    fn skip_comment_no_leader_at_all() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_com(b"://");
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);

        assert_eq!(unsafe { skip_comment(b"plain code", false, false) }, (0, false));
    }

    #[test]
    fn skip_comment_process_false_detects_an_unclosed_comment() {
        let _lock = crate::globals::global_state_test_lock();
        // "s1:/*" - an opening-style comment leader with NO COM_END
        // ('e') flag at all, so the scan hits the colon before ever
        // finding COM_END, correctly reporting the comment as open.
        let mut buf = buf_with_com(b"s1:/*");
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);

        assert_eq!(unsafe { skip_comment(b"code /*", false, false) }, (0, true));
    }

    #[test]
    fn skip_comment_process_true_advances_past_the_leader() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_com(b"://");
        let _guard = CurbufGuard::set(&mut buf as *mut crate::buffer_defs::BufT);

        assert_eq!(unsafe { skip_comment(b"// hello", true, false) }, (2, true));
        assert_eq!(unsafe { skip_comment(b"// hello", true, true) }, (3, true));
    }

    // ---- get_vts / get_vts_sum ----

    #[test]
    fn get_vts_index_less_than_one_is_zero() {
        assert_eq!(get_vts(&[4, 8, 12], 0), 0);
        assert_eq!(get_vts(&[4, 8, 12], -1), 0);
    }

    #[test]
    fn get_vts_in_range_returns_that_width() {
        assert_eq!(get_vts(&[4, 8, 12], 1), 4);
        assert_eq!(get_vts(&[4, 8, 12], 3), 12);
    }

    #[test]
    fn get_vts_out_of_range_returns_the_last_width() {
        assert_eq!(get_vts(&[4, 8, 12], 5), 12);
    }

    #[test]
    fn get_vts_sum_index_zero_or_less_is_zero() {
        assert_eq!(get_vts_sum(&[4, 8, 12], 0), 0);
        assert_eq!(get_vts_sum(&[4, 8, 12], -1), 0);
    }

    #[test]
    fn get_vts_sum_in_range_sums_up_to_the_index() {
        assert_eq!(get_vts_sum(&[4, 8, 12], 2), 12); // 4+8
        assert_eq!(get_vts_sum(&[4, 8, 12], 3), 24); // 4+8+12
    }

    #[test]
    fn get_vts_sum_out_of_range_extends_with_the_last_width() {
        // 4+8+12 + 12*(5-3) = 24+24 = 48
        assert_eq!(get_vts_sum(&[4, 8, 12], 5), 48);
    }

    // ---- get_new_sw_indent / get_new_vts_indent ----

    /// RAII guard installing `win`/`buf` as curwin/curbuf, restoring
    /// the previous pointers on drop. Holds `global_state_test_lock`
    /// for its entire lifetime, matching `indent.rs`'s own
    /// `CursorTestGuard` precedent (needed since `ml_open`, used to
    /// build the test memline below, touches shared `GLOBALS.got_int`
    /// internally).
    #[test]
    fn shift_line_right_adds_one_shiftwidth() {
        // Cross-verified against real nvim: with expandtab, ts=8 and
        // sw=4, ">>" on a line indented 4 columns yields 8.
        let (mut buf, mut win) = shift_fixture(4, b"    abc");
        let buf_ptr: *mut crate::buffer_defs::BufT = &mut *buf;
        let win_ptr: *mut WinT = &mut *win;
        let _guard = CursorTestGuard::set(win_ptr, buf_ptr);

        unsafe { shift_line(false, false, 1, false) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"        abc\0");

        drop(_guard);
        close_shift_fixture(buf);
    }

    #[test]
    fn shift_line_left_removes_one_shiftwidth() {
        // Cross-verified against real nvim: the same setup with "<<"
        // removes the indent entirely.
        let (mut buf, mut win) = shift_fixture(4, b"    abc");
        let buf_ptr: *mut crate::buffer_defs::BufT = &mut *buf;
        let win_ptr: *mut WinT = &mut *win;
        let _guard = CursorTestGuard::set(win_ptr, buf_ptr);

        unsafe { shift_line(true, false, 1, false) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abc\0");

        drop(_guard);
        close_shift_fixture(buf);
    }

    #[test]
    fn shift_line_falls_back_to_tabstop_when_shiftwidth_is_zero() {
        // 'shiftwidth' zero and no 'vartabstop': the shift size comes
        // from 'tabstop' instead.
        let (mut buf, mut win) = shift_fixture(0, b"abc");
        let buf_ptr: *mut crate::buffer_defs::BufT = &mut *buf;
        let win_ptr: *mut WinT = &mut *win;
        let _guard = CursorTestGuard::set(win_ptr, buf_ptr);

        unsafe { shift_line(false, false, 1, false) };

        assert_eq!(
            unsafe { crate::memline::ml_get(1) },
            b"        abc\0",
            "shifted by ts=8"
        );

        drop(_guard);
        close_shift_fixture(buf);
    }

    /// Builds a fixture for the [`shift_line`] tests. As for
    /// `indent.rs`'s own `set_indent` fixture, a real undo header is
    /// installed so `extmark_splice_cols` does not have to create one.
    fn shift_fixture(
        sw: crate::types_defs::OptInt,
        line: &[u8],
    ) -> (Box<crate::buffer_defs::BufT>, Box<WinT>) {
        let mut buf = Box::new(crate::buffer_defs::BufT {
            b_p_et: 1,
            b_p_ts: 8,
            b_p_sw: sw,
            b_u_curhead: Box::into_raw(Box::new(crate::undo_defs::UHeader::default())),
            ..Default::default()
        });
        assert_eq!(
            unsafe { crate::memline::ml_open(&mut buf) },
            crate::vim_defs::OK
        );
        let mut owned = line.to_vec();
        owned.push(0);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, &owned) },
            crate::vim_defs::OK
        );
        let win = Box::new(WinT {
            w_cursor: crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        });
        (buf, win)
    }

    fn close_shift_fixture(mut buf: Box<crate::buffer_defs::BufT>) {
        unsafe {
            if !buf.b_u_curhead.is_null() {
                drop(Box::from_raw(buf.b_u_curhead));
                buf.b_u_curhead = std::ptr::null_mut();
            }
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn swapchars_walks_a_byte_budget_across_characters() {
        // Cross-verified against real nvim: "aéb" with "g~~" becomes
        // "AÉB". The é is 2 bytes, so a 4-byte budget covers all three
        // characters exactly.
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, "aéb\0".as_bytes());
        let prev = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 1 };

        let mut p = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        assert!(unsafe { swapchars(OpType::Tilde, &mut p, 4) });

        assert_eq!(unsafe { crate::memline::ml_get(1) }, "AÉB\0".as_bytes());

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev };
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn swapchars_stops_at_the_byte_budget() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"abcd\0");
        let prev = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 1 };

        let mut p = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        assert!(unsafe { swapchars(OpType::Upper, &mut p, 2) });

        assert_eq!(
            unsafe { crate::memline::ml_get(1) },
            b"ABcd\0",
            "only the first two bytes were touched"
        );

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev };
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn swapchars_reports_false_when_nothing_changed() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"123\0");
        let prev = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 1 };

        let mut p = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        assert!(!unsafe { swapchars(OpType::Tilde, &mut p, 3) });
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"123\0");

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev };
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn pbyte_replaces_one_byte_in_place() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"abc\0");
        let prev = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 1 };

        unsafe { pbyte(crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 }, i32::from(b'X')) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"aXc\0");

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev };
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn pbyte_clamps_a_column_past_the_end_of_the_line() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"abc\0");
        let prev = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 1 };

        // textlen is 4 (including the NUL), so a column of 99 clamps
        // to len - 2 == 2, the last real character.
        unsafe { pbyte(crate::pos_defs::PosT { lnum: 1, col: 99, coladd: 0 }, i32::from(b'Z')) };

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abZ\0");

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev };
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn swapchar_toggles_ascii_case() {
        // Cross-verified against real nvim: "aB" with "g~~" becomes
        // "Ab", so each character's case is toggled independently.
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"aB\0");
        let prev = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 1 };

        let p0 = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let p1 = crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 };
        assert!(unsafe { swapchar(OpType::Tilde, &p0) });
        assert!(unsafe { swapchar(OpType::Tilde, &p1) });

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"Ab\0");

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev };
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn swapchar_rot13_rotates_ascii_letters() {
        // Cross-verified against real nvim: "abn" with "g??" becomes
        // "noa" - a->n, b->o and n wraps back round to a.
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"abn\0");
        let prev = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 1 };

        for col in 0..3 {
            let p = crate::pos_defs::PosT { lnum: 1, col, coladd: 0 };
            assert!(unsafe { swapchar(OpType::Rot13, &p) });
        }

        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"noa\0");

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev };
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn swapchar_respects_the_upper_and_lower_operators() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"aB\0");
        let prev = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 1 };

        let p0 = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let p1 = crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 };
        // gU only uppercases: 'a' changes, the already-upper 'B' does not.
        assert!(unsafe { swapchar(OpType::Upper, &p0) });
        assert!(!unsafe { swapchar(OpType::Upper, &p1) });
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"AB\0");

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev };
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn swapchar_leaves_non_letters_alone() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"1-2\0");
        let prev = *unsafe { crate::extmark::CURBUF_SPLICE_PENDING.get_mut() };
        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = 1 };

        for col in 0..3 {
            let p = crate::pos_defs::PosT { lnum: 1, col, coladd: 0 };
            assert!(!unsafe { swapchar(OpType::Tilde, &p) }, "col {col}");
        }
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"1-2\0");

        unsafe { *crate::extmark::CURBUF_SPLICE_PENDING.get_mut() = prev };
        drop(guard);
        close_buf_with_memline(buf);
    }

    struct CursorTestGuard {
        prev_curwin: *mut WinT,
        prev_curbuf: *mut crate::buffer_defs::BufT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CursorTestGuard {
        fn set(win: *mut WinT, buf: *mut crate::buffer_defs::BufT) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = CursorTestGuard { prev_curwin: globals.curwin, prev_curbuf: globals.curbuf, _lock };
            globals.curwin = win;
            globals.curbuf = buf;
            guard
        }
    }

    impl Drop for CursorTestGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.curwin = self.prev_curwin;
            globals.curbuf = self.prev_curbuf;
        }
    }

    /// Installs `win`/`buf` as curwin/curbuf, then opens a fresh
    /// memline for `buf` and replaces line 1 with `line` (matching
    /// `indent.rs`'s own `open_and_set_test_buf` precedent). Callers
    /// must close `buf.b_ml.ml_mfp` themselves after the guard drops.
    fn open_and_set_test_buf(
        win: &mut WinT,
        buf: &mut crate::buffer_defs::BufT,
        line: &[u8],
    ) -> CursorTestGuard {
        let guard = CursorTestGuard::set(win as *mut WinT, buf as *mut crate::buffer_defs::BufT);
        assert_eq!(unsafe { crate::memline::ml_open(buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(buf, 1, line) },
            crate::vim_defs::OK
        );
        guard
    }

    fn close_buf_with_memline(buf: crate::buffer_defs::BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    /// Restores `GLOBALS.State` on drop, including during a panic -
    /// essential for [`replace_character`]'s boundary test, which
    /// unwinds out of the function BEFORE its own restore runs and
    /// would otherwise leave `MODE_REPLACE` set for every later test
    /// in this process.
    struct StateGuard {
        prev: i32,
    }

    impl StateGuard {
        fn set(state: i32) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = StateGuard { prev: g.State };
            g.State = state;
            guard
        }
    }

    impl Drop for StateGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.State = self.prev;
        }
    }

    #[test]
    fn replace_character_switches_to_replace_mode_and_reaches_its_boundary() {
        // Proves the mode switch is real: ins_char's Replace branch is
        // reachable ONLY because this function sets MODE_REPLACE, and
        // that branch still needs replace_push/replace_push_nul.
        //
        // Cross-verified against real nvim for when the dependency
        // lands: "abcd" with the cursor on column 3 and `rX` yields
        // "abXd" with the cursor left ON the replaced character.
        let mut buf = crate::buffer_defs::BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"abcd\0");
        buf.b_u_curhead = Box::into_raw(Box::new(crate::undo_defs::UHeader::default()));
        buf.b_p_ma = 1;
        buf.b_u_synced = true;
        win.w_cursor.lnum = 1;
        win.w_cursor.col = 2;
        let _state = StateGuard::set(crate::state_defs::mode::NORMAL as i32);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            replace_character(i32::from(b'X'));
        }));
        assert!(result.is_err(), "expected the replace_push boundary");

        drop(_state);
        drop(guard);
        unsafe {
            drop(Box::from_raw(buf.b_u_curhead));
            buf.b_u_curhead = std::ptr::null_mut();
        }
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_new_sw_indent_shift_right_adds_sw_val() {
        let mut buf = crate::buffer_defs::BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0"); // indent=4
        win.w_cursor.lnum = 1;

        assert_eq!(unsafe { get_new_sw_indent(false, false, 1, 4) }, 8);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_new_sw_indent_shift_left_subtracts_sw_val_clamped_at_zero() {
        let mut buf = crate::buffer_defs::BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0"); // indent=4
        win.w_cursor.lnum = 1;

        assert_eq!(unsafe { get_new_sw_indent(true, false, 1, 4) }, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_new_sw_indent_round_shift_right_rounds_up_to_the_next_tabstop() {
        let mut buf = crate::buffer_defs::BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0"); // indent=4
        win.w_cursor.lnum = 1;

        // indent=4 is already an exact multiple of sw_val=4, so
        // rounding right just adds one full shiftwidth: 4+4=8.
        assert_eq!(unsafe { get_new_sw_indent(false, true, 1, 4) }, 8);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_new_sw_indent_round_shift_left_removes_the_remainder_first() {
        let mut buf = crate::buffer_defs::BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0"); // indent=4
        win.w_cursor.lnum = 1;

        // indent=4, sw_val=3: 4/3=1 remainder 1. The remainder makes
        // the first "shift" just remove it (rounding down to 3), so
        // amount=1 consumes exactly that removal - final indent=3.
        assert_eq!(unsafe { get_new_sw_indent(true, true, 1, 3) }, 3);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_new_vts_indent_shift_right_moves_to_the_next_tabstop() {
        let mut buf = crate::buffer_defs::BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0"); // indent=4
        win.w_cursor.lnum = 1;

        // vts=[4,8]: indent=4 sits exactly at the first tabstop
        // boundary; shifting right by 1 moves to the SECOND tabstop's
        // own cumulative offset (4+8=12).
        assert_eq!(unsafe { get_new_vts_indent(false, false, 1, &[4, 8]) }, 12);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_new_vts_indent_shift_left_moves_to_the_previous_tabstop() {
        let mut buf = crate::buffer_defs::BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0"); // indent=4
        win.w_cursor.lnum = 1;

        // Same setup as above, but shifting LEFT by 1 from the first
        // tabstop boundary lands at column 0.
        assert_eq!(unsafe { get_new_vts_indent(true, false, 1, &[4, 8]) }, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    // ---- mb_adjust_opend ----

    #[test]
    fn mb_adjust_opend_noop_when_not_inclusive() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        let mut oap = crate::normal_defs::OpargT {
            inclusive: false,
            end: crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 },
            ..Default::default()
        };
        unsafe { mb_adjust_opend(&mut oap) };
        assert_eq!(oap.end.col, 1);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn mb_adjust_opend_ascii_character_is_unchanged() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");

        // 'e' at index 1 is already its own single-byte "last byte".
        let mut oap = crate::normal_defs::OpargT {
            inclusive: true,
            end: crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 },
            ..Default::default()
        };
        unsafe { mb_adjust_opend(&mut oap) };
        assert_eq!(oap.end.col, 1);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn mb_adjust_opend_multibyte_character_from_its_lead_byte() {
        // "日本\0" = [E6,97,A5, E6,9C,AC, 00] - same verified bytes as
        // mbyte.rs's own utf_head_off_does_not_merge_two_independent_
        // cjk_characters test.
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, "\u{65E5}\u{672C}\0".as_bytes());

        // col=3 is the LEAD byte of 本 (the second character) - the
        // last byte of that same 3-byte character is at col=5.
        let mut oap = crate::normal_defs::OpargT {
            inclusive: true,
            end: crate::pos_defs::PosT { lnum: 1, col: 3, coladd: 0 },
            ..Default::default()
        };
        unsafe { mb_adjust_opend(&mut oap) };
        assert_eq!(oap.end.col, 5);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn mb_adjust_opend_multibyte_character_from_a_continuation_byte() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, "\u{65E5}\u{672C}\0".as_bytes());

        // col=4 is the SECOND byte of 本 - mb_adjust_opend must first
        // walk back to the lead byte (col=3), then land at the SAME
        // final position (col=5) as starting from the lead byte
        // itself.
        let mut oap = crate::normal_defs::OpargT {
            inclusive: true,
            end: crate::pos_defs::PosT { lnum: 1, col: 4, coladd: 0 },
            ..Default::default()
        };
        unsafe { mb_adjust_opend(&mut oap) };
        assert_eq!(oap.end.col, 5);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn mb_adjust_opend_noop_at_end_of_line() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, "\u{65E5}\u{672C}\0".as_bytes());

        // col=6 points AT the trailing NUL - *ptr == NUL, so this is a
        // no-op (matches the original's own `if (*ptr != NUL)` guard).
        let mut oap = crate::normal_defs::OpargT {
            inclusive: true,
            end: crate::pos_defs::PosT { lnum: 1, col: 6, coladd: 0 },
            ..Default::default()
        };
        unsafe { mb_adjust_opend(&mut oap) };
        assert_eq!(oap.end.col, 6);

        drop(guard);
        close_buf_with_memline(buf);
    }

    // ---- get_op_vcol ----

    /// Sets `GLOBALS.Visual.mode`/`redo_busy` for a test, wrapping the
    /// required `unsafe` access in one place. Callers must already
    /// hold `global_state_test_lock()` (via a self-locking guard like
    /// `CursorTestGuard`) for their whole body.
    fn set_visual_state(mode: u8, redo_busy: bool) {
        // SAFETY: forwarded from this function's own doc comment.
        let visual = &mut unsafe { crate::globals::GLOBALS.get_mut() }.Visual;
        visual.mode = i32::from(mode);
        visual.redo_busy = redo_busy;
    }

    /// Sets `OPTION_VARS.p_sel` for a test, wrapping the required
    /// `unsafe` access in one place. Same locking obligation as
    /// [`set_visual_state`].
    fn set_p_sel(value: Option<&[u8]>) {
        // SAFETY: forwarded from this function's own doc comment.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sel = value.map(<[u8]>::to_vec);
    }

    /// Same as [`open_and_set_test_buf`], but replaces line 1 with
    /// `lines[0]`, appends each remaining entry of `lines` in order
    /// (matching `plines.rs`'s own `buf_with_lines` precedent), and
    /// wires `win.w_buffer` to point at `buf` (needed since
    /// `get_op_vcol` itself reads `(*curwin).w_buffer` directly,
    /// matching the original's own `curwin->w_buffer` usage - unlike
    /// every OTHER function tested via `open_and_set_test_buf`, which
    /// only ever needs `GLOBALS.curbuf`). `win.w_buffer` is wired
    /// LAST, after every other reborrow of `buf`, to avoid the
    /// "raw pointer invalidated by a later reborrow of the same local"
    /// Tree Borrows hazard already documented elsewhere in this crate.
    fn open_and_set_test_buf_lines(
        win: &mut WinT,
        buf: &mut crate::buffer_defs::BufT,
        lines: &[&[u8]],
    ) -> CursorTestGuard {
        let guard = open_and_set_test_buf(win, buf, lines[0]);
        for (after, line) in (1..).zip(lines[1..].iter()) {
            assert_eq!(
                unsafe { crate::memline::ml_append_buf(buf, after, line, line.len() as i32, false) },
                crate::vim_defs::OK
            );
        }
        win.w_buffer = buf as *mut crate::buffer_defs::BufT;
        guard
    }

    #[test]
    fn get_op_vcol_wrong_visual_mode_is_a_no_op() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"abcde\0"]);
        set_visual_state(b'v', false); // charwise, not Ctrl-V

        let mut oap = crate::normal_defs::OpargT {
            start: crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 },
            end: crate::pos_defs::PosT { lnum: 1, col: 3, coladd: 0 },
            ..Default::default()
        };
        unsafe { get_op_vcol(&mut oap, 0, true) };

        // Completely untouched: motion_type stays the default CharWise.
        assert_eq!(oap.motion_type, crate::normal_defs::MotionType::CharWise);
        assert_eq!(oap.start.col, 1);
        assert_eq!(oap.end.col, 3);
        assert_eq!(oap.start_vcol, 0);
        assert_eq!(oap.end_vcol, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_op_vcol_narrow_selection_is_a_no_op_when_not_initial() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT { w_view_width: 10, ..Default::default() };
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"abcde\0"]);
        set_visual_state(crate::ascii_defs::CTRL_V, false);

        let mut oap = crate::normal_defs::OpargT {
            start: crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 },
            end: crate::pos_defs::PosT { lnum: 1, col: 3, coladd: 0 }, // 3 < w_view_width(10)
            ..Default::default()
        };
        unsafe { get_op_vcol(&mut oap, 0, false) };

        assert_eq!(oap.motion_type, crate::normal_defs::MotionType::CharWise);
        assert_eq!(oap.end.col, 3);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_op_vcol_normalizes_start_and_end_vcol_and_converts_to_positions() {
        // Passes the early-return guard via `oap.end.col >= w_view_width`
        // (not via `initial`), unlike the other tests below.
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT { w_view_width: 0, ..Default::default() };
        let guard =
            open_and_set_test_buf_lines(&mut win, &mut buf, &[b"abcdefgh\0", b"ijklmnop\0"]);
        set_visual_state(crate::ascii_defs::CTRL_V, false);

        // start.col(5) > end.col(1): hand-traced (see the commit
        // history) that the function still normalizes these into the
        // upper-left(vcol1)/lower-right(vcol5) corners regardless of
        // which one was given as "start" vs. "end".
        let mut oap = crate::normal_defs::OpargT {
            start: crate::pos_defs::PosT { lnum: 1, col: 5, coladd: 0 },
            end: crate::pos_defs::PosT { lnum: 2, col: 1, coladd: 0 },
            ..Default::default()
        };
        unsafe { get_op_vcol(&mut oap, 0, false) };

        assert_eq!(oap.motion_type, crate::normal_defs::MotionType::BlockWise);
        assert_eq!(oap.start_vcol, 1);
        assert_eq!(oap.end_vcol, 5);
        assert_eq!(oap.start, crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 });
        assert_eq!(oap.end, crate::pos_defs::PosT { lnum: 2, col: 5, coladd: 0 });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_op_vcol_exclusive_selection_excludes_the_last_column() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard =
            open_and_set_test_buf_lines(&mut win, &mut buf, &[b"abcdefgh\0", b"abcdefgh\0"]);
        set_visual_state(crate::ascii_defs::CTRL_V, false);
        set_p_sel(Some(b"exclusive"));

        // start vcol=2, end vcol=6: with 'selection'=exclusive and
        // initial=true, the trailing column is excluded (end_vcol
        // becomes end's own start(6) - 1 = 5, not 6).
        let mut oap = crate::normal_defs::OpargT {
            start: crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 },
            end: crate::pos_defs::PosT { lnum: 2, col: 6, coladd: 0 },
            ..Default::default()
        };
        unsafe { get_op_vcol(&mut oap, 0, true) };

        assert_eq!(oap.start_vcol, 2);
        assert_eq!(oap.end_vcol, 5);

        set_p_sel(None);
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_op_vcol_redo_busy_computes_end_vcol_arithmetically() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf_lines(
            &mut win,
            &mut buf,
            &[b"xyz\0", b"abcdefghijklmnop\0"],
        );
        set_visual_state(crate::ascii_defs::CTRL_V, true);

        // redo_busy=true skips the "getvvcol(oap.end)" block entirely:
        // oap.end_vcol is computed purely as
        // start_vcol(2) + redo_visual_vcol(10) - 1 = 11, regardless of
        // oap.end's own original column.
        let mut oap = crate::normal_defs::OpargT {
            start: crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 },
            end: crate::pos_defs::PosT { lnum: 2, col: 0, coladd: 0 },
            ..Default::default()
        };
        unsafe { get_op_vcol(&mut oap, 10, true) };

        assert_eq!(oap.start_vcol, 2);
        assert_eq!(oap.end_vcol, 11);
        assert_eq!(oap.end.lnum, 2);
        assert_eq!(oap.end.col, 11); // vcol 11 in "abcdefghijklmnop" is 'l'

        set_visual_state(crate::ascii_defs::CTRL_V, false);
        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_op_vcol_dollar_motion_uses_the_longest_line_in_range() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf_lines(
            &mut win,
            &mut buf,
            &[b"ab\0", b"abcdefgh\0", b"abcd\0"],
        );
        set_visual_state(crate::ascii_defs::CTRL_V, false);
        win.w_curswant = crate::pos_defs::MAXCOL;

        let mut oap = crate::normal_defs::OpargT {
            start: crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 },
            end: crate::pos_defs::PosT { lnum: 3, col: 0, coladd: 0 },
            ..Default::default()
        };
        unsafe { get_op_vcol(&mut oap, 0, true) };

        // Scans every line from oap.start.lnum..=oap.end.lnum (1..=3)
        // via curwin.w_cursor mutation, keeping the MAXIMUM
        // end-of-line vcol seen: line 2 ("abcdefgh", 8 chars) is the
        // longest, so end_vcol == 8 (its own NUL-terminator vcol),
        // even though oap.end itself points at line 3.
        assert_eq!(oap.motion_type, crate::normal_defs::MotionType::BlockWise);
        assert_eq!(oap.start_vcol, 0);
        assert_eq!(oap.end_vcol, 8);

        drop(guard);
        close_buf_with_memline(buf);
    }

    // ---- adjust_cursor_eol ----

    /// Sets `GLOBALS.State`/`restart_edit` for a test, wrapping the
    /// required `unsafe` access in one place. Same locking obligation
    /// as [`set_visual_state`].
    fn set_state_and_restart_edit(state: i32, restart_edit: i32) {
        // SAFETY: forwarded from this function's own doc comment.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.State = state;
        g.restart_edit = restart_edit;
    }

    #[test]
    fn adjust_cursor_eol_not_at_eol_is_a_no_op() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"ab\0"]);
        set_state_and_restart_edit(crate::state_defs::mode::NORMAL as i32, 0);

        // col=1 points at 'b' - not the trailing NUL, so gchar_cursor()
        // != 0 and the whole check short-circuits to false.
        unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin }.w_cursor =
            crate::pos_defs::PosT { lnum: 1, col: 1, coladd: 0 };

        unsafe { adjust_cursor_eol() };

        assert_eq!(
            unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col,
            1
        );

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn adjust_cursor_eol_at_column_zero_is_a_no_op() {
        // col=0 short-circuits before gchar_cursor() is ever called -
        // no real memline needed at all (matching the original's own
        // short-circuiting && chain, faithfully preserved in this
        // translation).
        let mut buf = crate::buffer_defs::BufT::default();
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT {
            w_cursor: crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 },
            w_buffer: buf_ptr,
            ..Default::default()
        };
        let guard = CursorTestGuard::set(&mut win as *mut WinT, buf_ptr);
        set_state_and_restart_edit(crate::state_defs::mode::NORMAL as i32, 0);

        unsafe { adjust_cursor_eol() };
        assert_eq!(win.w_cursor.col, 0);

        drop(guard);
    }

    #[test]
    fn adjust_cursor_eol_onemore_flag_is_a_no_op() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_ve_flags: crate::option_vars::opt_ve_flag::ONEMORE,
                ..Default::default()
            },
            ..Default::default()
        };
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"ab\0"]);
        set_state_and_restart_edit(crate::state_defs::mode::NORMAL as i32, 0);
        unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin }.w_cursor =
            crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 }; // at the trailing NUL

        unsafe { adjust_cursor_eol() };

        assert_eq!(
            unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col,
            2
        );

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn adjust_cursor_eol_all_flag_is_a_no_op_the_dead_branch_is_never_reached() {
        // Demonstrates the real upstream quirk documented on
        // adjust_cursor_eol's own doc comment: 'virtualedit'=all
        // itself disables the whole adjustment (the guard's own
        // `(cur_ve_flags & opt_ve_flag::ALL) == 0` check fails), so
        // the coladd-computing branch further down is unreachable.
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT {
                wo_ve_flags: crate::option_vars::opt_ve_flag::ALL,
                ..Default::default()
            },
            ..Default::default()
        };
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"ab\0"]);
        set_state_and_restart_edit(crate::state_defs::mode::NORMAL as i32, 0);
        unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin }.w_cursor =
            crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 };

        unsafe { adjust_cursor_eol() };

        let w_cursor = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor;
        assert_eq!(w_cursor.col, 2); // unchanged
        assert_eq!(w_cursor.coladd, 0); // the dead branch never ran

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn adjust_cursor_eol_insert_mode_is_a_no_op() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"ab\0"]);
        set_state_and_restart_edit(crate::state_defs::mode::INSERT as i32, 0);
        unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin }.w_cursor =
            crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 };

        unsafe { adjust_cursor_eol() };

        assert_eq!(
            unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col,
            2
        );

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn adjust_cursor_eol_restart_edit_is_a_no_op() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"ab\0"]);
        set_state_and_restart_edit(crate::state_defs::mode::NORMAL as i32, i32::from(b'i'));
        unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin }.w_cursor =
            crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 };

        unsafe { adjust_cursor_eol() };

        assert_eq!(
            unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor.col,
            2
        );

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn adjust_cursor_eol_moves_cursor_left_when_conditions_are_met() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf_lines(&mut win, &mut buf, &[b"ab\0"]);
        set_state_and_restart_edit(crate::state_defs::mode::NORMAL as i32, 0);
        unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin }.w_cursor =
            crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 }; // at the trailing NUL

        unsafe { adjust_cursor_eol() };

        let w_cursor = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_cursor;
        assert_eq!(w_cursor.col, 1); // moved back onto 'b'
        assert_eq!(w_cursor.coladd, 0);

        drop(guard);
        close_buf_with_memline(buf);
    }

    // ---- line_count_info ----

    #[test]
    fn line_count_info_counts_words_and_chars_and_adds_eol_size() {
        // "hello world\0": 11 real chars + 2 words, then the trailing
        // NUL confirms end-of-line was reached before `limit`, so
        // `eol_size` is added to both the char count and the return
        // value (matching the doc comment's own description).
        let mut wc: i64 = 0;
        let mut cc: i64 = 0;
        let n = line_count_info(b"hello world\0", &mut wc, &mut cc, 1000, 1);
        assert_eq!(wc, 2);
        assert_eq!(cc, 12); // 11 chars + 1 for eol_size
        assert_eq!(n, 12);
    }

    #[test]
    fn line_count_info_truncated_by_limit_before_eol_omits_eol_size() {
        // limit=3 stops scanning after 'h','e','l' - never reaches the
        // trailing NUL, so eol_size must NOT be added (this is the
        // "stopped because of limit, not because of EOL" branch).
        let mut wc: i64 = 0;
        let mut cc: i64 = 0;
        let n = line_count_info(b"hello\0", &mut wc, &mut cc, 3, 1);
        assert_eq!(wc, 1); // still mid-word when truncated: counts as 1 word
        assert_eq!(cc, 3);
        assert_eq!(n, 3);
    }

    #[test]
    fn line_count_info_accumulates_into_existing_wc_and_cc() {
        // *wc/*cc are ADDED to, not overwritten - matches the doc
        // comment's own "added to `*wc`"/"added to `*cc`" wording.
        let mut wc: i64 = 10;
        let mut cc: i64 = 20;
        let n = line_count_info(b"hi\0", &mut wc, &mut cc, 1000, 1);
        assert_eq!(wc, 11); // +1 word
        assert_eq!(cc, 23); // +2 chars +1 eol_size
        assert_eq!(n, 3);
    }

    #[test]
    fn line_count_info_empty_line_still_counts_the_eol() {
        let mut wc: i64 = 0;
        let mut cc: i64 = 0;
        let n = line_count_info(b"\0", &mut wc, &mut cc, 1000, 1);
        assert_eq!(wc, 0);
        assert_eq!(cc, 1); // no real characters, just eol_size
        assert_eq!(n, 1);
    }

    #[test]
    fn line_count_info_counts_a_multibyte_character_as_one_char() {
        // "日\0" (U+65E5, 3 UTF-8 bytes) - chars must count 1 for the
        // whole character, not 3 for its bytes; the return value still
        // advances by the full byte length (3) plus eol_size (1).
        let mut wc: i64 = 0;
        let mut cc: i64 = 0;
        let n = line_count_info("\u{65E5}\0".as_bytes(), &mut wc, &mut cc, 1000, 1);
        assert_eq!(wc, 1);
        assert_eq!(cc, 2); // 1 char + 1 eol_size
        assert_eq!(n, 4); // 3 bytes + 1 eol_size
    }

    // ---- get_region_bytecount ----

    /// Opens `buf` (real block 0/data block allocation via `ml_open`)
    /// with `first_line` as line 1, then appends each of `rest` in
    /// order, matching `plines.rs`'s own `buf_with_lines` precedent.
    /// Callers must hold `crate::globals::global_state_test_lock()`
    /// for their whole test body (touches `mf_sync` internally via
    /// `ml_open`) and clean up via [`close_buf`].
    unsafe fn buf_with_lines(first_line: &[u8], rest: &[&[u8]]) -> crate::buffer_defs::BufT {
        let mut buf = crate::buffer_defs::BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, first_line) },
            crate::vim_defs::OK
        );
        for (after, line) in (1..).zip(rest.iter()) {
            assert_eq!(
                unsafe {
                    crate::memline::ml_append_buf(&mut buf, after, line, line.len() as i32, false)
                },
                crate::vim_defs::OK
            );
        }
        buf
    }

    unsafe fn close_buf(buf: crate::buffer_defs::BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn get_region_bytecount_start_lnum_beyond_buffer_returns_zero() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut buf = buf_with_lines(b"x\0", &[]);
            assert_eq!(get_region_bytecount(&mut buf, 5, 5, 0, 0), 0);
            close_buf(buf);
        }
    }

    #[test]
    fn get_region_bytecount_same_line_is_just_the_column_difference() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut buf = buf_with_lines(b"hello\0", &[]);
            assert_eq!(get_region_bytecount(&mut buf, 1, 1, 1, 4), 3);
            close_buf(buf);
        }
    }

    #[test]
    fn get_region_bytecount_spans_multiple_full_lines() {
        // "hello"/"world"/"abc", region from (1,2) to (3,1):
        // line 1 tail ("llo", 3 bytes) + 1 newline = 4,
        // line 2 in full ("world", 5 bytes) + 1 newline = 6,
        // line 3 up to (exclusive) column 1 = 1 byte ('a').
        // Total: 4 + 6 + 1 = 11.
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut buf = buf_with_lines(b"hello\0", &[b"world\0", b"abc\0"]);
            assert_eq!(get_region_bytecount(&mut buf, 1, 3, 2, 1), 11);
            close_buf(buf);
        }
    }

    #[test]
    fn get_region_bytecount_end_lnum_far_beyond_buffer_stops_mid_loop() {
        // end_lnum=10 is far past max_lnum=2: the interior-line bounds
        // check inside the loop (not the one after it) catches this,
        // returning the running total without ever consulting end_col.
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut buf = buf_with_lines(b"hi\0", &[b"yo\0"]);
            // deleted_bytes = (2 - 0 + 1) = 3 from line 1, then i=1
            // (start_lnum+i=2 <= max_lnum=2) adds (2 + 1) = 3 -> 6,
            // then i=2 (start_lnum+i=3 > max_lnum=2) returns 6 early.
            assert_eq!(get_region_bytecount(&mut buf, 1, 10, 0, 999), 6);
            close_buf(buf);
        }
    }

    #[test]
    fn get_region_bytecount_end_lnum_exactly_one_past_buffer_skips_end_col() {
        // end_lnum = max_lnum + 1 exactly: every INTERIOR line (2, 3)
        // is in range, so the loop completes normally: only the final
        // `end_lnum > max_lnum` check (after the loop) catches this,
        // and `end_col` is never added.
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let mut buf = buf_with_lines(b"aa\0", &[b"bb\0", b"cc\0"]);
            // (2-0+1)=3, then +(2+1)=3 twice more -> 3+3+3=9.
            assert_eq!(get_region_bytecount(&mut buf, 1, 4, 0, 5), 9);
            close_buf(buf);
        }
    }
}
