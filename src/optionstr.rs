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
//! omitted, matching this crate's established policy). Also
//! [`did_set_lispoptions`] (a fixed-shape string validator) and
//! [`did_set_matchpairs`] (a comma-separated `X:Y` character-pair
//! validator, hand-traced against the original's own for-loop -
//! whose OWN increment clause consumes the comma between pairs, on
//! top of the manual per-character advancement the loop body already
//! does - see its own doc comment). Also [`did_set_selection`]
//! (delegates entirely to `did_set_str_generic`, its own pure
//! redraw-scheduling call omitted) and [`did_set_sessionoptions`]
//! (rejects `"sesdir"`+`"curdir"` together, restoring the OLD
//! `ssop_flags` on that specific failure). Also [`did_set_keymodel`]
//! (sets `GLOBALS.km_stopsel`/`km_startsel` from `'keymodel'`'s own
//! character content), [`did_set_showcmdloc`] (delegates then calls
//! the already-real `comp_col`), [`did_set_splitkeep`] (snapshots
//! every window's own height across every tabpage into
//! `w_prev_height`, using `tabpage_win_valid`'s own already-
//! established `curtab`-vs-`tp_firstwin` window-list-walk
//! convention), [`did_set_spellsuggest`] (re-scanned once
//! `spellsuggest.rs`'s own `spell_check_sps` landed in an earlier
//! commit this segment - its only remaining real blocker),
//! [`did_set_mkspellmem`] (same shape, now that `spellfile.rs`'s own
//! new `spell_check_msm` exists), [`did_set_mouse`] (built on a
//! new `did_set_option_listflag` helper - its own dynamically-
//! formatted `"E539: Illegal character <c>"` message is simplified
//! to a static `e_invarg`, matching this whole module's established
//! "display text differs, boolean outcome identical" policy),
//! [`did_set_mousescroll`] (parses a comma-separated
//! `"ver:N"`/`"hor:M"` list into `OPTION_VARS.p_mousescroll_vert`/
//! `p_mousescroll_hor`), [`did_set_showbreak`] (every character
//! must occupy exactly 1 screen cell, via the already-real
//! `ptr2cells`/`utfc_ptr2len`), and [`did_set_wildmode`] (built on a
//! new `ex_getln.rs::check_opt_wim`).
//! `check_str_opt`'s own real, load-bearing side effect - writing the
//! computed flags bitmask into the option's `flags_var`, when it has
//! one - is preserved even though nothing currently reads it (no
//! translated code consumes e.g. `'sessionoptions'`'s own resulting
//! bitmask yet), matching this crate's established "keep the real
//! state mutation even without a current consumer" policy.
//!
//! Also [`check_stl_option`] (`'statusline'`/`'winbar'`/`'tabline'`/
//! `'rulerformat'`/`'statuscolumn'` format-string validation) -
//! genuinely self-contained (only needs `STL_ALL`, a fixed
//! character set derived from `statusline_defs.rs`'s own
//! `stl_flag` module, plus `ascii_isdigit`), even though its own real
//! caller (`did_set_statustabline_rulerformat`, needing
//! `win_config_float`/`get_option_default`/`comp_col`) is not yet
//! translated - matching this crate's established "small, simple,
//! no design freedom" ahead-of-caller precedent. Every dynamically-
//! formatted `illegal_char` message is simplified to a static
//! `e_invarg`, matching this module's own established policy.
//!
//! Also [`did_set_iconstring`]/[`did_set_titlestring`] (both real
//! `did_set_*` callbacks now, built on a new private
//! `did_set_titleiconstring` shared helper) - tractable once
//! `GLOBALS.stl_syntax`/`check_stl_option` existed, plus
//! `option.rs`'s own new `did_set_title` (a provable, always-taken
//! no-op today: its real `maketitle()` call is gated behind
//! `starting != NO_SCREEN`, and `starting` is only ever assigned by
//! `main.c`'s not-yet-translated startup sequence).
//!
//! Also [`did_set_varsofttabstop`]/[`did_set_vartabstop`] - both
//! built directly on `indent.rs`'s own already-real `tabstop_set`
//! (translated ahead of its real caller in an earlier pass). Rust's
//! own assignment automatically frees the previous
//! `b_p_vsts_array`/`b_p_vts_array`, matching the original's manual
//! `xfree(oldarray)`. `did_set_vartabstop`'s own extra
//! `foldmethodIsIndent`-gated `foldUpdateAll` call panics - a
//! genuinely REACHABLE gap (`'foldmethod'` is an ordinary string
//! option that can legitimately be `"indent"` in a real session,
//! unlike e.g. `AUTOCMDS`'s always-empty-registry precedent) -
//! `fold.rs`'s own real fold-tree search/update machinery isn't
//! translated yet.
//!
//! Also [`did_set_whichwrap`] - a thin `did_set_option_listflag`
//! wrapper over `option_vars::WW_ALL` (plus a trailing `,`, since
//! `'whichwrap'` is itself a comma-separated flag list - the original
//! appends the separator via adjacent string-literal concatenation
//! for this one call).
//!
//! Also [`did_set_virtualedit`] - resolves `ve`/`flags` from either
//! `win.w_onebuf_opt.wo_ve`/`wo_ve_flags` (`OPT_LOCAL`) or
//! `OPTION_VARS.p_ve`/`ve_flags` (otherwise) as an owned copy (Rust
//! can't alias 2 different `&mut` targets behind one binding the way
//! the original's own pointer-aliasing trick does), then writes
//! `opt_strings_flags`'s own already-real, brand-new-flags-value
//! return back to whichever target was selected. On a genuine value
//! change, calls the already-real `move::validate_virtcol`/
//! `cursor::coladvance` to recompute the cursor position.
//!
//! Also [`did_set_tagcase`] - the exact same "resolve from local or
//! global storage as an owned copy" pattern as
//! [`did_set_virtualedit`], but simpler (no cursor-position recompute
//! step, and `opt_strings_flags`'s own `list` parameter is `false` -
//! a single value, not a comma-separated list).
//!
//! Also [`did_set_concealcursor`] (a thin `did_set_option_listflag`
//! wrapper over `option_vars::COCU_ALL` - unlike `'whichwrap'`,
//! `'concealcursor'` is NOT a comma-separated list, so no separator
//! character is appended) and `did_set_completeslash` (Windows-only,
//! matching the original's own `#ifdef BACKSLASH_IN_FILENAME` guard
//! via `#[cfg(windows)]`; validates BOTH the global and buffer-local
//! value regardless of which was actually being set, faithfully
//! matching the original's own two-call `||` condition).
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

/// The `'lispoptions'` option is changed (`did_set_lispoptions`).
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the whole
/// call.
pub unsafe fn did_set_lispoptions(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);
    if val.is_empty() || val == b"expr:0" || val == b"expr:1" {
        None
    } else {
        Some(crate::errors::e_invarg.as_bytes())
    }
}

/// The `'matchpairs'` option is changed (`did_set_matchpairs`).
///
/// Validates a comma-separated list of `X:Y` character pairs (e.g.
/// `"(:)"`), where `X`/`Y` may each be a multi-byte (composing-aware)
/// character. Hand-traced against the original's own for-loop, whose
/// OWN increment clause (`p++`, running after every non-`break`/
/// `return` iteration) consumes the comma separator between pairs, on
/// top of the manual advancement the loop body itself already does
/// for `X`/the literal `:`/`Y` - traced against `"(:),{:}"`
/// (2 real pairs) before writing any test.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the whole
/// call. Touches `OPTION_VARS` (via `utfc_ptr2len`).
pub unsafe fn did_set_matchpairs(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);
    let mut i = 0usize;
    while i < val.len() {
        // Advance past the first character ("X").
        // SAFETY: forwarded from this function's own safety doc.
        i += unsafe { crate::mbyte::utfc_ptr2len(&val[i..]) } as usize;

        let mut x2: i32 = -1;
        if let Some(&b) = val.get(i) {
            x2 = i32::from(b);
            i += 1;
        }

        let mut x3: i32 = -1;
        if i < val.len() {
            x3 = crate::mbyte::utf_ptr2char(&val[i..]);
            // SAFETY: forwarded from this function's own safety doc.
            i += unsafe { crate::mbyte::utfc_ptr2len(&val[i..]) } as usize;
        }

        let next = val.get(i).copied();
        if x2 != i32::from(b':') || x3 == -1 || (next.is_some() && next != Some(b',')) {
            return Some(crate::errors::e_invarg.as_bytes());
        }
        if next.is_none() {
            break;
        }
        // The original for-loop's own increment - consumes the comma.
        i += 1;
    }
    None
}

/// The `'selection'` option is changed (`did_set_selection`).
///
/// Omits the original's own pure redraw-scheduling call
/// (`redraw_curbuf_later`, reached when `GLOBALS.Visual.active`) -
/// matching this crate's established policy - while keeping the
/// underlying [`did_set_str_generic`] check.
///
/// # Safety
/// Same as [`did_set_str_generic`].
pub unsafe fn did_set_selection(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { did_set_str_generic(args) }
}

/// The `'sessionoptions'` option is changed (`did_set_sessionoptions`).
///
/// After the generic comma-list check, rejects the combination of
/// both `"sesdir"` and `"curdir"` - restoring `ssop_flags` back to
/// whatever the OLD value implies (matching the original's own
/// re-parse-the-old-value call exactly, since `did_set_str_generic`'s
/// own `check_str_opt` has already written the NEW, rejected flags
/// into `ssop_flags` by this point).
///
/// # Safety
/// Same as [`did_set_str_generic`].
pub unsafe fn did_set_sessionoptions(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let errmsg = unsafe { did_set_str_generic(args) };
    if errmsg.is_some() {
        return errmsg;
    }
    // SAFETY: a plain, momentary read - no aliasing hazard.
    let ssop_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ssop_flags;
    if (ssop_flags & crate::option_vars::opt_ssop_flag::CURDIR != 0)
        && (ssop_flags & crate::option_vars::opt_ssop_flag::SESDIR != 0)
    {
        if let crate::option_defs::OptVal::String(ref old) = args.os_oldval
            && let Some(restored_flags) = opt_strings_flags(old, crate::option_vars::OPT_SSOP_VALUES, true)
        {
            // SAFETY: a plain, momentary write - no aliasing hazard.
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ssop_flags = restored_flags;
        }
        return Some(crate::errors::e_invarg.as_bytes());
    }
    None
}

