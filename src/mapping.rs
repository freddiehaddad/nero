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
//! `langmap_mapchar[]` (the original's other, single-byte-only lookup
//! table) is initialized to its identity mapping by [`langmap_init`],
//! matching startup before any `'langmap'` value is parsed.
//!
//! Also translated: [`map_mode_to_chars`] - encodes a mode bitmask
//! (`MODE_INSERT`/`MODE_CMDLINE`/etc., already real via
//! `crate::state_defs::mode`) into the `:map`-family prefix string
//! (`!`/`i`/`l`/`c`/` `/`n`/`o`/`t`/`v`/`x`/`s`) - a pure computation
//! with no dependency on `MapblockT` at all. Returns an owned,
//! growable `Vec<u8>` instead of writing into the original's own
//! caller-provided fixed 7-byte buffer. Its only 2 real callers
//! (`showmap`/`ex_map_check_prefix` in `mapping.c`) both read
//! `mp->m_mode`, `MapblockT`'s own still-opaque field - translated
//! ahead of them anyway since it's small, simple, and mechanically
//! correct with no design freedom to get wrong, matching this
//! crate's established precedent (e.g. `ops.rs`'s `clear_oparg`/
//! `reset_lbr`/`restore_lbr`).
//!
//! Also translated: [`get_map_mode`] - the exact inverse of
//! [`map_mode_to_chars`], decoding a `:map`-family command name's own
//! leading mode-character(s) back into mode bits. Also has no real
//! translated caller yet (every real caller is one of `mapping.c`'s
//! own not-yet-translated command handlers) - harvested ahead of them
//! for the same reason as `map_mode_to_chars`.
//!
//! Deferred: everything else - the whole `:map`/`:unmap`/`:abbreviate`
//! command family and mapping-table storage/lookup (needs
//! `MapblockT`'s real fields), `ExpandMappings` (needs the regex
//! engine), `makemap`/`put_escstr` (real file I/O), `langmap_init`/
//! `did_set_langmap` (the `'langmap'` option callback itself).

use crate::globals::GlobalCell;
use crate::state_defs::mode;

/// File-static `langmap_mapga`: a sorted-by-`from` table of
/// `'langmap'` character remappings for multi-byte characters. Empty
/// by default, matching the original's own `GA_EMPTY_INIT_VALUE`.
static LANGMAP_MAPGA: GlobalCell<Vec<(i32, i32)>> = GlobalCell::new(Vec::new());

/// Resets both language-map tables to the identity mapping
/// (`langmap_init`).
pub fn langmap_init() {
    let mapchar = &mut unsafe { crate::globals::GLOBALS.get_mut() }.langmap_mapchar;
    for (i, slot) in mapchar.iter_mut().enumerate() {
        *slot = i as u8;
    }
    unsafe { LANGMAP_MAPGA.get_mut() }.clear();
}

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

/// Encode `m` (a `MODE_*` bitmask) into its `:map`-family prefix
/// string (`map_mode_to_chars`). Returns an owned `Vec<u8>` instead of
/// writing into the original's own caller-provided fixed 7-byte
/// buffer - see this module's own doc comment.
#[must_use]
#[allow(dead_code)] // no real translated caller yet - see this module's own doc comment
pub fn map_mode_to_chars(m: i32) -> Vec<u8> {
    let mut buf = Vec::new();
    if (m & (mode::INSERT as i32 | mode::CMDLINE as i32)) == (mode::INSERT as i32 | mode::CMDLINE as i32) {
        buf.push(b'!'); // :map!
    } else if m & mode::INSERT as i32 != 0 {
        buf.push(b'i'); // :imap
    } else if m & mode::LANGMAP as i32 != 0 {
        buf.push(b'l'); // :lmap
    } else if m & mode::CMDLINE as i32 != 0 {
        buf.push(b'c'); // :cmap
    } else if (m
        & (mode::NORMAL as i32 | mode::VISUAL as i32 | mode::SELECT as i32 | mode::OP_PENDING as i32))
        == (mode::NORMAL as i32 | mode::VISUAL as i32 | mode::SELECT as i32 | mode::OP_PENDING as i32)
    {
        buf.push(b' '); // :map
    } else {
        if m & mode::NORMAL as i32 != 0 {
            buf.push(b'n'); // :nmap
        }
        if m & mode::OP_PENDING as i32 != 0 {
            buf.push(b'o'); // :omap
        }
        if m & mode::TERMINAL as i32 != 0 {
            buf.push(b't'); // :tmap
        }
        if (m & (mode::VISUAL as i32 | mode::SELECT as i32)) == (mode::VISUAL as i32 | mode::SELECT as i32) {
            buf.push(b'v'); // :vmap
        } else {
            if m & mode::VISUAL as i32 != 0 {
                buf.push(b'x'); // :xmap
            }
            if m & mode::SELECT as i32 != 0 {
                buf.push(b's'); // :smap
            }
        }
    }

    buf
}

