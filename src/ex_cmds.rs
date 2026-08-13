//! Translated from `src/nvim/ex_cmds.c` (partial - a tiny, deliberate
//! harvest of a couple of small, self-contained functions).
//!
//! `ex_cmds.c` (~7000 lines) implements most `:`-command handlers
//! (`:copy`, `:move`, `:sort`, `:write`, etc.) - a whole separate,
//! substantial phase-6 undertaking, not attempted here.
//!
//! Translated: `check_secure` - needed as a dependency by
//! `eval/fs.c`'s `f_delete`/`f_rename`/`f_filecopy` (none of the
//! latter two translated yet, but `f_delete` needs this one directly).
//! Harvested here on its own rather than waiting for the rest of this
//! file, matching the same "one tractable function ahead of a huge
//! file" precedent already used for `ex_docmd.rs`. Also
//! [`check_regexp_delim`] - whether `c` is NOT a letter (letters can't
//! delimit a `:s/pat/sub/`-style regexp), needed by `do_sub` and a
//! sibling substitution-command handler (neither translated yet) - the
//! original's own real `emsg("E146: ...")` display is omitted
//! (message display, not tractable), matching `check_secure`'s own
//! established policy just above, while the exact same `FAIL`/`OK`
//! return value is kept.

/// Return `true` (and disallow the caller's own operation) when
/// running with `'secure'`/`'-Z'`-style restrictions or inside the
/// sandbox (`check_secure`). The original's own real `emsg()` display
/// is omitted (message display, not tractable), matching this crate's
/// established "skip the message, keep the state" policy - but the
/// real, observable side effect (bumping `secure` from `1` to `2`,
/// so a SECOND, later call site can tell the restriction was actually
/// triggered at least once) is kept exactly.
///
/// # Safety
/// Touches `crate::globals::GLOBALS`.
#[must_use]
pub unsafe fn check_secure() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if globals.secure != 0 {
        globals.secure = 2;
        return true;
    }

    if globals.sandbox != 0 {
        return true;
    }

    false
}

/// Whether `c` is a valid regexp delimiter for `:s/pat/sub/`-style
/// commands: `false` for a letter (letters can't delimit a regexp),
/// `true` otherwise (`check_regexp_delim`).
///
/// The original's own real `emsg("E146: ...")` display (for the
/// letter case) is omitted, matching [`check_secure`]'s own
/// established policy - the exact same `FAIL`/`OK`-equivalent return
/// value is kept.
#[must_use]
pub fn check_regexp_delim(c: i32) -> bool {
    !crate::macros_defs::ascii_isalpha(c)
}

/// Skip the replacement part of a substitute command
/// (`skip_substitute`).
///
/// Returns the offset after the closing delimiter and replaces that
/// delimiter with NUL.
#[allow(dead_code)]
fn skip_substitute(replacement: &mut [u8], delimiter: u8) -> usize {
    let mut offset = 0;
    while replacement.get(offset).is_some_and(|&byte| byte != 0) {
        if replacement[offset] == delimiter {
            replacement[offset] = crate::ascii_defs::NUL;
            return offset + 1;
        }
        if replacement[offset] == b'\\'
            && replacement.get(offset + 1).is_some_and(|&byte| byte != 0)
        {
            offset += 1;
        }
        offset += crate::mbyte::utf_ptr2len(&replacement[offset..]).max(1) as usize;
    }
    offset
}

/// Find the first unquoted `|` in `cmd` (`find_pipe`).
///
/// Quoted sections (inside `"`) and backslash-escaped bytes do not
/// count, so a `|` inside a quoted shell argument is not mistaken for
/// a command separator.
///
/// @return the byte offset of the separator, or `None` when there is
///         none. The original returns a pointer into `cmd`.
///
/// Only non-Unix builds use this - Unix passes the whole command to
/// the shell, which does its own splitting - matching the original's
/// own `#ifndef UNIX` guard.
#[cfg(not(unix))]
#[must_use]
pub fn find_pipe(cmd: &[u8]) -> Option<usize> {
    let mut inquote = false;
    let mut p = 0;
    while p < cmd.len() && cmd[p] != 0 {
        if !inquote && cmd[p] == b'|' {
            return Some(p);
        }
        if cmd[p] == b'"' {
            inquote = !inquote;
        } else if crate::charset::rem_backslash(&cmd[p..]) {
            p += 1;
        }
        p += 1;
    }
    None
}

