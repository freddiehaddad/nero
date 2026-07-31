//! Translated from `src/nvim/highlight.c` (tractable core only).
//!
//! `highlight.c` is the highlight-attribute-table/color-blending file
//! (thousands of lines - needs the whole highlight-group registry,
//! attribute-table allocation, and UI-attribute dispatch machinery,
//! not attempted here). Translated: [`hl_cterm2rgb_color`]/
//! [`hl_rgb2cterm_color`] (the 8-bit terminal-color <-> packed-RGB
//! conversions, pure table lookups/bit arithmetic with no external
//! dependencies) and [`rgb_blend`]/[`cterm_blend`] (blend two RGB/
//! terminal colors by a percentage ratio) - a self-contained group of
//! 4 pure color-math functions, harvested together as this file's
//! first translated content, ahead of their real caller
//! (`hl_blend_attrs`, needing the full `HlAttrs`-table/highlight-group
//! machinery, not yet translated), matching this crate's established
//! "translate ahead of a real caller" precedent for small,
//! self-contained pieces with no design freedom of their own.
//!
//! Also translated: [`hl_combine_ae`] (combine two attribute-flag
//! bitmasks, e.g. for spelling combined with syntax highlighting - the
//! underline-kind bits in `prim_ae` overrule `char_ae`'s, every other
//! bit is a plain bitwise OR), via already-real
//! `crate::highlight_defs::HL_UNDERLINE_MASK`. Translated ahead of its
//! real caller (`hl_combine_attr`, needing the `combine_attr_entries`
//! hashmap and `syn_attr2entry`'s own `attr_entries` table, neither
//! translated), matching the same precedent.
//!
//! Deferred: everything else in the file.

/// Convert an 8-bit terminal color number (0-255) to a packed RGB
/// value, compatible with xterm's own color cube/greyscale-ramp
/// layout (`hl_cterm2rgb_color`).
#[must_use]
pub fn hl_cterm2rgb_color(nr: i32) -> i32 {
    const CUBE_VALUE: [i32; 6] = [0x00, 0x5F, 0x87, 0xAF, 0xD7, 0xFF];
    const GREY_RAMP: [i32; 24] = [
        0x08, 0x12, 0x1C, 0x26, 0x30, 0x3A, 0x44, 0x4E, 0x58, 0x62, 0x6C, 0x76, 0x80, 0x8A, 0x94,
        0x9E, 0xA8, 0xB2, 0xBC, 0xC6, 0xD0, 0xDA, 0xE4, 0xEE,
    ];
    const ANSI_TABLE: [[i32; 3]; 16] = [
        [0, 0, 0],
        [224, 0, 0],
        [0, 224, 0],
        [224, 224, 0],
        [0, 0, 224],
        [224, 0, 224],
        [0, 224, 224],
        [224, 224, 224],
        [128, 128, 128],
        [255, 64, 64],
        [64, 255, 64],
        [255, 255, 64],
        [64, 64, 255],
        [255, 64, 255],
        [64, 255, 255],
        [255, 255, 255],
    ];

    let (mut r, mut g, mut b) = (0, 0, 0);
    if nr < 16 {
        if let Some(row) = ANSI_TABLE.get(nr as usize) {
            [r, g, b] = *row;
        }
    } else if nr < 232 {
        // 216 color-cube
        let idx = (nr - 16) as usize;
        r = CUBE_VALUE[idx / 36 % 6];
        g = CUBE_VALUE[idx / 6 % 6];
        b = CUBE_VALUE[idx % 6];
    } else if nr < 256 {
        // 24 greyscale ramp
        let idx = (nr - 232) as usize;
        r = GREY_RAMP[idx];
        g = GREY_RAMP[idx];
        b = GREY_RAMP[idx];
    }
    (r << 16) + (g << 8) + b
}

/// Convert a packed RGB color to an 8-bit terminal color number
/// (0-255) (`hl_rgb2cterm_color`).
#[must_use]
pub fn hl_rgb2cterm_color(rgb: i32) -> i32 {
    let r = (rgb & 0xFF0000) >> 16;
    let g = (rgb & 0x00FF00) >> 8;
    let b = rgb & 0x0000FF;

    (r * 6 / 256) * 36 + (g * 6 / 256) * 6 + (b * 6 / 256)
}

