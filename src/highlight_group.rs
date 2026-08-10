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
pub static HIGHLIGHT_UNAMES: std::sync::LazyLock<GlobalCell<crate::map::Map<Vec<u8>, i32>>> =
    std::sync::LazyLock::new(|| GlobalCell::new(crate::map::Map::new()));

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
    let name_u = crate::strings::vim_strsave_up(name);
    // SAFETY: forwarded from this function's own safety doc. A missing
    // key yields 0, which is the original's own "no such group".
    unsafe { HIGHLIGHT_UNAMES.get_mut() }.get_or_default(&name_u)
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
                    sg_name_u: crate::strings::vim_strsave_up(n),
                    ..Default::default()
                })
                .collect();

            let unames = unsafe { HIGHLIGHT_UNAMES.get_mut() };
            let saved_names = std::mem::replace(unames, crate::map::Map::new());
            for (i, n) in names.iter().enumerate() {
                // IDs are 1-based, matching syn_id2name.
                unames.insert(crate::strings::vim_strsave_up(n), i as i32 + 1);
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

    #[test]
    fn max_syn_name_matches_the_original() {
        assert_eq!(MAX_SYN_NAME, 200);
    }
}
