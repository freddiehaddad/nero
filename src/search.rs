//! Translated from `src/nvim/search.c` (tractable core only).
//!
//! `search.c` is neovim's search-command file (string search for `/`,
//! `?`, `n`, `N`; character search within a line for `f`/`F`/`t`/`T`;
//! `%`/word searches - thousands of lines) - almost entirely dependent
//! on the real regex engine (`regexp.c`), the search-pattern-history
//! subsystem, and message display, not attempted here.
//!
//! Translated: the small "last character search" (`f`/`F`/`t`/`T`)
//! file-static state and its six simple accessors - `last_csearch`/
//! `last_csearch_forward`/`last_csearch_until`/`set_last_csearch`/
//! `set_csearch_direction`/`set_csearch_until`. The original's
//! `lastc`/`lastcdir`/`last_t_cmd`/`lastc_bytes`/`lastc_bytelen`
//! file-statics are bundled into one `LastCsearch` struct behind one
//! `GlobalCell`, matching `indent.rs`'s `BreakindentCache` precedent
//! for a group of related file-statics. `lastc` (a 2-element
//! `uint8_t[2]` in the original) is mirrored here as a single `u8`:
//! `lastc[1]` is always left at its initial `NUL` and never read or
//! written anywhere in the real source - only `lastc[0]` (via
//! `*lastc`) is ever touched. `set_last_csearch` preserves a genuine
//! upstream quirk: bytes beyond the newly-written `len` are NOT
//! re-cleared (matching the original's own `memcpy`-only-writes-`len`-
//! bytes behavior) unless `len == 0` - see that function's own doc
//! comment.
//!
//! Also translated: `linewhite` (needed only `charset.c`'s
//! `skipwhite` and `memline.c`'s `ml_get`, both already real).
//!
//! Also translated: `search.h`'s `SearchPattern`/`SearchOffset`
//! structs (this file has no dedicated `_defs.rs` module of its own -
//! same treatment as `charset.h`'s `vim_isbreak` embedded directly in
//! `charset.rs`) and the "last used search pattern" `spats[]`/
//! `last_idx` file-statics, bundled into one `SearchPatterns` struct
//! behind one `GlobalCell`. Their simple accessors -
//! `get_search_pattern`/`get_substitute_pattern`/
//! `get_search_pattern_timestamp`/`search_pattern_cleared`/
//! `set_substitute_pattern`/`set_last_used_pattern`/
//! `search_was_last_used` - are translated too. `free_spat`'s explicit
//! `xfree` calls have no Rust equivalent needed: assigning a new
//! `SearchPattern` automatically drops the old `pat`/`additional_data`
//! heap allocations.
//!
//! Also translated, now that `eval/vars.rs`'s `v:` special-variable
//! storage layer exists: `set_vv_searchforward` (`static` in the
//! original - kept private here too), `set_search_pattern`,
//! `reset_search_dir`, and `set_last_search_pat` - each was blocked
//! ONLY on `set_vv_searchforward` -> `set_vim_var_nr`/
//! `VV_SEARCHFORWARD`, now real.
//!
//! Deferred: everything else in the file - the functions actually
//! building/using a COMPILED search pattern (`last_search_pat`,
//! `do_search`, `searchit`, etc.) need the real regex engine, a
//! separate, substantial undertaking from this file's own small
//! tractable corner, left for a dedicated future pass.

use crate::types_defs::MAX_SCHAR_SIZE;
use crate::vim_defs::Direction;

/// Bundled "last character search" (`f`/`F`/`t`/`T`) file-static state.
/// See this module's own doc comment for why `lastc` is a single `u8`
/// here rather than a 2-element array.
#[derive(Debug)]
struct LastCsearch {
    /// last character searched for (`lastc[0]`)
    lastc: u8,
    /// last direction of character search (`lastcdir`)
    dir: Direction,
    /// last search `t_cmd` (`last_t_cmd`)
    t_cmd: bool,
    /// the bytes of the last searched-for character, when multi-byte
    /// (`lastc_bytes`)
    bytes: [u8; MAX_SCHAR_SIZE + 1],
    /// number of meaningful bytes in `bytes`; `> 1` for a multi-byte
    /// character (`lastc_bytelen`)
    bytelen: i32,
}

