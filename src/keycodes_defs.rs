//! Translated from `src/nvim/keycodes.h` (partial: only the constants
//! needed by `keycodes.rs`'s/`input.rs`'s own translated functions so
//! far).
//!
//! Neovim represents "special" keys (function keys, arrow keys, etc,
//! anything that doesn't fit in a single byte) as a 3-byte in-band
//! escape sequence: [`K_SPECIAL`] followed by two "termcap" bytes
//! (`KS_*`/`KE_*`). [`termcap2key`] packs a pair of such bytes into a
//! single negative `i32` "key code" (and [`key2termcap0`]/
//! [`key2termcap1`] unpack them again) - the original's own encoding is
//! kept exactly (not reinterpreted as some more "idiomatic" Rust enum),
//! since downstream code (`simplify_key`, not yet translated,
//! `getchar.c`'s input pipeline, also not yet translated) manipulates
//! these codes arithmetically, not just symbolically. [`is_special`]/
//! [`k_second`]/[`k_third`] (`IS_SPECIAL`/`K_SECOND`/`K_THIRD`) build on
//! [`key2termcap0`]/[`key2termcap1`] to also handle the 2 real
//! characters (`K_SPECIAL` itself, and a literal NUL byte) that need
//! the SAME 3-byte escaping treatment as a genuine special key code,
//! for `input.rs`'s `add_byte_buff`.

/// Marks the start of a "special" (multi-byte-encoded) key sequence in
/// the low-level input byte stream (`K_SPECIAL`).
pub const K_SPECIAL: u8 = 0x80;

/// Packs two termcap bytes `a`, `b` into the single `i32` "key code"
/// value used throughout this crate's (future) keyboard-input handling
/// (`TERMCAP2KEY(a, b)`).
#[must_use]
pub const fn termcap2key(a: u8, b: u8) -> i32 {
    -((a as i32) + ((b as i32) << 8))
}

/// Recovers the first termcap byte from a key code produced by
/// [`termcap2key`] (`KEY2TERMCAP0(x)`).
#[must_use]
pub const fn key2termcap0(x: i32) -> u8 {
    ((-x) & 0xff) as u8
}

/// Recovers the second termcap byte from a key code produced by
/// [`termcap2key`] (`KEY2TERMCAP1(x)`).
#[must_use]
pub const fn key2termcap1(x: i32) -> u8 {
    (((-x) as u32 >> 8) & 0xff) as u8
}

/// Whether `c` is a "special" key code, i.e. one produced by
/// [`termcap2key`] rather than a plain byte/character value
/// (`IS_SPECIAL`).
#[must_use]
pub const fn is_special(c: i32) -> bool {
    c < 0
}

/// First termcap byte meaning "this is `K_SPECIAL` itself, escaped so
/// it isn't mistaken for the start of a special-key sequence"
/// (`KS_SPECIAL`).
pub const KS_SPECIAL: u8 = 254;

/// First termcap byte meaning "this is a literal NUL byte, escaped the
/// same way since a real NUL can't be stored in a NUL-terminated C
/// string" (`KS_ZERO`).
pub const KS_ZERO: u8 = 255;

/// Filler second byte used after [`KS_SPECIAL`]/[`KS_ZERO`] (which
/// don't need a second real termcap byte) - `'X'` (`KE_FILLER`).
pub const KE_FILLER: u8 = b'X';

/// Second byte of the 3-byte `K_SPECIAL` escape sequence for character
/// or special key code `c` (`K_SECOND(c)`). See [`k_third`] for the
/// third byte.
#[must_use]
pub const fn k_second(c: i32) -> u8 {
    if c == K_SPECIAL as i32 {
        KS_SPECIAL
    } else if c == 0 {
        KS_ZERO
    } else {
        key2termcap0(c)
    }
}

