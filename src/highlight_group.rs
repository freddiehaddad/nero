//! Translated from `src/nvim/highlight_group.c` (tractable core only).
//!
//! `highlight_group.c` (~3200 lines) owns the highlight-group
//! registry: the table every group name and its attributes live in,
//! plus `:highlight` command handling, colour parsing, group linking
//! and the attribute-set plumbing into the UI.
//!
//! Translated so far: the registry's own storage - [`HlGroup`], the
//! [`sg_set`] flags and the [`HL_TABLE`] file-static - together with
//! [`syn_id2name`], the ID-to-name lookup, and the namespace-aware
//! [`syn_ns_get_final_id`]/[`syn_ns_id2attr`] link and attribute
//! resolution used by [`syn_id2attr`].
//!
//! Deferred: `syn_name2id`/`syn_name2id_len` need the separate
//! `highlight_unames` name-to-ID hash map (and `syn_check_group` for
//! the `@` tree-sitter capture form); `:highlight` parsing, colour
//! lookup, group linking, `highlight_clear` and the attribute plumbing
//! all need the UI attribute tables.

use crate::garray_defs::TypedGarrayT;
use crate::globals::GlobalCell;

/// Maximum length of a syntax/highlight group name (`MAX_SYN_NAME`).
pub const MAX_SYN_NAME: usize = 200;

/// Which parts of a highlight group have been set explicitly
/// (`SG_SET`: `SG_CTERM`/`SG_GUI`/`SG_LINK`).
pub mod sg_set {
    /// `cterm` has been set (`SG_CTERM`).
    pub const CTERM: i32 = 2;
    /// `gui` has been set (`SG_GUI`).
    pub const GUI: i32 = 4;
    /// a link has been set (`SG_LINK`).
    pub const LINK: i32 = 8;
}

/// One entry in the highlight-group table (`HlGroup`).
///
/// The original's three owned `char *` become owned Rust values:
/// `sg_name` and `sg_name_u` are always present so they are plain
/// `Vec<u8>`, while `sg_font` is genuinely optional (`NULL` when not
/// set) so it is an `Option`.
///
/// `sg_name_u` is the uppercase of `sg_name`, precomputed exactly as
/// the original does so name comparisons avoid repeated case-folding -
/// the same arrangement as `syntax.rs`'s `SynClusterT`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HlGroup {
    /// highlight group name (`sg_name`).
    pub sg_name: Vec<u8>,
    /// uppercase of `sg_name` (`sg_name_u`).
    pub sg_name_u: Vec<u8>,
    /// `":hi clear"` was used (`sg_cleared`).
    pub sg_cleared: bool,
    /// screen attribute (`sg_attr`).
    pub sg_attr: i32,
    /// link to this highlight group ID (`sg_link`).
    pub sg_link: i32,
    /// default link, restored by `highlight_clear` (`sg_deflink`).
    pub sg_deflink: i32,
    /// combination of the [`sg_set`] flags (`sg_set`).
    pub sg_set: i32,
    /// script where the default link was set (`sg_deflink_sctx`).
    pub sg_deflink_sctx: crate::eval::typval_defs::SctxT,
    /// script in which the group was last set (`sg_script_ctx`).
    pub sg_script_ctx: crate::eval::typval_defs::SctxT,
    /// `"cterm="` highlighting attributes (`sg_cterm`).
    pub sg_cterm: i32,
    /// terminal foreground colour number + 1 (`sg_cterm_fg`).
    pub sg_cterm_fg: i32,
    /// terminal background colour number + 1 (`sg_cterm_bg`).
    pub sg_cterm_bg: i32,
    /// bold was set for a light colour on RGB UIs (`sg_cterm_bold`).
    pub sg_cterm_bold: bool,
    /// `"gui="` highlighting attributes (`sg_gui`).
    pub sg_gui: i32,
    /// RGB foreground colour (`sg_rgb_fg`).
    pub sg_rgb_fg: crate::highlight_defs::RgbValue,
    /// RGB background colour (`sg_rgb_bg`).
    pub sg_rgb_bg: crate::highlight_defs::RgbValue,
    /// RGB special colour (`sg_rgb_sp`).
    pub sg_rgb_sp: crate::highlight_defs::RgbValue,
    /// RGB foreground colour index (`sg_rgb_fg_idx`).
    pub sg_rgb_fg_idx: i32,
    /// RGB background colour index (`sg_rgb_bg_idx`).
    pub sg_rgb_bg_idx: i32,
    /// RGB special colour index (`sg_rgb_sp_idx`).
    pub sg_rgb_sp_idx: i32,
    /// blend level (0-100 inclusive), `-1` if unset (`sg_blend`).
    pub sg_blend: i32,
    /// font name, absent if not set (`sg_font`).
    pub sg_font: Option<Vec<u8>>,
    /// parent of an `@nested.group` (`sg_parent`).
    pub sg_parent: i32,
}

/// The highlight-group table itself (`highlight_ga`, whose items the
/// original reaches through the `hl_table` macro).
///
/// A [`TypedGarrayT`] rather than the erased `GarrayT`, matching this
/// crate's treatment of every other growarray holding a struct that
/// owns heap memory.
pub static HL_TABLE: GlobalCell<TypedGarrayT<HlGroup>> =
    GlobalCell::new(TypedGarrayT::new(10));

/// Index from a group's UPPERCASE name to its 1-based ID
/// (`highlight_unames`).
///
/// Kept as a real map rather than folded into a scan over
/// [`HL_TABLE`], matching the original: it exists precisely so name
/// lookups do not walk the table, and its keys are the uppercase
/// forms so lookups are case-insensitive without repeated folding.
///
/// Keys hold the uppercase name with NO trailing NUL, and so does
/// each group's own `sg_name_u`. The original stores both as
/// NUL-terminated C strings, but its map keys are `cstr_t`, hashed by
/// content up to the NUL, so the terminator is not part of the
/// logical key there either. A Rust `Vec<u8>` key would include it,
/// which is why every writer and reader here must agree to leave it
/// off - uppercase with [`crate::strings::vim_strup`], never
/// `vim_strsave_up`, which appends one.
pub static HIGHLIGHT_UNAMES: std::sync::LazyLock<GlobalCell<crate::map::Map<Vec<u8>, i32>>> =
    std::sync::LazyLock::new(|| GlobalCell::new(crate::map::Map::new()));

/// Uppercase `name` into a fresh buffer with no trailing NUL, the
/// form [`HIGHLIGHT_UNAMES`] keys and `sg_name_u` both use.
#[must_use]
fn upper_name(name: &[u8]) -> Vec<u8> {
    let mut out = name.to_vec();
    crate::strings::vim_strup(&mut out);
    out
}

/// Look up a highlight group by name and return its 1-based ID, or
/// `0` when there is no such group (`syn_name2id_len`).
///
/// The lookup is case-INSENSITIVE: the needle is uppercased and
/// matched against [`HIGHLIGHT_UNAMES`]'s uppercase keys.
///
/// An empty name, or one longer than [`MAX_SYN_NAME`], is rejected
/// outright - the original guards this because it uppercases into a
/// fixed `MAX_SYN_NAME + 1` stack buffer. That buffer is unnecessary
/// here, but the bound is part of the observable contract, so it is
/// kept rather than silently accepting longer names.
///
/// # Safety
/// Reads the [`HIGHLIGHT_UNAMES`] file-static.
#[must_use]
pub unsafe fn syn_name2id_len(name: &[u8]) -> i32 {
    if name.is_empty() || name.len() > MAX_SYN_NAME {
        return 0;
    }
    let name_u = upper_name(name);
    // SAFETY: forwarded from this function's own safety doc. A missing
    // key yields 0, which is the original's own "no such group".
    unsafe { HIGHLIGHT_UNAMES.get_mut() }.get_or_default(&name_u)
}

/// Maximum value for a highlight ID (`MAX_HL_ID`).
pub const MAX_HL_ID: i32 = 20000;

/// "no colour index" sentinel (`kColorIdxNone`).
pub const COLOR_IDX_NONE: i32 = -1;
/// Explicit hexadecimal colour (`kColorIdxHex`).
pub const COLOR_IDX_HEX: i32 = -2;
/// Follow the active `Normal` foreground (`kColorIdxFg`).
pub const COLOR_IDX_FG: i32 = -3;
/// Follow the active `Normal` background (`kColorIdxBg`).
pub const COLOR_IDX_BG: i32 = -4;

const CTERM_COLOR_NAMES: [&[u8]; 28] = [
    b"Black",
    b"DarkBlue",
    b"DarkGreen",
    b"DarkCyan",
    b"DarkRed",
    b"DarkMagenta",
    b"Brown",
    b"DarkYellow",
    b"Gray",
    b"Grey",
    b"LightGray",
    b"LightGrey",
    b"DarkGray",
    b"DarkGrey",
    b"Blue",
    b"LightBlue",
    b"Green",
    b"LightGreen",
    b"Cyan",
    b"LightCyan",
    b"Red",
    b"LightRed",
    b"Magenta",
    b"LightMagenta",
    b"Yellow",
    b"LightYellow",
    b"White",
    b"NONE",
];
const CTERM_COLORS_16: [i32; 28] = [
    0, 1, 2, 3, 4, 5, 6, 6, 7, 7, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14,
    14, 15, -1,
];
const CTERM_COLORS_88: [i32; 28] = [
    0, 4, 2, 6, 1, 5, 32, 72, 84, 84, 7, 7, 82, 82, 12, 43, 10, 61, 14, 63, 9, 74, 13,
    75, 11, 78, 15, -1,
];
const CTERM_COLORS_256: [i32; 28] = [
    0, 4, 2, 6, 1, 5, 130, 3, 248, 248, 7, 7, 242, 242, 12, 81, 10, 121, 14, 159, 9, 224,
    13, 225, 11, 229, 15, -1,
];
const CTERM_COLORS_8: [i32; 28] = [
    0, 4, 2, 6, 1, 5, 3, 3, 7, 7, 7, 7, 8, 8, 12, 12, 10, 10, 14, 14, 9, 9, 13, 13, 11,
    11, 15, -1,
];

