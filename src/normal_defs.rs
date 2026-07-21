//! Translated from `src/nvim/normal_defs.h`.

use crate::pos_defs::{ColnrT, LinenrT, PosT};
use crate::types_defs::MAX_SCHAR_SIZE;

/// Motion types, used for operators and for yank/delete registers.
///
/// The three valid numerical values must not be changed, as they are
/// used in external communication and serialization (`MotionType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionType {
    /// character-wise movement/register
    #[default]
    CharWise = 0,
    /// line-wise movement/register
    LineWise = 1,
    /// block-wise movement/register
    BlockWise = 2,
}

/// Unknown or invalid motion type (`kMTUnknown`); kept separate from
/// [`MotionType`] since it isn't a valid motion type itself (the
/// original's enum includes it as `-1`, but every real motion-type field
/// only ever holds one of the three real variants above once assigned -
/// callers that need to represent "not yet known" use `Option<MotionType>`
/// instead of this extra variant, a safer idiom than a magic sentinel).
pub const K_MT_UNKNOWN: i32 = -1;

/// Arguments for operators (`oparg_T`).
#[derive(Debug, Clone, Copy, Default)]
pub struct OpargT {
    /// current pending operator type
    pub op_type: i32,
    /// register to use for the operator
    pub regname: i32,
    /// type of the current cursor motion
    pub motion_type: MotionType,
    /// force motion type: 'v', 'V' or CTRL-V
    pub motion_force: i32,
    /// true if delete uses reg 1 even when not linewise
    pub use_reg_one: bool,
    /// true if char motion is inclusive (only valid when `motion_type`
    /// is `CharWise`)
    pub inclusive: bool,
    /// backuped `b_op_end` one char (only used by `do_format()`)
    pub end_adjusted: bool,
    /// start of the operator
    pub start: PosT,
    /// end of the operator
    pub end: PosT,
    /// cursor position before motion for "gw"
    pub cursor_start: PosT,
    /// restore cursor after yank
    pub restore_cursor: bool,

    /// number of lines from `op_start` to `op_end` (inclusive)
    pub line_count: LinenrT,
    /// `op_start` and `op_end` the same (only used by `op_change()`)
    pub empty: bool,
    /// operator on Visual area
    pub is_visual: bool,
    /// start col for block mode operator
    pub start_vcol: ColnrT,
    /// end col for block mode operator
    pub end_vcol: ColnrT,
    /// `ca.opcount` saved for `K_EVENT`
    pub prev_opcount: i32,
    /// `ca.count0` saved for `K_EVENT`
    pub prev_count0: i32,
    /// exclude trailing whitespace for yank of a block
    pub excl_tr_ws: bool,
}

/// Arguments for Normal mode commands (`cmdarg_T`).
///
/// `oap` stays a raw pointer (borrows into the caller's own `oparg_T`,
/// same as the original - this crate's convention is to only introduce
/// ownership/lifetime machinery where the original's manual memory
/// management genuinely needs replacing, not for a plain non-owning
/// back-reference like this one).
pub struct CmdargT {
    /// Operator arguments
    pub oap: *mut OpargT,
    /// prefix character (optional, always 'g')
    pub prechar: i32,
    /// command character
    pub cmdchar: i32,
    /// next command character (optional)
    pub nchar: i32,
    /// next char with composing chars (optional)
    pub nchar_composing: [u8; MAX_SCHAR_SIZE],
    /// len of `nchar_composing` (when zero, use `nchar` instead)
    pub nchar_len: i32,
    /// yet another character (optional)
    pub extra_char: i32,
    /// count before an operator
    pub opcount: i32,
    /// count before command, default 0
    pub count0: i32,
    /// count before command, default 1
    pub count1: i32,
    /// extra argument from `nv_cmds[]`
    pub arg: i32,
    /// return: `CA_*` values
    pub retval: i32,
    /// return: pointer to search pattern or `None`
    pub searchbuf: Option<Vec<u8>>,
}

