//! Translated from `src/nvim/highlight_defs.h`.

pub type RgbValue = i32;

/// Highlighting attribute bits (`HlAttrFlags`). Kept as plain `u32`
/// bit-flag constants (not a Rust `enum`) since these are combined via
/// bitwise OR into a single `rgb_ae_attr`/`cterm_ae_attr` field - a Rust
/// `enum` represents mutually-exclusive variants, which these are not.
///
/// The sign bit should not be used here, as it identifies an invalid
/// highlight.
pub const HL_INVERSE: u32 = 0x01;
pub const HL_BOLD: u32 = 0x02;
pub const HL_ITALIC: u32 = 0x04;
// The next three bits are all underline styles.
pub const HL_UNDERLINE_MASK: u32 = 0x38;
pub const HL_UNDERLINE: u32 = 0x08;
pub const HL_UNDERCURL: u32 = 0x10;
pub const HL_UNDERDOUBLE: u32 = 0x18;
pub const HL_UNDERDOTTED: u32 = 0x20;
pub const HL_UNDERDASHED: u32 = 0x28;
// 0x30 and 0x38 spare for underline styles.
pub const HL_STANDOUT: u32 = 0x0040;
pub const HL_STRIKETHROUGH: u32 = 0x0080;
pub const HL_ALTFONT: u32 = 0x0100;
pub const HL_DIM: u32 = 0x0200;
pub const HL_BLINK: u32 = 0x8000;
/// SGR attribute, unrelated to the `HL_CONCEAL` syntax flag.
pub const HL_CONCEALED: u32 = 0x10000;
pub const HL_OVERLINE: u32 = 0x20000;
pub const HL_NOCOMBINE: u32 = 0x0400;
pub const HL_BG_INDEXED: u32 = 0x0800;
pub const HL_FG_INDEXED: u32 = 0x1000;
pub const HL_DEFAULT: u32 = 0x2000;
pub const HL_GLOBAL: u32 = 0x4000;

/// Stores a complete highlighting entry, including colors and attributes
/// for both TUI and GUI (`HlAttrs`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HlAttrs {
    /// `HlAttrFlags`
    pub rgb_ae_attr: i32,
    /// `HlAttrFlags`
    pub cterm_ae_attr: i32,
    pub rgb_fg_color: RgbValue,
    pub rgb_bg_color: RgbValue,
    pub rgb_sp_color: RgbValue,
    pub cterm_fg_color: i16,
    pub cterm_bg_color: i16,
    pub hl_blend: i32,
    pub url: i32,
    pub font: i32,
}

impl Default for HlAttrs {
    /// `HLATTRS_INIT`
    fn default() -> Self {
        HlAttrs {
            rgb_ae_attr: 0,
            cterm_ae_attr: 0,
            rgb_fg_color: -1,
            rgb_bg_color: -1,
            rgb_sp_color: -1,
            cterm_fg_color: 0,
            cterm_bg_color: 0,
            hl_blend: -1,
            url: -1,
            font: -1,
        }
    }
}

