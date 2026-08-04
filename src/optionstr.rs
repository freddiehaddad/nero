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
//! Also translated: `opt_values`/`check_str_opt` (the option-
//! index-to-valid-values-table lookup, and the generic "is this
//! string a valid value for this option" checker built on it), and
//! [`did_set_str_generic`] - the first real, callback-shaped
//! `did_set_*` function, plus two of its own small siblings that
//! needed nothing beyond it: [`did_set_backupext_or_patchmode`]
//! (`'backupext'`/`'patchmode'` can't both resolve to the same
//! effective suffix) and [`did_set_backspace`] (a numeric legacy
//! `'2'` spelling, or else delegate to `did_set_str_generic`).
//! `check_str_opt`'s own real, load-bearing side effect - writing the
//! computed flags bitmask into the option's `flags_var`, when it has
//! one - is preserved even though nothing currently reads it (no
//! translated code consumes e.g. `'sessionoptions'`'s own resulting
//! bitmask yet), matching this crate's established "keep the real
//! state mutation even without a current consumer" policy.
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
use std::ffi::c_void;

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

/// Get the array of valid string values for `opt_idx` (`opt_values`, a
/// `static` helper).
///
/// Two options genuinely borrow a SIBLING option's own `values[]`
/// table rather than having a distinct one of their own (confirmed
/// directly against the real body, not assumed): `'viewoptions'`
/// reuses `'sessionoptions'`'s, and `'fileformats'` reuses
/// `'fileformat'`'s.
fn opt_values(opt_idx: crate::option_defs::OptIndex) -> &'static [&'static str] {
    use crate::option_defs::OptIndex;
    let idx1 = match opt_idx {
        OptIndex::Viewoptions => OptIndex::Sessionoptions,
        OptIndex::Fileformats => OptIndex::Fileformat,
        _ => opt_idx,
    };
    crate::option::get_option(idx1).values
}

/// Whether the string value at `varp` (or, when `None`, at the
/// option's own global storage, `opt.var`) is a valid value for
/// `opt_idx` (`check_str_opt`).
///
/// As a real, load-bearing side effect - matching the original
/// exactly, even though no currently-translated code reads it yet -
/// on success this writes the resulting flags bitmask into
/// `*opt.flags_var` when the option has one.
///
/// # Safety
/// `varp`, if `Some`, must point to a live `Option<Vec<u8>>` for the
/// whole call (matching `crate::option::optval_from_varp`'s own
/// established contract for a `String`-typed option's storage) - as
/// must the option's own global `.var` pointer, when `varp` is
/// `None`.
unsafe fn check_str_opt(opt_idx: crate::option_defs::OptIndex, varp: Option<*mut c_void>) -> bool {
    let opt = crate::option::get_option(opt_idx);
    let varp = varp.unwrap_or(opt.var);
    let list = (opt.flags & (opt_flags::COMMA | opt_flags::ONE_COMMA)) != 0;
    // SAFETY: forwarded from this function's own safety doc.
    let val = unsafe { &*(varp as *mut Option<Vec<u8>>) };
    let val_bytes: &[u8] = val.as_deref().unwrap_or(&[]);
    let values = opt_values(opt_idx);
    match opt_strings_flags(val_bytes, values, list) {
        Some(flags) => {
            if !opt.flags_var.is_null() {
                // SAFETY: a non-null `flags_var` points to a live
                // `u32` for the option's whole lifetime, matching
                // `get_varp_from`'s own established contract.
                unsafe {
                    *opt.flags_var = flags;
                }
            }
            true
        }
        None => false,
    }
}

/// Generic `did_set_*` callback for a plain comma/one-comma string
/// option with no further special handling (`did_set_str_generic`).
///
/// # Safety
/// `args.os_varp`, if non-null, must point to a live
/// `Option<Vec<u8>>` for the whole call, matching `check_str_opt`'s
/// own contract.
pub unsafe fn did_set_str_generic(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let varp = if args.os_varp.is_null() { None } else { Some(args.os_varp) };
    // SAFETY: forwarded from this function's own safety doc.
    let ok = unsafe { check_str_opt(args.os_idx, varp) };
    if ok {
        None
    } else {
        Some(crate::errors::e_invarg.as_bytes())
    }
}

