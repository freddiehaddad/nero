//! Translated from `src/nvim/ops.h` (partial: the `OP_*` operator-ID
//! enum only - `ops.h` is a large header covering register/yank/paste
//! types and structures too, not attempted here).
//!
//! `OpType` (`OP_*`): mechanically transcribed directly from the
//! header's own `enum { OP_NOP = 0, OP_DELETE = 1, ... }` (30 values).
//! Its numeric order is load-bearing: `src/nvim/ops.c`'s own
//! `opchars[]` table (translated as `crate::ops::OPCHARS`) is indexed
//! by this exact enum, per the header's own comment ("Index must
//! correspond with defines in ops.h!!!" / "order must correspond to
//! opchars[] in ops.c!").

/// Operator IDs (`OP_*`). The order must correspond to
/// `crate::ops::OPCHARS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum OpType {
    /// no pending operation (`OP_NOP`).
    Nop = 0,
    /// `"d"` delete operator (`OP_DELETE`).
    Delete = 1,
    /// `"y"` yank operator (`OP_YANK`).
    Yank = 2,
    /// `"c"` change operator (`OP_CHANGE`).
    Change = 3,
    /// `"<"` left shift operator (`OP_LSHIFT`).
    Lshift = 4,
    /// `">"` right shift operator (`OP_RSHIFT`).
    Rshift = 5,
    /// `"!"` filter operator (`OP_FILTER`).
    Filter = 6,
    /// `"g~"` switch case operator (`OP_TILDE`).
    Tilde = 7,
    /// `"="` indent operator (`OP_INDENT`).
    Indent = 8,
    /// `"gq"` format operator (`OP_FORMAT`).
    Format = 9,
    /// `":"` colon operator (`OP_COLON`).
    Colon = 10,
    /// `"gU"` make upper case operator (`OP_UPPER`).
    Upper = 11,
    /// `"gu"` make lower case operator (`OP_LOWER`).
    Lower = 12,
    /// `"J"` join operator, only for Visual mode (`OP_JOIN`).
    Join = 13,
    /// `"gJ"` join operator, only for Visual mode (`OP_JOIN_NS`).
    JoinNs = 14,
    /// `"g?"` rot-13 encoding (`OP_ROT13`).
    Rot13 = 15,
    /// `"r"` replace chars, only for Visual mode (`OP_REPLACE`).
    Replace = 16,
    /// `"I"` Insert column, only for Visual mode (`OP_INSERT`).
    Insert = 17,
    /// `"A"` Append column, only for Visual mode (`OP_APPEND`).
    Append = 18,
    /// `"zf"` define a fold (`OP_FOLD`).
    Fold = 19,
    /// `"zo"` open folds (`OP_FOLDOPEN`).
    Foldopen = 20,
    /// `"zO"` open folds recursively (`OP_FOLDOPENREC`).
    Foldopenrec = 21,
    /// `"zc"` close folds (`OP_FOLDCLOSE`).
    Foldclose = 22,
    /// `"zC"` close folds recursively (`OP_FOLDCLOSEREC`).
    Foldcloserec = 23,
    /// `"zd"` delete folds (`OP_FOLDDEL`).
    Folddel = 24,
    /// `"zD"` delete folds recursively (`OP_FOLDDELREC`).
    Folddelrec = 25,
    /// `"gw"` format operator, keeps cursor pos (`OP_FORMAT2`).
    Format2 = 26,
    /// `"g@"` call `'operatorfunc'` (`OP_FUNCTION`).
    Function = 27,
    /// `"<C-A>"` add to the number or alphabetic character
    /// (`OP_NR_ADD`).
    NrAdd = 28,
    /// `"<C-X>"` subtract from the number or alphabetic character
    /// (`OP_NR_SUB`).
    NrSub = 29,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_type_first_and_last_values_match_the_c_enum() {
        assert_eq!(OpType::Nop as i32, 0);
        assert_eq!(OpType::NrSub as i32, 29);
    }

    #[test]
    fn op_type_spot_check_a_few_values_against_the_c_enum() {
        assert_eq!(OpType::Delete as i32, 1);
        assert_eq!(OpType::Replace as i32, 16);
        assert_eq!(OpType::Function as i32, 27);
    }
}
