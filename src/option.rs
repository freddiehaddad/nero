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
//! Also translated: `check_illegal_path_names` (`optionstr.c`) -
//! `did_set_option`'s own early "is this string value shaped like a
//! real file/directory name, or does it contain a shell-wildcard/
//! separator character that shouldn't be there" guard, needing only
//! `GLOBALS.secure`. Translated ahead of `did_set_option` itself
//! (still deferred, see below), matching this crate's established
//! "small, simple, mechanically correct piece ahead of its real
//! caller" precedent.
//!
//! Also translated: `do_syntax_autocmd` - `did_set_option`'s own
//! `'syntax'`-changed handler, called once `varp == &curbuf->b_p_syn`
//! is confirmed. Sets `BufT::BF_SYN_SET` and fires the `Syntax`
//! autocmd event via the already-real `crate::autocmd::apply_autocmds`
//! (always taking that function's own already-documented empty-
//! `AUTOCMDS` bypass path today, so this has no observable effect
//! beyond its own `SYN_RECURSIVE` recursion-depth bookkeeping and the
//! `BF_SYN_SET` flag update - becomes fully correct automatically once
//! a future session adds real `Syntax` autocmd registration).
//! Translated ahead of `did_set_option` itself, matching the same
//! "small, simple, mechanically correct piece ahead of its real
//! caller" precedent.
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
//! Also translated: `option_scope_idx` (`options[idx].scope_idx[scope]`,
//! trivial) and `set_option_sctx` (`did_set_option`'s own `sctx`-
//! tracking step, previously flagged above as "small, not yet
//! checked") - the per-option script-context bookkeeping recording
//! where each option was last set. The original's own
//! `options[opt_idx].script_ctx = script_ctx` write (for global
//! options) is modeled as a NEW side-table, `OPTION_SCRIPT_CTX`,
//! rather than a mutated `VimoptionT` field, exactly mirroring
//! `OPTION_WAS_SET`'s own already-established reasoning: `get_option`'s
//! safety doc relies on `VimoptionT` never being mutated once
//! `OPTIONS` is built, so a real write through a raw pointer into that
//! array would risk violating that invariant. Buffer-/window-local
//! script contexts use the already-real `BufT.b_p_script_ctx`/
//! `WinoptT.wo_script_ctx` fields directly (both `Vec<SctxT>`, grown
//! on demand via a new `ensure_sctx_len` helper, rather than pre-sized
//! up front - matching those fields' own already-documented "nothing
//! here depends on it being a fixed size" design intent). The
//! original's own `nlua_set_sctx(&script_ctx)` call is omitted
//! (Lua-diagnostic-message enhancement only, matching `eval/vars.rs`'s
//! `find_var_ht_dict`'s own identical omission). `SOURCING_LNUM`
//! (`runtime.rs`'s new `sourcing_lnum()`) is always `0` today, since
//! `EXESTACK` is always empty (see that module's own doc comment) -
//! the real, correct answer for "no active execution context", not a
//! hardcoded shortcut.
//!
//! Also translated: `crate::drawscreen::comp_col` (real screen-column
//! geometry - see that module's own doc comment), `do_syntax_autocmd`
//! (below), and `crate::autocmd::do_filetype_autocmd` (`autocmd.c`,
//! NOT inlined into `did_set_option` - a stale note here had
//! previously claimed otherwise; see `crate::autocmd`'s own module
//! doc) - `did_set_option`'s own 3 remaining real prerequisites beyond
//! `set_option_sctx`/`check_illegal_path_names` (previous paragraph).
//!
//! **`did_set_option` itself is now translated** (see its own doc
//! comment for its precise remaining gaps: the per-option callback
//! dispatch - currently unreachable for every option in this crate's
//! own table, see that doc comment -, `'spelllang'`'s
//! `do_spelllang_source`, and `'winbar'`'s real frame-layout
//! resizing). Every other piece of its own real body is genuinely
//! correct and complete today: `set_option_varp`'s error-path value
//! restoration, `set_option_sctx`'s script-context tracking,
//! `scope_both`'s global-local-unset handling, the 3 autocmd-trigger
//! pointer-identity checks, `comp_col`, `curwin.w_set_curswant`
//! bookkeeping, and the final `insecure_flag`-bit maintenance. The
//! redraw-only calls in its own tail (`setmouse`/`redraw_all_later`/
//! `check_redraw`) are omitted per this crate's established
//! `redraw_later`-omission precedent (see that function's own doc
//! comment for the exact reasoning at each call site). Its own real
//! callers (`set_option`/`set_option_value`/`do_set`) remain
//! untranslated - harvested ahead of them, matching this crate's
//! established "small, simple, mechanically correct piece ahead of
//! its real caller" precedent (here, simply a much larger "piece"
//! than usual, given how much groundwork had already accumulated
//! across many earlier commits this session).
//!
//! Deferred: the full `set_option_value`/`set_option`/
//! `validate_option_value` write pipeline around `did_set_option` -
//! each layer found to need a currently-blocked subsystem while
//! scoping this pass:
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
//! - `was_set_insecurely`/`insecure_flag` are now translated too (see
//!   below) - `insecure_flag` returns a real `*mut u32` into the
//!   correct `WAS_SET`/`INSECURE`-bit-bearing storage (global, or a
//!   local override when one exists), and `was_set_insecurely` simply
//!   checks the `INSECURE` bit through it.
//! - `option_was_set`/`reset_option_was_set` are now translated too
//!   (see below) - via a dedicated `OPTION_WAS_SET` side-table rather
//!   than mutating `OPTIONS[idx].flags` directly (see
//!   [`option_was_set`]'s own doc comment for why).
//! - `parse_winhl_opt` needs the decoration/highlight-group subsystem
//!   (`nvim_create_namespace`/`get_decor_provider`/`syn_check_group`/
//!   `ns_hl_def`).
//! - `do_set`/`ex_set`'s command-line parsing itself, plus
//!   `get_vimoption`/etc. (everything needing the full parsed-`:set`-
//!   argument machinery, not just a resolved storage address and a
//!   read/write). `get_winbuf_options` (below) does NOT need any of
//!   this, though - it only needs a resolved storage address per
//!   option (`get_varp`) plus the already-real `optval_from_varp`/
//!   `optval_as_tv`/`tv_dict_add_tv`, so it is translated here despite
//!   `do_set` itself still being deferred.

use crate::buffer_defs::{BufT, WinT};
use crate::eval::typval_defs::SctxT;
use crate::option_defs::{OptIndex, OptScope, OptValType, OptVal, VimoptionT, OPTIONS, OPT_COUNT};
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

/// Returns the declared value type of option `opt_idx`
/// (`option_get_type`).
#[must_use]
pub fn option_get_type(opt_idx: OptIndex) -> OptValType {
    get_option(opt_idx).r#type
}

/// Initializes tab-local `'cmdheight'` from its declared default
/// (`set_init_tablocal`).
///
/// The option describes itself as global in the generated table but
/// is actually tab-local, so it needs this explicit initialization.
///
/// # Safety
/// Mutates `OPTION_VARS.p_ch`.
pub unsafe fn set_init_tablocal() {
    let crate::option_defs::OptVal::Number(default) =
        &get_option(OptIndex::Cmdheight).def_val
    else {
        unreachable!("'cmdheight' must be a numeric option");
    };
    unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = *default;
}

/// Check if option supports a specific type (`option_has_type`).
#[must_use]
pub fn option_has_type(opt_idx: OptIndex, typ: OptValType) -> bool {
    opt_idx != OptIndex::Invalid && option_get_type(opt_idx) == typ
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

/// Get the option value that represents an "unset" local value for
/// `opt_idx` (`get_option_unset_value`).
///
/// For global-local options: string-typed ones always use an empty
/// string (`STATIC_CSTR_AS_OPTVAL("")` in the original); the handful
/// of non-string global-local options each get their own real,
/// individually-verified sentinel (`Autocomplete`/`Autoread`/`Fsync`
/// use [`TriState::None`]; `Scrolloff`/`Scrolloffpad`/`Sidescrolloff`
/// use `-1`; `Undolevels` uses [`crate::option_vars`]'s already-real
/// `NO_LOCAL_UNDOLEVEL`) - this exhaustive list mirrors the original's
/// own `switch`, whose `default: abort();` asserts no OTHER non-string
/// global-local option exists in `options[]` (verified true for this
/// crate's own 377-entry `OPTIONS` table, mechanically transcribed
/// from the same real generated source).
///
/// For options that are NOT global-local, the global value itself
/// represents "unset".
///
/// # Safety
/// `opt_idx` must not be [`OptIndex::Invalid`].
#[must_use]
pub unsafe fn get_option_unset_value(opt_idx: OptIndex) -> OptVal {
    if option_is_global_local(opt_idx) {
        if option_has_type(opt_idx, OptValType::String) {
            return OptVal::String(Vec::new());
        }
        return match opt_idx {
            OptIndex::Autocomplete | OptIndex::Autoread | OptIndex::Fsync => {
                OptVal::Boolean(TriState::None)
            }
            OptIndex::Scrolloff | OptIndex::Scrolloffpad | OptIndex::Sidescrolloff => {
                OptVal::Number(-1)
            }
            OptIndex::Undolevels => OptVal::Number(crate::option_vars::NO_LOCAL_UNDOLEVEL),
            _ => unreachable!(
                "get_option_unset_value: {opt_idx:?} is global-local and non-string, but \
                 isn't one of the 7 options with a known unset sentinel - matches the \
                 original's own `default: abort();`"
            ),
        };
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { optval_from_varp(opt_idx, get_varp_scope(opt_idx, crate::option_defs::opt_set_flags::OPT_GLOBAL)) }
}

/// Check if the local value of a global-local option is unset for the
/// current buffer/window. Always `false` for options that aren't
/// global-local (`is_option_local_value_unset`).
///
/// # Safety
/// `opt_idx` must not be [`OptIndex::Invalid`]; `crate::globals::
/// GLOBALS.curbuf`/`curwin` must be valid, non-null pointers to live
/// `BufT`/`WinT` values (forwarded to `get_varp_scope`'s own
/// `OPT_LOCAL` resolution, which reads through `curbuf`/`curwin` when
/// no explicit buffer/window is supplied - matching the original's own
/// unconditional reliance on those globals here).
#[must_use]
pub unsafe fn is_option_local_value_unset(opt_idx: OptIndex) -> bool {
    if !option_is_global_local(opt_idx) {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let local_value =
        unsafe { optval_from_varp(opt_idx, get_varp_scope(opt_idx, crate::option_defs::opt_set_flags::OPT_LOCAL)) };
    // SAFETY: forwarded from this function's own safety doc.
    let unset_local_value = unsafe { get_option_unset_value(opt_idx) };

    local_value == unset_local_value
}

/// The scope-specific index of option `opt_idx` at `scope`
/// (`option_scope_idx`, `options[opt_idx].scope_idx[scope]`) - e.g.
/// its position within `BufOptIndex`/`WinOptIndex` when `scope` is
/// `Buf`/`Win`, used to index into `BufT.b_p_script_ctx`/
/// `WinoptT.wo_script_ctx`.
///
/// # Panics
/// Debug-asserts `opt_idx != OptIndex::Invalid`, matching the
/// original's own `assert(opt_idx != kOptInvalid)`.
#[must_use]
pub fn option_scope_idx(opt_idx: OptIndex, scope: OptScope) -> isize {
    debug_assert!(opt_idx != OptIndex::Invalid);
    get_option(opt_idx).scope_idx[scope as usize]
}

/// Per-option script context (`options[idx].script_ctx` in the
/// original), for GLOBAL options only. Modeled as its OWN parallel
/// side-table, exactly mirroring [`OPTION_WAS_SET`]'s own already-
/// established reasoning (see its own doc comment): `get_option`'s
/// safety doc relies on `VimoptionT` never being mutated once
/// `OPTIONS` is built, so a dedicated side-table sidesteps introducing
/// a real write through a raw pointer into that array.
static OPTION_SCRIPT_CTX: crate::globals::GlobalCell<[SctxT; crate::option_defs::OPT_COUNT]> =
    crate::globals::GlobalCell::new(
        [SctxT { sc_sid: 0, sc_seq: 0, sc_lnum: 0, sc_chan: 0 }; crate::option_defs::OPT_COUNT],
    );

/// The script context recorded for GLOBAL option `opt_idx`
/// (`get_option_sctx`). Buffer-/window-local
/// script contexts live on `BufT.b_p_script_ctx`/
/// `WinoptT.wo_script_ctx` instead (see [`set_option_sctx`]'s own doc
/// comment) and are read directly through those fields, matching the
/// original exactly.
///
/// # Panics
/// Debug-asserts `opt_idx != OptIndex::Invalid`, matching
/// [`option_was_set`]'s own established convention for this exact
/// side-table shape.
#[must_use]
pub fn get_option_sctx(opt_idx: OptIndex) -> SctxT {
    debug_assert!(opt_idx != OptIndex::Invalid);
    // SAFETY: a plain Copy-out read through one exclusive borrow, no
    // aliasing hazard.
    let table = unsafe { OPTION_SCRIPT_CTX.get_mut() };
    table[opt_idx as usize]
}

/// Backwards-compatible descriptive alias for [`get_option_sctx`].
#[must_use]
pub fn option_script_ctx(opt_idx: OptIndex) -> SctxT {
    get_option_sctx(opt_idx)
}

/// Grow `v` in place (with `SctxT::default()` fill) so that
/// `v[idx]` is valid, if it isn't already.
///
/// `BufT.b_p_script_ctx`/`WinoptT.wo_script_ctx` are `Vec<SctxT>`
/// standing in for the original's own fixed-size
/// `sctx_T[kBufOptCount]`/`sctx_T[kWinOptCount]` arrays (a codegen-
/// derived size not available without running `src/gen/*.lua` -
/// flagged and deferred since phase 1, see each field's own doc
/// comment) - starting empty (`Vec::new()` via `#[derive(Default)]`)
/// and growing on first real write here, rather than pre-sizing every
/// `BufT`/`WinoptT` up front, matches those fields' own already-
/// documented "nothing here depends on it being a fixed size" design
/// intent exactly (no other code reads/writes these fields yet, so
/// this is the very first real consumer, not a retrofit).
fn ensure_sctx_len(v: &mut Vec<SctxT>, len: usize) {
    if v.len() < len {
        v.resize(len, SctxT::default());
    }
}

/// Set the script context for option `opt_idx` (`set_option_sctx`).
/// Remembers where the option was last set - in `OPTION_SCRIPT_CTX`
/// for a global option, or in the current buffer's/window's own
/// `b_p_script_ctx`/`wo_script_ctx` for a local one (both buffer- and
/// window-local `w_onebuf_opt`, plus `w_allbuf_opt` too when `both` is
/// set, matching the original's own "also setting the all buffers
/// value" branch).
///
/// The original's own `nlua_set_sctx(&script_ctx)` call is omitted -
/// a Lua-diagnostic-message enhancement only, matching
/// `eval/vars.rs`'s `find_var_ht_dict`'s own already-established
/// handling of the identical call (confirmed via direct reading that
/// it never touches any field this function itself reads or writes).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf`/`curwin` must be valid, non-null
/// pointers to live `BufT`/`WinT` values (only read/written through
/// when a local option is being recorded), matching `get_varp`'s own
/// established safety requirement.
pub unsafe fn set_option_sctx(opt_idx: OptIndex, opt_flags: u32, mut script_ctx: SctxT) {
    use crate::option_defs::opt_set_flags::{OPT_GLOBAL, OPT_LOCAL, OPT_MODELINE};

    let both = (opt_flags & (OPT_LOCAL | OPT_GLOBAL)) == 0;

    // Modeline already has the line number set.
    if opt_flags & OPT_MODELINE == 0 {
        script_ctx.sc_lnum = script_ctx.sc_lnum.wrapping_add(crate::runtime::sourcing_lnum());
    }

    // Remember where the option was set. For local options need to do
    // that in the buffer or window structure.
    if both || (opt_flags & OPT_GLOBAL != 0) || option_is_global_only(opt_idx) {
        // SAFETY: a plain Copy-in write through one exclusive borrow,
        // no aliasing hazard.
        let table = unsafe { OPTION_SCRIPT_CTX.get_mut() };
        table[opt_idx as usize] = script_ctx;
    }
    if both || (opt_flags & OPT_LOCAL != 0) {
        if option_has_scope(opt_idx, OptScope::Buf) {
            let idx = option_scope_idx(opt_idx, OptScope::Buf) as usize;
            // SAFETY: forwarded from this function's own safety doc.
            let buf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
            ensure_sctx_len(&mut buf.b_p_script_ctx, idx + 1);
            buf.b_p_script_ctx[idx] = script_ctx;
        } else if option_has_scope(opt_idx, OptScope::Win) {
            let idx = option_scope_idx(opt_idx, OptScope::Win) as usize;
            // SAFETY: forwarded from this function's own safety doc.
            let win = unsafe { &mut *crate::globals::GLOBALS.get_mut().curwin };
            ensure_sctx_len(&mut win.w_onebuf_opt.wo_script_ctx, idx + 1);
            win.w_onebuf_opt.wo_script_ctx[idx] = script_ctx;
            if both {
                // also setting the "all buffers" value
                ensure_sctx_len(&mut win.w_allbuf_opt.wo_script_ctx, idx + 1);
                win.w_allbuf_opt.wo_script_ctx[idx] = script_ctx;
            }
        }
    }
}

/// Record the current script context for a sentinel-terminated option
/// list (`didset_options_sctx`).
///
/// # Safety
/// Mutates option script-context state and reads `GLOBALS.current_sctx`.
#[allow(dead_code)]
unsafe fn didset_options_sctx(opt_flags: u32, options: &[OptIndex]) {
    // SAFETY: forwarded from this function's own safety doc.
    let current = unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx;
    for &option in options {
        if option == OptIndex::Invalid {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { set_option_sctx(option, opt_flags, current) };
    }
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

/// Get a pointer to the flags used for the `kOptFlagInsecure`/
/// `opt_flags::INSECURE` flag of `opt_idx`. For some local options a
/// local flags field is used instead of the global `options[]` table's
/// own (`insecure_flag`). Every branch below mirrors the original's own
/// `switch`es exactly.
///
/// NOTE (matches the original's own comment): caller must make sure
/// `wp` is the window the option is actually being used from.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` (and, for
/// the `OPT_LOCAL` branches touching `w_buffer`, its own `w_buffer`
/// must also be a valid, non-null pointer to a live `BufT`).
#[must_use]
pub unsafe fn insecure_flag(wp: *mut WinT, opt_idx: OptIndex, opt_flags: u32) -> *mut u32 { unsafe {
    if opt_flags & crate::option_defs::opt_set_flags::OPT_LOCAL != 0 {
        match opt_idx {
            OptIndex::Wrap => return std::ptr::addr_of_mut!((*wp).w_onebuf_opt.wo_wrap_flags),
            OptIndex::Statusline => return std::ptr::addr_of_mut!((*wp).w_onebuf_opt.wo_stl_flags),
            OptIndex::Winbar => return std::ptr::addr_of_mut!((*wp).w_onebuf_opt.wo_wbr_flags),
            OptIndex::Foldexpr => return std::ptr::addr_of_mut!((*wp).w_onebuf_opt.wo_fde_flags),
            OptIndex::Foldtext => return std::ptr::addr_of_mut!((*wp).w_onebuf_opt.wo_fdt_flags),
            OptIndex::Indentexpr => return std::ptr::addr_of_mut!((*(*wp).w_buffer).b_p_inde_flags),
            OptIndex::Formatexpr => return std::ptr::addr_of_mut!((*(*wp).w_buffer).b_p_fex_flags),
            OptIndex::Includeexpr => return std::ptr::addr_of_mut!((*(*wp).w_buffer).b_p_inex_flags),
            _ => {}
        }
    } else {
        // For global value of window-local options, use flags in w_allbuf_opt.
        match opt_idx {
            OptIndex::Wrap => return std::ptr::addr_of_mut!((*wp).w_allbuf_opt.wo_wrap_flags),
            OptIndex::Foldexpr => return std::ptr::addr_of_mut!((*wp).w_allbuf_opt.wo_fde_flags),
            OptIndex::Foldtext => return std::ptr::addr_of_mut!((*wp).w_allbuf_opt.wo_fdt_flags),
            _ => {}
        }
    }
    // Nothing special, return global flags field.
    std::ptr::addr_of_mut!((*opt_ptr(opt_idx)).flags)
}}

/// Check whether option `opt_idx` was set insecurely (e.g. from a
/// modeline) (`was_set_insecurely`).
///
/// # Safety
/// Forwarded from [`insecure_flag`]'s own safety doc.
#[must_use]
pub unsafe fn was_set_insecurely(wp: *mut WinT, opt_idx: OptIndex, opt_flags: u32) -> bool {
    debug_assert!(opt_idx != OptIndex::Invalid);
    // SAFETY: forwarded from this function's own safety doc.
    let flagp = unsafe { insecure_flag(wp, opt_idx, opt_flags) };
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { *flagp } & crate::option_defs::opt_flags::INSECURE) != 0
}

/// Per-option "was this option ever explicitly `:set`?" bits
/// (`kOptFlagWasSet`, backing [`option_was_set`]/
/// [`reset_option_was_set`]). Modeled as its OWN parallel side-table,
/// rather than a bit mutated directly on `OPTIONS[idx].flags` the way
/// the original's `options[opt_idx].flags |= kOptFlagWasSet` does:
/// [`get_option`]'s own safety doc relies on `VimoptionT` NEVER being
/// mutated once `OPTIONS` is built, so introducing the first real
/// write through a raw pointer into that array (as opposed to
/// `insecure_flag`'s own pre-existing, so-far read-only, forward-
/// looking `*mut u32` return value) would risk violating that
/// existing, load-bearing invariant. A dedicated side-table sidesteps
/// this entirely, matching this crate's own established "necessarily-
/// adapted representation, not a literal reinterpretation" precedent
/// (e.g. `DictitemVariant`, `BhData`).
static OPTION_WAS_SET: crate::globals::GlobalCell<[bool; crate::option_defs::OPT_COUNT]> =
    crate::globals::GlobalCell::new([false; crate::option_defs::OPT_COUNT]);

static P_ET_NOBIN: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
static P_ML_NOBIN: crate::globals::GlobalCell<i32> =
    crate::globals::GlobalCell::new(0);
static P_TW_NOBIN: crate::globals::GlobalCell<crate::types_defs::OptInt> =
    crate::globals::GlobalCell::new(0);
static P_WM_NOBIN: crate::globals::GlobalCell<crate::types_defs::OptInt> =
    crate::globals::GlobalCell::new(0);

/// Save, clear, or restore the options coupled to `'binary'`
/// (`set_options_bin`).
///
/// # Safety
/// `GLOBALS.curbuf` must point to a live buffer and option/global state
/// must not be mutated concurrently.
pub unsafe fn set_options_bin(old_value: i32, new_value: i32, opt_flags: u32) {
    use crate::option_defs::{opt_set_flags as scope, OptIndex};

    let curbuf = unsafe { &mut *crate::globals::GLOBALS.as_ptr() }.curbuf;
    let buf = unsafe { &mut *curbuf };
    let options = unsafe { crate::option_vars::OPTION_VARS.get_mut() };

    if new_value != 0 {
        if old_value == 0 {
            if opt_flags & scope::OPT_GLOBAL == 0 {
                buf.b_p_tw_nobin = buf.b_p_tw;
                buf.b_p_wm_nobin = buf.b_p_wm;
                buf.b_p_ml_nobin = buf.b_p_ml;
                buf.b_p_et_nobin = buf.b_p_et;
            }
            if opt_flags & scope::OPT_LOCAL == 0 {
                *unsafe { P_TW_NOBIN.get_mut() } = options.p_tw;
                *unsafe { P_WM_NOBIN.get_mut() } = options.p_wm;
                *unsafe { P_ML_NOBIN.get_mut() } = options.p_ml;
                *unsafe { P_ET_NOBIN.get_mut() } = options.p_et;
            }
        }
        if opt_flags & scope::OPT_GLOBAL == 0 {
            buf.b_p_tw = 0;
            buf.b_p_wm = 0;
            buf.b_p_ml = 0;
            buf.b_p_et = 0;
        }
        if opt_flags & scope::OPT_LOCAL == 0 {
            options.p_tw = 0;
            options.p_wm = 0;
            options.p_ml = 0;
            options.p_et = 0;
            options.p_bin = 1;
        }
    } else if old_value != 0 {
        if opt_flags & scope::OPT_GLOBAL == 0 {
            buf.b_p_tw = buf.b_p_tw_nobin;
            buf.b_p_wm = buf.b_p_wm_nobin;
            buf.b_p_ml = buf.b_p_ml_nobin;
            buf.b_p_et = buf.b_p_et_nobin;
        }
        if opt_flags & scope::OPT_LOCAL == 0 {
            options.p_tw = *unsafe { P_TW_NOBIN.get_mut() };
            options.p_wm = *unsafe { P_WM_NOBIN.get_mut() };
            options.p_ml = *unsafe { P_ML_NOBIN.get_mut() };
            options.p_et = *unsafe { P_ET_NOBIN.get_mut() };
        }
    }

    unsafe {
        didset_options_sctx(
            opt_flags,
            &[
                OptIndex::Textwidth,
                OptIndex::Wrapmargin,
                OptIndex::Modeline,
                OptIndex::Expandtab,
                OptIndex::Invalid,
            ],
        )
    };
}

/// Process an updated `'binary'` option (`did_set_binary`).
///
/// # Safety
/// `args.os_buf` and `GLOBALS.curbuf` must point to the same live
/// buffer, matching the option-setting context established by the
/// caller.
pub unsafe fn did_set_binary(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let buf = unsafe { &*(args.os_buf as *const crate::buffer_defs::BufT) };
    let old = match &args.os_oldval {
        crate::option_defs::OptVal::Boolean(value) => *value as i32,
        _ => 0,
    };
    unsafe { set_options_bin(old, buf.b_p_bin, args.os_flags as u32) };
    // `redraw_titles()` is redraw scheduling only.
    None
}

/// Process an updated `'iminsert'` option (`did_set_iminsert`).
///
/// # Safety
/// `GLOBALS.curbuf`/`curwin` and the current window list must be live
/// and valid.
pub unsafe fn did_set_iminsert(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // `showmode()` is immediate rendering only.
    unsafe { crate::drawscreen::status_redraw_curbuf() };
    None
}

/// Record the first vimrc path in an environment variable
/// (`vimrc_found`).
///
/// # Safety
/// Must satisfy [`crate::os::env::vim_getenv`] and
/// [`crate::os::env::os_setenv`]'s environment-state contracts.
pub unsafe fn vimrc_found(fname: Option<&[u8]>, envname: Option<&[u8]>) {
    let (Some(fname), Some(envname)) = (fname, envname) else {
        return;
    };
    if unsafe { crate::os::env::vim_getenv(envname) }.is_none()
        && let Some(fullname) = crate::path::full_name_save(Some(fname), false)
    {
        let _ = unsafe { crate::os::env::os_setenv(envname, &fullname, 1) };
    }
}

/// Schedule redraw work required after an option changes
/// (`check_redraw_for`).
///
/// # Safety
/// `buf`/`win` must be valid whenever the selected flags use them,
/// and global window lists must satisfy the called redraw helpers.
pub unsafe fn check_redraw_for(
    buf: *mut crate::buffer_defs::BufT,
    win: *mut crate::buffer_defs::WinT,
    flags: u32,
) {
    use crate::option_defs::opt_flags as flag;

    let all = flags & flag::REDR_ALL == flag::REDR_ALL;
    if flags & flag::REDR_STAT != 0 || all {
        unsafe { crate::drawscreen::status_redraw_all() };
    }
    if flags & flag::REDR_TABL != 0 || all {
        unsafe { crate::globals::GLOBALS.get_mut() }.redraw_tabline = true;
    }
    if flags & (flag::REDR_BUF | flag::REDR_WIN) != 0 || all {
        if flags & flag::HL_ONLY != 0 {
            unsafe { crate::drawscreen::redraw_later(win, crate::drawscreen::UPD_NOT_VALID) };
        } else {
            unsafe { crate::r#move::changed_window_setting(win) };
        }
    }
    if flags & flag::REDR_BUF != 0 {
        unsafe { crate::drawscreen::redraw_buf_later(buf, crate::drawscreen::UPD_NOT_VALID) };
    }
    if all {
        unsafe { crate::drawscreen::redraw_all_later(crate::drawscreen::UPD_NOT_VALID) };
    }
}

/// [`check_redraw_for`] for the current buffer/window (`check_redraw`).
///
/// # Safety
/// `GLOBALS.curbuf`/`curwin` and the global window lists must be valid.
pub unsafe fn check_redraw(flags: u32) {
    let globals = unsafe { &*crate::globals::GLOBALS.as_ptr() };
    unsafe { check_redraw_for(globals.curbuf, globals.curwin, flags) };
}

#[cfg(test)]
mod option_redraw_tests {
    use super::*;

    #[test]
    fn redraw_dispatch_sets_tabline_and_highlight_only_window_redraw() {
        let _lock = crate::globals::global_state_test_lock();
        let _tabline = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.redraw_tabline,
                false,
            )
        };
        unsafe {
            check_redraw_for(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                crate::option_defs::opt_flags::REDR_TABL,
            );
        }
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.redraw_tabline);

        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = std::ptr::from_mut(&mut win);
        unsafe {
            check_redraw_for(
                std::ptr::null_mut(),
                win_ptr,
                crate::option_defs::opt_flags::REDR_WIN
                    | crate::option_defs::opt_flags::HL_ONLY,
            );
        }
        assert_eq!(unsafe { (*win_ptr).w_redr_type }, crate::drawscreen::UPD_NOT_VALID);
    }
}

#[cfg(test)]
mod binary_option_tests {
    use super::*;

    #[test]
    fn binary_mode_saves_clears_and_restores_dependent_options() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_tw: 10,
            b_p_wm: 2,
            b_p_ml: 1,
            b_p_et: 1,
            ..Default::default()
        };
        let buf_ptr = std::ptr::from_mut(&mut buf);
        let _curbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr)
        };
        let saved_options = {
            let options = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let saved = (options.p_tw, options.p_wm, options.p_ml, options.p_et, options.p_bin);
            (options.p_tw, options.p_wm, options.p_ml, options.p_et, options.p_bin) =
                (20, 3, 1, 1, 0);
            saved
        };
        let saved_nobin = (
            *unsafe { P_TW_NOBIN.get_mut() },
            *unsafe { P_WM_NOBIN.get_mut() },
            *unsafe { P_ML_NOBIN.get_mut() },
            *unsafe { P_ET_NOBIN.get_mut() },
        );
        let saved_context = *unsafe { OPTION_SCRIPT_CTX.get_mut() };

        unsafe { set_options_bin(0, 1, 0) };
        assert_eq!((buf.b_p_tw, buf.b_p_wm, buf.b_p_ml, buf.b_p_et), (0, 0, 0, 0));
        {
            let options = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            assert_eq!((options.p_tw, options.p_wm, options.p_ml, options.p_et, options.p_bin), (0, 0, 0, 0, 1));
        }
        unsafe { set_options_bin(1, 0, 0) };
        assert_eq!((buf.b_p_tw, buf.b_p_wm, buf.b_p_ml, buf.b_p_et), (10, 2, 1, 1));
        {
            let options = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            assert_eq!((options.p_tw, options.p_wm, options.p_ml, options.p_et), (20, 3, 1, 1));
            (options.p_tw, options.p_wm, options.p_ml, options.p_et, options.p_bin) =
                saved_options;
        }
        (
            *unsafe { P_TW_NOBIN.get_mut() },
            *unsafe { P_WM_NOBIN.get_mut() },
            *unsafe { P_ML_NOBIN.get_mut() },
            *unsafe { P_ET_NOBIN.get_mut() },
        ) = saved_nobin;
        *unsafe { OPTION_SCRIPT_CTX.get_mut() } = saved_context;
    }

    #[test]
    fn did_set_binary_applies_buffer_local_binary_defaults() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_bin: 1,
            b_p_tw: 10,
            b_p_wm: 2,
            b_p_ml: 1,
            b_p_et: 1,
            ..Default::default()
        };
        let buf_ptr = std::ptr::from_mut(&mut buf);
        let _curbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr)
        };
        let mut args = crate::option_defs::OptsetT {
            os_buf: buf_ptr.cast(),
            os_oldval: crate::option_defs::OptVal::Boolean(
                crate::types_defs::TriState::False,
            ),
            os_flags: crate::option_defs::opt_set_flags::OPT_LOCAL as i32,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_binary(&mut args) }, None);
        assert_eq!(
            (buf.b_p_tw, buf.b_p_wm, buf.b_p_ml, buf.b_p_et),
            (0, 0, 0, 0)
        );
    }
}