/// Determine the mapping mode bits from a `:map`-family command
/// name's own leading mode-character(s) (`get_map_mode`). Returns
/// `(mode_bits, bytes_consumed)` in place of the original's own
/// `char **cmdp` advancing-pointer out-parameter, matching this
/// crate's established C-out-parameter-to-owned-return convention.
///
/// `bytes_consumed` is `1` when a recognized single-letter mode prefix
/// (`i`/`l`/`c`/`n`/`v`/`x`/`s`/`o`/`t`) was found, or `0` when none
/// was (the "plain `:map`" default case) - matching the original's own
/// `p++`-then-conditional-`p--` structure exactly. The `'n'` check
/// specifically also looks at the SECOND byte (`cmd[1]`) to avoid
/// misreading a raw, un-prefix-stripped `"noremap"` command name as
/// the `:nmap`-specific prefix (real neovim's own `// avoid :noremap`
/// comment) - only `'n'` NOT immediately followed by `'o'` counts as
/// the `:nmap` prefix.
///
/// This is the exact inverse of [`map_mode_to_chars`] (which encodes
/// mode bits back INTO a prefix string) - see that function's own doc
/// comment for the full mode-bit vocabulary.
///
/// No real caller is translated yet (`do_mapclear`/`do_map`/`showmap`/
/// `ex_map_check_prefix`, none of `mapping.c`'s own command handlers),
/// so this is harvested ahead of them, matching this crate's
/// established precedent for a small, self-contained function with no
/// design freedom of its own.
#[must_use]
#[allow(dead_code)] // no real translated caller yet - see this function's own doc comment
pub fn get_map_mode(cmd: &[u8], forceit: bool) -> (i32, usize) {
    let modec = cmd.first().copied().unwrap_or(0);
    if modec == b'i' {
        (mode::INSERT as i32, 1) // :imap
    } else if modec == b'l' {
        (mode::LANGMAP as i32, 1) // :lmap
    } else if modec == b'c' {
        (mode::CMDLINE as i32, 1) // :cmap
    } else if modec == b'n' && cmd.get(1).copied() != Some(b'o') {
        (mode::NORMAL as i32, 1) // :nmap (but not :noremap)
    } else if modec == b'v' {
        (mode::VISUAL as i32 | mode::SELECT as i32, 1) // :vmap
    } else if modec == b'x' {
        (mode::VISUAL as i32, 1) // :xmap
    } else if modec == b's' {
        (mode::SELECT as i32, 1) // :smap
    } else if modec == b'o' {
        (mode::OP_PENDING as i32, 1) // :omap
    } else if modec == b't' {
        (mode::TERMINAL as i32, 1) // :tmap
    } else if forceit {
        (mode::INSERT as i32 | mode::CMDLINE as i32, 0) // :map !
    } else {
        (
            mode::VISUAL as i32 | mode::SELECT as i32 | mode::NORMAL as i32 | mode::OP_PENDING as i32,
            0,
        ) // :map
    }
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

    // --- langmap_init / langmap_adjust_mb (shared tables) ---

    struct LangmapStateGuard {
        mapchar: [u8; 256],
        mapga: Vec<(i32, i32)>,
    }

    impl LangmapStateGuard {
        fn save() -> Self {
            Self {
                mapchar: unsafe { crate::globals::GLOBALS.get_mut() }.langmap_mapchar,
                mapga: unsafe { LANGMAP_MAPGA.get_mut() }.clone(),
            }
        }
    }

    impl Drop for LangmapStateGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.langmap_mapchar = self.mapchar;
            *(unsafe { LANGMAP_MAPGA.get_mut() }) = std::mem::take(&mut self.mapga);
        }
    }

    #[test]
    fn langmap_init_builds_the_single_byte_identity_map_and_clears_multibyte_entries() {
        let _lock = global_state_test_lock();
        let _g = LangmapStateGuard::save();
        unsafe { crate::globals::GLOBALS.get_mut() }.langmap_mapchar = [0; 256];
        unsafe { LANGMAP_MAPGA.get_mut() }.push((0x4e00, 0x61));

        langmap_init();

        let map = unsafe { crate::globals::GLOBALS.get_mut() }.langmap_mapchar;
        assert_eq!(map[0], 0);
        assert_eq!(map[1], 1);
        assert_eq!(map[127], 127);
        assert_eq!(map[255], 255);
        assert!(unsafe { LANGMAP_MAPGA.get_mut() }.is_empty());
    }

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

    // --- map_mode_to_chars (pure, no shared state) ---

    #[test]
    fn map_mode_to_chars_insert_and_cmdline_is_bang() {
        assert_eq!(map_mode_to_chars(mode::INSERT as i32 | mode::CMDLINE as i32), b"!");
    }

    #[test]
    fn map_mode_to_chars_insert_alone_is_i() {
        assert_eq!(map_mode_to_chars(mode::INSERT as i32), b"i");
    }

    #[test]
    fn map_mode_to_chars_insert_and_langmap_only_reports_insert() {
        // The original's own `else if` chain checks INSERT before
        // LANGMAP, so LANGMAP is never even consulted once INSERT
        // matches.
        assert_eq!(map_mode_to_chars(mode::INSERT as i32 | mode::LANGMAP as i32), b"i");
    }

    #[test]
    fn map_mode_to_chars_langmap_alone_is_l() {
        assert_eq!(map_mode_to_chars(mode::LANGMAP as i32), b"l");
    }

    #[test]
    fn map_mode_to_chars_cmdline_alone_is_c() {
        assert_eq!(map_mode_to_chars(mode::CMDLINE as i32), b"c");
    }

    #[test]
    fn map_mode_to_chars_all_four_normal_modes_is_a_space() {
        let m = mode::NORMAL as i32 | mode::VISUAL as i32 | mode::SELECT as i32 | mode::OP_PENDING as i32;
        assert_eq!(map_mode_to_chars(m), b" ");
    }

    #[test]
    fn map_mode_to_chars_all_four_normal_modes_plus_terminal_is_still_a_space() {
        // The all-4 check only masks those specific bits, so an EXTRA
        // bit (TERMINAL) being set alongside them doesn't stop it
        // from matching - this is checked BEFORE the `else` block's
        // own per-bit terminal handling ever runs.
        let m = mode::NORMAL as i32
            | mode::VISUAL as i32
            | mode::SELECT as i32
            | mode::OP_PENDING as i32
            | mode::TERMINAL as i32;
        assert_eq!(map_mode_to_chars(m), b" ");
    }

    #[test]
    fn map_mode_to_chars_normal_alone_is_n() {
        assert_eq!(map_mode_to_chars(mode::NORMAL as i32), b"n");
    }

    #[test]
    fn map_mode_to_chars_normal_op_pending_terminal_is_not() {
        let m = mode::NORMAL as i32 | mode::OP_PENDING as i32 | mode::TERMINAL as i32;
        assert_eq!(map_mode_to_chars(m), b"not");
    }

    #[test]
    fn map_mode_to_chars_visual_and_select_together_is_v() {
        assert_eq!(map_mode_to_chars(mode::VISUAL as i32 | mode::SELECT as i32), b"v");
    }

    #[test]
    fn map_mode_to_chars_visual_alone_is_x() {
        assert_eq!(map_mode_to_chars(mode::VISUAL as i32), b"x");
    }

    #[test]
    fn map_mode_to_chars_select_alone_is_s() {
        assert_eq!(map_mode_to_chars(mode::SELECT as i32), b"s");
    }

    #[test]
    fn map_mode_to_chars_normal_and_visual_only_is_nx() {
        assert_eq!(map_mode_to_chars(mode::NORMAL as i32 | mode::VISUAL as i32), b"nx");
    }

    #[test]
    fn map_mode_to_chars_zero_is_empty() {
        assert_eq!(map_mode_to_chars(0), b"");
    }

    // --- get_map_mode (pure, no shared state) ---

    #[test]
    fn get_map_mode_insert() {
        assert_eq!(get_map_mode(b"imap", false), (mode::INSERT as i32, 1));
    }

    #[test]
    fn get_map_mode_langmap() {
        assert_eq!(get_map_mode(b"lmap", false), (mode::LANGMAP as i32, 1));
    }

    #[test]
    fn get_map_mode_cmdline() {
        assert_eq!(get_map_mode(b"cmap", false), (mode::CMDLINE as i32, 1));
    }

    #[test]
    fn get_map_mode_normal() {
        assert_eq!(get_map_mode(b"nmap", false), (mode::NORMAL as i32, 1));
    }

    #[test]
    fn get_map_mode_noremap_is_not_normal_specific() {
        // "noremap" starts with 'n' followed by 'o' - this must NOT be
        // read as the :nmap prefix (real neovim's own "avoid :noremap"
        // comment); it falls through to the plain ":map" default.
        assert_eq!(
            get_map_mode(b"noremap", false),
            (mode::VISUAL as i32 | mode::SELECT as i32 | mode::NORMAL as i32 | mode::OP_PENDING as i32, 0)
        );
    }

    #[test]
    fn get_map_mode_visual_and_select() {
        assert_eq!(get_map_mode(b"vmap", false), (mode::VISUAL as i32 | mode::SELECT as i32, 1));
    }

    #[test]
    fn get_map_mode_visual_only() {
        assert_eq!(get_map_mode(b"xmap", false), (mode::VISUAL as i32, 1));
    }

    #[test]
    fn get_map_mode_select() {
        assert_eq!(get_map_mode(b"smap", false), (mode::SELECT as i32, 1));
    }

    #[test]
    fn get_map_mode_op_pending() {
        assert_eq!(get_map_mode(b"omap", false), (mode::OP_PENDING as i32, 1));
    }

    #[test]
    fn get_map_mode_terminal() {
        assert_eq!(get_map_mode(b"tmap", false), (mode::TERMINAL as i32, 1));
    }

    #[test]
    fn get_map_mode_plain_map_without_forceit() {
        assert_eq!(
            get_map_mode(b"map", false),
            (mode::VISUAL as i32 | mode::SELECT as i32 | mode::NORMAL as i32 | mode::OP_PENDING as i32, 0)
        );
    }

    #[test]
    fn get_map_mode_plain_map_with_forceit() {
        // ":map!" - forceit is set by the caller (having already
        // recognized the trailing "!"), not derived from `cmd` itself.
        assert_eq!(get_map_mode(b"map", true), (mode::INSERT as i32 | mode::CMDLINE as i32, 0));
    }

    #[test]
    fn get_map_mode_empty_slice_behaves_like_an_unrecognized_prefix() {
        assert_eq!(
            get_map_mode(b"", false),
            (mode::VISUAL as i32 | mode::SELECT as i32 | mode::NORMAL as i32 | mode::OP_PENDING as i32, 0)
        );
        assert_eq!(get_map_mode(b"", true), (mode::INSERT as i32 | mode::CMDLINE as i32, 0));
    }

    #[test]
    fn get_map_mode_round_trips_through_map_mode_to_chars() {
        // get_map_mode is the exact inverse of map_mode_to_chars -
        // verify a representative sample encodes to a prefix that,
        // reparsed (with a trailing "map" appended, mimicking a real
        // command name), recovers the SAME mode bits.
        for (name, expected_mode) in [
            (&b"imap"[..], mode::INSERT as i32),
            (b"lmap", mode::LANGMAP as i32),
            (b"cmap", mode::CMDLINE as i32),
            (b"nmap", mode::NORMAL as i32),
            (b"vmap", mode::VISUAL as i32 | mode::SELECT as i32),
            (b"xmap", mode::VISUAL as i32),
            (b"smap", mode::SELECT as i32),
            (b"omap", mode::OP_PENDING as i32),
            (b"tmap", mode::TERMINAL as i32),
        ] {
            let (decoded_mode, _consumed) = get_map_mode(name, false);
            assert_eq!(decoded_mode, expected_mode, "get_map_mode({name:?})");
            let mut encoded = map_mode_to_chars(decoded_mode);
            encoded.extend_from_slice(b"map");
            let (re_decoded_mode, _) = get_map_mode(&encoded, false);
            assert_eq!(re_decoded_mode, expected_mode, "round trip for {name:?} via {encoded:?}");
        }
    }
}