/// The `'keymodel'` option is changed (`did_set_keymodel`).
///
/// # Safety
/// Same as [`did_set_str_generic`]. Also touches `GLOBALS`
/// (`km_stopsel`/`km_startsel`).
pub unsafe fn did_set_keymodel(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let errmsg = unsafe { did_set_str_generic(args) };
    if errmsg.is_some() {
        return errmsg;
    }
    // SAFETY: a plain, momentary read - no aliasing hazard.
    let p_km = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_km.clone();
    let val: &[u8] = p_km.as_deref().unwrap_or(&[]);
    let stopsel = crate::strings::vim_strchr(val, i32::from(b'o')).is_some();
    let startsel = crate::strings::vim_strchr(val, i32::from(b'a')).is_some();
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    globals.km_stopsel = stopsel;
    globals.km_startsel = startsel;
    None
}

/// The `'showcmdloc'` option is changed (`did_set_showcmdloc`).
///
/// # Safety
/// Same as [`did_set_str_generic`]. Also touches `GLOBALS`/
/// `OPTION_VARS` (via `comp_col`).
pub unsafe fn did_set_showcmdloc(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let errmsg = unsafe { did_set_str_generic(args) };
    if errmsg.is_none() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::drawscreen::comp_col() };
    }
    errmsg
}

/// The `'splitkeep'` option is changed (`did_set_splitkeep`).
///
/// Snapshots every window's own current height into `w_prev_height`,
/// across every tabpage - matching the original's own
/// `FOR_ALL_TAB_WINDOWS` walk (the current tab's own windows are
/// reached via `GLOBALS.firstwin`, matching `tabpage_win_valid`'s own
/// already-established convention for this exact distinction).
///
/// # Safety
/// Same as [`did_set_str_generic`]. Also touches `GLOBALS`'s
/// `first_tabpage`/`firstwin` window-list pointers, which must all be
/// valid, live pointers for the whole call.
pub unsafe fn did_set_splitkeep(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let is_curtab = std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab);
        let mut wp = if is_curtab {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &*tp }.tp_firstwin
        };
        while !wp.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let win = unsafe { &mut *wp };
            win.w_prev_height = win.w_height;
            wp = win.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { did_set_str_generic(args) }
}

/// The `'spellsuggest'` option is changed (`did_set_spellsuggest`).
///
/// # Safety
/// Touches `OPTION_VARS`, matching `spell_check_sps`'s own safety doc.
pub unsafe fn did_set_spellsuggest() -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::spellsuggest::spell_check_sps() } == crate::vim_defs::OK {
        None
    } else {
        Some(crate::errors::e_invarg.as_bytes())
    }
}

/// The `'mkspellmem'` option is changed (`did_set_mkspellmem`).
///
/// # Safety
/// Touches `OPTION_VARS`, matching `spell_check_msm`'s own safety doc.
pub unsafe fn did_set_mkspellmem() -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::spellfile::spell_check_msm() } == crate::vim_defs::OK {
        None
    } else {
        Some(crate::errors::e_invarg.as_bytes())
    }
}

/// An option which is a list of flags is set. Valid values are in
/// `flags` (`did_set_option_listflag`, a `static` helper).
///
/// The original's own dynamically-formatted `"E539: Illegal character
/// <%s>"` message (built via `illegal_char`, needing a shared
/// scratch `errbuf`) is simplified to the same static `e_invarg`
/// message this whole module already uses for every other validation
/// failure - the DISPLAYED text differs from the original, but the
/// boolean valid/invalid outcome (the only thing any translated
/// caller can observe) is identical.
fn did_set_option_listflag(val: &[u8], flags: &[u8]) -> Option<&'static [u8]> {
    for &c in val {
        if crate::strings::vim_strchr(flags, i32::from(c)).is_none() {
            return Some(crate::errors::e_invarg.as_bytes());
        }
    }
    None
}

/// The `'mouse'` option is changed (`did_set_mouse`).
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
pub unsafe fn did_set_mouse(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);
    did_set_option_listflag(val, crate::option_vars::MOUSE_ALL.as_bytes())
}

/// The `'whichwrap'` option is changed (`did_set_whichwrap`).
///
/// `'whichwrap'` is itself a comma-separated flag list, so the
/// original appends a `,` to `WW_ALL` (adjacent string-literal
/// concatenation, `WW_ALL ","`) for this one call, making the comma
/// separator itself pass as a valid character.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
pub unsafe fn did_set_whichwrap(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);

    let mut flags = crate::option_vars::WW_ALL.as_bytes().to_vec();
    flags.push(b',');
    did_set_option_listflag(val, &flags)
}

/// The `'mousescroll'` option is changed (`did_set_mousescroll`).
///
/// Parses a comma-separated `"ver:N"`/`"hor:M"` list (each direction
/// at most once), applying the real default for whichever direction
/// wasn't given.
///
/// # Safety
/// Touches `OPTION_VARS`.
pub unsafe fn did_set_mousescroll() -> Option<&'static [u8]> {
    use crate::option_vars::{MOUSESCROLL_HOR_DFLT, MOUSESCROLL_VERT_DFLT};

    // SAFETY: forwarded from this function's own safety doc.
    let p_ms = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll.clone();
    let string: &[u8] = p_ms.as_deref().unwrap_or(&[]);

    let mut vertical: crate::types_defs::OptInt = -1;
    let mut horizontal: crate::types_defs::OptInt = -1;
    let mut pos = 0usize;

    loop {
        let remaining = &string[pos..];
        let end = crate::strings::vim_strchr(remaining, i32::from(b','));
        let length = end.unwrap_or(remaining.len());

        // Both "ver:" and "hor:" are 4 bytes long, followed by at
        // least one digit.
        if length <= 4 {
            return Some(crate::errors::e_invarg.as_bytes());
        }

        let is_vert = &remaining[..4] == b"ver:";
        let is_hor = &remaining[..4] == b"hor:";
        if !is_vert && !is_hor {
            return Some(crate::errors::e_invarg.as_bytes());
        }
        let target = if is_vert { &mut vertical } else { &mut horizontal };
        if *target != -1 {
            // Direction already set - this is a duplicate.
            return Some(crate::errors::e_invarg.as_bytes());
        }

        // Verify that only digits follow the colon.
        for &b in &remaining[4..length] {
            if !crate::ascii_defs::ascii_isdigit(i32::from(b)) {
                return Some(crate::gettext_defs::gettext_noop("E5080: Digit expected").as_bytes());
            }
        }

        let (value, _consumed) = crate::charset::getdigits_int(&remaining[4..], false, -1);
        *target = i64::from(value);
        // Num options are generally kept within the signed int range.
        // We know this number won't be negative because we've already
        // checked for a minus sign. We'll allow 0 as a means of
        // disabling mouse scrolling.
        if *target == -1 {
            return Some(crate::errors::e_invarg.as_bytes());
        }

        match end {
            None => break,
            Some(comma_pos) => pos += comma_pos + 1,
        }
    }

    // If a direction wasn't set, fall back to the default value.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    opts.p_mousescroll_vert = if vertical == -1 { MOUSESCROLL_VERT_DFLT } else { vertical };
    opts.p_mousescroll_hor = if horizontal == -1 { MOUSESCROLL_HOR_DFLT } else { horizontal };

    None
}

/// The `'showbreak'` option is changed (`did_set_showbreak`).
///
/// Every character in the value must occupy exactly 1 screen cell -
/// no unprintable or double-wide characters allowed.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call. Touches `OPTION_VARS` (via `ptr2cells`/`utfc_ptr2len`).
pub unsafe fn did_set_showbreak(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);
    let mut pos = 0usize;
    while pos < val.len() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::charset::ptr2cells(&val[pos..]) } != 1 {
            return Some(
                crate::gettext_defs::gettext_noop(
                    "E595: 'showbreak' contains unprintable or wide character",
                )
                .as_bytes(),
            );
        }
        // SAFETY: forwarded from this function's own safety doc.
        pos += unsafe { crate::mbyte::utfc_ptr2len(&val[pos..]) }.max(1) as usize;
    }
    None
}

/// The `'wildmode'` option is changed (`did_set_wildmode`).
///
/// # Safety
/// Touches `OPTION_VARS`/`GLOBALS`, matching `check_opt_wim`'s own
/// safety doc.
pub unsafe fn did_set_wildmode() -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::ex_getln::check_opt_wim() } == crate::vim_defs::OK {
        None
    } else {
        Some(crate::errors::e_invarg.as_bytes())
    }
}

