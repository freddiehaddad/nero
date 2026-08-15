//! Translated from `src/nvim/vterm/pen.c`.

/// RGB triple without `VTermColor` metadata (`VTermRGB`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermRgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// Current terminal pen (`VTermPen`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermPen {
    pub fg: crate::vterm_defs::VTermColor,
    pub bg: crate::vterm_defs::VTermColor,
    pub uri: i32,
    pub bold: bool,
    pub underline: u8,
    pub italic: bool,
    pub blink: bool,
    pub reverse: bool,
    pub conceal: bool,
    pub strike: bool,
    pub font: u8,
    pub small: bool,
    pub baseline: u8,
    pub dim: bool,
    pub overline: bool,
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

/// Pen and color fields owned by `VTermState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VTermPenState {
    pub pen: VTermPen,
    pub saved_pen: VTermPen,
    pub default_fg: crate::vterm_defs::VTermColor,
    pub default_bg: crate::vterm_defs::VTermColor,
    pub palette: VTermPalette,
}

impl Default for VTermPenState {
    fn default() -> Self {
        Self {
            pen: VTermPen::default(),
            saved_pen: VTermPen::default(),
            default_fg: crate::vterm_defs::VTermColor::default(),
            default_bg: crate::vterm_defs::VTermColor::default(),
            palette: VTermPalette {
                colors: [crate::vterm_defs::VTermColor::default(); 16],
            },
        }
    }
}

/// Updates the terminal's default colors
/// (`vterm_state_set_default_colors`).
pub fn vterm_state_set_default_colors(
    state: &mut VTermPenState,
    default_fg: Option<&crate::vterm_defs::VTermColor>,
    default_bg: Option<&crate::vterm_defs::VTermColor>,
) {
    if let Some(default_fg) = default_fg {
        state.default_fg = *default_fg;
        state.default_fg.color_type =
            (state.default_fg.color_type & !crate::vterm_defs::VTERM_COLOR_DEFAULT_MASK)
                | crate::vterm_defs::VTERM_COLOR_DEFAULT_FG;
    }
    if let Some(default_bg) = default_bg {
        state.default_bg = *default_bg;
        state.default_bg.color_type =
            (state.default_bg.color_type & !crate::vterm_defs::VTERM_COLOR_DEFAULT_MASK)
                | crate::vterm_defs::VTERM_COLOR_DEFAULT_BG;
    }
}

/// Initializes default foreground/background and ANSI colors
/// (`vterm_state_newpen`).
pub fn vterm_state_newpen(state: &mut VTermPenState) {
    let mut default_fg = crate::vterm_defs::VTermColor::default();
    crate::vterm_defs::vterm_color_rgb(&mut default_fg, 240, 240, 240);
    let mut default_bg = crate::vterm_defs::VTermColor::default();
    crate::vterm_defs::vterm_color_rgb(&mut default_bg, 0, 0, 0);
    vterm_state_set_default_colors(state, Some(&default_fg), Some(&default_bg));
    for (index, color) in state.palette.colors.iter_mut().enumerate() {
        lookup_default_colour_ansi(index as i64, color);
    }
}

/// Replaces one mutable ANSI palette entry
/// (`vterm_state_set_palette_color`).
pub fn vterm_state_set_palette_color(
    state: &mut VTermPenState,
    index: i32,
    color: &crate::vterm_defs::VTermColor,
) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    if let Some(slot) = state.palette.colors.get_mut(index) {
        *slot = *color;
    }
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

/// Resolves an xterm 256-color palette index
/// (`lookup_colour_palette`).
#[must_use]
pub fn lookup_colour_palette(
    palette: &VTermPalette,
    mut index: i64,
    color: &mut crate::vterm_defs::VTermColor,
) -> bool {
    if (0..16).contains(&index) {
        return lookup_colour_ansi(palette, index, color);
    }
    if (16..232).contains(&index) {
        index -= 16;
        let index = index as usize;
        crate::vterm_defs::vterm_color_rgb(
            color,
            RAMP6[index / 6 / 6 % 6],
            RAMP6[index / 6 % 6],
            RAMP6[index % 6],
        );
        return true;
    }
    if (232..256).contains(&index) {
        index -= 232;
        let value = RAMP24[index as usize];
        crate::vterm_defs::vterm_color_rgb(color, value, value, value);
        return true;
    }
    false
}

