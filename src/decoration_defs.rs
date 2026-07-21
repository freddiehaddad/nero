//! Translated from `src/nvim/decoration_defs.h`.

use crate::api::private::defs::NvimString;
use crate::types_defs::{HandleT, LuaRef, ScharT, SIGN_WIDTH};

pub const DECOR_ID_INVALID: u32 = u32::MAX;

/// `NS` (namespace id) - re-exported here since decoration types are keyed
/// by it pervasively.
pub type Ns = HandleT;

#[derive(Debug, Clone)]
pub struct VirtTextChunk {
    pub text: NvimString,
    /// `-1` if not specified
    pub hl_id: i32,
}

pub type VirtText = Vec<VirtTextChunk>;

/// Keep in sync with `virt_text_pos_str[]` in `decoration.c` (not yet
/// translated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtTextPos {
    EndOfLine,
    EndOfLineRightAlign,
    Inline,
    Overlay,
    RightAlign,
    WinCol,
}

/// Flags for virtual lines.
pub const VL_LEFTCOL: u8 = 1; // Start at left window edge, ignoring number column, etc.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtLineOverflow {
    /// Truncate with `'nowrap'`
    Trunc,
    /// Scroll horizontally with `'nowrap'`
    Scroll,
    /// Wrap onto extra rows
    Wrap,
    /// Scroll with `'nowrap'`; wrap with `'wrap'`
    Auto,
}

#[derive(Debug, Clone)]
pub struct VirtLine {
    pub line: VirtText,
    pub flags: i32,
    pub overflow: VirtLineOverflow,
}

pub type VirtLines = Vec<VirtLine>;

pub type DecorPriority = u16;
pub type DecorPriorityInternal = u32;
pub const DECOR_PRIORITY_BASE: DecorPriorityInternal = 0x1000;

/// Keep in sync with `hl_mode_str[]` in `decoration.c` (not yet translated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlMode {
    Unknown,
    Replace,
    Combine,
    Blend,
}

pub const SH_IS_SIGN: u16 = 1;
pub const SH_HL_EOL: u16 = 2;
pub const SH_UI_WATCHED: u16 = 4;
pub const SH_UI_WATCHED_OVERLAY: u16 = 8;
pub const SH_SPELL_ON: u16 = 16;
pub const SH_SPELL_OFF: u16 = 32;
pub const SH_CONCEAL: u16 = 64;
pub const SH_CONCEAL_LINES: u16 = 128;
pub const SH_CONCEAL_OFF: u16 = 256;

#[derive(Debug, Clone, Copy)]
pub struct DecorHighlightInline {
    pub flags: u16,
    pub priority: DecorPriority,
    pub hl_id: i32,
    pub conceal_char: ScharT,
}