/// Blend two packed RGB colors by `ratio` percent of `rgb1` (and
/// `100 - ratio` percent of `rgb2`) (`rgb_blend`).
#[must_use]
pub fn rgb_blend(ratio: i32, rgb1: i32, rgb2: i32) -> i32 {
    let a = ratio;
    let b = 100 - ratio;
    let r1 = (rgb1 & 0xFF0000) >> 16;
    let g1 = (rgb1 & 0x00FF00) >> 8;
    let b1 = rgb1 & 0x0000FF;
    let r2 = (rgb2 & 0xFF0000) >> 16;
    let g2 = (rgb2 & 0x00FF00) >> 8;
    let b2 = rgb2 & 0x0000FF;
    let mr = (a * r1 + b * r2) / 100;
    let mg = (a * g1 + b * g2) / 100;
    let mb = (a * b1 + b * b2) / 100;
    (mr << 16) + (mg << 8) + mb
}

/// Blend two 8-bit terminal colors by `ratio` percent of `c1` (and
/// `100 - ratio` percent of `c2`), via their RGB conversions
/// (`cterm_blend`).
#[must_use]
pub fn cterm_blend(ratio: i32, c1: i16, c2: i16) -> i32 {
    // 1. Convert cterm color numbers to RGB.
    // 2. Blend the RGB colors.
    // 3. Convert the RGB result to a cterm color.
    let rgb1 = hl_cterm2rgb_color(i32::from(c1));
    let rgb2 = hl_cterm2rgb_color(i32::from(c2));
    let rgb_blended = rgb_blend(ratio, rgb1, rgb2);
    hl_rgb2cterm_color(rgb_blended)
}

