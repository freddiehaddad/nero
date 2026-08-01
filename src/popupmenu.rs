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
//! Also translated: [`pum_border_width`] (the popup menu's own border
//! width in screen columns, via already-real
//! `crate::option_vars::OPTION_VARS.p_pumborder`/
//! `OPT_WINBORDER_VALUES`) and [`pum_align_order`] (`'completeitemalign'`'s
//! own abbr/kind/menu display order, via already-real
//! `OPTION_VARS.cia_flags` and `crate::insexpand`'s newly-real
//! `CPT_ABBR`/`CPT_KIND`/`CPT_MENU`). `pum_align_order` returns an
//! owned `[i32; 3]` rather than writing through an out-parameter,
//! matching this crate's established "return an owned value instead
//! of an out-parameter" idiom. Both translated ahead of their real
//! caller (`pum_redraw`, needing the real, populated `pum_array` of
//! completion items plus the whole grid-rendering pipeline, not yet
//! translated), matching the established "translate ahead of a real
//! caller" precedent.
//!
//! Also translated: [`pum_drawn`] (via [`pum_visible`] plus a new
//! `PUM_EXTERNAL` file-static, matching `pum_is_visible`'s own
//! "only ever set by `pum_display`, not yet translated, always
//! `false` today" treatment) and [`pum_get_height`] (via a new
//! `PUM_HEIGHT` file-static - only ever set by
//! `pum_compute_vertical_placement`/`pum_show_popupmenu`, neither
//! translated, so it stays `0` today; its own `ui_pum_get_height()`
//! branch, gated on `PUM_EXTERNAL`, is `unimplemented!()` -
//! unreachable today since nothing can set `PUM_EXTERNAL` true yet).
//!
//! Also [`pum_set_event_info`] (`pum_getpos()`'s own real backend) -
//! its own FIRST check is `!pum_visible()`, always true today, so it
//! always takes the "leave the dict empty" early return - the real
//! body needing `ui_pum_get_pos`/`pum_width`/`pum_height`/`pum_row`/
//! `pum_col`/`pum_size`/`pum_scrollbar` is `unimplemented!()`,
//! unreachable for the same reason `pum_visible` itself never returns
//! `true`.
//!
//! Deferred: everything else in the file.

use crate::globals::GlobalCell;

/// `pum_is_visible` - whether the popup menu is currently displayed.
/// Only ever set `true` by `pum_display`, not yet translated, so this
/// stays `false` forever in this crate today.
static PUM_IS_VISIBLE: GlobalCell<bool> = GlobalCell::new(false);

/// `pum_external` - whether an attached UI client has its own native
/// popup-menu rendering support (`ui_has(kUIPopupmenu)`). Only ever
/// set by `pum_display`/`pum_undisplay`, neither translated, so this
/// stays `false` forever in this crate today.
static PUM_EXTERNAL: GlobalCell<bool> = GlobalCell::new(false);

/// `pum_height` - the number of popup-menu entries currently
/// displayed. Only ever set by `pum_compute_vertical_placement`
/// (part of `pum_display`'s own layout computation) and
/// `pum_show_popupmenu`, neither translated, so this stays `0`
/// forever in this crate today.
static PUM_HEIGHT: GlobalCell<i32> = GlobalCell::new(0);

/// `true` if the popup menu is currently displayed (`pum_visible`).
#[must_use]
pub fn pum_visible() -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    unsafe { *PUM_IS_VISIBLE.get_mut() }
}

/// `true` if the popup menu is displayed AND drawn on the grid, as
/// opposed to an attached UI's own external, native rendering
/// (`pum_drawn`).
#[must_use]
pub fn pum_drawn() -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    pum_visible() && !unsafe { *PUM_EXTERNAL.get_mut() }
}