/// Parses an SGR color payload (`lookup_colour`).
#[must_use]
pub fn lookup_colour(
    palette: i32,
    args: &[crate::vterm::parser::CsiArg],
    color: &mut crate::vterm_defs::VTermColor,
) -> usize {
    match palette {
        2 => {
            if args.len() < 3 {
                return args.len();
            }
            crate::vterm_defs::vterm_color_rgb(
                color,
                crate::vterm::parser::csi_arg(args[0]) as u8,
                crate::vterm::parser::csi_arg(args[1]) as u8,
                crate::vterm::parser::csi_arg(args[2]) as u8,
            );
            3
        }
        5 => {
            let Some(&index) = args.first() else {
                return 0;
            };
            if crate::vterm::parser::csi_arg_is_missing(index) {
                return 1;
            }
            crate::vterm_defs::vterm_color_indexed(color, index as u8);
            1
        }
        _ => 0,
    }
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
    fn terminal_pen_defaults_to_zeroed_c_state() {
        let pen = VTermPen::default();
        assert_eq!(pen.fg, crate::vterm_defs::VTermColor::default());
        assert_eq!(pen.bg, crate::vterm_defs::VTermColor::default());
        assert_eq!(pen.uri, 0);
        assert!(!pen.bold);
        assert_eq!(pen.underline, 0);
        assert!(!pen.italic);
        assert!(!pen.blink);
        assert!(!pen.reverse);
        assert!(!pen.conceal);
        assert!(!pen.strike);
        assert_eq!(pen.font, 0);
        assert!(!pen.small);
        assert_eq!(pen.baseline, 0);
        assert!(!pen.dim);
        assert!(!pen.overline);
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
    fn pen_state_defaults_to_zeroed_allocator_storage() {
                let state = VTermPenState::default();
                assert_eq!(state.pen, VTermPen::default());
                assert_eq!(state.saved_pen, VTermPen::default());
                assert_eq!(
                    state.default_fg,
                    crate::vterm_defs::VTermColor::default()
                );
                assert_eq!(
                    state.default_bg,
                    crate::vterm_defs::VTermColor::default()
                );
                assert!(
                    state
                        .palette
                        .colors
                        .iter()
                        .all(|color| *color == crate::vterm_defs::VTermColor::default())
                );
    }

    #[test]
    fn set_default_colors_replaces_metadata_with_the_matching_default_flag() {
        let mut state = VTermPenState::default();
        let fg = crate::vterm_defs::VTermColor {
            color_type: crate::vterm_defs::VTERM_COLOR_INDEXED
                | crate::vterm_defs::VTERM_COLOR_DEFAULT_BG,
            index: 7,
            ..Default::default()
        };
        let bg = crate::vterm_defs::VTermColor {
            color_type: crate::vterm_defs::VTERM_COLOR_RGB
                | crate::vterm_defs::VTERM_COLOR_DEFAULT_FG,
            red: 1,
            green: 2,
            blue: 3,
            ..Default::default()
        };
        vterm_state_set_default_colors(&mut state, Some(&fg), Some(&bg));
        assert!(state.default_fg.is_indexed());
        assert!(state.default_fg.is_default_fg());
        assert!(!state.default_fg.is_default_bg());
        assert_eq!(state.default_fg.index, 7);
        assert!(state.default_bg.is_rgb());
        assert!(!state.default_bg.is_default_fg());
        assert!(state.default_bg.is_default_bg());
        assert_eq!(
            (
                state.default_bg.red,
                state.default_bg.green,
                state.default_bg.blue,
            ),
            (1, 2, 3)
        );
    }

    #[test]
    fn set_default_colors_leaves_none_side_unchanged() {
        let mut state = VTermPenState::default();
        state.default_fg.red = 9;
        state.default_bg.blue = 8;
        let original = state.clone();
        vterm_state_set_default_colors(&mut state, None, None);
        assert_eq!(state, original);
    }

    #[test]
    fn newpen_initializes_defaults_and_all_ansi_colors() {
        let mut state = VTermPenState::default();
        vterm_state_newpen(&mut state);
        assert_eq!(
            (
                state.default_fg.red,
                state.default_fg.green,
                state.default_fg.blue,
            ),
            (240, 240, 240)
        );
        assert!(state.default_fg.is_default_fg());
        assert_eq!(
            (
                state.default_bg.red,
                state.default_bg.green,
                state.default_bg.blue,
            ),
            (0, 0, 0)
        );
        assert!(state.default_bg.is_default_bg());
        for (color, expected) in state.palette.colors.iter().zip(ANSI_COLORS) {
            assert_eq!(
                (color.red, color.green, color.blue),
                (expected.red, expected.green, expected.blue)
            );
            assert_eq!(color.color_type, crate::vterm_defs::VTERM_COLOR_RGB);
        }
    }

    #[test]
    fn newpen_does_not_modify_current_or_saved_pen() {
        let mut state = VTermPenState::default();
        state.pen.bold = true;
        state.saved_pen.italic = true;
        vterm_state_newpen(&mut state);
        assert!(state.pen.bold);
        assert!(state.saved_pen.italic);
    }

    #[test]
    fn set_palette_color_replaces_valid_entries() {
        let mut state = VTermPenState::default();
        vterm_state_newpen(&mut state);
        let replacement = crate::vterm_defs::VTermColor {
            color_type: crate::vterm_defs::VTERM_COLOR_INDEXED
                | crate::vterm_defs::VTERM_COLOR_DEFAULT_FG,
            index: 99,
            ..Default::default()
        };
        vterm_state_set_palette_color(&mut state, 7, &replacement);
        assert_eq!(state.palette.colors[7], replacement);
    }

    #[test]
    fn set_palette_color_ignores_out_of_range_indices() {
        let mut state = VTermPenState::default();
        vterm_state_newpen(&mut state);
        let original = state.palette.clone();
        let replacement = crate::vterm_defs::VTermColor {
            red: 1,
            green: 2,
            blue: 3,
            ..Default::default()
        };
        for index in [-1, 16, i32::MAX] {
            vterm_state_set_palette_color(&mut state, index, &replacement);
        }
        assert_eq!(state.palette, original);
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

    #[test]
    fn palette_lookup_uses_mutable_ansi_entries() {
            let mut palette = VTermPalette::default();
            crate::vterm_defs::vterm_color_rgb(&mut palette.colors[5], 1, 2, 3);
            let mut color = crate::vterm_defs::VTermColor::default();
            assert!(lookup_colour_palette(&palette, 5, &mut color));
            assert_eq!((color.red, color.green, color.blue), (1, 2, 3));
        }

    #[test]
    fn palette_lookup_maps_color_cube_boundaries() {
            let palette = VTermPalette::default();
            let mut color = crate::vterm_defs::VTermColor::default();
            for (index, expected) in [
                (16, (0x00, 0x00, 0x00)),
                (17, (0x00, 0x00, 0x33)),
                (21, (0x00, 0x00, 0xFF)),
                (22, (0x00, 0x33, 0x00)),
                (51, (0x00, 0xFF, 0xFF)),
                (196, (0xFF, 0x00, 0x00)),
                (231, (0xFF, 0xFF, 0xFF)),
            ] {
                assert!(lookup_colour_palette(&palette, index, &mut color));
                assert_eq!(
                    (color.red, color.green, color.blue),
                    expected,
                    "index {index}"
                );
            }
        }

    #[test]
    fn palette_lookup_maps_grayscale_and_rejects_invalid_indices() {
            let palette = VTermPalette::default();
            let mut color = crate::vterm_defs::VTermColor::default();
            for (index, expected) in [(232, 0x00), (233, 0x0B), (254, 0xF3), (255, 0xFF)] {
                assert!(lookup_colour_palette(&palette, index, &mut color));
                assert_eq!(
                    (color.red, color.green, color.blue),
                    (expected, expected, expected)
                );
            }
            let original = color;
            for index in [-1, 256, i64::MAX] {
                assert!(!lookup_colour_palette(&palette, index, &mut color));
                assert_eq!(color, original);
            }
    }

    #[test]
    fn lookup_colour_parses_rgb_payloads_and_partial_input() {
            let mut color = crate::vterm_defs::VTermColor::default();
            assert_eq!(lookup_colour(2, &[10, 20], &mut color), 2);
            assert_eq!(color, crate::vterm_defs::VTermColor::default());
            assert_eq!(
                lookup_colour(
                    2,
                    &[
                        crate::vterm::parser::CSI_ARG_FLAG_MORE | 10,
                        crate::vterm::parser::CSI_ARG_FLAG_MORE | 20,
                        300,
                    ],
                    &mut color,
                ),
                3
            );
            assert_eq!((color.red, color.green, color.blue), (10, 20, 44));
        }

    #[test]
    fn lookup_colour_parses_indexed_payloads_and_missing_input() {
            let mut color = crate::vterm_defs::VTermColor::default();
            assert_eq!(lookup_colour(5, &[], &mut color), 0);
            assert_eq!(
                lookup_colour(
                    5,
                    &[crate::vterm::parser::CSI_ARG_MISSING],
                    &mut color,
                ),
                1
            );
            assert_eq!(color, crate::vterm_defs::VTermColor::default());
            assert_eq!(lookup_colour(5, &[300], &mut color), 1);
            assert!(color.is_indexed());
            assert_eq!(color.index, 44);
        }

    #[test]
    fn lookup_colour_rejects_unknown_palette_modes() {
            let original = crate::vterm_defs::VTermColor {
                red: 1,
                green: 2,
                blue: 3,
                ..Default::default()
            };
            let mut color = original;
            assert_eq!(lookup_colour(7, &[1, 2, 3], &mut color), 0);
            assert_eq!(color, original);
    }
}
