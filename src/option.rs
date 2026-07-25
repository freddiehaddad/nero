//! Translated from `src/nvim/option.c` (tractable core only).
//!
//! `option.c` is a massive (~6897-line) file implementing the entire
//! `:set`/options-parsing engine, deeply entangled with the eval
//! engine, autocmd triggers, and nearly every other subsystem: the
//! `:set` command parser (`do_set`/`ex_set`) and the typed
//! `get_option_value`/`set_option_value` entry points are not
//! attempted here.
//!
//! Translated: `get_fileformat` (harvested first because it directly
//! unblocks part of `memline.c`'s `ml_open`); `get_fileformat_force`
//! (now tractable now that `crate::ex_cmds_defs::ExargT` exists); and a
//! batch of small, genuinely standalone option-value accessors that
//! read already-translated `option_vars.rs`/`buffer_defs.rs`/
//! `globals.rs` fields directly, without needing the options table at
//! all: `magic_isset`, `shortmess`, `can_bs`, `get_bkc_flags`,
//! `get_flp_value`, `get_ve_flags`, `get_showbreak_value`,
//! `default_fileformat`, `csh_like_shell`, `fish_like_shell`,
//! `get_scrolloff_value`, `get_scrolloffpad_value`,
//! `get_sidescrolloff_value`, `valid_name`, `check_blending`,
//! `fill_culopt_flags` (parses `'cursorlineopt'`'s comma-separated
//! flag list into `WinT.w_p_culopt_flags` - needed only
//! `option_vars.rs`'s already-real `opt_culopt_flag` bit-flag module
//! and `buffer_defs.rs`'s already-real `WinT.w_onebuf_opt.wo_culopt`;
//! preserves a real, faithfully-replicated parsing quirk rather than
//! "fixing" it - see its own doc comment).
//! `can_bs`/`shortmess`/`valid_name` needed `strings.c`'s `vim_strchr`
//! (also translated this pass - re-examined and found NOT actually
//! blocked on `g_chartab`/`option.c` as an earlier note claimed, see
//! `strings.rs`'s own module doc).
//!
//! Now also translated, now that `option_defs.rs`'s [`OPTIONS`] table
//! is real: `is_option_hidden`/`option_has_type`/`option_has_scope`/
//! `option_is_global_local`/`option_is_global_only`/
//! `option_is_window_local` (small boolean predicates over a single
//! entry's `scope_flags`/`immutable`/`var`/`type`), `get_option`
//! (index -> `&'static VimoptionT`, via `OPTIONS.as_ptr()` - never
//! `.get_mut()`, so the returned reference stays valid across any
//! later, unrelated `OPTIONS`/`OPTION_VARS` access elsewhere, matching
//! `eval/vars.rs`'s `vimvar_ptr` precedent), and the big one:
//! `get_varp_from`/`get_varp` (the ~145-branch "resolve an option's
//! effective storage address for THIS buffer/window" engine - every
//! branch mechanically transcribed via a throwaway Python parser
//! script against a captured copy of the real function body, cross-
//! checked field-by-field against `BufT`/`WinT`/`WinoptT`/
//! `SynblockT`'s real Rust field names and types before trusting the
//! generated code, same methodology as `option_defs.rs`'s own
//! `OPTIONS` table). `get_varp_scope_from`/`get_varp_scope`/
//! `get_option_varp_scope_from` (the `:setlocal`/`:setglobal`-specific
//! wrapper, needing its OWN separate ~34-branch "force the local
//! value" table plus `GLOBAL_WO`'s pointer-arithmetic trick - unsound
//! to replicate directly in Rust, since it computes a sibling
//! `WinT.w_allbuf_opt` field's address via byte-offset arithmetic from
//! a `w_onebuf_opt`-derived pointer, which has no provenance over
//! `w_allbuf_opt`'s separate allocation) are deliberately NOT included
//! in this pass - a well-defined, self-contained follow-up, needed
//! only once `do_set`'s `:setlocal`/`:setglobal` scope flags actually
//! exist as callers.
//!
//! **No Rust equivalent needed** (not "deferred" - genuinely
//! unnecessary): `optval_free`/`optval_copy`/`optval_equal`. These
//! exist in the original purely to manually manage `OptVal`'s C union
//! (freeing/duplicating/comparing the `String` case's heap buffer by
//! hand). `option_defs.rs`'s `OptVal` is already a safe tagged Rust
//! `enum` with `#[derive(Debug, Clone, PartialEq)]` - so plain
//! `drop(val)`/`val.clone()`/`val1 == val2` already do exactly what
//! these three functions do, automatically, for free.
//!
//! Established convention (this file's first real readers of
//! `Option<Vec<u8>>` *option string values*, as opposed to freshly
//! produced/NUL-scanned output buffers elsewhere in this crate):
//! **these fields carry NO trailing NUL byte** - the `Vec`'s own
//! `.len()` is authoritative, matching `get_fileformat`'s own
//! pre-existing test data (`b_p_ff: Some("unix".as_bytes().to_vec())`,
//! no NUL). This is deliberately different from this crate's `Vec<u8>`-
//! includes-its-own-trailing-NUL convention used for line storage
//! (`memline.rs`) and freshly-copied/NUL-scanned string outputs
//! (`strings.rs`'s `vim_strup`/`mb_strup_buf`/`strcase_save`) - those
//! mirror a real heap-allocated `char *` the original explicitly
//! NUL-terminates itself; a persistent *option value*, once stored as
//! an exact-length Rust `Vec<u8>`, has no such need (a redundant
//! trailing NUL only invites bugs like direct content comparisons
//! - e.g. `get_showbreak_value`'s `"NONE"` check - silently failing).
//!
//! Deferred: everything else, including `was_set_insecurely`/
//! `insecure_flag` (still needs `OPTIONS[idx].flags`' own
//! `WAS_SET`/`INSECURE` bits threaded through a real `:set` call site
//! to have any meaningful test value), `parse_winhl_opt` (needs the
//! decoration/highlight-group subsystem: `nvim_create_namespace`/
//! `get_decor_provider`/`syn_check_group`/`ns_hl_def`), and `do_set`/
//! `ex_set`'s command-line parsing plus `get_option_value`/
//! `set_option_value`/`option_was_set`/`get_winbuf_options`/
//! `get_vimoption`/etc. (everything that needs the full parsed-`:set`-
//! argument machinery, not just a resolved storage address).

use crate::buffer_defs::{BufT, WinT};
use crate::option_defs::{OptIndex, OptScope, OptValType, VimoptionT, OPTIONS};
use crate::option_vars::{EOL_DOS, EOL_MAC, EOL_UNIX};
use crate::types_defs::OptInt;
use std::ffi::c_void;

/// Gets the `'fileformat'` of `buf` as an `EOL_*` constant
/// (`get_fileformat`).
#[must_use]
pub fn get_fileformat(buf: &BufT) -> i32 {
    let c = buf
        .b_p_ff
        .as_deref()
        .and_then(|s| s.first())
        .copied()
        .unwrap_or(0);

    if buf.b_p_bin != 0 || c == b'u' {
        return EOL_UNIX;
    }
    if c == b'm' {
        return EOL_MAC;
    }
    EOL_DOS
}

/// Like [`get_fileformat`], but override `'fileformat'` with the
/// `++opt=val` argument's forced value, if given (`get_fileformat_force`).
///
/// `eap` can be `None` (matching the original's own "`eap` can be
/// NULL!" doc comment) - now tractable now that
/// `crate::ex_cmds_defs::ExargT` exists.
#[must_use]
pub fn get_fileformat_force(buf: &BufT, eap: Option<&crate::ex_cmds_defs::ExargT>) -> i32 {
    let c: u8 = if eap.is_some_and(|e| e.force_ff != 0) {
        eap.unwrap().force_ff
    } else {
        let forced_bin = match eap {
            Some(e) if e.force_bin != 0 => e.force_bin == crate::ex_cmds_defs::FORCE_BIN,
            _ => buf.b_p_bin != 0,
        };
        if forced_bin {
            return EOL_UNIX;
        }
        buf.b_p_ff.as_deref().and_then(|s| s.first()).copied().unwrap_or(0)
    };

    if c == b'u' {
        return EOL_UNIX;
    }
    if c == b'm' {
        return EOL_MAC;
    }
    EOL_DOS
}

/// Get the value of `'magic'` taking `magic_overruled` into account
/// (`magic_isset`).
#[must_use]
pub fn magic_isset() -> bool {
    match unsafe { crate::globals::GLOBALS.get_mut() }.magic_overruled {
        crate::regexp_defs::OptmagicT::MagicOn => true,
        crate::regexp_defs::OptmagicT::MagicOff => false,
        crate::regexp_defs::OptmagicT::NotSet => {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_magic != 0
        }
    }
}

