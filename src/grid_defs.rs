//! Translated from `src/nvim/grid_defs.h`.

use crate::pos_defs::ColnrT;
use crate::types_defs::{HandleT, SattrT, ScharT};

/// Default z-index for a normal (non-floating) grid.
pub const K_ZINDEX_DEFAULT_GRID: i32 = 0;
/// Default z-index for a floating window (`kZIndexFloatDefault`).
pub const K_ZINDEX_FLOAT_DEFAULT: i32 = 50;
/// Z-index for the popup menu.
pub const K_ZINDEX_POPUP_MENU: i32 = 100;
/// Z-index for messages.
pub const K_ZINDEX_MESSAGES: i32 = 200;
/// Z-index for the cmdline popup menu.
pub const K_ZINDEX_CMDLINE_POPUP_MENU: i32 = 250;

/// `ScreenGrid` represents a resizable rectangular grid displayed by UI
/// clients.
///
/// `chars[]` contains the UTF-8 text that is currently displayed on the
/// grid. It is stored as a single block of cells. When redrawing a part
/// of the grid, the new state can be compared with the existing state of
/// the grid. This way we can avoid sending bigger updates than necessary
/// to the UI layer.
///
/// Screen cells are stored as NUL-terminated UTF-8 strings, and a cell
/// can contain composing characters as many as fits in
/// `MAX_SCHAR_SIZE-1` bytes. The composing characters are to be drawn on
/// top of the original character. The content after the NUL is not
/// defined (so comparison must be done a single cell at a time).
/// Double-width characters are stored in the left cell, and the right
/// cell should only contain the empty string. When a part of the screen
/// is cleared, the cells should be filled with a single whitespace char.
///
/// `attrs[]` contains the highlighting attribute for each cell.
///
/// `vcols[]` contains the virtual columns in the line. -1 means not
/// available or before buffer text. -2 or -3 means in fold column and a
/// mouse click should: -2: open a fold, -3: close a fold.
///
/// `line_offset[n]` is the offset from `chars[]`, `attrs[]` and
/// `vcols[]` for the start of line `n`. These offsets are in general not
/// linear, as full screen scrolling is implemented by rotating the
/// offsets in the `line_offset` array.
///
/// The four parallel-array pointers (`chars`/`attrs`/`vcols`/
/// `line_offset`) and `dirty_col` stay raw pointers to not-yet-allocated
/// buffers, matching this crate's established convention for
/// not-yet-translated owning subsystems (`grid.c`'s `grid_alloc()` and
/// friends, phase 9, own the actual allocation/layout logic).
pub struct ScreenGrid {
    pub handle: HandleT,

    pub chars: *mut ScharT,
    pub attrs: *mut SattrT,
    pub vcols: *mut ColnrT,
    pub line_offset: *mut usize,

    /// last column that was drawn (not cleared with the default
    /// background). Only used when `throttled` is set. Not allocated by
    /// `grid_alloc()`.
    pub dirty_col: *mut i32,

    /// the size of the allocated grid.
    pub rows: i32,
    pub cols: i32,

    /// The state of the grid is valid. Otherwise it needs to be redrawn.
    pub valid: bool,

    /// only draw internally and don't send updates yet to the
    /// compositor or external UI.
    pub throttled: bool,

    /// whether the compositor should blend the grid with the background
    /// grid
    pub blending: bool,

    /// whether the grid interacts with mouse events
    pub mouse_enabled: bool,

    /// z-index: the order in the stack of grids.
    pub zindex: i32,

    // Below is state owned by the compositor. Should generally not be
    // set/read outside this module, except for specific compatibility
    // hacks.
    /// position of the grid on the composed screen.
    pub comp_row: i32,
    pub comp_col: i32,

    /// Requested width and height of the grid upon resize. Used by
    /// `ui_compositor` to correctly determine which regions need to be
    /// redrawn.
    pub comp_width: i32,
    pub comp_height: i32,

    /// z-index of the grid. Grids with higher index are drawn on top.
    /// `default_grid.comp_index` is always zero.
    pub comp_index: usize,

    /// compositor should momentarily ignore the grid. Used internally
    /// when moving around grids etc.
    pub comp_disabled: bool,

    /// need to resend `win_float_pos` or similar due to `comp_index`
    /// change
    pub pending_comp_index_update: bool,
}

impl Default for ScreenGrid {
    /// `SCREEN_GRID_INIT`
    fn default() -> Self {
        ScreenGrid {
            handle: 0,
            chars: std::ptr::null_mut(),
            attrs: std::ptr::null_mut(),
            vcols: std::ptr::null_mut(),
            line_offset: std::ptr::null_mut(),
            dirty_col: std::ptr::null_mut(),
            rows: 0,
            cols: 0,
            valid: false,
            throttled: false,
            blending: false,
            mouse_enabled: true,
            zindex: 0,
            comp_row: 0,
            comp_col: 0,
            comp_width: 0,
            comp_height: 0,
            comp_index: 0,
            comp_disabled: false,
            pending_comp_index_update: true,
        }
    }
}

/// Represents the position of a viewport within a [`ScreenGrid`]
/// (`GridView`).
pub struct GridView {
    pub target: *mut ScreenGrid,
    pub row_offset: i32,
    pub col_offset: i32,
}

impl Default for GridView {
    fn default() -> Self {
        GridView {
            target: std::ptr::null_mut(),
            row_offset: 0,
            col_offset: 0,
        }
    }
}

/// `GridLineEvent`.
#[derive(Debug, Clone, Copy, Default)]
pub struct GridLineEvent {
    pub args: [i32; 3],
    pub icell: i32,
    pub ncells: i32,
    pub coloff: i32,
    pub cur_attr: i32,
    pub clear_width: i32,
    pub wrap: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zindex_constants_match_c_enum() {
        assert_eq!(K_ZINDEX_DEFAULT_GRID, 0);
        assert_eq!(K_ZINDEX_FLOAT_DEFAULT, 50);
        assert_eq!(K_ZINDEX_POPUP_MENU, 100);
        assert_eq!(K_ZINDEX_MESSAGES, 200);
        assert_eq!(K_ZINDEX_CMDLINE_POPUP_MENU, 250);
    }

    #[test]
    fn screen_grid_default_matches_screen_grid_init_macro() {
        let g = ScreenGrid::default();
        assert!(g.chars.is_null());
        assert!(!g.valid);
        assert!(g.mouse_enabled);
        assert!(g.pending_comp_index_update);
        assert_eq!(g.rows, 0);
        assert_eq!(g.cols, 0);
    }

    #[test]
    fn grid_view_default_has_null_target() {
        let gv = GridView::default();
        assert!(gv.target.is_null());
        assert_eq!(gv.row_offset, 0);
        assert_eq!(gv.col_offset, 0);
    }
}