/// Resolve one indexed cterm color for the active terminal palette
/// (`lookup_color`).
///
/// # Safety
/// Reads `GLOBALS.t_colors`.
fn lookup_color(
    idx: usize,
    foreground: bool,
    bold: &mut crate::types_defs::TriState,
) -> i32 {
    let mut color = CTERM_COLORS_16[idx];
    if color < 0 {
        return -1;
    }
    let t_colors = unsafe { crate::globals::GLOBALS.get_mut() }.t_colors;
    if t_colors == 8 {
        color = CTERM_COLORS_8[idx];
        if foreground {
            *bold = if color & 8 != 0 {
                crate::types_defs::TriState::True
            } else {
                crate::types_defs::TriState::False
            };
        }
        color &= 7;
    } else if t_colors == 16 {
        color = CTERM_COLORS_8[idx];
    } else if t_colors == 88 {
        color = CTERM_COLORS_88[idx];
    } else if t_colors >= 256 {
        color = CTERM_COLORS_256[idx];
    }
    color
}

/// Translate a named cterm color to its terminal color number
/// (`name_to_ctermcolor`).
///
/// # Safety
/// Reads `GLOBALS.t_colors`.
#[must_use]
pub unsafe fn name_to_ctermcolor(name: &[u8]) -> i32 {
    let Some(index) = CTERM_COLOR_NAMES
        .iter()
        .rposition(|candidate| candidate.eq_ignore_ascii_case(name))
    else {
        return -1;
    };
    let mut bold = crate::types_defs::TriState::None;
    lookup_color(index, false, &mut bold)
}

/// Append a new highlight group and return its 1-based ID, or `0` on
/// failure (`syn_add_group`).
///
/// Rejects a name containing an unprintable character (`E669`) or any
/// character outside ASCII alphanumerics, `_`, `.`, `@` and `-`. The
/// `.` and `@` are allowed for tree-sitter capture names. Both
/// messages are omitted, matching this crate's policy, keeping the
/// same `0` return.
///
/// A scoped `@a.b` name records its parent `@a` in `sg_parent`,
/// creating that parent on demand - which is why this and
/// [`syn_check_group`] are mutually recursive, exactly as in the
/// original.
///
/// The original's first-call growarray init and its `ga_grow(300)`
/// pre-size are dropped: a `Vec` owns and grows its own storage, and
/// the pre-size is purely an allocation hint.
///
/// # Safety
/// Touches the [`HL_TABLE`] and [`HIGHLIGHT_UNAMES`] file-statics, and
/// reads the charset tables via `vim_isprintc`.
pub unsafe fn syn_add_group(name: &[u8]) -> i32 {
    // Check that the name is valid.
    for &b in name {
        let c = i32::from(b);
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { crate::charset::vim_isprintc(c) } {
            return 0;
        }
        if !crate::macros_defs::ascii_isalnum(c)
            && b != b'_'
            && b != b'.'
            && b != b'@'
            && b != b'-'
        {
            return 0;
        }
    }

    // A scoped "@a.b" group records "@a" as its parent, creating it if
    // it does not exist yet.
    let mut scoped_parent = 0;
    if name.len() > 1 && name[0] == b'@' {
        let delim = crate::memory::xmemrchr(name, b'.');
        if let Some(delim) = delim {
            // SAFETY: forwarded from this function's own safety doc.
            scoped_parent = unsafe { syn_check_group(&name[..delim]) };
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    let table = unsafe { HL_TABLE.get_mut() };
    if table.ga_len() >= MAX_HL_ID {
        return 0;
    }

    let name_u = upper_name(name);

    table.items.push(HlGroup {
        sg_name: name.to_vec(),
        sg_name_u: name_u.clone(),
        // Cleared until the caller adds settings.
        sg_cleared: true,
        sg_rgb_fg: -1,
        sg_rgb_bg: -1,
        sg_rgb_sp: -1,
        sg_rgb_fg_idx: COLOR_IDX_NONE,
        sg_rgb_bg_idx: COLOR_IDX_NONE,
        sg_rgb_sp_idx: COLOR_IDX_NONE,
        sg_blend: -1,
        sg_parent: scoped_parent,
        ..Default::default()
    });

    // The ID is the index plus one.
    let id = table.ga_len();
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { HIGHLIGHT_UNAMES.get_mut() }.insert(name_u, id);
    id
}

/// Look up a highlight group by name, creating it if it does not
/// exist yet, and return its 1-based ID (`syn_check_group`).
///
/// Returns `0` on failure, including a name longer than
/// [`MAX_SYN_NAME`] (the original's own length check, whose message is
/// omitted).
///
/// # Safety
/// Same as [`syn_add_group`].
pub unsafe fn syn_check_group(name: &[u8]) -> i32 {
    if name.len() > MAX_SYN_NAME {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let id = unsafe { syn_name2id_len(name) };
    if id == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { syn_add_group(name) }
    } else {
        id
    }
}

/// Look up a highlight group by name and return its 1-based ID
/// (`syn_name2id`).
///
/// A name beginning with `@` is a tree-sitter capture and is looked up
/// through [`syn_check_group`], which CREATES it when absent - so this
/// can have a side effect for those names, unlike for ordinary ones.
/// That asymmetry is the original's: looking up `@aaa.bbb` has to
/// consider `@aaa` as well.
///
/// # Safety
/// Same as [`syn_add_group`].
#[must_use]
pub unsafe fn syn_name2id(name: &[u8]) -> i32 {
    if name.first() == Some(&b'@') {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { syn_check_group(name) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { syn_name2id_len(name) }
}

/// The number of highlight groups currently defined
/// (`highlight_num_groups`).
///
/// # Safety
/// Reads the [`HL_TABLE`] file-static.
#[must_use]
pub unsafe fn highlight_num_groups() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { HL_TABLE.get_mut() }.ga_len()
}

/// The name of the highlight group at table index `id`
/// (`highlight_group_name`).
///
/// **`id` is 0-BASED here**, unlike [`syn_id2name`]'s 1-based group
/// ID: the original indexes `hl_table[id]` directly rather than
/// `hl_table[id - 1]`. That difference is real and deliberate - this
/// is an index into the table, not a group ID - so it is preserved
/// rather than harmonised.
///
/// The original does no bounds check and relies on the caller passing
/// a valid index; indexing here panics instead of reading out of
/// bounds, which is the same contract for every valid input and
/// strictly safer for an invalid one.
///
/// # Safety
/// Reads the [`HL_TABLE`] file-static. `id` must be a valid index,
/// i.e. `0 <= id < highlight_num_groups()`.
#[must_use]
pub unsafe fn highlight_group_name(id: i32) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { HL_TABLE.get_mut() }.items[id as usize].sg_name.clone()
}

/// The ID of the group that the group at table index `id` links to,
/// or `0` when it links to nothing (`highlight_link_id`).
///
/// `id` is 0-BASED, exactly as in [`highlight_group_name`].
///
/// # Safety
/// Same as [`highlight_group_name`].
#[must_use]
pub unsafe fn highlight_link_id(id: i32) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { HL_TABLE.get_mut() }.items[id as usize].sg_link
}

/// Whether the group at table index `idx` has any settings of its own
/// (`hl_has_settings`).
///
/// A cleared group never counts, whatever else it holds. Otherwise any
/// one of the attribute, cterm colour or RGB colour-index settings is
/// enough. A link only counts when `check_link` asks for it, which is
/// how callers distinguish "styled in its own right" from "merely
/// points at another group".
///
/// # Safety
/// Same as [`highlight_group_name`]: reads [`HL_TABLE`], and `idx`
/// must be a valid 0-based index.
#[must_use]
pub unsafe fn hl_has_settings(idx: i32, check_link: bool) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let g = &unsafe { HL_TABLE.get_mut() }.items[idx as usize];
    !g.sg_cleared
        && (g.sg_attr != 0
            || g.sg_cterm_fg != 0
            || g.sg_cterm_bg != 0
            || g.sg_rgb_fg_idx != COLOR_IDX_NONE
            || g.sg_rgb_bg_idx != COLOR_IDX_NONE
            || g.sg_rgb_sp_idx != COLOR_IDX_NONE
            || (check_link && (g.sg_set & sg_set::LINK) != 0))
}

/// Clear the highlighting for the group at table index `idx`
/// (`highlight_clear`).
///
/// Resets every attribute, colour and font setting, and marks the
/// group cleared.
///
/// Note the link is NOT simply dropped: it is restored to the group's
/// DEFAULT link, and the script context follows it to wherever that
/// default was set, so a `:highlight clear` returns the group to its
/// built-in state rather than to nothing. Groups with no default link
/// have `sg_deflink` of 0, so for them this does clear the link.
///
/// The original's `XFREE_CLEAR(sg_font)` becomes assigning `None`:
/// dropping the owned value is what frees it.
///
/// # Safety
/// Same as [`highlight_group_name`].
pub unsafe fn highlight_clear(idx: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = &mut unsafe { HL_TABLE.get_mut() }.items[idx as usize];

    g.sg_cleared = true;
    g.sg_attr = 0;
    g.sg_cterm = 0;
    g.sg_cterm_bold = false;
    g.sg_cterm_fg = 0;
    g.sg_cterm_bg = 0;
    g.sg_gui = 0;
    g.sg_rgb_fg = -1;
    g.sg_rgb_bg = -1;
    g.sg_rgb_sp = -1;
    g.sg_rgb_fg_idx = COLOR_IDX_NONE;
    g.sg_rgb_bg_idx = COLOR_IDX_NONE;
    g.sg_rgb_sp_idx = COLOR_IDX_NONE;
    g.sg_blend = -1;
    g.sg_font = None;

    // Restore the default link and the context it was set from.
    g.sg_link = g.sg_deflink;
    g.sg_script_ctx = g.sg_deflink_sctx;
}

