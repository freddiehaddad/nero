//! Translated from `src/nvim/ui.c` (a single harvested function only).
//!
//! `ui.c` implements the whole UI-client protocol (grid updates,
//! highlight state, the msgpack-RPC `nvim_ui_attach` handshake, etc.),
//! none of which is translated. Harvested one small, self-contained
//! piece ahead of the rest of the file: `ui_has` (needed by
//! `window.c`'s `tabline_height`), matching this crate's established
//! "one tractable function ahead of a huge file" precedent
//! (`ex_docmd.rs`, `search.rs`, etc.).
//!
//! `ui_has` reads the original's own file-static `bool ui_ext
//! [kUIExtCount] = { 0 }` array - only ever mutated by `ui_refresh`/
//! the real UI-attachment negotiation machinery (`nvim_ui_attach`),
//! none of which is translated. Since nothing in this crate can
//! currently attach a real UI, that array genuinely stays all-`false`
//! forever in any session this crate can construct today - `ui_has`
//! is translated as an always-`false` predicate, matching this
//! crate's established "always-empty-registry" precedent
//! (`crate::autocmd::AUTOCMDS`) rather than modeling the full,
//! currently-inert array.

/// Maximum number of UIs that may be attached at once
/// (`MAX_UI_COUNT`).
pub const MAX_UI_COUNT: usize = 16;

/// Number of [`UiExtension`] variants (`kUIExtCount`).
pub const UI_EXT_COUNT: usize = 10;

/// One attached UI (`RemoteUI`).
///
/// Only the capability, geometry and TUI half of the original struct
/// is translated. Everything after `channel_id` - the `PackerBuffer`
/// and the msgpack event-batching bookkeeping (`nevents_pos`,
/// `ncalls_pos`, `cur_event` and friends) - belongs to the msgpack-RPC
/// wire protocol, which is not translated; those fields have no
/// equivalent here yet. The fields that ARE present are exactly the
/// ones the small `ui.c` predicates below read.
#[derive(Debug, Default)]
pub struct RemoteUI {
    /// Whether this UI wants RGB colors (`rgb`).
    pub rgb: bool,
    /// Force highest-requested UI capabilities (`override`).
    ///
    /// Named with a trailing underscore because `override` is a
    /// reserved word in Rust.
    pub override_: bool,
    /// Whether this UI is composed by the internal compositor
    /// (`composed`).
    pub composed: bool,
    /// UI capabilities/extensions (`ui_ext`).
    pub ui_ext: [bool; UI_EXT_COUNT],
    /// Reported width in cells (`width`).
    pub width: i32,
    /// Reported height in cells (`height`).
    pub height: i32,
    /// Actual number of lines shown in the popup menu (`pum_nlines`).
    pub pum_nlines: i32,
    /// Whether the UI reports back the popup menu position
    /// (`pum_pos`).
    pub pum_pos: bool,
    /// Popup menu row (`pum_row`).
    pub pum_row: f64,
    /// Popup menu column (`pum_col`).
    pub pum_col: f64,
    /// Popup menu height (`pum_height`).
    pub pum_height: f64,
    /// Popup menu width (`pum_width`).
    pub pum_width: f64,
    /// Terminal name, for a TUI (`term_name`).
    pub term_name: Option<Vec<u8>>,
    /// Number of terminal colors, for a TUI (`term_colors`).
    pub term_colors: i32,
    /// Whether stdin is a TTY (`stdin_tty`).
    pub stdin_tty: bool,
    /// Whether stdout is a TTY (`stdout_tty`).
    pub stdout_tty: bool,
    /// Channel this UI is attached over (`channel_id`).
    pub channel_id: u64,
}

impl RemoteUI {
    /// Whether this UI is the built-in terminal UI rather than a
    /// separate GUI.
    ///
    /// The original spells this out inline as
    /// `ui->stdin_tty || ui->stdout_tty` in each of the predicates
    /// below; naming it once keeps the three uses from drifting.
    #[must_use]
    pub fn is_tui(&self) -> bool {
        self.stdin_tty || self.stdout_tty
    }
}