#[cfg(test)]
mod iminsert_option_tests {
    use super::*;

    #[test]
    fn did_set_iminsert_marks_current_buffer_statusline() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT {
            w_status_height: 1,
            ..Default::default()
        };
        let buf_ptr = std::ptr::from_mut(&mut buf);
        let win_ptr = std::ptr::from_mut(&mut win);
        unsafe { (*win_ptr).w_buffer = buf_ptr };
        let _curbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr)
        };
        let _curwin = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.curwin, win_ptr)
        };
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.firstwin, win_ptr)
        };
        assert_eq!(
            unsafe { did_set_iminsert(&mut crate::option_defs::OptsetT::default()) },
            None
        );
        assert!(unsafe { (*win_ptr).w_redr_status });
    }
}

#[cfg(test)]
mod vimrc_found_tests {
    use super::*;

    #[test]
    fn vimrc_found_sets_only_an_absent_environment_variable() {
        let name = "NERO_TEST_FIRST_VIMRC";
        let old = std::env::var_os(name);
        unsafe { std::env::remove_var(name) };
        let expected = crate::path::full_name_save(Some(b"first.vim"), false).unwrap();
        unsafe { vimrc_found(Some(b"first.vim"), Some(name.as_bytes())) };
        assert_eq!(
            std::env::var_os(name).unwrap().as_encoded_bytes(),
            expected
        );

        unsafe { std::env::set_var(name, "existing.vim") };
        unsafe { vimrc_found(Some(b"second.vim"), Some(name.as_bytes())) };
        assert_eq!(std::env::var_os(name).unwrap(), "existing.vim");

        match old {
            Some(value) => unsafe { std::env::set_var(name, value) },
            None => unsafe { std::env::remove_var(name) },
        }
    }
}

/// Whether option `opt_idx` has ever been explicitly `:set`
/// (`option_was_set`). Set for real by [`did_set_option`] (via
/// [`set_option_was_set`], below) whenever an option is successfully
/// set through it - matching the original's own `opt->flags |=
/// kOptFlagWasSet`, translated as a side-table write instead (see
/// `OPTION_WAS_SET`'s own doc comment).
///
/// # Panics
/// Debug-asserts `opt_idx != OptIndex::Invalid`, matching the
/// original's own `assert(opt_idx != kOptInvalid)`.
#[must_use]
pub fn option_was_set(opt_idx: OptIndex) -> bool {
    debug_assert!(opt_idx != OptIndex::Invalid);
    // SAFETY: a plain bool copy-out read through one exclusive borrow,
    // no aliasing hazard.
    let table = unsafe { OPTION_WAS_SET.get_mut() };
    table[opt_idx as usize]
}

/// Record that option `opt_idx` has been explicitly `:set`
/// (`opt->flags |= kOptFlagWasSet` in the original, mutating
/// `options[opt_idx].flags` directly - translated as a side-table
/// write instead, exactly mirroring [`reset_option_was_set`]'s own
/// established reasoning). [`did_set_option`]'s own real caller.
///
/// # Panics
/// Debug-asserts `opt_idx != OptIndex::Invalid`, matching
/// [`option_was_set`]'s own established convention for this exact
/// side-table shape.
pub fn set_option_was_set(opt_idx: OptIndex) {
    debug_assert!(opt_idx != OptIndex::Invalid);
    // SAFETY: a plain bool write-through-exclusive-borrow, no
    // aliasing hazard.
    let table = unsafe { OPTION_WAS_SET.get_mut() };
    table[opt_idx as usize] = true;
}

/// Reset the flag indicating option `opt_idx` was set
/// (`reset_option_was_set`).
///
/// # Panics
/// Debug-asserts `opt_idx != OptIndex::Invalid`, matching the
/// original's own `assert(opt_idx != kOptInvalid)`.
pub fn reset_option_was_set(opt_idx: OptIndex) {
    debug_assert!(opt_idx != OptIndex::Invalid);
    // SAFETY: a plain bool write-through-exclusive-borrow, no
    // aliasing hazard.
    let table = unsafe { OPTION_WAS_SET.get_mut() };
    table[opt_idx as usize] = false;
}

/// Whether `val` (a prospective new string-option value) contains a
/// character illegal for a normal file name (`kOptFlagNFname`) or
/// directory name (`kOptFlagNDname`) option - e.g. `'*'`/`'?'`/`'['`
/// are shell-wildcard-shaped and never allowed in either, while
/// `'/'`/`'\\'`/`'<'`/`'>'` are additionally disallowed for a bare
/// file name (not a full path) unless `sandbox`-adjacent
/// `GLOBALS.secure` mode further restricts it with `'|'`/`';'`/`'&'`
/// too (`check_illegal_path_names`, `optionstr.c`).
///
/// `groups.contains(b)`-style membership checks replace the
/// original's own `strpbrk(val, charset) != NULL` (does ANY character
/// from `charset` appear anywhere in `val`) - `.iter().any(...)`
/// over a fixed byte-set is an exact, idiomatic Rust equivalent, with
/// no dependency on `val` being NUL-terminated (matching this crate's
/// own "option string values carry no trailing NUL" convention).
///
/// # Safety
/// Touches `crate::globals::GLOBALS.secure`.
#[must_use]
pub unsafe fn check_illegal_path_names(val: &[u8], flags: u32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let secure = unsafe { crate::globals::GLOBALS.get_mut() }.secure;
    let nfname_chars: &[u8] =
        if secure != 0 { b"/\\*?[|;&<>\r\n" } else { b"/\\*?[<>\r\n" };
    (flags & crate::option_defs::opt_flags::NFNAME != 0
        && val.iter().any(|b| nfname_chars.contains(b)))
        || (flags & crate::option_defs::opt_flags::NDNAME != 0
            && val.iter().any(|b| b"*?[|;&<>\r\n".contains(b)))
}

/// Recursion depth guard for [`do_syntax_autocmd`] (`syn_recursive`,
/// a function-local `static int` in the original).
static SYN_RECURSIVE: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// When `'syntax'` is set, load the syntax of that name
/// (`do_syntax_autocmd`).
///
/// Only passes `force = true` to the `Syntax` autocmd event (via
/// [`crate::autocmd::apply_autocmds`]) when the value changed or this
/// isn't a recursive call, to avoid endless recurrence. Since
/// `AUTOCMDS[EventT::Syntax]` is always empty today (nothing in this
/// crate can register a real autocmd yet), `apply_autocmds` always
/// takes its own already-documented bypass path regardless of `force`,
/// so this call has no observable effect beyond the recursion-depth
/// bookkeeping today, but becomes fully correct automatically the
/// moment a future session adds real `Syntax` autocmd registration,
/// with no changes needed here.
///
/// # Safety
/// `buf` must be a valid, non-null pointer to a live [`BufT`].
pub unsafe fn do_syntax_autocmd(buf: *mut BufT, value_changed: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let syn_recursive = unsafe { SYN_RECURSIVE.get_mut() };
    *syn_recursive += 1;
    let force = value_changed || *syn_recursive == 1;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*buf).b_flags |= crate::buffer_defs::b_flags::BF_SYN_SET as i32 };
    // SAFETY: forwarded from this function's own safety doc; a shared
    // reference (rather than `&mut BufT` as just used above) since
    // `apply_autocmds` only needs read access, and taking one avoids
    // any mutable/shared-borrow conflict across the several field
    // reads below.
    let b = unsafe { &*buf };
    let _ = crate::autocmd::apply_autocmds(
        crate::autocmd_defs::EventT::Syntax,
        b.b_p_syn.as_deref(),
        b.b_fname.as_deref(),
        force,
        Some(b),
    );

    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { SYN_RECURSIVE.get_mut() } -= 1;
}

