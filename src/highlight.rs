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

/// Interned font names, indexed by the value `hl_add_font_idx`
/// returns (`fonts`).
///
/// The original is a hash set whose slot index doubles as the font's
/// stable identity. A `Vec` gives the same guarantee - an index, once
/// handed out, keeps naming the same font - without the separate key
/// array, since the entry IS the name.
static FONTS: crate::globals::GlobalCell<Vec<Vec<u8>>> =
    crate::globals::GlobalCell::new(Vec::new());
/// Interned URLs, indexed by `HlAttrs.url` (`urls`).
static URLS: crate::globals::GlobalCell<Vec<Vec<u8>>> =
    crate::globals::GlobalCell::new(Vec::new());

static HLSTATE_ACTIVE: crate::globals::GlobalCell<bool> =
    crate::globals::GlobalCell::new(false);

pub(crate) static ATTR_ENTRIES: std::sync::LazyLock<
    crate::globals::GlobalCell<crate::map::Set<crate::highlight_defs::HlEntry>>,
> = std::sync::LazyLock::new(|| {
    crate::globals::GlobalCell::new(crate::map::Set::new())
});

/// Initialize the highlight attribute table (`highlight_init`).
///
/// The table's zero entry is the no-attribute sentinel.
///
/// # Safety
/// Mutates the shared attribute table.
pub unsafe fn highlight_init() {
    let entries = unsafe { ATTR_ENTRIES.get_mut() };
    if entries.is_empty() {
        let _ = entries.put(crate::highlight_defs::HlEntry {
            attr: Default::default(),
            kind: crate::highlight_defs::HlKind::Invalid,
            id1: 0,
            id2: 0,
            winid: 0,
        });
    }
}

/// Intern one highlight attribute entry (`get_attr_entry`).
///
/// The remote-UI `hl_attr_define` notification is omitted with the UI
/// dispatch layer; stable IDs and deduplication are complete.
///
/// # Safety
/// Mutates the shared attribute table.
fn get_attr_entry(mut entry: crate::highlight_defs::HlEntry) -> i32 {
    if !unsafe { *HLSTATE_ACTIVE.get_mut() } {
        entry.kind = crate::highlight_defs::HlKind::Unknown;
        entry.id1 = 0;
        entry.id2 = 0;
    }
    unsafe { highlight_init() };
    let entries = unsafe { ATTR_ENTRIES.get_mut() };
    if entries.len() > crate::vim_defs::MAX_TYPENR as usize {
        return 0;
    }
    entries.put(entry).0 as i32
}

/// Get the attribute ID for a syntax group (`hl_get_syn_attr`).
///
/// # Safety
/// Mutates the shared attribute table when a new combination is seen.
#[must_use]
pub unsafe fn hl_get_syn_attr(
    ns_id: i32,
    idx: i32,
    attrs: crate::highlight_defs::HlAttrs,
) -> i32 {
    if attrs.cterm_fg_color != 0
        || attrs.cterm_bg_color != 0
        || attrs.rgb_fg_color != -1
        || attrs.rgb_bg_color != -1
        || attrs.rgb_sp_color != -1
        || attrs.cterm_ae_attr != 0
        || attrs.rgb_ae_attr != 0
        || attrs.font >= 0
        || ns_id != 0
    {
        get_attr_entry(crate::highlight_defs::HlEntry {
            attr: attrs,
            kind: crate::highlight_defs::HlKind::Syntax,
            id1: idx,
            id2: ns_id,
            winid: 0,
        })
    } else {
        0
    }
}

/// Get an interned URL by index (`hl_get_url`).
///
/// # Safety
/// Reads the `URLS` file-static.
#[must_use]
pub unsafe fn hl_get_url(index: u32) -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { URLS.get_mut() }.get(index as usize).cloned()
}

/// Intern `font_name` and return its index (`hl_add_font_idx`).
///
/// Adding a name that is already interned returns its existing index
/// rather than a new one, which is what makes the index a stable
/// identity.
///
/// @return the font index, or `-1` for an empty name.
///
/// # Safety
/// Mutates the `FONTS` file-static.
pub unsafe fn hl_add_font_idx(font_name: &[u8]) -> i32 {
    if font_name.is_empty() || font_name.first() == Some(&0) {
        return -1;
    }
    // The original's NUL-terminated name stops at the first NUL.
    let name = &font_name[..font_name.iter().position(|&c| c == 0).unwrap_or(font_name.len())];

    // SAFETY: forwarded from this function's own safety doc.
    let fonts = unsafe { FONTS.get_mut() };
    if let Some(i) = fonts.iter().position(|f| f == name) {
        return i32::try_from(i).unwrap_or(-1);
    }
    fonts.push(name.to_vec());
    i32::try_from(fonts.len() - 1).unwrap_or(-1)
}

