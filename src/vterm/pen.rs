//! Translated from `src/nvim/vterm/pen.c`.

/// RGB triple without `VTermColor` metadata (`VTermRGB`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermRgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[allow(dead_code)]
const ANSI_COLORS: [VTermRgb; 16] = [
    VTermRgb { red: 0, green: 0, blue: 0 },
    VTermRgb { red: 224, green: 0, blue: 0 },
    VTermRgb { red: 0, green: 224, blue: 0 },
    VTermRgb { red: 224, green: 224, blue: 0 },
    VTermRgb { red: 0, green: 0, blue: 224 },
    VTermRgb { red: 224, green: 0, blue: 224 },
    VTermRgb { red: 0, green: 224, blue: 224 },
    VTermRgb { red: 224, green: 224, blue: 224 },
    VTermRgb { red: 128, green: 128, blue: 128 },
    VTermRgb { red: 255, green: 64, blue: 64 },
    VTermRgb { red: 64, green: 255, blue: 64 },
    VTermRgb { red: 255, green: 255, blue: 64 },
    VTermRgb { red: 64, green: 64, blue: 255 },
    VTermRgb { red: 255, green: 64, blue: 255 },
    VTermRgb { red: 64, green: 255, blue: 255 },
    VTermRgb { red: 255, green: 255, blue: 255 },
];

#[allow(dead_code)]
const RAMP6: [u8; 6] = [0x00, 0x33, 0x66, 0x99, 0xCC, 0xFF];

#[allow(dead_code)]
const RAMP24: [u8; 24] = [
    0x00, 0x0B, 0x16, 0x21, 0x2C, 0x37, 0x42, 0x4D, 0x58, 0x63, 0x6E, 0x79,
    0x85, 0x90, 0x9B, 0xA6, 0xB1, 0xBC, 0xC7, 0xD2, 0xDD, 0xE8, 0xF3, 0xFF,
];

/// Looks up one of the fixed ANSI colors
/// (`lookup_default_colour_ansi`).
pub fn lookup_default_colour_ansi(
    index: i64,
    color: &mut crate::vterm_defs::VTermColor,
) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    let Some(rgb) = ANSI_COLORS.get(index) else {
        return;
    };
    crate::vterm_defs::vterm_color_rgb(color, rgb.red, rgb.green, rgb.blue);
}

/// The sixteen mutable ANSI palette entries from `VTermState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VTermPalette {
    pub colors: [crate::vterm_defs::VTermColor; 16],
}

impl Default for VTermPalette {
    fn default() -> Self {
        let mut colors = [crate::vterm_defs::VTermColor::default(); 16];
        for (index, color) in colors.iter_mut().enumerate() {
            lookup_default_colour_ansi(index as i64, color);
        }
        Self { colors }
    }
}

/// Looks up a mutable ANSI palette entry (`lookup_colour_ansi`).
#[must_use]
pub fn lookup_colour_ansi(
    palette: &VTermPalette,
    index: i64,
    color: &mut crate::vterm_defs::VTermColor,
) -> bool {
    let Ok(index) = usize::try_from(index) else {
        return false;
    };
    let Some(found) = palette.colors.get(index) else {
        return false;
    };
    *color = *found;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_color_table_matches_pen_c() {
        assert_eq!(ANSI_COLORS.len(), 16);
        assert_eq!(ANSI_COLORS[0], VTermRgb { red: 0, green: 0, blue: 0 });
        assert_eq!(ANSI_COLORS[7], VTermRgb { red: 224, green: 224, blue: 224 });
        assert_eq!(ANSI_COLORS[8], VTermRgb { red: 128, green: 128, blue: 128 });
        assert_eq!(ANSI_COLORS[15], VTermRgb { red: 255, green: 255, blue: 255 });
    }

    #[test]
    fn color_ramps_match_pen_c() {
        assert_eq!(RAMP6, [0x00, 0x33, 0x66, 0x99, 0xCC, 0xFF]);
        assert_eq!(RAMP24.first(), Some(&0x00));
        assert_eq!(RAMP24.last(), Some(&0xFF));
        assert_eq!(RAMP24.len(), 24);
        assert!(RAMP24.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn default_ansi_lookup_maps_all_sixteen_colors() {
        for (index, expected) in ANSI_COLORS.iter().enumerate() {
            let mut color = crate::vterm_defs::VTermColor::default();
            lookup_default_colour_ansi(index as i64, &mut color);
            assert!(color.is_rgb());
            assert_eq!(
                (color.red, color.green, color.blue),
                (expected.red, expected.green, expected.blue)
            );
        }
    }

    #[test]
    fn default_ansi_lookup_leaves_invalid_indices_unchanged() {
        let original = crate::vterm_defs::VTermColor {
            color_type: crate::vterm_defs::VTERM_COLOR_INDEXED,
            index: 42,
            ..Default::default()
        };
        for index in [-1, 16, i64::MAX] {
            let mut color = original;
            lookup_default_colour_ansi(index, &mut color);
            assert_eq!(color, original);
        }
    }

    #[test]
    fn terminal_palette_defaults_to_the_ansi_table() {
            let palette = VTermPalette::default();
            for (color, expected) in palette.colors.iter().zip(ANSI_COLORS) {
                assert_eq!(
                    (color.red, color.green, color.blue),
                    (expected.red, expected.green, expected.blue)
                );
            }
        }

    #[test]
    fn ansi_palette_lookup_copies_valid_entries_only() {
            let mut palette = VTermPalette::default();
            crate::vterm_defs::vterm_color_indexed(&mut palette.colors[3], 99);
            let mut color = crate::vterm_defs::VTermColor::default();
            assert!(lookup_colour_ansi(&palette, 3, &mut color));
            assert_eq!(color, palette.colors[3]);

            for index in [-1, 16] {
                let original = color;
                assert!(!lookup_colour_ansi(&palette, index, &mut color));
                assert_eq!(color, original);
        }
    }
}