/// Values for index in `highlight_attr[]` (`hlf_T`).
///
/// When making changes, also update `hlf_names` in `highlight.c` (not yet
/// translated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum HlfT {
    /// no UI highlight active
    None = 0,
    /// Meta & special keys listed with ":map", text that is displayed
    /// different from what it is
    F8,
    /// after the last line in the buffer
    Eob,
    /// terminal cursor focused
    Term,
    /// @ characters at end of screen, characters that don't really exist in
    /// the text
    At,
    /// directories in CTRL-D listing
    D,
    /// error messages
    E,
    /// incremental search
    I,
    /// last search string
    L,
    /// current search match
    Lc,
    /// "--More--" message
    M,
    /// Mode (e.g., "-- INSERT --")
    Cm,
    /// line number for ":number" and ":#" commands
    N,
    /// LineNrAbove
    Lna,
    /// LineNrBelow
    Lnb,
    /// current line number when 'cursorline' is set
    Cln,
    /// current line sign column
    Cls,
    /// current line fold
    Clf,
    /// return to continue message and yes/no questions
    R,
    /// status lines
    S,
    /// status lines of not-current windows
    Snc,
    /// window split separators
    C,
    /// VertSplit
    Vsp,
    /// Titles for output from ":set all", ":autocmd" etc.
    T,
    /// Visual mode
    V,
    /// Visual mode, autoselecting and not clipboard owner
    Vnc,
    /// warning messages
    W,
    /// Wildmenu highlight
    Wm,
    /// Folded line
    Fl,
    /// Fold column
    Fc,
    /// Added diff line
    Add,
    /// Changed diff line
    Chd,
    /// Deleted diff line
    Ded,
    /// Text Changed in diff line
    Txd,
    /// Text Added in changed diff line
    Txa,
    /// Sign column
    Sc,
    /// Concealed text
    Conceal,
    /// SpellBad
    Spb,
    /// SpellCap
    Spc,
    /// SpellRare
    Spr,
    /// SpellLocal
    Spl,
    /// popup menu normal item
    Pni,
    /// popup menu selected item
    Psi,
    /// popup menu matched text in normal item
    Pmni,
    /// popup menu matched text in selected item
    Pmsi,
    /// popup menu normal item "kind"
    Pnk,
    /// popup menu selected item "kind"
    Psk,
    /// popup menu normal item "menu" (extra text)
    Pnx,
    /// popup menu selected item "menu" (extra text)
    Psx,
    /// popup menu scrollbar
    Psb,
    /// popup menu scrollbar thumb
    Pst,
    /// popup menu border
    Pbr,
    /// tabpage line
    Tp,
    /// tabpage line selected
    Tps,
    /// tabpage line filler
    Tpf,
    /// 'cursorcolumn'
    Cuc,
    /// 'cursorline'
    Cul,
    /// 'colorcolumn'
    Mc,
    /// selected quickfix line
    Qfl,
    /// Whitespace
    F0,
    /// NormalNC: Normal text in non-current windows
    Inactive,
    /// message separator line
    Msgsep,
    /// Floating window
    Nfloat,
    /// Message area
    Msg,
    /// Floating window border
    Border,
    /// Window bars
    Wbr,
    /// Window bars of not-current windows
    Wbrnc,
    /// Cursor
    Cu,
    /// Float Border Title
    Btitle,
    /// Float Border Footer
    Bfooter,
    /// status line for terminal window
    Ts,
    /// status line for non-current terminal window
    Tsnc,
    /// stderr messages (from shell)
    Se,
    /// stdout messages (from shell)
    So,
    /// OK message
    Ok,
    /// "preinsert" in 'completeopt'
    Pre,
    /// MUST be the last one
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlKind {
    Unknown,
    Ui,
    Syntax,
    Terminal,
    Combine,
    Blend,
    BlendThrough,
    Invalid,
}

#[derive(Debug, Clone, Copy)]
pub struct HlEntry {
    pub attr: HlAttrs,
    pub kind: HlKind,
    pub id1: i32,
    pub id2: i32,
    pub winid: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColorKey {
    pub ns_id: i32,
    pub syn_id: i32,
}

impl ColorKey {
    /// `ColorKey(n, s)` macro constructor.
    #[inline]
    pub const fn new(ns_id: i32, syn_id: i32) -> Self {
        ColorKey { ns_id, syn_id }
    }
}

/// `HlAttrKey(a, b)`: packs two 32-bit values into one 64-bit lookup key.
#[inline]
pub const fn hl_attr_key(a: i32, b: i32) -> u64 {
    ((a as u32 as u64) << 32) | (b as u32 as u64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorItem {
    pub attr_id: i32,
    pub link_id: i32,
    pub version: i32,
    pub is_default: bool,
    pub link_global: bool,
}

impl Default for ColorItem {
    /// `COLOR_ITEM_INITIALIZER`
    fn default() -> Self {
        ColorItem {
            attr_id: -1,
            link_id: -1,
            version: -1,
            is_default: false,
            link_global: false,
        }
    }
}

pub const HLATTRS_DICT_SIZE: i32 = 24;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hlattrs_default_matches_c_initializer() {
        let d = HlAttrs::default();
        assert_eq!(d.rgb_fg_color, -1);
        assert_eq!(d.rgb_bg_color, -1);
        assert_eq!(d.rgb_sp_color, -1);
        assert_eq!(d.hl_blend, -1);
        assert_eq!(d.url, -1);
        assert_eq!(d.font, -1);
        assert_eq!(d.cterm_fg_color, 0);
    }

    #[test]
    fn color_item_default_matches_c_initializer() {
        let d = ColorItem::default();
        assert_eq!(d.attr_id, -1);
        assert_eq!(d.link_id, -1);
        assert_eq!(d.version, -1);
        assert!(!d.is_default);
        assert!(!d.link_global);
    }

    #[test]
    fn hl_attr_key_packs_both_halves() {
        let k = hl_attr_key(1, 2);
        assert_eq!(k >> 32, 1);
        assert_eq!(k & 0xFFFF_FFFF, 2);
    }

    #[test]
    fn hlf_t_count_is_last_and_none_is_first() {
        assert_eq!(HlfT::None as u8, 0);
        // Spot check the enum is sequential and Count is indeed last by
        // construction (auto-incremented, matching the C enum exactly).
        assert!(HlfT::Count as u8 > HlfT::Pre as u8);
    }
}