/// All `'statusline'`/`'winbar'`/`'tabline'`/`'rulerformat'`/
/// `'statuscolumn'` item-flag characters (`STL_ALL`, `option_vars.h`).
///
/// Faithfully preserves a real, harmless upstream quirk: `TABPAGENR`/
/// `TABCLOSENR`/`CLICK_FUNC` are listed TWICE in the original's own
/// array literal - since this is only ever used for membership
/// testing (`vim_strchr`/`.contains()`), the duplication changes
/// nothing observable, but is transcribed exactly rather than
/// silently de-duplicated.
const STL_ALL: &[u8] = &[
    crate::statusline_defs::stl_flag::FILEPATH,
    crate::statusline_defs::stl_flag::FULLPATH,
    crate::statusline_defs::stl_flag::FILENAME,
    crate::statusline_defs::stl_flag::COLUMN,
    crate::statusline_defs::stl_flag::VIRTCOL,
    crate::statusline_defs::stl_flag::VIRTCOL_ALT,
    crate::statusline_defs::stl_flag::LINE,
    crate::statusline_defs::stl_flag::NUMLINES,
    crate::statusline_defs::stl_flag::BUFNO,
    crate::statusline_defs::stl_flag::KEYMAP,
    crate::statusline_defs::stl_flag::OFFSET,
    crate::statusline_defs::stl_flag::OFFSET_X,
    crate::statusline_defs::stl_flag::BYTEVAL,
    crate::statusline_defs::stl_flag::BYTEVAL_X,
    crate::statusline_defs::stl_flag::ROFLAG,
    crate::statusline_defs::stl_flag::ROFLAG_ALT,
    crate::statusline_defs::stl_flag::HELPFLAG,
    crate::statusline_defs::stl_flag::HELPFLAG_ALT,
    crate::statusline_defs::stl_flag::FILETYPE,
    crate::statusline_defs::stl_flag::FILETYPE_ALT,
    crate::statusline_defs::stl_flag::PREVIEWFLAG,
    crate::statusline_defs::stl_flag::PREVIEWFLAG_ALT,
    crate::statusline_defs::stl_flag::MODIFIED,
    crate::statusline_defs::stl_flag::MODIFIED_ALT,
    crate::statusline_defs::stl_flag::QUICKFIX,
    crate::statusline_defs::stl_flag::PERCENTAGE,
    crate::statusline_defs::stl_flag::ALTPERCENT,
    crate::statusline_defs::stl_flag::ARGLISTSTAT,
    crate::statusline_defs::stl_flag::PAGENUM,
    crate::statusline_defs::stl_flag::SHOWCMD,
    crate::statusline_defs::stl_flag::FOLDCOL,
    crate::statusline_defs::stl_flag::SIGNCOL,
    crate::statusline_defs::stl_flag::VIM_EXPR,
    crate::statusline_defs::stl_flag::SEPARATE,
    crate::statusline_defs::stl_flag::TRUNCMARK,
    crate::statusline_defs::stl_flag::USER_HL,
    crate::statusline_defs::stl_flag::HIGHLIGHT,
    crate::statusline_defs::stl_flag::HIGHLIGHT_COMB,
    crate::statusline_defs::stl_flag::TABPAGENR,
    crate::statusline_defs::stl_flag::TABCLOSENR,
    crate::statusline_defs::stl_flag::CLICK_FUNC,
    // Real, harmless duplicate from the original's own array literal.
    crate::statusline_defs::stl_flag::TABPAGENR,
    crate::statusline_defs::stl_flag::TABCLOSENR,
    crate::statusline_defs::stl_flag::CLICK_FUNC,
];

/// Check validity of options with the `'statusline'` format
/// (`check_stl_option`). Returns an error message, or `None` on
/// success.
///
/// Every dynamically-formatted "illegal character" message the
/// original builds via `illegal_char` is simplified to a static
/// [`crate::errors::e_invarg`], matching this file's own established
/// policy (the DISPLAYED text differs, the valid/invalid boolean
/// outcome is identical) - see this module's own doc comment.
///
/// Operates on byte positions rather than a NUL-terminated pointer
/// walk; every `*s`-past-the-end read in the original (which relies
/// on the implicit C-string NUL terminator) is replicated as an
/// explicit `pos >= s.len()` bounds check instead, EXCEPT at the
/// `STL_ALL` membership test: the original's own `vim_strchr` has a
/// real, deliberate `if (c <= 0) return NULL;` guard (`strings.c`),
/// so running off the end there is a genuine ILLEGAL CHARACTER, not
/// a graceful "found the terminator" match - confirmed directly
/// against a real `nvim` binary (a bare trailing `%` with nothing
/// after it is rejected as `E539: Illegal character <^@>`) before
/// trusting this, since an earlier draft assumed the opposite.
#[must_use]
pub fn check_stl_option(s: &[u8]) -> Option<&'static [u8]> {
    let len = s.len();
    let mut pos = 0usize;
    let mut groupdepth: i32 = 0;

    while pos < len {
        // Scan forward for the next '%'.
        while pos < len && s[pos] != b'%' {
            pos += 1;
        }
        if pos >= len {
            break;
        }
        pos += 1;

        if pos < len
            && (s[pos] == b'%'
                || s[pos] == crate::statusline_defs::stl_flag::TRUNCMARK
                || s[pos] == crate::statusline_defs::stl_flag::SEPARATE)
        {
            pos += 1;
            continue;
        }
        if pos < len && s[pos] == b')' {
            pos += 1;
            groupdepth -= 1;
            if groupdepth < 0 {
                break;
            }
            continue;
        }
        if pos < len && s[pos] == b'-' {
            pos += 1;
        }
        while pos < len && crate::ascii_defs::ascii_isdigit(i32::from(s[pos])) {
            pos += 1;
        }
        if pos < len && s[pos] == crate::statusline_defs::stl_flag::USER_HL {
            continue;
        }
        if pos < len && s[pos] == b'.' {
            pos += 1;
            while pos < len && crate::ascii_defs::ascii_isdigit(i32::from(s[pos])) {
                pos += 1;
            }
        }
        if pos < len && s[pos] == b'(' {
            groupdepth += 1;
            continue;
        }
        // The original checks `vim_strchr(STL_ALL, (uint8_t)(*s)) ==
        // NULL` here - and `vim_strchr` itself has a real, deliberate
        // `if (c <= 0) return NULL;` guard (`strings.c`), UNLIKE a
        // raw C-string dereference of `*s` past the string's own end
        // (which just reads the value 0). This means running off the
        // end here is genuinely, faithfully an ILLEGAL CHARACTER (a
        // dangling '%' with nothing after it is INVALID, not silently
        // accepted) - confirmed directly against a real `nvim` binary
        // before trusting this (an earlier draft assumed the
        // opposite, incorrectly treating a bare trailing '%' as
        // valid).
        if pos >= len || !STL_ALL.contains(&s[pos]) {
            return Some(crate::errors::e_invarg.as_bytes());
        }
        if s[pos] == crate::statusline_defs::stl_flag::VIM_EXPR {
            pos += 1; // `*++s`
            let reevaluate = pos < len && s[pos] == b'%';
            if reevaluate {
                pos += 1; // `*++s`
                if pos < len && s[pos] == b'}' {
                    // "}" is not allowed immediately after "%{%"
                    return Some(crate::errors::e_invarg.as_bytes());
                }
            }
            loop {
                if pos >= len {
                    break;
                }
                let stop = s[pos] == b'}' && (!reevaluate || (pos > 0 && s[pos - 1] == b'%'));
                if stop {
                    break;
                }
                pos += 1;
            }
            if pos >= len || s[pos] != b'}' {
                return Some(
                    crate::gettext_defs::gettext_noop("E540: Unclosed expression sequence")
                        .as_bytes(),
                );
            }
        }
    }

    if groupdepth != 0 {
        return Some(crate::gettext_defs::gettext_noop("E542: Unbalanced groups").as_bytes());
    }
    None
}

/// The `'iconstring'`/`'titlestring'` option is changed
/// (`did_set_titleiconstring`).
///
/// Updates `GLOBALS.stl_syntax`'s `flagval` bit depending on whether
/// the new value looks like `'statusline'` syntax (contains a `%`
/// AND passes [`check_stl_option`]), then calls the already-real
/// `crate::option::did_set_title` (a provable no-op today, see its
/// own doc comment).
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
unsafe fn did_set_titleiconstring(
    args: &mut crate::option_defs::OptsetT,
    flagval: i32,
) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);

    // NULL => statusline syntax
    // SAFETY: a plain field read/write, no aliasing hazard (no other
    // reference into `GLOBALS` is held across this call).
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if crate::strings::vim_strchr(val, i32::from(b'%')).is_some() && check_stl_option(val).is_none() {
        globals.stl_syntax |= flagval;
    } else {
        globals.stl_syntax &= !flagval;
    }
    crate::option::did_set_title();

    None
}

/// The `'iconstring'` option is changed (`did_set_iconstring`).
///
/// # Safety
/// Forwarded from `did_set_titleiconstring`'s own safety doc.
pub unsafe fn did_set_iconstring(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { did_set_titleiconstring(args, crate::globals::STL_IN_ICON) }
}

/// The `'titlestring'` option is changed (`did_set_titlestring`).
///
/// # Safety
/// Forwarded from `did_set_titleiconstring`'s own safety doc.
pub unsafe fn did_set_titlestring(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { did_set_titleiconstring(args, crate::globals::STL_IN_TITLE) }
}

/// The `'varsofttabstop'` option is changed (`did_set_varsofttabstop`).
///
/// Parses the comma-separated tab-width list via the already-real
/// `crate::indent::tabstop_set`, which already returns an
/// `Option<Vec<ColnrT>>` matching `BufT.b_p_vsts_array`'s own
/// representation directly - Rust's own assignment automatically
/// drops (frees) the previous array, matching the original's manual
/// `xfree(oldarray)` step.
///
/// # Safety
/// `args.os_buf` must be a valid, non-null pointer to a live `BufT`
/// for the whole call. `args.os_varp` must point to a live
/// `Option<Vec<u8>>`.
pub unsafe fn did_set_varsofttabstop(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *(args.os_buf as *mut crate::buffer_defs::BufT) };
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);

    match crate::indent::tabstop_set(val) {
        Ok(array) => {
            buf.b_p_vsts_array = array;
            None
        }
        Err(()) => Some(crate::errors::e_invarg.as_bytes()),
    }
}

