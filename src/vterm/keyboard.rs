//! Translated from `src/nvim/vterm/keyboard.c`.
//!
//! The static key-description tables are translated first. Sequence
//! generation is layered on top of these exact descriptors below.

/// Key output encoding (`keycodes_s.type`).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeycodeType {
    None,
    Literal,
    Tab,
    Enter,
    Ss3,
    Csi,
    CsiCursor,
    CsiNum,
    Keypad,
}

/// One entry from libvterm's key description tables (`keycodes_s`).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Keycode {
    key_type: KeycodeType,
    literal: i32,
    csi_num: i32,
}

/// Ordinary terminal keys (`keycodes`).
#[allow(dead_code)]
const KEYCODES: [Keycode; 15] = [
    Keycode { key_type: KeycodeType::None, literal: 0, csi_num: 0 },
    Keycode { key_type: KeycodeType::Enter, literal: b'\r' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::Tab, literal: b'\t' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::Literal, literal: 0x7F, csi_num: 0 },
    Keycode { key_type: KeycodeType::Literal, literal: 0x1B, csi_num: 0 },
    Keycode { key_type: KeycodeType::CsiCursor, literal: b'A' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::CsiCursor, literal: b'B' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::CsiCursor, literal: b'D' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::CsiCursor, literal: b'C' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 2 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 3 },
    Keycode { key_type: KeycodeType::CsiCursor, literal: b'H' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::CsiCursor, literal: b'F' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 5 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 6 },
];

/// Function keys (`keycodes_fn`), indexed from F0.
#[allow(dead_code)]
const KEYCODES_FN: [Keycode; 13] = [
    Keycode { key_type: KeycodeType::None, literal: 0, csi_num: 0 },
    Keycode { key_type: KeycodeType::Ss3, literal: b'P' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::Ss3, literal: b'Q' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::Ss3, literal: b'R' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::Ss3, literal: b'S' as i32, csi_num: 0 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 15 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 17 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 18 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 19 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 20 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 21 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 23 },
    Keycode { key_type: KeycodeType::CsiNum, literal: b'~' as i32, csi_num: 24 },
];

/// Legacy keypad key descriptions (`keycodes_kp`).
#[allow(dead_code)]
const KEYCODES_KP: [Keycode; 18] = [
    Keycode { key_type: KeycodeType::Keypad, literal: b'0' as i32, csi_num: b'p' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'1' as i32, csi_num: b'q' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'2' as i32, csi_num: b'r' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'3' as i32, csi_num: b's' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'4' as i32, csi_num: b't' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'5' as i32, csi_num: b'u' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'6' as i32, csi_num: b'v' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'7' as i32, csi_num: b'w' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'8' as i32, csi_num: b'x' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'9' as i32, csi_num: b'y' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'*' as i32, csi_num: b'j' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'+' as i32, csi_num: b'k' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b',' as i32, csi_num: b'l' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'-' as i32, csi_num: b'm' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'.' as i32, csi_num: b'n' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'/' as i32, csi_num: b'o' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'\n' as i32, csi_num: b'M' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: b'=' as i32, csi_num: b'X' as i32 },
];

/// CSI-u keypad key descriptions (`keycodes_kp_csiu`).
#[allow(dead_code)]
const KEYCODES_KP_CSIU: [Keycode; 18] = [
    Keycode { key_type: KeycodeType::Keypad, literal: 57399, csi_num: b'p' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57400, csi_num: b'q' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57401, csi_num: b'r' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57402, csi_num: b's' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57403, csi_num: b't' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57404, csi_num: b'u' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57405, csi_num: b'v' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57406, csi_num: b'w' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57407, csi_num: b'x' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57408, csi_num: b'y' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57411, csi_num: b'j' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57413, csi_num: b'k' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57416, csi_num: b'l' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57412, csi_num: b'm' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57409, csi_num: b'n' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57410, csi_num: b'o' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57414, csi_num: b'M' as i32 },
    Keycode { key_type: KeycodeType::Keypad, literal: 57415, csi_num: b'X' as i32 },
];

