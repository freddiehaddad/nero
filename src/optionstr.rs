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
//! `foldmethodIsIndent`-gated `foldUpdateAll` call is real.
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
//! Also [`did_set_foldignore`] (pure side effect, no validation -
//! `'foldignore'` accepts any value), [`did_set_foldmarker`] (the
//! value must contain a comma with at least one character on either
//! side of it) and [`did_set_foldmethod`] (delegates to
//! [`did_set_str_generic`] for the value check, then updates the
//! folds unconditionally, additionally recomputing the fold levels
//! when the new method is `"diff"`). Each drives a real
//! `foldUpdateAll` call.
//!
//! Also [`did_set_shortmess`] and [`did_set_cpoptions`] - two more
//! thin `did_set_option_listflag` wrappers. `'shortmess'` validates
//! against a new file-local `SHM_ALL` table (built from
//! `option_vars.rs`'s own `shm` module, following this module's own
//! `STL_ALL` precedent; the original's own array ends with 4 bare
//! character literals having no `SHM_*` constant, transcribed exactly
//! as-is), `'cpoptions'` against `option_vars::CPO_VI`.
//!
//! Also [`did_set_breakat`] - rebuilds `OPTION_VARS.breakat_flags`
//! (a 256-entry "is this byte a line-break character" lookup table)
//! from `'breakat'`, with no validation at all. Reads the GLOBAL
//! `p_breakat` directly rather than `args.os_varp`, exactly as the
//! original does. Its `OPTIONS` entry and `did_set_option` dispatch
//! are real now, but `charset::vim_isbreak` still uses its own fixed
//! DEFAULT-`'breakat'` table rather than reading `breakat_flags`.
//!
//! Also [`did_set_cursorlineopt`] (rejects an empty value outright,
//! then delegates to `option.rs`'s already-real `fill_culopt_flags`;
//! the original's own "could be changed to use opt_strings_flags()"
//! note is preserved as-is rather than acted on, since doing so would
//! change real behavior) and [`did_set_inccommand`] (refuses to change
//! `'inccommand'` while a command preview is already running, via
//! `GLOBALS.cmdpreview`, otherwise delegating to
//! [`did_set_str_generic`]).
//!
//! Also [`did_set_backupcopy`] - the same local-or-global flags
//! pattern as [`did_set_virtualedit`]/[`did_set_tagcase`], plus a
//! plain-`:set` branch that also clears the buffer-local flags first
//! (matching [`did_set_completeopt`]'s own shape) and an
//! "exactly one of `auto`/`yes`/`no`" constraint. On that specific
//! failure the original re-derives the flags from `args.os_oldval`
//! before returning the error - preserved here rather than leaving
//! the rejected value's own flags in place.
//!
//! Also [`did_set_spelloptions`] - unlike this file's other
//! local-or-global callbacks (which pick ONE storage slot), this one
//! writes BOTH slots from the SAME `args.os_newval` string, each
//! guarded by its own inverted flag check (the global `spo_flags`
//! unless `OPT_LOCAL`, the window's own `w_s.b_p_spo_flags` unless
//! `OPT_GLOBAL`), so a plain `:set` updates both.
//!
//! Also [`did_set_formatoptions`] (a thin
//! `did_set_option_listflag` wrapper over `option_vars::FO_ALL` -
//! which itself already contains a literal `,`, so no separator has
//! to be appended, unlike [`did_set_whichwrap`]'s own `WW_ALL`
//! handling) and [`did_set_commentstring`] (the value must be empty
//! or contain a literal `%s` placeholder).
//!
//! Also [`did_set_comments`] - a real parser for the comma-separated
//! `flags:string` list. It faithfully preserves a genuinely
//! surprising control-flow quirk: the illegal-character `break` only
//! leaves the INNER flag-scanning loop, so the following
//! "missing colon"/"zero length string" checks still run and can
//! OVERWRITE that error (`comments=z` reports `E525`, while
//! `comments=zb:x` reports the illegal-character error). Both were
//! verified against a real `nvim` binary and have dedicated
//! regression tests - preserved, not "fixed".
//!
//! Also [`did_set_breakindentopt`] - delegates to `indent.rs`'s
//! already-real `briopt_check`, which was deliberately harvested
//! ahead of this exact caller in an earlier pass. The original's own
//! `varp == &win->w_p_briopt ? win : NULL` pointer comparison (which
//! decides whether the parsed values are actually STORED into the
//! window, or merely validated) is reproduced as a real
//! `std::ptr::eq` against the window's own `wo_briopt` address. Its
//! `redraw_all_later` call is omitted - pure redraw scheduling.
//!
//! Also [`did_set_colorcolumn`] - exactly the same shape, delegating
//! to `window.rs`'s already-real `check_colorcolumn` and reproducing
//! the original's own `varp == &win->w_p_cc ? win : NULL` comparison
//! the same `std::ptr::eq` way.
//!
//! Also [`did_set_optexpr`] - the `'*expr'` family (`'diffexpr'`,
//! `'foldexpr'`, `'formatexpr'`, `'includeexpr'`, `'indentexpr'`,
//! `'patchexpr'`, `'printexpr'`, `'charconvert'`). It is the only
//! callback in this file that REPLACES the option value rather than
//! just reading it: a `<SID>`/`s:` prefix is expanded to the real
//! script identifier via `userfunc.rs`'s already-real
//! `get_scriptlocal_funcname`. Never fails.
//!
//! Also [`did_set_foldexpr`] - delegates to [`did_set_optexpr`] for
//! the `<SID>`/`s:` expansion (discarding its return value, matching
//! the original, which likewise ignores it since it never fails),
//! then updates the folds when `'foldmethod'` is `"expr"`, the same
//! shape as [`did_set_foldignore`].
//!
//! Also [`did_set_statusline`]/[`did_set_winbar`]/[`did_set_tabline`]/
//! [`did_set_statuscolumn`]/[`did_set_rulerformat`], built on a new
//! private `did_set_statustabline_rulerformat` shared helper
//! (unlocked by `check_stl_option` landing earlier in this segment;
//! the `'rulerformat'` path additionally needed `ui.rs`'s `ui_has`,
//! `globals.rs`'s `ru_wid` and `drawscreen.rs`'s `comp_col`, all of
//! which already existed). All five options in that family share one
//! body in the original. Two narrow `'statusline'` branches remain
//! deferred: resetting an empty global value needs
//! `get_option_default`, and changing a floating window needs
//! `win_config_float`. Ordinary nonempty statusline validation is
//! fully working.
//!
//! Also [`did_set_verbosefile`] - closes any currently-open verbose
//! log file and reopens it when `'verbosefile'` is non-empty, built
//! on `message.rs`'s own new `verbose_stop`/`verbose_open`.
//!
//! Also [`did_set_filetype_or_syntax`] (validates via
//! [`valid_filetype`], then records `os_value_changed`/
//! `os_value_checked` back into `args` for the option engine's own
//! later use) and [`did_set_highlight`] (`'highlight'` is not
//! configurable at all - the ONLY accepted value is the built-in
//! `option_vars::HIGHLIGHT_INIT` default, anything else is rejected
//! with `e_unsupportedoption`).
//!
//! Also [`did_set_complete`] - the comma-separated `'complete'`
//! source list. Hand-traced and then verified case-by-case against a
//! real `nvim` binary before being written, preserving three real
//! behaviours rather than tidying them up: an escaped comma consumes
//! BOTH bytes (so `"u\,"` parses as the bare entry `"u"` and is
//! VALID, while the `escape` flag it sets makes only a SUBSEQUENT
//! comma literal); spaces are not skipped during extraction, only
//! after a completed entry; and an empty entry is a genuine error
//! (a leading comma yields a NUL first byte, which `vim_strchr`'s own
//! `c <= 0` guard never matches), though a DOUBLED comma is fine.
//! Adds `LSIZE` to `tag.rs` (its real home, `tag.h`), matching the
//! original's own cross-file use of that constant as the per-entry
//! scratch-buffer bound.
//!
//! Also [`did_set_shellpipe_redir`] (`'shellpipe'`/`'shellredir'`) -
//! a shell command template in which `%s` marks the file-name
//! substitution point. At most ONE `%s` is allowed, `%%` is a
//! literal-percent escape, and any other `%`-sequence (or a trailing
//! bare `%`) is rejected. Reads `args.os_newval` rather than
//! `args.os_varp`, matching the original exactly.
//!
//! Also [`did_set_completeitemalign`] - the value must list EXACTLY
//! the three items `"abbr"`, `"kind"` and `"menu"` (in any order,
//! each exactly once). The resulting order is packed into
//! `OPTION_VARS.cia_flags` as a base-10 digit sequence via the
//! original's own `new_cia_flags * 10 + CPT_*` accumulation, kept
//! exactly rather than re-encoded as a cleaner bitmask, since real
//! consumers decode those decimal digits positionally.
//!
//! Also [`did_set_fileformat`] - refuses the change when the buffer
//! is not `'modifiable'` (unless the GLOBAL value is what's being
//! set), then delegates to [`did_set_str_generic`] and updates the
//! swap file's own flags via `memline.rs`'s already-real
//! `ml_setflags`. Its `redraw_titles`/`redraw_buf_later` calls are
//! pure redraw scheduling and are omitted - so the `'mac'`-related
//! redraw condition, which exists only to decide WHICH redraw to
//! schedule, has no observable effect here and is omitted with them.
//!
//! Also [`check_signcolumn`] and [`did_set_signcolumn`] - the
//! `WinT` field wiring that previously blocked these
//! (`w_minscwidth`/`w_maxscwidth`/`w_scwidth`, plus `SCL_NO`/
//! `SCL_NUM`/`OPT_SCL_VALUES`) all landed in the meantime, so the old
//! "deferred" note below no longer applied. `check_signcolumn`
//! follows `check_colorcolumn`/`briopt_check`'s established
//! `Option<&[u8]>`/`Option<&mut WinT>`/plain-`bool` shape. Two real
//! behaviours are preserved: `"number"` only maps to `SCL_NUM` when
//! the window actually has `'number'`/`'relativenumber'` set
//! (otherwise it falls through to the same `min=0, max=1` bare
//! `"auto"` uses), and the `auto:<MIN>-<MAX>` form is NOT in the
//! value list so it is shape-checked by hand.
//!
//! Also [`did_set_winborder`]/[`did_set_pumborder`] - both validate
//! their option by parsing it into a throwaway `WinConfig` via the
//! private `parse_border_opt` helper and the newly-translated
//! `api/win_config.rs`'s `parse_winborder`/`parse_border_style`.
//! Cross-checked against a real `nvim` binary for every accepted and
//! rejected form, which caught one non-obvious case: `,2,3,4,5,6,7,8`
//! DOES split into exactly eight parts, so it passes the length check
//! and is instead rejected by the "corner char between edge chars"
//! rule.
//!
//! Also `set_chars_option` and its `get_encoded_char_adv`/
//! `field_value_err` helpers, plus the `LCS_TAB`/`FCS_TAB` field
//! tables - the `'listchars'`/`'fillchars'` parser. The original's
//! `struct chars_tab` holds a raw `schar_T *cp` into the file-static
//! `lcs_chars`/`fcs_chars` and then compares that POINTER against
//! specific field addresses to recognise the multi-character
//! `tab`/`leadtab` entries; that becomes a `CharsField` selector here,
//! whose equality test is exactly the original's pointer equality
//! without needing raw pointers into a mutable static. The two scratch
//! structs likewise become plain locals - they are fully
//! re-initialised at the top of the storing round and only read at the
//! very end, so nothing observable depends on their being statics.
//!
//! Cross-checked field-by-field against a real `nvim` binary, which
//! pinned several things worth stating: the `'fillchars'` field is
//! spelled `foldclose` even though the struct member is `foldclosed`
//! (`foldclosed:+` is E474); invalid OR truncated hex escapes
//! (`\xZZ`, `\x`) report E1512 "wrong character width" rather than a
//! parse error, because `get_encoded_char_adv` funnels both into the
//! same `0` sentinel; and a double-width escape (`\U0001F600`) lands
//! on that same path. Note `field_value_err` returns a non-NULL EMPTY
//! string when given no error buffer, so a per-field error is still
//! reported but carries no message - `check_chars_options` depends on
//! exactly that, and it is pinned by its own test rather than papered
//! over.
//!
//! Also `check_chars_options`, `did_set_global_chars_option`,
//! [`did_set_chars_option`], [`did_set_ambiwidth`] and
//! [`did_set_emoji`] - the four callbacks `set_chars_option` unlocked.
//! `did_set_emoji` deliberately validates `'ambiwidth'` rather than
//! `'emoji'` itself, since both feed the same character-width tables.
//! The `E834`/`E835` "conflicts with value of" messages are kept
//! distinguishable so a caller can tell which of the two options was
//! at fault. `clear_string_option` is inlined as a plain assignment:
//! it exists in the original purely to free a C string and point it at
//! the shared empty one, which an owned `Option<Vec<u8>>` handles.
//!
//! `did_set_background` remains deferred - it needs `init_highlight`
//! and `do_unlet` (colorscheme reload), neither translated.
//!
//! `free_string_option`, `clear_string_option` and
//! `check_string_option` need NO Rust equivalent at all: all three
//! exist purely to manage the original's `empty_string_option`
//! sentinel pointer (a C technique for avoiding an allocation for the
//! empty string, and for telling "empty" apart from "unset"). This
//! crate models a string option as `Option<Vec<u8>>`, where `None`
//! already carries the "unset" distinction and `Vec`'s own `Drop`
//! already performs the free - see `option_vars.rs`'s own module doc.
//!
//! Deferred: everything else - the ~150 real `did_set_*`/`expand_*`
//! per-option callbacks (each needs a real `optset_T args` from an
//! actual `:set`/`set_option_value` call, per `option_defs.rs`'s own
//! `OPTIONS` doc comment) and `copy_option_part`/`skip_to_option_part`
//! (already translated in `option.rs`, not here).

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

/// The `'filetype'`/`'syntax'` option is changed
/// (`did_set_filetype_or_syntax`).
///
/// Validates the value via [`valid_filetype`], then records two flags
/// back into `args` for the option engine's own later use:
/// `os_value_changed` (whether the value genuinely differs from
/// `os_oldval`) and `os_value_checked` (always `true` - since the
/// value has been validated here, the caller need not mark it
/// insecure even when it came from a modeline).
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
pub unsafe fn did_set_filetype_or_syntax(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);

    if !valid_filetype(val) {
        return Some(crate::errors::e_invarg.as_bytes());
    }

    args.os_value_changed = match &args.os_oldval {
        crate::option_defs::OptVal::String(old) => old.as_slice() != val,
        // A non-string old value can never compare equal to the new
        // string, matching the original's own `strcmp(...) != 0`
        // against a real old string.
        _ => true,
    };

    // Since we check the value, there is no need to set
    // `kOptFlagInsecure`, even when the value comes from a modeline.
    args.os_value_checked = true;

    None
}

/// The `'verbosefile'` option is changed (`did_set_verbosefile`).
///
/// Closes any currently-open verbose log file, then reopens it when
/// `'verbosefile'` is non-empty. Built on `message.rs`'s own new
/// `verbose_stop`/`verbose_open`.
///
/// # Safety
/// Forwarded from `crate::message::verbose_open`'s own safety doc -
/// touches that module's `VERBOSE_FD`/`VERBOSE_DID_OPEN` statics and
/// `OPTION_VARS`.
pub unsafe fn did_set_verbosefile(_args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::message::verbose_stop() };

    // SAFETY: forwarded from this function's own safety doc.
    let vfile_set = !unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_vfile
        .as_deref()
        .unwrap_or(&[])
        .is_empty();

    // SAFETY: forwarded from this function's own safety doc.
    if vfile_set && unsafe { crate::message::verbose_open() } == crate::vim_defs::FAIL {
        return Some(crate::errors::e_invarg.as_bytes());
    }
    None
}

/// The `'highlight'` option is changed (`did_set_highlight`).
///
/// `'highlight'` is not configurable: the ONLY accepted value is the
/// built-in default `option_vars::HIGHLIGHT_INIT`, and anything else
/// is rejected outright.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
pub unsafe fn did_set_highlight(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);

    if val != crate::option_vars::HIGHLIGHT_INIT.as_bytes() {
        return Some(crate::errors::e_unsupportedoption.as_bytes());
    }
    None
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

/// Validate every string option that carries a flags bitmask, at
/// startup and after `:set all&` (`didset_string_options`).
///
/// Each entry is checked against its own value list, which as a side
/// effect recomputes the option's `flags_var` bitmask. Return values
/// are discarded exactly as the original does - this runs over values
/// that are already known-good defaults.
///
/// # Safety
/// Forwarded from `check_str_opt`'s own safety doc: every listed
/// option's global `.var` pointer must be live.
pub unsafe fn didset_string_options() {
    const OPTS: [crate::option_defs::OptIndex; 17] = [
        crate::option_defs::OptIndex::Casemap,
        crate::option_defs::OptIndex::Backupcopy,
        crate::option_defs::OptIndex::Belloff,
        crate::option_defs::OptIndex::Completeopt,
        crate::option_defs::OptIndex::Sessionoptions,
        crate::option_defs::OptIndex::Viewoptions,
        crate::option_defs::OptIndex::Foldopen,
        crate::option_defs::OptIndex::Display,
        crate::option_defs::OptIndex::Jumpoptions,
        crate::option_defs::OptIndex::Redrawdebug,
        crate::option_defs::OptIndex::Tagcase,
        crate::option_defs::OptIndex::Termpastefilter,
        crate::option_defs::OptIndex::Virtualedit,
        crate::option_defs::OptIndex::Switchbuf,
        crate::option_defs::OptIndex::Tabclose,
        crate::option_defs::OptIndex::Wildoptions,
        crate::option_defs::OptIndex::Clipboard,
    ];
    for opt in OPTS {
        // SAFETY: forwarded from this function's own safety doc.
        let _ = unsafe { check_str_opt(opt, None) };
    }
}

/// Resolve `val` against `values` and store the resulting flags
/// bitmask, or report `E474` (`did_set_opt_flags`).
///
/// A thin wrapper over [`opt_strings_flags`], turning its
/// success/failure into the `Option<&'static [u8]>` error shape every
/// `did_set_*` callback returns.
///
/// The original writes through an `unsigned *flagp` out-parameter and
/// returns `OK`/`FAIL`; [`opt_strings_flags`] here already returns the
/// bitmask itself, so the flags are stored only on success, exactly as
/// upstream does.
pub fn did_set_opt_flags(
    val: &[u8],
    values: &[&str],
    flagp: &mut u32,
    list: bool,
) -> Option<&'static [u8]> {
    match opt_strings_flags(val, values, list) {
        Some(flags) => {
            *flagp = flags;
            None
        }
        None => Some(crate::errors::e_invarg.as_bytes()),
    }
}

/// The `'iskeyword'` option is changed (`did_set_iskeyword`).
///
/// The GLOBAL value is only validated, never applied: it is the
/// template new buffers inherit, so there is no single `b_chartab[]`
/// to refill for it. A buffer-LOCAL value falls through to
/// [`did_set_isopt`], which does refill that buffer's table.
///
/// The original decides between the two with a `varp == &p_isk`
/// pointer comparison; that is reproduced here as a real
/// [`std::ptr::eq`] against `OPTION_VARS`' own `p_isk` address,
/// matching this module's established precedent for
/// `'briopt'`/`'colorcolumn'`.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>`, and (for the
/// local case) `args.os_buf` must be a valid, non-null pointer to a
/// live `BufT` for the whole call.
pub unsafe fn did_set_iskeyword(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let global_isk =
        std::ptr::from_mut(&mut unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_isk);

    if std::ptr::eq(args.os_varp as *const Option<Vec<u8>>, global_isk) {
        // Only check the global value.
        // SAFETY: forwarded from this function's own safety doc.
        let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::charset::check_isopt(varp.as_deref().unwrap_or_default()) }
            == crate::vim_defs::FAIL
        {
            return Some(crate::errors::e_invarg.as_bytes());
        }
        return None;
    }

    // Fallthrough for the local value.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { did_set_isopt(args) }
}