impl Default for DecorHighlightInline {
    /// `DECOR_HIGHLIGHT_INLINE_INIT`
    fn default() -> Self {
        DecorHighlightInline {
            flags: 0,
            priority: DECOR_PRIORITY_BASE as DecorPriority,
            hl_id: 0,
            conceal_char: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecorSignHighlight {
    pub flags: u16,
    pub priority: DecorPriority,
    /// if sign: highlight of sign text
    pub hl_id: i32,
    /// conceal text only uses `text[0]`
    pub text: [ScharT; SIGN_WIDTH as usize],
    // NOTE: if more functionality is added to a Highlight these should be
    // overloaded or restructured (kept from the original's own comment).
    pub sign_name: Option<NvimString>,
    pub sign_add_id: i32,
    pub number_hl_id: i32,
    pub line_hl_id: i32,
    pub cursorline_hl_id: i32,
    pub next: u32,
    pub url: Option<NvimString>,
}

impl Default for DecorSignHighlight {
    /// `DECOR_SIGN_HIGHLIGHT_INIT`
    fn default() -> Self {
        DecorSignHighlight {
            flags: 0,
            priority: DECOR_PRIORITY_BASE as DecorPriority,
            hl_id: 0,
            text: [0; SIGN_WIDTH as usize],
            sign_name: None,
            sign_add_id: 0,
            number_hl_id: 0,
            line_hl_id: 0,
            cursorline_hl_id: 0,
            next: DECOR_ID_INVALID,
            url: None,
        }
    }
}

pub const VT_IS_LINES: u8 = 1;
pub const VT_HIDE: u8 = 2;
pub const VT_LINES_ABOVE: u8 = 4;
pub const VT_REPEAT_LINEBREAK: u8 = 8;

/// `struct DecorVirtText`'s `data` union in the original is tagged
/// externally by the `kVTIsLines` bit in `flags`. Since `DecorVirtText`
/// values here are never stored compactly inline in the marktree (only
/// `DecorVirtText` *pointers* are, via `DecorExt`/`next`), there is no
/// memory-layout reason to prefer an untagged union over a safe Rust enum
/// for this one - unlike [`DecorInlineData`] below, which explains the
/// opposite choice and why.

#[derive(Debug, Clone)]
pub struct DecorVirtText {
    pub flags: u8,
    pub hl_mode: HlMode,
    pub priority: DecorPriority,
    /// width of `virt_text`
    pub width: i32,
    pub col: i32,
    pub pos: VirtTextPos,
    // TODO(bfredl): reduce this to one datatype, later (kept from the
    // original's own comment). Modeled as a safe tagged enum here rather
    // than the union above: unlike DecorInlineData, this data is never
    // stored compactly inline in the marktree (only DecorVirtText
    // *pointers* are), so there is no memory-layout reason to prefer an
    // untagged union over a safe enum here.
    pub data: DecorVirtTextEnumData,
    pub next: Option<Box<DecorVirtText>>,
}

#[derive(Debug, Clone)]
pub enum DecorVirtTextEnumData {
    VirtText(VirtText),
    VirtLines(VirtLines),
}

impl Default for DecorVirtText {
    /// `DECOR_VIRT_TEXT_INIT`
    fn default() -> Self {
        DecorVirtText {
            flags: 0,
            hl_mode: HlMode::Unknown,
            priority: DECOR_PRIORITY_BASE as DecorPriority,
            width: 0,
            col: 0,
            pos: VirtTextPos::EndOfLine,
            data: DecorVirtTextEnumData::VirtText(Vec::new()),
            next: None,
        }
    }
}

impl DecorVirtText {
    /// `DECOR_VIRT_LINES_INIT`
    pub fn virt_lines_init() -> Self {
        DecorVirtText {
            flags: VT_IS_LINES,
            data: DecorVirtTextEnumData::VirtLines(Vec::new()),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecorExt {
    pub sh_idx: u32,
    pub vt: Option<Box<DecorVirtText>>,
}

/// Stored inline in the marktree, with `MT_FLAG_DECOR_EXT` in `MTKey.flags`
/// (`marktree_defs.h`, phase 3) telling readers which field is live.
///
/// Translated as a genuine Rust `union` (requiring `unsafe` to access,
/// exactly like the original's implicit "trust the tag" contract) rather
/// than a safe tagged enum: the whole point of this union in the original
/// is compact, uniform-size, zero-extra-tag storage inline in every
/// marktree key (a data structure that may have very many keys, where
/// memory matters) - the tag deliberately lives *outside* this type (in
/// the owning `MTKey`), so wrapping it in a safe enum here would add a
/// redundant discriminant that doesn't exist in the original and would
/// undermine the exact memory-layout optimization the original is making.
///
/// No `Clone`/`Copy`/`Drop` are implemented (yet): `ext`'s `DecorExt` owns a
/// `Box`, so cloning or dropping this union safely requires knowing which
/// field is live - information that only the not-yet-translated owning
/// `MTKey`/marktree (`marktree.c`, phase 3) has. Implementing those here,
/// disconnected from that real owner and its actual tag-checking logic,
/// would risk getting the unsafe contract wrong; deferred until that
/// context exists.
pub union DecorInlineData {
    pub hl: DecorHighlightInline,
    pub ext: std::mem::ManuallyDrop<DecorExt>,
}

/// Not stored in the marktree, but used when passing around args.
///
/// Convention: an empty "no decoration" value should always be encoded
/// with `ext: false` and an unset [`DecorHighlightInline`] (no flags, no
/// `hl_id`).
///
/// Unlike [`DecorInlineData`] (see its doc comment), this struct's tag
/// (`ext`) lives right alongside the data, so translating it as a safe
/// Rust enum is a direct, lossless simplification with no memory-layout
/// tradeoff being discarded.
#[derive(Debug, Clone)]
pub enum DecorInline {
    Highlight(DecorHighlightInline),
    Ext(DecorExt),
}

impl Default for DecorInline {
    /// `DECOR_INLINE_INIT`
    fn default() -> Self {
        DecorInline::Highlight(DecorHighlightInline::default())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorProviderState {
    Active = 1,
    WinDisabled = 2,
    RedrawDisabled = 3,
    Disabled = 4,
}

#[derive(Debug, Clone, Copy)]
pub struct DecorProvider {
    pub ns_id: Ns,
    pub state: DecorProviderState,
    pub win_skip_row: i32,
    pub win_skip_col: i32,
    pub redraw_start: LuaRef,
    pub redraw_buf: LuaRef,
    pub redraw_win: LuaRef,
    pub redraw_line: LuaRef,
    pub redraw_range: LuaRef,
    pub redraw_end: LuaRef,
    pub hl_def: LuaRef,
    pub spell_nav: LuaRef,
    pub conceal_line: LuaRef,
    pub hl_valid: i32,
    pub hl_cached: bool,
    pub error_count: u8,
}

impl DecorProvider {
    /// `DECORATION_PROVIDER_INIT(ns_id)`
    pub fn new(ns_id: Ns) -> Self {
        const LUA_NOREF: LuaRef = -1;
        DecorProvider {
            ns_id,
            state: DecorProviderState::Disabled,
            win_skip_row: 0,
            win_skip_col: 0,
            redraw_start: LUA_NOREF,
            redraw_buf: LUA_NOREF,
            redraw_win: LUA_NOREF,
            redraw_line: LUA_NOREF,
            redraw_range: LUA_NOREF,
            redraw_end: LUA_NOREF,
            hl_def: LUA_NOREF,
            spell_nav: LUA_NOREF,
            conceal_line: LUA_NOREF,
            hl_valid: -1,
            hl_cached: false,
            error_count: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decor_highlight_inline_default_matches_c_initializer() {
        // DECOR_HIGHLIGHT_INLINE_INIT = { 0, DECOR_PRIORITY_BASE, 0, 0 }
        let d = DecorHighlightInline::default();
        assert_eq!(d.flags, 0);
        assert_eq!(d.priority as u32, DECOR_PRIORITY_BASE);
        assert_eq!(d.hl_id, 0);
        assert_eq!(d.conceal_char, 0);
    }

    #[test]
    fn decor_sign_highlight_default_matches_c_initializer() {
        // DECOR_SIGN_HIGHLIGHT_INIT: next = DECOR_ID_INVALID, rest zero/None
        let d = DecorSignHighlight::default();
        assert_eq!(d.next, DECOR_ID_INVALID);
        assert_eq!(d.hl_id, 0);
        assert!(d.sign_name.is_none());
        assert!(d.url.is_none());
    }

    #[test]
    fn decor_inline_default_is_unset_highlight() {
        match DecorInline::default() {
            DecorInline::Highlight(h) => {
                assert_eq!(h.flags, 0);
                assert_eq!(h.hl_id, 0);
            }
            DecorInline::Ext(_) => panic!("DECOR_INLINE_INIT must default to the hl branch"),
        }
    }

    #[test]
    fn decoration_provider_init_matches_c_macro() {
        // DECORATION_PROVIDER_INIT(ns_id): state=Disabled, all LuaRefs=LUA_NOREF(-1),
        // hl_valid=-1, hl_cached=false, error_count=0.
        let p = DecorProvider::new(7);
        assert_eq!(p.ns_id, 7);
        assert_eq!(p.state, DecorProviderState::Disabled);
        assert_eq!(p.redraw_start, -1);
        assert_eq!(p.hl_valid, -1);
        assert!(!p.hl_cached);
        assert_eq!(p.error_count, 0);
    }

    #[test]
    fn decor_virt_text_vs_virt_lines_init_differ_in_flags_and_data() {
        let vt = DecorVirtText::default();
        assert_eq!(vt.flags, 0);
        assert!(matches!(vt.data, DecorVirtTextEnumData::VirtText(_)));

        let vl = DecorVirtText::virt_lines_init();
        assert_eq!(vl.flags, VT_IS_LINES);
        assert!(matches!(vl.data, DecorVirtTextEnumData::VirtLines(_)));
    }
}
