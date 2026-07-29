//! Translated from `src/nvim/popupmenu.c` (tractable core only).
//!
//! `popupmenu.c` is neovim's insert/cmdline-completion popup-menu
//! rendering file - almost entirely dependent on the completion
//! subsystem (`insexpand.c`) and the screen/grid rendering pipeline,
//! neither translated. Translated: [`pum_visible`], which reads a
//! real file-static (`pum_is_visible`) only ever set `true` by
//! `pum_display` (drawing the popup menu on screen, not translated) -
//! since nothing in this crate can currently display a popup menu,
//! this genuinely, provably stays `false` in every session today,
//! matching this crate's established "always-empty-registry" pattern
//! (e.g. `crate::autocmd::AUTOCMDS`) rather than a hardcoded stub.
//!
//! Deferred: everything else in the file.

use crate::globals::GlobalCell;

/// `pum_is_visible` - whether the popup menu is currently displayed.
/// Only ever set `true` by `pum_display`, not yet translated, so this
/// stays `false` forever in this crate today.
static PUM_IS_VISIBLE: GlobalCell<bool> = GlobalCell::new(false);

/// `true` if the popup menu is currently displayed (`pum_visible`).
#[must_use]
pub fn pum_visible() -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    unsafe { *PUM_IS_VISIBLE.get_mut() }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Test-only helper letting other modules' own tests (e.g.
    /// `eval::funcs`'s `f_pumvisible` test) directly set the
    /// otherwise-private `PUM_IS_VISIBLE` flag. Caller must hold
    /// `crate::globals::global_state_test_lock()` for the whole
    /// duration this value matters.
    pub(crate) fn set_pum_is_visible(value: bool) {
        unsafe { *PUM_IS_VISIBLE.get_mut() = value };
    }

    #[test]
    fn pum_visible_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        set_pum_is_visible(false);
        assert!(!pum_visible());
    }

    #[test]
    fn pum_visible_reflects_the_underlying_flag() {
        let _lock = crate::globals::global_state_test_lock();
        set_pum_is_visible(true);
        assert!(pum_visible());
        set_pum_is_visible(false);
    }
}