/// Third byte of the 3-byte `K_SPECIAL` escape sequence for character
/// or special key code `c` (`K_THIRD(c)`). See [`k_second`] for the
/// second byte.
#[must_use]
pub const fn k_third(c: i32) -> u8 {
    if c == K_SPECIAL as i32 || c == 0 {
        KE_FILLER
    } else {
        key2termcap1(c)
    }
}

/// First termcap byte meaning "this is one of the `KE_*` pseudo-keys
/// below, see the second byte" (`KS_EXTRA`).
pub const KS_EXTRA: u8 = 253;

// `KE_*`: second termcap byte values following `KS_EXTRA`, a subset of
// the original's much larger enum - only the entries needed by
// `keycodes.rs`'s own translated functions so far. Add more as more
// functions get translated, rather than transcribing the whole enum
// upfront.
pub const KE_S_F1: u8 = 6;
pub const KE_S_F2: u8 = 7;
pub const KE_S_F3: u8 = 8;
pub const KE_S_F4: u8 = 9;
pub const KE_XF1: u8 = 57;
pub const KE_XF2: u8 = 58;
pub const KE_XF3: u8 = 59;
pub const KE_XF4: u8 = 60;
pub const KE_XEND: u8 = 61;
pub const KE_ZEND: u8 = 62;
pub const KE_XHOME: u8 = 63;
pub const KE_ZHOME: u8 = 64;
pub const KE_XUP: u8 = 65;
pub const KE_XDOWN: u8 = 66;
pub const KE_XLEFT: u8 = 67;
pub const KE_XRIGHT: u8 = 68;
pub const KE_S_XF1: u8 = 71;
pub const KE_S_XF2: u8 = 72;
pub const KE_S_XF3: u8 = 73;
pub const KE_S_XF4: u8 = 74;
pub const KE_S_UP: u8 = 4;
pub const KE_S_DOWN: u8 = 5;
pub const KE_S_F5: u8 = 10;
pub const KE_S_F6: u8 = 11;
pub const KE_S_F7: u8 = 12;
pub const KE_S_F8: u8 = 13;
pub const KE_S_F9: u8 = 14;
pub const KE_S_F10: u8 = 15;
pub const KE_S_F11: u8 = 16;
pub const KE_S_F12: u8 = 17;
pub const KE_S_F13: u8 = 18;
pub const KE_S_F14: u8 = 19;
pub const KE_S_F15: u8 = 20;
pub const KE_S_F16: u8 = 21;
pub const KE_S_F17: u8 = 22;
pub const KE_S_F18: u8 = 23;
pub const KE_S_F19: u8 = 24;
pub const KE_S_F20: u8 = 25;
pub const KE_S_F21: u8 = 26;
pub const KE_S_F22: u8 = 27;
pub const KE_S_F23: u8 = 28;
pub const KE_S_F24: u8 = 29;
pub const KE_S_F25: u8 = 30;
pub const KE_S_F26: u8 = 31;
pub const KE_S_F27: u8 = 32;
pub const KE_S_F28: u8 = 33;
pub const KE_S_F29: u8 = 34;
pub const KE_S_F30: u8 = 35;
pub const KE_S_F31: u8 = 36;
pub const KE_S_F32: u8 = 37;
pub const KE_S_F33: u8 = 38;
pub const KE_S_F34: u8 = 39;
pub const KE_S_F35: u8 = 40;
pub const KE_S_F36: u8 = 41;
pub const KE_S_F37: u8 = 42;
pub const KE_TAB: u8 = 54;
pub const KE_C_LEFT: u8 = 85;
pub const KE_C_RIGHT: u8 = 86;
pub const KE_C_HOME: u8 = 87;
pub const KE_C_END: u8 = 88;
pub const KE_COMMAND: u8 = 104;

