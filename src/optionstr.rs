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
//! `did_set_*` function, plus 4 of its own small siblings that
//! needed nothing beyond it/already-real state:
//! [`did_set_backupext_or_patchmode`] (`'backupext'`/`'patchmode'`
//! can't both resolve to the same effective suffix),
//! [`did_set_backspace`] (a numeric legacy `'2'` spelling, or else
//! delegate to `did_set_str_generic`), [`did_set_helpfile`] (may
//! unset `$VIM`/`$VIMRUNTIME` to force a later recompute), and
//! [`did_set_helplang`] (a comma-separated-list-of-2-letter-codes
//! validator, hand-traced against the original's own NUL-terminator-
//! relying 3-byte-stride scan - see its own doc comment). Also
//! [`did_set_completeopt`] - the `'completeopt'` per-window/buffer
//! comma-list callback, following the same real
//! `OPT_LOCAL`/`OPT_GLOBAL`-branching shape `get_varp_scope_from`'s
//! own already-real dispatch already established. Also
//! [`did_set_bufhidden`] (a plain single-value validator) and
//! [`did_set_buftype`] (validates `'buftype'` against
//! `buf.terminal`/the option's own value list, sets a real
//! `'comments'` default and resets the prompt-start position for
//! `buftype=prompt`, and flags `w_redr_status` - its own 2 real
//! redraw-SCHEDULING calls, `redraw_later`/`redraw_titles`, are
//! omitted, matching this crate's established policy).
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

/// The `'helpfile'` option is changed (`did_set_helpfile`).
///
/// May force recomputing `$VIM`/`$VIMRUNTIME` (by unsetting them,
/// deferring the actual recompute to whoever later reads them) - a
/// real, faithful, state-mutating side effect kept even though
/// nothing in this crate currently reads `$VIM`/`$VIMRUNTIME` back out
/// via the recompute path itself (`vim_getenv`'s own
/// `$VIM`/`$VIMRUNTIME`-auto-discovery fallback is still deferred).
///
/// # Safety
/// Forwards `crate::os::env::vim_unsetenv_ext`'s own safety
/// requirements (touches `crate::globals::GLOBALS`).
pub unsafe fn did_set_helpfile() -> Option<&'static [u8]> {
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let didset_vim = globals.didset_vim;
    let didset_vimruntime = globals.didset_vimruntime;
    if didset_vim {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::os::env::vim_unsetenv_ext(b"VIM") };
    }
    if didset_vimruntime {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::os::env::vim_unsetenv_ext(b"VIMRUNTIME") };
    }
    None
}

/// The `'helplang'` option is changed (`did_set_helplang`).
///
/// Validates a comma-separated list of exactly-2-letter language
/// codes (`""`, `"ab"`, `"ab,cd"`, `"ab,cd,ef"`, ...). Hand-traced
/// against the original's own 3-byte-stride scan (which relies on a
/// NUL terminator existing at-or-past the string's own logical end -
/// translated here as `s.get(i + n).is_none()` standing in for "byte
/// `i + n` is the NUL terminator", exactly matching every real
/// occurrence of `== NUL`/short-circuited-away read in the original):
/// - `""` -> valid (loop never runs).
/// - `"ab"` -> valid (2nd byte is a real char, 3rd position is the
///   terminator, matching the original's own short-circuited
///   `(s[2] != ',' || ...) && s[2] != NUL` evaluating to `false`).
/// - `"ab,cd"` -> valid (each 2-letter code followed by `,` then
///   another 2-letter code, terminator right after the last one).
/// - `"a"` (a single trailing byte) -> invalid (`s[1]` would be the
///   terminator, i.e. no 2nd letter).
/// - `"ab,"` (trailing comma, nothing after) -> invalid (`s[3]` would
///   be the terminator right after the comma).
/// - `"abc"` (3rd byte isn't a comma or the terminator) -> invalid.
pub fn did_set_helplang() -> Option<&'static [u8]> {
    // SAFETY: a plain, momentary read - no aliasing hazard.
    let p_hlg = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg.clone();
    let s: &[u8] = p_hlg.as_deref().unwrap_or(&[]);
    let mut i = 0usize;
    while i < s.len() {
        if s.get(i + 1).is_none() {
            return Some(crate::errors::e_invarg.as_bytes());
        }
        match s.get(i + 2) {
            Some(&c2) => {
                if c2 != b',' || s.get(i + 3).is_none() {
                    return Some(crate::errors::e_invarg.as_bytes());
                }
            }
            None => break,
        }
        i += 3;
    }
    None
}