/// The attached UIs (`uis`).
///
/// The original is a fixed `RemoteUI *uis[MAX_UI_COUNT]` plus a
/// separate `ui_count`; a `Vec` carries the count itself, and the
/// capacity limit is enforced by [`ui_can_attach_more`] exactly as
/// before.
static UIS: crate::globals::GlobalCell<Vec<*mut RemoteUI>> =
    crate::globals::GlobalCell::new(Vec::new());

/// The number of UIs connected to this server (`ui_active`).
///
/// # Safety
/// Reads the `UIS` file-static.
#[must_use]
pub unsafe fn ui_active() -> usize {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { UIS.get_mut() }.len()
}

/// Whether another UI may still attach (`ui_can_attach_more`).
///
/// # Safety
/// Reads the `UIS` file-static.
#[must_use]
pub unsafe fn ui_can_attach_more() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let n = unsafe { ui_active() };
    n < MAX_UI_COUNT
}

/// Whether any attached UI requested `override` (`ui_override`).
///
/// # Safety
/// Every pointer in `UIS` must point at a live [`RemoteUI`].
#[must_use]
pub unsafe fn ui_override() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { UIS.get_mut() }
        .iter()
        // SAFETY: forwarded from this function's own safety doc.
        .any(|&ui| unsafe { (*ui).override_ })
}

/// Whether any attached UI is a GUI rather than the terminal UI
/// (`ui_gui_attached`).
///
/// # Safety
/// Same as [`ui_override`].
#[must_use]
pub unsafe fn ui_gui_attached() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { UIS.get_mut() }
        .iter()
        // SAFETY: forwarded from this function's own safety doc.
        .any(|&ui| !unsafe { (*ui).is_tui() })
}

/// Whether any attached UI wants RGB colors (`ui_rgb_attached`).
///
/// `'termguicolors'` short-circuits this to true. Past that, the TUI
/// is deliberately NOT considered: the option check above already
/// covered it, so what remains is whether any OTHER UI wants RGB.
///
/// # Safety
/// Same as [`ui_override`], and reads `OPTION_VARS`.
#[must_use]
pub unsafe fn ui_rgb_attached() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_tgc != 0 {
        return true;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { UIS.get_mut() }.iter().any(|&ui| {
        // SAFETY: forwarded from this function's own safety doc.
        let ui = unsafe { &*ui };
        !ui.is_tui() && ui.rgb
    })
}

/// UI extension/capability identifiers (`UIExtension`), mechanically
/// transcribed from `ui_defs.h` for [`ui_has`]'s own parameter type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UiExtension {
    Cmdline = 0,
    Popupmenu = 1,
    Tabline = 2,
    Wildmenu = 3,
    Messages = 4,
    Linegrid = 5,
    Multigrid = 6,
    HlState = 7,
    TermColors = 8,
    FloatDebug = 9,
}

