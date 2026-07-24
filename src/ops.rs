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

#[cfg(test)]
mod tests {
    use super::*;

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
}