impl Default for CmdargT {
    fn default() -> Self {
        CmdargT {
            oap: std::ptr::null_mut(),
            prechar: 0,
            cmdchar: 0,
            nchar: 0,
            nchar_composing: [0; MAX_SCHAR_SIZE],
            nchar_len: 0,
            extra_char: 0,
            opcount: 0,
            count0: 0,
            count1: 0,
            arg: 0,
            retval: 0,
            searchbuf: None,
        }
    }
}

/// Values for `cmdarg_T.retval` (`CA_*`).
pub mod ca {
    /// skip restarting `edit()` once
    pub const COMMAND_BUSY: i32 = 1;
    /// don't adjust operator end
    pub const NO_ADJ_OP_END: i32 = 2;
}

/// A Visual selection's mode and extent (line/column span, not absolute
/// positions), so it can be re-applied starting at the cursor: for "gv"
/// reselect (`Visual.resel`) and Visual-operator redo (`redo_VIsual`)
/// (`VisualExtent`).
#[derive(Debug, Clone, Copy, Default)]
pub struct VisualExtent {
    /// 'v', 'V', or Ctrl-V
    pub mode: i32,
    /// number of lines
    pub line_count: LinenrT,
    /// number of cols or end column (`MAXCOL`: to end of line)
    pub vcol: ColnrT,
    /// count for the Visual operator
    pub count: i32,
    /// extra argument
    pub arg: i32,
}

/// Visual/Select mode state, as one global "group" (`Visual`).
/// Previously these were bare `EXTERN` symbols in `globals.h`; grouped
/// here to make subsystem ownership explicit (`VisualState`).
#[derive(Debug, Clone, Copy, Default)]
pub struct VisualState {
    /// Start position of the active Visual selection.
    pub start: PosT,
    /// Whether Visual mode is active.
    pub active: bool,
    /// Whether Select mode is active.
    pub select: bool,
    /// Register name for Select mode.
    pub select_reg: i32,
    /// Cursor was incremented during exclusive selection.
    pub select_exclu_adj: bool,
    /// Restart Select mode when next cmd finished.
    pub restart_select: i32,
    /// Restart the selection after a Select-mode mapping or menu.
    pub reselect: i32,
    /// Type of Visual mode: 'v', 'V', Ctrl-V.
    pub mode: i32,
    /// True when redoing Visual.
    pub redo_busy: bool,
    /// Previous Visual area, for reselection ("gv"); seeds operator-redo.
    pub resel: VisualExtent,
}

/// Replacement for `nchar` used by `nv_replace()` (not yet translated).
pub mod replace_nchar {
    pub const CR: i32 = -1;
    pub const NL: i32 = -2;
}

/// columns needed by shown command (`SHOWCMD_COLS`).
pub const SHOWCMD_COLS: usize = 10;
/// `SHOWCMD_BUFLEN`.
pub const SHOWCMD_BUFLEN: usize = SHOWCMD_COLS + 1 + 30;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_type_default_is_charwise() {
        assert_eq!(MotionType::default(), MotionType::CharWise);
        assert_eq!(MotionType::LineWise as i32, 1);
        assert_eq!(MotionType::BlockWise as i32, 2);
        assert_eq!(K_MT_UNKNOWN, -1);
    }

    #[test]
    fn oparg_default_is_zeroed() {
        let oap = OpargT::default();
        assert_eq!(oap.op_type, 0);
        assert_eq!(oap.motion_type, MotionType::CharWise);
        assert!(!oap.inclusive);
    }

    #[test]
    fn cmdarg_default_has_null_oap_and_no_searchbuf() {
        let ca = CmdargT::default();
        assert!(ca.oap.is_null());
        assert!(ca.searchbuf.is_none());
        assert_eq!(ca.nchar_composing.len(), MAX_SCHAR_SIZE);
    }

    #[test]
    fn showcmd_buflen_matches_c_macro() {
        assert_eq!(SHOWCMD_BUFLEN, 10 + 1 + 30);
    }
}