/// The font name at `index` (`hl_get_font`).
///
/// `None` for an index that names no font.
///
/// # Safety
/// Reads the `FONTS` file-static.
#[must_use]
pub unsafe fn hl_get_font(index: i32) -> Option<Vec<u8>> {
    if index < 0 {
        return None;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { FONTS.get_mut() }.get(index as usize).cloned()
}

/// Global highlight namespace (`ns_hl_global`).
pub static NS_HL_GLOBAL: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);
/// Highlight namespace for the current window (`ns_hl_win`).
pub static NS_HL_WIN: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(-1);
/// Highlight namespace specified in a fast callback (`ns_hl_fast`).
///
/// Negative means no fast-callback namespace is in effect, which is
/// what lets per-window highlight overrides apply.
pub static NS_HL_FAST: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(-1);

/// Namespace highlight definitions (`ns_hls`).
pub static NS_HLS: std::sync::LazyLock<
    crate::globals::GlobalCell<
        crate::map::Map<
            crate::highlight_defs::ColorKey,
            crate::highlight_defs::ColorItem,
        >,
    >,
> = std::sync::LazyLock::new(|| {
    crate::globals::GlobalCell::new(crate::map::Map::new())
});

/// Define one namespace highlight (`ns_hl_def`).
///
/// Namespace-zero definitions remain with the global `:highlight`
/// table's `set_hl_group`; nonzero link and direct-attribute
/// definitions are complete.
///
/// # Safety
/// Mutates the namespace-highlight and decoration-provider registries.
pub unsafe fn ns_hl_def(
    ns_id: i32,
    hl_id: i32,
    attrs: crate::highlight_defs::HlAttrs,
    link_id: i32,
) {
    if ns_id == 0 {
        unimplemented!("ns_hl_def: namespace zero needs set_hl_group");
    }
    let key = crate::highlight_defs::ColorKey::new(ns_id, hl_id);
    if attrs.rgb_ae_attr & crate::highlight_defs::HL_DEFAULT as i32 != 0
        && unsafe { NS_HLS.get_mut() }.contains_key(&key)
    {
        return;
    }

    let provider = unsafe {
        crate::decoration_provider::get_decor_provider(ns_id, true)
    };
    let attr_id = if link_id > 0 {
        -1
    } else {
        unsafe { hl_get_syn_attr(ns_id, hl_id, attrs) }
    };
    let item = crate::highlight_defs::ColorItem {
        attr_id,
        link_id,
        version: unsafe { (*provider).hl_valid },
        is_default: attrs.rgb_ae_attr & crate::highlight_defs::HL_DEFAULT as i32 != 0,
        link_global: attrs.rgb_ae_attr & crate::highlight_defs::HL_GLOBAL as i32 != 0,
    };
    unsafe { NS_HLS.get_mut() }.insert(key, item);
    unsafe { (*provider).hl_cached = false };
}

/// The currently active UI highlight attribute for each `HlfT`
/// (`hl_attr_active`).
pub static HL_ATTR_ACTIVE: crate::globals::GlobalCell<
    [i32; crate::highlight_defs::HlfT::Count as usize],
> = crate::globals::GlobalCell::new([0; crate::highlight_defs::HlfT::Count as usize]);

/// The background highlight attribute for window `wp`
/// (`win_bg_attr`).
///
/// A window-local override wins, but only when no fast-callback
/// highlight namespace is in effect. Otherwise the global `Normal`
/// attribute applies, except that a non-current window uses
/// `NormalNC` when one is defined.
///
/// # Safety
/// `wp` must point at a live `WinT`, and reads `GLOBALS` plus the
/// `NS_HL_FAST`/`HL_ATTR_ACTIVE` file-statics.
#[must_use]
pub unsafe fn win_bg_attr(wp: *const crate::buffer_defs::WinT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    let is_curwin = std::ptr::eq(wp, curwin);

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *NS_HL_FAST.get_mut() } < 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let local = unsafe {
            if is_curwin { (*wp).w_hl_attr_normal } else { (*wp).w_hl_attr_normalnc }
        };
        if local != 0 {
            return local;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    let active = unsafe { HL_ATTR_ACTIVE.get_mut() };
    let inactive = active[crate::highlight_defs::HlfT::Inactive as usize];
    if is_curwin || inactive == 0 {
        active[crate::highlight_defs::HlfT::None as usize]
    } else {
        inactive
    }
}

