//! Translated from `src/nvim/statusline.c` (tractable core only).
//!
//! `statusline.c` (~1900 lines) builds neovim's status line/winbar/
//! ruler/tabline text - almost entirely dependent on the redraw/render
//! pipeline (`drawscreen`/`highlight`/`grid`/`ui`/`window`/`sign`/
//! `fold`/`option`) and the big statusline-format parser, none of which
//! is translated.
//!
//! Translated: the click-definition-table lifecycle trio -
//! [`stl_clear_click_defs`], [`stl_alloc_click_defs`],
//! [`stl_fill_click_defs`] - all pure struct/array manipulation over
//! the already-translated [`StlClickDefinition`]/[`StlClickRecord`]
//! shapes (`statusline_defs.rs`).
//!
//! `StlClickDefinition.func` is stored here as an owned
//! `Option<Vec<u8>>` (see `statusline_defs.rs`) rather than the
//! original's raw, possibly-shared `char *` pointer - so none of these
//! 3 functions need the original's manual `xfree`/pointer-identity-
//! deduplication dance; Rust's own ownership model (a plain
//! assignment or a `Vec` reset already dropping the old owned data)
//! does the same job automatically. See each function's own doc
//! comment for exactly which C-specific bookkeeping this replaces.
//!
//! Deferred: everything else - `win_redr_stl_expr`/`win_redr_status`/
//! `win_redr_winbar`/`redraw_ruler`/`draw_tabline`/`build_statuscol_str`/
//! `build_stl_str_hl`/`stl_truncate`/`stl_expand`, all needing the
//! redraw/render pipeline and the statusline-format parser.

use crate::statusline_defs::{StlClickDefinition, StlClickRecord, StlClickType};

/// Clears a click-definitions table, resetting every entry back to its
/// default ("disabled", no function) state - but NOT changing the
/// table's own length (`stl_clear_click_defs`).
///
/// The original manually frees each entry's own `func` pointer,
/// skipping a re-free when consecutive entries share the exact same
/// pointer (a deliberate pointer-sharing optimization elsewhere in this
/// file, avoiding one allocation per repeated run of identical
/// clicks). Since `StlClickDefinition.func` is an owned
/// `Option<Vec<u8>>` here, each entry already owns its own independent
/// data - overwriting an entry with its own `Default` value already
/// correctly drops whatever it used to hold, with no pointer-sharing
/// deduplication to replicate.
pub fn stl_clear_click_defs(click_defs: &mut [StlClickDefinition]) {
    for entry in click_defs.iter_mut() {
        *entry = StlClickDefinition::default();
    }
}

/// Resizes a click-definitions table to at least `width` entries if it
/// isn't already that large, discarding any existing content
/// (`stl_alloc_click_defs`). A no-op if `click_defs` already has at
/// least `width` entries.
///
/// The original takes/returns a raw pointer plus a separate `size_t
/// *size` out-parameter, `xfree`ing the old array and `xcalloc`ing a
/// fresh one when it needs to grow. This operates on an owned `Vec` in
/// place instead, matching this crate's established C-out-parameter-
/// to-owned-collection convention; the `Vec` reassignment already
/// drops every old entry's own data, same as the original's `xfree`.
pub fn stl_alloc_click_defs(click_defs: &mut Vec<StlClickDefinition>, width: i32) {
    if click_defs.len() < width as usize {
        *click_defs = vec![StlClickDefinition::default(); width.max(0) as usize];
    }
}

