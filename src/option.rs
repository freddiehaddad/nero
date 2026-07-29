//! Translated from `src/nvim/option.c` (tractable core only).
//!
//! `option.c` is a massive (~6897-line) file implementing the entire
//! `:set`/options-parsing engine, deeply entangled with the eval
//! engine, autocmd triggers, and nearly every other subsystem: the
//! `:set` command parser (`do_set`/`ex_set`) and the typed
//! `set_option_value` entry point are not attempted here.
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
//! `OPTIONS` table), and now also `get_varp_scope_from`/
//! `get_varp_scope` (the `:setlocal`/`:setglobal`-specific wrapper -
//! folds in `get_option_varp_scope_from`, since this crate's
//! `OptIndex`-taking signatures already make that a pure duplicate).
//! Its own ~34-entry "force the local value" table was extracted
//! mechanically from the ALREADY-committed `get_varp_from` source
//! (the single source of truth for these field names, not re-derived
//! from the C source a second time - avoids a second chance to
//! introduce a transcription mismatch between the two functions).
//! Its `OPT_GLOBAL`-for-a-window-local-option branch replaces the
//! original's `GLOBAL_WO(p)` pointer-arithmetic trick (computing a
//! sibling `WinT.w_allbuf_opt` field's address via byte-offset
//! arithmetic from a `w_onebuf_opt`-derived pointer - unsound in Rust,
//! since the result has no provenance over `w_allbuf_opt`'s own,
//! separate allocation) with a real, from-scratch 44-entry
//! `w_allbuf_opt` field table instead, mechanically derived from
//! `get_varp_from`'s own window-local arms by substituting
//! `w_onebuf_opt` for `w_allbuf_opt`. Deliberately omits `kOptTagfunc`
//! from the "force local" table - present in the original's own
//! switch, but unreachable there given `tagfunc`'s real, verified
//! `scope_flags` (buf-only, no global scope) make
//! `option_is_global_local` always `false` for it, so the guard
//! already prevents that arm from ever being reached in the original
//! either (a likely harmless upstream leftover, not replicated as
//! dead code here).
//!
//! Also translated this pass: `set_option_varp` (the write-side
//! counterpart to `optval_from_varp`, symmetric type-punning in the
//! other direction - deliberately drops the original's
//! `free_oldval: bool` parameter, a pure C-manual-memory-management
//! artifact that doesn't apply to a real Rust enum with automatic
//! `Drop` semantics, see its own doc comment for the full reasoning);
//! `find_tty_option_end`/`is_tty_option`/`get_tty_option`/
//! `set_tty_option` (small, self-contained TTY/keycode-option name
//! parsing and value storage - `p_term`/`p_ttytype` translated as new
//! file-local `GlobalCell` statics, matching `os/env.rs`'s own
//! `HOMEDIR` precedent for file-local state that doesn't belong in
//! the shared `OptionVars`/`Globals` struct bags).
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
//! Also translated: `skip_to_option_part`/`copy_option_part`
//! (`option.c`'s own comma-separated-option-string parsing helpers,
//! e.g. for `'suffixes'`/`'path'`) - `p`/`option` cursors replace the
//! original's own `char *`/`char **` in-out pointers with a plain
//! byte offset into the whole option string; `copy_option_part`'s own
//! `maxlen`-truncation is kept faithfully (real, observable behavior
//! for a part longer than `maxlen`, not just an implementation
//! detail this crate's growable `Vec<u8>` could otherwise drop).
//! `path.rs`'s `match_suffix` is their first real caller.
//!
//! **STALE-NOTE FIX**: `find_option`/`find_option_len` (below) were
//! previously (incorrectly) described here as still needing the
//! perfect-hash generated table - they are, in fact, ALREADY
//! translated, via `OPTION_HASH_ELEMS` (a plain `HashMap`, not a
//! literal port of the original's hand-rolled dispatch tree - see
//! that static's own doc comment for why a `HashMap` is a faithful,
//! simpler substitute). This note is corrected here rather than left
//! to compound further.
//!
//! Deferred: the full `set_option_value`/`set_option`/
//! `did_set_option`/`validate_option_value` write pipeline - each
//! layer found to need a currently-blocked subsystem while scoping
//! this pass:
//! - `did_set_option` needs the ~150 real `did_set_*`/`expand_*`
//!   per-option callbacks (already known-deferred, see
//!   `option_defs.rs`'s own `OPTIONS` doc comment) AND the redraw
//!   pipeline (`check_redraw`/`redraw_later`/`redraw_buf_later`/
//!   `redraw_all_later`/`changed_window_setting` - already documented
//!   as blocking `undo.c`'s undo/redo state machine, per this
//!   project's own plan) AND the `OptionSet` autocommand trigger
//!   machinery.
//! - `validate_option_value`/`validate_num_option`/
//!   `check_num_option_bounds` are now FULLY translated (no remaining
//!   panics): `OptIndex::Lines`/`OptIndex::Scroll` (previously the
//!   only 2 blocked cases, needing `window.c`'s
//!   `min_rows_for_all_tabpages`/`win_default_scroll`) are real now
//!   that `frame_minheight`/`tabline_height`/`global_stl_height` exist
//!   (`window.rs`) - `OptIndex::Lines`'s own real error message embeds
//!   a dynamic value the original formats with `vim_snprintf`; this
//!   crate returns a fixed placeholder string instead, since nothing
//!   here displays it to a real user yet (see `check_num_option_
//!   bounds`'s own doc comment for the full reasoning).
//! - `was_set_insecurely`/`insecure_flag` still need `OPTIONS[idx]
//!   .flags`'s own `WAS_SET`/`INSECURE` bits threaded through a real
//!   `:set` call site to have any meaningful test value.
//! - `parse_winhl_opt` needs the decoration/highlight-group subsystem
//!   (`nvim_create_namespace`/`get_decor_provider`/`syn_check_group`/
//!   `ns_hl_def`).
//! - `do_set`/`ex_set`'s command-line parsing itself, plus
//!   `option_was_set`/`get_winbuf_options`/`get_vimoption`/etc.
//!   (everything needing the full parsed-`:set`-argument machinery,
//!   not just a resolved storage address and a read/write).

use crate::buffer_defs::{BufT, WinT};
use crate::option_defs::{OptIndex, OptScope, OptValType, OptVal, VimoptionT, OPTIONS};
use crate::option_vars::{EOL_DOS, EOL_MAC, EOL_UNIX};
use crate::types_defs::{OptInt, TriState};
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

/// Set the global value for `'iminsert'` to `buf`'s own local value
/// (`set_iminsert_global`).
pub fn set_iminsert_global(buf: &BufT) {
    unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_iminsert = buf.b_p_iminsert;
}

/// Set the global value for `'imsearch'` to `buf`'s own local value
/// (`set_imsearch_global`).
pub fn set_imsearch_global(buf: &BufT) {
    unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_imsearch = buf.b_p_imsearch;
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


/// Get pointer to option variable, depending on local or global scope
/// (`get_varp_scope_from`).
///
/// `opt_flags` can be `opt_set_flags::OPT_LOCAL`, `OPT_GLOBAL`, or a
/// combination - when both are missing, this falls through to
/// [`get_varp_from`] exactly (the "just get the effective value"
/// case).
///
/// # Safety
/// `buf`/`win` must be valid, non-null pointers to live `BufT`/`WinT`
/// values.
#[must_use]
pub unsafe fn get_varp_scope_from(opt_idx: OptIndex, opt_flags: u32, buf: *mut BufT, win: *mut WinT) -> *mut c_void {
    if (opt_flags & crate::option_defs::opt_set_flags::OPT_GLOBAL) != 0 && !option_is_global_only(opt_idx) {
        if option_is_window_local(opt_idx) {
            // Real per-field addressing into WinT.w_allbuf_opt, NOT the
            // original's GLOBAL_WO(p) byte-offset-from-w_onebuf_opt trick
            // (which has no sound Rust equivalent - the resulting pointer
            // would have no provenance over w_allbuf_opt's own, separate
            // allocation). See this file's own module doc comment.
            // SAFETY: forwarded from this function's own safety doc.
            return unsafe {
                match opt_idx {
        OptIndex::Arabic => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_arab) as *mut c_void,
        OptIndex::List => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_list) as *mut c_void,
        OptIndex::Spell => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_spell) as *mut c_void,
        OptIndex::Cursorcolumn => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_cuc) as *mut c_void,
        OptIndex::Cursorline => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_cul) as *mut c_void,
        OptIndex::Cursorlineopt => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_culopt) as *mut c_void,
        OptIndex::Colorcolumn => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_cc) as *mut c_void,
        OptIndex::Diff => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_diff) as *mut c_void,
        OptIndex::Eventignorewin => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_eiw) as *mut c_void,
        OptIndex::Foldcolumn => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_fdc) as *mut c_void,
        OptIndex::Foldenable => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_fen) as *mut c_void,
        OptIndex::Foldignore => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_fdi) as *mut c_void,
        OptIndex::Foldlevel => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_fdl) as *mut c_void,
        OptIndex::Foldmethod => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_fdm) as *mut c_void,
        OptIndex::Foldminlines => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_fml) as *mut c_void,
        OptIndex::Foldnestmax => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_fdn) as *mut c_void,
        OptIndex::Foldexpr => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_fde) as *mut c_void,
        OptIndex::Foldtext => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_fdt) as *mut c_void,
        OptIndex::Foldmarker => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_fmr) as *mut c_void,
        OptIndex::Number => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_nu) as *mut c_void,
        OptIndex::Relativenumber => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_rnu) as *mut c_void,
        OptIndex::Numberwidth => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_nuw) as *mut c_void,
        OptIndex::Winfixbuf => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_wfb) as *mut c_void,
        OptIndex::Winfixheight => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_wfh) as *mut c_void,
        OptIndex::Winfixwidth => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_wfw) as *mut c_void,
        OptIndex::Winpinned => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_wp) as *mut c_void,
        OptIndex::Previewwindow => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_pvw) as *mut c_void,
        OptIndex::Lhistory => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_lhi) as *mut c_void,
        OptIndex::Rightleft => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_rl) as *mut c_void,
        OptIndex::Rightleftcmd => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_rlc) as *mut c_void,
        OptIndex::Scroll => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_scr) as *mut c_void,
        OptIndex::Smoothscroll => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_sms) as *mut c_void,
        OptIndex::Wrap => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_wrap) as *mut c_void,
        OptIndex::Linebreak => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_lbr) as *mut c_void,
        OptIndex::Breakindent => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_bri) as *mut c_void,
        OptIndex::Breakindentopt => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_briopt) as *mut c_void,
        OptIndex::Scrollbind => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_scb) as *mut c_void,
        OptIndex::Cursorbind => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_crb) as *mut c_void,
        OptIndex::Concealcursor => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_cocu) as *mut c_void,
        OptIndex::Conceallevel => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_cole) as *mut c_void,
        OptIndex::Signcolumn => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_scl) as *mut c_void,
        OptIndex::Winhighlight => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_winhl) as *mut c_void,
        OptIndex::Winblend => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_winbl) as *mut c_void,
        OptIndex::Statuscolumn => std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_stc) as *mut c_void,
                    _ => {
                        debug_assert!(
                            false,
                            "get_varp_scope_from: OPT_GLOBAL window-local branch missing an OptIndex arm"
                        );
                        std::ptr::addr_of_mut!((*win).w_allbuf_opt.wo_wrap) as *mut c_void
                    }
                }
            };
        }
        return get_option(opt_idx).var;
    }

    if (opt_flags & crate::option_defs::opt_set_flags::OPT_LOCAL) != 0 && option_is_global_local(opt_idx) {
        // Force the local value regardless of whether it's currently
        // "unset" (unlike get_varp_from's own fallback branches, which
        // check for that) - matches the original's own separate switch
        // exactly. Deliberately omits `kOptTagfunc`, present in the
        // original's switch but unreachable there: `tagfunc`'s real,
        // verified `scope_flags` (buf-only, no global scope - see
        // OPTIONS's own [309] entry) make `option_is_global_local`
        // always false for it, so this guard would never let control
        // reach that case in the original either - a likely harmless
        // upstream leftover, not replicated as dead code here.
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe {
            match opt_idx {
        OptIndex::Equalprg => std::ptr::addr_of_mut!((*buf).b_p_ep) as *mut c_void,
        OptIndex::Keywordprg => std::ptr::addr_of_mut!((*buf).b_p_kp) as *mut c_void,
        OptIndex::Path => std::ptr::addr_of_mut!((*buf).b_p_path) as *mut c_void,
        OptIndex::Autocomplete => std::ptr::addr_of_mut!((*buf).b_p_ac) as *mut c_void,
        OptIndex::Autoread => std::ptr::addr_of_mut!((*buf).b_p_ar) as *mut c_void,
        OptIndex::Tags => std::ptr::addr_of_mut!((*buf).b_p_tags) as *mut c_void,
        OptIndex::Tagcase => std::ptr::addr_of_mut!((*buf).b_p_tc) as *mut c_void,
        OptIndex::Sidescrolloff => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_siso) as *mut c_void,
        OptIndex::Scrolloff => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_so) as *mut c_void,
        OptIndex::Scrolloffpad => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_sop) as *mut c_void,
        OptIndex::Backupcopy => std::ptr::addr_of_mut!((*buf).b_p_bkc) as *mut c_void,
        OptIndex::Define => std::ptr::addr_of_mut!((*buf).b_p_def) as *mut c_void,
        OptIndex::Include => std::ptr::addr_of_mut!((*buf).b_p_inc) as *mut c_void,
        OptIndex::Completeopt => std::ptr::addr_of_mut!((*buf).b_p_cot) as *mut c_void,
        OptIndex::Dictionary => std::ptr::addr_of_mut!((*buf).b_p_dict) as *mut c_void,
        OptIndex::Diffanchors => std::ptr::addr_of_mut!((*buf).b_p_dia) as *mut c_void,
        OptIndex::Thesaurus => std::ptr::addr_of_mut!((*buf).b_p_tsr) as *mut c_void,
        OptIndex::Thesaurusfunc => std::ptr::addr_of_mut!((*buf).b_p_tsrfu) as *mut c_void,
        OptIndex::Formatprg => std::ptr::addr_of_mut!((*buf).b_p_fp) as *mut c_void,
        OptIndex::Fsync => std::ptr::addr_of_mut!((*buf).b_p_fs) as *mut c_void,
        OptIndex::Findfunc => std::ptr::addr_of_mut!((*buf).b_p_ffu) as *mut c_void,
        OptIndex::Errorformat => std::ptr::addr_of_mut!((*buf).b_p_efm) as *mut c_void,
        OptIndex::Grepformat => std::ptr::addr_of_mut!((*buf).b_p_gefm) as *mut c_void,
        OptIndex::Grepprg => std::ptr::addr_of_mut!((*buf).b_p_gp) as *mut c_void,
        OptIndex::Makeprg => std::ptr::addr_of_mut!((*buf).b_p_mp) as *mut c_void,
        OptIndex::Showbreak => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_sbr) as *mut c_void,
        OptIndex::Statusline => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_stl) as *mut c_void,
        OptIndex::Winbar => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_wbr) as *mut c_void,
        OptIndex::Undolevels => std::ptr::addr_of_mut!((*buf).b_p_ul) as *mut c_void,
        OptIndex::Lispwords => std::ptr::addr_of_mut!((*buf).b_p_lw) as *mut c_void,
        OptIndex::Makeencoding => std::ptr::addr_of_mut!((*buf).b_p_menc) as *mut c_void,
        OptIndex::Fillchars => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fcs) as *mut c_void,
        OptIndex::Listchars => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_lcs) as *mut c_void,
        OptIndex::Virtualedit => std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_ve) as *mut c_void,
                _ => {
                    debug_assert!(false, "get_varp_scope_from: OPT_LOCAL global-local branch missing an OptIndex arm");
                    get_option(opt_idx).var
                }
            }
        };
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { get_varp_from(opt_idx, buf, win) }
}