/// Cterm color of the `Normal` highlight group's foreground
/// (`cterm_normal_fg_color`).
pub static CTERM_NORMAL_FG_COLOR: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
/// Cterm color of the `Normal` highlight group's background
/// (`cterm_normal_bg_color`).
pub static CTERM_NORMAL_BG_COLOR: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);

/// RGB foreground of the `Normal` highlight group (`normal_fg`).
///
/// `-1` means unset, which is why it is a signed value rather than an
/// unsigned color.
pub static NORMAL_FG: crate::globals::GlobalCell<crate::highlight_defs::RgbValue> =
    crate::globals::GlobalCell::new(-1);
/// RGB background of the `Normal` highlight group (`normal_bg`).
pub static NORMAL_BG: crate::globals::GlobalCell<crate::highlight_defs::RgbValue> =
    crate::globals::GlobalCell::new(-1);
/// RGB special (underline/undercurl) color of the `Normal` highlight
/// group (`normal_sp`).
pub static NORMAL_SP: crate::globals::GlobalCell<crate::highlight_defs::RgbValue> =
    crate::globals::GlobalCell::new(-1);

/// Substitute the built-in default for any unset color
/// (`HL_SET_DEFAULT_COLORS`).
///
/// The foreground/background defaults depend on `'background'`, so a
/// dark background gets a light foreground and vice versa; the
/// special color has one fixed default.
///
/// The original is a macro assigning through its arguments; this takes
/// and returns the triple instead.
///
/// # Safety
/// Reads `OPTION_VARS`.
#[must_use]
pub unsafe fn hl_set_default_colors(
    rgb_fg: crate::highlight_defs::RgbValue,
    rgb_bg: crate::highlight_defs::RgbValue,
    rgb_sp: crate::highlight_defs::RgbValue,
) -> (
    crate::highlight_defs::RgbValue,
    crate::highlight_defs::RgbValue,
    crate::highlight_defs::RgbValue,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let p_bg = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bg.clone();
    let dark = p_bg.as_deref().and_then(<[u8]>::first) == Some(&b'd');

    let fg = if rgb_fg != -1 {
        rgb_fg
    } else if dark {
        0x00FF_FFFF
    } else {
        0x0000_0000
    };
    let bg = if rgb_bg != -1 {
        rgb_bg
    } else if dark {
        0x0000_0000
    } else {
        0x00FF_FFFF
    };
    let sp = if rgb_sp != -1 { rgb_sp } else { 0x00FF_0000 };

    (fg, bg, sp)
}