/// The `'isident'`/`'iskeyword'`/`'isprint'`/`'isfname'` option is
/// changed (`did_set_isopt`).
///
/// Refills the buffer's own `b_chartab[]`. On failure the caller is
/// asked to put the old value back, via `os_restore_chartab`.
///
/// # Safety
/// `args.os_buf` must be a valid, non-null pointer to a live `BufT`
/// for the whole call. Forwarded from
/// [`crate::charset::buf_init_chartab`]'s own safety doc.
pub unsafe fn did_set_isopt(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *(args.os_buf as *mut crate::buffer_defs::BufT) };
    // 'isident', 'iskeyword', 'isprint' or 'isfname': refill
    // b_chartab[]. If the new option is invalid, use the old value.
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::charset::buf_init_chartab(buf, true) } == crate::vim_defs::FAIL {
        args.os_restore_chartab = true; // need to restore it below
        return Some(crate::errors::e_invarg.as_bytes()); // error in value
    }
    None
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
pub fn did_set_backupext_or_patchmode(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
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
pub unsafe fn did_set_helpfile(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
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

/// Process an updated `'display'` option (`did_set_display`).
///
/// # Safety
/// Forwarded from [`did_set_str_generic`] and
/// [`crate::charset::init_chartab`].
pub unsafe fn did_set_display(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let error = unsafe { did_set_str_generic(args) };
    if error.is_some() {
        return error;
    }
    let _ = unsafe { crate::charset::init_chartab() };
    // `msg_grid_validate()` belongs to the message-grid redraw path.
    None
}

/// The `'eventignore'` or `'eventignorewin'` option is changed
/// (`did_set_eventignore`).
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>`.
pub unsafe fn did_set_eventignore(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let win = args.os_idx == crate::option_defs::OptIndex::Eventignorewin;
    if crate::autocmd::check_ei(varp.as_deref().unwrap_or(&[]), win) {
        None
    } else {
        Some(crate::errors::e_invarg.as_bytes())
    }
}

/// The `'diffopt'` option is changed (`did_set_diffopt`).
pub fn did_set_diffopt(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let value = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_dip
        .clone()
        .unwrap_or_default();
    if unsafe { crate::diff::diffopt_changed(&value) } {
        None
    } else {
        Some(crate::errors::e_invarg.as_bytes())
    }
}

/// One of `'encoding'`, `'fileencoding'` or `'makeencoding'` changed
/// (`did_set_encoding`).
///
/// Redrawing titles, updating swap-file flags and reloading spell
/// data are deferred with those subsystems; validation and stored
/// value canonicalization are complete.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>`.
/// `args.os_buf` must point to a live `BufT` for `'fileencoding'`.
pub unsafe fn did_set_encoding(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let varp = unsafe { &mut *(args.os_varp as *mut Option<Vec<u8>>) };
    if args.os_idx == crate::option_defs::OptIndex::Fileencoding {
        let buf = unsafe { &*(args.os_buf as *const crate::buffer_defs::BufT) };
        if buf.b_p_ma == 0
            && args.os_flags as u32 & crate::option_defs::opt_set_flags::OPT_GLOBAL == 0
        {
            return Some(crate::errors::e_modifiable.as_bytes());
        }
        if varp.as_deref().unwrap_or(&[]).contains(&b',') {
            return Some(crate::errors::e_invarg.as_bytes());
        }
    }

    *varp = Some(crate::mbyte::enc_canonize(
        varp.as_deref().unwrap_or(&[]),
    ));
    if args.os_idx == crate::option_defs::OptIndex::Encoding
        && varp.as_deref() != Some(&b"utf-8"[..])
    {
        return Some(crate::errors::e_unsupportedoption.as_bytes());
    }
    None
}

/// Expand canonical encoding names (`expand_set_encoding`).
///
/// Returning all names when no regex filter is supplied is complete.
/// Filtering through `oe_regmatch` still needs the real regexp engine.
pub fn expand_set_encoding(
    args: &mut crate::option_defs::OptexpandT,
) -> Option<Vec<Vec<u8>>> {
    if !args.oe_regmatch.is_null() {
        unimplemented!("expand_set_encoding: regex filtering needs the regexp engine");
    }
    Some(
        crate::mbyte::ENC_CANON_TABLE
            .iter()
            .map(|entry| entry.name.as_bytes().to_vec())
            .collect(),
    )
}

/// Expand an option's fixed string-value list
/// (`expand_set_str_generic`).
///
/// Regex filtering remains deferred with the regexp engine.
pub fn expand_set_str_generic(
    args: &mut crate::option_defs::OptexpandT,
) -> Option<Vec<Vec<u8>>> {
    if !args.oe_regmatch.is_null() {
        unimplemented!("expand_set_str_generic: regex filtering needs the regexp engine");
    }
    let values_idx = match args.oe_idx {
        crate::option_defs::OptIndex::Viewoptions => {
            crate::option_defs::OptIndex::Sessionoptions
        }
        crate::option_defs::OptIndex::Fileformats => {
            crate::option_defs::OptIndex::Fileformat
        }
        idx => idx,
    };
    Some(
        crate::option::get_option(values_idx)
            .values
            .iter()
            .map(|value| value.as_bytes().to_vec())
            .collect(),
    )
}

/// Expand an option whose value is a list of one-byte flags
/// (`expand_set_opt_listflag`).
fn expand_set_opt_listflag(
    args: &crate::option_defs::OptexpandT,
    flags: &[u8],
) -> Option<Vec<Vec<u8>>> {
    let option_val = args.oe_opt_value.as_deref().unwrap_or(&[]);
    let cmdline_val = args.oe_set_arg.as_deref().unwrap_or(&[]);
    let include_orig_val = args.oe_include_orig_val && !option_val.is_empty();
    let mut matches = Vec::with_capacity(flags.len() + usize::from(include_orig_val));

    if include_orig_val {
        matches.push(option_val.to_vec());
    }

    for &flag in flags {
        if args.oe_append && option_val.contains(&flag) {
            continue;
        }
        if !cmdline_val.contains(&flag) {
            if include_orig_val && option_val.len() == 1 && flag == option_val[0] {
                continue;
            }
            matches.push(vec![flag]);
        }
    }

    (!matches.is_empty()).then_some(matches)
}

/// Expand `'concealcursor'` flag values (`expand_set_concealcursor`).
pub fn expand_set_concealcursor(
    args: &mut crate::option_defs::OptexpandT,
) -> Option<Vec<Vec<u8>>> {
    expand_set_opt_listflag(args, crate::option_vars::COCU_ALL.as_bytes())
}

/// Process an updated `'messagesopt'` value
/// (`did_set_messagesopt`).
pub fn did_set_messagesopt(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let value = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_mopt
        .clone()
        .unwrap_or_default();
    if crate::message::messagesopt_changed(&value) {
        None
    } else {
        Some(crate::errors::e_invarg.as_bytes())
    }
}

/// The `'cinoptions'` option is changed (`did_set_cinoptions`).
///
/// # Safety
/// `args.os_buf` must point to a live `BufT`.
pub unsafe fn did_set_cinoptions(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let buf = unsafe { &mut *(args.os_buf as *mut crate::buffer_defs::BufT) };
    crate::indent_c::parse_cino(buf);
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
pub fn did_set_helplang(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
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
pub unsafe fn did_set_spellsuggest(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
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
pub unsafe fn did_set_mkspellmem(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
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
pub unsafe fn did_set_mousescroll(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
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
pub unsafe fn did_set_wildmode(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
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
/// check (`foldmethodIsIndent`) gating a real `foldUpdateAll` call.
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
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    crate::fold::fold_update_all(args.os_win as *mut crate::buffer_defs::WinT)
                };
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

/// Error message for `'foldmarker'` given a value with no comma
/// (`e_comma_required`, a file-local `static const char[]` in the
/// original - kept file-local here too, matching the original's own
/// scoping, rather than added to the shared `errors.rs`; same
/// precedent as `option.rs`'s own
/// `e_cannot_have_negative_or_zero_number_of_quickfix`).
#[allow(non_upper_case_globals)]
const e_comma_required: &str = crate::gettext_defs::gettext_noop("E536: Comma required");

/// Error message for a `'listchars'`/`'fillchars'` field given the
/// wrong number of characters (`e_wrong_number_of_characters_for_field_str`,
/// a file-local `static const char[]` in the original - kept
/// file-local here too, same precedent as `e_comma_required`).
///
/// The original formats the offending field's own name into the `%s`
/// via `field_value_err`/`vim_vsnprintf`; per this module's own
/// established policy for dynamically-formatted messages, the
/// placeholder is left unformatted here. The message is still worth
/// reproducing as its own constant because it is genuinely
/// distinguishable from `e_invarg`, so tests can tell the two
/// failure paths apart.
#[allow(non_upper_case_globals)]
const e_wrong_number_of_characters_for_field_str: &str =
    crate::gettext_defs::gettext_noop("E1511: Wrong number of characters for field \"%s\"");

/// Error message for a `'listchars'`/`'fillchars'` field whose
/// character is the wrong width - i.e. double-width, which the
/// original notes is forbidden, apparently a TUI limitation
/// (`e_wrong_character_width_for_field_str`). Same file-local scoping
/// and same unformatted-`%s` treatment as
/// `e_wrong_number_of_characters_for_field_str` above.
#[allow(non_upper_case_globals)]
const e_wrong_character_width_for_field_str: &str =
    crate::gettext_defs::gettext_noop("E1512: Wrong character width for field \"%s\"");

/// All `'shortmess'` flag characters (`SHM_ALL`, a file-local
/// `static char[]` in the original - kept file-local here too,
/// matching the original's own scoping, following this module's own
/// `STL_ALL` precedent).
///
/// The original's own array ends with 4 bare character literals
/// (`'n'`, `'f'`, `'x'`, `'i'`) that have no `SHM_*` constant of
/// their own - transcribed exactly as-is here, since inventing
/// constants for them would drift from the original.
const SHM_ALL: &[u8] = &[
    crate::option_vars::shm::RO,
    crate::option_vars::shm::MOD,
    crate::option_vars::shm::LINES,
    crate::option_vars::shm::WRI,
    crate::option_vars::shm::ABBREVIATIONS,
    crate::option_vars::shm::WRITE,
    crate::option_vars::shm::TRUNC,
    crate::option_vars::shm::TRUNCALL,
    crate::option_vars::shm::OVER,
    crate::option_vars::shm::OVERALL,
    crate::option_vars::shm::SEARCH,
    crate::option_vars::shm::ATTENTION,
    crate::option_vars::shm::INTRO,
    crate::option_vars::shm::COMPLETIONMENU,
    crate::option_vars::shm::COMPLETIONSCAN,
    crate::option_vars::shm::RECORDING,
    crate::option_vars::shm::FILEINFO,
    crate::option_vars::shm::SEARCHCOUNT,
    crate::option_vars::shm::UNDO,
    // Bare literals in the original's own array, with no SHM_*
    // constant of their own.
    b'n',
    b'f',
    b'x',
    b'i',
];

/// The `'shortmess'` option is changed (`did_set_shortmess`).
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
pub unsafe fn did_set_shortmess(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);
    did_set_option_listflag(val, SHM_ALL)
}

/// The `'cpoptions'` option is changed (`did_set_cpoptions`).
///
/// Validates against `option_vars::CPO_VI` - the full set of Vi
/// compatibility flags, which is also the option's own default value.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
pub unsafe fn did_set_cpoptions(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);
    did_set_option_listflag(val, crate::option_vars::CPO_VI.as_bytes())
}

/// The `'cursorlineopt'` option is changed (`did_set_cursorlineopt`).
///
/// Rejects an empty value outright, then delegates to the already-real
/// `option::fill_culopt_flags` (which parses the comma-separated flag
/// list into `WinT.w_p_culopt_flags`). The original carries a
/// `// This could be changed to use opt_strings_flags() instead.`
/// note - preserved as-is here rather than acted on, since doing so
/// would change real behavior.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call. `args.os_varp` must point to a live
/// `Option<Vec<u8>>`.
pub unsafe fn did_set_cursorlineopt(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &mut *(args.os_win as *mut crate::buffer_defs::WinT) };
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);

    if val.is_empty() || crate::option::fill_culopt_flags(Some(val), win) != crate::vim_defs::OK {
        return Some(crate::errors::e_invarg.as_bytes());
    }

    None
}

/// The `'inccommand'` option is changed (`did_set_inccommand`).
///
/// Refuses to change `'inccommand'` while a command preview is
/// already running (`GLOBALS.cmdpreview`), otherwise delegates to
/// [`did_set_str_generic`].
///
/// # Safety
/// Forwarded from [`did_set_str_generic`]'s own safety doc.
pub unsafe fn did_set_inccommand(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: a plain `bool` copy-out read, no aliasing hazard.
    if unsafe { crate::globals::GLOBALS.get_mut() }.cmdpreview {
        return Some(crate::errors::e_invarg.as_bytes());
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { did_set_str_generic(args) }
}

/// The `'backupcopy'` option is changed (`did_set_backupcopy`).
///
/// Same "resolve from local or global storage as an owned copy"
/// pattern as [`did_set_virtualedit`]/[`did_set_tagcase`], plus two
/// extra behaviours the others don't have:
///
/// - a plain `:set` (neither `OPT_LOCAL` nor `OPT_GLOBAL`) also
///   clears the buffer-local flags first, matching
///   [`did_set_completeopt`]'s own already-established branch shape;
/// - the resulting flags must contain EXACTLY ONE of `"auto"`,
///   `"yes"` and `"no"`. On that specific failure the original
///   re-derives the flags from `args.os_oldval` (restoring the
///   previous value's own bitmask) before returning the error -
///   preserved here rather than simply leaving the rejected value's
///   flags in place.
///
/// # Safety
/// `args.os_buf` must be a valid, non-null pointer to a live `BufT`
/// for the whole call.
pub unsafe fn did_set_backupcopy(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let buf_ptr = args.os_buf as *mut crate::buffer_defs::BufT;
    let opt_flags = args.os_flags as u32;
    let use_local = opt_flags & crate::option_defs::opt_set_flags::OPT_LOCAL != 0;

    let bkc: Vec<u8> = if use_local {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*buf_ptr }.b_p_bkc.clone().unwrap_or_default()
    } else {
        if opt_flags & crate::option_defs::opt_set_flags::OPT_GLOBAL == 0 {
            // When using `:set`, clear the local flags.
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &mut *buf_ptr }.b_bkc_flags = 0;
        }
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_bkc.clone().unwrap_or_default()
    };

    if use_local && bkc.is_empty() {
        // make the local value empty: use the global value
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { bkc_store(buf_ptr, use_local, 0) };
        return None;
    }

    let Some(new_flags) = opt_strings_flags(&bkc, crate::option_vars::OPT_BKC_VALUES, true) else {
        return Some(crate::errors::e_invarg.as_bytes());
    };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { bkc_store(buf_ptr, use_local, new_flags) };

    let exclusive_count = u32::from(new_flags & crate::option_vars::opt_bkc_flag::AUTO != 0)
        + u32::from(new_flags & crate::option_vars::opt_bkc_flag::YES != 0)
        + u32::from(new_flags & crate::option_vars::opt_bkc_flag::NO != 0);
    if exclusive_count != 1 {
        // Must have exactly one of "auto", "yes" and "no". Restore the
        // flags the PREVIOUS value implied, matching the original's
        // own `opt_strings_flags(oldval, ...)` re-derivation (which
        // likewise ignores its own return value - a malformed oldval
        // simply leaves the flags as that partial parse left them).
        if let crate::option_defs::OptVal::String(oldval) = &args.os_oldval
            && let Some(old_flags) =
                opt_strings_flags(oldval, crate::option_vars::OPT_BKC_VALUES, true)
        {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { bkc_store(buf_ptr, use_local, old_flags) };
        }
        return Some(crate::errors::e_invarg.as_bytes());
    }

    None
}

/// Writes `value` to whichever `'backupcopy'` flags slot
/// [`did_set_backupcopy`] selected.
///
/// # Safety
/// `buf_ptr` must be a valid, non-null pointer to a live `BufT`
/// whenever `use_local` is true.
unsafe fn bkc_store(buf_ptr: *mut crate::buffer_defs::BufT, use_local: bool, value: u32) {
    if use_local {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *buf_ptr }.b_bkc_flags = value;
    } else {
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.bkc_flags = value;
    }
}

/// The `'spellfile'` option is changed (`did_set_spellfile`).
///
/// The value is validated in full. Reloading active spell wordlists
/// remains with the not-yet-translated `did_set_spell_option` path.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>`.
pub unsafe fn did_set_spellfile(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    if crate::spell::valid_spellfile(varp.as_deref().unwrap_or(&[])) {
        None
    } else {
        Some(crate::errors::e_invarg.as_bytes())
    }
}

/// The `'spelllang'` option is changed (`did_set_spelllang`).
///
/// The value is validated in full. Reloading active spell wordlists
/// remains with the not-yet-translated `did_set_spell_option` path.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>`.
pub unsafe fn did_set_spelllang(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    if crate::spell::valid_spelllang(varp.as_deref().unwrap_or(&[])) {
        None
    } else {
        Some(crate::errors::e_invarg.as_bytes())
    }
}

/// The `'shada'` option is changed (`did_set_shada`).
///
/// The original writes a more specific diagnostic into `os_errbuf`;
/// message text is not modeled here, so every invalid form returns
/// `e_invarg` while preserving the exact accepted/rejected grammar.
pub fn did_set_shada(_args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let shada = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_shada
        .clone()
        .unwrap_or_default();
    let mut pos = 0;

    while pos < shada.len() {
        let kind = shada[pos];
        if !b"!\"%'/:<@cfhnrs".contains(&kind) {
            return Some(crate::errors::e_invarg.as_bytes());
        }
        pos += 1;

        if kind == b'n' {
            break;
        } else if kind == b'r' {
            while pos < shada.len() && shada[pos] != b',' {
                pos += 1;
            }
        } else if kind == b'%' {
            while pos < shada.len() && shada[pos].is_ascii_digit() {
                pos += 1;
            }
        } else if matches!(kind, b'!' | b'h' | b'c') {
            // No value follows these boolean parameters.
        } else {
            let digits_start = pos;
            while pos < shada.len() && shada[pos].is_ascii_digit() {
                pos += 1;
            }
            if pos == digits_start {
                return Some(crate::errors::e_invarg.as_bytes());
            }
        }

        if pos < shada.len() {
            if shada[pos] != b',' {
                return Some(crate::errors::e_invarg.as_bytes());
            }
            pos += 1;
        }
    }

    if !shada.is_empty() && crate::shada::get_shada_parameter(&shada, b'\'') < 0 {
        Some(crate::errors::e_invarg.as_bytes())
    } else {
        None
    }
}

/// The `'spelloptions'` option is changed (`did_set_spelloptions`).
///
/// Unlike this file's other local-or-global callbacks (which pick ONE
/// storage slot), this one writes BOTH slots from the SAME
/// `args.os_newval` string, each guarded by its own inverted flag
/// check: the global `OPTION_VARS.spo_flags` unless `OPT_LOCAL`, and
/// the window's own `w_s.b_p_spo_flags` unless `OPT_GLOBAL`. A plain
/// `:set` (neither flag) therefore updates both.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// whose own `w_s` is also a valid, non-null pointer to a live
/// `SynblockT` - but only when `OPT_GLOBAL` is NOT set (the original
/// likewise only dereferences `win->w_s` on that branch).
pub unsafe fn did_set_spelloptions(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let opt_flags = args.os_flags as u32;
    let val: &[u8] = match &args.os_newval {
        crate::option_defs::OptVal::String(s) => s,
        _ => &[],
    };

    if opt_flags & crate::option_defs::opt_set_flags::OPT_LOCAL == 0 {
        let Some(flags) = opt_strings_flags(val, crate::option_vars::OPT_SPO_VALUES, true) else {
            return Some(crate::errors::e_invarg.as_bytes());
        };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.spo_flags = flags;
    }

    if opt_flags & crate::option_defs::opt_set_flags::OPT_GLOBAL == 0 {
        let Some(flags) = opt_strings_flags(val, crate::option_vars::OPT_SPO_VALUES, true) else {
            return Some(crate::errors::e_invarg.as_bytes());
        };
        // SAFETY: forwarded from this function's own safety doc.
        let synblock = unsafe { &mut *(*(args.os_win as *mut crate::buffer_defs::WinT)).w_s };
        synblock.b_p_spo_flags = flags;
    }

    None
}

/// The `'formatoptions'` option is changed (`did_set_formatoptions`).
///
/// A thin `did_set_option_listflag` wrapper over
/// `option_vars::FO_ALL`. Note `FO_ALL` itself contains a literal
/// `,` character, so no separator has to be appended here (unlike
/// [`did_set_whichwrap`]'s own `WW_ALL` handling).
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
pub unsafe fn did_set_formatoptions(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);
    did_set_option_listflag(val, crate::option_vars::FO_ALL.as_bytes())
}

/// The `'guicursor'` option is changed (`did_set_guicursor`).
///
/// Visual-mode line redraw scheduling is omitted; parsing and shape
/// table updates are handled by the real cursor-shape parser.
pub fn did_set_guicursor(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    crate::cursor_shape::parse_shape_opt(crate::cursor_shape::SHAPE_CURSOR)
}

/// The `'commentstring'` option is changed (`did_set_commentstring`).
///
/// The value must be empty, or contain a literal `%s` placeholder
/// somewhere.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
pub unsafe fn did_set_commentstring(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);

    if !val.is_empty() && !val.windows(2).any(|w| w == b"%s") {
        return Some(
            crate::gettext_defs::gettext_noop(
                "E537: 'commentstring' must be empty or contain %s",
            )
            .as_bytes(),
        );
    }
    None
}

/// Error message for an entry with trailing junk after a flag
/// character (`e_illegal_character_after_chr`, a file-local
/// `static const char[]` in the original - kept file-local here too,
/// following this module's own `e_comma_required` precedent).
///
/// The original formats the offending character into the message's
/// own `%c` placeholder via `vim_snprintf`; this crate has no
/// dynamic-message infrastructure wired up for this callback yet, so
/// the placeholder is returned unformatted. The DISPLAYED text
/// therefore differs from the original, but the message IDENTITY is
/// preserved - which is what matters here, since it is genuinely
/// distinguishable from the `e_invarg` returned by this same
/// function's other failure path.
#[allow(non_upper_case_globals)]
const e_illegal_character_after_chr: &str =
    crate::gettext_defs::gettext_noop("E535: Illegal character after <%c>");

/// Valid `'complete'` flag characters (the original's own inline
/// `".wbuksid]tUfFo"` literal).
const CPT_ALL: &[u8] = b".wbuksid]tUfFo";

/// `'complete'` flags that accept arbitrary trailing text (a file
/// name, a pattern, a function name) rather than only an optional
/// `^`-prefixed count - the original's own inline `"ksF"` literal.
const CPT_TAKES_ARGUMENT: &[u8] = b"ksF";

/// The `'complete'` option is changed (`did_set_complete`).
///
/// `'complete'` is a comma-separated list of one-character flags,
/// each optionally followed by `^` plus a decimal count - except
/// `k`/`s`/`F`, which instead take arbitrary trailing text (a file
/// name, a pattern, or a function name).
///
/// Hand-traced against the original and then verified case-by-case
/// against a real `nvim` binary before being written. Three real
/// behaviours are preserved deliberately rather than tidied up:
///
/// - **An escaped comma consumes BOTH bytes.** `\,` contributes
///   nothing at all to the extracted entry (the original advances
///   past the `\` inside the `if`, then past the `,` via the loop's
///   own increment), so `"u\,"` is VALID - it parses as the bare
///   entry `"u"`. The `escape` flag it sets only makes a
///   SUBSEQUENT comma literal, so `"u\,,x"` parses as `"u,x"` and is
///   rejected. Both cases are covered by dedicated tests.
/// - **Spaces are not skipped during extraction**, only after a
///   completed entry - so `". , w"` extracts `". "` and is rejected
///   for the trailing space.
/// - **An empty entry is an error, not a no-op.** A leading comma
///   yields an empty buffer, whose first byte the original reads as
///   its own NUL terminator; `vim_strchr` has a deliberate
///   `if (c <= 0) return NULL;` guard, so that NUL is reported as an
///   illegal character (`",w"` is genuinely invalid). A DOUBLED comma
///   is fine, because the end-of-entry skip loop consumes runs.
///
/// The `char_before != NUL` path returns `None` (success) when
/// `args.os_errbuf` is absent, exactly as the original does - an
/// asymmetry with its other failure path, which reports an error
/// either way.
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
pub unsafe fn did_set_complete(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);
    let len = val.len();

    let mut char_before: u8 = 0;
    let mut p = 0usize;

    while p < len {
        let mut buffer: Vec<u8> = Vec::new();
        let mut escape = false;

        // Extract one entry, handling escaped commas.
        while p < len
            && (val[p] != b',' || escape)
            && buffer.len() < crate::tag::LSIZE - 1
        {
            if val[p] == b'\\' && val.get(p + 1) == Some(&b',') {
                escape = true;
                p += 1; // skip the backslash; the loop skips the comma
            } else {
                escape = false;
                buffer.push(val[p]);
            }
            p += 1;
        }

        // An empty entry reads as the original's own NUL terminator,
        // which `vim_strchr` never matches - see this function's own
        // doc comment.
        let first = buffer.first().copied().unwrap_or(0);
        if !CPT_ALL.contains(&first) {
            return Some(crate::errors::e_invarg.as_bytes());
        }

        if !CPT_TAKES_ARGUMENT.contains(&first) && buffer.get(1).is_some_and(|&c| c != b'^') {
            char_before = first;
        } else if let Some(caret) = buffer.iter().position(|&c| c == b'^') {
            // Everything after the first '^' must be a non-empty run
            // of decimal digits.
            let rest = &buffer[caret + 1..];
            if rest.is_empty()
                || !rest.iter().all(|&c| crate::ascii_defs::ascii_isdigit(i32::from(c)))
            {
                char_before = b'^';
            }
        }

        if char_before != 0 {
            if args.os_errbuf.is_some() {
                return Some(e_illegal_character_after_chr.as_bytes());
            }
            return None;
        }

        // Skip the entry separator plus any following spaces.
        while p < len && (val[p] == b',' || val[p] == b' ') {
            p += 1;
        }
    }

    if unsafe { crate::insexpand::set_cpt_callbacks(args) }
        != crate::vim_defs::OK
    {
        Some(e_illegal_character_after_chr.as_bytes())
    } else {
        None
    }
}

/// Check `scl` as a `'signcolumn'` value and update `wp`'s own
/// `w_minscwidth`/`w_maxscwidth`/`w_scwidth` (`check_signcolumn`).
///
/// `scl`, when `Some`, overrides reading `wp`'s own
/// `w_onebuf_opt.wo_scl` directly - matching the original's own "use
/// `scl` if given, else fall back to `wp->w_p_scl`, else the empty
/// string" 3-way fallback, the same shape `check_colorcolumn`/
/// `briopt_check` already established. Returns `false` for an
/// invalid value (the original's own `FAIL`), matching those two
/// functions' plain-`bool` precedent for this exact "valid or not"
/// shape. `wp`'s fields are only touched when `wp` is `Some`.
///
/// An empty value is invalid, unlike most options in this file.
///
/// Accepts either one of the 22 literal `OPT_SCL_VALUES` entries, or
/// the separate `auto:<MIN>-<MAX>` form (which that list does NOT
/// contain, so it is validated by hand - exactly 8 bytes, two single
/// digits around a `-`, with `1 <= MIN < MAX` and `MAX >= 2`).
///
/// Two real behaviours worth noting, both preserved:
/// - `"number"` only maps to `SCL_NUM` when the window actually has
///   `'number'` or `'relativenumber'` set; otherwise it falls all the
///   way through to the same `min=0, max=1` the bare `"auto"` uses.
/// - `"yes:<N>"`/`"auto:<N>"` read their digit positionally without
///   re-validating it, which is safe precisely because
///   `opt_strings_flags` already matched the whole value against the
///   fixed list.
#[must_use]
pub fn check_signcolumn(scl: Option<&[u8]>, wp: Option<&mut crate::buffer_defs::WinT>) -> bool {
    let owned;
    let val: &[u8] = if let Some(scl) = scl {
        scl
    } else if let Some(ref w) = wp {
        owned = w.w_onebuf_opt.wo_scl.clone().unwrap_or_default();
        &owned
    } else {
        &[]
    };

    if val.is_empty() {
        return false;
    }

    let listed = opt_strings_flags(val, crate::option_vars::OPT_SCL_VALUES, false).is_some();

    // The `auto:<MIN>-<MAX>` form is not in the value list, so it is
    // shape-checked by hand. Done BEFORE the `wp` check so an invalid
    // value is rejected even when only validating.
    let mut range: Option<(i32, i32)> = None;
    if !listed {
        if !(val.starts_with(b"auto:")
            && val.len() == 8
            && crate::ascii_defs::ascii_isdigit(i32::from(val[5]))
            && val[6] == b'-'
            && crate::ascii_defs::ascii_isdigit(i32::from(val[7])))
        {
            return false;
        }
        let min = i32::from(val[5] - b'0');
        let max = i32::from(val[7] - b'0');
        if min < 1 || max < 2 || min > 8 || min >= max {
            return false;
        }
        range = Some((min, max));
    }

    let Some(w) = wp else {
        return true;
    };

    if let Some((min, max)) = range {
        w.w_minscwidth = min;
        w.w_maxscwidth = max;
    } else if val.starts_with(b"no") {
        w.w_minscwidth = crate::option_vars::SCL_NO;
        w.w_maxscwidth = crate::option_vars::SCL_NO;
    } else if val.starts_with(b"nu")
        && (w.w_onebuf_opt.wo_nu != 0 || w.w_onebuf_opt.wo_rnu != 0)
    {
        w.w_minscwidth = crate::option_vars::SCL_NUM;
        w.w_maxscwidth = crate::option_vars::SCL_NUM;
    } else if val.starts_with(b"yes:") {
        let n = i32::from(val[4] - b'0');
        w.w_minscwidth = n;
        w.w_maxscwidth = n;
    } else if val.first() == Some(&b'y') {
        w.w_minscwidth = 1;
        w.w_maxscwidth = 1;
    } else if val.starts_with(b"auto:") {
        w.w_minscwidth = 0;
        w.w_maxscwidth = i32::from(val[5] - b'0');
    } else {
        // Bare "auto" - and also "number" on a window with neither
        // 'number' nor 'relativenumber' set.
        w.w_minscwidth = 0;
        w.w_maxscwidth = 1;
    }

    let scwidth = if w.w_minscwidth <= 0 { 0 } else { w.w_maxscwidth.min(w.w_scwidth) };
    w.w_scwidth = w.w_minscwidth.max(scwidth);
    true
}

/// The `'signcolumn'` option is changed (`did_set_signcolumn`).
///
/// Delegates to [`check_signcolumn`], passing the window only when
/// the value being set IS the window-local `'signcolumn'` storage -
/// the same `std::ptr::eq` reproduction of the original's own
/// `varp == &win->w_p_scl ? win : NULL` comparison already used by
/// [`did_set_breakindentopt`]/[`did_set_colorcolumn`].
///
/// Then resets the number column's cached width when switching TO or
/// FROM `"number"`. Note the "from" half reads `args.os_oldval`'s own
/// first two bytes, so it also fires for any other old value starting
/// with `nu` - faithfully preserved.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call. `args.os_varp` must point to a live
/// `Option<Vec<u8>>`.
pub unsafe fn did_set_signcolumn(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;
    let varp = args.os_varp as *const Option<Vec<u8>>;

    // SAFETY: forwarded from this function's own safety doc.
    let is_window_local =
        std::ptr::eq(varp, unsafe { std::ptr::addr_of!((*win_ptr).w_onebuf_opt.wo_scl) });

    // SAFETY: forwarded from this function's own safety doc.
    let val: Option<Vec<u8>> = unsafe { &*varp }.clone();

    // SAFETY: forwarded from this function's own safety doc.
    let ok = if is_window_local {
        check_signcolumn(val.as_deref(), Some(unsafe { &mut *win_ptr }))
    } else {
        check_signcolumn(val.as_deref(), None)
    };
    if !ok {
        return Some(crate::errors::e_invarg.as_bytes());
    }

    let oldval_is_nu = match &args.os_oldval {
        crate::option_defs::OptVal::String(old) => old.starts_with(b"nu"),
        _ => false,
    };
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *win_ptr };
    if oldval_is_nu || w.w_minscwidth == crate::option_vars::SCL_NUM {
        w.w_nrwidth_line_count = 0;
    }

    None
}

/// The `'breakindentopt'` option is changed (`did_set_breakindentopt`).
///
/// Delegates to `indent.rs`'s already-real `briopt_check`, which was
/// deliberately harvested ahead of this exact caller in an earlier
/// pass. `briopt_check`'s own `wp` parameter is `Some` only when the
/// value being set IS the window-local `'breakindentopt'` storage
/// (so the parsed values are actually stored into the window),
/// matching the original's own `varp == &win->w_p_briopt ? win : NULL`
/// pointer comparison - reproduced here as a real
/// `std::ptr::eq` against the window's own `wo_briopt` address.
///
/// The original's own `redraw_all_later(UPD_NOT_VALID)` call for the
/// `'list'` sub-option is omitted - pure redraw scheduling, matching
/// this crate's established policy.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call. `args.os_varp` must point to a live
/// `Option<Vec<u8>>`.
pub unsafe fn did_set_breakindentopt(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;
    let varp = args.os_varp as *const Option<Vec<u8>>;

    // SAFETY: forwarded from this function's own safety doc.
    let is_window_local =
        std::ptr::eq(varp, unsafe { std::ptr::addr_of!((*win_ptr).w_onebuf_opt.wo_briopt) });

    // SAFETY: forwarded from this function's own safety doc.
    let val: Option<Vec<u8>> = unsafe { &*varp }.clone();

    let ok = if is_window_local {
        // SAFETY: forwarded from this function's own safety doc.
        crate::indent::briopt_check(val.as_deref(), Some(unsafe { &mut *win_ptr }))
    } else {
        crate::indent::briopt_check(val.as_deref(), None)
    };

    if !ok {
        return Some(crate::errors::e_invarg.as_bytes());
    }

    None
}

/// The `'colorcolumn'` option is changed (`did_set_colorcolumn`).
///
/// Exactly the same shape as [`did_set_breakindentopt`]: delegates to
/// `window.rs`'s already-real `check_colorcolumn`, passing the window
/// only when the value being set IS the window-local `'colorcolumn'`
/// storage (so the parsed columns are actually stored), matching the
/// original's own `varp == &win->w_p_cc ? win : NULL` pointer
/// comparison via a real `std::ptr::eq`.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// whose own `w_buffer` is either null or a valid, live `BufT`
/// pointer (forwarded to `check_colorcolumn`'s own safety doc).
/// `args.os_varp` must point to a live `Option<Vec<u8>>`.
pub unsafe fn did_set_colorcolumn(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;
    let varp = args.os_varp as *const Option<Vec<u8>>;

    // SAFETY: forwarded from this function's own safety doc.
    let is_window_local =
        std::ptr::eq(varp, unsafe { std::ptr::addr_of!((*win_ptr).w_onebuf_opt.wo_cc) });

    // SAFETY: forwarded from this function's own safety doc.
    let val: Option<Vec<u8>> = unsafe { &*varp }.clone();

    // SAFETY: forwarded from this function's own safety doc.
    let ok = unsafe {
        if is_window_local {
            crate::window::check_colorcolumn(val.as_deref(), Some(&mut *win_ptr))
        } else {
            crate::window::check_colorcolumn(val.as_deref(), None)
        }
    };

    if !ok {
        return Some(crate::errors::e_invarg.as_bytes());
    }

    None
}

/// The `'fileformat'` option is changed (`did_set_fileformat`).
///
/// Refuses the change when the buffer is not `'modifiable'`, unless
/// the GLOBAL value is what's being set, then delegates to
/// [`did_set_str_generic`] for the actual value validation and
/// updates the swap file's own flags via `memline.rs`'s already-real
/// `ml_setflags`.
///
/// The original's `redraw_titles`/`redraw_buf_later` calls are pure
/// redraw scheduling and are omitted, matching this crate's
/// established policy - so the `'mac'`-related redraw condition (which
/// exists only to decide WHICH redraw to schedule) has no observable
/// effect here and is omitted with it.
///
/// # Safety
/// `args.os_buf` must be a valid, non-null pointer to a live `BufT`
/// for the whole call.
pub unsafe fn did_set_fileformat(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let buf_ptr = args.os_buf as *mut crate::buffer_defs::BufT;
    let opt_flags = args.os_flags as u32;

    // SAFETY: forwarded from this function's own safety doc.
    let modifiable = unsafe { &*buf_ptr }.b_p_ma != 0;
    if !modifiable && opt_flags & crate::option_defs::opt_set_flags::OPT_GLOBAL == 0 {
        return Some(crate::errors::e_modifiable.as_bytes());
    }

    // SAFETY: forwarded from this function's own safety doc.
    if let Some(errmsg) = unsafe { did_set_str_generic(args) } {
        return Some(errmsg);
    }

    // Update the flag in the swap file.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::ml_setflags(&mut *buf_ptr) };

    None
}

/// The `'completeitemalign'` option is changed
/// (`did_set_completeitemalign`).
///
/// The value must list EXACTLY the three items `"abbr"`, `"kind"`
/// and `"menu"` (in any order, each exactly once) as a
/// comma-separated list. The resulting order is packed into
/// `OPTION_VARS.cia_flags` as a base-10 digit sequence - the
/// original's own `new_cia_flags * 10 + CPT_*` accumulation, kept
/// exactly rather than re-encoded as a cleaner bitmask, since real
/// consumers decode those decimal digits positionally.
///
/// Reads the GLOBAL `OPTION_VARS.p_cia` directly rather than
/// `args.os_varp`, exactly as the original does (see
/// [`did_set_breakat`]'s own note about this recurring pattern).
pub fn did_set_completeitemalign(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // SAFETY: a plain field read on `OPTION_VARS`, no aliasing hazard.
    let p_cia = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_cia.clone().unwrap_or_default();

    let mut new_cia_flags: u32 = 0;
    let mut seen = [false; 3];
    let mut count = 0;
    let mut p = 0usize;

    while p < p_cia.len() {
        let (buf, next) = crate::option::copy_option_part(&p_cia, p, 10, b",");
        p = next;

        if count >= 3 {
            return Some(crate::errors::e_invarg.as_bytes());
        }

        let idx = match buf.as_slice() {
            b"abbr" => crate::insexpand::CPT_ABBR,
            b"kind" => crate::insexpand::CPT_KIND,
            b"menu" => crate::insexpand::CPT_MENU,
            _ => return Some(crate::errors::e_invarg.as_bytes()),
        };

        if seen[idx as usize] {
            return Some(crate::errors::e_invarg.as_bytes());
        }
        new_cia_flags = new_cia_flags * 10 + idx as u32;
        seen[idx as usize] = true;
        count += 1;
    }

    if new_cia_flags == 0 || count != 3 {
        return Some(crate::errors::e_invarg.as_bytes());
    }

    unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cia_flags = new_cia_flags;
    None
}