/// Returns `true` if the given UI extension is enabled (`ui_has`).
///
/// Always `false` - see this module's own doc comment.
#[must_use]
pub const fn ui_has(_ext: UiExtension) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- UI registry predicates ----

    /// Installs a set of UIs and restores the registry afterwards,
    /// even if the test panics.
    ///
    /// The UIs are boxed deliberately. `UIS` holds raw pointers to
    /// them, so each must have an address that does not move when the
    /// owning collection does - `Vec<RemoteUI>` would make the
    /// pointers depend on the vector's buffer never reallocating,
    /// which is exactly the class of dangling-pointer bug this crate
    /// has hit before. Hence the `vec_box` allow.
    #[allow(clippy::vec_box)]
    struct UisGuard {
        _owned: Vec<Box<RemoteUI>>,
        prev: Vec<*mut RemoteUI>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl UisGuard {
        #[allow(clippy::vec_box)]
        fn install(mut uis: Vec<Box<RemoteUI>>) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let prev = unsafe { UIS.get_mut() }.clone();
            let ptrs: Vec<*mut RemoteUI> =
                uis.iter_mut().map(|u| std::ptr::addr_of_mut!(**u)).collect();
            unsafe { *UIS.get_mut() = ptrs };
            UisGuard { _owned: uis, prev, _lock }
        }
    }

    impl Drop for UisGuard {
        fn drop(&mut self) {
            unsafe { *UIS.get_mut() = std::mem::take(&mut self.prev) };
        }
    }

    fn tui(rgb: bool) -> Box<RemoteUI> {
        Box::new(RemoteUI { rgb, stdout_tty: true, ..Default::default() })
    }

    fn gui(rgb: bool) -> Box<RemoteUI> {
        Box::new(RemoteUI { rgb, ..Default::default() })
    }

    fn with_tgc<T>(on: bool, f: impl FnOnce() -> T) -> T {
        let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = ov.p_tgc;
        ov.p_tgc = i32::from(on);
        let r = f();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_tgc = prev;
        r
    }

    #[test]
    fn ui_active_counts_attached_uis() {
        let _g = UisGuard::install(vec![tui(false), gui(false)]);
        assert_eq!(unsafe { ui_active() }, 2);
    }

    #[test]
    fn ui_can_attach_more_until_the_limit_is_reached() {
        // Each guard holds the global test lock, so the first must be
        // released before installing a second - a nested install would
        // self-deadlock on the non-reentrant mutex.
        let one = UisGuard::install(vec![tui(false)]);
        assert!(unsafe { ui_can_attach_more() });
        drop(one);

        let full: Vec<Box<RemoteUI>> = (0..MAX_UI_COUNT).map(|_| gui(false)).collect();
        let _g = UisGuard::install(full);
        assert!(!unsafe { ui_can_attach_more() });
    }

    #[test]
    fn ui_override_is_true_when_any_ui_asks_for_it() {
        let plain = UisGuard::install(vec![gui(false), gui(false)]);
        assert!(!unsafe { ui_override() });
        drop(plain);

        // Set on the SECOND UI, so a scan that stopped early fails.
        let mut a = gui(false);
        let mut b = gui(false);
        a.override_ = false;
        b.override_ = true;
        let _g = UisGuard::install(vec![a, b]);
        assert!(unsafe { ui_override() });
    }

    #[test]
    fn ui_gui_attached_ignores_the_terminal_ui() {
        let only_tui = UisGuard::install(vec![tui(false)]);
        assert!(!unsafe { ui_gui_attached() });
        drop(only_tui);

        let _g = UisGuard::install(vec![tui(false), gui(false)]);
        assert!(unsafe { ui_gui_attached() });
    }

    #[test]
    fn ui_rgb_attached_is_true_when_termguicolors_is_set() {
        // Short-circuits before looking at any UI at all.
        let _g = UisGuard::install(vec![]);
        assert!(with_tgc(true, || unsafe { ui_rgb_attached() }));
    }

    #[test]
    fn ui_rgb_attached_does_not_count_the_terminal_ui() {
        // The TUI is deliberately excluded: 'termguicolors' already
        // covered it, so the scan asks whether any OTHER UI wants RGB.
        // A TUI with rgb set must therefore NOT make this true.
        let tui_only = UisGuard::install(vec![tui(true)]);
        assert!(!with_tgc(false, || unsafe { ui_rgb_attached() }));
        drop(tui_only);

        let _g = UisGuard::install(vec![tui(true), gui(true)]);
        assert!(with_tgc(false, || unsafe { ui_rgb_attached() }));
    }

    #[test]
    fn ui_rgb_attached_is_false_for_a_gui_that_does_not_want_rgb() {
        let _g = UisGuard::install(vec![gui(false)]);
        assert!(!with_tgc(false, || unsafe { ui_rgb_attached() }));
    }

    #[test]
    fn predicates_are_false_with_no_uis_attached() {
        let _g = UisGuard::install(vec![]);
        assert_eq!(unsafe { ui_active() }, 0);
        assert!(!unsafe { ui_override() });
        assert!(!unsafe { ui_gui_attached() });
        assert!(!with_tgc(false, || unsafe { ui_rgb_attached() }));
        assert!(unsafe { ui_can_attach_more() });
    }

    #[test]
    fn ui_has_is_always_false() {
        assert!(!ui_has(UiExtension::Tabline));
        assert!(!ui_has(UiExtension::Cmdline));
        assert!(!ui_has(UiExtension::FloatDebug));
    }
}