/// @return true if `x` is present in `'shortmess'` option, or
/// `'shortmess'` contains `'a'` and `x` is present in
/// `SHM_ALL_ABBREVIATIONS` (`shortmess`).
#[must_use]
pub fn shortmess(x: u8) -> bool {
    let p_shm = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm.clone();
    let Some(p_shm) = p_shm else {
        return false;
    };

    crate::strings::vim_strchr(&p_shm, i32::from(x)).is_some()
        || (crate::strings::vim_strchr(&p_shm, i32::from(b'a')).is_some()
            && crate::strings::vim_strchr(&crate::option_vars::SHM_ALL_ABBREVIATIONS, i32::from(x))
                .is_some())
}

/// Check if backspacing over something is allowed (`can_bs`).
///
/// @param what one of [`crate::option_vars::BS_INDENT`]/
/// [`crate::option_vars::BS_EOL`]/[`crate::option_vars::BS_START`]/
/// [`crate::option_vars::BS_NOSTOP`].
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` (matches every other `curbuf`-touching function in
/// this crate).
#[must_use]
pub unsafe fn can_bs(what: u8) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    if what == crate::option_vars::BS_START && crate::buffer::bt_prompt(Some(curbuf)) {
        return false;
    }

    let p_bs = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_bs
        .clone()
        .unwrap_or_default();

    // support for number values was removed but we keep '2' since it
    // is used in legacy tests
    if p_bs.first() == Some(&b'2') {
        return what != crate::option_vars::BS_NOSTOP;
    }

    crate::strings::vim_strchr(&p_bs, i32::from(what)).is_some()
}

/// Get the local or global value of `'backupcopy'` flags
/// (`get_bkc_flags`).
#[must_use]
pub fn get_bkc_flags(buf: &BufT) -> u32 {
    if buf.b_bkc_flags != 0 {
        buf.b_bkc_flags
    } else {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.bkc_flags
    }
}

/// Get the local or global value of `'formatlistpat'` (`get_flp_value`).
#[must_use]
pub fn get_flp_value(buf: &BufT) -> Vec<u8> {
    match buf.b_p_flp.as_deref() {
        Some(flp) if !flp.is_empty() => flp.to_vec(),
        _ => unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_flp
            .clone()
            .unwrap_or_default(),
    }
}

/// Get the value of `'equalprg'`, either the buffer-local one or the
/// global one (`get_equalprg`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn get_equalprg() -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    match curbuf.b_p_ep.as_deref() {
        Some(ep) if !ep.is_empty() => ep.to_vec(),
        _ => unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_ep
            .clone()
            .unwrap_or_default(),
    }
}

/// Get the value of `'findfunc'`, either the buffer-local one or the
/// global one (`get_findfunc`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn get_findfunc() -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    match curbuf.b_p_ffu.as_deref() {
        Some(ffu) if !ffu.is_empty() => ffu.to_vec(),
        _ => unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_ffu
            .clone()
            .unwrap_or_default(),
    }
}

/// Get the local or global value of `'virtualedit'` flags
/// (`get_ve_flags`).
#[must_use]
pub fn get_ve_flags(wp: &WinT) -> u32 {
    let flags = if wp.w_onebuf_opt.wo_ve_flags != 0 {
        wp.w_onebuf_opt.wo_ve_flags
    } else {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags
    };
    flags & !(crate::option_vars::opt_ve_flag::NONE | crate::option_vars::opt_ve_flag::NONE_U)
}

/// Get the local or global value of `'showbreak'` (`get_showbreak_value`).
///
/// Deviates from the original's `char *` return (always non-NULL,
/// backed by the `empty_string_option` sentinel for "nothing") by
/// returning an owned `Vec<u8>` directly (empty when there's nothing to
/// show) - see `option_vars.rs`'s own module doc for why
/// `empty_string_option` itself needs no Rust equivalent.
#[must_use]
pub fn get_showbreak_value(win: &WinT) -> Vec<u8> {
    match win.w_onebuf_opt.wo_sbr.as_deref() {
        Some(sbr) if !sbr.is_empty() => {
            if sbr == b"NONE" {
                Vec::new()
            } else {
                sbr.to_vec()
            }
        }
        _ => unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_sbr
            .clone()
            .unwrap_or_default(),
    }
}

/// Return the default fileformat from `'fileformats'`
/// (`default_fileformat`).
#[must_use]
pub fn default_fileformat() -> i32 {
    let p_ffs = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ffs.clone();
    match p_ffs.as_deref().and_then(|s| s.first()) {
        Some(b'm') => EOL_MAC,
        Some(b'd') => EOL_DOS,
        _ => EOL_UNIX,
    }
}

/// Returns whether `haystack` contains `needle` anywhere (a `strstr`-
/// equivalent boolean check, used by [`csh_like_shell`]/
/// [`fish_like_shell`] below - the original itself has no shared helper
/// for this, it's purely a Rust-side convenience).
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Return true when `'shell'` has "csh" in the tail (`csh_like_shell`).
#[must_use]
pub fn csh_like_shell() -> bool {
    let p_sh = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sh.clone().unwrap_or_default();
    let tail_start = crate::path::path_tail(&p_sh);
    contains(&p_sh[tail_start..], b"csh")
}

/// Return true when `'shell'` has "fish" in the tail (`fish_like_shell`).
#[must_use]
pub fn fish_like_shell() -> bool {
    let p_sh = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sh.clone().unwrap_or_default();
    let tail_start = crate::path::path_tail(&p_sh);
    contains(&p_sh[tail_start..], b"fish")
}

/// Check that every character in `val` is either alphanumeric or present
/// in `allowed` (`valid_name`).
#[must_use]
pub fn valid_name(val: &[u8], allowed: &[u8]) -> bool {
    let end = val.iter().position(|&b| b == 0).unwrap_or(val.len());
    for &b in &val[..end] {
        if !crate::macros_defs::ascii_isalnum(i32::from(b))
            && crate::strings::vim_strchr(allowed, i32::from(b)).is_none()
        {
            return false;
        }
    }
    true
}

/// Update `wp.w_grid_alloc.blending` from `'winblend'`/the floating
/// window's shadow setting (`check_blending`).
pub fn check_blending(wp: &mut WinT) {
    wp.w_grid_alloc.blending = wp.w_onebuf_opt.wo_winbl > 0 || (wp.w_floating && wp.w_config.shadow);
}

/// Return the effective `'scrolloff'` value for the current window,
/// using the global value when appropriate (`get_scrolloff_value`).
///
/// # Safety
/// `wp.w_buffer` must be a valid, non-null pointer to a live `BufT`.
#[must_use]
pub unsafe fn get_scrolloff_value(wp: &WinT) -> OptInt {
    // Disallow scrolloff in terminal-mode. #11915
    // Still allow 'scrolloff' for non-terminal buffers. #34447
    let state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
    // SAFETY: forwarded from this function's own safety doc.
    let is_terminal_buf = !unsafe { &*wp.w_buffer }.terminal.is_null();
    if (state as u32 & crate::state_defs::mode::TERMINAL) != 0 && is_terminal_buf {
        return 0;
    }
    if wp.w_onebuf_opt.wo_so < 0 {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_so
    } else {
        wp.w_onebuf_opt.wo_so
    }
}

/// Return the effective `'scrolloffpad'` value for the current window,
/// using the global value when appropriate (`get_scrolloffpad_value`).
///
/// Note: the original's own `else` branch reads `curwin->w_p_sop`, NOT
/// `wp->w_p_sop` - preserved exactly as-is (every real caller always
/// passes `curwin` itself, so this divergence is unobservable in
/// practice; this project translates the original faithfully rather
/// than "fixing" perceived upstream inconsistencies).
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT`.
#[must_use]
pub unsafe fn get_scrolloffpad_value(wp: &WinT) -> OptInt {
    if wp.w_onebuf_opt.wo_sop == -1 {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sop
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_onebuf_opt.wo_sop
    }
}

/// Return the effective `'sidescrolloff'` value for the current window,
/// using the global value when appropriate (`get_sidescrolloff_value`).
#[must_use]
pub fn get_sidescrolloff_value(wp: &WinT) -> OptInt {
    if wp.w_onebuf_opt.wo_siso < 0 {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_siso
    } else {
        wp.w_onebuf_opt.wo_siso
    }
}

