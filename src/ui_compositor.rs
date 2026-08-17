//! Translated from `src/nvim/ui_compositor.c` (initial state helpers).

/// Number of UIs using internal composition (`composed_uis`).
static COMPOSED_UIS: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
/// Whether the composed screen is currently valid (`valid_screen`).
static VALID_SCREEN: crate::globals::GlobalCell<bool> =
    crate::globals::GlobalCell::new(true);
/// Scratch composed glyph line (`linebuf`).
#[allow(dead_code)]
static LINEBUF: crate::globals::GlobalCell<Option<Vec<crate::types_defs::ScharT>>> =
    crate::globals::GlobalCell::new(None);
/// Scratch composed attribute line (`attrbuf`).
#[allow(dead_code)]
static ATTRBUF: crate::globals::GlobalCell<Option<Vec<crate::types_defs::SattrT>>> =
    crate::globals::GlobalCell::new(None);
/// Allocated scratch-line width (`bufsize`).
#[allow(dead_code)]
static BUFSIZE: crate::globals::GlobalCell<usize> =
    crate::globals::GlobalCell::new(0);
/// Message separator screen row (`msg_sep_row`).
#[allow(dead_code)]
static MSG_SEP_ROW: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(-1);
/// Compositor layers, bottom to top (`layers`).
static LAYERS: crate::globals::GlobalCell<
    Vec<*mut crate::grid_defs::ScreenGrid>,
> = crate::globals::GlobalCell::new(Vec::new());
/// Grid currently receiving composed drawing (`curgrid`).
static CURGRID: crate::globals::GlobalCell<*mut crate::grid_defs::ScreenGrid> =
    crate::globals::GlobalCell::new(std::ptr::null_mut());
/// Debug highlight groups used while composing grids.
static DBGHL_NORMAL: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
static DBGHL_CLEAR: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
static DBGHL_COMPOSED: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
static DBGHL_RECOMPOSE: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);

/// Initialize the compositor's debug highlight groups
/// (`ui_comp_syn_init`).
///
/// # Safety
/// Must not run concurrently with highlight-group or compositor state.
pub unsafe fn ui_comp_syn_init() {
    *unsafe { DBGHL_NORMAL.get_mut() } =
        unsafe { crate::highlight_group::syn_check_group(b"RedrawDebugNormal") };
    *unsafe { DBGHL_CLEAR.get_mut() } =
        unsafe { crate::highlight_group::syn_check_group(b"RedrawDebugClear") };
    *unsafe { DBGHL_COMPOSED.get_mut() } =
        unsafe { crate::highlight_group::syn_check_group(b"RedrawDebugComposed") };
    *unsafe { DBGHL_RECOMPOSE.get_mut() } =
        unsafe { crate::highlight_group::syn_check_group(b"RedrawDebugRecompose") };
}

/// Initialize compositor layers (`ui_comp_init`).
pub fn ui_comp_init() {
    let default = std::ptr::from_mut(unsafe { crate::grid::DEFAULT_GRID.get_mut() });
    unsafe { LAYERS.get_mut() }.push(default);
    *unsafe { CURGRID.get_mut() } = default;
}

/// Release compositor-owned allocations (`ui_comp_free_all_mem`).
pub fn ui_comp_free_all_mem() {
    let layers = unsafe { LAYERS.get_mut() };
    layers.clear();
    layers.shrink_to_fit();
    *unsafe { LINEBUF.get_mut() } = None;
    *unsafe { ATTRBUF.get_mut() } = None;
    *unsafe { BUFSIZE.get_mut() } = 0;
}

