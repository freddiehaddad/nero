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
//! Also translated: [`get_cot_flags`] and the `'completeopt'`
//! predicates over it - [`cot_fuzzy`]/[`is_nearest_active`]/
//! [`ins_compl_preinsert_longest`] - plus the `compl_autocomplete`
//! static they consult. Note `ins_compl_preinsert_longest` masks
//! `longest|preinsert|fuzzy` and compares the WHOLE result against
//! `longest`, so it is true only when `longest` is set WITHOUT either
//! companion - not merely when `longest` is present.
//!
//! Also translated: the completion-continuation state
//! (`compl_cont_status` with its `CONT_*` flags, plus
//! `compl_direction`/`compl_shows_dir`) and the small predicates over
//! it - [`compl_status_adding`]/[`compl_status_sol`]/
//! [`compl_status_local`]/[`compl_status_clear`]/[`compl_dir_forward`]/
//! [`compl_shows_dir_forward`]/[`compl_shows_dir_backward`]. These
//! stay at their initial values today for the same reason as
//! `CTRL_X_MODE`/`COMPL_STARTED` above. Note
//! `compl_shows_dir_backward` is NOT the negation of
//! `compl_shows_dir_forward`: `Direction` also carries the
//! `FORWARD_FILE`/`BACKWARD_FILE` values, so both can be false.
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

/// Whether autocompletion is active (`compl_autocomplete`).
///
/// Stays `false` today for the same reason as the statics above: the
/// completion engine that sets it is not translated.
static COMPL_AUTOCOMPLETE: GlobalCell<bool> = GlobalCell::new(false);

/// Get the local or global value of `'completeopt'` flags
/// (`get_cot_flags`).
///
/// A buffer-local value of `0` means "unset", so the global value is
/// used - the original spells this as a plain `!= 0` test rather than
/// tracking whether the local option was ever assigned.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer,
/// and this must not run concurrently with any write to
/// `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn get_cot_flags() -> u32 {
    // SAFETY: forwarded from this function's own safety doc.
    let local = unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_cot_flags };
    if local != 0 {
        return local;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cot_flags
}

/// Whether fuzzy matching is enabled (`cot_fuzzy`).
///
/// Thesaurus completion opts out, since its matches are looked up
/// rather than filtered.
///
/// # Safety
/// Forwarded from [`get_cot_flags`].
#[must_use]
pub unsafe fn cot_fuzzy() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { get_cot_flags() }) & crate::option_vars::opt_cot_flag::FUZZY != 0
        // SAFETY: forwarded from this function's own safety doc.
        && !unsafe { ctrl_x_mode_thesaurus() }
}

/// Whether matches should be sorted by proximity to the cursor
/// (`is_nearest_active`).
///
/// Fuzzy matching wins outright, since it imposes its own ordering.
///
/// # Safety
/// Forwarded from [`get_cot_flags`].
#[must_use]
pub unsafe fn is_nearest_active() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let autocomplete = unsafe { *COMPL_AUTOCOMPLETE.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let nearest = unsafe { get_cot_flags() } & crate::option_vars::opt_cot_flag::NEAREST != 0;
    // SAFETY: forwarded from this function's own safety doc.
    (autocomplete || nearest) && !unsafe { cot_fuzzy() }
}

/// Whether autocomplete is active and the pre-insert effect targets
/// the longest prefix (`ins_compl_preinsert_longest`).
///
/// The original masks `longest|preinsert|fuzzy` and compares the whole
/// result against `longest` alone, so this is true ONLY when
/// `'completeopt'` has `longest` WITHOUT either `preinsert` or
/// `fuzzy` - not merely when `longest` is present.
///
/// # Safety
/// Forwarded from [`get_cot_flags`].
#[must_use]
pub unsafe fn ins_compl_preinsert_longest() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { *COMPL_AUTOCOMPLETE.get_mut() } {
        return false;
    }
    use crate::option_vars::opt_cot_flag::{FUZZY, LONGEST, PREINSERT};
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { get_cot_flags() }) & (LONGEST | PREINSERT | FUZZY) == LONGEST
}