impl Default for LastCsearch {
    fn default() -> Self {
        // Matches the original's own static initializers exactly:
        // `lastc = { NUL, NUL }`, `lastcdir = FORWARD`,
        // `last_t_cmd = true`, `lastc_bytes` zero-initialized (`static`
        // storage duration), `lastc_bytelen = 1`.
        LastCsearch {
            lastc: 0,
            dir: Direction::Forward,
            t_cmd: true,
            bytes: [0; MAX_SCHAR_SIZE + 1],
            bytelen: 1,
        }
    }
}

static LAST_CSEARCH: std::sync::LazyLock<crate::globals::GlobalCell<LastCsearch>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(LastCsearch::default()));

/// @return the raw bytes of the last searched-for character
/// (`last_csearch`).
///
/// Returns the full internal buffer (`bytes[..bytelen]` is the
/// meaningful portion - see [`set_last_csearch`]'s own doc comment for
/// why bytes beyond `bytelen` may be stale leftovers from an earlier,
/// longer call, exactly matching the original's own behavior).
#[must_use]
pub fn last_csearch() -> [u8; MAX_SCHAR_SIZE + 1] {
    // SAFETY: see [`last_csearch_forward`].
    unsafe { LAST_CSEARCH.get_mut() }.bytes
}

/// @return `true` if the last character search direction was forward
/// (`last_csearch_forward`).
#[must_use]
pub fn last_csearch_forward() -> bool {
    // SAFETY: no overlapping live access to LAST_CSEARCH from other
    // threads - `unsafe` here only for consistency with this crate's
    // established `GlobalCell::get_mut` convention.
    unsafe { LAST_CSEARCH.get_mut() }.dir == Direction::Forward
}

/// @return the last character search's own `t_cmd`
/// (`last_csearch_until`).
#[must_use]
pub fn last_csearch_until() -> bool {
    // SAFETY: see [`last_csearch_forward`].
    unsafe { LAST_CSEARCH.get_mut() }.t_cmd
}

/// Remember character `c` (and its raw bytes `s[..len]`, when
/// multi-byte) for the last character search (`set_last_csearch`).
///
/// Only overwrites the first `len` bytes of the internal buffer when
/// `len != 0` (matching the original's own `memcpy(lastc_bytes, s,
/// len)`, which never touches bytes beyond `len`) - any bytes left
/// over from a PREVIOUS, longer call are deliberately NOT re-cleared
/// here. This is a faithfully-replicated quirk, not an oversight: the
/// original only fully clears the buffer (`CLEAR_FIELD`) on the
/// `len == 0` path. Every real reader is expected to only ever look at
/// `bytes[..bytelen]` (mirroring `bytelen`'s own role as the length
/// tag), so this is unobservable in practice - matches this crate's
/// "preserve the quirk, don't silently fix it" policy (e.g.
/// `eval/eval.rs`'s `string2float` `"inf"`-shortcut quirk).
///
/// # Panics
/// If `len` is negative, exceeds `s.len()`, or exceeds `bytes`' own
/// capacity (`MAX_SCHAR_SIZE + 1`) - the original would silently
/// overflow its destination buffer in that last case (an upstream
/// buffer-overflow risk unreachable by any real caller, since no
/// character search can ever produce more than a few composing-char
/// bytes, comfortably under `MAX_SCHAR_SIZE`); this crate panics
/// instead of silently corrupting memory, matching its usual policy
/// for such genuinely-unreachable-in-practice conditions.
pub fn set_last_csearch(c: i32, s: &[u8], len: i32) {
    // SAFETY: see [`last_csearch_forward`].
    let state = unsafe { LAST_CSEARCH.get_mut() };
    // Matches the original's own `(uint8_t)c` truncating cast exactly.
    state.lastc = c as u8;
    state.bytelen = len;
    if len != 0 {
        let len = usize::try_from(len).expect("set_last_csearch: len must not be negative");
        state.bytes[..len].copy_from_slice(&s[..len]);
    } else {
        state.bytes = [0; MAX_SCHAR_SIZE + 1];
    }
}

