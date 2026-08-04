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
//! the `CTRL_X_MODE` file-static itself (`ctrl_x_mode`), and now
//! [`ins_compl_active`] plus its own backing `COMPL_STARTED`
//! file-static (`compl_started`). All are small, self-contained,
//! no-design-freedom equality checks - translated ahead of their real
//! callers (`ins_ctrl_x`/the whole completion-source dispatch, none
//! translated), matching this crate's established "translate a small,
//! simple, mechanically-correct piece ahead of the surrounding engine"
//! precedent.
//!
//! Since `ins_ctrl_x` (the only real mutator of `ctrl_x_mode`) isn't
//! translated, `CTRL_X_MODE` stays `CTRL_X_NORMAL` (its own real
//! static initializer value) forever in this crate today, and
//! `COMPL_STARTED` likewise stays `false` forever (its own only real
//! mutator is the same not-yet-translated completion engine) -
//! exactly matching `state.rs`'s own already-documented assumption
//! for `get_mode`'s `ins_compl_active()`/
//! `ctrl_x_mode_not_defined_yet()` checks (see that function's own
//! doc comment) - `state.rs`'s own `get_mode` has now been refined to
//! call these 2 real predicates directly instead of its own
//! hardcoded-false assumption, since both now exist for real.
//!
//! Also translated: [`set_ref_in_cpt_callbacks`]/
//! [`set_ref_in_insexpand_funcs`] - mark the global `'completefunc'`/
//! `'omnifunc'`/`'thesaurusfunc'`/`'complete'`-`F{func}` callbacks with
//! a GC `copy_id` so they survive garbage collection, via
//! `eval/eval.rs`'s `set_ref_in_callback`. Every one of `CFU_CB`/
//! `OFU_CB`/`TSRFU_CB`/`CPT_CB` stays at its own empty default forever
//! today (see each one's own doc comment) - matches every real,
//! unconfigured session.
//!
//! Also translated, from `insexpand.h` (not `insexpand.c` - a tiny,
//! self-contained enum needed by `popupmenu.c`'s `pum_align_order`):
//! [`CPT_ABBR`]/[`CPT_KIND`]/[`CPT_MENU`]/[`CPT_INFO`]/[`CPT_COUNT`].
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

/// Indices into a completion match's own `cp_text` array, and into
/// `'completeitemalign'`'s own display-order array (`CPT_*`,
/// `insexpand.h`). Kept as `i32` (matching the original's own plain C
/// `enum`, implicitly `int`) rather than `usize`, since real callers
/// use these both as array indices and in `int`-typed arithmetic
/// (e.g. `popupmenu.rs`'s `pum_align_order`, comparing against
/// `cia_flags / 100`).
pub const CPT_ABBR: i32 = 0;
/// (`CPT_KIND`).
pub const CPT_KIND: i32 = 1;
/// (`CPT_MENU`).
pub const CPT_MENU: i32 = 2;
/// (`CPT_INFO`).
pub const CPT_INFO: i32 = 3;
/// Number of `CPT_*` entries (`CPT_COUNT`).
pub const CPT_COUNT: i32 = 4;

/// Which Ctrl-X mode are we in? (`ctrl_x_mode`). Always
/// [`CTRL_X_NORMAL`] today - see this module's own doc comment.
static CTRL_X_MODE: GlobalCell<i32> = GlobalCell::new(CTRL_X_NORMAL);

/// Whether Insert-mode completion is currently active (`compl_started`).
/// Always `false` today - nothing in this crate can currently start a
/// real completion session (the only real mutator, `ins_ctrl_x`/the
/// completion-source dispatch machinery, isn't translated), matching
/// [`CTRL_X_MODE`]'s own established treatment exactly.
static COMPL_STARTED: GlobalCell<bool> = GlobalCell::new(false);

/// Check that Insert-mode completion is active (`ins_compl_active`).
///
/// # Safety
/// Same as `ctrl_x_mode()`.
#[must_use]
pub unsafe fn ins_compl_active() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *COMPL_STARTED.get_mut() }
}

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

/// The `'completefunc'` callback (`cfu_cb`, a file-static `Callback`).
/// Nothing in this crate can currently set a real value here - see
/// `ops.rs`'s `OPFUNC_CB` for the identical reasoning (needs
/// `option_set_callback_func`, not translated).
static CFU_CB: GlobalCell<crate::eval::typval_defs::Callback> =
    GlobalCell::new(crate::eval::typval_defs::Callback::None);

/// The `'omnifunc'` callback (`ofu_cb`). See [`CFU_CB`].
static OFU_CB: GlobalCell<crate::eval::typval_defs::Callback> =
    GlobalCell::new(crate::eval::typval_defs::Callback::None);

/// The `'thesaurusfunc'` callback (`tsrfu_cb`). See [`CFU_CB`].
static TSRFU_CB: GlobalCell<crate::eval::typval_defs::Callback> =
    GlobalCell::new(crate::eval::typval_defs::Callback::None);

/// The `'completefunc'`-style callbacks associated with each `F{func}`
/// entry in `'complete'`/`'completeopt'` (`cpt_cb`/`cpt_cb_count`
/// collapsed into one `Vec`, matching this crate's established
/// "translate a `T*`+count pair as a `Vec<T>` when nothing needs the
/// original's raw-pointer/manual-count shape" precedent - e.g.
/// `runtime.rs`'s own `SCRIPT_ITEMS`). Always empty today: nothing in
/// this crate can currently populate it.
static CPT_CB: GlobalCell<Vec<crate::eval::typval_defs::Callback>> = GlobalCell::new(Vec::new());