/// Whether a highlight group with this name exists
/// (`highlight_exists`).
///
/// Note this is NOT a pure query for an `@` capture name: it goes
/// through [`syn_name2id`], which CREATES such a group - so asking
/// whether `@foo` exists makes it exist. That is the original's
/// behaviour, inherited from the same call.
///
/// # Safety
/// Same as [`syn_name2id`].
#[must_use]
pub unsafe fn highlight_exists(name: &[u8]) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let id = unsafe { syn_name2id(name) };
    id > 0
}

/// Look up a highlight group name and return its active attribute code
/// (`syn_name2attr`), or zero when the group does not exist.
///
/// # Safety
/// Forwards [`syn_name2id`] and [`syn_id2attr`]'s shared-state
/// requirements.
#[must_use]
pub unsafe fn syn_name2attr(name: &[u8]) -> i32 {
    let id = unsafe { syn_name2id(name) };
    if id == 0 {
        0
    } else {
        unsafe { syn_id2attr(id) }
    }
}

/// Whether highlight group `id` has attribute `flag` set, reported as
/// the original's `"1"` string or nothing (`highlight_has_attr`).
///
/// `modec` selects which attribute set to inspect: `'g'` reads the
/// GUI attributes, anything else the cterm ones.
///
/// **`id` is 1-BASED here**, unlike [`highlight_group_name`]'s 0-based
/// table index - this one indexes `hl_table[id - 1]`, exactly like
/// [`syn_id2name`]. The two conventions genuinely coexist in the
/// original and are preserved rather than harmonised.
///
/// The underline attributes are a mutually-exclusive GROUP, not
/// independent bits: when `flag` names one of them, the group's own
/// underline bits must equal `flag` exactly rather than merely
/// overlap it, so asking about `HL_UNDERLINE` on an undercurled group
/// answers no. Every other flag is a plain bit test.
///
/// Returns `None` for an out-of-range `id`, matching the original's
/// `NULL`.
///
/// # Safety
/// Reads the [`HL_TABLE`] file-static.
#[must_use]
pub unsafe fn highlight_has_attr(id: i32, flag: u32, modec: u8) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let table = unsafe { HL_TABLE.get_mut() };
    if id <= 0 || id > table.ga_len() {
        return None;
    }
    let g = &table.items[(id - 1) as usize];

    let attr = if modec == b'g' { g.sg_gui } else { g.sg_cterm };
    #[allow(clippy::cast_sign_loss)]
    let attr = attr as u32;

    let matched = if flag & crate::highlight_defs::HL_UNDERLINE_MASK != 0 {
        // Underline styles are exclusive: an exact match, not overlap.
        (attr & crate::highlight_defs::HL_UNDERLINE_MASK) == flag
    } else {
        (attr & flag) != 0
    };
    matched.then_some(b"1".as_slice())
}

/// Reset the `Normal` highlight group's colours to "unset"
/// (`restore_cterm_colors`).
///
/// The RGB values reset to `-1` (no colour) while the cterm ones reset
/// to `0`. That asymmetry is the original's: `0` is a valid cterm
/// colour number in general, but here it is the sentinel the rest of
/// the code treats as "not set", whereas the RGB values use `-1`.
///
/// # Safety
/// Touches the `NORMAL_FG`/`NORMAL_BG`/`NORMAL_SP` and
/// `CTERM_NORMAL_FG_COLOR`/`CTERM_NORMAL_BG_COLOR` file-statics in
/// [`crate::highlight`].
pub unsafe fn restore_cterm_colors() {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        *crate::highlight::NORMAL_FG.get_mut() = -1;
        *crate::highlight::NORMAL_BG.get_mut() = -1;
        *crate::highlight::NORMAL_SP.get_mut() = -1;
        *crate::highlight::CTERM_NORMAL_FG_COLOR.get_mut() = 0;
        *crate::highlight::CTERM_NORMAL_BG_COLOR.get_mut() = 0;
    }
}

/// The `idx`-th name offered when completing a highlight group
/// (`get_highlight_name_ext`), or `None` past the end.
///
/// The completion list is the group table followed by up to four
/// keyword entries, each included only when the corresponding
/// `include_*` counter says so. That layout means the keywords' own
/// indices SHIFT with the counters, which is why each arm adds the
/// preceding counters rather than a fixed offset.
///
/// A cleared group yields an empty name rather than being skipped:
/// entries are never removed from the table, so the caller filters
/// them out by the empty result instead of the indices moving.
///
/// `idx` is a 0-BASED index into that completion list, not a group ID.
///
/// # Safety
/// Reads the [`HL_TABLE`] file-static and `GLOBALS`' `include_*`
/// counters.
#[must_use]
pub unsafe fn get_highlight_name_ext(idx: i32, skip_cleared: bool) -> Option<Vec<u8>> {
    if idx < 0 {
        return None;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let table = unsafe { HL_TABLE.get_mut() };
    let len = table.ga_len();

    // Items are never removed from the table, so cleared ones are
    // reported as an empty name rather than shifting the indices.
    if skip_cleared && idx < len && table.items[idx as usize].sg_cleared {
        return Some(Vec::new());
    }

    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let (none, default, link) = (g.include_none, g.include_default, g.include_link);

    if idx == len && none != 0 {
        return Some(b"none".to_vec());
    }
    if idx == len + none && default != 0 {
        return Some(b"default".to_vec());
    }
    if idx == len + none + default && link != 0 {
        return Some(b"link".to_vec());
    }
    if idx == len + none + default + 1 && link != 0 {
        return Some(b"clear".to_vec());
    }
    if idx >= len {
        return None;
    }
    Some(table.items[idx as usize].sg_name.clone())
}

/// The `idx`-th highlight completion name (`get_highlight_name`).
///
/// This is the completion callback's ordinary form, which always
/// hides cleared groups by returning their empty-name marker.
///
/// # Safety
/// Same as [`get_highlight_name_ext`].
#[must_use]
pub unsafe fn get_highlight_name(idx: i32) -> Option<Vec<u8>> {
    unsafe { get_highlight_name_ext(idx, true) }
}

/// The name of highlight group `id`, or an empty name when there is no
/// such group (`syn_id2name`).
///
/// Group IDs are 1-BASED: `id` indexes the table at `id - 1`, and `0`
/// means "no group".
///
/// The original returns a borrowed `char *` into the table (or a
/// literal `""`). This returns an owned copy instead: the table lives
/// behind a `GlobalCell`, so handing out a borrow tied to it would
/// outlive the access it came from, and every real caller immediately
/// copies the name into a string or dictionary anyway.
///
/// # Safety
/// Reads the [`HL_TABLE`] file-static.
#[must_use]
pub unsafe fn syn_id2name(id: i32) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let table = unsafe { HL_TABLE.get_mut() };
    if id <= 0 || id > table.ga_len() {
        return Vec::new();
    }
    table.items[(id - 1) as usize].sg_name.clone()
}

/// Follow global and namespace highlight links to their final group ID
/// (`syn_ns_get_final_id`).
///
/// Returns whether a namespace definition participated in resolving
/// the link. At most 100 links are followed, matching the original's
/// loop guard.
///
/// # Safety
/// Reads the shared highlight registry and namespace/provider state.
pub unsafe fn syn_ns_get_final_id(ns_id: &mut i32, hl_id: &mut i32) -> bool {
    let table_len = unsafe { HL_TABLE.get_mut() }.ga_len();
    if *hl_id > table_len || *hl_id < 1 {
        *hl_id = 0;
        return false;
    }

    let mut current = *hl_id;
    let mut used = false;
    for _ in 0..100 {
        let (sg_set, sg_link, sg_cleared, sg_parent) = {
            let table = unsafe { HL_TABLE.get_mut() };
            let group = &table.items[(current - 1) as usize];
            (
                group.sg_set,
                group.sg_link,
                group.sg_cleared,
                group.sg_parent,
            )
        };

        let check =
            unsafe { crate::highlight::ns_get_hl(ns_id, current, true, sg_set != 0) };
        if check == 0 {
            *hl_id = current;
            return true;
        } else if check > 0 {
            used = true;
            current = check;
            continue;
        }

        if sg_link > 0 && sg_link <= table_len {
            current = sg_link;
        } else if sg_cleared && sg_parent > 0 {
            current = sg_parent;
        } else {
            break;
        }
    }

    *hl_id = current;
    used
}

/// Resolve one group ID to its namespace-aware attribute code
/// (`syn_ns_id2attr`).
///
/// `optional` prevents falling back to the global attribute when a
/// positive namespace does not define the group.
///
/// # Safety
/// Reads the shared highlight registry and namespace/provider state.
#[must_use]
pub unsafe fn syn_ns_id2attr(mut ns_id: i32, mut hl_id: i32, optional: &mut bool) -> i32 {
    if unsafe { syn_ns_get_final_id(&mut ns_id, &mut hl_id) } {
        *optional = false;
    }
    if hl_id < 1 {
        return 0;
    }
    let (sg_set, sg_attr) = {
        let table = unsafe { HL_TABLE.get_mut() };
        let Some(group) = table.items.get((hl_id - 1) as usize) else {
            return 0;
        };
        (group.sg_set, group.sg_attr)
    };

    let attr = unsafe { crate::highlight::ns_get_hl(&mut ns_id, hl_id, false, sg_set != 0) };
    if attr >= 0 || (*optional && ns_id > 0) {
        attr
    } else {
        sg_attr
    }
}

/// Translate a highlight group ID to its active attribute code
/// (`syn_id2attr`).
///
/// # Safety
/// Reads the shared highlight registry and namespace/provider state.
#[must_use]
pub unsafe fn syn_id2attr(hl_id: i32) -> i32 {
    let mut optional = false;
    unsafe { syn_ns_id2attr(-1, hl_id, &mut optional) }
}

