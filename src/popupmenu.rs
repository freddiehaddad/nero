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
//! Also translated: [`PumitemT`] (`pumitem_T`, from `popupmenu.h`) -
//! one popup menu entry. Translated ahead of `pum_display`, its own
//! real consumer, to unblock `cmdexpand.c`'s `compl_match_array`
//! (hence `cmdline_pum_active`), matching this crate's established
//! "translate the blocking type first" approach.
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

/// `pum_first` - the index of the first displayed popup-menu item,
/// i.e. the scroll offset. Reset by [`pum_clear`]; otherwise only set
/// by `pum_display`/`pum_set_selected`, neither translated.
static PUM_FIRST: GlobalCell<i32> = GlobalCell::new(0);

/// `pum_invalid` - whether the popup menu must be redrawn because the
/// screen was cleared. Set by [`pum_invalidate`]; only cleared by
/// `pum_display`, not translated.
static PUM_INVALID: GlobalCell<bool> = GlobalCell::new(false);

/// `pum_size` - the total number of items in the popup menu. Only ever
/// set by `pum_display`, not translated, so this stays `0` forever in
/// this crate today - matching `PUM_HEIGHT`'s own established
/// treatment.
static PUM_SIZE: GlobalCell<i32> = GlobalCell::new(0);
/// Popup-menu entries currently being displayed (`pum_array`).
static PUM_ARRAY: GlobalCell<Option<Vec<PumitemT>>> = GlobalCell::new(None);
/// Width of the popup menu (`pum_width`).
#[allow(dead_code)]
static PUM_WIDTH: GlobalCell<i32> = GlobalCell::new(0);
/// Widest abbreviation column (`pum_base_width`).
static PUM_BASE_WIDTH: GlobalCell<i32> = GlobalCell::new(0);
/// Widest kind column including its separator (`pum_kind_width`).
static PUM_KIND_WIDTH: GlobalCell<i32> = GlobalCell::new(0);
/// Widest extra/menu column including its separator (`pum_extra_width`).
static PUM_EXTRA_WIDTH: GlobalCell<i32> = GlobalCell::new(0);
/// Whether a scrollbar column is present (`pum_scrollbar`).
static PUM_SCROLLBAR: GlobalCell<i32> = GlobalCell::new(0);
/// Whether the popup is laid out right-to-left (`pum_rl`).
static PUM_RL: GlobalCell<bool> = GlobalCell::new(false);
/// Popup-menu anchor column (`pum_col`).
static PUM_COL: GlobalCell<i32> = GlobalCell::new(0);

/// One popup menu entry (`pumitem_T`).
///
/// The original's four text fields are `char *` that the popup menu
/// does NOT own - `pum_display`'s own callers build the array out of
/// borrowed pointers into the completion state and free the array (but
/// not the strings) in `pum_undisplay`. Owned `Vec<u8>`s are used here
/// instead: this crate's established idiom for a C string field, and
/// the borrow relationship the original relies on cannot be expressed
/// without tying the item's lifetime to the completion state that
/// outlives it.
///
/// `pum_kind`/`pum_extra`/`pum_info` are genuinely optional (the
/// original leaves them `NULL` when a completion source supplies no
/// kind/menu/info text), so they are `Option`; `pum_text` is always
/// present.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PumitemT {
    /// main menu text (`pum_text`).
    pub pum_text: Vec<u8>,
    /// extra kind text, may be truncated (`pum_kind`).
    pub pum_kind: Option<Vec<u8>>,
    /// extra menu text, may be truncated (`pum_extra`).
    pub pum_extra: Option<Vec<u8>>,
    /// extra info (`pum_info`).
    pub pum_info: Option<Vec<u8>>,
    /// index of the completion source in `'complete'` (`pum_cpt_source_idx`).
    pub pum_cpt_source_idx: i32,
    /// highlight attribute for abbr (`pum_user_abbr_hlattr`).
    pub pum_user_abbr_hlattr: i32,
    /// highlight attribute for kind (`pum_user_kind_hlattr`).
    pub pum_user_kind_hlattr: i32,
}