/// Whether a Unicode key bypasses modifier encoding and is emitted as
/// plain UTF-8 (`vterm_keyboard_unichar`'s `passthru` test).
#[allow(dead_code)]
fn unicode_passthrough(c: u32, modifiers: crate::vterm_defs::VTermModifier) -> bool {
    if c == u32::from(b' ') {
        modifiers == crate::vterm_defs::VTERM_MOD_NONE
    } else {
        modifiers & !crate::vterm_defs::VTERM_MOD_SHIFT == 0
    }
}

/// Applies the legacy Ctrl-key codepoint mapping from
/// `vterm_keyboard_unichar`.
#[allow(dead_code)]
fn control_codepoint(c: u32) -> u32 {
    match c {
        0x32 | 0x20 => 0,
        0x33..=0x37 => 0x1B + c - u32::from(b'3'),
        0x38 => 0x7F,
        0x2F => 0x1F,
        0x40..=0x7F => c & 0x1F,
        _ => c,
    }
}

/// Encodes one Unicode key press (`vterm_keyboard_unichar`).
///
/// The C function pushes these bytes through `VTerm`'s output
/// callback. Until the output-callback host is translated, this
/// equivalent returns the exact byte sequence to its caller.
#[must_use]
pub fn vterm_keyboard_unichar(
    mut c: u32,
    mut modifiers: crate::vterm_defs::VTermModifier,
    flags: crate::vterm_defs::VTermKeyEncodingFlags,
    ctrl8bit: bool,
) -> Vec<u8> {
    if unicode_passthrough(c, modifiers) {
        let mut bytes = [0; 6];
        let len = crate::mbyte::utf_char2bytes(c as i32, &mut bytes) as usize;
        return bytes[..len].to_vec();
    }

    if flags.disambiguate {
        // CSI-u always reports the unshifted ASCII codepoint.
        if (u32::from(b'A')..=u32::from(b'Z')).contains(&c) {
            c += u32::from(b'a' - b'A');
            modifiers |= crate::vterm_defs::VTERM_MOD_SHIFT;
        }
        let mut output = Vec::new();
        if ctrl8bit {
            output.push(crate::vterm_defs::C1_CSI);
        } else {
            output.extend_from_slice(b"\x1b[");
        }
        output.extend_from_slice(format!("{c};{}u", modifiers + 1).as_bytes());
        return output;
    }

    if modifiers & crate::vterm_defs::VTERM_MOD_CTRL != 0 {
        c = control_codepoint(c);
    }

    let mut output = Vec::with_capacity(2);
    if modifiers & crate::vterm_defs::VTERM_MOD_ALT != 0 {
        output.push(0x1B);
    }
    // `%c` in the original writes the low unsigned-char byte.
    output.push(c as u8);
    output
}

/// Selects a key descriptor from the three static keyboard tables.
#[allow(dead_code)]
fn select_keycode(
    key: crate::vterm_defs::VTermKey,
    flags: crate::vterm_defs::VTermKeyEncodingFlags,
) -> Option<Keycode> {
    if key < 0 {
        return None;
    }
    if key < crate::vterm_defs::VTERM_KEY_FUNCTION_0 {
        KEYCODES.get(key as usize).copied()
    } else if key <= crate::vterm_defs::VTERM_KEY_FUNCTION_MAX {
        KEYCODES_FN
            .get((key - crate::vterm_defs::VTERM_KEY_FUNCTION_0) as usize)
            .copied()
    } else if key >= crate::vterm_defs::VTERM_KEY_KP_0 {
        let index = (key - crate::vterm_defs::VTERM_KEY_KP_0) as usize;
        if flags.disambiguate {
            KEYCODES_KP_CSIU.get(index).copied()
        } else {
            KEYCODES_KP.get(index).copied()
        }
    } else {
        None
    }
}

fn push_control(output: &mut Vec<u8>, ctrl: u8, ctrl8bit: bool) {
    if ctrl >= 0x80 && !ctrl8bit {
        output.extend_from_slice(&[0x1B, ctrl - 0x40]);
    } else {
        output.push(ctrl);
    }
}

