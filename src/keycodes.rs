//! Translated from `src/nvim/keycodes.c` (tractable core only).
//!
//! `keycodes.c` (~830 lines) is neovim's special-key name/termcap-code
//! lookup and conversion file - many of its functions need `mouse.c`'s
//! mouse-event table (not translated) or the full key-name table
//! (`get_special_key_name`/`find_special_key_in_table`, a large
//! generated table not transcribed here).
//!
//! Translated: [`name_to_mod_mask`], [`handle_x_keys`],
//! [`simplify_key`] - all pure, self-contained lookups needing only
//! [`crate::keycodes_defs`]'s own (partial) constant/table
//! translations - and [`vim_unescape_ks`], a self-contained in-place
//! unescape needing only [`crate::keycodes_defs::K_SPECIAL`]/
//! [`crate::keycodes_defs::KS_SPECIAL`]/[`crate::keycodes_defs::KE_FILLER`].
//!
//! Deferred: everything else - `get_special_key`/`get_special_key_name`/
//! `find_special_key_in_table`/`find_special_key`/`replace_termcodes`/
//! `trans_special`/`special_to_buf` (need the full generated key-name
//! table), `get_mouse_button` (needs `mouse.c`), `add_char2buf`/
//! `vim_strsave_escape_ks` (the escaping counterpart of
//! `vim_unescape_ks` - needs `utf_ptr2len`, verify separately).

