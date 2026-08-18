//! Translated from `src/nvim/cursor_shape.c` (tractable core only).
//!
//! `cursor_shape.c` handles cursor/mouse-pointer shape configuration via
//! the `'guicursor'`/`'mouseshape'` options. The option-string parser
//! is translated for shape/blink/mode syntax; highlight-group names
//! remain deferred on `syn_check_group`.
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
//! Also translated: [`clear_shape_table`] (resets every `SHAPE_TABLE`
//! entry's `shape`/`blinkwait`/`blinkon`/`blinkoff`/`id`/`id_lm`
//! fields, deliberately NOT `full_name`/`mshape`/`percentage`/`name`/
//! `used_for`, matching the original's own exact field list) and
//! [`cursor_get_mode_idx`] (the `SHAPE_TABLE` index for the current
//! mode), via already-real `crate::globals::GLOBALS.State`/
//! `finish_op`/`Visual.active`, `crate::state_defs::mode`'s flag
//! constants, `crate::option_vars::OPTION_VARS.p_sel`, and
//! `crate::ex_getln`'s newly-real `cmdline_at_end`/`cmdline_overstrike`.
//!
//! Deferred: `mode_style_array` (needs `Arena`/`Dict`/`Array`, the API
//! layer), highlight-group resolution in [`parse_shape_opt`],
//! `update_mouseshape`/`ui_cursor_shape_*`
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

/// Clears all entries in `SHAPE_TABLE` to block, blinkon0, and default
/// color (`clear_shape_table`).
pub fn clear_shape_table() {
    // SAFETY: a plain write through one exclusive borrow.
    let table = unsafe { SHAPE_TABLE.get_mut() };
    clear_shape_entries(table);
}

fn clear_shape_entries(table: &mut [CursorentryT; SHAPE_IDX_COUNT]) {
    for entry in table.iter_mut() {
        entry.shape = CursorShape::Block;
        entry.blinkwait = 0;
        entry.blinkon = 0;
        entry.blinkoff = 0;
        entry.id = 0;
        entry.id_lm = 0;
    }
}

const E_MISSING_COLON: &[u8] = b"E545: Missing colon";
const E_ILLEGAL_MODE: &[u8] = b"E546: Illegal mode";
const E_ILLEGAL_PERCENTAGE: &[u8] = b"E549: Illegal percentage";

#[derive(Clone, Copy)]
enum ShapeAttribute {
    Block,
    Vertical(i32),
    Horizontal(i32),
    BlinkWait(i32),
    BlinkOn(i32),
    BlinkOff(i32),
}