/// arrow/function/home/end key codes (`K_UP`/`K_DOWN`/etc.) - a subset
/// of the original's much larger `K_*` constant list, matching the
/// naming exactly.
pub const K_UP: i32 = termcap2key(b'k', b'u');
pub const K_DOWN: i32 = termcap2key(b'k', b'd');
pub const K_LEFT: i32 = termcap2key(b'k', b'l');
pub const K_RIGHT: i32 = termcap2key(b'k', b'r');
pub const K_HOME: i32 = termcap2key(b'k', b'h');
pub const K_END: i32 = termcap2key(b'@', b'7');
pub const K_F1: i32 = termcap2key(b'k', b'1');
pub const K_F2: i32 = termcap2key(b'k', b'2');
pub const K_F3: i32 = termcap2key(b'k', b'3');
pub const K_F4: i32 = termcap2key(b'k', b'4');
pub const K_S_F1: i32 = termcap2key(KS_EXTRA, KE_S_F1);
pub const K_S_F2: i32 = termcap2key(KS_EXTRA, KE_S_F2);
pub const K_S_F3: i32 = termcap2key(KS_EXTRA, KE_S_F3);
pub const K_S_F4: i32 = termcap2key(KS_EXTRA, KE_S_F4);
pub const K_XUP: i32 = termcap2key(KS_EXTRA, KE_XUP);
pub const K_XDOWN: i32 = termcap2key(KS_EXTRA, KE_XDOWN);
pub const K_XLEFT: i32 = termcap2key(KS_EXTRA, KE_XLEFT);
pub const K_XRIGHT: i32 = termcap2key(KS_EXTRA, KE_XRIGHT);
pub const K_XHOME: i32 = termcap2key(KS_EXTRA, KE_XHOME);
pub const K_ZHOME: i32 = termcap2key(KS_EXTRA, KE_ZHOME);
pub const K_XEND: i32 = termcap2key(KS_EXTRA, KE_XEND);
pub const K_ZEND: i32 = termcap2key(KS_EXTRA, KE_ZEND);
pub const K_XF1: i32 = termcap2key(KS_EXTRA, KE_XF1);
pub const K_XF2: i32 = termcap2key(KS_EXTRA, KE_XF2);
pub const K_XF3: i32 = termcap2key(KS_EXTRA, KE_XF3);
pub const K_XF4: i32 = termcap2key(KS_EXTRA, KE_XF4);
pub const K_S_XF1: i32 = termcap2key(KS_EXTRA, KE_S_XF1);
pub const K_S_XF2: i32 = termcap2key(KS_EXTRA, KE_S_XF2);
pub const K_S_XF3: i32 = termcap2key(KS_EXTRA, KE_S_XF3);
pub const K_S_XF4: i32 = termcap2key(KS_EXTRA, KE_S_XF4);
pub const K_S_UP: i32 = termcap2key(KS_EXTRA, KE_S_UP);
pub const K_S_DOWN: i32 = termcap2key(KS_EXTRA, KE_S_DOWN);
pub const K_C_LEFT: i32 = termcap2key(KS_EXTRA, KE_C_LEFT);
pub const K_C_RIGHT: i32 = termcap2key(KS_EXTRA, KE_C_RIGHT);
pub const K_C_HOME: i32 = termcap2key(KS_EXTRA, KE_C_HOME);
pub const K_C_END: i32 = termcap2key(KS_EXTRA, KE_C_END);
/// `<Cmd>` special key (`K_COMMAND`), used by `ops.rs`'s `is_ex_cmdchar`.
pub const K_COMMAND: i32 = termcap2key(KS_EXTRA, KE_COMMAND);