/// Combine two [`crate::highlight_defs`] attribute-flag bitmasks
/// (e.g. for spelling combined with syntax highlighting). The
/// underline-kind bits (`HL_UNDERLINE_MASK`) in `prim_ae` overrule the
/// ones in `char_ae` if both are present; every other bit is a plain
/// bitwise OR of both masks (`hl_combine_ae`).
#[must_use]
pub fn hl_combine_ae(char_ae: i32, prim_ae: i32) -> i32 {
    let char_ul = char_ae & (crate::highlight_defs::HL_UNDERLINE_MASK as i32);
    let prim_ul = prim_ae & (crate::highlight_defs::HL_UNDERLINE_MASK as i32);
    let new_ul = if prim_ul != 0 { prim_ul } else { char_ul };
    (char_ae & !(crate::highlight_defs::HL_UNDERLINE_MASK as i32))
        | (prim_ae & !(crate::highlight_defs::HL_UNDERLINE_MASK as i32))
        | new_ul
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- hl_cterm2rgb_color ----

    #[test]
    fn hl_cterm2rgb_color_ansi_black_is_zero() {
        assert_eq!(hl_cterm2rgb_color(0), 0x000000);
    }

    #[test]
    fn hl_cterm2rgb_color_ansi_dark_red() {
        assert_eq!(hl_cterm2rgb_color(1), 0xE00000);
    }

    #[test]
    fn hl_cterm2rgb_color_ansi_white_is_last_ansi_entry() {
        assert_eq!(hl_cterm2rgb_color(15), 0xFFFFFF);
    }

    #[test]
    fn hl_cterm2rgb_color_color_cube_first_entry() {
        // nr=16 -> idx=0 -> cube_value[0]=0x00 for all 3 channels.
        assert_eq!(hl_cterm2rgb_color(16), 0x000000);
    }

    #[test]
    fn hl_cterm2rgb_color_color_cube_matches_hand_computed_index() {
        // nr=231 -> idx=215 -> r=idx/36%6=5, g=idx/6%6=5, b=idx%6=5
        // -> cube_value[5]=0xFF for all 3 channels.
        assert_eq!(hl_cterm2rgb_color(231), 0xFFFFFF);
    }

    #[test]
    fn hl_cterm2rgb_color_greyscale_ramp_first_and_last() {
        // nr=232 -> idx=0 -> grey_ramp[0]=0x08.
        assert_eq!(hl_cterm2rgb_color(232), 0x080808);
        // nr=255 -> idx=23 -> grey_ramp[23]=0xEE.
        assert_eq!(hl_cterm2rgb_color(255), 0xEEEEEE);
    }

    #[test]
    fn hl_cterm2rgb_color_out_of_range_is_black() {
        // nr >= 256 falls through every branch, leaving r=g=b=0.
        assert_eq!(hl_cterm2rgb_color(256), 0x000000);
    }

    // ---- hl_rgb2cterm_color ----

    #[test]
    fn hl_rgb2cterm_color_black_is_zero() {
        assert_eq!(hl_rgb2cterm_color(0x000000), 0);
    }

    #[test]
    fn hl_rgb2cterm_color_white_is_the_max_bucket_sum() {
        // r=g=b=255 -> (255*6/256)=5 for each -> 5*36 + 5*6 + 5 = 215.
        assert_eq!(hl_rgb2cterm_color(0xFFFFFF), 215);
    }

    #[test]
    fn hl_rgb2cterm_color_isolates_each_channel() {
        // Pure red: only the r*36 term contributes.
        assert_eq!(hl_rgb2cterm_color(0xFF0000), 5 * 36);
        // Pure green: only the g*6 term contributes.
        assert_eq!(hl_rgb2cterm_color(0x00FF00), 5 * 6);
        // Pure blue: only the b term contributes.
        assert_eq!(hl_rgb2cterm_color(0x0000FF), 5);
    }

    // ---- rgb_blend ----

    #[test]
    fn rgb_blend_ratio_100_is_pure_rgb1() {
        assert_eq!(rgb_blend(100, 0xFF0000, 0x00FF00), 0xFF0000);
    }

    #[test]
    fn rgb_blend_ratio_0_is_pure_rgb2() {
        assert_eq!(rgb_blend(0, 0xFF0000, 0x00FF00), 0x00FF00);
    }

    #[test]
    fn rgb_blend_ratio_50_averages_each_channel() {
        // r: (50*255 + 50*0)/100 = 127 (integer division).
        // g: (50*0 + 50*255)/100 = 127.
        assert_eq!(rgb_blend(50, 0xFF0000, 0x00FF00), (127 << 16) + (127 << 8));
    }

    // ---- cterm_blend ----

    #[test]
    fn cterm_blend_ratio_100_uses_only_c1() {
        // ratio=100 means rgb_blend takes rgb1 entirely (a=100, b=0),
        // so this equals hl_rgb2cterm_color(hl_cterm2rgb_color(16))
        // directly - NOT necessarily 16 itself, since the cube-index
        // <-> RGB mapping is lossy in general (16's RGB is 0x000000,
        // which rgb2cterm maps back to index 0, not 16).
        assert_eq!(cterm_blend(100, 16, 231), 0);
    }

    #[test]
    fn cterm_blend_ratio_0_uses_only_c2() {
        // Same reasoning as ratio_100 above, but for c2: 231's RGB is
        // 0xFFFFFF, which rgb2cterm maps back to 215, not 231.
        assert_eq!(cterm_blend(0, 16, 231), 215);
    }

    #[test]
    fn cterm_blend_matches_manual_rgb_blend_and_convert() {
        let expected = hl_rgb2cterm_color(rgb_blend(
            50,
            hl_cterm2rgb_color(16),
            hl_cterm2rgb_color(231),
        ));
        assert_eq!(cterm_blend(50, 16, 231), expected);
    }

    // ---- hl_combine_ae ----

    #[test]
    fn hl_combine_ae_prim_underline_overrules_char_underline() {
        use crate::highlight_defs::{HL_BOLD, HL_ITALIC, HL_UNDERCURL, HL_UNDERLINE};
        let char_ae = (HL_BOLD | HL_UNDERLINE) as i32;
        let prim_ae = (HL_ITALIC | HL_UNDERCURL) as i32;
        // Non-underline bits OR together (HL_BOLD | HL_ITALIC), and
        // the underline-kind bits come from prim_ae (HL_UNDERCURL),
        // NOT char_ae's own HL_UNDERLINE.
        assert_eq!(hl_combine_ae(char_ae, prim_ae), 0x16);
    }

    #[test]
    fn hl_combine_ae_keeps_char_underline_when_prim_has_none() {
        use crate::highlight_defs::{HL_BOLD, HL_ITALIC, HL_UNDERLINE};
        let char_ae = (HL_BOLD | HL_UNDERLINE) as i32;
        let prim_ae = HL_ITALIC as i32;
        assert_eq!(hl_combine_ae(char_ae, prim_ae), 0x0E);
    }

    #[test]
    fn hl_combine_ae_both_zero_is_zero() {
        assert_eq!(hl_combine_ae(0, 0), 0);
    }
}