/// Handle side-effects of setting an option (`did_set_option`).
///
/// The `opt.opt_did_set_cb.is_some()` branch `unimplemented!()`s at
/// the exact point a real per-option callback would be invoked -
/// currently unreachable for EVERY option in this crate's own
/// [`OPTIONS`] table (all 377 entries have `opt_did_set_cb: None`
/// today, matching `option_defs.rs`'s own module doc: none of the
/// ~150 real `did_set_*`/`expand_*` callback FUNCTIONS have been
/// translated yet, so there is nothing for any entry's own
/// `opt_did_set_cb` to reference - this is a stronger statement than
/// the original's own "188/377 options have one, 189 don't" split,
/// which describes upstream neovim's real callback population, not
/// this crate's own not-yet-populated one). Kept as a real branch
/// (not omitted) so a future session translating even a single real
/// callback and wiring it into that option's own [`OPTIONS`] entry
/// makes this genuinely reachable, with no restructuring needed here.
///
/// Also `unimplemented!()`s for 2 further real-but-substantial
/// branches: `curwin.w_s.b_p_spl` (`'spelllang'`, needs
/// `do_spelllang_source`'s own `source_runtime_vim_lua` - real file
/// I/O/script sourcing) and `p_wbr`/`curwin.w_onebuf_opt.wo_wbr`
/// (`'winbar'`, needs `set_winbar`/`set_winbar_all`'s own real frame-
/// layout resizing, `window.c`'s `set_winbar_win`/`win_set_inner_size`,
/// substantially more than a redraw hint unlike every other omitted
/// call in this function's own tail).
///
/// Since nothing in this scope ever runs a real per-option callback,
/// `value_changed`/`value_checked`/`restore_chartab` (the callback's
/// own `os_value_changed`/`os_value_checked`/`os_restore_chartab`
/// outputs) can only ever be `false` here - kept as real, named
/// bindings (not inlined `false` literals at each use) so a future
/// session wiring up even one real callback can just start assigning
/// them, with no restructuring needed here. The original's own
/// `buf_init_chartab(curbuf, true)` call (reached only when
/// `restore_chartab` is true) is therefore unreachable in this scope
/// and not translated.
///
/// Drops `errbuf`/`errbuflen` from the original's own signature
/// entirely: both are used ONLY to populate `optset_T`'s own fields
/// for the (here, `unimplemented!()`'d) per-option callback dispatch;
/// this function's own 3 real error messages are all static
/// `&'static str` constants, never written through a caller-supplied
/// buffer.
///
/// Returns `None` on success, `Some(message)` on error - collapsing
/// the original's own `NULL`/non-`NULL` `const char *` return into an
/// `Option`, matching this crate's usual preference.
///
/// # Safety
/// `varp` must be a valid, non-null pointer of the correct concrete
/// type for `opt_idx`'s own declared `OptValType` (see
/// [`optval_from_varp`]'s own safety doc for the exact mapping).
/// `crate::globals::GLOBALS.curbuf`/`curwin` must be valid, non-null
/// pointers to live `BufT`/`WinT` values, and `curwin.w_s` must be a
/// valid, non-null pointer to a live `SynblockT`.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub unsafe fn did_set_option(
    opt_idx: OptIndex,
    varp: *mut c_void,
    old_value: OptVal,
    mut new_value: OptVal,
    opt_flags: u32,
    set_sid: crate::eval::typval_defs::ScidT,
    direct: bool,
    value_replaced: bool,
) -> Option<&'static str> {
    let opt = get_option(opt_idx);
    let mut errmsg: Option<&'static str> = None;
    let value_changed = false;
    let value_checked = false;

    // Disallow changing some options from secure mode.
    // SAFETY: momentary read.
    let disallowed_in_secure_mode = {
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        (g.secure != 0 || g.sandbox != 0) && (opt.flags & crate::option_defs::opt_flags::SECURE != 0)
    };

    if direct {
        // Don't do any extra processing if setting directly.
    } else if opt.immutable && old_value != new_value {
        // Disallow changing immutable options.
        errmsg = Some(crate::errors::e_unsupportedoption);
    } else if disallowed_in_secure_mode {
        errmsg = Some(crate::errors::e_secure);
    } else if let OptVal::String(ref s) = new_value
        // Check for a "normal" directory or file name in some string options.
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { check_illegal_path_names(s, opt.flags) }
    {
        errmsg = Some(crate::errors::e_invarg);
    } else if opt.opt_did_set_cb.is_some() {
        // Invoke the option specific callback function to validate and
        // apply the new value.
        unimplemented!(
            "did_set_option: {opt_idx:?} has a real opt_did_set_cb - only the 189/377 \
             options without one are translated so far"
        );
    }

    // If option is hidden or if an error is detected, restore the
    // previous value and don't do any further processing.
    if let Some(e) = errmsg {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { set_option_varp(opt_idx, varp, old_value) };
        return Some(e);
    }

    // Re-assign the new value as its value may get freed or modified
    // by the option callback.
    // SAFETY: forwarded from this function's own safety doc.
    new_value = unsafe { optval_from_varp(opt_idx, varp) };

    if set_sid != crate::globals::SID_NONE {
        let script_ctx = if set_sid == 0 {
            // SAFETY: momentary read.
            unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx
        } else {
            crate::eval::typval_defs::SctxT { sc_sid: set_sid, ..Default::default() }
        };
        // Remember where the option was set.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { set_option_sctx(opt_idx, opt_flags, script_ctx) };
    }

    // optval_free(old_value): `Vec<u8>`'s own `Drop` already frees the
    // `String` variant's storage (`Nil`/`Boolean`/`Number` need no
    // cleanup at all) - an explicit `drop` here (rather than letting
    // `old_value` fall out of scope implicitly at the function's own
    // end) mirrors the original's own explicit release point.
    drop(old_value);

    let scope_both = (opt_flags
        & (crate::option_defs::opt_set_flags::OPT_LOCAL | crate::option_defs::opt_set_flags::OPT_GLOBAL))
        == 0;

    if scope_both {
        if option_is_global_local(opt_idx) {
            // Global option with local value set to use global value.
            // Free the local value and clear it.
            // SAFETY: forwarded from this function's own safety doc.
            let varp_local = unsafe { get_varp_scope(opt_idx, crate::option_defs::opt_set_flags::OPT_LOCAL) };
            // SAFETY: forwarded from this function's own safety doc.
            let local_unset_value = unsafe { get_option_unset_value(opt_idx) };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { set_option_varp(opt_idx, varp_local, local_unset_value) };
        } else {
            // May set global value for local option.
            // SAFETY: forwarded from this function's own safety doc.
            let varp_global = unsafe { get_varp_scope(opt_idx, crate::option_defs::opt_set_flags::OPT_GLOBAL) };
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { set_option_varp(opt_idx, varp_global, new_value.clone()) };
        }
    }

    // Don't do anything else if setting the option directly.
    if direct {
        return errmsg;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let curbuf = g.curbuf;
    let curwin = g.curwin;

    // SAFETY: forwarded from this function's own safety doc.
    let b_p_syn_ptr = unsafe { std::ptr::addr_of_mut!((*curbuf).b_p_syn) as *mut c_void };
    // SAFETY: forwarded from this function's own safety doc.
    let b_p_ft_ptr = unsafe { std::ptr::addr_of_mut!((*curbuf).b_p_ft) as *mut c_void };
    // SAFETY: forwarded from this function's own safety doc.
    let b_p_spl_ptr = unsafe { std::ptr::addr_of_mut!((*(*curwin).w_s).b_p_spl) as *mut c_void };

    // Trigger the autocommand only after setting the flags.
    if varp == b_p_syn_ptr {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { do_syntax_autocmd(curbuf, value_changed) };
    } else if varp == b_p_ft_ptr {
        // 'filetype' is set, trigger the FileType autocommand. Skip
        // this when called from a modeline. Force autocmd when the
        // filetype was changed.
        if opt_flags & crate::option_defs::opt_set_flags::OPT_MODELINE == 0 || value_changed {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::autocmd::do_filetype_autocmd(curbuf, value_changed) };
        }
    } else if varp == b_p_spl_ptr {
        unimplemented!(
            "did_set_option: 'spelllang' needs do_spelllang_source's own \
             source_runtime_vim_lua (real file I/O/script sourcing), not yet translated"
        );
    }

    // In case 'ruler' or 'showcmd' or 'columns' or 'ls' changed.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::drawscreen::comp_col() };

    // SAFETY: momentary reads, no aliasing.
    let p_mouse_ptr = unsafe { std::ptr::addr_of_mut!(crate::option_vars::OPTION_VARS.get_mut().p_mouse) as *mut c_void };
    // SAFETY: momentary reads, no aliasing.
    let p_flp_ptr = unsafe { std::ptr::addr_of_mut!(crate::option_vars::OPTION_VARS.get_mut().p_flp) as *mut c_void };
    // SAFETY: forwarded from this function's own safety doc.
    let b_p_flp_ptr = unsafe { std::ptr::addr_of_mut!((*curbuf).b_p_flp) as *mut c_void };
    // SAFETY: momentary reads, no aliasing.
    let p_wbr_ptr = unsafe { std::ptr::addr_of_mut!(crate::option_vars::OPTION_VARS.get_mut().p_wbr) as *mut c_void };
    // SAFETY: forwarded from this function's own safety doc.
    let w_p_wbr_ptr = unsafe { std::ptr::addr_of_mut!((*curwin).w_onebuf_opt.wo_wbr) as *mut c_void };
    // SAFETY: forwarded from this function's own safety doc.
    let w_briopt_list = unsafe { &*curwin }.w_briopt_list;

    if varp == p_mouse_ptr {
        // setmouse() omitted: pure UI-cursor-shape/mouse-on-off
        // dispatch, matching this crate's established `ui_cursor_shape`
        // omission precedent (see `ex_docmd.rs`'s
        // `restore_current_state` for the identical reasoning).
    } else if (varp == p_flp_ptr || varp == b_p_flp_ptr) && w_briopt_list != 0 {
        // redraw_all_later(UPD_NOT_VALID) omitted: pure redraw
        // scheduling, matching this crate's established `redraw_later`
        // omission precedent.
    } else if varp == p_wbr_ptr || varp == w_p_wbr_ptr {
        unimplemented!(
            "did_set_option: 'winbar' needs set_winbar/set_winbar_all's own real \
             frame-layout resizing (window.c's set_winbar_win/win_set_inner_size), \
             not yet translated"
        );
    }

    // SAFETY: forwarded from this function's own safety doc.
    let curwin_ref = unsafe { &mut *curwin };
    if curwin_ref.w_curswant != crate::pos_defs::MAXCOL
        && (opt.flags & (crate::option_defs::opt_flags::CURSWANT | crate::option_defs::opt_flags::REDR_ALL)) != 0
        && (opt.flags & crate::option_defs::opt_flags::HL_ONLY) == 0
    {
        curwin_ref.w_set_curswant = true;
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_redraw(opt.flags) };

    if errmsg.is_none() {
        set_option_was_set(opt_idx);

        // SAFETY: forwarded from this function's own safety doc.
        let flagsp = unsafe { insecure_flag(curwin, opt_idx, opt_flags) };
        let flagsp_local = if scope_both {
            // SAFETY: forwarded from this function's own safety doc.
            Some(unsafe { insecure_flag(curwin, opt_idx, crate::option_defs::opt_set_flags::OPT_LOCAL) })
        } else {
            None
        };

        // SAFETY: momentary read.
        let g2 = unsafe { crate::globals::GLOBALS.get_mut() };
        // When an option is set in the sandbox, from a modeline or in
        // secure mode set the kOptFlagInsecure flag. Otherwise, if a
        // new value is stored reset the flag.
        if !value_checked
            && (g2.secure != 0 || g2.sandbox != 0 || (opt_flags & crate::option_defs::opt_set_flags::OPT_MODELINE != 0))
        {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { *flagsp |= crate::option_defs::opt_flags::INSECURE };
            if let Some(fl) = flagsp_local {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { *fl |= crate::option_defs::opt_flags::INSECURE };
            }
        } else if value_replaced {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { *flagsp &= !crate::option_defs::opt_flags::INSECURE };
            if let Some(fl) = flagsp_local {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { *fl &= !crate::option_defs::opt_flags::INSECURE };
            }
        }
    }

    errmsg
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
pub unsafe fn get_varp_from(opt_idx: OptIndex, buf: *mut BufT, win: *mut WinT) -> *mut c_void { unsafe {
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
}}

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