/// Bit-mask/bit-value pairs for key modifiers (`MOD_MASK_*`).
pub const MOD_MASK_SHIFT: u16 = 0x02;
pub const MOD_MASK_CTRL: u16 = 0x04;
/// aka META (`MOD_MASK_ALT`).
pub const MOD_MASK_ALT: u16 = 0x08;
/// META when it's different from ALT (`MOD_MASK_META`).
pub const MOD_MASK_META: u16 = 0x10;
pub const MOD_MASK_2CLICK: u16 = 0x20;
pub const MOD_MASK_3CLICK: u16 = 0x40;
pub const MOD_MASK_4CLICK: u16 = 0x60;
/// "super" key (macOS: command-key) (`MOD_MASK_CMD`).
pub const MOD_MASK_CMD: u16 = 0x80;
pub const MOD_MASK_MULTI_CLICK: u16 = MOD_MASK_2CLICK | MOD_MASK_3CLICK | MOD_MASK_4CLICK;

/// One entry of [`MOD_MASK_TABLE`] (`struct modmasktable`).
pub struct ModMaskEntry {
    /// Bit-mask for particular key modifier (`mod_mask`). Only used as
    /// a broader "category" grouping (e.g. every multi-click entry
    /// shares [`MOD_MASK_MULTI_CLICK`] here even though each has its
    /// own distinct `mod_flag`) - kept for fidelity even though
    /// [`crate::keycodes::name_to_mod_mask`] (the only translated
    /// reader so far) doesn't need it.
    pub mod_mask: u16,
    /// Bit(s) for particular key modifier (`mod_flag`).
    pub mod_flag: u16,
    /// Single letter name of modifier (`name`).
    pub name: u8,
}

/// `mod_mask_table` - modifier-letter lookup table used by
/// `<C-x>`/`<M-x>`/etc. key-notation parsing.
///
/// The original terminates this array with a `{ 0, 0, NUL }` sentinel
/// entry so a C loop knows where to stop; not needed here since a Rust
/// slice already carries its own length.
pub const MOD_MASK_TABLE: &[ModMaskEntry] = &[
    ModMaskEntry { mod_mask: MOD_MASK_ALT, mod_flag: MOD_MASK_ALT, name: b'M' },
    ModMaskEntry { mod_mask: MOD_MASK_META, mod_flag: MOD_MASK_META, name: b'T' },
    ModMaskEntry { mod_mask: MOD_MASK_CTRL, mod_flag: MOD_MASK_CTRL, name: b'C' },
    ModMaskEntry { mod_mask: MOD_MASK_SHIFT, mod_flag: MOD_MASK_SHIFT, name: b'S' },
    ModMaskEntry { mod_mask: MOD_MASK_MULTI_CLICK, mod_flag: MOD_MASK_2CLICK, name: b'2' },
    ModMaskEntry { mod_mask: MOD_MASK_MULTI_CLICK, mod_flag: MOD_MASK_3CLICK, name: b'3' },
    ModMaskEntry { mod_mask: MOD_MASK_MULTI_CLICK, mod_flag: MOD_MASK_4CLICK, name: b'4' },
    ModMaskEntry { mod_mask: MOD_MASK_CMD, mod_flag: MOD_MASK_CMD, name: b'D' },
    // 'A' must be the last one.
    ModMaskEntry { mod_mask: MOD_MASK_ALT, mod_flag: MOD_MASK_ALT, name: b'A' },
];

/// Shifted TAB (`K_S_TAB`).
pub const K_S_TAB: i32 = termcap2key(b'k', b'B');

/// One entry of [`MODIFIER_KEYS_TABLE`]: an unmodified key that has its
/// own dedicated termcap code when combined with a specific single
/// modifier (the original's flat 5-`uint8_t` grouping,
/// `MOD_KEYS_ENTRY_SIZE`): `(mod_mask, with_modifier0, with_modifier1,
/// without_modifier0, without_modifier1)`. A plain tuple is used rather
/// than a named struct, matching the original's own flat/positional
/// array shape closely given its sheer size (75 entries).
pub type ModifierKeyEntry = (u16, u8, u8, u8, u8);