/// Get pointer to option variable, depending on local or global scope,
/// using the current buffer/window (`get_varp_scope`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf`/`curwin` must be valid, non-null
/// pointers to live `BufT`/`WinT` values.
#[must_use]
pub unsafe fn get_varp_scope(opt_idx: OptIndex, opt_flags: u32) -> *mut c_void {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { get_varp_scope_from(opt_idx, opt_flags, globals.curbuf, globals.curwin) }
}

/// Create an `OptVal` from a var pointer (`optval_from_varp`).
///
/// # Safety
/// `varp` must be a valid, non-null pointer of the correct concrete
/// type for `opt_idx`'s own declared `OptValType` - `*mut i32` for
/// Boolean, `*mut OptInt` for Number, `*mut Option<Vec<u8>>` for
/// String - exactly what `get_varp_from`/`get_varp` always produce
/// (independently verified field-by-field against every one of
/// `get_varp_from`'s 145 branches before trusting this type-punning
/// approach, given a `*mut c_void` erases which concrete type is on
/// the other end).
///
/// Also requires `crate::globals::GLOBALS.curbuf` to be null OR a
/// valid, non-null pointer to a live `BufT` (used only for the
/// `'modified'`/`b_changed` special case below).
#[must_use]
pub unsafe fn optval_from_varp(opt_idx: OptIndex, varp: *mut c_void) -> OptVal {
    // Special case: 'modified' is b_changed, but we also want to
    // consider it set when 'fileformat'/'fileencoding' changed.
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    if !curbuf.is_null() {
        // SAFETY: curbuf just checked non-null; forwarded from this
        // function's own safety doc.
        let b_changed_ptr = unsafe { std::ptr::addr_of_mut!((*curbuf).b_changed) as *mut c_void };
        if varp == b_changed_ptr {
            // SAFETY: forwarded from this function's own safety doc.
            let changed = unsafe { crate::undo::curbuf_is_changed() };
            return OptVal::Boolean(if changed { TriState::True } else { TriState::False });
        }
    }

    match get_option(opt_idx).r#type {
        OptValType::Nil => OptVal::Nil,
        OptValType::Boolean => {
            // SAFETY: forwarded from this function's own safety doc.
            let v = unsafe { *(varp as *mut i32) };
            OptVal::Boolean(crate::types_defs::tristate_from_int(v as i64))
        }
        OptValType::Number => {
            // SAFETY: forwarded from this function's own safety doc.
            OptVal::Number(unsafe { *(varp as *mut OptInt) })
        }
        OptValType::String => {
            // SAFETY: forwarded from this function's own safety doc.
            let s = unsafe { &*(varp as *mut Option<Vec<u8>>) };
            OptVal::String(s.clone().unwrap_or_default())
        }
    }
}

/// Set option var pointer value from an `OptVal` (`set_option_varp`).
///
/// Deliberately drops the original's `free_oldval: bool` parameter.
/// `free_oldval` exists purely to navigate C's MANUAL memory
/// management around [`optval_from_varp`]'s OWN `String` case
/// ALIASING (not copying) the existing `char *` at `varp` in the
/// original - some callers capture an `old_value` that still points
/// at the SAME buffer `varp` currently holds, and must avoid a
/// double-free/premature-free until that alias is done being read.
/// This crate's own `optval_from_varp` (see its own doc comment)
/// instead CLONES the string out (a real, independent `Vec<u8>`, no
/// aliasing) - so there is no aliasing hazard to navigate here:
/// assigning `*varp = ...` through an `Option<Vec<u8>>`-typed pointer
/// already correctly drops whatever `Vec<u8>` was previously there
/// (Rust's ordinary assignment-drops-the-previous-place-value
/// semantics), unconditionally, for free - exactly matching
/// `free_oldval: true`'s behavior in EVERY call site, since no caller
/// needs the aliasing-avoidance `false` behavior once cloning
/// replaces aliasing.
///
/// # Safety
/// `varp` must be a valid, non-null pointer of the correct concrete
/// type for `value`'s own `OptVal` variant - see [`optval_from_varp`]'s
/// own safety doc for the exact type-per-variant mapping (the same
/// mapping applies symmetrically here, for writes).
///
/// # Panics
/// Debug-asserts `option_has_type(opt_idx, value.value_type())`,
/// matching the original's own `assert`. Panics via `unreachable!()`
/// for `OptVal::Nil`, matching the original's own `abort()` - the
/// original documents this case as "not a real option value", only
/// ever produced transiently and never actually written through a
/// `varp` (`kOptValTypeNil` exists solely so `Nil` can't be
/// bitshifted and mistaken for a real option type flag).
pub unsafe fn set_option_varp(opt_idx: OptIndex, varp: *mut c_void, value: OptVal) {
    debug_assert!(option_has_type(opt_idx, value.value_type()));
    match value {
        OptVal::Nil => unreachable!("set_option_varp: Nil OptVal (matches the original's own abort())"),
        OptVal::Boolean(b) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { *(varp as *mut i32) = b as i32 };
        }
        OptVal::Number(n) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { *(varp as *mut OptInt) = n };
        }
        OptVal::String(s) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { *(varp as *mut Option<Vec<u8>>) = Some(s) };
        }
    }
}

/// Get the value of an option (`get_option_value`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf`/`curwin` must be valid, non-null
/// pointers to live `BufT`/`WinT` values.
#[must_use]
pub unsafe fn get_option_value(opt_idx: OptIndex, opt_flags: u32) -> OptVal {
    if opt_idx == OptIndex::Invalid {
        return OptVal::Nil;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { get_varp_scope(opt_idx, opt_flags) };
    // SAFETY: get_varp_scope always returns a pointer of the type
    // optval_from_varp expects for opt_idx's own declared type.
    unsafe { optval_from_varp(opt_idx, varp) }
}

/// The `'term'` value (`p_term`, a file-local `static char *` in the
/// original - not an `EXTERN` global, same treatment as `os/env.rs`'s
/// own `HOMEDIR` precedent for a file-static piece of state that
/// doesn't belong in the shared `OptionVars`/`Globals` struct bags).
static P_TERM: std::sync::LazyLock<crate::globals::GlobalCell<Option<Vec<u8>>>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(None));

/// The `'ttytype'` value (`p_ttytype`, file-local in the original -
/// see [`P_TERM`]'s own doc comment).
static P_TTYTYPE: std::sync::LazyLock<crate::globals::GlobalCell<Option<Vec<u8>>>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(None));

/// Skip over the name of a TTY option or keycode option
/// (`find_tty_option_end`).
///
/// Returns `None` when `arg` isn't (wholly, or as a `t_xx`/`<t_xx>`
/// prefix) a TTY or keycode option name. Otherwise, the byte offset
/// into `arg` just past the option name - NOT necessarily `arg.len()`
/// for the `t_xx`/`<t_xx>` keycode forms, which may have trailing
/// bytes after the returned offset when embedded in a larger `:set`
/// argument string.
///
/// `arg == b"term"`/`arg == b"ttytype"` deliberately require an EXACT,
/// WHOLE-slice match (matching the original's own `strequal`, a full
/// `strcmp`, not a prefix check) - this only recognizes a BARE
/// `"term"`/`"ttytype"` with nothing else following (e.g. for
/// `:set term?`), NOT `"term=xterm"` (that reaches this function too,
/// via the not-yet-translated `find_option_end`, but falls through to
/// an ordinary alpha-name scan there instead, per the original's own
/// logic).
fn find_tty_option_end(arg: &[u8]) -> Option<usize> {
    if arg == b"term" {
        return Some(4);
    }
    if arg == b"ttytype" {
        return Some(7);
    }

    let mut p = 0usize;
    let mut delimit = false; // whether to delimit '<'

    if arg.first() == Some(&b'<') {
        // look out for <t_>;>
        delimit = true;
        p += 1;
    }
    if arg.get(p) == Some(&b't') && arg.get(p + 1) == Some(&b'_') && arg.get(p + 2).is_some() && arg.get(p + 3).is_some()
    {
        // "t_xx" ("t_Co") option.
        p += 4;
    } else if delimit {
        // Search for delimiting '>'.
        while p < arg.len() && arg[p] != b'>' {
            p += 1;
        }
    }
    // Return None when delimiting '>' is not found.
    if delimit {
        if arg.get(p) != Some(&b'>') {
            return None;
        }
        p += 1;
    }

    if p == 0 { None } else { Some(p) }
}

/// Whether `name` is (wholly) a TTY option or keycode option name
/// (`is_tty_option`).
#[must_use]
pub fn is_tty_option(name: &[u8]) -> bool {
    find_tty_option_end(name).is_some()
}

/// Get value of TTY option (`get_tty_option`).
///
/// Returns `OptVal::Nil` if `name` isn't a TTY option, matching the
/// original's own `NIL_OPTVAL` return (rather than this crate's
/// `Option<OptVal>` convention elsewhere) since `OptVal` itself
/// already has a dedicated `Nil` variant for exactly this "no value"
/// case - the original's own caller (`get_option_value_for`) already
/// treats `NIL_OPTVAL` as the "not found" signal, so mirroring that
/// exactly (rather than wrapping in `Option`) keeps this a direct,
/// literal translation.
#[must_use]
pub fn get_tty_option(name: &[u8]) -> OptVal {
    if name == b"t_Co" {
        let t_colors = unsafe { crate::globals::GLOBALS.get_mut() }.t_colors;
        let value = if t_colors <= 1 { Vec::new() } else { t_colors.to_string().into_bytes() };
        return OptVal::String(value);
    }
    if name == b"term" {
        let value = unsafe { P_TERM.get_mut() }.clone();
        return OptVal::String(value.unwrap_or_else(|| b"nvim".to_vec()));
    }
    if name == b"ttytype" {
        let value = unsafe { P_TTYTYPE.get_mut() }.clone();
        return OptVal::String(value.unwrap_or_else(|| b"nvim".to_vec()));
    }
    if is_tty_option(name) {
        // XXX: All other t_* options were removed in 3baba1e7.
        return OptVal::String(Vec::new());
    }
    OptVal::Nil
}

/// Set value of TTY option (`set_tty_option`). Returns `false` when
/// `name` isn't a settable TTY option (only `'term'`/`'ttytype'` are).
pub fn set_tty_option(name: &[u8], value: Vec<u8>) -> bool {
    if name == b"term" {
        unsafe { *P_TERM.get_mut() = Some(value) };
        return true;
    }
    if name == b"ttytype" {
        unsafe { *P_TTYTYPE.get_mut() = Some(value) };
        return true;
    }
    false
}

