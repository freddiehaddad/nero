//! Translated from `src/nvim/grid_defs.h` (partial: only the `kZIndex*`
//! constants, needed by `buffer_defs.h`'s `WinConfig` default value).
//!
//! The bulk of this header (`ScreenGrid`, the resizable rectangular grid
//! displayed by UI clients, and its cell-buffer management) belongs with
//! the rendering subsystem (phase 9) - deferred, not started.

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
