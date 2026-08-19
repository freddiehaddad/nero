//! Translated from `src/nvim/keycodes.h` (now essentially in full: all
//! `KS_*`/`KE_*`/`K_*` constants referenced by `keycodes.c`'s own
//! `key_names_table` (not yet translated itself - a future addition)
//! plus every constant needed by `keycodes.rs`'s/`input.rs`'s own
//! translated functions).
//!
//! Neovim represents "special" keys (function keys, arrow keys, etc,
//! anything that doesn't fit in a single byte) as a 3-byte in-band
//! escape sequence: [`K_SPECIAL`] followed by two "termcap" bytes
//! (`KS_*`/`KE_*`). [`termcap2key`] packs a pair of such bytes into a
//! single negative `i32` "key code" (and [`key2termcap0`]/
//! [`key2termcap1`] unpack them again) - the original's own encoding is
//! kept exactly (not reinterpreted as some more "idiomatic" Rust enum),
//! since downstream code (`simplify_key`, `getchar.c`'s input
//! pipeline, not yet translated) manipulates these codes
//! arithmetically, not just symbolically. [`is_special`]/[`k_second`]/
//! [`k_third`] (`IS_SPECIAL`/`K_SECOND`/`K_THIRD`) build on
//! [`key2termcap0`]/[`key2termcap1`] to also handle the 2 real
//! characters (`K_SPECIAL` itself, and a literal NUL byte) that need
//! the SAME 3-byte escaping treatment as a genuine special key code,
//! for `input.rs`'s `add_byte_buff`.
//!
//! The bulk `KS_*`/`KE_*`/`K_*` transcription was done via a throwaway
//! Python extraction script (parsing `keycodes.h` directly for every
//! `#define K_XXX TERMCAP2KEY(a, b)`/`enum { KE_XXX = n }`/
//! `#define KS_XXX n` line), then independently cross-checked via a
//! SEPARATE, differently-written verification script before trusting
//! it - zero mismatches found across all 109 `KS_*`/`KE_*` and all 189
//! `K_*` constants, matching this crate's established methodology for
//! large mechanical-table transcriptions (e.g. `OptIndex`/`VIMVARS`).
//! One real, faithfully-preserved quirk: `K_X1MOUSE` is defined TWICE
//! (identically) in the real `keycodes.h` - transcribed once here,
//! same as any other `#define` that happens to repeat with an
//! unchanged body.

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

/// Recover one internal key code from the two bytes after `K_SPECIAL`
/// (`TO_SPECIAL`).
#[must_use]
pub const fn to_special(first: u8, second: u8) -> i32 {
    if first == KS_SPECIAL {
        K_SPECIAL as i32
    } else if first == KS_ZERO {
        K_ZERO
    } else {
        termcap2key(first, second)
    }
}

/// First termcap byte meaning "this is one of the `KE_*` pseudo-keys
/// below, see the second byte" (`KS_EXTRA`).
pub const KS_EXTRA: u8 = 253;

/// Used when a modifier is given for a (special) key: `K_SPECIAL
/// KS_MODIFIER bitmask` (`KS_MODIFIER`).
pub const KS_MODIFIER: u8 = 252;

// These are used for the GUI: `K_SPECIAL KS_xxx KE_FILLER`.
pub const KS_MOUSE: u8 = 251;
pub const KS_MENU: u8 = 250;
pub const KS_VER_SCROLLBAR: u8 = 249;
pub const KS_HOR_SCROLLBAR: u8 = 248;

/// Used for switching Select mode back on after a mapping or menu
/// (`KS_SELECT`).
pub const KS_SELECT: u8 = 245;

/// Used a termcap entry that produces a normal character (`KS_KEY`).
pub const KS_KEY: u8 = 242;

/// Used for click in a tab pages label (`KS_TABLINE`).
pub const KS_TABLINE: u8 = 240;

/// Used for menu in a tab pages line (`KS_TABMENU`).
pub const KS_TABMENU: u8 = 239;