/// Parse `val` (or `wp.w_onebuf_opt.wo_culopt` when `val` is `None`) -
/// `'cursorlineopt'`'s comma-separated flag list (`"line"`/`"both"`/
/// `"number"`/`"screenline"`) - into `wp.w_p_culopt_flags`
/// (`fill_culopt_flags`).
///
/// Returns `OK`/`FAIL`. Preserves a real, faithfully-replicated quirk
/// rather than "fixing" it: an unrecognized token leaves the parse
/// position unchanged, so if that position happens to already be `,`
/// (e.g. a leading or doubled comma, `",line"`/`"line,,number"`), the
/// original's own `*p != ',' && *p != NUL` guard does NOT reject it -
/// it's silently skipped as an empty entry, exactly like this
/// translation's `match` arm falling through to advance past a `,` at
/// the SAME position no token was recognized at.
#[must_use]
pub fn fill_culopt_flags(val: Option<&[u8]>, wp: &mut WinT) -> i32 {
    let owned;
    let p: &[u8] = match val {
        Some(v) => v,
        None => {
            owned = wp.w_onebuf_opt.wo_culopt.clone().unwrap_or_default();
            &owned
        }
    };

    let mut flags_new: u8 = 0;
    let mut i = 0;
    while i < p.len() {
        // Note: Keep this in sync with `opt_culopt_values`.
        if p[i..].starts_with(b"line") {
            i += 4;
            flags_new |= crate::option_vars::opt_culopt_flag::LINE as u8;
        } else if p[i..].starts_with(b"both") {
            i += 4;
            flags_new |= (crate::option_vars::opt_culopt_flag::LINE
                | crate::option_vars::opt_culopt_flag::NUMBER) as u8;
        } else if p[i..].starts_with(b"number") {
            i += 6;
            flags_new |= crate::option_vars::opt_culopt_flag::NUMBER as u8;
        } else if p[i..].starts_with(b"screenline") {
            i += 10;
            flags_new |= crate::option_vars::opt_culopt_flag::SCREENLINE as u8;
        }

        match p.get(i) {
            Some(&b',') => i += 1,
            Some(_) => return crate::vim_defs::FAIL,
            None => {}
        }
    }

    // Can't have both "line" and "screenline".
    let line = crate::option_vars::opt_culopt_flag::LINE as u8;
    let screenline = crate::option_vars::opt_culopt_flag::SCREENLINE as u8;
    if flags_new & line != 0 && flags_new & screenline != 0 {
        return crate::vim_defs::FAIL;
    }
    wp.w_p_culopt_flags = flags_new;

    crate::vim_defs::OK
}


/// Hidden options are always immutable and point to their default
/// value (`is_option_hidden`). In this crate's own encoding (see
/// `option_defs.rs`'s own `OPTIONS` doc comment), the original's
/// self-referential `var == &options[opt_idx].def_val.data` check
/// becomes simply `var.is_null()` - verified 1:1 equivalent to
/// `immutable == true` for every entry when `OPTIONS` was built.
#[must_use]
pub fn is_option_hidden(opt_idx: OptIndex) -> bool {
    if opt_idx == OptIndex::Invalid {
        return false;
    }
    let p = get_option(opt_idx);
    p.immutable && p.var.is_null()
}

/// Check if option supports a specific type (`option_has_type`).
#[must_use]
pub fn option_has_type(opt_idx: OptIndex, typ: OptValType) -> bool {
    opt_idx != OptIndex::Invalid && get_option(opt_idx).r#type == typ
}

/// Check if option supports a specific scope (`option_has_scope`).
#[must_use]
pub fn option_has_scope(opt_idx: OptIndex, scope: OptScope) -> bool {
    get_option(opt_idx).scope_flags & (1 << (scope as u8)) != 0
}

/// Check if option is global-local (has global AND buffer/window
/// scope). Tab scope is independent and does not make an option
/// "global-local" (`option_is_global_local`).
#[must_use]
pub fn option_is_global_local(opt_idx: OptIndex) -> bool {
    if opt_idx == OptIndex::Invalid {
        return false;
    }
    let bw = (1 << (OptScope::Buf as u8)) | (1 << (OptScope::Win as u8));
    (get_option(opt_idx).scope_flags & bw) != 0 && option_has_scope(opt_idx, OptScope::Global)
}

/// Check if option only supports global scope, ignoring tab scope
/// (which is independent) (`option_is_global_only`).
#[must_use]
pub fn option_is_global_only(opt_idx: OptIndex) -> bool {
    if opt_idx == OptIndex::Invalid {
        return false;
    }
    let bw = (1 << (OptScope::Buf as u8)) | (1 << (OptScope::Win as u8));
    (get_option(opt_idx).scope_flags & bw) == 0 && option_has_scope(opt_idx, OptScope::Global)
}

/// Check if option only supports window scope, ignoring tab scope
/// (which is independent) (`option_is_window_local`).
#[must_use]
pub fn option_is_window_local(opt_idx: OptIndex) -> bool {
    if opt_idx == OptIndex::Invalid {
        return false;
    }
    let exclude = (1 << (OptScope::Global as u8)) | (1 << (OptScope::Buf as u8));
    (get_option(opt_idx).scope_flags & exclude) == 0 && option_has_scope(opt_idx, OptScope::Win)
}

/// Get a raw pointer to `OPTIONS`'s element at `opt_idx`. Built on
/// `GlobalCell::as_ptr()` (never `.get_mut()`), matching
/// `eval/vars.rs`'s `vimvar_ptr`/`vimvar_ptr_at` precedent exactly:
/// `as_ptr()` never creates an intermediate reference, so pointers
/// (and the `&'static VimoptionT` references `get_option` hands out
/// below) derived this way stay valid across any later, unrelated
/// `OPTIONS`/`OPTION_VARS` access elsewhere in this crate - unlike a
/// `.get_mut()[idx]`-derived reference, which `GlobalCell::get_mut`'s
/// own doc comment already warns is invalidated by the very next call
/// to `.get_mut()` (the exact root cause fixed for `VIMVARDICT`
/// earlier this session).
fn opt_ptr(opt_idx: OptIndex) -> *mut VimoptionT {
    let base = OPTIONS.as_ptr() as *mut VimoptionT;
    // SAFETY: OPTIONS's own LazyLock ensures its fixed OPT_COUNT-entry
    // array is fully populated before this pointer is ever
    // dereferenced elsewhere, and every `OptIndex` variant (other than
    // `Invalid`) is a valid 0..OPT_COUNT index into it by construction.
    unsafe { base.add(opt_idx as usize) }
}

/// Return information for the option at `opt_idx` (`get_option`).
///
/// # Panics
/// Debug-asserts `opt_idx != OptIndex::Invalid`, matching the
/// original's own `assert(opt_idx != kOptInvalid)`.
#[must_use]
pub fn get_option(opt_idx: OptIndex) -> &'static VimoptionT {
    debug_assert!(opt_idx != OptIndex::Invalid);
    // SAFETY: opt_ptr's own safety reasoning; VimoptionT itself is
    // never mutated anywhere in this crate once OPTIONS is built, so a
    // shared reference derived this way is sound to hold indefinitely.
    unsafe { &*opt_ptr(opt_idx) }
}