/// Fill in any unset colors of `attrs`, and resolve an inverse
/// attribute by swapping foreground and background
/// (`get_colors_force`).
///
/// Never leaves a color at `-1`. Cterm colors are untouched.
///
/// # Safety
/// Reads `OPTION_VARS` and the `NORMAL_*` file-statics.
#[must_use]
pub unsafe fn get_colors_force(
    attrs: crate::highlight_defs::HlAttrs,
) -> crate::highlight_defs::HlAttrs {
    let mut attrs = attrs;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        if attrs.rgb_bg_color == -1 {
            attrs.rgb_bg_color = *NORMAL_BG.get_mut();
        }
        if attrs.rgb_fg_color == -1 {
            attrs.rgb_fg_color = *NORMAL_FG.get_mut();
        }
        if attrs.rgb_sp_color == -1 {
            attrs.rgb_sp_color = *NORMAL_SP.get_mut();
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    let (fg, bg, sp) =
        unsafe { hl_set_default_colors(attrs.rgb_fg_color, attrs.rgb_bg_color, attrs.rgb_sp_color) };
    attrs.rgb_fg_color = fg;
    attrs.rgb_bg_color = bg;
    attrs.rgb_sp_color = sp;

    if attrs.rgb_ae_attr & (crate::highlight_defs::HL_INVERSE as i32) != 0 {
        std::mem::swap(&mut attrs.rgb_bg_color, &mut attrs.rgb_fg_color);
        attrs.rgb_ae_attr &= !(crate::highlight_defs::HL_INVERSE as i32);
    }

    attrs
}

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

    struct AttributeEntriesGuard {
        entries: crate::map::Set<crate::highlight_defs::HlEntry>,
        active: bool,
    }

    impl AttributeEntriesGuard {
        fn empty() -> Self {
            let entries = std::mem::replace(
                unsafe { ATTR_ENTRIES.get_mut() },
                crate::map::Set::new(),
            );
            let active = unsafe { *HLSTATE_ACTIVE.get_mut() };
            unsafe { *HLSTATE_ACTIVE.get_mut() = false };
            Self { entries, active }
        }
    }

    impl Drop for AttributeEntriesGuard {
        fn drop(&mut self) {
            *unsafe { ATTR_ENTRIES.get_mut() } =
                std::mem::replace(&mut self.entries, crate::map::Set::new());
            unsafe { *HLSTATE_ACTIVE.get_mut() = self.active };
        }
    }

    #[test]
    fn highlight_init_installs_the_zero_attribute_entry_once() {
        let _lock = crate::globals::global_state_test_lock();
        let _entries = AttributeEntriesGuard::empty();

        unsafe { highlight_init() };
        unsafe { highlight_init() };

        assert_eq!(unsafe { ATTR_ENTRIES.get_mut() }.len(), 1);
    }

    #[test]
    fn hl_get_syn_attr_deduplicates_attrs_and_separates_namespaces() {
        let _lock = crate::globals::global_state_test_lock();
        let _entries = AttributeEntriesGuard::empty();
        unsafe { *HLSTATE_ACTIVE.get_mut() = true };
        let attrs = crate::highlight_defs::HlAttrs {
            rgb_ae_attr: crate::highlight_defs::HL_GLOBAL as i32,
            ..Default::default()
        };

        let first = unsafe { hl_get_syn_attr(4, 2, attrs) };
        let same = unsafe { hl_get_syn_attr(4, 2, attrs) };
        let other_namespace = unsafe { hl_get_syn_attr(5, 2, attrs) };

        assert!(first > 0);
        assert_eq!(same, first);
        assert_ne!(other_namespace, first);
    }

    #[test]
    fn hl_get_syn_attr_returns_zero_for_default_attrs_in_namespace_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let _entries = AttributeEntriesGuard::empty();
        assert_eq!(
            unsafe { hl_get_syn_attr(0, 3, Default::default()) },
            0
        );
    }

    struct NamespaceHighlightsGuard {
        definitions: crate::map::Map<
            crate::highlight_defs::ColorKey,
            crate::highlight_defs::ColorItem,
        >,
        providers: Vec<crate::decoration_defs::DecorProvider>,
    }

    impl NamespaceHighlightsGuard {
        fn empty() -> Self {
            let definitions = std::mem::replace(
                unsafe { NS_HLS.get_mut() },
                crate::map::Map::new(),
            );
            let providers =
                std::mem::take(unsafe { crate::decoration_provider::DECOR_PROVIDERS.get_mut() });
            Self {
                definitions,
                providers,
            }
        }
    }

    impl Drop for NamespaceHighlightsGuard {
        fn drop(&mut self) {
            *unsafe { NS_HLS.get_mut() } =
                std::mem::replace(&mut self.definitions, crate::map::Map::new());
            *unsafe { crate::decoration_provider::DECOR_PROVIDERS.get_mut() } =
                std::mem::take(&mut self.providers);
        }
    }

    #[test]
    fn ns_hl_def_stores_a_link_with_the_provider_version() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = NamespaceHighlightsGuard::empty();
        let provider = unsafe { crate::decoration_provider::get_decor_provider(12, true) };
        unsafe {
            (*provider).hl_valid = 7;
            (*provider).hl_cached = true;
        }
        let attrs = crate::highlight_defs::HlAttrs {
            rgb_ae_attr: crate::highlight_defs::HL_GLOBAL as i32,
            ..Default::default()
        };

        unsafe { ns_hl_def(12, 3, attrs, 9) };

        let item = *unsafe { NS_HLS.get_mut() }
            .get(&crate::highlight_defs::ColorKey::new(12, 3))
            .expect("namespace highlight");
        assert_eq!(item.attr_id, -1);
        assert_eq!(item.link_id, 9);
        assert_eq!(item.version, 7);
        assert!(item.link_global);
        assert!(!item.is_default);
        let provider = unsafe { crate::decoration_provider::get_decor_provider(12, false) };
        assert!(!unsafe { (*provider).hl_cached });
    }

    #[test]
    fn ns_hl_def_keeps_an_existing_definition_for_default_attrs() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = NamespaceHighlightsGuard::empty();
        let attrs = crate::highlight_defs::HlAttrs {
            rgb_ae_attr: crate::highlight_defs::HL_GLOBAL as i32,
            ..Default::default()
        };
        unsafe { ns_hl_def(4, 2, attrs, 8) };

        let default_attrs = crate::highlight_defs::HlAttrs {
            rgb_ae_attr: (crate::highlight_defs::HL_GLOBAL
                | crate::highlight_defs::HL_DEFAULT) as i32,
            ..Default::default()
        };
        unsafe { ns_hl_def(4, 2, default_attrs, 11) };

        let item = unsafe { NS_HLS.get_mut() }
            .get(&crate::highlight_defs::ColorKey::new(4, 2))
            .expect("namespace highlight");
        assert_eq!(item.link_id, 8);
        assert!(!item.is_default);
    }

    #[test]
    fn ns_hl_def_stores_direct_attributes() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = NamespaceHighlightsGuard::empty();
        let _entries = AttributeEntriesGuard::empty();
        let attrs = crate::highlight_defs::HlAttrs {
            rgb_ae_attr: crate::highlight_defs::HL_GLOBAL as i32,
            ..Default::default()
        };

        unsafe { ns_hl_def(4, 2, attrs, -1) };

        let item = unsafe { NS_HLS.get_mut() }
            .get(&crate::highlight_defs::ColorKey::new(4, 2))
            .expect("namespace highlight");
        assert!(item.attr_id > 0);
        assert_eq!(item.link_id, -1);
        assert!(item.link_global);
    }

    struct UrlsGuard(Vec<Vec<u8>>);

    impl UrlsGuard {
        fn install(urls: Vec<Vec<u8>>) -> Self {
            Self(std::mem::replace(unsafe { URLS.get_mut() }, urls))
        }
    }

    impl Drop for UrlsGuard {
        fn drop(&mut self) {
            *unsafe { URLS.get_mut() } = std::mem::take(&mut self.0);
        }
    }

    #[test]
    fn hl_get_url_returns_an_owned_indexed_copy() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = UrlsGuard::install(vec![
            b"https://one.example".to_vec(),
            b"https://two.example".to_vec(),
        ]);
        let mut url = unsafe { hl_get_url(1) }.expect("URL");
        assert_eq!(url, b"https://two.example");
        url[8] = b'X';
        assert_eq!(
            unsafe { hl_get_url(1) }.as_deref(),
            Some(b"https://two.example".as_slice())
        );
        assert_eq!(unsafe { hl_get_url(9) }, None);
    }

    // ---- hl_add_font_idx / hl_get_font ----

    /// Restores the interning table, so tests cannot leak entries into
    /// each other's index expectations.
    struct FontsGuard {
        prev: Vec<Vec<u8>>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl FontsGuard {
        fn new() -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let prev = unsafe { FONTS.get_mut() }.clone();
            unsafe { FONTS.get_mut().clear() };
            FontsGuard { prev, _lock }
        }
    }

    impl Drop for FontsGuard {
        fn drop(&mut self) {
            unsafe { *FONTS.get_mut() = std::mem::take(&mut self.prev) };
        }
    }

    #[test]
    fn hl_add_font_idx_returns_a_stable_index_for_the_same_name() {
        // Re-adding an interned name must reuse its index - that is
        // what makes the index a durable identity rather than a
        // position in insertion order.
        let _g = FontsGuard::new();

        let a = unsafe { hl_add_font_idx(b"Fira Code") };
        let b = unsafe { hl_add_font_idx(b"Cascadia") };
        let a_again = unsafe { hl_add_font_idx(b"Fira Code") };

        assert_ne!(a, b, "distinct names get distinct indices");
        assert_eq!(a, a_again, "the same name reuses its index");
    }

    #[test]
    fn hl_get_font_reads_back_what_was_interned() {
        let _g = FontsGuard::new();

        let a = unsafe { hl_add_font_idx(b"Fira Code") };
        let b = unsafe { hl_add_font_idx(b"Cascadia") };

        assert_eq!(unsafe { hl_get_font(a) }.as_deref(), Some(&b"Fira Code"[..]));
        assert_eq!(unsafe { hl_get_font(b) }.as_deref(), Some(&b"Cascadia"[..]));
    }

    #[test]
    fn hl_add_font_idx_rejects_an_empty_name() {
        let _g = FontsGuard::new();
        assert_eq!(unsafe { hl_add_font_idx(b"") }, -1);
        assert_eq!(unsafe { hl_add_font_idx(b"\0") }, -1, "a leading NUL is empty too");
    }

    #[test]
    fn hl_get_font_is_none_for_an_index_naming_no_font() {
        let _g = FontsGuard::new();
        assert_eq!(unsafe { hl_get_font(-1) }, None);
        assert_eq!(unsafe { hl_get_font(0) }, None, "nothing interned yet");

        let a = unsafe { hl_add_font_idx(b"Only") };
        assert_eq!(unsafe { hl_get_font(a + 1) }, None, "past the end");
    }

    #[test]
    fn hl_add_font_idx_stops_a_name_at_an_embedded_nul() {
        // The original interns a NUL-terminated string, so trailing
        // bytes past the NUL are not part of the name.
        let _g = FontsGuard::new();
        let a = unsafe { hl_add_font_idx(b"Mono\0junk") };
        assert_eq!(unsafe { hl_get_font(a) }.as_deref(), Some(&b"Mono"[..]));
    }

    // ---- win_bg_attr ----

    /// Boxed: the window pointer is compared against GLOBALS.curwin.
    fn bg_attr_win(normal: i32, normalnc: i32) -> Box<crate::buffer_defs::WinT> {
        Box::new(crate::buffer_defs::WinT {
            w_hl_attr_normal: normal,
            w_hl_attr_normalnc: normalnc,
            ..Default::default()
        })
    }

    fn with_bg_attr_state<T>(
        curwin: *mut crate::buffer_defs::WinT,
        ns_fast: i32,
        none_attr: i32,
        inactive_attr: i32,
        f: impl FnOnce() -> T,
    ) -> T {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_win = g.curwin;
        g.curwin = curwin;

        let prev_fast = unsafe { *NS_HL_FAST.get_mut() };
        unsafe { *NS_HL_FAST.get_mut() = ns_fast };

        let prev_active = unsafe { *HL_ATTR_ACTIVE.get_mut() };
        unsafe {
            let a = HL_ATTR_ACTIVE.get_mut();
            a[crate::highlight_defs::HlfT::None as usize] = none_attr;
            a[crate::highlight_defs::HlfT::Inactive as usize] = inactive_attr;
        }

        let r = f();

        unsafe {
            *HL_ATTR_ACTIVE.get_mut() = prev_active;
            *NS_HL_FAST.get_mut() = prev_fast;
            crate::globals::GLOBALS.get_mut().curwin = prev_win;
        }
        r
    }

    #[test]
    fn win_bg_attr_prefers_a_window_local_override() {
        let mut win = bg_attr_win(42, 43);
        let wp = std::ptr::addr_of_mut!(*win);
        // Current window: uses w_hl_attr_normal.
        let got = with_bg_attr_state(wp, -1, 7, 8, || unsafe { win_bg_attr(wp) });
        assert_eq!(got, 42);

        // Not the current window: uses w_hl_attr_normalnc instead.
        let got = with_bg_attr_state(std::ptr::null_mut(), -1, 7, 8, || unsafe { win_bg_attr(wp) });
        assert_eq!(got, 43);
    }

    #[test]
    fn win_bg_attr_ignores_the_local_override_during_a_fast_callback() {
        // A non-negative ns_hl_fast means a fast-callback namespace is
        // in effect, which suppresses per-window overrides entirely.
        let mut win = bg_attr_win(42, 43);
        let wp = std::ptr::addr_of_mut!(*win);

        let got = with_bg_attr_state(wp, 1, 7, 8, || unsafe { win_bg_attr(wp) });
        assert_eq!(got, 7, "falls through to the global Normal attribute");
    }

    #[test]
    fn win_bg_attr_uses_normalnc_for_a_non_current_window() {
        // With no window-local override, a non-current window picks up
        // the Inactive attribute - but only when one is defined.
        let mut win = bg_attr_win(0, 0);
        let wp = std::ptr::addr_of_mut!(*win);

        let with_inactive =
            with_bg_attr_state(std::ptr::null_mut(), -1, 7, 8, || unsafe { win_bg_attr(wp) });
        assert_eq!(with_inactive, 8);

        // Undefined Inactive falls back to the normal attribute.
        let without_inactive =
            with_bg_attr_state(std::ptr::null_mut(), -1, 7, 0, || unsafe { win_bg_attr(wp) });
        assert_eq!(without_inactive, 7);
    }

    // ---- hl_set_default_colors / get_colors_force ----

    fn with_background<T>(dark: bool, f: impl FnOnce() -> T) -> T {
        let _lock = crate::globals::global_state_test_lock();
        let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = ov.p_bg.clone();
        ov.p_bg = Some(if dark { b"dark".to_vec() } else { b"light".to_vec() });
        let r = f();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bg = prev;
        r
    }

    #[test]
    fn hl_set_default_colors_depends_on_the_background_setting() {
        // A dark background gets a light foreground and vice versa,
        // so the two must not be filled in with the same constant.
        let (fg_d, bg_d, sp_d) = with_background(true, || unsafe {
            hl_set_default_colors(-1, -1, -1)
        });
        assert_eq!((fg_d, bg_d), (0x00FF_FFFF, 0x0000_0000));

        let (fg_l, bg_l, sp_l) = with_background(false, || unsafe {
            hl_set_default_colors(-1, -1, -1)
        });
        assert_eq!((fg_l, bg_l), (0x0000_0000, 0x00FF_FFFF));

        // The special color has one fixed default either way.
        assert_eq!(sp_d, 0x00FF_0000);
        assert_eq!(sp_l, 0x00FF_0000);
    }

    #[test]
    fn hl_set_default_colors_leaves_colors_that_are_already_set() {
        let got = with_background(true, || unsafe {
            hl_set_default_colors(0x0011_2233, 0x0044_5566, 0x0077_8899)
        });
        assert_eq!(got, (0x0011_2233, 0x0044_5566, 0x0077_8899));
    }

    #[test]
    fn get_colors_force_never_leaves_a_color_unset() {
        let attrs = crate::highlight_defs::HlAttrs {
            rgb_fg_color: -1,
            rgb_bg_color: -1,
            rgb_sp_color: -1,
            ..Default::default()
        };
        let got = with_background(true, || unsafe { get_colors_force(attrs) });

        assert_ne!(got.rgb_fg_color, -1);
        assert_ne!(got.rgb_bg_color, -1);
        assert_ne!(got.rgb_sp_color, -1);
    }

    #[test]
    fn get_colors_force_resolves_inverse_by_swapping_and_clearing_it() {
        // The inverse attribute is applied here rather than passed on,
        // so the caller sees already-swapped colors and no flag.
        let attrs = crate::highlight_defs::HlAttrs {
            rgb_fg_color: 0x0011_1111,
            rgb_bg_color: 0x0022_2222,
            rgb_sp_color: 0x0033_3333,
            rgb_ae_attr: crate::highlight_defs::HL_INVERSE as i32,
            ..Default::default()
        };
        let got = with_background(true, || unsafe { get_colors_force(attrs) });

        assert_eq!(got.rgb_fg_color, 0x0022_2222);
        assert_eq!(got.rgb_bg_color, 0x0011_1111);
        assert_eq!(
            got.rgb_ae_attr & crate::highlight_defs::HL_INVERSE as i32,
            0,
            "the flag is cleared once applied"
        );
    }

    #[test]
    fn get_colors_force_leaves_cterm_colors_alone() {
        let attrs = crate::highlight_defs::HlAttrs {
            rgb_fg_color: -1,
            rgb_bg_color: -1,
            rgb_sp_color: -1,
            cterm_fg_color: 4,
            cterm_bg_color: 5,
            ..Default::default()
        };
        let got = with_background(true, || unsafe { get_colors_force(attrs) });

        assert_eq!(got.cterm_fg_color, 4);
        assert_eq!(got.cterm_bg_color, 5);
    }

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
