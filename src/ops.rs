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
//! Deferred: everything else in the file.

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

/// The `'operatorfunc'` callback (`opfunc_cb`, a file-static
/// `Callback`). Nothing in this crate can currently set a real value
/// here: doing so needs `option_set_callback_func`, itself needing
/// `eval_expr`/the full `:set`-parsing `Callback` machinery, none
/// translated - so this stays [`crate::eval::typval_defs::Callback::None`]
/// forever today, matching every real, unconfigured session before
/// `'operatorfunc'` is ever assigned.
static OPFUNC_CB: crate::globals::GlobalCell<crate::eval::typval_defs::Callback> =
    crate::globals::GlobalCell::new(crate::eval::typval_defs::Callback::None);

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
}
