//! Translated from `src/nvim/message.c` (tractable core only).
//!
//! `message.c` (~3400 lines) is neovim's central message/echo display
//! file - used everywhere, but almost entirely dependent on the
//! screen/message redraw pipeline (`msg_puts*`/`msg_grid_validate`/
//! `msg_scroll_*`/`msg_ext_*`), none of which is translated.
//!
//! Translated: [`msg_id_exists`], [`msg_use_grid`], [`msg_do_throttle`],
//! [`msg_scrollsize`], [`redirecting`] - small, pure predicates/
//! computations needing only a couple of small pieces of genuinely-new
//! state (see below), not the actual message pipeline.
//!
//! `DEFAULT_GRID` is harvested here ahead of its real owning file,
//! `grid.c` (not translated) - it is the original's own file-static
//! `ScreenGrid default_grid` (declared in `grid.c`, `SCREEN_GRID_INIT`-
//! initialized), needed by [`msg_use_grid`]. Since nothing in this
//! crate can currently allocate a real grid (`grid_alloc`, not
//! translated), `DEFAULT_GRID.chars` stays permanently null - the same
//! "harvest a real global ahead of the rest of its file" precedent
//! already used for `mod_mask_table`/`modifier_keys_table`
//! (`keycodes_defs.rs`, ahead of `keycodes.c`) and `shape_table`
//! (`cursor_shape.rs`, ahead of the rest of `cursor_shape.c`).
//!
//! Deferred: everything else - the entire `msg_puts`/`msg_grid_*`/
//! `msg_scroll_*`/`msg_ext_*` output and routing pipeline,
//! `message_filtered` (needs `vim_regexec`, the regex engine, not
//! translated), `msg_strtrunc`/`trunc_string`/`other_sourcing_name`/
//! `get_emsg_source`/`get_emsg_lnum` (candidates for a future commit),
//! `messaging`/`msg_use_printf` (need `char_avail`/`ui_active`,
//! neither translated).

use crate::globals::GlobalCell;
use std::sync::LazyLock;

/// message id to be allocated to the next message (`msg_id_next`).
static MSG_ID_NEXT: GlobalCell<i64> = GlobalCell::new(1);

/// `default_grid` - the main screen's own [`crate::grid_defs::ScreenGrid`],
/// harvested here from `grid.c` ahead of the rest of that file (see this
/// module's own doc comment). Stays at [`crate::grid_defs::ScreenGrid::default`]
/// (`SCREEN_GRID_INIT`, `chars` null) forever today, since nothing in
/// this crate can currently allocate a real grid.
static DEFAULT_GRID: LazyLock<GlobalCell<crate::grid_defs::ScreenGrid>> =
    LazyLock::new(|| GlobalCell::new(crate::grid_defs::ScreenGrid::default()));

/// Returns `true` if the given integer message-id was previously
/// generated (i.e. is a real, already-issued id, not `0`/negative/not-
/// yet-issued) (`msg_id_exists`).
#[must_use]
pub fn msg_id_exists(id: i64) -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    id > 0 && id < unsafe { *MSG_ID_NEXT.get_mut() }
}

/// Whether messages should be displayed on the built-in `DEFAULT_GRID`
/// (as opposed to routed entirely through the `ext_messages` UI
/// extension) (`msg_use_grid`).
///
/// Always `false` today: `DEFAULT_GRID`'s own `chars` pointer is
/// always null (nothing in this crate can allocate a real grid yet),
/// which alone makes the original's own `default_grid.chars &&
/// !ui_has(kUIMessages)` condition false regardless of the second
/// operand - a real, faithful consequence of the current state, not a
/// hardcoded stub.
#[must_use]
pub fn msg_use_grid() -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    let has_chars = !unsafe { DEFAULT_GRID.get_mut() }.chars.is_null();
    has_chars && !crate::ui::ui_has(crate::ui::UiExtension::Messages)
}

/// Whether message-scrolling should be throttled (`msg_do_throttle`).
///
/// Always `false` today, following directly from [`msg_use_grid`]
/// always being `false`.
#[must_use]
pub fn msg_do_throttle() -> bool {
    msg_use_grid()
        && unsafe { crate::option_vars::OPTION_VARS.get_mut() }.rdb_flags
            & crate::option_vars::opt_rdb_flag::NOTHROTTLE
            == 0
}