/// Get the height (number of visible entries) of the popup menu -
/// only meaningful when [`pum_visible`] returns `true` (`pum_get_height`).
///
/// # Panics
/// If `PUM_EXTERNAL` is ever `true` (unreachable today - nothing in
/// this crate can currently set it - since that branch needs
/// `ui_pum_get_height()`'s real UI-attach/ext-popupmenu event
/// dispatch, not yet translated).
#[must_use]
pub fn pum_get_height() -> i32 {
    // SAFETY: a plain read through one exclusive borrow.
    if unsafe { *PUM_EXTERNAL.get_mut() } {
        unimplemented!(
            "pum_get_height: the ui_pum_get_height() branch needs the real UI-attach/ \
             ext-popupmenu event dispatch, not yet translated"
        );
    }
    // SAFETY: a plain read through one exclusive borrow.
    unsafe { *PUM_HEIGHT.get_mut() }
}

/// Compute the popup menu's own border width in screen columns, based
/// on `'pumborder'` (`pum_border_width`). Shadow (`"shadow"`) only has
/// a right+bottom edge (`1`); every other non-empty, non-`"none"`
/// style has a full border (`2`); an empty or `"none"` value has no
/// border (`0`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn pum_border_width() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    let p_pumborder = opts.p_pumborder.as_deref().unwrap_or(&[]);
    if p_pumborder.is_empty()
        || p_pumborder == crate::option_vars::OPT_WINBORDER_VALUES[7].as_bytes()
    {
        return 0; // No border
    }
    // Shadow (1) only has right+bottom, others (2) have full border
    if p_pumborder == crate::option_vars::OPT_WINBORDER_VALUES[3].as_bytes() {
        1
    } else {
        2
    }
}

/// Compute `'completeitemalign'`'s own display order for a completion
/// item's abbr/kind/menu parts, returning
/// `[first, second, third]` as [`crate::insexpand::CPT_ABBR`]/
/// `CPT_KIND`/`CPT_MENU`-style indices (`pum_align_order`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn pum_align_order() -> [i32; 3] {
    // SAFETY: forwarded from this function's own safety doc.
    let cia_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cia_flags;
    let is_default = cia_flags == 0;
    let cia_flags = cia_flags as i32;
    [
        if is_default { crate::insexpand::CPT_ABBR } else { cia_flags / 100 },
        if is_default { crate::insexpand::CPT_KIND } else { (cia_flags / 10) % 10 },
        if is_default { crate::insexpand::CPT_MENU } else { cia_flags % 10 },
    ]
}

