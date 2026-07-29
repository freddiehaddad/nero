//! Translated from `src/nvim/insexpand.c` (tractable core only).
//!
//! `insexpand.c` (~5500 lines) is the real insert-mode completion
//! engine (`i_CTRL-N`/`i_CTRL-P`/omni-completion/etc.) - almost every
//! function needs the popup menu, the completion-source dispatch
//! machinery, and real buffer/cursor mutation, none translated.
//!
//! Translated: the `CTRL_X_*` mode constants and the whole
//! `ctrl_x_mode_*` predicate family (19 pure, `FUNC_ATTR_PURE`
//! functions checking the current Ctrl-X completion sub-mode) plus
//! the `CTRL_X_MODE` file-static itself (`ctrl_x_mode`). All are
//! small, self-contained, no-design-freedom equality checks -
//! translated ahead of their real callers (`ins_ctrl_x`/the whole
//! completion-source dispatch, none translated), matching this
//! crate's established "translate a small, simple, mechanically-
//! correct piece ahead of the surrounding engine" precedent.
//!
//! Since `ins_ctrl_x` (the only real mutator of `ctrl_x_mode`) isn't
//! translated, `CTRL_X_MODE` stays `CTRL_X_NORMAL` (its own real
//! static initializer value) forever in this crate today - exactly
//! matching `state.rs`'s own already-documented assumption for
//! `get_mode`'s `ctrl_x_mode_not_defined_yet()` check (see that
//! function's own doc comment). `state.rs` could be refined to read
//! through `CTRL_X_MODE` directly instead of its own hardcoded
//! assumption as a low-risk future follow-up - not done here, to keep
//! this change scoped to `insexpand.c` itself.
//!
//! Deferred: everything else in the file.

use crate::globals::GlobalCell;

/// CTRL-N CTRL-P completion, default (`CTRL_X_NORMAL`).
pub const CTRL_X_NORMAL: i32 = 0;
/// `CTRL_X_NOT_DEFINED_YET`.
pub const CTRL_X_NOT_DEFINED_YET: i32 = 1;
/// `CTRL_X_SCROLL`.
pub const CTRL_X_SCROLL: i32 = 2;
/// `CTRL_X_WHOLE_LINE`.
pub const CTRL_X_WHOLE_LINE: i32 = 3;
/// `CTRL_X_FILES`.
pub const CTRL_X_FILES: i32 = 4;
/// Bit indicating the mode wants an identifier character class
/// (`CTRL_X_WANT_IDENT`).
const CTRL_X_WANT_IDENT: i32 = 0x100;
/// `CTRL_X_TAGS`.
pub const CTRL_X_TAGS: i32 = 5 + CTRL_X_WANT_IDENT;
/// `CTRL_X_PATH_PATTERNS`.
pub const CTRL_X_PATH_PATTERNS: i32 = 6 + CTRL_X_WANT_IDENT;
/// `CTRL_X_PATH_DEFINES`.
pub const CTRL_X_PATH_DEFINES: i32 = 7 + CTRL_X_WANT_IDENT;
/// `CTRL_X_FINISHED`.
pub const CTRL_X_FINISHED: i32 = 8;
/// `CTRL_X_DICTIONARY`.
pub const CTRL_X_DICTIONARY: i32 = 9 + CTRL_X_WANT_IDENT;
/// `CTRL_X_THESAURUS`.
pub const CTRL_X_THESAURUS: i32 = 10 + CTRL_X_WANT_IDENT;
/// `CTRL_X_CMDLINE`.
pub const CTRL_X_CMDLINE: i32 = 11;
/// `CTRL_X_FUNCTION`.
pub const CTRL_X_FUNCTION: i32 = 12;
/// `CTRL_X_OMNI`.
pub const CTRL_X_OMNI: i32 = 13;
/// `CTRL_X_SPELL`.
pub const CTRL_X_SPELL: i32 = 14;
/// Only used in `ctrl_x_msgs` (`CTRL_X_LOCAL_MSG`).
pub const CTRL_X_LOCAL_MSG: i32 = 15;
/// For the builtin `complete()` function (`CTRL_X_EVAL`).
pub const CTRL_X_EVAL: i32 = 16;
/// CTRL-X typed in [`CTRL_X_CMDLINE`] mode (`CTRL_X_CMDLINE_CTRL_X`).
pub const CTRL_X_CMDLINE_CTRL_X: i32 = 17;
/// `CTRL_X_BUFNAMES`.
pub const CTRL_X_BUFNAMES: i32 = 18;
/// Complete words from registers (`CTRL_X_REGISTER`).
pub const CTRL_X_REGISTER: i32 = 19;