/// Get every buffer-local (`bufopt == true`) or window-local
/// (`bufopt == false`) option's current effective value, as a real
/// `name -> value` `Dict` (`get_winbuf_options`) - the current
/// buffer's/window's own local option values, used by (in the
/// original) `getbufvar()`/`getwinvar()`'s bare `"&"` (whole-options-
/// dict) argument form.
///
/// # Safety
/// Forwarded from [`get_varp`]'s own safety doc.
#[must_use]
pub unsafe fn get_winbuf_options(bufopt: bool) -> *mut crate::eval::typval_defs::DictT {
    let d = crate::eval::typval::tv_dict_alloc();

    for idx in 0..OPT_COUNT {
        // SAFETY: `idx < OPT_COUNT` by this loop's own range.
        let opt_idx = OptIndex::from_index(idx).expect("0..OPT_COUNT is always a valid OptIndex");

        let wanted_scope = if bufopt { OptScope::Buf } else { OptScope::Win };
        if !option_has_scope(opt_idx, wanted_scope) {
            continue;
        }

        // SAFETY: forwarded from this function's own safety doc.
        let varp = unsafe { get_varp(opt_idx) };
        if varp.is_null() {
            continue;
        }

        // SAFETY: `varp` was just confirmed non-null above and
        // resolved for `opt_idx` itself, matching this function's own
        // established `optval_from_varp` calling convention.
        let opt_val = unsafe { optval_from_varp(opt_idx, varp) };
        let opt_tv = crate::eval::vars::optval_as_tv(opt_val, true);

        let opt = get_option(opt_idx);
        // SAFETY: `d` was just freshly allocated above, not yet
        // shared with anything else; forwarded from `tv_dict_add_tv`'s
        // own safety doc for `opt_tv` (a plain Number/String/Bool/Nil
        // value here, never a raw pointer needing extra care).
        unsafe { crate::eval::typval::tv_dict_add_tv(&mut *d, opt.fullname, &opt_tv) };
        // `opt_tv` is dropped here at the end of the loop body,
        // freeing its own owned String bytes (if any) via Rust's
        // normal `Drop` - `tv_dict_add_tv` only ever *copies* `tv` in
        // (see its own doc comment), it does not take ownership,
        // matching the original's own identical `tv_copy`-based
        // semantics for `tv_dict_add_tv`.
    }

    d
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

/// Resolves an option variable by index and requested scope
/// (`get_option_varp_scope_from`).
///
/// # Safety
/// Same as [`get_varp_scope_from`].
pub unsafe fn get_option_varp_scope_from(
    opt_idx: OptIndex,
    opt_flags: u32,
    buf: *mut BufT,
    win: *mut WinT,
) -> *mut c_void {
    unsafe { get_varp_scope_from(opt_idx, opt_flags, buf, win) }
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

/// Restore editor context after an option get/set operation
/// (`restore_option_context`).
///
/// # Safety
/// `ctx` must point to a live `CtxSwitch` for window/buffer scope, or
/// a saved live `TabpageT` pointer for tab scope. The current tabpage
/// and its window list must also be live for tab restoration.
pub unsafe fn restore_option_context(
    ctx: *mut std::ffi::c_void,
    scope: crate::option_defs::OptScope,
) {
    match scope {
        crate::option_defs::OptScope::Global => {}
        crate::option_defs::OptScope::Win | crate::option_defs::OptScope::Buf => {
            crate::context::ctx_restore(unsafe { &*(ctx as *const crate::context_defs::CtxSwitch) });
        }
        crate::option_defs::OptScope::Tab => {
            let saved = unsafe { *(ctx as *const *mut crate::buffer_defs::TabpageT) };
            let current = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
            unsafe {
                crate::window::unuse_tabpage(current);
                crate::window::use_tabpage(saved);
            }
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

/// Concatenate an original and new string option, inserting a comma
/// when the option's flags require one
/// (`stropt_concat_with_comma`).
#[allow(dead_code)]
fn stropt_concat_with_comma(
    origval: &[u8],
    newval: &mut Vec<u8>,
    op: crate::option_defs::SetOpT,
    flags: u32,
) {
    let comma = flags & crate::option_defs::opt_flags::COMMA != 0
        && !origval.is_empty()
        && !newval.is_empty();

    if op == crate::option_defs::SetOpT::Adding {
        let mut len = origval.len();
        if comma
            && len > 1
            && flags & crate::option_defs::opt_flags::ONE_COMMA
                == crate::option_defs::opt_flags::ONE_COMMA
            && origval[len - 1] == b','
            && origval[len - 2] != b'\\'
        {
            len -= 1;
        }
        let mut combined =
            Vec::with_capacity(len + usize::from(comma) + newval.len());
        combined.extend_from_slice(&origval[..len]);
        if comma {
            combined.push(b',');
        }
        combined.extend_from_slice(newval);
        *newval = combined;
    } else {
        let mut combined =
            Vec::with_capacity(newval.len() + usize::from(comma) + origval.len());
        combined.extend_from_slice(newval);
        if comma {
            combined.push(b',');
        }
        combined.extend_from_slice(origval);
        *newval = combined;
    }
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
        if let Some(&c) = option.get(p)
            && buf.len() < maxlen.saturating_sub(1)
        {
            buf.push(c);
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
    fn get_option_unset_value_string_global_local_is_always_empty() {
        assert_eq!(
            unsafe { get_option_unset_value(OptIndex::Equalprg) },
            OptVal::String(Vec::new())
        );
    }

    #[test]
    fn get_option_unset_value_number_global_local_is_minus_one() {
        assert_eq!(
            unsafe { get_option_unset_value(OptIndex::Sidescrolloff) },
            OptVal::Number(-1)
        );
    }

    #[test]
    fn get_option_unset_value_undolevels_uses_the_no_local_sentinel() {
        assert_eq!(
            unsafe { get_option_unset_value(OptIndex::Undolevels) },
            OptVal::Number(crate::option_vars::NO_LOCAL_UNDOLEVEL)
        );
    }

    #[test]
    fn get_option_unset_value_non_global_local_uses_the_real_global_value() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ts;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ts = 4;

        // 'tabstop' is a plain buffer-local option (not global-local),
        // so its "unset" value is just the real global value.
        assert_eq!(
            unsafe { get_option_unset_value(OptIndex::Tabstop) },
            OptVal::Number(4)
        );

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ts = prev;
    }

    #[test]
    fn is_option_local_value_unset_false_for_a_non_global_local_option() {
        assert!(!unsafe { is_option_local_value_unset(OptIndex::Tabstop) });
    }

    #[test]
    fn is_option_local_value_unset_true_when_local_matches_the_sentinel() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_siso = -1; // unset sentinel
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(unsafe { is_option_local_value_unset(OptIndex::Sidescrolloff) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn is_option_local_value_unset_false_when_local_is_explicitly_set() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_siso = 3; // explicitly set, not the sentinel
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(!unsafe { is_option_local_value_unset(OptIndex::Sidescrolloff) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
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
    fn option_get_type_returns_each_declared_value_kind() {
        assert_eq!(option_get_type(OptIndex::Aleph), OptValType::Number);
        assert_eq!(option_get_type(OptIndex::Ambiwidth), OptValType::String);
        assert_eq!(
            option_get_type(OptIndex::Allowrevins),
            OptValType::Boolean
        );
    }

    struct CmdheightGuard(crate::types_defs::OptInt);

    impl CmdheightGuard {
        fn save() -> Self {
            Self(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch)
        }
    }

    impl Drop for CmdheightGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = self.0;
        }
    }

    #[test]
    fn set_init_tablocal_restores_cmdheight_to_its_default() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CmdheightGuard::save();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = 99;

        unsafe { set_init_tablocal() };

        assert_eq!(
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch,
            1
        );
        assert_eq!(
            get_option(OptIndex::Cmdheight).def_val,
            crate::option_defs::OptVal::Number(1)
        );
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

    #[test]
    fn insecure_flag_local_wrap_goes_through_w_onebuf_opt() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_wrap_flags = crate::option_defs::opt_flags::INSECURE;

        let flagp = unsafe {
            insecure_flag(&mut win as *mut WinT, OptIndex::Wrap, crate::option_defs::opt_set_flags::OPT_LOCAL)
        };
        assert_eq!(unsafe { *flagp }, crate::option_defs::opt_flags::INSECURE);
    }

    #[test]
    fn insecure_flag_local_indentexpr_goes_through_w_buffer() {
        let mut buf = BufT { b_p_inde_flags: crate::option_defs::opt_flags::INSECURE, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };

        let flagp = unsafe {
            insecure_flag(&mut win as *mut WinT, OptIndex::Indentexpr, crate::option_defs::opt_set_flags::OPT_LOCAL)
        };
        assert_eq!(unsafe { *flagp }, crate::option_defs::opt_flags::INSECURE);
    }

    #[test]
    fn insecure_flag_global_wrap_goes_through_w_allbuf_opt() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_allbuf_opt.wo_wrap_flags = crate::option_defs::opt_flags::INSECURE;
        // Deliberately leave w_onebuf_opt's own copy clear, to prove
        // the OPT_GLOBAL branch reads w_allbuf_opt specifically, not
        // w_onebuf_opt.
        win.w_onebuf_opt.wo_wrap_flags = 0;

        let flagp = unsafe { insecure_flag(&mut win as *mut WinT, OptIndex::Wrap, 0) };
        assert_eq!(unsafe { *flagp }, crate::option_defs::opt_flags::INSECURE);
    }

    #[test]
    fn insecure_flag_falls_back_to_the_global_options_table() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };

        // 'tabstop' is a plain option with no window/buffer-local
        // insecure-flags field of its own, so this exercises the
        // "nothing special" fallback to the global OPTIONS[] table.
        let flagp = unsafe { insecure_flag(&mut win as *mut WinT, OptIndex::Tabstop, 0) };
        let prev = unsafe { *flagp };
        unsafe { *flagp |= crate::option_defs::opt_flags::INSECURE };
        assert_ne!(unsafe { *flagp } & crate::option_defs::opt_flags::INSECURE, 0);
        // Restore, since OPTIONS is genuinely global/shared state.
        unsafe { *flagp = prev };
    }

    #[test]
    fn was_set_insecurely_true_when_the_insecure_bit_is_set() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_wrap_flags = crate::option_defs::opt_flags::INSECURE;

        assert!(unsafe {
            was_set_insecurely(&mut win as *mut WinT, OptIndex::Wrap, crate::option_defs::opt_set_flags::OPT_LOCAL)
        });
    }

    #[test]
    fn was_set_insecurely_false_when_the_insecure_bit_is_clear() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_wrap_flags = 0;

        assert!(!unsafe {
            was_set_insecurely(&mut win as *mut WinT, OptIndex::Wrap, crate::option_defs::opt_set_flags::OPT_LOCAL)
        });
    }

    #[test]
    fn was_set_insecurely_ignores_unrelated_bits() {
        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        // Some other, unrelated flag bit set, but NOT the insecure one.
        win.w_onebuf_opt.wo_wrap_flags = crate::option_defs::opt_flags::REDR_WIN;

        assert!(!unsafe {
            was_set_insecurely(&mut win as *mut WinT, OptIndex::Wrap, crate::option_defs::opt_set_flags::OPT_LOCAL)
        });
    }

    // --- option_was_set / reset_option_was_set ---

    #[test]
    fn option_was_set_is_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!option_was_set(OptIndex::Wrap));
        assert!(!option_was_set(OptIndex::Tabstop));
    }

    #[test]
    fn reset_option_was_set_is_a_no_op_since_nothing_can_set_it_yet() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!option_was_set(OptIndex::Window));
        reset_option_was_set(OptIndex::Window);
        assert!(!option_was_set(OptIndex::Window));
    }

    #[test]
    fn option_was_set_tracks_each_option_independently() {
        let _lock = crate::globals::global_state_test_lock();
        // Directly manipulate the side-table (something no real,
        // translated caller can currently do, since nothing sets this
        // bit yet) to prove option_was_set reads the RIGHT index, not
        // just always returning a hardcoded false.
        {
            let table = unsafe { OPTION_WAS_SET.get_mut() };
            table[OptIndex::Tabstop as usize] = true;
        }

        assert!(option_was_set(OptIndex::Tabstop));
        assert!(!option_was_set(OptIndex::Wrap));

        reset_option_was_set(OptIndex::Tabstop);
        assert!(!option_was_set(OptIndex::Tabstop));
    }

    // --- check_illegal_path_names ---

    #[test]
    fn check_illegal_path_names_neither_flag_set_is_always_false() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!unsafe { check_illegal_path_names(b"a*b", 0) });
    }

    #[test]
    fn check_illegal_path_names_clean_value_is_false() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!unsafe {
            check_illegal_path_names(b"hello-world_123", crate::option_defs::opt_flags::NFNAME)
        });
    }

    #[test]
    fn check_illegal_path_names_nfname_wildcard_is_illegal() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(unsafe {
            check_illegal_path_names(b"foo*bar", crate::option_defs::opt_flags::NFNAME)
        });
    }

    #[test]
    fn check_illegal_path_names_nfname_non_secure_allows_pipe_and_semicolon() {
        // Outside secure mode, NFNAME's own charset omits '|'/';'/'&' -
        // only present when GLOBALS.secure is set (see the next test).
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        assert!(!unsafe {
            check_illegal_path_names(b"a|b;c&d", crate::option_defs::opt_flags::NFNAME)
        });
    }

    #[test]
    fn check_illegal_path_names_nfname_secure_mode_also_forbids_pipe_semicolon_ampersand() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.secure;
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 1;

        assert!(unsafe { check_illegal_path_names(b"a|b", crate::option_defs::opt_flags::NFNAME) });
        assert!(unsafe { check_illegal_path_names(b"a;b", crate::option_defs::opt_flags::NFNAME) });
        assert!(unsafe { check_illegal_path_names(b"a&b", crate::option_defs::opt_flags::NFNAME) });

        unsafe { crate::globals::GLOBALS.get_mut() }.secure = prev;
    }

    #[test]
    fn check_illegal_path_names_ndname_always_forbids_pipe_semicolon_ampersand() {
        // NDNAME's own charset includes '|'/';'/'&' unconditionally,
        // regardless of GLOBALS.secure (unlike NFNAME above).
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;
        assert!(unsafe { check_illegal_path_names(b"a|b", crate::option_defs::opt_flags::NDNAME) });
    }

    #[test]
    fn check_illegal_path_names_ndname_allows_backslash_and_forward_slash() {
        // NDNAME's own charset omits '/'/'\\' entirely (unlike
        // NFNAME) - a directory name option is expected to contain
        // real path separators.
        let _lock = crate::globals::global_state_test_lock();
        assert!(!unsafe {
            check_illegal_path_names(br"C:\some/dir", crate::option_defs::opt_flags::NDNAME)
        });
    }

    #[test]
    fn check_illegal_path_names_both_flags_set_matches_either_condition() {
        let _lock = crate::globals::global_state_test_lock();
        let flags = crate::option_defs::opt_flags::NFNAME | crate::option_defs::opt_flags::NDNAME;
        assert!(unsafe { check_illegal_path_names(b"a*b", flags) });
        assert!(!unsafe { check_illegal_path_names(b"clean", flags) });
    }

    // --- do_syntax_autocmd ---

    #[test]
    fn do_syntax_autocmd_sets_bf_syn_set_without_clobbering_other_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_flags: crate::buffer_defs::b_flags::BF_DUMMY as i32, ..Default::default() };
        unsafe { do_syntax_autocmd(&mut buf as *mut BufT, true) };

        assert_ne!(buf.b_flags & crate::buffer_defs::b_flags::BF_SYN_SET as i32, 0);
        assert_ne!(
            buf.b_flags & crate::buffer_defs::b_flags::BF_DUMMY as i32,
            0,
            "the pre-existing BF_DUMMY bit must survive the |="
        );
    }

    #[test]
    fn do_syntax_autocmd_leaves_syn_recursive_back_at_zero_after_a_normal_call() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { *SYN_RECURSIVE.get_mut() }, 0, "clean starting state");

        let mut buf = BufT::default();
        unsafe { do_syntax_autocmd(&mut buf as *mut BufT, false) };

        assert_eq!(unsafe { *SYN_RECURSIVE.get_mut() }, 0, "increment/decrement net to zero");
    }

    #[test]
    fn do_syntax_autocmd_works_with_no_name_or_fname_set() {
        // buf.b_p_syn/b_fname both None - do_syntax_autocmd must not
        // panic when passing them through as Option<&[u8]>::None.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_syn: None, b_fname: None, ..Default::default() };
        unsafe { do_syntax_autocmd(&mut buf as *mut BufT, true) };
        assert_ne!(buf.b_flags & crate::buffer_defs::b_flags::BF_SYN_SET as i32, 0);
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
    fn restore_option_context_global_and_noop_ctxswitch_are_noops() {
        unsafe {
            restore_option_context(std::ptr::null_mut(), crate::option_defs::OptScope::Global)
        };

        let switch = crate::context_defs::CtxSwitch::default();
        unsafe {
            restore_option_context(
                (&switch as *const crate::context_defs::CtxSwitch)
                    .cast_mut()
                    .cast(),
                crate::option_defs::OptScope::Buf,
            )
        };
    }

    #[test]
    fn restore_option_context_tab_reactivates_the_saved_tabpage() {
        struct ChGuard(crate::types_defs::OptInt);
        impl Drop for ChGuard {
            fn drop(&mut self) {
                unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = self.0;
            }
        }

        let _lock = crate::globals::global_state_test_lock();
        let mut current = crate::buffer_defs::TabpageT::default();
        let mut saved = crate::buffer_defs::TabpageT {
            tp_ch_used: 7,
            ..Default::default()
        };
        let currentp = &mut current as *mut crate::buffer_defs::TabpageT;
        let savedp = &mut saved as *mut crate::buffer_defs::TabpageT;
        let _curtab = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.curtab,
                currentp,
            )
        };
        let _topframe = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.topframe,
                std::ptr::null_mut(),
            )
        };
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.firstwin,
                std::ptr::null_mut(),
            )
        };
        let _lastwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.lastwin,
                std::ptr::null_mut(),
            )
        };
        let _curwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.curwin,
                std::ptr::null_mut(),
            )
        };
        let old_ch = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch;
        let _ch = ChGuard(old_ch);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = 3;
        let mut ctx = savedp;

        unsafe {
            restore_option_context(
                (&mut ctx as *mut *mut crate::buffer_defs::TabpageT).cast(),
                crate::option_defs::OptScope::Tab,
            )
        };

        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.curtab, savedp);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch, 7);
        assert_eq!(unsafe { (*currentp).tp_ch_used }, 3);
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
    fn get_option_varp_scope_from_matches_the_existing_resolver() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let flags = crate::option_defs::opt_set_flags::OPT_LOCAL;
        assert_eq!(
            unsafe {
                get_option_varp_scope_from(
                    OptIndex::Tabstop,
                    flags,
                    &mut buf,
                    &mut win,
                )
            },
            unsafe {
                get_varp_scope_from(
                    OptIndex::Tabstop,
                    flags,
                    &mut buf,
                    &mut win,
                )
            }
        );
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
    fn stropt_concat_with_comma_appends_and_prepends() {
        let mut appended = b"new".to_vec();
        stropt_concat_with_comma(
            b"old",
            &mut appended,
            crate::option_defs::SetOpT::Adding,
            crate::option_defs::opt_flags::COMMA,
        );
        assert_eq!(appended, b"old,new");

        let mut prepended = b"new".to_vec();
        stropt_concat_with_comma(
            b"old",
            &mut prepended,
            crate::option_defs::SetOpT::Prepending,
            crate::option_defs::opt_flags::COMMA,
        );
        assert_eq!(prepended, b"new,old");
    }

    #[test]
    fn stropt_concat_with_comma_only_inserts_a_comma_between_nonempty_values() {
        for (orig, new, expected) in [
            (&b""[..], &b"new"[..], &b"new"[..]),
            (&b"old"[..], &b""[..], &b"old"[..]),
            (&b""[..], &b""[..], &b""[..]),
        ] {
            let mut value = new.to_vec();
            stropt_concat_with_comma(
                orig,
                &mut value,
                crate::option_defs::SetOpT::Adding,
                crate::option_defs::opt_flags::COMMA,
            );
            assert_eq!(value, expected);
        }
    }

    #[test]
    fn stropt_concat_with_comma_strips_one_unescaped_trailing_comma() {
        let mut value = b"new".to_vec();
        stropt_concat_with_comma(
            b"old,",
            &mut value,
            crate::option_defs::SetOpT::Adding,
            crate::option_defs::opt_flags::ONE_COMMA,
        );
        assert_eq!(value, b"old,new");

        let mut escaped = b"new".to_vec();
        stropt_concat_with_comma(
            b"old\\,",
            &mut escaped,
            crate::option_defs::SetOpT::Adding,
            crate::option_defs::opt_flags::ONE_COMMA,
        );
        assert_eq!(escaped, b"old\\,,new");
    }

    #[test]
    fn stropt_concat_with_comma_without_comma_flag_concatenates_directly() {
        let mut value = b"new".to_vec();
        stropt_concat_with_comma(
            b"old",
            &mut value,
            crate::option_defs::SetOpT::Adding,
            0,
        );
        assert_eq!(value, b"oldnew");
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

#[cfg(test)]
mod get_winbuf_options_tests {
    use super::*;
    use crate::eval::typval::{tv_dict_find, tv_dict_free};
    use crate::eval::typval_defs::TypvalValue;

    /// Points `GLOBALS.curbuf`/`curwin` at real, linked `buf`/`win`
    /// instances for the guard's lifetime, restoring the previous
    /// values on drop. Callers must hold `global_state_test_lock()`
    /// for the guard's whole lifetime (matching this file's own
    /// `CurbufGuard` precedent, widened to cover `curwin` too since
    /// `get_winbuf_options` resolves both buffer-local and
    /// window-local storage via `get_varp`, which reads through both).
    struct CurBufWinGuard {
        prev_buf: *mut BufT,
        prev_win: *mut WinT,
    }

    impl CurBufWinGuard {
        fn set(buf: *mut BufT, win: *mut WinT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_buf = globals.curbuf;
            let prev_win = globals.curwin;
            globals.curbuf = buf;
            globals.curwin = win;
            CurBufWinGuard { prev_buf, prev_win }
        }
    }

    impl Drop for CurBufWinGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.curbuf = self.prev_buf;
            globals.curwin = self.prev_win;
        }
    }

    /// Reads back a `Number` value stored under `key` in `d`, panicking
    /// if the key is absent or not a `Number` - keeps the tests below
    /// focused on the one property they're actually checking.
    fn dict_number(d: &mut crate::eval::typval_defs::DictT, key: &[u8]) -> crate::eval::typval_defs::VarnumberT {
        let item = tv_dict_find(Some(d), key).unwrap_or_else(|| {
            panic!("expected key {:?} to be present", String::from_utf8_lossy(key))
        });
        match unsafe { &(*item).di_tv }.value {
            TypvalValue::Number(n) => n,
            ref other => panic!("expected {:?} to be a Number, got {:?}", String::from_utf8_lossy(key), other),
        }
    }

    #[test]
    fn get_winbuf_options_bufopt_true_includes_tabstop_with_the_real_value() {
        let _lock = crate::globals::global_state_test_lock();

        let mut buf = BufT { b_p_ts: 12, ..Default::default() };
        let mut syn = crate::buffer_defs::SynblockT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let syn_ptr = &mut syn as *mut crate::buffer_defs::SynblockT;
        let mut win = WinT { w_buffer: buf_ptr, w_s: syn_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurBufWinGuard::set(buf_ptr, win_ptr);

        // SAFETY: curbuf/curwin were just set to valid, linked
        // instances above.
        let d = unsafe { get_winbuf_options(true) };
        assert_eq!(dict_number(unsafe { &mut *d }, b"tabstop"), 12);
        unsafe { tv_dict_free(d) };
    }

    #[test]
    fn get_winbuf_options_bufopt_true_excludes_window_local_wrap() {
        let _lock = crate::globals::global_state_test_lock();

        let mut buf = BufT::default();
        let mut syn = crate::buffer_defs::SynblockT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let syn_ptr = &mut syn as *mut crate::buffer_defs::SynblockT;
        let mut win = WinT { w_buffer: buf_ptr, w_s: syn_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurBufWinGuard::set(buf_ptr, win_ptr);

        let d = unsafe { get_winbuf_options(true) };
        assert!(tv_dict_find(Some(unsafe { &mut *d }), b"wrap").is_none());
        unsafe { tv_dict_free(d) };
    }

    #[test]
    fn get_winbuf_options_bufopt_false_includes_wrap_with_the_real_value() {
        let _lock = crate::globals::global_state_test_lock();

        let mut buf = BufT::default();
        let mut syn = crate::buffer_defs::SynblockT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let syn_ptr = &mut syn as *mut crate::buffer_defs::SynblockT;
        let mut win = WinT {
            w_buffer: buf_ptr,
            w_s: syn_ptr,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_wrap: 1, ..Default::default() },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurBufWinGuard::set(buf_ptr, win_ptr);

        let d = unsafe { get_winbuf_options(false) };
        assert_eq!(dict_number(unsafe { &mut *d }, b"wrap"), 1);
        unsafe { tv_dict_free(d) };
    }

    #[test]
    fn get_winbuf_options_bufopt_false_excludes_buffer_local_tabstop() {
        let _lock = crate::globals::global_state_test_lock();

        let mut buf = BufT { b_p_ts: 8, ..Default::default() };
        let mut syn = crate::buffer_defs::SynblockT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let syn_ptr = &mut syn as *mut crate::buffer_defs::SynblockT;
        let mut win = WinT { w_buffer: buf_ptr, w_s: syn_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurBufWinGuard::set(buf_ptr, win_ptr);

        let d = unsafe { get_winbuf_options(false) };
        assert!(tv_dict_find(Some(unsafe { &mut *d }), b"tabstop").is_none());
        unsafe { tv_dict_free(d) };
    }

    #[test]
    fn get_winbuf_options_excludes_global_only_options_either_way() {
        let _lock = crate::globals::global_state_test_lock();

        let mut buf = BufT::default();
        let mut syn = crate::buffer_defs::SynblockT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let syn_ptr = &mut syn as *mut crate::buffer_defs::SynblockT;
        let mut win = WinT { w_buffer: buf_ptr, w_s: syn_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurBufWinGuard::set(buf_ptr, win_ptr);

        // 'ignorecase' is Global-only (verified against option_defs.rs's
        // own generated table) - must appear in neither dict.
        let bufdict = unsafe { get_winbuf_options(true) };
        assert!(tv_dict_find(Some(unsafe { &mut *bufdict }), b"ignorecase").is_none());
        unsafe { tv_dict_free(bufdict) };

        let windict = unsafe { get_winbuf_options(false) };
        assert!(tv_dict_find(Some(unsafe { &mut *windict }), b"ignorecase").is_none());
        unsafe { tv_dict_free(windict) };
    }
}

#[cfg(test)]
mod set_option_sctx_tests {
    use super::*;

    /// Points `GLOBALS.curbuf`/`curwin` at real, linked `buf`/`win`
    /// instances for the guard's lifetime, restoring the previous
    /// values on drop. Callers must hold `global_state_test_lock()`
    /// for the guard's whole lifetime (matching
    /// `get_winbuf_options_tests::CurBufWinGuard`'s own identical
    /// precedent, duplicated here rather than shared since that one is
    /// private to its own module).
    struct CurBufWinGuard {
        prev_buf: *mut BufT,
        prev_win: *mut WinT,
    }

    impl CurBufWinGuard {
        fn set(buf: *mut BufT, win: *mut WinT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_buf = globals.curbuf;
            let prev_win = globals.curwin;
            globals.curbuf = buf;
            globals.curwin = win;
            CurBufWinGuard { prev_buf, prev_win }
        }
    }

    impl Drop for CurBufWinGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.curbuf = self.prev_buf;
            globals.curwin = self.prev_win;
        }
    }

    /// Resets `OPTION_SCRIPT_CTX`'s slot for `opt_idx` back to a fresh
    /// zeroed `SctxT`, so tests don't leak state into one another via
    /// this shared side-table (matching `OPTION_WAS_SET`'s own tests'
    /// established need to reset shared per-option side-table state).
    fn reset_script_ctx(opt_idx: OptIndex) {
        let table = unsafe { OPTION_SCRIPT_CTX.get_mut() };
        table[opt_idx as usize] = SctxT::default();
    }

    struct OptionScriptCtxGuard {
        idx: OptIndex,
        saved: SctxT,
    }

    impl OptionScriptCtxGuard {
        fn install(idx: OptIndex, value: SctxT) -> Self {
            let table = unsafe { OPTION_SCRIPT_CTX.get_mut() };
            let saved = table[idx as usize];
            table[idx as usize] = value;
            Self { idx, saved }
        }
    }

    impl Drop for OptionScriptCtxGuard {
        fn drop(&mut self) {
            (unsafe { OPTION_SCRIPT_CTX.get_mut() })[self.idx as usize] = self.saved;
        }
    }

    #[test]
    fn option_scope_idx_matches_the_known_buf_opt_index() {
        // 'tabstop' is BufOptIndex::Tabstop - verified against
        // option_defs.rs's own BUF_OPT_IDX table.
        assert_eq!(
            option_scope_idx(OptIndex::Tabstop, OptScope::Buf),
            crate::option_defs::BufOptIndex::Tabstop as isize
        );
        // A Global-only option has no Buf-scope entry at all (-1,
        // matching options_enum.generated.h's own "unused scope"
        // sentinel).
        assert_eq!(option_scope_idx(OptIndex::Ignorecase, OptScope::Buf), -1);
    }

    #[test]
    fn option_script_ctx_defaults_to_a_zeroed_sctx() {
        let _lock = crate::globals::global_state_test_lock();
        reset_script_ctx(OptIndex::Ignorecase);
        assert_eq!(option_script_ctx(OptIndex::Ignorecase), SctxT::default());
    }

    #[test]
    fn didset_options_sctx_stops_at_invalid_and_uses_current_context() {
        let _lock = crate::globals::global_state_test_lock();
        let _ignore = OptionScriptCtxGuard::install(
            OptIndex::Ignorecase,
            crate::eval::typval_defs::SctxT {
                sc_sid: 11,
                ..Default::default()
            },
        );
        let _magic = OptionScriptCtxGuard::install(
            OptIndex::Magic,
            crate::eval::typval_defs::SctxT {
                sc_sid: 22,
                ..Default::default()
            },
        );
        let _history = OptionScriptCtxGuard::install(
            OptIndex::History,
            crate::eval::typval_defs::SctxT {
                sc_sid: 33,
                ..Default::default()
            },
        );
        let current = crate::eval::typval_defs::SctxT {
            sc_sid: 77,
            sc_lnum: 5,
            ..Default::default()
        };
        let _current = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.current_sctx, current)
        };

        unsafe {
            didset_options_sctx(
                crate::option_defs::opt_set_flags::OPT_GLOBAL,
                &[
                    OptIndex::Ignorecase,
                    OptIndex::Magic,
                    OptIndex::Invalid,
                    OptIndex::History,
                ],
            )
        };

        assert_eq!(option_script_ctx(OptIndex::Ignorecase).sc_sid, 77);
        assert_eq!(option_script_ctx(OptIndex::Magic).sc_sid, 77);
        assert_eq!(option_script_ctx(OptIndex::History).sc_sid, 33);
    }

    #[test]
    fn get_option_sctx_reads_the_global_context_side_table() {
        let _lock = crate::globals::global_state_test_lock();
        let ctx = SctxT {
            sc_sid: 17,
            sc_seq: 3,
            sc_lnum: 29,
            sc_chan: 5,
        };
        let _g = OptionScriptCtxGuard::install(OptIndex::Ignorecase, ctx);

        assert_eq!(get_option_sctx(OptIndex::Ignorecase), ctx);
        assert_eq!(option_script_ctx(OptIndex::Ignorecase), ctx);
    }

    #[test]
    fn set_option_sctx_records_a_global_only_option() {
        let _lock = crate::globals::global_state_test_lock();
        reset_script_ctx(OptIndex::Ignorecase);

        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let _guard = CurBufWinGuard::set(&mut buf as *mut BufT, &mut win as *mut WinT);

        let ctx = SctxT { sc_sid: 7, sc_seq: 1, sc_lnum: 10, sc_chan: 0 };
        unsafe { set_option_sctx(OptIndex::Ignorecase, crate::option_defs::opt_set_flags::OPT_MODELINE, ctx) };

        // OPT_MODELINE means sourcing_lnum() is NOT added (the modeline
        // already carries its own real line number) - sc_lnum stays
        // exactly as passed in.
        assert_eq!(option_script_ctx(OptIndex::Ignorecase), ctx);
    }

    #[test]
    fn set_option_sctx_adds_sourcing_lnum_unless_modeline() {
        let _lock = crate::globals::global_state_test_lock();
        reset_script_ctx(OptIndex::Ignorecase);

        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let _guard = CurBufWinGuard::set(&mut buf as *mut BufT, &mut win as *mut WinT);

        let ctx = SctxT { sc_sid: 3, sc_seq: 0, sc_lnum: 5, sc_chan: 0 };
        // Not OPT_MODELINE, so sourcing_lnum() (0, EXESTACK is always
        // empty) is added on top of the passed-in sc_lnum - a real,
        // if currently-inert (0 + 5 == 5), addition.
        unsafe { set_option_sctx(OptIndex::Ignorecase, 0, ctx) };
        assert_eq!(option_script_ctx(OptIndex::Ignorecase).sc_lnum, 5 + crate::runtime::sourcing_lnum());
    }

    #[test]
    fn set_option_sctx_records_a_buffer_local_option() {
        let _lock = crate::globals::global_state_test_lock();

        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let _guard = CurBufWinGuard::set(&mut buf as *mut BufT, &mut win as *mut WinT);

        assert!(buf.b_p_script_ctx.is_empty());
        let ctx = SctxT { sc_sid: 2, sc_seq: 0, sc_lnum: 0, sc_chan: 0 };
        unsafe { set_option_sctx(OptIndex::Tabstop, crate::option_defs::opt_set_flags::OPT_MODELINE, ctx) };

        let idx = option_scope_idx(OptIndex::Tabstop, OptScope::Buf) as usize;
        assert_eq!(buf.b_p_script_ctx[idx], ctx);
    }

    #[test]
    fn set_option_sctx_records_a_window_local_option_and_all_buffers_value_when_both() {
        let _lock = crate::globals::global_state_test_lock();

        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let _guard = CurBufWinGuard::set(&mut buf as *mut BufT, &mut win as *mut WinT);

        // 'arabic' is pure window-local (option_is_window_local_true_
        // only_for_pure_window_scope's own precedent).
        let ctx = SctxT { sc_sid: 9, sc_seq: 0, sc_lnum: 1, sc_chan: 0 };
        // opt_flags == 0 means "both" (neither OPT_LOCAL nor
        // OPT_GLOBAL): OPT_MODELINE also set so sourcing_lnum() isn't
        // added, keeping the assertion exact.
        unsafe {
            set_option_sctx(OptIndex::Arabic, crate::option_defs::opt_set_flags::OPT_MODELINE, ctx)
        };

        let idx = option_scope_idx(OptIndex::Arabic, OptScope::Win) as usize;
        assert_eq!(win.w_onebuf_opt.wo_script_ctx[idx], ctx);
        assert_eq!(win.w_allbuf_opt.wo_script_ctx[idx], ctx);
    }

    #[test]
    fn set_option_sctx_skips_all_buffers_value_when_opt_local_is_explicit() {
        let _lock = crate::globals::global_state_test_lock();

        let mut buf = BufT::default();
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let _guard = CurBufWinGuard::set(&mut buf as *mut BufT, &mut win as *mut WinT);

        let ctx = SctxT { sc_sid: 4, sc_seq: 0, sc_lnum: 2, sc_chan: 0 };
        let flags = crate::option_defs::opt_set_flags::OPT_LOCAL | crate::option_defs::opt_set_flags::OPT_MODELINE;
        unsafe { set_option_sctx(OptIndex::Arabic, flags, ctx) };

        let idx = option_scope_idx(OptIndex::Arabic, OptScope::Win) as usize;
        assert_eq!(win.w_onebuf_opt.wo_script_ctx[idx], ctx);
        // w_allbuf_opt's own vec was never even grown, since only the
        // single-buffer value is recorded for an explicit OPT_LOCAL
        // call (matching the original's own `if (both)` guard around
        // its "also setting the all buffers value" branch).
        assert!(win.w_allbuf_opt.wo_script_ctx.is_empty());
    }
}

#[cfg(test)]
mod did_set_option_tests {
    use super::*;

    /// Points `GLOBALS.curbuf`/`curwin` at real, linked `buf`/`win`
    /// instances for the guard's lifetime, restoring the previous
    /// values on drop. Callers must hold `global_state_test_lock()`
    /// for the guard's whole lifetime (matching
    /// `set_option_sctx_tests::CurBufWinGuard`'s own identical
    /// precedent, duplicated here rather than shared since that one is
    /// private to its own module).
    struct CurBufWinGuard {
        prev_buf: *mut BufT,
        prev_win: *mut WinT,
    }

    impl CurBufWinGuard {
        fn set(buf: *mut BufT, win: *mut WinT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_buf = globals.curbuf;
            let prev_win = globals.curwin;
            globals.curbuf = buf;
            globals.curwin = win;
            CurBufWinGuard { prev_buf, prev_win }
        }
    }