fn push_literal(
    output: &mut Vec<u8>,
    keycode: Keycode,
    key: crate::vterm_defs::VTermKey,
    modifiers: crate::vterm_defs::VTermModifier,
    mut disambiguate: bool,
    ctrl8bit: bool,
) {
    if disambiguate
        && matches!(
            key,
            crate::vterm_defs::VTERM_KEY_TAB
                | crate::vterm_defs::VTERM_KEY_ENTER
                | crate::vterm_defs::VTERM_KEY_BACKSPACE
        )
    {
        disambiguate = modifiers != crate::vterm_defs::VTERM_MOD_NONE;
    }
    if disambiguate {
        push_control(output, crate::vterm_defs::C1_CSI, ctrl8bit);
        output.extend_from_slice(
            format!("{};{}u", keycode.literal, modifiers + 1).as_bytes(),
        );
    } else {
        if modifiers & crate::vterm_defs::VTERM_MOD_ALT != 0 {
            output.push(0x1B);
        }
        output.push(keycode.literal as u8);
    }
}

fn push_csi(
    output: &mut Vec<u8>,
    literal: i32,
    modifiers: crate::vterm_defs::VTermModifier,
    ctrl8bit: bool,
) {
    push_control(output, crate::vterm_defs::C1_CSI, ctrl8bit);
    if modifiers == 0 {
        output.push(literal as u8);
    } else {
        output.extend_from_slice(format!("1;{}{}", modifiers + 1, literal as u8 as char).as_bytes());
    }
}

fn push_ss3(
    output: &mut Vec<u8>,
    literal: i32,
    modifiers: crate::vterm_defs::VTermModifier,
    ctrl8bit: bool,
) {
    if modifiers == 0 {
        push_control(output, crate::vterm_defs::C1_SS3, ctrl8bit);
        output.push(literal as u8);
    } else {
        push_csi(output, literal, modifiers, ctrl8bit);
    }
}

/// Encodes one special terminal key (`vterm_keyboard_key`).
///
/// As with [`vterm_keyboard_unichar`], this returns the exact bytes
/// that the C implementation pushes through `VTerm`'s output callback.
#[must_use]
pub fn vterm_keyboard_key(
    key: crate::vterm_defs::VTermKey,
    modifiers: crate::vterm_defs::VTermModifier,
    flags: crate::vterm_defs::VTermKeyEncodingFlags,
    mode: crate::vterm_defs::VTermKeyboardMode,
) -> Vec<u8> {
    if key == crate::vterm_defs::VTERM_KEY_NONE {
        return Vec::new();
    }
    let Some(mut keycode) = select_keycode(key, flags) else {
        return Vec::new();
    };
    let mut output = Vec::new();

    match keycode.key_type {
        KeycodeType::None => {}
        KeycodeType::Tab => {
            if flags.disambiguate {
                push_literal(
                    &mut output,
                    keycode,
                    key,
                    modifiers,
                    true,
                    mode.ctrl8bit,
                );
            } else if modifiers == crate::vterm_defs::VTERM_MOD_SHIFT {
                push_control(&mut output, crate::vterm_defs::C1_CSI, mode.ctrl8bit);
                output.push(b'Z');
            } else if modifiers & crate::vterm_defs::VTERM_MOD_SHIFT != 0 {
                push_control(&mut output, crate::vterm_defs::C1_CSI, mode.ctrl8bit);
                output.extend_from_slice(format!("1;{}Z", modifiers + 1).as_bytes());
            } else {
                push_literal(
                    &mut output,
                    keycode,
                    key,
                    modifiers,
                    false,
                    mode.ctrl8bit,
                );
            }
        }
        KeycodeType::Enter if mode.newline => output.extend_from_slice(b"\r\n"),
        KeycodeType::Enter | KeycodeType::Literal => push_literal(
            &mut output,
            keycode,
            key,
            modifiers,
            flags.disambiguate,
            mode.ctrl8bit,
        ),
        KeycodeType::Ss3 => push_ss3(
            &mut output,
            keycode.literal,
            modifiers,
            mode.ctrl8bit,
        ),
        KeycodeType::Csi => push_csi(
            &mut output,
            keycode.literal,
            modifiers,
            mode.ctrl8bit,
        ),
        KeycodeType::CsiNum => {
            push_control(&mut output, crate::vterm_defs::C1_CSI, mode.ctrl8bit);
            let text = if modifiers == 0 {
                format!("{}{}", keycode.csi_num, keycode.literal as u8 as char)
            } else {
                format!(
                    "{};{}{}",
                    keycode.csi_num,
                    modifiers + 1,
                    keycode.literal as u8 as char
                )
            };
            output.extend_from_slice(text.as_bytes());
        }
        KeycodeType::CsiCursor if mode.cursor => push_ss3(
            &mut output,
            keycode.literal,
            modifiers,
            mode.ctrl8bit,
        ),
        KeycodeType::CsiCursor => push_csi(
            &mut output,
            keycode.literal,
            modifiers,
            mode.ctrl8bit,
        ),
        KeycodeType::Keypad if mode.keypad => {
            keycode.literal = keycode.csi_num;
            push_ss3(
                &mut output,
                keycode.literal,
                modifiers,
                mode.ctrl8bit,
            );
        }
        KeycodeType::Keypad => push_literal(
            &mut output,
            keycode,
            key,
            modifiers,
            flags.disambiguate,
            mode.ctrl8bit,
        ),
    }
    output
}