/// Every recognized option name (both full names and short
/// abbreviations, e.g. `"equalalways"` and `"ea"` both map to the
/// same entry) mapped to its `OptIndex` (`option_hash_elems[]` +
/// `find_option_hash`, both entirely machine-generated in the
/// original from `options.lua` via `src/gen/gen_options.lua`, into
/// `build/.../options_map.generated.h`).
///
/// Uses a plain `HashMap` instead of literally replicating the
/// original's own hand-rolled length-plus-last/first-character
/// dispatch tree (narrowing to a small `[low, high)` range within a
/// flat array, then a linear `memcmp` scan) - that structure is a
/// pure C performance micro-optimization over the exact same
/// (name, `OptIndex`) data this table holds, not a simplification of
/// any business logic; a `HashMap` gives the same O(1)-ish lookup
/// far more simply and with no hand-rolled dispatch tree to
/// transcribe or verify. The 725-entry DATA itself (every real name
/// -> `OptIndex` mapping, including historical aliases like
/// `"viminfo"`/`"vi"` for `shada`) is mechanically transcribed and
/// cross-checked in full: exactly 725 entries, all unique names, all
/// 725 `OptIndex` references resolved against the real enum with
/// zero gaps.
pub static OPTION_HASH_ELEMS: std::sync::LazyLock<std::collections::HashMap<&'static [u8], OptIndex>> =
    std::sync::LazyLock::new(|| {
        [
            (&b"ea"[..], OptIndex::Equalalways),
            (&b"wa"[..], OptIndex::Writeany),
            (&b"pa"[..], OptIndex::Path),
            (&b"ma"[..], OptIndex::Modifiable),
            (&b"wb"[..], OptIndex::Writebackup),
            (&b"cb"[..], OptIndex::Clipboard),
            (&b"vb"[..], OptIndex::Visualbell),
            (&b"sb"[..], OptIndex::Splitbelow),
            (&b"pb"[..], OptIndex::Pumblend),
            (&b"eb"[..], OptIndex::Errorbells),
            (&b"sc"[..], OptIndex::Showcmd),
            (&b"tc"[..], OptIndex::Tagcase),
            (&b"wc"[..], OptIndex::Wildchar),
            (&b"uc"[..], OptIndex::Updatecount),
            (&b"ac"[..], OptIndex::Autocomplete),
            (&b"cc"[..], OptIndex::Colorcolumn),
            (&b"ic"[..], OptIndex::Ignorecase),
            (&b"gd"[..], OptIndex::Gdefault),
            (&b"cd"[..], OptIndex::Cdpath),
            (&b"wd"[..], OptIndex::Writedelay),
            (&b"ed"[..], OptIndex::Edcompatible),
            (&b"sd"[..], OptIndex::Shada),
            (&b"ve"[..], OptIndex::Virtualedit),
            (&b"re"[..], OptIndex::Regexpengine),
            (&b"qe"[..], OptIndex::Quoteescape),
            (&b"hf"[..], OptIndex::Helpfile),
            (&b"ef"[..], OptIndex::Errorfile),
            (&b"ff"[..], OptIndex::Fileformat),
            (&b"tf"[..], OptIndex::Ttyfast),
            (&b"cf"[..], OptIndex::Confirm),
            (&b"nf"[..], OptIndex::Nrformats),
            (&b"dg"[..], OptIndex::Digraph),
            (&b"bg"[..], OptIndex::Background),
            (&b"hh"[..], OptIndex::Helpheight),
            (&b"wh"[..], OptIndex::Winheight),
            (&b"ph"[..], OptIndex::Pumheight),
            (&b"ch"[..], OptIndex::Cmdheight),
            (&b"bh"[..], OptIndex::Bufhidden),
            (&b"mh"[..], OptIndex::Mousehide),
            (&b"sh"[..], OptIndex::Shell),
            (&b"ei"[..], OptIndex::Eventignore),
            (&b"pi"[..], OptIndex::Preserveindent),
            (&b"hi"[..], OptIndex::History),
            (&b"wi"[..], OptIndex::Window),
            (&b"si"[..], OptIndex::Smartindent),
            (&b"ai"[..], OptIndex::Autoindent),
            (&b"ri"[..], OptIndex::Revins),
            (&b"ci"[..], OptIndex::Copyindent),
            (&b"vi"[..], OptIndex::Shada),
            (&b"sj"[..], OptIndex::Scrolljump),
            (&b"hk"[..], OptIndex::Hkmap),
            (&b"bk"[..], OptIndex::Backup),
            (&b"ul"[..], OptIndex::Undolevels),
            (&b"al"[..], OptIndex::Aleph),
            (&b"tl"[..], OptIndex::Taglength),
            (&b"rl"[..], OptIndex::Rightleft),
            (&b"bl"[..], OptIndex::Buflisted),
            (&b"ml"[..], OptIndex::Modeline),
            (&b"hl"[..], OptIndex::Highlight),
            (&b"wm"[..], OptIndex::Wrapmargin),
            (&b"tm"[..], OptIndex::Timeoutlen),
            (&b"sm"[..], OptIndex::Showmatch),
            (&b"pm"[..], OptIndex::Patchmode),
            (&b"im"[..], OptIndex::Insertmode),
            (&b"lm"[..], OptIndex::Langmenu),
            (&b"km"[..], OptIndex::Keymodel),
            (&b"fo"[..], OptIndex::Formatoptions),
            (&b"to"[..], OptIndex::Timeout),
            (&b"co"[..], OptIndex::Columns),
            (&b"bo"[..], OptIndex::Belloff),
            (&b"go"[..], OptIndex::Guioptions),
            (&b"so"[..], OptIndex::Scrolloff),
            (&b"ro"[..], OptIndex::Readonly),
            (&b"fp"[..], OptIndex::Formatprg),
            (&b"gp"[..], OptIndex::Grepprg),
            (&b"wp"[..], OptIndex::Winpinned),
            (&b"pp"[..], OptIndex::Packpath),
            (&b"cp"[..], OptIndex::Compatible),
            (&b"kp"[..], OptIndex::Keywordprg),
            (&b"mp"[..], OptIndex::Makeprg),
            (&b"ep"[..], OptIndex::Equalprg),
            (&b"sp"[..], OptIndex::Shellpipe),
            (&b"tr"[..], OptIndex::Tagrelative),
            (&b"ur"[..], OptIndex::Undoreload),
            (&b"ar"[..], OptIndex::Autoread),
            (&b"sr"[..], OptIndex::Shiftround),
            (&b"ws"[..], OptIndex::Wrapscan),
            (&b"ts"[..], OptIndex::Tabstop),
            (&b"ss"[..], OptIndex::Sidescroll),
            (&b"js"[..], OptIndex::Joinspaces),
            (&b"bs"[..], OptIndex::Backspace),
            (&b"fs"[..], OptIndex::Fsync),
            (&b"is"[..], OptIndex::Incsearch),
            (&b"ls"[..], OptIndex::Laststatus),
            (&b"et"[..], OptIndex::Expandtab),
            (&b"ut"[..], OptIndex::Updatetime),
            (&b"pt"[..], OptIndex::Pastetoggle),
            (&b"bt"[..], OptIndex::Buftype),
            (&b"ft"[..], OptIndex::Filetype),
            (&b"ru"[..], OptIndex::Ruler),
            (&b"su"[..], OptIndex::Suffixes),
            (&b"nu"[..], OptIndex::Number),
            (&b"sw"[..], OptIndex::Shiftwidth),
            (&b"ww"[..], OptIndex::Whichwrap),
            (&b"tw"[..], OptIndex::Textwidth),
            (&b"pw"[..], OptIndex::Pumwidth),
            (&b"aw"[..], OptIndex::Autowrite),
            (&b"lw"[..], OptIndex::Lispwords),
            (&b"ex"[..], OptIndex::Exrc),
            (&b"dy"[..], OptIndex::Display),
            (&b"lz"[..], OptIndex::Lazyredraw),
            (&b"sta"[..], OptIndex::Smarttab),
            (&b"awa"[..], OptIndex::Autowriteall),
            (&b"sua"[..], OptIndex::Suffixesadd),
            (&b"cia"[..], OptIndex::Completeitemalign),
            (&b"dia"[..], OptIndex::Diffanchors),
            (&b"wfb"[..], OptIndex::Winfixbuf),
            (&b"swb"[..], OptIndex::Switchbuf),
            (&b"scb"[..], OptIndex::Scrollbind),
            (&b"rdb"[..], OptIndex::Redrawdebug),
            (&b"crb"[..], OptIndex::Cursorbind),
            (&b"enc"[..], OptIndex::Encoding),
            (&b"smc"[..], OptIndex::Synmaxcol),
            (&b"tgc"[..], OptIndex::Termguicolors),
            (&b"stc"[..], OptIndex::Statuscolumn),
            (&b"imc"[..], OptIndex::Imcmdline),
            (&b"inc"[..], OptIndex::Include),
            (&b"spc"[..], OptIndex::Spellcapcheck),
            (&b"wic"[..], OptIndex::Wildignorecase),
            (&b"bkc"[..], OptIndex::Backupcopy),
            (&b"msc"[..], OptIndex::Maxsearchcount),
            (&b"cuc"[..], OptIndex::Cursorcolumn),
            (&b"fic"[..], OptIndex::Fileignorecase),
            (&b"fdc"[..], OptIndex::Foldcolumn),
            (&b"rlc"[..], OptIndex::Rightleftcmd),
            (&b"ead"[..], OptIndex::Eadirection),
            (&b"hid"[..], OptIndex::Hidden),
            (&b"smd"[..], OptIndex::Showmode),
            (&b"imd"[..], OptIndex::Imdisable),
            (&b"acd"[..], OptIndex::Autochdir),
            (&b"mfd"[..], OptIndex::Maxfuncdepth),
            (&b"mmd"[..], OptIndex::Maxmapdepth),
            (&b"mod"[..], OptIndex::Modified),
            (&b"sxe"[..], OptIndex::Shellxescape),
            (&b"mle"[..], OptIndex::Modelineexpr),
            (&b"fde"[..], OptIndex::Foldexpr),
            (&b"eof"[..], OptIndex::Endoffile),
            (&b"ruf"[..], OptIndex::Rulerformat),
            (&b"swf"[..], OptIndex::Swapfile),
            (&b"spf"[..], OptIndex::Spellfile),
            (&b"tpf"[..], OptIndex::Termpastefilter),
            (&b"plf"[..], OptIndex::Packlockfile),
            (&b"udf"[..], OptIndex::Undofile),
            (&b"inf"[..], OptIndex::Infercase),
            (&b"mef"[..], OptIndex::Makeef),
            (&b"isf"[..], OptIndex::Isfname),
            (&b"sdf"[..], OptIndex::Shadafile),
            (&b"vif"[..], OptIndex::Shadafile),
            (&b"def"[..], OptIndex::Define),
            (&b"tag"[..], OptIndex::Tags),
            (&b"hlg"[..], OptIndex::Helplang),
            (&b"wig"[..], OptIndex::Wildignore),
            (&b"cdh"[..], OptIndex::Cdhome),
            (&b"wfh"[..], OptIndex::Winfixheight),
            (&b"wmh"[..], OptIndex::Winminheight),
            (&b"pvh"[..], OptIndex::Previewheight),
            (&b"cwh"[..], OptIndex::Cmdwinheight),
            (&b"chi"[..], OptIndex::Chistory),
            (&b"ari"[..], OptIndex::Allowrevins),
            (&b"imi"[..], OptIndex::Iminsert),
            (&b"isi"[..], OptIndex::Isident),
            (&b"bri"[..], OptIndex::Breakindent),
            (&b"lhi"[..], OptIndex::Lhistory),
            (&b"fdi"[..], OptIndex::Foldignore),
            (&b"wak"[..], OptIndex::Winaltkeys),
            (&b"spk"[..], OptIndex::Splitkeep),
            (&b"bsk"[..], OptIndex::Backupskip),
            (&b"brk"[..], OptIndex::Breakat),
            (&b"isk"[..], OptIndex::Iskeyword),
            (&b"gtl"[..], OptIndex::Guitablabel),
            (&b"sel"[..], OptIndex::Selection),
            (&b"fml"[..], OptIndex::Foldminlines),
            (&b"stl"[..], OptIndex::Statusline),
            (&b"tcl"[..], OptIndex::Tabclose),
            (&b"sol"[..], OptIndex::Startofline),
            (&b"scl"[..], OptIndex::Signcolumn),
            (&b"spl"[..], OptIndex::Spelllang),
            (&b"acl"[..], OptIndex::Autocompletedelay),
            (&b"tal"[..], OptIndex::Tabline),
            (&b"csl"[..], OptIndex::Completeslash),
            (&b"eol"[..], OptIndex::Endofline),
            (&b"lpl"[..], OptIndex::Loadplugins),
            (&b"ssl"[..], OptIndex::Shellslash),
            (&b"fcl"[..], OptIndex::Foldclose),
            (&b"cul"[..], OptIndex::Cursorline),
            (&b"fdl"[..], OptIndex::Foldlevel),
            (&b"gfm"[..], OptIndex::Grepformat),
            (&b"efm"[..], OptIndex::Errorformat),
            (&b"wim"[..], OptIndex::Wildmode),
            (&b"ttm"[..], OptIndex::Ttimeoutlen),
            (&b"icm"[..], OptIndex::Inccommand),
            (&b"wcm"[..], OptIndex::Wildcharm),
            (&b"com"[..], OptIndex::Comments),
            (&b"tpm"[..], OptIndex::Tabpagemax),
            (&b"slm"[..], OptIndex::Selectmode),
            (&b"msm"[..], OptIndex::Mkspellmem),
            (&b"lrm"[..], OptIndex::Langremap),
            (&b"shm"[..], OptIndex::Shortmess),
            (&b"fdm"[..], OptIndex::Foldmethod),
            (&b"fdn"[..], OptIndex::Foldnestmax),
            (&b"gfn"[..], OptIndex::Guifont),
            (&b"cin"[..], OptIndex::Cindent),
            (&b"syn"[..], OptIndex::Syntax),
            (&b"bin"[..], OptIndex::Binary),
            (&b"fen"[..], OptIndex::Foldenable),
            (&b"fdo"[..], OptIndex::Foldopen),
            (&b"emo"[..], OptIndex::Emoji),
            (&b"sbo"[..], OptIndex::Scrollopt),
            (&b"spo"[..], OptIndex::Spelloptions),
            (&b"cto"[..], OptIndex::Completetimeout),
            (&b"mco"[..], OptIndex::Maxcombine),
            (&b"cpo"[..], OptIndex::Cpoptions),
            (&b"flp"[..], OptIndex::Formatlistpat),
            (&b"cmp"[..], OptIndex::Casemap),
            (&b"hkp"[..], OptIndex::Hkmapp),
            (&b"sop"[..], OptIndex::Scrolloffpad),
            (&b"wop"[..], OptIndex::Wildoptions),
            (&b"vop"[..], OptIndex::Viewoptions),
            (&b"top"[..], OptIndex::Tildeop),
            (&b"isp"[..], OptIndex::Isprint),
            (&b"jop"[..], OptIndex::Jumpoptions),
            (&b"kmp"[..], OptIndex::Keymap),
            (&b"rtp"[..], OptIndex::Runtimepath),
            (&b"lop"[..], OptIndex::Lispoptions),
            (&b"lsp"[..], OptIndex::Linespace),
            (&b"mmp"[..], OptIndex::Maxmempattern),
            (&b"dip"[..], OptIndex::Diffopt),
            (&b"sxq"[..], OptIndex::Shellxquote),
            (&b"shq"[..], OptIndex::Shellquote),
            (&b"dir"[..], OptIndex::Directory),
            (&b"gcr"[..], OptIndex::Guicursor),
            (&b"sbr"[..], OptIndex::Showbreak),
            (&b"wbr"[..], OptIndex::Winbar),
            (&b"tsr"[..], OptIndex::Thesaurus),
            (&b"spr"[..], OptIndex::Splitright),
            (&b"scr"[..], OptIndex::Scroll),
            (&b"lbr"[..], OptIndex::Linebreak),
            (&b"srr"[..], OptIndex::Shellredir),
            (&b"lnr"[..], OptIndex::Langnoremap),
            (&b"fmr"[..], OptIndex::Foldmarker),
            (&b"hls"[..], OptIndex::Hlsearch),
            (&b"vts"[..], OptIndex::Vartabstop),
            (&b"tbs"[..], OptIndex::Tagbsearch),
            (&b"sps"[..], OptIndex::Spellsuggest),
            (&b"vbs"[..], OptIndex::Verbose),
            (&b"ims"[..], OptIndex::Imsearch),
            (&b"sts"[..], OptIndex::Softtabstop),
            (&b"sms"[..], OptIndex::Smoothscroll),
            (&b"scs"[..], OptIndex::Smartcase),
            (&b"cms"[..], OptIndex::Commentstring),
            (&b"lcs"[..], OptIndex::Listchars),
            (&b"mps"[..], OptIndex::Matchpairs),
            (&b"mis"[..], OptIndex::Menuitems),
            (&b"ffs"[..], OptIndex::Fileformats),
            (&b"fcs"[..], OptIndex::Fillchars),
            (&b"mls"[..], OptIndex::Modelines),
            (&b"fdt"[..], OptIndex::Foldtext),
            (&b"gtt"[..], OptIndex::Guitabtooltip),
            (&b"sft"[..], OptIndex::Showfulltag),
            (&b"act"[..], OptIndex::Autocompletetimeout),
            (&b"cpt"[..], OptIndex::Complete),
            (&b"cot"[..], OptIndex::Completeopt),
            (&b"mat"[..], OptIndex::Matchtime),
            (&b"rdt"[..], OptIndex::Redrawtime),
            (&b"tfu"[..], OptIndex::Tagfunc),
            (&b"cfu"[..], OptIndex::Completefunc),
            (&b"ffu"[..], OptIndex::Findfunc),
            (&b"ofu"[..], OptIndex::Omnifunc),
            (&b"rnu"[..], OptIndex::Relativenumber),
            (&b"ccv"[..], OptIndex::Charconvert),
            (&b"gfw"[..], OptIndex::Guifontwide),
            (&b"wfw"[..], OptIndex::Winfixwidth),
            (&b"wmw"[..], OptIndex::Winminwidth),
            (&b"wiw"[..], OptIndex::Winwidth),
            (&b"pmw"[..], OptIndex::Pummaxwidth),
            (&b"pvw"[..], OptIndex::Previewwindow),
            (&b"nuw"[..], OptIndex::Numberwidth),
            (&b"eiw"[..], OptIndex::Eventignorewin),
            (&b"fex"[..], OptIndex::Formatexpr),
            (&b"pex"[..], OptIndex::Patchexpr),
            (&b"pyx"[..], OptIndex::Pyxversion),
            (&b"bex"[..], OptIndex::Backupext),
            (&b"dex"[..], OptIndex::Diffexpr),
            (&b"para"[..], OptIndex::Paragraphs),
            (&b"arab"[..], OptIndex::Arabic),
            (&b"bomb"[..], OptIndex::Bomb),
            (&b"exrc"[..], OptIndex::Exrc),
            (&b"sloc"[..], OptIndex::Showcmdloc),
            (&b"tenc"[..], OptIndex::Termencoding),
            (&b"fenc"[..], OptIndex::Fileencoding),
            (&b"menc"[..], OptIndex::Makeencoding),
            (&b"cole"[..], OptIndex::Conceallevel),
            (&b"more"[..], OptIndex::More),
            (&b"inde"[..], OptIndex::Indentexpr),
            (&b"qftf"[..], OptIndex::Quickfixtextfunc),
            (&b"shcf"[..], OptIndex::Shellcmdflag),
            (&b"diff"[..], OptIndex::Diff),
            (&b"path"[..], OptIndex::Path),
            (&b"cink"[..], OptIndex::Cinkeys),
            (&b"scbk"[..], OptIndex::Scrollback),
            (&b"indk"[..], OptIndex::Indentkeys),
            (&b"stal"[..], OptIndex::Showtabline),
            (&b"warn"[..], OptIndex::Warn),
            (&b"icon"[..], OptIndex::Icon),
            (&b"siso"[..], OptIndex::Sidescrolloff),
            (&b"cino"[..], OptIndex::Cinoptions),
            (&b"deco"[..], OptIndex::Delcombine),
            (&b"wrap"[..], OptIndex::Wrap),
            (&b"stmp"[..], OptIndex::Shelltemp),
            (&b"lmap"[..], OptIndex::Langmap),
            (&b"lisp"[..], OptIndex::Lisp),
            (&b"ssop"[..], OptIndex::Sessionoptions),
            (&b"vdir"[..], OptIndex::Viewdir),
            (&b"udir"[..], OptIndex::Undodir),
            (&b"bdir"[..], OptIndex::Backupdir),
            (&b"vsts"[..], OptIndex::Varsofttabstop),
            (&b"tags"[..], OptIndex::Tags),
            (&b"fdls"[..], OptIndex::Foldlevelstart),
            (&b"sect"[..], OptIndex::Sections),
            (&b"list"[..], OptIndex::List),
            (&b"tgst"[..], OptIndex::Tagstack),
            (&b"mopt"[..], OptIndex::Messagesopt),
            (&b"dict"[..], OptIndex::Dictionary),
            (&b"wmnu"[..], OptIndex::Wildmenu),
            (&b"cocu"[..], OptIndex::Concealcursor),
            (&b"odev"[..], OptIndex::Opendevice),
            (&b"cinw"[..], OptIndex::Cinwords),
            (&b"ambw"[..], OptIndex::Ambiwidth),
            (&b"inex"[..], OptIndex::Includeexpr),
            (&b"busy"[..], OptIndex::Busy),
            (&b"aleph"[..], OptIndex::Aleph),
            (&b"bsdir"[..], OptIndex::Browsedir),
            (&b"cedit"[..], OptIndex::Cedit),
            (&b"cinsd"[..], OptIndex::Cinscopedecls),
            (&b"debug"[..], OptIndex::Debug),
            (&b"emoji"[..], OptIndex::Emoji),
            (&b"fsync"[..], OptIndex::Fsync),
            (&b"fencs"[..], OptIndex::Fileencodings),
            (&b"hkmap"[..], OptIndex::Hkmap),
            (&b"lines"[..], OptIndex::Lines),
            (&b"magic"[..], OptIndex::Magic),
            (&b"mouse"[..], OptIndex::Mouse),
            (&b"paste"[..], OptIndex::Paste),
            (&b"remap"[..], OptIndex::Remap),
            (&b"ruler"[..], OptIndex::Ruler),
            (&b"shell"[..], OptIndex::Shell),
            (&b"spell"[..], OptIndex::Spell),
            (&b"shada"[..], OptIndex::Shada),
            (&b"tbidi"[..], OptIndex::Termbidi),
            (&b"terse"[..], OptIndex::Terse),
            (&b"tsrfu"[..], OptIndex::Thesaurusfunc),
            (&b"title"[..], OptIndex::Title),
            (&b"vfile"[..], OptIndex::Verbosefile),
            (&b"write"[..], OptIndex::Write),
            (&b"winhl"[..], OptIndex::Winhighlight),
            (&b"winbl"[..], OptIndex::Winblend),
            (&b"arabic"[..], OptIndex::Arabic),
            (&b"secure"[..], OptIndex::Secure),
            (&b"backup"[..], OptIndex::Backup),
            (&b"hidden"[..], OptIndex::Hidden),
            (&b"opfunc"[..], OptIndex::Operatorfunc),
            (&b"define"[..], OptIndex::Define),
            (&b"cdhome"[..], OptIndex::Cdhome),
            (&b"briopt"[..], OptIndex::Breakindentopt),
            (&b"makeef"[..], OptIndex::Makeef),
            (&b"culopt"[..], OptIndex::Cursorlineopt),
            (&b"hkmapp"[..], OptIndex::Hkmapp),
            (&b"number"[..], OptIndex::Number),
            (&b"window"[..], OptIndex::Window),
            (&b"winbar"[..], OptIndex::Winbar),
            (&b"binary"[..], OptIndex::Binary),
            (&b"syntax"[..], OptIndex::Syntax),
            (&b"prompt"[..], OptIndex::Prompt),
            (&b"cdpath"[..], OptIndex::Cdpath),
            (&b"report"[..], OptIndex::Report),
            (&b"scroll"[..], OptIndex::Scroll),
            (&b"mousef"[..], OptIndex::Mousefocus),
            (&b"mousem"[..], OptIndex::Mousemodel),
            (&b"mouses"[..], OptIndex::Mouseshape),
            (&b"mouset"[..], OptIndex::Mousetime),
            (&b"revins"[..], OptIndex::Revins),
            (&b"fixeol"[..], OptIndex::Fixendofline),
            (&b"keymap"[..], OptIndex::Keymap),
            (&b"channel"[..], OptIndex::Channel),
            (&b"tabline"[..], OptIndex::Tabline),
            (&b"tabstop"[..], OptIndex::Tabstop),
            (&b"include"[..], OptIndex::Include),
            (&b"undodir"[..], OptIndex::Undodir),
            (&b"viewdir"[..], OptIndex::Viewdir),
            (&b"grepprg"[..], OptIndex::Grepprg),
            (&b"breakat"[..], OptIndex::Breakat),
            (&b"diffopt"[..], OptIndex::Diffopt),
            (&b"buftype"[..], OptIndex::Buftype),
            (&b"isfname"[..], OptIndex::Isfname),
            (&b"digraph"[..], OptIndex::Digraph),
            (&b"tagcase"[..], OptIndex::Tagcase),
            (&b"tagfunc"[..], OptIndex::Tagfunc),
            (&b"guifont"[..], OptIndex::Guifont),
            (&b"isident"[..], OptIndex::Isident),
            (&b"makeprg"[..], OptIndex::Makeprg),
            (&b"tildeop"[..], OptIndex::Tildeop),
            (&b"columns"[..], OptIndex::Columns),
            (&b"belloff"[..], OptIndex::Belloff),
            (&b"timeout"[..], OptIndex::Timeout),
            (&b"viminfo"[..], OptIndex::Shada),
            (&b"cindent"[..], OptIndex::Cindent),
            (&b"cinkeys"[..], OptIndex::Cinkeys),
            (&b"langmap"[..], OptIndex::Langmap),
            (&b"confirm"[..], OptIndex::Confirm),
            (&b"showcmd"[..], OptIndex::Showcmd),
            (&b"isprint"[..], OptIndex::Isprint),
            (&b"verbose"[..], OptIndex::Verbose),
            (&b"display"[..], OptIndex::Display),
            (&b"casemap"[..], OptIndex::Casemap),
            (&b"history"[..], OptIndex::History),
            (&b"arshape"[..], OptIndex::Arabicshape),
            (&b"ttyfast"[..], OptIndex::Ttyfast),
            (&b"autoread"[..], OptIndex::Autoread),
            (&b"chistory"[..], OptIndex::Chistory),
            (&b"comments"[..], OptIndex::Comments),
            (&b"complete"[..], OptIndex::Complete),
            (&b"cinwords"[..], OptIndex::Cinwords),
            (&b"diffexpr"[..], OptIndex::Diffexpr),
            (&b"encoding"[..], OptIndex::Encoding),
            (&b"equalprg"[..], OptIndex::Equalprg),
            (&b"foldopen"[..], OptIndex::Foldopen),
            (&b"foldtext"[..], OptIndex::Foldtext),
            (&b"findfunc"[..], OptIndex::Findfunc),
            (&b"filetype"[..], OptIndex::Filetype),
            (&b"foldexpr"[..], OptIndex::Foldexpr),
            (&b"gdefault"[..], OptIndex::Gdefault),
            (&b"helpfile"[..], OptIndex::Helpfile),
            (&b"helplang"[..], OptIndex::Helplang),
            (&b"hlsearch"[..], OptIndex::Hlsearch),
            (&b"iminsert"[..], OptIndex::Iminsert),
            (&b"imsearch"[..], OptIndex::Imsearch),
            (&b"keymodel"[..], OptIndex::Keymodel),
            (&b"langmenu"[..], OptIndex::Langmenu),
            (&b"lhistory"[..], OptIndex::Lhistory),
            (&b"modeline"[..], OptIndex::Modeline),
            (&b"mousemev"[..], OptIndex::Mousemoveevent),
            (&b"modified"[..], OptIndex::Modified),
            (&b"omnifunc"[..], OptIndex::Omnifunc),
            (&b"packpath"[..], OptIndex::Packpath),
            (&b"pumwidth"[..], OptIndex::Pumwidth),
            (&b"pumblend"[..], OptIndex::Pumblend),
            (&b"readonly"[..], OptIndex::Readonly),
            (&b"sections"[..], OptIndex::Sections),
            (&b"swapfile"[..], OptIndex::Swapfile),
            (&b"suffixes"[..], OptIndex::Suffixes),
            (&b"showmode"[..], OptIndex::Showmode),
            (&b"smarttab"[..], OptIndex::Smarttab),
            (&b"tabclose"[..], OptIndex::Tabclose),
            (&b"tagstack"[..], OptIndex::Tagstack),
            (&b"termbidi"[..], OptIndex::Termbidi),
            (&b"termsync"[..], OptIndex::Termsync),
            (&b"titlelen"[..], OptIndex::Titlelen),
            (&b"titleold"[..], OptIndex::Titleold),
            (&b"ttimeout"[..], OptIndex::Ttimeout),
            (&b"undofile"[..], OptIndex::Undofile),
            (&b"writeany"[..], OptIndex::Writeany),
            (&b"wrapscan"[..], OptIndex::Wrapscan),
            (&b"winwidth"[..], OptIndex::Winwidth),
            (&b"wildmenu"[..], OptIndex::Wildmenu),
            (&b"wildchar"[..], OptIndex::Wildchar),
            (&b"winblend"[..], OptIndex::Winblend),
            (&b"wildmode"[..], OptIndex::Wildmode),
            (&b"expandtab"[..], OptIndex::Expandtab),
            (&b"winborder"[..], OptIndex::Winborder),
            (&b"pumborder"[..], OptIndex::Pumborder),
            (&b"whichwrap"[..], OptIndex::Whichwrap),
            (&b"guicursor"[..], OptIndex::Guicursor),
            (&b"patchexpr"[..], OptIndex::Patchexpr),
            (&b"patchmode"[..], OptIndex::Patchmode),
            (&b"matchtime"[..], OptIndex::Matchtime),
            (&b"wildcharm"[..], OptIndex::Wildcharm),
            (&b"foldclose"[..], OptIndex::Foldclose),
            (&b"shadafile"[..], OptIndex::Shadafile),
            (&b"foldlevel"[..], OptIndex::Foldlevel),
            (&b"directory"[..], OptIndex::Directory),
            (&b"infercase"[..], OptIndex::Infercase),
            (&b"linespace"[..], OptIndex::Linespace),
            (&b"selection"[..], OptIndex::Selection),
            (&b"linebreak"[..], OptIndex::Linebreak),
            (&b"iskeyword"[..], OptIndex::Iskeyword),
            (&b"modelines"[..], OptIndex::Modelines),
            (&b"winfixbuf"[..], OptIndex::Winfixbuf),
            (&b"langremap"[..], OptIndex::Langremap),
            (&b"highlight"[..], OptIndex::Highlight),
            (&b"winheight"[..], OptIndex::Winheight),
            (&b"pumheight"[..], OptIndex::Pumheight),
            (&b"cmdheight"[..], OptIndex::Cmdheight),
            (&b"rightleft"[..], OptIndex::Rightleft),
            (&b"bufhidden"[..], OptIndex::Bufhidden),
            (&b"imdisable"[..], OptIndex::Imdisable),
            (&b"splitkeep"[..], OptIndex::Splitkeep),
            (&b"ambiwidth"[..], OptIndex::Ambiwidth),
            (&b"backspace"[..], OptIndex::Backspace),
            (&b"backupdir"[..], OptIndex::Backupdir),
            (&b"backupext"[..], OptIndex::Backupext),
            (&b"spellfile"[..], OptIndex::Spellfile),
            (&b"spelllang"[..], OptIndex::Spelllang),
            (&b"taglength"[..], OptIndex::Taglength),
            (&b"buflisted"[..], OptIndex::Buflisted),
            (&b"shellpipe"[..], OptIndex::Shellpipe),
            (&b"fillchars"[..], OptIndex::Fillchars),
            (&b"shelltemp"[..], OptIndex::Shelltemp),
            (&b"formatprg"[..], OptIndex::Formatprg),
            (&b"imcmdline"[..], OptIndex::Imcmdline),
            (&b"synmaxcol"[..], OptIndex::Synmaxcol),
            (&b"endoffile"[..], OptIndex::Endoffile),
            (&b"endofline"[..], OptIndex::Endofline),
            (&b"scrolloff"[..], OptIndex::Scrolloff),
            (&b"scrollopt"[..], OptIndex::Scrollopt),
            (&b"autochdir"[..], OptIndex::Autochdir),
            (&b"autowrite"[..], OptIndex::Autowrite),
            (&b"errorfile"[..], OptIndex::Errorfile),
            (&b"nrformats"[..], OptIndex::Nrformats),
            (&b"winpinned"[..], OptIndex::Winpinned),
            (&b"clipboard"[..], OptIndex::Clipboard),
            (&b"lispwords"[..], OptIndex::Lispwords),
            (&b"cpoptions"[..], OptIndex::Cpoptions),
            (&b"shortmess"[..], OptIndex::Shortmess),
            (&b"smartcase"[..], OptIndex::Smartcase),
            (&b"thesaurus"[..], OptIndex::Thesaurus),
            (&b"incsearch"[..], OptIndex::Incsearch),
            (&b"mousehide"[..], OptIndex::Mousehide),
            (&b"mousetime"[..], OptIndex::Mousetime),
            (&b"textwidth"[..], OptIndex::Textwidth),
            (&b"switchbuf"[..], OptIndex::Switchbuf),
            (&b"listchars"[..], OptIndex::Listchars),
            (&b"menuitems"[..], OptIndex::Menuitems),
            (&b"showmatch"[..], OptIndex::Showmatch),
            (&b"browsedir"[..], OptIndex::Browsedir),
            (&b"showbreak"[..], OptIndex::Showbreak),
            (&b"tagbsearch"[..], OptIndex::Tagbsearch),
            (&b"joinspaces"[..], OptIndex::Joinspaces),
            (&b"paragraphs"[..], OptIndex::Paragraphs),
            (&b"laststatus"[..], OptIndex::Laststatus),
            (&b"matchpairs"[..], OptIndex::Matchpairs),
            (&b"modifiable"[..], OptIndex::Modifiable),
            (&b"foldenable"[..], OptIndex::Foldenable),
            (&b"visualbell"[..], OptIndex::Visualbell),
            (&b"scrollback"[..], OptIndex::Scrollback),
            (&b"scrollbind"[..], OptIndex::Scrollbind),
            (&b"maxcombine"[..], OptIndex::Maxcombine),
            (&b"cursorbind"[..], OptIndex::Cursorbind),
            (&b"delcombine"[..], OptIndex::Delcombine),
            (&b"ignorecase"[..], OptIndex::Ignorecase),
            (&b"backupcopy"[..], OptIndex::Backupcopy),
            (&b"showcmdloc"[..], OptIndex::Showcmdloc),
            (&b"keywordprg"[..], OptIndex::Keywordprg),
            (&b"lazyredraw"[..], OptIndex::Lazyredraw),
            (&b"copyindent"[..], OptIndex::Copyindent),
            (&b"autoindent"[..], OptIndex::Autoindent),
            (&b"formatexpr"[..], OptIndex::Formatexpr),
            (&b"errorbells"[..], OptIndex::Errorbells),
            (&b"writedelay"[..], OptIndex::Writedelay),
            (&b"tabpagemax"[..], OptIndex::Tabpagemax),
            (&b"splitbelow"[..], OptIndex::Splitbelow),
            (&b"shellredir"[..], OptIndex::Shellredir),
            (&b"indentexpr"[..], OptIndex::Indentexpr),
            (&b"mouseshape"[..], OptIndex::Mouseshape),
            (&b"guioptions"[..], OptIndex::Guioptions),
            (&b"helpheight"[..], OptIndex::Helpheight),
            (&b"splitright"[..], OptIndex::Splitright),
            (&b"compatible"[..], OptIndex::Compatible),
            (&b"shiftwidth"[..], OptIndex::Shiftwidth),
            (&b"cinoptions"[..], OptIndex::Cinoptions),
            (&b"scrolljump"[..], OptIndex::Scrolljump),
            (&b"winaltkeys"[..], OptIndex::Winaltkeys),
            (&b"indentkeys"[..], OptIndex::Indentkeys),
            (&b"statusline"[..], OptIndex::Statusline),
            (&b"undoreload"[..], OptIndex::Undoreload),
            (&b"signcolumn"[..], OptIndex::Signcolumn),
            (&b"foldcolumn"[..], OptIndex::Foldcolumn),
            (&b"mkspellmem"[..], OptIndex::Mkspellmem),
            (&b"shellslash"[..], OptIndex::Shellslash),
            (&b"cursorline"[..], OptIndex::Cursorline),
            (&b"inccommand"[..], OptIndex::Inccommand),
            (&b"selectmode"[..], OptIndex::Selectmode),
            (&b"insertmode"[..], OptIndex::Insertmode),
            (&b"wildignore"[..], OptIndex::Wildignore),
            (&b"foldignore"[..], OptIndex::Foldignore),
            (&b"dictionary"[..], OptIndex::Dictionary),
            (&b"shiftround"[..], OptIndex::Shiftround),
            (&b"background"[..], OptIndex::Background),
            (&b"mousefocus"[..], OptIndex::Mousefocus),
            (&b"mousemodel"[..], OptIndex::Mousemodel),
            (&b"grepformat"[..], OptIndex::Grepformat),
            (&b"wrapmargin"[..], OptIndex::Wrapmargin),
            (&b"iconstring"[..], OptIndex::Iconstring),
            (&b"sidescroll"[..], OptIndex::Sidescroll),
            (&b"fileformat"[..], OptIndex::Fileformat),
            (&b"foldmarker"[..], OptIndex::Foldmarker),
            (&b"vartabstop"[..], OptIndex::Vartabstop),
            (&b"backupskip"[..], OptIndex::Backupskip),
            (&b"pyxversion"[..], OptIndex::Pyxversion),
            (&b"updatetime"[..], OptIndex::Updatetime),
            (&b"timeoutlen"[..], OptIndex::Timeoutlen),
            (&b"foldmethod"[..], OptIndex::Foldmethod),
            (&b"redrawtime"[..], OptIndex::Redrawtime),
            (&b"shellquote"[..], OptIndex::Shellquote),
            (&b"undolevels"[..], OptIndex::Undolevels),
            (&b"opendevice"[..], OptIndex::Opendevice),
            (&b"equalalways"[..], OptIndex::Equalalways),
            (&b"virtualedit"[..], OptIndex::Virtualedit),
            (&b"softtabstop"[..], OptIndex::Softtabstop),
            (&b"showtabline"[..], OptIndex::Showtabline),
            (&b"guitablabel"[..], OptIndex::Guitablabel),
            (&b"writebackup"[..], OptIndex::Writebackup),
            (&b"arabicshape"[..], OptIndex::Arabicshape),
            (&b"colorcolumn"[..], OptIndex::Colorcolumn),
            (&b"includeexpr"[..], OptIndex::Includeexpr),
            (&b"foldnestmax"[..], OptIndex::Foldnestmax),
            (&b"updatecount"[..], OptIndex::Updatecount),
            (&b"eadirection"[..], OptIndex::Eadirection),
            (&b"quoteescape"[..], OptIndex::Quoteescape),
            (&b"completeopt"[..], OptIndex::Completeopt),
            (&b"rulerformat"[..], OptIndex::Rulerformat),
            (&b"errorformat"[..], OptIndex::Errorformat),
            (&b"viminfofile"[..], OptIndex::Shadafile),
            (&b"messagesopt"[..], OptIndex::Messagesopt),
            (&b"smartindent"[..], OptIndex::Smartindent),
            (&b"breakindent"[..], OptIndex::Breakindent),
            (&b"eventignore"[..], OptIndex::Eventignore),
            (&b"tagrelative"[..], OptIndex::Tagrelative),
            (&b"loadplugins"[..], OptIndex::Loadplugins),
            (&b"runtimepath"[..], OptIndex::Runtimepath),
            (&b"guifontwide"[..], OptIndex::Guifontwide),
            (&b"winminwidth"[..], OptIndex::Winminwidth),
            (&b"diffanchors"[..], OptIndex::Diffanchors),
            (&b"charconvert"[..], OptIndex::Charconvert),
            (&b"ttimeoutlen"[..], OptIndex::Ttimeoutlen),
            (&b"startofline"[..], OptIndex::Startofline),
            (&b"fileformats"[..], OptIndex::Fileformats),
            (&b"langnoremap"[..], OptIndex::Langnoremap),
            (&b"wildoptions"[..], OptIndex::Wildoptions),
            (&b"viewoptions"[..], OptIndex::Viewoptions),
            (&b"jumpoptions"[..], OptIndex::Jumpoptions),
            (&b"lispoptions"[..], OptIndex::Lispoptions),
            (&b"maxmapdepth"[..], OptIndex::Maxmapdepth),
            (&b"allowrevins"[..], OptIndex::Allowrevins),
            (&b"numberwidth"[..], OptIndex::Numberwidth),
            (&b"verbosefile"[..], OptIndex::Verbosefile),
            (&b"titlestring"[..], OptIndex::Titlestring),
            (&b"mousescroll"[..], OptIndex::Mousescroll),
            (&b"pastetoggle"[..], OptIndex::Pastetoggle),
            (&b"showfulltag"[..], OptIndex::Showfulltag),
            (&b"redrawdebug"[..], OptIndex::Redrawdebug),
            (&b"winfixwidth"[..], OptIndex::Winfixwidth),
            (&b"pummaxwidth"[..], OptIndex::Pummaxwidth),
            (&b"suffixesadd"[..], OptIndex::Suffixesadd),
            (&b"shellxquote"[..], OptIndex::Shellxquote),
            (&b"statuscolumn"[..], OptIndex::Statuscolumn),
            (&b"edcompatible"[..], OptIndex::Edcompatible),
            (&b"packlockfile"[..], OptIndex::Packlockfile),
            (&b"modelineexpr"[..], OptIndex::Modelineexpr),
            (&b"cmdwinheight"[..], OptIndex::Cmdwinheight),
            (&b"operatorfunc"[..], OptIndex::Operatorfunc),
            (&b"spellsuggest"[..], OptIndex::Spellsuggest),
            (&b"spelloptions"[..], OptIndex::Spelloptions),
            (&b"shellxescape"[..], OptIndex::Shellxescape),
            (&b"shellcmdflag"[..], OptIndex::Shellcmdflag),
            (&b"regexpengine"[..], OptIndex::Regexpengine),
            (&b"rightleftcmd"[..], OptIndex::Rightleftcmd),
            (&b"makeencoding"[..], OptIndex::Makeencoding),
            (&b"fileencoding"[..], OptIndex::Fileencoding),
            (&b"foldminlines"[..], OptIndex::Foldminlines),
            (&b"completefunc"[..], OptIndex::Completefunc),
            (&b"winfixheight"[..], OptIndex::Winfixheight),
            (&b"winhighlight"[..], OptIndex::Winhighlight),
            (&b"winminheight"[..], OptIndex::Winminheight),
            (&b"conceallevel"[..], OptIndex::Conceallevel),
            (&b"smoothscroll"[..], OptIndex::Smoothscroll),
            (&b"scrolloffpad"[..], OptIndex::Scrolloffpad),
            (&b"termencoding"[..], OptIndex::Termencoding),
            (&b"cursorcolumn"[..], OptIndex::Cursorcolumn),
            (&b"autocomplete"[..], OptIndex::Autocomplete),
            (&b"autowriteall"[..], OptIndex::Autowriteall),
            (&b"maxfuncdepth"[..], OptIndex::Maxfuncdepth),
            (&b"fixendofline"[..], OptIndex::Fixendofline),
            (&b"concealcursor"[..], OptIndex::Concealcursor),
            (&b"guitabtooltip"[..], OptIndex::Guitabtooltip),
            (&b"sidescrolloff"[..], OptIndex::Sidescrolloff),
            (&b"spellcapcheck"[..], OptIndex::Spellcapcheck),
            (&b"previewheight"[..], OptIndex::Previewheight),
            (&b"completeslash"[..], OptIndex::Completeslash),
            (&b"previewwindow"[..], OptIndex::Previewwindow),
            (&b"maxmempattern"[..], OptIndex::Maxmempattern),
            (&b"fileencodings"[..], OptIndex::Fileencodings),
            (&b"commentstring"[..], OptIndex::Commentstring),
            (&b"cinscopedecls"[..], OptIndex::Cinscopedecls),
            (&b"cursorlineopt"[..], OptIndex::Cursorlineopt),
            (&b"formatlistpat"[..], OptIndex::Formatlistpat),
            (&b"formatoptions"[..], OptIndex::Formatoptions),
            (&b"termguicolors"[..], OptIndex::Termguicolors),
            (&b"thesaurusfunc"[..], OptIndex::Thesaurusfunc),
            (&b"breakindentopt"[..], OptIndex::Breakindentopt),
            (&b"eventignorewin"[..], OptIndex::Eventignorewin),
            (&b"fileignorecase"[..], OptIndex::Fileignorecase),
            (&b"foldlevelstart"[..], OptIndex::Foldlevelstart),
            (&b"maxsearchcount"[..], OptIndex::Maxsearchcount),
            (&b"mousemoveevent"[..], OptIndex::Mousemoveevent),
            (&b"preserveindent"[..], OptIndex::Preserveindent),
            (&b"relativenumber"[..], OptIndex::Relativenumber),
            (&b"sessionoptions"[..], OptIndex::Sessionoptions),
            (&b"varsofttabstop"[..], OptIndex::Varsofttabstop),
            (&b"wildignorecase"[..], OptIndex::Wildignorecase),
            (&b"completetimeout"[..], OptIndex::Completetimeout),
            (&b"termpastefilter"[..], OptIndex::Termpastefilter),
            (&b"quickfixtextfunc"[..], OptIndex::Quickfixtextfunc),
            (&b"autocompletedelay"[..], OptIndex::Autocompletedelay),
            (&b"completeitemalign"[..], OptIndex::Completeitemalign),
            (&b"autocompletetimeout"[..], OptIndex::Autocompletetimeout),
        ]
        .into_iter()
        .collect()
    });

