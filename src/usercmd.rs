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
//! [`crate::ex_cmds_defs::CmdAddrT`] enum. Also translated:
//! [`cmdcomplete_type_to_str`]/[`cmdcomplete_str_to_type`]/
//! [`parse_compl_arg`], via a new `COMMAND_COMPLETE` lookup table (46
//! entries, mechanically transcribed + individually cross-checked
//! against the original's own `command_complete[]` sparse designated-
//! initializer array) - the original module doc's own claim that this
//! needed "a much larger ~70-entry table - a separate, dedicated
//! undertaking" was stale (that number was
//! [`crate::cmdexpand_defs::ExpandContext`]'s own total variant count,
//! not `command_complete[]`'s real, much smaller populated size) -
//! re-verified directly against the real C source, not assumed.
//!
//! Also translated: [`uc_validate_name`] - a tiny, self-contained
//! "does this look like a valid command name, and where does it end"
//! scan, via already-real [`crate::ex_docmd::ends_excmd`] and
//! [`crate::ascii_defs::ascii_iswhite`]/`crate::macros_defs::
//! ascii_isalpha`/`crate::macros_defs::ascii_isalnum`.
//!
//! Also translated: the four `ExpandGeneric()` callbacks
//! [`get_user_cmd_addr_type`]/[`get_user_cmd_flags`]/
//! [`get_user_cmd_nargs`]/[`get_user_cmd_complete`], which only need
//! the `ADDR_TYPE_COMPLETE`/`COMMAND_COMPLETE` tables above and no
//! part of the command registry. Their expected outputs were read out
//! of a real `nvim` binary via `getcompletion('command -nargs=',
//! 'cmdline')` and friends; notably it reports 45 `-complete=` names
//! against `COMMAND_COMPLETE`'s 46 entries, confirming that
//! `EXPAND_USER_LUA`'s `"<Lua function>"` really is blanked out.
//!
//! Deferred: everything else - `uc_add_command`/`ex_command`/
//! `ex_comclear`/`ex_delcommand`/`do_ucmd`/`uc_list`/`uc_scan_attr`
//! (the real user-command registry and its `:command`-parsing
//! caller).

use crate::ascii_defs::ascii_iswhite;
use crate::ex_docmd::ends_excmd;
use crate::macros_defs::{ascii_isalnum, ascii_isalpha};

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

/// Names of every `-complete=` completion type that has one
/// (`command_complete[]`) - a sparse C designated-initializer array
/// (indexed by [`crate::cmdexpand_defs::ExpandContext`] discriminant,
/// with most of that enum's ~70 variants NOT present at all) kept
/// here as a flat list of (variant, name) pairs instead, avoiding any
/// numeric discriminant round-tripping. Mechanically transcribed from
/// the original in its own exact declaration order and individually
/// cross-checked, entry by entry, against both the original array and
/// `ExpandContext`'s own already-verified discriminant values.
const COMMAND_COMPLETE: &[(crate::cmdexpand_defs::ExpandContext, &str)] = {
    use crate::cmdexpand_defs::ExpandContext as Ec;
    &[
        (Ec::Arglist, "arglist"),
        (Ec::Augroup, "augroup"),
        (Ec::Buffers, "buffer"),
        (Ec::Checkhealth, "checkhealth"),
        (Ec::Colors, "color"),
        (Ec::Commands, "command"),
        (Ec::Compiler, "compiler"),
        (Ec::UserDefined, "custom"),
        (Ec::UserList, "customlist"),
        (Ec::UserLua, "<Lua function>"),
        (Ec::DiffBuffers, "diff_buffer"),
        (Ec::Directories, "dir"),
        (Ec::EnvVars, "environment"),
        (Ec::Events, "event"),
        (Ec::Expression, "expression"),
        (Ec::Files, "file"),
        (Ec::FilesInPath, "file_in_path"),
        (Ec::Filetype, "filetype"),
        (Ec::Filetypecmd, "filetypecmd"),
        (Ec::Functions, "function"),
        (Ec::Help, "help"),
        (Ec::Highlight, "highlight"),
        (Ec::History, "history"),
        (Ec::Keymap, "keymap"),
        (Ec::Locales, "locale"),
        (Ec::Lua, "lua"),
        (Ec::Mapclear, "mapclear"),
        (Ec::Mappings, "mapping"),
        (Ec::Menus, "menu"),
        (Ec::Messages, "messages"),
        (Ec::Ownsyntax, "syntax"),
        (Ec::Syntime, "syntime"),
        (Ec::Settings, "option"),
        (Ec::Packadd, "packadd"),
        (Ec::Retab, "retab"),
        (Ec::Runtime, "runtime"),
        (Ec::Shellcmd, "shellcmd"),
        (Ec::Shellcmdline, "shellcmdline"),
        (Ec::Sign, "sign"),
        (Ec::Tags, "tag"),
        (Ec::TagsListfiles, "tag_listfiles"),
        (Ec::User, "user"),
        (Ec::UserVars, "var"),
        (Ec::Breakpoint, "breakpoint"),
        (Ec::Scriptnames, "scriptnames"),
        (Ec::DirsInCdpath, "dir_in_path"),
    ]
};