/// Compute popup-menu text column widths (`pum_compute_size`).
///
/// # Safety
/// Must not run concurrently with popup-menu mutation; forwards
/// [`crate::charset::vim_strsize`]'s option-state requirements.
#[allow(dead_code)]
unsafe fn pum_compute_size() {
    let mut base_width = 0;
    let mut kind_width = 0;
    let mut extra_width = 0;
    let size = unsafe { *PUM_SIZE.get_mut() }.max(0) as usize;
    if let Some(items) = unsafe { PUM_ARRAY.get_mut() }.as_ref() {
        for item in items.iter().take(size) {
            // SAFETY: forwarded from this function's own safety doc.
            base_width =
                base_width.max(unsafe { crate::charset::vim_strsize(&item.pum_text) });
            if let Some(kind) = item.pum_kind.as_deref() {
                // SAFETY: forwarded from this function's own safety doc.
                kind_width = kind_width
                    .max(unsafe { crate::charset::vim_strsize(kind) } + 1);
            }
            if let Some(extra) = item.pum_extra.as_deref() {
                // SAFETY: forwarded from this function's own safety doc.
                extra_width = extra_width
                    .max(unsafe { crate::charset::vim_strsize(extra) } + 1);
            }
        }
    }
    *unsafe { PUM_BASE_WIDTH.get_mut() } = base_width;
    *unsafe { PUM_KIND_WIDTH.get_mut() } = kind_width;
    *unsafe { PUM_EXTRA_WIDTH.get_mut() } = extra_width;
}

/// Set popup width while aligned with the cursor
/// (`set_pum_width_aligned_with_cursor`).
#[allow(dead_code)]
fn set_pum_width_aligned_with_cursor(
    mut width: i32,
    available_width: i32,
) -> bool {
    let options = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    let mut end_padding = true;
    if i64::from(width) < options.p_pw {
        width = options.p_pw as i32;
        end_padding = false;
    }
    if options.p_pmw > 0 && i64::from(width) > options.p_pmw {
        width = options.p_pmw as i32;
        end_padding = false;
    }
    let padding = i32::from(end_padding && i64::from(width) >= options.p_pw);
    *unsafe { PUM_WIDTH.get_mut() } = width.wrapping_add(padding);
    available_width >= unsafe { *PUM_WIDTH.get_mut() }
}

/// Compute horizontal popup-menu placement
/// (`pum_compute_horizontal_placement`).
///
/// # Safety
/// Reads global screen geometry and popup option state.
#[allow(dead_code)]
unsafe fn pum_compute_horizontal_placement(
    target_win: Option<&crate::buffer_defs::WinT>,
    cursor_col: i32,
    border_width: i32,
) {
    let columns = unsafe { crate::globals::GLOBALS.get_mut() }.Columns;
    let window_end =
        target_win.map_or(0, |win| win.w_wincol + win.w_view_width);
    let max_col = columns.max(window_end);
    let desired_width = unsafe {
        *PUM_BASE_WIDTH.get_mut()
            + *PUM_KIND_WIDTH.get_mut()
            + *PUM_EXTRA_WIDTH.get_mut()
    };
    let scrollbar = unsafe { *PUM_SCROLLBAR.get_mut() };
    let right_left = unsafe { *PUM_RL.get_mut() };
    let mut available_width = if right_left {
        cursor_col - scrollbar + 1 - border_width
    } else {
        max_col - cursor_col - scrollbar - border_width
    };

    *unsafe { PUM_COL.get_mut() } = cursor_col;
    if set_pum_width_aligned_with_cursor(desired_width, available_width) {
        return;
    }
    let minimum = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_pw;
    if i64::from(available_width) > minimum {
        *unsafe { PUM_WIDTH.get_mut() } = available_width;
        return;
    }

    if right_left {
        available_width = max_col - scrollbar - border_width;
    } else {
        available_width += cursor_col;
    }
    if i64::from(available_width) > minimum {
        let width = minimum as i32 + 1;
        *unsafe { PUM_WIDTH.get_mut() } = width;
        *unsafe { PUM_COL.get_mut() } = if right_left {
            width + scrollbar + border_width
        } else {
            max_col - width - scrollbar - border_width
        };
        return;
    }

    *unsafe { PUM_COL.get_mut() } = if right_left { max_col - 1 } else { 0 };
    *unsafe { PUM_WIDTH.get_mut() } = max_col - scrollbar - border_width;
}

/// State for `pum_ext_select_item` (`pum_want`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PumWant {
    /// Whether an external selection request is pending (`active`).
    pub active: bool,
    /// The item index being requested (`item`).
    pub item: i32,
    /// Whether the requested item should be inserted (`insert`).
    pub insert: bool,
    /// Whether completion should finish afterwards (`finish`).
    pub finish: bool,
}

