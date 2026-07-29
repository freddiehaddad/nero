//! Translated from `src/nvim/clipboard.c` (tractable core only).
//!
//! `clipboard.c` implements the `'clipboard'`-option-driven system
//! clipboard integration (`"*"`/`"+"` registers), routed through a
//! Lua `g:clipboard` provider. Almost every function
//! (`adjust_clipboard_name`/`get_clipboard`/`set_clipboard`) needs
//! both the real yank-register storage (`register.c`'s `yankreg_T`,
//! only partially translated) and the Lua-callback provider machinery
//! (`eval_has_provider`/calling into `vim.g.clipboard`), neither of
//! which exists yet.
//!
//! Translated: `start_batch_changes`/`end_batch_changes` (the
//! recursion-depth-counted "pause clipboard updates during a
//! `:while`/`:for` loop" pair). `end_batch_changes`'s own real
//! "flush a pending update" branch (`set_clipboard(NUL,
//! get_y_previous())`) is never reached today: `clipboard_needs_update`
//! is only ever set `true` inside `adjust_clipboard_name`, not yet
//! translated, so it stays `false` forever in this crate today -
//! exactly matching `autocmd.rs`'s own `AU_NEED_CLEAN`/
//! `TERMRESPONSE_CHANGED` "provably always false today, not a
//! hardcoded stub" precedent.
//!
//! Deferred: everything else in the file.

use crate::globals::GlobalCell;

/// `batch_change_count` - nesting depth of [`start_batch_changes`]/
/// [`end_batch_changes`] calls.
static BATCH_CHANGE_COUNT: GlobalCell<i32> = GlobalCell::new(0);

/// `clipboard_delay_update` - whether clipboard updates are currently
/// deferred (inside a batch).
static CLIPBOARD_DELAY_UPDATE: GlobalCell<bool> = GlobalCell::new(false);

/// `clipboard_needs_update` - whether a clipboard update was deferred
/// and still needs to be flushed once the outermost batch ends.
/// Always `false` today: only ever set `true` by
/// `adjust_clipboard_name`, not yet translated (see this module's own
/// doc comment).
static CLIPBOARD_NEEDS_UPDATE: GlobalCell<bool> = GlobalCell::new(false);

/// Avoid slow things (clipboard) during batch operations (`:while`/
/// `:for` loops) (`start_batch_changes`).
pub fn start_batch_changes() {
    // SAFETY: single-threaded test/editor state, matching every other
    // `GlobalCell`-backed file-static in this crate.
    let count = unsafe { BATCH_CHANGE_COUNT.get_mut() };
    *count += 1;
    if *count > 1 {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *CLIPBOARD_DELAY_UPDATE.get_mut() = true };
}

/// Counterpart to [`start_batch_changes`] (`end_batch_changes`).
///
/// # Panics
/// If ending the outermost batch reveals a deferred clipboard update
/// is pending (`clipboard_needs_update` was set `true`) - this would
/// need the real yank-register + Lua-provider clipboard machinery
/// (`set_clipboard`/`get_y_previous`), neither translated. This is
/// unreachable today: nothing currently translated can ever set
/// `clipboard_needs_update` to `true` (see this module's own doc
/// comment).
pub fn end_batch_changes() {
    // SAFETY: forwarded from this function's own safety doc.
    let count = unsafe { BATCH_CHANGE_COUNT.get_mut() };
    *count -= 1;
    if *count > 0 {
        // recursive
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *CLIPBOARD_DELAY_UPDATE.get_mut() = false };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *CLIPBOARD_NEEDS_UPDATE.get_mut() } {
        // must be before, as set_clipboard would invoke
        // start/end_batch_changes recursively
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *CLIPBOARD_NEEDS_UPDATE.get_mut() = false };
        // unnamed ("implicit" clipboard)
        unimplemented!(
            "end_batch_changes: flushing a deferred clipboard update needs \
             set_clipboard/get_y_previous (register.c's real yank-register \
             storage plus the Lua clipboard-provider machinery), neither \
             translated - unreachable today since nothing can set \
             CLIPBOARD_NEEDS_UPDATE true yet"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    /// Resets every shared static this module touches back to its own
    /// real C static-initializer default, both before and after each
    /// test - matching this crate's own established
    /// `global_state_test_lock`-guarded reset convention (e.g.
    /// `autocmd.rs`'s own test module).
    fn reset() {
        unsafe {
            *BATCH_CHANGE_COUNT.get_mut() = 0;
            *CLIPBOARD_DELAY_UPDATE.get_mut() = false;
            *CLIPBOARD_NEEDS_UPDATE.get_mut() = false;
        }
    }

    #[test]
    fn start_batch_changes_sets_delay_update_on_first_call() {
        let _lock = global_state_test_lock();
        reset();
        start_batch_changes();
        assert_eq!(unsafe { *BATCH_CHANGE_COUNT.get_mut() }, 1);
        assert!(unsafe { *CLIPBOARD_DELAY_UPDATE.get_mut() });
        reset();
    }

    #[test]
    fn start_batch_changes_is_reentrant() {
        let _lock = global_state_test_lock();
        reset();
        start_batch_changes();
        start_batch_changes();
        start_batch_changes();
        assert_eq!(unsafe { *BATCH_CHANGE_COUNT.get_mut() }, 3);
        assert!(unsafe { *CLIPBOARD_DELAY_UPDATE.get_mut() });
        reset();
    }

    #[test]
    fn end_batch_changes_clears_delay_update_only_at_outermost_level() {
        let _lock = global_state_test_lock();
        reset();
        start_batch_changes();
        start_batch_changes();
        end_batch_changes();
        // Still nested one level deep - delay_update stays set.
        assert!(unsafe { *CLIPBOARD_DELAY_UPDATE.get_mut() });
        assert_eq!(unsafe { *BATCH_CHANGE_COUNT.get_mut() }, 1);

        end_batch_changes();
        // Back to the outermost level - delay_update is cleared.
        assert!(!unsafe { *CLIPBOARD_DELAY_UPDATE.get_mut() });
        assert_eq!(unsafe { *BATCH_CHANGE_COUNT.get_mut() }, 0);
        reset();
    }

    #[test]
    fn end_batch_changes_does_nothing_extra_when_no_update_is_pending() {
        let _lock = global_state_test_lock();
        reset();
        start_batch_changes();
        // clipboard_needs_update stays false (nothing can set it yet),
        // so this must NOT reach the unimplemented!() flush branch.
        end_batch_changes();
        assert_eq!(unsafe { *BATCH_CHANGE_COUNT.get_mut() }, 0);
        reset();
    }

    #[test]
    fn end_batch_changes_panics_if_an_update_is_pending() {
        let _lock = global_state_test_lock();
        reset();
        start_batch_changes();
        // Simulates what adjust_clipboard_name would have done, since
        // it isn't translated yet.
        unsafe { *CLIPBOARD_NEEDS_UPDATE.get_mut() = true };
        let result = std::panic::catch_unwind(end_batch_changes);
        reset();
        let err = result.expect_err("end_batch_changes should have panicked");
        let msg = err
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| err.downcast_ref::<String>().map(String::as_str))
            .unwrap_or_default();
        assert!(
            msg.contains("flushing a deferred clipboard update"),
            "unexpected panic message: {msg}"
        );
    }
}
