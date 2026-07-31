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
