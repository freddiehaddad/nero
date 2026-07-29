//! Translated from `src/nvim/cursor_shape.c` (tractable core only).
//!
//! `cursor_shape.c` handles cursor/mouse-pointer shape configuration via
//! the `'guicursor'`/`'mouseshape'` options. The option-string *parser*
//! (`parse_shape_opt`, needing `cmdline_at_end`/`cmdline_overstrike`/
//! `syn_check_group`/UI mode-info plumbing, none translated) and
//! everything depending on real syntax-highlight attribute lookup
//! (`syn_id2attr`, needing the syntax subsystem) are not attempted here.
//!
//! Translated: `SHAPE_TABLE` (`shape_table`, with its own real static
//! initializer values - `parse_shape_opt`, the only thing that would
//! ever repopulate it from a parsed `'guicursor'`/`'mouseshape'` value,
//! is not translated, so every entry stays at its literal C initializer
//! values in this crate today, matching the established "translate the
//! real current state, not a hardcoded shortcut" pattern), plus 3 pure
//! functions reading it: [`cursor_is_block_during_visual`],
//! [`cursor_mode_str2int`], [`cursor_mode_uses_syn_id`].
//!
//! Note this means [`cursor_is_block_during_visual`] provably always
//! returns `false` today: both `SHAPE_IDX_V` and `SHAPE_IDX_VE` have
//! `blinkon == 400` in the real static initializer (not `0`), so its
//! `blinkon == 0` condition never holds - a real, faithful consequence
//! of `shape_table` not yet being reparsed from option values, not a
//! hardcoded stub.
//!
//! Deferred: `mode_style_array` (needs `Arena`/`Dict`/`Array`, the API
//! layer), `parse_shape_opt`, `update_mouseshape`/`ui_cursor_shape_*`
//! (UI dispatch), `get_default_cursor_shape` and friends.

use crate::globals::GlobalCell;

/// Indexes into `SHAPE_TABLE` (`ModeShape`/`SHAPE_IDX_*`).
pub const SHAPE_IDX_N: usize = 0;
pub const SHAPE_IDX_V: usize = 1;
pub const SHAPE_IDX_I: usize = 2;
pub const SHAPE_IDX_R: usize = 3;
pub const SHAPE_IDX_C: usize = 4;
pub const SHAPE_IDX_CI: usize = 5;
pub const SHAPE_IDX_CR: usize = 6;
pub const SHAPE_IDX_O: usize = 7;
pub const SHAPE_IDX_VE: usize = 8;
pub const SHAPE_IDX_CLINE: usize = 9;
pub const SHAPE_IDX_STATUS: usize = 10;
pub const SHAPE_IDX_SDRAG: usize = 11;
pub const SHAPE_IDX_VSEP: usize = 12;
pub const SHAPE_IDX_VDRAG: usize = 13;
pub const SHAPE_IDX_MORE: usize = 14;
pub const SHAPE_IDX_MOREL: usize = 15;
pub const SHAPE_IDX_SM: usize = 16;
pub const SHAPE_IDX_TERM: usize = 17;
pub const SHAPE_IDX_COUNT: usize = 18;

/// used for mouse pointer shape (`SHAPE_MOUSE`).
pub const SHAPE_MOUSE: u8 = 1;
/// used for text cursor shape (`SHAPE_CURSOR`).
pub const SHAPE_CURSOR: u8 = 2;

/// Cursor shape kind (`CursorShape`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    /// block cursor (`SHAPE_BLOCK`).
    Block,
    /// horizontal bar cursor (`SHAPE_HOR`).
    Hor,
    /// vertical bar cursor (`SHAPE_VER`).
    Ver,
}

/// One entry of `SHAPE_TABLE`: stored values from `'guicursor'` and
/// `'mouseshape'` for a single mode (`cursorentry_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorentryT {
    /// mode description (`full_name`).
    pub full_name: &'static str,
    /// cursor shape (`shape`).
    pub shape: CursorShape,
    /// mouse shape: one of the `MSHAPE_*` defines (`mshape`).
    pub mshape: i32,
    /// percentage of cell for bar (`percentage`).
    pub percentage: i32,
    /// blinking, wait time before blinking starts (`blinkwait`).
    pub blinkwait: i32,
    /// blinking, on time (`blinkon`).
    pub blinkon: i32,
    /// blinking, off time (`blinkoff`).
    pub blinkoff: i32,
    /// highlight group ID (`id`).
    pub id: i32,
    /// highlight group ID for `:lmap` mode (`id_lm`).
    pub id_lm: i32,
    /// mode short name (`name`).
    pub name: &'static str,
    /// [`SHAPE_MOUSE`] and/or [`SHAPE_CURSOR`] (`used_for`).
    pub used_for: u8,
}