/// `pum_want` - the pending external popup-menu selection request.
///
/// Only ever set by `pum_ext_select_item`, which is not translated, so
/// this stays at its default in this crate today - matching
/// `PUM_HEIGHT`'s own established treatment.
pub static PUM_WANT: GlobalCell<PumWant> = GlobalCell::new(PumWant {
    active: false,
    item: 0,
    insert: false,
    finish: false,
});

/// Clear the popup menu (`pum_clear`).
///
/// Currently only resets the offset to the first displayed item, as
/// the original's own comment notes.
pub fn pum_clear() {
    // SAFETY: a plain write through one borrow of this file's static.
    unsafe {
        *PUM_FIRST.get_mut() = 0;
    }
}

/// Record that the screen was cleared, so the popup menu is redrawn
/// next time (`pum_invalidate`).
pub fn pum_invalidate() {
    // SAFETY: a plain write through one borrow of this file's static.
    unsafe {
        *PUM_INVALID.get_mut() = true;
    }
}

/// Request that an attached UI select popup-menu item `item`
/// (`pum_ext_select_item`).
///
/// Ignored unless the menu is visible and `item` is in range: `-1`
/// (meaning "no selection") or a real item index. Note the original
/// guards on `pum_size`, the TOTAL item count, not on `pum_height`,
/// the number currently on screen.
pub fn pum_ext_select_item(item: i32, insert: bool, finish: bool) {
    // SAFETY: a plain read through one borrow of this file's static.
    let size = unsafe { *PUM_SIZE.get_mut() };
    if !pum_visible() || item < -1 || item >= size {
        return;
    }
    // SAFETY: a plain write through one borrow of this file's static.
    let want = unsafe { PUM_WANT.get_mut() };
    want.active = true;
    want.item = item;
    want.insert = insert;
    want.finish = finish;
}

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

    // ---- pum_clear / pum_invalidate / pum_ext_select_item ----

    /// Restores `PUM_FIRST`, `PUM_INVALID`, `PUM_SIZE` and `PUM_WANT`
    /// on drop, even through a panic.
    struct PumStateGuard {
        first: i32,
        invalid: bool,
        size: i32,
        want: PumWant,
    }

    struct PumSizingGuard {
        array: Option<Vec<PumitemT>>,
        size: i32,
        width: i32,
        base: i32,
        kind: i32,
        extra: i32,
        scrollbar: i32,
        right_left: bool,
        col: i32,
    }

    impl PumSizingGuard {
        fn install(items: Vec<PumitemT>) -> Self {
            let size = items.len() as i32;
            Self {
                array: unsafe { PUM_ARRAY.get_mut() }.replace(items),
                size: std::mem::replace(unsafe { PUM_SIZE.get_mut() }, size),
                width: std::mem::replace(unsafe { PUM_WIDTH.get_mut() }, 0),
                base: std::mem::replace(unsafe { PUM_BASE_WIDTH.get_mut() }, 0),
                kind: std::mem::replace(unsafe { PUM_KIND_WIDTH.get_mut() }, 0),
                extra: std::mem::replace(unsafe { PUM_EXTRA_WIDTH.get_mut() }, 0),
                scrollbar: std::mem::replace(unsafe { PUM_SCROLLBAR.get_mut() }, 0),
                right_left: std::mem::replace(unsafe { PUM_RL.get_mut() }, false),
                col: std::mem::replace(unsafe { PUM_COL.get_mut() }, 0),
            }
        }
    }

    impl Drop for PumSizingGuard {
        fn drop(&mut self) {
            *unsafe { PUM_ARRAY.get_mut() } = self.array.take();
            *unsafe { PUM_SIZE.get_mut() } = self.size;
            *unsafe { PUM_WIDTH.get_mut() } = self.width;
            *unsafe { PUM_BASE_WIDTH.get_mut() } = self.base;
            *unsafe { PUM_KIND_WIDTH.get_mut() } = self.kind;
            *unsafe { PUM_EXTRA_WIDTH.get_mut() } = self.extra;
            *unsafe { PUM_SCROLLBAR.get_mut() } = self.scrollbar;
            *unsafe { PUM_RL.get_mut() } = self.right_left;
            *unsafe { PUM_COL.get_mut() } = self.col;
        }
    }

    impl PumStateGuard {
        fn save() -> Self {
            unsafe {
                Self {
                    first: *PUM_FIRST.get_mut(),
                    invalid: *PUM_INVALID.get_mut(),
                    size: *PUM_SIZE.get_mut(),
                    want: *PUM_WANT.get_mut(),
                }
            }
        }

        fn set_size(size: i32) {
            unsafe { *PUM_SIZE.get_mut() = size };
        }
    }

    impl Drop for PumStateGuard {
        fn drop(&mut self) {
            unsafe {
                *PUM_FIRST.get_mut() = self.first;
                *PUM_INVALID.get_mut() = self.invalid;
                *PUM_SIZE.get_mut() = self.size;
                *PUM_WANT.get_mut() = self.want;
            }
        }
    }

    #[test]
    fn pum_clear_resets_the_scroll_offset() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PumStateGuard::save();

        unsafe { *PUM_FIRST.get_mut() = 7 };
        pum_clear();
        assert_eq!(unsafe { *PUM_FIRST.get_mut() }, 0);
    }

    #[test]
    fn pum_compute_size_tracks_widest_text_kind_and_extra_columns() {
        let _lock = crate::globals::global_state_test_lock();
        let _sizing = PumSizingGuard::install(vec![
            PumitemT {
                pum_text: b"abc".to_vec(),
                pum_kind: Some(b"K".to_vec()),
                pum_extra: Some(b"menu".to_vec()),
                ..Default::default()
            },
            PumitemT {
                pum_text: "界".as_bytes().to_vec(),
                pum_kind: Some(b"long".to_vec()),
                ..Default::default()
            },
        ]);

        unsafe { pum_compute_size() };

        assert_eq!(unsafe { *PUM_BASE_WIDTH.get_mut() }, 3);
        assert_eq!(unsafe { *PUM_KIND_WIDTH.get_mut() }, 5);
        assert_eq!(unsafe { *PUM_EXTRA_WIDTH.get_mut() }, 5);
    }

    #[test]
    fn set_pum_width_aligned_clamps_to_options_and_adds_optional_padding() {
        let _lock = crate::globals::global_state_test_lock();
        let _sizing = PumSizingGuard::install(Vec::new());
        let options = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let saved = (options.p_pw, options.p_pmw);
        options.p_pw = 5;
        options.p_pmw = 0;

        assert!(set_pum_width_aligned_with_cursor(8, 9));
        assert_eq!(unsafe { *PUM_WIDTH.get_mut() }, 9);
        assert!(!set_pum_width_aligned_with_cursor(8, 8));

        assert!(set_pum_width_aligned_with_cursor(3, 5));
        assert_eq!(unsafe { *PUM_WIDTH.get_mut() }, 5);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_pmw = 6;
        assert!(set_pum_width_aligned_with_cursor(8, 6));
        assert_eq!(unsafe { *PUM_WIDTH.get_mut() }, 6);

        let options = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        (options.p_pw, options.p_pmw) = saved;
    }

    #[test]
    fn pum_compute_horizontal_placement_aligns_then_repositions_when_needed() {
        let _lock = crate::globals::global_state_test_lock();
        let _sizing = PumSizingGuard::install(Vec::new());
        let _columns = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.Columns,
                80,
            )
        };
        let options = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let saved = (options.p_pw, options.p_pmw);
        options.p_pw = 5;
        options.p_pmw = 0;
        unsafe {
            *PUM_BASE_WIDTH.get_mut() = 10;
            *PUM_KIND_WIDTH.get_mut() = 5;
            *PUM_EXTRA_WIDTH.get_mut() = 0;
            pum_compute_horizontal_placement(None, 10, 0);
        }
        assert_eq!(unsafe { *PUM_COL.get_mut() }, 10);
        assert_eq!(unsafe { *PUM_WIDTH.get_mut() }, 16);

        unsafe { pum_compute_horizontal_placement(None, 78, 0) };
        assert_eq!(unsafe { *PUM_COL.get_mut() }, 74);
        assert_eq!(unsafe { *PUM_WIDTH.get_mut() }, 6);

        unsafe { *PUM_RL.get_mut() = true };
        unsafe { pum_compute_horizontal_placement(None, 10, 0) };
        assert_eq!(unsafe { *PUM_COL.get_mut() }, 10);
        assert_eq!(unsafe { *PUM_WIDTH.get_mut() }, 11);

        let options = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        (options.p_pw, options.p_pmw) = saved;
    }

    #[test]
    fn pum_invalidate_sets_the_invalid_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PumStateGuard::save();

        unsafe { *PUM_INVALID.get_mut() = false };
        pum_invalidate();
        assert!(unsafe { *PUM_INVALID.get_mut() });
    }

    /// Nothing is recorded while the menu is not visible.
    #[test]
    fn pum_ext_select_item_is_ignored_when_the_menu_is_hidden() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PumStateGuard::save();
        let _v = PumVisibleGuard::set(false);
        PumStateGuard::set_size(5);
        unsafe { *PUM_WANT.get_mut() = PumWant::default() };

        pum_ext_select_item(2, true, true);
        assert!(!unsafe { PUM_WANT.get_mut() }.active);
    }

    /// A visible menu records the whole request.
    #[test]
    fn pum_ext_select_item_records_the_request() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PumStateGuard::save();
        let _v = PumVisibleGuard::set(true);
        PumStateGuard::set_size(5);
        unsafe { *PUM_WANT.get_mut() = PumWant::default() };

        pum_ext_select_item(3, true, false);
        let want = unsafe { *PUM_WANT.get_mut() };
        assert_eq!(
            (want.active, want.item, want.insert, want.finish),
            (true, 3, true, false)
        );
    }

    /// `-1` means "no selection" and is explicitly in range, while
    /// anything below it is not.
    #[test]
    fn pum_ext_select_item_accepts_minus_one_but_not_below() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PumStateGuard::save();
        let _v = PumVisibleGuard::set(true);
        PumStateGuard::set_size(5);

        unsafe { *PUM_WANT.get_mut() = PumWant::default() };
        pum_ext_select_item(-1, false, false);
        assert!(unsafe { PUM_WANT.get_mut() }.active, "-1 is in range");

        unsafe { *PUM_WANT.get_mut() = PumWant::default() };
        pum_ext_select_item(-2, false, false);
        assert!(!unsafe { PUM_WANT.get_mut() }.active, "-2 is out of range");
    }

    /// The upper bound is `pum_size` exclusive, so the last valid
    /// index is `size - 1`.
    #[test]
    fn pum_ext_select_item_bounds_the_index_by_pum_size() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PumStateGuard::save();
        let _v = PumVisibleGuard::set(true);
        PumStateGuard::set_size(5);

        unsafe { *PUM_WANT.get_mut() = PumWant::default() };
        pum_ext_select_item(4, false, false);
        assert!(unsafe { PUM_WANT.get_mut() }.active, "size - 1 is in range");

        unsafe { *PUM_WANT.get_mut() = PumWant::default() };
        pum_ext_select_item(5, false, false);
        assert!(!unsafe { PUM_WANT.get_mut() }.active, "size itself is not");
    }

    /// With no items at all every index is out of range, including 0.
    #[test]
    fn pum_ext_select_item_rejects_any_item_when_the_menu_is_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PumStateGuard::save();
        let _v = PumVisibleGuard::set(true);
        PumStateGuard::set_size(0);
        unsafe { *PUM_WANT.get_mut() = PumWant::default() };

        pum_ext_select_item(0, false, false);
        assert!(!unsafe { PUM_WANT.get_mut() }.active);
    }

    #[test]
    fn pumitem_default_leaves_every_optional_text_field_absent() {
        // The original never default-constructs a pumitem_T (each
        // field is assigned explicitly from the completion state), so
        // what matters here is only that the three genuinely-optional
        // texts start ABSENT rather than as an empty string: the
        // popup menu distinguishes "this source supplied no kind text"
        // from "this source supplied an empty kind text" when laying
        // out the kind/menu columns.
        let item = PumitemT::default();
        assert!(item.pum_text.is_empty());
        assert_eq!(item.pum_kind, None);
        assert_eq!(item.pum_extra, None);
        assert_eq!(item.pum_info, None);
        assert_eq!(item.pum_cpt_source_idx, 0);
        assert_eq!(item.pum_user_abbr_hlattr, 0);
        assert_eq!(item.pum_user_kind_hlattr, 0);
    }

    #[test]
    fn pumitem_absent_kind_is_distinct_from_an_empty_kind() {
        let absent = PumitemT { pum_text: b"foo".to_vec(), ..PumitemT::default() };
        let empty = PumitemT {
            pum_text: b"foo".to_vec(),
            pum_kind: Some(Vec::new()),
            ..PumitemT::default()
        };
        assert_ne!(absent, empty);
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
