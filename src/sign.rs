//! Translated from `src/nvim/sign.c` (tractable core only).
//!
//! `sign.c` (~1650 lines) implements the `:sign` command family
//! (`:sign define`/`place`/`unplace`/`list`/`jump`) and its Vimscript
//! `sign_*()` builtin-function counterparts - almost every function
//! needs the real sign-registry/placed-sign-list machinery (`first_sign`,
//! `buf->b_signlist`) and/or the Ex-command execution engine, neither
//! translated.
//!
//! Also translated: [`group_get_ns`]/[`buf_findsign`]. Note that the
//! original's `map_get` yields `0` for a missing key and then maps
//! that same `0` to `-1`, so a group genuinely registered under
//! namespace `0` is indistinguishable from an unknown one - which is
//! why `buf_findsign` rejects a NAMED group resolving to `0` rather
//! than treating it as the global namespace.
//!
//! Translated: `sign_cmd_idx` (find a `":sign"` subcommand's index by
//! name, via the small, fixed `cmds` table). No real caller yet
//! (`ex_sign`, its only reader, isn't translated) - translated ahead
//! of it anyway, matching this crate's established "translate a
//! small, simple, mechanically-correct piece ahead of the surrounding
//! engine" precedent.
//!
//! Also `describe_sign_text` and `init_sign_text`, both unlocked once
//! `grid.rs`'s own `schar_get`/`schar_from_ascii` and `mbyte.rs`'s
//! `utfc_ptr2schar` landed. `init_sign_text` takes an `is_sign: bool`
//! rather than the original's `sign_T *sp`: that pointer is used only
//! as a NULL test (an extmark sign both skips backslash unescaping and
//! suppresses the error message), so passing the flag keeps this free
//! of the sign registry, which is not translated.
//!
//! Cross-checked against a real `nvim` binary via `:sign define
//! ... text=...`, which pinned the cell-width rules: two ASCII cells
//! or ONE double-width character are accepted, three ASCII cells or
//! two double-width characters are E239.
//!
//! Deferred: everything else in the file.

/// `":sign"` subcommand names, in `SIGNCMD_*` order (`cmds`).
const CMDS: [&str; 6] = ["define", "undefine", "list", "place", "unplace", "jump"];

/// `":sign define"` (`SIGNCMD_DEFINE`).
pub const SIGNCMD_DEFINE: i32 = 0;
/// `":sign undefine"` (`SIGNCMD_UNDEFINE`).
pub const SIGNCMD_UNDEFINE: i32 = 1;
/// `":sign list"` (`SIGNCMD_LIST`).
pub const SIGNCMD_LIST: i32 = 2;
/// `":sign place"` (`SIGNCMD_PLACE`).
pub const SIGNCMD_PLACE: i32 = 3;
/// `":sign unplace"` (`SIGNCMD_UNPLACE`).
pub const SIGNCMD_UNPLACE: i32 = 4;
/// `":sign jump"` (`SIGNCMD_JUMP`).
pub const SIGNCMD_JUMP: i32 = 5;
/// One past the last real `SIGNCMD_*` value - returned by
/// [`sign_cmd_idx`] when no subcommand name matches (`SIGNCMD_LAST`).
pub const SIGNCMD_LAST: i32 = 6;

/// Find the index of a `":sign"` subcommand from its name
/// (`sign_cmd_idx`). Returns [`SIGNCMD_LAST`] if `cmd` doesn't match
/// any known subcommand name.
///
/// The original takes `begin_cmd`/`end_cmd` pointers into a shared
/// buffer, temporarily NUL-terminating at `end_cmd` (restoring the
/// original character afterward) to compare just the subcommand
/// portion - this crate's own byte slice already carries its own
/// bound, so `cmd` is simply the already-isolated subcommand text
/// directly, with no NUL-poking/restoring needed.
/// Convert a sign `group` to a namespace filter (`group_get_ns`).
///
/// `None` is the global namespace (`0`), `"*"` means every namespace
/// (`u32::MAX`), and any other name is looked up. An unknown name
/// yields `-1`, which callers must treat as "no such group" rather
/// than as a namespace id.
///
/// # Safety
/// Must not run concurrently with any write to
/// `crate::api::extmark::NAMESPACE_IDS`.
#[must_use]
pub unsafe fn group_get_ns(group: Option<&[u8]>) -> i64 {
    let Some(group) = group else {
        return 0; // Global namespace
    };
    if group == b"*" {
        return i64::from(u32::MAX); // All namespaces
    }
    // SAFETY: forwarded from this function's own safety doc.
    let ids = unsafe { crate::api::extmark::NAMESPACE_IDS.get_mut() };
    // The original's `map_get` yields 0 for a missing key, and it
    // maps that same 0 to -1 - so a namespace id of 0 is
    // indistinguishable from "not found" here too, by construction.
    match ids.get(&group.to_vec()).copied().unwrap_or(0) {
        0 => -1,
        ns => i64::from(ns),
    }
}

