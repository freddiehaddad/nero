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

/// Current cursor row as last sent to the UIs (`cursor_row`).
static CURSOR_ROW: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);
/// Current cursor column as last sent to the UIs (`cursor_col`).
static CURSOR_COL: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);
/// Whether a cursor update is still owed to the UIs
/// (`pending_cursor_update`).
static PENDING_CURSOR_UPDATE: crate::globals::GlobalCell<bool> =
    crate::globals::GlobalCell::new(false);
/// Whether a mode-info update is still owed to the UIs
/// (`pending_mode_info_update`).
static PENDING_MODE_INFO_UPDATE: crate::globals::GlobalCell<bool> =
    crate::globals::GlobalCell::new(false);
/// The grid the cursor currently sits on (`cursor_grid_handle`).
static CURSOR_GRID_HANDLE: crate::globals::GlobalCell<crate::types_defs::HandleT> =
    crate::globals::GlobalCell::new(crate::grid::DEFAULT_GRID_HANDLE);

#[cfg(test)]
pub(crate) unsafe fn ui_test_cursor_state() -> (crate::types_defs::HandleT, i32, i32, bool) {
    unsafe {
        (
            *CURSOR_GRID_HANDLE.get_mut(),
            *CURSOR_ROW.get_mut(),
            *CURSOR_COL.get_mut(),
            *PENDING_CURSOR_UPDATE.get_mut(),
        )
    }
}

#[cfg(test)]
pub(crate) unsafe fn ui_test_restore_cursor_state(
    state: (crate::types_defs::HandleT, i32, i32, bool),
) {
    unsafe {
        *CURSOR_GRID_HANDLE.get_mut() = state.0;
        *CURSOR_ROW.get_mut() = state.1;
        *CURSOR_COL.get_mut() = state.2;
        *PENDING_CURSOR_UPDATE.get_mut() = state.3;
    }
}

/// The cursor-shape mode index last sent to the UIs (`ui_mode_idx`).
///
/// Starts at `-1`, which matches no real mode, so the first check
/// always reports a change and sends the initial shape.
static UI_MODE_IDX: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(-1);
/// Whether a cursor-shape update is still owed to the UIs
/// (`pending_mode_update`).
static PENDING_MODE_UPDATE: crate::globals::GlobalCell<bool> =
    crate::globals::GlobalCell::new(false);

/// Move the cursor to `new_row`/`new_col` on grid `grid_handle`
/// (`ui_grid_cursor_goto`).
///
/// A move to where the cursor already is, on the grid it is already
/// on, is skipped entirely - so no redundant update is queued.
///
/// # Safety
/// Mutates the cursor file-statics.
pub unsafe fn ui_grid_cursor_goto(
    grid_handle: crate::types_defs::HandleT,
    new_row: i32,
    new_col: i32,
) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        if new_row == *CURSOR_ROW.get_mut()
            && new_col == *CURSOR_COL.get_mut()
            && grid_handle == *CURSOR_GRID_HANDLE.get_mut()
        {
            return;
        }

        *CURSOR_ROW.get_mut() = new_row;
        *CURSOR_COL.get_mut() = new_col;
        *CURSOR_GRID_HANDLE.get_mut() = grid_handle;
        *PENDING_CURSOR_UPDATE.get_mut() = true;
    }
}

/// Moves the cursor on the default grid (`ui_cursor_goto`).
///
/// # Safety
/// Same as [`ui_grid_cursor_goto`].
pub unsafe fn ui_cursor_goto(new_row: i32, new_col: i32) {
    unsafe {
        ui_grid_cursor_goto(
            crate::grid::DEFAULT_GRID_HANDLE,
            new_row,
            new_col,
        );
    }
}

/// Update the cursor shape if the mode changed
/// (`ui_cursor_shape_no_check_conceal`).
///
/// Does nothing before the screen is up, since there is no UI to tell.
///
/// # Safety
/// Reads `GLOBALS`, forwards
/// [`crate::cursor_shape::cursor_get_mode_idx`]'s own safety doc, and
/// mutates the mode file-statics.
pub unsafe fn ui_cursor_shape_no_check_conceal() {
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { crate::globals::GLOBALS.get_mut() }.full_screen {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let new_mode_idx = i32::try_from(unsafe { crate::cursor_shape::cursor_get_mode_idx() })
        .unwrap_or(-1);

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        if new_mode_idx != *UI_MODE_IDX.get_mut() {
            *UI_MODE_IDX.get_mut() = new_mode_idx;
            *PENDING_MODE_UPDATE.get_mut() = true;
        }
    }
}