    impl Drop for CurBufWinGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.curbuf = self.prev_buf;
            globals.curwin = self.prev_win;
        }
    }

    /// Resets every piece of shared global state a `did_set_option`
    /// test could observe leaking from a sibling test: `GLOBALS.secure`/
    /// `sandbox`, `OPTION_VARS.p_mouse`/`p_flp`/`p_wbr`, and (for the
    /// specific `opt_idx`es this module's own tests exercise)
    /// `OPTION_WAS_SET`/`OPTION_SCRIPT_CTX`.
    fn reset_shared_state() {
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.secure = 0;
        g.sandbox = 0;
        let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        ov.p_mouse = None;
        ov.p_flp = None;
        ov.p_wbr = None;
        for &idx in &[OptIndex::Aleph, OptIndex::Cdhome, OptIndex::Backupext, OptIndex::Allowrevins, OptIndex::Arabic]
        {
            reset_option_was_set(idx);
            let table = unsafe { OPTION_SCRIPT_CTX.get_mut() };
            table[idx as usize] = SctxT::default();
        }
    }

    /// Sets up a real, linked `buf`/`win` pair (with `win.w_s` pointing
    /// at a real `SynblockT`) and installs them as `curbuf`/`curwin`
    /// via [`CurBufWinGuard`]. Returns the guard, plus raw pointers to
    /// the (heap-boxed, deliberately leaked for the test's duration)
    /// `buf`/`win`/`syn` - leaked rather than stack-allocated so the
    /// returned pointers stay valid for the caller's own use after
    /// this function returns (matching this crate's own established
    /// "small, deliberate test-only leak" precedent elsewhere for the
    /// identical stack-lifetime problem).
    fn setup_curbuf_curwin() -> (CurBufWinGuard, *mut BufT, *mut WinT) {
        let buf = Box::into_raw(Box::new(BufT::default()));
        let syn = Box::into_raw(Box::new(crate::buffer_defs::SynblockT::default()));
        let win = Box::into_raw(Box::new(WinT { w_buffer: buf, w_s: syn, ..Default::default() }));
        let guard = CurBufWinGuard::set(buf, win);
        (guard, buf, win)
    }

    #[test]
    fn immutable_option_with_a_changed_value_is_rejected_and_restores_old_value() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let mut dummy: OptInt = 999; // simulates varp already holding new_value
        let varp = &mut dummy as *mut OptInt as *mut c_void;

        let result = unsafe {
            did_set_option(
                OptIndex::Aleph,
                varp,
                OptVal::Number(224),
                OptVal::Number(999),
                0,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        assert_eq!(result, Some(crate::errors::e_unsupportedoption));
        // The previous value was restored through varp.
        assert_eq!(dummy, 224);
    }

    #[test]
    fn immutable_option_with_an_unchanged_value_is_not_rejected() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, _buf, _win) = setup_curbuf_curwin();

        let mut dummy: OptInt = 224;
        let varp = &mut dummy as *mut OptInt as *mut c_void;

        let result = unsafe {
            did_set_option(
                OptIndex::Aleph,
                varp,
                OptVal::Number(224),
                OptVal::Number(224),
                // OPT_GLOBAL (not 0/scope_both): 'aleph' is a hidden
                // option (immutable + null `var`), so scope_both's own
                // "set the global/local value" step - which would
                // resolve through the SAME null `var` via
                // get_varp_scope - must be skipped here; this test
                // only cares about the immutable-check bypass itself.
                crate::option_defs::opt_set_flags::OPT_GLOBAL,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        // Same value -> immutable check doesn't fire -> proceeds to
        // the real, always-reachable tail (no callback, no other
        // special-cased varp match for 'aleph') -> succeeds.
        assert_eq!(result, None);
    }

    #[test]
    fn secure_mode_rejects_a_secure_flagged_option() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 1;

        let mut dummy: i32 = 0;
        let varp = &mut dummy as *mut i32 as *mut c_void;

        let result = unsafe {
            did_set_option(
                OptIndex::Cdhome,
                varp,
                OptVal::Boolean(TriState::False),
                OptVal::Boolean(TriState::True),
                0,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        assert_eq!(result, Some(crate::errors::e_secure));
        // Old value restored.
        assert_eq!(dummy, TriState::False as i32);
        reset_shared_state();
    }

    #[test]
    fn illegal_path_name_in_a_string_option_is_rejected() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let mut dummy: Option<Vec<u8>> = Some(b"old".to_vec());
        let varp = &mut dummy as *mut Option<Vec<u8>> as *mut c_void;

        let result = unsafe {
            did_set_option(
                OptIndex::Backupext,
                varp,
                OptVal::String(b"old".to_vec()),
                OptVal::String(b"foo*bar".to_vec()),
                0,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        assert_eq!(result, Some(crate::errors::e_invarg));
        assert_eq!(dummy, Some(b"old".to_vec()));
    }

    #[test]
    fn direct_mode_skips_the_error_checks_and_the_autocmd_redraw_tail() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, _buf, _win) = setup_curbuf_curwin();

        // Even an otherwise-immutable option's "changed value" check
        // is skipped entirely when direct == true.
        let mut dummy: OptInt = 999;
        let varp = &mut dummy as *mut OptInt as *mut c_void;

        let result = unsafe {
            did_set_option(
                OptIndex::Aleph,
                varp,
                OptVal::Number(224),
                OptVal::Number(999),
                // OPT_GLOBAL: see immutable_option_with_an_unchanged_
                // value_is_not_rejected's own identical comment -
                // scope_both's write step still runs even when direct
                // == true, and 'aleph' is a hidden (null `var`) option.
                crate::option_defs::opt_set_flags::OPT_GLOBAL,
                crate::globals::SID_NONE,
                true,
                false,
            )
        };

        assert_eq!(result, None);
        // The tail (set_option_was_set) is never reached in direct
        // mode - it returns before that point.
        assert!(!option_was_set(OptIndex::Aleph));
    }

    #[test]
    fn normal_success_path_records_was_set_and_script_ctx() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, _buf, _win) = setup_curbuf_curwin();

        let mut dummy: i32 = 1;
        let varp = &mut dummy as *mut i32 as *mut c_void;
        assert!(!option_was_set(OptIndex::Allowrevins));

        let result = unsafe {
            did_set_option(
                OptIndex::Allowrevins,
                varp,
                OptVal::Boolean(TriState::False),
                OptVal::Boolean(TriState::True),
                0,
                7, // set_sid: a concrete script ID, not 0/SID_NONE
                false,
                false,
            )
        };

        assert_eq!(result, None);
        assert!(option_was_set(OptIndex::Allowrevins));
        assert_eq!(option_script_ctx(OptIndex::Allowrevins).sc_sid, 7);
        reset_shared_state();
    }

    #[test]
    fn set_sid_zero_uses_current_sctx() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, _buf, _win) = setup_curbuf_curwin();

        let prev_sctx = unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx;
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx = SctxT { sc_sid: 42, ..Default::default() };

        let mut dummy: i32 = 1;
        let varp = &mut dummy as *mut i32 as *mut c_void;
        let _ = unsafe {
            did_set_option(
                OptIndex::Allowrevins,
                varp,
                OptVal::Boolean(TriState::False),
                OptVal::Boolean(TriState::True),
                0,
                0, // set_sid == 0: use current_sctx
                false,
                false,
            )
        };

        assert_eq!(option_script_ctx(OptIndex::Allowrevins).sc_sid, 42);

        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx = prev_sctx;
        reset_shared_state();
    }

    #[test]
    fn set_sid_none_never_records_a_script_ctx() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, _buf, _win) = setup_curbuf_curwin();

        let mut dummy: i32 = 1;
        let varp = &mut dummy as *mut i32 as *mut c_void;
        let _ = unsafe {
            did_set_option(
                OptIndex::Allowrevins,
                varp,
                OptVal::Boolean(TriState::False),
                OptVal::Boolean(TriState::True),
                0,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        assert_eq!(option_script_ctx(OptIndex::Allowrevins), SctxT::default());
        reset_shared_state();
    }

    #[test]
    fn scope_both_global_local_option_resets_local_value_to_unset() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, buf, _win) = setup_curbuf_curwin();
        unsafe { (*buf).b_p_ep = Some(b"local".to_vec()) };

        // 'equalprg' is global-local (Buf + Global scope) - verified
        // against option_is_global_local's own established tests.
        let mut dummy: Option<Vec<u8>> = Some(b"new-global".to_vec());
        let varp = &mut dummy as *mut Option<Vec<u8>> as *mut c_void;

        let result = unsafe {
            did_set_option(
                OptIndex::Equalprg,
                varp,
                OptVal::String(b"old-global".to_vec()),
                OptVal::String(b"new-global".to_vec()),
                0, // scope_both: neither OPT_LOCAL nor OPT_GLOBAL
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        assert_eq!(result, None);
        // The buffer-local value was reset to the "unset" sentinel
        // (empty string, for a String-typed global-local option).
        assert_eq!(unsafe { &*buf }.b_p_ep, Some(Vec::new()));
    }

    #[test]
    fn scope_both_non_global_local_option_sets_the_global_value() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, _buf, _win) = setup_curbuf_curwin();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ari;

        let mut dummy: i32 = 1;
        let varp = &mut dummy as *mut i32 as *mut c_void;
        let _ = unsafe {
            did_set_option(
                OptIndex::Allowrevins,
                varp,
                OptVal::Boolean(TriState::False),
                OptVal::Boolean(TriState::True),
                0, // scope_both
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        // 'allowrevins' is global-only, so the "set the global value"
        // branch runs, writing through get_varp_scope(OPT_GLOBAL) -
        // which for a global-only option resolves to the same
        // OPTION_VARS storage varp itself already points at.
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ari, TriState::True as i32);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ari = prev;
        reset_shared_state();
    }

    #[test]
    fn syntax_option_triggers_do_syntax_autocmd() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, buf, _win) = setup_curbuf_curwin();

        // SAFETY: matches did_set_option's own internal resolution.
        let varp = unsafe { std::ptr::addr_of_mut!((*buf).b_p_syn) as *mut c_void };
        assert_eq!(unsafe { &*buf }.b_flags & crate::buffer_defs::b_flags::BF_SYN_SET as i32, 0);

        let result = unsafe {
            did_set_option(
                OptIndex::Syntax,
                varp,
                OptVal::String(b"".to_vec()),
                OptVal::String(b"rust".to_vec()),
                0,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        assert_eq!(result, None);
        assert_ne!(unsafe { &*buf }.b_flags & crate::buffer_defs::b_flags::BF_SYN_SET as i32, 0);
    }

    #[test]
    fn filetype_option_triggers_do_filetype_autocmd_when_not_from_a_modeline() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, buf, _win) = setup_curbuf_curwin();

        let varp = unsafe { std::ptr::addr_of_mut!((*buf).b_p_ft) as *mut c_void };
        assert!(!unsafe { &*buf }.b_did_filetype);

        let result = unsafe {
            did_set_option(
                OptIndex::Filetype,
                varp,
                OptVal::String(b"".to_vec()),
                OptVal::String(b"rust".to_vec()),
                0, // no OPT_MODELINE
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        assert_eq!(result, None);
        assert!(unsafe { &*buf }.b_did_filetype);
    }

    #[test]
    fn filetype_option_from_a_modeline_skips_do_filetype_autocmd() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, buf, _win) = setup_curbuf_curwin();

        let varp = unsafe { std::ptr::addr_of_mut!((*buf).b_p_ft) as *mut c_void };

        let result = unsafe {
            did_set_option(
                OptIndex::Filetype,
                varp,
                OptVal::String(b"".to_vec()),
                OptVal::String(b"rust".to_vec()),
                crate::option_defs::opt_set_flags::OPT_MODELINE,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        assert_eq!(result, None);
        // value_changed is always false in this scope (no real
        // callback ever runs), so "!(opt_flags & OPT_MODELINE) ||
        // value_changed" is false || false == false: the FileType
        // autocmd trigger is skipped, and b_did_filetype stays false.
        assert!(!unsafe { &*buf }.b_did_filetype);
    }

    #[test]
    #[should_panic(expected = "spelllang")]
    fn spelllang_option_is_unimplemented() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, _buf, win) = setup_curbuf_curwin();

        let varp = unsafe { std::ptr::addr_of_mut!((*(*win).w_s).b_p_spl) as *mut c_void };

        let _ = unsafe {
            did_set_option(
                OptIndex::Spelllang,
                varp,
                OptVal::String(b"".to_vec()),
                OptVal::String(b"en".to_vec()),
                0,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };
    }

    #[test]
    #[should_panic(expected = "winbar")]
    fn winbar_option_is_unimplemented() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, _buf, _win) = setup_curbuf_curwin();

        let varp =
            unsafe { std::ptr::addr_of_mut!(crate::option_vars::OPTION_VARS.get_mut().p_wbr) as *mut c_void };

        let _ = unsafe {
            did_set_option(
                OptIndex::Winbar,
                varp,
                OptVal::String(b"".to_vec()),
                OptVal::String(b"%f".to_vec()),
                0,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };
    }

    #[test]
    fn curswant_flagged_option_sets_w_set_curswant() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, _buf, win) = setup_curbuf_curwin();
        unsafe {
            (*win).w_curswant = 5; // != MAXCOL
            (*win).w_set_curswant = false;
        }

        // 'arabic' has opt_flags::CURSWANT and no HL_ONLY.
        let mut dummy: i32 = 0;
        let varp = &mut dummy as *mut i32 as *mut c_void;

        let _ = unsafe {
            did_set_option(
                OptIndex::Arabic,
                varp,
                OptVal::Boolean(TriState::False),
                OptVal::Boolean(TriState::True),
                0,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        assert!(unsafe { &*win }.w_set_curswant);
    }

    #[test]
    fn insecure_flag_is_set_when_option_set_from_a_modeline() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, _buf, win) = setup_curbuf_curwin();

        let mut dummy: i32 = 1;
        let varp = &mut dummy as *mut i32 as *mut c_void;

        let _ = unsafe {
            did_set_option(
                OptIndex::Allowrevins,
                varp,
                OptVal::Boolean(TriState::False),
                OptVal::Boolean(TriState::True),
                crate::option_defs::opt_set_flags::OPT_MODELINE,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };

        assert!(unsafe { was_set_insecurely(win, OptIndex::Allowrevins, 0) });
        reset_shared_state();
    }

    #[test]
    fn insecure_flag_is_cleared_when_value_replaced_outside_secure_context() {
        let _lock = crate::globals::global_state_test_lock();
        reset_shared_state();
        let (_guard, _buf, win) = setup_curbuf_curwin();

        // First, mark it insecure (as the previous test's own scenario
        // would), then set again with value_replaced == true and no
        // secure/sandbox/modeline context - the flag must clear.
        let _ = unsafe {
            did_set_option(
                OptIndex::Allowrevins,
                &mut (1_i32) as *mut i32 as *mut c_void,
                OptVal::Boolean(TriState::False),
                OptVal::Boolean(TriState::True),
                crate::option_defs::opt_set_flags::OPT_MODELINE,
                crate::globals::SID_NONE,
                false,
                false,
            )
        };
        assert!(unsafe { was_set_insecurely(win, OptIndex::Allowrevins, 0) });

        let mut dummy: i32 = 1;
        let varp = &mut dummy as *mut i32 as *mut c_void;
        let _ = unsafe {
            did_set_option(
                OptIndex::Allowrevins,
                varp,
                OptVal::Boolean(TriState::True),
                OptVal::Boolean(TriState::False),
                0,
                crate::globals::SID_NONE,
                false,
                true, // value_replaced
            )
        };

        assert!(!unsafe { was_set_insecurely(win, OptIndex::Allowrevins, 0) });
        reset_shared_state();
    }
}

/// Whether two option values are equal (`optval_equal`).
///
/// The original hand-writes a per-variant comparison because its
/// `OptVal` is an untagged union plus a separate `type` field, so `==`
/// on it would be meaningless. This crate models `OptVal` as a real
/// Rust enum, whose derived `PartialEq` already performs exactly the
/// original's own dispatch: different variants are unequal, `Nil` is
/// equal to `Nil`, and each payload compares by value.
///
/// Kept as a named function anyway, so callers translated later read
/// the same way the original does.
#[must_use]
pub fn optval_equal(o1: &crate::option_defs::OptVal, o2: &crate::option_defs::OptVal) -> bool {
    o1 == o2
}

/// Convert an option value to an API object (`optval_as_object`).
#[must_use]
pub fn optval_as_object(
    value: crate::option_defs::OptVal,
) -> crate::api::private::defs::Object {
    use crate::api::private::defs::Object;
    use crate::option_defs::OptVal;
    match value {
        OptVal::Nil | OptVal::Boolean(crate::types_defs::TriState::None) => {
            Object::Nil
        }
        OptVal::Boolean(crate::types_defs::TriState::False) => {
            Object::Boolean(false)
        }
        OptVal::Boolean(crate::types_defs::TriState::True) => {
            Object::Boolean(true)
        }
        OptVal::Number(number) => Object::Integer(number),
        OptVal::String(string) => Object::String(string),
    }
}

/// Convert an API object to an option value (`object_as_optval`).
///
/// The boolean reports the original out-parameter `error`.
#[must_use]
pub fn object_as_optval(
    object: crate::api::private::defs::Object,
) -> (crate::option_defs::OptVal, bool) {
    use crate::api::private::defs::Object;
    use crate::option_defs::OptVal;
    match object {
        Object::Nil => (OptVal::Nil, false),
        Object::Boolean(value) => (
            OptVal::Boolean(if value {
                crate::types_defs::TriState::True
            } else {
                crate::types_defs::TriState::False
            }),
            false,
        ),
        Object::Integer(value) => (OptVal::Number(value), false),
        Object::String(value) => (OptVal::String(value), false),
        _ => (OptVal::Nil, true),
    }
}

/// Whether an option currently has its default value
/// (`optval_default`).
///
/// # Safety
/// For a visible option, `varp` must point to storage of the option's
/// declared type.
#[allow(dead_code)]
unsafe fn optval_default(
    opt_idx: OptIndex,
    varp: *mut std::ffi::c_void,
) -> bool {
    if is_option_hidden(opt_idx) {
        return true;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let current = unsafe { optval_from_varp(opt_idx, varp) };
    optval_equal(&current, &get_option(opt_idx).def_val)
}

/// Copy an owned string option value (`copy_option_val`).
///
/// The original avoids allocating for its shared
/// `empty_string_option` sentinel. Rust's empty `Vec` has no shared
/// allocation, so `clone` already preserves that optimization's
/// observable behavior.
#[allow(dead_code)]
fn copy_option_val(value: &Option<Vec<u8>>) -> Option<Vec<u8>> {
    value.clone()
}

/// Whether `varp` is `'wildchar'`/`'wildcharm'` and its value has a
/// printable key name (`wc_use_keyname`).
///
/// # Safety
/// `varp` must point to a live `OptInt`.
#[allow(dead_code)]
unsafe fn wc_use_keyname(
    varp: *const crate::types_defs::OptInt,
    wildchar: &mut crate::types_defs::OptInt,
) -> bool {
    let options = crate::option_vars::OPTION_VARS.as_ptr();
    // SAFETY: `as_ptr` gives the stable option-state address.
    let wc = unsafe { std::ptr::addr_of!((*options).p_wc) };
    // SAFETY: `as_ptr` gives the stable option-state address.
    let wcm = unsafe { std::ptr::addr_of!((*options).p_wcm) };
    if varp != wc && varp != wcm {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    *wildchar = unsafe { *varp };
    let key = *wildchar as i32;
    crate::keycodes_defs::is_special(key)
        || crate::keycodes::find_special_key_in_table(key) >= 0
}

/// This option's own flags, or `0` for an invalid index
/// (`get_option_flags`).
#[must_use]
pub fn get_option_flags(opt_idx: OptIndex) -> u32 {
    if opt_idx == OptIndex::Invalid { 0 } else { get_option(opt_idx).flags }
}

/// Process the new `'window'` option value (`did_set_window`).
///
/// Values outside the usable screen range are reset to one less than
/// the current row count, exactly as the original does.
///
/// # Safety
/// Reads `GLOBALS.Rows` and mutates `OPTION_VARS.p_window`.
pub unsafe fn did_set_window(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let rows = unsafe { crate::globals::GLOBALS.get_mut() }.Rows;
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    if opts.p_window < 1 || opts.p_window >= i64::from(rows) {
        opts.p_window = i64::from(rows - 1);
    }
    None
}

/// Process the new `'winblend'` option value (`did_set_winblend`).
///
/// A real value change clamps the window-local setting into `0..=100`,
/// invalidates its highlights, and recomputes grid blending.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn did_set_winblend(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let (old_value, value) = match (&args.os_oldval, &args.os_newval) {
        (
            crate::option_defs::OptVal::Number(old_value),
            crate::option_defs::OptVal::Number(value),
        ) => (*old_value, *value),
        _ => return None,
    };
    if value != old_value {
        let win = args.os_win as *mut crate::buffer_defs::WinT;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            (*win).w_onebuf_opt.wo_winbl = (*win).w_onebuf_opt.wo_winbl.clamp(0, 100);
            (*win).w_hl_needs_update = 1;
            check_blending(&mut *win);
        }
    }
    None
}

/// Process the updated `'number'` or `'relativenumber'` option value
/// (`did_set_number_relativenumber`).
///
/// A custom status column needs its cached width recomputed, and the
/// sign-column bounds are always refreshed.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn did_set_number_relativenumber(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let win = args.os_win as *mut crate::buffer_defs::WinT;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        if (*win)
            .w_onebuf_opt
            .wo_stc
            .as_deref()
            .is_some_and(|value| !value.is_empty())
        {
            (*win).w_nrwidth_line_count = 0;
        }
        let _ = crate::optionstr::check_signcolumn(None, Some(&mut *win));
    }
    None
}

const E_PREVIEW_WINDOW_ALREADY_EXISTS: &[u8] = b"E590: A preview window already exists";

/// Process the updated `'previewwindow'` option value
/// (`did_set_previewwindow`).
///
/// # Safety
/// `args.os_win` must point to a live window, and
/// `GLOBALS.firstwin`'s `w_next` chain must contain live windows.
pub unsafe fn did_set_previewwindow(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let win = args.os_win as *mut crate::buffer_defs::WinT;
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*win).w_onebuf_opt.wo_pvw } == 0 {
        return None;
    }

    // `FOR_ALL_WINDOWS_IN_TAB(wp, curtab)` starts at `firstwin` for
    // this exact call, as established elsewhere in this crate.
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { (*wp).w_onebuf_opt.wo_pvw } != 0 && wp != win {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*win).w_onebuf_opt.wo_pvw = 0 };
            return Some(E_PREVIEW_WINDOW_ALREADY_EXISTS);
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { (*wp).w_next };
    }
    None
}

/// Process the updated `'buflisted'` option value (`did_set_buflisted`).
///
/// # Safety
/// `args.os_buf` must point to a live `BufT`. Also forwarded from the
/// global-state requirements of [`crate::autocmd::apply_autocmds`].
pub unsafe fn did_set_buflisted(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let buf = args.os_buf as *mut crate::buffer_defs::BufT;
    let old_value = match args.os_oldval {
        crate::option_defs::OptVal::Boolean(value) => value as i32,
        _ => return None,
    };
    // SAFETY: forwarded from this function's own safety doc.
    let listed = unsafe { (*buf).b_p_bl };
    if old_value != listed {
        let event = if listed != 0 {
            crate::autocmd_defs::EventT::BufAdd
        } else {
            crate::autocmd_defs::EventT::BufDelete
        };
        // SAFETY: forwarded from this function's own safety doc.
        let _ = crate::autocmd::apply_autocmds(
            event,
            None,
            None,
            true,
            Some(unsafe { &*buf }),
        );
    }
    None
}

/// Process the updated global or buffer-local `'undolevels'` value
/// (`did_set_undolevels`).
///
/// # Safety
/// `args.os_buf`/`args.os_varp` must identify the live buffer and
/// option field for this callback. Forwarded from the selected
/// `did_set_*_undolevels` helper.
pub unsafe fn did_set_undolevels(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let (value, old_value) = match (&args.os_newval, &args.os_oldval) {
        (
            crate::option_defs::OptVal::Number(value),
            crate::option_defs::OptVal::Number(old_value),
        ) => (*value, *old_value),
        _ => return None,
    };
    let buf = args.os_buf as *mut crate::buffer_defs::BufT;
    let varp = args.os_varp.cast::<crate::types_defs::OptInt>();
    let global_ul =
        // SAFETY: `as_ptr` provides the stable address of OPTION_VARS.
        unsafe { std::ptr::addr_of_mut!((*crate::option_vars::OPTION_VARS.as_ptr()).p_ul) };
    if varp == global_ul {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { did_set_global_undolevels(value, old_value) };
    } else {
        let local_ul =
            // SAFETY: `buf` is live by the caller's contract.
            unsafe { std::ptr::addr_of_mut!((*buf).b_p_ul) };
        if varp == local_ul {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { did_set_buflocal_undolevels(buf, value, old_value) };
        }
    }
    None
}

/// Process the updated `'wildchar'`/`'wildcharm'` option value
/// (`did_set_wildchar`).
///
/// # Safety
/// `args.os_varp` must point to a live numeric option value.
pub unsafe fn did_set_wildchar(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let c = unsafe { *args.os_varp.cast::<crate::types_defs::OptInt>() };
    if c == i64::from(crate::ascii_defs::CTRL_C)
        || c == i64::from(b'\n')
        || c == i64::from(b'\r')
        || c == i64::from(crate::keycodes_defs::K_KENTER)
    {
        Some(crate::errors::e_invarg.as_bytes())
    } else {
        None
    }
}