use crate::ascii_defs::TAB;
use crate::keycodes_defs::{
    key2termcap0, key2termcap1, termcap2key, MODIFIER_KEYS_TABLE, MOD_MASK_CTRL, MOD_MASK_SHIFT,
    MOD_MASK_TABLE, K_DOWN, K_END, K_F1, K_F2, K_F3, K_F4, K_HOME, K_LEFT, K_RIGHT, K_S_F1,
    K_S_F2, K_S_F3, K_S_F4, K_S_TAB, K_S_XF1, K_S_XF2, K_S_XF3, K_S_XF4, K_UP, K_XDOWN, K_XEND,
    K_XF1, K_XF2, K_XF3, K_XF4, K_XHOME, K_XLEFT, K_XRIGHT, K_XUP, K_ZEND, K_ZHOME,
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

/// Simplifies `key` + `*modifiers` into a single combined key code when
/// there's a dedicated termcap code for that specific
/// key-plus-one-modifier combination, clearing the now-redundant
/// modifier bit from `*modifiers` (`simplify_key`). Returns `key`
/// unchanged (and leaves `*modifiers` untouched) if no such combination
/// applies.
///
/// TAB + Shift is a special case with its own dedicated check (matching
/// the original) since `TAB`'s own plain ASCII value isn't itself a
/// `termcap2key`-encoded "special key", unlike every other entry in
/// [`MODIFIER_KEYS_TABLE`].
pub fn simplify_key(key: i32, modifiers: &mut i32) -> i32 {
    if *modifiers & i32::from(MOD_MASK_SHIFT | MOD_MASK_CTRL) == 0 {
        return key;
    }

    // TAB is a special case.
    if key == i32::from(TAB) && *modifiers & i32::from(MOD_MASK_SHIFT) != 0 {
        *modifiers &= !i32::from(MOD_MASK_SHIFT);
        return K_S_TAB;
    }

    let key0 = key2termcap0(key);
    let key1 = key2termcap1(key);
    for &(mod_mask, with0, with1, without0, without1) in MODIFIER_KEYS_TABLE {
        if key0 == without0 && key1 == without1 && *modifiers & i32::from(mod_mask) != 0 {
            *modifiers &= !i32::from(mod_mask);
            return termcap2key(with0, with1);
        }
    }
    key
}

/// Remove escaping from `K_SPECIAL` characters - the reverse of
/// `vim_strsave_escape_ks` (not yet translated - needs `add_char2buf`/
/// `utf_ptr2len`). Works in place, returning the number of bytes in
/// the unescaped result (`vim_unescape_ks`).
///
/// Modeled as `p: &mut [u8]` (in place, like the original's own
/// `char *p` in/out buffer) rather than returning a fresh `Vec<u8>`:
/// every real caller (`mapping.c`, `lua/executor.c`, `register.c`)
/// already owns a mutable buffer it wants shrunk in place, matching
/// this crate's established convention for genuinely in-place C
/// buffer algorithms (e.g. `charset::rl_mirror_ascii`).
#[must_use]
pub fn vim_unescape_ks(p: &mut [u8]) -> usize {
    let mut s = 0usize;
    let mut d = 0usize;
    while s < p.len() && p[s] != 0 {
        if p[s] == crate::keycodes_defs::K_SPECIAL
            && p.get(s + 1).copied() == Some(crate::keycodes_defs::KS_SPECIAL)
            && p.get(s + 2).copied() == Some(crate::keycodes_defs::KE_FILLER)
        {
            p[d] = crate::keycodes_defs::K_SPECIAL;
            d += 1;
            s += 3;
        } else {
            p[d] = p[s];
            d += 1;
            s += 1;
        }
    }
    d
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

    #[test]
    fn simplify_key_returns_key_unchanged_without_shift_or_ctrl() {
        let mut modifiers = i32::from(crate::keycodes_defs::MOD_MASK_ALT);
        assert_eq!(simplify_key(K_UP, &mut modifiers), K_UP);
        assert_eq!(modifiers, i32::from(crate::keycodes_defs::MOD_MASK_ALT));
    }

    #[test]
    fn simplify_key_tab_plus_shift_is_a_special_case() {
        let mut modifiers = i32::from(MOD_MASK_SHIFT);
        assert_eq!(simplify_key(i32::from(TAB), &mut modifiers), crate::keycodes_defs::K_S_TAB);
        // The Shift bit is consumed - now folded into K_S_TAB itself.
        assert_eq!(modifiers, 0);
    }

    #[test]
    fn simplify_key_folds_ctrl_left_arrow() {
        let mut modifiers = i32::from(MOD_MASK_CTRL);
        assert_eq!(simplify_key(K_LEFT, &mut modifiers), crate::keycodes_defs::K_C_LEFT);
        assert_eq!(modifiers, 0);
    }

    #[test]
    fn simplify_key_folds_shift_up_arrow() {
        let mut modifiers = i32::from(MOD_MASK_SHIFT);
        assert_eq!(simplify_key(K_UP, &mut modifiers), crate::keycodes_defs::K_S_UP);
        assert_eq!(modifiers, 0);
    }

    #[test]
    fn simplify_key_preserves_other_modifier_bits_when_folding() {
        // ALT should survive being combined with a Ctrl-Left fold.
        let mut modifiers = i32::from(MOD_MASK_CTRL) | i32::from(crate::keycodes_defs::MOD_MASK_ALT);
        assert_eq!(simplify_key(K_LEFT, &mut modifiers), crate::keycodes_defs::K_C_LEFT);
        assert_eq!(modifiers, i32::from(crate::keycodes_defs::MOD_MASK_ALT));
    }

    #[test]
    fn simplify_key_no_matching_table_entry_leaves_everything_unchanged() {
        // K_F1 (a plain, un-simplifiable function key with no combined
        // Ctrl form in the table) with Ctrl set: no match, nothing
        // changes.
        let mut modifiers = i32::from(MOD_MASK_CTRL);
        assert_eq!(simplify_key(K_F1, &mut modifiers), K_F1);
        assert_eq!(modifiers, i32::from(MOD_MASK_CTRL));
    }

    // --- vim_unescape_ks ---

    fn ks() -> (u8, u8, u8) {
        (crate::keycodes_defs::K_SPECIAL, crate::keycodes_defs::KS_SPECIAL, crate::keycodes_defs::KE_FILLER)
    }

    #[test]
    fn vim_unescape_ks_unescapes_a_single_sequence() {
        let (k, ks, ke) = ks();
        let mut buf = [k, ks, ke, b'a', b'b'];
        let new_len = vim_unescape_ks(&mut buf);
        assert_eq!(new_len, 3);
        assert_eq!(&buf[..new_len], &[k, b'a', b'b']);
    }

    #[test]
    fn vim_unescape_ks_no_escape_sequences_is_unchanged() {
        let mut buf = *b"hi\0\0\0";
        let new_len = vim_unescape_ks(&mut buf);
        assert_eq!(new_len, 2);
        assert_eq!(&buf[..new_len], b"hi");
    }

    #[test]
    fn vim_unescape_ks_bare_escape_sequence_becomes_one_byte() {
        let (k, ks, ke) = ks();
        let mut buf = [k, ks, ke];
        let new_len = vim_unescape_ks(&mut buf);
        assert_eq!(new_len, 1);
        assert_eq!(buf[0], k);
    }

    #[test]
    fn vim_unescape_ks_k_special_not_followed_by_the_full_pattern_is_left_alone() {
        // K_SPECIAL followed by bytes that don't match KS_SPECIAL/
        // KE_FILLER exactly - not a real escape sequence, copied
        // through unchanged (matching the original's own exact-match
        // requirement on all 3 bytes).
        let (k, _, _) = ks();
        let mut buf = [k, b'x', b'y', 0];
        let new_len = vim_unescape_ks(&mut buf);
        assert_eq!(new_len, 3);
        assert_eq!(&buf[..new_len], &[k, b'x', b'y']);
    }

    #[test]
    fn vim_unescape_ks_multiple_sequences_in_one_buffer() {
        let (k, ks, ke) = ks();
        let mut buf = [k, ks, ke, b'-', k, ks, ke, 0];
        let new_len = vim_unescape_ks(&mut buf);
        assert_eq!(new_len, 3);
        assert_eq!(&buf[..new_len], &[k, b'-', k]);
    }

    #[test]
    fn vim_unescape_ks_empty_string_stays_empty() {
        let mut buf = [0u8];
        assert_eq!(vim_unescape_ks(&mut buf), 0);
    }
}
