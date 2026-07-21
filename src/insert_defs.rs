//! Translated from `src/nvim/insert_defs.h`.

use crate::pos_defs::{ColnrT, LinenrT, PosT};
use crate::types_defs::TriState;

/// Insert-mode session state: the in-progress insert session, as one
/// global "group" (`Ins`), so the insert session can be saved/restored
/// as a whole around nested `edit()` mini-sessions (e.g. a future
/// multicursor live-mirror) (`InsState`).
#[derive(Debug, Clone, Copy, Default)]
pub struct InsState {
    /// Where the latest insert/append mode started
    pub start: PosT,
    /// Where the latest insert/append mode started. In contrast to
    /// `start`, this won't be reset by certain keys and is needed for
    /// `op_insert()`, to detect correctly where inserting by the user
    /// started.
    pub start_orig: PosT,
    /// length of line when insert started
    pub start_textlen: ColnrT,
    /// vcol for first inserted blank
    pub start_blank_vcol: ColnrT,
    /// Normally false, set to true after hitting a cursor key in insert
    /// mode. Used by `vgetorpeek()` to decide when to call `u_sync()`.
    pub arrow_used: bool,
    /// for `":stopinsert"`
    pub stop_insert_mode: bool,
    /// may do cindenting on this line
    pub can_cindent: bool,
    /// call `u_save()` before inserting a char. Set when `edit()` is
    /// called; after that `arrow_used` is used.
    pub need_undo: bool,
    /// Makes auto-indent work right on lines where only a `<CR>` or
    /// `<Esc>` is typed: set when an auto-indent is done, reset when any
    /// other editing is done on the line. If an `<Esc>` or `<CR>` is
    /// received and `did_ai` is true, the line is truncated.
    pub did_ai: bool,
    /// Column of first char after autoindent. 0 when no autoindent
    /// done. Used when `'backspace'` is 0, to avoid backspacing over
    /// autoindent.
    pub ai_col: ColnrT,
    /// A character which will end a start-middle-end comment when typed
    /// as the first character on a new line. Taken from the last
    /// character of the "end" comment leader when the `COM_AUTO_END`
    /// flag is given for that comment end in `'comments'`. Only valid
    /// when `did_ai` is true.
    pub end_comment_pending: i32,
    /// Set when a smart indent has been performed: when the next typed
    /// character is a '{' the inserted tab will be deleted again.
    pub did_si: bool,
    /// after an auto indent: a typed '}' removes one indent
    pub can_si: bool,
    /// after an "O" command: a typed '{' removes one indent
    pub can_si_back: bool,
    /// set `start_orig` to `start`
    pub update_start_orig: bool,
    /// number of chars in front of the current insert
    pub new_insert_skip: i32,
    /// `restart_edit` when `edit()` was called
    pub did_restart_edit: i32,
    /// reverse insert mode on
    pub revins_on: bool,
    /// how much to skip after edit
    pub revins_chars: i32,
    /// was the last char "legal"?
    pub revins_legal: i32,
    /// start column of revins session
    pub revins_scol: i32,
    /// CTRL-G U prevents syncing undo for the next left/right cursor key
    pub dont_sync_undo: TriState,
    /// "o" command's line, for "CTRL-O ." that adds a line (`ins_at_eol`)
    pub o_lnum: LinenrT,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ins_state_default_is_zeroed() {
        let s = InsState::default();
        assert_eq!(s.start, PosT::default());
        assert!(!s.arrow_used);
        assert!(!s.did_ai);
        assert_eq!(s.dont_sync_undo, TriState::None);
        assert_eq!(s.o_lnum, 0);
    }
}