fn starts_with_ignore_ascii_case(value: &[u8], prefix: &[u8]) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Parse `'guicursor'` shape/blink/mode syntax (`parse_shape_opt`).
///
/// Highlight-group tokens still need `syn_check_group` and stop at
/// that exact dependency. Successful parses replace `SHAPE_TABLE`
/// atomically, matching the original's validate-then-apply two rounds.
#[must_use]
pub fn parse_shape_opt(what: u8) -> Option<&'static [u8]> {
    let value = unsafe { (*crate::option_vars::OPTION_VARS.as_ptr()).p_guicursor.clone() }
        .unwrap_or_default();
    let mut table = unsafe { *SHAPE_TABLE.get_mut() };
    clear_shape_entries(&mut table);
    if value.is_empty() {
        unsafe { *SHAPE_TABLE.get_mut() = table };
        return None;
    }

    let mut found_ve = false;
    for part in value.split(|&byte| byte == b',') {
        let Some(colon) = part.iter().position(|&byte| byte == b':') else {
            return Some(E_MISSING_COLON);
        };
        if colon == 0 {
            return Some(E_ILLEGAL_MODE);
        }

        let mut modes = Vec::new();
        for mode in part[..colon].split(|&byte| byte == b'-') {
            if mode.len() == 1 && mode[0].eq_ignore_ascii_case(&b'a') {
                modes.extend(0..SHAPE_IDX_COUNT);
                continue;
            }
            let Some(index) = table.iter().position(|entry| {
                entry.name.as_bytes().eq_ignore_ascii_case(mode)
            }) else {
                return Some(E_ILLEGAL_MODE);
            };
            if table[index].used_for & what == 0 {
                return Some(E_ILLEGAL_MODE);
            }
            found_ve |= index == SHAPE_IDX_VE;
            modes.push(index);
        }

        let mut attributes = Vec::new();
        let tail = &part[colon + 1..];
        if !tail.is_empty() {
            for attribute in tail.split(|&byte| byte == b'-') {
                let parsed = if attribute.eq_ignore_ascii_case(b"block") {
                    ShapeAttribute::Block
                } else {
                    let (kind, prefix_len) = if starts_with_ignore_ascii_case(attribute, b"ver") {
                        (0, 3)
                    } else if starts_with_ignore_ascii_case(attribute, b"hor") {
                        (1, 3)
                    } else if starts_with_ignore_ascii_case(attribute, b"blinkwait") {
                        (2, 9)
                    } else if starts_with_ignore_ascii_case(attribute, b"blinkon") {
                        (3, 7)
                    } else if starts_with_ignore_ascii_case(attribute, b"blinkoff") {
                        (4, 8)
                    } else {
                        unimplemented!(
                            "parse_shape_opt: highlight-group names need syn_check_group"
                        );
                    };
                    let digits = &attribute[prefix_len..];
                    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
                        return Some(crate::gettext_defs::gettext_noop(
                            "E5080: Digit expected",
                        )
                        .as_bytes());
                    }
                    let number = digits
                        .iter()
                        .fold(0i32, |value, &digit| {
                            value
                                .saturating_mul(10)
                                .saturating_add(i32::from(digit - b'0'))
                        });
                    match kind {
                        0 if number == 0 => return Some(E_ILLEGAL_PERCENTAGE),
                        1 if number == 0 => return Some(E_ILLEGAL_PERCENTAGE),
                        0 => ShapeAttribute::Vertical(number),
                        1 => ShapeAttribute::Horizontal(number),
                        2 => ShapeAttribute::BlinkWait(number),
                        3 => ShapeAttribute::BlinkOn(number),
                        _ => ShapeAttribute::BlinkOff(number),
                    }
                };
                attributes.push(parsed);
            }
        }

        for index in modes {
            for attribute in &attributes {
                match *attribute {
                    ShapeAttribute::Block => table[index].shape = CursorShape::Block,
                    ShapeAttribute::Vertical(percent) => {
                        table[index].shape = CursorShape::Ver;
                        table[index].percentage = percent;
                    }
                    ShapeAttribute::Horizontal(percent) => {
                        table[index].shape = CursorShape::Hor;
                        table[index].percentage = percent;
                    }
                    ShapeAttribute::BlinkWait(value) => table[index].blinkwait = value,
                    ShapeAttribute::BlinkOn(value) => table[index].blinkon = value,
                    ShapeAttribute::BlinkOff(value) => table[index].blinkoff = value,
                }
            }
        }
    }

    if !found_ve {
        let visual = table[SHAPE_IDX_V];
        table[SHAPE_IDX_VE].shape = visual.shape;
        table[SHAPE_IDX_VE].percentage = visual.percentage;
        table[SHAPE_IDX_VE].blinkwait = visual.blinkwait;
        table[SHAPE_IDX_VE].blinkon = visual.blinkon;
        table[SHAPE_IDX_VE].blinkoff = visual.blinkoff;
        table[SHAPE_IDX_VE].id = visual.id;
        table[SHAPE_IDX_VE].id_lm = visual.id_lm;
    }
    unsafe { *SHAPE_TABLE.get_mut() = table };
    None
}