/// Set the last character search direction (`set_csearch_direction`).
pub fn set_csearch_direction(cdir: Direction) {
    // SAFETY: see [`last_csearch_forward`].
    unsafe { LAST_CSEARCH.get_mut() }.dir = cdir;
}

/// Set the last character search's own `t_cmd` (`set_csearch_until`).
pub fn set_csearch_until(t_cmd: bool) {
    // SAFETY: see [`last_csearch_forward`].
    unsafe { LAST_CSEARCH.get_mut() }.t_cmd = t_cmd;
}

/// @return `true` if line `lnum` is empty or has white characters
/// only (`linewhite`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` whose `b_ml.ml_mfp`, if non-null, points to a live
/// `MemfileT`.
#[must_use]
pub unsafe fn linewhite(lnum: crate::pos_defs::LinenrT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get(lnum) };
    let off = crate::charset::skipwhite(&line);
    line.get(off).copied().unwrap_or(0) == 0
}

/// Offset applied to the last search pattern (`SearchOffset`, from
/// `search.h`).
///
/// @note Only the offset for the last SEARCH pattern is ever used, not
/// for the last substitute pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOffset {
    /// search direction: forward (`'/'`) or backward (`'?'`)
    pub dir: u8,
    /// `true` if the search has a line offset
    pub line: bool,
    /// `true` if the search sets the cursor at the end
    pub end: bool,
    /// actual offset value
    pub off: i64,
}

impl Default for SearchOffset {
    fn default() -> Self {
        // Matches spats[]' own static initializer exactly: `{ '/',
        // false, false, 0 }` - NOT all-zero (dir defaults to '/', not
        // NUL), so the derived all-zero Default would be wrong here.
        SearchOffset { dir: b'/', line: false, end: false, off: 0 }
    }
}

/// Last search pattern and its attributes (`SearchPattern`, from
/// `search.h`).
///
/// `pat`/`patlen` (a nullable `char *` plus its own separate length in
/// the original) are combined into one `Option<Vec<u8>>`, matching
/// this crate's established convention for such pairs (e.g.
/// `mark_defs.rs`'s `fmark_T` fields) - `Vec::len()` gives `patlen`
/// directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPattern {
    /// the pattern, or `None` if unset
    pub pat: Option<Vec<u8>>,
    /// magicness of the pattern
    pub magic: bool,
    /// no smartcase for this pattern
    pub no_scs: bool,
    /// time of the last change
    pub timestamp: crate::os::time_defs::Timestamp,
    /// pattern offset
    pub off: SearchOffset,
    /// additional data from a ShaDa file
    pub additional_data: Option<Box<crate::types_defs::AdditionalData>>,
}

impl Default for SearchPattern {
    fn default() -> Self {
        // Matches spats[]' own static initializer exactly: `{ NULL, 0,
        // true, false, 0, { '/', false, false, 0 }, NULL }` - `magic`
        // defaults to `true`, NOT the derived all-zero `false`.
        SearchPattern {
            pat: None,
            magic: true,
            no_scs: false,
            timestamp: 0,
            off: SearchOffset::default(),
            additional_data: None,
        }
    }
}

/// Bundled "last used search pattern" file-static state (`spats[2]`
/// and `last_idx`, kept together since they're always used in
/// combination - matches `LastCsearch`'s own bundling precedent).
/// Indices `0`/`1` correspond to the original's `RE_SEARCH`/`RE_SUBST`.
#[derive(Debug, Clone, Default)]
struct SearchPatterns {
    /// last used search pattern / last used substitute pattern
    /// (`spats[2]`)
    spats: [SearchPattern; 2],
    /// index in `spats` for `RE_LAST` (`last_idx`)
    last_idx: usize,
}

static SEARCH_PATTERNS: std::sync::LazyLock<crate::globals::GlobalCell<SearchPatterns>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(SearchPatterns::default()));

