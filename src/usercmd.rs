//! Translated from `src/nvim/usercmd.c` (tractable core only).
//!
//! `usercmd.c` (~1700 lines) manages user-defined commands (`:command`,
//! `:delcommand`, `:comclear`) - almost entirely dependent on the
//! not-yet-translated user-command registry (`ucmd_T`, per-buffer/
//! global `garray_T` command tables) and Ex-command execution.
//!
//! Translated: [`uc_split_args_iter`] and [`uc_nargs_upper_bound`] - pure
//! byte-string parsing helpers used for Lua `<f-args>` callback argument
//! splitting, with no dependency on the command registry itself - plus
//! [`parse_addr_type_arg`], a small lookup table (`ADDR_TYPE_COMPLETE`,
//! 8 entries) needing only the already-translated
//! [`crate::ex_cmds_defs::CmdAddrT`] enum.
//!
//! Deferred: everything else - `uc_add_command`/`ex_command`/
//! `ex_comclear`/`ex_delcommand`/`do_ucmd`/`uc_list`, and
//! `cmdcomplete_str_to_type`/`parse_compl_arg` (both need
//! `command_complete[]`'s much larger ~70-entry `EXPAND_*`-indexed
//! table - a separate, dedicated undertaking).

use crate::ascii_defs::ascii_iswhite;

/// Splits a byte string by unescaped whitespace (space & tab), used for
/// `<f-args>` on Lua command callbacks. Similar to the original's
/// `uc_split_args` (not translated), but does not allocate, add quotes,
/// or add commas - an iterator instead (`uc_split_args_iter`).
///
/// `end` is the byte offset to resume from (`0` on the first call for a
/// given `arg`); updated in place to the offset to resume from on the
/// NEXT call. Writes the next unescaped-whitespace-delimited token into
/// `buf` (which must be at least `arg.len()` bytes long), storing its
/// length in `len`.
///
/// Returns `true` once iteration is complete (no more tokens remain).
///
/// The original reads one byte past `pos` (`arg[pos + 1]`) at two
/// points in its loop - always in-bounds relative to its own `arglen`
/// EXCEPT when an escape sequence (`\\` or `\` followed by whitespace)
/// is the very last thing before `arglen`, where it can read exactly
/// `arg[arglen]`. That is well-defined in the original because callers
/// pass a real NUL-terminated C string (so `arg[arglen]` is always the
/// NUL terminator, and `ascii_iswhite(NUL)` is `false`); reproduced here
/// via `slice::get`, treating "no byte there" the same as "not
/// whitespace" - the same observable outcome without assuming a
/// terminator byte exists past the slice.
pub fn uc_split_args_iter(arg: &[u8], end: &mut usize, buf: &mut [u8], len: &mut usize) -> bool {
    if arg.is_empty() {
        return true;
    }

    let arglen = arg.len();
    let mut pos = *end;
    while pos < arglen && ascii_iswhite(i32::from(arg[pos])) {
        pos += 1;
    }

    let mut l = 0usize;
    while pos < arglen - 1 {
        let next_is_backslash_or_white = arg
            .get(pos + 1)
            .is_some_and(|&b| b == b'\\' || ascii_iswhite(i32::from(b)));
        if arg[pos] == b'\\' && next_is_backslash_or_white {
            pos += 1;
            buf[l] = arg[pos];
            l += 1;
        } else {
            buf[l] = arg[pos];
            l += 1;
        }
        if arg.get(pos + 1).is_some_and(|&b| ascii_iswhite(i32::from(b))) {
            *end = pos + 1;
            *len = l;
            return false;
        }
        pos += 1;
    }

    if pos < arglen && !ascii_iswhite(i32::from(arg[pos])) {
        buf[l] = arg[pos];
        l += 1;
    }

    *len = l;
    true
}

/// An upper bound on the number of whitespace-separated arguments in
/// `arg` (the exact count when arguments are separated by exactly one
/// whitespace character each; escaped whitespace within an argument
/// still counts as a separator here, matching the original exactly -
/// hence "upper bound", not an exact count) (`uc_nargs_upper_bound`).
#[must_use]
pub fn uc_nargs_upper_bound(arg: &[u8]) -> usize {
    let mut was_white = true;
    let mut nargs = 0usize;
    for &byte in arg {
        let is_white = ascii_iswhite(i32::from(byte));
        if was_white && !is_white {
            nargs += 1;
        }
        was_white = is_white;
    }
    nargs
}

/// List of names of address types (`addr_type_complete`). Must be
/// alphabetical by long name for completion (matching the original's
/// own comment) - kept in the exact original order here too.
///
/// `shortname` isn't read by [`parse_addr_type_arg`] itself (only
/// `name`, the long form, is checked there), but is kept for fidelity
/// with the original's full struct shape, ready for a future
/// completion-listing function that needs it too.
const ADDR_TYPE_COMPLETE: &[(crate::ex_cmds_defs::CmdAddrT, &str, &str)] = &[
    (crate::ex_cmds_defs::CmdAddrT::Arguments, "arguments", "arg"),
    (crate::ex_cmds_defs::CmdAddrT::Lines, "lines", "line"),
    (crate::ex_cmds_defs::CmdAddrT::LoadedBuffers, "loaded_buffers", "load"),
    (crate::ex_cmds_defs::CmdAddrT::Tabs, "tabs", "tab"),
    (crate::ex_cmds_defs::CmdAddrT::Buffers, "buffers", "buf"),
    (crate::ex_cmds_defs::CmdAddrT::Windows, "windows", "win"),
    (crate::ex_cmds_defs::CmdAddrT::Quickfix, "quickfix", "qf"),
    (crate::ex_cmds_defs::CmdAddrT::Other, "other", "?"),
];

