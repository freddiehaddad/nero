//! Translated from `src/nvim/optionstr.c` (tractable core only).
//!
//! `optionstr.c` implements string-option parsing/validation - the
//! ~150 real `did_set_*` per-option callbacks (each triggered only
//! through `option.c`'s own not-yet-translated `did_set_option`), plus
//! a handful of small, genuinely standalone helpers used elsewhere.
//!
//! Translated: [`check_illegal_path_names`] - a small, pure
//! byte-scanning predicate (does `val` contain any of a small,
//! fixed set of "illegal" path/directory characters, gated by
//! `GLOBALS.secure` and the option's own `NFNAME`/`NDNAME` flag bits) -
//! genuinely standalone even though its only real caller
//! (`option.c`'s `did_set_option`) is not yet translated, matching
//! this crate's established "small, simple, no design freedom"
//! ahead-of-caller precedent.
//!
//! Also translated: [`opt_strings_flags`] (a comma-separated-or-single
//! string-value validator/bitmask builder, e.g. for `'backupcopy'`/
//! `'signcolumn'`/`'virtualedit'`), [`check_ff_value`] (its first real
//! translated caller - is `p` a valid `'fileformat'` name), and
//! `charset.c`'s sibling `valid_filetype` (a thin wrapper over the
//! already-real `option::valid_name`).
//!
//! **Note on `opt_strings_flags`'s own doc comment**: the original
//! claims "Empty is always OK" - hand-traced and confirmed this is
//! only true when `list == true`. For `list == false` with an empty
//! `val`, the original still forces exactly one inner scan (via its
//! own `iter_one` local) against the empty string, which never
//! matches any REAL (non-empty) `values[]` entry via `strncmp` (an
//! empty `val`'s first byte is always NUL, differing from any
//! non-empty candidate's own first byte) - so it actually falls
//! through to the "not found" `FAIL` path, unless `values` itself
//! contains a literal empty-string entry (none of this crate's own
//! `OPT_*_VALUES` tables do). Preserved faithfully here, not "fixed"
//! to always succeed - see [`opt_strings_flags`]'s own doc comment and
//! its dedicated regression test.
//!
//! Deferred: everything else - the ~150 real `did_set_*`/`expand_*`
//! per-option callbacks (each needs a real `optset_T args` from an
//! actual `:set`/`set_option_value` call, per `option_defs.rs`'s own
//! `OPTIONS` doc comment), `copy_option_part`/`skip_to_option_part`
//! (already translated in `option.rs`, not here), and
//! `check_signcolumn`/other `opt_strings_flags` callers (each needs
//! its own additional `WinT` field wiring, deliberately not bundled
//! into this same pass).

use crate::option_defs::opt_flags;

/// Whether `val` contains an illegal character for an option flagged
/// `NFNAME`/`NDNAME` (`check_illegal_path_names`, `optionstr.c`) -
/// used to reject dangerous characters (e.g. a literal `;`/`&`/`|`
/// shell-command separator) in options like `'backupdir'`/
/// `'directory'` that build a real file/directory name. When
/// [`crate::globals::Globals::secure`] is set (running in a sandboxed
/// modeline/plugin context), the `NFNAME` character set additionally
/// includes `*`/`?`/`[`/`|`/`;`/`&` (wildcard/shell-metacharacters),
/// matching the original's own extra caution in that mode.
#[must_use]
pub fn check_illegal_path_names(val: &[u8], flags: u32) -> bool {
    // SAFETY: a plain `i32` copy-out read, no aliasing hazard.
    let secure = unsafe { crate::globals::GLOBALS.get_mut() }.secure != 0;

    let nfname_bad: &[u8] = if secure { b"/\\*?[|;&<>\r\n" } else { b"/\\*?[<>\r\n" };
    let ndname_bad: &[u8] = b"*?[|;&<>\r\n";

    (flags & opt_flags::NFNAME != 0 && val.iter().any(|b| nfname_bad.contains(b)))
        || (flags & opt_flags::NDNAME != 0 && val.iter().any(|b| ndname_bad.contains(b)))
}