/// Look up the `-complete=` name for a given `xp_context` value, as a
/// raw `i32` (`get_command_complete`). Returns `None` for any `arg`
/// absent from [`COMMAND_COMPLETE`] - covering, in one check, both the
/// original's own explicit bounds check (`arg < 0 || arg >=
/// ARRAY_SIZE(command_complete)`) and its sparse array's implicit
/// `NULL` for an in-bounds-but-unpopulated index; every `ExpandContext`
/// variant not listed in `COMMAND_COMPLETE` falls into exactly one of
/// those two original cases.
#[must_use]
fn get_command_complete(arg: i32) -> Option<&'static str> {
    COMMAND_COMPLETE.iter().find(|(ec, _)| *ec as i32 == arg).map(|&(_, name)| name)
}

/// `ARRAY_SIZE(command_complete)`.
///
/// The original's `command_complete[]` is a SPARSE C designated-
/// initializer array indexed by the `EXPAND_*` value, so its length is
/// the largest index it initialises plus one - not its number of
/// populated entries. That distinction matters because
/// [`get_user_cmd_complete`] bounds-checks against the length but then
/// returns `""` for the unpopulated holes below it.
///
/// Derived from [`COMMAND_COMPLETE`] rather than hardcoded, so it
/// cannot drift if an entry is ever added.
const fn command_complete_len() -> i32 {
    let mut max = 0;
    let mut i = 0;
    while i < COMMAND_COMPLETE.len() {
        let v = COMMAND_COMPLETE[i].0 as i32;
        if v > max {
            max = v;
        }
        i += 1;
    }
    max + 1
}

/// Obtain the list of user address type names, for `ExpandGeneric()`
/// (`get_user_cmd_addr_type`).
///
/// The original indexes `addr_type_complete[]` directly and relies on
/// its trailing `{ ADDR_NONE, NULL, NULL }` sentinel to return `NULL`
/// and stop the caller's enumeration. `ADDR_TYPE_COMPLETE` holds only
/// the eight real entries, so running off its end is exactly that
/// sentinel.
#[must_use]
pub fn get_user_cmd_addr_type(idx: i32) -> Option<&'static str> {
    if idx < 0 {
        return None;
    }
    ADDR_TYPE_COMPLETE.get(idx as usize).map(|&(_, name, _)| name)
}

/// Obtain the list of user command attributes, for `ExpandGeneric()`
/// (`get_user_cmd_flags`).
#[must_use]
pub fn get_user_cmd_flags(idx: i32) -> Option<&'static str> {
    const USER_CMD_FLAGS: &[&str] = &[
        "addr", "bang", "bar", "buffer", "complete", "count", "nargs", "range", "register",
        "keepscript",
    ];
    if idx < 0 {
        return None;
    }
    USER_CMD_FLAGS.get(idx as usize).copied()
}

/// Obtain the list of values for `-nargs`, for `ExpandGeneric()`
/// (`get_user_cmd_nargs`).
#[must_use]
pub fn get_user_cmd_nargs(idx: i32) -> Option<&'static str> {
    const USER_CMD_NARGS: &[&str] = &["0", "1", "_", "*", "?", "+"];
    if idx < 0 {
        return None;
    }
    USER_CMD_NARGS.get(idx as usize).copied()
}