/// Find the index for an option name, without going beyond `len`
/// bytes of `name` (`find_option_len`).
///
/// Returns [`OptIndex::Invalid`] if `len` is out of bounds for `name`
/// or the option wasn't found - matching the original's own
/// `kOptInvalid` return exactly (a caller-contract violation on
/// `len` is treated the same as "not found" rather than panicking,
/// since every real caller derives `len` from scanning WITHIN `name`
/// itself, so it can never actually exceed `name.len()` in practice).
#[must_use]
pub fn find_option_len(name: &[u8], len: usize) -> OptIndex {
    name.get(..len).map_or(OptIndex::Invalid, find_option)
}

/// Find the index for an option name (`find_option`).
#[must_use]
pub fn find_option(name: &[u8]) -> OptIndex {
    OPTION_HASH_ELEMS.get(name).copied().unwrap_or(OptIndex::Invalid)
}

/// Find the end of an option name, handling TTY options separately
/// (`find_option_end`).
///
/// Returns `(end, opt_idx)`: `end` is the byte offset just past the
/// name, or `None` if `arg` doesn't start with a valid option name at
/// all (matching the original's own `NULL` return; `opt_idx` is
/// meaningless/`Invalid` in that case, matching the original leaving
/// `*opt_idxp` set to `kOptInvalid` there too).
#[must_use]
pub fn find_option_end(arg: &[u8]) -> (Option<usize>, OptIndex) {
    if let Some(end) = find_tty_option_end(arg) {
        return (Some(end), OptIndex::Invalid);
    }

    if !arg.first().is_some_and(|&c| crate::macros_defs::ascii_isalpha(i32::from(c))) {
        return (None, OptIndex::Invalid);
    }
    let mut p = 0;
    while arg.get(p).is_some_and(|&c| crate::macros_defs::ascii_isalpha(i32::from(c))) {
        p += 1;
    }
    (Some(p), find_option_len(arg, p))
}