/// Translate a group ID to its final linked group ID
/// (`syn_get_final_id`).
///
/// # Safety
/// `GLOBALS.curwin` must point at a live window, and the shared
/// highlight registry/provider state must be accessed serially.
#[must_use]
pub unsafe fn syn_get_final_id(mut hl_id: i32) -> i32 {
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    assert!(!curwin.is_null(), "syn_get_final_id: curwin is null");
    let mut ns_id = unsafe { (*curwin).w_ns_hl_active };
    let _ = unsafe { syn_ns_get_final_id(&mut ns_id, &mut hl_id) };
    hl_id
}

/// Recompute the interned attribute code for one highlight group
/// (`set_hl_attr`).
///
/// `idx` is the group's zero-based table index.
///
/// # Safety
/// Reads and mutates the shared highlight-group/attribute/font tables,
/// and may queue a cursor-mode-info UI update.
pub unsafe fn set_hl_attr(idx: i32) {
    let (
        cterm,
        cterm_fg,
        cterm_bg,
        gui,
        rgb_fg,
        rgb_bg,
        rgb_sp,
        rgb_fg_idx,
        rgb_bg_idx,
        rgb_sp_idx,
        blend,
        font,
    ) = {
        let table = unsafe { HL_TABLE.get_mut() };
        let group = &table.items[idx as usize];
        (
            group.sg_cterm,
            group.sg_cterm_fg,
            group.sg_cterm_bg,
            group.sg_gui,
            group.sg_rgb_fg,
            group.sg_rgb_bg,
            group.sg_rgb_sp,
            group.sg_rgb_fg_idx,
            group.sg_rgb_bg_idx,
            group.sg_rgb_sp_idx,
            group.sg_blend,
            group.sg_font.clone(),
        )
    };

    let mut attrs = crate::highlight_defs::HlAttrs {
        cterm_ae_attr: cterm,
        cterm_fg_color: cterm_fg as i16,
        cterm_bg_color: cterm_bg as i16,
        rgb_ae_attr: gui,
        rgb_fg_color: if rgb_fg_idx != COLOR_IDX_NONE {
            rgb_fg
        } else {
            -1
        },
        rgb_bg_color: if rgb_bg_idx != COLOR_IDX_NONE {
            rgb_bg
        } else {
            -1
        },
        rgb_sp_color: if rgb_sp_idx != COLOR_IDX_NONE {
            rgb_sp
        } else {
            -1
        },
        hl_blend: blend,
        ..Default::default()
    };
    if let Some(font) = font {
        attrs.font = unsafe { crate::highlight::hl_add_font_idx(&font) };
    }

    let attr = unsafe { crate::highlight::hl_get_syn_attr(0, idx + 1, attrs) };
    unsafe { HL_TABLE.get_mut() }.items[idx as usize].sg_attr = attr;

    if crate::cursor_shape::cursor_mode_uses_syn_id(idx + 1) {
        unsafe { crate::ui::ui_mode_info_set() };
    }
}

/// Refresh every highlight group's resolved colors and attribute ID
/// (`highlight_attr_set_all`).
///
/// Groups using symbolic `fg`/`bg` colors first pick up the current
/// `Normal` colors, then every group is reinterned through
/// [`set_hl_attr`].
///
/// # Safety
/// Mutates the shared highlight-group and attribute tables and reads
/// the shared `Normal` colors.
pub unsafe fn highlight_attr_set_all() {
    let normal_fg = unsafe { *crate::highlight::NORMAL_FG.get_mut() };
    let normal_bg = unsafe { *crate::highlight::NORMAL_BG.get_mut() };
    let len = unsafe { HL_TABLE.get_mut() }.ga_len();
    for idx in 0..len {
        {
            let group = &mut unsafe { HL_TABLE.get_mut() }.items[idx as usize];
            if group.sg_rgb_bg_idx == COLOR_IDX_FG {
                group.sg_rgb_bg = normal_fg;
            } else if group.sg_rgb_bg_idx == COLOR_IDX_BG {
                group.sg_rgb_bg = normal_bg;
            }
            if group.sg_rgb_fg_idx == COLOR_IDX_FG {
                group.sg_rgb_fg = normal_fg;
            } else if group.sg_rgb_fg_idx == COLOR_IDX_BG {
                group.sg_rgb_fg = normal_bg;
            }
            if group.sg_rgb_sp_idx == COLOR_IDX_FG {
                group.sg_rgb_sp = normal_fg;
            } else if group.sg_rgb_sp_idx == COLOR_IDX_BG {
                group.sg_rgb_sp = normal_bg;
            }
        }
        unsafe { set_hl_attr(idx) };
    }
}