/// Get pointer to option variable, given the option and the buffer/
/// window it should be resolved against (`get_varp_from`). Every
/// branch below mirrors the original's own big `switch` exactly - see
/// this file's own module doc comment for the transcription
/// methodology.
///
/// # Safety
/// `buf`/`win` must be valid, non-null pointers to live `BufT`/`WinT`
/// values (a purely buffer-scoped option never dereferences `win`, and
/// a purely window-scoped option never dereferences `buf`, but the
/// original itself has no way to know which in advance either, hence
/// both are always required).
#[must_use]
pub unsafe fn get_varp_from(opt_idx: OptIndex, buf: *mut BufT, win: *mut WinT) -> *mut c_void {
    let p = get_option(opt_idx);

    // Hidden options and global-only options always use the same var pointer.
    if is_option_hidden(opt_idx) || option_is_global_only(opt_idx) {
        return p.var;
    }

    match opt_idx {
        OptIndex::Equalprg => {
            if (*buf).b_p_ep.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_ep) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Keywordprg => {
            if (*buf).b_p_kp.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_kp) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Path => {
            if (*buf).b_p_path.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_path) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Autocomplete => {
            if (*buf).b_p_ac >= 0 {
                std::ptr::addr_of_mut!((*buf).b_p_ac) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Autoread => {
            if (*buf).b_p_ar >= 0 {
                std::ptr::addr_of_mut!((*buf).b_p_ar) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Tags => {
            if (*buf).b_p_tags.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_tags) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Tagcase => {
            if (*buf).b_p_tc.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_tc) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Sidescrolloff => {
            if (*win).w_onebuf_opt.wo_siso >= 0 {
                std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_siso) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Scrolloff => {
            if (*win).w_onebuf_opt.wo_so >= 0 {
                std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_so) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Scrolloffpad => {
            if (*win).w_onebuf_opt.wo_sop >= 0 {
                std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_sop) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Backupcopy => {
            if (*buf).b_p_bkc.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_bkc) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Define => {
            if (*buf).b_p_def.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_def) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Include => {
            if (*buf).b_p_inc.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_inc) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Completeopt => {
            if (*buf).b_p_cot.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_cot) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Dictionary => {
            if (*buf).b_p_dict.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_dict) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Diffanchors => {
            if (*buf).b_p_dia.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_dia) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Thesaurus => {
            if (*buf).b_p_tsr.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_tsr) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Thesaurusfunc => {
            if (*buf).b_p_tsrfu.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_tsrfu) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Formatprg => {
            if (*buf).b_p_fp.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_fp) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Fsync => {
            if (*buf).b_p_fs >= 0 {
                std::ptr::addr_of_mut!((*buf).b_p_fs) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Findfunc => {
            if (*buf).b_p_ffu.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_ffu) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Errorformat => {
            if (*buf).b_p_efm.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_efm) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Grepformat => {
            if (*buf).b_p_gefm.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_gefm) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Grepprg => {
            if (*buf).b_p_gp.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_gp) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Makeprg => {
            if (*buf).b_p_mp.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_mp) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Showbreak => {
            if (*win).w_onebuf_opt.wo_sbr.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_sbr) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Statusline => {
            if (*win).w_onebuf_opt.wo_stl.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_stl) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Winbar => {
            if (*win).w_onebuf_opt.wo_wbr.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_wbr) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Undolevels => {
            if (*buf).b_p_ul != crate::option_vars::NO_LOCAL_UNDOLEVEL {
                std::ptr::addr_of_mut!((*buf).b_p_ul) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Lispwords => {
            if (*buf).b_p_lw.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_lw) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Makeencoding => {
            if (*buf).b_p_menc.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*buf).b_p_menc) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Fillchars => {
            if (*win).w_onebuf_opt.wo_fcs.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fcs) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Listchars => {
            if (*win).w_onebuf_opt.wo_lcs.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_lcs) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Virtualedit => {
            if (*win).w_onebuf_opt.wo_ve.as_deref().is_some_and(|s| !s.is_empty()) {
                std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_ve) as *mut c_void
            } else {
                p.var
            }
        }
        OptIndex::Arabic => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_arab) as *mut c_void,
        OptIndex::List => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_list) as *mut c_void,
        OptIndex::Spell => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_spell) as *mut c_void,
        OptIndex::Cursorcolumn => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_cuc) as *mut c_void,
        OptIndex::Cursorline => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_cul) as *mut c_void,
        OptIndex::Cursorlineopt => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_culopt) as *mut c_void,
        OptIndex::Colorcolumn => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_cc) as *mut c_void,
        OptIndex::Diff => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_diff) as *mut c_void,
        OptIndex::Eventignorewin => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_eiw) as *mut c_void,
        OptIndex::Foldcolumn => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fdc) as *mut c_void,
        OptIndex::Foldenable => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fen) as *mut c_void,
        OptIndex::Foldignore => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fdi) as *mut c_void,
        OptIndex::Foldlevel => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fdl) as *mut c_void,
        OptIndex::Foldmethod => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fdm) as *mut c_void,
        OptIndex::Foldminlines => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fml) as *mut c_void,
        OptIndex::Foldnestmax => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fdn) as *mut c_void,
        OptIndex::Foldexpr => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fde) as *mut c_void,
        OptIndex::Foldtext => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fdt) as *mut c_void,
        OptIndex::Foldmarker => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fmr) as *mut c_void,
        OptIndex::Number => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_nu) as *mut c_void,
        OptIndex::Relativenumber => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_rnu) as *mut c_void,
        OptIndex::Numberwidth => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_nuw) as *mut c_void,
        OptIndex::Winfixbuf => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_wfb) as *mut c_void,
        OptIndex::Winfixheight => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_wfh) as *mut c_void,
        OptIndex::Winfixwidth => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_wfw) as *mut c_void,
        OptIndex::Winpinned => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_wp) as *mut c_void,
        OptIndex::Previewwindow => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_pvw) as *mut c_void,
        OptIndex::Lhistory => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_lhi) as *mut c_void,
        OptIndex::Rightleft => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_rl) as *mut c_void,
        OptIndex::Rightleftcmd => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_rlc) as *mut c_void,
        OptIndex::Scroll => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_scr) as *mut c_void,
        OptIndex::Smoothscroll => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_sms) as *mut c_void,
        OptIndex::Wrap => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_wrap) as *mut c_void,
        OptIndex::Linebreak => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_lbr) as *mut c_void,
        OptIndex::Breakindent => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_bri) as *mut c_void,
        OptIndex::Breakindentopt => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_briopt) as *mut c_void,
        OptIndex::Scrollbind => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_scb) as *mut c_void,
        OptIndex::Cursorbind => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_crb) as *mut c_void,
        OptIndex::Concealcursor => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_cocu) as *mut c_void,
        OptIndex::Conceallevel => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_cole) as *mut c_void,
        OptIndex::Autoindent => std::ptr::addr_of_mut!((*buf).b_p_ai) as *mut c_void,
        OptIndex::Binary => std::ptr::addr_of_mut!((*buf).b_p_bin) as *mut c_void,
        OptIndex::Bomb => std::ptr::addr_of_mut!((*buf).b_p_bomb) as *mut c_void,
        OptIndex::Bufhidden => std::ptr::addr_of_mut!((*buf).b_p_bh) as *mut c_void,
        OptIndex::Buftype => std::ptr::addr_of_mut!((*buf).b_p_bt) as *mut c_void,
        OptIndex::Buflisted => std::ptr::addr_of_mut!((*buf).b_p_bl) as *mut c_void,
        OptIndex::Busy => std::ptr::addr_of_mut!((*buf).b_p_busy) as *mut c_void,
        OptIndex::Channel => std::ptr::addr_of_mut!((*buf).b_p_channel) as *mut c_void,
        OptIndex::Copyindent => std::ptr::addr_of_mut!((*buf).b_p_ci) as *mut c_void,
        OptIndex::Cindent => std::ptr::addr_of_mut!((*buf).b_p_cin) as *mut c_void,
        OptIndex::Cinkeys => std::ptr::addr_of_mut!((*buf).b_p_cink) as *mut c_void,
        OptIndex::Cinoptions => std::ptr::addr_of_mut!((*buf).b_p_cino) as *mut c_void,
        OptIndex::Cinscopedecls => std::ptr::addr_of_mut!((*buf).b_p_cinsd) as *mut c_void,
        OptIndex::Cinwords => std::ptr::addr_of_mut!((*buf).b_p_cinw) as *mut c_void,
        OptIndex::Comments => std::ptr::addr_of_mut!((*buf).b_p_com) as *mut c_void,
        OptIndex::Commentstring => std::ptr::addr_of_mut!((*buf).b_p_cms) as *mut c_void,
        OptIndex::Complete => std::ptr::addr_of_mut!((*buf).b_p_cpt) as *mut c_void,
        #[cfg(windows)]
        OptIndex::Completeslash => std::ptr::addr_of_mut!((*buf).b_p_csl) as *mut c_void,
        OptIndex::Completefunc => std::ptr::addr_of_mut!((*buf).b_p_cfu) as *mut c_void,
        OptIndex::Omnifunc => std::ptr::addr_of_mut!((*buf).b_p_ofu) as *mut c_void,
        OptIndex::Endoffile => std::ptr::addr_of_mut!((*buf).b_p_eof) as *mut c_void,
        OptIndex::Endofline => std::ptr::addr_of_mut!((*buf).b_p_eol) as *mut c_void,
        OptIndex::Fixendofline => std::ptr::addr_of_mut!((*buf).b_p_fixeol) as *mut c_void,
        OptIndex::Expandtab => std::ptr::addr_of_mut!((*buf).b_p_et) as *mut c_void,
        OptIndex::Fileencoding => std::ptr::addr_of_mut!((*buf).b_p_fenc) as *mut c_void,
        OptIndex::Fileformat => std::ptr::addr_of_mut!((*buf).b_p_ff) as *mut c_void,
        OptIndex::Filetype => std::ptr::addr_of_mut!((*buf).b_p_ft) as *mut c_void,
        OptIndex::Formatoptions => std::ptr::addr_of_mut!((*buf).b_p_fo) as *mut c_void,
        OptIndex::Formatlistpat => std::ptr::addr_of_mut!((*buf).b_p_flp) as *mut c_void,
        OptIndex::Iminsert => std::ptr::addr_of_mut!((*buf).b_p_iminsert) as *mut c_void,
        OptIndex::Imsearch => std::ptr::addr_of_mut!((*buf).b_p_imsearch) as *mut c_void,
        OptIndex::Infercase => std::ptr::addr_of_mut!((*buf).b_p_inf) as *mut c_void,
        OptIndex::Iskeyword => std::ptr::addr_of_mut!((*buf).b_p_isk) as *mut c_void,
        OptIndex::Includeexpr => std::ptr::addr_of_mut!((*buf).b_p_inex) as *mut c_void,
        OptIndex::Indentexpr => std::ptr::addr_of_mut!((*buf).b_p_inde) as *mut c_void,
        OptIndex::Indentkeys => std::ptr::addr_of_mut!((*buf).b_p_indk) as *mut c_void,
        OptIndex::Formatexpr => std::ptr::addr_of_mut!((*buf).b_p_fex) as *mut c_void,
        OptIndex::Lisp => std::ptr::addr_of_mut!((*buf).b_p_lisp) as *mut c_void,
        OptIndex::Lispoptions => std::ptr::addr_of_mut!((*buf).b_p_lop) as *mut c_void,
        OptIndex::Modeline => std::ptr::addr_of_mut!((*buf).b_p_ml) as *mut c_void,
        OptIndex::Matchpairs => std::ptr::addr_of_mut!((*buf).b_p_mps) as *mut c_void,
        OptIndex::Modifiable => std::ptr::addr_of_mut!((*buf).b_p_ma) as *mut c_void,
        OptIndex::Modified => std::ptr::addr_of_mut!((*buf).b_changed) as *mut c_void,
        OptIndex::Nrformats => std::ptr::addr_of_mut!((*buf).b_p_nf) as *mut c_void,
        OptIndex::Preserveindent => std::ptr::addr_of_mut!((*buf).b_p_pi) as *mut c_void,
        OptIndex::Quoteescape => std::ptr::addr_of_mut!((*buf).b_p_qe) as *mut c_void,
        OptIndex::Readonly => std::ptr::addr_of_mut!((*buf).b_p_ro) as *mut c_void,
        OptIndex::Scrollback => std::ptr::addr_of_mut!((*buf).b_p_scbk) as *mut c_void,
        OptIndex::Smartindent => std::ptr::addr_of_mut!((*buf).b_p_si) as *mut c_void,
        OptIndex::Softtabstop => std::ptr::addr_of_mut!((*buf).b_p_sts) as *mut c_void,
        OptIndex::Suffixesadd => std::ptr::addr_of_mut!((*buf).b_p_sua) as *mut c_void,
        OptIndex::Swapfile => std::ptr::addr_of_mut!((*buf).b_p_swf) as *mut c_void,
        OptIndex::Synmaxcol => std::ptr::addr_of_mut!((*buf).b_p_smc) as *mut c_void,
        OptIndex::Syntax => std::ptr::addr_of_mut!((*buf).b_p_syn) as *mut c_void,
        OptIndex::Spellcapcheck => std::ptr::addr_of_mut!((*(*win).w_s).b_p_spc) as *mut c_void,
        OptIndex::Spellfile => std::ptr::addr_of_mut!((*(*win).w_s).b_p_spf) as *mut c_void,
        OptIndex::Spelllang => std::ptr::addr_of_mut!((*(*win).w_s).b_p_spl) as *mut c_void,
        OptIndex::Spelloptions => std::ptr::addr_of_mut!((*(*win).w_s).b_p_spo) as *mut c_void,
        OptIndex::Shiftwidth => std::ptr::addr_of_mut!((*buf).b_p_sw) as *mut c_void,
        OptIndex::Tagfunc => std::ptr::addr_of_mut!((*buf).b_p_tfu) as *mut c_void,
        OptIndex::Tabstop => std::ptr::addr_of_mut!((*buf).b_p_ts) as *mut c_void,
        OptIndex::Textwidth => std::ptr::addr_of_mut!((*buf).b_p_tw) as *mut c_void,
        OptIndex::Undofile => std::ptr::addr_of_mut!((*buf).b_p_udf) as *mut c_void,
        OptIndex::Wrapmargin => std::ptr::addr_of_mut!((*buf).b_p_wm) as *mut c_void,
        OptIndex::Varsofttabstop => std::ptr::addr_of_mut!((*buf).b_p_vsts) as *mut c_void,
        OptIndex::Vartabstop => std::ptr::addr_of_mut!((*buf).b_p_vts) as *mut c_void,
        OptIndex::Keymap => std::ptr::addr_of_mut!((*buf).b_p_keymap) as *mut c_void,
        OptIndex::Signcolumn => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_scl) as *mut c_void,
        OptIndex::Winhighlight => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_winhl) as *mut c_void,
        OptIndex::Winblend => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_winbl) as *mut c_void,
        OptIndex::Statuscolumn => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_stc) as *mut c_void,
        _ => {
            // Matches the original's own `default: iemsg(...)` branch:
            // every OptIndex reaching this point should already have
            // been handled above (buf-scoped or win-scoped - the only
            // way to reach the switch at all, given the is_option_hidden/
            // option_is_global_only early return above) - this is an
            // internal-consistency safety net, not a reachable case in
            // practice (`iemsg` itself isn't translated yet - message.c
            // is a separate, not-yet-started subsystem).
            debug_assert!(false, "E356: get_varp ERROR - unhandled OptIndex in get_varp_from");
            // always return a valid pointer to avoid a crash!
            std::ptr::addr_of_mut!((*buf).b_p_wm) as *mut c_void
        }
    }
}