/// Populate `dict` with the popup menu's own position/size info (for
/// `pum_getpos()`/`v:event` during `CompleteChanged`), or leave it
/// EMPTY if the popup menu isn't currently visible
/// (`pum_set_event_info`).
///
/// Since [`pum_visible`] always returns `false` today (nothing in this
/// crate can currently display a real popup menu), this always takes
/// the early-return branch - a real, faithful consequence of the
/// current state, matching this file's own established
/// "always-empty" pattern, not a hardcoded stub. The real body
/// (`ui_pum_get_pos`/`pum_width`/`pum_height`/`pum_row`/`pum_col`/
/// `pum_size`/`pum_scrollbar`, none translated) is `unimplemented!()`,
/// unreachable today for the same reason.
pub fn pum_set_event_info(_dict: &mut crate::eval::typval_defs::DictT) {
    if !pum_visible() {
        return;
    }
    unimplemented!(
        "pum_set_event_info: needs ui_pum_get_pos/pum_width/pum_height/pum_row/pum_col/ \
         pum_size/pum_scrollbar, none translated"
    );
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

    /// Restores `PUM_IS_VISIBLE` to `false` on drop, even through a
    /// panic - needed for any `#[should_panic]` test (in this file or
    /// another, e.g. `eval::funcs`'s own `f_pum_getpos` test)
    /// deliberately triggering a panic while `PUM_IS_VISIBLE` is
    /// temporarily set `true`. Unlike `PumExternalGuard`, always
    /// restores to `false` specifically (not "whatever it was
    /// before") since that's this whole test suite's own universal
    /// default for this flag.
    pub(crate) struct PumVisibleGuard;

    impl PumVisibleGuard {
        pub(crate) fn set(value: bool) -> Self {
            set_pum_is_visible(value);
            PumVisibleGuard
        }
    }

    impl Drop for PumVisibleGuard {
        fn drop(&mut self) {
            set_pum_is_visible(false);
        }
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

    // ---- pum_drawn / pum_get_height ----

    fn set_pum_external(value: bool) -> bool {
        std::mem::replace(unsafe { PUM_EXTERNAL.get_mut() }, value)
    }

    fn set_pum_height(value: i32) -> i32 {
        std::mem::replace(unsafe { PUM_HEIGHT.get_mut() }, value)
    }

    #[test]
    fn pum_drawn_false_when_not_visible_regardless_of_external() {
        let _lock = crate::globals::global_state_test_lock();
        set_pum_is_visible(false);
        let prev = set_pum_external(false);
        assert!(!pum_drawn());
        set_pum_external(prev);
    }

    #[test]
    fn pum_drawn_true_when_visible_and_not_external() {
        let _lock = crate::globals::global_state_test_lock();
        set_pum_is_visible(true);
        let prev = set_pum_external(false);
        assert!(pum_drawn());
        set_pum_external(prev);
        set_pum_is_visible(false);
    }

    #[test]
    fn pum_drawn_false_when_visible_but_external() {
        let _lock = crate::globals::global_state_test_lock();
        set_pum_is_visible(true);
        let prev = set_pum_external(true);
        assert!(!pum_drawn());
        set_pum_external(prev);
        set_pum_is_visible(false);
    }

    #[test]
    fn pum_get_height_reads_the_real_static_when_not_external() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_ext = set_pum_external(false);
        let prev_h = set_pum_height(5);
        assert_eq!(pum_get_height(), 5);
        set_pum_height(prev_h);
        set_pum_external(prev_ext);
    }

    /// Restores `PUM_EXTERNAL` to its previous value on drop, even
    /// through a panic - needed since `pum_get_height`'s own
    /// `#[should_panic]` test deliberately triggers a panic while
    /// `PUM_EXTERNAL` is temporarily set `true`.
    struct PumExternalGuard {
        previous: bool,
    }

    impl PumExternalGuard {
        fn set(value: bool) -> Self {
            PumExternalGuard { previous: set_pum_external(value) }
        }
    }

    impl Drop for PumExternalGuard {
        fn drop(&mut self) {
            set_pum_external(self.previous);
        }
    }

    #[test]
    #[should_panic(expected = "ui_pum_get_height")]
    fn pum_get_height_panics_when_external() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = PumExternalGuard::set(true);
        let _ = pum_get_height();
    }

    // ---- pum_border_width ----

    fn set_pumborder(value: Option<&[u8]>) -> Option<Vec<u8>> {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        std::mem::replace(&mut opts.p_pumborder, value.map(<[u8]>::to_vec))
    }

    #[test]
    fn pum_border_width_zero_when_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_pumborder(None);
        assert_eq!(unsafe { pum_border_width() }, 0);
        set_pumborder(prev.as_deref());
    }

    #[test]
    fn pum_border_width_zero_when_none() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_pumborder(Some(b"none"));
        assert_eq!(unsafe { pum_border_width() }, 0);
        set_pumborder(prev.as_deref());
    }

    #[test]
    fn pum_border_width_one_when_shadow() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_pumborder(Some(b"shadow"));
        assert_eq!(unsafe { pum_border_width() }, 1);
        set_pumborder(prev.as_deref());
    }

    #[test]
    fn pum_border_width_two_for_any_other_style() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_pumborder(Some(b"single"));
        assert_eq!(unsafe { pum_border_width() }, 2);
        set_pumborder(prev.as_deref());
    }

    // ---- pum_align_order ----

    fn set_cia_flags(value: u32) -> u32 {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        std::mem::replace(&mut opts.cia_flags, value)
    }

    #[test]
    fn pum_align_order_default_is_abbr_kind_menu() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_cia_flags(0);
        assert_eq!(
            unsafe { pum_align_order() },
            [crate::insexpand::CPT_ABBR, crate::insexpand::CPT_KIND, crate::insexpand::CPT_MENU]
        );
        set_cia_flags(prev);
    }

    #[test]
    fn pum_align_order_parses_each_digit_of_cia_flags() {
        let _lock = crate::globals::global_state_test_lock();
        // 213 -> [2, 1, 3] (hundreds, tens, units digit respectively).
        let prev = set_cia_flags(213);
        assert_eq!(unsafe { pum_align_order() }, [2, 1, 3]);
        set_cia_flags(prev);
    }
}