/// The `'*expr'` family of options is changed (`did_set_optexpr`) -
/// `'diffexpr'`, `'foldexpr'`, `'formatexpr'`, `'includeexpr'`,
/// `'indentexpr'`, `'patchexpr'`, `'printexpr'`, `'charconvert'`.
///
/// If the value starts with `<SID>` or `s:`, that prefix is replaced
/// with the real, resolved script identifier, via `userfunc.rs`'s
/// already-real `get_scriptlocal_funcname`. Never fails.
///
/// The original frees the old string and stores the expanded one
/// through `*varp`; here that is a plain assignment through the same
/// pointer, with Rust's own `Vec` drop handling the free.
///
/// # Safety
/// `args.os_varp` must point to a live, WRITABLE `Option<Vec<u8>>`
/// for the whole call (this callback REPLACES the value, unlike every
/// other one in this file, which only read it). Touches
/// `crate::globals::GLOBALS` via `get_scriptlocal_funcname`.
pub unsafe fn did_set_optexpr(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let varp = args.os_varp as *mut Option<Vec<u8>>;
    // SAFETY: forwarded from this function's own safety doc.
    let val: Vec<u8> = unsafe { &*varp }.clone().unwrap_or_default();

    // SAFETY: forwarded from this function's own safety doc.
    if let Some(name) = unsafe { crate::eval::userfunc::get_scriptlocal_funcname(&val) } {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *varp = Some(name) };
    }

    None
}

/// The `'foldexpr'` option is changed (`did_set_foldexpr`).
///
/// Delegates to [`did_set_optexpr`] for the `<SID>`/`s:` expansion
/// (deliberately discarding its return value, matching the original,
/// which likewise ignores it - `did_set_optexpr` never fails), then
/// updates the folds when `'foldmethod'` is `"expr"` - the same shape
/// as [`did_set_foldignore`].
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call. `args.os_varp` must point to a live, WRITABLE
/// `Option<Vec<u8>>` (forwarded from [`did_set_optexpr`]).
pub unsafe fn did_set_foldexpr(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;

    // The original discards this return value; `did_set_optexpr`
    // never fails, so nothing is lost.
    // SAFETY: forwarded from this function's own safety doc.
    let _ = unsafe { did_set_optexpr(args) };

    // SAFETY: forwarded from this function's own safety doc.
    if crate::fold::foldmethod_is_expr(unsafe { &*win_ptr }) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::fold::fold_update_all(win_ptr) };
    }

    None
}

/// The `'statusline'`/`'winbar'`/`'tabline'`/`'rulerformat'`/
/// `'statuscolumn'` option is changed
/// (`did_set_statustabline_rulerformat`, a `static` helper in the
/// original).
///
/// All five options share this one body, distinguished by the
/// `rulerformat`/`statuscolumn` flags plus an `os_idx ==
/// kOptStatusline` check. Two of its branches are NOT translated and
/// panic if reached:
///
/// - the `is_stl` branches (reset an empty global `'statusline'` to
///   its default, and reconfigure a floating window) need
///   `get_option_default`/`win_config_float`, neither translated;
/// - the `rulerformat` branch needs `ui_has`, not translated.
///
/// The two statusline-only branches panic only when their exact
/// conditions are reached. Ordinary nonempty statusline values on
/// non-floating windows are validated in full.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// when `statuscolumn` is true, and any non-null statusline window
/// pointer must likewise be valid. `args.os_varp` must point to a live
/// `Option<Vec<u8>>`. When `rulerformat` is true,
/// `crate::globals::GLOBALS.firstwin`'s own `w_next` chain must
/// consist of valid, live `WinT` pointers (forwarded to `comp_col`).
unsafe fn did_set_statustabline_rulerformat(
    args: &mut crate::option_defs::OptsetT,
    rulerformat: bool,
    statuscolumn: bool,
) -> Option<&'static [u8]> {
    let mut errmsg: Option<&'static [u8]> = None;

    if rulerformat {
        // Reset ru_wid first.
        unsafe { crate::globals::GLOBALS.get_mut() }.ru_wid = 0;
    } else if statuscolumn {
        // Reset the 'statuscolumn' width.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *(args.os_win as *mut crate::buffer_defs::WinT) }.w_nrwidth_line_count = 0;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let s: &[u8] = varp.as_deref().unwrap_or(&[]);

    let is_stl = args.os_idx == crate::option_defs::OptIndex::Statusline;
    let flags = args.os_flags as u32;
    if is_stl
        && (flags & crate::option_defs::opt_set_flags::OPT_GLOBAL != 0
            || flags & crate::option_defs::opt_set_flags::OPT_LOCAL == 0)
        && s.is_empty()
    {
        unimplemented!(
            "did_set_statustabline_rulerformat: an empty global 'statusline' \
             needs get_option_default, not translated"
        );
    }
    if is_stl
        && !args.os_win.is_null()
        && unsafe { &*(args.os_win as *const crate::buffer_defs::WinT) }.w_floating
    {
        unimplemented!(
            "did_set_statustabline_rulerformat: a floating 'statusline' needs \
             win_config_float, not translated"
        );
    }

    if rulerformat
        && !crate::ui::ui_has(crate::ui::UiExtension::Messages)
        && s.first() == Some(&b'%')
    {
        // Set ru_wid if 'ruf' starts with "%99(".
        //
        // Note this validates the GLOBAL `p_ruf` rather than the local
        // `s`, and reads `(*varp)[1]` (the ORIGINAL value) rather than
        // the advanced scan position - both exactly as the original
        // does.
        let p_ruf = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_ruf
            .clone()
            .unwrap_or_default();

        let mut pos = 1usize; // the original's own `*++s`
        if s.get(pos) == Some(&b'-') {
            pos += 1; // ignore a '-'
        }
        let (wid, consumed) =
            crate::charset::getdigits_int(&s[pos.min(s.len())..], true, 0);
        pos += consumed;

        let mut took_wid = false;
        if wid != 0 && s.get(pos) == Some(&b'(') {
            errmsg = check_stl_option(&p_ruf);
            if errmsg.is_none() {
                unsafe { crate::globals::GLOBALS.get_mut() }.ru_wid = wid;
                took_wid = true;
            }
        }
        if !took_wid && s.get(1) != Some(&b'!') {
            // Validate the flags in 'rulerformat' only if it doesn't
            // point to a custom function ("%!" flag).
            errmsg = check_stl_option(&p_ruf);
        }
    } else if s.first() != Some(&b'%') || s.get(1) != Some(&b'!') {
        // Check the value only if it doesn't start with "%!" (a custom
        // function reference, which isn't statusline syntax at all).
        errmsg = check_stl_option(s);
    }

    if rulerformat && errmsg.is_none() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::drawscreen::comp_col() };
    }

    errmsg
}

/// The `'statusline'` option is changed (`did_set_statusline`).
///
/// # Safety
/// Forwarded from `did_set_statustabline_rulerformat`'s own safety
/// doc.
pub unsafe fn did_set_statusline(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    unsafe { did_set_statustabline_rulerformat(args, false, false) }
}

/// The `'winbar'` option is changed (`did_set_winbar`).
///
/// # Safety
/// Forwarded from `did_set_statustabline_rulerformat`'s own safety
/// doc.
pub unsafe fn did_set_winbar(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { did_set_statustabline_rulerformat(args, false, false) }
}

/// `parse_border_opt` - validate one border-style option value.
///
/// Parses `border_opt` into a throwaway `WinConfig`, purely to find out
/// whether it is a legal border style. Both the config and any error
/// message are discarded; only the accepted/rejected outcome matters.
fn parse_border_opt(border_opt: &[u8]) -> bool {
    let mut fconfig = crate::buffer_defs::WinConfig::default();
    let mut err = crate::api::private::defs::Error::default();
    let result = crate::api::win_config::parse_winborder(&mut fconfig, border_opt, &mut err);
    // The original's own `api_clear_error(&err)` frees a heap-allocated
    // message; `Error`'s own `Drop` already does that here.
    result
}

/// The `'winborder'` option is changed (`did_set_winborder`).
///
/// Note this reads the global `p_winborder` rather than
/// `args.os_varp`, matching the original.
///
/// # Safety
/// Must not run concurrently with any other access to `OPTION_VARS`.
pub unsafe fn did_set_winborder(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    let value = opts.p_winborder.clone().unwrap_or_default();
    if !parse_border_opt(&value) {
        return Some(crate::errors::e_invarg.as_bytes());
    }
    None
}

/// The `'pumborder'` option is changed (`did_set_pumborder`).
///
/// Note this reads the global `p_pumborder` rather than
/// `args.os_varp`, matching the original.
///
/// # Safety
/// Must not run concurrently with any other access to `OPTION_VARS`.
pub unsafe fn did_set_pumborder(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    let value = opts.p_pumborder.clone().unwrap_or_default();
    if !parse_border_opt(&value) {
        return Some(crate::errors::e_invarg.as_bytes());
    }
    None
}

/// The `'tabline'` option is changed (`did_set_tabline`).
///
/// # Safety
/// Forwarded from `did_set_statustabline_rulerformat`'s own safety
/// doc.
pub unsafe fn did_set_tabline(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { did_set_statustabline_rulerformat(args, false, false) }
}

/// The `'statuscolumn'` option is changed (`did_set_statuscolumn`).
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// (this variant always resets the window's own
/// `w_nrwidth_line_count`). Otherwise forwarded from
/// `did_set_statustabline_rulerformat`'s own safety doc.
pub unsafe fn did_set_statuscolumn(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { did_set_statustabline_rulerformat(args, false, true) }
}

/// The `'rulerformat'` option is changed (`did_set_rulerformat`).
///
/// Takes the shared helper's own `rulerformat` path: reset `ru_wid`,
/// then - when the value starts with `%` and the `messages` UI
/// extension is not active - parse a leading `%99(`-style width and
/// store it in `ru_wid`, finally recomputing the ruler column via
/// `drawscreen.rs`'s already-real `comp_col`.
///
/// # Safety
/// Forwarded from `did_set_statustabline_rulerformat`'s own safety
/// doc - in particular `GLOBALS.firstwin`'s own `w_next` chain must
/// be valid, since this variant always reaches `comp_col`.
pub unsafe fn did_set_rulerformat(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { did_set_statustabline_rulerformat(args, true, false) }
}

/// Validate `'shellpipe'`/`'shellredir'` (`did_set_shellpipe_redir`).
///
/// The value is a shell command template in which `%s` marks where
/// the real file name is substituted. At most ONE `%s` is allowed;
/// `%%` is a literal-percent escape; any other `%`-sequence (or a
/// trailing bare `%`) is rejected.
///
/// Reads `args.os_newval` rather than `args.os_varp`, matching the
/// original exactly.
pub fn did_set_shellpipe_redir(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let val: &[u8] = match &args.os_newval {
        crate::option_defs::OptVal::String(s) => s,
        _ => &[],
    };

    let mut seen = false;
    let mut p = 0usize;
    while p < val.len() {
        if val[p] != b'%' {
            p += 1;
            continue;
        }
        // A bare trailing '%' with nothing after it. The original
        // reads `p[1]` and finds its own NUL terminator; here that is
        // an explicit "is '%' the last byte" bounds check.
        if p + 1 >= val.len() {
            return Some(crate::errors::e_invalid_format_string_single_percent_s.as_bytes());
        }
        if val[p + 1] == b'%' {
            p += 2; // skip second %, plus the loop's own increment
            continue;
        }
        if val[p + 1] == b's' {
            if seen {
                return Some(crate::errors::e_invalid_format_string_single_percent_s.as_bytes());
            }
            seen = true;
            p += 2; // consume 's', plus the loop's own increment
            continue;
        }
        return Some(crate::errors::e_invalid_format_string_single_percent_s.as_bytes());
    }

    None
}

/// Error message for a `'comments'` entry with no `:` separator
/// (`E524`, an inline `N_()` literal in the original - kept
/// file-local here, following this module's own `e_comma_required`
/// precedent).
#[allow(non_upper_case_globals)]
const e_missing_colon: &str = crate::gettext_defs::gettext_noop("E524: Missing colon");

/// Error message for a `'comments'` entry whose comment string is
/// empty (`E525`, an inline `N_()` literal in the original - see
/// [`e_missing_colon`]'s own note).
#[allow(non_upper_case_globals)]
const e_zero_length_string: &str = crate::gettext_defs::gettext_noop("E525: Zero length string");

/// The `'comments'` option is changed (`did_set_comments`).
///
/// `'comments'` is a comma-separated list of `flags:string` parts.
/// Each part's flag section may contain only `option_vars::COM_ALL`
/// characters, ASCII digits, and `-`; it must be followed by a `:`
/// and a non-empty comment string.
///
/// **Faithfully preserves a genuinely surprising control-flow quirk.**
/// When the flag scan hits an illegal character it sets the error and
/// `break`s - but that `break` only leaves the INNER loop, so the
/// following "missing colon"/"zero length string" checks still run
/// and can OVERWRITE the illegal-character error. Concretely,
/// `comments=z` reports `E525` (not the illegal-character error),
/// because after the break the scan advances past `z` onto the
/// string's own end; whereas `comments=zb:x` does report the
/// illegal-character error, because `b` follows. Both were verified
/// directly against a real `nvim` binary before this was written, and
/// each has its own dedicated regression test - this is preserved,
/// not "fixed".
///
/// Per this module's established policy, the original's own
/// dynamically-formatted `illegal_char` message is simplified to a
/// static [`crate::errors::e_invarg`].
///
/// # Safety
/// `args.os_varp` must point to a live `Option<Vec<u8>>` for the
/// whole call.
pub unsafe fn did_set_comments(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);
    let len = val.len();

    let mut errmsg: Option<&'static [u8]> = None;
    let mut s = 0usize;

    while s < len {
        // Flag characters, up to the ':' separator.
        while s < len && val[s] != b':' {
            let c = val[s];
            if !crate::option_vars::COM_ALL.as_bytes().contains(&c)
                && !crate::ascii_defs::ascii_isdigit(i32::from(c))
                && c != b'-'
            {
                errmsg = Some(crate::errors::e_invarg.as_bytes());
                break;
            }
            s += 1;
        }

        // The original's `if (*s++ == NUL)` reads the current byte and
        // advances UNCONDITIONALLY, including on the illegal-character
        // path above - see this function's own doc comment.
        let at_end = s >= len;
        s += 1;
        if at_end {
            errmsg = Some(e_missing_colon.as_bytes());
        } else if s >= len || val[s] == b',' {
            errmsg = Some(e_zero_length_string.as_bytes());
        }
        if errmsg.is_some() {
            break;
        }

        // The comment string itself, honouring backslash escapes.
        while s < len && val[s] != b',' {
            if val[s] == b'\\' && s + 1 < len {
                s += 1;
            }
            s += 1;
        }
        s = crate::option::skip_to_option_part(val, s);
    }

    errmsg
}

/// The `'breakat'` option is changed (`did_set_breakat`).
///
/// Rebuilds `option_vars::OPTION_VARS.breakat_flags` - a 256-entry
/// "is this byte a line-break character" lookup table - by clearing
/// it and then setting one entry per byte of `'breakat'`. No
/// validation at all: any value is accepted.
///
/// Reads the GLOBAL `OPTION_VARS.p_breakat` directly rather than
/// `args.os_varp`, exactly as the original does. For a real
/// invocation those are the same storage (`'breakat'` is global-only),
/// but a test that only sets a disconnected local value would
/// silently read stale global state - so this function's own tests
/// set `OPTION_VARS.p_breakat` itself.
///
/// Its `OPTIONS` entry and `did_set_option` dispatch are real now.
/// [`crate::charset::vim_isbreak`] still uses its own fixed
/// DEFAULT-`'breakat'` table rather than reading `breakat_flags`.
pub fn did_set_breakat(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // SAFETY: a plain field read/write on `OPTION_VARS`, no aliasing
    // hazard (no other reference into it is held across this call).
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };

    opts.breakat_flags = [0; 256];

    if let Some(breakat) = opts.p_breakat.clone() {
        for &b in &breakat {
            opts.breakat_flags[b as usize] = 1;
        }
    }

    None
}

/// The `'foldignore'` option is changed (`did_set_foldignore`).
///
/// Pure side effect, no validation at all - `'foldignore'` accepts
/// any value.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call.
pub unsafe fn did_set_foldignore(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;
    // SAFETY: forwarded from this function's own safety doc.
    if crate::fold::foldmethod_is_indent(unsafe { &*win_ptr }) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::fold::fold_update_all(win_ptr) };
    }
    None
}

/// The `'foldmarker'` option is changed (`did_set_foldmarker`).
///
/// The value must contain a comma with at least one character on
/// EITHER side of it (the start and end marker strings).
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call. `args.os_varp` must point to a live
/// `Option<Vec<u8>>`.
pub unsafe fn did_set_foldmarker(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &*(args.os_win as *const crate::buffer_defs::WinT) };
    // SAFETY: forwarded from this function's own safety doc.
    let varp = unsafe { &*(args.os_varp as *const Option<Vec<u8>>) };
    let val: &[u8] = varp.as_deref().unwrap_or(&[]);

    let Some(p) = crate::strings::vim_strchr(val, i32::from(b',')) else {
        return Some(e_comma_required.as_bytes());
    };

    // `p == *varp` (comma is the very first byte, so no start marker)
    // or `p[1] == NUL` (nothing after the comma, so no end marker).
    // The latter's own past-the-end read in the original relies on
    // the implicit C-string NUL terminator; here it's an explicit
    // "is the comma the last byte" bounds check instead.
    if p == 0 || p + 1 >= val.len() {
        return Some(crate::errors::e_invarg.as_bytes());
    }

    if crate::fold::foldmethod_is_marker(win) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::fold::fold_update_all(args.os_win as *mut crate::buffer_defs::WinT) };
    }

    None
}

/// The `'foldmethod'` option is changed (`did_set_foldmethod`).
///
/// Validates the new value against the option's own value list via
/// [`did_set_str_generic`], then - because the whole fold tree was
/// built by the PREVIOUS method and is now meaningless - invalidates
/// it unconditionally. When the new method is `"diff"` the fold
/// levels are recomputed straight away too, since `'diff'` folds get
/// their level from the diff state rather than from the buffer text.
///
/// # Safety
/// `args.os_win` must be a valid, non-null pointer to a live `WinT`
/// for the whole call, and (when the new method is `"diff"`)
/// `crate::globals::GLOBALS.curwin` must likewise be valid and
/// non-null. `args.os_varp` must point to a live `Option<Vec<u8>>`.
pub unsafe fn did_set_foldmethod(args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let errmsg = unsafe { did_set_str_generic(args) };
    if errmsg.is_some() {
        return errmsg;
    }

    let win_ptr = args.os_win as *mut crate::buffer_defs::WinT;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::fold::fold_update_all(win_ptr) };
    // SAFETY: forwarded from this function's own safety doc.
    if crate::fold::foldmethod_is_diff(unsafe { &*win_ptr }) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::fold::new_fold_level() };
    }
    None
}

/// Read one `'listchars'`/`'fillchars'` field character from `p`,
/// advancing past it (`get_encoded_char_adv`).
///
/// Calls the equivalent of `mb_cptr2char_adv(p)` and returns the
/// character. If `p` starts with `\x`, `\u` or `\U` the hex or
/// unicode value is used instead.
///
/// Returns `(schar, consumed)`. A `schar` of `0` means invalid hex or
/// an invalid UTF-8 byte, matching the original's own sentinel; note
/// a double-width character also yields `0`, since the original notes
/// two-column characters are forbidden here.
///
/// The original takes `const char **p` and advances it in place;
/// returning the byte count consumed says the same thing without a
/// pointer-to-pointer. Note the original advances `*p` even on the
/// hex path's own failure returns, so the consumed count is reported
/// on every path rather than only on success.
///
/// # Safety
/// Forwarded from [`crate::mbyte::utfc_ptr2schar`]'s own safety doc.
unsafe fn get_encoded_char_adv(p: &[u8]) -> (crate::types_defs::ScharT, usize) {
    if p.len() >= 2 && p[0] == b'\\' && matches!(p[1], b'x' | b'u' | b'U') {
        let mut num: i64 = 0;
        let bytes = match p[1] {
            b'x' => 1,
            b'u' => 2,
            _ => 4,
        };
        let mut off = 0usize;
        for _ in 0..bytes {
            off += 2;
            let n = if off < p.len() {
                crate::charset::hexhex2nr(&p[off..])
            } else {
                -1
            };
            if n < 0 {
                return (0, off);
            }
            num = num * 256 + i64::from(n);
        }
        off += 2;
        let num = i32::try_from(num).unwrap_or(0xFFFD);
        // SAFETY: a plain width lookup on a codepoint value.
        let too_wide = unsafe { crate::charset::char2cells(num) } > 1;
        return (
            if too_wide {
                0
            } else {
                crate::grid::schar_from_char(num)
            },
            off,
        );
    }

    // SAFETY: forwarded from this function's own safety doc.
    let clen = usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(p) }).unwrap_or(0);
    // SAFETY: forwarded from this function's own safety doc.
    let (c, firstc) = unsafe { crate::mbyte::utfc_ptr2schar(p) };
    // SAFETY: a plain width lookup on a codepoint value.
    let too_wide = unsafe { crate::charset::char2cells(firstc) } > 1;
    // Invalid UTF-8 byte or doublewidth not allowed
    let sc = if (clen == 1 && firstc > 127) || too_wide {
        0
    } else {
        c
    };
    (sc, clen)
}

/// Which `'listchars'`/`'fillchars'` option a `set_chars_option` call
/// is handling (`CharsOption`, `optionstr.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharsOption {
    Fillchars,
    Listchars,
}

/// Which field of `LcsCharsT`/`FcsCharsT` one `chars_tab` entry
/// writes to.
///
/// The original stores a raw `schar_T *cp` pointing straight into the
/// file-static `lcs_chars`/`fcs_chars`, and then compares that pointer
/// against specific field addresses (`tab[i].cp == &lcs_chars.tab2`)
/// to recognise the multi-character `tab`/`leadtab` entries. A field
/// selector is the direct translation of "which field does this entry
/// write", and selector equality is exactly the same test as the
/// original's pointer equality - without needing raw pointers into a
/// mutable static. `None` models the original's own `NULL` `cp`, used
/// by the `multispace`/`leadmultispace` entries that are handled
/// entirely by special cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharsField {
    None,
    LcsEol,
    LcsExt,
    LcsNbsp,
    LcsPrec,
    LcsSpace,
    LcsTab2,
    LcsLeadtab2,
    LcsLead,
    LcsTrail,
    LcsConceal,
    FcsStl,
    FcsStlnc,
    FcsWbr,
    FcsHoriz,
    FcsHorizup,
    FcsHorizdown,
    FcsVert,
    FcsVertleft,
    FcsVertright,
    FcsVerthoriz,
    FcsFold,
    FcsFoldopen,
    FcsFoldclosed,
    FcsFoldsep,
    FcsFoldinner,
    FcsDiff,
    FcsMsgsep,
    FcsEob,
    FcsLastline,
    FcsTrunc,
    FcsTruncrl,
}

/// One row of `lcs_tab`/`fcs_tab` (`struct chars_tab`).
struct CharsTab {
    /// which field this entry writes (`schar_T *cp`)
    cp: CharsField,
    /// char id (`String name`)
    name: &'static str,
    /// default value
    def: Option<&'static str>,
    /// default value when `def` isn't single-width
    fallback: Option<&'static str>,
}

/// Shorthand mirroring the original's own `CHARSTAB_ENTRY` macro.
const fn charstab_entry(
    cp: CharsField,
    name: &'static str,
    def: Option<&'static str>,
    fallback: Option<&'static str>,
) -> CharsTab {
    CharsTab {
        cp,
        name,
        def,
        fallback,
    }
}

/// `fcs_tab` - the `'fillchars'` field table.
const FCS_TAB: [CharsTab; 21] = [
    charstab_entry(CharsField::FcsStl, "stl", Some(" "), None),
    charstab_entry(CharsField::FcsStlnc, "stlnc", Some(" "), None),
    charstab_entry(CharsField::FcsWbr, "wbr", Some(" "), None),
    charstab_entry(CharsField::FcsHoriz, "horiz", Some("─"), Some("-")),
    charstab_entry(CharsField::FcsHorizup, "horizup", Some("┴"), Some("-")),
    charstab_entry(CharsField::FcsHorizdown, "horizdown", Some("┬"), Some("-")),
    charstab_entry(CharsField::FcsVert, "vert", Some("│"), Some("|")),
    charstab_entry(CharsField::FcsVertleft, "vertleft", Some("┤"), Some("|")),
    charstab_entry(CharsField::FcsVertright, "vertright", Some("├"), Some("|")),
    charstab_entry(CharsField::FcsVerthoriz, "verthoriz", Some("┼"), Some("+")),
    charstab_entry(CharsField::FcsFold, "fold", Some("·"), Some("-")),
    charstab_entry(CharsField::FcsFoldopen, "foldopen", Some("-"), None),
    // NB: the field is `foldclosed` but the option name is `foldclose`.
    charstab_entry(CharsField::FcsFoldclosed, "foldclose", Some("+"), None),
    charstab_entry(CharsField::FcsFoldsep, "foldsep", Some("│"), Some("|")),
    charstab_entry(CharsField::FcsFoldinner, "foldinner", None, None),
    charstab_entry(CharsField::FcsDiff, "diff", Some("-"), None),
    charstab_entry(CharsField::FcsMsgsep, "msgsep", Some(" "), None),
    charstab_entry(CharsField::FcsEob, "eob", Some("~"), None),
    charstab_entry(CharsField::FcsLastline, "lastline", Some("@"), None),
    charstab_entry(CharsField::FcsTrunc, "trunc", Some(">"), None),
    charstab_entry(CharsField::FcsTruncrl, "truncrl", Some("<"), None),
];

/// `lcs_tab` - the `'listchars'` field table.
const LCS_TAB: [CharsTab; 12] = [
    charstab_entry(CharsField::LcsEol, "eol", None, None),
    charstab_entry(CharsField::LcsExt, "extends", None, None),
    charstab_entry(CharsField::LcsNbsp, "nbsp", None, None),
    charstab_entry(CharsField::LcsPrec, "precedes", None, None),
    charstab_entry(CharsField::LcsSpace, "space", None, None),
    charstab_entry(CharsField::LcsTab2, "tab", None, None),
    charstab_entry(CharsField::LcsLeadtab2, "leadtab", None, None),
    charstab_entry(CharsField::LcsLead, "lead", None, None),
    charstab_entry(CharsField::LcsTrail, "trail", None, None),
    charstab_entry(CharsField::LcsConceal, "conceal", None, None),
    charstab_entry(CharsField::None, "multispace", None, None),
    charstab_entry(CharsField::None, "leadmultispace", None, None),
];

/// The `idx`'th possible `'fileformat'` value, or `None` past the end
/// (`get_fileformat_name`, given to `ExpandGeneric` for completion).
///
/// The original's own unused `expand_T *xp` parameter is dropped -
/// this crate's established treatment for parameters no branch reads.
#[must_use]
pub fn get_fileformat_name(idx: i32) -> Option<&'static str> {
    let idx = usize::try_from(idx).ok()?;
    crate::option_vars::OPT_FF_VALUES.get(idx).copied()
}

/// The `idx`'th `'fillchars'` field name, or `None` past the end
/// (`get_fillchars_name`).
///
/// Unlike [`get_fileformat_name`] the original bounds-checks `idx < 0`
/// explicitly here; the `usize` conversion covers both ends.
#[must_use]
pub fn get_fillchars_name(idx: i32) -> Option<&'static str> {
    let idx = usize::try_from(idx).ok()?;
    FCS_TAB.get(idx).map(|e| e.name)
}

/// The `idx`'th `'listchars'` field name, or `None` past the end
/// (`get_listchars_name`).
#[must_use]
pub fn get_listchars_name(idx: i32) -> Option<&'static str> {
    let idx = usize::try_from(idx).ok()?;
    LCS_TAB.get(idx).map(|e| e.name)
}