/// Honour an `:write ++p` argument by creating any missing parent
/// directories (`handle_mkdir_p_arg`).
///
/// @return [`crate::vim_defs::FAIL`] when the directories could not be
///         created, [`crate::vim_defs::OK`] otherwise - including when
///         `++p` was not given at all.
#[must_use]
pub fn handle_mkdir_p_arg(eap: &crate::ex_cmds_defs::ExargT, fname: &[u8]) -> i32 {
    if eap.mkdir_p && crate::os::fs::os_file_mkdir(fname, 0o755) < 0 {
        return crate::vim_defs::FAIL;
    }
    crate::vim_defs::OK
}

/// `sort_lc` - `:sort` should use locale collation.
///
/// Only ever set by `ex_sort` (not yet translated), so this stays
/// `false` in this crate today.
static SORT_LC: crate::globals::GlobalCell<bool> = crate::globals::GlobalCell::new(false);

/// `sort_ic` - `:sort` should ignore case.
///
/// Only ever set by `ex_sort` (not yet translated), so this stays
/// `false` in this crate today.
static SORT_IC: crate::globals::GlobalCell<bool> = crate::globals::GlobalCell::new(false);

/// Comparator ordering two lines for `:sort` (`string_compare`).
///
/// Three modes, checked in the original's own order: locale collation
/// wins outright, then case-insensitive, then a plain byte
/// comparison. Note `sort_lc` takes priority over `sort_ic`, so
/// `:sort il` collates rather than folding case.
///
/// Returns a negative/zero/positive `i32`, matching `qsort`'s own
/// convention and this crate's established comparator shape.
///
/// # Panics
/// If `SORT_LC` is ever `true`. That branch is `strcoll`, whose
/// locale-aware collation has no counterpart in this crate; it is
/// unreachable today because only `ex_sort`, not translated, can set
/// the flag - the same treatment as `popupmenu::pum_get_height`'s own
/// external-UI branch.
///
/// # Safety
/// Reads the `SORT_LC`/`SORT_IC` file-statics.
#[must_use]
pub unsafe fn string_compare(s1: &[u8], s2: &[u8]) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *SORT_LC.get_mut() } {
        unimplemented!(
            "string_compare: the locale-collation branch needs strcoll, which has no \
             counterpart in this crate - unreachable until ex_sort is translated"
        );
    }
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *SORT_IC.get_mut() } {
        return crate::strings::vim_stricmp(s1, s2);
    }
    match s1.cmp(s2) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// The screen width of the cursor line's own text, ignoring trailing
/// whitespace, and whether that text contains a TAB (`linelen`).
///
/// Returns `(len, has_tab)`. The original writes `has_tab` through an
/// optional out-parameter; returning it is this crate's convention.
///
/// The measured span runs from the START of the line (leading
/// whitespace included, since it still occupies screen cells) up to
/// just past the LAST non-blank. The TAB search, by contrast, starts
/// at the first non-blank - so a line indented with tabs but holding
/// none in its text reports `false`.
///
/// The original brackets this with a temporary NUL poked over the
/// character after the last non-blank; a subslice does the same job
/// here without mutating the line.
///
/// # Safety
/// Forwarded from [`crate::cursor::get_cursor_line_ptr`]/
/// [`crate::plines::linetabsize_str`]'s own safety docs.
#[must_use]
pub unsafe fn linelen() -> (i32, bool) {
    // Get the line. If it's empty bail out early (could be the empty
    // string for an unloaded buffer).
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::cursor::get_cursor_line_ptr() };
    if line.first().copied().unwrap_or(0) == 0 {
        return (0, false);
    }

    // Find the first non-blank character.
    let first = crate::charset::skipwhite(&line);
    // Find the character after the last non-blank character, treating
    // the NUL terminator as the end of the text.
    let text_end = line.iter().position(|&b| b == 0).unwrap_or(line.len());
    let mut last = text_end;
    while last > first && crate::ascii_defs::ascii_iswhite(i32::from(line[last - 1])) {
        last -= 1;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let len = unsafe { crate::plines::linetabsize_str(&line[..last]) };
    let has_tab = line[first..last].contains(&crate::ascii_defs::TAB);
    (len, has_tab)
}