/// Process the new global `'undolevels'` option value
/// (`did_set_global_undolevels`).
///
/// The undo state is synced with the OLD value still installed, then
/// the new one is applied. Doing it the other way round would let
/// `u_sync` misjudge how much history to keep.
///
/// # Safety
/// Forwarded from [`crate::undo::u_sync`]'s own safety doc; also
/// mutates `crate::option_vars::OPTION_VARS`.
pub unsafe fn did_set_global_undolevels(
    value: crate::types_defs::OptInt,
    old_value: crate::types_defs::OptInt,
) -> Option<&'static [u8]> {
    // Sync undo before 'undolevels' changes; use the old value,
    // otherwise u_sync() may not work properly.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul = old_value;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::undo::u_sync(true) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul = value;
    None
}

/// Process the new buffer-local `'undolevels'` option value
/// (`did_set_buflocal_undolevels`).
///
/// The buffer-local mirror of [`did_set_global_undolevels`]; see its
/// doc comment for why the old value is installed first.
///
/// # Safety
/// `buf` must be a valid, non-null pointer to a live `BufT` for the
/// whole call. Also forwarded from [`crate::undo::u_sync`]'s own
/// safety doc.
pub unsafe fn did_set_buflocal_undolevels(
    buf: *mut crate::buffer_defs::BufT,
    value: crate::types_defs::OptInt,
    old_value: crate::types_defs::OptInt,
) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*buf).b_p_ul = old_value };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::undo::u_sync(true) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*buf).b_p_ul = value };
    None
}

/// Process the updated `'langnoremap'` option value
/// (`did_set_langnoremap`).
///
/// `'langnoremap'` and `'langremap'` are exact inverses of one
/// another, so setting either updates the other. This pair of
/// callbacks is what keeps them consistent.
///
/// # Safety
/// Mutates `crate::option_vars::OPTION_VARS`.
pub unsafe fn did_set_langnoremap(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    opts.p_lrm = i32::from(opts.p_lnr == 0);
    None
}

/// Process the updated `'langremap'` option value
/// (`did_set_langremap`).
///
/// The mirror image of [`did_set_langnoremap`]; see its doc comment.
///
/// # Safety
/// Mutates `crate::option_vars::OPTION_VARS`.
pub unsafe fn did_set_langremap(_args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    opts.p_lnr = i32::from(opts.p_lrm == 0);
    None
}

/// Process the updated `'title'` or `'icon'` option value
/// (`did_set_title_icon`).
///
/// One shared callback for both, exactly as upstream registers it.
///
/// # Safety
/// Forwarded from [`did_set_title`]'s own safety doc.
pub unsafe fn did_set_title_icon(_args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    did_set_title();
    None
}

/// Process the updated `'smoothscroll'` option value
/// (`did_set_smoothscroll`).
///
/// Turning `'smoothscroll'` OFF clears `w_skipcol`, since a partially
/// scrolled first line is only meaningful while it is on. Turning it
/// on leaves the value alone.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call.
pub unsafe fn did_set_smoothscroll(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        if (*win_ptr).w_onebuf_opt.wo_sms == 0 {
            (*win_ptr).w_skipcol = 0;
        }
    }
    None
}

/// Process an updated terminal `'scrollback'` value
/// (`did_set_scrollback`).
///
/// Decreasing scrollback on a live terminal needs
/// `on_scrollback_option_changed`, still part of terminal-buffer
/// integration. Every other path is complete.
///
/// # Safety
/// `args.os_buf` must point to a live `BufT`.
pub unsafe fn did_set_scrollback(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let buf = unsafe { &*(args.os_buf as *const crate::buffer_defs::BufT) };
    let old_value = match args.os_oldval {
        crate::option_defs::OptVal::Number(value) => value,
        _ => 0,
    };
    let value = match args.os_newval {
        crate::option_defs::OptVal::Number(value) => value,
        _ => 0,
    };
    if !buf.terminal.is_null() && value < old_value {
        unimplemented!(
            "did_set_scrollback: shrinking a live terminal needs \
             on_scrollback_option_changed"
        );
    }
    None
}

/// Process the updated `'textwidth'` option value
/// (`did_set_textwidth`).
///
/// `'colorcolumn'` can be given relative to `'textwidth'` (`+1`), so
/// every window's resolved column list has to be recomputed.
///
/// As elsewhere in this crate, the original's
/// `FOR_ALL_TAB_WINDOWS(tp, wp)` is walked as
/// `GLOBALS.firstwin`/`w_next`, the established simplification here.
///
/// # Safety
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid, live
/// `WinT` pointers.
pub unsafe fn did_set_textwidth(_args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // The original discards this "is the value valid" result here:
        // it calls check_colorcolumn purely for its recomputation side
        // effect, since 'colorcolumn' itself has not changed.
        // SAFETY: forwarded from this function's own safety doc.
        let _ = unsafe { crate::window::check_colorcolumn(None, Some(&mut *wp)) };
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { (*wp).w_next };
    }
    None
}

/// Process the updated `'titlelen'` option value (`did_set_titlelen`).
///
/// The title is only redrawn when the value actually CHANGED, and
/// never during startup - re-setting `'titlelen'` to what it already
/// was schedules nothing.
///
/// # Safety
/// Mutates `crate::globals::GLOBALS`.
pub unsafe fn did_set_titlelen(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let old_value = match args.os_oldval {
        crate::option_defs::OptVal::Number(n) => n,
        _ => return None,
    };
    // SAFETY: forwarded from this function's own safety doc.
    let starting = unsafe { crate::globals::GLOBALS.get_mut() }.starting;
    // SAFETY: forwarded from this function's own safety doc.
    let p_titlelen = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_titlelen;

    if starting != crate::globals::NO_SCREEN && old_value != p_titlelen {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.need_maketitle = true;
    }
    None
}

/// Build the new value of a string option after a `-=` removal
/// (`stropt_remove_val`).
///
/// `strval` is the offset into `origval` where the matched value
/// starts, and `len` its length. For a
/// [`crate::option_defs::opt_flags::COMMA`] option one separating
/// comma is removed along with the value, mirroring
/// [`remove_comma_item`]'s rule but expressed the way this caller
/// needs it: a leading match takes the comma AFTER it, any other match
/// takes the comma BEFORE it.
///
/// @return the new value. The original writes into a caller-supplied
///         `newval` buffer sized from `origval`; returning an owned
///         `Vec<u8>` removes the need for the caller to size anything.
#[must_use]
pub fn stropt_remove_val(origval: &[u8], flags: u32, strval: usize, len: usize) -> Vec<u8> {
    let mut newval = origval.to_vec();
    // An offset at (or past) the end means nothing matched, so the
    // value is copied through unchanged - the original's `if (*strval)`
    // guard, which is false at the terminating NUL.
    if strval >= origval.len() {
        return newval;
    }

    let (mut start, mut len) = (strval, len);
    if flags & crate::option_defs::opt_flags::COMMA != 0 {
        if start == 0 {
            // Include the comma after the string.
            if origval.get(len) == Some(&b',') {
                len += 1;
            }
        } else {
            // Include the comma before the string.
            start -= 1;
            len += 1;
        }
    }

    newval.drain(start..(start + len).min(newval.len()));
    newval
}

/// Remove flags that appear twice from a flag-list option value
/// (`stropt_remove_dupflags`).
///
/// Two shapes are handled, selected by the option's own flags:
/// - [`crate::option_defs::opt_flags::ONE_COMMA`]: each flag is a
///   single character followed by a comma, so a duplicate takes its
///   trailing comma with it.
/// - otherwise: bare characters, so only the duplicate itself goes.
///   A comma is never treated as a duplicate for a
///   [`crate::option_defs::opt_flags::COMMA`] option.
///
/// The LAST occurrence of each flag is kept and every earlier one
/// removed: the scan drops a character precisely when that same
/// character appears again later in the string. Cross-verified
/// against a real nvim binary, where `whichwrap=b,s,b,h` becomes
/// `s,b,h` - the FIRST `b` is the one that goes.
pub fn stropt_remove_dupflags(newval: &mut Vec<u8>, flags: u32) {
    use crate::option_defs::opt_flags;
    let mut s = 0usize;
    while s < newval.len() {
        if flags & opt_flags::ONE_COMMA != 0 {
            if newval[s] != b','
                && newval.get(s + 1) == Some(&b',')
                && newval[s + 2..].contains(&newval[s])
            {
                // Remove the duplicated value and the next comma.
                newval.drain(s..s + 2);
                continue;
            }
        } else if (flags & opt_flags::COMMA == 0 || newval[s] != b',')
            && newval[s + 1..].contains(&newval[s])
        {
            newval.remove(s);
            continue;
        }
        s += 1;
    }
}

/// Remove the comma-separated item at `[item, item + item_len)` from
/// `str` (`remove_comma_item`).
///
/// Exactly one separating comma goes with the item, so the result is
/// never left with a doubled or dangling comma:
/// - a following comma is taken with the item;
/// - otherwise a preceding comma is taken instead;
/// - a lone item leaves an empty string.
///
/// The original mutates a C buffer in place via `STRMOVE` and relies
/// on a trailing NUL; a `Vec<u8>` carries its own length, so the item
/// is simply drained out.
pub fn remove_comma_item(s: &mut Vec<u8>, item: usize, item_len: usize) {
    let after = item + item_len;
    if s.get(after) == Some(&b',') {
        // Remove item and trailing comma.
        s.drain(item..after + 1);
    } else if item > 0 && s[item - 1] == b',' {
        // Last item: remove leading comma and item.
        s.drain(item - 1..after);
    } else {
        // Only item.
        s.truncate(item);
    }
}

/// Find a comma-separated item matching `key[..key_len]`
/// (`find_key_item`), returning its offset and full item length.
#[must_use]
pub fn find_key_item(src: &[u8], key: &[u8], key_len: usize) -> Option<(usize, usize)> {
    let key = key.get(..key_len)?;
    let mut start = 0usize;
    while start < src.len() {
        let end = src[start..]
            .iter()
            .position(|&b| b == b',')
            .map_or(src.len(), |off| start + off);
        if src[start..end].starts_with(key) {
            return Some((start, end - start));
        }
        start = end.saturating_add(1);
    }
    None
}

/// Remove every comma-separated item matching a key, except an
/// optional item at `skip` (`remove_key_item`).
pub fn remove_key_item(
    s: &mut Vec<u8>,
    key: &[u8],
    key_len: usize,
    skip: Option<usize>,
) {
    while let Some((mut found, mut item_len)) = find_key_item(s, key, key_len) {
        if Some(found) == skip {
            let next = found + item_len + usize::from(s.get(found + item_len) == Some(&b','));
            let Some((offset, len)) = find_key_item(&s[next..], key, key_len) else {
                break;
            };
            found = next + offset;
            item_len = len;
        }
        remove_comma_item(s, found, item_len);
    }
}

/// Append a comma-separated item to the end of `str`
/// (`append_item`).
///
/// A comma is added before the item only when `str` is not already
/// empty, so the result never opens with a stray separator.
pub fn append_item(s: &mut Vec<u8>, item: &[u8]) {
    if !s.is_empty() {
        s.push(b',');
    }
    s.extend_from_slice(item);
}

/// Prepend a comma-separated item to the beginning of `str`
/// (`prepend_item`).
///
/// A comma is added after the item only when `str` is not already
/// empty, so the result never ends with a stray separator.
pub fn prepend_item(s: &mut Vec<u8>, item: &[u8]) {
    let mut prefixed = Vec::with_capacity(item.len() + usize::from(!s.is_empty()) + s.len());
    prefixed.extend_from_slice(item);
    if !s.is_empty() {
        prefixed.push(b',');
    }
    prefixed.extend_from_slice(s);
    *s = prefixed;
}

/// Recognize the `:set` operator prefixing `arg`'s `=`
/// (`get_op`).
///
/// Only a two-character `X=` opening sequence counts, so a bare `+`,
/// or a `+` followed by anything other than `=`, is
/// [`crate::option_defs::SetOpT::None`].
#[must_use]
pub fn get_op(arg: &[u8]) -> crate::option_defs::SetOpT {
    use crate::option_defs::SetOpT;
    if arg.first().is_some_and(|&c| c != 0) && arg.get(1) == Some(&b'=') {
        match arg[0] {
            b'+' => return SetOpT::Adding,
            b'^' => return SetOpT::Prepending,
            b'-' => return SetOpT::Removing,
            _ => {}
        }
    }
    SetOpT::None
}

/// Recognize and consume a `:set` boolean-option prefix
/// (`get_option_prefix`).
///
/// @return the prefix found, and how many bytes of `arg` it occupied -
///         replacing the original's `char **argp` in/out pointer,
///         which the caller advances past the prefix. Reporting the
///         length instead lets the caller keep `arg` as a plain slice.
///
/// The match is a plain prefix test, exactly as upstream: `"nose"` is
/// read as `"no"` + `"se"` rather than as the option name `nose`.
/// Disambiguating that is the caller's job, not this function's.
#[must_use]
pub fn get_option_prefix(arg: &[u8]) -> (crate::option_defs::SetPrefixT, usize) {
    use crate::option_defs::SetPrefixT;
    if arg.starts_with(b"no") {
        (SetPrefixT::No, 2)
    } else if arg.starts_with(b"inv") {
        (SetPrefixT::Inv, 3)
    } else {
        (SetPrefixT::None, 0)
    }
}

/// Process the updated `'modifiable'` option value
/// (`did_set_modifiable`).
///
/// Changing `'modifiable'` only affects what the window title shows.
///
/// # Safety
/// Mutates `crate::globals::GLOBALS` (via [`redraw_titles`]).
pub unsafe fn did_set_modifiable(_args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { redraw_titles() };
    None
}

/// Process the updated `'endoffile'`/`'endofline'`/`'fixendofline'`/
/// `'bomb'` option value (`did_set_eof_eol_fixeol_bomb`).
///
/// One shared callback for all four, exactly as upstream registers it:
/// each of them only affects what the window title and tab page text
/// show.
///
/// # Safety
/// Mutates `crate::globals::GLOBALS` (via [`redraw_titles`]).
pub unsafe fn did_set_eof_eol_fixeol_bomb(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { redraw_titles() };
    None
}

/// Process the updated `'modified'` option value (`did_set_modified`).
///
/// When the buffer is being marked UNmodified, its current
/// `'fileformat'`/`'fileencoding'` are snapshotted, so a later change
/// back can tell whether those settings themselves moved.
///
/// # Safety
/// `args.os_buf` must be a valid, non-null pointer to a live `BufT`
/// for the whole call. Also mutates `crate::globals::GLOBALS`.
pub unsafe fn did_set_modified(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let buf_ptr = args.os_buf as *mut crate::buffer_defs::BufT;
    // A TriState, not a plain bool: only an explicit True counts as
    // "modified", matching the original's own `!!(int)` coercion.
    let newval = matches!(
        args.os_newval,
        crate::option_defs::OptVal::Boolean(crate::types_defs::TriState::True)
    );

    if !newval {
        // SAFETY: forwarded from this function's own safety doc.
        crate::change::save_file_ff(unsafe { &mut *buf_ptr });
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { redraw_titles() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*buf_ptr).b_modified_was_set = newval };
    None
}

/// Mark the window title and tabline for redraw (`redraw_titles`).
///
/// # Safety
/// Mutates `crate::globals::GLOBALS`.
pub unsafe fn redraw_titles() {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    g.need_maketitle = true;
    g.redraw_tabline = true;
}

/// Process the updated `'readonly'` option value (`did_set_readonly`).
///
/// Resetting `'readonly'` globally also clears `readonlymode`, so a
/// `-R`-started session stops opening further files read-only; a
/// `:setlocal` is deliberately excluded from that, since it says
/// nothing about the global intent.
///
/// Setting `'readonly'` clears `b_did_warn` so the "W10: Changing a
/// readonly file" warning can be given again.
///
/// # Safety
/// `args.os_buf` must be a valid, non-null pointer to a live `BufT`
/// for the whole call. Also mutates `crate::globals::GLOBALS`.
pub unsafe fn did_set_readonly(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let buf_ptr = args.os_buf as *mut crate::buffer_defs::BufT;
    // SAFETY: forwarded from this function's own safety doc.
    let b_p_ro = unsafe { (*buf_ptr).b_p_ro };

    // When 'readonly' is reset globally, also reset readonlymode.
    if b_p_ro == 0 && args.os_flags as u32 & crate::option_defs::opt_set_flags::OPT_LOCAL == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.readonlymode = false;
    }

    // When 'readonly' is set we may give W10 again.
    if b_p_ro != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*buf_ptr).b_did_warn = false };
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { redraw_titles() };
    None
}

/// Process the updated `'lisp'` option value (`did_set_lisp`).
///
/// Changing `'lisp'` includes/excludes `-` in the keyword characters,
/// so the buffer's character table is rebuilt. Errors are ignored,
/// exactly as the original does.
///
/// # Safety
/// `args.os_buf` must be a valid, non-null pointer to a live `BufT`
/// for the whole call.
pub unsafe fn did_set_lisp(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let buf_ptr = args.os_buf as *mut crate::buffer_defs::BufT;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::charset::buf_init_chartab(&mut *buf_ptr, false) };
    None
}

/// Process the updated `'wrap'` option value (`did_set_wrap`).
///
/// Only ONE of the two scroll offsets is reset, depending on the new
/// value: a wrapped window cannot be scrolled horizontally (so
/// `w_leftcol` goes), and an unwrapped one has no partially-shown
/// first line (so `w_skipcol` goes). The other is deliberately left
/// alone, matching the original.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call.
pub unsafe fn did_set_wrap(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        if (*win_ptr).w_onebuf_opt.wo_wrap != 0 {
            (*win_ptr).w_leftcol = 0;
        } else {
            (*win_ptr).w_skipcol = 0;
        }
    }
    None
}

/// Process the updated `'hlsearch'` option value (`did_set_hlsearch`).
///
/// Setting *or* resetting `'hlsearch'` clears the "temporarily
/// suppress highlighting" flag, so `:nohlsearch` does not survive a
/// later `:set hlsearch`.
///
/// # Safety
/// Forwarded from [`crate::ex_docmd::set_no_hlsearch`]'s own safety
/// doc.
pub unsafe fn did_set_hlsearch(_args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::ex_docmd::set_no_hlsearch(false) };
    None
}

/// Process the updated `'ignorecase'` option value
/// (`did_set_ignorecase`).
///
/// Only redraws when `'hlsearch'` is on: with highlighting off, the
/// case-sensitivity change has nothing visible to update.
///
/// # Safety
/// Forwarded from [`crate::drawscreen::redraw_all_later`]'s own safety
/// doc.
pub unsafe fn did_set_ignorecase(_args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: a plain scalar copy-out read, no aliasing hazard.
    let p_hls = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hls;
    if p_hls != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::drawscreen::redraw_all_later(crate::drawscreen::UPD_SOME_VALID) };
    }
    None
}

/// Process the updated `'numberwidth'` option value
/// (`did_set_numberwidth`).
///
/// Zeroing `w_nrwidth_line_count` is what triggers the redraw: the
/// number column's width is recomputed whenever that cached line count
/// no longer matches the buffer's.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call.
pub unsafe fn did_set_numberwidth(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*win_ptr).w_nrwidth_line_count = 0 };
    None
}

/// Process the new `'foldlevel'` option value (`did_set_foldlevel`).
///
/// # Safety
/// Forwarded from [`crate::fold::new_fold_level`]'s own safety doc.
pub unsafe fn did_set_foldlevel(_args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::fold::new_fold_level() };
    None
}

/// Process the new `'foldminlines'` option value
/// (`did_set_foldminlines`).
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call.
pub unsafe fn did_set_foldminlines(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::fold::fold_update_all(win_ptr) };
    None
}

/// Process the new `'foldnestmax'` option value
/// (`did_set_foldnestmax`).
///
/// Only the `"syntax"` and `"indent"` fold methods actually consult
/// `'foldnestmax'`, so every other method skips the update entirely -
/// the original's own guard, not an optimization added here.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call.
pub unsafe fn did_set_foldnestmax(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &*win_ptr };
    if crate::fold::foldmethod_is_syntax(win) || crate::fold::foldmethod_is_indent(win) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::fold::fold_update_all(win_ptr) };
    }
    None
}

/// Update the window title/icon after `'titlestring'`/`'iconstring'`/
/// `'title'`/`'icon'` change (`did_set_title`).
///
/// The original's own real body (`maketitle()`, terminal-title
/// escape-sequence output) is gated behind `starting != NO_SCREEN` -
/// `starting` is only ever assigned by `main.c`'s startup sequence
/// (confirmed via a direct search: `starting = NO_BUFFERS`/`= 0`, both
/// in `main.c`, not translated at all in this crate), so
/// `GLOBALS.starting` stays at its own default
/// [`crate::globals::NO_SCREEN`] value forever today, making this
/// guard provably, always false - a real, faithful "always-taken
/// fast path" no-op, matching this crate's established `AUTOCMDS`/
/// `ctx_restore`-style precedent (translates the REAL check, not a
/// hardcoded shortcut, so this becomes correct automatically once
/// `main.c`'s startup sequence is translated, with zero revision
/// needed here).
pub fn did_set_title() {
    // SAFETY: a plain `i32` copy-out read, no aliasing hazard.
    let starting = unsafe { crate::globals::GLOBALS.get_mut() }.starting;
    if starting != crate::globals::NO_SCREEN {
        unimplemented!("did_set_title: maketitle() - terminal title rendering, not translated");
    }
}

#[cfg(test)]
mod did_set_title_tests {
    use super::*;

    struct PWindowGuard(crate::types_defs::OptInt);

    impl PWindowGuard {
        unsafe fn set(value: crate::types_defs::OptInt) -> Self {
            // SAFETY: forwarded from this helper's own contract.
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let saved = opts.p_window;
            opts.p_window = value;
            Self(saved)
        }
    }

    impl Drop for PWindowGuard {
        fn drop(&mut self) {
            // SAFETY: every caller holds `global_state_test_lock`.
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_window = self.0;
        }
    }
    use std::ffi::c_void;