/// Looks up an address-type name (e.g. `b"lines"`, `b"windows"`)
/// against `ADDR_TYPE_COMPLETE`'s own long names, returning the
/// matching [`crate::ex_cmds_defs::CmdAddrT`], or `None` if `value`
/// isn't a recognized address type (`parse_addr_type_arg`).
///
/// Returns `Option` in place of the original's `OK`/`FAIL` int plus
/// `cmd_addr_T *addr_type_arg` out-parameter, matching this crate's
/// established C-out-parameter-to-owned-return convention. Omits the
/// original's own `semsg` error-message display - which also
/// truncates its own `value` argument in place purely to shape that
/// message's text, not replicated here since nothing needs it without
/// displaying the message - matching this crate's established "skip
/// the deferred message-display side effect" policy (e.g.
/// `window::check_split_disallowed`).
#[must_use]
pub fn parse_addr_type_arg(value: &[u8]) -> Option<crate::ex_cmds_defs::CmdAddrT> {
    ADDR_TYPE_COMPLETE
        .iter()
        .find(|&&(_, name, _)| name.as_bytes() == value)
        .map(|&(addr, _, _)| addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `uc_split_args_iter` to completion, collecting every token
    /// as an owned `Vec<u8>` for easy assertion. Mirrors the real
    /// calling convention used by `lua/executor.c`'s own
    /// `nlua_set_sctx`-adjacent fargs-splitting loop: call repeatedly
    /// while not done, and after EVERY call (whether done or not),
    /// push a token if `len > 0`.
    fn split_all(arg: &[u8]) -> Vec<Vec<u8>> {
        let mut tokens = Vec::new();
        let mut end = 0usize;
        let mut buf = vec![0u8; arg.len().max(1)];
        let mut done = false;
        while !done {
            let mut len = 0usize;
            done = uc_split_args_iter(arg, &mut end, &mut buf, &mut len);
            if len > 0 {
                tokens.push(buf[..len].to_vec());
            }
        }
        tokens
    }

    #[test]
    fn uc_split_args_iter_empty_is_immediately_done() {
        let mut end = 0;
        let mut buf = [0u8; 1];
        let mut len = 0;
        assert!(uc_split_args_iter(b"", &mut end, &mut buf, &mut len));
    }

    #[test]
    fn uc_split_args_iter_single_word() {
        assert_eq!(split_all(b"hello"), vec![b"hello".to_vec()]);
    }

    #[test]
    fn uc_split_args_iter_multiple_words() {
        assert_eq!(
            split_all(b"foo bar baz"),
            vec![b"foo".to_vec(), b"bar".to_vec(), b"baz".to_vec()]
        );
    }

    #[test]
    fn uc_split_args_iter_collapses_repeated_whitespace() {
        assert_eq!(
            split_all(b"foo   bar\tbaz"),
            vec![b"foo".to_vec(), b"bar".to_vec(), b"baz".to_vec()]
        );
    }

    #[test]
    fn uc_split_args_iter_unescapes_backslash_whitespace() {
        // "foo\ bar" (an escaped space) is one token "foo bar".
        assert_eq!(split_all(b"foo\\ bar"), vec![b"foo bar".to_vec()]);
    }

    #[test]
    fn uc_split_args_iter_unescapes_double_backslash() {
        // "a\\b" ('\' escaping '\') collapses to a single backslash.
        assert_eq!(split_all(b"a\\\\b"), vec![b"a\\b".to_vec()]);
    }

    #[test]
    fn uc_split_args_iter_trailing_escape_at_end_of_string_is_safe() {
        // A lone trailing backslash with nothing after it: must not
        // panic (the one-byte-past-the-end read this function's own
        // doc comment discusses).
        assert_eq!(split_all(b"foo\\"), vec![b"foo\\".to_vec()]);
    }

    #[test]
    fn uc_nargs_upper_bound_counts_words() {
        assert_eq!(uc_nargs_upper_bound(b"foo bar baz"), 3);
        assert_eq!(uc_nargs_upper_bound(b"  foo   bar  "), 2);
        assert_eq!(uc_nargs_upper_bound(b""), 0);
        assert_eq!(uc_nargs_upper_bound(b"   "), 0);
        assert_eq!(uc_nargs_upper_bound(b"oneword"), 1);
    }

    #[test]
    fn parse_addr_type_arg_recognizes_every_long_name() {
        use crate::ex_cmds_defs::CmdAddrT;
        assert_eq!(parse_addr_type_arg(b"arguments"), Some(CmdAddrT::Arguments));
        assert_eq!(parse_addr_type_arg(b"lines"), Some(CmdAddrT::Lines));
        assert_eq!(parse_addr_type_arg(b"loaded_buffers"), Some(CmdAddrT::LoadedBuffers));
        assert_eq!(parse_addr_type_arg(b"tabs"), Some(CmdAddrT::Tabs));
        assert_eq!(parse_addr_type_arg(b"buffers"), Some(CmdAddrT::Buffers));
        assert_eq!(parse_addr_type_arg(b"windows"), Some(CmdAddrT::Windows));
        assert_eq!(parse_addr_type_arg(b"quickfix"), Some(CmdAddrT::Quickfix));
        assert_eq!(parse_addr_type_arg(b"other"), Some(CmdAddrT::Other));
    }

    #[test]
    fn parse_addr_type_arg_does_not_match_short_names() {
        // Only the long name is checked, matching the original exactly.
        assert_eq!(parse_addr_type_arg(b"arg"), None);
        assert_eq!(parse_addr_type_arg(b"win"), None);
    }

    #[test]
    fn parse_addr_type_arg_unknown_name_is_none() {
        assert_eq!(parse_addr_type_arg(b"nonexistent"), None);
        assert_eq!(parse_addr_type_arg(b""), None);
    }
}