/// The cursor's current row (`ui_current_row`).
///
/// # Safety
/// Reads the `CURSOR_ROW` file-static.
#[must_use]
pub unsafe fn ui_current_row() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *CURSOR_ROW.get_mut() }
}

/// The cursor's current column (`ui_current_col`).
///
/// # Safety
/// Reads the `CURSOR_COL` file-static.
#[must_use]
pub unsafe fn ui_current_col() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *CURSOR_COL.get_mut() }
}

#[cfg(test)]
pub(crate) unsafe fn ui_current_grid_handle_for_test() -> crate::types_defs::HandleT {
    unsafe { *CURSOR_GRID_HANDLE.get_mut() }
}

/// Note that a mode-info update is owed to the UIs
/// (`ui_mode_info_set`).
///
/// # Safety
/// Mutates the `PENDING_MODE_INFO_UPDATE` file-static.
pub unsafe fn ui_mode_info_set() {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *PENDING_MODE_INFO_UPDATE.get_mut() = true };
}

/// Note that moving `grid_handle` implicitly moved the cursor
/// (`ui_check_cursor_grid`).
///
/// Only the grid the cursor actually sits on matters; moving any
/// other leaves the cursor where it was.
///
/// # Safety
/// Reads/mutates the `CURSOR_GRID_HANDLE`/`PENDING_CURSOR_UPDATE`
/// file-statics.
pub unsafe fn ui_check_cursor_grid(grid_handle: crate::types_defs::HandleT) {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *CURSOR_GRID_HANDLE.get_mut() } == grid_handle {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *PENDING_CURSOR_UPDATE.get_mut() = true };
    }
}

/// Whether the mouse is active for `mode` (`ui_mouse_has`).
///
/// `mode` is one of the `'mouse'` mode characters. An `"a"` in the
/// option means every mode in `MOUSE_A`, and an `"h"` means every mode
/// EXCEPT the hit-return prompt, but only while a help buffer is
/// current.
///
/// # Safety
/// Reads `OPTION_VARS` and `GLOBALS.curbuf`, which must be a valid
/// pointer to a live buffer.
#[must_use]
pub unsafe fn ui_mouse_has(mode: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let Some(p_mouse) = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mouse.clone() else {
        return false;
    };

    for &c in &p_mouse {
        if c == 0 {
            break;
        }
        if c == b'a' {
            if crate::strings::vim_strchr(crate::option_vars::MOUSE_A.as_bytes(), mode).is_some() {
                return true;
            }
        } else if c == crate::option_vars::MOUSE_HELP {
            // SAFETY: forwarded from this function's own safety doc.
            let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            // SAFETY: forwarded from this function's own safety doc.
            let is_help = !curbuf.is_null() && unsafe { (*curbuf).b_help };
            if mode != i32::from(crate::option_vars::MOUSE_RETURN) && is_help {
                return true;
            }
        } else if mode == i32::from(c) {
            return true;
        }
    }

    false
}

/// The popup-menu height every attached UI can accommodate
/// (`ui_pum_get_height`).
///
/// UIs reporting no height of their own are skipped; among those that
/// do, the SMALLEST wins, so the menu fits in all of them. Zero means
/// nothing was reported at all.
///
/// # Safety
/// Every pointer in `UIS` must point at a live [`RemoteUI`].
#[must_use]
pub unsafe fn ui_pum_get_height() -> i32 {
    let mut pum_height = 0;
    // SAFETY: forwarded from this function's own safety doc.
    for &ui in unsafe { UIS.get_mut() }.iter() {
        // SAFETY: forwarded from this function's own safety doc.
        let ui_pum_height = unsafe { (*ui).pum_nlines };
        if ui_pum_height != 0 {
            pum_height =
                if pum_height == 0 { ui_pum_height } else { pum_height.min(ui_pum_height) };
        }
    }
    pum_height
}