/// `shape_table` - handling of cursor and mouse pointer shapes in
/// various modes. Values are set by `'guicursor'` and `'mouseshape'`
/// (via `parse_shape_opt`, not yet translated - see this module's own
/// doc comment).
static SHAPE_TABLE: GlobalCell<[CursorentryT; SHAPE_IDX_COUNT]> = GlobalCell::new([
    CursorentryT {
        full_name: "normal",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 700,
        blinkon: 400,
        blinkoff: 250,
        id: 0,
        id_lm: 0,
        name: "n",
        used_for: SHAPE_CURSOR + SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "visual",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 700,
        blinkon: 400,
        blinkoff: 250,
        id: 0,
        id_lm: 0,
        name: "v",
        used_for: SHAPE_CURSOR + SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "insert",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 700,
        blinkon: 400,
        blinkoff: 250,
        id: 0,
        id_lm: 0,
        name: "i",
        used_for: SHAPE_CURSOR + SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "replace",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 700,
        blinkon: 400,
        blinkoff: 250,
        id: 0,
        id_lm: 0,
        name: "r",
        used_for: SHAPE_CURSOR + SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "cmdline_normal",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 700,
        blinkon: 400,
        blinkoff: 250,
        id: 0,
        id_lm: 0,
        name: "c",
        used_for: SHAPE_CURSOR + SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "cmdline_insert",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 700,
        blinkon: 400,
        blinkoff: 250,
        id: 0,
        id_lm: 0,
        name: "ci",
        used_for: SHAPE_CURSOR + SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "cmdline_replace",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 700,
        blinkon: 400,
        blinkoff: 250,
        id: 0,
        id_lm: 0,
        name: "cr",
        used_for: SHAPE_CURSOR + SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "operator",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 700,
        blinkon: 400,
        blinkoff: 250,
        id: 0,
        id_lm: 0,
        name: "o",
        used_for: SHAPE_CURSOR + SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "visual_select",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 700,
        blinkon: 400,
        blinkoff: 250,
        id: 0,
        id_lm: 0,
        name: "ve",
        used_for: SHAPE_CURSOR + SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "cmdline_hover",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 0,
        blinkon: 0,
        blinkoff: 0,
        id: 0,
        id_lm: 0,
        name: "e",
        used_for: SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "statusline_hover",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 0,
        blinkon: 0,
        blinkoff: 0,
        id: 0,
        id_lm: 0,
        name: "s",
        used_for: SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "statusline_drag",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 0,
        blinkon: 0,
        blinkoff: 0,
        id: 0,
        id_lm: 0,
        name: "sd",
        used_for: SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "vsep_hover",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 0,
        blinkon: 0,
        blinkoff: 0,
        id: 0,
        id_lm: 0,
        name: "vs",
        used_for: SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "vsep_drag",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 0,
        blinkon: 0,
        blinkoff: 0,
        id: 0,
        id_lm: 0,
        name: "vd",
        used_for: SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "more",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 0,
        blinkon: 0,
        blinkoff: 0,
        id: 0,
        id_lm: 0,
        name: "m",
        used_for: SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "more_lastline",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 0,
        blinkon: 0,
        blinkoff: 0,
        id: 0,
        id_lm: 0,
        name: "ml",
        used_for: SHAPE_MOUSE,
    },
    CursorentryT {
        full_name: "showmatch",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 100,
        blinkon: 100,
        blinkoff: 100,
        id: 0,
        id_lm: 0,
        name: "sm",
        used_for: SHAPE_CURSOR,
    },
    CursorentryT {
        full_name: "terminal",
        shape: CursorShape::Block,
        mshape: 0,
        percentage: 0,
        blinkwait: 0,
        blinkon: 0,
        blinkoff: 0,
        id: 0,
        id_lm: 0,
        name: "t",
        used_for: SHAPE_CURSOR,
    },
]);

/// Whether the cursor is a block cursor with blinking disabled during
/// Visual mode - `exclusive` selects `'selection'` exclusive
/// ([`SHAPE_IDX_VE`]) vs. inclusive ([`SHAPE_IDX_V`])
/// (`cursor_is_block_during_visual`).
#[must_use]
pub fn cursor_is_block_during_visual(exclusive: bool) -> bool {
    let mode_idx = if exclusive { SHAPE_IDX_VE } else { SHAPE_IDX_V };
    // SAFETY: a plain read through one exclusive borrow.
    let entry = unsafe { SHAPE_TABLE.get_mut()[mode_idx] };
    entry.shape == CursorShape::Block && entry.blinkon == 0
}