/// Emits the bracketed-paste start marker
/// (`vterm_keyboard_start_paste`).
#[must_use]
pub fn vterm_keyboard_start_paste(
    mode: crate::vterm_defs::VTermKeyboardMode,
) -> Vec<u8> {
    if !mode.bracketpaste {
        return Vec::new();
    }
    let mut output = Vec::with_capacity(6);
    push_control(&mut output, crate::vterm_defs::C1_CSI, mode.ctrl8bit);
    output.extend_from_slice(b"200~");
    output
}

/// Emits the bracketed-paste end marker (`vterm_keyboard_end_paste`).
#[must_use]
pub fn vterm_keyboard_end_paste(
    mode: crate::vterm_defs::VTermKeyboardMode,
) -> Vec<u8> {
    if !mode.bracketpaste {
        return Vec::new();
    }
    let mut output = Vec::with_capacity(6);
    push_control(&mut output, crate::vterm_defs::C1_CSI, mode.ctrl8bit);
    output.extend_from_slice(b"201~");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_keycode_table_matches_keyboard_c() {
        assert_eq!(KEYCODES.len(), 15);
        assert_eq!(KEYCODES[0], Keycode {
            key_type: KeycodeType::None,
            literal: 0,
            csi_num: 0,
        });
        assert_eq!(KEYCODES[crate::vterm_defs::VTERM_KEY_ENTER as usize].key_type, KeycodeType::Enter);
        assert_eq!(KEYCODES[crate::vterm_defs::VTERM_KEY_TAB as usize].key_type, KeycodeType::Tab);
        assert_eq!(
            KEYCODES[crate::vterm_defs::VTERM_KEY_BACKSPACE as usize].literal,
            0x7F
        );
        assert_eq!(
            KEYCODES[crate::vterm_defs::VTERM_KEY_ESCAPE as usize].literal,
            0x1B
        );
        assert_eq!(
            [
                KEYCODES[crate::vterm_defs::VTERM_KEY_UP as usize].literal,
                KEYCODES[crate::vterm_defs::VTERM_KEY_DOWN as usize].literal,
                KEYCODES[crate::vterm_defs::VTERM_KEY_LEFT as usize].literal,
                KEYCODES[crate::vterm_defs::VTERM_KEY_RIGHT as usize].literal,
            ],
            [b'A' as i32, b'B' as i32, b'D' as i32, b'C' as i32]
        );
        assert_eq!(
            [
                KEYCODES[crate::vterm_defs::VTERM_KEY_INS as usize].csi_num,
                KEYCODES[crate::vterm_defs::VTERM_KEY_DEL as usize].csi_num,
                KEYCODES[crate::vterm_defs::VTERM_KEY_PAGEUP as usize].csi_num,
                KEYCODES[crate::vterm_defs::VTERM_KEY_PAGEDOWN as usize].csi_num,
            ],
            [2, 3, 5, 6]
        );
    }

    #[test]
    fn function_keycode_table_matches_keyboard_c() {
        assert_eq!(KEYCODES_FN.len(), 13);
        assert_eq!(KEYCODES_FN[0].key_type, KeycodeType::None);
        assert_eq!(
            KEYCODES_FN[1..=4]
                .iter()
                .map(|key| (key.key_type, key.literal))
                .collect::<Vec<_>>(),
            [
                (KeycodeType::Ss3, b'P' as i32),
                (KeycodeType::Ss3, b'Q' as i32),
                (KeycodeType::Ss3, b'R' as i32),
                (KeycodeType::Ss3, b'S' as i32),
            ]
        );
        assert_eq!(
            KEYCODES_FN[5..=12]
                .iter()
                .map(|key| key.csi_num)
                .collect::<Vec<_>>(),
            [15, 17, 18, 19, 20, 21, 23, 24]
        );
        assert!(
            KEYCODES_FN[5..=12]
                .iter()
                .all(|key| key.key_type == KeycodeType::CsiNum && key.literal == b'~' as i32)
        );
    }

    #[test]
    fn legacy_keypad_table_matches_keyboard_c() {
        assert_eq!(KEYCODES_KP.len(), 18);
        assert!(KEYCODES_KP.iter().all(|key| key.key_type == KeycodeType::Keypad));
        assert_eq!(
            KEYCODES_KP.iter().map(|key| key.literal).collect::<Vec<_>>(),
            [
                b'0' as i32,
                b'1' as i32,
                b'2' as i32,
                b'3' as i32,
                b'4' as i32,
                b'5' as i32,
                b'6' as i32,
                b'7' as i32,
                b'8' as i32,
                b'9' as i32,
                b'*' as i32,
                b'+' as i32,
                b',' as i32,
                b'-' as i32,
                b'.' as i32,
                b'/' as i32,
                b'\n' as i32,
                b'=' as i32,
            ]
        );
        assert_eq!(
            KEYCODES_KP.iter().map(|key| key.csi_num).collect::<Vec<_>>(),
            b"pqrstuvwxyjklmnoMX"
                .iter()
                .map(|&byte| i32::from(byte))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn csiu_keypad_table_matches_keyboard_c() {
        assert_eq!(KEYCODES_KP_CSIU.len(), KEYCODES_KP.len());
        assert!(
            KEYCODES_KP_CSIU
                .iter()
                .all(|key| key.key_type == KeycodeType::Keypad)
        );
        assert_eq!(
            KEYCODES_KP_CSIU
                .iter()
                .map(|key| key.literal)
                .collect::<Vec<_>>(),
            [
                57399, 57400, 57401, 57402, 57403, 57404, 57405, 57406, 57407,
                57408, 57411, 57413, 57416, 57412, 57409, 57410, 57414, 57415,
            ]
        );
        assert_eq!(
            KEYCODES_KP_CSIU
                .iter()
                .map(|key| key.csi_num)
                .collect::<Vec<_>>(),
            KEYCODES_KP
                .iter()
                .map(|key| key.csi_num)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn unicode_passthrough_requires_unmodified_space() {
        assert!(unicode_passthrough(
            b' ' as u32,
            crate::vterm_defs::VTERM_MOD_NONE
        ));
        for modifiers in [
            crate::vterm_defs::VTERM_MOD_SHIFT,
            crate::vterm_defs::VTERM_MOD_ALT,
            crate::vterm_defs::VTERM_MOD_CTRL,
        ] {
            assert!(!unicode_passthrough(b' ' as u32, modifiers));
        }
    }

    #[test]
    fn unicode_passthrough_ignores_shift_for_non_space_codepoints() {
        assert!(unicode_passthrough(
            b'A' as u32,
            crate::vterm_defs::VTERM_MOD_NONE
        ));
        assert!(unicode_passthrough(
            b'A' as u32,
            crate::vterm_defs::VTERM_MOD_SHIFT
        ));
        assert!(!unicode_passthrough(
            b'A' as u32,
            crate::vterm_defs::VTERM_MOD_ALT
        ));
        assert!(!unicode_passthrough(
            b'A' as u32,
            crate::vterm_defs::VTERM_MOD_CTRL
        ));
    }

    #[test]
    fn control_codepoint_maps_digit_and_slash_special_cases() {
        assert_eq!(control_codepoint(b'2' as u32), 0);
        assert_eq!(control_codepoint(b' ' as u32), 0);
        assert_eq!(
            (b'3'..=b'7')
                .map(|byte| control_codepoint(u32::from(byte)))
                .collect::<Vec<_>>(),
            [0x1B, 0x1C, 0x1D, 0x1E, 0x1F]
        );
        assert_eq!(control_codepoint(b'8' as u32), 0x7F);
        assert_eq!(control_codepoint(b'/' as u32), 0x1F);
    }

    #[test]
    fn control_codepoint_masks_the_historical_ascii_range() {
        assert_eq!(control_codepoint(b'@' as u32), 0);
        assert_eq!(control_codepoint(b'A' as u32), 1);
        assert_eq!(control_codepoint(b'Z' as u32), 26);
        assert_eq!(control_codepoint(b'[' as u32), 27);
        assert_eq!(control_codepoint(0x7F), 0x1F);
    }

    #[test]
    fn control_codepoint_preserves_values_outside_the_mapping() {
        for codepoint in [0x1F, b'1' as u32, b'9' as u32, 0x80, 0x20AC] {
            assert_eq!(control_codepoint(codepoint), codepoint);
        }
    }

    #[test]
    fn keyboard_unichar_passes_plain_and_shifted_nonspace_as_utf8() {
            assert_eq!(
                vterm_keyboard_unichar(
                    0x20AC,
                    crate::vterm_defs::VTERM_MOD_NONE,
                    Default::default(),
                    false,
                ),
                "€".as_bytes()
            );
            assert_eq!(
                vterm_keyboard_unichar(
                    b'A' as u32,
                    crate::vterm_defs::VTERM_MOD_SHIFT,
                    Default::default(),
                    false,
                ),
                b"A"
            );
        }

    #[test]
    fn keyboard_unichar_encodes_csiu_and_unshifts_ascii_uppercase() {
            let flags = crate::vterm_defs::VTermKeyEncodingFlags {
                disambiguate: true,
                ..Default::default()
            };
            assert_eq!(
                vterm_keyboard_unichar(
                    b'A' as u32,
                    crate::vterm_defs::VTERM_MOD_CTRL,
                    flags,
                    false,
                ),
                b"\x1b[97;6u"
            );
            assert_eq!(
                vterm_keyboard_unichar(
                    b'A' as u32,
                    crate::vterm_defs::VTERM_MOD_CTRL,
                    flags,
                    true,
                ),
                [vec![crate::vterm_defs::C1_CSI], b"97;6u".to_vec()].concat()
            );
        }

    #[test]
    fn keyboard_unichar_applies_legacy_control_and_alt_encoding() {
            for (key, expected) in [
                (b'2', 0x00),
                (b'3', 0x1B),
                (b'6', 0x1E),
                (b'8', 0x7F),
                (b'/', 0x1F),
                (b'A', 0x01),
            ] {
                assert_eq!(
                    vterm_keyboard_unichar(
                        u32::from(key),
                        crate::vterm_defs::VTERM_MOD_CTRL,
                        Default::default(),
                        false,
                    ),
                    [expected]
                );
            }
            assert_eq!(
                vterm_keyboard_unichar(
                    b'x' as u32,
                    crate::vterm_defs::VTERM_MOD_ALT,
                    Default::default(),
                    false,
                ),
                b"\x1bx"
            );
            assert_eq!(
                vterm_keyboard_unichar(
                    b'A' as u32,
                    crate::vterm_defs::VTERM_MOD_CTRL | crate::vterm_defs::VTERM_MOD_ALT,
                    Default::default(),
                    false,
                ),
                b"\x1b\x01"
            );
        }

    #[test]
    fn keyboard_unichar_treats_shifted_space_as_encoded_input() {
            assert_eq!(
                vterm_keyboard_unichar(
                    b' ' as u32,
                    crate::vterm_defs::VTERM_MOD_SHIFT,
                    Default::default(),
                    false,
                ),
                b" "
            );
            assert_eq!(
                vterm_keyboard_unichar(
                    b' ' as u32,
                    crate::vterm_defs::VTERM_MOD_SHIFT,
                    crate::vterm_defs::VTermKeyEncodingFlags {
                        disambiguate: true,
                        ..Default::default()
                    },
                    false,
                ),
                b"\x1b[32;2u"
            );
    }

    #[test]
    fn select_keycode_dispatches_ordinary_function_and_keypad_ranges() {
        assert_eq!(
            select_keycode(crate::vterm_defs::VTERM_KEY_UP, Default::default()),
            Some(KEYCODES[crate::vterm_defs::VTERM_KEY_UP as usize])
        );
        assert_eq!(
            select_keycode(
                crate::vterm_defs::vterm_key_function(12),
                Default::default(),
            ),
            Some(KEYCODES_FN[12])
        );
        assert_eq!(
            select_keycode(
                crate::vterm_defs::VTERM_KEY_KP_PLUS,
                Default::default(),
            ),
            Some(KEYCODES_KP[11])
        );
        assert_eq!(
            select_keycode(
                crate::vterm_defs::VTERM_KEY_KP_PLUS,
                crate::vterm_defs::VTermKeyEncodingFlags {
                    disambiguate: true,
                    ..Default::default()
                },
            ),
            Some(KEYCODES_KP_CSIU[11])
        );
    }

    #[test]
    fn select_keycode_rejects_unsupported_and_out_of_range_values() {
        for key in [
            -1,
            15,
            255,
            crate::vterm_defs::vterm_key_function(13),
            crate::vterm_defs::VTERM_KEY_MAX,
            i32::MAX,
        ] {
            assert_eq!(select_keycode(key, Default::default()), None);
        }
    }

    #[test]
    fn keyboard_key_encodes_tab_and_enter_modes() {
        let mode = crate::vterm_defs::VTermKeyboardMode::default();
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_TAB,
                0,
                Default::default(),
                mode,
            ),
            b"\t"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_TAB,
                crate::vterm_defs::VTERM_MOD_SHIFT,
                Default::default(),
                mode,
            ),
            b"\x1b[Z"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_TAB,
                crate::vterm_defs::VTERM_MOD_SHIFT | crate::vterm_defs::VTERM_MOD_CTRL,
                Default::default(),
                mode,
            ),
            b"\x1b[1;6Z"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_ENTER,
                0,
                Default::default(),
                mode,
            ),
            b"\r"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_ENTER,
                crate::vterm_defs::VTERM_MOD_ALT,
                Default::default(),
                crate::vterm_defs::VTermKeyboardMode {
                    newline: true,
                    ..Default::default()
                },
            ),
            b"\r\n"
        );
    }

    #[test]
    fn keyboard_key_encodes_cursor_function_and_numbered_keys() {
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_UP,
                0,
                Default::default(),
                Default::default(),
            ),
            b"\x1b[A"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_UP,
                0,
                Default::default(),
                crate::vterm_defs::VTermKeyboardMode {
                    cursor: true,
                    ..Default::default()
                },
            ),
            b"\x1bOA"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_UP,
                crate::vterm_defs::VTERM_MOD_CTRL,
                Default::default(),
                crate::vterm_defs::VTermKeyboardMode {
                    cursor: true,
                    ..Default::default()
                },
            ),
            b"\x1b[1;5A"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::vterm_key_function(1),
                0,
                Default::default(),
                Default::default(),
            ),
            b"\x1bOP"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::vterm_key_function(5),
                crate::vterm_defs::VTERM_MOD_SHIFT,
                Default::default(),
                Default::default(),
            ),
            b"\x1b[15;2~"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_INS,
                0,
                Default::default(),
                Default::default(),
            ),
            b"\x1b[2~"
        );
    }

    #[test]
    fn keyboard_key_encodes_keypad_and_csiu_modes() {
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_KP_2,
                0,
                Default::default(),
                Default::default(),
            ),
            b"2"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_KP_2,
                0,
                Default::default(),
                crate::vterm_defs::VTermKeyboardMode {
                    keypad: true,
                    ..Default::default()
                },
            ),
            b"\x1bOr"
        );
        let flags = crate::vterm_defs::VTermKeyEncodingFlags {
            disambiguate: true,
            ..Default::default()
        };
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_KP_2,
                0,
                flags,
                Default::default(),
            ),
            b"\x1b[57401;1u"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_TAB,
                0,
                flags,
                Default::default(),
            ),
            b"\t"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_TAB,
                crate::vterm_defs::VTERM_MOD_CTRL,
                flags,
                Default::default(),
            ),
            b"\x1b[9;5u"
        );
    }

    #[test]
    fn keyboard_key_encodes_alt_and_eight_bit_controls() {
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_ESCAPE,
                crate::vterm_defs::VTERM_MOD_ALT,
                Default::default(),
                Default::default(),
            ),
            b"\x1b\x1b"
        );
        assert_eq!(
            vterm_keyboard_key(
                crate::vterm_defs::VTERM_KEY_UP,
                0,
                Default::default(),
                crate::vterm_defs::VTermKeyboardMode {
                    ctrl8bit: true,
                    ..Default::default()
                },
            ),
            [crate::vterm_defs::C1_CSI, b'A']
        );
    }

    #[test]
    fn keyboard_key_ignores_none_and_unsupported_values() {
        for key in [
            crate::vterm_defs::VTERM_KEY_NONE,
            15,
            crate::vterm_defs::vterm_key_function(20),
            crate::vterm_defs::VTERM_KEY_MAX,
        ] {
            assert!(vterm_keyboard_key(
                key,
                0,
                Default::default(),
                Default::default(),
            )
            .is_empty());
        }
    }

    #[test]
    fn keyboard_start_paste_honors_bracketpaste_and_control_width() {
            assert!(vterm_keyboard_start_paste(Default::default()).is_empty());
            assert_eq!(
                vterm_keyboard_start_paste(crate::vterm_defs::VTermKeyboardMode {
                    bracketpaste: true,
                    ..Default::default()
                }),
                b"\x1b[200~"
            );
            assert_eq!(
                vterm_keyboard_start_paste(crate::vterm_defs::VTermKeyboardMode {
                    bracketpaste: true,
                    ctrl8bit: true,
                    ..Default::default()
                }),
                [vec![crate::vterm_defs::C1_CSI], b"200~".to_vec()].concat()
            );
    }

    #[test]
    fn keyboard_end_paste_honors_bracketpaste_and_control_width() {
            assert!(vterm_keyboard_end_paste(Default::default()).is_empty());
            assert_eq!(
                vterm_keyboard_end_paste(crate::vterm_defs::VTermKeyboardMode {
                    bracketpaste: true,
                    ..Default::default()
                }),
                b"\x1b[201~"
            );
            assert_eq!(
                vterm_keyboard_end_paste(crate::vterm_defs::VTermKeyboardMode {
                    bracketpaste: true,
                    ctrl8bit: true,
                    ..Default::default()
                }),
                [vec![crate::vterm_defs::C1_CSI], b"201~".to_vec()].concat()
            );
    }
}