/// Raise or lower a compositor layer to match its z-index
/// (`ui_comp_layers_adjust`).
///
/// # Safety
/// `layer_idx` must index `LAYERS`, whose pointers must all be live.
pub unsafe fn ui_comp_layers_adjust(mut layer_idx: usize, raise: bool) {
    let layers = unsafe { LAYERS.get_mut() };
    let size = layers.len();
    let layer = layers[layer_idx];
    if raise {
        while layer_idx < size - 1
            && unsafe { (*layer).zindex > (*layers[layer_idx + 1]).zindex }
        {
            layers[layer_idx] = layers[layer_idx + 1];
            unsafe {
                (*layers[layer_idx]).comp_index = layer_idx;
                (*layers[layer_idx]).pending_comp_index_update = true;
            }
            layer_idx += 1;
        }
    } else {
        while layer_idx > 0
            && unsafe { (*layer).zindex < (*layers[layer_idx - 1]).zindex }
        {
            layers[layer_idx] = layers[layer_idx - 1];
            unsafe {
                (*layers[layer_idx]).comp_index = layer_idx;
                (*layers[layer_idx]).pending_comp_index_update = true;
            }
            layer_idx -= 1;
        }
    }
    layers[layer_idx] = layer;
    unsafe {
        (*layer).comp_index = layer_idx;
        (*layer).pending_comp_index_update = true;
    }
}

/// Select the compositor grid with `handle` (`ui_comp_set_grid`).
///
/// # Safety
/// Every pointer in `LAYERS` and non-null `CURGRID` must be live.
#[must_use]
pub unsafe fn ui_comp_set_grid(handle: crate::types_defs::HandleT) -> bool {
    let current = unsafe { *CURGRID.get_mut() };
    if !current.is_null() && unsafe { (*current).handle } == handle {
        return true;
    }
    for &grid in unsafe { LAYERS.get_mut() }.iter() {
        if unsafe { (*grid).handle } == handle {
            *unsafe { CURGRID.get_mut() } = grid;
            return true;
        }
    }
    false
}

/// Topmost grid covering screen coordinates (`ui_comp_get_grid_at_coord`).
///
/// # Safety
/// Layer and current-tab window lists must contain live pointers.
#[must_use]
pub unsafe fn ui_comp_get_grid_at_coord(
    row: i32,
    col: i32,
) -> *mut crate::grid_defs::ScreenGrid {
    let layers = unsafe { LAYERS.get_mut() };
    for &grid in layers.iter().skip(1).rev() {
        let candidate = unsafe { &*grid };
        if row >= candidate.comp_row
            && row < candidate.comp_row + candidate.rows
            && col >= candidate.comp_col
            && col < candidate.comp_col + candidate.cols
        {
            return grid;
        }
    }

    let mut win = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !win.is_null() {
        let window = unsafe { &mut *win };
        let next = window.w_next;
        let hidden = window.w_config.hide;
        let candidate = &mut window.w_grid_alloc;
        if row >= candidate.comp_row
            && row < candidate.comp_row + candidate.rows
            && col >= candidate.comp_col
            && col < candidate.comp_col + candidate.cols
            && !hidden
        {
            return std::ptr::from_mut(candidate);
        }
        win = next;
    }
    std::ptr::from_mut(unsafe { crate::grid::DEFAULT_GRID.get_mut() })
}

/// Whether the compositor should draw (`ui_comp_should_draw`).
#[must_use]
pub fn ui_comp_should_draw() -> bool {
    unsafe { *COMPOSED_UIS.get_mut() != 0 && *VALID_SCREEN.get_mut() }
}

/// Attach a UI to the compositor (`ui_comp_attach`).
pub fn ui_comp_attach(ui: &mut crate::ui::RemoteUI) {
    unsafe {
        let count = COMPOSED_UIS.get_mut();
        *count = count.wrapping_add(1);
    }
    ui.composed = true;
}

/// Detach a UI from the compositor (`ui_comp_detach`).
pub fn ui_comp_detach(ui: &mut crate::ui::RemoteUI) {
    let count = unsafe { COMPOSED_UIS.get_mut() };
    *count = count.wrapping_sub(1);
    if *count == 0 {
        *unsafe { LINEBUF.get_mut() } = None;
        *unsafe { ATTRBUF.get_mut() } = None;
        *unsafe { BUFSIZE.get_mut() } = 0;
    }
    ui.composed = false;
}

