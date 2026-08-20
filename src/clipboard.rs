//! Translated from `src/nvim/clipboard.c` (tractable core only).
//!
//! `clipboard.c` implements the `'clipboard'`-option-driven system
//! clipboard integration (`"*"`/`"+"` registers), routed through a
//! Lua `g:clipboard` provider. Most of the file
//! (`get_clipboard`/`set_clipboard`) needs both the real yank-register
//! storage (`register.c`'s `yankreg_T`, only partially translated) and
//! the Lua-callback provider machinery (`eval_has_provider`/calling
//! into `vim.g.clipboard`), neither of which exists yet.
//!
//! Translated: `start_batch_changes`/`end_batch_changes` (the
//! recursion-depth-counted "pause clipboard updates during a
//! `:while`/`:for` loop" pair). `end_batch_changes`'s own real
//! "flush a pending update" branch (`set_clipboard(NUL,
//! get_y_previous())`) is never reached today: `clipboard_needs_update`
//! is only ever set `true` inside `adjust_clipboard_name`, so it stays
//! `false` forever in this crate today - exactly matching
//! `autocmd.rs`'s own `AU_NEED_CLEAN`/`TERMRESPONSE_CHANGED` "provably
//! always false today, not a hardcoded stub" precedent.
//!
//! Also translated: [`adjust_clipboard_name`], including explicit and
//! `'clipboard'`-driven implicit registers. With no Lua host/provider,
//! these take the original's real "No provider" path and return no
//! target without changing the register name.
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
static CLIPBOARD_DIDWARN: GlobalCell<bool> = GlobalCell::new(false);

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

/// Determine if register `*name` should be used as a clipboard
/// (`adjust_clipboard_name`). Returns `Some(target)` (the clipboard
/// register that should be used) when `*name` is a clipboard register
/// AND a provider is available, else `None` - possibly having updated
/// `*name` along the way.
///
/// # Safety
/// Touches `OPTION_VARS`.
pub unsafe fn adjust_clipboard_name(
    name: &mut i32,
    quiet: bool,
    _writing: bool,
) -> Option<*mut crate::register_defs::YankregT> {
    let explicit_cb_reg = *name == i32::from(b'*') || *name == i32::from(b'+');
    // SAFETY: forwarded from this function's own safety doc.
    let cb_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags;
    let implicit_cb_reg = *name == 0
        && (cb_flags & (crate::option_vars::opt_cb_flag::UNNAMED | crate::option_vars::opt_cb_flag::UNNAMEDPLUS))
            != 0;
    if !explicit_cb_reg && !implicit_cb_reg {
        return None;
    }

    let count = unsafe { *BATCH_CHANGE_COUNT.get_mut() };
    let didwarn = unsafe { *CLIPBOARD_DIDWARN.get_mut() };
    if count <= 1
        && !quiet
        && (!didwarn || (explicit_cb_reg && !crate::message::redirecting()))
    {
        unsafe { *CLIPBOARD_DIDWARN.get_mut() = true };
    }
    None
}

/// Read a clipboard register through the configured provider
/// (`get_clipboard`).
///
/// Without a Lua clipboard provider, [`adjust_clipboard_name`] returns
/// no target and this faithfully reports `false`.
///
/// # Safety
/// Forwards [`adjust_clipboard_name`]'s shared option-state access.
pub unsafe fn get_clipboard(
    name: i32,
    _target: Option<&mut *mut crate::register_defs::YankregT>,
    quiet: bool,
) -> bool {
    let mut adjusted = name;
    let Some(_register) =
        (unsafe { adjust_clipboard_name(&mut adjusted, quiet, false) })
    else {
        return false;
    };
    unimplemented!(
        "get_clipboard: provider results need eval_call_provider"
    );
}

/// Publish a register through the configured clipboard provider
/// (`set_clipboard`).
///
/// Without a Lua clipboard provider, this is the original's no-op
/// path.
///
/// # Safety
/// Forwards [`adjust_clipboard_name`]'s shared option-state access.
pub unsafe fn set_clipboard(
    name: i32,
    _register: &crate::register_defs::YankregT,
) {
    let mut adjusted = name;
    let Some(_target) =
        (unsafe { adjust_clipboard_name(&mut adjusted, false, true) })
    else {
        return;
    };
    unimplemented!("set_clipboard: provider calls need eval_call_provider");
}