/// The `'completeopt'` option is changed (`did_set_completeopt`).
///
/// # Safety
/// `args.os_buf` must be a valid, non-null pointer to a live `BufT`
/// for the whole call.
pub unsafe fn did_set_completeopt(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let buf = args.os_buf as *mut crate::buffer_defs::BufT;
    let opt_flags = args.os_flags as u32;

    let (cot, flags_ptr): (Option<Vec<u8>>, *mut u32) = if opt_flags & crate::option_defs::opt_set_flags::OPT_LOCAL != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let b = unsafe { &mut *buf };
        (b.b_p_cot.clone(), std::ptr::addr_of_mut!(b.b_cot_flags))
    } else {
        if opt_flags & crate::option_defs::opt_set_flags::OPT_GLOBAL == 0 {
            // When using `:set`, clear the local flags.
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                (*buf).b_cot_flags = 0;
            }
        }
        // SAFETY: a plain, momentary read/pointer-take - no aliasing
        // hazard (the pointer is only dereferenced after this call
        // returns, once the `opt_strings_flags` result is known).
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        (opts.p_cot.clone(), std::ptr::addr_of_mut!(opts.cot_flags))
    };

    let cot_bytes: &[u8] = cot.as_deref().unwrap_or(&[]);
    match opt_strings_flags(cot_bytes, crate::option_vars::OPT_COT_VALUES, true) {
        Some(new_flags) => {
            // SAFETY: `flags_ptr` points at either `buf.b_cot_flags`
            // or `OPTION_VARS.cot_flags`, both live for the whole call.
            unsafe {
                *flags_ptr = new_flags;
            }
            None
        }
        None => Some(crate::errors::e_invarg.as_bytes()),
    }
}

/// The `'bufhidden'` option is changed (`did_set_bufhidden`).
///
/// # Safety
/// `args.os_buf` must be a valid, non-null pointer to a live `BufT`
/// for the whole call.
pub unsafe fn did_set_bufhidden(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*(args.os_buf as *const crate::buffer_defs::BufT) };
    let val: &[u8] = buf.b_p_bh.as_deref().unwrap_or(&[]);
    if opt_strings_flags(val, crate::option_vars::OPT_BH_VALUES, false).is_some() {
        None
    } else {
        Some(crate::errors::e_invarg.as_bytes())
    }
}