/// Obtain the list of values for `-complete`, for `ExpandGeneric()`
/// (`get_user_cmd_complete`).
///
/// Returns `Some("")` - NOT `None` - for an in-range index that names
/// no completion type, so the caller keeps enumerating past the sparse
/// array's holes; `None` only once `idx` runs past the array itself.
/// `EXPAND_USER_LUA` is deliberately blanked the same way, since
/// `"<Lua function>"` is not a name a user can type after `-complete=`.
#[must_use]
pub fn get_user_cmd_complete(idx: i32) -> Option<&'static str> {
    if idx < 0 || idx >= command_complete_len() {
        return None;
    }
    match get_command_complete(idx) {
        Some(name) if idx != crate::cmdexpand_defs::ExpandContext::UserLua as i32 => Some(name),
        _ => Some(""),
    }
}

/// Get the name of completion type `expand` as an owned byte string,
/// or `None` if no completion is available (`cmdcomplete_type_to_str`).
/// `compl_arg` is the function name for the `"custom"`/`"customlist"`
/// types.
///
/// `expand` stays a raw `i32` (matching the original's own `int
/// expand` parameter) rather than
/// [`crate::cmdexpand_defs::ExpandContext`] directly, since real
/// callers (`cmdexpand.c`'s `f_getcompletion`/`ex_getln.c`'s
/// `set_context_by_ecmd_flag`-adjacent completion state) pass an
/// already-live `xp_context` value that may not correspond to any
/// declared variant name (never itself asserted valid before this
/// call in the original either).
#[must_use]
pub fn cmdcomplete_type_to_str(expand: i32, compl_arg: &[u8]) -> Option<Vec<u8>> {
    let cmd_compl = get_command_complete(expand)?;
    if expand == crate::cmdexpand_defs::ExpandContext::UserLua as i32 {
        return None;
    }

    if expand == crate::cmdexpand_defs::ExpandContext::UserList as i32
        || expand == crate::cmdexpand_defs::ExpandContext::UserDefined as i32
    {
        let mut buffer = Vec::with_capacity(cmd_compl.len() + compl_arg.len() + 1);
        buffer.extend_from_slice(cmd_compl.as_bytes());
        buffer.push(b',');
        buffer.extend_from_slice(compl_arg);
        return Some(buffer);
    }

    Some(cmd_compl.as_bytes().to_vec())
}

/// Parse a `-complete=` argument's own type name back into its
/// `xp_context` value (`cmdcomplete_str_to_type`) - the inverse of
/// [`cmdcomplete_type_to_str`]. Returns
/// [`crate::cmdexpand_defs::ExpandContext::Nothing`] if `complete_str`
/// doesn't match any known completion type name, matching the
/// original's own `return EXPAND_NOTHING;` fallback.
#[must_use]
pub fn cmdcomplete_str_to_type(complete_str: &[u8]) -> crate::cmdexpand_defs::ExpandContext {
    if complete_str.starts_with(b"custom,") {
        return crate::cmdexpand_defs::ExpandContext::UserDefined;
    }
    if complete_str.starts_with(b"customlist,") {
        return crate::cmdexpand_defs::ExpandContext::UserList;
    }

    COMMAND_COMPLETE
        .iter()
        .find(|(_, name)| name.as_bytes() == complete_str)
        .map_or(crate::cmdexpand_defs::ExpandContext::Nothing, |&(ec, _)| ec)
}

