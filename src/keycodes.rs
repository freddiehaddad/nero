//! Translated from `src/nvim/keycodes.c` (tractable core only).
//!
//! `keycodes.c` (~830 lines) is neovim's special-key name/termcap-code
//! lookup and conversion file - many of its functions need `mouse.c`'s
//! mouse-event table (not translated) or the full key-name table
//! (`get_special_key_name`/`find_special_key_in_table`, a large
//! generated table not transcribed here).
//!
//! Translated: [`name_to_mod_mask`] and [`handle_x_keys`] - both pure,
//! self-contained lookups needing only [`crate::keycodes_defs`]'s own
//! (partial) constant/table translations.
//!
//! Deferred: everything else - `simplify_key` (needs the much larger
//! `modifier_keys_table`, ~90 entries, not transcribed), `get_special_key`/
//! `get_special_key_name`/`find_special_key_in_table`/`find_special_key`/
//! `replace_termcodes`/`trans_special`/`special_to_buf` (need the full
//! generated key-name table), `get_mouse_button` (needs `mouse.c`).

use crate::keycodes_defs::{
    MOD_MASK_TABLE, K_DOWN, K_END, K_F1, K_F2, K_F3, K_F4, K_HOME, K_LEFT, K_RIGHT, K_S_F1,
    K_S_F2, K_S_F3, K_S_F4, K_S_XF1, K_S_XF2, K_S_XF3, K_S_XF4, K_UP, K_XDOWN, K_XEND, K_XF1,
    K_XF2, K_XF3, K_XF4, K_XHOME, K_XLEFT, K_XRIGHT, K_XUP, K_ZEND, K_ZHOME,
};

/// Returns the [`crate::keycodes_defs::MOD_MASK_TABLE`] modifier-mask
/// bit corresponding to modifier letter `c` (e.g. `'S'` for shift, `'C'`
/// for ctrl), or `0` if `c` isn't a recognized modifier letter
/// (`name_to_mod_mask`). `c` is matched case-insensitively (uppercased
/// via [`crate::macros_defs::toupper_asc`] first, matching the
/// original).
#[must_use]
pub fn name_to_mod_mask(c: i32) -> i32 {
    let c = crate::macros_defs::toupper_asc(c);
    for entry in MOD_MASK_TABLE {
        if c == i32::from(entry.name) {
            return i32::from(entry.mod_flag);
        }
    }
    0
}

/// Changes an `<xKey>`-style key code (e.g. `<xUp>`, `<xF1>`) to its
/// plain equivalent (e.g. `<Up>`, `<F1>`) - `key` is returned unchanged
/// if it isn't one of the recognized `<x...>`/`<z...>` codes
/// (`handle_x_keys`).
#[must_use]
pub fn handle_x_keys(key: i32) -> i32 {
    match key {
        K_XUP => K_UP,
        K_XDOWN => K_DOWN,
        K_XLEFT => K_LEFT,
        K_XRIGHT => K_RIGHT,
        K_XHOME | K_ZHOME => K_HOME,
        K_XEND | K_ZEND => K_END,
        K_XF1 => K_F1,
        K_XF2 => K_F2,
        K_XF3 => K_F3,
        K_XF4 => K_F4,
        K_S_XF1 => K_S_F1,
        K_S_XF2 => K_S_F2,
        K_S_XF3 => K_S_F3,
        K_S_XF4 => K_S_F4,
        _ => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_to_mod_mask_recognizes_every_letter() {
        assert_eq!(name_to_mod_mask('S' as i32), i32::from(crate::keycodes_defs::MOD_MASK_SHIFT));
        assert_eq!(name_to_mod_mask('C' as i32), i32::from(crate::keycodes_defs::MOD_MASK_CTRL));
        assert_eq!(name_to_mod_mask('M' as i32), i32::from(crate::keycodes_defs::MOD_MASK_ALT));
        assert_eq!(name_to_mod_mask('T' as i32), i32::from(crate::keycodes_defs::MOD_MASK_META));
        assert_eq!(name_to_mod_mask('D' as i32), i32::from(crate::keycodes_defs::MOD_MASK_CMD));
        assert_eq!(name_to_mod_mask('2' as i32), i32::from(crate::keycodes_defs::MOD_MASK_2CLICK));
        assert_eq!(name_to_mod_mask('3' as i32), i32::from(crate::keycodes_defs::MOD_MASK_3CLICK));
        assert_eq!(name_to_mod_mask('4' as i32), i32::from(crate::keycodes_defs::MOD_MASK_4CLICK));
        // 'A' is a second, later entry for MOD_MASK_ALT.
        assert_eq!(name_to_mod_mask('A' as i32), i32::from(crate::keycodes_defs::MOD_MASK_ALT));
    }

    #[test]
    fn name_to_mod_mask_is_case_insensitive() {
        assert_eq!(name_to_mod_mask('s' as i32), name_to_mod_mask('S' as i32));
        assert_eq!(name_to_mod_mask('c' as i32), name_to_mod_mask('C' as i32));
    }

    #[test]
    fn name_to_mod_mask_unknown_letter_is_zero() {
        assert_eq!(name_to_mod_mask('Q' as i32), 0);
        assert_eq!(name_to_mod_mask('1' as i32), 0);
    }

    #[test]
    fn handle_x_keys_maps_arrow_keys() {
        assert_eq!(handle_x_keys(K_XUP), K_UP);
        assert_eq!(handle_x_keys(K_XDOWN), K_DOWN);
        assert_eq!(handle_x_keys(K_XLEFT), K_LEFT);
        assert_eq!(handle_x_keys(K_XRIGHT), K_RIGHT);
    }

    #[test]
    fn handle_x_keys_maps_both_home_variants() {
        assert_eq!(handle_x_keys(K_XHOME), K_HOME);
        assert_eq!(handle_x_keys(K_ZHOME), K_HOME);
    }

    #[test]
    fn handle_x_keys_maps_both_end_variants() {
        assert_eq!(handle_x_keys(K_XEND), K_END);
        assert_eq!(handle_x_keys(K_ZEND), K_END);
    }

    #[test]
    fn handle_x_keys_maps_function_keys_and_shifted_variants() {
        assert_eq!(handle_x_keys(K_XF1), K_F1);
        assert_eq!(handle_x_keys(K_XF2), K_F2);
        assert_eq!(handle_x_keys(K_XF3), K_F3);
        assert_eq!(handle_x_keys(K_XF4), K_F4);
        assert_eq!(handle_x_keys(K_S_XF1), K_S_F1);
        assert_eq!(handle_x_keys(K_S_XF2), K_S_F2);
        assert_eq!(handle_x_keys(K_S_XF3), K_S_F3);
        assert_eq!(handle_x_keys(K_S_XF4), K_S_F4);
    }

    #[test]
    fn handle_x_keys_leaves_unrelated_keys_unchanged() {
        assert_eq!(handle_x_keys(K_UP), K_UP);
        assert_eq!(handle_x_keys(42), 42);
    }
}