/// Configure command-line completion for `:highlight`
/// (`set_context_in_highlight_cmd`).
///
/// The `:highlight Ni` animated listing is display-only and remains
/// with the message/UI pipeline; completion parsing and include flags
/// are complete.
///
/// # Safety
/// Mutates `GLOBALS.include_default`/`include_link`.
pub unsafe fn set_context_in_highlight_cmd(
    xp: &mut crate::cmdexpand_defs::ExpandT,
    argument: &[u8],
) {
    xp.xp_context = crate::cmdexpand_defs::ExpandContext::Highlight;
    xp.xp_pattern = Some(argument.to_vec());
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    globals.include_link = 2;
    globals.include_default = 1;

    if argument.is_empty() {
        return;
    }
    let mut start = 0;
    let mut end = crate::charset::skiptowhite(&argument[start..]) + start;
    if end == argument.len() {
        return;
    }

    unsafe { crate::globals::GLOBALS.get_mut() }.include_default = 0;
    if b"default".starts_with(&argument[start..end]) {
        start = end + crate::charset::skipwhite(&argument[end..]);
        xp.xp_pattern = Some(argument[start..].to_vec());
        end = start + crate::charset::skiptowhite(&argument[start..]);
    }
    if end == argument.len() {
        return;
    }

    unsafe { crate::globals::GLOBALS.get_mut() }.include_link = 0;
    let token = &argument[start..end];
    if b"link".starts_with(token) || b"clear".starts_with(token) {
        start = end + crate::charset::skipwhite(&argument[end..]);
        xp.xp_pattern = Some(argument[start..].to_vec());
        end = start + crate::charset::skiptowhite(&argument[start..]);
        if end != argument.len() {
            start = end + crate::charset::skipwhite(&argument[end..]);
            xp.xp_pattern = Some(argument[start..].to_vec());
            end = start + crate::charset::skiptowhite(&argument[start..]);
        }
    }
    if end != argument.len() {
        xp.xp_context = crate::cmdexpand_defs::ExpandContext::Nothing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight_defs::{HL_BOLD, HL_ITALIC};

    /// Installs a table of groups AND the matching uppercase name
    /// index, restoring both on drop even through a panic.
    struct HlTableGuard {
        saved: Vec<HlGroup>,
        saved_names: crate::map::Map<Vec<u8>, i32>,
    }

    impl HlTableGuard {
        fn with_names(names: &[&[u8]]) -> Self {
            let table = unsafe { HL_TABLE.get_mut() };
            let saved = std::mem::take(&mut table.items);
            table.items = names
                .iter()
                .map(|n| HlGroup {
                    sg_name: (*n).to_vec(),
                    sg_name_u: upper_name(n),
                    ..Default::default()
                })
                .collect();

            let unames = unsafe { HIGHLIGHT_UNAMES.get_mut() };
            let saved_names = std::mem::replace(unames, crate::map::Map::new());
            for (i, n) in names.iter().enumerate() {
                // IDs are 1-based, matching syn_id2name.
                unames.insert(upper_name(n), i as i32 + 1);
            }
            Self { saved, saved_names }
        }
    }

    impl Drop for HlTableGuard {
        fn drop(&mut self) {
            unsafe { HL_TABLE.get_mut() }.items = std::mem::take(&mut self.saved);
            *unsafe { HIGHLIGHT_UNAMES.get_mut() } =
                std::mem::replace(&mut self.saved_names, crate::map::Map::new());
        }
    }

    struct NamespaceStateGuard {
        definitions: crate::map::Map<
            crate::highlight_defs::ColorKey,
            crate::highlight_defs::ColorItem,
        >,
        providers: Vec<crate::decoration_defs::DecorProvider>,
        active: i32,
    }

    impl NamespaceStateGuard {
        fn empty() -> Self {
            let definitions = std::mem::replace(
                unsafe { crate::highlight::NS_HLS.get_mut() },
                crate::map::Map::new(),
            );
            let providers =
                std::mem::take(unsafe { crate::decoration_provider::DECOR_PROVIDERS.get_mut() });
            let active = unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() };
            unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() = 0 };
            Self {
                definitions,
                providers,
                active,
            }
        }
    }

    impl Drop for NamespaceStateGuard {
        fn drop(&mut self) {
            *unsafe { crate::highlight::NS_HLS.get_mut() } =
                std::mem::replace(&mut self.definitions, crate::map::Map::new());
            *unsafe { crate::decoration_provider::DECOR_PROVIDERS.get_mut() } =
                std::mem::take(&mut self.providers);
            unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() = self.active };
        }
    }

    struct CurwinGuard(*mut crate::buffer_defs::WinT);

    impl CurwinGuard {
        fn set(curwin: *mut crate::buffer_defs::WinT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let old = globals.curwin;
            globals.curwin = curwin;
            Self(old)
        }
    }

    impl Drop for CurwinGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.0;
        }
    }

    struct AttributeTableGuard(crate::map::Set<crate::highlight_defs::HlEntry>);

    impl AttributeTableGuard {
        fn empty() -> Self {
            Self(std::mem::replace(
                unsafe { crate::highlight::ATTR_ENTRIES.get_mut() },
                crate::map::Set::new(),
            ))
        }
    }

    impl Drop for AttributeTableGuard {
        fn drop(&mut self) {
            *unsafe { crate::highlight::ATTR_ENTRIES.get_mut() } =
                std::mem::replace(&mut self.0, crate::map::Set::new());
        }
    }

    struct NormalColorsGuard {
        fg: crate::highlight_defs::RgbValue,
        bg: crate::highlight_defs::RgbValue,
    }

    impl NormalColorsGuard {
        fn set(fg: i32, bg: i32) -> Self {
            let old = Self {
                fg: unsafe { *crate::highlight::NORMAL_FG.get_mut() },
                bg: unsafe { *crate::highlight::NORMAL_BG.get_mut() },
            };
            unsafe {
                *crate::highlight::NORMAL_FG.get_mut() = fg;
                *crate::highlight::NORMAL_BG.get_mut() = bg;
            }
            old
        }
    }

    struct TerminalColorsGuard(i32);

    impl TerminalColorsGuard {
        fn set(value: i32) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let old = globals.t_colors;
            globals.t_colors = value;
            Self(old)
        }
    }

    struct HighlightCompletionGuard {
        default_: i32,
        link: i32,
    }

    impl HighlightCompletionGuard {
        fn capture() -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            Self {
                default_: globals.include_default,
                link: globals.include_link,
            }
        }
    }

    impl Drop for HighlightCompletionGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.include_default = self.default_;
            globals.include_link = self.link;
        }
    }

    impl Drop for TerminalColorsGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.t_colors = self.0;
        }
    }

    impl Drop for NormalColorsGuard {
        fn drop(&mut self) {
            unsafe {
                *crate::highlight::NORMAL_FG.get_mut() = self.fg;
                *crate::highlight::NORMAL_BG.get_mut() = self.bg;
            }
        }
    }

    #[test]
    fn syn_ns_get_final_id_follows_global_links_and_scoped_parents() {
        let _lock = crate::globals::global_state_test_lock();
        let _table = HlTableGuard::with_names(&[b"A", b"B", b"C"]);
        let _namespace = NamespaceStateGuard::empty();
        {
            let table = unsafe { HL_TABLE.get_mut() };
            table.items[0].sg_link = 2;
            table.items[1].sg_link = 3;
        }
        let mut ns_id = 0;
        let mut hl_id = 1;
        assert!(!unsafe { syn_ns_get_final_id(&mut ns_id, &mut hl_id) });
        assert_eq!(hl_id, 3);

        {
            let table = unsafe { HL_TABLE.get_mut() };
            table.items[0].sg_link = 0;
            table.items[2].sg_cleared = true;
            table.items[2].sg_parent = 1;
        }
        hl_id = 3;
        assert!(!unsafe { syn_ns_get_final_id(&mut ns_id, &mut hl_id) });
        assert_eq!(hl_id, 1);
    }

    #[test]
    fn syn_ns_get_final_id_obeys_namespace_links_and_direct_breaks() {
        let _lock = crate::globals::global_state_test_lock();
        let _table = HlTableGuard::with_names(&[b"A", b"B"]);
        let _namespace = NamespaceStateGuard::empty();
        let _ = unsafe { crate::decoration_provider::get_decor_provider(4, true) };
        unsafe { crate::highlight::NS_HLS.get_mut() }.insert(
            crate::highlight_defs::ColorKey::new(4, 1),
            crate::highlight_defs::ColorItem {
                attr_id: -1,
                link_id: 2,
                version: -1,
                ..Default::default()
            },
        );
        let mut ns_id = 4;
        let mut hl_id = 1;
        assert!(unsafe { syn_ns_get_final_id(&mut ns_id, &mut hl_id) });
        assert_eq!(hl_id, 2);

        unsafe { crate::highlight::NS_HLS.get_mut() }.insert(
            crate::highlight_defs::ColorKey::new(4, 1),
            crate::highlight_defs::ColorItem {
                attr_id: 77,
                version: -1,
                ..Default::default()
            },
        );
        hl_id = 1;
        assert!(unsafe { syn_ns_get_final_id(&mut ns_id, &mut hl_id) });
        assert_eq!(hl_id, 1);
    }

    #[test]
    fn syn_ns_id2attr_uses_namespace_then_optional_or_global_fallback() {
        let _lock = crate::globals::global_state_test_lock();
        let _table = HlTableGuard::with_names(&[b"A"]);
        let _namespace = NamespaceStateGuard::empty();
        unsafe { HL_TABLE.get_mut() }.items[0].sg_attr = 11;
        let _ = unsafe { crate::decoration_provider::get_decor_provider(4, true) };

        let mut optional = false;
        assert_eq!(unsafe { syn_ns_id2attr(4, 1, &mut optional) }, 11);
        optional = true;
        assert_eq!(unsafe { syn_ns_id2attr(4, 1, &mut optional) }, -1);

        unsafe { crate::highlight::NS_HLS.get_mut() }.insert(
            crate::highlight_defs::ColorKey::new(4, 1),
            crate::highlight_defs::ColorItem {
                attr_id: 77,
                version: -1,
                ..Default::default()
            },
        );
        optional = true;
        assert_eq!(unsafe { syn_ns_id2attr(4, 1, &mut optional) }, 77);
        assert_eq!(unsafe { syn_id2attr(1) }, 11);
    }

    #[test]
    fn syn_get_final_id_uses_the_current_windows_namespace() {
        let _lock = crate::globals::global_state_test_lock();
        let _table = HlTableGuard::with_names(&[b"A", b"B"]);
        let _namespace = NamespaceStateGuard::empty();
        let _ = unsafe { crate::decoration_provider::get_decor_provider(5, true) };
        unsafe { crate::highlight::NS_HLS.get_mut() }.insert(
            crate::highlight_defs::ColorKey::new(5, 1),
            crate::highlight_defs::ColorItem {
                attr_id: -1,
                link_id: 2,
                version: -1,
                ..Default::default()
            },
        );
        let mut win = crate::buffer_defs::WinT {
            w_ns_hl_active: 5,
            ..Default::default()
        };
        let win_ptr = std::ptr::addr_of_mut!(win);
        let _curwin = CurwinGuard::set(win_ptr);

        let result = unsafe { syn_get_final_id(1) };

        assert_eq!(result, 2);
    }

    #[test]
    fn set_hl_attr_interns_all_explicit_group_attributes() {
        let _lock = crate::globals::global_state_test_lock();
        let _table = HlTableGuard::with_names(&[b"Comment"]);
        let _attrs = AttributeTableGuard::empty();
        {
            let group = &mut unsafe { HL_TABLE.get_mut() }.items[0];
            group.sg_cterm = crate::highlight_defs::HL_BOLD as i32;
            group.sg_cterm_fg = 4;
            group.sg_cterm_bg = 5;
            group.sg_gui = crate::highlight_defs::HL_ITALIC as i32;
            group.sg_rgb_fg = 0x11_22_33;
            group.sg_rgb_bg = 0x44_55_66;
            group.sg_rgb_sp = 0x77_88_99;
            group.sg_rgb_fg_idx = 1;
            group.sg_rgb_bg_idx = 2;
            group.sg_rgb_sp_idx = 3;
            group.sg_blend = 30;
        }

        unsafe { set_hl_attr(0) };

        let attr_id = unsafe { HL_TABLE.get_mut() }.items[0].sg_attr;
        assert!(attr_id > 0);
        let attrs = unsafe { crate::highlight::syn_attr2entry(attr_id) };
        assert_eq!(attrs.cterm_ae_attr, crate::highlight_defs::HL_BOLD as i32);
        assert_eq!((attrs.cterm_fg_color, attrs.cterm_bg_color), (4, 5));
        assert_eq!(attrs.rgb_ae_attr, crate::highlight_defs::HL_ITALIC as i32);
        assert_eq!(
            (attrs.rgb_fg_color, attrs.rgb_bg_color, attrs.rgb_sp_color),
            (0x11_22_33, 0x44_55_66, 0x77_88_99)
        );
        assert_eq!(attrs.hl_blend, 30);
    }

    #[test]
    fn set_hl_attr_leaves_unset_rgb_colors_at_minus_one() {
        let _lock = crate::globals::global_state_test_lock();
        let _table = HlTableGuard::with_names(&[b"Empty"]);
        let _attrs = AttributeTableGuard::empty();
        {
            let group = &mut unsafe { HL_TABLE.get_mut() }.items[0];
            group.sg_rgb_fg = 0;
            group.sg_rgb_bg = 0;
            group.sg_rgb_sp = 0;
            group.sg_rgb_fg_idx = COLOR_IDX_NONE;
            group.sg_rgb_bg_idx = COLOR_IDX_NONE;
            group.sg_rgb_sp_idx = COLOR_IDX_NONE;
        }

        unsafe { set_hl_attr(0) };

        assert_eq!(unsafe { HL_TABLE.get_mut() }.items[0].sg_attr, 0);
    }

    #[test]
    fn highlight_attr_set_all_resolves_symbolic_normal_colors() {
        let _lock = crate::globals::global_state_test_lock();
        let _table = HlTableGuard::with_names(&[b"One", b"Two"]);
        let _attrs = AttributeTableGuard::empty();
        let _colors = NormalColorsGuard::set(0x11_22_33, 0x44_55_66);
        {
            let table = unsafe { HL_TABLE.get_mut() };
            table.items[0].sg_rgb_fg_idx = COLOR_IDX_FG;
            table.items[0].sg_rgb_bg_idx = COLOR_IDX_BG;
            table.items[0].sg_rgb_sp_idx = COLOR_IDX_FG;
            table.items[1].sg_rgb_fg_idx = COLOR_IDX_HEX;
            table.items[1].sg_rgb_fg = 0x77_88_99;
        }

        unsafe { highlight_attr_set_all() };

        let table = unsafe { HL_TABLE.get_mut() };
        assert_eq!(
            (
                table.items[0].sg_rgb_fg,
                table.items[0].sg_rgb_bg,
                table.items[0].sg_rgb_sp,
            ),
            (0x11_22_33, 0x44_55_66, 0x11_22_33)
        );
        assert_eq!(table.items[1].sg_rgb_fg, 0x77_88_99);
        assert!(table.items[0].sg_attr > 0);
        assert!(table.items[1].sg_attr > 0);
    }

    #[test]
    fn name_to_ctermcolor_uses_the_active_terminal_palette() {
        let _lock = crate::globals::global_state_test_lock();
        let _colors = TerminalColorsGuard::set(256);
        assert_eq!(unsafe { name_to_ctermcolor(b"Brown") }, 130);
        assert_eq!(unsafe { name_to_ctermcolor(b"gray") }, 248);

        unsafe { crate::globals::GLOBALS.get_mut() }.t_colors = 88;
        assert_eq!(unsafe { name_to_ctermcolor(b"Brown") }, 32);
        assert_eq!(unsafe { name_to_ctermcolor(b"Gray") }, 84);

        unsafe { crate::globals::GLOBALS.get_mut() }.t_colors = 16;
        assert_eq!(unsafe { name_to_ctermcolor(b"Brown") }, 3);
        assert_eq!(unsafe { name_to_ctermcolor(b"Gray") }, 7);

        unsafe { crate::globals::GLOBALS.get_mut() }.t_colors = 8;
        assert_eq!(unsafe { name_to_ctermcolor(b"Brown") }, 3);
        assert_eq!(unsafe { name_to_ctermcolor(b"Gray") }, 7);
        assert_eq!(unsafe { name_to_ctermcolor(b"DarkGray") }, 0);
        assert_eq!(unsafe { name_to_ctermcolor(b"NONE") }, -1);
        assert_eq!(unsafe { name_to_ctermcolor(b"missing") }, -1);
    }

    #[test]
    fn lookup_color_reports_bold_for_light_colors_in_eight_color_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let _colors = TerminalColorsGuard::set(8);
        let mut bold = crate::types_defs::TriState::None;
        assert_eq!(lookup_color(15, true, &mut bold), 4);
        assert_eq!(bold, crate::types_defs::TriState::True);

        bold = crate::types_defs::TriState::None;
        assert_eq!(lookup_color(1, true, &mut bold), 4);
        assert_eq!(bold, crate::types_defs::TriState::False);
    }

    #[test]
    fn set_context_in_highlight_cmd_completes_groups_and_subcommands() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = HighlightCompletionGuard::capture();
        let mut xp = crate::cmdexpand_defs::ExpandT::default();

        unsafe { set_context_in_highlight_cmd(&mut xp, b"") };
        assert_eq!(xp.xp_context, crate::cmdexpand_defs::ExpandContext::Highlight);
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"".as_slice()));
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.include_default, 1);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.include_link, 2);

        unsafe { set_context_in_highlight_cmd(&mut xp, b"Err") };
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"Err".as_slice()));
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.include_default, 1);

        unsafe { set_context_in_highlight_cmd(&mut xp, b"default Err") };
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"Err".as_slice()));
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.include_default, 0);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.include_link, 2);
    }

    #[test]
    fn set_context_in_highlight_cmd_advances_link_and_clear_arguments() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = HighlightCompletionGuard::capture();
        let mut xp = crate::cmdexpand_defs::ExpandT::default();

        unsafe { set_context_in_highlight_cmd(&mut xp, b"link Source Tar") };
        assert_eq!(xp.xp_context, crate::cmdexpand_defs::ExpandContext::Highlight);
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"Tar".as_slice()));
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.include_link, 0);

        unsafe { set_context_in_highlight_cmd(&mut xp, b"clear Group extra") };
        assert_eq!(xp.xp_context, crate::cmdexpand_defs::ExpandContext::Highlight);
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"extra".as_slice()));

        unsafe { set_context_in_highlight_cmd(&mut xp, b"clear Group extra more") };
        assert_eq!(xp.xp_context, crate::cmdexpand_defs::ExpandContext::Nothing);
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"extra more".as_slice()));
    }

    /// IDs are 1-based, so the first group is id 1 and id 0 is "no
    /// group" rather than the first entry.
    #[test]
    fn syn_id2name_is_one_based() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"Normal", b"Comment"]);

        assert_eq!(unsafe { syn_id2name(1) }, b"Normal".to_vec());
        assert_eq!(unsafe { syn_id2name(2) }, b"Comment".to_vec());
        assert!(unsafe { syn_id2name(0) }.is_empty(), "0 means no group");
    }

    /// An out-of-range ID yields an empty name rather than failing, and
    /// the upper bound is inclusive of the last real group.
    #[test]
    fn syn_id2name_bounds_the_table_exactly() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"Normal", b"Comment"]);

        assert_eq!(unsafe { syn_id2name(2) }, b"Comment".to_vec(), "last group");
        assert!(unsafe { syn_id2name(3) }.is_empty(), "one past the end");
        assert!(unsafe { syn_id2name(9999) }.is_empty());
    }

    /// A negative ID is rejected like any other out-of-range value.
    #[test]
    fn syn_id2name_rejects_a_negative_id() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"Normal"]);
        assert!(unsafe { syn_id2name(-1) }.is_empty());
    }

    #[test]
    fn syn_id2name_returns_nothing_from_an_empty_table() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[]);
        assert!(unsafe { syn_id2name(1) }.is_empty());
    }

    #[test]
    fn sg_set_flag_values_match_the_original() {
        assert_eq!((sg_set::CTERM, sg_set::GUI, sg_set::LINK), (2, 4, 8));
    }

    // --- syn_name2id_len ---

    /// Lookup is case-insensitive and returns the same 1-based IDs
    /// that [`syn_id2name`] maps back.
    #[test]
    fn syn_name2id_len_is_case_insensitive_and_round_trips() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"Normal", b"Comment"]);

        assert_eq!(unsafe { syn_name2id_len(b"Normal") }, 1);
        assert_eq!(unsafe { syn_name2id_len(b"NORMAL") }, 1);
        assert_eq!(unsafe { syn_name2id_len(b"normal") }, 1);
        assert_eq!(unsafe { syn_name2id_len(b"Comment") }, 2);

        // Round-trip: the ID maps back to the ORIGINAL-cased name.
        let id = unsafe { syn_name2id_len(b"cOmMeNt") };
        assert_eq!(unsafe { syn_id2name(id) }, b"Comment".to_vec());
    }

    #[test]
    fn syn_name2id_len_returns_zero_for_an_unknown_name() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"Normal"]);
        assert_eq!(unsafe { syn_name2id_len(b"Nope") }, 0);
    }

    /// An empty name is rejected rather than matching anything.
    #[test]
    fn syn_name2id_len_rejects_an_empty_name() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"Normal"]);
        assert_eq!(unsafe { syn_name2id_len(b"") }, 0);
    }

    /// The `MAX_SYN_NAME` bound is part of the contract: a name of
    /// exactly that length is accepted, one byte longer is rejected.
    #[test]
    fn syn_name2id_len_bounds_the_name_length_exactly() {
        let _lock = crate::globals::global_state_test_lock();
        let at_limit = vec![b'a'; MAX_SYN_NAME];
        let too_long = vec![b'a'; MAX_SYN_NAME + 1];
        let _g = HlTableGuard::with_names(&[&at_limit, &too_long]);

        assert_eq!(unsafe { syn_name2id_len(&at_limit) }, 1, "exactly at the limit");
        assert_eq!(
            unsafe { syn_name2id_len(&too_long) },
            0,
            "one byte past the limit is rejected, even though it is in the table"
        );
    }

    // --- syn_add_group / syn_check_group / syn_name2id ---

    /// A new group is appended with a 1-based ID and the defaults the
    /// original sets explicitly (cleared, colours unset).
    #[test]
    fn syn_add_group_appends_with_the_documented_defaults() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[]);

        let id = unsafe { syn_add_group(b"MyGroup") };
        assert_eq!(id, 1, "IDs are 1-based");
        assert_eq!(unsafe { syn_id2name(1) }, b"MyGroup".to_vec());
        assert_eq!(unsafe { syn_name2id_len(b"mygroup") }, 1, "index updated too");

        let table = unsafe { HL_TABLE.get_mut() };
        let g = &table.items[0];
        assert!(g.sg_cleared, "cleared until settings are added");
        assert_eq!((g.sg_rgb_fg, g.sg_rgb_bg, g.sg_rgb_sp), (-1, -1, -1));
        assert_eq!(g.sg_rgb_fg_idx, COLOR_IDX_NONE);
        assert_eq!(g.sg_blend, -1);
        assert_eq!(g.sg_name_u, b"MYGROUP".to_vec());
    }

    /// The name charset is exactly ASCII alphanumerics plus `_`, `.`,
    /// `@` and `-`; anything else is refused.
    #[test]
    fn syn_add_group_accepts_only_the_documented_characters() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[]);

        for ok in [
            b"Plain".as_slice(),
            b"with_underscore".as_slice(),
            b"with.dot".as_slice(),
            b"@capture".as_slice(),
            b"with-dash".as_slice(),
            b"digits123".as_slice(),
        ] {
            assert_ne!(unsafe { syn_add_group(ok) }, 0, "{ok:?} should be accepted");
        }

        for bad in [
            b"has space".as_slice(),
            b"has#hash".as_slice(),
            b"has/slash".as_slice(),
        ] {
            assert_eq!(unsafe { syn_add_group(bad) }, 0, "{bad:?} should be refused");
        }
    }

    /// An unprintable character is refused (the original's E669).
    #[test]
    fn syn_add_group_refuses_an_unprintable_character() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[]);
        assert_eq!(unsafe { syn_add_group(b"bad\x01name") }, 0);
        assert_eq!(unsafe { HL_TABLE.get_mut() }.ga_len(), 0, "nothing appended");
    }

    /// A scoped `@a.b` group records `@a` as its parent, CREATING that
    /// parent on demand - the mutual recursion with syn_check_group.
    #[test]
    fn syn_add_group_creates_the_scoped_parent_on_demand() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[]);

        let child = unsafe { syn_add_group(b"@aaa.bbb") };
        let parent = unsafe { syn_name2id_len(b"@aaa") };

        assert_ne!(parent, 0, "the parent was created as a side effect");
        assert_ne!(child, 0);
        let table = unsafe { HL_TABLE.get_mut() };
        let child_group = &table.items[(child - 1) as usize];
        assert_eq!(child_group.sg_parent, parent, "parent recorded on the child");
        // The parent is created FIRST, so it gets the lower ID.
        assert!(parent < child);
    }

    /// A group with no dot has no scoped parent.
    #[test]
    fn syn_add_group_leaves_an_unscoped_group_without_a_parent() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[]);

        let id = unsafe { syn_add_group(b"@plain") };
        let table = unsafe { HL_TABLE.get_mut() };
        assert_eq!(table.items[(id - 1) as usize].sg_parent, 0);
    }

    #[test]
    fn syn_check_group_reuses_an_existing_group() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"Existing"]);

        assert_eq!(unsafe { syn_check_group(b"Existing") }, 1);
        assert_eq!(unsafe { syn_check_group(b"EXISTING") }, 1, "case-insensitive");
        assert_eq!(unsafe { HL_TABLE.get_mut() }.ga_len(), 1, "nothing appended");
    }

    #[test]
    fn syn_check_group_creates_a_missing_group() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"Existing"]);

        assert_eq!(unsafe { syn_check_group(b"Fresh") }, 2);
        assert_eq!(unsafe { HL_TABLE.get_mut() }.ga_len(), 2);
    }

    #[test]
    fn syn_check_group_refuses_an_over_long_name() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[]);
        let too_long = vec![b'a'; MAX_SYN_NAME + 1];
        assert_eq!(unsafe { syn_check_group(&too_long) }, 0);
        assert_eq!(unsafe { HL_TABLE.get_mut() }.ga_len(), 0);
    }

    /// `syn_name2id` is a pure lookup for an ordinary name, but an
    /// `@` capture name goes through syn_check_group and so CREATES
    /// the group. That asymmetry is the original's.
    #[test]
    fn syn_name2id_creates_only_for_an_at_capture_name() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[]);

        assert_eq!(unsafe { syn_name2id(b"Missing") }, 0, "ordinary name: no group");
        assert_eq!(unsafe { HL_TABLE.get_mut() }.ga_len(), 0, "and none created");

        let id = unsafe { syn_name2id(b"@capture") };
        assert_ne!(id, 0, "@ name resolves");
        assert_eq!(unsafe { HL_TABLE.get_mut() }.ga_len(), 1, "because it was created");
    }

    // --- highlight_num_groups / highlight_group_name / highlight_link_id ---

    #[test]
    fn highlight_num_groups_counts_the_table() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A", b"B", b"C"]);
        assert_eq!(unsafe { highlight_num_groups() }, 3);
    }

    #[test]
    fn highlight_num_groups_is_zero_for_an_empty_table() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[]);
        assert_eq!(unsafe { highlight_num_groups() }, 0);
    }

    /// `highlight_group_name` indexes the table DIRECTLY, so it is
    /// 0-based, while `syn_id2name` takes a 1-based group ID. The two
    /// therefore disagree by one for the same entry, and that is the
    /// original's behaviour rather than a mistake.
    #[test]
    fn highlight_group_name_is_zero_based_unlike_syn_id2name() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"First", b"Second"]);

        assert_eq!(unsafe { highlight_group_name(0) }, b"First".to_vec());
        assert_eq!(unsafe { highlight_group_name(1) }, b"Second".to_vec());

        // Same entry, different convention.
        assert_eq!(unsafe { syn_id2name(1) }, b"First".to_vec());
        assert_eq!(unsafe { syn_id2name(2) }, b"Second".to_vec());
    }

    #[test]
    fn highlight_link_id_reports_the_link_target() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A", b"B"]);

        assert_eq!(unsafe { highlight_link_id(0) }, 0, "unlinked by default");

        unsafe { HL_TABLE.get_mut() }.items[0].sg_link = 2;
        assert_eq!(unsafe { highlight_link_id(0) }, 2);
        assert_eq!(unsafe { highlight_link_id(1) }, 0, "the other is untouched");
    }

    // --- hl_has_settings / highlight_clear / highlight_exists ---

    /// A cleared group never counts as having settings, whatever else
    /// it holds.
    #[test]
    fn hl_has_settings_ignores_a_cleared_group() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);

        let table = unsafe { HL_TABLE.get_mut() };
        table.items[0].sg_cleared = true;
        table.items[0].sg_attr = 5;
        assert!(!unsafe { hl_has_settings(0, true) });
    }

    /// Each setting on its own is enough, so a check that missed one
    /// would fail here.
    #[test]
    fn hl_has_settings_accepts_each_setting_on_its_own() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);

        // Every case starts from a not-cleared group with nothing set.
        let reset = || {
            let table = unsafe { HL_TABLE.get_mut() };
            table.items[0] = HlGroup {
                sg_cleared: false,
                sg_rgb_fg_idx: COLOR_IDX_NONE,
                sg_rgb_bg_idx: COLOR_IDX_NONE,
                sg_rgb_sp_idx: COLOR_IDX_NONE,
                ..Default::default()
            };
        };

        reset();
        assert!(!unsafe { hl_has_settings(0, true) }, "nothing set");

        for setter in [
            (|g: &mut HlGroup| g.sg_attr = 1) as fn(&mut HlGroup),
            |g: &mut HlGroup| g.sg_cterm_fg = 1,
            |g: &mut HlGroup| g.sg_cterm_bg = 1,
            |g: &mut HlGroup| g.sg_rgb_fg_idx = 0,
            |g: &mut HlGroup| g.sg_rgb_bg_idx = 0,
            |g: &mut HlGroup| g.sg_rgb_sp_idx = 0,
        ] {
            reset();
            setter(&mut unsafe { HL_TABLE.get_mut() }.items[0]);
            assert!(unsafe { hl_has_settings(0, false) }, "one setting is enough");
        }
    }

    /// A link only counts when `check_link` asks for it - the flag is
    /// what separates "styled itself" from "points elsewhere".
    #[test]
    fn hl_has_settings_counts_a_link_only_when_asked() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);

        let table = unsafe { HL_TABLE.get_mut() };
        table.items[0] = HlGroup {
            sg_cleared: false,
            sg_set: sg_set::LINK,
            sg_rgb_fg_idx: COLOR_IDX_NONE,
            sg_rgb_bg_idx: COLOR_IDX_NONE,
            sg_rgb_sp_idx: COLOR_IDX_NONE,
            ..Default::default()
        };

        assert!(unsafe { hl_has_settings(0, true) }, "counted when asked");
        assert!(!unsafe { hl_has_settings(0, false) }, "not counted otherwise");
    }

    /// Clearing resets the styling and marks the group cleared.
    #[test]
    fn highlight_clear_resets_the_styling() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);

        {
            let g = &mut unsafe { HL_TABLE.get_mut() }.items[0];
            g.sg_cleared = false;
            g.sg_attr = 9;
            g.sg_cterm_fg = 3;
            g.sg_rgb_fg = 0x00ff00;
            g.sg_rgb_fg_idx = 4;
            g.sg_blend = 50;
            g.sg_font = Some(b"Mono".to_vec());
        }

        unsafe { highlight_clear(0) };

        let g = &unsafe { HL_TABLE.get_mut() }.items[0];
        assert!(g.sg_cleared);
        assert_eq!((g.sg_attr, g.sg_cterm_fg, g.sg_gui), (0, 0, 0));
        assert_eq!(g.sg_rgb_fg, -1);
        assert_eq!(g.sg_rgb_fg_idx, COLOR_IDX_NONE);
        assert_eq!(g.sg_blend, -1);
        assert_eq!(g.sg_font, None, "the font is released");
    }

    /// Clearing RESTORES the default link rather than dropping the
    /// link entirely, and the script context follows it to where that
    /// default was set.
    #[test]
    fn highlight_clear_restores_the_default_link_not_nothing() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);

        let deflink_ctx = crate::eval::typval_defs::SctxT {
            sc_sid: 42,
            ..Default::default()
        };
        {
            let g = &mut unsafe { HL_TABLE.get_mut() }.items[0];
            g.sg_link = 7; // currently linked somewhere else
            g.sg_deflink = 3; // but its default is group 3
            g.sg_deflink_sctx = deflink_ctx;
            g.sg_script_ctx = crate::eval::typval_defs::SctxT {
                sc_sid: 99,
                ..Default::default()
            };
        }

        unsafe { highlight_clear(0) };

        let g = &unsafe { HL_TABLE.get_mut() }.items[0];
        assert_eq!(g.sg_link, 3, "restored to the default link, not 0");
        assert_eq!(g.sg_script_ctx, deflink_ctx, "context follows the default");
    }

    /// With no default link, clearing does leave the group unlinked.
    #[test]
    fn highlight_clear_unlinks_a_group_with_no_default() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);

        unsafe { HL_TABLE.get_mut() }.items[0].sg_link = 7;
        unsafe { highlight_clear(0) };
        assert_eq!(unsafe { HL_TABLE.get_mut() }.items[0].sg_link, 0);
    }

    #[test]
    fn highlight_exists_reports_a_known_group() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"Normal"]);

        assert!(unsafe { highlight_exists(b"Normal") });
        assert!(unsafe { highlight_exists(b"NORMAL") }, "case-insensitive");
        assert!(!unsafe { highlight_exists(b"Nope") });
    }

    #[test]
    fn syn_name2attr_resolves_case_insensitive_names_or_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"Comment"]);
        let _namespace = NamespaceStateGuard::empty();
        unsafe { HL_TABLE.get_mut() }.items[0].sg_attr = 42;

        assert_eq!(unsafe { syn_name2attr(b"comment") }, 42);
        assert_eq!(unsafe { syn_name2attr(b"missing") }, 0);
    }

    /// Asking whether an `@` capture group exists CREATES it, so the
    /// answer is always true - inherited from syn_name2id.
    #[test]
    fn highlight_exists_creates_an_at_capture_group() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[]);

        assert!(unsafe { highlight_exists(b"@brand.new") });
        assert_ne!(
            unsafe { HL_TABLE.get_mut() }.ga_len(),
            0,
            "the query created the group"
        );
    }

    // --- highlight_has_attr ---

    #[test]
    fn highlight_has_attr_rejects_an_out_of_range_id() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);

        assert_eq!(unsafe { highlight_has_attr(0, HL_BOLD, b'g') }, None);
        assert_eq!(unsafe { highlight_has_attr(2, HL_BOLD, b'g') }, None);
        assert_eq!(unsafe { highlight_has_attr(-1, HL_BOLD, b'g') }, None);
    }

    /// `modec` picks which attribute set is read: `'g'` the GUI one,
    /// anything else the cterm one. Setting only one shows through
    /// only one.
    #[test]
    fn highlight_has_attr_selects_gui_or_cterm_by_modec() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);

        #[allow(clippy::cast_possible_wrap)]
        {
            unsafe { HL_TABLE.get_mut() }.items[0].sg_gui = HL_BOLD as i32;
        }

        assert_eq!(unsafe { highlight_has_attr(1, HL_BOLD, b'g') }, Some(b"1".as_slice()));
        assert_eq!(unsafe { highlight_has_attr(1, HL_BOLD, b'c') }, None, "cterm unset");
    }

    /// Underline styles are a mutually-exclusive GROUP: the bits must
    /// match exactly, so an undercurled group does NOT report a plain
    /// underline even though the two masks overlap.
    #[test]
    fn highlight_has_attr_matches_underline_styles_exactly() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);

        #[allow(clippy::cast_possible_wrap)]
        {
            unsafe { HL_TABLE.get_mut() }.items[0].sg_gui =
                crate::highlight_defs::HL_UNDERCURL as i32;
        }

        assert_eq!(
            unsafe { highlight_has_attr(1, crate::highlight_defs::HL_UNDERCURL, b'g') },
            Some(b"1".as_slice()),
            "the exact style matches"
        );
        assert_eq!(
            unsafe { highlight_has_attr(1, crate::highlight_defs::HL_UNDERLINE, b'g') },
            None,
            "a different underline style must NOT match, despite overlapping the mask"
        );
    }

    /// A non-underline flag is a plain bit test, so it matches when
    /// present alongside others.
    #[test]
    fn highlight_has_attr_bit_tests_other_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);

        #[allow(clippy::cast_possible_wrap)]
        {
            unsafe { HL_TABLE.get_mut() }.items[0].sg_cterm = (HL_BOLD | HL_ITALIC) as i32;
        }

        assert_eq!(unsafe { highlight_has_attr(1, HL_BOLD, b'c') }, Some(b"1".as_slice()));
        assert_eq!(unsafe { highlight_has_attr(1, HL_ITALIC, b'c') }, Some(b"1".as_slice()));
    }

    // --- restore_cterm_colors ---

    /// The RGB colours reset to -1 while the cterm ones reset to 0 -
    /// two different "unset" sentinels, which is the original's own
    /// asymmetry rather than an oversight.
    #[test]
    fn restore_cterm_colors_uses_two_different_unset_sentinels() {
        let _lock = crate::globals::global_state_test_lock();

        unsafe {
            *crate::highlight::NORMAL_FG.get_mut() = 0x11_22_33;
            *crate::highlight::NORMAL_BG.get_mut() = 0x44_55_66;
            *crate::highlight::NORMAL_SP.get_mut() = 0x77_88_99;
            *crate::highlight::CTERM_NORMAL_FG_COLOR.get_mut() = 7;
            *crate::highlight::CTERM_NORMAL_BG_COLOR.get_mut() = 8;
        }

        unsafe { restore_cterm_colors() };

        unsafe {
            assert_eq!(*crate::highlight::NORMAL_FG.get_mut(), -1);
            assert_eq!(*crate::highlight::NORMAL_BG.get_mut(), -1);
            assert_eq!(*crate::highlight::NORMAL_SP.get_mut(), -1);
            assert_eq!(*crate::highlight::CTERM_NORMAL_FG_COLOR.get_mut(), 0);
            assert_eq!(*crate::highlight::CTERM_NORMAL_BG_COLOR.get_mut(), 0);
        }
    }

    // --- get_highlight_name_ext ---

    /// Sets the three completion-inclusion counters, restoring them on
    /// drop even through a panic.
    struct IncludeGuard {
        none: i32,
        default_: i32,
        link: i32,
    }

    impl IncludeGuard {
        fn set(none: i32, default_: i32, link: i32) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let me = Self {
                none: g.include_none,
                default_: g.include_default,
                link: g.include_link,
            };
            g.include_none = none;
            g.include_default = default_;
            g.include_link = link;
            me
        }
    }

    impl Drop for IncludeGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.include_none = self.none;
            g.include_default = self.default_;
            g.include_link = self.link;
        }
    }

    #[test]
    fn get_highlight_name_ext_lists_the_groups_then_stops() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A", b"B"]);
        let _i = IncludeGuard::set(0, 0, 0);

        assert_eq!(unsafe { get_highlight_name_ext(0, false) }, Some(b"A".to_vec()));
        assert_eq!(unsafe { get_highlight_name_ext(1, false) }, Some(b"B".to_vec()));
        assert_eq!(unsafe { get_highlight_name_ext(2, false) }, None, "past the end");
        assert_eq!(unsafe { get_highlight_name_ext(-1, false) }, None);
    }

    /// The keyword entries sit AFTER the groups, and their indices
    /// shift with the counters rather than being fixed. With every
    /// counter on, the order is none, default, link, clear.
    #[test]
    fn get_highlight_name_ext_appends_the_keywords_in_order() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);
        let _i = IncludeGuard::set(1, 1, 1);

        assert_eq!(unsafe { get_highlight_name_ext(1, false) }, Some(b"none".to_vec()));
        assert_eq!(unsafe { get_highlight_name_ext(2, false) }, Some(b"default".to_vec()));
        assert_eq!(unsafe { get_highlight_name_ext(3, false) }, Some(b"link".to_vec()));
        assert_eq!(unsafe { get_highlight_name_ext(4, false) }, Some(b"clear".to_vec()));
        assert_eq!(unsafe { get_highlight_name_ext(5, false) }, None);
    }

    /// With "none" excluded, the remaining keywords MOVE DOWN by one -
    /// the indices are relative to the counters, not fixed slots.
    #[test]
    fn get_highlight_name_ext_shifts_the_keywords_when_none_is_excluded() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);
        let _i = IncludeGuard::set(0, 1, 1);

        assert_eq!(
            unsafe { get_highlight_name_ext(1, false) },
            Some(b"default".to_vec()),
            "default takes the slot none would have used"
        );
        assert_eq!(unsafe { get_highlight_name_ext(2, false) }, Some(b"link".to_vec()));
        assert_eq!(unsafe { get_highlight_name_ext(3, false) }, Some(b"clear".to_vec()));
    }

    /// "clear" is offered only alongside "link", since both are gated
    /// on the same counter.
    #[test]
    fn get_highlight_name_ext_omits_link_and_clear_together() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);
        let _i = IncludeGuard::set(1, 1, 0);

        assert_eq!(unsafe { get_highlight_name_ext(1, false) }, Some(b"none".to_vec()));
        assert_eq!(unsafe { get_highlight_name_ext(2, false) }, Some(b"default".to_vec()));
        assert_eq!(unsafe { get_highlight_name_ext(3, false) }, None, "no link");
        assert_eq!(unsafe { get_highlight_name_ext(4, false) }, None, "no clear");
    }

    /// A cleared group yields an EMPTY name rather than being skipped,
    /// so the indices of later entries do not move.
    #[test]
    fn get_highlight_name_ext_reports_a_cleared_group_as_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A", b"B"]);
        let _i = IncludeGuard::set(0, 0, 0);

        unsafe { HL_TABLE.get_mut() }.items[0].sg_cleared = true;

        assert_eq!(
            unsafe { get_highlight_name_ext(0, true) },
            Some(Vec::new()),
            "empty, not skipped"
        );
        assert_eq!(
            unsafe { get_highlight_name_ext(1, true) },
            Some(b"B".to_vec()),
            "the next entry keeps its index"
        );
        // Without skip_cleared the real name still comes through.
        assert_eq!(unsafe { get_highlight_name_ext(0, false) }, Some(b"A".to_vec()));
    }

    #[test]
    fn get_highlight_name_delegates_with_cleared_groups_hidden() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A", b"B"]);
        let _i = IncludeGuard::set(0, 0, 0);
        unsafe { HL_TABLE.get_mut() }.items[0].sg_cleared = true;

        assert_eq!(unsafe { get_highlight_name(0) }, Some(Vec::new()));
        assert_eq!(unsafe { get_highlight_name(1) }, Some(b"B".to_vec()));
        assert_eq!(unsafe { get_highlight_name(2) }, None);
    }

    #[test]
    fn get_highlight_name_keeps_completion_keywords() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = HlTableGuard::with_names(&[b"A"]);
        let _i = IncludeGuard::set(1, 1, 1);

        assert_eq!(unsafe { get_highlight_name(1) }, Some(b"none".to_vec()));
        assert_eq!(unsafe { get_highlight_name(2) }, Some(b"default".to_vec()));
    }

    #[test]
    fn max_hl_id_and_color_idx_none_match_the_original() {
        assert_eq!(MAX_HL_ID, 20000);
        assert_eq!(COLOR_IDX_NONE, -1);
    }

    #[test]
    fn max_syn_name_matches_the_original() {
        assert_eq!(MAX_SYN_NAME, 200);
    }
}