/// Skip to next part of an option argument: skip a leading comma and
/// any following spaces (`skip_to_option_part`).
///
/// `option` is the whole option string being parsed; `p` is the
/// current byte offset into it (replacing the original's own `char *`
/// cursor). Returns the new offset.
#[must_use]
pub fn skip_to_option_part(option: &[u8], mut p: usize) -> usize {
    if option.get(p) == Some(&b',') {
        p += 1;
    }
    while option.get(p) == Some(&b' ') {
        p += 1;
    }
    p
}

/// Isolate one part of a string option separated by `sep_chars`
/// (`copy_option_part`).
///
/// `option` is the whole option string; `p` is the current byte
/// offset into it (replacing the original's own `char **option`
/// in-out cursor). Returns `(isolated_part, next_p)`: the isolated
/// part, truncated to at most `maxlen - 1` bytes (matching the
/// original's own fixed-size `buf[maxlen]` truncation - this crate's
/// growable `Vec<u8>` doesn't NEED this limit, but it's kept
/// faithfully since it's real, observable behavior for an option part
/// longer than `maxlen`, not merely an implementation detail to
/// simplify away), and the offset where parsing should continue for
/// the next part.
#[must_use]
pub fn copy_option_part(option: &[u8], p: usize, maxlen: usize, sep_chars: &[u8]) -> (Vec<u8>, usize) {
    let mut buf = Vec::new();
    let mut p = p;

    // Skip '.' at start of option part, for 'suffixes'.
    if option.get(p) == Some(&b'.') {
        buf.push(b'.');
        p += 1;
    }
    while option.get(p).is_some_and(|&c| {
        c != crate::ascii_defs::NUL && crate::strings::vim_strchr(sep_chars, i32::from(c)).is_none()
    }) {
        // Skip a backslash before a separator character (and space) -
        // the escaped separator itself is then copied as a literal
        // character instead of ending the part.
        if option.get(p) == Some(&b'\\')
            && option
                .get(p + 1)
                .is_some_and(|&c2| crate::strings::vim_strchr(sep_chars, i32::from(c2)).is_some())
        {
            p += 1;
        }
        if let Some(&c) = option.get(p) {
            if buf.len() < maxlen.saturating_sub(1) {
                buf.push(c);
            }
        }
        p += 1;
    }

    if option.get(p).is_some_and(|&c| c != crate::ascii_defs::NUL && c != b',') {
        p += 1;
    }
    p = skip_to_option_part(option, p);

    (buf, p)
}

/// Error message for `'chistory'`/`'lhistory'` given a non-positive
/// value (`e_cannot_have_negative_or_zero_number_of_quickfix`, a
/// file-local `static const char[]` in the original - kept file-local
/// here too, matching the original's own scoping, rather than added
/// to the shared `errors.rs`).
#[allow(non_upper_case_globals)]
const e_cannot_have_negative_or_zero_number_of_quickfix: &str =
    crate::gettext_defs::gettext_noop("E1542: Cannot have a negative or zero number of quickfix/location lists");

