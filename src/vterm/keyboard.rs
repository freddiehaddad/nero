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
}