/// The `'vartabstop'` option is changed (`did_set_vartabstop`).
///
/// Same shape as [`did_set_varsofttabstop`], targeting
/// `BufT.b_p_vts_array` instead, plus a real `'foldmethod'=="indent"`
/// check (`foldmethodIsIndent`, already real) whose own real
/// `foldUpdateAll` call panics - a genuinely REACHABLE gap (unlike
/// e.g. `AUTOCMDS`'s always-empty-registry precedent, `'foldmethod'`
/// is an ordinary string option that CAN legitimately be `"indent"`
/// in a real session) - `fold.rs`'s own real fold-tree
/// search/update machinery isn't translated yet.
///
/// # Safety
/// `args.os_buf`/`args.os_win` must be valid, non-null pointers to a
/// live `BufT`/`WinT` respectively, for the whole call. `args.os_varp`
/// must point to a live `Option<Vec<u8>>`.
pub unsafe fn did_set_vartabstop(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *(args.os_buf as *mut crate::buffer_defs::BufT) };
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &*(args.os_win as *const crate::buffer_defs::WinT) };
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);

    match crate::indent::tabstop_set(val) {
        Ok(array) => {
            buf.b_p_vts_array = array;
            if crate::fold::foldmethod_is_indent(win) {
                unimplemented!(
                    "did_set_vartabstop: foldUpdateAll - real fold-tree update machinery, not translated"
                );
            }
            None
        }
        Err(()) => Some(crate::errors::e_invarg.as_bytes()),
    }
}

/// The `'virtualedit'` option is changed (`did_set_virtualedit`).
///
/// Resolves `ve`/`flags` from either `win.w_onebuf_opt.wo_ve`/
/// `wo_ve_flags` (`OPT_LOCAL`) or `OPTION_VARS.p_ve`/`ve_flags`
/// (otherwise) - an owned copy rather than the original's own
/// pointer-aliasing trick, since Rust can't alias 2 different `&mut`
/// targets behind one binding. `opt_strings_flags` already returns a
/// brand-new flags value here (not a mutable out-param, matching this
/// crate's own established simplification), so the "recompute" path
/// just writes the result back to whichever target `use_local`
/// selected.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call. `args.os_varp` is NOT read (unlike most
/// `did_set_*` callbacks, this one only ever reads through
/// `args.os_win`/`OPTION_VARS`, matching the original's own body,
/// which never touches `args->os_varp` either).
pub unsafe fn did_set_virtualedit(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;
    let use_local = args.os_flags as u32 & crate::option_defs::opt_set_flags::OPT_LOCAL != 0;

    let ve: Vec<u8> = if use_local {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*win_ptr }.w_onebuf_opt.wo_ve.clone().unwrap_or_default()
    } else {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ve.clone().unwrap_or_default()
    };

    if use_local && ve.is_empty() {
        // make the local value empty: use the global value
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *win_ptr }.w_onebuf_opt.wo_ve_flags = 0;
        return None;
    }

    let Some(new_flags) = opt_strings_flags(&ve, crate::option_vars::OPT_VE_VALUES, true) else {
        return Some(crate::errors::e_invarg.as_bytes());
    };

    if use_local {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *win_ptr }.w_onebuf_opt.wo_ve_flags = new_flags;
    } else {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags = new_flags;
    }

    let old_matches =
        matches!(&args.os_oldval, crate::option_defs::OptVal::String(old) if *old == ve);
    if !old_matches {
        // Recompute cursor position in case the new 've' setting
        // changes something.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::r#move::validate_virtcol(win_ptr) };
        // SAFETY: forwarded from this function's own safety doc.
        let virtcol = unsafe { &*win_ptr }.w_virtcol;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::cursor::coladvance(win_ptr, virtcol) };
    }

    None
}

/// The `'tagcase'` option is changed (`did_set_tagcase`).
///
/// Same "resolve from local or global storage as an owned copy"
/// pattern already established by [`did_set_virtualedit`], but
/// simpler - no cursor-position recompute step at all, and
/// `opt_strings_flags`'s own `list` parameter is `false` (a single
/// value, not a comma-separated list, unlike `'virtualedit'`).
///
/// # Safety
/// `args.os_buf` must be a valid, non-null pointer to a live `BufT`
/// for the whole call.
pub unsafe fn did_set_tagcase(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let buf_ptr = args.os_buf as *mut crate::buffer_defs::BufT;
    let use_local = args.os_flags as u32 & crate::option_defs::opt_set_flags::OPT_LOCAL != 0;

    let p: Vec<u8> = if use_local {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*buf_ptr }.b_p_tc.clone().unwrap_or_default()
    } else {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_tc.clone().unwrap_or_default()
    };

    if use_local && p.is_empty() {
        // make the local value empty: use the global value
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *buf_ptr }.b_tc_flags = 0;
        return None;
    }

    let Some(new_flags) = opt_strings_flags(&p, crate::option_vars::OPT_TC_VALUES, false) else {
        return Some(crate::errors::e_invarg.as_bytes());
    };

    if use_local {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *buf_ptr }.b_tc_flags = new_flags;
    } else {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tc_flags = new_flags;
    }

    None
}

/// The `'concealcursor'` option is changed (`did_set_concealcursor`).
///
/// A thin `did_set_option_listflag` wrapper over
/// `option_vars::COCU_ALL` - unlike `'whichwrap'`, `'concealcursor'`
/// is NOT a comma-separated list, so no separator character is
/// appended to the valid-character set.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
pub unsafe fn did_set_concealcursor(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);
    did_set_option_listflag(val, crate::option_vars::COCU_ALL.as_bytes())
}