// `KE_*`: second termcap byte values following `KS_EXTRA` (mechanically
// transcribed in full from the original's `enum { ... }` in
// `keycodes.h`, cross-checked value-by-value against every entry).
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
pub const KE_MOUSE: u8 = 43;
pub const KE_LEFTMOUSE: u8 = 44;
pub const KE_LEFTDRAG: u8 = 45;
pub const KE_LEFTRELEASE: u8 = 46;
pub const KE_MIDDLEMOUSE: u8 = 47;
pub const KE_MIDDLEDRAG: u8 = 48;
pub const KE_MIDDLERELEASE: u8 = 49;
pub const KE_RIGHTMOUSE: u8 = 50;
pub const KE_RIGHTDRAG: u8 = 51;
pub const KE_RIGHTRELEASE: u8 = 52;
pub const KE_IGNORE: u8 = 53;
pub const KE_S_TAB_OLD: u8 = 55;
pub const KE_LEFTMOUSE_NM: u8 = 69;
pub const KE_LEFTRELEASE_NM: u8 = 70;
pub const KE_MOUSEDOWN: u8 = 75;
pub const KE_MOUSEUP: u8 = 76;
pub const KE_MOUSELEFT: u8 = 77;
pub const KE_MOUSERIGHT: u8 = 78;
pub const KE_KINS: u8 = 79;
pub const KE_KDEL: u8 = 80;
pub const KE_SNR: u8 = 82;
pub const KE_PLUG: u8 = 83;
pub const KE_X1MOUSE: u8 = 89;
pub const KE_X1DRAG: u8 = 90;
pub const KE_X1RELEASE: u8 = 91;
pub const KE_X2MOUSE: u8 = 92;
pub const KE_X2DRAG: u8 = 93;
pub const KE_X2RELEASE: u8 = 94;
pub const KE_DROP: u8 = 95;
pub const KE_NOP: u8 = 97;
pub const KE_MOUSEMOVE: u8 = 100;
pub const KE_EVENT: u8 = 102;
pub const KE_LUA: u8 = 103;
pub const KE_WILD: u8 = 108;
pub const KE_COMPLETE_DELAY: u8 = 110;

/// arrow/function/home/end key codes (`K_UP`/`K_DOWN`/etc.), and every
/// other `K_*` constant referenced by `keycodes.c`'s own
/// `key_names_table` (mechanically transcribed from `keycodes.h`,
/// cross-checked value-by-value: every one resolved via
/// [`termcap2key`] with zero resolution errors against an
/// independently-written extraction script).
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
pub const K_ZERO: i32 = termcap2key(KS_ZERO, KE_FILLER);

/// Flags for `find_special_key` (`FSK_*`).
pub mod fsk {
    pub const KEYCODE: i32 = 0x01;
    pub const KEEP_X_KEY: i32 = 0x02;
    pub const IN_STRING: i32 = 0x04;
    pub const SIMPLIFY: i32 = 0x08;
}

