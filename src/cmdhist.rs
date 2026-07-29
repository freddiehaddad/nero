//! Translated from `src/nvim/cmdhist.c` (tractable core only).
//!
//! `cmdhist.c` manages the command-line history (`:history`, up/down
//! arrow recall) - almost entirely dependent on the command-line
//! editing subsystem (`ex_getln.c`, which explicitly does not attempt
//! the history-editing machinery either - see that module's own doc
//! comment).
//!
//! Translated: [`HistoryType`], [`HIST_COUNT`], [`get_hislen`] (reading
//! a new file-static `HISLEN`, always `0` today since nothing can add a
//! history entry yet), and [`hist_char2type`] - a pure character-to-enum
//! mapping with no dependencies at all.
//!
//! Deferred: everything else - `get_histentry`/`set_histentry`/
//! `get_hisidx`/`get_hisnum`/`get_history_arg`/`init_history`/
//! `add_to_history`/`clr_history`/`f_hist*`/`ex_history` (need
//! `histentry_T`'s own `AdditionalData`/full history-table storage, and
//! the command-line editing subsystem to ever populate it).

use crate::globals::GlobalCell;

/// Present history tables (`HistoryType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum HistoryType {
    /// Default (current) history.
    Default = -2,
    /// Unknown history.
    Invalid = -1,
    /// Colon commands.
    Cmd = 0,
    /// Search commands.
    Search = 1,
    /// Expressions (e.g. from entering a `=` register).
    Expr = 2,
    /// `input()` lines.
    Input = 3,
    /// Debug commands.
    Debug = 4,
}

/// Number of history tables (`HIST_COUNT`).
pub const HIST_COUNT: usize = 5;

/// actual length of the history tables (`hislen`).
static HISLEN: GlobalCell<i32> = GlobalCell::new(0);

/// Returns the length of the history tables (`get_hislen`).
///
/// Always `0` today: nothing in this crate can currently add a history
/// entry (`init_history`/`add_to_history`, not translated), so
/// `HISLEN` never becomes anything else - a real, faithful consequence
/// of the current state, not a hardcoded stub.
#[must_use]
pub fn get_hislen() -> i32 {
    // SAFETY: a plain read through one exclusive borrow.
    unsafe { *HISLEN.get_mut() }
}

/// Translates a history character (`:`, `=`, `@`, `>`, `/`, `?`, or NUL)
/// to its associated [`HistoryType`], or [`HistoryType::Invalid`] for
/// anything else (`hist_char2type`).
#[must_use]
pub fn hist_char2type(c: i32) -> HistoryType {
    if c == i32::from(b':') {
        HistoryType::Cmd
    } else if c == i32::from(b'=') {
        HistoryType::Expr
    } else if c == i32::from(b'@') {
        HistoryType::Input
    } else if c == i32::from(b'>') {
        HistoryType::Debug
    } else if c == 0 || c == i32::from(b'/') || c == i32::from(b'?') {
        HistoryType::Search
    } else {
        HistoryType::Invalid
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Test-only helper letting tests set the otherwise-private
    /// `HISLEN` value, matching the established `set_pum_is_visible`-
    /// style pattern (`popupmenu.rs`). Caller must hold
    /// `crate::globals::global_state_test_lock()` for the whole
    /// duration this value matters, and should restore the original
    /// value before releasing the lock.
    pub(crate) fn set_hislen(value: i32) -> i32 {
        let cell = unsafe { HISLEN.get_mut() };
        let old = *cell;
        *cell = value;
        old
    }

    #[test]
    fn get_hislen_is_zero_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(get_hislen(), 0);
    }

    #[test]
    fn get_hislen_reflects_the_underlying_value() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_hislen(42);
        assert_eq!(get_hislen(), 42);
        set_hislen(old);
    }

    #[test]
    fn hist_char2type_maps_every_known_character() {
        assert_eq!(hist_char2type(i32::from(b':')), HistoryType::Cmd);
        assert_eq!(hist_char2type(i32::from(b'=')), HistoryType::Expr);
        assert_eq!(hist_char2type(i32::from(b'@')), HistoryType::Input);
        assert_eq!(hist_char2type(i32::from(b'>')), HistoryType::Debug);
        assert_eq!(hist_char2type(i32::from(b'/')), HistoryType::Search);
        assert_eq!(hist_char2type(i32::from(b'?')), HistoryType::Search);
        assert_eq!(hist_char2type(0), HistoryType::Search);
    }

    #[test]
    fn hist_char2type_unknown_char_is_invalid() {
        assert_eq!(hist_char2type(i32::from(b'x')), HistoryType::Invalid);
        assert_eq!(hist_char2type(-1), HistoryType::Invalid);
    }
}