/// The `'completeslash'` option is changed (`did_set_completeslash`).
///
/// Windows-only, matching the original's own `#ifdef
/// BACKSLASH_IN_FILENAME` guard around this whole function (the
/// option itself is likewise `enable_if`-gated to that same platform,
/// per `option_defs.rs`'s own already-established handling) -
/// translated as `#[cfg(windows)]`, following `os/os_defs.rs`'s own
/// `BACKSLASH_IN_FILENAME_BOOL` precedent.
///
/// Validates BOTH the global `'completeslash'` and the buffer-local
/// one, regardless of which was actually being set - faithfully
/// matching the original's own two-call `||` condition rather than
/// "fixing" it to check only the one that changed.
///
/// # Safety
/// `args.os_buf` must be a valid, non-null pointer to a live `BufT`
/// for the whole call.
#[cfg(windows)]
pub unsafe fn did_set_completeslash(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*(args.os_buf as *const crate::buffer_defs::BufT) };
    let p_csl = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_csl.clone().unwrap_or_default();
    let b_p_csl: &[u8] = buf.b_p_csl.as_deref().unwrap_or(&[]);

    if opt_strings_flags(&p_csl, crate::option_vars::OPT_CSL_VALUES, false).is_none()
        || opt_strings_flags(b_p_csl, crate::option_vars::OPT_CSL_VALUES, false).is_none()
    {
        return Some(crate::errors::e_invarg.as_bytes());
    }
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

    // ---- did_set_lispoptions ----

    fn set_varp_args(val: &mut Option<Vec<u8>>) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT { os_varp: val as *mut Option<Vec<u8>> as *mut c_void, ..Default::default() }
    }

    #[test]
    fn did_set_lispoptions_empty_is_valid() {
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = set_varp_args(&mut val);
        assert_eq!(unsafe { did_set_lispoptions(&mut args) }, None);
    }

    #[test]
    fn did_set_lispoptions_accepts_expr_0_and_expr_1() {
        let mut val: Option<Vec<u8>> = Some(b"expr:0".to_vec());
        let mut args = set_varp_args(&mut val);
        assert_eq!(unsafe { did_set_lispoptions(&mut args) }, None);

        let mut val2: Option<Vec<u8>> = Some(b"expr:1".to_vec());
        let mut args2 = set_varp_args(&mut val2);
        assert_eq!(unsafe { did_set_lispoptions(&mut args2) }, None);
    }

    #[test]
    fn did_set_lispoptions_rejects_anything_else() {
        let mut val: Option<Vec<u8>> = Some(b"expr:2".to_vec());
        let mut args = set_varp_args(&mut val);
        assert_eq!(unsafe { did_set_lispoptions(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));

        let mut val2: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let mut args2 = set_varp_args(&mut val2);
        assert_eq!(unsafe { did_set_lispoptions(&mut args2) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    // ---- did_set_matchpairs ----

    #[test]
    fn did_set_matchpairs_empty_is_valid() {
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = set_varp_args(&mut val);
        assert_eq!(unsafe { did_set_matchpairs(&mut args) }, None);
    }

    #[test]
    fn did_set_matchpairs_single_pair_with_no_trailing_comma_is_valid() {
        let mut val: Option<Vec<u8>> = Some(b"(:)".to_vec());
        let mut args = set_varp_args(&mut val);
        assert_eq!(unsafe { did_set_matchpairs(&mut args) }, None);
    }

    #[test]
    fn did_set_matchpairs_the_real_default_value_is_valid() {
        let mut val: Option<Vec<u8>> = Some(b"(:),{:},[:]".to_vec());
        let mut args = set_varp_args(&mut val);
        assert_eq!(unsafe { did_set_matchpairs(&mut args) }, None);
    }

    #[test]
    fn did_set_matchpairs_wrong_middle_character_is_invalid() {
        let mut val: Option<Vec<u8>> = Some(b"(-)".to_vec());
        let mut args = set_varp_args(&mut val);
        assert_eq!(unsafe { did_set_matchpairs(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn did_set_matchpairs_missing_second_character_is_invalid() {
        let mut val: Option<Vec<u8>> = Some(b"(:".to_vec());
        let mut args = set_varp_args(&mut val);
        assert_eq!(unsafe { did_set_matchpairs(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn did_set_matchpairs_trailing_comma_with_nothing_after_is_valid() {
        // A genuine, real quirk of the original: once one pair parses
        // successfully and the following byte is a comma, the for
        // loop's own increment consumes it - if that lands exactly on
        // the terminator, the loop's own condition simply exits
        // cleanly, never re-entering the body to notice nothing
        // follows. Preserved faithfully, not "fixed" to reject this.
        let mut val: Option<Vec<u8>> = Some(b"(:),".to_vec());
        let mut args = set_varp_args(&mut val);
        assert_eq!(unsafe { did_set_matchpairs(&mut args) }, None);
    }

    #[test]
    fn did_set_matchpairs_a_doubled_comma_is_invalid() {
        // The comma right after ")" is consumed by the for-loop's own
        // increment; the SECOND, adjacent comma is then treated as
        // the next pair's own "X" character, so the byte after it
        // ('{') is read as x2 - which isn't ':', so this is correctly
        // rejected.
        let mut val: Option<Vec<u8>> = Some(b"(:),,{:}".to_vec());
        let mut args = set_varp_args(&mut val);
        assert_eq!(unsafe { did_set_matchpairs(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    // ---- did_set_selection ----

    #[test]
    fn did_set_selection_accepts_every_real_value() {
        for val in crate::option_vars::OPT_SEL_VALUES {
            let mut val_opt: Option<Vec<u8>> = Some(val.as_bytes().to_vec());
            let varp = &mut val_opt as *mut Option<Vec<u8>> as *mut c_void;
            let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Selection, os_varp: varp, ..Default::default() };
            assert_eq!(unsafe { did_set_selection(&mut args) }, None, "value {val:?} should be accepted");
        }
    }

    #[test]
    fn did_set_selection_rejects_an_unknown_value() {
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Selection, os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_selection(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    // ---- did_set_sessionoptions ----

    #[test]
    fn did_set_sessionoptions_accepts_a_valid_combination() {
        let _lock = crate::globals::global_state_test_lock();
        let mut val: Option<Vec<u8>> = Some(b"blank,help".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT {
            os_idx: OptIndex::Sessionoptions,
            os_varp: varp,
            os_oldval: crate::option_defs::OptVal::String(Vec::new()),
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_sessionoptions(&mut args) }, None);
    }

    #[test]
    fn did_set_sessionoptions_rejects_sesdir_and_curdir_together_and_restores_old_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let mut val: Option<Vec<u8>> = Some(b"sesdir,curdir".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT {
            os_idx: OptIndex::Sessionoptions,
            os_varp: varp,
            os_oldval: crate::option_defs::OptVal::String(b"blank".to_vec()),
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_sessionoptions(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));

        // ssop_flags is restored to whatever "blank" (the old value)
        // implies - "blank" is index 7 in OPT_SSOP_VALUES.
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ssop_flags, 1 << 7);
    }

    #[test]
    fn did_set_sessionoptions_invalid_value_fails_before_the_sesdir_curdir_check() {
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Sessionoptions, os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_sessionoptions(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    // ---- did_set_keymodel ----

    #[test]
    fn did_set_keymodel_sets_stopsel_and_startsel_from_o_and_a() {
        let _lock = crate::globals::global_state_test_lock();
        // The original reads the GLOBAL `p_km` directly (not
        // `args->os_varp`) - matching a real invocation, where
        // `os_varp` points at this SAME global storage for a
        // global-only option, `OPTION_VARS.p_km` is set to the new
        // value directly here, and `os_varp` points at it too.
        // Derived via `as_ptr()` (not `get_mut()`) so this pointer
        // survives `did_set_keymodel`'s OWN internal `get_mut()`
        // call without being invalidated under Tree Borrows.
        let ov_ptr = crate::option_vars::OPTION_VARS.as_ptr();
        let km_ptr = unsafe { std::ptr::addr_of_mut!((*ov_ptr).p_km) };
        let prev = unsafe { (*km_ptr).clone() };
        unsafe { *km_ptr = Some(b"stopsel,startsel".to_vec()) };
        let varp = km_ptr as *mut c_void;

        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Keymodel, os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_keymodel(&mut args) }, None);
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(globals.km_stopsel);
        assert!(globals.km_startsel);

        unsafe { *km_ptr = prev };
    }

    #[test]
    fn did_set_keymodel_empty_clears_both_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.km_stopsel = true;
        globals.km_startsel = true;

        let ov_ptr = crate::option_vars::OPTION_VARS.as_ptr();
        let km_ptr = unsafe { std::ptr::addr_of_mut!((*ov_ptr).p_km) };
        let prev = unsafe { (*km_ptr).clone() };
        unsafe { *km_ptr = Some(Vec::new()) };
        let varp = km_ptr as *mut c_void;

        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Keymodel, os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_keymodel(&mut args) }, None);
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(!globals.km_stopsel);
        assert!(!globals.km_startsel);

        unsafe { *km_ptr = prev };
    }

    // ---- did_set_showcmdloc ----

    #[test]
    fn did_set_showcmdloc_valid_value_recomputes_comp_col() {
        let mut val: Option<Vec<u8>> = Some(b"last".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Showcmdloc, os_varp: varp, ..Default::default() };
        // comp_col() itself is exercised extensively in drawscreen.rs's
        // own tests - this only verifies did_set_showcmdloc reaches
        // and calls it without panicking, on a valid value.
        assert_eq!(unsafe { did_set_showcmdloc(&mut args) }, None);
    }

    #[test]
    fn did_set_showcmdloc_invalid_value_fails_without_calling_comp_col() {
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Showcmdloc, os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_showcmdloc(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    // ---- did_set_splitkeep ----

    #[test]
    fn did_set_splitkeep_snapshots_curtab_window_heights_via_firstwin() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_first_tabpage = globals.first_tabpage;
        let prev_curtab = globals.curtab;
        let prev_firstwin = globals.firstwin;

        let mut win = crate::buffer_defs::WinT { w_height: 12, w_prev_height: 0, w_next: std::ptr::null_mut(), ..Default::default() };
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.first_tabpage = tp_ptr;
        globals.curtab = tp_ptr;
        globals.firstwin = win_ptr;

        let mut val: Option<Vec<u8>> = Some(b"cursor".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Splitkeep, os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_splitkeep(&mut args) }, None);
        assert_eq!(unsafe { &*win_ptr }.w_prev_height, 12);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.first_tabpage = prev_first_tabpage;
        globals.curtab = prev_curtab;
        globals.firstwin = prev_firstwin;
    }

    #[test]
    fn did_set_splitkeep_snapshots_a_non_current_tabpage_via_its_own_tp_firstwin() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_first_tabpage = globals.first_tabpage;
        let prev_curtab = globals.curtab;

        let mut other_win =
            crate::buffer_defs::WinT { w_height: 33, w_prev_height: 0, w_next: std::ptr::null_mut(), ..Default::default() };
        let other_win_ptr = &mut other_win as *mut crate::buffer_defs::WinT;
        let mut other_tp = crate::buffer_defs::TabpageT { tp_firstwin: other_win_ptr, tp_next: std::ptr::null_mut(), ..Default::default() };
        let other_tp_ptr = &mut other_tp as *mut crate::buffer_defs::TabpageT;

        // A separate "current" tabpage with no windows of its own -
        // just needs to be a distinct, valid tabpage so `other_tp` is
        // NOT `curtab` (exercising the `tp_firstwin` branch, not the
        // `GLOBALS.firstwin` one).
        let mut curtab = crate::buffer_defs::TabpageT { tp_firstwin: std::ptr::null_mut(), tp_next: other_tp_ptr, ..Default::default() };
        let curtab_ptr = &mut curtab as *mut crate::buffer_defs::TabpageT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.first_tabpage = curtab_ptr;
        globals.curtab = curtab_ptr;
        globals.firstwin = std::ptr::null_mut();

        let mut val: Option<Vec<u8>> = Some(b"screen".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Splitkeep, os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_splitkeep(&mut args) }, None);
        assert_eq!(unsafe { &*other_win_ptr }.w_prev_height, 33);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.first_tabpage = prev_first_tabpage;
        globals.curtab = prev_curtab;
    }

    // ---- did_set_spellsuggest ----

    fn set_p_sps(value: Option<&[u8]>) -> Option<Vec<u8>> {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_sps.clone();
        opts.p_sps = value.map(<[u8]>::to_vec);
        prev
    }

    #[test]
    fn did_set_spellsuggest_valid_value_is_ok() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_sps(Some(b"best,10"));
        assert_eq!(unsafe { did_set_spellsuggest() }, None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sps = prev;
    }

    #[test]
    fn did_set_spellsuggest_invalid_value_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_sps(Some(b"bogus"));
        assert_eq!(unsafe { did_set_spellsuggest() }, Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sps = prev;
    }

    // ---- did_set_mkspellmem ----

    fn set_p_msm(value: Option<&[u8]>) -> Option<Vec<u8>> {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_msm.clone();
        opts.p_msm = value.map(<[u8]>::to_vec);
        prev
    }

    #[test]
    fn did_set_mkspellmem_valid_value_is_ok() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_msm(Some(b"460000,2000,500"));
        assert_eq!(unsafe { did_set_mkspellmem() }, None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_msm = prev;
    }

    #[test]
    fn did_set_mkspellmem_invalid_value_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_msm(Some(b"bogus"));
        assert_eq!(unsafe { did_set_mkspellmem() }, Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_msm = prev;
    }

    // ---- did_set_option_listflag / did_set_mouse ----

    #[test]
    fn did_set_option_listflag_accepts_every_character_in_flags() {
        assert_eq!(did_set_option_listflag(b"anvi", crate::option_vars::MOUSE_ALL.as_bytes()), None);
    }

    #[test]
    fn did_set_option_listflag_empty_val_is_vacuously_valid() {
        assert_eq!(did_set_option_listflag(b"", crate::option_vars::MOUSE_ALL.as_bytes()), None);
    }

    #[test]
    fn did_set_option_listflag_rejects_a_character_not_in_flags() {
        assert_eq!(
            did_set_option_listflag(b"anz", crate::option_vars::MOUSE_ALL.as_bytes()),
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_mouse_accepts_every_real_mouse_flag() {
        // MOUSE_ALL == "anvichr" - every one of its own characters,
        // in any combination, must be individually valid.
        let mut val: Option<Vec<u8>> = Some(b"anvichr".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT { os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_mouse(&mut args) }, None);
    }

    #[test]
    fn did_set_mouse_empty_is_valid() {
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT { os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_mouse(&mut args) }, None);
    }

    #[test]
    fn did_set_mouse_rejects_an_unknown_flag_character() {
        let mut val: Option<Vec<u8>> = Some(b"az".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT { os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_mouse(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    // ---- did_set_whichwrap ----

    fn whichwrap_args(val: &mut Option<Vec<u8>>) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT { os_varp: val as *mut Option<Vec<u8>> as *mut c_void, ..Default::default() }
    }

    #[test]
    fn did_set_whichwrap_accepts_every_real_flag_character() {
        // WW_ALL == "bshl<>[]~" - every one of its own characters is
        // individually valid.
        let mut val: Option<Vec<u8>> = Some(b"bshl<>[]~".to_vec());
        let mut args = whichwrap_args(&mut val);
        assert_eq!(unsafe { did_set_whichwrap(&mut args) }, None);
    }

    #[test]
    fn did_set_whichwrap_accepts_a_comma_separated_list() {
        // The real default value: a comma-separated list of flags.
        let mut val: Option<Vec<u8>> = Some(b"b,s".to_vec());
        let mut args = whichwrap_args(&mut val);
        assert_eq!(unsafe { did_set_whichwrap(&mut args) }, None);
    }

    #[test]
    fn did_set_whichwrap_empty_is_valid() {
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = whichwrap_args(&mut val);
        assert_eq!(unsafe { did_set_whichwrap(&mut args) }, None);
    }

    #[test]
    fn did_set_whichwrap_rejects_an_unknown_flag_character() {
        let mut val: Option<Vec<u8>> = Some(b"bz".to_vec());
        let mut args = whichwrap_args(&mut val);
        assert_eq!(unsafe { did_set_whichwrap(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    // ---- did_set_virtualedit ----

    /// Builds an `OptsetT` with `os_oldval` pre-set to match `ve`
    /// exactly, so `did_set_virtualedit`'s own "value genuinely
    /// changed" recompute path (`validate_virtcol`/`coladvance`,
    /// which need a real memline) is never reached - used by every
    /// test below that isn't specifically exercising that path.
    fn virtualedit_args_no_recompute(
        win: &mut crate::buffer_defs::WinT,
        flags: u32,
        ve: &[u8],
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_flags: flags as i32,
            os_oldval: crate::option_defs::OptVal::String(ve.to_vec()),
            ..Default::default()
        }
    }

    #[test]
    fn did_set_virtualedit_global_valid_value_sets_ve_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags;
        let prev_p_ve = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ve.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ve = Some(b"all".to_vec());

        let mut win = crate::buffer_defs::WinT::default();
        let mut args = virtualedit_args_no_recompute(&mut win, 0, b"all");
        assert_eq!(unsafe { did_set_virtualedit(&mut args) }, None);
        assert_eq!(
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags,
            crate::option_vars::opt_ve_flag::ALL
        );

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags = prev;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ve = prev_p_ve;
    }

    #[test]
    fn did_set_virtualedit_global_invalid_value_fails_and_leaves_flags_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags;
        let prev_p_ve = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ve.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags = 0xDEAD;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ve = Some(b"bogus".to_vec());

        let mut win = crate::buffer_defs::WinT::default();
        let mut args = virtualedit_args_no_recompute(&mut win, 0, b"bogus");
        assert_eq!(
            unsafe { did_set_virtualedit(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags, 0xDEAD);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags = prev;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ve = prev_p_ve;
    }

    #[test]
    fn did_set_virtualedit_local_empty_resets_to_global() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_ve = Some(Vec::new());
        win.w_onebuf_opt.wo_ve_flags = crate::option_vars::opt_ve_flag::ALL;
        let mut args = virtualedit_args_no_recompute(
            &mut win,
            crate::option_defs::opt_set_flags::OPT_LOCAL,
            b"",
        );
        assert_eq!(unsafe { did_set_virtualedit(&mut args) }, None);
        assert_eq!(win.w_onebuf_opt.wo_ve_flags, 0);
    }

    #[test]
    fn did_set_virtualedit_local_valid_value_sets_wo_ve_flags() {
        // Uses "all" (index 2 in OPT_VE_VALUES) since its own
        // opt_ve_flag::ALL constant (0x04) genuinely matches
        // opt_strings_flags's own `1 << index` scheme; "block"/
        // "insert" (indices 0/1) do NOT - their opt_ve_flag constants
        // (0x05/0x06) are dead, unreferenced-anywhere-in-the-real-
        // source generator artifacts, confirmed by grepping the whole
        // original codebase, not values opt_strings_flags itself ever
        // actually produces.
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_ve = Some(b"all".to_vec());
        let mut args = virtualedit_args_no_recompute(
            &mut win,
            crate::option_defs::opt_set_flags::OPT_LOCAL,
            b"all",
        );
        assert_eq!(unsafe { did_set_virtualedit(&mut args) }, None);
        assert_eq!(win.w_onebuf_opt.wo_ve_flags, crate::option_vars::opt_ve_flag::ALL);
    }

    #[test]
    fn did_set_virtualedit_local_invalid_value_fails() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_ve = Some(b"bogus".to_vec());
        let mut args = virtualedit_args_no_recompute(
            &mut win,
            crate::option_defs::opt_set_flags::OPT_LOCAL,
            b"bogus",
        );
        assert_eq!(
            unsafe { did_set_virtualedit(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_virtualedit_recomputes_cursor_position_when_value_genuinely_changes() {
        // Uses the same real-memline test-fixture pattern established
        // in cursor.rs's own test module (`CursorTestGuard`/
        // `open_and_set_test_buf`) since the recompute path
        // (validate_virtcol/coladvance) needs a real w_buffer.
        struct VirtualeditTestGuard {
            prev_curwin: *mut crate::buffer_defs::WinT,
            prev_curbuf: *mut crate::buffer_defs::BufT,
            _lock: std::sync::MutexGuard<'static, ()>,
        }
        impl VirtualeditTestGuard {
            fn set(win: *mut crate::buffer_defs::WinT, buf: *mut crate::buffer_defs::BufT) -> Self {
                let _lock = crate::globals::global_state_test_lock();
                let globals = unsafe { crate::globals::GLOBALS.get_mut() };
                let guard = VirtualeditTestGuard {
                    prev_curwin: globals.curwin,
                    prev_curbuf: globals.curbuf,
                    _lock,
                };
                globals.curwin = win;
                globals.curbuf = buf;
                guard
            }
        }
        impl Drop for VirtualeditTestGuard {
            fn drop(&mut self) {
                let globals = unsafe { crate::globals::GLOBALS.get_mut() };
                globals.curwin = self.prev_curwin;
                globals.curbuf = self.prev_curbuf;
            }
        }

        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT::default();
        let guard = VirtualeditTestGuard::set(&mut win as *mut _, &mut buf as *mut _);
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        win.w_buffer = &mut buf as *mut crate::buffer_defs::BufT;

        let prev_p_ve = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ve.clone();
        let prev_ve_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ve = Some(b"all".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_win: &mut win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_flags: 0,
            // A genuinely different old value forces the recompute path.
            os_oldval: crate::option_defs::OptVal::String(b"".to_vec()),
            ..Default::default()
        };

        // Must not panic - validate_virtcol/coladvance both run
        // through their own real, working logic here.
        assert_eq!(unsafe { did_set_virtualedit(&mut args) }, None);
        assert!(win.w_valid & i32::from(crate::buffer_defs::w_valid::VALID_VIRTCOL) != 0);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ve = prev_p_ve;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags = prev_ve_flags;

        drop(guard);
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    // ---- did_set_tagcase ----

    fn tagcase_args(buf: &mut crate::buffer_defs::BufT, flags: u32) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_buf: buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_flags: flags as i32,
            ..Default::default()
        }
    }

    #[test]
    fn did_set_tagcase_global_valid_value_sets_tc_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tc_flags;
        let prev_p_tc = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_tc.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_tc = Some(b"ignore".to_vec());

        let mut buf = crate::buffer_defs::BufT::default();
        let mut args = tagcase_args(&mut buf, 0);
        assert_eq!(unsafe { did_set_tagcase(&mut args) }, None);
        // "ignore" is index 1 in OPT_TC_VALUES, matching
        // opt_strings_flags's own `1 << index` scheme exactly.
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tc_flags, 0x02);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tc_flags = prev;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_tc = prev_p_tc;
    }

    #[test]
    fn did_set_tagcase_global_invalid_value_fails_and_leaves_flags_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tc_flags;
        let prev_p_tc = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_tc.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tc_flags = 0xDEAD;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_tc = Some(b"bogus".to_vec());

        let mut buf = crate::buffer_defs::BufT::default();
        let mut args = tagcase_args(&mut buf, 0);
        assert_eq!(unsafe { did_set_tagcase(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tc_flags, 0xDEAD);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tc_flags = prev;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_tc = prev_p_tc;
    }

    #[test]
    fn did_set_tagcase_local_empty_resets_to_global() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_tc: Some(Vec::new()),
            b_tc_flags: 0x02,
            ..Default::default()
        };
        let mut args = tagcase_args(&mut buf, crate::option_defs::opt_set_flags::OPT_LOCAL);
        assert_eq!(unsafe { did_set_tagcase(&mut args) }, None);
        assert_eq!(buf.b_tc_flags, 0);
    }

    #[test]
    fn did_set_tagcase_local_valid_value_sets_b_tc_flags() {
        let mut buf = crate::buffer_defs::BufT { b_p_tc: Some(b"smart".to_vec()), ..Default::default() };
        let mut args = tagcase_args(&mut buf, crate::option_defs::opt_set_flags::OPT_LOCAL);
        assert_eq!(unsafe { did_set_tagcase(&mut args) }, None);
        // "smart" is index 4 in OPT_TC_VALUES.
        assert_eq!(buf.b_tc_flags, 0x10);
    }

    #[test]
    fn did_set_tagcase_local_invalid_value_fails() {
        let mut buf = crate::buffer_defs::BufT { b_p_tc: Some(b"bogus".to_vec()), ..Default::default() };
        let mut args = tagcase_args(&mut buf, crate::option_defs::opt_set_flags::OPT_LOCAL);
        assert_eq!(unsafe { did_set_tagcase(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    // ---- did_set_concealcursor ----

    fn concealcursor_args(val: &mut Option<Vec<u8>>) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT { os_varp: val as *mut Option<Vec<u8>> as *mut c_void, ..Default::default() }
    }

    #[test]
    fn did_set_concealcursor_accepts_every_real_flag_character() {
        // COCU_ALL == "nvic".
        let mut val: Option<Vec<u8>> = Some(b"nvic".to_vec());
        let mut args = concealcursor_args(&mut val);
        assert_eq!(unsafe { did_set_concealcursor(&mut args) }, None);
    }

    #[test]
    fn did_set_concealcursor_empty_is_valid() {
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = concealcursor_args(&mut val);
        assert_eq!(unsafe { did_set_concealcursor(&mut args) }, None);
    }

    #[test]
    fn did_set_concealcursor_rejects_an_unknown_flag_character() {
        let mut val: Option<Vec<u8>> = Some(b"nz".to_vec());
        let mut args = concealcursor_args(&mut val);
        assert_eq!(
            unsafe { did_set_concealcursor(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_concealcursor_rejects_a_comma_unlike_whichwrap() {
        // 'concealcursor' is NOT a comma-separated list, so (unlike
        // 'whichwrap') a comma is genuinely an invalid character here.
        let mut val: Option<Vec<u8>> = Some(b"n,v".to_vec());
        let mut args = concealcursor_args(&mut val);
        assert_eq!(
            unsafe { did_set_concealcursor(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    // ---- did_set_completeslash (Windows-only) ----

    #[cfg(windows)]
    #[test]
    fn did_set_completeslash_accepts_every_real_value() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_csl.clone();

        for value in [&b""[..], b"slash", b"backslash"] {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_csl = Some(value.to_vec());
            let mut buf =
                crate::buffer_defs::BufT { b_p_csl: Some(value.to_vec()), ..Default::default() };
            let mut args = crate::option_defs::OptsetT {
                os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
                ..Default::default()
            };
            assert_eq!(unsafe { did_set_completeslash(&mut args) }, None, "value {value:?}");
        }

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_csl = prev;
    }

    #[cfg(windows)]
    #[test]
    fn did_set_completeslash_rejects_a_bad_global_value() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_csl.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_csl = Some(b"bogus".to_vec());

        let mut buf = crate::buffer_defs::BufT { b_p_csl: Some(b"slash".to_vec()), ..Default::default() };
        let mut args = crate::option_defs::OptsetT {
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_completeslash(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_csl = prev;
    }

    #[cfg(windows)]
    #[test]
    fn did_set_completeslash_rejects_a_bad_buffer_local_value_even_when_global_is_fine() {
        // Faithfully exercises the original's own two-call `||`
        // condition: a bad LOCAL value is rejected even though the
        // global one is perfectly valid.
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_csl.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_csl = Some(b"slash".to_vec());

        let mut buf = crate::buffer_defs::BufT { b_p_csl: Some(b"bogus".to_vec()), ..Default::default() };
        let mut args = crate::option_defs::OptsetT {
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_completeslash(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_csl = prev;
    }

    // ---- did_set_mousescroll ----

    fn set_p_mousescroll(value: Option<&[u8]>) -> Option<Vec<u8>> {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_mousescroll.clone();
        opts.p_mousescroll = value.map(<[u8]>::to_vec);
        prev
    }

    #[test]
    fn did_set_mousescroll_the_real_default_value_sets_both_directions() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"ver:3,hor:6"));
        assert_eq!(unsafe { did_set_mousescroll() }, None);
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        assert_eq!(opts.p_mousescroll_vert, 3);
        assert_eq!(opts.p_mousescroll_hor, 6);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_only_vertical_falls_back_to_the_horizontal_default() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"ver:5"));
        assert_eq!(unsafe { did_set_mousescroll() }, None);
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        assert_eq!(opts.p_mousescroll_vert, 5);
        assert_eq!(opts.p_mousescroll_hor, crate::option_vars::MOUSESCROLL_HOR_DFLT);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_only_horizontal_falls_back_to_the_vertical_default() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"hor:10"));
        assert_eq!(unsafe { did_set_mousescroll() }, None);
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        assert_eq!(opts.p_mousescroll_vert, crate::option_vars::MOUSESCROLL_VERT_DFLT);
        assert_eq!(opts.p_mousescroll_hor, 10);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_duplicate_direction_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"ver:1,ver:2"));
        assert_eq!(unsafe { did_set_mousescroll() }, Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_unknown_direction_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"foo:1"));
        assert_eq!(unsafe { did_set_mousescroll() }, Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_too_short_fails() {
        let _lock = crate::globals::global_state_test_lock();
        // length == 4 ("ver:"), no digit at all - length <= 4 fails
        // before the direction/digit checks even run.
        let prev = set_p_mousescroll(Some(b"ver:"));
        assert_eq!(unsafe { did_set_mousescroll() }, Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_non_digit_after_colon_reports_e5080() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"ver:x"));
        assert_eq!(
            unsafe { did_set_mousescroll() },
            Some(crate::gettext_defs::gettext_noop("E5080: Digit expected").as_bytes())
        );
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_empty_value_fails() {
        let _lock = crate::globals::global_state_test_lock();
        // A genuine, real quirk of the original: an empty value makes
        // `length` (== strlen("") == 0) satisfy `length <= 4`,
        // rejecting it immediately - not a translation bug.
        let prev = set_p_mousescroll(Some(b""));
        assert_eq!(unsafe { did_set_mousescroll() }, Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_allows_zero_to_disable_scrolling() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"ver:0,hor:0"));
        assert_eq!(unsafe { did_set_mousescroll() }, None);
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        assert_eq!(opts.p_mousescroll_vert, 0);
        assert_eq!(opts.p_mousescroll_hor, 0);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    // ---- did_set_showbreak ----

    fn showbreak_args(val: &mut Option<Vec<u8>>) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT { os_varp: val as *mut Option<Vec<u8>> as *mut c_void, ..Default::default() }
    }

    #[test]
    fn did_set_showbreak_empty_is_valid() {
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = showbreak_args(&mut val);
        assert_eq!(unsafe { did_set_showbreak(&mut args) }, None);
    }

    #[test]
    fn did_set_showbreak_plain_ascii_is_valid() {
        let mut val: Option<Vec<u8>> = Some(b"->".to_vec());
        let mut args = showbreak_args(&mut val);
        assert_eq!(unsafe { did_set_showbreak(&mut args) }, None);
    }

    #[test]
    fn did_set_showbreak_control_character_is_invalid() {
        let mut val: Option<Vec<u8>> = Some(vec![0x01]);
        let mut args = showbreak_args(&mut val);
        assert_eq!(
            unsafe { did_set_showbreak(&mut args) },
            Some(
                crate::gettext_defs::gettext_noop(
                    "E595: 'showbreak' contains unprintable or wide character"
                )
                .as_bytes()
            )
        );
    }

    #[test]
    fn did_set_showbreak_double_wide_character_is_invalid() {
        // U+65E5 ("日") is a double-wide CJK character (2 screen
        // cells), confirmed via ptr2cells in an earlier session.
        let mut val: Option<Vec<u8>> = Some("日".as_bytes().to_vec());
        let mut args = showbreak_args(&mut val);
        assert_eq!(
            unsafe { did_set_showbreak(&mut args) },
            Some(
                crate::gettext_defs::gettext_noop(
                    "E595: 'showbreak' contains unprintable or wide character"
                )
                .as_bytes()
            )
        );
    }

    #[test]
    fn did_set_showbreak_rejects_the_first_bad_character_even_after_good_ones() {
        let mut val: Option<Vec<u8>> = Some(b"ok\x01".to_vec());
        let mut args = showbreak_args(&mut val);
        assert!(unsafe { did_set_showbreak(&mut args) }.is_some());
    }

    // ---- did_set_wildmode ----

    fn set_p_wim(value: Option<&[u8]>) -> Option<Vec<u8>> {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_wim.clone();
        opts.p_wim = value.map(<[u8]>::to_vec);
        prev
    }

    #[test]
    fn did_set_wildmode_valid_value_is_ok() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_wim(Some(b"full"));
        assert_eq!(unsafe { did_set_wildmode() }, None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wim = prev;
    }

    #[test]
    fn did_set_wildmode_invalid_value_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_wim(Some(b"bogus"));
        assert_eq!(unsafe { did_set_wildmode() }, Some(crate::errors::e_invarg.as_bytes()));
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wim = prev;
    }

    // ---- check_stl_option ----

    #[test]
    fn check_stl_option_empty_string_is_ok() {
        assert_eq!(check_stl_option(b""), None);
    }

    #[test]
    fn check_stl_option_plain_text_with_no_percent_is_ok() {
        assert_eq!(check_stl_option(b"just plain text"), None);
    }

    #[test]
    fn check_stl_option_a_bare_trailing_percent_is_illegal() {
        // vim_strchr's own `if (c <= 0) return NULL;` guard means a
        // dangling '%' with nothing after it is a genuine illegal
        // character (NUL), NOT a graceful match against STL_ALL's own
        // trailing sentinel - verified directly against a real `nvim`
        // binary (`E539: Illegal character <^@>`) before trusting
        // this.
        assert_eq!(check_stl_option(b"%"), Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn check_stl_option_percent_percent_is_a_literal_escape() {
        assert_eq!(check_stl_option(b"%%"), None);
    }

    #[test]
    fn check_stl_option_truncmark_and_separate_are_ok() {
        assert_eq!(check_stl_option(b"%<"), None);
        assert_eq!(check_stl_option(b"%="), None);
    }

    #[test]
    fn check_stl_option_unrecognized_flag_character_fails() {
        assert_eq!(check_stl_option(b"%z"), Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn check_stl_option_a_realistic_default_like_statusline_is_ok() {
        assert_eq!(check_stl_option(b"%f%m%r%h%w%=%l,%c%V %P"), None);
    }

    #[test]
    fn check_stl_option_minwid_and_maxwid_digits_are_ok() {
        assert_eq!(check_stl_option(b"%3f"), None);
        assert_eq!(check_stl_option(b"%-3.2f"), None);
    }

    #[test]
    fn check_stl_option_user_highlight_digit_flag_is_ok() {
        assert_eq!(check_stl_option(b"%1*text"), None);
    }

    #[test]
    fn check_stl_option_balanced_group_is_ok() {
        assert_eq!(check_stl_option(b"%(text%)"), None);
    }

    #[test]
    fn check_stl_option_a_lone_close_paren_is_unbalanced() {
        assert_eq!(
            check_stl_option(b"%)"),
            Some(crate::gettext_defs::gettext_noop("E542: Unbalanced groups").as_bytes())
        );
    }

    #[test]
    fn check_stl_option_an_unclosed_open_paren_is_unbalanced() {
        assert_eq!(
            check_stl_option(b"%(unclosed"),
            Some(crate::gettext_defs::gettext_noop("E542: Unbalanced groups").as_bytes())
        );
    }

    #[test]
    fn check_stl_option_a_plain_expression_is_ok() {
        assert_eq!(check_stl_option(b"%{expr}"), None);
    }

    #[test]
    fn check_stl_option_a_reevaluating_expression_is_ok() {
        assert_eq!(check_stl_option(b"%{%1+1%}"), None);
    }

    #[test]
    fn check_stl_option_a_reevaluating_expression_immediately_closed_fails() {
        assert_eq!(check_stl_option(b"%{%}"), Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn check_stl_option_an_unclosed_expression_fails() {
        assert_eq!(
            check_stl_option(b"%{expr"),
            Some(crate::gettext_defs::gettext_noop("E540: Unclosed expression sequence").as_bytes())
        );
    }

    #[test]
    fn check_stl_option_an_unclosed_reevaluating_expression_fails() {
        assert_eq!(
            check_stl_option(b"%{%expr"),
            Some(crate::gettext_defs::gettext_noop("E540: Unclosed expression sequence").as_bytes())
        );
    }

    // ---- did_set_iconstring / did_set_titlestring ----

    fn stl_syntax_test(value: &[u8], f: impl FnOnce(&mut crate::option_defs::OptsetT) -> Option<&'static [u8]>) -> i32 {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.stl_syntax;
        unsafe { crate::globals::GLOBALS.get_mut() }.stl_syntax = 0;

        let mut val: Option<Vec<u8>> = Some(value.to_vec());
        let mut args = showbreak_args(&mut val);
        let result = f(&mut args);
        assert_eq!(result, None, "did_set_iconstring/titlestring must always return None");

        let stl_syntax = unsafe { crate::globals::GLOBALS.get_mut() }.stl_syntax;
        unsafe { crate::globals::GLOBALS.get_mut() }.stl_syntax = prev;
        stl_syntax
    }

    #[test]
    fn did_set_iconstring_sets_stl_in_icon_for_valid_statusline_syntax() {
        let stl_syntax = stl_syntax_test(b"%f", |args| unsafe { did_set_iconstring(args) });
        assert_eq!(stl_syntax, crate::globals::STL_IN_ICON);
    }

    #[test]
    fn did_set_iconstring_clears_stl_in_icon_for_plain_text() {
        let stl_syntax = stl_syntax_test(b"just plain text", |args| unsafe { did_set_iconstring(args) });
        assert_eq!(stl_syntax, 0);
    }

    #[test]
    fn did_set_iconstring_clears_stl_in_icon_for_invalid_statusline_syntax() {
        // Contains a '%', but check_stl_option itself would reject it -
        // the bit is cleared, but did_set_iconstring's own return
        // value is still None (this function never reports an error;
        // 'iconstring' need not look like statusline syntax at all).
        let stl_syntax = stl_syntax_test(b"%z", |args| unsafe { did_set_iconstring(args) });
        assert_eq!(stl_syntax, 0);
    }

    #[test]
    fn did_set_titlestring_sets_stl_in_title_for_valid_statusline_syntax() {
        let stl_syntax = stl_syntax_test(b"%f", |args| unsafe { did_set_titlestring(args) });
        assert_eq!(stl_syntax, crate::globals::STL_IN_TITLE);
    }

    #[test]
    fn did_set_titlestring_clears_stl_in_title_for_plain_text() {
        let stl_syntax = stl_syntax_test(b"just plain text", |args| unsafe { did_set_titlestring(args) });
        assert_eq!(stl_syntax, 0);
    }

    #[test]
    fn did_set_iconstring_and_did_set_titlestring_use_independent_bits() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.stl_syntax;
        unsafe { crate::globals::GLOBALS.get_mut() }.stl_syntax = 0;

        let mut icon_val: Option<Vec<u8>> = Some(b"%f".to_vec());
        let mut icon_args = showbreak_args(&mut icon_val);
        assert_eq!(unsafe { did_set_iconstring(&mut icon_args) }, None);

        // Setting 'titlestring' to plain text must not clear the
        // already-set STL_IN_ICON bit - each option only ever touches
        // its own bit.
        let mut title_val: Option<Vec<u8>> = Some(b"plain".to_vec());
        let mut title_args = showbreak_args(&mut title_val);
        assert_eq!(unsafe { did_set_titlestring(&mut title_args) }, None);

        let stl_syntax = unsafe { crate::globals::GLOBALS.get_mut() }.stl_syntax;
        unsafe { crate::globals::GLOBALS.get_mut() }.stl_syntax = prev;
        assert_eq!(stl_syntax, crate::globals::STL_IN_ICON);
    }

    // ---- did_set_varsofttabstop / did_set_vartabstop ----

    fn vartabstop_args(
        buf: &mut crate::buffer_defs::BufT,
        win: &mut crate::buffer_defs::WinT,
        val: &mut Option<Vec<u8>>,
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_buf: buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        }
    }

    #[test]
    fn did_set_varsofttabstop_empty_clears_the_array() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_vsts_array: Some(vec![4, 8]),
            ..Default::default()
        };
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = vartabstop_args(&mut buf, &mut win, &mut val);
        assert_eq!(unsafe { did_set_varsofttabstop(&mut args) }, None);
        assert_eq!(buf.b_p_vsts_array, None);
    }

    #[test]
    fn did_set_varsofttabstop_valid_list_sets_the_array() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"4,8,12".to_vec());
        let mut args = vartabstop_args(&mut buf, &mut win, &mut val);
        assert_eq!(unsafe { did_set_varsofttabstop(&mut args) }, None);
        assert_eq!(buf.b_p_vsts_array, Some(vec![4, 8, 12]));
    }

    #[test]
    fn did_set_varsofttabstop_invalid_value_fails_and_leaves_the_array_untouched() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_vsts_array: Some(vec![4]),
            ..Default::default()
        };
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"4,bogus".to_vec());
        let mut args = vartabstop_args(&mut buf, &mut win, &mut val);
        assert_eq!(
            unsafe { did_set_varsofttabstop(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
        // Matches the original: a failed tabstop_set call never
        // touches the buffer's own array at all.
        assert_eq!(buf.b_p_vsts_array, Some(vec![4]));
    }

    #[test]
    fn did_set_vartabstop_valid_list_sets_the_array_without_fold_update() {
        let mut buf = crate::buffer_defs::BufT::default();
        // Default 'foldmethod' ("manual") means foldmethod_is_indent
        // is false, so the unimplemented!() fold-update branch is
        // never reached.
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"4,8".to_vec());
        let mut args = vartabstop_args(&mut buf, &mut win, &mut val);
        assert_eq!(unsafe { did_set_vartabstop(&mut args) }, None);
        assert_eq!(buf.b_p_vts_array, Some(vec![4, 8]));
    }

    #[test]
    fn did_set_vartabstop_invalid_value_fails() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"0,1".to_vec());
        let mut args = vartabstop_args(&mut buf, &mut win, &mut val);
        assert_eq!(unsafe { did_set_vartabstop(&mut args) }, Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    #[should_panic(expected = "foldUpdateAll")]
    fn did_set_vartabstop_panics_when_foldmethod_is_indent() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_fdm = Some(b"indent".to_vec());
        let mut val: Option<Vec<u8>> = Some(b"4,8".to_vec());
        let mut args = vartabstop_args(&mut buf, &mut win, &mut val);
        let _ = unsafe { did_set_vartabstop(&mut args) };
    }
}