/// Get pointer to option variable, using the current buffer/window
/// (`get_varp`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf`/`curwin` must be valid, non-null
/// pointers to live `BufT`/`WinT` values.
#[must_use]
pub unsafe fn get_varp(opt_idx: OptIndex) -> *mut c_void {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { get_varp_from(opt_idx, globals.curbuf, globals.curwin) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf_with_ff(ff: &str, bin: bool) -> BufT {
        BufT {
            b_p_ff: Some(ff.as_bytes().to_vec()),
            b_p_bin: i32::from(bin),
            ..Default::default()
        }
    }

    #[test]
    fn get_fileformat_unix() {
        assert_eq!(get_fileformat(&buf_with_ff("unix", false)), EOL_UNIX);
    }

    #[test]
    fn get_fileformat_mac() {
        assert_eq!(get_fileformat(&buf_with_ff("mac", false)), EOL_MAC);
    }

    #[test]
    fn get_fileformat_dos() {
        assert_eq!(get_fileformat(&buf_with_ff("dos", false)), EOL_DOS);
    }

    #[test]
    fn get_fileformat_binary_forces_unix() {
        assert_eq!(get_fileformat(&buf_with_ff("dos", true)), EOL_UNIX);
    }

    #[test]
    fn get_fileformat_empty_ff_defaults_to_dos() {
        let buf = BufT::default(); // b_p_ff is None
        assert_eq!(get_fileformat(&buf), EOL_DOS);
    }

    #[test]
    fn get_fileformat_force_none_eap_matches_get_fileformat() {
        let buf = buf_with_ff("mac", false);
        assert_eq!(get_fileformat_force(&buf, None), EOL_MAC);
        let bin_buf = buf_with_ff("dos", true);
        assert_eq!(get_fileformat_force(&bin_buf, None), EOL_UNIX);
    }

    #[test]
    fn get_fileformat_force_uses_force_ff_when_set() {
        let buf = buf_with_ff("unix", false);
        let eap = crate::ex_cmds_defs::ExargT { force_ff: b'm', ..Default::default() };
        assert_eq!(get_fileformat_force(&buf, Some(&eap)), EOL_MAC);
    }

    #[test]
    fn get_fileformat_force_bin_flag_forces_unix() {
        let buf = buf_with_ff("mac", false);
        let eap = crate::ex_cmds_defs::ExargT {
            force_bin: crate::ex_cmds_defs::FORCE_BIN,
            ..Default::default()
        };
        assert_eq!(get_fileformat_force(&buf, Some(&eap)), EOL_UNIX);
    }

    #[test]
    fn get_fileformat_force_nobin_flag_overrides_buffer_binary() {
        let buf = buf_with_ff("mac", true); // buffer itself is binary
        let eap = crate::ex_cmds_defs::ExargT {
            force_bin: crate::ex_cmds_defs::FORCE_NOBIN,
            ..Default::default()
        };
        // force_bin != 0 (FORCE_NOBIN) takes the ternary's "true" branch
        // in the original, comparing against FORCE_BIN specifically -
        // FORCE_NOBIN != FORCE_BIN, so this does NOT force unix, and
        // falls through to reading b_p_ff instead.
        assert_eq!(get_fileformat_force(&buf, Some(&eap)), EOL_MAC);
    }

    #[test]
    fn get_fileformat_force_falls_back_to_buf_bin_when_eap_force_bin_unset() {
        let buf = buf_with_ff("mac", true); // buffer itself is binary
        let eap = crate::ex_cmds_defs::ExargT::default(); // force_bin == 0
        assert_eq!(get_fileformat_force(&buf, Some(&eap)), EOL_UNIX);
    }

    #[test]
    fn magic_isset_not_set_follows_p_magic() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.magic_overruled;
        let prev_magic = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_magic;
        unsafe { crate::globals::GLOBALS.get_mut() }.magic_overruled =
            crate::regexp_defs::OptmagicT::NotSet;

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_magic = 1;
        assert!(magic_isset());
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_magic = 0;
        assert!(!magic_isset());

        unsafe { crate::globals::GLOBALS.get_mut() }.magic_overruled = prev;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_magic = prev_magic;
    }

    #[test]
    fn magic_isset_overruled_on_and_off() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.magic_overruled;

        unsafe { crate::globals::GLOBALS.get_mut() }.magic_overruled =
            crate::regexp_defs::OptmagicT::MagicOn;
        assert!(magic_isset());
        unsafe { crate::globals::GLOBALS.get_mut() }.magic_overruled =
            crate::regexp_defs::OptmagicT::MagicOff;
        assert!(!magic_isset());

        unsafe { crate::globals::GLOBALS.get_mut() }.magic_overruled = prev;
    }

    #[test]
    fn shortmess_false_when_p_shm_unset() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm = None;

        assert!(!shortmess(b'r'));

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm = prev;
    }

    #[test]
    fn shortmess_true_when_x_directly_present() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm = Some(b"rl".to_vec());

        assert!(shortmess(b'r'));
        assert!(!shortmess(b'x'));

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm = prev;
    }

    #[test]
    fn shortmess_true_via_all_abbreviations_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm.clone();
        // 'a' present, and 'm' (SHM_MOD) is in SHM_ALL_ABBREVIATIONS.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm = Some(b"a".to_vec());

        assert!(shortmess(crate::option_vars::shm::MOD));

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm = prev;
    }

    #[test]
    fn can_bs_false_for_start_in_prompt_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_bt: Some(b"prompt".to_vec()), ..Default::default() };
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;

        let result = unsafe { can_bs(crate::option_vars::BS_START) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        assert!(!result);
    }

    #[test]
    fn can_bs_legacy_numeric_2_excludes_only_nostop() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let prev_bs = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs.clone();
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs = Some(b"2".to_vec());

        let indent_result = unsafe { can_bs(crate::option_vars::BS_INDENT) };
        let nostop_result = unsafe { can_bs(crate::option_vars::BS_NOSTOP) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs = prev_bs;
        assert!(indent_result);
        assert!(!nostop_result);
    }

    #[test]
    fn can_bs_checks_flag_presence_in_p_bs() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let prev_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let prev_bs = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs.clone();
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut buf as *mut BufT;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs =
            Some(vec![crate::option_vars::BS_INDENT]);

        let indent_result = unsafe { can_bs(crate::option_vars::BS_INDENT) };
        let eol_result = unsafe { can_bs(crate::option_vars::BS_EOL) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bs = prev_bs;
        assert!(indent_result);
        assert!(!eol_result);
    }

    #[test]
    fn get_bkc_flags_prefers_buffer_local() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.bkc_flags;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.bkc_flags = 7;

        let buf_local = BufT { b_bkc_flags: 3, ..Default::default() };
        let buf_unset = BufT::default();
        assert_eq!(get_bkc_flags(&buf_local), 3);
        assert_eq!(get_bkc_flags(&buf_unset), 7);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.bkc_flags = prev;
    }

    #[test]
    fn get_flp_value_prefers_non_empty_buffer_local() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_flp.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_flp = Some(b"global".to_vec());

        let buf_local = BufT { b_p_flp: Some(b"local".to_vec()), ..Default::default() };
        let buf_empty = BufT { b_p_flp: Some(Vec::new()), ..Default::default() };
        let buf_unset = BufT::default();
        assert_eq!(get_flp_value(&buf_local), b"local");
        assert_eq!(get_flp_value(&buf_empty), b"global");
        assert_eq!(get_flp_value(&buf_unset), b"global");

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_flp = prev;
    }

    #[test]
    fn get_ve_flags_prefers_window_local_and_masks_none_bits() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags =
            crate::option_vars::opt_ve_flag::ALL;

        let mut win_local = WinT::default();
        win_local.w_onebuf_opt.wo_ve_flags =
            crate::option_vars::opt_ve_flag::ONEMORE | crate::option_vars::opt_ve_flag::NONE;
        assert_eq!(get_ve_flags(&win_local), crate::option_vars::opt_ve_flag::ONEMORE);

        let win_unset = WinT::default();
        assert_eq!(get_ve_flags(&win_unset), crate::option_vars::opt_ve_flag::ALL);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ve_flags = prev;
    }

    #[test]
    fn get_showbreak_value_variants() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sbr.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sbr = Some(b">>".to_vec());

        let mut win_local = WinT::default();
        win_local.w_onebuf_opt.wo_sbr = Some(b"++".to_vec());
        assert_eq!(get_showbreak_value(&win_local), b"++");

        let mut win_none_literal = WinT::default();
        win_none_literal.w_onebuf_opt.wo_sbr = Some(b"NONE".to_vec());
        assert_eq!(get_showbreak_value(&win_none_literal), Vec::<u8>::new());

        let win_unset = WinT::default();
        assert_eq!(get_showbreak_value(&win_unset), b">>");

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sbr = prev;
    }

    #[test]
    fn default_fileformat_variants() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ffs.clone();

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ffs = Some(b"mac".to_vec());
        assert_eq!(default_fileformat(), EOL_MAC);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ffs = Some(b"dos".to_vec());
        assert_eq!(default_fileformat(), EOL_DOS);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ffs = Some(b"unix".to_vec());
        assert_eq!(default_fileformat(), EOL_UNIX);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ffs = None;
        assert_eq!(default_fileformat(), EOL_UNIX);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ffs = prev;
    }

    #[test]
    fn csh_and_fish_like_shell_detect_tail() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sh.clone();

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sh = Some(b"/bin/tcsh".to_vec());
        assert!(csh_like_shell());
        assert!(!fish_like_shell());

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sh = Some(b"/usr/bin/fish".to_vec());
        assert!(fish_like_shell());
        assert!(!csh_like_shell());

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sh = Some(b"/bin/bash".to_vec());
        assert!(!csh_like_shell());
        assert!(!fish_like_shell());

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sh = prev;
    }

    #[test]
    fn get_scrolloff_value_zero_in_terminal_mode_for_terminal_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
        unsafe { crate::globals::GLOBALS.get_mut() }.State =
            crate::state_defs::mode::TERMINAL as i32;

        let mut dummy_terminal: u8 = 0;
        let mut buf = BufT {
            terminal: (&mut dummy_terminal as *mut u8).cast(),
            ..Default::default()
        };
        let win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };

        let result = unsafe { get_scrolloff_value(&win) };

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        assert_eq!(result, 0);
    }

    #[test]
    fn get_scrolloff_value_falls_back_to_global_when_local_negative() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
        let prev_so = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_so;
        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::NORMAL as i32;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_so = 5;

        let mut buf = BufT::default(); // terminal is null - not a terminal buffer
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_so = -1;

        let result = unsafe { get_scrolloff_value(&win) };

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_so = prev_so;
        assert_eq!(result, 5);
    }

    #[test]
    fn get_scrolloff_value_uses_local_when_set() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
        unsafe { crate::globals::GLOBALS.get_mut() }.State = crate::state_defs::mode::NORMAL as i32;

        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_so = 9;

        let result = unsafe { get_scrolloff_value(&win) };

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        assert_eq!(result, 9);
    }

    #[test]
    fn get_scrolloffpad_value_falls_back_to_curwin_global() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_sop = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sop;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sop = 2;

        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_sop = -1;

        let result = unsafe { get_scrolloffpad_value(&wp) };

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sop = prev_sop;
        assert_eq!(result, 2);
    }

    #[test]
    fn get_scrolloffpad_value_non_default_reads_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        let mut curwin_win = WinT::default();
        curwin_win.w_onebuf_opt.wo_sop = 8;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut curwin_win as *mut WinT;

        // wp itself is a *different* WinT whose own w_p_sop is merely
        // used for the != -1 check; the actual returned value, per the
        // original's own (preserved) quirk, comes from curwin instead.
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_sop = 3;

        let result = unsafe { get_scrolloffpad_value(&wp) };

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
        assert_eq!(result, 8);
    }

    #[test]
    fn get_sidescrolloff_value_variants() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_siso;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_siso = 4;

        let mut win_negative = WinT::default();
        win_negative.w_onebuf_opt.wo_siso = -1;
        assert_eq!(get_sidescrolloff_value(&win_negative), 4);

        let mut win_local = WinT::default();
        win_local.w_onebuf_opt.wo_siso = 6;
        assert_eq!(get_sidescrolloff_value(&win_local), 6);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_siso = prev;
    }

    #[test]
    fn valid_name_allows_alnum_and_allowed_chars() {
        assert!(valid_name(b"abc123", b"_-"));
        assert!(valid_name(b"ab_c-1", b"_-"));
        assert!(!valid_name(b"ab c", b"_-")); // space not allowed
        assert!(!valid_name(b"ab.c", b"_-")); // dot not allowed
    }

    #[test]
    fn valid_name_stops_at_first_embedded_nul() {
        // Only the (empty) allowed-set matters here since everything
        // before the NUL is alphanumeric.
        assert!(valid_name(b"abc\0!!!", b""));
    }

    #[test]
    fn check_blending_true_when_winblend_positive() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_winbl = 30;
        check_blending(&mut wp);
        assert!(wp.w_grid_alloc.blending);
    }

    #[test]
    fn check_blending_true_when_floating_with_shadow() {
        let mut wp = WinT { w_floating: true, ..Default::default() };
        wp.w_config.shadow = true;
        check_blending(&mut wp);
        assert!(wp.w_grid_alloc.blending);
    }

    #[test]
    fn check_blending_false_otherwise() {
        let mut wp = WinT::default();
        check_blending(&mut wp);
        assert!(!wp.w_grid_alloc.blending);

        // Floating without shadow also stays false.
        let mut wp2 = WinT { w_floating: true, ..Default::default() };
        check_blending(&mut wp2);
        assert!(!wp2.w_grid_alloc.blending);
    }

    #[test]
    fn fill_culopt_flags_parses_line() {
        let mut wp = WinT::default();
        assert_eq!(fill_culopt_flags(Some(b"line"), &mut wp), crate::vim_defs::OK);
        assert_eq!(wp.w_p_culopt_flags, crate::option_vars::opt_culopt_flag::LINE as u8);
    }

    #[test]
    fn fill_culopt_flags_parses_both_as_line_and_number() {
        let mut wp = WinT::default();
        assert_eq!(fill_culopt_flags(Some(b"both"), &mut wp), crate::vim_defs::OK);
        assert_eq!(
            wp.w_p_culopt_flags,
            (crate::option_vars::opt_culopt_flag::LINE | crate::option_vars::opt_culopt_flag::NUMBER)
                as u8
        );
    }

    #[test]
    fn fill_culopt_flags_parses_number() {
        let mut wp = WinT::default();
        assert_eq!(fill_culopt_flags(Some(b"number"), &mut wp), crate::vim_defs::OK);
        assert_eq!(wp.w_p_culopt_flags, crate::option_vars::opt_culopt_flag::NUMBER as u8);
    }

    #[test]
    fn fill_culopt_flags_parses_screenline() {
        let mut wp = WinT::default();
        assert_eq!(fill_culopt_flags(Some(b"screenline"), &mut wp), crate::vim_defs::OK);
        assert_eq!(wp.w_p_culopt_flags, crate::option_vars::opt_culopt_flag::SCREENLINE as u8);
    }

    #[test]
    fn fill_culopt_flags_parses_comma_separated_combination() {
        let mut wp = WinT::default();
        assert_eq!(fill_culopt_flags(Some(b"number,line"), &mut wp), crate::vim_defs::OK);
        assert_eq!(
            wp.w_p_culopt_flags,
            (crate::option_vars::opt_culopt_flag::LINE | crate::option_vars::opt_culopt_flag::NUMBER)
                as u8
        );
    }

    #[test]
    fn fill_culopt_flags_empty_string_gives_zero_flags() {
        let mut wp = WinT::default();
        assert_eq!(fill_culopt_flags(Some(b""), &mut wp), crate::vim_defs::OK);
        assert_eq!(wp.w_p_culopt_flags, 0);
    }

    #[test]
    fn fill_culopt_flags_rejects_line_and_screenline_together() {
        let mut wp = WinT::default();
        assert_eq!(fill_culopt_flags(Some(b"line,screenline"), &mut wp), crate::vim_defs::FAIL);
    }

    #[test]
    fn fill_culopt_flags_rejects_unrecognized_token() {
        let mut wp = WinT::default();
        assert_eq!(fill_culopt_flags(Some(b"bogus"), &mut wp), crate::vim_defs::FAIL);
    }

    #[test]
    fn fill_culopt_flags_rejects_recognized_token_with_trailing_garbage() {
        let mut wp = WinT::default();
        assert_eq!(fill_culopt_flags(Some(b"linex"), &mut wp), crate::vim_defs::FAIL);
    }

    #[test]
    fn fill_culopt_flags_silently_skips_a_leading_comma_real_quirk() {
        // A real, faithfully-replicated quirk (not "fixed"): an
        // unrecognized token at a position that's already ',' - such
        // as a leading or doubled comma - is silently skipped as an
        // empty entry rather than rejected. See this function's own
        // doc comment.
        let mut wp = WinT::default();
        assert_eq!(fill_culopt_flags(Some(b",line"), &mut wp), crate::vim_defs::OK);
        assert_eq!(wp.w_p_culopt_flags, crate::option_vars::opt_culopt_flag::LINE as u8);

        let mut wp2 = WinT::default();
        assert_eq!(fill_culopt_flags(Some(b"line,,number"), &mut wp2), crate::vim_defs::OK);
        assert_eq!(
            wp2.w_p_culopt_flags,
            (crate::option_vars::opt_culopt_flag::LINE | crate::option_vars::opt_culopt_flag::NUMBER)
                as u8
        );
    }

    #[test]
    fn fill_culopt_flags_none_uses_windows_own_wo_culopt_value() {
        let mut wp = WinT::default();
        wp.w_onebuf_opt.wo_culopt = Some(b"number".to_vec());
        assert_eq!(fill_culopt_flags(None, &mut wp), crate::vim_defs::OK);
        assert_eq!(wp.w_p_culopt_flags, crate::option_vars::opt_culopt_flag::NUMBER as u8);
    }

    #[test]
    fn fill_culopt_flags_none_with_unset_wo_culopt_defaults_to_empty() {
        let mut wp = WinT::default();
        assert_eq!(fill_culopt_flags(None, &mut wp), crate::vim_defs::OK);
        assert_eq!(wp.w_p_culopt_flags, 0);
    }

    /// Points `GLOBALS.curbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime
    /// (matching `change.rs`/`buffer.rs`'s own identically-named
    /// helper).
    struct CurbufGuard {
        previous: *mut BufT,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut BufT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    #[test]
    fn get_equalprg_prefers_non_empty_buffer_local() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ep.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ep = Some(b"global-ep".to_vec());

        let mut buf_local = BufT { b_p_ep: Some(b"local-ep".to_vec()), ..Default::default() };
        {
            let _guard = CurbufGuard::set(&mut buf_local as *mut BufT);
            assert_eq!(unsafe { get_equalprg() }, b"local-ep");
        }

        let mut buf_empty = BufT { b_p_ep: Some(Vec::new()), ..Default::default() };
        {
            let _guard = CurbufGuard::set(&mut buf_empty as *mut BufT);
            assert_eq!(unsafe { get_equalprg() }, b"global-ep");
        }

        let mut buf_unset = BufT { b_p_ep: None, ..Default::default() };
        {
            let _guard = CurbufGuard::set(&mut buf_unset as *mut BufT);
            assert_eq!(unsafe { get_equalprg() }, b"global-ep");
        }

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ep = None;
        let mut buf_both_unset = BufT::default();
        {
            let _guard = CurbufGuard::set(&mut buf_both_unset as *mut BufT);
            assert_eq!(unsafe { get_equalprg() }, Vec::<u8>::new());
        }

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ep = prev;
    }

    #[test]
    fn get_findfunc_prefers_non_empty_buffer_local() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ffu.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ffu = Some(b"GlobalFindFunc".to_vec());

        let mut buf_local = BufT { b_p_ffu: Some(b"LocalFindFunc".to_vec()), ..Default::default() };
        {
            let _guard = CurbufGuard::set(&mut buf_local as *mut BufT);
            assert_eq!(unsafe { get_findfunc() }, b"LocalFindFunc");
        }

        let mut buf_empty = BufT { b_p_ffu: Some(Vec::new()), ..Default::default() };
        {
            let _guard = CurbufGuard::set(&mut buf_empty as *mut BufT);
            assert_eq!(unsafe { get_findfunc() }, b"GlobalFindFunc");
        }

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ffu = prev;
    }
}