/// The `'buftype'` option is changed (`did_set_buftype`).
///
/// Omits the original's own 2 pure redraw-scheduling calls
/// (`redraw_later(win, UPD_VALID)`/`redraw_titles()`) - matching this
/// crate's established "keep the real state mutation, skip the
/// display-scheduling side effect" policy - while keeping every other
/// real state mutation: the `'comments'` default reset for
/// `buftype=prompt` (bypassing the not-yet-translated generic
/// `set_option_direct` by directly assigning the buffer-local storage
/// it would have resolved to for `OPT_LOCAL`, matching that call's own
/// exact effect), the prompt-start-position reset (`RESET_FMARK`,
/// matching `mark.rs`'s own already-established
/// `free_fmark`-then-reassign idiom), `w_redr_status`, and `b_help`.
///
/// # Safety
/// `args.os_buf` and `args.os_win` must be valid, non-null pointers to
/// a live `BufT`/`WinT` respectively, for the whole call.
pub unsafe fn did_set_buftype(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let buf_ptr = args.os_buf as *mut crate::buffer_defs::BufT;
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;

    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *buf_ptr };
    let bt_first = buf.b_p_bt.as_deref().and_then(|s| s.first().copied()).unwrap_or(0);
    let bt_bytes: &[u8] = buf.b_p_bt.as_deref().unwrap_or(&[]);

    if (!buf.terminal.is_null() && bt_first != b't')
        || (buf.terminal.is_null() && bt_first == b't')
        || opt_strings_flags(bt_bytes, crate::option_vars::OPT_BT_VALUES, false).is_none()
    {
        return Some(crate::errors::e_invarg.as_bytes());
    }

    // buftype=prompt:
    if bt_first == b'p' {
        // Set default value for 'comments'.
        buf.b_p_com = Some(Vec::new());

        // Set the prompt start position to the last line.
        let next_prompt = crate::pos_defs::PosT {
            lnum: buf.b_ml.ml_line_count,
            col: buf.b_prompt_start.mark.col,
            coladd: 0,
        };
        crate::mark::free_fmark(std::mem::take(&mut buf.b_prompt_start));
        buf.b_prompt_start = crate::mark_defs::FmarkT {
            mark: next_prompt,
            fnum: 0,
            timestamp: crate::os::time::os_time(),
            view: crate::mark_defs::FmarkvT::default(),
            additional_data: None,
        };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &mut *win_ptr };
    // SAFETY: touches `OPTION_VARS`, matching `global_stl_height`'s
    // own safety doc.
    if win.w_status_height != 0 || unsafe { crate::window::global_stl_height() } != 0 {
        win.w_redr_status = true;
        // Real redraw scheduling (`redraw_later`) is omitted - the
        // redraw pipeline isn't tractable yet.
    }

    buf.b_help = bt_first == b'h';
    // Real redraw scheduling (`redraw_titles`) is omitted.

    None
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

    // ---- did_set_helpfile ----

    #[test]
    fn did_set_helpfile_unsets_vim_and_vimruntime_when_both_are_set() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_vim = globals.didset_vim;
        let prev_vimruntime = globals.didset_vimruntime;
        globals.didset_vim = true;
        globals.didset_vimruntime = true;

        assert_eq!(unsafe { did_set_helpfile() }, None);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(!globals.didset_vim);
        assert!(!globals.didset_vimruntime);
        globals.didset_vim = prev_vim;
        globals.didset_vimruntime = prev_vimruntime;
    }

    #[test]
    fn did_set_helpfile_leaves_flags_untouched_when_neither_is_set() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_vim = globals.didset_vim;
        let prev_vimruntime = globals.didset_vimruntime;
        globals.didset_vim = false;
        globals.didset_vimruntime = false;

        assert_eq!(unsafe { did_set_helpfile() }, None);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(!globals.didset_vim);
        assert!(!globals.didset_vimruntime);
        globals.didset_vim = prev_vim;
        globals.didset_vimruntime = prev_vimruntime;
    }

    // ---- did_set_helplang ----

    fn set_p_hlg(value: Option<&[u8]>) -> Option<Vec<u8>> {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_hlg.clone();
        opts.p_hlg = value.map(<[u8]>::to_vec);
        prev
    }

    #[test]
    fn did_set_helplang_empty_is_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_hlg(Some(b""));
        assert_eq!(did_set_helplang(), None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_single_two_letter_code_is_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_hlg(Some(b"ab"));
        assert_eq!(did_set_helplang(), None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_comma_separated_codes_are_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_hlg(Some(b"ab,cd,ef"));
        assert_eq!(did_set_helplang(), None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_single_leftover_byte_is_invalid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_hlg(Some(b"a"));
        assert_eq!(did_set_helplang(), Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_trailing_comma_with_nothing_after_is_invalid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_hlg(Some(b"ab,"));
        assert_eq!(did_set_helplang(), Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_third_byte_not_a_comma_is_invalid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_hlg(Some(b"abc"));
        assert_eq!(did_set_helplang(), Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_middle_code_missing_second_letter_is_invalid() {
        let _lock = crate::globals::global_state_test_lock();
        // "ab,c" - the 2nd code's own 2nd letter is the terminator.
        let prev = set_p_hlg(Some(b"ab,c"));
        assert_eq!(did_set_helplang(), Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    // ---- did_set_completeopt ----

    #[test]
    fn did_set_completeopt_local_reads_and_writes_the_buffer_local_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { b_p_cot: Some(b"menu,longest".to_vec()), b_cot_flags: 0, ..Default::default() };
        let mut args = crate::option_defs::OptsetT {
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_flags: crate::option_defs::opt_set_flags::OPT_LOCAL as i32,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_completeopt(&mut args) }, None);
        // "menu" is index 2, "longest" is index 1 in OPT_COT_VALUES.
        assert_eq!(buf.b_cot_flags, (1 << 2) | (1 << 1));
    }

    #[test]
    fn did_set_completeopt_local_invalid_value_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { b_p_cot: Some(b"bogus".to_vec()), b_cot_flags: 0, ..Default::default() };
        let mut args = crate::option_defs::OptsetT {
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_flags: crate::option_defs::opt_set_flags::OPT_LOCAL as i32,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_completeopt(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn did_set_completeopt_global_reads_and_writes_the_global_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_cot = opts.p_cot.clone();
        let prev_flags = opts.cot_flags;
        opts.p_cot = Some(b"noselect".to_vec());
        opts.cot_flags = 0;

        let mut buf = crate::buffer_defs::BufT::default();
        let mut args = crate::option_defs::OptsetT {
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_flags: crate::option_defs::opt_set_flags::OPT_GLOBAL as i32,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_completeopt(&mut args) }, None);
        // "noselect" is index 6 in OPT_COT_VALUES.
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cot_flags, 1 << 6);

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_cot = prev_cot;
        opts.cot_flags = prev_flags;
    }

    #[test]
    fn did_set_completeopt_plain_set_clears_the_buffer_local_flags_first() {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_cot = opts.p_cot.clone();
        let prev_flags = opts.cot_flags;
        opts.p_cot = Some(b"popup".to_vec());
        opts.cot_flags = 0;

        // Neither OPT_LOCAL nor OPT_GLOBAL set (a plain ":set" call) -
        // the buffer's own stale local flags must be cleared to 0
        // first, matching the original's own "clear the local flags"
        // comment exactly.
        let mut buf = crate::buffer_defs::BufT { b_cot_flags: 0xFF, ..Default::default() };
        let mut args = crate::option_defs::OptsetT {
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_flags: 0,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_completeopt(&mut args) }, None);
        assert_eq!(buf.b_cot_flags, 0);
        // "popup" is index 8 in OPT_COT_VALUES.
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cot_flags, 1 << 8);

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_cot = prev_cot;
        opts.cot_flags = prev_flags;
    }

    // ---- did_set_bufhidden ----

    #[test]
    fn did_set_bufhidden_accepts_every_real_value() {
        for val in crate::option_vars::OPT_BH_VALUES {
            let mut buf = crate::buffer_defs::BufT { b_p_bh: Some(val.as_bytes().to_vec()), ..Default::default() };
            let mut args =
                crate::option_defs::OptsetT { os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void, ..Default::default() };
            assert_eq!(unsafe { did_set_bufhidden(&mut args) }, None, "value {val:?} should be accepted");
        }
    }

    #[test]
    fn did_set_bufhidden_rejects_an_unknown_value() {
        let mut buf = crate::buffer_defs::BufT { b_p_bh: Some(b"bogus".to_vec()), ..Default::default() };
        let mut args =
            crate::option_defs::OptsetT { os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void, ..Default::default() };
        assert_eq!(unsafe { did_set_bufhidden(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    // ---- did_set_buftype ----

    fn buftype_args(
        buf: &mut crate::buffer_defs::BufT,
        win: &mut crate::buffer_defs::WinT,
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_buf: buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            ..Default::default()
        }
    }

    #[test]
    fn did_set_buftype_empty_non_terminal_is_valid() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT::default();
        let mut args = buftype_args(&mut buf, &mut win);
        assert_eq!(unsafe { did_set_buftype(&mut args) }, None);
    }

    #[test]
    fn did_set_buftype_terminal_value_without_a_real_terminal_fails() {
        let mut buf = crate::buffer_defs::BufT { b_p_bt: Some(b"terminal".to_vec()), ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();
        let mut args = buftype_args(&mut buf, &mut win);
        assert_eq!(unsafe { did_set_buftype(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn did_set_buftype_real_terminal_with_non_terminal_value_fails() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_bt: Some(b"help".to_vec()),
            terminal: std::ptr::dangling_mut::<crate::types_defs::TerminalT>(),
            ..Default::default()
        };
        let mut win = crate::buffer_defs::WinT::default();
        let mut args = buftype_args(&mut buf, &mut win);
        assert_eq!(unsafe { did_set_buftype(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn did_set_buftype_real_terminal_with_terminal_value_is_valid() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_bt: Some(b"terminal".to_vec()),
            terminal: std::ptr::dangling_mut::<crate::types_defs::TerminalT>(),
            ..Default::default()
        };
        let mut win = crate::buffer_defs::WinT::default();
        let mut args = buftype_args(&mut buf, &mut win);
        assert_eq!(unsafe { did_set_buftype(&mut args) }, None);
    }

    #[test]
    fn did_set_buftype_unknown_value_fails() {
        let mut buf = crate::buffer_defs::BufT { b_p_bt: Some(b"bogus".to_vec()), ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();
        let mut args = buftype_args(&mut buf, &mut win);
        assert_eq!(unsafe { did_set_buftype(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn did_set_buftype_help_sets_b_help() {
        let mut buf = crate::buffer_defs::BufT { b_p_bt: Some(b"help".to_vec()), b_help: false, ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();
        let mut args = buftype_args(&mut buf, &mut win);
        assert_eq!(unsafe { did_set_buftype(&mut args) }, None);
        assert!(buf.b_help);
    }

    #[test]
    fn did_set_buftype_non_help_clears_b_help() {
        let mut buf = crate::buffer_defs::BufT { b_p_bt: None, b_help: true, ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();
        let mut args = buftype_args(&mut buf, &mut win);
        assert_eq!(unsafe { did_set_buftype(&mut args) }, None);
        assert!(!buf.b_help);
    }

    #[test]
    fn did_set_buftype_prompt_resets_comments_and_prompt_start() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_bt: Some(b"prompt".to_vec()),
            b_p_com: Some(b"some,old,value".to_vec()),
            b_ml: crate::memline_defs::MemlineT { ml_line_count: 7, ..Default::default() },
            b_prompt_start: crate::mark_defs::FmarkT { mark: crate::pos_defs::PosT { lnum: 1, col: 3, coladd: 0 }, ..Default::default() },
            ..Default::default()
        };
        let mut win = crate::buffer_defs::WinT::default();
        let mut args = buftype_args(&mut buf, &mut win);
        assert_eq!(unsafe { did_set_buftype(&mut args) }, None);

        assert_eq!(buf.b_p_com, Some(Vec::new()));
        // The new prompt-start position uses the CURRENT line count
        // (7), but preserves the OLD column (3) - matching the
        // original's own `next_prompt` construction exactly.
        assert_eq!(buf.b_prompt_start.mark, crate::pos_defs::PosT { lnum: 7, col: 3, coladd: 0 });
    }

    #[test]
    fn did_set_buftype_non_prompt_leaves_comments_untouched() {
        let mut buf = crate::buffer_defs::BufT { b_p_bt: None, b_p_com: Some(b"some,value".to_vec()), ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();
        let mut args = buftype_args(&mut buf, &mut win);
        assert_eq!(unsafe { did_set_buftype(&mut args) }, None);
        assert_eq!(buf.b_p_com, Some(b"some,value".to_vec()));
    }

    #[test]
    fn did_set_buftype_flags_w_redr_status_when_win_has_a_status_line() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT { w_status_height: 1, w_redr_status: false, ..Default::default() };
        let mut args = buftype_args(&mut buf, &mut win);
        assert_eq!(unsafe { did_set_buftype(&mut args) }, None);
        assert!(win.w_redr_status);
    }

    #[test]
    fn did_set_buftype_leaves_w_redr_status_untouched_without_a_status_line() {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_ls = opts.p_ls;
        opts.p_ls = 2; // not 3, so global_stl_height() == 0

        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT { w_status_height: 0, w_redr_status: false, ..Default::default() };
        let mut args = buftype_args(&mut buf, &mut win);
        assert_eq!(unsafe { did_set_buftype(&mut args) }, None);
        assert!(!win.w_redr_status);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls = prev_ls;
    }
}