/// `modifier_keys_table` - shifted/ctrl'd terminal codes and their
/// unshifted equivalent, used by `simplify_key` to fold a separate
/// modifier bit into a single combined key code when the terminal (or
/// this crate's own key-encoding) has a dedicated code for that
/// combination. Mouse codes are handled separately (not listed here,
/// matching the original's own comment).
pub const MODIFIER_KEYS_TABLE: &[ModifierKeyEntry] = &[
    (MOD_MASK_SHIFT, b'&', b'9', b'@', b'1'), // begin
    (MOD_MASK_SHIFT, b'&', b'0', b'@', b'2'), // cancel
    (MOD_MASK_SHIFT, b'*', b'1', b'@', b'4'), // command
    (MOD_MASK_SHIFT, b'*', b'2', b'@', b'5'), // copy
    (MOD_MASK_SHIFT, b'*', b'3', b'@', b'6'), // create
    (MOD_MASK_SHIFT, b'*', b'4', b'k', b'D'), // delete char
    (MOD_MASK_SHIFT, b'*', b'5', b'k', b'L'), // delete line
    (MOD_MASK_SHIFT, b'*', b'7', b'@', b'7'), // end
    (MOD_MASK_CTRL, KS_EXTRA, KE_C_END, b'@', b'7'), // end
    (MOD_MASK_SHIFT, b'*', b'9', b'@', b'9'), // exit
    (MOD_MASK_SHIFT, b'*', b'0', b'@', b'0'), // find
    (MOD_MASK_SHIFT, b'#', b'1', b'%', b'1'), // help
    (MOD_MASK_SHIFT, b'#', b'2', b'k', b'h'), // home
    (MOD_MASK_CTRL, KS_EXTRA, KE_C_HOME, b'k', b'h'), // home
    (MOD_MASK_SHIFT, b'#', b'3', b'k', b'I'), // insert
    (MOD_MASK_SHIFT, b'#', b'4', b'k', b'l'), // left arrow
    (MOD_MASK_CTRL, KS_EXTRA, KE_C_LEFT, b'k', b'l'), // left arrow
    (MOD_MASK_SHIFT, b'%', b'a', b'%', b'3'), // message
    (MOD_MASK_SHIFT, b'%', b'b', b'%', b'4'), // move
    (MOD_MASK_SHIFT, b'%', b'c', b'%', b'5'), // next
    (MOD_MASK_SHIFT, b'%', b'd', b'%', b'7'), // options
    (MOD_MASK_SHIFT, b'%', b'e', b'%', b'8'), // previous
    (MOD_MASK_SHIFT, b'%', b'f', b'%', b'9'), // print
    (MOD_MASK_SHIFT, b'%', b'g', b'%', b'0'), // redo
    (MOD_MASK_SHIFT, b'%', b'h', b'&', b'3'), // replace
    (MOD_MASK_SHIFT, b'%', b'i', b'k', b'r'), // right arr.
    (MOD_MASK_CTRL, KS_EXTRA, KE_C_RIGHT, b'k', b'r'), // right arr.
    (MOD_MASK_SHIFT, b'%', b'j', b'&', b'5'), // resume
    (MOD_MASK_SHIFT, b'!', b'1', b'&', b'6'), // save
    (MOD_MASK_SHIFT, b'!', b'2', b'&', b'7'), // suspend
    (MOD_MASK_SHIFT, b'!', b'3', b'&', b'8'), // undo
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_UP, b'k', b'u'), // up arrow
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_DOWN, b'k', b'd'), // down arrow
    // vt100 F1-F4
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_XF1, KS_EXTRA, KE_XF1),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_XF2, KS_EXTRA, KE_XF2),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_XF3, KS_EXTRA, KE_XF3),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_XF4, KS_EXTRA, KE_XF4),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F1, b'k', b'1'), // F1
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F2, b'k', b'2'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F3, b'k', b'3'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F4, b'k', b'4'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F5, b'k', b'5'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F6, b'k', b'6'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F7, b'k', b'7'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F8, b'k', b'8'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F9, b'k', b'9'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F10, b'k', b';'), // F10
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F11, b'F', b'1'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F12, b'F', b'2'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F13, b'F', b'3'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F14, b'F', b'4'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F15, b'F', b'5'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F16, b'F', b'6'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F17, b'F', b'7'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F18, b'F', b'8'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F19, b'F', b'9'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F20, b'F', b'A'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F21, b'F', b'B'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F22, b'F', b'C'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F23, b'F', b'D'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F24, b'F', b'E'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F25, b'F', b'F'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F26, b'F', b'G'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F27, b'F', b'H'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F28, b'F', b'I'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F29, b'F', b'J'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F30, b'F', b'K'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F31, b'F', b'L'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F32, b'F', b'M'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F33, b'F', b'N'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F34, b'F', b'O'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F35, b'F', b'P'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F36, b'F', b'Q'),
    (MOD_MASK_SHIFT, KS_EXTRA, KE_S_F37, b'F', b'R'),
    // TAB pseudo code
    (MOD_MASK_SHIFT, b'k', b'B', KS_EXTRA, KE_TAB),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn termcap2key_roundtrips_through_key2termcap() {
        let key = termcap2key(b'k', b'u');
        assert_eq!(key2termcap0(key), b'k');
        assert_eq!(key2termcap1(key), b'u');
    }

    #[test]
    fn termcap2key_is_always_negative_for_nonzero_input() {
        assert!(termcap2key(b'k', b'u') < 0);
        assert!(termcap2key(KS_EXTRA, KE_XUP) < 0);
    }

    #[test]
    fn is_special_true_for_negative_key_codes() {
        assert!(is_special(K_UP));
        assert!(is_special(-1));
        assert!(!is_special(0));
        assert!(!is_special(b'a' as i32));
    }

    #[test]
    fn k_second_k_third_of_k_special_itself() {
        // c == K_SPECIAL is escaped via KS_SPECIAL/KE_FILLER, not the
        // generic key2termcap0/1 bit-math path.
        let c = i32::from(K_SPECIAL);
        assert_eq!(k_second(c), KS_SPECIAL);
        assert_eq!(k_third(c), KE_FILLER);
    }

    #[test]
    fn k_second_k_third_of_a_literal_nul() {
        assert_eq!(k_second(0), KS_ZERO);
        assert_eq!(k_third(0), KE_FILLER);
    }

    #[test]
    fn k_second_k_third_of_a_real_special_key_matches_key2termcap() {
        // Any other (real, negative) special key falls through to the
        // plain key2termcap0/1 bit-math, exactly like K_SECOND/K_THIRD
        // do for the non-K_SPECIAL/non-NUL case.
        assert_eq!(k_second(K_UP), key2termcap0(K_UP));
        assert_eq!(k_third(K_UP), key2termcap1(K_UP));
        assert_eq!(k_second(K_UP), b'k');
        assert_eq!(k_third(K_UP), b'u');
    }

    #[test]
    fn mod_mask_table_has_no_sentinel_entry() {
        // Unlike the original's NUL-terminated array, every entry here
        // is a real, meaningful modifier mapping.
        assert_eq!(MOD_MASK_TABLE.len(), 9);
        assert!(MOD_MASK_TABLE.iter().all(|e| e.mod_flag != 0));
    }

    #[test]
    fn modifier_keys_table_has_75_entries() {
        // Hand-counted from the original's own modifier_keys_table
        // initializer (32 named entries + 4 vt100 F1-4 + 33 F1-F37
        // shifted function keys + 1 TAB pseudo-entry = 75), with no
        // NUL-sentinel entry needed here.
        assert_eq!(MODIFIER_KEYS_TABLE.len(), 75);
    }

    #[test]
    fn modifier_keys_table_entries_have_a_nonzero_mod_mask() {
        assert!(MODIFIER_KEYS_TABLE.iter().all(|&(mod_mask, ..)| mod_mask != 0));
    }
}