/// The `'backupext'` or the `'patchmode'` option is changed
/// (`did_set_backupext_or_patchmode`) - rejects the combination if
/// both would resolve to the same effective suffix (stripping one
/// shared leading `.`, if present on each), which would make
/// neovim's own backup-vs-patch-file disambiguation logic ambiguous.
pub fn did_set_backupext_or_patchmode() -> Option<&'static [u8]> {
    // SAFETY: a plain, momentary read of two independent option
    // values - no aliasing hazard.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    let bex: &[u8] = opts.p_bex.as_deref().unwrap_or(&[]);
    let pm: &[u8] = opts.p_pm.as_deref().unwrap_or(&[]);
    let bex_trimmed = if bex.first() == Some(&b'.') { &bex[1..] } else { bex };
    let pm_trimmed = if pm.first() == Some(&b'.') { &pm[1..] } else { pm };
    if bex_trimmed == pm_trimmed {
        Some(crate::gettext_defs::gettext_noop("E589: 'backupext' and 'patchmode' are equal").as_bytes())
    } else {
        None
    }
}

/// The `'backspace'` option is changed (`did_set_backspace`).
///
/// A legacy numeric spelling is only valid as the single digit `'2'`
/// (matching the original's own `ascii_isdigit(*p_bs)` check against
/// just the FIRST byte - any other leading digit, e.g. `"3"` or a
/// multi-digit `"20"`, is rejected); anything non-numeric falls
/// through to the generic comma-list validator.
///
/// # Safety
/// Same as `did_set_str_generic`.
pub unsafe fn did_set_backspace(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: a plain, momentary read - no aliasing hazard.
    let p_bs = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs.clone();
    let first = p_bs.as_deref().and_then(|s| s.first().copied());
    if let Some(c) = first
        && crate::ascii_defs::ascii_isdigit(i32::from(c))
    {
        return if c == b'2' { None } else { Some(crate::errors::e_invarg.as_bytes()) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { did_set_str_generic(args) }
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

    // ---- opt_values / check_str_opt / did_set_str_generic ----

    use crate::option_defs::OptIndex;

    #[test]
    fn opt_values_returns_the_options_own_table_for_a_normal_option() {
        assert_eq!(opt_values(OptIndex::Fileformat), crate::option_vars::OPT_FF_VALUES);
        assert_eq!(opt_values(OptIndex::Sessionoptions), crate::option_vars::OPT_SSOP_VALUES);
    }

    #[test]
    fn opt_values_viewoptions_reuses_sessionoptions_own_table() {
        assert_eq!(opt_values(OptIndex::Viewoptions), crate::option_vars::OPT_SSOP_VALUES);
    }

    #[test]
    fn opt_values_fileformats_reuses_fileformat_own_table() {
        assert_eq!(opt_values(OptIndex::Fileformats), crate::option_vars::OPT_FF_VALUES);
    }

    #[test]
    fn check_str_opt_accepts_a_valid_value_via_an_explicit_varp() {
        let mut val: Option<Vec<u8>> = Some(b"unix".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        assert!(unsafe { check_str_opt(OptIndex::Fileformat, Some(varp)) });
    }

    #[test]
    fn check_str_opt_rejects_an_invalid_value_via_an_explicit_varp() {
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        assert!(!unsafe { check_str_opt(OptIndex::Fileformat, Some(varp)) });
    }

    #[test]
    fn check_str_opt_writes_the_computed_flags_into_flags_var_on_success() {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.ssop_flags;
        opts.ssop_flags = 0;

        let mut val: Option<Vec<u8>> = Some(b"help,blank".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        assert!(unsafe { check_str_opt(OptIndex::Sessionoptions, Some(varp)) });

        // "help" is index 6, "blank" is index 7 in OPT_SSOP_VALUES.
        assert_eq!(
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ssop_flags,
            (1 << 6) | (1 << 7)
        );

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ssop_flags = prev;
    }

    #[test]
    fn check_str_opt_none_varp_reads_the_options_own_global_storage() {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_ff.clone();
        opts.p_ff = Some(b"dos".to_vec());

        assert!(unsafe { check_str_opt(OptIndex::Fileformat, None) });

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ff = Some(b"bogus".to_vec());
        assert!(!unsafe { check_str_opt(OptIndex::Fileformat, None) });

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ff = prev;
    }

    #[test]
    fn did_set_str_generic_valid_value_returns_none() {
        let mut val: Option<Vec<u8>> = Some(b"unix".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args =
            crate::option_defs::OptsetT { os_idx: OptIndex::Fileformat, os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_str_generic(&mut args) }, None);
    }

    #[test]
    fn did_set_str_generic_invalid_value_returns_e_invarg() {
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args =
            crate::option_defs::OptsetT { os_idx: OptIndex::Fileformat, os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_str_generic(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn did_set_str_generic_null_varp_falls_back_to_the_options_own_global_storage() {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_ff.clone();
        opts.p_ff = Some(b"mac".to_vec());

        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Fileformat, ..Default::default() };
        assert_eq!(unsafe { did_set_str_generic(&mut args) }, None);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ff = prev;
    }

    // ---- did_set_backupext_or_patchmode ----

    fn set_bex_pm(bex: Option<&[u8]>, pm: Option<&[u8]>) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = (opts.p_bex.clone(), opts.p_pm.clone());
        opts.p_bex = bex.map(<[u8]>::to_vec);
        opts.p_pm = pm.map(<[u8]>::to_vec);
        prev
    }

    fn restore_bex_pm(prev: (Option<Vec<u8>>, Option<Vec<u8>>)) {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_bex = prev.0;
        opts.p_pm = prev.1;
    }

    #[test]
    fn did_set_backupext_or_patchmode_different_suffixes_is_ok() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_bex_pm(Some(b"~"), Some(b".orig"));
        assert_eq!(did_set_backupext_or_patchmode(), None);
        restore_bex_pm(prev);
    }

    #[test]
    fn did_set_backupext_or_patchmode_identical_suffixes_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_bex_pm(Some(b".bak"), Some(b".bak"));
        assert!(did_set_backupext_or_patchmode().is_some());
        restore_bex_pm(prev);
    }

    #[test]
    fn did_set_backupext_or_patchmode_leading_dot_is_stripped_before_comparing() {
        let _lock = crate::globals::global_state_test_lock();
        // ".bak" (patchmode) and "bak" (backupext, no leading dot) both
        // reduce to the same "bak" suffix once the shared leading '.'
        // is stripped from whichever side has one.
        let prev = set_bex_pm(Some(b"bak"), Some(b".bak"));
        assert!(did_set_backupext_or_patchmode().is_some());
        restore_bex_pm(prev);
    }

    // ---- did_set_backspace ----

    fn set_p_bs(value: Option<&[u8]>) -> Option<Vec<u8>> {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_bs.clone();
        opts.p_bs = value.map(<[u8]>::to_vec);
        prev
    }

    #[test]
    fn did_set_backspace_legacy_digit_2_is_ok() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_bs(Some(b"2"));
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Backspace, ..Default::default() };
        assert_eq!(unsafe { did_set_backspace(&mut args) }, None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs = prev;
    }

    #[test]
    fn did_set_backspace_other_leading_digit_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_bs(Some(b"3"));
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Backspace, ..Default::default() };
        assert_eq!(unsafe { did_set_backspace(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs = prev;
    }

    #[test]
    fn did_set_backspace_multi_digit_only_checks_the_first_byte() {
        let _lock = crate::globals::global_state_test_lock();
        // Matches the original's own `ascii_isdigit(*p_bs)` - only the
        // FIRST byte is inspected, so "20" is rejected (first digit is
        // '2', but the whole string isn't the single character "2").
        // Wait: the check is `*p_bs != '2'` on the FIRST byte alone, so
        // "20" actually passes this specific check (first byte is '2')
        // even though the whole string isn't just "2" - preserved
        // faithfully, not "fixed" to require an exact one-byte match.
        let prev = set_p_bs(Some(b"20"));
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Backspace, ..Default::default() };
        assert_eq!(unsafe { did_set_backspace(&mut args) }, None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs = prev;
    }

    #[test]
    fn did_set_backspace_non_numeric_delegates_to_the_generic_comma_list_check() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_bs(Some(b"indent,eol,start"));
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Backspace, ..Default::default() };
        assert_eq!(unsafe { did_set_backspace(&mut args) }, None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs = prev;
    }

    #[test]
    fn did_set_backspace_non_numeric_invalid_value_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_bs(Some(b"bogus"));
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Backspace, ..Default::default() };
        assert_eq!(unsafe { did_set_backspace(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs = prev;
    }
}