/// Mark the composed screen valid or invalid (`ui_comp_set_screen_valid`).
///
/// Returns the previous validity; invalidating also hides the message
/// separator until the screen is cleared.
pub fn ui_comp_set_screen_valid(valid: bool) -> bool {
    let old = std::mem::replace(unsafe { VALID_SCREEN.get_mut() }, valid);
    if !valid {
        *unsafe { MSG_SEP_ROW.get_mut() } = -1;
    }
    old
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compositor_syntax_init_creates_all_debug_groups() {
        let _lock = crate::globals::global_state_test_lock();
        let saved_table =
            std::mem::take(&mut unsafe { crate::highlight_group::HL_TABLE.get_mut() }.items);
        let saved_names =
            std::mem::take(unsafe { crate::highlight_group::HIGHLIGHT_UNAMES.get_mut() });
        let saved_debug = (
            *unsafe { DBGHL_NORMAL.get_mut() },
            *unsafe { DBGHL_CLEAR.get_mut() },
            *unsafe { DBGHL_COMPOSED.get_mut() },
            *unsafe { DBGHL_RECOMPOSE.get_mut() },
        );

        unsafe { ui_comp_syn_init() };
        let got = (
            *unsafe { DBGHL_NORMAL.get_mut() },
            *unsafe { DBGHL_CLEAR.get_mut() },
            *unsafe { DBGHL_COMPOSED.get_mut() },
            *unsafe { DBGHL_RECOMPOSE.get_mut() },
        );

        unsafe { crate::highlight_group::HL_TABLE.get_mut() }.items = saved_table;
        *unsafe { crate::highlight_group::HIGHLIGHT_UNAMES.get_mut() } = saved_names;
        (
            *unsafe { DBGHL_NORMAL.get_mut() },
            *unsafe { DBGHL_CLEAR.get_mut() },
            *unsafe { DBGHL_COMPOSED.get_mut() },
            *unsafe { DBGHL_RECOMPOSE.get_mut() },
        ) = saved_debug;

        assert_eq!(got, (1, 2, 3, 4));
    }

    struct CompositorStateGuard {
        composed: i32,
        valid: bool,
    }

    struct CompositorBuffersGuard {
        line: Option<Vec<crate::types_defs::ScharT>>,
        attrs: Option<Vec<crate::types_defs::SattrT>>,
        size: usize,
    }

    struct MsgSepGuard(i32);

    struct LayerStateGuard {
        layers: Vec<*mut crate::grid_defs::ScreenGrid>,
        current: *mut crate::grid_defs::ScreenGrid,
    }

    impl LayerStateGuard {
        fn empty() -> Self {
            Self {
                layers: std::mem::take(unsafe { LAYERS.get_mut() }),
                current: std::mem::replace(
                    unsafe { CURGRID.get_mut() },
                    std::ptr::null_mut(),
                ),
            }
        }
    }

    impl Drop for LayerStateGuard {
        fn drop(&mut self) {
            *unsafe { LAYERS.get_mut() } = std::mem::take(&mut self.layers);
            *unsafe { CURGRID.get_mut() } = self.current;
        }
    }

    impl MsgSepGuard {
        fn install(value: i32) -> Self {
            Self(std::mem::replace(
                unsafe { MSG_SEP_ROW.get_mut() },
                value,
            ))
        }
    }

    impl Drop for MsgSepGuard {
        fn drop(&mut self) {
            *unsafe { MSG_SEP_ROW.get_mut() } = self.0;
        }
    }

    impl CompositorBuffersGuard {
        fn install() -> Self {
            Self {
                line: unsafe { LINEBUF.get_mut() }.replace(vec![1, 2]),
                attrs: unsafe { ATTRBUF.get_mut() }.replace(vec![3, 4]),
                size: std::mem::replace(unsafe { BUFSIZE.get_mut() }, 2),
            }
        }
    }

    impl Drop for CompositorBuffersGuard {
        fn drop(&mut self) {
            *unsafe { LINEBUF.get_mut() } = self.line.take();
            *unsafe { ATTRBUF.get_mut() } = self.attrs.take();
            *unsafe { BUFSIZE.get_mut() } = self.size;
        }
    }

    impl CompositorStateGuard {
        fn install(composed: i32, valid: bool) -> Self {
            let previous = Self {
                composed: unsafe { *COMPOSED_UIS.get_mut() },
                valid: unsafe { *VALID_SCREEN.get_mut() },
            };
            unsafe {
                *COMPOSED_UIS.get_mut() = composed;
                *VALID_SCREEN.get_mut() = valid;
            }
            previous
        }
    }

    impl Drop for CompositorStateGuard {
        fn drop(&mut self) {
            unsafe {
                *COMPOSED_UIS.get_mut() = self.composed;
                *VALID_SCREEN.get_mut() = self.valid;
            }
        }
    }

    #[test]
    fn ui_comp_should_draw_requires_a_composed_ui_and_valid_screen() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = CompositorStateGuard::install(0, true);
        assert!(!ui_comp_should_draw());

        unsafe { *COMPOSED_UIS.get_mut() = 1 };
        assert!(ui_comp_should_draw());

        unsafe { *VALID_SCREEN.get_mut() = false };
        assert!(!ui_comp_should_draw());
    }

    #[test]
    fn ui_comp_attach_marks_the_ui_and_increments_the_count() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = CompositorStateGuard::install(2, true);
        let mut ui = crate::ui::RemoteUI::default();

        ui_comp_attach(&mut ui);

        assert!(ui.composed);
        assert_eq!(unsafe { *COMPOSED_UIS.get_mut() }, 3);
        assert!(ui_comp_should_draw());
    }

    #[test]
    fn ui_comp_detach_releases_scratch_buffers_after_the_last_ui() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = CompositorStateGuard::install(1, true);
        let _buffers = CompositorBuffersGuard::install();
        let mut ui = crate::ui::RemoteUI {
            composed: true,
            ..Default::default()
        };

        ui_comp_detach(&mut ui);

        assert!(!ui.composed);
        assert_eq!(unsafe { *COMPOSED_UIS.get_mut() }, 0);
        assert!(unsafe { LINEBUF.get_mut() }.is_none());
        assert!(unsafe { ATTRBUF.get_mut() }.is_none());
        assert_eq!(unsafe { *BUFSIZE.get_mut() }, 0);
    }

    #[test]
    fn ui_comp_set_screen_valid_returns_old_state_and_resets_separator() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = CompositorStateGuard::install(1, true);
        let _separator = MsgSepGuard::install(12);

        assert!(ui_comp_set_screen_valid(false));
        assert!(!ui_comp_should_draw());
        assert_eq!(unsafe { *MSG_SEP_ROW.get_mut() }, -1);

        assert!(!ui_comp_set_screen_valid(true));
        assert!(ui_comp_should_draw());
        assert_eq!(unsafe { *MSG_SEP_ROW.get_mut() }, -1);
    }

    #[test]
    fn ui_comp_init_installs_the_default_grid_as_bottom_and_current() {
        let _lock = crate::globals::global_state_test_lock();
        let _layers = LayerStateGuard::empty();

        ui_comp_init();

        let default =
            std::ptr::from_mut(unsafe { crate::grid::DEFAULT_GRID.get_mut() });
        assert_eq!(unsafe { LAYERS.get_mut() }.as_slice(), &[default]);
        assert_eq!(unsafe { *CURGRID.get_mut() }, default);
    }

    #[test]
    fn ui_comp_free_all_mem_releases_layers_and_scratch_buffers() {
        let _lock = crate::globals::global_state_test_lock();
        let _layers = LayerStateGuard::empty();
        let _buffers = CompositorBuffersGuard::install();
        ui_comp_init();

        ui_comp_free_all_mem();

        assert!(unsafe { LAYERS.get_mut() }.is_empty());
        assert!(unsafe { LINEBUF.get_mut() }.is_none());
        assert!(unsafe { ATTRBUF.get_mut() }.is_none());
        assert_eq!(unsafe { *BUFSIZE.get_mut() }, 0);
    }

    #[test]
    fn ui_comp_layers_adjust_reorders_and_updates_indices() {
        let _lock = crate::globals::global_state_test_lock();
        let _layers = LayerStateGuard::empty();
        let mut low = Box::new(crate::grid_defs::ScreenGrid {
            zindex: 0,
            comp_index: 0,
            ..Default::default()
        });
        let mut moving = Box::new(crate::grid_defs::ScreenGrid {
            zindex: 30,
            comp_index: 1,
            ..Default::default()
        });
        let mut high = Box::new(crate::grid_defs::ScreenGrid {
            zindex: 20,
            comp_index: 2,
            ..Default::default()
        });
        let low_ptr = std::ptr::addr_of_mut!(*low);
        let moving_ptr = std::ptr::addr_of_mut!(*moving);
        let high_ptr = std::ptr::addr_of_mut!(*high);
        unsafe { LAYERS.get_mut() }.extend([low_ptr, moving_ptr, high_ptr]);

        unsafe { ui_comp_layers_adjust(1, true) };
        assert_eq!(
            unsafe { LAYERS.get_mut() }.as_slice(),
            &[low_ptr, high_ptr, moving_ptr]
        );
        assert_eq!((low.comp_index, high.comp_index, moving.comp_index), (0, 1, 2));
        assert!(high.pending_comp_index_update);
        assert!(moving.pending_comp_index_update);

        moving.zindex = 10;
        unsafe { ui_comp_layers_adjust(2, false) };
        assert_eq!(
            unsafe { LAYERS.get_mut() }.as_slice(),
            &[low_ptr, moving_ptr, high_ptr]
        );
        assert_eq!((moving.comp_index, high.comp_index), (1, 2));
    }

    #[test]
    fn ui_comp_set_grid_selects_known_handles_and_preserves_unknown_current() {
        let _lock = crate::globals::global_state_test_lock();
        let _layers = LayerStateGuard::empty();
        let mut first = Box::new(crate::grid_defs::ScreenGrid {
            handle: 1,
            ..Default::default()
        });
        let mut second = Box::new(crate::grid_defs::ScreenGrid {
            handle: 9,
            ..Default::default()
        });
        let first_ptr = std::ptr::addr_of_mut!(*first);
        let second_ptr = std::ptr::addr_of_mut!(*second);
        unsafe { LAYERS.get_mut() }.extend([first_ptr, second_ptr]);
        *unsafe { CURGRID.get_mut() } = first_ptr;

        assert!(unsafe { ui_comp_set_grid(1) });
        assert_eq!(unsafe { *CURGRID.get_mut() }, first_ptr);
        assert!(unsafe { ui_comp_set_grid(9) });
        assert_eq!(unsafe { *CURGRID.get_mut() }, second_ptr);
        assert!(!unsafe { ui_comp_set_grid(99) });
        assert_eq!(unsafe { *CURGRID.get_mut() }, second_ptr);
    }

    #[test]
    fn ui_comp_get_grid_at_coord_prefers_top_layers_then_windows_then_default() {
        let _lock = crate::globals::global_state_test_lock();
        let _layers = LayerStateGuard::empty();
        let default =
            std::ptr::from_mut(unsafe { crate::grid::DEFAULT_GRID.get_mut() });
        let mut lower = Box::new(crate::grid_defs::ScreenGrid {
            comp_row: 2,
            comp_col: 3,
            rows: 5,
            cols: 5,
            ..Default::default()
        });
        let mut upper = Box::new(crate::grid_defs::ScreenGrid {
            comp_row: 4,
            comp_col: 4,
            rows: 3,
            cols: 3,
            ..Default::default()
        });
        let lower_ptr = std::ptr::addr_of_mut!(*lower);
        let upper_ptr = std::ptr::addr_of_mut!(*upper);
        unsafe { LAYERS.get_mut() }.extend([default, lower_ptr, upper_ptr]);

        assert_eq!(unsafe { ui_comp_get_grid_at_coord(4, 4) }, upper_ptr);
        assert_eq!(unsafe { ui_comp_get_grid_at_coord(2, 3) }, lower_ptr);

        let mut win = crate::buffer_defs::WinT::default();
        win.w_grid_alloc.comp_row = 10;
        win.w_grid_alloc.comp_col = 20;
        win.w_grid_alloc.rows = 2;
        win.w_grid_alloc.cols = 4;
        let win_grid = std::ptr::addr_of_mut!(win.w_grid_alloc);
        let _first = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.firstwin,
                std::ptr::addr_of_mut!(win),
            )
        };
        assert_eq!(unsafe { ui_comp_get_grid_at_coord(10, 21) }, win_grid);
        assert_eq!(unsafe { ui_comp_get_grid_at_coord(30, 30) }, default);
    }
}