    /// Builds an `OptsetT` pointing at `win`, matching the fixture
    /// `optionstr.rs`'s own fold-callback tests use.
    fn fold_args(
        idx: crate::option_defs::OptIndex,
        win: &mut crate::buffer_defs::WinT,
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_idx: idx,
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            ..Default::default()
        }
    }

    struct GlobalUlGuard(crate::types_defs::OptInt);

    impl GlobalUlGuard {
        fn capture() -> Self {
            Self(unsafe { (*crate::option_vars::OPTION_VARS.as_ptr()).p_ul })
        }
    }

    impl Drop for GlobalUlGuard {
        fn drop(&mut self) {
            unsafe { (*crate::option_vars::OPTION_VARS.as_ptr()).p_ul = self.0 };
        }
    }

    struct WildcharGuard {
        wc: crate::types_defs::OptInt,
        wcm: crate::types_defs::OptInt,
    }

    impl WildcharGuard {
        fn set(wc: crate::types_defs::OptInt, wcm: crate::types_defs::OptInt) -> Self {
            let options = crate::option_vars::OPTION_VARS.as_ptr();
            let saved = Self {
                wc: unsafe { (*options).p_wc },
                wcm: unsafe { (*options).p_wcm },
            };
            unsafe {
                (*options).p_wc = wc;
                (*options).p_wcm = wcm;
            }
            saved
        }
    }

    impl Drop for WildcharGuard {
        fn drop(&mut self) {
            let options = crate::option_vars::OPTION_VARS.as_ptr();
            unsafe {
                (*options).p_wc = self.wc;
                (*options).p_wcm = self.wcm;
            }
        }
    }

    #[test]
    fn optval_equal_matches_the_originals_per_variant_dispatch() {
        use crate::option_defs::OptVal;
        use crate::types_defs::TriState;

        // Nil is equal to Nil.
        assert!(optval_equal(&OptVal::Nil, &OptVal::Nil));
        // Same variant, same payload.
        assert!(optval_equal(
            &OptVal::Boolean(TriState::True),
            &OptVal::Boolean(TriState::True)
        ));
        assert!(optval_equal(&OptVal::Number(7), &OptVal::Number(7)));
        assert!(optval_equal(
            &OptVal::String(b"abc".to_vec()),
            &OptVal::String(b"abc".to_vec())
        ));

        // Same variant, different payload.
        assert!(!optval_equal(
            &OptVal::Boolean(TriState::True),
            &OptVal::Boolean(TriState::False)
        ));
        assert!(!optval_equal(&OptVal::Number(7), &OptVal::Number(8)));
        assert!(!optval_equal(
            &OptVal::String(b"abc".to_vec()),
            &OptVal::String(b"abd".to_vec())
        ));

        // Different variants are never equal, whatever they carry.
        assert!(!optval_equal(&OptVal::Nil, &OptVal::Number(0)));
        assert!(!optval_equal(&OptVal::Number(1), &OptVal::Boolean(TriState::True)));
    }

    #[test]
    fn optval_as_object_maps_every_option_variant() {
        use crate::api::private::defs::Object;
        use crate::option_defs::OptVal;
        assert!(matches!(optval_as_object(OptVal::Nil), Object::Nil));
        assert!(matches!(
            optval_as_object(OptVal::Boolean(crate::types_defs::TriState::None)),
            Object::Nil
        ));
        assert!(matches!(
            optval_as_object(OptVal::Boolean(crate::types_defs::TriState::True)),
            Object::Boolean(true)
        ));
        assert!(matches!(
            optval_as_object(OptVal::Number(42)),
            Object::Integer(42)
        ));
        match optval_as_object(OptVal::String(b"value".to_vec())) {
            Object::String(value) => assert_eq!(value, b"value"),
            other => panic!("unexpected object: {other:?}"),
        }
    }

    #[test]
    fn object_as_optval_maps_supported_types_and_flags_others() {
        use crate::api::private::defs::Object;
        use crate::option_defs::OptVal;
        assert_eq!(object_as_optval(Object::Nil), (OptVal::Nil, false));
        assert_eq!(
            object_as_optval(Object::Boolean(false)),
            (
                OptVal::Boolean(crate::types_defs::TriState::False),
                false
            )
        );
        assert_eq!(
            object_as_optval(Object::Integer(7)),
            (OptVal::Number(7), false)
        );
        assert_eq!(
            object_as_optval(Object::String(b"text".to_vec())),
            (OptVal::String(b"text".to_vec()), false)
        );
        assert_eq!(
            object_as_optval(Object::Float(1.5)),
            (OptVal::Nil, true)
        );
    }

    #[test]
    fn get_option_flags_reports_zero_for_an_invalid_index() {
        assert_eq!(get_option_flags(OptIndex::Invalid), 0);
    }

    #[test]
    fn optval_default_short_circuits_hidden_and_compares_visible_values() {
        assert!(unsafe { optval_default(OptIndex::Aleph, std::ptr::null_mut()) });

        let mut value = 0i32;
        let varp = std::ptr::addr_of_mut!(value).cast();
        assert!(unsafe { optval_default(OptIndex::Allowrevins, varp) });
        unsafe { *varp.cast::<i32>() = 1 };
        assert!(!unsafe { optval_default(OptIndex::Allowrevins, varp) });
    }

    #[test]
    fn copy_option_val_deep_copies_nonempty_and_preserves_empty_states() {
        let mut original = Some(b"value".to_vec());
        let copied = copy_option_val(&original);
        original.as_mut().expect("value")[0] = b'V';
        assert_eq!(copied.as_deref(), Some(b"value".as_slice()));

        assert_eq!(copy_option_val(&Some(Vec::new())), Some(Vec::new()));
        assert_eq!(copy_option_val(&None), None);
    }

    #[test]
    fn wc_use_keyname_requires_wild_option_identity_and_a_named_key() {
        let _lock = crate::globals::global_state_test_lock();
        let options = crate::option_vars::OPTION_VARS.as_ptr();
        let _guard =
            WildcharGuard::set(i64::from(crate::keycodes_defs::K_UP), i64::from(b'x'));

        let mut value = -99;
        assert!(unsafe {
            wc_use_keyname(std::ptr::addr_of!((*options).p_wc), &mut value)
        });
        assert_eq!(value, i64::from(crate::keycodes_defs::K_UP));

        assert!(!unsafe {
            wc_use_keyname(std::ptr::addr_of!((*options).p_wcm), &mut value)
        });
        assert_eq!(value, i64::from(b'x'));

        let unrelated = i64::from(crate::keycodes_defs::K_UP);
        value = 77;
        assert!(!unsafe {
            wc_use_keyname(std::ptr::addr_of!(unrelated), &mut value)
        });
        assert_eq!(value, 77);

    }

    #[test]
    fn get_option_flags_matches_the_options_table() {
        // A real option must report exactly its table entry's flags.
        for idx in [OptIndex::Path, OptIndex::Whichwrap, OptIndex::Shortmess] {
            assert_eq!(get_option_flags(idx), get_option(idx).flags);
        }
        // 'path' is a comma-separated list, so at minimum COMMA is set.
        assert_ne!(
            get_option_flags(OptIndex::Path) & crate::option_defs::opt_flags::COMMA,
            0
        );
    }

    #[test]
    fn did_set_window_clamps_values_outside_the_screen_range() {
        let _lock = crate::globals::global_state_test_lock();
        let _rows =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.Rows, 24) };
        let _window = unsafe { PWindowGuard::set(0) };
        let mut args = crate::option_defs::OptsetT::default();

        assert_eq!(unsafe { did_set_window(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_window, 23);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_window = 12;
        assert_eq!(unsafe { did_set_window(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_window, 12);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_window = 24;
        assert_eq!(unsafe { did_set_window(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_window, 23);
    }

    #[test]
    fn did_set_winblend_clamps_and_invalidates_only_when_the_value_changes() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_winbl = 150;
        let win_ptr = std::ptr::addr_of_mut!(win);
        let mut args = crate::option_defs::OptsetT {
            os_win: win_ptr.cast(),
            os_oldval: crate::option_defs::OptVal::Number(20),
            os_newval: crate::option_defs::OptVal::Number(20),
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_winblend(&mut args) }, None);
        assert_eq!(unsafe { (*win_ptr).w_onebuf_opt.wo_winbl }, 150);
        assert_eq!(unsafe { (*win_ptr).w_hl_needs_update }, 0);
        assert!(!unsafe { (*win_ptr).w_grid_alloc.blending });

        args.os_newval = crate::option_defs::OptVal::Number(150);
        assert_eq!(unsafe { did_set_winblend(&mut args) }, None);
        assert_eq!(unsafe { (*win_ptr).w_onebuf_opt.wo_winbl }, 100);
        assert_eq!(unsafe { (*win_ptr).w_hl_needs_update }, 1);
        assert!(unsafe { (*win_ptr).w_grid_alloc.blending });

        unsafe { (*win_ptr).w_onebuf_opt.wo_winbl = -7 };
        args.os_oldval = crate::option_defs::OptVal::Number(150);
        args.os_newval = crate::option_defs::OptVal::Number(-7);
        assert_eq!(unsafe { did_set_winblend(&mut args) }, None);
        assert_eq!(unsafe { (*win_ptr).w_onebuf_opt.wo_winbl }, 0);
        assert!(!unsafe { (*win_ptr).w_grid_alloc.blending });
    }

    #[test]
    fn did_set_number_relativenumber_resets_statuscolumn_width_and_sign_bounds() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_stc = Some(b"%l".to_vec());
        win.w_onebuf_opt.wo_scl = Some(b"yes".to_vec());
        win.w_nrwidth_line_count = 42;
        let win_ptr = std::ptr::addr_of_mut!(win);
        let mut args = crate::option_defs::OptsetT {
            os_win: win_ptr.cast(),
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_number_relativenumber(&mut args) }, None);
        assert_eq!(unsafe { (*win_ptr).w_nrwidth_line_count }, 0);
        assert_eq!(unsafe { (*win_ptr).w_minscwidth }, 1);
        assert_eq!(unsafe { (*win_ptr).w_maxscwidth }, 1);
    }

    #[test]
    fn did_set_number_relativenumber_keeps_width_without_statuscolumn() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_stc = None;
        win.w_onebuf_opt.wo_scl = Some(b"no".to_vec());
        win.w_nrwidth_line_count = 42;
        let win_ptr = std::ptr::addr_of_mut!(win);
        let mut args = crate::option_defs::OptsetT {
            os_win: win_ptr.cast(),
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_number_relativenumber(&mut args) }, None);
        assert_eq!(unsafe { (*win_ptr).w_nrwidth_line_count }, 42);
        assert_eq!(
            unsafe { (*win_ptr).w_minscwidth },
            crate::option_vars::SCL_NO
        );
        assert_eq!(
            unsafe { (*win_ptr).w_maxscwidth },
            crate::option_vars::SCL_NO
        );
    }

    #[test]
    fn did_set_previewwindow_rejects_a_second_preview_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut first = crate::buffer_defs::WinT::default();
        let first_ptr = std::ptr::addr_of_mut!(first);
        let mut target = crate::buffer_defs::WinT::default();
        let target_ptr = std::ptr::addr_of_mut!(target);
        unsafe {
            (*first_ptr).w_onebuf_opt.wo_pvw = 1;
            (*first_ptr).w_next = target_ptr;
            (*target_ptr).w_onebuf_opt.wo_pvw = 1;
        }
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.firstwin, first_ptr)
        };
        let mut args = crate::option_defs::OptsetT {
            os_win: target_ptr.cast(),
            ..Default::default()
        };

        assert_eq!(
            unsafe { did_set_previewwindow(&mut args) },
            Some(E_PREVIEW_WINDOW_ALREADY_EXISTS)
        );
        assert_eq!(unsafe { (*target_ptr).w_onebuf_opt.wo_pvw }, 0);
        assert_eq!(unsafe { (*first_ptr).w_onebuf_opt.wo_pvw }, 1);
    }

    #[test]
    fn did_set_previewwindow_accepts_the_only_preview_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = std::ptr::addr_of_mut!(win);
        unsafe { (*win_ptr).w_onebuf_opt.wo_pvw = 1 };
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.firstwin, win_ptr)
        };
        let mut args = crate::option_defs::OptsetT {
            os_win: win_ptr.cast(),
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_previewwindow(&mut args) }, None);
        assert_eq!(unsafe { (*win_ptr).w_onebuf_opt.wo_pvw }, 1);
    }

    #[test]
    fn did_set_previewwindow_is_a_noop_when_disabling_the_option() {
        let _lock = crate::globals::global_state_test_lock();
        let mut existing = crate::buffer_defs::WinT::default();
        let existing_ptr = std::ptr::addr_of_mut!(existing);
        unsafe { (*existing_ptr).w_onebuf_opt.wo_pvw = 1 };
        let mut target = crate::buffer_defs::WinT::default();
        let target_ptr = std::ptr::addr_of_mut!(target);
        unsafe { (*existing_ptr).w_next = target_ptr };
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.firstwin, existing_ptr)
        };
        let mut args = crate::option_defs::OptsetT {
            os_win: target_ptr.cast(),
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_previewwindow(&mut args) }, None);
        assert_eq!(unsafe { (*existing_ptr).w_onebuf_opt.wo_pvw }, 1);
    }

    #[test]
    fn did_set_buflisted_handles_add_delete_and_unchanged_values() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_bl: 1,
            ..Default::default()
        };
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let mut args = crate::option_defs::OptsetT {
            os_buf: buf_ptr.cast(),
            os_oldval: crate::option_defs::OptVal::Boolean(
                crate::types_defs::TriState::False,
            ),
            ..Default::default()
        };

        // BufAdd. The autocmd registry is empty today, so the real
        // apply_autocmds fast path returns without running commands.
        assert_eq!(unsafe { did_set_buflisted(&mut args) }, None);

        unsafe { (*buf_ptr).b_p_bl = 0 };
        args.os_oldval = crate::option_defs::OptVal::Boolean(
            crate::types_defs::TriState::True,
        );
        // BufDelete.
        assert_eq!(unsafe { did_set_buflisted(&mut args) }, None);

        // No change: no event is dispatched.
        args.os_oldval = crate::option_defs::OptVal::Boolean(
            crate::types_defs::TriState::False,
        );
        assert_eq!(unsafe { did_set_buflisted(&mut args) }, None);
    }

    #[test]
    fn did_set_undolevels_dispatches_by_the_option_field_address() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_ul: 40,
            b_u_synced: true,
            ..Default::default()
        };
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let _curbuf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr) };
        let _global_ul = GlobalUlGuard::capture();
        let global_ptr =
            unsafe { std::ptr::addr_of_mut!((*crate::option_vars::OPTION_VARS.as_ptr()).p_ul) };
        let mut args = crate::option_defs::OptsetT {
            os_buf: buf_ptr.cast(),
            os_varp: global_ptr.cast(),
            os_oldval: crate::option_defs::OptVal::Number(80),
            os_newval: crate::option_defs::OptVal::Number(100),
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_undolevels(&mut args) }, None);
        assert_eq!(
            unsafe { (*crate::option_vars::OPTION_VARS.as_ptr()).p_ul },
            100
        );

        let local_ptr = unsafe { std::ptr::addr_of_mut!((*buf_ptr).b_p_ul) };
        args.os_varp = local_ptr.cast();
        args.os_oldval = crate::option_defs::OptVal::Number(40);
        args.os_newval = crate::option_defs::OptVal::Number(60);
        assert_eq!(unsafe { did_set_undolevels(&mut args) }, None);
        assert_eq!(unsafe { (*buf_ptr).b_p_ul }, 60);

    }

    #[test]
    fn did_set_wildchar_rejects_control_c_newlines_and_keypad_enter() {
        for c in [
            i64::from(crate::ascii_defs::CTRL_C),
            i64::from(b'\n'),
            i64::from(b'\r'),
            i64::from(crate::keycodes_defs::K_KENTER),
        ] {
            let mut value = c;
            let mut args = crate::option_defs::OptsetT {
                os_varp: std::ptr::addr_of_mut!(value).cast(),
                ..Default::default()
            };
            assert_eq!(
                unsafe { did_set_wildchar(&mut args) },
                Some(crate::errors::e_invarg.as_bytes())
            );
        }
    }

    #[test]
    fn did_set_wildchar_accepts_usable_key_values() {
        for c in [i64::from(b'\t'), i64::from(b'*'), 0x1234] {
            let mut value = c;
            let mut args = crate::option_defs::OptsetT {
                os_varp: std::ptr::addr_of_mut!(value).cast(),
                ..Default::default()
            };
            assert_eq!(unsafe { did_set_wildchar(&mut args) }, None);
        }
    }

    #[test]
    fn did_set_global_undolevels_ends_with_the_new_value() {
        // The OLD value is installed while u_sync runs, then the new
        // one is applied - so the observable end state is the new
        // value, but the sync sees the old one.
        //
        // u_sync dereferences GLOBALS.curbuf, so a real buffer must be
        // installed BEFORE the call (and restored after).
        let _lock = crate::globals::global_state_test_lock();
        let prev_ul = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul;
        let prev_buf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let mut curbuf = crate::buffer_defs::BufT { b_u_synced: true, ..Default::default() };
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut curbuf;

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul = 85;
        assert_eq!(unsafe { did_set_global_undolevels(100, 85) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul, 100);

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_buf;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul = prev_ul;
    }

    #[test]
    fn did_set_buflocal_undolevels_ends_with_the_new_value() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_buf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let mut curbuf = crate::buffer_defs::BufT { b_u_synced: true, ..Default::default() };
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut curbuf;

        let mut buf = crate::buffer_defs::BufT { b_p_ul: 85, ..Default::default() };
        assert_eq!(unsafe { did_set_buflocal_undolevels(&mut buf, 50, 85) }, None);
        assert_eq!(buf.b_p_ul, 50);

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_buf;
    }

    #[test]
    fn did_set_buflocal_undolevels_does_not_touch_the_global_value() {
        // Cross-verified against real nvim: :setlocal undolevels=50
        // leaves &g:undolevels at its own value.
        let _lock = crate::globals::global_state_test_lock();
        let prev_ul = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul;
        let prev_buf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let mut curbuf = crate::buffer_defs::BufT { b_u_synced: true, ..Default::default() };
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut curbuf;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul = 100;

        let mut buf = crate::buffer_defs::BufT { b_p_ul: 85, ..Default::default() };
        assert_eq!(unsafe { did_set_buflocal_undolevels(&mut buf, 50, 85) }, None);
        assert_eq!(buf.b_p_ul, 50);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul, 100);

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_buf;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ul = prev_ul;
    }

    #[test]
    fn langremap_and_langnoremap_stay_exact_inverses() {
        // Cross-verified against real nvim:
        //   set langremap     -> langremap 1, langnoremap 0
        //   set nolangremap   -> langremap 0, langnoremap 1
        //   set langnoremap   -> langnoremap 1, langremap 0
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let (prev_lnr, prev_lrm) = (opts.p_lnr, opts.p_lrm);

        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Langremap,
            ..Default::default()
        };

        // Setting 'langremap' clears 'langnoremap'.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lrm = 1;
        assert_eq!(unsafe { did_set_langremap(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lnr, 0);

        // Clearing it sets 'langnoremap'.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lrm = 0;
        assert_eq!(unsafe { did_set_langremap(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lnr, 1);

        // And the mirror direction.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lnr = 1;
        assert_eq!(unsafe { did_set_langnoremap(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lrm, 0);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lnr = 0;
        assert_eq!(unsafe { did_set_langnoremap(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lrm, 1);

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_lnr = prev_lnr;
        opts.p_lrm = prev_lrm;
    }

    #[test]
    fn did_set_title_icon_delegates_to_did_set_title() {
        // Shares one callback for both 'title' and 'icon'. Must not
        // panic given `starting`'s own real default (see
        // did_set_title's doc comment).
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.starting;
        unsafe { crate::globals::GLOBALS.get_mut() }.starting = crate::globals::NO_SCREEN;

        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Title,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_title_icon(&mut args) }, None);

        unsafe { crate::globals::GLOBALS.get_mut() }.starting = prev;
    }

    #[test]
    fn did_set_smoothscroll_clears_skipcol_only_when_turned_off() {
        // A partially scrolled first line is only meaningful while
        // 'smoothscroll' is ON, so turning it off clears w_skipcol and
        // turning it on leaves the value alone.
        let mut win = crate::buffer_defs::WinT { w_skipcol: 12, ..Default::default() };
        win.w_onebuf_opt.wo_sms = 0;
        let mut args = fold_args(crate::option_defs::OptIndex::Smoothscroll, &mut win);
        assert_eq!(unsafe { did_set_smoothscroll(&mut args) }, None);
        assert_eq!(win.w_skipcol, 0);

        let mut win = crate::buffer_defs::WinT { w_skipcol: 12, ..Default::default() };
        win.w_onebuf_opt.wo_sms = 1;
        let mut args = fold_args(crate::option_defs::OptIndex::Smoothscroll, &mut win);
        assert_eq!(unsafe { did_set_smoothscroll(&mut args) }, None);
        assert_eq!(win.w_skipcol, 12, "w_skipcol must survive turning it ON");
    }

    #[test]
    fn did_set_scrollback_ignores_nonterminal_buffers_and_increases() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut args = crate::option_defs::OptsetT {
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut std::ffi::c_void,
            os_oldval: crate::option_defs::OptVal::Number(100),
            os_newval: crate::option_defs::OptVal::Number(50),
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_scrollback(&mut args) }, None);

        let mut terminal = crate::types_defs::TerminalT::default();
        let mut buf = crate::buffer_defs::BufT {
            terminal: &mut terminal,
            ..Default::default()
        };
        let mut args = crate::option_defs::OptsetT {
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut std::ffi::c_void,
            os_oldval: crate::option_defs::OptVal::Number(50),
            os_newval: crate::option_defs::OptVal::Number(100),
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_scrollback(&mut args) }, None);
    }

    #[test]
    #[should_panic(expected = "on_scrollback_option_changed")]
    fn did_set_scrollback_shrinking_a_live_terminal_needs_terminal_integration() {
        let mut terminal = crate::types_defs::TerminalT::default();
        let mut buf = crate::buffer_defs::BufT {
            terminal: &mut terminal,
            ..Default::default()
        };
        let mut args = crate::option_defs::OptsetT {
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut std::ffi::c_void,
            os_oldval: crate::option_defs::OptVal::Number(100),
            os_newval: crate::option_defs::OptVal::Number(50),
            ..Default::default()
        };
        let _ = unsafe { did_set_scrollback(&mut args) };
    }

    #[test]
    fn did_set_textwidth_walks_every_window() {
        // 'colorcolumn' can be relative to 'textwidth' (+1), so every
        // window's resolved column list is recomputed. Two windows are
        // chained so the walk itself is exercised, not just the head.
        let _lock = crate::globals::global_state_test_lock();
        let prev_firstwin = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;

        let mut second = crate::buffer_defs::WinT::default();
        let mut first = crate::buffer_defs::WinT {
            w_next: &mut second,
            ..crate::buffer_defs::WinT::default()
        };
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = &mut first;

        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Textwidth,
            ..Default::default()
        };
        // Must walk the whole chain without panicking on the null
        // w_buffer both fixtures carry.
        assert_eq!(unsafe { did_set_textwidth(&mut args) }, None);

        first.w_next = std::ptr::null_mut();
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    #[test]
    fn did_set_titlelen_only_redraws_on_a_real_change() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_start, prev_title) = (g.starting, g.need_maketitle);
        let prev_len = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_titlelen;

        // Past startup, with a genuinely different old value.
        unsafe { crate::globals::GLOBALS.get_mut() }.starting = 0;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_titlelen = 50;
        unsafe { crate::globals::GLOBALS.get_mut() }.need_maketitle = false;
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Titlelen,
            os_oldval: crate::option_defs::OptVal::Number(85),
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_titlelen(&mut args) }, None);
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.need_maketitle);

        // Re-setting it to what it already was schedules nothing.
        unsafe { crate::globals::GLOBALS.get_mut() }.need_maketitle = false;
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Titlelen,
            os_oldval: crate::option_defs::OptVal::Number(50),
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_titlelen(&mut args) }, None);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.need_maketitle);

        // During startup nothing is scheduled even for a real change.
        unsafe { crate::globals::GLOBALS.get_mut() }.starting = crate::globals::NO_SCREEN;
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Titlelen,
            os_oldval: crate::option_defs::OptVal::Number(85),
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_titlelen(&mut args) }, None);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.need_maketitle);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.starting = prev_start;
        g.need_maketitle = prev_title;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_titlelen = prev_len;
    }

    #[test]
    fn stropt_remove_val_first_item_takes_the_comma_after_it() {
        // Cross-verified against real nvim:
        //   set path=/a,/b,/c | set path-=/a  ->  /b,/c
        use crate::option_defs::opt_flags;
        let out = stropt_remove_val(b"/a,/b,/c", opt_flags::COMMA, 0, 2);
        assert_eq!(out, b"/b,/c");
    }

    #[test]
    fn stropt_remove_val_middle_item_takes_the_comma_before_it() {
        // Cross-verified: set path-=/b  ->  /a,/c
        use crate::option_defs::opt_flags;
        let out = stropt_remove_val(b"/a,/b,/c", opt_flags::COMMA, 3, 2);
        assert_eq!(out, b"/a,/c");
    }

    #[test]
    fn stropt_remove_val_last_item_takes_the_comma_before_it() {
        // Cross-verified: set path-=/c  ->  /a,/b
        use crate::option_defs::opt_flags;
        let out = stropt_remove_val(b"/a,/b,/c", opt_flags::COMMA, 6, 2);
        assert_eq!(out, b"/a,/b");
    }

    #[test]
    fn stropt_remove_val_only_item_leaves_it_empty() {
        use crate::option_defs::opt_flags;
        let out = stropt_remove_val(b"/a", opt_flags::COMMA, 0, 2);
        assert_eq!(out, b"");
    }

    #[test]
    fn stropt_remove_val_without_the_comma_flag_leaves_separators_alone() {
        // A non-COMMA option has no separator to account for, so only
        // the matched span itself is removed.
        let out = stropt_remove_val(b"abcdef", 0, 2, 2);
        assert_eq!(out, b"abef");
    }

    #[test]
    fn stropt_remove_val_with_an_out_of_range_offset_copies_through() {
        // The original's `if (*strval)` guard is false at the
        // terminating NUL, so nothing is removed.
        use crate::option_defs::opt_flags;
        let out = stropt_remove_val(b"/a,/b", opt_flags::COMMA, 5, 2);
        assert_eq!(out, b"/a,/b");
    }

    #[test]
    fn stropt_remove_dupflags_one_comma_keeps_the_last_occurrence() {
        // Cross-verified against real nvim:
        //   set whichwrap=b,s,b,h  ->  s,b,h
        // The FIRST 'b' goes, not the second - the scan drops a flag
        // precisely when the same one appears again later.
        use crate::option_defs::opt_flags;
        let mut s = b"b,s,b,h".to_vec();
        stropt_remove_dupflags(&mut s, opt_flags::ONE_COMMA);
        assert_eq!(s, b"s,b,h");
    }

    #[test]
    fn stropt_remove_dupflags_one_comma_collapses_a_pure_duplicate() {
        // Cross-verified: set whichwrap=b,b  ->  b
        use crate::option_defs::opt_flags;
        let mut s = b"b,b".to_vec();
        stropt_remove_dupflags(&mut s, opt_flags::ONE_COMMA);
        assert_eq!(s, b"b");
    }

    #[test]
    fn stropt_remove_dupflags_bare_flags_keep_the_last_occurrence() {
        // Cross-verified against real nvim:
        //   set shortmess=filf  ->  ilf
        // Same last-wins rule, but without a comma to carry along.
        let mut s = b"filf".to_vec();
        stropt_remove_dupflags(&mut s, 0);
        assert_eq!(s, b"ilf");
    }

    #[test]
    fn stropt_remove_dupflags_leaves_a_unique_list_alone() {
        use crate::option_defs::opt_flags;
        let mut s = b"b,s,h".to_vec();
        stropt_remove_dupflags(&mut s, opt_flags::ONE_COMMA);
        assert_eq!(s, b"b,s,h");

        let mut s = b"ilf".to_vec();
        stropt_remove_dupflags(&mut s, 0);
        assert_eq!(s, b"ilf");
    }

    #[test]
    fn stropt_remove_dupflags_never_treats_a_comma_as_a_duplicate() {
        // For a COMMA option the separators themselves must survive,
        // however many of them there are.
        use crate::option_defs::opt_flags;
        let mut s = b"a,b,c".to_vec();
        stropt_remove_dupflags(&mut s, opt_flags::COMMA);
        assert_eq!(s, b"a,b,c");
    }

    #[test]
    fn stropt_remove_dupflags_of_an_empty_value_is_a_no_op() {
        use crate::option_defs::opt_flags;
        let mut s = Vec::new();
        stropt_remove_dupflags(&mut s, opt_flags::ONE_COMMA);
        assert_eq!(s, b"");
    }

    #[test]
    fn remove_comma_item_takes_the_trailing_comma_with_a_middle_item() {
        // Cross-verified against real nvim:
        //   set path=/a,/b,/c | set path-=/b  ->  /a,/c
        let mut s = b"/a,/b,/c".to_vec();
        remove_comma_item(&mut s, 3, 2);
        assert_eq!(s, b"/a,/c");
    }

    #[test]
    fn remove_comma_item_takes_the_leading_comma_with_the_last_item() {
        // Cross-verified: set path=/a,/b | set path-=/b  ->  /a
        let mut s = b"/a,/b".to_vec();
        remove_comma_item(&mut s, 3, 2);
        assert_eq!(s, b"/a");
    }

    #[test]
    fn remove_comma_item_of_the_only_item_leaves_it_empty() {
        // Cross-verified: set path=/a | set path-=/a  ->  ""
        let mut s = b"/a".to_vec();
        remove_comma_item(&mut s, 0, 2);
        assert_eq!(s, b"");
    }

    #[test]
    fn remove_comma_item_of_the_first_of_several_keeps_the_rest_joined() {
        // The trailing-comma branch, taken at offset 0.
        let mut s = b"/a,/b,/c".to_vec();
        remove_comma_item(&mut s, 0, 2);
        assert_eq!(s, b"/b,/c");
    }

    #[test]
    fn find_key_item_returns_the_matching_item_and_full_length() {
        assert_eq!(
            find_key_item(b"a:1,b:22,a:333", b"a:", 2),
            Some((0, 3))
        );
        assert_eq!(
            find_key_item(b"a:1,b:22,a:333", b"b:", 2),
            Some((4, 4))
        );
        assert_eq!(find_key_item(b"a:1,b:22", b"c:", 2), None);
    }

    #[test]
    fn find_key_item_matches_only_at_item_boundaries() {
        assert_eq!(
            find_key_item(b"prefixa:1,a:2", b"a:", 2),
            Some((10, 3))
        );
    }

    #[test]
    fn remove_key_item_removes_every_matching_key() {
        let mut value = b"a:1,b:2,a:3,c:4".to_vec();
        remove_key_item(&mut value, b"a:", 2, None);
        assert_eq!(value, b"b:2,c:4");
    }

    #[test]
    fn remove_key_item_keeps_the_requested_match_only() {
        let mut value = b"a:1,b:2,a:3,a:4".to_vec();
        remove_key_item(&mut value, b"a:", 2, Some(0));
        assert_eq!(value, b"a:1,b:2");
    }

    #[test]
    fn append_item_does_not_lead_with_a_separator() {
        // Cross-verified: set path= | set path+=/x  ->  /x
        let mut s = Vec::new();
        append_item(&mut s, b"/x");
        assert_eq!(s, b"/x");

        // Cross-verified: a second append then separates with a comma.
        append_item(&mut s, b"/y");
        assert_eq!(s, b"/x,/y");
    }

    #[test]
    fn prepend_item_does_not_trail_with_a_separator() {
        let mut s = Vec::new();
        prepend_item(&mut s, b"/x");
        assert_eq!(s, b"/x");

        prepend_item(&mut s, b"/y");
        assert_eq!(s, b"/y,/x");
    }

    #[test]
    fn prepend_and_append_preserve_item_order() {
        let mut s = b"middle".to_vec();
        prepend_item(&mut s, b"first");
        append_item(&mut s, b"last");
        assert_eq!(s, b"first,middle,last");
    }

    #[test]
    fn append_then_remove_round_trips() {
        // Removing what was just appended must restore the original,
        // with no doubled or dangling comma left behind.
        let mut s = b"/a,/b".to_vec();
        let before = s.clone();
        let at = s.len() + 1;
        append_item(&mut s, b"/c");
        assert_eq!(s, b"/a,/b,/c");
        remove_comma_item(&mut s, at, 2);
        assert_eq!(s, before);
    }

    #[test]
    fn get_op_recognizes_each_set_operator() {
        use crate::option_defs::SetOpT;
        // Cross-verified against real nvim: += appends, ^= prepends
        // and -= removes.
        assert_eq!(get_op(b"+=/b"), SetOpT::Adding);
        assert_eq!(get_op(b"^=/z"), SetOpT::Prepending);
        assert_eq!(get_op(b"-=/b"), SetOpT::Removing);
        assert_eq!(get_op(b"=/a"), SetOpT::None);
    }

    #[test]
    fn get_op_needs_the_equals_in_the_second_position() {
        use crate::option_defs::SetOpT;
        // Only a two-character "X=" opening sequence counts.
        assert_eq!(get_op(b"+"), SetOpT::None);
        assert_eq!(get_op(b"+x="), SetOpT::None);
        assert_eq!(get_op(b""), SetOpT::None);
        // A recognized operator character but no '=' is still None.
        assert_eq!(get_op(b"-x"), SetOpT::None);
    }

    #[test]
    fn get_option_prefix_recognizes_and_measures_no_and_inv() {
        use crate::option_defs::SetPrefixT;
        // Cross-verified against real nvim: :set nonumber clears it,
        // :set invnumber toggles it back.
        assert_eq!(get_option_prefix(b"number"), (SetPrefixT::None, 0));
        assert_eq!(get_option_prefix(b"nonumber"), (SetPrefixT::No, 2));
        assert_eq!(get_option_prefix(b"invnumber"), (SetPrefixT::Inv, 3));
    }

    #[test]
    fn get_option_prefix_is_a_plain_prefix_test() {
        use crate::option_defs::SetPrefixT;
        // "nose" reads as "no" + "se", exactly as upstream - telling
        // that apart from a real option named "nose" is the caller's
        // job, not this function's.
        assert_eq!(get_option_prefix(b"nose"), (SetPrefixT::No, 2));
        // Too short to carry the prefix at all.
        assert_eq!(get_option_prefix(b"n"), (SetPrefixT::None, 0));
        assert_eq!(get_option_prefix(b""), (SetPrefixT::None, 0));
    }

    #[test]
    fn set_prefix_discriminants_match_the_original() {
        use crate::option_defs::SetPrefixT;
        // The "no prefix" case is deliberately NOT the zero value.
        assert_eq!(SetPrefixT::No as i32, 0);
        assert_eq!(SetPrefixT::None as i32, 1);
        assert_eq!(SetPrefixT::Inv as i32, 2);
    }

    #[test]
    fn modifiable_and_eof_eol_fixeol_bomb_both_request_a_title_redraw() {
        // Both are pure "the title text may have changed" callbacks,
        // so each must set both redraw flags and touch nothing else.
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_title, prev_tabline) = (g.need_maketitle, g.redraw_tabline);

        for which in 0..2 {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.need_maketitle = false;
            g.redraw_tabline = false;

            let mut args = crate::option_defs::OptsetT {
                os_idx: crate::option_defs::OptIndex::Modifiable,
                ..Default::default()
            };
            let r = if which == 0 {
                unsafe { did_set_modifiable(&mut args) }
            } else {
                unsafe { did_set_eof_eol_fixeol_bomb(&mut args) }
            };
            assert_eq!(r, None);

            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            assert!(g.need_maketitle);
            assert!(g.redraw_tabline);
        }

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.need_maketitle = prev_title;
        g.redraw_tabline = prev_tabline;
    }

    #[test]
    fn did_set_modified_unmodified_snapshots_the_file_format() {
        // Marking the buffer UNmodified snapshots the current
        // 'fileformat'/'fileencoding', so a later change back can tell
        // whether those settings themselves moved.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_fenc: Some(b"utf-8".to_vec()),
            b_p_bomb: 1,
            b_modified_was_set: true,
            ..Default::default()
        };
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Modified,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_newval: crate::option_defs::OptVal::Boolean(crate::types_defs::TriState::False),
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_modified(&mut args) }, None);
        assert_eq!(buf.b_start_fenc.as_deref(), Some(&b"utf-8"[..]));
        assert_eq!(buf.b_start_bomb, 1);
        assert!(!buf.b_modified_was_set);
    }

    #[test]
    fn did_set_modified_modified_does_not_snapshot() {
        // Marking it MODIFIED must not snapshot: b_start_fenc stays
        // whatever it was, so file_ff_differs can still see the change.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_fenc: Some(b"latin1".to_vec()),
            b_start_fenc: None,
            b_modified_was_set: false,
            ..Default::default()
        };
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Modified,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_newval: crate::option_defs::OptVal::Boolean(crate::types_defs::TriState::True),
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_modified(&mut args) }, None);
        assert_eq!(buf.b_start_fenc, None, "must NOT snapshot when marking modified");
        assert!(buf.b_modified_was_set);
    }

    #[test]
    fn redraw_titles_sets_both_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_title, prev_tabline) = (g.need_maketitle, g.redraw_tabline);
        g.need_maketitle = false;
        g.redraw_tabline = false;

        unsafe { redraw_titles() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(g.need_maketitle);
        assert!(g.redraw_tabline);
        g.need_maketitle = prev_title;
        g.redraw_tabline = prev_tabline;
    }

    #[test]
    fn did_set_readonly_reset_globally_clears_readonlymode() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.readonlymode;
        unsafe { crate::globals::GLOBALS.get_mut() }.readonlymode = true;

        let mut buf = crate::buffer_defs::BufT { b_p_ro: 0, ..Default::default() };
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Readonly,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_flags: 0,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_readonly(&mut args) }, None);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.readonlymode);

        unsafe { crate::globals::GLOBALS.get_mut() }.readonlymode = prev;
    }

    #[test]
    fn did_set_readonly_reset_locally_leaves_readonlymode_alone() {
        // A :setlocal says nothing about the global intent, so
        // readonlymode must survive it.
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.readonlymode;
        unsafe { crate::globals::GLOBALS.get_mut() }.readonlymode = true;

        let mut buf = crate::buffer_defs::BufT { b_p_ro: 0, ..Default::default() };
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Readonly,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_flags: crate::option_defs::opt_set_flags::OPT_LOCAL as i32,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_readonly(&mut args) }, None);
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.readonlymode);

        unsafe { crate::globals::GLOBALS.get_mut() }.readonlymode = prev;
    }

    #[test]
    fn did_set_readonly_set_clears_the_warning_flag() {
        // Setting 'readonly' lets the W10 warning be given again.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf =
            crate::buffer_defs::BufT { b_p_ro: 1, b_did_warn: true, ..Default::default() };
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Readonly,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_readonly(&mut args) }, None);
        assert!(!buf.b_did_warn);
    }

    #[test]
    fn did_set_readonly_reset_leaves_the_warning_flag_alone() {
        // Only SETTING 'readonly' re-arms the warning; resetting it
        // must not, since there is nothing to warn about.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf =
            crate::buffer_defs::BufT { b_p_ro: 0, b_did_warn: true, ..Default::default() };
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Readonly,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_readonly(&mut args) }, None);
        assert!(buf.b_did_warn);
    }

    #[test]
    fn did_set_wrap_resets_only_the_relevant_scroll_offset() {
        // A wrapped window cannot scroll horizontally, so w_leftcol
        // goes; an unwrapped one has no partially-shown first line, so
        // w_skipcol goes. The OTHER is deliberately left alone.
        let mut win = crate::buffer_defs::WinT {
            w_leftcol: 7,
            w_skipcol: 9,
            ..crate::buffer_defs::WinT::default()
        };
        win.w_onebuf_opt.wo_wrap = 1;
        let mut args = fold_args(crate::option_defs::OptIndex::Wrap, &mut win);

        assert_eq!(unsafe { did_set_wrap(&mut args) }, None);
        assert_eq!(win.w_leftcol, 0);
        assert_eq!(win.w_skipcol, 9, "w_skipcol must be untouched when wrapping");

        let mut win = crate::buffer_defs::WinT {
            w_leftcol: 7,
            w_skipcol: 9,
            ..crate::buffer_defs::WinT::default()
        };
        win.w_onebuf_opt.wo_wrap = 0;
        let mut args = fold_args(crate::option_defs::OptIndex::Wrap, &mut win);

        assert_eq!(unsafe { did_set_wrap(&mut args) }, None);
        assert_eq!(win.w_skipcol, 0);
        assert_eq!(win.w_leftcol, 7, "w_leftcol must be untouched when not wrapping");
    }

    #[test]
    fn did_set_lisp_rebuilds_the_buffers_chartab() {
        // Changing 'lisp' includes/excludes '-' in the keyword
        // characters, which is what rebuilding b_chartab achieves.
        // A default BufT has an empty 'iskeyword', so '-' being
        // present is attributable to the lisp branch alone.
        //
        // Asserts on b_chartab directly (same bit layout set_chartab
        // writes) rather than through vim_iswordc_buf, which resolves
        // sub-0x100 characters via the GLOBAL chartab and so would not
        // observe this buffer-local change at all.
        const DASH: u32 = b'-' as u32;
        let dash_set = |buf: &crate::buffer_defs::BufT| {
            buf.b_chartab[(DASH >> 6) as usize] & (1u64 << (DASH & 0x3f)) != 0
        };

        let mut buf = crate::buffer_defs::BufT { b_p_lisp: 1, ..Default::default() };
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Lisp,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_lisp(&mut args) }, None);
        assert!(dash_set(&buf), "'lisp' must make '-' a keyword character");

        // With 'lisp' off it must NOT be added.
        let mut buf = crate::buffer_defs::BufT { b_p_lisp: 0, ..Default::default() };
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Lisp,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_lisp(&mut args) }, None);
        assert!(!dash_set(&buf));
    }

    #[test]
    fn did_set_numberwidth_zeroes_the_cached_line_count() {
        // Zeroing w_nrwidth_line_count is what forces the number
        // column's width to be recomputed on the next redraw.
        let mut win = crate::buffer_defs::WinT {
            w_nrwidth_line_count: 42,
            ..crate::buffer_defs::WinT::default()
        };
        let mut args = fold_args(crate::option_defs::OptIndex::Numberwidth, &mut win);

        assert_eq!(unsafe { did_set_numberwidth(&mut args) }, None);
        assert_eq!(win.w_nrwidth_line_count, 0);
    }

    #[test]
    fn did_set_hlsearch_clears_the_no_hlsearch_flag() {
        // Setting OR resetting 'hlsearch' clears the ":nohlsearch"
        // suppression, so it does not survive a later :set hlsearch.
        let _lock = crate::globals::global_state_test_lock();
        unsafe { crate::globals::GLOBALS.get_mut() }.Search.no_hlsearch = true;

        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Hlsearch,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_hlsearch(&mut args) }, None);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.Search.no_hlsearch);
    }

    #[test]
    fn did_set_ignorecase_only_redraws_when_hlsearch_is_on() {
        // With highlighting off there is nothing visible to update, so
        // the original skips the redraw entirely.
        let _lock = crate::globals::global_state_test_lock();
        let prev_hls = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hls;
        let prev_firstwin = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;

        let mut win = crate::buffer_defs::WinT {
            w_redr_type: 0,
            ..crate::buffer_defs::WinT::default()
        };
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = &mut win;

        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Ignorecase,
            ..Default::default()
        };

        // 'hlsearch' off: no redraw is scheduled.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hls = 0;
        assert_eq!(unsafe { did_set_ignorecase(&mut args) }, None);
        assert_eq!(win.w_redr_type, 0);

        // 'hlsearch' on: every window is marked for redraw.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hls = 1;
        assert_eq!(unsafe { did_set_ignorecase(&mut args) }, None);
        assert_eq!(win.w_redr_type, crate::drawscreen::UPD_SOME_VALID);

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hls = prev_hls;
    }

    #[test]
    fn did_set_foldminlines_invalidates_the_windows_folds() {
        let mut win = crate::buffer_defs::WinT {
            w_foldinvalid: false,
            ..crate::buffer_defs::WinT::default()
        };
        let mut args = fold_args(crate::option_defs::OptIndex::Foldminlines, &mut win);

        assert_eq!(unsafe { did_set_foldminlines(&mut args) }, None);
        assert!(win.w_foldinvalid, "foldUpdateAll must invalidate the folds");
    }

    #[test]
    fn did_set_foldnestmax_updates_for_syntax_and_indent_only() {
        // Only 'syntax' and 'indent' actually consult 'foldnestmax',
        // so every other method must leave the folds alone - the
        // original's own guard.
        for (method, expect_update) in [
            (&b"syntax"[..], true),
            (&b"indent"[..], true),
            (&b"manual"[..], false),
            (&b"marker"[..], false),
            (&b"expr"[..], false),
            (&b"diff"[..], false),
        ] {
            let mut win = crate::buffer_defs::WinT {
                w_foldinvalid: false,
                ..crate::buffer_defs::WinT::default()
            };
            win.w_onebuf_opt.wo_fdm = Some(method.to_vec());
            let mut args = fold_args(crate::option_defs::OptIndex::Foldnestmax, &mut win);

            assert_eq!(unsafe { did_set_foldnestmax(&mut args) }, None);
            assert_eq!(
                win.w_foldinvalid,
                expect_update,
                "foldmethod={} should{} update",
                String::from_utf8_lossy(method),
                if expect_update { "" } else { " NOT" }
            );
        }
    }

    #[test]
    fn did_set_foldlevel_runs_new_fold_level_against_curwin() {
        // new_fold_level reads GLOBALS.curwin, so this needs the
        // global lock, and curwin must be installed BEFORE the call.
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_fdm = Some(b"manual".to_vec());

        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win;

        let mut args = fold_args(crate::option_defs::OptIndex::Foldlevel, &mut win);
        assert_eq!(unsafe { did_set_foldlevel(&mut args) }, None);

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev;
    }

    #[test]
    fn did_set_title_is_a_no_op_when_starting_is_no_screen() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.starting;
        unsafe { crate::globals::GLOBALS.get_mut() }.starting = crate::globals::NO_SCREEN;

        // Must not panic - the real `maketitle()` branch is
        // unreachable given `starting`'s own real default.
        did_set_title();

        unsafe { crate::globals::GLOBALS.get_mut() }.starting = prev;
    }

    #[test]
    fn did_set_title_panics_if_starting_were_ever_changed() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.starting;
        unsafe { crate::globals::GLOBALS.get_mut() }.starting = 0;

        let result = std::panic::catch_unwind(did_set_title);

        unsafe { crate::globals::GLOBALS.get_mut() }.starting = prev;

        let payload = result.expect_err("expected did_set_title to panic");
        let msg = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_default();
        assert!(msg.contains("maketitle"), "unexpected panic message: {msg}");
    }
}