/// Handle an option that can be a range of string values, setting a
/// flag for each string present (`opt_strings_flags`, a `static`
/// helper in the original).
///
/// `values` is the option's own fixed set of valid string forms
/// (e.g. `option_vars::OPT_FF_VALUES`); `list`, when `true`, accepts a
/// comma-separated LIST of values (e.g. `'virtualedit'`), rather than
/// just one.
///
/// Returns `Some(flags)` on success (`OK` in the original - one bit
/// set per matched `values[]` entry, by its own index - the original's
/// own `unsigned *flagp` out-parameter is collapsed into the return
/// value here, since every real call site either wants the resulting
/// flags or doesn't, never anything else), `None` on failure (`FAIL`).
///
/// See this module's own doc comment for a real, hand-traced
/// correction to the original's own "Empty is always OK" doc claim -
/// only true for `list == true`.
#[must_use]
pub fn opt_strings_flags(val: &[u8], values: &[&str], list: bool) -> Option<u32> {
    let mut new_flags: u32 = 0;
    // If not list and val is empty, then force one iteration of the
    // loop below (matching the original's own `iter_one` local).
    let iter_one = val.is_empty() && !list;
    let mut pos = 0usize;

    loop {
        if pos >= val.len() && !iter_one {
            break;
        }

        let remaining = &val[pos..];
        let mut matched = false;
        for (i, candidate) in values.iter().enumerate() {
            let cand_bytes = candidate.as_bytes();
            let len = cand_bytes.len();
            let matches_prefix = remaining.len() >= len && remaining[..len] == *cand_bytes;
            let followed_by_boundary = if matches_prefix {
                let next = remaining.get(len);
                (list && next == Some(&b',')) || next.is_none()
            } else {
                false
            };
            if matches_prefix && followed_by_boundary {
                let advance = len + usize::from(remaining.get(len) == Some(&b','));
                pos += advance;
                debug_assert!(i < 32, "opt_strings_flags: too many values for a u32 flag bitmask");
                new_flags |= 1u32 << i;
                matched = true;
                break;
            }
        }
        if !matched {
            return None;
        }
        if iter_one {
            break;
        }
    }

    Some(new_flags)
}

/// Whether `p` is a valid `'fileformat'` name (`check_ff_value`) -
/// [`opt_strings_flags`]'s first real translated caller.
#[must_use]
pub fn check_ff_value(p: &[u8]) -> bool {
    opt_strings_flags(p, crate::option_vars::OPT_FF_VALUES, false).is_some()
}