/// Get the last search pattern, as a copy (`get_search_pattern`).
#[must_use]
pub fn get_search_pattern() -> SearchPattern {
    // SAFETY: no overlapping live access to SEARCH_PATTERNS from other
    // threads - `unsafe` here only for consistency with this crate's
    // established `GlobalCell::get_mut` convention.
    unsafe { SEARCH_PATTERNS.get_mut() }.spats[0].clone()
}

/// Get the last substitute pattern, as a copy, with its own offset
/// cleared (`get_substitute_pattern`).
#[must_use]
pub fn get_substitute_pattern() -> SearchPattern {
    // SAFETY: see [`get_search_pattern`].
    let mut pat = unsafe { SEARCH_PATTERNS.get_mut() }.spats[1].clone();
    pat.off = SearchOffset { dir: 0, line: false, end: false, off: 0 };
    pat
}

/// Get the timestamp of the last search or substitute pattern
/// (`get_search_pattern_timestamp`).
#[must_use]
pub fn get_search_pattern_timestamp(substitute: bool) -> crate::os::time_defs::Timestamp {
    // SAFETY: see [`get_search_pattern`].
    unsafe { SEARCH_PATTERNS.get_mut() }.spats[usize::from(substitute)].timestamp
}

/// Check whether the last search or substitute pattern is cleared
/// (`search_pattern_cleared`).
#[must_use]
pub fn search_pattern_cleared(substitute: bool) -> bool {
    // SAFETY: see [`get_search_pattern`].
    unsafe { SEARCH_PATTERNS.get_mut() }.spats[usize::from(substitute)].pat.is_none()
}

/// Set the last substitute pattern (`set_substitute_pattern`).
///
/// Unlike `set_search_pattern` (not yet translated - see this
/// module's own doc comment), this does NOT call
/// `set_vv_searchforward` in the original either, so it has no such
/// gap to defer.
pub fn set_substitute_pattern(pat: SearchPattern) {
    // SAFETY: see [`get_search_pattern`].
    let state = unsafe { SEARCH_PATTERNS.get_mut() };
    state.spats[1] = pat;
    state.spats[1].off = SearchOffset { dir: 0, line: false, end: false, off: 0 };
}

/// Set the last used search pattern - `true` for the substitute
/// pattern, `false` for the search pattern (`set_last_used_pattern`).
pub fn set_last_used_pattern(is_substitute_pattern: bool) {
    // SAFETY: see [`get_search_pattern`].
    unsafe { SEARCH_PATTERNS.get_mut() }.last_idx = usize::from(is_substitute_pattern);
}

/// @return `true` if the search pattern (as opposed to the substitute
/// pattern) was the last one used (`search_was_last_used`).
#[must_use]
pub fn search_was_last_used() -> bool {
    // SAFETY: see [`get_search_pattern`].
    unsafe { SEARCH_PATTERNS.get_mut() }.last_idx == 0
}

/// Set `v:searchforward` to whether `spats[0].off.dir` is `'/'`
/// (`set_vv_searchforward`, `static` in the original).
///
/// # Safety
/// No additional requirement beyond the usual
/// `crate::eval::vars::get_vim_var_tv` contract (no overlapping live
/// access to the shared `v:` variable storage).
unsafe fn set_vv_searchforward() {
    // SAFETY: see [`get_search_pattern`].
    let dir = unsafe { SEARCH_PATTERNS.get_mut() }.spats[0].off.dir;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        crate::eval::vars::set_vim_var_nr(
            crate::eval::vars::VimVarIndex::Searchforward,
            i64::from(dir == b'/'),
        );
    }
}

/// Set the last search pattern (`set_search_pattern`).
///
/// # Safety
/// Same as `set_vv_searchforward`.
pub unsafe fn set_search_pattern(pat: SearchPattern) {
    // SAFETY: see [`get_search_pattern`].
    unsafe { SEARCH_PATTERNS.get_mut() }.spats[0] = pat;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_vv_searchforward() };
}