/// Maps a cursor mode's full name to its `SHAPE_TABLE` index, or `-1`
/// if `mode` is not a known mode name (`cursor_mode_str2int`).
#[must_use]
pub fn cursor_mode_str2int(mode: &str) -> i32 {
    // SAFETY: a plain read through one exclusive borrow.
    let table = unsafe { SHAPE_TABLE.get_mut() };
    for (mode_idx, entry) in table.iter().enumerate() {
        if entry.full_name == mode {
            return mode_idx as i32;
        }
    }
    crate::log::logmsg(
        crate::log::LOGLVL_WRN,
        None,
        Some("cursor_mode_str2int"),
        Some(line!() as i32),
        true,
        &format!("Unknown mode {mode}"),
    );
    -1
}

/// Whether `syn_id` is used as a cursor style's highlight group (either
/// its normal `id` or its `:lmap`-mode `id_lm`) - always `false` while
/// `'guicursor'` is empty (`cursor_mode_uses_syn_id`).
#[must_use]
pub fn cursor_mode_uses_syn_id(syn_id: i32) -> bool {
    let guicursor_is_empty = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_guicursor
        .as_deref()
        .unwrap_or(&[])
        .is_empty();
    if guicursor_is_empty {
        return false;
    }
    // SAFETY: a plain read through one exclusive borrow.
    let table = unsafe { SHAPE_TABLE.get_mut() };
    table.iter().any(|entry| entry.id == syn_id || entry.id_lm == syn_id)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Test-only helper letting tests temporarily overwrite one
    /// [`SHAPE_TABLE`] entry. Caller must hold
    /// `crate::globals::global_state_test_lock()` for the whole
    /// duration this value matters, and should restore the original
    /// entry before releasing the lock.
    pub(crate) fn set_shape_table_entry(mode_idx: usize, entry: CursorentryT) -> CursorentryT {
        let table = unsafe { SHAPE_TABLE.get_mut() };
        let old = table[mode_idx];
        table[mode_idx] = entry;
        old
    }

    #[test]
    fn cursor_is_block_during_visual_is_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        // Real static initializer: blinkon == 400 for both V and VE, so
        // the `blinkon == 0` condition never holds today.
        assert!(!cursor_is_block_during_visual(false));
        assert!(!cursor_is_block_during_visual(true));
    }

    #[test]
    fn cursor_is_block_during_visual_true_when_block_and_no_blink() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_shape_table_entry(
            SHAPE_IDX_V,
            CursorentryT {
                full_name: "visual",
                shape: CursorShape::Block,
                mshape: 0,
                percentage: 0,
                blinkwait: 0,
                blinkon: 0,
                blinkoff: 0,
                id: 0,
                id_lm: 0,
                name: "v",
                used_for: SHAPE_CURSOR + SHAPE_MOUSE,
            },
        );
        assert!(cursor_is_block_during_visual(false));
        set_shape_table_entry(SHAPE_IDX_V, old);
    }

    #[test]
    fn cursor_is_block_during_visual_false_when_not_block_shape() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_shape_table_entry(SHAPE_IDX_VE, CursorentryT {
            full_name: "visual_select",
            shape: CursorShape::Ver,
            mshape: 0,
            percentage: 0,
            blinkwait: 0,
            blinkon: 0,
            blinkoff: 0,
            id: 0,
            id_lm: 0,
            name: "ve",
            used_for: SHAPE_CURSOR + SHAPE_MOUSE,
        });
        assert!(!cursor_is_block_during_visual(true));
        set_shape_table_entry(SHAPE_IDX_VE, old);
    }

    #[test]
    fn cursor_mode_str2int_finds_known_modes() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(cursor_mode_str2int("normal"), SHAPE_IDX_N as i32);
        assert_eq!(cursor_mode_str2int("terminal"), SHAPE_IDX_TERM as i32);
        assert_eq!(cursor_mode_str2int("visual_select"), SHAPE_IDX_VE as i32);
    }

    #[test]
    fn cursor_mode_str2int_unknown_mode_is_negative_one() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(cursor_mode_str2int("not_a_real_mode"), -1);
    }

    #[test]
    fn cursor_mode_uses_syn_id_false_when_guicursor_empty() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_guicursor = None;
        assert!(!cursor_mode_uses_syn_id(1));

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_guicursor = Some(Vec::new());
        assert!(!cursor_mode_uses_syn_id(1));
    }

    #[test]
    fn cursor_mode_uses_syn_id_true_when_id_matches() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_guicursor = Some(b"n-v-c:block".to_vec());
        let old = set_shape_table_entry(SHAPE_IDX_N, CursorentryT {
            full_name: "normal",
            shape: CursorShape::Block,
            mshape: 0,
            percentage: 0,
            blinkwait: 700,
            blinkon: 400,
            blinkoff: 250,
            id: 42,
            id_lm: 0,
            name: "n",
            used_for: SHAPE_CURSOR + SHAPE_MOUSE,
        });

        assert!(cursor_mode_uses_syn_id(42));
        assert!(!cursor_mode_uses_syn_id(43));

        set_shape_table_entry(SHAPE_IDX_N, old);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_guicursor = None;
    }
}