/// Error message for `'chistory'`/`'lhistory'` given a too-large value
/// (`e_cannot_have_more_than_hundred_quickfix`, file-local in the
/// original - see the constant above's own doc comment).
#[allow(non_upper_case_globals)]
const e_cannot_have_more_than_hundred_quickfix: &str =
    crate::gettext_defs::gettext_noop("E1543: Cannot have more than a hundred quickfix/location lists");

/// Check the bounds of numeric options (`check_num_option_bounds`).
///
/// Returns `Some(error_message)` when the value was out of bounds -
/// `*newval` may ALSO have been corrected in that case (or even when
/// there's no error at all, e.g. `Pumblend`'s silent clamp), matching
/// the original's own in-out `newval` parameter.
///
/// `OptIndex::Lines`'s own real error message embeds the dynamic
/// `min_rows_for_all_tabpages()` value ("E593: Need at least %d
/// lines") - this crate has no real message-DISPLAY consumer yet that
/// would need the exact number embedded in the text (there is no
/// `:set` command parser calling this function today), so a fixed
/// placeholder string is returned instead, matching this whole
/// project's established "skip the message display, keep the state/
/// return value correct" policy (`mf_write`/`u_get_headentry`/etc.) -
/// the `Some`-vs-`None` result and the corrected `*newval` are both
/// exactly faithful.
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` (dereferenced by the `OptIndex::Scroll` branch).
/// `crate::globals::GLOBALS.first_tabpage`'s own `tp_next` chain, each
/// with a valid, live `tp_topframe` frame tree, must consist of valid,
/// live pointers (`OptIndex::Lines`, via `min_rows_for_all_tabpages`).
#[must_use]
pub unsafe fn check_num_option_bounds(opt_idx: OptIndex, newval: &mut OptInt) -> Option<&'static str> {
    match opt_idx {
        OptIndex::Lines => {
            // SAFETY: forwarded from this function's own safety doc.
            let min_rows = OptInt::from(unsafe { crate::window::min_rows_for_all_tabpages() });
            let full_screen = unsafe { crate::globals::GLOBALS.get_mut() }.full_screen;
            let mut errmsg = None;
            if *newval < min_rows && full_screen {
                errmsg = Some("E593: Need at least N lines");
                *newval = min_rows;
            }
            // True max size is defined by check_screensize().
            *newval = (*newval).min(OptInt::from(i32::MAX));
            errmsg
        }
        OptIndex::Columns => {
            let full_screen = unsafe { crate::globals::GLOBALS.get_mut() }.full_screen;
            let mut errmsg = None;
            if *newval < OptInt::from(crate::window::MIN_COLUMNS) && full_screen {
                // "12" mirrors MIN_COLUMNS's own value (see window.rs).
                errmsg = Some("E594: Need at least 12 columns");
                *newval = OptInt::from(crate::window::MIN_COLUMNS);
            }
            *newval = (*newval).min(OptInt::from(i32::MAX));
            errmsg
        }
        OptIndex::Pumblend => {
            *newval = (*newval).clamp(0, 100);
            None
        }
        OptIndex::Scrolljump => {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            if (*newval < -100 || *newval >= OptInt::from(globals.Rows)) && globals.full_screen {
                *newval = 1;
                return Some(crate::errors::e_scroll);
            }
            None
        }
        OptIndex::Scroll => {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let full_screen = globals.full_screen;
            // SAFETY: forwarded from this function's own safety doc.
            let w_view_height = OptInt::from(unsafe { &*globals.curwin }.w_view_height);
            let mut errmsg = None;
            if (*newval <= 0 || (*newval > w_view_height && w_view_height > 0)) && full_screen {
                if *newval != 0 {
                    errmsg = Some(crate::errors::e_scroll);
                }
                // SAFETY: forwarded from this function's own safety doc.
                *newval = unsafe { crate::window::win_default_scroll(globals.curwin) };
            }
            errmsg
        }
        _ => None,
    }
}

/// Validate and bound check option value (`validate_num_option`).
///
/// Returns `Some(error_message)` on failure; `*newval` may ALSO have
/// been corrected even on success (e.g. `Maxcombine` always forces
/// `MAX_MCO`, `Pyxversion` forces `3`).
///
/// # Safety
/// Forwards [`check_num_option_bounds`]'s own safety requirement.
#[must_use]
pub unsafe fn validate_num_option(opt_idx: OptIndex, newval: &mut OptInt) -> Option<&'static str> {
    let value = *newval;

    if value < OptInt::from(i32::MIN) || value > OptInt::from(i32::MAX) {
        return Some(crate::errors::e_invarg);
    }

    // if you increase this, also increase SEARCH_STAT_BUF_LEN in search.c
    const MAX_SEARCH_COUNT: OptInt = 9999;

    let full_screen = unsafe { crate::globals::GLOBALS.get_mut() }.full_screen;
    let opt_vars = unsafe { crate::option_vars::OPTION_VARS.get_mut() };

    match opt_idx {
        OptIndex::Helpheight
        | OptIndex::Titlelen
        | OptIndex::Updatecount
        | OptIndex::Report
        | OptIndex::Updatetime
        | OptIndex::Sidescroll
        | OptIndex::Foldlevel
        | OptIndex::Shiftwidth
        | OptIndex::Textwidth
        | OptIndex::Writedelay
        | OptIndex::Timeoutlen => {
            if value < 0 {
                return Some(crate::errors::e_positive);
            }
        }
        OptIndex::Winheight => {
            if value < 1 {
                return Some(crate::errors::e_positive);
            } else if opt_vars.p_wmh > value {
                return Some(crate::errors::e_winheight);
            }
        }
        OptIndex::Winminheight => {
            if value < 0 {
                return Some(crate::errors::e_positive);
            } else if value > opt_vars.p_wh {
                return Some(crate::errors::e_winheight);
            }
        }
        OptIndex::Winwidth => {
            if value < 1 {
                return Some(crate::errors::e_positive);
            } else if opt_vars.p_wmw > value {
                return Some(crate::errors::e_winwidth);
            }
        }
        OptIndex::Winminwidth => {
            if value < 0 {
                return Some(crate::errors::e_positive);
            } else if value > opt_vars.p_wiw {
                return Some(crate::errors::e_winwidth);
            }
        }
        OptIndex::Maxcombine => {
            *newval = OptInt::from(crate::option_vars::MAX_MCO);
        }
        OptIndex::Cmdheight => {
            if value < 0 {
                return Some(crate::errors::e_positive);
            }
        }
        OptIndex::History => {
            if value < 0 {
                return Some(crate::errors::e_positive);
            } else if value > 10000 {
                return Some(crate::errors::e_invarg);
            }
        }
        OptIndex::Pyxversion => {
            if value == 0 {
                *newval = 3;
            } else if value != 3 {
                return Some(crate::errors::e_invarg);
            }
        }
        OptIndex::Regexpengine => {
            if !(0..=2).contains(&value) {
                return Some(crate::errors::e_invarg);
            }
        }
        OptIndex::Scrolloff => {
            if value < 0 && full_screen {
                return Some(crate::errors::e_positive);
            }
        }
        OptIndex::Scrolloffpad => {
            if value < 0 {
                return Some(crate::errors::e_invarg);
            }
        }
        OptIndex::Sidescrolloff => {
            if value < 0 && full_screen {
                return Some(crate::errors::e_positive);
            }
        }
        OptIndex::Cmdwinheight => {
            if value < 1 {
                return Some(crate::errors::e_positive);
            }
        }
        OptIndex::Conceallevel => {
            if value < 0 {
                return Some(crate::errors::e_positive);
            } else if value > 3 {
                return Some(crate::errors::e_invarg);
            }
        }
        OptIndex::Numberwidth => {
            if value < 1 {
                return Some(crate::errors::e_positive);
            } else if value > OptInt::from(crate::option_vars::MAX_NUMBERWIDTH) {
                return Some(crate::errors::e_invarg);
            }
        }
        OptIndex::Iminsert => {
            if !(0..=crate::buffer_defs::B_IMODE_LAST).contains(&value) {
                return Some(crate::errors::e_invarg);
            }
        }
        OptIndex::Imsearch => {
            if !(-1..=crate::buffer_defs::B_IMODE_LAST).contains(&value) {
                return Some(crate::errors::e_invarg);
            }
        }
        OptIndex::Channel => {
            return Some(crate::errors::e_invarg);
        }
        OptIndex::Scrollback => {
            if !(-1..=OptInt::from(crate::option_vars::SB_MAX)).contains(&value) {
                return Some(crate::errors::e_invarg);
            }
        }
        OptIndex::Tabstop => {
            if value < 1 {
                return Some(crate::errors::e_positive);
            } else if value > OptInt::from(crate::option_vars::TABSTOP_MAX) {
                return Some(crate::errors::e_invarg);
            }
        }
        OptIndex::Chistory | OptIndex::Lhistory => {
            if value < 1 {
                return Some(e_cannot_have_negative_or_zero_number_of_quickfix);
            } else if value > 100 {
                return Some(e_cannot_have_more_than_hundred_quickfix);
            }
        }
        OptIndex::Maxsearchcount => {
            if value <= 0 {
                return Some(crate::errors::e_positive);
            } else if value > MAX_SEARCH_COUNT {
                return Some(crate::errors::e_invarg);
            }
        }
        _ => {}
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_num_option_bounds(opt_idx, newval) }
}

#[cfg(test)]
mod num_option_bounds_tests {
    use super::*;

    #[test]
    fn check_num_option_bounds_lines_uses_min_rows_for_all_tabpages() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        let prev_full_screen = globals.full_screen;
        // firstwin == null takes min_rows_for_all_tabpages's own
        // "not initialized yet" fast path, returning MIN_LINES (2) -
        // avoids needing a full frame tree for this test.
        globals.firstwin = std::ptr::null_mut();
        globals.full_screen = true;

        let mut v: OptInt = 1;
        assert_eq!(
            unsafe { check_num_option_bounds(OptIndex::Lines, &mut v) },
            Some("E593: Need at least N lines")
        );
        assert_eq!(v, 2);

        let mut v2: OptInt = 5;
        assert_eq!(unsafe { check_num_option_bounds(OptIndex::Lines, &mut v2) }, None);
        assert_eq!(v2, 5);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.firstwin = prev_firstwin;
        globals.full_screen = prev_full_screen;
    }

    #[test]
    fn check_num_option_bounds_scroll_uses_win_default_scroll() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { w_view_height: 20, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_curwin = globals.curwin;
        let prev_full_screen = globals.full_screen;
        globals.curwin = win_ptr;
        globals.full_screen = true;

        // 0 always takes the "correct it" branch, but 0 itself never
        // produces an error message (matching the original's own
        // `if (*newval != 0) { errmsg = e_scroll; }` guard).
        let mut v0: OptInt = 0;
        assert_eq!(unsafe { check_num_option_bounds(OptIndex::Scroll, &mut v0) }, None);
        assert_eq!(v0, 10); // win_default_scroll: max(20/2, 1) = 10

        // Larger than w_view_height (20) and w_view_height > 0: errors
        // AND gets corrected to win_default_scroll's value.
        let mut v_big: OptInt = 100;
        assert_eq!(
            unsafe { check_num_option_bounds(OptIndex::Scroll, &mut v_big) },
            Some(crate::errors::e_scroll)
        );
        assert_eq!(v_big, 10);

        // Within [1, w_view_height]: left untouched.
        let mut v_ok: OptInt = 5;
        assert_eq!(unsafe { check_num_option_bounds(OptIndex::Scroll, &mut v_ok) }, None);
        assert_eq!(v_ok, 5);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curwin = prev_curwin;
        globals.full_screen = prev_full_screen;
    }

    #[test]
    fn check_num_option_bounds_columns_clamps_and_errors_when_full_screen() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.full_screen;
        unsafe { crate::globals::GLOBALS.get_mut() }.full_screen = true;

        let mut v: OptInt = 5;
        let err = unsafe { check_num_option_bounds(OptIndex::Columns, &mut v) };
        assert_eq!(err, Some("E594: Need at least 12 columns"));
        assert_eq!(v, 12);

        unsafe { crate::globals::GLOBALS.get_mut() }.full_screen = prev;
    }

    #[test]
    fn check_num_option_bounds_columns_no_error_when_not_full_screen() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.full_screen;
        unsafe { crate::globals::GLOBALS.get_mut() }.full_screen = false;

        let mut v: OptInt = 5;
        let err = unsafe { check_num_option_bounds(OptIndex::Columns, &mut v) };
        assert_eq!(err, None);
        assert_eq!(v, 5); // not clamped to MIN_COLUMNS, only the i32::MAX ceiling applies

        unsafe { crate::globals::GLOBALS.get_mut() }.full_screen = prev;
    }

    #[test]
    fn check_num_option_bounds_columns_clamps_to_i32_max() {
        let mut v: OptInt = OptInt::from(i32::MAX) + 1000;
        let err = unsafe { check_num_option_bounds(OptIndex::Columns, &mut v) };
        assert_eq!(err, None);
        assert_eq!(v, OptInt::from(i32::MAX));
    }

    #[test]
    fn check_num_option_bounds_pumblend_clamps_both_directions() {
        let mut v: OptInt = -5;
        assert_eq!(unsafe { check_num_option_bounds(OptIndex::Pumblend, &mut v) }, None);
        assert_eq!(v, 0);

        let mut v2: OptInt = 250;
        assert_eq!(unsafe { check_num_option_bounds(OptIndex::Pumblend, &mut v2) }, None);
        assert_eq!(v2, 100);
    }

    #[test]
    fn check_num_option_bounds_scrolljump_errors_when_out_of_range_and_full_screen() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_full_screen = globals.full_screen;
        let prev_rows = globals.Rows;
        globals.full_screen = true;
        globals.Rows = 24;

        let mut v: OptInt = -200;
        assert_eq!(unsafe { check_num_option_bounds(OptIndex::Scrolljump, &mut v) }, Some(crate::errors::e_scroll));
        assert_eq!(v, 1);

        let mut v2: OptInt = 30;
        assert_eq!(unsafe { check_num_option_bounds(OptIndex::Scrolljump, &mut v2) }, Some(crate::errors::e_scroll));
        assert_eq!(v2, 1);

        let mut v3: OptInt = 5;
        assert_eq!(unsafe { check_num_option_bounds(OptIndex::Scrolljump, &mut v3) }, None);
        assert_eq!(v3, 5);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.full_screen = prev_full_screen;
        globals.Rows = prev_rows;
    }

    #[test]
    fn check_num_option_bounds_scrolljump_no_error_when_not_full_screen() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_full_screen = globals.full_screen;
        globals.full_screen = false;

        let mut v: OptInt = -200;
        assert_eq!(unsafe { check_num_option_bounds(OptIndex::Scrolljump, &mut v) }, None);
        assert_eq!(v, -200);

        unsafe { crate::globals::GLOBALS.get_mut() }.full_screen = prev_full_screen;
    }

    #[test]
    fn check_num_option_bounds_default_case_leaves_value_untouched() {
        let mut v: OptInt = 224;
        assert_eq!(unsafe { check_num_option_bounds(OptIndex::Aleph, &mut v) }, None);
        assert_eq!(v, 224);
    }

    #[test]
    fn validate_num_option_rejects_out_of_i32_range() {
        let mut v: OptInt = OptInt::from(i32::MAX) + 1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Cmdheight, &mut v) }, Some(crate::errors::e_invarg));
    }

    #[test]
    fn validate_num_option_simple_non_negative_group() {
        let mut v: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Timeoutlen, &mut v) }, Some(crate::errors::e_positive));
        let mut v2: OptInt = 5;
        assert_eq!(unsafe { validate_num_option(OptIndex::Timeoutlen, &mut v2) }, None);
    }

    #[test]
    fn validate_num_option_winheight_checks_against_p_wmh() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmh;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmh = 5;

        let mut v: OptInt = 0;
        assert_eq!(unsafe { validate_num_option(OptIndex::Winheight, &mut v) }, Some(crate::errors::e_positive));

        let mut v2: OptInt = 3;
        assert_eq!(unsafe { validate_num_option(OptIndex::Winheight, &mut v2) }, Some(crate::errors::e_winheight));

        let mut v3: OptInt = 10;
        assert_eq!(unsafe { validate_num_option(OptIndex::Winheight, &mut v3) }, None);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmh = prev;
    }

    #[test]
    fn validate_num_option_winminheight_checks_against_p_wh() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wh;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wh = 5;

        let mut v: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Winminheight, &mut v) }, Some(crate::errors::e_positive));

        let mut v2: OptInt = 10;
        assert_eq!(unsafe { validate_num_option(OptIndex::Winminheight, &mut v2) }, Some(crate::errors::e_winheight));

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wh = prev;
    }

    #[test]
    fn validate_num_option_winwidth_checks_against_p_wmw() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmw;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmw = 5;

        let mut v: OptInt = 0;
        assert_eq!(unsafe { validate_num_option(OptIndex::Winwidth, &mut v) }, Some(crate::errors::e_positive));

        let mut v2: OptInt = 3;
        assert_eq!(unsafe { validate_num_option(OptIndex::Winwidth, &mut v2) }, Some(crate::errors::e_winwidth));

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmw = prev;
    }

    #[test]
    fn validate_num_option_winminwidth_checks_against_p_wiw() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wiw;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wiw = 5;

        let mut v: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Winminwidth, &mut v) }, Some(crate::errors::e_positive));

        let mut v2: OptInt = 10;
        assert_eq!(unsafe { validate_num_option(OptIndex::Winminwidth, &mut v2) }, Some(crate::errors::e_winwidth));

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wiw = prev;
    }

    #[test]
    fn validate_num_option_maxcombine_always_forces_max_mco() {
        let mut v: OptInt = 999;
        assert_eq!(unsafe { validate_num_option(OptIndex::Maxcombine, &mut v) }, None);
        assert_eq!(v, OptInt::from(crate::option_vars::MAX_MCO));
    }

    #[test]
    fn validate_num_option_history_bounds() {
        let mut v: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::History, &mut v) }, Some(crate::errors::e_positive));
        let mut v2: OptInt = 20000;
        assert_eq!(unsafe { validate_num_option(OptIndex::History, &mut v2) }, Some(crate::errors::e_invarg));
        let mut v3: OptInt = 500;
        assert_eq!(unsafe { validate_num_option(OptIndex::History, &mut v3) }, None);
    }

    #[test]
    fn validate_num_option_pyxversion_forces_3_or_rejects() {
        let mut v: OptInt = 0;
        assert_eq!(unsafe { validate_num_option(OptIndex::Pyxversion, &mut v) }, None);
        assert_eq!(v, 3);

        let mut v2: OptInt = 2;
        assert_eq!(unsafe { validate_num_option(OptIndex::Pyxversion, &mut v2) }, Some(crate::errors::e_invarg));
    }

    #[test]
    fn validate_num_option_regexpengine_range() {
        let mut v: OptInt = 3;
        assert_eq!(unsafe { validate_num_option(OptIndex::Regexpengine, &mut v) }, Some(crate::errors::e_invarg));
        let mut v2: OptInt = 1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Regexpengine, &mut v2) }, None);
    }

    #[test]
    fn validate_num_option_scrolloff_needs_full_screen_to_error() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.full_screen;

        unsafe { crate::globals::GLOBALS.get_mut() }.full_screen = true;
        let mut v: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Scrolloff, &mut v) }, Some(crate::errors::e_positive));

        unsafe { crate::globals::GLOBALS.get_mut() }.full_screen = false;
        let mut v2: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Scrolloff, &mut v2) }, None);

        unsafe { crate::globals::GLOBALS.get_mut() }.full_screen = prev;
    }

    #[test]
    fn validate_num_option_scrolloffpad_errors_regardless_of_full_screen() {
        let mut v: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Scrolloffpad, &mut v) }, Some(crate::errors::e_invarg));
    }

    #[test]
    fn validate_num_option_sidescrolloff_needs_full_screen_to_error() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.full_screen;

        unsafe { crate::globals::GLOBALS.get_mut() }.full_screen = true;
        let mut v: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Sidescrolloff, &mut v) }, Some(crate::errors::e_positive));

        unsafe { crate::globals::GLOBALS.get_mut() }.full_screen = prev;
    }

    #[test]
    fn validate_num_option_cmdwinheight() {
        let mut v: OptInt = 0;
        assert_eq!(unsafe { validate_num_option(OptIndex::Cmdwinheight, &mut v) }, Some(crate::errors::e_positive));
        let mut v2: OptInt = 1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Cmdwinheight, &mut v2) }, None);
    }

    #[test]
    fn validate_num_option_conceallevel_bounds() {
        let mut v: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Conceallevel, &mut v) }, Some(crate::errors::e_positive));
        let mut v2: OptInt = 4;
        assert_eq!(unsafe { validate_num_option(OptIndex::Conceallevel, &mut v2) }, Some(crate::errors::e_invarg));
        let mut v3: OptInt = 2;
        assert_eq!(unsafe { validate_num_option(OptIndex::Conceallevel, &mut v3) }, None);
    }

    #[test]
    fn validate_num_option_numberwidth_bounds() {
        let mut v: OptInt = 0;
        assert_eq!(unsafe { validate_num_option(OptIndex::Numberwidth, &mut v) }, Some(crate::errors::e_positive));
        let mut v2: OptInt = 21;
        assert_eq!(unsafe { validate_num_option(OptIndex::Numberwidth, &mut v2) }, Some(crate::errors::e_invarg));
    }

    #[test]
    fn validate_num_option_iminsert_and_imsearch_bounds() {
        let mut v: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Iminsert, &mut v) }, Some(crate::errors::e_invarg));
        let mut v2: OptInt = 2;
        assert_eq!(unsafe { validate_num_option(OptIndex::Iminsert, &mut v2) }, Some(crate::errors::e_invarg));

        let mut v3: OptInt = -2;
        assert_eq!(unsafe { validate_num_option(OptIndex::Imsearch, &mut v3) }, Some(crate::errors::e_invarg));
        let mut v4: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Imsearch, &mut v4) }, None);
    }

    #[test]
    fn validate_num_option_channel_always_errors() {
        let mut v: OptInt = 0;
        assert_eq!(unsafe { validate_num_option(OptIndex::Channel, &mut v) }, Some(crate::errors::e_invarg));
    }

    #[test]
    fn validate_num_option_scrollback_bounds() {
        let mut v: OptInt = -2;
        assert_eq!(unsafe { validate_num_option(OptIndex::Scrollback, &mut v) }, Some(crate::errors::e_invarg));
        let mut v2: OptInt = OptInt::from(crate::option_vars::SB_MAX) + 1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Scrollback, &mut v2) }, Some(crate::errors::e_invarg));
        let mut v3: OptInt = -1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Scrollback, &mut v3) }, None);
    }

    #[test]
    fn validate_num_option_tabstop_bounds() {
        let mut v: OptInt = 0;
        assert_eq!(unsafe { validate_num_option(OptIndex::Tabstop, &mut v) }, Some(crate::errors::e_positive));
        let mut v2: OptInt = OptInt::from(crate::option_vars::TABSTOP_MAX) + 1;
        assert_eq!(unsafe { validate_num_option(OptIndex::Tabstop, &mut v2) }, Some(crate::errors::e_invarg));
    }

    #[test]
    fn validate_num_option_chistory_and_lhistory_bounds() {
        let mut v: OptInt = 0;
        assert_eq!(
            unsafe { validate_num_option(OptIndex::Chistory, &mut v) },
            Some(e_cannot_have_negative_or_zero_number_of_quickfix)
        );
        let mut v2: OptInt = 101;
        assert_eq!(
            unsafe { validate_num_option(OptIndex::Lhistory, &mut v2) },
            Some(e_cannot_have_more_than_hundred_quickfix)
        );
    }

    #[test]
    fn validate_num_option_maxsearchcount_bounds() {
        let mut v: OptInt = 0;
        assert_eq!(unsafe { validate_num_option(OptIndex::Maxsearchcount, &mut v) }, Some(crate::errors::e_positive));
        let mut v2: OptInt = 10000;
        assert_eq!(unsafe { validate_num_option(OptIndex::Maxsearchcount, &mut v2) }, Some(crate::errors::e_invarg));
    }

    #[test]
    fn validate_num_option_default_case_delegates_to_check_num_option_bounds() {
        // Pumblend has no explicit arm in validate_num_option's own
        // switch - it must fall through to check_num_option_bounds,
        // which clamps it.
        let mut v: OptInt = 500;
        assert_eq!(unsafe { validate_num_option(OptIndex::Pumblend, &mut v) }, None);
        assert_eq!(v, 100);
    }

    #[test]
    fn validate_num_option_lines_delegates_to_check_num_option_bounds() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        let prev_full_screen = globals.full_screen;
        globals.firstwin = std::ptr::null_mut();
        globals.full_screen = true;

        let mut v: OptInt = 1;
        assert_eq!(
            unsafe { validate_num_option(OptIndex::Lines, &mut v) },
            Some("E593: Need at least N lines")
        );
        assert_eq!(v, 2);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.firstwin = prev_firstwin;
        globals.full_screen = prev_full_screen;
    }
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
    fn set_iminsert_global_copies_the_buffers_local_value() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_iminsert;

        let buf = BufT {
            b_p_iminsert: 2,
            ..Default::default()
        };
        set_iminsert_global(&buf);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_iminsert, 2);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_iminsert = prev;
    }

    #[test]
    fn set_imsearch_global_copies_the_buffers_local_value() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_imsearch;

        let buf = BufT {
            b_p_imsearch: 1,
            ..Default::default()
        };
        set_imsearch_global(&buf);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_imsearch, 1);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_imsearch = prev;
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

#[cfg(test)]
mod optval_tests {
    use super::*;

    #[test]
    fn optval_from_varp_reads_boolean_via_tristate_from_int() {
        let mut val: i32 = 1;
        let varp = &mut val as *mut i32 as *mut c_void;
        assert_eq!(unsafe { optval_from_varp(OptIndex::Allowrevins, varp) }, OptVal::Boolean(TriState::True));

        // Write through varp itself (not the original `val` binding) from
        // here on - a later write through `val` directly would invalidate
        // the pointer already derived from it (a real Tree Borrows
        // violation caught by Miri in this exact test during development;
        // see eval/vars.rs's own `Box::as_mut()`-then-reborrow precedent
        // for the same class of bug).
        unsafe { *(varp as *mut i32) = 0 };
        assert_eq!(unsafe { optval_from_varp(OptIndex::Allowrevins, varp) }, OptVal::Boolean(TriState::False));

        unsafe { *(varp as *mut i32) = -1 };
        assert_eq!(unsafe { optval_from_varp(OptIndex::Allowrevins, varp) }, OptVal::Boolean(TriState::None));
    }

    #[test]
    fn optval_from_varp_reads_number() {
        let mut val: OptInt = 224;
        let varp = &mut val as *mut OptInt as *mut c_void;
        assert_eq!(unsafe { optval_from_varp(OptIndex::Aleph, varp) }, OptVal::Number(224));
    }

    #[test]
    fn optval_from_varp_reads_string_cloning_it_out() {
        let mut val: Option<Vec<u8>> = Some(b"double".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        assert_eq!(
            unsafe { optval_from_varp(OptIndex::Ambiwidth, varp) },
            OptVal::String(b"double".to_vec())
        );
    }

    #[test]
    fn optval_from_varp_reads_none_string_as_empty() {
        let mut val: Option<Vec<u8>> = None;
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        assert_eq!(unsafe { optval_from_varp(OptIndex::Ambiwidth, varp) }, OptVal::String(Vec::new()));
    }

    #[test]
    fn optval_from_varp_special_cases_b_changed_via_curbuf_is_changed() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_buf = globals.curbuf;
        globals.curbuf = &mut buf as *mut BufT;

        // Derive varp from globals.curbuf itself (not from `buf` directly) -
        // optval_from_varp/curbuf_is_changed both access the buffer through
        // GLOBALS.curbuf internally, so a pointer derived independently
        // from `buf` would be a Tree Borrows sibling access, not the same
        // lineage, and a later write through it would invalidate
        // GLOBALS.curbuf's own pointer (caught by Miri during development).
        let curbuf_ptr = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let varp = unsafe { std::ptr::addr_of_mut!((*curbuf_ptr).b_changed) as *mut c_void };
        assert_eq!(unsafe { optval_from_varp(OptIndex::Modified, varp) }, OptVal::Boolean(TriState::False));
        unsafe { *(varp as *mut i32) = 1 };
        assert_eq!(unsafe { optval_from_varp(OptIndex::Modified, varp) }, OptVal::Boolean(TriState::True));

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curbuf = prev_buf;
    }

    #[test]
    fn get_option_value_returns_nil_for_invalid_index() {
        assert_eq!(unsafe { get_option_value(OptIndex::Invalid, 0) }, OptVal::Nil);
    }

    #[test]
    fn get_option_value_resolves_the_effective_value_for_current_buffer_and_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ai: 1, ..Default::default() };
        let mut win = WinT::default();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_buf = globals.curbuf;
        let prev_win = globals.curwin;
        globals.curbuf = &mut buf as *mut BufT;
        globals.curwin = &mut win as *mut WinT;

        assert_eq!(
            unsafe { get_option_value(OptIndex::Autoindent, 0) },
            OptVal::Boolean(TriState::True)
        );

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curbuf = prev_buf;
        globals.curwin = prev_win;
    }

    #[test]
    fn get_varp_scope_from_opt_local_forces_the_buffer_local_value_even_when_unset() {
        let mut buf = BufT::default(); // b_p_ep unset (None)
        let mut win = WinT::default();
        let varp = unsafe {
            get_varp_scope_from(OptIndex::Equalprg, crate::option_defs::opt_set_flags::OPT_LOCAL, &mut buf, &mut win)
        } as *mut Option<Vec<u8>>;
        unsafe { *varp = Some(b"forced".to_vec()) };
        assert_eq!(buf.b_p_ep, Some(b"forced".to_vec()));
    }

    #[test]
    fn get_varp_scope_from_opt_local_forces_the_window_local_value() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let varp = unsafe {
            get_varp_scope_from(
                OptIndex::Sidescrolloff,
                crate::option_defs::opt_set_flags::OPT_LOCAL,
                &mut buf,
                &mut win,
            )
        } as *mut OptInt;
        unsafe { *varp = 5 };
        assert_eq!(win.w_onebuf_opt.wo_siso, 5);
    }

    #[test]
    fn get_varp_scope_from_opt_global_on_global_local_option_returns_p_var() {
        let mut buf = BufT { b_p_ep: Some(b"local".to_vec()), ..Default::default() };
        let mut win = WinT::default();
        let p = get_option(OptIndex::Equalprg);
        let varp = unsafe {
            get_varp_scope_from(OptIndex::Equalprg, crate::option_defs::opt_set_flags::OPT_GLOBAL, &mut buf, &mut win)
        };
        assert_eq!(varp, p.var);
    }

    #[test]
    fn get_varp_scope_from_opt_global_on_window_local_only_option_uses_w_allbuf_opt() {
        // 'arabic' is purely window-local (no global scope) - OPT_GLOBAL
        // must resolve to w_allbuf_opt, NOT w_onebuf_opt (the original's
        // GLOBAL_WO(p) trick, replaced here with real field addressing).
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let varp = unsafe {
            get_varp_scope_from(OptIndex::Arabic, crate::option_defs::opt_set_flags::OPT_GLOBAL, &mut buf, &mut win)
        } as *mut i32;
        unsafe { *varp = 1 };
        assert_eq!(win.w_allbuf_opt.wo_arab, 1);
        assert_eq!(win.w_onebuf_opt.wo_arab, 0); // untouched
    }

    #[test]
    fn get_varp_scope_from_no_flags_falls_through_to_get_varp_from() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let a = unsafe { get_varp_scope_from(OptIndex::Autoindent, 0, &mut buf, &mut win) };
        let b = unsafe { get_varp_from(OptIndex::Autoindent, &mut buf, &mut win) };
        assert_eq!(a, b);
    }

    #[test]
    fn get_option_value_with_opt_local_reads_the_forced_local_value() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default(); // b_p_ep unset
        let mut win = WinT::default();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_buf = globals.curbuf;
        let prev_win = globals.curwin;
        globals.curbuf = &mut buf as *mut BufT;
        globals.curwin = &mut win as *mut WinT;

        // OPT_LOCAL forces the (unset, empty) local value, unlike plain
        // get_option_value(opt_idx, 0) which would fall back to global.
        assert_eq!(
            unsafe { get_option_value(OptIndex::Equalprg, crate::option_defs::opt_set_flags::OPT_LOCAL) },
            OptVal::String(Vec::new())
        );

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curbuf = prev_buf;
        globals.curwin = prev_win;
    }
}

#[cfg(test)]
mod set_option_varp_tests {
    use super::*;

    #[test]
    fn set_option_varp_writes_boolean() {
        let mut val: i32 = 0;
        let varp = &mut val as *mut i32 as *mut c_void;
        unsafe { set_option_varp(OptIndex::Allowrevins, varp, OptVal::Boolean(TriState::True)) };
        assert_eq!(unsafe { *(varp as *mut i32) }, 1);

        unsafe { set_option_varp(OptIndex::Allowrevins, varp, OptVal::Boolean(TriState::None)) };
        assert_eq!(unsafe { *(varp as *mut i32) }, -1);
    }

    #[test]
    fn set_option_varp_writes_number() {
        let mut val: OptInt = 0;
        let varp = &mut val as *mut OptInt as *mut c_void;
        unsafe { set_option_varp(OptIndex::Aleph, varp, OptVal::Number(42)) };
        assert_eq!(unsafe { *(varp as *mut OptInt) }, 42);
    }

    #[test]
    fn set_option_varp_writes_string_replacing_previous_value() {
        let mut val: Option<Vec<u8>> = Some(b"old".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        unsafe { set_option_varp(OptIndex::Ambiwidth, varp, OptVal::String(b"double".to_vec())) };
        assert_eq!(unsafe { (*(varp as *mut Option<Vec<u8>>)).clone() }, Some(b"double".to_vec()));
    }

    #[test]
    fn set_option_varp_roundtrips_through_optval_from_varp() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let varp = unsafe { get_varp_from(OptIndex::Autoindent, &mut buf, &mut win) };
        unsafe { set_option_varp(OptIndex::Autoindent, varp, OptVal::Boolean(TriState::True)) };
        assert_eq!(unsafe { optval_from_varp(OptIndex::Autoindent, varp) }, OptVal::Boolean(TriState::True));
        assert_eq!(buf.b_p_ai, 1);
    }

    #[test]
    #[should_panic]
    fn set_option_varp_nil_panics() {
        let mut val: i32 = 0;
        let varp = &mut val as *mut i32 as *mut c_void;
        unsafe { set_option_varp(OptIndex::Allowrevins, varp, OptVal::Nil) };
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic]
    fn set_option_varp_debug_panics_on_type_mismatch() {
        let mut val: OptInt = 0;
        let varp = &mut val as *mut OptInt as *mut c_void;
        // Allowrevins is Boolean-typed; writing a Number should trip the
        // debug_assert! (matches the original's own `assert`).
        unsafe { set_option_varp(OptIndex::Allowrevins, varp, OptVal::Number(1)) };
    }
}

#[cfg(test)]
mod tty_option_tests {
    use super::*;

    #[test]
    fn find_tty_option_end_matches_exact_term_and_ttytype() {
        assert_eq!(find_tty_option_end(b"term"), Some(4));
        assert_eq!(find_tty_option_end(b"ttytype"), Some(7));
    }

    #[test]
    fn find_tty_option_end_rejects_term_with_trailing_content() {
        // "term=xterm" isn't a WHOLE match for "term" - falls through to
        // the t_xx/delimited scan, which also doesn't match, so None.
        assert_eq!(find_tty_option_end(b"term=xterm"), None);
    }

    #[test]
    fn find_tty_option_end_matches_t_xx_keycode_form() {
        assert_eq!(find_tty_option_end(b"t_Co"), Some(4));
        // Embedded in a larger string: offset points just past "t_Co".
        assert_eq!(find_tty_option_end(b"t_Co=5"), Some(4));
    }

    #[test]
    fn find_tty_option_end_matches_delimited_angle_bracket_form() {
        assert_eq!(find_tty_option_end(b"<t_Co>"), Some(6));
        assert_eq!(find_tty_option_end(b"<t_Co>rest"), Some(6));
    }

    #[test]
    fn find_tty_option_end_rejects_unterminated_delimited_form() {
        assert_eq!(find_tty_option_end(b"<t_Co"), None);
    }

    #[test]
    fn find_tty_option_end_none_for_ordinary_option_name() {
        assert_eq!(find_tty_option_end(b"autoindent"), None);
        assert_eq!(find_tty_option_end(b""), None);
    }

    #[test]
    fn is_tty_option_matches_find_tty_option_end() {
        assert!(is_tty_option(b"term"));
        assert!(is_tty_option(b"t_Co"));
        assert!(!is_tty_option(b"autoindent"));
    }

    #[test]
    fn get_tty_option_t_co_reflects_t_colors() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.t_colors;

        unsafe { crate::globals::GLOBALS.get_mut() }.t_colors = 1;
        assert_eq!(get_tty_option(b"t_Co"), OptVal::String(Vec::new()));

        unsafe { crate::globals::GLOBALS.get_mut() }.t_colors = 256;
        assert_eq!(get_tty_option(b"t_Co"), OptVal::String(b"256".to_vec()));

        unsafe { crate::globals::GLOBALS.get_mut() }.t_colors = prev;
    }

    #[test]
    fn get_and_set_tty_option_term_roundtrip() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { P_TERM.get_mut() }.clone();

        unsafe { *P_TERM.get_mut() = None };
        assert_eq!(get_tty_option(b"term"), OptVal::String(b"nvim".to_vec()));

        assert!(set_tty_option(b"term", b"xterm-256color".to_vec()));
        assert_eq!(get_tty_option(b"term"), OptVal::String(b"xterm-256color".to_vec()));

        unsafe { *P_TERM.get_mut() = prev };
    }

    #[test]
    fn get_and_set_tty_option_ttytype_roundtrip() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { P_TTYTYPE.get_mut() }.clone();

        unsafe { *P_TTYTYPE.get_mut() = None };
        assert_eq!(get_tty_option(b"ttytype"), OptVal::String(b"nvim".to_vec()));

        assert!(set_tty_option(b"ttytype", b"xterm".to_vec()));
        assert_eq!(get_tty_option(b"ttytype"), OptVal::String(b"xterm".to_vec()));

        unsafe { *P_TTYTYPE.get_mut() = prev };
    }

    #[test]
    fn get_tty_option_generic_keycode_returns_empty_string() {
        assert_eq!(get_tty_option(b"t_kb"), OptVal::String(Vec::new()));
    }

    #[test]
    fn get_tty_option_non_tty_name_returns_nil() {
        assert_eq!(get_tty_option(b"autoindent"), OptVal::Nil);
    }

    #[test]
    fn set_tty_option_non_settable_name_returns_false() {
        assert!(!set_tty_option(b"t_Co", b"8".to_vec()));
        assert!(!set_tty_option(b"autoindent", b"x".to_vec()));
    }
}

#[cfg(test)]
mod find_option_tests {
    use super::*;

    #[test]
    fn find_option_matches_full_name() {
        assert_eq!(find_option(b"equalalways"), OptIndex::Equalalways);
    }

    #[test]
    fn find_option_matches_short_abbreviation() {
        assert_eq!(find_option(b"ea"), OptIndex::Equalalways);
    }

    #[test]
    fn find_option_matches_historical_alias() {
        // "viminfo"/"vi" are pre-rename aliases for "shada"/"sd".
        assert_eq!(find_option(b"viminfo"), OptIndex::Shada);
        assert_eq!(find_option(b"vi"), OptIndex::Shada);
        assert_eq!(find_option(b"shada"), OptIndex::Shada);
        assert_eq!(find_option(b"sd"), OptIndex::Shada);
    }

    #[test]
    fn find_option_unknown_name_is_invalid() {
        assert_eq!(find_option(b"notarealoption"), OptIndex::Invalid);
        assert_eq!(find_option(b""), OptIndex::Invalid);
    }

    #[test]
    fn find_option_is_case_sensitive() {
        // Option names are always lowercase; an uppercase variant must
        // not match.
        assert_eq!(find_option(b"Equalalways"), OptIndex::Invalid);
    }

    #[test]
    fn find_option_len_bounds_the_search_to_a_prefix() {
        assert_eq!(find_option_len(b"eaXXXX", 2), OptIndex::Equalalways);
    }

    #[test]
    fn find_option_len_out_of_bounds_is_invalid() {
        assert_eq!(find_option_len(b"ea", 10), OptIndex::Invalid);
    }

    #[test]
    fn find_option_end_plain_alpha_name() {
        let (end, idx) = find_option_end(b"equalalways more text");
        assert_eq!(end, Some(11));
        assert_eq!(idx, OptIndex::Equalalways);
    }

    #[test]
    fn find_option_end_short_name_with_trailing_equals() {
        let (end, idx) = find_option_end(b"ea=foo");
        assert_eq!(end, Some(2));
        assert_eq!(idx, OptIndex::Equalalways);
    }

    #[test]
    fn find_option_end_unknown_alpha_name_still_returns_end_but_invalid_idx() {
        // The scan itself only looks for alpha characters - an
        // unrecognized (but alpha-shaped) name still reports where it
        // ends, just with an Invalid index (matching the original's
        // own find_option_len returning kOptInvalid there).
        let (end, idx) = find_option_end(b"notarealoption=x");
        assert_eq!(end, Some(14));
        assert_eq!(idx, OptIndex::Invalid);
    }

    #[test]
    fn find_option_end_non_alpha_start_is_none() {
        assert_eq!(find_option_end(b"123"), (None, OptIndex::Invalid));
        assert_eq!(find_option_end(b""), (None, OptIndex::Invalid));
    }

    #[test]
    fn find_option_end_tty_option_takes_priority() {
        // "term"/"ttytype" are recognized as TTY options FIRST, before
        // the ordinary alpha-name/OPTIONS-table scan - matching the
        // original's own find_tty_option_end short-circuit.
        let (end, idx) = find_option_end(b"term");
        assert_eq!(end, Some(4));
        assert_eq!(idx, OptIndex::Invalid);
    }

    #[test]
    fn find_option_end_keycode_form_is_a_tty_option_with_invalid_idx() {
        let (end, idx) = find_option_end(b"t_kb=x");
        assert_eq!(end, Some(4));
        assert_eq!(idx, OptIndex::Invalid);
    }

    #[test]
    fn skip_to_option_part_skips_comma_and_spaces() {
        assert_eq!(skip_to_option_part(b",  next", 0), 3);
    }

    #[test]
    fn skip_to_option_part_no_comma_still_skips_spaces() {
        assert_eq!(skip_to_option_part(b"   next", 0), 3);
    }

    #[test]
    fn skip_to_option_part_neither_comma_nor_space_is_a_no_op() {
        assert_eq!(skip_to_option_part(b"next", 0), 0);
    }

    #[test]
    fn skip_to_option_part_at_end_of_string_is_a_no_op() {
        assert_eq!(skip_to_option_part(b"abc", 3), 3);
    }

    #[test]
    fn copy_option_part_isolates_up_to_a_separator() {
        let (part, next) = copy_option_part(b"foo,bar", 0, 30, b".,");
        assert_eq!(part, b"foo");
        assert_eq!(next, 4); // past the comma
        let (part2, next2) = copy_option_part(b"foo,bar", next, 30, b".,");
        assert_eq!(part2, b"bar");
        assert_eq!(next2, 7);
    }

    #[test]
    fn copy_option_part_keeps_a_leading_dot() {
        // The leading '.' skip is specifically for 'suffixes' entries
        // like ".bak" - it's copied, not treated as a separator itself
        // even when '.' is one of sep_chars.
        let (part, next) = copy_option_part(b".bak,~", 0, 30, b".,");
        assert_eq!(part, b".bak");
        assert_eq!(next, 5);
    }

    #[test]
    fn copy_option_part_empty_entry_between_commas() {
        let (part, next) = copy_option_part(b",,foo", 0, 30, b".,");
        assert_eq!(part, b"");
        assert_eq!(next, 1);
        let (part2, next2) = copy_option_part(b",,foo", next, 30, b".,");
        assert_eq!(part2, b"");
        assert_eq!(next2, 2);
        let (part3, _) = copy_option_part(b",,foo", next2, 30, b".,");
        assert_eq!(part3, b"foo");
    }

    #[test]
    fn copy_option_part_backslash_escapes_a_separator_character() {
        // "a\,b,c" - the backslash-comma is an escaped literal comma
        // within the FIRST part, not a part boundary. `next` lands on
        // the 'c' (index 5), NOT past it - "c" is still a whole
        // unconsumed part waiting for the next call, matching the
        // original's own pointer-arithmetic trace (only the FIRST
        // comma after "a\,b" is consumed as the part separator).
        let (part, next) = copy_option_part(b"a\\,b,c", 0, 30, b",");
        assert_eq!(part, b"a,b");
        assert_eq!(next, 5);
        assert_eq!(&b"a\\,b,c"[next..], b"c");
    }

    #[test]
    fn copy_option_part_truncates_to_maxlen_but_still_advances_past_the_whole_part() {
        // maxlen=4 means at most 3 bytes are copied (maxlen - 1), but
        // the cursor still advances past the ENTIRE "abcdef" part -
        // parsing correctly continues at "ghi" afterward.
        let (part, next) = copy_option_part(b"abcdef,ghi", 0, 4, b",");
        assert_eq!(part, b"abc");
        assert_eq!(next, 7);
        let (part2, _) = copy_option_part(b"abcdef,ghi", next, 4, b",");
        assert_eq!(part2, b"ghi");
    }

    #[test]
    fn copy_option_part_no_trailing_separator_reaches_the_end() {
        let (part, next) = copy_option_part(b"solo", 0, 30, b".,");
        assert_eq!(part, b"solo");
        assert_eq!(next, 4);
    }

    #[test]
    fn copy_option_part_space_separator_is_skipped_like_a_comma() {
        // sep_chars containing a space (matches expand_path_option's
        // own " ," call) - stopping on a space (not a comma) takes the
        // explicit "skip non-standard separator" branch.
        let (part, next) = copy_option_part(b"foo bar", 0, 30, b" ,");
        assert_eq!(part, b"foo");
        assert_eq!(next, 4);
    }
}