/// The popup-menu geometry reported back by a UI (`ui_pum_get_pos`).
///
/// @return `Some((width, height, row, col))` from the first UI that
///         reports a position, or `None` when none does. The original
///         writes those through four `double *` out-parameters.
///
/// # Safety
/// Same as [`ui_pum_get_height`].
#[must_use]
pub unsafe fn ui_pum_get_pos() -> Option<(f64, f64, f64, f64)> {
    // SAFETY: forwarded from this function's own safety doc.
    for &ui in unsafe { UIS.get_mut() }.iter() {
        // SAFETY: forwarded from this function's own safety doc.
        let ui = unsafe { &*ui };
        if !ui.pum_pos {
            continue;
        }
        return Some((ui.pum_width, ui.pum_height, ui.pum_row, ui.pum_col));
    }
    None
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
    fn ui_grid_cursor_goto_moves_and_flags_an_update() {
        let _lock = crate::globals::global_state_test_lock();
        let grid = unsafe { *CURSOR_GRID_HANDLE.get_mut() };
        unsafe { *PENDING_CURSOR_UPDATE.get_mut() = false };

        unsafe { ui_grid_cursor_goto(grid, 4, 9) };

        let (row, col, pending) = unsafe {
            (ui_current_row(), ui_current_col(), *PENDING_CURSOR_UPDATE.get_mut())
        };
        unsafe {
            *CURSOR_ROW.get_mut() = 0;
            *CURSOR_COL.get_mut() = 0;
            *PENDING_CURSOR_UPDATE.get_mut() = false;
        }
        assert_eq!((row, col), (4, 9));
        assert!(pending);
    }

    #[test]
    fn ui_cursor_goto_targets_the_default_grid() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            *CURSOR_GRID_HANDLE.get_mut() = 99;
            *CURSOR_ROW.get_mut() = 0;
            *CURSOR_COL.get_mut() = 0;
            *PENDING_CURSOR_UPDATE.get_mut() = false;
            ui_cursor_goto(5, 7);
        }

        assert_eq!(unsafe { *CURSOR_GRID_HANDLE.get_mut() }, crate::grid::DEFAULT_GRID_HANDLE);
        assert_eq!((unsafe { ui_current_row() }, unsafe { ui_current_col() }), (5, 7));
        assert!(unsafe { *PENDING_CURSOR_UPDATE.get_mut() });

        unsafe {
            *CURSOR_GRID_HANDLE.get_mut() = crate::grid::DEFAULT_GRID_HANDLE;
            *CURSOR_ROW.get_mut() = 0;
            *CURSOR_COL.get_mut() = 0;
            *PENDING_CURSOR_UPDATE.get_mut() = false;
        }
    }

    #[test]
    fn ui_grid_cursor_goto_skips_a_move_that_changes_nothing() {
        // Same row, same column, same grid: no update should be
        // queued, or every redraw would send a redundant one.
        let _lock = crate::globals::global_state_test_lock();
        let grid = unsafe { *CURSOR_GRID_HANDLE.get_mut() };
        unsafe {
            *CURSOR_ROW.get_mut() = 4;
            *CURSOR_COL.get_mut() = 9;
            *PENDING_CURSOR_UPDATE.get_mut() = false;
        }

        unsafe { ui_grid_cursor_goto(grid, 4, 9) };
        let pending = unsafe { *PENDING_CURSOR_UPDATE.get_mut() };

        unsafe {
            *CURSOR_ROW.get_mut() = 0;
            *CURSOR_COL.get_mut() = 0;
            *PENDING_CURSOR_UPDATE.get_mut() = false;
        }
        assert!(!pending);
    }

    #[test]
    fn ui_grid_cursor_goto_reacts_to_a_grid_change_alone() {
        // Same position but a different grid IS a real move, so the
        // grid must be part of the comparison.
        let _lock = crate::globals::global_state_test_lock();
        let grid = unsafe { *CURSOR_GRID_HANDLE.get_mut() };
        unsafe {
            *CURSOR_ROW.get_mut() = 4;
            *CURSOR_COL.get_mut() = 9;
            *PENDING_CURSOR_UPDATE.get_mut() = false;
        }

        unsafe { ui_grid_cursor_goto(grid + 1, 4, 9) };
        let (pending, new_grid) =
            unsafe { (*PENDING_CURSOR_UPDATE.get_mut(), *CURSOR_GRID_HANDLE.get_mut()) };

        unsafe {
            *CURSOR_ROW.get_mut() = 0;
            *CURSOR_COL.get_mut() = 0;
            *CURSOR_GRID_HANDLE.get_mut() = grid;
            *PENDING_CURSOR_UPDATE.get_mut() = false;
        }
        assert!(pending);
        assert_eq!(new_grid, grid + 1);
    }

    #[test]
    fn ui_cursor_shape_does_nothing_before_the_screen_is_up() {
        // Nothing to tell without a UI, so the mode index is left
        // alone entirely.
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_fs = g.full_screen;
        g.full_screen = false;

        let before = unsafe { *UI_MODE_IDX.get_mut() };
        unsafe { *PENDING_MODE_UPDATE.get_mut() = false };
        unsafe { ui_cursor_shape_no_check_conceal() };
        let (after, pending) =
            unsafe { (*UI_MODE_IDX.get_mut(), *PENDING_MODE_UPDATE.get_mut()) };

        unsafe { crate::globals::GLOBALS.get_mut() }.full_screen = prev_fs;
        assert_eq!(after, before);
        assert!(!pending);
    }

    #[test]
    fn ui_cursor_shape_flags_an_update_only_when_the_mode_changes() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_fs = g.full_screen;
        g.full_screen = true;
        let prev_idx = unsafe { *UI_MODE_IDX.get_mut() };

        // Starting from -1 (no mode yet), the first call always
        // reports a change.
        unsafe {
            *UI_MODE_IDX.get_mut() = -1;
            *PENDING_MODE_UPDATE.get_mut() = false;
            ui_cursor_shape_no_check_conceal();
        }
        let first = unsafe { *PENDING_MODE_UPDATE.get_mut() };

        // A second call with the mode unchanged queues nothing.
        unsafe {
            *PENDING_MODE_UPDATE.get_mut() = false;
            ui_cursor_shape_no_check_conceal();
        }
        let second = unsafe { *PENDING_MODE_UPDATE.get_mut() };

        unsafe {
            *UI_MODE_IDX.get_mut() = prev_idx;
            *PENDING_MODE_UPDATE.get_mut() = false;
            crate::globals::GLOBALS.get_mut().full_screen = prev_fs;
        }
        assert!(first, "the initial shape is always sent");
        assert!(!second, "an unchanged mode queues nothing");
    }

    // ---- ui_mouse_has / cursor accessors ----

    fn with_mouse_opt<T>(opt: Option<&[u8]>, f: impl FnOnce() -> T) -> T {
        let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = ov.p_mouse.clone();
        ov.p_mouse = opt.map(<[u8]>::to_vec);
        let r = f();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mouse = prev;
        r
    }

    /// Boxed: the pointer is installed into GLOBALS.curbuf.
    fn with_help_buf<T>(is_help: bool, f: impl FnOnce() -> T) -> T {
        let mut buf = Box::new(crate::buffer_defs::BufT { b_help: is_help, ..Default::default() });
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.curbuf;
        g.curbuf = std::ptr::addr_of_mut!(*buf);
        let r = f();
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
        r
    }

    #[test]
    fn ui_mouse_has_all_covers_every_mode_in_mouse_a() {
        let _lock = crate::globals::global_state_test_lock();
        // MOUSE_A is "nvich", so those modes are covered...
        for m in *b"nvich" {
            assert!(
                with_mouse_opt(Some(b"a"), || unsafe { ui_mouse_has(i32::from(m)) }),
                "mode {} should be covered by 'a'",
                m as char
            );
        }
        // ...and the hit-return prompt is NOT one of them.
        assert!(!with_mouse_opt(Some(b"a"), || unsafe {
            ui_mouse_has(i32::from(crate::option_vars::MOUSE_RETURN))
        }));
    }

    #[test]
    fn ui_mouse_has_matches_an_explicitly_listed_mode() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(with_mouse_opt(Some(b"nv"), || unsafe { ui_mouse_has(i32::from(b'n')) }));
        assert!(with_mouse_opt(Some(b"nv"), || unsafe { ui_mouse_has(i32::from(b'v')) }));
        assert!(!with_mouse_opt(Some(b"nv"), || unsafe { ui_mouse_has(i32::from(b'c')) }));
    }

    #[test]
    fn ui_mouse_has_help_flag_needs_a_help_buffer() {
        // 'h' means "any mode, but only in a help buffer".
        let _lock = crate::globals::global_state_test_lock();

        let in_help = with_help_buf(true, || {
            with_mouse_opt(Some(b"h"), || unsafe { ui_mouse_has(i32::from(b'n')) })
        });
        let not_help = with_help_buf(false, || {
            with_mouse_opt(Some(b"h"), || unsafe { ui_mouse_has(i32::from(b'n')) })
        });

        assert!(in_help);
        assert!(!not_help, "'h' does nothing outside a help buffer");
    }

    #[test]
    fn ui_mouse_has_help_flag_excludes_the_hit_return_prompt() {
        // Even in a help buffer, 'h' deliberately does not cover the
        // hit-return prompt.
        let _lock = crate::globals::global_state_test_lock();
        let got = with_help_buf(true, || {
            with_mouse_opt(Some(b"h"), || unsafe {
                ui_mouse_has(i32::from(crate::option_vars::MOUSE_RETURN))
            })
        });
        assert!(!got);
    }

    #[test]
    fn ui_mouse_has_is_false_when_mouse_is_unset_or_empty() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!with_mouse_opt(None, || unsafe { ui_mouse_has(i32::from(b'n')) }));
        assert!(!with_mouse_opt(Some(b""), || unsafe { ui_mouse_has(i32::from(b'n')) }));
    }

    #[test]
    fn ui_check_cursor_grid_only_reacts_to_the_cursors_own_grid() {
        let _lock = crate::globals::global_state_test_lock();
        let cursor_grid = unsafe { *CURSOR_GRID_HANDLE.get_mut() };

        unsafe { *PENDING_CURSOR_UPDATE.get_mut() = false };
        unsafe { ui_check_cursor_grid(cursor_grid + 1) };
        let other = unsafe { *PENDING_CURSOR_UPDATE.get_mut() };

        unsafe { ui_check_cursor_grid(cursor_grid) };
        let same = unsafe { *PENDING_CURSOR_UPDATE.get_mut() };

        unsafe { *PENDING_CURSOR_UPDATE.get_mut() = false };
        assert!(!other, "another grid moving leaves the cursor alone");
        assert!(same);
    }

    #[test]
    fn ui_mode_info_set_records_the_pending_update() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *PENDING_MODE_INFO_UPDATE.get_mut() = false };
        unsafe { ui_mode_info_set() };
        let got = unsafe { *PENDING_MODE_INFO_UPDATE.get_mut() };
        unsafe { *PENDING_MODE_INFO_UPDATE.get_mut() = false };
        assert!(got);
    }

    #[test]
    fn ui_current_row_and_col_start_at_zero() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { ui_current_row() }, 0);
        assert_eq!(unsafe { ui_current_col() }, 0);
    }

    #[test]
    fn ui_pum_get_height_takes_the_smallest_reported_height() {
        // The menu must fit in every UI, so the smallest wins - and
        // the smallest is deliberately NOT first, so an implementation
        // that just took the first reported value would fail.
        let mut a = gui(false);
        let mut b = gui(false);
        let mut c = gui(false);
        a.pum_nlines = 20;
        b.pum_nlines = 5;
        c.pum_nlines = 12;
        let _g = UisGuard::install(vec![a, b, c]);

        assert_eq!(unsafe { ui_pum_get_height() }, 5);
    }

    #[test]
    fn ui_pum_get_height_skips_uis_reporting_nothing() {
        // A zero means "no height of my own", not "height zero", so it
        // must not clamp the result down to zero.
        let mut a = gui(false);
        let mut b = gui(false);
        a.pum_nlines = 0;
        b.pum_nlines = 7;
        let _g = UisGuard::install(vec![a, b]);

        assert_eq!(unsafe { ui_pum_get_height() }, 7);
    }

    #[test]
    fn ui_pum_get_height_is_zero_when_nothing_is_reported() {
        let _g = UisGuard::install(vec![gui(false), tui(false)]);
        assert_eq!(unsafe { ui_pum_get_height() }, 0);
    }

    #[test]
    fn ui_pum_get_pos_reports_the_first_ui_that_has_one() {
        // The first UI does not report a position, so a version that
        // only looked at index 0 would find nothing.
        let mut a = gui(false);
        let mut b = gui(false);
        a.pum_pos = false;
        b.pum_pos = true;
        b.pum_width = 12.0;
        b.pum_height = 4.0;
        b.pum_row = 2.5;
        b.pum_col = 7.5;
        let _g = UisGuard::install(vec![a, b]);

        assert_eq!(unsafe { ui_pum_get_pos() }, Some((12.0, 4.0, 2.5, 7.5)));
    }

    #[test]
    fn ui_pum_get_pos_is_none_when_no_ui_reports_one() {
        let _g = UisGuard::install(vec![gui(false), tui(false)]);
        assert_eq!(unsafe { ui_pum_get_pos() }, None);
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