/// "normal" or "adding" expansion (`CONT_ADDING`).
pub const CONT_ADDING: i32 = 1;
/// A `^X` interrupted the current expansion (`CONT_INTRPT`).
///
/// Deliberately `2 + 4` in the original: it implies
/// [`CONT_N_ADDS`], so testing for it also reports "next `^X<>` will
/// add-new or expand-current".
pub const CONT_INTRPT: i32 = 2 + 4;
/// Next `^X<>` will add-new or expand-current (`CONT_N_ADDS`).
pub const CONT_N_ADDS: i32 = 4;
/// Next `^X<>` will set the initial position (`CONT_S_IPOS`).
pub const CONT_S_IPOS: i32 = 8;
/// Pattern includes start of line, just for word-wise expansion
/// (`CONT_SOL`).
pub const CONT_SOL: i32 = 16;
/// For `ctrl_x_mode` 0, `^X^P`/`^X^N` do a local completion
/// (`CONT_LOCAL`).
pub const CONT_LOCAL: i32 = 32;

/// Flags tracking how the current completion continues
/// (`compl_cont_status`).
///
/// Stays `0` in this crate today for the same reason as
/// [`CTRL_X_MODE`]/[`COMPL_STARTED`]: nothing translated yet can start
/// a real completion session to set it.
static COMPL_CONT_STATUS: GlobalCell<i32> = GlobalCell::new(0);

/// Direction the completion is searching in (`compl_direction`).
static COMPL_DIRECTION: GlobalCell<crate::vim_defs::Direction> =
    GlobalCell::new(crate::vim_defs::Direction::Forward);

/// Direction whose matches are currently being shown
/// (`compl_shows_dir`).
///
/// Tracked separately from [`COMPL_DIRECTION`] because the displayed
/// direction can differ from the one being searched.
static COMPL_SHOWS_DIR: GlobalCell<crate::vim_defs::Direction> =
    GlobalCell::new(crate::vim_defs::Direction::Forward);

/// Whether in "normal" or "adding" insert completion matches state
/// (`compl_status_adding`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_CONT_STATUS`.
#[must_use]
pub unsafe fn compl_status_adding() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_CONT_STATUS.get_mut() }) & CONT_ADDING != 0
}

/// Whether the completion pattern includes the start of the line, just
/// for word-wise expansion (`compl_status_sol`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_CONT_STATUS`.
#[must_use]
pub unsafe fn compl_status_sol() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_CONT_STATUS.get_mut() }) & CONT_SOL != 0
}

/// Whether `^X^P`/`^X^N` will do a local completion, i.e. use
/// `complete=.` (`compl_status_local`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_CONT_STATUS`.
#[must_use]
pub unsafe fn compl_status_local() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_CONT_STATUS.get_mut() }) & CONT_LOCAL != 0
}

/// Clear the completion status flags (`compl_status_clear`).
///
/// # Safety
/// Must not run concurrently with any other access to
/// `COMPL_CONT_STATUS`.
pub unsafe fn compl_status_clear() {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { COMPL_CONT_STATUS.get_mut() } = 0;
}

/// Whether completion is using the forward direction matches
/// (`compl_dir_forward`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_DIRECTION`.
#[must_use]
pub unsafe fn compl_dir_forward() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_DIRECTION.get_mut() }) == crate::vim_defs::Direction::Forward
}

