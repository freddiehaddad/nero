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
}
