//! Translated from `src/nvim/mapping.c` (a small, tractable slice).
//!
//! `mapping.c` (~2600 lines) implements the `:map`/`:unmap`/`:abbreviate`
//! command family, mapping-table storage and lookup
//! (`buf_do_map`/`map_to_exists`), and `'langmap'` character
//! remapping. Almost the entire file needs `MapblockT`'s real fields
//! (still an opaque placeholder), the regex engine (`ExpandMappings`),
//! or real file I/O (`makemap`) - none of which exist yet.
//!
//! Translated: [`langmap_adjust_mb`] (+ its own private
//! `langmap_set_entry`/`langmap_adjust_mb_impl` helpers) - the
//! `'langmap'`-remapping lookup for multi-byte characters (`c >=
//! 256`), a self-contained binary search over a sorted `(from, to)`
//! table with no dependency on `MapblockT`/the regex engine/file I/O.
//! `LANGMAP_MAPGA` (the original's own file-static `langmap_mapga`)
//! starts empty (matching the original's own `GA_EMPTY_INIT_VALUE`)
//! and is only ever populated by `did_set_langmap` (the `'langmap'`
//! option's callback, not yet translated), so [`langmap_adjust_mb`]
//! always returns its input unchanged today - a real, faithful
//! reflection of "`'langmap'` has never been configured", not a
//! hardcoded shortcut: the real binary-search algorithm is translated
//! in full (via `langmap_adjust_mb_impl`, directly unit-tested
//! against a manually-populated table), not stubbed.
//!
//! Note: `langmap_mapchar[]` (the original's OTHER, single-byte-only
//! `'langmap'` lookup table, `c < 256`, already a real
//! `crate::globals::Globals::langmap_mapchar` field) is deliberately
//! NOT exposed via its own lookup function here: unlike
//! `langmap_mapga`, `langmap_mapchar` currently defaults to all zeros
//! (this crate's usual "mirrors raw C zero-init" convention), NOT the
//! identity mapping the original's own `langmap_init()` would set up
//! (`langmap_init`/`did_set_langmap` are both still deferred) - every
//! real call site guards this table behind the original's own
//! `LANGMAP_ADJUST` macro (which itself first checks `*p_langmap` is
//! non-empty before ever indexing `langmap_mapchar[]`), so exposing a
//! bare, ungated lookup now would give an actively wrong answer
//! (mapping every character to NUL) rather than a faithful "nothing
//! configured yet" result - the same trap already identified and
//! avoided for `charclass()`/`mb_get_class`.
//!
//! Deferred: everything else - the whole `:map`/`:unmap`/`:abbreviate`
//! command family and mapping-table storage/lookup (needs
//! `MapblockT`'s real fields), `ExpandMappings` (needs the regex
//! engine), `makemap`/`put_escstr` (real file I/O), `langmap_init`/
//! `did_set_langmap` (the `'langmap'` option callback itself).

use crate::globals::GlobalCell;

/// File-static `langmap_mapga`: a sorted-by-`from` table of
/// `'langmap'` character remappings for multi-byte characters. Empty
/// by default, matching the original's own `GA_EMPTY_INIT_VALUE`.
static LANGMAP_MAPGA: GlobalCell<Vec<(i32, i32)>> = GlobalCell::new(Vec::new());

/// Search `entries` (sorted by `from`) for `from`; if found, update
/// its `to`; otherwise insert a new `(from, to)` entry at the correct
/// sorted position, keeping `entries` sorted (`langmap_set_entry`).
#[allow(dead_code)] // no real translated caller yet (did_set_langmap, its only real caller, isn't translated) - tested directly, matching this crate's established convention for private helpers harvested ahead of their real caller
fn langmap_set_entry(entries: &mut Vec<(i32, i32)>, from: i32, to: i32) {
    match entries.binary_search_by_key(&from, |&(f, _)| f) {
        Ok(idx) => entries[idx].1 = to,
        Err(idx) => entries.insert(idx, (from, to)),
    }
}

/// Apply `'langmap'` to character `c` using `entries` (sorted by
/// `from`), returning the mapped `to` value if found, else `c`
/// unchanged (the real `langmap_adjust_mb` algorithm, parameterized
/// over its own table for direct unit-testing).
#[must_use]
fn langmap_adjust_mb_impl(entries: &[(i32, i32)], c: i32) -> i32 {
    let mut a = 0usize;
    let mut b = entries.len();
    while a != b {
        let i = (a + b) / 2;
        let d = entries[i].0 - c;
        if d == 0 {
            return entries[i].1; // found matching entry
        }
        if d < 0 {
            a = i + 1;
        } else {
            b = i;
        }
    }
    c // no entry found, return "c" unmodified
}