/// The previous `:!` shell command (`prevcmd`).
///
/// Only ever set by `do_bang` (not yet translated), so this stays
/// `None` in this crate today. `None` models the original's own NULL,
/// which is what `prevcmd_is_set` reports on.
static PREVCMD: crate::globals::GlobalCell<Option<Vec<u8>>> =
    crate::globals::GlobalCell::new(None);

/// Release the remembered `:!` shell command
/// (`free_prev_shellcmd`).
///
/// The original's `xfree` becomes a plain `None` assignment: dropping
/// the owned value is what frees it.
///
/// # Safety
/// Mutates the `PREVCMD` file-static.
pub unsafe fn free_prev_shellcmd() {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { PREVCMD.get_mut() } = None;
}

/// Whether a previous `:!` command has been remembered
/// (`prevcmd_is_set`).
///
/// The original also emits `E34: No previous command` when there is
/// none; that message display is omitted here, matching this crate's
/// established "skip the deferred message-display side effect, keep
/// the exact same pass/fail outcome" policy (e.g.
/// `arglist::check_arglist_locked`).
///
/// # Safety
/// Reads the `PREVCMD` file-static.
#[must_use]
pub unsafe fn prevcmd_is_set() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { PREVCMD.get_mut() }.is_some()
}

/// Whether writing files is currently disabled by `'write'`
/// (`not_writing`).
///
/// Note the INVERTED sense: this returns `true` when writing is NOT
/// allowed, so callers can use it as a guard. The original also emits
/// `E142` in that case; that message display is omitted, as above.
///
/// # Safety
/// Reads `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn not_writing() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_write == 0
}

/// The previous `:substitute` replacement string (`old_sub`).
///
/// The original owns raw pointers and frees the outgoing value when a
/// new one is stored; here the old value is simply dropped by
/// replacement, which is the whole of that bookkeeping.
static OLD_SUB: crate::globals::GlobalCell<crate::ex_cmds_defs::SubReplacementString> =
    crate::globals::GlobalCell::new(crate::ex_cmds_defs::SubReplacementString {
        sub: None,
        timestamp: 0,
        additional_data: None,
    });

/// Get the previously used replacement string
/// (`sub_get_replacement`).
///
/// # Safety
/// Reads the `OLD_SUB` file-static.
#[must_use]
pub unsafe fn sub_get_replacement() -> crate::ex_cmds_defs::SubReplacementString {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { OLD_SUB.get_mut() }.clone()
}

/// Set the previously used replacement string
/// (`sub_set_replacement`).
///
/// # Safety
/// Mutates the `OLD_SUB` file-static.
pub unsafe fn sub_set_replacement(sub: crate::ex_cmds_defs::SubReplacementString) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *OLD_SUB.get_mut() = sub };
}