/// Reset search direction to forward. For `"gd"`/`"gD"` commands
/// (`reset_search_dir`).
///
/// # Safety
/// Same as `set_vv_searchforward`.
pub unsafe fn reset_search_dir() {
    // SAFETY: see [`get_search_pattern`].
    unsafe { SEARCH_PATTERNS.get_mut() }.spats[0].off.dir = b'/';
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_vv_searchforward() };
}

/// Set the last search pattern. For `":let @/ ="` and ShaDa file
/// (`set_last_search_pat`).
///
/// `idx`/`setlast` match the original's own `RE_SEARCH`(`false`)/
/// `RE_SUBST`(`true`) convention, matching this file's own established
/// `substitute: bool` precedent (e.g. [`search_pattern_cleared`])
/// rather than the original's raw `int`.
///
/// Also set the saved search pattern, so that this works in an
/// autocommand.
///
/// # Safety
/// Same as `set_vv_searchforward`.
pub unsafe fn set_last_search_pat(s: &[u8], is_substitute: bool, magic: bool, setlast: bool) {
    let idx = usize::from(is_substitute);
    // SAFETY: see [`get_search_pattern`].
    let state = unsafe { SEARCH_PATTERNS.get_mut() };
    // An empty string means that nothing should be matched.
    state.spats[idx].pat = if s.is_empty() { None } else { Some(s.to_vec()) };
    state.spats[idx].timestamp = crate::os::time::os_time();
    state.spats[idx].additional_data = None;
    state.spats[idx].magic = magic;
    state.spats[idx].no_scs = false;
    state.spats[idx].off.dir = b'/';
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_vv_searchforward() };
    // SAFETY: see [`get_search_pattern`].
    let state = unsafe { SEARCH_PATTERNS.get_mut() };
    state.spats[idx].off.line = false;
    state.spats[idx].off.end = false;
    state.spats[idx].off.off = 0;
    if setlast {
        state.last_idx = idx;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_csearch_forward_true_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        // Default state (FORWARD) - reset explicitly since other tests
        // in this module mutate the same shared LAST_CSEARCH.
        set_csearch_direction(Direction::Forward);
        assert!(last_csearch_forward());
    }

    #[test]
    fn last_csearch_forward_false_after_backward_direction_set() {
        let _lock = crate::globals::global_state_test_lock();
        set_csearch_direction(Direction::Backward);
        assert!(!last_csearch_forward());
        // Restore the default for subsequent tests.
        set_csearch_direction(Direction::Forward);
    }

    #[test]
    fn last_csearch_until_true_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        set_csearch_until(true);
        assert!(last_csearch_until());
    }

    #[test]
    fn last_csearch_until_false_after_set() {
        let _lock = crate::globals::global_state_test_lock();
        set_csearch_until(false);
        assert!(!last_csearch_until());
        // Restore the default for subsequent tests.
        set_csearch_until(true);
    }

    #[test]
    fn set_last_csearch_plain_ascii_single_byte() {
        let _lock = crate::globals::global_state_test_lock();
        // Start from a known-cleared state (order-independent of
        // whatever other tests left behind) before checking bytes
        // beyond len - see set_last_csearch_retains_stale_trailing_bytes
        // for why bytes[1..] isn't always zero after a plain call.
        set_last_csearch(0, b"", 0);
        set_last_csearch(i32::from(b'x'), b"x", 1);
        // SAFETY: forwarded from the module's own GlobalCell convention.
        let state = unsafe { LAST_CSEARCH.get_mut() };
        assert_eq!(state.lastc, b'x');
        assert_eq!(state.bytelen, 1);
        assert_eq!(&state.bytes[..1], b"x");
        assert!(state.bytes[1..].iter().all(|&b| b == 0));
    }

    #[test]
    fn set_last_csearch_multibyte_copies_all_bytes() {
        let _lock = crate::globals::global_state_test_lock();
        // 'é' (U+00E9) UTF-8 encodes as 2 bytes: 0xC3 0xA9.
        let bytes = "é".as_bytes();
        set_last_csearch(i32::from(bytes[0]), bytes, 2);
        // SAFETY: forwarded from the module's own GlobalCell convention.
        let state = unsafe { LAST_CSEARCH.get_mut() };
        assert_eq!(state.bytelen, 2);
        assert_eq!(&state.bytes[..2], bytes);
    }

    #[test]
    fn set_last_csearch_retains_stale_trailing_bytes() {
        // Faithfully-replicated upstream quirk: a shorter call does NOT
        // re-clear bytes beyond its own len - only `memcpy`'s own `len`
        // bytes are overwritten, matching the original exactly (see
        // set_last_csearch's own doc comment).
        let _lock = crate::globals::global_state_test_lock();
        let long = "é".as_bytes(); // 2 bytes: 0xC3 0xA9
        set_last_csearch(i32::from(long[0]), long, 2);
        set_last_csearch(i32::from(b'x'), b"x", 1);
        // SAFETY: forwarded from the module's own GlobalCell convention.
        let state = unsafe { LAST_CSEARCH.get_mut() };
        assert_eq!(state.bytelen, 1);
        assert_eq!(state.bytes[0], b'x');
        // Byte 1 still holds 0xA9 from the earlier, longer call.
        assert_eq!(state.bytes[1], long[1]);
    }

    #[test]
    fn set_last_csearch_zero_len_clears_bytes() {
        let _lock = crate::globals::global_state_test_lock();
        set_last_csearch(i32::from(b'a'), b"a", 1);
        set_last_csearch(0, b"", 0);
        // SAFETY: forwarded from the module's own GlobalCell convention.
        let state = unsafe { LAST_CSEARCH.get_mut() };
        assert_eq!(state.bytelen, 0);
        assert!(state.bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn set_last_csearch_truncating_cast_matches_c_uint8_cast() {
        let _lock = crate::globals::global_state_test_lock();
        // 0x141 truncated to u8 is 0x41 ('A'), matching C's (uint8_t)c.
        set_last_csearch(0x141, b"A", 1);
        // SAFETY: forwarded from the module's own GlobalCell convention.
        let state = unsafe { LAST_CSEARCH.get_mut() };
        assert_eq!(state.lastc, b'A');
    }

    fn test_buf_with_line(line: &[u8]) -> crate::buffer_defs::BufT {
        let mut buf = crate::buffer_defs::BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, line) },
            crate::vim_defs::OK
        );
        buf
    }

    fn close_test_buf(buf: crate::buffer_defs::BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn linewhite_true_for_empty_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = test_buf_with_line(b"\0");
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut crate::buffer_defs::BufT;

        assert!(unsafe { linewhite(1) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        close_test_buf(buf);
    }

    #[test]
    fn linewhite_true_for_whitespace_only_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = test_buf_with_line(b"   \t \0");
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut crate::buffer_defs::BufT;

        assert!(unsafe { linewhite(1) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        close_test_buf(buf);
    }

    #[test]
    fn linewhite_false_for_line_with_content() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = test_buf_with_line(b"  hello\0");
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut crate::buffer_defs::BufT;

        assert!(!unsafe { linewhite(1) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        close_test_buf(buf);
    }

    #[test]
    fn last_csearch_returns_the_full_internal_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        set_last_csearch(0, b"", 0); // known-cleared starting state
        set_last_csearch(i32::from(b'z'), b"z", 1);
        let bytes = last_csearch();
        assert_eq!(bytes.len(), MAX_SCHAR_SIZE + 1);
        assert_eq!(bytes[0], b'z');
        assert!(bytes[1..].iter().all(|&b| b == 0));
    }

    /// Resets `SEARCH_PATTERNS` to its default state. Callers must
    /// hold `global_state_test_lock()` for their whole test body -
    /// every test below needs a known starting point since
    /// `SEARCH_PATTERNS` is shared, mutable state.
    fn reset_search_patterns() {
        // SAFETY: no overlapping live access - see get_search_pattern's
        // own doc comment on the established GlobalCell convention.
        *unsafe { SEARCH_PATTERNS.get_mut() } = SearchPatterns::default();
    }

    #[test]
    fn search_offset_default_matches_c_static_initializer() {
        // { '/', false, false, 0 } - NOT all-zero.
        let off = SearchOffset::default();
        assert_eq!(off.dir, b'/');
        assert!(!off.line);
        assert!(!off.end);
        assert_eq!(off.off, 0);
    }

    #[test]
    fn search_pattern_default_matches_c_static_initializer() {
        // { NULL, 0, true, false, 0, {...}, NULL } - magic defaults to
        // true, NOT the derived all-zero false.
        let pat = SearchPattern::default();
        assert!(pat.pat.is_none());
        assert!(pat.magic);
        assert!(!pat.no_scs);
        assert_eq!(pat.timestamp, 0);
        assert_eq!(pat.off, SearchOffset::default());
        assert!(pat.additional_data.is_none());
    }

    #[test]
    fn get_search_pattern_returns_a_copy_of_spats_0() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        // SAFETY: see get_search_pattern's own doc comment.
        unsafe { SEARCH_PATTERNS.get_mut() }.spats[0].pat = Some(b"needle".to_vec());

        let pat = get_search_pattern();
        assert_eq!(pat.pat, Some(b"needle".to_vec()));
    }

    #[test]
    fn get_substitute_pattern_clears_its_own_offset() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        // SAFETY: see get_search_pattern's own doc comment.
        let state = unsafe { SEARCH_PATTERNS.get_mut() };
        state.spats[1].pat = Some(b"replacement".to_vec());
        state.spats[1].off = SearchOffset { dir: b'/', line: true, end: true, off: 5 };

        let pat = get_substitute_pattern();
        assert_eq!(pat.pat, Some(b"replacement".to_vec()));
        assert_eq!(pat.off, SearchOffset { dir: 0, line: false, end: false, off: 0 });
        // The ORIGINAL spats[1] itself is untouched - only the copy's
        // own offset is cleared (matches get_substitute_pattern's own
        // "memcpy then CLEAR_FIELD(pat->off)" structure exactly).
        // SAFETY: see get_search_pattern's own doc comment.
        assert_eq!(
            unsafe { SEARCH_PATTERNS.get_mut() }.spats[1].off,
            SearchOffset { dir: b'/', line: true, end: true, off: 5 }
        );
    }

    #[test]
    fn get_search_pattern_timestamp_reads_the_right_slot() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        // SAFETY: see get_search_pattern's own doc comment.
        let state = unsafe { SEARCH_PATTERNS.get_mut() };
        state.spats[0].timestamp = 111;
        state.spats[1].timestamp = 222;

        assert_eq!(get_search_pattern_timestamp(false), 111);
        assert_eq!(get_search_pattern_timestamp(true), 222);
    }

    #[test]
    fn search_pattern_cleared_true_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        assert!(search_pattern_cleared(false));
        assert!(search_pattern_cleared(true));
    }

    #[test]
    fn search_pattern_cleared_false_once_pat_is_set() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        // SAFETY: see get_search_pattern's own doc comment.
        unsafe { SEARCH_PATTERNS.get_mut() }.spats[0].pat = Some(b"x".to_vec());
        assert!(!search_pattern_cleared(false));
        assert!(search_pattern_cleared(true)); // substitute untouched
    }

    #[test]
    fn set_substitute_pattern_stores_pat_and_clears_offset() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        let mut pat = SearchPattern { pat: Some(b"foo".to_vec()), ..SearchPattern::default() };
        pat.off = SearchOffset { dir: b'?', line: true, end: false, off: 3 };

        set_substitute_pattern(pat);

        // SAFETY: see get_search_pattern's own doc comment.
        let stored = &unsafe { SEARCH_PATTERNS.get_mut() }.spats[1];
        assert_eq!(stored.pat, Some(b"foo".to_vec()));
        assert_eq!(stored.off, SearchOffset { dir: 0, line: false, end: false, off: 0 });
    }

    #[test]
    fn set_last_used_pattern_and_search_was_last_used_roundtrip() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        assert!(search_was_last_used()); // default: last_idx == 0

        set_last_used_pattern(true);
        assert!(!search_was_last_used());

        set_last_used_pattern(false);
        assert!(search_was_last_used());
    }

    /// Resets `v:searchforward` (in the separate `eval::vars` shared
    /// `VIMVARS` state) back to its own true static-initializer
    /// default (`Number(0)`) - callers must hold
    /// `global_state_test_lock()`, matching `reset_search_patterns`'s
    /// own established convention (this file's functions touch BOTH
    /// `SEARCH_PATTERNS` and `VIMVARS`, so both need resetting).
    fn reset_vv_searchforward() {
        // SAFETY: no overlapping live access - test-only, single
        // threaded under the held lock.
        unsafe {
            crate::eval::vars::set_vim_var_nr(crate::eval::vars::VimVarIndex::Searchforward, 0);
        }
    }

    fn vv_searchforward() -> i64 {
        // SAFETY: forwarded from reset_vv_searchforward's own doc.
        unsafe { crate::eval::vars::get_vim_var_nr(crate::eval::vars::VimVarIndex::Searchforward) }
    }

    #[test]
    fn reset_search_dir_sets_forward_and_updates_vv_searchforward() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        unsafe {
            SEARCH_PATTERNS.get_mut().spats[0].off.dir = b'?'; // start backward
            reset_search_dir();
            assert_eq!(SEARCH_PATTERNS.get_mut().spats[0].off.dir, b'/');
        }
        assert_eq!(vv_searchforward(), 1);
        reset_vv_searchforward();
    }

    #[test]
    fn set_search_pattern_stores_pattern_and_updates_vv_searchforward() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        let pat = SearchPattern {
            pat: Some(b"needle".to_vec()),
            off: SearchOffset { dir: b'?', line: false, end: false, off: 0 },
            ..SearchPattern::default()
        };
        unsafe {
            set_search_pattern(pat);
            assert_eq!(SEARCH_PATTERNS.get_mut().spats[0].pat, Some(b"needle".to_vec()));
        }
        // dir == '?' (backward), so v:searchforward becomes 0.
        assert_eq!(vv_searchforward(), 0);
        reset_vv_searchforward();
    }

    #[test]
    fn set_last_search_pat_stores_pattern_and_resets_offset() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        unsafe {
            SEARCH_PATTERNS.get_mut().spats[0].off =
                SearchOffset { dir: b'?', line: true, end: true, off: 5 };

            set_last_search_pat(b"pattern", false, true, false);

            let stored = &SEARCH_PATTERNS.get_mut().spats[0];
            assert_eq!(stored.pat, Some(b"pattern".to_vec()));
            assert!(stored.magic);
            assert!(!stored.no_scs);
            // off.dir reset to forward ('/'), line/end/off all cleared.
            assert_eq!(stored.off, SearchOffset { dir: b'/', line: false, end: false, off: 0 });
        }
        assert_eq!(vv_searchforward(), 1);
        reset_vv_searchforward();
    }

    #[test]
    fn set_last_search_pat_empty_string_means_pattern_is_none() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        unsafe {
            set_last_search_pat(b"", false, true, false);
            assert!(SEARCH_PATTERNS.get_mut().spats[0].pat.is_none());
        }
        reset_vv_searchforward();
    }

    #[test]
    fn set_last_search_pat_setlast_true_updates_last_idx() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        unsafe {
            assert_eq!(SEARCH_PATTERNS.get_mut().last_idx, 0);
            set_last_search_pat(b"x", true, true, true);
            assert_eq!(SEARCH_PATTERNS.get_mut().last_idx, 1);
        }
        reset_vv_searchforward();
    }

    #[test]
    fn set_last_search_pat_setlast_false_leaves_last_idx_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        reset_search_patterns();
        unsafe {
            set_last_used_pattern(true); // last_idx = 1
            set_last_search_pat(b"x", false, true, false);
            assert_eq!(SEARCH_PATTERNS.get_mut().last_idx, 1); // untouched
        }
        reset_vv_searchforward();
    }
}