/// Fills a click-definitions table from a sequence of click "region
/// start" records, applying each region's own click definition to
/// every screen column from where it starts up to (but not including)
/// the next region's start (`stl_fill_click_defs`).
///
/// `buf` is the full status-line/tabline text that `click_recs`' own
/// `start` byte offsets are measured against. A no-op if `click_defs`
/// is empty (the original's own `click_defs == NULL` check).
///
/// The original's `else { xfree(cur_click_def.func); }` branches (freeing
/// a click definition that ends up NOT copied into any column before
/// being replaced) have no counterpart here: `cur_click_def = ...`
/// reassignment already drops whatever `cur_click_def` previously
/// owned, exactly like those branches did manually.
pub fn stl_fill_click_defs(
    click_defs: &mut [StlClickDefinition],
    click_recs: &[StlClickRecord],
    buf: &[u8],
    width: i32,
    tabline: bool,
) {
    if click_defs.is_empty() {
        return;
    }

    let mut col: usize = 0;
    let mut len: i32 = 0;
    let mut buf_pos: usize = 0;
    let mut cur_click_def = StlClickDefinition::default();

    for rec in click_recs {
        // SAFETY: `rec.start` is a valid byte offset within `buf`
        // (matching `StlClickRecord`'s own doc comment), so
        // `buf[buf_pos..]` and the byte count below stay in bounds.
        len += unsafe {
            crate::charset::vim_strnsize(&buf[buf_pos..], (rec.start - buf_pos) as i32)
        };
        debug_assert!(len <= width);
        while (col as i32) < len {
            click_defs[col] = cur_click_def.clone();
            col += 1;
        }
        buf_pos = rec.start;
        cur_click_def = rec.def.clone();
        if !tabline
            && !matches!(cur_click_def.r#type, StlClickType::Disabled | StlClickType::FuncRun)
        {
            // window bar and status line only support click functions
            cur_click_def.r#type = StlClickType::Disabled;
        }
    }
    while (col as i32) < width {
        click_defs[col] = cur_click_def.clone();
        col += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stl_clear_click_defs_resets_but_keeps_length() {
        let mut defs = vec![
            StlClickDefinition { r#type: StlClickType::FuncRun, tabnr: 0, func: Some(b"F1".to_vec()) },
            StlClickDefinition { r#type: StlClickType::TabSwitch, tabnr: 2, func: None },
        ];
        stl_clear_click_defs(&mut defs);
        assert_eq!(defs.len(), 2);
        assert!(defs.iter().all(|d| d.r#type == StlClickType::Disabled && d.func.is_none()));
    }

    #[test]
    fn stl_clear_click_defs_empty_is_a_no_op() {
        let mut defs: Vec<StlClickDefinition> = Vec::new();
        stl_clear_click_defs(&mut defs);
        assert!(defs.is_empty());
    }

    #[test]
    fn stl_alloc_click_defs_grows_when_too_small() {
        let mut defs: Vec<StlClickDefinition> = Vec::new();
        stl_alloc_click_defs(&mut defs, 5);
        assert_eq!(defs.len(), 5);
        assert!(defs.iter().all(|d| d.r#type == StlClickType::Disabled));
    }

    #[test]
    fn stl_alloc_click_defs_no_op_when_already_big_enough() {
        let mut defs = vec![
            StlClickDefinition { r#type: StlClickType::FuncRun, tabnr: 0, func: Some(b"keep-me".to_vec()) };
            5
        ];
        stl_alloc_click_defs(&mut defs, 3);
        // Still 5 entries, untouched (3 <= 5, no resize needed).
        assert_eq!(defs.len(), 5);
        assert_eq!(defs[0].func.as_deref(), Some(b"keep-me".as_slice()));
    }

    #[test]
    fn stl_fill_click_defs_empty_click_defs_is_a_no_op() {
        let mut defs: Vec<StlClickDefinition> = Vec::new();
        stl_fill_click_defs(&mut defs, &[], b"abcdef", 6, false);
        assert!(defs.is_empty());
    }

    #[test]
    fn stl_fill_click_defs_fills_regions_between_records() {
        // "abcdef": columns 0-1 before any click region (disabled),
        // 2-3 belong to the first region's def, 4-5 to the second's.
        let mut defs = vec![StlClickDefinition::default(); 6];
        let recs = [
            StlClickRecord {
                def: StlClickDefinition {
                    r#type: StlClickType::FuncRun,
                    tabnr: 0,
                    func: Some(b"F1".to_vec()),
                },
                start: 2,
            },
            StlClickRecord {
                def: StlClickDefinition {
                    r#type: StlClickType::FuncRun,
                    tabnr: 0,
                    func: Some(b"F2".to_vec()),
                },
                start: 4,
            },
        ];
        stl_fill_click_defs(&mut defs, &recs, b"abcdef", 6, false);

        assert_eq!(defs[0].r#type, StlClickType::Disabled);
        assert_eq!(defs[1].r#type, StlClickType::Disabled);
        assert_eq!(defs[2].func.as_deref(), Some(b"F1".as_slice()));
        assert_eq!(defs[3].func.as_deref(), Some(b"F1".as_slice()));
        assert_eq!(defs[4].func.as_deref(), Some(b"F2".as_slice()));
        assert_eq!(defs[5].func.as_deref(), Some(b"F2".as_slice()));
    }

    #[test]
    fn stl_fill_click_defs_disables_tab_switch_outside_tabline() {
        // TabSwitch clicks only make sense on the tabline; elsewhere
        // they're forced to Disabled.
        let mut defs = vec![StlClickDefinition::default(); 3];
        let recs = [StlClickRecord {
            def: StlClickDefinition { r#type: StlClickType::TabSwitch, tabnr: 1, func: None },
            start: 0,
        }];
        stl_fill_click_defs(&mut defs, &recs, b"abc", 3, false);
        assert!(defs.iter().all(|d| d.r#type == StlClickType::Disabled));
    }

    #[test]
    fn stl_fill_click_defs_keeps_tab_switch_on_tabline() {
        let mut defs = vec![StlClickDefinition::default(); 3];
        let recs = [StlClickRecord {
            def: StlClickDefinition { r#type: StlClickType::TabSwitch, tabnr: 1, func: None },
            start: 0,
        }];
        stl_fill_click_defs(&mut defs, &recs, b"abc", 3, true);
        assert!(defs.iter().all(|d| d.r#type == StlClickType::TabSwitch));
    }
}