/// Release the previous substitution replacement (`free_old_sub`).
///
/// # Safety
/// Forwarded from [`sub_set_replacement`].
pub unsafe fn free_old_sub() {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        sub_set_replacement(crate::ex_cmds_defs::SubReplacementString::default())
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_substitute_ignores_escaped_delimiters_and_terminates_at_closing_one() {
        let mut replacement = b"one\\/two/rest".to_vec();
        let next = skip_substitute(&mut replacement, b'/');
        assert_eq!(&replacement[..next], b"one\\/two\0");
        assert_eq!(&replacement[next..], b"rest");

        let mut unterminated = b"replacement".to_vec();
        assert_eq!(
            skip_substitute(&mut unterminated, b'/'),
            unterminated.len()
        );
    }

    struct OldSubGuard(Option<crate::ex_cmds_defs::SubReplacementString>);

    impl OldSubGuard {
        fn capture() -> Self {
            Self(Some(unsafe { sub_get_replacement() }))
        }
    }

    impl Drop for OldSubGuard {
        fn drop(&mut self) {
            unsafe {
                sub_set_replacement(self.0.take().expect("saved replacement"))
            };
        }
    }

    fn globals_test_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::globals::global_state_test_lock()
    }

    // ---- prevcmd / free_prev_shellcmd / prevcmd_is_set / not_writing ----

    /// Restores `PREVCMD` and `'write'` on drop, even through a panic.
    struct ExCmdsGuard {
        prevcmd: Option<Vec<u8>>,
        p_write: i32,
    }

    impl ExCmdsGuard {
        fn save() -> Self {
            Self {
                prevcmd: unsafe { PREVCMD.get_mut() }.take(),
                p_write: unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_write,
            }
        }
    }

    impl Drop for ExCmdsGuard {
        fn drop(&mut self) {
            *unsafe { PREVCMD.get_mut() } = self.prevcmd.take();
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_write = self.p_write;
        }
    }

    // ---- string_compare ----

    /// Restores the sort-mode flags on drop, even through a panic.
    struct SortFlagsGuard {
        lc: bool,
        ic: bool,
    }

    impl SortFlagsGuard {
        fn set(lc: bool, ic: bool) -> Self {
            let me = Self {
                lc: unsafe { *SORT_LC.get_mut() },
                ic: unsafe { *SORT_IC.get_mut() },
            };
            unsafe { *SORT_LC.get_mut() = lc };
            unsafe { *SORT_IC.get_mut() = ic };
            me
        }
    }

    impl Drop for SortFlagsGuard {
        fn drop(&mut self) {
            unsafe { *SORT_LC.get_mut() = self.lc };
            unsafe { *SORT_IC.get_mut() = self.ic };
        }
    }

    #[test]
    fn string_compare_orders_bytewise_by_default() {
        let _guard = globals_test_lock();
        let _g = SortFlagsGuard::set(false, false);
        assert!(unsafe { string_compare(b"abc", b"abd") } < 0);
        assert!(unsafe { string_compare(b"abd", b"abc") } > 0);
        assert_eq!(unsafe { string_compare(b"abc", b"abc") }, 0);
    }

    /// Without the ignore-case flag, upper and lower case differ - and
    /// uppercase sorts FIRST, since it has the lower byte value.
    #[test]
    fn string_compare_is_case_sensitive_by_default() {
        let _guard = globals_test_lock();
        let _g = SortFlagsGuard::set(false, false);
        assert!(unsafe { string_compare(b"ABC", b"abc") } < 0);
        assert_ne!(unsafe { string_compare(b"ABC", b"abc") }, 0);
    }

    /// With the flag set, the same pair compares equal.
    #[test]
    fn string_compare_folds_case_when_ignore_case_is_set() {
        let _guard = globals_test_lock();
        let _g = SortFlagsGuard::set(false, true);
        assert_eq!(unsafe { string_compare(b"ABC", b"abc") }, 0);
        assert!(unsafe { string_compare(b"abc", b"ABD") } < 0);
    }

    /// Locale collation is checked FIRST, so it wins even when the
    /// ignore-case flag is also set (`:sort il`).
    #[test]
    #[should_panic(expected = "locale-collation branch")]
    fn string_compare_prefers_locale_collation_over_ignore_case() {
        let _guard = globals_test_lock();
        let _g = SortFlagsGuard::set(true, true);
        let _ = unsafe { string_compare(b"abc", b"abc") };
    }

    // ---- linelen ----

    fn linelen_buf(line: &[u8]) -> crate::buffer_defs::BufT {
        let mut buf = crate::buffer_defs::BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, line) },
            crate::vim_defs::OK
        );
        buf
    }

    fn close_linelen_buf(buf: crate::buffer_defs::BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    /// Runs `linelen` against `line` as the cursor line.
    fn linelen_of(line: &[u8]) -> (i32, bool) {
        let mut buf = linelen_buf(line);
        let mut win = crate::buffer_defs::WinT::default();
        win.w_cursor.lnum = 1;
        win.w_buffer = std::ptr::from_mut(&mut buf);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (pw, pb) = (g.curwin, g.curbuf);
        g.curwin = std::ptr::from_mut(&mut win);
        g.curbuf = std::ptr::from_mut(&mut buf);

        let got = unsafe { linelen() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = pw;
        g.curbuf = pb;
        close_linelen_buf(buf);
        got
    }

    #[test]
    fn linelen_measures_plain_text() {
        let _guard = globals_test_lock();
        assert_eq!(linelen_of(b"abcde"), (5, false));
    }

    #[test]
    fn linelen_reports_zero_for_an_empty_line() {
        let _guard = globals_test_lock();
        assert_eq!(linelen_of(b""), (0, false));
    }

    /// Trailing whitespace is excluded from the width.
    #[test]
    fn linelen_ignores_trailing_whitespace() {
        let _guard = globals_test_lock();
        assert_eq!(linelen_of(b"abc   ").0, 3);
    }

    /// LEADING whitespace still occupies screen cells, so it counts
    /// toward the width even though trailing whitespace does not.
    #[test]
    fn linelen_counts_leading_whitespace_but_not_trailing() {
        let _guard = globals_test_lock();
        // Two leading spaces + 3 characters = 5 cells; the trailing
        // spaces are dropped.
        assert_eq!(linelen_of(b"  abc  ").0, 5);
    }

    /// A TAB inside the text is reported.
    #[test]
    fn linelen_reports_an_embedded_tab() {
        let _guard = globals_test_lock();
        assert!(linelen_of(b"ab\tcd").1);
    }

    /// The TAB search starts at the FIRST NON-BLANK, so a line
    /// indented with tabs but holding none in its text reports false.
    /// Searching the whole line instead would wrongly report true.
    #[test]
    fn linelen_does_not_count_an_indenting_tab_as_embedded() {
        let _guard = globals_test_lock();
        assert!(!linelen_of(b"\tabc").1, "an indenting TAB is not embedded");
    }

    /// ...and a TRAILING tab is past the last non-blank, so it is
    /// excluded too.
    #[test]
    fn linelen_does_not_count_a_trailing_tab_as_embedded() {
        let _guard = globals_test_lock();
        assert!(!linelen_of(b"abc\t").1);
    }

    #[test]
    fn prevcmd_is_unset_until_a_shell_command_is_remembered() {
        let _guard = globals_test_lock();
        let _g = ExCmdsGuard::save();
        *unsafe { PREVCMD.get_mut() } = None;
        assert!(!unsafe { prevcmd_is_set() });

        *unsafe { PREVCMD.get_mut() } = Some(b"ls -l".to_vec());
        assert!(unsafe { prevcmd_is_set() });
    }

    /// An EMPTY previous command still counts as set - the original
    /// tests the pointer for NULL, not the string for emptiness, so
    /// `:!` with an empty command is remembered.
    #[test]
    fn prevcmd_counts_an_empty_command_as_set() {
        let _guard = globals_test_lock();
        let _g = ExCmdsGuard::save();
        *unsafe { PREVCMD.get_mut() } = Some(Vec::new());
        assert!(unsafe { prevcmd_is_set() });
    }

    #[test]
    fn free_prev_shellcmd_releases_the_remembered_command() {
        let _guard = globals_test_lock();
        let _g = ExCmdsGuard::save();
        *unsafe { PREVCMD.get_mut() } = Some(b"ls -l".to_vec());

        unsafe { free_prev_shellcmd() };

        assert!(!unsafe { prevcmd_is_set() });
        assert_eq!(*unsafe { PREVCMD.get_mut() }, None);
    }

    /// Freeing twice must be safe - the original's `xfree(NULL)` is a
    /// no-op too.
    #[test]
    fn free_prev_shellcmd_is_safe_to_repeat() {
        let _guard = globals_test_lock();
        let _g = ExCmdsGuard::save();
        *unsafe { PREVCMD.get_mut() } = Some(b"ls".to_vec());
        unsafe { free_prev_shellcmd() };
        unsafe { free_prev_shellcmd() };
        assert!(!unsafe { prevcmd_is_set() });
    }

    /// The sense is INVERTED: `not_writing()` is true when writing is
    /// DISABLED. Reading it as "writing is allowed" would invert every
    /// caller's guard, so both directions are asserted.
    #[test]
    fn not_writing_is_true_only_when_the_write_option_is_off() {
        let _guard = globals_test_lock();
        let _g = ExCmdsGuard::save();

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_write = 1;
        assert!(!unsafe { not_writing() }, "'write' on means writing IS allowed");

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_write = 0;
        assert!(unsafe { not_writing() }, "'write' off means writing is disabled");
    }

    #[test]
    fn sub_replacement_round_trips_through_its_setter() {
        let _guard = globals_test_lock();
        let previous = unsafe { sub_get_replacement() };

        unsafe {
            sub_set_replacement(crate::ex_cmds_defs::SubReplacementString {
                sub: Some(b"replacement".to_vec()),
                timestamp: 1234,
                additional_data: None,
            });
        }

        let got = unsafe { sub_get_replacement() };
        assert_eq!(got.sub.as_deref(), Some(&b"replacement"[..]));
        assert_eq!(got.timestamp, 1234);

        unsafe { sub_set_replacement(previous) };
    }

    #[test]
    fn sub_set_replacement_discards_the_earlier_value() {
        // The original frees the outgoing string here; replacing the
        // owned value is the whole of that bookkeeping.
        let _guard = globals_test_lock();
        let previous = unsafe { sub_get_replacement() };

        unsafe {
            sub_set_replacement(crate::ex_cmds_defs::SubReplacementString {
                sub: Some(b"first".to_vec()),
                timestamp: 1,
                additional_data: None,
            });
            sub_set_replacement(crate::ex_cmds_defs::SubReplacementString {
                sub: Some(b"second".to_vec()),
                timestamp: 2,
                additional_data: None,
            });
        }

        let got = unsafe { sub_get_replacement() };
        assert_eq!(got.sub.as_deref(), Some(&b"second"[..]));
        assert_eq!(got.timestamp, 2);

        unsafe { sub_set_replacement(previous) };
    }

    #[test]
    fn sub_replacement_starts_out_unset() {
        // A session that has never run :substitute has no previous
        // replacement string.
        let _guard = globals_test_lock();
        let previous = unsafe { sub_get_replacement() };

        unsafe {
            sub_set_replacement(crate::ex_cmds_defs::SubReplacementString {
                sub: None,
                timestamp: 0,
                additional_data: None,
            });
        }
        let got = unsafe { sub_get_replacement() };
        assert_eq!(got.sub, None);
        assert_eq!(got.timestamp, 0);

        unsafe { sub_set_replacement(previous) };
    }

    #[test]
    fn free_old_sub_clears_every_owned_replacement_field() {
        let _lock = globals_test_lock();
        let _guard = OldSubGuard::capture();
        unsafe {
            sub_set_replacement(crate::ex_cmds_defs::SubReplacementString {
                sub: Some(b"replacement".to_vec()),
                timestamp: 42,
                additional_data: Some(Box::new(
                    crate::types_defs::AdditionalData {
                        nitems: 2,
                        nbytes: 8,
                    },
                )),
            });
            free_old_sub();
        }
        let cleared = unsafe { sub_get_replacement() };
        assert!(cleared.sub.is_none());
        assert_eq!(cleared.timestamp, 0);
        assert!(cleared.additional_data.is_none());
    }

    #[cfg(not(unix))]
    #[test]
    fn find_pipe_finds_an_unquoted_bar() {
        assert_eq!(find_pipe(b"sort | uniq"), Some(5));
        assert_eq!(find_pipe(b"|first"), Some(0));
    }

    #[cfg(not(unix))]
    #[test]
    fn find_pipe_is_none_without_a_bar() {
        assert_eq!(find_pipe(b"sort file"), None);
        assert_eq!(find_pipe(b""), None);
    }

    #[cfg(not(unix))]
    #[test]
    fn find_pipe_ignores_a_quoted_bar() {
        // A bar inside a quoted argument is part of that argument, not
        // a command separator.
        assert_eq!(find_pipe(br#"grep "a|b" file"#), None);
        // ...but one after the quotes close still counts.
        assert_eq!(find_pipe(br#"grep "a|b" | wc"#), Some(11));
    }

    #[cfg(not(unix))]
    #[test]
    fn find_pipe_stops_at_an_embedded_nul() {
        // The original scans a NUL-terminated string, so a NUL ends
        // the scan even with more bytes in the slice.
        assert_eq!(find_pipe(b"abc\0|def"), None);
    }

    #[test]
    fn handle_mkdir_p_arg_is_ok_when_the_flag_is_not_set() {
        // Without ++p nothing is created at all, so a path that could
        // never be made is still fine.
        let eap = crate::ex_cmds_defs::ExargT { mkdir_p: false, ..Default::default() };
        assert_eq!(handle_mkdir_p_arg(&eap, b""), crate::vim_defs::OK);
    }

    #[test]
    fn handle_mkdir_p_arg_creates_missing_parents() {
        let dir = std::env::temp_dir().join("nero_mkdir_p_test").join("a").join("b");
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("nero_mkdir_p_test"));
        let target = dir.join("file.txt");

        let eap = crate::ex_cmds_defs::ExargT { mkdir_p: true, ..Default::default() };
        let r = handle_mkdir_p_arg(&eap, target.to_str().unwrap().as_bytes());

        let made = dir.is_dir();
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("nero_mkdir_p_test"));

        assert_eq!(r, crate::vim_defs::OK);
        assert!(made, "the parent directories of the file must exist afterwards");
    }

    #[test]
    fn check_secure_false_when_neither_secure_nor_sandbox() {
        let _guard = globals_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.secure = 0;
        globals.sandbox = 0;
        assert!(!unsafe { check_secure() });
    }

    #[test]
    fn check_secure_true_and_bumps_secure_to_two() {
        let _guard = globals_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.secure = 1;
        globals.sandbox = 0;
        assert!(unsafe { check_secure() });
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.secure, 2);
        // Restore for any other test relying on the default.
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
    }

    #[test]
    fn check_secure_true_when_sandboxed_but_leaves_secure_untouched() {
        let _guard = globals_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.secure = 0;
        globals.sandbox = 1;
        assert!(unsafe { check_secure() });
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.secure, 0);
        unsafe { crate::globals::GLOBALS.get_mut() }.sandbox = 0;
    }

    #[test]
    fn check_regexp_delim_rejects_letters() {
        assert!(!check_regexp_delim(i32::from(b'a')));
        assert!(!check_regexp_delim(i32::from(b'Z')));
    }

    #[test]
    fn check_regexp_delim_accepts_common_delimiters() {
        assert!(check_regexp_delim(i32::from(b'/')));
        assert!(check_regexp_delim(i32::from(b'#')));
        assert!(check_regexp_delim(i32::from(b',')));
    }

    #[test]
    fn check_regexp_delim_accepts_a_digit() {
        assert!(check_regexp_delim(i32::from(b'0')));
    }
}