#[cfg(test)]
mod varp_tests {
    use super::*;

    #[test]
    fn is_option_hidden_true_for_immutable_option_false_for_normal_one() {
        assert!(is_option_hidden(OptIndex::Aleph)); // immutable, self-ref var
        assert!(!is_option_hidden(OptIndex::Allowrevins));
        assert!(!is_option_hidden(OptIndex::Invalid));
    }

    #[test]
    fn option_has_type_matches_the_declared_type() {
        assert!(option_has_type(OptIndex::Aleph, OptValType::Number));
        assert!(!option_has_type(OptIndex::Aleph, OptValType::String));
        assert!(option_has_type(OptIndex::Ambiwidth, OptValType::String));
        assert!(option_has_type(OptIndex::Allowrevins, OptValType::Boolean));
    }

    #[test]
    fn option_has_scope_reflects_scope_flags() {
        assert!(option_has_scope(OptIndex::Allowrevins, OptScope::Global));
        assert!(!option_has_scope(OptIndex::Allowrevins, OptScope::Win));
        assert!(option_has_scope(OptIndex::Arabic, OptScope::Win));
        assert!(!option_has_scope(OptIndex::Arabic, OptScope::Global));
    }

    #[test]
    fn option_is_global_local_true_only_for_global_plus_buf_or_win() {
        assert!(option_is_global_local(OptIndex::Equalprg)); // global + buf
        assert!(!option_is_global_local(OptIndex::Allowrevins)); // global only
        assert!(!option_is_global_local(OptIndex::Arabic)); // win only
        assert!(!option_is_global_local(OptIndex::Invalid));
    }