/// Total number of screen lines occupied by scrolled messages,
/// including the reserved `'cmdheight'`/"hit-enter" lines
/// (`msg_scrollsize`).
#[must_use]
pub fn msg_scrollsize() -> i32 {
    // SAFETY: plain reads through their own exclusive borrows.
    let msg_scrolled = unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled;
    let p_ch = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch as i32;
    msg_scrolled + p_ch + i32::from(p_ch > 0 || msg_scrolled > 1)
}

/// Whether message output is currently being redirected - to a file
/// (`:redir >`), a register, a variable, or `execute()`'s output
/// capture (`redirecting`).
///
/// Always `false` today: none of `GLOBALS.redir_fd`/`redir_reg`/
/// `redir_vname`/`capture_ga` can currently be set by anything in this
/// crate (`:redir`/`execute()`, neither translated).
#[must_use]
pub fn redirecting() -> bool {
    // SAFETY: `.is_null()` never dereferences; the rest are plain
    // reads through their own exclusive borrows.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let vfile_set = !unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_vfile
        .as_deref()
        .unwrap_or(&[])
        .is_empty();
    !globals.redir_fd.is_null()
        || vfile_set
        || globals.redir_reg != 0
        || globals.redir_vname
        || !globals.capture_ga.is_null()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Test-only helper letting tests bump the otherwise-private
    /// `MSG_ID_NEXT` counter, matching the established
    /// `set_pum_is_visible`-style pattern (`popupmenu.rs`). Caller must
    /// hold `crate::globals::global_state_test_lock()` for the whole
    /// duration this value matters, and should restore the original
    /// value before releasing the lock.
    pub(crate) fn set_msg_id_next(value: i64) -> i64 {
        let cell = unsafe { MSG_ID_NEXT.get_mut() };
        let old = *cell;
        *cell = value;
        old
    }

    #[test]
    fn msg_id_exists_default_state() {
        let _lock = crate::globals::global_state_test_lock();
        // MSG_ID_NEXT starts at 1, so no id has ever been issued yet.
        assert!(!msg_id_exists(1));
        assert!(!msg_id_exists(0));
        assert!(!msg_id_exists(-1));
    }

    #[test]
    fn msg_id_exists_after_ids_issued() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_msg_id_next(5);
        assert!(msg_id_exists(1));
        assert!(msg_id_exists(4));
        assert!(!msg_id_exists(5));
        assert!(!msg_id_exists(6));
        assert!(!msg_id_exists(0));
        set_msg_id_next(old);
    }

    #[test]
    fn msg_use_grid_is_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!msg_use_grid());
    }

    #[test]
    fn msg_do_throttle_is_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!msg_do_throttle());
    }

    #[test]
    fn msg_scrollsize_matches_hand_computed_formula() {
        let _lock = crate::globals::global_state_test_lock();
        let old_scrolled = unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled;
        let old_ch = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch;

        unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled = 0;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = 0;
        assert_eq!(msg_scrollsize(), 0); // 0 + 0 + (false as i32) = 0

        unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled = 3;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = 0;
        assert_eq!(msg_scrollsize(), 4); // 3 + 0 + (3 > 1 => true) = 4

        unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled = 0;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = 2;
        assert_eq!(msg_scrollsize(), 3); // 0 + 2 + (2 > 0 => true) = 3

        unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled = old_scrolled;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = old_ch;
    }

    #[test]
    fn redirecting_is_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!redirecting());
    }

    #[test]
    fn redirecting_true_when_redir_reg_set() {
        let _lock = crate::globals::global_state_test_lock();
        let old = unsafe { crate::globals::GLOBALS.get_mut() }.redir_reg;
        unsafe { crate::globals::GLOBALS.get_mut() }.redir_reg = b'a' as i32;
        assert!(redirecting());
        unsafe { crate::globals::GLOBALS.get_mut() }.redir_reg = old;
    }

    #[test]
    fn redirecting_true_when_redir_vname_set() {
        let _lock = crate::globals::global_state_test_lock();
        let old = unsafe { crate::globals::GLOBALS.get_mut() }.redir_vname;
        unsafe { crate::globals::GLOBALS.get_mut() }.redir_vname = true;
        assert!(redirecting());
        unsafe { crate::globals::GLOBALS.get_mut() }.redir_vname = old;
    }

    #[test]
    fn redirecting_true_when_verbosefile_set() {
        let _lock = crate::globals::global_state_test_lock();
        let old = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile.take();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile = Some(b"log.txt".to_vec());
        assert!(redirecting());
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile = old;
    }
}