/// Find the line a sign is on (`buf_findsign`), or `0` when there is
/// no such sign.
///
/// Returns a 1-based line number, since `0` is the "not found"
/// answer.
///
/// # Safety
/// Forwarded from [`group_get_ns`].
#[must_use]
pub unsafe fn buf_findsign(
    buf: &crate::buffer_defs::BufT,
    id: u32,
    group: Option<&[u8]>,
) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let ns = unsafe { group_get_ns(group) };
    // A named group that resolved to the global namespace is not a
    // real match, so it is rejected alongside an unknown group.
    if ns < 0 || (group.is_some() && ns == 0) {
        return 0;
    }
    let key = crate::marktree::marktree_lookup_ns(
        &buf.b_marktree,
        u32::try_from(ns).unwrap_or(0),
        id,
        false,
        None,
    );
    key.pos.row + 1
}

#[must_use]
pub fn sign_cmd_idx(cmd: &[u8]) -> i32 {
    for (idx, name) in CMDS.iter().enumerate() {
        if cmd == name.as_bytes() {
            return idx as i32;
        }
    }
    SIGNCMD_LAST
}

/// Render a sign's own `schar_T` cells back into their UTF-8 bytes
/// (`describe_sign_text`).
///
/// The original fills a caller-provided `char *buf` that must be
/// `SIGN_WIDTH * MAX_SCHAR_SIZE` bytes and returns the byte count;
/// returning an owned `Vec<u8>` says the same thing and removes the
/// buffer-sizing obligation entirely.
///
/// Stops at the first empty cell, matching the original's own
/// `if (len == 0) break;`.
///
/// # Safety
/// Forwarded from `crate::grid::schar_get`'s own safety doc.
#[must_use]
pub unsafe fn describe_sign_text(sign_text: &[crate::types_defs::ScharT]) -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..usize::try_from(crate::types_defs::SIGN_WIDTH).unwrap_or(0) {
        let Some(&sc) = sign_text.get(i) else {
            break;
        };
        let bytes = crate::grid::schar_get(sc);
        if bytes.is_empty() {
            break;
        }
        out.extend_from_slice(&bytes);
    }
    out
}

