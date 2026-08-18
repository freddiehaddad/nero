//! Translated from `src/nvim/os/input.c` (blocking-state core).
//!
//! The event-loop, input buffering, mouse decoding, and stream
//! callbacks remain deferred. [`input_blocking`] is independently
//! complete and is needed by `nvim_get_mode`.

use crate::globals::GlobalCell;

/// Whether the main loop is waiting for input (`blocking`).
static BLOCKING: GlobalCell<bool> = GlobalCell::new(false);

/// Whether the main loop is blocked waiting for input
/// (`input_blocking`).
///
/// # Safety
/// Reads the shared `blocking` file-static.
#[must_use]
pub unsafe fn input_blocking() -> bool {
    unsafe { *BLOCKING.get_mut() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_blocking_reflects_the_file_static() {
        let _lock = crate::globals::global_state_test_lock();
        let previous = unsafe { *BLOCKING.get_mut() };
        unsafe { *BLOCKING.get_mut() = false };
        let unblocked = unsafe { input_blocking() };
        unsafe { *BLOCKING.get_mut() = true };
        let blocked = unsafe { input_blocking() };
        unsafe { *BLOCKING.get_mut() = previous };
        assert!(!unblocked);
        assert!(blocked);
    }
}