/// Store `v` into whichever `LcsCharsT`/`FcsCharsT` field `f` names.
///
/// A no-op for [`CharsField::None`], matching the original's own
/// `if (tab[i].cp != NULL)` guard.
fn store_chars_field(
    lcs: &mut crate::buffer_defs::LcsCharsT,
    fcs: &mut crate::buffer_defs::FcsCharsT,
    f: CharsField,
    v: crate::types_defs::ScharT,
) {
    use CharsField as F;
    match f {
        F::None => {}
        F::LcsEol => lcs.eol = v,
        F::LcsExt => lcs.ext = v,
        F::LcsNbsp => lcs.nbsp = v,
        F::LcsPrec => lcs.prec = v,
        F::LcsSpace => lcs.space = v,
        F::LcsTab2 => lcs.tab2 = v,
        F::LcsLeadtab2 => lcs.leadtab2 = v,
        F::LcsLead => lcs.lead = v,
        F::LcsTrail => lcs.trail = v,
        F::LcsConceal => lcs.conceal = v,
        F::FcsStl => fcs.stl = v,
        F::FcsStlnc => fcs.stlnc = v,
        F::FcsWbr => fcs.wbr = v,
        F::FcsHoriz => fcs.horiz = v,
        F::FcsHorizup => fcs.horizup = v,
        F::FcsHorizdown => fcs.horizdown = v,
        F::FcsVert => fcs.vert = v,
        F::FcsVertleft => fcs.vertleft = v,
        F::FcsVertright => fcs.vertright = v,
        F::FcsVerthoriz => fcs.verthoriz = v,
        F::FcsFold => fcs.fold = v,
        F::FcsFoldopen => fcs.foldopen = v,
        F::FcsFoldclosed => fcs.foldclosed = v,
        F::FcsFoldsep => fcs.foldsep = v,
        F::FcsFoldinner => fcs.foldinner = v,
        F::FcsDiff => fcs.diff = v,
        F::FcsMsgsep => fcs.msgsep = v,
        F::FcsEob => fcs.eob = v,
        F::FcsLastline => fcs.lastline = v,
        F::FcsTrunc => fcs.trunc = v,
        F::FcsTruncrl => fcs.truncrl = v,
    }
}

/// `field_value_err` - report a bad `'listchars'`/`'fillchars'` field.
///
/// The original formats the offending field's own name into `fmt`'s
/// `%s` and returns `errbuf`; when `errbuf` is NULL it returns `""`
/// instead. That empty string still compares non-NULL at every call
/// site, so the field error is still reported as an error, just
/// without a message - `check_chars_options` relies on exactly this.
/// Reproduced faithfully here, including the empty-message case.
fn field_value_err(errbuf: Option<&mut Vec<u8>>, fmt: &'static str) -> &'static [u8] {
    match errbuf {
        None => b"",
        Some(buf) => {
            buf.clear();
            buf.extend_from_slice(fmt.as_bytes());
            fmt.as_bytes()
        }
    }
}

/// Handle setting `'listchars'` or `'fillchars'`
/// (`set_chars_option`). Assumes monocell characters.
///
/// `value` is either the global or the window-local value; `what`
/// selects which option; `apply` false means check for errors only,
/// without storing anything.
///
/// Returns an error message, or `None` if the value is OK.
///
/// The original keeps its two scratch structs (`lcs_chars`/
/// `fcs_chars`) as file statics, but they are pure scratch: every
/// field is re-initialised at the top of the storing round and they
/// are only read at the very end, so they are ordinary locals here.
/// That removes two mutable statics without changing any observable
/// behaviour.
///
/// # Safety
/// Forwarded from `get_encoded_char_adv`'s own safety doc. Must not
/// run concurrently with any other access to `OPTION_VARS`.
pub unsafe fn set_chars_option(
    wp: &mut crate::buffer_defs::WinT,
    value: &[u8],
    what: CharsOption,
    apply: bool,
    mut errbuf: Option<&mut Vec<u8>>,
) -> Option<&'static [u8]> {
    // Last occurrence of "multispace:" / "leadmultispace:"
    let mut last_multispace: Option<usize> = None;
    let mut last_lmultispace: Option<usize> = None;
    let mut multispace_len = 0usize;
    let mut lead_multispace_len = 0usize;

    let tab: &[CharsTab] = match what {
        CharsOption::Listchars => &LCS_TAB,
        CharsOption::Fillchars => &FCS_TAB,
    };
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    // A local value of "" means "use the global value".
    let value: Vec<u8> = match what {
        CharsOption::Listchars => {
            if wp.w_onebuf_opt.wo_lcs.as_deref().unwrap_or(b"").is_empty() {
                opts.p_lcs.clone().unwrap_or_default()
            } else {
                value.to_vec()
            }
        }
        CharsOption::Fillchars => {
            if wp.w_onebuf_opt.wo_fcs.as_deref().unwrap_or(b"").is_empty() {
                opts.p_fcs.clone().unwrap_or_default()
            } else {
                value.to_vec()
            }
        }
    };

    let mut lcs = crate::buffer_defs::LcsCharsT::default();
    let mut fcs = crate::buffer_defs::FcsCharsT::default();

    // first round: check for valid value, second round: assign values
    let last_round = i32::from(apply);
    for round in 0..=last_round {
        let mut has_tab = false;
        let mut has_leadtab = false;

        if round > 0 {
            // After checking that the value is valid: set defaults
            for e in tab {
                // XXX (from the original): characters taking 2 columns
                // are forbidden (TUI limitation?). Set old defaults in
                // that case.
                // SAFETY: a plain width lookup on the table's own text.
                let use_def = e
                    .def
                    .is_some_and(|d| unsafe { crate::charset::ptr2cells(d.as_bytes()) } == 1);
                let src = if use_def { e.def } else { e.fallback };
                store_chars_field(
                    &mut lcs,
                    &mut fcs,
                    e.cp,
                    crate::grid::schar_from_str(src.map(str::as_bytes)),
                );
            }

            if what == CharsOption::Listchars {
                lcs.tab1 = 0;
                lcs.tab3 = 0;
                lcs.leadtab1 = 0;
                lcs.leadtab3 = 0;
                lcs.multispace = (multispace_len > 0).then(|| vec![0; multispace_len]);
                lcs.leadmultispace = (lead_multispace_len > 0).then(|| vec![0; lead_multispace_len]);
            }
        }

        let mut p = 0usize;
        while p < value.len() && value[p] != 0 {
            let mut matched = false;
            for e in tab {
                let n = e.name.len();
                if !(value[p..].starts_with(e.name.as_bytes())
                    && value.get(p + n) == Some(&b':'))
                {
                    continue;
                }
                matched = true;
                let mut s = p + n + 1;

                let is_multi = what == CharsOption::Listchars
                    && (e.name == "multispace" || e.name == "leadmultispace");
                if is_multi {
                    let lead = e.name == "leadmultispace";
                    if round == 0 {
                        let mut count = 0usize;
                        while s < value.len() && value[s] != 0 && value[s] != b',' {
                            // SAFETY: forwarded from this fn's safety doc.
                            let (c1, adv) = unsafe { get_encoded_char_adv(&value[s..]) };
                            s += adv;
                            if c1 == 0 {
                                return Some(field_value_err(
                                    errbuf.as_deref_mut(),
                                    e_wrong_character_width_for_field_str,
                                ));
                            }
                            count += 1;
                        }
                        if count == 0 {
                            // cannot be an empty string
                            return Some(field_value_err(
                                errbuf.as_deref_mut(),
                                e_wrong_number_of_characters_for_field_str,
                            ));
                        }
                        if lead {
                            last_lmultispace = Some(p);
                            lead_multispace_len = count;
                        } else {
                            last_multispace = Some(p);
                            multispace_len = count;
                        }
                    } else {
                        let mut pos = 0usize;
                        let is_last = if lead {
                            last_lmultispace == Some(p)
                        } else {
                            last_multispace == Some(p)
                        };
                        while s < value.len() && value[s] != 0 && value[s] != b',' {
                            // SAFETY: forwarded from this fn's safety doc.
                            let (c1, adv) = unsafe { get_encoded_char_adv(&value[s..]) };
                            s += adv;
                            if is_last {
                                let dst = if lead {
                                    lcs.leadmultispace.as_mut()
                                } else {
                                    lcs.multispace.as_mut()
                                };
                                if let Some(v) = dst
                                    && pos < v.len()
                                {
                                    v[pos] = c1;
                                }
                                pos += 1;
                            }
                        }
                    }
                    p = s;
                    break;
                }

                if s >= value.len() || value[s] == 0 {
                    return Some(field_value_err(
                        errbuf.as_deref_mut(),
                        e_wrong_number_of_characters_for_field_str,
                    ));
                }
                // SAFETY: forwarded from this function's own safety doc.
                let (c1, adv) = unsafe { get_encoded_char_adv(&value[s..]) };
                s += adv;
                if c1 == 0 {
                    return Some(field_value_err(
                        errbuf.as_deref_mut(),
                        e_wrong_character_width_for_field_str,
                    ));
                }
                let mut c2 = 0;
                let mut c3 = 0;
                if e.cp == CharsField::LcsTab2 || e.cp == CharsField::LcsLeadtab2 {
                    if s >= value.len() || value[s] == 0 {
                        return Some(field_value_err(
                            errbuf.as_deref_mut(),
                            e_wrong_number_of_characters_for_field_str,
                        ));
                    }
                    // SAFETY: forwarded from this fn's safety doc.
                    let (v, adv) = unsafe { get_encoded_char_adv(&value[s..]) };
                    c2 = v;
                    s += adv;
                    if c2 == 0 {
                        return Some(field_value_err(
                            errbuf.as_deref_mut(),
                            e_wrong_character_width_for_field_str,
                        ));
                    }
                    let at_end = s >= value.len() || value[s] == 0 || value[s] == b',';
                    if !at_end {
                        // SAFETY: forwarded from this fn's safety doc.
                        let (v, adv) = unsafe { get_encoded_char_adv(&value[s..]) };
                        c3 = v;
                        s += adv;
                        if c3 == 0 {
                            return Some(field_value_err(
                                errbuf.as_deref_mut(),
                                e_wrong_character_width_for_field_str,
                            ));
                        }
                    }
                    if e.cp == CharsField::LcsTab2 {
                        has_tab = true;
                    } else {
                        has_leadtab = true;
                    }
                }

                if s >= value.len() || value[s] == 0 || value[s] == b',' {
                    if round > 0 {
                        if e.cp == CharsField::LcsTab2 {
                            lcs.tab1 = c1;
                            lcs.tab2 = c2;
                            lcs.tab3 = c3;
                        } else if e.cp == CharsField::LcsLeadtab2 {
                            lcs.leadtab1 = c1;
                            lcs.leadtab2 = c2;
                            lcs.leadtab3 = c3;
                        } else {
                            store_chars_field(&mut lcs, &mut fcs, e.cp, c1);
                        }
                    }
                    p = s;
                    break;
                }
                return Some(field_value_err(
                    errbuf.as_deref_mut(),
                    e_wrong_number_of_characters_for_field_str,
                ));
            }

            if !matched {
                return Some(crate::errors::e_invarg.as_bytes());
            }

            if p < value.len() && value[p] == b',' {
                p += 1;
            }
        }

        if what == CharsOption::Listchars && has_leadtab && !has_tab {
            return Some(crate::errors::e_leadtab_requires_tab.as_bytes());
        }
    }

    if apply {
        if what == CharsOption::Listchars {
            wp.w_p_lcs_chars = lcs;
        } else {
            wp.w_p_fcs_chars = fcs;
        }
    }

    None // no error
}

/// Error message for a `'listchars'` value that conflicts with the
/// current character widths (`e_conflicts_with_value_of_listchars`, a
/// file-local `static const char[]` in the original - kept file-local
/// here too, same precedent as `e_comma_required`).
#[allow(non_upper_case_globals)]
const e_conflicts_with_value_of_listchars: &str =
    crate::gettext_defs::gettext_noop("E834: Conflicts with value of 'listchars'");

/// Error message for a `'fillchars'` value that conflicts with the
/// current character widths (`e_conflicts_with_value_of_fillchars`).
/// Same file-local scoping as above.
#[allow(non_upper_case_globals)]
const e_conflicts_with_value_of_fillchars: &str =
    crate::gettext_defs::gettext_noop("E835: Conflicts with value of 'fillchars'");

/// The global `'listchars'` or `'fillchars'` option is changed
/// (`did_set_global_chars_option`).
///
/// # Safety
/// Forwarded from `set_chars_option`'s own safety doc. `win` must be
/// a valid, live `WinT`, and `GLOBALS`' own tabpage/window chains
/// must consist of valid, live pointers.
unsafe fn did_set_global_chars_option(
    win: *mut crate::buffer_defs::WinT,
    val: &[u8],
    what: CharsOption,
    opt_flags: i32,
    errbuf: Option<&mut Vec<u8>>,
) -> Option<&'static [u8]> {
    let is_global = (opt_flags & crate::option_defs::opt_set_flags::OPT_GLOBAL as i32) != 0;
    // SAFETY: forwarded from this function's own safety doc.
    let local_empty = unsafe {
        let w = &*win;
        match what {
            CharsOption::Listchars => w.w_onebuf_opt.wo_lcs.as_deref().unwrap_or(b""),
            CharsOption::Fillchars => w.w_onebuf_opt.wo_fcs.as_deref().unwrap_or(b""),
        }
        .is_empty()
    };

    // only apply the global value to "win" when it does not have a
    // local value
    // SAFETY: forwarded from this function's own safety doc.
    let errmsg = unsafe {
        set_chars_option(
            &mut *win,
            val,
            what,
            local_empty || !is_global,
            errbuf,
        )
    };
    if errmsg.is_some() {
        return errmsg;
    }

    // If the current window is set to use the global
    // 'listchars'/'fillchars' value, clear the window-local value.
    if !is_global {
        // `clear_string_option` in the original: frees the old string
        // and points it at the shared empty one. With an owned
        // `Option<Vec<u8>>` that is just an assignment.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            let w = &mut *win;
            match what {
                CharsOption::Listchars => w.w_onebuf_opt.wo_lcs = Some(Vec::new()),
                CharsOption::Fillchars => w.w_onebuf_opt.wo_fcs = Some(Vec::new()),
            }
        }
    }

    // If a window has a local value it needs to be applied again, it
    // was changed when setting the global value. No error is expected
    // here since none was returned above, so the result is ignored -
    // exactly as the original notes.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        for_all_tab_windows(|wp| {
            let opt = match what {
                CharsOption::Listchars => (*wp).w_onebuf_opt.wo_lcs.clone(),
                CharsOption::Fillchars => (*wp).w_onebuf_opt.wo_fcs.clone(),
            };
            let opt = opt.unwrap_or_default();
            if opt.is_empty() {
                let _ = set_chars_option(&mut *wp, &opt, what, true, None);
            }
        });
    }

    None
}

/// Call `f` for every window in every tabpage (`FOR_ALL_TAB_WINDOWS`),
/// following `move.rs`'s own established walk idiom.
///
/// # Safety
/// `GLOBALS.first_tabpage`'s own `tp_next` chain, and each tabpage's
/// own window list (`GLOBALS.firstwin`/`tp_firstwin`, then `w_next`),
/// must consist of valid, live pointers.
unsafe fn for_all_tab_windows(mut f: impl FnMut(*mut crate::buffer_defs::WinT)) {
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
            f(wp);
            // SAFETY: forwarded from this function's own safety doc.
            wp = unsafe { &*wp }.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
}

/// Check all global and local values of `'listchars'` and
/// `'fillchars'` (`check_chars_options`). May set different defaults
/// in case character widths change.
///
/// Returns an untranslated error message if any of them is invalid,
/// `None` otherwise.
///
/// # Safety
/// Forwarded from `set_chars_option`'s own safety doc.
/// `GLOBALS.curwin` must be a valid, live `WinT`, and the
/// tabpage/window chains must consist of valid, live pointers.
pub unsafe fn check_chars_options() -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let curwin = g.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    let (p_lcs, p_fcs) = (
        opts.p_lcs.clone().unwrap_or_default(),
        opts.p_fcs.clone().unwrap_or_default(),
    );

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { set_chars_option(&mut *curwin, &p_lcs, CharsOption::Listchars, false, None) }
        .is_some()
    {
        return Some(e_conflicts_with_value_of_listchars.as_bytes());
    }
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { set_chars_option(&mut *curwin, &p_fcs, CharsOption::Fillchars, false, None) }
        .is_some()
    {
        return Some(e_conflicts_with_value_of_fillchars.as_bytes());
    }

    let mut err: Option<&'static [u8]> = None;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        for_all_tab_windows(|wp| {
            if err.is_some() {
                return;
            }
            let lcs = (*wp).w_onebuf_opt.wo_lcs.clone().unwrap_or_default();
            if set_chars_option(&mut *wp, &lcs, CharsOption::Listchars, true, None).is_some() {
                err = Some(e_conflicts_with_value_of_listchars.as_bytes());
                return;
            }
            let fcs = (*wp).w_onebuf_opt.wo_fcs.clone().unwrap_or_default();
            if set_chars_option(&mut *wp, &fcs, CharsOption::Fillchars, true, None).is_some() {
                err = Some(e_conflicts_with_value_of_fillchars.as_bytes());
            }
        });
    }
    err
}

/// The `'fillchars'` option or the `'listchars'` option is changed
/// (`did_set_chars_option`).
///
/// # Safety
/// Forwarded from `did_set_global_chars_option`'s own safety doc.
/// `args.os_win` must be a valid, live `WinT`, and `args.os_varp`
/// must point to a live `Option<Vec<u8>>`.
pub unsafe fn did_set_chars_option(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let win = args.os_win.cast::<crate::buffer_defs::WinT>();
    let varp = args.os_varp.cast::<Option<Vec<u8>>>();
    // SAFETY: forwarded from this function's own safety doc.
    let val = unsafe { &*varp }.clone().unwrap_or_default();
    let opt_flags = args.os_flags;
    let mut errbuf = args.os_errbuf.take();

    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    let is_global_lcs = std::ptr::eq(varp, std::ptr::addr_of_mut!(opts.p_lcs));
    let is_global_fcs = std::ptr::eq(varp, std::ptr::addr_of_mut!(opts.p_fcs));

    // SAFETY: forwarded from this function's own safety doc.
    let errmsg = unsafe {
        if is_global_lcs {
            did_set_global_chars_option(
                win,
                &val,
                CharsOption::Listchars,
                opt_flags,
                errbuf.as_mut(),
            )
        } else if is_global_fcs {
            did_set_global_chars_option(
                win,
                &val,
                CharsOption::Fillchars,
                opt_flags,
                errbuf.as_mut(),
            )
        } else if std::ptr::eq(varp, std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_lcs)) {
            set_chars_option(&mut *win, &val, CharsOption::Listchars, true, errbuf.as_mut())
        } else if std::ptr::eq(varp, std::ptr::addr_of_mut!((*win).w_onebuf_opt.wo_fcs)) {
            set_chars_option(&mut *win, &val, CharsOption::Fillchars, true, errbuf.as_mut())
        } else {
            // The original leaves `errmsg` NULL when `varp` matches
            // none of the four - there is no final `else`.
            None
        }
    };

    args.os_errbuf = errbuf;
    errmsg
}

/// The `'ambiwidth'` option is changed (`did_set_ambiwidth`).
///
/// # Safety
/// Forwarded from `did_set_str_generic`'s and `check_chars_options`'s
/// own safety docs.
pub unsafe fn did_set_ambiwidth(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    let errmsg = unsafe { did_set_str_generic(args) };
    if errmsg.is_some() {
        return errmsg;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_chars_options() }
}

/// The `'background'` option is changed (`did_set_background`).
///
/// Validation and unchanged-value detection are complete. A real
/// light/dark transition still needs highlight initialization and
/// colorscheme-variable handling.
///
/// # Safety
/// Forwarded from [`did_set_str_generic`].
pub unsafe fn did_set_background(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    if let Some(error) = unsafe { did_set_str_generic(args) } {
        return Some(error);
    }

    let old = match &args.os_oldval {
        crate::option_defs::OptVal::String(value) => value.first().copied().unwrap_or(0),
        _ => 0,
    };
    let current = unsafe { (*crate::option_vars::OPTION_VARS.as_ptr()).p_bg.as_deref() }
        .and_then(|value| value.first().copied())
        .unwrap_or(0);
    if old == current {
        return None;
    }

    unimplemented!(
        "did_set_background: a light/dark transition needs highlight initialization"
    );
}