/// Which Ctrl-X mode are we in? (`ctrl_x_mode`). Always
/// [`CTRL_X_NORMAL`] today - see this module's own doc comment.
static CTRL_X_MODE: GlobalCell<i32> = GlobalCell::new(CTRL_X_NORMAL);

/// # Safety
/// Single-threaded test/editor state, matching every other
/// `GlobalCell`-backed file-static in this crate.
unsafe fn ctrl_x_mode() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *CTRL_X_MODE.get_mut() }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_none() -> bool {
    unsafe { ctrl_x_mode() == 0 }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_normal() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_NORMAL }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_scroll() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_SCROLL }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_whole_line() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_WHOLE_LINE }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_files() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_FILES }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_tags() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_TAGS }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_path_patterns() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_PATH_PATTERNS }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_path_defines() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_PATH_DEFINES }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_dictionary() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_DICTIONARY }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_thesaurus() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_THESAURUS }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_cmdline() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_CMDLINE || ctrl_x_mode() == CTRL_X_CMDLINE_CTRL_X }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_function() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_FUNCTION }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_omni() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_OMNI }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_spell() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_SPELL }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_eval() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_EVAL }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_line_or_eval() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_WHOLE_LINE || ctrl_x_mode() == CTRL_X_EVAL }
}

/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_register() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_REGISTER }
}

/// Whether other than default completion has been selected.
///
/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_not_default() -> bool {
    unsafe { ctrl_x_mode() != CTRL_X_NORMAL }
}

