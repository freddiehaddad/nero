//! Translated from `src/nvim/cmdhist.c` (tractable core only).
//!
//! `cmdhist.c` manages the command-line history (`:history`, up/down
//! arrow recall) - almost entirely dependent on the command-line
//! editing subsystem (`ex_getln.c`, which explicitly does not attempt
//! the history-editing machinery either - see that module's own doc
//! comment).
//!
//! Translated: [`HistoryType`], [`HIST_COUNT`], [`get_hislen`] (reading
//! a new file-static `HISLEN`, always `0` today since nothing can add a
//! history entry yet), [`hist_char2type`] - a pure character-to-enum
//! mapping with no dependencies at all - and [`get_histtype`]/
//! [`get_history_idx`]/[`f_histnr`] (`histnr()`): tractable now that
//! `HISLEN` exists (making `get_history_idx`'s own real early-return
//! always taken today) and `ex_getln.rs` gained
//! `get_cmdline_firstc()`.
//!
//! Also `calc_hist_idx`/[`f_histget`] (`histget()`): `calc_hist_idx`'s
//! own real early-return condition has `hislen == 0` as its FIRST,
//! short-circuited disjunct - always true today, making the whole
//! condition unconditionally true regardless of the other disjuncts
//! (only the `hisidx[histype] < 0` one, needing the real, not-yet-
//! populated `history[]`/`hisidx[]` arrays, is omitted from the
//! translated condition for this reason - see its own doc comment).
//! `histget()` itself is therefore always an empty string today
//! (never a real history entry to return) unless `{history}` itself
//! is a type error, matching the original's own `NULL`-string case.
//!
//! Deferred: everything else - `get_histentry`/`set_histentry`/
//! `get_hisidx`/`get_hisnum`/`get_history_arg`/`init_history`/
//! `add_to_history`/`clr_history`/`f_histadd`/`f_histdel`/
//! `ex_history` (need `histentry_T`'s own `AdditionalData`/full
//! history-table storage, and the command-line editing subsystem to
//! ever populate it).

use crate::globals::GlobalCell;

/// Present history tables (`HistoryType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum HistoryType {
    /// Default (current) history.
    Default = -2,
    /// Unknown history.
    Invalid = -1,
    /// Colon commands.
    Cmd = 0,
    /// Search commands.
    Search = 1,
    /// Expressions (e.g. from entering a `=` register).
    Expr = 2,
    /// `input()` lines.
    Input = 3,
    /// Debug commands.
    Debug = 4,
}

/// Number of history tables (`HIST_COUNT`).
pub const HIST_COUNT: usize = 5;

/// actual length of the history tables (`hislen`).
static HISLEN: GlobalCell<i32> = GlobalCell::new(0);

/// Returns the length of the history tables (`get_hislen`).
///
/// Always `0` today: nothing in this crate can currently add a history
/// entry (`init_history`/`add_to_history`, not translated), so
/// `HISLEN` never becomes anything else - a real, faithful consequence
/// of the current state, not a hardcoded stub.
#[must_use]
pub fn get_hislen() -> i32 {
    // SAFETY: a plain read through one exclusive borrow.
    unsafe { *HISLEN.get_mut() }
}

/// Translates a history character (`:`, `=`, `@`, `>`, `/`, `?`, or NUL)
/// to its associated [`HistoryType`], or [`HistoryType::Invalid`] for
/// anything else (`hist_char2type`).
#[must_use]
pub fn hist_char2type(c: i32) -> HistoryType {
    if c == i32::from(b':') {
        HistoryType::Cmd
    } else if c == i32::from(b'=') {
        HistoryType::Expr
    } else if c == i32::from(b'@') {
        HistoryType::Input
    } else if c == i32::from(b'>') {
        HistoryType::Debug
    } else if c == 0 || c == i32::from(b'/') || c == i32::from(b'?') {
        HistoryType::Search
    } else {
        HistoryType::Invalid
    }
}

/// Table of history names (`history_names[]`), used by
/// [`get_histtype`] and (in the original) `:history`'s own argument
/// completion.
const HISTORY_NAMES: [&[u8]; HIST_COUNT] = [b"cmd", b"search", b"expr", b"input", b"debug"];

/// The corresponding [`HistoryType`] for each entry of
/// `HISTORY_NAMES`, in the same order.
const HISTORY_NAME_TYPES: [HistoryType; HIST_COUNT] = [
    HistoryType::Cmd,
    HistoryType::Search,
    HistoryType::Expr,
    HistoryType::Input,
    HistoryType::Debug,
];