/// Return the index into `SHAPE_TABLE` for the current mode
/// (`cursor_get_mode_idx`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS`/`crate::option_vars::OPTION_VARS`.
// The VREPLACE_FLAG and REPLACE_FLAG branches both resolve to
// SHAPE_IDX_R - clippy's `if_same_then_else` flags this as
// suspicious, but it's a faithful match to the original's own 2
// separate `else if` branches: VREPLACE mode always also sets
// REPLACE_FLAG (VREPLACE = REPLACE_FLAG | VREPLACE_FLAG | INSERT), so
// checking VREPLACE_FLAG first is a real, independently-meaningful
// distinction in the original that just happens to produce the same
// index today, not an accidental copy-paste duplication.
#[allow(clippy::if_same_then_else)]
#[must_use]
pub unsafe fn cursor_get_mode_idx() -> usize {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let state = g.State;
    if state == crate::state_defs::mode::SHOWMATCH as i32 {
        SHAPE_IDX_SM
    } else if state == crate::state_defs::mode::TERMINAL as i32 {
        SHAPE_IDX_TERM
    } else if state & crate::state_defs::mode::VREPLACE_FLAG as i32 != 0 {
        SHAPE_IDX_R
    } else if state & crate::state_defs::mode::REPLACE_FLAG as i32 != 0 {
        SHAPE_IDX_R
    } else if state & crate::state_defs::mode::INSERT as i32 != 0 {
        SHAPE_IDX_I
    } else if state & crate::state_defs::mode::CMDLINE as i32 != 0 {
        if crate::ex_getln::cmdline_at_end() {
            SHAPE_IDX_C
        } else if crate::ex_getln::cmdline_overstrike() {
            SHAPE_IDX_CR
        } else {
            SHAPE_IDX_CI
        }
    } else if g.finish_op {
        SHAPE_IDX_O
    } else if g.Visual.active {
        // SAFETY: forwarded from this function's own safety doc.
        let p_sel_is_exclusive = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_sel
            .as_deref()
            .and_then(<[u8]>::first)
            == Some(&b'e');
        if p_sel_is_exclusive { SHAPE_IDX_VE } else { SHAPE_IDX_V }
    } else {
        SHAPE_IDX_N
    }
}

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

    struct ShapeParseGuard {
        table: [CursorentryT; SHAPE_IDX_COUNT],
        option: Option<Vec<u8>>,
    }

    impl ShapeParseGuard {
        fn set(value: &[u8]) -> Self {
            let options = crate::option_vars::OPTION_VARS.as_ptr();
            let guard = ShapeParseGuard {
                table: unsafe { *SHAPE_TABLE.get_mut() },
                option: unsafe { (*options).p_guicursor.clone() },
            };
            unsafe { (*options).p_guicursor = Some(value.to_vec()) };
            guard
        }

        fn replace_option(value: &[u8]) {
            unsafe { (*crate::option_vars::OPTION_VARS.as_ptr()).p_guicursor = Some(value.to_vec()) };
        }
    }

    impl Drop for ShapeParseGuard {
        fn drop(&mut self) {
            unsafe {
                *SHAPE_TABLE.get_mut() = self.table;
                (*crate::option_vars::OPTION_VARS.as_ptr()).p_guicursor =
                    self.option.take();
            }
        }
    }

    #[test]
    fn parse_shape_opt_applies_shape_blink_and_mode_lists() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShapeParseGuard::set(
            b"n:block-blinkon0,i-ci-ve:ver25-blinkwait100,r-cr-o:hor20",
        );
        assert_eq!(parse_shape_opt(SHAPE_CURSOR), None);
        let table = unsafe { SHAPE_TABLE.get_mut() };
        assert_eq!(table[SHAPE_IDX_N].shape, CursorShape::Block);
        assert_eq!(table[SHAPE_IDX_N].blinkon, 0);
        for index in [SHAPE_IDX_I, SHAPE_IDX_CI, SHAPE_IDX_VE] {
            assert_eq!(table[index].shape, CursorShape::Ver);
            assert_eq!(table[index].percentage, 25);
            assert_eq!(table[index].blinkwait, 100);
        }
        for index in [SHAPE_IDX_R, SHAPE_IDX_CR, SHAPE_IDX_O] {
            assert_eq!(table[index].shape, CursorShape::Hor);
            assert_eq!(table[index].percentage, 20);
        }
    }

    #[test]
    fn parse_shape_opt_all_mode_and_visual_fallback_match_the_original() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShapeParseGuard::set(b"a:block-blinkwait123");
        assert_eq!(parse_shape_opt(SHAPE_CURSOR), None);
        assert!(unsafe { SHAPE_TABLE.get_mut() }
            .iter()
            .all(|entry| entry.shape == CursorShape::Block && entry.blinkwait == 123));

        ShapeParseGuard::replace_option(b"v:hor30-blinkon0");
        assert_eq!(parse_shape_opt(SHAPE_CURSOR), None);
        let table = unsafe { SHAPE_TABLE.get_mut() };
        assert_eq!(table[SHAPE_IDX_VE].shape, CursorShape::Hor);
        assert_eq!(table[SHAPE_IDX_VE].percentage, 30);
        assert_eq!(table[SHAPE_IDX_VE].blinkon, 0);
    }

    #[test]
    fn parse_shape_opt_rejects_malformed_mode_and_numeric_fields() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShapeParseGuard::set(b"nblock");
        assert_eq!(parse_shape_opt(SHAPE_CURSOR), Some(E_MISSING_COLON));

        ShapeParseGuard::replace_option(b"x:block");
        assert_eq!(parse_shape_opt(SHAPE_CURSOR), Some(E_ILLEGAL_MODE));

        ShapeParseGuard::replace_option(b"n:ver0");
        assert_eq!(parse_shape_opt(SHAPE_CURSOR), Some(E_ILLEGAL_PERCENTAGE));

        ShapeParseGuard::replace_option(b"n:blinkon");
        assert!(parse_shape_opt(SHAPE_CURSOR).is_some());
    }

    #[test]
    #[should_panic(expected = "syn_check_group")]
    fn parse_shape_opt_highlight_names_need_the_highlight_registry() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = ShapeParseGuard::set(b"n:TermCursor");
        let _ = parse_shape_opt(SHAPE_CURSOR);
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

    // ---- clear_shape_table ----

    #[test]
    fn clear_shape_table_resets_shape_blink_and_id_but_not_other_fields() {
        let _lock = crate::globals::global_state_test_lock();
        // clear_shape_table() resets EVERY entry, not just one - save
        // and restore the whole table, not only SHAPE_IDX_N (a real
        // test-fixture bug caught via the FULL test suite: an earlier
        // version of this test left every OTHER entry's blinkon/etc.
        // zeroed out, which broke cursor_is_block_during_visual's own
        // "blinkon == 400 by default" assumption in a later test).
        // SAFETY: a plain read through one exclusive borrow.
        let saved_table = unsafe { *SHAPE_TABLE.get_mut() };
        set_shape_table_entry(
            SHAPE_IDX_N,
            CursorentryT {
                full_name: "normal",
                shape: CursorShape::Ver,
                mshape: 5,
                percentage: 50,
                blinkwait: 700,
                blinkon: 400,
                blinkoff: 250,
                id: 42,
                id_lm: 7,
                name: "n",
                used_for: SHAPE_CURSOR + SHAPE_MOUSE,
            },
        );

        clear_shape_table();

        // SAFETY: a plain read through one exclusive borrow.
        let entry = unsafe { SHAPE_TABLE.get_mut()[SHAPE_IDX_N] };
        assert_eq!(entry.shape, CursorShape::Block);
        assert_eq!(entry.blinkwait, 0);
        assert_eq!(entry.blinkon, 0);
        assert_eq!(entry.blinkoff, 0);
        assert_eq!(entry.id, 0);
        assert_eq!(entry.id_lm, 0);
        // Not touched by clear_shape_table.
        assert_eq!(entry.full_name, "normal");
        assert_eq!(entry.mshape, 5);
        assert_eq!(entry.percentage, 50);
        assert_eq!(entry.name, "n");
        assert_eq!(entry.used_for, SHAPE_CURSOR + SHAPE_MOUSE);

        // SAFETY: a plain write through one exclusive borrow.
        unsafe { *SHAPE_TABLE.get_mut() = saved_table };
    }

    // ---- cursor_get_mode_idx ----

    /// RAII guard for `cursor_get_mode_idx` tests: saves/restores
    /// every `GLOBALS`/`OPTION_VARS` field it reads.
    struct CursorModeIdxGuard {
        prev_state: i32,
        prev_finish_op: bool,
        prev_visual_active: bool,
        prev_p_sel: Option<Vec<u8>>,
    }
    impl CursorModeIdxGuard {
        fn set(state: i32, finish_op: bool, visual_active: bool, p_sel: Option<&[u8]>) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard = CursorModeIdxGuard {
                prev_state: g.State,
                prev_finish_op: g.finish_op,
                prev_visual_active: g.Visual.active,
                prev_p_sel: opts.p_sel.clone(),
            };
            g.State = state;
            g.finish_op = finish_op;
            g.Visual.active = visual_active;
            opts.p_sel = p_sel.map(<[u8]>::to_vec);
            guard
        }
    }
    impl Drop for CursorModeIdxGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            g.State = self.prev_state;
            g.finish_op = self.prev_finish_op;
            g.Visual.active = self.prev_visual_active;
            opts.p_sel = self.prev_p_sel.take();
        }
    }

    #[test]
    fn cursor_get_mode_idx_showmatch() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = CursorModeIdxGuard::set(
            crate::state_defs::mode::SHOWMATCH as i32,
            false,
            false,
            None,
        );
        assert_eq!(unsafe { cursor_get_mode_idx() }, SHAPE_IDX_SM);
    }

    #[test]
    fn cursor_get_mode_idx_terminal() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard =
            CursorModeIdxGuard::set(crate::state_defs::mode::TERMINAL as i32, false, false, None);
        assert_eq!(unsafe { cursor_get_mode_idx() }, SHAPE_IDX_TERM);
    }

    #[test]
    fn cursor_get_mode_idx_vreplace() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = CursorModeIdxGuard::set(
            crate::state_defs::mode::VREPLACE as i32,
            false,
            false,
            None,
        );
        assert_eq!(unsafe { cursor_get_mode_idx() }, SHAPE_IDX_R);
    }

    #[test]
    fn cursor_get_mode_idx_replace_without_vreplace() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard =
            CursorModeIdxGuard::set(crate::state_defs::mode::REPLACE as i32, false, false, None);
        assert_eq!(unsafe { cursor_get_mode_idx() }, SHAPE_IDX_R);
    }

    #[test]
    fn cursor_get_mode_idx_insert() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard =
            CursorModeIdxGuard::set(crate::state_defs::mode::INSERT as i32, false, false, None);
        assert_eq!(unsafe { cursor_get_mode_idx() }, SHAPE_IDX_I);
    }

    #[test]
    fn cursor_get_mode_idx_cmdline_uses_cmdline_at_end() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard =
            CursorModeIdxGuard::set(crate::state_defs::mode::CMDLINE as i32, false, false, None);
        // cmdline_at_end() is always true today (both CMDLINE_CMDPOS
        // and CMDLINE_CMDLEN default to 0).
        assert_eq!(unsafe { cursor_get_mode_idx() }, SHAPE_IDX_C);
    }

    #[test]
    fn cursor_get_mode_idx_finish_op() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = CursorModeIdxGuard::set(0, true, false, None);
        assert_eq!(unsafe { cursor_get_mode_idx() }, SHAPE_IDX_O);
    }

    #[test]
    fn cursor_get_mode_idx_visual_inclusive_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = CursorModeIdxGuard::set(0, false, true, None);
        assert_eq!(unsafe { cursor_get_mode_idx() }, SHAPE_IDX_V);
    }

    #[test]
    fn cursor_get_mode_idx_visual_exclusive_when_p_sel_starts_with_e() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = CursorModeIdxGuard::set(0, false, true, Some(b"exclusive"));
        assert_eq!(unsafe { cursor_get_mode_idx() }, SHAPE_IDX_VE);
    }

    #[test]
    fn cursor_get_mode_idx_normal_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = CursorModeIdxGuard::set(0, false, false, None);
        assert_eq!(unsafe { cursor_get_mode_idx() }, SHAPE_IDX_N);
    }
}