    #[test]
    fn option_is_global_only_true_only_when_no_buf_or_win_scope() {
        assert!(option_is_global_only(OptIndex::Allowrevins));
        assert!(!option_is_global_only(OptIndex::Equalprg)); // also has buf scope
        assert!(!option_is_global_only(OptIndex::Arabic)); // win only, no global
        assert!(!option_is_global_only(OptIndex::Invalid));
    }

    #[test]
    fn option_is_window_local_true_only_for_pure_window_scope() {
        assert!(option_is_window_local(OptIndex::Arabic));
        assert!(!option_is_window_local(OptIndex::Allowrevins)); // global only
        assert!(!option_is_window_local(OptIndex::Equalprg)); // global + buf
        assert!(!option_is_window_local(OptIndex::Invalid));
    }

    #[test]
    fn get_option_returns_the_right_entry() {
        assert_eq!(get_option(OptIndex::Aleph).fullname, b"aleph");
        assert_eq!(get_option(OptIndex::Ambiwidth).fullname, b"ambiwidth");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic]
    fn get_option_debug_panics_on_invalid_index() {
        let _ = get_option(OptIndex::Invalid);
    }

    #[test]
    fn get_varp_from_global_only_option_returns_p_var_directly() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let p = get_option(OptIndex::Allowrevins);
        let varp = unsafe { get_varp_from(OptIndex::Allowrevins, &mut buf as *mut BufT, &mut win as *mut WinT) };
        assert_eq!(varp, p.var);
        assert!(!varp.is_null());
    }

    #[test]
    fn get_varp_from_hidden_immutable_option_returns_p_var() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let p = get_option(OptIndex::Aleph);
        let varp = unsafe { get_varp_from(OptIndex::Aleph, &mut buf as *mut BufT, &mut win as *mut WinT) };
        assert_eq!(varp, p.var);
        assert!(varp.is_null()); // aleph's var is null (see OPTIONS's own doc comment)
    }

    #[test]
    fn get_varp_from_string_fallback_uses_global_when_buffer_local_unset() {
        let mut buf = BufT::default(); // b_p_ep defaults to None (unset)
        let mut win = WinT::default();
        let p = get_option(OptIndex::Equalprg);
        let varp = unsafe { get_varp_from(OptIndex::Equalprg, &mut buf as *mut BufT, &mut win as *mut WinT) };
        assert_eq!(varp, p.var);
    }

    #[test]
    fn get_varp_from_string_fallback_uses_buffer_local_when_set() {
        let mut buf = BufT { b_p_ep: Some(b"myprg".to_vec()), ..Default::default() };
        let mut win = WinT::default();
        let varp = unsafe { get_varp_from(OptIndex::Equalprg, &mut buf as *mut BufT, &mut win as *mut WinT) }
            as *mut Option<Vec<u8>>;
        // Verify the pointer genuinely aliases buf.b_p_ep (write through it,
        // observe the change via the original binding), not just a copy.
        unsafe { *varp = Some(b"changed".to_vec()) };
        assert_eq!(buf.b_p_ep, Some(b"changed".to_vec()));
    }

    #[test]
    fn get_varp_from_string_fallback_treats_empty_string_as_unset() {
        let mut buf = BufT { b_p_ep: Some(Vec::new()), ..Default::default() }; // empty = unset, matching *x != NUL in C
        let mut win = WinT::default();
        let p = get_option(OptIndex::Equalprg);
        let varp = unsafe { get_varp_from(OptIndex::Equalprg, &mut buf as *mut BufT, &mut win as *mut WinT) };
        assert_eq!(varp, p.var);
    }

    #[test]
    fn get_varp_from_number_fallback_uses_global_when_negative() {
        let mut buf = BufT { b_p_ac: -1, ..Default::default() };
        let mut win = WinT::default();
        let p = get_option(OptIndex::Autocomplete);
        let varp = unsafe { get_varp_from(OptIndex::Autocomplete, &mut buf as *mut BufT, &mut win as *mut WinT) };
        assert_eq!(varp, p.var);
    }

    #[test]
    fn get_varp_from_number_fallback_uses_buffer_local_when_non_negative() {
        let mut buf = BufT { b_p_ac: 1, ..Default::default() };
        let mut win = WinT::default();
        let varp =
            unsafe { get_varp_from(OptIndex::Autocomplete, &mut buf as *mut BufT, &mut win as *mut WinT) } as *mut i32;
        unsafe { *varp = 42 };
        assert_eq!(buf.b_p_ac, 42);
    }

    #[test]
    fn get_varp_from_undolevels_special_sentinel_case() {
        let mut buf = BufT { b_p_ul: crate::option_vars::NO_LOCAL_UNDOLEVEL, ..Default::default() };
        let mut win = WinT::default();
        let p = get_option(OptIndex::Undolevels);
        let varp = unsafe { get_varp_from(OptIndex::Undolevels, &mut buf as *mut BufT, &mut win as *mut WinT) };
        assert_eq!(varp, p.var);

        buf.b_p_ul = 500;
        let varp2 =
            unsafe { get_varp_from(OptIndex::Undolevels, &mut buf as *mut BufT, &mut win as *mut WinT) } as *mut OptInt;
        unsafe { *varp2 = 999 };
        assert_eq!(buf.b_p_ul, 999);
    }

    #[test]
    fn get_varp_from_window_local_option_aliases_w_onebuf_opt() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let varp =
            unsafe { get_varp_from(OptIndex::Arabic, &mut buf as *mut BufT, &mut win as *mut WinT) } as *mut i32;
        unsafe { *varp = 1 };
        assert_eq!(win.w_onebuf_opt.wo_arab, 1);
    }

    #[test]
    fn get_varp_from_buffer_local_option_aliases_buf_field_directly() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let varp =
            unsafe { get_varp_from(OptIndex::Autoindent, &mut buf as *mut BufT, &mut win as *mut WinT) } as *mut i32;
        unsafe { *varp = 1 };
        assert_eq!(buf.b_p_ai, 1);
    }

    #[test]
    fn get_varp_from_modified_aliases_b_changed() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let varp =
            unsafe { get_varp_from(OptIndex::Modified, &mut buf as *mut BufT, &mut win as *mut WinT) } as *mut i32;
        unsafe { *varp = 1 };
        assert_eq!(buf.b_changed, 1);
    }

    #[test]
    fn get_varp_from_spell_option_goes_through_w_s() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let mut syn = crate::buffer_defs::SynblockT::default();
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;
        let varp = unsafe { get_varp_from(OptIndex::Spellcapcheck, &mut buf as *mut BufT, &mut win as *mut WinT) }
            as *mut Option<Vec<u8>>;
        unsafe { *varp = Some(b"foo".to_vec()) };
        assert_eq!(syn.b_p_spc, Some(b"foo".to_vec()));
    }

    #[cfg(windows)]
    #[test]
    fn get_varp_from_completeslash_resolves_on_windows() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let varp = unsafe { get_varp_from(OptIndex::Completeslash, &mut buf as *mut BufT, &mut win as *mut WinT) }
            as *mut Option<Vec<u8>>;
        unsafe { *varp = Some(b"slash".to_vec()) };
        assert_eq!(buf.b_p_csl, Some(b"slash".to_vec()));
    }

    #[cfg(not(windows))]
    #[test]
    fn get_varp_from_completeslash_is_hidden_off_windows() {
        // enable_if = 'BACKSLASH_IN_FILENAME' makes it immutable with a
        // null self-ref var off Windows (see OPTIONS's own doc comment) -
        // get_varp_from never even reaches the switch for it.
        assert!(is_option_hidden(OptIndex::Completeslash));
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let varp =
            unsafe { get_varp_from(OptIndex::Completeslash, &mut buf as *mut BufT, &mut win as *mut WinT) };
        assert!(varp.is_null());
    }

    #[test]
    fn get_varp_uses_current_buffer_and_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_buf = globals.curbuf;
        let prev_win = globals.curwin;
        globals.curbuf = &mut buf as *mut BufT;
        globals.curwin = &mut win as *mut WinT;

        let varp = unsafe { get_varp(OptIndex::Autoindent) } as *mut i32;
        unsafe { *varp = 1 };
        assert_eq!(buf.b_p_ai, 1);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curbuf = prev_buf;
        globals.curwin = prev_win;
    }
}