/// Apply `'langmap'` to multi-byte character `c` and return the
/// result (`langmap_adjust_mb`). See this module's own doc comment
/// for why this always returns `c` unchanged today.
#[must_use]
pub fn langmap_adjust_mb(c: i32) -> i32 {
    // SAFETY: momentary read.
    langmap_adjust_mb_impl(unsafe { LANGMAP_MAPGA.get_mut() }, c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    // --- langmap_set_entry / langmap_adjust_mb_impl (pure, no shared state) ---

    #[test]
    fn set_entry_inserts_into_an_empty_table() {
        let mut entries = Vec::new();
        langmap_set_entry(&mut entries, 100, 200);
        assert_eq!(entries, vec![(100, 200)]);
    }

    #[test]
    fn set_entry_inserts_in_sorted_order() {
        let mut entries = Vec::new();
        langmap_set_entry(&mut entries, 300, 1);
        langmap_set_entry(&mut entries, 100, 2);
        langmap_set_entry(&mut entries, 200, 3);
        assert_eq!(entries, vec![(100, 2), (200, 3), (300, 1)]);
    }

    #[test]
    fn set_entry_updates_an_existing_from() {
        let mut entries = Vec::new();
        langmap_set_entry(&mut entries, 100, 1);
        langmap_set_entry(&mut entries, 100, 2);
        assert_eq!(entries, vec![(100, 2)]);
    }

    #[test]
    fn adjust_mb_impl_returns_c_unchanged_on_an_empty_table() {
        assert_eq!(langmap_adjust_mb_impl(&[], 0x4e00), 0x4e00);
    }

    #[test]
    fn adjust_mb_impl_finds_a_matching_entry() {
        let entries = vec![(100, 111), (200, 222), (300, 333)];
        assert_eq!(langmap_adjust_mb_impl(&entries, 200), 222);
        assert_eq!(langmap_adjust_mb_impl(&entries, 100), 111);
        assert_eq!(langmap_adjust_mb_impl(&entries, 300), 333);
    }

    #[test]
    fn adjust_mb_impl_returns_c_unchanged_when_not_found() {
        let entries = vec![(100, 111), (200, 222), (300, 333)];
        assert_eq!(langmap_adjust_mb_impl(&entries, 150), 150);
        assert_eq!(langmap_adjust_mb_impl(&entries, 50), 50);
        assert_eq!(langmap_adjust_mb_impl(&entries, 999), 999);
    }

    #[test]
    fn set_entry_then_adjust_mb_impl_round_trip() {
        let mut entries = Vec::new();
        langmap_set_entry(&mut entries, 0x4e00, 0x61);
        langmap_set_entry(&mut entries, 0x4e01, 0x62);
        assert_eq!(langmap_adjust_mb_impl(&entries, 0x4e00), 0x61);
        assert_eq!(langmap_adjust_mb_impl(&entries, 0x4e01), 0x62);
        assert_eq!(langmap_adjust_mb_impl(&entries, 0x4e02), 0x4e02);
    }

    // --- langmap_adjust_mb (touches the shared LANGMAP_MAPGA) ---

    #[test]
    fn langmap_adjust_mb_is_identity_by_default() {
        let _lock = global_state_test_lock();
        unsafe { LANGMAP_MAPGA.get_mut() }.clear();
        assert_eq!(langmap_adjust_mb(0x4e00), 0x4e00);
        assert_eq!(langmap_adjust_mb(0), 0);
    }

    #[test]
    fn langmap_adjust_mb_uses_a_populated_langmap_mapga() {
        let _lock = global_state_test_lock();
        unsafe { LANGMAP_MAPGA.get_mut() }.clear();
        langmap_set_entry(unsafe { LANGMAP_MAPGA.get_mut() }, 0x4e00, 0x7a);
        assert_eq!(langmap_adjust_mb(0x4e00), 0x7a);
        assert_eq!(langmap_adjust_mb(0x4e01), 0x4e01);
        unsafe { LANGMAP_MAPGA.get_mut() }.clear();
    }
}