/// Parses a history-name argument into its [`HistoryType`]
/// (`get_histtype`) - a case-insensitive prefix match against
/// `HISTORY_NAMES` (e.g. `"s"`/`"sea"`/`"search"` all match
/// `Search`), or a single history-marker character (`:`/`=`/`@`/`>`/
/// `/`/`?`). An empty `name` resolves to either [`HistoryType::Default`]
/// (when `return_default` is set) or the CURRENT command line's own
/// leading character via [`crate::ex_getln::get_cmdline_firstc`]/
/// [`hist_char2type`] - always resolves to [`HistoryType::Search`]
/// today, since `get_cmdline_firstc()` is always `0` (NUL) and
/// `hist_char2type(0)` is `Search`.
#[must_use]
pub fn get_histtype(name: &[u8], return_default: bool) -> HistoryType {
    if name.is_empty() {
        return if return_default {
            HistoryType::Default
        } else {
            hist_char2type(crate::ex_getln::get_cmdline_firstc())
        };
    }

    for (hn, ty) in HISTORY_NAMES.iter().zip(HISTORY_NAME_TYPES.iter()) {
        // `STRNICMP(name, hn, name.len())`: a case-insensitive
        // comparison of `name`'s own full length against `hn`'s FIRST
        // `name.len()` bytes - only meaningful (and only ever matches)
        // when `name` is no longer than `hn` itself, since C's
        // `strncasecmp` would otherwise compare past `hn`'s own NUL
        // terminator against a byte that can never be NUL in `name`.
        if name.len() <= hn.len() && name.eq_ignore_ascii_case(&hn[..name.len()]) {
            return *ty;
        }
    }

    if name.len() == 1 && b":=@>?/".contains(&name[0]) {
        return hist_char2type(i32::from(name[0]));
    }

    HistoryType::Invalid
}

/// Gets the identifier of the newest entry in history table `histype`
/// (`get_history_idx`). Always `-1` today: [`get_hislen`] always
/// returns `0` (no history entry can exist yet), so this function's
/// own real early-return condition is always taken - a faithful,
/// always-taken early return (matching this crate's established
/// `AUTOCMDS`/`cmdline_is_active` precedent), not a hardcoded
/// shortcut. The real per-table `hisidx[]` lookup (reached only once
/// `init_history`/`add_to_history` exist) is `unimplemented!()`.
#[must_use]
pub fn get_history_idx(histype: HistoryType) -> i32 {
    if get_hislen() == 0 || (histype as i32) < 0 || (histype as i32) >= HIST_COUNT as i32 {
        return -1;
    }
    unimplemented!("get_history_idx: needs the real hisidx[]/history[] table, not yet translated")
}

/// `histnr({history})` - the identifier of the newest entry in the
/// given history table, or `-1` for an unknown history name
/// (`f_histnr`, `cmdhist.c`). Always `-1` today, since
/// [`get_history_idx`]'s own real early-return is always taken.
pub fn f_histnr(
    argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    let histname = crate::eval::typval::tv_get_string_chk(&argvars[0]);
    let i = match &histname {
        Some(name) => get_histtype(name, false),
        None => HistoryType::Invalid,
    };
    let n = if i == HistoryType::Invalid { HistoryType::Invalid as i32 } else { get_history_idx(i) };
    rettv.value = crate::eval::typval_defs::TypvalValue::Number(i64::from(n));
}

/// Calculate an entry's index in the history array for a given
/// history number (`calc_hist_idx`).
///
/// `hislen == 0` (see [`get_hislen`]) is ALWAYS true today - the FIRST
/// short-circuited disjunct of the original's own condition
/// (`hislen == 0 || histype < 0 || histype >= HIST_COUNT ||
/// hisidx[histype] < 0 || num == 0`), so it alone makes the whole
/// "-1, not found" early return unconditional today, regardless of
/// every OTHER disjunct's own value. The `histype` bounds/`num == 0`
/// checks are still translated faithfully (cheap, no blocked
/// dependency, matching [`get_history_idx`]'s own identical
/// treatment); only `hisidx[histype] < 0` is omitted, since it needs
/// the real, not-yet-populated `history[]`/`hisidx[]` arrays and can
/// NEVER be reached while `hislen` stays `0`. The function's own
/// remaining body (walking `history[histype]` to find a matching
/// entry) is `unimplemented!()`, unreachable for the same reason.
fn calc_hist_idx(histype: HistoryType, num: i32) -> i32 {
    if get_hislen() == 0 || (histype as i32) < 0 || (histype as i32) >= HIST_COUNT as i32 || num == 0 {
        return -1;
    }
    unimplemented!(
        "calc_hist_idx: needs the real history[]/hisidx[] arrays \
         (init_history/add_to_history, not translated)"
    )
}