/// Parse a completion argument `value` (`parse_compl_arg`). The
/// detected completion type is written into `*complp`; `EX_BUFNAME`/
/// `EX_XFILE` are OR'd into `*argt` for the completion types that need
/// them; any argument part after a `,` (used by the `"custom"`/
/// `"customlist"` types, e.g. `-complete=custom,MyFunc`) is copied
/// into `*compl_arg`.
///
/// Omits the original's own `semsg`/`emsg` error-message display on
/// both failure paths, matching this crate's established "skip the
/// deferred message-display side effect, keep the exact same return
/// value" policy (e.g. `window::check_split_disallowed_err`).
///
/// No real caller is translated yet (`uc_scan_attr`, the `:command`
/// attribute parser) - harvested ahead of it, matching this crate's
/// established precedent for a small, self-contained function with no
/// design freedom of its own.
pub fn parse_compl_arg(
    value: &[u8],
    complp: &mut crate::cmdexpand_defs::ExpandContext,
    argt: &mut u32,
    compl_arg: &mut Option<Vec<u8>>,
) -> i32 {
    // Look for any argument part - the part after any ','.
    let mut arg: Option<&[u8]> = None;
    let mut valend = value.len();
    if let Some(comma) = value.iter().position(|&b| b == b',') {
        arg = Some(&value[comma + 1..]);
        valend = comma;
    }

    let value_prefix = &value[..valend];
    let Some(&(ec, _)) = COMMAND_COMPLETE.iter().find(|(_, name)| name.as_bytes() == value_prefix) else {
        return crate::vim_defs::FAIL;
    };
    *complp = ec;
    if ec == crate::cmdexpand_defs::ExpandContext::Buffers {
        *argt |= crate::ex_cmds_defs::ex_flags::BUFNAME;
    } else if matches!(
        ec,
        crate::cmdexpand_defs::ExpandContext::Directories
            | crate::cmdexpand_defs::ExpandContext::Files
            | crate::cmdexpand_defs::ExpandContext::Shellcmdline
    ) {
        *argt |= crate::ex_cmds_defs::ex_flags::XFILE;
    }

    let is_user_custom = matches!(
        *complp,
        crate::cmdexpand_defs::ExpandContext::UserDefined | crate::cmdexpand_defs::ExpandContext::UserList
    );
    if !is_user_custom && arg.is_some() {
        return crate::vim_defs::FAIL;
    }
    if is_user_custom && arg.is_none() {
        return crate::vim_defs::FAIL;
    }

    if let Some(a) = arg {
        *compl_arg = Some(a.to_vec());
    }
    crate::vim_defs::OK
}