/// Whether the default register (used for an unnamed paste) should be
/// a clipboard register (`get_default_register_name`). Always `0`
/// (`NUL`) today - see [`adjust_clipboard_name`]'s own doc comment.
///
/// # Safety
/// Touches `OPTION_VARS` (via [`adjust_clipboard_name`]).
#[must_use]
pub unsafe fn get_default_register_name() -> i32 {
    let mut name = 0;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { adjust_clipboard_name(&mut name, true, false) };
    name
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
            *CLIPBOARD_DIDWARN.get_mut() = false;
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
        // Simulates the "unnamed register, deferred write" branch
        // `adjust_clipboard_name` would take if it ever reached its
        // own genuinely-unreachable-today body (see this module's own
        // doc comment) - directly exercised here since no currently
        // possible call sequence can reach it for real.
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

    // --- adjust_clipboard_name / get_default_register_name ---

    #[test]
    fn adjust_clipboard_name_is_none_for_an_ordinary_register_with_cb_flags_unset() {
        let _lock = global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags = 0;

        let mut name = i32::from(b'a');
        assert!(unsafe { adjust_clipboard_name(&mut name, true, false) }.is_none());
        assert_eq!(name, i32::from(b'a'));

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags = prev;
    }

    #[test]
    fn adjust_clipboard_name_is_none_for_an_unnamed_register_with_cb_flags_unset() {
        let _lock = global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags = 0;

        let mut name = 0;
        assert!(unsafe { adjust_clipboard_name(&mut name, true, false) }.is_none());
        assert_eq!(name, 0);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags = prev;
    }

    #[test]
    fn adjust_clipboard_name_rejects_explicit_register_without_provider() {
        let _lock = global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags = 0;

        let mut name = i32::from(b'*');
        assert!(unsafe {
            adjust_clipboard_name(&mut name, true, false)
        }
        .is_none());
        assert_eq!(name, i32::from(b'*'));

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags = prev;
    }

    #[test]
    fn adjust_clipboard_name_rejects_implicit_register_without_provider() {
        let _lock = global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags = crate::option_vars::opt_cb_flag::UNNAMED;

        let mut name = 0;
        assert!(unsafe {
            adjust_clipboard_name(&mut name, true, false)
        }
        .is_none());
        assert_eq!(name, 0);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags = prev;
    }

    #[test]
    fn adjust_clipboard_name_records_one_unsuppressed_warning() {
        let _lock = global_state_test_lock();
        reset();
        let mut name = i32::from(b'+');

        assert!(unsafe {
            adjust_clipboard_name(&mut name, false, false)
        }
        .is_none());

        assert!(unsafe { *CLIPBOARD_DIDWARN.get_mut() });
        reset();
    }

    #[test]
    fn adjust_clipboard_name_suppresses_warnings_in_nested_batches() {
        let _lock = global_state_test_lock();
        reset();
        start_batch_changes();
        start_batch_changes();
        let mut name = i32::from(b'*');

        assert!(unsafe {
            adjust_clipboard_name(&mut name, false, false)
        }
        .is_none());

        assert!(!unsafe { *CLIPBOARD_DIDWARN.get_mut() });
        reset();
    }

    #[test]
    fn get_default_register_name_is_nul_by_default() {
        let _lock = global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags = 0;

        assert_eq!(unsafe { get_default_register_name() }, 0);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags = prev;
    }

    #[test]
    fn get_and_set_clipboard_take_no_provider_fallbacks() {
        let _lock = global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags =
            crate::option_vars::opt_cb_flag::UNNAMED;
        let register = crate::register_defs::YankregT {
            y_array: Some(vec![b"value".to_vec()]),
            ..Default::default()
        };

        assert!(!unsafe { get_clipboard(0, None, true) });
        unsafe { set_clipboard(0, &register) };
        assert_eq!(
            register.y_array.as_deref(),
            Some([b"value".to_vec()].as_slice())
        );

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cb_flags = prev;
    }
}
