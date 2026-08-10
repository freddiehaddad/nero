//! Translated from `src/nvim/highlight_group.c` (tractable core only).
//!
//! `highlight_group.c` (~3200 lines) owns the highlight-group
//! registry: the table every group name and its attributes live in,
//! plus `:highlight` command handling, colour parsing, group linking
//! and the attribute-set plumbing into the UI.
//!
//! Translated so far: the registry's own storage - [`HlGroup`], the
//! [`sg_set`] flags and the [`HL_TABLE`] file-static - together with
//! [`syn_id2name`], the ID-to-name lookup. That lookup is what several
//! already-translated call sites elsewhere were waiting on (for
//! example `match.rs`'s `f_getmatches` item conversion and
//! `f_matcharg`'s reporting branch).
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

#[cfg(test)]
mod tests {
    use super::*;

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