/// Whether forward completion matches are currently being shown
/// (`compl_shows_dir_forward`).
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_SHOWS_DIR`.
#[must_use]
pub unsafe fn compl_shows_dir_forward() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_SHOWS_DIR.get_mut() }) == crate::vim_defs::Direction::Forward
}

/// Whether backward completion matches are currently being shown
/// (`compl_shows_dir_backward`).
///
/// Note this is NOT the negation of [`compl_shows_dir_forward`]: the
/// original's `Direction` also has the `FORWARD_FILE`/`BACKWARD_FILE`
/// values, so both can be false at once.
///
/// # Safety
/// Must not run concurrently with any write to `COMPL_SHOWS_DIR`.
#[must_use]
pub unsafe fn compl_shows_dir_backward() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *COMPL_SHOWS_DIR.get_mut() }) == crate::vim_defs::Direction::Backward
}

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

    /// RAII guard temporarily overriding the completion continuation
    /// state, restoring the previous values on drop (even on panic).
    struct ComplStateGuard {
        prev_status: i32,
        prev_dir: crate::vim_defs::Direction,
        prev_shows: crate::vim_defs::Direction,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl ComplStateGuard {
        fn new() -> Self {
            let _lock = global_state_test_lock();
            Self {
                prev_status: unsafe { *COMPL_CONT_STATUS.get_mut() },
                prev_dir: unsafe { *COMPL_DIRECTION.get_mut() },
                prev_shows: unsafe { *COMPL_SHOWS_DIR.get_mut() },
                _lock,
            }
        }
    }

    impl Drop for ComplStateGuard {
        fn drop(&mut self) {
            unsafe {
                *COMPL_CONT_STATUS.get_mut() = self.prev_status;
                *COMPL_DIRECTION.get_mut() = self.prev_dir;
                *COMPL_SHOWS_DIR.get_mut() = self.prev_shows;
            }
        }
    }

    /// Installs a buffer as `curbuf` for the test's duration and
    /// restores the previous one on drop, so a failing assertion
    /// cannot leave a dangling pointer behind.
    struct CurbufGuard {
        prev: *mut crate::buffer_defs::BufT,
    }

    impl CurbufGuard {
        fn set(buf: &mut crate::buffer_defs::BufT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev = globals.curbuf;
            globals.curbuf = buf as *mut crate::buffer_defs::BufT;
            Self { prev }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.prev;
        }
    }

    /// Saves and restores the global `'completeopt'` flags.
    struct CotFlagsGuard {
        saved: u32,
    }

    impl CotFlagsGuard {
        fn set(flags: u32) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let saved = opts.cot_flags;
            opts.cot_flags = flags;
            Self { saved }
        }
    }

    impl Drop for CotFlagsGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cot_flags = self.saved;
        }
    }

    #[test]
    fn get_cot_flags_prefers_a_nonzero_buffer_local_value() {
        let _lock = global_state_test_lock();
        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::MENU);
        let mut buf = crate::buffer_defs::BufT {
            b_cot_flags: crate::option_vars::opt_cot_flag::FUZZY,
            ..Default::default()
        };
        let _curbuf = CurbufGuard::set(&mut buf);

        assert_eq!(unsafe { get_cot_flags() }, crate::option_vars::opt_cot_flag::FUZZY);
    }

    #[test]
    fn get_cot_flags_falls_back_to_the_global_when_local_is_zero() {
        let _lock = global_state_test_lock();
        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::MENU);
        // Zero means "unset" here, not "no flags".
        let mut buf = crate::buffer_defs::BufT { b_cot_flags: 0, ..Default::default() };
        let _curbuf = CurbufGuard::set(&mut buf);

        assert_eq!(unsafe { get_cot_flags() }, crate::option_vars::opt_cot_flag::MENU);
    }

    #[test]
    fn cot_fuzzy_follows_the_fuzzy_flag() {
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);

        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::FUZZY);
        assert!(unsafe { cot_fuzzy() });
        drop(_cot);

        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::MENU);
        assert!(!unsafe { cot_fuzzy() });
    }

    #[test]
    fn cot_fuzzy_is_off_in_thesaurus_mode() {
        // Thesaurus matches are looked up rather than filtered, so
        // fuzzy matching opts out even with the flag set.
        let _guard = CtrlXModeGuard::set(CTRL_X_THESAURUS);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::FUZZY);

        assert!(unsafe { ctrl_x_mode_thesaurus() });
        assert!(!unsafe { cot_fuzzy() });
    }

    #[test]
    fn is_nearest_active_follows_the_nearest_flag() {
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);

        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::NEAREST);
        assert!(unsafe { is_nearest_active() });
        drop(_cot);

        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::MENU);
        assert!(!unsafe { is_nearest_active() });
    }

    #[test]
    fn is_nearest_active_yields_to_fuzzy_matching() {
        // Fuzzy imposes its own ordering, so it wins outright.
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        let _cot = CotFlagsGuard::set(
            crate::option_vars::opt_cot_flag::NEAREST | crate::option_vars::opt_cot_flag::FUZZY,
        );

        assert!(!unsafe { is_nearest_active() });
    }

    #[test]
    fn ins_compl_preinsert_longest_needs_autocomplete() {
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        let _cot = CotFlagsGuard::set(crate::option_vars::opt_cot_flag::LONGEST);

        // Autocomplete is off by default, so the flag alone is not
        // enough.
        assert!(!unsafe { ins_compl_preinsert_longest() });

        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = true };
        assert!(unsafe { ins_compl_preinsert_longest() });
        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = false };
    }

    #[test]
    fn ins_compl_preinsert_longest_wants_longest_alone() {
        let _guard = CtrlXModeGuard::set(CTRL_X_NORMAL);
        let mut buf = crate::buffer_defs::BufT::default();
        let _curbuf = CurbufGuard::set(&mut buf);
        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = true };

        use crate::option_vars::opt_cot_flag::{FUZZY, LONGEST, MENU, PREINSERT};
        // The original masks longest|preinsert|fuzzy and compares the
        // whole result to longest, so either companion flag disables
        // it - it is not merely "is longest set".
        for (flags, expected) in [
            (LONGEST, true),
            // An unrelated flag does not disturb the masked compare.
            (LONGEST | MENU, true),
            (LONGEST | PREINSERT, false),
            (LONGEST | FUZZY, false),
            (PREINSERT, false),
            (0, false),
        ] {
            let _cot = CotFlagsGuard::set(flags);
            assert_eq!(
                unsafe { ins_compl_preinsert_longest() },
                expected,
                "flags {flags:#x}"
            );
        }

        unsafe { *COMPL_AUTOCOMPLETE.get_mut() = false };
    }

    #[test]
    fn compl_cont_status_defaults_to_no_flags_set() {
        let _guard = ComplStateGuard::new();
        assert!(!unsafe { compl_status_adding() });
        assert!(!unsafe { compl_status_sol() });
        assert!(!unsafe { compl_status_local() });
    }

    #[test]
    fn compl_status_predicates_each_read_their_own_flag() {
        let _guard = ComplStateGuard::new();

        unsafe { *COMPL_CONT_STATUS.get_mut() = CONT_ADDING };
        assert!(unsafe { compl_status_adding() });
        assert!(!unsafe { compl_status_sol() });
        assert!(!unsafe { compl_status_local() });

        unsafe { *COMPL_CONT_STATUS.get_mut() = CONT_SOL };
        assert!(!unsafe { compl_status_adding() });
        assert!(unsafe { compl_status_sol() });

        unsafe { *COMPL_CONT_STATUS.get_mut() = CONT_LOCAL };
        assert!(!unsafe { compl_status_sol() });
        assert!(unsafe { compl_status_local() });
    }

    #[test]
    fn compl_status_flags_combine() {
        let _guard = ComplStateGuard::new();
        unsafe { *COMPL_CONT_STATUS.get_mut() = CONT_ADDING | CONT_SOL | CONT_LOCAL };
        assert!(unsafe { compl_status_adding() });
        assert!(unsafe { compl_status_sol() });
        assert!(unsafe { compl_status_local() });
    }

    #[test]
    fn cont_intrpt_implies_cont_n_adds() {
        // CONT_INTRPT is deliberately 2 + 4 in the original, so it
        // carries CONT_N_ADDS with it rather than being a lone bit.
        assert_eq!(CONT_INTRPT, 6);
        assert_ne!(CONT_INTRPT & CONT_N_ADDS, 0);
    }

    #[test]
    fn cont_flags_are_distinct_bits() {
        // Every flag but CONT_INTRPT is a single, distinct bit.
        for (a, b) in [
            (CONT_ADDING, CONT_N_ADDS),
            (CONT_ADDING, CONT_S_IPOS),
            (CONT_N_ADDS, CONT_S_IPOS),
            (CONT_S_IPOS, CONT_SOL),
            (CONT_SOL, CONT_LOCAL),
        ] {
            assert_eq!(a & b, 0, "{a} and {b} overlap");
        }
    }

    #[test]
    fn compl_status_clear_resets_every_flag() {
        let _guard = ComplStateGuard::new();
        unsafe { *COMPL_CONT_STATUS.get_mut() = CONT_ADDING | CONT_SOL | CONT_LOCAL };
        unsafe { compl_status_clear() };
        assert_eq!(unsafe { *COMPL_CONT_STATUS.get_mut() }, 0);
        assert!(!unsafe { compl_status_adding() });
    }

    #[test]
    fn completion_directions_default_to_forward() {
        let _guard = ComplStateGuard::new();
        assert!(unsafe { compl_dir_forward() });
        assert!(unsafe { compl_shows_dir_forward() });
        assert!(!unsafe { compl_shows_dir_backward() });
    }

    #[test]
    fn compl_dir_forward_is_independent_of_the_shown_direction() {
        let _guard = ComplStateGuard::new();
        unsafe { *COMPL_DIRECTION.get_mut() = crate::vim_defs::Direction::Backward };
        assert!(!unsafe { compl_dir_forward() });
        // The direction being shown is tracked separately.
        assert!(unsafe { compl_shows_dir_forward() });
    }

    #[test]
    fn shows_dir_forward_and_backward_can_both_be_false() {
        let _guard = ComplStateGuard::new();
        // Direction also has the *_FILE values, so these two
        // predicates are not each other's negation.
        unsafe { *COMPL_SHOWS_DIR.get_mut() = crate::vim_defs::Direction::ForwardFile };
        assert!(!unsafe { compl_shows_dir_forward() });
        assert!(!unsafe { compl_shows_dir_backward() });

        unsafe { *COMPL_SHOWS_DIR.get_mut() = crate::vim_defs::Direction::Backward };
        assert!(unsafe { compl_shows_dir_backward() });
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