/// Whether `val` is a syntactically valid `'filetype'`/`'syntax'`
/// value (`valid_filetype`, a `static` helper in `optionstr.c`) - a
/// thin wrapper over the already-real `option::valid_name`.
#[must_use]
pub fn valid_filetype(val: &[u8]) -> bool {
    crate::option::valid_name(val, b".-_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_secure(value: i32) -> i32 {
        // SAFETY: caller holds `global_state_test_lock()` for the
        // whole duration this value matters.
        let cell = unsafe { crate::globals::GLOBALS.get_mut() };
        let old = cell.secure;
        cell.secure = value;
        old
    }

    #[test]
    fn plain_path_with_no_flags_set_is_never_illegal() {
        assert!(!check_illegal_path_names(b"foo/bar", 0));
    }

    #[test]
    fn nfname_flagged_option_rejects_a_semicolon_only_when_secure() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_secure(0);

        // Not secure: ';' is NOT in the (smaller) non-secure NFNAME set.
        assert!(!check_illegal_path_names(b"foo;bar", opt_flags::NFNAME));

        set_secure(1);
        // Secure: ';' IS in the secure-mode NFNAME set.
        assert!(check_illegal_path_names(b"foo;bar", opt_flags::NFNAME));

        set_secure(old);
    }

    #[test]
    fn nfname_flagged_option_rejects_backslash_and_wildcards_in_either_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_secure(0);

        assert!(check_illegal_path_names(b"foo\\bar", opt_flags::NFNAME));
        assert!(check_illegal_path_names(b"foo*bar", opt_flags::NFNAME));
        assert!(check_illegal_path_names(b"foo[bar", opt_flags::NFNAME));
        assert!(check_illegal_path_names(b"foo<bar", opt_flags::NFNAME));
        assert!(check_illegal_path_names(b"foo>bar", opt_flags::NFNAME));

        set_secure(old);
    }

    #[test]
    fn ndname_flagged_option_rejects_a_semicolon_regardless_of_secure() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_secure(0);

        // NDNAME's own bad-char set always includes the "secure" set of
        // characters, unconditionally (no `secure`-gated variant, unlike
        // NFNAME).
        assert!(check_illegal_path_names(b"foo;bar", opt_flags::NDNAME));

        set_secure(old);
    }

    #[test]
    fn neither_flag_set_never_rejects_even_a_bad_character() {
        assert!(!check_illegal_path_names(b"foo;bar<baz", 0));
    }

    #[test]
    fn both_flags_set_checks_both_character_sets() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_secure(0);

        let both = opt_flags::NFNAME | opt_flags::NDNAME;
        // ';' isn't in the non-secure NFNAME set, but IS in the NDNAME
        // set - so the combined check still rejects it.
        assert!(check_illegal_path_names(b"foo;bar", both));

        set_secure(old);
    }

    const FF_VALUES: &[&str] = &["unix", "dos", "mac"];

    #[test]
    fn opt_strings_flags_single_exact_match_sets_the_matching_bit() {
        assert_eq!(opt_strings_flags(b"unix", FF_VALUES, false), Some(0b001));
        assert_eq!(opt_strings_flags(b"dos", FF_VALUES, false), Some(0b010));
        assert_eq!(opt_strings_flags(b"mac", FF_VALUES, false), Some(0b100));
    }

    #[test]
    fn opt_strings_flags_unknown_value_fails() {
        assert_eq!(opt_strings_flags(b"bogus", FF_VALUES, false), None);
    }

    #[test]
    fn opt_strings_flags_list_true_accepts_comma_separated_values() {
        assert_eq!(opt_strings_flags(b"unix,dos", FF_VALUES, true), Some(0b011));
        assert_eq!(opt_strings_flags(b"unix,dos,mac", FF_VALUES, true), Some(0b111));
    }

    #[test]
    fn opt_strings_flags_list_true_fails_on_trailing_garbage_after_a_comma() {
        assert_eq!(opt_strings_flags(b"unix,bogus", FF_VALUES, true), None);
    }

    #[test]
    fn opt_strings_flags_list_false_rejects_a_comma_separated_value() {
        // Without `list`, a value must match the WHOLE string, not just
        // a comma-separated prefix.
        assert_eq!(opt_strings_flags(b"unix,dos", FF_VALUES, false), None);
    }

    #[test]
    fn opt_strings_flags_prefix_ambiguity_is_resolved_by_the_boundary_check() {
        // A shorter values[] entry that happens to be a PREFIX of a
        // longer one must not falsely match - the "followed by a
        // comma or end of string" check correctly skips "a" here and
        // finds "ab" instead.
        let values: &[&str] = &["a", "ab"];
        assert_eq!(opt_strings_flags(b"ab", values, false), Some(0b10));
    }

    #[test]
    fn opt_strings_flags_empty_val_with_list_true_is_ok_and_empty() {
        // Genuinely "empty is always OK" - but ONLY for list == true,
        // per this module's own doc comment.
        assert_eq!(opt_strings_flags(b"", FF_VALUES, true), Some(0));
    }

    #[test]
    fn opt_strings_flags_empty_val_with_list_false_fails() {
        // The real, hand-traced correction to the original's own
        // "Empty is always OK" doc comment: for list == false, an
        // empty val does NOT match any real (non-empty) values[]
        // entry, so this returns None (FAIL), not Some(0) (OK) - see
        // this module's own doc comment for the full derivation.
        assert_eq!(opt_strings_flags(b"", FF_VALUES, false), None);
    }

    #[test]
    fn check_ff_value_accepts_the_three_real_fileformat_names() {
        assert!(check_ff_value(b"unix"));
        assert!(check_ff_value(b"dos"));
        assert!(check_ff_value(b"mac"));
    }

    #[test]
    fn check_ff_value_rejects_an_unknown_name() {
        assert!(!check_ff_value(b"bogus"));
        assert!(!check_ff_value(b""));
    }

    #[test]
    fn valid_filetype_accepts_letters_digits_dot_dash_underscore() {
        assert!(valid_filetype(b"c"));
        assert!(valid_filetype(b"cpp"));
        assert!(valid_filetype(b"foo.bar-baz_2"));
    }

    #[test]
    fn valid_filetype_rejects_other_punctuation() {
        assert!(!valid_filetype(b"foo bar"));
        assert!(!valid_filetype(b"foo/bar"));
    }

    #[test]
    fn valid_filetype_empty_is_vacuously_valid() {
        // Matches valid_name's own real behavior: a `for` loop over
        // zero characters never finds a disallowed one, so an empty
        // value is vacuously valid - not a translation bug.
        assert!(valid_filetype(b""));
    }
}