/// The `'emoji'` option is changed (`did_set_emoji`).
///
/// Note this validates `'ambiwidth'`, not `'emoji'` itself - both
/// feed the same character-width tables, so a change to either has to
/// be re-checked against the current `'ambiwidth'` value.
///
/// # Safety
/// Forwarded from `check_str_opt`'s and `check_chars_options`'s own
/// safety docs.
pub unsafe fn did_set_emoji(_args: &mut crate::option_defs::OptsetT) -> Option<&'static [u8]> {
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { check_str_opt(crate::option_defs::OptIndex::Ambiwidth, None) } {
        return Some(crate::errors::e_invarg.as_bytes());
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_chars_options() }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BackgroundGuard(Option<Vec<u8>>);

    impl BackgroundGuard {
        fn set(value: &[u8]) -> Self {
            let options = crate::option_vars::OPTION_VARS.as_ptr();
            let previous = unsafe { (*options).p_bg.replace(value.to_vec()) };
            BackgroundGuard(previous)
        }
    }

    impl Drop for BackgroundGuard {
        fn drop(&mut self) {
            unsafe { (*crate::option_vars::OPTION_VARS.as_ptr()).p_bg = self.0.take() };
        }
    }

    fn background_args(old: &[u8]) -> crate::option_defs::OptsetT {
        let options = crate::option_vars::OPTION_VARS.as_ptr();
        crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Background,
            os_varp: unsafe { std::ptr::addr_of_mut!((*options).p_bg) }.cast(),
            os_oldval: crate::option_defs::OptVal::String(old.to_vec()),
            ..Default::default()
        }
    }

    #[test]
    fn did_set_background_unchanged_value_needs_no_highlight_reinitialization() {
        let _lock = crate::globals::global_state_test_lock();
        let _background = BackgroundGuard::set(b"dark");
        let mut args = background_args(b"dark");
        assert_eq!(unsafe { did_set_background(&mut args) }, None);
    }

    #[test]
    fn did_set_background_rejects_an_invalid_value_before_highlight_work() {
        let _lock = crate::globals::global_state_test_lock();
        let _background = BackgroundGuard::set(b"bogus");
        let mut args = background_args(b"dark");
        assert_eq!(
            unsafe { did_set_background(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    #[should_panic(expected = "highlight initialization")]
    fn did_set_background_changed_value_needs_highlight_reinitialization() {
        let _lock = crate::globals::global_state_test_lock();
        let _background = BackgroundGuard::set(b"light");
        let mut args = background_args(b"dark");
        unsafe { did_set_background(&mut args) };
    }

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

    // ---- didset_string_options / did_set_isopt ----

    #[test]
    fn didset_string_options_recomputes_a_flags_var_from_the_live_value() {
        // The whole point of the sweep is its side effect: each
        // listed option's flags bitmask is recomputed from whatever
        // value its global storage currently holds.
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_ssop = opts.p_ssop.clone();
        let prev_flags = opts.ssop_flags;
        opts.p_ssop = Some(b"help,blank".to_vec());
        opts.ssop_flags = 0;

        unsafe { didset_string_options() };

        assert_ne!(
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ssop_flags,
            0,
            "'sessionoptions' flags were recomputed"
        );

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_ssop = prev_ssop;
        opts.ssop_flags = prev_flags;
    }

    #[test]
    fn didset_string_options_ignores_an_invalid_value() {
        // Return values are discarded, matching the original - an
        // invalid value must not panic or abort the sweep.
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_ssop.clone();
        opts.p_ssop = Some(b"bogus".to_vec());

        unsafe { didset_string_options() };

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ssop = prev;
    }

    #[test]
    fn did_set_opt_flags_stores_the_bitmask_on_success() {
        // The wrapper's whole job is turning opt_strings_flags'
        // success into "no error" plus a stored bitmask.
        let values = ["one", "two", "three"];
        let mut flags = 0u32;
        assert_eq!(did_set_opt_flags(b"two", &values, &mut flags, false), None);
        assert_eq!(flags, 1 << 1);
    }

    #[test]
    fn did_set_opt_flags_leaves_the_bitmask_untouched_on_failure() {
        // An unknown value must report E474 AND leave the caller's
        // existing flags alone, exactly as upstream does by only
        // writing through flagp on success.
        let values = ["one", "two"];
        let mut flags = 0xdead_u32;
        assert_eq!(
            did_set_opt_flags(b"nope", &values, &mut flags, false),
            Some(crate::errors::e_invarg.as_bytes())
        );
        assert_eq!(flags, 0xdead, "flags must survive a rejected value");
    }

    #[test]
    fn did_set_opt_flags_list_true_accepts_a_comma_separated_value() {
        let values = ["one", "two", "three"];
        let mut flags = 0u32;
        assert_eq!(did_set_opt_flags(b"one,three", &values, &mut flags, true), None);
        assert_eq!(flags, (1 << 0) | (1 << 2));
    }

    #[test]
    fn did_set_iskeyword_global_only_validates() {
        // The GLOBAL value is the template new buffers inherit, so it
        // is validated but never applied - no buffer chartab is
        // refilled and no restore is ever requested.
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_isk.clone();

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_isk =
            Some(b"@,48-57,_,192-255".to_vec());
        let varp = std::ptr::from_mut(
            &mut unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_isk,
        );
        let mut args = crate::option_defs::OptsetT {
            os_varp: varp as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_iskeyword(&mut args) }, None);
        assert!(!args.os_restore_chartab, "the global path never refills a chartab");

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_isk = prev;
    }

    #[test]
    fn did_set_iskeyword_global_rejects_an_invalid_value() {
        // Cross-verified against real nvim: an unparseable value is
        // rejected and the previous one kept.
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_isk.clone();

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_isk =
            Some(b"@@@bogus###".to_vec());
        let varp = std::ptr::from_mut(
            &mut unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_isk,
        );
        let mut args = crate::option_defs::OptsetT {
            os_varp: varp as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_iskeyword(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_isk = prev;
    }

    #[test]
    fn did_set_iskeyword_local_falls_through_to_did_set_isopt() {
        // A buffer-LOCAL value does refill that buffer's chartab, so
        // the local path must reach did_set_isopt rather than stopping
        // at the global validation.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_isk: Some(b"@,48-57,_,192-255".to_vec()),
            ..Default::default()
        };
        // os_varp points at the BUFFER's own value, not the global one.
        let varp = std::ptr::from_mut(&mut buf.b_p_isk);
        let mut args = crate::option_defs::OptsetT {
            os_varp: varp as *mut c_void,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_iskeyword(&mut args) }, None);
        assert!(!args.os_restore_chartab);
        // The chartab really was refilled from the local value.
        assert!(buf.b_chartab.iter().any(|&w| w != 0));
    }

    #[test]
    fn did_set_isopt_accepts_a_valid_iskeyword_value() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_isk: Some(b"@,48-57,_,192-255".to_vec()),
            ..Default::default()
        };
        let mut args = crate::option_defs::OptsetT {
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_isopt(&mut args) }, None);
        assert!(!args.os_restore_chartab, "no restore needed on success");
    }

    #[test]
    fn did_set_isopt_rejects_a_bad_value_and_asks_for_a_restore() {
        let _lock = crate::globals::global_state_test_lock();
        // A reversed range is invalid, so buf_init_chartab FAILs.
        let mut buf = crate::buffer_defs::BufT {
            b_p_isk: Some(b"200-100".to_vec()),
            ..Default::default()
        };
        let mut args = crate::option_defs::OptsetT {
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_isopt(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
        assert!(args.os_restore_chartab, "the caller must put the old value back");
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

    #[test]
    fn did_set_display_rejects_an_invalid_value_before_rebuilding_chartab() {
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: OptIndex::Display,
            os_varp: &mut val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };

        assert_eq!(
            unsafe { did_set_display(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_display_rebuilds_the_current_buffers_chartab() {
        struct CurbufRestore(*mut crate::buffer_defs::BufT);

        impl Drop for CurbufRestore {
            fn drop(&mut self) {
                unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.0;
            }
        }

        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_p_isk: Some(b"a".to_vec()),
            ..Default::default()
        };
        let bufp = &mut buf as *mut crate::buffer_defs::BufT;
        let old_curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let _restore = CurbufRestore(old_curbuf);
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = bufp;

        let mut val: Option<Vec<u8>> = Some(b"lastline".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: OptIndex::Display,
            os_varp: &mut val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_display(&mut args) }, None);
        let bit = 1u64 << (u32::from(b'a') & 0x3f);
        assert_ne!(unsafe { (*bufp).b_chartab[usize::from(b'a' >> 6)] } & bit, 0);
    }

    #[test]
    fn did_set_eventignore_accepts_global_events_and_aliases() {
        let mut value = Some(b"ColorScheme,FileEncoding".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Eventignore,
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_eventignore(&mut args) }, None);
    }

    #[test]
    fn did_set_eventignorewin_accepts_only_window_local_events() {
        let mut value = Some(b"BufEnter".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Eventignorewin,
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_eventignore(&mut args) }, None);

        let mut invalid = Some(b"ColorScheme".to_vec());
        let mut invalid_args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Eventignorewin,
            os_varp: &mut invalid as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_eventignore(&mut invalid_args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_eventignore_rejects_an_unknown_event() {
        let mut value = Some(b"NotAnEvent".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Eventignore,
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_eventignore(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    struct DiffoptCallbackGuard {
        value: Option<Vec<u8>>,
        flags: i32,
        context: i32,
        linematch: i32,
        foldcolumn: i32,
        algorithm: i32,
    }

    impl DiffoptCallbackGuard {
        fn set(value: &[u8]) -> Self {
            let old = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
                .p_dip
                .replace(value.to_vec());
            Self {
                value: old,
                flags: *unsafe { crate::diff::DIFF_FLAGS.get_mut() },
                context: *unsafe { crate::diff::DIFF_CONTEXT.get_mut() },
                linematch: *unsafe { crate::diff::LINEMATCH_LINES.get_mut() },
                foldcolumn: *unsafe { crate::diff::DIFF_FOLDCOLUMN.get_mut() },
                algorithm: *unsafe { crate::diff::DIFF_ALGORITHM.get_mut() },
            }
        }
    }

    impl Drop for DiffoptCallbackGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_dip = self.value.take();
            *unsafe { crate::diff::DIFF_FLAGS.get_mut() } = self.flags;
            *unsafe { crate::diff::DIFF_CONTEXT.get_mut() } = self.context;
            *unsafe { crate::diff::LINEMATCH_LINES.get_mut() } = self.linematch;
            *unsafe { crate::diff::DIFF_FOLDCOLUMN.get_mut() } = self.foldcolumn;
            *unsafe { crate::diff::DIFF_ALGORITHM.get_mut() } = self.algorithm;
        }
    }

    #[test]
    fn did_set_diffopt_accepts_and_applies_a_valid_value() {
        let _lock = crate::globals::global_state_test_lock();
        let _tabs = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.first_tabpage,
                std::ptr::null_mut(),
            )
        };
        let _state = DiffoptCallbackGuard::set(
            b"internal,filler,context:3,algorithm:minimal,inline:simple",
        );
        assert_eq!(
            did_set_diffopt(&mut crate::option_defs::OptsetT::default()),
            None
        );
        assert_eq!(*unsafe { crate::diff::DIFF_CONTEXT.get_mut() }, 3);
        assert_eq!(
            *unsafe { crate::diff::DIFF_ALGORITHM.get_mut() },
            crate::diff::xdf_flag::NEED_MINIMAL
        );
    }

    #[test]
    fn did_set_diffopt_rejects_an_invalid_value() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = DiffoptCallbackGuard::set(b"horizontal,vertical");
        assert_eq!(
            did_set_diffopt(&mut crate::option_defs::OptsetT::default()),
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_encoding_canonicalizes_utf8() {
        let mut value = Some(b"UTF8".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Encoding,
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_encoding(&mut args) }, None);
        assert_eq!(value, Some(b"utf-8".to_vec()));
    }

    #[test]
    fn did_set_encoding_rejects_non_utf8_after_canonicalizing_it() {
        let mut value = Some(b"ISO88591".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Encoding,
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_encoding(&mut args) },
            Some(crate::errors::e_unsupportedoption.as_bytes())
        );
        assert_eq!(value, Some(b"latin1".to_vec()));
    }

    #[test]
    fn did_set_fileencoding_rejects_a_comma() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_ma: 1,
            ..Default::default()
        };
        let mut value = Some(b"utf-8,latin1".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Fileencoding,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_encoding(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_fileencoding_requires_a_modifiable_buffer_for_local_changes() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut value = Some(b"utf8".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Fileencoding,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_encoding(&mut args) },
            Some(crate::errors::e_modifiable.as_bytes())
        );
        assert_eq!(value, Some(b"utf8".to_vec()));
    }

    #[test]
    fn did_set_fileencoding_allows_a_global_change_on_an_unmodifiable_buffer() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut value = Some(b"UTF16LE".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Fileencoding,
            os_flags: crate::option_defs::opt_set_flags::OPT_GLOBAL as i32,
            os_buf: &mut buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_encoding(&mut args) }, None);
        assert_eq!(value, Some(b"utf-16le".to_vec()));
    }

    #[test]
    fn did_set_makeencoding_canonicalizes_aliases_without_a_buffer() {
        let mut value = Some(b"mac-roman".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Makeencoding,
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_encoding(&mut args) }, None);
        assert_eq!(value, Some(b"macroman".to_vec()));
    }

    #[test]
    fn expand_set_encoding_returns_every_canonical_name_without_a_filter() {
        let mut args = crate::option_defs::OptexpandT::default();
        let matches = expand_set_encoding(&mut args).expect("encoding matches");
        assert_eq!(matches.len(), crate::mbyte::ENC_CANON_TABLE.len());
        assert_eq!(matches.first().map(Vec::as_slice), Some(b"latin1".as_slice()));
        assert_eq!(
            matches.last().map(Vec::as_slice),
            Some(b"hp-roman8".as_slice())
        );
    }

    #[test]
    #[should_panic(expected = "regexp engine")]
    fn expand_set_encoding_filtered_completion_needs_the_regexp_engine() {
        let mut args = crate::option_defs::OptexpandT {
            oe_regmatch: std::ptr::NonNull::dangling().as_ptr(),
            ..Default::default()
        };
        let _ = expand_set_encoding(&mut args);
    }

    #[test]
    fn expand_set_str_generic_uses_the_fileformat_and_sessionoption_value_lists() {
        let mut fileformats = crate::option_defs::OptexpandT {
            oe_idx: crate::option_defs::OptIndex::Fileformats,
            ..Default::default()
        };
        assert_eq!(
            expand_set_str_generic(&mut fileformats),
            Some(vec![
                b"unix".to_vec(),
                b"dos".to_vec(),
                b"mac".to_vec(),
            ])
        );

        let mut viewoptions = crate::option_defs::OptexpandT {
            oe_idx: crate::option_defs::OptIndex::Viewoptions,
            ..Default::default()
        };
        let values = expand_set_str_generic(&mut viewoptions).expect("viewoption values");
        assert!(values.iter().any(|value| value == b"folds"));
        assert!(values.iter().any(|value| value == b"cursor"));
        assert!(values.iter().any(|value| value == b"curdir"));
    }

    #[test]
    #[should_panic(expected = "regexp engine")]
    fn expand_set_str_generic_filtered_completion_needs_the_regexp_engine() {
        let mut args = crate::option_defs::OptexpandT {
            oe_idx: crate::option_defs::OptIndex::Fileformats,
            oe_regmatch: std::ptr::NonNull::dangling().as_ptr(),
            ..Default::default()
        };
        let _ = expand_set_str_generic(&mut args);
    }

    #[test]
    fn expand_set_opt_listflag_filters_used_and_typed_flags() {
        let args = crate::option_defs::OptexpandT {
            oe_opt_value: Some(b"ni".to_vec()),
            oe_append: true,
            oe_set_arg: Some(b"v".to_vec()),
            ..Default::default()
        };
        assert_eq!(
            expand_set_opt_listflag(&args, b"nvic"),
            Some(vec![b"c".to_vec()])
        );
    }

    #[test]
    fn expand_set_opt_listflag_includes_original_without_duplicating_a_single_flag() {
        let args = crate::option_defs::OptexpandT {
            oe_opt_value: Some(b"n".to_vec()),
            oe_include_orig_val: true,
            ..Default::default()
        };
        assert_eq!(
            expand_set_opt_listflag(&args, b"nvic"),
            Some(vec![
                b"n".to_vec(),
                b"v".to_vec(),
                b"i".to_vec(),
                b"c".to_vec(),
            ])
        );
    }

    #[test]
    fn expand_set_opt_listflag_fails_when_every_flag_is_already_present() {
        let args = crate::option_defs::OptexpandT {
            oe_set_arg: Some(b"nvic".to_vec()),
            ..Default::default()
        };
        assert_eq!(expand_set_opt_listflag(&args, b"nvic"), None);
    }

    #[test]
    fn expand_set_concealcursor_returns_each_valid_flag() {
        let mut args = crate::option_defs::OptexpandT::default();
        assert_eq!(
            expand_set_concealcursor(&mut args),
            Some(vec![
                b"n".to_vec(),
                b"v".to_vec(),
                b"i".to_vec(),
                b"c".to_vec(),
            ])
        );
    }

    struct MessagesoptValueGuard(Option<Vec<u8>>);

    impl MessagesoptValueGuard {
        fn set(value: &[u8]) -> Self {
            let old = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
                .p_mopt
                .replace(value.to_vec());
            Self(old)
        }
    }

    impl Drop for MessagesoptValueGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mopt = self.0.take();
        }
    }

    #[test]
    fn did_set_messagesopt_accepts_the_default_shape() {
        let _lock = crate::globals::global_state_test_lock();
        let _value = MessagesoptValueGuard::set(b"hit-enter,history:500,progress:c");
        assert_eq!(
            did_set_messagesopt(&mut crate::option_defs::OptsetT::default()),
            None
        );
    }

    #[test]
    fn did_set_messagesopt_rejects_a_value_without_history() {
        let _lock = crate::globals::global_state_test_lock();
        let _value = MessagesoptValueGuard::set(b"wait:10");
        assert_eq!(
            did_set_messagesopt(&mut crate::option_defs::OptsetT::default()),
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_cinoptions_rebuilds_the_buffers_indent_cache() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_sw: 4,
            b_p_ts: 8,
            b_p_cino: Some(b">2s,e-s".to_vec()),
            ..Default::default()
        };
        let bufp = &mut buf as *mut crate::buffer_defs::BufT;
        let mut args = crate::option_defs::OptsetT {
            os_buf: bufp as *mut c_void,
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_cinoptions(&mut args) }, None);
        assert_eq!(unsafe { (*bufp).b_ind_level }, 8);
        assert_eq!(unsafe { (*bufp).b_ind_open_imag }, -4);
    }

    #[test]
    fn did_set_cinoptions_restores_defaults_for_an_empty_value() {
        let mut buf = crate::buffer_defs::BufT {
            b_p_sw: 3,
            b_p_ts: 8,
            b_p_cino: Some(Vec::new()),
            b_ind_level: 99,
            b_ind_jump_label: 99,
            ..Default::default()
        };
        let bufp = &mut buf as *mut crate::buffer_defs::BufT;
        let mut args = crate::option_defs::OptsetT {
            os_buf: bufp as *mut c_void,
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_cinoptions(&mut args) }, None);
        assert_eq!(unsafe { (*bufp).b_ind_level }, 3);
        assert_eq!(unsafe { (*bufp).b_ind_jump_label }, -1);
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
        assert_eq!(
            did_set_backupext_or_patchmode(&mut Default::default()),
            None
        );
        restore_bex_pm(prev);
    }

    #[test]
    fn did_set_backupext_or_patchmode_identical_suffixes_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_bex_pm(Some(b".bak"), Some(b".bak"));
        assert!(did_set_backupext_or_patchmode(&mut Default::default()).is_some());
        restore_bex_pm(prev);
    }

    #[test]
    fn did_set_backupext_or_patchmode_leading_dot_is_stripped_before_comparing() {
        let _lock = crate::globals::global_state_test_lock();
        // ".bak" (patchmode) and "bak" (backupext, no leading dot) both
        // reduce to the same "bak" suffix once the shared leading '.'
        // is stripped from whichever side has one.
        let prev = set_bex_pm(Some(b"bak"), Some(b".bak"));
        assert!(did_set_backupext_or_patchmode(&mut Default::default()).is_some());
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

        assert_eq!(unsafe { did_set_helpfile(&mut Default::default()) }, None);

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

        assert_eq!(unsafe { did_set_helpfile(&mut Default::default()) }, None);

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
        assert_eq!(did_set_helplang(&mut Default::default()), None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_single_two_letter_code_is_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_hlg(Some(b"ab"));
        assert_eq!(did_set_helplang(&mut Default::default()), None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_comma_separated_codes_are_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_hlg(Some(b"ab,cd,ef"));
        assert_eq!(did_set_helplang(&mut Default::default()), None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_single_leftover_byte_is_invalid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_hlg(Some(b"a"));
        assert_eq!(
            did_set_helplang(&mut Default::default()),
            Some(crate::errors::e_invarg.as_bytes())
        );
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_trailing_comma_with_nothing_after_is_invalid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_hlg(Some(b"ab,"));
        assert_eq!(
            did_set_helplang(&mut Default::default()),
            Some(crate::errors::e_invarg.as_bytes())
        );
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_third_byte_not_a_comma_is_invalid() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_hlg(Some(b"abc"));
        assert_eq!(
            did_set_helplang(&mut Default::default()),
            Some(crate::errors::e_invarg.as_bytes())
        );
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_hlg = prev;
    }

    #[test]
    fn did_set_helplang_middle_code_missing_second_letter_is_invalid() {
        let _lock = crate::globals::global_state_test_lock();
        // "ab,c" - the 2nd code's own 2nd letter is the terminator.
        let prev = set_p_hlg(Some(b"ab,c"));
        assert_eq!(
            did_set_helplang(&mut Default::default()),
            Some(crate::errors::e_invarg.as_bytes())
        );
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

        let mut win = crate::buffer_defs::WinT { w_height: 12, w_prev_height: 0, w_next: std::ptr::null_mut(), ..Default::default() };
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;

        let mut _ft = unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.first_tabpage, tp_ptr) };

        let mut _ct = unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curtab, tp_ptr) };

        let mut _fw = unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.firstwin, win_ptr) };

        let mut val: Option<Vec<u8>> = Some(b"cursor".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Splitkeep, os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_splitkeep(&mut args) }, None);
        assert_eq!(unsafe { &*win_ptr }.w_prev_height, 12);

        _ft.restore_now();

        _ct.restore_now();

        _fw.restore_now();
    }

    #[test]
    fn did_set_splitkeep_snapshots_a_non_current_tabpage_via_its_own_tp_firstwin() {
        let _lock = crate::globals::global_state_test_lock();

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
        let mut _ft = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.first_tabpage, curtab_ptr)
        };
        let mut _ct =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curtab, curtab_ptr) };
        // The original test set firstwin and never restored it, leaving
        // it clobbered for every later test; the guard fixes that.
        let mut _fw = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.firstwin, std::ptr::null_mut())
        };

        let mut val: Option<Vec<u8>> = Some(b"screen".to_vec());
        let varp = &mut val as *mut Option<Vec<u8>> as *mut c_void;
        let mut args = crate::option_defs::OptsetT { os_idx: OptIndex::Splitkeep, os_varp: varp, ..Default::default() };
        assert_eq!(unsafe { did_set_splitkeep(&mut args) }, None);
        assert_eq!(unsafe { &*other_win_ptr }.w_prev_height, 33);

        _ft.restore_now();
        _ct.restore_now();
        _fw.restore_now();
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
        assert_eq!(unsafe { did_set_spellsuggest(&mut Default::default()) }, None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_sps = prev;
    }

    #[test]
    fn did_set_spellsuggest_invalid_value_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_sps(Some(b"bogus"));
        assert_eq!(
            unsafe { did_set_spellsuggest(&mut Default::default()) },
            Some(crate::errors::e_invarg.as_bytes())
        );
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
        assert_eq!(unsafe { did_set_mkspellmem(&mut Default::default()) }, None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_msm = prev;
    }

    #[test]
    fn did_set_mkspellmem_invalid_value_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_msm(Some(b"bogus"));
        assert_eq!(
            unsafe { did_set_mkspellmem(&mut Default::default()) },
            Some(crate::errors::e_invarg.as_bytes())
        );
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

    // ---- did_set_foldignore / did_set_foldmarker ----

    fn fold_args(
        win: &mut crate::buffer_defs::WinT,
        val: &mut Option<Vec<u8>>,
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        }
    }

    #[test]
    fn did_set_foldignore_is_a_no_op_for_a_non_indent_foldmethod() {
        // Default 'foldmethod' is "manual", so foldmethod_is_indent is
        // false and the fold-update branch is never reached.
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"#".to_vec());
        let mut args = fold_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldignore(&mut args) }, None);
    }

    #[test]
    fn did_set_foldignore_accepts_any_value_including_empty() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = fold_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldignore(&mut args) }, None);
    }

    #[test]
    fn did_set_foldignore_invalidates_the_folds_when_foldmethod_is_indent() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_fdm = Some(b"indent".to_vec());
        let mut val: Option<Vec<u8>> = Some(b"#".to_vec());
        let mut args = fold_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldignore(&mut args) }, None);
        assert!(win.w_foldinvalid, "foldUpdateAll ran");
    }

    #[test]
    fn did_set_foldmarker_accepts_the_real_default_value() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"{{{,}}}".to_vec());
        let mut args = fold_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldmarker(&mut args) }, None);
    }

    #[test]
    fn did_set_foldmarker_requires_a_comma() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"nocomma".to_vec());
        let mut args = fold_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldmarker(&mut args) }, Some(e_comma_required.as_bytes()));
    }

    #[test]
    fn did_set_foldmarker_rejects_an_empty_start_marker() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b",}}}".to_vec());
        let mut args = fold_args(&mut win, &mut val);
        assert_eq!(
            unsafe { did_set_foldmarker(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_foldmarker_rejects_an_empty_end_marker() {
        // The comma is the last byte: the original reads `p[1]` and
        // finds its own NUL terminator; here that's an explicit
        // "comma is the last byte" bounds check.
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"{{{,".to_vec());
        let mut args = fold_args(&mut win, &mut val);
        assert_eq!(
            unsafe { did_set_foldmarker(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_foldmarker_uses_only_the_first_comma() {
        // vim_strchr finds the FIRST comma; everything after it is the
        // end marker, commas included.
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"a,b,c".to_vec());
        let mut args = fold_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldmarker(&mut args) }, None);
    }

    #[test]
    fn did_set_foldmarker_invalidates_the_folds_when_foldmethod_is_marker() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_fdm = Some(b"marker".to_vec());
        let mut val: Option<Vec<u8>> = Some(b"{{{,}}}".to_vec());
        let mut args = fold_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldmarker(&mut args) }, None);
        assert!(win.w_foldinvalid, "foldUpdateAll ran");
    }

    #[test]
    fn did_set_foldmarker_validates_before_the_foldmethod_check() {
        // An invalid value must be rejected (returned, not panicked)
        // even when 'foldmethod' is "marker" - matching the original's
        // own ordering, where both validation checks precede the
        // foldmethodIsMarker branch entirely.
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_fdm = Some(b"marker".to_vec());
        let mut val: Option<Vec<u8>> = Some(b"nocomma".to_vec());
        let mut args = fold_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldmarker(&mut args) }, Some(e_comma_required.as_bytes()));
    }

    // ---- get_fileformat_name / get_fillchars_name / get_listchars_name ----

    #[test]
    fn get_fileformat_name_walks_the_value_list_then_stops() {
        // Cross-verified against real nvim: 'fileformat' accepts
        // exactly unix/dos/mac and rejects anything else.
        assert_eq!(get_fileformat_name(0), Some("unix"));
        assert_eq!(get_fileformat_name(1), Some("dos"));
        assert_eq!(get_fileformat_name(2), Some("mac"));
        assert_eq!(get_fileformat_name(3), None);
    }

    #[test]
    fn get_fileformat_name_rejects_a_negative_index() {
        assert_eq!(get_fileformat_name(-1), None);
    }

    #[test]
    fn get_fillchars_name_walks_the_field_table_then_stops() {
        assert_eq!(get_fillchars_name(0), Some("stl"));
        assert_eq!(get_fillchars_name(1), Some("stlnc"));
        assert_eq!(
            get_fillchars_name((FCS_TAB.len() - 1) as i32),
            Some(FCS_TAB[FCS_TAB.len() - 1].name)
        );
        assert_eq!(get_fillchars_name(FCS_TAB.len() as i32), None);
        assert_eq!(get_fillchars_name(-1), None);
    }

    #[test]
    fn get_listchars_name_walks_the_field_table_then_stops() {
        assert_eq!(get_listchars_name(0), Some("eol"));
        assert_eq!(get_listchars_name(1), Some("extends"));
        // The two trailing pseudo-fields are listed for completion
        // even though they map to CharsField::None.
        assert_eq!(get_listchars_name(10), Some("multispace"));
        assert_eq!(get_listchars_name(11), Some("leadmultispace"));
        assert_eq!(get_listchars_name(LCS_TAB.len() as i32), None);
        assert_eq!(get_listchars_name(-1), None);
    }

    // ---- did_set_foldmethod ----

    fn foldmethod_args(
        win: &mut crate::buffer_defs::WinT,
        val: &mut Option<Vec<u8>>,
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_idx: OptIndex::Foldmethod,
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        }
    }

    #[test]
    fn did_set_foldmethod_accepts_every_documented_value() {
        // Cross-verified against real nvim: `set foldmethod=indent`
        // is accepted and reported back by &foldmethod.
        for m in [
            &b"manual"[..],
            &b"indent"[..],
            &b"expr"[..],
            &b"marker"[..],
            &b"syntax"[..],
            &b"diff"[..],
        ] {
            let mut win = crate::buffer_defs::WinT::default();
            let mut val: Option<Vec<u8>> = Some(m.to_vec());
            let mut args = foldmethod_args(&mut win, &mut val);
            assert_eq!(
                unsafe { did_set_foldmethod(&mut args) },
                None,
                "{} must be accepted",
                String::from_utf8_lossy(m)
            );
        }
    }

    #[test]
    fn did_set_foldmethod_invalidates_the_folds_unconditionally() {
        // Unlike its siblings this one is NOT gated on the method: the
        // whole tree was built by the previous method, so it always
        // has to go.
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"indent".to_vec());
        let mut args = foldmethod_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldmethod(&mut args) }, None);
        assert!(win.w_foldinvalid, "foldUpdateAll ran");
    }

    #[test]
    fn did_set_foldmethod_rejects_an_unknown_value_without_touching_the_folds() {
        // Cross-verified against real nvim: `set foldmethod=bogus`
        // leaves the previous value in place.
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let mut args = foldmethod_args(&mut win, &mut val);
        assert_eq!(
            unsafe { did_set_foldmethod(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
        assert!(
            !win.w_foldinvalid,
            "a rejected value must not invalidate the fold tree"
        );
    }

    // ---- did_set_shortmess / did_set_cpoptions ----

    fn listflag_args(val: &mut Option<Vec<u8>>) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT { os_varp: val as *mut Option<Vec<u8>> as *mut c_void, ..Default::default() }
    }

    #[test]
    fn did_set_shortmess_accepts_every_character_in_shm_all() {
        let mut val: Option<Vec<u8>> = Some(SHM_ALL.to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_shortmess(&mut args) }, None);
    }

    #[test]
    fn did_set_shortmess_accepts_the_real_default_value() {
        let mut val: Option<Vec<u8>> = Some(b"filnxtToOCF".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_shortmess(&mut args) }, None);
    }

    #[test]
    fn did_set_shortmess_empty_is_valid() {
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_shortmess(&mut args) }, None);
    }

    #[test]
    fn did_set_shortmess_rejects_an_unknown_flag_character() {
        // 'z' is genuinely absent from SHM_ALL.
        let mut val: Option<Vec<u8>> = Some(b"az".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(
            unsafe { did_set_shortmess(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn shm_all_contains_the_four_bare_literals_from_the_original() {
        // The original's own SHM_ALL array ends with 'n', 'f', 'x',
        // 'i' as bare character literals with no SHM_* constant.
        for c in *b"nfxi" {
            assert!(SHM_ALL.contains(&c), "SHM_ALL missing {}", c as char);
        }
    }

    #[test]
    fn did_set_cpoptions_accepts_the_full_vi_default() {
        let mut val: Option<Vec<u8>> = Some(crate::option_vars::CPO_VI.as_bytes().to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_cpoptions(&mut args) }, None);
    }

    #[test]
    fn did_set_cpoptions_accepts_the_vim_subset() {
        // CPO_VIM's own characters are all drawn from CPO_VI.
        let mut val: Option<Vec<u8>> = Some(crate::option_vars::CPO_VIM.as_bytes().to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_cpoptions(&mut args) }, None);
    }

    #[test]
    fn did_set_cpoptions_empty_is_valid() {
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_cpoptions(&mut args) }, None);
    }

    #[test]
    fn did_set_cpoptions_rejects_an_unknown_flag_character() {
        // 'g' is genuinely absent from CPO_VI.
        let mut val: Option<Vec<u8>> = Some(b"ag".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(
            unsafe { did_set_cpoptions(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    // ---- did_set_breakat ----

    fn set_p_breakat(value: Option<&[u8]>) -> Option<Vec<u8>> {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_breakat.clone();
        opts.p_breakat = value.map(<[u8]>::to_vec);
        prev
    }

    /// Restores both the option string and the derived flags table,
    /// so these tests can't leak state into any other test.
    fn restore_breakat(prev_val: Option<Vec<u8>>, prev_flags: [u8; 256]) {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_breakat = prev_val;
        opts.breakat_flags = prev_flags;
    }

    #[test]
    fn did_set_breakat_sets_one_flag_per_character_of_the_real_default() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.breakat_flags;
        let prev = set_p_breakat(Some(b" \t!@*-+;:,./?"));

        assert_eq!(did_set_breakat(&mut Default::default()), None);

        let flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.breakat_flags;
        for b in *b" \t!@*-+;:,./?" {
            assert_eq!(flags[b as usize], 1, "byte {b:?} should be a break character");
        }
        // A character genuinely absent from the default value.
        assert_eq!(flags[b'a' as usize], 0);

        restore_breakat(prev, prev_flags);
    }

    #[test]
    fn did_set_breakat_clears_previously_set_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.breakat_flags;
        let prev = set_p_breakat(Some(b"abc"));
        assert_eq!(did_set_breakat(&mut Default::default()), None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.breakat_flags[b'a' as usize], 1);

        // Setting a disjoint value must clear every previous flag -
        // the original rebuilds the whole table from scratch.
        set_p_breakat(Some(b"xyz"));
        assert_eq!(did_set_breakat(&mut Default::default()), None);
        let flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.breakat_flags;
        assert_eq!(flags[b'a' as usize], 0);
        assert_eq!(flags[b'x' as usize], 1);

        restore_breakat(prev, prev_flags);
    }

    #[test]
    fn did_set_breakat_empty_value_clears_every_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.breakat_flags;
        let prev = set_p_breakat(Some(b"abc"));
        assert_eq!(did_set_breakat(&mut Default::default()), None);

        set_p_breakat(Some(b""));
        assert_eq!(did_set_breakat(&mut Default::default()), None);
        assert!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.breakat_flags.iter().all(|&f| f == 0));

        restore_breakat(prev, prev_flags);
    }

    #[test]
    fn did_set_breakat_none_value_clears_every_flag() {
        // Matches the original's own `if (p_breakat != NULL)` guard:
        // an absent value leaves the freshly-cleared table untouched.
        let _lock = crate::globals::global_state_test_lock();
        let prev_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.breakat_flags;
        let prev = set_p_breakat(Some(b"abc"));
        assert_eq!(did_set_breakat(&mut Default::default()), None);

        set_p_breakat(None);
        assert_eq!(did_set_breakat(&mut Default::default()), None);
        assert!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.breakat_flags.iter().all(|&f| f == 0));

        restore_breakat(prev, prev_flags);
    }

    #[test]
    fn did_set_breakat_handles_high_bytes_without_panicking() {
        // Every byte value is a valid index into the 256-entry table.
        let _lock = crate::globals::global_state_test_lock();
        let prev_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.breakat_flags;
        let prev = set_p_breakat(Some(&[0x00, 0x7F, 0x80, 0xFF]));

        assert_eq!(did_set_breakat(&mut Default::default()), None);

        let flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.breakat_flags;
        for b in [0x00u8, 0x7F, 0x80, 0xFF] {
            assert_eq!(flags[b as usize], 1, "byte {b:#04x} should be set");
        }

        restore_breakat(prev, prev_flags);
    }

    // ---- did_set_cursorlineopt / did_set_inccommand ----

    fn culopt_args(
        win: &mut crate::buffer_defs::WinT,
        val: &mut Option<Vec<u8>>,
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        }
    }

    #[test]
    fn did_set_cursorlineopt_accepts_the_real_default_value() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"both".to_vec());
        let mut args = culopt_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_cursorlineopt(&mut args) }, None);
    }

    #[test]
    fn did_set_cursorlineopt_accepts_a_comma_separated_list() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"line,number".to_vec());
        let mut args = culopt_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_cursorlineopt(&mut args) }, None);
    }

    #[test]
    fn did_set_cursorlineopt_rejects_an_empty_value() {
        // The original's own `**varp == NUL` guard - an empty
        // 'cursorlineopt' is invalid, unlike most flag-list options.
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = culopt_args(&mut win, &mut val);
        assert_eq!(
            unsafe { did_set_cursorlineopt(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_cursorlineopt_rejects_an_unknown_token() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let mut args = culopt_args(&mut win, &mut val);
        assert_eq!(
            unsafe { did_set_cursorlineopt(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_inccommand_delegates_when_no_preview_is_running() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.cmdpreview;
        unsafe { crate::globals::GLOBALS.get_mut() }.cmdpreview = false;

        let prev_icm = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_icm.clone();
        let opts_ptr = crate::option_vars::OPTION_VARS.as_ptr();
        let varp = unsafe { std::ptr::addr_of_mut!((*opts_ptr).p_icm) };
        unsafe { (*varp) = Some(b"split".to_vec()) };

        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Inccommand,
            os_varp: varp as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_inccommand(&mut args) }, None);

        unsafe { (*varp) = prev_icm };
        unsafe { crate::globals::GLOBALS.get_mut() }.cmdpreview = prev;
    }

    #[test]
    fn did_set_inccommand_refuses_while_a_preview_is_running() {
        // Rejected before did_set_str_generic even runs, so the value
        // itself is irrelevant here.
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.cmdpreview;
        unsafe { crate::globals::GLOBALS.get_mut() }.cmdpreview = true;

        let mut val: Option<Vec<u8>> = Some(b"split".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Inccommand,
            os_varp: &mut val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_inccommand(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );

        unsafe { crate::globals::GLOBALS.get_mut() }.cmdpreview = prev;
    }

    // ---- did_set_backupcopy ----

    /// Saves/restores every piece of shared state `did_set_backupcopy`
    /// can touch, so these tests can't leak into any other test.
    fn with_backupcopy<R>(global: Option<&[u8]>, f: impl FnOnce() -> R) -> R {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_bkc = opts.p_bkc.clone();
        let prev_flags = opts.bkc_flags;
        opts.p_bkc = global.map(<[u8]>::to_vec);

        let result = f();

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_bkc = prev_bkc;
        opts.bkc_flags = prev_flags;
        result
    }

    fn bkc_args(
        buf: &mut crate::buffer_defs::BufT,
        flags: u32,
        oldval: &[u8],
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_buf: buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_flags: flags as i32,
            os_oldval: crate::option_defs::OptVal::String(oldval.to_vec()),
            ..Default::default()
        }
    }

    #[test]
    fn did_set_backupcopy_global_auto_is_accepted() {
        with_backupcopy(Some(b"auto"), || {
            let mut buf = crate::buffer_defs::BufT::default();
            let mut args = bkc_args(&mut buf, 0, b"auto");
            assert_eq!(unsafe { did_set_backupcopy(&mut args) }, None);
            assert_eq!(
                unsafe { crate::option_vars::OPTION_VARS.get_mut() }.bkc_flags,
                crate::option_vars::opt_bkc_flag::AUTO
            );
        });
    }

    #[test]
    fn did_set_backupcopy_accepts_one_exclusive_value_plus_a_modifier() {
        with_backupcopy(Some(b"yes,breaksymlink"), || {
            let mut buf = crate::buffer_defs::BufT::default();
            let mut args = bkc_args(&mut buf, 0, b"auto");
            assert_eq!(unsafe { did_set_backupcopy(&mut args) }, None);
            assert_eq!(
                unsafe { crate::option_vars::OPTION_VARS.get_mut() }.bkc_flags,
                crate::option_vars::opt_bkc_flag::YES
                    | crate::option_vars::opt_bkc_flag::BREAKSYMLINK
            );
        });
    }

    #[test]
    fn did_set_backupcopy_rejects_two_exclusive_values() {
        with_backupcopy(Some(b"yes,no"), || {
            let mut buf = crate::buffer_defs::BufT::default();
            let mut args = bkc_args(&mut buf, 0, b"auto");
            assert_eq!(
                unsafe { did_set_backupcopy(&mut args) },
                Some(crate::errors::e_invarg.as_bytes())
            );
            // The original re-derives the flags from oldval ("auto")
            // on this specific failure, rather than leaving the
            // rejected value's own yes|no bitmask in place.
            assert_eq!(
                unsafe { crate::option_vars::OPTION_VARS.get_mut() }.bkc_flags,
                crate::option_vars::opt_bkc_flag::AUTO
            );
        });
    }

    #[test]
    fn did_set_backupcopy_rejects_zero_exclusive_values() {
        with_backupcopy(Some(b"breaksymlink"), || {
            let mut buf = crate::buffer_defs::BufT::default();
            let mut args = bkc_args(&mut buf, 0, b"yes");
            assert_eq!(
                unsafe { did_set_backupcopy(&mut args) },
                Some(crate::errors::e_invarg.as_bytes())
            );
            assert_eq!(
                unsafe { crate::option_vars::OPTION_VARS.get_mut() }.bkc_flags,
                crate::option_vars::opt_bkc_flag::YES
            );
        });
    }

    #[test]
    fn did_set_backupcopy_rejects_an_unknown_value() {
        with_backupcopy(Some(b"bogus"), || {
            let mut buf = crate::buffer_defs::BufT::default();
            let mut args = bkc_args(&mut buf, 0, b"auto");
            assert_eq!(
                unsafe { did_set_backupcopy(&mut args) },
                Some(crate::errors::e_invarg.as_bytes())
            );
        });
    }

    #[test]
    fn did_set_backupcopy_plain_set_clears_the_buffer_local_flags() {
        with_backupcopy(Some(b"auto"), || {
            let mut buf = crate::buffer_defs::BufT { b_bkc_flags: 0xDEAD, ..Default::default() };
            // opt_flags == 0 means a plain `:set` (neither OPT_LOCAL
            // nor OPT_GLOBAL), which also clears the local flags.
            let mut args = bkc_args(&mut buf, 0, b"auto");
            assert_eq!(unsafe { did_set_backupcopy(&mut args) }, None);
            assert_eq!(buf.b_bkc_flags, 0);
        });
    }

    #[test]
    fn did_set_backupcopy_opt_global_leaves_the_buffer_local_flags_alone() {
        with_backupcopy(Some(b"auto"), || {
            let mut buf = crate::buffer_defs::BufT { b_bkc_flags: 0xDEAD, ..Default::default() };
            let mut args =
                bkc_args(&mut buf, crate::option_defs::opt_set_flags::OPT_GLOBAL, b"auto");
            assert_eq!(unsafe { did_set_backupcopy(&mut args) }, None);
            // OPT_GLOBAL skips the local-clearing branch entirely.
            assert_eq!(buf.b_bkc_flags, 0xDEAD);
        });
    }

    #[test]
    fn did_set_backupcopy_local_empty_resets_to_global() {
        with_backupcopy(Some(b"auto"), || {
            let mut buf = crate::buffer_defs::BufT {
                b_p_bkc: Some(Vec::new()),
                b_bkc_flags: 0xDEAD,
                ..Default::default()
            };
            let mut args =
                bkc_args(&mut buf, crate::option_defs::opt_set_flags::OPT_LOCAL, b"auto");
            assert_eq!(unsafe { did_set_backupcopy(&mut args) }, None);
            assert_eq!(buf.b_bkc_flags, 0);
        });
    }

    #[test]
    fn did_set_backupcopy_local_valid_value_sets_the_buffer_local_flags() {
        with_backupcopy(Some(b"auto"), || {
            let mut buf =
                crate::buffer_defs::BufT { b_p_bkc: Some(b"no".to_vec()), ..Default::default() };
            let mut args =
                bkc_args(&mut buf, crate::option_defs::opt_set_flags::OPT_LOCAL, b"auto");
            assert_eq!(unsafe { did_set_backupcopy(&mut args) }, None);
            assert_eq!(buf.b_bkc_flags, crate::option_vars::opt_bkc_flag::NO);
            // The global flags must be untouched by an OPT_LOCAL call.
            assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.bkc_flags, 0);
        });
    }

    // ---- did_set_shada / did_set_spellfile / did_set_spelllang / did_set_spelloptions ----

    struct ShadaGuard(Option<Vec<u8>>);

    impl ShadaGuard {
        fn set(value: &[u8]) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let old = opts.p_shada.replace(value.to_vec());
            Self(old)
        }
    }

    impl Drop for ShadaGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shada = self.0.take();
        }
    }

    fn check_shada(value: &[u8]) -> Option<&'static [u8]> {
        let _lock = crate::globals::global_state_test_lock();
        let _shada = ShadaGuard::set(value);
        did_set_shada(&mut crate::option_defs::OptsetT::default())
    }

    #[test]
    fn did_set_shada_accepts_empty_and_numbered_parameters() {
        assert_eq!(check_shada(b""), None);
        assert_eq!(check_shada(b"'100,<50,s10,h"), None);
    }

    #[test]
    fn did_set_shada_accepts_optional_percent_and_text_parameters() {
        assert_eq!(check_shada(b"'10,%,r/tmp,nshada.file"), None);
        assert_eq!(check_shada(b"'10,%20,r/tmp"), None);
    }

    #[test]
    fn did_set_shada_stops_parsing_after_the_n_parameter() {
        assert_eq!(check_shada(b"'10,nshada.file,<not-parsed"), None);
    }

    #[test]
    fn did_set_shada_rejects_an_illegal_parameter_character() {
        assert_eq!(
            check_shada(b"'10,x20"),
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_shada_rejects_a_missing_required_number() {
        assert_eq!(
            check_shada(b"',<50"),
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_shada_rejects_a_missing_comma() {
        assert_eq!(
            check_shada(b"'10<50"),
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_shada_requires_the_quote_parameter_when_nonempty() {
        assert_eq!(
            check_shada(b"<50,s10"),
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_spellfile_accepts_add_wordlists() {
        let mut value = Some(b"en.utf-8.add,de.add".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_spellfile(&mut args) }, None);
    }

    #[test]
    fn did_set_spellfile_rejects_a_non_add_file() {
        let mut value = Some(b"spell.txt".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_spellfile(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_spelllang_accepts_language_names_and_punctuation() {
        let mut value = Some(b"en_US.utf-8,de@quot".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_spelllang(&mut args) }, None);
    }

    #[test]
    fn did_set_spelllang_rejects_whitespace() {
        let mut value = Some(b"en US".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_varp: &mut value as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_spelllang(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    /// Saves/restores the global `spo_flags` around a test body.
    fn with_spo_flags<R>(f: impl FnOnce() -> R) -> R {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.spo_flags;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.spo_flags = 0;
        let result = f();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.spo_flags = prev;
        result
    }

    fn spo_args(
        win: &mut crate::buffer_defs::WinT,
        flags: u32,
        newval: &[u8],
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_flags: flags as i32,
            os_newval: crate::option_defs::OptVal::String(newval.to_vec()),
            ..Default::default()
        }
    }

    #[test]
    fn did_set_spelloptions_plain_set_writes_both_slots() {
        with_spo_flags(|| {
            let mut synblock = crate::buffer_defs::SynblockT::default();
            let mut win = crate::buffer_defs::WinT {
                w_s: &mut synblock as *mut crate::buffer_defs::SynblockT,
                ..Default::default()
            };
            // opt_flags == 0: neither OPT_LOCAL nor OPT_GLOBAL, so
            // both inverted guards pass and both slots are written.
            let mut args = spo_args(&mut win, 0, b"camel");
            assert_eq!(unsafe { did_set_spelloptions(&mut args) }, None);

            // "camel" is index 0 in OPT_SPO_VALUES.
            assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.spo_flags, 0x01);
            assert_eq!(synblock.b_p_spo_flags, 0x01);
        });
    }

    #[test]
    fn did_set_spelloptions_opt_local_skips_the_global_slot() {
        with_spo_flags(|| {
            let mut synblock = crate::buffer_defs::SynblockT::default();
            let mut win = crate::buffer_defs::WinT {
                w_s: &mut synblock as *mut crate::buffer_defs::SynblockT,
                ..Default::default()
            };
            let mut args =
                spo_args(&mut win, crate::option_defs::opt_set_flags::OPT_LOCAL, b"camel");
            assert_eq!(unsafe { did_set_spelloptions(&mut args) }, None);

            assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.spo_flags, 0);
            assert_eq!(synblock.b_p_spo_flags, 0x01);
        });
    }

    #[test]
    fn did_set_spelloptions_opt_global_skips_the_local_slot() {
        with_spo_flags(|| {
            // OPT_GLOBAL means `win.w_s` is never dereferenced, so a
            // null w_s is safe here - matching the original, which
            // likewise only touches win->w_s on the non-OPT_GLOBAL
            // branch.
            let mut win = crate::buffer_defs::WinT::default();
            let mut args =
                spo_args(&mut win, crate::option_defs::opt_set_flags::OPT_GLOBAL, b"camel");
            assert_eq!(unsafe { did_set_spelloptions(&mut args) }, None);

            assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.spo_flags, 0x01);
        });
    }

    #[test]
    fn did_set_spelloptions_accepts_both_values_together() {
        with_spo_flags(|| {
            let mut synblock = crate::buffer_defs::SynblockT::default();
            let mut win = crate::buffer_defs::WinT {
                w_s: &mut synblock as *mut crate::buffer_defs::SynblockT,
                ..Default::default()
            };
            let mut args = spo_args(&mut win, 0, b"camel,noplainbuffer");
            assert_eq!(unsafe { did_set_spelloptions(&mut args) }, None);
            assert_eq!(synblock.b_p_spo_flags, 0x03);
        });
    }

    #[test]
    fn did_set_spelloptions_empty_is_valid() {
        with_spo_flags(|| {
            let mut synblock = crate::buffer_defs::SynblockT::default();
            let mut win = crate::buffer_defs::WinT {
                w_s: &mut synblock as *mut crate::buffer_defs::SynblockT,
                ..Default::default()
            };
            let mut args = spo_args(&mut win, 0, b"");
            assert_eq!(unsafe { did_set_spelloptions(&mut args) }, None);
            assert_eq!(synblock.b_p_spo_flags, 0);
        });
    }

    #[test]
    fn did_set_spelloptions_rejects_an_unknown_value() {
        with_spo_flags(|| {
            let mut synblock = crate::buffer_defs::SynblockT::default();
            let mut win = crate::buffer_defs::WinT {
                w_s: &mut synblock as *mut crate::buffer_defs::SynblockT,
                ..Default::default()
            };
            let mut args = spo_args(&mut win, 0, b"bogus");
            assert_eq!(
                unsafe { did_set_spelloptions(&mut args) },
                Some(crate::errors::e_invarg.as_bytes())
            );
        });
    }

    // ---- did_set_formatoptions / did_set_commentstring ----

    #[test]
    fn did_set_formatoptions_accepts_every_character_in_fo_all() {
        let mut val: Option<Vec<u8>> = Some(crate::option_vars::FO_ALL.as_bytes().to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_formatoptions(&mut args) }, None);
    }

    #[test]
    fn did_set_formatoptions_accepts_the_real_default_value() {
        let mut val: Option<Vec<u8>> = Some(b"tcqj".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_formatoptions(&mut args) }, None);
    }

    #[test]
    fn did_set_formatoptions_empty_is_valid() {
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_formatoptions(&mut args) }, None);
    }

    #[test]
    fn did_set_formatoptions_accepts_a_comma_since_fo_all_contains_one() {
        // Unlike WW_ALL, FO_ALL itself already contains a literal ','
        // so no separator has to be appended by the caller.
        assert!(crate::option_vars::FO_ALL.as_bytes().contains(&b','));
        let mut val: Option<Vec<u8>> = Some(b"t,c".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_formatoptions(&mut args) }, None);
    }

    #[test]
    fn did_set_formatoptions_rejects_an_unknown_flag_character() {
        // 'z' is genuinely absent from FO_ALL.
        let mut val: Option<Vec<u8>> = Some(b"tz".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(
            unsafe { did_set_formatoptions(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_commentstring_empty_is_valid() {
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_commentstring(&mut args) }, None);
    }

    #[test]
    fn did_set_commentstring_accepts_a_value_containing_the_placeholder() {
        let mut val: Option<Vec<u8>> = Some(b"/*%s*/".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_commentstring(&mut args) }, None);
    }

    #[test]
    fn did_set_commentstring_accepts_the_placeholder_at_the_very_end() {
        // Exercises the 2-byte window scan reaching the last position.
        let mut val: Option<Vec<u8>> = Some(b"# %s".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_commentstring(&mut args) }, None);
    }

    #[test]
    fn did_set_commentstring_rejects_a_non_empty_value_without_the_placeholder() {
        let mut val: Option<Vec<u8>> = Some(b"# ".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(
            unsafe { did_set_commentstring(&mut args) },
            Some(
                crate::gettext_defs::gettext_noop(
                    "E537: 'commentstring' must be empty or contain %s"
                )
                .as_bytes()
            )
        );
    }

    #[test]
    fn did_set_commentstring_rejects_a_lone_percent() {
        // A single '%' with no following 's' must not satisfy the
        // 2-byte "%s" scan.
        let mut val: Option<Vec<u8>> = Some(b"%".to_vec());
        let mut args = listflag_args(&mut val);
        assert!(unsafe { did_set_commentstring(&mut args) }.is_some());
    }

    // ---- did_set_comments ----

    fn comments_result(value: &[u8]) -> Option<&'static [u8]> {
        let mut val: Option<Vec<u8>> = Some(value.to_vec());
        let mut args = listflag_args(&mut val);
        unsafe { did_set_comments(&mut args) }
    }

    #[test]
    fn did_set_comments_accepts_a_simple_entry() {
        assert_eq!(comments_result(b"b:x"), None);
    }

    #[test]
    fn did_set_comments_accepts_the_real_multi_part_default_shape() {
        assert_eq!(comments_result(b"s1:/*,mb:*,ex:*/"), None);
    }

    #[test]
    fn did_set_comments_empty_is_valid() {
        assert_eq!(comments_result(b""), None);
    }

    #[test]
    fn did_set_comments_accepts_digits_and_minus_in_the_flag_section() {
        assert_eq!(comments_result(b"-:x"), None);
        assert_eq!(comments_result(b"3:x"), None);
    }

    #[test]
    fn did_set_comments_missing_colon_is_e524() {
        assert_eq!(comments_result(b"b"), Some(e_missing_colon.as_bytes()));
    }

    #[test]
    fn did_set_comments_empty_comment_string_is_e525() {
        assert_eq!(comments_result(b":"), Some(e_zero_length_string.as_bytes()));
        assert_eq!(comments_result(b"b:"), Some(e_zero_length_string.as_bytes()));
    }

    #[test]
    fn did_set_comments_lone_illegal_char_is_overwritten_by_e525() {
        // THE QUIRK: the illegal-character `break` only leaves the
        // inner loop, so the "zero length string" check still runs and
        // overwrites the error. Verified against a real nvim binary:
        // `comments=z` genuinely reports E525, not E539.
        assert_eq!(comments_result(b"z"), Some(e_zero_length_string.as_bytes()));
    }

    #[test]
    fn did_set_comments_illegal_char_survives_when_a_flag_char_follows() {
        // Same quirk, other side: with `b` following the illegal `z`,
        // neither overwrite condition holds, so the illegal-character
        // error survives. Verified against a real nvim binary
        // (`comments=zb:x` reports E539).
        assert_eq!(comments_result(b"zb:x"), Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn did_set_comments_handles_a_backslash_escaped_comma() {
        // The comment-string scan skips a backslash-escaped byte, so
        // an escaped comma does not split the entry.
        assert_eq!(comments_result(b"b:x\\,y"), None);
    }

    #[test]
    fn did_set_comments_trailing_backslash_does_not_run_past_the_end() {
        // `s[1] != NUL` guards the escape skip in the original; here
        // that is an explicit `s + 1 < len` bounds check.
        assert_eq!(comments_result(b"b:x\\"), None);
    }

    // ---- did_set_breakindentopt ----

    fn briopt_args(
        win: &mut crate::buffer_defs::WinT,
        window_local: bool,
        val: &mut Option<Vec<u8>>,
    ) -> crate::option_defs::OptsetT {
        // When `window_local`, os_varp must literally BE the window's
        // own wo_briopt storage, so did_set_breakindentopt's ptr::eq
        // check matches (mirroring the original's own
        // `varp == &win->w_p_briopt` comparison).
        let varp = if window_local {
            std::ptr::addr_of_mut!(win.w_onebuf_opt.wo_briopt) as *mut c_void
        } else {
            val as *mut Option<Vec<u8>> as *mut c_void
        };
        crate::option_defs::OptsetT {
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: varp,
            ..Default::default()
        }
    }

    #[test]
    fn did_set_breakindentopt_empty_is_valid() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = briopt_args(&mut win, false, &mut val);
        assert_eq!(unsafe { did_set_breakindentopt(&mut args) }, None);
    }

    #[test]
    fn did_set_breakindentopt_accepts_real_sub_options() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"min:20,shift:2,sbr".to_vec());
        let mut args = briopt_args(&mut win, false, &mut val);
        assert_eq!(unsafe { did_set_breakindentopt(&mut args) }, None);
    }

    #[test]
    fn did_set_breakindentopt_rejects_an_unknown_sub_option() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let mut args = briopt_args(&mut win, false, &mut val);
        assert_eq!(
            unsafe { did_set_breakindentopt(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_breakindentopt_window_local_value_is_stored_into_the_window() {
        // The ptr::eq branch: because os_varp IS the window's own
        // wo_briopt storage, briopt_check gets Some(wp) and actually
        // writes the parsed values into the window.
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_briopt = Some(b"min:7,shift:3".to_vec());
        let mut unused: Option<Vec<u8>> = None;
        let mut args = briopt_args(&mut win, true, &mut unused);
        assert_eq!(unsafe { did_set_breakindentopt(&mut args) }, None);
        assert_eq!(win.w_briopt_min, 7);
        assert_eq!(win.w_briopt_shift, 3);
    }

    #[test]
    fn did_set_breakindentopt_non_window_local_value_leaves_the_window_alone() {
        // The NULL branch: os_varp is a disconnected value, so
        // briopt_check gets None and only validates.
        let mut win = crate::buffer_defs::WinT::default();
        let before_min = win.w_briopt_min;
        let mut val: Option<Vec<u8>> = Some(b"min:7,shift:3".to_vec());
        let mut args = briopt_args(&mut win, false, &mut val);
        assert_eq!(unsafe { did_set_breakindentopt(&mut args) }, None);
        assert_eq!(win.w_briopt_min, before_min);
    }

    // ---- set_chars_option ----

    /// Runs `f` with both global chars options saved/restored, taking
    /// the shared test lock exactly once for the whole body.
    fn with_chars<R>(f: impl FnOnce() -> R) -> R {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let (plcs, pfcs) = (opts.p_lcs.clone(), opts.p_fcs.clone());
        let result = f();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_lcs = plcs;
        opts.p_fcs = pfcs;
        result
    }

    /// Check `value` against `what`, with a non-empty window-local
    /// value so the global fallback is not taken.
    fn chars_check(what: CharsOption, value: &[u8]) -> Option<&'static [u8]> {
        let mut win = crate::buffer_defs::WinT::default();
        match what {
            CharsOption::Listchars => win.w_onebuf_opt.wo_lcs = Some(value.to_vec()),
            CharsOption::Fillchars => win.w_onebuf_opt.wo_fcs = Some(value.to_vec()),
        }
        unsafe { set_chars_option(&mut win, value, what, false, None) }
    }

    #[test]
    fn set_chars_option_accepts_the_forms_real_nvim_accepts() {
        // Every case cross-checked against a real nvim binary.
        with_chars(|| {
            use CharsOption::{Fillchars, Listchars};
            for (what, v) in [
                (Listchars, &b"eol:$"[..]),
                (Listchars, b"tab:>-"),
                (Listchars, b"tab:>-."),
                (Listchars, b"tab:>-,leadtab:<-"),
                (Listchars, b"multispace:."),
                (Listchars, b"multispace:abc"),
                (Listchars, b"leadmultispace:xy,tab:>-"),
                (Listchars, b"eol:\\x41"),
                (Listchars, b"eol:\\u2500"),
                (Listchars, b"tab:\\x41\\x42"),
                (Listchars, b""),
                (Fillchars, b"vert:|"),
                (Fillchars, b"foldclose:+"),
                (Fillchars, b""),
            ] {
                assert_eq!(
                    chars_check(what, v),
                    None,
                    "{:?} should be accepted",
                    std::string::String::from_utf8_lossy(v)
                );
            }
        });
    }

    #[test]
    fn set_chars_option_rejects_the_forms_real_nvim_rejects() {
        with_chars(|| {
            use CharsOption::{Fillchars, Listchars};
            // Per-field errors go through `field_value_err`, which
            // returns a non-NULL EMPTY string when no errbuf is
            // supplied - still an error, just without a message.
            // `e_invarg`/`e_leadtab_requires_tab` are returned
            // directly instead, so they carry their text either way.
            // This split is exactly what `check_chars_options` relies
            // on, so it is pinned here rather than papered over.
            let field = &b""[..];
            let invarg = crate::errors::e_invarg.as_bytes();
            let leadtab = crate::errors::e_leadtab_requires_tab.as_bytes();
            for (what, v, want) in [
                // Wrong number of characters for the field.
                (Listchars, &b"tab:>"[..], field),
                (Listchars, b"eol:", field),
                (Listchars, b"eol:ab", field),
                (Listchars, b"multispace:", field),
                // Wrong character width: invalid/truncated hex all
                // return 0 from get_encoded_char_adv, which the
                // original reports as a width error rather than a
                // parse error - confirmed against real nvim.
                (Listchars, b"eol:\\xZZ", field),
                (Listchars, b"eol:\\x", field),
                (Listchars, b"eol:\\U0001F600", field),
                // Unknown field, or no colon at all.
                (Listchars, b"eol", invarg),
                (Listchars, b"bogus:x", invarg),
                (Fillchars, b"bogus:x", invarg),
                // `foldclosed` is not a field name; `foldclose` is.
                (Fillchars, b"foldclosed:+", invarg),
                // leadtab without tab.
                (Listchars, b"leadtab:>-", leadtab),
            ] {
                assert_eq!(
                    chars_check(what, v),
                    Some(want),
                    "{:?}",
                    std::string::String::from_utf8_lossy(v)
                );
            }
        });
    }

    #[test]
    fn set_chars_option_rejects_a_double_width_field_character() {
        with_chars(|| {
            // Real nvim reports E1512 here; with no errbuf the
            // message is empty, as above.
            assert_eq!(
                chars_check(CharsOption::Listchars, "nbsp:一".as_bytes()),
                Some(&b""[..])
            );
        });
    }

    #[test]
    fn set_chars_option_stores_values_and_defaults_when_applying() {
        with_chars(|| {
            let mut win = crate::buffer_defs::WinT::default();
            win.w_onebuf_opt.wo_lcs = Some(b"eol:$,tab:>-.".to_vec());
            let v = b"eol:$,tab:>-.".to_vec();
            assert_eq!(
                unsafe {
                    set_chars_option(&mut win, &v, CharsOption::Listchars, true, None)
                },
                None
            );
            let lcs = &win.w_p_lcs_chars;
            assert_eq!(lcs.eol, crate::grid::schar_from_char(i32::from(b'$')));
            assert_eq!(lcs.tab1, crate::grid::schar_from_char(i32::from(b'>')));
            assert_eq!(lcs.tab2, crate::grid::schar_from_char(i32::from(b'-')));
            assert_eq!(lcs.tab3, crate::grid::schar_from_char(i32::from(b'.')));

            // 'fillchars' picks up its table defaults for untouched
            // fields; `vert` defaults to the box-drawing char.
            let mut win = crate::buffer_defs::WinT::default();
            win.w_onebuf_opt.wo_fcs = Some(b"eob:~".to_vec());
            let v = b"eob:~".to_vec();
            assert_eq!(
                unsafe {
                    set_chars_option(&mut win, &v, CharsOption::Fillchars, true, None)
                },
                None
            );
            assert_eq!(
                win.w_p_fcs_chars.vert,
                crate::grid::schar_from_str(Some("│".as_bytes()))
            );
            assert_eq!(
                win.w_p_fcs_chars.eob,
                crate::grid::schar_from_char(i32::from(b'~'))
            );
        });
    }

    #[test]
    fn set_chars_option_sizes_multispace_from_the_last_occurrence() {
        with_chars(|| {
            // Two `multispace:` fields: the original records only the
            // LAST one's position, so only its characters are stored,
            // and the array is sized from it too.
            let mut win = crate::buffer_defs::WinT::default();
            win.w_onebuf_opt.wo_lcs = Some(b"multispace:ab,multispace:xyz".to_vec());
            let v = b"multispace:ab,multispace:xyz".to_vec();
            assert_eq!(
                unsafe {
                    set_chars_option(&mut win, &v, CharsOption::Listchars, true, None)
                },
                None
            );
            let ms = win.w_p_lcs_chars.multispace.as_ref().expect("multispace");
            assert_eq!(ms.len(), 3);
            assert_eq!(ms[0], crate::grid::schar_from_char(i32::from(b'x')));
            assert_eq!(ms[2], crate::grid::schar_from_char(i32::from(b'z')));
        });
    }

    #[test]
    fn set_chars_option_falls_back_to_the_global_value_when_local_is_empty() {
        with_chars(|| {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lcs =
                Some(b"bogus:x".to_vec());
            // The window-local value is empty, so the (invalid)
            // global value is what actually gets checked.
            let mut win = crate::buffer_defs::WinT::default();
            assert_eq!(
                unsafe { set_chars_option(&mut win, b"", CharsOption::Listchars, false, None) },
                Some(crate::errors::e_invarg.as_bytes())
            );
        });
    }

    #[test]
    fn set_chars_option_writes_the_message_into_errbuf_when_given_one() {
        with_chars(|| {
            // With no errbuf the original returns a non-NULL EMPTY
            // string, so the error is still reported but carries no
            // message - `check_chars_options` relies on exactly this.
            let mut win = crate::buffer_defs::WinT::default();
            win.w_onebuf_opt.wo_lcs = Some(b"eol:ab".to_vec());
            let v = b"eol:ab".to_vec();
            assert_eq!(
                unsafe {
                    set_chars_option(&mut win, &v, CharsOption::Listchars, false, None)
                },
                Some(&b""[..])
            );

            let mut errbuf = Vec::new();
            let mut win = crate::buffer_defs::WinT::default();
            win.w_onebuf_opt.wo_lcs = Some(b"eol:ab".to_vec());
            let got = unsafe {
                set_chars_option(
                    &mut win,
                    &v,
                    CharsOption::Listchars,
                    false,
                    Some(&mut errbuf),
                )
            };
            assert_eq!(got, Some(e_wrong_number_of_characters_for_field_str.as_bytes()));
            assert_eq!(errbuf, e_wrong_number_of_characters_for_field_str.as_bytes());
        });
    }

    // ---- check_chars_options / did_set_chars_option ----

    /// Runs `f` with `GLOBALS.curwin`/`firstwin`/tabpage chain
    /// pointed at `win`, restoring everything afterward, so the
    /// `FOR_ALL_TAB_WINDOWS` walk has exactly one window to visit.
    fn with_one_window<R>(win: &mut crate::buffer_defs::WinT, f: impl FnOnce() -> R) -> R {
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let saved = (g.curwin, g.firstwin, g.first_tabpage, g.curtab);
        let wp: *mut crate::buffer_defs::WinT = win;
        unsafe { &mut *wp }.w_next = std::ptr::null_mut();
        g.curwin = wp;
        g.firstwin = wp;
        // A null tabpage list means the walk visits nothing; give it
        // one tabpage whose "is curtab" test picks up `firstwin`.
        let mut tp = crate::buffer_defs::TabpageT {
            tp_firstwin: wp,
            tp_next: std::ptr::null_mut(),
            ..Default::default()
        };
        let tpp: *mut crate::buffer_defs::TabpageT = &mut tp;
        g.first_tabpage = tpp;
        g.curtab = tpp;

        let result = f();

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        (g.curwin, g.firstwin, g.first_tabpage, g.curtab) = saved;
        result
    }

    #[test]
    fn check_chars_options_accepts_valid_globals_and_rejects_invalid_ones() {
        with_chars(|| {
            let mut win = crate::buffer_defs::WinT::default();
            with_one_window(&mut win, || {
                let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
                opts.p_lcs = Some(b"eol:$".to_vec());
                opts.p_fcs = Some(b"vert:|".to_vec());
                assert_eq!(unsafe { check_chars_options() }, None);

                // A bad global 'listchars' is reported as E834, and a
                // bad 'fillchars' as E835 - the two are deliberately
                // distinguishable.
                let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
                opts.p_lcs = Some(b"bogus:x".to_vec());
                assert_eq!(
                    unsafe { check_chars_options() },
                    Some(e_conflicts_with_value_of_listchars.as_bytes())
                );

                let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
                opts.p_lcs = Some(b"eol:$".to_vec());
                opts.p_fcs = Some(b"bogus:x".to_vec());
                assert_eq!(
                    unsafe { check_chars_options() },
                    Some(e_conflicts_with_value_of_fillchars.as_bytes())
                );
            });
        });
    }

    #[test]
    fn did_set_chars_option_applies_a_window_local_value() {
        with_chars(|| {
            let mut win = crate::buffer_defs::WinT::default();
            win.w_onebuf_opt.wo_lcs = Some(b"tab:>-".to_vec());
            with_one_window(&mut win, || {
                let wp: *mut crate::buffer_defs::WinT =
                    unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
                let mut args = crate::option_defs::OptsetT {
                    os_win: wp.cast(),
                    os_varp: unsafe { std::ptr::addr_of_mut!((*wp).w_onebuf_opt.wo_lcs) }.cast(),
                    ..Default::default()
                };
                assert_eq!(unsafe { did_set_chars_option(&mut args) }, None);
                assert_eq!(
                    unsafe { &*wp }.w_p_lcs_chars.tab1,
                    crate::grid::schar_from_char(i32::from(b'>'))
                );
            });
        });
    }

    #[test]
    fn did_set_chars_option_reports_a_bad_window_local_value() {
        with_chars(|| {
            let mut win = crate::buffer_defs::WinT::default();
            win.w_onebuf_opt.wo_fcs = Some(b"bogus:x".to_vec());
            with_one_window(&mut win, || {
                let wp: *mut crate::buffer_defs::WinT =
                    unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
                let mut args = crate::option_defs::OptsetT {
                    os_win: wp.cast(),
                    os_varp: unsafe { std::ptr::addr_of_mut!((*wp).w_onebuf_opt.wo_fcs) }.cast(),
                    ..Default::default()
                };
                assert_eq!(
                    unsafe { did_set_chars_option(&mut args) },
                    Some(crate::errors::e_invarg.as_bytes())
                );
            });
        });
    }

    #[test]
    fn did_set_chars_option_clears_the_local_value_for_a_non_global_set() {
        with_chars(|| {
            let mut win = crate::buffer_defs::WinT::default();
            win.w_onebuf_opt.wo_lcs = Some(b"tab:>-".to_vec());
            with_one_window(&mut win, || {
                let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
                opts.p_lcs = Some(b"eol:$".to_vec());
                let wp: *mut crate::buffer_defs::WinT =
                    unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
                let mut args = crate::option_defs::OptsetT {
                    os_win: wp.cast(),
                    os_varp: std::ptr::addr_of_mut!(
                        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lcs
                    )
                    .cast(),
                    os_flags: 0, // not OPT_GLOBAL
                    ..Default::default()
                };
                assert_eq!(unsafe { did_set_chars_option(&mut args) }, None);
                // Setting the global value without OPT_GLOBAL clears
                // the window-local one.
                assert_eq!(
                    unsafe { &*wp }.w_onebuf_opt.wo_lcs.as_deref(),
                    Some(&b""[..])
                );
            });
        });
    }

    #[test]
    fn did_set_ambiwidth_and_emoji_validate_and_recheck_the_chars_options() {
        with_chars(|| {
            let mut win = crate::buffer_defs::WinT::default();
            with_one_window(&mut win, || {
                let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
                opts.p_lcs = Some(b"eol:$".to_vec());
                opts.p_fcs = Some(b"vert:|".to_vec());
                let prev_ambw = opts.p_ambw.clone();

                // A valid 'ambiwidth' passes, then re-checks the
                // chars options and finds them fine.
                opts.p_ambw = Some(b"single".to_vec());
                let mut args = crate::option_defs::OptsetT {
                    os_idx: crate::option_defs::OptIndex::Ambiwidth,
                    ..Default::default()
                };
                assert_eq!(unsafe { did_set_ambiwidth(&mut args) }, None);
                let mut args = crate::option_defs::OptsetT::default();
                assert_eq!(unsafe { did_set_emoji(&mut args) }, None);

                // An invalid 'ambiwidth' is rejected by both - E474
                // from real nvim, cross-checked.
                let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
                opts.p_ambw = Some(b"bogus".to_vec());
                let mut args = crate::option_defs::OptsetT {
                    os_idx: crate::option_defs::OptIndex::Ambiwidth,
                    ..Default::default()
                };
                assert_eq!(
                    unsafe { did_set_ambiwidth(&mut args) },
                    Some(crate::errors::e_invarg.as_bytes())
                );
                let mut args = crate::option_defs::OptsetT::default();
                assert_eq!(
                    unsafe { did_set_emoji(&mut args) },
                    Some(crate::errors::e_invarg.as_bytes())
                );

                unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ambw = prev_ambw;
            });
        });
    }

    // ---- did_set_winborder / did_set_pumborder ----

    /// Saves/restores both global border options around `f`, taking
    /// the shared test lock exactly ONCE for the whole body.
    ///
    /// Deliberately not a per-value helper: a per-value version would
    /// acquire and release the lock once per case, and this file's own
    /// verbose-file tests already showed that many short locked
    /// regions measurably perturb an unrelated pre-existing race.
    fn with_border_opts<R>(f: impl FnOnce(&mut dyn FnMut(bool, &[u8])) -> R) -> R {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_win = opts.p_winborder.clone();
        let prev_pum = opts.p_pumborder.clone();

        let mut set = |is_win: bool, value: &[u8]| {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            if is_win {
                opts.p_winborder = Some(value.to_vec());
            } else {
                opts.p_pumborder = Some(value.to_vec());
            }
        };
        let result = f(&mut set);

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_winborder = prev_win;
        opts.p_pumborder = prev_pum;
        result
    }

    #[test]
    fn did_set_winborder_accepts_every_valid_form() {
        // Cross-checked one-for-one against a real nvim binary via
        // `let &winborder = ...`: each of these is accepted there too.
        with_border_opts(|set| {
            for v in [
                &b"rounded"[..],
                b"double",
                b"single",
                b"shadow",
                b"solid",
                b"bold",
                b"none",
                b"",
                b"1,2,3,4,5,6,7,8",
                b"x,x,x,x,x,x,x,x",
            ] {
                set(true, v);
                let mut args = crate::option_defs::OptsetT::default();
                assert_eq!(
                    unsafe { did_set_winborder(&mut args) },
                    None,
                    "{:?} should be accepted",
                    std::string::String::from_utf8_lossy(v)
                );
            }
        });
    }

    #[test]
    fn did_set_winborder_rejects_every_invalid_form() {
        // Also cross-checked against a real nvim binary: each of these
        // reports E474 there. Note `,2,3,4,5,6,7,8` does split into
        // exactly eight parts, so it passes the length check and is
        // rejected instead by the "corner char between edge chars"
        // rule - an easy case to get wrong by assuming otherwise.
        with_border_opts(|set| {
            for v in [
                &b"nope"[..],
                b"1,2,3",
                b"1,2,3,4,5,6,7,8,9",
                b",2,3,4,5,6,7,8",
                "一,一,一,一,一,一,一,一".as_bytes(),
            ] {
                set(true, v);
                let mut args = crate::option_defs::OptsetT::default();
                assert_eq!(
                    unsafe { did_set_winborder(&mut args) },
                    Some(crate::errors::e_invarg.as_bytes()),
                    "{:?} should be rejected",
                    std::string::String::from_utf8_lossy(v)
                );
            }
        });
    }

    #[test]
    fn did_set_pumborder_shares_the_same_validation() {
        with_border_opts(|set| {
            set(false, b"rounded");
            let mut args = crate::option_defs::OptsetT::default();
            assert_eq!(unsafe { did_set_pumborder(&mut args) }, None);

            set(false, b"nope");
            let mut args = crate::option_defs::OptsetT::default();
            assert_eq!(
                unsafe { did_set_pumborder(&mut args) },
                Some(crate::errors::e_invarg.as_bytes())
            );
        });
    }

    // ---- did_set_verbosefile ----

    /// Saves/restores `p_vfile` and always stops the verbose file
    /// afterward, so these tests can't leak an open handle.
    fn with_vfile<R>(vfile: Option<&[u8]>, f: impl FnOnce() -> R) -> R {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile =
            vfile.map(<[u8]>::to_vec);
        unsafe { crate::message::verbose_stop() };

        let result = f();

        unsafe { crate::message::verbose_stop() };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile = prev;
        result
    }

    fn vfile_scratch(tag: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "nero_dsvf_{tag}_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn did_set_verbosefile_stops_for_an_empty_or_absent_value() {
        // Grouped deliberately - see the note on message.rs's own
        // verbose tests about lock-hold time and I/O perturbation.
        for v in [Some(&b""[..]), None] {
            with_vfile(v, || {
                let mut args = crate::option_defs::OptsetT::default();
                assert_eq!(unsafe { did_set_verbosefile(&mut args) }, None);
            });
        }
    }

    #[test]
    fn did_set_verbosefile_opens_a_real_file_and_reports_open_failures() {
        let path = vfile_scratch("open");
        let bytes = path.to_str().unwrap().as_bytes().to_vec();
        with_vfile(Some(&bytes), || {
            let mut args = crate::option_defs::OptsetT::default();
            assert_eq!(unsafe { did_set_verbosefile(&mut args) }, None);
            assert!(path.exists());
        });
        let _ = std::fs::remove_file(&path);

        // A file inside a non-existent directory can't be opened.
        let mut bad = std::env::temp_dir();
        bad.push("nero_dsvf_missing_dir");
        bad.push("nested");
        bad.push("log.txt");
        let bad_bytes = bad.to_str().unwrap().as_bytes().to_vec();
        with_vfile(Some(&bad_bytes), || {
            let mut args = crate::option_defs::OptsetT::default();
            assert_eq!(
                unsafe { did_set_verbosefile(&mut args) },
                Some(crate::errors::e_invarg.as_bytes())
            );
        });
    }

    // ---- did_set_filetype_or_syntax / did_set_highlight ----

    fn ft_args(val: &mut Option<Vec<u8>>, oldval: &[u8]) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_varp: val as *mut Option<Vec<u8>> as *mut c_void,
            os_oldval: crate::option_defs::OptVal::String(oldval.to_vec()),
            ..Default::default()
        }
    }

    #[test]
    fn did_set_filetype_or_syntax_accepts_a_plain_name() {
        let mut val: Option<Vec<u8>> = Some(b"rust".to_vec());
        let mut args = ft_args(&mut val, b"c");
        assert_eq!(unsafe { did_set_filetype_or_syntax(&mut args) }, None);
    }

    #[test]
    fn did_set_filetype_or_syntax_accepts_the_extra_name_characters() {
        // valid_filetype additionally allows '.', '-' and '_'.
        let mut val: Option<Vec<u8>> = Some(b"a.b-c_d".to_vec());
        let mut args = ft_args(&mut val, b"c");
        assert_eq!(unsafe { did_set_filetype_or_syntax(&mut args) }, None);
    }

    #[test]
    fn did_set_filetype_or_syntax_rejects_an_invalid_name() {
        let mut val: Option<Vec<u8>> = Some(b"bad/name".to_vec());
        let mut args = ft_args(&mut val, b"c");
        assert_eq!(
            unsafe { did_set_filetype_or_syntax(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_filetype_or_syntax_sets_value_changed_when_it_differs() {
        let mut val: Option<Vec<u8>> = Some(b"rust".to_vec());
        let mut args = ft_args(&mut val, b"c");
        assert_eq!(unsafe { did_set_filetype_or_syntax(&mut args) }, None);
        assert!(args.os_value_changed);
        assert!(args.os_value_checked);
    }

    #[test]
    fn did_set_filetype_or_syntax_clears_value_changed_when_it_matches() {
        let mut val: Option<Vec<u8>> = Some(b"rust".to_vec());
        let mut args = ft_args(&mut val, b"rust");
        assert_eq!(unsafe { did_set_filetype_or_syntax(&mut args) }, None);
        assert!(!args.os_value_changed);
        // os_value_checked is set unconditionally on success.
        assert!(args.os_value_checked);
    }

    #[test]
    fn did_set_filetype_or_syntax_leaves_the_flags_alone_on_failure() {
        // Both flags are only touched AFTER validation succeeds.
        let mut val: Option<Vec<u8>> = Some(b"bad/name".to_vec());
        let mut args = ft_args(&mut val, b"c");
        assert!(unsafe { did_set_filetype_or_syntax(&mut args) }.is_some());
        assert!(!args.os_value_changed);
        assert!(!args.os_value_checked);
    }

    #[test]
    fn did_set_highlight_accepts_only_the_builtin_default() {
        let mut val: Option<Vec<u8>> =
            Some(crate::option_vars::HIGHLIGHT_INIT.as_bytes().to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_highlight(&mut args) }, None);
    }

    #[test]
    fn did_set_highlight_rejects_anything_else() {
        for v in [&b""[..], b"8:SpecialKey", b"bogus"] {
            let mut val: Option<Vec<u8>> = Some(v.to_vec());
            let mut args = listflag_args(&mut val);
            assert_eq!(
                unsafe { did_set_highlight(&mut args) },
                Some(crate::errors::e_unsupportedoption.as_bytes()),
                "value {v:?}"
            );
        }
    }

    #[test]
    fn did_set_highlight_rejects_a_truncated_default() {
        // Even a strict prefix of the default is rejected - the
        // comparison is a whole-value equality, not a prefix match.
        let full = crate::option_vars::HIGHLIGHT_INIT.as_bytes();
        let mut val: Option<Vec<u8>> = Some(full[..full.len() - 1].to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(
            unsafe { did_set_highlight(&mut args) },
            Some(crate::errors::e_unsupportedoption.as_bytes())
        );
    }

    // ---- did_set_complete ----

    /// Runs `did_set_complete` with a real (non-None) `os_errbuf`, so
    /// the `char_before` path reports its error rather than taking
    /// the original's own errbuf-absent success shortcut.
    fn complete_result(value: &[u8]) -> Option<&'static [u8]> {
        let _lock = crate::globals::global_state_test_lock();
        let mut val: Option<Vec<u8>> = Some(value.to_vec());
        let mut buf = Box::new(crate::buffer_defs::BufT {
            b_p_cpt: Some(value.to_vec()),
            ..Default::default()
        });
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let _curbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.curbuf,
                buf_ptr,
            )
        };
        let mut args = crate::option_defs::OptsetT {
            os_varp: &mut val as *mut Option<Vec<u8>> as *mut c_void,
            os_flags: crate::option_defs::opt_set_flags::OPT_LOCAL as i32,
            os_errbuf: Some(vec![0; 80]),
            os_errbuflen: 80,
            ..Default::default()
        };
        let result = unsafe { did_set_complete(&mut args) };
        for callback in unsafe { &mut (*buf_ptr).b_p_cpt_cb } {
            crate::eval::typval::callback_free(callback);
        }
        result
    }

    fn illegal_after() -> Option<&'static [u8]> {
        Some(e_illegal_character_after_chr.as_bytes())
    }

    #[test]
    fn did_set_complete_accepts_the_real_default_value() {
        assert_eq!(complete_result(b".,w,b,u,t"), None);
    }

    #[test]
    fn did_set_complete_empty_is_valid() {
        assert_eq!(complete_result(b""), None);
    }

    #[test]
    fn did_set_complete_accepts_bare_argument_taking_flags() {
        for v in [&b"k"[..], b"s", b"F"] {
            assert_eq!(complete_result(v), None, "value {v:?}");
        }
    }

    #[test]
    fn did_set_complete_argument_taking_flags_accept_trailing_text() {
        assert_eq!(complete_result(b"k/some/path"), None);
        assert_eq!(complete_result(b"Fmyfunc"), None);
    }

    #[test]
    fn did_set_complete_builds_function_callback_slots() {
        let _lock = crate::globals::global_state_test_lock();
        let mut val: Option<Vec<u8>> = Some(b".,FMyComplete,w".to_vec());
        let mut buf = Box::new(crate::buffer_defs::BufT {
            b_p_cpt: val.clone(),
            ..Default::default()
        });
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let _curbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.curbuf,
                buf_ptr,
            )
        };
        let mut args = crate::option_defs::OptsetT {
            os_varp: (&mut val as *mut Option<Vec<u8>>).cast(),
            os_flags: crate::option_defs::opt_set_flags::OPT_LOCAL as i32,
            os_errbuf: Some(vec![0; 80]),
            os_errbuflen: 80,
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_complete(&mut args) }, None);
        let callbacks = unsafe { &mut (*buf_ptr).b_p_cpt_cb };
        assert_eq!(callbacks.len(), 3);
        assert!(matches!(
            &callbacks[1],
            crate::eval::typval_defs::Callback::Funcref(name)
                if name == b"MyComplete"
        ));
        for callback in callbacks {
            crate::eval::typval::callback_free(callback);
        }
    }

    #[test]
    fn did_set_complete_accepts_a_caret_count() {
        assert_eq!(complete_result(b"u^5"), None);
        assert_eq!(complete_result(b"w^3"), None);
    }

    #[test]
    fn did_set_complete_rejects_a_caret_with_no_digits() {
        assert_eq!(complete_result(b"u^"), illegal_after());
        assert_eq!(complete_result(b".^"), illegal_after());
    }

    #[test]
    fn did_set_complete_rejects_a_caret_followed_by_a_non_digit() {
        assert_eq!(complete_result(b"u^x"), illegal_after());
    }

    #[test]
    fn did_set_complete_rejects_trailing_text_after_a_plain_flag() {
        assert_eq!(complete_result(b"ux"), illegal_after());
    }

    #[test]
    fn did_set_complete_rejects_an_unknown_flag() {
        assert_eq!(complete_result(b"z"), Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn did_set_complete_leading_comma_is_an_illegal_empty_entry() {
        // The empty entry's first byte reads as the original's own NUL
        // terminator, which vim_strchr's `c <= 0` guard never matches.
        assert_eq!(complete_result(b",w"), Some(crate::errors::e_invarg.as_bytes()));
    }

    #[test]
    fn did_set_complete_doubled_comma_is_fine() {
        // Unlike a LEADING comma: the end-of-entry skip loop consumes
        // runs of commas.
        assert_eq!(complete_result(b".,,w"), None);
    }

    #[test]
    fn did_set_complete_a_space_is_not_skipped_during_extraction() {
        // ". , w" extracts ". " (with the trailing space), which is
        // then rejected as trailing text after a plain flag.
        assert_eq!(complete_result(b". , w"), illegal_after());
    }

    #[test]
    fn did_set_complete_escaped_comma_consumes_both_bytes() {
        // THE QUIRK: `\,` contributes nothing at all, so this parses
        // as the bare entry "u" and is VALID. Verified against a real
        // nvim binary.
        assert_eq!(complete_result(b"u\\,"), None);
        assert_eq!(complete_result(b"k\\,x"), None);
    }

    #[test]
    fn did_set_complete_escape_flag_makes_only_a_following_comma_literal() {
        // Same quirk, other side: after `\,` is consumed, the escape
        // flag makes the NEXT comma literal, so this parses as "u,x"
        // and is rejected. Verified against a real nvim binary.
        assert_eq!(complete_result(b"u\\,,x"), illegal_after());
    }

    #[test]
    fn did_set_complete_without_an_errbuf_the_char_before_path_succeeds() {
        // The original's own asymmetry: this failure path returns
        // success when os_errbuf is absent, while the unknown-flag
        // path reports an error either way.
        let mut val: Option<Vec<u8>> = Some(b"ux".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_varp: &mut val as *mut Option<Vec<u8>> as *mut c_void,
            os_errbuf: None,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_complete(&mut args) }, None);

        // ...whereas the unknown-flag path still errors with no errbuf.
        let mut bad: Option<Vec<u8>> = Some(b"z".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_varp: &mut bad as *mut Option<Vec<u8>> as *mut c_void,
            os_errbuf: None,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_complete(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_complete_truncates_an_entry_at_the_lsize_bound() {
        // An entry is truncated at LSIZE-1 bytes, matching the
        // original's own `buf_ptr < buffer + LSIZE - 1` guard. The
        // parse position is left mid-entry, so the untruncated TAIL is
        // then re-parsed as a fresh entry - which starts with 'x' and
        // is therefore illegal. All three boundary cases below were
        // verified against a real nvim binary.
        let long_k = |n: usize| {
            let mut v = vec![b'k'];
            v.extend(std::iter::repeat_n(b'x', n));
            v
        };

        // "k" + 510 x's == exactly LSIZE-1 bytes: fits, nothing left.
        assert_eq!(complete_result(&long_k(crate::tag::LSIZE - 2)), None);
        // One byte over: the tail re-parses as an illegal entry.
        assert_eq!(
            complete_result(&long_k(crate::tag::LSIZE - 1)),
            Some(crate::errors::e_invarg.as_bytes())
        );
        assert_eq!(
            complete_result(&long_k(crate::tag::LSIZE * 2)),
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    // ---- check_signcolumn / did_set_signcolumn ----

    fn scl_win() -> crate::buffer_defs::WinT {
        crate::buffer_defs::WinT::default()
    }

    #[test]
    fn check_signcolumn_accepts_every_listed_value_without_a_window() {
        for v in crate::option_vars::OPT_SCL_VALUES {
            assert!(check_signcolumn(Some(v.as_bytes()), None), "value {v}");
        }
    }

    #[test]
    fn check_signcolumn_rejects_an_empty_value() {
        // Unlike most options in this file, empty is invalid here.
        assert!(!check_signcolumn(Some(b""), None));
    }

    #[test]
    fn check_signcolumn_rejects_an_unknown_value() {
        assert!(!check_signcolumn(Some(b"bogus"), None));
    }

    #[test]
    fn check_signcolumn_accepts_a_valid_auto_range() {
        assert!(check_signcolumn(Some(b"auto:1-3"), None));
        assert!(check_signcolumn(Some(b"auto:2-9"), None));
    }

    #[test]
    fn check_signcolumn_rejects_out_of_bounds_auto_ranges() {
        // Every one of these was confirmed rejected by a real nvim.
        for v in [
            &b"auto:0-3"[..], // min < 1
            b"auto:3-2",      // min > max
            b"auto:3-3",      // min == max
            b"auto:9-9",      // min > 8 and min == max
            b"auto:1-1",      // max < 2
            b"auto:1-12",     // wrong length
        ] {
            assert!(!check_signcolumn(Some(v), None), "value {v:?}");
        }
    }

    #[test]
    fn check_signcolumn_no_sets_the_scl_no_sentinel() {
        let mut win = scl_win();
        assert!(check_signcolumn(Some(b"no"), Some(&mut win)));
        assert_eq!(win.w_minscwidth, crate::option_vars::SCL_NO);
        assert_eq!(win.w_maxscwidth, crate::option_vars::SCL_NO);
    }

    #[test]
    fn check_signcolumn_number_needs_number_or_relativenumber() {
        // With 'number' set, "number" maps to the SCL_NUM sentinel.
        let mut win = scl_win();
        win.w_onebuf_opt.wo_nu = 1;
        assert!(check_signcolumn(Some(b"number"), Some(&mut win)));
        assert_eq!(win.w_minscwidth, crate::option_vars::SCL_NUM);

        // 'relativenumber' alone works too.
        let mut win = scl_win();
        win.w_onebuf_opt.wo_rnu = 1;
        assert!(check_signcolumn(Some(b"number"), Some(&mut win)));
        assert_eq!(win.w_minscwidth, crate::option_vars::SCL_NUM);
    }

    #[test]
    fn check_signcolumn_number_without_nu_falls_through_to_auto() {
        // The real behaviour: with neither 'number' nor
        // 'relativenumber', "number" falls all the way through to the
        // same min=0/max=1 the bare "auto" uses.
        let mut win = scl_win();
        assert!(check_signcolumn(Some(b"number"), Some(&mut win)));
        assert_eq!(win.w_minscwidth, 0);
        assert_eq!(win.w_maxscwidth, 1);
    }

    #[test]
    fn check_signcolumn_yes_and_yes_n_set_both_bounds() {
        let mut win = scl_win();
        assert!(check_signcolumn(Some(b"yes"), Some(&mut win)));
        assert_eq!((win.w_minscwidth, win.w_maxscwidth), (1, 1));

        let mut win = scl_win();
        assert!(check_signcolumn(Some(b"yes:3"), Some(&mut win)));
        assert_eq!((win.w_minscwidth, win.w_maxscwidth), (3, 3));
    }

    #[test]
    fn check_signcolumn_auto_forms_set_min_zero() {
        let mut win = scl_win();
        assert!(check_signcolumn(Some(b"auto"), Some(&mut win)));
        assert_eq!((win.w_minscwidth, win.w_maxscwidth), (0, 1));

        let mut win = scl_win();
        assert!(check_signcolumn(Some(b"auto:5"), Some(&mut win)));
        assert_eq!((win.w_minscwidth, win.w_maxscwidth), (0, 5));

        let mut win = scl_win();
        assert!(check_signcolumn(Some(b"auto:2-6"), Some(&mut win)));
        assert_eq!((win.w_minscwidth, win.w_maxscwidth), (2, 6));
    }

    #[test]
    fn check_signcolumn_falls_back_to_the_windows_own_value() {
        // scl == None reads wp's own wo_scl instead.
        let mut win = scl_win();
        win.w_onebuf_opt.wo_scl = Some(b"yes:4".to_vec());
        assert!(check_signcolumn(None, Some(&mut win)));
        assert_eq!((win.w_minscwidth, win.w_maxscwidth), (4, 4));
    }

    #[test]
    fn check_signcolumn_recomputes_scwidth_from_the_new_bounds() {
        // min > 0 clamps w_scwidth into [min, max].
        let mut win = scl_win();
        win.w_scwidth = 9;
        assert!(check_signcolumn(Some(b"yes:3"), Some(&mut win)));
        assert_eq!(win.w_scwidth, 3);

        // min <= 0 drives the intermediate to 0, so w_scwidth becomes
        // max(min, 0) == 0.
        let mut win = scl_win();
        win.w_scwidth = 9;
        assert!(check_signcolumn(Some(b"auto"), Some(&mut win)));
        assert_eq!(win.w_scwidth, 0);
    }

    #[test]
    fn check_signcolumn_leaves_the_window_untouched_on_failure() {
        let mut win = scl_win();
        win.w_minscwidth = 7;
        assert!(!check_signcolumn(Some(b"auto:3-2"), Some(&mut win)));
        assert_eq!(win.w_minscwidth, 7);
    }

    fn scl_args(
        win: &mut crate::buffer_defs::WinT,
        window_local: bool,
        val: &mut Option<Vec<u8>>,
        oldval: &[u8],
    ) -> crate::option_defs::OptsetT {
        let varp = if window_local {
            std::ptr::addr_of_mut!(win.w_onebuf_opt.wo_scl) as *mut c_void
        } else {
            val as *mut Option<Vec<u8>> as *mut c_void
        };
        crate::option_defs::OptsetT {
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: varp,
            os_oldval: crate::option_defs::OptVal::String(oldval.to_vec()),
            ..Default::default()
        }
    }

    #[test]
    fn did_set_signcolumn_accepts_a_valid_value() {
        let mut win = scl_win();
        let mut val: Option<Vec<u8>> = Some(b"yes".to_vec());
        let mut args = scl_args(&mut win, false, &mut val, b"auto");
        assert_eq!(unsafe { did_set_signcolumn(&mut args) }, None);
    }

    #[test]
    fn did_set_signcolumn_rejects_an_invalid_value() {
        let mut win = scl_win();
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let mut args = scl_args(&mut win, false, &mut val, b"auto");
        assert_eq!(
            unsafe { did_set_signcolumn(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_signcolumn_window_local_value_updates_the_window() {
        let mut win = scl_win();
        win.w_onebuf_opt.wo_scl = Some(b"yes:2".to_vec());
        let mut unused: Option<Vec<u8>> = None;
        let mut args = scl_args(&mut win, true, &mut unused, b"auto");
        assert_eq!(unsafe { did_set_signcolumn(&mut args) }, None);
        assert_eq!((win.w_minscwidth, win.w_maxscwidth), (2, 2));
    }

    #[test]
    fn did_set_signcolumn_resets_nrwidth_when_switching_away_from_number() {
        // The "from" half of the check reads os_oldval's first two
        // bytes.
        let mut win = crate::buffer_defs::WinT { w_nrwidth_line_count: 42, ..Default::default() };
        let mut val: Option<Vec<u8>> = Some(b"yes".to_vec());
        let mut args = scl_args(&mut win, false, &mut val, b"number");
        assert_eq!(unsafe { did_set_signcolumn(&mut args) }, None);
        assert_eq!(win.w_nrwidth_line_count, 0);
    }

    #[test]
    fn did_set_signcolumn_resets_nrwidth_when_switching_to_number() {
        // The "to" half: w_minscwidth ends up as SCL_NUM.
        let mut win = crate::buffer_defs::WinT { w_nrwidth_line_count: 42, ..Default::default() };
        win.w_onebuf_opt.wo_nu = 1;
        win.w_onebuf_opt.wo_scl = Some(b"number".to_vec());
        let mut unused: Option<Vec<u8>> = None;
        let mut args = scl_args(&mut win, true, &mut unused, b"auto");
        assert_eq!(unsafe { did_set_signcolumn(&mut args) }, None);
        assert_eq!(win.w_minscwidth, crate::option_vars::SCL_NUM);
        assert_eq!(win.w_nrwidth_line_count, 0);
    }

    #[test]
    fn did_set_signcolumn_leaves_nrwidth_alone_for_an_unrelated_change() {
        let mut win = crate::buffer_defs::WinT { w_nrwidth_line_count: 42, ..Default::default() };
        let mut val: Option<Vec<u8>> = Some(b"yes".to_vec());
        let mut args = scl_args(&mut win, false, &mut val, b"auto");
        assert_eq!(unsafe { did_set_signcolumn(&mut args) }, None);
        assert_eq!(win.w_nrwidth_line_count, 42);
    }

    // ---- did_set_colorcolumn ----

    fn cc_args(
        win: &mut crate::buffer_defs::WinT,
        window_local: bool,
        val: &mut Option<Vec<u8>>,
    ) -> crate::option_defs::OptsetT {
        let varp = if window_local {
            std::ptr::addr_of_mut!(win.w_onebuf_opt.wo_cc) as *mut c_void
        } else {
            val as *mut Option<Vec<u8>> as *mut c_void
        };
        crate::option_defs::OptsetT {
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: varp,
            ..Default::default()
        }
    }

    #[test]
    fn did_set_colorcolumn_empty_is_valid() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = cc_args(&mut win, false, &mut val);
        assert_eq!(unsafe { did_set_colorcolumn(&mut args) }, None);
    }

    #[test]
    fn did_set_colorcolumn_accepts_absolute_and_relative_columns() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"+1,-1,80".to_vec());
        let mut args = cc_args(&mut win, false, &mut val);
        assert_eq!(unsafe { did_set_colorcolumn(&mut args) }, None);
    }

    #[test]
    fn did_set_colorcolumn_rejects_a_non_numeric_value() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let mut args = cc_args(&mut win, false, &mut val);
        assert_eq!(
            unsafe { did_set_colorcolumn(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_colorcolumn_window_local_value_is_stored_into_the_window() {
        // The ptr::eq branch: os_varp IS the window's own wo_cc
        // storage, so check_colorcolumn gets Some(wp) and fills
        // w_p_cc_cols. A real (non-null) w_buffer is required - a null
        // one makes check_colorcolumn return early with "buffer was
        // closed", storing nothing.
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT {
            w_buffer: &mut buf as *mut crate::buffer_defs::BufT,
            ..Default::default()
        };
        win.w_onebuf_opt.wo_cc = Some(b"80".to_vec());
        let mut unused: Option<Vec<u8>> = None;
        let mut args = cc_args(&mut win, true, &mut unused);
        assert_eq!(unsafe { did_set_colorcolumn(&mut args) }, None);
        // Stored 0-based, so column 80 becomes 79.
        assert_eq!(win.w_p_cc_cols, Some(vec![79]));
    }

    #[test]
    fn did_set_colorcolumn_window_local_null_buffer_is_treated_as_closed() {
        // A null w_buffer short-circuits check_colorcolumn to success
        // without storing anything - matching the original's own
        // "buffer was closed" early return.
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_cc = Some(b"80".to_vec());
        let mut unused: Option<Vec<u8>> = None;
        let mut args = cc_args(&mut win, true, &mut unused);
        assert_eq!(unsafe { did_set_colorcolumn(&mut args) }, None);
        assert!(win.w_p_cc_cols.is_none());
    }

    #[test]
    fn did_set_colorcolumn_non_window_local_value_leaves_the_window_alone() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"80".to_vec());
        let mut args = cc_args(&mut win, false, &mut val);
        assert_eq!(unsafe { did_set_colorcolumn(&mut args) }, None);
        assert!(win.w_p_cc_cols.is_none());
    }

    // ---- did_set_optexpr ----

    #[test]
    fn did_set_optexpr_leaves_a_plain_name_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let mut val: Option<Vec<u8>> = Some(b"MyFunc()".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_optexpr(&mut args) }, None);
        assert_eq!(val, Some(b"MyFunc()".to_vec()));
    }

    #[test]
    fn did_set_optexpr_leaves_an_empty_value_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_optexpr(&mut args) }, None);
        assert_eq!(val, Some(Vec::new()));
    }

    #[test]
    fn did_set_optexpr_expands_an_s_colon_prefix_in_place() {
        // The only callback in this file that REPLACES the value.
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        let (sid, _) = crate::runtime::new_script_item(None);
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid = sid;

        let mut val: Option<Vec<u8>> = Some(b"s:MyFunc".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_optexpr(&mut args) }, None);
        assert_eq!(val, Some(format!("<SNR>{sid}_MyFunc").into_bytes()));
    }

    #[test]
    fn did_set_optexpr_expands_a_sid_tag_prefix_in_place() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        let (sid, _) = crate::runtime::new_script_item(None);
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid = sid;

        let mut val: Option<Vec<u8>> = Some(b"<SID>MyFunc".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_optexpr(&mut args) }, None);
        assert_eq!(val, Some(format!("<SNR>{sid}_MyFunc").into_bytes()));
    }

    #[test]
    fn did_set_optexpr_leaves_the_value_alone_when_the_sid_is_invalid() {
        // get_scriptlocal_funcname returns None when the script
        // context has no valid SID, so the value is left as-is.
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid = 0;

        let mut val: Option<Vec<u8>> = Some(b"s:MyFunc".to_vec());
        let mut args = listflag_args(&mut val);
        assert_eq!(unsafe { did_set_optexpr(&mut args) }, None);
        assert_eq!(val, Some(b"s:MyFunc".to_vec()));
    }

    // ---- did_set_foldexpr ----

    fn foldexpr_args(
        win: &mut crate::buffer_defs::WinT,
        val: &mut Option<Vec<u8>>,
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        }
    }

    #[test]
    fn did_set_foldexpr_is_a_no_op_for_a_non_expr_foldmethod() {
        // Default 'foldmethod' is "manual", so the fold-update branch
        // is never reached.
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"MyFunc()".to_vec());
        let mut args = foldexpr_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldexpr(&mut args) }, None);
        assert_eq!(val, Some(b"MyFunc()".to_vec()));
    }

    #[test]
    fn did_set_foldexpr_still_expands_a_script_local_prefix() {
        // Delegation to did_set_optexpr must actually happen.
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        let (sid, _) = crate::runtime::new_script_item(None);
        unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid = sid;

        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"s:MyFunc".to_vec());
        let mut args = foldexpr_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldexpr(&mut args) }, None);
        assert_eq!(val, Some(format!("<SNR>{sid}_MyFunc").into_bytes()));
    }

    #[test]
    fn did_set_foldexpr_invalidates_the_folds_when_foldmethod_is_expr() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_fdm = Some(b"expr".to_vec());
        let mut val: Option<Vec<u8>> = Some(b"MyFunc()".to_vec());
        let mut args = foldexpr_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_foldexpr(&mut args) }, None);
        assert!(win.w_foldinvalid, "foldUpdateAll ran");
    }

    // ---- did_set_winbar / did_set_tabline / did_set_statuscolumn ----

    fn stl_family_args(
        win: &mut crate::buffer_defs::WinT,
        val: &mut Option<Vec<u8>>,
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            // Any index EXCEPT Statusline keeps the untranslated
            // is_stl branches unreachable.
            os_idx: crate::option_defs::OptIndex::Winbar,
            os_win: win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        }
    }

    #[test]
    fn did_set_winbar_accepts_valid_statusline_syntax() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"%f %m".to_vec());
        let mut args = stl_family_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_winbar(&mut args) }, None);
    }

    #[test]
    fn did_set_winbar_empty_is_valid() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = stl_family_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_winbar(&mut args) }, None);
    }

    #[test]
    fn did_set_winbar_rejects_invalid_statusline_syntax() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"%z".to_vec());
        let mut args = stl_family_args(&mut win, &mut val);
        assert_eq!(
            unsafe { did_set_winbar(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_winbar_skips_validation_for_a_percent_bang_function_ref() {
        // "%!" means a custom function reference, which is NOT
        // statusline syntax - the original skips check_stl_option
        // entirely for it, so an otherwise-invalid tail is accepted.
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"%!MyFunc()".to_vec());
        let mut args = stl_family_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_winbar(&mut args) }, None);
    }

    #[test]
    fn did_set_tabline_validates_the_same_way() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut ok: Option<Vec<u8>> = Some(b"%f".to_vec());
        let mut args = stl_family_args(&mut win, &mut ok);
        assert_eq!(unsafe { did_set_tabline(&mut args) }, None);

        let mut bad: Option<Vec<u8>> = Some(b"%z".to_vec());
        let mut args = stl_family_args(&mut win, &mut bad);
        assert_eq!(
            unsafe { did_set_tabline(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_statuscolumn_resets_the_windows_nrwidth_line_count() {
        // The statuscolumn variant's own extra side effect.
        let mut win = crate::buffer_defs::WinT { w_nrwidth_line_count: 42, ..Default::default() };
        let mut val: Option<Vec<u8>> = Some(b"%l".to_vec());
        let mut args = stl_family_args(&mut win, &mut val);
        assert_eq!(unsafe { did_set_statuscolumn(&mut args) }, None);
        assert_eq!(win.w_nrwidth_line_count, 0);
    }

    #[test]
    fn did_set_statuscolumn_resets_the_count_even_when_the_value_is_invalid() {
        // The reset happens BEFORE validation, matching the original's
        // own ordering.
        let mut win = crate::buffer_defs::WinT { w_nrwidth_line_count: 42, ..Default::default() };
        let mut val: Option<Vec<u8>> = Some(b"%z".to_vec());
        let mut args = stl_family_args(&mut win, &mut val);
        assert_eq!(
            unsafe { did_set_statuscolumn(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
        assert_eq!(win.w_nrwidth_line_count, 0);
    }

    #[test]
    fn did_set_statusline_validates_a_nonempty_local_value() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"%f".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Statusline,
            os_flags: crate::option_defs::opt_set_flags::OPT_LOCAL as i32,
            os_win: &mut win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: &mut val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(unsafe { did_set_statusline(&mut args) }, None);
    }

    #[test]
    fn did_set_statusline_rejects_invalid_statusline_syntax() {
        let mut win = crate::buffer_defs::WinT::default();
        let mut val: Option<Vec<u8>> = Some(b"%z".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Statusline,
            os_flags: crate::option_defs::opt_set_flags::OPT_LOCAL as i32,
            os_win: &mut win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: &mut val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        assert_eq!(
            unsafe { did_set_statusline(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    #[should_panic(expected = "get_option_default")]
    fn did_set_statusline_empty_global_value_needs_the_default() {
        let mut val: Option<Vec<u8>> = Some(Vec::new());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Statusline,
            os_flags: crate::option_defs::opt_set_flags::OPT_GLOBAL as i32,
            os_varp: &mut val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        let _ = unsafe { did_set_statusline(&mut args) };
    }

    #[test]
    #[should_panic(expected = "win_config_float")]
    fn did_set_statusline_floating_window_needs_reconfiguration() {
        let mut win = crate::buffer_defs::WinT {
            w_floating: true,
            ..Default::default()
        };
        let mut val: Option<Vec<u8>> = Some(b"%f".to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Statusline,
            os_flags: crate::option_defs::opt_set_flags::OPT_LOCAL as i32,
            os_win: &mut win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: &mut val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        let _ = unsafe { did_set_statusline(&mut args) };
    }

    // ---- did_set_rulerformat ----

    /// `did_set_rulerformat` always reaches `comp_col`, which walks
    /// `GLOBALS.firstwin`'s own `w_next` chain - so these tests hold
    /// the global state lock and point `firstwin`/`lastwin` at a real
    /// single window, restoring the previous pointers afterward.
    fn with_ruf<R>(value: &[u8], f: impl FnOnce(&mut crate::option_defs::OptsetT) -> R) -> R {
        let _lock = crate::globals::global_state_test_lock();

        let mut win = crate::buffer_defs::WinT::default();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_first = globals.firstwin;
        let prev_last = globals.lastwin;
        let prev_ruwid = globals.ru_wid;
        globals.firstwin = &mut win as *mut crate::buffer_defs::WinT;
        globals.lastwin = &mut win as *mut crate::buffer_defs::WinT;

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_ruf = opts.p_ruf.clone();
        opts.p_ruf = Some(value.to_vec());

        let mut val: Option<Vec<u8>> = Some(value.to_vec());
        let mut args = crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Rulerformat,
            os_win: &mut win as *mut crate::buffer_defs::WinT as *mut c_void,
            os_varp: &mut val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        };
        let result = f(&mut args);

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_ruf = prev_ruf;
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.firstwin = prev_first;
        globals.lastwin = prev_last;
        globals.ru_wid = prev_ruwid;
        result
    }

    #[test]
    fn did_set_rulerformat_accepts_a_width_group_and_stores_ru_wid() {
        with_ruf(b"%99(hello%)", |args| {
            assert_eq!(unsafe { did_set_rulerformat(args) }, None);
            assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.ru_wid, 99);
        });
    }

    #[test]
    fn did_set_rulerformat_ignores_a_minus_before_the_width() {
        // The original skips one '-' before reading the digits, so
        // "%-99(" still yields the width group.
        with_ruf(b"%-99(hello%)", |args| {
            assert_eq!(unsafe { did_set_rulerformat(args) }, None);
            assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.ru_wid, 99);
        });
    }

    #[test]
    fn did_set_rulerformat_width_needs_an_immediately_following_paren() {
        // The real default-shaped value: after the '-' and the digits
        // "14" comes a '.', NOT a '(', so the `wid && *s == '('`
        // condition fails and ru_wid is left at the 0 it was reset to
        // on entry. The value is still perfectly valid - it just
        // doesn't carry a ruler width.
        with_ruf(b"%-14.14(%l,%c%V%) %P", |args| {
            assert_eq!(unsafe { did_set_rulerformat(args) }, None);
            assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.ru_wid, 0);
        });
    }

    #[test]
    fn did_set_rulerformat_empty_is_valid_and_leaves_ru_wid_reset() {
        with_ruf(b"", |args| {
            assert_eq!(unsafe { did_set_rulerformat(args) }, None);
            // ru_wid is reset to 0 at entry and never re-set.
            assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.ru_wid, 0);
        });
    }

    #[test]
    fn did_set_rulerformat_plain_text_is_valid() {
        with_ruf(b"plain text", |args| {
            assert_eq!(unsafe { did_set_rulerformat(args) }, None);
            assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.ru_wid, 0);
        });
    }

    #[test]
    fn did_set_rulerformat_rejects_invalid_statusline_syntax() {
        with_ruf(b"%z", |args| {
            assert_eq!(
                unsafe { did_set_rulerformat(args) },
                Some(crate::errors::e_invarg.as_bytes())
            );
        });
    }

    #[test]
    fn did_set_rulerformat_rejects_unbalanced_groups() {
        with_ruf(b"%(", |args| {
            assert_eq!(
                unsafe { did_set_rulerformat(args) },
                Some(crate::gettext_defs::gettext_noop("E542: Unbalanced groups").as_bytes())
            );
        });
    }

    #[test]
    fn did_set_rulerformat_skips_validation_for_a_percent_bang_function_ref() {
        // (*varp)[1] == '!' suppresses the check_stl_option call.
        with_ruf(b"%!MyFunc()", |args| {
            assert_eq!(unsafe { did_set_rulerformat(args) }, None);
        });
    }

    // ---- did_set_shellpipe_redir ----

    fn shellpipe_result(value: &[u8]) -> Option<&'static [u8]> {
        let mut args = crate::option_defs::OptsetT {
            os_newval: crate::option_defs::OptVal::String(value.to_vec()),
            ..Default::default()
        };
        did_set_shellpipe_redir(&mut args)
    }

    #[test]
    fn did_set_shellpipe_redir_accepts_a_single_placeholder() {
        assert_eq!(shellpipe_result(b"2>&1| tee %s"), None);
    }

    #[test]
    fn did_set_shellpipe_redir_accepts_a_value_with_no_percent_at_all() {
        assert_eq!(shellpipe_result(b">"), None);
        assert_eq!(shellpipe_result(b""), None);
    }

    #[test]
    fn did_set_shellpipe_redir_accepts_a_doubled_percent_escape() {
        assert_eq!(shellpipe_result(b"100%% done"), None);
    }

    #[test]
    fn did_set_shellpipe_redir_accepts_an_escape_next_to_a_placeholder() {
        // "%%" consumes both bytes, so the following "%s" is still the
        // FIRST real placeholder, not a second one.
        assert_eq!(shellpipe_result(b"%%%s"), None);
    }

    #[test]
    fn did_set_shellpipe_redir_rejects_two_placeholders() {
        assert_eq!(
            shellpipe_result(b"%s %s"),
            Some(crate::errors::e_invalid_format_string_single_percent_s.as_bytes())
        );
    }

    #[test]
    fn did_set_shellpipe_redir_rejects_a_trailing_bare_percent() {
        // The original reads p[1] and finds its own NUL terminator;
        // here that is an explicit bounds check.
        assert_eq!(
            shellpipe_result(b"tee %"),
            Some(crate::errors::e_invalid_format_string_single_percent_s.as_bytes())
        );
    }

    #[test]
    fn did_set_shellpipe_redir_rejects_an_unknown_percent_sequence() {
        assert_eq!(
            shellpipe_result(b"%d"),
            Some(crate::errors::e_invalid_format_string_single_percent_s.as_bytes())
        );
    }

    // ---- did_set_completeitemalign ----

    /// Saves/restores both `p_cia` and the derived `cia_flags`.
    fn with_cia<R>(value: Option<&[u8]>, f: impl FnOnce() -> R) -> R {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_cia = opts.p_cia.clone();
        let prev_flags = opts.cia_flags;
        opts.p_cia = value.map(<[u8]>::to_vec);
        opts.cia_flags = 0;

        let result = f();

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_cia = prev_cia;
        opts.cia_flags = prev_flags;
        result
    }

    #[test]
    fn did_set_completeitemalign_accepts_the_real_default_order() {
        with_cia(Some(b"abbr,kind,menu"), || {
            assert_eq!(did_set_completeitemalign(&mut Default::default()), None);
            // CPT_ABBR=0, CPT_KIND=1, CPT_MENU=2 packed base-10:
            // ((0*10+1)*10+2) == 12.
            assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cia_flags, 12);
        });
    }

    #[test]
    fn did_set_completeitemalign_accepts_a_reordered_list() {
        with_cia(Some(b"menu,kind,abbr"), || {
            assert_eq!(did_set_completeitemalign(&mut Default::default()), None);
            // ((2*10+1)*10+0) == 210.
            assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cia_flags, 210);
        });
    }

    #[test]
    fn did_set_completeitemalign_rejects_a_duplicate_item() {
        with_cia(Some(b"abbr,abbr,menu"), || {
            assert_eq!(
                did_set_completeitemalign(&mut Default::default()),
                Some(crate::errors::e_invarg.as_bytes())
            );
            // Rejected before any store, so the flags stay untouched.
            assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.cia_flags, 0);
        });
    }

    #[test]
    fn did_set_completeitemalign_rejects_too_few_items() {
        with_cia(Some(b"abbr,kind"), || {
            assert_eq!(
                did_set_completeitemalign(&mut Default::default()),
                Some(crate::errors::e_invarg.as_bytes())
            );
        });
    }

    #[test]
    fn did_set_completeitemalign_rejects_an_empty_value() {
        // Zero items means new_cia_flags == 0 AND count != 3.
        with_cia(Some(b""), || {
            assert_eq!(
                did_set_completeitemalign(&mut Default::default()),
                Some(crate::errors::e_invarg.as_bytes())
            );
        });
    }

    #[test]
    fn did_set_completeitemalign_rejects_an_unknown_item() {
        with_cia(Some(b"abbr,kind,bogus"), || {
            assert_eq!(
                did_set_completeitemalign(&mut Default::default()),
                Some(crate::errors::e_invarg.as_bytes())
            );
        });
    }

    #[test]
    fn did_set_completeitemalign_rejects_more_than_three_items() {
        with_cia(Some(b"abbr,kind,menu,abbr"), || {
            assert_eq!(
                did_set_completeitemalign(&mut Default::default()),
                Some(crate::errors::e_invarg.as_bytes())
            );
        });
    }

    // ---- did_set_fileformat ----

    fn ff_args(
        buf: &mut crate::buffer_defs::BufT,
        flags: u32,
        val: &mut Option<Vec<u8>>,
    ) -> crate::option_defs::OptsetT {
        crate::option_defs::OptsetT {
            os_idx: crate::option_defs::OptIndex::Fileformat,
            os_buf: buf as *mut crate::buffer_defs::BufT as *mut c_void,
            os_flags: flags as i32,
            os_varp: val as *mut Option<Vec<u8>> as *mut c_void,
            ..Default::default()
        }
    }

    #[test]
    fn did_set_fileformat_modifiable_buffer_accepts_a_valid_value() {
        // ml_setflags returns early on a null ml_mfp, so a default
        // BufT needs no real swap file here.
        let mut buf = crate::buffer_defs::BufT { b_p_ma: 1, ..Default::default() };
        let mut val: Option<Vec<u8>> = Some(b"unix".to_vec());
        let mut args = ff_args(&mut buf, 0, &mut val);
        assert_eq!(unsafe { did_set_fileformat(&mut args) }, None);
    }

    #[test]
    fn did_set_fileformat_non_modifiable_buffer_is_rejected() {
        let mut buf = crate::buffer_defs::BufT { b_p_ma: 0, ..Default::default() };
        let mut val: Option<Vec<u8>> = Some(b"unix".to_vec());
        let mut args = ff_args(&mut buf, 0, &mut val);
        assert_eq!(
            unsafe { did_set_fileformat(&mut args) },
            Some(crate::errors::e_modifiable.as_bytes())
        );
    }

    #[test]
    fn did_set_fileformat_non_modifiable_buffer_is_allowed_for_the_global_value() {
        // The !MODIFIABLE check is skipped entirely for OPT_GLOBAL.
        let mut buf = crate::buffer_defs::BufT { b_p_ma: 0, ..Default::default() };
        let mut val: Option<Vec<u8>> = Some(b"unix".to_vec());
        let mut args =
            ff_args(&mut buf, crate::option_defs::opt_set_flags::OPT_GLOBAL, &mut val);
        assert_eq!(unsafe { did_set_fileformat(&mut args) }, None);
    }

    #[test]
    fn did_set_fileformat_rejects_an_invalid_value() {
        // The modifiable check passes, so this is did_set_str_generic
        // rejecting the value itself.
        let mut buf = crate::buffer_defs::BufT { b_p_ma: 1, ..Default::default() };
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let mut args = ff_args(&mut buf, 0, &mut val);
        assert_eq!(
            unsafe { did_set_fileformat(&mut args) },
            Some(crate::errors::e_invarg.as_bytes())
        );
    }

    #[test]
    fn did_set_fileformat_checks_modifiable_before_validating_the_value() {
        // An invalid value on a non-modifiable buffer must report
        // E21, not E474 - the original checks MODIFIABLE first.
        let mut buf = crate::buffer_defs::BufT { b_p_ma: 0, ..Default::default() };
        let mut val: Option<Vec<u8>> = Some(b"bogus".to_vec());
        let mut args = ff_args(&mut buf, 0, &mut val);
        assert_eq!(
            unsafe { did_set_fileformat(&mut args) },
            Some(crate::errors::e_modifiable.as_bytes())
        );
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
        assert_eq!(unsafe { did_set_mousescroll(&mut Default::default()) }, None);
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        assert_eq!(opts.p_mousescroll_vert, 3);
        assert_eq!(opts.p_mousescroll_hor, 6);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_only_vertical_falls_back_to_the_horizontal_default() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"ver:5"));
        assert_eq!(unsafe { did_set_mousescroll(&mut Default::default()) }, None);
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        assert_eq!(opts.p_mousescroll_vert, 5);
        assert_eq!(opts.p_mousescroll_hor, crate::option_vars::MOUSESCROLL_HOR_DFLT);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_only_horizontal_falls_back_to_the_vertical_default() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"hor:10"));
        assert_eq!(unsafe { did_set_mousescroll(&mut Default::default()) }, None);
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        assert_eq!(opts.p_mousescroll_vert, crate::option_vars::MOUSESCROLL_VERT_DFLT);
        assert_eq!(opts.p_mousescroll_hor, 10);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_duplicate_direction_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"ver:1,ver:2"));
        assert_eq!(
            unsafe { did_set_mousescroll(&mut Default::default()) },
            Some(crate::errors::e_invarg.as_bytes())
        );
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_unknown_direction_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"foo:1"));
        assert_eq!(
            unsafe { did_set_mousescroll(&mut Default::default()) },
            Some(crate::errors::e_invarg.as_bytes())
        );
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_too_short_fails() {
        let _lock = crate::globals::global_state_test_lock();
        // length == 4 ("ver:"), no digit at all - length <= 4 fails
        // before the direction/digit checks even run.
        let prev = set_p_mousescroll(Some(b"ver:"));
        assert_eq!(
            unsafe { did_set_mousescroll(&mut Default::default()) },
            Some(crate::errors::e_invarg.as_bytes())
        );
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_non_digit_after_colon_reports_e5080() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"ver:x"));
        assert_eq!(
            unsafe { did_set_mousescroll(&mut Default::default()) },
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
        assert_eq!(
            unsafe { did_set_mousescroll(&mut Default::default()) },
            Some(crate::errors::e_invarg.as_bytes())
        );
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_mousescroll = prev;
    }

    #[test]
    fn did_set_mousescroll_allows_zero_to_disable_scrolling() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_mousescroll(Some(b"ver:0,hor:0"));
        assert_eq!(unsafe { did_set_mousescroll(&mut Default::default()) }, None);
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
        assert_eq!(unsafe { did_set_wildmode(&mut Default::default()) }, None);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wim = prev;
    }

    #[test]
    fn did_set_wildmode_invalid_value_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = set_p_wim(Some(b"bogus"));
        assert_eq!(
            unsafe { did_set_wildmode(&mut Default::default()) },
            Some(crate::errors::e_invarg.as_bytes())
        );
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
        // is false, so the fold-update branch is
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
    fn did_set_vartabstop_invalidates_the_folds_when_foldmethod_is_indent() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_fdm = Some(b"indent".to_vec());
        let mut val: Option<Vec<u8>> = Some(b"4,8".to_vec());
        let mut args = vartabstop_args(&mut buf, &mut win, &mut val);
        assert_eq!(unsafe { did_set_vartabstop(&mut args) }, None);
        assert_eq!(buf.b_p_vts_array, Some(vec![4, 8]), "the array still lands");
        assert!(win.w_foldinvalid, "foldUpdateAll ran");
    }
}