/// `histget({history} [, {index}])` - an entry from the given
/// command-line history, or an empty string if there is no such entry
/// (`f_histget`, `cmdhist.c`). Always an empty string when `{history}`
/// itself is valid, since `calc_hist_idx`'s own real early-return is
/// always taken today (never a real history entry to return). A
/// type-error on `{history}` itself (`tv_get_string_chk` returning
/// `None`) resolves to a `None` (null) string, matching the original's
/// own `rettv->vval.v_string = NULL` for that specific case.
pub fn f_histget(
    argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    let histname = crate::eval::typval::tv_get_string_chk(&argvars[0]);
    rettv.value = crate::eval::typval_defs::TypvalValue::String(match histname {
        None => None,
        Some(name) => {
            let histype = get_histtype(&name, false);
            let idx = if argvars.len() > 1 {
                crate::eval::typval::tv_get_number_chk(&argvars[1], None) as i32
            } else {
                get_history_idx(histype)
            };
            if calc_hist_idx(histype, idx) < 0 {
                Some(Vec::new())
            } else {
                unimplemented!(
                    "f_histget: a real match needs the history[] array, unreachable today \
                     since calc_hist_idx always returns -1"
                )
            }
        }
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Test-only helper letting tests set the otherwise-private
    /// `HISLEN` value, matching the established `set_pum_is_visible`-
    /// style pattern (`popupmenu.rs`). Caller must hold
    /// `crate::globals::global_state_test_lock()` for the whole
    /// duration this value matters, and should restore the original
    /// value before releasing the lock.
    pub(crate) fn set_hislen(value: i32) -> i32 {
        let cell = unsafe { HISLEN.get_mut() };
        let old = *cell;
        *cell = value;
        old
    }

    #[test]
    fn get_hislen_is_zero_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(get_hislen(), 0);
    }

    #[test]
    fn get_hislen_reflects_the_underlying_value() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_hislen(42);
        assert_eq!(get_hislen(), 42);
        set_hislen(old);
    }

    #[test]
    fn hist_char2type_maps_every_known_character() {
        assert_eq!(hist_char2type(i32::from(b':')), HistoryType::Cmd);
        assert_eq!(hist_char2type(i32::from(b'=')), HistoryType::Expr);
        assert_eq!(hist_char2type(i32::from(b'@')), HistoryType::Input);
        assert_eq!(hist_char2type(i32::from(b'>')), HistoryType::Debug);
        assert_eq!(hist_char2type(i32::from(b'/')), HistoryType::Search);
        assert_eq!(hist_char2type(i32::from(b'?')), HistoryType::Search);
        assert_eq!(hist_char2type(0), HistoryType::Search);
    }

    #[test]
    fn hist_char2type_unknown_char_is_invalid() {
        assert_eq!(hist_char2type(i32::from(b'x')), HistoryType::Invalid);
        assert_eq!(hist_char2type(-1), HistoryType::Invalid);
    }

    // --- get_histtype ---

    #[test]
    fn get_histtype_matches_full_names_case_insensitively() {
        assert_eq!(get_histtype(b"cmd", false), HistoryType::Cmd);
        assert_eq!(get_histtype(b"CMD", false), HistoryType::Cmd);
        assert_eq!(get_histtype(b"search", false), HistoryType::Search);
        assert_eq!(get_histtype(b"expr", false), HistoryType::Expr);
        assert_eq!(get_histtype(b"input", false), HistoryType::Input);
        assert_eq!(get_histtype(b"debug", false), HistoryType::Debug);
    }

    #[test]
    fn get_histtype_matches_a_significant_prefix() {
        // "It is sufficient to give the significant prefix of a
        // history name" (the original's own comment on
        // `history_names[]`).
        assert_eq!(get_histtype(b"s", false), HistoryType::Search);
        assert_eq!(get_histtype(b"sea", false), HistoryType::Search);
        assert_eq!(get_histtype(b"c", false), HistoryType::Cmd);
    }

    #[test]
    fn get_histtype_a_name_longer_than_any_table_entry_does_not_match() {
        // "searching" is longer than "search" itself - STRNICMP would
        // compare past "search"'s own NUL terminator and fail, so this
        // must NOT match Search (or anything else).
        assert_eq!(get_histtype(b"searching", false), HistoryType::Invalid);
    }

    #[test]
    fn get_histtype_matches_a_single_marker_character() {
        assert_eq!(get_histtype(b":", false), HistoryType::Cmd);
        assert_eq!(get_histtype(b"=", false), HistoryType::Expr);
        assert_eq!(get_histtype(b"@", false), HistoryType::Input);
        assert_eq!(get_histtype(b">", false), HistoryType::Debug);
        assert_eq!(get_histtype(b"/", false), HistoryType::Search);
        assert_eq!(get_histtype(b"?", false), HistoryType::Search);
    }

    #[test]
    fn get_histtype_unrecognized_name_is_invalid() {
        assert_eq!(get_histtype(b"bogus", false), HistoryType::Invalid);
        assert_eq!(get_histtype(b"x", false), HistoryType::Invalid);
    }

    #[test]
    fn get_histtype_empty_name_resolves_via_cmdline_firstc() {
        let _lock = crate::globals::global_state_test_lock();
        // get_cmdline_firstc() is always 0 (NUL) today, and
        // hist_char2type(0) is Search.
        assert_eq!(get_histtype(b"", false), HistoryType::Search);
    }

    #[test]
    fn get_histtype_empty_name_with_return_default_is_default() {
        assert_eq!(get_histtype(b"", true), HistoryType::Default);
    }

    // --- get_history_idx / f_histnr ---

    #[test]
    fn get_history_idx_is_negative_one_when_hislen_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(get_hislen(), 0);
        assert_eq!(get_history_idx(HistoryType::Cmd), -1);
        assert_eq!(get_history_idx(HistoryType::Search), -1);
    }

    #[test]
    fn get_history_idx_is_negative_one_for_an_out_of_range_type() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(get_history_idx(HistoryType::Invalid), -1);
        assert_eq!(get_history_idx(HistoryType::Default), -1);
    }

    #[test]
    #[should_panic(expected = "get_history_idx: needs the real hisidx")]
    fn get_history_idx_panics_when_hislen_is_genuinely_nonzero() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_hislen(10);
        let result = std::panic::catch_unwind(|| get_history_idx(HistoryType::Cmd));
        set_hislen(old);
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    }

    #[test]
    fn histnr_of_a_known_history_name_is_negative_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_histnr(
            &[crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::String(Some(b"cmd".to_vec())),
                ..Default::default()
            }],
            &mut rettv,
        );
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(-1));
    }

    #[test]
    fn histnr_of_an_unknown_history_name_is_also_negative_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_histnr(
            &[crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::String(Some(b"bogus".to_vec())),
                ..Default::default()
            }],
            &mut rettv,
        );
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(-1));
    }

    // --- calc_hist_idx / f_histget ---

    #[test]
    fn calc_hist_idx_is_negative_one_when_hislen_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(get_hislen(), 0);
        assert_eq!(calc_hist_idx(HistoryType::Cmd, 1), -1);
        assert_eq!(calc_hist_idx(HistoryType::Search, -1), -1);
    }

    #[test]
    fn calc_hist_idx_is_negative_one_for_an_out_of_range_type_or_zero_num() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(calc_hist_idx(HistoryType::Invalid, 1), -1);
        assert_eq!(calc_hist_idx(HistoryType::Default, 1), -1);
        assert_eq!(calc_hist_idx(HistoryType::Cmd, 0), -1);
    }

    #[test]
    #[should_panic(expected = "calc_hist_idx: needs the real history")]
    fn calc_hist_idx_panics_when_hislen_is_genuinely_nonzero() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_hislen(10);
        let result = std::panic::catch_unwind(|| calc_hist_idx(HistoryType::Cmd, 1));
        set_hislen(old);
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    }

    #[test]
    fn histget_of_a_known_history_name_is_an_empty_string() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_histget(
            &[crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::String(Some(b"search".to_vec())),
                ..Default::default()
            }],
            &mut rettv,
        );
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(Vec::new()))
        );
    }

    #[test]
    fn histget_with_an_explicit_index_argument_is_also_an_empty_string() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_histget(
            &[
                crate::eval::typval_defs::TypvalT {
                    value: crate::eval::typval_defs::TypvalValue::String(Some(b"cmd".to_vec())),
                    ..Default::default()
                },
                crate::eval::typval_defs::TypvalT {
                    value: crate::eval::typval_defs::TypvalValue::Number(-2),
                    ..Default::default()
                },
            ],
            &mut rettv,
        );
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(Vec::new()))
        );
    }

    #[test]
    fn histget_of_a_type_error_history_name_is_a_null_string() {
        let _lock = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_histget(
            &[crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::List(std::ptr::null_mut()),
                ..Default::default()
            }],
            &mut rettv,
        );
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::String(None));
    }
}