/// Mark `copy_id` on every callback in `callbacks` so none of them are
/// garbage collected (`set_ref_in_cpt_callbacks`).
///
/// Uses `||`'s own short-circuit evaluation (matching the original's
/// `abort = abort || set_ref_in_callback(...)`): once `abort` becomes
/// `true`, later callbacks in `callbacks` are NOT visited at all for
/// this call - a faithful translation of the original's real
/// structure, even though nothing in this crate can currently make
/// `abort` become `true` at all (every callback here is always
/// [`crate::eval::typval_defs::Callback::None`] today).
///
/// # Safety
/// Same as [`crate::eval::eval::set_ref_in_callback`].
pub unsafe fn set_ref_in_cpt_callbacks(
    callbacks: &[crate::eval::typval_defs::Callback],
    copy_id: i32,
) -> bool {
    let mut abort = false;
    for cb in callbacks {
        abort = abort
            // SAFETY: forwarded from this function's own safety doc.
            || unsafe {
                crate::eval::eval::set_ref_in_callback(cb, copy_id, std::ptr::null_mut(), std::ptr::null_mut())
            };
    }
    abort
}

/// Mark the global `'completefunc'`/`'omnifunc'`/`'thesaurusfunc'`
/// callbacks, plus every `F{func}` callback in `'complete'`, with
/// `copy_id` so none of them are garbage collected
/// (`set_ref_in_insexpand_funcs`).
///
/// # Safety
/// Same as [`crate::eval::eval::set_ref_in_callback`].
pub unsafe fn set_ref_in_insexpand_funcs(copy_id: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let cfu = unsafe { &*CFU_CB.as_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let ofu = unsafe { &*OFU_CB.as_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let tsrfu = unsafe { &*TSRFU_CB.as_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let cpt = unsafe { &*CPT_CB.as_ptr() };

    // SAFETY: forwarded from this function's own safety doc.
    let mut abort = unsafe {
        crate::eval::eval::set_ref_in_callback(cfu, copy_id, std::ptr::null_mut(), std::ptr::null_mut())
    };
    abort = abort
        // SAFETY: forwarded from this function's own safety doc.
        || unsafe {
            crate::eval::eval::set_ref_in_callback(ofu, copy_id, std::ptr::null_mut(), std::ptr::null_mut())
        };
    abort = abort
        // SAFETY: forwarded from this function's own safety doc.
        || unsafe {
            crate::eval::eval::set_ref_in_callback(tsrfu, copy_id, std::ptr::null_mut(), std::ptr::null_mut())
        };
    abort = abort || unsafe { set_ref_in_cpt_callbacks(cpt, copy_id) };

    abort
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
    fn ins_compl_active_defaults_to_false() {
        let _lock = global_state_test_lock();
        assert!(!unsafe { ins_compl_active() });
    }

    #[test]
    fn ins_compl_active_reflects_compl_started() {
        // Directly manipulate the file-static (something no real,
        // translated caller can currently do, since nothing starts a
        // real completion session yet) to prove ins_compl_active
        // reads the REAL value, not a hardcoded false.
        let _lock = global_state_test_lock();
        unsafe { *COMPL_STARTED.get_mut() = true };
        assert!(unsafe { ins_compl_active() });
        unsafe { *COMPL_STARTED.get_mut() = false };
        assert!(!unsafe { ins_compl_active() });
    }

    #[test]
    fn get_mode_insert_reports_c_when_completion_is_active() {
        // Proves state.rs's get_mode() now calls the REAL
        // ins_compl_active() (wired in this same update), not a
        // hardcoded false - manipulates COMPL_STARTED directly, only
        // possible from within this same module (it's a private
        // static), so this test lives here rather than in state.rs.
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut crate::buffer_defs::BufT;
        let prev_state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::INSERT as i32;
        unsafe { *COMPL_STARTED.get_mut() = true };

        let result = unsafe { crate::state::get_mode() };

        unsafe { *COMPL_STARTED.get_mut() = false };
        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;

        assert_eq!(result, b"ic".to_vec());
    }

    #[test]
    fn get_mode_insert_reports_x_when_ctrl_x_mode_not_defined_yet() {
        let _guard = CtrlXModeGuard::set(CTRL_X_NOT_DEFINED_YET);
        let mut buf = crate::buffer_defs::BufT::default();
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut crate::buffer_defs::BufT;
        let prev_state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::INSERT as i32;

        let result = unsafe { crate::state::get_mode() };

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;

        assert_eq!(result, b"ix".to_vec());
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

    #[test]
    fn set_ref_in_insexpand_funcs_is_always_false_since_every_callback_stays_empty() {
        // Nothing in this crate can populate CFU_CB/OFU_CB/TSRFU_CB/
        // CPT_CB with a real callback yet (needs
        // option_set_callback_func) - they always stay at their own
        // empty defaults, matching a real, unconfigured session.
        let _lock = global_state_test_lock();
        assert!(!unsafe { set_ref_in_insexpand_funcs(1) });
    }

    #[test]
    fn set_ref_in_cpt_callbacks_empty_slice_is_always_false() {
        assert!(!unsafe { set_ref_in_cpt_callbacks(&[], 1) });
    }

    #[test]
    fn set_ref_in_cpt_callbacks_none_callbacks_are_always_false() {
        let callbacks = [
            crate::eval::typval_defs::Callback::None,
            crate::eval::typval_defs::Callback::Funcref(b"MyFunc".to_vec()),
        ];
        assert!(!unsafe { set_ref_in_cpt_callbacks(&callbacks, 1) });
    }
}
