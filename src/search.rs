//! Translated from `src/nvim/search.c` (tractable core only).
//!
//! `search.c` is neovim's search-command file (string search for `/`,
//! `?`, `n`, `N`; character search within a line for `f`/`F`/`t`/`T`;
//! `%`/word searches - thousands of lines) - almost entirely dependent
//! on the real regex engine (`regexp.c`), the search-pattern-history
//! subsystem, and message display, not attempted here.
//!
//! Translated: the small "last character search" (`f`/`F`/`t`/`T`)
//! file-static state and its five simple accessors -
//! `last_csearch_forward`/`last_csearch_until`/`set_last_csearch`/
//! `set_csearch_direction`/`set_csearch_until`. The original's
//! `lastc`/`lastcdir`/`last_t_cmd`/`lastc_bytes`/`lastc_bytelen`
//! file-statics are bundled into one `LastCsearch` struct behind one
//! `GlobalCell`, matching `indent.rs`'s `BreakindentCache` precedent
//! for a group of related file-statics. `lastc` (a 2-element
//! `uint8_t[2]` in the original) is mirrored here as a single `u8`:
//! `lastc[1]` is always left at its initial `NUL` and never read or
//! written anywhere in the real source - only `lastc[0]` (via
//! `*lastc`) is ever touched.
//!
//! Also translated: `linewhite` (needed only `charset.c`'s
//! `skipwhite` and `memline.c`'s `ml_get`, both already real).
//!
//! Deferred: everything else in the file - `SearchPattern`/
//! `SearchOffset`/`spats[]` (the `/`/`?`/`:s` search-pattern-history
//! state) and the functions built on them (`last_search_pat`,
//! `reset_search_dir`, `set_last_search_pat`, etc.) need the real
//! regex engine (to actually COMPILE a stored pattern) and
//! `set_vim_var_nr`/`VV_SEARCHFORWARD` (the `v:` special-variable
//! subsystem, not yet translated) - a separate, substantial
//! undertaking from this file's own small "last character search"
//! corner, left for a dedicated future pass.

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
    state.bytes = [0; MAX_SCHAR_SIZE + 1];
    if len != 0 {
        let len = usize::try_from(len).expect("set_last_csearch: len must not be negative");
        state.bytes[..len].copy_from_slice(&s[..len]);
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
}
