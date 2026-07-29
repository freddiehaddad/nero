//! Translated from `src/nvim/keycodes.h` (partial: only the constants
//! needed by `keycodes.rs`'s own translated functions so far).
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
//! these codes arithmetically, not just symbolically.

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
    fn mod_mask_table_has_no_sentinel_entry() {
        // Unlike the original's NUL-terminated array, every entry here
        // is a real, meaningful modifier mapping.
        assert_eq!(MOD_MASK_TABLE.len(), 9);
        assert!(MOD_MASK_TABLE.iter().all(|e| e.mod_flag != 0));
    }
}