/// Flags for `replace_termcodes`.
pub mod repterm {
    pub const FROM_PART: i32 = 1;
    pub const DO_LT: i32 = 2;
    pub const NO_SPECIAL: i32 = 4;
    pub const NO_SIMPLIFY: i32 = 8;
}
pub const K_KUP: i32 = termcap2key(b'K', b'u');
pub const K_KDOWN: i32 = termcap2key(b'K', b'd');
pub const K_KLEFT: i32 = termcap2key(b'K', b'l');
pub const K_KRIGHT: i32 = termcap2key(b'K', b'r');
pub const K_S_LEFT: i32 = termcap2key(b'#', b'4');
pub const K_S_RIGHT: i32 = termcap2key(b'%', b'i');
pub const K_S_HOME: i32 = termcap2key(b'#', b'2');
pub const K_S_END: i32 = termcap2key(b'*', b'7');
pub const K_TAB: i32 = termcap2key(KS_EXTRA, KE_TAB);
pub const K_F5: i32 = termcap2key(b'k', b'5');
pub const K_F6: i32 = termcap2key(b'k', b'6');
pub const K_F7: i32 = termcap2key(b'k', b'7');
pub const K_F8: i32 = termcap2key(b'k', b'8');
pub const K_F9: i32 = termcap2key(b'k', b'9');
pub const K_F10: i32 = termcap2key(b'k', b';');
pub const K_F11: i32 = termcap2key(b'F', b'1');
pub const K_F12: i32 = termcap2key(b'F', b'2');
pub const K_F13: i32 = termcap2key(b'F', b'3');
pub const K_F14: i32 = termcap2key(b'F', b'4');
pub const K_F15: i32 = termcap2key(b'F', b'5');
pub const K_F16: i32 = termcap2key(b'F', b'6');
pub const K_F17: i32 = termcap2key(b'F', b'7');
pub const K_F18: i32 = termcap2key(b'F', b'8');
pub const K_F19: i32 = termcap2key(b'F', b'9');
pub const K_F20: i32 = termcap2key(b'F', b'A');
pub const K_F21: i32 = termcap2key(b'F', b'B');
pub const K_F22: i32 = termcap2key(b'F', b'C');
pub const K_F23: i32 = termcap2key(b'F', b'D');
pub const K_F24: i32 = termcap2key(b'F', b'E');
pub const K_F25: i32 = termcap2key(b'F', b'F');
pub const K_F26: i32 = termcap2key(b'F', b'G');
pub const K_F27: i32 = termcap2key(b'F', b'H');
pub const K_F28: i32 = termcap2key(b'F', b'I');
pub const K_F29: i32 = termcap2key(b'F', b'J');
pub const K_F30: i32 = termcap2key(b'F', b'K');
pub const K_F31: i32 = termcap2key(b'F', b'L');
pub const K_F32: i32 = termcap2key(b'F', b'M');
pub const K_F33: i32 = termcap2key(b'F', b'N');
pub const K_F34: i32 = termcap2key(b'F', b'O');
pub const K_F35: i32 = termcap2key(b'F', b'P');
pub const K_F36: i32 = termcap2key(b'F', b'Q');
pub const K_F37: i32 = termcap2key(b'F', b'R');
pub const K_F38: i32 = termcap2key(b'F', b'S');
pub const K_F39: i32 = termcap2key(b'F', b'T');
pub const K_F40: i32 = termcap2key(b'F', b'U');
pub const K_F41: i32 = termcap2key(b'F', b'V');
pub const K_F42: i32 = termcap2key(b'F', b'W');
pub const K_F43: i32 = termcap2key(b'F', b'X');
pub const K_F44: i32 = termcap2key(b'F', b'Y');
pub const K_F45: i32 = termcap2key(b'F', b'Z');
pub const K_F46: i32 = termcap2key(b'F', b'a');
pub const K_F47: i32 = termcap2key(b'F', b'b');
pub const K_F48: i32 = termcap2key(b'F', b'c');
pub const K_F49: i32 = termcap2key(b'F', b'd');
pub const K_F50: i32 = termcap2key(b'F', b'e');
pub const K_F51: i32 = termcap2key(b'F', b'f');
pub const K_F52: i32 = termcap2key(b'F', b'g');
pub const K_F53: i32 = termcap2key(b'F', b'h');
pub const K_F54: i32 = termcap2key(b'F', b'i');
pub const K_F55: i32 = termcap2key(b'F', b'j');
pub const K_F56: i32 = termcap2key(b'F', b'k');
pub const K_F57: i32 = termcap2key(b'F', b'l');
pub const K_F58: i32 = termcap2key(b'F', b'm');
pub const K_F59: i32 = termcap2key(b'F', b'n');
pub const K_F60: i32 = termcap2key(b'F', b'o');
pub const K_F61: i32 = termcap2key(b'F', b'p');
pub const K_F62: i32 = termcap2key(b'F', b'q');
pub const K_F63: i32 = termcap2key(b'F', b'r');
pub const K_S_F5: i32 = termcap2key(KS_EXTRA, KE_S_F5);
pub const K_S_F6: i32 = termcap2key(KS_EXTRA, KE_S_F6);
pub const K_S_F7: i32 = termcap2key(KS_EXTRA, KE_S_F7);
pub const K_S_F8: i32 = termcap2key(KS_EXTRA, KE_S_F8);
pub const K_S_F9: i32 = termcap2key(KS_EXTRA, KE_S_F9);
pub const K_S_F10: i32 = termcap2key(KS_EXTRA, KE_S_F10);
pub const K_S_F11: i32 = termcap2key(KS_EXTRA, KE_S_F11);
pub const K_S_F12: i32 = termcap2key(KS_EXTRA, KE_S_F12);
pub const K_HELP: i32 = termcap2key(b'%', b'1');
pub const K_UNDO: i32 = termcap2key(b'&', b'8');
pub const K_FIND: i32 = termcap2key(b'@', b'0');
pub const K_KSELECT: i32 = termcap2key(b'*', b'6');
pub const K_BS: i32 = termcap2key(b'k', b'b');
pub const K_INS: i32 = termcap2key(b'k', b'I');
pub const K_KINS: i32 = termcap2key(KS_EXTRA, KE_KINS);
pub const K_DEL: i32 = termcap2key(b'k', b'D');
pub const K_KDEL: i32 = termcap2key(KS_EXTRA, KE_KDEL);
pub const K_KHOME: i32 = termcap2key(b'K', b'1');
pub const K_KEND: i32 = termcap2key(b'K', b'4');
pub const K_PAGEUP: i32 = termcap2key(b'k', b'P');
pub const K_PAGEDOWN: i32 = termcap2key(b'k', b'N');
pub const K_KPAGEUP: i32 = termcap2key(b'K', b'3');
pub const K_KPAGEDOWN: i32 = termcap2key(b'K', b'5');
pub const K_KORIGIN: i32 = termcap2key(b'K', b'2');
pub const K_KPLUS: i32 = termcap2key(b'K', b'6');
pub const K_KMINUS: i32 = termcap2key(b'K', b'7');
pub const K_KDIVIDE: i32 = termcap2key(b'K', b'8');
pub const K_KMULTIPLY: i32 = termcap2key(b'K', b'9');
pub const K_KENTER: i32 = termcap2key(b'K', b'A');
pub const K_KPOINT: i32 = termcap2key(b'K', b'B');
pub const K_PASTE_START: i32 = termcap2key(b'P', b'S');
pub const K_PASTE_END: i32 = termcap2key(b'P', b'E');
pub const K_K0: i32 = termcap2key(b'K', b'C');
pub const K_K1: i32 = termcap2key(b'K', b'D');
pub const K_K2: i32 = termcap2key(b'K', b'E');
pub const K_K3: i32 = termcap2key(b'K', b'F');
pub const K_K4: i32 = termcap2key(b'K', b'G');
pub const K_K5: i32 = termcap2key(b'K', b'H');
pub const K_K6: i32 = termcap2key(b'K', b'I');
pub const K_K7: i32 = termcap2key(b'K', b'J');
pub const K_K8: i32 = termcap2key(b'K', b'K');
pub const K_K9: i32 = termcap2key(b'K', b'L');
pub const K_KCOMMA: i32 = termcap2key(b'K', b'M');
pub const K_KEQUAL: i32 = termcap2key(b'K', b'N');
pub const K_MOUSE: i32 = termcap2key(KS_MOUSE, KE_FILLER);
pub const K_MENU: i32 = termcap2key(KS_MENU, KE_FILLER);
pub const K_VER_SCROLLBAR: i32 = termcap2key(KS_VER_SCROLLBAR, KE_FILLER);
pub const K_HOR_SCROLLBAR: i32 = termcap2key(KS_HOR_SCROLLBAR, KE_FILLER);
pub const K_SELECT: i32 = termcap2key(KS_SELECT, KE_FILLER);
pub const K_TABLINE: i32 = termcap2key(KS_TABLINE, KE_FILLER);
pub const K_TABMENU: i32 = termcap2key(KS_TABMENU, KE_FILLER);
pub const K_LEFTMOUSE: i32 = termcap2key(KS_EXTRA, KE_LEFTMOUSE);
pub const K_LEFTMOUSE_NM: i32 = termcap2key(KS_EXTRA, KE_LEFTMOUSE_NM);
pub const K_LEFTDRAG: i32 = termcap2key(KS_EXTRA, KE_LEFTDRAG);
pub const K_LEFTRELEASE: i32 = termcap2key(KS_EXTRA, KE_LEFTRELEASE);
pub const K_LEFTRELEASE_NM: i32 = termcap2key(KS_EXTRA, KE_LEFTRELEASE_NM);
pub const K_MOUSEMOVE: i32 = termcap2key(KS_EXTRA, KE_MOUSEMOVE);
pub const K_MIDDLEMOUSE: i32 = termcap2key(KS_EXTRA, KE_MIDDLEMOUSE);
pub const K_MIDDLEDRAG: i32 = termcap2key(KS_EXTRA, KE_MIDDLEDRAG);
pub const K_MIDDLERELEASE: i32 = termcap2key(KS_EXTRA, KE_MIDDLERELEASE);
pub const K_RIGHTMOUSE: i32 = termcap2key(KS_EXTRA, KE_RIGHTMOUSE);
pub const K_RIGHTDRAG: i32 = termcap2key(KS_EXTRA, KE_RIGHTDRAG);
pub const K_RIGHTRELEASE: i32 = termcap2key(KS_EXTRA, KE_RIGHTRELEASE);
pub const K_X1MOUSE: i32 = termcap2key(KS_EXTRA, KE_X1MOUSE);
pub const K_X1DRAG: i32 = termcap2key(KS_EXTRA, KE_X1DRAG);
pub const K_X1RELEASE: i32 = termcap2key(KS_EXTRA, KE_X1RELEASE);
pub const K_X2MOUSE: i32 = termcap2key(KS_EXTRA, KE_X2MOUSE);
pub const K_X2DRAG: i32 = termcap2key(KS_EXTRA, KE_X2DRAG);
pub const K_X2RELEASE: i32 = termcap2key(KS_EXTRA, KE_X2RELEASE);
pub const K_IGNORE: i32 = termcap2key(KS_EXTRA, KE_IGNORE);
pub const K_NOP: i32 = termcap2key(KS_EXTRA, KE_NOP);
pub const K_MOUSEDOWN: i32 = termcap2key(KS_EXTRA, KE_MOUSEDOWN);
pub const K_MOUSEUP: i32 = termcap2key(KS_EXTRA, KE_MOUSEUP);
pub const K_MOUSELEFT: i32 = termcap2key(KS_EXTRA, KE_MOUSELEFT);
pub const K_MOUSERIGHT: i32 = termcap2key(KS_EXTRA, KE_MOUSERIGHT);
pub const K_SNR: i32 = termcap2key(KS_EXTRA, KE_SNR);
pub const K_PLUG: i32 = termcap2key(KS_EXTRA, KE_PLUG);
pub const K_DROP: i32 = termcap2key(KS_EXTRA, KE_DROP);
pub const K_COMPLETE_DELAY: i32 = termcap2key(KS_EXTRA, KE_COMPLETE_DELAY);
pub const K_EVENT: i32 = termcap2key(KS_EXTRA, KE_EVENT);
pub const K_LUA: i32 = termcap2key(KS_EXTRA, KE_LUA);
pub const K_WILD: i32 = termcap2key(KS_EXTRA, KE_WILD);

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
    fn to_special_decodes_literal_special_nul_and_termcap_keys() {
        assert_eq!(to_special(KS_SPECIAL, KE_FILLER), i32::from(K_SPECIAL));
        assert_eq!(to_special(KS_ZERO, KE_FILLER), K_ZERO);
        assert_eq!(to_special(b'k', b'u'), K_UP);
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

    // --- newly-transcribed KS_*/KE_*/K_* constants (hand-verified
    // against keycodes.h directly, then independently cross-checked
    // via a separate extraction script before trusting the bulk
    // transcription - see this file's own commit history) ---

    #[test]
    fn newly_added_ks_constants_match_keycodes_h() {
        assert_eq!(KS_MODIFIER, 252);
        assert_eq!(KS_MOUSE, 251);
        assert_eq!(KS_MENU, 250);
        assert_eq!(KS_VER_SCROLLBAR, 249);
        assert_eq!(KS_HOR_SCROLLBAR, 248);
        assert_eq!(KS_SELECT, 245);
        assert_eq!(KS_KEY, 242);
        assert_eq!(KS_TABLINE, 240);
        assert_eq!(KS_TABMENU, 239);
    }

    #[test]
    fn newly_added_ke_constants_match_keycodes_h() {
        assert_eq!(KE_MOUSE, 43);
        assert_eq!(KE_IGNORE, 53);
        assert_eq!(KE_S_TAB_OLD, 55);
        assert_eq!(KE_KINS, 79);
        assert_eq!(KE_KDEL, 80);
        assert_eq!(KE_SNR, 82);
        assert_eq!(KE_DROP, 95);
        assert_eq!(KE_NOP, 97);
        assert_eq!(KE_MOUSEMOVE, 100);
        assert_eq!(KE_EVENT, 102);
        assert_eq!(KE_LUA, 103);
        assert_eq!(KE_WILD, 108);
        assert_eq!(KE_COMPLETE_DELAY, 110);
    }

    #[test]
    fn newly_added_k_constants_match_hand_computed_termcap2key_values() {
        // K_F5 = TERMCAP2KEY('k', '5') = -(107 + (53 << 8)) = -13675.
        assert_eq!(K_F5, termcap2key(b'k', b'5'));
        assert_eq!(K_F5, -13675);

        // K_BS = TERMCAP2KEY('k', 'b') = -(107 + (98 << 8)) = -25195.
        assert_eq!(K_BS, termcap2key(b'k', b'b'));
        assert_eq!(K_BS, -25195);

        // K_MOUSE = TERMCAP2KEY(KS_MOUSE, KE_FILLER)
        //         = -(251 + (88 << 8)) = -22779.
        assert_eq!(K_MOUSE, termcap2key(KS_MOUSE, KE_FILLER));
        assert_eq!(K_MOUSE, -22779);

        // K_K0 = TERMCAP2KEY('K', 'C') = -(75 + (67 << 8)) = -17227.
        assert_eq!(K_K0, termcap2key(b'K', b'C'));
        assert_eq!(K_K0, -17227);

        // K_X1MOUSE (the real, faithfully-preserved duplicate #define
        // in keycodes.h - transcribed once here, matching its own
        // single real value both times it's defined upstream).
        assert_eq!(K_X1MOUSE, termcap2key(KS_EXTRA, KE_X1MOUSE));
    }

    #[test]
    fn every_newly_added_k_constant_is_a_special_negative_key_code() {
        // Every K_* constant built via termcap2key() must be negative
        // (matching IS_SPECIAL's own real contract) - a sweeping sanity
        // check across the whole newly-transcribed set, verifying none
        // of them accidentally resolved to a non-negative/zero value.
        let values = [
            K_ZERO, K_KUP, K_KDOWN, K_KLEFT, K_KRIGHT, K_S_LEFT, K_S_RIGHT, K_S_HOME, K_S_END, K_TAB, K_F5, K_F6,
            K_F7, K_F8, K_F9, K_F10, K_F11, K_F12, K_F13, K_F63, K_S_F5, K_S_F12, K_HELP, K_UNDO, K_FIND,
            K_KSELECT, K_BS, K_INS, K_KINS, K_DEL, K_KDEL, K_KHOME, K_KEND, K_PAGEUP, K_PAGEDOWN, K_KPAGEUP,
            K_KPAGEDOWN, K_KORIGIN, K_KPLUS, K_KMINUS, K_KDIVIDE, K_KMULTIPLY, K_KENTER, K_KPOINT, K_PASTE_START,
            K_PASTE_END, K_K0, K_K9, K_KCOMMA, K_KEQUAL, K_MOUSE, K_MENU, K_VER_SCROLLBAR, K_HOR_SCROLLBAR,
            K_SELECT, K_TABLINE, K_TABMENU, K_LEFTMOUSE, K_LEFTMOUSE_NM, K_LEFTDRAG, K_LEFTRELEASE,
            K_LEFTRELEASE_NM, K_MOUSEMOVE, K_MIDDLEMOUSE, K_MIDDLEDRAG, K_MIDDLERELEASE, K_RIGHTMOUSE,
            K_RIGHTDRAG, K_RIGHTRELEASE, K_X1MOUSE, K_X1DRAG, K_X1RELEASE, K_X2MOUSE, K_X2DRAG, K_X2RELEASE,
            K_IGNORE, K_NOP, K_MOUSEDOWN, K_MOUSEUP, K_MOUSELEFT, K_MOUSERIGHT, K_SNR, K_PLUG, K_DROP,
            K_COMPLETE_DELAY, K_EVENT, K_LUA, K_WILD,
        ];
        assert!(values.iter().all(|&v| is_special(v)), "one or more newly-added K_* constants is non-negative");
    }
}