/// Return the byte offset within `name` right after a valid command
/// name (an alphabetic character followed by zero or more alphanumeric
/// characters), or `None` if `name` doesn't start with a valid command
/// name (`uc_validate_name`).
///
/// Matches the original's own subtlety: if `name`'s first byte isn't
/// alphabetic, NO characters are consumed at all (the alphanumeric-run
/// scan is only entered when the very first byte is alphabetic) - so a
/// `name` that immediately ends the command (e.g. is empty, or starts
/// with whitespace/`|`/`"`) still succeeds with an offset of `0`, not a
/// `None` failure. Running off the end of `name` during the
/// alphanumeric scan is treated the same as hitting a real NUL
/// terminator would in the original (matching [`ends_excmd`]'s own
/// `c == 0` "ran off the end" convention).
///
/// No real caller is translated yet (`ex_command`, the `:command`
/// command's own handler) - harvested ahead of it, matching this
/// crate's established precedent for a small, self-contained function
/// with no design freedom of its own.
#[must_use]
pub fn uc_validate_name(name: &[u8]) -> Option<usize> {
    let mut i = 0;
    if name.first().is_some_and(|&c| ascii_isalpha(i32::from(c))) {
        while name.get(i).is_some_and(|&c| ascii_isalnum(i32::from(c))) {
            i += 1;
        }
    }
    let c = name.get(i).copied().unwrap_or(0);
    if !ends_excmd(c) && !ascii_iswhite(i32::from(c)) {
        return None;
    }
    Some(i)
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

    // --- get_command_complete / cmdcomplete_type_to_str / cmdcomplete_str_to_type ---

    #[test]
    fn get_command_complete_finds_every_entry_by_its_own_discriminant() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        for &(ec, name) in COMMAND_COMPLETE {
            assert_eq!(get_command_complete(ec as i32), Some(name), "mismatch for {ec:?}");
        }
        // A representative negative value and a value known to be
        // absent from the sparse array (e.g. Nothing == 0, never
        // itself a completion type name).
        assert_eq!(get_command_complete(Ec::Unsuccessful as i32), None);
        assert_eq!(get_command_complete(Ec::Nothing as i32), None);
        assert_eq!(get_command_complete(9999), None);
    }

    #[test]
    fn cmdcomplete_type_to_str_plain_type_returns_its_own_name() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        assert_eq!(cmdcomplete_type_to_str(Ec::Buffers as i32, b""), Some(b"buffer".to_vec()));
        assert_eq!(cmdcomplete_type_to_str(Ec::Files as i32, b""), Some(b"file".to_vec()));
    }

    #[test]
    fn cmdcomplete_type_to_str_user_lua_is_always_none() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        // UserLua IS present in COMMAND_COMPLETE (as "<Lua function>")
        // but is specifically excluded by its own real caller-visible
        // check, matching the original's own `expand == EXPAND_USER_LUA`
        // special case.
        assert_eq!(cmdcomplete_type_to_str(Ec::UserLua as i32, b""), None);
    }

    #[test]
    fn cmdcomplete_type_to_str_user_defined_and_list_append_the_compl_arg() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        assert_eq!(
            cmdcomplete_type_to_str(Ec::UserDefined as i32, b"MyFunc"),
            Some(b"custom,MyFunc".to_vec())
        );
        assert_eq!(
            cmdcomplete_type_to_str(Ec::UserList as i32, b"MyFunc"),
            Some(b"customlist,MyFunc".to_vec())
        );
    }

    #[test]
    fn cmdcomplete_type_to_str_unknown_type_is_none() {
        assert_eq!(cmdcomplete_type_to_str(9999, b""), None);
    }

    #[test]
    fn cmdcomplete_str_to_type_recognizes_custom_and_customlist_prefixes() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        assert_eq!(cmdcomplete_str_to_type(b"custom,MyFunc"), Ec::UserDefined);
        assert_eq!(cmdcomplete_str_to_type(b"customlist,MyFunc"), Ec::UserList);
    }

    #[test]
    fn cmdcomplete_str_to_type_is_the_inverse_of_get_command_complete() {
        for &(ec, name) in COMMAND_COMPLETE {
            // "custom"/"customlist" are handled by their own dedicated
            // prefix check first in the real function (never reached
            // via a bare, comma-less name for those two specifically),
            // so they're excluded from this generic round-trip check.
            if name == "custom" || name == "customlist" {
                continue;
            }
            assert_eq!(cmdcomplete_str_to_type(name.as_bytes()), ec, "mismatch for {name:?}");
        }
    }

    #[test]
    fn cmdcomplete_str_to_type_unrecognized_name_is_nothing() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        assert_eq!(cmdcomplete_str_to_type(b"not-a-real-type"), Ec::Nothing);
        assert_eq!(cmdcomplete_str_to_type(b""), Ec::Nothing);
    }

    // --- parse_compl_arg ---

    #[test]
    fn parse_compl_arg_plain_type_succeeds_with_no_argument() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        let mut complp = Ec::Nothing;
        let mut argt = 0u32;
        let mut compl_arg = None;
        let rc = parse_compl_arg(b"command", &mut complp, &mut argt, &mut compl_arg);
        assert_eq!(rc, crate::vim_defs::OK);
        assert_eq!(complp, Ec::Commands);
        assert_eq!(argt, 0);
        assert_eq!(compl_arg, None);
    }

    #[test]
    fn parse_compl_arg_buffer_type_sets_ex_bufname() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        let mut complp = Ec::Nothing;
        let mut argt = 0u32;
        let mut compl_arg = None;
        let rc = parse_compl_arg(b"buffer", &mut complp, &mut argt, &mut compl_arg);
        assert_eq!(rc, crate::vim_defs::OK);
        assert_eq!(complp, Ec::Buffers);
        assert_eq!(argt, crate::ex_cmds_defs::ex_flags::BUFNAME);
    }

    #[test]
    fn parse_compl_arg_file_like_types_set_ex_xfile() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        for name in [&b"dir"[..], b"file", b"shellcmdline"] {
            let mut complp = Ec::Nothing;
            let mut argt = 0u32;
            let mut compl_arg = None;
            let rc = parse_compl_arg(name, &mut complp, &mut argt, &mut compl_arg);
            assert_eq!(rc, crate::vim_defs::OK);
            assert_eq!(argt, crate::ex_cmds_defs::ex_flags::XFILE, "mismatch for {name:?}");
        }
    }

    #[test]
    fn parse_compl_arg_custom_with_a_function_argument_succeeds() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        let mut complp = Ec::Nothing;
        let mut argt = 0u32;
        let mut compl_arg = None;
        let rc = parse_compl_arg(b"custom,MyFunc", &mut complp, &mut argt, &mut compl_arg);
        assert_eq!(rc, crate::vim_defs::OK);
        assert_eq!(complp, Ec::UserDefined);
        assert_eq!(compl_arg, Some(b"MyFunc".to_vec()));
    }

    #[test]
    fn parse_compl_arg_customlist_with_a_function_argument_succeeds() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        let mut complp = Ec::Nothing;
        let mut argt = 0u32;
        let mut compl_arg = None;
        let rc = parse_compl_arg(b"customlist,MyFunc", &mut complp, &mut argt, &mut compl_arg);
        assert_eq!(rc, crate::vim_defs::OK);
        assert_eq!(complp, Ec::UserList);
        assert_eq!(compl_arg, Some(b"MyFunc".to_vec()));
    }

    #[test]
    fn parse_compl_arg_custom_without_a_function_argument_fails() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        let mut complp = Ec::Nothing;
        let mut argt = 0u32;
        let mut compl_arg = None;
        let rc = parse_compl_arg(b"custom", &mut complp, &mut argt, &mut compl_arg);
        assert_eq!(rc, crate::vim_defs::FAIL);
    }

    #[test]
    fn parse_compl_arg_non_custom_type_with_an_argument_fails() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        let mut complp = Ec::Nothing;
        let mut argt = 0u32;
        let mut compl_arg = None;
        // "buffer" never takes a function argument (only "custom"/
        // "customlist" do), so a trailing ",whatever" is rejected.
        let rc = parse_compl_arg(b"buffer,whatever", &mut complp, &mut argt, &mut compl_arg);
        assert_eq!(rc, crate::vim_defs::FAIL);
    }

    #[test]
    fn parse_compl_arg_unknown_type_fails() {
        use crate::cmdexpand_defs::ExpandContext as Ec;
        let mut complp = Ec::Nothing;
        let mut argt = 0u32;
        let mut compl_arg = None;
        let rc = parse_compl_arg(b"not-a-real-type", &mut complp, &mut argt, &mut compl_arg);
        assert_eq!(rc, crate::vim_defs::FAIL);
    }

    // --- uc_validate_name ---

    #[test]
    fn uc_validate_name_all_alnum_runs_to_the_end_of_the_slice() {
        // "MyCmd" is entirely alphanumeric with nothing after it -
        // running off the end is treated like a NUL terminator,
        // which counts as ending the command.
        assert_eq!(uc_validate_name(b"MyCmd"), Some(5));
    }

    #[test]
    fn uc_validate_name_stops_at_whitespace() {
        assert_eq!(uc_validate_name(b"MyCmd arg"), Some(5));
    }

    #[test]
    fn uc_validate_name_stops_at_a_pipe() {
        assert_eq!(uc_validate_name(b"MyCmd|"), Some(5));
    }

    #[test]
    fn uc_validate_name_stops_at_a_comment_quote() {
        assert_eq!(uc_validate_name(b"MyCmd\"comment"), Some(5));
    }

    #[test]
    fn uc_validate_name_rejects_a_non_name_character_right_after_the_alnum_run() {
        // "-" is neither alphanumeric (so it stops the scan) nor an
        // end-of-command/whitespace character - real command names
        // can't contain it.
        assert_eq!(uc_validate_name(b"My-Cmd"), None);
    }

    #[test]
    fn uc_validate_name_empty_slice_succeeds_with_offset_zero() {
        // No characters at all to scan; running straight off the end
        // is itself an "ends the command" position.
        assert_eq!(uc_validate_name(b""), Some(0));
    }

    #[test]
    fn uc_validate_name_leading_whitespace_succeeds_with_offset_zero() {
        // The first byte isn't alphabetic, so no scan is entered at
        // all - the check falls straight through to "is position 0
        // itself an end/whitespace byte", which it is here.
        assert_eq!(uc_validate_name(b" foo"), Some(0));
    }

    #[test]
    fn uc_validate_name_rejects_a_name_starting_with_a_digit() {
        // The first byte isn't alphabetic (a digit doesn't count), so
        // no scan is entered - position 0 itself ('1') is neither an
        // end-of-command nor whitespace byte, so this fails.
        assert_eq!(uc_validate_name(b"123abc"), None);
    }

    /// The expected sets below were all read out of a real `nvim`
    /// binary with `getcompletion('command -nargs=', 'cmdline')` and
    /// friends before being written here.
    #[test]
    fn get_user_cmd_nargs_enumerates_the_six_nargs_values() {
        let mut got: Vec<&str> = (0..).map_while(get_user_cmd_nargs).collect();
        got.sort_unstable();
        assert_eq!(got, ["*", "+", "0", "1", "?", "_"]);
    }

    #[test]
    fn get_user_cmd_flags_enumerates_the_ten_attributes() {
        let mut got: Vec<&str> = (0..).map_while(get_user_cmd_flags).collect();
        got.sort_unstable();
        assert_eq!(
            got,
            [
                "addr", "bang", "bar", "buffer", "complete", "count", "keepscript", "nargs",
                "range", "register"
            ]
        );
    }

    #[test]
    fn get_user_cmd_addr_type_enumerates_the_eight_address_types() {
        let mut got: Vec<&str> = (0..).map_while(get_user_cmd_addr_type).collect();
        got.sort_unstable();
        assert_eq!(
            got,
            [
                "arguments",
                "buffers",
                "lines",
                "loaded_buffers",
                "other",
                "quickfix",
                "tabs",
                "windows"
            ]
        );
    }

    #[test]
    fn get_user_cmd_complete_yields_45_real_names_plus_blanks() {
        let all: Vec<&str> = (0..).map_while(get_user_cmd_complete).collect();
        // The bound is the sparse array's LENGTH, not its number of
        // populated entries, so enumeration runs well past 46.
        assert_eq!(all.len() as i32, command_complete_len());

        let mut named: Vec<&str> = all.into_iter().filter(|s| !s.is_empty()).collect();
        named.sort_unstable();
        // Real nvim reports exactly 45 completable names here, one
        // fewer than COMMAND_COMPLETE's 46 entries, because
        // "<Lua function>" is blanked out.
        assert_eq!(named.len(), 45);
        assert_eq!(COMMAND_COMPLETE.len(), 46);
        assert!(!named.contains(&"<Lua function>"));
        assert_eq!(named.first(), Some(&"arglist"));
        assert_eq!(named.last(), Some(&"var"));
        for expected in ["custom", "customlist", "dir_in_path", "retab", "filetypecmd"] {
            assert!(named.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn get_user_cmd_complete_blanks_the_lua_entry_rather_than_ending() {
        let lua = crate::cmdexpand_defs::ExpandContext::UserLua as i32;
        // Present in the table...
        assert_eq!(get_command_complete(lua), Some("<Lua function>"));
        // ...but not offered as a typeable -complete= name.
        assert_eq!(get_user_cmd_complete(lua), Some(""));
    }

    #[test]
    fn get_user_cmd_complete_returns_a_blank_for_an_unpopulated_hole() {
        // ExpandContext::Nothing (0) is a real in-range index that
        // `command_complete[]` never initialises.
        let nothing = crate::cmdexpand_defs::ExpandContext::Nothing as i32;
        assert_eq!(get_command_complete(nothing), None);
        assert_eq!(get_user_cmd_complete(nothing), Some(""));
    }

    #[test]
    fn the_expand_generic_callbacks_reject_negative_indices() {
        // The original is only ever called with idx >= 0; guarding
        // keeps the `as usize` cast from wrapping into a huge index.
        assert_eq!(get_user_cmd_nargs(-1), None);
        assert_eq!(get_user_cmd_flags(-1), None);
        assert_eq!(get_user_cmd_addr_type(-1), None);
        assert_eq!(get_user_cmd_complete(-1), None);
    }
}