/// Whether CTRL-X was typed without a following character, not
/// including when in `CTRL_X_CMDLINE_CTRL_X` mode.
///
/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ctrl_x_mode_not_defined_yet() -> bool {
    unsafe { ctrl_x_mode() == CTRL_X_NOT_DEFINED_YET }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    /// RAII guard temporarily overriding `CTRL_X_MODE`, restoring the
    /// previous value on drop (even on test panic).
    struct CtrlXModeGuard {
        prev: i32,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CtrlXModeGuard {
        fn set(value: i32) -> Self {
            let _lock = global_state_test_lock();
            let prev = unsafe { *CTRL_X_MODE.get_mut() };
            unsafe { *CTRL_X_MODE.get_mut() = value };
            Self { prev, _lock }
        }
    }

    impl Drop for CtrlXModeGuard {
        fn drop(&mut self) {
            unsafe { *CTRL_X_MODE.get_mut() = self.prev };
        }
    }

    #[test]
    fn defaults_to_ctrl_x_normal() {
        let _lock = global_state_test_lock();
        assert_eq!(unsafe { ctrl_x_mode() }, CTRL_X_NORMAL);
        assert!(unsafe { ctrl_x_mode_normal() });
        assert!(unsafe { ctrl_x_mode_none() }); // CTRL_X_NORMAL == 0
        assert!(!unsafe { ctrl_x_mode_not_default() });
    }

    #[test]
    fn ctrl_x_mode_scroll_matches_only_scroll() {
        let _guard = CtrlXModeGuard::set(CTRL_X_SCROLL);
        assert!(unsafe { ctrl_x_mode_scroll() });
        assert!(!unsafe { ctrl_x_mode_normal() });
        assert!(unsafe { ctrl_x_mode_not_default() });
    }

    #[test]
    fn ctrl_x_mode_whole_line_and_line_or_eval() {
        let _guard = CtrlXModeGuard::set(CTRL_X_WHOLE_LINE);
        assert!(unsafe { ctrl_x_mode_whole_line() });
        assert!(unsafe { ctrl_x_mode_line_or_eval() });
        assert!(!unsafe { ctrl_x_mode_eval() });
    }

    #[test]
    fn ctrl_x_mode_eval_and_line_or_eval() {
        let _guard = CtrlXModeGuard::set(CTRL_X_EVAL);
        assert!(unsafe { ctrl_x_mode_eval() });
        assert!(unsafe { ctrl_x_mode_line_or_eval() });
        assert!(!unsafe { ctrl_x_mode_whole_line() });
    }

    #[test]
    fn ctrl_x_mode_files_tags_path_patterns_path_defines() {
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_FILES);
            assert!(unsafe { ctrl_x_mode_files() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_TAGS);
            assert!(unsafe { ctrl_x_mode_tags() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_PATH_PATTERNS);
            assert!(unsafe { ctrl_x_mode_path_patterns() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_PATH_DEFINES);
            assert!(unsafe { ctrl_x_mode_path_defines() });
        }
    }

    #[test]
    fn ctrl_x_mode_dictionary_and_thesaurus() {
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_DICTIONARY);
            assert!(unsafe { ctrl_x_mode_dictionary() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_THESAURUS);
            assert!(unsafe { ctrl_x_mode_thesaurus() });
        }
    }

    #[test]
    fn ctrl_x_mode_cmdline_matches_both_cmdline_variants() {
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_CMDLINE);
            assert!(unsafe { ctrl_x_mode_cmdline() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_CMDLINE_CTRL_X);
            assert!(unsafe { ctrl_x_mode_cmdline() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
            assert!(!unsafe { ctrl_x_mode_cmdline() });
        }
    }

    #[test]
    fn ctrl_x_mode_function_omni_spell_register() {
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_FUNCTION);
            assert!(unsafe { ctrl_x_mode_function() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_OMNI);
            assert!(unsafe { ctrl_x_mode_omni() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_SPELL);
            assert!(unsafe { ctrl_x_mode_spell() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_REGISTER);
            assert!(unsafe { ctrl_x_mode_register() });
        }
    }

    #[test]
    fn ctrl_x_mode_not_defined_yet_matches_only_that_state() {
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_NOT_DEFINED_YET);
            assert!(unsafe { ctrl_x_mode_not_defined_yet() });
        }
        {
            let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
            assert!(!unsafe { ctrl_x_mode_not_defined_yet() });
        }
    }

    #[test]
    fn ctrl_x_constants_match_the_real_source_values() {
        // Mechanically re-derived from the original's own enum +
        // CTRL_X_WANT_IDENT = 0x100 definition, cross-checked directly
        // against the real source before trusting these.
        assert_eq!(CTRL_X_NORMAL, 0);
        assert_eq!(CTRL_X_NOT_DEFINED_YET, 1);
        assert_eq!(CTRL_X_SCROLL, 2);
        assert_eq!(CTRL_X_WHOLE_LINE, 3);
        assert_eq!(CTRL_X_FILES, 4);
        assert_eq!(CTRL_X_TAGS, 5 + 0x100);
        assert_eq!(CTRL_X_PATH_PATTERNS, 6 + 0x100);
        assert_eq!(CTRL_X_PATH_DEFINES, 7 + 0x100);
        assert_eq!(CTRL_X_FINISHED, 8);
        assert_eq!(CTRL_X_DICTIONARY, 9 + 0x100);
        assert_eq!(CTRL_X_THESAURUS, 10 + 0x100);
        assert_eq!(CTRL_X_CMDLINE, 11);
        assert_eq!(CTRL_X_FUNCTION, 12);
        assert_eq!(CTRL_X_OMNI, 13);
        assert_eq!(CTRL_X_SPELL, 14);
        assert_eq!(CTRL_X_LOCAL_MSG, 15);
        assert_eq!(CTRL_X_EVAL, 16);
        assert_eq!(CTRL_X_CMDLINE_CTRL_X, 17);
        assert_eq!(CTRL_X_BUFNAMES, 18);
        assert_eq!(CTRL_X_REGISTER, 19);
    }
}