/// Initialize the text for a new sign and store it in `sign_text`
/// (`init_sign_text`).
///
/// `is_sign` is the original's own `sp != NULL` test: `false` means
/// the sign came from `nvim_buf_set_extmark()` rather than
/// `:sign define`, which both skips backslash unescaping and
/// suppresses the error message. Passing the flag rather than a
/// `sign_T` pointer keeps this free of the sign registry, which is
/// not translated - `sp` is used for nothing else here.
///
/// Returns `OK`/`FAIL`. The original's own `semsg` on failure is
/// omitted, matching this crate's established policy of keeping the
/// exact return value while skipping deferred message display.
///
/// # Safety
/// Forwarded from `crate::mbyte::utfc_ptr2schar`'s own safety doc.
pub unsafe fn init_sign_text(
    is_sign: bool,
    sign_text: &mut [crate::types_defs::ScharT],
    text: &[u8],
) -> i32 {
    // Remove backslashes, so that it is possible to use a space.
    // The original edits `text` in place with STRMOVE and shrinks
    // `endp`; building the unescaped copy is the same transformation
    // without mutating the caller's buffer. When `sp` is NULL the
    // original starts its loop AT `endp`, so it never runs - hence
    // the `is_sign` guard here.
    let unescaped: Vec<u8> = if is_sign {
        let mut v = Vec::with_capacity(text.len());
        let mut i = 0;
        while i < text.len() {
            // The original's `s + 1 < endp` bound means a trailing
            // backslash (the last byte) is NOT removed.
            if text[i] == b'\\' && i + 2 < text.len() {
                i += 1;
            }
            v.push(text[i]);
            i += 1;
        }
        v
    } else {
        text.to_vec()
    };
    let text = unescaped.as_slice();

    // Count cells and check for non-printable chars
    let mut cells = 0usize;
    let mut s = 0usize;
    while s < text.len() {
        // SAFETY: forwarded from this function's own safety doc.
        let (sc, c) = unsafe { crate::mbyte::utfc_ptr2schar(&text[s..]) };
        if let Some(slot) = sign_text.get_mut(cells) {
            *slot = sc;
        }
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { crate::charset::vim_isprintc(c) } {
            break;
        }
        // SAFETY: a plain width lookup; the slice is non-empty.
        let width = usize::try_from(unsafe { crate::mbyte::utf_ptr2cells(&text[s..]) }).unwrap_or(0);
        if width == 2
            && let Some(slot) = sign_text.get_mut(cells + 1)
        {
            *slot = 0;
        }
        cells += width;
        // SAFETY: forwarded from this function's own safety doc.
        s += usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(&text[s..]) }).unwrap_or(1);
    }

    // Currently must be empty, one or two display cells
    if s != text.len() || cells > usize::try_from(crate::types_defs::SIGN_WIDTH).unwrap_or(0) {
        return crate::vim_defs::FAIL;
    }

    if cells < 1 {
        if let Some(slot) = sign_text.first_mut() {
            *slot = 0;
        }
    } else if cells == 1
        && let Some(slot) = sign_text.get_mut(1)
    {
        *slot = crate::grid::schar_from_ascii(b' ');
    }

    crate::vim_defs::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Removes the namespace names a test inserted, so the shared
    /// table is left as it was found. Cloning the whole map is not an
    /// option: `Map` is not `Clone`.
    struct NamespaceIdsGuard {
        inserted: Vec<Vec<u8>>,
    }

    impl NamespaceIdsGuard {
        fn new() -> Self {
            Self { inserted: Vec::new() }
        }

        fn insert(&mut self, name: &[u8], id: i32) {
            unsafe { crate::api::extmark::NAMESPACE_IDS.get_mut() }.insert(name.to_vec(), id);
            self.inserted.push(name.to_vec());
        }
    }

    impl Drop for NamespaceIdsGuard {
        fn drop(&mut self) {
            let ids = unsafe { crate::api::extmark::NAMESPACE_IDS.get_mut() };
            for name in &self.inserted {
                ids.remove(name);
            }
        }
    }

    #[test]
    fn group_get_ns_maps_none_to_the_global_namespace() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { group_get_ns(None) }, 0);
    }

    #[test]
    fn group_get_ns_maps_star_to_all_namespaces() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { group_get_ns(Some(b"*")) }, i64::from(u32::MAX));
    }

    #[test]
    fn group_get_ns_looks_up_a_named_group() {
        let _lock = crate::globals::global_state_test_lock();
        let mut guard = NamespaceIdsGuard::new();
        guard.insert(b"mygroup", 7);

        assert_eq!(unsafe { group_get_ns(Some(b"mygroup")) }, 7);
    }

    #[test]
    fn group_get_ns_reports_minus_one_for_an_unknown_group() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = NamespaceIdsGuard::new();
        // -1 is "no such group", which is NOT a namespace id - callers
        // have to reject it rather than pass it through.
        assert_eq!(unsafe { group_get_ns(Some(b"nope")) }, -1);
    }

    #[test]
    fn buf_findsign_returns_zero_for_an_unknown_group() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = NamespaceIdsGuard::new();
        let buf = crate::buffer_defs::BufT::default();

        assert_eq!(unsafe { buf_findsign(&buf, 1, Some(b"nope")) }, 0);
    }

    #[test]
    fn buf_findsign_rejects_a_named_group_resolving_to_global() {
        let _lock = crate::globals::global_state_test_lock();
        let mut guard = NamespaceIdsGuard::new();
        // A group explicitly mapped to namespace 0 is indistinguishable
        // from "not found" in the original's map_get, so it is
        // rejected rather than treated as the global namespace.
        guard.insert(b"zero", 0);
        let buf = crate::buffer_defs::BufT::default();

        assert_eq!(unsafe { buf_findsign(&buf, 1, Some(b"zero")) }, 0);
    }

    #[test]
    fn buf_findsign_finds_nothing_in_an_empty_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = NamespaceIdsGuard::new();
        let buf = crate::buffer_defs::BufT::default();

        // The global namespace is allowed through, but there is no
        // such mark, so the lookup reports row -1 -> line 0.
        assert_eq!(unsafe { buf_findsign(&buf, 1, None) }, 0);
    }

    // ---- describe_sign_text / init_sign_text ----

    /// The glyph cache is process-wide, so any test touching it must
    /// hold the same lock every other global-state test holds.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        crate::globals::global_state_test_lock()
    }

    /// Run `init_sign_text` over a fresh two-cell buffer, returning
    /// `(result, cells)`.
    fn init(is_sign: bool, text: &[u8]) -> (i32, [crate::types_defs::ScharT; 2]) {
        let mut cells = [0u32; 2];
        let rv = unsafe { init_sign_text(is_sign, &mut cells, text) };
        (rv, cells)
    }

    #[test]
    fn init_sign_text_accepts_the_forms_real_nvim_accepts() {
        let _l = lock();
        // Every case cross-checked against a real nvim binary with
        // `:sign define ... text=...`.
        for t in [&b"ab"[..], b"a", b">>", "一".as_bytes()] {
            assert_eq!(
                init(true, t).0,
                crate::vim_defs::OK,
                "{:?} should be accepted",
                std::string::String::from_utf8_lossy(t)
            );
        }
    }

    #[test]
    fn init_sign_text_rejects_text_wider_than_two_cells() {
        let _l = lock();
        // Three ASCII cells, and two double-width chars (four cells);
        // real nvim reports E239 for both.
        for t in [&b"abc"[..], "一一".as_bytes()] {
            assert_eq!(
                init(true, t).0,
                crate::vim_defs::FAIL,
                "{:?} should be rejected",
                std::string::String::from_utf8_lossy(t)
            );
        }
    }

    #[test]
    fn init_sign_text_pads_a_single_cell_with_a_space() {
        let _l = lock();
        let (rv, cells) = init(true, b"a");
        assert_eq!(rv, crate::vim_defs::OK);
        assert_eq!(cells[0], crate::grid::schar_from_ascii(b'a'));
        assert_eq!(cells[1], crate::grid::schar_from_ascii(b' '));
    }

    #[test]
    fn init_sign_text_marks_the_second_cell_of_a_double_width_char() {
        let _l = lock();
        let (rv, cells) = init(true, "一".as_bytes());
        assert_eq!(rv, crate::vim_defs::OK);
        assert_eq!(cells[0], crate::grid::schar_from_char(0x4e00));
        // A double-width glyph owns both cells, so the second is
        // zeroed rather than space-padded.
        assert_eq!(cells[1], 0);
    }

    #[test]
    fn init_sign_text_unescapes_backslashes_only_for_a_real_sign() {
        let _l = lock();
        // `\ a` is accepted by real nvim: the backslash is removed so
        // a literal space can be used, leaving the two cells " a".
        let (rv, cells) = init(true, b"\\ a");
        assert_eq!(rv, crate::vim_defs::OK);
        assert_eq!(cells[0], crate::grid::schar_from_ascii(b' '));
        assert_eq!(cells[1], crate::grid::schar_from_ascii(b'a'));

        // For an extmark sign the original starts its unescaping loop
        // AT the end of the string, so it never runs - the backslash
        // survives and the text is three cells wide, hence rejected.
        assert_eq!(init(false, b"\\ a").0, crate::vim_defs::FAIL);
    }

    #[test]
    fn init_sign_text_rejects_a_non_printable_character() {
        let _l = lock();
        // The scan breaks early on a non-printable char, so `s` never
        // reaches the end and the value is rejected.
        assert_eq!(init(true, b"\x01b").0, crate::vim_defs::FAIL);
    }

    #[test]
    fn init_sign_text_accepts_empty_text_and_zeroes_the_first_cell() {
        let _l = lock();
        let mut cells = [crate::grid::schar_from_ascii(b'x'); 2];
        assert_eq!(
            unsafe { init_sign_text(true, &mut cells, b"") },
            crate::vim_defs::OK
        );
        assert_eq!(cells[0], 0);
    }

    #[test]
    fn describe_sign_text_round_trips_what_init_sign_text_stored() {
        let _l = lock();
        let (rv, cells) = init(true, b"ab");
        assert_eq!(rv, crate::vim_defs::OK);
        assert_eq!(unsafe { describe_sign_text(&cells) }, b"ab");

        // A double-width glyph leaves cell 1 empty, which stops the
        // walk - so only the one glyph comes back.
        let (_, cells) = init(true, "一".as_bytes());
        assert_eq!(unsafe { describe_sign_text(&cells) }, "一".as_bytes());

        // An all-empty buffer yields nothing at all.
        assert_eq!(unsafe { describe_sign_text(&[0, 0]) }, b"");
    }

    #[test]
    fn matches_every_real_subcommand_name() {
        assert_eq!(sign_cmd_idx(b"define"), SIGNCMD_DEFINE);
        assert_eq!(sign_cmd_idx(b"undefine"), SIGNCMD_UNDEFINE);
        assert_eq!(sign_cmd_idx(b"list"), SIGNCMD_LIST);
        assert_eq!(sign_cmd_idx(b"place"), SIGNCMD_PLACE);
        assert_eq!(sign_cmd_idx(b"unplace"), SIGNCMD_UNPLACE);
        assert_eq!(sign_cmd_idx(b"jump"), SIGNCMD_JUMP);
    }

    #[test]
    fn unrecognized_name_returns_signcmd_last() {
        assert_eq!(sign_cmd_idx(b"bogus"), SIGNCMD_LAST);
        assert_eq!(sign_cmd_idx(b""), SIGNCMD_LAST);
    }

    #[test]
    fn is_case_sensitive_and_requires_an_exact_match() {
        // "Define" (wrong case) and "def" (a mere prefix) both fail -
        // the original's own strcmp is a full, case-sensitive match.
        assert_eq!(sign_cmd_idx(b"Define"), SIGNCMD_LAST);
        assert_eq!(sign_cmd_idx(b"def"), SIGNCMD_LAST);
        assert_eq!(sign_cmd_idx(b"defined"), SIGNCMD_LAST);
    }
}
